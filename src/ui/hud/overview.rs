//! „Übersicht“ (right panel): the kingdom at a glance – population, happiness, treasury trend,
//! food reserve, research, the ruler's rank, threats (wars, approaching enemies, proposals
//! waiting, the event decision, money and food) and the latest events – each line from the
//! simulation's reports, each with a link to the window, place or dialog that explains it.

use glam::IVec2;
use kingdoms_engine::egui::{self, RichText, Sense, Ui};
use kingdoms_sim::buildings::{BuildingId, BuildingStatus};
use kingdoms_sim::economy::finance;
use kingdoms_sim::event::Severity;
use kingdoms_sim::events::ModifierKind;
use kingdoms_sim::report::{self, TechState};
use kingdoms_sim::diplomacy::KingdomId;
use kingdoms_sim::research::ResearchStatus;
use kingdoms_sim::state::GameState;

use super::feed::severity_color;
use super::format::{count, decimal, int, signed_gold_milli};
use super::management::Tab;
use super::{HudAction, Icons};
use crate::ui::theme::{Palette, heading_font};
use crate::ui::widgets::{Mark, mark, section_header};

/// Days of food below which the reserve is a threat.
const FOOD_WARNING_DAYS: f32 = 3.0;

/// Where a line of the overview leads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Window(Tab),
    Building(BuildingId),
    /// The decision dialog of the pending event.
    EventDialog,
    /// The diplomacy tab at a kingdom.
    Kingdom(KingdomId),
    /// The decision dialog of a diplomatic proposal.
    Proposal(u32),
    /// A place on the map.
    Tile(IVec2),
    /// The Aufgaben panel.
    Tasks,
}

/// A danger or problem worth the player's attention.
#[derive(Clone, Debug, PartialEq)]
pub struct Threat {
    pub severity: Severity,
    pub text: String,
    pub target: Option<Target>,
}

/// A decided event.
#[derive(Clone, Debug, PartialEq)]
pub struct RecentEvent {
    pub title: String,
    pub severity: Severity,
    pub date: String,
    pub choice: String,
    pub by_default: bool,
}

/// Everything the overview shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Overview {
    pub population: u32,
    pub capacity: u32,
    /// Births and arrivals minus deaths and departures today.
    pub change_today: i64,
    pub happiness: u8,
    pub unhappy: u32,
    /// The factor that costs the most happiness.
    pub worst_factor: Option<(String, f32)>,
    pub treasury: i64,
    pub net_milli: i64,
    pub food_stock: f32,
    pub food_days: Option<f32>,
    pub food_summary: String,
    pub research: String,
    /// Rank name and the next rank with its progress in percent.
    pub rank: (String, Option<(String, u8)>),
    /// Festival days from an event, with the time left („Festtag (Religiöses Fest), noch 1 Tag“).
    pub festival: Option<String>,
    pub threats: Vec<Threat>,
    pub recent: Vec<RecentEvent>,
}

