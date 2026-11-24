//! „Handel“: whether trading works (trading post built, staffed, connected), the trade balance
//! (exports against imports per day, [`report::trade_balance`]), the standing
//! trade orders per resource (buy below / sell above / daily limit) created, edited and removed
//! with `Command::SetTradeOrder` / `RemoveTradeOrder`, buy and sell prices of every good and
//! the trades executed since the game was started or loaded.
//!
//! With other kingdoms on the map the orders are carried out by their caravans at the trading
//! post, at each kingdom's own prices; the tab then shows the best quote of the kingdoms with
//! trade access instead of the world-market prices (only used without kingdoms).
//!
//! Below the orders the caravans on the way, the kingdoms as partners with their prices and the
//! dialog that sends an own caravan ([`super::caravans`]).
//!
//! Rejected orders come back from the simulation as `CommandRejected` and appear as toasts
//! (the feed shows every rejection); the editor previews the same German text from
//! `trade::validate_order` before the order is sent.

use std::collections::VecDeque;

use kingdoms_engine::egui::{self, RichText, Ui};
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingRole, Defs, ResourceCategory, ResourceId};
use kingdoms_sim::diplomacy::has_access;
use kingdoms_sim::economy::{TradeDirection, TradeOrder, storage, trade};
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::report::{self, TradeBalance, TradeReport};
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::GameClock;

use super::storage::{category_label, price};
use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, TabContext, command, hint, select};
use crate::session::{EventCursor, Session};
use crate::ui::hud::HudAction;
use crate::ui::chart::{Chart, Series, ValueFormat};
use crate::ui::hud::format::{int, signed_gold_milli};
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Mark, mark, section_header};

/// Trades kept in the log of the tab.
const TRADE_LOG: usize = 40;

/// Why trade orders are (not) executed, derived from the public state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostStatus {
    /// Orders are executed at this trading post.
    Active(BuildingId),
    NotBuilt,
    UnderConstruction(BuildingId),
    Paused(BuildingId),
    NoWorkers(BuildingId),
    /// Not part of the kingdom stock network (no road to the headquarters).
    NotConnected(BuildingId),
}

impl PostStatus {
    /// German explanation (`caravans`: other kingdoms exist, so their caravans carry out the
    /// orders instead of the hourly world market).
    pub fn text(self, caravans: bool) -> &'static str {
        match self {
            PostStatus::Active(_) if caravans => {
                "Das Handelskontor ist besetzt und mit dem Reichslager verbunden – die Aufträge werden ausgeführt, wenn Karawanen fremder Reiche am Kontor handeln."
            }
            PostStatus::Active(_) => "Das Handelskontor ist besetzt und mit dem Reichslager verbunden – die Aufträge werden stündlich ausgeführt.",
            PostStatus::NotBuilt => "Kein Handelskontor: Aufträge können angelegt werden, werden aber erst ausgeführt, wenn ein Handelskontor steht.",
            PostStatus::UnderConstruction(_) => "Das Handelskontor ist noch im Bau – die Aufträge ruhen bis zur Fertigstellung.",
            PostStatus::Paused(_) => "Das Handelskontor ist stillgelegt – die Aufträge ruhen.",
            PostStatus::NoWorkers(_) => "Das Handelskontor hat keine Arbeiter – die Aufträge ruhen.",
            PostStatus::NotConnected(_) => "Das Handelskontor ist nicht über Straßen mit dem Rathaus verbunden – die Aufträge ruhen.",
        }
    }

    pub fn building(self) -> Option<BuildingId> {
        match self {
            PostStatus::NotBuilt => None,
            PostStatus::Active(b)
            | PostStatus::UnderConstruction(b)
            | PostStatus::Paused(b)
            | PostStatus::NoWorkers(b)
            | PostStatus::NotConnected(b) => Some(b),
        }
    }
}

