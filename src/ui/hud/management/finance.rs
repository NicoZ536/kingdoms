//! „Finanzen“: treasury, today's and yesterday's balance by ledger category with the running
//! daily rates, the history of the recorded days as a chart, the tax rate
//! (`Command::SetTaxRate`) with its effect on happiness and the expected tax income, and the
//! court of the rank (`report::court_report`: upkeep per day, part of „Unterhalt“, the next
//! rank that costs more) with the festival's cost and a link to it in „Bevölkerung“.

use kingdoms_engine::egui::{self, Color32, RichText, Ui};
use kingdoms_sim::command::Command;
use kingdoms_sim::economy::{DayLedger, LedgerCategory, MILLI_PER_GOLD};
use kingdoms_sim::population::HappinessFactor;
use kingdoms_sim::defs::Defs;
use kingdoms_sim::report::{self, CourtReport, FinanceReport};

use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, Tab, TabContext, card_row, columns, command, day_label, day_title, hint};
use crate::ui::chart::{Chart, Series, ValueFormat};
use crate::ui::hud::HudAction;
use crate::ui::hud::format::{decimal, int, signed_gold_milli};
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::section_header;

/// Chart colour of a ledger category (income green/blue, expenses red/brown).
pub fn category_color(category: LedgerCategory) -> Color32 {
    match category {
        LedgerCategory::Taxes => Palette::GOOD,
        LedgerCategory::MarketFees => Color32::from_rgb(56, 140, 140),
        LedgerCategory::Exports => Palette::INFO,
        LedgerCategory::Wages => Palette::BAD,
        LedgerCategory::Upkeep => Palette::WARN,
        LedgerCategory::Imports => Color32::from_rgb(128, 84, 150),
        LedgerCategory::Construction => Palette::WOOD_LIGHT,
        LedgerCategory::Military => Color32::from_rgb(96, 96, 96),
        LedgerCategory::Research => Color32::from_rgb(176, 72, 132),
        LedgerCategory::Events => Color32::from_rgb(140, 140, 36),
        LedgerCategory::Diplomacy => Color32::from_rgb(70, 110, 170),
    }
}

/// The projected daily amount of a category in milli-gold (`None` for flows without a
/// running rate such as trade, construction, research and events).
pub fn daily_rate(report: &FinanceReport, category: LedgerCategory) -> Option<i64> {
    let r = &report.rates;
    match category {
        LedgerCategory::Taxes => Some(r.taxes),
        LedgerCategory::MarketFees => Some(r.market_fees),
        LedgerCategory::Wages => Some(r.wages),
        LedgerCategory::Upkeep => Some(r.upkeep),
        LedgerCategory::Military => Some(r.military),
        LedgerCategory::Exports
        | LedgerCategory::Imports
        | LedgerCategory::Construction
        | LedgerCategory::Research
        | LedgerCategory::Events
        | LedgerCategory::Diplomacy => None,
    }
}

struct Data {
    report: FinanceReport,
    court: CourtReport,
    /// Index of the current rank.
    rank: usize,
    /// Average happiness points from the tax rate (`None` without citizens).
    tax_happiness: Option<f32>,
}

/// State of the tab.
#[derive(Default)]
pub struct FinanceTab {
    /// Rate being dragged on the slider (sent when released).
    dragging: Option<u8>,
    data: Cached<Data>,
}

