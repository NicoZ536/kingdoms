//! „Wirtschaft“: the economy at a glance – supply and demand per category of goods, shortages
//! and surpluses, the price index, the trade balance, and the health and the economics of the
//! production chains (value of the output against inputs, tool wear, wages and upkeep per day at
//! today's prices). Everything is aggregated from the simulation's reports
//! ([`report::economy_summary`], [`report::production_report`], [`report::building_economics`],
//! [`report::trade_balance`], [`finance::daily_rates`]); the classification of a good as
//! shortage or surplus is a reading aid of this window, not a game rule.

use kingdoms_engine::egui::{self, Color32, RichText, Ui};
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::defs::{BuildingTypeId, ResourceCategory, ResourceId};
use kingdoms_sim::economy::finance;
use kingdoms_sim::production::ProductionStatus;
use kingdoms_sim::report::{self, EconomicsBasis, ResourceRow};

use super::storage::category_label;
use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, Navigate, TabContext, card_row, hint, select};
use crate::ui::hud::HudAction;
use crate::ui::hud::format::{count, int, signed_gold_milli};
use crate::ui::theme::{Palette, heading_font};
use crate::ui::widgets::{mini_bar, section_header};

/// Days of stock below which a good with falling stock counts as a shortage.
const SHORTAGE_DAYS: f64 = 3.0;
/// Storage fill (percent) above which a good with rising stock counts as a surplus.
const SURPLUS_FILL: u32 = 75;

/// Supply and demand of one category of goods.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CategoryBalance {
    pub stock: u64,
    pub capacity: u64,
    pub produced: u64,
    pub consumed: u64,
    /// Value of the stock at current prices in gold.
    pub value: u64,
    /// Mean of current price / base price in percent (100 = base prices).
    pub price_index: u32,
}

impl CategoryBalance {
    /// Produced minus consumed on the last closed day.
    pub fn balance(&self) -> i64 {
        self.produced as i64 - self.consumed as i64
    }
}

/// Sums `rows` (with their categories) per category; categories without goods are left out.
pub fn category_balances(rows: &[(ResourceCategory, ResourceRow)]) -> Vec<(ResourceCategory, CategoryBalance)> {
    ResourceCategory::ALL
        .into_iter()
        .filter_map(|category| {
            let members: Vec<&ResourceRow> = rows.iter().filter(|(c, _)| *c == category).map(|(_, r)| r).collect();
            if members.is_empty() {
                return None;
            }
            let mut b = CategoryBalance::default();
            for r in &members {
                b.stock += u64::from(r.stock);
                b.capacity += u64::from(r.capacity);
                b.produced += u64::from(r.produced_per_day);
                b.consumed += u64::from(r.consumed_per_day);
                b.value += u64::from(r.stock) * u64::from(r.price_centi) / 100;
            }
            b.price_index = price_index(members.iter().copied());
            Some((category, b))
        })
        .collect()
}

/// Mean of current price / base price in percent over `rows` (100 without goods).
pub fn price_index<'a>(rows: impl Iterator<Item = &'a ResourceRow>) -> u32 {
    let (sum, n) = rows
        .filter(|r| r.base_price_centi > 0)
        .fold((0u64, 0u64), |(s, n), r| (s + u64::from(r.price_centi) * 10_000 / u64::from(r.base_price_centi), n + 1));
    sum.checked_div(n).map_or(100, |mean| u32::try_from((mean + 50) / 100).unwrap_or(u32::MAX))
}

/// Why a good is short, if it is.
#[derive(Clone, Debug, PartialEq)]
pub enum Shortage {
    /// Construction sites wait for more than the stock.
    Construction { missing: u32 },
    /// Used yesterday, nothing left.
    Empty,
    /// Falling stock lasts only this many days.
    Falling { days: f64 },
}

