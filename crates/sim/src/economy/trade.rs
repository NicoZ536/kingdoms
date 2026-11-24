//! Trade orders with the world market (docs/game_design.md §7), executed at a trading post.
//!
//! An order per resource holds a sell threshold ("sell wood above 200"), a buy threshold
//! ("buy spices up to 20") and a daily limit in units. Once per game hour every order trades
//! as much as its thresholds, the remaining daily limit, storage room and the treasury allow:
//! sales earn the current price minus the market margin, purchases cost the price plus the
//! margin (research `TradePriceBonus` improves both). Trading needs an operational, staffed
//! trading post that is part of the kingdom stock network; goods come from and go to the
//! storages nearest to it. Imports-only luxury goods (spices, silk) can only be obtained this
//! way. Until AI kingdoms exist (Phase 7) the counterpart is an abstract world market.

use std::collections::BTreeMap;

use glam::IVec2;

use serde::{Deserialize, Serialize};

use super::ledger::{LedgerCategory, MILLI_PER_GOLD};
use super::storage;
use crate::buildings::BuildingId;
use crate::command::RejectReason;
use crate::defs::{BuildingRole, Defs, ResourceId, TechEffect};
use crate::event::SimEvent;
use crate::research::Research;
use crate::state::GameState;

/// A standing trade order for one resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeOrder {
    /// Traded resource.
    pub resource: ResourceId,
    /// Buy while the kingdom stock is below this amount.
    pub buy_below: Option<u32>,
    /// Sell what exceeds this amount (material owed to construction sites is never sold).
    pub sell_above: Option<u32>,
    /// Most units bought and sold per day.
    pub daily_limit: u32,
}

/// Direction of a trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TradeDirection {
    /// Goods bought.
    Import,
    /// Goods sold.
    Export,
}

impl TradeDirection {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            TradeDirection::Import => "Einkauf",
            TradeDirection::Export => "Verkauf",
        }
    }
}

/// Units traded today for one resource.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeVolume {
    /// Units bought.
    pub bought: u32,
    /// Units sold.
    pub sold: u32,
}

/// Standing orders and today's volumes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeBook {
    orders: BTreeMap<ResourceId, TradeOrder>,
    today: BTreeMap<ResourceId, TradeVolume>,
}

impl TradeBook {
    /// All orders in resource id order.
    pub fn orders(&self) -> impl Iterator<Item = &TradeOrder> + '_ {
        self.orders.values()
    }

    /// The order for `resource`.
    pub fn order(&self, resource: ResourceId) -> Option<&TradeOrder> {
        self.orders.get(&resource)
    }

    /// Units of `resource` traded today.
    pub fn volume_today(&self, resource: ResourceId) -> TradeVolume {
        self.today.get(&resource).copied().unwrap_or_default()
    }

    pub(crate) fn set(&mut self, order: TradeOrder) -> bool {
        self.orders.insert(order.resource, order) != Some(order)
    }

    pub(crate) fn remove(&mut self, resource: ResourceId) -> bool {
        self.orders.remove(&resource).is_some()
    }

    pub(crate) fn reset_day(&mut self) {
        self.today.clear();
    }

    pub(crate) fn record(&mut self, resource: ResourceId, direction: TradeDirection, units: u32) {
        let v = self.today.entry(resource).or_default();
        match direction {
            TradeDirection::Import => v.bought += units,
            TradeDirection::Export => v.sold += units,
        }
    }

    pub(crate) fn is_consistent(&self, defs: &Defs) -> bool {
        let n = defs.resources().len();
        self.orders.iter().all(|(r, o)| *r == o.resource && r.index() < n && validate_order(o, defs).is_ok())
            && self.today.keys().all(|r| r.index() < n)
    }
}

