//! Developer aid `KINGDOMS_DEV_SETTLEMENT`: lays out a settlement around the headquarters.
//! * `=1` ([`DevPlan::Showcase`]): construction sites, finished buildings in every rotation
//!   (including the non-square sawmill) and several road types.
//! * `=2` ([`DevPlan::Town`]): a small but viable town that keeps growing for two years,
//!   so long warps show a living settlement (fields in every stage, pastures with animals,
//!   smoke in winter). Every game hour it builds what an attentive player would build next
//!   ([`town_goal`]): houses when people are homeless or room runs out, a well, woodcutters with
//!   foresters (building material and firewood, more in autumn), chicken farms, a market,
//!   vegetable farms sown in time, storehouses and a warehouse for the harvest – opening only
//!   workplaces for adults that are there (two stay free as builders, so carriers and builders
//!   are never short) and keeping firewood for the winter (woodcutters and foresters may use
//!   that reserve: they bring the firewood in). With that base in place it adds an orchard, a
//!   wheat farm, a quarry, a charcoal kiln, a sawmill (with a limited plank order) and, once
//!   the sawmill makes planks, a sheep farm – most of them in the second year, because workers
//!   and carriers walk. It unlocks all technologies first with the developer command
//!   `DebugUnlockAllResearch` (wheat farm, sheep farm and orchard need research, which would
//!   take too long for a test town) and sets two trade orders (buy stone, sell planks) for a
//!   trading post. It decides no events: they expire to their default choice.
//!
//! Everything goes through regular commands and the simulation's own validation: the showcase
//! is planned with [`check_placement`] and placed with `PlaceBuilding`; once the sites exist,
//! their access roads are planned with [`plan_access_road`] and built with `BuildRoad`. The town
//! checks spots with [`check_placement`] and [`plan_access_road`] (a path must reach it) and
//! places them with `PlaceBuildingWithAccess`; a goal the simulation rejects or that has no spot
//! is retried six hours later.

use std::collections::BTreeMap;

use glam::IVec2;
use kingdoms_sim::buildings::{EntranceStatus, Rotation, check_placement};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingRole, BuildingTypeId, Cost, Defs, ResourceId, RoadTypeId};
use kingdoms_sim::economy::TradeOrder;
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::grid::TileRect;
use kingdoms_sim::report;
use kingdoms_sim::roads::plan_access_road;
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::{Season, TICKS_PER_HOUR};
use kingdoms_sim::world::Vegetation;

/// Where a planned building should stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Site {
    /// The nearest free spot.
    Near,
    /// On a terrace edge: lower ground right in front of the footprint (shows the depth
    /// ordering of buildings above cliffs).
    TerraceEdge,
}

/// Which test settlement to lay out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevPlan {
    /// Buildings in every rotation and several road types (`KINGDOMS_DEV_SETTLEMENT=1`).
    Showcase,
    /// A small, viable, growing town (`KINGDOMS_DEV_SETTLEMENT=2`).
    Town,
}

impl DevPlan {
    /// Parses the value of `KINGDOMS_DEV_SETTLEMENT`: `2`, `stadt`, `town`, `wirtschaft` or
    /// `economy` for the town, any other true value for the showcase; `None` for off values.
    pub fn parse(value: &str) -> Option<DevPlan> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "0" | "false" | "no" | "off" => None,
            "2" | "stadt" | "town" | "wirtschaft" | "economy" => Some(DevPlan::Town),
            _ => Some(DevPlan::Showcase),
        }
    }
}

/// Buildings to place, in priority order: `(building id, quarter turns, site)`.
const SHOWCASE: &[(&str, u8, Site)] = &[
    ("sawmill", 0, Site::Near),
    ("chicken_farm", 0, Site::Near),
    ("sawmill", 1, Site::Near),
    ("woodcutter", 2, Site::Near),
    ("forester", 1, Site::Near),
    ("woodcutter", 3, Site::TerraceEdge),
    ("sawmill", 3, Site::Near),
    ("sawmill", 2, Site::Near),
    ("house_wood", 0, Site::Near),
];

