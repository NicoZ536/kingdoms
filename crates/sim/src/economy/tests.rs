//! Economy tests: daily money flows, taxes, bankruptcy, prices, trade and housing.

use glam::IVec2;

use super::*;
use crate::buildings::BuildingId;
use crate::command::{Command, RejectReason};
use crate::defs::ResourceId;
use crate::event::SimEvent;
use crate::state::GameState;
use crate::test_support::{defs, depot, exec, flat_state, link, rejection, run, spawn};
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};

fn res(key: &str) -> ResourceId {
    defs().resource_id(key).unwrap()
}

fn set_stock(state: &mut GameState, resource: ResourceId, amount: u32) {
    let d = depot(state);
    let b = state.buildings.get_mut(d).unwrap();
    let have = b.inventory.get(resource);
    b.inventory.remove(resource, have);
    b.inventory.add(resource, amount);
    storage::refresh_stock(state, defs());
}

/// Runs until the next midnight.
fn run_to_midnight(state: &mut GameState) -> Vec<SimEvent> {
    let left = TICKS_PER_DAY - state.clock().tick % TICKS_PER_DAY;
    run(state, left)
}

/// A town with a connected town hall, two houses, a market and the depot.
fn town() -> (GameState, BuildingId, [BuildingId; 2]) {
    let mut state = flat_state(48);
    let hall = spawn(&mut state, "town_hall", 30, 30);
    let d = depot(&state);
    link(&mut state, hall, d);
    let houses = [spawn(&mut state, "house_wood", 24, 30), spawn(&mut state, "house_wood", 24, 34)];
    spawn(&mut state, "market", 30, 36);
    (state, hall, houses)
}

#[test]
fn daily_money_flows_follow_the_rates() {
    let d = defs();
    let (mut state, _, houses) = town();
    run(&mut state, 1);
    // 20 settlers, 10 places: both houses full.
    let occupancy = crate::workforce::housing_occupancy(&state, d);
    assert_eq!(occupancy.iter().map(|h| (h.building, h.residents)).collect::<Vec<_>>(), vec![(houses[0], 5), (houses[1], 5)]);
    let rates = finance::daily_rates(&state, d);
    // Taxes: 10 residents x 10 gold x 10 % tax rate.
    assert_eq!(rates.taxes, 10_000);
    // Market fees: 10 residents x 0.5 gold.
    assert_eq!(rates.market_fees, 5_000);
    // Wages: town hall 2 skilled (1.0), market 3 + depot 3 unskilled (0.6).
    assert_eq!(rates.wages, -5_600);
    // Upkeep: market 3 + warehouse 3 (town hall and houses are free).
    assert_eq!(rates.upkeep, -6_000);
    assert_eq!(rates.net(), 3_400);
    // A full day books exactly these amounts.
    run_to_midnight(&mut state);
    let before = state.economy().treasury_milli();
    let events = run_to_midnight(&mut state);
    let day = *state.economy().ledger().yesterday().unwrap();
    assert_eq!(day.milli(LedgerCategory::Taxes), 10_000);
    assert_eq!(day.milli(LedgerCategory::MarketFees), 5_000);
    assert_eq!(day.milli(LedgerCategory::Wages), -5_600);
    assert_eq!(day.milli(LedgerCategory::Upkeep), -6_000);
    assert_eq!(day.milli(LedgerCategory::Military), 0);
    assert_eq!(state.economy().treasury_milli() - before, 3_400);
    assert!(events.iter().any(|e| matches!(e, SimEvent::DailyBalance { income: 15, expenses: -11, .. })));
    assert_eq!(state.economy().ledger().history().len(), 2);
    // The tax rate drives the taxes.
    exec(&mut state, Command::SetTaxRate { percent: 20 });
    assert_eq!(finance::daily_rates(&state, d).taxes, 20_000);
    // Houses outside every administration radius pay nothing.
    let far = spawn(&mut state, "house_wood", 2, 2);
    crate::test_support::set_adults(&mut state, 15);
    run(&mut state, 1);
    assert!(crate::workforce::housing_occupancy(&state, d).iter().any(|h| h.building == far && h.residents == 5));
    assert_eq!(finance::daily_rates(&state, d).taxes, 20_000);
    let report = crate::report::finance_report(&state, d);
    assert_eq!(report.history.len(), 2);
    assert_eq!(report.tax_rate_percent, 20);
}

