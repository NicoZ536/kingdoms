//! The walk-cost grid: one cost per tile, bridge axes, elevations and connected components.

use std::collections::BTreeMap;

use glam::IVec2;

use crate::buildings::{Buildings, carries_road};
use crate::defs::Defs;
use crate::diplomacy::LandMap;
use super::memory::{ChangeCollector, GridChange};
use crate::grid::{Axis, Direction, TileRect};
use crate::roads::RoadNetwork;
use crate::world::{Vegetation, WorldMap};

/// Cost value of a tile nobody can enter.
pub const BLOCKED: u16 = 0;

/// Bridge marker of a tile: none, or the axis a bridge runs along.
const NO_BRIDGE: u8 = 0;

fn bridge_code(axis: Axis) -> u8 {
    match axis {
        Axis::X => 1,
        Axis::Y => 2,
    }
}

fn step_code(d: Direction) -> u8 {
    bridge_code(d.axis())
}

/// Path cost of a road tile with walking speed multiplier `speed`, in permille (a path with
/// speed 1.0 costs 1000).
pub fn road_cost_permille(speed: f32) -> u16 {
    let cost = (1000.0 / f64::from(speed.max(0.01))).round();
    cost.clamp(1.0, f64::from(u16::MAX)) as u16
}

/// Walk cost of every tile plus the derived connected components.
#[derive(Clone, Debug, Default)]
pub(crate) struct NavGrid {
    width: i32,
    height: i32,
    cost: Vec<u16>,
    elevation: Vec<u8>,
    bridge: Vec<u8>,
    /// Component label per tile (0 = blocked); valid unless `components_dirty`.
    component: Vec<u32>,
    components_dirty: bool,
    /// Counts effective cost changes (path cache key).
    revision: u64,
    /// Number of enterable tiles per cost value (the lowest is the A* heuristic unit; a pure
    /// function of the grid, so searches expand the same nodes after loading).
    cost_counts: BTreeMap<u16, u32>,
    max_step: u8,
    slope_cost: u16,
}

impl NavGrid {
    /// Builds the grid for the whole map.
    pub(crate) fn build(world: &WorldMap, buildings: &Buildings, roads: &RoadNetwork, land: &LandMap, defs: &Defs) -> Self {
        let (width, height) = (world.width(), world.height());
        let len = (width.max(0) as usize) * (height.max(0) as usize);
        let walking = &defs.rules().walking;
        let mut grid = NavGrid {
            width,
            height,
            cost: vec![BLOCKED; len],
            elevation: vec![0; len],
            bridge: vec![NO_BRIDGE; len],
            component: vec![0; len],
            components_dirty: true,
            revision: 0,
            cost_counts: BTreeMap::new(),
            max_step: walking.max_step,
            slope_cost: walking.slope_cost_permille,
        };
        for y in 0..height {
            for x in 0..width {
                grid.refresh_tile(world, buildings, roads, land, defs, IVec2::new(x, y));
            }
        }
        grid.recompute_components();
        grid
    }

    /// True if the grid was built for a `width` × `height` map.
    pub(crate) fn fits(&self, width: i32, height: i32) -> bool {
        self.width == width && self.height == height && !self.cost.is_empty()
    }

