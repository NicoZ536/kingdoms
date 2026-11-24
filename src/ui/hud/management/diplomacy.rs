//! „Diplomatie“: the other kingdoms and the player's dealings with them (docs/game_design.md
//! §10). Left the kingdoms (crest, name, state and relation), right the chosen kingdom: ruler and
//! personality, the relation with its named parts, war and treaties with their remaining time,
//! estimates of its people, strength and wealth, the goods it wants and offers at the player's
//! prices, every diplomatic action with the simulation's costs, the kingdom's expected answer or
//! why the action is impossible, and the history with it. Proposals of the kingdoms wait at the
//! top as decisions with their deadline.
//!
//! Everything comes from `report::diplomacy_report` / `report::kingdom_actions`; every control
//! queues a real `Command::Diplomacy` or `Command::AnswerProposal`. Gifts, tribute demands and
//! peace offers with gold open a dialog for the amount; declaring war and cancelling a treaty ask
//! first. Sim rejections come back as toasts.

use kingdoms_engine::egui::{self, Align, Layout, RichText, Ui};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{Defs, ResourceAmount, ResourceId};
use kingdoms_sim::diplomacy::actions::{self, Judgement};
use kingdoms_sim::diplomacy::{DiplomaticAction, KingdomId, PeaceTerms, RelationState};
use kingdoms_sim::report::{self, ActionRow, DiplomacyReport, KingdomRow, ProposalRow};
use kingdoms_sim::state::GameState;

use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, TabContext, card_row, columns, command, hint};
use crate::session::Session;
use crate::ui::hud::format::{int, signed};
use crate::ui::hud::heraldry::{crest, relation_bar, state_color};
use crate::ui::hud::{HudAction, HudInput, Icons};
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Mark, mark, progress_bar, section_header};

/// Width of the kingdom list.
const LIST_WIDTH: f32 = 250.0;

/// Data of the tab, recomputed with the reports.
struct Data {
    report: DiplomacyReport,
    /// The actions towards each kingdom (in the order of `report.kingdoms`).
    actions: Vec<Vec<ActionRow>>,
    exhaustion_peace: u16,
}

/// A dialog of the tab: the amount of a gift, tribute or peace payment, or a confirmation.
#[derive(Clone, Debug, PartialEq)]
pub enum ActionDialog {
    /// A gift of gold and goods.
    Gift { kingdom: KingdomId, gold: u32, goods: Vec<ResourceAmount>, pick: Option<ResourceId> },
    /// A tribute demand.
    Tribute { kingdom: KingdomId, gold: u32 },
    /// Peace the player pays for (`player_pays`) or with reparations from the kingdom.
    PeaceGold { kingdom: KingdomId, gold: u32, player_pays: bool },
    /// An action with grave consequences (war, cancelling a treaty).
    Confirm { kingdom: KingdomId, action: DiplomaticAction, title: String, text: String },
}

impl ActionDialog {
    /// The dialog an action opens (`None`: it is sent right away).
    pub fn for_action(kingdom: KingdomId, row: &ActionRow, name: &str) -> Option<ActionDialog> {
        match &row.action {
            DiplomaticAction::Gift { gold, goods } => Some(ActionDialog::Gift { kingdom, gold: *gold, goods: goods.clone(), pick: None }),
            DiplomaticAction::DemandTribute { gold } => Some(ActionDialog::Tribute { kingdom, gold: *gold }),
            DiplomaticAction::ProposePeace { terms: PeaceTerms::PlayerPays { gold } } => {
                Some(ActionDialog::PeaceGold { kingdom, gold: *gold, player_pays: true })
            }
            DiplomaticAction::ProposePeace { terms: PeaceTerms::KingdomPays { gold } } => {
                Some(ActionDialog::PeaceGold { kingdom, gold: *gold, player_pays: false })
            }
            DiplomaticAction::DeclareWar => Some(ActionDialog::Confirm {
                kingdom,
                action: row.action.clone(),
                title: format!("{name} den Krieg erklären?"),
                text: format!("{}. Feindliche Heere können Euer Land angreifen; Verträge mit {name} enden.", row.cost_text),
            }),
            DiplomaticAction::CancelTreaty { .. } => Some(ActionDialog::Confirm {
                kingdom,
                action: row.action.clone(),
                title: format!("{}?", row.label),
                text: format!("{}. Alle Reiche merken sich den Wortbruch.", row.cost_text),
            }),
            DiplomaticAction::ProposeTreaty { .. } | DiplomaticAction::ProposePeace { .. } => None,
        }
    }

