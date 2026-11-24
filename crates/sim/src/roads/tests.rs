//! Tests of road masks, planning, bridges, slopes, commands and connectivity.

use glam::IVec2;

use super::*;
use crate::buildings::construction::spawn_finished;
use crate::buildings::Rotation;
use crate::command::{Command, RejectReason};
use crate::defs::{BuildingTypeId, ResourceId};
use crate::economy::Economy;
use crate::event::SimEvent;
use crate::grid::TileRect;
use crate::state::GameState;
use crate::territory::Territory;
use crate::test_support::{defs, exec, flat_state, rejection};
use crate::world::{Terrain, TreeKind, Vegetation};

fn road(key: &str) -> RoadTypeId {
    defs().road_id(key).unwrap()
}

fn res(key: &str) -> ResourceId {
    defs().resource_id(key).unwrap()
}

fn kind(key: &str) -> BuildingTypeId {
    defs().building_id(key).unwrap()
}

fn set_road(state: &mut GameState, p: IVec2, key: &str) {
    state.roads.set(p, Some(RoadTile { road: road(key), bridge_axis: None }));
    state.roads.rebuild_components();
}

fn spawn(state: &mut GameState, key: &str, x: i32, y: i32, rotation: u8) -> BuildingId {
    spawn_finished(state, defs(), kind(key), IVec2::new(x, y), Rotation::new(rotation).unwrap()).unwrap()
}

fn line(from: IVec2, to: IVec2) -> Vec<IVec2> {
    let step = (to - from).signum();
    let n = (to - from).abs().max_element();
    (0..=n).map(|i| from + step * i).collect()
}

fn is_contiguous(tiles: &[IVec2]) -> bool {
    tiles.windows(2).all(|w| (w[1] - w[0]).abs().element_sum() == 1)
}

fn river(state: &mut GameState, xs: std::ops::RangeInclusive<i32>, terrain: Terrain) {
    for x in xs {
        for y in 0..state.world.height() {
            state.world.tile_mut(IVec2::new(x, y)).terrain = terrain;
        }
    }
}

#[test]
fn masks_connect_neighbours_and_building_entrances() {
    let mut state = flat_state(32);
    for p in [IVec2::new(5, 5), IVec2::new(6, 5), IVec2::new(7, 5), IVec2::new(6, 6)] {
        set_road(&mut state, p, "path");
    }
    assert_eq!(state.road_mask(IVec2::new(6, 5)), Some(8 | 2 | 4));
    assert_eq!(state.road_mask(IVec2::new(5, 5)), Some(2));
    assert_eq!(state.road_mask(IVec2::new(6, 6)), Some(1));
    assert_eq!(state.road_mask(IVec2::new(9, 9)), None);
    // A house facing south: the road in front of its door connects north into it.
    let house = spawn(&mut state, "house_wood", 10, 10, 0);
    let entrance = state.building(house).unwrap().entrance();
    assert_eq!(entrance, IVec2::new(11, 12));
    assert!(!state.has_road_access(house));
    set_road(&mut state, entrance, "path");
    set_road(&mut state, entrance + IVec2::new(1, 0), "path");
    assert_eq!(state.road_mask(entrance), Some(1 | 2));
    assert!(state.has_road_access(house));
    // A road beside the house (not at its entrance) does not connect into it (no W bit).
    set_road(&mut state, IVec2::new(12, 11), "path");
    assert_eq!(state.road_mask(IVec2::new(12, 11)), Some(4));
    let tile = state.roads().get(entrance).unwrap();
    assert_eq!(tile.sprite(defs(), 3), "roads/path_3");
    let bridge = RoadTile { road: road("bridge"), bridge_axis: Some(Axis::Y) };
    assert_eq!(bridge.sprite(defs(), 5), "roads/bridge_y");
}