/// Classifies a good as shortage (see [`Shortage`]).
pub fn shortage(row: &ResourceRow) -> Option<Shortage> {
    let net = i64::from(row.produced_per_day) - i64::from(row.consumed_per_day);
    if row.committed > row.stock {
        return Some(Shortage::Construction { missing: row.committed - row.stock });
    }
    if row.stock == 0 && row.consumed_per_day > 0 {
        return Some(Shortage::Empty);
    }
    if net < 0 {
        let days = f64::from(row.stock) / (-net) as f64;
        if days < SHORTAGE_DAYS {
            return Some(Shortage::Falling { days });
        }
    }
    None
}

/// Fill of the storage in percent if the good is a surplus (rising stock in nearly full storage).
pub fn surplus(row: &ResourceRow) -> Option<u32> {
    if row.capacity == 0 || row.produced_per_day <= row.consumed_per_day {
        return None;
    }
    let fill = u32::try_from(u64::from(row.stock) * 100 / u64::from(row.capacity)).unwrap_or(u32::MAX);
    (fill >= SURPLUS_FILL).then_some(fill)
}

/// A good in the shortage or surplus list.
#[derive(Clone, Debug)]
struct GoodLine {
    resource: ResourceId,
    key: String,
    name: String,
    row: ResourceRow,
    text: String,
    /// Production buildings waiting for it.
    waiting: usize,
}

/// Production buildings of one type.
#[derive(Clone, Debug)]
struct Chain {
    kind: BuildingTypeId,
    icon: String,
    name: String,
    total: usize,
    working: usize,
    /// Waiting for something normal (shift, lunch, season, way to work).
    resting: usize,
    /// Problems with the building that has one (for the click).
    problems: Vec<(BuildingId, String)>,
    /// Inputs this type waits for.
    missing: Vec<ResourceId>,
    /// Output value − inputs − wages − upkeep per day of all its buildings with figures
    /// (milli-gold), and how many of them have such figures (field farms and orchards only
    /// after their first measured day).
    net_milli: i64,
    costed: usize,
    /// Of the costed ones, how many are valued by their recent harvests (field farms,
    /// orchards) instead of the crew's work rate.
    by_harvest: usize,
}

struct Data {
    treasury: i64,
    net_milli: i64,
    /// Average trade balance per day of the last closed days and its explanation.
    trade_milli: i64,
    trade_text: String,
    categories: Vec<(ResourceCategory, CategoryBalance)>,
    index: u32,
    /// Sum of the price changes against yesterday in centi-gold.
    index_trend: i64,
    shortages: Vec<GoodLine>,
    surpluses: Vec<GoodLine>,
    chains: Vec<Chain>,
    has_history: bool,
}

