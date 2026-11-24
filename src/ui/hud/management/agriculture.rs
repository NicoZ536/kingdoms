//! „Landwirtschaft & Wetter“: today's weather with intensity, its effects, snow cover and
//! ground wetness, the season and the days left in it (and who forces it, if anybody); the food
//! forecast for the coming winter (`report::food_forecast`); farms, claimed fields, the average
//! condition of the sown fields (which the harvest events depend on), crops with growth and the
//! expected harvests (`report::agriculture_summary`, `report::farm_report`). A click on a farm
//! selects it.

use kingdoms_engine::egui::{self, Color32, Pos2, RichText, Stroke, Ui, Vec2};
use kingdoms_sim::agriculture::FarmKind;
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::defs::{Defs, EventCondition, Weather};
use kingdoms_sim::report::{self, AgricultureSummary, FoodForecast, WeatherReport};
use kingdoms_sim::time::Season;

use super::production::status_style;
use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, TabContext, card_row, hint, select};
use crate::ui::hud::HudAction;
use crate::ui::hud::format::{days_dative, int};
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{progress_bar, section_header};

/// One farm of the list.
#[derive(Clone, Debug)]
struct FarmRow {
    building: BuildingId,
    icon: String,
    name: String,
    kind: FarmKind,
    fields: usize,
    max_fields: u16,
    ripe: usize,
    crop: String,
    status_text: String,
    color: Color32,
    expected: String,
}

struct Data {
    weather: WeatherReport,
    summary: AgricultureSummary,
    forecast: FoodForecast,
    farms: Vec<FarmRow>,
}

/// State of the tab.
#[derive(Default)]
pub struct AgricultureTab {
    data: Cached<Data>,
}

/// Seasons as a German list („Frühling, Sommer“).
pub fn seasons_text(seasons: &[Season]) -> String {
    if seasons.is_empty() {
        return "–".to_owned();
    }
    seasons.iter().map(|s| s.display_name()).collect::<Vec<_>>().join(", ")
}

/// Resource amounts as „120 Weizen, 40 Gemüse“.
fn amounts_text(defs: &Defs, list: &[kingdoms_sim::defs::ResourceAmount]) -> String {
    if list.is_empty() {
        return "–".to_owned();
    }
    list.iter()
        .map(|a| format!("{} {}", int(i64::from(a.amount)), defs.resources().get(a.resource.index()).map_or("?", |r| r.name.as_str())))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A harvest event that depends on the condition of the fields: its title, the threshold in
/// percent and the season it may come in (game data `events.ron`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct HarvestEvent {
    title: String,
    percent: u8,
    season: Option<Season>,
}

/// The harvest events that depend on the condition of the fields: the one that needs weak
/// fields (at most `percent`) and the one that needs healthy fields (at least `percent`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct HarvestThresholds {
    weak: Option<HarvestEvent>,
    healthy: Option<HarvestEvent>,
}

impl HarvestThresholds {
    fn from_defs(defs: &Defs) -> Self {
        let mut out = Self::default();
        for event in defs.events() {
            let season = event.conditions.iter().find_map(|c| match c {
                EventCondition::Season(s) => Some(*s),
                _ => None,
            });
            let entry = |percent: u8| HarvestEvent { title: event.title.clone(), percent, season };
            for condition in &event.conditions {
                match *condition {
                    EventCondition::MaxFieldCondition(p) if out.weak.is_none() => out.weak = Some(entry(p)),
                    EventCondition::MinFieldCondition(p) if out.healthy.is_none() => out.healthy = Some(entry(p)),
                    _ => {}
                }
            }
        }
        out
    }

    /// Colour of a field condition: warning where the weak-fields event may come, good where
    /// the healthy-fields event may come.
    fn color(&self, condition: u8) -> Color32 {
        if self.weak.as_ref().is_some_and(|e| condition <= e.percent) {
            Palette::WARN
        } else if self.healthy.as_ref().is_some_and(|e| condition >= e.percent) {
            Palette::GOOD
        } else {
            Palette::INFO.gamma_multiply(0.7)
        }
    }

