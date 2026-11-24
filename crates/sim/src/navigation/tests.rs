use glam::IVec2;

use super::*;
use crate::grid::Axis;
use crate::roads::RoadTile;
use crate::test_support::{defs, flat_state, spawn};
use crate::world::{Terrain, Vegetation};

fn built(state: &mut GameState) {
    rebuild(state, defs());
}

fn walk(state: &mut GameState, from: IVec2, to: IVec2) -> SearchResult {
    state.navigation.find_path(from, to, defs())
}

fn cost_of(state: &GameState, from: IVec2, steps: &[Direction]) -> u32 {
    let mut p = from;
    let mut total = 0;
    for d in steps {
        total += state.navigation.step_cost(p, *d).expect("allowed step");
        p += d.offset();
    }
    total
}

fn end_of(from: IVec2, steps: &[Direction]) -> IVec2 {
    steps.iter().fold(from, |p, d| p + d.offset())
}

fn road(state: &mut GameState, key: &str, p: IVec2) {
    let road = defs().road_id(key).unwrap();
    state.roads.set(p, Some(RoadTile { road, bridge_axis: None }));
}

#[test]
fn costs_follow_terrain_roads_and_obstacles() {
    let mut state = flat_state(24);
    state.world.tile_mut(IVec2::new(3, 3)).terrain = Terrain::Forest;
    state.world.tile_mut(IVec2::new(4, 3)).terrain = Terrain::ShallowWater;
    state.world.tile_mut(IVec2::new(5, 3)).vegetation = Vegetation::Boulder;
    road(&mut state, "path", IVec2::new(6, 3));
    road(&mut state, "paved", IVec2::new(7, 3));
    let hut = spawn(&mut state, "woodcutter", 10, 10);
    built(&mut state);
    let nav = state.navigation();
    assert_eq!(nav.walk_cost(IVec2::new(2, 3)), Some(1250), "grass off-road");
    assert_eq!(nav.walk_cost(IVec2::new(3, 3)), Some(1750), "forest");
    assert_eq!(nav.walk_cost(IVec2::new(4, 3)), None, "water without a bridge");
    assert_eq!(nav.walk_cost(IVec2::new(5, 3)), None, "boulder");
    assert_eq!(nav.walk_cost(IVec2::new(6, 3)), Some(1000), "path, speed 1.0");
    assert_eq!(nav.walk_cost(IVec2::new(7, 3)), Some(714), "paved, speed 1.4");
    let b = state.building(hut).unwrap();
    assert!(b.rect().tiles().all(|p| !nav.is_walkable(p)), "footprints block");
    assert!(nav.is_walkable(b.entrance()), "the entrance lies outside the footprint");
    assert_eq!(nav.walk_cost(IVec2::new(-1, 0)), None, "outside the map");
}

#[test]
fn shortest_paths_prefer_roads_and_avoid_obstacles() {
    let mut state = flat_state(32);
    built(&mut state);
    // Open grass: a straight line of 10 steps at 1250.
    let found = walk(&mut state, IVec2::new(2, 5), IVec2::new(12, 5));
    let steps = found.steps.clone().unwrap();
    assert_eq!(steps.len(), 10);
    assert_eq!(cost_of(&state, IVec2::new(2, 5), &steps), 12_500);
    assert!(found.expansions >= 10);
    // A road one row away: stepping down onto it, walking along it and stepping back up
    // (11 × 1000 + 1250) is cheaper than staying on the grass (10 × 1250).
    for x in 2..=12 {
        road(&mut state, "path", IVec2::new(x, 6));
    }
    built(&mut state);
    let steps = walk(&mut state, IVec2::new(2, 5), IVec2::new(12, 5)).steps.unwrap();
    let cost = cost_of(&state, IVec2::new(2, 5), &steps);
    assert_eq!(cost, 1000 * 11 + 1250, "down to the road, along it, back up: {steps:?}");
    assert_eq!(end_of(IVec2::new(2, 5), &steps), IVec2::new(12, 5));
    // A wall of boulders with one gap forces the path through the gap.
    let mut state = flat_state(32);
    for y in 0..32 {
        if y != 20 {
            state.world.tile_mut(IVec2::new(8, y)).vegetation = Vegetation::Boulder;
        }
    }
    built(&mut state);
    let steps = walk(&mut state, IVec2::new(4, 4), IVec2::new(12, 4)).steps.unwrap();
    let mut p = IVec2::new(4, 4);
    let mut through_gap = false;
    for d in &steps {
        p += d.offset();
        through_gap |= p == IVec2::new(8, 20);
    }
    assert!(through_gap);
    assert_eq!(steps.len(), 8 + 2 * 16);
}

