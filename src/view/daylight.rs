//! Day and night – presentation only.
//!
//! The light of the world follows the game time: night, dawn at sunrise, warm morning light,
//! white daylight, a golden hour before sunset, dusk at sunset and back to night, with smooth
//! transitions between these key colours. The times of sunrise and sunset depend on the season
//! ([`SunTimes::of`]; the simulation knows no day length, the sim's working hours stay the
//! same). They change at midnight, when it is night anyway, so the light never jumps.
//!
//! The light multiplies the weather's ambient light and applies to the world only (sprites
//! without `FLAG_EMISSIVE`), never to the UI. [`Daylight::night`] (0 by day, 1 at night)
//! switches the window lights and the night ambience.

use kingdoms_sim::time::{GameClock, Season, TICKS_PER_DAY, TICKS_PER_HOUR};

/// Light at night: moonlit blue, dark but readable.
pub const NIGHT: [f32; 3] = [0.13, 0.16, 0.31];
/// At sunrise: pale violet-pink.
pub const DAWN: [f32; 3] = [0.6, 0.48, 0.56];
/// Early morning: warm.
pub const MORNING: [f32; 3] = [1.0, 0.88, 0.74];
/// Full daylight (neutral).
pub const DAY: [f32; 3] = [1.0, 1.0, 1.0];
/// Golden hour before sunset.
pub const GOLDEN: [f32; 3] = [1.0, 0.84, 0.62];
/// At sunset: red-orange.
pub const DUSK: [f32; 3] = [0.8, 0.52, 0.45];

/// Hours of the key colours relative to sunrise (`Rise`) and sunset (`Set`).
const KEYS: [(Anchor, f32, [f32; 3]); 8] = [
    (Anchor::Rise, -1.0, NIGHT),
    (Anchor::Rise, 0.0, DAWN),
    (Anchor::Rise, 1.0, MORNING),
    (Anchor::Rise, 2.5, DAY),
    (Anchor::Set, -2.0, DAY),
    (Anchor::Set, -0.5, GOLDEN),
    (Anchor::Set, 0.0, DUSK),
    (Anchor::Set, 1.25, NIGHT),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Anchor {
    Rise,
    Set,
}

/// Sunrise and sunset in hours of the day.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunTimes {
    pub sunrise: f32,
    pub sunset: f32,
}

impl SunTimes {
    /// Sun times of a season: long summer days, short winter days.
    pub const fn of(season: Season) -> Self {
        match season {
            Season::Spring => SunTimes { sunrise: 5.5, sunset: 20.0 },
            Season::Summer => SunTimes { sunrise: 4.75, sunset: 21.25 },
            Season::Autumn => SunTimes { sunrise: 6.25, sunset: 19.0 },
            Season::Winter => SunTimes { sunrise: 7.75, sunset: 16.5 },
        }
    }

    /// Hours of daylight.
    pub fn day_length(&self) -> f32 {
        self.sunset - self.sunrise
    }
}

/// Smooth interpolation weight between two keys.
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Light colour (linear RGB multiplier) at `hour` (0–24, fractional) for the sun times `sun`.
pub fn light_at(hour: f32, sun: SunTimes) -> [f32; 3] {
    let at = |(anchor, offset, _): &(Anchor, f32, [f32; 3])| match anchor {
        Anchor::Rise => sun.sunrise + offset,
        Anchor::Set => sun.sunset + offset,
    };
    let hour = hour.rem_euclid(24.0);
    let first = &KEYS[0];
    let last = &KEYS[KEYS.len() - 1];
    if hour <= at(first) || hour >= at(last) {
        return NIGHT;
    }
    for pair in KEYS.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let (ta, tb) = (at(a), at(b));
        if hour <= tb {
            let t = if tb > ta { smooth((hour - ta) / (tb - ta)) } else { 1.0 };
            return std::array::from_fn(|i| a.2[i] + (b.2[i] - a.2[i]) * t);
        }
    }
    NIGHT
}

/// Perceived brightness of a light colour.
pub fn luma(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

/// How much night it is for light colour `c`: 0 in daylight, 1 at night.
pub fn night_of(c: [f32; 3]) -> f32 {
    ((luma(DAY) - luma(c)) / (luma(DAY) - luma(NIGHT))).clamp(0.0, 1.0)
}

/// The light of one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Daylight {
    /// Hour of the day (fractional).
    pub hour: f32,
    pub sun: SunTimes,
    /// Light colour (linear RGB multiplier of the world).
    pub light: [f32; 3],
    /// 0 by day, 1 at night.
    pub night: f32,
}

impl Daylight {
    /// Light at game time `ticks` (whole ticks plus the frame's fraction).
    pub fn at(ticks: f64) -> Self {
        let whole = ticks.max(0.0).floor();
        let clock = GameClock::at_tick(whole as u64);
        let fraction = (ticks.max(0.0) - whole) as f32;
        let hour = (clock.tick_of_day() as f32 + fraction) / TICKS_PER_HOUR as f32;
        let sun = SunTimes::of(clock.season());
        let light = light_at(hour, sun);
        Daylight { hour: hour.rem_euclid(TICKS_PER_DAY as f32 / TICKS_PER_HOUR as f32), sun, light, night: night_of(light) }
    }

