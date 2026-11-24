//! Tests of placement rules, construction, demolition, upgrades, moves, pausing, territory and
//! the scenario start.

use glam::IVec2;

use super::construction::spawn_finished;
use super::*;
use crate::command::{Command, RejectReason};
use crate::defs::{ResourceAmount, ResourceId, ServiceKind};
use crate::economy::Economy;
use crate::event::SimEvent;
use crate::roads::{self, RoadTile};
use crate::state::{GameSettings, GameState};
use crate::territory::Territory;
use crate::test_support::{defs, depot, empty_storages, exec, flat_state, link, rejection, run, supply};
use crate::world::{Deposit, DepositKind, Terrain, TreeKind, Vegetation, WorldSize};

fn kind(key: &str) -> BuildingTypeId {
    defs().building_id(key).unwrap()
}

fn res(key: &str) -> ResourceId {
    defs().resource_id(key).unwrap()
}

fn rot(n: u8) -> Rotation {
    Rotation::new(n).unwrap()
}

fn tech(state: &mut GameState, key: &str) {
    state.research.mark_researched(defs().tech_id(key).unwrap());
}

fn spawn(state: &mut GameState, key: &str, x: i32, y: i32) -> BuildingId {
    spawn_finished(state, defs(), kind(key), IVec2::new(x, y), Rotation::default()).unwrap()
}

/// Spawns a headquarters and connects it to the fixture depot, so the depot's goods stay part
/// of the kingdom stock.
fn spawn_hq(state: &mut GameState, key: &str, x: i32, y: i32) -> BuildingId {
    let hq = spawn(state, key, x, y);
    let d = depot(state);
    link(state, hq, d);
    hq
}

fn errors(state: &GameState, key: &str, x: i32, y: i32) -> Vec<PlacementError> {
    check_placement(state, defs(), kind(key), IVec2::new(x, y), Rotation::default()).errors
}

/// Places a building via command and returns its id (panics on rejection).
fn place(state: &mut GameState, key: &str, x: i32, y: i32, rotation: Rotation) -> BuildingId {
    let events = exec(state, Command::PlaceBuilding { kind: kind(key), origin: IVec2::new(x, y), rotation });
    assert_eq!(rejection(&events), None);
    events
        .iter()
        .find_map(|e| match e {
            SimEvent::BuildingPlaced { building, .. } => Some(*building),
            _ => None,
        })
        .unwrap()
}

fn tree() -> Vegetation {
    Vegetation::Tree { kind: TreeKind::Deciduous, growth: 255 }
}

fn has<F: Fn(&SimEvent) -> bool>(events: &[SimEvent], f: F) -> bool {
    events.iter().any(f)
}

#[test]
fn rotation_sets_footprint_entrance_and_occupancy() {
    let mut state = flat_state(48);
    let check = check_placement(&state, defs(), kind("sawmill"), IVec2::new(10, 10), rot(1));
    assert!(check.ok, "{:?}", check.errors);
    assert_eq!(check.footprint, TileRect::new(IVec2::new(10, 10), IVec2::new(3, 2)));
    assert_eq!(check.entrance, IVec2::new(9, 11));
    assert_eq!(check.tile_status.len(), 6);
    assert!(check.tile_status.iter().all(|(_, s)| *s == TileStatus::Free));
    let id = place(&mut state, "sawmill", 10, 10, rot(1));
    let b = state.building(id).unwrap();
    assert_eq!(b.rect(), check.footprint);
    assert_eq!(b.entrance(), IVec2::new(9, 11));
    assert_eq!(b.door(), IVec2::new(10, 11));
    assert_eq!(b.facing(), crate::grid::Direction::West);
    assert_eq!(b.status(), BuildingStatus::UnderConstruction);
    for p in check.footprint.tiles() {
        assert_eq!(state.buildings().id_at(p), Some(id));
    }
    assert_eq!(state.buildings().id_at(IVec2::new(9, 11)), None);
    assert_eq!(state.buildings().at(IVec2::new(12, 11)).map(|(i, _)| i), Some(id));
    assert_eq!(state.buildings().ids_in(TileRect::new(IVec2::new(8, 8), IVec2::new(4, 4))), vec![id]);
}

#[test]
fn general_placement_errors_are_reported_per_tile() {
    let mut state = flat_state(48);
    // Out of bounds.
    let check = check_placement(&state, defs(), kind("town_hall"), IVec2::new(46, 10), rot(0));
    assert_eq!(check.errors, vec![PlacementError::OutOfBounds]);
    assert_eq!(check.tile_status.iter().filter(|(_, s)| *s == TileStatus::Blocked(TileBlock::OutOfBounds)).count(), 8);
    assert!(!check.ok);
    // Mixed elevation.
    state.world.tile_mut(IVec2::new(11, 11)).elevation = 3;
    let check = check_placement(&state, defs(), kind("house_wood"), IVec2::new(10, 10), rot(0));
    assert_eq!(check.errors, vec![PlacementError::NotFlat]);
    assert!(check.tile_status.contains(&(IVec2::new(11, 11), TileStatus::Blocked(TileBlock::Uneven))));
    // Water and swamp.
    state.world.tile_mut(IVec2::new(20, 10)).terrain = Terrain::ShallowWater;
    assert_eq!(errors(&state, "house_wood", 20, 10), vec![PlacementError::Water]);
    state.world.tile_mut(IVec2::new(25, 10)).terrain = Terrain::Swamp;
    assert_eq!(errors(&state, "house_wood", 24, 9), vec![PlacementError::Swamp]);
    // Overlapping a building or a road.
    spawn(&mut state, "house_wood", 30, 30);
    assert_eq!(errors(&state, "house_wood", 31, 31), vec![PlacementError::OverlapsBuilding]);
    let path = defs().road_id("path").unwrap();
    state.roads.set(IVec2::new(36, 30), Some(RoadTile { road: path, bridge_axis: None }));
    assert_eq!(errors(&state, "house_wood", 35, 30), vec![PlacementError::OverlapsRoad]);
    // Outside the territory.
    state.territory = Territory::new(48, 48);
    state.territory.claim(TileRect::new(IVec2::new(0, 0), IVec2::new(4, 4)), 4);
    assert_eq!(errors(&state, "house_wood", 20, 20), vec![PlacementError::OutsideTerritory]);
    assert!(errors(&state, "house_wood", 1, 1).is_empty());
    // All messages are German sentences.
    for e in [PlacementError::OutOfBounds, PlacementError::NotFlat, PlacementError::OutsideTerritory] {
        assert!(e.to_string().ends_with('.'), "{e}");
    }
}

#[test]
fn research_buildable_flag_max_count_and_cost_are_checked() {
    let mut state = flat_state(48);
    assert!(matches!(errors(&state, "wheat_farm", 10, 10).as_slice(), [PlacementError::NotUnlocked { tech, .. }] if !tech.is_empty()));
    assert!(matches!(errors(&state, "farmhouse", 10, 10).as_slice(), [PlacementError::NotBuildable { .. }]));
    spawn_hq(&mut state, "town_hall", 30, 30);
    assert_eq!(
        errors(&state, "town_hall", 10, 10),
        vec![PlacementError::MaxCountReached { name: "Rathaus".into(), max: 1 }]
    );
    state.economy = Economy::new(defs());
    empty_storages(&mut state);
    let errs = errors(&state, "house_wood", 10, 10);
    assert_eq!(
        errs,
        vec![
            PlacementError::InsufficientGold { needed: 20, available: 0 },
            PlacementError::InsufficientMaterial { name: "Holz".into(), needed: 12, available: 0 },
        ]
    );
    assert_eq!(errs[1].to_string(), "Nicht genug Holz (0 von 12).");
}

