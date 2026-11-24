//! Build menu (left): the vertical bar of the master design – Bauen, Straßen, Landwirtschaft,
//! Produktion, Militär, Forschung – and the flyout of the chosen category with an icon button per
//! building type or road type. Tooltips describe each type with its real data; types that need
//! research or reached their maximum count are disabled with the reason. „Forschung“ opens the
//! research tab of the management window. The Militär flyout starts with the army itself – the
//! military and diplomacy tabs with the current strength, companies and threats – and then
//! lists the military buildings to build.

use kingdoms_engine::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use kingdoms_sim::defs::{BuildingCategory, BuildingDef, BuildingRole, Defs, RoadDef};
use kingdoms_sim::state::GameState;

use super::format::{self, cost_items, decimal, hours, recipe_summary};
use super::management::Tab;
use super::top_bar::{ok_color, row, tooltip_title};
use super::{HudAction, HudInput, Icons, MARGIN, TOP_BAR_HEIGHT, ToolRequest};
use crate::actions::Action;
use crate::tools::Tool;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::icon_button;

/// Size of a button of the vertical bar.
const BAR_BUTTON: Vec2 = Vec2::new(76.0, 56.0);
/// Size of a building button in the flyout.
const BUTTON: f32 = 46.0;
/// Space between buttons.
const GAP: f32 = 5.0;
/// Building buttons per row of the flyout.
const PER_ROW: usize = 6;

/// A category of the build menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildTab {
    /// Homes and public buildings (storage, market, services).
    Build,
    /// Roads, walls and gates.
    Roads,
    Agriculture,
    Production,
    Military,
}

impl BuildTab {
    /// All categories in bar order.
    pub const ALL: [BuildTab; 5] = [BuildTab::Build, BuildTab::Roads, BuildTab::Agriculture, BuildTab::Production, BuildTab::Military];

    /// Label in the bar.
    pub const fn label(self) -> &'static str {
        match self {
            BuildTab::Build => "Bauen",
            BuildTab::Roads => "Straßen",
            BuildTab::Agriculture => "Landwirtschaft",
            BuildTab::Production => "Produktion",
            BuildTab::Military => "Militär",
        }
    }

    /// Title of the flyout.
    pub const fn title(self) -> &'static str {
        match self {
            BuildTab::Build => "Wohnen & öffentliche Gebäude",
            BuildTab::Roads => "Straßen & Mauern",
            BuildTab::Agriculture => "Landwirtschaft",
            BuildTab::Production => "Produktion",
            BuildTab::Military => "Militär",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            BuildTab::Build => "ui_build",
            BuildTab::Roads => "ui_roads",
            BuildTab::Agriculture => "ui_farm",
            BuildTab::Production => "ui_production",
            BuildTab::Military => "ui_military",
        }
    }

    /// Name used by the developer option `KINGDOMS_PANEL`.
    pub const fn key(self) -> &'static str {
        match self {
            BuildTab::Build => "bauen",
            BuildTab::Roads => "strassen",
            BuildTab::Agriculture => "landwirtschaft",
            BuildTab::Production => "produktion",
            BuildTab::Military => "militaer",
        }
    }

    /// Building categories shown in the flyout, each as a section.
    fn categories(self) -> &'static [BuildingCategory] {
        match self {
            BuildTab::Build => &[BuildingCategory::Housing, BuildingCategory::Public],
            BuildTab::Roads => &[],
            BuildTab::Agriculture => &[BuildingCategory::Agriculture],
            BuildTab::Production => &[BuildingCategory::Production],
            BuildTab::Military => &[BuildingCategory::Military],
        }
    }
}

/// The build menu: which flyout is open.
pub struct BuildMenu {
    open: Option<BuildTab>,
    /// The category `B` opens.
    last: BuildTab,
}

impl BuildMenu {
    pub fn new() -> Self {
        Self { open: None, last: BuildTab::Build }
    }

    /// Opens `tab`, or closes the flyout if `tab` is open; returns true if open afterwards.
    pub fn toggle(&mut self, tab: BuildTab) -> bool {
        self.open = if self.open == Some(tab) { None } else { Some(tab) };
        self.last = tab;
        self.open.is_some()
    }

    /// `B`: opens the last category or closes the flyout.
    pub fn toggle_last(&mut self) -> bool {
        self.toggle(self.open.unwrap_or(self.last))
    }

    /// Closes the flyout; returns true if it was open.
    pub fn close(&mut self) -> bool {
        self.open.take().is_some()
    }

