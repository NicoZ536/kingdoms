//! Robustness (master spec §21, docs/architecture.md §3.6): command fuzzing and crafted saves.
//!
//! * **Commands:** on several seeds, random commands of every kind — valid ones (existing
//!   buildings, citizens, companies, kingdoms, pending events and proposals, spots near the
//!   start) and invalid ones (stale or made-up ids, positions off the map, unknown data ids,
//!   absurd amounts) — are fed into running games with AI kingdoms, events and threats. The
//!   simulation must never panic, the state must validate every game hour, and saving and
//!   loading in the middle must continue exactly (same events, same bytes).
//! * **Saves:** a real save is truncated, damaged (with and without a repaired checksum) and
//!   replaced by garbage; loading must fail with an error (the game shows it in German, see
//!   `src/save/mod.rs`) or produce a state that validates and keeps running — never panic.

use std::path::Path;

use glam::IVec2;
use kingdoms_sim::buildings::{BuildingId, Rotation};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{Defs, JobCategory, ResourceAmount, TechId, Weather};
use kingdoms_sim::diplomacy::{DiplomaticAction, KingdomId, PeaceTerms, TreatyKind};
use kingdoms_sim::economy::TradeOrder;
use kingdoms_sim::military::{DefendTarget, Order};
use kingdoms_sim::policy::JobPriority;
use kingdoms_sim::population::CitizenId;
use kingdoms_sim::save::{SaveError, SaveHeader, decode, encode};
use kingdoms_sim::state::{GameSettings, GameState};
use kingdoms_sim::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
use kingdoms_sim::world::WorldSize;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

fn load_defs() -> Defs {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"));
    match Defs::load(dir) {
        Ok(defs) => defs,
        Err(e) => panic!("shipped game data must load: {e}"),
    }
}

/// A data id outside the data (deserialized like a damaged command from a replay; every
/// list of the shipped data is shorter than 200).
fn bogus<T: serde::de::DeserializeOwned>(rng: &mut ChaCha8Rng) -> T {
    let n: u8 = rng.random_range(200..=255);
    match ron::from_str(&n.to_string()) {
        Ok(v) => v,
        Err(e) => panic!("ids are plain numbers: {e}"),
    }
}

fn pick<T: Copy>(rng: &mut ChaCha8Rng, list: &[T]) -> Option<T> {
    (!list.is_empty()).then(|| list[rng.random_range(0..list.len())])
}

fn position(rng: &mut ChaCha8Rng, state: &GameState) -> IVec2 {
    if rng.random_range(0..10) < 8 {
        state.world().start + IVec2::new(rng.random_range(-16..=16), rng.random_range(-16..=16))
    } else {
        IVec2::new(rng.random_range(-20..200), rng.random_range(-20..200))
    }
}

fn building(rng: &mut ChaCha8Rng, state: &GameState) -> BuildingId {
    let ids: Vec<BuildingId> = state.buildings().iter().map(|(id, _)| id).collect();
    match pick(rng, &ids) {
        Some(id) if rng.random_range(0..10) < 8 => id,
        _ => BuildingId::from_raw_parts(rng.random_range(0..400), rng.random_range(0..3)),
    }
}

fn citizens(rng: &mut ChaCha8Rng, state: &GameState) -> Vec<CitizenId> {
    let soldiers: Vec<CitizenId> = state.military().soldiers().map(|(id, _)| id).collect();
    let all: Vec<CitizenId> = state.population().citizens().map(|(id, _)| id).collect();
    let n = rng.random_range(0..6);
    (0..n)
        .filter_map(|_| match rng.random_range(0..10) {
            0..6 => pick(rng, &soldiers),
            6..9 => pick(rng, &all),
            _ => Some(CitizenId::from_raw_parts(rng.random_range(0..500), rng.random_range(0..4))),
        })
        .collect()
}

fn company(rng: &mut ChaCha8Rng, state: &GameState) -> u32 {
    let ids: Vec<u32> = state.military().companies().iter().map(|c| c.id()).collect();
    match pick(rng, &ids) {
        Some(id) if rng.random_range(0..4) > 0 => id,
        _ => rng.random_range(0..20),
    }
}

fn kingdom(rng: &mut ChaCha8Rng) -> KingdomId {
    KingdomId::from_index(rng.random_range(0..7)).unwrap_or_else(|| panic!("indices below 7 are kingdom ids"))
}

