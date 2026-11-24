//! „Bevölkerung“: citizens by life stage and sex, households and housing, births, deaths and
//! migration today and over the recorded days, kingdom-wide need satisfaction, happiness with
//! its factors, holding a festival (`Command::HoldFestival` with its cost, effects and the
//! reason it cannot be held now, `report::festival_report`), the households' savings (wealth classes, income and spending), taverns with
//! their beer stock, every house with its tier, residents, unmet needs and upgrade blockers (a
//! click selects the house) and a searchable list of the citizens (a click opens the citizen
//! window).

use kingdoms_engine::egui::{self, RichText, Ui};
use kingdoms_sim::buildings::{BuildingId, UpgradeBlocker, upgrade_blockers};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::WealthRules;
use kingdoms_sim::population::{CitizenId, DayCounts, DeathCause, HappinessFactor, LifeStage, WealthClass, PopulationDay, TripPhase, activity_text};
use kingdoms_sim::report::{self, FestivalReport, HappinessReport, NeedsSummary, PopulationSummary};

use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, TabContext, card_row, command, day_label, day_title, hint, select};
use crate::ui::chart::{Chart, Series, ValueFormat};
use crate::ui::hud::HudAction;
use crate::ui::hud::format::{decimal, int};
use crate::ui::hud::info_panel::wealth_color;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Mark, marked, mini_bar, rating_color, section_header, stat_row};

/// One house of the list.
#[derive(Clone, Debug)]
struct House {
    building: BuildingId,
    icon: String,
    name: String,
    tier: u8,
    residents: usize,
    capacity: u16,
    happiness: u8,
    /// Needs below the satisfaction threshold.
    unmet: Vec<String>,
    /// Why the house cannot be upgraded now (empty: it can; `None`: highest tier).
    blockers: Option<Vec<String>>,
    downgrade_in_hours: Option<u16>,
}

struct Data {
    summary: PopulationSummary,
    needs: NeedsSummary,
    happiness: HappinessReport,
    festival: FestivalReport,
    houses: Vec<House>,
}

/// Which citizens the list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PeopleFilter {
    #[default]
    All,
    Unhappy,
    Jobless,
    Homeless,
    NoPath,
    OnTheWay,
}

impl PeopleFilter {
    pub const ALL: [PeopleFilter; 6] =
        [PeopleFilter::All, PeopleFilter::Unhappy, PeopleFilter::Jobless, PeopleFilter::Homeless, PeopleFilter::NoPath, PeopleFilter::OnTheWay];

    pub const fn label(self) -> &'static str {
        match self {
            PeopleFilter::All => "Alle",
            PeopleFilter::Unhappy => "Unzufrieden",
            PeopleFilter::Jobless => "Ohne Arbeit",
            PeopleFilter::Homeless => "Obdachlos",
            PeopleFilter::NoPath => "Ohne Weg",
            PeopleFilter::OnTheWay => "Unterwegs",
        }
    }
}

/// What the filters look at of one citizen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PersonFacts {
    pub unhappy: bool,
    pub jobless_adult: bool,
    pub homeless: bool,
    pub no_path: bool,
    pub walking: bool,
}

/// True if a citizen with `facts` named `name` passes `filter` and the search text `search`
/// (case-insensitive part of the name; empty: everybody).
pub fn person_matches(filter: PeopleFilter, facts: PersonFacts, name: &str, search: &str) -> bool {
    let passes = match filter {
        PeopleFilter::All => true,
        PeopleFilter::Unhappy => facts.unhappy,
        PeopleFilter::Jobless => facts.jobless_adult,
        PeopleFilter::Homeless => facts.homeless,
        PeopleFilter::NoPath => facts.no_path,
        PeopleFilter::OnTheWay => facts.walking,
    };
    let search = search.trim();
    passes && (search.is_empty() || name.to_lowercase().contains(&search.to_lowercase()))
}

/// Citizens listed at most (the list says how many more match).
const PEOPLE_LIMIT: usize = 150;

/// One citizen of the list.
#[derive(Clone, Debug)]
struct Person {
    id: CitizenId,
    name: String,
    age: u32,
    job: String,
    activity: String,
    happiness: u8,
    health: u8,
}

/// The citizens matching the filter (at most [`PEOPLE_LIMIT`]) and how many match in all.
struct People {
    rows: Vec<Person>,
    matching: usize,
}

/// State of the tab.
#[derive(Default)]
pub struct PopulationTab {
    data: Cached<Data>,
    filter: PeopleFilter,
    search: String,
    people: Cached<People>,
}

