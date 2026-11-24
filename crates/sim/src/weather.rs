//! Seasons and weather (docs/game_design.md §8.1–8.2).
//!
//! The weather of a day is drawn at midnight (and for the first day when the game starts) from
//! the season's weights in `rules.ron`: with `climate.persistence_percent` the previous day's
//! weather continues (if the season knows it), otherwise a new condition is drawn; the
//! intensity is uniform in `climate.min_intensity ..= climate.max_intensity`. The draws use a
//! dedicated [`ChaCha8Rng`] stored in the state, so weather does not shift other random rolls.
//!
//! Effects ([`WeatherEffect`]) are given for intensity 100 %; a weaker day moves every
//! percentage proportionally towards 100 % and every amount towards 0 ([`scale_percent`],
//! [`scale_amount`]). Consumers: crop growth, field moisture and storm damage (agriculture),
//! outdoor work speed (production, construction), winter fuel demand, disease risk and the
//! water need during droughts (population). Snow cover and ground wetness are visual values
//! updated hourly.

use glam::IVec2;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::defs::{BuildingDef, Defs, EventDefId, Gathering, GameRules, Weather, WeatherEffect};
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::state::GameState;
use crate::time::{GameClock, Season};
use crate::world::DepositKind;

/// Stream constant mixed into the seed for the weather RNG.
const WEATHER_RNG_STREAM: u64 = 0x5745_4154_4845_5221; // "WEATHER!"

/// Weather forced for some days: by a developer command or as the consequence of an event
/// (drought, flood; docs/game_design.md §12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForcedWeather {
    /// The weather.
    pub weather: Weather,
    /// Its intensity in percent.
    pub intensity: u8,
    /// First calendar day ([`GameClock::total_days`]) on which the normal draw resumes.
    pub until_day: u64,
    /// The event whose consequence it is (`None`: a developer command).
    pub event: Option<EventDefId>,
}

/// Notifications already given in the current winter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct WinterNotes {
    first_snow: bool,
    hard_frost: bool,
}

/// Today's weather and the visual ground state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherState {
    current: Weather,
    intensity: u8,
    /// Consecutive days with `current` (1 on its first day).
    streak_days: u16,
    /// Consecutive days without rain, storm or snow up to today (0 on a wet day).
    dry_days: u16,
    snow_cover: u8,
    ground_wetness: u8,
    forced: Option<ForcedWeather>,
    winter_notes: WinterNotes,
    rng: ChaCha8Rng,
    /// Seed of the wind (a hash per day and hour; the weather draws are not touched).
    wind_seed: u64,
    /// Compass point the wind blows towards, 0–15 (0 = north, clockwise in steps of 22.5°).
    wind_towards: u8,
    /// Wind strength 0–100.
    wind_strength: u8,
    /// Shift of the temperature by the start area's climate in °C (set at game creation).
    climate_c: i8,
}

/// Unit vectors of the 16 compass points in permille (map coordinates: north is −y).
const COMPASS_PERMILLE: [(i32, i32); 16] = [
    (0, -1000),
    (383, -924),
    (707, -707),
    (924, -383),
    (1000, 0),
    (924, 383),
    (707, 707),
    (383, 924),
    (0, 1000),
    (-383, 924),
    (-707, 707),
    (-924, 383),
    (-1000, 0),
    (-924, -383),
    (-707, -707),
    (-383, -924),
];

/// SplitMix64 finaliser (the wind's hash).
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// `percent` (given for intensity 100 %) at `intensity` %: proportionally closer to 100 %.
pub fn scale_percent(percent: u16, intensity: u8) -> u32 {
    let p = i64::from(percent);
    let scaled = 100 + (p - 100) * i64::from(intensity.min(100)) / 100;
    scaled.max(0) as u32
}

/// `amount` (given for intensity 100 %) at `intensity` %: proportionally closer to 0.
pub fn scale_amount(amount: i32, intensity: u8) -> i32 {
    amount * i32::from(intensity.min(100)) / 100
}