#[test]
fn cliffs_block_and_single_steps_cost_extra() {
    let mut state = flat_state(16);
    // A plateau two levels up: unreachable from the plain.
    for y in 0..16 {
        for x in 8..16 {
            state.world.tile_mut(IVec2::new(x, y)).elevation = 4;
        }
    }
    built(&mut state);
    assert!(walk(&mut state, IVec2::new(2, 2), IVec2::new(12, 2)).steps.is_none());
    assert_eq!(walk(&mut state, IVec2::new(2, 2), IVec2::new(12, 2)).expansions, 0, "different components");
    // A ramp of single levels at row 8 connects it.
    state.world.tile_mut(IVec2::new(7, 8)).elevation = 3;
    built(&mut state);
    let steps = walk(&mut state, IVec2::new(2, 8), IVec2::new(12, 8)).steps.unwrap();
    assert_eq!(steps.len(), 10);
    let slope = u32::from(defs().rules().walking.slope_cost_permille);
    assert_eq!(cost_of(&state, IVec2::new(2, 8), &steps), 10 * 1250 + 2 * slope);
}

#[test]
fn water_is_crossed_only_on_bridges_along_their_axis() {
    let mut state = flat_state(24);
    for y in 0..24 {
        for x in 10..13 {
            state.world.tile_mut(IVec2::new(x, y)).terrain = Terrain::ShallowWater;
        }
    }
    built(&mut state);
    assert!(walk(&mut state, IVec2::new(5, 5), IVec2::new(18, 5)).steps.is_none(), "a river without a bridge");
    let bridge = defs().road_id("bridge").unwrap();
    for x in 10..13 {
        state.roads.set(IVec2::new(x, 12), Some(RoadTile { road: bridge, bridge_axis: Some(Axis::X) }));
    }
    built(&mut state);
    let steps = walk(&mut state, IVec2::new(5, 5), IVec2::new(18, 5)).steps.unwrap();
    let mut p = IVec2::new(5, 5);
    let mut crossed = Vec::new();
    for d in &steps {
        p += d.offset();
        if (10..13).contains(&p.x) {
            assert_eq!(p.y, 12, "only on the bridge");
            assert_eq!(d.axis(), Axis::X, "along the bridge axis");
            crossed.push(p.x);
        }
    }
    assert_eq!(crossed, vec![10, 11, 12]);
    // Bridge tiles cannot be entered from the side.
    assert_eq!(state.navigation.step_cost(IVec2::new(11, 11), Direction::South), None);
    assert!(state.navigation.step_cost(IVec2::new(9, 12), Direction::East).is_some());
}

#[test]
fn walls_block_and_gates_let_walkers_through() {
    let mut state = flat_state(24);
    let gate_y = 9;
    for y in 0..24 {
        let key = if y == gate_y { "gate" } else { "wall" };
        spawn(&mut state, key, 12, y);
    }
    built(&mut state);
    assert!(!state.navigation.is_walkable(IVec2::new(12, 3)), "wall");
    assert!(state.navigation.is_walkable(IVec2::new(12, gate_y)), "gate");
    let steps = walk(&mut state, IVec2::new(6, 3), IVec2::new(18, 3)).steps.unwrap();
    let mut p = IVec2::new(6, 3);
    let mut via_gate = false;
    for d in &steps {
        p += d.offset();
        via_gate |= p == IVec2::new(12, gate_y);
        assert!(p.x != 12 || p.y == gate_y);
    }
    assert!(via_gate);
}

