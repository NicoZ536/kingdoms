//! Walking, navigation and the daily routine (`rules.ron` → `walking`; docs/game_design.md §2,
//! §3, §6).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::error::{Loc, Problems};
use crate::time::{HOURS_PER_DAY, TICKS_PER_DAY, TICKS_PER_HOUR};
use crate::world::Terrain;

/// Hours of the daily routine (docs/game_design.md §6). All hours are whole hours of the day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutineHours {
    /// Getting up (end of the night at home).
    pub wake_hour: u8,
    /// Workers leave home for the workplace ("Arbeitsweg"); work starts on arrival.
    pub work_start_hour: u8,
    /// Lunch break at the workplace starts.
    pub lunch_start_hour: u8,
    /// Lunch break ends.
    pub lunch_end_hour: u8,
    /// End of work ("Feierabend"); workers walk home.
    pub work_end_hour: u8,
    /// Farmers (field and pasture farms) work this many hours longer in summer.
    pub farmer_summer_extra_hours: u8,
    /// Farmers end this many hours earlier in winter.
    pub farmer_winter_fewer_hours: u8,
    /// One adult per household walks to the market ("Besorgungen").
    pub errand_hour: u8,
    /// Free time at a tavern or church in reach.
    pub leisure_hour: u8,
    /// Everybody walks home ("Heimweg").
    pub home_hour: u8,
    /// Night rest until `wake_hour`.
    pub sleep_hour: u8,
    /// Pupils walk to school.
    pub school_start_hour: u8,
    /// School ends; pupils walk home.
    pub school_end_hour: u8,
}

impl RoutineHours {
    /// Hours of the reference work shift without the lunch break (8 with the shipped data).
    pub fn shift_hours(&self) -> u64 {
        let day = u64::from(self.work_end_hour.saturating_sub(self.work_start_hour));
        day.saturating_sub(u64::from(self.lunch_end_hour.saturating_sub(self.lunch_start_hour)))
    }

    /// Production speed of a present worker relative to round-the-clock operation, in permille:
    /// recipe and build times are given for 24 hours of operation, but people work only during
    /// the shift (24 h / 8 h → 3000).
    pub fn shift_factor_permille(&self) -> u64 {
        let shift = self.shift_hours().max(1) * TICKS_PER_HOUR;
        TICKS_PER_DAY * 1000 / shift
    }

    fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        let day = HOURS_PER_DAY as u8;
        let work = [self.wake_hour, self.work_start_hour, self.lunch_start_hour, self.lunch_end_hour, self.work_end_hour];
        let evening = [self.errand_hour, self.leisure_hour, self.home_hour, self.sleep_hour];
        let rising = work.windows(2).all(|w| w[0] < w[1])
            && self.work_end_hour <= self.errand_hour
            && evening.windows(2).all(|w| w[0] < w[1]);
        p.check(rising && self.sleep_hour <= day, loc, || {
            "walking.routine hours must rise: wake < work_start < lunch_start < lunch_end < work_end <= errand < \
             leisure < home < sleep <= 24"
                .into()
        });
        p.check(self.wake_hour <= self.school_start_hour && self.school_start_hour < self.school_end_hour, loc, || {
            "walking.routine: wake_hour <= school_start_hour < school_end_hour".into()
        });
        p.check(self.school_end_hour < self.errand_hour, loc, || "walking.routine: school ends before errands".into());
        p.check(
            self.work_end_hour.saturating_add(self.farmer_summer_extra_hours) <= self.home_hour
                && self.farmer_winter_fewer_hours < self.work_end_hour.saturating_sub(self.lunch_end_hour),
            loc,
            || "walking.routine: farmer shifts must end after lunch and before home_hour".into(),
        );
        p.check(self.shift_hours() > 0, loc, || "walking.routine: the work shift must last at least an hour".into());
    }
}