impl Data {
    fn compute(context: &TabContext<'_, '_>) -> Self {
        let state = context.input.session.state();
        let defs = context.input.session.defs();
        let economy = report::economy_summary(state, defs);
        let rows: Vec<(ResourceCategory, ResourceRow)> = economy
            .resources
            .iter()
            .filter_map(|r| defs.resources().get(r.resource.index()).map(|d| (d.category, *r)))
            .collect();
        // Production chains: every production building by type.
        let mut chains: Vec<Chain> = Vec::new();
        for (id, b) in state.buildings().iter() {
            let Some(def) = defs.buildings().get(b.kind().index()) else { continue };
            if def.production.is_none() || !b.is_complete() {
                continue;
            }
            let Some(p) = report::production_report(state, defs, id) else { continue };
            let chain = match chains.iter_mut().position(|c| c.kind == b.kind()) {
                Some(i) => &mut chains[i],
                None => {
                    chains.push(Chain {
                        kind: b.kind(),
                        icon: format!("bld_{}", def.key),
                        name: def.name.clone(),
                        total: 0,
                        working: 0,
                        resting: 0,
                        problems: Vec::new(),
                        missing: Vec::new(),
                        net_milli: 0,
                        costed: 0,
                        by_harvest: 0,
                    });
                    let last = chains.len() - 1;
                    &mut chains[last]
                }
            };
            chain.total += 1;
            match report::building_economics(state, defs, id).map(|e| (e.basis, e.net_milli)) {
                // A field farm without a measured day has no figures yet.
                Some((EconomicsBasis::Harvests { days: 0 }, _)) | None => {}
                Some((basis, net)) => {
                    chain.net_milli += net;
                    chain.costed += 1;
                    chain.by_harvest += usize::from(matches!(basis, EconomicsBasis::Harvests { .. }));
                }
            }
            let (_, problem) = super::production::status_style(p.status);
            if p.status.is_working() || p.status == ProductionStatus::Growing {
                chain.working += 1;
            } else if problem {
                chain.problems.push((id, p.status_text.clone()));
            } else {
                chain.resting += 1;
            }
            if let ProductionStatus::MissingInput(r) = p.status
                && !chain.missing.contains(&r)
            {
                chain.missing.push(r);
            }
        }
        // Problems first, then by type.
        chains.sort_by_key(|c| (c.problems.is_empty(), c.kind));
        let waiting_for = |r: ResourceId| chains.iter().filter(|c| c.missing.contains(&r)).map(|c| c.problems.len()).sum::<usize>();
        let line = |r: &ResourceRow, text: String| {
            let def = defs.resources().get(r.resource.index());
            GoodLine {
                resource: r.resource,
                key: def.map_or_else(String::new, |d| d.key.clone()),
                name: def.map_or_else(String::new, |d| d.name.clone()),
                row: *r,
                text,
                waiting: waiting_for(r.resource),
            }
        };
        let mut shortages: Vec<GoodLine> = economy
            .resources
            .iter()
            .filter_map(|r| {
                let text = match shortage(r)? {
                    Shortage::Construction { missing } => format!("Baustellen fehlen {}", int(i64::from(missing))),
                    Shortage::Empty => "aufgebraucht".to_owned(),
                    Shortage::Falling { days } if days < 1.0 => "reicht keinen Tag mehr".to_owned(),
                    Shortage::Falling { days } => format!("reicht noch etwa {}", count(days.floor() as u64, "Tag", "Tage")),
                };
                Some(line(r, text))
            })
            .collect();
        // Inputs production waits for are shortages even without a daily balance yet.
        for c in &chains {
            for r in &c.missing {
                if !shortages.iter().any(|s| s.resource == *r)
                    && let Some(row) = economy.resources.iter().find(|x| x.resource == *r)
                {
                    shortages.push(line(row, "Betriebe warten darauf".to_owned()));
                }
            }
        }
        let surpluses = economy
            .resources
            .iter()
            .filter_map(|r| surplus(r).map(|fill| line(r, format!("Lager zu {fill} % voll"))))
            .collect();
        let index = price_index(economy.resources.iter());
        let index_trend = economy.resources.iter().map(|r| r.price_trend_centi).sum();
        let has_history = state.economy().stats().yesterday().is_some();
        let trade = report::trade_balance(state);
        Self {
            treasury: economy.treasury,
            net_milli: finance::daily_rates(state, defs).net(),
            trade_milli: trade.average_milli,
            trade_text: trade.text,
            categories: category_balances(&rows),
            index,
            index_trend,
            shortages,
            surpluses,
            chains,
            has_history,
        }
    }
}

/// The economy tab.
#[derive(Default)]
pub struct EconomyTab {
    data: Cached<Data>,
}

