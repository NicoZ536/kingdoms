//! Top bar (dark wood) in the order of the master design: population and housing, treasury
//! with the daily balance trend, food, wood, stone, iron (and further key goods where there is
//! room), happiness, season and date; on the right the buttons of the management window and the
//! menu. Speed and time of day are in the bottom bar.

use kingdoms_engine::egui::{self, Align, Color32, Layout, RichText};
use kingdoms_sim::defs::ResourceId;
use kingdoms_sim::economy::finance;
use kingdoms_sim::report;
use kingdoms_sim::time::Season;

use super::format::{decimal, int, signed_gold_milli};
use super::management::Tab;
use super::{HudAction, HudInput, Icons, TOP_BAR_HEIGHT};
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{compact_number, icon_button, resource_chip};

/// Seconds between refreshes of the bar's numbers.
const REFRESH_SECONDS: f64 = 0.25;
/// Resources of the master design, in order: wood, stone, iron.
const MAIN_RESOURCES: &[(&str, &str)] = &[("wood", "ui_wood"), ("stone", "ui_stone"), ("iron_bars", "ui_iron")];
/// Further goods shown while there is room.
const EXTRA_RESOURCES: &[&str] = &["planks", "tools", "bricks", "loam"];
/// Width a resource chip needs at most.
const CHIP_WIDTH: f32 = 86.0;
/// Width kept free for the happiness chip after the resources.
const HAPPINESS_WIDTH: f32 = 82.0;
/// Width kept free for the festival mark after the happiness chip.
const FESTIVAL_WIDTH: f32 = 78.0;

/// One resource chip.
#[derive(Clone, Debug)]
struct ResourceChip {
    key: String,
    icon: String,
    name: String,
    stock: u32,
    capacity: u32,
    committed: u32,
    produced_today: u32,
    consumed_today: u32,
    produced_per_day: u32,
    consumed_per_day: u32,
}

/// Food: all edible resources together.
#[derive(Clone, Debug, Default)]
struct FoodChip {
    total: u32,
    /// (name, stock) of every edible resource in stock.
    items: Vec<(String, u32)>,
    produced_per_day: u32,
    consumed_per_day: u32,
}

/// Numbers of the bar, refreshed a few times per second.
#[derive(Clone, Debug, Default)]
struct BarData {
    tick: u64,
    treasury: i64,
    bankrupt: bool,
    rates: finance::DailyRates,
    today_net_milli: i64,
    population: u32,
    capacity: u32,
    housed: u32,
    homeless: u32,
    /// Free places immigrants may take (every house keeps one for births and marriages).
    immigrant_places: u32,
    households: u32,
    working_age: u32,
    employed: u32,
    happiness: u8,
    unhappy: u32,
    factors: Vec<(String, f32)>,
    /// Wood, stone, iron.
    main: Vec<ResourceChip>,
    /// Further goods, shown while there is room.
    extra: Vec<ResourceChip>,
    food: FoodChip,
    /// Iron ore in stock (part of the iron tooltip).
    iron_ore: Option<(String, u32)>,
    /// Active festival days („Festtag (Religiöses Fest), noch 1 Tag“).
    festival: Option<String>,
}

#[derive(Default)]
pub struct TopBar {
    data: Option<BarData>,
    refreshed_at: f64,
}

