//! Agriculture tests: claims, sowing, growth, harvest, frost, soil, orchards, pastures, bees.

use glam::IVec2;

use super::*;
use crate::command::{Command, RejectReason};
use crate::defs::Weather;
use crate::event::SimEvent;
use crate::production::ProductionStatus;
use crate::test_support::{defs, depot, exec, flat_state, link, rejection, run, spawn};
use crate::time::{GameClock, TICKS_PER_DAY, TICKS_PER_HOUR, TICKS_PER_SEASON};
use crate::weather::WeatherState;

fn farm(state: &GameState, id: BuildingId) -> &Farm {
    state.agriculture().farm(id).expect("farm state")
}

fn status(state: &GameState, id: BuildingId) -> ProductionStatus {
    state.building(id).unwrap().production().unwrap().status()
}

fn crop(key: &str) -> CropId {
    defs().crop_id(key).unwrap()
}

/// Empties `resource` from the fixture depot and adds a second, empty warehouse, both linked
/// to `farm`, so a whole harvest fits into storage.
fn storage_for(state: &mut GameState, farm: BuildingId, resource: crate::defs::ResourceId) {
    let dep = depot(state);
    state.buildings.get_mut(dep).unwrap().inventory.remove(resource, u32::MAX);
    let extra = spawn(state, "warehouse", 30, 4);
    link(state, farm, dep);
    link(state, farm, extra);
}

/// Sets every field of farm `id` to `crop` with `growth` points.
fn plant_all(state: &mut GameState, id: BuildingId, crop: CropId, growth: u32) {
    let f = state.agriculture.farms.get_mut(&id).unwrap();
    for t in &mut f.tiles {
        t.crop = Some(crop);
        t.growth = growth;
    }
}

#[test]
fn farms_claim_exclusive_fields_nearest_first() {
    let d = defs();
    let mut state = flat_state(48);
    let a = spawn(&mut state, "vegetable_farm", 10, 10);
    let events = run(&mut state, 1);
    assert_eq!(farm(&state, a).tiles().len(), 16);
    assert!(events.contains(&SimEvent::FieldsChanged { building: a, fields: 16 }));
    let rect = state.building(a).unwrap().rect();
    let entrance = state.building(a).unwrap().entrance();
    // The ring around the footprint except the entrance, then the nearest tile of the next ring.
    let tiles = farm(&state, a).tiles();
    assert!(tiles.iter().all(|t| t.pos() != entrance));
    assert_eq!(tiles.iter().filter(|t| rect.distance_to(t.pos()) == 1).count(), 15);
    assert_eq!(rect.distance_to(tiles[15].pos()), 2);
    // A second farm next to it only gets tiles the first one does not hold.
    let b = spawn(&mut state, "vegetable_farm", 14, 10);
    run(&mut state, 1);
    let mine: Vec<IVec2> = farm(&state, a).tiles().iter().map(|t| t.pos()).collect();
    assert_eq!(farm(&state, b).tiles().len(), 16);
    assert!(farm(&state, b).tiles().iter().all(|t| !mine.contains(&t.pos())));
    assert_eq!(state.agriculture().claimed_tiles(), 32);
    for t in farm(&state, b).tiles() {
        assert_eq!(state.agriculture().owner_of(t.pos()), Some(b));
        assert_eq!(state.agriculture().field_at(t.pos()).unwrap().sprite(d), "fields/soil");
    }
    state.validate_entities(d).unwrap();
}

#[test]
fn placement_counts_only_unclaimed_farmland() {
    let d = defs();
    let mut state = flat_state(24);
    state.research.mark_researched(d.tech_id("grain_farming").unwrap());
    // A farm that takes its whole radius leaves too little land for another one nearby.
    let first = spawn(&mut state, "wheat_farm", 8, 8);
    run(&mut state, 1);
    assert_eq!(farm(&state, first).tiles().len(), 24);
    let kind = d.building_id("wheat_farm").unwrap();
    let rect = crate::grid::Rotation::default().footprint(IVec2::new(8, 14), d.building(kind).size);
    let free = farmland_around(&state, d, rect, FarmKind::Fields, 5, None, None).len();
    let with_first = farmland_around(&state, d, rect, FarmKind::Fields, 5, Some(first), None).len();
    assert!(free < with_first, "claimed tiles are not free farmland ({free} vs {with_first})");
    let inside = farm(&state, first).tiles().iter().filter(|t| rect.distance_to(t.pos()) <= 5 && !rect.contains(t.pos())).count();
    assert_eq!(with_first - free, inside);
}

