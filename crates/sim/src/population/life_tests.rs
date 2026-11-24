//! Tests of household money, taverns selling beer, friendships and the daily routine reacting
//! to festivals, sickness and enemies (docs/game_design.md §5.4, §6).

use glam::IVec2;

use super::movement::{Destination, Purpose};
use super::routine::{Venues, desired_destination};
use super::*;
use crate::defs::ServiceKind;
use crate::defs::Defs;
use crate::state::GameState;
use crate::test_support::{defs, depot, flat_state, link, run, spawn};
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};

fn beer(defs: &Defs) -> crate::defs::ResourceId {
    defs.resource_id("beer").unwrap()
}

/// A couple living in a house at (30, 30) near the depot, with a well; returns household,
/// house and the two adults.
fn couple_at_home(state: &mut GameState) -> (HouseholdId, BuildingId, Vec<CitizenId>) {
    let d = defs();
    crate::test_support::set_adults(state, 0);
    let house = spawn(state, "house_wood", 30, 30);
    spawn(state, "well", 33, 33);
    let family = demography::make_family(state, d, 2, 0);
    housing::sync_homes(state, d, &mut Vec::new());
    movement::place_inside_home(state, d, family);
    let members = state.population.household(family).unwrap().members().to_vec();
    (family, house, members)
}

#[test]
fn households_earn_pay_tax_and_fee_and_the_treasury_books_it() {
    let d = defs();
    let mut state = flat_state(64);
    let hall = spawn(&mut state, "town_hall", 20, 20);
    let dep = depot(&state);
    crate::test_support::link(&mut state, hall, dep);
    let (family, _house, members) = couple_at_home(&mut state);
    let market = spawn(&mut state, "market", 36, 26);
    run(&mut state, 2);
    state.buildings.get_mut(market).unwrap().staffed = 1;
    let services = ServiceIndex::collect(&state, d);
    let dues = wealth::dues(&state, d, &services, family);
    // Two adults: earnings of the wooden house per worker (employed) or a quarter of it.
    let earnings = wealth::gold_milli(d.building_by_key("house_wood").unwrap().housing.as_ref().unwrap().income_per_worker);
    assert!(dues.income >= earnings / 2, "{dues:?}");
    // Head tax 2 × 10 × 10 % and the market fee 2 × 0.5 per day.
    assert_eq!(dues.tax, 2_000);
    assert_eq!(dues.fee, 1_000);
    let before = state.population.household(family).unwrap().purse().savings_milli();
    let paid = wealth::run_hour(&mut state, d);
    let after = state.population.household(family).unwrap().purse().savings_milli();
    let hour = state.clock().total_hours() % 24;
    let share = |daily: i64| daily * (hour as i64 + 1) / 24 - daily * hour as i64 / 24;
    assert_eq!(after - before, share(dues.income) - share(dues.tax) - share(dues.fee));
    assert!(paid.taxes >= share(dues.tax), "the treasury books what the households paid");
    // Without savings and income nothing is paid, and the family is poor.
    for m in &members {
        state.population.citizens.get_mut(*m).unwrap().stage = LifeStage::Child;
        state.population.citizens.get_mut(*m).unwrap().job = None;
    }
    state.population.households.get_mut(family).unwrap().purse = Purse::default();
    let services = ServiceIndex::collect(&state, d);
    assert_eq!(wealth::dues(&state, d, &services, family).income, 0);
    let _ = wealth::run_hour(&mut state, d);
    assert_eq!(state.population.household(family).unwrap().purse().savings_milli(), 0);
    assert_eq!(wealth_class(&state, d, family), Some(WealthClass::Poor));
    let parts = citizen_happiness_parts(&state, d, members[0]).unwrap();
    let wealth = parts.iter().find(|p| p.factor == HappinessFactor::Wealth).map(|p| p.points);
    assert_eq!(wealth, Some(f32::from(d.rules().population.wealth.poverty_happiness)));
}

