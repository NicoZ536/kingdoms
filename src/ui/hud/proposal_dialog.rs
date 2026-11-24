//! Decision card of the diplomatic proposals (docs/game_design.md §10): when a kingdom proposes
//! a treaty, demands tribute, offers peace or a deal, a compact parchment card docked at the
//! right edge (left of the right bar and its panels, above the notification cards) shows its
//! crest, the proposal, a countdown to the deadline (after which it counts as declined) and one
//! card per answer with its consequence or why accepting is impossible. An answer queues
//! `Command::AnswerProposal` and the card moves on to the next unanswered proposal. Several
//! proposals share the one card: „1 von 3“ with buttons for the previous and next one. The card
//! opens by itself for every new proposal (not while the event dialog or the rank celebration
//! is shown: they keep priority); „Später“ hides it until the next new proposal – the
//! Diplomatie tab and the events panel keep every proposal answerable until its deadline.

use std::collections::BTreeSet;

use kingdoms_engine::egui::{self, Align2, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use kingdoms_sim::command::Command;
use kingdoms_sim::report::ProposalRow;
use kingdoms_sim::time::SECONDS_PER_TICK;

use super::event_dialog::countdown_text;
use super::heraldry::crest;
use super::management::diplomacy::queued_answer;
use super::{HudAction, HudInput, Icons};
use crate::actions::Action;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Glyph, Mark, glyph_button, icon_button, mark};

/// Width of the card: the width of the notification cards it stacks with.
pub const CARD_WIDTH: f32 = super::feed::FEED_WIDTH;
/// Frames the card is kept on top after it opened (a window opened before must not hide it).
const RAISE_FRAMES: u8 = 3;

/// Share of the answer time left for a proposal created at `created` that expires at `expires`
/// (ticks), at tick `now`.
pub fn time_left_fraction(created: u64, expires: u64, now: u64) -> f32 {
    let total = expires.saturating_sub(created);
    if total == 0 { 0.0 } else { (expires.saturating_sub(now) as f32 / total as f32).clamp(0.0, 1.0) }
}

/// Game minutes until `expires` at tick `now`.
pub fn minutes_left(expires: u64, now: u64) -> u64 {
    expires.saturating_sub(now) * SECONDS_PER_TICK / 60
}

/// Position of the queue: „2 von 3“.
pub fn queue_text(index: usize, len: usize) -> String {
    format!("{} von {len}", index + 1)
}

/// The proposal to show after the one at `index` was answered: the next one in the queue
/// (wrapping around) for which `open` holds, `None` when every other one is answered.
pub fn next_open(proposals: &[ProposalRow], index: usize, open: impl Fn(u32) -> bool) -> Option<u32> {
    let len = proposals.len();
    (1..len).map(|step| &proposals[(index + step) % len]).find(|p| open(p.serial)).map(|p| p.serial)
}

/// State of the card.
#[derive(Default)]
pub struct ProposalDialog {
    /// The proposal shown (serial); `None` while the card is hidden.
    open: Option<u32>,
    /// Proposals the card has presented (it opens by itself only for new ones).
    seen: BTreeSet<u32>,
    raise_frames: u8,
}

impl ProposalDialog {
    /// Keeps the card in step with the pending proposals: a new one opens the card (unless
    /// `blocked`: an event decision or the rank celebration is shown), a shown one that expired
    /// gives way to its neighbour in the queue.
    pub fn update(&mut self, proposals: &[ProposalRow], blocked: bool) {
        if let Some(serial) = self.open
            && !proposals.iter().any(|p| p.serial == serial)
        {
            // Expired (or answered elsewhere): the next younger proposal, else the last one.
            self.open = proposals.iter().find(|p| p.serial > serial).or(proposals.last()).map(|p| p.serial);
        }
        self.seen.retain(|s| proposals.iter().any(|p| p.serial == *s));
        if blocked {
            return;
        }
        if self.open.is_none()
            && let Some(p) = proposals.iter().find(|p| !self.seen.contains(&p.serial))
        {
            self.show_proposal(p.serial);
        }
        // Everything in the queue of an open card counts as presented.
        if self.open.is_some() {
            self.seen.extend(proposals.iter().map(|p| p.serial));
        }
    }

