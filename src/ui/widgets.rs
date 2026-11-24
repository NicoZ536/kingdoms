//! Custom game UI widgets drawn on top of egui (resource chips, icon buttons, ornamental headers,
//! progress bars, stat rows, themed windows).

use kingdoms_engine::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::ui::theme::{Palette, heading_font};

/// A sub-image of a registered texture (e.g. an icon inside the icon atlas).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconRef {
    pub texture: egui::TextureId,
    pub uv: Rect,
    /// Width / height of the sprite in pixels.
    pub aspect: f32,
}

impl IconRef {
    /// Paints the icon into `rect`, stretched.
    pub fn paint(self, painter: &egui::Painter, rect: Rect, tint: Color32) {
        painter.image(self.texture, rect, self.uv, tint);
    }

    /// Paints the icon centred in `rect` keeping its aspect ratio.
    pub fn paint_fit(self, painter: &egui::Painter, rect: Rect, tint: Color32) {
        let aspect = if self.aspect.is_finite() && self.aspect > 0.0 { self.aspect } else { 1.0 };
        let size = if rect.width() / rect.height().max(1.0) > aspect {
            Vec2::new(rect.height() * aspect, rect.height())
        } else {
            Vec2::new(rect.width(), rect.width() / aspect)
        };
        self.paint(painter, Rect::from_center_size(rect.center(), size), tint);
    }

    /// Allocates `size` × `size` points in `ui` and paints the icon there.
    pub fn show(self, ui: &mut Ui, size: f32) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        let tint = if ui.is_enabled() { Color32::WHITE } else { Color32::from_white_alpha(110) };
        self.paint_fit(ui.painter(), rect, tint);
        response
    }
}

/// Heading with gold rules on both sides.
pub fn section_header(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    let font = heading_font(16.0);
    let color = ui.visuals().override_text_color.unwrap_or(Palette::INK);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let width = ui.available_width().max(galley.size().x + 40.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, galley.size().y + 4.0), Sense::hover());
    let painter = ui.painter();
    let text_pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 2.0);
    let y = rect.center().y;
    let gap = 8.0;
    let left_end = text_pos.x - gap;
    let right_start = text_pos.x + galley.size().x + gap;
    let stroke = Stroke::new(1.2, Palette::GOLD_DARK);
    if left_end > rect.left() {
        painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(left_end, y)], stroke);
        painter.circle_filled(egui::pos2(left_end, y), 2.2, Palette::GOLD);
    }
    if right_start < rect.right() {
        painter.line_segment([egui::pos2(right_start, y), egui::pos2(rect.right(), y)], stroke);
        painter.circle_filled(egui::pos2(right_start, y), 2.2, Palette::GOLD);
    }
    painter.galley(text_pos, galley, color);
    ui.add_space(2.0);
}

/// Compact "icon + value" display for the top resource bar. Returns the hover response (attach tooltips to it).
pub fn resource_chip(ui: &mut Ui, icon: Option<IconRef>, value: &str, trend: Option<f32>) -> Response {
    let font = FontId::proportional(16.0);
    let galley = ui.painter().layout_no_wrap(value.to_owned(), font, Palette::ON_WOOD);
    let icon_size = 22.0;
    let trend_w = if trend.is_some() { 10.0 } else { 0.0 };
    let size = Vec2::new(icon_size + 6.0 + galley.size().x + trend_w + 6.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(4), Palette::WOOD_LIGHT.gamma_multiply(0.6));
    }
    let icon_rect = Rect::from_min_size(egui::pos2(rect.left() + 2.0, rect.center().y - icon_size / 2.0), Vec2::splat(icon_size));
    match icon {
        Some(icon) => icon.paint_fit(painter, icon_rect, Color32::WHITE),
        None => {
            painter.circle_filled(icon_rect.center(), icon_size * 0.35, Palette::GOLD);
        }
    }
    let text_pos = egui::pos2(icon_rect.right() + 6.0, rect.center().y - galley.size().y / 2.0);
    let text_right = text_pos.x + galley.size().x;
    painter.galley(text_pos, galley, Palette::ON_WOOD);
    if let Some(t) = trend {
        let c = egui::pos2(text_right + 6.0, rect.center().y);
        let (color, up) = if t > 0.0 { (Palette::GOOD_LIGHT, true) } else { (Palette::BAD_LIGHT, false) };
        if t.abs() > f32::EPSILON {
            let pts = if up {
                vec![c + egui::vec2(-4.0, 3.0), c + egui::vec2(4.0, 3.0), c + egui::vec2(0.0, -4.0)]
            } else {
                vec![c + egui::vec2(-4.0, -3.0), c + egui::vec2(4.0, -3.0), c + egui::vec2(0.0, 4.0)]
            };
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
    }
    response
}

