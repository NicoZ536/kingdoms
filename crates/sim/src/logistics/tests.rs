//! Logistics tests: travel delays, reach, carriers, returns and priorities.

use glam::IVec2;

use super::*;
use crate::command::Command;
use crate::event::SimEvent;
use crate::test_support::{defs, depot, exec, flat_state, link, run, run_until, spawn};
use crate::time::TICKS_PER_HOUR;

fn res(key: &str) -> ResourceId {
    defs().resource_id(key).unwrap()
}

#[test]
fn a_carrier_walks_to_the_source_and_carries_the_goods_back() {
    let d = defs();
    let mut state = flat_state(48);
    let meat = res("meat");
    let farm = spawn(&mut state, "chicken_farm", 30, 38);
    let store = depot(&state);
    link(&mut state, farm, store);
    // Wait for the first collection to be taken by a carrier.
    let mut job = None;
    for _ in 0..(2 * crate::time::TICKS_PER_DAY) {
        run(&mut state, 1);
        job = state.logistics().jobs().iter().find(|j| j.kind == TransportKind::Collect && j.occupies_carrier()).copied();
        if job.is_some() {
            break;
        }
    }
    let job = job.expect("a carrier took a collection");
    let carrier = job.carrier().unwrap();
    assert_eq!(job.carrier_base, store);
    assert!(state.population().citizen(carrier).unwrap().job().is_some_and(|j| j.building == store));
    // On the way to the farm the goods stay in its buffer.
    let held = state.building(farm).unwrap().production().unwrap().output().get(meat);
    assert!(held >= job.amount);
    let mut picked = None;
    let mut seen_walking = false;
    for _ in 0..TICKS_PER_HOUR * 3 {
        let now = state.clock().tick + 1;
        let events = run(&mut state, 1);
        let c = state.population().citizen(carrier).unwrap();
        seen_walking |= c.motion().is_moving();
        if let Some(e) = events.iter().find(|e| matches!(e, SimEvent::TransportStarted { job: j, .. } if *j == job.id)) {
            let SimEvent::TransportStarted { amount, carrier: by, .. } = e else { unreachable!() };
            assert_eq!(*by, carrier);
            picked = Some((now, *amount));
            break;
        }
    }
    let (picked_at, amount) = picked.expect("picked up");
    assert!(seen_walking, "the carrier walked to the farm");
    let c = state.population().citizen(carrier).unwrap();
    let door = crate::population::SubPos::center(state.building(farm).unwrap().door());
    let near = (c.motion().pos().x - door.x).abs() + (c.motion().pos().y - door.y).abs() <= crate::population::SUBTILE;
    assert!(c.motion().inside() == Some(farm) || near, "loaded at the farm: {:?} vs door {door:?}", c.motion().pos());
    assert_eq!(c.carried(), Some(crate::defs::ResourceAmount { resource: meat, amount }));
    let view = crate::population::citizen_view(&state, d, carrier).unwrap();
    assert_eq!(view.carried, Some((meat, amount)));
    assert_eq!(view.role, crate::population::CitizenRole::Carrier);
    // Goods on the carrier: neither in the farm's buffer nor in the storage yet.
    let stock = state.building(store).unwrap().inventory().get(meat);
    let mut delivered_at = None;
    for _ in 0..TICKS_PER_HOUR * 3 {
        let now = state.clock().tick + 1;
        let events = run(&mut state, 1);
        if state.logistics().job(job.id).is_some() {
            assert_eq!(state.building(store).unwrap().inventory().get(meat), stock, "nothing arrives before the carrier");
        }
        if events.iter().any(|e| matches!(e, SimEvent::TransportCompleted { job: j, amount: a, .. } if *j == job.id && *a == amount)) {
            delivered_at = Some(now);
            break;
        }
    }
    let delivered_at = delivered_at.expect("delivered");
    let handling = u64::from(d.rules().carrier_handling_ticks);
    assert!(delivered_at >= picked_at + handling, "walk and unload take time");
    assert_eq!(state.building(store).unwrap().inventory().get(meat), stock + amount);
    assert!(state.logistics().job(job.id).is_none());
    let c = state.population().citizen(carrier).unwrap();
    assert!(c.carry().is_none() && c.carried().is_none());
    assert_eq!(state.logistics().trips_completed(), 1);
    assert!(crate::report::logistics_report(&state, d).average_trip_minutes.is_some_and(|m| m > 0));
    state.validate_entities(d).expect("consistent after the trip");
}