    /// The kingdom it concerns.
    pub fn kingdom(&self) -> KingdomId {
        match self {
            ActionDialog::Gift { kingdom, .. }
            | ActionDialog::Tribute { kingdom, .. }
            | ActionDialog::PeaceGold { kingdom, .. }
            | ActionDialog::Confirm { kingdom, .. } => *kingdom,
        }
    }

    /// The action the dialog sends as it stands.
    pub fn action(&self) -> DiplomaticAction {
        match self {
            ActionDialog::Gift { gold, goods, .. } => {
                DiplomaticAction::Gift { gold: *gold, goods: goods.iter().filter(|g| g.amount > 0).copied().collect() }
            }
            ActionDialog::Tribute { gold, .. } => DiplomaticAction::DemandTribute { gold: *gold },
            ActionDialog::PeaceGold { gold, player_pays: true, .. } => DiplomaticAction::ProposePeace { terms: PeaceTerms::PlayerPays { gold: *gold } },
            ActionDialog::PeaceGold { gold, player_pays: false, .. } => DiplomaticAction::ProposePeace { terms: PeaceTerms::KingdomPays { gold: *gold } },
            ActionDialog::Confirm { action, .. } => action.clone(),
        }
    }
}

/// Value of a gift in gold at the player's market prices (for the relation estimate).
pub fn gift_value(state: &GameState, gold: u32, goods: &[ResourceAmount]) -> i64 {
    let prices = state.economy().prices();
    let goods: i64 = goods.iter().map(|g| i64::from(g.amount) * i64::from(prices.centi(g.resource)) / 100).sum();
    i64::from(gold) + goods
}

/// The answer queued for proposal `serial` while the game waits for its next tick.
pub fn queued_answer(session: &Session, serial: u32) -> Option<bool> {
    session.queued_commands().iter().find_map(|c| match c {
        Command::AnswerProposal { serial: s, accept } if *s == serial => Some(*accept),
        _ => None,
    })
}

/// State of the tab.
#[derive(Default)]
pub struct DiplomacyTab {
    data: Cached<Data>,
    selected: Option<KingdomId>,
    dialog: Option<ActionDialog>,
}

impl DiplomacyTab {
    /// Shows kingdom `id` (from other tabs and the map).
    pub fn select(&mut self, id: KingdomId) {
        self.selected = Some(id);
    }

