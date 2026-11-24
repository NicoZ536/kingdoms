//! Frame timing and the fixed-timestep simulation clock.
//!
//! Rendering runs at a variable rate (as fast as vsync / the frame limiter allows), while the
//! simulation advances in fixed ticks (default 20 per second at normal speed). The two are
//! decoupled through an accumulator: each frame adds the scaled real frame time and runs as many
//! whole ticks as fit. The remainder is exposed as an interpolation factor ([`Time::alpha`]) so
//! renderers can blend between the previous and current simulation state.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Largest real frame delta that is fed into the simulation. Longer hitches (window drag,
/// breakpoint, sleep) are clamped so the game does not try to "catch up" minutes of time.
pub const MAX_FRAME_DELTA: f64 = 0.25;

/// Simulation speed selected by the player.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameSpeed {
    Paused,
    #[default]
    Normal,
    Double,
    Quadruple,
}

impl GameSpeed {
    /// Multiplier applied to real time before it is fed into the fixed-step accumulator.
    pub fn multiplier(self) -> f64 {
        match self {
            GameSpeed::Paused => 0.0,
            GameSpeed::Normal => 1.0,
            GameSpeed::Double => 2.0,
            GameSpeed::Quadruple => 4.0,
        }
    }

    pub fn is_paused(self) -> bool {
        self == GameSpeed::Paused
    }

    /// Next faster speed (paused → normal → 2× → 4×, saturating).
    pub fn faster(self) -> Self {
        match self {
            GameSpeed::Paused => GameSpeed::Normal,
            GameSpeed::Normal => GameSpeed::Double,
            GameSpeed::Double | GameSpeed::Quadruple => GameSpeed::Quadruple,
        }
    }

    /// Next slower speed (4× → 2× → normal → paused, saturating).
    pub fn slower(self) -> Self {
        match self {
            GameSpeed::Quadruple => GameSpeed::Double,
            GameSpeed::Double => GameSpeed::Normal,
            GameSpeed::Normal | GameSpeed::Paused => GameSpeed::Paused,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GameSpeed::Paused => "Pause",
            GameSpeed::Normal => "1×",
            GameSpeed::Double => "2×",
            GameSpeed::Quadruple => "4×",
        }
    }
}

/// Pure fixed-timestep accumulator. Separated from [`Time`] so it can be unit tested without
/// touching the wall clock.
#[derive(Clone, Debug)]
pub struct FixedTimestep {
    tick_seconds: f64,
    accumulator: f64,
    max_ticks_per_frame: u32,
    /// Ticks that were dropped because the frame budget was exceeded (diagnostics).
    dropped_ticks: u64,
}

impl FixedTimestep {
    /// Creates a timestep running `tick_rate` ticks per second at normal speed.
    pub fn new(tick_rate: u32, max_ticks_per_frame: u32) -> Self {
        assert!(tick_rate > 0, "tick rate must be positive");
        Self {
            tick_seconds: 1.0 / f64::from(tick_rate),
            accumulator: 0.0,
            max_ticks_per_frame: max_ticks_per_frame.max(1),
            dropped_ticks: 0,
        }
    }

    /// Real seconds per tick at normal speed.
    pub fn tick_seconds(&self) -> f64 {
        self.tick_seconds
    }

    /// Adds `real_dt` seconds scaled by `speed` and returns how many ticks to run now.
    ///
    /// If more ticks are due than `max_ticks_per_frame`, the surplus is discarded (the game slows
    /// down instead of entering a "spiral of death").
    pub fn advance(&mut self, real_dt: f64, speed: f64) -> u32 {
        let dt = real_dt.clamp(0.0, MAX_FRAME_DELTA) * speed.max(0.0);
        self.accumulator += dt;
        // Small epsilon so accumulated floating point error does not swallow a tick.
        let due = (self.accumulator / self.tick_seconds + 1e-9).floor();
        let due_ticks = due as u64;
        let ticks = due_ticks.min(u64::from(self.max_ticks_per_frame)) as u32;
        self.accumulator = (self.accumulator - f64::from(ticks) * self.tick_seconds).max(0.0);
        if due_ticks > u64::from(ticks) {
            self.dropped_ticks += due_ticks - u64::from(ticks);
            // Keep at most one partial tick so interpolation stays meaningful.
            self.accumulator = self.accumulator.min(self.tick_seconds * 0.999);
        }
        ticks
    }

