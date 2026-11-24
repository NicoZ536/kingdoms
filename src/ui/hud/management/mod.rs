//! The kingdom management window („Reichsverwaltung“): one draggable window with a tab per
//! area – economy overview, goods and storage, production, finances, trade, diplomacy,
//! military, workforce, population, agriculture and weather, research, statistics. Only one tab is shown at a time; the tab buttons in the top bar and
//! the hotkeys open the window at their tab (the key of the shown tab closes it again), Esc
//! and the close button close it.
//!
//! Every number comes from the simulation's reports (`kingdoms_sim::report`) or its public
//! state; every control queues a real [`Command`](kingdoms_sim::command::Command). The tabs
//! cache their data and recompute it at most every [`REFRESH_SECONDS`] once the simulation
//! advanced.

mod agriculture;
mod caravans;
pub mod diplomacy;
mod economy;
mod finance;
pub mod military;
mod population;
mod production;
mod research;
mod statistics;
mod storage;
mod table;
mod trade;
mod workforce;

pub use agriculture::seasons_text;

use kingdoms_engine::egui::{self, Align, Color32, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::ResourceId;
use kingdoms_sim::diplomacy::KingdomId;
use kingdoms_sim::time::{DAYS_PER_SEASON, HOURS_PER_DAY, Season};

use super::{HudAction, HudInput, Icons, MARGIN, TOP_BAR_HEIGHT};
use crate::actions::Action;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::icon_button;

/// Seconds between recomputations of a tab's data while the simulation runs.
pub const REFRESH_SECONDS: f64 = 0.25;
/// Preferred outer size of the window (smaller screens shrink it).
const WINDOW_SIZE: Vec2 = Vec2::new(1040.0, 700.0);
/// The research tab is wider: the tech tree and its side column side by side.
const RESEARCH_WINDOW_SIZE: Vec2 = Vec2::new(1240.0, 700.0);
/// Frames the window is kept on top after it opened.
const RAISE_FRAMES: u8 = 3;

/// A tab of the management window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Economy,
    Storage,
    Production,
    Finance,
    Trade,
    Diplomacy,
    Military,
    Workforce,
    Population,
    Agriculture,
    Research,
    Statistics,
}

impl Tab {
    /// All tabs in display order.
    pub const ALL: [Tab; 12] = [
        Tab::Economy,
        Tab::Storage,
        Tab::Production,
        Tab::Finance,
        Tab::Trade,
        Tab::Diplomacy,
        Tab::Military,
        Tab::Workforce,
        Tab::Population,
        Tab::Agriculture,
        Tab::Research,
        Tab::Statistics,
    ];

