//! Resource statistics (production, consumption, trade per day) and dynamic market prices
//! (docs/game_design.md §7).
//!
//! Price formula: `p = base × clamp((demand + 1) / (supply + 1))^0.5`, smoothed once per day
//! towards that target. Supply = kingdom stock + production + imports of the closed day,
//! demand = consumption + exports of the closed day + outstanding construction material.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::ledger::HISTORY_DAYS;
use crate::defs::{Defs, GameRules, ResourceId};

/// Hundredths of gold per gold (prices are stored in centi-gold).
pub const CENTI_PER_GOLD: u32 = 100;

/// Per-resource amounts of one kind of flow, indexed by [`ResourceId::index`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCounts {
    values: Vec<u32>,
}

impl ResourceCounts {
    /// Zero for every resource of `defs`.
    pub fn new(defs: &Defs) -> Self {
        Self { values: vec![0; defs.resources().len()] }
    }

    /// Value for `resource` (0 for unknown resources).
    pub fn get(&self, resource: ResourceId) -> u32 {
        self.values.get(resource.index()).copied().unwrap_or(0)
    }

    /// Number of slots.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// True without slots.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub(crate) fn add(&mut self, resource: ResourceId, amount: u32) {
        if let Some(slot) = self.values.get_mut(resource.index()) {
            *slot = slot.saturating_add(amount);
        }
    }

    pub(crate) fn set(&mut self, resource: ResourceId, amount: u32) {
        if let Some(slot) = self.values.get_mut(resource.index()) {
            *slot = amount;
        }
    }
}

/// Goods flows of one day.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayFlows {
    /// Produced by buildings (incl. gathering).
    pub produced: ResourceCounts,
    /// Consumed by production, built into construction sites and used by households.
    pub consumed: ResourceCounts,
    /// Taken by households from the storages (part of `consumed`).
    pub household: ResourceCounts,
    /// Bought through trade.
    pub imported: ResourceCounts,
    /// Sold through trade.
    pub exported: ResourceCounts,
}

impl DayFlows {
    /// Zero flows for all resources.
    pub fn new(defs: &Defs) -> Self {
        Self {
            produced: ResourceCounts::new(defs),
            consumed: ResourceCounts::new(defs),
            household: ResourceCounts::new(defs),
            imported: ResourceCounts::new(defs),
            exported: ResourceCounts::new(defs),
        }
    }
}

/// A closed day: flows plus the end-of-day stock and prices.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayRecord {
    /// Day number since the start of the game (0-based).
    pub day: u64,
    /// Goods flows of the day.
    pub flows: DayFlows,
    /// Kingdom stock at the end of the day.
    pub stock: ResourceCounts,
    /// Prices (centi-gold) after the end-of-day update.
    pub price_centi: ResourceCounts,
}

/// Flows of the running day and the closed days (oldest first, at most [`HISTORY_DAYS`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceStats {
    today: DayFlows,
    history: VecDeque<DayRecord>,
}

impl ResourceStats {
    /// Empty statistics.
    pub fn new(defs: &Defs) -> Self {
        Self { today: DayFlows::new(defs), history: VecDeque::new() }
    }

    /// Flows of the running day.
    pub fn today(&self) -> &DayFlows {
        &self.today
    }

    /// The last closed day.
    pub fn yesterday(&self) -> Option<&DayRecord> {
        self.history.back()
    }

    /// Closed days, oldest first.
    pub fn history(&self) -> impl DoubleEndedIterator<Item = &DayRecord> + ExactSizeIterator + '_ {
        self.history.iter()
    }

    pub(crate) fn today_mut(&mut self) -> &mut DayFlows {
        &mut self.today
    }

    /// Closes the running day with the given snapshots; flows restart at zero.
    pub(crate) fn close_day(&mut self, defs: &Defs, day: u64, stock: ResourceCounts, price_centi: ResourceCounts) {
        let flows = std::mem::replace(&mut self.today, DayFlows::new(defs));
        self.history.push_back(DayRecord { day, flows, stock, price_centi });
        while self.history.len() > HISTORY_DAYS {
            self.history.pop_front();
        }
    }

    pub(crate) fn is_consistent(&self, resources: usize) -> bool {
        let flows_ok = |f: &DayFlows| {
            [&f.produced, &f.consumed, &f.household, &f.imported, &f.exported].iter().all(|c| c.len() == resources)
        };
        flows_ok(&self.today)
            && self.history.len() <= HISTORY_DAYS
            && self
                .history
                .iter()
                .all(|r| flows_ok(&r.flows) && r.stock.len() == resources && r.price_centi.len() == resources)
    }
}

