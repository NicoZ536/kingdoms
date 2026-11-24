//! Tests of citizens, households, needs, supply, demography and house tiers.

use glam::IVec2;

use super::*;
use crate::command::Command;
use crate::defs::{NeedKind, ServiceKind};
use crate::state::{GameSettings, GameState};
use crate::test_support::{defs, defs_edited, depot, empty_storages, exec, flat_state, run, spawn};
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
use crate::world::WorldSize;

fn need(state: &GameState, id: CitizenId, need: NeedKind) -> u8 {
    state.population.citizen(id).unwrap().needs().get(need)
}

fn residents(state: &GameState, house: BuildingId) -> Vec<CitizenId> {
    state.population.residents_of(house).collect()
}

/// Runs `ticks` with custom data.
fn run_with(state: &mut GameState, defs: &Defs, ticks: u64) -> Vec<SimEvent> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(state.tick(defs, Vec::new()));
    }
    events
}

/// Replaces the flat state's singles by one family (couple + `children`) and returns it.
fn one_family(state: &mut GameState, children: u16) -> HouseholdId {
    crate::test_support::set_adults(state, 0);
    demography::make_family(state, defs(), 2, children)
}

#[test]
fn settlers_form_families_and_fill_the_start_houses() {
    let d = defs();
    for seed in [1, 7, 42] {
        let state = GameState::new(d, GameSettings::new(d, seed, WorldSize::Small));
        state.validate(d).unwrap();
        let pop = &state.population;
        assert_eq!(pop.len(), 12, "seed {seed}");
        assert_eq!(pop.households().count(), 4);
        let housed: u16 = pop.homes().map(|(_, h)| pop.home_residents(h)).sum();
        assert_eq!(housed, 10, "two houses of five are full (seed {seed})");
        for (_, h) in pop.households() {
            assert!(!h.family_name().is_empty());
            let adults: Vec<_> = h.members().iter().filter(|m| pop.citizen(**m).unwrap().stage() != LifeStage::Child).collect();
            assert_eq!(adults.len(), 2, "every family is a couple");
            let (a, b) = (*adults[0], *adults[1]);
            assert_eq!(pop.citizen(a).unwrap().partner(), Some(b));
            for m in h.members() {
                let c = pop.citizen(*m).unwrap();
                assert!(!c.first_name().is_empty());
                assert_eq!(c.stage(), LifeStage::for_age(c.age_on(0), &d.rules().population));
                if c.stage() == LifeStage::Child {
                    assert!(c.mother().is_some() && c.father().is_some());
                }
            }
        }
        assert_eq!(state.workforce().population(), 12);
        assert_eq!(state.workforce().available(), 8);
    }
}

#[test]
fn households_eat_from_the_storages_and_record_it() {
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 30, 30); // within the depot's carrier radius
    run(&mut state, 1);
    let people = residents(&state, house);
    assert_eq!(people.len(), 5);
    let household = state.population.citizen(people[0]).unwrap().household();
    // The pantry is empty: the first adult walks to the depot at once and restocks there.
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| s.population.household(household).unwrap().supply().food_variety > 0);
    let h = state.population.household(household).unwrap();
    assert!(h.supply().fed);
    assert!(h.supply().food_variety >= 2, "a pantry with several kinds of food");
    let taken: u32 = defs().resources().iter().map(|r| state.economy().stats().today().household.get(r.id)).sum();
    assert!(taken > 0);
    let consumed: u32 = defs().resources().iter().map(|r| state.economy().stats().today().consumed.get(r.id)).sum();
    assert!(consumed >= taken);
    // Fed again: hunger is back at 100 within the hour after the short wait for the errand.
    run(&mut state, 2 * TICKS_PER_HOUR);
    assert_eq!(need(&state, people[0], NeedKind::Hunger), 100);
}

#[test]
fn thirst_follows_the_water_supply() {
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 30, 30);
    run(&mut state, 10 * TICKS_PER_HOUR);
    let c = residents(&state, house)[0];
    assert_eq!(need(&state, c, NeedKind::Thirst), defs().rules().population.no_water_thirst);
    spawn(&mut state, "well", 33, 33);
    run(&mut state, 10 * TICKS_PER_HOUR);
    assert_eq!(need(&state, c, NeedKind::Thirst), 100);
    assert!(state.population.household(state.population.citizen(c).unwrap().household()).unwrap().supply().has_service(ServiceKind::Water));
}

#[test]
fn citizens_starve_without_food() {
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 30, 30);
    spawn(&mut state, "well", 33, 33);
    empty_storages(&mut state);
    let mut events = Vec::new();
    for _ in 0..4 {
        events.extend(run(&mut state, TICKS_PER_DAY));
    }
    let starved = events.iter().filter(|e| matches!(e, SimEvent::CitizenDied { cause: DeathCause::Starvation, .. })).count();
    assert!(starved >= 5, "the residents starved ({starved})");
    assert!(residents(&state, house).is_empty());
    let deaths: u32 = state.population.stats().history().map(|d| d.counts.deaths_by(DeathCause::Starvation)).sum::<u32>()
        + state.population.stats().today().deaths_by(DeathCause::Starvation);
    assert_eq!(deaths as usize, starved);
    assert!(events.iter().filter_map(SimEvent::notification).any(|n| n.text.contains("Nahrungsmangel")));
    assert!(events.iter().filter_map(SimEvent::notification).any(|n| n.text.ends_with("ist verhungert!")));
    state.validate_entities(defs()).unwrap();
}