    /// Window title of the tab.
    pub const fn title(self) -> &'static str {
        match self {
            Tab::Economy => "Wirtschaft",
            Tab::Storage => "Lager & Waren",
            Tab::Production => "Produktion",
            Tab::Finance => "Finanzen",
            Tab::Trade => "Handel",
            Tab::Diplomacy => "Diplomatie",
            Tab::Military => "Militär",
            Tab::Workforce => "Arbeitskräfte",
            Tab::Population => "Bevölkerung",
            Tab::Agriculture => "Landwirtschaft & Wetter",
            Tab::Research => "Forschung",
            Tab::Statistics => "Statistik",
        }
    }

    /// Short label on the tab strip.
    pub const fn label(self) -> &'static str {
        match self {
            Tab::Storage => "Lager",
            Tab::Workforce => "Arbeit",
            Tab::Agriculture => "Landwirtschaft",
            other => other.title(),
        }
    }

    /// The tabs in their groups, row by row, as the tab strip shows them: the economy in the
    /// first row, the people and the realm in the second.
    pub const ROWS: [&'static [TabGroup]; 2] = [
        &[TabGroup {
            name: "Wirtschaft",
            tabs: &[Tab::Storage, Tab::Production, Tab::Economy, Tab::Finance, Tab::Trade],
        }],
        &[
            TabGroup { name: "Volk", tabs: &[Tab::Workforce, Tab::Population, Tab::Agriculture] },
            TabGroup { name: "Reich", tabs: &[Tab::Research, Tab::Diplomacy, Tab::Military, Tab::Statistics] },
        ],
    ];

    /// Icon of the `icons` atlas.
    pub const fn icon(self) -> &'static str {
        match self {
            Tab::Economy => "ui_economy",
            Tab::Storage => "ui_storage",
            Tab::Production => "ui_production",
            Tab::Finance => "ui_finance",
            Tab::Trade => "ui_trade",
            Tab::Diplomacy => "ui_diplomacy",
            Tab::Military => "ui_military",
            Tab::Workforce => "ui_tasks",
            Tab::Population => "ui_population",
            Tab::Agriculture => "ui_farm",
            Tab::Research => "ui_research",
            Tab::Statistics => "ui_statistics",
        }
    }

    /// The hotkey action opening the tab.
    pub const fn action(self) -> Action {
        match self {
            Tab::Economy => Action::WindowEconomy,
            Tab::Storage => Action::WindowStorage,
            Tab::Production => Action::WindowProduction,
            Tab::Finance => Action::WindowFinance,
            Tab::Trade => Action::WindowTrade,
            Tab::Diplomacy => Action::WindowDiplomacy,
            Tab::Military => Action::WindowMilitary,
            Tab::Workforce => Action::WindowWorkforce,
            Tab::Population => Action::WindowPopulation,
            Tab::Agriculture => Action::WindowAgriculture,
            Tab::Research => Action::WindowResearch,
            Tab::Statistics => Action::WindowStatistics,
        }
    }

    /// Name used by the developer option `KINGDOMS_WINDOW`.
    pub const fn key(self) -> &'static str {
        match self {
            Tab::Economy => "wirtschaft",
            Tab::Storage => "lager",
            Tab::Production => "produktion",
            Tab::Finance => "finanzen",
            Tab::Trade => "handel",
            Tab::Diplomacy => "diplomatie",
            Tab::Military => "militaer",
            Tab::Workforce => "arbeit",
            Tab::Population => "bevoelkerung",
            Tab::Agriculture => "landwirtschaft",
            Tab::Research => "forschung",
            Tab::Statistics => "statistik",
        }
    }

    /// The tab named `key` (see [`Tab::key`]; case-insensitive, „ä“/„ö“ accepted).
    pub fn from_key(key: &str) -> Option<Tab> {
        let key = key.trim().to_lowercase().replace('ö', "oe").replace('ä', "ae");
        Tab::ALL.into_iter().find(|t| t.key() == key)
    }
}

/// A named group of tabs on the tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabGroup {
    pub name: &'static str,
    pub tabs: &'static [Tab],
}

/// Data computed from the simulation, refreshed at most every [`REFRESH_SECONDS`] once the
/// simulation tick changed.
pub struct Cached<T> {
    data: Option<T>,
    tick: u64,
    at: f64,
}

impl<T> Default for Cached<T> {
    fn default() -> Self {
        Self { data: None, tick: 0, at: f64::NEG_INFINITY }
    }
}

impl<T> Cached<T> {
    /// True if the data has to be recomputed at `tick` and real time `now`.
    fn is_stale(&self, tick: u64, now: f64) -> bool {
        self.data.is_none() || (tick != self.tick && now - self.at >= REFRESH_SECONDS)
    }

    /// The data, recomputed by `compute` when stale.
    pub fn get(&mut self, input: &HudInput<'_>, compute: impl FnOnce() -> T) -> &T {
        let tick = input.session.state().clock().tick;
        if self.is_stale(tick, input.now) {
            self.data = None;
            self.tick = tick;
            self.at = input.now;
        }
        self.data.get_or_insert_with(compute)
    }

    /// The data as last computed, without refreshing it.
    pub fn peek(&self) -> Option<&T> {
        self.data.as_ref()
    }

    /// Forces a recomputation on the next access.
    pub fn invalidate(&mut self) {
        self.data = None;
    }
}

/// A jump from one tab to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigate {
    /// The statistics tab with the history of a good.
    ResourceStatistics(ResourceId),
    /// The diplomacy tab showing a kingdom.
    Kingdom(KingdomId),
}

/// What the tabs need to draw.
pub struct TabContext<'a, 'b> {
    pub input: &'a HudInput<'b>,
    pub icons: &'a Icons,
}

