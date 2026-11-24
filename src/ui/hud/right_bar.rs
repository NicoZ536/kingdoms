//! Right bar of the master design: Ereignisse (the pending decision, diplomatic proposals,
//! active event effects and the event history), Benachrichtigungen (all notifications of the
//! session), Aufgaben (the ruler's rank and the requirements of the next one) and Übersicht (the
//! kingdom at a glance). Each button opens its panel left of the bar (left of the info panel
//! while one is shown); only one panel is open at a time.

use kingdoms_engine::egui::{self, Align2, Color32, CornerRadius, Pos2, Rect, RichText, Stroke, Vec2};
use kingdoms_sim::event::Severity;
use kingdoms_sim::report::{self, EventsReport, PendingEventReport, ProposalRow, RankReport};

use super::build_menu::bar_button;
use super::event_dialog::{countdown_text, queued_choice};
use super::format::signed;
use super::feed::{Feed, severity_color};
use super::management::Cached;
use super::overview::{self, Overview};
use super::tasks;
use super::{HudAction, HudInput, Icons, MARGIN, TOP_BAR_HEIGHT};
use crate::actions::Action;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Mark, icon_button, mark, section_header};

/// Width of a panel.
pub const PANEL_WIDTH: f32 = 330.0;

/// What the buttons of the right bar show besides their state.
pub struct Badges<'a> {
    /// The event waiting for a decision.
    pub pending: Option<&'a PendingEventReport>,
    /// The decision dialog is shown.
    pub dialog_open: bool,
    /// Notifications since the panel was last open.
    pub unread: usize,
    /// Diplomatic proposals waiting for an answer.
    pub proposals: &'a [ProposalRow],
    /// Progress towards the next rank in percent (`None` at the highest rank).
    pub rank: Option<(String, u8)>,
}

/// Where a panel goes: its right edge and the lowest point it may reach.
#[derive(Clone, Copy, Debug)]
pub struct PanelPlace {
    pub right: f32,
    pub bottom: f32,
}

/// A panel of the right bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightPanel {
    Events,
    Notifications,
    Tasks,
    Overview,
}

impl RightPanel {
    pub const ALL: [RightPanel; 4] = [RightPanel::Events, RightPanel::Notifications, RightPanel::Tasks, RightPanel::Overview];

    const fn label(self) -> &'static str {
        match self {
            RightPanel::Events => "Ereignisse",
            RightPanel::Notifications => "Meldungen",
            RightPanel::Tasks => "Aufgaben",
            RightPanel::Overview => "Übersicht",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            RightPanel::Events => "Ereignisse",
            RightPanel::Notifications => "Benachrichtigungen",
            RightPanel::Tasks => "Aufgaben",
            RightPanel::Overview => "Übersicht",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            RightPanel::Events => "ui_events",
            RightPanel::Notifications => "ui_notifications",
            RightPanel::Tasks => tasks::RANK_ICON,
            RightPanel::Overview => "ui_overview",
        }
    }

    /// Name used by the developer option `KINGDOMS_PANEL`.
    pub const fn key(self) -> &'static str {
        match self {
            RightPanel::Events => "ereignisse",
            RightPanel::Notifications => "meldungen",
            RightPanel::Tasks => "aufgaben",
            RightPanel::Overview => "uebersicht",
        }
    }
}

/// The right bar and its panels.
#[derive(Default)]
pub struct RightBar {
    open: Option<RightPanel>,
    events: Cached<EventsReport>,
    overview: Cached<Overview>,
    ranks: Cached<RankReport>,
    /// Notifications panel: only warnings and dangers.
    important_only: bool,
}

impl RightBar {
    pub fn open_panel(&self) -> Option<RightPanel> {
        self.open
    }

    /// Opens `panel`, or closes it if open; returns true if it is open afterwards.
    pub fn toggle(&mut self, panel: RightPanel) -> bool {
        self.open = if self.open == Some(panel) { None } else { Some(panel) };
        self.open.is_some()
    }

    /// Closes the open panel; returns true if one was open.
    pub fn close(&mut self) -> bool {
        self.open.take().is_some()
    }

    /// Forgets the session's data (after loading a game).
    pub fn reset(&mut self) {
        self.events = Cached::default();
        self.overview = Cached::default();
        self.ranks = Cached::default();
    }

