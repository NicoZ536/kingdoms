//! Info panel (right, next to the right bar) of the selected building or tile, with the building's controls: status,
//! construction and upgrades, workers and priority, production (recipes, buffers, queue,
//! productivity factors), residents and needs of houses, storage contents, territory of the
//! headquarters, pause / move / demolish. Every button issues a simulation command.

use kingdoms_engine::egui::{self, Align, Align2, Layout, RichText};
use kingdoms_sim::buildings::{
    Building, BuildingId, BuildingPhase, BuildingStatus, ConstructionSite, DemolishRefund, SitePurpose, builders_wanted,
    demolish_refund, pause_allowed, upgrade_blockers, upgrade_service_gaps, upgrade_target,
};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingDef, BuildingRole, Defs, ResourceId};
use kingdoms_sim::policy::JobPriority;
use kingdoms_sim::population::{Heating, LifeStage, WealthClass};
use kingdoms_sim::production::ProductionStatus;
use kingdoms_sim::report::{self, BuildingEconomics, EconomicsBasis, HouseReport, MeasuredOutput, ProductionReport};
use kingdoms_sim::state::GameState;

use super::build_menu::cost_line;
use super::format::{self, decimal, int, recipe_summary, shift_note, tile_rows};
use super::top_bar::row;
use super::{HudAction, HudInput, Icons, MARGIN, TOP_BAR_HEIGHT, ToolRequest};
use crate::actions::Action;
use crate::tools::{DemolishRequest, movable, needs_confirmation};
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Mark, icon_button, mark, marked, mini_bar, progress_bar, rating_color, section_header};
use crate::view::picking::Selection;

const WIDTH: f32 = 340.0;
/// Space the minimap needs below the panel.
const MINIMAP_RESERVE: f32 = 172.0;

/// The info panel (stateless; it shows `HudInput::selection`).
pub struct InfoPanel;

impl InfoPanel {
    /// Draws the panel with its right edge at `right`; returns its rectangle if shown.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        input: &HudInput<'_>,
        icons: &Icons,
        right: f32,
        actions: &mut Vec<HudAction>,
        confirm: &mut Option<DemolishRequest>,
    ) -> Option<egui::Rect> {
        let selection = input.selection?;
        let screen = ctx.content_rect();
        let max_height = (screen.height() - TOP_BAR_HEIGHT - MINIMAP_RESERVE - 3.0 * MARGIN - 24.0).max(200.0);
        let shown = egui::Area::new(egui::Id::unique("hud_info_panel"))
            .anchor(Align2::RIGHT_TOP, egui::vec2(right - screen.right(), TOP_BAR_HEIGHT + MARGIN))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::parchment_frame().show(ui, |ui| {
                    ui.set_width(WIDTH);
                    // An area's content starts with last frame's size as its bounds: allow the
                    // panel to grow to its full height (before adding anything).
                    ui.set_max_height(max_height);
                    match selection {
                        Selection::Building(id) => {
                            if let Some(b) = input.session.state().building(id) {
                                building_panel(ui, input, icons, id, b, actions, confirm);
                            }
                        }
                        Selection::Tile(p) => tile_panel(ui, input, p, actions),
                    }
                });
            });
        Some(shown.response.rect)
    }
}

fn close_button(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, actions: &mut Vec<HudAction>) {
    if icon_button(ui, icons.get("ui_close"), "×", false, 26.0)
        .on_hover_text(format!("Schließen ({})", input.hint(Action::Cancel)))
        .clicked()
    {
        actions.push(HudAction::Deselect);
        actions.push(HudAction::Sound("ui_close"));
    }
}

fn tile_panel(ui: &mut egui::Ui, input: &HudInput<'_>, p: glam::IVec2, actions: &mut Vec<HudAction>) {
    let state = input.session.state();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Gelände").font(heading_font(19.0)).color(Palette::INK));
        ui.label(RichText::new(format!("Feld {}, {}", p.x, p.y)).small().color(Palette::INK_SOFT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Schließen").clicked() {
                actions.push(HudAction::Deselect);
                actions.push(HudAction::Sound("ui_close"));
            }
        });
    });
    section_header(ui, "Feld");
    egui::Grid::new("tile_rows").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
        for (label, value) in tile_rows(state, input.session.defs(), p) {
            row(ui, &label, value);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn building_panel(
    ui: &mut egui::Ui,
    input: &HudInput<'_>,
    icons: &Icons,
    id: BuildingId,
    b: &Building,
    actions: &mut Vec<HudAction>,
    confirm: &mut Option<DemolishRequest>,
) {
    let state = input.session.state();
    let defs = input.session.defs();
    let Some(def) = defs.buildings().get(b.kind().index()) else { return };
    let production = report::production_report(state, defs, id);
    let house = def.housing.as_ref().and_then(|_| report::house_report(state, defs, id));
    let farm = report::farm_report(state, defs, id).filter(|_| b.is_complete());
    // Header: icon, name, status, close.
    ui.horizontal(|ui| {
        if let Some(icon) = icons.get(&format!("bld_{}", def.key)) {
            icon.show(ui, 46.0);
        }
        ui.vertical(|ui| {
            ui.label(RichText::new(&def.name).font(heading_font(19.0)).color(Palette::INK));
            let (text, color) = status_line(state, defs, id, b, production.as_ref());
            ui.label(RichText::new(text).color(color));
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| close_button(ui, input, icons, actions));
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} · {} Felder", def.category.display_name(), super::format::size(def))).small().color(Palette::INK_SOFT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            controls(ui, input, icons, id, b, actions, confirm);
        });
    });
    let max_height = ui.available_height().max(120.0);
    egui::ScrollArea::vertical().max_height(max_height).auto_shrink([false, true]).show(ui, |ui| {
        ui.set_width(WIDTH - 4.0);
        match b.phase() {
            BuildingPhase::Construction(site) => construction_section(ui, input, icons, id, b, site, None, actions),
            BuildingPhase::Upgrading(up) => {
                let target = defs.buildings().get(up.target().index());
                construction_section(ui, input, icons, id, b, up.site(), target, actions);
            }
            BuildingPhase::Active => {}
        }
        if b.is_complete() && def.workers > 0 {
            workers_section(ui, id, b, def, production.is_some(), actions);
        }
        if b.is_complete() && !def.sells.is_empty() {
            shop_section(ui, input, icons, id);
        }
        if b.is_complete() && (!def.trains.is_empty() || def.garrison > 0 || def.defense.is_some()) {
            military_section(ui, input, id, def, actions);
        }
        if let Some(farm) = farm.as_ref() {
            super::farm_panel::farm_section(ui, defs, icons, state.clock().season(), id, farm, WIDTH - 24.0, actions);
        }
        if let Some(report) = production.as_ref().filter(|_| b.is_complete()) {
            production_section(ui, input, icons, id, b, def, report, actions);
        }
        if let Some(house) = house.as_ref() {
            house_section(ui, input, id, house, actions);
        }
        if def.storage.is_some() && b.is_complete() {
            storage_section(ui, input, icons, id);
        }
        if matches!(b.phase(), BuildingPhase::Active) && def.upgrade.is_some() {
            upgrade_section(ui, input, icons, id, b, def, house.as_ref(), actions);
        }
        if def.role == BuildingRole::Headquarters || def.territory_radius > 0 || !def.services.is_empty() {
            reach_section(ui, state, def);
        }
    });
}