/// Current market price of every resource in centi-gold.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceBook {
    centi: ResourceCounts,
}

impl PriceBook {
    /// All prices at their base price.
    pub fn new(defs: &Defs) -> Self {
        let mut centi = ResourceCounts::new(defs);
        for r in defs.resources() {
            centi.set(r.id, r.base_price.saturating_mul(CENTI_PER_GOLD));
        }
        Self { centi }
    }

    /// Price of one unit in centi-gold.
    pub fn centi(&self, resource: ResourceId) -> u32 {
        self.centi.get(resource)
    }

    /// Price of one unit in gold.
    pub fn gold(&self, resource: ResourceId) -> f32 {
        self.centi(resource) as f32 / CENTI_PER_GOLD as f32
    }

    /// All prices.
    pub fn all(&self) -> &ResourceCounts {
        &self.centi
    }

    pub(crate) fn set_centi(&mut self, resource: ResourceId, centi: u32) {
        self.centi.set(resource, centi);
    }

    pub(crate) fn is_consistent(&self, resources: usize) -> bool {
        self.centi.len() == resources
    }
}

/// Target price in centi-gold for the given daily supply and demand.
pub fn target_price_centi(base_price: u32, supply: u64, demand: u64, rules: &GameRules) -> u32 {
    let ratio = (demand as f64 + 1.0) / (supply as f64 + 1.0);
    let ratio = ratio.clamp(f64::from(rules.price_ratio_min), f64::from(rules.price_ratio_max));
    let centi = f64::from(base_price) * f64::from(CENTI_PER_GOLD) * ratio.sqrt();
    centi.round().clamp(1.0, f64::from(u32::MAX)) as u32
}

/// Moves `current` towards `target` by `percent` of the difference (at least one centi-gold,
/// so prices always converge).
pub fn smooth_price(current: u32, target: u32, percent: u8) -> u32 {
    let diff = i64::from(target) - i64::from(current);
    if diff == 0 {
        return current;
    }
    let mut step = diff * i64::from(percent.min(100)) / 100;
    if step == 0 {
        step = diff.signum();
    }
    (i64::from(current) + step).clamp(1, i64::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::defs;

    #[test]
    fn price_formula_follows_supply_and_demand() {
        let rules = defs().rules();
        // Balanced: base price.
        assert_eq!(target_price_centi(10, 50, 50, rules), 1000);
        // Shortage: demand 4x supply ⇒ price x2 (clamped at the maximum ratio).
        assert_eq!(target_price_centi(10, 0, 3, rules), 2000);
        assert_eq!(target_price_centi(10, 0, 500, rules), (1000.0 * f64::from(rules.price_ratio_max).sqrt()).round() as u32);
        // Oversupply: clamped at the minimum ratio.
        assert_eq!(target_price_centi(10, 1000, 0, rules), (1000.0 * f64::from(rules.price_ratio_min).sqrt()).round() as u32);
        assert!(target_price_centi(10, 100, 30, rules) < 1000);
        assert!(target_price_centi(10, 30, 100, rules) > 1000);
    }

    #[test]
    fn smoothing_converges() {
        assert_eq!(smooth_price(1000, 2000, 25), 1250);
        assert_eq!(smooth_price(1000, 500, 25), 875);
        assert_eq!(smooth_price(1000, 1001, 25), 1001, "small differences still move");
        assert_eq!(smooth_price(1000, 1000, 25), 1000);
        let mut p = 1000;
        for _ in 0..100 {
            p = smooth_price(p, 400, 25);
        }
        assert_eq!(p, 400);
    }

    #[test]
    fn stats_history_rolls() {
        let d = defs();
        let wood = d.resource_id("wood").unwrap();
        let mut s = ResourceStats::new(d);
        for day in 0..50 {
            s.today_mut().produced.add(wood, 3);
            s.close_day(d, day, ResourceCounts::new(d), PriceBook::new(d).all().clone());
        }
        assert_eq!(s.history().len(), HISTORY_DAYS);
        assert_eq!(s.yesterday().map(|r| (r.day, r.flows.produced.get(wood))), Some((49, 3)));
        assert_eq!(s.today().produced.get(wood), 0);
        assert!(s.is_consistent(d.resources().len()));
        let prices = PriceBook::new(d);
        assert_eq!(prices.centi(wood), 400);
        assert!((prices.gold(wood) - 4.0).abs() < 1e-6);
    }
}
