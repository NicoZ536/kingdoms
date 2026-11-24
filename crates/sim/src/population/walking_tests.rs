//! Walking citizens: routines, paths, presence, travellers, the request budget and the
//! spatial index (docs/game_design.md §6).

use glam::IVec2;

use super::movement::{self, SUBTILE, SubPos, TripPhase, WalkerId};
use super::*;
use crate::buildings::BuildingId;
use crate::event::SimEvent;
use crate::grid::{Axis, TileRect};
use crate::production::ProductionStatus;
use crate::roads::RoadTile;
use crate::state::GameState;
use crate::test_support::{defs, defs_edited, flat_state, link, run, run_until, set_adults, spawn};
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
use crate::world::Terrain;

/// Stops the fixture depot, so it takes no base carrier from the few citizens of a test.
fn quiet_depot(state: &mut GameState) {
    let depot = crate::test_support::depot(state);
    state.buildings.get_mut(depot).unwrap().paused = true;
}

/// A household of `adults` (1 or 2) and `children` living in a house (placed inside it), with
/// jobs allocated; the fixture's singles are removed and the depot is stopped.
fn settle(state: &mut GameState, adults: u16, children: u16) -> (HouseholdId, Vec<CitizenId>) {
    let d = defs();
    set_adults(state, 0);
    quiet_depot(state);
    let family = demography::make_family(state, d, adults, children);
    housing::sync_homes(state, d, &mut Vec::new());
    movement::place_inside_home(state, d, family);
    state.workforce.mark_dirty();
    run(state, 1);
    let members = state.population.household(family).unwrap().members().to_vec();
    (family, members)
}

fn motion(state: &GameState, id: CitizenId) -> &Motion {
    state.population.citizen(id).unwrap().motion()
}

fn door(state: &GameState, b: BuildingId) -> SubPos {
    SubPos::center(state.building(b).unwrap().door())
}

/// Runs `ticks` ticks and checks after every tick that nobody teleports: every walker moved at
/// most one tile edge, and visible walkers stand on walkable tiles or a footprint (doors).
fn run_checked(state: &mut GameState, ticks: u64) -> Vec<SimEvent> {
    let d = defs();
    let mut events = Vec::new();
    for _ in 0..ticks {
        let before: Vec<(CitizenId, SubPos)> = state.population.citizens().map(|(id, c)| (id, c.motion().pos())).collect();
        events.extend(state.tick(d, Vec::new()));
        for (id, old) in before {
            let Some(c) = state.population.citizen(id) else { continue };
            let m = c.motion();
            let jump = (m.pos().x - old.x).abs() + (m.pos().y - old.y).abs();
            assert!(jump <= SUBTILE / 4 || m.prev_pos() == m.pos() && jump <= SUBTILE, "{id:?} jumped {jump}");
            if m.is_visible() {
                let tile = m.pos().tile();
                assert!(
                    state.navigation().is_walkable(tile) || state.buildings().at(tile).is_some(),
                    "{id:?} on impassable {tile}"
                );
            }
        }
    }
    events
}

#[test]
fn a_worker_walks_to_work_and_home_again() {
    let d = defs();
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 6, 6);
    let farm = spawn(&mut state, "chicken_farm", 30, 6);
    link(&mut state, house, farm);
    let (_, members) = settle(&mut state, 1, 0);
    let worker = members[0];
    assert_eq!(state.population.citizen(worker).unwrap().job().map(|j| j.building), Some(farm));
    // 06:00: inside the house, invisible.
    assert_eq!(motion(&state, worker).inside(), Some(house));
    assert!(!motion(&state, worker).is_visible());
    assert_eq!(motion(&state, worker).pos(), door(&state, house));
    // 08:00: the commute starts; the worker walks out of the door along the path.
    run_checked(&mut state, 2 * TICKS_PER_HOUR - 1);
    assert_eq!(motion(&state, worker).inside(), Some(house), "nobody leaves before 08:00");
    let left = run_until(&mut state, 40, |s| motion(s, worker).is_moving());
    assert!(left < 40);
    let departure = state.clock().tick;
    assert_eq!(current_activity(&state, d, state.population.citizen(worker).unwrap()), Activity::Walking);
    assert!(activity_text(&state, d, worker).unwrap().starts_with("Auf dem Weg zur Arbeit (Hühnerhof"));
    // The renderer sees the walker on the map, moving east.
    run_checked(&mut state, 20);
    let rect = TileRect::new(IVec2::ZERO, IVec2::splat(48));
    let views: Vec<CitizenView> = citizens_in(&state, d, rect).collect();
    assert_eq!(views.len(), 1);
    let v = views[0];
    assert_eq!(v.id, WalkerId::Citizen(worker));
    assert!(v.moving && v.visible && v.carried.is_none());
    assert!(v.pos != v.prev_pos);
    assert!(matches!(v.role, CitizenRole::Man | CitizenRole::Woman));
    // Arrival: inside the farm, present, the farm works.
    run_checked(&mut state, TICKS_PER_HOUR);
    let m = motion(&state, worker);
    assert_eq!(m.inside(), Some(farm));
    assert_eq!(m.pos(), door(&state, farm));
    let commute = m.last_commute_ticks().unwrap();
    assert!(commute > 20 && u64::from(commute) <= state.clock().tick - departure + 20, "{commute}");
    assert!(is_at_work(&state, d, state.population.citizen(worker).unwrap()));
    assert_eq!(present_workers(&state, d, farm), 1);
    assert_eq!(state.building(farm).unwrap().production().unwrap().status(), ProductionStatus::Working);
    let report = crate::report::production_report(&state, d, farm).unwrap();
    assert_eq!((report.staffed, report.present), (1, 1));
    assert!(report.commute.average_minutes.is_some_and(|m| m > 0));
    assert!(report.commute.text.starts_with("Arbeitsweg Ø"), "{}", report.commute.text);
    assert!(citizens_in(&state, d, rect).next().is_none(), "hidden inside");
    // Lunch at the workplace, then work again until 17:00.
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 12 && s.clock().minute() == 30);
    assert_eq!(current_activity(&state, d, state.population.citizen(worker).unwrap()), Activity::LunchBreak);
    assert_eq!(state.building(farm).unwrap().production().unwrap().status(), ProductionStatus::LunchBreak);
    assert_eq!(motion(&state, worker).inside(), Some(farm));
    // After work: home again by night.
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 17 && s.clock().minute() == 30);
    assert!(motion(&state, worker).is_moving() || motion(&state, worker).inside() == Some(house));
    run_checked(&mut state, 4 * TICKS_PER_HOUR);
    assert_eq!(motion(&state, worker).inside(), Some(house));
    assert_eq!(present_workers(&state, d, farm), 0);
    assert_ne!(state.building(farm).unwrap().production().unwrap().status(), ProductionStatus::Working);
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 23);
    assert_eq!(current_activity(&state, d, state.population.citizen(worker).unwrap()), Activity::Sleeping);
    state.validate_entities(d).unwrap();
}

