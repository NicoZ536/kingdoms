//! Measured output of production buildings (docs/game_design.md §6.3 „gemessen“): the work
//! cycles completed and the goods made today and on each of the last closed days (one year).
//! Field farms count every harvested field tile as one cycle. The record is game state (saved
//! and validated) but no rule reads it; it feeds [`crate::report::building_economics`].

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::defs::{ResourceAmount, ResourceId};
use crate::time::DAYS_PER_YEAR;

/// Closed days a building remembers (one year: a field farm's harvests average over all seasons).
pub const OUTPUT_HISTORY_DAYS: usize = DAYS_PER_YEAR as usize;

/// What a building made in one day.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayOutput {
    /// Completed work cycles (field farms: harvested field tiles).
    pub cycles: u32,
    /// Whole units made, by resource in id order (no zero amounts).
    pub goods: Vec<ResourceAmount>,
}

impl DayOutput {
    /// Units of `resource` made.
    pub fn units_of(&self, resource: ResourceId) -> u32 {
        self.goods.iter().find(|m| m.resource == resource).map_or(0, |m| m.amount)
    }

    /// Units of all goods made.
    pub fn units(&self) -> u32 {
        self.goods.iter().fold(0u32, |sum, m| sum.saturating_add(m.amount))
    }

    fn add(&mut self, resource: ResourceId, amount: u32) {
        if amount == 0 {
            return;
        }
        match self.goods.binary_search_by_key(&resource, |m| m.resource) {
            Ok(i) => self.goods[i].amount = self.goods[i].amount.saturating_add(amount),
            Err(i) => self.goods.insert(i, ResourceAmount { resource, amount }),
        }
    }

    fn is_canonical(&self, resources: usize) -> bool {
        self.goods.iter().all(|m| m.amount > 0 && m.resource.index() < resources)
            && self.goods.windows(2).all(|w| w[0].resource < w[1].resource)
    }
}

/// Today's output and that of the last closed days of a production building.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputLog {
    today: DayOutput,
    /// Closed days, oldest first (at most [`OUTPUT_HISTORY_DAYS`]); only days the building
    /// stood finished count.
    days: VecDeque<DayOutput>,
}

impl OutputLog {
    /// The running day so far.
    pub fn today(&self) -> &DayOutput {
        &self.today
    }

    /// The last closed day (`None` before the building's first midnight).
    pub fn yesterday(&self) -> Option<&DayOutput> {
        self.days.back()
    }

    /// Closed days, oldest first.
    pub fn days(&self) -> impl DoubleEndedIterator<Item = &DayOutput> + ExactSizeIterator + '_ {
        self.days.iter()
    }

    /// Records a completed work cycle and the goods it yielded.
    pub(crate) fn record_cycle(&mut self, goods: &[ResourceAmount]) {
        self.today.cycles = self.today.cycles.saturating_add(1);
        for m in goods {
            self.today.add(m.resource, m.amount);
        }
    }

    /// Records the harvest of one field tile.
    pub(crate) fn record_harvest(&mut self, resource: ResourceId, amount: u32) {
        self.today.cycles = self.today.cycles.saturating_add(1);
        self.today.add(resource, amount);
    }

    /// Closes the day at midnight: today becomes the newest closed day.
    pub(crate) fn close_day(&mut self) {
        if self.days.len() >= OUTPUT_HISTORY_DAYS {
            self.days.pop_front();
        }
        self.days.push_back(std::mem::take(&mut self.today));
    }

    /// Checks bounds and resource ids (used when validating a loaded state).
    pub(crate) fn validate(&self, resources: usize) -> Result<(), String> {
        if self.days.len() > OUTPUT_HISTORY_DAYS {
            return Err("output history too long".into());
        }
        if !self.today.is_canonical(resources) || !self.days.iter().all(|d| d.is_canonical(resources)) {
            return Err("malformed output record".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(i: usize) -> ResourceId {
        ResourceId::from_index(i).unwrap()
    }

    #[test]
    fn cycles_and_goods_are_counted_per_day_for_a_year() {
        let mut log = OutputLog::default();
        assert!(log.yesterday().is_none());
        log.record_cycle(&[ResourceAmount { resource: r(3), amount: 2 }, ResourceAmount { resource: r(1), amount: 1 }]);
        log.record_cycle(&[ResourceAmount { resource: r(3), amount: 2 }]);
        log.record_cycle(&[]);
        assert_eq!(log.today().cycles, 3);
        assert_eq!(log.today().units_of(r(3)), 4);
        assert_eq!(log.today().units(), 5);
        assert_eq!(log.today().goods.first().map(|m| m.resource), Some(r(1)), "sorted by resource");
        log.close_day();
        assert_eq!(log.yesterday().map(|d| d.cycles), Some(3));
        assert_eq!(log.today(), &DayOutput::default());
        log.record_harvest(r(2), 7);
        log.record_harvest(r(2), 0);
        assert_eq!((log.today().cycles, log.today().units()), (2, 7));
        assert_eq!(log.validate(4), Ok(()));
        assert!(log.validate(3).is_err(), "unknown resource");
        for _ in 0..2 * OUTPUT_HISTORY_DAYS {
            log.close_day();
        }
        assert_eq!(log.days().len(), OUTPUT_HISTORY_DAYS);
        let mut broken = log.clone();
        broken.today.goods = vec![ResourceAmount { resource: r(2), amount: 0 }];
        assert!(broken.validate(4).is_err(), "zero amounts are not kept");
        let mut broken = log.clone();
        broken.days.push_back(DayOutput::default());
        assert!(broken.validate(4).is_err(), "bounded");
    }
}