fn goods(rng: &mut ChaCha8Rng, defs: &Defs) -> Vec<ResourceAmount> {
    let n = rng.random_range(0..4);
    (0..n)
        .map(|_| {
            let resource = if rng.random_range(0..10) == 0 { bogus(rng) } else { defs.resources()[rng.random_range(0..defs.resources().len())].id };
            ResourceAmount { resource, amount: rng.random_range(0..500) }
        })
        .collect()
}

fn tech(rng: &mut ChaCha8Rng, defs: &Defs) -> TechId {
    if rng.random_range(0..10) == 0 { bogus(rng) } else { defs.techs()[rng.random_range(0..defs.techs().len())].id }
}

fn order(rng: &mut ChaCha8Rng, state: &GameState) -> Order {
    match rng.random_range(0..6) {
        0 => Order::Station,
        1 => Order::Defend(DefendTarget::Building(building(rng, state))),
        2 => Order::Defend(DefendTarget::Area { center: position(rng, state), radius: rng.random_range(0..20) }),
        3 => Order::Patrol { route: (0..rng.random_range(0..8)).map(|_| position(rng, state)).collect() },
        4 => Order::Campaign { kingdom: kingdom(rng), siege: rng.random_range(0..2) == 0 },
        _ => Order::Retreat,
    }
}

fn diplomatic_action(rng: &mut ChaCha8Rng, defs: &Defs) -> DiplomaticAction {
    let treaty = TreatyKind::ALL[rng.random_range(0..TreatyKind::ALL.len())];
    match rng.random_range(0..6) {
        0 => DiplomaticAction::ProposeTreaty { treaty },
        1 => DiplomaticAction::CancelTreaty { treaty },
        2 => DiplomaticAction::Gift { gold: rng.random_range(0..2_000), goods: goods(rng, defs) },
        3 => DiplomaticAction::DemandTribute { gold: rng.random_range(0..5_000) },
        4 => DiplomaticAction::DeclareWar,
        _ => {
            let terms = match rng.random_range(0..4) {
                0 => PeaceTerms::White,
                1 => PeaceTerms::PlayerPays { gold: rng.random_range(0..3_000) },
                2 => PeaceTerms::KingdomPays { gold: rng.random_range(0..3_000) },
                _ => PeaceTerms::BorderCeded,
            };
            DiplomaticAction::ProposePeace { terms }
        }
    }
}

/// A building for a destructive command: rarely the main headquarters (the fuzzed kingdom
/// should keep its territory most of the time).
fn victim(rng: &mut ChaCha8Rng, state: &GameState, defs: &Defs) -> BuildingId {
    let b = building(rng, state);
    let hq = kingdoms_sim::economy::storage::main_headquarters(state, defs);
    if Some(b) == hq && rng.random_range(0..20) > 0 { BuildingId::from_raw_parts(rng.random_range(0..400), 7) } else { b }
}

