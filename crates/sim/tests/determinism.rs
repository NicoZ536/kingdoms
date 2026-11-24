//! Determinism: identical seed + identical commands ⇒ bit-identical state.

use std::path::Path;

use glam::IVec2;
use kingdoms_sim::buildings::{BuildingStatus, Rotation, check_placement};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingTypeId, Defs, JobCategory};
use kingdoms_sim::economy::TradeOrder;
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::policy::JobPriority;
use kingdoms_sim::roads::plan_access_road;
use kingdoms_sim::save::{self, SaveHeader};
use kingdoms_sim::state::{GameSettings, GameState};
use kingdoms_sim::time::TICKS_PER_DAY;
use kingdoms_sim::world::WorldSize;

/// Long enough to cross midnight (tick 3600) and close a day in the ledger.
const TICKS: u64 = 6000;

fn load_defs() -> Defs {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"));
    match Defs::load(dir) {
        Ok(defs) => defs,
        Err(e) => panic!("shipped game data must load: {e}"),
    }
}

fn building(defs: &Defs, key: &str) -> BuildingTypeId {
    match defs.building_id(key) {
        Some(id) => id,
        None => panic!("missing building {key}"),
    }
}

/// First valid spot for `kind` in a deterministic scan around the start point.
fn find_spot(state: &GameState, defs: &Defs, kind: BuildingTypeId, min_ring: i32) -> Option<IVec2> {
    let start = state.world().start;
    (min_ring..30).find_map(|r| {
        (-r..=r)
            .flat_map(|dy| (-r..=r).map(move |dx| IVec2::new(dx, dy)))
            .map(|d| start + d)
            .find(|origin| check_placement(state, defs, kind, *origin, Rotation::default()).ok)
    })
}

/// The scripted commands for `tick`, derived deterministically from the state.
fn commands_for(state: &GameState, defs: &Defs, tick: u64) -> Vec<Command> {
    let house = building(defs, "house_wood");
    let cutter = building(defs, "woodcutter");
    let farm = building(defs, "chicken_farm");
    let path = defs.road_id("path");
    let last_of =
        |kind: BuildingTypeId| state.buildings().iter().rev().find(|(_, b)| b.kind() == kind).map(|(id, _)| id);
    let access = |kind: BuildingTypeId| -> Vec<Command> {
        last_of(kind)
            .and_then(|id| state.building(id))
            .zip(path)
            .and_then(|(b, path)| plan_access_road(state, defs, kind, b.origin(), b.rotation(), path))
            .and_then(|plan| plan.to_command())
            .into_iter()
            .collect()
    };
    match tick {
        5 => vec![Command::SetTaxRate { percent: 15 }],
        10 => find_spot(state, defs, house, 5)
            .map(|origin| Command::PlaceBuilding { kind: house, origin, rotation: Rotation::default() })
            .into_iter()
            .collect(),
        11 => access(house),
        60 => find_spot(state, defs, farm, 6)
            .map(|origin| Command::PlaceBuilding { kind: farm, origin, rotation: Rotation::default() })
            .into_iter()
            .collect(),
        61 => access(farm),
        40 => find_spot(state, defs, cutter, 8)
            .map(|origin| Command::PlaceBuilding { kind: cutter, origin, rotation: Rotation::new(1).unwrap_or_default() })
            .into_iter()
            .collect(),
        120 => vec![
            Command::SetWorkforcePriority { category: JobCategory::Food, priority: JobPriority::High },
            Command::SetTaxRate { percent: 99 }, // rejected
        ],
        200 => last_of(cutter).map(|building| Command::CancelConstruction { building }).into_iter().collect(),
        300 => last_of(farm)
            .map(|building| Command::SetProductionPriority { building, priority: JobPriority::High })
            .into_iter()
            .collect(),
        400 => last_of(farm)
            .map(|building| Command::QueueProduction { building, recipe: 0, units: Some(3) })
            .into_iter()
            .collect(),
        500 => defs
            .resource_id("wood")
            .map(|resource| Command::SetTradeOrder {
                order: TradeOrder { resource, buy_below: None, sell_above: Some(40), daily_limit: 5 },
            })
            .into_iter()
            .collect(),
        2500 => last_of(farm).map(|building| Command::ClearQueue { building }).into_iter().collect(),
        2600 => last_of(farm).map(|building| Command::SetRecipe { building, recipe: 0 }).into_iter().collect(),
        777 => vec![Command::SetWorkforcePriority { category: JobCategory::Military, priority: JobPriority::Low }],
        1500 => vec![Command::SetTaxRate { percent: 8 }],
        _ => Vec::new(),
    }
}

/// Runs a game and returns its serialized state plus all emitted events.
fn run(defs: &Defs, seed: u64) -> (Vec<u8>, Vec<SimEvent>) {
    let mut state = GameState::new(defs, GameSettings::new(defs, seed, WorldSize::Small));
    let mut events = Vec::new();
    for tick in 0..TICKS {
        let commands = commands_for(&state, defs, tick);
        events.extend(state.tick(defs, commands));
    }
    assert_eq!(state.clock().tick, TICKS);
    // A fixed timestamp keeps the header deterministic too.
    let header = SaveHeader::for_state(&state, defs, "determinism", 0);
    let bytes = match save::encode(&header, &state) {
        Ok(bytes) => bytes,
        Err(e) => panic!("encoding failed: {e}"),
    };
    (bytes, events)
}

