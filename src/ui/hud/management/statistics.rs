//! „Statistik“: selectable history charts from the simulation's statistics – population,
//! treasury, food stock, production and price of a chosen good, happiness, employment, the
//! households' savings and territory and buildings – over the recorded days (at most 48),
//! where sampled the last 72 hours, or the whole game season by season.

use kingdoms_engine::egui::{self, RichText, Ui};
use kingdoms_sim::defs::{Defs, ResourceId};
use kingdoms_sim::economy::MILLI_PER_GOLD;
use kingdoms_sim::report;
use kingdoms_sim::state::GameState;
use kingdoms_sim::statistics::SeasonSample;
use kingdoms_sim::time::Season;

use super::{Cached, TabContext, day_label, day_title, hint, hour_label, hour_title};
use crate::ui::chart::{Chart, Series, ValueFormat};
use crate::ui::hud::HudAction;
use crate::ui::theme::Palette;

/// What the chart shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Metric {
    #[default]
    Population,
    Treasury,
    Food,
    Resource,
    Happiness,
    Employment,
    /// Savings of all households.
    Wealth,
    /// Territory and finished buildings (sampled per season only).
    Realm,
}

impl Metric {
    const ALL: [Metric; 8] = [
        Metric::Population,
        Metric::Treasury,
        Metric::Food,
        Metric::Resource,
        Metric::Happiness,
        Metric::Employment,
        Metric::Wealth,
        Metric::Realm,
    ];

    fn label(self) -> &'static str {
        match self {
            Metric::Population => "Bevölkerung",
            Metric::Treasury => "Schatzkammer",
            Metric::Food => "Nahrungsvorrat",
            Metric::Resource => "Ware",
            Metric::Happiness => "Zufriedenheit",
            Metric::Employment => "Beschäftigung",
            Metric::Wealth => "Wohlstand",
            Metric::Realm => "Gebiet und Bauten",
        }
    }

    /// True if the simulation samples the metric hourly.
    pub fn has_hours(self) -> bool {
        matches!(self, Metric::Population | Metric::Treasury | Metric::Employment)
    }

    /// True if the simulation keeps the metric per day.
    pub fn has_days(self) -> bool {
        self != Metric::Realm
    }

    /// `range` if the metric has it, else the closest range it has (every metric is sampled
    /// per season).
    pub fn supported(self, range: Range) -> Range {
        match range {
            Range::Hours if !self.has_hours() => self.supported(Range::Days),
            Range::Days if !self.has_days() => Range::Seasons,
            r => r,
        }
    }
}

/// Time axis of the chart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Range {
    /// Closed days (daily closing at midnight).
    #[default]
    Days,
    /// Hourly samples of the last three days.
    Hours,
    /// One sample at the end of every season of the whole game.
    Seasons,
}

/// A chart ready to draw.
struct Panel {
    caption: String,
    format: ValueFormat,
    series: Vec<Series>,
    /// The value axis starts at zero (counts, stocks); prices zoom to their range.
    from_zero: bool,
}

struct Data {
    labels: Vec<String>,
    titles: Vec<String>,
    panels: Vec<Panel>,
}

/// State of the tab.
#[derive(Default)]
pub struct StatisticsTab {
    metric: Metric,
    range: Range,
    resource: Option<ResourceId>,
    /// Selection the cached data belongs to.
    shown: Option<(Metric, Range, Option<ResourceId>)>,
    data: Cached<Data>,
}

/// End-of-day treasury of every closed day in milli-gold, reconstructed backwards from the
/// current treasury: every change of the treasury is booked in the ledger (`nets`: the net of
/// each closed day, oldest first; `today_net`: the running day so far).
pub fn daily_treasury(treasury_milli: i64, today_net: i64, nets: &[i64]) -> Vec<i64> {
    let mut out = vec![0; nets.len()];
    let mut end_of_day = treasury_milli.saturating_sub(today_net);
    for (i, net) in nets.iter().enumerate().rev() {
        out[i] = end_of_day;
        end_of_day = end_of_day.saturating_sub(*net);
    }
    out
}