#[test]
fn bankruptcy_stops_construction_and_recovers() {
    let d = defs();
    let mut state = flat_state(48);
    // Expenses only: the depot's carriers and upkeep.
    let gold = state.economy().treasury();
    state.economy.add_gold(-gold + 1);
    let mut events = Vec::new();
    for _ in 0..(2 * TICKS_PER_DAY) {
        events.extend(run(&mut state, 1));
        if state.economy().is_bankrupt() {
            break;
        }
    }
    assert!(state.economy().is_bankrupt());
    assert!(state.economy().treasury() < 0);
    assert!(events.iter().any(|e| matches!(e, SimEvent::BankruptcyStarted { .. })));
    assert!(events.iter().filter_map(SimEvent::notification).any(|n| n.text.contains("Staatsbankrott")));
    // No new construction, no paid roads; free paths are still allowed.
    let house = d.building_id("house_wood").unwrap();
    let events = exec(&mut state, Command::PlaceBuilding { kind: house, origin: IVec2::new(10, 10), rotation: Default::default() });
    assert_eq!(rejection(&events), Some(&RejectReason::Placement(crate::buildings::PlacementError::Bankrupt)));
    let gravel = d.road_id("gravel").unwrap();
    let tiles = vec![IVec2::new(5, 5), IVec2::new(6, 5)];
    let events = exec(&mut state, Command::BuildRoad { tiles: tiles.clone(), road: gravel });
    assert_eq!(rejection(&events), Some(&RejectReason::Road(crate::roads::RoadProblem::Bankrupt)));
    let events = exec(&mut state, Command::BuildRoad { tiles, road: d.road_id("path").unwrap() });
    assert_eq!(rejection(&events), None);
    // Money comes in: the bankruptcy ends at the next hour.
    state.economy.add_gold(100);
    let events = run(&mut state, TICKS_PER_HOUR);
    assert!(!state.economy().is_bankrupt());
    assert!(events.iter().any(|e| matches!(e, SimEvent::BankruptcyEnded { .. })));
}

#[test]
fn shrinking_treasury_is_announced() {
    let mut state = flat_state(48);
    let gold = state.economy().treasury();
    // 6 gold upkeep + 1.8 wages per day: 15 gold last about two days.
    state.economy.add_gold(-gold + 15);
    run_to_midnight(&mut state);
    let events = run_to_midnight(&mut state);
    assert!(events.iter().any(|e| matches!(e, SimEvent::TreasuryLow { days_left: 0..=2, .. })), "{events:?}");
}

#[test]
fn prices_react_to_oversupply_and_shortage() {
    let d = defs();
    let mut state = flat_state(48);
    let (wood, flour) = (res("wood"), res("flour"));
    set_stock(&mut state, flour, 6);
    let bakery = spawn(&mut state, "bakery", 30, 38);
    let store = depot(&state);
    link(&mut state, bakery, store);
    run_to_midnight(&mut state);
    run_to_midnight(&mut state);
    let prices = state.economy().prices();
    let base = |r: ResourceId| d.resource(r).base_price * 100;
    // 6 flour eaten, none left: demand above supply.
    assert!(prices.centi(flour) > base(flour), "flour {}", prices.centi(flour));
    // 300 wood lying around with little use: oversupply.
    assert!(prices.centi(wood) < base(wood), "wood {}", prices.centi(wood));
    // Smoothing: after many days the price approaches the floor of the formula.
    for _ in 0..10 {
        run_to_midnight(&mut state);
    }
    let floor = (f64::from(base(wood)) * f64::from(d.rules().price_ratio_min).sqrt()).round() as u32;
    let wood_price = state.economy().prices().centi(wood);
    assert!(wood_price <= floor + 20, "wood {wood_price}, floor {floor}");
    // The history keeps one price per closed day.
    let history = crate::report::resource_history(&state, wood);
    assert_eq!(history.len(), 12);
    assert!(history.windows(2).all(|w| w[1].price_centi <= w[0].price_centi));
    let row = crate::report::economy_summary(&state, d).resources.into_iter().find(|r| r.resource == wood).unwrap();
    assert_eq!(row.price_centi, wood_price);
    assert!(row.price_trend_centi <= 0);
}