    /// Draws the bar at the right edge; returns its rectangle.
    pub fn show_bar(&mut self, ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons, badges: &Badges<'_>, actions: &mut Vec<HudAction>) -> Rect {
        let Badges { pending, dialog_open, unread, proposals, ref rank } = *badges;
        egui::Area::new(egui::Id::unique("hud_right_bar"))
            .anchor(Align2::RIGHT_TOP, Vec2::new(-MARGIN, TOP_BAR_HEIGHT + MARGIN))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::wood_frame().inner_margin(egui::Margin::same(5)).show(ui, |ui| {
                    theme::on_wood(ui);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for panel in RightPanel::ALL {
                        let selected = self.open == Some(panel) || (panel == RightPanel::Events && dialog_open);
                        let response = bar_button(ui, icons, panel.icon(), panel.label(), selected);
                        let response = match panel {
                            RightPanel::Events => {
                                if let Some(p) = pending {
                                    // A pending decision pulses until the player looks at it.
                                    let pulse = if dialog_open { 1.0 } else { 0.55 + 0.45 * ((input.now * 3.0).sin() as f32 * 0.5 + 0.5) };
                                    badge(ui, response.rect, "!", Palette::BAD.gamma_multiply(pulse));
                                    ui.ctx().request_repaint();
                                    response.on_hover_text(format!(
                                        "„{}“ wartet auf Eure Entscheidung ({}).\nKlick: Entscheidung zeigen ({})",
                                        p.title,
                                        countdown_text(p.minutes_left),
                                        input.hint(Action::ToggleEvents)
                                    ))
                                } else if !proposals.is_empty() {
                                    badge(ui, response.rect, &proposals.len().to_string(), Palette::INFO);
                                    let text = if proposals.len() == 1 {
                                        "1 diplomatische Anfrage wartet auf Antwort.".to_owned()
                                    } else {
                                        format!("{} diplomatische Anfragen warten auf Antwort.", proposals.len())
                                    };
                                    response.on_hover_text(format!("{text}\nEreignisse: Anfragen, Wirkungen und Verlauf ({})", input.hint(Action::ToggleEvents)))
                                } else {
                                    response.on_hover_text(format!("Ereignisse: Wirkungen und Verlauf ({})", input.hint(Action::ToggleEvents)))
                                }
                            }
                            RightPanel::Notifications => {
                                if unread > 0 {
                                    let text = if unread > 99 { "99+".to_owned() } else { unread.to_string() };
                                    badge(ui, response.rect, &text, Palette::GOLD_DARK);
                                }
                                response.on_hover_text("Benachrichtigungen: alle Meldungen dieser Sitzung")
                            }
                            RightPanel::Tasks => match rank {
                                Some((next, percent)) => {
                                    progress_ring(ui, response.rect, *percent);
                                    response.on_hover_text(format!("Aufgaben: nächster Rang „{next}“ zu {percent} % erreicht"))
                                }
                                None => response.on_hover_text("Aufgaben: Herrscherrang"),
                            },
                            RightPanel::Overview => response.on_hover_text("Übersicht: das Reich auf einen Blick"),
                        };
                        if response.clicked() {
                            if panel == RightPanel::Events && pending.is_some() {
                                actions.push(HudAction::EventDialog(!dialog_open));
                                actions.push(HudAction::Sound(if dialog_open { "ui_close" } else { "ui_open" }));
                            } else {
                                let open = self.toggle(panel);
                                actions.push(HudAction::Sound(if open { "ui_open" } else { "ui_close" }));
                            }
                        }
                    }
                });
            })
            .response
            .rect
    }

    /// Draws the open panel at `place`; returns its rectangle.
    pub fn show_panel(
        &mut self,
        ctx: &egui::Context,
        input: &HudInput<'_>,
        icons: &Icons,
        feed: &mut Feed,
        place: PanelPlace,
        actions: &mut Vec<HudAction>,
    ) -> Option<Rect> {
        let panel = self.open?;
        let PanelPlace { right, bottom } = place;
        let screen = ctx.content_rect();
        let top = TOP_BAR_HEIGHT + MARGIN;
        let max_height = (bottom - top - 28.0).max(200.0);
        let mut close = false;
        let rect = egui::Area::new(egui::Id::unique("hud_right_panel"))
            .anchor(Align2::RIGHT_TOP, Vec2::new(right - screen.right(), top))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::parchment_frame().show(ui, |ui| {
                    ui.set_width(PANEL_WIDTH);
                    // An area starts with last frame's size as its bounds: allow the full height.
                    ui.set_max_height(max_height);
                    ui.horizontal(|ui| {
                        if let Some(icon) = icons.get(panel.icon()) {
                            icon.show(ui, 26.0);
                        }
                        ui.label(RichText::new(panel.title()).font(heading_font(20.0)).color(Palette::INK));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, icons.get("ui_close"), "×", false, 24.0)
                                .on_hover_text(format!("Schließen ({})", input.hint(Action::Cancel)))
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
                    egui::ScrollArea::vertical().id_salt(("right_panel", panel.key())).max_height(max_height - 40.0).show(ui, |ui| {
                        ui.set_width(PANEL_WIDTH - 12.0);
                        match panel {
                            RightPanel::Events => {
                                let state = input.session.state();
                                let defs = input.session.defs();
                                let report = self.events.get(input, || report::events_report(state, defs));
                                events_panel(ui, input, report, proposals_of(input), actions);
                            }
                            RightPanel::Notifications => {
                                ui.checkbox(&mut self.important_only, "Nur Warnungen und Gefahren");
                                feed.show_history(ui, self.important_only, actions);
                            }
                            RightPanel::Tasks => {
                                let state = input.session.state();
                                let defs = input.session.defs();
                                let data = self.ranks.get(input, || report::rank_report(state, defs));
                                tasks::show(ui, icons, data, input.show_territory, actions);
                            }
                            RightPanel::Overview => {
                                let state = input.session.state();
                                let defs = input.session.defs();
                                let data = self.overview.get(input, || Overview::compute(state, defs));
                                overview::show(ui, icons, data, actions);
                            }
                        }
                    });
                });
            })
            .response
            .rect;
        if panel == RightPanel::Notifications {
            feed.mark_read();
        }
        if close {
            self.open = None;
            actions.push(HudAction::Sound("ui_close"));
        }
        Some(rect)
    }
}

