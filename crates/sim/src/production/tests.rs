//! Production tests: chains over simulated days, status reasons, tools, gathering, queues.

use glam::IVec2;

use super::*;
use crate::buildings::BuildingId;
use crate::command::Command;
use crate::defs::ResourceId;
use crate::event::SimEvent;
use crate::logistics::JobStage;
use crate::state::GameState;
use crate::test_support::{defs, depot, exec, flat_state, link, rejection, run, run_until, run_until_at_work, spawn};
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
use crate::world::{Deposit, DepositKind, Terrain, TreeKind, Vegetation};

fn res(key: &str) -> ResourceId {
    defs().resource_id(key).unwrap()
}

/// Spawns a finished building and connects it to the fixture depot.
fn linked(state: &mut GameState, key: &str, x: i32, y: i32) -> BuildingId {
    let id = spawn(state, key, x, y);
    let d = depot(state);
    link(state, id, d);
    id
}

/// Lets the job allocation run, then waits until every worker has walked to work.
fn work(state: &mut GameState) {
    run(state, 1);
    run_until_at_work(state);
    run(state, 2);
}

fn prod(state: &GameState, id: BuildingId) -> &ProductionState {
    state.building(id).unwrap().production().unwrap()
}

/// Units of `resource` anywhere: storages, production buffers and carriers.
fn total(state: &GameState, resource: ResourceId) -> u64 {
    let held: u64 = state
        .buildings()
        .iter()
        .flat_map(|(_, b)| b.all_goods())
        .filter(|m| m.resource == resource)
        .map(|m| u64::from(m.amount))
        .sum();
    let carried: u64 = state
        .logistics()
        .jobs()
        .iter()
        .filter(|j| j.resource == resource && matches!(j.stage, JobStage::Carrying { .. }))
        .map(|j| u64::from(j.amount))
        .sum();
    held + carried + taken_by_households(state, resource)
}

/// Units households took from the storages so far (eaten or in their pantries).
fn taken_by_households(state: &GameState, resource: ResourceId) -> u64 {
    let stats = state.economy().stats();
    stats.history().map(|d| u64::from(d.flows.household.get(resource))).sum::<u64>()
        + u64::from(stats.today().household.get(resource))
}

/// Cycles whose inputs were consumed: completed ones plus the running one.
fn started_cycles(state: &GameState, id: BuildingId) -> u64 {
    let p = prod(state, id);
    p.cycles_completed() + u64::from(p.cycle().is_some())
}

fn set_stock(state: &mut GameState, resource: ResourceId, amount: u32) {
    let d = depot(state);
    let b = state.buildings.get_mut(d).unwrap();
    let have = b.inventory.get(resource);
    b.inventory.remove(resource, have);
    b.inventory.add(resource, amount);
    crate::economy::storage::refresh_stock(state, defs());
}

fn produced_total(state: &GameState, resource: ResourceId) -> u64 {
    let stats = state.economy().stats();
    stats.history().map(|d| u64::from(d.flows.produced.get(resource))).sum::<u64>()
        + u64::from(stats.today().produced.get(resource))
}

#[test]
fn wheat_flour_bread_chain_over_two_days() {
    let mut state = flat_state(48);
    let (wheat, flour, bread, wood) = (res("wheat"), res("flour"), res("bread"), res("wood"));
    set_stock(&mut state, flour, 0);
    set_stock(&mut state, bread, 0);
    let mill = linked(&mut state, "mill", 36, 38);
    let bakery = linked(&mut state, "bakery", 31, 38);
    let wheat0 = total(&state, wheat);
    let wood0 = total(&state, wood);
    run(&mut state, 2 * TICKS_PER_DAY);

    let mill_started = started_cycles(&state, mill);
    let mill_done = prod(&state, mill).cycles_completed();
    let bakery_started = started_cycles(&state, bakery);
    let bakery_done = prod(&state, bakery).cycles_completed();
    // Conservation: every unit is accounted for by the recipes.
    assert_eq!(wheat0 - total(&state, wheat), 2 * mill_started, "2 wheat per mill cycle");
    assert_eq!(total(&state, flour), 2 * mill_done - 2 * bakery_started, "2 flour in, 2 flour out");
    assert_eq!(total(&state, bread), 4 * bakery_done, "4 bread per bakery cycle");
    assert_eq!(wood0 - total(&state, wood), bakery_started, "1 wood per bakery cycle");
    assert_eq!(produced_total(&state, bread), 4 * bakery_done);
    assert_eq!(produced_total(&state, flour), 2 * mill_done);
    // Throughput: a mill cycle takes 2 h with its single worker (24 in two days); carriers keep
    // it busy most of the time. The bakery is limited by the flour it gets: every unit takes
    // two carrier trips (mill → storage → bakery), and carriers walk only during the shift
    // (step 3b), so each morning starts with deliveries.
    assert!(mill_done >= 20, "mill completed only {mill_done} cycles");
    assert!(bakery_done >= 14, "bakery completed only {bakery_done} cycles");
    // Bread reached the kingdom stock.
    assert!(u64::from(state.economy().stock().get(bread)) + taken_by_households(&state, bread) >= 4 * (bakery_done - 2));
    // The day's statistics were closed and stored.
    let yesterday = state.economy().stats().yesterday().unwrap();
    assert!(yesterday.flows.produced.get(bread) > 0 && yesterday.flows.consumed.get(wheat) > 0);
}

