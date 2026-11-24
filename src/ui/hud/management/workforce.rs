//! „Arbeitskräfte“: demand, staffed and unfilled jobs per category with the category priorities
//! (`Command::SetWorkforcePriority`), the totals of the working population, and every
//! workplace with its own priority (`Command::SetProductionPriority`).

use kingdoms_engine::egui::{self, RichText, Ui};
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::JobCategory;
use kingdoms_sim::policy::JobPriority;
use kingdoms_sim::report;
use kingdoms_sim::workforce::WorkforceSummary;

use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, TabContext, card_row, command, hint, select};
use crate::ui::hud::HudAction;
use crate::ui::theme::{Palette, heading_font};
use crate::ui::widgets::{mini_bar, section_header};

const PRIORITIES: [JobPriority; 3] = [JobPriority::Low, JobPriority::Normal, JobPriority::High];

/// One workplace or construction site.
#[derive(Clone, Debug)]
struct Workplace {
    building: BuildingId,
    category: JobCategory,
    icon: String,
    name: String,
    /// The building is a construction site (builders) rather than a finished workplace.
    site: bool,
    staffed: u8,
    wanted: u8,
    priority: JobPriority,
}

struct Data {
    summary: WorkforceSummary,
    workplaces: Vec<Workplace>,
}

/// State of the tab.
#[derive(Default)]
pub struct WorkforceTab {
    open_categories: Vec<JobCategory>,
    data: Cached<Data>,
}

/// Share of filled slots, 1.0 without demand.
pub fn fill_fraction(staffed: u32, wanted: u32) -> f32 {
    if wanted == 0 { 1.0 } else { (staffed as f32 / wanted as f32).min(1.0) }
}