#[test]
fn planner_avoids_buildings_and_boulders() {
    let mut state = flat_state(48);
    for y in 5..=30 {
        state.world.tile_mut(IVec2::new(20, y)).vegetation = Vegetation::Boulder;
    }
    spawn(&mut state, "house_wood", 14, 31, 0);
    let from = IVec2::new(10, 20);
    let to = IVec2::new(30, 20);
    let plan = plan_road(&state, defs(), from, to, road("gravel"));
    assert!(plan.valid, "{:?}", plan.problems);
    let tiles = plan.positions();
    assert_eq!((tiles[0], *tiles.last().unwrap()), (from, to));
    assert!(is_contiguous(&tiles));
    assert!(tiles.iter().all(|p| state.world.tile(*p).vegetation != Vegetation::Boulder));
    assert!(tiles.iter().all(|p| state.buildings().id_at(*p).is_none()));
    assert!(tiles.iter().any(|p| p.x == 20 && !(5..=30).contains(&p.y)), "goes around the boulder line");
    // Gravel: 1 gold + 1 stone per tile.
    assert_eq!(plan.cost.gold as usize, tiles.len());
    assert_eq!(plan.cost.material(res("stone")) as usize, tiles.len());
    assert_eq!(plan.new_tile_count(), tiles.len());
    // Few turns: the route is made of long straight runs.
    let turns = tiles.windows(3).filter(|w| w[1] - w[0] != w[2] - w[1]).count();
    assert!(turns <= 4, "{turns} turns");
    assert!(matches!(plan.to_command(), Some(Command::BuildRoad { .. })));
}

#[test]
fn bridges_cross_shallow_water_in_a_straight_line() {
    let mut state = flat_state(48);
    river(&mut state, 20..=22, Terrain::ShallowWater);
    let plan = plan_road(&state, defs(), IVec2::new(15, 20), IVec2::new(28, 20), road("gravel"));
    assert!(plan.valid, "{:?}", plan.problems);
    assert_eq!(plan.positions(), line(IVec2::new(15, 20), IVec2::new(28, 20)));
    let bridges: Vec<&PlannedRoadTile> = plan.tiles.iter().filter(|t| t.bridge_axis.is_some()).collect();
    assert_eq!(bridges.len(), 3);
    assert!(bridges.iter().all(|t| t.road == road("bridge") && t.bridge_axis == Some(Axis::X)));
    // 11 gravel tiles (1 G, 1 stone) + 3 bridge tiles (15 G, 4 planks, 2 stone).
    assert_eq!(plan.cost.gold, 11 + 3 * 15);
    assert_eq!(plan.cost.material(res("stone")), 11 + 3 * 2);
    assert_eq!(plan.cost.material(res("planks")), 3 * 4);

    let tiles = plan.positions();
    let events = exec(&mut state, Command::BuildRoad { tiles: tiles.clone(), road: road("gravel") });
    assert!(events.iter().any(|e| matches!(e, SimEvent::RoadsBuilt { tiles: t, .. } if t.len() == 14)));
    assert_eq!(state.roads().get(IVec2::new(21, 20)).unwrap().bridge_axis, Some(Axis::X));
    assert!(state.roads().connected(IVec2::new(15, 20), IVec2::new(28, 20)));
    // Bridges connect only along their axis.
    set_road(&mut state, IVec2::new(21, 21), "gravel");
    assert!(!state.roads().links(IVec2::new(21, 20), crate::grid::Direction::South));
    assert_eq!(state.road_mask(IVec2::new(21, 20)), Some(2 | 8));
    // Removing one bridge tile removes the whole bridge but not the banks.
    let events = exec(&mut state, Command::RemoveRoad { tiles: vec![IVec2::new(22, 20)] });
    let removed = events.iter().find_map(|e| match e {
        SimEvent::RoadsRemoved { tiles } => Some(tiles.clone()),
        _ => None,
    });
    assert_eq!(removed, Some(vec![IVec2::new(20, 20), IVec2::new(21, 20), IVec2::new(22, 20)]));
    assert!(state.roads().has_road(IVec2::new(19, 20)) && state.roads().has_road(IVec2::new(23, 20)));
    assert!(!state.roads().connected(IVec2::new(15, 20), IVec2::new(28, 20)));
}