/// egui memory keys of the hover sound: the button hovered last and the pending sound.
const HOVERED_BUTTON: &str = "kingdoms_hovered_button";
const HOVER_SOUND: &str = "kingdoms_hover_sound";

/// Notes that the pointer moved onto button `response` (once per entry; leaving it re-arms),
/// so the scene plays `ui_hover` ([`take_hover_sound`]).
fn note_hover(ui: &Ui, response: &Response) {
    let key = egui::Id::unique(HOVERED_BUTTON);
    let last: Option<egui::Id> = ui.ctx().data(|d| d.get_temp(key));
    if response.hovered() && ui.is_enabled() {
        if last != Some(response.id) {
            ui.ctx().data_mut(|d| {
                d.insert_temp(key, response.id);
                d.insert_temp(egui::Id::unique(HOVER_SOUND), true);
            });
        }
    } else if last == Some(response.id) {
        ui.ctx().data_mut(|d| d.remove::<egui::Id>(key));
    }
}

/// True (once) if the pointer moved onto a HUD button since the last call.
pub fn take_hover_sound(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::unique(HOVER_SOUND))).unwrap_or(false)
}

/// Square icon button for tool bars (gold frame when selected).
pub fn icon_button(ui: &mut Ui, icon: Option<IconRef>, fallback_label: &str, selected: bool, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    note_hover(ui, &response);
    let enabled = ui.is_enabled();
    let painter = ui.painter();
    let hovered = response.hovered() && enabled;
    let fill = if selected {
        Palette::GOLD_DARK
    } else if hovered {
        Palette::WOOD_LIGHT
    } else {
        Palette::WOOD
    };
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    let stroke = if selected || hovered { Stroke::new(1.5, Palette::GOLD_BRIGHT) } else { Stroke::new(1.0, Palette::GOLD_DARK) };
    painter.rect_stroke(rect, CornerRadius::same(5), stroke, egui::StrokeKind::Inside);
    let inner = rect.shrink(size * 0.12);
    // Unavailable entries stay recognisable, only greyed.
    let tint = if enabled { Color32::WHITE } else { Color32::from_rgba_unmultiplied(150, 145, 140, 215) };
    match icon {
        Some(icon) => icon.paint_fit(painter, inner, tint),
        None => {
            painter.text(rect.center(), Align2::CENTER_CENTER, fallback_label, FontId::proportional(size * 0.42), Palette::ON_WOOD);
        }
    }
    if !enabled {
        painter.rect_filled(rect, CornerRadius::same(5), Color32::from_black_alpha(35));
    }
    response
}

/// A small painted status mark (independent of the fonts' symbol coverage).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Green circle with a tick.
    Ok,
    /// Red circle with a cross.
    Error,
    /// Amber triangle with an exclamation mark.
    Warning,
    /// Gold arrow pointing right (current item, link).
    Next,
}

/// Paints `mark` in a square of the current text height.
pub fn mark(ui: &mut Ui, mark: Mark) -> Response {
    let size = 14.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    let c = rect.center();
    let r = size * 0.5;
    let white = Stroke::new(1.8, Color32::WHITE);
    match mark {
        Mark::Ok => {
            painter.circle_filled(c, r, Palette::GOOD);
            painter.line_segment([c + egui::vec2(-3.5, 0.0), c + egui::vec2(-1.0, 3.0)], white);
            painter.line_segment([c + egui::vec2(-1.0, 3.0), c + egui::vec2(3.8, -3.2)], white);
        }
        Mark::Error => {
            painter.circle_filled(c, r, Palette::BAD);
            painter.line_segment([c + egui::vec2(-3.0, -3.0), c + egui::vec2(3.0, 3.0)], white);
            painter.line_segment([c + egui::vec2(3.0, -3.0), c + egui::vec2(-3.0, 3.0)], white);
        }
        Mark::Warning => {
            let pts = vec![c + egui::vec2(0.0, -r), c + egui::vec2(r, r * 0.8), c + egui::vec2(-r, r * 0.8)];
            painter.add(egui::Shape::convex_polygon(pts, Palette::WARN, Stroke::NONE));
            painter.line_segment([c + egui::vec2(0.0, -3.0), c + egui::vec2(0.0, 2.0)], Stroke::new(1.6, Color32::WHITE));
            painter.circle_filled(c + egui::vec2(0.0, 4.2), 0.9, Color32::WHITE);
        }
        Mark::Next => {
            let pts = vec![c + egui::vec2(-3.0, -r * 0.7), c + egui::vec2(4.0, 0.0), c + egui::vec2(-3.0, r * 0.7)];
            painter.add(egui::Shape::convex_polygon(pts, Palette::GOLD_DARK, Stroke::NONE));
        }
    }
    response
}

/// Symbol of a [`glyph_button`] (painted, independent of the fonts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Up,
    Down,
    Left,
    Right,
    Cross,
}

