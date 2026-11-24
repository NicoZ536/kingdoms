//! Road planning for the drag tool, validation of road tile lists, building and removing roads.
//!
//! Rules (docs/game_design.md §3, §5.6):
//! * roads need owned, dry land without buildings or boulders (trees and bushes are cleared,
//!   yielding wood); gates are the exception: a road crosses a wall line on a gate tile,
//! * consecutive tiles may differ by at most one elevation level,
//! * water is crossed by straight bridges of the bridge road type (max. span and shallow-water
//!   restriction from its `BridgeDef`), bank to bank with at most one level difference;
//!   bridge tiles are priced with the bridge type's cost per water tile,
//! * dragging a road over an existing road upgrades it if the new type is faster, otherwise the
//!   existing road is kept at no cost.

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};
use std::mem::discriminant;

use glam::IVec2;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{RoadTile, bridge_type};
use crate::buildings::{carries_road, clear_vegetation};
use crate::command::{Command, CommandResult, RejectReason};
use crate::defs::{BridgeDef, BuildingTypeId, Cost, Defs, ResourceAmount, RoadTypeId};
use crate::economy::{LedgerCategory, Shortfall, storage};
use crate::event::SimEvent;
use crate::grid::{Axis, Direction, Rotation, TileRect};
use crate::state::GameState;
use crate::world::{Terrain, Vegetation};

/// Most tiles a single plan or `BuildRoad` command may contain.
pub const MAX_PLAN_TILES: usize = 4096;
/// The A* search for a drag covers the bounding box of start and end plus this margin.
const SEARCH_MARGIN: i32 = 16;
/// Search radius when looking for the nearest road from a building entrance.
const ACCESS_SEARCH_RADIUS: i32 = 32;

// A* step costs (integers keep the search deterministic and platform independent).
const COST_NEW: u32 = 10;
const COST_EXISTING: u32 = 4;
const COST_VEGETATION: u32 = 8;
const COST_SLOPE: u32 = 3;
const COST_TURN: u32 = 5;
const COST_BRIDGE_NEW: u32 = 40;
const COST_BRIDGE_EXISTING: u32 = 4;
/// Lower bound of the cost per tile advanced (keeps the heuristic admissible).
const MIN_STEP_COST: u32 = 4;
const NONE: u32 = u32::MAX;