#[test]
fn bridges_have_a_maximum_span_and_need_shallow_water() {
    let mut wide = flat_state(48);
    river(&mut wide, 20..=26, Terrain::ShallowWater);
    let plan = plan_road(&wide, defs(), IVec2::new(15, 20), IVec2::new(30, 20), road("path"));
    assert!(!plan.valid);
    assert_eq!(plan.problems.first(), Some(&RoadProblem::NoRoute));
    assert!(plan.problems.contains(&RoadProblem::BridgeTooLong { pos: IVec2::new(20, 20), span: 7, max: 6 }));
    assert!(plan.tiles.iter().any(|t| !t.ok), "offending tiles are marked");

    let mut deep = flat_state(48);
    river(&mut deep, 20..=22, Terrain::DeepWater);
    let plan = plan_road(&deep, defs(), IVec2::new(15, 20), IVec2::new(28, 20), road("path"));
    assert_eq!(plan.problems.first(), Some(&RoadProblem::NoRoute));
    assert!(plan.problems.contains(&RoadProblem::DeepWater { pos: IVec2::new(20, 20) }));

    // Six tiles are fine.
    let mut six = flat_state(48);
    river(&mut six, 20..=25, Terrain::ShallowWater);
    assert!(plan_road(&six, defs(), IVec2::new(15, 20), IVec2::new(30, 20), road("path")).valid);
}

#[test]
fn slopes_of_more_than_one_level_are_avoided() {
    let mut state = flat_state(40);
    for p in TileRect::new(IVec2::new(16, 0), IVec2::new(24, 40)).tiles() {
        state.world.tile_mut(p).elevation = 4;
    }
    let from = IVec2::new(10, 10);
    let to = IVec2::new(20, 10);
    let plan = plan_road(&state, defs(), from, to, road("path"));
    assert_eq!(plan.problems.first(), Some(&RoadProblem::NoRoute));
    assert!(plan.problems.contains(&RoadProblem::TooSteep { pos: IVec2::new(16, 10) }));
    // A ramp tile at level 3 makes the climb possible.
    state.world.tile_mut(IVec2::new(15, 18)).elevation = 3;
    let plan = plan_road(&state, defs(), from, to, road("path"));
    assert!(plan.valid, "{:?}", plan.problems);
    assert!(plan.positions().contains(&IVec2::new(15, 18)));
    let tiles = plan.positions();
    assert!(tiles.windows(2).all(|w| {
        (i32::from(state.world.tile(w[0]).elevation) - i32::from(state.world.tile(w[1]).elevation)).abs() <= 1
    }));
}

#[test]
fn road_commands_pay_clear_and_report() {
    let mut state = flat_state(32);
    state.world.tile_mut(IVec2::new(6, 5)).vegetation = Vegetation::Tree { kind: TreeKind::Conifer, growth: 200 };
    state.world.tile_mut(IVec2::new(7, 5)).vegetation = Vegetation::Bush;
    let gold = state.economy().treasury();
    let stone = state.economy().stock().get(res("stone"));
    let wood = state.economy().stock().get(res("wood"));
    let tiles = line(IVec2::new(5, 5), IVec2::new(9, 5));
    let events = exec(&mut state, Command::BuildRoad { tiles: tiles.clone(), road: road("gravel") });
    assert_eq!(events, vec![SimEvent::RoadsBuilt { road: road("gravel"), tiles: tiles.clone() }]);
    assert_eq!(state.economy().treasury(), gold - 5);
    assert_eq!(state.economy().stock().get(res("stone")), stone - 5);
    assert_eq!(state.economy().stock().get(res("wood")), wood + 2, "one tree cleared");
    assert_eq!(state.world().tile(IVec2::new(7, 5)).vegetation, Vegetation::None);
    assert_eq!(state.roads().len(), 5);
    // Rebuilding the same road costs nothing and changes nothing.
    assert!(exec(&mut state, Command::BuildRoad { tiles: tiles.clone(), road: road("gravel") }).is_empty());
    assert_eq!(state.economy().treasury(), gold - 5);
    // A slower type keeps the existing road; a faster one upgrades it.
    let keep = validate_road_tiles(&state, defs(), &tiles, road("path"));
    assert!(keep.valid && keep.new_tile_count() == 0 && keep.cost.gold == 0);
    assert!(keep.tiles.iter().all(|t| t.road == road("gravel")));
    // Removal.
    let events = exec(&mut state, Command::RemoveRoad { tiles: vec![IVec2::new(9, 5), IVec2::new(20, 20)] });
    assert_eq!(events, vec![SimEvent::RoadsRemoved { tiles: vec![IVec2::new(9, 5)] }]);
    let events = exec(&mut state, Command::RemoveRoad { tiles: vec![IVec2::new(9, 5)] });
    assert_eq!(rejection(&events), Some(&RejectReason::NoRoadToRemove));
    // Unaffordable roads are rejected with the German reason.
    state.economy = Economy::new(defs());
    let events = exec(&mut state, Command::BuildRoad { tiles: line(IVec2::new(5, 8), IVec2::new(6, 8)), road: road("gravel") });
    let reason = rejection(&events).unwrap();
    assert_eq!(reason, &RejectReason::Road(RoadProblem::InsufficientGold { needed: 2, available: 0 }));
    assert_eq!(reason.to_string(), "Nicht genug Gold (0 von 2).");
}