/// One random command, valid or not.
fn random_command(rng: &mut ChaCha8Rng, state: &GameState, defs: &Defs) -> Command {
    let kinds = defs.buildings();
    let kind = if rng.random_range(0..12) == 0 { bogus(rng) } else { kinds[rng.random_range(0..kinds.len())].id };
    let rotation = Rotation::new(rng.random_range(0..4)).unwrap_or_default();
    let road = if rng.random_range(0..12) == 0 { bogus(rng) } else { defs.roads()[rng.random_range(0..defs.roads().len())].id };
    match rng.random_range(0..52) {
        0 => Command::SetTaxRate { percent: rng.random_range(0..=255) },
        1 => Command::SetWorkforcePriority {
            category: JobCategory::ALL[rng.random_range(0..JobCategory::ALL.len())],
            priority: [JobPriority::Low, JobPriority::Normal, JobPriority::High][rng.random_range(0..3)],
        },
        2..6 => Command::PlaceBuilding { kind, origin: position(rng, state), rotation },
        6 => Command::DemolishBuilding { building: victim(rng, state, defs) },
        7 => Command::CancelConstruction { building: victim(rng, state, defs) },
        8 => Command::UpgradeBuilding { building: building(rng, state) },
        9 => Command::SetUpgradeLocked { building: building(rng, state), locked: rng.random_range(0..2) == 0 },
        10 => Command::MoveBuilding { building: victim(rng, state, defs), origin: position(rng, state), rotation },
        11 => Command::SetBuildingPaused { building: building(rng, state), paused: rng.random_range(0..2) == 0 },
        12 | 13 => {
            let from = position(rng, state);
            let len = rng.random_range(0..12);
            let tiles = if rng.random_range(0..3) == 0 {
                (0..len).map(|_| position(rng, state)).collect()
            } else {
                (0..len).map(|i| from + IVec2::new(i, 0)).collect()
            };
            Command::BuildRoad { tiles, road }
        }
        14 => Command::RemoveRoad { tiles: (0..rng.random_range(0..6)).map(|_| position(rng, state)).collect() },
        15 => Command::SetRecipe { building: building(rng, state), recipe: rng.random_range(0..5) },
        16 => Command::SetProductionPriority {
            building: building(rng, state),
            priority: [JobPriority::Low, JobPriority::Normal, JobPriority::High][rng.random_range(0..3)],
        },
        17 => Command::QueueProduction {
            building: building(rng, state),
            recipe: rng.random_range(0..4),
            units: [None, Some(0), Some(5), Some(u32::MAX)][rng.random_range(0..4)],
        },
        18 => Command::ClearQueue { building: building(rng, state) },
        19 => {
            let resource = defs.resources()[rng.random_range(0..defs.resources().len())].id;
            let buy = [None, Some(0), Some(50), Some(u32::MAX)][rng.random_range(0..4)];
            let sell = [None, Some(0), Some(40), Some(u32::MAX)][rng.random_range(0..4)];
            Command::SetTradeOrder { order: TradeOrder { resource, buy_below: buy, sell_above: sell, daily_limit: rng.random_range(0..1_000) } }
        }
        20 => Command::RemoveTradeOrder { resource: defs.resources()[rng.random_range(0..defs.resources().len())].id },
        21 => {
            let crop = if rng.random_range(0..5) == 0 { bogus(rng) } else { defs.crops()[rng.random_range(0..defs.crops().len())].id };
            Command::SetCrop { building: building(rng, state), crop }
        }
        22 => Command::DebugSetWeather {
            weather: Weather::ALL[rng.random_range(0..Weather::ALL.len())],
            intensity: rng.random_range(0..=255),
            days: rng.random_range(0..5),
        },
        23 => Command::QueueResearch { tech: tech(rng, defs) },
        24 => Command::QueueResearchWithPrerequisites { tech: tech(rng, defs) },
        25 => Command::CancelResearch { tech: tech(rng, defs) },
        26 => Command::MoveResearch { tech: tech(rng, defs), position: rng.random_range(0..12) },
        27 => {
            let serial = match state.events().pending() {
                Some(p) if rng.random_range(0..4) > 0 => p.serial,
                _ => rng.random_range(0..100),
            };
            Command::ResolveEvent { serial, choice: rng.random_range(0..5) }
        }
        28 => {
            let event = if rng.random_range(0..8) == 0 { bogus(rng) } else { defs.events()[rng.random_range(0..defs.events().len())].id };
            Command::DebugTriggerEvent { event }
        }
        29 | 44..52 => Command::PlaceBuildingWithAccess { kind, origin: position(rng, state), rotation, road },
        30 | 31 => Command::Diplomacy { kingdom: kingdom(rng), action: diplomatic_action(rng, defs) },
        32 => {
            let serial = match state.diplomacy().proposals().first() {
                Some(p) if rng.random_range(0..4) > 0 => p.serial,
                _ => rng.random_range(0..100),
            };
            Command::AnswerProposal { serial, accept: rng.random_range(0..2) == 0 }
        }
        33 => Command::SendCaravan { kingdom: kingdom(rng), goods: goods(rng, defs), gold: rng.random_range(0..800), buy: goods(rng, defs) },
        34 => Command::DebugGrant { gold: rng.random_range(0..3_000), goods: goods(rng, defs) },
        35 => Command::DebugSpawnHouseholds { count: if rng.random_range(0..10) == 0 { rng.random_range(0..60) } else { rng.random_range(0..4) } },
        36 => Command::DebugSetRelation { kingdom: kingdom(rng), value: rng.random_range(i8::MIN..=i8::MAX) },
        37 => {
            let unit = if rng.random_range(0..10) == 0 { bogus(rng) } else { defs.units()[rng.random_range(0..defs.units().len())].id };
            Command::Recruit { building: building(rng, state), unit, count: rng.random_range(0..10) }
        }
        38 => Command::DismissSoldiers { soldiers: citizens(rng, state) },
        39 => Command::CreateCompany { soldiers: citizens(rng, state) },
        40 => {
            let to = (rng.random_range(0..3) > 0).then(|| company(rng, state));
            Command::AssignSoldiers { soldiers: citizens(rng, state), company: to }
        }
        41 => Command::DisbandCompany { company: company(rng, state) },
        42 => Command::CompanyOrder { company: company(rng, state), order: order(rng, state) },
        43 => {
            let kingdom = (rng.random_range(0..2) == 0).then(|| kingdom(rng));
            match rng.random_range(0..20) {
                0 => Command::DebugUnlockAllResearch,
                1..=3 => Command::HoldFestival,
                _ => Command::DebugSpawnThreat { kingdom, size: rng.random_range(0..150) },
            }
        }
        _ => Command::DebugSpawnHouseholds { count: 1 },
    }
}