impl TopBar {
    fn refresh(&mut self, input: &HudInput<'_>) {
        let tick = input.session.state().clock().tick;
        let stale = self.data.as_ref().is_none_or(|d| d.tick != tick && input.now - self.refreshed_at >= REFRESH_SECONDS);
        if !stale && self.data.is_some() {
            return;
        }
        self.refreshed_at = input.now;
        let state = input.session.state();
        let defs = input.session.defs();
        let economy = report::economy_summary(state, defs);
        let population = report::population_summary(state, defs);
        let happiness = report::happiness_report(state, defs);
        let row = |id: ResourceId| economy.resources.iter().find(|r| r.resource == id);
        let chip = |key: &str, icon: String| {
            let def = defs.resource_by_key(key)?;
            let r = row(def.id)?;
            Some(ResourceChip {
                key: def.key.clone(),
                icon,
                name: def.name.clone(),
                stock: r.stock,
                capacity: r.capacity,
                committed: r.committed,
                produced_today: r.produced_today,
                consumed_today: r.consumed_today,
                produced_per_day: r.produced_per_day,
                consumed_per_day: r.consumed_per_day,
            })
        };
        let main = MAIN_RESOURCES.iter().filter_map(|(key, icon)| chip(key, (*icon).to_owned())).collect();
        let extra = EXTRA_RESOURCES.iter().filter_map(|key| chip(key, format!("res_{key}"))).collect();
        let iron_ore = defs.resource_by_key("iron_ore").and_then(|d| row(d.id).map(|r| (d.name.clone(), r.stock)));
        let mut food = FoodChip::default();
        for def in defs.resources().iter().filter(|r| r.is_edible()) {
            let Some(r) = row(def.id) else { continue };
            food.total += r.stock;
            food.produced_per_day += r.produced_per_day;
            food.consumed_per_day += r.consumed_per_day;
            if r.stock > 0 {
                food.items.push((def.name.clone(), r.stock));
            }
        }
        let festival = report::events_report(state, defs)
            .modifiers
            .into_iter()
            .find(|m| m.kind == kingdoms_sim::events::ModifierKind::Festival)
            .map(|m| m.text);
        let mut factors: Vec<(String, f32)> = happiness.factors.iter().map(|f| (f.label.clone(), f.points)).collect();
        factors.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        self.data = Some(BarData {
            tick,
            treasury: economy.treasury,
            bankrupt: economy.bankrupt,
            rates: finance::daily_rates(state, defs),
            today_net_milli: state.economy().ledger().today().net_milli(),
            population: population.population,
            capacity: population.housing_capacity,
            housed: population.housed,
            homeless: population.homeless,
            immigrant_places: population.immigrant_places,
            households: population.households,
            working_age: population.working_age,
            employed: population.employed,
            happiness: happiness.average,
            unhappy: happiness.unhappy,
            factors,
            main,
            extra,
            food,
            iron_ore,
            festival,
        });
    }

    pub fn show(&mut self, ctx: &egui::Context, input: &HudInput<'_>, icons: &Icons, menu_open: bool, window: Option<Tab>, actions: &mut Vec<HudAction>) {
        self.refresh(input);
        let Some(data) = self.data.as_ref() else { return };
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("hud_top_bar"))
            .fixed_pos(screen.min)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::wood_frame()
                    .corner_radius(egui::CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 })
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.set_width(screen.width() - 23.0);
                        ui.set_height(TOP_BAR_HEIGHT - 10.0);
                        theme::on_wood(ui);
                        ui.horizontal_centered(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                menu_button(ui, icons, menu_open, actions);
                                ui.add_space(6.0);
                                window_buttons(ui, input, icons, window, actions);
                                separator(ui);
                                date(ui, input, icons);
                                separator(ui);
                                // The rest from the left: the kingdom's numbers in the order of
                                // the master design, further goods while there is room.
                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    kingdom_chips(ui, data, icons);
                                    resources(ui, data, icons);
                                    happiness(ui, data, icons);
                                });
                            });
                        });
                    });
            });
    }
}

fn separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 24.0), egui::Sense::hover());
    ui.painter().line_segment([rect.center_top(), rect.center_bottom()], egui::Stroke::new(1.0, Palette::GOLD_DARK));
}

/// Population and treasury.
fn kingdom_chips(ui: &mut egui::Ui, data: &BarData, icons: &Icons) {
    let pop_text = format!("{} / {}", data.population, data.capacity);
    resource_chip(ui, icons.get("ui_population"), &pop_text, None).on_hover_ui(|ui| {
        tooltip_title(ui, "Bevölkerung");
        egui::Grid::new("tt_pop").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
            row(ui, "Bewohner", data.population.to_string());
            row(ui, "Haushalte", data.households.to_string());
            row(ui, "Wohnraum", format!("{}, {} bewohnt", super::format::count(data.capacity, "Platz", "Plätze"), data.housed));
            row(ui, "Frei für Zuwanderer", format!("{} (je Haus bleibt ein Platz für Geburten und Hochzeiten)", data.immigrant_places));
            row(ui, "Obdachlos", data.homeless.to_string());
            row(ui, "Erwerbsfähig", data.working_age.to_string());
            row(ui, "Beschäftigt", data.employed.to_string());
            row(ui, "Arbeitslos", data.working_age.saturating_sub(data.employed).to_string());
        });
    });
    let net = data.rates.net();
    let trend = if net == 0 { None } else { Some(net as f32) };
    resource_chip(ui, icons.get("ui_gold"), &compact_number(data.treasury as f64), trend).on_hover_ui(|ui| {
        tooltip_title(ui, "Schatzkammer");
        egui::Grid::new("tt_gold").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
            row(ui, "Bestand", format!("{} Gold", int(data.treasury)));
            row(ui, "Bilanz heute", format!("{} Gold", signed_gold_milli(data.today_net_milli)));
            row(ui, "Laufend pro Tag", format!("{} Gold", signed_gold_milli(net)));
            row(ui, "  Steuern", signed_gold_milli(data.rates.taxes));
            row(ui, "  Marktgebühren", signed_gold_milli(data.rates.market_fees));
            row(ui, "  Löhne", signed_gold_milli(data.rates.wages));
            row(ui, "  Unterhalt", signed_gold_milli(data.rates.upkeep));
        });
        if data.bankrupt {
            ui.colored_label(Palette::BAD, "Bankrott: keine neuen Bauten, Produktion verlangsamt.");
        }
    });
    separator(ui);
}

