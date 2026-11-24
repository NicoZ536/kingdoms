//! Farm section of the info panel for field farms, orchards and pastures
//! (`report::farm_report`): status with an explanation, crop choice (`Command::SetCrop`, only
//! the crops the simulation offers for the farm), claimed fields with crop and growth stage,
//! expected harvest, soil fertility, exhaustion and fallow fields, the running work, the barn
//! and the last harvest.

use kingdoms_engine::egui::{self, RichText, Ui};
use kingdoms_sim::agriculture::FarmKind;
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{CropDef, Defs, ResourceAmount};
use kingdoms_sim::production::ProductionStatus;
use kingdoms_sim::report::{FarmReport, FieldRow};
use kingdoms_sim::time::Season;

use super::format::{decimal, int};
use super::management::{day_label, seasons_text};
use super::top_bar::row;
use super::{HudAction, Icons};
use crate::ui::theme::{self, Palette};
use crate::ui::widgets::{Mark, marked, mini_bar, progress_bar, rating_color, section_header};

/// Icon name of a claimed tile's look (`field_<sprite>` of the `fields` atlas).
pub fn field_icon(defs: &Defs, kind: FarmKind, field: &FieldRow) -> String {
    match (kind, field.crop.and_then(|c| defs.crops().get(c.index()))) {
        (FarmKind::Pasture, _) => "field_pasture".to_owned(),
        (FarmKind::Fields, None) => "field_soil".to_owned(),
        (FarmKind::Fields, Some(crop)) => format!("field_{}_{}", crop.sprite, field.stage.min(4)),
    }
}

/// German plural of the claimed tiles.
pub fn tiles_label(kind: FarmKind) -> &'static str {
    match kind {
        FarmKind::Fields => "Felder",
        FarmKind::Pasture => "Weideflächen",
    }
}

/// Averages over the claimed tiles.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FieldStats {
    /// Tiles with a crop.
    pub sown: usize,
    /// Tiles without a crop (field farms only).
    pub fallow: usize,
    pub ripe: usize,
    /// Mean growth of the sown tiles in percent.
    pub growth: f32,
    pub fertility: f32,
    pub natural_fertility: f32,
    /// Mean moisture 0–255.
    pub moisture: f32,
    /// Mean condition of the sown tiles in percent.
    pub condition: f32,
}

impl FieldStats {
    pub fn of(report: &FarmReport) -> Self {
        let n = report.fields.len();
        if n == 0 {
            return Self::default();
        }
        let sown: Vec<&FieldRow> = report.fields.iter().filter(|f| f.crop.is_some()).collect();
        let mean = |values: &mut dyn Iterator<Item = f32>, count: usize| if count == 0 { 0.0 } else { values.sum::<f32>() / count as f32 };
        Self {
            sown: sown.len(),
            fallow: if report.kind == FarmKind::Fields { n - sown.len() } else { 0 },
            ripe: report.fields.iter().filter(|f| f.ripe).count(),
            growth: mean(&mut sown.iter().map(|f| f32::from(f.growth_percent)), sown.len()),
            fertility: mean(&mut report.fields.iter().map(|f| f32::from(f.fertility)), n),
            natural_fertility: mean(&mut report.fields.iter().map(|f| f32::from(f.natural_fertility)), n),
            moisture: mean(&mut report.fields.iter().map(|f| f32::from(f.moisture)), n),
            condition: mean(&mut sown.iter().map(|f| f32::from(f.condition)), sown.len()),
        }
    }

    /// Fertility the harvests took from the soil (mean, ≥ 0).
    pub fn exhaustion(&self) -> f32 {
        (self.natural_fertility - self.fertility).max(0.0)
    }
}

