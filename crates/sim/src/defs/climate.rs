//! Seasons, weather and field rules (`rules.ron` → `season_gathering`, `weather_effects`,
//! `climate`, `agriculture`; docs/game_design.md §8).

use serde::{Deserialize, Serialize};

use super::error::{Loc, Problems};
use super::rules::Weather;

/// Seasonal factors of gatherers that depend on nature rather than on the work speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeasonGathering {
    /// Fishing (fish deposits), in percent.
    pub fishing_percent: u16,
    /// Animal farms on pasture, in percent.
    pub pasture_percent: u16,
    /// Bees, in percent (0 = winter rest).
    pub bees_percent: u16,
}

/// Effects of one weather condition at full intensity (100 %). Weaker weather moves every
/// percentage proportionally towards 100 % and every amount towards 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherEffect {
    /// Crop growth, in percent.
    pub growth_percent: u16,
    /// Change of field moisture (0–255) per day.
    pub moisture_per_day: i16,
    /// Work speed of outdoor buildings and construction sites, in percent.
    pub outdoor_work_percent: u16,
    /// Fuel demand of households in winter, in percent.
    pub heating_percent: u16,
    /// Daily disease risk of citizens, in percent.
    pub disease_percent: u16,
    /// Share of their condition standing crops lose on such a day, in percent.
    pub field_damage_percent: u8,
    /// Ground wetness (0–100) the weather drives towards (visual only).
    pub ground_wetness: u8,
    /// Snow cover change per hour in points of 0–100 (negative: melting; visual only).
    pub snow_per_hour: i8,
    /// Change of the day's temperature in °C (at intensity 100 %).
    pub temperature_c: i8,
}

/// [`WeatherEffect`] of every weather condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherEffects {
    /// Klar.
    pub clear: WeatherEffect,
    /// Bewölkt.
    pub cloudy: WeatherEffect,
    /// Regen.
    pub rain: WeatherEffect,
    /// Sturm.
    pub storm: WeatherEffect,
    /// Schnee.
    pub snow: WeatherEffect,
    /// Dürre.
    pub drought: WeatherEffect,
}

impl WeatherEffects {
    /// Effects of `weather`.
    pub fn get(&self, weather: Weather) -> &WeatherEffect {
        match weather {
            Weather::Clear => &self.clear,
            Weather::Cloudy => &self.cloudy,
            Weather::Rain => &self.rain,
            Weather::Storm => &self.storm,
            Weather::Snow => &self.snow,
            Weather::Drought => &self.drought,
        }
    }
}

/// How the daily weather is drawn and reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClimateRules {
    /// Chance that yesterday's weather continues (if the season knows it), in percent.
    pub persistence_percent: u8,
    /// Lowest drawn intensity, in percent.
    pub min_intensity: u8,
    /// Highest drawn intensity, in percent.
    pub max_intensity: u8,
    /// Extra snow melt per hour outside winter, in points of 0–100.
    pub snow_melt_outside_winter_per_hour: u8,
    /// Most the ground wetness rises per hour, in points of 0–100.
    pub wetness_rise_per_hour: u8,
    /// Most the ground wetness falls per hour, in points of 0–100.
    pub wetness_fall_per_hour: u8,
    /// Storms from this intensity on are announced.
    pub storm_warning_intensity: u8,
    /// Winter snow from this intensity on is a hard frost (announced once per winter).
    pub hard_frost_intensity: u8,
    /// Water need of citizens without a well or fresh water during a drought (instead of
    /// `population.no_water_thirst`).
    pub drought_thirst_without_water: u8,
    /// Daily temperature (docs/game_design.md §8.1).
    pub temperature: TemperatureRules,
}

/// The daily temperature in whole °C: the season's mean, the weather's change, a daily
/// deviation of up to ± `daily_spread_c` (a hash of the day, no random draw) and the climate of
/// the start area (the generator's temperature of the start tile, ± `climate_range_c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemperatureRules {
    /// Mean in spring.
    pub spring_c: i8,
    /// Mean in summer.
    pub summer_c: i8,
    /// Mean in autumn.
    pub autumn_c: i8,
    /// Mean in winter.
    pub winter_c: i8,
    /// Largest daily deviation from the mean.
    pub daily_spread_c: u8,
    /// Largest shift by the start area's climate (cold mountains, warm lowlands).
    pub climate_range_c: u8,
    /// Households heat (burn fuel, need it for warmth) below this temperature.
    pub heating_below_c: i8,
    /// Frost kills unripe frost-sensitive crops at or below this temperature.
    pub frost_at_c: i8,
}

impl TemperatureRules {
    /// Mean temperature of `season`.
    pub fn season_mean_c(&self, season: crate::time::Season) -> i8 {
        match season {
            crate::time::Season::Spring => self.spring_c,
            crate::time::Season::Summer => self.summer_c,
            crate::time::Season::Autumn => self.autumn_c,
            crate::time::Season::Winter => self.winter_c,
        }
    }
}

