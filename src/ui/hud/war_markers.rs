//! Markers of war on the screen: approaching and attacking enemies (`report::threats_report`)
//! as red arrows at the edge of the view pointing to them (or a label over them while they are
//! in view), with their kind, size and time until arrival (the hover text names the clock time);
//! a click moves the camera there. While the
//! order tool is active a banner at the top explains the pick and offers „Fertig“ and
//! „Abbrechen“.

use glam::{IVec2, Vec2};
use kingdoms_engine::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke};
use kingdoms_engine::rendering::iso;
use kingdoms_sim::report::ThreatRow;

use super::{HudAction, HudInput, MARGIN, TOP_BAR_HEIGHT, ToolRequest};
use crate::tools::{OrderPick, Tool};
use crate::ui::theme::{self, Palette, heading_font};
use crate::view::citizens::walker_world_position;

/// Inset of the edge markers' centres from the sides of the free map area (half a chip and
/// its arrow).
const INSET_SIDES: f32 = 130.0;
/// Inset from the top of the free area (or the order banner) and from its bottom.
const INSET_TOP: f32 = 36.0;
const INSET_BOTTOM: f32 = 30.0;
/// Height of a standing figure in world pixels (head to feet), and the gap the label keeps
/// above the highest head of its group (screen points).
const FIGURE_HEIGHT: f32 = 28.0;
const LABEL_GAP: f32 = 6.0;
/// Distance in tiles from the median fighter within which fighters count as the group's body.
const GROUP_REACH: f32 = 3.0;

/// The part of the screen the map shows without HUD on it: right of the left bar (`left_bar`),
/// left of `right` (the notification cards or the panels on the right), between the top bar
/// and the bottom bar.
pub fn free_area(screen: Rect, left_bar: Rect, right: f32, bottom_bar: Rect) -> Rect {
    let left = if left_bar.is_positive() { left_bar.right() + MARGIN } else { screen.left() + MARGIN };
    let bottom = if bottom_bar.is_positive() { bottom_bar.top() - MARGIN } else { screen.bottom() - MARGIN };
    let min = Pos2::new(left, screen.top() + TOP_BAR_HEIGHT + MARGIN);
    let max = Pos2::new(right.min(screen.right() - MARGIN), bottom);
    Rect::from_min_max(min, max.max(min + egui::vec2(1.0, 1.0)))
}

/// Where the threat markers may go inside the free area `free`, below the order banner
/// `banner` (if shown); never empty.
pub fn marker_area(free: Rect, banner: Option<Rect>) -> Rect {
    let top = banner.map_or(free.top(), |b| b.bottom().max(free.top())) + INSET_TOP;
    let min = Pos2::new(free.left() + INSET_SIDES, top);
    let max = Pos2::new(free.right() - INSET_SIDES, free.bottom() - INSET_BOTTOM);
    if max.x > min.x && max.y > min.y {
        Rect::from_min_max(min, max)
    } else {
        // Too little room: shrink towards the centre of the free area.
        Rect::from_center_size(free.center(), egui::vec2(1.0, 1.0))
    }
}

/// Where a marker goes: on the point itself while it is inside `inner`, else on the border of
/// `inner` towards it (from the centre), with the direction it points in.
pub fn marker_spot(inner: Rect, point: Pos2) -> (Pos2, Option<egui::Vec2>) {
    if inner.contains(point) {
        return (point, None);
    }
    let c = inner.center();
    let d = point - c;
    let sx = if d.x.abs() > 1e-3 { (inner.width() * 0.5) / d.x.abs() } else { f32::INFINITY };
    let sy = if d.y.abs() > 1e-3 { (inner.height() * 0.5) / d.y.abs() } else { f32::INFINITY };
    let s = sx.min(sy).min(1.0);
    let dir = if d.length() > 1e-3 { d / d.length() } else { egui::vec2(0.0, -1.0) };
    (c + d * s, Some(dir))
}

/// Screen point (egui points) of `tile` for the camera showing world rectangle `view` on
/// `screen`.
pub fn tile_on_screen(input: &HudInput<'_>, screen: Rect, tile: IVec2) -> Pos2 {
    let level = input.session.state().world().get(tile).map_or(0.0, |t| f32::from(t.elevation));
    world_on_screen(input, screen, iso::tile_center_world(tile, level))
}

