//! The kingdom's money and goods (docs/game_design.md §7).
//!
//! * **Storage:** goods live in the [`Inventory`] of storage buildings (town hall, warehouse,
//!   storehouse, trading post; capacities from their `StorageDef` plus research bonuses) and in
//!   the input/output buffers of production buildings. The *kingdom stock* ([`Economy::stock`])
//!   is a view: the sum over all operational storages that are reachable in the road network of
//!   the main headquarters (see [`storage`]). It is refreshed after every change.
//! * **Treasury:** gold in milli-gold so hourly accruals stay exact; booked per
//!   [`LedgerCategory`] in a daily [`Ledger`] with a rolling history.
//! * **Prices:** dynamic per-resource prices from supply and demand ([`market`]).
//! * **Finance:** hourly taxes, market fees, wages and upkeep; bankruptcy ([`finance`]).
//! * **Trade:** buy/sell orders with thresholds executed at a trading post ([`trade`]).

pub mod finance;
mod inventory;
pub mod ledger;
pub mod market;
pub mod storage;
pub mod trade;

use serde::{Deserialize, Serialize};

pub use inventory::Inventory;
pub use ledger::{DayLedger, HISTORY_DAYS, Ledger, LedgerCategory, MILLI_PER_GOLD};
pub use market::{DayFlows, DayRecord, PriceBook, ResourceCounts, ResourceStats};
pub use trade::{TradeBook, TradeDirection, TradeOrder};

use crate::defs::{Cost, Defs, ResourceAmount, ResourceId};

/// Amount of every resource, indexed by [`ResourceId::index`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stock {
    amounts: Vec<u32>,
}

impl Stock {
    /// Empty stock for all resources of `defs`.
    pub fn new(defs: &Defs) -> Self {
        Self { amounts: vec![0; defs.resources().len()] }
    }

    /// Number of resource slots (equals the number of resource definitions).
    pub fn slot_count(&self) -> usize {
        self.amounts.len()
    }

    /// Units of `resource` in stock.
    pub fn get(&self, resource: ResourceId) -> u32 {
        self.amounts.get(resource.index()).copied().unwrap_or(0)
    }

    /// Iterates `(resource, amount)` for every resource with a positive amount, in id order.
    pub fn iter(&self) -> impl Iterator<Item = ResourceAmount> + '_ {
        self.amounts.iter().enumerate().filter(|(_, a)| **a > 0).filter_map(|(i, &amount)| {
            ResourceId::from_index(i).map(|resource| ResourceAmount { resource, amount })
        })
    }

    /// Sum of all units.
    pub fn total(&self) -> u64 {
        self.amounts.iter().map(|a| u64::from(*a)).sum()
    }

    /// True if at least `amount` units of `resource` are available.
    pub fn has(&self, resource: ResourceId, amount: u32) -> bool {
        self.get(resource) >= amount
    }

    /// Adds units (saturating at `u32::MAX`). Unknown resources are ignored.
    pub(crate) fn add(&mut self, resource: ResourceId, amount: u32) {
        if let Some(slot) = self.amounts.get_mut(resource.index()) {
            *slot = slot.saturating_add(amount);
        }
    }

    /// The amounts as per-resource counts (for statistics snapshots).
    pub(crate) fn to_counts(&self, defs: &Defs) -> ResourceCounts {
        let mut counts = ResourceCounts::new(defs);
        for m in self.iter() {
            counts.set(m.resource, m.amount);
        }
        counts
    }
}

/// A missing part of a cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shortfall {
    /// Not enough gold in the treasury.
    Gold {
        /// Required gold.
        needed: u32,
        /// Gold in the treasury (may be negative after debts).
        available: i64,
    },
    /// Not enough of a material available (stock minus material committed to sites).
    Material {
        /// The resource.
        resource: ResourceId,
        /// Required units.
        needed: u32,
        /// Units available.
        available: u32,
    },
}

/// Treasury, kingdom stock view, ledger, prices, statistics and trade orders.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Economy {
    treasury_milli: i64,
    stock: Stock,
    capacity: Stock,
    committed: Stock,
    ledger: Ledger,
    prices: PriceBook,
    stats: ResourceStats,
    trade: TradeBook,
    bankrupt: bool,
    /// Accrual remainders per ledger category in 1/24 milli-gold (hourly booking of daily rates).
    carry: [i64; LedgerCategory::ALL.len()],
    /// The spell of a shrinking treasury: warned once when it begins (docs/game_design.md §2
    /// Meldungen), the tick it was last seen.
    #[serde(default)]
    low_treasury: crate::notices::Spells<u8>,
}

