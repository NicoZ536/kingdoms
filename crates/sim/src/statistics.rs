//! Pipeline step 10: statistics (docs/architecture.md §3.3). It runs last in every tick, after
//! all other steps have booked their results:
//! * every game hour an [`HourSample`] (treasury, kingdom stock, population, employment) is
//!   appended to a rolling hourly history ([`HOURLY_HISTORY_HOURS`]) for intra-day charts,
//! * every midnight the day that just ended is closed: daily ledger, price update, resource
//!   statistics, trade limits and the population counters (daily histories for charts live in
//!   [`Ledger`](crate::economy::Ledger), [`ResourceStats`](crate::economy::ResourceStats) and
//!   [`PopulationStats`](crate::population::PopulationStats)),
//! * at the end of every season a [`SeasonSample`] goes into the long-term history of the whole
//!   game ([`SeasonHistory`]: at most [`MAX_SEASON_SAMPLES`]; when it is full every second
//!   sample is dropped and from then on only every second season is kept, so the history
//!   always spans the whole game).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::defs::Defs;
use crate::economy::{MILLI_PER_GOLD, finance};
use crate::event::SimEvent;
use crate::state::GameState;
use crate::time::{DAYS_PER_SEASON, Season};

/// Hours kept in the hourly history (three game days).
pub const HOURLY_HISTORY_HOURS: usize = 72;

/// Season samples kept at most (then halved, see [`SeasonHistory`]); 400 seasons are 100
/// game years.
pub const MAX_SEASON_SAMPLES: usize = 400;

/// Largest sampling stride of the season history (2²⁰ seasons, far beyond any game).
pub const MAX_SEASON_STRIDE: u32 = 1 << 20;

/// Snapshot of the kingdom at the end of a season (long-term statistics, docs §13).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonSample {
    /// Seasons since the start of the game (0 = the first season); the sample closes it.
    pub season_index: u64,
    /// Inhabitants.
    pub population: u32,
    /// Children, adults and elders, indexed like [`LifeStage::ALL`](crate::population::LifeStage::ALL).
    pub by_stage: [u32; 3],
    /// Households.
    pub households: u32,
    /// Homeless citizens.
    pub homeless: u32,
    /// Inhabitants with a job.
    pub employed: u32,
    /// Average happiness in points.
    pub happiness: u8,
    /// Gold in the treasury (rounded down; negative = debt).
    pub treasury: i64,
    /// Gold booked as income during the season (closed days of the ledger).
    pub income: i64,
    /// Gold booked as expenses during the season (a positive number).
    pub expenses: i64,
    /// Savings of all households in gold.
    pub savings: i64,
    /// Kingdom stock per resource (resource id order).
    pub stock: Vec<u32>,
    /// Tiles of the territory.
    pub territory: u32,
    /// Finished buildings (walls and gates included).
    pub buildings: u32,
    /// Ruler rank (index into the ranks of the data).
    pub rank: u8,
}

impl SeasonSample {
    /// Game year of the season (1-based).
    pub fn year(&self) -> u64 {
        self.season_index / 4 + 1
    }

    /// The season.
    pub fn season(&self) -> Season {
        Season::ALL[(self.season_index % 4) as usize]
    }
}

/// Long-term history: one [`SeasonSample`] per `stride` seasons, oldest first (see the module
/// documentation).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonHistory {
    samples: Vec<SeasonSample>,
    stride: u32,
}

impl Default for SeasonHistory {
    fn default() -> Self {
        Self { samples: Vec::new(), stride: 1 }
    }
}

impl SeasonHistory {
    /// The samples, oldest first.
    pub fn samples(&self) -> &[SeasonSample] {
        &self.samples
    }

    /// Seasons between two samples (1 until the history was first halved).
    pub fn stride(&self) -> u32 {
        self.stride
    }