#[test]
fn storages_serve_only_connected_buildings_within_their_radius() {
    let mut state = flat_state(96);
    let store = depot(&state);
    // 40 tiles away: connected by road but outside the warehouse's carrier radius (24).
    let far = spawn(&mut state, "chicken_farm", 40, 92);
    link(&mut state, far, store);
    let near = spawn(&mut state, "chicken_farm", 80, 92);
    link(&mut state, near, store);
    let isolated = spawn(&mut state, "chicken_farm", 80, 80);
    run(&mut state, 12 * TICKS_PER_HOUR);
    // Collected = less meat in the buffer than was produced there.
    let collected = |id| {
        let p = state.building(id).unwrap().production().unwrap();
        assert!(p.cycles_completed() >= 5);
        u64::from(p.output().get(res("meat"))) < p.cycles_completed()
    };
    assert!(collected(near));
    assert!(!collected(far), "outside the carrier radius");
    assert!(!collected(isolated), "no road");
    assert_eq!(crate::report::serving_storages(&state, defs(), far), Vec::new());
    assert_eq!(crate::report::serving_storages(&state, defs(), near), vec![store]);
}

#[test]
fn carriers_are_the_storage_workers() {
    let mut state = flat_state(48);
    let farm = spawn(&mut state, "chicken_farm", 30, 38);
    let store = depot(&state);
    link(&mut state, farm, store);
    // One adult: the depot's first carrier comes before everything else, the farm waits.
    crate::test_support::set_adults(&mut state, 1);
    run(&mut state, 2 * TICKS_PER_HOUR);
    assert_eq!(carriers::carriers_of(&state, defs(), store), 1);
    assert_eq!(state.building(farm).unwrap().staffed(), 0);
    // Two adults: the farm works and its one carrier takes the meat to the depot.
    crate::test_support::set_adults(&mut state, 2);
    run(&mut state, 12 * TICKS_PER_HOUR);
    assert_eq!(state.building(farm).unwrap().staffed(), 1);
    assert_eq!(carriers::carriers_of(&state, defs(), store), 1);
    assert!(state.economy().stats().today().produced.get(res("meat")) > 0);
    assert!(state.building(farm).unwrap().production().unwrap().output().get(res("meat")) < 8);
    let report = crate::report::logistics_report(&state, defs());
    assert_eq!(report.carriers, 1);
    // More adults: the depot gets all its carriers.
    crate::test_support::set_adults(&mut state, 4);
    run(&mut state, 1);
    assert_eq!(carriers::carriers_of(&state, defs(), store), 3);
}

#[test]
fn storages_without_carriers_leave_jobs_waiting_and_warn() {
    let mut state = flat_state(64);
    let meat = res("meat");
    let farm = spawn(&mut state, "chicken_farm", 4, 4);
    let store = spawn(&mut state, "storehouse", 10, 4);
    link(&mut state, farm, store);
    // The only adult is the depot's base carrier (lowest id); the storehouse has nobody, and
    // only the storehouse is connected to the farm.
    crate::test_support::set_adults(&mut state, 1);
    state.buildings.get_mut(farm).unwrap().production.as_mut().unwrap().output.add(meat, 8);
    let events = run(&mut state, 2 * NO_CARRIER_CHECK_TICKS);
    assert_eq!(state.building(store).unwrap().staffed(), 0);
    assert!(state.logistics().waiting().any(|j| j.carrier_base == store && j.resource == meat));
    assert_eq!(storages_without_carriers(&state, defs()), vec![store]);
    let warnings: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            SimEvent::Notification(n) => Some(n.text.as_str()),
            _ => None,
        })
        .filter(|t| t.starts_with("Keine Träger"))
        .collect();
    assert_eq!(warnings, vec!["Keine Träger: Vorratsspeicher hat keine Arbeiter, Transporte bleiben liegen."; 2]);
    // A second adult becomes the storehouse's carrier: the meat moves, the warning stops.
    crate::test_support::set_adults(&mut state, 2);
    run(&mut state, crate::time::TICKS_PER_DAY);
    assert_eq!(state.building(store).unwrap().staffed(), 1);
    assert!(storages_without_carriers(&state, defs()).is_empty());
    assert!(state.building(store).unwrap().inventory().get(meat) > 0);
}