impl FinanceTab {
    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let input = ctx.input;
        let defs = input.session.defs();
        let data = self.data.get(input, || {
            let state = input.session.state();
            let happiness = report::happiness_report(state, defs);
            let tax_happiness = happiness.factors.iter().find(|f| f.factor == HappinessFactor::Taxes).map(|f| f.points);
            Data {
                report: report::finance_report(state, defs),
                court: report::court_report(state, defs),
                rank: usize::from(state.progress().rank()),
                tax_happiness,
            }
        });
        let r = &data.report;
        let yesterday = r.history.last();
        let treasury_color = if r.treasury < 0 { Palette::BAD } else { Palette::INK };
        card_row(
            ui,
            &[
                ("Schatzkammer", format!("{} Gold", int(r.treasury)), treasury_color),
                ("Bilanz heute (bisher)", gold_milli(r.today.net_milli()), table::signed_color(r.today.net_milli() as f64)),
                (
                    "Bilanz gestern",
                    yesterday.map_or_else(|| "–".to_owned(), |d| gold_milli(d.net_milli())),
                    yesterday.map_or(Palette::INK_SOFT, |d| table::signed_color(d.net_milli() as f64)),
                ),
                ("Laufend pro Tag", gold_milli(r.rates.net()), table::signed_color(r.rates.net() as f64)),
                ("Steuersatz", format!("{} %", r.tax_rate_percent), Palette::INK),
            ],
        );
        if r.bankrupt {
            ui.add_space(4.0);
            ui.colored_label(Palette::BAD, "Bankrott: Es werden keine neuen Bauten begonnen und alle Betriebe arbeiten langsamer, bis die Kasse wieder im Plus ist.");
        }
        section_header(ui, "Verlauf der Tagesbilanzen");
        history_chart(ui, &r.history);
        ui.add_space(6.0);
        let tax_happiness = data.tax_happiness;
        columns(ui, 2, |cols| {
            ledger_table(&mut cols[0], r);
            tax_box(&mut self.dragging, &mut cols[1], ctx, r, tax_happiness, actions);
            cols[1].add_space(6.0);
            court_box(&mut cols[1], defs, &data.court, data.rank, actions);
        });
    }
}

/// The tax rate control with its effects (`dragging`: rate being dragged, sent on release).
fn tax_box(dragging: &mut Option<u8>, ui: &mut Ui, ctx: &TabContext<'_, '_>, r: &FinanceReport, tax_happiness: Option<f32>, actions: &mut Vec<HudAction>) {
    let input = ctx.input;
    let defs = input.session.defs();
    let rules = defs.rules();
    let max = rules.max_tax_rate_percent;
    let pop_rules = &rules.population;
    // A rate waiting in the command queue (e.g. while paused) is shown as the new value.
    let queued = input.session.queued_commands().iter().rev().find_map(|c| match c {
        Command::SetTaxRate { percent } => Some(*percent),
        _ => None,
    });
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Steuern").font(heading_font(17.0)).color(Palette::INK));
        let current = queued.unwrap_or(r.tax_rate_percent);
        let mut value = dragging.unwrap_or(current);
        ui.horizontal(|ui| {
            if ui.add_enabled(value > 0, egui::Button::new(RichText::new(" − ").strong())).on_hover_text("Steuersatz senken").clicked() {
                command(actions, Command::SetTaxRate { percent: value - 1 });
            }
            let slider = ui.add(egui::Slider::new(&mut value, 0..=max).suffix(" %").show_value(true));
            if slider.dragged() {
                *dragging = Some(value);
            } else if slider.drag_stopped() || slider.changed() {
                *dragging = None;
                if value != current {
                    command(actions, Command::SetTaxRate { percent: value });
                }
            }
            if ui.add_enabled(value < max, egui::Button::new(RichText::new(" + ").strong())).on_hover_text("Steuersatz erhöhen").clicked() {
                command(actions, Command::SetTaxRate { percent: value + 1 });
            }
        });
        if let Some(q) = queued.filter(|q| *q != r.tax_rate_percent) {
            ui.label(RichText::new(format!("Neuer Satz {q} % wird beim nächsten Takt übernommen.")).small().color(Palette::WARN));
        }
        egui::Grid::new("tax_grid").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
            crate::ui::widgets::stat_row(ui, "Steuereinnahmen laufend", RichText::new(format!("{} Gold/Tag", signed_gold_milli(r.rates.taxes))).color(Palette::GOOD));
            let today = r.today.milli(LedgerCategory::Taxes);
            crate::ui::widgets::stat_row(ui, "Heute eingenommen", format!("{} Gold", signed_gold_milli(today)));
            match tax_happiness {
                Some(points) => {
                    let color = table::signed_color(f64::from(points));
                    let sign = if points > 0.0 { "+" } else { "" };
                    crate::ui::widgets::stat_row(ui, "Wirkung auf die Zufriedenheit", RichText::new(format!("{sign}{} Punkte", decimal(f64::from(points), 1))).color(color));
                }
                None => crate::ui::widgets::stat_row(ui, "Wirkung auf die Zufriedenheit", "– (keine Bewohner)"),
            }
            crate::ui::widgets::stat_row(ui, "Neutraler Steuersatz", format!("{} %", pop_rules.neutral_tax_rate_percent));
            crate::ui::widgets::stat_row(
                ui,
                "Je Prozentpunkt über / unter neutral",
                {
                    let v = decimal(f64::from(pop_rules.tax_happiness_per_percent), 2);
                    format!("−{v} / +{v} Punkte")
                },
            );
        });
        ui.add_space(2.0);
        hint(ui, "Steuern zahlen die Haushalte aller Häuser im Verwaltungsradius von Rathaus oder Burg; Obdachlose zahlen nichts. Höhere Steuern senken die Zufriedenheit, niedrigere heben sie.");
    });
}

