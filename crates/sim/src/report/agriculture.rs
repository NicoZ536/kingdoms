//! Agriculture, weather and food reports (docs/game_design.md §8).

use glam::IVec2;
use serde::{Deserialize, Serialize};

use crate::agriculture::{self, FarmKind, HarvestRecord, TaskKind};
use crate::buildings::BuildingId;
use crate::defs::{CropId, Defs, ResourceAmount, ResourceId, Weather};
use crate::population::LifeStage;
use crate::production::{MILLI_PER_UNIT, ProductionStatus};
use crate::state::GameState;
use crate::time::{DAYS_PER_SEASON, Season};

use super::status_text;

/// A crop the player can choose for a field farm.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CropChoice {
    /// Id for `Command::SetCrop`.
    pub crop: CropId,
    /// German name.
    pub name: String,
    /// Harvested resource.
    pub output: ResourceId,
    /// True if it is the farm's current choice.
    pub selected: bool,
}

/// One claimed tile in the farm window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldRow {
    /// Position.
    pub pos: IVec2,
    /// Crop growing (`None`: fallow soil or pasture).
    pub crop: Option<CropId>,
    /// German state, e.g. „Brache“, „Weizen (Stufe 2/4)“, „Reif“, „Weide“.
    pub state_text: String,
    /// Visual stage 0–4.
    pub stage: u8,
    /// Growth towards ripeness (annuals) or bearing trees (orchards) in percent.
    pub growth_percent: u8,
    /// True if it can be harvested now.
    pub ripe: bool,
    /// Soil fertility 0–255 and the tile's natural value.
    pub fertility: u8,
    /// Natural fertility of the tile.
    pub natural_fertility: u8,
    /// Soil moisture 0–255.
    pub moisture: u8,
    /// Share of the full yield left after storms and withering, in percent.
    pub condition: u8,
    /// Expected harvest of this tile in units (0 if it will not ripen before winter).
    pub expected_yield: u32,
}

/// Farm window of a field or pasture farm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FarmReport {
    /// The building.
    pub building: BuildingId,
    /// Fields or pasture.
    pub kind: FarmKind,
    /// German kind of tile: „Feld“ / „Weide“.
    pub tile_name: String,
    /// Status of the farm.
    pub status: ProductionStatus,
    /// German status text.
    pub status_text: String,
    /// Selectable crops (field farms).
    pub crops: Vec<CropChoice>,
    /// Claimed tiles (nearest first).
    pub fields: Vec<FieldRow>,
    /// Most tiles the farm can hold.
    pub max_fields: u16,
    /// Running work: German description, e.g. „Ernte (45 %)“.
    pub task_text: Option<String>,
    /// Progress of the running work 0.0–1.0.
    pub task_progress: f32,
    /// Units expected from the standing crops until winter (field farms), per resource.
    pub expected_harvest: Vec<ResourceAmount>,
    /// Harvest in progress.
    pub harvest_in_progress: Option<HarvestRecord>,
    /// The last completed harvest.
    pub last_harvest: Option<HarvestRecord>,
    /// Harvested units waiting in the barn for carriers, per resource.
    pub in_barn: Vec<ResourceAmount>,
    /// Pasture coverage in percent (pasture farms).
    pub pasture_percent: u8,
}

fn crop_name(defs: &Defs, crop: CropId) -> String {
    defs.crops().get(crop.index()).map_or_else(|| crop.to_string(), |c| c.name.clone())
}

/// Adds `units` of `resource` to a list of amounts.
fn add_amount(list: &mut Vec<ResourceAmount>, resource: ResourceId, units: u32) {
    if units == 0 {
        return;
    }
    match list.iter_mut().find(|a| a.resource == resource) {
        Some(a) => a.amount = a.amount.saturating_add(units),
        None => list.push(ResourceAmount { resource, amount: units }),
    }
    list.sort_by_key(|a| a.resource);
}