/// Small square parchment button with a painted symbol (list controls: move up/down, remove;
/// previous/next).
pub fn glyph_button(ui: &mut Ui, glyph: Glyph, enabled: bool) -> Response {
    let size = 20.0;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    let painter = ui.painter();
    let hovered = enabled && response.hovered();
    let fill = if hovered { Palette::GOLD_BRIGHT.gamma_multiply(0.6) } else { Palette::PARCHMENT_DARK };
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, Palette::WOOD.gamma_multiply(0.8)), egui::StrokeKind::Inside);
    let color = match (enabled, glyph) {
        (false, _) => Palette::PARCHMENT_SHADE,
        (true, Glyph::Cross) => Palette::BAD,
        (true, _) => Palette::INK,
    };
    let c = rect.center();
    match glyph {
        Glyph::Up | Glyph::Down => {
            let s = if glyph == Glyph::Up { -1.0 } else { 1.0 };
            let pts = vec![c + egui::vec2(-5.0, -3.0 * s), c + egui::vec2(5.0, -3.0 * s), c + egui::vec2(0.0, 4.0 * s)];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Glyph::Left | Glyph::Right => {
            let s = if glyph == Glyph::Left { -1.0 } else { 1.0 };
            let pts = vec![c + egui::vec2(-3.0 * s, -5.0), c + egui::vec2(4.0 * s, 0.0), c + egui::vec2(-3.0 * s, 5.0)];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Glyph::Cross => {
            let stroke = Stroke::new(2.0, color);
            painter.line_segment([c + egui::vec2(-4.0, -4.0), c + egui::vec2(4.0, 4.0)], stroke);
            painter.line_segment([c + egui::vec2(4.0, -4.0), c + egui::vec2(-4.0, 4.0)], stroke);
        }
    }
    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// A mark followed by wrapping text.
pub fn marked(ui: &mut Ui, kind: Mark, text: impl Into<egui::WidgetText>) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        mark(ui, kind);
        ui.add(egui::Label::new(text).wrap());
    });
}

/// Labelled progress bar in theme colours.
pub fn progress_bar(ui: &mut Ui, fraction: f32, color: Color32, text: &str) -> Response {
    let height = 18.0;
    let width = ui.available_width().max(60.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), Palette::PARCHMENT_SHADE.gamma_multiply(0.6));
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        let fill = Rect::from_min_size(rect.min, Vec2::new(rect.width() * f, rect.height()));
        painter.rect_filled(fill, CornerRadius::same(4), color);
    }
    painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, Palette::WOOD), egui::StrokeKind::Inside);
    if !text.is_empty() {
        painter.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(13.0), Palette::INK);
    }
    response
}

/// Small bar without text (compact lists).
pub fn mini_bar(ui: &mut Ui, fraction: f32, color: Color32, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 10.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), Palette::PARCHMENT_SHADE.gamma_multiply(0.6));
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width() * f, rect.height())), CornerRadius::same(3), color);
    }
    painter.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.0, Palette::WOOD.gamma_multiply(0.7)), egui::StrokeKind::Inside);
    response
}

/// Two-column "label: value" row (use inside an `egui::Grid`).
pub fn stat_row(ui: &mut Ui, label: &str, value: impl Into<egui::WidgetText>) {
    ui.label(egui::RichText::new(label).color(Palette::INK_SOFT));
    ui.label(value);
    ui.end_row();
}

/// Colour for a 0..100 satisfaction-like value.
pub fn rating_color(value: f32) -> Color32 {
    if value >= 66.0 {
        Palette::GOOD
    } else if value >= 33.0 {
        Palette::WARN
    } else {
        Palette::BAD
    }
}

/// Formats numbers compactly in German ("950", "9.999", "12,4 Tsd.", "2,5 Mio.", "−3").
pub fn compact_number(v: f64) -> String {
    let a = v.abs();
    if a >= 1_000_000.0 {
        format!("{} Mio.", format!("{:.1}", v / 1_000_000.0).replace('.', ",").replace('-', "−"))
    } else if a >= 10_000.0 {
        format!("{} Tsd.", format!("{:.1}", v / 1000.0).replace('.', ",").replace('-', "−"))
    } else {
        let n = v.round() as i64;
        let digits = n.unsigned_abs().to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push('.');
            }
            out.push(c);
        }
        if n < 0 { format!("−{out}") } else { out }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_numbers() {
        assert_eq!(compact_number(950.0), "950");
        assert_eq!(compact_number(9_999.0), "9.999");
        assert_eq!(compact_number(12_400.0), "12,4 Tsd.");
        assert_eq!(compact_number(-2_500_000.0), "−2,5 Mio.");
        assert_eq!(compact_number(-1_234.0), "−1.234");
    }

    #[test]
    fn rating_colors() {
        assert_eq!(rating_color(80.0), Palette::GOOD);
        assert_eq!(rating_color(10.0), Palette::BAD);
    }
}