/// The next rank above `rank` whose court costs more than `upkeep` gold a day: its name and
/// gold per day.
pub fn next_court_step(defs: &Defs, rank: usize, upkeep: u32) -> Option<(&str, u32)> {
    defs.ranks().iter().skip(rank + 1).find(|r| r.court_upkeep > upkeep).map(|r| (r.name.as_str(), r.court_upkeep))
}

/// The court of the rank: its upkeep (booked with „Unterhalt“), what the next dearer rank
/// costs, and the festival's cost with a link to the tab where it is held.
fn court_box(ui: &mut Ui, defs: &Defs, court: &CourtReport, rank: usize, actions: &mut Vec<HudAction>) {
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Hof und Feste").font(heading_font(17.0)).color(Palette::INK));
        let upkeep_color = if court.upkeep_per_day > 0 { Palette::WARN } else { Palette::INK };
        ui.label(RichText::new(&court.upkeep_text).color(upkeep_color));
        if let Some((name, gold)) = next_court_step(defs, rank, court.upkeep_per_day) {
            ui.label(RichText::new(format!("Ab dem Rang „{name}“: {} Gold je Tag", int(i64::from(gold)))).small().color(Palette::INK_SOFT));
        }
        ui.add_space(4.0);
        let festival = &court.festival;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("Ein Fest kostet jetzt {} Gold.", int(i64::from(festival.cost)))).color(Palette::INK));
            if let Some(problem) = festival.problem.as_ref() {
                ui.label(RichText::new(problem).color(Palette::INK_SOFT));
            }
        });
        if ui.button("Feste im Reiter Bevölkerung …").on_hover_text("Fest ausrichten: Kosten, Wirkungen und wann das nächste möglich ist").clicked() {
            actions.push(HudAction::OpenWindow(Tab::Population));
            actions.push(HudAction::Sound("ui_click"));
        }
        hint(ui, "Die Hofhaltung (Beamte, Boten, Gesandte) wächst mit dem Rang und ist im Unterhalt enthalten; Feste werden unter „Ereignisse“ gebucht.");
    });
}

/// „+12,5 Gold“ from milli-gold.
fn gold_milli(milli: i64) -> String {
    format!("{} Gold", signed_gold_milli(milli))
}