/// German text of a day's deaths by cause („2 (Alter 1, Hunger 1)“).
pub fn deaths_text(counts: &DayCounts) -> String {
    let total = counts.total_deaths();
    let causes: Vec<String> = DeathCause::ALL
        .iter()
        .filter(|c| counts.deaths_by(**c) > 0)
        .map(|c| format!("{} {}", c.display_name(), counts.deaths_by(*c)))
        .collect();
    if causes.is_empty() { total.to_string() } else { format!("{total} ({})", causes.join(", ")) }
}

impl PopulationTab {
    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let input = ctx.input;
        let data = self.data.get(input, || {
            let state = input.session.state();
            let defs = input.session.defs();
            let threshold = defs.rules().population.need_met_threshold;
            let houses = state
                .buildings()
                .iter()
                .filter_map(|(id, b)| {
                    let def = defs.buildings().get(b.kind().index())?;
                    def.housing.as_ref()?;
                    let house = report::house_report(state, defs, id)?;
                    let unmet = house
                        .needs
                        .iter()
                        .filter(|n| !house.residents.is_empty() && n.average < threshold)
                        .map(|n| n.label.clone())
                        .collect();
                    let blockers = upgrade_blockers(state, defs, id).and_then(|list| {
                        (!list.contains(&UpgradeBlocker::MaxTier)).then(|| list.iter().map(ToString::to_string).collect())
                    });
                    Some(House {
                        building: id,
                        icon: format!("bld_{}", def.key),
                        name: def.name.clone(),
                        tier: house.tier,
                        residents: house.residents.len(),
                        capacity: house.capacity,
                        happiness: house.happiness,
                        unmet,
                        blockers,
                        downgrade_in_hours: house.downgrade_in_hours,
                    })
                })
                .collect();
            Data {
                summary: report::population_summary(state, defs),
                needs: report::needs_summary(state, defs),
                happiness: report::happiness_report(state, defs),
                festival: report::festival_report(state, defs),
                houses,
            }
        });
        let s = &data.summary;
        card_row(
            ui,
            &[
                ("Bewohner", s.population.to_string(), Palette::INK),
                ("Haushalte", s.households.to_string(), Palette::INK),
                ("Wohnraum belegt", format!("{} / {}", s.housed, s.housing_capacity), Palette::INK),
                ("Obdachlos", s.homeless.to_string(), if s.homeless > 0 { Palette::BAD } else { Palette::INK }),
                ("Frei für Zuwanderer", s.immigrant_places.to_string(), if s.immigrant_places == 0 { Palette::WARN } else { Palette::INK }),
                ("Ø Zufriedenheit", s.average_happiness.to_string(), rating_color(f32::from(s.average_happiness))),
                ("Ø Gesundheit", s.average_health.to_string(), rating_color(f32::from(s.average_health))),
            ],
        );
        ui.add_space(6.0);
        ui.columns(2, |cols| {
            stages(&mut cols[0], s);
            movement(&mut cols[1], s);
        });
        section_header(ui, "Verlauf");
        ui.columns(2, |cols| {
            history_charts(cols, &s.history);
        });
        ui.columns(2, |cols| {
            needs(&mut cols[0], &data.needs, input.session.defs().rules().population.need_met_threshold);
            happiness(&mut cols[1], &data.happiness);
        });
        ui.add_space(4.0);
        festival_box(ui, ctx, &data.festival, actions);
        ui.columns(2, |cols| {
            let rules = &input.session.defs().rules().population.wealth;
            wealth(&mut cols[0], s, &data.happiness, rules);
            taverns(&mut cols[1], s.taverns.open, s.taverns.stocked);
        });
        houses(ui, ctx, &data.houses, actions);
        self.people(ui, ctx, actions);
    }

    /// The searchable list of citizens; a click opens the citizen window.
    fn people(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        section_header(ui, "Bürger");
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Zeigen").color(Palette::INK_SOFT));
            for f in PeopleFilter::ALL {
                if ui.selectable_label(self.filter == f, f.label()).clicked() && self.filter != f {
                    self.filter = f;
                    changed = true;
                }
            }
            ui.add_space(12.0);
            ui.label(RichText::new("Name").color(Palette::INK_SOFT));
            changed |= ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(140.0).hint_text("suchen …")).changed();
        });
        if changed {
            self.people.invalidate();
        }
        let input = ctx.input;
        let (filter, search) = (self.filter, self.search.clone());
        let people = self.people.get(input, || {
            let state = input.session.state();
            let defs = input.session.defs();
            let pop = state.population();
            let rules = &defs.rules().population;
            let day = state.clock().total_days();
            let mut rows = Vec::new();
            let mut matching = 0;
            for (id, c) in pop.citizens() {
                let Some(name) = pop.full_name(id) else { continue };
                let facts = PersonFacts {
                    unhappy: c.happiness() < rules.emigration_happiness,
                    jobless_adult: c.stage() == LifeStage::Adult && c.job().is_none(),
                    homeless: pop.household(c.household()).is_none_or(|h| h.home().is_none()),
                    no_path: matches!(c.motion().phase(), TripPhase::NoPath { .. }),
                    walking: c.motion().is_moving(),
                };
                if !person_matches(filter, facts, &name, &search) {
                    continue;
                }
                matching += 1;
                if rows.len() >= PEOPLE_LIMIT {
                    continue;
                }
                let workplace = c
                    .job()
                    .and_then(|j| state.building(j.building))
                    .and_then(|b| defs.buildings().get(b.kind().index()))
                    .map(|d| d.name.as_str());
                let job = job_column(c.stage(), workplace, c.is_on_sick_leave());
                rows.push(Person {
                    id,
                    name,
                    age: c.age_on(day),
                    job,
                    activity: activity_text(state, defs, id).unwrap_or_else(|| c.activity().display_name().to_owned()),
                    happiness: c.happiness(),
                    health: c.health(),
                });
            }
            People { rows, matching }
        });
        if people.rows.is_empty() {
            hint(ui, "Niemand passt zu dieser Auswahl.");
            return;
        }
        const W: [f32; 6] = [190.0, 50.0, 170.0, 330.0, 90.0, 80.0];
        table::header(
            ui,
            &[
                ("Name", W[0], CellAlign::Left),
                ("Alter", W[1], CellAlign::Right),
                ("Arbeit", W[2], CellAlign::Left),
                ("Tätigkeit", W[3], CellAlign::Left),
                ("Zufriedenheit", W[4], CellAlign::Right),
                ("Gesundheit", W[5], CellAlign::Right),
            ],
        );
        for (i, p) in people.rows.iter().enumerate() {
            let response = table::row(ui, ("person", p.id.index()), ROW_HEIGHT, i % 2 == 1, true, |ui| {
                table::text(ui, W[0], CellAlign::Left, RichText::new(&p.name).color(Palette::INK));
                table::text(ui, W[1], CellAlign::Right, p.age.to_string());
                table::text(ui, W[2], CellAlign::Left, RichText::new(&p.job).color(Palette::INK_SOFT));
                table::text(ui, W[3], CellAlign::Left, RichText::new(&p.activity).color(Palette::INK_SOFT));
                table::text(ui, W[4], CellAlign::Right, RichText::new(p.happiness.to_string()).color(rating_color(f32::from(p.happiness))));
                table::text(ui, W[5], CellAlign::Right, RichText::new(p.health.to_string()).color(rating_color(f32::from(p.health))));
            });
            if response.on_hover_text("Klick: Bürgerfenster öffnen").clicked() {
                actions.push(HudAction::InspectCitizen(p.id));
                actions.push(HudAction::Sound("ui_click"));
            }
        }
        if people.matching > people.rows.len() {
            hint(ui, format!("… und {} weitere – Auswahl oder Suche eingrenzen.", people.matching - people.rows.len()));
        }
    }
}