#[test]
fn deposit_water_and_farmland_rules_from_data() {
    let mut state = flat_state(48);
    // Quarry: stone within 6 tiles of the 3x3 footprint.
    assert_eq!(
        errors(&state, "quarry", 10, 10),
        vec![PlacementError::DepositMissing { deposit: DepositKind::Stone, max_distance: 6 }]
    );
    state.world.tile_mut(IVec2::new(19, 11)).deposit = Some(Deposit { kind: DepositKind::Stone, amount: 50 });
    assert!(!errors(&state, "quarry", 10, 10).is_empty(), "7 tiles away is too far");
    state.world.tile_mut(IVec2::new(18, 11)).deposit = Some(Deposit { kind: DepositKind::Stone, amount: 50 });
    assert!(errors(&state, "quarry", 10, 10).is_empty());

    // Fishery: water adjacent and fish within 8.
    let errs = errors(&state, "fishery", 30, 30);
    assert!(errs.contains(&PlacementError::WaterMissing { max_distance: 1 }));
    assert!(errs.contains(&PlacementError::DepositMissing { deposit: DepositKind::Fish, max_distance: 8 }));
    let water = IVec2::new(32, 30);
    state.world.tile_mut(water).terrain = Terrain::ShallowWater;
    state.world.tile_mut(water).deposit = Some(Deposit { kind: DepositKind::Fish, amount: 100 });
    let check = check_placement(&state, defs(), kind("fishery"), IVec2::new(30, 30), rot(0));
    assert!(check.ok, "{:?}", check.errors);
    assert_eq!(check.warnings, vec![PlacementWarning::NoRoadAccess]);
    // Facing the water, the entrance can never get a road.
    // Covering another building's entrance is allowed but warned about.
    let house = spawn(&mut state, "house_wood", 40, 10); // entrance (41, 12)
    let blocking = check_placement(&state, defs(), kind("well"), IVec2::new(41, 12), rot(0));
    assert!(blocking.ok && blocking.warnings.contains(&PlacementWarning::BlocksEntrance));
    assert!(!check_placement(&state, defs(), kind("well"), IVec2::new(42, 12), rot(0)).warnings.contains(&PlacementWarning::BlocksEntrance));
    assert!(check_move(&state, defs(), house, IVec2::new(40, 11), rot(0)).unwrap().ok);
    let facing_water = check_placement(&state, defs(), kind("fishery"), IVec2::new(30, 30), rot(3));
    assert_eq!(facing_water.entrance, water);
    assert_eq!(facing_water.entrance_status, EntranceStatus::Blocked);
    assert!(facing_water.warnings.contains(&PlacementWarning::EntranceBlocked));

    // Wheat farm: at least half of its 24 field tiles free and arable within radius 5.
    tech(&mut state, "grain_farming");
    let farm = TileRect::new(IVec2::new(10, 30), IVec2::splat(3));
    for p in farm.expanded(5).tiles() {
        state.world.tile_mut(p).terrain = Terrain::Rock;
    }
    let arable: Vec<IVec2> = farm.expanded(1).tiles().filter(|p| !farm.contains(*p)).collect();
    for p in arable.iter().take(11) {
        state.world.tile_mut(*p).terrain = Terrain::Meadow;
    }
    assert_eq!(errors(&state, "wheat_farm", 10, 30), vec![PlacementError::NotEnoughFarmland { needed: 12, available: 11 }]);
    state.world.tile_mut(arable[11]).terrain = Terrain::Meadow;
    assert!(errors(&state, "wheat_farm", 10, 30).is_empty());
}

#[test]
fn gates_replace_wall_segments() {
    let mut state = flat_state(32);
    tech(&mut state, "city_walls");
    assert_eq!(errors(&state, "gate", 10, 10), vec![PlacementError::NotOnWallLine { name: "Tor".into() }]);
    let wall = spawn(&mut state, "wall", 10, 10);
    let check = check_placement(&state, defs(), kind("gate"), IVec2::new(10, 10), rot(0));
    assert!(check.ok, "{:?}", check.errors);
    assert_eq!(check.replaces, Some(wall));
    assert!(check.warnings.contains(&PlacementWarning::ReplacesWall));
    let events = exec(&mut state, Command::PlaceBuilding { kind: kind("gate"), origin: IVec2::new(10, 10), rotation: rot(0) });
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingDemolished { building, .. } if *building == wall)));
    assert!(state.building(wall).is_none());
    let (_, gate) = state.buildings().at(IVec2::new(10, 10)).unwrap();
    assert_eq!(gate.kind(), kind("gate"));
    // Walls auto-connect.
    spawn(&mut state, "wall", 11, 10);
    spawn(&mut state, "wall", 10, 11);
    assert_eq!(wall_mask(state.buildings(), defs(), IVec2::new(10, 10)), Some(2 | 4));
    assert_eq!(wall_mask(state.buildings(), defs(), IVec2::new(5, 5)), None);
}

#[test]
fn vegetation_is_cleared_with_wood_and_extra_time() {
    let mut state = flat_state(32);
    state.world.tile_mut(IVec2::new(10, 10)).vegetation = tree();
    state.world.tile_mut(IVec2::new(11, 10)).vegetation = Vegetation::Boulder;
    let check = check_placement(&state, defs(), kind("house_wood"), IVec2::new(10, 10), rot(0));
    assert!(check.ok);
    assert_eq!(check.clearing, Clearing { trees: 1, bushes: 0, boulders: 1 });
    assert!(check.tile_status.contains(&(IVec2::new(10, 10), TileStatus::NeedsClearing)));
    assert!(check.warnings.contains(&PlacementWarning::ClearsVegetation { trees: 1, bushes: 0, boulders: 1, wood: 2 }));
    // German text: singular, counts of 0 left out.
    let text = |trees, bushes, boulders, wood| PlacementWarning::ClearsVegetation { trees, bushes, boulders, wood }.to_string();
    assert_eq!(text(1, 0, 1, 2), "Bewuchs wird gerodet (1 Baum, 1 Felsbrocken): +2 Holz, längere Bauzeit.");
    assert_eq!(text(0, 1, 0, 0), "Bewuchs wird gerodet (1 Busch): längere Bauzeit.");
    assert_eq!(text(3, 2, 0, 6), "Bewuchs wird gerodet (3 Bäume, 2 Büsche): +6 Holz, längere Bauzeit.");
    assert!((check.build_hours - 6.0 * 1.2).abs() < 1e-4);
    let wood_before = state.economy().stock().get(res("wood"));
    let events = exec(&mut state, Command::PlaceBuilding { kind: kind("house_wood"), origin: IVec2::new(10, 10), rotation: rot(0) });
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingPlaced { cleared_wood: 2, .. })));
    // The cleared wood is stored at once; the 12 wood for the site stay in stock until carriers
    // pick them up, but are committed to the site.
    assert_eq!(state.economy().stock().get(res("wood")), wood_before + 2);
    assert_eq!(state.economy().committed().get(res("wood")), 12);
    assert_eq!(state.economy().available(res("wood")), wood_before + 2 - 12);
    assert_eq!(state.world().tile(IVec2::new(10, 10)).vegetation, Vegetation::None);
    assert_eq!(state.world().tile(IVec2::new(11, 10)).vegetation, Vegetation::None);
    let (_, b) = state.buildings().at(IVec2::new(10, 10)).unwrap();
    assert_eq!(b.construction_site().unwrap().work_required(), 144_000);
}