fn ledger_table(ui: &mut Ui, r: &FinanceReport) {
    let yesterday = r.history.last();
    let width = ui.available_width();
    let w = [width - 3.0 * 96.0 - 12.0, 96.0, 96.0, 96.0];
    table::header(
        ui,
        &[("Kassenbuch", w[0], CellAlign::Left), ("Heute", w[1], CellAlign::Right), ("Gestern", w[2], CellAlign::Right), ("Laufend/Tag", w[3], CellAlign::Right)],
    );
    let money = |milli: i64| RichText::new(if milli == 0 { "–".to_owned() } else { signed_gold_milli(milli) }).color(table::signed_color(milli as f64));
    for (i, c) in LedgerCategory::ALL.iter().enumerate() {
        table::row(ui, ("ledger", i), ROW_HEIGHT - 4.0, i % 2 == 1, false, |ui| {
            table::cell(ui, w[0], CellAlign::Left, |ui| {
                swatch(ui, category_color(*c));
                ui.label(RichText::new(c.display_name()).color(Palette::INK));
            });
            table::text(ui, w[1], CellAlign::Right, money(r.today.milli(*c)));
            table::text(ui, w[2], CellAlign::Right, yesterday.map_or_else(|| RichText::new("–").color(Palette::INK_SOFT), |d| money(d.milli(*c))));
            let rate = daily_rate(r, *c);
            table::text(ui, w[3], CellAlign::Right, rate.map_or_else(|| RichText::new("–").color(Palette::INK_SOFT), money));
        });
    }
    table::rule(ui);
    type Total = (&'static str, fn(&DayLedger) -> i64, Option<i64>);
    let totals: [Total; 3] = [
        ("Einnahmen", DayLedger::income_milli, Some(r.rates.taxes + r.rates.market_fees)),
        ("Ausgaben", DayLedger::expenses_milli, Some(r.rates.wages + r.rates.upkeep + r.rates.military)),
        ("Bilanz", DayLedger::net_milli, Some(r.rates.net())),
    ];
    for (i, (label, f, rate)) in totals.iter().enumerate() {
        table::row(ui, ("ledger_total", i), ROW_HEIGHT - 4.0, false, false, |ui| {
            table::text(ui, w[0], CellAlign::Left, RichText::new(*label).strong());
            table::text(ui, w[1], CellAlign::Right, money(f(&r.today)).strong());
            table::text(ui, w[2], CellAlign::Right, yesterday.map_or_else(|| RichText::new("–").color(Palette::INK_SOFT), |d| money(f(d)).strong()));
            table::text(ui, w[3], CellAlign::Right, rate.map_or_else(|| RichText::new("–"), |m| money(m).strong()));
        });
    }
    ui.label(RichText::new("„Heute“ zählt seit Mitternacht; Steuern, Gebühren, Löhne und Unterhalt werden stündlich gebucht.").small().color(Palette::INK_SOFT));
}

/// Stacked bars per category and the daily balance as a line.
fn history_chart(ui: &mut Ui, history: &[DayLedger]) {
    let labels: Vec<String> = history.iter().map(|d| day_label(d.day)).collect();
    let titles: Vec<String> = history.iter().map(|d| day_title(d.day)).collect();
    let gold = |milli: i64| milli as f64 / MILLI_PER_GOLD as f64;
    let mut series: Vec<Series> = LedgerCategory::ALL
        .iter()
        .filter(|c| history.iter().any(|d| d.milli(**c) != 0))
        .map(|c| Series::bar(c.display_name(), category_color(*c), history.iter().map(|d| gold(d.milli(*c))).collect()))
        .collect();
    series.push(Series::line("Bilanz", Palette::INK, history.iter().map(|d| gold(d.net_milli())).collect()));
    Chart::new(("chart", "finance_history"), &labels, &series)
        .titles(&titles)
        .height(170.0)
        .format(ValueFormat::Gold)
        .empty_text("Der Verlauf beginnt nach dem ersten abgeschlossenen Tag.")
        .show(ui);
}

fn swatch(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(rect.shrink(1.0), egui::CornerRadius::same(2), color);
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_dearer_court_follows_the_ranks() {
        let defs = super::super::tests::test_defs();
        let ranks = defs.ranks();
        let stadt = ranks.iter().position(|r| r.court_upkeep > 0).expect("a rank with court upkeep");
        // From the settlement on, the first rank with a court is named.
        let (name, gold) = next_court_step(&defs, 0, 0).expect("next step");
        assert_eq!((name, gold), (ranks[stadt].name.as_str(), ranks[stadt].court_upkeep));
        // At that rank, the next dearer one.
        let next = next_court_step(&defs, stadt, ranks[stadt].court_upkeep).expect("dearer rank");
        assert!(next.1 > ranks[stadt].court_upkeep);
        // At the top nothing follows.
        let top = ranks.len() - 1;
        assert_eq!(next_court_step(&defs, top, ranks[top].court_upkeep), None);
    }

    #[test]
    fn categories_have_distinct_colours_and_rates_where_projected() {
        let colors: std::collections::BTreeSet<[u8; 4]> = LedgerCategory::ALL.iter().map(|c| category_color(*c).to_array()).collect();
        assert_eq!(colors.len(), LedgerCategory::ALL.len());
        let report = FinanceReport {
            treasury: 100,
            bankrupt: false,
            tax_rate_percent: 10,
            today: DayLedger::new(3),
            history: Vec::new(),
            rates: kingdoms_sim::economy::finance::DailyRates { taxes: 12_000, market_fees: 3_000, wages: -9_000, upkeep: -4_000, military: 0 },
        };
        assert_eq!(daily_rate(&report, LedgerCategory::Taxes), Some(12_000));
        assert_eq!(daily_rate(&report, LedgerCategory::Upkeep), Some(-4_000));
        assert_eq!(daily_rate(&report, LedgerCategory::Exports), None);
        assert_eq!(daily_rate(&report, LedgerCategory::Construction), None);
        assert_eq!(gold_milli(12_500), "+12,5 Gold");
        assert_eq!(gold_milli(-3_000), "−3 Gold");
    }
}