#[test]
fn goods_for_a_demolished_target_go_back_to_storage() {
    let mut state = flat_state(48);
    let flour = res("flour");
    let bakery = spawn(&mut state, "bakery", 30, 38);
    let store = depot(&state);
    link(&mut state, bakery, store);
    let before = state.economy().stock().get(flour);
    // Run until flour is on its way, then demolish the bakery.
    for _ in 0..crate::time::TICKS_PER_DAY {
        run(&mut state, 1);
        if state.logistics().jobs().iter().any(|j| j.resource == flour && j.is_delivering()) {
            break;
        }
    }
    assert!(state.economy().stock().get(flour) < before);
    // Flour lives in the storage, on the carriers and in the bakery's buffer (it may have
    // started a cycle with some).
    let in_bakery = |s: &GameState| s.building(bakery).and_then(|b| b.production()).map_or(0, |p| p.input().get(flour));
    let on_carriers = |s: &GameState| s.logistics().goods_in_transit().iter().filter(|m| m.resource == flour).map(|m| m.amount).sum::<u32>();
    let held = |s: &GameState| s.economy().stock().get(flour) + on_carriers(s) + in_bakery(s);
    let total = held(&state);
    let events = exec(&mut state, Command::DemolishBuilding { building: bakery });
    assert!(events.iter().any(|e| matches!(e, SimEvent::BuildingDemolished { .. })));
    assert!(on_carriers(&state) > 0, "a carrier still has flour for the demolished bakery");
    for _ in 0..crate::time::TICKS_PER_DAY {
        run(&mut state, 1);
        assert_eq!(held(&state), total, "flour is never lost or duplicated");
        if on_carriers(&state) == 0 {
            break;
        }
    }
    assert_eq!(on_carriers(&state), 0, "the carrier took the flour to the storage");
    assert!(state.logistics().jobs().iter().all(|j| j.target != bakery));
    assert_eq!(state.economy().stock().get(flour), total, "nothing was lost");
    state.validate_entities(defs()).expect("consistent");
}

#[test]
fn recipe_changes_return_surplus_inputs() {
    let mut state = flat_state(48);
    let (cloth, leather) = (res("cloth"), res("leather"));
    let tailor = spawn(&mut state, "tailor", 30, 38);
    let store = depot(&state);
    link(&mut state, tailor, store);
    // Recipe 0 (cloth + leather): both are delivered.
    crate::test_support::run_until(&mut state, crate::time::TICKS_PER_DAY, |s| s.building(tailor).unwrap().production().unwrap().input().get(leather) > 0);
    let p = state.building(tailor).unwrap().production().unwrap();
    assert!(p.input().get(leather) > 0 && p.input().capacity(leather) > 0);
    // Recipe 1 needs only cloth: leather becomes surplus and is carried back.
    exec(&mut state, Command::SetRecipe { building: tailor, recipe: 1 });
    assert_eq!(state.building(tailor).unwrap().production().unwrap().input().capacity(leather), 0);
    crate::test_support::run_until(&mut state, crate::time::TICKS_PER_DAY, |s| s.building(tailor).unwrap().production().unwrap().input().get(leather) == 0);
    let p = state.building(tailor).unwrap().production().unwrap();
    assert_eq!(p.input().get(leather), 0);
    assert!(p.input().get(cloth) > 0 || p.cycle().is_some());
    assert!(state.logistics().jobs().iter().all(|j| j.kind != TransportKind::Return || j.resource == leather));
}