#[test]
fn construction_completes_after_the_expected_ticks() {
    let mut state = flat_state(32);
    let gold = state.economy().treasury();
    let id = place(&mut state, "house_wood", 10, 10, rot(0));
    // Gold is paid on placement; the 12 wood must be delivered by carriers.
    assert_eq!(state.economy().treasury(), gold - 20);
    assert_eq!(state.economy().ledger().today().gold(crate::economy::LedgerCategory::Construction), -20);
    let b = state.building(id).unwrap();
    let site = b.construction_site().unwrap();
    assert_eq!(site.delivered(res("wood")), 0);
    assert_eq!(site.gold_paid(), 20);
    assert!(!site.can_progress());
    // 6 builder-hours = 6 * 200 ticks * 100 points.
    assert_eq!(site.work_required(), 120_000);
    // Without delivered material nobody is assigned.
    assert_eq!(b.staffed(), 0);
    assert_eq!(b.workers_wanted(defs()), 0);
    // Without a road the site is never supplied.
    run(&mut state, 400);
    assert_eq!(state.building(id).unwrap().construction_site().unwrap().delivered(res("wood")), 0);
    // Connected to the depot, carriers bring the wood and builders start.
    supply(&mut state, id);
    let b = state.building(id).unwrap();
    let site = b.construction_site().unwrap();
    assert_eq!(site.delivered(res("wood")), 12);
    assert_eq!(state.economy().committed().get(res("wood")), 0);
    run(&mut state, 1);
    let b = state.building(id).unwrap();
    assert_eq!(b.staffed(), 2, "a 2x2 site gets 2 builders");
    assert_eq!(work_per_builder_tick(&state, defs()), WORK_PER_BUILDER_TICK);
    // The builders walk to the site; once both are there they add 2 × 100 points per tick at
    // the shift factor 3 (build times are round-the-clock times, docs §6).
    crate::test_support::run_until_at_work(&mut state);
    let b = state.building(id).unwrap();
    let left = b.construction_site().unwrap().remaining_work();
    let ticks = u64::from(left.div_ceil(600));
    let events = run(&mut state, ticks - 1);
    let b = state.building(id).unwrap();
    assert!(b.construction_site().unwrap().is_started());
    let rest = b.construction_site().unwrap().remaining_work();
    assert!((1..=600).contains(&rest), "{rest}");
    assert_eq!(b.construction_site().unwrap().stage(), 2);
    assert_eq!(b.sprite(defs()), "buildings/construction_2x2_2");
    assert!(!has(&events, |e| matches!(e, SimEvent::BuildingCompleted { .. })));
    let events = run(&mut state, 1);
    let b = state.building(id).unwrap();
    assert_eq!(b.status(), BuildingStatus::Active);
    assert!(b.is_complete() && b.is_operational());
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingCompleted { building, .. } if *building == id)));
    assert!(events.iter().filter_map(SimEvent::notification).any(|n| n.text == "Fertiggestellt: Einfaches Holzhaus"));
    assert_eq!(b.sprite(defs()), "buildings/house_wood");
    // Houses have no jobs: the builders are released; only the depot's carriers work.
    assert_eq!(b.staffed(), 0);
    assert_eq!(state.workforce().employed(), 3);
}

#[test]
fn construction_started_is_emitted_once_builders_work() {
    let mut state = flat_state(32);
    let events = exec(&mut state, Command::PlaceBuilding { kind: kind("well"), origin: IVec2::new(20, 20), rotation: rot(0) });
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingPlaced { .. })));
    assert!(!has(&events, |e| matches!(e, SimEvent::ConstructionStarted { .. })), "no material yet");
    let (well, _) = state.buildings().at(IVec2::new(20, 20)).unwrap();
    let mut started = 0;
    let d = depot(&state);
    link(&mut state, well, d);
    for _ in 0..2000 {
        let events = run(&mut state, 1);
        started += events.iter().filter(|e| matches!(e, SimEvent::ConstructionStarted { .. })).count();
        if state.building(well).unwrap().is_complete() {
            break;
        }
    }
    assert_eq!(started, 1, "emitted once, when builders first work");
    assert!(state.building(well).unwrap().is_complete());
    // Without workers nothing happens even with material on site.
    let mut idle = flat_state(32);
    crate::test_support::set_adults(&mut idle, 0);
    exec(&mut idle, Command::PlaceBuilding { kind: kind("well"), origin: IVec2::new(20, 20), rotation: rot(0) });
    let (site, _) = idle.buildings().at(IVec2::new(20, 20)).unwrap();
    if let Some(s) = idle.buildings.get_mut(site).unwrap().construction_site_mut() {
        s.deliver(res("stone"), 5);
    }
    idle.workforce.mark_dirty();
    let events = run(&mut idle, 500);
    assert!(!has(&events, |e| matches!(e, SimEvent::ConstructionStarted { .. })));
    assert_eq!(idle.building(site).unwrap().construction_site().unwrap().work_done(), 0);
}

#[test]
fn winter_and_research_change_the_work_rate() {
    let mut state = flat_state(16);
    state.clock = crate::time::GameClock::at_tick(3 * crate::time::TICKS_PER_SEASON);
    assert_eq!(work_per_builder_tick(&state, defs()), 85);
    tech(&mut state, "architecture_1");
    assert_eq!(work_per_builder_tick(&state, defs()), 93);
    state.clock = crate::time::GameClock::new();
    assert_eq!(work_per_builder_tick(&state, defs()), 110);
}

#[test]
fn cancelling_refunds_the_unused_part() {
    let mut state = flat_state(32);
    let gold = state.economy().treasury();
    let wood = state.economy().stock().get(res("wood"));
    let id = place(&mut state, "house_wood", 10, 10, rot(0));
    supply(&mut state, id);
    // Run until half the work is done (2 builders at 600 points per tick once both are there).
    let site = |s: &GameState| s.building(id).unwrap().construction_site().unwrap().clone();
    crate::test_support::run_until(&mut state, 2 * crate::time::TICKS_PER_DAY, |s| site(s).work_done() >= 60_000);
    let done = site(&state);
    assert!((60_000..60_600).contains(&done.work_done()), "{}", done.work_done());
    let unused = done.unused_materials();
    let kept = 12 - unused.iter().map(|m| m.amount).sum::<u32>();
    assert_eq!(unused, vec![ResourceAmount { resource: res("wood"), amount: 12 - kept }]);
    assert!((6..=7).contains(&kept), "about half the wood was used: {kept}");
    let gold_back = done.unused_gold();
    assert!((9..=10).contains(&gold_back), "about half the gold comes back: {gold_back}");
    let events = exec(&mut state, Command::CancelConstruction { building: id });
    assert!(has(&events, |e| matches!(e, SimEvent::ConstructionCancelled { refund: r, .. } if *r == unused)));
    assert!(state.building(id).is_none());
    assert_eq!(state.buildings().id_at(IVec2::new(10, 10)), None);
    // The unused gold came back (wages and upkeep were paid meanwhile, so check the ledger).
    let construction = |s: &GameState| s.economy().ledger().today().gold(crate::economy::LedgerCategory::Construction);
    let paid = 20 - i64::from(gold_back);
    assert_eq!(construction(&state), -paid);
    assert!(state.economy().treasury() <= gold - paid);
    assert_eq!(state.economy().stock().get(res("wood")), wood - kept);

    // Cancelled right away: everything comes back (nothing was delivered yet; wood already
    // on its way returns to the depot).
    let id = place(&mut state, "house_wood", 10, 10, rot(0));
    assert_eq!(state.building(id).unwrap().construction_site().unwrap().outstanding(res("wood")), 12);
    let events = exec(&mut state, Command::CancelConstruction { building: id });
    assert_eq!(rejection(&events), None);
    crate::test_support::run_until(&mut state, crate::time::TICKS_PER_DAY, |s| s.logistics().jobs().is_empty());
    assert_eq!(construction(&state), -paid);
    assert_eq!(state.economy().stock().get(res("wood")), wood - kept);
    assert_eq!(state.economy().committed().get(res("wood")), 0);
    assert_eq!(state.economy().committed().get(res("wood")), 0);
    // A stale id is rejected.
    let events = exec(&mut state, Command::CancelConstruction { building: id });
    assert_eq!(rejection(&events), Some(&RejectReason::UnknownBuilding));
    // Finished buildings cannot be "cancelled".
    let house = spawn(&mut state, "house_wood", 20, 20);
    let events = exec(&mut state, Command::CancelConstruction { building: house });
    assert_eq!(rejection(&events), Some(&RejectReason::NotUnderConstruction));
}