#[test]
fn winter_needs_fuel() {
    let d = defs();
    let mut state = flat_state(48);
    state.clock = crate::time::GameClock::from_date(1, crate::time::Season::Winter, 1, 6, 0).unwrap();
    let house = spawn(&mut state, "house_wood", 30, 30);
    spawn(&mut state, "well", 33, 33);
    run(&mut state, 1);
    let c = residents(&state, house)[0];
    let hh = state.population.citizen(c).unwrap().household();
    let wood = d.resource_id("wood").unwrap();
    let coal = d.resource_id("coal").unwrap();
    // The first errand brings food and fuel; warmth recovers within hours.
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| {
        let h = s.population.household(hh).unwrap();
        h.supply().fuel && h.pantry_milli(wood) + h.pantry_milli(coal) > 0 && need(s, c, NeedKind::Warmth) == 100
    });
    let h = state.population.household(hh).unwrap();
    assert!(h.supply().fuel);
    assert!(h.pantry_milli(wood) + h.pantry_milli(coal) > 0, "fuel taken from the depot");
    assert_eq!(need(&state, c, NeedKind::Warmth), 100);
    // Without fuel (stock and pantries empty) the household freezes and is warned.
    for r in [wood, coal] {
        let depot = depot(&state);
        let have = state.building(depot).unwrap().inventory().get(r);
        state.buildings.get_mut(depot).unwrap().inventory.remove(r, have);
    }
    crate::economy::storage::refresh_stock(&mut state, d);
    for (_, h) in state.population.households.iter_mut() {
        h.pantry.retain(|p| p.resource != wood && p.resource != coal);
    }
    let events = run(&mut state, 12 * TICKS_PER_HOUR);
    assert!(!state.population.household(hh).unwrap().supply().fuel);
    assert!(need(&state, c, NeedKind::Warmth) < d.rules().population.freezing_warmth);
    assert!(events.iter().filter_map(SimEvent::notification).any(|n| n.text.starts_with("Brennstoffmangel")));
}

/// Fills a tavern's beer stock (the tests do not connect it to the depot).
fn stock_tavern(state: &mut GameState, tavern: BuildingId) {
    let beer = defs().resource_id("beer").unwrap();
    let b = state.buildings.get_mut(tavern).unwrap();
    let cap = b.inventory.capacity(beer);
    b.inventory.add(beer, cap);
}

#[test]
fn markets_bring_the_goods_of_the_tier() {
    let d = defs();
    let mut state = flat_state(64);
    let clothes = d.resource_id("clothes").unwrap();
    let furniture = d.resource_id("furniture").unwrap();
    let house = spawn(&mut state, "citizen_house", 44, 44);
    run(&mut state, TICKS_PER_HOUR + 1);
    let hh = state.population.citizen(residents(&state, house)[0]).unwrap().household();
    // Goods are bought: the household can afford them.
    state.population.households.get_mut(hh).unwrap().purse = crate::population::Purse::with_savings(5_000_000);
    assert_eq!(state.population.household(hh).unwrap().pantry_milli(clothes), 0, "no market: only food");
    assert_eq!(state.population.household(hh).unwrap().supply().missing_goods, 2);
    spawn(&mut state, "market", 50, 38);
    // The goods come with the next errand to the market (18:00).
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| s.population.household(hh).unwrap().pantry_milli(clothes) > 0);
    run(&mut state, TICKS_PER_HOUR);
    let h = state.population.household(hh).unwrap();
    assert!(h.pantry_milli(clothes) > 0, "the citizen house's goods");
    assert!(h.pantry_milli(furniture) > 0, "and the next tier's goods (upgrades are not locked)");
    assert_eq!(h.supply().missing_goods, 0);
    assert!(state.economy().stats().today().household.get(clothes) > 0);
}

#[test]
fn couples_have_children_when_there_is_room() {
    let d = defs_edited("rules.ron", |t| t.replace("birth_chance_percent: 5.0", "birth_chance_percent: 100.0"));
    let mut state = flat_state(48);
    let family = one_family(&mut state, 0);
    // Make the mother fertile and the household happy.
    let members = state.population.household(family).unwrap().members().to_vec();
    for m in &members {
        let c = state.population.citizens.get_mut(*m).unwrap();
        c.birth_day = -25 * crate::time::DAYS_PER_SEASON as i64;
        c.happiness = MAX_VALUE;
    }
    spawn(&mut state, "house_wood", 30, 30);
    spawn(&mut state, "well", 33, 33);
    let events = run_with(&mut state, &d, TICKS_PER_DAY + TICKS_PER_HOUR);
    let born: Vec<_> = events.iter().filter(|e| matches!(e, SimEvent::CitizenBorn { .. })).collect();
    assert_eq!(born.len(), 1, "one birth per mother and interval");
    let child = match born[0] {
        SimEvent::CitizenBorn { citizen, household } => {
            assert_eq!(*household, family);
            *citizen
        }
        _ => unreachable!(),
    };
    let c = state.population.citizen(child).unwrap();
    assert_eq!((c.stage(), c.age_on(state.clock().total_days())), (LifeStage::Child, 0));
    // Newborns appear inside their home (and stay there: no school yet).
    assert_eq!(c.motion().inside(), state.population.household(family).unwrap().home());
    assert!(members.contains(&c.mother().unwrap()) && members.contains(&c.father().unwrap()));
    assert_eq!(state.population.children_of(c.mother().unwrap()), vec![child]);
    assert!(state.population.full_name(child).unwrap().ends_with(state.population.household(family).unwrap().family_name()));
    assert_eq!(state.population.stats().today().births + state.population.stats().history().map(|h| h.counts.births).sum::<u32>(), 1);
    state.validate_entities(&d).unwrap();
}

