//! Daily money ledger by category plus a rolling history for the finance window and charts.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Days kept in the rolling histories (ledger, prices, resource statistics).
pub const HISTORY_DAYS: usize = 48;

/// Milli-gold per gold: money is booked in thousandths so hourly accruals stay exact.
pub const MILLI_PER_GOLD: i64 = 1000;

/// Category of a money flow (docs/game_design.md §7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LedgerCategory {
    /// Income: taxes from households.
    Taxes,
    /// Income: fees of market activity.
    MarketFees,
    /// Income: goods sold to other kingdoms / the world market.
    Exports,
    /// Expense: wages of workers and builders.
    Wages,
    /// Expense: building upkeep.
    Upkeep,
    /// Expense: goods bought from other kingdoms / the world market.
    Imports,
    /// Expense: construction, upgrades and roads (refunds are booked here as income).
    Construction,
    /// Expense: soldiers' pay (military phase).
    Military,
    /// Expense: research costs (paid when a technology starts).
    Research,
    /// Income or expense: consequences of events (purchases, sales, ransom, theft).
    Events,
    /// Income or expense: gifts, tribute and peace payments between kingdoms.
    Diplomacy,
}

impl LedgerCategory {
    /// All categories in display order (income first).
    pub const ALL: [LedgerCategory; 11] = [
        LedgerCategory::Taxes,
        LedgerCategory::MarketFees,
        LedgerCategory::Exports,
        LedgerCategory::Wages,
        LedgerCategory::Upkeep,
        LedgerCategory::Imports,
        LedgerCategory::Construction,
        LedgerCategory::Military,
        LedgerCategory::Research,
        LedgerCategory::Events,
        LedgerCategory::Diplomacy,
    ];

    /// Position in [`LedgerCategory::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// True for categories that normally bring money in.
    pub const fn is_income(self) -> bool {
        matches!(self, LedgerCategory::Taxes | LedgerCategory::MarketFees | LedgerCategory::Exports)
    }

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            LedgerCategory::Taxes => "Steuern",
            LedgerCategory::MarketFees => "Marktgebühren",
            LedgerCategory::Exports => "Exporte",
            LedgerCategory::Wages => "Löhne",
            LedgerCategory::Upkeep => "Unterhalt",
            LedgerCategory::Imports => "Importe",
            LedgerCategory::Construction => "Bau",
            LedgerCategory::Military => "Militär",
            LedgerCategory::Research => "Forschung",
            LedgerCategory::Events => "Ereignisse",
            LedgerCategory::Diplomacy => "Diplomatie",
        }
    }
}

/// Money flows of one day in milli-gold: positive = income, negative = expense.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayLedger {
    /// Day number since the start of the game (0-based, see `GameClock::total_days`).
    pub day: u64,
    amounts: [i64; LedgerCategory::ALL.len()],
}

impl DayLedger {
    /// Empty ledger for `day`.
    pub fn new(day: u64) -> Self {
        Self { day, amounts: [0; LedgerCategory::ALL.len()] }
    }

    /// Net flow of a category in milli-gold.
    pub fn milli(&self, category: LedgerCategory) -> i64 {
        self.amounts[category.index()]
    }

    /// Net flow of a category in whole gold (rounded towards zero).
    pub fn gold(&self, category: LedgerCategory) -> i64 {
        self.milli(category) / MILLI_PER_GOLD
    }

    /// Sum of all positive flows in milli-gold.
    pub fn income_milli(&self) -> i64 {
        self.amounts.iter().filter(|a| **a > 0).sum()
    }

    /// Sum of all negative flows in milli-gold (a negative number).
    pub fn expenses_milli(&self) -> i64 {
        self.amounts.iter().filter(|a| **a < 0).sum()
    }

    /// Balance of the day in milli-gold.
    pub fn net_milli(&self) -> i64 {
        self.amounts.iter().sum()
    }

    /// Books `milli` (signed) on `category`.
    pub(crate) fn book(&mut self, category: LedgerCategory, milli: i64) {
        let slot = &mut self.amounts[category.index()];
        *slot = slot.saturating_add(milli);
    }
}

/// Today's ledger plus the closed days (oldest first, at most [`HISTORY_DAYS`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    today: DayLedger,
    history: VecDeque<DayLedger>,
}

impl Ledger {
    /// Flows of the running day.
    pub fn today(&self) -> &DayLedger {
        &self.today
    }

    /// The last closed day.
    pub fn yesterday(&self) -> Option<&DayLedger> {
        self.history.back()
    }

    /// Closed days, oldest first.
    pub fn history(&self) -> impl DoubleEndedIterator<Item = &DayLedger> + ExactSizeIterator + '_ {
        self.history.iter()
    }

    pub(crate) fn book(&mut self, category: LedgerCategory, milli: i64) {
        self.today.book(category, milli);
    }

    /// Closes the running day and starts `next_day`; returns the closed ledger.
    pub(crate) fn close_day(&mut self, next_day: u64) -> DayLedger {
        let closed = std::mem::replace(&mut self.today, DayLedger::new(next_day));
        self.history.push_back(closed);
        while self.history.len() > HISTORY_DAYS {
            self.history.pop_front();
        }
        closed
    }

    pub(crate) fn is_consistent(&self) -> bool {
        self.history.len() <= HISTORY_DAYS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn booking_and_totals() {
        let mut d = DayLedger::new(3);
        d.book(LedgerCategory::Taxes, 12_500);
        d.book(LedgerCategory::Wages, -7_200);
        d.book(LedgerCategory::Construction, -20_000);
        d.book(LedgerCategory::Construction, 5_000);
        assert_eq!(d.gold(LedgerCategory::Taxes), 12);
        assert_eq!(d.milli(LedgerCategory::Construction), -15_000);
        assert_eq!((d.income_milli(), d.expenses_milli(), d.net_milli()), (12_500, -22_200, -9_700));
        assert!(LedgerCategory::Exports.is_income() && !LedgerCategory::Imports.is_income());
        assert_eq!(LedgerCategory::ALL.map(LedgerCategory::index), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(LedgerCategory::Research.display_name(), "Forschung");
        assert!(!LedgerCategory::Events.is_income());
        assert_eq!(LedgerCategory::MarketFees.display_name(), "Marktgebühren");
    }

    #[test]
    fn history_is_rolling() {
        let mut l = Ledger::default();
        for day in 0..60u64 {
            l.book(LedgerCategory::Taxes, day as i64);
            let closed = l.close_day(day + 1);
            assert_eq!(closed.day, day);
        }
        assert_eq!(l.history().len(), HISTORY_DAYS);
        assert_eq!(l.history().next().map(|d| d.day), Some(12));
        assert_eq!(l.yesterday().map(|d| d.milli(LedgerCategory::Taxes)), Some(59));
        assert_eq!(l.today().day, 60);
        assert!(l.is_consistent());
    }
}