#[test]
fn demolishing_refunds_half_the_materials() {
    let mut state = flat_state(48);
    let hall = spawn_hq(&mut state, "town_hall", 30, 30);
    let house = spawn(&mut state, "house_wood", 10, 10);
    let gold = state.economy().treasury();
    let wood = state.economy().stock().get(res("wood"));
    state.buildings.get_mut(house).unwrap().inventory.set_capacities([(res("bread"), 10)]);
    state.buildings.get_mut(house).unwrap().inventory.add(res("bread"), 3);
    // The preview the UI shows before demolishing is what the command returns.
    let preview = super::demolish_refund(&state, defs(), house);
    let events = exec(&mut state, Command::DemolishBuilding { building: house });
    let expected = vec![
        ResourceAmount { resource: res("wood"), amount: 6 },
        ResourceAmount { resource: res("bread"), amount: 3 },
    ];
    let mut expected_sorted = expected.clone();
    expected_sorted.sort_by_key(|m| m.resource);
    assert_eq!(preview, Ok(super::DemolishRefund { gold: 0, materials: expected_sorted.clone(), cancels_site: false }));
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingDemolished { building, refund, .. } if *building == house && *refund == expected_sorted)));
    assert_eq!(state.economy().treasury(), gold, "no gold refund");
    assert_eq!(state.economy().stock().get(res("wood")), wood + 6);
    assert!(state.building(house).is_none());
    assert_eq!(state.buildings().id_at(IVec2::new(11, 11)), None);
    // The only headquarters stays.
    assert_eq!(super::demolish_refund(&state, defs(), hall), Err(RejectReason::LastHeadquarters));
    let events = exec(&mut state, Command::DemolishBuilding { building: hall });
    assert_eq!(rejection(&events), Some(&RejectReason::LastHeadquarters));
    assert!(rejection(&events).unwrap().to_string().contains("Hauptquartier"));
    // A site is cancelled instead (full refund of the unused part).
    let site = place(&mut state, "house_wood", 10, 10, rot(0));
    let preview = super::demolish_refund(&state, defs(), site).unwrap();
    assert!(preview.cancels_site && preview.gold > 0);
    let events = exec(&mut state, Command::DemolishBuilding { building: site });
    assert!(has(&events, |e| matches!(e, SimEvent::ConstructionCancelled { refund, .. } if *refund == preview.materials)));
}

#[test]
fn territory_follows_headquarters_and_watchtowers() {
    let mut state = flat_state(96);
    state.territory = Territory::new(96, 96);
    assert_eq!(errors(&state, "house_wood", 40, 40), vec![PlacementError::OutsideTerritory]);
    let hall = spawn(&mut state, "town_hall", 30, 30);
    state.territory.recompute(&state.buildings, defs(), &state.diplomacy.land);
    // The far-away depot is not connected: stock the town hall instead.
    for r in ["wood", "stone", "planks"] {
        state.buildings.get_mut(hall).unwrap().inventory.add(res(r), 100);
    }
    crate::economy::storage::refresh_stock(&mut state, defs());
    let owned = state.territory().owned_tiles();
    assert!(state.territory().contains(IVec2::new(32 + 19, 32)));
    assert!(!state.territory().contains(IVec2::new(32 + 22, 32)));
    assert!(errors(&state, "house_wood", 40, 40).is_empty());
    assert_eq!(errors(&state, "house_wood", 60, 32), vec![PlacementError::OutsideTerritory]);
    // A watchtower at the border extends the territory by 8 once finished.
    tech(&mut state, "militia_training");
    let tower = place(&mut state, "watchtower", 50, 32, rot(0));
    assert_eq!(state.territory().owned_tiles(), owned, "sites claim nothing");
    assert_eq!(state.building(tower).unwrap().construction_site().unwrap().work_required(), 80_000);
    link(&mut state, tower, hall);
    // 4 builder-hours, one builder on a 1x1 site: 800 ticks once the material is there.
    let mut events = Vec::new();
    for _ in 0..3000 {
        events.extend(run(&mut state, 1));
        if state.building(tower).unwrap().is_complete() {
            break;
        }
    }
    assert!(state.building(tower).unwrap().is_complete());
    assert!(has(&events, |e| matches!(e, SimEvent::TerritoryChanged { .. })));
    assert!(state.territory().contains(IVec2::new(57, 32)));
    assert!(state.territory().owned_tiles() > owned);
    assert!(errors(&state, "house_wood", 55, 32).is_empty());
    // Demolishing it shrinks the territory again.
    let events = exec(&mut state, Command::DemolishBuilding { building: tower });
    assert!(has(&events, |e| matches!(e, SimEvent::TerritoryChanged { owned_tiles } if *owned_tiles == owned)));
}

#[test]
fn moving_keeps_id_and_contents_and_takes_30_percent() {
    let mut state = flat_state(48);
    let cutter = spawn(&mut state, "woodcutter", 10, 10);
    let hall = spawn_hq(&mut state, "town_hall", 30, 30);
    state.buildings.get_mut(cutter).unwrap().production.as_mut().unwrap().output.add(res("wood"), 5);
    let check = check_move(&state, defs(), cutter, IVec2::new(20, 10), rot(2)).unwrap();
    assert!(check.ok && check.cost == crate::defs::Cost::default());
    assert!((check.build_hours - 0.9).abs() < 1e-4);
    // Overlapping its own old tiles is fine.
    assert!(check_move(&state, defs(), cutter, IVec2::new(11, 10), rot(0)).unwrap().ok);
    let events = exec(&mut state, Command::MoveBuilding { building: cutter, origin: IVec2::new(20, 10), rotation: rot(2) });
    assert!(has(&events, |e| matches!(e, SimEvent::MoveStarted { building, to, .. } if *building == cutter && *to == IVec2::new(20, 10))));
    let b = state.building(cutter).unwrap();
    assert_eq!(b.status(), BuildingStatus::Moving);
    assert_eq!(b.rotation(), rot(2));
    assert_eq!(b.production().unwrap().output().get(res("wood")), 5);
    assert_eq!(b.construction_site().unwrap().work_required(), 18_000);
    assert_eq!(state.buildings().id_at(IVec2::new(10, 10)), None);
    assert_eq!(state.buildings().id_at(IVec2::new(21, 11)), Some(cutter));
    // Moves cannot be cancelled, and sites cannot be moved.
    let events = exec(&mut state, Command::CancelConstruction { building: cutter });
    assert_eq!(rejection(&events), Some(&RejectReason::CannotCancelMove));
    let events = exec(&mut state, Command::MoveBuilding { building: cutter, origin: IVec2::new(5, 5), rotation: rot(0) });
    assert_eq!(rejection(&events), Some(&RejectReason::NotMovable));
    // 18 000 points; the 2 builders walk over and add 600 points per tick at the site.
    let mut events = Vec::new();
    for _ in 0..crate::time::TICKS_PER_DAY {
        events.extend(run(&mut state, 1));
        if state.building(cutter).unwrap().status() == BuildingStatus::Active {
            break;
        }
    }
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingMoved { building, .. } if *building == cutter)));
    assert_eq!(state.building(cutter).unwrap().status(), BuildingStatus::Active);
    // Headquarters stay where they are; targets must be valid.
    assert_eq!(super::move_allowed(&state, defs(), hall), Err(RejectReason::CannotMoveHeadquarters));
    assert_eq!(super::move_allowed(&state, defs(), cutter), Ok(()));
    let events = exec(&mut state, Command::MoveBuilding { building: hall, origin: IVec2::new(5, 30), rotation: rot(0) });
    assert_eq!(rejection(&events), Some(&RejectReason::CannotMoveHeadquarters));
    let events = exec(&mut state, Command::MoveBuilding { building: cutter, origin: IVec2::new(31, 31), rotation: rot(0) });
    assert_eq!(rejection(&events), Some(&RejectReason::Placement(PlacementError::OverlapsBuilding)));
}