#[test]
fn wood_planks_furniture_chain_driven_by_a_woodcutter() {
    let mut state = flat_state(48);
    let (wood, planks, furniture, tools) = (res("wood"), res("planks"), res("furniture"), res("tools"));
    set_stock(&mut state, wood, 0);
    set_stock(&mut state, planks, 0);
    set_stock(&mut state, furniture, 0);
    let cutter = linked(&mut state, "woodcutter", 30, 30);
    let sawmill = linked(&mut state, "sawmill", 36, 36);
    let carpenter = linked(&mut state, "carpenter", 40, 30);
    // A small forest north-west of the woodcutter.
    let mut trees = 0;
    for y in 22..28 {
        for x in 24..34 {
            let p = IVec2::new(x, y);
            if state.buildings().id_at(p).is_none() && !state.roads().has_road(p) {
                state.world.tile_mut(p).vegetation = Vegetation::Tree { kind: TreeKind::Conifer, growth: 230 };
                trees += 1;
            }
        }
    }
    let tools0 = total(&state, tools);
    let mut felled = 0;
    for _ in 0..3 * TICKS_PER_DAY {
        felled += run(&mut state, 1).iter().filter(|e| matches!(e, SimEvent::TreeFelled { .. })).count() as u64;
    }
    let cut = prod(&state, cutter).cycles_completed();
    let sawn = prod(&state, sawmill).cycles_completed();
    let made = prod(&state, carpenter).cycles_completed();
    assert_eq!(felled, cut, "one tree per woodcutter cycle");
    // The woodcutters walk to every tree and one of them carries each log back to the hut
    // (step 3b); the first cycles run without tools until a carrier brings some.
    assert!(cut >= 21, "woodcutter completed only {cut} cycles");
    let standing = (22..28)
        .flat_map(|y| (24..34).map(move |x| IVec2::new(x, y)))
        .filter(|p| state.world().tile(*p).vegetation.is_tree())
        .count() as u64;
    assert_eq!(standing, trees - felled);
    // Wood: 1 per felled tree, 2 per sawmill cycle.
    assert_eq!(total(&state, wood), cut - 2 * started_cycles(&state, sawmill));
    // Planks: 3 per sawmill cycle, 3 per carpenter cycle.
    assert_eq!(total(&state, planks), 3 * sawn - 3 * started_cycles(&state, carpenter));
    assert_eq!(total(&state, furniture), made);
    // Carriers keep the chain moving: the sawmill turns most of the wood into planks (2 per
    // cycle), the carpenter most of the planks into furniture (3 per cycle); a few units are on
    // the way or wait in buffers at the end.
    assert!(sawn + 3 >= cut / 2 && made + 3 >= sawn, "woodcutter {cut}, sawmill {sawn}, carpenter {made}");
    // Tools: the carpenter's recipe takes 0.25 per cycle, and every tool-wearing building takes
    // one tool per 20 cycles.
    let tool_users = [cutter, sawmill, carpenter].len() as u64;
    let used = tools0 - total(&state, tools);
    let recipe_tools = started_cycles(&state, carpenter).div_ceil(4);
    assert!(used >= recipe_tools && used <= recipe_tools + 2 * tool_users, "{used} tools used");
}