/// Recruitment at a military building (units with costs, recruits in training) and its
/// defence (garrison, shots, hit points and damage).
fn military_section(ui: &mut egui::Ui, input: &HudInput<'_>, id: BuildingId, def: &BuildingDef, actions: &mut Vec<HudAction>) {
    let (state, defs) = (input.session.state(), input.session.defs());
    if def.garrison > 0 || def.defense.is_some() {
        section_header(ui, "Verteidigung");
        egui::Grid::new(("defense", id.index())).num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
            if def.garrison > 0 {
                row(ui, "Besatzung", format!("bis zu {}", format::count(def.garrison, "Soldat", "Soldaten")));
            }
            if let Some(d) = def.defense {
                if d.ranged > 0 {
                    row(ui, "Schüsse", format!("Fernkampf {} · Reichweite {}", d.ranged, format::tiles(usize::from(d.range))));
                }
                let damage = state.military().damage_of(id);
                let hp = if damage > 0 { format!("{} von {}", d.hit_points.saturating_sub(damage), d.hit_points) } else { d.hit_points.to_string() };
                row(ui, "Trefferpunkte", hp);
            }
        });
    }
    if def.trains.is_empty() {
        return;
    }
    section_header(ui, "Rekrutierung");
    let options = report::recruit_options(state, defs, id);
    super::management::military::recruit_rows(ui, defs, id, &options, actions);
    let training = super::management::military::training_at(state, defs, id);
    if training.is_empty() {
        ui.label(RichText::new("Niemand in Ausbildung.").small().italics().color(Palette::INK_SOFT));
    }
    for (name, unit, percent) in &training {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{name} ({unit})")).small().color(Palette::INK));
            mini_bar(ui, f32::from(*percent) / 100.0, Palette::INFO, 80.0);
            ui.label(RichText::new(format!("{percent} %")).small().color(Palette::INK_SOFT));
        });
    }
    if ui.small_button("Militärverwaltung öffnen").clicked() {
        actions.push(HudAction::OpenWindow(super::WindowTab::Military));
        actions.push(HudAction::Sound("ui_open"));
    }
}

fn status_line(state: &GameState, defs: &Defs, id: BuildingId, b: &Building, production: Option<&ProductionReport>) -> (String, egui::Color32) {
    let status = b.status();
    let base = status.display_name().to_owned();
    if status == BuildingStatus::Paused {
        return (base, Palette::WARN);
    }
    if !state.has_road_access(id) {
        return (format!("{base} · kein Straßenanschluss"), Palette::BAD);
    }
    if let Some(site) = b.construction_site() {
        let pct = (site.progress() * 100.0).round() as u32;
        if !site.can_progress() && !site.is_complete() {
            return (format!("{base} · {pct} % · wartet auf Baumaterial"), Palette::WARN);
        }
        if b.staffed() == 0 {
            return (format!("{base} · {pct} % · keine Bauarbeiter"), Palette::WARN);
        }
        return (format!("{base} · {pct} %"), Palette::INFO);
    }
    if let Some(p) = production {
        // Nobody arrives because the workers find no path: a problem, not a neutral wait.
        if p.present == 0 && p.commute.no_path > 0 {
            return ("Kein Weg zur Arbeit".to_owned(), Palette::BAD);
        }
        let color = match p.status {
            ProductionStatus::Working | ProductionStatus::Growing => Palette::GOOD,
            ProductionStatus::Idle
            | ProductionStatus::UnderConstruction
            | ProductionStatus::Upgrading
            | ProductionStatus::OutOfSeason
            | ProductionStatus::WorkersAway
            | ProductionStatus::LunchBreak
            | ProductionStatus::OffShift
            | ProductionStatus::NoPlantingSpace => Palette::INK_SOFT,
            _ => Palette::BAD,
        };
        return (p.status_text.clone(), color);
    }
    if let Some(def) = defs.buildings().get(b.kind().index())
        && def.workers > 0
        && b.staffed() == 0
    {
        return (format!("{base} · keine Arbeiter"), Palette::WARN);
    }
    (base, Palette::GOOD)
}

#[allow(clippy::too_many_arguments)]
fn construction_section(
    ui: &mut egui::Ui,
    input: &HudInput<'_>,
    icons: &Icons,
    id: BuildingId,
    b: &Building,
    site: &ConstructionSite,
    upgrade_to: Option<&BuildingDef>,
    actions: &mut Vec<HudAction>,
) {
    let defs = input.session.defs();
    let title = match (upgrade_to, site.purpose()) {
        (Some(target), _) => format!("Ausbau: {}", target.name),
        (None, SitePurpose::Move) => "Versetzen".to_owned(),
        (None, SitePurpose::New) => "Bau".to_owned(),
        (None, SitePurpose::Repair) => "Wiederaufbau".to_owned(),
    };
    section_header(ui, &title);
    let pct = site.progress();
    progress_bar(ui, pct, Palette::GOLD, &format!("{} % · Stufe {} von 3", (pct * 100.0).round(), site.stage() + 1));
    ui.add_space(2.0);
    let state = input.session.state();
    egui::Grid::new(("site", id)).num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
        let wanted = builders_wanted(b.rect());
        let present = kingdoms_sim::population::present_workers(state, defs, id);
        row(ui, "Bauarbeiter", format!("{} von {} ({present} an der Baustelle)", b.staffed(), wanted));
        if site.gold_paid() > 0 {
            row(ui, "Bezahlt", format!("{} Gold", site.gold_paid()));
        }
    });
    walking_notes(ui, state, id, None, true);
    if !site.materials_required().is_empty() {
        ui.label(RichText::new("Baumaterial (geliefert / benötigt)").small().color(Palette::INK_SOFT));
        theme::inset_frame().show(ui, |ui| {
            ui.set_width(WIDTH - 24.0);
            for m in site.materials_required() {
                let delivered = site.delivered(m.resource);
                resource_bar(ui, icons, defs, m.resource, delivered, m.amount, None);
            }
        });
    }
    ui.horizontal(|ui| {
        let label = if upgrade_to.is_some() { "Ausbau abbrechen" } else { "Bau abbrechen" };
        if (site.purpose() == SitePurpose::New || upgrade_to.is_some())
            && ui
                .button(label)
                .on_hover_text("Nicht verbautes Gold und Material wird vollständig erstattet.")
                .clicked()
        {
            actions.push(HudAction::Command(Command::CancelConstruction { building: id }));
            actions.push(HudAction::Sound("ui_click"));
        }
    });
}

/// One resource line: icon, name, bar of `amount` / `capacity`, optional incoming units.
fn resource_bar(ui: &mut egui::Ui, icons: &Icons, defs: &Defs, resource: ResourceId, amount: u32, capacity: u32, incoming: Option<u32>) {
    let Some(def) = defs.resources().get(resource.index()) else { return };
    ui.horizontal(|ui| {
        if let Some(icon) = icons.get(&format!("res_{}", def.key)) {
            icon.show(ui, 20.0);
        }
        ui.allocate_ui_with_layout(egui::vec2(86.0, 18.0), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(86.0);
            ui.add(egui::Label::new(RichText::new(&def.name).color(Palette::INK)).truncate());
        });
        let fraction = if capacity == 0 { 0.0 } else { amount as f32 / capacity as f32 };
        let mut text = format!("{amount} / {capacity}");
        if let Some(n) = incoming.filter(|n| *n > 0) {
            text.push_str(&format!(" (+{n} unterwegs)"));
        }
        let color = if capacity > 0 && amount >= capacity { Palette::GOOD } else { Palette::GOLD };
        progress_bar(ui, fraction, color, &text);
    });
}