/// Walking speeds, path costs, pathfinding limits and the daily routine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalkingRules {
    /// Cost of entering an off-road tile per land terrain, in permille of a tile on a road with
    /// speed 1.0 (higher = slower). Water is not listed: only bridges cross it.
    pub terrain_cost_permille: BTreeMap<Terrain, u16>,
    /// Extra cost of a step one elevation level up or down, in permille.
    pub slope_cost_permille: u16,
    /// Largest walkable elevation difference between neighbouring tiles (cliffs block).
    pub max_step: u8,
    /// Walking speed of children, in permille of adults.
    pub child_speed_permille: u16,
    /// Walking speed of elders, in permille of adults.
    pub elder_speed_permille: u16,
    /// Walking speed at health 0 (rising linearly to full speed at `full_productivity_health`).
    pub sick_speed_permille: u16,
    /// Walking speed at full snow cover, in permille.
    pub snow_speed_permille: u16,
    /// Walking speed in rain at full intensity, in permille.
    pub rain_speed_permille: u16,
    /// Walking speed in a storm at full intensity, in permille.
    pub storm_speed_permille: u16,
    /// Node expansions of the pathfinder per tick (requests wait in a queue beyond it).
    pub path_budget_per_tick: u32,
    /// Extra node expansions per tick for every request waiting in the queue at the start of
    /// the tick (rush hours drain faster), up to `path_budget_max_per_tick`.
    pub path_budget_per_queued: u32,
    /// Upper limit of the per-tick budget including the extra for waiting requests.
    pub path_budget_max_per_tick: u32,
    /// Most node expansions of one search; a longer search counts as "no path".
    pub max_search_nodes: u32,
    /// Paths kept in the path memory (least recently used are forgotten first).
    pub path_cache_size: u32,
    /// Node expansions a remembered path costs from the per-tick budget.
    pub path_cache_hit_cost: u32,
    /// Ticks until a citizen without a path tries again.
    pub no_path_retry_ticks: u32,
    /// Hours of the daily routine.
    pub routine: RoutineHours,
}

impl WalkingRules {
    /// Path cost of an off-road tile of `terrain` in permille (`None`: not walkable).
    pub fn terrain_cost(&self, terrain: Terrain) -> Option<u16> {
        self.terrain_cost_permille.get(&terrain).copied()
    }

    /// Node expansions the pathfinder may spend in a tick that starts with `queued` waiting
    /// requests: the base budget plus the extra per waiting request, capped.
    pub fn path_budget(&self, queued: usize) -> u32 {
        let queued = u32::try_from(queued).unwrap_or(u32::MAX);
        self.path_budget_per_tick
            .saturating_add(self.path_budget_per_queued.saturating_mul(queued))
            .min(self.path_budget_max_per_tick.max(self.path_budget_per_tick))
    }

    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        for t in Terrain::ALL {
            let listed = self.terrain_cost_permille.get(&t);
            if t.is_water() {
                p.check(listed.is_none(), loc, || format!("walking.terrain_cost_permille must not list water ({t:?})"));
            } else {
                p.check(listed.is_some_and(|c| (500..=20_000).contains(c)), loc, || {
                    format!("walking.terrain_cost_permille needs {t:?} in 500..=20000")
                });
            }
        }
        p.check(self.slope_cost_permille <= 10_000, loc, || "walking.slope_cost_permille must be <= 10000".into());
        p.check(self.max_step >= 1, loc, || "walking.max_step must be at least 1".into());
        for (name, v) in [
            ("child_speed_permille", self.child_speed_permille),
            ("elder_speed_permille", self.elder_speed_permille),
            ("sick_speed_permille", self.sick_speed_permille),
            ("snow_speed_permille", self.snow_speed_permille),
            ("rain_speed_permille", self.rain_speed_permille),
            ("storm_speed_permille", self.storm_speed_permille),
        ] {
            p.check((100..=2000).contains(&v), loc, || format!("walking.{name} must be in 100..=2000"));
        }
        p.check(self.path_budget_per_tick >= 100, loc, || "walking.path_budget_per_tick must be >= 100".into());
        p.check(self.path_budget_max_per_tick >= self.path_budget_per_tick, loc, || {
            "walking.path_budget_max_per_tick must be >= path_budget_per_tick".into()
        });
        p.check(self.max_search_nodes >= 1000, loc, || "walking.max_search_nodes must be >= 1000".into());
        p.check((16..=1_000_000).contains(&self.path_cache_size), loc, || {
            "walking.path_cache_size must be in 16..=1000000".into()
        });
        p.check((1..=10_000).contains(&self.path_cache_hit_cost), loc, || "walking.path_cache_hit_cost must be in 1..=10000".into());
        p.check(self.no_path_retry_ticks > 0, loc, || "walking.no_path_retry_ticks must be positive".into());
        self.routine.validate(p, loc);
    }
}