#[test]
fn missing_input_pauses_with_a_shortage_warning() {
    let mut state = flat_state(48);
    let flour = res("flour");
    set_stock(&mut state, flour, 0);
    let bakery = linked(&mut state, "bakery", 34, 38);
    // Wood arrives (carriers work from 08:00), flour never: after an hour without flour on its
    // way the player is warned.
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| prod(s, bakery).input().get(res("wood")) > 0);
    let events = run(&mut state, 3 * TICKS_PER_HOUR);
    assert_eq!(prod(&state, bakery).status(), ProductionStatus::MissingInput(flour));
    let shortages: Vec<&SimEvent> = events.iter().filter(|e| matches!(e, SimEvent::Shortage { .. })).collect();
    assert_eq!(shortages.len(), 1, "warned once");
    let entrance = state.building(bakery).unwrap().entrance();
    assert!(matches!(shortages[0], SimEvent::Shortage { building, resource, position }
        if *building == bakery && *resource == flour && *position == entrance));
    assert!(events.iter().filter_map(SimEvent::notification).any(|n| n.text.contains("Mehl") && n.position == Some(entrance)));
    assert!(events.iter().any(|e| matches!(e, SimEvent::ProductionStatusChanged { status: ProductionStatus::MissingInput(_), .. })));
    assert_eq!(prod(&state, bakery).cycles_completed(), 0);
    assert_eq!(crate::report::status_text(defs(), prod(&state, bakery).status()), "Rohstoffmangel: Mehl fehlt");
    // Flour arrives: the bakery works again.
    set_stock(&mut state, flour, 50);
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| started_cycles(s, bakery) > 0);
    // No further warning while it works.
    let events = run(&mut state, 200);
    assert!(!events.iter().any(|e| matches!(e, SimEvent::Shortage { .. })));
}

#[test]
fn full_output_stops_and_disconnected_buildings_cannot_deliver() {
    let mut state = flat_state(48);
    let meat = res("meat");
    let farm = spawn(&mut state, "chicken_farm", 34, 38);
    let stock0 = state.economy().stock().get(meat);
    // 1 meat per 2 h; the 8-unit buffer is full after 8 cycles (16 h).
    run(&mut state, 17 * TICKS_PER_HOUR);
    let p = prod(&state, farm);
    assert_eq!(p.output().get(meat), 8);
    assert_eq!(p.status(), ProductionStatus::OutputFull);
    assert_eq!(p.cycles_completed(), 8);
    assert!(state.logistics().jobs().is_empty(), "no transport without a road");
    let stock_and_eaten = |state: &GameState| u64::from(state.economy().stock().get(meat)) + taken_by_households(state, meat);
    assert_eq!(stock_and_eaten(&state), u64::from(stock0));
    // A road to the depot: carriers collect and production resumes the next working day.
    let d = depot(&state);
    link(&mut state, farm, d);
    run(&mut state, 13 * TICKS_PER_HOUR);
    assert!(stock_and_eaten(&state) >= u64::from(stock0) + 8);
    assert!(prod(&state, farm).cycles_completed() > 8);
}