fn stages(ui: &mut Ui, s: &PopulationSummary) {
    ui.label(RichText::new("Lebensabschnitte").font(heading_font(16.0)).color(Palette::INK));
    let w = ui.available_width();
    let cols = [w - 3.0 * 80.0, 80.0, 80.0, 80.0];
    table::header(ui, &[("", cols[0], CellAlign::Left), ("Männer", cols[1], CellAlign::Right), ("Frauen", cols[2], CellAlign::Right), ("Gesamt", cols[3], CellAlign::Right)]);
    for (i, row) in s.stages.iter().enumerate() {
        table::row(ui, ("stage", i), ROW_HEIGHT - 2.0, i % 2 == 1, false, |ui| {
            table::text(ui, cols[0], CellAlign::Left, RichText::new(&row.label).color(Palette::INK));
            table::text(ui, cols[1], CellAlign::Right, row.male.to_string());
            table::text(ui, cols[2], CellAlign::Right, row.female.to_string());
            table::text(ui, cols[3], CellAlign::Right, RichText::new(row.total().to_string()).strong());
        });
    }
    table::rule(ui);
    egui::Grid::new("pop_work").num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
        crate::ui::widgets::stat_row(ui, "Erwerbsfähig", s.working_age.to_string());
        crate::ui::widgets::stat_row(ui, "Beschäftigt (mit Bauarbeitern)", s.employed.to_string());
        crate::ui::widgets::stat_row(ui, "Arbeitslos", s.unemployed.to_string());
        crate::ui::widgets::stat_row(ui, "Gebildet", s.educated.to_string());
    });
}