/// Screen point (egui points) of world point `w` for the camera of `input` on `screen`.
fn world_on_screen(input: &HudInput<'_>, screen: Rect, w: Vec2) -> Pos2 {
    let view = input.view;
    let size = (view.max - view.min).max(Vec2::splat(1.0));
    let f = (w - view.min) / size;
    Pos2::new(screen.left() + f.x * screen.width(), screen.top() + f.y * screen.height())
}

/// The middle and top (screen points) of a band or army on the map: above the highest head,
/// counting the spread of a crowd on one spot ([`crate::view::citizens::crowd_offset`]);
/// `None` while none of its fighters walks on the map.
fn group_top(input: &HudInput<'_>, screen: Rect, warband: u32) -> Option<Pos2> {
    let state = input.session.state();
    let w = state.military().warbands().iter().find(|w| w.id() == warband)?;
    let walking = state.population().walking();
    let feet: Vec<Pos2> = w
        .fighters()
        .iter()
        .filter_map(|f| walking.traveler(f.walker()))
        // Where the map draws the fighter this frame (between the last two ticks).
        .map(|t| world_on_screen(input, screen, walker_world_position(state, t.motion().prev_pos(), t.motion().pos(), input.alpha)))
        .collect();
    if feet.is_empty() {
        return None;
    }
    let scale = screen.height() / (input.view.max.y - input.view.min.y).max(1.0);
    // The main body: around the median fighter (stragglers of a fleeing band do not pull the
    // label away).
    let median = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    let centre = Pos2::new(median(feet.iter().map(|p| p.x).collect()), median(feet.iter().map(|p| p.y).collect()));
    let reach = GROUP_REACH * iso::TILE_WIDTH * scale;
    let body: Vec<Pos2> = feet.iter().copied().filter(|p| p.distance(centre) <= reach).collect();
    // The widest crowd: all fighters on one spot spread over a disc of this radius (tiles); a
    // tile step towards the viewer is up to √2 × half a tile height on screen.
    let spread = crate::view::citizens::crowd_offset(body.len()).length() * iso::HALF_TILE_HEIGHT * std::f32::consts::SQRT_2;
    let top = body.iter().map(|p| p.y).fold(centre.y, f32::min) - (FIGURE_HEIGHT + spread) * scale;
    Some(Pos2::new(centre.x, top))
}