#[test]
fn same_seed_and_commands_give_identical_bytes() {
    let defs = load_defs();
    let (a, events_a) = run(&defs, 4242);
    let (b, events_b) = run(&defs, 4242);
    assert_eq!(a, b, "serialized states differ");
    assert_eq!(events_a, events_b, "event streams differ");
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::CommandRejected { .. })));
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::TaxRateChanged { percent: 8 })));
    // The building script ran: placed, connected, finished, cancelled.
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::BuildingPlaced { .. })));
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::RoadsBuilt { .. })));
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::BuildingCompleted { .. })));
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::ConstructionCancelled { .. })));
    // The economy ran: production, transports, a closed day.
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::ProductionCompleted { .. })));
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::TransportCompleted { .. })));
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::DailyBalance { .. })));
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::ProductionQueueChanged { .. })));
    // The population ran: the homeless settlers moved into the new house.
    assert!(events_a.iter().any(|e| matches!(e, SimEvent::HouseholdMoved { home: Some(_), .. })));
}

#[test]
fn different_seed_gives_different_bytes() {
    let defs = load_defs();
    let (a, _) = run(&defs, 1);
    let (b, _) = run(&defs, 2);
    assert_ne!(a, b);
}

#[test]
fn saved_and_loaded_games_continue_identically_with_buildings() {
    let defs = load_defs();
    let mut state = GameState::new(&defs, GameSettings::new(&defs, 77, WorldSize::Small));
    for tick in 0..300 {
        let commands = commands_for(&state, &defs, tick);
        state.tick(&defs, commands);
    }
    assert!(state.buildings().iter().any(|(_, b)| b.status() == BuildingStatus::UnderConstruction));
    let header = SaveHeader::for_state(&state, &defs, "mid-construction", 0);
    let bytes = match save::encode(&header, &state) {
        Ok(bytes) => bytes,
        Err(e) => panic!("encoding failed: {e}"),
    };
    let mut loaded = match save::decode(&bytes, &defs) {
        Ok(save) => save.state,
        Err(e) => panic!("decoding failed: {e}"),
    };
    for tick in 300..4000 {
        let a = state.tick(&defs, commands_for(&state, &defs, tick));
        let b = loaded.tick(&defs, commands_for(&loaded, &defs, tick));
        assert_eq!(a, b, "events differ at tick {tick}");
    }
    let ha = SaveHeader::for_state(&state, &defs, "x", 0);
    let hb = SaveHeader::for_state(&loaded, &defs, "x", 0);
    assert_eq!(save::encode(&ha, &state).ok(), save::encode(&hb, &loaded).ok());
}

/// A small town for the population test: houses, a well, a market and food, built in the
/// first hours (each building with an access path; the chicken farms a bit further out, where
/// their entrance is not boxed in by the houses, so that carriers can reach them).
fn town_commands(state: &GameState, defs: &Defs, tick: u64) -> Vec<Command> {
    let plan: [(u64, &str, i32); 7] = [
        (2, "well", 3),
        (4, "house_wood", 5),
        (6, "chicken_farm", 9),
        (8, "house_wood", 6),
        (10, "market", 6),
        (12, "house_wood", 7),
        (14, "chicken_farm", 10),
    ];
    let Some(&(_, key, ring)) = plan.iter().find(|(t, ..)| *t == tick) else { return Vec::new() };
    let kind = building(defs, key);
    let Some(origin) = find_spot(state, defs, kind, ring) else { return Vec::new() };
    let mut commands = vec![Command::PlaceBuilding { kind, origin, rotation: Rotation::default() }];
    if let Some(road) = defs
        .road_id("path")
        .and_then(|path| plan_access_road(state, defs, kind, origin, Rotation::default(), path))
        .and_then(|plan| plan.to_command())
    {
        commands.push(road);
    }
    commands
}

#[test]
fn population_continues_identically_after_loading() {
    let defs = load_defs();
    let mut state = GameState::new(&defs, GameSettings::new(&defs, 5, WorldSize::Small));
    let split = TICKS_PER_DAY + TICKS_PER_DAY / 2;
    let mut events = Vec::new();
    for tick in 0..split {
        events.extend(state.tick(&defs, town_commands(&state, &defs, tick)));
    }
    let header = SaveHeader::for_state(&state, &defs, "population", 0);
    let bytes = match save::encode(&header, &state) {
        Ok(bytes) => bytes,
        Err(e) => panic!("encoding failed: {e}"),
    };
    let mut loaded = match save::decode(&bytes, &defs) {
        Ok(save) => save.state,
        Err(e) => panic!("decoding failed: {e}"),
    };
    for tick in split..4 * TICKS_PER_DAY {
        let a = state.tick(&defs, Vec::new());
        let b = loaded.tick(&defs, Vec::new());
        assert_eq!(a, b, "events differ at tick {tick}");
        events.extend(a);
    }
    let ha = SaveHeader::for_state(&state, &defs, "x", 0);
    let hb = SaveHeader::for_state(&loaded, &defs, "x", 0);
    assert_eq!(save::encode(&ha, &state).ok(), save::encode(&hb, &loaded).ok());
    // Citizens lived: households moved in, settlers arrived, the daily history was kept.
    assert!(events.iter().any(|e| matches!(e, SimEvent::HouseholdMoved { home: Some(_), .. })));
    assert!(events.iter().any(|e| matches!(e, SimEvent::HouseholdArrived { .. })));
    assert!(state.population().len() > 12);
    assert_eq!(state.population().stats().history().count(), 4);
}

