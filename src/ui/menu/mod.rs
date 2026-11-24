//! Menus outside the HUD: main menu pages (new game, load, settings, credits), the save and
//! load browser shared with the pause menu, the settings panel and the loading screen.
//!
//! Shared look: dark wood buttons with gold trim on the world backdrop, parchment pages with
//! Cinzel headings. Every element shows real state and triggers a real action.

pub mod credits;
pub mod loading_screen;
pub mod new_game;
pub mod save_browser;
pub mod settings_panel;

use kingdoms_engine::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::ui::theme::{self, Palette, heading_font};

/// Page sizes (outer, points); smaller screens shrink them.
pub const PAGE_WIDE: Vec2 = Vec2::new(980.0, 640.0);
pub const PAGE_NARROW: Vec2 = Vec2::new(780.0, 640.0);
/// Margin around pages.
const PAGE_MARGIN: f32 = 24.0;

/// A tall wooden menu button (main and pause menu). `focused`: selected with the keyboard.
pub fn menu_button(ui: &mut Ui, text: &str, width: f32, enabled: bool, focused: bool) -> Response {
    let height = 46.0;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
    let hovered = enabled && response.hovered();
    let lit = enabled && (hovered || focused);
    let painter = ui.painter();
    let fill = if !enabled {
        Palette::WOOD_DARK.gamma_multiply(0.85)
    } else if response.is_pointer_button_down_on() {
        Palette::GOLD_DARK
    } else if lit {
        Palette::WOOD_LIGHT
    } else {
        Palette::WOOD
    };
    painter.rect_filled(rect.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(6), Color32::from_black_alpha(90));
    painter.rect_filled(rect, CornerRadius::same(6), fill);
    // Inner bevel line and gold trim.
    painter.rect_stroke(rect.shrink(3.0), CornerRadius::same(4), Stroke::new(1.0, Palette::GOLD_DARK.gamma_multiply(0.6)), egui::StrokeKind::Inside);
    let trim = if lit { Stroke::new(2.0, Palette::GOLD_BRIGHT) } else { Stroke::new(1.5, Palette::GOLD_DARK) };
    painter.rect_stroke(rect, CornerRadius::same(6), trim, egui::StrokeKind::Inside);
    let color = if !enabled {
        Palette::ON_WOOD_SOFT.gamma_multiply(0.6)
    } else if lit {
        Palette::GOLD_BRIGHT
    } else {
        Palette::ON_WOOD
    };
    painter.text(rect.center(), Align2::CENTER_CENTER, text, heading_font(20.0), color);
    if lit {
        // Ornament diamonds left and right of the focused entry.
        for x in [rect.left() + 16.0, rect.right() - 16.0] {
            diamond(painter, Pos2::new(x, rect.center().y), 4.5, Palette::GOLD_BRIGHT);
        }
    }
    response
}

/// A small filled diamond (ornament).
pub fn diamond(painter: &egui::Painter, center: Pos2, r: f32, color: Color32) {
    let pts = vec![center + Vec2::new(0.0, -r), center + Vec2::new(r, 0.0), center + Vec2::new(0.0, r), center + Vec2::new(-r, 0.0)];
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A button on parchment pages (primary: gold, the main action of the page).
pub fn page_button(ui: &mut Ui, text: &str, primary: bool, enabled: bool) -> Response {
    let font = heading_font(16.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), Palette::INK);
    let size = Vec2::new((galley.size().x + 36.0).max(130.0), 36.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let painter = ui.painter();
    let (fill, text_color) = match (enabled, primary, hovered) {
        (false, _, _) => (Palette::PARCHMENT_SHADE.gamma_multiply(0.7), Palette::INK_SOFT.gamma_multiply(0.7)),
        (true, true, false) => (Palette::GOLD, Palette::INK),
        (true, true, true) => (Palette::GOLD_BRIGHT, Palette::INK),
        (true, false, false) => (Palette::PARCHMENT_DARK, Palette::INK),
        (true, false, true) => (Palette::GOLD_BRIGHT.gamma_multiply(0.8), Palette::INK),
    };
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    let stroke = if hovered || response.has_focus() { Stroke::new(1.8, Palette::WOOD) } else { Stroke::new(1.2, Palette::GOLD_DARK) };
    painter.rect_stroke(rect, CornerRadius::same(5), stroke, egui::StrokeKind::Inside);
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, text_color);
    response
}