/// Work progress of a production building in milli-ticks (completed cycles + running one).
fn output_progress(state: &GameState, id: BuildingId) -> u64 {
    let p = state.building(id).unwrap().production().unwrap();
    p.cycles_completed() * 2 * TICKS_PER_HOUR * 1000 + p.cycle().map_or(0, |c| c.progress)
}

#[test]
fn long_commutes_cost_output() {
    let d = defs();
    let mut state = flat_state(64);
    let house = spawn(&mut state, "house_wood", 4, 4);
    let near = spawn(&mut state, "chicken_farm", 4, 8);
    let far = spawn(&mut state, "chicken_farm", 50, 50);
    let (_, members) = settle(&mut state, 2, 0);
    assert!(members.iter().all(|m| state.population.citizen(*m).unwrap().job().is_some()));
    // One working day until 14:00: the far farm's worker walks almost 90 tiles off-road.
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 14);
    let (a, b) = (output_progress(&state, near), output_progress(&state, far));
    assert!(a > b + 100 * 3000, "near {a}, far {b}");
    let rn = crate::report::commute_report(&state, d, near);
    let rf = crate::report::commute_report(&state, d, far);
    let (an, af) = (rn.average_minutes, rf.average_minutes);
    assert!(af.zip(an).is_some_and(|(f, n)| f > n + 30), "{rn:?} {rf:?}");
    assert!(rf.lost_percent > rn.lost_percent && rf.lost_percent >= 10, "{rf:?}");
    assert_eq!((rf.present, rf.walking, rf.no_path), (1, 0, 0));
    let _ = house;
}

#[test]
fn unreachable_workplaces_report_no_path_until_a_bridge_is_built() {
    let d = defs();
    let mut state = flat_state(48);
    for y in 0..48 {
        for x in 20..22 {
            state.world.tile_mut(IVec2::new(x, y)).terrain = Terrain::ShallowWater;
        }
    }
    let house = spawn(&mut state, "house_wood", 6, 6);
    let farm = spawn(&mut state, "chicken_farm", 30, 6);
    crate::navigation::rebuild(&mut state, d);
    let (_, members) = settle(&mut state, 1, 0);
    let worker = members[0];
    let events = run_checked(&mut state, 3 * TICKS_PER_HOUR);
    let m = motion(&state, worker);
    assert!(matches!(m.phase(), TripPhase::NoPath { .. }));
    assert_eq!(m.inside(), Some(house), "waits at home");
    let notes: Vec<_> = events.iter().filter_map(SimEvent::notification).filter(|n| n.text.starts_with("Kein Weg zur Arbeit")).collect();
    assert_eq!(notes.len(), 1, "once, then rate-limited");
    assert!(notes[0].text.contains("Hühnerhof"));
    assert_eq!(current_activity(&state, d, state.population.citizen(worker).unwrap()), Activity::NoPath);
    assert!(activity_text(&state, d, worker).unwrap().starts_with("Findet keinen Weg (Ziel: Hühnerhof"));
    assert_eq!(crate::report::commute_report(&state, d, farm).no_path, 1);
    assert!(crate::report::path_problem(&state, farm).is_some_and(|t| t.starts_with("Kein Weg: 1 Bewohner")));
    assert_eq!(state.building(farm).unwrap().production().unwrap().status(), ProductionStatus::WorkersAway);
    // A bridge (tracked like any road) connects the banks; the next attempt finds the way.
    let bridge = d.road_id("bridge").unwrap();
    for x in 20..22 {
        state.roads.set(IVec2::new(x, 20), Some(RoadTile { road: bridge, bridge_axis: Some(Axis::X) }));
    }
    let retry = u64::from(d.rules().walking.no_path_retry_ticks);
    run_until(&mut state, retry + 3 * TICKS_PER_HOUR, |s| motion(s, worker).inside() == Some(farm));
    assert_eq!(crate::report::path_problem(&state, farm), None);
    state.validate_entities(d).unwrap();
}