#[test]
fn walking_citizens_continue_identically_after_loading() {
    let defs = load_defs();
    let mut state = GameState::new(&defs, GameSettings::new(&defs, 3, WorldSize::Small));
    // Build the town, then stop in the morning commute with somebody between two tiles.
    let mut tick = 0;
    let mid_walk = |s: &GameState| {
        s.population().citizens().filter(|(_, c)| c.motion().is_moving() && !c.motion().pos().is_center()).count()
    };
    while tick < TICKS_PER_DAY + TICKS_PER_DAY / 12 || mid_walk(&state) < 2 {
        state.tick(&defs, town_commands(&state, &defs, tick));
        tick += 1;
        assert!(tick < 3 * TICKS_PER_DAY, "nobody walked");
    }
    let header = SaveHeader::for_state(&state, &defs, "walking", 0);
    let bytes = match save::encode(&header, &state) {
        Ok(bytes) => bytes,
        Err(e) => panic!("encoding failed: {e}"),
    };
    let mut loaded = match save::decode(&bytes, &defs) {
        Ok(save) => save.state,
        Err(e) => panic!("decoding failed: {e}"),
    };
    let rect = kingdoms_sim::grid::TileRect::new(IVec2::ZERO, IVec2::splat(256));
    let views = |s: &GameState| kingdoms_sim::population::citizens_in(s, &defs, rect).collect::<Vec<_>>();
    assert_eq!(views(&state), views(&loaded), "the loaded walkers stand where they stood");
    for t in 0..TICKS_PER_DAY {
        let a = state.tick(&defs, Vec::new());
        let b = loaded.tick(&defs, Vec::new());
        assert_eq!(a, b, "events differ {t} ticks after loading");
        if t % 200 == 0 {
            assert_eq!(views(&state), views(&loaded), "walkers differ {t} ticks after loading");
        }
    }
    let ha = SaveHeader::for_state(&state, &defs, "x", 0);
    let hb = SaveHeader::for_state(&loaded, &defs, "x", 0);
    assert_eq!(save::encode(&ha, &state).ok(), save::encode(&hb, &loaded).ok());
    assert_eq!(state.navigation().stats().no_path, 0, "everybody found a path in the town");
}

#[test]
fn carriers_with_goods_continue_identically_after_loading() {
    let defs = load_defs();
    let mut state = GameState::new(&defs, GameSettings::new(&defs, 5, WorldSize::Small));
    // Build the town, then stop while a carrier walks with goods between two tiles.
    let mut tick = 0;
    let carrying = |s: &GameState| {
        s.logistics().jobs().iter().any(|j| {
            j.is_delivering()
                && j.carrier().and_then(|c| s.population().citizen(c)).is_some_and(|c| !c.motion().pos().is_center())
        })
    };
    while tick < TICKS_PER_DAY || !carrying(&state) {
        state.tick(&defs, town_commands(&state, &defs, tick));
        tick += 1;
        assert!(tick < 4 * TICKS_PER_DAY, "no carrier walked with goods");
    }
    let header = SaveHeader::for_state(&state, &defs, "carriers", 0);
    let bytes = match save::encode(&header, &state) {
        Ok(bytes) => bytes,
        Err(e) => panic!("encoding failed: {e}"),
    };
    let mut loaded = match save::decode(&bytes, &defs) {
        Ok(save) => save.state,
        Err(e) => panic!("decoding failed: {e}"),
    };
    let rect = kingdoms_sim::grid::TileRect::new(IVec2::ZERO, IVec2::splat(256));
    let views = |s: &GameState| kingdoms_sim::population::citizens_in(s, &defs, rect).collect::<Vec<_>>();
    assert!(views(&loaded).iter().any(|v| v.carried.is_some()), "a walker carries goods in the loaded game");
    assert_eq!(views(&state), views(&loaded));
    let mut delivered = 0;
    for t in 0..TICKS_PER_DAY {
        let a = state.tick(&defs, Vec::new());
        let b = loaded.tick(&defs, Vec::new());
        assert_eq!(a, b, "events differ {t} ticks after loading");
        delivered += a.iter().filter(|e| matches!(e, SimEvent::TransportCompleted { .. })).count();
    }
    assert!(delivered > 0, "the carriers delivered");
    assert_eq!(state.logistics(), loaded.logistics());
    let ha = SaveHeader::for_state(&state, &defs, "x", 0);
    let hb = SaveHeader::for_state(&loaded, &defs, "x", 0);
    assert_eq!(save::encode(&ha, &state).ok(), save::encode(&hb, &loaded).ok());
}

/// A farming village for the agriculture test: research unlocked by the developer command,
/// then a well and farms of every kind (pasture, vegetable fields, orchard), each with a path.
fn farm_commands(state: &GameState, defs: &Defs, tick: u64) -> Vec<Command> {
    if tick == 1 {
        return vec![Command::DebugUnlockAllResearch];
    }
    let plan: [(u64, &str, i32); 4] = [(3, "well", 3), (5, "orchard", 6), (7, "vegetable_farm", 7), (9, "sheep_farm", 8)];
    let Some(&(_, key, ring)) = plan.iter().find(|(t, ..)| *t == tick) else { return Vec::new() };
    let kind = building(defs, key);
    let Some(origin) = find_spot(state, defs, kind, ring) else { return Vec::new() };
    let mut commands = vec![Command::PlaceBuilding { kind, origin, rotation: Rotation::default() }];
    if let Some(road) = defs
        .road_id("path")
        .and_then(|path| plan_access_road(state, defs, kind, origin, Rotation::default(), path))
        .and_then(|plan| plan.to_command())
    {
        commands.push(road);
    }
    commands
}