    /// Draws the bar and the open flyout; returns the rectangle of the bar.
    pub fn show(&mut self, ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons, research_open: bool, actions: &mut Vec<HudAction>) -> Rect {
        let bar = egui::Area::new(egui::Id::unique("hud_left_bar"))
            .fixed_pos(Pos2::new(MARGIN, TOP_BAR_HEIGHT + MARGIN))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::wood_frame().inner_margin(egui::Margin::same(5)).show(ui, |ui| {
                    theme::on_wood(ui);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for tab in BuildTab::ALL {
                        let hint = if tab == self.last { format!(" ({})", input.hint(Action::ToggleBuildMenu)) } else { String::new() };
                        let response = bar_button(ui, icons, tab.icon(), tab.label(), self.open == Some(tab))
                            .on_hover_text(format!("{}{hint}", tab.title()));
                        if response.clicked() {
                            let open = self.toggle(tab);
                            actions.push(HudAction::Sound(if open { "ui_open" } else { "ui_close" }));
                        }
                    }
                    let response = bar_button(ui, icons, "ui_research", "Forschung", research_open)
                        .on_hover_text(format!("Forschung ({})", input.hint(Action::WindowResearch)));
                    if response.clicked() {
                        actions.push(HudAction::ToggleWindow(Tab::Research));
                    }
                });
            })
            .response
            .rect;
        if let Some(tab) = self.open
            && !self.flyout(ctx, input, icons, tab, bar, actions)
        {
            self.open = None;
            actions.push(HudAction::Sound("ui_close"));
        }
        bar
    }

    /// The flyout of `tab` right of the bar; returns false when its close button was clicked.
    fn flyout(&mut self, ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons, tab: BuildTab, bar: Rect, actions: &mut Vec<HudAction>) -> bool {
        let mut keep = true;
        egui::Area::new(egui::Id::unique("hud_build_flyout"))
            .fixed_pos(Pos2::new(bar.right() + 6.0, bar.top()))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::wood_frame().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                    theme::on_wood(ui);
                    let width = PER_ROW as f32 * (BUTTON + GAP) - GAP;
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tab.title()).font(heading_font(15.0)).color(Palette::GOLD_BRIGHT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, icons.get("ui_close"), "×", false, 22.0)
                                .on_hover_text(format!("Schließen ({})", input.hint(Action::Cancel)))
                                .clicked()
                            {
                                keep = false;
                            }
                        });
                    });
                    ui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
                    if tab == BuildTab::Military {
                        army_buttons(ui, input, icons, width, actions);
                        ui.label(RichText::new("Militärgebäude bauen").small().color(Palette::ON_WOOD_SOFT));
                    }
                    let categories = tab.categories();
                    let sections: Vec<(Option<&str>, Vec<Entry<'_>>)> = if tab == BuildTab::Roads {
                        vec![(None, road_entries(input))]
                    } else {
                        categories
                            .iter()
                            .map(|c| (if categories.len() > 1 { Some(category_title(*c)) } else { None }, category_entries(input, *c)))
                            .collect()
                    };
                    for (title, entries) in sections {
                        if let Some(title) = title {
                            ui.label(RichText::new(title).small().color(Palette::ON_WOOD_SOFT));
                        }
                        if entries.is_empty() {
                            ui.label(RichText::new("Keine Gebäude in dieser Kategorie.").small().color(Palette::ON_WOOD_SOFT));
                        }
                        // Fixed rows (an area's first layout pass has no width limit to wrap at).
                        for row in entries.chunks(PER_ROW) {
                            ui.horizontal(|ui| {
                                for entry in row {
                                    match entry {
                                        Entry::Building(def) => building_button(ui, input, icons, def, actions),
                                        Entry::Road(road) => road_button(ui, input, icons, road, actions),
                                    }
                                }
                            });
                        }
                    }
                    let hint = if tab == BuildTab::Roads {
                        "Klick wählt den Typ · mit gedrückter Maustaste ziehen · Rechtsklick/Esc: abbrechen".to_owned()
                    } else {
                        format!("Klick wählt das Gebäude · {}: drehen · Rechtsklick/Esc: abbrechen", input.hint(Action::Rotate))
                    };
                    ui.label(RichText::new(hint).small().color(Palette::ON_WOOD_SOFT));
                });
            });
        keep
    }
}