/// The management window with the state of every tab.
#[derive(Default)]
pub struct Management {
    /// The shown tab (`None`: closed).
    open: Option<Tab>,
    /// Frames left in which the window is raised above the other HUD panels (after opening;
    /// panels that appear for the first time would otherwise cover it).
    raise_frames: u8,
    economy: economy::EconomyTab,
    storage: storage::StorageTab,
    production: production::ProductionTab,
    finance: finance::FinanceTab,
    trade: trade::TradeTab,
    diplomacy: diplomacy::DiplomacyTab,
    military: military::MilitaryTab,
    workforce: workforce::WorkforceTab,
    population: population::PopulationTab,
    agriculture: agriculture::AgricultureTab,
    research: research::ResearchTab,
    statistics: statistics::StatisticsTab,
}

impl Management {
    /// The shown tab, if the window is open.
    pub fn open_tab(&self) -> Option<Tab> {
        self.open
    }

    /// Opens the window at `tab`, or closes it if `tab` is already shown. Returns true if the
    /// window is open afterwards.
    pub fn toggle(&mut self, tab: Tab) -> bool {
        self.open = if self.open == Some(tab) { None } else { Some(tab) };
        self.raise_frames = RAISE_FRAMES;
        self.open.is_some()
    }

    /// Closes the window; returns true if it was open.
    pub fn close(&mut self) -> bool {
        self.open.take().is_some()
    }

    /// Opens the diplomacy tab at kingdom `id`.
    pub fn show_kingdom(&mut self, id: KingdomId) {
        self.diplomacy.select(id);
        if self.open != Some(Tab::Diplomacy) {
            self.open = Some(Tab::Diplomacy);
            self.raise_frames = RAISE_FRAMES;
        }
    }

    /// True while a dialog of a tab (amounts, confirmations) is open.
    pub fn dialog_open(&self) -> bool {
        self.open.is_some() && (self.diplomacy.dialog_open() || self.military.dialog_open() || self.trade.dialog_open())
    }

    /// Forgets everything that belongs to the previous session (after loading a game).
    pub fn reset(&mut self) {
        *self = Self { open: self.open, raise_frames: RAISE_FRAMES, ..Self::default() };
    }