/// A small round counter at the top right of a bar button.
fn badge(ui: &egui::Ui, button: Rect, text: &str, color: Color32) {
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.0), Color32::WHITE);
    let width = (galley.size().x + 8.0).max(16.0);
    let rect = Rect::from_center_size(Pos2::new(button.right() - width / 2.0 + 2.0, button.top() + 6.0), Vec2::new(width, 16.0));
    painter.rect_filled(rect, CornerRadius::same(8), color);
    painter.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, Palette::PARCHMENT_LIGHT), egui::StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

/// A thin progress arc along the bottom of a bar button.
fn progress_ring(ui: &egui::Ui, button: Rect, percent: u8) {
    let painter = ui.painter();
    let bar = Rect::from_min_max(Pos2::new(button.left() + 6.0, button.bottom() - 4.0), Pos2::new(button.right() - 6.0, button.bottom() - 2.0));
    painter.rect_filled(bar, CornerRadius::same(1), Palette::WOOD_DARK);
    let filled = Rect::from_min_max(bar.min, Pos2::new(bar.left() + bar.width() * f32::from(percent.min(100)) / 100.0, bar.bottom()));
    painter.rect_filled(filled, CornerRadius::same(1), Palette::GOLD_BRIGHT);
}

/// Proposals waiting for an answer (cheap: few at a time).
fn proposals_of(input: &HudInput<'_>) -> Vec<ProposalRow> {
    report::pending_proposals(input.session.state(), input.session.defs())
}