#[test]
fn buildings_roads_and_demolition_release_fields() {
    let d = defs();
    let mut state = flat_state(40);
    let id = spawn(&mut state, "vegetable_farm", 10, 10);
    run(&mut state, 1);
    let p = farm(&state, id).tiles()[0].pos();
    state.agriculture.take_dirty_chunks();
    // A road over a field releases it; the farm claims a replacement in its next hour.
    let path = d.road_id("path").unwrap();
    state.roads.set(p, Some(crate::roads::RoadTile { road: path, bridge_axis: None }));
    let events = run(&mut state, 1);
    assert_eq!(state.agriculture().owner_of(p), None);
    assert!(events.contains(&SimEvent::FieldsChanged { building: id, fields: 15 }));
    assert!(!state.take_field_dirty_chunks().is_empty());
    run(&mut state, TICKS_PER_HOUR);
    assert_eq!(farm(&state, id).tiles().len(), 16);
    state.validate_entities(d).unwrap();
    // Demolishing the farm releases everything.
    exec(&mut state, Command::DemolishBuilding { building: id });
    run(&mut state, 1);
    assert!(state.agriculture().farm(id).is_none());
    assert_eq!(state.agriculture().claimed_tiles(), 0);
    state.validate_entities(d).unwrap();
}

#[test]
fn vegetables_are_sown_grow_through_stages_and_are_harvested() {
    let d = defs();
    let mut state = flat_state(40);
    let id = spawn(&mut state, "vegetable_farm", 10, 10);
    storage_for(&mut state, id, d.resource_id("vegetables").unwrap());
    run(&mut state, 2);
    assert_eq!(farm(&state, id).crop(), Some(crop("vegetables")));
    // Sowing 16 tiles takes about 4 farm hours at full staffing.
    run(&mut state, 5 * TICKS_PER_HOUR);
    assert!(farm(&state, id).tiles().iter().all(|t| t.crop() == Some(crop("vegetables"))));
    assert_eq!(status(&state, id), ProductionStatus::Growing);
    // Growth stages rise with the days.
    run(&mut state, 3 * TICKS_PER_DAY);
    let stages: Vec<u8> = farm(&state, id).tiles().iter().map(|t| t.stage()).collect();
    assert!(stages.iter().all(|s| (1..=2).contains(s)), "{stages:?}");
    assert!(state.agriculture().field_at(farm(&state, id).tiles()[0].pos()).unwrap().sprite(d).starts_with("fields/vegetables_"));
    // Ripe after about a week; the harvest goes to the buffer and on to storage.
    let produced_before = state.economy().stats().today().produced.get(d.resource_id("vegetables").unwrap());
    let mut harvested = Vec::new();
    for _ in 0..(5 * TICKS_PER_DAY) {
        for e in state.tick(d, Vec::new()) {
            if let SimEvent::HarvestCompleted { amount, .. } = e {
                harvested.push(amount);
            }
        }
        if !harvested.is_empty() {
            break;
        }
    }
    assert_eq!(harvested.len(), 1, "one harvest round");
    // 16 tiles × 45 × soil (fertility 128: 40 % + 60 % × 0.64 = 78.4 %) ≈ 564.
    let expected = (16.0 * 45.0 * 0.784) as u32;
    assert!(harvested[0].abs_diff(expected) <= 16, "harvest {} vs {expected}", harvested[0]);
    let produced_after = state.economy().stats().today().produced.get(d.resource_id("vegetables").unwrap());
    assert!(produced_after > produced_before || state.economy().stats().history().count() > 0);
    // The farm's output record counts every harvested tile and unit (measured output, §6.3).
    let log = state.building(id).unwrap().production().unwrap().output_log();
    assert_eq!(log.today().units() + log.days().map(|d| d.units()).sum::<u32>(), harvested[0]);
    assert_eq!(log.today().cycles + log.days().map(|d| d.cycles).sum::<u32>(), 16, "one cycle per harvested tile");
    // Harvested fields lose fertility and are sown again (summer is a sowing season).
    let f = farm(&state, id);
    assert!(f.last_harvest().is_some_and(|h| h.amount == harvested[0]));
    // (Tiles waiting for the rest of the harvest over a midnight already regained 3.)
    assert!(f.tiles().iter().all(|t| (128 - 25..=128 - 22).contains(&t.fertility())), "{:?}", f.tiles());
    // After a midnight the farm is valued at its harvests per day.
    run(&mut state, TICKS_PER_DAY);
    let e = crate::report::building_economics(&state, d, id).expect("field farms have economics");
    let days = state.building(id).unwrap().production().unwrap().output_log().days().len() as u32;
    assert_eq!(e.basis, crate::report::EconomicsBasis::Harvests { days });
    assert_eq!(e.recipe, None);
    assert!(e.measured.average_units_milli > 0 && e.outputs_milli > 0, "{e:?}");
    assert_eq!(e.net_milli, e.outputs_milli - e.wages_milli - e.upkeep_milli);
    assert_eq!(e.measured.resource, d.resource_id("vegetables"));
    assert!(e.measured.text.starts_with("Ernte im Mittel der letzten") || e.measured.text.starts_with("Ernte am letzten Tag"), "{}", e.measured.text);
    assert!(e.main_output.is_some_and(|u| u.cost_milli.is_some()));
    state.validate_entities(d).unwrap();
}