/// Review finding: removing a bridge with a walker on it left the walker on water it could
/// not leave, and every later save failed validation.
#[test]
fn a_bridge_with_a_walker_on_it_is_not_removed() {
    let d = defs();
    let mut state = flat_state(128);
    for y in 0..128 {
        for x in 20..24 {
            state.world.tile_mut(IVec2::new(x, y)).terrain = Terrain::ShallowWater;
        }
    }
    spawn(&mut state, "house_wood", 6, 6);
    spawn(&mut state, "chicken_farm", 30, 6);
    let bridge = d.road_id("bridge").unwrap();
    for x in 20..24 {
        state.roads.set(IVec2::new(x, 20), Some(RoadTile { road: bridge, bridge_axis: Some(Axis::X) }));
    }
    crate::navigation::rebuild(&mut state, d);
    let (_, members) = settle(&mut state, 1, 0);
    let worker = members[0];
    run_until(&mut state, 2 * TICKS_PER_DAY, |s| motion(s, worker).is_moving() && motion(s, worker).pos().tile() == IVec2::new(22, 20));
    // Any tile of the bridge names the whole bridge: rejected while the worker is on it.
    let events = state.tick(d, vec![crate::command::Command::RemoveRoad { tiles: vec![IVec2::new(20, 20)] }]);
    let reason = crate::test_support::rejection(&events).cloned();
    assert!(matches!(reason, Some(crate::command::RejectReason::BridgeInUse { .. })), "{reason:?}");
    assert!(reason.unwrap().to_string().starts_with("Auf der Brücke bei"));
    assert!(state.roads().get(IVec2::new(22, 20)).is_some_and(|t| t.is_bridge()));
    // Once nobody is on it, it goes.
    let on_bridge = |s: &GameState| crate::population::movement::walker_on(s, &[20, 21, 22, 23].map(|x| IVec2::new(x, 20))).is_some();
    run_until(&mut state, TICKS_PER_HOUR, |s| !on_bridge(s));
    let events = state.tick(d, vec![crate::command::Command::RemoveRoad { tiles: vec![IVec2::new(21, 20)] }]);
    assert_eq!(crate::test_support::rejection(&events), None);
    assert!((20..24).all(|x| state.roads().get(IVec2::new(x, 20)).is_none()));
    run_checked(&mut state, 3 * TICKS_PER_HOUR);
    let m = motion(&state, worker);
    assert!(!state.world().tile(m.pos().tile()).is_water(), "on land: {:?}", m.pos().tile());
    state.validate_entities(d).expect("consistent");
    let header = crate::save::SaveHeader::for_state(&state, d, "bridge", 0);
    let bytes = crate::save::encode(&header, &state).unwrap();
    assert!(crate::save::decode(&bytes, d).is_ok(), "the save loads");
}

/// Review finding: `MoveBuilding` put everybody inside the building at its new door in the same
/// tick (up to the width of the territory). They step out at the old door and walk instead.
#[test]
fn people_inside_a_moved_building_walk_to_it() {
    let d = defs();
    let mut state = flat_state(64);
    let house = spawn(&mut state, "house_wood", 6, 6);
    let (_, members) = settle(&mut state, 2, 1);
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 23);
    let old_door = door(&state, house);
    assert!(members.iter().all(|m| motion(&state, *m).inside() == Some(house)), "at home at night");
    let events = state.tick(
        d,
        vec![crate::command::Command::MoveBuilding { building: house, origin: IVec2::new(40, 40), rotation: Default::default() }],
    );
    assert_eq!(crate::test_support::rejection(&events), None);
    let new_door = door(&state, house);
    assert_ne!(new_door, old_door);
    for m in &members {
        let m = motion(&state, *m);
        assert!(m.inside().is_none(), "stepped out");
        assert_eq!(m.prev_pos(), old_door, "left from the old door");
        let walked = (m.pos().x - old_door.x).abs() + (m.pos().y - old_door.y).abs();
        assert!(walked <= SUBTILE / 4, "walking, not teleported: {walked}");
    }
    // From there on everybody walks (checked every tick) while the house is being moved.
    run_checked(&mut state, 6 * TICKS_PER_HOUR);
    state.validate_entities(d).expect("consistent");
}

#[test]
fn a_blocked_entrance_cannot_be_reached() {
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 6, 6);
    let farm = spawn(&mut state, "chicken_farm", 30, 6);
    // A well right on the farm's entrance tile.
    let entrance = state.building(farm).unwrap().entrance();
    spawn(&mut state, "well", entrance.x, entrance.y);
    let (_, members) = settle(&mut state, 1, 0);
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 10);
    assert!(matches!(motion(&state, members[0]).phase(), TripPhase::NoPath { .. }));
    assert_eq!(motion(&state, members[0]).inside(), Some(house));
}

#[test]
fn walkers_find_a_new_way_around_new_buildings() {
    let d = defs();
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 4, 20);
    let farm = spawn(&mut state, "chicken_farm", 40, 20);
    let (_, members) = settle(&mut state, 1, 0);
    let worker = members[0];
    run_until(&mut state, TICKS_PER_DAY, |s| motion(s, worker).is_moving());
    run(&mut state, 10);
    // A sawmill right ahead on the straight line to the farm.
    let at = motion(&state, worker).pos().tile();
    let blocker = spawn(&mut state, "sawmill", at.x + 6, at.y - 1);
    let rect = state.building(blocker).unwrap().rect();
    for _ in 0..TICKS_PER_DAY {
        run_checked(&mut state, 1);
        let m = motion(&state, worker);
        assert!(!m.is_visible() || !rect.contains(m.pos().tile()), "walked through the sawmill");
        if m.inside() == Some(farm) {
            break;
        }
    }
    assert_eq!(motion(&state, worker).inside(), Some(farm));
    let _ = house;
    state.validate_entities(d).unwrap();
}

