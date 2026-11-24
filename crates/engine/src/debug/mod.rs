//! Frame metrics and lightweight profiling sections (shown in the debug HUD).

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

const HISTORY: usize = 240;

/// Exponentially smoothed timing of a named section.
#[derive(Clone, Copy, Debug, Default)]
pub struct SectionTiming {
    /// Last measured value in milliseconds.
    pub last_ms: f32,
    /// Smoothed value in milliseconds.
    pub avg_ms: f32,
    /// Peak over the last ~4 seconds (decaying).
    pub peak_ms: f32,
}

/// Collects frame times and per-section timings.
#[derive(Debug, Default)]
pub struct FrameMetrics {
    frame_ms: VecDeque<f32>,
    sections: BTreeMap<&'static str, SectionTiming>,
    counters: BTreeMap<&'static str, f64>,
    /// Every sample per section while a profile capture runs ([`FrameMetrics::begin_capture`]).
    capture: Option<Capture>,
}

/// Samples of a profile capture.
#[derive(Debug, Default)]
struct Capture {
    frames: usize,
    frame_ms: Vec<f32>,
    sections: BTreeMap<&'static str, Vec<f32>>,
}

/// Statistics of one section over a profile capture, in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionStats {
    /// Frames in which the section ran.
    pub samples: usize,
    pub mean: f32,
    pub median: f32,
    pub p95: f32,
    pub max: f32,
}

impl SectionStats {
    /// Statistics of `samples` (`None` if empty).
    pub fn of(samples: &[f32]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f32::total_cmp);
        let at = |q: f32| sorted[((sorted.len() - 1) as f32 * q).round() as usize];
        Some(Self {
            samples: sorted.len(),
            mean: sorted.iter().sum::<f32>() / sorted.len() as f32,
            median: at(0.5),
            p95: at(0.95),
            max: sorted[sorted.len() - 1],
        })
    }
}

/// Result of a profile capture: frame time and every section that ran.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileReport {
    /// Frames captured.
    pub frames: usize,
    /// Wall time between frames.
    pub frame: Option<SectionStats>,
    /// Per section, sorted by name.
    pub sections: Vec<(&'static str, SectionStats)>,
}

impl ProfileReport {
    /// A plain-text table (one line per section) for the log.
    pub fn table(&self) -> String {
        let mut out = format!("profile over {} frames (ms): {:<28} {:>8} {:>8} {:>8} {:>8}\n", self.frames, "section", "mean", "median", "p95", "max");
        let mut line = |name: &str, s: &SectionStats| {
            out.push_str(&format!("  {name:<28} {:>8.3} {:>8.3} {:>8.3} {:>8.3}  ({} samples)\n", s.mean, s.median, s.p95, s.max, s.samples));
        };
        if let Some(f) = &self.frame {
            line("frame interval", f);
        }
        for (name, s) in &self.sections {
            line(name, s);
        }
        out
    }
}

impl FrameMetrics {
    pub(crate) fn record_frame(&mut self, dt: Duration) {
        if self.frame_ms.len() >= HISTORY {
            self.frame_ms.pop_front();
        }
        let ms = dt.as_secs_f32() * 1000.0;
        self.frame_ms.push_back(ms);
        if let Some(c) = self.capture.as_mut() {
            c.frames += 1;
            c.frame_ms.push(ms);
        }
        for timing in self.sections.values_mut() {
            timing.peak_ms *= 0.995;
        }
    }

    /// Records the duration of a named section for this frame (accumulates if called repeatedly
    /// within a frame is not needed — the last value wins, the average is smoothed).
    pub fn record_section(&mut self, name: &'static str, elapsed: Duration) {
        let ms = elapsed.as_secs_f32() * 1000.0;
        if let Some(c) = self.capture.as_mut() {
            c.sections.entry(name).or_default().push(ms);
        }
        let t = self.sections.entry(name).or_default();
        t.last_ms = ms;
        t.avg_ms = if t.avg_ms == 0.0 { ms } else { t.avg_ms * 0.95 + ms * 0.05 };
        t.peak_ms = t.peak_ms.max(ms);
    }

    /// Starts keeping every sample of every section (and the frame times) until
    /// [`FrameMetrics::end_capture`]; a running capture starts over.
    pub fn begin_capture(&mut self) {
        self.capture = Some(Capture::default());
    }