#[test]
fn the_old_die_of_old_age() {
    let mut state = flat_state(48);
    let old = state.population.citizens().next().map(|(id, _)| id).unwrap();
    state.population.citizens.get_mut(old).unwrap().birth_day = -90 * crate::time::DAYS_PER_SEASON as i64;
    let mut events = Vec::new();
    for _ in 0..60 {
        events.extend(run(&mut state, TICKS_PER_DAY));
        if state.population.citizen(old).is_none() {
            break;
        }
    }
    assert!(state.population.citizen(old).is_none());
    assert!(events.iter().any(|e| matches!(e, SimEvent::CitizenDied { citizen, cause: DeathCause::OldAge, age } if *citizen == old && *age >= 90)));
    assert!(state.workforce().population() < 20);
}

#[test]
fn settlers_immigrate_to_a_happy_kingdom() {
    let mut state = flat_state(64);
    let hall = spawn(&mut state, "town_hall", 20, 20);
    let d = depot(&state);
    crate::test_support::link(&mut state, hall, d);
    // Houses within the depot's reach, with water, market, church and tavern.
    for (x, y) in [(40, 40), (43, 40), (46, 40), (40, 44), (43, 44), (46, 44), (40, 48), (43, 48)] {
        spawn(&mut state, "house_wood", x, y);
    }
    spawn(&mut state, "well", 44, 52);
    spawn(&mut state, "market", 50, 40);
    spawn(&mut state, "church", 50, 46);
    let tavern = spawn(&mut state, "tavern", 34, 40);
    stock_tavern(&mut state, tavern);
    let before = state.population.len();
    let mut arrivals = Vec::new();
    for _ in 0..TICKS_PER_DAY {
        let day = state.clock().total_days();
        for e in run(&mut state, 1) {
            if let SimEvent::HouseholdArrived { members, position, .. } = e {
                arrivals.push((day, members, position));
            }
        }
    }
    assert!(!arrivals.is_empty(), "happiness {:?}", state.population.average_happiness());
    let entrance = state.building(hall).unwrap().entrance();
    assert!(arrivals.iter().all(|(_, _, p)| *p == entrance), "immigrants arrive at the headquarters");
    let rules = &defs().rules().population;
    let first_day: u32 = arrivals.iter().filter(|(day, ..)| *day == 0).map(|(_, n, _)| u32::from(*n)).sum();
    assert!(first_day <= u32::from(rules.immigration_daily_base), "daily cap");
    assert!(state.population.len() > before);
    assert!(state.population.households().all(|(_, h)| h.home().is_some()), "immigrants moved into houses");
    state.validate_entities(defs()).unwrap();
}

#[test]
fn unhappy_households_emigrate() {
    let mut state = flat_state(48);
    // Homeless, no water and the highest tax rate.
    exec(&mut state, Command::SetTaxRate { percent: defs().rules().max_tax_rate_percent });
    let mut left = 0;
    let mut events = Vec::new();
    for _ in 0..4 {
        events.extend(run(&mut state, TICKS_PER_DAY));
    }
    for e in &events {
        if let SimEvent::HouseholdEmigrated { members, .. } = e {
            left += u32::from(*members);
        }
    }
    assert!(left > 0, "unhappy households left");
    assert_eq!(state.population.len() as u32, 20 - left);
    let emigrants: u32 = state.population.stats().history().map(|d| d.counts.emigrants).sum::<u32>() + state.population.stats().today().emigrants;
    assert_eq!(emigrants, left);
    assert!(events.iter().filter_map(SimEvent::notification).any(|n| n.text.contains("unzufrieden verlassen")));
}

#[test]
fn singles_marry_and_move_together() {
    let d = defs_edited("rules.ron", |t| t.replace("marriage_chance_percent: 20", "marriage_chance_percent: 100"));
    let mut state = flat_state(48);
    crate::test_support::set_adults(&mut state, 0);
    let man = demography::make_family(&mut state, &d, 1, 0);
    let woman = demography::make_family(&mut state, &d, 1, 0);
    for (hh, sex) in [(man, Sex::Male), (woman, Sex::Female)] {
        let id = state.population.household(hh).unwrap().members()[0];
        let c = state.population.citizens.get_mut(id).unwrap();
        c.sex = sex;
        c.birth_day = -25 * crate::time::DAYS_PER_SEASON as i64;
    }
    spawn(&mut state, "house_wood", 30, 30);
    let events = run_with(&mut state, &d, TICKS_PER_DAY + 1);
    assert!(events.iter().any(|e| matches!(e, SimEvent::CitizensMarried { .. })));
    assert_eq!(state.population.households().count(), 1, "one household moved in with the other");
    let (_, h) = state.population.households().next().unwrap();
    let a = h.members()[0];
    assert_eq!(state.population.citizen(state.population.citizen(a).unwrap().partner().unwrap()).unwrap().partner(), Some(a));
    state.validate_entities(&d).unwrap();
}

#[test]
fn children_come_of_age_get_educated_and_take_jobs() {
    let d = defs();
    let mut state = flat_state(48);
    let family = one_family(&mut state, 1);
    // A third adult becomes the depot's base carrier.
    crate::test_support::set_adults(&mut state, 3);
    // Educated parents teach at the school.
    for m in state.population.household(family).unwrap().members().to_vec() {
        let c = state.population.citizens.get_mut(m).unwrap();
        c.educated = c.stage == LifeStage::Adult;
    }
    spawn(&mut state, "house_wood", 30, 30);
    let school = spawn(&mut state, "school", 34, 30);
    spawn(&mut state, "woodcutter", 20, 20);
    exec(&mut state, Command::SetProductionPriority { building: school, priority: crate::policy::JobPriority::High });
    assert_eq!(state.building(school).unwrap().staffed(), 2, "the parents teach");
    let child = state
        .population
        .household(family)
        .unwrap()
        .members()
        .iter()
        .copied()
        .find(|m| state.population.citizen(*m).unwrap().stage() == LifeStage::Child)
        .unwrap();
    // 13 years and 5 days old: an adult tomorrow, with most of the schooling done.
    let rules = &d.rules().population;
    {
        let c = state.population.citizens.get_mut(child).unwrap();
        c.birth_day = -(i64::from(rules.adult_age) * crate::time::DAYS_PER_SEASON as i64) + 1;
        c.education = rules.educated_threshold - rules.education_per_day;
    }
    assert!(state.population.citizen(child).unwrap().job().is_none());
    run(&mut state, 2 * TICKS_PER_DAY);
    let c = state.population.citizen(child).unwrap();
    assert_eq!(c.stage(), LifeStage::Adult);
    assert!(c.is_educated(), "schooling completed before adulthood");
    assert!(c.job().is_some(), "the new adult found work");
    state.validate_entities(d).unwrap();
}

