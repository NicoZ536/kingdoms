//! The road network: road grid, auto-connection masks, bridges, connectivity and planning.
//!
//! * Every tile holds at most one [`RoadTile`] (road type from `data/roads.ron`; bridges also
//!   store their axis because they only connect straight on).
//! * Masks use 4 bits N=1 (−y), E=2 (+x), S=4 (+y), W=8 (−x) and include connections into the
//!   entrance of adjacent buildings ([`road_mask`]).
//! * Connected components ([`RoadNetwork::component`]) answer reachability queries for
//!   logistics; they are derived data, rebuilt on load and after every change.
//! * [`plan_road`] finds a buildable route for the road drag tool (A* with slope, obstacle,
//!   territory and bridge rules) and prices it.

mod planner;

use std::collections::VecDeque;

use glam::IVec2;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::buildings::{BuildingId, Buildings};
use crate::defs::{Defs, RoadDef, RoadTypeId};
use crate::grid::{Axis, ChangeTracker, Direction, TileRect};
use crate::world::{MAX_MAP_EDGE, WorldMap};

pub use planner::{
    MAX_PLAN_TILES, PlannedRoadTile, RoadPlan, RoadProblem, plan_access_road, plan_road, validate_road_tiles,
};
pub(crate) use planner::{build_roads, remove_roads, route_between};

/// Road on one tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoadTile {
    /// Road type.
    pub road: RoadTypeId,
    /// Axis of a bridge tile (`None` on land). Bridges connect only along their axis.
    pub bridge_axis: Option<Axis>,
}

impl RoadTile {
    /// True for bridge tiles.
    pub fn is_bridge(&self) -> bool {
        self.bridge_axis.is_some()
    }

    /// Sprite for this tile given its connection mask: `roads/bridge_<x|y>` for bridges,
    /// otherwise `roads/<type>_<mask>`.
    pub fn sprite(&self, defs: &Defs, mask: u8) -> String {
        match self.bridge_axis {
            Some(axis) => format!("roads/bridge_{}", axis.sprite_suffix()),
            None => defs.roads().get(self.road.index()).map_or_else(|| format!("roads/path_{}", mask & 15), |d| d.sprite(mask)),
        }
    }
}

/// An inconsistent road grid in a damaged or crafted save.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum RoadGridError {
    /// The stored size is not positive or exceeds [`MAX_MAP_EDGE`].
    #[error("road grid size {width}x{height} is outside 1..={MAX_MAP_EDGE}")]
    InvalidSize {
        /// Stored width.
        width: i32,
        /// Stored height.
        height: i32,
    },
    /// The tile list does not match the declared size.
    #[error("road grid has {actual} tiles but the map has {expected}")]
    TileCountMismatch {
        /// Tiles required by the map size.
        expected: usize,
        /// Tiles stored.
        actual: usize,
    },
}

/// The road grid plus derived connectivity.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "RoadNetworkRepr")]
pub struct RoadNetwork {
    width: i32,
    height: i32,
    tiles: Vec<Option<RoadTile>>,
    #[serde(skip)]
    components: Vec<u32>,
    #[serde(skip)]
    component_count: u32,
    #[serde(skip)]
    count: u32,
    #[serde(skip)]
    changes: ChangeTracker,
}

/// Serialized form of [`RoadNetwork`] (same field order as its non-skipped fields).
#[derive(Deserialize)]
struct RoadNetworkRepr {
    width: i32,
    height: i32,
    tiles: Vec<Option<RoadTile>>,
}

impl TryFrom<RoadNetworkRepr> for RoadNetwork {
    type Error = RoadGridError;