/// The two wide buttons at the top of the Militär flyout: the army (military tab) with soldiers,
/// companies and threats, and the diplomacy tab with wars and open proposals.
fn army_buttons(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, width: f32, actions: &mut Vec<HudAction>) {
    let state = input.session.state();
    let m = state.military();
    let soldiers = m.combatants().count();
    let threats = m.warbands().len();
    let wars = state.diplomacy().kingdoms().filter(|(_, k)| k.at_war()).count();
    let proposals = state.diplomacy().proposals().len();
    let army = format!(
        "{} · {}{}",
        format::count(u32::try_from(soldiers).unwrap_or(0), "Soldat", "Soldaten"),
        format::count(u32::try_from(m.companies().len()).unwrap_or(0), "Kompanie", "Kompanien"),
        if threats > 0 { format!(" · {} im Anmarsch", format::count(u32::try_from(threats).unwrap_or(0), "Feind-Trupp", "Feind-Trupps")) } else { String::new() }
    );
    let reich = match (wars, proposals) {
        (0, 0) => format!("{} Königreiche", state.diplomacy().len()),
        (w, p) => {
            let mut parts = Vec::new();
            if w > 0 {
                parts.push(format::count(u32::try_from(w).unwrap_or(0), "Krieg", "Kriege"));
            }
            if p > 0 {
                parts.push(format::count(u32::try_from(p).unwrap_or(0), "Anfrage", "Anfragen"));
            }
            parts.join(" · ")
        }
    };
    let entries = [
        ("ui_military", "Heer verwalten", army, Tab::Military, threats > 0),
        ("ui_diplomacy", "Diplomatie", reich, Tab::Diplomacy, wars > 0 || proposals > 0),
    ];
    for (icon, title, detail, tab, alert) in entries {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 44.0), Sense::click());
        let painter = ui.painter();
        let fill = if response.hovered() { Palette::WOOD_LIGHT } else { Palette::WOOD };
        painter.rect_filled(rect, CornerRadius::same(5), fill);
        let stroke = if alert { Stroke::new(1.5, Palette::BAD_LIGHT) } else { Stroke::new(1.0, Palette::GOLD_DARK) };
        painter.rect_stroke(rect, CornerRadius::same(5), stroke, egui::StrokeKind::Inside);
        if let Some(i) = icons.get(icon) {
            i.paint_fit(painter, Rect::from_min_size(rect.min + egui::vec2(6.0, 6.0), Vec2::splat(32.0)), Color32::WHITE);
        }
        painter.text(rect.min + egui::vec2(46.0, 5.0), egui::Align2::LEFT_TOP, title, heading_font(15.0), Palette::GOLD_BRIGHT);
        painter.text(rect.min + egui::vec2(46.0, 25.0), egui::Align2::LEFT_TOP, detail, FontId::proportional(12.0), if alert { Palette::BAD_LIGHT } else { Palette::ON_WOOD });
        if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("{} öffnen ({})", tab.title(), input.hint(tab.action()))).clicked() {
            actions.push(HudAction::OpenWindow(tab));
            actions.push(HudAction::Sound("ui_open"));
        }
    }
}

/// Section title of a building category in the flyout.
fn category_title(category: BuildingCategory) -> &'static str {
    match category {
        BuildingCategory::Housing => "Wohnhäuser",
        BuildingCategory::Public => "Öffentliche Gebäude und Lager",
        other => other.display_name(),
    }
}

/// A button of the vertical bar: icon above a label (shrunk to fit), gold when its flyout or
/// window is open.
pub(super) fn bar_button(ui: &mut egui::Ui, icons: &Icons, icon: &str, label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(BAR_BUTTON, Sense::click());
    let painter = ui.painter();
    let hovered = response.hovered();
    let fill = if selected {
        Palette::GOLD_DARK
    } else if hovered {
        Palette::WOOD_LIGHT
    } else {
        Palette::WOOD
    };
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    let stroke = if selected || hovered { Stroke::new(1.5, Palette::GOLD_BRIGHT) } else { Stroke::new(1.0, Palette::GOLD_DARK) };
    painter.rect_stroke(rect, CornerRadius::same(5), stroke, egui::StrokeKind::Inside);
    let icon_rect = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 19.0), Vec2::splat(28.0));
    if let Some(icon) = icons.get(icon) {
        icon.paint_fit(painter, icon_rect, Color32::WHITE);
    }
    let color = if selected { Palette::PARCHMENT_LIGHT } else { Palette::ON_WOOD };
    let galley = fitted_label(ui, label, rect.width() - 6.0, color);
    painter.galley(Pos2::new(rect.center().x - galley.size().x / 2.0, rect.bottom() - galley.size().y - 3.0), galley, color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `label` in the largest font size from 12 down to 9 points that fits `width`.
pub fn fitted_label(ui: &egui::Ui, label: &str, width: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    let mut size = 12.0;
    loop {
        let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(size), color);
        if galley.size().x <= width || size <= 9.0 {
            return galley;
        }
        size -= 0.5;
    }
}