fn workers_section(ui: &mut egui::Ui, id: BuildingId, b: &Building, def: &BuildingDef, has_production: bool, actions: &mut Vec<HudAction>) {
    section_header(ui, "Arbeiter");
    let staffed = b.staffed();
    let color = if staffed >= def.workers { Palette::GOOD } else if staffed > 0 { Palette::WARN } else { Palette::BAD };
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{staffed} von {}", def.workers)).color(color).strong());
        if let Some(job) = def.job {
            ui.label(RichText::new(format!("· {}", job.category.display_name())).color(Palette::INK_SOFT));
        }
    });
    if has_production || def.workers > 0 {
        priority_row(ui, id, b.priority(), actions);
    }
}

fn priority_row(ui: &mut egui::Ui, id: BuildingId, current: JobPriority, actions: &mut Vec<HudAction>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Priorität").color(Palette::INK_SOFT));
        for p in [JobPriority::Low, JobPriority::Normal, JobPriority::High] {
            if ui
                .selectable_label(current == p, p.display_name())
                .on_hover_text("Reihenfolge, in der Arbeiter und Träger Gebäude versorgen")
                .clicked()
                && current != p
            {
                actions.push(HudAction::Command(Command::SetProductionPriority { building: id, priority: p }));
                actions.push(HudAction::Sound("ui_click"));
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn production_section(
    ui: &mut egui::Ui,
    input: &HudInput<'_>,
    icons: &Icons,
    id: BuildingId,
    b: &Building,
    def: &BuildingDef,
    report: &ProductionReport,
    actions: &mut Vec<HudAction>,
) {
    let state = input.session.state();
    let defs = input.session.defs();
    section_header(ui, "Produktion");
    progress_bar(ui, report.progress, Palette::GOOD, &format!("Arbeitsgang {} %", (report.progress * 100.0).round()));
    ui.add_space(2.0);
    // Productivity and its factors.
    let factors = productivity_factors(state, defs, id, b, def, report);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Produktivität").color(Palette::INK_SOFT));
        ui.label(RichText::new(format!("{} %", report.productivity_percent)).strong().color(rating_color(report.productivity_percent as f32)));
        ui.label(RichText::new(format!("· {}", cycles_text(report.cycles_completed))).small().color(Palette::INK_SOFT));
    })
    .response
    .on_hover_ui(|ui| {
        ui.label("Produktivität des letzten Arbeitstakts. Faktoren:");
        egui::Grid::new("prod_factors").num_columns(2).show(ui, |ui| {
            for (label, value) in &factors {
                row(ui, label, value.clone());
            }
        });
    });
    egui::Grid::new(("factors", id)).num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
        for (label, value) in &factors {
            row(ui, label, value.clone());
        }
    });
    if !report.has_road_access {
        ui.colored_label(Palette::BAD, "Kein Straßenanschluss: Träger erreichen das Gebäude nicht.");
    } else if report.serving_storages == 0 {
        ui.colored_label(Palette::WARN, "Kein Lager in Trägerreichweite.");
    }
    walking_notes(ui, state, id, Some(report), true);
    // Recipes.
    let note = shift_note(defs);
    if report.recipes.len() > 1 {
        ui.label(RichText::new("Rezept (Dauer rund um die Uhr)").small().color(Palette::INK_SOFT)).on_hover_text(&note);
        for recipe in &report.recipes {
            let active = report.active_recipe == Some(recipe.index);
            let text = def
                .production
                .as_ref()
                .and_then(|p| p.recipes.get(usize::from(recipe.index)))
                .map_or_else(
                    || recipe.name.clone(),
                    // A recipe without goods already shows its name in the summary.
                    |r| if r.outputs.is_empty() { recipe_summary(defs, r) } else { format!("{}: {}", r.name, recipe_summary(defs, r)) },
                );
            if ui.selectable_label(active, text).on_hover_text(&note).clicked() && !active {
                actions.push(HudAction::Command(Command::SetRecipe { building: id, recipe: recipe.index }));
                actions.push(HudAction::Sound("ui_click"));
            }
        }
    } else if let Some(r) = def.production.as_ref().and_then(|p| p.recipes.first()) {
        ui.label(RichText::new(recipe_summary(defs, r)).color(Palette::INK)).on_hover_text(&note);
    }
    if let Some(e) = report::building_economics(state, defs, id) {
        economics_rows(ui, defs, &e);
    }
    // Buffers.
    if !report.input.is_empty() {
        ui.label(RichText::new("Eingangslager").small().color(Palette::INK_SOFT));
        theme::inset_frame().show(ui, |ui| {
            ui.set_width(WIDTH - 24.0);
            for slot in &report.input {
                resource_bar(ui, icons, defs, slot.resource, slot.amount, slot.capacity, Some(slot.incoming));
            }
        });
    }
    if !report.output.is_empty() {
        ui.label(RichText::new("Ausgangslager").small().color(Palette::INK_SOFT));
        theme::inset_frame().show(ui, |ui| {
            ui.set_width(WIDTH - 24.0);
            for slot in &report.output {
                resource_bar(ui, icons, defs, slot.resource, slot.amount, slot.capacity, None);
            }
        });
    }
    // Queue.
    if !report.recipes.is_empty() {
        ui.label(RichText::new("Aufträge").small().color(Palette::INK_SOFT));
        if report.queue.is_empty() {
            ui.label(RichText::new("Keine Aufträge – das Gebäude ruht.").color(Palette::WARN));
        }
        for (i, entry) in report.queue.iter().enumerate() {
            let name = report.recipes.get(usize::from(entry.recipe)).map_or("?", |r| r.name.as_str());
            let amount = entry.remaining.map_or_else(|| "dauerhaft".to_owned(), |n| format!("noch {n}"));
            ui.horizontal(|ui| {
                if i == 0 {
                    mark(ui, Mark::Next);
                } else {
                    ui.add_space(19.0);
                }
                ui.label(format!("{name}: {amount}"));
            });
        }
        let recipe = report.active_recipe.unwrap_or(0);
        ui.horizontal(|ui| {
            for n in [1u32, 5, 10] {
                if ui.button(format!("+{n}")).on_hover_text(format!("{} in Auftrag geben", format::count(n, "Einheit", "Einheiten"))).clicked() {
                    actions.push(HudAction::Command(Command::QueueProduction { building: id, recipe, units: Some(n) }));
                    actions.push(HudAction::Sound("ui_click"));
                }
            }
            if ui.button("Dauerhaft").on_hover_text("Ununterbrochen herstellen").clicked() {
                actions.push(HudAction::Command(Command::QueueProduction { building: id, recipe, units: None }));
                actions.push(HudAction::Sound("ui_click"));
            }
            if ui.add_enabled(!report.queue.is_empty(), egui::Button::new("Leeren")).clicked() {
                actions.push(HudAction::Command(Command::ClearQueue { building: id }));
                actions.push(HudAction::Sound("ui_click"));
            }
        });
    }
}