/// German explanation of a farm status that is not an error (`None`: nothing to explain).
pub fn status_explanation(report: &FarmReport, crop: Option<&CropDef>, season: Season, min_fertility: u8) -> Option<String> {
    match (report.status, report.kind) {
        (ProductionStatus::Growing, _) => Some("Alle bestellten Felder wachsen – die Arbeiter warten, bis die Frucht reif ist.".to_owned()),
        (ProductionStatus::OutOfSeason, FarmKind::Fields) => {
            let crop = crop?;
            let text = if crop.perennial {
                format!(
                    "Die {} werden im {} gepflanzt und im {} geerntet; bis dahin ruht die Arbeit.",
                    crop.name,
                    seasons_text(&crop.sow_seasons),
                    seasons_text(&crop.harvest_seasons)
                )
            } else if season == Season::Winter {
                format!("Im Winter wächst nichts. {} wird im {} gesät.", crop.name, seasons_text(&crop.sow_seasons))
            } else {
                let frost = if crop.frost_kills { " Gesät wird nur, wenn die Frucht vor dem Winter reif werden kann." } else { "" };
                format!("{} wird im {} gesät; bis dahin ruhen die Felder.{frost}", crop.name, seasons_text(&crop.sow_seasons))
            };
            Some(text)
        }
        (ProductionStatus::OutOfSeason, FarmKind::Pasture) => Some("Die Weide ruht bis zur nächsten Jahreszeit.".to_owned()),
        (ProductionStatus::NoResourcesInRange, FarmKind::Fields) => {
            Some(format!("Im Umkreis gibt es kein freies Ackerland (Boden mit Fruchtbarkeit ab {min_fertility}, nicht bebaut, kein Weg)."))
        }
        (ProductionStatus::NoResourcesInRange, FarmKind::Pasture) => Some("Im Umkreis gibt es keine freie Weidefläche.".to_owned()),
        _ => None,
    }
}

/// „120 Weizen, 40 Gerste“ or „–“.
fn amounts(defs: &Defs, list: &[ResourceAmount]) -> String {
    if list.is_empty() {
        return "–".to_owned();
    }
    list.iter()
        .map(|a| format!("{} {}", int(i64::from(a.amount)), defs.resources().get(a.resource.index()).map_or("?", |r| r.name.as_str())))
        .collect::<Vec<_>>()
        .join(", ")
}

fn resource_name(defs: &Defs, resource: kingdoms_sim::defs::ResourceId) -> &str {
    defs.resources().get(resource.index()).map_or("?", |r| r.name.as_str())
}