    /// Shows proposal `serial` (from the events panel, the overview or the queue buttons).
    pub fn show_proposal(&mut self, serial: u32) {
        self.seen.insert(serial);
        if self.open.is_none() {
            self.raise_frames = RAISE_FRAMES;
        }
        self.open = Some(serial);
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Hides the card („Später“, Esc) until the next new proposal.
    pub fn close(&mut self) {
        self.open = None;
    }

    /// Draws the card of the open proposal with its right edge at `right` and its top at `top`;
    /// returns the rectangle it covers.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        input: &HudInput<'_>,
        icons: &Icons,
        proposals: &[ProposalRow],
        right: f32,
        top: f32,
        actions: &mut Vec<HudAction>,
    ) -> Option<Rect> {
        let serial = self.open?;
        let index = proposals.iter().position(|p| p.serial == serial)?;
        let p = &proposals[index];
        let state = input.session.state();
        let now = state.clock().tick;
        let (color, created) = (
            state.diplomacy().kingdom(p.kingdom).map_or(0, |k| k.color()),
            state.diplomacy().proposals().iter().find(|x| x.serial == serial).map_or(now, |x| x.created_tick),
        );
        let queued = queued_answer(input.session, serial);
        // „Später“ (or ×) postpones the proposals; „Zum Reich“ only moves them to the tab.
        let mut postpone = false;
        let mut close = false;
        let mut goto = None;
        let mut answered = false;
        let shown = egui::Area::new(egui::Id::unique("hud_proposal_card"))
            .fixed_pos(Pos2::new(right - CARD_WIDTH, top))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(CARD_WIDTH);
                theme::parchment_frame().inner_margin(egui::Margin { left: 12, right: 8, top: 8, bottom: 8 }).show(ui, |ui| {
                    ui.set_width(CARD_WIDTH - 20.0);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.horizontal(|ui| {
                        crest(ui, icons, 34.0, color);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(RichText::new(format!("Gesandtschaft aus {}", p.kingdom_name)).small().strong().color(Palette::INFO));
                            ui.add(egui::Label::new(RichText::new(&p.title).font(heading_font(18.0)).color(Palette::INK)).wrap());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            let hint = input.hint(Action::Cancel);
                            if icon_button(ui, icons.get("ui_close"), "×", false, 20.0).on_hover_text(format!("Später entscheiden ({hint})")).clicked() {
                                postpone = true;
                            }
                        });
                    });
                    let (rule, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 2.0), Sense::hover());
                    ui.painter().rect_filled(rule, CornerRadius::ZERO, Palette::INFO.gamma_multiply(0.8));
                    ui.add(egui::Label::new(RichText::new(&p.text).size(15.0).color(Palette::INK)).wrap());
                    countdown(ui, time_left_fraction(created, p.expires_tick, now), minutes_left(p.expires_tick, now), &p.decline_label);
                    if answer_card(ui, &p.accept_label, p.accept_problem.as_deref(), None, queued == Some(true), queued.is_none(), ("accept", serial)) {
                        actions.push(HudAction::Command(Command::AnswerProposal { serial, accept: true }));
                        actions.push(HudAction::Sound("ui_click"));
                        answered = true;
                    }
                    if answer_card(ui, &p.decline_label, None, Some(&p.decline_text), queued == Some(false), queued.is_none(), ("decline", serial)) {
                        actions.push(HudAction::Command(Command::AnswerProposal { serial, accept: false }));
                        actions.push(HudAction::Sound("ui_click"));
                        answered = true;
                    }
                    if let Some(accept) = queued {
                        let label = if accept { &p.accept_label } else { &p.decline_label };
                        ui.add(egui::Label::new(RichText::new(format!("„{label}“ gilt, sobald das Spiel weiterläuft.")).small().color(Palette::WARN)).wrap());
                    }
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if proposals.len() > 1 {
                            if glyph_button(ui, Glyph::Left, index > 0).on_hover_text("Vorige Anfrage").clicked() {
                                goto = index.checked_sub(1);
                            }
                            ui.label(RichText::new(queue_text(index, proposals.len())).small().color(Palette::INK));
                            if glyph_button(ui, Glyph::Right, index + 1 < proposals.len()).on_hover_text("Nächste Anfrage").clicked() {
                                goto = Some(index + 1);
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Zum Reich").on_hover_text("Den Reiter „Diplomatie“ mit diesem Reich öffnen").clicked() {
                                actions.push(HudAction::OpenKingdom(p.kingdom));
                                close = true;
                            }
                            if ui
                                .button("Später")
                                .on_hover_text("Die Karte ausblenden. Jede Anfrage bleibt bis zu ihrer Frist im Reiter „Diplomatie“ und unter „Ereignisse“ beantwortbar.")
                                .clicked()
                            {
                                postpone = true;
                            }
                        });
                    });
                });
            });
        if self.raise_frames > 0 {
            self.raise_frames -= 1;
            ctx.move_to_top(shown.response.layer_id);
        }
        if let Some(i) = goto
            && let Some(next) = proposals.get(i)
        {
            self.open = Some(next.serial);
            actions.push(HudAction::Sound("ui_click"));
        }
        if answered {
            // On to the next proposal that still waits for an answer.
            self.open = next_open(proposals, index, |s| queued_answer(input.session, s).is_none());
        }
        if postpone {
            actions.push(HudAction::ProposalPostponed);
        }
        if close || postpone {
            self.open = None;
            actions.push(HudAction::Sound("ui_close"));
        }
        Some(shown.response.rect)
    }
}