impl Economy {
    /// Empty treasury and stock; prices at their base values.
    pub fn new(defs: &Defs) -> Self {
        Self {
            treasury_milli: 0,
            stock: Stock::new(defs),
            capacity: Stock::new(defs),
            committed: Stock::new(defs),
            ledger: Ledger::default(),
            prices: PriceBook::new(defs),
            stats: ResourceStats::new(defs),
            trade: TradeBook::default(),
            bankrupt: false,
            carry: [0; LedgerCategory::ALL.len()],
            low_treasury: crate::notices::Spells::default(),
        }
    }

    /// Gold in the treasury, rounded down (negative = debt).
    pub fn treasury(&self) -> i64 {
        self.treasury_milli.div_euclid(MILLI_PER_GOLD)
    }

    /// Gold in the treasury in milli-gold.
    pub fn treasury_milli(&self) -> i64 {
        self.treasury_milli
    }

    /// The kingdom stock: goods in all reachable, operational storage buildings.
    pub fn stock(&self) -> &Stock {
        &self.stock
    }

    /// Total storage capacity per resource of the reachable storage buildings.
    pub fn capacity(&self) -> &Stock {
        &self.capacity
    }

    /// Construction material still owed to construction sites and upgrades (not yet delivered
    /// or on its way). It stays in the stock until carriers pick it up, but is not available
    /// for new projects.
    pub fn committed(&self) -> &Stock {
        &self.committed
    }

    /// Units of `resource` available for new projects (stock minus committed material).
    pub fn available(&self, resource: ResourceId) -> u32 {
        self.stock.get(resource).saturating_sub(self.committed.get(resource))
    }

    /// True while the treasury is negative (no new construction, slower production).
    pub fn is_bankrupt(&self) -> bool {
        self.bankrupt
    }

    /// Daily money ledger.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Current market prices.
    pub fn prices(&self) -> &PriceBook {
        &self.prices
    }

    /// Production, consumption and trade statistics.
    pub fn stats(&self) -> &ResourceStats {
        &self.stats
    }

    /// Trade orders and today's traded volumes.
    pub fn trade(&self) -> &TradeBook {
        &self.trade
    }

    /// Everything missing to pay `cost` (empty if affordable). Gold comes first, then materials
    /// in resource id order; materials are compared with [`Economy::available`].
    pub fn shortfalls(&self, cost: &Cost) -> Vec<Shortfall> {
        let mut out = Vec::new();
        if cost.gold > 0 && i64::from(cost.gold) > self.treasury() {
            out.push(Shortfall::Gold { needed: cost.gold, available: self.treasury() });
        }
        for m in &cost.materials {
            let available = self.available(m.resource);
            if available < m.amount {
                out.push(Shortfall::Material { resource: m.resource, needed: m.amount, available });
            }
        }
        out
    }

    /// True if `cost` can be paid.
    pub fn can_afford(&self, cost: &Cost) -> bool {
        self.shortfalls(cost).is_empty()
    }

    /// Adds (or with a negative value removes) whole gold without a ledger entry (scenario
    /// start, tests).
    pub(crate) fn add_gold(&mut self, gold: i64) {
        self.treasury_milli = self.treasury_milli.saturating_add(gold.saturating_mul(MILLI_PER_GOLD));
    }

    /// Books a signed amount of milli-gold on `category` and applies it to the treasury.
    pub(crate) fn book_milli(&mut self, category: LedgerCategory, milli: i64) {
        if milli == 0 {
            return;
        }
        self.treasury_milli = self.treasury_milli.saturating_add(milli);
        self.ledger.book(category, milli);
    }

    /// Books a signed amount of whole gold on `category`.
    pub(crate) fn book_gold(&mut self, category: LedgerCategory, gold: i64) {
        self.book_milli(category, gold.saturating_mul(MILLI_PER_GOLD));
    }

    /// Pays `gold` for `category` if the treasury covers it.
    pub(crate) fn pay_gold(&mut self, category: LedgerCategory, gold: u32) -> Result<(), Shortfall> {
        if gold > 0 && i64::from(gold) > self.treasury() {
            return Err(Shortfall::Gold { needed: gold, available: self.treasury() });
        }
        self.book_gold(category, -i64::from(gold));
        Ok(())
    }

    /// Books one hour of a daily rate (`daily_milli` per day) on `category`, carrying the
    /// remainder so that 24 hourly bookings add up to the daily amount exactly.
    pub(crate) fn accrue_hour(&mut self, category: LedgerCategory, daily_milli: i64) {
        let carry = &mut self.carry[category.index()];
        *carry = carry.saturating_add(daily_milli);
        let hourly = carry.div_euclid(24);
        *carry = carry.rem_euclid(24);
        self.book_milli(category, hourly);
    }

    pub(crate) fn set_bankrupt(&mut self, bankrupt: bool) {
        self.bankrupt = bankrupt;
    }

    /// The treasury runs out soon at `now`: true if this begins a new spell (the warning is
    /// given once per spell, `notices.waiting_resolved_hours`).
    pub(crate) fn low_treasury_begins(&mut self, now: u64, gap: u64) -> bool {
        self.low_treasury.begins(0, now, gap)
    }