#[test]
fn agriculture_and_weather_continue_identically_after_loading() {
    let defs = load_defs();
    let settings = GameSettings::new(&defs, 7, WorldSize::Small);
    // From spring into winter: sowing, growth, harvests, daily weather, frost and snow.
    let end = 19 * TICKS_PER_DAY;
    let split = 8 * TICKS_PER_DAY + 1234;
    let mut state = GameState::new(&defs, settings.clone());
    let mut events = Vec::new();
    // Weather draws at midnight (events may force weather at other hours as well).
    let mut midnight_draws = 0;
    let mut count_draws = |state: &GameState, tick_events: &[SimEvent]| {
        if state.clock().is_new_day() {
            midnight_draws += tick_events.iter().filter(|e| matches!(e, SimEvent::WeatherChanged { .. })).count();
        }
    };
    for tick in 0..split {
        let e = state.tick(&defs, farm_commands(&state, &defs, tick));
        count_draws(&state, &e);
        events.extend(e);
    }
    let header = SaveHeader::for_state(&state, &defs, "agriculture", 0);
    let bytes = match save::encode(&header, &state) {
        Ok(bytes) => bytes,
        Err(e) => panic!("encoding failed: {e}"),
    };
    let mut loaded = match save::decode(&bytes, &defs) {
        Ok(save) => save.state,
        Err(e) => panic!("decoding failed: {e}"),
    };
    for tick in split..end {
        let a = state.tick(&defs, Vec::new());
        let b = loaded.tick(&defs, Vec::new());
        assert_eq!(a, b, "events differ at tick {tick}");
        count_draws(&state, &a);
        events.extend(a);
    }
    let encode = |s: &GameState| save::encode(&SaveHeader::for_state(s, &defs, "x", 0), s).ok();
    assert_eq!(encode(&state), encode(&loaded));
    // The saved path memory, the wind and the herds continued identically (they are in the bytes).
    assert!(loaded.navigation().remembered_paths() > 0);
    assert_eq!(loaded.navigation().remembered_paths(), state.navigation().remembered_paths());
    assert_eq!(
        (loaded.weather().wind_direction(), loaded.weather().wind_strength()),
        (state.weather().wind_direction(), state.weather().wind_strength())
    );
    assert!(state.agriculture().farms().any(|(_, f)| f.herd() > 0), "the sheep farm keeps a herd");
    // The output records (cycles and goods per day, harvests of the field farms) continued
    // identically: they are in the bytes and were filled before and after the save.
    let made = |s: &GameState| -> Vec<(u32, u32)> {
        s.buildings()
            .iter()
            .filter_map(|(_, b)| b.production())
            .flat_map(|p| p.output_log().days().map(|d| (d.cycles, d.units())).collect::<Vec<_>>())
            .collect()
    };
    assert!(made(&state).iter().any(|(cycles, _)| *cycles > 0), "buildings recorded their output");
    assert_eq!(made(&loaded), made(&state));
    // The same seed and commands from scratch give the same bytes.
    let mut again = GameState::new(&defs, settings);
    for tick in 0..end {
        again.tick(&defs, farm_commands(&again, &defs, tick));
    }
    assert_eq!(encode(&state), encode(&again));
    // The village farmed: fields and pastures claimed, harvests, weather every day, winter.
    let kinds: Vec<_> = state.agriculture().farms().map(|(_, f)| f.kind()).collect();
    assert!(kinds.contains(&kingdoms_sim::agriculture::FarmKind::Pasture) && kinds.len() == 3, "farms {kinds:?}");
    assert!(events.iter().any(|e| matches!(e, SimEvent::HarvestCompleted { .. })));
    assert_eq!(midnight_draws, 19, "one draw per midnight");
    assert_eq!(state.clock().season(), kingdoms_sim::time::Season::Winter);
    state.validate(&defs).expect("consistent state");
}

/// Research and events for the save test: a research queue at the start, a festival (a
/// happiness modifier) on day 1 and a merchant who is still waiting for a decision when the
/// game is saved.
fn progression_commands(defs: &Defs, tick: u64, split: u64) -> Vec<Command> {
    let tech = |key: &str| defs.tech_id(key).map(|tech| Command::QueueResearch { tech });
    let event = |key: &str| defs.event_id(key).map(|event| Command::DebugTriggerEvent { event });
    match tick {
        2 => ["grain_farming", "improved_plows", "bookkeeping"].into_iter().filter_map(tech).collect(),
        t if t == TICKS_PER_DAY => event("religious_festival").into_iter().collect(),
        t if t == TICKS_PER_DAY + 10 => vec![Command::ResolveEvent { serial: 1, choice: 0 }],
        t if t == split - 50 => event("merchant_visit").into_iter().collect(),
        _ => Vec::new(),
    }
}