/// Adjective for an intensity: leicht / mäßig / stark.
fn strength(intensity: u8) -> usize {
    match intensity {
        0..40 => 0,
        40..70 => 1,
        _ => 2,
    }
}

impl WeatherState {
    /// Weather for a new game: the first day's weather is drawn from the season at `clock`.
    pub(crate) fn new(defs: &Defs, seed: u64, clock: GameClock, climate_c: i8) -> Self {
        let mut state = WeatherState {
            current: Weather::Clear,
            intensity: 50,
            streak_days: 1,
            dry_days: 1,
            snow_cover: 0,
            ground_wetness: 0,
            forced: None,
            winter_notes: WinterNotes::default(),
            rng: ChaCha8Rng::seed_from_u64(seed ^ WEATHER_RNG_STREAM),
            wind_seed: mix(seed ^ WEATHER_RNG_STREAM),
            wind_towards: 0,
            wind_strength: 0,
            climate_c,
        };
        let (weather, intensity) = state.draw(defs.rules(), clock.season(), false);
        state.current = weather;
        state.intensity = intensity;
        state.dry_days = u16::from(!weather.is_precipitation());
        state.update_wind(clock);
        state
    }

    /// A weather that never changes (fixtures of unit tests).
    #[cfg(test)]
    pub(crate) fn fixed(weather: Weather, intensity: u8) -> Self {
        WeatherState {
            current: weather,
            intensity,
            streak_days: 1,
            dry_days: u16::from(!weather.is_precipitation()),
            snow_cover: 0,
            ground_wetness: 0,
            forced: Some(ForcedWeather { weather, intensity, until_day: u64::MAX, event: None }),
            winter_notes: WinterNotes::default(),
            rng: ChaCha8Rng::seed_from_u64(WEATHER_RNG_STREAM),
            wind_seed: 0,
            wind_towards: 0,
            wind_strength: 0,
            climate_c: 0,
        }
    }

    /// The same weather after `days` dry days (fixtures of unit tests; 0 only for wet weather).
    #[cfg(test)]
    pub(crate) fn with_dry_days(mut self, days: u16) -> Self {
        self.dry_days = days;
        self
    }

    /// The same weather in a start area `climate_c` degrees warmer (fixtures of unit tests).
    #[cfg(test)]
    pub(crate) fn with_climate(mut self, climate_c: i8) -> Self {
        self.climate_c = climate_c;
        self
    }

    /// Climate shift of a start tile with the generator's `temperature` (0–255, 128 =
    /// temperate): ± `climate.temperature.climate_range_c`.
    pub(crate) fn climate_shift_c(rules: &GameRules, temperature: u8) -> i8 {
        let range = i32::from(rules.climate.temperature.climate_range_c);
        i8::try_from((i32::from(temperature) - 128) * range / 128).unwrap_or(0)
    }

    /// Today's temperature in whole °C (docs/game_design.md §8.1): the season's mean, the
    /// weather's change scaled by its intensity, a daily deviation (hash of the day) and the
    /// start area's climate.
    pub fn temperature_c(&self, rules: &GameRules, clock: GameClock) -> i8 {
        let t = &rules.climate.temperature;
        let mean = i32::from(t.season_mean_c(clock.season()));
        let weather = scale_amount(i32::from(self.effect(rules).temperature_c), self.intensity);
        let spread = i32::from(t.daily_spread_c);
        let day = clock.total_days();
        let noise = if spread == 0 {
            0
        } else {
            i32::try_from(mix(self.wind_seed ^ day.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5445_4D50) % (2 * spread as u64 + 1))
                .unwrap_or(0)
                - spread
        };
        i8::try_from((mean + weather + noise + i32::from(self.climate_c)).clamp(-60, 60)).unwrap_or(0)
    }

    /// True if households heat today (temperature below `heating_below_c`).
    pub fn needs_heating(&self, rules: &GameRules, clock: GameClock) -> bool {
        self.temperature_c(rules, clock) < rules.climate.temperature.heating_below_c
    }

