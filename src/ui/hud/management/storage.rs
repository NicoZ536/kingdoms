//! „Lager & Waren“: every resource with kingdom stock, material bound to construction sites,
//! today's production, consumption and household use, the daily trend, price and price trend,
//! grouped by category (filterable, sortable), and every storage building with its fill level.

use kingdoms_engine::egui::{self, Color32, RichText, Ui};
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::defs::ResourceCategory;
use kingdoms_sim::report::{self, LogisticsReport, ResourceRow};

use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, Navigate, TabContext, card_row, hint, select};
use crate::ui::hud::HudAction;
use crate::ui::hud::format::{decimal_fixed, int, signed};
use crate::ui::theme::{Palette, heading_font};
use crate::ui::widgets::{mini_bar, section_header};

/// Plural German label of a category for the filter.
pub fn category_label(category: ResourceCategory) -> &'static str {
    match category {
        ResourceCategory::Raw => "Rohstoffe",
        ResourceCategory::Food => "Nahrung",
        ResourceCategory::Goods => "Waren",
        ResourceCategory::Luxury => "Luxusgüter",
    }
}

/// Column the goods table is sorted by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    /// Grouped by category, data order within (the default).
    #[default]
    Category,
    Name,
    Stock,
    Produced,
    Consumed,
    Trend,
    Price,
}

/// One resource of the table.
#[derive(Clone, Debug)]
struct Goods {
    key: String,
    name: String,
    category: ResourceCategory,
    row: ResourceRow,
    /// A day was closed already (trend and price trend exist).
    has_history: bool,
}

impl Goods {
    /// Net units per day on the last closed day.
    fn trend(&self) -> i64 {
        i64::from(self.row.produced_per_day) - i64::from(self.row.consumed_per_day)
    }

    /// True if the kingdom ever dealt with it (stock, bound material or flows).
    fn is_relevant(&self) -> bool {
        let r = &self.row;
        r.stock > 0 || r.committed > 0 || r.produced_today + r.consumed_today + r.produced_per_day + r.consumed_per_day > 0
    }
}

/// German summary of the carriers' work: who fetches, delivers and walks back, what is on the
/// way and how long a trip takes.
pub fn logistics_line(l: &LogisticsReport) -> String {
    let trip = l.average_trip_minutes.map_or_else(|| "noch keine".to_owned(), |m| format!("Ø {m} min"));
    // Number, then the word that agrees with it („1 holt ab“, „2 holen ab“).
    let agree = |n: u64, one: &str, many: &str| format!("{} {}", int(i64::try_from(n).unwrap_or(i64::MAX)), if n == 1 { one } else { many });
    format!(
        "Träger: {} ab, {}, {} auf dem Rückweg, {} frei · {} unterwegs · {} Ertrag heim · Transportdauer {trip} · {} erledigt",
        agree(u64::from(l.fetching), "holt", "holen"),
        agree(u64::from(l.delivering), "liefert", "liefern"),
        l.returning,
        l.idle_carriers,
        agree(u64::from(l.units_in_transit), "Einheit", "Einheiten"),
        agree(u64::from(l.hauls), "Sammler trägt", "Sammler tragen"),
        agree(l.trips_completed, "Transport", "Transporte"),
    )
}

/// One storage building.
#[derive(Clone, Debug)]
struct Store {
    building: BuildingId,
    icon: String,
    name: String,
    reachable: bool,
    carriers: u32,
    busy: u32,
    idle: u32,
    waiting: u32,
    used: u64,
    capacity: u64,
    /// Names of resources whose slot is full.
    full: Vec<String>,
}

struct Data {
    goods: Vec<Goods>,
    stores: Vec<Store>,
    logistics: LogisticsReport,
}

/// State of the tab.
#[derive(Default)]
pub struct StorageTab {
    filter: Option<ResourceCategory>,
    show_all: bool,
    sort: SortKey,
    descending: bool,
    data: Cached<Data>,
}

