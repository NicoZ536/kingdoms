//! The tooltip next to the cursor on the map: the citizen under the cursor or tile information
//! while inspecting, placement
//! verdict with reasons and cost while building or moving, length and cost of a planned road,
//! the refund of a demolition.

use kingdoms_engine::egui::{self, RichText};
use kingdoms_sim::buildings::{EntranceStatus, PlacementCheck, PlacementWarning};
use kingdoms_sim::defs::Defs;
use kingdoms_sim::roads::RoadPlan;
use kingdoms_sim::state::GameState;

use super::build_menu::cost_line;
use super::format::{count, direction, hours, tile_rows, tiles};
use super::info_panel::refund_text;
use super::top_bar::{row, tooltip_title};
use super::{HudInput, Icons};
use crate::actions::Action;
use crate::tools::{PlacementPreview, QUEUED_CONFLICT, Tool, l_line, movable};
use crate::ui::theme::{self, Palette};
use crate::ui::widgets::{Mark, marked};

/// Seconds the cursor rests on a tile before the inspect tooltip appears.
const INSPECT_DELAY: f32 = 0.35;
const WIDTH: f32 = 300.0;

/// What a click of the order tool would order, or why it is impossible.
#[allow(clippy::too_many_arguments)]
fn order(
    ui: &mut egui::Ui,
    state: &kingdoms_sim::state::GameState,
    defs: &kingdoms_sim::defs::Defs,
    company: u32,
    pick: crate::tools::OrderPick,
    tile: Option<glam::IVec2>,
    building: Option<kingdoms_sim::buildings::BuildingId>,
    points: &[glam::IVec2],
) {
    let Some(tile) = tile else { return };
    let name = state.military().company(company).map_or("Kompanie", |c| c.name());
    match pick {
        crate::tools::OrderPick::Defend { radius } => {
            let order = crate::tools::defend_order(state, building, tile, radius);
            let what = match &order {
                kingdoms_sim::military::Order::Defend(kingdoms_sim::military::DefendTarget::Building(b)) => {
                    let n = state.building(*b).and_then(|b| defs.buildings().get(b.kind().index())).map_or("Gebäude", |d| d.name.as_str());
                    format!("{name} verteidigt {n}")
                }
                _ => format!("{name} hält das Gebiet um Feld {}, {} (Umkreis {radius})", tile.x, tile.y),
            };
            tooltip_title(ui, &what);
            match kingdoms_sim::military::order_problem(state, defs, company, &order) {
                Some(p) => ui.colored_label(Palette::BAD, p),
                None => ui.colored_label(Palette::GOOD, "Klicken, um den Befehl zu geben."),
            };
        }
        crate::tools::OrderPick::Patrol => {
            tooltip_title(ui, &format!("{name}: Patrouille"));
            if points.len() >= 2 && points.first() == Some(&tile) {
                ui.colored_label(Palette::GOOD, "Klicken schließt die Runde und gibt den Befehl.");
            } else if !state.navigation().is_walkable(tile) {
                ui.colored_label(Palette::BAD, "Hier kann niemand gehen.");
            } else {
                ui.colored_label(Palette::GOOD, format!("Klicken setzt Wegpunkt {}.", points.len() + 1));
            }
        }
    }
}

pub fn show(ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons) {
    let Some(pointer) = input.pointer else { return };
    if input.hover.tile.is_none() && input.hover.building.is_none() {
        return;
    }
    let state = input.session.state();
    let defs = input.session.defs();
    let tools = input.tools;
    let content: Box<dyn FnOnce(&mut egui::Ui) + '_> = match tools.tool() {
        Tool::Inspect if input.hover.citizen.is_some() => {
            let Some(id) = input.hover.citizen else { return };
            Box::new(move |ui| citizen(ui, state, defs, id))
        }
        Tool::Inspect => {
            if input.hover_rest < INSPECT_DELAY {
                return;
            }
            Box::new(move |ui| inspect(ui, input, state, defs))
        }
        Tool::Build { .. } | Tool::Move { building: Some(_) } => {
            let Some(preview) = tools.placement() else { return };
            Box::new(move |ui| placement(ui, input, icons, state, defs, preview))
        }
        Tool::Move { building: None } => {
            let Some(id) = input.hover.building else { return };
            Box::new(move |ui| {
                let name = state.building(id).and_then(|b| defs.buildings().get(b.kind().index())).map_or("", |d| d.name.as_str());
                tooltip_title(ui, name);
                match movable(state, defs, id) {
                    Ok(()) => ui.colored_label(Palette::GOOD, "Klicken, um das Gebäude aufzunehmen."),
                    Err(reason) => ui.colored_label(Palette::BAD, reason),
                };
            })
        }
        Tool::Road { .. } => {
            let plan = tools.road_drag().map(|d| &d.plan).or(tools.road_hover());
            let Some(plan) = plan else { return };
            let dragging = tools.road_drag().is_some();
            let conflict = tools.road_queued_conflict();
            Box::new(move |ui| road(ui, input, icons, state, defs, plan, dragging, conflict))
        }
        Tool::Demolish => Box::new(move |ui| demolish(ui, input, icons, state, defs)),
        Tool::Order { company, pick } => {
            let tile = input.hover.tile;
            let building = input.hover.building;
            Box::new(move |ui| order(ui, state, defs, company, pick, tile, building, tools.patrol_points()))
        }
    };
    egui::Area::new(egui::Id::unique("hud_world_tooltip"))
        .fixed_pos(pointer + egui::vec2(22.0, 20.0))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .constrain(true)
        .show(ctx, |ui| {
            theme::parchment_frame()
                .inner_margin(egui::Margin::symmetric(10, 8))
                .stroke(egui::Stroke::new(1.5, Palette::WOOD))
                .show(ui, |ui| {
                    ui.set_max_width(WIDTH);
                    content(ui);
                });
        });
}