/// Gold the town keeps for wages and upkeep beyond a building's price.
const GOLD_RESERVE: i64 = 40;
/// Hours a goal without a spot (or rejected by the simulation) waits before the next try.
const RETRY_HOURS: u64 = 6;
/// Planks the town's sawmill cuts (enough for the plank-built farms, without eating the wood).
const SAWMILL_PLANKS: u32 = 24;
/// Coal the town's charcoal kiln burns per day (one 4-hour cycle from 6 o'clock, 3 wood): it
/// smokes every morning without eating the firewood.
const KILN_COAL_PER_DAY: u32 = 1;

/// Road types for the access roads, in order of the placed buildings (the rest use the
/// cheapest type). Shows the different road sprites side by side.
const ROAD_TYPES: &[&str] = &["gravel", "road"];

/// How far from the headquarters the planner looks for free spots.
const SEARCH_RADIUS: i32 = 18;

/// One planned building.
#[derive(Clone, Copy, Debug)]
struct Planned {
    kind: BuildingTypeId,
    origin: IVec2,
    rotation: Rotation,
}

/// Progress of the test settlement.
#[derive(Debug)]
pub struct DevSettlement {
    plan: DevPlan,
    /// Showcase: the buildings placed at the start and whether their roads were sent.
    planned: Vec<Planned>,
    roads_queued: bool,
    /// Town: goals waiting for a retry with the tick of the failed attempt, and the goal sent
    /// last (a rejection blocks it).
    blocked: Vec<(&'static str, u64)>,
    sent: Option<&'static str>,
}

/// Gold and materials still available to the plan.
struct Budget {
    gold: i64,
    materials: BTreeMap<ResourceId, u32>,
}

impl Budget {
    fn of(state: &GameState, defs: &Defs) -> Self {
        let materials = defs.resources().iter().map(|r| (r.id, state.economy().available(r.id))).collect();
        Self { gold: state.economy().treasury(), materials }
    }

    /// Reserves `cost` if it is affordable.
    fn take(&mut self, cost: &Cost) -> bool {
        let affordable = self.gold >= i64::from(cost.gold)
            && cost.materials.iter().all(|m| self.materials.get(&m.resource).copied().unwrap_or(0) >= m.amount);
        if affordable {
            self.gold -= i64::from(cost.gold);
            for m in &cost.materials {
                if let Some(have) = self.materials.get_mut(&m.resource) {
                    *have -= m.amount;
                }
            }
        }
        affordable
    }
}

impl DevSettlement {
    /// Plans the buildings around the headquarters and returns the `PlaceBuilding` commands
    /// (and the trade orders of the economy plan).
    pub fn start(state: &GameState, defs: &Defs, plan: DevPlan) -> (Self, Vec<Command>) {
        let mut settlement = Self { plan, planned: Vec::new(), roads_queued: false, blocked: Vec::new(), sent: None };
        if plan == DevPlan::Town {
            // Wheat farm, sheep farm and orchard need research (developer command).
            let mut commands = vec![Command::DebugUnlockAllResearch];
            let order = |key: &str, buy_below: Option<u32>, sell_above: Option<u32>, daily_limit: u32| {
                defs.resource_id(key).map(|resource| Command::SetTradeOrder { order: TradeOrder { resource, buy_below, sell_above, daily_limit } })
            };
            commands.extend(order("stone", Some(40), None, 20));
            commands.extend(order("planks", None, Some(60), 30));
            return (settlement, commands);
        }
        let center = headquarters_center(state, defs).unwrap_or(state.world().start);
        // Keep a free ring around existing buildings so access roads can pass.
        let mut reserved: Vec<TileRect> = state.buildings().iter().map(|(_, b)| b.rect().expanded(1)).collect();
        let mut budget = Budget::of(state, defs);
        let mut planned = Vec::new();
        for &(key, turns, site) in SHOWCASE {
            let (Some(kind), Some(rotation)) = (defs.building_id(key), Rotation::new(turns)) else {
                log::warn!("dev settlement: unknown building '{key}' or rotation {turns}");
                continue;
            };
            let Some(spot) = find_spot(state, defs, kind, rotation, site, center, &reserved) else {
                log::warn!("dev settlement: no spot for {key} at rotation {turns}");
                continue;
            };
            if !budget.take(&spot.cost) {
                log::info!("dev settlement: {key} skipped (start budget used up)");
                continue;
            }
            log::info!("dev settlement: {key} at {} facing {:?}", spot.rect, rotation.facing());
            reserved.push(spot.rect.expanded(1));
            reserved.push(TileRect::tile(spot.entrance));
            planned.push(Planned { kind, origin: spot.origin, rotation });
        }
        log::info!("dev settlement: placing {} buildings around {center}", planned.len());
        let commands: Vec<Command> = planned
            .iter()
            .map(|p| Command::PlaceBuilding { kind: p.kind, origin: p.origin, rotation: p.rotation })
            .collect();
        settlement.planned = planned;
        (settlement, commands)
    }