/// Bar of the time left; names the answer that counts after the deadline when it fits.
fn countdown(ui: &mut egui::Ui, fraction: f32, minutes: u64, decline: &str) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), Palette::PARCHMENT_SHADE.gamma_multiply(0.6));
    let color = if fraction < 0.25 { Palette::BAD } else { Palette::INFO };
    if fraction > 0.0 {
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width() * fraction, rect.height())), CornerRadius::same(4), color);
    }
    painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, Palette::WOOD), egui::StrokeKind::Inside);
    let font = egui::FontId::proportional(13.0);
    let left = countdown_text(minutes);
    let full = format!("{left} – danach gilt „{decline}“");
    let fits = painter.layout_no_wrap(full.clone(), font.clone(), Palette::INK).size().x <= rect.width() - 8.0;
    painter.text(rect.center(), Align2::CENTER_CENTER, if fits { full } else { left }, font, Palette::INK);
    response.on_hover_text(format!("Ohne Antwort gilt nach Fristende „{decline}“."));
}

/// One answer as a clickable card; returns true when chosen.
fn answer_card(
    ui: &mut egui::Ui,
    label: &str,
    problem: Option<&str>,
    consequence: Option<&str>,
    chosen: bool,
    open: bool,
    id: (&str, u32),
) -> bool {
    let available = problem.is_none();
    let enabled = available && open;
    let fill = if chosen {
        Palette::GOLD_BRIGHT.gamma_multiply(0.5)
    } else if available {
        Palette::PARCHMENT_LIGHT
    } else {
        Palette::PARCHMENT_DARK.gamma_multiply(0.7)
    };
    let inner = egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, Palette::WOOD.gamma_multiply(0.8)))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin { left: 12, right: 8, top: 4, bottom: 4 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new(label).font(heading_font(15.0)).color(if available { Palette::INK } else { Palette::INK_SOFT }));
            if let Some(text) = consequence {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    mark(ui, Mark::Next);
                    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(Palette::INK)).wrap());
                });
            }
            if let Some(problem) = problem {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    mark(ui, Mark::Error);
                    ui.add(egui::Label::new(RichText::new(problem).size(13.0).color(Palette::BAD)).wrap());
                });
            }
        });
    let rect = inner.response.rect;
    let stripe = Rect::from_min_max(rect.min + Vec2::new(2.0, 3.0), Pos2::new(rect.min.x + 6.0, rect.max.y - 3.0));
    ui.painter().rect_filled(stripe, CornerRadius::same(2), if available { Palette::INFO } else { Palette::PARCHMENT_SHADE });
    let response = ui.interact(rect, ui.make_persistent_id(id), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && response.hovered() {
        ui.painter().rect_stroke(rect, CornerRadius::same(5), Stroke::new(2.0, Palette::GOLD), egui::StrokeKind::Inside);
    }
    let response = if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Klick: so antworten") } else { response };
    enabled && response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(serial: u32) -> ProposalRow {
        let Some(kingdom) = kingdoms_sim::diplomacy::KingdomId::from_index(0) else { panic!("kingdom 0") };
        ProposalRow {
            serial,
            kingdom,
            kingdom_name: "Eschenwald".into(),
            kind: kingdoms_sim::diplomacy::ProposalKind::Tribute { gold: 100 },
            title: "Tributforderung".into(),
            text: String::new(),
            expires_tick: 100,
            remaining_text: String::new(),
            accept_label: "100 Gold zahlen".into(),
            decline_label: "Verweigern".into(),
            accept_problem: None,
            decline_text: String::new(),
        }
    }

    #[test]
    fn each_new_proposal_opens_the_card_once() {
        let mut d = ProposalDialog::default();
        d.update(&[row(3)], true);
        assert!(!d.is_open(), "waits while the event dialog is open");
        d.update(&[row(3)], false);
        assert_eq!(d.open, Some(3));
        // „Später“ hides the card; only a new proposal brings it back, at the new one.
        d.close();
        d.update(&[row(3)], false);
        assert!(!d.is_open(), "a postponed proposal stays hidden");
        d.update(&[row(3), row(4)], false);
        assert_eq!(d.open, Some(4), "the new proposal");
        d.close();
        d.update(&[row(3), row(4)], false);
        assert!(!d.is_open(), "seen proposals stay closed");
        // Proposals that arrive while the card is open join its queue without switching.
        d.show_proposal(3);
        d.update(&[row(3), row(4), row(5)], false);
        assert_eq!(d.open, Some(3));
        d.close();
        d.update(&[row(3), row(4), row(5)], false);
        assert!(!d.is_open(), "5 was presented in the queue");
        // An expired shown proposal gives way to the next younger one, else the last one.
        d.show_proposal(4);
        d.update(&[row(3), row(5)], false);
        assert_eq!(d.open, Some(5));
        d.update(&[row(3)], false);
        assert_eq!(d.open, Some(3));
        d.update(&[], false);
        assert!(!d.is_open());
    }

    #[test]
    fn the_queue_moves_on_to_unanswered_proposals() {
        let rows = [row(1), row(2), row(3)];
        assert_eq!(queue_text(0, 3), "1 von 3");
        assert_eq!(next_open(&rows, 0, |_| true), Some(2));
        assert_eq!(next_open(&rows, 2, |_| true), Some(1), "wraps around");
        assert_eq!(next_open(&rows, 0, |s| s != 2), Some(3), "skips answered ones");
        assert_eq!(next_open(&rows, 1, |s| s == 2), None, "only the answered one is left");
        assert_eq!(next_open(&rows[..1], 0, |_| true), None);
    }

    #[test]
    fn countdown_arithmetic() {
        assert_eq!(time_left_fraction(0, 100, 50), 0.5);
        assert_eq!(time_left_fraction(0, 100, 200), 0.0);
        assert_eq!(minutes_left(0, 0), 0);
    }
}