    /// Tooltip of the field condition: what it means and which harvest events it allows.
    fn tooltip(&self) -> String {
        let mut text = "Mittlerer Zustand der bestellten Felder: der Anteil der vollen Ernte, den Stürme, Dürre und Welken übrig gelassen haben.".to_owned();
        let when = |e: &HarvestEvent, bound: String| match e.season {
            Some(season) => format!("„{}“ im {} {bound}", e.title, season.display_name()),
            None => format!("„{}“ {bound}", e.title),
        };
        let mut events = Vec::new();
        if let Some(e) = &self.weak {
            events.push(when(e, format!("bei höchstens {} %", e.percent)));
        }
        if let Some(e) = &self.healthy {
            events.push(when(e, format!("ab {} %", e.percent)));
        }
        if !events.is_empty() {
            text.push_str("\nMögliche Ereignisse: ");
            text.push_str(&events.join(", "));
            text.push('.');
        }
        text
    }
}

/// One row „Zustand der Felder“ with a bar (or a dash without sown fields).
fn field_condition_row(ui: &mut Ui, defs: &Defs, condition: Option<u8>) {
    let thresholds = HarvestThresholds::from_defs(defs);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Zustand der Felder").color(Palette::INK_SOFT));
        let response = match condition {
            Some(c) => progress_bar(ui, f32::from(c) / 100.0, thresholds.color(c), &format!("{c} %")),
            None => ui.label(RichText::new("– (keine bestellten Felder)").color(Palette::INK_SOFT)),
        };
        response.on_hover_text(thresholds.tooltip());
    });
}

/// Food in saturation points, rounded („1.240“).
fn food(v: f32) -> String {
    int(v.round() as i64)
}