#[test]
fn crafted_huge_savings_are_rejected_and_sums_never_overflow() {
    let d = defs();
    let mut state = flat_state(64);
    let (family, _house, _) = couple_at_home(&mut state);
    state.population.validate(&state, d).unwrap();
    let mut broken = state.clone();
    broken.population.households.get_mut(family).unwrap().purse.savings_milli = i64::MAX - 5;
    assert!(broken.population.validate(&broken, d).is_err(), "impossible savings are refused on loading");
    // The largest valid purse in every household: the hourly money step, the forecast and
    // the report add up without overflowing.
    let ids: Vec<HouseholdId> = state.population.households.iter().map(|(id, _)| id).collect();
    for id in ids {
        let p = &mut state.population.households.get_mut(id).unwrap().purse;
        p.savings_milli = MAX_PURSE_MILLI;
        p.earned_yesterday = MAX_PURSE_MILLI;
        p.spent_yesterday = MAX_PURSE_MILLI;
    }
    state.population.validate(&state, d).unwrap();
    let services = ServiceIndex::collect(&state, d);
    let _ = wealth::expected_daily_payments(&state, d, &services);
    let _ = wealth::run_hour(&mut state, d);
    assert!(state.population.household(family).unwrap().purse().savings_milli() <= MAX_PURSE_MILLI);
    let report = crate::report::population_summary(&state, d);
    assert!(report.wealth.total_savings > 0);
}

#[test]
fn purchases_cost_the_market_price_and_goods_need_money() {
    let d = defs();
    let mut state = flat_state(64);
    let (family, house, _) = couple_at_home(&mut state);
    let market = spawn(&mut state, "market", 36, 26);
    state.buildings.get_mut(market).unwrap().staffed = 1;
    let dep = depot(&state);
    link(&mut state, house, dep);
    let clothes = d.resource_id("clothes").unwrap();
    // Rich: food and the goods of the next tier are bought; the price leaves the savings.
    state.population.households.get_mut(family).unwrap().purse = Purse::with_savings(1_000_000);
    supply::restock_on_errand(&mut state, d, family, market);
    let h = state.population.household(family).unwrap();
    assert!(h.pantry().iter().any(|p| d.resource(p.resource).is_edible()));
    let spent = 1_000_000 - h.purse().savings_milli();
    let expected: i64 = h.pantry().iter().map(|p| i64::from(p.milli / 1000) * wealth::unit_price_milli(&state, p.resource)).sum();
    assert_eq!(spent, expected, "every unit at its market price");
    // Broke: food still comes (the family pays nothing), goods do not.
    let mut state2 = flat_state(64);
    let (family2, house2, _) = couple_at_home(&mut state2);
    let market2 = spawn(&mut state2, "market", 36, 26);
    state2.buildings.get_mut(market2).unwrap().staffed = 1;
    let dep = depot(&state2);
    link(&mut state2, house2, dep);
    state2.population.households.get_mut(family2).unwrap().purse = Purse::default();
    supply::restock_on_errand(&mut state2, d, family2, market2);
    let h2 = state2.population.household(family2).unwrap();
    assert!(h2.pantry().iter().any(|p| d.resource(p.resource).is_edible()), "the poor are not left to starve");
    assert_eq!(h2.pantry_milli(clothes), 0);
    assert_eq!(h2.purse().savings_milli(), 0);
}

#[test]
fn taverns_are_stocked_by_carriers_and_sell_beer_to_their_guests() {
    let d = defs();
    let mut state = flat_state(64);
    let tavern = spawn(&mut state, "tavern", 50, 48);
    let dep = depot(&state);
    link(&mut state, tavern, dep);
    assert_eq!(state.building(tavern).unwrap().inventory().capacity(beer(d)), u32::from(d.rules().shop_stock));
    let report = crate::report::shop_report(&state, d, tavern).unwrap();
    assert_eq!(report.problem.as_deref(), Some("Kein Bier vorrätig"));
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| s.building(tavern).unwrap().inventory().get(beer(d)) > 0);
    run(&mut state, TICKS_PER_HOUR);
    assert!(crate::report::shop_report(&state, d, tavern).unwrap().problem.is_none());
    assert!(state.building(tavern).unwrap().inventory().get(beer(d)) > 0);
    // A guest's household buys beer for its pantry, at the market price.
    let household = state.population.households().map(|(id, _)| id).next().unwrap();
    state.population.households.get_mut(household).unwrap().purse = Purse::with_savings(100_000);
    let stock = state.building(tavern).unwrap().inventory().get(beer(d));
    supply::buy_at_venues(&mut state, d, &[(household, tavern)]);
    let h = state.population.household(household).unwrap();
    assert!(h.pantry_milli(beer(d)) >= 1000);
    let bought = stock - state.building(tavern).unwrap().inventory().get(beer(d));
    assert!(bought >= 1);
    assert_eq!(100_000 - h.purse().savings_milli(), i64::from(bought) * wealth::unit_price_milli(&state, beer(d)));
    // A household without money buys nothing.
    state.population.households.get_mut(household).unwrap().purse = Purse::default();
    let stock = state.building(tavern).unwrap().inventory().get(beer(d));
    supply::buy_at_venues(&mut state, d, &[(household, tavern)]);
    assert_eq!(state.building(tavern).unwrap().inventory().get(beer(d)), stock);
    assert!(crate::report::population_summary(&state, d).taverns.stocked >= 1);
}

