//! Caravans and trading partners in the „Handel“ tab (docs/game_design.md §10): the caravans on
//! the way (foreign merchants and own caravans) with stage, time left, cargo, gold, the
//! robbery risk and the trade road bonus of their walk; the kingdoms as partners with their buy and sell prices for every good they
//! want or offer; and the dialog that sends an own caravan with goods to sell, gold and goods to
//! buy, checked live with the simulation's `caravans::send_problem`.

use kingdoms_engine::egui::{self, Align, Layout, RichText, Ui};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{Defs, ResourceAmount, ResourceId};
use kingdoms_sim::diplomacy::caravans::{self, CaravanKind, MAX_OWN_CARAVANS};
use kingdoms_sim::diplomacy::{KingdomId, has_access};
use kingdoms_sim::report::{self, CaravanRow, PartnerPriceRow};
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::TICKS_PER_HOUR;

use super::storage::price;
use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, TabContext, command, hint};
use crate::ui::hud::format::int;
use crate::ui::hud::heraldry::crest;
use crate::ui::hud::{HudAction, Icons};
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Mark, mark, section_header};

/// Texts of a caravan's row.
struct CaravanLine {
    who: String,
    stage: String,
    cargo: String,
    risk: String,
    route: (String, egui::Color32),
}

/// A kingdom as trading partner.
struct Partner {
    id: KingdomId,
    name: String,
    color: u8,
    access: bool,
    danger: u32,
    /// Hours of one leg of an own caravan.
    travel_hours: u64,
    prices: Vec<PartnerPriceRow>,
}

struct Data {
    partners: Vec<Partner>,
    caravans: Vec<CaravanRow>,
    /// Trade road share of each caravan's walk on the map so far: (caravan id, tiles walked,
    /// bonus in percent).
    routes: Vec<(u32, u32, i32)>,
    own: usize,
    capacity: u32,
}

/// An own caravan being put together.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaravanDraft {
    pub goods: Vec<ResourceAmount>,
    pub gold: u32,
    pub buy: Vec<ResourceAmount>,
    pick_sell: Option<ResourceId>,
    pick_buy: Option<ResourceId>,
}

impl CaravanDraft {
    /// Units carried out.
    pub fn units(&self) -> u32 {
        self.goods.iter().map(|g| g.amount).sum()
    }

    /// Non-empty rows of a list.
    fn cleaned(list: &[ResourceAmount]) -> Vec<ResourceAmount> {
        list.iter().filter(|g| g.amount > 0).copied().collect()
    }

    /// The command that sends the caravan to `kingdom`.
    pub fn command(&self, kingdom: KingdomId) -> Command {
        Command::SendCaravan { kingdom, goods: Self::cleaned(&self.goods), gold: self.gold, buy: Self::cleaned(&self.buy) }
    }

    /// Expected earnings of the goods sold and costs of the goods bought at the partner's
    /// prices, in centi-gold.
    pub fn estimate(&self, prices: &[PartnerPriceRow]) -> (u64, u64) {
        let at = |r: ResourceId| prices.iter().find(|p| p.resource == r);
        let sold = self.goods.iter().map(|g| u64::from(g.amount) * u64::from(at(g.resource).map_or(0, |p| p.sell_centi))).sum();
        let bought = self.buy.iter().map(|g| u64::from(g.amount) * u64::from(at(g.resource).map_or(0, |p| p.buy_centi))).sum();
        (sold, bought)
    }
}

/// The caravan part of the trade tab.
#[derive(Default)]
pub struct CaravanPanel {
    data: Cached<Data>,
    partner: Option<KingdomId>,
    /// Show every good of the partner, not only those it wants or offers.
    all_goods: bool,
    /// The send dialog (partner and draft).
    dialog: Option<(KingdomId, CaravanDraft)>,
}