    /// True on a frost day (temperature at or below `frost_at_c`).
    pub fn is_frost(&self, rules: &GameRules, clock: GameClock) -> bool {
        self.temperature_c(rules, clock) <= rules.climate.temperature.frost_at_c
    }

    /// German temperature text („−3 °C“).
    pub fn temperature_text(&self, rules: &GameRules, clock: GameClock) -> String {
        let t = self.temperature_c(rules, clock);
        if t < 0 { format!("−{} °C", -i32::from(t)) } else { format!("{t} °C") }
    }

    /// Today's weather.
    pub fn current(&self) -> Weather {
        self.current
    }

    /// Today's intensity in percent (1–100).
    pub fn intensity(&self) -> u8 {
        self.intensity
    }

    /// Consecutive days with today's weather (1 on its first day).
    pub fn streak_days(&self) -> u16 {
        self.streak_days
    }

    /// Consecutive days without rain, storm or snow up to today (0 on a wet day): the dry spell
    /// a drought event follows (docs/game_design.md §12).
    pub fn dry_days(&self) -> u16 {
        self.dry_days
    }

    /// Snow cover 0–100 (visual: 0 = no snow, 100 = deep snow).
    pub fn snow_cover(&self) -> u8 {
        self.snow_cover
    }

    /// Ground wetness 0–100 (visual: puddles, dark soil).
    pub fn ground_wetness(&self) -> u8 {
        self.ground_wetness
    }

    /// Weather forced by a developer command or an event, if any.
    pub fn forced(&self) -> Option<ForcedWeather> {
        self.forced
    }

    /// Compass point the wind blows towards: 0–15, 0 = north (−y), clockwise in steps of 22.5°
    /// (4 = east, 8 = south, 12 = west).
    pub fn wind_direction(&self) -> u8 {
        self.wind_towards
    }

    /// Direction the wind blows towards in degrees clockwise from north (0–337.5 in steps of
    /// 22.5, rounded down).
    pub fn wind_degrees(&self) -> u16 {
        u16::from(self.wind_towards) * 45 / 2
    }

    /// Wind strength 0–100 (calm below 15, a storm above 70).
    pub fn wind_strength(&self) -> u8 {
        self.wind_strength
    }

    /// Unit vector of the wind direction in permille of a tile (map coordinates: north is −y),
    /// e.g. for leaning trees, slanted rain or drifting smoke; multiply by
    /// [`WeatherState::wind_strength`] for the force.
    pub fn wind_vector_permille(&self) -> IVec2 {
        let (x, y) = COMPASS_PERMILLE[usize::from(self.wind_towards % 16)];
        IVec2::new(x, y)
    }

    /// The wind of the current hour: a prevailing direction per day that veers by up to one
    /// compass point per hour (not in storms), and a strength by today's weather and intensity
    /// with an hourly gust of ±10 (docs/game_design.md §8.2). Pure function of seed, day,
    /// hour and weather.
    fn update_wind(&mut self, clock: GameClock) {
        let day = mix(self.wind_seed ^ clock.total_days().wrapping_mul(0x0001_0000_0001));
        let hour = mix(day ^ clock.total_hours());
        let veer = if self.current == Weather::Storm { 0 } else { (hour % 3) as u8 + 15 };
        self.wind_towards = ((day % 16) as u8 + veer) % 16;
        let base: i32 = match self.current {
            Weather::Clear => 20,
            Weather::Cloudy => 30,
            Weather::Rain => 45,
            Weather::Storm => 55 + 45 * i32::from(self.intensity.min(100)) / 100,
            Weather::Snow => 35,
            Weather::Drought => 25,
        };
        let gust = ((hour >> 8) % 21) as i32 - 10;
        self.wind_strength = (base + gust).clamp(0, 100) as u8;
    }

