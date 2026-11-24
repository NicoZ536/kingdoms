//! Bottom bar (centre): the selection tools (inspect, road, demolish, move, grid, territory
//! border), the game speed with pause, and the time of day as a sun/moon dial.

use kingdoms_engine::core::time::GameSpeed;
use kingdoms_engine::egui::{self, Align2, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use kingdoms_sim::defs::RoadTypeId;
use kingdoms_sim::time::GameClock;

use super::{HudAction, HudInput, Icons, MARGIN, ToolRequest};
use crate::actions::Action;
use crate::tools::Tool;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::icon_button;

/// Size of the tool and speed buttons.
const BUTTON: f32 = 38.0;

/// The bottom bar; remembers the road type of the road tool.
#[derive(Default)]
pub struct BottomBar {
    road: Option<RoadTypeId>,
}

impl BottomBar {
    /// Draws the bar; returns the rectangle it covers.
    pub fn show(&mut self, ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons, actions: &mut Vec<HudAction>) -> Rect {
        let tool = input.tools.tool();
        if let Tool::Road { road } = tool {
            self.road = Some(road);
        }
        egui::Area::new(egui::Id::unique("hud_bottom_bar"))
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -MARGIN))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::wood_frame().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                    theme::on_wood(ui);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        self.tools(ui, input, icons, tool, actions);
                        separator(ui);
                        speeds(ui, input, icons, actions);
                        separator(ui);
                        clock(ui, input);
                    });
                });
            })
            .response
            .rect
    }

    fn tools(&mut self, ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, tool: Tool, actions: &mut Vec<HudAction>) {
        let inspect = tool == Tool::Inspect;
        if icon_button(ui, icons.get("ui_inspect"), "?", inspect, BUTTON)
            .on_hover_text(format!(
                "Auswählen ({}): Gebäude, Felder und Bewohner anklicken. Rechtsklick/Esc beendet andere Werkzeuge.",
                input.hint(Action::Select)
            ))
            .clicked()
            && !inspect
        {
            actions.push(HudAction::Tool(ToolRequest::Inspect));
        }
        let state = input.session.state();
        let defs = input.session.defs();
        // The last road type used, else the best unlocked one without a bridge.
        let road = self.road.filter(|r| state.research().road_unlocked(defs, *r)).or_else(|| {
            defs.roads().iter().filter(|r| r.bridge.is_none() && state.research().road_unlocked(defs, r.id)).map(|r| r.id).next()
        });
        let road_tool = matches!(tool, Tool::Road { .. });
        if let Some(road) = road {
            let name = defs.roads().get(road.index()).map_or("Straße", |r| r.name.as_str());
            if icon_button(ui, icons.get("ui_roads"), "S", road_tool, BUTTON)
                .on_hover_text(format!(
                    "Straße bauen: {name} (mit gedrückter Maustaste ziehen). Weitere Straßentypen im Menü „Straßen“ links."
                ))
                .clicked()
            {
                actions.push(HudAction::Tool(if road_tool { ToolRequest::Inspect } else { ToolRequest::Road(road) }));
            }
        }
        let demolish = tool == Tool::Demolish;
        if icon_button(ui, icons.get("ui_demolish"), "X", demolish, BUTTON)
            .on_hover_text(format!(
                "Abreißen ({}): Gebäude anklicken, Straßen anklicken oder ziehen. Rechtsklick/Esc beendet.",
                input.hint(Action::DemolishTool)
            ))
            .clicked()
        {
            actions.push(HudAction::Tool(if demolish { ToolRequest::Inspect } else { ToolRequest::Demolish }));
        }
        let moving = matches!(tool, Tool::Move { .. });
        if icon_button(ui, icons.get("ui_move"), "M", moving, BUTTON)
            .on_hover_text(format!(
                "Versetzen ({}): Gebäude anklicken, dann am neuen Ort platzieren ({} dreht).",
                input.hint(Action::MoveTool),
                input.hint(Action::Rotate)
            ))
            .clicked()
        {
            actions.push(HudAction::Tool(if moving { ToolRequest::Inspect } else { ToolRequest::Move(None) }));
        }
        ui.add_space(4.0);
        if icon_button(ui, icons.get("ui_grid"), "#", input.show_grid, BUTTON)
            .on_hover_text(format!("Raster ein/aus ({})", input.hint(Action::ToggleGrid)))
            .clicked()
        {
            actions.push(HudAction::ToggleGrid);
            actions.push(HudAction::Sound("ui_click"));
        }
        if icon_button(ui, icons.get("ui_territory"), "T", input.show_territory, BUTTON)
            .on_hover_text(format!("Gebietsgrenze ein/aus ({})", input.hint(Action::ToggleTerritory)))
            .clicked()
        {
            actions.push(HudAction::ToggleTerritory);
            actions.push(HudAction::Sound("ui_click"));
        }
    }
}

