//! „Aufgaben“ (right panel) and the rank-up celebration (docs/game_design.md §1): the ruler's
//! rank with prestige and wealth, the requirements of the next rank with their progress (the
//! weakest one decides), the ranks reached with their dates and, once lost, the end of the game –
//! all from `report::rank_report`. A new rank (`SimEvent::RankAchieved`) opens a celebration
//! window; the world scene plays the fanfare and the advisor's voice for the same event.

use kingdoms_engine::egui::{self, Align2, Pos2, RichText, Sense, Ui, Vec2};
use kingdoms_sim::defs::Defs;
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::ranks::RequirementKind;
use kingdoms_sim::report::RankReport;

use super::format::{int, signed};
use super::management::Tab;
use super::{HudAction, HudInput, Icons, TOP_BAR_HEIGHT, ToolRequest};
use crate::session::{EventCursor, Session};
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Mark, mark, progress_bar, section_header};

/// Icon of the ruler's rank.
pub const RANK_ICON: &str = "ui_rank";

/// Where a click on a requirement leads, with its hover hint: the tab that explains it (wealth:
/// treasury and stock value in the economy tab), the territory border (shown if hidden), or the
/// build tool of a required building that is still missing.
pub fn requirement_link(kind: RequirementKind, done: bool, territory_shown: bool) -> Option<(HudAction, &'static str)> {
    match kind {
        RequirementKind::Population => Some((HudAction::OpenWindow(Tab::Population), "Klick: Bevölkerung in der Reichsverwaltung")),
        RequirementKind::Wealth => Some((HudAction::OpenWindow(Tab::Economy), "Klick: Schatz und Wert der Vorräte in der Reichsverwaltung")),
        RequirementKind::Buildings => Some((HudAction::OpenWindow(Tab::Production), "Klick: Gebäude in der Reichsverwaltung")),
        RequirementKind::Territory if territory_shown => None,
        RequirementKind::Territory => Some((HudAction::ToggleTerritory, "Klick: Gebietsgrenze auf der Karte zeigen")),
        RequirementKind::Building(_) if done => None,
        RequirementKind::Building(kind) => Some((HudAction::Tool(ToolRequest::Build(kind)), "Klick: bauen")),
    }
}