    /// Called after every tick with its events; returns further commands (showcase: the access
    /// roads once the construction sites exist; town: the next building every game hour).
    pub fn after_tick(&mut self, state: &GameState, defs: &Defs, events: &[SimEvent]) -> Vec<Command> {
        match self.plan {
            DevPlan::Town => self.grow_town(state, defs, events),
            DevPlan::Showcase if self.roads_queued => Vec::new(),
            DevPlan::Showcase => self.showcase_roads(state, defs, events),
        }
    }

    fn showcase_roads(&mut self, state: &GameState, defs: &Defs, events: &[SimEvent]) -> Vec<Command> {
        let placed: Vec<Planned> = events
            .iter()
            .filter_map(|e| match e {
                SimEvent::BuildingPlaced { kind, origin, rotation, .. } => {
                    Some(Planned { kind: *kind, origin: *origin, rotation: *rotation })
                }
                _ => None,
            })
            .filter(|p| self.planned.iter().any(|q| q.kind == p.kind && q.origin == p.origin && q.rotation == p.rotation))
            .collect();
        let rejected = events.iter().any(|e| matches!(e, SimEvent::CommandRejected { .. }));
        if placed.is_empty() && !rejected {
            return Vec::new();
        }
        self.roads_queued = true;
        let mut budget = Budget::of(state, defs);
        let cheapest = cheapest_road(state, defs);
        let mut commands = Vec::new();
        for (i, p) in placed.iter().enumerate() {
            let preferred = ROAD_TYPES
                .get(i)
                .and_then(|key| defs.road_id(key))
                .filter(|id| state.research().road_unlocked(defs, *id));
            let candidates = preferred.into_iter().chain(cheapest);
            let plan = candidates
                .filter_map(|road| plan_access_road(state, defs, p.kind, p.origin, p.rotation, road))
                .find(|plan| plan.valid && budget.take(&plan.cost));
            match plan {
                Some(plan) if !plan.tiles.is_empty() => commands.push(Command::BuildRoad {
                    tiles: plan.tiles.iter().map(|t| t.pos).collect(),
                    road: plan.road,
                }),
                Some(_) => {}
                None => log::warn!("dev settlement: no access road for the building at {}", p.origin),
            }
        }
        log::info!("dev settlement: building {} access roads", commands.len());
        commands
    }