/// The state of trading: the active post, else the reason of the first trading post.
pub fn post_status(state: &GameState, defs: &Defs, active: Option<BuildingId>) -> PostStatus {
    if let Some(id) = active {
        return PostStatus::Active(id);
    }
    let reachable = storage::reachable_storages(state, defs);
    let posts: Vec<(BuildingId, PostStatus)> = state
        .buildings()
        .iter()
        .filter(|(_, b)| defs.buildings().get(b.kind().index()).is_some_and(|d| d.role == BuildingRole::TradeHub))
        .map(|(id, b)| {
            let status = if !b.is_complete() {
                PostStatus::UnderConstruction(id)
            } else if b.is_paused() {
                PostStatus::Paused(id)
            } else if b.staffed() == 0 {
                PostStatus::NoWorkers(id)
            } else if !reachable.contains(&id) {
                PostStatus::NotConnected(id)
            } else {
                PostStatus::Active(id)
            };
            (id, status)
        })
        .collect();
    // The post closest to working decides the explanation.
    let rank = |s: &PostStatus| match s {
        PostStatus::Active(_) => 0,
        PostStatus::NotConnected(_) => 1,
        PostStatus::NoWorkers(_) => 2,
        PostStatus::Paused(_) => 3,
        PostStatus::UnderConstruction(_) => 4,
        PostStatus::NotBuilt => 5,
    };
    posts.into_iter().map(|(_, s)| s).min_by_key(rank).unwrap_or(PostStatus::NotBuilt)
}

/// Best prices (centi-gold) the kingdoms with trade access quote for one good: the cheapest
/// kingdom that offers it and the best paying kingdom that wants it (`None`: none does).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartnerQuote {
    pub buy: Option<u32>,
    pub sell: Option<u32>,
}

/// Per resource (in id order) the best quotes of the kingdoms with trade access, or `None`
/// without kingdoms (the orders then trade with the world market).
pub fn partner_quotes(state: &GameState, defs: &Defs) -> Option<Vec<PartnerQuote>> {
    let diplomacy = state.diplomacy();
    if diplomacy.is_empty() {
        return None;
    }
    let mut quotes = vec![PartnerQuote::default(); defs.resources().len()];
    for (id, _) in diplomacy.kingdoms() {
        if !has_access(state, defs, id) {
            continue;
        }
        for row in report::partner_prices(state, defs, id) {
            let Some(quote) = quotes.get_mut(row.resource.index()) else { continue };
            if row.offered > 0 {
                quote.buy = Some(quote.buy.map_or(row.buy_centi, |b| b.min(row.buy_centi)));
            }
            if row.wanted > 0 {
                quote.sell = Some(quote.sell.map_or(row.sell_centi, |s| s.max(row.sell_centi)));
            }
        }
    }
    Some(quotes)
}

/// A price or „–“.
fn quote_text(centi: Option<u32>) -> String {
    centi.map_or_else(|| "–".to_owned(), price)
}

/// Buying and selling price of a good („2,28 / 1,90 Gold“; „2,28 Gold / –“ when one side is
/// missing, so the unit never follows the dash).
fn quote_pair_text(buy: Option<u32>, sell: Option<u32>) -> String {
    match (buy, sell) {
        (Some(b), Some(s)) => format!("{} / {} Gold", price(b), price(s)),
        (None, None) => "–".to_owned(),
        _ => format!("{} / {}", quote_gold_text(buy), quote_gold_text(sell)),
    }
}

/// A price with its unit („2,28 Gold“) or „–“ without one.
fn quote_gold_text(centi: Option<u32>) -> String {
    centi.map_or_else(|| "–".to_owned(), |c| format!("{} Gold", price(c)))
}

/// One executed trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TradeEntry {
    tick: u64,
    resource: ResourceId,
    amount: u32,
    gold: i64,
    direction: TradeDirection,
}

/// Prices and yesterday's volumes of one resource.
#[derive(Clone, Debug)]
struct PriceRow {
    resource: ResourceId,
    key: String,
    name: String,
    category: ResourceCategory,
    import_only: bool,
    stock: u32,
    price: u32,
    /// Import and export price per unit: the world market's, or the best kingdom quote.
    buy: Option<u32>,
    sell: Option<u32>,
    bought_yesterday: u32,
    sold_yesterday: u32,
}

struct Data {
    report: TradeReport,
    balance: TradeBalance,
    status: PostStatus,
    prices: Vec<PriceRow>,
    /// Other kingdoms exist: caravans carry out the orders at their prices.
    caravans: bool,
}

impl Data {
    fn price_row(&self, resource: ResourceId) -> Option<&PriceRow> {
        self.prices.iter().find(|p| p.resource == resource)
    }
}

/// The order being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Draft {
    pub resource: ResourceId,
    pub buy: bool,
    pub buy_below: u32,
    pub sell: bool,
    pub sell_above: u32,
    pub daily_limit: u32,
}