impl Overview {
    pub fn compute(state: &GameState, defs: &kingdoms_sim::defs::Defs) -> Self {
        let population = report::population_summary(state, defs);
        let happiness = report::happiness_report(state, defs);
        let rates = finance::daily_rates(state, defs);
        let food = report::food_forecast(state, defs);
        let research = report::research_report(state, defs);
        let events = report::events_report(state, defs);
        let treasury = state.economy().treasury();
        let today = &population.today;
        let change_today = i64::from(today.births) + i64::from(today.immigrants) - i64::from(today.total_deaths()) - i64::from(today.emigrants);
        let worst_factor = happiness
            .factors
            .iter()
            .filter(|f| f.points < 0.0)
            .min_by(|a, b| a.points.total_cmp(&b.points))
            .map(|f| (f.label.clone(), f.points));
        let research_line = match research.status {
            ResearchStatus::Idle if research.techs.iter().all(|t| t.state == TechState::Researched) => {
                "Alle Technologien sind erforscht".to_owned()
            }
            ResearchStatus::Idle => "Keine Forschung ausgewählt".to_owned(),
            ResearchStatus::WaitingForGold { tech, missing } => {
                let name = research.techs.iter().find(|t| t.tech == tech).map_or("", |t| t.name.as_str());
                format!("„{name}“ wartet auf {missing} Gold")
            }
            ResearchStatus::Researching(tech) => match research.techs.iter().find(|t| t.tech == tech) {
                Some(t) => format!("{} · {} %", t.name, t.progress_percent),
                None => String::new(),
            },
        };
        let mut threats = Vec::new();
        // War and enemies first: they decide the kingdom's fate.
        for w in report::war_status(state, defs) {
            threats.push(Threat {
                severity: Severity::Danger,
                text: if w.days == 0 { format!("Krieg mit {} seit heute", w.name) } else { format!("Krieg mit {} seit {}", w.name, count(w.days, "Tag", "Tagen")) },
                target: Some(Target::Kingdom(w.kingdom)),
            });
        }
        for t in report::threats_report(state, defs) {
            let text = match t.eta_ticks {
                Some(_) => format!("{} ({} Kämpfer) erreicht Euch {} – Ziel: {}", t.kind_text, t.size, t.eta_text, t.target_name),
                None => format!("{} ({} Kämpfer) auf Eurem Land: {}", t.kind_text, t.size, t.stage_text),
            };
            threats.push(Threat { severity: Severity::Danger, text, target: Some(Target::Tile(t.position)) });
        }
        if state.military().is_unpaid() {
            threats.push(Threat {
                severity: Severity::Warning,
                text: "Der Sold der Soldaten bleibt aus – ihre Moral sinkt".to_owned(),
                target: Some(Target::Window(Tab::Military)),
            });
        }
        for p in report::pending_proposals(state, defs) {
            threats.push(Threat {
                severity: Severity::Warning,
                text: format!("{}: {} ({})", p.kingdom_name, p.title, p.remaining_text),
                target: Some(Target::Proposal(p.serial)),
            });
        }
        if let Some(p) = &events.pending {
            threats.push(Threat {
                severity: if severity_rank(p.severity) < severity_rank(Severity::Warning) { Severity::Warning } else { p.severity },
                text: format!("„{}“ wartet auf Eure Entscheidung ({})", p.title, super::event_dialog::countdown_text(p.minutes_left)),
                target: Some(Target::EventDialog),
            });
        }
        if state.economy().is_bankrupt() {
            threats.push(Threat {
                severity: Severity::Danger,
                text: "Bankrott: keine neuen Bauten, Produktion verlangsamt".to_owned(),
                target: Some(Target::Window(Tab::Finance)),
            });
        } else if rates.net() < 0 && treasury >= 0 {
            let days = treasury as f64 / (-rates.net() as f64 / 1000.0);
            if days < 5.0 {
                threats.push(Threat {
                    severity: Severity::Warning,
                    text: format!("Die Schatzkammer reicht nur noch für etwa {}", count(days.round().max(1.0) as u64, "Tag", "Tage")),
                    target: Some(Target::Window(Tab::Finance)),
                });
            }
        }
        if let Some(days) = food.days_of_stock.filter(|d| *d < FOOD_WARNING_DAYS) {
            threats.push(Threat {
                severity: if days < 1.0 { Severity::Danger } else { Severity::Warning },
                text: if days < 1.0 {
                    "Die Nahrung reicht keinen Tag mehr".to_owned()
                } else {
                    format!("Die Nahrung reicht nur noch für etwa {}", count(days.round().max(1.0) as u64, "Tag", "Tage"))
                },
                target: Some(Target::Window(Tab::Agriculture)),
            });
        }
        let ruins: Vec<(BuildingId, IVec2)> = state
            .buildings()
            .iter()
            .filter(|(_, b)| b.status() == BuildingStatus::Damaged)
            .map(|(id, b)| (id, b.rect().origin + b.rect().size / 2))
            .collect();
        if let Some((first, _)) = ruins.first() {
            threats.push(Threat {
                severity: Severity::Warning,
                text: match ruins.len() {
                    1 => "1 Gebäude liegt in Trümmern und wird wiederaufgebaut".to_owned(),
                    n => format!("{n} Gebäude liegen in Trümmern und werden wiederaufgebaut"),
                },
                target: Some(Target::Building(*first)),
            });
        }
        for m in events.modifiers.iter().filter(|m| m.amount < 0) {
            let tab = match m.kind {
                ModifierKind::Happiness | ModifierKind::Festival => Tab::Population,
                ModifierKind::Production => Tab::Production,
                ModifierKind::FieldYield => Tab::Agriculture,
            };
            threats.push(Threat { severity: Severity::Warning, text: m.text.clone(), target: Some(Target::Window(tab)) });
        }
        if let Some((max, until)) = &events.tax_cap {
            threats.push(Threat {
                severity: Severity::Info,
                text: format!("Steuersatz höchstens {max} % bis {until}"),
                target: Some(Target::Window(Tab::Finance)),
            });
        }
        if population.homeless > 0 {
            threats.push(Threat {
                severity: Severity::Warning,
                text: if population.homeless == 1 { "1 Bewohner ist obdachlos".to_owned() } else { format!("{} Bewohner sind obdachlos", population.homeless) },
                target: Some(Target::Window(Tab::Population)),
            });
        }
        if happiness.unhappy > 0 && happiness.average < 50 {
            threats.push(Threat {
                severity: Severity::Warning,
                text: if happiness.unhappy == 1 { "1 Bewohner ist unzufrieden".to_owned() } else { format!("{} Bewohner sind unzufrieden", happiness.unhappy) },
                target: Some(Target::Window(Tab::Population)),
            });
        }
        // Nothing to warn about once every technology is researched.
        let all_researched = research.techs.iter().all(|t| t.state == TechState::Researched);
        if matches!(research.status, ResearchStatus::Idle) && !all_researched {
            threats.push(Threat {
                severity: Severity::Info,
                text: "Die Forschung steht still – die Forschungspunkte verfallen".to_owned(),
                target: Some(Target::Window(Tab::Research)),
            });
        }
        threats.sort_by_key(|t| std::cmp::Reverse(severity_rank(t.severity)));
        let recent = events
            .history
            .iter()
            .take(4)
            .map(|h| RecentEvent { title: h.title.clone(), severity: h.severity, date: h.date.clone(), choice: h.choice.clone(), by_default: h.by_default })
            .collect();
        let ranks = report::rank_report(state, defs);
        let rank = (ranks.name.clone(), ranks.next.map(|n| (n.name, n.percent)));
        Self {
            rank,
            population: population.population,
            capacity: population.housing_capacity,
            change_today,
            happiness: happiness.average,
            unhappy: happiness.unhappy,
            worst_factor,
            treasury,
            net_milli: rates.net(),
            food_stock: food.in_stock,
            food_days: food.days_of_stock,
            food_summary: food.summary,
            research: research_line,
            festival: events.modifiers.iter().find(|m| m.kind == ModifierKind::Festival).map(|m| m.text.clone()),
            threats,
            recent,
        }
    }
}

fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Danger => 3,
        Severity::Warning => 2,
        Severity::Info => 1,
        Severity::Good => 0,
    }
}

/// Draws the overview; a click on a line follows its target.
pub fn show(ui: &mut Ui, icons: &Icons, data: &Overview, actions: &mut Vec<HudAction>) {
    let change = match data.change_today {
        0 => String::new(),
        n => format!(" ({} heute)", super::format::signed(n)),
    };
    let mut line = |ui: &mut Ui, icon: &str, caption: &str, value: String, detail: Option<String>, target: Target| {
        let response = ui
            .horizontal(|ui| {
                if let Some(icon) = icons.get(icon) {
                    icon.show(ui, 22.0);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(caption).small().color(Palette::INK_SOFT));
                    ui.label(RichText::new(value).strong().color(Palette::INK));
                    if let Some(detail) = detail {
                        ui.add(egui::Label::new(RichText::new(detail).small().color(Palette::INK_SOFT)).wrap());
                    }
                });
            })
            .response
            .interact(Sense::click());
        if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            follow(target, actions);
        }
    };
    section_header(ui, "Reich");
    line(
        ui,
        "ui_population",
        "Bevölkerung",
        format!("{} Bewohner{change}", int(i64::from(data.population))),
        Some(format!("Wohnraum für {}", int(i64::from(data.capacity)))),
        Target::Window(Tab::Population),
    );
    let worst = data.worst_factor.as_ref().map(|(label, points)| format!("Größte Last: {label} ({})", decimal(f64::from(*points), 1)));
    line(
        ui,
        "ui_happiness",
        "Zufriedenheit",
        format!("{} von 100 · {} unzufrieden", data.happiness, data.unhappy),
        worst,
        Target::Window(Tab::Population),
    );
    if let Some(festival) = data.festival.as_ref() {
        line(
            ui,
            "ui_happiness",
            "Festtag",
            festival.clone(),
            Some("Die Bewohner feiern nachmittags bei der Kirche oder auf dem Markt statt zu arbeiten.".to_owned()),
            Target::Window(Tab::Population),
        );
    }
    let trend = if data.net_milli >= 0 { "steigt" } else { "sinkt" };
    line(
        ui,
        "ui_gold",
        "Schatzkammer",
        format!("{} Gold", int(data.treasury)),
        Some(if data.net_milli == 0 {
            "ausgeglichen".to_owned()
        } else {
            format!("{trend} um {} Gold pro Tag", signed_gold_milli(data.net_milli.abs()).trim_start_matches('+'))
        }),
        Target::Window(Tab::Finance),
    );
    let days = data.food_days.map_or_else(|| "niemand isst".to_owned(), food_days_text);
    line(
        ui,
        "ui_food",
        "Nahrung",
        format!("{} Sättigung · {days}", int(data.food_stock.round() as i64)),
        Some(data.food_summary.clone()),
        Target::Window(Tab::Agriculture),
    );
    line(ui, "ui_research", "Forschung", data.research.clone(), None, Target::Window(Tab::Research));
    let (rank, next) = &data.rank;
    let detail = next.as_ref().map(|(name, percent)| format!("Nächster Rang „{name}“ zu {percent} % erreicht"));
    line(ui, super::tasks::RANK_ICON, "Herrscherrang", rank.clone(), detail, Target::Tasks);
    section_header(ui, "Gefahren");
    if data.threats.is_empty() {
        ui.horizontal(|ui| {
            mark(ui, Mark::Ok);
            ui.label(RichText::new("Keine Gefahren in Sicht.").color(Palette::GOOD));
        });
    }
    for threat in &data.threats {
        let response = ui
            .horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                mark(
                    ui,
                    match threat.severity {
                        Severity::Danger => Mark::Error,
                        Severity::Warning => Mark::Warning,
                        Severity::Info | Severity::Good => Mark::Next,
                    },
                );
                ui.add(egui::Label::new(RichText::new(&threat.text).color(Palette::INK)).wrap());
            })
            .response
            .interact(Sense::click());
        if let Some(target) = threat.target
            && response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
        {
            follow(target, actions);
        }
    }
    section_header(ui, "Letzte Ereignisse");
    if data.recent.is_empty() {
        ui.label(RichText::new("Noch keine Ereignisse.").italics().color(Palette::INK_SOFT));
    }
    for e in &data.recent {
        ui.horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0, 30.0), Sense::hover());
            ui.painter().rect_filled(rect, egui::CornerRadius::same(2), severity_color(e.severity));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(&e.title).font(heading_font(14.0)).color(Palette::INK));
                let how = if e.by_default { "ohne Entscheidung" } else { "entschieden" };
                ui.label(RichText::new(format!("{} · {how}: {}", e.date, e.choice)).small().color(Palette::INK_SOFT));
            });
        });
    }
}