#[test]
fn late_sowing_is_skipped_and_frost_kills_unripe_crops() {
    let d = defs();
    let mut state = flat_state(40);
    // Last day of autumn: nothing may be sown any more.
    state.clock = GameClock::at_tick(3 * TICKS_PER_SEASON - TICKS_PER_DAY);
    let id = spawn(&mut state, "wheat_farm", 10, 10);
    run(&mut state, TICKS_PER_HOUR);
    assert!(farm(&state, id).tiles().iter().all(|t| t.crop().is_none()));
    assert_eq!(status(&state, id), ProductionStatus::OutOfSeason);
    // Unripe wheat dies when winter starts; ripe wheat survives.
    let wheat = crop("wheat");
    plant_all(&mut state, id, wheat, 1000);
    state.buildings.get_mut(id).unwrap().paused = true;
    let ripe = farm(&state, id).tiles()[0].pos();
    state.agriculture.farms.get_mut(&id).unwrap().tiles[0].growth = required_growth(d.crop(wheat));
    let mut frozen = 0;
    for e in run(&mut state, TICKS_PER_DAY) {
        if let SimEvent::CropsFrozen { fields } = e {
            frozen += fields;
        }
    }
    assert_eq!(frozen, 23);
    assert_eq!(state.agriculture().tile_at(ripe).unwrap().crop(), Some(wheat));
    assert_eq!(state.clock().season(), Season::Winter);
}

#[test]
fn set_crop_switches_future_sowing() {
    let d = defs();
    let mut state = flat_state(40);
    let id = spawn(&mut state, "wheat_farm", 10, 10);
    let events = exec(&mut state, Command::SetCrop { building: id, crop: crop("barley") });
    assert_eq!(rejection(&events), Some(&RejectReason::FarmNotReady), "the farm starts in its first agriculture step");
    let events = exec(&mut state, Command::SetCrop { building: id, crop: crop("barley") });
    assert!(events.contains(&SimEvent::CropChanged { building: id, crop: crop("barley") }), "{events:?}");
    run(&mut state, 8 * TICKS_PER_HOUR);
    assert!(farm(&state, id).tiles().iter().all(|t| t.crop() == Some(crop("barley"))));
    let events = exec(&mut state, Command::SetCrop { building: id, crop: crop("vegetables") });
    assert_eq!(rejection(&events), Some(&RejectReason::CropNotGrownHere));
    let mill = spawn(&mut state, "sawmill", 30, 10);
    let events = exec(&mut state, Command::SetCrop { building: mill, crop: crop("wheat") });
    assert_eq!(rejection(&events), Some(&RejectReason::NotAFieldFarm));
    state.validate_entities(d).unwrap();
}

#[test]
fn full_output_buffer_holds_the_harvest_back() {
    let d = defs();
    let mut state = flat_state(40);
    let id = spawn(&mut state, "vegetable_farm", 10, 10);
    run(&mut state, 1);
    plant_all(&mut state, id, crop("vegetables"), u32::MAX / 2);
    // Not connected to any storage: the barn (here shrunk to 100) fills after the first tiles.
    let veg = d.resource_id("vegetables").unwrap();
    let p = state.buildings.get_mut(id).unwrap().production.as_mut().unwrap();
    p.output.set_capacities([(veg, 100)]);
    run(&mut state, 12 * TICKS_PER_HOUR);
    let held = state.building(id).unwrap().production().unwrap().output().get(veg);
    assert!((60..=100).contains(&held), "buffer holds {held}");
    assert_eq!(status(&state, id), ProductionStatus::OutputFull);
    assert!(farm(&state, id).tiles().iter().filter(|t| t.crop().is_some()).count() >= 13);
    state.validate_entities(d).unwrap();
}