/// Production costs against the value of the output per day at today's prices
/// ([`report::building_economics`]): for buildings with recipes at the work rate of the
/// assigned crew, for field farms and orchards from their recent harvests; below it what was
/// actually made ("gemessen") against what is possible.
fn economics_rows(ui: &mut egui::Ui, defs: &Defs, e: &BuildingEconomics) {
    let gold = |milli: i64| format::signed_gold_milli(milli).trim_start_matches('+').to_owned();
    let harvest_days = match e.basis {
        EconomicsBasis::Crew => None,
        EconomicsBasis::Harvests { days } => Some(days),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("Wirtschaftlichkeit").color(Palette::INK_SOFT));
        if harvest_days == Some(0) {
            ui.label(RichText::new("noch nicht gemessen").color(Palette::INK_SOFT));
        } else {
            let color = if e.net_milli < 0 { Palette::BAD } else { Palette::GOOD };
            ui.label(RichText::new(format!("{} Gold/Tag", format::signed_gold_milli(e.net_milli))).strong().color(color));
        }
    })
    .response
    .on_hover_ui(|ui| {
        ui.label(economics_basis_text(e.basis));
        if harvest_days == Some(0) {
            return;
        }
        egui::Grid::new("economics_detail").num_columns(2).show(ui, |ui| {
            match harvest_days {
                None => {
                    row(ui, "Mögliche Arbeitsgänge", format::decimal(e.cycles_per_day_milli as f64 / 1000.0, 1));
                    row(ui, "Wert der Erzeugnisse", format!("{} Gold", gold(e.outputs_milli)));
                    row(ui, "Rohstoffe und Werkzeug", format!("−{} Gold", gold(e.inputs_milli)));
                }
                Some(_) => {
                    row(ui, "Abgeerntete Felder", format::decimal(e.cycles_per_day_milli as f64 / 1000.0, 1));
                    row(ui, "Wert der Ernte", format!("{} Gold", gold(e.outputs_milli)));
                }
            }
            row(ui, "Löhne", format!("−{} Gold", gold(e.wages_milli)));
            row(ui, "Unterhalt", format!("−{} Gold", gold(e.upkeep_milli)));
        });
    });
    ui.label(RichText::new(&e.text).small().color(Palette::INK_SOFT));
    if let Some(unit) = e.main_output
        && let Some(cost) = unit.cost_milli
        && let Some(name) = defs.resources().get(unit.resource.index()).map(|r| r.name.as_str())
    {
        let color = if cost > unit.price_milli { Palette::BAD } else { Palette::INK_SOFT };
        ui.label(
            RichText::new(format!(
                "Herstellkosten {name}: {} Gold je Einheit (Marktpreis {} Gold)",
                format::unit_gold_milli(cost),
                format::unit_gold_milli(unit.price_milli)
            ))
                .small()
                .color(color),
        );
    }
    measured_rows(ui, defs, &e.measured, harvest_days.is_some());
}

/// First line of the economics tooltip: what the figures are based on.
fn economics_basis_text(basis: EconomicsBasis) -> String {
    match basis {
        EconomicsBasis::Crew => "Je Tag mit der zugewiesenen Belegschaft, zu heutigen Marktpreisen:".to_owned(),
        EconomicsBasis::Harvests { days: 0 } => {
            "Die Ernte folgt den Jahreszeiten: Gerechnet wird mit den Ernten der abgeschlossenen Tage – noch ist keiner gemessen.".to_owned()
        }
        EconomicsBasis::Harvests { days: 1 } => "Je Tag nach den Ernten des letzten Tages, zu heutigen Marktpreisen:".to_owned(),
        EconomicsBasis::Harvests { days } => format!("Je Tag im Mittel der Ernten der letzten {days} Tage, zu heutigen Marktpreisen:"),
    }
}

/// What was made on the last closed day against what the crew could make in a whole day
/// (0 … ; `None` before the first midnight or while no work is possible).
pub fn measured_share(m: &MeasuredOutput) -> Option<f32> {
    let done = m.cycles_yesterday?;
    let possible = m.possible_cycles_milli.filter(|p| *p > 0)?;
    Some(done as f32 * 1000.0 / possible as f32)
}

/// Average output line of a workshop over its recorded days („Mittel der letzten 12 Tage: 9,5
/// Bretter je Tag“; `None` with fewer than two days or without a counted good).
pub fn average_output_text(defs: &Defs, m: &MeasuredOutput) -> Option<String> {
    let name = m.resource.and_then(|r| defs.resources().get(r.index())).map(|r| r.name.as_str())?;
    (m.days >= 2).then(|| format!("Mittel der letzten {} Tage: {} {name} je Tag", m.days, decimal(m.average_units_milli as f64 / 1000.0, 1)))
}

/// The measured output ("gemessen") of a building: yesterday's work against what is possible
/// with today's crew, the sentence of the simulation and the average of the recorded days.
fn measured_rows(ui: &mut egui::Ui, defs: &Defs, m: &MeasuredOutput, harvests: bool) {
    if !harvests && let Some(share) = measured_share(m) {
        let percent = (share * 100.0).round();
        progress_bar(ui, share.min(1.0), rating_color(percent), &format!("Gestern {percent} % des Möglichen"))
            .on_hover_text("Fertige Arbeitsgänge des letzten abgeschlossenen Tages gegen das, was die heutige Belegschaft an einem ganzen Tag schafft.");
    }
    ui.label(RichText::new(&m.text).small().color(Palette::INK_SOFT));
    if !harvests && let Some(text) = average_output_text(defs, m) {
        ui.label(RichText::new(text).small().color(Palette::INK_SOFT));
    }
}

/// Productivity factors as label/value rows (staff efficiency, tools, research, environment,
/// season/weather, bankruptcy), from the public state.
fn productivity_factors(
    state: &GameState,
    defs: &Defs,
    id: BuildingId,
    b: &Building,
    def: &BuildingDef,
    report: &ProductionReport,
) -> Vec<(String, String)> {
    let rules = defs.rules();
    let staff = kingdoms_sim::production::staffing_permille(state, defs, def, id);
    // Staffing counts only the workers at their place of work right now.
    let mut rows = vec![(
        "Belegschaft".to_owned(),
        format!("{} % ({} vor Ort, {} von {} besetzt)", staff / 10, report.present, b.staffed(), def.workers),
    )];
    rows.extend(training_row(report.training_percent));
    if report.uses_tools {
        let with_tools = b.production().and_then(|p| p.cycle()).map_or(report.tool_charge > 0, |c| c.with_tools);
        let value = match (with_tools, report.tool_charge) {
            (true, charge) => format!("100 % (noch {})", cycles_text(u64::from(charge))),
            (false, _) => format!("{} % (kein Werkzeug)", rules.no_tools_productivity_percent),
        };
        rows.push(("Werkzeug".to_owned(), value));
    }
    let research = state.research().production_bonus_percent(defs, def);
    if research != 0 {
        rows.push(("Forschung".to_owned(), format!("{} %", format::signed(i64::from(research)))));
    }
    if let Some(cycle) = b.production().and_then(|p| p.cycle()) {
        if cycle.environment_permille != 1000 {
            rows.push(("Umgebung".to_owned(), format!("{} %", decimal(f64::from(cycle.environment_permille) / 10.0, 1))));
        }
        if cycle.bonus_percent > 0 {
            rows.push(("Zusatzstoffe".to_owned(), format!("+{} %", cycle.bonus_percent)));
        }
    }
    let season = kingdoms_sim::production::season_weather_permille(state, defs, def);
    if season != 1000 {
        rows.push(("Jahreszeit/Wetter".to_owned(), format!("{} %", decimal(f64::from(season) / 10.0, 1))));
    }
    if report.event_modifier_percent != 0 {
        rows.push(("Ereignisse".to_owned(), format!("{} %", format::signed(i64::from(report.event_modifier_percent)))));
    }
    if state.economy().is_bankrupt() {
        rows.push(("Bankrott".to_owned(), format!("{} %", rules.bankruptcy_productivity_percent)));
    }
    rows
}

/// The „Einarbeitung“ row while new workers are still learning a skilled job (part of the
/// staffing factor, docs §6 Qualifikation); `None` for a fully trained crew.
fn training_row(training_percent: u32) -> Option<(String, String)> {
    (training_percent < 100).then(|| ("Einarbeitung".to_owned(), format!("{training_percent} % (in der Belegschaft enthalten)")))
}

