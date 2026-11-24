//! The scripted reasonable player shared by the balance tests (`reasonable_player`,
//! `campaign`): where it builds, what it builds next and how it decides events.
// Each test crate uses a different part of these helpers.
#![allow(dead_code)]

use std::path::Path;

use glam::IVec2;
use kingdoms_sim::buildings::{Rotation, check_placement};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingTypeId, Defs};
use kingdoms_sim::report;
use kingdoms_sim::roads::plan_access_road;
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::{Season, TICKS_PER_HOUR};
use kingdoms_sim::world::Vegetation;

pub fn load_defs() -> Defs {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"));
    match Defs::load(dir) {
        Ok(defs) => defs,
        Err(e) => panic!("shipped game data must load: {e}"),
    }
}

pub fn kind(defs: &Defs, key: &str) -> BuildingTypeId {
    match defs.building_id(key) {
        Some(id) => id,
        None => panic!("missing building {key}"),
    }
}

/// Mature trees within `radius` of `p`.
pub fn trees_near(state: &GameState, p: IVec2, radius: i32) -> i32 {
    let mut n = 0;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if let Some(t) = state.world().get(p + IVec2::new(dx, dy))
                && matches!(t.vegetation, Vegetation::Tree { growth, .. } if growth >= 192)
            {
                n += 1;
            }
        }
    }
    n
}

/// The best valid spot for `kind` near `center` (rings `min..max`) that a path can connect:
/// the first ring with a valid spot, best `score` within it (ties: scan order); with
/// `all_rings` the best score of all rings.
pub fn find_spot_in(
    state: &GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    center: IVec2,
    rings: std::ops::Range<i32>,
    all_rings: bool,
    score: impl Fn(IVec2) -> i32,
) -> Option<(IVec2, Rotation, Command)> {
    let path = defs.road_id("path")?;
    let mut best: Option<(i32, IVec2, Rotation, Command)> = None;
    for r in rings {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) != r {
                    continue;
                }
                let origin = center + IVec2::new(dx, dy);
                for rot in 0..4 {
                    let rotation = Rotation::new(rot).unwrap_or_default();
                    let check = check_placement(state, defs, kind, origin, rotation);
                    // Fields stay untouched (building on them would destroy the crops).
                    let on_fields = |p: IVec2| state.agriculture().owner_of(p).is_some();
                    if !check.ok || check.clearing.tiles() > 2 || check.footprint.tiles().any(on_fields) {
                        continue;
                    }
                    let Some(plan) = plan_access_road(state, defs, kind, origin, rotation, path) else { continue };
                    if plan.tiles.iter().any(|t| t.is_new && on_fields(t.pos)) {
                        continue;
                    }
                    let Some(road) = plan.to_command() else { continue };
                    let s = score(origin);
                    if best.as_ref().is_none_or(|(b, ..)| s > *b) {
                        best = Some((s, origin, rotation, road));
                    }
                }
            }
        }
        if best.is_some() && !all_rings {
            break;
        }
    }
    best.map(|(_, o, r, c)| (o, r, c))
}

/// The first valid spot for `kind` near `center` (rings `min..max`, nearest first) that a path
/// can connect and `accept` takes; `accept` is asked before the (costly) placement check.
pub fn find_spot_where(
    state: &GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    center: IVec2,
    rings: std::ops::Range<i32>,
    accept: impl Fn(IVec2) -> bool,
) -> Option<(IVec2, Rotation, Command)> {
    let path = defs.road_id("path")?;
    for r in rings {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) != r {
                    continue;
                }
                let origin = center + IVec2::new(dx, dy);
                if !accept(origin) {
                    continue;
                }
                for rot in 0..4 {
                    let rotation = Rotation::new(rot).unwrap_or_default();
                    let check = check_placement(state, defs, kind, origin, rotation);
                    let on_fields = |p: IVec2| state.agriculture().owner_of(p).is_some();
                    if !check.ok || check.clearing.tiles() > 2 || check.footprint.tiles().any(on_fields) {
                        continue;
                    }
                    let Some(plan) = plan_access_road(state, defs, kind, origin, rotation, path) else { continue };
                    if plan.tiles.iter().any(|t| t.is_new && on_fields(t.pos)) {
                        continue;
                    }
                    if let Some(road) = plan.to_command() {
                        return Some((origin, rotation, road));
                    }
                }
            }
        }
    }
    None
}

/// [`find_spot_in`] with the nearest ring that has a valid spot.
pub fn find_spot(
    state: &GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    center: IVec2,
    rings: std::ops::Range<i32>,
    score: impl Fn(IVec2) -> i32,
) -> Option<(IVec2, Rotation, Command)> {
    find_spot_in(state, defs, kind, center, rings, false, score)
}

