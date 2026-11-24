//! Decision dialog of the pending event (docs/game_design.md §12): a parchment card with the
//! event's title and text, the building it concerns, a countdown to the default choice and one
//! card per choice with its predicted effects (or why it is not possible now). A choice queues
//! `Command::ResolveEvent`. The dialog opens by itself when a new event starts; „Später
//! entscheiden“ hides it until the player opens it again (right bar, `E`).

use kingdoms_engine::egui::{self, Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use kingdoms_sim::command::Command;
use kingdoms_sim::event::Severity;
use kingdoms_sim::report::{self, EventChoiceRow, PendingEventReport};
use kingdoms_sim::time::SECONDS_PER_TICK;

use super::feed::severity_color;
use super::{HudAction, HudInput, Icons, TOP_BAR_HEIGHT};
use crate::actions::Action;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Mark, icon_button, mark};

/// Width of the dialog.
const WIDTH: f32 = 560.0;
/// Frames the dialog is kept on top after it opened.
const RAISE_FRAMES: u8 = 3;

/// German countdown to the default choice from game minutes, e.g. „noch 7 h 30 min“.
pub fn countdown_text(minutes_left: u64) -> String {
    if minutes_left == 0 { "die Frist läuft ab".to_owned() } else { format!("noch {}", super::format::duration_minutes(minutes_left)) }
}

/// Text of the link to the place of an event: „Gebäude zeigen“ when the event text already
/// names the building („Im Gebäude „Mühle“ …“), „Betrifft: Mühle – zeigen“ when it does not,
/// „Ort des Ereignisses – zeigen“ without a building.
pub fn place_link_text(text: &str, building: Option<&str>) -> String {
    match building {
        Some(name) if text.contains(&format!("„{name}“")) => "Gebäude zeigen".to_owned(),
        Some(name) => format!("Betrifft: {name} – zeigen"),
        None => "Ort des Ereignisses – zeigen".to_owned(),
    }
}

/// End of the decision time as a clock time seen from tick `now`, e.g. „Frist bis 18:00 Uhr“
/// or „Frist bis morgen 06:00 Uhr“ ([`report::deadline_clock_text`]; the time left is shown
/// once, by [`countdown_text`]).
pub fn deadline_clock_text(now: u64, expires_tick: u64) -> String {
    format!("Frist bis {}", report::deadline_clock_text(now, expires_tick))
}

/// Share of the decision time left (1.0 right after the event started).
pub fn time_left_fraction(event: &PendingEventReport) -> f32 {
    let total = event.expires_tick.saturating_sub(event.started_tick) * SECONDS_PER_TICK / 60;
    if total == 0 { 0.0 } else { (event.minutes_left as f32 / total as f32).clamp(0.0, 1.0) }
}

/// The choice queued for `event` while the game waits for its next tick.
pub fn queued_choice(input: &HudInput<'_>, event: &PendingEventReport) -> Option<u8> {
    input.session.queued_commands().iter().find_map(|c| match c {
        Command::ResolveEvent { serial, choice } if *serial == event.serial => Some(*choice),
        _ => None,
    })
}

/// State of the dialog.
#[derive(Default)]
pub struct EventDialog {
    open: bool,
    /// Serial of the last event the dialog opened for by itself.
    seen: Option<u32>,
    raise_frames: u8,
}