#[test]
fn construction_sites_receive_material_in_carrier_loads() {
    let mut state = flat_state(48);
    let wood = res("wood");
    let kind = defs().building_id("sawmill").unwrap();
    let events = exec(&mut state, Command::PlaceBuilding { kind, origin: IVec2::new(30, 36), rotation: crate::grid::Rotation::default() });
    let site = events
        .iter()
        .find_map(|e| match e {
            SimEvent::BuildingPlaced { building, .. } => Some(*building),
            _ => None,
        })
        .unwrap();
    let store = depot(&state);
    link(&mut state, site, store);
    assert_eq!(state.economy().committed().get(wood), 15);
    run(&mut state, PLAN_INTERVAL_TICKS + 1);
    let loads: Vec<u32> = state
        .logistics()
        .jobs()
        .iter()
        .filter(|j| j.kind == TransportKind::Construction && j.resource == wood)
        .map(|j| j.amount)
        .collect();
    let capacity = u32::from(defs().rules().carrier_capacity);
    assert_eq!(loads.iter().sum::<u32>(), 15);
    assert!(loads.iter().all(|l| *l <= capacity));
    // Committed material shrinks as it leaves the storage.
    let in_transit: u32 = state.logistics().in_transit_to(site, wood);
    assert_eq!(state.economy().committed().get(wood), 15 - in_transit);
    crate::test_support::run_until(&mut state, crate::time::TICKS_PER_DAY, |s| s.building(site).unwrap().construction_site().is_none_or(|s| s.fully_supplied()));
    let s = state.building(site).unwrap().construction_site().map(|s| s.delivered(wood));
    assert!(s.is_none_or(|delivered| delivered == 15));
    assert_eq!(state.economy().committed().get(wood), 0);
}

#[test]
fn high_priority_clients_are_served_first() {
    let mut state = flat_state(48);
    let flour = res("flour");
    let store = depot(&state);
    // Only 8 flour for two bakeries: the high-priority one gets it.
    {
        let b = state.buildings.get_mut(store).unwrap();
        let have = b.inventory.get(flour);
        b.inventory.remove(flour, have);
        b.inventory.add(flour, 8);
    }
    let low = spawn(&mut state, "bakery", 26, 38);
    let high = spawn(&mut state, "bakery", 34, 38);
    link(&mut state, low, store);
    link(&mut state, high, store);
    exec(&mut state, Command::SetProductionPriority { building: high, priority: JobPriority::High });
    run(&mut state, crate::time::TICKS_PER_DAY);
    let flour_at = |s: &GameState, id| {
        let p = s.building(id).unwrap().production().unwrap();
        u64::from(p.input().get(flour)) + 2 * (p.cycles_completed() + u64::from(p.cycle().is_some()))
    };
    assert_eq!(flour_at(&state, high), 8);
    assert_eq!(flour_at(&state, low), 0);
}

/// Runs until a carrier has goods of `resource` on the way (at most two days); returns the job.
fn run_until_carrying(state: &mut GameState, resource: ResourceId) -> TransportJob {
    for _ in 0..2 * crate::time::TICKS_PER_DAY {
        if let Some(j) = state.logistics().jobs().iter().find(|j| j.resource == resource && j.is_delivering()) {
            return *j;
        }
        run(state, 1);
    }
    panic!("no carrier picked up {resource:?}");
}

/// Units of `resource` in all buildings (storages, buffers, sites) and on carriers.
fn held(state: &GameState, resource: ResourceId) -> u32 {
    let in_buildings: u32 =
        state.buildings().iter().flat_map(|(_, b)| b.all_goods()).filter(|m| m.resource == resource).map(|m| m.amount).sum();
    let carried: u32 = state.population().citizens().filter_map(|(_, c)| c.carried()).filter(|m| m.resource == resource).map(|m| m.amount).sum();
    // Households eat from the storages; what they took counts as held.
    let stats = state.economy().stats();
    let eaten: u32 = stats.history().map(|d| d.flows.household.get(resource)).sum::<u32>() + stats.today().household.get(resource);
    in_buildings + carried + eaten
}