impl StatisticsTab {
    /// Shows the history of `resource` (from the goods table).
    pub fn show_resource(&mut self, resource: ResourceId) {
        self.metric = Metric::Resource;
        self.range = Range::Days;
        self.resource = Some(resource);
    }

    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let input = ctx.input;
        let defs = input.session.defs();
        // Selection.
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Anzeige").color(Palette::INK_SOFT));
            for m in Metric::ALL {
                if ui.selectable_label(self.metric == m, m.label()).clicked() && self.metric != m {
                    self.metric = m;
                    self.range = m.supported(self.range);
                    actions.push(HudAction::Sound("ui_click"));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Zeitraum").color(Palette::INK_SOFT));
            let days = ui
                .add_enabled(self.metric.has_days(), egui::Button::selectable(self.range == Range::Days, "Tage (bis 48)"))
                .on_disabled_hover_text("Gebiet und Bauten werden am Ende jeder Jahreszeit erfasst.");
            if days.clicked() {
                self.range = Range::Days;
            }
            let hours = ui
                .add_enabled(self.metric.has_hours(), egui::Button::selectable(self.range == Range::Hours, "Stunden (72)"))
                .on_disabled_hover_text("Stündliche Werte gibt es für Bevölkerung, Schatzkammer und Beschäftigung.");
            if hours.clicked() {
                self.range = Range::Hours;
            }
            let seasons = ui
                .selectable_label(self.range == Range::Seasons, "Jahreszeiten (gesamt)")
                .on_hover_text("Ein Wert am Ende jeder Jahreszeit über das ganze Spiel.");
            if seasons.clicked() {
                self.range = Range::Seasons;
            }
            if self.metric == Metric::Resource {
                ui.add_space(16.0);
                ui.label(RichText::new("Ware").color(Palette::INK_SOFT));
                let current = self.resource.or_else(|| defs.resources().first().map(|r| r.id));
                let name = current.and_then(|id| defs.resources().get(id.index())).map_or_else(String::new, |r| r.name.clone());
                let mut chosen = current;
                egui::ComboBox::from_id_salt("stats_resource").selected_text(name).width(180.0).height(320.0).show_ui(ui, |ui| {
                    for r in defs.resources() {
                        ui.selectable_value(&mut chosen, Some(r.id), &r.name);
                    }
                });
                self.resource = chosen;
            }
        });
        let selection = (self.metric, self.range, self.resource);
        if self.shown != Some(selection) {
            self.data.invalidate();
            self.shown = Some(selection);
        }
        let data = self.data.get(input, || build(input.session.state(), defs, selection.0, selection.1, selection.2));
        ui.add_space(4.0);
        for (i, panel) in data.panels.iter().enumerate() {
            ui.label(RichText::new(&panel.caption).strong().color(Palette::INK));
            let height = if data.panels.len() > 1 { 200.0 } else { 330.0 };
            Chart::new(("chart", "statistics", i), &data.labels, &panel.series)
                .titles(&data.titles)
                .height(height)
                .format(panel.format)
                .include_zero(panel.from_zero)
                .empty_text(match self.range {
                    Range::Seasons => "Noch keine Werte – der erste Wert entsteht am Ende der ersten Jahreszeit.",
                    Range::Days | Range::Hours => {
                        "Noch keine Werte – die Tagesstatistik beginnt nach dem ersten abgeschlossenen Tag, die Stundenwerte nach der ersten vollen Stunde."
                    }
                })
                .show(ui);
            ui.add_space(6.0);
        }
        hint(ui, match self.range {
            Range::Seasons => "Fahre mit der Maus über den Verlauf, um die Werte am Ende einer Jahreszeit zu sehen.",
            Range::Days | Range::Hours => "Fahre mit der Maus über den Verlauf, um die Werte eines Tages bzw. einer Stunde zu sehen.",
        });
    }
}