/// Checks an order against the rules.
pub fn validate_order(order: &TradeOrder, defs: &Defs) -> Result<(), RejectReason> {
    if order.resource.index() >= defs.resources().len() {
        return Err(RejectReason::UnknownResource);
    }
    if order.buy_below.is_none() && order.sell_above.is_none() {
        return Err(RejectReason::TradeOrderEmpty);
    }
    if let (Some(buy), Some(sell)) = (order.buy_below, order.sell_above)
        && buy > sell
    {
        return Err(RejectReason::TradeThresholdsOverlap { buy_below: buy, sell_above: sell });
    }
    let max = defs.rules().max_trade_daily_limit;
    if order.daily_limit == 0 || order.daily_limit > max {
        return Err(RejectReason::TradeLimitOutOfRange { max });
    }
    Ok(())
}

/// Sum of all researched [`TechEffect::TradePriceBonus`] percentages.
pub fn trade_bonus_percent(research: &Research, defs: &Defs) -> i32 {
    research
        .researched()
        .filter_map(|t| defs.techs().get(t.index()))
        .flat_map(|t| t.effects.iter())
        .map(|e| match e {
            TechEffect::TradePriceBonus { percent } => i32::from(*percent),
            _ => 0,
        })
        .sum()
}

/// Trade road share of the trading post's road connection to the map edge (world market,
/// docs/game_design.md §5.6): along the shortest road route from the post's entrance to any
/// map-edge tile, the average `trade_bonus_percent` of the roads (a route of trade road only
/// gives the full bonus); 0 without a road route to the edge.
pub fn trade_route_percent(state: &GameState, defs: &Defs, post: BuildingId) -> i32 {
    let Some(start) = state.buildings.get(post).map(|b| b.entrance()) else { return 0 };
    let (w, h) = (state.world.width(), state.world.height());
    let index = |p: IVec2| usize::try_from(p.y * w + p.x).ok();
    if !state.roads.has_road(start) {
        return 0;
    }
    let edge = |p: IVec2| p.x == 0 || p.y == 0 || p.x == w - 1 || p.y == h - 1;
    let mut parent: Vec<u32> = vec![u32::MAX; usize::try_from(w * h).unwrap_or(0)];
    let Some(si) = index(start) else { return 0 };
    parent[si] = si as u32;
    let mut queue = std::collections::VecDeque::from([start]);
    let mut goal = None;
    while let Some(p) = queue.pop_front() {
        if edge(p) {
            goal = Some(p);
            break;
        }
        for d in [IVec2::new(0, -1), IVec2::new(1, 0), IVec2::new(0, 1), IVec2::new(-1, 0)] {
            let q = p + d;
            if !state.world.in_bounds(q) || !state.roads.has_road(q) {
                continue;
            }
            let (Some(qi), Some(pi)) = (index(q), index(p)) else { continue };
            if parent[qi] == u32::MAX {
                parent[qi] = pi as u32;
                queue.push_back(q);
            }
        }
    }
    let Some(mut p) = goal else { return 0 };
    let (mut tiles, mut sum) = (0u32, 0u32);
    loop {
        tiles += 1;
        sum += u32::from(crate::diplomacy::trade_road_bonus_at(state, defs, p));
        let Some(pi) = index(p) else { break };
        let up = parent[pi] as usize;
        if up == pi {
            break;
        }
        p = IVec2::new(up as i32 % w, up as i32 / w);
    }
    i32::try_from(sum / tiles.max(1)).unwrap_or(0)
}

/// Percent of the partner's price a trade in `direction` pays (docs/game_design.md §7): the
/// margin against the player (sell for 100 − margin, buy for 100 + margin), narrowed by a
/// positive `bonus_percent` – at most down to the partner's price itself, so buying never costs
/// less than selling fetches – and widened by a negative one.
pub fn trade_percent(direction: TradeDirection, margin_percent: u8, bonus_percent: i32) -> i64 {
    let margin = i64::from(margin_percent);
    let shift = i64::from(bonus_percent).min(margin);
    match direction {
        TradeDirection::Export => (100 - margin + shift).clamp(1, 200),
        TradeDirection::Import => (100 + margin - shift).clamp(1, 300),
    }
}