impl AgricultureTab {
    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let input = ctx.input;
        let defs = input.session.defs();
        let data = self.data.get(input, || {
            let state = input.session.state();
            let farms = state
                .agriculture()
                .farms()
                .filter_map(|(id, _)| {
                    let farm = report::farm_report(state, defs, id)?;
                    let def = defs.buildings().get(state.building(id)?.kind().index())?;
                    let crop = match farm.kind {
                        FarmKind::Pasture => "Weide".to_owned(),
                        FarmKind::Fields => farm.crops.iter().find(|c| c.selected).map_or_else(|| "–".to_owned(), |c| c.name.clone()),
                    };
                    let (color, _) = status_style(farm.status);
                    Some(FarmRow {
                        building: id,
                        icon: format!("bld_{}", def.key),
                        name: def.name.clone(),
                        kind: farm.kind,
                        fields: farm.fields.len(),
                        max_fields: farm.max_fields,
                        ripe: farm.fields.iter().filter(|f| f.ripe).count(),
                        crop,
                        status_text: farm.status_text.clone(),
                        color,
                        expected: amounts_text(defs, &farm.expected_harvest),
                    })
                })
                .collect();
            Data {
                weather: report::weather_report(state, defs),
                summary: report::agriculture_summary(state, defs),
                forecast: report::food_forecast(state, defs),
                farms,
            }
        });
        let a = &data.summary;
        ui.columns(2, |cols| {
            weather_card(&mut cols[0], ctx, &data.weather);
            cols[0].add_space(4.0);
            card_row(
                &mut cols[0],
                &[
                    ("Feldfarmen", a.field_farms.to_string(), Palette::INK),
                    ("Weidehöfe", a.pasture_farms.to_string(), Palette::INK),
                    ("Weideflächen", a.pasture_tiles.to_string(), Palette::INK),
                ],
            );
            card_row(
                &mut cols[0],
                &[
                    ("Felder bestellt", (a.fields - a.fallow.min(a.fields)).to_string(), Palette::GOOD),
                    ("Felder brach", a.fallow.to_string(), Palette::INK),
                    ("Ernte in Scheunen", amounts_text(defs, &a.in_barns), Palette::INK),
                ],
            );
            field_condition_row(&mut cols[0], defs, a.average_condition);
            forecast_card(&mut cols[1], &data.forecast);
        });
        section_header(ui, "Feldfrüchte");
        const C: [f32; 6] = [170.0, 70.0, 70.0, 170.0, 200.0, 300.0];
        table::header(
            ui,
            &[
                ("Frucht", C[0], CellAlign::Left),
                ("Felder", C[1], CellAlign::Right),
                ("Reif", C[2], CellAlign::Right),
                ("Erwartete Ernte", C[3], CellAlign::Right),
                ("Aussaat", C[4], CellAlign::Left),
                ("Wachstum", C[5], CellAlign::Left),
            ],
        );
        for (i, c) in a.crops.iter().enumerate() {
            let def = defs.crops().get(c.crop.index());
            let output = defs.resources().get(c.output.index());
            table::row(ui, ("crop", c.crop.index()), ROW_HEIGHT, i % 2 == 1, false, |ui| {
                table::cell(ui, C[0], CellAlign::Left, |ui| {
                    if let Some(icon) = output.and_then(|r| ctx.icons.get(&format!("res_{}", r.key))) {
                        icon.show(ui, 20.0);
                    }
                    ui.label(RichText::new(&c.name).color(Palette::INK));
                });
                table::text(ui, C[1], CellAlign::Right, c.fields.to_string());
                table::text(ui, C[2], CellAlign::Right, RichText::new(c.ripe.to_string()).color(if c.ripe > 0 { Palette::GOOD } else { Palette::INK_SOFT }));
                let unit = output.map_or("", |r| r.name.as_str());
                table::text(ui, C[3], CellAlign::Right, if c.expected > 0 { format!("{} {unit}", int(i64::from(c.expected))) } else { "–".to_owned() });
                table::cell(ui, C[4], CellAlign::Left, |ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new(def.map_or_else(String::new, |d| seasons_text(&d.sow_seasons))).small().color(Palette::INK_SOFT));
                });
                let growth = def.map_or_else(String::new, |d| {
                    if d.perennial {
                        format!("trägt nach {}, Ernte {}", days_dative(f64::from(d.growth_days)), seasons_text(&d.harvest_seasons))
                    } else {
                        format!("≈ {} bis zur Reife", crate::ui::hud::format::days_decimal(f64::from(d.growth_days)))
                    }
                });
                table::text(ui, C[5], CellAlign::Left, RichText::new(growth).small().color(Palette::INK_SOFT));
            });
        }
        ui.label(RichText::new("Die Wachstumszeit gilt bei normalem Wetter; Jahreszeit, Wetter, Boden und Feuchte beschleunigen oder bremsen sie.").small().color(Palette::INK_SOFT));
        section_header(ui, "Farmen");
        if data.farms.is_empty() {
            hint(ui, "Noch keine fertigen Farmen. Feldfarmen, Obstgärten und Weidehöfe stehen im Baumenü (B) unter „Landwirtschaft“.");
            return;
        }
        const F: [f32; 6] = [190.0, 100.0, 120.0, 300.0, 200.0, 60.0];
        table::header(
            ui,
            &[
                ("Betrieb", F[0], CellAlign::Left),
                ("Flächen", F[1], CellAlign::Right),
                ("Frucht", F[2], CellAlign::Left),
                ("Zustand", F[3], CellAlign::Left),
                ("Erwartet bis Winter", F[4], CellAlign::Left),
                ("Reif", F[5], CellAlign::Right),
            ],
        );
        for (i, f) in data.farms.iter().enumerate() {
            let response = table::row(ui, ("farm", f.building.index()), ROW_HEIGHT, i % 2 == 1, true, |ui| {
                table::cell(ui, F[0], CellAlign::Left, |ui| {
                    if let Some(icon) = ctx.icons.get(&f.icon) {
                        icon.show(ui, 20.0);
                    }
                    ui.add(egui::Label::new(RichText::new(&f.name).color(Palette::INK)).truncate());
                });
                let full = f.fields >= usize::from(f.max_fields);
                table::text(ui, F[1], CellAlign::Right, RichText::new(format!("{} / {}", f.fields, f.max_fields)).color(if full { Palette::INK } else { Palette::WARN }));
                table::cell(ui, F[2], CellAlign::Left, |ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new(&f.crop).color(Palette::INK));
                });
                table::text(ui, F[3], CellAlign::Left, RichText::new(&f.status_text).color(f.color));
                table::text(ui, F[4], CellAlign::Left, RichText::new(&f.expected).color(Palette::INK_SOFT));
                let ripe = if f.kind == FarmKind::Pasture { "–".to_owned() } else { f.ripe.to_string() };
                table::text(ui, F[5], CellAlign::Right, ripe);
            });
            if response.on_hover_text("Klick: Farm auswählen und hinschauen").clicked() {
                select(actions, f.building);
            }
        }
    }
}

