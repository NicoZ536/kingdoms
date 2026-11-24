//! Themed charts for the management windows: line and (stacked) bar series over a shared x
//! axis, German axis labels, a legend and a hover readout of all values of one x slot.
//!
//! The axis scaling ([`Scale`]), the value formatting and the hit testing are pure functions
//! and unit tested; [`Chart::show`] only paints them with the theme colours.

use kingdoms_engine::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::ui::hud::format::{decimal, int};
use crate::ui::theme::Palette;

/// How a series is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeriesKind {
    /// A polyline through the slot centres.
    Line,
    /// A bar per slot; several bar series stack (positive values upwards, negative downwards).
    Bar,
}

/// How values (axis ticks, hover readout) are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueFormat {
    /// Whole numbers with thousands separators ("1.250").
    Integer,
    /// Gold amounts ("1.250 Gold" in the readout, plain numbers on the axis).
    Gold,
    /// Numbers with up to one decimal ("12,5").
    Decimal,
}

impl ValueFormat {
    /// Readout text of a value.
    pub fn value(self, v: f64) -> String {
        match self {
            ValueFormat::Integer => int(v.round() as i64),
            ValueFormat::Gold => format!("{} Gold", if v.abs() >= 100.0 { int(v.round() as i64) } else { decimal(v, 1) }),
            ValueFormat::Decimal => decimal(v, 1),
        }
    }

    /// Axis label of a tick `v` on an axis with tick distance `step`.
    pub fn tick(self, v: f64, step: f64) -> String {
        if v.abs() >= 1_000_000.0 {
            format!("{} Mio.", decimal(v / 1_000_000.0, 1))
        } else if v.abs() >= 10_000.0 {
            format!("{} Tsd.", decimal(v / 1000.0, 1))
        } else if step < 1.0 {
            decimal(v, decimals_for_step(step))
        } else {
            int(v.round() as i64)
        }
    }
}

/// Decimal places needed to tell ticks `step` apart (0.5 → 1, 0.25 → 2).
fn decimals_for_step(step: f64) -> usize {
    let mut decimals = 0;
    let mut s = step.abs();
    while decimals < 3 && (s - s.round()).abs() > 1e-6 {
        s *= 10.0;
        decimals += 1;
    }
    decimals
}

/// One data series; `values` has one entry per x slot (non-finite values are gaps).
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub label: String,
    pub color: Color32,
    pub kind: SeriesKind,
    pub values: Vec<f64>,
}

impl Series {
    pub fn line(label: impl Into<String>, color: Color32, values: Vec<f64>) -> Self {
        Self { label: label.into(), color, kind: SeriesKind::Line, values }
    }

    pub fn bar(label: impl Into<String>, color: Color32, values: Vec<f64>) -> Self {
        Self { label: label.into(), color, kind: SeriesKind::Bar, values }
    }
}

/// A linear axis from `lo` to `hi` with ticks every `step` (all "nice" numbers: 1, 2, 2.5 or
/// 5 × 10ⁿ).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub lo: f64,
    pub hi: f64,
    pub step: f64,
}

impl Scale {
    /// Like [`Scale::nice`] with whole-number ticks (counts, gold): steps of at least 1 and
    /// never 2.5.
    pub fn nice_whole(min: f64, max: f64, target_ticks: usize) -> Self {
        let s = Self::nice(min, max, target_ticks);
        if s.step >= 1.0 && s.step.fract() == 0.0 {
            return s;
        }
        let step = if s.step < 1.0 { 1.0 } else { s.step.floor() };
        Self { lo: (s.lo / step).floor() * step, hi: (s.hi / step).ceil() * step, step }
    }

    /// The nice axis covering `min..=max` with about `target_ticks` intervals. An empty range
    /// is widened around its value (or to 0..1 at zero).
    pub fn nice(min: f64, max: f64, target_ticks: usize) -> Self {
        let (mut min, mut max) = if min.is_finite() && max.is_finite() { (min.min(max), min.max(max)) } else { (0.0, 1.0) };
        if (max - min).abs() < f64::EPSILON {
            if min == 0.0 {
                max = 1.0;
            } else if min > 0.0 {
                min = 0.0;
            } else {
                max = 0.0;
            }
        }
        let step = nice_step((max - min) / target_ticks.max(1) as f64);
        let lo = (min / step).floor() * step;
        let hi = (max / step).ceil() * step;
        Self { lo, hi, step }
    }