impl Draft {
    /// A new order for `resource` (sell what exceeds 200, 50 a day).
    pub fn new(resource: ResourceId) -> Self {
        Self { resource, buy: false, buy_below: 50, sell: true, sell_above: 200, daily_limit: 50 }
    }

    pub fn from_order(order: &TradeOrder) -> Self {
        Self {
            resource: order.resource,
            buy: order.buy_below.is_some(),
            buy_below: order.buy_below.unwrap_or(50),
            sell: order.sell_above.is_some(),
            sell_above: order.sell_above.unwrap_or(200),
            daily_limit: order.daily_limit,
        }
    }

    /// The order the draft describes.
    pub fn order(&self) -> TradeOrder {
        TradeOrder {
            resource: self.resource,
            buy_below: self.buy.then_some(self.buy_below),
            sell_above: self.sell.then_some(self.sell_above),
            daily_limit: self.daily_limit,
        }
    }
}

/// State of the tab.
#[derive(Default)]
pub struct TradeTab {
    cursor: EventCursor,
    log: VecDeque<TradeEntry>,
    draft: Option<Draft>,
    data: Cached<Data>,
    caravans: super::caravans::CaravanPanel,
}

impl TradeTab {
    /// True while the caravan dialog is open.
    pub fn dialog_open(&self) -> bool {
        self.caravans.dialog_open()
    }