/// Draws the panel.
pub fn show(ui: &mut Ui, icons: &Icons, report: &RankReport, territory_shown: bool, actions: &mut Vec<HudAction>) {
    ui.horizontal(|ui| {
        if let Some(icon) = icons.get(RANK_ICON).or_else(|| icons.get("ui_tasks")) {
            icon.show(ui, 40.0);
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new("Herrscherrang").small().color(Palette::INK_SOFT));
            ui.label(RichText::new(&report.name).font(heading_font(24.0)).color(Palette::INK));
        });
    });
    ui.label(RichText::new(format!("Ansehen bei allen Reichen: {} · Wohlstand {} Gold", signed(i64::from(report.prestige)), int(i64::try_from(report.wealth).unwrap_or(i64::MAX)))).color(Palette::INK_SOFT));
    if let Some(over) = &report.game_over {
        section_header(ui, "Niederlage");
        ui.add(egui::Label::new(RichText::new(&over.text).color(Palette::BAD)).wrap());
        ui.label(RichText::new(&over.date_text).small().color(Palette::INK_SOFT));
    }
    match &report.next {
        Some(next) => {
            section_header(ui, &format!("Nächster Rang: {}", next.name));
            progress_bar(ui, f32::from(next.percent) / 100.0, Palette::GOLD_DARK, &format!("{} % erreicht", next.percent));
            ui.label(RichText::new(format!("Bringt {} Ansehen bei allen Königreichen. Die schwächste Anforderung zählt.", signed(i64::from(next.prestige)))).small().color(Palette::INK_SOFT));
            ui.add_space(4.0);
            // The rows are links: their labels must not take the click for text selection.
            ui.style_mut().interaction.selectable_labels = false;
            for q in &next.requirements {
                let response = ui
                    .vertical(|ui| {
                        ui.horizontal(|ui| {
                            mark(ui, if q.done { Mark::Ok } else { Mark::Next });
                            ui.label(RichText::new(&q.label).strong().color(Palette::INK));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let text = if q.required <= 1 && q.current <= 1 {
                                    if q.done { "vorhanden".to_owned() } else { "fehlt".to_owned() }
                                } else {
                                    format!("{} / {}", int(i64::try_from(q.current).unwrap_or(i64::MAX)), int(i64::try_from(q.required).unwrap_or(i64::MAX)))
                                };
                                ui.label(RichText::new(text).color(if q.done { Palette::GOOD } else { Palette::INK }));
                            });
                        });
                        let color = if q.done { Palette::GOOD } else { Palette::GOLD };
                        progress_bar(ui, f32::from(q.percent) / 100.0, color, &format!("{} %", q.percent));
                    })
                    .response
                    .interact(Sense::click());
                if let Some((action, hint)) = requirement_link(q.kind, q.done, territory_shown)
                    && response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked()
                {
                    actions.push(action);
                    actions.push(HudAction::Sound("ui_click"));
                }
                ui.add_space(2.0);
            }
        }
        None => {
            section_header(ui, "Höchster Rang");
            ui.label(RichText::new("Euer Reich hat den höchsten Rang erreicht.").color(Palette::GOOD));
        }
    }
    section_header(ui, "Erreichte Ränge");
    if report.reached.is_empty() {
        ui.label(RichText::new("Noch kein Aufstieg – das Reich beginnt als Siedlung.").italics().color(Palette::INK_SOFT));
    }
    for (name, date) in report.reached.iter().rev() {
        ui.horizontal(|ui| {
            mark(ui, Mark::Ok);
            ui.label(RichText::new(name).strong().color(Palette::INK));
            ui.label(RichText::new(date).small().color(Palette::INK_SOFT));
        });
    }
}

/// The celebration of a new rank.
#[derive(Clone, Debug, PartialEq)]
pub struct Celebration {
    pub rank: u8,
    pub name: String,
    pub prestige: i8,
    /// The rank after it and its first open requirement („nächstes Ziel“).
    pub next: Option<String>,
}

impl Celebration {
    /// The celebration of rank `rank` of the data.
    pub fn of(defs: &Defs, rank: u8) -> Option<Celebration> {
        let r = defs.ranks().get(usize::from(rank))?;
        let next = defs.ranks().get(usize::from(rank) + 1).map(|n| n.name.clone());
        Some(Celebration { rank, name: r.name.clone(), prestige: r.prestige, next })
    }
}

/// The window that celebrates a new rank (opened by `SimEvent::RankAchieved`).
pub struct RankDialog {
    cursor: EventCursor,
    open: Option<Celebration>,
    opened_at: f64,
}