    /// Tick values from `lo` to `hi`.
    pub fn ticks(&self) -> Vec<f64> {
        let n = ((self.hi - self.lo) / self.step).round().max(0.0) as usize;
        (0..=n).map(|i| self.lo + self.step * i as f64).map(|v| if v.abs() < self.step * 1e-9 { 0.0 } else { v }).collect()
    }

    /// Position of `v` on the axis: 0.0 at `lo`, 1.0 at `hi`.
    pub fn fraction(&self, v: f64) -> f32 {
        let span = self.hi - self.lo;
        if span <= 0.0 { 0.0 } else { ((v - self.lo) / span) as f32 }
    }
}

/// The nice number (1, 2, 2.5 or 5 × 10ⁿ) closest above `raw`.
fn nice_step(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let magnitude = 10f64.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let nice = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 2.5 {
        2.5
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * magnitude
}

/// Smallest and largest value the chart has to show: lines by value, bars as stacks
/// (positives and negatives summed separately per slot) starting at zero.
pub fn value_range(series: &[Series]) -> Option<(f64, f64)> {
    let slots = series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    let mut range: Option<(f64, f64)> = None;
    let mut include = |v: f64| {
        if v.is_finite() {
            range = Some(range.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))));
        }
    };
    for s in series.iter().filter(|s| s.kind == SeriesKind::Line) {
        s.values.iter().for_each(|v| include(*v));
    }
    let bars: Vec<&Series> = series.iter().filter(|s| s.kind == SeriesKind::Bar).collect();
    if !bars.is_empty() {
        for i in 0..slots {
            let (pos, neg) = stack_totals(&bars, i);
            include(pos);
            include(neg);
        }
        include(0.0);
    }
    range
}

/// Sums of the positive and of the negative bar values in slot `i`.
fn stack_totals(bars: &[&Series], i: usize) -> (f64, f64) {
    bars.iter().filter_map(|s| s.values.get(i).copied().filter(|v| v.is_finite())).fold((0.0, 0.0), |(pos, neg), v| {
        if v >= 0.0 { (pos + v, neg) } else { (pos, neg + v) }
    })
}

/// Every how many slots an x label fits (labels of `label_width` points in `plot_width`).
pub fn label_stride(slots: usize, plot_width: f32, label_width: f32) -> usize {
    if slots == 0 || plot_width <= 0.0 {
        return 1;
    }
    let per_slot = plot_width / slots as f32;
    let needed = (label_width + 6.0) / per_slot.max(0.01);
    (needed.ceil() as usize).max(1)
}

/// The slot under horizontal position `x` of a plot starting at `left` with `width` points.
pub fn slot_at(x: f32, left: f32, width: f32, slots: usize) -> Option<usize> {
    if slots == 0 || width <= 0.0 || x < left || x > left + width {
        return None;
    }
    Some((((x - left) / width * slots as f32) as usize).min(slots - 1))
}

/// A chart over `x_labels.len()` slots.
pub struct Chart<'a> {
    id: egui::Id,
    x_labels: &'a [String],
    /// Long slot names for the hover readout (default: the axis labels).
    titles: Option<&'a [String]>,
    series: &'a [Series],
    height: f32,
    format: ValueFormat,
    /// Always show 0 on the value axis (counts, stocks).
    include_zero: bool,
    empty_text: &'a str,
}