    /// True while a dialog of the tab is open (the map then ignores keys).
    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let input = ctx.input;
        let (state, defs) = (input.session.state(), input.session.defs());
        let data = self.data.get(input, || {
            let report = report::diplomacy_report(state, defs);
            let actions = report.kingdoms.iter().map(|k| report::kingdom_actions(state, defs, k.id)).collect();
            Data { report, actions, exhaustion_peace: u16::from(defs.rules().diplomacy.war.exhaustion_peace) }
        });
        if data.report.kingdoms.is_empty() {
            hint(ui, "In dieser Welt gibt es keine anderen Königreiche. Handelsaufträge laufen über den Weltmarkt.");
            return;
        }
        let at_war = data.report.kingdoms.iter().filter(|k| k.war.is_some()).count();
        let treaties: usize = data.report.kingdoms.iter().map(|k| k.treaties.len()).sum();
        let friendly = data.report.kingdoms.iter().filter(|k| well_disposed(k.state, k.war.is_some())).count();
        card_row(
            ui,
            &[
                ("Königreiche", data.report.kingdoms.len().to_string(), Palette::INK),
                ("Eure Stärke", int(i64::from(data.report.player_strength)), Palette::INK),
                ("Wohlgesonnen", friendly.to_string(), Palette::GOOD),
                ("Verträge", treaties.to_string(), Palette::INFO),
                ("Kriege", at_war.to_string(), if at_war > 0 { Palette::BAD } else { Palette::INK }),
                ("Offene Anfragen", data.report.proposals.len().to_string(), if data.report.proposals.is_empty() { Palette::INK } else { Palette::WARN }),
            ],
        );
        if !data.report.proposals.is_empty() {
            section_header(ui, "Anfragen an Euch");
            for p in &data.report.proposals {
                let color = data.report.kingdoms.iter().find(|k| k.id == p.kingdom).map_or(0, |k| k.color);
                proposal_card(ui, ctx.icons, input, p, color, actions);
            }
        }
        let selected = self.selected.filter(|id| data.report.kingdoms.iter().any(|k| k.id == *id)).unwrap_or(data.report.kingdoms[0].id);
        self.selected = Some(selected);
        ui.add_space(6.0);
        let mut open_dialog = None;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(LIST_WIDTH);
                for k in &data.report.kingdoms {
                    if kingdom_entry(ui, ctx.icons, k, k.id == selected).clicked() {
                        self.selected = Some(k.id);
                        actions.push(HudAction::Sound("ui_click"));
                    }
                }
                if !data.report.ai_wars.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Kriege zwischen den Reichen").font(heading_font(14.0)).color(Palette::INK));
                    for w in &data.report.ai_wars {
                        ui.add(egui::Label::new(RichText::new(w).small().color(Palette::BAD)).wrap());
                    }
                }
            });
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                if let Some(i) = data.report.kingdoms.iter().position(|k| k.id == selected) {
                    let k = &data.report.kingdoms[i];
                    open_dialog = kingdom_details(ui, ctx, k, &data.actions[i], data.exhaustion_peace, actions);
                }
            });
        });
        if let Some(dialog) = open_dialog {
            self.dialog = Some(dialog);
            actions.push(HudAction::Sound("ui_open"));
        }
        section_header(ui, "Diplomatischer Verlauf");
        if data.report.history.is_empty() {
            hint(ui, "Noch keine diplomatischen Ereignisse.");
        }
        for (i, h) in data.report.history.iter().enumerate() {
            table::row(ui, ("dipl_history", i), ROW_HEIGHT, i % 2 == 1, false, |ui| {
                table::text(ui, 170.0, CellAlign::Left, RichText::new(&h.date).small().color(Palette::INK_SOFT));
                table::cell(ui, ui.available_width(), CellAlign::Left, |ui| {
                    ui.add(egui::Label::new(RichText::new(&h.text).color(Palette::INK)).truncate()).on_hover_text(&h.text);
                });
            });
        }
        self.show_dialog(ui.ctx(), input, ctx.icons, actions);
    }

    /// The open dialog (amount or confirmation) as a modal window.
    fn show_dialog(&mut self, ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons, actions: &mut Vec<HudAction>) {
        let Some(dialog) = self.dialog.as_mut() else { return };
        let (state, defs) = (input.session.state(), input.session.defs());
        let kingdom = dialog.kingdom();
        let Some(k) = state.diplomacy().kingdom(kingdom) else {
            self.dialog = None;
            return;
        };
        let name = k.name().to_owned();
        let color = k.color();
        let mut close = false;
        let mut send = false;
        let modal = egui::Modal::new(egui::Id::unique("diplomacy_dialog")).frame(theme::parchment_frame().inner_margin(egui::Margin::same(18))).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.horizontal(|ui| {
                crest(ui, icons, 40.0, color);
                let title = match dialog {
                    ActionDialog::Gift { .. } => format!("Geschenk an {name}"),
                    ActionDialog::Tribute { .. } => format!("Tribut von {name} fordern"),
                    ActionDialog::PeaceGold { player_pays: true, .. } => format!("Frieden mit {name} erkaufen"),
                    ActionDialog::PeaceGold { player_pays: false, .. } => format!("Entschädigung von {name} fordern"),
                    ActionDialog::Confirm { title, .. } => title.clone(),
                };
                ui.label(RichText::new(title).font(heading_font(20.0)).color(Palette::INK));
            });
            ui.add_space(6.0);
            let treasury = state.economy().treasury().max(0);
            match dialog {
                ActionDialog::Gift { gold, goods, pick, .. } => {
                    gold_row(ui, "Gold", gold, u32::try_from(treasury).unwrap_or(u32::MAX), Some(treasury));
                    ui.label(RichText::new("Waren aus dem Reichslager").small().color(Palette::INK_SOFT));
                    goods_editor(ui, icons, state, defs, goods, pick);
                    let value = gift_value(state, *gold, goods);
                    let points = actions::gift_points(defs, value);
                    ui.label(format!("Wert etwa {} Gold · Beziehung etwa +{points:.0}", int(value)));
                }
                ActionDialog::Tribute { gold, .. } => {
                    gold_row(ui, "Forderung", gold, 100_000, None);
                    ui.label(RichText::new(format!("Beziehung {} auch bei Zahlung.", defs.rules().diplomacy.relations.demanded)).small().color(Palette::INK_SOFT));
                }
                ActionDialog::PeaceGold { gold, player_pays, .. } => {
                    let max = if *player_pays { u32::try_from(treasury).unwrap_or(u32::MAX) } else { 100_000 };
                    gold_row(ui, if *player_pays { "Ihr zahlt" } else { "Sie zahlen" }, gold, max, player_pays.then_some(treasury));
                }
                ActionDialog::Confirm { text, .. } => {
                    ui.add(egui::Label::new(RichText::new(text.as_str()).color(Palette::INK)).wrap());
                }
            }
            let action = dialog.action();
            let problem = actions::action_problem(state, defs, kingdom, &action);
            let prospect = if problem.is_none() { actions::judge(state, defs, kingdom, &action) } else { None };
            ui.add_space(4.0);
            match (&problem, &prospect) {
                (Some(p), _) => {
                    ui.horizontal_top(|ui| {
                        mark(ui, Mark::Error);
                        ui.add(egui::Label::new(RichText::new(p).color(Palette::BAD)).wrap());
                    });
                }
                (None, Some(j)) => prospect_line(ui, j),
                (None, None) => {}
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = match dialog {
                    ActionDialog::Confirm { action: DiplomaticAction::DeclareWar, .. } => "Krieg erklären",
                    ActionDialog::Confirm { .. } => "Kündigen",
                    ActionDialog::Gift { .. } => "Geschenk senden",
                    ActionDialog::Tribute { .. } => "Fordern",
                    ActionDialog::PeaceGold { .. } => "Frieden anbieten",
                };
                let danger = matches!(dialog, ActionDialog::Confirm { .. });
                let button = if danger {
                    egui::Button::new(RichText::new(label).strong().color(Palette::PARCHMENT_LIGHT)).fill(Palette::BAD)
                } else {
                    egui::Button::new(RichText::new(label).strong())
                };
                if ui.add_enabled(problem.is_none(), button).clicked() {
                    send = true;
                }
                if ui.button("Abbrechen").clicked() {
                    close = true;
                }
            });
        });
        if send {
            command(actions, Command::Diplomacy { kingdom, action: dialog.action() });
            close = true;
        } else if modal.should_close() {
            close = true;
            actions.push(HudAction::Sound("ui_close"));
        }
        if close {
            self.dialog = None;
        }
    }
}