#[test]
fn orchards_bear_fruit_in_autumn_once_mature() {
    let d = defs();
    let mut state = flat_state(40);
    let id = spawn(&mut state, "orchard", 10, 10);
    storage_for(&mut state, id, d.resource_id("fruit").unwrap());
    run(&mut state, TICKS_PER_HOUR);
    let fruit = crop("fruit");
    let def = d.crop(fruit);
    // Planted trees are saplings, then young trees.
    plant_all(&mut state, id, fruit, required_growth(def) / 4);
    run(&mut state, TICKS_PER_HOUR);
    assert!(farm(&state, id).tiles().iter().all(|t| t.stage() == 0));
    // Mature trees blossom in spring, gather fruit over summer and are harvested in autumn.
    plant_all(&mut state, id, fruit, required_growth(def));
    run(&mut state, TICKS_PER_HOUR);
    assert!(farm(&state, id).tiles().iter().all(|t| t.stage() == 3), "blossom in spring");
    state.clock = GameClock::at_tick(2 * TICKS_PER_SEASON - TICKS_PER_HOUR);
    for t in &mut state.agriculture.farms.get_mut(&id).unwrap().tiles {
        t.fruit = required_fruit(def);
    }
    let mut harvest = None;
    for _ in 0..(2 * TICKS_PER_DAY) {
        for e in state.tick(d, Vec::new()) {
            if let SimEvent::HarvestCompleted { amount, .. } = e {
                harvest = Some(amount);
            }
        }
        if harvest.is_some() {
            break;
        }
    }
    let amount = harvest.expect("autumn harvest");
    let expected = (16.0 * 40.0 * 0.784) as u32;
    assert!(amount.abs_diff(expected) <= 16, "harvest {amount} vs {expected}");
    // Trees stay, fruit starts over; no soil exhaustion.
    assert!(farm(&state, id).tiles().iter().all(|t| t.crop() == Some(fruit) && t.fruit() == 0 && t.fertility() == 128));
    state.validate_entities(d).unwrap();
}

#[test]
fn frost_days_outside_winter_kill_seed_but_keep_the_fruit() {
    let d = defs();
    let mut state = flat_state(40);
    let orchard = spawn(&mut state, "orchard", 10, 10);
    let farm_id = spawn(&mut state, "wheat_farm", 25, 10);
    run(&mut state, TICKS_PER_HOUR);
    let fruit = crop("fruit");
    plant_all(&mut state, orchard, fruit, required_growth(d.crop(fruit)));
    plant_all(&mut state, farm_id, crop("wheat"), 1000);
    for id in [orchard, farm_id] {
        state.buildings.get_mut(id).unwrap().paused = true;
    }
    for t in &mut state.agriculture.farms.get_mut(&orchard).unwrap().tiles {
        t.fruit = 5000;
    }
    // A snowy spring in a cold start area: every day is a frost day.
    state.weather = WeatherState::fixed(Weather::Snow, 100).with_climate(-12);
    // One tick before midnight (the clock starts at 06:00).
    let midnight = |ticks: u64| GameClock::at_tick(ticks - 6 * TICKS_PER_HOUR - 1);
    state.clock = midnight(3 * TICKS_PER_DAY);
    assert_eq!(state.clock.season(), Season::Spring);
    let events = run(&mut state, 2);
    assert!(state.weather.is_frost(d.rules(), state.clock));
    assert!(events.iter().any(|e| matches!(e, SimEvent::CropsFrozen { .. })), "unripe seed freezes");
    assert!(farm(&state, orchard).tiles().iter().all(|t| t.fruit() >= 5000), "the fruit stays on the trees: {:?}", farm(&state, orchard).tiles().iter().map(|t| t.fruit()).collect::<Vec<_>>());
    // The start of winter spoils what is left.
    state.clock = midnight(3 * TICKS_PER_SEASON);
    run(&mut state, 2);
    assert_eq!(state.clock.season(), Season::Winter);
    assert!(farm(&state, orchard).tiles().iter().all(|t| t.fruit() == 0));
}

#[test]
fn fallow_fields_regenerate_and_exhausted_fields_rest() {
    let mut state = flat_state(40);
    let id = spawn(&mut state, "vegetable_farm", 10, 10);
    run(&mut state, 1);
    // Exhausted below 60 % of the natural fertility: left fallow, regaining 3 per day.
    for t in &mut state.agriculture.farms.get_mut(&id).unwrap().tiles {
        t.fertility = 70;
    }
    run(&mut state, TICKS_PER_DAY);
    let f = farm(&state, id);
    assert!(f.tiles().iter().all(|t| t.crop().is_none()), "too poor to sow");
    assert!(f.tiles().iter().all(|t| t.fertility() == 73), "{:?}", f.tiles().iter().map(|t| t.fertility()).collect::<Vec<_>>());
    // Regeneration stops at the natural fertility.
    for t in &mut state.agriculture.farms.get_mut(&id).unwrap().tiles {
        t.fertility = 127;
        t.crop = None;
    }
    state.clock = GameClock::at_tick(3 * TICKS_PER_SEASON + TICKS_PER_HOUR * 18);
    run(&mut state, TICKS_PER_DAY);
    assert!(farm(&state, id).tiles().iter().all(|t| t.fertility() == 128));
}