impl<'a> Chart<'a> {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug, x_labels: &'a [String], series: &'a [Series]) -> Self {
        Self {
            id: egui::Id::unique(id),
            x_labels,
            titles: None,
            series,
            height: 160.0,
            format: ValueFormat::Integer,
            include_zero: true,
            empty_text: "Noch keine Daten.",
        }
    }

    /// Long names of the slots for the hover readout (same length as the labels).
    pub fn titles(mut self, titles: &'a [String]) -> Self {
        self.titles = Some(titles);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn format(mut self, format: ValueFormat) -> Self {
        self.format = format;
        self
    }

    pub fn include_zero(mut self, include_zero: bool) -> Self {
        self.include_zero = include_zero;
        self
    }

    pub fn empty_text(mut self, text: &'a str) -> Self {
        self.empty_text = text;
        self
    }

    /// Paints the chart over the available width.
    pub fn show(self, ui: &mut Ui) -> Response {
        let width = ui.available_width().max(120.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, self.height), Sense::hover());
        let painter = ui.painter_at(rect.expand(1.0));
        painter.rect_filled(rect, CornerRadius::same(4), Palette::PARCHMENT_LIGHT);
        painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, Palette::PARCHMENT_SHADE), egui::StrokeKind::Inside);
        let slots = self.x_labels.len();
        let range = value_range(self.series);
        let Some((mut min, mut max)) = range.filter(|_| slots > 0) else {
            painter.text(rect.center(), Align2::CENTER_CENTER, self.empty_text, FontId::proportional(14.0), Palette::INK_SOFT);
            self.show_legend(ui);
            return response;
        };
        if self.include_zero {
            min = min.min(0.0);
            max = max.max(0.0);
        }
        // A little headroom so that lines do not run along the frame.
        let pad = (max - min) * 0.04;
        if max > 0.0 {
            max += pad;
        }
        if min < 0.0 || !self.include_zero {
            min -= pad;
        }
        let ticks_wanted = ((self.height - 30.0) / 28.0).clamp(2.0, 8.0) as usize;
        let scale = match self.format {
            ValueFormat::Integer | ValueFormat::Gold => Scale::nice_whole(min, max, ticks_wanted),
            ValueFormat::Decimal => Scale::nice(min, max, ticks_wanted),
        };
        let small = FontId::proportional(12.0);
        let tick_labels: Vec<(f64, String)> = scale.ticks().into_iter().map(|t| (t, self.format.tick(t, scale.step))).collect();
        let label_width = tick_labels
            .iter()
            .map(|(_, l)| ui.painter().layout_no_wrap(l.clone(), small.clone(), Palette::INK_SOFT).size().x)
            .fold(0.0f32, f32::max);
        let plot = Rect::from_min_max(
            Pos2::new(rect.left() + label_width + 12.0, rect.top() + 8.0),
            Pos2::new(rect.right() - 8.0, rect.bottom() - 20.0),
        );
        let y_of = |v: f64| plot.bottom() - scale.fraction(v) * plot.height();
        // Grid and value axis.
        for (t, label) in &tick_labels {
            let y = y_of(*t);
            let stroke = if *t == 0.0 {
                Stroke::new(1.2, Palette::INK_SOFT)
            } else {
                Stroke::new(1.0, Palette::PARCHMENT_SHADE.gamma_multiply(0.7))
            };
            painter.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], stroke);
            painter.text(Pos2::new(plot.left() - 6.0, y), Align2::RIGHT_CENTER, label, small.clone(), Palette::INK_SOFT);
        }
        let slot_w = plot.width() / slots as f32;
        let x_of = |i: usize| plot.left() + slot_w * (i as f32 + 0.5);
        // Hover band.
        let hovered = response.hover_pos().and_then(|p| slot_at(p.x, plot.left(), plot.width(), slots));
        if let Some(i) = hovered {
            let band = Rect::from_min_max(Pos2::new(plot.left() + slot_w * i as f32, plot.top()), Pos2::new(plot.left() + slot_w * (i + 1) as f32, plot.bottom()));
            painter.rect_filled(band, CornerRadius::ZERO, Palette::GOLD.gamma_multiply(0.18));
        }
        // Bars (stacked).
        let bars: Vec<&Series> = self.series.iter().filter(|s| s.kind == SeriesKind::Bar).collect();
        if !bars.is_empty() {
            let bar_w = (slot_w * 0.7).clamp(1.0, 26.0);
            for i in 0..slots {
                let (mut pos, mut neg) = (0.0, 0.0);
                for s in &bars {
                    let Some(v) = s.values.get(i).copied().filter(|v| v.is_finite() && *v != 0.0) else { continue };
                    let (from, to) = if v > 0.0 {
                        pos += v;
                        (pos - v, pos)
                    } else {
                        neg += v;
                        (neg - v, neg)
                    };
                    let r = Rect::from_x_y_ranges(x_of(i) - bar_w / 2.0..=x_of(i) + bar_w / 2.0, y_of(to).min(y_of(from))..=y_of(to).max(y_of(from)));
                    painter.rect_filled(r, CornerRadius::same(1), s.color);
                }
            }
        }
        // Lines.
        for s in self.series.iter().filter(|s| s.kind == SeriesKind::Line) {
            let mut run: Vec<Pos2> = Vec::new();
            let flush = |run: &mut Vec<Pos2>| {
                if run.len() > 1 {
                    painter.add(egui::Shape::line(std::mem::take(run), Stroke::new(2.0, s.color)));
                } else {
                    run.clear();
                }
            };
            for i in 0..slots {
                match s.values.get(i).copied().filter(|v| v.is_finite()) {
                    Some(v) => {
                        let p = Pos2::new(x_of(i), y_of(v));
                        if slots <= 30 || hovered == Some(i) {
                            painter.circle_filled(p, if hovered == Some(i) { 3.5 } else { 2.2 }, s.color);
                        }
                        run.push(p);
                    }
                    None => flush(&mut run),
                }
            }
            flush(&mut run);
        }
        // Slot labels.
        let widest = self
            .x_labels
            .iter()
            .map(|l| ui.painter().layout_no_wrap(l.clone(), small.clone(), Palette::INK_SOFT).size().x)
            .fold(0.0f32, f32::max);
        let stride = label_stride(slots, plot.width(), widest);
        for (i, label) in self.x_labels.iter().enumerate() {
            // Counted from the newest slot, which is always labelled.
            if (slots - 1 - i) % stride != 0 {
                continue;
            }
            painter.text(Pos2::new(x_of(i), plot.bottom() + 3.0), Align2::CENTER_TOP, label, small.clone(), Palette::INK_SOFT);
        }
        painter.line_segment([plot.left_bottom(), plot.right_bottom()], Stroke::new(1.0, Palette::INK_SOFT));
        let response = match hovered {
            Some(i) => response.on_hover_ui_at_pointer(|ui| {
                let title = self.titles.and_then(|t| t.get(i)).or_else(|| self.x_labels.get(i));
                ui.label(egui::RichText::new(title.map_or("", String::as_str)).strong());
                egui::Grid::new(self.id.with("readout")).num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
                    for s in self.series {
                        let value = s.values.get(i).copied().filter(|v| v.is_finite());
                        ui.horizontal(|ui| {
                            swatch(ui, s.color, s.kind);
                            ui.label(&s.label);
                        });
                        ui.label(value.map_or_else(|| "–".to_owned(), |v| self.format.value(v)));
                        ui.end_row();
                    }
                });
            }),
            None => response,
        };
        self.show_legend(ui);
        response
    }

    fn show_legend(&self, ui: &mut Ui) {
        if self.series.len() < 2 {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for s in self.series {
                swatch(ui, s.color, s.kind);
                ui.label(egui::RichText::new(&s.label).small().color(Palette::INK));
                ui.add_space(8.0);
            }
        });
    }
}