/// Happiness with a mood dot and the factors in the tooltip.
fn happiness(ui: &mut egui::Ui, data: &BarData, icons: &Icons) {
    separator(ui);
    let happy_text = format!("{} %", data.happiness);
    let response = resource_chip(ui, icons.get("ui_happiness"), &happy_text, None);
    // Colour dot for the mood.
    let color = match data.happiness {
        66.. => Palette::GOOD_LIGHT,
        33..=65 => Palette::GOLD_BRIGHT,
        _ => Palette::BAD_LIGHT,
    };
    ui.painter().circle_filled(response.rect.right_center() - egui::vec2(1.0, 0.0), 3.0, color);
    if let Some(festival) = data.festival.as_ref() {
        ui.add_space(4.0);
        let text = RichText::new("Festtag").font(heading_font(15.0)).color(Palette::GOLD_BRIGHT);
        ui.label(text).on_hover_ui(|ui| {
            tooltip_title(ui, "Festtag");
            ui.label(festival);
            ui.label("Die Bewohner feiern nachmittags bei der Kirche oder auf dem Markt statt zu arbeiten.");
        });
    }
    response.on_hover_ui(|ui| {
        tooltip_title(ui, "Zufriedenheit");
        ui.label(format!("Durchschnitt {} von 100 · {} unzufrieden", data.happiness, data.unhappy));
        egui::Grid::new("tt_happy").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
            for (label, points) in data.factors.iter().take(10) {
                let color = if *points >= 0.0 { Palette::GOOD } else { Palette::BAD };
                ui.label(label);
                ui.colored_label(color, format!("{}{}", if *points >= 0.0 { "+" } else { "" }, decimal(f64::from(*points), 1)));
                ui.end_row();
            }
        });
    });
}

/// Food, wood, stone and iron, then further goods while there is room (the happiness chip
/// keeps its place).
fn resources(ui: &mut egui::Ui, data: &BarData, icons: &Icons) {
    let food_trend = trend(data.food.produced_per_day, data.food.consumed_per_day);
    resource_chip(ui, icons.get("ui_food"), &compact_number(f64::from(data.food.total)), food_trend).on_hover_ui(|ui| {
        tooltip_title(ui, "Nahrung");
        egui::Grid::new("tt_food").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
            for (name, stock) in &data.food.items {
                row(ui, name, int(i64::from(*stock)));
            }
            row(ui, "Gestern erzeugt", int(i64::from(data.food.produced_per_day)));
            row(ui, "Gestern verbraucht", int(i64::from(data.food.consumed_per_day)));
        });
        if data.food.items.is_empty() {
            ui.colored_label(Palette::BAD, "Keine Nahrung im Lager!");
        }
    });
    for chip in &data.main {
        let ore = if chip.key == "iron_bars" { data.iron_ore.as_ref() } else { None };
        resource(ui, chip, icons, ore);
    }
    let reserve = HAPPINESS_WIDTH + if data.festival.is_some() { FESTIVAL_WIDTH } else { 0.0 };
    for chip in &data.extra {
        if ui.available_width() < CHIP_WIDTH + reserve {
            break;
        }
        resource(ui, chip, icons, None);
    }
}

/// One resource chip with stock, flows and (for iron) the ore in stock in its tooltip.
fn resource(ui: &mut egui::Ui, chip: &ResourceChip, icons: &Icons, ore: Option<&(String, u32)>) {
    let t = trend(chip.produced_per_day, chip.consumed_per_day);
    resource_chip(ui, icons.get(&chip.icon), &compact_number(f64::from(chip.stock)), t).on_hover_ui(|ui| {
        tooltip_title(ui, &chip.name);
        egui::Grid::new(("tt_res", chip.key.as_str())).num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
            row(ui, "Bestand", format!("{} / {}", int(i64::from(chip.stock)), int(i64::from(chip.capacity))));
            if chip.committed > 0 {
                row(ui, "Für Baustellen gebunden", int(i64::from(chip.committed)));
            }
            row(ui, "Heute erzeugt", int(i64::from(chip.produced_today)));
            row(ui, "Heute verbraucht", int(i64::from(chip.consumed_today)));
            row(ui, "Gestern erzeugt", int(i64::from(chip.produced_per_day)));
            row(ui, "Gestern verbraucht", int(i64::from(chip.consumed_per_day)));
            if let Some((name, stock)) = ore {
                row(ui, name, int(i64::from(*stock)));
            }
        });
    });
}

