//! Modal dialogs: the pause menu, the defeat screen that replaces it once the game is lost
//! (reason, a short summary of the reign, loading or leaving) and the demolition confirmation.

use kingdoms_engine::egui::{self, RichText};
use kingdoms_sim::buildings::demolish_refund;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::BuildingRole;
use kingdoms_sim::defs::Defs;
use kingdoms_sim::military::BattleOutcome;
use kingdoms_sim::ranks::{GameOver, defeat_text};
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::{GameClock, TICKS_PER_DAY};

use super::format::{count, int};
use super::info_panel::refund_text;
use super::{HudAction, HudInput, Icons};
use crate::actions::Action;
use crate::tools::DemolishRequest;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::section_header;

const BUTTON_WIDTH: f32 = 250.0;

fn menu_button(ui: &mut egui::Ui, text: &str, hint: &str, focused: bool) -> egui::Response {
    let button =
        egui::Button::new(RichText::new(text).font(heading_font(17.0))).min_size(egui::vec2(BUTTON_WIDTH, 34.0)).selected(focused);
    let response = ui.add(button);
    if hint.is_empty() { response } else { response.on_hover_text(hint) }
}

/// Height the content of a full-screen menu may take before it scrolls (screen minus the
/// frame's margins).
fn modal_max_height(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() - 80.0).max(120.0)
}

/// True if the dialog should close: click outside, or Esc unless the dialog opened in this very
/// frame (the Esc that opened it must not close it again).
fn dismissed<T>(ctx: &egui::Context, modal: &egui::ModalResponse<T>, fresh: bool) -> bool {
    let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    modal.backdrop_response.clicked() || (esc && !fresh && modal.is_top_modal)
}

/// Entries of the pause menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuEntry {
    Resume,
    Save,
    Load,
    Settings,
    MainMenu,
    Quit,
}

const ENTRIES: [MenuEntry; 6] =
    [MenuEntry::Resume, MenuEntry::Save, MenuEntry::Load, MenuEntry::Settings, MenuEntry::MainMenu, MenuEntry::Quit];

/// Heading of the pause menu, the reason of a lost game (`None` while the game goes on) and the
/// label and hint of its first entry.
pub fn pause_texts(state: &GameState, game_over: Option<GameOver>) -> (&'static str, Option<String>, &'static str, &'static str) {
    match game_over {
        None => ("Pause", None, "Fortsetzen", "Esc"),
        Some(over) => (
            "Niederlage",
            Some(format!("{} Die Zeit steht still: Ihr könnt das Reich noch ansehen, einen Spielstand laden oder ins Hauptmenü zurückkehren.", defeat_text(state, over.reason))),
            "Reich ansehen",
            "Menü schließen (Esc); das Spiel geht nicht weiter",
        ),
    }
}