fn units(milli: u64) -> u32 {
    u32::try_from(milli / u64::from(MILLI_PER_UNIT)).unwrap_or(u32::MAX)
}

/// Farm window data (`None` if the building is no finished farm).
pub fn farm_report(state: &GameState, defs: &Defs, building: BuildingId) -> Option<FarmReport> {
    let b = state.building(building)?;
    let def = defs.buildings().get(b.kind().index())?;
    let (kind, max_fields, _) = agriculture::farm_spec(def)?;
    let farm = state.agriculture().farm(building)?;
    let season = state.clock().season();
    let status = b.production().map_or(ProductionStatus::Idle, |p| p.status());
    let crops = agriculture::farm_crops(def)
        .iter()
        .filter_map(|c| defs.crops().get(c.index()))
        .map(|c| CropChoice { crop: c.id, name: c.name.clone(), output: c.output, selected: farm.crop() == Some(c.id) })
        .collect();
    let mut expected_harvest = Vec::new();
    let fields = farm
        .tiles()
        .iter()
        .map(|t| {
            let ripe = t.is_ripe(defs, season);
            let expected = units(agriculture::expected_harvest_milli(state, defs, t));
            if let Some(crop) = t.crop().and_then(|c| defs.crops().get(c.index())) {
                add_amount(&mut expected_harvest, crop.output, expected);
            }
            let state_text = match (kind, t.crop()) {
                (FarmKind::Pasture, _) => "Weide".to_owned(),
                (FarmKind::Fields, None) => "Brache".to_owned(),
                (FarmKind::Fields, Some(c)) if ripe => format!("{} – reif", crop_name(defs, c)),
                (FarmKind::Fields, Some(c)) => format!("{} (Stufe {}/4)", crop_name(defs, c), t.stage()),
            };
            FieldRow {
                pos: t.pos(),
                crop: t.crop(),
                state_text,
                stage: t.stage(),
                growth_percent: (t.growth_fraction(defs) * 100.0).round() as u8,
                ripe,
                fertility: t.fertility(),
                natural_fertility: state.world().get(t.pos()).map_or(0, |w| w.fertility),
                moisture: t.moisture(),
                condition: t.condition(),
                expected_yield: expected,
            }
        })
        .collect();
    let task = farm.task();
    let in_barn = b
        .production()
        .map(|p| p.output().iter().filter(|m| m.amount > 0).collect::<Vec<ResourceAmount>>())
        .unwrap_or_default();
    let pasture_percent = match kind {
        FarmKind::Pasture => (farm.tiles().len() * 100 / usize::from(max_fields.max(1))).min(100) as u8,
        FarmKind::Fields => 0,
    };
    Some(FarmReport {
        building,
        kind,
        tile_name: kind.tile_name().to_owned(),
        status,
        status_text: status_text(defs, status),
        crops,
        fields,
        max_fields,
        task_text: task.map(|t| {
            let what = match t.kind {
                TaskKind::Sow if farm.crop().and_then(|c| defs.crops().get(c.index())).is_some_and(|c| c.perennial) => {
                    "Bäume pflanzen"
                }
                kind => kind.display_name(),
            };
            format!("{what} ({:.0} %)", t.fraction() * 100.0)
        }),
        task_progress: task.map_or(0.0, |t| t.fraction()),
        expected_harvest,
        harvest_in_progress: farm.harvest_in_progress(),
        last_harvest: farm.last_harvest(),
        in_barn,
        pasture_percent,
    })
}

/// One crop in the agriculture overview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CropRow {
    /// The crop.
    pub crop: CropId,
    /// German name.
    pub name: String,
    /// Harvested resource.
    pub output: ResourceId,
    /// Tiles growing it.
    pub fields: u32,
    /// Tiles ripe now.
    pub ripe: u32,
    /// Units expected until winter.
    pub expected: u32,
}

