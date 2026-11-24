//! „Produktion“: every production building grouped by type with status and reason, staffing,
//! productivity, what it made yesterday against what is possible (field farms: their harvest)
//! and the current recipe or order. A click on a row selects the building and moves the camera
//! to it.

use kingdoms_engine::egui::{self, Color32, RichText, Ui};
use kingdoms_sim::buildings::{BuildingId, BuildingStatus};
use kingdoms_sim::defs::{BuildingTypeId, Defs};
use kingdoms_sim::policy::JobPriority;
use kingdoms_sim::production::ProductionStatus;
use kingdoms_sim::report::{self, BuildingEconomics, EconomicsBasis};

use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, TabContext, card_row, hint, select};
use crate::ui::hud::HudAction;
use crate::ui::hud::format::decimal;
use crate::ui::theme::{Palette, heading_font};
use crate::ui::widgets::{mini_bar, rating_color};

/// One production building.
#[derive(Clone, Debug)]
struct Row {
    building: BuildingId,
    /// Number within its type (1-based, in id order).
    number: usize,
    status_text: String,
    /// What the status cell does not show (the way to work, waiting transports), as its tooltip.
    detail: Option<String>,
    color: Color32,
    /// Counts as a problem in the filter and the overview.
    problem: bool,
    /// Working or waiting for growing fields (counts as "in Arbeit").
    working: bool,
    /// A work cycle runs (productivity is meaningful).
    busy: bool,
    staffed: u8,
    workers: u8,
    productivity: u32,
    /// Yesterday's output against the possible ([`measured_cell`]) and the simulation's
    /// sentence on it as the tooltip (`None`: no figures, e.g. a construction site).
    measured: Option<(String, Color32, String)>,
    recipe: String,
    priority: JobPriority,
}

/// All buildings of one type.
#[derive(Clone, Debug)]
struct Group {
    kind: BuildingTypeId,
    icon: String,
    name: String,
    rows: Vec<Row>,
}

impl Group {
    fn staffed(&self) -> (u32, u32) {
        self.rows.iter().fold((0, 0), |(s, w), r| (s + u32::from(r.staffed), w + u32::from(r.workers)))
    }
}

/// Colour and problem flag of a production status (the same colours as the info panel).
pub fn status_style(status: ProductionStatus) -> (Color32, bool) {
    match status {
        ProductionStatus::Working | ProductionStatus::Growing => (Palette::GOOD, false),
        ProductionStatus::Idle
        | ProductionStatus::UnderConstruction
        | ProductionStatus::Upgrading
        | ProductionStatus::OutOfSeason
        | ProductionStatus::WorkersAway
        | ProductionStatus::LunchBreak
        | ProductionStatus::OffShift
        // A dense forest leaves the forester nothing to plant: a wait, not a fault.
        | ProductionStatus::NoPlantingSpace => (Palette::INK_SOFT, false),
        ProductionStatus::Paused => (Palette::WARN, false),
        _ => (Palette::BAD, true),
    }
}

/// State of the tab.
#[derive(Default)]
pub struct ProductionTab {
    only_problems: bool,
    collapsed: Vec<BuildingTypeId>,
    data: Cached<Vec<Group>>,
}