#[test]
fn research_and_events_continue_identically_after_loading() {
    let defs = load_defs();
    let settings = GameSettings::new(&defs, 11, WorldSize::Small);
    assert!(settings.events_enabled, "events are on by default");
    let split = TICKS_PER_DAY + 2000;
    let end = 4 * TICKS_PER_DAY;
    let mut state = GameState::new(&defs, settings.clone());
    let mut events = Vec::new();
    for tick in 0..split {
        events.extend(state.tick(&defs, progression_commands(&defs, tick, split)));
    }
    // Saved mid-research, with a modifier running and an event waiting for a decision.
    let grain = defs.tech_id("grain_farming").expect("tech");
    assert!(state.research().is_researched(grain) || state.research().progress_milli(grain) > 0);
    assert!(state.events().pending().is_some(), "the merchant waits for a decision");
    assert!(state.events().happiness_modifier(state.clock().tick) > 0, "the festival modifier runs");
    let header = SaveHeader::for_state(&state, &defs, "progression", 0);
    let bytes = match save::encode(&header, &state) {
        Ok(bytes) => bytes,
        Err(e) => panic!("encoding failed: {e}"),
    };
    let mut loaded = match save::decode(&bytes, &defs) {
        Ok(save) => save.state,
        Err(e) => panic!("decoding failed: {e}"),
    };
    assert_eq!(loaded.events().pending(), state.events().pending());
    assert_eq!(loaded.research().queue(), state.research().queue());
    for tick in split..end {
        let a = state.tick(&defs, progression_commands(&defs, tick, split));
        let b = loaded.tick(&defs, progression_commands(&defs, tick, split));
        assert_eq!(a, b, "events differ at tick {tick}");
        events.extend(a);
    }
    let encode = |s: &GameState| save::encode(&SaveHeader::for_state(s, &defs, "x", 0), s).ok();
    assert_eq!(encode(&state), encode(&loaded));
    let mut again = GameState::new(&defs, settings);
    for tick in 0..end {
        again.tick(&defs, progression_commands(&defs, tick, split));
    }
    assert_eq!(encode(&state), encode(&again));
    // Research finished technologies; the merchant left without a decision.
    assert!(events.iter().any(|e| matches!(e, SimEvent::ResearchCompleted { tech } if *tech == grain)));
    assert!(events.iter().any(|e| matches!(e, SimEvent::EventResolved { serial: 1, by_default: false, .. })));
    assert!(events.iter().any(|e| matches!(e, SimEvent::EventResolved { serial: 2, by_default: true, .. })));
    state.validate(&defs).expect("consistent state");
}

/// Diplomacy for the save test: a trading post (research and materials granted) with standing orders, a
/// friendly partner, a gift, a treaty proposal, an own caravan once the post works, and the
/// developer commands.
fn diplomacy_commands(state: &GameState, defs: &Defs, tick: u64) -> Vec<Command> {
    use kingdoms_sim::defs::ResourceAmount;
    use kingdoms_sim::diplomacy::{DiplomaticAction, KingdomId, TreatyKind};
    let post = building(defs, "trading_post");
    let res = |key: &str| defs.resource_id(key).unwrap_or_else(|| panic!("missing resource {key}"));
    let first = KingdomId::from_index(0).unwrap_or_else(|| panic!("kingdom id"));
    let second = KingdomId::from_index(1).unwrap_or_else(|| panic!("kingdom id"));
    match tick {
        1 => vec![
            Command::DebugUnlockAllResearch,
            Command::DebugGrant { gold: 2000, goods: vec![ResourceAmount { resource: res("planks"), amount: 60 }, ResourceAmount { resource: res("stone"), amount: 60 }] },
            Command::DebugSetRelation { kingdom: first, value: 40 },
        ],
        2 => find_spot(state, defs, post, 6)
            .zip(defs.road_id("path"))
            .map(|(origin, road)| Command::PlaceBuildingWithAccess { kind: post, origin, rotation: Rotation::default(), road })
            .into_iter()
            .collect(),
        3 => vec![
            Command::SetTradeOrder { order: TradeOrder { resource: res("stone"), buy_below: None, sell_above: Some(30), daily_limit: 40 } },
            Command::SetTradeOrder { order: TradeOrder { resource: res("bread"), buy_below: Some(200), sell_above: None, daily_limit: 40 } },
            Command::Diplomacy { kingdom: second, action: DiplomaticAction::Gift { gold: 50, goods: Vec::new() } },
            Command::Diplomacy { kingdom: first, action: DiplomaticAction::ProposeTreaty { treaty: TreatyKind::TradeAgreement } },
            Command::DebugSpawnHouseholds { count: 2 },
        ],
        t if t % 400 == 7 && kingdoms_sim::economy::trade::active_trading_post(state, defs).is_some()
            && state.diplomacy().caravans().iter().all(|c| c.kind != kingdoms_sim::diplomacy::CaravanKind::Own) =>
        {
            vec![Command::SendCaravan { kingdom: first, goods: vec![ResourceAmount { resource: res("wood"), amount: 10 }], gold: 50, buy: Vec::new() }]
        }
        _ => Vec::new(),
    }
}