/// What the player builds next, in order; repeated entries are built again later. Goals in
/// `blocked` (no valid spot recently) are passed over.
pub fn next_goal(state: &GameState, defs: &Defs, blocked: &[&str]) -> Option<&'static str> {
    let count = |key: &str| state.buildings().count_of(kind(defs, key));
    let summary = report::population_summary(state, defs);
    // Places newcomers may take (every house keeps one for births and marriages).
    let free = summary.immigrant_places;
    let pop = summary.population as usize;
    let open = |key: &str| !blocked.contains(&key);
    // Houses already being built count as room on the way.
    let house = kind(defs, "house_wood");
    let building_houses = state.buildings().iter().filter(|(_, b)| b.kind() == house && b.construction_site().is_some()).count();
    if summary.homeless > 0 && building_houses == 0 {
        return Some("house_wood");
    }
    // The basics: water, wood, food, the market and the first fields in time for spring.
    let basics: [(&'static str, usize); 5] =
        [("well", 1), ("woodcutter", 1), ("chicken_farm", 1), ("market", 1), ("vegetable_farm", 1)];
    if let Some((key, _)) = basics.iter().find(|(key, n)| count(key) < *n && open(key)) {
        return Some(key);
    }
    // Money and firewood: keep 40 gold in the treasury, and from autumn on the wood the
    // households will burn until spring (0.6 per citizen and winter day, +20 % for snow).
    let wood = state.economy().stock().get(defs.resource_id("wood").expect("wood"));
    let clock = state.clock();
    let winter_days_ahead = match clock.season() {
        Season::Autumn => 6,
        Season::Winter => 7 - clock.day_of_season(),
        _ => 0,
    };
    let reserve =
        (f64::from(summary.population) * f64::from(defs.rules().population.fuel_per_resident_day) * winter_days_ahead as f64 * 1.2)
            as u32;
    let affordable = |key: &str| {
        let cost = &defs.building(kind(defs, key)).cost;
        let wood_cost: u32 =
            cost.materials.iter().filter(|m| defs.resource(m.resource).key == "wood").map(|m| m.amount).sum();
        // Food stores may use the firewood reserve: a full barn helps nobody in winter.
        let reserve = if key == "storehouse" { 0 } else { reserve };
        state.economy().treasury() >= i64::from(cost.gold) + 40 && (wood_cost == 0 || wood >= reserve + wood_cost)
    };
    // Workplaces only for adults that are there; two stay free as builders.
    let slots: u32 = state.buildings().iter().map(|(_, b)| u32::from(defs.building(b.kind()).workers)).sum();
    let adults = state.workforce().available();
    let workers = |key: &str| u32::from(defs.building(kind(defs, key)).workers);
    let staffable = |key: &str| open(key) && affordable(key) && slots + workers(key) + 2 <= adults;
    // Daily food ahead of the population: an animal or fishing building per 9 citizens (food
    // may take the builders' hands).
    let food = count("chicken_farm") + count("fishery");
    if food * 9 < pop + 4 {
        let fits = |key: &str| open(key) && affordable(key) && slots + workers(key) <= adults;
        if count("fishery") == 0 && fits("fishery") {
            return Some("fishery");
        }
        if fits("chicken_farm") {
            return Some("chicken_farm");
        }
    }
    // Firewood: a woodcutter per 10 citizens, each with a forester replanting.
    if count("woodcutter") * 10 < pop && staffable("woodcutter") {
        return Some("woodcutter");
    }
    if count("forester") < count("woodcutter") && staffable("forester") {
        return Some("forester");
    }
    // The harvest piles up in the barns: more food stores (one per 150 units waiting). A store
    // gets its first carrier before every other workplace (base staffing), so it is built even
    // when no adult is free: full barns block the next harvest.
    let in_barns: u32 = report::agriculture_summary(state, defs).in_barns.iter().map(|a| a.amount).sum();
    let site = |key: &str| state.buildings().iter().any(|(_, b)| b.kind() == kind(defs, key) && b.construction_site().is_some());
    if in_barns > 150 * (1 + count("storehouse") as u32) / 2 && !site("storehouse") && open("storehouse") && affordable("storehouse")
    {
        return Some("storehouse");
    }
    // Room for newcomers (they bring the hands for the next workplaces).
    if free < 3 && building_houses == 0 && affordable("house_wood") {
        return Some("house_wood");
    }
    // More fields only while they can still be sown this year (spring, start of summer).
    let sowing_time = clock.season() == Season::Spring || (clock.season() == Season::Summer && clock.day_of_season() <= 2);
    let order: [(&'static str, usize); 5] =
        [("vegetable_farm", 2), ("storehouse", 1), ("warehouse", 1), ("quarry", 1), ("vegetable_farm", 3)];
    if let Some((key, _)) =
        order.iter().find(|(key, n)| count(key) < *n && staffable(key) && (*key != "vegetable_farm" || sowing_time))
    {
        return Some(key);
    }
    // Room for the harvest and the winter stock: a food store per 20 citizens.
    if count("storehouse") * 20 < pop && staffable("storehouse") {
        return Some("storehouse");
    }
    if count("well") * 25 < pop && open("well") && affordable("well") {
        return Some("well");
    }
    None
}

/// Gold a choice costs (its negative gold effects).
pub fn choice_cost(choice: &kingdoms_sim::defs::EventChoice) -> i64 {
    choice
        .effects
        .iter()
        .map(|e| match e {
            kingdoms_sim::defs::EventEffect::Gold(g) if *g < 0 => i64::from(g.unsigned_abs()),
            _ => 0,
        })
        .sum()
}

/// The reasonable player's decision on the pending event: the preferred choices of a short
/// table (damage control, relief), if available and the treasury keeps 250 gold; otherwise
/// the default.
pub fn decide_event(state: &GameState, defs: &Defs) -> Option<Command> {
    let pending = state.events().pending()?;
    let def = defs.event(pending.event);
    let free = kingdoms_sim::population::free_housing(state, defs);
    let preferred: &[u8] = match def.key.as_str() {
        "bad_harvest" | "drought" | "flood" | "fire" | "storm_damage" | "bandit_raid" => &[1],
        "plague" => &[2, 1],
        "refugees" | "war_refugees" if free < 8 => &[1],
        "political_unrest" => &[2],
        _ => &[],
    };
    let choice = preferred
        .iter()
        .copied()
        .find(|c| {
            def.choices.get(usize::from(*c)).is_some_and(|choice| {
                kingdoms_sim::events::choice_problem(state, defs, choice).is_none()
                    && state.economy().treasury() - choice_cost(choice) >= 250
            })
        })
        .unwrap_or(0);
    Some(Command::ResolveEvent { serial: pending.serial, choice })
}

/// Goals that found no valid spot, with the tick of the last failed attempt.
pub type Blocked = Vec<(&'static str, u64)>;

/// Chooses the next building (see [`next_goal`], [`naive_next_goal`]).
pub type GoalFn = fn(&GameState, &Defs, &[&str]) -> Option<&'static str>;

pub fn plan(state: &GameState, defs: &Defs, blocked: &mut Blocked, next: GoalFn) -> Vec<Command> {
    plan_with(state, defs, blocked, next, None)
}

/// Where a goal may stand beyond the plan's rings (20 tiles from the start): `ok(goal, origin)`.
pub type Outskirts<'a> = &'a dyn Fn(&str, IVec2) -> bool;

/// [`plan`] that, when the old town has no spot for the goal, looks 20–40 tiles out for an
/// origin `outskirts` accepts.
pub fn plan_with(state: &GameState, defs: &Defs, blocked: &mut Blocked, next: GoalFn, outskirts: Option<Outskirts<'_>>) -> Vec<Command> {
    let now = state.clock().tick;
    // A goal without a spot (or without the material for it) is retried after six hours.
    blocked.retain(|(_, at)| now.saturating_sub(*at) < 6 * TICKS_PER_HOUR);
    let names: Vec<&str> = blocked.iter().map(|(k, _)| *k).collect();
    let Some(goal) = next(state, defs, &names) else { return Vec::new() };
    let k = kind(defs, goal);
    let center = state.world().start;
    let spot = match goal {
        // The most trees within reach of the town hall's carriers; not worth it below 8 trees.
        "woodcutter" => find_spot_in(state, defs, k, center, 4..19, true, |o| trees_near(state, o, 8))
            .filter(|(o, ..)| trees_near(state, *o, 8) >= 8),
        "forester" => {
            // Next to the newest woodcutter.
            let cutter = state.buildings().iter().rev().find(|(_, b)| b.kind() == kind(defs, "woodcutter")).map(|(_, b)| b.origin());
            find_spot(state, defs, k, cutter.unwrap_or(center), 3..14, |_| 0)
        }
        "fishery" | "quarry" => find_spot(state, defs, k, center, 3..24, |_| 0),
        // Mines must stand by their deposit (anywhere in the land).
        "iron_mine" | "coal_mine" => find_spot(state, defs, k, center, 3..40, |_| 0),
        // Within reach of the town hall's carriers (radius 20 from its footprint).
        "vegetable_farm" => find_spot(state, defs, k, center, 5..16, |_| 0),
        "well" => find_spot(state, defs, k, center, 2..12, |_| 0),
        _ => find_spot(state, defs, k, center, 4..20, |_| 0),
    };
    let spot = spot.or_else(|| {
        let ok = outskirts?;
        find_spot_where(state, defs, k, center, 20..40, |o| ok(goal, o))
    });
    match spot {
        Some((origin, rotation, road)) => vec![Command::PlaceBuilding { kind: k, origin, rotation }, road],
        None => {
            blocked.push((goal, now));
            Vec::new()
        }
    }
}