#[test]
fn staffing_and_tools_scale_the_cycle_time() {
    let mut state = flat_state(48);
    let tools = res("tools");
    set_stock(&mut state, tools, 0);
    // Chicken farm: no tools, 1 worker, 2 h per cycle at full staffing.
    let farm = linked(&mut state, "chicken_farm", 34, 38);
    work(&mut state);
    let p = prod(&state, farm);
    assert_eq!(p.status(), ProductionStatus::Working);
    assert_eq!(p.productivity_permille(), 1000);
    let required = p.cycle().unwrap().required;
    assert_eq!(required, 2 * TICKS_PER_HOUR * 1000);
    // Present workers advance at the shift factor: 3 milli-ticks of progress per tick.
    let before = p.cycle().unwrap().progress;
    run(&mut state, 1);
    assert_eq!(prod(&state, farm).cycle().unwrap().progress - before, 3000);
    // Sawmill without tools: 60 % productivity once its wood arrived; half staffed: 30 %.
    let sawmill = linked(&mut state, "sawmill", 30, 38);
    for _ in 0..TICKS_PER_DAY {
        run(&mut state, 1);
        if prod(&state, sawmill).cycle().is_some() {
            break;
        }
    }
    run(&mut state, 1);
    let p = prod(&state, sawmill);
    assert!(p.cycle().is_some_and(|c| !c.with_tools));
    assert_eq!(p.productivity_permille(), 600);
    // Three workers left: the depot's base carrier, then food (the farm), then processing
    // (one for the sawmill).
    crate::test_support::set_adults(&mut state, 3);
    run(&mut state, 1);
    assert_eq!(state.building(sawmill).unwrap().staffed(), 1);
    run_until(&mut state, TICKS_PER_DAY, |s| crate::population::present_workers(s, defs(), sawmill) == 1);
    run(&mut state, 1);
    assert_eq!(prod(&state, sawmill).productivity_permille(), 300);
    // With tools in stock the next cycle runs at full speed and one tool lasts 20 cycles.
    crate::test_support::set_adults(&mut state, 20);
    set_stock(&mut state, tools, 10);
    for _ in 0..(3 * TICKS_PER_DAY) {
        run(&mut state, 1);
        if prod(&state, sawmill).cycle().is_some_and(|c| c.with_tools) {
            break;
        }
    }
    let p = prod(&state, sawmill);
    assert!(p.cycle().is_some_and(|c| c.with_tools));
    assert_eq!(p.tool_charge(), defs().rules().tool_wear_cycles - 1);
}

#[test]
fn woodcutters_fell_foresters_plant_and_mines_deplete() {
    let d = defs();
    let mut state = flat_state(48);
    // Forester plants saplings at least two tiles away within radius 7.
    let forester = linked(&mut state, "forester", 20, 20);
    state.world.take_dirty_chunks();
    let mut planted = Vec::new();
    for _ in 0..(5 * TICKS_PER_HOUR) {
        for e in run(&mut state, 1) {
            if let SimEvent::TreePlanted { position } = e {
                planted.push(position);
            }
        }
    }
    assert_eq!(planted.len() as u64, prod(&state, forester).cycles_completed());
    assert!(planted.len() >= 2);
    let rect = state.building(forester).unwrap().rect();
    for p in &planted {
        assert!(matches!(state.world().tile(*p).vegetation, Vegetation::Tree { growth, .. } if growth < 30));
        let dist = rect.distance_to(*p);
        assert!((2..=7).contains(&dist), "{p} at distance {dist}");
    }
    assert!(!state.take_dirty_chunks().is_empty(), "planting marks tiles dirty");
    // Saplings are not fellable: a woodcutter next to them finds nothing.
    let cutter = linked(&mut state, "woodcutter", 12, 20);
    run(&mut state, 2);
    let felled_any = planted.iter().any(|p| !state.world().tile(*p).vegetation.is_tree());
    assert!(!felled_any);
    let cutter_status = prod(&state, cutter).status();
    if !planted.iter().any(|p| state.building(cutter).unwrap().rect().distance_to(*p) <= 8) {
        assert_eq!(cutter_status, ProductionStatus::NoResourcesInRange);
    } else {
        assert_ne!(cutter_status, ProductionStatus::Working);
    }
    assert!(!gathering::is_fellable(Vegetation::Tree { kind: TreeKind::Birch, growth: 10 }, d.rules()));

    // Quarry: each cycle takes one unit from the nearest stone deposit; empty deposits vanish.
    let mut state = flat_state(48);
    let quarry_tile = IVec2::new(30, 34);
    state.world.tile_mut(quarry_tile).deposit = Some(Deposit { kind: DepositKind::Stone, amount: 2 });
    let quarry = linked(&mut state, "quarry", 32, 34);
    let mut depleted = false;
    for _ in 0..(10 * TICKS_PER_HOUR) {
        depleted |= run(&mut state, 1).iter().any(|e| matches!(e, SimEvent::DepositDepleted { kind: DepositKind::Stone, .. }));
    }
    assert!(depleted);
    assert_eq!(state.world().tile(quarry_tile).deposit, None);
    assert_eq!(prod(&state, quarry).cycles_completed(), 2);
    assert_eq!(prod(&state, quarry).status(), ProductionStatus::NoResourcesInRange);

    // Fishery: fish deposits shrink but stay (they regenerate).
    let mut state = flat_state(48);
    for x in 28..36 {
        state.world.tile_mut(IVec2::new(x, 30)).terrain = Terrain::ShallowWater;
    }
    let fish_tile = IVec2::new(31, 30);
    state.world.tile_mut(fish_tile).deposit = Some(Deposit { kind: DepositKind::Fish, amount: 3 });
    let fishery = linked(&mut state, "fishery", 31, 31);
    run(&mut state, 8 * TICKS_PER_HOUR);
    assert!(prod(&state, fishery).cycles_completed() >= 1);
    assert_eq!(state.world().tile(fish_tile).deposit, Some(Deposit { kind: DepositKind::Fish, amount: 0 }));
    assert_eq!(prod(&state, fishery).status(), ProductionStatus::NoResourcesInRange);
}