/// A gold amount with a slider-like drag value (`have`: the treasury, shown when it limits).
fn gold_row(ui: &mut Ui, label: &str, gold: &mut u32, max: u32, have: Option<i64>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(gold).range(0..=max.max(1)).speed(5.0).suffix(" Gold"));
        if let Some(have) = have {
            ui.label(RichText::new(format!("(Schatzkammer: {} Gold)", int(have))).small().color(Palette::INK_SOFT));
        }
    });
}

/// Rows of goods with amounts (at most the stock) and a picker to add another good.
fn goods_editor(ui: &mut Ui, icons: &Icons, state: &GameState, defs: &Defs, goods: &mut Vec<ResourceAmount>, pick: &mut Option<ResourceId>) {
    let mut remove = None;
    for (i, g) in goods.iter_mut().enumerate() {
        let Some(r) = defs.resources().get(g.resource.index()) else { continue };
        let stock = state.economy().available(g.resource);
        ui.horizontal(|ui| {
            if let Some(icon) = icons.get(&format!("res_{}", r.key)) {
                icon.show(ui, 18.0);
            }
            ui.label(&r.name);
            ui.add(egui::DragValue::new(&mut g.amount).range(0..=stock.max(1)).speed(1.0));
            ui.label(RichText::new(format!("von {}", int(i64::from(stock)))).small().color(Palette::INK_SOFT));
            if ui.small_button("×").on_hover_text("Ware entfernen").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        goods.remove(i);
    }
    ui.horizontal(|ui| {
        let name = pick.and_then(|r| defs.resources().get(r.index())).map_or_else(|| "Ware wählen …".to_owned(), |r| r.name.clone());
        egui::ComboBox::from_id_salt("gift_good").selected_text(name).width(180.0).height(300.0).show_ui(ui, |ui| {
            for r in defs.resources().iter().filter(|r| state.economy().available(r.id) > 0) {
                ui.selectable_value(pick, Some(r.id), format!("{} ({})", r.name, int(i64::from(state.economy().available(r.id)))));
            }
        });
        let addable = pick.filter(|r| goods.iter().all(|g| g.resource != *r));
        if ui.add_enabled(addable.is_some(), egui::Button::new("Hinzufügen")).clicked()
            && let Some(resource) = addable
        {
            let amount = state.economy().available(resource).min(10);
            goods.push(ResourceAmount { resource, amount });
        }
    });
}

/// „Würde zustimmen / ablehnen“ with the strongest reasons.
fn prospect_line(ui: &mut Ui, j: &Judgement) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        if j.accepted {
            mark(ui, Mark::Ok);
            ui.add(egui::Label::new(RichText::new(format!("Würde zustimmen ({} von {}): {}", j.score, j.threshold, j.arguments())).color(Palette::GOOD)).wrap());
        } else {
            mark(ui, Mark::Warning);
            ui.add(egui::Label::new(RichText::new(format!("Würde ablehnen ({} von {}): {}", j.score, j.threshold, j.objections())).color(Palette::BAD)).wrap());
        }
    });
}