/// Unit price in centi-gold for a trade in `direction` at market price `price_centi` on the
/// world market (bonus 0–50 %, see [`trade_percent`]).
pub fn trade_unit_price_centi(price_centi: u32, direction: TradeDirection, margin_percent: u8, bonus_percent: i32) -> u32 {
    let percent = trade_percent(direction, margin_percent, bonus_percent.clamp(0, 50));
    (i64::from(price_centi) * percent / 100).clamp(1, i64::from(u32::MAX)) as u32
}

/// The trading post that executes orders: operational, staffed, part of the kingdom stock
/// network (lowest id).
pub fn active_trading_post(state: &GameState, defs: &Defs) -> Option<BuildingId> {
    storage::reachable_storages(state, defs).into_iter().find(|id| {
        state.buildings.get(*id).is_some_and(|b| {
            b.trade_staff() > 0 && defs.buildings().get(b.kind().index()).is_some_and(|d| d.role == BuildingRole::TradeHub)
        })
    })
}

/// Executes all orders once (called every game hour).
pub(crate) fn run_trade(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    if state.economy.trade().orders.is_empty() {
        return;
    }
    let Some(post) = active_trading_post(state, defs) else { return };
    let Some(at) = state.buildings.get(post).map(|b| b.entrance()) else { return };
    let margin = defs.rules().trade_margin_percent;
    // Research plus the trade road share of the post's route to the map edge.
    let bonus = trade_bonus_percent(&state.research, defs) + trade_route_percent(state, defs, post);
    let orders: Vec<TradeOrder> = state.economy.trade().orders().copied().collect();
    for order in orders {
        let r = order.resource;
        let used = {
            let v = state.economy.trade().volume_today(r);
            v.bought + v.sold
        };
        let mut left = order.daily_limit.saturating_sub(used);
        let price = state.economy.prices().centi(r);
        if let Some(threshold) = order.sell_above
            && left > 0
        {
            let excess = state.economy.available(r).saturating_sub(threshold);
            let n = excess.min(left);
            if n > 0 {
                let sold = storage::withdraw(state, defs, r, n, at);
                if sold > 0 {
                    let unit = trade_unit_price_centi(price, TradeDirection::Export, margin, bonus);
                    let milli = i64::from(sold) * i64::from(unit) * (MILLI_PER_GOLD / 100);
                    state.economy.book_milli(LedgerCategory::Exports, milli);
                    state.economy.stats_mut().today_mut().exported.add(r, sold);
                    state.economy.trade_mut().record(r, TradeDirection::Export, sold);
                    events.push(SimEvent::TradeExecuted {
                        resource: r,
                        amount: sold,
                        gold: milli / MILLI_PER_GOLD,
                        direction: TradeDirection::Export,
                    });
                    left -= sold;
                }
            }
        }
        if let Some(threshold) = order.buy_below
            && left > 0
            && !state.economy.is_bankrupt()
        {
            let stock = state.economy.stock().get(r);
            let room = state.economy.capacity().get(r).saturating_sub(stock);
            let unit = trade_unit_price_centi(price, TradeDirection::Import, margin, bonus);
            let unit_milli = i64::from(unit) * (MILLI_PER_GOLD / 100);
            let affordable = (state.economy.treasury_milli().max(0) / unit_milli.max(1)).min(i64::from(u32::MAX)) as u32;
            let n = threshold.saturating_sub(stock).min(left).min(room).min(affordable);
            if n > 0 {
                let bought = storage::deposit(state, defs, r, n, at);
                if bought > 0 {
                    let milli = i64::from(bought) * unit_milli;
                    state.economy.book_milli(LedgerCategory::Imports, -milli);
                    state.economy.stats_mut().today_mut().imported.add(r, bought);
                    state.economy.trade_mut().record(r, TradeDirection::Import, bought);
                    events.push(SimEvent::TradeExecuted {
                        resource: r,
                        amount: bought,
                        gold: -(milli / MILLI_PER_GOLD),
                        direction: TradeDirection::Import,
                    });
                }
            }
        }
    }
}