#[test]
fn ties_are_broken_deterministically() {
    // Many equally cheap routes on open grass: the answer is fixed and does not depend on
    // the cache or on earlier searches.
    let mut state = flat_state(32);
    built(&mut state);
    let first = walk(&mut state, IVec2::new(3, 3), IVec2::new(9, 8)).steps.unwrap();
    assert_eq!(first.len(), 11);
    let _ = walk(&mut state, IVec2::new(20, 20), IVec2::new(4, 25));
    let mut fresh = flat_state(32);
    built(&mut fresh);
    let _ = walk(&mut fresh, IVec2::new(1, 30), IVec2::new(30, 1));
    let again = walk(&mut fresh, IVec2::new(3, 3), IVec2::new(9, 8));
    assert_eq!(again.steps.as_ref(), Some(&first));
    // A* prefers nodes closer to the goal (lower h) and then lower tile indices: it walks the
    // longer axis last.
    use Direction::{East as E, South as S};
    assert_eq!(first, vec![E, E, E, E, E, E, S, S, S, S, S]);
}

#[test]
fn the_memory_answers_repeated_requests_cheaply() {
    let mut state = flat_state(48);
    built(&mut state);
    let a = walk(&mut state, IVec2::new(2, 2), IVec2::new(40, 30));
    let b = walk(&mut state, IVec2::new(2, 2), IVec2::new(40, 30));
    assert_eq!(a.steps, b.steps, "same steps");
    assert_eq!(b.expansions, defs().rules().walking.path_cache_hit_cost, "a remembered path costs only the hit cost");
    assert!(a.expansions > b.expansions);
    let stats = state.navigation().stats();
    assert_eq!((stats.requests, stats.cache_hits, stats.searches), (2, 1, 1));
    assert_eq!(stats.cache_hit_permille(), 500);
    // A road on the way is a possible shortcut: searched again.
    road(&mut state, "path", IVec2::new(20, 20));
    sync(&mut state, defs());
    let c = walk(&mut state, IVec2::new(2, 2), IVec2::new(40, 30));
    assert_eq!(state.navigation().stats().searches, 2);
    assert!(c.steps.is_some());
}

#[test]
fn incremental_updates_match_a_full_build() {
    let mut state = flat_state(48);
    built(&mut state);
    let before = state.navigation().revision();
    let id = spawn(&mut state, "sawmill", 20, 20);
    road(&mut state, "gravel", IVec2::new(5, 5));
    sync(&mut state, defs());
    assert!(state.navigation().revision() > before);
    let rect = state.building(id).unwrap().rect();
    assert!(rect.tiles().all(|p| !state.navigation().is_walkable(p)));
    let mut full = state.clone();
    built(&mut full);
    for y in 0..48 {
        for x in 0..48 {
            let p = IVec2::new(x, y);
            assert_eq!(state.navigation().walk_cost(p), full.navigation().walk_cost(p), "{p}");
        }
    }
    // Removing the building frees its tiles again; an unchanged rebuild keeps the revision.
    crate::buildings::demolish_building(&mut state, defs(), id, &mut Vec::new()).expect("demolished");
    sync(&mut state, defs());
    assert!(rect.tiles().all(|p| state.navigation().is_walkable(p)));
    let revision = state.navigation().revision();
    state.buildings.changes_mut().touch(rect);
    sync(&mut state, defs());
    assert_eq!(state.navigation().revision(), revision, "no effective change");
}

#[test]
fn searches_stop_at_the_node_limit() {
    let d = defs_with_limit(1000);
    let mut state = flat_state(64);
    rebuild(&mut state, &d);
    let far = state.navigation.find_path(IVec2::new(1, 1), IVec2::new(50, 50), &d);
    // The straight route needs about 100 expansions; a detour behind a long wall needs more
    // than the limit.
    assert!(far.steps.is_some() && far.expansions < 1000, "{}", far.expansions);
    for y in 0..63 {
        state.world.tile_mut(IVec2::new(32, y)).vegetation = Vegetation::Boulder;
    }
    rebuild(&mut state, &d);
    let blocked = state.navigation.find_path(IVec2::new(31, 1), IVec2::new(33, 1), &d);
    assert!(blocked.steps.is_none(), "over the node limit");
    assert_eq!(blocked.expansions, 1000);
}