/// One kingdom in the list; returns the click response.
fn kingdom_entry(ui: &mut Ui, icons: &Icons, k: &KingdomRow, selected: bool) -> egui::Response {
    let fill = if selected { Palette::GOLD_BRIGHT.gamma_multiply(0.45) } else { Palette::PARCHMENT_LIGHT };
    let inner = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { Palette::GOLD_DARK } else { Palette::PARCHMENT_SHADE }))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(LIST_WIDTH - 14.0);
            ui.horizontal(|ui| {
                crest(ui, icons, 34.0, k.color);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new(&k.name).font(heading_font(15.0)).color(Palette::INK));
                    ui.label(RichText::new(format!("{} · {}", k.state_name, signed(i64::from(k.relation)))).small().strong().color(state_color(k.state, k.war.is_some())));
                    relation_bar(ui, k.relation, k.state, k.war.is_some(), None, LIST_WIDTH - 70.0);
                });
            });
        });
    let response = ui.interact(inner.response.rect, ui.make_persistent_id(("kingdom_entry", k.id.index())), egui::Sense::click());
    ui.add_space(2.0);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A proposal of a kingdom with its deadline and the two answers.
fn proposal_card(ui: &mut Ui, icons: &Icons, input: &HudInput<'_>, p: &ProposalRow, color: u8, actions: &mut Vec<HudAction>) {
    let queued = queued_answer(input.session, p.serial);
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            crest(ui, icons, 34.0, color);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&p.title).font(heading_font(15.0)).color(Palette::INK));
                    ui.label(RichText::new(format!("{} · {}", p.kingdom_name, p.remaining_text)).small().color(Palette::INK_SOFT));
                });
                ui.add(egui::Label::new(RichText::new(&p.text).color(Palette::INK)).wrap());
                match queued {
                    Some(accept) => {
                        let label = if accept { &p.accept_label } else { &p.decline_label };
                        ui.colored_label(Palette::WARN, format!("Antwort „{label}“ wird übernommen, sobald das Spiel weiterläuft."));
                    }
                    None => {
                        ui.horizontal(|ui| {
                            let accept = ui
                                .add_enabled(p.accept_problem.is_none(), egui::Button::new(RichText::new(&p.accept_label).strong()))
                                .on_disabled_hover_text(p.accept_problem.clone().unwrap_or_default());
                            if accept.clicked() {
                                command(actions, Command::AnswerProposal { serial: p.serial, accept: true });
                            }
                            if ui.button(&p.decline_label).on_hover_text(&p.decline_text).clicked() {
                                command(actions, Command::AnswerProposal { serial: p.serial, accept: false });
                            }
                            if let Some(problem) = &p.accept_problem {
                                ui.label(RichText::new(problem).small().color(Palette::BAD));
                            } else {
                                ui.label(RichText::new(format!("Ablehnen: {}", p.decline_text)).small().color(Palette::INK_SOFT));
                            }
                        });
                    }
                }
            });
        });
    });
}