impl WorkforceTab {
    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let input = ctx.input;
        let data = self.data.get(input, || {
            let state = input.session.state();
            let defs = input.session.defs();
            let workplaces = state
                .buildings()
                .iter()
                .filter_map(|(id, b)| {
                    let (category, wanted) = b.job_demand(defs)?;
                    let def = defs.buildings().get(b.kind().index())?;
                    Some(Workplace {
                        building: id,
                        category,
                        icon: format!("bld_{}", def.key),
                        name: def.name.clone(),
                        site: b.construction_site().is_some(),
                        staffed: b.staffed(),
                        wanted,
                        priority: b.priority(),
                    })
                })
                .collect();
            Data { summary: report::workforce_report(state, defs), workplaces }
        });
        let s = &data.summary;
        card_row(
            ui,
            &[
                ("Bewohner", s.population.to_string(), Palette::INK),
                ("Erwerbsfähig", s.working_age.to_string(), Palette::INK),
                ("Beschäftigt", s.employed.to_string(), Palette::GOOD),
                ("Arbeitslos", s.unemployed.to_string(), if s.unemployed > 0 { Palette::WARN } else { Palette::INK }),
                ("Offene Stellen", s.vacancies.to_string(), if s.vacancies > 0 { Palette::BAD } else { Palette::INK }),
            ],
        );
        ui.add_space(4.0);
        hint(
            ui,
            "Zuerst erhält jedes Lager einen Träger und jeder Markt einen Arbeiter. Danach werden die Bereiche nach ihrer Priorität besetzt, innerhalb eines Bereichs die Betriebe nach ihrer eigenen Priorität; Betriebe ohne Arbeit kommen zuletzt an die Reihe.",
        );
        section_header(ui, "Bereiche");
        const W: [f32; 6] = [170.0, 80.0, 80.0, 100.0, 220.0, 260.0];
        table::header(
            ui,
            &[
                ("Bereich", W[0], CellAlign::Left),
                ("Bedarf", W[1], CellAlign::Right),
                ("Besetzt", W[2], CellAlign::Right),
                ("Unbesetzt", W[3], CellAlign::Right),
                ("Besetzung", W[4], CellAlign::Left),
                ("Priorität", W[5], CellAlign::Left),
            ],
        );
        for (i, c) in s.categories.iter().enumerate() {
            table::row(ui, ("category", c.category.index()), ROW_HEIGHT + 4.0, i % 2 == 1, false, |ui| {
                table::text(ui, W[0], CellAlign::Left, RichText::new(c.category.display_name()).strong().color(Palette::INK));
                table::text(ui, W[1], CellAlign::Right, c.wanted.to_string());
                table::text(ui, W[2], CellAlign::Right, c.staffed.to_string());
                let unfilled = c.wanted.saturating_sub(c.staffed);
                table::text(ui, W[3], CellAlign::Right, RichText::new(unfilled.to_string()).color(if unfilled > 0 { Palette::BAD } else { Palette::INK_SOFT }));
                table::cell(ui, W[4], CellAlign::Left, |ui| {
                    ui.add_space(14.0);
                    if c.wanted == 0 {
                        ui.label(RichText::new("kein Bedarf").small().color(Palette::INK_SOFT));
                    } else {
                        let f = fill_fraction(c.staffed, c.wanted);
                        mini_bar(ui, f, if f >= 1.0 { Palette::GOOD } else if f > 0.5 { Palette::WARN } else { Palette::BAD }, 150.0);
                        ui.label(RichText::new(format!("{} %", (f * 100.0).round())).small().color(Palette::INK_SOFT));
                    }
                });
                table::cell(ui, W[5], CellAlign::Left, |ui| {
                    for p in PRIORITIES {
                        if ui
                            .selectable_label(c.priority == p, p.display_name())
                            .on_hover_text(format!("{}: Priorität {}", c.category.display_name(), p.display_name()))
                            .clicked()
                            && c.priority != p
                        {
                            command(actions, Command::SetWorkforcePriority { category: c.category, priority: p });
                        }
                        ui.add_space(4.0);
                    }
                });
            });
        }
        section_header(ui, "Betriebe und Baustellen");
        if data.workplaces.is_empty() {
            hint(ui, "Keine Arbeitsplätze.");
        }
        let mut toggle = None;
        for c in JobCategory::ALL {
            let places: Vec<&Workplace> = data.workplaces.iter().filter(|w| w.category == c).collect();
            if places.is_empty() {
                continue;
            }
            let open = self.open_categories.contains(&c);
            let staffed: u32 = places.iter().map(|w| u32::from(w.staffed)).sum();
            let wanted: u32 = places.iter().map(|w| u32::from(w.wanted)).sum();
            let response = table::row(ui, ("wp_group", c.index()), ROW_HEIGHT + 4.0, false, true, |ui| {
                table::cell(ui, 520.0, CellAlign::Left, |ui| {
                    super::production::disclosure(ui, !open);
                    ui.label(RichText::new(c.display_name()).font(heading_font(15.0)).color(Palette::INK));
                    ui.label(RichText::new(format!("· {} Gebäude, {staffed} von {wanted} besetzt", places.len())).small().color(Palette::INK_SOFT));
                });
            });
            if response.on_hover_text(if open { "Zuklappen" } else { "Aufklappen" }).clicked() {
                toggle = Some(c);
            }
            if !open {
                continue;
            }
            for (i, w) in places.iter().enumerate() {
                let response = table::row(ui, ("wp", w.building.index()), ROW_HEIGHT, i % 2 == 0, true, |ui| {
                    table::cell(ui, 300.0, CellAlign::Left, |ui| {
                        ui.add_space(22.0);
                        if let Some(icon) = ctx.icons.get(&w.icon) {
                            icon.show(ui, 20.0);
                        }
                        let name = if w.site { format!("{} (Baustelle)", w.name) } else { w.name.clone() };
                        ui.add(egui::Label::new(RichText::new(name).color(Palette::INK)).truncate());
                    });
                    let color = if w.staffed >= w.wanted { Palette::INK } else if w.staffed > 0 { Palette::WARN } else { Palette::BAD };
                    table::text(ui, 100.0, CellAlign::Right, RichText::new(format!("{} / {}", w.staffed, w.wanted)).color(color));
                    table::cell(ui, 280.0, CellAlign::Left, |ui| {
                        ui.add_space(20.0);
                        for p in PRIORITIES {
                            if ui.selectable_label(w.priority == p, p.display_name()).clicked() && w.priority != p {
                                command(actions, Command::SetProductionPriority { building: w.building, priority: p });
                            }
                            ui.add_space(4.0);
                        }
                    });
                });
                if response.on_hover_text("Klick: Gebäude auswählen und hinschauen").clicked() {
                    select(actions, w.building);
                }
            }
        }
        if let Some(c) = toggle {
            match self.open_categories.iter().position(|x| *x == c) {
                Some(i) => {
                    self.open_categories.remove(i);
                }
                None => self.open_categories.push(c),
            }
            actions.push(HudAction::Sound("ui_click"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_fractions() {
        assert_eq!(fill_fraction(0, 0), 1.0);
        assert_eq!(fill_fraction(1, 4), 0.25);
        assert_eq!(fill_fraction(5, 4), 1.0);
    }
}
