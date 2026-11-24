//! Research and event rules (`rules.ron` → `research`, `events`; docs/game_design.md §9, §12).

use serde::{Deserialize, Serialize};

use super::error::{Loc, Problems};
use crate::state::Difficulty;
use crate::time::HOURS_PER_DAY;

/// Research progress (docs/game_design.md §9).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchRules {
    /// Research points per game hour from a finished headquarters (independent of its staff);
    /// buildings with `research_points_per_hour` (schools) add their share.
    pub base_points_per_hour: f32,
    /// Most technologies in the research queue.
    pub max_queue: u8,
}

/// One value per difficulty level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifficultyValues<T> {
    /// Leicht.
    pub easy: T,
    /// Normal.
    pub normal: T,
    /// Schwer.
    pub hard: T,
}

impl<T: Copy> DifficultyValues<T> {
    /// Value for `difficulty`.
    pub fn get(&self, difficulty: Difficulty) -> T {
        match difficulty {
            Difficulty::Easy => self.easy,
            Difficulty::Normal => self.normal,
            Difficulty::Hard => self.hard,
        }
    }
}

/// Random events with choices (docs/game_design.md §12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRules {
    /// No event is drawn before this day (0-based day count of the game).
    pub first_day: u16,
    /// Hour of the day of the daily draw (the day's weather is known by then).
    pub check_hour: u8,
    /// Chance of an event on a day without a pending event, in percent (normal difficulty).
    pub daily_chance_percent: u8,
    /// The daily chance per difficulty, in percent of `daily_chance_percent`.
    pub difficulty_chance_percent: DifficultyValues<u16>,
    /// Days between two events at least (counted from the day the last one occurred).
    pub min_days_between: u16,
    /// Game hours the player has to decide; then the first choice applies.
    pub decision_hours: u16,
    /// Intensity of weather forced by an event when the effect gives none, in percent.
    pub weather_intensity: u8,
    /// A fire spreads to unprotected wooden buildings at most this many tiles from the burning
    /// one.
    pub fire_spread_radius: u8,
    /// Repairing a damaged building takes this share of its build time, in percent.
    pub repair_work_percent: u8,
    /// Repairing a damaged building needs this share of its material cost, in percent.
    pub repair_material_percent: u8,
    /// Share of the goods inside a burning building that are lost, in percent.
    pub fire_goods_loss_percent: u8,
    /// Share of the goods inside a flooded building that are lost, in percent.
    pub flood_goods_loss_percent: u8,
    /// Health points a citizen loses when falling ill in an epidemic (a hospital in reach
    /// reduces the loss to `population.hospital_disease_percent`).
    pub plague_health_loss: u8,
    /// Tiles of a newly discovered deposit (the centre and its four neighbours at most).
    pub deposit_tiles: u8,
    /// Resolved events kept in the history.
    pub history_length: u8,
}

impl EventRules {
    /// Ticks the player has to decide.
    pub fn decision_ticks(&self) -> u64 {
        u64::from(self.decision_hours) * crate::time::TICKS_PER_HOUR
    }

    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        p.check(u64::from(self.check_hour) < HOURS_PER_DAY, loc, || "events.check_hour must be 0..=23".into());
        p.check(self.daily_chance_percent <= 100, loc, || "events.daily_chance_percent must be <= 100".into());
        p.check(self.decision_hours > 0, loc, || "events.decision_hours must be positive".into());
        p.check((1..=100).contains(&self.weather_intensity), loc, || "events.weather_intensity must be 1..=100".into());
        let percents = [
            self.repair_work_percent,
            self.repair_material_percent,
            self.fire_goods_loss_percent,
            self.flood_goods_loss_percent,
            self.plague_health_loss,
        ];
        p.check(percents.iter().all(|v| *v <= 100), loc, || {
            "events: repair, goods loss and plague values must be <= 100".into()
        });
        p.check(self.repair_work_percent > 0, loc, || "events.repair_work_percent must be positive".into());
        p.check((1..=5).contains(&self.deposit_tiles), loc, || "events.deposit_tiles must be 1..=5".into());
        p.check(self.history_length > 0, loc, || "events.history_length must be positive".into());
    }
}

impl ResearchRules {
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        p.check(self.base_points_per_hour.is_finite() && self.base_points_per_hour >= 0.0, loc, || {
            "research.base_points_per_hour must be >= 0".into()
        });
        p.check(self.max_queue > 0, loc, || "research.max_queue must be positive".into());
    }

    /// Base rate in milli-points per hour.
    pub fn base_milli_per_hour(&self) -> u64 {
        (f64::from(self.base_points_per_hour) * 1000.0).round().max(0.0) as u64
    }
}

/// A festival the player holds for gold (docs/game_design.md §7 Feste und Hofhaltung): its cost
/// grows with the town, its effects are the event modifiers of §12 and the relation memory
/// „Festgäste“ of §10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FestivalRules {
    /// Gold for a festival: this much …
    pub gold_base: u32,
    /// … plus this much per inhabitant.
    pub gold_per_inhabitant: u32,
    /// Festival days (afternoons celebrated instead of worked, §6).
    pub festival_days: u16,
    /// Happiness points of every citizen …
    pub happiness: i16,
    /// … for this many days.
    pub happiness_days: u16,
    /// Days from one festival to the next at least.
    pub cooldown_days: u16,
    /// Relation points every kingdom at peace remembers (envoys as guests).
    pub relation: u8,
    /// … at most this many points from festivals in all.
    pub relation_max: u8,
}

/// When lasting waiting states are reported (docs/game_design.md §2 Meldungen): a building
/// that lacks an input, finds nothing to gather or waits for room to plant, a storage without
/// carriers, a building nobody can reach, hunger, cold and homelessness are reported once when
/// the state begins and not again until it has ended for a while.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoticeRules {
    /// A building's waiting state must last this many game hours before it is reported
    /// (deliveries on their way often end it sooner).
    pub waiting_delay_hours: u16,
    /// A reported waiting state counts as ended once it has not held for this many game hours
    /// in a row; only then is it reported again (a forester that plants one sapling between two
    /// waits stays in the same spell).
    pub waiting_resolved_hours: u16,
}

impl NoticeRules {
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        p.check(self.waiting_resolved_hours > 0, loc, || "notices.waiting_resolved_hours must be positive".into());
    }

    /// [`NoticeRules::waiting_delay_hours`] in ticks.
    pub fn delay_ticks(&self) -> u64 {
        u64::from(self.waiting_delay_hours) * crate::time::TICKS_PER_HOUR
    }

    /// [`NoticeRules::waiting_resolved_hours`] in ticks.
    pub fn resolved_ticks(&self) -> u64 {
        u64::from(self.waiting_resolved_hours) * crate::time::TICKS_PER_HOUR
    }
}

impl FestivalRules {
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        p.check(self.festival_days > 0 && self.happiness > 0 && self.happiness_days > 0, loc, || {
            "festival: festival_days, happiness and happiness_days must be positive".into()
        });
        p.check(self.cooldown_days >= self.festival_days, loc, || "festival.cooldown_days must cover the festival days".into());
        p.check(self.relation <= self.relation_max && self.relation_max <= 30, loc, || {
            "festival: relation <= relation_max <= 30".into()
        });
    }
}