fn house_section(ui: &mut egui::Ui, input: &HudInput<'_>, id: BuildingId, house: &HouseReport, actions: &mut Vec<HudAction>) {
    let state = input.session.state();
    let defs = input.session.defs();
    section_header(ui, "Bewohner");
    let residents = house.residents.len();
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{residents} von {}", house.capacity)).strong());
        ui.label(RichText::new(format!("· Stufe {}", house.tier)).color(Palette::INK_SOFT));
        ui.label(RichText::new(format!("· Zufriedenheit {}", house.happiness)).color(rating_color(f32::from(house.happiness))));
    });
    if !house.households.is_empty() {
        households_rows(ui, id, house);
    }
    let weather = state.weather();
    let cold = weather.needs_heating(defs.rules(), state.clock());
    if let Some(heating) = state.population().heating(id, cold) {
        let color = match heating {
            Heating::Burning => Palette::GOOD,
            Heating::OutOfFuel => Palette::BAD,
            Heating::Empty | Heating::NotNeeded => Palette::INK_SOFT,
        };
        ui.label(RichText::new(format!("Heizung: {}", heating.text())).small().color(color)).on_hover_text(format!(
            "Heute {} – geheizt wird unter {} °C, mit Brennholz oder Kohle aus dem Haushaltsvorrat.",
            weather.temperature_text(defs.rules(), state.clock()),
            format::int(i64::from(defs.rules().climate.temperature.heating_below_c))
        ));
    }
    if residents > 0 {
        theme::inset_frame().show(ui, |ui| {
            ui.set_width(WIDTH - 24.0);
            egui::Grid::new(("residents", id)).num_columns(3).spacing([10.0, 1.0]).striped(true).show(ui, |ui| {
                for r in house.residents.iter().take(12) {
                    if ui.link(RichText::new(&r.name).color(Palette::INFO)).on_hover_text("Bürger anzeigen").clicked() {
                        actions.push(HudAction::InspectCitizen(r.citizen));
                        actions.push(HudAction::Sound("ui_click"));
                    }
                    ui.label(RichText::new(format!("{} J.", r.age)).small().color(Palette::INK_SOFT));
                    let job = r.job.as_ref().map_or_else(
                        || match r.stage {
                            LifeStage::Child => "Kind".to_owned(),
                            LifeStage::Adult => "ohne Arbeit".to_owned(),
                            LifeStage::Elder => "im Ruhestand".to_owned(),
                        },
                        |j| j.title.clone(),
                    );
                    ui.label(RichText::new(job).small().color(Palette::INK_SOFT));
                    ui.end_row();
                }
            });
            if residents > 12 {
                ui.label(RichText::new(format!("… und {} weitere", residents - 12)).small());
            }
        });
    }
    if !house.needs.is_empty() && residents > 0 {
        ui.label(RichText::new("Bedürfnisse (Mittel der Bewohner)").small().color(Palette::INK_SOFT));
        egui::Grid::new(("needs", id)).num_columns(4).spacing([6.0, 3.0]).show(ui, |ui| {
            for (i, need) in house.needs.iter().enumerate() {
                ui.label(RichText::new(&need.label).small().color(Palette::INK));
                mini_bar(ui, f32::from(need.average) / 100.0, rating_color(f32::from(need.average)), 62.0)
                    .on_hover_text(format!("{}: {} von 100", need.label, need.average));
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    }
    if let Some(problem) = house.path_problem.as_ref() {
        ui.colored_label(Palette::BAD, problem);
    }
    if let Some(hours) = house.downgrade_in_hours {
        ui.colored_label(
            Palette::BAD,
            format!("Abstieg in {}, wenn die Voraussetzungen fehlen.", format::duration_minutes(u64::from(hours) * 60)),
        );
    }
}

/// The next level of any building with an upgrade target (houses: the next tier with its
/// requirements and the lock against automatic upgrades): what changes, cost, construction
/// work and the simulation's blockers ([`upgrade_blockers`]) as the disabled reason.
#[allow(clippy::too_many_arguments)]
fn upgrade_section(
    ui: &mut egui::Ui,
    input: &HudInput<'_>,
    icons: &Icons,
    id: BuildingId,
    b: &Building,
    def: &BuildingDef,
    house: Option<&HouseReport>,
    actions: &mut Vec<HudAction>,
) {
    let (state, defs) = (input.session.state(), input.session.defs());
    let Some((upgrade, target)) = upgrade_target(defs, def.id) else { return };
    section_header(ui, &format!("Ausbau: {}", target.name));
    if def.housing.is_none() && !target.description.is_empty() {
        ui.label(RichText::new(&target.description).small().color(Palette::INK_SOFT));
    }
    if let Some(next) = house.and_then(|h| h.next.as_ref()) {
        let threshold = defs.rules().population.need_met_threshold;
        for need in &next.needs {
            requirement(ui, need.met, &format!("{} ≥ {threshold} (jetzt {})", need.need.display_name(), need.average));
        }
        for s in &next.services {
            requirement(ui, s.met, &format!("Versorgung: {}", s.what.display_name()));
        }
        for g in &next.goods {
            let names: Vec<&str> = g.what.iter().filter_map(|r| defs.resources().get(r.index()).map(|d| d.name.as_str())).collect();
            requirement(ui, g.met, &format!("Waren: {}", names.join(" oder ")));
        }
    }
    egui::Grid::new(("upgrade_cost", id)).num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
        ui.label(RichText::new("Kosten").color(Palette::INK_SOFT));
        cost_line(ui, icons, defs, state, &upgrade.cost);
        ui.end_row();
        row(ui, "Bauzeit", format!("{} (ein Bauarbeiter)", format::hours(upgrade.hours)));
        // The house's living space includes research bonuses; the next tier gets the same.
        let bonus = house.map_or(0, |h| h.capacity.saturating_sub(def.housing.as_ref().map_or(0, |d| d.capacity)));
        for (label, value) in upgrade_changes(def, target, bonus) {
            row(ui, &label, value);
        }
    });
    // Houses that only this building serves go without the service while it is rebuilt (the
    // only market: hunger).
    if !def.services.is_empty() {
        for gap in upgrade_service_gaps(state, defs, id) {
            ui.label(RichText::new(gap.text()).small().color(Palette::WARN));
        }
    }
    let blockers = upgrade_blockers(state, defs, id).unwrap_or_default();
    ui.horizontal(|ui| {
        let response = ui.add_enabled(blockers.is_empty(), egui::Button::new(RichText::new("Ausbauen").strong()));
        let response = if blockers.is_empty() {
            let note = if def.housing.is_some() {
                "Das Haus wird zur nächsten Wohnstufe ausgebaut; die Bewohner bleiben."
            } else {
                "Bestand, Aufträge und Arbeiter bleiben; während des Ausbaus ruht der Betrieb."
            };
            response.on_hover_text(note)
        } else {
            let text = blockers.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
            response.on_disabled_hover_text(text)
        };
        if response.clicked() {
            actions.push(HudAction::Command(Command::UpgradeBuilding { building: id }));
            actions.push(HudAction::Sound("ui_click"));
        }
        // Only houses upgrade on their own.
        if def.housing.is_some() {
            let mut locked = b.is_upgrade_locked();
            if ui
                .checkbox(&mut locked, "Ausbau sperren")
                .on_hover_text("Verhindert den automatischen Ausbau dieses Hauses.")
                .changed()
            {
                actions.push(HudAction::Command(Command::SetUpgradeLocked { building: id, locked }));
                actions.push(HudAction::Sound("ui_click"));
            }
        }
    });
    if let Some(first) = blockers.first() {
        ui.label(RichText::new(first_blocker_text(&first.to_string(), blockers.len() - 1)).small().color(Palette::BAD));
    }
}