/// The chart data of a selection.
fn build(state: &GameState, defs: &Defs, metric: Metric, range: Range, resource: Option<ResourceId>) -> Data {
    let range = metric.supported(range);
    let econ = state.economy();
    let pop_days: Vec<_> = state.population().stats().history().copied().collect();
    let hourly: Vec<_> = state.statistics().hourly().copied().collect();
    let day_axis = |days: &[u64]| -> (Vec<String>, Vec<String>) { (days.iter().map(|d| day_label(*d)).collect(), days.iter().map(|d| day_title(*d)).collect()) };
    let hour_axis = || -> (Vec<String>, Vec<String>) { (hourly.iter().map(|h| hour_label(h.hour)).collect(), hourly.iter().map(|h| hour_title(h.hour)).collect()) };
    let (axis, panels) = match (metric, range) {
        (_, Range::Seasons) | (Metric::Realm, _) => {
            let seasons = state.statistics().seasons();
            return season_data(seasons.samples(), seasons.stride(), defs, metric, resource);
        }
        (Metric::Population, Range::Days) => {
            let days: Vec<u64> = pop_days.iter().map(|d| d.day).collect();
            let stage = |i: usize| pop_days.iter().map(|d| f64::from(d.by_stage[i])).collect::<Vec<_>>();
            (
                day_axis(&days),
                vec![
                    Panel {
                        caption: "Bewohner am Tagesende".into(),
                        format: ValueFormat::Integer,
                        from_zero: true,
                        series: vec![
                            Series::line("Bewohner", Palette::INFO, pop_days.iter().map(|d| f64::from(d.population)).collect()),
                            Series::line("Haushalte", Palette::GOLD_DARK, pop_days.iter().map(|d| f64::from(d.households)).collect()),
                            Series::line("Obdachlos", Palette::BAD, pop_days.iter().map(|d| f64::from(d.homeless)).collect()),
                        ],
                    },
                    Panel {
                        caption: "Lebensabschnitte".into(),
                        format: ValueFormat::Integer,
                        from_zero: true,
                        series: vec![
                            Series::bar("Kinder", Palette::GOOD_LIGHT, stage(0)),
                            Series::bar("Erwachsene", Palette::GOOD, stage(1)),
                            Series::bar("Alte", Palette::GOLD_DARK, stage(2)),
                        ],
                    },
                ],
            )
        }
        (Metric::Population, Range::Hours) => (
            hour_axis(),
            vec![Panel {
                caption: "Bewohner (stündlich)".into(),
                format: ValueFormat::Integer,
                from_zero: true,
                series: vec![Series::line("Bewohner", Palette::INFO, hourly.iter().map(|h| f64::from(h.population)).collect())],
            }],
        ),
        (Metric::Treasury, Range::Days) => {
            let ledger: Vec<_> = econ.ledger().history().copied().collect();
            let nets: Vec<i64> = ledger.iter().map(|d| d.net_milli()).collect();
            let treasury = daily_treasury(econ.treasury_milli(), econ.ledger().today().net_milli(), &nets);
            let gold = |m: i64| m as f64 / MILLI_PER_GOLD as f64;
            let days: Vec<u64> = ledger.iter().map(|d| d.day).collect();
            (
                day_axis(&days),
                vec![
                    Panel {
                        caption: "Schatzkammer am Tagesende".into(),
                        format: ValueFormat::Gold,
                        from_zero: true,
                        series: vec![Series::line("Schatz", Palette::GOLD_DARK, treasury.iter().map(|m| gold(*m)).collect())],
                    },
                    Panel {
                        caption: "Einnahmen und Ausgaben je Tag".into(),
                        format: ValueFormat::Gold,
                        from_zero: true,
                        series: vec![
                            Series::bar("Einnahmen", Palette::GOOD, ledger.iter().map(|d| gold(d.income_milli())).collect()),
                            Series::bar("Ausgaben", Palette::BAD, ledger.iter().map(|d| gold(d.expenses_milli())).collect()),
                            Series::line("Bilanz", Palette::INK, nets.iter().map(|m| gold(*m)).collect()),
                        ],
                    },
                ],
            )
        }
        (Metric::Treasury, Range::Hours) => (
            hour_axis(),
            vec![Panel {
                caption: "Schatzkammer (stündlich)".into(),
                format: ValueFormat::Gold,
                from_zero: true,
                series: vec![Series::line("Schatz", Palette::GOLD_DARK, hourly.iter().map(|h| h.treasury as f64).collect())],
            }],
        ),
        (Metric::Food, _) => {
            let records: Vec<_> = econ.stats().history().collect();
            let edible: Vec<ResourceId> = defs.resources().iter().filter(|r| r.is_edible()).map(|r| r.id).collect();
            let sum = |f: &dyn Fn(&kingdoms_sim::economy::DayRecord, ResourceId) -> u32| -> Vec<f64> {
                records.iter().map(|d| edible.iter().map(|r| f64::from(f(d, *r))).sum()).collect()
            };
            let days: Vec<u64> = records.iter().map(|d| d.day).collect();
            let consumed = sum(&|d, r| d.flows.consumed.get(r));
            (
                day_axis(&days),
                vec![
                    Panel {
                        caption: "Nahrung im Reichslager am Tagesende (alle Lebensmittel)".into(),
                        format: ValueFormat::Integer,
                        from_zero: true,
                        series: vec![Series::line("Vorrat", Palette::GOOD, sum(&|d, r| d.stock.get(r)))],
                    },
                    Panel {
                        caption: "Erzeugt und verbraucht je Tag".into(),
                        format: ValueFormat::Integer,
                        from_zero: true,
                        series: vec![
                            Series::bar("Erzeugt", Palette::GOOD, sum(&|d, r| d.flows.produced.get(r))),
                            Series::bar("Eingekauft", Palette::INFO, sum(&|d, r| d.flows.imported.get(r))),
                            Series::bar("Verbraucht", Palette::BAD, consumed.iter().map(|v| -v).collect()),
                        ],
                    },
                ],
            )
        }
        (Metric::Resource, _) => {
            let resource = resource.or_else(|| defs.resources().first().map(|r| r.id));
            let history = resource.map(|r| report::resource_history(state, r)).unwrap_or_default();
            let name = resource.and_then(|r| defs.resources().get(r.index())).map_or_else(String::new, |r| r.name.clone());
            let days: Vec<u64> = history.iter().map(|d| d.day).collect();
            (
                day_axis(&days),
                vec![
                    Panel {
                        caption: format!("{name}: Bestand am Tagesende, Erzeugung und Verbrauch je Tag"),
                        format: ValueFormat::Integer,
                        from_zero: true,
                        series: vec![
                            Series::bar("Erzeugt", Palette::GOOD, history.iter().map(|d| f64::from(d.produced)).collect()),
                            Series::bar("Eingekauft", Palette::INFO, history.iter().map(|d| f64::from(d.imported)).collect()),
                            Series::bar("Verbraucht", Palette::BAD, history.iter().map(|d| -f64::from(d.consumed)).collect()),
                            Series::bar("Verkauft", Palette::WARN, history.iter().map(|d| -f64::from(d.exported)).collect()),
                            Series::line("Bestand", Palette::INK, history.iter().map(|d| f64::from(d.stock)).collect()),
                        ],
                    },
                    Panel {
                        caption: format!("{name}: Marktpreis in Gold"),
                        format: ValueFormat::Decimal,
                        from_zero: false,
                        series: vec![Series::line("Preis", Palette::GOLD_DARK, history.iter().map(|d| f64::from(d.price_centi) / 100.0).collect())],
                    },
                ],
            )
        }
        (Metric::Happiness, _) => {
            let days: Vec<u64> = pop_days.iter().map(|d| d.day).collect();
            (
                day_axis(&days),
                vec![Panel {
                    caption: "Durchschnittliche Zufriedenheit am Tagesende (0–100)".into(),
                    format: ValueFormat::Integer,
                    from_zero: true,
                    series: vec![Series::line("Zufriedenheit", Palette::GOOD, pop_days.iter().map(|d| f64::from(d.happiness)).collect())],
                }],
            )
        }
        (Metric::Employment, Range::Days) => {
            let days: Vec<u64> = pop_days.iter().map(|d| d.day).collect();
            (
                day_axis(&days),
                vec![Panel {
                    caption: "Arbeitslose Erwachsene am Tagesende".into(),
                    format: ValueFormat::Integer,
                    from_zero: true,
                    series: vec![
                        Series::line("Erwachsene", Palette::INFO, pop_days.iter().map(|d| f64::from(d.by_stage[1])).collect()),
                        Series::line("Arbeitslos", Palette::BAD, pop_days.iter().map(|d| f64::from(d.unemployed)).collect()),
                    ],
                }],
            )
        }
        (Metric::Wealth, _) => {
            let days: Vec<u64> = pop_days.iter().map(|d| d.day).collect();
            let savings: Vec<f64> = pop_days.iter().map(|d| d.savings as f64).collect();
            let per_household = pop_days.iter().map(|d| per_household(d.savings, d.households)).collect();
            (day_axis(&days), wealth_panels("am Tagesende", savings, per_household))
        }
        (Metric::Employment, Range::Hours) => (
            hour_axis(),
            vec![Panel {
                caption: "Beschäftigte und Bewohner (stündlich)".into(),
                format: ValueFormat::Integer,
                from_zero: true,
                series: vec![
                    Series::line("Bewohner", Palette::INFO, hourly.iter().map(|h| f64::from(h.population)).collect()),
                    Series::line("Beschäftigt", Palette::GOOD, hourly.iter().map(|h| f64::from(h.employed)).collect()),
                ],
            }],
        ),
    };
    let (labels, titles) = axis;
    Data { labels, titles, panels }
}

