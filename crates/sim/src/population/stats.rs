//! Daily population counters and their rolling history (closed at midnight by the statistics
//! step, docs/architecture.md §3.3).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::economy::HISTORY_DAYS;

use super::{DeathCause, LifeStage, Population};

/// Highest plausible value of a daily counter (a loaded save with more is rejected, so sums
/// over the whole history stay far from overflowing).
pub const MAX_DAY_COUNT: u32 = 1_000_000;

/// Births, deaths, arrivals and departures of one day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayCounts {
    /// Children born.
    pub births: u32,
    /// Deaths per cause, indexed by [`DeathCause::index`].
    pub deaths: [u32; DeathCause::ALL.len()],
    /// Settlers who arrived.
    pub immigrants: u32,
    /// Citizens who left the kingdom.
    pub emigrants: u32,
}

impl DayCounts {
    /// All deaths.
    pub fn total_deaths(&self) -> u32 {
        self.deaths.iter().fold(0, |sum, n| sum.saturating_add(*n))
    }

    /// Every counter at most [`MAX_DAY_COUNT`].
    fn is_plausible(&self) -> bool {
        [self.births, self.immigrants, self.emigrants].iter().chain(&self.deaths).all(|n| *n <= MAX_DAY_COUNT)
    }

    /// Deaths of one cause.
    pub fn deaths_by(&self, cause: DeathCause) -> u32 {
        self.deaths[cause.index()]
    }
}

/// A closed day: the counters plus a snapshot of the population at midnight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopulationDay {
    /// Day number since the start of the game (0-based).
    pub day: u64,
    /// The day's counters.
    pub counts: DayCounts,
    /// Citizens at the end of the day.
    pub population: u32,
    /// Children, adults and elders, indexed like [`LifeStage::ALL`].
    pub by_stage: [u32; 3],
    /// Households.
    pub households: u32,
    /// Homeless citizens.
    pub homeless: u32,
    /// Adults of working age without a job.
    pub unemployed: u32,
    /// Average happiness in points.
    pub happiness: u8,
    /// Savings of all households in gold (the kingdom's private wealth).
    pub savings: i64,
}

/// Today's counters and the closed days (oldest first, at most [`HISTORY_DAYS`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopulationStats {
    pub(crate) today: DayCounts,
    history: VecDeque<PopulationDay>,
}

impl PopulationStats {
    /// Counters of the running day.
    pub fn today(&self) -> &DayCounts {
        &self.today
    }

    /// Closed days, oldest first.
    pub fn history(&self) -> impl DoubleEndedIterator<Item = &PopulationDay> + ExactSizeIterator + '_ {
        self.history.iter()
    }

    /// The last closed day.
    pub fn yesterday(&self) -> Option<&PopulationDay> {
        self.history.back()
    }

    /// History in order and within [`HISTORY_DAYS`], every counter at most
    /// [`MAX_DAY_COUNT`] (sums over the history cannot overflow).
    pub(crate) fn is_consistent(&self) -> bool {
        self.history.len() <= HISTORY_DAYS
            && self.history.iter().zip(self.history.iter().skip(1)).all(|(a, b)| a.day < b.day)
            && std::iter::once(&self.today).chain(self.history.iter().map(|d| &d.counts)).all(DayCounts::is_plausible)
            && self.history.iter().all(|d| d.savings >= 0)
    }
}

/// Closes day `day` with a snapshot of `population`; the counters restart at zero.
pub(crate) fn close_day(population: &mut Population, day: u64) {
    let mut by_stage = [0u32; 3];
    let mut unemployed = 0;
    for (_, c) in population.citizens() {
        let i = LifeStage::ALL.iter().position(|s| *s == c.stage).unwrap_or(0);
        by_stage[i] += 1;
        if c.can_take_job() && c.job.is_none() {
            unemployed += 1;
        }
    }
    let homeless = population.households().filter(|(_, h)| h.home.is_none()).map(|(_, h)| u32::from(h.size())).sum();
    let happiness = population.average_happiness().map_or(0, |h| h.round().clamp(0.0, 100.0) as u8);
    let savings = population.households().fold(0i64, |sum, (_, h)| sum.saturating_add(h.purse.savings_milli))
        / crate::economy::MILLI_PER_GOLD;
    let record = PopulationDay {
        day,
        counts: std::mem::take(&mut population.stats.today),
        population: u32::try_from(population.len()).unwrap_or(u32::MAX),
        by_stage,
        households: u32::try_from(population.households.len()).unwrap_or(u32::MAX),
        homeless,
        unemployed,
        happiness,
        savings,
    };
    population.stats.history.push_back(record);
    while population.stats.history.len() > HISTORY_DAYS {
        population.stats.history.pop_front();
    }
}