/// Agriculture overview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgricultureSummary {
    /// Field farms.
    pub field_farms: u32,
    /// Animal farms on pasture.
    pub pasture_farms: u32,
    /// Claimed field tiles.
    pub fields: u32,
    /// Field tiles without a crop.
    pub fallow: u32,
    /// Claimed pasture tiles.
    pub pasture_tiles: u32,
    /// Field tiles per crop.
    pub crops: Vec<CropRow>,
    /// Harvested units waiting in barns, per resource.
    pub in_barns: Vec<ResourceAmount>,
    /// Average condition of the sown fields in percent (`None` without sown fields).
    pub average_condition: Option<u8>,
}

/// Fields, crops and expected harvests of the whole kingdom.
pub fn agriculture_summary(state: &GameState, defs: &Defs) -> AgricultureSummary {
    let season = state.clock().season();
    let mut out = AgricultureSummary {
        field_farms: 0,
        pasture_farms: 0,
        fields: 0,
        fallow: 0,
        pasture_tiles: 0,
        crops: defs
            .crops()
            .iter()
            .map(|c| CropRow { crop: c.id, name: c.name.clone(), output: c.output, fields: 0, ripe: 0, expected: 0 })
            .collect(),
        in_barns: Vec::new(),
        average_condition: state.agriculture().average_field_condition(),
    };
    for (id, farm) in state.agriculture().farms() {
        match farm.kind() {
            FarmKind::Pasture => {
                out.pasture_farms += 1;
                out.pasture_tiles += farm.tiles().len() as u32;
            }
            FarmKind::Fields => {
                out.field_farms += 1;
                out.fields += farm.tiles().len() as u32;
                for t in farm.tiles() {
                    match t.crop().and_then(|c| out.crops.get_mut(c.index())) {
                        Some(row) => {
                            row.fields += 1;
                            row.ripe += u32::from(t.is_ripe(defs, season));
                            row.expected = row.expected.saturating_add(units(agriculture::expected_harvest_milli(state, defs, t)));
                        }
                        None => out.fallow += 1,
                    }
                }
                if let Some(p) = state.building(id).and_then(|b| b.production()) {
                    for m in p.output().iter() {
                        add_amount(&mut out.in_barns, m.resource, m.amount);
                    }
                }
            }
        }
    }
    out
}

/// Today's weather and the season with their effects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherReport {
    /// German date, e.g. „Sommer · Tag 3 · Jahr 1“.
    pub date: String,
    /// Season.
    pub season: Season,
    /// German season name.
    pub season_name: String,
    /// Days left in the season including today.
    pub days_left_in_season: u64,
    /// Today's weather.
    pub weather: Weather,
    /// German weather name („Regen“).
    pub weather_name: String,
    /// German description with strength („Starker Regen“).
    pub description: String,
    /// Intensity in percent.
    pub intensity: u8,
    /// Consecutive days with this weather.
    pub streak_days: u16,
    /// Consecutive days without rain, storm or snow (0 on a wet day).
    pub dry_days: u16,
    /// True while a developer command or an event forces the weather.
    pub forced: bool,
    /// Who forces the weather and for how long (German), e.g. „Wetter vom Ereignis „Dürre“
    /// vorgegeben (noch 3 Tage)“; `None` for the normal weather.
    pub forced_text: Option<String>,
    /// Crop growth now (season × weather) in percent.
    pub growth_percent: u32,
    /// Work speed of outdoor buildings now (winter × weather) in percent.
    pub outdoor_work_percent: u32,
    /// Work speed of indoor buildings now (winter) in percent.
    pub indoor_work_percent: u32,
    /// Fuel demand of households in percent (0 on warm days: no heating).
    pub heating_percent: u32,
    /// Today's temperature in °C.
    pub temperature_c: i8,
    /// German temperature („−3 °C“).
    pub temperature_text: String,
    /// Households heat today (below `heating_below_c`).
    pub heating_needed: bool,
    /// A frost day (crops that frost kills die).
    pub frost: bool,
    /// Disease risk in percent.
    pub disease_percent: u32,
    /// Snow cover 0–100.
    pub snow_cover: u8,
    /// Ground wetness 0–100.
    pub ground_wetness: u8,
    /// German lines describing the effects for the player.
    pub effects: Vec<String>,
}