/// Same as [`page_button`] in red for destructive actions (delete, leave without saving).
pub fn danger_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    let font = heading_font(16.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), Palette::PARCHMENT_LIGHT);
    let size = Vec2::new((galley.size().x + 36.0).max(130.0), 36.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let painter = ui.painter();
    let (fill, text_color) = if !enabled {
        (Palette::PARCHMENT_SHADE.gamma_multiply(0.7), Palette::INK_SOFT.gamma_multiply(0.7))
    } else if hovered {
        (Palette::BAD_LIGHT, Palette::PARCHMENT_LIGHT)
    } else {
        (Palette::BAD, Palette::PARCHMENT_LIGHT)
    };
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    painter.rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.2, Palette::WOOD), egui::StrokeKind::Inside);
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, text_color);
    response
}

/// Page title with a gold rule below.
pub fn page_title(ui: &mut Ui, title: &str, subtitle: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(heading_font(30.0)).color(Palette::INK));
        if let Some(sub) = subtitle {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(sub).color(Palette::INK_SOFT));
        }
    });
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 8.0), Sense::hover());
    let y = rect.center().y;
    let painter = ui.painter();
    painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.2, Palette::GOLD_DARK));
    diamond(painter, Pos2::new(rect.left() + 3.0, y), 3.0, Palette::GOLD);
    diamond(painter, Pos2::new(rect.right() - 3.0, y), 3.0, Palette::GOLD);
}

/// The rectangle of a page of `size` centred on the screen (shrunk to fit).
pub fn page_rect(ctx: &egui::Context, size: Vec2) -> Rect {
    let screen = ctx.content_rect();
    let size = Vec2::new(size.x.min(screen.width() - 2.0 * PAGE_MARGIN), size.y.min(screen.height() - 2.0 * PAGE_MARGIN));
    Rect::from_center_size(screen.center(), size.max(Vec2::splat(200.0)))
}

/// Shows a parchment page of `size` in the middle of the screen; `add` fills it (the inner
/// size is fixed, so pages do not jump while their content changes).
pub fn show_page<R>(ctx: &egui::Context, id: &str, size: Vec2, add: impl FnOnce(&mut Ui) -> R) -> R {
    let rect = page_rect(ctx, size);
    let inner = rect.size() - Vec2::splat(2.0 * 22.0);
    egui::Area::new(egui::Id::unique(id))
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            theme::parchment_frame()
                .inner_margin(egui::Margin::same(20))
                .show(ui, |ui| {
                    ui.set_width(inner.x);
                    ui.set_height(inner.y);
                    add(ui)
                })
                .inner
        })
        .inner
}

/// Darkens the screen behind menus: a soft vignette plus a stronger shade on the left where the
/// main menu sits (`left`: 0–1).
pub fn backdrop(ctx: &egui::Context, left: f32, overall: f32) {
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    if overall > 0.0 {
        painter.rect_filled(screen, CornerRadius::ZERO, Color32::from_black_alpha((overall.clamp(0.0, 1.0) * 255.0) as u8));
    }
    if left > 0.0 {
        let a = (left.clamp(0.0, 1.0) * 235.0) as u8;
        let dark = Color32::from_rgba_unmultiplied(16, 11, 7, a);
        let mid = Color32::from_rgba_unmultiplied(16, 11, 7, a / 2);
        let clear = Color32::from_rgba_unmultiplied(16, 11, 7, 0);
        let x1 = screen.left() + screen.width() * 0.28;
        let x2 = screen.left() + screen.width() * 0.6;
        gradient(&painter, Rect::from_min_max(screen.min, Pos2::new(x1, screen.bottom())), dark, mid);
        gradient(&painter, Rect::from_min_max(Pos2::new(x1, screen.top()), Pos2::new(x2, screen.bottom())), mid, clear);
    }
    // Top and bottom vignette bands.
    let band = (screen.height() * 0.18).max(40.0);
    let shade = Color32::from_rgba_unmultiplied(10, 7, 4, 150);
    let clear = Color32::from_rgba_unmultiplied(10, 7, 4, 0);
    vertical_gradient(&painter, Rect::from_min_max(screen.min, Pos2::new(screen.right(), screen.top() + band)), shade, clear);
    vertical_gradient(&painter, Rect::from_min_max(Pos2::new(screen.left(), screen.bottom() - band), screen.max), clear, shade);
}

/// The dark wood screen of loading and waiting (vertical gradient, lighter in the middle) with
/// opacity `alpha` (0–1).
pub fn wood_backdrop(ctx: &egui::Context, alpha: f32) {
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    let top = Color32::from_rgba_unmultiplied(38, 27, 18, a);
    let middle = Color32::from_rgba_unmultiplied(58, 41, 26, a);
    let bottom = Color32::from_rgba_unmultiplied(20, 14, 9, a);
    let mid_y = screen.top() + screen.height() * 0.45;
    vertical_gradient(&painter, Rect::from_min_max(screen.min, Pos2::new(screen.right(), mid_y)), top, middle);
    vertical_gradient(&painter, Rect::from_min_max(Pos2::new(screen.left(), mid_y), screen.max), middle, bottom);
}