fn movement(ui: &mut Ui, s: &PopulationSummary) {
    ui.label(RichText::new("Bewegung").font(heading_font(16.0)).color(Palette::INK));
    let yesterday = s.history.last().map(|d| d.counts);
    let w = ui.available_width();
    let cols = [w - 2.0 * 150.0, 150.0, 150.0];
    table::header(ui, &[("", cols[0], CellAlign::Left), ("Heute", cols[1], CellAlign::Right), ("Gestern", cols[2], CellAlign::Right)]);
    type Row = (&'static str, fn(&DayCounts) -> String);
    let rows: [Row; 4] = [
        ("Geburten", |c| c.births.to_string()),
        ("Todesfälle", deaths_text),
        ("Zuwanderer", |c| c.immigrants.to_string()),
        ("Abwanderer", |c| c.emigrants.to_string()),
    ];
    for (i, (label, f)) in rows.iter().enumerate() {
        table::row(ui, ("movement", i), ROW_HEIGHT - 2.0, i % 2 == 1, false, |ui| {
            table::text(ui, cols[0], CellAlign::Left, RichText::new(*label).color(Palette::INK));
            table::text(ui, cols[1], CellAlign::Right, f(&s.today));
            table::text(ui, cols[2], CellAlign::Right, yesterday.as_ref().map_or_else(|| "–".to_owned(), f));
        });
    }
}

fn history_charts(cols: &mut [Ui], history: &[PopulationDay]) {
    let labels: Vec<String> = history.iter().map(|d| day_label(d.day)).collect();
    let titles: Vec<String> = history.iter().map(|d| day_title(d.day)).collect();
    let empty = "Der Verlauf beginnt nach dem ersten abgeschlossenen Tag.";
    if let Some(ui) = cols.get_mut(0) {
        ui.label(RichText::new("Einwohner am Tagesende").small().color(Palette::INK_SOFT));
        let series = vec![
            Series::line("Bewohner", Palette::INFO, history.iter().map(|d| f64::from(d.population)).collect()),
            Series::line("Obdachlos", Palette::BAD, history.iter().map(|d| f64::from(d.homeless)).collect()),
            Series::line("Arbeitslos", Palette::WARN, history.iter().map(|d| f64::from(d.unemployed)).collect()),
        ];
        Chart::new(("chart", "population_count"), &labels, &series).titles(&titles).height(170.0).empty_text(empty).show(ui);
    }
    if let Some(ui) = cols.get_mut(1) {
        ui.label(RichText::new("Geburten, Zuzug, Todesfälle und Fortzug je Tag").small().color(Palette::INK_SOFT));
        let series = vec![
            Series::bar("Geburten", Palette::GOOD, history.iter().map(|d| f64::from(d.counts.births)).collect()),
            Series::bar("Zuwanderer", Palette::INFO, history.iter().map(|d| f64::from(d.counts.immigrants)).collect()),
            Series::bar("Todesfälle", Palette::BAD, history.iter().map(|d| -f64::from(d.counts.total_deaths())).collect()),
            Series::bar("Abwanderer", Palette::WARN, history.iter().map(|d| -f64::from(d.counts.emigrants)).collect()),
        ];
        Chart::new(("chart", "population_moves"), &labels, &series)
            .titles(&titles)
            .height(170.0)
            .format(ValueFormat::Integer)
            .empty_text(empty)
            .show(ui);
    }
}

fn needs(ui: &mut Ui, needs: &NeedsSummary, threshold: u8) {
    ui.label(RichText::new("Bedürfnisse im Reich").font(heading_font(16.0)).color(Palette::INK));
    ui.label(RichText::new(format!("Mittelwert aller Bewohner · Anteil mit mindestens {threshold} Punkten")).small().color(Palette::INK_SOFT));
    egui::Grid::new("kingdom_needs").num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
        for n in &needs.needs {
            ui.label(RichText::new(&n.label).color(Palette::INK));
            mini_bar(ui, f32::from(n.average) / 100.0, rating_color(f32::from(n.average)), 140.0).on_hover_text(format!("{}: {} von 100", n.label, n.average));
            ui.label(RichText::new(format!("{} · {} % versorgt", n.average, n.satisfied_percent)).small().color(Palette::INK_SOFT));
            ui.end_row();
        }
    });
}