#[test]
fn houses_downgrade_after_unmet_requirements() {
    let d = defs();
    let mut state = flat_state(64);
    // A farmhouse needs a market; without one it falls back after 48 hours.
    let farmhouse = spawn(&mut state, "farmhouse", 20, 20);
    spawn(&mut state, "well", 23, 23);
    let mut events = Vec::new();
    for _ in 0..(50 * TICKS_PER_HOUR) {
        events.extend(run(&mut state, 1));
    }
    let to = d.building_id("house_wood").unwrap();
    assert!(events.iter().any(|e| matches!(e, SimEvent::HouseDowngraded { building, to: t, .. } if *building == farmhouse && *t == to)));
    assert_eq!(state.building(farmhouse).unwrap().kind(), to);
    assert!(state.population.resident_count(farmhouse) <= 5, "capacity of the lower tier");
    // A residence (3x3) shrinks to a townhouse (2x2) around its entrance.
    let residence = spawn(&mut state, "residence", 40, 40);
    let entrance = state.building(residence).unwrap().entrance();
    run(&mut state, 1);
    assert!(state.population.resident_count(residence) > 0);
    state.population.homes.get_mut(&residence).unwrap().unmet_hours = d.rules().population.downgrade_after_hours - 1;
    run(&mut state, TICKS_PER_HOUR);
    let b = state.building(residence).unwrap();
    assert_eq!(b.kind(), d.building_id("townhouse").unwrap());
    assert_eq!(b.size(), IVec2::new(2, 2));
    assert_eq!(b.entrance(), entrance);
    state.validate_entities(d).unwrap();
}

#[test]
fn demolished_houses_send_households_to_other_houses() {
    let mut state = flat_state(48);
    let a = spawn(&mut state, "house_wood", 10, 10);
    let b = spawn(&mut state, "house_wood", 20, 10);
    run(&mut state, 1);
    assert_eq!((state.population.resident_count(a), state.population.resident_count(b)), (5, 5));
    let events = exec(&mut state, Command::DemolishBuilding { building: a });
    assert!(events.iter().any(|e| matches!(e, SimEvent::HouseholdMoved { home: None, .. })));
    run(&mut state, 1);
    assert!(state.population.home(a).is_none());
    assert_eq!(state.population.resident_count(b), 5);
    let homeless: u16 = state.population.households().filter(|(_, h)| h.home().is_none()).map(|(_, h)| h.size()).sum();
    assert_eq!(homeless, 15);
    state.validate_entities(defs()).unwrap();
}

#[test]
fn demolished_workplaces_release_their_workers() {
    let mut state = flat_state(48);
    let cutter = spawn(&mut state, "woodcutter", 10, 10);
    run(&mut state, 1);
    let workers = state.population.workers_of(cutter).to_vec();
    assert_eq!(workers.len(), 2);
    exec(&mut state, Command::DemolishBuilding { building: cutter });
    assert!(state.population.workers_of(cutter).is_empty());
    assert!(workers.iter().all(|w| state.population.citizen(*w).unwrap().job().is_none()));
    state.validate_entities(defs()).unwrap();
}

#[test]
fn unhealthy_or_unhappy_workers_produce_less() {
    let rules = &defs().rules().population;
    let mut state = flat_state(32);
    let id = state.population.citizens().next().map(|(id, _)| id).unwrap();
    let c = state.population.citizens.get_mut(id).unwrap();
    assert_eq!(work_efficiency_permille(rules, c), 1000);
    c.health = 0;
    assert_eq!(work_efficiency_permille(rules, c), 500);
    c.happiness = 0;
    assert_eq!(work_efficiency_permille(rules, c), 250);
}

#[test]
fn skilled_workers_need_training_and_gain_it_by_working() {
    let d = defs();
    let rules = &d.rules().population;
    let mut state = flat_state(48);
    let id = state.population.citizens().next().map(|(id, _)| id).unwrap();
    let skilled = crate::defs::JobTier::Skilled;
    let c = state.population.citizens.get_mut(id).unwrap();
    c.experience = [0; JobCategory::ALL.len()];
    c.educated = false;
    assert_eq!(training_permille(rules, c, JobCategory::Processing, crate::defs::JobTier::Unskilled), 1000);
    assert_eq!(training_permille(rules, c, JobCategory::Processing, skilled), 800);
    c.educated = true;
    assert_eq!(training_permille(rules, c, JobCategory::Processing, skilled), 900, "the educated start higher");
    c.experience[JobCategory::Processing.index()] = 2;
    assert_eq!(training_permille(rules, c, JobCategory::Processing, skilled), 966);
    assert_eq!(training_permille(rules, c, JobCategory::Food, skilled), 900, "experience counts per category");
    c.experience[JobCategory::Processing.index()] = 3;
    assert_eq!(training_permille(rules, c, JobCategory::Processing, skilled), 1000);
    // A new sawmill worker: below full staffing at first, fully trained after three days.
    let mill = spawn(&mut state, "sawmill", 20, 20);
    let dep = depot(&state);
    crate::test_support::link(&mut state, mill, dep);
    let def = d.building_by_key("sawmill").unwrap();
    run(&mut state, 2);
    let workers = state.population.workers_of(mill).to_vec();
    assert!(!workers.is_empty());
    for w in &workers {
        let c = state.population.citizens.get_mut(*w).unwrap();
        c.experience = [0; JobCategory::ALL.len()];
        c.educated = false;
    }
    assert_eq!(crate::production::crew_training_permille(&state, d, def, mill), 800);
    let report = crate::report::production_report(&state, d, mill).unwrap();
    assert_eq!(report.training_percent, 80);
    let detail = crate::report::citizen_detail(&state, d, workers[0]).unwrap();
    assert_eq!(detail.training.as_ref().map(|t| t.text.as_str()), Some("In Einarbeitung: 0 von 3 Tagen, 80 %"));
    run(&mut state, 3 * TICKS_PER_DAY + TICKS_PER_HOUR);
    assert!(state.population.citizen(workers[0]).unwrap().experience_days(JobCategory::Processing) >= 3);
    assert_eq!(crate::production::crew_training_permille(&state, d, def, mill), 1000);
    let detail = crate::report::citizen_detail(&state, d, workers[0]).unwrap();
    assert_eq!(detail.training.map(|t| t.text), Some("Eingearbeitet".to_owned()));
}