impl CaravanPanel {
    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let input = ctx.input;
        let (state, defs) = (input.session.state(), input.session.defs());
        let data = self.data.get(input, || compute(state, defs));
        section_header(ui, "Karawanen unterwegs");
        if data.caravans.is_empty() {
            hint(ui, "Gerade ist keine Karawane unterwegs. Fremde Händler kommen, wenn Eure Handelsaufträge zu ihren Wünschen oder Angeboten passen.");
        }
        // Texts of every row first: the columns fit the widest one, long ones wrap.
        let rows: Vec<CaravanLine> = data
            .caravans
            .iter()
            .map(|c| {
                let goods: Vec<String> = c.goods.iter().take(3).map(|g| format!("{} {}", int(i64::from(g.amount)), g.name)).collect();
                let more = if c.goods.len() > 3 { " …" } else { "" };
                let cargo = if goods.is_empty() { "keine Waren".to_owned() } else { format!("{}{more}", goods.join(", ")) };
                let (tiles, bonus) = data.routes.iter().find(|r| r.0 == c.id).map_or((0, 0), |r| (r.1, r.2));
                CaravanLine {
                    who: match c.kind {
                        CaravanKind::Foreign => format!("Händler aus {}", c.kingdom_name),
                        CaravanKind::Own => format!("Eigene nach {}", c.kingdom_name),
                    },
                    stage: format!("{} {}", c.stage_text, c.eta_text),
                    cargo: format!("{cargo} · {} Gold", int(c.gold)),
                    risk: format!("Risiko {} %", c.danger_percent),
                    route: route_text(tiles, bonus),
                }
            })
            .collect();
        let widths = if rows.is_empty() {
            Vec::new()
        } else {
            let measure = |f: &dyn Fn(&CaravanLine) -> RichText| table::content_width(ui, rows.iter().map(|r| f(r).into()));
            let who = 26.0 + measure(&|r| RichText::new(&r.who).strong());
            let risk = measure(&|r| RichText::new(format!("{} · {}", r.risk, r.route.0)).small()) + 8.0;
            let button = table::text_width(ui, RichText::new("Kamera hin")) + 24.0;
            table::fit_widths(
                &[
                    table::Fit::flex(120.0, who),
                    table::Fit::flex(150.0, measure(&|r| RichText::new(&r.stage))),
                    table::Fit::flex(130.0, measure(&|r| RichText::new(&r.cargo).small())),
                    table::Fit::fixed(risk),
                    table::Fit::fixed(button),
                ],
                ui.available_width() - 12.0,
            )
        };
        for (i, (c, line)) in data.caravans.iter().zip(&rows).enumerate() {
            let color = data.partners.iter().find(|p| p.id == c.kingdom).map_or(0, |p| p.color);
            let height = table::wrapped_height(ui, RichText::new(&line.stage), widths[1], ROW_HEIGHT + 6.0)
                .max(table::wrapped_height(ui, RichText::new(&line.cargo).small(), widths[2], ROW_HEIGHT + 6.0));
            table::row(ui, ("caravan", c.id), height, i % 2 == 1, false, |ui| {
                table::cell(ui, widths[0], CellAlign::Left, |ui| {
                    crest(ui, ctx.icons, 22.0, color);
                    ui.add(egui::Label::new(RichText::new(&line.who).strong().color(Palette::INK)).truncate());
                });
                table::wrapped(ui, widths[1], RichText::new(&line.stage));
                table::wrapped(ui, widths[2], RichText::new(&line.cargo).small());
                table::cell(ui, widths[3], CellAlign::Right, |ui| {
                    ui.label(RichText::new(&line.risk).small().color(danger_color(c.danger_percent)));
                    ui.label(RichText::new("·").small().color(Palette::INK_SOFT));
                    let (text, color) = &line.route;
                    ui.label(RichText::new(text).small().color(*color)).on_hover_text(
                        "Anteil der Handelsstraßen am bisherigen Weg dieser Karawane auf Eurer Karte: Er verbessert die Preise beim Handel.",
                    );
                });
                table::cell(ui, ui.available_width(), CellAlign::Right, |ui| {
                    if let Some(p) = c.position
                        && ui.small_button("Kamera hin").clicked()
                    {
                        actions.push(HudAction::Focus(p));
                        actions.push(HudAction::Sound("ui_click"));
                    }
                });
            });
        }
        section_header(ui, "Handelspartner");
        if data.partners.is_empty() {
            hint(ui, "Es gibt keine anderen Königreiche: Die Aufträge handeln mit dem Weltmarkt.");
            return;
        }
        let selected = self.partner.filter(|id| data.partners.iter().any(|p| p.id == *id)).unwrap_or(data.partners[0].id);
        self.partner = Some(selected);
        ui.horizontal_wrapped(|ui| {
            for p in &data.partners {
                let text = RichText::new(&p.name).color(if p.access { Palette::INK } else { Palette::INK_SOFT });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    crest(ui, ctx.icons, 22.0, p.color);
                    if ui.selectable_label(p.id == selected, text).clicked() {
                        self.partner = Some(p.id);
                    }
                });
            }
        });
        let Some(p) = data.partners.iter().find(|p| p.id == selected) else { return };
        ui.horizontal(|ui| {
            mark(ui, if p.access { Mark::Ok } else { Mark::Warning });
            let access = if p.access { "Handel möglich" } else { "Kein Handel (Krieg oder schlechte Beziehung)" };
            ui.label(RichText::new(format!("{access} · Reise etwa {} h je Weg · Überfallrisiko {} %", p.travel_hours, p.danger)).color(Palette::INK));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let own = format!("Eigene Karawanen: {} von {MAX_OWN_CARAVANS}", data.own);
                if ui.button("Karawane aussenden …").on_hover_text(own).clicked() {
                    self.dialog = Some((p.id, CaravanDraft::default()));
                    actions.push(HudAction::Sound("ui_open"));
                }
            });
        });
        ui.checkbox(&mut self.all_goods, "Alle Waren zeigen (sonst nur, was das Reich sucht oder anbietet)");
        let rows: Vec<&PartnerPriceRow> = p.prices.iter().filter(|r| self.all_goods || r.wanted > 0 || r.offered > 0).collect();
        let titles = ["Ware", "Sucht", "Bietet", "Ihr kauft für", "Ihr verkauft für", "Euer Bestand"];
        let measure = |i: usize, f: &dyn Fn(&PartnerPriceRow) -> RichText| table::column_width(ui, titles[i], rows.iter().map(|r| f(r).into()));
        let w = table::fit_widths(
            &[
                table::Fit::flex(120.0, 26.0 + measure(0, &|r| RichText::new(&r.name))),
                table::Fit::fixed(measure(1, &|r| amount_text(r.wanted))),
                table::Fit::fixed(measure(2, &|r| amount_text(r.offered))),
                table::Fit::fixed(measure(3, &|r| RichText::new(format!("{} Gold", price(r.buy_centi))))),
                table::Fit::fixed(measure(4, &|r| RichText::new(format!("{} Gold", price(r.sell_centi))))),
                table::Fit::fixed(measure(5, &|r| RichText::new(int(i64::from(state.economy().available(r.resource)))))),
            ],
            ui.available_width() - 12.0,
        );
        let columns: Vec<(&str, f32, CellAlign)> =
            titles.iter().zip(&w).enumerate().map(|(i, (t, w))| (*t, *w, if i == 0 { CellAlign::Left } else { CellAlign::Right })).collect();
        table::header(ui, &columns);
        if rows.is_empty() {
            hint(ui, "Dieses Reich sucht und bietet gerade nichts.");
        }
        for (i, r) in rows.iter().enumerate() {
            table::row(ui, ("partner_price", r.resource.index()), ROW_HEIGHT, i % 2 == 1, false, |ui| {
                table::cell(ui, w[0], CellAlign::Left, |ui| {
                    if let Some(icon) = defs.resources().get(r.resource.index()).and_then(|d| ctx.icons.get(&format!("res_{}", d.key))) {
                        icon.show(ui, 20.0);
                    }
                    ui.add(egui::Label::new(RichText::new(&r.name).color(Palette::INK)).truncate());
                });
                table::text(ui, w[1], CellAlign::Right, amount_text(r.wanted));
                table::text(ui, w[2], CellAlign::Right, amount_text(r.offered));
                table::text(ui, w[3], CellAlign::Right, RichText::new(format!("{} Gold", price(r.buy_centi))).color(Palette::BAD));
                table::text(ui, w[4], CellAlign::Right, RichText::new(format!("{} Gold", price(r.sell_centi))).color(Palette::GOOD));
                table::text(ui, w[5], CellAlign::Right, int(i64::from(state.economy().available(r.resource))));
            });
        }
        let capacity = data.capacity;
        self.send_dialog(ui.ctx(), ctx, capacity, actions);
    }

    /// The dialog that puts an own caravan together.
    fn send_dialog(&mut self, ctx: &egui::Context, tab: &TabContext<'_, '_>, capacity: u32, actions: &mut Vec<HudAction>) {
        let Some((kingdom, draft)) = self.dialog.as_mut() else { return };
        let kingdom = *kingdom;
        let (state, defs) = (tab.input.session.state(), tab.input.session.defs());
        let Some(k) = state.diplomacy().kingdom(kingdom) else {
            self.dialog = None;
            return;
        };
        let prices = report::partner_prices(state, defs, kingdom);
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("caravan_dialog")).frame(theme::parchment_frame().inner_margin(egui::Margin::same(18))).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.horizontal(|ui| {
                crest(ui, tab.icons, 40.0, k.color());
                ui.label(RichText::new(format!("Karawane nach {}", k.name())).font(heading_font(20.0)).color(Palette::INK));
            });
            ui.label(RichText::new(format!("Die Karawane bricht am Handelskontor auf, verkauft dort, kauft Eure Wünsche ein und kehrt zurück. Sie trägt höchstens {capacity} Einheiten.")).small().color(Palette::INK_SOFT));
            ui.add_space(4.0);
            ui.label(RichText::new("Zum Verkauf mitnehmen").font(heading_font(15.0)).color(Palette::INK));
            goods_rows(ui, tab.icons, state, defs, &prices, &mut draft.goods, &mut draft.pick_sell, true);
            ui.horizontal(|ui| {
                ui.label("Gold für Einkäufe");
                let max = u32::try_from(state.economy().treasury().max(0)).unwrap_or(u32::MAX).max(1);
                ui.add(egui::DragValue::new(&mut draft.gold).range(0..=max).speed(5.0).suffix(" Gold"));
            });
            ui.label(RichText::new("Einkaufen (Wünsche)").font(heading_font(15.0)).color(Palette::INK));
            goods_rows(ui, tab.icons, state, defs, &prices, &mut draft.buy, &mut draft.pick_buy, false);
            let (sold, bought) = draft.estimate(&prices);
            ui.label(format!(
                "{} von {capacity} Einheiten · Verkauf bringt etwa {} Gold · Einkauf kostet etwa {} Gold",
                draft.units(),
                price(u32::try_from(sold).unwrap_or(u32::MAX)),
                price(u32::try_from(bought).unwrap_or(u32::MAX))
            ));
            let (danger, robber) = caravans::route_danger(state, defs, kingdom);
            let robber = robber.and_then(|r| state.diplomacy().kingdom(r)).map_or(String::new(), |r| format!(" (Gefahr durch {})", r.name()));
            ui.label(RichText::new(format!("Überfallrisiko {danger} % je Weg{robber}")).small().color(danger_color(danger)));
            let Command::SendCaravan { goods, gold, buy, .. } = draft.command(kingdom) else { return };
            let problem = caravans::send_problem(state, defs, kingdom, &goods, gold, &buy);
            if let Some(p) = &problem {
                ui.horizontal_top(|ui| {
                    mark(ui, Mark::Error);
                    ui.add(egui::Label::new(RichText::new(p).color(Palette::BAD)).wrap());
                });
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(problem.is_none(), egui::Button::new(RichText::new("Aussenden").strong())).clicked() {
                    command(actions, draft.command(kingdom));
                    close = true;
                }
                if ui.button("Abbrechen").clicked() {
                    close = true;
                }
            });
        });
        if close || modal.should_close() {
            self.dialog = None;
        }
    }
}