    pub(crate) fn index(&self, p: IVec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height).then(|| (p.y * self.width + p.x) as usize)
    }

    pub(crate) fn pos(&self, index: usize) -> IVec2 {
        let i = index as i32;
        IVec2::new(i % self.width, i / self.width)
    }

    pub(crate) fn len(&self) -> usize {
        self.cost.len()
    }

    pub(crate) fn width(&self) -> i32 {
        self.width
    }

    pub(crate) fn height(&self) -> i32 {
        self.height
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Lowest cost of an enterable tile (1000 on a map without any).
    pub(crate) fn min_cost(&self) -> u16 {
        self.cost_counts.keys().next().copied().unwrap_or(1000)
    }

    /// Cost of entering `p` in permille (`None`: blocked or outside the map).
    pub(crate) fn cost(&self, p: IVec2) -> Option<u16> {
        self.index(p).map(|i| self.cost[i]).filter(|c| *c != BLOCKED)
    }

    /// Component of `p` (`None`: blocked, outside the map or components out of date).
    pub(crate) fn component(&self, p: IVec2) -> Option<u32> {
        if self.components_dirty {
            return None;
        }
        self.index(p).map(|i| self.component[i]).filter(|c| *c != 0)
    }

    /// Recomputes the tiles of `rects`; returns the rectangles of changed tiles (empty: no
    /// change). Connected components are marked out of date when passability or a bridge
    /// changed.
    pub(crate) fn update(
        &mut self,
        world: &WorldMap,
        buildings: &Buildings,
        roads: &RoadNetwork,
        land: &LandMap,
        defs: &Defs,
        rects: &[TileRect],
    ) -> Vec<GridChange> {
        let mut changes = Vec::new();
        for rect in rects {
            let r = rect.clipped(self.width, self.height);
            let mut collector = ChangeCollector::default();
            for p in r.tiles() {
                if let Some((cheaper, dearer)) = self.refresh_tile(world, buildings, roads, land, defs, p) {
                    collector.add(p, cheaper, dearer);
                }
            }
            changes.extend(collector.finish());
        }
        if !changes.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        changes
    }

    /// The changes from `old` to this grid (one rectangle around all changed tiles; empty if
    /// the grids are equal or of different size).
    pub(crate) fn diff(&self, old: &NavGrid) -> Vec<GridChange> {
        if !old.fits(self.width, self.height) || self.max_step != old.max_step || self.slope_cost != old.slope_cost {
            return Vec::new();
        }
        let mut collector = ChangeCollector::default();
        for i in 0..self.cost.len() {
            let (a, b) = ((old.cost[i], old.bridge[i], old.elevation[i]), (self.cost[i], self.bridge[i], self.elevation[i]));
            if a != b {
                let (cheaper, dearer) = classify(a, b);
                collector.add(self.pos(i), cheaper, dearer);
            }
        }
        collector.finish().into_iter().collect()
    }

    /// Fingerprint of everything a path depends on (size, rules, cost, bridge and elevation of
    /// every tile); FNV-1a, never 0.
    pub(crate) fn fingerprint(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        eat(&self.width.to_le_bytes());
        eat(&self.height.to_le_bytes());
        eat(&[self.max_step]);
        eat(&self.slope_cost.to_le_bytes());
        for i in 0..self.cost.len() {
            eat(&self.cost[i].to_le_bytes());
            eat(&[self.bridge[i], self.elevation[i]]);
        }
        h.max(1)
    }

    /// Recomputes one tile; returns whether it became cheaper and/or dearer if its cost,
    /// bridge or elevation changed.
    fn refresh_tile(
        &mut self,
        world: &WorldMap,
        buildings: &Buildings,
        roads: &RoadNetwork,
        land: &LandMap,
        defs: &Defs,
        p: IVec2,
    ) -> Option<(bool, bool)> {
        let i = self.index(p)?;
        let (cost, bridge) = tile_cost(world, buildings, roads, land, defs, p);
        let elevation = world.get(p).map_or(0, |t| t.elevation);
        let old = (self.cost[i], self.bridge[i], self.elevation[i]);
        if old == (cost, bridge, elevation) {
            return None;
        }
        if (old.0 == BLOCKED) != (cost == BLOCKED) || old.1 != bridge || old.2 != elevation {
            self.components_dirty = true;
        }
        if old.0 != cost {
            if old.0 != BLOCKED
                && let Some(n) = self.cost_counts.get_mut(&old.0)
            {
                *n -= 1;
                if *n == 0 {
                    self.cost_counts.remove(&old.0);
                }
            }
            if cost != BLOCKED {
                *self.cost_counts.entry(cost).or_insert(0) += 1;
            }
        }
        self.cost[i] = cost;
        self.bridge[i] = bridge;
        self.elevation[i] = elevation;
        Some(classify(old, (cost, bridge, elevation)))
    }

    /// Cost of the step from `p` in direction `d` (`None` if it is not allowed): the cost of
    /// the entered tile plus the slope cost for a level change. Bridges are entered, left and
    /// crossed only along their axis (no level check: the deck spans the banks); elsewhere the
    /// level difference must not exceed `max_step`. The start tile itself may be blocked.
    pub(crate) fn step_cost(&self, p: IVec2, d: Direction) -> Option<u32> {
        let pi = self.index(p)?;
        let qi = self.index(p + d.offset())?;
        let cost = self.cost[qi];
        if cost == BLOCKED {
            return None;
        }
        let (bp, bq) = (self.bridge[pi], self.bridge[qi]);
        if bp != NO_BRIDGE || bq != NO_BRIDGE {
            let axis = step_code(d);
            if (bp != NO_BRIDGE && bp != axis) || (bq != NO_BRIDGE && bq != axis) {
                return None;
            }
            return Some(u32::from(cost));
        }
        let dz = self.elevation[pi].abs_diff(self.elevation[qi]);
        if dz > self.max_step {
            return None;
        }
        Some(u32::from(cost) + if dz > 0 { u32::from(self.slope_cost) } else { 0 })
    }

    /// [`NavGrid::step_cost`] by tile index: the neighbour's index and the step's cost (the
    /// pathfinder's inner loop; `x` is the column of `i`).
    #[inline]
    pub(crate) fn step_from(&self, i: usize, x: i32, d: Direction) -> Option<(usize, u32)> {
        let w = self.width as usize;
        let j = match d {
            Direction::North => i.checked_sub(w)?,
            Direction::South => Some(i + w).filter(|j| *j < self.cost.len())?,
            Direction::East => (x + 1 < self.width).then_some(i + 1)?,
            Direction::West => (x > 0).then(|| i.wrapping_sub(1))?,
        };
        let cost = self.cost[j];
        if cost == BLOCKED {
            return None;
        }
        let (bp, bq) = (self.bridge[i], self.bridge[j]);
        if bp != NO_BRIDGE || bq != NO_BRIDGE {
            let axis = step_code(d);
            if (bp != NO_BRIDGE && bp != axis) || (bq != NO_BRIDGE && bq != axis) {
                return None;
            }
            return Some((j, u32::from(cost)));
        }
        let dz = self.elevation[i].abs_diff(self.elevation[j]);
        if dz > self.max_step {
            return None;
        }
        Some((j, u32::from(cost) + if dz > 0 { u32::from(self.slope_cost) } else { 0 }))
    }

    /// True while the connected components match the grid.
    pub(crate) fn components_known(&self) -> bool {
        !self.components_dirty
    }

    /// Relabels all connected components if they are out of date (flood fill over allowed
    /// steps; steps are symmetric, so components are well defined).
    pub(crate) fn ensure_components(&mut self) {
        if self.components_dirty {
            self.recompute_components();
        }
    }

    fn recompute_components(&mut self) {
        self.component.iter_mut().for_each(|c| *c = 0);
        let mut next = 1u32;
        let mut stack: Vec<usize> = Vec::new();
        for start in 0..self.cost.len() {
            if self.cost[start] == BLOCKED || self.component[start] != 0 {
                continue;
            }
            self.component[start] = next;
            stack.push(start);
            while let Some(i) = stack.pop() {
                let p = self.pos(i);
                for d in Direction::ALL {
                    if self.step_cost(p, d).is_none() {
                        continue;
                    }
                    if let Some(j) = self.index(p + d.offset())
                        && self.component[j] == 0
                    {
                        self.component[j] = next;
                        stack.push(j);
                    }
                }
            }
            next += 1;
        }
        self.components_dirty = false;
    }
}