#[test]
fn workers_sick_for_a_day_go_on_sick_leave_and_free_their_place() {
    let d = defs();
    let mut state = flat_state(48);
    crate::test_support::set_adults(&mut state, 8);
    let cutter = spawn(&mut state, "sawmill", 20, 20);
    let dep = depot(&state);
    crate::test_support::link(&mut state, cutter, dep);
    run(&mut state, 26 * TICKS_PER_HOUR);
    let workers = state.population.workers_of(cutter).to_vec();
    assert_eq!(workers.len(), 2);
    let sick = workers[0];
    let spare = state.population.citizens().find(|(id, c)| !workers.contains(id) && c.job().is_none()).map(|(id, _)| id).unwrap();
    // Ill for a day already: at the next update the job goes to the healthy spare worker.
    let now = state.clock.tick;
    let c = state.population.citizens.get_mut(sick).unwrap();
    c.health = 20 * CENTI;
    c.sick_since = Some(now.saturating_sub(25 * TICKS_PER_HOUR));
    run(&mut state, 2 * crate::population::CITIZEN_UPDATE_TICKS);
    let c = state.population.citizen(sick).unwrap();
    assert!(c.is_on_sick_leave() && c.job().is_none() && !c.can_take_job());
    run(&mut state, 2);
    assert!(state.population.workers_of(cutter).contains(&spare), "the spare worker took the place");
    let detail = crate::report::citizen_detail(&state, d, sick).unwrap();
    assert!(detail.sick_leave && detail.sick);
    state.validate_entities(d).unwrap();
    // Healthy again: back in the pool of job seekers.
    state.population.citizens.get_mut(sick).unwrap().health = MAX_VALUE;
    run(&mut state, 2 * crate::population::CITIZEN_UPDATE_TICKS);
    let c = state.population.citizen(sick).unwrap();
    assert!(!c.is_on_sick_leave() && c.sick_since().is_none() && c.can_take_job());
    state.validate_entities(d).unwrap();
}

#[test]
fn happiness_parts_explain_the_target() {
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 30, 30);
    spawn(&mut state, "well", 33, 33);
    run(&mut state, 30 * TICKS_PER_HOUR);
    let c = residents(&state, house)[0];
    let parts = citizen_happiness_parts(&state, defs(), c).unwrap();
    let target: f32 = parts.iter().map(|p| p.points).sum();
    // Happiness converged to the target.
    let happiness = f32::from(state.population.citizen(c).unwrap().happiness());
    assert!((happiness - target.clamp(0.0, 100.0)).abs() <= 1.0, "{happiness} vs {target}");
    assert!(parts.iter().any(|p| p.factor == HappinessFactor::Need(NeedKind::Faith)));
    assert!(parts.iter().all(|p| !p.factor.label().is_empty()));
    // Taxes above the neutral rate cost happiness.
    exec(&mut state, Command::SetTaxRate { percent: 20 });
    let parts = citizen_happiness_parts(&state, defs(), c).unwrap();
    let tax = parts.iter().find(|p| p.factor == HappinessFactor::Taxes).unwrap();
    assert!(tax.points < 0.0);
}

#[test]
fn reports_describe_population_houses_and_citizens() {
    let d = defs();
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 30, 30);
    spawn(&mut state, "well", 33, 33);
    spawn(&mut state, "woodcutter", 20, 20);
    run(&mut state, TICKS_PER_DAY + 1);
    let s = crate::report::population_summary(&state, d);
    assert_eq!(s.population, 20);
    assert_eq!(s.stages.iter().map(|r| r.total()).sum::<u32>(), 20);
    assert_eq!((s.housed, s.homeless, s.housing_capacity), (5, 15, 5));
    assert_eq!(s.employed + s.unemployed, s.working_age);
    assert_eq!(s.history.len(), 1, "one closed day");
    let needs = crate::report::needs_summary(&state, d);
    assert_eq!(needs.needs.len(), NeedKind::ALL.len());
    assert_eq!(needs.needs[0].label, "Nahrung");
    let happy = crate::report::happiness_report(&state, d);
    assert!(happy.factors.iter().any(|f| f.label == "Glaube"));
    let report = crate::report::house_report(&state, d, house).unwrap();
    assert_eq!((report.tier, report.capacity, report.residents.len()), (1, 5, 5));
    assert!(report.keep.is_none(), "tier 1 has no requirements");
    let next = report.next.unwrap();
    assert!(next.services.iter().any(|s| s.what == ServiceKind::Market && !s.met));
    assert!(!next.all_met());
    let worker = state.population.workers_of(state.buildings().iter().find(|(_, b)| d.building(b.kind()).key == "woodcutter").unwrap().0)[0];
    let detail = crate::report::citizen_detail(&state, d, worker).unwrap();
    assert_eq!(detail.job.as_ref().unwrap().title, "Holzfällerhütte");
    assert_eq!(detail.stage_label, "Erwachsen");
    assert_eq!(detail.sex_label, "männlich");
    assert_eq!(detail.needs.len(), NeedKind::ALL.len());
    assert!(detail.work_efficiency_percent > 0);
    let depot_worker = state.population.workers_of(depot(&state))[0];
    assert_eq!(crate::report::citizen_detail(&state, d, depot_worker).unwrap().job.unwrap().title, "Träger");
    assert!(crate::report::house_report(&state, d, depot(&state)).is_none());
}