/// The first reason against an upgrade and how many more there are („Nicht genug Stein (0 von
/// 30) – und ein weiterer Grund.“; all of them are the button's tooltip).
fn first_blocker_text(first: &str, more: usize) -> String {
    let first = first.trim_end_matches('.');
    match more {
        0 => format!("{first}."),
        1 => format!("{first} – und ein weiterer Grund."),
        n => format!("{first} – und {n} weitere Gründe."),
    }
}

/// What an upgrade from `from` to `to` changes, as German label/value rows
/// ("Arbeiter", "2 → 3"); values that stay the same are left out. `housing_bonus`: extra
/// living space of the house from research (kept by the next tier).
pub fn upgrade_changes(from: &BuildingDef, to: &BuildingDef, housing_bonus: u16) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut change = |label: &str, a: String, b: String| {
        if a != b {
            rows.push((label.to_owned(), format!("{a} → {b}")));
        }
    };
    if from.size != to.size {
        change("Grundfläche", format::size(from), format::size(to));
    }
    let housing = |d: &BuildingDef| d.housing.as_ref().map_or(0, |h| h.capacity + housing_bonus);
    if from.housing.is_some() || to.housing.is_some() {
        change("Wohnraum", housing(from).to_string(), housing(to).to_string());
    }
    change("Arbeiter", from.workers.to_string(), to.workers.to_string());
    let buffer = |d: &BuildingDef| d.production.as_ref().map_or(0, |p| p.buffer_capacity);
    if from.production.is_some() && to.production.is_some() {
        change("Puffer je Ware", buffer(from).to_string(), buffer(to).to_string());
    }
    let storage = |d: &BuildingDef| d.storage.as_ref().map_or(0, |s| s.capacity_per_resource);
    if from.storage.is_some() || to.storage.is_some() {
        change("Lagerplatz je Ware", int(i64::from(storage(from))), int(i64::from(storage(to))));
    }
    let carriers = |d: &BuildingDef| d.storage.as_ref().map_or(0, |s| s.carrier_radius);
    if from.storage.is_some() && to.storage.is_some() {
        change("Trägerreichweite", format::tiles(usize::from(carriers(from))), format::tiles(usize::from(carriers(to))));
    }
    if from.garrison > 0 || to.garrison > 0 {
        change("Besatzung", format::count(u64::from(from.garrison), "Soldat", "Soldaten"), format::count(u64::from(to.garrison), "Soldat", "Soldaten"));
    }
    let defense = |d: &BuildingDef| d.defense.unwrap_or_default();
    if from.defense.is_some() || to.defense.is_some() {
        let (a, b) = (defense(from), defense(to));
        change("Fernkampf", a.ranged.to_string(), b.ranged.to_string());
        change("Reichweite", format::tiles(usize::from(a.range)), format::tiles(usize::from(b.range)));
        change("Trefferpunkte", int(i64::from(a.hit_points)), int(i64::from(b.hit_points)));
    }
    if from.territory_radius > 0 || to.territory_radius > 0 {
        change("Gebietsradius", format::tiles(usize::from(from.territory_radius)), format::tiles(usize::from(to.territory_radius)));
    }
    for s in &to.services {
        let before = from.service_radius(s.kind).map_or_else(|| "–".to_owned(), |r| format!("Umkreis {r}"));
        change(s.kind.display_name(), before, format!("Umkreis {}", s.radius));
    }
    if from.research_points_per_hour > 0.0 || to.research_points_per_hour > 0.0 {
        change(
            "Forschung",
            format!("{} Punkte/h", decimal(f64::from(from.research_points_per_hour), 1)),
            format!("{} Punkte/h", decimal(f64::from(to.research_points_per_hour), 1)),
        );
    }
    change("Unterhalt", format!("{} Gold/Tag", from.upkeep_per_day), format!("{} Gold/Tag", to.upkeep_per_day));
    rows
}

/// The households of a house: family, members, savings and wealth class, with yesterday's
/// income and spending in the tooltip and as a total below.
fn households_rows(ui: &mut egui::Ui, id: BuildingId, house: &HouseReport) {
    ui.label(RichText::new("Haushalte (Ersparnisse und Wohlstand)").small().color(Palette::INK_SOFT));
    egui::Grid::new(("households", id)).num_columns(4).spacing([10.0, 1.0]).striped(true).show(ui, |ui| {
        for h in &house.households {
            let tip = household_tooltip(h);
            ui.label(RichText::new(format!("Familie {}", h.family_name)).small().color(Palette::INK)).on_hover_text(&tip);
            ui.label(RichText::new(format::count(h.members, "Person", "Personen")).small().color(Palette::INK_SOFT));
            ui.label(RichText::new(format!("{} Gold", int(h.savings))).small().color(Palette::INK)).on_hover_text(&tip);
            ui.label(RichText::new(&h.wealth_label).small().color(wealth_color(h.wealth))).on_hover_text(&tip);
            ui.end_row();
        }
    });
    let earned: i64 = house.households.iter().map(|h| h.earned_yesterday).sum();
    let spent: i64 = house.households.iter().map(|h| h.spent_yesterday).sum();
    ui.label(RichText::new(yesterday_text(earned, spent)).small().color(Palette::INK_SOFT))
        .on_hover_text("Einkommen aus Arbeit, Lohn oder Sold; Ausgaben für Steuern, Marktgebühren und Einkäufe zum Marktpreis.");
}

/// „Gestern: 12 Gold verdient, 9 Gold ausgegeben“.
pub fn yesterday_text(earned: i64, spent: i64) -> String {
    format!("Gestern: {} Gold verdient, {} Gold ausgegeben", int(earned), int(spent))
}

fn household_tooltip(h: &kingdoms_sim::report::HouseholdRow) -> String {
    let mut text = format!(
        "Familie {} · {}\nErsparnisse: {} Gold ({})\n{}",
        h.family_name,
        format::count(h.members, "Person", "Personen"),
        int(h.savings),
        h.wealth_label,
        yesterday_text(h.earned_yesterday, h.spent_yesterday)
    );
    if h.unhappy_days > 0 {
        text.push_str(&format!("\nUnzufrieden seit {}", format::count(h.unhappy_days, "Tag", "Tagen")));
    }
    text
}

/// Colour of a wealth class: poor red, wealthy green.
pub fn wealth_color(class: WealthClass) -> egui::Color32 {
    match class {
        WealthClass::Poor => Palette::BAD,
        WealthClass::Modest => Palette::INK,
        WealthClass::Wealthy => Palette::GOOD,
    }
}

fn requirement(ui: &mut egui::Ui, met: bool, text: &str) {
    let kind = if met { Mark::Ok } else { Mark::Error };
    marked(ui, kind, RichText::new(text).color(if met { Palette::INK } else { Palette::INK_SOFT }));
}

