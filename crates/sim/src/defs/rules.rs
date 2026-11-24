//! Global balancing rules (`data/rules.ron`).

use serde::{Deserialize, Serialize};

use super::climate::{AgricultureRules, ClimateRules, SeasonGathering, WeatherEffects};
use super::common::JobTier;
use super::diplomacy::DiplomacyRules;
use super::military::{MilitaryRules, RankRule, validate_ranks};
use super::population::PopulationRules;
use super::progression::{EventRules, FestivalRules, NoticeRules, ResearchRules};
use super::walking::WalkingRules;
use super::error::{Loc, Problems};
use crate::time::Season;

/// Weather conditions (docs/game_design.md §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Weather {
    /// Klar.
    Clear,
    /// Bewölkt.
    Cloudy,
    /// Regen: raises soil moisture.
    Rain,
    /// Sturm: can damage fields and buildings.
    Storm,
    /// Schnee.
    Snow,
    /// Dürre: reduces growth.
    Drought,
}

impl Weather {
    /// All conditions.
    pub const ALL: [Weather; 6] =
        [Weather::Clear, Weather::Cloudy, Weather::Rain, Weather::Storm, Weather::Snow, Weather::Drought];

    /// True for rain, storm and snow: they water the ground and end a dry spell.
    pub const fn is_precipitation(self) -> bool {
        matches!(self, Weather::Rain | Weather::Storm | Weather::Snow)
    }

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            Weather::Clear => "Klar",
            Weather::Cloudy => "Bewölkt",
            Weather::Rain => "Regen",
            Weather::Storm => "Sturm",
            Weather::Snow => "Schnee",
            Weather::Drought => "Dürre",
        }
    }
}

/// One value per season.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeasonValues<T> {
    /// Frühling.
    pub spring: T,
    /// Sommer.
    pub summer: T,
    /// Herbst.
    pub autumn: T,
    /// Winter.
    pub winter: T,
}

impl<T> SeasonValues<T> {
    /// Value for `season`.
    pub fn get(&self, season: Season) -> &T {
        match season {
            Season::Spring => &self.spring,
            Season::Summer => &self.summer,
            Season::Autumn => &self.autumn,
            Season::Winter => &self.winter,
        }
    }
}

/// Daily wage per worker in gold, by job tier.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wages {
    /// Hilfskraft.
    pub unskilled: f32,
    /// Fachkraft.
    pub skilled: f32,
    /// Gelehrter.
    pub educated: f32,
}

impl Wages {
    /// Wage for a job tier.
    pub fn for_tier(&self, tier: JobTier) -> f32 {
        match tier {
            JobTier::Unskilled => self.unskilled,
            JobTier::Skilled => self.skilled,
            JobTier::Educated => self.educated,
        }
    }
}

/// Global balancing constants; values follow docs/game_design.md where it gives them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameRules {
    /// Walking speed in tiles per tick at speed multiplier 1.0 (roads multiply it).
    pub base_walk_speed: f32,
    /// Wages per worker and day.
    pub wages: Wages,
    /// Work cycles per consumed tool.
    pub tool_wear_cycles: u16,
    /// Productivity without tools, in percent.
    pub no_tools_productivity_percent: u8,
    /// Default input/output buffer per resource of production buildings.
    pub default_buffer_capacity: u16,
    /// Extra construction time per cleared tile with vegetation, in percent.
    pub clearing_time_percent_per_tile: u16,
    /// Wood gained per cleared tree.
    pub wood_per_cleared_tree: u16,
    /// Daily crop and tree growth factor per season.
    pub season_growth: SeasonValues<f32>,
    /// Seasonal factors of fishing, pastures and bees.
    pub season_gathering: SeasonValues<SeasonGathering>,
    /// Food consumption in winter, in percent of normal.
    pub winter_food_consumption_percent: u16,
    /// Work speed in winter, in percent of normal.
    pub winter_work_speed_percent: u16,
    /// Weather weights per season.
    pub weather: SeasonValues<Vec<(Weather, u32)>>,
    /// Effects of every weather condition at full intensity.
    pub weather_effects: WeatherEffects,
    /// Drawing and reporting of the daily weather.
    pub climate: ClimateRules,
    /// Field farming.
    pub agriculture: AgricultureRules,
    /// Highest selectable tax rate in percent.
    pub max_tax_rate_percent: u8,
    /// Research rate and queue (docs/game_design.md §9).
    pub research: ResearchRules,
    /// Random events with choices (docs/game_design.md §12).
    pub events: EventRules,
    /// Festivals the player holds for gold (docs/game_design.md §7).
    pub festival: FestivalRules,
    /// When lasting waiting states are reported (docs/game_design.md §2 Meldungen).
    pub notices: NoticeRules,
    /// Units one carrier moves per trip.
    pub carrier_capacity: u16,
    /// Ticks a carrier needs to load and unload at the ends of a trip.
    pub carrier_handling_ticks: u16,
    /// Tools kept in the input buffer of production buildings that wear tools.
    pub tools_buffer: u16,
    /// Daily price adjustment towards the target price, in percent of the difference.
    pub price_smoothing_percent: u8,
    /// Lowest demand/supply ratio used by the price formula.
    pub price_ratio_min: f32,
    /// Highest demand/supply ratio used by the price formula.
    pub price_ratio_max: f32,
    /// Margin of the world market in percent: sales earn the price minus the margin,
    /// purchases cost the price plus the margin.
    pub trade_margin_percent: u8,
    /// Highest daily limit of one trade order in units.
    pub max_trade_daily_limit: u32,
    /// Market fee per resident served by a market, in gold per day.
    pub market_fee_per_resident: f32,
    /// Units of every good a selling building (tavern: beer) keeps in stock; carriers refill
    /// it from the storages (docs/game_design.md §5.4).
    pub shop_stock: u16,
    /// Productivity of all production while the treasury is bankrupt, in percent.
    pub bankruptcy_productivity_percent: u8,
    /// Tree growth (0–255 scale) per game hour at season growth factor 1.0.
    pub tree_growth_per_hour: u8,
    /// Growth from which a tree can be felled by woodcutters.
    pub fellable_tree_growth: u8,
    /// Game hours until the stump of a felled tree has rotted away.
    pub stump_decay_hours: u8,
    /// Fish regenerated per fish tile and day.
    pub fish_regeneration_per_day: u16,
    /// Most fish a fish tile holds after regeneration.
    pub max_fish_per_tile: u16,
    /// Citizens, households, needs, happiness and demography (docs/game_design.md §6).
    pub population: PopulationRules,
    /// Walking speeds, path costs, pathfinding limits and the daily routine (§2, §3, §6).
    pub walking: WalkingRules,
    /// AI kingdoms, relations, treaties, war and caravans (§7, §10).
    pub diplomacy: DiplomacyRules,
    /// Recruitment, pay, morale, combat, bandits, AI armies and campaigns (§11).
    pub military: MilitaryRules,
    /// Ruler ranks in order; the first one is the start (§1).
    pub ranks: Vec<RankRule>,
}