    /// Effects of today's weather at full intensity.
    pub fn effect<'a>(&self, rules: &'a GameRules) -> &'a WeatherEffect {
        rules.weather_effects.get(self.current)
    }

    /// Crop growth factor of today's weather in percent.
    pub fn growth_percent(&self, rules: &GameRules) -> u32 {
        scale_percent(self.effect(rules).growth_percent, self.intensity)
    }

    /// Work speed of outdoor buildings and construction sites today in percent.
    pub fn outdoor_work_percent(&self, rules: &GameRules) -> u32 {
        scale_percent(self.effect(rules).outdoor_work_percent, self.intensity)
    }

    /// Fuel demand in winter today in percent.
    pub fn heating_percent(&self, rules: &GameRules) -> u32 {
        scale_percent(self.effect(rules).heating_percent, self.intensity)
    }

    /// Disease risk today in percent.
    pub fn disease_percent(&self, rules: &GameRules) -> u32 {
        scale_percent(self.effect(rules).disease_percent, self.intensity)
    }

    /// Field moisture change today (0–255 scale).
    pub fn moisture_per_day(&self, rules: &GameRules) -> i32 {
        scale_amount(i32::from(self.effect(rules).moisture_per_day), self.intensity)
    }

    /// Share of their condition standing crops lose today, in percent.
    pub fn field_damage_percent(&self, rules: &GameRules) -> u8 {
        scale_amount(i32::from(self.effect(rules).field_damage_percent), self.intensity).clamp(0, 100) as u8
    }

    /// German description, e.g. „Starker Regen“.
    pub fn description(&self) -> String {
        let s = strength(self.intensity);
        let text = match self.current {
            Weather::Clear => ["Leicht diesig", "Heiter", "Strahlend klar"][s],
            Weather::Cloudy => ["Leicht bewölkt", "Bewölkt", "Stark bewölkt"][s],
            Weather::Rain => ["Nieselregen", "Regen", "Starker Regen"][s],
            Weather::Storm => ["Stürmisch", "Sturm", "Schwerer Sturm"][s],
            Weather::Snow => ["Leichter Schneefall", "Schneefall", "Starker Schneefall"][s],
            Weather::Drought => ["Trocken", "Dürre", "Schwere Dürre"][s],
        };
        text.to_owned()
    }

    /// Draws a day's weather for `season` (with persistence of the current weather if
    /// `persist`) and its intensity.
    fn draw(&mut self, rules: &GameRules, season: Season, persist: bool) -> (Weather, u8) {
        let climate = &rules.climate;
        let table = rules.weather.get(season);
        let known = table.iter().any(|(w, weight)| *w == self.current && *weight > 0);
        let keep = persist && known && self.rng.random_range(0..100u32) < u32::from(climate.persistence_percent);
        let weather = if keep {
            self.current
        } else {
            let total: u64 = table.iter().map(|(_, w)| u64::from(*w)).sum();
            let mut roll = if total > 0 { self.rng.random_range(0..total) } else { 0 };
            let mut chosen = table.first().map_or(Weather::Clear, |(w, _)| *w);
            for (w, weight) in table {
                if roll < u64::from(*weight) {
                    chosen = *w;
                    break;
                }
                roll -= u64::from(*weight);
            }
            chosen
        };
        let (lo, hi) = (climate.min_intensity.max(1), climate.max_intensity.max(climate.min_intensity.max(1)));
        let intensity = self.rng.random_range(lo..=hi).min(100);
        (weather, intensity)
    }

    /// Sets today's weather, keeping the streak count consistent.
    fn set_today(&mut self, weather: Weather, intensity: u8) {
        self.streak_days = if weather == self.current { self.streak_days.saturating_add(1) } else { 1 };
        self.dry_days = if weather.is_precipitation() { 0 } else { self.dry_days.saturating_add(1) };
        self.current = weather;
        self.intensity = intensity.clamp(1, 100);
    }

    /// Forces `weather` from today for `days` days (0 ends a forced weather; the normal draw
    /// resumes at the next midnight); `event` is the event whose consequence it is (`None`: a
    /// developer command).
    pub(crate) fn force(&mut self, weather: Weather, intensity: u8, days: u16, today: u64, event: Option<EventDefId>) {
        if days == 0 {
            self.forced = None;
            return;
        }
        let until_day = today.saturating_add(u64::from(days));
        self.forced = Some(ForcedWeather { weather, intensity: intensity.clamp(1, 100), until_day, event });
        self.set_today(weather, intensity);
    }

    /// Hourly update of the visual ground values.
    fn update_ground(&mut self, rules: &GameRules, season: Season) {
        let climate = &rules.climate;
        let e = self.effect(rules);
        let mut snow = scale_amount(i32::from(e.snow_per_hour), self.intensity);
        if season != Season::Winter && snow <= 0 {
            snow -= i32::from(climate.snow_melt_outside_winter_per_hour);
        }
        self.snow_cover = (i32::from(self.snow_cover) + snow).clamp(0, 100) as u8;
        let target = scale_amount(i32::from(e.ground_wetness), self.intensity).clamp(0, 100);
        let now = i32::from(self.ground_wetness);
        let next = if target > now {
            (now + i32::from(climate.wetness_rise_per_hour)).min(target)
        } else {
            (now - i32::from(climate.wetness_fall_per_hour)).max(target)
        };
        self.ground_wetness = next.clamp(0, 100) as u8;
    }

    /// Checks value ranges and the event of a forced weather (loaded saves).
    pub(crate) fn validate(&self, defs: &Defs) -> Result<(), String> {
        if !(1..=100).contains(&self.intensity) || self.snow_cover > 100 || self.ground_wetness > 100 {
            return Err("weather values out of range".into());
        }
        if self.current.is_precipitation() != (self.dry_days == 0) {
            return Err("dry days do not match today's weather".into());
        }
        if self.forced.is_some_and(|f| !(1..=100).contains(&f.intensity)) {
            return Err("forced weather intensity out of range".into());
        }
        if self.forced.and_then(|f| f.event).is_some_and(|e| e.index() >= defs.events().len()) {
            return Err("forced weather names an unknown event".into());
        }
        if self.wind_towards >= 16 || self.wind_strength > 100 {
            return Err("wind out of range".into());
        }
        Ok(())
    }
}