#[test]
fn storms_damage_and_droughts_wither_standing_crops() {
    let mut state = flat_state(40);
    let id = spawn(&mut state, "vegetable_farm", 10, 10);
    run(&mut state, 1);
    plant_all(&mut state, id, crop("vegetables"), 1000);
    state.weather = WeatherState::fixed(Weather::Storm, 100);
    run(&mut state, TICKS_PER_DAY);
    assert!(farm(&state, id).tiles().iter().all(|t| t.condition() == 80), "storm takes 20 %");
    // A hard drought dries the soil (natural moisture 128, raised by the storm) below 40 within
    // a few days; then the crops wither by 6 % a day.
    state.weather = WeatherState::fixed(Weather::Drought, 100);
    let mut days = 0;
    while farm(&state, id).tiles().iter().any(|t| t.moisture() >= 40) {
        run(&mut state, TICKS_PER_DAY);
        days += 1;
    }
    assert!((4..=6).contains(&days), "dry after {days} days");
    let f = farm(&state, id);
    let before = f.tiles()[0].condition();
    run(&mut state, TICKS_PER_DAY);
    assert_eq!(farm(&state, id).tiles()[0].condition(), before - 6);
}

#[test]
fn growth_depends_on_season_weather_soil_and_moisture() {
    let d = defs();
    let mut state = flat_state(16);
    let tile = FieldTile { fertility: 200, moisture: 200, ..FieldTile::new(IVec2::ZERO, state.world.tile(IVec2::ZERO)) };
    assert_eq!(growth_factor_permille(&state, d, &tile), 1000);
    state.weather = WeatherState::fixed(Weather::Rain, 100);
    assert_eq!(growth_factor_permille(&state, d, &tile), 1100);
    state.weather = WeatherState::fixed(Weather::Drought, 100);
    let dry = FieldTile { moisture: 0, ..tile.clone() };
    assert_eq!(growth_factor_permille(&state, d, &dry), 600 * 400 / 1000);
    let poor = FieldTile { fertility: 0, ..tile.clone() };
    state.weather = WeatherState::fixed(Weather::Clear, 100);
    assert_eq!(growth_factor_permille(&state, d, &poor), 900);
    state.clock = GameClock::at_tick(TICKS_PER_SEASON);
    assert_eq!(growth_factor_permille(&state, d, &tile), 1200);
    state.clock = GameClock::at_tick(3 * TICKS_PER_SEASON);
    assert_eq!(growth_factor_permille(&state, d, &tile), 0);
}

#[test]
fn pastures_are_claimed_exclusively_and_drive_animal_farms() {
    let d = defs();
    let mut state = flat_state(40);
    let a = spawn(&mut state, "cattle_farm", 10, 10);
    let b = spawn(&mut state, "sheep_farm", 14, 10);
    run(&mut state, 1);
    crate::test_support::run_until_at_work(&mut state);
    run(&mut state, 2);
    let (fa, fb) = (farm(&state, a), farm(&state, b));
    assert_eq!((fa.kind(), fa.tiles().len(), fb.tiles().len()), (FarmKind::Pasture, 16, 16));
    assert!(fa.tiles().iter().all(|t| state.agriculture().owner_of(t.pos()) == Some(a)));
    assert!(fb.tiles().iter().all(|t| state.agriculture().owner_of(t.pos()) == Some(b)));
    assert_eq!(state.agriculture().field_at(fa.tiles()[0].pos()).unwrap().sprite(d), "fields/pasture");
    // A new farm starts with half its herd (8 animals on 16 tiles); the animals graze half the
    // pasture until the herd has grown.
    assert_eq!((fa.herd(), fa.herd_capacity(d)), (4, 8));
    assert_eq!(state.building(a).unwrap().production().unwrap().cycle().map(|c| c.environment_permille), Some(500));
    crate::test_support::run_until(&mut state, 3 * crate::time::TICKS_PER_DAY, |s| farm(s, a).herd() == 8);
    assert_eq!(crate::production::pasture_permille(&state, d, a, 16), 1000, "the grown herd grazes the whole pasture");
    state.validate_entities(d).unwrap();
    // Only 8 tiles of grass around a third farm: half productivity.
    let c = IVec2::new(28, 10);
    let kind = d.building_id("cattle_farm").unwrap();
    let rect = crate::grid::Rotation::default().footprint(c, d.building(kind).size);
    let grass: Vec<IVec2> = (0..8).map(|i| IVec2::new(rect.origin.x - 1, rect.origin.y + i - 2)).collect();
    for p in rect.expanded(4).tiles().filter(|p| !rect.contains(*p) && !grass.contains(p)) {
        state.world.tile_mut(p).terrain = crate::world::Terrain::Rock;
    }
    let id = spawn(&mut state, "cattle_farm", c.x, c.y);
    run(&mut state, 1);
    crate::test_support::run_until_at_work(&mut state);
    run(&mut state, 2);
    assert_eq!(farm(&state, id).tiles().len(), 8);
    // Its herd grows to the 4 animals the 8 tiles feed: half of what a full pasture yields.
    crate::test_support::run_until(&mut state, 3 * crate::time::TICKS_PER_DAY, |s| farm(s, id).herd() == 4);
    assert_eq!(crate::production::pasture_permille(&state, d, id, 16), 500);
    run(&mut state, 2 * crate::time::TICKS_PER_DAY);
    assert_eq!(farm(&state, id).herd(), 4, "never more animals than the pasture feeds");
}