/// Who forces the weather and how many days (today included) it still holds.
fn forced_text(defs: &Defs, forced: crate::weather::ForcedWeather, today: u64) -> String {
    let by = match forced.event.and_then(|e| defs.events().get(e.index())) {
        Some(event) => format!("vom Ereignis „{}“", event.title),
        None => "vom Entwicklerbefehl".to_owned(),
    };
    let days = match forced.until_day.saturating_sub(today) {
        1 => "nur noch heute".to_owned(),
        n => format!("noch {n} Tage"),
    };
    format!("Wetter {by} vorgegeben ({days})")
}

/// Dry days from which the weather report mentions the dry spell.
const DRY_SPELL_DAYS: u16 = 3;

/// Weather window / HUD data.
pub fn weather_report(state: &GameState, defs: &Defs) -> WeatherReport {
    let rules = defs.rules();
    let clock = state.clock();
    let w = state.weather();
    let season = clock.season();
    let winter = season == Season::Winter;
    let season_growth = (f64::from(*rules.season_growth.get(season)) * 100.0).round() as u32;
    let growth = season_growth * w.growth_percent(rules) / 100;
    let indoor = if winter { u32::from(rules.winter_work_speed_percent) } else { 100 };
    let outdoor = indoor * w.outdoor_work_percent(rules) / 100;
    let heating = if w.needs_heating(rules, clock) { w.heating_percent(rules) } else { 0 };
    let disease = w.disease_percent(rules);
    let mut effects = vec![format!("Wachstum der Felder: {growth} %"), format!("Arbeit im Freien: {outdoor} %")];
    if indoor != 100 {
        effects.push(format!("Arbeit in Werkstätten: {indoor} %"));
    }
    let temperature_text = w.temperature_text(rules, clock);
    effects.insert(0, format!("Temperatur: {temperature_text}"));
    if heating > 0 {
        effects.push(format!("Heizbedarf: {heating} %"));
    }
    if w.is_frost(rules, clock) {
        effects.push("Frost: unreife Saat erfriert".to_owned());
    }
    if winter {
        effects.push(format!("Nahrungsbedarf: {} %", rules.winter_food_consumption_percent));
    }
    if disease != 100 {
        effects.push(format!("Krankheitsrisiko: {disease} %"));
    }
    let damage = w.field_damage_percent(rules);
    if damage > 0 {
        effects.push(format!("Sturmschäden an Feldern: −{damage} %"));
    }
    let moisture = w.moisture_per_day(rules);
    if moisture <= -20 {
        effects.push("Die Felder trocknen aus.".to_owned());
    } else if moisture >= 20 {
        effects.push("Der Regen bewässert die Felder.".to_owned());
    }
    if w.current() == Weather::Drought {
        effects.push("Ohne Brunnen leiden die Bewohner unter Durst.".to_owned());
    }
    // A long dry spell is what a drought follows (§12).
    if !winter && w.dry_days() >= DRY_SPELL_DAYS {
        effects.push(format!("Seit {} Tagen kein Regen.", w.dry_days()));
    }
    WeatherReport {
        date: clock.date_string(),
        season,
        season_name: season.display_name().to_owned(),
        days_left_in_season: DAYS_PER_SEASON - clock.day_of_season() + 1,
        weather: w.current(),
        weather_name: w.current().display_name().to_owned(),
        description: w.description(),
        intensity: w.intensity(),
        streak_days: w.streak_days(),
        dry_days: w.dry_days(),
        forced: w.forced().is_some(),
        forced_text: w.forced().map(|f| forced_text(defs, f, clock.total_days())),
        growth_percent: growth,
        outdoor_work_percent: outdoor,
        indoor_work_percent: indoor,
        heating_percent: heating,
        temperature_c: w.temperature_c(rules, clock),
        temperature_text,
        heating_needed: heating > 0,
        frost: w.is_frost(rules, clock),
        disease_percent: disease,
        snow_cover: w.snow_cover(),
        ground_wetness: w.ground_wetness(),
        effects,
    }
}