fn happiness(ui: &mut Ui, h: &HappinessReport) {
    ui.label(RichText::new("Zufriedenheit").font(heading_font(16.0)).color(Palette::INK));
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} von 100", h.average)).font(egui::FontId::proportional(20.0)).strong().color(rating_color(f32::from(h.average))));
        ui.label(RichText::new(format!("· {} unzufrieden (drohen abzuwandern) · {} glücklich", h.unhappy, h.happy)).small().color(Palette::INK_SOFT));
    });
    let mut factors: Vec<_> = h.factors.iter().collect();
    factors.sort_by(|a, b| b.points.abs().total_cmp(&a.points.abs()));
    let max = factors.iter().map(|f| f.points.abs()).fold(1.0f32, f32::max);
    egui::Grid::new("happiness_factors").num_columns(3).spacing([10.0, 3.0]).show(ui, |ui| {
        for f in factors {
            ui.label(RichText::new(&f.label).color(Palette::INK));
            let color = if f.points >= 0.0 { Palette::GOOD } else { Palette::BAD };
            mini_bar(ui, f.points.abs() / max, color, 120.0);
            let sign = if f.points > 0.0 { "+" } else { "" };
            ui.label(RichText::new(format!("{sign}{}", decimal(f64::from(f.points), 1))).color(color));
            ui.end_row();
        }
    });
    if h.factors.is_empty() {
        hint(ui, "Keine Bewohner.");
    }
}

/// Label of the festival button with the current cost („Fest ausrichten (1.060 Gold)“).
pub fn festival_button_label(cost: u32) -> String {
    format!("Fest ausrichten ({} Gold)", int(i64::from(cost)))
}

/// The state line under the festival (text, true if it is a problem): a festival waiting in the
/// command queue; while the town celebrates, from when the next one is possible; else why it
/// cannot be held now (with the day of the last festival when it waits for its turn) or when
/// the town last celebrated.
pub fn festival_status(f: &FestivalReport, queued: bool) -> Option<(String, bool)> {
    if queued {
        return Some(("Das Fest wird ausgerichtet, sobald das Spiel weiterläuft.".to_owned(), false));
    }
    if f.active {
        return f.next_day.map(|d| (format!("Das nächste Fest ist ab {} möglich.", day_title(d)), false));
    }
    let last = f.last_day.map(|d| format!("Zuletzt gefeiert: {}.", day_title(d)));
    match (&f.problem, last) {
        (Some(problem), Some(last)) if f.next_day.is_some() => Some((format!("{problem} {last}"), true)),
        (Some(problem), _) => Some((problem.clone(), true)),
        (None, last) => last.map(|l| (l, false)),
    }
}

/// „Fest ausrichten“: what a festival costs and brings now, the button that sends
/// `Command::HoldFestival` (disabled with the simulation's reason while it cannot be held)
/// and when the town last celebrated.
fn festival_box(ui: &mut Ui, ctx: &TabContext<'_, '_>, f: &FestivalReport, actions: &mut Vec<HudAction>) {
    let input = ctx.input;
    let rules = &input.session.defs().rules().festival;
    let queued = input.session.queued_commands().iter().any(|c| matches!(c, Command::HoldFestival));
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Fest ausrichten").font(heading_font(16.0)).color(Palette::INK));
            if f.active {
                ui.label(RichText::new("· Heute wird gefeiert").color(Palette::GOOD));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = f.problem.is_none() && !queued;
                let button = ui.add_enabled(enabled, egui::Button::new(RichText::new(festival_button_label(f.cost)).strong()));
                let button = match (&f.problem, queued) {
                    (_, true) => button.on_disabled_hover_text("Das Fest ist schon angeordnet."),
                    (Some(problem), false) => button.on_disabled_hover_text(problem.as_str()),
                    (None, false) => button.on_hover_text(f.effects.join("\n")),
                };
                if button.clicked() {
                    command(actions, Command::HoldFestival);
                }
            });
        });
        ui.label(
            RichText::new(format!(
                "Kosten: {} Gold ({} Gold und {} Gold je Einwohner, gebucht unter „Ereignisse“) · gefeiert wird an Kirche oder Marktplatz",
                int(i64::from(f.cost)),
                rules.gold_base,
                rules.gold_per_inhabitant
            ))
            .small()
            .color(Palette::INK_SOFT),
        );
        for effect in &f.effects {
            marked(ui, Mark::Next, RichText::new(effect).color(Palette::INK));
        }
        if let Some((text, problem)) = festival_status(f, queued) {
            ui.label(RichText::new(text).color(if problem { Palette::WARN } else { Palette::INK_SOFT }));
        }
    });
}