impl ProductionTab {
    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let input = ctx.input;
        let groups = self.data.get(input, || {
            let state = input.session.state();
            let defs = input.session.defs();
            let mut groups: Vec<Group> = Vec::new();
            for (id, b) in state.buildings().iter() {
                let Some(def) = defs.buildings().get(b.kind().index()) else { continue };
                if def.production.is_none() {
                    continue;
                }
                let row = match report::production_report(state, defs, id).filter(|_| b.is_complete()) {
                    Some(p) => {
                        // Workers who find no path never arrive: a problem, not a neutral wait.
                        let no_path = p.present == 0 && p.commute.no_path > 0;
                        let (color, problem) = if no_path { (Palette::BAD, true) } else { status_style(p.status) };
                        let detail = status_detail(&p, no_path, report::path_problem(state, id));
                        let status_text = if no_path { "Kein Weg zur Arbeit".to_owned() } else { p.status_text };
                        let recipe = match (p.active_recipe, p.queue.first()) {
                            (_, None) if !p.recipes.is_empty() => "keine Aufträge".to_owned(),
                            (Some(i), first) => {
                                let name = p.recipes.get(usize::from(i)).map_or_else(String::new, |r| r.name.clone());
                                match first.and_then(|q| q.remaining) {
                                    Some(n) => format!("{name} (noch {n})"),
                                    None => name,
                                }
                            }
                            (None, _) => String::new(),
                        };
                        Row {
                            building: id,
                            number: 0,
                            status_text,
                            detail,
                            color,
                            problem,
                            working: p.status.is_working() || p.status == ProductionStatus::Growing,
                            busy: p.status.is_working(),
                            staffed: p.staffed,
                            workers: p.workers,
                            productivity: p.productivity_percent,
                            measured: report::building_economics(state, defs, id).map(|e| {
                                let (text, color) = measured_cell(defs, &e);
                                (text, color, e.measured.text)
                            }),
                            recipe,
                            priority: p.priority,
                        }
                    }
                    None => {
                        let status = b.status();
                        let progress = b.construction_site().map_or(0, |s| (s.progress() * 100.0).round() as u32);
                        let text = match status {
                            BuildingStatus::UnderConstruction
                            | BuildingStatus::Moving
                            | BuildingStatus::Upgrading
                            | BuildingStatus::Damaged => {
                                format!("{} · {progress} %", status.display_name())
                            }
                            other => other.display_name().to_owned(),
                        };
                        Row {
                            building: id,
                            number: 0,
                            status_text: text,
                            detail: report::carrier_wait(state, id).map(|w| w.text),
                            color: Palette::INFO,
                            problem: false,
                            working: false,
                            busy: false,
                            // Builders are counted under „Arbeitskräfte“; the site has no workers yet.
                            staffed: 0,
                            workers: 0,
                            productivity: 0,
                            measured: None,
                            recipe: String::new(),
                            priority: b.priority(),
                        }
                    }
                };
                match groups.iter_mut().find(|g| g.kind == b.kind()) {
                    Some(g) => g.rows.push(row),
                    None => groups.push(Group { kind: b.kind(), icon: format!("bld_{}", def.key), name: def.name.clone(), rows: vec![row] }),
                }
            }
            for g in &mut groups {
                for (i, r) in g.rows.iter_mut().enumerate() {
                    r.number = i + 1;
                }
            }
            groups.sort_by_key(|g| g.kind);
            groups
        });
        let all: Vec<&Row> = groups.iter().flat_map(|g| g.rows.iter()).collect();
        let working = all.iter().filter(|r| r.working).count();
        let problems = all.iter().filter(|r| r.problem).count();
        let unstaffed = all.iter().filter(|r| r.workers > 0 && r.staffed == 0).count();
        let (staffed, workers) = groups.iter().fold((0, 0), |(s, w), g| {
            let (a, b) = g.staffed();
            (s + a, w + b)
        });
        card_row(
            ui,
            &[
                ("Betriebe", all.len().to_string(), Palette::INK),
                ("In Arbeit", working.to_string(), Palette::GOOD),
                ("Mit Problemen", problems.to_string(), if problems > 0 { Palette::BAD } else { Palette::INK }),
                ("Ohne Arbeiter", unstaffed.to_string(), if unstaffed > 0 { Palette::WARN } else { Palette::INK }),
                ("Arbeiter", format!("{staffed} von {workers}"), if staffed < workers { Palette::WARN } else { Palette::INK }),
            ],
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.only_problems, "Nur Betriebe mit Problemen");
            ui.label(RichText::new("· Klick auf einen Betrieb: auswählen und hinschauen").small().color(Palette::INK_SOFT));
        });
        if groups.is_empty() {
            hint(ui, "Noch keine Produktionsbetriebe. Das Baumenü (B) bietet sie unter „Produktion“ und „Landwirtschaft“.");
            return;
        }
        const W: [f32; 7] = [180.0, 220.0, 80.0, 130.0, 110.0, 150.0, 80.0];
        table::header(
            ui,
            &[
                ("Betrieb", W[0], CellAlign::Left),
                ("Zustand", W[1], CellAlign::Left),
                ("Arbeiter", W[2], CellAlign::Right),
                ("Produktivität", W[3], CellAlign::Left),
                ("Gestern", W[4], CellAlign::Left),
                ("Arbeitsgang / Auftrag", W[5], CellAlign::Left),
                ("Priorität", W[6], CellAlign::Left),
            ],
        );
        let mut toggle = None;
        let mut any = false;
        for g in groups.iter() {
            let rows: Vec<&Row> = g.rows.iter().filter(|r| !self.only_problems || r.problem).collect();
            if rows.is_empty() {
                continue;
            }
            any = true;
            let collapsed = self.collapsed.contains(&g.kind);
            let (staffed, workers) = g.staffed();
            let working = g.rows.iter().filter(|r| r.working).count();
            let response = table::row(ui, ("group", g.kind.index()), ROW_HEIGHT + 6.0, false, true, |ui| {
                table::cell(ui, W[0] + W[1], CellAlign::Left, |ui| {
                    disclosure(ui, collapsed);
                    if let Some(icon) = ctx.icons.get(&g.icon) {
                        icon.show(ui, 26.0);
                    }
                    ui.label(RichText::new(&g.name).font(heading_font(15.0)).color(Palette::INK));
                    let count = if g.rows.len() == 1 { "1 Betrieb".to_owned() } else { format!("{} Betriebe", g.rows.len()) };
                    ui.label(RichText::new(format!("· {count}, {working} in Arbeit")).small().color(Palette::INK_SOFT));
                });
                table::text(ui, W[2], CellAlign::Right, RichText::new(format!("{staffed} / {workers}")).strong());
            });
            if response.on_hover_text(if collapsed { "Aufklappen" } else { "Zuklappen" }).clicked() {
                toggle = Some(g.kind);
            }
            if collapsed {
                continue;
            }
            for (i, r) in rows.iter().enumerate() {
                let response = table::row(ui, ("prod", r.building.index()), ROW_HEIGHT, i % 2 == 0, true, |ui| {
                    table::cell(ui, W[0], CellAlign::Left, |ui| {
                        ui.add_space(30.0);
                        ui.label(RichText::new(format!("{} {}", g.name, r.number)).color(Palette::INK));
                    });
                    table::cell(ui, W[1], CellAlign::Left, |ui| {
                        // A cut-off status shows in full when hovered; extra detail only where
                        // there is some (no tooltip that merely repeats the cell).
                        let status = ui.add(egui::Label::new(RichText::new(&r.status_text).color(r.color)).truncate());
                        if let Some(detail) = &r.detail {
                            status.on_hover_text(detail);
                        }
                    });
                    if r.workers == 0 {
                        table::text(ui, W[2], CellAlign::Right, RichText::new("–").color(Palette::INK_SOFT));
                    } else {
                        let staff_color = if r.staffed >= r.workers { Palette::INK } else if r.staffed > 0 { Palette::WARN } else { Palette::BAD };
                        table::text(ui, W[2], CellAlign::Right, RichText::new(format!("{} / {}", r.staffed, r.workers)).color(staff_color));
                    }
                    table::cell(ui, W[3], CellAlign::Left, |ui| {
                        ui.add_space(10.0);
                        if r.busy || r.problem {
                            mini_bar(ui, r.productivity as f32 / 100.0, rating_color(r.productivity as f32), 70.0);
                            ui.label(RichText::new(format!("{} %", r.productivity)).small());
                        } else {
                            ui.label(RichText::new("–").color(Palette::INK_SOFT));
                        }
                    });
                    table::cell(ui, W[4], CellAlign::Left, |ui| match &r.measured {
                        Some((text, color, sentence)) => {
                            ui.add(egui::Label::new(RichText::new(text).small().color(*color)).truncate()).on_hover_text(sentence);
                        }
                        None => {
                            ui.label(RichText::new("–").color(Palette::INK_SOFT));
                        }
                    });
                    table::text(ui, W[5], CellAlign::Left, RichText::new(&r.recipe).color(Palette::INK_SOFT));
                    let p_color = match r.priority {
                        JobPriority::High => Palette::GOOD,
                        JobPriority::Low => Palette::WARN,
                        JobPriority::Normal => Palette::INK_SOFT,
                    };
                    table::text(ui, W[6], CellAlign::Left, RichText::new(r.priority.display_name()).small().color(p_color));
                });
                if response.clicked() {
                    select(actions, r.building);
                }
            }
        }
        if !any {
            hint(ui, "Keine Probleme – alle Betriebe arbeiten oder warten planmäßig.");
        }
        if let Some(kind) = toggle {
            match self.collapsed.iter().position(|k| *k == kind) {
                Some(i) => {
                    self.collapsed.remove(i);
                }
                None => self.collapsed.push(kind),
            }
            actions.push(HudAction::Sound("ui_click"));
        }
    }
}