/// Will the food last through the next winter? All amounts in saturation points (§4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FoodForecast {
    /// Citizens.
    pub population: u32,
    /// Food eaten per day now.
    pub consumption_per_day: f32,
    /// Food eaten per winter day (+20 %).
    pub winter_consumption_per_day: f32,
    /// Edible food in the kingdom stock.
    pub in_stock: f32,
    /// Edible food harvested and waiting in farm barns.
    pub in_barns: f32,
    /// Edible harvest expected from standing crops until winter (or from the trees' fruit).
    pub expected_harvest: f32,
    /// Edible food made on the last closed day by producers other than fields (animals,
    /// fishing, bakeries, …).
    pub other_production_per_day: f32,
    /// Days until winter starts (0 during winter).
    pub days_until_winter: u32,
    /// Winter days still ahead (6 before winter, fewer during it).
    pub winter_days: u32,
    /// Food expected at the start of winter (now during winter).
    pub at_winter_start: f32,
    /// Food left at the end of winter (negative: shortage).
    pub after_winter: f32,
    /// True if the food is expected to last through winter.
    pub covers_winter: bool,
    /// Days the current stock lasts at today's consumption without production (`None` while
    /// nobody eats).
    pub days_of_stock: Option<f32>,
    /// German summary, e.g. „Die Vorräte reichen über den Winter.“
    pub summary: String,
}

/// Sum that is `+0.0` for no values (`Iterator::sum` of `f32` starts at `-0.0`, which formats
/// as "-0").
fn total(values: impl Iterator<Item = f32>) -> f32 {
    values.fold(0.0, |sum, v| sum + v)
}

/// Saturation per unit of a resource.
fn nutrition(defs: &Defs, resource: ResourceId) -> f32 {
    defs.resources().get(resource.index()).map_or(0.0, |r| r.nutrition)
}

/// Food forecast for the coming (or running) winter.
pub fn food_forecast(state: &GameState, defs: &Defs) -> FoodForecast {
    let rules = defs.rules();
    let pop = &rules.population;
    let clock = state.clock();
    let population = state.population();
    let consumers = total(
        population.citizens().map(|(_, c)| if c.stage() == LifeStage::Child { f32::from(pop.child_food_percent) / 100.0 } else { 1.0 }),
    );
    let base = consumers * pop.food_per_day;
    let winter_factor = f32::from(rules.winter_food_consumption_percent) / 100.0;
    let winter_now = clock.season() == Season::Winter;
    let consumption = if winter_now { base * winter_factor } else { base };
    let in_stock = total(defs.resources().iter().map(|r| state.economy().stock().get(r.id) as f32 * r.nutrition));
    let summary = agriculture_summary(state, defs);
    let in_barns = total(summary.in_barns.iter().map(|a| a.amount as f32 * nutrition(defs, a.resource)));
    let expected_harvest = total(summary.crops.iter().map(|c| c.expected as f32 * nutrition(defs, c.output)));
    // Producers other than fields: edible resources that no crop yields directly.
    let crop_outputs: Vec<ResourceId> = defs.crops().iter().map(|c| c.output).collect();
    let other: f32 = state.economy().stats().history().next_back().map_or(0.0, |day| {
        total(
            defs.resources()
                .iter()
                .filter(|r| r.is_edible() && !crop_outputs.contains(&r.id))
                .map(|r| day.flows.produced.get(r.id) as f32 * r.nutrition),
        )
    });
    let days_until_winter = if winter_now {
        0
    } else {
        let mut days = (DAYS_PER_SEASON - clock.day_of_season() + 1) as u32;
        let mut s = clock.season().next();
        while s != Season::Winter {
            days += DAYS_PER_SEASON as u32;
            s = s.next();
        }
        days
    };
    let winter_days = if winter_now { (DAYS_PER_SEASON - clock.day_of_season() + 1) as u32 } else { DAYS_PER_SEASON as u32 };
    let at_winter_start = in_stock + in_barns + expected_harvest + (other - base) * days_until_winter as f32;
    let after_winter = at_winter_start + (other - base * winter_factor) * winter_days as f32;
    let covers_winter = after_winter >= 0.0;
    let days_of_stock = (consumption > 0.0).then(|| in_stock / consumption);
    let summary = if covers_winter {
        "Die Vorräte reichen voraussichtlich über den Winter.".to_owned()
    } else {
        let days = if base > 0.0 { (-after_winter / (base * winter_factor)).ceil().max(1.0) } else { 1.0 };
        if days <= 1.0 {
            "Im Winter fehlen voraussichtlich Nahrungsmittel für etwa 1 Tag.".to_owned()
        } else {
            format!("Im Winter fehlen voraussichtlich Nahrungsmittel für etwa {days:.0} Tage.")
        }
    };
    FoodForecast {
        population: population.len() as u32,
        consumption_per_day: consumption,
        winter_consumption_per_day: base * winter_factor,
        in_stock,
        in_barns,
        expected_harvest,
        other_production_per_day: other,
        days_until_winter,
        winter_days,
        at_winter_start,
        after_winter,
        covers_winter,
        days_of_stock,
        summary,
    }
}