    /// Adds the sample of a closed season (seasons off the stride are skipped); halves the
    /// history when it outgrows [`MAX_SEASON_SAMPLES`].
    pub(crate) fn record(&mut self, sample: SeasonSample) {
        if sample.season_index % u64::from(self.stride) != 0
            || self.samples.last().is_some_and(|s| s.season_index >= sample.season_index)
        {
            return;
        }
        self.samples.push(sample);
        while self.samples.len() > MAX_SEASON_SAMPLES && self.stride < MAX_SEASON_STRIDE {
            self.stride *= 2;
            let stride = u64::from(self.stride);
            self.samples.retain(|s| s.season_index % stride == 0);
        }
    }

    /// True if the history is well-formed: bounded, strictly increasing seasons on the stride
    /// (a power of two), one stock value per resource, no negative flows or savings.
    pub(crate) fn is_consistent(&self, resources: usize) -> bool {
        self.stride.is_power_of_two()
            && self.stride <= MAX_SEASON_STRIDE
            && self.samples.len() <= MAX_SEASON_SAMPLES
            && self.samples.windows(2).all(|w| w[0].season_index < w[1].season_index)
            && self.samples.iter().all(|s| {
                s.season_index % u64::from(self.stride) == 0
                    && s.stock.len() == resources
                    && s.income >= 0
                    && s.expenses >= 0
                    && s.savings >= 0
            })
    }
}

/// Snapshot of the kingdom at the start of a game hour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HourSample {
    /// Whole hours since midnight of the first calendar day
    /// ([`GameClock::total_hours`](crate::time::GameClock::total_hours)).
    pub hour: u64,
    /// Gold in the treasury (rounded down; negative = debt).
    pub treasury: i64,
    /// Units of all resources in the kingdom stock.
    pub stock_total: u64,
    /// Inhabitants.
    pub population: u32,
    /// Inhabitants with a job (workers and builders).
    pub employed: u32,
}

/// Rolling hourly history of [`HourSample`]s (oldest first) and the long-term
/// [`SeasonHistory`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statistics {
    hourly: VecDeque<HourSample>,
    seasons: SeasonHistory,
}

impl Statistics {
    /// The long-term history (one sample per season, see [`SeasonHistory`]).
    pub fn seasons(&self) -> &SeasonHistory {
        &self.seasons
    }

    /// The hourly samples of the last [`HOURLY_HISTORY_HOURS`] hours, oldest first.
    pub fn hourly(&self) -> impl DoubleEndedIterator<Item = &HourSample> + ExactSizeIterator + '_ {
        self.hourly.iter()
    }

    /// The most recent sample.
    pub fn latest(&self) -> Option<&HourSample> {
        self.hourly.back()
    }

    /// Appends a sample, dropping the oldest beyond [`HOURLY_HISTORY_HOURS`].
    pub(crate) fn record(&mut self, sample: HourSample) {
        if self.hourly.len() == HOURLY_HISTORY_HOURS {
            self.hourly.pop_front();
        }
        self.hourly.push_back(sample);
    }

    /// True if the histories are well-formed (bounded, strictly increasing hours and seasons;
    /// see [`SeasonHistory::is_consistent`]); used when validating a loaded state.
    pub(crate) fn is_consistent(&self, resources: usize) -> bool {
        self.hourly.len() <= HOURLY_HISTORY_HOURS
            && self.hourly.iter().zip(self.hourly.iter().skip(1)).all(|(a, b)| a.hour < b.hour)
            && self.seasons.is_consistent(resources)
    }
}