#[test]
fn orchard_fields_feed_the_bees() {
    let mut state = flat_state(40);
    let orchard = spawn(&mut state, "orchard", 10, 10);
    run(&mut state, 1);
    let plain = spawn(&mut state, "apiary", 16, 10);
    run(&mut state, 1);
    crate::test_support::run_until_at_work(&mut state);
    run(&mut state, 2);
    let env = |state: &GameState, id| state.building(id).unwrap().production().unwrap().cycle().map(|c| c.environment_permille);
    assert_eq!(env(&state, plain), Some(1000), "grass does not count");
    plant_all(&mut state, orchard, crop("fruit"), 0);
    let bees = spawn(&mut state, "apiary", 10, 16);
    run(&mut state, 1);
    crate::test_support::run_until_at_work(&mut state);
    run(&mut state, 2);
    assert!(env(&state, bees).is_some_and(|e| e > 1000), "orchard fields count: {:?}", env(&state, bees));
}

#[test]
fn foresters_do_not_plant_on_fields_and_fields_clear_stumps() {
    let mut state = flat_state(40);
    let p = IVec2::new(9, 11);
    state.world.tile_mut(p).vegetation = crate::world::Vegetation::Stump { hours: 3 };
    let id = spawn(&mut state, "vegetable_farm", 10, 10);
    run(&mut state, 1);
    assert_eq!(state.agriculture().owner_of(p), Some(id));
    assert_eq!(state.world().tile(p).vegetation, crate::world::Vegetation::None);
    assert!(!crate::production::is_plantable(&state, p));
}



#[test]
fn reports_describe_farms_weather_and_food() {
    let d = defs();
    let mut state = flat_state(40);
    let id = spawn(&mut state, "vegetable_farm", 10, 10);
    run(&mut state, 1);
    // The farmer walks over and sows the first field.
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| {
        crate::report::farm_report(s, d, id).unwrap().fields.iter().any(|f| f.state_text.starts_with("Gemüse"))
    });
    let r = crate::report::farm_report(&state, d, id).unwrap();
    assert_eq!((r.kind, r.fields.len(), r.max_fields), (FarmKind::Fields, 16, 16));
    assert_eq!(r.crops.len(), 1);
    assert!(r.crops[0].selected);
    assert!(r.task_text.as_deref().is_some_and(|t| t.starts_with("Aussaat")), "{:?}", r.task_text);
    assert!(r.fields.iter().any(|f| f.state_text.starts_with("Gemüse")) && r.fields.iter().any(|f| f.state_text == "Brache"));
    run(&mut state, 5 * TICKS_PER_HOUR);
    let r = crate::report::farm_report(&state, d, id).unwrap();
    // Sown at the start of spring: the whole harvest is expected before winter.
    let expected: u32 = r.expected_harvest.iter().map(|a| a.amount).sum();
    assert!((16 * 30..=16 * 36).contains(&expected), "expected harvest {expected}");
    let summary = crate::report::agriculture_summary(&state, d);
    assert_eq!((summary.field_farms, summary.fields, summary.fallow), (1, 16, 0));
    assert_eq!(summary.crops.iter().find(|c| c.crop == crop("vegetables")).map(|c| c.fields), Some(16));
    let weather = crate::report::weather_report(&state, d);
    assert_eq!((weather.season_name.as_str(), weather.weather_name.as_str()), ("Frühling", "Klar"));
    assert_eq!(weather.growth_percent, 100);
    assert!(weather.effects.iter().any(|e| e == "Arbeit im Freien: 100 %"));
    let food = crate::report::food_forecast(&state, d);
    assert_eq!(food.population, 20);
    assert!((food.consumption_per_day - 80.0).abs() < 0.01, "20 adults eat 4 each");
    assert_eq!(food.days_until_winter, 18);
    assert!(food.expected_harvest > 900.0, "vegetables count with 2 saturation each");
    assert!(food.covers_winter, "{food:?}");
    // Nothing harvested yet: the barns hold +0 (no "-0" in the UI); the stock lasts days.
    assert!(food.in_barns == 0.0 && food.in_barns.is_sign_positive(), "{}", food.in_barns);
    assert_eq!(format!("{:.0}", food.in_barns), "0");
    assert!(food.days_of_stock.is_some_and(|d| d.is_finite() && d > 0.0));
    // Nobody eats: no stock duration instead of infinity.
    let mut empty = state.clone();
    crate::test_support::set_adults(&mut empty, 0);
    let food = crate::report::food_forecast(&empty, d);
    assert_eq!(food.days_of_stock, None);
    assert!(food.consumption_per_day.is_sign_positive());
    // In winter the forecast looks at the remaining winter days.
    state.clock = GameClock::at_tick(3 * TICKS_PER_SEASON);
    let food = crate::report::food_forecast(&state, d);
    assert_eq!((food.days_until_winter, food.winter_days), (0, 6));
    assert!((food.consumption_per_day - 96.0).abs() < 0.01, "+20 % in winter");
    let pasture = spawn(&mut state, "sheep_farm", 24, 10);
    run(&mut state, 1);
    let r = crate::report::farm_report(&state, d, pasture).unwrap();
    assert_eq!((r.tile_name.as_str(), r.pasture_percent), ("Weide", 100));
}