/// The goods to show in display order.
fn visible(goods: &[Goods], filter: Option<ResourceCategory>, show_all: bool, sort: SortKey, descending: bool) -> Vec<&Goods> {
    let mut list: Vec<&Goods> = goods.iter().filter(|g| filter.is_none_or(|c| g.category == c) && (show_all || g.is_relevant())).collect();
    let category_rank = |c: ResourceCategory| ResourceCategory::ALL.iter().position(|x| *x == c).unwrap_or(usize::MAX);
    match sort {
        SortKey::Category => list.sort_by_key(|g| (category_rank(g.category), g.row.resource)),
        SortKey::Name => list.sort_by(|a, b| a.name.cmp(&b.name)),
        SortKey::Stock => list.sort_by_key(|g| g.row.stock),
        SortKey::Produced => list.sort_by_key(|g| g.row.produced_today),
        SortKey::Consumed => list.sort_by_key(|g| g.row.consumed_today),
        SortKey::Trend => list.sort_by_key(|g| g.trend()),
        SortKey::Price => list.sort_by_key(|g| g.row.price_centi),
    }
    if descending && sort != SortKey::Category {
        list.reverse();
    }
    list
}

impl StorageTab {
    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) -> Option<Navigate> {
        let input = ctx.input;
        let data = self.data.get(input, || {
            let state = input.session.state();
            let defs = input.session.defs();
            let has_history = state.economy().stats().yesterday().is_some();
            let economy = report::economy_summary(state, defs);
            let goods = economy
                .resources
                .iter()
                .filter_map(|row| {
                    let def = defs.resources().get(row.resource.index())?;
                    Some(Goods { key: def.key.clone(), name: def.name.clone(), category: def.category, row: *row, has_history })
                })
                .collect();
            let stores = report::storage_report(state, defs)
                .into_iter()
                .filter_map(|s| {
                    let def = defs.buildings().get(s.kind.index())?;
                    let used = s.contents.iter().map(|c| u64::from(c.amount)).sum();
                    let capacity = s.contents.iter().map(|c| u64::from(c.capacity)).sum();
                    let full = s
                        .contents
                        .iter()
                        .filter(|c| c.capacity > 0 && c.amount >= c.capacity)
                        .filter_map(|c| defs.resources().get(c.resource.index()).map(|r| r.name.clone()))
                        .collect();
                    Some(Store {
                        building: s.building,
                        icon: format!("bld_{}", def.key),
                        name: def.name.clone(),
                        reachable: s.reachable,
                        carriers: s.carriers,
                        busy: s.busy_carriers,
                        idle: s.idle_carriers,
                        waiting: s.waiting_jobs,
                        used,
                        capacity,
                        full,
                    })
                })
                .collect();
            Data { goods, stores, logistics: report::logistics_report(state, defs) }
        });
        let mut nav = None;
        // Overview.
        let total: u64 = data.goods.iter().map(|g| u64::from(g.row.stock)).sum();
        let food: u64 = data.goods.iter().filter(|g| g.category == ResourceCategory::Food).map(|g| u64::from(g.row.stock)).sum();
        let committed: u64 = data.goods.iter().map(|g| u64::from(g.row.committed)).sum();
        let reachable = data.stores.iter().filter(|s| s.reachable).count();
        let l = &data.logistics;
        card_row(
            ui,
            &[
                ("Waren im Reichslager", int(total as i64), Palette::INK),
                ("davon Nahrung", int(food as i64), if food == 0 { Palette::BAD } else { Palette::INK }),
                ("Für Baustellen gebunden", int(committed as i64), Palette::INK),
                ("Lagerhäuser erreichbar", format!("{reachable} von {}", data.stores.len()), if reachable < data.stores.len() { Palette::WARN } else { Palette::INK }),
                ("Träger unterwegs", format!("{} von {}", l.busy_carriers, l.carriers), Palette::INK),
                ("Transporte wartend", int(i64::from(l.waiting)), if l.waiting > 10 { Palette::WARN } else { Palette::INK }),
            ],
        );
        ui.add_space(4.0);
        ui.label(RichText::new(logistics_line(l)).color(Palette::INK_SOFT));
        ui.add_space(6.0);
        // Filter.
        ui.horizontal(|ui| {
            ui.label(RichText::new("Kategorie").color(Palette::INK_SOFT));
            if ui.selectable_label(self.filter.is_none(), "Alle").clicked() {
                self.filter = None;
            }
            for c in ResourceCategory::ALL {
                if ui.selectable_label(self.filter == Some(c), category_label(c)).clicked() {
                    self.filter = Some(c);
                }
            }
            ui.add_space(16.0);
            ui.checkbox(&mut self.show_all, "Auch Waren ohne Bestand und Umsatz")
                .on_hover_text("Ohne Haken erscheinen nur Waren, die das Reich hat, braucht oder bewegt.");
            if self.sort != SortKey::Category && ui.small_button("Nach Kategorien gruppieren").clicked() {
                self.sort = SortKey::Category;
            }
        });
        ui.add_space(2.0);
        // Goods table.
        const W: [f32; 10] = [176.0, 84.0, 74.0, 76.0, 80.0, 78.0, 90.0, 66.0, 70.0, 60.0];
        table::row(ui, "goods_header", 22.0, false, false, |ui| {
            let mut sort = |ui: &mut Ui, title: &str, width: f32, align: CellAlign, key: SortKey| {
                if table::sort_header(ui, title, width, align, self.sort == key, self.descending) {
                    if self.sort == key {
                        self.descending = !self.descending;
                    } else {
                        self.sort = key;
                        self.descending = !matches!(key, SortKey::Name | SortKey::Category);
                    }
                }
            };
            sort(ui, "Ware", W[0], CellAlign::Left, SortKey::Name);
            sort(ui, "Bestand", W[1], CellAlign::Right, SortKey::Stock);
            table::text(ui, W[2], CellAlign::Right, RichText::new("Lagerplatz").small().strong().color(Palette::INK_SOFT));
            table::text(ui, W[3], CellAlign::Right, RichText::new("Für Bau").small().strong().color(Palette::INK_SOFT));
            sort(ui, "Heute +", W[4], CellAlign::Right, SortKey::Produced);
            sort(ui, "Heute −", W[5], CellAlign::Right, SortKey::Consumed);
            table::text(ui, W[6], CellAlign::Right, RichText::new("Haushalte").small().strong().color(Palette::INK_SOFT));
            sort(ui, "Trend/Tag", W[7], CellAlign::Right, SortKey::Trend);
            sort(ui, "Preis", W[8], CellAlign::Right, SortKey::Price);
            table::text(ui, W[9], CellAlign::Right, RichText::new("Preis ±").small().strong().color(Palette::INK_SOFT));
        });
        table::rule(ui);
        let list = visible(&data.goods, self.filter, self.show_all, self.sort, self.descending);
        if list.is_empty() {
            hint(ui, "Keine Waren in dieser Auswahl.");
        }
        let mut last_category = None;
        for (i, g) in list.iter().enumerate() {
            if self.sort == SortKey::Category && last_category != Some(g.category) {
                last_category = Some(g.category);
                ui.add_space(2.0);
                ui.label(RichText::new(category_label(g.category)).font(heading_font(14.0)).color(Palette::GOLD_DARK));
            }
            let r = &g.row;
            let response = table::row(ui, ("goods", r.resource.index()), ROW_HEIGHT, i % 2 == 1, true, |ui| {
                table::cell(ui, W[0], CellAlign::Left, |ui| {
                    if let Some(icon) = ctx.icons.get(&format!("res_{}", g.key)) {
                        icon.show(ui, 20.0);
                    }
                    ui.add(egui::Label::new(RichText::new(&g.name).color(Palette::INK)).truncate());
                });
                let full = r.capacity > 0 && r.stock >= r.capacity;
                let stock_color = if full { Palette::WARN } else if r.stock == 0 { Palette::INK_SOFT } else { Palette::INK };
                table::text(ui, W[1], CellAlign::Right, RichText::new(int(i64::from(r.stock))).strong().color(stock_color));
                table::text(ui, W[2], CellAlign::Right, RichText::new(int(i64::from(r.capacity))).color(Palette::INK_SOFT));
                table::text(ui, W[3], CellAlign::Right, quantity(r.committed));
                table::text(ui, W[4], CellAlign::Right, quantity(r.produced_today));
                table::text(ui, W[5], CellAlign::Right, quantity(r.consumed_today));
                table::text(ui, W[6], CellAlign::Right, quantity(r.household_today));
                let trend = g.trend();
                let trend_text = if g.has_history { signed(trend) } else { "–".to_owned() };
                table::text(ui, W[7], CellAlign::Right, RichText::new(trend_text).color(table::signed_color(trend as f64)));
                table::text(ui, W[8], CellAlign::Right, RichText::new(price(r.price_centi)).color(Palette::INK));
                let price_trend = if g.has_history { signed_price(r.price_trend_centi) } else { "–".to_owned() };
                table::text(ui, W[9], CellAlign::Right, RichText::new(price_trend).small().color(price_color(r.price_trend_centi)));
            });
            let response = response.on_hover_ui(|ui| goods_tooltip(ui, g));
            if response.clicked() {
                nav = Some(Navigate::ResourceStatistics(r.resource));
                actions.push(HudAction::Sound("ui_click"));
            }
        }
        // Storage buildings.
        section_header(ui, "Lagerhäuser");
        if data.stores.is_empty() {
            hint(ui, "Noch kein fertiges Lager.");
        }
        const S: [f32; 4] = [230.0, 300.0, 120.0, 260.0];
        table::header(ui, &[("Gebäude", S[0], CellAlign::Left), ("Füllstand", S[1], CellAlign::Left), ("Träger", S[2], CellAlign::Right), ("Zustand", S[3], CellAlign::Left)]);
        for (i, s) in data.stores.iter().enumerate() {
            let response = table::row(ui, ("store", s.building.index()), ROW_HEIGHT + 2.0, i % 2 == 1, true, |ui| {
                table::cell(ui, S[0], CellAlign::Left, |ui| {
                    if let Some(icon) = ctx.icons.get(&s.icon) {
                        icon.show(ui, 22.0);
                    }
                    ui.add(egui::Label::new(RichText::new(&s.name).color(Palette::INK)).truncate());
                });
                table::cell(ui, S[1], CellAlign::Left, |ui| {
                    let fraction = if s.capacity == 0 { 0.0 } else { s.used as f32 / s.capacity as f32 };
                    mini_bar(ui, fraction, fill_color(fraction), 150.0);
                    ui.label(RichText::new(format!("{} / {}", int(s.used as i64), int(s.capacity as i64))).small().color(Palette::INK_SOFT));
                });
                table::text(ui, S[2], CellAlign::Right, format!("{} von {}", s.busy, s.carriers));
                table::cell(ui, S[3], CellAlign::Left, |ui| {
                    ui.add_space(12.0);
                    let (text, color) = if !s.reachable {
                        ("nicht mit dem Rathaus verbunden".to_owned(), Palette::BAD)
                    } else if s.full.is_empty() {
                        ("in Betrieb".to_owned(), Palette::GOOD)
                    } else {
                        (format!("voll: {}", s.full.join(", ")), Palette::WARN)
                    };
                    ui.add(egui::Label::new(RichText::new(text).color(color)).truncate());
                });
            });
            let waiting = if s.waiting == 1 { "1 Transport wartet".to_owned() } else { format!("{} Transporte warten", s.waiting) };
            let hover = format!(
                "Träger: {} unterwegs, {} frei im Lager · {waiting}\nKlick: Lager auswählen und hinschauen",
                s.busy, s.idle
            );
            if response.on_hover_text(hover).clicked() {
                select(actions, s.building);
            }
        }
        nav
    }
}