fn weather_card(ui: &mut Ui, ctx: &TabContext<'_, '_>, w: &WeatherReport) {
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            weather_glyph(ui, w.weather, 44.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(&w.description).font(heading_font(19.0)).color(Palette::INK));
                let streak = if w.streak_days > 1 { format!(" · seit {} Tagen", w.streak_days) } else { String::new() };
                ui.label(RichText::new(format!("Stärke {} %{streak}", w.intensity)).color(Palette::INK_SOFT));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new(&w.temperature_text).font(heading_font(17.0)).color(temperature_color(w.temperature_c)))
                        .on_hover_text("Tagestemperatur aus Jahreszeit, Wetter und Klima des Startgebiets");
                    if w.heating_needed {
                        ui.label(RichText::new("· Häuser heizen").color(Palette::WARN))
                            .on_hover_text("Bei Kälte verbrauchen die Haushalte Brennholz oder Kohle; ohne Brennstoff frieren sie.");
                    }
                    if w.frost {
                        ui.label(RichText::new("· Frost").color(Palette::BAD)).on_hover_text("Unreife Saat, die keinen Frost verträgt, erfriert.");
                    }
                });
                if let Some(forced) = &w.forced_text {
                    ui.label(RichText::new(forced).small().color(Palette::WARN));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.horizontal(|ui| {
                        if let Some(icon) = ctx.icons.get(&format!("ui_season_{}", w.season.sprite_key())) {
                            icon.show(ui, 22.0);
                        }
                        ui.label(RichText::new(&w.season_name).font(heading_font(17.0)).color(Palette::INK));
                    });
                    let left = if w.days_left_in_season == 1 { "letzter Tag".to_owned() } else { format!("noch {} Tage", w.days_left_in_season) };
                    ui.label(RichText::new(left).small().color(Palette::INK_SOFT));
                });
            });
        });
        ui.add_space(4.0);
        ui.label(RichText::new(&w.date).small().color(Palette::INK_SOFT));
        egui::Grid::new("weather_bars").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new("Schneedecke").color(Palette::INK_SOFT));
            progress_bar(ui, f32::from(w.snow_cover) / 100.0, Color32::from_rgb(220, 232, 244), &format!("{} %", w.snow_cover));
            ui.end_row();
            ui.label(RichText::new("Bodennässe").color(Palette::INK_SOFT));
            progress_bar(ui, f32::from(w.ground_wetness) / 100.0, Palette::INFO.gamma_multiply(0.7), &format!("{} %", w.ground_wetness));
            ui.end_row();
            let weather = ctx.input.session.state().weather();
            ui.label(RichText::new("Wind").color(Palette::INK_SOFT));
            let strength = weather.wind_strength();
            progress_bar(
                ui,
                f32::from(strength) / 100.0,
                Palette::INK_SOFT.gamma_multiply(0.8),
                &crate::ui::hud::format::wind_text(weather.wind_direction(), strength),
            )
            .on_hover_text("Bäume, Felder, Rauch und Regen neigen sich mit dem Wind; starker Wind trägt Brände weiter in seine Richtung.");
            ui.end_row();
        });
        ui.add_space(2.0);
        ui.label(RichText::new("Wirkungen").small().strong().color(Palette::INK_SOFT));
        // The temperature stands in the header already.
        let temperature = format!("Temperatur: {}", w.temperature_text);
        for line in w.effects.iter().filter(|l| **l != temperature) {
            ui.label(RichText::new(format!("· {line}")).color(Palette::INK));
        }
    });
}

/// Label of the winter bar: food at winter start against the need of the winter, e.g. „Vorrat
/// 4.594 · Bedarf im Winter 1.728“, with the shortfall when the stock does not cover it
/// („… · 528 fehlen“); compared as shown (whole points).
fn winter_bar_text(at_winter_start: f32, need: f32) -> String {
    let (stock, need) = (at_winter_start.max(0.0).round() as i64, need.max(0.0).round() as i64);
    if need == 0 {
        return format!("Vorrat {} · kein Bedarf im Winter", int(stock));
    }
    let text = format!("Vorrat {} · Bedarf im Winter {}", int(stock), int(need));
    if stock >= need { text } else { format!("{text} · {} fehlen", int(need - stock)) }
}