impl GameRules {
    /// Checks value ranges.
    pub(crate) fn validate(&self, problems: &mut Problems, file: &str) {
        let loc = Loc { file, kind: "rules", key: "" };
        let p = problems;
        let pos = |v: f32| v.is_finite() && v > 0.0;
        p.check(pos(self.base_walk_speed) && self.base_walk_speed <= 1.0, &loc, || {
            "base_walk_speed must be in (0, 1]".into()
        });
        let w = self.wages;
        p.check([w.unskilled, w.skilled, w.educated].iter().all(|v| v.is_finite() && *v >= 0.0), &loc, || {
            "wages must be >= 0".into()
        });
        p.check(w.unskilled <= w.skilled && w.skilled <= w.educated, &loc, || {
            "wages must rise with the job tier".into()
        });
        p.check(self.tool_wear_cycles > 0, &loc, || "tool_wear_cycles must be positive".into());
        p.check(self.no_tools_productivity_percent <= 100, &loc, || {
            "no_tools_productivity_percent must be <= 100".into()
        });
        p.check(self.default_buffer_capacity > 0, &loc, || "default_buffer_capacity must be positive".into());
        for season in Season::ALL {
            let g = *self.season_growth.get(season);
            p.check(g.is_finite() && g >= 0.0, &loc, || format!("season_growth for {season:?} must be >= 0"));
            let table = self.weather.get(season);
            p.check(table.iter().map(|(_, w)| u64::from(*w)).sum::<u64>() > 0, &loc, || {
                format!("weather table for {season:?} needs a positive total weight")
            });
            let mut seen: Vec<Weather> = table.iter().map(|(w, _)| *w).collect();
            seen.sort();
            let before = seen.len();
            seen.dedup();
            p.check(seen.len() == before, &loc, || format!("weather table for {season:?} lists a weather twice"));
            self.season_gathering.get(season).validate(p, &loc, season.sprite_key());
        }
        self.weather_effects.validate(p, &loc);
        self.climate.validate(p, &loc);
        self.agriculture.validate(p, &loc);
        p.check(self.winter_food_consumption_percent > 0, &loc, || {
            "winter_food_consumption_percent must be positive".into()
        });
        p.check(self.winter_work_speed_percent > 0, &loc, || "winter_work_speed_percent must be positive".into());
        p.check(self.max_tax_rate_percent <= 100, &loc, || "max_tax_rate_percent must be <= 100".into());
        self.research.validate(p, &loc);
        self.events.validate(p, &loc);
        self.festival.validate(p, &loc);
        self.notices.validate(p, &loc);
        p.check(self.carrier_capacity > 0, &loc, || "carrier_capacity must be positive".into());
        p.check(self.tools_buffer > 0, &loc, || "tools_buffer must be positive".into());
        p.check(self.price_smoothing_percent > 0 && self.price_smoothing_percent <= 100, &loc, || {
            "price_smoothing_percent must be in 1..=100".into()
        });
        p.check(
            pos(self.price_ratio_min) && pos(self.price_ratio_max) && self.price_ratio_min <= 1.0 && self.price_ratio_max >= 1.0,
            &loc,
            || "price ratios must be positive with min <= 1 <= max".into(),
        );
        p.check(self.trade_margin_percent < 100, &loc, || "trade_margin_percent must be below 100".into());
        p.check(self.max_trade_daily_limit > 0, &loc, || "max_trade_daily_limit must be positive".into());
        p.check(self.shop_stock > 0, &loc, || "shop_stock must be positive".into());
        p.check(self.market_fee_per_resident.is_finite() && self.market_fee_per_resident >= 0.0, &loc, || {
            "market_fee_per_resident must be >= 0".into()
        });
        p.check(self.bankruptcy_productivity_percent <= 100, &loc, || {
            "bankruptcy_productivity_percent must be <= 100".into()
        });
        p.check(self.fellable_tree_growth > 0, &loc, || "fellable_tree_growth must be positive".into());
        p.check(self.stump_decay_hours > 0, &loc, || "stump_decay_hours must be positive".into());
        p.check(self.max_fish_per_tile > 0, &loc, || "max_fish_per_tile must be positive".into());
        self.population.validate(p, &loc);
        self.walking.validate(p, &loc);
        self.diplomacy.validate(p, &loc);
        self.military.validate(p, &loc);
        validate_ranks(&self.ranks, p, &loc);
    }
}