#[test]
fn pausing_releases_workers_and_stops_construction() {
    let mut state = flat_state(32);
    let cutter = spawn(&mut state, "woodcutter", 10, 10);
    run(&mut state, 1);
    assert_eq!(state.building(cutter).unwrap().staffed(), 2);
    let events = exec(&mut state, Command::SetBuildingPaused { building: cutter, paused: true });
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingPauseChanged { paused: true, .. })));
    let b = state.building(cutter).unwrap();
    assert_eq!((b.status(), b.staffed()), (BuildingStatus::Paused, 0));
    assert!(!b.is_operational());
    // Repeating the same state is a no-op.
    assert!(exec(&mut state, Command::SetBuildingPaused { building: cutter, paused: true }).is_empty());
    exec(&mut state, Command::SetBuildingPaused { building: cutter, paused: false });
    assert_eq!(state.building(cutter).unwrap().staffed(), 2);
    // Houses cannot be paused, inhabited or empty; the query the UI uses agrees.
    let house = spawn(&mut state, "house_wood", 20, 20);
    let events = exec(&mut state, Command::SetBuildingPaused { building: house, paused: true });
    assert_eq!(rejection(&events), Some(&RejectReason::CannotPauseHousing));
    assert_eq!(super::pause_allowed(&state, defs(), cutter), Ok(()));
    let mut empty = state.clone();
    crate::test_support::set_adults(&mut empty, 0);
    run(&mut empty, 1);
    assert_eq!(empty.population.resident_count(house), 0);
    assert_eq!(super::pause_allowed(&empty, defs(), house), Err(RejectReason::CannotPauseHousing));
    let events = exec(&mut empty, Command::SetBuildingPaused { building: house, paused: true });
    assert_eq!(rejection(&events), Some(&RejectReason::CannotPauseHousing));
    assert_eq!(RejectReason::CannotPauseHousing.to_string(), "Wohnhäuser können nicht stillgelegt werden.");
    // A paused site makes no progress.
    let site = place(&mut state, "house_wood", 20, 10, rot(0));
    supply(&mut state, site);
    exec(&mut state, Command::SetBuildingPaused { building: site, paused: true });
    let done = state.building(site).unwrap().construction_site().unwrap().work_done();
    run(&mut state, 50);
    assert_eq!(state.building(site).unwrap().construction_site().unwrap().work_done(), done);
    assert!(state.building(site).unwrap().is_paused());
    exec(&mut state, Command::SetBuildingPaused { building: site, paused: false });
    crate::test_support::run_until(&mut state, crate::time::TICKS_PER_DAY, |s| {
        s.building(site).unwrap().construction_site().unwrap().work_done() > done
    });
}

#[test]
fn house_upgrade_requires_services_and_takes_the_target_build_time() {
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 10, 10);
    assert!(upgrade_blockers(&state, defs(), house).unwrap().contains(&UpgradeBlocker::NoResidents));
    // Locked: the hourly automatic upgrade must not start it before the manual command.
    exec(&mut state, Command::SetUpgradeLocked { building: house, locked: true });
    // Settlers move in; without water nearby their thirst drops below the threshold.
    run(&mut state, 6 * crate::time::TICKS_PER_HOUR);
    assert_eq!(state.population().resident_count(house), 5);
    let blockers = upgrade_blockers(&state, defs(), house).unwrap();
    assert!(blockers.contains(&UpgradeBlocker::MissingService { service: ServiceKind::Market }));
    assert!(blockers.contains(&UpgradeBlocker::UnmetNeed { need: crate::defs::NeedKind::Thirst }));
    assert!(!blockers.contains(&UpgradeBlocker::UnmetNeed { need: crate::defs::NeedKind::Hunger }), "fed from the depot");
    let events = exec(&mut state, Command::UpgradeBuilding { building: house });
    assert!(matches!(rejection(&events), Some(RejectReason::Upgrade(_))));
    // Market (staffed) and well in range, food in stock: thirst recovers within hours.
    spawn(&mut state, "market", 16, 10);
    spawn(&mut state, "well", 12, 14);
    run(&mut state, 5 * crate::time::TICKS_PER_HOUR);
    assert_eq!(upgrade_blockers(&state, defs(), house).unwrap(), Vec::<UpgradeBlocker>::new());
    let gold = state.economy().treasury();
    let events = exec(&mut state, Command::UpgradeBuilding { building: house });
    assert!(has(&events, |e| matches!(e, SimEvent::UpgradeStarted { building, to, .. } if *building == house && *to == kind("farmhouse"))));
    assert_eq!(state.economy().treasury(), gold - 40);
    assert_eq!(state.economy().committed().get(res("planks")), 10);
    let b = state.building(house).unwrap();
    assert_eq!(b.status(), BuildingStatus::Upgrading);
    assert!(b.is_complete(), "the house stays usable while upgrading");
    assert_eq!(b.housing_tier(defs()), Some(1));
    assert_eq!(state.buildings().count_of(kind("farmhouse")), 1);
    // Carriers of a nearby warehouse bring 10 planks and 6 loam; then 8 builder-hours with 2
    // builders.
    let store = spawn(&mut state, "warehouse", 20, 16);
    for r in ["planks", "loam"] {
        state.buildings.get_mut(store).unwrap().inventory.add(res(r), 50);
    }
    link(&mut state, house, store);
    for _ in 0..2 * crate::time::TICKS_PER_DAY {
        if state.building(house).unwrap().construction_site().unwrap().fully_supplied() {
            break;
        }
        run(&mut state, 1);
    }
    run(&mut state, 1);
    assert_eq!(state.building(house).unwrap().staffed(), 2);
    let mut events = Vec::new();
    for _ in 0..crate::time::TICKS_PER_DAY {
        events.extend(run(&mut state, 1));
        if state.building(house).unwrap().status() == BuildingStatus::Active {
            break;
        }
    }
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingUpgraded { building, .. } if *building == house)));
    let b = state.building(house).unwrap();
    assert_eq!(b.kind(), kind("farmhouse"));
    assert_eq!(b.housing_tier(defs()), Some(2));
    assert_eq!(b.status(), BuildingStatus::Active);
}

#[test]
fn upgrades_can_be_cancelled_and_need_space_for_larger_tiers() {
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 10, 10);
    spawn(&mut state, "market", 16, 10);
    spawn(&mut state, "well", 12, 14);
    run(&mut state, 1);
    let gold = state.economy().treasury();
    exec(&mut state, Command::UpgradeBuilding { building: house });
    let events = exec(&mut state, Command::CancelConstruction { building: house });
    assert!(has(&events, |e| matches!(e, SimEvent::UpgradeCancelled { building, .. } if *building == house)));
    let b = state.building(house).unwrap();
    assert_eq!((b.kind(), b.status()), (kind("house_wood"), BuildingStatus::Active));
    assert_eq!(state.economy().treasury(), gold, "unused upgrade cost refunded");

    // Townhouse (2x2) -> residence (3x3) grows backwards, keeping the front edge.
    let town = spawn(&mut state, "townhouse", 30, 30);
    let blockers = upgrade_blockers(&state, defs(), town).unwrap();
    assert!(!blockers.contains(&UpgradeBlocker::NoSpace));
    let path = defs().road_id("path").unwrap();
    state.roads.set(IVec2::new(32, 29), Some(RoadTile { road: path, bridge_axis: None }));
    let blockers = upgrade_blockers(&state, defs(), town).unwrap();
    assert!(blockers.contains(&UpgradeBlocker::NoSpace));
    assert!(blockers.iter().any(|b| matches!(b, UpgradeBlocker::NotUnlocked { .. })));
    // Top tier and non-housing buildings cannot be upgraded.
    let manor = spawn(&mut state, "manor", 5, 30);
    assert_eq!(upgrade_blockers(&state, defs(), manor).unwrap(), vec![UpgradeBlocker::MaxTier]);
    let well = spawn(&mut state, "well", 40, 5);
    assert_eq!(upgrade_blockers(&state, defs(), well).unwrap(), vec![UpgradeBlocker::NoUpgrade]);
    let castle = spawn(&mut state, "castle", 20, 5);
    assert_eq!(upgrade_blockers(&state, defs(), castle).unwrap(), vec![UpgradeBlocker::MaxTier], "end of the chain");
}