/// Why a road (or part of it) cannot be built. The `Display` text is German.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum RoadProblem {
    /// The game is lost: nothing can be built any more.
    #[error("Das Spiel ist verloren.")]
    GameOver,
    /// The road type id does not exist.
    #[error("Unbekannter Straßentyp.")]
    UnknownRoadType,
    /// The road type needs research.
    #[error("Der Straßentyp „{name}“ ist noch nicht erforscht.")]
    NotUnlocked {
        /// German road type name.
        name: String,
    },
    /// The bridge type cannot be drawn directly.
    #[error("Brücken entstehen automatisch, wenn eine Straße über Wasser gezogen wird.")]
    BridgeTypeSelected,
    /// The data defines no bridge type.
    #[error("Brücken sind nicht verfügbar.")]
    NoBridgeType,
    /// No tiles were given.
    #[error("Keine Felder für die Straße angegeben.")]
    Empty,
    /// More tiles than [`MAX_PLAN_TILES`].
    #[error("Die Straße ist zu lang ({len} Felder, höchstens {max}).")]
    TooLong {
        /// Given tiles.
        len: usize,
        /// Allowed maximum.
        max: usize,
    },
    /// Two consecutive tiles are not neighbours.
    #[error("Die Straße ist bei {pos} unterbrochen.")]
    NotContiguous {
        /// First tile after the gap.
        pos: IVec2,
    },
    /// A tile appears twice.
    #[error("Feld {pos} kommt mehrfach vor.")]
    Repeated {
        /// The repeated tile.
        pos: IVec2,
    },
    /// Outside the map.
    #[error("Feld {pos} liegt außerhalb der Karte.")]
    OutOfBounds {
        /// The tile.
        pos: IVec2,
    },
    /// Not owned by the kingdom.
    #[error("Feld {pos} liegt außerhalb des eigenen Gebiets.")]
    OutsideTerritory {
        /// The tile.
        pos: IVec2,
    },
    /// A building stands there.
    #[error("Auf Feld {pos} steht ein Gebäude.")]
    Building {
        /// The tile.
        pos: IVec2,
    },
    /// A boulder blocks the tile.
    #[error("Ein Felsbrocken versperrt Feld {pos}.")]
    Obstacle {
        /// The tile.
        pos: IVec2,
    },
    /// Bridges cannot cross deep water.
    #[error("Über tiefes Wasser kann keine Brücke gebaut werden ({pos}).")]
    DeepWater {
        /// The tile.
        pos: IVec2,
    },
    /// A bridge must start and end on land.
    #[error("Eine Brücke braucht an beiden Enden festes Ufer ({pos}).")]
    BridgeNeedsBanks {
        /// First water tile of the bridge.
        pos: IVec2,
    },
    /// Bridges must be straight.
    #[error("Brücken müssen gerade verlaufen ({pos}).")]
    BridgeNotStraight {
        /// A tile of the bridge.
        pos: IVec2,
    },
    /// Bridge longer than allowed.
    #[error("Die Brücke bei {pos} ist zu lang ({span} Felder, höchstens {max}).")]
    BridgeTooLong {
        /// First water tile of the bridge.
        pos: IVec2,
        /// Water tiles crossed.
        span: usize,
        /// Allowed maximum.
        max: u8,
    },
    /// More than one elevation level between neighbouring road tiles (or bridge banks).
    #[error("Zu steil bei {pos}: höchstens eine Höhenstufe Unterschied.")]
    TooSteep {
        /// The higher/later tile.
        pos: IVec2,
    },
    /// The planner found no buildable route.
    #[error("Kein gültiger Weg gefunden.")]
    NoRoute,
    /// The treasury is bankrupt: only free roads can be built.
    #[error("Die Staatskasse ist bankrott – nur kostenlose Wege können angelegt werden.")]
    Bankrupt,
    /// Not enough gold.
    #[error("Nicht genug Gold ({available} von {needed}).")]
    InsufficientGold {
        /// Required.
        needed: u32,
        /// In the treasury.
        available: i64,
    },
    /// Not enough of a material.
    #[error("Nicht genug {name} ({available} von {needed}).")]
    InsufficientMaterial {
        /// German resource name.
        name: String,
        /// Required.
        needed: u32,
        /// In stock.
        available: u32,
    },
}

/// One tile of a road plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedRoadTile {
    /// The tile.
    pub pos: IVec2,
    /// Road type the tile will have (the bridge type on water).
    pub road: RoadTypeId,
    /// Bridge axis for water tiles.
    pub bridge_axis: Option<Axis>,
    /// True if something is built here (and paid for); false if a road that is kept is there.
    pub is_new: bool,
    /// False if this tile violates a rule (draw it red).
    pub ok: bool,
}

/// Result of planning or validating a road.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoadPlan {
    /// Road type for land tiles.
    pub road: RoadTypeId,
    /// Tiles from start to end.
    pub tiles: Vec<PlannedRoadTile>,
    /// Total price of the new tiles.
    pub cost: Cost,
    /// True if the road can be built as planned.
    pub valid: bool,
    /// Rule violations (at most one per kind, in check order).
    pub problems: Vec<RoadProblem>,
}

impl RoadPlan {
    /// Tile positions in order.
    pub fn positions(&self) -> Vec<IVec2> {
        self.tiles.iter().map(|t| t.pos).collect()
    }

    /// Number of tiles that will be built or upgraded.
    pub fn new_tile_count(&self) -> usize {
        self.tiles.iter().filter(|t| t.is_new).count()
    }