/// Savings of the households: distribution over the wealth classes, totals, yesterday's
/// income and spending and the happiness factor „Wohlstand“.
fn wealth(ui: &mut Ui, s: &PopulationSummary, h: &HappinessReport, rules: &WealthRules) {
    let w = &s.wealth;
    ui.label(RichText::new("Wohlstand der Haushalte").font(heading_font(16.0)).color(Palette::INK));
    let classes: Vec<(WealthClass, &str, u32)> = w.classes.iter().map(|c| (c.class, c.label.as_str(), c.households)).collect();
    let total: u32 = classes.iter().map(|c| c.2).sum();
    if total == 0 {
        hint(ui, "Noch keine Haushalte.");
        return;
    }
    wealth_bar(ui, &classes, total);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for (class, label, n) in &classes {
            ui.label(RichText::new(format!("{label}: {n}")).color(wealth_color(*class)));
        }
    });
    egui::Grid::new("wealth_rows").num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
        stat_row(ui, "Ersparnisse gesamt", format!("{} Gold", int(w.total_savings)));
        stat_row(ui, "Ø je Haushalt", format!("{} Gold (Median {})", int(w.average_savings), int(w.median_savings)));
        stat_row(ui, "Gestern verdient", format!("{} Gold", int(w.earned_yesterday)));
        stat_row(ui, "Gestern ausgegeben", format!("{} Gold", int(w.spent_yesterday)));
        if let Some(f) = h.factors.iter().find(|f| f.factor == HappinessFactor::Wealth) {
            let color = if f.points < 0.0 { Palette::BAD } else { Palette::GOOD };
            let sign = if f.points > 0.0 { "+" } else { "" };
            ui.label(RichText::new(format!("Zufriedenheit: {}", f.label)).color(Palette::INK_SOFT));
            ui.label(RichText::new(format!("{sign}{}", decimal(f64::from(f.points), 1))).color(color));
            ui.end_row();
        }
    });
    hint(
        ui,
        format!(
            "Arm unter {} Gold je Person ({} Zufriedenheit), wohlhabend ab {} Gold ({}). Ausgaben: Steuern, Marktgebühren und Einkäufe zum Marktpreis.",
            decimal(f64::from(rules.poor_savings_per_member), 0),
            crate::ui::hud::format::signed(i64::from(rules.poverty_happiness)),
            decimal(f64::from(rules.wealthy_savings_per_member), 0),
            crate::ui::hud::format::signed(i64::from(rules.wealth_happiness))
        ),
    );
}

/// One bar split into the shares of the wealth classes.
fn wealth_bar(ui: &mut Ui, classes: &[(WealthClass, &str, u32)], total: u32) {
    let width = ui.available_width().min(420.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 14.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, Palette::INK_SOFT.gamma_multiply(0.2));
    let mut x = rect.left();
    for (class, _, n) in classes {
        let w = rect.width() * *n as f32 / total as f32;
        if w > 0.0 {
            let part = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, rect.height()));
            let fill = match class {
                WealthClass::Poor => Palette::BAD,
                WealthClass::Modest => Palette::GOLD,
                WealthClass::Wealthy => Palette::GOOD,
            };
            painter.rect_filled(part, 0.0, fill);
            x += w;
        }
    }
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, Palette::INK_SOFT), egui::StrokeKind::Inside);
    response.on_hover_ui(|ui| {
        for (_, label, n) in classes {
            ui.label(format!("{label}: {} ({} %)", format_households(*n), n * 100 / total.max(1)));
        }
    });
}

/// „1 Haushalt“, „3 Haushalte“.
fn format_households(n: u32) -> String {
    crate::ui::hud::format::count(n, "Haushalt", "Haushalte")
}

/// Taverns with their beer stock (staffed and open; stocked with everything they sell).
fn taverns(ui: &mut Ui, open: u32, stocked: u32) {
    ui.label(RichText::new("Gasthäuser").font(heading_font(16.0)).color(Palette::INK));
    if open == 0 {
        hint(ui, "Kein geöffnetes Gasthaus. Ein Gasthaus mit Wirt und Bier hebt Unterhaltung und Gemeinschaft.");
        return;
    }
    let empty = open - stocked.min(open);
    egui::Grid::new("tavern_rows").num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
        stat_row(ui, "Geöffnet", open.to_string());
        stat_row(ui, "Mit Bier", stocked.to_string());
        ui.label(RichText::new("Ohne Bier").color(Palette::INK_SOFT));
        ui.label(RichText::new(empty.to_string()).color(if empty > 0 { Palette::BAD } else { Palette::INK }));
        ui.end_row();
    });
    if empty > 0 {
        hint(ui, "Ohne Bier bieten Gasthäuser weniger Unterhaltung – Brauerei und Träger prüfen.");
    }
}