impl EconomyTab {
    pub fn show(&mut self, ui: &mut Ui, context: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) -> Option<Navigate> {
        let data = self.data.get(context.input, || Data::compute(context));
        let mut nav = None;
        let working: usize = data.chains.iter().map(|c| c.working).sum();
        let total: usize = data.chains.iter().map(|c| c.total).sum();
        let problems: usize = data.chains.iter().map(|c| c.problems.len()).sum();
        let value: u64 = data.categories.iter().map(|(_, b)| b.value).sum();
        let index_color = match data.index {
            0..=94 => Palette::GOOD,
            95..=110 => Palette::INK,
            _ => Palette::BAD,
        };
        let responses = card_row(
            ui,
            &[
                ("Schatzkammer", format!("{} Gold", int(data.treasury)), Palette::INK),
                ("Laufend pro Tag", format!("{} Gold", signed_gold_milli(data.net_milli)), table::signed_color(data.net_milli as f64)),
                ("Warenwert", format!("{} Gold", int(i64::try_from(value).unwrap_or(i64::MAX))), Palette::INK),
                ("Preisindex", data.index.to_string(), index_color),
                ("Engpässe", data.shortages.len().to_string(), if data.shortages.is_empty() { Palette::GOOD } else { Palette::BAD }),
                ("Betriebe in Arbeit", format!("{working} von {total}"), if problems > 0 { Palette::WARN } else { Palette::GOOD }),
                ("Handelsbilanz je Tag", format!("{} Gold", signed_gold_milli(data.trade_milli)), table::signed_color(data.trade_milli as f64)),
            ],
        );
        if let Some(r) = responses.get(6) {
            r.clone().on_hover_text(format!("{} Verlauf im Reiter Handel.", data.trade_text));
        }
        if let Some(r) = responses.get(3) {
            r.clone().on_hover_text(format!(
                "Mittel der aktuellen Preise im Verhältnis zum Grundpreis (Grundpreise = 100). Gegenüber gestern {}.",
                match data.index_trend.signum() {
                    1 => "gestiegen",
                    -1 => "gefallen",
                    _ => "unverändert",
                }
            ));
        }
        if let Some(r) = responses.get(2) {
            r.clone().on_hover_text("Bestand aller Waren zu aktuellen Preisen");
        }
        if !data.has_history {
            hint(ui, "Erzeugung, Verbrauch und Preistrends erscheinen nach dem ersten vollen Tag.");
        }
        section_header(ui, "Angebot und Nachfrage je Warengruppe (gestern)");
        let columns: [(&str, f32, CellAlign); 8] = [
            ("Warengruppe", 150.0, CellAlign::Left),
            ("Bestand", 90.0, CellAlign::Right),
            ("Lager", 150.0, CellAlign::Left),
            ("Erzeugt", 90.0, CellAlign::Right),
            ("Verbraucht", 100.0, CellAlign::Right),
            ("Bilanz", 90.0, CellAlign::Right),
            ("Wert", 110.0, CellAlign::Right),
            ("Preisindex", 100.0, CellAlign::Right),
        ];
        table::header(ui, &columns);
        for (i, (category, b)) in data.categories.iter().enumerate() {
            table::row(ui, ("eco_cat", i), ROW_HEIGHT, i % 2 == 1, false, |ui| {
                table::text(ui, columns[0].1, CellAlign::Left, RichText::new(category_label(*category)).strong());
                table::text(ui, columns[1].1, CellAlign::Right, int(i64::try_from(b.stock).unwrap_or(i64::MAX)));
                table::cell(ui, columns[2].1, CellAlign::Left, |ui| {
                    let fill = if b.capacity == 0 { 0.0 } else { b.stock as f32 / b.capacity as f32 };
                    let color = if fill >= 0.9 { Palette::WARN } else { Palette::GOOD };
                    mini_bar(ui, fill, color, 80.0);
                    ui.label(RichText::new(format!("{} %", (fill * 100.0).round() as u32)).small().color(Palette::INK_SOFT));
                });
                table::text(ui, columns[3].1, CellAlign::Right, int(i64::try_from(b.produced).unwrap_or(i64::MAX)));
                table::text(ui, columns[4].1, CellAlign::Right, int(i64::try_from(b.consumed).unwrap_or(i64::MAX)));
                let net = b.balance();
                table::text(ui, columns[5].1, CellAlign::Right, RichText::new(signed(net)).color(table::signed_color(net as f64)));
                table::text(ui, columns[6].1, CellAlign::Right, format!("{} Gold", int(i64::try_from(b.value).unwrap_or(i64::MAX))));
                table::text(ui, columns[7].1, CellAlign::Right, b.price_index.to_string());
            });
        }
        ui.add_space(6.0);
        super::columns(ui, 2, |cols| {
            if let Some(n) = goods_list(&mut cols[0], context, "Engpässe", &data.shortages, true) {
                nav = Some(n);
            }
            if let Some(n) = goods_list(&mut cols[1], context, "Überschüsse", &data.surpluses, false) {
                nav = Some(n);
            }
        });
        section_header(ui, "Produktionsketten");
        if data.chains.is_empty() {
            hint(ui, "Noch keine fertigen Produktionsbetriebe.");
        }
        // The columns fit the widest name, staff count and result; the state text wraps.
        let texts: Vec<String> = data.chains.iter().map(|c| chain_text(context, c)).collect();
        let staff = |c: &Chain| format!("{} von {} arbeiten", c.working, c.total);
        let net = |c: &Chain| if c.costed == 0 { "–".to_owned() } else { format!("{} Gold/Tag", signed_gold_milli(c.net_milli)) };
        let widths = table::fit_widths(
            &[
                table::Fit::flex(120.0, 24.0 + table::content_width(ui, data.chains.iter().map(|c| RichText::new(&c.name).into()))),
                table::Fit::fixed(74.0 + table::content_width(ui, data.chains.iter().map(|c| RichText::new(staff(c)).small().into()))),
                table::Fit::fixed(table::content_width(ui, data.chains.iter().map(|c| RichText::new(net(c)).into())) + 8.0),
                table::Fit::flex(160.0, table::content_width(ui, texts.iter().map(|t| RichText::new(t).into()))),
            ],
            ui.available_width() - 18.0,
        );
        for (i, (c, text)) in data.chains.iter().zip(&texts).enumerate() {
            let clickable = !c.problems.is_empty();
            let height = table::wrapped_height(ui, RichText::new(text), widths[3], ROW_HEIGHT + 2.0);
            let response = table::row(ui, ("eco_chain", c.kind), height, i % 2 == 1, clickable, |ui| {
                table::cell(ui, widths[0], CellAlign::Left, |ui| {
                    if let Some(icon) = context.icons.get(&c.icon) {
                        icon.show(ui, 20.0);
                    }
                    ui.add(egui::Label::new(RichText::new(&c.name).color(Palette::INK)).truncate());
                });
                table::cell(ui, widths[1], CellAlign::Left, |ui| {
                    let share = if c.total == 0 { 0.0 } else { c.working as f32 / c.total as f32 };
                    let color = if !c.problems.is_empty() {
                        Palette::BAD
                    } else if share >= 0.5 {
                        Palette::GOOD
                    } else {
                        Palette::WARN
                    };
                    mini_bar(ui, share, color, 70.0);
                    ui.label(RichText::new(staff(c)).small());
                });
                table::cell(ui, widths[2], CellAlign::Right, |ui| {
                    if c.costed == 0 {
                        ui.label(RichText::new(net(c)).color(Palette::INK_SOFT)).on_hover_text(
                            "Noch keine Zahlen: Feldfarmen und Obstgärten werden nach ihren Ernten bewertet, sobald ein Tag abgeschlossen ist.",
                        );
                    } else {
                        let color = table::signed_color(c.net_milli as f64);
                        ui.label(RichText::new(net(c)).color(color)).on_hover_text(chain_net_note(c));
                    }
                });
                ui.add_space(6.0);
                let color = if c.problems.is_empty() { Palette::INK_SOFT } else { Palette::BAD };
                table::wrapped(ui, widths[3], RichText::new(text).color(color));
            });
            if clickable && response.on_hover_text("Klick: den ersten Betrieb mit Problem zeigen").clicked()
                && let Some((building, _)) = c.problems.first()
            {
                select(actions, *building);
            }
        }
        nav
    }
}