/// Performance with about 2000 citizens; run with
/// `cargo test --release -p kingdoms_sim population_performance -- --ignored --nocapture`.
#[test]
#[ignore = "performance measurement; run in release mode"]
fn population_performance() {
    use std::time::Instant;
    let d = defs();
    let mut state = flat_state(256);
    crate::test_support::set_adults(&mut state, 0);
    // 400 houses (2000 places) with wells, markets, workplaces and storages around them.
    let mut houses = 0;
    for by in 0..20 {
        for bx in 0..20 {
            let (x, y) = (4 + bx * 12, 4 + by * 12);
            if x + 10 >= 250 || y + 10 >= 250 {
                continue;
            }
            spawn(&mut state, "house_wood", x, y);
            houses += 1;
            if (bx + by) % 2 == 0 {
                spawn(&mut state, "well", x + 3, y);
            }
            match (bx + 3 * by) % 6 {
                0 => {
                    spawn(&mut state, "market", x + 4, y + 4);
                }
                1 | 2 | 4 => {
                    spawn(&mut state, "chicken_farm", x + 5, y + 5);
                }
                3 => {
                    spawn(&mut state, "warehouse", x + 4, y + 4);
                }
                _ => {
                    spawn(&mut state, "woodcutter", x + 5, y + 5);
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
    while state.population.len() < 1990 {
        let kids = (state.population.len() % 4) as u16;
        demography::make_family(&mut state, d, 2, kids);
    }
    run(&mut state, TICKS_PER_HOUR);
    let citizens = state.population.len();
    let ticks = 2 * TICKS_PER_DAY;
    let mut population_ns = 0u128;
    let mut population_max = 0u128;
    let mut slowest_at = 0;
    let mut slow_steps = 0;
    let start = Instant::now();
    for _ in 0..ticks {
        // The tick pipeline (`GameState::tick`) with the population step timed separately.
        let tick_of_day = state.clock.tick_of_day() + 1;
        let spent = state.tick_timed(d).into_iter().find(|(name, _)| *name == "population").map_or(0, |(_, ns)| ns);
        population_ns += spent;
        if spent > 200_000 {
            slow_steps += 1;
        }
        if spent > population_max {
            population_max = spent;
            slowest_at = tick_of_day;
        }
    }
    let total = start.elapsed().as_nanos();
    eprintln!(
        "{houses} houses, {citizens} citizens ({} now), {} households: tick {:.1} µs, population step {:.1} µs \
         (average over {ticks} ticks), slowest population step {:.1} µs (tick {slowest_at} of the day), {slow_steps} steps over 200 µs",
        state.population.len(),
        state.population.households().count(),
        total as f64 / ticks as f64 / 1000.0,
        population_ns as f64 / ticks as f64 / 1000.0,
        population_max as f64 / 1000.0,
    );
    state.validate_entities(d).unwrap();
}


#[test]
fn loading_with_rebalanced_ages_and_capacities_reconciles() {
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 30, 30);
    run(&mut state, 1);
    assert_eq!(state.population.resident_count(house), 5);
    // Unchanged data: nothing changes.
    let mut copy = state.clone();
    rederive(&mut copy, defs());
    let bytes = |s: &GameState| crate::save::encode(&crate::save::SaveHeader::for_state(s, defs(), "x", 0), s).unwrap();
    assert_eq!(bytes(&copy), bytes(&state));
    // Adults become elders at 30 and houses hold 3: stages are fixed, households move out.
    let d = defs_edited("rules.ron", |t| t.replace("elder_age: 61", "elder_age: 30"));
    rederive(&mut state, &d);
    assert!(state.population.citizens().all(|(_, c)| c.stage() == LifeStage::Elder));
    assert!(state.workforce.needs_allocation());
    state.tick(&d, Vec::new());
    assert_eq!(state.workforce().employed(), 0, "elders do not work");
    state.validate_entities(&d).unwrap();
    let d = defs_edited("buildings/buildings.ron", |t| t.replace("housing: (capacity: 5,", "housing: (capacity: 3,"));
    rederive(&mut state, &d);
    state.tick(&d, Vec::new());
    assert_eq!(state.population.resident_count(house), 3);
    state.validate_entities(&d).unwrap();
}

/// Saves and loads `state` through the save format (validation included).
fn roundtrip(state: &GameState) -> Result<GameState, crate::save::SaveError> {
    let d = defs();
    let bytes = crate::save::encode(&crate::save::SaveHeader::for_state(state, d, "test", 0), state)?;
    crate::save::decode(&bytes, d).map(|s| s.state)
}

#[test]
fn retiring_workers_leave_their_job_at_once() {
    let d = defs();
    // The size of a small world, so that the state passes the save validation.
    let mut state = flat_state(128);
    crate::test_support::set_adults(&mut state, 2);
    // More jobs than adults: full employment.
    spawn(&mut state, "sawmill", 10, 10);
    spawn(&mut state, "sawmill", 20, 10);
    run(&mut state, 2);
    assert_eq!((state.workforce().employed(), state.workforce().available()), (2, 2));
    // One adult turns `elder_age` tomorrow.
    let (id, _) = state.population.citizens().next().unwrap();
    let today = i64::try_from(state.clock.total_days()).unwrap();
    let elder_age = i64::from(d.rules().population.elder_age);
    state.population.citizens.get_mut(id).unwrap().birth_day = today + 1 - elder_age * crate::time::DAYS_PER_SEASON as i64;
    let mut retired = false;
    for _ in 0..(2 * TICKS_PER_DAY) {
        run(&mut state, 1);
        assert!(state.workforce().employed() <= state.workforce().available());
        if state.population.citizen(id).unwrap().stage() == LifeStage::Elder {
            retired = true;
            break;
        }
    }
    assert!(retired);
    // The tick the stage changed: the elder has no job any more and a save taken now loads.
    assert!(state.population.citizen(id).unwrap().job().is_none());
    assert_eq!((state.workforce().employed(), state.workforce().available()), (1, 1));
    state.validate_entities(d).unwrap();
    roundtrip(&state).unwrap();
}

#[test]
fn crafted_saves_with_impossible_values_are_rejected() {
    let d = defs();
    let mut base = flat_state(128);
    let family = one_family(&mut base, 2);
    spawn(&mut base, "house_wood", 10, 10);
    run(&mut base, 2);
    base.validate_entities(d).unwrap();
    let members = base.population.household(family).unwrap().members().to_vec();
    let mother = members.iter().copied().find(|m| base.population.citizen(*m).unwrap().sex() == Sex::Female).unwrap();
    let invalid = |edit: &dyn Fn(&mut GameState)| {
        let mut state = base.clone();
        edit(&mut state);
        state.validate_entities(d).is_err() && roundtrip(&state).is_err()
    };
    roundtrip(&base).unwrap();
    // A birth day in the future or a last birth before the mother's own birth.
    assert!(invalid(&|s| s.population.citizens.get_mut(mother).unwrap().last_birth_day = Some(i64::MAX)));
    assert!(invalid(&|s| s.population.citizens.get_mut(mother).unwrap().last_birth_day = Some(i64::MIN)));
    assert!(invalid(&|s| s.population.citizens.get_mut(mother).unwrap().birth_day = 1_000));
    // Daily counters beyond any plausible value.
    assert!(invalid(&|s| s.population.stats.today.deaths = [u32::MAX; DeathCause::ALL.len()]));
    assert!(invalid(&|s| s.population.stats.today.immigrants = MAX_DAY_COUNT + 1));
    // An elder or a child with a job.
    let worker = members
        .iter()
        .copied()
        .find(|m| base.population.citizen(*m).unwrap().job().is_some())
        .expect("a parent is the depot's base carrier");
    assert!(invalid(&|s| s.population.citizens.get_mut(worker).unwrap().stage = LifeStage::Elder));
    // A member listed twice (another citizen unlisted) and a self-partnership.
    assert!(invalid(&|s| {
        let h = s.population.households.get_mut(family).unwrap();
        let first = h.members[0];
        h.members[1] = first;
    }));
    assert!(invalid(&|s| {
        let partner = s.population.citizen(mother).unwrap().partner().unwrap();
        s.population.citizens.get_mut(partner).unwrap().partner = None;
        s.population.citizens.get_mut(mother).unwrap().partner = Some(mother);
    }));
    // Counters at the limit saturate instead of overflowing.
    let mut state = base.clone();
    state.population.stats.today.deaths = [u32::MAX; DeathCause::ALL.len()];
    demography::die(&mut state, d, mother, DeathCause::Disease, &mut Vec::new());
    assert_eq!(state.population.stats.today().deaths_by(DeathCause::Disease), u32::MAX);
    assert_eq!(state.population.stats.today().total_deaths(), u32::MAX);
}

#[test]
fn oversized_households_split_instead_of_staying_homeless() {
    let d = defs();
    let mut state = flat_state(64);
    crate::test_support::set_adults(&mut state, 0);
    let farmhouse = spawn(&mut state, "farmhouse", 30, 30); // 7 places, cannot be built
    for i in 0..3 {
        spawn(&mut state, "house_wood", 20 + i * 3, 24);
    }
    spawn(&mut state, "well", 34, 30);
    let family = demography::make_family(&mut state, d, 2, 5);
    run(&mut state, 2);
    assert_eq!(state.population.household(family).unwrap().home(), Some(farmhouse));
    let name = state.population.household(family).unwrap().family_name().to_string();
    let couple: Vec<CitizenId> = state
        .population
        .household(family)
        .unwrap()
        .members()
        .iter()
        .copied()
        .filter(|m| state.population.citizen(*m).unwrap().stage() != LifeStage::Child)
        .collect();
    let mut events = exec(&mut state, Command::DemolishBuilding { building: farmhouse });
    assert!(crate::test_support::rejection(&events).is_none());
    events.extend(run(&mut state, 2));
    // No house the player can build holds 7: the family splits (5 + 2) and everybody lives
    // in one of the wooden houses.
    assert!(state.population.households().all(|(_, h)| h.home().is_some()), "nobody stays homeless");
    let parts: Vec<&Household> = state.population.households().map(|(_, h)| h).filter(|h| h.family_name() == name).collect();
    let mut sizes: Vec<usize> = parts.iter().map(|h| h.members().len()).collect();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![2, 5]);
    let core = state.population.household(family).unwrap();
    assert!(couple.iter().all(|c| core.members().contains(c)), "the parents stay together");
    assert!(events.iter().any(|e| matches!(e, SimEvent::Notification(n) if n.text.contains("verteilt sich auf 2 Haushalte"))));
    state.validate_entities(d).unwrap();
}

#[test]
fn immigrants_leave_a_place_for_births_and_marriages() {
    // A village of singles without church or tavern staff stays near 66 points: let settlers
    // come from 60 on, so the test does not depend on the first hours' rolls.
    let d = &defs_edited("rules.ron", |t| t.replace("immigration_happiness: 70,", "immigration_happiness: 60,"));
    let mut state = flat_state(64);
    let hall = spawn(&mut state, "town_hall", 20, 20);
    let depot = depot(&state);
    crate::test_support::link(&mut state, hall, depot);
    let houses: Vec<BuildingId> =
        [(40, 40), (43, 40), (46, 40), (40, 44)].iter().map(|(x, y)| spawn(&mut state, "house_wood", *x, *y)).collect();
    spawn(&mut state, "well", 44, 52);
    spawn(&mut state, "market", 50, 40);
    spawn(&mut state, "church", 50, 46);
    let tavern = spawn(&mut state, "tavern", 34, 40);
    stock_tavern(&mut state, tavern);
    crate::test_support::set_adults(&mut state, 3);
    let mut arrived = 0;
    for _ in 0..(2 * TICKS_PER_DAY) {
        for e in run_with(&mut state, d, 1) {
            if let SimEvent::HouseholdArrived { members, .. } = e {
                arrived += members;
            }
        }
    }
    assert!(arrived > 0, "happiness {:?}", state.population.average_happiness());
    let reserve = u16::from(d.rules().population.immigration_reserved_places);
    for h in &houses {
        let capacity = home_capacity(&state, d, *h);
        assert!(state.population.resident_count(*h) + reserve <= capacity, "house {h:?} keeps a free place");
    }
    let summary = crate::report::population_summary(&state, d);
    let free: u32 = houses.iter().map(|h| u32::from(home_capacity(&state, d, *h) - state.population.resident_count(*h))).sum();
    assert_eq!(summary.housing_capacity - summary.housed, free);
    assert!(summary.immigrant_places < free, "the reserved places are not open to immigrants");
    state.validate_entities(d).unwrap();
}

#[test]
fn marriages_pair_the_closest_age_of_another_household() {
    let d = defs();
    let mut state = flat_state(48);
    crate::test_support::set_adults(&mut state, 0);
    let rules = &d.rules().population;
    let day = state.clock.total_days();
    let age_of = |s: &GameState, id: CitizenId| s.population.citizen(id).unwrap().age_on(day);
    // Homeless singles (both homeless: anybody may join anybody).
    let mut singles = Vec::new();
    for _ in 0..8 {
        let h = demography::make_family(&mut state, d, 1, 0);
        singles.push(state.population.household(h).unwrap().members()[0]);
    }
    singles.sort_unstable();
    let women: Vec<CitizenId> = singles.iter().copied().filter(|c| state.population.citizen(*c).unwrap().sex() == Sex::Female).collect();
    let men: Vec<CitizenId> = singles.iter().copied().filter(|c| state.population.citizen(*c).unwrap().sex() == Sex::Male).collect();
    assert!(!women.is_empty() && !men.is_empty());
    // Expected pairs: every woman in id order takes the free man closest in age (ties: id).
    let mut free_men = men.clone();
    let mut expected = Vec::new();
    for w in &women {
        let best = free_men
            .iter()
            .copied()
            .filter(|m| age_of(&state, *m).abs_diff(age_of(&state, *w)) <= u32::from(rules.marriage_max_age_gap))
            .min_by_key(|m| (age_of(&state, *m).abs_diff(age_of(&state, *w)), *m));
        if let Some(m) = best {
            free_men.retain(|x| *x != m);
            expected.push((m, *w));
        }
    }
    // Certain marriages for the comparison.
    let edited = defs_edited("rules.ron", |t| t.replace("marriage_chance_percent: 20", "marriage_chance_percent: 100"));
    let mut events = Vec::new();
    demography::run_marriages(&mut state, &edited, &mut events);
    let married: Vec<(CitizenId, CitizenId)> = events
        .iter()
        .filter_map(|e| match e {
            SimEvent::CitizensMarried { a, b } => Some((*a, *b)),
            _ => None,
        })
        .collect();
    assert_eq!(married, expected);
    state.workforce.refresh_counts(&state.population);
    state.validate_entities(d).unwrap();
}

#[test]
fn households_restock_on_their_errand_not_every_hour() {
    let d = defs();
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 30, 30); // within the depot's carrier radius
    run(&mut state, 1);
    let household = state.population.citizen(residents(&state, house)[0]).unwrap().household();
    let food = |s: &GameState| -> u32 {
        s.population.household(household).unwrap().pantry().iter().filter(|p| d.resource(p.resource).is_edible()).map(|p| p.milli).sum()
    };
    // The first errand (empty pantry) fills it.
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| food(s) > 0);
    // The runner works during the day; its family's pantry empties at 10:00 (say, a feast).
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 10);
    let runner = state.population.household(household).unwrap().members()[0];
    assert!(state.population.citizen(runner).unwrap().job().is_some(), "the runner works until 17:00");
    state.population.households.get_mut(household).unwrap().pantry.clear();
    // Nothing is fetched every hour: the pantry stays empty until the runner is free.
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| s.clock().hour() == 16);
    assert_eq!(food(&state), 0, "no hourly restocking");
    let store = depot(&state);
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| {
        let m = s.population.citizen(runner).unwrap().motion();
        m.inside() == Some(store) && m.destination().and_then(Destination::purpose) == Some(Purpose::Errand)
    });
    assert_eq!(crate::population::current_activity(&state, d, state.population.citizen(runner).unwrap()), Activity::Errand);
    assert!(food(&state) > 0, "restocked at the depot (no market in reach)");
    state.validate_entities(d).unwrap();
}