    /// Collects executed trades from the session's event log (newest first).
    pub fn read_events(&mut self, session: &Session) {
        let read = session.events().read(&mut self.cursor);
        for logged in read.events {
            if let SimEvent::TradeExecuted { resource, amount, gold, direction } = logged.event {
                self.log.push_front(TradeEntry { tick: logged.tick, resource, amount, gold, direction });
            }
        }
        self.log.truncate(TRADE_LOG);
    }

    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let input = ctx.input;
        let defs = input.session.defs();
        let data = self.data.get(input, || {
            let state = input.session.state();
            let report = report::trade_report(state, defs);
            let status = post_status(state, defs, report.trading_post);
            let e = state.economy();
            let margin = defs.rules().trade_margin_percent;
            // Research and the trade road to the map edge, like the orders of the report.
            let bonus = trade::trade_bonus_percent(state.research(), defs) + report.trade_road_percent;
            let yesterday = e.stats().yesterday();
            let quotes = partner_quotes(state, defs);
            let prices = defs
                .resources()
                .iter()
                .map(|r| {
                    let price = e.prices().centi(r.id);
                    let (buy, sell) = match &quotes {
                        Some(quotes) => quotes.get(r.id.index()).map_or((None, None), |q| (q.buy, q.sell)),
                        None => (
                            Some(trade::trade_unit_price_centi(price, TradeDirection::Import, margin, bonus)),
                            Some(trade::trade_unit_price_centi(price, TradeDirection::Export, margin, bonus)),
                        ),
                    };
                    PriceRow {
                        resource: r.id,
                        key: r.key.clone(),
                        name: r.name.clone(),
                        category: r.category,
                        import_only: r.import_only,
                        stock: e.stock().get(r.id),
                        price,
                        buy,
                        sell,
                        bought_yesterday: yesterday.map_or(0, |d| d.flows.imported.get(r.id)),
                        sold_yesterday: yesterday.map_or(0, |d| d.flows.exported.get(r.id)),
                    }
                })
                .collect();
            Data { report, balance: report::trade_balance(state), status, prices, caravans: quotes.is_some() }
        });
        // Status of trading.
        theme::inset_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                mark(ui, if matches!(data.status, PostStatus::Active(_)) { Mark::Ok } else { Mark::Warning });
                ui.add(egui::Label::new(RichText::new(data.status.text(data.caravans)).color(Palette::INK)).wrap());
            });
            ui.horizontal(|ui| {
                let road = data.report.trade_road_percent > 0;
                mark(ui, if road { Mark::Ok } else { Mark::Next });
                ui.add(egui::Label::new(RichText::new(&data.report.trade_road_text).color(if road { Palette::INK } else { Palette::INK_SOFT })).wrap())
                    .on_hover_text(if data.caravans {
                        "Karawanen handeln mit dem Anteil der Handelsstraßen auf ihrem eigenen Weg (siehe Karawanen unten)."
                    } else {
                        "Gilt für den Handel mit dem Weltmarkt: kürzester Straßenweg vom Handelskontor zum Kartenrand."
                    });
            });
            if let Some(post) = data.status.building() {
                if ui.small_button("Handelskontor zeigen").clicked() {
                    select(actions, post);
                }
            }
        });
        ui.add_space(4.0);
        balance_card(ui, &data.balance);
        ui.add_space(4.0);
        // Orders.
        section_header(ui, "Handelsaufträge");
        let max_limit = defs.rules().max_trade_daily_limit;
        let resource_name = |id: ResourceId| defs.resources().get(id.index()).map_or_else(|| id.to_string(), |r| r.name.clone());
        let resource_icon = |id: ResourceId| defs.resources().get(id.index()).map(|r| format!("res_{}", r.key));
        if data.report.orders.is_empty() {
            hint(ui, "Noch keine Handelsaufträge. Wählt unten eine Ware und legt fest, ab welchem Bestand verkauft oder bis zu welchem Bestand gekauft wird.");
        } else {
            // Prices per piece: those of the caravans' kingdoms, else of the world market.
            let quotes: Vec<String> = data
                .report
                .orders
                .iter()
                .map(|row| {
                    let (buy, sell) = match data.price_row(row.order.resource) {
                        Some(p) if data.caravans => (p.buy, p.sell),
                        _ => (Some(row.buy_price_centi), Some(row.sell_price_centi)),
                    };
                    quote_pair_text(buy, sell)
                })
                .collect();
            let titles = ["Ware", "Bestand", "Kaufen unter", "Verkaufen über", "Je Tag", "Heute gekauft/verkauft", "Einkauf/Verkauf je Stück", ""];
            let orders = &data.report.orders;
            let numbers = |i: usize, f: &dyn Fn(&kingdoms_sim::report::TradeRow) -> RichText| {
                table::Fit::fixed(table::column_width(ui, titles[i], orders.iter().map(|r| f(r).into())))
            };
            let buttons = table::text_width(ui, RichText::new("Bearbeiten")) + table::text_width(ui, RichText::new("Löschen")) + 40.0;
            let w = table::fit_widths(
                &[
                    table::Fit::flex(110.0, 26.0 + table::column_width(ui, titles[0], orders.iter().map(|r| RichText::new(resource_name(r.order.resource)).into()))),
                    numbers(1, &|r| RichText::new(int(i64::from(r.stock)))),
                    numbers(2, &|r| threshold(r.order.buy_below)),
                    numbers(3, &|r| threshold(r.order.sell_above)),
                    numbers(4, &|r| RichText::new(int(i64::from(r.order.daily_limit)))),
                    numbers(5, &|r| RichText::new(format!("{} / {}", r.today.bought, r.today.sold))),
                    table::Fit::fixed(table::column_width(ui, titles[6], quotes.iter().map(|q| RichText::new(q).into()))),
                    table::Fit::fixed(buttons),
                ],
                ui.available_width() - 12.0,
            );
            let columns: Vec<(&str, f32, CellAlign)> =
                titles.iter().zip(&w).enumerate().map(|(i, (t, w))| (*t, *w, if i == 0 || i == 7 { CellAlign::Left } else { CellAlign::Right })).collect();
            table::header(ui, &columns);
            for (i, (row, quote)) in orders.iter().zip(&quotes).enumerate() {
                let o = row.order;
                table::row(ui, ("order", o.resource.index()), ROW_HEIGHT + 2.0, i % 2 == 1, false, |ui| {
                    table::cell(ui, w[0], CellAlign::Left, |ui| {
                        if let Some(icon) = resource_icon(o.resource).and_then(|k| ctx.icons.get(&k)) {
                            icon.show(ui, 20.0);
                        }
                        ui.add(egui::Label::new(RichText::new(resource_name(o.resource)).color(Palette::INK)).truncate());
                    });
                    table::text(ui, w[1], CellAlign::Right, int(i64::from(row.stock)));
                    table::text(ui, w[2], CellAlign::Right, threshold(o.buy_below));
                    table::text(ui, w[3], CellAlign::Right, threshold(o.sell_above));
                    table::text(ui, w[4], CellAlign::Right, int(i64::from(o.daily_limit)));
                    table::text(ui, w[5], CellAlign::Right, format!("{} / {}", row.today.bought, row.today.sold));
                    table::text(ui, w[6], CellAlign::Right, quote.as_str());
                    table::cell(ui, w[7], CellAlign::Left, |ui| {
                        ui.add_space(10.0);
                        if ui.small_button("Bearbeiten").clicked() {
                            self.draft = Some(Draft::from_order(&o));
                        }
                        if ui.small_button("Löschen").on_hover_text("Auftrag entfernen").clicked() {
                            command(actions, Command::RemoveTradeOrder { resource: o.resource });
                            if self.draft.is_some_and(|d| d.resource == o.resource) {
                                self.draft = None;
                            }
                        }
                    });
                });
            }
        }
        ui.add_space(6.0);
        editor(&mut self.draft, ui, ctx, data, max_limit, actions);
        self.caravans.show(ui, ctx, actions);
        // Prices.
        section_header(ui, "Marktpreise");
        if data.caravans {
            hint(ui, "Karawanen handeln zu den Preisen ihres Reiches; gute Beziehungen, Verträge, Forschung und Handelsstraßen verbessern sie – höchstens bis auf den Preis des Reiches selbst, Einkaufen kostet also nie weniger, als der Verkauf derselben Ware an dasselbe Reich bringt. Einkauf: das günstigste Reich mit Handelszugang, das die Ware anbietet; Verkauf: das Reich, das sie am besten bezahlt; „–“: kein Reich bietet bzw. sucht sie gerade. Klick auf eine Ware: Auftrag bearbeiten.");
        } else {
            let margin = defs.rules().trade_margin_percent;
            let bonus = trade::trade_bonus_percent(input.session.state().research(), defs);
            hint(ui, world_market_price_hint(margin, bonus));
        }
        let category = |p: &PriceRow| if p.import_only { "nur Einfuhr".to_owned() } else { category_label(p.category).to_owned() };
        let titles = ["Ware", "Kategorie", "Bestand", "Marktpreis", "Einkauf", "Verkauf", "Gestern ge-/verkauft"];
        let prices = &data.prices;
        let measure = |i: usize, f: &dyn Fn(&PriceRow) -> RichText| table::column_width(ui, titles[i], prices.iter().map(|p| f(p).into()));
        let w = table::fit_widths(
            &[
                table::Fit::flex(120.0, 26.0 + measure(0, &|p| RichText::new(&p.name))),
                table::Fit::fixed(measure(1, &|p| RichText::new(category(p)).small())),
                table::Fit::fixed(measure(2, &|p| RichText::new(int(i64::from(p.stock))))),
                table::Fit::fixed(measure(3, &|p| RichText::new(price(p.price)).strong())),
                table::Fit::fixed(measure(4, &|p| RichText::new(quote_text(p.buy)))),
                table::Fit::fixed(measure(5, &|p| RichText::new(quote_text(p.sell)))),
                table::Fit::fixed(measure(6, &|p| RichText::new(format!("{} / {}", p.bought_yesterday, p.sold_yesterday)))),
            ],
            ui.available_width() - 12.0,
        );
        const P_ALIGN: [CellAlign; 7] =
            [CellAlign::Left, CellAlign::Left, CellAlign::Right, CellAlign::Right, CellAlign::Right, CellAlign::Right, CellAlign::Right];
        let columns: Vec<(&str, f32, CellAlign)> = titles.iter().zip(&w).zip(P_ALIGN).map(|((t, w), a)| (*t, *w, a)).collect();
        table::header(ui, &columns);
        for (i, p) in data.prices.iter().enumerate() {
            let response = table::row(ui, ("price", p.resource.index()), ROW_HEIGHT, i % 2 == 1, true, |ui| {
                table::cell(ui, w[0], CellAlign::Left, |ui| {
                    if let Some(icon) = ctx.icons.get(&format!("res_{}", p.key)) {
                        icon.show(ui, 20.0);
                    }
                    ui.add(egui::Label::new(RichText::new(&p.name).color(Palette::INK)).truncate());
                });
                table::text(ui, w[1], CellAlign::Left, RichText::new(category(p)).small().color(Palette::INK_SOFT));
                table::text(ui, w[2], CellAlign::Right, int(i64::from(p.stock)));
                table::text(ui, w[3], CellAlign::Right, RichText::new(price(p.price)).strong());
                table::text(ui, w[4], CellAlign::Right, RichText::new(quote_text(p.buy)).color(Palette::BAD));
                table::text(ui, w[5], CellAlign::Right, RichText::new(quote_text(p.sell)).color(Palette::GOOD));
                table::text(ui, w[6], CellAlign::Right, format!("{} / {}", p.bought_yesterday, p.sold_yesterday));
            });
            if response.clicked() {
                let existing = data.report.orders.iter().find(|r| r.order.resource == p.resource).map(|r| r.order);
                self.draft = Some(existing.as_ref().map_or_else(|| Draft::new(p.resource), Draft::from_order));
                actions.push(HudAction::Sound("ui_click"));
            }
        }
        // Executed trades.
        section_header(ui, "Letzte Geschäfte");
        if self.log.is_empty() {
            hint(ui, "Seit dem Start bzw. Laden des Spiels wurde noch nichts gehandelt.");
        }
        let log: Vec<(&TradeEntry, String, String, String)> = self
            .log
            .iter()
            .take(12)
            .map(|t| {
                let clock = GameClock::at_tick(t.tick);
                let when = format!("{} {} · {}", clock.season().display_name(), clock.day_of_season(), clock.time_string());
                (t, when, format!("{} {}", int(i64::from(t.amount)), resource_name(t.resource)), format!("{} Gold", crate::ui::hud::format::signed(t.gold)))
            })
            .collect();
        let w = table::fit_widths(
            &[
                table::Fit::fixed(table::content_width(ui, log.iter().map(|l| RichText::new(&l.1).small().into()))),
                table::Fit::fixed(table::content_width(ui, log.iter().map(|l| RichText::new(l.0.direction.display_name()).into()))),
                table::Fit::flex(120.0, table::content_width(ui, log.iter().map(|l| RichText::new(&l.2).into()))),
                table::Fit::fixed(table::content_width(ui, log.iter().map(|l| RichText::new(&l.3).into())) + 8.0),
            ],
            ui.available_width() - 12.0,
        );
        for (i, (t, when, goods, gold)) in log.iter().enumerate() {
            table::row(ui, ("trade_log", i), ROW_HEIGHT - 2.0, i % 2 == 1, false, |ui| {
                table::text(ui, w[0], CellAlign::Left, RichText::new(when).small().color(Palette::INK_SOFT));
                table::text(ui, w[1], CellAlign::Left, t.direction.display_name());
                table::text(ui, w[2], CellAlign::Left, goods.as_str());
                table::text(ui, w[3], CellAlign::Right, RichText::new(gold).color(table::signed_color(t.gold as f64)));
            });
        }
    }
}