    /// Fraction of the next tick that has already elapsed, in `[0, 1)`.
    pub fn alpha(&self) -> f32 {
        (self.accumulator / self.tick_seconds).clamp(0.0, 1.0) as f32
    }

    pub fn dropped_ticks(&self) -> u64 {
        self.dropped_ticks
    }

    /// Clears accumulated time (e.g. after loading a save).
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
    }
}

/// Engine time state, updated once per rendered frame.
#[derive(Clone, Debug)]
pub struct Time {
    startup: Instant,
    last_frame: Instant,
    frame_delta: f64,
    elapsed: f64,
    frame_count: u64,
    speed: GameSpeed,
    fixed: FixedTimestep,
    ticks_this_frame: u32,
    total_ticks: u64,
    /// Every frame lasts exactly this long instead of the measured time (scripted captures).
    fixed_frame: Option<f64>,
}

impl Time {
    pub fn new(tick_rate: u32, max_ticks_per_frame: u32) -> Self {
        let now = Instant::now();
        Self {
            startup: now,
            last_frame: now,
            frame_delta: 0.0,
            elapsed: 0.0,
            frame_count: 0,
            speed: GameSpeed::Normal,
            fixed: FixedTimestep::new(tick_rate, max_ticks_per_frame),
            ticks_this_frame: 0,
            total_ticks: 0,
            fixed_frame: None,
        }
    }

    /// Makes every frame last exactly `seconds` (game time, animations, particles and the
    /// shader clock then depend on the frame count only, not on how fast frames render: for
    /// screenshots that must match pixel for pixel); `None` measures real time again.
    pub fn set_fixed_frame(&mut self, seconds: Option<f64>) {
        self.fixed_frame = seconds.filter(|s| s.is_finite() && *s > 0.0);
    }

    /// True while frames last a fixed time ([`Time::set_fixed_frame`]).
    pub fn is_fixed_frame(&self) -> bool {
        self.fixed_frame.is_some()
    }

    /// The fixed frame time, if set.
    pub fn fixed_frame(&self) -> Option<f64> {
        self.fixed_frame
    }

    /// Starts a new frame: measures the real delta and computes how many simulation ticks are due.
    pub(crate) fn begin_frame(&mut self) -> u32 {
        let now = Instant::now();
        let raw = match self.fixed_frame {
            Some(fixed) => fixed,
            None => now.duration_since(self.last_frame).as_secs_f64(),
        };
        self.last_frame = now;
        self.frame_delta = raw.min(MAX_FRAME_DELTA);
        self.elapsed = match self.fixed_frame {
            Some(fixed) => self.elapsed + fixed,
            None => now.duration_since(self.startup).as_secs_f64(),
        };
        self.frame_count += 1;
        self.ticks_this_frame = self.fixed.advance(raw, self.speed.multiplier());
        self.ticks_this_frame
    }

    pub(crate) fn record_tick(&mut self) {
        self.total_ticks += 1;
    }

    /// Real seconds since the previous frame (clamped to [`MAX_FRAME_DELTA`]).
    pub fn delta(&self) -> f32 {
        self.frame_delta as f32
    }