    /// The command that builds this plan (`None` if the plan is invalid).
    pub fn to_command(&self) -> Option<Command> {
        self.valid.then(|| Command::BuildRoad { tiles: self.positions(), road: self.road })
    }
}

/// Read-only view of the map for road rules.
struct RoadRules<'a> {
    state: &'a GameState,
    defs: &'a Defs,
    bridge: Option<(RoadTypeId, BridgeDef)>,
    /// Extra blocked area (the footprint of a building being placed).
    blocked: Option<TileRect>,
}

impl<'a> RoadRules<'a> {
    fn new(state: &'a GameState, defs: &'a Defs, blocked: Option<TileRect>) -> Self {
        let bridge = bridge_type(defs).and_then(|b| b.bridge.map(|d| (b.id, d)));
        Self { state, defs, bridge, blocked }
    }

    /// True if a building (other than a gate, which roads pass through) or the blocked area
    /// covers `p`.
    fn built_over(&self, p: IVec2) -> bool {
        self.state.buildings.at(p).is_some_and(|(_, b)| !carries_road(self.defs, b))
            || self.blocked.is_some_and(|r| r.contains(p))
    }

    fn is_water(&self, p: IVec2) -> bool {
        self.state.world.get(p).is_some_and(|t| t.is_water())
    }

    fn elevation(&self, p: IVec2) -> i32 {
        self.state.world.get(p).map_or(0, |t| i32::from(t.elevation))
    }

    /// Checks a land tile (not the slope).
    fn land_ok(&self, p: IVec2) -> Result<(), RoadProblem> {
        let Some(tile) = self.state.world.get(p) else { return Err(RoadProblem::OutOfBounds { pos: p }) };
        if !self.state.territory.contains(p) {
            return Err(RoadProblem::OutsideTerritory { pos: p });
        }
        if self.built_over(p) {
            return Err(RoadProblem::Building { pos: p });
        }
        if tile.vegetation == Vegetation::Boulder {
            return Err(RoadProblem::Obstacle { pos: p });
        }
        Ok(())
    }

    /// Checks a water tile crossed by a bridge along `axis`.
    fn water_ok(&self, p: IVec2, axis: Axis) -> Result<(), RoadProblem> {
        let Some(tile) = self.state.world.get(p) else { return Err(RoadProblem::OutOfBounds { pos: p }) };
        let Some((_, bridge)) = self.bridge else { return Err(RoadProblem::NoBridgeType) };
        if bridge.shallow_water_only && tile.terrain == Terrain::DeepWater {
            return Err(RoadProblem::DeepWater { pos: p });
        }
        if !self.state.territory.contains(p) {
            return Err(RoadProblem::OutsideTerritory { pos: p });
        }
        if self.built_over(p) {
            return Err(RoadProblem::Building { pos: p });
        }
        if let Some(existing) = self.state.roads.get(p)
            && existing.bridge_axis != Some(axis)
        {
            return Err(RoadProblem::BridgeNotStraight { pos: p });
        }
        Ok(())
    }

    /// A* cost of entering land tile `p`.
    fn land_cost(&self, p: IVec2) -> u32 {
        let existing = self.state.roads.get(p).is_some_and(|t| !t.is_bridge());
        let vegetation = self.state.world.get(p).is_some_and(|t| matches!(t.vegetation, Vegetation::Tree { .. } | Vegetation::Bush));
        let base = if existing { COST_EXISTING } else { COST_NEW };
        if vegetation && !existing { base + COST_VEGETATION } else { base }
    }

    fn bridge_cost(&self, p: IVec2) -> u32 {
        if self.state.roads.has_road(p) { COST_BRIDGE_EXISTING } else { COST_BRIDGE_NEW }
    }
}