fn storage_section(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, id: BuildingId) {
    let state = input.session.state();
    let defs = input.session.defs();
    let Some(storage) = report::storage_report(state, defs).into_iter().find(|s| s.building == id) else { return };
    section_header(ui, "Lager");
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "Träger {} von {} unterwegs, {} frei",
                storage.busy_carriers, storage.carriers, storage.idle_carriers
            ))
            .color(Palette::INK_SOFT),
        );
        if !storage.reachable {
            ui.colored_label(Palette::BAD, "· nicht erreichbar");
        }
    });
    if storage.waiting_jobs > 0 {
        let color = if storage.carriers == 0 { Palette::BAD } else { Palette::WARN };
        ui.colored_label(color, storage_wait_text(state, id, storage.waiting_jobs));
    }
    // The line above already names this storage's waiting transports (with their age).
    walking_notes(ui, state, id, None, storage.waiting_jobs == 0);
    let filled: Vec<_> = storage.contents.iter().filter(|s| s.amount > 0 || s.incoming > 0).collect();
    if filled.is_empty() {
        ui.label(RichText::new("Leer").color(Palette::INK_SOFT));
        return;
    }
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(WIDTH - 24.0);
        for slot in filled {
            resource_bar(ui, icons, defs, slot.resource, slot.amount, slot.capacity, Some(slot.incoming));
        }
    });
}

/// Goods a selling building (tavern) keeps for its guests ([`report::shop_report`]).
fn shop_section(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, id: BuildingId) {
    let (state, defs) = (input.session.state(), input.session.defs());
    let Some(shop) = report::shop_report(state, defs, id) else { return };
    section_header(ui, "Ausschank");
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(WIDTH - 24.0);
        for good in &shop.goods {
            resource_bar(ui, icons, defs, good.resource, good.stock, good.capacity, None);
        }
    });
    match shop.problem.as_ref() {
        Some(problem) => {
            ui.colored_label(Palette::BAD, problem);
        }
        None => {
            ui.label(
                RichText::new("Träger füllen den Vorrat aus den Lagern auf; die Gäste kaufen abends für ihren Haushalt.")
                    .small()
                    .color(Palette::INK_SOFT),
            );
        }
    }
}

/// Walking-related lines of a building: the workers' commute (production), transports waiting
/// for a carrier and citizens without a path there.
/// „3 Transporte warten auf einen Träger dieses Lagers (seit 2 h 10 min).“ for the `jobs`
/// waiting jobs of storage `id`'s carriers (the age of the oldest from the public job queue).
fn storage_wait_text(state: &GameState, id: BuildingId, jobs: u32) -> String {
    let what = if jobs == 1 { "1 Transport wartet".to_owned() } else { format!("{jobs} Transporte warten") };
    let oldest = state.logistics().waiting().filter(|j| j.carrier_base == id).map(|j| j.created_at).min();
    match oldest {
        Some(created) => {
            let minutes = report::ticks_to_minutes(state.clock().tick.saturating_sub(created));
            format!("{what} auf einen Träger dieses Lagers (seit {}).", report::minutes_text(minutes))
        }
        None => format!("{what} auf einen Träger dieses Lagers."),
    }
}

/// Commute, transports waiting for a carrier (unless `carrier_wait` is false) and path
/// problems of building `id`.
fn walking_notes(ui: &mut egui::Ui, state: &GameState, id: BuildingId, production: Option<&ProductionReport>, carrier_wait: bool) {
    if let Some(p) = production.filter(|p| p.commute.assigned > 0) {
        let c = &p.commute;
        let mut details = format!("Zugeteilt {} · vor Ort {} · unterwegs {}", c.assigned, c.present, c.walking);
        if c.no_path > 0 {
            details.push_str(&format!(" · ohne Weg {}", c.no_path));
        }
        if let Some(longest) = c.longest_minutes {
            details.push_str(&format!("\nLängster Arbeitsweg zuletzt: {}", format::duration_minutes(u64::from(longest))));
        }
        details.push_str("\nDie Arbeit beginnt erst bei der Ankunft; der Weg kostet Schichtzeit.");
        let color = if c.lost_percent >= 25 { Palette::WARN } else { Palette::INK_SOFT };
        ui.label(RichText::new(&c.text).color(color)).on_hover_text(details);
    }
    let wait = production.map_or_else(|| report::carrier_wait(state, id), |p| p.carrier_wait.clone());
    if let Some(wait) = wait.filter(|_| carrier_wait) {
        let color = if wait.oldest_minutes >= 120 { Palette::BAD } else { Palette::WARN };
        ui.colored_label(color, wait.text);
    }
    if let Some(problem) = report::path_problem(state, id) {
        ui.colored_label(Palette::BAD, problem);
    }
}

fn reach_section(ui: &mut egui::Ui, state: &GameState, def: &BuildingDef) {
    section_header(ui, "Wirkung");
    egui::Grid::new(("reach", def.key.as_str())).num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
        if def.territory_radius > 0 {
            row(ui, "Gebietsradius", format::tiles(usize::from(def.territory_radius)));
            row(ui, "Eigenes Gebiet", format!("{} Felder", int(i64::from(state.territory().owned_tiles()))));
        }
        for s in &def.services {
            row(ui, s.kind.display_name(), format!("Umkreis {}", format::tiles(usize::from(s.radius))));
        }
    });
}

/// Pause, move and demolish as icon buttons (laid out right to left).
fn controls(
    ui: &mut egui::Ui,
    input: &HudInput<'_>,
    icons: &Icons,
    id: BuildingId,
    b: &Building,
    actions: &mut Vec<HudAction>,
    confirm: &mut Option<DemolishRequest>,
) {
    let state = input.session.state();
    let defs = input.session.defs();
    ui.spacing_mut().item_spacing.x = 4.0;
    // Demolish (or cancel a new site).
    let refund = demolish_refund(state, defs, id);
    let cancels = refund.as_ref().is_ok_and(|r| r.cancels_site);
    let response = icon_button(ui, icons.get("ui_demolish"), "X", false, 28.0).on_hover_ui(|ui| {
        ui.label(RichText::new(if cancels { "Bau abbrechen" } else { "Abreißen" }).strong());
        match refund.as_ref() {
            Ok(refund) => refund_text(ui, icons, defs, refund),
            Err(reason) => {
                ui.colored_label(Palette::BAD, reason.to_string());
            }
        }
    });
    if response.clicked() {
        if needs_confirmation(state, defs, id) {
            *confirm = Some(DemolishRequest { building: id });
            actions.push(HudAction::Sound("ui_open"));
        } else {
            actions.push(HudAction::Command(Command::DemolishBuilding { building: id }));
            actions.push(HudAction::Deselect);
        }
    }
    // Move.
    let move_check = movable(state, defs, id);
    let response = ui.add_enabled_ui(move_check.is_ok(), |ui| icon_button(ui, icons.get("ui_move"), "M", false, 28.0)).inner;
    let response = match &move_check {
        Ok(()) => response.on_hover_text(format!("Versetzen ({}): an einem neuen Ort aufstellen", input.hint(Action::MoveTool))),
        Err(reason) => response.on_disabled_hover_text(format!("Versetzen: {reason}")),
    };
    if response.clicked() {
        actions.push(HudAction::Tool(ToolRequest::Move(Some(id))));
    }
    // Pause / resume (the simulation's rule: finished houses cannot be paused).
    if b.is_paused() || pause_allowed(state, defs, id).is_ok() {
        let paused = b.is_paused();
        let (icon, text) = if paused {
            ("ui_play", "Fortsetzen: Arbeit wieder aufnehmen")
        } else {
            ("ui_pause", "Stilllegen: Arbeiter und Bauarbeiter abziehen")
        };
        if icon_button(ui, icons.get(icon), "II", paused, 28.0).on_hover_text(text).clicked() {
            actions.push(HudAction::Command(Command::SetBuildingPaused { building: id, paused: !paused }));
            actions.push(HudAction::Sound(if paused { "ui_click" } else { "ui_pause" }));
        }
    }
}