    /// Real seconds since engine start.
    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }

    pub fn elapsed_duration(&self) -> Duration {
        Duration::from_secs_f64(self.elapsed)
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    pub fn speed(&self) -> GameSpeed {
        self.speed
    }

    pub fn set_speed(&mut self, speed: GameSpeed) {
        if self.speed != speed {
            log::debug!("game speed: {:?} -> {:?}", self.speed, speed);
        }
        self.speed = speed;
    }

    /// Interpolation factor between the last two simulation states. While paused nothing
    /// accumulates, so it stays where it was: interpolated positions neither jump on pausing
    /// nor on resuming.
    pub fn alpha(&self) -> f32 {
        self.fixed.alpha()
    }

    /// Fixed simulation step length in real seconds at normal speed.
    pub fn tick_seconds(&self) -> f64 {
        self.fixed.tick_seconds()
    }

    pub fn ticks_this_frame(&self) -> u32 {
        self.ticks_this_frame
    }

    /// Total simulation ticks executed since start (engine-side counter, not saved).
    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }

    pub fn dropped_ticks(&self) -> u64 {
        self.fixed.dropped_ticks()
    }

    /// Discards accumulated simulation time (used after loading or long blocking operations).
    pub fn reset_accumulator(&mut self) {
        self.fixed.reset();
        self.last_frame = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_is_frozen_while_paused() {
        let mut time = Time::new(20, 8);
        let mut fixed = Time::new(20, 8);
        fixed.set_fixed_frame(Some(0.1));
        assert!(fixed.is_fixed_frame());
        assert_eq!(fixed.begin_frame(), 2, "0.1 s are two ticks at 20 Hz");
        assert_eq!(fixed.begin_frame(), 2);
        assert!((fixed.elapsed() - 0.2).abs() < 1e-9 && (fixed.delta() - 0.1).abs() < 1e-6);
        fixed.set_fixed_frame(Some(-1.0));
        assert!(!fixed.is_fixed_frame());
        // A third of a tick has elapsed.
        time.fixed.advance(1.0 / 60.0, 1.0);
        let running = time.alpha();
        assert!(running > 0.3 && running < 0.4, "{running}");
        time.set_speed(GameSpeed::Paused);
        assert_eq!(time.alpha(), running, "pausing does not jump");
        assert_eq!(time.fixed.advance(0.5, GameSpeed::Paused.multiplier()), 0);
        assert_eq!(time.alpha(), running, "paused time does not move");
        time.set_speed(GameSpeed::Normal);
        assert_eq!(time.alpha(), running, "resuming does not jump");
    }

    #[test]
    fn runs_expected_ticks_at_normal_speed() {
        let mut ts = FixedTimestep::new(20, 8);
        let mut total = 0;
        // 60 frames of 1/60 s = 1 second -> 20 ticks
        for _ in 0..60 {
            total += ts.advance(1.0 / 60.0, 1.0);
        }
        assert_eq!(total, 20);
    }

    #[test]
    fn speed_multiplier_scales_ticks() {
        let mut ts = FixedTimestep::new(20, 16);
        let mut total = 0;
        for _ in 0..60 {
            total += ts.advance(1.0 / 60.0, 4.0);
        }
        assert_eq!(total, 80);
    }

    #[test]
    fn paused_runs_no_ticks() {
        let mut ts = FixedTimestep::new(20, 8);
        for _ in 0..100 {
            assert_eq!(ts.advance(0.1, 0.0), 0);
        }
    }

    #[test]
    fn caps_ticks_per_frame_and_drops_surplus() {
        let mut ts = FixedTimestep::new(20, 4);
        // 0.25 s at 4x = 1 s = 20 ticks due, but only 4 allowed.
        let ticks = ts.advance(0.25, 4.0);
        assert_eq!(ticks, 4);
        assert!(ts.dropped_ticks() > 0);
        assert!(ts.alpha() < 1.0);
    }

    #[test]
    fn huge_hitch_is_clamped() {
        let mut ts = FixedTimestep::new(20, 100);
        let ticks = ts.advance(10.0, 1.0);
        // clamped to MAX_FRAME_DELTA (0.25 s) -> 5 ticks
        assert_eq!(ticks, 5);
    }

    #[test]
    fn speed_cycle() {
        assert_eq!(GameSpeed::Paused.faster(), GameSpeed::Normal);
        assert_eq!(GameSpeed::Quadruple.faster(), GameSpeed::Quadruple);
        assert_eq!(GameSpeed::Normal.slower(), GameSpeed::Paused);
    }
}