#[test]
fn trade_orders_buy_and_sell_at_the_trading_post() {
    let d = defs();
    let mut state = flat_state(48);
    let (wood, spices) = (res("wood"), res("spices"));
    set_stock(&mut state, spices, 0);
    let sell = TradeOrder { resource: wood, buy_below: None, sell_above: Some(100), daily_limit: 50 };
    let buy = TradeOrder { resource: spices, buy_below: Some(20), sell_above: None, daily_limit: 30 };
    for order in [sell, buy] {
        let events = exec(&mut state, Command::SetTradeOrder { order });
        assert!(events.iter().any(|e| matches!(e, SimEvent::TradeOrderChanged { order: Some(_), .. })));
    }
    // Without a trading post nothing happens.
    run(&mut state, 2 * TICKS_PER_HOUR);
    assert_eq!(state.economy().stock().get(wood), 300);
    assert_eq!(crate::report::trade_report(&state, d).trading_post, None);
    let post = spawn(&mut state, "trading_post", 30, 36);
    let store = depot(&state);
    link(&mut state, post, store);
    let wood_price = state.economy().prices().centi(wood);
    let spice_price = state.economy().prices().centi(spices);
    let treasury = state.economy().treasury_milli();
    let events = run(&mut state, TICKS_PER_HOUR);
    // Sold 50 wood (daily limit) at price -20 %, bought 20 spices at price +20 %.
    assert_eq!(state.economy().stock().get(wood), 250);
    assert_eq!(state.economy().stock().get(spices), 20);
    let income = 50 * i64::from(wood_price) * 8;
    let cost = 20 * i64::from(spice_price) * 12;
    assert!(events.iter().any(|e| matches!(e, SimEvent::TradeExecuted { direction: TradeDirection::Export, amount: 50, .. })));
    assert!(events.iter().any(|e| matches!(e, SimEvent::TradeExecuted { direction: TradeDirection::Import, amount: 20, .. })));
    let today = state.economy().ledger().today();
    assert_eq!(today.milli(LedgerCategory::Exports), income);
    assert_eq!(today.milli(LedgerCategory::Imports), -cost);
    assert!(state.economy().treasury_milli() <= treasury + income - cost);
    assert_eq!(state.economy().stats().today().exported.get(wood), 50);
    // The daily limit holds until midnight, then trading resumes.
    run(&mut state, 2 * TICKS_PER_HOUR);
    assert_eq!(state.economy().stock().get(wood), 250);
    run_to_midnight(&mut state);
    run(&mut state, TICKS_PER_HOUR);
    assert_eq!(state.economy().stock().get(wood), 200);
    assert_eq!(state.economy().stock().get(spices), 20, "already at the buy threshold");
    let report = crate::report::trade_report(&state, d);
    assert_eq!(report.trading_post, Some(post));
    assert_eq!(report.orders.len(), 2);
    // Removing an order stops it; removing it twice is rejected.
    let events = exec(&mut state, Command::RemoveTradeOrder { resource: wood });
    assert!(events.iter().any(|e| matches!(e, SimEvent::TradeOrderChanged { order: None, .. })));
    let events = exec(&mut state, Command::RemoveTradeOrder { resource: wood });
    assert_eq!(rejection(&events), Some(&RejectReason::NoTradeOrder));
    let bad = TradeOrder { resource: wood, buy_below: Some(50), sell_above: Some(10), daily_limit: 5 };
    let events = exec(&mut state, Command::SetTradeOrder { order: bad });
    assert!(matches!(rejection(&events), Some(RejectReason::TradeThresholdsOverlap { .. })));
}