impl EventDialog {
    /// Opens the dialog when `pending` is a new event; closes it when nothing is pending.
    pub fn update(&mut self, pending: Option<&PendingEventReport>) {
        match pending {
            Some(p) if self.seen != Some(p.serial) => {
                self.seen = Some(p.serial);
                self.set_open(true);
            }
            Some(_) => {}
            None => self.open = false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool) {
        if open && !self.open {
            self.raise_frames = RAISE_FRAMES;
        }
        self.open = open;
    }

    /// Draws the dialog for `event` if open.
    pub fn show(&mut self, ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons, event: Option<&PendingEventReport>, actions: &mut Vec<HudAction>) {
        let Some(event) = event.filter(|_| self.open) else { return };
        let screen = ctx.content_rect();
        let decided = queued_choice(input, event);
        let mut close = false;
        let shown = egui::Window::new("Ereignis")
            .id(egui::Id::unique("hud_event_dialog"))
            .title_bar(false)
            .frame(theme::parchment_frame().inner_margin(egui::Margin::same(14)))
            .resizable(false)
            .collapsible(false)
            .pivot(Align2::CENTER_TOP)
            .default_pos(Pos2::new(screen.center().x, screen.top() + TOP_BAR_HEIGHT + 40.0))
            .constrain_to(screen.shrink(8.0))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                close = header(ui, input, icons, event);
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(&event.text).size(16.0).color(Palette::INK)).wrap());
                if let Some(tile) = event.position {
                    ui.add_space(2.0);
                    let building = event
                        .focus
                        .and_then(|b| input.session.state().building(b))
                        .and_then(|b| input.session.defs().buildings().get(b.kind().index()))
                        .map(|d| d.name.as_str());
                    let place = place_link_text(&event.text, building);
                    let link = ui.add(egui::Label::new(RichText::new(place).color(Palette::INFO).underline()).sense(Sense::click()));
                    if link.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        actions.push(HudAction::Focus(tile));
                        actions.push(HudAction::Sound("ui_click"));
                    }
                }
                ui.add_space(6.0);
                countdown(ui, event);
                ui.add_space(6.0);
                // The choices scroll if they do not fit above the screen's bottom edge.
                let room = (screen.bottom() - ui.cursor().top() - 80.0).max(150.0);
                egui::ScrollArea::vertical().id_salt("event_choices").max_height(room).auto_shrink([false, true]).show(ui, |ui| {
                    for choice in &event.choices {
                        if choice_card(ui, choice, decided) {
                            actions.push(HudAction::Command(Command::ResolveEvent { serial: event.serial, choice: choice.index }));
                            actions.push(HudAction::Sound("ui_click"));
                            close = true;
                        }
                        ui.add_space(4.0);
                    }
                });
                if let Some(choice) = decided {
                    let label = event.choices.iter().find(|c| c.index == choice).map_or("", |c| c.label.as_str());
                    ui.colored_label(Palette::WARN, format!("Entschieden: „{label}“ – wird übernommen, sobald das Spiel weiterläuft."));
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Später entscheiden").on_hover_text(format!("Der Knopf „Ereignisse“ rechts ({}) holt die Entscheidung zurück.", input.hint(Action::ToggleEvents))).clicked() {
                        close = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Only the clock time here: the bar above shows the time left.
                        let now = input.session.state().clock().tick;
                        ui.label(RichText::new(deadline_clock_text(now, event.expires_tick)).small().color(Palette::INK_SOFT));
                    });
                });
            });
        // Kept above the docked panels: one opened (or clicked) later would hide the choices.
        if let Some(shown) = shown
            && (self.raise_frames > 0 || super::docked_area_on_top(ctx))
        {
            self.raise_frames = self.raise_frames.saturating_sub(1);
            ctx.move_to_top(shown.response.layer_id);
        }
        if close {
            self.open = false;
            actions.push(HudAction::Sound("ui_close"));
        }
    }
}

/// Title line; returns true if the close button was clicked.
fn header(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, event: &PendingEventReport) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        if let Some(icon) = icons.get("ui_events") {
            icon.show(ui, 34.0);
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let kind = match event.severity {
                Severity::Danger => "Gefahr",
                Severity::Warning => "Ereignis",
                Severity::Info => "Nachricht",
                Severity::Good => "Glücksfall",
            };
            ui.label(RichText::new(kind).small().strong().color(severity_color(event.severity)));
            ui.label(RichText::new(&event.title).font(heading_font(24.0)).color(Palette::INK));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if icon_button(ui, icons.get("ui_close"), "×", false, 24.0)
                .on_hover_text(format!("Später entscheiden ({})", input.hint(Action::Cancel)))
                .clicked()
            {
                close = true;
            }
        });
    });
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 2.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, severity_color(event.severity).gamma_multiply(0.8));
    close
}

/// Bar of the time left with the countdown and what happens then.
fn countdown(ui: &mut egui::Ui, event: &PendingEventReport) {
    let fraction = time_left_fraction(event);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), Palette::PARCHMENT_SHADE.gamma_multiply(0.6));
    let color = if fraction < 0.25 { Palette::BAD } else { Palette::GOLD_DARK };
    if fraction > 0.0 {
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width() * fraction, rect.height())), CornerRadius::same(4), color);
    }
    painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, Palette::WOOD), egui::StrokeKind::Inside);
    let default = event.choices.iter().find(|c| c.is_default).map_or("", |c| c.label.as_str());
    let text = format!("Entscheidung: {} – danach gilt „{default}“", countdown_text(event.minutes_left));
    painter.text(rect.center(), Align2::CENTER_CENTER, text, egui::FontId::proportional(13.0), Palette::INK);
}