/// What the search is looking for.
#[derive(Clone, Copy)]
enum Goal {
    Tile(IVec2),
    /// Any land road tile.
    AnyRoad,
}

/// A* over (tile, arrival direction) inside `area`. Returns the tiles from `from` to the goal
/// with bridge axes for water tiles.
fn search(rules: &RoadRules<'_>, from: IVec2, goal: Goal, area: TileRect) -> Option<Vec<(IVec2, Option<Axis>)>> {
    let world = &rules.state.world;
    let area = area.clipped(world.width(), world.height());
    if !area.contains(from) || rules.is_water(from) || rules.land_ok(from).is_err() {
        return None;
    }
    if let Goal::Tile(t) = goal
        && (!area.contains(t) || rules.is_water(t) || rules.land_ok(t).is_err())
    {
        return None;
    }
    let w = area.size.x;
    let local = |p: IVec2| -> Option<u32> {
        area.contains(p).then(|| ((p.y - area.origin.y) * w + (p.x - area.origin.x)) as u32)
    };
    let tile_of = |state: u32| {
        let li = (state / 5) as i32;
        area.origin + IVec2::new(li % w, li / w)
    };
    let h = |p: IVec2| match goal {
        Goal::Tile(t) => MIN_STEP_COST * ((p - t).abs().element_sum() as u32),
        Goal::AnyRoad => 0,
    };
    let states = area.area() as usize * 5;
    let mut g = vec![NONE; states];
    let mut parent = vec![NONE; states];
    let mut jump = vec![0u8; states];
    let mut heap = BinaryHeap::new();
    let start = local(from)? * 5 + 4;
    g[start as usize] = 0;
    heap.push(Reverse((h(from), h(from), start)));
    let max_span = rules.bridge.map_or(0, |(_, b)| i32::from(b.max_span));

    while let Some(Reverse((f, hs, s))) = heap.pop() {
        let gs = f - hs;
        if gs > g[s as usize] {
            continue;
        }
        let p = tile_of(s);
        let reached = match goal {
            Goal::Tile(t) => p == t,
            Goal::AnyRoad => rules.state.roads.get(p).is_some_and(|r| !r.is_bridge()),
        };
        if reached {
            return Some(reconstruct(s, &parent, &jump, tile_of));
        }
        let slot = s % 5;
        let ep = rules.elevation(p);
        for d in Direction::ALL {
            let turn = if slot < 4 && slot != d.index() as u32 { COST_TURN } else { 0 };
            let q = p + d.offset();
            if !area.contains(q) {
                continue;
            }
            let mut relax = |target: IVec2, cost: u32, k: u8, g: &mut [u32]| {
                let Some(li) = local(target) else { return };
                let ns = li * 5 + d.index() as u32;
                let ng = gs + cost;
                if ng < g[ns as usize] {
                    g[ns as usize] = ng;
                    parent[ns as usize] = s;
                    jump[ns as usize] = k;
                    heap.push(Reverse((ng + h(target), h(target), ns)));
                }
            };
            if !rules.is_water(q) {
                let dz = (rules.elevation(q) - ep).abs();
                if dz <= 1 && rules.land_ok(q).is_ok() {
                    let cost = rules.land_cost(q) + turn + if dz == 1 { COST_SLOPE } else { 0 };
                    relax(q, cost, 0, &mut g);
                }
                continue;
            }
            // Straight bridge over 1..=max_span water tiles to the next land tile.
            let mut bridge_cost = 0;
            for k in 1..=max_span {
                let wtile = p + d.offset() * k;
                if !area.contains(wtile) || !rules.is_water(wtile) || rules.water_ok(wtile, d.axis()).is_err() {
                    break;
                }
                bridge_cost += rules.bridge_cost(wtile);
                let landing = p + d.offset() * (k + 1);
                if !area.contains(landing) {
                    break;
                }
                if rules.is_water(landing) {
                    continue;
                }
                if (rules.elevation(landing) - ep).abs() <= 1 && rules.land_ok(landing).is_ok() {
                    let cost = bridge_cost + rules.land_cost(landing) + turn;
                    relax(landing, cost, k as u8, &mut g);
                }
                break;
            }
        }
    }
    None
}