fn houses(ui: &mut Ui, ctx: &TabContext<'_, '_>, houses: &[House], actions: &mut Vec<HudAction>) {
    section_header(ui, "Wohnhäuser");
    if houses.is_empty() {
        hint(ui, "Noch keine fertigen Wohnhäuser.");
        return;
    }
    const W: [f32; 6] = [200.0, 60.0, 90.0, 110.0, 220.0, 300.0];
    table::header(
        ui,
        &[
            ("Haus", W[0], CellAlign::Left),
            ("Stufe", W[1], CellAlign::Right),
            ("Bewohner", W[2], CellAlign::Right),
            ("Zufriedenheit", W[3], CellAlign::Right),
            ("Unerfüllte Bedürfnisse", W[4], CellAlign::Left),
            ("Ausbau", W[5], CellAlign::Left),
        ],
    );
    for (i, h) in houses.iter().enumerate() {
        let response = table::row(ui, ("house", h.building.index()), ROW_HEIGHT, i % 2 == 1, true, |ui| {
            table::cell(ui, W[0], CellAlign::Left, |ui| {
                if let Some(icon) = ctx.icons.get(&h.icon) {
                    icon.show(ui, 20.0);
                }
                ui.add(egui::Label::new(RichText::new(&h.name).color(Palette::INK)).truncate());
            });
            table::text(ui, W[1], CellAlign::Right, h.tier.to_string());
            let full = h.residents >= usize::from(h.capacity);
            table::text(ui, W[2], CellAlign::Right, RichText::new(format!("{} / {}", h.residents, h.capacity)).color(if full { Palette::INK } else { Palette::INK_SOFT }));
            let happy = if h.residents == 0 { RichText::new("–").color(Palette::INK_SOFT) } else { RichText::new(h.happiness.to_string()).color(rating_color(f32::from(h.happiness))) };
            table::text(ui, W[3], CellAlign::Right, happy);
            table::cell(ui, W[4], CellAlign::Left, |ui| {
                ui.add_space(12.0);
                let (text, color) = match (h.downgrade_in_hours, h.unmet.is_empty()) {
                    (Some(hours), _) => {
                        (format!("Abstieg in {}: {}", crate::ui::hud::format::duration_minutes(u64::from(hours) * 60), h.unmet.join(", ")), Palette::BAD)
                    }
                    (None, true) => ("–".to_owned(), Palette::INK_SOFT),
                    (None, false) => (h.unmet.join(", "), Palette::WARN),
                };
                ui.add(egui::Label::new(RichText::new(text).color(color)).truncate());
            });
            table::cell(ui, W[5], CellAlign::Left, |ui| {
                let (text, color) = match &h.blockers {
                    None => ("höchste Stufe".to_owned(), Palette::INK_SOFT),
                    Some(list) if list.is_empty() => ("möglich".to_owned(), Palette::GOOD),
                    Some(list) => (list.first().cloned().unwrap_or_default(), Palette::INK_SOFT),
                };
                ui.add(egui::Label::new(RichText::new(text).small().color(color)).truncate());
            });
        });
        let response = response.on_hover_ui(|ui| {
            ui.label(RichText::new(&h.name).strong());
            if !h.unmet.is_empty() {
                ui.label(RichText::new(format!("Unerfüllt: {}", h.unmet.join(", "))).color(Palette::WARN));
            }
            if let Some(list) = h.blockers.as_ref().filter(|l| !l.is_empty()) {
                ui.label(RichText::new("Ausbau nicht möglich:").color(Palette::INK_SOFT));
                for b in list {
                    ui.label(format!("· {b}"));
                }
            }
            ui.label(RichText::new("Klick: Haus auswählen und hinschauen").small().color(Palette::INK_SOFT));
        });
        if response.clicked() {
            select(actions, h.building);
        }
    }
}