impl RankDialog {
    pub fn new(session: &Session) -> Self {
        Self { cursor: session.events().cursor_at_end(), open: None, opened_at: 0.0 }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn close(&mut self) {
        self.open = None;
    }

    /// Reads new events; a new rank opens the window (the highest one if several).
    pub fn read_events(&mut self, session: &Session, now: f64) {
        let read = session.events().read(&mut self.cursor);
        let rank = read.events.filter_map(|e| if let SimEvent::RankAchieved { rank } = e.event { Some(rank) } else { None }).max();
        if let Some(c) = rank.and_then(|r| Celebration::of(session.defs(), r)) {
            self.open = Some(c);
            self.opened_at = now;
        }
    }

    /// Draws the window.
    pub fn show(&mut self, ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons, actions: &mut Vec<HudAction>) {
        let Some(c) = self.open.clone() else { return };
        let screen = ctx.content_rect();
        let mut close = false;
        let glow = ((input.now - self.opened_at) * 2.0).sin() as f32 * 0.5 + 0.5;
        egui::Window::new("Aufstieg")
            .id(egui::Id::unique("hud_rank_dialog"))
            .title_bar(false)
            .frame(theme::parchment_frame().stroke(egui::Stroke::new(3.0, Palette::GOLD_DARK.lerp_to_gamma(Palette::GOLD_BRIGHT, glow))).inner_margin(egui::Margin::same(20)))
            .resizable(false)
            .collapsible(false)
            .pivot(Align2::CENTER_TOP)
            .default_pos(Pos2::new(screen.center().x, screen.top() + TOP_BAR_HEIGHT + 70.0))
            .constrain_to(screen.shrink(8.0))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.vertical_centered(|ui| {
                    if let Some(icon) = icons.get(RANK_ICON).or_else(|| icons.get("ui_tasks")) {
                        icon.show(ui, 64.0);
                    }
                    ui.label(RichText::new("Euer Reich steigt auf!").font(heading_font(18.0)).color(Palette::INK_SOFT));
                    ui.label(RichText::new(&c.name).font(heading_font(34.0)).color(Palette::GOLD_DARK));
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Alle Königreiche achten Euch mehr: Ansehen {}.", signed(i64::from(c.prestige)))).color(Palette::INK));
                    if let Some(next) = &c.next {
                        ui.label(RichText::new(format!("Nächstes Ziel: {next} – die Anforderungen stehen unter „Aufgaben“.")).small().color(Palette::INK_SOFT));
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space(70.0);
                        if ui.button(RichText::new("Weiter so!").font(heading_font(16.0))).clicked() {
                            close = true;
                        }
                        if c.next.is_some() && ui.button("Aufgaben zeigen").clicked() {
                            actions.push(HudAction::OpenPanel(super::RightPanel::Tasks));
                            close = true;
                        }
                    });
                });
                ui.allocate_exact_size(Vec2::new(0.0, 2.0), Sense::hover());
            });
        ctx.request_repaint();
        if close {
            self.open = None;
            actions.push(HudAction::Sound("ui_close"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_link_to_their_tabs_and_ranks_celebrate() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = Defs::load(&dir).unwrap_or_else(|e| panic!("shipped game data must load: {e}"));
        let link = |kind, done, shown| requirement_link(kind, done, shown).map(|(action, _)| action);
        assert_eq!(link(RequirementKind::Population, false, false), Some(HudAction::OpenWindow(Tab::Population)));
        assert_eq!(link(RequirementKind::Wealth, false, false), Some(HudAction::OpenWindow(Tab::Economy)));
        assert_eq!(link(RequirementKind::Buildings, true, false), Some(HudAction::OpenWindow(Tab::Production)));
        assert_eq!(link(RequirementKind::Territory, false, false), Some(HudAction::ToggleTerritory));
        assert_eq!(link(RequirementKind::Territory, false, true), None, "a shown border is not hidden by a click");
        let Some(market) = defs.building_id("market") else { panic!("market exists") };
        assert_eq!(link(RequirementKind::Building(market), false, false), Some(HudAction::Tool(ToolRequest::Build(market))));
        assert_eq!(link(RequirementKind::Building(market), true, false), None);
        // Every requirement of every rank in the shipped data gets a link while it is open.
        let state = kingdoms_sim::state::GameState::new(&defs, kingdoms_sim::state::GameSettings::new(&defs, 7, kingdoms_sim::world::WorldSize::Small));
        for rank in defs.ranks() {
            for q in kingdoms_sim::ranks::requirements(&state, &defs, rank) {
                assert!(requirement_link(q.kind, false, false).is_some(), "{}", q.label);
            }
        }
        let Some(c) = Celebration::of(&defs, 1) else { panic!("rank 1 exists") };
        assert_eq!(c.name, defs.ranks()[1].name);
        assert_eq!(c.next.as_deref(), defs.ranks().get(2).map(|r| r.name.as_str()));
        let last = u8::try_from(defs.ranks().len() - 1).unwrap_or(0);
        assert!(Celebration::of(&defs, last).is_some_and(|c| c.next.is_none()));
        assert!(Celebration::of(&defs, 200).is_none());
    }
}