fn reconstruct(
    goal: u32,
    parent: &[u32],
    jump: &[u8],
    tile_of: impl Fn(u32) -> IVec2,
) -> Vec<(IVec2, Option<Axis>)> {
    let mut out = Vec::new();
    let mut s = goal;
    loop {
        let p = tile_of(s);
        out.push((p, None));
        let k = i32::from(jump[s as usize]);
        if k > 0 {
            let d = Direction::ALL[(s % 5) as usize];
            for i in 1..=k {
                out.push((p - d.offset() * i, Some(d.axis())));
            }
        }
        let prev = parent[s as usize];
        if prev == NONE {
            break;
        }
        s = prev;
    }
    out.reverse();
    out
}

/// L-shaped line (first along x, then along y) used to show an unbuildable drag.
fn l_line(from: IVec2, to: IVec2) -> Vec<IVec2> {
    let mut out = vec![from];
    let mut p = from;
    while p.x != to.x {
        p.x += (to.x - p.x).signum();
        out.push(p);
    }
    while p.y != to.y {
        p.y += (to.y - p.y).signum();
        out.push(p);
    }
    out
}

fn push_problem(problems: &mut Vec<RoadProblem>, problem: RoadProblem) {
    if !problems.iter().any(|p| discriminant(p) == discriminant(&problem)) {
        problems.push(problem);
    }
}

/// Marks tile `i` as violating a rule and records the problem.
fn flag(i: usize, problem: RoadProblem, problems: &mut Vec<RoadProblem>, bad: &mut [bool]) {
    bad[i] = true;
    push_problem(problems, problem);
}

fn add_cost(acc: &mut Cost, add: &Cost) {
    acc.gold = acc.gold.saturating_add(add.gold);
    for m in &add.materials {
        match acc.materials.binary_search_by_key(&m.resource, |a| a.resource) {
            Ok(i) => acc.materials[i].amount = acc.materials[i].amount.saturating_add(m.amount),
            Err(i) => acc.materials.insert(i, ResourceAmount { resource: m.resource, amount: m.amount }),
        }
    }
}

fn shortfall_problem(defs: &Defs, s: Shortfall) -> RoadProblem {
    match s {
        Shortfall::Gold { needed, available } => RoadProblem::InsufficientGold { needed, available },
        Shortfall::Material { resource, needed, available } => RoadProblem::InsufficientMaterial {
            name: defs.resources().get(resource.index()).map_or_else(|| resource.to_string(), |r| r.name.clone()),
            needed,
            available,
        },
    }
}