/// Whether a tile changing from `old` to `new` (cost, bridge, elevation) became cheaper (or
/// enterable) and/or dearer (or blocked); a changed bridge or elevation counts as both.
fn classify(old: (u16, u8, u8), new: (u16, u8, u8)) -> (bool, bool) {
    if old.1 != new.1 || old.2 != new.2 {
        return (true, true);
    }
    let (a, b) = (old.0, new.0);
    let cheaper = b != BLOCKED && (a == BLOCKED || b < a);
    let dearer = a != BLOCKED && (b == BLOCKED || b > a);
    (cheaper, dearer)
}

/// True if a walker may enter tile `p` (used to validate loaded positions without a grid).
pub(crate) fn tile_enterable(world: &WorldMap, buildings: &Buildings, roads: &RoadNetwork, land: &LandMap, defs: &Defs, p: IVec2) -> bool {
    tile_cost(world, buildings, roads, land, defs, p).0 != BLOCKED
}

/// Walk cost and bridge marker of tile `p` from the world, buildings, roads and the structures
/// of AI settlements (docs/game_design.md §3, §5.6, §6, §10): buildings block except gates,
/// structures block, roads (and bridges) cost by their speed, water without a bridge and
/// boulders block, other tiles cost by terrain.
fn tile_cost(world: &WorldMap, buildings: &Buildings, roads: &RoadNetwork, land: &LandMap, defs: &Defs, p: IVec2) -> (u16, u8) {
    let Some(tile) = world.get(p) else { return (BLOCKED, NO_BRIDGE) };
    if land.has_structure(p) {
        return (BLOCKED, NO_BRIDGE);
    }
    if let Some((_, b)) = buildings.at(p)
        && !carries_road(defs, b)
    {
        return (BLOCKED, NO_BRIDGE);
    }
    if let Some(road) = roads.get(p) {
        let speed = defs.roads().get(road.road.index()).map_or(1.0, |r| r.speed);
        return (road_cost_permille(speed), road.bridge_axis.map_or(NO_BRIDGE, bridge_code));
    }
    if tile.terrain.is_water() || matches!(tile.vegetation, Vegetation::Boulder) {
        return (BLOCKED, NO_BRIDGE);
    }
    (defs.rules().walking.terrain_cost(tile.terrain).unwrap_or(BLOCKED), NO_BRIDGE)
}
