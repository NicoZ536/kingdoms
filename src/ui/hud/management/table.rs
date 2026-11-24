//! Table rows for the management windows: column widths (fixed or fitted to their content with
//! [`fit_widths`]), zebra stripes, a hover highlight and whole-row clicks (buttons inside a row
//! keep their own clicks). Texts that still do not fit end in „…“ and show the full text as a
//! tooltip; [`wrapped`] cells break long texts onto a second line instead.

use kingdoms_engine::egui::{
    self, Align, Color32, CornerRadius, FontSelection, Layout, Response, RichText, Sense, TextWrapMode, Ui, UiBuilder, Vec2, WidgetText,
};

use crate::ui::theme::Palette;

/// Height of a table row in points.
pub const ROW_HEIGHT: f32 = 24.0;

/// Horizontal alignment of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellAlign {
    Left,
    Right,
}

/// A full-width row of `height` points. `clickable` rows highlight on hover and report clicks;
/// `stripe` paints the zebra background. Cells are added left to right by `add_cells`.
pub fn row(ui: &mut Ui, id: impl egui::AsIdSalt, height: f32, stripe: bool, clickable: bool, add_cells: impl FnOnce(&mut Ui)) -> Response {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    // Registered before the cells so that buttons inside the row stay on top of it.
    let sense = if clickable { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, ui.make_persistent_id(id), sense);
    let fill = if clickable && response.hovered() {
        Some(Palette::GOLD_BRIGHT.gamma_multiply(0.35))
    } else if stripe {
        Some(Palette::PARCHMENT_DARK.gamma_multiply(0.45))
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
    }
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(Vec2::new(6.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    child.spacing_mut().item_spacing.x = 0.0;
    // Selectable texts would take the row's clicks.
    child.style_mut().interaction.selectable_labels = false;
    add_cells(&mut child);
    if clickable {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A cell of `width` points (the last cell may use the rest with `f32::INFINITY`).
pub fn cell<R>(ui: &mut Ui, width: f32, align: CellAlign, add: impl FnOnce(&mut Ui) -> R) -> R {
    let width = width.min(ui.available_width()).max(0.0);
    let layout = match align {
        CellAlign::Left => Layout::left_to_right(Align::Center),
        CellAlign::Right => Layout::right_to_left(Align::Center),
    };
    let height = ui.available_height();
    ui.allocate_ui_with_layout(Vec2::new(width, height), layout, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.x = 4.0;
        // Keep a small gap to the next column.
        if align == CellAlign::Right {
            ui.add_space(8.0);
        }
        add(ui)
    })
    .inner
}

/// A text cell (truncated to the width; the full text then shows as a tooltip).
pub fn text(ui: &mut Ui, width: f32, align: CellAlign, text: impl Into<egui::WidgetText>) {
    cell(ui, width, align, |ui| {
        ui.add(egui::Label::new(text).truncate().show_tooltip_when_elided(true));
    });
}

/// A text cell of a column `width` points wide that breaks a long text onto further lines
/// (left aligned, [`CELL_PAD`] kept free on the right); pair it with a row height from
/// [`wrapped_height`].
pub fn wrapped(ui: &mut Ui, width: f32, text: impl Into<egui::WidgetText>) {
    let inner = (width - CELL_PAD).max(1.0);
    cell(ui, width, CellAlign::Left, |ui| {
        let height = ui.available_height();
        ui.allocate_ui_with_layout(Vec2::new(inner, height), Layout::left_to_right(Align::Center), |ui| {
            ui.set_width(inner);
            ui.add(egui::Label::new(text).wrap());
        });
    });
}

/// Room a text needs on one line in `ui` (its own font or the body font), in points.
pub fn text_width(ui: &Ui, text: impl Into<WidgetText>) -> f32 {
    text.into().into_galley(ui, Some(TextWrapMode::Extend), f32::INFINITY, FontSelection::Default).size().x
}

/// Height of a row whose [`wrapped`] cell in a column of `width` points shows `text`: at least
/// `min`.
pub fn wrapped_height(ui: &Ui, text: impl Into<WidgetText>, width: f32, min: f32) -> f32 {
    let galley = text.into().into_galley(ui, Some(TextWrapMode::Wrap), (width - CELL_PAD).max(1.0), FontSelection::Default);
    min.max(galley.size().y + 6.0)
}

/// What a column of a fitted table needs: `want` (its widest content plus padding) and the
/// `min` it may shrink to when space is short (only `flex` columns shrink).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub min: f32,
    pub want: f32,
    pub flex: bool,
}

impl Fit {
    /// A column that keeps the width of its content.
    pub fn fixed(want: f32) -> Self {
        Self { min: want, want, flex: false }
    }

    /// A column of texts that may shrink down to `min` (they then wrap or end in „…“).
    pub fn flex(min: f32, want: f32) -> Self {
        Self { min: min.min(want), want, flex: true }
    }
}

/// Widths of `columns` in `available` points: every column gets the width its content wants;
/// if that is wider than the room, the flexible columns give up the excess in proportion to
/// their slack (never below their minimum); room to spare is shared in proportion to the
/// widths, so the table fills the row and its columns keep their proportions.
pub fn fit_widths(columns: &[Fit], available: f32) -> Vec<f32> {
    let wanted: f32 = columns.iter().map(|c| c.want).sum();
    let mut widths: Vec<f32> = columns.iter().map(|c| c.want).collect();
    if wanted > available {
        let slack: f32 = columns.iter().filter(|c| c.flex).map(|c| c.want - c.min).sum();
        let excess = (wanted - available).min(slack);
        if slack > 0.0 {
            for (w, c) in widths.iter_mut().zip(columns) {
                if c.flex {
                    *w -= (c.want - c.min) * excess / slack;
                }
            }
        }
    } else if wanted > 0.0 {
        let scale = available / wanted;
        for w in &mut widths {
            *w *= scale;
        }
    }
    widths
}

/// Padding added to measured contents: the gap to the next column and the row's margin.
pub const CELL_PAD: f32 = 14.0;

/// Width a column needs for its header and cell texts (measured in `ui`) plus [`CELL_PAD`].
pub fn content_width(ui: &Ui, texts: impl IntoIterator<Item = WidgetText>) -> f32 {
    texts.into_iter().map(|t| text_width(ui, t)).fold(0.0, f32::max) + CELL_PAD
}

/// A header row of small caption texts with a rule below.
pub fn header(ui: &mut Ui, columns: &[(&str, f32, CellAlign)]) {
    row(ui, "table_header", 20.0, false, false, |ui| {
        for (title, width, align) in columns {
            text(ui, *width, *align, caption(title));
        }
    });
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, Palette::GOLD_DARK.gamma_multiply(0.6));
}

/// A header cell that sorts the table when clicked; returns true on a click. The active
/// column shows a painted arrow (down: descending).
pub fn sort_header(ui: &mut Ui, title: &str, width: f32, align: CellAlign, active: bool, descending: bool) -> bool {
    cell(ui, width, align, |ui| {
        let color = if active { Palette::INK } else { Palette::INK_SOFT };
        let response = ui.add(egui::Label::new(RichText::new(title).small().strong().color(color)).sense(Sense::click()));
        if active {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
            let c = rect.center();
            let points = if descending {
                vec![c + egui::vec2(-4.0, -2.5), c + egui::vec2(4.0, -2.5), c + egui::vec2(0.0, 3.0)]
            } else {
                vec![c + egui::vec2(-4.0, 2.5), c + egui::vec2(4.0, 2.5), c + egui::vec2(0.0, -3.0)]
            };
            ui.painter().add(egui::Shape::convex_polygon(points, Palette::GOLD_DARK, egui::Stroke::NONE));
        }
        response.on_hover_text("Sortieren").on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    })
}

/// Thin separator line under a header built with [`sort_header`].
pub fn rule(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, Palette::GOLD_DARK.gamma_multiply(0.6));
}