/// Agriculture on a huge map: 300 farms (fields of every crop and pastures, about 6000 claimed
/// tiles) with real weather over two summer days and the frost tick at the start of winter.
/// `cargo test --release -p kingdoms_sim agriculture_performance -- --ignored --nocapture`.
#[test]
#[ignore = "performance measurement; run in release mode"]
fn agriculture_performance() {
    let d = defs();
    let mut state = flat_state(384);
    state.research.mark_researched(d.tech_id("grain_farming").unwrap());
    state.research.mark_researched(d.tech_id("animal_husbandry").unwrap());
    state.research.mark_researched(d.tech_id("orchards_and_bees").unwrap());
    let kinds = ["wheat_farm", "vegetable_farm", "orchard", "wheat_farm", "vegetable_farm", "sheep_farm"];
    let mut farms = 0;
    'outer: for by in 0..30 {
        for bx in 0..30 {
            let (x, y) = (6 + bx * 12i32, 6 + by * 12i32);
            if x + 8 >= 378 || y + 8 >= 378 {
                continue;
            }
            spawn(&mut state, kinds[(bx + by) as usize % kinds.len()], x, y);
            farms += 1;
            if farms == 300 {
                break 'outer;
            }
        }
    }
    crate::test_support::set_adults(&mut state, 900);
    state.weather = WeatherState::new(d, 5, state.clock, 0);
    run(&mut state, TICKS_PER_HOUR);
    let fields = state.agriculture().claimed_tiles();
    let measure = |state: &mut GameState, ticks: u64| {
        let (mut agriculture, mut total, mut max) = (0u128, 0u128, 0u128);
        for _ in 0..ticks {
            let steps = state.tick_timed(d);
            let a = steps.iter().find(|(n, _)| *n == "agriculture").map_or(0, |(_, ns)| *ns);
            agriculture += a;
            max = max.max(a);
            total += steps.iter().map(|(_, ns)| ns).sum::<u128>();
        }
        (agriculture as f64 / ticks as f64 / 1000.0, max as f64 / 1000.0, total as f64 / ticks as f64 / 1000.0)
    };
    // Summer: growth, sowing of vegetables, daily soil updates.
    state.clock = GameClock::at_tick(TICKS_PER_SEASON);
    let ticks = 2 * TICKS_PER_DAY;
    let (avg, max, tick) = measure(&mut state, ticks);
    // The last hour of autumn and the first of winter (frost pass over every field).
    state.clock = GameClock::at_tick(3 * TICKS_PER_SEASON - 6 * TICKS_PER_HOUR - TICKS_PER_HOUR);
    let (winter_avg, winter_max, _) = measure(&mut state, 2 * TICKS_PER_HOUR);
    eprintln!(
        "{farms} farms, {fields} claimed tiles: agriculture step {avg:.1} µs (average over {ticks} ticks), slowest {max:.1} µs; \
         whole tick {tick:.1} µs; around the first winter tick: {winter_avg:.1} µs average, {winter_max:.1} µs slowest"
    );
    state.validate_entities(d).unwrap();
}

#[test]
fn validation_and_rederive_keep_claims_consistent() {
    let d = defs();
    let mut state = flat_state(40);
    let id = spawn(&mut state, "vegetable_farm", 10, 10);
    run(&mut state, 1);
    state.validate_entities(d).unwrap();
    // A claim without its farm tile, a field under a road, a foreign crop on a pasture.
    let p = farm(&state, id).tiles()[0].pos();
    let mut broken = state.clone();
    broken.agriculture.claims.remove(&key(p));
    assert!(matches!(broken.validate_entities(d), Err(crate::state::StateError::InvalidAgriculture(_))));
    let mut broken = state.clone();
    let path = d.road_id("path").unwrap();
    broken.roads.set(p, Some(crate::roads::RoadTile { road: path, bridge_axis: None }));
    assert!(matches!(broken.validate_entities(d), Err(crate::state::StateError::InvalidAgriculture(_))));
    let mut broken = state.clone();
    broken.agriculture.farms.get_mut(&id).unwrap().crop = Some(crop("wheat"));
    assert!(matches!(broken.validate_entities(d), Err(crate::state::StateError::InvalidAgriculture(_))));
    let bytes = |s: &GameState| crate::save::encode(&crate::save::SaveHeader::for_state(s, d, "x", 0), s).unwrap();
    // Unchanged data: rederive is a no-op (the fixture's territory is not derived from
    // buildings, so only the agriculture part is applied here).
    let mut copy = state.clone();
    copy.agriculture.rederive(&copy.buildings, d);
    assert_eq!(bytes(&copy), bytes(&state));
    // Fewer fields per farm in rebalanced data: the farthest tiles are released.
    let fewer = crate::test_support::defs_edited("buildings/buildings.ron", |t| {
        t.replace("Fields(crops: [\"vegetables\"], max_tiles: 16", "Fields(crops: [\"vegetables\"], max_tiles: 10")
    });
    state.agriculture.rederive(&state.buildings, &fewer);
    assert_eq!(farm(&state, id).tiles().len(), 10);
    state.validate_entities(&fewer).unwrap();
}

