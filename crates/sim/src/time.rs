//! The game calendar (docs/game_design.md §2).
//!
//! Simulation time advances only in fixed ticks. One tick is 18 game seconds; an hour has
//! [`TICKS_PER_HOUR`] ticks, a day [`TICKS_PER_DAY`]. A season lasts [`DAYS_PER_SEASON`] days and a
//! year has four seasons. Tick 0 is **Spring, day 1, year 1, 06:00**.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::event::{Notification, Severity, SimEvent};

/// Game seconds per tick.
pub const SECONDS_PER_TICK: u64 = 18;
/// Ticks per game hour.
pub const TICKS_PER_HOUR: u64 = 200;
/// Hours per game day.
pub const HOURS_PER_DAY: u64 = 24;
/// Ticks per game day.
pub const TICKS_PER_DAY: u64 = TICKS_PER_HOUR * HOURS_PER_DAY;
/// Days per season.
pub const DAYS_PER_SEASON: u64 = 6;
/// Seasons per year.
pub const SEASONS_PER_YEAR: u64 = 4;
/// Days per year.
pub const DAYS_PER_YEAR: u64 = DAYS_PER_SEASON * SEASONS_PER_YEAR;
/// Ticks per season.
pub const TICKS_PER_SEASON: u64 = TICKS_PER_DAY * DAYS_PER_SEASON;
/// Ticks per year.
pub const TICKS_PER_YEAR: u64 = TICKS_PER_DAY * DAYS_PER_YEAR;
/// Hour of day at which a new game starts.
pub const START_HOUR: u64 = 6;
/// Calendar offset of tick 0 (ticks since midnight of the first day).
const START_OFFSET: u64 = START_HOUR * TICKS_PER_HOUR;

const _: () = assert!(TICKS_PER_HOUR * SECONDS_PER_TICK == 3600, "an hour must be 3600 game seconds");

/// German date of a calendar day (0-based, like [`GameClock::total_days`]) as the top bar shows
/// it, e.g. `Sommer · Tag 3 · Jahr 1`.
pub fn day_date_string(day: u64) -> String {
    format!(
        "{} · Tag {} · Jahr {}",
        Season::from_index(day / DAYS_PER_SEASON).display_name(),
        day % DAYS_PER_SEASON + 1,
        day / DAYS_PER_YEAR + 1
    )
}

/// Converts a duration in game hours (as used in the data files) to whole ticks, rounding to the
/// nearest tick and never returning 0 for a positive duration.
pub fn hours_to_ticks(hours: f32) -> u64 {
    if hours <= 0.0 || hours.is_nan() {
        return 0;
    }
    ((f64::from(hours) * TICKS_PER_HOUR as f64).round() as u64).max(1)
}

/// The four seasons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Season {
    /// Sowing and growth (growth factor 1.0).
    Spring,
    /// Strong growth, drought risk.
    Summer,
    /// Harvest, slower growth.
    Autumn,
    /// No growth, heating demand.
    Winter,
}

impl Season {
    /// All seasons in calendar order.
    pub const ALL: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];

    /// Position in the year (Spring = 0).
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Season for a position in the year (wraps around).
    pub const fn from_index(index: u64) -> Season {
        Season::ALL[(index % SEASONS_PER_YEAR) as usize]
    }

    /// The following season.
    pub const fn next(self) -> Season {
        Season::from_index(self as u64 + 1)
    }

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            Season::Spring => "Frühling",
            Season::Summer => "Sommer",
            Season::Autumn => "Herbst",
            Season::Winter => "Winter",
        }
    }

    /// Lower-case key used in sprite names (`<tree>_<season>_<stage>`).
    pub const fn sprite_key(self) -> &'static str {
        match self {
            Season::Spring => "spring",
            Season::Summer => "summer",
            Season::Autumn => "autumn",
            Season::Winter => "winter",
        }
    }
}

impl fmt::Display for Season {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// The simulation clock: a tick counter with calendar accessors.
///
/// The `is_new_*` predicates are true on the *first tick* of an hour/day/season/year, i.e. right
/// after [`GameClock::advance`] crossed the boundary. Tick 0 (06:00 on the first day) is the
/// start of an hour but not of a day, season or year, because those began before the game did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GameClock {
    /// Ticks elapsed since the game started.
    pub tick: u64,
}

impl GameClock {
    /// Clock at the game start (Spring, day 1, year 1, 06:00).
    pub const fn new() -> Self {
        Self { tick: 0 }
    }