/// The chosen kingdom; returns a dialog to open.
fn kingdom_details(
    ui: &mut Ui,
    ctx: &TabContext<'_, '_>,
    k: &KingdomRow,
    rows: &[ActionRow],
    exhaustion_peace: u16,
    actions: &mut Vec<HudAction>,
) -> Option<ActionDialog> {
    let icons = ctx.icons;
    let mut dialog = None;
    ui.horizontal(|ui| {
        crest(ui, icons, 56.0, k.color);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new(&k.name).font(heading_font(24.0)).color(Palette::INK));
            ui.label(RichText::new(format!("{} · Wesen: {}", k.ruler, k.personality_name)).color(Palette::INK_SOFT));
            ui.label(RichText::new(&k.state_name).strong().color(state_color(k.state, k.war.is_some())));
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            if ui.button("Kamera hin").on_hover_text("Zum Sitz des Reiches").clicked() {
                actions.push(HudAction::Focus(k.seat));
                actions.push(HudAction::Sound("ui_click"));
            }
        });
    });
    ui.add_space(4.0);
    relation_bar(ui, k.relation, k.state, k.war.is_some(), Some(&k.state_name), ui.available_width());
    ui.add_space(2.0);
    // Parts of the relation, two per line.
    egui::Grid::new(("relation_parts", k.id.index())).num_columns(4).spacing([10.0, 2.0]).show(ui, |ui| {
        for (i, part) in k.relation_parts.iter().enumerate() {
            ui.label(RichText::new(&part.label).small().color(Palette::INK));
            let color = if part.points >= 0 { Palette::GOOD } else { Palette::BAD };
            ui.label(RichText::new(signed(i64::from(part.points))).small().strong().color(color));
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    if let Some(war) = &k.war {
        ui.add_space(4.0);
        theme::inset_frame().fill(Palette::BAD.gamma_multiply(0.12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(&war.text).strong().color(Palette::BAD));
            let f = f32::from(war.exhaustion) / f32::from(exhaustion_peace.max(1));
            progress_bar(ui, f, Palette::WARN, &format!("Kriegsmüdigkeit {} von {} (dann bietet {} Frieden an)", war.exhaustion, exhaustion_peace, k.name));
        });
    }
    ui.add_space(4.0);
    egui::Grid::new(("kingdom_facts", k.id.index())).num_columns(4).min_col_width(90.0).spacing([14.0, 3.0]).show(ui, |ui| {
        fact(ui, "Einwohner", int(i64::from(k.population)));
        fact(ui, "Wohlstand", format!("etwa {} Gold", int(k.wealth)));
        ui.end_row();
        fact(ui, "Gebiet", format!("{} Felder", int(i64::from(k.territory_tiles))));
        fact(ui, "Bauwerke", k.structures.to_string());
        ui.end_row();
        fact(ui, "Karawanen", format!("{} % Überfallrisiko je Weg", k.route_danger_percent));
        fact(ui, "Grenze", if k.borders_player { "grenzt an Euer Land".to_owned() } else { "keine gemeinsame".to_owned() });
        ui.end_row();
    });
    let treaties = if k.treaties.is_empty() {
        "keine".to_owned()
    } else {
        k.treaties.iter().map(|t| format!("{} ({})", t.name, t.remaining_text)).collect::<Vec<_>>().join(", ")
    };
    let enemies = if k.enemies.is_empty() { "keine".to_owned() } else { k.enemies.join(", ") };
    for (label, value) in [("Stärke", format!("etwa {} – {}", k.strength, k.strength_text)), ("Verträge", treaties), ("Im Krieg mit", enemies)] {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(label).color(Palette::INK_SOFT));
            ui.label(RichText::new(value).color(Palette::INK));
        });
    }
    ui.horizontal_top(|ui| {
        mark(ui, if k.access { Mark::Ok } else { Mark::Warning });
        ui.add(egui::Label::new(RichText::new(&k.access_text).color(Palette::INK)).wrap());
    });
    // Goods.
    ui.add_space(2.0);
    columns(ui, 2, |cols| {
        let defs = ctx.input.session.defs();
        goods_list(&mut cols[0], icons, defs, "Sucht (Ihr verkauft)", &k.wanted);
        goods_list(&mut cols[1], icons, defs, "Bietet an (Ihr kauft)", &k.offered);
    });
    section_header(ui, "Handlungen");
    for row in rows {
        if let Some(d) = action_card(ui, k, row, actions) {
            dialog = Some(d);
        }
    }
    if !k.history.is_empty() {
        section_header(ui, &format!("Verlauf mit {}", k.name));
        for h in &k.history {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(&h.date).small().color(Palette::INK_SOFT));
                ui.add(egui::Label::new(RichText::new(&h.text).small().color(Palette::INK)).wrap());
            });
        }
    }
    dialog
}