/// Fuel for the households' heating (docs/game_design.md §6.1 Heizen geht vor): what the stock
/// holds and how long it lasts at today's demand.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FuelForecast {
    /// Heating value of the wood and coal in the kingdom stock (construction material
    /// included), in units of wood.
    pub in_stock: f32,
    /// Heating value all households burn per day at today's demand (on warm days the plain
    /// winter demand).
    pub per_day: f32,
    /// True on days the households heat or stock up for it (cold days, the last day of autumn):
    /// workshops then get fuel only beyond the reserve.
    pub heating_season: bool,
    /// The households' reserve the workshops do not get (0 outside the heating season).
    pub reserve: f32,
    /// Whole days the stock lasts (`None` while nobody needs heating).
    pub days: Option<u32>,
    /// True if the stock lasts fewer than `population.fuel_warning_days` days in the heating
    /// season (the warning „Brennholz wird knapp“).
    pub short: bool,
    /// German summary, e.g. „Holz und Kohle reichen zum Heizen etwa 6 Tage (Bedarf 170 je Tag).“
    pub summary: String,
}

/// The fuel forecast (see [`FuelForecast`]).
pub fn fuel_forecast(state: &GameState, defs: &Defs) -> FuelForecast {
    use crate::population::supply;
    let rules = defs.rules();
    let stock = supply::fuel_stock_milli(state, defs);
    // On warm days the plain demand (100 %), as on the last day of autumn.
    let per_day = supply::heating_demand_milli_per_day(state, defs);
    let heating_season = supply::heating_season(state, defs);
    let reserve = supply::fuel_reserve_milli(state, defs);
    let days = (per_day > 0).then(|| u32::try_from(stock / per_day).unwrap_or(u32::MAX));
    let short = heating_season && per_day > 0 && (stock as f64) < per_day as f64 * f64::from(rules.population.fuel_warning_days);
    let units = |milli: u64| milli as f32 / 1000.0;
    let summary = match days {
        None => "Niemand muss heizen.".to_owned(),
        Some(n) => {
            let lasts = match n {
                0 => "nicht einmal einen Tag".to_owned(),
                1 => "etwa einen Tag".to_owned(),
                n => format!("etwa {n} Tage"),
            };
            format!("Holz und Kohle reichen zum Heizen {lasts} (Bedarf {} je Tag).", (units(per_day)).round() as u64)
        }
    };
    FuelForecast { in_stock: units(stock), per_day: units(per_day), heating_season, reserve: units(reserve), days, short, summary }
}