/// Runs until the upgrade of `id` is done (materials from the depot); returns the events.
fn finish_upgrade(state: &mut GameState, id: BuildingId) -> Vec<SimEvent> {
    supply(state, id);
    let mut events = Vec::new();
    for _ in 0..4 * crate::time::TICKS_PER_DAY {
        events.extend(run(state, 1));
        if !state.building(id).unwrap().is_upgrading() {
            return events;
        }
    }
    panic!("upgrade of {id:?} did not finish");
}

#[test]
fn workshop_upgrade_keeps_buffers_and_orders_and_rests_meanwhile() {
    let mut state = flat_state(48);
    // Within the depot's carrier radius (the depot stands at (45, 45)).
    let mill = spawn(&mut state, "sawmill", 30, 30);
    let d = depot(&state);
    link(&mut state, mill, d);
    let blockers = upgrade_blockers(&state, defs(), mill).unwrap();
    assert!(blockers.iter().any(|b| matches!(b, UpgradeBlocker::NotUnlocked { tech, .. } if tech == "Zunftwesen")), "{blockers:?}");
    tech(&mut state, "guild_system");
    // Production with stock in the buffers and a finite order.
    exec(&mut state, Command::ClearQueue { building: mill });
    exec(&mut state, Command::QueueProduction { building: mill, recipe: 0, units: Some(30) });
    {
        let p = state.buildings.get_mut(mill).unwrap().production.as_mut().unwrap();
        p.input.add(res("wood"), 6);
        p.output.add(res("planks"), 3);
    }
    run(&mut state, 1);
    assert_eq!(upgrade_blockers(&state, defs(), mill).unwrap(), Vec::<UpgradeBlocker>::new());
    let gold = state.economy().treasury();
    let events = exec(&mut state, Command::UpgradeBuilding { building: mill });
    assert!(has(&events, |e| matches!(e, SimEvent::UpgradeStarted { to, .. } if *to == kind("large_sawmill"))));
    assert_eq!(state.economy().treasury(), gold - 80, "upgrade cost = the target's cost (no own cost given)");
    run(&mut state, 2);
    let b = state.building(mill).unwrap();
    assert_eq!(b.production().unwrap().status(), crate::production::ProductionStatus::Upgrading);
    assert_eq!(b.trade_staff(), 0, "builders hold the slots");
    let events = finish_upgrade(&mut state, mill);
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingUpgraded { building, .. } if *building == mill)));
    let b = state.building(mill).unwrap();
    assert_eq!(b.kind(), kind("large_sawmill"));
    let p = b.production().unwrap();
    assert!(p.input().get(res("wood")) >= 2, "input buffer kept");
    assert_eq!(p.output().capacity(res("planks")), 16, "larger buffer of level 2");
    assert_eq!(p.queue().first().map(|e| e.recipe), Some(0), "orders kept");
    assert!(matches!(p.queue().first().and_then(|e| e.remaining), Some(n) if n <= 30));
    assert_eq!(b.workers_wanted(defs()), 3);
}

#[test]
fn storehouse_becomes_warehouse_with_its_stock_and_town_hall_becomes_castle() {
    let mut state = flat_state(64);
    // Within the depot's carrier radius (the depot stands at (61, 61)).
    let store = spawn(&mut state, "storehouse", 40, 50);
    state.buildings.get_mut(store).unwrap().inventory.add(res("bread"), 40);
    let gold = state.economy().treasury();
    let events = exec(&mut state, Command::UpgradeBuilding { building: store });
    assert_eq!(rejection(&events), None);
    assert_eq!(state.economy().treasury(), gold - 50, "own upgrade cost instead of the warehouse's 80");
    assert!(state.building(store).unwrap().inventory().get(res("bread")) >= 30, "stock stays while upgrading");
    finish_upgrade(&mut state, store);
    let b = state.building(store).unwrap();
    assert_eq!((b.kind(), b.size()), (kind("warehouse"), IVec2::new(3, 3)));
    // The settlers eat from it meanwhile; the rest is kept, now with the warehouse's room.
    assert!(b.inventory().get(res("bread")) > 0);
    assert_eq!(b.inventory().capacity(res("bread")), 400);
    assert_eq!(b.inventory().capacity(res("stone")), 400, "the warehouse takes every resource");

    // Town hall -> castle: research, the castle limit, a larger territory.
    let hall = spawn_hq(&mut state, "town_hall", 50, 42);
    let blockers = upgrade_blockers(&state, defs(), hall).unwrap();
    assert!(blockers.iter().any(|b| matches!(b, UpgradeBlocker::NotUnlocked { .. })));
    tech(&mut state, "fortification");
    let other = spawn(&mut state, "castle", 5, 40);
    let blockers = upgrade_blockers(&state, defs(), hall).unwrap();
    assert!(blockers.contains(&UpgradeBlocker::MaxCountReached { name: "Burg".into(), max: 1 }), "{blockers:?}");
    exec(&mut state, Command::DemolishBuilding { building: other });
    for r in ["stone", "planks", "iron_bars"] {
        state.buildings.get_mut(depot(&state)).unwrap().inventory.add(res(r), 200);
    }
    crate::economy::storage::refresh_stock(&mut state, defs());
    assert_eq!(upgrade_blockers(&state, defs(), hall).unwrap(), Vec::<UpgradeBlocker>::new());
    exec(&mut state, Command::UpgradeBuilding { building: hall });
    assert_eq!(state.buildings().count_of(kind("castle")), 1);
    assert!(crate::economy::storage::main_headquarters(&state, defs()) == Some(hall), "still the seat while upgrading");
    finish_upgrade(&mut state, hall);
    let b = state.building(hall).unwrap();
    assert_eq!((b.kind(), b.size()), (kind("castle"), IVec2::new(5, 5)));
    assert_eq!(b.workers_wanted(defs()), 4);
}

#[test]
fn upgrade_start_and_cancel_move_the_claim_with_the_footprint() {
    let mut state = flat_state(64);
    let hall = spawn_hq(&mut state, "town_hall", 30, 30);
    tech(&mut state, "fortification");
    let derived = |s: &GameState| Territory::compute_without_foreign(&s.buildings, defs(), None, &s.diplomacy.land);
    // Spawning does not claim; start from the territory a loaded game would derive.
    state.territory = derived(&state);
    let before = state.territory.clone();
    let events = exec(&mut state, Command::UpgradeBuilding { building: hall });
    assert_eq!(rejection(&events), None);
    assert_eq!(state.territory, derived(&state), "claim follows the grown 5×5 footprint at once");
    assert_ne!(state.territory, before);
    assert!(has(&events, |e| matches!(e, SimEvent::TerritoryChanged { .. })));
    let events = exec(&mut state, Command::CancelConstruction { building: hall });
    assert!(has(&events, |e| matches!(e, SimEvent::UpgradeCancelled { .. })));
    assert_eq!(state.territory, derived(&state));
    assert_eq!(state.territory, before, "the old claim is back");
}

#[test]
fn upgrading_the_only_market_warns_about_the_houses_it_leaves() {
    let mut state = flat_state(48);
    spawn(&mut state, "house_wood", 10, 10);
    let market = spawn(&mut state, "market", 16, 10);
    run(&mut state, 2);
    state.buildings.get_mut(market).unwrap().staffed = 1;
    let gaps = upgrade_service_gaps(&state, defs(), market);
    assert_eq!(gaps, vec![ServiceGap { service: ServiceKind::Market, houses: 1 }]);
    assert_eq!(gaps[0].text(), "Während des Ausbaus fehlt einem Haus die Versorgung „Markt“.");
    assert_eq!(ServiceGap { service: ServiceKind::Market, houses: 3 }.text(), "Während des Ausbaus fehlt 3 Häusern die Versorgung „Markt“.");
    // A second market in reach keeps the house supplied.
    let other = spawn(&mut state, "market", 16, 16);
    state.buildings.get_mut(other).unwrap().staffed = 1;
    assert!(upgrade_service_gaps(&state, defs(), market).is_empty());
}