#[test]
fn a_forester_in_a_dense_forest_waits_for_room_and_says_so_once() {
    let mut state = flat_state(48);
    let forester = linked(&mut state, "forester", 20, 20);
    // Every free tile within reach already carries a grown tree.
    let area = state.building(forester).unwrap().rect().expanded(9).clipped(48, 48);
    for p in area.tiles() {
        if state.buildings.at(p).is_none() && !state.roads.has_road(p) {
            state.world.tile_mut(p).vegetation = Vegetation::Tree { kind: TreeKind::Deciduous, growth: 255 };
        }
    }
    crate::navigation::rebuild(&mut state, defs());
    let mut notes = Vec::new();
    for _ in 0..2 * TICKS_PER_DAY {
        for e in run(&mut state, 1) {
            if let SimEvent::Notification(n) = e {
                notes.push((state.clock().tick / TICKS_PER_DAY, n));
            }
        }
    }
    assert_eq!(prod(&state, forester).status(), ProductionStatus::NoPlantingSpace);
    assert_eq!(ProductionStatus::NoPlantingSpace.display_name(), "Kein freier Platz zum Pflanzen");
    let notices: Vec<&(u64, crate::event::Notification)> = notes.iter().filter(|(_, n)| n.text.starts_with("Försterhaus")).collect();
    assert_eq!(notices.len(), 1, "one notice for the spell, no reminders: {notices:?}");
    assert_eq!(notices[0].1.text, "Försterhaus: Der Wald im Umkreis ist dicht – gepflanzt wird wieder, sobald Platz frei wird.");
    assert_eq!(notices[0].1.severity, crate::event::Severity::Info, "a dense forest is no problem");
    // A new spell (a tree was felled, a sapling planted, the forest closed again) is told once
    // more – but on one day only once.
    let mut again = Vec::new();
    for _ in 0..2 {
        let now = state.clock.tick;
        state.buildings.get_mut(forester).and_then(|b| b.production.as_mut()).unwrap().problem_since = Some(now);
        for _ in 0..6 * TICKS_PER_HOUR {
            for e in run(&mut state, 1) {
                if let SimEvent::Notification(n) = e
                    && n.text.starts_with("Försterhaus")
                {
                    again.push(state.clock().tick / TICKS_PER_DAY);
                }
            }
        }
    }
    assert_eq!(again, vec![2], "two spells on day 2, one notice");
    assert!(crate::workforce::is_idle_status(ProductionStatus::NoPlantingSpace), "staffed last");
}