/// The sample of the season that ends now (called at the first midnight of the next season,
/// after the day's closing).
fn season_sample(state: &GameState, season_index: u64) -> SeasonSample {
    let pop = &state.population;
    let gold = |milli: i64| milli / MILLI_PER_GOLD;
    let first_day = season_index * DAYS_PER_SEASON;
    let days = state.economy.ledger().history().filter(|d| d.day >= first_day && d.day < first_day + DAYS_PER_SEASON);
    let (income, expenses) = days.fold((0i64, 0i64), |(i, e), d| {
        (i.saturating_add(d.income_milli().max(0)), e.saturating_add(d.expenses_milli().min(0).saturating_neg()))
    });
    let mut by_stage = [0u32; 3];
    for (_, c) in pop.citizens() {
        let i = crate::population::LifeStage::ALL.iter().position(|s| *s == c.stage()).unwrap_or(0);
        by_stage[i] = by_stage[i].saturating_add(1);
    }
    let savings = pop.households().fold(0i64, |sum, (_, h)| sum.saturating_add(h.purse().savings_milli()));
    SeasonSample {
        season_index,
        population: u32::try_from(pop.len()).unwrap_or(u32::MAX),
        by_stage,
        households: u32::try_from(pop.households().count()).unwrap_or(u32::MAX),
        homeless: pop.households().filter(|(_, h)| h.home().is_none()).map(|(_, h)| u32::from(h.size())).sum(),
        employed: state.workforce.employed(),
        happiness: pop.average_happiness().map_or(0, |h| h.round().clamp(0.0, 100.0) as u8),
        treasury: state.economy.treasury(),
        income: gold(income),
        expenses: gold(expenses),
        savings: gold(savings),
        stock: (0..state.economy.stock().slot_count())
            .map(|i| crate::defs::ResourceId::from_index(i).map_or(0, |r| state.economy.stock().get(r)))
            .collect(),
        territory: state.territory.owned_tiles(),
        buildings: u32::try_from(state.buildings.iter().filter(|(_, b)| b.is_complete()).count()).unwrap_or(u32::MAX),
        rank: state.progress.rank(),
    }
}