/// The pause menu (`fresh`: opened this frame; `focus`: entry selected with the keyboard).
/// Returns false once it closes.
pub fn pause_menu(ctx: &egui::Context, input: &HudInput<'_>, fresh: bool, focus: &mut usize, actions: &mut Vec<HudAction>) -> bool {
    let mut open = true;
    let mut chosen = crate::ui::menu::list_navigation(ctx, focus, ENTRIES.len(), |_| true).map(|i| ENTRIES[i]);
    let state = input.session.state();
    let (heading, defeat, resume, resume_hint) = pause_texts(state, state.game_over());
    let modal = egui::Modal::new(egui::Id::unique("hud_pause_menu"))
        .frame(theme::parchment_frame().inner_margin(egui::Margin::symmetric(28, 20)))
        .show(ctx, |ui| {
            ui.set_width(BUTTON_WIDTH);
            // On a small screen the menu scrolls instead of running off the edges.
            egui::ScrollArea::vertical().max_height(modal_max_height(ctx)).min_scrolled_height(modal_max_height(ctx)).auto_shrink([false, true]).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(heading).font(heading_font(26.0)).color(if defeat.is_some() { Palette::BAD } else { Palette::INK }));
                    ui.label(RichText::new(state.clock().display()).color(Palette::INK_SOFT));
                    if let Some(text) = &defeat {
                        ui.add_space(4.0);
                        ui.add(egui::Label::new(RichText::new(text).color(Palette::INK)).wrap());
                    }
                    ui.add_space(6.0);
                    section_header(ui, "Spiel");
                    ui.add_space(4.0);
                    for (i, entry) in ENTRIES.iter().enumerate() {
                        if *entry == MenuEntry::MainMenu {
                            ui.add_space(4.0);
                            section_header(ui, "Verlassen");
                            ui.add_space(4.0);
                        }
                        let (text, hint) = match entry {
                            MenuEntry::Resume => (resume, resume_hint.to_owned()),
                            MenuEntry::Save => ("Speichern …", "Unter einem Namen speichern".to_owned()),
                            MenuEntry::Load => ("Laden …", "Einen Spielstand laden".to_owned()),
                            MenuEntry::Settings => ("Einstellungen", "Audio, Grafik, Spiel, Steuerung".to_owned()),
                            MenuEntry::MainMenu => ("Hauptmenü", "Zurück zum Hauptmenü".to_owned()),
                            MenuEntry::Quit => ("Beenden", "Das Spiel beenden".to_owned()),
                        };
                        let response = menu_button(ui, text, &hint, *focus == i);
                        if response.hovered() {
                            *focus = i;
                        }
                        if response.clicked() {
                            chosen = Some(*entry);
                        }
                        ui.add_space(4.0);
                    }
                    ui.add_space(4.0);
                    if let Some(status) = input.save_status {
                        ui.label(RichText::new(status).small().color(Palette::INK_SOFT));
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let save_hint = input.hint(Action::QuickSave);
                        if ui.small_button(format!("Schnellspeichern ({save_hint})")).clicked() {
                            actions.push(HudAction::QuickSave);
                            open = false;
                        }
                        let load_hint = input.quicksave.unwrap_or("Noch kein Schnellspielstand vorhanden.");
                        let load = ui
                            .add_enabled(input.quicksave.is_some(), egui::Button::new(format!("Schnellladen ({})", input.hint(Action::QuickLoad))).small())
                            .on_hover_text(load_hint)
                            .on_disabled_hover_text(load_hint);
                        if load.clicked() {
                            actions.push(HudAction::QuickLoad);
                            open = false;
                        }
                    });
                });
            });
        });
    match chosen {
        Some(MenuEntry::Resume) => {
            open = false;
            actions.push(HudAction::Sound("ui_close"));
        }
        Some(MenuEntry::Save) => actions.push(HudAction::OpenSave),
        Some(MenuEntry::Load) => actions.push(HudAction::OpenLoad),
        Some(MenuEntry::Settings) => actions.push(HudAction::OpenSettings),
        Some(MenuEntry::MainMenu) => actions.push(HudAction::MainMenu),
        Some(MenuEntry::Quit) => actions.push(HudAction::Quit),
        None => {}
    }
    if open && dismissed(ctx, &modal, fresh) {
        open = false;
        actions.push(HudAction::Sound("ui_close"));
    }
    open
}

/// The reign in numbers for the defeat screen.
#[derive(Clone, Debug, PartialEq)]
pub struct DefeatSummary {
    /// Heading reason („Das Hauptquartier ist gefallen: …“).
    pub reason: String,
    /// Date of the defeat.
    pub date: String,
    /// Rows of label and value.
    pub rows: Vec<(&'static str, String)>,
}

impl DefeatSummary {
    pub fn compute(state: &GameState, defs: &Defs, over: GameOver) -> Self {
        let rank = kingdoms_sim::report::rank_report(state, defs);
        let days = over.tick / TICKS_PER_DAY;
        let battles = state.military().battles();
        let won = state.military().battles().filter(|b| b.outcome == BattleOutcome::Victory).count();
        let lost = state.military().battles().filter(|b| b.outcome == BattleOutcome::Defeat).count();
        let total = battles.len();
        let buildings = state.buildings().iter().filter(|(_, b)| b.is_complete()).count();
        let wars: Vec<&str> = state.diplomacy().kingdoms().filter(|(_, k)| k.at_war()).map(|(_, k)| k.name()).collect();
        let mut rows = vec![
            ("Regierungszeit", if days == 0 { "weniger als ein Tag".to_owned() } else { count(days, "Tag", "Tage") }),
            ("Höchster Rang", rank.name.clone()),
            ("Bewohner", int(i64::try_from(state.population().len()).unwrap_or(i64::MAX))),
            ("Gebäude", int(i64::try_from(buildings).unwrap_or(i64::MAX))),
            ("Schatzkammer", format!("{} Gold", int(state.economy().treasury()))),
            ("Schlachten", if total == 0 { "keine".to_owned() } else { format!("{total} geschlagen: {won} gewonnen, {lost} verloren") }),
        ];
        if !wars.is_empty() {
            rows.push(("Im Krieg mit", wars.join(", ")));
        }
        Self { reason: defeat_text(state, over.reason), date: GameClock::at_tick(over.tick).display(), rows }
    }
}

/// Entries of the defeat screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DefeatEntry {
    Load,
    MainMenu,
    View,
    Quit,
}