    /// Draws the window (if open) and collects the player's actions. The trade log is read
    /// every frame, also while the window is closed.
    pub fn show(&mut self, ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons, actions: &mut Vec<HudAction>) {
        self.trade.read_events(input.session);
        let Some(tab) = self.open else { return };
        let screen = ctx.content_rect();
        let bounds = Rect::from_min_max(
            Pos2::new(screen.left() + MARGIN, screen.top() + TOP_BAR_HEIGHT + MARGIN),
            Pos2::new(screen.right() - MARGIN, screen.bottom() - MARGIN),
        );
        let preferred = if tab == Tab::Research { RESEARCH_WINDOW_SIZE } else { WINDOW_SIZE };
        let outer = Vec2::new(preferred.x.min(bounds.width()), preferred.y.min(bounds.height()));
        // Frame margin (12) and stroke (2) on each side.
        let inner = outer - Vec2::splat(28.0);
        // Left of centre, so that the info panel (right) stays visible where there is room.
        let x = (bounds.left() + (bounds.width() - outer.x) * 0.25).max(bounds.left());
        let y = bounds.top() + ((bounds.height() - outer.y) * 0.3).max(0.0);
        let mut close = false;
        let mut nav = None;
        let context = TabContext { input, icons };
        let shown = egui::Window::new("Reichsverwaltung")
            .id(egui::Id::unique("hud_management"))
            .title_bar(false)
            .frame(theme::parchment_frame())
            .resizable(false)
            .collapsible(false)
            .constrain_to(bounds)
            .default_pos(Pos2::new(x, y))
            .fixed_size(inner)
            .show(ctx, |ui| {
                ui.set_width(inner.x);
                ui.set_height(inner.y);
                close = self.header(ui, &context, tab, actions);
                ui.add_space(2.0);
                let footer = 22.0;
                let height = (ui.available_height() - footer).max(80.0);
                egui::ScrollArea::vertical()
                    .id_salt(("management_scroll", tab))
                    .max_height(height)
                    .min_scrolled_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width() - 10.0);
                        match tab {
                            Tab::Economy => nav = self.economy.show(ui, &context, actions),
                            Tab::Storage => nav = self.storage.show(ui, &context, actions),
                            Tab::Production => self.production.show(ui, &context, actions),
                            Tab::Finance => self.finance.show(ui, &context, actions),
                            Tab::Trade => self.trade.show(ui, &context, actions),
                            Tab::Diplomacy => self.diplomacy.show(ui, &context, actions),
                            Tab::Military => nav = self.military.show(ui, &context, actions),
                            Tab::Workforce => self.workforce.show(ui, &context, actions),
                            Tab::Population => self.population.show(ui, &context, actions),
                            Tab::Agriculture => self.agriculture.show(ui, &context, actions),
                            Tab::Research => self.research.show(ui, &context, actions),
                            Tab::Statistics => self.statistics.show(ui, &context, actions),
                        }
                    });
                footer_line(ui, input);
            });
        if let Some(shown) = shown
            && self.raise_frames > 0
        {
            self.raise_frames -= 1;
            ctx.move_to_top(shown.response.layer_id);
        }
        if close {
            self.open = None;
            actions.push(HudAction::Sound("ui_close"));
        }
        match nav {
            Some(Navigate::ResourceStatistics(resource)) => {
                self.statistics.show_resource(resource);
                self.open = Some(Tab::Statistics);
            }
            Some(Navigate::Kingdom(id)) => {
                self.show_kingdom(id);
                actions.push(HudAction::Sound("ui_click"));
            }
            None => {}
        }
    }

    /// Title, close button and the tab strip; returns true if the close button was clicked.
    fn header(&mut self, ui: &mut Ui, context: &TabContext<'_, '_>, tab: Tab, actions: &mut Vec<HudAction>) -> bool {
        let mut close = false;
        ui.horizontal(|ui| {
            if let Some(icon) = context.icons.get(tab.icon()) {
                icon.show(ui, 30.0);
            }
            ui.label(RichText::new("Reichsverwaltung").font(heading_font(14.0)).color(Palette::INK_SOFT));
            ui.label(RichText::new(tab.title()).font(heading_font(22.0)).color(Palette::INK));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, context.icons.get("ui_close"), "×", false, 26.0)
                    .on_hover_text(format!("Schließen ({})", context.input.hint(Action::Cancel)))
                    .clicked()
                {
                    close = true;
                }
            });
        });
        ui.add_space(4.0);
        // Two rows of grouped tabs, all with their labels; a smaller font if a row needs it,
        // and only on very narrow windows the icons alone (the shown tab keeps its label).
        let room = ui.available_width();
        let size = if Tab::ROWS.iter().all(|row| row_width(ui, row, 15.0) <= room) { 15.0 } else { 13.0 };
        for row in Tab::ROWS {
            let compact = row_width(ui, row, size) > room;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for (i, group) in row.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(GROUP_GAP);
                    }
                    group_label(ui, group.name);
                    for &t in group.tabs {
                        let hint = context.input.hint(t.action());
                        let response = tab_button(ui, context.icons, t, t == tab, compact && t != tab, size);
                        if response.on_hover_text(format!("{} ({hint})", t.title())).clicked() && t != tab {
                            self.open = Some(t);
                            actions.push(HudAction::Sound("ui_click"));
                        }
                    }
                }
            });
            ui.add_space(2.0);
        }
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 2.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::ZERO, Palette::GOLD_DARK);
        close
    }
}

/// Space between two groups of a row.
const GROUP_GAP: f32 = 12.0;
/// Width of the group names before their tabs.
const GROUP_LABEL_WIDTH: f32 = 78.0;

/// The name of a tab group at the start of its tabs.
fn group_label(ui: &mut Ui, name: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(GROUP_LABEL_WIDTH, 30.0), Sense::hover());
    ui.painter().text(
        Pos2::new(rect.right() - 6.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        name,
        heading_font(13.0),
        Palette::INK_SOFT,
    );
}