/// Editable rows of goods (`sell`: limited to the stock) and a picker to add a good.
#[allow(clippy::too_many_arguments)]
fn goods_rows(
    ui: &mut Ui,
    icons: &Icons,
    state: &GameState,
    defs: &Defs,
    prices: &[PartnerPriceRow],
    list: &mut Vec<ResourceAmount>,
    pick: &mut Option<ResourceId>,
    sell: bool,
) {
    let mut remove = None;
    for (i, g) in list.iter_mut().enumerate() {
        let Some(r) = defs.resources().get(g.resource.index()) else { continue };
        let stock = state.economy().available(g.resource);
        let p = prices.iter().find(|p| p.resource == g.resource);
        ui.horizontal(|ui| {
            if let Some(icon) = icons.get(&format!("res_{}", r.key)) {
                icon.show(ui, 18.0);
            }
            ui.label(&r.name);
            let max = if sell { stock.max(1) } else { 500 };
            ui.add(egui::DragValue::new(&mut g.amount).range(0..=max).speed(1.0));
            let note = match (sell, p) {
                (true, Some(p)) => format!("je {} Gold · Bestand {}", price(p.sell_centi), int(i64::from(stock))),
                (false, Some(p)) => format!("je {} Gold · bietet {}", price(p.buy_centi), int(i64::from(p.offered))),
                _ => String::new(),
            };
            ui.label(RichText::new(note).small().color(Palette::INK_SOFT));
            if ui.small_button("×").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        list.remove(i);
    }
    ui.horizontal(|ui| {
        let name = pick.and_then(|r| defs.resources().get(r.index())).map_or_else(|| "Ware wählen …".to_owned(), |r| r.name.clone());
        egui::ComboBox::from_id_salt(("caravan_pick", sell)).selected_text(name).width(200.0).height(300.0).show_ui(ui, |ui| {
            for r in defs.resources() {
                let p = prices.iter().find(|p| p.resource == r.id);
                let (show, note) = if sell {
                    let stock = state.economy().available(r.id);
                    (stock > 0, format!("{} ({}, sucht {})", r.name, int(i64::from(stock)), p.map_or(0, |p| p.wanted)))
                } else {
                    (p.is_some_and(|p| p.offered > 0), format!("{} (bietet {})", r.name, p.map_or(0, |p| p.offered)))
                };
                if show {
                    ui.selectable_value(pick, Some(r.id), note);
                }
            }
        });
        let addable = pick.filter(|r| list.iter().all(|g| g.resource != *r));
        if ui.add_enabled(addable.is_some(), egui::Button::new("Hinzufügen")).clicked()
            && let Some(resource) = addable
        {
            let amount = if sell { state.economy().available(resource).min(10) } else { 10 };
            list.push(ResourceAmount { resource, amount });
        }
    });
}

/// Short text of a caravan's trade road bonus: „Straße +8 %“, „ohne Handelsstraße“ or
/// „noch nicht gezählt“ before it walked on the map.
pub fn route_text(tiles: u32, bonus_percent: i32) -> (String, egui::Color32) {
    match (tiles, bonus_percent) {
        (0, _) => ("Weg noch nicht gezählt".to_owned(), Palette::INK_SOFT),
        (_, b) if b > 0 => (format!("Handelsstraße +{b} %"), Palette::GOOD),
        _ => ("ohne Handelsstraße".to_owned(), Palette::INK_SOFT),
    }
}

fn amount_text(n: u32) -> RichText {
    if n == 0 { RichText::new("–").color(Palette::INK_SOFT) } else { RichText::new(int(i64::from(n))).color(Palette::INK) }
}

fn danger_color(percent: u32) -> egui::Color32 {
    match percent {
        0..=4 => Palette::GOOD,
        5..=14 => Palette::WARN,
        _ => Palette::BAD,
    }
}

fn compute(state: &GameState, defs: &Defs) -> Data {
    let partners = state
        .diplomacy()
        .kingdoms()
        .map(|(id, k)| Partner {
            id,
            name: k.name().to_owned(),
            color: k.color(),
            access: has_access(state, defs, id),
            danger: caravans::route_danger(state, defs, id).0,
            travel_hours: caravans::travel_ticks(state, defs, id).div_ceil(TICKS_PER_HOUR),
            prices: report::partner_prices(state, defs, id),
        })
        .collect();
    let caravans = report::caravans_report(state, defs);
    let own = caravans.iter().filter(|c| c.kind == CaravanKind::Own).count();
    let routes = state.diplomacy().caravans().iter().map(|c| (c.id, c.route.tiles, c.route.bonus_percent())).collect();
    Data { partners, caravans, routes, own, capacity: defs.rules().diplomacy.caravans.capacity }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::hud::management::tests::test_defs;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    #[test]
    fn drafts_send_only_filled_rows_and_estimate_at_partner_prices() {
        let defs = test_defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let Some((id, _)) = state.diplomacy().kingdoms().next() else { panic!("a small map has kingdoms") };
        let prices = report::partner_prices(&state, &defs, id);
        let wood = defs.resource_id("wood").unwrap_or_else(|| panic!("wood"));
        let draft = CaravanDraft {
            goods: vec![ResourceAmount { resource: wood, amount: 10 }, ResourceAmount { resource: wood, amount: 0 }],
            gold: 50,
            buy: Vec::new(),
            ..CaravanDraft::default()
        };
        assert_eq!(draft.units(), 10);
        let Command::SendCaravan { goods, gold, buy, .. } = draft.command(id) else { panic!("a caravan command") };
        assert_eq!((goods.len(), gold, buy.len()), (1, 50, 0));
        let sell = prices.iter().find(|p| p.resource == wood).map_or(0, |p| p.sell_centi);
        assert_eq!(draft.estimate(&prices), (u64::from(sell) * 10, 0));
        // Without a trading post the simulation refuses, and the dialog shows why.
        assert!(caravans::send_problem(&state, &defs, id, &goods, gold, &buy).is_some());
        let data = compute(&state, &defs);
        assert_eq!(data.partners.len(), state.diplomacy().len());
        assert!(data.caravans.is_empty() && data.own == 0);
        assert!(data.partners.iter().all(|p| p.travel_hours > 0));
    }
}