#[test]
fn walkers_caught_by_a_new_building_walk_out_of_it() {
    let d = defs();
    let mut state = flat_state(48);
    spawn(&mut state, "house_wood", 4, 20);
    let farm = spawn(&mut state, "chicken_farm", 40, 20);
    let (_, members) = settle(&mut state, 1, 0);
    let worker = members[0];
    run_until(&mut state, TICKS_PER_DAY, |s| motion(s, worker).is_moving());
    run_until(&mut state, 200, |s| motion(s, worker).pos().tile().x >= 14);
    // A tavern (3×3) right on top of the walker: it stands in the middle of the footprint.
    let at = motion(&state, worker).pos().tile();
    let tavern = spawn(&mut state, "tavern", at.x - 1, at.y - 1);
    let rect = state.building(tavern).unwrap().rect();
    assert!(rect.contains(at) && rect.tiles().all(|p| p == at || (p - at).abs().max_element() == 1));
    for _ in 0..TICKS_PER_DAY {
        run_checked(&mut state, 1);
        if motion(&state, worker).inside() == Some(farm) {
            break;
        }
    }
    assert_eq!(motion(&state, worker).inside(), Some(farm), "walked out of the market and on to work");
    state.validate_entities(d).unwrap();
}

#[test]
fn the_path_budget_spreads_requests_over_ticks() {
    let tight = defs_edited("rules.ron", |t| {
        t.replace("path_budget_per_tick: 10000", "path_budget_per_tick: 100").replace("path_budget_per_queued: 100", "path_budget_per_queued: 0")
    });
    let mut state = flat_state(64);
    set_adults(&mut state, 0);
    let farm = spawn(&mut state, "chicken_farm", 58, 4);
    let _ = farm;
    // 24 singles standing in a row, each asked to walk to a far building at once.
    let target = state.buildings().iter().next().map(|(id, _)| id).unwrap();
    let mut walkers = Vec::new();
    for i in 0..24 {
        let h = demography::make_family(&mut state, &tight, 1, 0);
        let id = state.population.household(h).unwrap().members()[0];
        movement::place(&mut state, WalkerId::Citizen(id), Motion::standing(IVec2::new(2 + i, 40)));
        walkers.push(id);
    }
    for id in &walkers {
        movement::set_destination(&mut state, WalkerId::Citizen(*id), Some(Destination::Building { building: target, purpose: Purpose::Work }));
    }
    assert_eq!(state.population.walking().queued_requests(), 24);
    let mut ticks = 0;
    let mut answered = 0;
    while state.population.walking().queued_requests() > 0 {
        let (requests, expanded) = (state.navigation().stats().requests, state.navigation().stats().expansions);
        movement::run_movement(&mut state, &tight, &mut Vec::new());
        let stats = state.navigation().stats();
        answered += stats.requests - requests;
        // Never more search work in a tick than the budget: a search that does not fit goes
        // on in the next tick.
        assert!(stats.expansions - expanded <= 100, "tick {ticks}: {} expansions", stats.expansions - expanded);
        ticks += 1;
        assert!(ticks < 100);
    }
    assert_eq!(answered, 24);
    // About 80 expansions per search: the 24 requests need about 20 ticks of budget.
    let total = state.navigation().stats().expansions;
    assert!(ticks as u64 >= total / 100 && ticks as u64 <= total / 100 + 2, "{ticks} ticks for {total} expansions");
    assert!(walkers.iter().all(|id| motion(&state, *id).is_moving()));
}

#[test]
fn a_long_path_queue_gets_a_larger_budget_up_to_the_cap() {
    let walking = &defs().rules().walking;
    let base = walking.path_budget_per_tick;
    assert_eq!(walking.path_budget(0), base);
    assert_eq!(walking.path_budget(10), base + 10 * walking.path_budget_per_queued);
    assert_eq!(walking.path_budget(1_000_000), walking.path_budget_max_per_tick);
    assert_eq!(walking.path_budget(usize::MAX), walking.path_budget_max_per_tick);
}

#[test]
fn immigrants_walk_in_from_the_map_edge() {
    let d = defs();
    let mut state = flat_state(48);
    set_adults(&mut state, 0);
    let hall = spawn(&mut state, "town_hall", 20, 20);
    let house = spawn(&mut state, "house_wood", 26, 20);
    crate::navigation::sync(&mut state, d);
    let entry = movement::entry_point(&mut state, d);
    let e = state.building(hall).unwrap().entrance();
    assert!(entry.x == 0 || entry.y == 0 || entry.x == 47 || entry.y == 47, "on the edge: {entry}");
    // The nearest edge tile (Manhattan distance, ties by row then column).
    let best = e.y.min(47 - e.y).min(e.x.min(47 - e.x));
    assert_eq!((entry - e).abs().element_sum(), best);
    // A road leaving the map is preferred.
    let path = d.road_id("path").unwrap();
    state.roads.set(IVec2::new(47, 30), Some(RoadTile { road: path, bridge_axis: None }));
    crate::navigation::sync(&mut state, d);
    assert_eq!(movement::entry_point(&mut state, d), IVec2::new(47, 30));
    // A family arrives there and walks to its new home.
    let family = demography::make_family(&mut state, d, 2, 1);
    housing::sync_homes(&mut state, d, &mut Vec::new());
    assert_eq!(state.population.household(family).unwrap().home(), Some(house));
    movement::place_standing(&mut state, d, family, IVec2::new(47, 30));
    let members = state.population.household(family).unwrap().members().to_vec();
    assert!(members.iter().all(|m| motion(&state, *m).is_visible()));
    run_until(&mut state, TICKS_PER_DAY, |s| members.iter().all(|m| motion(s, *m).inside() == Some(house)));
    state.validate_entities(d).unwrap();
}