/// A chicken farm next to the depot with eight meat waiting; the farm itself has no worker, so
/// the meat only moves (nothing is produced or eaten).
fn meat_fixture() -> (GameState, BuildingId, BuildingId) {
    let mut state = flat_state(48);
    let farm = spawn(&mut state, "chicken_farm", 30, 38);
    let store = depot(&state);
    link(&mut state, farm, store);
    // Three adults: the depot's carriers. The paused farm has no worker but its meat is
    // still collected.
    crate::test_support::set_adults(&mut state, 3);
    exec(&mut state, Command::SetBuildingPaused { building: farm, paused: true });
    state.buildings.get_mut(farm).unwrap().production.as_mut().unwrap().output.add(res("meat"), 8);
    (state, farm, store)
}

#[test]
fn a_carrier_who_dies_hands_the_goods_over() {
    let d = defs();
    let meat = res("meat");
    let (mut state, _farm, store) = meat_fixture();
    let total = held(&state, meat);
    let job = run_until_carrying(&mut state, meat);
    let carrier = job.carrier().unwrap();
    assert_eq!(held(&state, meat), total, "goods on the carrier are counted once");
    let in_store = state.building(store).unwrap().inventory().get(meat);
    let mut events = Vec::new();
    crate::population::remove_citizen(&mut state, d, carrier, &mut events);
    assert!(state.logistics().job(job.id).is_none());
    assert_eq!(state.building(store).unwrap().inventory().get(meat), in_store + job.amount, "handed over to the storage");
    assert_eq!(held(&state, meat), total, "nothing was lost");
    assert!(!events.iter().any(|e| matches!(e, SimEvent::ResourcesLost { .. })));
    run(&mut state, 1);
    state.validate_entities(d).expect("consistent");
}

#[test]
fn a_carrier_who_loses_its_job_finishes_the_delivery() {
    let d = defs();
    let meat = res("meat");
    let (mut state, _farm, store) = meat_fixture();
    let total = held(&state, meat);
    let job = run_until_carrying(&mut state, meat);
    let carrier = job.carrier().unwrap();
    // The carrier leaves the storage's service (e.g. it took another job).
    state.population.set_job(carrier, None);
    let staffed = state.building(store).unwrap().staffed();
    state.buildings.get_mut(store).unwrap().set_staffed(staffed - 1);
    run_until(&mut state, crate::time::TICKS_PER_DAY, |s| s.logistics().job(job.id).is_none());
    assert_eq!(held(&state, meat), total);
    assert!(state.population().citizen(carrier).unwrap().carry().is_none(), "delivered");
    state.validate_entities(d).expect("consistent");
}

#[test]
fn a_carrier_without_room_anywhere_stacks_the_goods_in_its_target() {
    let d = defs();
    let meat = res("meat");
    let (mut state, _farm, store) = meat_fixture();
    let total = held(&state, meat);
    let job = run_until_carrying(&mut state, meat);
    let carrier = job.carrier().unwrap();
    // The storage fills up while the carrier is on its way; there is no other storage.
    let free = state.building(store).unwrap().inventory().free_space(meat);
    state.buildings.get_mut(store).unwrap().inventory.add(meat, free);
    let total = total + free;
    let capacity = state.building(store).unwrap().inventory().capacity(meat);
    let mut texts = Vec::new();
    for _ in 0..crate::time::TICKS_PER_HOUR * 2 {
        run(&mut state, 1);
        assert_eq!(held(&state, meat), total, "nothing is lost or duplicated");
        if let Some(t) = crate::population::activity_text(&state, d, carrier) {
            texts.push(t);
        }
        if state.logistics().job(job.id).is_none() {
            break;
        }
    }
    // It does not wait for hours (and block the storage's other jobs): the load is stacked
    // beyond the capacity, the surplus is used up first.
    assert!(state.logistics().job(job.id).is_none(), "delivered");
    assert!(state.population().citizen(carrier).unwrap().carry().is_none());
    assert_eq!(state.building(store).unwrap().inventory().get(meat), capacity + job.amount);
    assert_eq!(state.building(store).unwrap().inventory().free_space(meat), 0);
    assert!(texts.iter().any(|t| t.starts_with("Lädt ab")), "{texts:?}");
    assert!(!texts.iter().any(|t| t.starts_with("Wartet mit")), "{texts:?}");
    state.validate_entities(d).expect("consistent");
}