/// Work speed of `def` from season and weather in permille: winter work speed for every
/// building, the weather's outdoor work speed for outdoor buildings, and the seasonal factor of
/// fishing, pastures and bees.
pub fn work_permille(state: &GameState, defs: &Defs, def: &BuildingDef) -> u32 {
    let rules = defs.rules();
    let season = state.clock.season();
    let mut permille: u64 = 1000;
    if season == Season::Winter {
        permille = permille * u64::from(rules.winter_work_speed_percent) / 100;
    }
    if def.outdoor {
        permille = permille * u64::from(state.weather.outdoor_work_percent(rules)) / 100;
    }
    let gathering = rules.season_gathering.get(season);
    let seasonal = match def.production.as_ref().and_then(|p| p.gathering.as_ref()) {
        Some(Gathering::Deposit { kind: DepositKind::Fish, .. }) => gathering.fishing_percent,
        Some(Gathering::Pasture { .. }) => gathering.pasture_percent,
        Some(Gathering::Bees { .. }) => gathering.bees_percent,
        _ => 100,
    };
    permille = permille * u64::from(seasonal) / 100;
    u32::try_from(permille).unwrap_or(u32::MAX)
}

/// Work speed of construction sites from season and weather in percent.
pub fn construction_percent(state: &GameState, defs: &Defs) -> u32 {
    let rules = defs.rules();
    let winter = if state.clock.season() == Season::Winter { u32::from(rules.winter_work_speed_percent) } else { 100 };
    winter * state.weather.outdoor_work_percent(rules) / 100
}

/// Pipeline step 2 (weather part, right after the calendar): draws the weather at midnight and
/// updates snow cover and ground wetness every hour.
pub(crate) fn run_weather(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let rules = defs.rules();
    let clock = state.clock;
    if clock.is_new_day() {
        new_day(state, rules, clock, events);
    }
    if clock.is_new_hour() {
        state.weather.update_ground(rules, clock.season());
        state.weather.update_wind(clock);
    }
}