/// The farm section (fields or pasture) of a finished farm.
#[allow(clippy::too_many_arguments)]
pub fn farm_section(
    ui: &mut Ui,
    defs: &Defs,
    icons: &Icons,
    season: Season,
    id: BuildingId,
    report: &FarmReport,
    width: f32,
    actions: &mut Vec<HudAction>,
) {
    let stats = FieldStats::of(report);
    let selected = report.crops.iter().find(|c| c.selected).and_then(|c| defs.crops().get(c.crop.index()));
    section_header(ui, if report.kind == FarmKind::Pasture { "Weide" } else { "Felder" });
    if let Some(text) = status_explanation(report, selected, season, defs.rules().agriculture.min_field_fertility) {
        let kind = if matches!(report.status, ProductionStatus::NoResourcesInRange) { Mark::Warning } else { Mark::Next };
        marked(ui, kind, RichText::new(text).color(Palette::INK));
    }
    // Crop choice.
    if report.kind == FarmKind::Fields {
        if report.crops.len() > 1 {
            ui.label(RichText::new("Frucht für die nächste Aussaat").small().color(Palette::INK_SOFT));
            ui.horizontal_wrapped(|ui| {
                for c in &report.crops {
                    let def = defs.crops().get(c.crop.index());
                    let response = crop_button(ui, icons, def, &c.name, c.selected);
                    let response = response.on_hover_text(match def {
                        Some(d) => format!(
                            "{}: Ertrag {} {} je Feld, Aussaat {}, ≈ {} bis zur Reife",
                            d.name,
                            decimal(f64::from(d.yield_per_tile), 0),
                            resource_name(defs, d.output),
                            seasons_text(&d.sow_seasons),
                            super::format::days_decimal(f64::from(d.growth_days))
                        ),
                        None => c.name.clone(),
                    });
                    if response.clicked() && !c.selected {
                        actions.push(HudAction::Command(Command::SetCrop { building: id, crop: c.crop }));
                        actions.push(HudAction::Sound("ui_click"));
                    }
                }
            });
            ui.label(RichText::new("Schon gesäte Felder behalten ihre Frucht bis zur Ernte.").small().color(Palette::INK_SOFT));
        } else if let Some(c) = report.crops.first() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Frucht").color(Palette::INK_SOFT));
                ui.label(RichText::new(&c.name).strong());
            });
        }
    }
    egui::Grid::new(("farm", id)).num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
        let claimed = report.fields.len();
        let color = if claimed >= usize::from(report.max_fields) { Palette::INK } else { Palette::WARN };
        row(ui, tiles_label(report.kind), RichText::new(format!("{claimed} von {}", report.max_fields)).color(color));
        match report.kind {
            FarmKind::Fields => {
                row(ui, "Bestellt · reif · brach", format!("{} · {} · {}", stats.sown, stats.ripe, stats.fallow));
                if stats.sown > 0 {
                    row(ui, "Wachstum (Mittel)", format!("{} %", stats.growth.round()));
                }
                row(ui, "Erwartete Ernte", amounts(defs, &report.expected_harvest));
                if let Some(crop) = selected {
                    let days = f64::from(crop.growth_days);
                    if crop.perennial {
                        row(ui, "Bäume tragen nach", format!("≈ {}", super::format::days_dative(days)));
                        row(ui, "Erntezeit", seasons_text(&crop.harvest_seasons));
                    } else {
                        row(ui, "Reifezeit", format!("≈ {}", super::format::days_decimal(days)));
                    }
                }
            }
            FarmKind::Pasture => {
                row(ui, "Weidefläche", format!("{} %", report.pasture_percent));
            }
        }
        if claimed > 0 {
            let soil = format!("{} von {}", stats.fertility.round(), stats.natural_fertility.round());
            row(ui, "Fruchtbarkeit", soil);
            if report.kind == FarmKind::Fields {
                let exhaustion = stats.exhaustion();
                let text = if exhaustion < 0.5 { "keine".to_owned() } else { format!("−{}", exhaustion.round()) };
                row(ui, "Erschöpfung", RichText::new(text).color(if exhaustion >= 20.0 { Palette::WARN } else { Palette::INK }));
            }
            row(ui, "Bodenfeuchte", format!("{} %", (stats.moisture * 100.0 / 255.0).round()));
            if stats.sown > 0 && report.kind == FarmKind::Fields {
                row(ui, "Zustand", RichText::new(format!("{} %", stats.condition.round())).color(rating_color(stats.condition)));
            }
        }
        if !report.in_barn.is_empty() {
            row(ui, "In der Scheune", amounts(defs, &report.in_barn));
        }
        if let Some(h) = report.harvest_in_progress {
            row(ui, "Laufende Ernte", format!("bisher {} {}", int(i64::from(h.amount)), resource_name(defs, h.resource)));
        }
        if let Some(h) = report.last_harvest {
            row(ui, "Letzte Ernte", format!("{} {} ({})", int(i64::from(h.amount)), resource_name(defs, h.resource), day_label(h.day)));
        }
    });
    if report.kind == FarmKind::Fields && !report.fields.is_empty() {
        ui.label(
            RichText::new("Fruchtbarkeit: Mittel der Felder von ihrem natürlichen Wert; Reifezeit bei normalem Wetter, die erwartete Ernte gilt bis zum Winter.")
                .small()
                .color(Palette::INK_SOFT),
        );
    }
    if let Some(task) = report.task_text.as_ref() {
        progress_bar(ui, report.task_progress, Palette::GOOD, task);
    }
    if report.fields.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(RichText::new(format!("{} einzeln ({})", tiles_label(report.kind), report.fields.len())).color(Palette::INK))
        .id_salt(("farm_fields", id))
        .default_open(false)
        .show(ui, |ui| {
            theme::inset_frame().show(ui, |ui| {
                ui.set_width(width);
                egui::Grid::new(("farm_field_rows", id)).num_columns(5).spacing([6.0, 2.0]).striped(true).show(ui, |ui| {
                    ui.label(RichText::new("").small());
                    ui.label(RichText::new("Zustand").small().color(Palette::INK_SOFT));
                    ui.label(RichText::new("Wachstum").small().color(Palette::INK_SOFT));
                    ui.label(RichText::new("Boden").small().color(Palette::INK_SOFT));
                    ui.label(RichText::new("Ertrag").small().color(Palette::INK_SOFT));
                    ui.end_row();
                    for f in &report.fields {
                        match icons.get(&field_icon(defs, report.kind, f)) {
                            Some(icon) => {
                                icon.show(ui, 24.0);
                            }
                            None => {
                                ui.label("");
                            }
                        }
                        ui.label(RichText::new(&f.state_text).small().color(if f.ripe { Palette::GOOD } else { Palette::INK }));
                        if f.crop.is_some() {
                            mini_bar(ui, f32::from(f.growth_percent) / 100.0, Palette::GOOD, 46.0).on_hover_text(format!("Wachstum {} %", f.growth_percent));
                        } else {
                            ui.label(RichText::new("–").small().color(Palette::INK_SOFT));
                        }
                        ui.label(RichText::new(format!("{}/{}", f.fertility, f.natural_fertility)).small().color(Palette::INK_SOFT))
                            .on_hover_text(format!(
                                "Fruchtbarkeit {} (natürlich {}), Feuchte {} %, Zustand {} %",
                                f.fertility,
                                f.natural_fertility,
                                u32::from(f.moisture) * 100 / 255,
                                f.condition
                            ));
                        let yield_text = if f.expected_yield > 0 { int(i64::from(f.expected_yield)) } else { "–".to_owned() };
                        ui.label(RichText::new(yield_text).small());
                        ui.end_row();
                    }
                });
            });
        });
}