/// Width of a row of tab groups with labels in `font_size`.
fn row_width(ui: &Ui, row: &[TabGroup], font_size: f32) -> f32 {
    let tabs: f32 = row.iter().flat_map(|g| g.tabs.iter()).map(|t| tab_width(ui, *t, false, font_size) + 3.0).sum();
    tabs + row.len() as f32 * (GROUP_LABEL_WIDTH + 3.0) + (row.len().saturating_sub(1)) as f32 * GROUP_GAP
}

/// Width of a tab of the strip with its label in `font_size` (`icon_only`: without label).
fn tab_width(ui: &Ui, tab: Tab, icon_only: bool, font_size: f32) -> f32 {
    if icon_only {
        return 34.0;
    }
    ui.painter().layout_no_wrap(tab.label().to_owned(), egui::FontId::proportional(font_size), Palette::INK).size().x + 36.0
}

/// One tab of the strip: icon and label (only the icon with `icon_only`); the selected tab is
/// light parchment with a gold rim.
fn tab_button(ui: &mut Ui, icons: &Icons, tab: Tab, selected: bool, icon_only: bool, font_size: f32) -> egui::Response {
    let color = if selected { Palette::INK } else { Palette::INK_SOFT };
    let label = if icon_only { String::new() } else { tab.label().to_owned() };
    let galley = ui.painter().layout_no_wrap(label, egui::FontId::proportional(font_size), color);
    let size = Vec2::new(tab_width(ui, tab, icon_only, font_size), 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let radius = CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 };
    let fill = if selected {
        Palette::PARCHMENT_LIGHT
    } else if response.hovered() {
        Palette::GOLD_BRIGHT.gamma_multiply(0.55)
    } else {
        Palette::PARCHMENT_DARK
    };
    let painter = ui.painter();
    painter.rect_filled(rect, radius, fill);
    let stroke = if selected { Stroke::new(1.5, Palette::GOLD_DARK) } else { Stroke::new(1.0, Palette::PARCHMENT_SHADE) };
    painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
    let icon_left = if icon_only { rect.center().x - 10.0 } else { rect.left() + 6.0 };
    let icon_rect = Rect::from_min_size(Pos2::new(icon_left, rect.center().y - 10.0), Vec2::splat(20.0));
    if let Some(icon) = icons.get(tab.icon()) {
        icon.paint_fit(painter, icon_rect, if selected { Color32::WHITE } else { Color32::from_white_alpha(200) });
    }
    painter.galley(Pos2::new(icon_rect.right() + 6.0, rect.center().y - galley.size().y / 2.0), galley, color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Bottom line: date of the shown data and commands waiting for the game to continue.
fn footer_line(ui: &mut Ui, input: &HudInput<'_>) {
    let state = input.session.state();
    let waiting = input.session.queued_commands().len();
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Stand: {}", state.clock().display())).small().color(Palette::INK_SOFT));
        if waiting > 0 && input.speed.is_paused() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let text = if waiting == 1 {
                    "1 Änderung wird übernommen, sobald das Spiel weiterläuft.".to_owned()
                } else {
                    format!("{waiting} Änderungen werden übernommen, sobald das Spiel weiterläuft.")
                };
                ui.label(RichText::new(text).small().color(Palette::WARN));
            });
        }
    });
}

/// Queues `command` with a click sound.
pub(super) fn command(actions: &mut Vec<HudAction>, command: Command) {
    actions.push(HudAction::Command(command));
    actions.push(HudAction::Sound("ui_click"));
}

/// Selects `building` (info panel) and moves the camera to it.
pub(super) fn select(actions: &mut Vec<HudAction>, building: BuildingId) {
    actions.push(HudAction::SelectBuilding(building));
    actions.push(HudAction::Sound("ui_select"));
}

/// Short axis label of a calendar day (0-based day number), e.g. „So 3“.
pub fn day_label(day: u64) -> String {
    let season = Season::from_index(day / DAYS_PER_SEASON);
    let short = match season {
        Season::Spring => "Fr",
        Season::Summer => "So",
        Season::Autumn => "He",
        Season::Winter => "Wi",
    };
    format!("{short} {}", day % DAYS_PER_SEASON + 1)
}

/// Full date of a calendar day, e.g. „Sommer · Tag 3 · Jahr 1“.
pub fn day_title(day: u64) -> String {
    kingdoms_sim::time::day_date_string(day)
}