/// Field farming (docs/game_design.md §8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgricultureRules {
    /// Lowest natural fertility (0–255) of a field tile.
    pub min_field_fertility: u8,
    /// Fertility from which the soil quality is full.
    pub full_fertility: u8,
    /// Yield at soil quality 0, in percent (linear up to 100 % at full fertility).
    pub min_yield_percent: u8,
    /// Growth at soil quality 0, in percent (linear up to 100 % at full fertility).
    pub min_growth_percent: u8,
    /// Field moisture (0–255) from which growth is full.
    pub full_moisture: u8,
    /// Growth on completely dry soil, in percent (linear up to 100 % at `full_moisture`).
    pub min_moisture_growth_percent: u8,
    /// Standing crops on fields drier than this wither.
    pub dry_moisture: u8,
    /// Condition withering crops lose per day, in percent.
    pub wither_percent_per_day: u8,
    /// Daily pull of field moisture back towards the tile's natural moisture, in percent of
    /// the difference.
    pub moisture_recovery_percent: u8,
    /// Fertility unsown fields regain per day (up to the tile's natural fertility).
    pub fallow_regeneration_per_day: u8,
    /// Fields below this share of their natural fertility stay fallow, in percent.
    pub fallow_below_percent: u8,
    /// Annual crops are only sown if they are expected to ripen this much before winter
    /// (expected growth ≥ required × (100 % + margin)), in percent.
    pub sowing_margin_percent: u8,
    /// Output buffer of field farms per crop resource (a harvest is large).
    pub harvest_buffer: u16,
    /// Pasture tiles one animal of a herd needs.
    pub pasture_tiles_per_animal: u8,
    /// Herd of a new pasture farm in percent of what its pasture feeds.
    pub herd_start_percent: u8,
    /// Daily growth of a herd outside winter in percent of what its pasture feeds (at least
    /// one animal).
    pub herd_growth_percent_per_day: u8,
}

fn percent(v: u8) -> bool {
    v <= 100
}

impl SeasonGathering {
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>, season: &str) {
        for (name, v) in
            [("fishing_percent", self.fishing_percent), ("pasture_percent", self.pasture_percent), ("bees_percent", self.bees_percent)]
        {
            p.check(v <= 1000, loc, || format!("season_gathering.{season}.{name} must be <= 1000"));
        }
    }
}

impl WeatherEffects {
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        for w in Weather::ALL {
            let e = self.get(w);
            let name = format!("{w:?}");
            for (field, v) in [
                ("growth_percent", e.growth_percent),
                ("outdoor_work_percent", e.outdoor_work_percent),
                ("heating_percent", e.heating_percent),
                ("disease_percent", e.disease_percent),
            ] {
                p.check(v <= 1000, loc, || format!("weather_effects.{name}.{field} must be <= 1000"));
            }
            p.check(e.outdoor_work_percent > 0, loc, || {
                format!("weather_effects.{name}.outdoor_work_percent must be positive")
            });
            p.check(percent(e.field_damage_percent), loc, || format!("weather_effects.{name}.field_damage_percent must be <= 100"));
            p.check(percent(e.ground_wetness), loc, || format!("weather_effects.{name}.ground_wetness must be <= 100"));
        }
    }
}

impl ClimateRules {
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        p.check(percent(self.persistence_percent), loc, || "climate.persistence_percent must be <= 100".into());
        p.check(self.min_intensity >= 1 && self.min_intensity <= self.max_intensity && percent(self.max_intensity), loc, || {
            "climate intensities must satisfy 1 <= min_intensity <= max_intensity <= 100".into()
        });
        for (name, v) in [
            ("storm_warning_intensity", self.storm_warning_intensity),
            ("hard_frost_intensity", self.hard_frost_intensity),
            ("drought_thirst_without_water", self.drought_thirst_without_water),
            ("wetness_rise_per_hour", self.wetness_rise_per_hour),
            ("wetness_fall_per_hour", self.wetness_fall_per_hour),
            ("snow_melt_outside_winter_per_hour", self.snow_melt_outside_winter_per_hour),
        ] {
            p.check(percent(v), loc, || format!("climate.{name} must be <= 100"));
        }
        p.check(self.wetness_rise_per_hour > 0 && self.wetness_fall_per_hour > 0, loc, || {
            "climate: ground wetness must be able to rise and fall".into()
        });
        let t = &self.temperature;
        p.check(t.winter_c < t.heating_below_c && t.summer_c >= t.heating_below_c, loc, || {
            "climate.temperature: winter must be below and summer at or above heating_below_c".into()
        });
        p.check(t.daily_spread_c <= 15 && t.climate_range_c <= 15 && t.frost_at_c < t.heating_below_c, loc, || {
            "climate.temperature: spreads must be <= 15 and frost_at_c below heating_below_c".into()
        });
    }
}

impl AgricultureRules {
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        for (name, v) in [
            ("min_yield_percent", self.min_yield_percent),
            ("min_growth_percent", self.min_growth_percent),
            ("min_moisture_growth_percent", self.min_moisture_growth_percent),
            ("wither_percent_per_day", self.wither_percent_per_day),
            ("moisture_recovery_percent", self.moisture_recovery_percent),
            ("fallow_below_percent", self.fallow_below_percent),
        ] {
            p.check(percent(v), loc, || format!("agriculture.{name} must be <= 100"));
        }
        p.check(self.full_fertility > 0 && self.full_moisture > 0, loc, || {
            "agriculture: full_fertility and full_moisture must be positive".into()
        });
        p.check(self.min_field_fertility <= self.full_fertility, loc, || {
            "agriculture: min_field_fertility must not exceed full_fertility".into()
        });
        p.check(self.harvest_buffer > 0, loc, || "agriculture.harvest_buffer must be positive".into());
        p.check(self.pasture_tiles_per_animal > 0, loc, || "agriculture.pasture_tiles_per_animal must be positive".into());
        p.check(percent(self.herd_start_percent), loc, || "agriculture.herd_start_percent must be <= 100".into());
        p.check(percent(self.herd_growth_percent_per_day), loc, || "agriculture.herd_growth_percent_per_day must be <= 100".into());
    }
}