#[test]
fn queues_recipes_and_priorities() {
    let mut state = flat_state(48);
    let (loam, clay) = (res("loam"), res("clay"));
    state.world.tile_mut(IVec2::new(30, 33)).deposit = Some(Deposit { kind: DepositKind::Clay, amount: 900 });
    let pit = linked(&mut state, "clay_pit", 31, 34);
    assert_eq!(prod(&state, pit).active_recipe(), Some(0));
    // Queue 2 clay (recipe 1) before the continuous loam order.
    let events = exec(&mut state, Command::QueueProduction { building: pit, recipe: 1, units: Some(2) });
    assert!(events.iter().any(|e| matches!(e, SimEvent::ProductionQueueChanged { .. })));
    assert_eq!(
        prod(&state, pit).queue(),
        &[QueueEntry { recipe: 1, remaining: Some(2) }, QueueEntry { recipe: 0, remaining: None }]
    );
    // The first cycle (started in the command tick) still makes loam; then 2 clay, then loam.
    run(&mut state, 8 * TICKS_PER_HOUR);
    let made_clay = crate::report::production_report(&state, defs(), pit).unwrap();
    assert_eq!(made_clay.queue, vec![QueueEntry { recipe: 0, remaining: None }]);
    assert_eq!(produced_total(&state, clay), 2);
    assert!(produced_total(&state, loam) >= 2);
    // SetRecipe replaces the queue; ClearQueue idles the building after its cycle.
    exec(&mut state, Command::SetRecipe { building: pit, recipe: 1 });
    assert_eq!(prod(&state, pit).queue(), &[QueueEntry { recipe: 1, remaining: None }]);
    exec(&mut state, Command::ClearQueue { building: pit });
    run(&mut state, 3 * TICKS_PER_HOUR);
    assert_eq!(prod(&state, pit).status(), ProductionStatus::Idle);
    // Invalid orders are rejected with German reasons.
    let events = exec(&mut state, Command::SetRecipe { building: pit, recipe: 7 });
    assert_eq!(rejection(&events), Some(&RejectReason::UnknownRecipe { recipe: 7 }));
    let events = exec(&mut state, Command::QueueProduction { building: pit, recipe: 0, units: Some(0) });
    assert_eq!(rejection(&events), Some(&RejectReason::InvalidQuantity));
    for _ in 0..MAX_QUEUE_LEN {
        exec(&mut state, Command::QueueProduction { building: pit, recipe: 0, units: Some(1) });
    }
    let events = exec(&mut state, Command::QueueProduction { building: pit, recipe: 0, units: Some(1) });
    assert!(matches!(rejection(&events), Some(RejectReason::QueueFull { .. })));
    let d = depot(&state);
    let events = exec(&mut state, Command::SetRecipe { building: d, recipe: 0 });
    assert_eq!(rejection(&events), Some(&RejectReason::NoProduction));
    assert!(RejectReason::NoProduction.to_string().ends_with('.'));

    // Building priority decides who gets scarce workers.
    let mut state = flat_state(48);
    crate::test_support::set_adults(&mut state, 4);
    let a = spawn(&mut state, "chicken_farm", 20, 20);
    let b = spawn(&mut state, "chicken_farm", 24, 20);
    run(&mut state, 1);
    // The depot's base carrier first, then food before services: both farms, then a second
    // depot carrier.
    assert_eq!((state.building(a).unwrap().staffed(), state.building(b).unwrap().staffed()), (1, 1));
    assert_eq!(state.building(depot(&state)).unwrap().staffed(), 2);
    crate::test_support::set_adults(&mut state, 2);
    run(&mut state, 1);
    assert_eq!((state.building(a).unwrap().staffed(), state.building(b).unwrap().staffed()), (1, 0));
    let events = exec(&mut state, Command::SetProductionPriority { building: b, priority: JobPriority::High });
    assert!(events.iter().any(|e| matches!(e, SimEvent::BuildingPriorityChanged { .. })));
    assert_eq!((state.building(a).unwrap().staffed(), state.building(b).unwrap().staffed()), (0, 1));
    assert_eq!(state.building(b).unwrap().priority(), JobPriority::High);
}

#[test]
fn pasture_bees_and_bankruptcy_modify_productivity() {
    let d = defs();
    let mut state = flat_state(48);
    // Sheep on grass: full pasture coverage.
    let sheep = linked(&mut state, "sheep_farm", 30, 30);
    work(&mut state);
    // Half the herd at first (it grows daily), then the whole pasture is grazed.
    assert_eq!(prod(&state, sheep).cycle().map(|c| c.environment_permille), Some(500));
    crate::test_support::run_until(&mut state, 3 * crate::time::TICKS_PER_DAY, |s| s.agriculture().farm(sheep).is_some_and(|f| f.herd() == 8));
    assert_eq!(super::pasture_permille(&state, d, sheep, 16), 1000);
    // Rock all around: no pasture.
    let mut rocky = flat_state(48);
    for p in crate::grid::TileRect::new(IVec2::new(20, 20), IVec2::splat(11)).tiles() {
        rocky.world.tile_mut(p).terrain = Terrain::Rock;
    }
    let sheep = spawn(&mut rocky, "sheep_farm", 24, 24);
    run(&mut rocky, 1);
    assert_eq!(prod(&rocky, sheep).status(), ProductionStatus::NoResourcesInRange);
    // Bees love meadows.
    let mut state = flat_state(48);
    for p in crate::grid::TileRect::new(IVec2::new(14, 14), IVec2::splat(14)).tiles() {
        state.world.tile_mut(p).terrain = Terrain::Meadow;
    }
    let apiary = spawn(&mut state, "apiary", 20, 20);
    work(&mut state);
    assert_eq!(prod(&state, apiary).cycle().map(|c| c.environment_permille), Some(1600));
    // Bankruptcy halves productivity.
    let farm = spawn(&mut state, "chicken_farm", 30, 20);
    work(&mut state);
    assert_eq!(prod(&state, farm).productivity_permille(), 1000);
    state.economy.set_bankrupt(true);
    run(&mut state, 1);
    assert_eq!(prod(&state, farm).productivity_permille(), u32::from(d.rules().bankruptcy_productivity_percent) * 10);
    assert_eq!(season_weather_permille(&state, d, d.building_by_key("chicken_farm").unwrap()), 1000);
}