fn trend(produced: u32, consumed: u32) -> Option<f32> {
    (produced != consumed).then_some(produced as f32 - consumed as f32)
}

/// Season (icon and name), today's temperature and the date; the time of day is in the
/// bottom bar. The tooltip names the weather, heating and frost.
fn date(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons) {
    let (state, defs) = (input.session.state(), input.session.defs());
    let clock = state.clock();
    let season = clock.season();
    let weather = state.weather();
    let rules = defs.rules();
    let celsius = weather.temperature_c(rules, clock);
    let tip = season_tooltip(
        &format!("{} · Tag {} des Jahres", season.display_name(), clock.day_of_year() + 1),
        &weather.description(),
        &weather.temperature_text(rules, clock),
        weather.needs_heating(rules, clock),
        weather.is_frost(rules, clock),
    );
    // Right-to-left: date, temperature, then the season.
    let response = ui.label(
        RichText::new(format!("Tag {} · Jahr {}", clock.day_of_season(), clock.year())).font(heading_font(15.0)).color(Palette::ON_WOOD),
    );
    let color = if celsius <= 0 { Palette::FROST } else { Palette::ON_WOOD };
    let temperature = ui.label(RichText::new(weather.temperature_text(rules, clock)).font(heading_font(15.0)).color(color));
    ui.label(RichText::new(season.display_name()).font(heading_font(15.0)).color(Palette::GOLD_BRIGHT)).on_hover_text(&tip);
    if let Some(icon) = icons.get(season_icon(season)) {
        icon.show(ui, 24.0).on_hover_text(&tip);
    }
    temperature.on_hover_text(&tip);
    response.on_hover_text(tip);
}

/// Tooltip of the season and date: the day of the year, today's weather and temperature,
/// whether households heat and whether there is frost.
pub fn season_tooltip(day: &str, weather: &str, temperature: &str, heating: bool, frost: bool) -> String {
    let mut text = format!("{day}\nWetter: {weather} · {temperature}");
    if heating {
        text.push_str("\nKalt: Die Haushalte heizen mit Brennholz oder Kohle.");
    }
    if frost {
        text.push_str("\nFrost: Unreife Saat, die keinen Frost verträgt, erfriert.");
    }
    text
}

fn season_icon(season: Season) -> &'static str {
    match season {
        Season::Spring => "ui_season_spring",
        Season::Summer => "ui_season_summer",
        Season::Autumn => "ui_season_autumn",
        Season::Winter => "ui_season_winter",
    }
}

/// The tab buttons of the management window (laid out right to left, so in reverse order).
fn window_buttons(ui: &mut egui::Ui, input: &HudInput<'_>, icons: &Icons, window: Option<Tab>, actions: &mut Vec<HudAction>) {
    ui.spacing_mut().item_spacing.x = 2.0;
    for tab in Tab::ALL.iter().rev() {
        let selected = window == Some(*tab);
        let hint = input.hint(tab.action());
        let text = if selected { format!("{} schließen ({hint})", tab.title()) } else { format!("{} ({hint})", tab.title()) };
        let initial: String = tab.label().chars().take(1).collect();
        if icon_button(ui, icons.get(tab.icon()), &initial, selected, 28.0).on_hover_text(text).clicked() {
            actions.push(HudAction::ToggleWindow(*tab));
        }
    }
    ui.spacing_mut().item_spacing.x = 4.0;
}

fn menu_button(ui: &mut egui::Ui, icons: &Icons, menu_open: bool, actions: &mut Vec<HudAction>) {
    if icon_button(ui, icons.get("ui_settings"), "≡", menu_open, 30.0).on_hover_text("Menü (Esc)").clicked() {
        actions.push(HudAction::Menu(!menu_open));
        actions.push(HudAction::Sound(if menu_open { "ui_close" } else { "ui_open" }));
    }
}

pub(super) fn tooltip_title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(heading_font(16.0)).color(Palette::INK));
}

pub(super) use crate::ui::widgets::stat_row as row;

/// Colour for affordability marks.
pub(super) fn ok_color(ok: bool) -> Color32 {
    if ok { Palette::GOOD } else { Palette::BAD }
}