/// A crop button: ripe-stage picture and name; gold frame when selected.
fn crop_button(ui: &mut Ui, icons: &Icons, crop: Option<&CropDef>, name: &str, selected: bool) -> egui::Response {
    let icon = crop.and_then(|c| icons.get(&format!("field_{}_4", c.sprite)));
    ui.scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
            let mut button = egui::Button::new(RichText::new(name).strong());
            if let Some(icon) = icon {
                button = egui::Button::image_and_text(
                    egui::Image::new(egui::load::SizedTexture::new(icon.texture, egui::vec2(30.0, 30.0 / icon.aspect.max(0.1)))).uv(icon.uv),
                    RichText::new(name).strong(),
                );
            }
            ui.add(button.selected(selected))
        })
        .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_sim::buildings::Rotation;
    use kingdoms_sim::command::Command;
    use kingdoms_sim::report;
    use kingdoms_sim::state::{GameSettings, GameState};
    use kingdoms_sim::world::WorldSize;

    fn defs() -> Defs {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => defs,
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    #[test]
    fn explanations_name_the_sowing_season() {
        let defs = defs();
        let wheat = defs.crop_by_key("wheat").expect("wheat");
        let fruit = defs.crop_by_key("fruit").expect("fruit");
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let building = state.buildings().iter().next().map(|(id, _)| id).expect("start building");
        let mut r = FarmReport {
            building,
            kind: FarmKind::Fields,
            tile_name: "Feld".into(),
            status: ProductionStatus::OutOfSeason,
            status_text: String::new(),
            crops: Vec::new(),
            fields: Vec::new(),
            max_fields: 24,
            task_text: None,
            task_progress: 0.0,
            expected_harvest: Vec::new(),
            harvest_in_progress: None,
            last_harvest: None,
            in_barn: Vec::new(),
            pasture_percent: 0,
        };
        let summer = status_explanation(&r, Some(wheat), Season::Summer, 60).expect("text");
        assert!(summer.starts_with("Weizen wird im Frühling gesät"), "{summer}");
        assert!(summer.contains("vor dem Winter"));
        assert!(status_explanation(&r, Some(wheat), Season::Winter, 60).expect("text").starts_with("Im Winter wächst nichts"));
        assert!(status_explanation(&r, Some(fruit), Season::Summer, 60).expect("text").contains("im Herbst geerntet"));
        r.status = ProductionStatus::NoResourcesInRange;
        assert!(status_explanation(&r, Some(wheat), Season::Spring, 60).expect("text").contains("ab 60"));
        r.status = ProductionStatus::Working;
        assert_eq!(status_explanation(&r, Some(wheat), Season::Spring, 60), None);
    }

    #[test]
    fn field_statistics_of_a_real_farm() {
        let defs = defs();
        let mut state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let kind = defs.building_id("vegetable_farm").expect("farm");
        let start = state.world().start;
        // Place the farm on the first valid spot near the start and let it finish and claim.
        let spot = (0..12)
            .flat_map(|r| (-r..=r).flat_map(move |dy| (-r..=r).map(move |dx| glam::IVec2::new(dx, dy))))
            .map(|d| start + d)
            .find(|p| kingdoms_sim::buildings::check_placement(&state, &defs, kind, *p, Rotation::default()).ok)
            .expect("a spot for the farm");
        state.tick(&defs, vec![Command::PlaceBuilding { kind, origin: spot, rotation: Rotation::default() }]);
        let id = state.buildings().id_at(spot).expect("farm placed");
        for _ in 0..(kingdoms_sim::time::TICKS_PER_DAY * 2) {
            state.tick(&defs, Vec::new());
        }
        let r = report::farm_report(&state, &defs, id).expect("the farm is finished and claims fields");
        assert!(!r.fields.is_empty());
        let stats = FieldStats::of(&r);
        assert_eq!(stats.sown + stats.fallow, r.fields.len());
        assert!(stats.fertility <= 255.0 && stats.exhaustion() >= 0.0);
        for f in &r.fields {
            let icon = field_icon(&defs, r.kind, f);
            assert!(icon == "field_soil" || icon.starts_with("field_vegetables_"), "{icon}");
        }
    }
}