/// Axis label of an hour since midnight of the first day („14 Uhr“).
pub fn hour_label(hour: u64) -> String {
    format!("{} Uhr", hour % HOURS_PER_DAY)
}

/// Full time of an hour, e.g. „Sommer · Tag 3 · 14:00“.
pub fn hour_title(hour: u64) -> String {
    let day = hour / HOURS_PER_DAY;
    format!("{} · Tag {} · {:02}:00", Season::from_index(day / DAYS_PER_SEASON).display_name(), day % DAYS_PER_SEASON + 1, hour % HOURS_PER_DAY)
}

/// A small inset card with a caption and a large value.
pub(super) fn stat_card(ui: &mut Ui, width: f32, caption: &str, value: &str, color: Color32) -> egui::Response {
    theme::inset_frame()
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            // Inner margin (2 × 8) and stroke (2 × 1).
            ui.set_width(width - 18.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(egui::Label::new(RichText::new(caption).small().color(Palette::INK_SOFT)).truncate());
                ui.label(RichText::new(value).font(egui::FontId::proportional(20.0)).strong().color(color));
            });
        })
        .response
}

/// A row of stat cards filling the width.
pub(super) fn card_row(ui: &mut Ui, cards: &[(&str, String, Color32)]) -> Vec<egui::Response> {
    let n = cards.len().max(1) as f32;
    let gap = 6.0;
    let width = ((ui.available_width() - gap * (n - 1.0)) / n).max(80.0);
    let mut responses = Vec::new();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (caption, value, color) in cards {
            responses.push(stat_card(ui, width, caption, value, *color));
        }
    });
    responses
}

/// A hint line in soft ink (empty states, explanations), left-aligned and ragged-right.
pub(super) fn hint(ui: &mut Ui, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text.into()).color(Palette::INK_SOFT).italics()).wrap());
}