fn forecast_card(ui: &mut Ui, f: &FoodForecast) {
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Nahrung bis zum Winterende").font(heading_font(17.0)).color(Palette::INK));
        let color = if f.covers_winter { Palette::GOOD } else { Palette::BAD };
        ui.add(egui::Label::new(RichText::new(&f.summary).strong().color(color)).wrap());
        // Food at winter start against the need of the winter.
        let need = f.winter_consumption_per_day * f.winter_days as f32;
        let fraction = if need > 0.0 { (f.at_winter_start / need).clamp(0.0, 1.0) } else { 1.0 };
        progress_bar(ui, fraction, color, &winter_bar_text(f.at_winter_start, need));
        ui.add_space(2.0);
        egui::Grid::new("food_forecast").num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
            let row = |ui: &mut Ui, label: &str, value: String| crate::ui::widgets::stat_row(ui, label, value);
            row(ui, "Bewohner", f.population.to_string());
            row(ui, "Verbrauch je Tag (jetzt / Winter)", format!("{} / {}", food(f.consumption_per_day), food(f.winter_consumption_per_day)));
            row(ui, "Im Reichslager", food(f.in_stock));
            row(ui, "In den Scheunen", food(f.in_barns));
            row(ui, "Erwartete Ernte", food(f.expected_harvest));
            row(ui, "Weitere Erzeugung je Tag", food(f.other_production_per_day));
            let until = if f.days_until_winter == 0 { "Winter läuft".to_owned() } else { crate::ui::hud::format::count(f.days_until_winter, "Tag", "Tage") };
            row(ui, "Bis zum Winter", until);
            row(ui, "Wintertage", f.winter_days.to_string());
            row(ui, "Vorrat bei Winterbeginn", food(f.at_winter_start));
            row(ui, "Nach dem Winter", food(f.after_winter));
            let stock_days = f.days_of_stock.map_or_else(|| "– (niemand isst)".to_owned(), |d| crate::ui::hud::format::days_decimal(f64::from(d)));
            row(ui, "Lager reicht ohne Erzeugung", stock_days);
        });
        ui.label(RichText::new("Mengen in Sättigungspunkten (Nährwert der Lebensmittel).").small().color(Palette::INK_SOFT));
    });
}

/// Colour of a temperature: blue at frost, ink when mild, warm when hot.
pub fn temperature_color(celsius: i8) -> Color32 {
    match celsius {
        ..=0 => Palette::INFO,
        1..=24 => Palette::INK,
        _ => Palette::WARN,
    }
}