    fn try_from(repr: RoadNetworkRepr) -> Result<Self, Self::Error> {
        if !(1..=MAX_MAP_EDGE).contains(&repr.width) || !(1..=MAX_MAP_EDGE).contains(&repr.height) {
            return Err(RoadGridError::InvalidSize { width: repr.width, height: repr.height });
        }
        // Both edges are at most MAX_MAP_EDGE, so the product cannot overflow.
        let expected = repr.width as usize * repr.height as usize;
        if repr.tiles.len() != expected {
            return Err(RoadGridError::TileCountMismatch { expected, actual: repr.tiles.len() });
        }
        let mut net = RoadNetwork {
            width: repr.width,
            height: repr.height,
            tiles: repr.tiles,
            components: Vec::new(),
            component_count: 0,
            count: 0,
            changes: ChangeTracker::default(),
        };
        net.rebuild_components();
        Ok(net)
    }
}

/// Component label of tiles without a road.
const NO_COMPONENT: u32 = u32::MAX;

impl RoadNetwork {
    /// No roads on a `width` × `height` map.
    pub fn new(width: i32, height: i32) -> Self {
        let len = (width.max(0) as usize) * (height.max(0) as usize);
        Self {
            width,
            height,
            tiles: vec![None; len],
            components: vec![NO_COMPONENT; len],
            component_count: 0,
            count: 0,
            changes: ChangeTracker::default(),
        }
    }

    /// Map size the grid covers.
    pub fn grid_size(&self) -> IVec2 {
        IVec2::new(self.width, self.height)
    }

    fn index(&self, p: IVec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height).then(|| (p.y * self.width + p.x) as usize)
    }

    /// Road at `p`.
    pub fn get(&self, p: IVec2) -> Option<RoadTile> {
        self.index(p).and_then(|i| self.tiles[i])
    }

    /// True if `p` holds a road (or bridge).
    pub fn has_road(&self, p: IVec2) -> bool {
        self.get(p).is_some()
    }

    /// Number of road tiles.
    pub fn len(&self) -> usize {
        self.count as usize
    }

    /// True if there are no roads.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// All road tiles in row-major order.
    pub fn iter(&self) -> impl Iterator<Item = (IVec2, RoadTile)> + '_ {
        let w = self.width.max(1);
        self.tiles
            .iter()
            .enumerate()
            .filter_map(move |(i, t)| t.map(|t| (IVec2::new(i as i32 % w, i as i32 / w), t)))
    }

    /// True if the roads on `p` and its neighbour in direction `d` connect (both are roads and
    /// a bridge on either side runs along `d`).
    pub fn links(&self, p: IVec2, d: Direction) -> bool {
        let (Some(a), Some(b)) = (self.get(p), self.get(p + d.offset())) else { return false };
        a.bridge_axis.is_none_or(|ax| ax == d.axis()) && b.bridge_axis.is_none_or(|ax| ax == d.axis())
    }

    /// Connection mask of `p` towards neighbouring roads only (no buildings).
    pub fn road_links_mask(&self, p: IVec2) -> u8 {
        Direction::ALL.iter().filter(|d| self.links(p, **d)).map(|d| d.mask_bit()).sum()
    }

    /// Connected-component label of the road at `p` (labels are only meaningful for comparing).
    pub fn component(&self, p: IVec2) -> Option<u32> {
        self.index(p).map(|i| self.components[i]).filter(|c| *c != NO_COMPONENT)
    }

    /// Number of separate road networks.
    pub fn component_count(&self) -> u32 {
        self.component_count
    }

    /// True if both tiles hold roads of the same connected network.
    pub fn connected(&self, a: IVec2, b: IVec2) -> bool {
        matches!((self.component(a), self.component(b)), (Some(x), Some(y)) if x == y)
    }

    /// Sets or clears a tile. Call [`RoadNetwork::rebuild_components`] after a batch of changes.
    pub(crate) fn set(&mut self, p: IVec2, tile: Option<RoadTile>) {
        if let Some(i) = self.index(p) {
            match (self.tiles[i].is_some(), tile.is_some()) {
                (false, true) => self.count += 1,
                (true, false) => self.count -= 1,
                _ => {}
            }
            if self.tiles[i] != tile {
                // Neighbours change their connection masks as well.
                self.changes.touch(TileRect::tile(p).expanded(1));
            }
            self.tiles[i] = tile;
        }
    }

    /// Change tracker of the road tiles (revision and touched chunks incl. neighbours).
    pub(crate) fn changes_mut(&mut self) -> &mut ChangeTracker {
        &mut self.changes
    }

    /// Number of road changes so far (not saved; starts at 0 after loading).
    pub fn revision(&self) -> u64 {
        self.changes.revision()
    }

    /// Recomputes connected components (breadth-first in row-major seed order, so labels are
    /// deterministic) and the tile count.
    pub(crate) fn rebuild_components(&mut self) {
        let len = self.tiles.len();
        self.components = vec![NO_COMPONENT; len];
        self.count = self.tiles.iter().filter(|t| t.is_some()).count() as u32;
        let mut next = 0;
        let mut queue = VecDeque::new();
        let w = self.width.max(1);
        for seed in 0..len {
            if self.tiles[seed].is_none() || self.components[seed] != NO_COMPONENT {
                continue;
            }
            self.components[seed] = next;
            queue.push_back(seed);
            while let Some(i) = queue.pop_front() {
                let p = IVec2::new(i as i32 % w, i as i32 / w);
                for d in Direction::ALL {
                    if !self.links(p, d) {
                        continue;
                    }
                    if let Some(j) = self.index(p + d.offset())
                        && self.components[j] == NO_COMPONENT
                    {
                        self.components[j] = next;
                        queue.push_back(j);
                    }
                }
            }
            next += 1;
        }
        self.component_count = next;
    }
}