#[test]
fn beer_decides_how_much_a_tavern_entertains() {
    let d = defs();
    let rules = &d.rules().population;
    let state = flat_state(32);
    let mut h = Household {
        family_name: "Test".into(),
        members: Vec::new(),
        home: None,
        pantry: Vec::new(),
        supply: SupplyState { services: service_bit(ServiceKind::Entertainment), tavern_goods: true, ..SupplyState::default() },
        unhappy_days: 0,
        purse: Purse::default(),
    };
    let level = |h: &Household, n: crate::defs::NeedKind| needs::need_levels(&state, d, h)[need_index(n)];
    assert_eq!(level(&h, crate::defs::NeedKind::Entertainment), 100);
    let social_with = level(&h, crate::defs::NeedKind::Social);
    h.supply.tavern_goods = false;
    assert_eq!(level(&h, crate::defs::NeedKind::Entertainment), rules.tavern_without_goods_entertainment);
    assert_eq!(social_with - level(&h, crate::defs::NeedKind::Social), rules.tavern_social_bonus - rules.tavern_social_bonus / 2);
    h.supply.services = 0;
    assert_eq!(level(&h, crate::defs::NeedKind::Entertainment), rules.base_entertainment);
}

#[test]
fn co_workers_become_friends_and_friendships_end_with_death() {
    let d = defs();
    let mut state = flat_state(48);
    crate::test_support::set_adults(&mut state, 0);
    let mill = spawn(&mut state, "sawmill", 30, 30);
    let a = demography::make_family(&mut state, d, 1, 0);
    let b = demography::make_family(&mut state, d, 1, 0);
    let ca = state.population.household(a).unwrap().members()[0];
    let cb = state.population.household(b).unwrap().members()[0];
    for c in [ca, cb] {
        let cit = state.population.citizens.get_mut(c).unwrap();
        cit.job = Some(Employment { building: mill, category: crate::defs::JobCategory::Processing });
        cit.motion.inside = Some(mill);
    }
    for _ in 0..400 {
        friends::run_hour(&mut state, d);
        state.clock.tick += TICKS_PER_HOUR;
        if !state.population.citizen(ca).unwrap().friends().is_empty() {
            break;
        }
    }
    assert_eq!(state.population.citizen(ca).unwrap().friends(), &[cb]);
    assert_eq!(state.population.citizen(cb).unwrap().friends(), &[ca], "friendships are mutual");
    assert_eq!(friends::social_bonus(d, 1), d.rules().population.friend_social_bonus);
    for c in [ca, cb] {
        state.population.citizens.get_mut(c).unwrap().job = None;
    }
    friends::validate(&state, d).unwrap();
    demography::remove_citizen(&mut state, d, cb, &mut Vec::new());
    assert!(state.population.citizen(ca).unwrap().friends().is_empty());
    // A one-sided friendship is rejected when loading.
    state.population.citizens.get_mut(ca).unwrap().friends.push(ca);
    assert!(friends::validate(&state, d).is_err());
}

/// Sets the clock to `hour`:00 of the second day (the game starts at 06:00).
fn at_hour(state: &mut GameState, hour: u64) {
    let start = crate::time::GameClock::new().tick_of_day();
    state.clock.tick = TICKS_PER_DAY + hour * TICKS_PER_HOUR - start;
}