/// Tooltip of a chain's daily result: how its figures were found.
fn chain_net_note(c: &Chain) -> String {
    let crew = "Wert der Erzeugnisse abzüglich Rohstoffe, Werkzeugverschleiß, Löhne und Unterhalt je Tag – mit der zugewiesenen Belegschaft, zu heutigen Preisen.";
    let harvest = "Wert der Ernten im Mittel der letzten Tage abzüglich Löhne und Unterhalt – die Ernte folgt den Jahreszeiten.";
    let detail = "Einzelheiten und das gestern Hergestellte im Info-Fenster des Betriebs.";
    match c.by_harvest {
        0 => format!("{crew} {detail}"),
        n if n == c.costed => format!("{harvest} {detail}"),
        _ => format!("Betriebe mit Rezepten: {crew} Feldfarmen und Obstgärten: {harvest} {detail}"),
    }
}

fn signed(v: i64) -> String {
    if v > 0 { format!("+{}", int(v)) } else { int(v) }
}

/// Short state of a chain: its problems or why the rest rests.
fn chain_text(context: &TabContext<'_, '_>, c: &Chain) -> String {
    let defs = context.input.session.defs();
    if !c.missing.is_empty() {
        let names: Vec<&str> = c.missing.iter().filter_map(|r| defs.resources().get(r.index())).map(|d| d.name.as_str()).collect();
        return format!("Es fehlt: {}", names.join(", "));
    }
    if let Some((_, text)) = c.problems.first() {
        return if c.problems.len() == 1 { text.clone() } else { format!("{text} (und {} weitere)", c.problems.len() - 1) };
    }
    match c.resting {
        0 => "läuft".to_owned(),
        1 => "1 Betrieb ruht gerade (Schicht, Pause, Jahreszeit oder Arbeitsweg)".to_owned(),
        n => format!("{n} Betriebe ruhen gerade (Schicht, Pause, Jahreszeit oder Arbeitsweg)"),
    }
}