/// Draws a marker per threat inside the free map area `free` (below the order banner
/// `banner`); a click focuses the camera on it.
pub fn show_threats(ctx: &egui::Context, input: &HudInput<'_>, threats: &[ThreatRow], free: Rect, banner: Option<Rect>, actions: &mut Vec<HudAction>) {
    if threats.is_empty() {
        return;
    }
    let screen = ctx.content_rect();
    let inner = marker_area(free, banner);
    let pulse = 0.6 + 0.4 * ((input.now * 4.0).sin() as f32 * 0.5 + 0.5);
    for t in threats {
        let point = tile_on_screen(input, screen, t.position);
        let (spot, dir) = marker_spot(inner, point);
        // In view the label floats above the group's highest figure instead of covering it.
        let (spot, pivot) = match (dir, group_top(input, screen, t.warband)) {
            (None, Some(top)) if inner.contains(top) => (top - egui::vec2(0.0, LABEL_GAP), Align2::CENTER_BOTTOM),
            (None, _) => (spot - egui::vec2(0.0, 46.0), Align2::CENTER_CENTER),
            (Some(_), _) => (spot, Align2::CENTER_CENTER),
        };
        let color = t.kingdom.and_then(|k| input.session.state().diplomacy().kingdom(k)).map_or(Palette::BAD, |k| theme::kingdom_color(k.color()));
        let label = format!("{} ({}) · {}", t.kind_text, t.size, if t.eta_ticks.is_some() { t.eta_text.as_str() } else { t.stage_text.as_str() });
        let id = egui::Id::unique(("threat_marker", t.warband));
        let area = egui::Area::new(id).fixed_pos(spot).pivot(pivot).order(egui::Order::Middle).show(ctx, |ui| {
            let galley = ui.painter().layout_no_wrap(label.clone(), FontId::proportional(13.0), Color32::WHITE);
            let size = galley.size() + egui::vec2(34.0, 8.0);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let painter = ui.painter();
            painter.rect_filled(rect, CornerRadius::same(10), Palette::WOOD_DARK.gamma_multiply(0.92));
            painter.rect_stroke(rect, CornerRadius::same(10), Stroke::new(2.0, Palette::BAD.gamma_multiply(pulse)), egui::StrokeKind::Inside);
            let dot = Pos2::new(rect.left() + 13.0, rect.center().y);
            painter.circle_filled(dot, 6.0, color);
            painter.circle_stroke(dot, 6.0, Stroke::new(1.5, Palette::BAD_LIGHT));
            painter.galley(Pos2::new(rect.left() + 25.0, rect.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
            if let Some(dir) = dir {
                // Arrow outside the chip, pointing to the enemy.
                let tip = rect.center() + dir * (rect.width().max(rect.height()) * 0.5 + 16.0);
                let base = tip - dir * 14.0;
                let side = egui::vec2(-dir.y, dir.x) * 8.0;
                painter.add(egui::Shape::convex_polygon(vec![tip, base + side, base - side], Palette::BAD.gamma_multiply(pulse), Stroke::new(1.0, Palette::WOOD_DARK)));
            }
            response
        });
        if area.inner.on_hover_text(threat_tooltip(t, input.session.state().clock().tick)).clicked() {
            actions.push(HudAction::Focus(t.position));
            actions.push(HudAction::Sound("ui_click"));
        }
    }
    ctx.request_repaint();
}

/// Hover text of a threat marker: composition, stage with the clock time of arrival while it
/// approaches (game tick `now`), target and the click hint.
pub fn threat_tooltip(t: &ThreatRow, now: u64) -> String {
    let stage = match t.eta_ticks {
        Some(eta) => format!("{} · Ankunft {}", t.stage_text, kingdoms_sim::report::deadline_clock_text(now, now + eta)),
        None => t.stage_text.clone(),
    };
    format!("{}\n{stage} · Ziel: {}\nKlick: Kamera hin", t.composition_text, t.target_name)
}

/// The banner of the order tool (what to click, waypoints, finish and cancel), centred at the
/// top of the free map area `free` and no wider than it; returns its rectangle.
pub fn order_banner(ctx: &egui::Context, input: &HudInput<'_>, free: Rect, actions: &mut Vec<HudAction>) -> Option<Rect> {
    let Tool::Order { company, pick } = input.tools.tool() else { return None };
    let state = input.session.state();
    let name = state.military().company(company).map_or_else(|| "Kompanie".to_owned(), |c| c.name().to_owned());
    let max = input.session.defs().rules().military.max_patrol_points;
    let points = input.tools.patrol_points().len();
    let (title, text) = match pick {
        OrderPick::Defend { radius } => (
            format!("{name}: Verteidigen"),
            format!(
                "Klick auf ein Gebäude: Besatzung · Klick auf einen Ort: Gebiet im Umkreis von {}",
                kingdoms_sim::report::tiles_dative_text(u32::from(radius))
            ),
        ),
        OrderPick::Patrol => (
            format!("{name}: Patrouille ({points} von {max} Wegpunkten)"),
            "Klick setzt einen Wegpunkt · Klick auf den ersten oder Eingabe gibt den Befehl · Rechtsklick/Esc nimmt den letzten zurück".to_owned(),
        ),
    };
    // Frame margin and stroke on both sides, the buttons on the right.
    let max_text = (free.width().min(BANNER_MAX_WIDTH) - 2.0 * BANNER_FRAME - BANNER_BUTTONS).max(160.0);
    let shown = egui::Area::new(egui::Id::unique("hud_order_banner"))
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(Pos2::new(free.center().x, free.top()))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            theme::wood_frame().show(ui, |ui| {
                theme::on_wood(ui);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(max_text);
                        ui.label(egui::RichText::new(title).font(heading_font(16.0)).color(Palette::GOLD_BRIGHT));
                        ui.add(egui::Label::new(egui::RichText::new(text).small()).wrap());
                    });
                    if pick == OrderPick::Patrol {
                        let done = ui.add_enabled(points >= 2, egui::Button::new("Fertig")).on_disabled_hover_text("Mindestens 2 Wegpunkte");
                        if done.clicked() {
                            actions.push(HudAction::FinishPatrol);
                        }
                    }
                    if ui.button("Abbrechen").clicked() {
                        actions.push(HudAction::Tool(ToolRequest::Inspect));
                        actions.push(HudAction::Sound("ui_close"));
                    }
                });
            });
        });
    Some(shown.response.rect)
}