fn goods_tooltip(ui: &mut Ui, g: &Goods) {
    let r = &g.row;
    ui.label(RichText::new(&g.name).strong());
    egui::Grid::new(("goods_tt", r.resource.index())).num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
        crate::ui::widgets::stat_row(ui, "Kategorie", category_label(g.category));
        crate::ui::widgets::stat_row(ui, "Bestand / Lagerplatz", format!("{} / {}", int(i64::from(r.stock)), int(i64::from(r.capacity))));
        if g.has_history {
            crate::ui::widgets::stat_row(ui, "Gestern erzeugt", int(i64::from(r.produced_per_day)));
            crate::ui::widgets::stat_row(ui, "Gestern verbraucht", int(i64::from(r.consumed_per_day)));
            crate::ui::widgets::stat_row(ui, "  davon Haushalte", int(i64::from(r.household_per_day)));
        }
        crate::ui::widgets::stat_row(ui, "Grundpreis", format!("{} Gold", price(r.base_price_centi)));
    });
    ui.label(RichText::new("Klick: Verlauf in der Statistik").small().color(Palette::INK_SOFT));
}

/// A count, soft when zero.
fn quantity(n: u32) -> RichText {
    if n == 0 { RichText::new("–").color(Palette::INK_SOFT) } else { RichText::new(int(i64::from(n))).color(Palette::INK) }
}