    /// True while a profile capture runs.
    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }

    /// Frames recorded by the running capture.
    pub fn captured_frames(&self) -> usize {
        self.capture.as_ref().map_or(0, |c| c.frames)
    }

    /// Ends the capture and returns its statistics (`None` if none ran).
    pub fn end_capture(&mut self) -> Option<ProfileReport> {
        let c = self.capture.take()?;
        let sections = c.sections.iter().filter_map(|(name, v)| SectionStats::of(v).map(|s| (*name, s))).collect();
        Some(ProfileReport { frames: c.frames, frame: SectionStats::of(&c.frame_ms), sections })
    }

    /// Times the closure as section `name`.
    pub fn time<R>(&mut self, name: &'static str, f: impl FnOnce() -> R) -> R {
        let start = Instant::now();
        let r = f();
        self.record_section(name, start.elapsed());
        r
    }

    /// Sets a numeric counter (entities, chunks, ...) for display.
    pub fn set_counter(&mut self, name: &'static str, value: f64) {
        self.counters.insert(name, value);
    }

    pub fn sections(&self) -> impl Iterator<Item = (&'static str, &SectionTiming)> {
        self.sections.iter().map(|(k, v)| (*k, v))
    }

    pub fn counters(&self) -> impl Iterator<Item = (&'static str, f64)> + '_ {
        self.counters.iter().map(|(k, v)| (*k, *v))
    }

    /// Frame time history in milliseconds (oldest first).
    pub fn frame_history(&self) -> impl Iterator<Item = f32> + '_ {
        self.frame_ms.iter().copied()
    }

    pub fn average_frame_ms(&self) -> f32 {
        if self.frame_ms.is_empty() {
            return 0.0;
        }
        self.frame_ms.iter().sum::<f32>() / self.frame_ms.len() as f32
    }

    pub fn fps(&self) -> f32 {
        let avg = self.average_frame_ms();
        if avg > 0.0 { 1000.0 / avg } else { 0.0 }
    }

    /// Average of the slowest 1 % of recent frames, in milliseconds.
    pub fn worst_frame_ms(&self) -> f32 {
        if self.frame_ms.is_empty() {
            return 0.0;
        }
        let mut v: Vec<f32> = self.frame_ms.iter().copied().collect();
        v.sort_by(|a, b| b.total_cmp(a));
        let n = (v.len() / 100).max(1);
        v[..n].iter().sum::<f32>() / n as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_keep_every_sample_and_summarise_them() {
        let mut m = FrameMetrics::default();
        m.record_section("map", Duration::from_millis(50));
        assert!(m.end_capture().is_none());
        m.begin_capture();
        for ms in 1..=20u64 {
            m.record_frame(Duration::from_millis(10));
            m.record_section("map", Duration::from_millis(ms));
        }
        m.record_section("rare", Duration::from_millis(4));
        assert!(m.capturing() && m.captured_frames() == 20);
        let Some(report) = m.end_capture() else { panic!("no report") };
        assert!(!m.capturing());
        assert_eq!(report.frames, 20);
        let map = report.sections.iter().find(|(n, _)| *n == "map").map(|(_, s)| *s);
        let Some(map) = map else { panic!("map missing") };
        assert_eq!(map.samples, 20, "the sample before the capture is not counted");
        assert!((map.mean - 10.5).abs() < 0.01 && (map.max - 20.0).abs() < 0.01);
        assert!((map.median - 11.0).abs() < 0.01 || (map.median - 10.0).abs() < 0.01);
        assert!((map.p95 - 19.0).abs() < 1.01);
        assert!(report.sections.iter().any(|(n, s)| *n == "rare" && s.samples == 1));
        assert!(report.frame.is_some_and(|f| (f.mean - 10.0).abs() < 0.01));
        let table = report.table();
        assert!(table.contains("map") && table.contains("frame interval"));
        assert_eq!(SectionStats::of(&[]), None);
    }

    #[test]
    fn fps_from_history() {
        let mut m = FrameMetrics::default();
        for _ in 0..10 {
            m.record_frame(Duration::from_millis(20));
        }
        assert!((m.fps() - 50.0).abs() < 0.5);
        m.record_section("sim", Duration::from_millis(3));
        assert!(m.sections().any(|(n, t)| n == "sim" && (t.last_ms - 3.0).abs() < 0.01));
    }
}