fn defs_with_limit(nodes: u32) -> crate::defs::Defs {
    crate::test_support::defs_edited("rules.ron", |text| {
        text.replace("max_search_nodes: 150000", &format!("max_search_nodes: {nodes}"))
    })
}

#[test]
fn index_steps_match_position_steps() {
    // The pathfinder's index-based steps agree with `step_cost` everywhere: map edges,
    // bridges, cliffs, buildings.
    let mut state = flat_state(24);
    for y in 0..24 {
        state.world.tile_mut(IVec2::new(12, y)).terrain = Terrain::ShallowWater;
        state.world.tile_mut(IVec2::new(4, y)).elevation = 4;
    }
    state.world.tile_mut(IVec2::new(4, 7)).elevation = 3;
    let bridge = defs().road_id("bridge").unwrap();
    state.roads.set(IVec2::new(12, 5), Some(RoadTile { road: bridge, bridge_axis: Some(Axis::X) }));
    spawn(&mut state, "sawmill", 16, 16);
    built(&mut state);
    let grid = &state.navigation.grid;
    for y in 0..24 {
        for x in 0..24 {
            let p = IVec2::new(x, y);
            let i = grid.index(p).unwrap();
            for d in Direction::ALL {
                let by_pos = grid.step_cost(p, d).map(|c| (grid.index(p + d.offset()).unwrap(), c));
                assert_eq!(grid.step_from(i, x, d), by_pos, "{p} {d:?}");
            }
        }
    }
}

#[test]
fn long_searches_continue_over_several_calls() {
    let d = defs();
    let mut state = flat_state(64);
    for y in 0..63 {
        state.world.tile_mut(IVec2::new(32, y)).vegetation = Vegetation::Boulder;
    }
    built(&mut state);
    let (from, to) = (IVec2::new(31, 1), IVec2::new(33, 1));
    // The whole search at once.
    let mut fresh = state.clone();
    let full = fresh.navigation.find_path(from, to, d);
    let total = full.expansions;
    assert!(total > 1000 && full.steps.is_some());
    // In slices of 300 expansions: pending until the slices add up to the total.
    let mut spent = 0;
    let answer = loop {
        match state.navigation.find_path_sliced(from, to, spent, 300, d) {
            PathAnswer::Done(answer) => break answer,
            PathAnswer::Pending => spent += 300,
        }
        assert!(spent <= total, "finishes once the slices cover the search");
    };
    assert_eq!(answer, full);
    assert!(spent < total && spent + 300 >= total);
    // Interrupted by another search (or a load): restarted and caught up, same result.
    let mut other = state.clone();
    other.navigation.memory.clear();
    assert_eq!(other.navigation.find_path_sliced(from, to, 0, 300, d), PathAnswer::Pending);
    let _ = other.navigation.find_path(IVec2::new(1, 60), IVec2::new(5, 60), d);
    assert_eq!(other.navigation.find_path_sliced(from, to, 300, total - 300, d), PathAnswer::Done(full.clone()));
    // The way back is the reversed way there, remembered: it costs only the hit cost.
    let back = state.navigation.find_path(to, from, d);
    let reversed: Vec<Direction> = full.steps.unwrap().iter().rev().map(|d| d.opposite()).collect();
    assert_eq!(back, SearchResult { steps: Some(reversed), expansions: d.rules().walking.path_cache_hit_cost });
}