/// Average savings of a household in gold (0 without households).
fn per_household(savings: i64, households: u32) -> f64 {
    if households == 0 { 0.0 } else { savings as f64 / f64::from(households) }
}

/// The „Wohlstand“ charts: savings of all households and per household (`when`: „am
/// Tagesende“ …).
fn wealth_panels(when: &str, savings: Vec<f64>, per_household: Vec<f64>) -> Vec<Panel> {
    vec![
        Panel {
            caption: format!("Ersparnisse aller Haushalte {when}"),
            format: ValueFormat::Gold,
            from_zero: true,
            series: vec![Series::line("Ersparnisse", Palette::GOLD_DARK, savings)],
        },
        Panel {
            caption: format!("Ersparnisse je Haushalt {when}"),
            format: ValueFormat::Gold,
            from_zero: true,
            series: vec![Series::line("Je Haushalt", Palette::GOOD, per_household)],
        },
    ]
}

/// Short axis label of a season sample („J2 Fr“).
fn season_label(sample: &SeasonSample) -> String {
    let short = match sample.season() {
        Season::Spring => "Fr",
        Season::Summer => "So",
        Season::Autumn => "He",
        Season::Winter => "Wi",
    };
    format!("J{} {short}", sample.year())
}

/// Hover title of a season sample, with the ruler's rank then („Ende Frühling · Jahr 2 ·
/// Marktflecken“).
fn season_title(sample: &SeasonSample, defs: &Defs) -> String {
    let mut title = format!("Ende {} · Jahr {}", sample.season().display_name(), sample.year());
    if let Some(rank) = defs.ranks().get(usize::from(sample.rank)) {
        title.push_str(" · ");
        title.push_str(&rank.name);
    }
    title
}