/// A painted weather symbol (independent of the fonts' symbol coverage).
pub fn weather_glyph(ui: &mut Ui, weather: Weather, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let s = size / 44.0;
    let sun = |center: Pos2, r: f32, color: Color32| {
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            let dir = Vec2::new(a.cos(), a.sin());
            p.line_segment([center + dir * (r + 3.0 * s), center + dir * (r + 8.0 * s)], Stroke::new(2.0 * s, color));
        }
        p.circle_filled(center, r, color);
    };
    let cloud = |center: Pos2, fill: Color32| {
        p.circle_filled(center + Vec2::new(-8.0, 2.0) * s, 8.0 * s, fill);
        p.circle_filled(center + Vec2::new(2.0, -3.0) * s, 10.0 * s, fill);
        p.circle_filled(center + Vec2::new(11.0, 3.0) * s, 7.0 * s, fill);
        p.rect_filled(egui::Rect::from_min_max(center + Vec2::new(-8.0, 2.0) * s, center + Vec2::new(11.0, 10.0) * s), 0.0, fill);
    };
    match weather {
        Weather::Clear => sun(c, 10.0 * s, Palette::GOLD_BRIGHT),
        Weather::Drought => {
            sun(c + Vec2::new(0.0, -4.0) * s, 9.0 * s, Palette::WARN);
            let y = c.y + 15.0 * s;
            p.line_segment([Pos2::new(c.x - 16.0 * s, y), Pos2::new(c.x + 16.0 * s, y)], Stroke::new(2.5 * s, Palette::WOOD_LIGHT));
            p.line_segment([Pos2::new(c.x - 3.0 * s, y), Pos2::new(c.x + 1.0 * s, y + 5.0 * s)], Stroke::new(1.5 * s, Palette::WOOD_LIGHT));
        }
        Weather::Cloudy => {
            sun(c + Vec2::new(-8.0, -8.0) * s, 7.0 * s, Palette::GOLD_BRIGHT);
            cloud(c + Vec2::new(2.0, 2.0) * s, Color32::from_rgb(170, 176, 186));
        }
        Weather::Rain | Weather::Storm | Weather::Snow => {
            let fill = if weather == Weather::Storm { Color32::from_rgb(92, 98, 112) } else { Color32::from_rgb(150, 158, 172) };
            cloud(c + Vec2::new(0.0, -6.0) * s, fill);
            match weather {
                Weather::Rain => {
                    for dx in [-8.0, 0.0, 8.0] {
                        let top = c + Vec2::new(dx, 8.0) * s;
                        p.line_segment([top, top + Vec2::new(-3.0, 8.0) * s], Stroke::new(2.0 * s, Palette::INFO));
                    }
                }
                Weather::Storm => {
                    let pts = [c + Vec2::new(2.0, 4.0) * s, c + Vec2::new(-4.0, 12.0) * s, c + Vec2::new(2.0, 12.0) * s, c + Vec2::new(-3.0, 21.0) * s];
                    p.line_segment([pts[0], pts[1]], Stroke::new(2.5 * s, Palette::GOLD_BRIGHT));
                    p.line_segment([pts[1], pts[2]], Stroke::new(2.5 * s, Palette::GOLD_BRIGHT));
                    p.line_segment([pts[2], pts[3]], Stroke::new(2.5 * s, Palette::GOLD_BRIGHT));
                }
                _ => {
                    for (dx, dy) in [(-9.0, 10.0), (0.0, 14.0), (9.0, 10.0), (-4.0, 19.0), (5.0, 20.0)] {
                        p.circle_filled(c + Vec2::new(dx, dy) * s, 2.2 * s, Color32::WHITE);
                        p.circle_stroke(c + Vec2::new(dx, dy) * s, 2.2 * s, Stroke::new(0.8, Color32::from_rgb(150, 170, 200)));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::hud::management::tests::test_defs;

    #[test]
    fn season_and_amount_texts() {
        assert_eq!(seasons_text(&[Season::Spring, Season::Summer]), "Frühling, Sommer");
        assert_eq!(seasons_text(&[]), "–");
        let defs = test_defs();
        let wheat = defs.resource_id("wheat").expect("wheat");
        let list = [kingdoms_sim::defs::ResourceAmount { resource: wheat, amount: 1200 }];
        assert_eq!(amounts_text(&defs, &list), "1.200 Weizen");
        assert_eq!(amounts_text(&defs, &[]), "–");
        assert_eq!(food(12.6), "13");
    }

    #[test]
    fn field_condition_follows_the_harvest_events() {
        let defs = test_defs();
        let t = HarvestThresholds::from_defs(&defs);
        let event = |title: &str, percent| Some(HarvestEvent { title: title.to_owned(), percent, season: Some(Season::Autumn) });
        assert_eq!(t.weak, event("Schlechte Ernte", 85));
        assert_eq!(t.healthy, event("Besonders erfolgreiche Ernte", 95));
        assert_eq!(t.color(85), Palette::WARN);
        assert_eq!(t.color(95), Palette::GOOD);
        assert_eq!(t.color(90), Palette::INFO.gamma_multiply(0.7));
        assert!(
            t.tooltip().ends_with("Mögliche Ereignisse: „Schlechte Ernte“ im Herbst bei höchstens 85 %, „Besonders erfolgreiche Ernte“ im Herbst ab 95 %."),
            "{}",
            t.tooltip()
        );
        // Without such events the tooltip only explains the value.
        assert!(!HarvestThresholds::default().tooltip().contains("Ereignisse"));
    }

    #[test]
    fn winter_bar_reads_as_stock_and_need() {
        assert_eq!(winter_bar_text(4594.2, 1728.0), "Vorrat 4.594 · Bedarf im Winter 1.728");
        assert_eq!(winter_bar_text(1728.0, 1728.0), "Vorrat 1.728 · Bedarf im Winter 1.728");
        assert_eq!(winter_bar_text(1200.0, 1728.0), "Vorrat 1.200 · Bedarf im Winter 1.728 · 528 fehlen");
        assert_eq!(winter_bar_text(1727.6, 1728.0), "Vorrat 1.728 · Bedarf im Winter 1.728", "compared as shown");
        assert_eq!(winter_bar_text(-40.0, 300.0), "Vorrat 0 · Bedarf im Winter 300 · 300 fehlen");
        assert_eq!(winter_bar_text(80.0, 0.0), "Vorrat 80 · kein Bedarf im Winter");
    }
}