#[test]
fn farmers_sow_standing_on_the_fields() {
    let d = defs();
    let mut state = flat_state(40);
    let id = spawn(&mut state, "vegetable_farm", 10, 10);
    run(&mut state, 1);
    crate::test_support::run_until(&mut state, TICKS_PER_DAY, |s| {
        s.building(id).unwrap().production().unwrap().status() == ProductionStatus::Working
    });
    let farmers: Vec<_> = state
        .population()
        .workers_of(id)
        .iter()
        .filter_map(|c| state.population().citizen(*c))
        .filter(|c| crate::population::is_at_work(&state, d, c))
        .map(|c| c.motion().clone())
        .collect();
    assert!(!farmers.is_empty());
    for m in farmers {
        assert!(m.is_visible(), "farmers work outside");
        assert_eq!(state.agriculture().owner_of(m.pos().tile()), Some(id), "on a field of their farm");
    }
    let view = state.population().workers_of(id).first().and_then(|c| crate::population::citizen_view(&state, d, *c)).unwrap();
    assert_eq!(view.role, crate::population::CitizenRole::Farmer);
}

/// Review finding: a claimed field tile enclosed by buildings (walkable, but in another
/// navigation component than the farm's entrance) became the task; its farmer found no way
/// there and the farm sowed nothing — or, with more farmers, sowed it "remotely".
#[test]
fn farmers_skip_field_tiles_they_cannot_reach() {
    let d = defs();
    for adults in [2, 20] {
        let mut state = flat_state(40);
        crate::test_support::set_adults(&mut state, adults);
        let id = spawn(&mut state, "vegetable_farm", 10, 10);
        run(&mut state, 1);
        let enclosed = farm(&state, id).tiles()[0].pos();
        for dir in crate::grid::Direction::ALL {
            let n = enclosed + dir.offset();
            if state.buildings().at(n).is_none() {
                spawn(&mut state, "well", n.x, n.y);
            }
        }
        crate::navigation::prepare(&mut state, d);
        let entrance = state.building(id).unwrap().entrance();
        assert_eq!(state.agriculture().owner_of(enclosed), Some(id), "still claimed");
        assert!(state.navigation().is_walkable(enclosed));
        assert!(!state.navigation.reachable(entrance, enclosed), "walled in");
        let mut stuck = 0u64;
        for _ in 0..2 * TICKS_PER_DAY {
            run(&mut state, 1);
            assert_ne!(farm(&state, id).task().map(|t| t.tile), Some(enclosed), "never the task");
            stuck += state
                .population()
                .workers_of(id)
                .iter()
                .filter_map(|c| state.population().citizen(*c))
                .filter(|c| matches!(c.motion().phase(), crate::population::TripPhase::NoPath { .. }))
                .count() as u64;
            // Present farmers stand near the task.
            if let Some(task) = farm(&state, id).task() {
                for c in state.population().workers_of(id).iter().filter_map(|c| state.population().citizen(*c)) {
                    if crate::population::is_at_work(&state, d, c) && c.motion().is_visible() {
                        let gap = (c.motion().pos().tile() - task.tile).abs().max_element();
                        assert!(gap <= crate::population::routine::FARMER_REACH, "farmer {gap} tiles from the task");
                    }
                }
            }
        }
        let f = farm(&state, id);
        let sown = f.tiles().iter().filter(|t| t.crop().is_some()).count();
        assert_eq!(stuck, 0, "{adults} adults: no farmer without a way");
        assert!(sown > 0, "{adults} adults: the reachable fields are sown");
        assert!(f.tiles().iter().find(|t| t.pos() == enclosed).is_none_or(|t| t.crop().is_none()), "the enclosed tile stays fallow");
        assert_eq!(state.agriculture().owner_of(enclosed), None, "released after a day out of reach, not claimed again");
    }
}