/// One choice as a clickable card; returns true when chosen.
fn choice_card(ui: &mut egui::Ui, choice: &EventChoiceRow, decided: Option<u8>) -> bool {
    let enabled = choice.available && decided.is_none();
    let chosen = decided == Some(choice.index);
    let fill = if chosen {
        Palette::GOLD_BRIGHT.gamma_multiply(0.5)
    } else if choice.available {
        Palette::PARCHMENT_LIGHT
    } else {
        Palette::PARCHMENT_DARK.gamma_multiply(0.7)
    };
    let inner = egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, Palette::WOOD.gamma_multiply(0.8)))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin { left: 12, right: 10, top: 6, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let color = if choice.available { Palette::INK } else { Palette::INK_SOFT };
                ui.label(RichText::new(&choice.label).font(heading_font(16.0)).color(color));
                if choice.is_default {
                    ui.label(RichText::new("gilt ohne Entscheidung").small().italics().color(Palette::INK_SOFT));
                }
            });
            for line in &choice.effects {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    mark(ui, Mark::Next);
                    ui.add(egui::Label::new(RichText::new(line).color(Palette::INK)).wrap());
                });
            }
            if let Some(problem) = &choice.problem {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    mark(ui, Mark::Error);
                    ui.add(egui::Label::new(RichText::new(problem).color(Palette::BAD)).wrap());
                });
            }
        });
    let rect = inner.response.rect;
    // Accent stripe: gold for possible choices.
    let stripe = Rect::from_min_max(rect.min + Vec2::new(2.0, 3.0), Pos2::new(rect.min.x + 6.0, rect.max.y - 3.0));
    let accent: Color32 = if choice.available { Palette::GOLD_DARK } else { Palette::PARCHMENT_SHADE };
    ui.painter().rect_filled(stripe, CornerRadius::same(2), accent);
    let response = ui.interact(rect, ui.make_persistent_id(("event_choice", choice.index)), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && response.hovered() {
        ui.painter().rect_stroke(rect, CornerRadius::same(5), Stroke::new(2.0, Palette::GOLD), egui::StrokeKind::Inside);
    }
    let response = if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Klick: so entscheiden") } else { response };
    enabled && response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdowns_read_naturally() {
        assert_eq!(countdown_text(0), "die Frist läuft ab");
        assert_eq!(countdown_text(45), "noch 45 min");
        assert_eq!(countdown_text(60), "noch 1 h");
        assert_eq!(countdown_text(7 * 60 + 30), "noch 7 h 30 min");
        assert_eq!(countdown_text(12 * 60), "noch 12 h");
        // The footer names only the clock time (the game starts at 6:00), so the dialog shows
        // one time left.
        let hour = kingdoms_sim::time::TICKS_PER_HOUR;
        let text = deadline_clock_text(0, 12 * hour);
        assert_eq!(text, "Frist bis 18:00 Uhr");
        assert!(!text.contains("noch"));
        // A deadline after midnight names the day.
        assert_eq!(deadline_clock_text(12 * hour, 24 * hour), "Frist bis morgen 06:00 Uhr");
        // Later deadlines carry the calendar date of the top bar, never a running day count.
        assert_eq!(deadline_clock_text(0, 24 * 7 * hour), "Frist bis Sommer · Tag 2 · Jahr 1, 06:00 Uhr");
    }

    #[test]
    fn the_place_link_does_not_repeat_the_building() {
        let fire = "Im Gebäude „Holzfällerhütte“ ist Feuer ausgebrochen! Löscht es schnell.";
        assert_eq!(place_link_text(fire, Some("Holzfällerhütte")), "Gebäude zeigen");
        assert_eq!(place_link_text("Ein Wolf reißt Schafe.", Some("Schafzucht")), "Betrifft: Schafzucht – zeigen");
        assert_eq!(place_link_text("Ein Händler kommt.", None), "Ort des Ereignisses – zeigen");
    }

    #[test]
    fn a_new_event_opens_the_dialog_once() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match kingdoms_sim::defs::Defs::load(&dir) {
            Ok(d) => std::sync::Arc::new(d),
            Err(e) => panic!("shipped game data must load: {e}"),
        };
        let settings = kingdoms_sim::state::GameSettings::new(&defs, 7, kingdoms_sim::world::WorldSize::Small);
        let mut session = crate::session::Session::new(defs.clone(), settings);
        let Some(event) = defs.event_id("merchant_visit") else { panic!("merchant_visit exists") };
        session.queue_command(Command::DebugTriggerEvent { event });
        session.tick();
        let pending = kingdoms_sim::report::pending_event(session.state(), session.defs());
        let Some(pending) = pending else { panic!("the event is pending") };
        assert!((time_left_fraction(&pending) - 1.0).abs() < 0.01, "the whole decision time is left");
        let mut dialog = EventDialog::default();
        dialog.update(Some(&pending));
        assert!(dialog.is_open());
        // Hidden by the player, it stays hidden for the same event …
        dialog.set_open(false);
        dialog.update(Some(&pending));
        assert!(!dialog.is_open());
        // … and closes when nothing is pending any more.
        dialog.set_open(true);
        dialog.update(None);
        assert!(!dialog.is_open());
        // Twelve hours later the default choice applied.
        for _ in 0..(12 * kingdoms_sim::time::TICKS_PER_HOUR) {
            session.tick();
        }
        assert!(kingdoms_sim::report::pending_event(session.state(), session.defs()).is_none());
    }
}