/// The chart data of `metric` over the season samples of the whole game (`stride`: seasons
/// between two samples).
fn season_data(samples: &[SeasonSample], stride: u32, defs: &Defs, metric: Metric, resource: Option<ResourceId>) -> Data {
    let when = if stride > 1 { format!("am Ende jeder {stride}. Jahreszeit") } else { "am Ende jeder Jahreszeit".to_owned() };
    let series = |f: &dyn Fn(&SeasonSample) -> f64| -> Vec<f64> { samples.iter().map(f).collect() };
    let stock_of = |s: &SeasonSample, r: ResourceId| s.stock.get(r.index()).copied().map_or(0.0, f64::from);
    let panels = match metric {
        Metric::Population => vec![
            Panel {
                caption: format!("Bewohner {when}"),
                format: ValueFormat::Integer,
                from_zero: true,
                series: vec![
                    Series::line("Bewohner", Palette::INFO, series(&|s| f64::from(s.population))),
                    Series::line("Haushalte", Palette::GOLD_DARK, series(&|s| f64::from(s.households))),
                    Series::line("Obdachlos", Palette::BAD, series(&|s| f64::from(s.homeless))),
                ],
            },
            Panel {
                caption: "Lebensabschnitte".into(),
                format: ValueFormat::Integer,
                from_zero: true,
                series: vec![
                    Series::bar("Kinder", Palette::GOOD_LIGHT, series(&|s| f64::from(s.by_stage[0]))),
                    Series::bar("Erwachsene", Palette::GOOD, series(&|s| f64::from(s.by_stage[1]))),
                    Series::bar("Alte", Palette::GOLD_DARK, series(&|s| f64::from(s.by_stage[2]))),
                ],
            },
        ],
        Metric::Treasury => vec![
            Panel {
                caption: format!("Schatzkammer {when}"),
                format: ValueFormat::Gold,
                from_zero: true,
                series: vec![Series::line("Schatz", Palette::GOLD_DARK, series(&|s| s.treasury as f64))],
            },
            Panel {
                caption: "Einnahmen und Ausgaben je Jahreszeit".into(),
                format: ValueFormat::Gold,
                from_zero: true,
                series: vec![
                    Series::bar("Einnahmen", Palette::GOOD, series(&|s| s.income as f64)),
                    Series::bar("Ausgaben", Palette::BAD, series(&|s| s.expenses as f64)),
                    Series::line("Bilanz", Palette::INK, series(&|s| (s.income - s.expenses) as f64)),
                ],
            },
        ],
        Metric::Food => {
            let edible: Vec<ResourceId> = defs.resources().iter().filter(|r| r.is_edible()).map(|r| r.id).collect();
            vec![Panel {
                caption: format!("Nahrung im Reichslager {when} (alle Lebensmittel)"),
                format: ValueFormat::Integer,
                from_zero: true,
                series: vec![Series::line("Vorrat", Palette::GOOD, series(&|s| edible.iter().map(|r| stock_of(s, *r)).sum()))],
            }]
        }
        Metric::Resource => {
            let resource = resource.or_else(|| defs.resources().first().map(|r| r.id));
            let name = resource.and_then(|r| defs.resources().get(r.index())).map_or_else(String::new, |r| r.name.clone());
            vec![Panel {
                caption: format!("{name}: Bestand im Reichslager {when}"),
                format: ValueFormat::Integer,
                from_zero: true,
                series: vec![Series::line("Bestand", Palette::INK, series(&|s| resource.map_or(0.0, |r| stock_of(s, r))))],
            }]
        }
        Metric::Happiness => vec![Panel {
            caption: format!("Durchschnittliche Zufriedenheit {when} (0–100)"),
            format: ValueFormat::Integer,
            from_zero: true,
            series: vec![Series::line("Zufriedenheit", Palette::GOOD, series(&|s| f64::from(s.happiness)))],
        }],
        Metric::Employment => vec![Panel {
            caption: format!("Beschäftigte und Bewohner {when}"),
            format: ValueFormat::Integer,
            from_zero: true,
            series: vec![
                Series::line("Bewohner", Palette::INFO, series(&|s| f64::from(s.population))),
                Series::line("Beschäftigt", Palette::GOOD, series(&|s| f64::from(s.employed))),
            ],
        }],
        Metric::Wealth => wealth_panels(&when, series(&|s| s.savings as f64), series(&|s| per_household(s.savings, s.households))),
        Metric::Realm => vec![
            Panel {
                caption: format!("Eigenes Gebiet in Feldern {when}"),
                format: ValueFormat::Integer,
                from_zero: true,
                series: vec![Series::line("Gebiet", Palette::GOOD, series(&|s| f64::from(s.territory)))],
            },
            Panel {
                caption: format!("Fertige Gebäude {when} (mit Mauern und Toren)"),
                format: ValueFormat::Integer,
                from_zero: true,
                series: vec![Series::line("Gebäude", Palette::GOLD_DARK, series(&|s| f64::from(s.buildings)))],
            },
        ],
    };
    Data { labels: samples.iter().map(season_label).collect(), titles: samples.iter().map(|s| season_title(s, defs)).collect(), panels }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::hud::management::tests::test_defs;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    #[test]
    fn treasury_is_reconstructed_from_the_ledger() {
        // Now 1 000 gold, +50 so far today; the closed days earned +100, −30, +20.
        let t = daily_treasury(1_000_000, 50_000, &[100_000, -30_000, 20_000]);
        assert_eq!(t, vec![960_000, 930_000, 950_000]);
        assert!(daily_treasury(5, 0, &[]).is_empty());
    }

    #[test]
    fn every_selection_builds_aligned_series() {
        let defs = test_defs();
        let mut state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        // Two closed days and some hourly samples.
        for _ in 0..(kingdoms_sim::time::TICKS_PER_DAY * 2) {
            state.tick(&defs, Vec::new());
        }
        // The ledger reconstruction ends at the treasury the hourly sample saw at midnight.
        let nets: Vec<i64> = state.economy().ledger().history().map(|d| d.net_milli()).collect();
        let rebuilt = daily_treasury(state.economy().treasury_milli(), state.economy().ledger().today().net_milli(), &nets);
        assert_eq!(rebuilt.len(), 2);
        // The hourly sample of each midnight saw the treasury at the end of the closed day.
        for (day, milli) in state.economy().ledger().history().map(|d| d.day).zip(&rebuilt) {
            let midnight = state.statistics().hourly().find(|h| h.hour == (day + 1) * 24).expect("midnight sample");
            assert_eq!(milli.div_euclid(MILLI_PER_GOLD), midnight.treasury, "day {day}");
        }
        let wood = defs.resource_id("wood");
        for metric in Metric::ALL {
            for range in [Range::Days, Range::Hours, Range::Seasons] {
                let data = build(&state, &defs, metric, range, wood);
                assert_eq!(data.labels.len(), data.titles.len());
                assert!(!data.panels.is_empty());
                for p in &data.panels {
                    assert!(p.series.iter().all(|s| s.values.len() == data.labels.len()), "{metric:?} {range:?} {}", p.caption);
                }
                let expected = match metric.supported(range) {
                    Range::Hours => state.statistics().hourly().len(),
                    Range::Days => 2,
                    Range::Seasons => state.statistics().seasons().samples().len(),
                };
                assert_eq!(data.labels.len(), expected, "{metric:?} {range:?}");
            }
        }
    }

    #[test]
    fn every_metric_falls_back_to_a_range_it_has() {
        assert_eq!(Metric::Happiness.supported(Range::Hours), Range::Days);
        assert_eq!(Metric::Wealth.supported(Range::Hours), Range::Days);
        assert_eq!(Metric::Realm.supported(Range::Hours), Range::Seasons);
        assert_eq!(Metric::Realm.supported(Range::Days), Range::Seasons);
        assert_eq!(Metric::Population.supported(Range::Hours), Range::Hours);
        for m in Metric::ALL {
            assert_eq!(m.supported(Range::Seasons), Range::Seasons);
        }
    }

    fn sample(season_index: u64, defs: &Defs) -> SeasonSample {
        let mut stock = vec![0; defs.resources().len()];
        let bread = defs.resource_id("bread").expect("bread");
        let wood = defs.resource_id("wood").expect("wood");
        stock[bread.index()] = 40;
        stock[wood.index()] = 25;
        SeasonSample {
            season_index,
            population: 30,
            by_stage: [8, 18, 4],
            households: 8,
            homeless: 2,
            employed: 15,
            happiness: 61,
            treasury: 420,
            income: 300,
            expenses: 180,
            savings: 96,
            stock,
            territory: 900,
            buildings: 21,
            rank: 1,
        }
    }

    #[test]
    fn season_samples_chart_the_whole_game() {
        let defs = test_defs();
        let samples = [sample(0, &defs), sample(1, &defs), sample(4, &defs)];
        let data = season_data(&samples, 1, &defs, Metric::Treasury, None);
        assert_eq!(data.labels, ["J1 Fr", "J1 So", "J2 Fr"]);
        let rank = &defs.ranks()[1].name;
        assert_eq!(data.titles[1], format!("Ende Sommer · Jahr 1 · {rank}"));
        assert_eq!(data.panels[0].caption, "Schatzkammer am Ende jeder Jahreszeit");
        assert_eq!(data.panels[1].series[2].values, [120.0; 3], "balance = income − expenses");
        // Savings per household; the food stock sums every edible good.
        let wealth = season_data(&samples, 2, &defs, Metric::Wealth, None);
        assert_eq!(wealth.panels[1].series[0].values, [12.0; 3]);
        assert_eq!(wealth.panels[0].caption, "Ersparnisse aller Haushalte am Ende jeder 2. Jahreszeit");
        let food = season_data(&samples, 1, &defs, Metric::Food, None);
        assert_eq!(food.panels[0].series[0].values, [40.0; 3]);
        let wood = season_data(&samples, 1, &defs, Metric::Resource, defs.resource_id("wood"));
        assert_eq!(wood.panels[0].series[0].values, [25.0; 3]);
        let realm = season_data(&samples, 1, &defs, Metric::Realm, None);
        assert_eq!(realm.panels[0].series[0].values, [900.0; 3]);
        assert_eq!(per_household(10, 0), 0.0);
    }
}