#[test]
fn workforce_summary_reports_housing_and_jobs() {
    let d = defs();
    let (mut state, _, _) = town();
    crate::test_support::set_adults(&mut state, 12);
    run(&mut state, 1);
    let s = crate::report::workforce_report(&state, d);
    assert_eq!((s.population, s.housed, s.homeless, s.housing_capacity), (12, 10, 2, 10));
    // Jobs: town hall 2, market 3, depot 3 (all services).
    let services = s.categories.iter().find(|c| c.category == crate::defs::JobCategory::Services).unwrap();
    assert_eq!((services.wanted, services.staffed), (8, 8));
    assert_eq!((s.employed, s.unemployed, s.vacancies), (8, 4, 0));
    crate::test_support::set_adults(&mut state, 5);
    run(&mut state, 1);
    let s = crate::report::workforce_report(&state, d);
    assert_eq!((s.employed, s.unemployed, s.vacancies), (5, 0, 3));
}

#[test]
fn treasury_warnings_use_the_singular_and_name_an_empty_treasury() {
    use crate::economy::finance::treasury_warning;
    assert_eq!(treasury_warning(0, 0), "Die Staatskasse ist leer.");
    assert_eq!(treasury_warning(-5, 0), "Die Staatskasse ist leer.");
    assert_eq!(treasury_warning(20, 0), "Die Staatskasse reicht nicht einmal mehr für 1 Tag.");
    assert_eq!(treasury_warning(80, 1), "Die Staatskasse reicht nur noch für etwa 1 Tag.");
    assert_eq!(treasury_warning(300, 3), "Die Staatskasse reicht nur noch für etwa 3 Tage.");
}

#[test]
fn trade_roads_to_the_map_edge_improve_world_market_prices() {
    let d = defs();
    let mut state = flat_state(48);
    crate::test_support::set_adults(&mut state, 20);
    let post = spawn(&mut state, "trading_post", 30, 20);
    let entrance = state.building(post).unwrap().entrance();
    assert_eq!(trade::trade_route_percent(&state, d, post), 0, "no road to the edge");
    // A path straight north from the entrance to the map edge.
    let lay = |state: &mut GameState, road: &str| {
        let road = d.road_id(road).unwrap();
        for y in 0..=entrance.y {
            state.roads.set(IVec2::new(entrance.x, y), Some(crate::roads::RoadTile { road, bridge_axis: None }));
        }
    };
    lay(&mut state, "path");
    assert_eq!(trade::trade_route_percent(&state, d, post), 0, "plain paths give no bonus");
    lay(&mut state, "trade_road");
    let full = i32::from(d.road_by_key("trade_road").unwrap().trade_bonus_percent);
    assert_eq!(trade::trade_route_percent(&state, d, post), full);
    // Half the route on a plain path: about half the bonus.
    let path = d.road_id("path").unwrap();
    for y in 0..=entrance.y / 2 {
        state.roads.set(IVec2::new(entrance.x, y), Some(crate::roads::RoadTile { road: path, bridge_axis: None }));
    }
    let half = trade::trade_route_percent(&state, d, post);
    assert!(half > 0 && half < full, "{half}");
    // The report names it, and prices include it.
    let report = crate::report::trade_report(&state, d);
    assert_eq!(report.trade_road_percent, half);
    assert!(report.trade_road_text.contains("Handelsstraßen"), "{}", report.trade_road_text);
    let price = 1000;
    assert!(
        trade::trade_unit_price_centi(price, TradeDirection::Export, 20, half)
            > trade::trade_unit_price_centi(price, TradeDirection::Export, 20, 0)
    );
}

#[test]
fn caravans_count_the_trade_road_share_of_their_walk() {
    let d = defs();
    let mut state = flat_state(32);
    let trade_road = d.road_id("trade_road").unwrap();
    for x in 0..4 {
        state.roads.set(IVec2::new(x, 5), Some(crate::roads::RoadTile { road: trade_road, bridge_axis: None }));
    }
    let full = u32::from(d.road_by_key("trade_road").unwrap().trade_bonus_percent);
    assert_eq!(crate::diplomacy::trade_road_bonus_at(&state, d, IVec2::new(2, 5)), full as u16);
    assert_eq!(crate::diplomacy::trade_road_bonus_at(&state, d, IVec2::new(2, 6)), 0);
    let mut tally = crate::diplomacy::RouteTally::default();
    tally.tiles = 8;
    tally.bonus_sum = 4 * full;
    assert_eq!(tally.bonus_percent(), i32::try_from(full / 2).unwrap());
    assert_eq!(crate::diplomacy::RouteTally::default().bonus_percent(), 0);
}