/// Review finding: an unclaimed inflow filled the room a carrier was bringing goods to; the
/// carrier waited inside the full storage for days, and a construction site that needed
/// material from that storage never got it (its only carrier was not free).
#[test]
fn a_full_storage_never_keeps_its_carrier_from_other_jobs() {
    let d = defs();
    let meat = res("meat");
    let wood = res("wood");
    let mut state = flat_state(48);
    let farm = spawn(&mut state, "chicken_farm", 30, 38);
    let store = depot(&state);
    link(&mut state, farm, store);
    crate::test_support::set_adults(&mut state, 1);
    exec(&mut state, Command::SetBuildingPaused { building: farm, paused: true });
    state.buildings.get_mut(farm).unwrap().production.as_mut().unwrap().output.add(meat, 8);
    // The depot has room for exactly the eight meat.
    let free = state.building(store).unwrap().inventory().free_space(meat);
    state.buildings.get_mut(store).unwrap().inventory.add(meat, free - 8);
    let job = run_until_carrying(&mut state, meat);
    // A purchase / refund fills the claimed room (no other storage has any).
    let at = state.building(store).unwrap().entrance();
    assert_eq!(crate::economy::storage::deposit(&mut state, d, meat, 8, at), 8);
    // A site that needs wood from the depot.
    let kind = d.building_id("sawmill").unwrap();
    let events = exec(&mut state, Command::PlaceBuilding { kind, origin: IVec2::new(30, 30), rotation: crate::grid::Rotation::default() });
    let site = events
        .iter()
        .find_map(|e| match e {
            SimEvent::BuildingPlaced { building, .. } => Some(*building),
            _ => None,
        })
        .expect("site placed");
    link(&mut state, site, store);
    let needed = state.building(site).unwrap().construction_site().unwrap().outstanding(wood);
    assert!(needed > 0 && state.building(store).unwrap().inventory().get(wood) >= needed);
    run_until(&mut state, 3 * crate::time::TICKS_PER_DAY, |s| {
        s.building(site).and_then(|b| b.construction_site()).is_none_or(|c| c.outstanding(wood) == 0)
    });
    assert!(state.logistics().job(job.id).is_none(), "the meat was delivered");
    let site_ref = state.building(site).unwrap();
    assert!(site_ref.construction_site().is_none_or(|c| c.outstanding(wood) == 0), "the site got its wood");
    state.validate_entities(d).expect("consistent");
}

#[test]
fn deposits_leave_room_claimed_by_carriers_to_them() {
    let d = defs();
    let meat = res("meat");
    let (mut state, _farm, store) = meat_fixture();
    let job = run_until_carrying(&mut state, meat);
    assert_eq!(job.target, store);
    let other = spawn(&mut state, "storehouse", 8, 38);
    link(&mut state, store, other);
    crate::economy::storage::refresh_stock(&mut state, d);
    // Fill the depot up to the claimed room; a deposit next to it goes to the storehouse.
    let free = state.building(store).unwrap().inventory().free_space(meat);
    state.buildings.get_mut(store).unwrap().inventory.add(meat, free - job.amount);
    let at = state.building(store).unwrap().entrance();
    assert_eq!(crate::economy::storage::deposit(&mut state, d, meat, 5, at), 5);
    assert_eq!(state.building(store).unwrap().inventory().free_space(meat), job.amount, "claimed room kept");
    assert_eq!(state.building(other).unwrap().inventory().get(meat), 5);
    // Without unclaimed room anywhere the claimed room is used rather than losing goods.
    let free_other = state.building(other).unwrap().inventory().free_space(meat);
    state.buildings.get_mut(other).unwrap().inventory.add(meat, free_other);
    assert_eq!(crate::economy::storage::deposit(&mut state, d, meat, 3, at), 3);
    assert_eq!(state.building(store).unwrap().inventory().free_space(meat), job.amount - 3);
}