/// Width of a column with the header `title` over the cell texts `cells` (see
/// [`content_width`]).
pub fn column_width(ui: &Ui, title: &str, cells: impl IntoIterator<Item = WidgetText>) -> f32 {
    content_width(ui, std::iter::once(caption(title).into()).chain(cells))
}

/// Caption text of a header cell (also for measuring a column).
pub fn caption(title: &str) -> RichText {
    RichText::new(title).small().strong().color(Palette::INK_SOFT)
}

/// Coloured text for a signed change: green above zero, red below, soft at zero.
pub fn signed_color(v: f64) -> Color32 {
    if v > 0.0 {
        Palette::GOOD
    } else if v < 0.0 {
        Palette::BAD
    } else {
        Palette::INK_SOFT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_fit_their_content_and_share_the_shortage() {
        let cols = [Fit::fixed(100.0), Fit::flex(60.0, 200.0), Fit::flex(40.0, 100.0)];
        // Room to spare is shared in proportion.
        assert_eq!(fit_widths(&cols, 800.0), vec![200.0, 400.0, 200.0]);
        // Too narrow: the flexible columns shrink by their slack (140 and 60 of 200).
        let w = fit_widths(&cols, 300.0);
        assert_eq!(w[0], 100.0, "fixed columns keep their width");
        assert!((w[1] - 130.0).abs() < 0.01 && (w[2] - 70.0).abs() < 0.01, "{w:?}");
        assert!((w.iter().sum::<f32>() - 300.0).abs() < 0.01);
        // Never below the minimum: the table is then wider than the room.
        assert_eq!(fit_widths(&cols, 150.0), vec![100.0, 60.0, 40.0]);
        // Only fixed columns: nothing to shrink.
        assert_eq!(fit_widths(&[Fit::fixed(50.0)], 10.0), vec![50.0]);
        assert_eq!(fit_widths(&[Fit::fixed(50.0)], 100.0), vec![100.0]);
        assert!(fit_widths(&[], 100.0).is_empty());
        assert_eq!(Fit::flex(80.0, 30.0).min, 30.0, "the minimum never exceeds the content");
    }
}