/// The order editor (`draft`: the order being edited).
/// Trade surplus or deficit: today so far, the recent average and the daily history.
fn balance_card(ui: &mut Ui, b: &TradeBalance) {
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Handelsbilanz").font(heading_font(17.0)).color(Palette::INK));
            let color = table::signed_color(b.average_milli as f64);
            ui.label(RichText::new(&b.text).color(if b.average_milli == 0 { Palette::INK_SOFT } else { color }));
        });
        ui.label(
            RichText::new(format!(
                "Heute bisher: Ausfuhr {} Gold, Einfuhr {} Gold, Bilanz {} Gold",
                signed_gold_milli(b.today.exports_milli).trim_start_matches('+'),
                signed_gold_milli(b.today.imports_milli).trim_start_matches('+'),
                signed_gold_milli(b.today.balance_milli())
            ))
            .small()
            .color(Palette::INK_SOFT),
        );
        let labels: Vec<String> = b.days.iter().map(|d| super::day_label(d.day)).collect();
        let gold = |milli: i64| milli as f64 / 1000.0;
        let series = [
            Series::bar("Ausfuhr", Palette::GOOD, b.days.iter().map(|d| gold(d.exports_milli)).collect()),
            Series::bar("Einfuhr", Palette::BAD, b.days.iter().map(|d| -gold(d.imports_milli)).collect()),
            Series::line("Bilanz", Palette::INK, b.days.iter().map(|d| gold(d.balance_milli())).collect()),
        ];
        Chart::new(("chart", "trade_balance"), &labels, &series)
            .height(120.0)
            .format(ValueFormat::Gold)
            .empty_text("Die Bilanz je Tag erscheint nach dem ersten abgeschlossenen Tag.")
            .show(ui);
    });
}