#[test]
fn a_carrier_at_a_full_workshop_says_it_waits_for_room() {
    let d = defs();
    let flour = res("flour");
    let mut state = flat_state(48);
    let bakery = spawn(&mut state, "bakery", 30, 38);
    let store = depot(&state);
    link(&mut state, bakery, store);
    let job = run_until_carrying(&mut state, flour);
    assert_eq!(job.target, bakery);
    let carrier = job.carrier().unwrap();
    run_until(&mut state, crate::time::TICKS_PER_DAY, |s| {
        s.population().citizen(carrier).is_some_and(|c| c.motion().inside() == Some(bakery))
    });
    // The bakery's input is full and the carrier's own storage stopped working: nowhere to go.
    {
        let input = &mut state.buildings.get_mut(bakery).unwrap().production.as_mut().unwrap().input;
        let free = input.free_space(flour);
        input.add(flour, free);
    }
    exec(&mut state, Command::SetBuildingPaused { building: store, paused: true });
    exec(&mut state, Command::SetBuildingPaused { building: bakery, paused: true });
    run(&mut state, u64::from(d.rules().carrier_handling_ticks) + 2);
    assert!(state.logistics().job(job.id).is_some_and(|j| j.is_delivering()), "still carrying");
    let text = crate::population::activity_text(&state, d, carrier).unwrap();
    assert_eq!(text, format!("Wartet mit {} Mehl auf Lagerplatz (Bäckerei)", job.amount));
}

#[test]
fn an_unreachable_target_sends_the_goods_to_another_storage() {
    let d = defs();
    let meat = res("meat");
    let (mut state, farm, store) = meat_fixture();
    // Outside the farm's carrier radius of the storehouse: the planner picks the depot.
    let other = spawn(&mut state, "storehouse", 8, 38);
    link(&mut state, farm, other);
    let total = held(&state, meat);
    let job = run_until_carrying(&mut state, meat);
    assert_eq!(job.target, store);
    // Something is built on the depot's entrance: nobody can get in any more.
    let entrance = state.building(store).unwrap().entrance();
    state.roads.set(entrance, None);
    state.roads.rebuild_components();
    spawn(&mut state, "well", entrance.x, entrance.y);
    run_until(&mut state, crate::time::TICKS_PER_DAY, |s| s.logistics().job(job.id).is_none());
    assert_eq!(held(&state, meat), total);
    assert_eq!(state.building(other).unwrap().inventory().get(meat), job.amount, "taken to the reachable storehouse");
    state.validate_entities(d).expect("consistent");
}

#[test]
fn waiting_jobs_for_the_same_goods_share_one_trip() {
    let meat = res("meat");
    let (mut state, farm, store) = meat_fixture();
    let capacity = u32::from(defs().rules().carrier_capacity);
    let small = capacity / 2 - 1;
    let template = TransportJob {
        id: 0,
        kind: TransportKind::Collect,
        resource: meat,
        amount: small,
        source: farm,
        target: store,
        carrier_base: store,
        priority: JobPriority::Normal,
        created_at: 0,
        stage: JobStage::Waiting,
    };
    for _ in 0..3 {
        state.logistics.push(template);
    }
    run_until(&mut state, 2 * crate::time::TICKS_PER_DAY, |s| s.logistics().jobs().iter().any(|j| j.occupies_carrier()));
    let job = *state.logistics().jobs().iter().find(|j| j.occupies_carrier()).unwrap();
    assert_eq!(job.amount, capacity, "one load instead of three small ones");
    let left: u32 = state.logistics().jobs().iter().filter(|j| j.id != job.id).map(|j| j.amount).sum();
    assert_eq!(job.amount + left, 3 * small, "the rest of the third job still waits");
}