/// Widest the order banner gets.
const BANNER_MAX_WIDTH: f32 = 760.0;
/// Inner margin and stroke of the wood frame on one side.
const BANNER_FRAME: f32 = 16.0;
/// Room of the „Fertig“ and „Abbrechen“ buttons.
const BANNER_BUTTONS: f32 = 160.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_stay_on_the_border_towards_the_enemy() {
        let inner = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(500.0, 300.0));
        // Inside: on the point, no arrow.
        assert_eq!(marker_spot(inner, Pos2::new(200.0, 150.0)), (Pos2::new(200.0, 150.0), None));
        // Far right: on the right border at the centre's height, pointing right.
        let (spot, dir) = marker_spot(inner, Pos2::new(2000.0, 200.0));
        assert!((spot.x - 500.0).abs() < 1e-3 && (spot.y - 200.0).abs() < 1e-3);
        assert!(dir.is_some_and(|d| d.x > 0.99));
        // Above-left: clamped to the top border.
        let (spot, dir) = marker_spot(inner, Pos2::new(250.0, -1000.0));
        assert!((spot.y - 100.0).abs() < 1e-3 && inner.expand(0.01).contains(spot));
        assert!(dir.is_some_and(|d| d.y < 0.0));
    }

    #[test]
    fn approaching_threats_name_their_arrival_time() {
        let mut t = ThreatRow {
            warband: 1,
            kind_text: "Räuberbande".to_owned(),
            kingdom: None,
            size: 5,
            composition_text: "5 Räuber".to_owned(),
            stage_text: "Im Anmarsch".to_owned(),
            eta_ticks: Some(8 * kingdoms_sim::time::TICKS_PER_HOUR),
            eta_text: "in 8 h".to_owned(),
            target: None,
            target_name: "Lager".to_owned(),
            position: IVec2::ZERO,
            loot_units: 0,
        };
        // Day 1, 09:00 (the game starts at 06:00) plus 8 h: 17:00 the same day.
        let now = 3 * kingdoms_sim::time::TICKS_PER_HOUR;
        assert_eq!(threat_tooltip(&t, now), "5 Räuber\nIm Anmarsch · Ankunft 17:00 Uhr · Ziel: Lager\nKlick: Kamera hin");
        t.eta_ticks = None;
        t.stage_text = "Greift an".to_owned();
        assert_eq!(threat_tooltip(&t, now), "5 Räuber\nGreift an · Ziel: Lager\nKlick: Kamera hin");
    }

    #[test]
    fn markers_and_the_banner_keep_clear_of_the_cards() {
        let screen = Rect::from_min_max(Pos2::ZERO, Pos2::new(1280.0, 720.0));
        let left_bar = Rect::from_min_max(Pos2::new(8.0, 60.0), Pos2::new(94.0, 420.0));
        let bottom_bar = Rect::from_min_max(Pos2::new(420.0, 660.0), Pos2::new(860.0, 712.0));
        // Notification cards from x = 880 on.
        let free = free_area(screen, left_bar, 872.0, bottom_bar);
        assert!(free.left() > left_bar.right() && free.right() <= 872.0 && free.bottom() < bottom_bar.top());
        let area = marker_area(free, None);
        assert!(free.contains_rect(area) && area.right() + INSET_SIDES <= free.right() + 0.01);
        // Markers start below the order banner.
        let banner = Rect::from_min_max(Pos2::new(300.0, free.top()), Pos2::new(700.0, free.top() + 60.0));
        assert!(marker_area(free, Some(banner)).top() >= banner.bottom());
        // Every marker spot towards any enemy lies inside the area.
        for p in [Pos2::new(5000.0, 300.0), Pos2::new(-5000.0, -100.0), Pos2::new(600.0, 5000.0)] {
            assert!(area.expand(0.01).contains(marker_spot(area, p).0));
        }
        // Without room the area collapses to the centre instead of turning inside out.
        let tiny = free_area(screen, left_bar, 150.0, bottom_bar);
        let collapsed = marker_area(tiny, None);
        assert!(collapsed.width() >= 0.0 && collapsed.height() >= 0.0);
    }
}