/// Why a building type cannot be chosen now (`None` = available).
pub fn lock_reason(state: &GameState, defs: &Defs, def: &BuildingDef) -> Option<String> {
    if let Some(tech) = def.unlocked_by.filter(|_| !state.research().building_unlocked(defs, def.id)) {
        let name = defs.techs().get(tech.index()).map_or("?", |t| t.name.as_str());
        return Some(format!("Benötigt die Forschung „{name}“."));
    }
    if let Some(max) = def.max_count()
        && state.buildings().count_of(def.id) >= usize::from(max)
    {
        return Some(format!("Von „{}“ kann es höchstens {max} geben.", def.name));
    }
    None
}

/// First letter of a name (fallback label without icon).
fn initial(name: &str) -> String {
    name.chars().next().map(String::from).unwrap_or_default()
}

/// One button of the build menu.
enum Entry<'a> {
    Building(&'a BuildingDef),
    Road(&'a RoadDef),
}

/// Buildable types of a category (walls and gates are listed with the roads).
fn category_entries<'a>(input: &HudInput<'a>, category: BuildingCategory) -> Vec<Entry<'a>> {
    input.session.defs().buildings_in(category).filter(|d| d.buildable && !is_wall(d)).map(Entry::Building).collect()
}

/// Road types followed by walls and gates.
fn road_entries<'a>(input: &HudInput<'a>) -> Vec<Entry<'a>> {
    let defs = input.session.defs();
    defs.roads()
        .iter()
        .filter(|r| r.bridge.is_none())
        .map(Entry::Road)
        .chain(defs.buildings().iter().filter(|d| d.buildable && is_wall(d)).map(Entry::Building))
        .collect()
}

fn is_wall(def: &BuildingDef) -> bool {
    matches!(def.role, BuildingRole::WallSegment | BuildingRole::Gate)
}

fn building_button(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, def: &BuildingDef, actions: &mut Vec<HudAction>) {
    let state = input.session.state();
    let defs = input.session.defs();
    let locked = lock_reason(state, defs, def);
    let selected = input.tools.tool() == Tool::Build { kind: def.id };
    let response = ui
        .add_enabled_ui(locked.is_none(), |ui| icon_button(ui, icons.get(&format!("bld_{}", def.key)), &initial(&def.name), selected, BUTTON))
        .inner;
    let tooltip = |ui: &mut egui::Ui| building_tooltip(ui, input, icons, defs, state, def, locked.as_deref());
    let response = response.on_hover_ui(tooltip).on_disabled_hover_ui(tooltip);
    if response.clicked() {
        if selected {
            actions.push(HudAction::Tool(ToolRequest::Inspect));
        } else {
            actions.push(HudAction::Tool(ToolRequest::Build(def.id)));
        }
    }
}

/// Description of a building type with cost, staff, upkeep and production.
pub fn building_tooltip(
    ui: &mut egui::Ui,
    input: &HudInput<'_>,
    icons: &Icons,
    defs: &Defs,
    state: &GameState,
    def: &BuildingDef,
    locked: Option<&str>,
) {
    ui.set_max_width(320.0);
    tooltip_title(ui, &def.name);
    ui.label(RichText::new(&def.description).italics().color(Palette::INK_SOFT));
    if let Some(reason) = locked {
        ui.colored_label(Palette::BAD, reason);
    }
    ui.add_space(2.0);
    egui::Grid::new(("tt_bld", def.key.as_str())).num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
        row(ui, "Größe", format::size(def));
        ui.label(RichText::new("Kosten").color(Palette::INK_SOFT));
        cost_line(ui, icons, defs, state, &def.cost);
        ui.end_row();
        row(ui, "Bauzeit", hours(def.build_hours));
        if def.workers > 0 {
            let category = def.job.map_or("", |j| j.category.display_name());
            row(ui, "Arbeiter", format!("{} ({category})", def.workers));
        }
        if def.upkeep_per_day > 0 {
            row(ui, "Unterhalt", format!("{} Gold pro Tag", def.upkeep_per_day));
        }
        if let Some(housing) = def.housing.as_ref() {
            row(ui, "Wohnraum", format!("{} Bewohner (Stufe {})", housing.capacity, housing.tier));
        }
        if let Some(storage) = def.storage.as_ref() {
            row(ui, "Lager", format!("{} je Ware, Träger im Umkreis {}", storage.capacity_per_resource, storage.carrier_radius));
        }
        for service in &def.services {
            row(ui, service.kind.display_name(), format!("Umkreis {}", format::tiles(usize::from(service.radius))));
        }
        if def.territory_radius > 0 {
            row(ui, "Gebiet", format!("Umkreis {}", format::tiles(usize::from(def.territory_radius))));
        }
        if let Some(production) = def.production.as_ref() {
            for (i, recipe) in production.recipes.iter().enumerate() {
                row(ui, if i == 0 { "Produktion" } else { "" }, recipe_summary(defs, recipe));
            }
            if production.uses_tools {
                row(ui, "", "verbraucht Werkzeug");
            }
        }
    });
    ui.add_space(2.0);
    ui.label(
        RichText::new(format!(
            "Klick: platzieren · {}: drehen · Umschalt: mehrfach bauen · Rechtsklick/Esc: abbrechen",
            input.hint(Action::Rotate)
        ))
        .small()
        .color(Palette::INK_SOFT),
    );
}