/// A list of goods (shortages or surpluses); a click opens the good's history.
fn goods_list(ui: &mut Ui, context: &TabContext<'_, '_>, title: &str, lines: &[GoodLine], shortage: bool) -> Option<Navigate> {
    let mut nav = None;
    ui.label(RichText::new(title).font(heading_font(15.0)).color(if shortage { Palette::BAD } else { Palette::GOOD }));
    if lines.is_empty() {
        hint(ui, if shortage { "Keine Engpässe." } else { "Keine Überschüsse." });
    }
    // The columns fit the widest name and figures; the note wraps.
    let net = |g: &GoodLine| i64::from(g.row.produced_per_day) - i64::from(g.row.consumed_per_day);
    let notes: Vec<String> = lines.iter().map(|g| if g.waiting > 0 { format!("{} · {} warten", g.text, g.waiting) } else { g.text.clone() }).collect();
    let widths = table::fit_widths(
        &[
            table::Fit::flex(90.0, 22.0 + table::content_width(ui, lines.iter().map(|g| RichText::new(&g.name).into()))),
            table::Fit::fixed(table::content_width(ui, lines.iter().map(|g| RichText::new(int(i64::from(g.row.stock))).into()))),
            table::Fit::fixed(table::content_width(ui, lines.iter().map(|g| RichText::new(format!("{}/Tag", signed(net(g)))).into()))),
            table::Fit::flex(90.0, table::content_width(ui, notes.iter().map(|t| RichText::new(t).small().into()))),
        ],
        ui.available_width() - 18.0,
    );
    for (i, (g, note)) in lines.iter().zip(&notes).enumerate() {
        let color: Color32 = if shortage { Palette::BAD } else { Palette::INK_SOFT };
        let height = table::wrapped_height(ui, RichText::new(note).small(), widths[3], ROW_HEIGHT);
        let response = table::row(ui, (title, i), height, i % 2 == 1, true, |ui| {
            table::cell(ui, widths[0], CellAlign::Left, |ui| {
                if let Some(icon) = context.icons.get(&format!("res_{}", g.key)) {
                    icon.show(ui, 18.0);
                }
                ui.add(egui::Label::new(&g.name).truncate());
            });
            let net = net(g);
            table::text(ui, widths[1], CellAlign::Right, int(i64::from(g.row.stock)));
            table::text(ui, widths[2], CellAlign::Right, RichText::new(format!("{}/Tag", signed(net))).color(table::signed_color(net as f64)));
            ui.add_space(6.0);
            table::wrapped(ui, widths[3], RichText::new(note).small().color(color));
        });
        if response.on_hover_text("Klick: Verlauf in der Statistik").clicked() {
            nav = Some(Navigate::ResourceStatistics(g.resource));
        }
    }
    nav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_figures_say_how_they_were_found() {
        let defs = super::super::tests::test_defs();
        let kind = defs.buildings()[0].id;
        let chain = |costed, by_harvest| Chain {
            kind,
            icon: String::new(),
            name: String::new(),
            total: costed,
            working: 0,
            resting: 0,
            problems: Vec::new(),
            missing: Vec::new(),
            net_milli: 0,
            costed,
            by_harvest,
        };
        assert!(chain_net_note(&chain(2, 0)).starts_with("Wert der Erzeugnisse"));
        assert!(chain_net_note(&chain(2, 2)).starts_with("Wert der Ernten"));
        let mixed = chain_net_note(&chain(3, 1));
        assert!(mixed.contains("Betriebe mit Rezepten") && mixed.contains("Feldfarmen und Obstgärten"), "{mixed}");
    }

    fn row(stock: u32, capacity: u32, committed: u32, produced: u32, consumed: u32, price: u32, base: u32) -> ResourceRow {
        let defs = super::super::tests::test_defs();
        ResourceRow {
            resource: defs.resources()[0].id,
            stock,
            capacity,
            committed,
            produced_today: 0,
            consumed_today: 0,
            produced_per_day: produced,
            consumed_per_day: consumed,
            household_today: 0,
            household_per_day: 0,
            price_centi: price,
            base_price_centi: base,
            price_trend_centi: 0,
        }
    }

    #[test]
    fn shortages_and_surpluses() {
        // Construction needs more than the stock.
        assert_eq!(shortage(&row(10, 100, 25, 0, 0, 400, 400)), Some(Shortage::Construction { missing: 15 }));
        // Used up.
        assert_eq!(shortage(&row(0, 100, 0, 0, 4, 400, 400)), Some(Shortage::Empty));
        // Falling: 10 left, −5 per day = 2 days.
        assert_eq!(shortage(&row(10, 100, 0, 5, 10, 400, 400)), Some(Shortage::Falling { days: 2.0 }));
        // Falling slowly: 30 left, −5 per day lasts 6 days.
        assert_eq!(shortage(&row(30, 100, 0, 5, 10, 400, 400)), None);
        // Nothing used, nothing in stock: no shortage.
        assert_eq!(shortage(&row(0, 100, 0, 0, 0, 400, 400)), None);
        // Rising stock in nearly full storage.
        assert_eq!(surplus(&row(80, 100, 0, 10, 2, 400, 400)), Some(80));
        assert_eq!(surplus(&row(70, 100, 0, 10, 2, 400, 400)), None);
        assert_eq!(surplus(&row(90, 100, 0, 2, 10, 400, 400)), None, "falling stock is no surplus");
        assert_eq!(surplus(&row(90, 0, 0, 10, 2, 400, 400)), None, "no storage");
    }

    #[test]
    fn categories_sum_their_goods() {
        let rows = vec![
            (ResourceCategory::Raw, row(100, 200, 0, 20, 10, 500, 400)),
            (ResourceCategory::Raw, row(50, 200, 0, 0, 5, 300, 400)),
            (ResourceCategory::Food, row(10, 100, 0, 3, 8, 1000, 1000)),
        ];
        let sums = category_balances(&rows);
        assert_eq!(sums.len(), 2, "categories without goods are left out");
        let (category, raw) = &sums[0];
        assert_eq!(*category, ResourceCategory::Raw);
        assert_eq!((raw.stock, raw.capacity, raw.produced, raw.consumed), (150, 400, 20, 15));
        assert_eq!(raw.balance(), 5);
        // 100 × 5 + 50 × 3 gold.
        assert_eq!(raw.value, 650);
        // (125 % + 75 %) / 2.
        assert_eq!(raw.price_index, 100);
        assert_eq!(sums[1].1.balance(), -5);
        assert_eq!(price_index(std::iter::empty()), 100);
        assert_eq!(price_index([row(0, 0, 0, 0, 0, 450, 400)].iter()), 113);
    }
}