    /// Strength of the window lights (0–1): they come on at dusk and go out in the morning.
    pub fn window_glow(&self) -> f32 {
        smooth((self.night - 0.15) / 0.45)
    }

    /// German name of the time of day (debug window).
    pub fn phase_name(&self) -> &'static str {
        let (rise, set) = (self.sun.sunrise, self.sun.sunset);
        let h = self.hour;
        if h < rise - 1.0 || h >= set + 1.25 {
            "Nacht"
        } else if h < rise + 1.0 {
            "Morgendämmerung"
        } else if h < set - 2.0 {
            "Tag"
        } else if h < set {
            "Abendsonne"
        } else {
            "Abenddämmerung"
        }
    }
}

/// Whether the residents of a house still have their lights on at `hour`: everybody until
/// 22:00, then the house goes dark at a bedtime between 22:00 and midnight and lights up again
/// at a waking time between 4:30 and 6:00 (both from `seed`, a value in `0..1` per house); one
/// house in ten stays up all night.
pub fn residents_awake(hour: f32, seed: f32) -> bool {
    if seed < 0.1 {
        return true;
    }
    let bedtime = 22.0 + 2.0 * seed;
    let waking = 4.5 + 1.5 * ((seed * 7.0).fract());
    let h = hour.rem_euclid(24.0);
    !(h >= bedtime || h < waking)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn noon_is_daylight_and_midnight_is_night_in_every_season() {
        for season in Season::ALL {
            let sun = SunTimes::of(season);
            assert!(close(light_at(13.0, sun), DAY), "{season:?}");
            assert!(close(light_at(0.0, sun), NIGHT), "{season:?}");
            assert!(close(light_at(23.9, sun), NIGHT), "{season:?}");
            assert!(close(light_at(sun.sunrise, sun), DAWN));
            assert!(close(light_at(sun.sunset, sun), DUSK));
        }
    }

    #[test]
    fn light_changes_smoothly_through_the_day() {
        for season in Season::ALL {
            let sun = SunTimes::of(season);
            let mut last = light_at(0.0, sun);
            for step in 1..=24 * 60 {
                let c = light_at(step as f32 / 60.0, sun);
                // At most a few percent per game minute: no visible jumps.
                for i in 0..3 {
                    assert!((c[i] - last[i]).abs() < 0.03, "{season:?} at minute {step}: {last:?} -> {c:?}");
                }
                last = c;
            }
        }
    }

    #[test]
    fn mornings_brighten_and_evenings_darken() {
        let sun = SunTimes::of(Season::Spring);
        let mut last = luma(light_at(sun.sunrise - 1.5, sun));
        for step in 0..=40 {
            let l = luma(light_at(sun.sunrise - 1.0 + step as f32 * 0.1, sun));
            assert!(l >= last - 1e-5, "morning got darker at step {step}");
            last = l;
        }
        let mut last = luma(light_at(sun.sunset - 2.0, sun));
        for step in 0..=40 {
            let l = luma(light_at(sun.sunset - 2.0 + step as f32 * 0.1, sun));
            assert!(l <= last + 1e-5, "evening got brighter at step {step}");
            last = l;
        }
    }

    #[test]
    fn winter_days_are_short_and_summer_days_long() {
        let winter = SunTimes::of(Season::Winter);
        let summer = SunTimes::of(Season::Summer);
        assert!(winter.day_length() < 10.0 && summer.day_length() > 15.0);
        // 17:30: night in winter, broad daylight in summer.
        assert!(night_of(light_at(17.5, winter)) > 0.9);
        assert!(night_of(light_at(17.5, summer)) < 0.01);
        // The game starts at 06:00 in spring with the morning light, not at night.
        let start = Daylight::at(0.0);
        assert!((start.hour - 6.0).abs() < 1e-4 && start.night < 0.5, "{start:?}");
    }

    #[test]
    fn night_value_and_window_glow() {
        assert_eq!(night_of(DAY), 0.0);
        assert_eq!(night_of(NIGHT), 1.0);
        let noon = Daylight::at((TICKS_PER_HOUR * 6) as f64);
        assert!((noon.hour - 12.0).abs() < 1e-4 && noon.night == 0.0 && noon.window_glow() == 0.0);
        let midnight = Daylight::at((TICKS_PER_HOUR * 18) as f64 + 0.5);
        assert!(midnight.night == 1.0 && (midnight.window_glow() - 1.0).abs() < 1e-6);
        assert_eq!(midnight.phase_name(), "Nacht");
        assert_eq!(noon.phase_name(), "Tag");
    }

    #[test]
    fn houses_go_dark_late_at_night() {
        for seed in [0.15, 0.5, 0.95] {
            assert!(residents_awake(20.0, seed) && residents_awake(21.9, seed) && residents_awake(7.0, seed));
            assert!(!residents_awake(2.0, seed) && !residents_awake(4.4, seed));
        }
        assert!(residents_awake(2.0, 0.05), "night owls keep their lights on");
    }
}