    /// The town's next step: limits the sawmill's order once it stands, notes rejected goals
    /// and, at the start of every game hour, places the next goal of [`town_goal`].
    fn grow_town(&mut self, state: &GameState, defs: &Defs, events: &[SimEvent]) -> Vec<Command> {
        let now = state.clock().tick;
        let mut commands = Vec::new();
        let kiln = defs.building_id("charcoal_kiln");
        for e in events {
            match e {
                SimEvent::BuildingCompleted { building, kind } if defs.building_id("sawmill") == Some(*kind) => {
                    // A sawmill left on its continuous order would cut up all firewood.
                    commands.push(Command::ClearQueue { building: *building });
                    commands.push(Command::QueueProduction { building: *building, recipe: 0, units: Some(SAWMILL_PLANKS) });
                }
                SimEvent::BuildingCompleted { building, kind } if kiln == Some(*kind) => {
                    // Daily orders instead (below).
                    commands.push(Command::ClearQueue { building: *building });
                }
                SimEvent::CommandRejected { command: Command::PlaceBuildingWithAccess { .. }, reason } => {
                    if let Some(goal) = self.sent.take() {
                        log::info!("dev settlement: {goal} rejected ({reason})");
                        self.blocked.push((goal, now));
                    }
                }
                _ => {}
            }
        }
        if !state.clock().is_new_hour() {
            return commands;
        }
        if state.clock().hour() == 6 {
            for (id, b) in state.buildings().iter().filter(|(_, b)| Some(b.kind()) == kiln && b.is_complete()) {
                if b.production().is_some_and(|p| p.queue().is_empty()) {
                    commands.push(Command::QueueProduction { building: id, recipe: 0, units: Some(KILN_COAL_PER_DAY) });
                }
            }
        }
        self.sent = None;
        self.blocked.retain(|(_, at)| now.saturating_sub(*at) < RETRY_HOURS * TICKS_PER_HOUR);
        let blocked: Vec<&str> = self.blocked.iter().map(|(k, _)| *k).collect();
        let Some(goal) = town_goal(state, defs, &blocked) else { return commands };
        let (Some(kind), Some(road)) = (defs.building_id(goal), cheapest_road(state, defs)) else { return commands };
        let center = headquarters_center(state, defs).unwrap_or(state.world().start);
        let spot = match goal {
            // The most mature trees within reach; not worth it below 8.
            "woodcutter" => town_spot(state, defs, kind, center, 4..19, true, road, |o| trees_near(state, o, 8))
                .filter(|(o, _)| trees_near(state, *o, 8) >= 8),
            "forester" => {
                let cutter = state
                    .buildings()
                    .iter()
                    .rev()
                    .find(|(_, b)| defs.building_id("woodcutter") == Some(b.kind()))
                    .map(|(_, b)| b.origin());
                town_spot(state, defs, kind, cutter.unwrap_or(center), 3..14, false, road, |_| 0)
            }
            "vegetable_farm" | "wheat_farm" | "orchard" | "sheep_farm" => {
                town_spot(state, defs, kind, center, 5..18, false, road, |_| 0)
            }
            "well" => town_spot(state, defs, kind, center, 2..12, false, road, |_| 0),
            "quarry" => town_spot(state, defs, kind, center, 3..24, false, road, |_| 0),
            _ => town_spot(state, defs, kind, center, 4..20, false, road, |_| 0),
        };
        match spot {
            Some((origin, rotation)) => {
                log::info!("dev settlement: {goal} at {origin}");
                self.sent = Some(goal);
                commands.push(Command::PlaceBuildingWithAccess { kind, origin, rotation, road });
            }
            None => {
                log::info!("dev settlement: no spot for {goal}");
                self.blocked.push((goal, now));
            }
        }
        commands
    }
}

/// What the town builds next (see the module docs); goals in `blocked` are passed over.
pub fn town_goal(state: &GameState, defs: &Defs, blocked: &[&str]) -> Option<&'static str> {
    let id = |key: &str| defs.building_id(key);
    let count = |key: &str| id(key).map_or(0, |k| state.buildings().count_of(k));
    let def = |key: &str| defs.building_by_key(key);
    let summary = report::population_summary(state, defs);
    let free = summary.immigrant_places;
    let pop = summary.population as usize;
    let open = |key: &str| !blocked.contains(&key) && def(key).is_some();
    let site = |key: &str| state.buildings().iter().any(|(_, b)| Some(b.kind()) == id(key) && b.construction_site().is_some());
    let building_houses = site("house_wood");
    if summary.homeless > 0 && !building_houses && open("house_wood") {
        return Some("house_wood");
    }
    // The basics: water, wood, food, the market and the first fields.
    let basics: [(&'static str, usize); 5] =
        [("well", 1), ("woodcutter", 1), ("chicken_farm", 1), ("market", 1), ("vegetable_farm", 1)];
    if let Some((key, _)) = basics.iter().find(|(key, n)| count(key) < *n && open(key)) {
        return Some(key);
    }
    // Keep gold for running costs and, from autumn on, the firewood the households burn until
    // spring (fuel per resident and winter day, +20 % for snow).
    let wood = defs.resource_id("wood").map_or(0, |w| state.economy().stock().get(w));
    let clock = state.clock();
    let winter_days_ahead = match clock.season() {
        Season::Autumn => 6,
        Season::Winter => 7u64.saturating_sub(clock.day_of_season()),
        _ => 0,
    };
    let fuel = f64::from(defs.rules().population.fuel_per_resident_day);
    let reserve = (f64::from(summary.population) * fuel * winter_days_ahead as f64 * 1.2) as u32;
    let affordable = |key: &str| {
        let Some(d) = def(key) else { return false };
        let wood_cost: u32 = d.cost.materials.iter().filter(|m| Some(m.resource) == defs.resource_id("wood")).map(|m| m.amount).sum();
        let others = d.cost.materials.iter().filter(|m| Some(m.resource) != defs.resource_id("wood")).all(|m| state.economy().available(m.resource) >= m.amount);
        // Food stores may use the firewood reserve (a full barn helps nobody in winter), and so
        // may the cheap charcoal kiln (it only burns wood while the stock allows) and the
        // woodcutters and foresters that bring the firewood in.
        let reserve = if matches!(key, "storehouse" | "charcoal_kiln" | "woodcutter" | "forester") { 0 } else { reserve };
        state.economy().treasury() >= i64::from(d.cost.gold) + GOLD_RESERVE
            && (wood_cost == 0 || wood >= reserve + wood_cost)
            && others
    };
    // Workplaces only for adults that are there; two stay free as builders.
    let slots: u32 = state.buildings().iter().filter_map(|(_, b)| defs.buildings().get(b.kind().index())).map(|d| u32::from(d.workers)).sum();
    let adults = state.workforce().available();
    let workers = |key: &str| def(key).map_or(0, |d| u32::from(d.workers));
    let staffable = |key: &str| open(key) && affordable(key) && slots + workers(key) + 2 <= adults;
    // Daily food ahead of the population: an animal farm per 9 citizens.
    if count("chicken_farm") * 9 < pop + 4 && open("chicken_farm") && affordable("chicken_farm") && slots + workers("chicken_farm") <= adults {
        return Some("chicken_farm");
    }
    // These keep only one adult free as a builder.
    let extra_ok = |key: &str| open(key) && affordable(key) && slots + workers(key) < adults;
    // Firewood before the extras: a woodcutter per 12 citizens (at least two), each with a
    // forester replanting.
    if count("woodcutter") < (pop / 12).max(2) && extra_ok("woodcutter") {
        return Some("woodcutter");
    }
    if count("forester") < count("woodcutter") && extra_ok("forester") {
        return Some("forester");
    }
    // Vegetable farms only while they can still be sown this year (spring, start of summer).
    // Orchard trees are planted from spring to autumn; a wheat farm built too late for this
    // year's sowing claims its fields and sows next spring. The quarry supplies the stone of
    // the later buildings, the sawmill the planks of the sheep farm (limited order, see
    // `grow_town`).
    let sowing_time = clock.season() == Season::Spring || (clock.season() == Season::Summer && clock.day_of_season() <= 2);
    let order: [(&'static str, usize); 10] = [
        ("orchard", 1),
        ("wheat_farm", 1),
        ("vegetable_farm", 2),
        ("quarry", 1),
        ("charcoal_kiln", 1),
        ("sawmill", 1),
        ("sheep_farm", 1),
        ("storehouse", 1),
        ("warehouse", 1),
        ("vegetable_farm", 3),
    ];
    // The charcoal kiln only with three woodcutters; the sheep farm only once the sawmill
    // makes planks (the start planks are kept for the orchard and the wheat farm).
    let ready = |key: &str| match key {
        "vegetable_farm" => sowing_time,
        "charcoal_kiln" => count("woodcutter") >= 3,
        "sheep_farm" => count("sawmill") >= 1,
        _ => true,
    };
    if let Some((key, _)) = order.iter().find(|(key, n)| count(key) < *n && ready(key) && extra_ok(key)) {
        return Some(key);
    }
    // Firewood: a woodcutter per 10 citizens, each with a forester replanting.
    if count("woodcutter") * 10 < pop && staffable("woodcutter") {
        return Some("woodcutter");
    }
    if count("forester") < count("woodcutter") && staffable("forester") {
        return Some("forester");
    }
    // The harvest piles up in the barns: more food stores.
    let in_barns: u32 = report::agriculture_summary(state, defs).in_barns.iter().map(|a| a.amount).sum();
    if in_barns > 150 * (1 + count("storehouse") as u32) / 2
        && count("storehouse") * 20 < pop + 20
        && !site("storehouse")
        && staffable("storehouse")
    {
        return Some("storehouse");
    }
    // Room for newcomers (they bring the hands for the next workplaces).
    if free < 3 && !building_houses && affordable("house_wood") && open("house_wood") {
        return Some("house_wood");
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

/// Mature trees within `radius` of `p`.
fn trees_near(state: &GameState, p: IVec2, radius: i32) -> i32 {
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

/// The best spot for the town's building `kind` near `center` (rings `rings`) that an access
/// road of type `road` can connect, without building on fields: the first ring with a valid
/// spot, best `score` within it (with `all_rings` the best of all rings).
#[allow(clippy::too_many_arguments)]
pub(super) fn town_spot(
    state: &GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    center: IVec2,
    rings: std::ops::Range<i32>,
    all_rings: bool,
    road: RoadTypeId,
    score: impl Fn(IVec2) -> i32,
) -> Option<(IVec2, Rotation)> {
    let on_fields = |p: IVec2| state.agriculture().owner_of(p).is_some();
    let mut best: Option<(i32, IVec2, Rotation)> = None;
    for r in rings {
        for offset in spiral_ring(r) {
            let origin = center + offset;
            for rotation in Rotation::ALL {
                let check = check_placement(state, defs, kind, origin, rotation);
                if !check.ok || check.clearing.tiles() > 2 || check.footprint.tiles().any(on_fields) {
                    continue;
                }
                let Some(plan) = plan_access_road(state, defs, kind, origin, rotation, road) else { continue };
                if !plan.valid || plan.tiles.iter().any(|t| t.is_new && on_fields(t.pos)) {
                    continue;
                }
                let s = score(origin);
                if best.is_none_or(|(b, ..)| s > b) {
                    best = Some((s, origin, rotation));
                }
            }
        }
        if best.is_some() && !all_rings {
            break;
        }
    }
    best.map(|(_, o, r)| (o, r))
}

/// A valid spot found by [`find_spot`].
struct Spot {
    origin: IVec2,
    rect: TileRect,
    entrance: IVec2,
    cost: Cost,
}

fn find_spot(
    state: &GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    rotation: Rotation,
    site: Site,
    center: IVec2,
    reserved: &[TileRect],
) -> Option<Spot> {
    for allow_clearing in [false, true] {
        for offset in spiral(SEARCH_RADIUS) {
            let check = check_placement(state, defs, kind, center + offset, rotation);
            let blocked = !check.ok
                || check.entrance_status == EntranceStatus::Blocked
                || (!allow_clearing && check.clearing.tiles() > 0)
                || reserved.iter().any(|r| r.intersects(&check.footprint) || r.contains(check.entrance))
                || state.buildings().iter().any(|(_, b)| check.footprint.contains(b.entrance()))
                || (site == Site::TerraceEdge && !above_lower_ground(state, check.footprint));
            if !blocked {
                return Some(Spot { origin: check.origin, rect: check.footprint, entrance: check.entrance, cost: check.cost });
            }
        }
    }
    None
}

/// True if a tile right in front of the footprint (+x or +y side) lies lower than it.
fn above_lower_ground(state: &GameState, rect: TileRect) -> bool {
    let world = state.world();
    let level = rect.tiles().filter_map(|p| world.elevation(p)).max().unwrap_or(0);
    let max = rect.max_inclusive();
    let front_x = (rect.origin.y..=max.y).map(|y| IVec2::new(max.x + 1, y));
    let front_y = (rect.origin.x..=max.x).map(|x| IVec2::new(x, max.y + 1));
    front_x.chain(front_y).filter_map(|p| world.elevation(p)).any(|e| e < level)
}

/// Offsets in rings of growing Chebyshev distance (ring by ring, row-major within a ring).
fn spiral(radius: i32) -> impl Iterator<Item = IVec2> {
    (0..=radius).flat_map(spiral_ring)
}

/// Offsets at Chebyshev distance `r`, row-major.
fn spiral_ring(r: i32) -> impl Iterator<Item = IVec2> {
    (-r..=r).flat_map(move |dy| (-r..=r).map(move |dx| IVec2::new(dx, dy))).filter(move |d| d.x.abs().max(d.y.abs()) == r)
}

/// Centre tile of the first headquarters.
pub(super) fn headquarters_center(state: &GameState, defs: &Defs) -> Option<IVec2> {
    let id = state.buildings().with_role(defs, BuildingRole::Headquarters).into_iter().next()?;
    let rect = state.building(id)?.rect();
    Some(rect.origin + rect.size / 2)
}

/// Cheapest unlocked land road type.
pub(super) fn cheapest_road(state: &GameState, defs: &Defs) -> Option<RoadTypeId> {
    defs.roads()
        .iter()
        .filter(|r| r.bridge.is_none() && state.research().road_unlocked(defs, r.id))
        .min_by_key(|r| (r.cost_per_tile.gold, r.cost_per_tile.materials.len(), r.id))
        .map(|r| r.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_from_the_variable() {
        assert_eq!(DevPlan::parse("1"), Some(DevPlan::Showcase));
        assert_eq!(DevPlan::parse("yes"), Some(DevPlan::Showcase));
        assert_eq!(DevPlan::parse(" 2 "), Some(DevPlan::Town));
        assert_eq!(DevPlan::parse("Stadt"), Some(DevPlan::Town));
        assert_eq!(DevPlan::parse("Wirtschaft"), Some(DevPlan::Town));
        assert_eq!(DevPlan::parse("0"), None);
        assert_eq!(DevPlan::parse("off"), None);
    }

    #[test]
    fn spiral_covers_each_ring_once() {
        let offsets: Vec<IVec2> = spiral(3).collect();
        assert_eq!(offsets.len(), 49);
        assert_eq!(offsets[0], IVec2::ZERO);
        let mut sorted = offsets.clone();
        sorted.sort_by_key(|d| (d.y, d.x));
        sorted.dedup();
        assert_eq!(sorted.len(), 49);
        // Rings are visited in order of distance.
        let dist: Vec<i32> = offsets.iter().map(|d| d.x.abs().max(d.y.abs())).collect();
        assert!(dist.windows(2).all(|w| w[0] <= w[1]));
    }
}

#[cfg(test)]
mod town_tests {
    use std::sync::Arc;

    use kingdoms_sim::agriculture::FarmKind;
    use kingdoms_sim::population::DeathCause;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::time::TICKS_PER_DAY;
    use kingdoms_sim::world::WorldSize;

    use super::*;
    use crate::session::Session;

    fn defs() -> Arc<Defs> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => Arc::new(defs),
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    /// Two whole years (24 days each) and the first day of the third spring.
    const DAYS: u64 = 49;

    /// The town of `KINGDOMS_DEV_SETTLEMENT=2` over two years: it grows, nobody starves or
    /// freezes, and it farms vegetables, wheat and fruit. Workers and carriers walk, so the
    /// first year brings the basics, woodcutters and the orchard; the wheat farm, quarry,
    /// sawmill, sheep farm and charcoal kiln follow in the second year when adults and material
    /// allow. Random events are switched off: the script decides none, so their default
    /// choices (droughts, raids, fires) would make this a test of the event draws, which the
    /// simulation's balance runs cover with events on.
    #[test]
    fn town_grows_through_two_years() {
        let defs = defs();
        let settings = GameSettings::new(&defs, 7, WorldSize::Small).with_events(false);
        let mut session = Session::new(defs.clone(), settings);
        let start = session.state().population().len();
        session.start_dev_settlement(DevPlan::Town);
        let mut cursor = session.events().cursor_at_end();
        let mut deaths = Vec::new();
        let mut rejected = 0;
        for _ in 0..DAYS * TICKS_PER_DAY {
            session.tick();
            let read = session.events().read(&mut cursor);
            for e in read.events {
                match &e.event {
                    SimEvent::CitizenDied { cause, .. } => deaths.push(*cause),
                    SimEvent::CommandRejected { .. } => rejected += 1,
                    _ => {}
                }
            }
            let state = session.state();
            if state.clock().is_new_day() {
                let s = report::population_summary(state, &defs);
                let slots: u32 = state.buildings().iter().map(|(_, b)| u32::from(defs.building(b.kind()).workers)).sum();
                eprintln!(
                    "day {} {:?}: pop {} homeless {} free {} adults {} slots {} buildings {} gold {} wood {} planks {} goal {:?}",
                    state.clock().total_days(),
                    state.clock().season(),
                    s.population,
                    s.homeless,
                    s.immigrant_places,
                    state.workforce().available(),
                    slots,
                    state.buildings().len(),
                    state.economy().treasury(),
                    defs.resource_id("wood").map_or(0, |w| state.economy().stock().get(w)),
                    defs.resource_id("planks").map_or(0, |w| state.economy().stock().get(w)),
                    town_goal(state, &defs, &[]),
                );
            }
        }
        let state = session.state();
        let count = |key: &str| defs.building_id(key).map_or(0, |k| state.buildings().count_of(k));
        let keys = ["house_wood", "well", "market", "woodcutter", "forester", "vegetable_farm", "wheat_farm", "sheep_farm", "charcoal_kiln", "sawmill", "orchard", "storehouse", "warehouse", "chicken_farm"];
        let built: Vec<(&str, usize)> = keys.iter().map(|k| (*k, count(k))).collect();
        eprintln!("built {built:?}; deaths {deaths:?}; rejected {rejected}");
        let pop = state.population().len();
        assert!(pop >= start + 8, "the town grew from {start} to {pop}");
        assert!(
            deaths.iter().all(|c| !matches!(c, DeathCause::Starvation | DeathCause::Freezing)),
            "nobody starved or froze: {deaths:?}"
        );
        for key in ["well", "market", "woodcutter", "forester", "vegetable_farm", "wheat_farm", "orchard", "storehouse", "quarry"] {
            assert!(count(key) >= 1, "{key} built: {built:?}");
        }
        assert!(count("woodcutter") * 14 >= pop, "enough woodcutters for {pop} citizens: {built:?}");
        let fields = state.agriculture().farms().filter(|(_, f)| f.kind() == FarmKind::Fields && !f.tiles().is_empty()).count();
        assert!(fields >= 3, "vegetable, wheat and fruit farms have fields");
        assert_eq!(rejected, 0, "the simulation accepted every command");
        assert!(state.economy().treasury() > 0);
    }
}
