//! How walkers get into a building (docs/game_design.md §5.5, §5.6, §7).
//!
//! Most buildings are entered through the door from the entrance tile in front of it
//! ([`Access::Entrance`]). Wall segments, gates and towers on a wall line ([`Access::AnySide`],
//! always 1×1) are also entered from any other walkable neighbour of their tile: in a closed
//! wall ring the entrance of a corner piece faces the next piece of wall, so carriers and
//! soldiers come in from outside (through a gate) instead.

use glam::IVec2;

use super::{Building, BuildingId};
use crate::defs::{Access, Defs};
use crate::grid::Direction;
use crate::state::GameState;

/// A tile a walker can enter a building from, and the step from there into the footprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessPoint {
    /// Walkable tile next to the footprint (the entrance for most buildings).
    pub tile: IVec2,
    /// Direction of the step from `tile` onto the door.
    pub into: Direction,
}

/// How walkers get into `building` (its type's [`Access`]).
pub fn access_of(defs: &Defs, building: &Building) -> Access {
    defs.buildings().get(building.kind().index()).map_or(Access::Entrance, |d| d.access)
}

/// The tiles walkers enter building `id` from, most preferred first: the entrance if it lies at
/// most one level from the door; for [`Access::AnySide`] the walkable ones of the entrance and
/// the other neighbours of the tile (north, east, south, west) that are no bridge and at most
/// one level from the door, or the entrance alone if none is walkable. Empty if the building
/// does not exist or its entrance cannot be used. Reads the walking grid as it stands.
pub fn access_points(state: &GameState, defs: &Defs, id: BuildingId) -> Vec<AccessPoint> {
    let Some(b) = state.buildings.get(id) else { return Vec::new() };
    let entrance = b.entrance_reachable(&state.world).then(|| AccessPoint { tile: b.entrance(), into: b.facing().opposite() });
    if access_of(defs, b) == Access::Entrance {
        return entrance.into_iter().collect();
    }
    let door = b.door();
    let level = state.world.get(door).map(|t| t.elevation);
    let usable = |p: IVec2| {
        state.navigation.is_walkable(p)
            && !state.roads.get(p).is_some_and(|t| t.is_bridge())
            && state.world.get(p).zip(level).is_some_and(|(t, l)| t.elevation.abs_diff(l) <= 1)
    };
    let mut points: Vec<AccessPoint> = entrance.into_iter().filter(|a| usable(a.tile)).collect();
    for d in Direction::ALL {
        let tile = door + d.offset();
        if tile != b.entrance() && usable(tile) {
            points.push(AccessPoint { tile, into: d.opposite() });
        }
    }
    if points.is_empty() {
        return entrance.into_iter().collect();
    }
    points
}

/// Navigation components of the access tiles of `id` (ascending, without repeats; see
/// [`access_points`]). Needs the components to be current (`navigation::prepare`).
pub(crate) fn access_components(state: &GameState, defs: &Defs, id: BuildingId) -> Vec<u32> {
    let mut components: Vec<u32> =
        access_points(state, defs, id).into_iter().filter_map(|a| state.navigation.component(a.tile)).collect();
    components.sort_unstable();
    components.dedup();
    components
}

/// True if `id` is entered from any side ([`Access::AnySide`]).
pub fn is_open_sided(state: &GameState, defs: &Defs, id: BuildingId) -> bool {
    state.buildings.get(id).is_some_and(|b| access_of(defs, b) == Access::AnySide)
}