/// Price in gold from centi-gold, always with two decimals so a column lines up („1,25“,
/// „3,50“, „3,00“).
pub fn price(centi: u32) -> String {
    decimal_fixed(f64::from(centi) / 100.0, 2)
}

/// Signed price change from centi-gold („+0,05“, „−1,20“, „0“).
pub fn signed_price(centi: i64) -> String {
    match centi {
        0 => "0".to_owned(),
        c if c > 0 => format!("+{}", decimal_fixed(c as f64 / 100.0, 2)),
        c => decimal_fixed(c as f64 / 100.0, 2),
    }
}

/// Rising prices are drawn in amber (more expensive), falling in green.
fn price_color(centi: i64) -> Color32 {
    match centi {
        c if c > 0 => Palette::WARN,
        c if c < 0 => Palette::GOOD,
        _ => Palette::INK_SOFT,
    }
}

/// Fill colour: green while there is room, amber when nearly full, red when full.
fn fill_color(fraction: f32) -> Color32 {
    if fraction >= 1.0 {
        Palette::BAD
    } else if fraction >= 0.85 {
        Palette::WARN
    } else {
        Palette::GOOD
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::hud::management::tests::test_defs as defs;
    use kingdoms_sim::defs::Defs;

    fn goods(defs: &Defs, key: &str, stock: u32, produced: u32, consumed: u32) -> Goods {
        let def = defs.resource_by_key(key).expect("resource");
        Goods {
            key: key.to_owned(),
            name: def.name.clone(),
            category: def.category,
            row: ResourceRow {
                resource: def.id,
                stock,
                capacity: 400,
                committed: 0,
                produced_today: produced,
                consumed_today: consumed,
                produced_per_day: produced,
                consumed_per_day: consumed,
                household_today: 0,
                household_per_day: 0,
                price_centi: 100,
                base_price_centi: 100,
                price_trend_centi: 0,
            },
            has_history: true,
        }
    }

    #[test]
    fn filter_sort_and_relevance() {
        let defs = defs();
        let list = vec![
            goods(&defs, "wood", 60, 10, 4),
            goods(&defs, "bread", 40, 0, 12),
            goods(&defs, "planks", 0, 0, 0),
            goods(&defs, "stone", 30, 0, 0),
        ];
        // Unused goods are hidden unless asked for.
        fn names(v: Vec<&Goods>) -> Vec<String> {
            v.iter().map(|g| g.key.clone()).collect()
        }
        assert_eq!(names(visible(&list, None, false, SortKey::Category, false)), vec!["wood", "stone", "bread"]);
        assert_eq!(visible(&list, None, true, SortKey::Category, false).len(), 4);
        assert_eq!(names(visible(&list, Some(ResourceCategory::Food), false, SortKey::Category, false)), vec!["bread"]);
        assert_eq!(names(visible(&list, None, false, SortKey::Stock, true)), vec!["wood", "bread", "stone"]);
        assert_eq!(names(visible(&list, None, false, SortKey::Trend, false)), vec!["bread", "stone", "wood"]);
        assert_eq!(list[0].trend(), 6);
    }

    #[test]
    fn logistics_line_agrees_with_its_numbers() {
        let one = LogisticsReport { fetching: 1, delivering: 1, units_in_transit: 1, hauls: 1, trips_completed: 1, ..Default::default() };
        assert_eq!(
            logistics_line(&one),
            "Träger: 1 holt ab, 1 liefert, 0 auf dem Rückweg, 0 frei · 1 Einheit unterwegs · 1 Sammler trägt Ertrag heim · \
             Transportdauer noch keine · 1 Transport erledigt"
        );
        let many = LogisticsReport { fetching: 2, delivering: 3, units_in_transit: 12, hauls: 2, trips_completed: 1500, average_trip_minutes: Some(9), ..Default::default() };
        assert_eq!(
            logistics_line(&many),
            "Träger: 2 holen ab, 3 liefern, 0 auf dem Rückweg, 0 frei · 12 Einheiten unterwegs · 2 Sammler tragen Ertrag heim · \
             Transportdauer Ø 9 min · 1.500 Transporte erledigt"
        );
    }

    #[test]
    fn german_signs_and_prices() {
        assert_eq!(price(125), "1,25");
        assert_eq!(price(300), "3,00");
        assert_eq!(price(350), "3,50");
        assert_eq!(signed_price(5), "+0,05");
        assert_eq!(signed_price(-120), "−1,20");
        assert_eq!(fill_color(1.0), Palette::BAD);
        assert_eq!(fill_color(0.5), Palette::GOOD);
    }
}