#[test]
fn buildings_roundtrip_rebuilds_the_occupancy_grid() {
    let mut state = flat_state(32);
    let a = spawn(&mut state, "house_wood", 3, 3);
    let b = place(&mut state, "sawmill", 10, 10, rot(1));
    let config = bincode::config::standard();
    let bytes = bincode::serde::encode_to_vec(&state.buildings, config).unwrap();
    let (back, _): (Buildings, usize) = bincode::serde::decode_from_slice(&bytes, config).unwrap();
    assert_eq!(back.id_at(IVec2::new(4, 4)), Some(a));
    assert_eq!(back.id_at(IVec2::new(12, 11)), Some(b));
    assert_eq!(back.len(), 3, "depot, house and sawmill site");
    // Overlapping footprints in damaged data are rejected.
    let mut broken = state.buildings.clone();
    let def = defs().building_by_key("house_wood").unwrap();
    broken.arena.insert(Building::new(def, IVec2::new(3, 3), Rotation::default(), BuildingPhase::Active));
    let bytes = bincode::serde::encode_to_vec(&broken, config).unwrap();
    assert!(bincode::serde::decode_from_slice::<Buildings, _>(&bytes, config).is_err());
    // Absurd grid sizes are rejected before the grid is allocated.
    for (w, h) in [(i32::MAX, i32::MAX), (100_000, 100_000), (0, 32), (32, -1)] {
        let mut huge = Buildings::new(1, 1);
        huge.width = w;
        huge.height = h;
        let bytes = bincode::serde::encode_to_vec(&huge, config).unwrap();
        let err = bincode::serde::decode_from_slice::<Buildings, _>(&bytes, config).unwrap_err();
        assert!(err.to_string().contains("outside"), "{w}x{h}: {err}");
    }
}

#[test]
fn rejected_commands_carry_german_reasons() {
    let mut state = flat_state(32);
    let cmd = Command::PlaceBuilding { kind: kind("town_hall"), origin: IVec2::new(30, 30), rotation: rot(0) };
    let events = exec(&mut state, cmd.clone());
    let reason = RejectReason::Placement(PlacementError::OutOfBounds);
    assert!(events.contains(&SimEvent::CommandRejected { command: cmd, reason: reason.clone() }));
    assert_eq!(reason.to_string(), "Das Gebäude ragt über den Kartenrand hinaus.");
    let bogus = BuildingTypeId::from_index(999).unwrap();
    let events = exec(&mut state, Command::PlaceBuilding { kind: bogus, origin: IVec2::ZERO, rotation: rot(0) });
    assert_eq!(rejection(&events), Some(&RejectReason::Placement(PlacementError::UnknownBuildingType)));
}

#[test]
fn scenario_start_is_valid_for_many_seeds() {
    let d = defs();
    let scenario = d.scenario(d.default_scenario());
    for seed in [1, 2, 3, 7, 42, 99, 1234, 4242] {
        let state = GameState::new(d, GameSettings::new(d, seed, WorldSize::Small));
        state.validate(d).unwrap();
        assert_eq!(state.buildings().len(), scenario.start_buildings.len(), "seed {seed}");
        let ids: Vec<BuildingId> = state.buildings().iter().map(|(id, _)| id).collect();
        let hall = ids[0];
        assert_eq!(state.building(hall).unwrap().kind(), kind("town_hall"));
        assert!(state.building(hall).unwrap().rect().distance_to(state.world().start) <= 2, "seed {seed}");
        for (i, &id) in ids.iter().enumerate() {
            let b = state.building(id).unwrap();
            assert!(b.is_complete(), "seed {seed}");
            assert_eq!(b.kind(), scenario.start_buildings[i].building);
            assert!(state.territory().contains_rect(b.rect()), "seed {seed}");
            assert!(state.has_road_access(id), "seed {seed}: building {i} has no road access");
            assert!(roads::buildings_connected(state.world(), state.roads(), state.buildings(), id, hall), "seed {seed}: {i}");
        }
        assert!(!state.roads().is_empty());
        assert_eq!(state.economy().treasury(), i64::from(scenario.gold));
        for m in &scenario.stock {
            assert_eq!(state.economy().stock().get(m.resource), m.amount, "seed {seed}");
        }
        assert_eq!(state.workforce().population(), u32::from(scenario.settlers), "seed {seed}");
        assert_eq!(state.population().len(), usize::from(scenario.settlers));
        state.validate(d).unwrap();
        assert_eq!(state.workforce().employed(), 2, "town hall workers");
        assert_eq!(state.territory().owned_tiles(), {
            let t = Territory::compute(state.buildings(), d, None);
            t.owned_tiles()
        });
    }
}

#[test]
fn demolishing_an_upgrading_house_refunds_half_and_the_unused_upgrade() {
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 10, 10);
    spawn(&mut state, "market", 16, 10);
    spawn(&mut state, "well", 12, 14);
    run(&mut state, 1);
    let events = exec(&mut state, Command::UpgradeBuilding { building: house });
    assert_eq!(rejection(&events), None);
    let wood = state.economy().stock().get(res("wood"));
    let gold = state.economy().treasury();
    let preview = super::demolish_refund(&state, defs(), house).unwrap();
    let events = exec(&mut state, Command::DemolishBuilding { building: house });
    let expected = vec![ResourceAmount { resource: res("wood"), amount: 6 }];
    assert_eq!((preview.gold, preview.materials.clone(), preview.cancels_site), (40, expected.clone(), false));
    assert!(has(&events, |e| matches!(e, SimEvent::BuildingDemolished { building, refund, .. } if *building == house && *refund == expected)));
    assert_eq!(state.economy().stock().get(res("wood")), wood + 6, "the reported refund is stored");
    assert_eq!(state.economy().treasury(), gold + 40, "the unused upgrade gold comes back");
    assert_eq!(state.economy().committed().get(res("planks")), 0);
}

#[test]
fn manor_upgrade_keeps_the_entrance_and_gets_supplied() {
    let mut state = flat_state(64);
    tech(&mut state, "representative_buildings");
    // Within the depot's carrier radius, all services in range.
    let residence = spawn(&mut state, "residence", 44, 44);
    for (key, x, y) in
        [("market", 50, 38), ("church", 38, 38), ("school", 50, 44), ("hospital", 38, 44), ("tavern", 44, 36), ("well", 42, 48)]
    {
        spawn(&mut state, key, x, y);
    }
    let entrance = state.building(residence).unwrap().entrance();
    assert_eq!(entrance, IVec2::new(45, 47));
    // The existing road stub at the entrance is the only connection.
    let d = depot(&state);
    link(&mut state, residence, d);
    // Wealthy families (they buy wine, art and silk at the market).
    run(&mut state, crate::time::TICKS_PER_HOUR + 1);
    let ids: Vec<_> = state.population.households.iter().map(|(id, _)| id).collect();
    for id in ids {
        state.population.households.get_mut(id).unwrap().purse = crate::population::Purse::with_savings(10_000_000);
    }
    // On their errand every household got its goods from the market.
    crate::test_support::run_until(&mut state, crate::time::TICKS_PER_DAY, |s| {
        upgrade_blockers(s, defs(), residence).unwrap().is_empty()
    });
    let events = exec(&mut state, Command::UpgradeBuilding { building: residence });
    assert_eq!(rejection(&events), None);
    let b = state.building(residence).unwrap();
    assert_eq!(b.rect(), TileRect::new(IVec2::new(43, 43), IVec2::new(4, 4)), "grows back- and sideways");
    assert_eq!(b.entrance(), entrance, "3x3 -> 4x4 keeps the entrance tile");
    assert!(state.has_road_access(residence));
    let mut supplied = false;
    for _ in 0..6000 {
        run(&mut state, 1);
        if state.building(residence).unwrap().construction_site().is_none_or(|s| s.fully_supplied()) {
            supplied = true;
            break;
        }
    }
    assert!(supplied, "carriers deliver the manor's materials over the old road");
}