#[test]
fn a_sick_hospital_worker_in_treatment_does_not_work() {
    let d = defs();
    let mut state = flat_state(64);
    let (_family, _house, members) = couple_at_home(&mut state);
    let hospital = spawn(&mut state, "hospital", 30, 36);
    let nurse = members[0];
    state.population.citizens.get_mut(nurse).unwrap().job =
        Some(Employment { building: hospital, category: d.building_by_key("hospital").unwrap().job.unwrap().category });
    at_hour(&mut state, 10);
    let door = state.building(hospital).unwrap().entrance();
    let mut inside = super::Motion::in_building(hospital, door);
    inside.dest = Some(Destination::Building { building: hospital, purpose: Purpose::Work });
    state.population.citizens.get_mut(nurse).unwrap().motion = inside.clone();
    let c = state.population.citizen(nurse).unwrap();
    assert!(is_at_work(&state, d, c));
    assert_eq!(routine::current_activity(&state, d, c), Activity::Working);
    // Sick and treated in the very building he works in: a patient, not a worker.
    inside.dest = Some(Destination::Building { building: hospital, purpose: Purpose::Treatment });
    let c = state.population.citizens.get_mut(nurse).unwrap();
    c.health = 30 * CENTI;
    c.motion = inside;
    let c = state.population.citizen(nurse).unwrap();
    assert!(!is_at_work(&state, d, c));
    assert_eq!(routine::current_activity(&state, d, c), Activity::Treatment);
}

#[test]
fn festivals_sickness_and_enemies_change_the_day() {
    let d = defs();
    let mut state = flat_state(64);
    let (family, house, members) = couple_at_home(&mut state);
    let mill = spawn(&mut state, "sawmill", 40, 30);
    let church = spawn(&mut state, "church", 22, 30);
    state.buildings.get_mut(church).unwrap().staffed = 1;
    let worker = members[0];
    state.population.citizens.get_mut(worker).unwrap().job =
        Some(Employment { building: mill, category: crate::defs::JobCategory::Processing });
    at_hour(&mut state, 15);
    let dest = |s: &GameState| desired_destination(s, d, worker, &mut Venues::default());
    assert_eq!(dest(&state), Some(Destination::Building { building: mill, purpose: Purpose::Work }));
    // Festival: the afternoon at the church.
    state.events.push_modifier(crate::events::ActiveModifier {
        kind: crate::events::ModifierKind::Festival,
        amount: 0,
        source: crate::events::ModifierSource::Event(d.events()[0].id),
        until_tick: state.clock.tick + TICKS_PER_HOUR,
    });
    assert_eq!(dest(&state), Some(Destination::Building { building: church, purpose: Purpose::Leisure }));
    at_hour(&mut state, 10);
    assert_eq!(dest(&state), Some(Destination::Building { building: mill, purpose: Purpose::Work }), "work in the morning");
    state.events.clear_modifiers();
    // Sick: at home, or in a hospital in reach.
    state.population.citizens.get_mut(worker).unwrap().health = 10 * CENTI;
    assert_eq!(dest(&state), Some(Destination::Building { building: house, purpose: Purpose::Home }));
    let hospital = spawn(&mut state, "hospital", 30, 36);
    state.buildings.get_mut(hospital).unwrap().staffed = 1;
    assert_eq!(dest(&state), Some(Destination::Building { building: hospital, purpose: Purpose::Treatment }));
    state.population.citizens.get_mut(worker).unwrap().health = MAX_VALUE;
    // Enemies near the workplace: the worker shelters at home.
    let hq = spawn(&mut state, "town_hall", 50, 50);
    let shelter = |s: &GameState, danger: Vec<IVec2>| desired_destination(s, d, worker, &mut Venues::with_danger(danger));
    assert_eq!(shelter(&state, vec![IVec2::new(44, 31)]), Some(Destination::Building { building: house, purpose: Purpose::Shelter }));
    // Enemies at the house too: the main headquarters.
    assert_eq!(
        shelter(&state, vec![IVec2::new(44, 31), IVec2::new(31, 33)]),
        Some(Destination::Building { building: hq, purpose: Purpose::Shelter })
    );
    // Far away: the normal day.
    assert_eq!(shelter(&state, vec![IVec2::new(5, 60)]), Some(Destination::Building { building: mill, purpose: Purpose::Work }));
    // Soldiers do not shelter.
    let _ = family;
}