    /// Clock at an arbitrary tick.
    pub const fn at_tick(tick: u64) -> Self {
        Self { tick }
    }

    /// Clock at a calendar date. `year`, `day_of_season` are 1-based; `hour` 0–23, `minute` 0–59.
    /// Returns `None` for dates outside the calendar or before the game start.
    pub fn from_date(year: u64, season: Season, day_of_season: u64, hour: u64, minute: u64) -> Option<Self> {
        if year == 0 || day_of_season == 0 || day_of_season > DAYS_PER_SEASON || hour >= HOURS_PER_DAY || minute >= 60 {
            return None;
        }
        let days = (year - 1) * DAYS_PER_YEAR + season as u64 * DAYS_PER_SEASON + (day_of_season - 1);
        // Minutes map to ticks by flooring, consistent with `minute()`'s flooring.
        let within_hour = (minute * TICKS_PER_HOUR).div_ceil(60);
        let absolute = days * TICKS_PER_DAY + hour * TICKS_PER_HOUR + within_hour;
        absolute.checked_sub(START_OFFSET).map(Self::at_tick)
    }

    /// Advances the clock by one tick.
    pub fn advance(&mut self) {
        self.tick += 1;
    }

    /// Ticks since midnight of the first calendar day.
    const fn absolute(self) -> u64 {
        self.tick + START_OFFSET
    }

    /// Ticks since the most recent midnight.
    pub const fn tick_of_day(self) -> u64 {
        self.absolute() % TICKS_PER_DAY
    }

    /// Hour of day, 0–23.
    pub const fn hour(self) -> u64 {
        self.tick_of_day() / TICKS_PER_HOUR
    }

    /// Minute of the hour, 0–59.
    pub const fn minute(self) -> u64 {
        (self.tick_of_day() % TICKS_PER_HOUR) * 60 / TICKS_PER_HOUR
    }

    /// Whole hours since midnight of the first calendar day (6 at the start of a game).
    pub const fn total_hours(self) -> u64 {
        self.absolute() / TICKS_PER_HOUR
    }

    /// Whole calendar days completed since the game started (0 on the first day).
    pub const fn total_days(self) -> u64 {
        self.absolute() / TICKS_PER_DAY
    }

    /// Day within the current season, 1–6.
    pub const fn day_of_season(self) -> u64 {
        self.total_days() % DAYS_PER_SEASON + 1
    }

    /// Day within the current year, 1–24.
    pub const fn day_of_year(self) -> u64 {
        self.total_days() % DAYS_PER_YEAR + 1
    }

    /// Seasons completed since the game started (0 during the first spring). Citizens age one
    /// year per season (docs/game_design.md §2).
    pub const fn total_seasons(self) -> u64 {
        self.total_days() / DAYS_PER_SEASON
    }

    /// Current season.
    pub const fn season(self) -> Season {
        Season::from_index(self.total_seasons())
    }

    /// Current year, starting at 1.
    pub const fn year(self) -> u64 {
        self.total_days() / DAYS_PER_YEAR + 1
    }

    /// True on the first tick of an hour.
    pub const fn is_new_hour(self) -> bool {
        self.absolute() % TICKS_PER_HOUR == 0
    }

    /// True on the first tick of a day (midnight), never at tick 0.
    pub const fn is_new_day(self) -> bool {
        self.absolute() % TICKS_PER_DAY == 0
    }

    /// True on the first tick of a season.
    pub const fn is_new_season(self) -> bool {
        self.is_new_day() && self.total_days() % DAYS_PER_SEASON == 0
    }

    /// True on the first tick of a year.
    pub const fn is_new_year(self) -> bool {
        self.is_new_day() && self.total_days() % DAYS_PER_YEAR == 0
    }

    /// Position within the day, `0.0` (midnight) ..< `1.0`; 0.5 is noon. Drives day/night lighting.
    pub fn fraction_of_day(self) -> f32 {
        self.tick_of_day() as f32 / TICKS_PER_DAY as f32
    }

    /// Position within the season, `0.0` ..< `1.0` (e.g. for gradual foliage colour changes).
    pub fn fraction_of_season(self) -> f32 {
        (self.absolute() % TICKS_PER_SEASON) as f32 / TICKS_PER_SEASON as f32
    }