fn editor(draft: &mut Option<Draft>, ui: &mut Ui, ctx: &TabContext<'_, '_>, data: &Data, max_limit: u32, actions: &mut Vec<HudAction>) {
    let defs = ctx.input.session.defs();
    let Some(first) = defs.resources().first() else { return };
    // Start with the first standing order, else a new order for the first good.
    let draft = draft.get_or_insert_with(|| data.report.orders.first().map_or_else(|| Draft::new(first.id), |r| Draft::from_order(&r.order)));
    let existing = data.report.orders.iter().find(|r| r.order.resource == draft.resource);
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let title = if existing.is_some() { "Auftrag bearbeiten" } else { "Neuer Auftrag" };
        ui.label(RichText::new(title).font(heading_font(16.0)).color(Palette::INK));
        ui.horizontal(|ui| {
            ui.label(RichText::new("Ware").color(Palette::INK_SOFT));
            let name = defs.resources().get(draft.resource.index()).map_or_else(String::new, |r| r.name.clone());
            let before = draft.resource;
            egui::ComboBox::from_id_salt("trade_resource").selected_text(name).width(190.0).height(320.0).show_ui(ui, |ui| {
                for r in defs.resources() {
                    ui.selectable_value(&mut draft.resource, r.id, &r.name);
                }
            });
            if draft.resource != before {
                // Switching to a good with an order loads it.
                *draft = data.report.orders.iter().find(|r| r.order.resource == draft.resource).map_or_else(|| Draft::new(draft.resource), |r| Draft::from_order(&r.order));
            }
            if let Some(p) = data.prices.iter().find(|p| p.resource == draft.resource) {
                ui.label(RichText::new(format!("Bestand {} · Einkauf {} · Verkauf {}", int(i64::from(p.stock)), quote_gold_text(p.buy), quote_gold_text(p.sell))).small().color(Palette::INK_SOFT));
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut draft.buy, "Kaufen, solange der Bestand unter");
            ui.add_enabled(draft.buy, egui::DragValue::new(&mut draft.buy_below).range(0..=100_000).speed(1.0));
            ui.label("liegt");
            ui.add_space(20.0);
            ui.checkbox(&mut draft.sell, "Verkaufen, was über");
            ui.add_enabled(draft.sell, egui::DragValue::new(&mut draft.sell_above).range(0..=100_000).speed(1.0));
            ui.label("liegt");
        });
        ui.horizontal(|ui| {
            ui.label("Höchstens");
            ui.add(egui::DragValue::new(&mut draft.daily_limit).range(0..=max_limit).speed(1.0));
            ui.label(format!("Einheiten am Tag (1–{})", int(i64::from(max_limit))));
        });
        let order = draft.order();
        let check = trade::validate_order(&order, defs);
        ui.horizontal(|ui| {
            let save = if existing.is_some() { "Änderung speichern" } else { "Auftrag anlegen" };
            let unchanged = existing.is_some_and(|r| r.order == order);
            if ui.add_enabled(!unchanged, egui::Button::new(RichText::new(save).strong())).clicked() {
                // Invalid orders are sent as well: the simulation's rejection is shown as a toast.
                command(actions, Command::SetTradeOrder { order });
            }
            if existing.is_some() && ui.button("Auftrag löschen").clicked() {
                command(actions, Command::RemoveTradeOrder { resource: order.resource });
            }
            match &check {
                Ok(()) => {
                    if unchanged {
                        ui.label(RichText::new("Der Auftrag gilt so.").small().color(Palette::INK_SOFT));
                    }
                }
                Err(reason) => {
                    mark(ui, Mark::Error);
                    ui.label(RichText::new(reason.to_string()).color(Palette::BAD));
                }
            }
        });
        if data.prices.iter().any(|p| p.resource == order.resource && p.import_only) && order.sell_above.is_some() {
            hint(ui, "Diese Ware kann nicht hergestellt, nur eingeführt werden.");
        }
    });
}