/// What demolishing returns.
pub fn refund_text(ui: &mut egui::Ui, icons: &Icons, defs: &Defs, refund: &DemolishRefund) {
    let title = if refund.cancels_site { "Baustelle abbrechen – Erstattung:" } else { "Abriss – Erstattung:" };
    ui.label(RichText::new(title).color(Palette::INK_SOFT));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        if refund.gold > 0 {
            if let Some(icon) = icons.get("ui_gold") {
                icon.show(ui, 18.0);
            }
            ui.label(format!("{} ", refund.gold));
        }
        for m in &refund.materials {
            let Some(def) = defs.resources().get(m.resource.index()) else { continue };
            if let Some(icon) = icons.get(&format!("res_{}", def.key)) {
                icon.show(ui, 18.0).on_hover_text(&def.name);
            } else {
                ui.label(&def.name);
            }
            ui.label(format!("{} ", m.amount));
        }
        if refund.gold == 0 && refund.materials.is_empty() {
            ui.label("nichts");
        }
    });
}

/// A number of work cycles („1 Arbeitsgang“, „12 Arbeitsgänge“).
fn cycles_text(n: u64) -> String {
    format!("{} {}", int(i64::try_from(n).unwrap_or(i64::MAX)), if n == 1 { "Arbeitsgang" } else { "Arbeitsgänge" })
}

#[cfg(test)]
mod tests {
    use kingdoms_sim::defs::Defs;
    use kingdoms_sim::population::WealthClass;

    use super::*;

    fn defs() -> Defs {
        match Defs::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data")) {
            Ok(defs) => defs,
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    #[test]
    fn work_cycles_agree_with_their_number() {
        assert_eq!(cycles_text(1), "1 Arbeitsgang");
        assert_eq!(cycles_text(0), "0 Arbeitsgänge");
        assert_eq!(cycles_text(1200), "1.200 Arbeitsgänge");
    }

    fn def<'a>(defs: &'a Defs, key: &str) -> &'a BuildingDef {
        match defs.building_by_key(key) {
            Some(d) => d,
            None => panic!("no building {key}"),
        }
    }

    #[test]
    fn upgrade_changes_name_only_what_changes() {
        let defs = defs();
        let rows = upgrade_changes(def(&defs, "sawmill"), def(&defs, "large_sawmill"), 0);
        let get = |label: &str| rows.iter().find(|(l, _)| l == label).map(|(_, v)| v.as_str());
        assert_eq!(get("Arbeiter"), Some("2 → 3"));
        assert!(get("Puffer je Ware").is_some_and(|v| v.contains('→')), "{rows:?}");
        assert!(get("Grundfläche").is_none(), "same footprint: {rows:?}");
        assert!(get("Wohnraum").is_none());
        // Every row really changes something.
        assert!(rows.iter().all(|(_, v)| v.split(" → ").collect::<Vec<_>>().windows(2).all(|w| w[0] != w[1])), "{rows:?}");
        // Houses compare living space, towers their defence, storages their space.
        let house = upgrade_changes(def(&defs, "house_wood"), def(&defs, "farmhouse"), 1);
        let space = |d: &BuildingDef| d.housing.as_ref().map_or(0, |h| h.capacity);
        let expected = format!("{} → {}", space(def(&defs, "house_wood")) + 1, space(def(&defs, "farmhouse")) + 1);
        assert!(house.iter().any(|(l, v)| l == "Wohnraum" && *v == expected), "{house:?}");
        let tower = upgrade_changes(def(&defs, "watchtower"), def(&defs, "stone_tower"), 0);
        assert!(tower.iter().any(|(l, _)| l == "Trefferpunkte"), "{tower:?}");
        let store = upgrade_changes(def(&defs, "storehouse"), def(&defs, "warehouse"), 0);
        assert!(store.iter().any(|(l, _)| l == "Lagerplatz je Ware"), "{store:?}");
    }

    #[test]
    fn every_upgrade_target_shows_changes() {
        let defs = defs();
        for d in defs.buildings().iter() {
            let Some(up) = d.upgrade.as_ref() else { continue };
            let Some(target) = defs.buildings().get(up.to.index()) else { panic!("missing target of {}", d.key) };
            assert!(!upgrade_changes(d, target, 0).is_empty(), "{} → {} shows no change", d.key, target.key);
        }
    }

    #[test]
    fn household_texts() {
        assert_eq!(yesterday_text(12, 1_250), "Gestern: 12 Gold verdient, 1.250 Gold ausgegeben");
        assert_eq!(wealth_color(WealthClass::Poor), Palette::BAD);
        assert_eq!(wealth_color(WealthClass::Wealthy), Palette::GOOD);
    }

    fn measured(resource: Option<ResourceId>) -> MeasuredOutput {
        MeasuredOutput {
            resource,
            cycles_yesterday: Some(5),
            units_yesterday: Some(10),
            cycles_today: 2,
            units_today: 4,
            possible_cycles_milli: Some(6_000),
            possible_units_milli: Some(12_000),
            days: 12,
            average_units_milli: 9_500,
            average_value_milli: 28_000,
            text: "Gestern 5 Arbeitsgänge (10 Einheiten Bretter), heute bisher 2 – möglich sind 6,0 je Tag.".to_owned(),
        }
    }

    #[test]
    fn upgrade_blockers_read_as_one_sentence() {
        assert_eq!(first_blocker_text("Nicht genug Stein (0 von 30).", 0), "Nicht genug Stein (0 von 30).");
        assert_eq!(first_blocker_text("Nicht genug Stein (0 von 30).", 1), "Nicht genug Stein (0 von 30) – und ein weiterer Grund.");
        assert_eq!(first_blocker_text("Erst erforschen: Baukunst", 2), "Erst erforschen: Baukunst – und 2 weitere Gründe.");
    }

    #[test]
    fn training_shows_only_while_workers_learn() {
        assert_eq!(training_row(100), None);
        assert_eq!(training_row(86), Some(("Einarbeitung".to_owned(), "86 % (in der Belegschaft enthalten)".to_owned())));
    }

    #[test]
    fn measured_output_is_set_against_the_possible() {
        let defs = defs();
        let planks = defs.resource_id("planks");
        let mut m = measured(planks);
        assert_eq!(measured_share(&m).map(|s| (s * 1000.0).round()), Some(833.0));
        assert_eq!(average_output_text(&defs, &m).as_deref(), Some("Mittel der letzten 12 Tage: 9,5 Bretter je Tag"));
        // Before the first midnight, without possible work or with one day only: nothing to compare.
        m.days = 1;
        assert_eq!(average_output_text(&defs, &m), None);
        m.possible_cycles_milli = Some(0);
        assert_eq!(measured_share(&m), None);
        m.cycles_yesterday = None;
        m.possible_cycles_milli = Some(6_000);
        assert_eq!(measured_share(&m), None);
        assert_eq!(economics_basis_text(EconomicsBasis::Crew), "Je Tag mit der zugewiesenen Belegschaft, zu heutigen Marktpreisen:");
        assert_eq!(
            economics_basis_text(EconomicsBasis::Harvests { days: 12 }),
            "Je Tag im Mittel der Ernten der letzten 12 Tage, zu heutigen Marktpreisen:"
        );
        assert!(economics_basis_text(EconomicsBasis::Harvests { days: 1 }).contains("des letzten Tages"));
        assert!(economics_basis_text(EconomicsBasis::Harvests { days: 0 }).contains("noch ist keiner gemessen"));
    }
}