#[test]
fn a_gate_on_a_wall_site_refunds_the_site() {
    let mut state = flat_state(32);
    tech(&mut state, "city_walls");
    let stone = res("stone");
    let wall = place(&mut state, "wall", 10, 10, rot(0));
    // Carriers have brought the wall's stone; nobody has built yet.
    state.buildings.get_mut(wall).unwrap().construction_site_mut().unwrap().deliver(stone, 3);
    crate::economy::storage::refresh_stock(&mut state, defs());
    let stock = state.economy().stock().get(stone);
    let gold = state.economy().treasury();
    let events = exec(&mut state, Command::PlaceBuilding { kind: kind("gate"), origin: IVec2::new(10, 10), rotation: rot(0) });
    assert_eq!(rejection(&events), None);
    let expected = vec![ResourceAmount { resource: stone, amount: 3 }];
    assert!(has(&events, |e| matches!(e, SimEvent::ConstructionCancelled { building, refund, .. } if *building == wall && *refund == expected)));
    assert!(state.building(wall).is_none());
    assert_eq!(state.economy().stock().get(stone), stock + 3, "delivered stone goes back to storage");
    assert_eq!(state.economy().treasury(), gold - 25 + 3, "gate paid, wall gold refunded");
}

#[test]
fn an_entrance_across_a_cliff_gives_no_road_access() {
    let mut state = flat_state(32);
    let entrance = IVec2::new(11, 12);
    state.world.tile_mut(entrance).elevation = 7;
    let check = check_placement(&state, defs(), kind("house_wood"), IVec2::new(10, 10), rot(0));
    assert_eq!(check.entrance, entrance);
    assert_eq!(check.entrance_status, EntranceStatus::Blocked);
    assert!(check.warnings.contains(&PlacementWarning::EntranceBlocked));
    let house = spawn(&mut state, "house_wood", 10, 10);
    let path = defs().road_id("path").unwrap();
    state.roads.set(entrance, Some(RoadTile { road: path, bridge_axis: None }));
    state.roads.rebuild_components();
    assert!(!state.building(house).unwrap().entrance_reachable(state.world()));
    assert!(!state.has_road_access(house));
    assert_eq!(state.road_mask(entrance), Some(0), "no road stub into the cliff");
    // One level is a walkable step.
    state.world.tile_mut(entrance).elevation = 3;
    assert!(state.has_road_access(house));
    assert_eq!(state.road_mask(entrance), Some(1));
    let check = check_placement(&state, defs(), kind("house_wood"), IVec2::new(20, 10), rot(0));
    assert_eq!(check.entrance_status, EntranceStatus::Free);
}

#[test]
fn houses_upgrade_automatically_unless_locked() {
    let mut state = flat_state(48);
    let free = spawn(&mut state, "house_wood", 10, 10);
    let locked = spawn(&mut state, "house_wood", 13, 10);
    spawn(&mut state, "market", 16, 10);
    spawn(&mut state, "well", 12, 14);
    let events = exec(&mut state, Command::SetUpgradeLocked { building: locked, locked: true });
    assert!(has(&events, |e| *e == SimEvent::UpgradeLockChanged { building: locked, locked: true }));
    assert!(state.building(locked).unwrap().is_upgrade_locked());
    // Every house is checked once an hour in its own slot (id index mod 200): nothing happens
    // before the slot of the unlocked house, then it starts by itself.
    let slot = u64::from(free.index()) % crate::time::TICKS_PER_HOUR;
    // The construction step of the k-th next tick sees the clock at `now + k`.
    let hour = crate::time::TICKS_PER_HOUR;
    let k = match (slot + hour - state.clock().tick % hour) % hour {
        0 => hour,
        k => k,
    };
    let events = run(&mut state, k - 1);
    assert!(!has(&events, |e| matches!(e, SimEvent::UpgradeStarted { .. })));
    let events = run(&mut state, 1);
    assert!(has(&events, |e| matches!(e, SimEvent::UpgradeStarted { building, to, .. } if *building == free && *to == kind("farmhouse"))));
    assert!(has(&events, |e| matches!(e, SimEvent::Notification(n) if n.text.contains("Ausbau begonnen"))));
    assert_eq!(state.building(free).unwrap().status(), BuildingStatus::Upgrading);
    assert_eq!(state.building(locked).unwrap().status(), BuildingStatus::Active, "locked houses wait");
    assert_eq!(state.economy().committed().get(res("planks")), 10, "only one upgrade was paid");
    // A locked house can still be upgraded by hand; unlocking lets the hourly pass take over.
    let mut events = exec(&mut state, Command::SetUpgradeLocked { building: locked, locked: false });
    assert!(has(&events, |e| matches!(e, SimEvent::UpgradeLockChanged { locked: false, .. })));
    events.extend(run(&mut state, crate::time::TICKS_PER_HOUR));
    assert!(has(&events, |e| matches!(e, SimEvent::UpgradeStarted { building, .. } if *building == locked)));
    // Only houses have an upgrade lock.
    let well = state.buildings().at(IVec2::new(12, 14)).map(|(id, _)| id).unwrap();
    let events = exec(&mut state, Command::SetUpgradeLocked { building: well, locked: true });
    assert_eq!(rejection(&events), Some(&RejectReason::Upgrade(UpgradeBlocker::NotHousing)));
}

#[test]
fn builders_work_around_the_site_and_only_there() {
    let d = defs();
    let mut state = flat_state(48);
    let kind = d.building_id("sawmill").unwrap();
    let events = exec(&mut state, Command::PlaceBuilding { kind, origin: IVec2::new(30, 36), rotation: Rotation::default() });
    let site = events
        .iter()
        .find_map(|e| match e {
            SimEvent::BuildingPlaced { building, .. } => Some(*building),
            _ => None,
        })
        .unwrap();
    supply(&mut state, site);
    let rect = state.building(site).unwrap().rect();
    assert_eq!(crate::population::work_area(&state, d, site), crate::population::WorkArea::Around(rect));
    let present = |s: &GameState| {
        s.population()
            .workers_of(site)
            .iter()
            .filter_map(|c| s.population().citizen(*c))
            .filter(|c| crate::population::is_at_work(s, d, c))
            .map(|c| c.motion().clone())
            .collect::<Vec<_>>()
    };
    crate::test_support::run_until(&mut state, crate::time::TICKS_PER_DAY, |s| !present(s).is_empty());
    for m in present(&state) {
        let tile = m.pos().tile();
        assert!(m.is_visible(), "builders stand outside, visible");
        assert!(rect.expanded(1).contains(tile) && !rect.contains(tile), "on a tile around the site");
    }
    // Work only while somebody is on site: not at night.
    crate::test_support::run_until(&mut state, 2 * crate::time::TICKS_PER_DAY, |s| {
        s.building(site).is_some_and(|b| b.construction_site().is_none()) || (present(s).is_empty() && s.clock().hour() >= 22)
    });
    if let Some(progress) = state.building(site).unwrap().construction_site().map(|s| s.progress()) {
        run(&mut state, 100);
        assert_eq!(state.building(site).unwrap().construction_site().map(|s| s.progress()), Some(progress), "no work at night");
    }
}

#[test]
fn sites_out_of_every_storages_reach_are_warned_about() {
    // The fixture depot (a warehouse, carrier radius 24) stands at (77, 77).
    let state = flat_state(80);
    let near = check_placement(&state, defs(), kind("house_wood"), IVec2::new(60, 60), rot(0));
    assert!(near.ok && !near.warnings.contains(&PlacementWarning::NoStorageInReach), "{:?} {:?}", near.errors, near.warnings);
    let far = check_placement(&state, defs(), kind("house_wood"), IVec2::new(10, 10), rot(0));
    assert!(far.ok, "only a warning: {:?}", far.errors);
    assert!(far.warnings.contains(&PlacementWarning::NoStorageInReach), "{:?}", far.warnings);
    assert_eq!(
        PlacementWarning::NoStorageInReach.to_string(),
        "Kein Lager erreicht diesen Bauplatz: Das Baumaterial käme nie an. Baut zuerst ein Lagerhaus in der Nähe."
    );
}