const DEFEAT_ENTRIES: [DefeatEntry; 4] = [DefeatEntry::Load, DefeatEntry::MainMenu, DefeatEntry::View, DefeatEntry::Quit];

/// The defeat screen (instead of the pause menu once the game is lost). Returns false once it
/// closes („Reich ansehen“, Esc).
pub fn defeat_screen(ctx: &egui::Context, input: &HudInput<'_>, over: GameOver, fresh: bool, focus: &mut usize, actions: &mut Vec<HudAction>) -> bool {
    let mut open = true;
    let mut chosen = crate::ui::menu::list_navigation(ctx, focus, DEFEAT_ENTRIES.len(), |_| true).map(|i| DEFEAT_ENTRIES[i]);
    let summary = DefeatSummary::compute(input.session.state(), input.session.defs(), over);
    let modal = egui::Modal::new(egui::Id::unique("hud_defeat_screen"))
        .frame(theme::parchment_frame().stroke(egui::Stroke::new(3.0, Palette::BAD)).inner_margin(egui::Margin::symmetric(30, 22)))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            egui::ScrollArea::vertical().max_height(modal_max_height(ctx)).min_scrolled_height(modal_max_height(ctx)).auto_shrink([false, true]).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Niederlage").font(heading_font(34.0)).color(Palette::BAD));
                    ui.label(RichText::new(&summary.date).color(Palette::INK_SOFT));
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new(&summary.reason).size(17.0).color(Palette::INK)).wrap());
                    ui.add_space(2.0);
                    ui.label(RichText::new("Die Zeit steht still: Das Spiel geht nicht weiter.").small().color(Palette::INK_SOFT));
                });
                section_header(ui, "Eure Herrschaft");
                egui::Grid::new("defeat_summary").num_columns(2).spacing([18.0, 4.0]).show(ui, |ui| {
                    for (label, value) in &summary.rows {
                        ui.label(RichText::new(*label).color(Palette::INK_SOFT));
                        ui.label(RichText::new(value).strong().color(Palette::INK));
                        ui.end_row();
                    }
                });
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    for (i, entry) in DEFEAT_ENTRIES.iter().enumerate() {
                        let (text, hint) = match entry {
                            DefeatEntry::Load => ("Spielstand laden", "Einen früheren Spielstand laden"),
                            DefeatEntry::MainMenu => ("Hauptmenü", "Zurück zum Hauptmenü"),
                            DefeatEntry::View => ("Reich ansehen", "Fenster schließen (Esc öffnet es wieder)"),
                            DefeatEntry::Quit => ("Beenden", "Das Spiel beenden"),
                        };
                        let response = menu_button(ui, text, hint, *focus == i);
                        if response.hovered() {
                            *focus = i;
                        }
                        if response.clicked() {
                            chosen = Some(*entry);
                        }
                        ui.add_space(4.0);
                    }
                });
            });
        });
    match chosen {
        Some(DefeatEntry::Load) => actions.push(HudAction::OpenLoad),
        Some(DefeatEntry::MainMenu) => actions.push(HudAction::MainMenu),
        Some(DefeatEntry::Quit) => actions.push(HudAction::Quit),
        Some(DefeatEntry::View) => {
            open = false;
            actions.push(HudAction::Sound("ui_close"));
        }
        None => {}
    }
    if open && dismissed(ctx, &modal, fresh) {
        open = false;
        actions.push(HudAction::Sound("ui_close"));
    }
    open
}