/// Colour sample of a series: a square for bars, a short thick line for lines.
fn swatch(ui: &mut Ui, color: Color32, kind: SeriesKind) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(14.0, 12.0), Sense::hover());
    match kind {
        SeriesKind::Bar => {
            ui.painter().rect_filled(rect.shrink(1.5), CornerRadius::same(2), color);
        }
        SeriesKind::Line => {
            let y = rect.center().y;
            ui.painter().line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(3.0, color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_scales() {
        let s = Scale::nice(0.0, 87.0, 4);
        assert_eq!((s.lo, s.hi, s.step), (0.0, 100.0, 25.0));
        assert_eq!(s.ticks(), vec![0.0, 25.0, 50.0, 75.0, 100.0]);
        let s = Scale::nice(-130.0, 420.0, 6);
        assert_eq!((s.lo, s.hi, s.step), (-200.0, 500.0, 100.0));
        assert!(s.ticks().contains(&0.0));
        // Small ranges get fractional steps.
        let s = Scale::nice(0.0, 1.3, 4);
        assert_eq!((s.lo, s.hi, s.step), (0.0, 1.5, 0.5));
        // An empty range is widened.
        assert_eq!(Scale::nice(0.0, 0.0, 4).hi, 1.0);
        let s = Scale::nice(12.0, 12.0, 4);
        assert!(s.lo <= 0.0 && s.hi >= 12.0);
        let s = Scale::nice(-5.0, -5.0, 4);
        assert!(s.lo <= -5.0 && s.hi >= 0.0);
        // Non-finite input falls back to 0..1.
        assert_eq!(Scale::nice(f64::NAN, 3.0, 4).lo, 0.0);
    }

    #[test]
    fn whole_number_scales() {
        // 0..12 in five steps would be 2.5 apart; counts use 2.
        assert_eq!(Scale::nice(0.0, 12.0, 5).step, 2.5);
        let s = Scale::nice_whole(0.0, 12.0, 5);
        assert_eq!((s.lo, s.hi, s.step), (0.0, 14.0, 2.0));
        assert!(s.ticks().iter().all(|t| t.fract() == 0.0));
        // Tiny ranges keep steps of 1.
        let s = Scale::nice_whole(-2.0, 1.0, 6);
        assert_eq!((s.lo, s.hi, s.step), (-2.0, 1.0, 1.0));
        // Whole steps stay as they are.
        assert_eq!(Scale::nice_whole(0.0, 87.0, 4), Scale::nice(0.0, 87.0, 4));
    }

    #[test]
    fn fractions_and_steps() {
        let s = Scale { lo: -100.0, hi: 300.0, step: 100.0 };
        assert_eq!(s.fraction(-100.0), 0.0);
        assert_eq!(s.fraction(0.0), 0.25);
        assert_eq!(s.fraction(300.0), 1.0);
        assert_eq!(nice_step(0.0), 1.0);
        assert_eq!(nice_step(3.0), 5.0);
        assert_eq!(nice_step(0.18), 0.2);
        assert_eq!(nice_step(2400.0), 2500.0);
        assert_eq!(nice_step(7.0), 10.0);
    }

    #[test]
    fn stacked_bars_and_lines_define_the_range() {
        let series = vec![
            Series::bar("Steuern", Palette::GOOD, vec![10.0, 20.0, 5.0]),
            Series::bar("Märkte", Palette::GOOD, vec![5.0, 5.0, 5.0]),
            Series::bar("Löhne", Palette::BAD, vec![-8.0, -30.0, -2.0]),
            Series::line("Bilanz", Palette::INK, vec![7.0, -5.0, f64::NAN]),
        ];
        assert_eq!(value_range(&series), Some((-30.0, 25.0)));
        // Bars always include zero; lines alone do not.
        assert_eq!(value_range(&[Series::bar("x", Palette::GOOD, vec![4.0, 6.0])]), Some((0.0, 6.0)));
        assert_eq!(value_range(&[Series::line("x", Palette::GOOD, vec![4.0, 6.0])]), Some((4.0, 6.0)));
        assert_eq!(value_range(&[Series::line("x", Palette::GOOD, vec![f64::NAN])]), None);
        assert_eq!(value_range(&[]), None);
    }

    #[test]
    fn labels_and_hit_testing() {
        // 48 slots on 480 points with 30-point labels: every 4th slot.
        assert_eq!(label_stride(48, 480.0, 30.0), 4);
        assert_eq!(label_stride(5, 500.0, 30.0), 1);
        assert_eq!(label_stride(0, 500.0, 30.0), 1);
        assert_eq!(slot_at(100.0, 100.0, 200.0, 4), Some(0));
        assert_eq!(slot_at(249.0, 100.0, 200.0, 4), Some(2));
        assert_eq!(slot_at(300.0, 100.0, 200.0, 4), Some(3));
        assert_eq!(slot_at(99.0, 100.0, 200.0, 4), None);
        assert_eq!(slot_at(150.0, 100.0, 200.0, 0), None);
    }

    #[test]
    fn german_value_texts() {
        assert_eq!(ValueFormat::Integer.tick(1250.0, 250.0), "1.250");
        assert_eq!(ValueFormat::Integer.tick(0.5, 0.5), "0,5");
        assert_eq!(ValueFormat::Integer.tick(0.25, 0.25), "0,25");
        assert_eq!(ValueFormat::Integer.tick(25_000.0, 5000.0), "25 Tsd.");
        assert_eq!(ValueFormat::Gold.value(-1234.4), "−1.234 Gold");
        assert_eq!(ValueFormat::Gold.value(12.46), "12,5 Gold");
        assert_eq!(ValueFormat::Integer.tick(2_500_000.0, 500_000.0), "2,5 Mio.");
        assert_eq!(ValueFormat::Decimal.value(3.26), "3,3");
        assert_eq!(ValueFormat::Integer.value(1999.6), "2.000");
        assert_eq!(decimals_for_step(0.5), 1);
        assert_eq!(decimals_for_step(2.5), 1);
        assert_eq!(decimals_for_step(100.0), 0);
    }
}