#[test]
fn emigrants_walk_off_the_map_and_vanish() {
    let d = defs();
    let mut state = flat_state(48);
    spawn(&mut state, "town_hall", 20, 20);
    spawn(&mut state, "house_wood", 26, 20);
    let (family, members) = settle(&mut state, 2, 1);
    demography::emigrate(&mut state, d, family, &mut Vec::new());
    assert!(members.iter().all(|m| state.population.citizen(*m).is_none()), "no longer citizens");
    let travelers: Vec<_> = state.population.walking().travelers().map(|(id, _)| id).collect();
    assert_eq!(travelers.len(), 3);
    run(&mut state, 5);
    let rect = TileRect::new(IVec2::ZERO, IVec2::splat(48));
    let seen = citizens_in(&state, d, rect).filter(|v| matches!(v.id, WalkerId::Traveler(_))).count();
    assert_eq!(seen, 3, "they walk on the map");
    assert!(citizens_in(&state, d, rect).any(|v| v.role == CitizenRole::Child));
    state.validate_entities(d).unwrap();
    run_until(&mut state, TICKS_PER_DAY, |s| s.population.walking().travelers().count() == 0);
    assert_eq!(citizens_in(&state, d, rect).count(), 0);
    assert_eq!(state.population.walking.index.len(), 0);
}

#[test]
fn children_go_to_school_and_adults_run_errands_and_go_out() {
    let d = defs();
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 10, 10);
    let school = spawn(&mut state, "school", 16, 10);
    let market = spawn(&mut state, "market", 10, 16);
    let tavern = spawn(&mut state, "tavern", 22, 10);
    let church = spawn(&mut state, "church", 22, 16);
    // Educated workers for the services (singles), then the family.
    set_adults(&mut state, 0);
    quiet_depot(&mut state);
    for _ in 0..6 {
        let h = demography::make_family(&mut state, d, 1, 0);
        let id = state.population.household(h).unwrap().members()[0];
        state.population.citizens.get_mut(id).unwrap().educated = true;
    }
    let family = demography::make_family(&mut state, d, 2, 1);
    housing::sync_homes(&mut state, d, &mut Vec::new());
    movement::place_inside_home(&mut state, d, family);
    let members = state.population.household(family).unwrap().members().to_vec();
    let child = *members.iter().find(|m| state.population.citizen(**m).unwrap().stage() == LifeStage::Child).unwrap();
    state.population.citizens.get_mut(child).unwrap().birth_day = -8 * crate::time::DAYS_PER_SEASON as i64;
    state.workforce.mark_dirty();
    run(&mut state, 1);
    assert_eq!(state.population.household(family).unwrap().home(), Some(house));
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 10);
    assert_eq!(motion(&state, child).inside(), Some(school));
    assert_eq!(current_activity(&state, d, state.population.citizen(child).unwrap()), Activity::School);
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 15);
    assert_eq!(motion(&state, child).inside(), Some(house), "school ends at 13:00");
    // Errands: the first adult of the household walks to the market at 18:00.
    let runner = members[0];
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 18 && s.clock().minute() == 45);
    assert_eq!(motion(&state, runner).inside(), Some(market));
    assert_eq!(current_activity(&state, d, state.population.citizen(runner).unwrap()), Activity::Errand);
    // Evenings: every other day each adult goes to the tavern or the church.
    let mut visited = Vec::new();
    for _ in 0..2 {
        // The stopped depot leaves the market empty; keep the family healthy (the sick stay home).
        for _ in 0..TICKS_PER_DAY {
            if state.clock().hour() == 20 {
                break;
            }
            for m in &members {
                state.population.citizens.get_mut(*m).unwrap().health = crate::population::MAX_VALUE;
            }
            run(&mut state, 1);
        }
        for m in members.iter().filter(|m| **m != child) {
            visited.extend(motion(&state, *m).inside().filter(|b| *b == tavern || *b == church));
        }
        run(&mut state, TICKS_PER_HOUR);
    }
    assert!(visited.len() >= 2, "{visited:?}");
    run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 23);
    assert!(members.iter().all(|m| motion(&state, *m).inside() == Some(house)), "home for the night");
    state.validate_entities(d).unwrap();
}