#[test]
fn explicit_tile_lists_are_validated() {
    let mut state = flat_state(32);
    let gravel = road("gravel");
    let check = |state: &GameState, tiles: &[IVec2], r: RoadTypeId| validate_road_tiles(state, defs(), tiles, r).problems;
    assert_eq!(check(&state, &[], gravel), vec![RoadProblem::Empty]);
    assert_eq!(validate_road_tiles(&state, defs(), &[IVec2::ZERO], RoadTypeId::from_index(200).unwrap()).problems, vec![
        RoadProblem::UnknownRoadType
    ]);
    assert_eq!(check(&state, &[IVec2::new(1, 1), IVec2::new(3, 1)], gravel), vec![RoadProblem::NotContiguous {
        pos: IVec2::new(3, 1)
    }]);
    assert_eq!(check(&state, &[IVec2::new(1, 1), IVec2::new(2, 1), IVec2::new(1, 1)], gravel), vec![
        RoadProblem::Repeated { pos: IVec2::new(1, 1) }
    ]);
    assert!(matches!(check(&state, &[IVec2::ZERO], road("road")).as_slice(), [RoadProblem::NotUnlocked { .. }]));
    assert_eq!(check(&state, &[IVec2::ZERO], road("bridge")), vec![RoadProblem::BridgeTypeSelected]);
    assert_eq!(check(&state, &[IVec2::new(-1, 0)], gravel), vec![RoadProblem::OutOfBounds { pos: IVec2::new(-1, 0) }]);
    state.world.tile_mut(IVec2::new(4, 4)).vegetation = Vegetation::Boulder;
    assert_eq!(check(&state, &[IVec2::new(4, 4)], gravel), vec![RoadProblem::Obstacle { pos: IVec2::new(4, 4) }]);
    spawn(&mut state, "well", 6, 6, 0);
    assert_eq!(check(&state, &[IVec2::new(6, 6)], gravel), vec![RoadProblem::Building { pos: IVec2::new(6, 6) }]);
    // Steps of two levels.
    state.world.tile_mut(IVec2::new(11, 10)).elevation = 4;
    assert_eq!(check(&state, &[IVec2::new(10, 10), IVec2::new(11, 10)], gravel), vec![RoadProblem::TooSteep {
        pos: IVec2::new(11, 10)
    }]);
    // Bridges: banks, straightness, span.
    for x in 20..=22 {
        for y in 0..32 {
            state.world.tile_mut(IVec2::new(x, y)).terrain = Terrain::ShallowWater;
        }
    }
    assert_eq!(check(&state, &[IVec2::new(19, 5), IVec2::new(20, 5)], gravel), vec![RoadProblem::BridgeNeedsBanks {
        pos: IVec2::new(20, 5)
    }]);
    let bent = [IVec2::new(19, 5), IVec2::new(20, 5), IVec2::new(21, 5), IVec2::new(21, 6), IVec2::new(22, 6), IVec2::new(23, 6)];
    assert_eq!(check(&state, &bent, gravel), vec![RoadProblem::BridgeNotStraight { pos: IVec2::new(20, 5) }]);
    assert!(check(&state, &line(IVec2::new(19, 5), IVec2::new(23, 5)), gravel).is_empty());
    // Foreign territory.
    state.territory = Territory::new(32, 32);
    assert_eq!(check(&state, &[IVec2::new(1, 1)], gravel), vec![RoadProblem::OutsideTerritory { pos: IVec2::new(1, 1) }]);
}