/// Validates an explicit list of road tiles (the `BuildRoad` command and the drag preview):
/// type, research, contiguity, per-tile rules, slopes, bridges and cost.
pub fn validate_road_tiles(state: &GameState, defs: &Defs, tiles: &[IVec2], road: RoadTypeId) -> RoadPlan {
    let mut problems = Vec::new();
    let mut plan = RoadPlan { road, tiles: Vec::new(), cost: Cost::default(), valid: false, problems: Vec::new() };
    let Some(def) = defs.roads().get(road.index()) else {
        plan.problems.push(RoadProblem::UnknownRoadType);
        return plan;
    };
    if state.progress.game_over().is_some() {
        push_problem(&mut problems, RoadProblem::GameOver);
    }
    if def.bridge.is_some() {
        push_problem(&mut problems, RoadProblem::BridgeTypeSelected);
    }
    if !state.research.road_unlocked(defs, road) {
        push_problem(&mut problems, RoadProblem::NotUnlocked { name: def.name.clone() });
    }
    if tiles.is_empty() {
        push_problem(&mut problems, RoadProblem::Empty);
    }
    if tiles.len() > MAX_PLAN_TILES {
        plan.problems = problems;
        plan.problems.push(RoadProblem::TooLong { len: tiles.len(), max: MAX_PLAN_TILES });
        return plan;
    }
    let rules = RoadRules::new(state, defs, None);
    let n = tiles.len();
    let mut bad = vec![false; n];

    let mut seen = BTreeSet::new();
    for (i, p) in tiles.iter().enumerate() {
        if !seen.insert((p.x, p.y)) {
            flag(i, RoadProblem::Repeated { pos: *p }, &mut problems, &mut bad);
        }
        if i > 0 && Direction::from_step(*p - tiles[i - 1]).is_none() {
            flag(i, RoadProblem::NotContiguous { pos: *p }, &mut problems, &mut bad);
        }
    }

    // Bridges: maximal runs of consecutive water tiles.
    let mut axes: Vec<Option<Axis>> = vec![None; n];
    let mut i = 0;
    while i < n {
        if !rules.is_water(tiles[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && rules.is_water(tiles[i]) {
            i += 1;
        }
        let end = i; // run is start..end, banks at start-1 and end
        let mark_run = |problem: RoadProblem, problems: &mut Vec<RoadProblem>, bad: &mut [bool]| {
            for b in bad.iter_mut().take(end).skip(start) {
                *b = true;
            }
            push_problem(problems, problem);
        };
        if start == 0 || end == n {
            mark_run(RoadProblem::BridgeNeedsBanks { pos: tiles[start] }, &mut problems, &mut bad);
            continue;
        }
        let dir = Direction::from_step(tiles[start] - tiles[start - 1]);
        let straight = dir.is_some() && (start..=end).all(|j| Direction::from_step(tiles[j] - tiles[j - 1]) == dir);
        let Some(dir) = dir.filter(|_| straight) else {
            mark_run(RoadProblem::BridgeNotStraight { pos: tiles[start] }, &mut problems, &mut bad);
            continue;
        };
        let span = end - start;
        if let Some((_, bridge)) = rules.bridge
            && span > usize::from(bridge.max_span)
        {
            mark_run(RoadProblem::BridgeTooLong { pos: tiles[start], span, max: bridge.max_span }, &mut problems, &mut bad);
            continue;
        }
        if (rules.elevation(tiles[start - 1]) - rules.elevation(tiles[end])).abs() > 1 {
            mark_run(RoadProblem::TooSteep { pos: tiles[end] }, &mut problems, &mut bad);
            continue;
        }
        for axis in axes.iter_mut().take(end).skip(start) {
            *axis = Some(dir.axis());
        }
    }

    // Per-tile rules and slopes between consecutive land tiles.
    for (i, &p) in tiles.iter().enumerate() {
        let result = if rules.is_water(p) {
            match axes[i] {
                Some(axis) => rules.water_ok(p, axis),
                None => Ok(()), // the run itself is already reported
            }
        } else {
            rules.land_ok(p)
        };
        if let Err(problem) = result {
            flag(i, problem, &mut problems, &mut bad);
        }
        if i > 0 {
            let q = tiles[i - 1];
            if state.world.in_bounds(p)
                && state.world.in_bounds(q)
                && !rules.is_water(p)
                && !rules.is_water(q)
                && (rules.elevation(p) - rules.elevation(q)).abs() > 1
            {
                flag(i, RoadProblem::TooSteep { pos: p }, &mut problems, &mut bad);
            }
        }
    }

    // Planned tiles and cost.
    let bridge_def = rules.bridge.and_then(|(id, _)| defs.roads().get(id.index()));
    for (i, &p) in tiles.iter().enumerate() {
        let existing = state.roads.get(p);
        let (tile_road, is_new, tile_cost) = if rules.is_water(p) {
            let id = bridge_def.map_or(road, |b| b.id);
            (id, existing.is_none(), bridge_def.map(|b| &b.cost_per_tile))
        } else {
            match existing {
                Some(e) if !e.is_bridge() => {
                    let old_speed = defs.roads().get(e.road.index()).map_or(0.0, |r| r.speed);
                    if def.speed > old_speed { (road, true, Some(&def.cost_per_tile)) } else { (e.road, false, None) }
                }
                _ => (road, true, Some(&def.cost_per_tile)),
            }
        };
        if is_new && let Some(c) = tile_cost {
            add_cost(&mut plan.cost, c);
        }
        plan.tiles.push(PlannedRoadTile { pos: p, road: tile_road, bridge_axis: axes[i], is_new, ok: !bad[i] });
    }
    if problems.is_empty() {
        let costs_something = plan.cost.gold > 0 || !plan.cost.materials.is_empty();
        if costs_something && state.economy.is_bankrupt() {
            push_problem(&mut problems, RoadProblem::Bankrupt);
        }
        for s in state.economy.shortfalls(&plan.cost) {
            push_problem(&mut problems, shortfall_problem(defs, s));
        }
    }
    plan.valid = problems.is_empty();
    plan.problems = problems;
    plan
}

/// Plans a road from `from` to `to` for the drag tool: the cheapest buildable route (avoiding
/// buildings, boulders, foreign territory, deep water and steep steps; few turns; bridges over
/// shallow water), priced and validated. If no route exists, the plan contains an L-shaped line
/// with the offending tiles marked (`ok == false`) and [`RoadProblem::NoRoute`] first.
pub fn plan_road(state: &GameState, defs: &Defs, from: IVec2, to: IVec2, road: RoadTypeId) -> RoadPlan {
    let rules = RoadRules::new(state, defs, None);
    let area = TileRect::spanning(from, to).expanded(SEARCH_MARGIN);
    match search(&rules, from, Goal::Tile(to), area) {
        Some(route) => {
            let tiles: Vec<IVec2> = route.iter().map(|(p, _)| *p).collect();
            validate_road_tiles(state, defs, &tiles, road)
        }
        None => {
            let mut plan = validate_road_tiles(state, defs, &l_line(from, to), road);
            plan.problems.insert(0, RoadProblem::NoRoute);
            plan.valid = false;
            plan
        }
    }
}

/// Suggests a road from the entrance of a building that would be placed at `origin` with
/// `rotation` to the nearest existing road (docs §5.6: "beim Platzieren wird automatisch ein
/// Pfad … vorgeschlagen"). Returns `None` if the type is unknown or no road is reachable within
/// 32 tiles; a plan without new tiles if the entrance already touches a road.
pub fn plan_access_road(
    state: &GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    origin: IVec2,
    rotation: Rotation,
    road: RoadTypeId,
) -> Option<RoadPlan> {
    let def = defs.buildings().get(kind.index())?;
    let footprint = rotation.footprint(origin, def.size);
    let entrance = rotation.entrance(origin, def.size);
    let rules = RoadRules::new(state, defs, Some(footprint));
    let area = TileRect::tile(entrance).expanded(ACCESS_SEARCH_RADIUS);
    let route = search(&rules, entrance, Goal::AnyRoad, area)?;
    let tiles: Vec<IVec2> = route.iter().map(|(p, _)| *p).collect();
    Some(validate_road_tiles(state, defs, &tiles, road))
}

/// Cheapest buildable route between two land tiles (ignoring cost and research), treating
/// `blocked` as occupied. Used to lay the scenario's start roads.
pub(crate) fn route_between(
    state: &GameState,
    defs: &Defs,
    from: IVec2,
    to: IVec2,
    blocked: Option<TileRect>,
) -> Option<Vec<(IVec2, Option<Axis>)>> {
    let rules = RoadRules::new(state, defs, blocked);
    search(&rules, from, Goal::Tile(to), TileRect::spanning(from, to).expanded(SEARCH_MARGIN))
}

/// Applies a `BuildRoad` command: validates, pays, clears trees/bushes (wood goes to the stock)
/// and sets the road tiles.
pub(crate) fn build_roads(
    state: &mut GameState,
    defs: &Defs,
    tiles: &[IVec2],
    road: RoadTypeId,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let plan = validate_road_tiles(state, defs, tiles, road);
    if let Some(problem) = plan.problems.first() {
        return Err(RejectReason::Road(problem.clone()));
    }
    if let Some(missing) = state.economy.shortfalls(&plan.cost).first() {
        return Err(RejectReason::Road(shortfall_problem(defs, *missing)));
    }
    let near = plan.tiles.first().map_or(state.world.start, |t| t.pos);
    if let Err(missing) = storage::withdraw_materials(state, defs, &plan.cost.materials, near) {
        let problem = missing.first().map_or(RoadProblem::NoRoute, |s| shortfall_problem(defs, *s));
        return Err(RejectReason::Road(problem));
    }
    if plan.cost.gold > 0
        && let Err(missing) = state.economy.pay_gold(LedgerCategory::Construction, plan.cost.gold)
    {
        return Err(RejectReason::Road(shortfall_problem(defs, missing)));
    }
    let mut wood = 0;
    let mut built = Vec::new();
    for t in plan.tiles.iter().filter(|t| t.is_new) {
        if t.bridge_axis.is_none() {
            wood += clear_vegetation(&mut state.world, TileRect::tile(t.pos)).wood(defs.rules());
        }
        state.roads.set(t.pos, Some(RoadTile { road: t.road, bridge_axis: t.bridge_axis }));
        built.push(t.pos);
    }
    if !built.is_empty() {
        state.roads.rebuild_components();
        events.push(SimEvent::RoadsBuilt { road, tiles: built });
    }
    if let Some(wood_id) = defs.resource_id("wood")
        && wood > 0
    {
        storage::deposit_or_lose(state, defs, &[ResourceAmount { resource: wood_id, amount: wood }], near, events);
    }
    storage::refresh_stock(state, defs);
    Ok(())
}

/// Applies a `RemoveRoad` command. Removing any tile of a bridge removes the whole bridge;
/// a bridge somebody is on (standing, or stepping onto or off it) is not removed — without its
/// deck the walker would stand on water it cannot leave.
pub(crate) fn remove_roads(state: &mut GameState, tiles: &[IVec2], events: &mut Vec<SimEvent>) -> CommandResult {
    let mut doomed: Vec<IVec2> = Vec::new();
    let mut bridges: Vec<IVec2> = Vec::new();
    for &p in tiles.iter().take(MAX_PLAN_TILES) {
        let Some(tile) = state.roads.get(p) else { continue };
        doomed.push(p);
        if let Some(axis) = tile.bridge_axis {
            bridges.push(p);
            let dirs = match axis {
                Axis::X => [Direction::East, Direction::West],
                Axis::Y => [Direction::North, Direction::South],
            };
            for d in dirs {
                let mut q = p + d.offset();
                while state.roads.get(q).is_some_and(|t| t.bridge_axis == Some(axis)) {
                    doomed.push(q);
                    bridges.push(q);
                    q += d.offset();
                }
            }
        }
    }
    doomed.sort_by_key(|p| (p.y, p.x));
    doomed.dedup();
    if doomed.is_empty() {
        return Err(RejectReason::NoRoadToRemove);
    }
    bridges.sort_by_key(|p| (p.y, p.x));
    bridges.dedup();
    if let Some(pos) = crate::population::movement::walker_on(state, &bridges) {
        return Err(RejectReason::BridgeInUse { pos });
    }
    for &p in &doomed {
        state.roads.set(p, None);
    }
    state.roads.rebuild_components();
    events.push(SimEvent::RoadsRemoved { tiles: doomed });
    Ok(())
}