/// Pipeline step 10: hourly sample and, at midnight, the daily closing.
pub(crate) fn run_statistics(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let clock = state.clock;
    if !clock.is_new_hour() {
        return;
    }
    let sample = HourSample {
        hour: clock.total_hours(),
        treasury: state.economy.treasury(),
        stock_total: state.economy.stock().total(),
        population: state.workforce.population(),
        employed: state.workforce.employed(),
    };
    state.statistics.record(sample);
    if clock.is_new_day() {
        finance::close_day(state, defs, clock.total_days(), events);
        crate::production::close_output_day(state);
        // The day that just ended (its number is one less than the new day's).
        crate::population::close_stats_day(&mut state.population, clock.total_days().saturating_sub(1));
        if clock.is_new_season() && clock.total_days() > 0 {
            let ended = clock.total_days() / DAYS_PER_SEASON - 1;
            let sample = season_sample(state, ended);
            state.statistics.seasons.record(sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::GameSettings;
    use crate::test_support::defs;
    use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
    use crate::world::WorldSize;

    fn sample(hour: u64) -> HourSample {
        HourSample { hour, treasury: 0, stock_total: 0, population: 0, employed: 0 }
    }

    #[test]
    fn history_is_bounded_and_ordered() {
        let mut s = Statistics::default();
        for h in 0..(HOURLY_HISTORY_HOURS as u64 + 5) {
            s.record(sample(h));
        }
        assert_eq!(s.hourly().len(), HOURLY_HISTORY_HOURS);
        assert_eq!(s.hourly().next().map(|x| x.hour), Some(5));
        assert_eq!(s.latest().map(|x| x.hour), Some(HOURLY_HISTORY_HOURS as u64 + 4));
        assert!(s.is_consistent(2));
        let mut broken = s.clone();
        broken.hourly.push_back(sample(0));
        assert!(!broken.is_consistent(2));
    }

    fn season(index: u64) -> SeasonSample {
        SeasonSample {
            season_index: index,
            population: 0,
            by_stage: [0; 3],
            households: 0,
            homeless: 0,
            employed: 0,
            happiness: 0,
            treasury: 0,
            income: 0,
            expenses: 0,
            savings: 0,
            stock: vec![0; 2],
            territory: 0,
            buildings: 0,
            rank: 0,
        }
    }

    #[test]
    fn the_season_history_spans_the_whole_game() {
        let mut h = SeasonHistory::default();
        for i in 0..MAX_SEASON_SAMPLES as u64 {
            h.record(season(i));
        }
        assert_eq!((h.samples().len(), h.stride()), (MAX_SEASON_SAMPLES, 1));
        // Full: every second sample goes, from now on every second season is kept.
        h.record(season(MAX_SEASON_SAMPLES as u64));
        assert_eq!(h.stride(), 2);
        assert_eq!(h.samples().len(), MAX_SEASON_SAMPLES / 2 + 1);
        assert_eq!(h.samples().first().map(|s| s.season_index), Some(0));
        assert_eq!(h.samples().last().map(|s| s.season_index), Some(MAX_SEASON_SAMPLES as u64));
        h.record(season(MAX_SEASON_SAMPLES as u64 + 1));
        assert_eq!(h.samples().len(), MAX_SEASON_SAMPLES / 2 + 1, "off the stride");
        assert!(h.is_consistent(2));
        for i in MAX_SEASON_SAMPLES as u64 + 2..3000 {
            h.record(season(i));
        }
        assert!(h.samples().len() <= MAX_SEASON_SAMPLES && h.is_consistent(2));
        assert_eq!(h.samples().first().map(|s| s.season_index), Some(0), "still from the first season");
        assert!(!h.is_consistent(3), "one stock value per resource");
        let mut broken = h.clone();
        broken.stride = 3;
        assert!(!broken.is_consistent(2));
        let mut broken = h.clone();
        broken.samples[1].expenses = -5;
        assert!(!broken.is_consistent(2));
        assert_eq!((season(5).year(), season(5).season()), (2, Season::Summer));
    }

    #[test]
    fn every_season_leaves_a_sample() {
        let d = defs();
        let mut state = GameState::new(d, GameSettings::new(d, 11, WorldSize::Small));
        let to_second_season = crate::time::TICKS_PER_SEASON - 6 * TICKS_PER_HOUR + 1;
        for _ in 0..to_second_season {
            state.tick(d, Vec::new());
        }
        let samples = state.statistics().seasons().samples();
        assert_eq!(samples.len(), 1, "the first season is closed");
        let s = &samples[0];
        assert_eq!((s.season_index, s.year(), s.season()), (0, 1, Season::Spring));
        assert_eq!(s.population, state.workforce().population());
        assert_eq!(s.treasury, state.economy().treasury());
        assert_eq!(s.stock.len(), d.resources().len());
        assert!(s.income > 0 && s.expenses > 0 && s.buildings > 0 && s.territory > 0, "{s:?}");
        let day = state.population().stats().yesterday().expect("closed day");
        assert!(day.savings > 0 && s.savings == day.savings);
        state.validate(d).unwrap();
    }

    #[test]
    fn samples_every_hour_and_closes_the_day_last() {
        let d = defs();
        let mut state = GameState::new(d, GameSettings::new(d, 11, WorldSize::Small));
        // From 06:00 to just after midnight: 18 hourly samples, then the daily closing.
        let to_midnight = TICKS_PER_DAY - 6 * TICKS_PER_HOUR;
        let mut events = Vec::new();
        for _ in 0..to_midnight {
            events = state.tick(d, Vec::new());
        }
        let samples: Vec<HourSample> = state.statistics().hourly().copied().collect();
        assert_eq!(samples.len(), 18);
        assert_eq!(samples.first().map(|s| s.hour), Some(7));
        assert_eq!(samples.last().map(|s| s.hour), Some(24));
        let last = samples[samples.len() - 1];
        assert_eq!(last.treasury, state.economy().treasury());
        assert_eq!(last.stock_total, state.economy().stock().total());
        assert_eq!(last.population, state.workforce().population());
        assert_eq!(last.employed, state.workforce().employed());
        // The daily balance is the last event of the midnight tick (step 10 runs last).
        assert!(matches!(events.last(), Some(SimEvent::DailyBalance { day: 0, .. })), "{events:?}");
        assert_eq!(state.economy().ledger().history().len(), 1);
        assert!(state.economy().stats().yesterday().is_some());
    }
}