#[test]
fn diplomacy_and_caravans_continue_identically_after_loading() {
    let defs = load_defs();
    let settings = GameSettings::new(&defs, 7, WorldSize::Small);
    let mut state = GameState::new(&defs, settings.clone());
    assert_eq!(state.diplomacy().len(), 3);
    let mut events = Vec::new();
    // Run until a caravan walks on the map (at most four days), then save.
    let mut split = None;
    for tick in 0..4 * TICKS_PER_DAY {
        events.extend(state.tick(&defs, diplomacy_commands(&state, &defs, tick)));
        if state.diplomacy().caravans().iter().any(|c| !c.walkers.is_empty()) {
            split = Some(tick + 1);
            break;
        }
    }
    let split = split.unwrap_or_else(|| panic!("no caravan walked within four days"));
    let header = SaveHeader::for_state(&state, &defs, "diplomacy", 0);
    let bytes = save::encode(&header, &state).unwrap_or_else(|e| panic!("encoding failed: {e}"));
    let mut loaded = save::decode(&bytes, &defs).unwrap_or_else(|e| panic!("decoding failed: {e}")).state;
    assert_eq!(loaded.diplomacy(), state.diplomacy());
    let end = split + 2 * TICKS_PER_DAY;
    for tick in split..end {
        let a = state.tick(&defs, diplomacy_commands(&state, &defs, tick));
        let b = loaded.tick(&defs, diplomacy_commands(&loaded, &defs, tick));
        assert_eq!(a, b, "events differ at tick {tick}");
        events.extend(a);
    }
    let encode = |s: &GameState| save::encode(&SaveHeader::for_state(s, &defs, "x", 0), s).ok();
    assert_eq!(encode(&state), encode(&loaded));
    let mut again = GameState::new(&defs, settings);
    for tick in 0..end {
        again.tick(&defs, diplomacy_commands(&again, &defs, tick));
    }
    assert_eq!(encode(&state), encode(&again), "same seed and commands, same game");
    assert!(events.iter().any(|e| matches!(e, SimEvent::GiftSent { .. })));
    assert!(events.iter().any(|e| matches!(e, SimEvent::CaravanDeparted { .. })));
    assert!(events.iter().any(|e| matches!(e, SimEvent::HouseholdArrived { .. })));
    state.validate(&defs).unwrap_or_else(|e| panic!("consistent state: {e}"));
}

/// A barracks and a watchtower, four recruits, then bandits and an army of a kingdom at war.
fn military_commands(state: &GameState, defs: &Defs, tick: u64) -> Vec<Command> {
    use kingdoms_sim::defs::ResourceAmount;
    use kingdoms_sim::diplomacy::{DiplomaticAction, KingdomId};
    let res = |key: &str| defs.resource_id(key).unwrap_or_else(|| panic!("missing resource {key}"));
    let barracks = building(defs, "barracks");
    let tower = building(defs, "watchtower");
    let first = KingdomId::from_index(0).unwrap_or_else(|| panic!("kingdom id"));
    let place = |kind: BuildingTypeId, ring: i32| -> Vec<Command> {
        find_spot(state, defs, kind, ring)
            .zip(defs.road_id("path"))
            .map(|(origin, road)| Command::PlaceBuildingWithAccess { kind, origin, rotation: Rotation::default(), road })
            .into_iter()
            .collect()
    };
    let finished = |kind: BuildingTypeId| state.buildings().iter().find(|(_, b)| b.kind() == kind && b.is_complete()).map(|(id, _)| id);
    match tick {
        1 => vec![
            Command::DebugUnlockAllResearch,
            Command::DebugGrant {
                gold: 3000,
                goods: vec![
                    ResourceAmount { resource: res("planks"), amount: 80 },
                    ResourceAmount { resource: res("stone"), amount: 80 },
                    ResourceAmount { resource: res("tools"), amount: 20 },
                ],
            },
        ],
        2 => place(barracks, 6),
        3 => place(tower, 4),
        t if t % 200 == 50 && state.military().soldiers().count() == 0 => {
            finished(barracks).map(|b| Command::Recruit { building: b, unit: defs.unit_id("militia").unwrap_or_else(|| panic!("militia")), count: 4 }).into_iter().collect()
        }
        t if t % 200 == 60 && state.military().soldiers().count() > 0 && state.military().companies().is_empty() => {
            vec![Command::CreateCompany { soldiers: state.military().soldiers().map(|(id, _)| id).collect() }]
        }
        t if t % 200 == 70
            && state.military().soldiers().any(|(_, s)| s.is_ready())
            && state.military().battles().count() == 0
            && state.military().warbands().is_empty() =>
        {
            vec![
                Command::DebugSpawnThreat { kingdom: None, size: 5 },
                Command::Diplomacy { kingdom: first, action: DiplomaticAction::DeclareWar },
                Command::DebugSpawnThreat { kingdom: Some(first), size: 8 },
            ]
        }
        _ => Vec::new(),
    }
}

#[test]
fn battles_continue_identically_after_loading() {
    let defs = load_defs();
    let settings = GameSettings::new(&defs, 7, WorldSize::Small);
    let mut state = GameState::new(&defs, settings.clone());
    let mut events = Vec::new();
    // Run until enemies have fought on the map for a while (at most four days), then save.
    let mut split = None;
    let mut fighting = 0;
    for tick in 0..4 * TICKS_PER_DAY {
        events.extend(state.tick(&defs, military_commands(&state, &defs, tick)));
        if state.military().enemies_on_map() {
            fighting += 1;
            if fighting == 300 {
                split = Some(tick + 1);
                break;
            }
        }
    }
    let split = split.unwrap_or_else(|| panic!("no enemies on the map within four days"));
    assert!(state.military().soldiers().count() > 0, "soldiers were recruited");
    let header = SaveHeader::for_state(&state, &defs, "battle", 0);
    let bytes = save::encode(&header, &state).unwrap_or_else(|e| panic!("encoding failed: {e}"));
    let mut loaded = save::decode(&bytes, &defs).unwrap_or_else(|e| panic!("decoding failed: {e}")).state;
    assert_eq!(loaded.military(), state.military());
    let end = split + TICKS_PER_DAY;
    for tick in split..end {
        let a = state.tick(&defs, military_commands(&state, &defs, tick));
        let b = loaded.tick(&defs, military_commands(&loaded, &defs, tick));
        assert_eq!(a, b, "events differ at tick {tick}");
        events.extend(a);
    }
    let encode = |s: &GameState| save::encode(&SaveHeader::for_state(s, &defs, "x", 0), s).ok();
    assert_eq!(encode(&state), encode(&loaded));
    let mut again = GameState::new(&defs, settings);
    for tick in 0..end {
        again.tick(&defs, military_commands(&again, &defs, tick));
    }
    assert_eq!(encode(&state), encode(&again), "same seed and commands, same game");
    assert!(events.iter().any(|e| matches!(e, SimEvent::SoldiersRecruited { .. })));
    assert!(events.iter().any(|e| matches!(e, SimEvent::ThreatArrived { .. })));
    assert!(events.iter().any(|e| matches!(e, SimEvent::BattleEnded { .. } | SimEvent::EnemyFell { .. })));
    state.validate(&defs).unwrap_or_else(|e| panic!("consistent state: {e}"));
}