#[test]
fn the_spatial_index_matches_a_full_scan() {
    let d = defs();
    let mut state = flat_state(96);
    set_adults(&mut state, 0);
    let mut houses = Vec::new();
    for i in 0..6 {
        houses.push(spawn(&mut state, "house_wood", 4 + 14 * i, 6));
        spawn(&mut state, "chicken_farm", 84 - 14 * i, 80);
    }
    for _ in 0..12 {
        demography::make_family(&mut state, d, 1, 0);
    }
    housing::sync_homes(&mut state, d, &mut Vec::new());
    state.workforce.mark_dirty();
    let rects = [TileRect::new(IVec2::ZERO, IVec2::splat(96)), TileRect::new(IVec2::new(20, 20), IVec2::new(40, 33))];
    for _ in 0..(3 * TICKS_PER_HOUR / 10) {
        run(&mut state, 10);
        for rect in rects {
            let mut indexed: Vec<WalkerId> = citizens_in(&state, d, rect).map(|v| v.id).collect();
            indexed.sort();
            let mut scanned: Vec<WalkerId> = state
                .population
                .citizens()
                .filter(|(_, c)| c.motion().is_visible() && rect.contains(c.motion().pos().tile()))
                .map(|(id, _)| WalkerId::Citizen(id))
                .collect();
            scanned.sort();
            assert_eq!(indexed, scanned);
        }
    }
    let incremental = state.population.walking.index.clone();
    movement::rebuild_derived(&mut state);
    assert!(incremental == state.population.walking.index, "incremental index equals a rebuild");
    assert!(state.population.walking.index.len() > 0, "people were on the way");
}

#[test]
fn validate_rejects_broken_positions() {
    let d = defs();
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 6, 6);
    spawn(&mut state, "chicken_farm", 30, 6);
    let (_, members) = settle(&mut state, 1, 0);
    let worker = members[0];
    run_until(&mut state, TICKS_PER_DAY, |s| motion(s, worker).is_moving());
    run(&mut state, 5);
    state.validate_entities(d).unwrap();
    let broken = |f: &dyn Fn(&mut Motion)| {
        let mut s = state.clone();
        f(&mut s.population.citizens.get_mut(worker).unwrap().motion);
        s.validate_entities(d).is_err()
    };
    assert!(broken(&|m| m.pos = SubPos { x: -5, y: 10 }), "outside the map");
    assert!(broken(&|m| m.prev = SubPos { x: m.pos.x + 3 * SUBTILE, y: m.pos.y }), "teleported");
    assert!(broken(&|m| m.progress = 7), "off its path");
    assert!(broken(&|m| m.phase = TripPhase::Queued), "queued without a request");
    assert!(broken(&|m| m.inside = Some(BuildingId::from_raw_parts(999, 0))), "inside a missing building");
    let mut wet = state.clone();
    let tile = motion(&wet, worker).step_origin();
    wet.world.tile_mut(tile).terrain = Terrain::DeepWater;
    if wet.population.citizen(worker).unwrap().motion().pos().tile() == tile {
        assert!(wet.validate_entities(d).is_err(), "standing in deep water");
    }
    let _ = house;
}

#[test]
fn pathfinding_is_deterministic_across_save_and_load() {
    let d = defs();
    let mut state = flat_state(48);
    spawn(&mut state, "house_wood", 6, 6);
    spawn(&mut state, "chicken_farm", 30, 30);
    settle(&mut state, 2, 0);
    run_until(&mut state, TICKS_PER_DAY, |s| s.population.citizens().any(|(_, c)| c.motion().is_moving()));
    let bytes = |s: &GameState| bincode::serde::encode_to_vec(s, bincode::config::standard()).unwrap();
    let mut loaded: GameState = bincode::serde::decode_from_slice(&bytes(&state), bincode::config::standard()).unwrap().0;
    // No rederive: the navigation grid, spatial index and active walkers are rebuilt lazily by
    // the first tick (the fixture's hand-made territory would not survive a rederive).
    loaded.validate_entities(d).unwrap();
    for _ in 0..TICKS_PER_HOUR {
        let a = state.tick(d, Vec::new());
        let b = loaded.tick(d, Vec::new());
        assert_eq!(a, b);
    }
    fn ser<T: serde::Serialize>(v: &T) -> Vec<u8> {
        bincode::serde::encode_to_vec(v, bincode::config::standard()).unwrap()
    }
    for (name, a, b) in [
        ("world", ser(&state.world), ser(&loaded.world)),
        ("clock", ser(&state.clock), ser(&loaded.clock)),
        ("rng", ser(&state.rng), ser(&loaded.rng)),
        ("weather", ser(&state.weather), ser(&loaded.weather)),
        ("economy", ser(&state.economy), ser(&loaded.economy)),
        ("buildings", ser(&state.buildings), ser(&loaded.buildings)),
        ("roads", ser(&state.roads), ser(&loaded.roads)),
        ("territory", ser(&state.territory), ser(&loaded.territory)),
        ("agriculture", ser(&state.agriculture), ser(&loaded.agriculture)),
        ("population", ser(&state.population), ser(&loaded.population)),
        ("workforce", ser(&state.workforce), ser(&loaded.workforce)),
        ("logistics", ser(&state.logistics), ser(&loaded.logistics)),
        ("statistics", ser(&state.statistics), ser(&loaded.statistics)),
    ] {
        assert!(a == b, "{name} differs");
    }
    assert_eq!(bytes(&state), bytes(&loaded));
    let rect = TileRect::new(IVec2::ZERO, IVec2::splat(48));
    let va: Vec<CitizenView> = citizens_in(&state, d, rect).collect();
    let vb: Vec<CitizenView> = citizens_in(&loaded, d, rect).collect();
    assert_eq!(va, vb);
}