fn fact(ui: &mut Ui, label: &str, value: String) {
    ui.label(RichText::new(label).color(Palette::INK_SOFT));
    ui.add(egui::Label::new(RichText::new(value).color(Palette::INK)).extend());
}

fn goods_list(ui: &mut Ui, icons: &Icons, defs: &Defs, title: &str, goods: &[report::GoodRow]) {
    ui.label(RichText::new(title).font(heading_font(14.0)).color(Palette::INK));
    if goods.is_empty() {
        ui.label(RichText::new("nichts").small().italics().color(Palette::INK_SOFT));
    }
    for g in goods {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if let Some(icon) = defs.resources().get(g.resource.index()).and_then(|r| icons.get(&format!("res_{}", r.key))) {
                icon.show(ui, 18.0);
            }
            ui.label(RichText::new(format!("{} {}", int(i64::from(g.amount)), g.name)).small().color(Palette::INK));
            ui.label(RichText::new(format!("je {} Gold", super::storage::price(g.price_centi))).small().color(Palette::INK_SOFT));
        });
    }
}

/// One diplomatic action: label, costs, the kingdom's expected answer or the reason it is
/// impossible, and the button. Returns a dialog to open.
fn action_card(ui: &mut Ui, k: &KingdomRow, row: &ActionRow, actions: &mut Vec<HudAction>) -> Option<ActionDialog> {
    let mut dialog = None;
    let possible = row.problem.is_none();
    let fill = if possible { Palette::PARCHMENT_LIGHT } else { Palette::PARCHMENT_DARK.gamma_multiply(0.6) };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, Palette::PARCHMENT_SHADE))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 150.0).max(200.0));
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let color = if possible { Palette::INK } else { Palette::INK_SOFT };
                    ui.label(RichText::new(&row.label).strong().color(color));
                    ui.label(RichText::new(&row.cost_text).small().color(Palette::INK_SOFT));
                    match (&row.problem, &row.prospect) {
                        (Some(p), _) => {
                            ui.add(egui::Label::new(RichText::new(p).small().color(Palette::BAD)).wrap());
                        }
                        (None, Some(j)) => prospect_line(ui, j),
                        (None, None) => {}
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let opens = ActionDialog::for_action(k.id, row, &k.name);
                    let label = if opens.is_some() { "Ausführen …" } else { "Ausführen" };
                    let button = ui.add_enabled(possible, egui::Button::new(label));
                    let button = match &row.problem {
                        Some(p) => button.on_disabled_hover_text(p.as_str()),
                        None => button,
                    };
                    if button.clicked() {
                        match opens {
                            Some(d) => dialog = Some(d),
                            None => command(actions, Command::Diplomacy { kingdom: k.id, action: row.action.clone() }),
                        }
                    }
                });
            });
        });
    ui.add_space(2.0);
    dialog
}

