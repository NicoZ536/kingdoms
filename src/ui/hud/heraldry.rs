//! Heraldry and diplomatic states for the HUD: the crest of a kingdom (`icons/crest_<colour>`,
//! painted as a plain shield in the kingdom's colour while the atlas lacks it), colours of the
//! diplomatic states and the relation bar (−100 … +100) used by the diplomacy tab, the proposal
//! dialog and the military tab.

use kingdoms_engine::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};
use kingdoms_sim::diplomacy::RelationState;

use super::Icons;
use crate::ui::theme::{self, Palette};

/// Number of crests in the `icons` atlas (one per kingdom colour index).
pub const CRESTS: u8 = 7;

/// Icon name of the crest of colour index `color`.
pub fn crest_icon(color: u8) -> String {
    format!("crest_{}", color % CRESTS)
}

/// Paints the crest of colour index `color` into `rect` (keeping the shield's aspect).
pub fn paint_crest(painter: &egui::Painter, icons: &Icons, rect: Rect, color: u8) {
    if let Some(icon) = icons.get(&crest_icon(color)) {
        icon.paint_fit(painter, rect, Color32::WHITE);
        return;
    }
    // Fallback: a heater shield in the kingdom colour with a gold rim.
    let w = rect.width().min(rect.height() * 0.83);
    let h = w / 0.83;
    let r = Rect::from_center_size(rect.center(), Vec2::new(w, h));
    let points = vec![
        r.left_top(),
        r.right_top(),
        Pos2::new(r.right(), r.top() + h * 0.55),
        Pos2::new(r.center().x, r.bottom()),
        Pos2::new(r.left(), r.top() + h * 0.55),
    ];
    painter.add(egui::Shape::convex_polygon(points, theme::kingdom_color(color), Stroke::new(1.5, Palette::GOLD_DARK)));
}

/// Allocates a `size`-point square in `ui` and paints the crest there.
pub fn crest(ui: &mut Ui, icons: &Icons, size: f32, color: u8) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_crest(ui.painter(), icons, rect, color);
    response
}

/// Text colour of a diplomatic state (`war`: at war with the player).
pub fn state_color(state: RelationState, war: bool) -> Color32 {
    if war {
        return Palette::BAD;
    }
    match state {
        RelationState::Hostile => Palette::BAD,
        RelationState::Rival => Palette::WARN,
        RelationState::Neutral => Palette::INK_SOFT,
        RelationState::Friendly => Palette::GOOD,
        RelationState::Allied => Palette::INFO,
    }
}

/// Fraction (0–1) of the relation bar's width at which `relation` (−100 … +100) lies.
pub fn relation_fraction(relation: i32) -> f32 {
    (relation.clamp(-100, 100) + 100) as f32 / 200.0
}

/// The relation as a bar from −100 (left) to +100 (right) filled from the middle, with the
/// value and `label` written on it (`label` `None`: a slim bar without text).
pub fn relation_bar(ui: &mut Ui, relation: i32, state: RelationState, war: bool, label: Option<&str>, width: f32) -> Response {
    // With a label, the text stands above the bar, clear of the middle line and the marker.
    let (label_height, bar_height) = if label.is_some() { (17.0, 12.0) } else { (0.0, 8.0) };
    let (outer, response) = ui.allocate_exact_size(Vec2::new(width, label_height + bar_height), Sense::hover());
    let rect = Rect::from_min_max(Pos2::new(outer.left(), outer.bottom() - bar_height), outer.max);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), Palette::PARCHMENT_SHADE.gamma_multiply(0.6));
    let middle = rect.left() + rect.width() * 0.5;
    let at = rect.left() + rect.width() * relation_fraction(relation);
    let fill = Rect::from_x_y_ranges(middle.min(at)..=middle.max(at), rect.y_range());
    let color = state_color(state, war);
    if fill.width() > 0.5 {
        painter.rect_filled(fill, CornerRadius::same(3), color.gamma_multiply(0.55));
    }
    painter.line_segment([Pos2::new(middle, rect.top() + 1.0), Pos2::new(middle, rect.bottom() - 1.0)], Stroke::new(1.0, Palette::WOOD.gamma_multiply(0.6)));
    painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, Palette::WOOD), egui::StrokeKind::Inside);
    // Marker of the current value (also readable when it sits next to the middle).
    let x = at.clamp(rect.left() + 2.0, rect.right() - 2.0);
    painter.line_segment([Pos2::new(x, rect.top() - 1.0), Pos2::new(x, rect.bottom() + 1.0)], Stroke::new(2.5, Palette::INK));
    painter.line_segment([Pos2::new(x, rect.top() + 1.0), Pos2::new(x, rect.bottom() - 1.0)], Stroke::new(1.0, color));
    if let Some(label) = label {
        let relation = super::format::signed(i64::from(relation));
        let text = if label.is_empty() { relation } else { format!("Beziehung {relation} · {label}") };
        painter.text(outer.left_top(), Align2::LEFT_TOP, text, FontId::proportional(13.0), Palette::INK);
        let scale = FontId::proportional(11.0);
        painter.text(outer.right_top() + Vec2::new(0.0, 2.0), Align2::RIGHT_TOP, "−100 … +100", scale, Palette::INK_SOFT);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relation_bar_spans_minus_to_plus_hundred() {
        assert_eq!(relation_fraction(-100), 0.0);
        assert_eq!(relation_fraction(0), 0.5);
        assert_eq!(relation_fraction(100), 1.0);
        assert_eq!(relation_fraction(250), 1.0);
        assert_eq!(crest_icon(3), "crest_3");
        assert_eq!(crest_icon(9), "crest_2");
        // War overrides the state colour.
        assert_eq!(state_color(RelationState::Allied, true), Palette::BAD);
        assert_ne!(state_color(RelationState::Friendly, false), state_color(RelationState::Hostile, false));
    }
}