/// About 2000 citizens on a 256² map with a road grid: homes in the west, workplaces in the
/// east (long commutes on many different paths), markets, taverns and churches for the
/// evenings. Run with
/// `cargo test --release -p kingdoms_sim walking_performance -- --ignored --nocapture`.
#[test]
#[ignore = "performance measurement; run in release mode"]
fn walking_performance() {
    use std::time::Instant;
    let d = defs();
    let mut state = flat_state(256);
    set_adults(&mut state, 0);
    let path = d.road_id("path").unwrap();
    for i in 0..256 {
        for line in (0..256).step_by(16) {
            for p in [IVec2::new(i, line), IVec2::new(line, i)] {
                if state.buildings().at(p).is_none() {
                    state.roads.set(p, Some(RoadTile { road: path, bridge_axis: None }));
                }
            }
        }
    }
    state.roads.rebuild_components();
    let mut houses = 0;
    let workplaces = ["sawmill", "brickworks", "weaver", "tailor", "carpenter", "smithy", "pottery", "tannery"];
    for by in 0..15 {
        for bx in 0..15 {
            let (x, y) = (16 * bx + 2, 16 * by + 2);
            if bx < 8 {
                for (dx, dy) in [(0, 0), (6, 0), (0, 6), (6, 6)] {
                    spawn(&mut state, "house_wood", x + dx, y + dy);
                    houses += 1;
                }
                let service = ["market", "tavern", "church", "well"][((bx + by) % 4) as usize];
                spawn(&mut state, service, x + 10, y + 2);
            } else {
                for (k, (dx, dy)) in [(0, 0), (6, 0), (0, 6), (6, 6)].into_iter().enumerate() {
                    spawn(&mut state, workplaces[((bx + by) as usize + k) % workplaces.len()], x + dx, y + dy);
                }
                if (bx + by) % 3 == 0 {
                    spawn(&mut state, "warehouse", x + 10, y + 2);
                }
            }
        }
    }
    for (_, b) in state.buildings.iter_mut() {
        for r in d.resources() {
            b.inventory.add(r.id, 200);
        }
    }
    crate::economy::storage::refresh_stock(&mut state, d);
    let mut families = Vec::new();
    while state.population.len() < 2000 {
        let kids = (state.population.len() % 3) as u16;
        families.push(demography::make_family(&mut state, d, 2, kids));
    }
    housing::sync_homes(&mut state, d, &mut Vec::new());
    for f in families {
        movement::place_inside_home(&mut state, d, f);
    }
    state.workforce.mark_dirty();
    run(&mut state, TICKS_PER_HOUR);
    let ticks = 2 * TICKS_PER_DAY;
    let (mut total_ns, mut total_max, mut move_ns, mut move_max, mut max_at) = (0u128, 0u128, 0u128, 0u128, 0);
    let (mut peak_walking, mut peak_queue, mut slow) = (0usize, 0usize, 0u32);
    let stats0 = state.navigation().stats();
    // How long requests wait in the path queue (ticks from queued to answered).
    let mut queued_since: std::collections::BTreeMap<CitizenId, u64> = std::collections::BTreeMap::new();
    let (mut max_wait, mut sum_wait, mut waits) = (0u64, 0u64, 0u64);
    for _ in 0..ticks {
        let tick_of_day = state.clock.tick_of_day() + 1;
        let start = Instant::now();
        let steps = state.tick_timed(d);
        let spent = start.elapsed().as_nanos();
        let moving = steps.iter().find(|(name, _)| *name == "movement").map_or(0, |(_, ns)| *ns);
        total_ns += spent;
        move_ns += moving;
        if moving > move_max {
            move_max = moving;
            max_at = tick_of_day;
        }
        total_max = total_max.max(spent);
        slow += u32::from(spent > 4_000_000);
        peak_walking = peak_walking.max(state.population.citizens().filter(|(_, c)| c.motion().is_moving()).count());
        peak_queue = peak_queue.max(state.population.walking().queued_requests());
        let now = state.clock.tick;
        for (id, c) in state.population.citizens() {
            let queued = c.motion().phase() == TripPhase::Queued;
            match (queued, queued_since.get(&id).copied()) {
                (true, None) => {
                    queued_since.insert(id, now);
                }
                (false, Some(since)) => {
                    queued_since.remove(&id);
                    max_wait = max_wait.max(now - since);
                    sum_wait += now - since;
                    waits += 1;
                }
                _ => {}
            }
        }
    }
    let stats = state.navigation().stats();
    let requests = stats.requests - stats0.requests;
    let hits = stats.cache_hits - stats0.cache_hits;
    eprintln!(
        "path queue: {waits} waits, mean {:.1} ticks, max {max_wait} ticks",
        sum_wait as f64 / waits.max(1) as f64
    );
    eprintln!(
        "{houses} houses, {} citizens, {} employed; over {ticks} ticks: whole tick avg {:.1} µs, max {:.1} µs \
         ({slow} ticks over 4 ms); movement step avg {:.1} µs, max {:.1} µs (tick {max_at} of the day); \
         peak {peak_walking} walking, peak queue {peak_queue}; {requests} path requests, cache hits {:.1} %, \
         {} searches with {} expansions (largest {}), {} without path",
        state.population.len(),
        state.workforce.employed(),
        total_ns as f64 / ticks as f64 / 1000.0,
        total_max as f64 / 1000.0,
        move_ns as f64 / ticks as f64 / 1000.0,
        move_max as f64 / 1000.0,
        hits as f64 * 100.0 / requests.max(1) as f64,
        stats.searches - stats0.searches,
        stats.expansions - stats0.expansions,
        stats.max_expansions,
        stats.no_path - stats0.no_path,
    );
    state.validate_entities(d).unwrap();
    assert!(peak_walking >= 1000, "most workers commute at the same time: {peak_walking}");
    // Review finding: with a fixed budget, rush-hour requests waited up to about two game
    // hours (mean 34 min); the budget grows with the queue.
    assert!(max_wait <= TICKS_PER_HOUR, "a path request waited {max_wait} ticks");
}