fn round_trip(state: &GameState, defs: &Defs, label: &str) -> GameState {
    let header = SaveHeader::for_state(state, defs, label, 0);
    let bytes = encode(&header, state).unwrap_or_else(|e| panic!("{label}: encode: {e}"));
    let loaded = decode(&bytes, defs).unwrap_or_else(|e| panic!("{label}: a fuzzed game loads: {e}")).state;
    let again = encode(&header, &loaded).unwrap_or_else(|e| panic!("{label}: encode loaded: {e}"));
    assert!(bytes == again, "{label}: the loaded game saves the same bytes");
    loaded
}

fn fuzz_commands(seed: u64) {
    let defs = load_defs();
    let mut state = GameState::new(&defs, GameSettings::new(&defs, seed, WorldSize::Small));
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xF0F0);
    // Some wealth so that valid commands do something.
    let materials: Vec<ResourceAmount> = ["wood", "planks", "stone", "bricks", "tools", "loam"]
        .iter()
        .filter_map(|k| defs.resource_id(k))
        .map(|resource| ResourceAmount { resource, amount: 300 })
        .collect();
    state.tick(
        &defs,
        vec![
            Command::DebugUnlockAllResearch,
            Command::DebugGrant { gold: 20_000, goods: materials.clone() },
            Command::DebugSpawnHouseholds { count: 20 },
        ],
    );
    let (mut sent, mut rejected, mut placed) = (0u32, 0u32, 0u32);
    for tick in 0..2 * TICKS_PER_DAY {
        let mut commands = Vec::new();
        if tick % TICKS_PER_DAY == TICKS_PER_DAY / 2 {
            commands.push(Command::DebugGrant { gold: 5_000, goods: materials.clone() });
        }
        if rng.random_range(0..12) == 0 {
            for _ in 0..rng.random_range(1..4) {
                commands.push(random_command(&mut rng, &state, &defs));
            }
        }
        sent += commands.len() as u32;
        let events = state.tick(&defs, commands);
        rejected += events.iter().filter(|e| matches!(e, kingdoms_sim::event::SimEvent::CommandRejected { .. })).count() as u32;
        placed += events.iter().filter(|e| matches!(e, kingdoms_sim::event::SimEvent::BuildingPlaced { .. })).count() as u32;
        if tick % TICKS_PER_HOUR == 0 {
            state.validate(&defs).unwrap_or_else(|e| panic!("seed {seed} tick {tick}: valid after fuzzed commands: {e}"));
        }
        if tick % (TICKS_PER_DAY / 2) == TICKS_PER_DAY / 4 {
            // Save and load mid-game: both continue identically for an hour.
            let mut loaded = round_trip(&state, &defs, &format!("seed {seed} tick {tick}"));
            for t in 0..TICKS_PER_HOUR {
                let commands = if t % 37 == 0 { vec![random_command(&mut rng, &state, &defs)] } else { Vec::new() };
                let a = state.tick(&defs, commands.clone());
                let b = loaded.tick(&defs, commands);
                assert_eq!(a, b, "seed {seed} tick {tick}+{t}: the loaded game continues identically");
            }
            state = loaded;
        }
    }
    state.validate(&defs).unwrap_or_else(|e| panic!("seed {seed}: valid at the end: {e}"));
    eprintln!(
        "seed {seed}: {sent} fuzzed commands, {rejected} rejected, {placed} buildings placed; {} citizens, {} buildings, {} soldiers, game over {:?}",
        state.population().len(),
        state.buildings().len(),
        state.military().soldiers().count(),
        state.game_over().map(|g| g.reason)
    );
    assert!(sent > 500 && rejected > 0 && rejected < sent, "seed {seed}: a mix of accepted and rejected commands");
}

#[test]
fn fuzzed_commands_never_break_the_game_seed_1() {
    fuzz_commands(1);
}

#[test]
fn fuzzed_commands_never_break_the_game_seed_2() {
    fuzz_commands(2);
}

#[test]
fn fuzzed_commands_never_break_the_game_seed_3() {
    fuzz_commands(3);
}

#[test]
fn fuzzed_commands_never_break_the_game_seed_4() {
    fuzz_commands(4);
}

#[test]
fn fuzzed_commands_never_break_the_game_seed_5() {
    fuzz_commands(5);
}