/// A threshold or „–“.
fn threshold(value: Option<u32>) -> RichText {
    match value {
        Some(v) => RichText::new(int(i64::from(v))).color(Palette::INK),
        None => RichText::new("–").color(Palette::INK_SOFT),
    }
}

/// Explanation of the world-market prices: the margin both ways, the research bonus and its cap
/// (`economy::trade::trade_percent`: a bonus narrows the margin at most to the market price).
fn world_market_price_hint(margin: u8, research_bonus: i32) -> String {
    let shown = research_bonus.min(i32::from(margin));
    let research = if shown > 0 { format!(" Die Forschung verbessert beide Preise um {shown} Prozentpunkte.") } else { String::new() };
    format!(
        "Einkauf kostet den Marktpreis plus {margin} % Aufschlag, Verkauf bringt den Marktpreis abzüglich {margin} %.{research} Forschung und Handelsstraße verringern Auf- und Abschlag höchstens bis auf den Marktpreis: Einkaufen kostet nie weniger, als der Verkauf derselben Ware bringt. Klick auf eine Ware: Auftrag bearbeiten."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::hud::management::tests::test_defs;
    use kingdoms_sim::state::{GameSettings, GameState};
    use kingdoms_sim::world::WorldSize;

    #[test]
    fn drafts_round_trip_orders() {
        let defs = test_defs();
        let wood = defs.resource_id("wood").expect("wood");
        let draft = Draft::new(wood);
        assert_eq!(draft.order(), TradeOrder { resource: wood, buy_below: None, sell_above: Some(200), daily_limit: 50 });
        let order = TradeOrder { resource: wood, buy_below: Some(20), sell_above: None, daily_limit: 7 };
        assert_eq!(Draft::from_order(&order).order(), order);
        // The editor's preview uses the simulation's own validation.
        let mut empty = Draft::new(wood);
        empty.sell = false;
        assert!(trade::validate_order(&empty.order(), &defs).is_err());
        assert!(trade::validate_order(&draft.order(), &defs).is_ok());
    }

    #[test]
    fn the_price_hint_names_the_capped_bonus() {
        let plain = world_market_price_hint(20, 0);
        assert!(plain.starts_with("Einkauf kostet den Marktpreis plus 20 % Aufschlag"));
        assert!(!plain.contains("Die Forschung"));
        assert!(plain.contains("höchstens bis auf den Marktpreis"));
        assert!(world_market_price_hint(20, 10).contains("um 10 Prozentpunkte"));
        // A bonus beyond the margin only closes it.
        assert!(world_market_price_hint(20, 35).contains("um 20 Prozentpunkte"));
    }

    #[test]
    fn trading_needs_a_post() {
        let defs = test_defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let report = report::trade_report(&state, &defs);
        assert_eq!(report.trading_post, None);
        assert_eq!(post_status(&state, &defs, report.trading_post), PostStatus::NotBuilt);
        assert!(PostStatus::NotBuilt.text(true).contains("Kein Handelskontor"));
        let post = BuildingId::from_raw_parts(1, 0);
        assert!(PostStatus::Active(post).text(false).contains("stündlich"));
        assert!(PostStatus::Active(post).text(true).contains("Karawanen"));
    }

    #[test]
    fn kingdoms_quote_the_prices_of_their_caravans() {
        let defs = test_defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        assert!(!state.diplomacy().is_empty(), "a small map has kingdoms");
        let Some(quotes) = partner_quotes(&state, &defs) else { panic!("kingdoms quote prices") };
        assert_eq!(quotes.len(), defs.resources().len());
        // Every quote is a price of a kingdom with access that offers (buy) or wants (sell) the good.
        let rows: Vec<Vec<report::PartnerPriceRow>> = state
            .diplomacy()
            .kingdoms()
            .filter(|(id, _)| has_access(&state, &defs, *id))
            .map(|(id, _)| report::partner_prices(&state, &defs, id))
            .collect();
        assert!(!rows.is_empty(), "neutral kingdoms trade at the start");
        for (i, quote) in quotes.iter().enumerate() {
            let offers = rows.iter().filter(|r| r[i].offered > 0).map(|r| r[i].buy_centi).min();
            let wants = rows.iter().filter(|r| r[i].wanted > 0).map(|r| r[i].sell_centi).max();
            assert_eq!((quote.buy, quote.sell), (offers, wants), "resource {i}");
        }
        assert!(quotes.iter().any(|q| q.buy.is_some()), "something is on offer");
        assert_eq!(quote_text(None), "–");
        assert_eq!(quote_gold_text(None), "–");
        assert_eq!(quote_gold_text(Some(228)), "2,28 Gold");
        assert_eq!(quote_pair_text(Some(228), Some(190)), "2,28 / 1,90 Gold");
        assert_eq!(quote_pair_text(Some(228), None), "2,28 Gold / –");
        assert_eq!(quote_pair_text(None, None), "–");
    }
}