#[test]
fn shared_deposits_never_yield_more_than_they_hold() {
    let stone = res("stone");
    // Two quarries next to a single stone unit: only one cycle can claim it.
    let mut state = flat_state(48);
    let tile = IVec2::new(20, 20);
    state.world.tile_mut(tile).deposit = Some(Deposit { kind: DepositKind::Stone, amount: 1 });
    let a = linked(&mut state, "quarry", 21, 18);
    let b = linked(&mut state, "quarry", 17, 18);
    let stone0 = total(&state, stone);
    run(&mut state, 10 * TICKS_PER_HOUR);
    assert_eq!(state.world().tile(tile).deposit, None);
    assert_eq!(produced_total(&state, stone), 1, "one unit in the ground, one unit out");
    assert_eq!(total(&state, stone), stone0 + 1);
    assert_eq!(prod(&state, a).cycles_completed() + prod(&state, b).cycles_completed(), 1);
    assert_eq!(prod(&state, b).status(), ProductionStatus::NoResourcesInRange);

    // Shared fish grounds: one unit left, two fisheries wanting two fish each.
    let mut state = flat_state(48);
    for x in 24..36 {
        state.world.tile_mut(IVec2::new(x, 30)).terrain = Terrain::ShallowWater;
    }
    let fish_tile = IVec2::new(30, 30);
    state.world.tile_mut(fish_tile).deposit = Some(Deposit { kind: DepositKind::Fish, amount: 1 });
    linked(&mut state, "fishery", 27, 31);
    linked(&mut state, "fishery", 32, 31);
    run(&mut state, 8 * TICKS_PER_HOUR);
    assert_eq!(produced_total(&state, res("fish")), 1);
    assert_eq!(state.world().tile(fish_tile).deposit, Some(Deposit { kind: DepositKind::Fish, amount: 0 }));
}

#[test]
fn a_vanished_deposit_yields_nothing() {
    let stone = res("stone");
    let mut state = flat_state(48);
    let tile = IVec2::new(20, 20);
    state.world.tile_mut(tile).deposit = Some(Deposit { kind: DepositKind::Stone, amount: 5 });
    let quarry = linked(&mut state, "quarry", 21, 18);
    run_until(&mut state, TICKS_PER_DAY, |s| prod(s, quarry).cycle().is_some());
    assert_eq!(prod(&state, quarry).cycle().and_then(|c| c.target), Some(tile));
    // The deposit disappears while the cycle runs and there is no other one in range.
    state.world.tile_mut(tile).deposit = None;
    run_until(&mut state, TICKS_PER_DAY, |s| prod(s, quarry).cycles_completed() == 1);
    assert_eq!(prod(&state, quarry).cycles_completed(), 1);
    assert_eq!(produced_total(&state, stone), 0, "no output from nothing");
    assert_eq!(prod(&state, quarry).output().get(stone), 0);
}

/// Workers of `id` present at work right now.
fn at_work(state: &GameState, id: BuildingId) -> Vec<crate::population::CitizenId> {
    state
        .population()
        .workers_of(id)
        .iter()
        .copied()
        .filter(|c| crate::population::is_at_work(state, defs(), state.population().citizen(*c).unwrap()))
        .collect()
}