#[test]
fn components_answer_reachability() {
    let mut state = flat_state(32);
    for p in line(IVec2::new(2, 2), IVec2::new(8, 2)) {
        set_road(&mut state, p, "path");
    }
    for p in line(IVec2::new(2, 10), IVec2::new(8, 10)) {
        set_road(&mut state, p, "path");
    }
    assert_eq!(state.roads().component_count(), 2);
    assert!(state.roads().connected(IVec2::new(2, 2), IVec2::new(8, 2)));
    assert!(!state.roads().connected(IVec2::new(2, 2), IVec2::new(2, 10)));
    assert!(!state.roads().connected(IVec2::new(2, 2), IVec2::new(15, 15)), "no road there");
    // Two houses on different networks.
    let a = spawn(&mut state, "house_wood", 4, 0, 0); // entrance (5, 2)
    let b = spawn(&mut state, "house_wood", 4, 11, 2); // entrance (4, 10)
    assert!(state.has_road_access(a) && state.has_road_access(b));
    assert!(!buildings_connected(state.world(), state.roads(), state.buildings(), a, b));
    for p in line(IVec2::new(8, 3), IVec2::new(8, 9)) {
        set_road(&mut state, p, "path");
    }
    assert_eq!(state.roads().component_count(), 1);
    assert!(buildings_connected(state.world(), state.roads(), state.buildings(), a, b));
    assert_eq!(building_component(state.world(), state.roads(), state.buildings(), a), state.roads().component(IVec2::new(2, 10)));
}

#[test]
fn access_road_suggestion_reaches_the_nearest_road() {
    let mut state = flat_state(40);
    for p in line(IVec2::new(20, 5), IVec2::new(20, 25)) {
        set_road(&mut state, p, "path");
    }
    let plan = plan_access_road(&state, defs(), kind("house_wood"), IVec2::new(10, 10), Rotation::default(), road("path"))
        .unwrap();
    assert!(plan.valid);
    let tiles = plan.positions();
    assert_eq!(tiles[0], IVec2::new(11, 12), "starts at the entrance");
    assert!(state.roads().has_road(*tiles.last().unwrap()));
    assert_eq!(tiles.len(), 10);
    assert_eq!(plan.new_tile_count(), 9);
    assert!(tiles.iter().all(|p| !TileRect::new(IVec2::new(10, 10), IVec2::splat(2)).contains(*p)));
    // Entrance already on a road: nothing to build.
    let east = Rotation::new(3).unwrap();
    let plan = plan_access_road(&state, defs(), kind("house_wood"), IVec2::new(18, 13), east, road("path")).unwrap();
    assert_eq!((plan.positions(), plan.new_tile_count()), (vec![IVec2::new(20, 13)], 0));
    // No road within reach.
    let empty = flat_state(40);
    assert!(
        plan_access_road(&empty, defs(), kind("house_wood"), IVec2::new(10, 10), Rotation::default(), road("path")).is_none()
    );
}

#[test]
fn access_roads_are_built_only_with_their_building() {
    let mut state = flat_state(40);
    for p in line(IVec2::new(20, 5), IVec2::new(20, 25)) {
        set_road(&mut state, p, "path");
    }
    let place = |origin| Command::PlaceBuildingWithAccess {
        kind: kind("house_wood"),
        origin,
        rotation: Rotation::default(),
        road: road("path"),
    };
    // Placed: the planned footpath from the entrance to the road is built in the same command.
    let events = exec(&mut state, place(IVec2::new(10, 10)));
    assert_eq!(rejection(&events), None);
    assert!(events.iter().any(|e| matches!(e, SimEvent::BuildingPlaced { .. })));
    assert!(state.roads().has_road(IVec2::new(11, 12)), "the entrance tile got the path");
    assert!(events.iter().any(|e| matches!(e, SimEvent::RoadsBuilt { .. })));
    // Rejected (the spot is taken now): no orphaned path.
    let roads_before = state.roads().iter().count();
    let events = exec(&mut state, place(IVec2::new(10, 10)));
    assert!(matches!(rejection(&events), Some(RejectReason::Placement(_))));
    assert!(!events.iter().any(|e| matches!(e, SimEvent::RoadsBuilt { .. })));
    assert_eq!(state.roads().iter().count(), roads_before);
    state.validate_entities(defs()).unwrap();
}