/// Name and current activity of the citizen under the cursor.
fn citizen(ui: &mut egui::Ui, state: &GameState, defs: &Defs, id: kingdoms_sim::population::CitizenId) {
    let pop = state.population();
    let Some(c) = pop.citizen(id) else { return };
    tooltip_title(ui, &pop.full_name(id).unwrap_or_default());
    let age = c.age_on(state.clock().total_days());
    ui.label(RichText::new(format!("{}, {}", c.stage().display_name(), super::format::years(age))).color(Palette::INK_SOFT));
    if let Some(text) = kingdoms_sim::population::activity_text(state, defs, id) {
        ui.label(RichText::new(text).color(Palette::INK));
    }
    if let Some(goods) = c.carried().and_then(|g| defs.resources().get(g.resource.index()).map(|d| (g.amount, d))) {
        ui.label(RichText::new(format!("Trägt {} {}", goods.0, goods.1.name)).color(Palette::INK));
    }
    hint(ui, "Klicken: Bürgerfenster öffnen".to_owned());
}

fn hint(ui: &mut egui::Ui, text: String) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).small().color(Palette::INK_SOFT));
}

fn inspect(ui: &mut egui::Ui, input: &HudInput<'_>, state: &GameState, defs: &Defs) {
    if let Some(b) = input.hover.building.and_then(|id| state.building(id))
        && let Some(def) = defs.buildings().get(b.kind().index())
    {
        tooltip_title(ui, &def.name);
        ui.label(RichText::new(b.status().display_name()).color(Palette::INK_SOFT));
        hint(ui, "Klicken für Details".into());
        return;
    }
    let Some(tile) = input.hover.tile else { return };
    tooltip_title(ui, &format!("Feld {}, {}", tile.x, tile.y));
    egui::Grid::new("tt_tile").num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        for (label, value) in tile_rows(state, defs, tile) {
            row(ui, &label, value);
        }
    });
}

fn placement(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, state: &GameState, defs: &Defs, preview: &PlacementPreview) {
    let check = &preview.check;
    let Some(def) = defs.buildings().get(check.kind.index()) else { return };
    let title = if preview.moving.is_some() { format!("{} versetzen", def.name) } else { def.name.clone() };
    tooltip_title(ui, &title);
    let access_built = preview.access.as_ref().is_some_and(|a| a.valid && a.new_tile_count() > 0);
    if preview.queued_conflict {
        marked(ui, Mark::Error, RichText::new(QUEUED_CONFLICT).color(Palette::BAD));
    }
    verdict(ui, check, access_built);
    egui::Grid::new("tt_place").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
        if preview.moving.is_none() {
            ui.label(RichText::new("Kosten").color(Palette::INK_SOFT));
            cost_line(ui, icons, defs, state, &check.cost);
            ui.end_row();
        }
        row(ui, "Bauzeit", format!("{} (ein Bauarbeiter)", hours(check.build_hours)));
        row(ui, "Ausrichtung", format!("Eingang nach {}", direction(input.tools.rotation().facing())));
        let entrance = match check.entrance_status {
            EntranceStatus::Road => RichText::new("an der Straße").color(Palette::GOOD),
            EntranceStatus::Free => RichText::new("noch ohne Straße").color(Palette::WARN),
            EntranceStatus::Blocked => RichText::new("versperrt").color(Palette::BAD),
        };
        row(ui, "Eingang", entrance);
    });
    if preview.moving.is_none() && check.ok && check.entrance_status == EntranceStatus::Free {
        match preview.access.as_ref().filter(|a| a.valid) {
            Some(access) if access.new_tile_count() > 0 => {
                let name = defs.roads().get(access.road.index()).map_or("Weg", |r| r.name.as_str());
                let n = access.new_tile_count();
                let verb = if n == 1 { "wird" } else { "werden" };
                ui.label(RichText::new(format!("Zufahrt: {} {name} {verb} mitgebaut.", tiles(n))).color(Palette::INFO));
            }
            Some(_) => {}
            None => {
                ui.label(RichText::new("Keine Straße in der Nähe – die Zufahrt muss selbst gebaut werden.").color(Palette::WARN));
            }
        }
    }
    hint(
        ui,
        format!(
            "Klick: {} · {} / Umschalt+Mausrad: drehen{} · Rechtsklick/Esc: abbrechen",
            if preview.moving.is_some() { "hier aufstellen" } else { "bauen" },
            input.hint(Action::Rotate),
            if preview.moving.is_some() { "" } else { " · Umschalt+Klick: weiterbauen" }
        ),
    );
}