/// True for a kingdom the card „Wohlgesonnen“ counts: friendly or allied in the simulation's
/// terms (the same state the list names), not at war.
fn well_disposed(state: RelationState, at_war: bool) -> bool {
    !at_war && matches!(state, RelationState::Friendly | RelationState::Allied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::hud::management::tests::test_defs;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    #[test]
    fn well_disposed_follows_the_relation_state() {
        // Neutral reaches up to +29: positive, but not „wohlgesonnen“ as the list labels it.
        assert!(!well_disposed(RelationState::of(15, false), false));
        assert!(well_disposed(RelationState::of(30, false), false));
        assert!(well_disposed(RelationState::of(-20, true), false));
        assert!(!well_disposed(RelationState::of(80, false), true));
        assert!(!well_disposed(RelationState::Rival, false));
    }

    #[test]
    fn dialogs_send_the_edited_action() {
        let defs = test_defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let Some((id, k)) = state.diplomacy().kingdoms().next() else { panic!("a small map has kingdoms") };
        let rows = report::kingdom_actions(&state, &defs, id);
        // Treaty proposals go out directly, gifts and war open a dialog.
        let treaty = rows.iter().find(|r| matches!(r.action, DiplomaticAction::ProposeTreaty { .. })).unwrap_or_else(|| panic!("treaty row"));
        assert!(ActionDialog::for_action(id, treaty, k.name()).is_none());
        let gift = rows.iter().find(|r| matches!(r.action, DiplomaticAction::Gift { .. })).unwrap_or_else(|| panic!("gift row"));
        let Some(mut dialog) = ActionDialog::for_action(id, gift, k.name()) else { panic!("gifts open a dialog") };
        assert_eq!(dialog.action(), gift.action);
        if let ActionDialog::Gift { gold, goods, .. } = &mut dialog {
            *gold = 0;
            let wood = defs.resource_id("wood").unwrap_or_else(|| panic!("wood"));
            goods.push(ResourceAmount { resource: wood, amount: 5 });
            goods.push(ResourceAmount { resource: wood, amount: 0 });
        }
        // Empty rows are dropped; the value counts goods at market prices.
        let DiplomaticAction::Gift { gold, goods } = dialog.action() else { panic!("still a gift") };
        assert_eq!((gold, goods.len()), (0, 1));
        assert!(gift_value(&state, 0, &goods) > 0);
        let war = rows.iter().find(|r| r.action == DiplomaticAction::DeclareWar).unwrap_or_else(|| panic!("war row"));
        let Some(confirm) = ActionDialog::for_action(id, war, k.name()) else { panic!("war asks first") };
        assert_eq!(confirm.action(), DiplomaticAction::DeclareWar);
        assert_eq!(confirm.kingdom(), id);
        // The simulation's own checks drive the dialog's error line.
        let empty = DiplomaticAction::Gift { gold: 0, goods: Vec::new() };
        assert!(actions::action_problem(&state, &defs, id, &empty).is_some());
    }
}