#[test]
fn road_grid_roundtrip_rebuilds_components_and_rejects_bad_sizes() {
    let mut state = flat_state(16);
    for p in line(IVec2::new(1, 1), IVec2::new(5, 1)) {
        set_road(&mut state, p, "gravel");
    }
    let config = bincode::config::standard();
    let bytes = bincode::serde::encode_to_vec(&state.roads, config).unwrap();
    let (back, _): (RoadNetwork, usize) = bincode::serde::decode_from_slice(&bytes, config).unwrap();
    assert_eq!(back.len(), 5);
    assert_eq!(back.component_count(), 1);
    assert!(back.connected(IVec2::new(1, 1), IVec2::new(5, 1)));
    let mut broken = state.roads.clone();
    broken.tiles.pop();
    let bytes = bincode::serde::encode_to_vec(&broken, config).unwrap();
    assert!(bincode::serde::decode_from_slice::<RoadNetwork, _>(&bytes, config).is_err());
    // Sizes beyond the largest map are rejected, even with a matching (empty) tile list.
    let mut huge = RoadNetwork::new(1, 1);
    huge.width = 1 << 20;
    huge.height = 0;
    huge.tiles.clear();
    let bytes = bincode::serde::encode_to_vec(&huge, config).unwrap();
    let err = bincode::serde::decode_from_slice::<RoadNetwork, _>(&bytes, config).unwrap_err();
    assert!(err.to_string().contains("outside"), "{err}");
}

#[test]
fn roads_cross_wall_lines_only_through_gates() {
    let mut state = flat_state(48);
    state.research.mark_researched(defs().tech_id("city_walls").unwrap());
    // A wall across the whole map at y = 20 with a finished gate at (24, 20).
    for x in 0..48 {
        if x != 24 {
            spawn(&mut state, "wall", x, 20, 0);
        }
    }
    let gate = spawn(&mut state, "gate", 24, 20, 0);
    // The planner routes through the gate.
    let plan = plan_road(&state, defs(), IVec2::new(30, 10), IVec2::new(30, 30), road("path"));
    assert!(plan.valid, "{:?}", plan.problems);
    assert!(plan.positions().contains(&IVec2::new(24, 20)));
    // A road over a plain wall segment is still rejected.
    let events = exec(&mut state, Command::BuildRoad { tiles: line(IVec2::new(30, 17), IVec2::new(30, 23)), road: road("path") });
    assert_eq!(rejection(&events), Some(&RejectReason::Road(RoadProblem::Building { pos: IVec2::new(30, 20) })));
    // Building the road through the gate connects buildings on both sides.
    let north = spawn(&mut state, "house_wood", 23, 10, 0);
    let south = crate::test_support::depot(&state);
    let events = exec(&mut state, Command::BuildRoad { tiles: line(IVec2::new(24, 12), IVec2::new(24, 30)), road: road("path") });
    assert_eq!(rejection(&events), None);
    assert!(state.roads().has_road(IVec2::new(24, 20)));
    assert_eq!(state.buildings().id_at(IVec2::new(24, 20)), Some(gate));
    let depot_entrance = state.building(south).unwrap().entrance();
    let plan = plan_road(&state, defs(), IVec2::new(24, 30), depot_entrance, road("path"));
    assert!(plan.valid, "{:?}", plan.problems);
    exec(&mut state, plan.to_command().unwrap());
    assert!(buildings_connected(state.world(), state.roads(), state.buildings(), north, south));
    // (The fixture map is smaller than any world size, so only the entities are validated.)
    state.validate_entities(defs()).unwrap();
    // Only gates carry roads: the same road under a wall segment is invalid save data.
    let mut broken = state.clone();
    broken.roads.set(IVec2::new(30, 20), Some(RoadTile { road: road("path"), bridge_axis: None }));
    assert_eq!(broken.validate_entities(defs()), Err(crate::state::StateError::InvalidRoad(IVec2::new(30, 20))));
}