    /// Error if the treasury warning was given in the future (a crafted save).
    pub(crate) fn validate_notices(&self, now: u64) -> Result<(), String> {
        self.low_treasury.validate(now, "treasury notices")
    }

    pub(crate) fn ledger_mut(&mut self) -> &mut Ledger {
        &mut self.ledger
    }

    pub(crate) fn prices_mut(&mut self) -> &mut PriceBook {
        &mut self.prices
    }

    pub(crate) fn stats_mut(&mut self) -> &mut ResourceStats {
        &mut self.stats
    }

    pub(crate) fn trade_mut(&mut self) -> &mut TradeBook {
        &mut self.trade
    }

    /// Replaces the cached views (see [`storage::refresh_stock`]).
    pub(crate) fn set_views(&mut self, stock: Stock, capacity: Stock, committed: Stock) {
        self.stock = stock;
        self.capacity = capacity;
        self.committed = committed;
    }

    /// Records produced units in today's statistics.
    pub(crate) fn record_produced(&mut self, resource: ResourceId, amount: u32) {
        self.stats.today_mut().produced.add(resource, amount);
    }

    /// Records consumed units in today's statistics.
    pub(crate) fn record_consumed(&mut self, resource: ResourceId, amount: u32) {
        self.stats.today_mut().consumed.add(resource, amount);
    }

    /// Records units households took from the storages (consumption by households).
    pub(crate) fn record_household_use(&mut self, resource: ResourceId, amount: u32) {
        let today = self.stats.today_mut();
        today.consumed.add(resource, amount);
        today.household.add(resource, amount);
    }

    /// Checks internal sizes against the data (used when validating a loaded state).
    pub(crate) fn is_consistent(&self, defs: &Defs) -> bool {
        let n = defs.resources().len();
        self.stock.slot_count() == n
            && self.capacity.slot_count() == n
            && self.committed.slot_count() == n
            && self.prices.is_consistent(n)
            && self.stats.is_consistent(n)
            && self.ledger.is_consistent()
            && self.trade.is_consistent(defs)
            && self.carry.iter().all(|c| (0..24).contains(c))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::test_support::defs;

    fn res(key: &str) -> ResourceId {
        defs().resource_id(key).unwrap()
    }

    #[test]
    fn stock_add_and_iterate() {
        let mut s = Stock::new(defs());
        assert_eq!(s.slot_count(), defs().resources().len());
        s.add(res("wood"), 10);
        s.add(res("stone"), 3);
        assert_eq!(s.get(res("wood")), 10);
        assert!(s.has(res("stone"), 3) && !s.has(res("stone"), 4));
        assert_eq!(s.total(), 13);
        assert_eq!(s.iter().count(), 2);
        assert_eq!(s.to_counts(defs()).get(res("stone")), 3);
    }

    #[test]
    fn shortfalls_respect_committed_material() {
        let mut e = Economy::new(defs());
        e.add_gold(25);
        let mut stock = Stock::new(defs());
        stock.add(res("wood"), 20);
        let mut committed = Stock::new(defs());
        committed.add(res("wood"), 10);
        e.set_views(stock, Stock::new(defs()), committed);
        assert_eq!(e.available(res("wood")), 10);
        let cost = Cost { gold: 20, materials: vec![ResourceAmount { resource: res("wood"), amount: 12 }] };
        assert_eq!(e.shortfalls(&cost), vec![Shortfall::Material { resource: res("wood"), needed: 12, available: 10 }]);
        let cheap = Cost { gold: 30, materials: vec![ResourceAmount { resource: res("wood"), amount: 10 }] };
        assert_eq!(e.shortfalls(&cheap), vec![Shortfall::Gold { needed: 30, available: 25 }]);
        assert!(!e.can_afford(&cheap));
    }

    #[test]
    fn money_is_booked_per_category() {
        let mut e = Economy::new(defs());
        e.add_gold(100);
        e.pay_gold(LedgerCategory::Construction, 30).unwrap();
        assert_eq!(e.pay_gold(LedgerCategory::Construction, 80), Err(Shortfall::Gold { needed: 80, available: 70 }));
        e.book_gold(LedgerCategory::Construction, 5);
        assert_eq!(e.treasury(), 75);
        assert_eq!(e.ledger().today().gold(LedgerCategory::Construction), -25);
        // 24 hourly bookings of a daily rate add up exactly.
        for _ in 0..24 {
            e.accrue_hour(LedgerCategory::Wages, -7_201);
        }
        assert_eq!(e.ledger().today().milli(LedgerCategory::Wages), -7_201);
        assert_eq!(e.treasury_milli(), 75_000 - 7_201);
        assert_eq!(e.treasury(), 67, "rounded down");
        assert!(e.is_consistent(defs()));
    }
}