/// `SetTradeOrder`.
pub(crate) fn set_order(state: &mut GameState, defs: &Defs, order: TradeOrder, events: &mut Vec<SimEvent>) -> Result<(), RejectReason> {
    validate_order(&order, defs)?;
    if state.economy.trade_mut().set(order) {
        events.push(SimEvent::TradeOrderChanged { resource: order.resource, order: Some(order) });
    }
    Ok(())
}

/// `RemoveTradeOrder`.
pub(crate) fn remove_order(state: &mut GameState, resource: ResourceId, events: &mut Vec<SimEvent>) -> Result<(), RejectReason> {
    if !state.economy.trade_mut().remove(resource) {
        return Err(RejectReason::NoTradeOrder);
    }
    events.push(SimEvent::TradeOrderChanged { resource, order: None });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::defs;

    fn res(key: &str) -> ResourceId {
        defs().resource_id(key).unwrap()
    }

    #[test]
    fn orders_are_validated() {
        let d = defs();
        let ok = TradeOrder { resource: res("wood"), buy_below: Some(20), sell_above: Some(200), daily_limit: 50 };
        assert_eq!(validate_order(&ok, d), Ok(()));
        let empty = TradeOrder { buy_below: None, sell_above: None, ..ok };
        assert_eq!(validate_order(&empty, d), Err(RejectReason::TradeOrderEmpty));
        let overlap = TradeOrder { buy_below: Some(300), ..ok };
        assert!(matches!(validate_order(&overlap, d), Err(RejectReason::TradeThresholdsOverlap { .. })));
        let zero = TradeOrder { daily_limit: 0, ..ok };
        assert!(matches!(validate_order(&zero, d), Err(RejectReason::TradeLimitOutOfRange { .. })));
        for e in [RejectReason::TradeOrderEmpty, validate_order(&overlap, d).unwrap_err()] {
            assert!(e.to_string().ends_with('.'), "{e}");
        }
    }

    #[test]
    fn unit_prices_apply_margin_and_bonus() {
        assert_eq!(trade_unit_price_centi(1000, TradeDirection::Export, 20, 0), 800);
        assert_eq!(trade_unit_price_centi(1000, TradeDirection::Import, 20, 0), 1200);
        assert_eq!(trade_unit_price_centi(1000, TradeDirection::Export, 20, 10), 900);
        assert_eq!(trade_unit_price_centi(1000, TradeDirection::Import, 20, 10), 1100);
        // A bonus beyond the margin cannot make buying cheaper than selling.
        for bonus in [20, 35, 50] {
            assert_eq!(trade_unit_price_centi(1000, TradeDirection::Export, 20, bonus), 1000);
            assert_eq!(trade_unit_price_centi(1000, TradeDirection::Import, 20, bonus), 1000);
        }
        assert_eq!(trade_percent(TradeDirection::Export, 15, -30), 55, "a bad relation widens the margin");
        assert_eq!(trade_percent(TradeDirection::Import, 15, -30), 145);
        let mut book = TradeBook::default();
        let order = TradeOrder { resource: res("wood"), buy_below: None, sell_above: Some(10), daily_limit: 5 };
        assert!(book.set(order));
        assert!(!book.set(order), "unchanged");
        book.record(res("wood"), TradeDirection::Export, 3);
        assert_eq!(book.volume_today(res("wood")), TradeVolume { bought: 0, sold: 3 });
        book.reset_day();
        assert_eq!(book.volume_today(res("wood")), TradeVolume::default());
        assert!(book.is_consistent(defs()));
        assert!(book.remove(res("wood")) && !book.remove(res("wood")));
    }
}