#[test]
fn a_carrier_takes_the_return_load_where_it_delivered() {
    let d = defs();
    let (flour, bread) = (res("flour"), res("bread"));
    let mut state = flat_state(48);
    let bakery = spawn(&mut state, "bakery", 30, 38);
    let store = depot(&state);
    link(&mut state, bakery, store);
    crate::test_support::set_adults(&mut state, 1);
    exec(&mut state, Command::SetBuildingPaused { building: bakery, paused: true });
    state.buildings.get_mut(bakery).unwrap().paused = false;
    state.buildings.get_mut(bakery).unwrap().production.as_mut().unwrap().output.add(bread, 4);
    let base = TransportJob {
        id: 0,
        kind: TransportKind::Supply,
        resource: flour,
        amount: 4,
        source: store,
        target: bakery,
        carrier_base: store,
        priority: JobPriority::Normal,
        created_at: 0,
        stage: JobStage::Waiting,
    };
    let supply = state.logistics.push(base);
    let collect =
        state.logistics.push(TransportJob { kind: TransportKind::Collect, resource: bread, source: bakery, target: store, ..base });
    exec(&mut state, Command::SetBuildingPaused { building: bakery, paused: true });
    let job = run_until_carrying(&mut state, flour);
    assert_eq!(job.id, supply);
    let carrier = job.carrier().unwrap();
    run_until(&mut state, crate::time::TICKS_PER_DAY, |s| s.logistics().job(supply).is_none());
    let next = state.logistics().job(collect).copied().expect("bread still to carry");
    assert_eq!(next.carrier(), Some(carrier), "the carrier took the bread right where it delivered the flour");
    let c = state.population().citizen(carrier).unwrap();
    assert!(c.motion().inside() == Some(bakery) || c.motion().pos() == crate::population::SubPos::center(state.building(bakery).unwrap().door()));
    run_until(&mut state, crate::time::TICKS_PER_DAY, |s| s.logistics().job(collect).is_none());
    state.validate_entities(d).expect("consistent");
}

#[test]
fn reports_name_waiting_transports_and_idle_carriers() {
    let d = defs();
    let meat = res("meat");
    let (mut state, farm, store) = meat_fixture();
    // At night the collection waits: the building reports it.
    run(&mut state, PLAN_INTERVAL_TICKS + 1);
    let wait = crate::report::carrier_wait(&state, farm).expect("a transport waits");
    assert_eq!(wait.jobs, 1);
    assert_eq!(wait.units, 8);
    assert!(wait.text.starts_with("Wartet auf Träger: 1 Transport (8 Einheiten) seit "), "{}", wait.text);
    let storage = crate::report::storage_report(&state, d).into_iter().find(|s| s.building == store).unwrap();
    assert_eq!(storage.waiting_jobs, 1);
    assert_eq!(storage.idle_carriers, 0, "the carriers are not at work yet");
    let job = run_until_carrying(&mut state, meat);
    let report = crate::report::logistics_report(&state, d);
    assert_eq!(report.delivering, 1);
    assert_eq!(report.units_in_transit, job.amount);
    assert!(report.busy_carriers >= 1 && report.carriers == 3);
    assert_eq!(state.logistics().goods_in_transit(), vec![crate::defs::ResourceAmount { resource: meat, amount: job.amount }]);
    assert!(crate::report::carrier_wait(&state, farm).is_none());
    let detail = crate::report::citizen_detail(&state, d, job.carrier().unwrap()).unwrap();
    assert_eq!(detail.carried, Some(crate::defs::ResourceAmount { resource: meat, amount: job.amount }));
    assert!(detail.activity_text.starts_with("Bringt 8 Fleisch") || detail.activity_text.starts_with("Lädt"), "{}", detail.activity_text);
}

#[test]
fn validate_rejects_goods_that_do_not_match_their_job() {
    let d = defs();
    let meat = res("meat");
    let (mut state, _farm, _store) = meat_fixture();
    let job = run_until_carrying(&mut state, meat);
    let carrier = job.carrier().unwrap();
    state.validate_entities(d).expect("consistent while carrying");
    let mut broken = state.clone();
    broken.population.citizens.get_mut(carrier).unwrap().carry.as_mut().unwrap().load.as_mut().unwrap().amount += 1;
    assert!(broken.validate_entities(d).is_err(), "one unit too many on the carrier");
    let mut broken = state.clone();
    broken.population.citizens.get_mut(carrier).unwrap().carry = None;
    assert!(broken.validate_entities(d).is_err(), "goods vanished from the carrier");
    let mut broken = state.clone();
    broken.logistics.job_mut(job.id).unwrap().stage = JobStage::Waiting;
    assert!(broken.validate_entities(d).is_err(), "carried goods without a carrying job");
}