/// Follows a line's target.
fn follow(target: Target, actions: &mut Vec<HudAction>) {
    match target {
        Target::Window(tab) => actions.push(HudAction::OpenWindow(tab)),
        Target::Building(id) => actions.push(HudAction::SelectBuilding(id)),
        Target::EventDialog => actions.push(HudAction::EventDialog(true)),
        Target::Kingdom(id) => actions.push(HudAction::OpenKingdom(id)),
        Target::Proposal(serial) => actions.push(HudAction::ShowProposal(serial)),
        Target::Tile(tile) => actions.push(HudAction::Focus(tile)),
        Target::Tasks => actions.push(HudAction::OpenPanel(super::RightPanel::Tasks)),
    }
    actions.push(HudAction::Sound("ui_click"));
}

/// „reicht etwa 3 Tage“ / „reicht etwa 1 Tag“ / „reicht keinen Tag mehr“.
fn food_days_text(days: f32) -> String {
    let rounded = f64::from(days).round();
    if rounded < 1.0 {
        "reicht keinen Tag mehr".to_owned()
    } else if rounded < 2.0 {
        "reicht etwa 1 Tag".to_owned()
    } else {
        format!("reicht etwa {} Tage", decimal(rounded, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    #[test]
    fn food_days_use_the_right_number() {
        assert_eq!(food_days_text(0.4), "reicht keinen Tag mehr");
        assert_eq!(food_days_text(1.2), "reicht etwa 1 Tag");
        assert_eq!(food_days_text(12.6), "reicht etwa 13 Tage");
    }

    #[test]
    fn a_new_kingdom_shows_its_numbers_and_the_idle_research() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match kingdoms_sim::defs::Defs::load(&dir) {
            Ok(d) => d,
            Err(e) => panic!("shipped game data must load: {e}"),
        };
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let o = Overview::compute(&state, &defs);
        assert_eq!(o.population, 12);
        assert_eq!(o.treasury, state.economy().treasury());
        assert_eq!(o.research, "Keine Forschung ausgewählt");
        assert!(o.food_days.is_some_and(|d| d > 0.0));
        // Nothing is pending or broken: no danger, but the idle research is mentioned.
        let threats: Vec<&str> = o.threats.iter().map(|t| t.text.as_str()).collect();
        assert!(o.threats.iter().all(|t| t.severity != Severity::Danger), "{threats:?}");
        assert!(threats.iter().any(|t| t.starts_with("Die Forschung steht still")), "{threats:?}");
        // Sorted by importance.
        let ranks: Vec<u8> = o.threats.iter().map(|t| severity_rank(t.severity)).collect();
        assert!(ranks.windows(2).all(|w| w[0] >= w[1]));
        assert!(o.recent.is_empty());
        // No war, no enemies, no proposals at the start; the rank is the first one.
        assert!(o.threats.iter().all(|t| !t.text.starts_with("Krieg mit")));
        assert_eq!(o.rank.0, defs.ranks()[0].name);
        assert!(o.rank.1.is_some());
        // With everything researched there is nothing left to choose.
        let mut state = state;
        state.tick(&defs, vec![kingdoms_sim::command::Command::DebugUnlockAllResearch]);
        let o = Overview::compute(&state, &defs);
        assert_eq!(o.research, "Alle Technologien sind erforscht");
        assert!(o.threats.iter().all(|t| !t.text.starts_with("Die Forschung steht still")));
    }
}