/// Like [`walking_performance`], but homes and workplace blocks alternate and every workplace
/// block has a warehouse full of the workshops' inputs: about 2000 citizens of whom a few hundred are carriers walking goods
/// between the workshops and the warehouses all day. Run with
/// `cargo test --release -p kingdoms_sim carrier_performance -- --ignored --nocapture`.
#[test]
#[ignore = "performance measurement; run in release mode"]
fn carrier_performance() {
    use std::time::Instant;
    let d = defs();
    let mut state = flat_state(256);
    set_adults(&mut state, 0);
    let path = d.road_id("path").unwrap();
    for i in 0..256 {
        for line in (0..256).step_by(16) {
            for p in [IVec2::new(i, line), IVec2::new(line, i)] {
                if state.buildings().at(p).is_none() {
                    state.roads.set(p, Some(RoadTile { road: path, bridge_axis: None }));
                }
            }
        }
    }
    state.roads.rebuild_components();
    let workplaces = ["sawmill", "brickworks", "weaver", "tailor", "carpenter", "smithy", "pottery", "tannery"];
    for by in 0..15 {
        for bx in 0..15 {
            let (x, y) = (16 * bx + 2, 16 * by + 2);
            // Homes and workplaces alternate like a chessboard: commutes of a block or two.
            if (bx + by) % 2 == 0 {
                for (dx, dy) in [(0, 0), (6, 0), (0, 6), (6, 6)] {
                    spawn(&mut state, "house_wood", x + dx, y + dy);
                }
                let service = ["market", "tavern", "church", "well"][((bx + by) % 4) as usize];
                spawn(&mut state, service, x + 10, y + 2);
            } else {
                let mut block = Vec::new();
                for (k, (dx, dy)) in [(0, 0), (6, 0), (0, 6), (6, 6)].into_iter().enumerate() {
                    block.push(spawn(&mut state, workplaces[((bx + by) as usize + k) % workplaces.len()], x + dx, y + dy));
                }
                block.push(spawn(&mut state, "warehouse", x + 10, y + 2));
                // Access paths from every entrance to the road grid (the line north of the block
                // or the one west of it).
                for b in block {
                    let e = state.building(b).unwrap().entrance();
                    let road = if e.y - 16 * by <= 7 { IVec2::new(e.x, 16 * by) } else { IVec2::new(e.x, 16 * (by + 1)) };
                    crate::test_support::lay_path(&mut state, e, road);
                }
            }
        }
    }
    for (_, b) in state.buildings.iter_mut() {
        for r in d.resources() {
            b.inventory.add(r.id, 400);
        }
    }
    crate::economy::storage::refresh_stock(&mut state, d);
    let mut families = Vec::new();
    while state.population.len() < 2000 {
        let kids = (state.population.len() % 3) as u16;
        families.push(demography::make_family(&mut state, d, 2, kids));
    }
    housing::sync_homes(&mut state, d, &mut Vec::new());
    for f in families {
        movement::place_inside_home(&mut state, d, f);
    }
    state.workforce.mark_dirty();
    run(&mut state, TICKS_PER_HOUR);
    let ticks = 2 * TICKS_PER_DAY;
    let (mut total_ns, mut total_max, mut logistics_ns, mut logistics_max) = (0u128, 0u128, 0u128, 0u128);
    let mut logistics_max_at = 0;
    let (mut slow, mut peak_busy, mut peak_units, mut peak_waiting) = (0u32, 0u32, 0u32, 0u32);
    let trips0 = state.logistics().trips_completed();
    for _ in 0..ticks {
        let start = Instant::now();
        let steps = state.tick_timed(d);
        let spent = start.elapsed().as_nanos();
        let logistics = steps.iter().find(|(name, _)| *name == "logistics").map_or(0, |(_, ns)| *ns);
        total_ns += spent;
        total_max = total_max.max(spent);
        logistics_ns += logistics;
        if logistics > logistics_max {
            logistics_max = logistics;
            logistics_max_at = state.clock.tick;
        }
        slow += u32::from(spent > 4_000_000);
        let report = crate::report::logistics_report(&state, d);
        peak_busy = peak_busy.max(report.busy_carriers);
        peak_units = peak_units.max(report.units_in_transit);
        peak_waiting = peak_waiting.max(report.waiting);
    }
    let report = crate::report::logistics_report(&state, d);
    eprintln!(
        "{} citizens, {} employed, {} carriers; over {ticks} ticks: whole tick avg {:.1} µs, max {:.1} µs ({slow} ticks \
         over 4 ms); logistics step avg {:.1} µs, max {:.1} µs (tick {logistics_max_at}); peak {peak_busy} carriers busy, peak {peak_units} units in \
         transit, peak {peak_waiting} jobs waiting; {} trips, average trip {:?} min",
        state.population.len(),
        state.workforce.employed(),
        report.carriers,
        total_ns as f64 / ticks as f64 / 1000.0,
        total_max as f64 / 1000.0,
        logistics_ns as f64 / ticks as f64 / 1000.0,
        logistics_max as f64 / 1000.0,
        state.logistics().trips_completed() - trips0,
        report.average_trip_minutes,
    );
    state.validate_entities(d).unwrap();
    assert!(peak_busy >= 150, "carriers worked: {peak_busy}");
}