/// Auto-connection mask of the road at `p` (N=1, E=2, S=4, W=8), including a connection into
/// an adjacent building whose entrance is `p` and reachable from it
/// ([`Building::entrance_reachable`](crate::buildings::Building::entrance_reachable)). `None`
/// if `p` has no road.
pub fn road_mask(world: &WorldMap, roads: &RoadNetwork, buildings: &Buildings, p: IVec2) -> Option<u8> {
    let tile = roads.get(p)?;
    let mut mask = roads.road_links_mask(p);
    if !tile.is_bridge() {
        for d in Direction::ALL {
            if let Some((_, b)) = buildings.at(p + d.offset())
                && b.entrance() == p
                && b.entrance_reachable(world)
            {
                mask |= d.mask_bit();
            }
        }
    }
    Some(mask)
}

/// True if a (non-bridge) road lies on the building's entrance tile and the entrance is at
/// most one elevation level away from the door.
pub fn has_road_access(world: &WorldMap, roads: &RoadNetwork, buildings: &Buildings, building: BuildingId) -> bool {
    buildings
        .get(building)
        .is_some_and(|b| roads.get(b.entrance()).is_some_and(|t| !t.is_bridge()) && b.entrance_reachable(world))
}

/// Road network component a building is attached to via its entrance.
pub fn building_component(
    world: &WorldMap,
    roads: &RoadNetwork,
    buildings: &Buildings,
    building: BuildingId,
) -> Option<u32> {
    if !has_road_access(world, roads, buildings, building) {
        return None;
    }
    buildings.get(building).and_then(|b| roads.component(b.entrance()))
}

/// True if both buildings have road access and their roads are connected.
pub fn buildings_connected(
    world: &WorldMap,
    roads: &RoadNetwork,
    buildings: &Buildings,
    a: BuildingId,
    b: BuildingId,
) -> bool {
    matches!(
        (building_component(world, roads, buildings, a), building_component(world, roads, buildings, b)),
        (Some(x), Some(y)) if x == y
    )
}

/// The first road type with bridge properties (used automatically for water tiles).
pub fn bridge_type(defs: &Defs) -> Option<&RoadDef> {
    defs.roads().iter().find(|r| r.bridge.is_some())
}

#[cfg(test)]
mod tests;