/// Tooltip of a production row's status: the path problem when the workers find no way, the
/// commute while they are away or on their way, transports waiting for a carrier; `None` when
/// the status says it all.
fn status_detail(p: &report::ProductionReport, no_path: bool, path_problem: Option<String>) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    if no_path {
        lines.extend(path_problem);
    } else if p.commute.assigned > 0
        && matches!(p.status, ProductionStatus::WorkersAway | ProductionStatus::LunchBreak | ProductionStatus::OffShift)
    {
        lines.push(p.commute.text.clone());
    }
    lines.extend(p.carrier_wait.as_ref().map(|w| w.text.clone()));
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The „Gestern“ cell of a building: work cycles of the last closed day against the cycles its
/// crew could do in a whole day („5 von 6,5“, coloured by the share), for field farms and
/// orchards the harvest of that day („12 Weizen“); „–“ before the building's first midnight.
pub fn measured_cell(defs: &Defs, e: &BuildingEconomics) -> (String, Color32) {
    let m = &e.measured;
    match e.basis {
        EconomicsBasis::Crew => match (m.cycles_yesterday, m.possible_cycles_milli) {
            (None, _) => ("–".to_owned(), Palette::INK_SOFT),
            (Some(done), Some(possible)) if possible > 0 => {
                let share = done as f32 * 100_000.0 / possible as f32;
                (format!("{done} von {}", decimal(possible as f64 / 1000.0, 1)), rating_color(share))
            }
            (Some(done), _) => (done.to_string(), Palette::INK_SOFT),
        },
        EconomicsBasis::Harvests { .. } => match m.units_yesterday {
            None => ("–".to_owned(), Palette::INK_SOFT),
            Some(units) => {
                let name = m.resource.and_then(|r| defs.resources().get(r.index())).map_or("", |r| r.name.as_str());
                let color = if units == 0 { Palette::INK_SOFT } else { Palette::INK };
                (format!("{units} {name}").trim_end().to_owned(), color)
            }
        },
    }
}