    /// German date string, e.g. `Frühling · Tag 3 · Jahr 1 · 07:30`.
    pub fn display(self) -> String {
        self.to_string()
    }

    /// German date without the time of day, e.g. `Frühling · Tag 3 · Jahr 1`.
    pub fn date_string(self) -> String {
        day_date_string(self.total_days())
    }

    /// Time of day as `HH:MM`.
    pub fn time_string(self) -> String {
        format!("{:02}:{:02}", self.hour(), self.minute())
    }
}

impl fmt::Display for GameClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} · Tag {} · Jahr {} · {:02}:{:02}",
            self.season().display_name(),
            self.day_of_season(),
            self.year(),
            self.hour(),
            self.minute()
        )
    }
}

/// Pipeline step 2 (calendar): advances the clock by one tick and reports calendar boundaries.
///
/// Emits [`SimEvent::DayChanged`], [`SimEvent::SeasonChanged`] and [`SimEvent::YearChanged`] plus
/// a German notification at the start of every season and year.
pub(crate) fn run_calendar(clock: &mut GameClock, events: &mut Vec<SimEvent>) {
    clock.advance();
    if !clock.is_new_day() {
        return;
    }
    let (season, year) = (clock.season(), clock.year());
    events.push(SimEvent::DayChanged { day_of_season: clock.day_of_season(), season, year });
    if clock.is_new_year() {
        events.push(SimEvent::YearChanged { year });
        events.push(SimEvent::Notification(Notification::new(
            Severity::Good,
            format!("Ein neues Jahr beginnt: Jahr {year}."),
        )));
    }
    if clock.is_new_season() {
        events.push(SimEvent::SeasonChanged { season, year });
        let text = match season {
            Season::Spring => "Der Frühling beginnt – Zeit für die Aussaat.",
            Season::Summer => "Der Sommer beginnt. Achtet auf Dürre!",
            Season::Autumn => "Der Herbst beginnt – die Erntezeit ist da.",
            Season::Winter => "Der Winter beginnt. Sorgt für Brennholz und Vorräte!",
        };
        events.push(SimEvent::Notification(Notification::new(Severity::Info, text)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_is_spring_day_one_six_oclock() {
        let c = GameClock::new();
        assert_eq!(c.season(), Season::Spring);
        assert_eq!(c.day_of_season(), 1);
        assert_eq!(c.day_of_year(), 1);
        assert_eq!(c.year(), 1);
        assert_eq!((c.hour(), c.minute()), (6, 0));
        assert_eq!(c.total_days(), 0);
        assert!(c.is_new_hour());
        assert!(!c.is_new_day() && !c.is_new_season() && !c.is_new_year());
        assert_eq!(c.display(), "Frühling · Tag 1 · Jahr 1 · 06:00");
        assert!((c.fraction_of_day() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn minutes_and_hours() {
        // 1 tick = 18 s; 10 ticks = 3 minutes.
        let c = GameClock::at_tick(10);
        assert_eq!((c.hour(), c.minute()), (6, 3));
        let c = GameClock::at_tick(TICKS_PER_HOUR + TICKS_PER_HOUR / 2);
        assert_eq!(c.time_string(), "07:30");
        assert_eq!(c.to_string(), "Frühling · Tag 1 · Jahr 1 · 07:30");
        let last = GameClock::at_tick(TICKS_PER_HOUR - 1);
        assert_eq!((last.hour(), last.minute()), (6, 59));
        assert!(!last.is_new_hour());
        assert!(GameClock::at_tick(TICKS_PER_HOUR).is_new_hour());
        assert_eq!(GameClock::new().total_hours(), 6);
        assert_eq!(last.total_hours(), 6);
        assert_eq!(GameClock::at_tick(TICKS_PER_HOUR).total_hours(), 7);
    }

    #[test]
    fn day_boundary_at_midnight() {
        let to_midnight = TICKS_PER_DAY - START_OFFSET;
        let before = GameClock::at_tick(to_midnight - 1);
        assert_eq!(before.time_string(), "23:59");
        assert_eq!(before.day_of_season(), 1);
        assert!(!before.is_new_day());
        let midnight = GameClock::at_tick(to_midnight);
        assert!(midnight.is_new_day() && midnight.is_new_hour());
        assert!(!midnight.is_new_season());
        assert_eq!(midnight.day_of_season(), 2);
        assert_eq!(midnight.total_days(), 1);
        assert_eq!(midnight.time_string(), "00:00");
        assert_eq!(midnight.fraction_of_day(), 0.0);
    }

    #[test]
    fn season_and_year_boundaries() {
        let summer = GameClock::from_date(1, Season::Summer, 1, 0, 0).unwrap();
        assert!(summer.is_new_season() && !summer.is_new_year());
        assert_eq!(summer.season(), Season::Summer);
        assert_eq!(summer.day_of_season(), 1);
        assert_eq!(summer.day_of_year(), 7);
        let prev = GameClock::at_tick(summer.tick - 1);
        assert_eq!(prev.season(), Season::Spring);
        assert_eq!(prev.day_of_season(), 6);

        let winter_end = GameClock::from_date(1, Season::Winter, 6, 23, 59).unwrap();
        assert_eq!(winter_end.year(), 1);
        assert_eq!(winter_end.day_of_year(), 24);
        let new_year = GameClock::at_tick(GameClock::from_date(2, Season::Spring, 1, 0, 0).unwrap().tick);
        assert!(new_year.is_new_year() && new_year.is_new_season() && new_year.is_new_day());
        assert_eq!(new_year.year(), 2);
        assert_eq!(new_year.season(), Season::Spring);
        assert_eq!(new_year.date_string(), "Frühling · Tag 1 · Jahr 2");
        assert_eq!(new_year.tick + START_OFFSET, TICKS_PER_YEAR);
    }

    #[test]
    fn from_date_roundtrips() {
        for (y, s, d, h, m) in
            [(1, Season::Spring, 1, 6, 0), (3, Season::Autumn, 4, 13, 27), (10, Season::Winter, 6, 23, 59)]
        {
            let c = GameClock::from_date(y, s, d, h, m).unwrap();
            assert_eq!((c.year(), c.season(), c.day_of_season(), c.hour(), c.minute()), (y, s, d, h, m));
        }
        assert!(GameClock::from_date(1, Season::Spring, 1, 5, 0).is_none(), "before game start");
        assert!(GameClock::from_date(0, Season::Spring, 1, 6, 0).is_none());
        assert!(GameClock::from_date(1, Season::Spring, 7, 6, 0).is_none());
        assert!(GameClock::from_date(1, Season::Spring, 1, 24, 0).is_none());
        assert!(GameClock::from_date(1, Season::Spring, 1, 6, 60).is_none());
    }

    #[test]
    fn every_minute_maps_back() {
        for minute in 0..60 {
            let c = GameClock::from_date(1, Season::Spring, 2, 8, minute).unwrap();
            assert_eq!(c.minute(), minute);
        }
    }

    #[test]
    fn season_helpers() {
        assert_eq!(Season::Winter.next(), Season::Spring);
        assert_eq!(Season::Spring.next(), Season::Summer);
        assert_eq!(Season::from_index(6), Season::Autumn);
        let names: Vec<_> = Season::ALL.iter().map(|s| s.to_string()).collect();
        assert_eq!(names, ["Frühling", "Sommer", "Herbst", "Winter"]);
    }

    #[test]
    fn hours_to_ticks_rounds() {
        assert_eq!(hours_to_ticks(2.0), 400);
        assert_eq!(hours_to_ticks(0.5), 100);
        assert_eq!(hours_to_ticks(0.0001), 1);
        assert_eq!(hours_to_ticks(0.0), 0);
        assert_eq!(hours_to_ticks(-1.0), 0);
        assert_eq!(hours_to_ticks(f32::NAN), 0);
    }

    #[test]
    fn calendar_step_reports_boundaries_once() {
        let mut clock = GameClock::new();
        let mut events = Vec::new();
        for _ in 0..TICKS_PER_YEAR {
            run_calendar(&mut clock, &mut events);
        }
        let days = events.iter().filter(|e| matches!(e, SimEvent::DayChanged { .. })).count();
        let seasons: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SimEvent::SeasonChanged { season, .. } => Some(*season),
                _ => None,
            })
            .collect();
        let years = events.iter().filter(|e| matches!(e, SimEvent::YearChanged { .. })).count();
        assert_eq!(days, DAYS_PER_YEAR as usize);
        assert_eq!(seasons, [Season::Summer, Season::Autumn, Season::Winter, Season::Spring]);
        assert_eq!(years, 1);
        assert_eq!(clock.tick, TICKS_PER_YEAR);
        assert_eq!(clock.year(), 2);
        assert_eq!(clock.hour(), 6);
    }
}