#[test]
fn woodcutters_walk_to_the_tree_and_carry_the_log_back() {
    let d = defs();
    let wood = res("wood");
    let mut state = flat_state(48);
    let cutter = linked(&mut state, "woodcutter", 30, 30);
    let tree = IVec2::new(30, 24);
    state.world.tile_mut(tree).vegetation = Vegetation::Tree { kind: TreeKind::Conifer, growth: 230 };
    run_until(&mut state, TICKS_PER_DAY, |s| prod(s, cutter).cycle().is_some());
    let cycle = *prod(&state, cutter).cycle().unwrap();
    assert_eq!((cycle.target, cycle.spot), (Some(tree), Some(tree)), "they work standing at the tree");
    assert_eq!(crate::population::work_area(&state, d, cutter), crate::population::WorkArea::Spot(tree));
    // They walk there; only workers at the tree count as present.
    run_until(&mut state, TICKS_PER_DAY, |s| !at_work(s, cutter).is_empty());
    for c in at_work(&state, cutter) {
        let m = state.population().citizen(c).unwrap().motion();
        assert!(m.is_visible() && m.pos().tile() == tree);
    }
    // The tree falls: the log is on a worker, not yet in the hut.
    let mut felled = false;
    for _ in 0..TICKS_PER_DAY {
        if run(&mut state, 1).iter().any(|e| matches!(e, SimEvent::TreeFelled { position } if *position == tree)) {
            felled = true;
            break;
        }
    }
    assert!(felled);
    assert_eq!(prod(&state, cutter).output().get(wood), 0);
    let hauler = state
        .population()
        .workers_of(cutter)
        .iter()
        .copied()
        .find(|c| state.population().citizen(*c).unwrap().carried().is_some())
        .expect("a woodcutter carries the log");
    let view = crate::population::citizen_view(&state, d, hauler).unwrap();
    assert_eq!(view.carried, Some((wood, 1)));
    assert_ne!(view.role, crate::population::CitizenRole::Carrier, "a woodcutter, not a storage carrier");
    let job = state.logistics().jobs().iter().find(|j| j.kind == crate::logistics::TransportKind::Haul).copied().unwrap();
    assert_eq!((job.source, job.target, job.carrier()), (cutter, cutter, Some(hauler)));
    // Back at the hut the log arrives (one log is below the carriers' collection threshold).
    run_until(&mut state, TICKS_PER_DAY, |s| s.logistics().job(job.id).is_none());
    assert_eq!(prod(&state, cutter).output().get(wood), 1);
    assert!(state.population().citizen(hauler).unwrap().carried().is_none());
    state.validate_entities(d).expect("consistent");
}

#[test]
fn foresters_plant_where_they_stand_and_fishers_work_from_the_shore() {
    let d = defs();
    let mut state = flat_state(48);
    let forester = linked(&mut state, "forester", 30, 30);
    let mut planted = None;
    for _ in 0..2 * TICKS_PER_DAY {
        let events = run(&mut state, 1);
        if let Some(p) = events.iter().find_map(|e| match e {
            SimEvent::TreePlanted { position } => Some(*position),
            _ => None,
        }) {
            planted = Some(p);
            break;
        }
    }
    let p = planted.expect("a sapling was planted");
    let worker = state.population().workers_of(forester)[0];
    assert_eq!(state.population().citizen(worker).unwrap().motion().pos().tile(), p, "planted where the forester stands");
    // A fishery at a lake: the fishers stand on the shore next to the fish.
    let mut state = flat_state(48);
    for y in 20..26 {
        for x in 28..36 {
            state.world.tile_mut(IVec2::new(x, y)).terrain = Terrain::ShallowWater;
        }
    }
    let fish = IVec2::new(31, 25);
    state.world.tile_mut(fish).deposit = Some(Deposit { kind: DepositKind::Fish, amount: 50 });
    crate::navigation::rebuild(&mut state, d);
    let fishery = linked(&mut state, "fishery", 30, 28);
    run_until(&mut state, TICKS_PER_DAY, |s| prod(s, fishery).cycle().is_some());
    let cycle = *prod(&state, fishery).cycle().unwrap();
    assert_eq!(cycle.target, Some(fish));
    let spot = cycle.spot.expect("fishers walk to the shore");
    assert_eq!((spot - fish).abs().element_sum(), 1, "next to the fish");
    assert!(state.navigation().is_walkable(spot));
    run_until(&mut state, TICKS_PER_DAY, |s| !at_work(s, fishery).is_empty());
}