/// Pending decision, diplomatic proposals, effects in force and the history of decided events.
fn events_panel(ui: &mut egui::Ui, input: &HudInput<'_>, report: &EventsReport, proposals: Vec<ProposalRow>, actions: &mut Vec<HudAction>) {
    if !report.enabled {
        ui.label(RichText::new("Zufallsereignisse sind in diesem Spiel abgeschaltet.").italics().color(Palette::INK_SOFT));
    }
    section_header(ui, "Offene Entscheidung");
    match &report.pending {
        Some(p) => {
            ui.label(RichText::new(&p.title).font(heading_font(16.0)).color(severity_color(p.severity)));
            ui.label(RichText::new(countdown_text(p.minutes_left)).small().color(Palette::INK_SOFT));
            match queued_choice(input, p) {
                Some(choice) => {
                    let label = p.choices.iter().find(|c| c.index == choice).map_or("", |c| c.label.as_str());
                    ui.colored_label(Palette::WARN, format!("Entschieden: „{label}“ – wird übernommen, sobald das Spiel weiterläuft."));
                }
                None => {
                    if ui.button("Entscheiden …").clicked() {
                        actions.push(HudAction::EventDialog(true));
                        actions.push(HudAction::Sound("ui_open"));
                    }
                }
            }
        }
        None => {
            ui.label(RichText::new("Keine Entscheidung offen.").italics().color(Palette::INK_SOFT));
        }
    }
    if !proposals.is_empty() {
        section_header(ui, "Diplomatische Anfragen");
        for p in &proposals {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 90.0);
                    ui.label(RichText::new(format!("{}: {}", p.kingdom_name, p.title)).strong().color(Palette::INK));
                    ui.label(RichText::new(&p.remaining_text).small().color(Palette::INK_SOFT));
                });
                if ui.button("Antworten …").clicked() {
                    actions.push(HudAction::ShowProposal(p.serial));
                    actions.push(HudAction::Sound("ui_open"));
                }
            });
        }
    }
    section_header(ui, "Wirkungen");
    if report.modifiers.is_empty() && report.tax_cap.is_none() {
        ui.label(RichText::new("Keine Ereignisse wirken gerade.").italics().color(Palette::INK_SOFT));
    }
    for m in &report.modifiers {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            mark(ui, if m.amount >= 0 { Mark::Ok } else { Mark::Warning });
            ui.add(egui::Label::new(RichText::new(&m.text).color(Palette::INK)).wrap());
        });
    }
    if let Some((max, until)) = &report.tax_cap {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            mark(ui, Mark::Warning);
            ui.add(egui::Label::new(RichText::new(format!("Steuersatz höchstens {max} % bis {until}")).color(Palette::INK)).wrap());
        });
    }
    if !report.modifiers.is_empty() {
        let mut sums = Vec::new();
        if report.happiness_points != 0 {
            sums.push(format!("Zufriedenheit {}", signed(i64::from(report.happiness_points))));
        }
        if report.production_percent != 0 {
            sums.push(format!("Produktion {} %", signed(i64::from(report.production_percent))));
        }
        if report.field_yield_percent != 0 {
            sums.push(format!("Feldertrag {} %", signed(i64::from(report.field_yield_percent))));
        }
        if !sums.is_empty() {
            ui.label(RichText::new(format!("Zusammen: {}", sums.join(" · "))).small().color(Palette::INK_SOFT));
        }
    }
    section_header(ui, "Verlauf");
    if report.history.is_empty() {
        ui.label(RichText::new("Noch keine Ereignisse.").italics().color(Palette::INK_SOFT));
    }
    for (i, h) in report.history.iter().enumerate() {
        let inner = egui::Frame::new()
            .fill(if i % 2 == 0 { Palette::PARCHMENT_LIGHT } else { Palette::PARCHMENT })
            .corner_radius(CornerRadius::same(4))
            .inner_margin(egui::Margin { left: 10, right: 6, top: 4, bottom: 4 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&h.title).font(heading_font(14.0)).color(Palette::INK));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(&h.date).small().color(Palette::INK_SOFT));
                    });
                });
                let how = if h.by_default { "Ohne Entscheidung" } else { "Entschieden" };
                ui.label(RichText::new(format!("{how}: {}", h.choice)).small().strong().color(Palette::INK_SOFT));
                for line in &h.outcome {
                    ui.add(egui::Label::new(RichText::new(line).small().color(Palette::INK)).wrap());
                }
            });
        let rect = inner.response.rect;
        let stripe = Rect::from_min_max(rect.min + Vec2::new(1.0, 3.0), Pos2::new(rect.min.x + 5.0, rect.max.y - 3.0));
        let color = if h.severity == Severity::Info { Palette::INFO } else { severity_color(h.severity) };
        ui.painter().rect_filled(stripe, CornerRadius::same(2), color);
    }
}