/// Confirmation before demolishing an important building. Returns false once it closes.
pub fn confirm_demolish(
    ctx: &egui::Context,
    input: &HudInput<'_>,
    icons: &Icons,
    request: &DemolishRequest,
    actions: &mut Vec<HudAction>,
) -> bool {
    let state = input.session.state();
    let defs = input.session.defs();
    let Some(b) = state.building(request.building) else { return false };
    let Some(def) = defs.buildings().get(b.kind().index()) else { return false };
    let mut open = true;
    let modal = egui::Modal::new(egui::Id::unique("hud_confirm_demolish"))
        .frame(theme::parchment_frame().inner_margin(egui::Margin::symmetric(24, 18)))
        .show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(RichText::new("Abriss bestätigen").font(heading_font(22.0)).color(Palette::INK));
            ui.add_space(4.0);
            ui.label(format!("Soll „{}“ wirklich abgerissen werden?", def.name));
            let residents = state.population().resident_count(request.building);
            if residents > 0 {
                let text = if residents == 1 { "1 Bewohner verliert sein Zuhause.".to_owned() } else { format!("{residents} Bewohner verlieren ihr Zuhause.") };
                ui.colored_label(Palette::WARN, text);
            }
            let workers = b.staffed();
            if workers > 0 && b.is_complete() {
                let text = if workers == 1 { "1 Arbeiter verliert seine Arbeit.".to_owned() } else { format!("{workers} Arbeiter verlieren ihre Arbeit.") };
                ui.colored_label(Palette::WARN, text);
            }
            // What the simulation would return, or why it refuses (the last headquarters).
            let refund = demolish_refund(state, defs, request.building);
            match &refund {
                Ok(refund) => {
                    if def.role == BuildingRole::Headquarters {
                        ui.colored_label(Palette::WARN, "Das Gebiet dieses Hauptquartiers geht verloren.");
                    }
                    ui.add_space(4.0);
                    refund_text(ui, icons, defs, refund);
                }
                Err(reason) => {
                    ui.colored_label(Palette::BAD, reason.to_string());
                }
            }
            let allowed = refund.is_ok();
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let demolish = egui::Button::new(RichText::new("Abreißen").strong().color(Palette::PARCHMENT_LIGHT)).fill(Palette::BAD);
                if ui.add_enabled(allowed, demolish).clicked() {
                    actions.push(HudAction::Command(Command::DemolishBuilding { building: request.building }));
                    actions.push(HudAction::Deselect);
                    open = false;
                }
                if ui.button("Abbrechen").clicked() {
                    actions.push(HudAction::Sound("ui_close"));
                    open = false;
                }
            });
        });
    if dismissed(ctx, &modal, false) {
        open = false;
        actions.push(HudAction::Sound("ui_close"));
    }
    open
}

#[cfg(test)]
mod tests {
    use kingdoms_sim::defs::Defs;
    use kingdoms_sim::diplomacy::KingdomId;
    use kingdoms_sim::ranks::{DefeatReason, GameOver};
    use kingdoms_sim::state::{GameSettings, GameState};
    use kingdoms_sim::world::WorldSize;

    use super::pause_texts;

    #[test]
    fn a_lost_game_names_the_reason_in_the_pause_menu() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = Defs::load(&dir).unwrap_or_else(|e| panic!("shipped game data must load: {e}"));
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        assert_eq!(pause_texts(&state, None), ("Pause", None, "Fortsetzen", "Esc"));
        let (heading, text, resume, _) = pause_texts(&state, Some(GameOver { reason: DefeatReason::Depopulated, tick: 4800 }));
        assert_eq!((heading, resume), ("Niederlage", "Reich ansehen"));
        assert!(text.is_some_and(|t| t.starts_with("Niemand lebt mehr in Eurem Reich. Die Zeit steht still")));
        // The conqueror is named when the kingdom exists.
        let (id, kingdom) = state.diplomacy().kingdoms().next().unwrap_or_else(|| panic!("a small map has kingdoms"));
        let conquered = GameOver { reason: DefeatReason::Conquered { kingdom: id }, tick: 4800 };
        let text = pause_texts(&state, Some(conquered)).1.unwrap_or_default();
        assert!(text.contains(kingdom.name()), "{text}");
        let Some(unknown) = KingdomId::from_index(6) else { panic!("index 6 is a valid kingdom id") };
        let unknown = GameOver { reason: DefeatReason::Conquered { kingdom: unknown }, tick: 1 };
        assert!(pause_texts(&state, Some(unknown)).1.is_some_and(|t| t.contains("einem feindlichen Reich")));
        // The defeat screen sums up the reign.
        let summary = super::DefeatSummary::compute(&state, &defs, conquered);
        assert!(summary.reason.contains(kingdom.name()));
        let row = |label: &str| summary.rows.iter().find(|(l, _)| *l == label).map(|(_, v)| v.clone()).unwrap_or_default();
        assert_eq!(row("Regierungszeit"), "1 Tag");
        assert_eq!(row("Bewohner"), "12");
        assert_eq!(row("Schlachten"), "keine");
        let early = super::DefeatSummary::compute(&state, &defs, GameOver { reason: DefeatReason::Depopulated, tick: 100 });
        assert!(early.rows.iter().any(|(l, v)| *l == "Regierungszeit" && v == "weniger als ein Tag"));
        assert!(!row("Höchster Rang").is_empty());
    }
}