fn separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(11.0, BUTTON - 6.0), Sense::hover());
    ui.painter().line_segment([rect.center_top(), rect.center_bottom()], Stroke::new(1.0, Palette::GOLD_DARK));
}

/// Pause and the three speeds.
fn speeds(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, actions: &mut Vec<HudAction>) {
    let speeds = [
        (GameSpeed::Paused, "ui_pause", "II", "Pause", Action::TogglePause),
        (GameSpeed::Normal, "ui_play", "1×", "Normale Geschwindigkeit", Action::SpeedNormal),
        (GameSpeed::Double, "ui_speed2", "2×", "Doppelte Geschwindigkeit", Action::SpeedDouble),
        (GameSpeed::Quadruple, "ui_speed4", "4×", "Vierfache Geschwindigkeit", Action::SpeedQuadruple),
    ];
    for (speed, icon, label, text, action) in speeds {
        let selected = input.speed == speed;
        if icon_button(ui, icons.get(icon), label, selected, BUTTON).on_hover_text(format!("{text} ({})", input.hint(action))).clicked() && !selected {
            actions.push(HudAction::Speed(speed));
        }
    }
}

/// Position of the sun (day) or moon (night) on the dial for `minute_of_day` (0–1439):
/// 0.0 = rising on the left, 0.5 = highest, 1.0 = setting on the right. Day is 6–18 Uhr.
pub fn dial_position(minute_of_day: u32) -> (bool, f32) {
    let minute = minute_of_day % (24 * 60);
    let day = (6 * 60..18 * 60).contains(&minute);
    let start = if day { 6 * 60 } else if minute >= 18 * 60 { 18 * 60 } else { 18 * 60 - 24 * 60 };
    let offset = i64::from(minute) - i64::from(start);
    (day, offset as f32 / (12.0 * 60.0))
}

/// The time of day: a half dial with the sun by day and the moon by night, then the clock.
fn clock(ui: &mut egui::Ui, input: &HudInput<'_>) {
    let clock: GameClock = input.session.state().clock();
    let minute = u32::try_from(clock.hour() * 60 + clock.minute()).unwrap_or(0);
    let (day, t) = dial_position(minute);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(44.0, BUTTON), Sense::hover());
    let painter = ui.painter();
    let centre = Pos2::new(rect.center().x, rect.bottom() - 6.0);
    let radius = 18.0;
    let sky = if day { Color32::from_rgb(120, 160, 200) } else { Color32::from_rgb(34, 42, 78) };
    let points: Vec<Pos2> = (0..=16)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 - i as f32 / 16.0);
            centre + Vec2::new(a.cos() * radius, -a.sin() * radius)
        })
        .collect();
    let mut fan = vec![centre];
    fan.extend(points.iter().copied());
    painter.add(egui::Shape::convex_polygon(fan, sky, Stroke::new(1.0, Palette::GOLD_DARK)));
    painter.line_segment([centre - Vec2::new(radius + 2.0, 0.0), centre + Vec2::new(radius + 2.0, 0.0)], Stroke::new(1.5, Palette::GOLD_DARK));
    let a = std::f32::consts::PI * (1.0 - t.clamp(0.0, 1.0));
    let body = centre + Vec2::new(a.cos() * (radius - 5.0), -a.sin() * (radius - 5.0));
    if day {
        painter.circle_filled(body, 4.5, Palette::GOLD_BRIGHT);
    } else {
        painter.circle_filled(body, 4.0, Palette::PARCHMENT_LIGHT);
        painter.circle_filled(body + Vec2::new(1.8, -1.2), 3.4, sky);
    }
    response.on_hover_text(format!("{} · {} Uhr", clock.date_string(), clock.time_string()));
    ui.label(RichText::new(format!("{} Uhr", clock.time_string())).font(heading_font(16.0)).color(Palette::GOLD_BRIGHT));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sun_crosses_the_dial_by_day_and_the_moon_by_night() {
        assert_eq!(dial_position(6 * 60), (true, 0.0));
        assert_eq!(dial_position(12 * 60), (true, 0.5));
        let (day, t) = dial_position(18 * 60 - 1);
        assert!(day && t > 0.99);
        assert_eq!(dial_position(18 * 60), (false, 0.0));
        assert_eq!(dial_position(0), (false, 0.5));
        let (day, t) = dial_position(6 * 60 - 1);
        assert!(!day && t > 0.99);
    }
}