/// Painted triangle: right when collapsed, down when open.
pub(super) fn disclosure(ui: &mut Ui, collapsed: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    let c = rect.center();
    let points = if collapsed {
        vec![c + egui::vec2(-3.0, -5.0), c + egui::vec2(4.0, 0.0), c + egui::vec2(-3.0, 5.0)]
    } else {
        vec![c + egui::vec2(-5.0, -3.0), c + egui::vec2(5.0, -3.0), c + egui::vec2(0.0, 4.0)]
    };
    ui.painter().add(egui::Shape::convex_polygon(points, Palette::GOLD_DARK, egui::Stroke::NONE));
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::hud::management::tests::test_defs;

    #[test]
    fn status_colours_flag_problems() {
        let defs = test_defs();
        let wood = defs.resource_id("wood").expect("wood");
        assert_eq!(status_style(ProductionStatus::Working), (Palette::GOOD, false));
        assert_eq!(status_style(ProductionStatus::Growing), (Palette::GOOD, false));
        assert_eq!(status_style(ProductionStatus::OutOfSeason), (Palette::INK_SOFT, false));
        // Workers on the way, at lunch or after work are no problem.
        assert_eq!(status_style(ProductionStatus::WorkersAway), (Palette::INK_SOFT, false));
        assert_eq!(status_style(ProductionStatus::LunchBreak), (Palette::INK_SOFT, false));
        assert_eq!(status_style(ProductionStatus::OffShift), (Palette::INK_SOFT, false));
        assert_eq!(status_style(ProductionStatus::Paused), (Palette::WARN, false));
        assert_eq!(status_style(ProductionStatus::NoPlantingSpace), (Palette::INK_SOFT, false));
        assert!(status_style(ProductionStatus::NoWorkers).1);
        assert!(status_style(ProductionStatus::OutputFull).1);
        assert!(status_style(ProductionStatus::MissingInput(wood)).1);
    }

    #[test]
    fn yesterday_is_set_against_the_possible_or_shows_the_harvest() {
        let defs = test_defs();
        let planks = defs.resource_id("planks");
        let wheat = defs.resource_id("wheat");
        let measured = |resource| report::MeasuredOutput {
            resource,
            cycles_yesterday: Some(5),
            units_yesterday: Some(10),
            cycles_today: 2,
            units_today: 4,
            possible_cycles_milli: Some(6_000),
            possible_units_milli: Some(12_000),
            days: 3,
            average_units_milli: 9_000,
            average_value_milli: 20_000,
            text: String::new(),
        };
        let mut e = BuildingEconomics {
            building: BuildingId::from_raw_parts(1, 0),
            recipe: Some(0),
            basis: EconomicsBasis::Crew,
            cycles_per_day_milli: 6_000,
            wages_milli: 0,
            upkeep_milli: 0,
            inputs_milli: 0,
            outputs_milli: 0,
            net_milli: 0,
            main_output: None,
            text: String::new(),
            measured: measured(planks),
        };
        assert_eq!(measured_cell(&defs, &e), ("5 von 6".to_owned(), Palette::GOOD));
        e.measured.cycles_yesterday = Some(1);
        assert_eq!(measured_cell(&defs, &e).1, Palette::BAD, "a sixth of the possible");
        e.measured.possible_cycles_milli = Some(0);
        assert_eq!(measured_cell(&defs, &e), ("1".to_owned(), Palette::INK_SOFT));
        e.measured.cycles_yesterday = None;
        assert_eq!(measured_cell(&defs, &e).0, "–", "before the first midnight");
        // Field farms show the harvest of the day.
        e.basis = EconomicsBasis::Harvests { days: 3 };
        e.measured = measured(wheat);
        e.measured.units_yesterday = Some(12);
        assert_eq!(measured_cell(&defs, &e), ("12 Weizen".to_owned(), Palette::INK));
        e.measured.units_yesterday = Some(0);
        assert_eq!(measured_cell(&defs, &e), ("0 Weizen".to_owned(), Palette::INK_SOFT));
    }

    #[test]
    fn row_tooltips_only_add_what_the_cell_does_not_say() {
        use crate::session::{DevPlan, Session};
        use kingdoms_sim::state::GameSettings;
        use kingdoms_sim::world::WorldSize;
        let defs = std::sync::Arc::new(test_defs());
        let mut session = Session::new(defs.clone(), GameSettings::new(&defs, 7, WorldSize::Small));
        // The showcase settlement places workshops (sites have a production report, too).
        session.start_dev_settlement(DevPlan::Showcase);
        session.tick();
        let state = session.state();
        let mut p = state
            .buildings()
            .iter()
            .find_map(|(id, _)| report::production_report(state, &defs, id))
            .expect("a production building in the showcase");
        p.carrier_wait = None;
        p.commute.assigned = 2;
        p.status = ProductionStatus::Working;
        assert_eq!(status_detail(&p, false, None), None, "„In Arbeit“ needs no tooltip");
        p.status = ProductionStatus::OffShift;
        p.commute.text = "Arbeitsweg Ø 12 min (2 % der Schicht)".to_owned();
        assert_eq!(status_detail(&p, false, None).as_deref(), Some("Arbeitsweg Ø 12 min (2 % der Schicht)"));
        let no_way = "Kein Weg: 1 Bewohner erreicht das Gebäude nicht.".to_owned();
        assert_eq!(status_detail(&p, true, Some(no_way.clone())), Some(no_way));
    }
}