/// Town hall → castle with every material stored in the town hall itself (the usual case: the
/// first storage holds the stone, planks and iron), saved right after the upgrade started.
fn castle_commands(state: &GameState, defs: &Defs, tick: u64) -> Vec<Command> {
    use kingdoms_sim::defs::ResourceAmount;
    let res = |key: &str| defs.resource_id(key).unwrap_or_else(|| panic!("missing resource {key}"));
    let hall = building(defs, "town_hall");
    match tick {
        1 => vec![
            Command::DebugUnlockAllResearch,
            Command::DebugGrant {
                gold: 2000,
                goods: vec![
                    ResourceAmount { resource: res("stone"), amount: 130 },
                    ResourceAmount { resource: res("planks"), amount: 40 },
                    ResourceAmount { resource: res("iron_bars"), amount: 25 },
                ],
            },
        ],
        2 => state
            .buildings()
            .iter()
            .find(|(_, b)| b.kind() == hall)
            .map(|(building, _)| Command::UpgradeBuilding { building })
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn castle_upgrade_supplied_from_the_town_hall_and_identical_after_loading() {
    let defs = load_defs();
    let hall = building(&defs, "town_hall");
    // The first seed whose town hall has room for the 5×5 castle.
    let mut state = (1..40)
        .map(|seed| GameState::new(&defs, GameSettings::new(&defs, seed, WorldSize::Small)))
        .find(|s| {
            s.buildings().iter().find(|(_, b)| b.kind() == hall).is_some_and(|(id, _)| {
                kingdoms_sim::buildings::upgrade_blockers(s, &defs, id)
                    .is_some_and(|list| !list.contains(&kingdoms_sim::buildings::UpgradeBlocker::NoSpace))
            })
        })
        .unwrap_or_else(|| panic!("no seed with room for the castle"));
    let storages = state.buildings().iter().filter(|(_, b)| defs.building(b.kind()).storage.is_some()).count();
    assert_eq!(storages, 1, "the town hall is the only storage");
    let owned_before = state.territory().owned_tiles();
    let mut events = Vec::new();
    for tick in 0..3 {
        events.extend(state.tick(&defs, castle_commands(&state, &defs, tick)));
    }
    assert!(events.iter().any(|e| matches!(e, SimEvent::UpgradeStarted { .. })), "{events:?}");
    assert_ne!(state.territory().owned_tiles(), owned_before, "the grown footprint moves the claim at once");
    // Saving right after the start gives the same state: the territory follows the footprint.
    let encode = |s: &GameState| save::encode(&SaveHeader::for_state(s, &defs, "x", 0), s).ok();
    let bytes = encode(&state).unwrap_or_else(|| panic!("encoding failed"));
    let mut loaded = save::decode(&bytes, &defs).unwrap_or_else(|e| panic!("decoding failed: {e}")).state;
    assert_eq!(loaded.territory(), state.territory());
    assert_eq!(encode(&loaded), encode(&state), "loading changes nothing");
    let mut upgraded = false;
    for tick in 3..4 * TICKS_PER_DAY {
        let a = state.tick(&defs, castle_commands(&state, &defs, tick));
        let b = loaded.tick(&defs, castle_commands(&loaded, &defs, tick));
        assert_eq!(a, b, "events differ at tick {tick}");
        upgraded |= a.iter().any(|e| matches!(e, SimEvent::BuildingUpgraded { .. }));
        if upgraded {
            break;
        }
    }
    assert!(upgraded, "the castle is built from the town hall's own stock");
    assert_eq!(encode(&state), encode(&loaded));
    state.validate(&defs).unwrap_or_else(|e| panic!("consistent state: {e}"));
}

/// Festival, raid and inbox pacing for the save test: a market (materials granted), a festival
/// as soon as it stands, a bandit raid announced by the scouts (decided at once, so the band
/// keeps the time of the report) while the AI kingdoms send paced proposals.
fn festival_commands(state: &GameState, defs: &Defs, tick: u64) -> Vec<Command> {
    use kingdoms_sim::defs::ResourceAmount;
    let market = building(defs, "market");
    let res = |key: &str| defs.resource_id(key).unwrap_or_else(|| panic!("missing resource {key}"));
    let raid = defs.event_id("bandit_raid").unwrap_or_else(|| panic!("bandit_raid"));
    match tick {
        1 => vec![Command::DebugGrant {
            gold: 3000,
            goods: vec![ResourceAmount { resource: res("wood"), amount: 60 }, ResourceAmount { resource: res("stone"), amount: 40 }],
        }],
        2 => find_spot(state, defs, market, 6)
            .zip(defs.road_id("path"))
            .map(|(origin, road)| Command::PlaceBuildingWithAccess { kind: market, origin, rotation: Rotation::default(), road })
            .into_iter()
            .collect(),
        t if t % 100 == 5
            && state.events().last_festival_day().is_none()
            && state.buildings().iter().any(|(_, b)| b.kind() == market && b.is_operational()) =>
        {
            vec![Command::HoldFestival]
        }
        t if t == 3 * TICKS_PER_DAY + 500 && state.events().pending().is_none() => vec![Command::DebugTriggerEvent { event: raid }],
        t if t > 3 * TICKS_PER_DAY + 500 && t % 50 == 0 => state
            .events()
            .pending()
            .filter(|p| p.event == raid)
            .map(|p| Command::ResolveEvent { serial: p.serial, choice: 0 })
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn festivals_raids_and_the_paced_inbox_continue_identically_after_loading() {
    let defs = load_defs();
    let settings = GameSettings::new(&defs, 7, WorldSize::Small);
    let mut state = GameState::new(&defs, settings.clone());
    let mut events = Vec::new();
    // Run until the scouts' band is on its way (at most five days), then save.
    let mut split = None;
    for tick in 0..5 * TICKS_PER_DAY {
        events.extend(state.tick(&defs, festival_commands(&state, &defs, tick)));
        if !state.military().warbands().is_empty() {
            split = Some(tick + 1);
            break;
        }
    }
    let split = split.unwrap_or_else(|| panic!("no band on its way within five days"));
    assert!(state.events().last_festival_day().is_some(), "the festival was held");
    let header = SaveHeader::for_state(&state, &defs, "festival", 0);
    let bytes = save::encode(&header, &state).unwrap_or_else(|e| panic!("encoding failed: {e}"));
    let mut loaded = save::decode(&bytes, &defs).unwrap_or_else(|e| panic!("decoding failed: {e}")).state;
    assert_eq!(loaded.events(), state.events());
    assert_eq!(loaded.diplomacy(), state.diplomacy());
    assert_eq!(loaded.military(), state.military());
    let end = split + 2 * TICKS_PER_DAY;
    for tick in split..end {
        let a = state.tick(&defs, festival_commands(&state, &defs, tick));
        let b = loaded.tick(&defs, festival_commands(&loaded, &defs, tick));
        assert_eq!(a, b, "events differ at tick {tick}");
        events.extend(a);
    }
    let encode = |s: &GameState| save::encode(&SaveHeader::for_state(s, &defs, "x", 0), s).ok();
    assert_eq!(encode(&state), encode(&loaded));
    let mut again = GameState::new(&defs, settings);
    for tick in 0..end {
        again.tick(&defs, festival_commands(&again, &defs, tick));
    }
    assert_eq!(encode(&state), encode(&again), "same seed and commands, same game");
    assert!(events.iter().any(|e| matches!(e, SimEvent::FestivalHeld { .. })));
    assert!(events.iter().any(|e| matches!(e, SimEvent::ThreatApproaching { kingdom: None, .. })));
    state.validate(&defs).unwrap_or_else(|e| panic!("consistent state: {e}"));
}

/// Snow and a crowd of newcomers: hunger, cold and homelessness begin as spells of waiting
/// states (docs/game_design.md §2 Meldungen).
fn spell_commands(state: &GameState, defs: &Defs, tick: u64) -> Vec<Command> {
    match tick {
        1 => vec![
            Command::DebugSetWeather { weather: kingdoms_sim::defs::Weather::Snow, intensity: 100, days: 4 },
            Command::DebugSpawnHouseholds { count: 12 },
        ],
        _ => town_commands(state, defs, tick),
    }
}

#[test]
fn waiting_state_spells_continue_identically_after_loading() {
    use kingdoms_sim::event::NotificationKind;
    let defs = load_defs();
    let mut state = GameState::new(&defs, GameSettings::new(&defs, 5, WorldSize::Small));
    let split = TICKS_PER_DAY + TICKS_PER_DAY / 3;
    let mut events = Vec::new();
    for tick in 0..split {
        events.extend(state.tick(&defs, spell_commands(&state, &defs, tick)));
    }
    let header = SaveHeader::for_state(&state, &defs, "spells", 0);
    let bytes = save::encode(&header, &state).unwrap_or_else(|e| panic!("encoding failed: {e}"));
    let mut loaded = save::decode(&bytes, &defs).unwrap_or_else(|e| panic!("decoding failed: {e}")).state;
    for tick in split..3 * TICKS_PER_DAY {
        let a = state.tick(&defs, spell_commands(&state, &defs, tick));
        let b = loaded.tick(&defs, spell_commands(&loaded, &defs, tick));
        assert_eq!(a, b, "events differ at tick {tick}");
        events.extend(a);
    }
    let encode = |s: &GameState| save::encode(&SaveHeader::for_state(s, &defs, "x", 0), s).ok();
    assert_eq!(encode(&state), encode(&loaded));
    // Waiting states are told once per spell, not every few hours.
    let waiting: Vec<(NotificationKind, String)> = events
        .iter()
        .filter_map(SimEvent::notification)
        .filter(|n| n.kind.is_waiting_state())
        .map(|n| (n.kind, n.text.clone()))
        .collect();
    eprintln!("waiting-state notices: {waiting:?}");
    assert!(!waiting.is_empty(), "snow and newcomers began a waiting state");
    for kind in NotificationKind::ALL.iter().filter(|k| matches!(k, NotificationKind::Homeless | NotificationKind::NoFuel | NotificationKind::Famine)) {
        assert!(waiting.iter().filter(|(k, _)| k == kind).count() <= 1, "{kind:?} told more than once: {waiting:?}");
    }
    state.validate(&defs).unwrap_or_else(|e| panic!("consistent state: {e}"));
}