/// `count` columns side by side like [`Ui::columns`], each filled top-down and left-aligned.
/// egui's own columns are laid out justified: their wrapped texts get stretched word spacing
/// and their buttons the full column width, unlike everything else in the window.
pub(super) fn columns<R>(ui: &mut Ui, count: usize, add: impl FnOnce(&mut [Ui]) -> R) -> R {
    let count = count.max(1);
    let gap = ui.spacing().item_spacing.x;
    let width = ((ui.available_width() - gap * (count as f32 - 1.0)) / count as f32).max(0.0);
    let top_left = ui.cursor().min;
    let bottom = ui.max_rect().bottom();
    let mut cols: Vec<Ui> = (0..count)
        .map(|i| {
            let left = top_left.x + i as f32 * (width + gap);
            let rect = Rect::from_min_max(Pos2::new(left, top_left.y), Pos2::new(left + width, bottom));
            let mut col = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
            col.set_width(width);
            col
        })
        .collect();
    let result = add(&mut cols);
    let widest = cols.iter().map(|c| c.min_rect().width()).fold(width, f32::max);
    let height = cols.iter().map(|c| c.min_size().y).fold(0.0, f32::max);
    let size = Vec2::new(ui.available_width().max(gap * (count as f32 - 1.0) + widest * count as f32), height);
    ui.advance_cursor_after_rect(Rect::from_min_size(top_left, size));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_sim::defs::Defs;

    /// The shipped game data.
    pub(super) fn test_defs() -> Defs {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => defs,
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    /// Texts of `add` laid out in a 600-point wide area: true for each justified one.
    fn justified_texts(add: impl Fn(&mut Ui)) -> Vec<bool> {
        let ctx = egui::Context::default();
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0))), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| add(ui));
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.job.justify),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hints_in_columns_are_not_justified() {
        let long = "Kein geöffnetes Gasthaus. Ein Gasthaus mit Wirt und Bier hebt Unterhaltung und Gemeinschaft.";
        // egui's columns justify wrapped texts (the stretched word spacing of the finding) …
        let egui_columns = justified_texts(|ui| {
            ui.columns(2, |cols| hint(&mut cols[1], long));
        });
        assert_eq!(egui_columns, vec![true]);
        // … the window's columns keep them ragged-right, like hints outside columns.
        let ours = justified_texts(|ui| {
            columns(ui, 2, |cols| {
                hint(&mut cols[0], long);
                hint(&mut cols[1], long);
            });
            hint(ui, long);
        });
        assert_eq!(ours, vec![false; 3]);
    }

    #[test]
    fn columns_share_the_width_and_advance_below_the_tallest() {
        let ctx = egui::Context::default();
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0))), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);
            let top = ui.cursor().min.y;
            let rects = columns(ui, 2, |cols| {
                cols[0].allocate_exact_size(Vec2::new(40.0, 30.0), Sense::hover());
                cols[1].allocate_exact_size(Vec2::new(40.0, 80.0), Sense::hover());
                [cols[0].min_rect(), cols[1].min_rect()]
            });
            assert!((rects[0].width() - rects[1].width()).abs() < 0.5, "equal widths");
            assert!((rects[1].left() - rects[0].right() - 10.0).abs() < 0.5, "one gap between");
            assert!((ui.cursor().min.y - (top + 80.0)).abs() < 0.5, "continues below the taller column");
        });
        output.textures_delta.clear();
    }

    #[test]
    fn tab_keys_round_trip() {
        for tab in Tab::ALL {
            assert_eq!(Tab::from_key(tab.key()), Some(tab));
        }
        assert_eq!(Tab::from_key(" Bevölkerung "), Some(Tab::Population));
        assert_eq!(Tab::from_key("FINANZEN"), Some(Tab::Finance));
        assert_eq!(Tab::from_key("schatz"), None);
        // Every tab opens with its own key.
        let actions: std::collections::BTreeSet<_> = Tab::ALL.iter().map(|t| t.action()).collect();
        assert_eq!(actions.len(), Tab::ALL.len());
        // The strip shows every tab exactly once, in its group.
        let shown: Vec<Tab> = Tab::ROWS.iter().flat_map(|row| row.iter()).flat_map(|g| g.tabs.iter().copied()).collect();
        assert_eq!(shown.len(), Tab::ALL.len());
        assert!(Tab::ALL.iter().all(|t| shown.contains(t)));
    }

    #[test]
    fn toggling_shows_one_tab_at_a_time() {
        let mut m = Management::default();
        assert_eq!(m.open_tab(), None);
        assert!(m.toggle(Tab::Finance));
        assert_eq!(m.open_tab(), Some(Tab::Finance));
        // Another tab's key switches the tab, the shown tab's key closes the window.
        assert!(m.toggle(Tab::Trade));
        assert_eq!(m.open_tab(), Some(Tab::Trade));
        assert!(!m.toggle(Tab::Trade));
        assert_eq!(m.open_tab(), None);
        assert!(!m.close());
        m.toggle(Tab::Storage);
        assert!(m.close());
        assert_eq!(m.open_tab(), None);
    }

    #[test]
    fn cache_refreshes_only_after_a_tick_and_the_interval() {
        let mut cache: Cached<u32> = Cached::default();
        assert!(cache.is_stale(0, 0.0));
        cache.data = Some(1);
        cache.tick = 10;
        cache.at = 5.0;
        assert!(!cache.is_stale(10, 100.0), "same tick: nothing changed");
        assert!(!cache.is_stale(11, 5.1), "too soon");
        assert!(cache.is_stale(11, 5.0 + REFRESH_SECONDS));
        cache.invalidate();
        assert!(cache.is_stale(10, 5.0));
    }

    #[test]
    fn calendar_labels() {
        assert_eq!(day_label(0), "Fr 1");
        assert_eq!(day_label(8), "So 3");
        assert_eq!(day_label(23), "Wi 6");
        assert_eq!(day_label(24), "Fr 1");
        assert_eq!(day_title(8), "Sommer · Tag 3 · Jahr 1");
        assert_eq!(day_title(24), "Frühling · Tag 1 · Jahr 2");
        assert_eq!(day_title(30), "Sommer · Tag 1 · Jahr 2");
        assert_eq!(hour_label(14), "14 Uhr");
        assert_eq!(hour_label(24 * 8 + 6), "6 Uhr");
        assert_eq!(hour_title(24 * 8 + 6), "Sommer · Tag 3 · 06:00");
    }
}
