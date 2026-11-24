//! Applies a scenario's starting conditions to a freshly generated world
//! (docs/game_design.md §13): research, treasury, stock, settlers, the headquarters at the
//! world's start point, the other start buildings nearby (finished) and a path network
//! connecting their entrances to the headquarters.

use glam::IVec2;

use crate::buildings::{self, BuildingId, PlacementOptions, Rotation, check_placement_with};
use crate::defs::{BuildingTypeId, Defs, RoadDef, ScenarioId};
use crate::economy::storage;
use crate::grid::TileRect;
use crate::roads::{RoadTile, bridge_type, route_between};
use crate::state::GameState;
use crate::{population, workforce};

/// How far from its preferred spot a start building may be moved.
pub(crate) const START_SEARCH_RADIUS: i32 = 24;

/// Offsets ordered by Chebyshev ring, row-major within a ring (deterministic spiral).
pub(crate) fn spiral(radius: i32) -> impl Iterator<Item = IVec2> {
    (0..=radius).flat_map(move |r| {
        (-r..=r).flat_map(move |dy| (-r..=r).map(move |dx| IVec2::new(dx, dy))).filter(move |d| d.x.abs().max(d.y.abs()) == r)
    })
}

/// Cheapest unlocked road type without bridge properties (the start path).
fn start_road_type<'a>(state: &GameState, defs: &'a Defs) -> Option<&'a RoadDef> {
    defs.roads()
        .iter()
        .filter(|r| r.bridge.is_none() && state.research.road_unlocked(defs, r.id))
        .min_by_key(|r| (r.cost_per_tile.gold, r.cost_per_tile.materials.len(), r.id))
}

/// Finds an origin near `preferred` where the building fits (first without clearing
/// vegetation, then with), its entrance is usable, it covers no other building's entrance and,
/// if `connect_to` is given, a road can link its entrance to that tile.
fn find_spot(
    state: &GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    preferred: IVec2,
    rotation: Rotation,
    is_headquarters: bool,
    connect_to: Option<IVec2>,
) -> Option<IVec2> {
    let options = PlacementOptions { skip_cost: true, skip_unlock: true, skip_territory: is_headquarters, ..Default::default() };
    for allow_clearing in [false, true] {
        for offset in spiral(START_SEARCH_RADIUS) {
            let origin = preferred + offset;
            let check = check_placement_with(state, defs, kind, origin, rotation, options);
            if !check.ok
                || (!allow_clearing && check.clearing.tiles() > 0)
                || check.entrance_status == buildings::EntranceStatus::Blocked
                || covers_an_entrance(state, check.footprint)
            {
                continue;
            }
            if let Some(target) = connect_to
                && route_between(state, defs, check.entrance, target, Some(check.footprint)).is_none()
            {
                continue;
            }
            return Some(origin);
        }
    }
    None
}

fn covers_an_entrance(state: &GameState, rect: TileRect) -> bool {
    state.buildings.iter().any(|(_, b)| rect.contains(b.entrance()))
}

/// Lays a free start road from the entrance of `building` to `target`.
fn connect(state: &mut GameState, defs: &Defs, building: BuildingId, target: IVec2) {
    let Some(entrance) = state.buildings.get(building).map(|b| b.entrance()) else { return };
    let Some(road) = start_road_type(state, defs).map(|r| r.id) else { return };
    let bridge = bridge_type(defs).map(|b| b.id);
    let Some(route) = route_between(state, defs, entrance, target, None) else {
        log::warn!("no start road from {entrance} to {target}");
        return;
    };
    for (p, axis) in route {
        if state.roads.has_road(p) {
            continue;
        }
        let tile = match (axis, bridge) {
            (Some(axis), Some(bridge)) => RoadTile { road: bridge, bridge_axis: Some(axis) },
            (Some(_), None) => continue,
            (None, _) => RoadTile { road, bridge_axis: None },
        };
        if axis.is_none() {
            buildings::clear_vegetation(&mut state.world, TileRect::tile(p));
        }
        state.roads.set(p, Some(tile));
    }
}

/// Applies scenario `scenario` to a new state whose world is generated and whose entity
/// containers are empty.
pub(crate) fn apply_scenario(state: &mut GameState, defs: &Defs, scenario: ScenarioId) {
    let Some(scenario) = defs.scenarios().get(scenario.index()) else { return };
    for tech in &scenario.researched {
        state.research.mark_researched(*tech);
    }
    state.economy.add_gold(i64::from(scenario.gold));

    let mut hq: Option<(BuildingId, IVec2)> = None;
    for (i, start) in scenario.start_buildings.iter().enumerate() {
        let is_headquarters = i == 0;
        let preferred = state.world.start + start.offset;
        let connect_to = hq.map(|(_, entrance)| entrance);
        let Some(origin) =
            find_spot(state, defs, start.building, preferred, start.rotation, is_headquarters, connect_to)
        else {
            log::warn!("no spot for start building {} near {preferred}", start.building);
            continue;
        };
        let Some(id) = buildings::spawn_finished(state, defs, start.building, origin, start.rotation) else { continue };
        if is_headquarters {
            state.territory.recompute(&state.buildings, defs, &state.diplomacy.land);
            hq = state.buildings.get(id).map(|b| (id, b.entrance()));
        } else if let Some(target) = connect_to {
            connect(state, defs, id, target);
        }
    }
    state.roads.rebuild_components();
    // The start stock lies in the headquarters' storage.
    storage::refresh_stock(state, defs);
    let at = hq.map_or(state.world.start, |(_, entrance)| entrance);
    for m in &scenario.stock {
        let stored = storage::deposit(state, defs, m.resource, m.amount, at);
        if stored < m.amount {
            log::warn!("start stock: only {stored} of {} {} fit into storage", m.amount, m.resource);
        }
    }
    // The settlers move into the start houses (events of the start are not reported).
    population::create_settlers(state, defs, scenario.settlers, scenario.families, &mut Vec::new());
    workforce::allocate(state, defs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiral_visits_rings_in_order() {
        let offsets: Vec<IVec2> = spiral(2).collect();
        assert_eq!(offsets.len(), 25);
        assert_eq!(offsets[0], IVec2::ZERO);
        assert_eq!(offsets[1], IVec2::new(-1, -1));
        assert!(offsets[..9].iter().all(|d| d.x.abs() <= 1 && d.y.abs() <= 1));
        let mut sorted = offsets.clone();
        sorted.sort_by_key(|d| (d.y, d.x));
        sorted.dedup();
        assert_eq!(sorted.len(), 25);
    }
}