/// The „Arbeit“ column of the citizen list: the workplace, „arbeitslos“ (or „krankgeschrieben“)
/// for an adult without one, else the life stage („Kind“, „Alt“).
fn job_column(stage: LifeStage, workplace: Option<&str>, sick_leave: bool) -> String {
    match (workplace, stage) {
        (Some(name), _) => name.to_owned(),
        (None, LifeStage::Adult) if sick_leave => "krankgeschrieben".to_owned(),
        (None, LifeStage::Adult) => "arbeitslos".to_owned(),
        (None, stage) => stage.display_name().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn festival(problem: Option<&str>, next_day: Option<u64>, last_day: Option<u64>) -> FestivalReport {
        FestivalReport {
            cost: 1060,
            problem: problem.map(str::to_owned),
            next_day,
            last_day,
            active: false,
            cost_text: "1060 Gold (100 Gold und 4 Gold je Einwohner)".to_owned(),
            effects: vec!["Zufriedenheit +8 für 6 Tage".to_owned()],
        }
    }

    #[test]
    fn festival_button_and_state_line() {
        assert_eq!(festival_button_label(1060), "Fest ausrichten (1.060 Gold)");
        assert_eq!(festival_button_label(260), "Fest ausrichten (260 Gold)");
        // Never held and possible: nothing to say.
        assert_eq!(festival_status(&festival(None, None, None), false), None);
        // Held before, possible again: when it was.
        assert_eq!(
            festival_status(&festival(None, None, Some(3)), false),
            Some(("Zuletzt gefeiert: Frühling · Tag 4 · Jahr 1.".to_owned(), false))
        );
        // Waiting for its turn: the reason with the last festival.
        let waiting = festival(Some("Das nächste Fest ist in 10 Tagen möglich (Herbst · Tag 1 · Jahr 1)."), Some(12), Some(0));
        assert_eq!(
            festival_status(&waiting, false),
            Some((
                "Das nächste Fest ist in 10 Tagen möglich (Herbst · Tag 1 · Jahr 1). Zuletzt gefeiert: Frühling · Tag 1 · Jahr 1.".to_owned(),
                true
            ))
        );
        // Other reasons stand alone.
        let poor = festival(Some("Es fehlen 160 Gold."), None, Some(0));
        assert_eq!(festival_status(&poor, false), Some(("Es fehlen 160 Gold.".to_owned(), true)));
        // While the town celebrates: from when the next one is possible.
        let mut feast = festival(Some("Es wird bereits gefeiert."), Some(14), Some(2));
        feast.active = true;
        assert_eq!(festival_status(&feast, false), Some(("Das nächste Fest ist ab Herbst · Tag 3 · Jahr 1 möglich.".to_owned(), false)));
        feast.next_day = None;
        assert_eq!(festival_status(&feast, false), None, "a festival of an event: the heading says it");
        // A festival waiting in the command queue (paused game).
        assert_eq!(festival_status(&poor, true), Some(("Das Fest wird ausgerichtet, sobald das Spiel weiterläuft.".to_owned(), false)));
    }

    #[test]
    fn people_filters_and_search() {
        let idle = PersonFacts::default();
        let lost = PersonFacts { no_path: true, walking: false, ..PersonFacts::default() };
        assert!(person_matches(PeopleFilter::All, idle, "Anna Müller", ""));
        assert!(!person_matches(PeopleFilter::NoPath, idle, "Anna Müller", ""));
        assert!(person_matches(PeopleFilter::NoPath, lost, "Anna Müller", ""));
        // The search is a case-insensitive part of the name, umlauts included.
        assert!(person_matches(PeopleFilter::All, idle, "Anna Müller", " müll "));
        assert!(person_matches(PeopleFilter::All, idle, "Anna Müller", "MÜLLER"));
        assert!(!person_matches(PeopleFilter::All, idle, "Anna Müller", "Schmidt"));
        let homeless = PersonFacts { homeless: true, jobless_adult: true, ..PersonFacts::default() };
        assert!(person_matches(PeopleFilter::Homeless, homeless, "x", "") && person_matches(PeopleFilter::Jobless, homeless, "x", ""));
        assert!(!person_matches(PeopleFilter::Unhappy, homeless, "x", ""));
    }

    #[test]
    fn job_column_names_the_unemployed() {
        assert_eq!(job_column(LifeStage::Adult, Some("Holzfällerhütte"), false), "Holzfällerhütte");
        assert_eq!(job_column(LifeStage::Adult, None, false), "arbeitslos");
        assert_eq!(job_column(LifeStage::Adult, None, true), "krankgeschrieben");
        assert_eq!(job_column(LifeStage::Child, None, false), "Kind");
        assert_eq!(job_column(LifeStage::Elder, None, false), "Alt");
    }

    #[test]
    fn deaths_by_cause() {
        let mut c = DayCounts::default();
        assert_eq!(deaths_text(&c), "0");
        c.deaths[DeathCause::OldAge.index()] = 1;
        c.deaths[DeathCause::Starvation.index()] = 2;
        assert_eq!(deaths_text(&c), "3 (Alter 1, Hunger 2)");
    }
}