/// Horizontal colour gradient over `rect` (left → right).
pub fn gradient(painter: &egui::Painter, rect: Rect, left: Color32, right: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Vertical colour gradient over `rect` (top → bottom).
pub fn vertical_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// The game title ("KINGDOMS", "Rise of an Empire") with a shadow, at `top_left`.
pub fn paint_title(painter: &egui::Painter, top_left: Pos2, scale: f32) -> Rect {
    let big = heading_font(74.0 * scale);
    let small = heading_font(25.0 * scale);
    let shadow = Color32::from_black_alpha(170);
    let title = "KINGDOMS";
    let galley = painter.layout_no_wrap(title.to_owned(), big.clone(), Palette::GOLD_BRIGHT);
    let size = galley.size();
    for d in [Vec2::new(3.0, 4.0), Vec2::new(1.5, 2.0)] {
        painter.text(top_left + d * scale, Align2::LEFT_TOP, title, big.clone(), shadow);
    }
    painter.galley(top_left, galley, Palette::GOLD_BRIGHT);
    let rule_y = top_left.y + size.y + 6.0 * scale;
    let rule = Stroke::new(1.5, Palette::GOLD);
    painter.line_segment([Pos2::new(top_left.x, rule_y), Pos2::new(top_left.x + size.x, rule_y)], rule);
    diamond(painter, Pos2::new(top_left.x + size.x * 0.5, rule_y), 5.0 * scale, Palette::GOLD_BRIGHT);
    let sub_pos = Pos2::new(top_left.x + size.x * 0.5, rule_y + 10.0 * scale);
    painter.text(sub_pos + Vec2::new(1.5, 2.0), Align2::CENTER_TOP, "Rise of an Empire", small.clone(), shadow);
    let sub = painter.text(sub_pos, Align2::CENTER_TOP, "Rise of an Empire", small, Palette::ON_WOOD);
    Rect::from_min_max(top_left, Pos2::new(top_left.x + size.x, sub.bottom()))
}

/// A rotating ring of gold dots (busy indicator) at `center`; `t` is the time in seconds.
pub fn spinner(painter: &egui::Painter, center: Pos2, radius: f32, t: f64) {
    let dots = 10;
    let head = (t * 1.6).fract() as f32 * dots as f32;
    for i in 0..dots {
        let angle = i as f32 / dots as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let p = center + Vec2::new(angle.cos(), angle.sin()) * radius;
        // Brightest at the head, fading behind it.
        let behind = (head - i as f32).rem_euclid(dots as f32);
        let strength = (1.0 - behind / dots as f32).powf(1.6);
        let color = Palette::GOLD_BRIGHT.gamma_multiply(0.2 + 0.8 * strength);
        painter.circle_filled(p, radius * 0.16 * (0.7 + 0.5 * strength), color);
    }
}

/// Keyboard navigation of a vertical list of `count` entries: Up/Down (or W/S) move the
/// selection, Enter/Space activate it. Returns the entry to activate.
pub fn list_navigation(ctx: &egui::Context, selected: &mut usize, count: usize, enabled: impl Fn(usize) -> bool) -> Option<usize> {
    if count == 0 || ctx.memory(|m| m.focused().is_some()) {
        return None;
    }
    let (up, down, activate) = ctx.input_mut(|i| {
        let up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) || i.consume_key(egui::Modifiers::NONE, egui::Key::W);
        let down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) || i.consume_key(egui::Modifiers::NONE, egui::Key::S);
        let activate = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) || i.consume_key(egui::Modifiers::NONE, egui::Key::Space);
        (up, down, activate)
    });
    let step = |from: usize, forward: bool| {
        let mut i = from;
        for _ in 0..count {
            i = if forward { (i + 1) % count } else { (i + count - 1) % count };
            if enabled(i) {
                return i;
            }
        }
        from
    };
    if !enabled(*selected) {
        *selected = step(*selected, true);
    }
    if up {
        *selected = step(*selected, false);
    }
    if down {
        *selected = step(*selected, true);
    }
    (activate && enabled(*selected)).then_some(*selected)
}

/// True if Esc was pressed this frame (and consumes it).
pub fn escape_pressed(ctx: &egui::Context) -> bool {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

/// A small caption above a form field.
pub fn field_label(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(heading_font(14.0)).color(Palette::INK_SOFT));
}

/// Informational text in a muted colour.
pub fn note(ui: &mut Ui, text: impl Into<String>) {
    ui.add(egui::Label::new(egui::RichText::new(text.into()).color(Palette::INK_SOFT).font(FontId::proportional(14.0))).wrap());
}