/// Remembered paths stay over changes that cannot affect them and are forgotten when a
/// change blocks them or offers a possible shortcut.
#[test]
fn remembered_paths_follow_grid_changes() {
    let d = defs();
    let mut state = flat_state(64);
    built(&mut state);
    let (a, b) = (IVec2::new(4, 10), IVec2::new(40, 10));
    let first = state.navigation.find_path(a, b, d);
    assert!(first.expansions > d.rules().walking.path_cache_hit_cost);
    assert_eq!(state.navigation.remembered_paths(), 1);
    // A building far away (south-east corner): still remembered.
    spawn(&mut state, "sawmill", 50, 50);
    crate::navigation::sync(&mut state, d);
    assert_eq!(state.navigation.remembered_paths(), 1);
    assert_eq!(state.navigation.find_path(a, b, d).expansions, d.rules().walking.path_cache_hit_cost);
    // A road far from the line between the two tiles: a detour over it can never be cheaper.
    crate::test_support::lay_path(&mut state, IVec2::new(4, 40), IVec2::new(40, 40));
    crate::navigation::sync(&mut state, d);
    assert_eq!(state.navigation.remembered_paths(), 1, "a road 30 tiles away is no shortcut");
    // A road along the way: a possible shortcut, searched again (and now on the road).
    crate::test_support::lay_path(&mut state, IVec2::new(4, 12), IVec2::new(40, 12));
    crate::navigation::sync(&mut state, d);
    assert_eq!(state.navigation.remembered_paths(), 0);
    let on_road = state.navigation.find_path(a, b, d);
    assert!(on_road.expansions > d.rules().walking.path_cache_hit_cost);
    assert_ne!(on_road.steps, first.steps);
    // An obstacle on the remembered path: forgotten.
    let path = on_road.steps.clone().unwrap();
    let mut p = a;
    for step in path.iter().take(path.len() / 2) {
        p += step.offset();
    }
    state.roads.set(p, None);
    state.world.tile_mut(p).vegetation = Vegetation::Boulder;
    crate::navigation::rebuild(&mut state, d);
    assert_eq!(state.navigation.remembered_paths(), 0, "a boulder on the way (full rebuild) is noticed");
    let around = state.navigation.find_path(a, b, d);
    assert!(around.steps.is_some_and(|s| s != path));
}

/// The path memory is saved: a loaded game adopts it for the same grid, drops it for another
/// grid and drops entries that do not describe a walkable path.
#[test]
fn the_path_memory_is_saved_and_checked_on_loading() {
    let d = defs();
    let mut state = flat_state(128);
    built(&mut state);
    for (a, b) in [(IVec2::new(2, 2), IVec2::new(30, 20)), (IVec2::new(5, 40), IVec2::new(40, 5)), (IVec2::new(10, 10), IVec2::new(11, 30))] {
        assert!(state.navigation.find_path(a, b, d).steps.is_some());
    }
    let header = crate::save::SaveHeader::for_state(&state, d, "paths", 0);
    let bytes = crate::save::encode(&header, &state).unwrap();
    let loaded = crate::save::decode(&bytes, d).unwrap().state;
    assert_eq!(loaded.navigation.remembered_paths(), 3);
    assert_eq!(loaded.navigation.memory.to_saved(1), state.navigation.memory.to_saved(1));
    // Another grid (a boulder changed after saving): the memory is not adopted.
    let mut other = crate::save::decode(&bytes, d).unwrap().state;
    other.world.tile_mut(IVec2::new(30, 30)).vegetation = Vegetation::Boulder;
    let saved = state.navigation.memory.to_saved(state.navigation.grid.fingerprint());
    other.navigation.loaded = Some(saved.clone());
    crate::navigation::rebuild(&mut other, d);
    assert_eq!(other.navigation.remembered_paths(), 0);
    // A forged entry (a path through a wall of boulders) is dropped, the others are kept.
    let mut walled = state.clone();
    for y in 0..128 {
        if y != 3 {
            walled.world.tile_mut(IVec2::new(20, y)).vegetation = Vegetation::Boulder;
        }
    }
    crate::navigation::rebuild(&mut walled, d);
    let fingerprint = walled.navigation.grid.fingerprint();
    let mut forged = saved;
    forged.grid = fingerprint;
    walled.navigation.loaded = Some(forged);
    crate::navigation::rebuild(&mut walled, d);
    assert_eq!(walled.navigation.remembered_paths(), 1, "only the path that does not cross x = 20 survives");
}