fn new_day(state: &mut GameState, rules: &GameRules, clock: GameClock, events: &mut Vec<SimEvent>) {
    let season = clock.season();
    let today = clock.total_days();
    let w = &mut state.weather;
    if clock.is_new_season() && season == Season::Winter {
        w.winter_notes = WinterNotes::default();
    }
    let previous = w.current;
    let (weather, intensity) = match w.forced {
        Some(f) if today < f.until_day => (f.weather, f.intensity),
        _ => {
            w.forced = None;
            w.draw(rules, season, true)
        }
    };
    w.set_today(weather, intensity);
    events.push(SimEvent::WeatherChanged { weather, intensity });
    let climate = &rules.climate;
    let mut notes: Vec<(Severity, &str)> = Vec::new();
    match weather {
        // Once per storm (a storm of several days is announced on its first day).
        Weather::Storm if intensity >= climate.storm_warning_intensity && w.streak_days == 1 => {
            notes.push((Severity::Warning, "Ein schwerer Sturm zieht auf – Felder nehmen Schaden, die Arbeit im Freien ruht fast."));
        }
        Weather::Drought if w.streak_days == 1 => {
            notes.push((Severity::Warning, "Dürre! Ohne Regen trocknen die Felder aus und wachsen kaum."));
        }
        Weather::Rain | Weather::Storm if previous == Weather::Drought => {
            notes.push((Severity::Good, "Endlich Regen – die Dürre ist vorbei."));
        }
        _ => {}
    }
    if season == Season::Winter && weather == Weather::Snow {
        if !w.winter_notes.first_snow {
            w.winter_notes.first_snow = true;
            notes.push((Severity::Info, "Der erste Schnee fällt."));
        }
        if intensity >= climate.hard_frost_intensity && !w.winter_notes.hard_frost {
            w.winter_notes.hard_frost = true;
            notes.push((Severity::Warning, "Strenger Frost: Die Bewohner verheizen deutlich mehr Brennholz."));
        }
    }
    for (severity, text) in notes {
        events.push(SimEvent::Notification(Notification::new(severity, text).with_kind(NotificationKind::Weather)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{defs, flat_state, run};
    use crate::time::{TICKS_PER_DAY, TICKS_PER_SEASON};

    #[test]
    fn scaling_moves_towards_neutral() {
        assert_eq!(scale_percent(50, 100), 50);
        assert_eq!(scale_percent(50, 50), 75);
        assert_eq!(scale_percent(130, 50), 115);
        assert_eq!(scale_percent(130, 0), 100);
        assert_eq!(scale_amount(-30, 50), -15);
        assert_eq!(scale_amount(35, 20), 7);
    }

    #[test]
    fn daily_draw_follows_the_season_tables_and_is_deterministic() {
        let d = defs();
        let mut a = WeatherState::new(d, 7, GameClock::new(), 0);
        let mut b = WeatherState::new(d, 7, GameClock::new(), 0);
        let mut counts = std::collections::BTreeMap::new();
        for day in 0..2000u64 {
            let season = Season::from_index(day / 6);
            let (w, i) = a.draw(d.rules(), season, true);
            assert_eq!((w, i), b.draw(d.rules(), season, true));
            a.set_today(w, i);
            b.set_today(w, i);
            assert!(d.rules().weather.get(season).iter().any(|(x, weight)| *x == w && *weight > 0), "{w:?} in {season:?}");
            assert!((20..=100).contains(&i));
            *counts.entry((season, w)).or_insert(0u32) += 1;
        }
        // Snow only in winter, drought only in summer; every listed weather occurs.
        assert!(counts.keys().all(|(s, w)| *w != Weather::Snow || *s == Season::Winter));
        assert!(counts.keys().all(|(s, w)| *w != Weather::Drought || *s == Season::Summer));
        assert!(counts.contains_key(&(Season::Summer, Weather::Drought)));
        assert!(counts.contains_key(&(Season::Winter, Weather::Snow)));
        assert_ne!(WeatherState::new(d, 7, GameClock::new(), 0).rng, WeatherState::new(d, 8, GameClock::new(), 0).rng);
    }

    #[test]
    fn midnight_draws_a_new_weather_and_reports_it() {
        let d = defs();
        let mut state = flat_state(16);
        state.weather = WeatherState::new(d, 3, GameClock::new(), 0);
        let events = run(&mut state, TICKS_PER_DAY);
        let changes: Vec<_> = events.iter().filter(|e| matches!(e, SimEvent::WeatherChanged { .. })).collect();
        assert_eq!(changes.len(), 1, "one draw at midnight");
        assert!(state.weather().validate(d).is_ok());
    }

    #[test]
    fn forced_weather_holds_and_snow_builds_up_and_melts() {
        let mut state = flat_state(16);
        state.clock = GameClock::at_tick(3 * TICKS_PER_SEASON);
        let today = state.clock.total_days();
        state.weather.force(Weather::Snow, 100, 2, today, None);
        let events = run(&mut state, TICKS_PER_DAY);
        assert_eq!(state.weather().current(), Weather::Snow);
        assert!(state.weather().snow_cover() > 80, "a day of heavy snow covers the ground: {}", state.weather().snow_cover());
        assert!(events.iter().any(|e| e.notification().is_some_and(|n| n.text.contains("erste Schnee"))));
        assert!(events.iter().any(|e| e.notification().is_some_and(|n| n.text.contains("Strenger Frost"))));
        // After the forced days the normal draw resumes; outside winter the snow melts fast.
        state.weather.force(Weather::Clear, 100, 0, today, None);
        state.clock = GameClock::at_tick(4 * TICKS_PER_SEASON + 6 * crate::time::TICKS_PER_HOUR);
        run(&mut state, TICKS_PER_DAY);
        assert_eq!(state.weather().snow_cover(), 0);
        assert!(state.weather().forced().is_none());
    }

    #[test]
    fn rain_wets_the_ground_and_drought_dries_it() {
        let d = defs();
        let mut w = WeatherState::fixed(Weather::Rain, 100);
        for _ in 0..12 {
            w.update_ground(d.rules(), Season::Spring);
        }
        assert_eq!(w.ground_wetness(), 100);
        w.set_today(Weather::Drought, 100);
        w.update_ground(d.rules(), Season::Summer);
        assert_eq!(w.ground_wetness(), 97, "falls by at most 3 per hour");
    }

    #[test]
    fn effects_scale_with_intensity() {
        let d = defs();
        let r = d.rules();
        let storm = WeatherState::fixed(Weather::Storm, 100);
        assert_eq!(storm.outdoor_work_percent(r), 50);
        assert_eq!(storm.field_damage_percent(r), 20);
        let mild = WeatherState::fixed(Weather::Storm, 50);
        assert_eq!(mild.outdoor_work_percent(r), 75);
        assert_eq!(mild.field_damage_percent(r), 10);
        assert_eq!(WeatherState::fixed(Weather::Drought, 100).moisture_per_day(r), -35);
        assert_eq!(WeatherState::fixed(Weather::Snow, 100).heating_percent(r), 125);
        assert_eq!(WeatherState::fixed(Weather::Rain, 80).description(), "Starker Regen");
    }

    #[test]
    fn work_speed_combines_winter_weather_and_nature() {
        let d = defs();
        let mut state = flat_state(16);
        let def = |key: &str| d.building_by_key(key).unwrap();
        assert_eq!(work_permille(&state, d, def("sawmill")), 1000);
        state.weather = WeatherState::fixed(Weather::Rain, 100);
        assert_eq!(work_permille(&state, d, def("sawmill")), 1000, "indoor work ignores rain");
        assert_eq!(work_permille(&state, d, def("woodcutter")), 850);
        state.clock = GameClock::at_tick(3 * TICKS_PER_SEASON);
        state.weather = WeatherState::fixed(Weather::Clear, 100);
        assert_eq!(work_permille(&state, d, def("sawmill")), 850, "winter slows everybody");
        assert_eq!(work_permille(&state, d, def("fishery")), 850 * 60 / 100);
        assert_eq!(work_permille(&state, d, def("apiary")), 0, "bees rest in winter");
        assert_eq!(construction_percent(&state, d), 85);
    }
}