/// The simulation's verdict: errors, then warnings (the missing road is left out when the
/// suggested access road is built with the building). A site no storage reaches would never
/// get its material: it is not called suitable, and that warning stands out in red.
fn verdict(ui: &mut egui::Ui, check: &PlacementCheck, access_built: bool) {
    if check.ok && !crate::tools::site_unsupplied(check) {
        marked(ui, Mark::Ok, RichText::new("Bauplatz geeignet").strong().color(Palette::GOOD));
    }
    for e in &check.errors {
        marked(ui, Mark::Error, RichText::new(e.to_string()).color(Palette::BAD));
    }
    for w in check.warnings.iter().filter(|w| !(access_built && matches!(w, PlacementWarning::NoRoadAccess))) {
        if matches!(w, PlacementWarning::NoStorageInReach) {
            marked(ui, Mark::Error, RichText::new(w.to_string()).strong().color(Palette::BAD));
        } else {
            marked(ui, Mark::Warning, RichText::new(w.to_string()).color(Palette::WARN));
        }
    }
}


#[allow(clippy::too_many_arguments)]
fn road(
    ui: &mut egui::Ui,
    input: &HudInput<'_>,
    icons: &Icons,
    state: &GameState,
    defs: &Defs,
    plan: &RoadPlan,
    dragging: bool,
    queued_conflict: bool,
) {
    let name = defs.roads().get(plan.road.index()).map_or("Straße", |r| r.name.as_str());
    tooltip_title(ui, name);
    let bridges = plan.tiles.iter().filter(|t| t.bridge_axis.is_some() && t.is_new).count();
    egui::Grid::new("tt_road").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
        row(ui, "Länge", format!("{}, davon {} neu", tiles(plan.tiles.len()), plan.new_tile_count()));
        if bridges > 0 {
            row(ui, "Brücke", tiles(bridges));
        }
        ui.label(RichText::new("Kosten").color(Palette::INK_SOFT));
        cost_line(ui, icons, defs, state, &plan.cost);
        ui.end_row();
    });
    if plan.valid {
        marked(ui, Mark::Ok, RichText::new("Kann gebaut werden").strong().color(Palette::GOOD));
    }
    if queued_conflict {
        marked(ui, Mark::Error, RichText::new(QUEUED_CONFLICT).color(Palette::BAD));
    }
    for p in plan.problems.iter().take(4) {
        marked(ui, Mark::Error, RichText::new(p.to_string()).color(Palette::BAD));
    }
    let text = if dragging {
        "Loslassen: bauen · Rechtsklick/Esc: abbrechen".to_owned()
    } else {
        format!("Linke Maustaste gedrückt halten und ziehen · Rechtsklick/{}: Werkzeug beenden", input.hint(Action::Cancel))
    };
    hint(ui, text);
}

fn demolish(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, state: &GameState, defs: &Defs) {
    if let Some(drag) = input.tools.demolish_drag().filter(|d| d.start != d.end) {
        let n = l_line(drag.start, drag.end).iter().filter(|t| state.roads().has_road(**t)).count();
        tooltip_title(ui, "Straßen entfernen");
        ui.label(count(n as u64, "Straßenfeld", "Straßenfelder"));
        hint(ui, "Loslassen: entfernen · Rechtsklick/Esc: abbrechen".into());
        return;
    }
    if let Some(id) = input.hover.building
        && let Some(b) = state.building(id)
        && let Some(def) = defs.buildings().get(b.kind().index())
    {
        tooltip_title(ui, &format!("{} abreißen", def.name));
        match kingdoms_sim::buildings::demolish_refund(state, defs, id) {
            Ok(refund) => refund_text(ui, icons, defs, &refund),
            Err(reason) => {
                ui.colored_label(Palette::BAD, reason.to_string());
            }
        }
        hint(ui, "Klick: abreißen · Rechtsklick/Esc: Werkzeug beenden".into());
        return;
    }
    if let Some(tile) = input.hover.tile.filter(|t| state.roads().has_road(*t)) {
        let name = state.roads().get(tile).and_then(|r| defs.roads().get(r.road.index())).map_or("Straße", |r| r.name.as_str());
        tooltip_title(ui, &format!("{name} entfernen"));
        ui.label(RichText::new("Straßen werden ohne Erstattung entfernt.").color(Palette::INK_SOFT));
        hint(ui, "Klick: entfernen · Ziehen: mehrere Felder".into());
        return;
    }
    tooltip_title(ui, "Abreißen");
    hint(ui, "Gebäude oder Straße anklicken · Rechtsklick/Esc: Werkzeug beenden".into());
}