#[test]
fn fuzzed_commands_never_break_the_game_seed_6() {
    fuzz_commands(6);
}

/// Builds the kingdom-wide reports the HUD and the management tabs read every frame; a loaded
/// state must not make any of them panic (e.g. overflowing cooldown arithmetic).
fn read_reports(state: &GameState, defs: &Defs) {
    use kingdoms_sim::report;
    let _ = report::economy_summary(state, defs);
    let _ = report::finance_report(state, defs);
    let _ = report::festival_report(state, defs);
    let _ = report::court_report(state, defs);
    let _ = report::events_report(state, defs);
    let _ = report::pending_event(state, defs);
    let _ = report::workforce_report(state, defs);
    let _ = report::trade_report(state, defs);
    let _ = report::logistics_report(state, defs);
    let _ = report::storage_report(state, defs);
    let _ = report::population_summary(state, defs);
    let _ = report::happiness_report(state, defs);
    let _ = report::agriculture_summary(state, defs);
    let _ = report::weather_report(state, defs);
    let _ = report::research_report(state, defs);
    let _ = report::diplomacy_report(state, defs);
    let _ = report::military_report(state, defs);
    let _ = report::rank_report(state, defs);
}

/// Loads `bytes`; a successful load must validate, report and keep running.
fn try_load(bytes: &[u8], defs: &Defs, what: &str) -> Result<(), SaveError> {
    let mut state = decode(bytes, defs)?.state;
    state.validate(defs).unwrap_or_else(|e| panic!("{what}: a loaded state is valid: {e}"));
    read_reports(&state, defs);
    state.tick(defs, vec![Command::HoldFestival]);
    for _ in 0..20 {
        state.tick(defs, Vec::new());
    }
    read_reports(&state, defs);
    Ok(())
}

#[test]
fn crafted_saves_are_rejected_without_panics() {
    let defs = load_defs();
    let mut state = GameState::new(&defs, GameSettings::new(&defs, 77, WorldSize::Small));
    state.tick(&defs, vec![Command::DebugSpawnHouseholds { count: 10 }]);
    for _ in 0..TICKS_PER_DAY / 2 {
        state.tick(&defs, Vec::new());
    }
    let header = SaveHeader::for_state(&state, &defs, "fuzz", 0);
    let bytes = encode(&header, &state).unwrap_or_else(|e| panic!("encode: {e}"));
    try_load(&bytes, &defs, "original").unwrap_or_else(|e| panic!("the original loads: {e}"));
    let mut rng = ChaCha8Rng::seed_from_u64(99);
    let (mut errors, mut loaded) = (std::collections::BTreeMap::new(), 0);
    let mut count = |result: Result<(), SaveError>| match result {
        Ok(()) => loaded += 1,
        Err(e) => {
            assert!(!e.to_string().is_empty());
            let name = format!("{e:?}");
            let kind = name.split(['(', ' ', '{']).next().unwrap_or("").to_owned();
            *errors.entry(kind).or_insert(0) += 1;
        }
    };
    // Truncated at random lengths.
    for _ in 0..40 {
        let len = rng.random_range(0..bytes.len());
        count(try_load(&bytes[..len], &defs, "truncated"));
    }
    // Damaged bytes, checksum left alone (detected by the checksum).
    for _ in 0..40 {
        let mut b = bytes.clone();
        let i = rng.random_range(0..b.len());
        b[i] ^= 1 << rng.random_range(0..8);
        count(try_load(&b, &defs, "damaged"));
    }
    // Damaged bytes with a repaired checksum: the decoder and the validation must cope.
    for round in 0..300 {
        let mut b = bytes.clone();
        for _ in 0..rng.random_range(1..4) {
            let i = rng.random_range(16..b.len());
            b[i] = if round % 3 == 0 { rng.random_range(0..=255) } else { b[i] ^ (1 << rng.random_range(0..8)) };
        }
        let crc = crc32fast::hash(&b[16..]);
        b[12..16].copy_from_slice(&crc.to_le_bytes());
        count(try_load(&b, &defs, &format!("crafted #{round}")));
    }
    // Garbage and an empty file.
    for _ in 0..20 {
        let len = rng.random_range(0..2_000);
        let garbage: Vec<u8> = (0..len).map(|_| rng.random_range(0..=255)).collect();
        count(try_load(&garbage, &defs, "garbage"));
    }
    count(try_load(&[], &defs, "empty"));
    eprintln!("crafted saves: {loaded} loaded (harmless changes), errors {errors:?}");
    assert!(errors.len() >= 4, "different kinds of damage are told apart: {errors:?}");
}