/// Price with icons; missing parts in red.
pub fn cost_line(ui: &mut egui::Ui, icons: &Icons, defs: &Defs, state: &GameState, cost: &kingdoms_sim::defs::Cost) {
    let items = cost_items(defs, state.economy(), cost);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        if items.is_empty() {
            ui.label("kostenlos");
        }
        for item in &items {
            if let Some(icon) = icons.get(&item.icon) {
                icon.show(ui, 18.0).on_hover_text(&item.name);
            } else {
                ui.label(&item.name);
            }
            let text = if item.affordable() {
                item.needed.to_string()
            } else {
                format!("{} (hast {})", item.needed, item.available.max(0))
            };
            ui.colored_label(ok_color(item.affordable()), text);
            ui.add_space(4.0);
        }
    });
}

fn road_button(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, road: &RoadDef, actions: &mut Vec<HudAction>) {
    let state = input.session.state();
    let defs = input.session.defs();
    let unlocked = state.research().road_unlocked(defs, road.id);
    let selected = input.tools.tool() == Tool::Road { road: road.id };
    let response = ui
        .add_enabled_ui(unlocked, |ui| icon_button(ui, icons.get(&format!("road_{}", road.key)), &initial(&road.name), selected, BUTTON))
        .inner;
    let tooltip = |ui: &mut egui::Ui| road_tooltip(ui, icons, defs, state, road, unlocked);
    let response = response.on_hover_ui(tooltip).on_disabled_hover_ui(tooltip);
    if response.clicked() {
        actions.push(HudAction::Tool(if selected { ToolRequest::Inspect } else { ToolRequest::Road(road.id) }));
    }
}

fn road_tooltip(ui: &mut egui::Ui, icons: &Icons, defs: &Defs, state: &GameState, road: &RoadDef, unlocked: bool) {
    ui.set_max_width(300.0);
    tooltip_title(ui, &road.name);
    if !unlocked {
        let tech = road.unlocked_by.and_then(|t| defs.techs().get(t.index())).map_or("?", |t| t.name.as_str());
        ui.colored_label(Palette::BAD, format!("Benötigt die Forschung „{tech}“."));
    }
    egui::Grid::new(("tt_road", road.key.as_str())).num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
        ui.label(RichText::new("Kosten je Feld").color(Palette::INK_SOFT));
        cost_line(ui, icons, defs, state, &road.cost_per_tile);
        ui.end_row();
        row(ui, "Tempo", format!("{} ×", decimal(f64::from(road.speed), 2)));
        if road.trade_bonus_percent > 0 {
            row(ui, "Handel", format!("+{} %", road.trade_bonus_percent));
        }
    });
    if let Some(bridge) = defs.roads().iter().find_map(|r| r.bridge.map(|b| (r, b))) {
        ui.label(
            RichText::new(format!(
                "Über flaches Wasser entstehen automatisch Brücken (bis {}, {} Gold je Feld).",
                format::tiles(usize::from(bridge.1.max_span)),
                bridge.0.cost_per_tile.gold
            ))
            .small()
            .color(Palette::INK_SOFT),
        );
    }
    ui.label(RichText::new("Mit gedrückter linker Maustaste ziehen · Rechtsklick/Esc: abbrechen").small().color(Palette::INK_SOFT));
}
