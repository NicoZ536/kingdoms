//! Tests of the data loader: the shipped data must load and match docs/game_design.md, and
//! broken data must be reported with file, definition and reason.

use std::path::Path;

use super::*;
use crate::test_support::{DATA_DIR, defs};
use crate::time::Season;
use crate::world::DepositKind;

fn shipped_files() -> Vec<DataFile> {
    match loader::read_dir(Path::new(DATA_DIR)) {
        Ok(files) => files,
        Err(e) => panic!("cannot read shipped data: {e}"),
    }
}

/// Shipped files with one exact text replacement in `path` (which must match exactly once).
fn edited(path: &str, from: &str, to: &str) -> Vec<DataFile> {
    let mut files = shipped_files();
    let file = files.iter_mut().find(|f| f.path == path).unwrap_or_else(|| panic!("no file {path}"));
    assert_eq!(file.text.matches(from).count(), 1, "pattern {from:?} must occur exactly once in {path}");
    file.text = file.text.replacen(from, to, 1);
    files
}

fn load_err(files: &[DataFile]) -> DefsError {
    match Defs::from_files(files) {
        Ok(_) => panic!("broken data was accepted"),
        Err(e) => e,
    }
}

fn assert_problem(err: &DefsError, file: &str, item: &str, message_part: &str) {
    assert!(
        err.problems().iter().any(|p| p.file == file && p.item == item && p.message.contains(message_part)),
        "expected problem {file}: {item}: …{message_part}… in:\n{err}"
    );
}

// ---- shipped data ------------------------------------------------------------------------

#[test]
fn shipped_data_loads_with_expected_counts() {
    let d = defs();
    assert_eq!(d.resources().len(), 38);
    assert_eq!(d.buildings().len(), 82);
    assert_eq!(d.roads().len(), 6);
    assert_eq!(d.crops().len(), 4);
    assert_eq!(d.units().len(), 7);
    assert_eq!(d.events().len(), 14);
    assert_eq!(d.kingdom_data().realms.len(), 10);
    assert_eq!(d.kingdom_data().town_plan.first().map(|e| d.building(e.building).key.as_str()), Some("castle"));
    assert!(d.techs().len() >= 24);
    for area in TechArea::ALL {
        assert!(d.techs_in(area).count() >= 3, "area {area:?} needs at least 3 technologies");
    }
    assert_eq!(d.scenario(d.default_scenario()).key, DEFAULT_SCENARIO);
    // Military (docs/game_design.md §11) and ruler ranks (§1).
    let ranks: Vec<&str> = d.ranks().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(ranks, ["Siedlung", "Dorf", "Marktflecken", "Stadt", "Großstadt", "Königreich", "Imperium"]);
    assert_eq!(d.ranks()[2].building, d.building_id("market"));
    assert_eq!(d.army_mix().len(), 4);
    let spear = d.unit_by_key("spearman").unwrap();
    assert_eq!(spear.bonus.map(|b| b.against), Some(crate::defs::UnitClass::Cavalry));
    assert_eq!(d.unit_by_key("archer").map(|u| (u.class, u.range)), Some((crate::defs::UnitClass::Ranged, 6)));
    assert_eq!(d.unit_by_key("cavalry").map(|u| (u.speed_permille, u.hit_points)), Some((1800, 120)));
    assert_eq!(d.unit_by_key("militia").map(|u| u.training_ticks), Some(4 * crate::time::TICKS_PER_HOUR));
    let tower = d.building_by_key("watchtower").and_then(|b| b.defense).unwrap();
    assert_eq!((tower.ranged, tower.range), (5, 8));
    assert!(d.building_by_key("wall").and_then(|b| b.defense).is_some_and(|w| w.hit_points > 0));
}

#[test]
fn broken_military_data_is_reported() {
    let err = load_err(&edited("units.ron", "ranged: 7, range: 6,", "ranged: 7, range: 0,")).to_string();
    assert!(err.contains("ranged units need a range"), "{err}");
    let err = load_err(&edited("rules.ron", r#"("militia", 30)"#, r#"("knight", 30)"#)).to_string();
    assert!(err.contains("unknown unit 'knight'"), "{err}");
    let err = load_err(&edited("rules.ron", r#"building: "market""#, r#"building: "bazaar""#)).to_string();
    assert!(err.contains("unknown building 'bazaar'"), "{err}");
    let err = load_err(&edited("rules.ron", "population: 25, wealth: 1500", "population: 25, wealth: 99999")).to_string();
    assert!(err.contains("must not require less"), "{err}");
    let err = load_err(&edited("buildings/buildings.ron", "defense: (ranged: 5, range: 8, hit_points: 250)", "defense: (ranged: 5, hit_points: 250)"))
        .to_string();
    assert!(err.contains("exactly when it shoots"), "{err}");
}

#[test]
fn every_canonical_id_of_the_design_document_exists() {
    let d = defs();
    let resources = [
        "wood",
        "stone",
        "loam",
        "clay",
        "iron_ore",
        "coal",
        "sand",
        "wool",
        "grain",
        "hides",
        "gold_ore",
        "wheat",
        "flour",
        "bread",
        "meat",
        "fish",
        "milk",
        "cheese",
        "vegetables",
        "fruit",
        "honey",
        "planks",
        "tools",
        "bricks",
        "iron_bars",
        "cloth",
        "clothes",
        "furniture",
        "weapons",
        "ceramics",
        "beer",
        "leather",
        "jewelry",
        "wine",
        "spices",
        "silk",
        "art",
    ];
    for key in resources {
        assert!(d.resource_id(key).is_some(), "resource {key} missing");
    }
    let buildings = [
        "house_wood",
        "farmhouse",
        "citizen_house",
        "townhouse",
        "residence",
        "manor",
        "woodcutter",
        "forester",
        "sawmill",
        "quarry",
        "clay_pit",
        "sand_pit",
        "iron_mine",
        "coal_mine",
        "charcoal_kiln",
        "gold_mine",
        "smelter",
        "smithy",
        "weaponsmith",
        "brickworks",
        "pottery",
        "weaver",
        "tailor",
        "tannery",
        "carpenter",
        "mill",
        "bakery",
        "brewery",
        "cheese_dairy",
        "winery",
        "jeweler",
        "artisan",
        "wheat_farm",
        "vegetable_farm",
        "orchard",
        "cattle_farm",
        "chicken_farm",
        "sheep_farm",
        "fishery",
        "apiary",
        "town_hall",
        "market",
        "church",
        "school",
        "hospital",
        "well",
        "tavern",
        "warehouse",
        "storehouse",
        "trading_post",
        "barracks",
        "watchtower",
        "archer_post",
        "stables",
        "wall",
        "gate",
        "castle",
    ];
    for key in buildings {
        assert!(d.building_id(key).is_some(), "building {key} missing");
    }
    for key in ["path", "gravel", "road", "paved", "trade_road", "bridge"] {
        assert!(d.road_id(key).is_some(), "road {key} missing");
    }
    for key in ["militia", "swordsman", "spearman", "archer", "crossbowman", "cavalry", "siege"] {
        assert!(d.unit_id(key).is_some(), "unit {key} missing");
    }
    for key in ["wheat", "barley", "vegetables", "fruit"] {
        assert!(d.crop_id(key).is_some(), "crop {key} missing");
    }
}

#[test]
fn design_values_are_carried_over() {
    let d = defs();
    let res = |k: &str| d.resource_id(k).unwrap();
    let b = |k: &str| d.building_by_key(k).unwrap();

    let bread = d.resource_by_key("bread").unwrap();
    assert_eq!((bread.base_price, bread.nutrition, bread.category), (10, 3.0, ResourceCategory::Food));
    assert!(d.resource_by_key("silk").unwrap().import_only);
    assert_eq!(d.resource_by_key("art").unwrap().base_price, 150);

    let house = b("house_wood");
    assert_eq!(house.size, glam::IVec2::new(2, 2));
    assert_eq!(house.cost.gold, 20);
    assert_eq!(house.cost.material(res("wood")), 12);
    assert_eq!(house.build_hours, 6.0);
    let housing = house.housing.as_ref().unwrap();
    assert_eq!((housing.capacity, housing.tier), (5, 1));
    assert_eq!(house.upgrade.as_ref().map(|u| u.to), d.building_id("farmhouse"));
    // Houses name no upgrade cost: the target tier's construction cost and hours apply.
    let farmhouse = b("farmhouse");
    assert_eq!(house.upgrade.as_ref().map(|u| (&u.cost, u.hours)), Some((&farmhouse.cost, farmhouse.build_hours)));

    let sawmill = b("sawmill");
    assert_eq!(sawmill.size, glam::IVec2::new(2, 3));
    let planks = sawmill.recipe("planks").unwrap();
    assert_eq!(planks.inputs, vec![ResourceRate { resource: res("wood"), amount: 2.0 }]);
    assert_eq!(planks.outputs, vec![ResourceRate { resource: res("planks"), amount: 3.0 }]);
    assert_eq!(planks.hours, 2.0);

    let carpenter = b("carpenter").recipe("furniture").unwrap().clone();
    assert!(carpenter.inputs.contains(&ResourceRate { resource: res("tools"), amount: 0.25 }));
    let beer = b("brewery").recipe("beer").unwrap().clone();
    assert_eq!(beer.optional_inputs[0].resource, res("honey"));
    assert_eq!(beer.optional_inputs[0].amount, 0.5);

    assert_eq!(b("clay_pit").production.as_ref().unwrap().recipes.len(), 2);
    assert!(b("forester").production.as_ref().unwrap().recipes[0].outputs.is_empty());
    assert!(!b("forester").production.as_ref().unwrap().uses_tools, "forester is exempt from tool wear");
    assert!(!b("mill").production.as_ref().unwrap().uses_tools, "mill is exempt from tool wear");
    assert!(b("smelter").production.as_ref().unwrap().uses_tools);
    assert_eq!(b("smelter").production.as_ref().unwrap().buffer_capacity, 8);
    assert!(matches!(
        b("iron_mine").production.as_ref().unwrap().gathering,
        Some(Gathering::Deposit { kind: DepositKind::Iron, radius: 1, .. })
    ));
    assert!(b("iron_mine").placement.contains(&PlacementRule::NearDeposit {
        kind: DepositKind::Iron,
        max_distance: 1,
        or_terrain: None
    }));
    match &b("wheat_farm").production.as_ref().unwrap().gathering {
        Some(Gathering::Fields { crops, max_tiles, .. }) => {
            assert_eq!(*max_tiles, 24);
            assert_eq!(crops, &vec![d.crop_id("wheat").unwrap(), d.crop_id("barley").unwrap()]);
        }
        other => panic!("wheat_farm must cultivate fields, got {other:?}"),
    }
    assert_eq!(d.crop_by_key("barley").unwrap().output, res("grain"));
    assert!(d.crop_by_key("fruit").unwrap().perennial);
    assert_eq!(d.crop_by_key("fruit").unwrap().harvest_seasons, vec![Season::Autumn]);

    let town_hall = b("town_hall");
    assert_eq!(town_hall.role, BuildingRole::Headquarters);
    assert_eq!(town_hall.max_count(), Some(1));
    assert_eq!(town_hall.storage.as_ref().unwrap().capacity_per_resource, 200);
    assert_eq!(town_hall.territory_radius, 20);
    assert_eq!(b("warehouse").storage.as_ref().unwrap().capacity_per_resource, 400);
    assert_eq!(b("well").service_radius(ServiceKind::Water), Some(8));
    assert_eq!(b("church").service_radius(ServiceKind::Religion), Some(20));
    assert_eq!(b("castle").service_radius(ServiceKind::Safety), Some(26));
    assert_eq!(b("watchtower").garrison, 1);
    assert_eq!(b("tavern").sells, vec![res("beer")]);
    assert!(b("storehouse").storage.as_ref().unwrap().accepts.accepts(ResourceCategory::Food));
    assert!(!b("storehouse").storage.as_ref().unwrap().accepts.accepts(ResourceCategory::Goods));

    let paved = d.road_by_key("paved").unwrap();
    assert_eq!(paved.speed, 1.4);
    assert_eq!(paved.cost_per_tile.gold, 4);
    assert_eq!(d.road_by_key("bridge").unwrap().bridge.unwrap().max_span, 6);

    let rules = d.rules();
    assert_eq!(rules.tool_wear_cycles, 20);
    assert_eq!(rules.max_tax_rate_percent, 30);
    assert_eq!(*rules.season_growth.get(Season::Summer), 1.2);
    assert_eq!(*rules.season_growth.get(Season::Winter), 0.0);
}

#[test]
fn unlocks_are_derived_from_technologies() {
    let d = defs();
    let tech = |k: &str| d.tech_id(k).unwrap();
    let b = |k: &str| d.building_by_key(k).unwrap();
    for start in [
        "house_wood",
        "woodcutter",
        "forester",
        "sawmill",
        "quarry",
        "clay_pit",
        "well",
        "market",
        "warehouse",
        "storehouse",
    ] {
        assert_eq!(b(start).unlocked_by, None, "{start} must be available from the start");
    }
    assert_eq!(b("wheat_farm").unlocked_by, Some(tech("grain_farming")));
    assert_eq!(b("townhouse").unlocked_by, Some(tech("stone_masonry")));
    assert_eq!(b("church").unlocked_by, Some(tech("clergy")));
    assert_eq!(b("hospital").unlocked_by, Some(tech("medicine")));
    assert_eq!(d.road_by_key("path").unwrap().unlocked_by, None);
    assert_eq!(d.road_by_key("gravel").unwrap().unlocked_by, None);
    assert_eq!(d.road_by_key("bridge").unwrap().unlocked_by, None);
    assert_eq!(d.road_by_key("road").unwrap().unlocked_by, Some(tech("architecture_1")));
    assert_eq!(d.road_by_key("paved").unwrap().unlocked_by, Some(tech("stone_masonry")));
    assert_eq!(d.road_by_key("trade_road").unwrap().unlocked_by, Some(tech("trade_agreements")));
    assert!(d.units().iter().all(|u| u.unlocked_by.is_some()), "all units need research");
    let trains: Vec<&str> = b("barracks").trains.iter().map(|u| d.unit(*u).key.as_str()).collect();
    assert_eq!(trains, ["militia", "swordsman", "spearman", "siege"]);
    assert_eq!(d.unit_by_key("cavalry").unwrap().support.unwrap().building, d.building_id("cattle_farm").unwrap());

    let start: Vec<&str> = d.start_buildings().map(|b| b.key.as_str()).collect();
    assert!(start.contains(&"house_wood") && start.contains(&"market"));
    assert!(!start.contains(&"farmhouse"), "upgrade-only tiers are not placeable");
    assert!(!start.contains(&"church"));
}

#[test]
fn housing_chain_is_complete_and_cumulative() {
    let d = defs();
    let mut current = d.building_by_key("house_wood").unwrap();
    let mut keys = vec![current.key.as_str()];
    while let Some(next) = current.upgrade.as_ref().map(|u| u.to) {
        let next_def = d.building(next);
        let (h, nh) = (current.housing.as_ref().unwrap(), next_def.housing.as_ref().unwrap());
        assert_eq!(nh.tier, h.tier + 1);
        assert!(nh.requirements.includes(&h.requirements));
        current = next_def;
        keys.push(current.key.as_str());
    }
    assert_eq!(keys, ["house_wood", "farmhouse", "citizen_house", "townhouse", "residence", "manor"]);
    let manor = d.building_by_key("manor").unwrap().housing.as_ref().unwrap();
    let silk_or_spices =
        GoodsRequirement { any_of: vec![d.resource_id("spices").unwrap(), d.resource_id("silk").unwrap()] };
    assert!(manor.requirements.goods.contains(&silk_or_spices));
    assert_eq!(manor.class, SocialClass::Noble);
}

#[test]
fn default_scenario_matches_design() {
    let d = defs();
    let s = d.scenario(d.default_scenario());
    assert_eq!((s.settlers, s.families, s.gold), (12, 4, 500));
    assert_eq!(d.building(s.start_buildings[0].building).key, "town_hall");
    let houses = s.start_buildings.iter().filter(|b| d.building(b.building).key == "house_wood").count();
    assert_eq!(houses, 2);
    let stock = |k: &str| s.stock.iter().find(|a| a.resource == d.resource_id(k).unwrap()).map(|a| a.amount);
    assert_eq!(stock("wood"), Some(60));
    assert_eq!(stock("stone"), Some(30));
    assert_eq!(stock("planks"), Some(20));
    assert_eq!(stock("bread"), Some(60));
    assert_eq!(stock("vegetables"), Some(20));
    assert_eq!(stock("tools"), Some(10));
    assert!(s.researched.is_empty());
}

#[test]
fn events_cover_the_design_list() {
    let d = defs();
    let keys: Vec<&str> = d.events().iter().map(|e| e.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "bad_harvest",
            "drought",
            "flood",
            "plague",
            "fire",
            "bandit_raid",
            "merchant_visit",
            "refugees",
            "war_refugees",
            "religious_festival",
            "political_unrest",
            "bountiful_harvest",
            "new_deposit",
            "storm_damage"
        ]
    );
    let plague = d.event_by_key("plague").unwrap();
    assert!(plague.conditions.contains(&EventCondition::LacksBuildings(vec![d.building_id("hospital").unwrap()])));
    assert_eq!(plague.risk, Some(crate::defs::EventRisk::Disease));
    assert_eq!(d.event_by_key("fire").unwrap().risk, Some(crate::defs::EventRisk::Fire));
    let storm = d.event_by_key("storm_damage").unwrap();
    assert!(storm.conditions.contains(&EventCondition::Weather(crate::defs::Weather::Storm)));
    assert!(matches!(
        storm.choices[0].effects[0],
        crate::defs::EventEffect::DamageBuildings { cause: crate::defs::DamageCause::Storm, .. }
    ));
    assert!(d.events().iter().all(|e| e.choices[0].requires.is_empty()));
}

#[test]
fn fingerprint_is_stable_and_id_sensitive() {
    let a = Defs::from_files(&shipped_files()).unwrap();
    assert_eq!(a.fingerprint(), defs().fingerprint());
    // Balancing changes keep the fingerprint ...
    let rebalanced =
        Defs::from_files(&edited("resources/resources.ron", "base_price: 120", "base_price: 125")).unwrap();
    assert_eq!(rebalanced.fingerprint(), a.fingerprint());
    // ... renaming an id changes it.
    let renamed =
        Defs::from_files(&edited("events/events.ron", "id: \"new_deposit\"", "id: \"new_deposits\"")).unwrap();
    assert_ne!(renamed.fingerprint(), a.fingerprint());
}

#[test]
fn lookups_by_key() {
    let d = defs();
    let id = d.building_id("sawmill").unwrap();
    assert_eq!(d.building(id).key, "sawmill");
    assert_eq!(d.building_by_key("sawmill").map(|b| b.id), Some(id));
    assert!(d.building_id("nonexistent").is_none());
    assert!(d.resource_by_key("").is_none());
    assert_eq!(d.buildings_in(BuildingCategory::Housing).count(), 6);
}

// ---- broken data -------------------------------------------------------------------------

#[test]
fn unknown_reference_is_reported_with_file_and_item() {
    let files = edited(
        "buildings/buildings.ron",
        "inputs: {\"wood\": 2.0}, outputs: {\"planks\": 3.0}, hours: 2.0",
        "inputs: {\"wod\": 2.0}, outputs: {\"planks\": 3.0}, hours: 2.0",
    );
    let err = load_err(&files);
    assert_problem(&err, "buildings/buildings.ron", "building 'sawmill'", "unknown resource 'wod'");
}

#[test]
fn all_problems_are_collected_in_one_error() {
    let mut files = edited("units.ron", "trained_at: \"stables\"", "trained_at: \"stable\"");
    let tech = files.iter_mut().find(|f| f.path == "technologies/technologies.ron").unwrap();
    tech.text = tech.text.replacen("prerequisites: [\"mining\"]", "prerequisites: [\"minning\"]", 1);
    tech.text = tech.text.replacen("UnlockBuilding(\"orchard\")", "UnlockBuilding(\"orchards\")", 1);
    let crops = files.iter_mut().find(|f| f.path == "crops.ron").unwrap();
    crops.text = crops.text.replacen("output: \"grain\"", "output: \"grains\"", 1);
    let err = load_err(&files);
    assert_problem(&err, "units.ron", "unit 'cavalry'", "unknown building 'stable'");
    assert_problem(&err, "technologies/technologies.ron", "technology 'smelting'", "unknown technology 'minning'");
    assert_problem(
        &err,
        "technologies/technologies.ron",
        "technology 'orchards_and_bees'",
        "unknown building 'orchards'",
    );
    assert_problem(&err, "crops.ron", "crop 'barley'", "unknown resource 'grains'");
    assert!(err.problems().len() >= 4);
}

#[test]
fn parse_errors_and_unknown_fields_are_reported() {
    let err = load_err(&edited("roads.ron", "speed: 1.4", "sped: 1.4"));
    assert!(err.problems().iter().any(|p| p.file == "roads.ron" && p.message.contains("parse error")), "{err}");
    let err = load_err(&edited("crops.ron", "growth_days: 14.0,", "growth_days: 14.0,,"));
    assert!(err.problems().iter().any(|p| p.file == "crops.ron"), "{err}");
}

#[test]
fn duplicate_ids_are_reported() {
    let err = load_err(&edited("resources/resources.ron", "(id: \"loam\",", "(id: \"stone\","));
    assert_problem(&err, "resources/resources.ron", "resource 'stone'", "duplicate id");
}

#[test]
fn malformed_ids_are_reported() {
    let err = load_err(&edited("roads.ron", "(id: \"gravel\",", "(id: \"Gravel\","));
    assert_problem(&err, "roads.ron", "road 'Gravel'", "lower_snake_case");
}

#[test]
fn tech_cycles_are_reported() {
    let err = load_err(&edited(
        "technologies/technologies.ron",
        "id: \"grain_farming\", name: \"Getreideanbau\", area: Agriculture, cost_gold: 60, research_points: 24,",
        "id: \"grain_farming\", name: \"Getreideanbau\", area: Agriculture, cost_gold: 60, research_points: 24, prerequisites: [\"irrigation\"],",
    ));
    assert!(err.problems().iter().any(|p| p.message.contains("cycle")), "{err}");
}

#[test]
fn double_unlocks_are_reported() {
    let err = load_err(&edited(
        "technologies/technologies.ron",
        "effects: [UnlockBuilding(\"winery\")]",
        "effects: [UnlockBuilding(\"winery\"), UnlockBuilding(\"mill\")]",
    ));
    assert_problem(&err, "technologies/technologies.ron", "technology 'winemaking'", "already unlocked");
}

#[test]
fn value_ranges_are_checked() {
    let err = load_err(&edited("units.ron", "speed: 1.0, morale: 50,", "speed: 1.0, morale: 0,"));
    assert_problem(&err, "units.ron", "unit 'militia'", "morale");
    let err = load_err(&edited(
        "buildings/buildings.ron",
        "build_hours: 1.0,\n        services",
        "build_hours: 0.0,\n        services",
    ));
    assert_problem(&err, "buildings/buildings.ron", "building 'well'", "build_hours");
    // Buildings that shoot are counted in the raid warning and need a plural.
    let err = load_err(&edited("buildings/buildings.ron", "name: \"Wachturm\", plural: \"Wachtürme\",", "name: \"Wachturm\","));
    assert_problem(&err, "buildings/buildings.ron", "building 'watchtower'", "plural");
    let err = load_err(&edited("rules.ron", "max_tax_rate_percent: 30", "max_tax_rate_percent: 130"));
    assert!(
        err.problems().iter().any(|p| p.file == "rules.ron" && p.message.contains("max_tax_rate_percent")),
        "{err}"
    );
}

#[test]
fn production_coverage_is_checked() {
    // Removing the only producer of cheese leaves it without a source.
    let err = load_err(&edited("buildings/buildings.ron", "outputs: {\"cheese\": 1.0}", "outputs: {\"meat\": 1.0}"));
    assert!(err.problems().iter().any(|p| p.item == "resource 'cheese'" && p.message.contains("produces")), "{err}");
    let err = load_err(&edited(
        "buildings/buildings.ron",
        "outputs: {\"art\": 1.0}",
        "outputs: {\"art\": 1.0, \"silk\": 1.0}",
    ));
    assert!(err.problems().iter().any(|p| p.item == "resource 'silk'" && p.message.contains("import-only")), "{err}");
}

#[test]
fn housing_chain_errors_are_reported() {
    let err = load_err(&edited("buildings/buildings.ron", "capacity: 7, tier: 2,", "capacity: 7, tier: 3,"));
    assert_problem(&err, "buildings/buildings.ron", "building 'house_wood'", "must have tier 2");
    let err = load_err(&edited(
        "buildings/buildings.ron",
        "capacity: 9, tier: 3, class: Commoner, tax_per_resident: 20.0, income_per_worker: 48.0,\n            requires: (\n                needs: [Hunger, Thirst], services: [Market, Religion],",
        "capacity: 9, tier: 3, class: Commoner, tax_per_resident: 20.0, income_per_worker: 48.0,\n            requires: (\n                needs: [Hunger, Thirst], services: [Market, Religion, Health],",
    ));
    assert_problem(&err, "buildings/buildings.ron", "building 'citizen_house'", "must include");
}

#[test]
fn upgrade_chains_of_other_buildings() {
    let d = defs();
    let up = |key: &str| d.building_by_key(key).unwrap().upgrade.as_ref().map(|u| d.building(u.to).key.as_str());
    assert_eq!(up("storehouse"), Some("warehouse"));
    assert_eq!(up("town_hall"), Some("castle"));
    assert_eq!(up("watchtower"), Some("stone_tower"));
    for (from, to) in [("sawmill", "large_sawmill"), ("smithy", "master_smithy"), ("mill", "large_mill"), ("bakery", "large_bakery")] {
        assert_eq!(up(from), Some(to));
        let (a, b) = (d.building_by_key(from).unwrap(), d.building_by_key(to).unwrap());
        assert!(!b.buildable && b.unlocked_by.is_some(), "{to} is reached by upgrading after research");
        assert!(b.workers > a.workers);
        let (pa, pb) = (a.production.as_ref().unwrap(), b.production.as_ref().unwrap());
        assert!(pb.buffer_capacity > pa.buffer_capacity);
        assert!(pb.recipes[0].hours < pa.recipes[0].hours, "{to} produces faster");
    }
    // Stage 2 of the other workshops, gatherers, services and military buildings.
    let workshops = [
        ("woodcutter", "lumber_camp"),
        ("quarry", "large_quarry"),
        ("smelter", "blast_furnace"),
        ("brickworks", "large_brickworks"),
        ("pottery", "large_pottery"),
        ("weaver", "large_weaver"),
        ("tailor", "large_tailor"),
        ("tannery", "large_tannery"),
        ("carpenter", "large_carpenter"),
        ("brewery", "large_brewery"),
        ("fishery", "large_fishery"),
        ("apiary", "large_apiary"),
    ];
    for (from, to) in workshops {
        assert_eq!(up(from), Some(to));
        let (a, b) = (d.building_by_key(from).unwrap(), d.building_by_key(to).unwrap());
        assert!(!b.buildable && b.unlocked_by.is_some(), "{to} is reached by upgrading after research");
        assert!(b.workers > a.workers && b.upkeep_per_day > a.upkeep_per_day, "{to}");
        let (pa, pb) = (a.production.as_ref().unwrap(), b.production.as_ref().unwrap());
        assert!(pb.recipes.iter().zip(&pa.recipes).all(|(nb, na)| nb.hours < na.hours), "{to} works faster");
    }
    let services = [("market", "grand_market"), ("church", "cathedral"), ("school", "academy"), ("hospital", "infirmary"), ("tavern", "inn")];
    for (from, to) in services {
        assert_eq!(up(from), Some(to));
        let (a, b) = (d.building_by_key(from).unwrap(), d.building_by_key(to).unwrap());
        assert!(!b.buildable && b.unlocked_by.is_some(), "{to}");
        let radius = |def: &BuildingDef| def.services.iter().map(|s| s.radius).max().unwrap_or(0);
        assert!(radius(b) > radius(a), "{to} serves a wider area");
    }
    assert!(d.building_by_key("academy").unwrap().research_points_per_hour > d.building_by_key("school").unwrap().research_points_per_hour);
    for (from, to) in [("barracks", "garrison"), ("archer_post", "archer_tower"), ("stables", "stud_farm")] {
        assert_eq!(up(from), Some(to));
        let (a, b) = (d.building_by_key(from).unwrap(), d.building_by_key(to).unwrap());
        assert!(b.garrison > a.garrison, "{to}");
        assert!(!a.trains.is_empty() && a.trains.iter().all(|u| b.trains.contains(u)), "{to} keeps training: {:?}", b.trains);
        assert!(b.descends_from(a.id) && b.lineage == vec![a.id]);
    }
    // Research bonuses for a type reach its upgrades (Tiefbau: +25 % in quarries).
    let deep = d.techs().iter().find(|t| t.key == "deep_mining").unwrap();
    let large_quarry = d.building_by_key("large_quarry").unwrap();
    assert!(deep.effects.iter().any(|e| matches!(e, TechEffect::ProductionBonus { target, .. } if target.matches(large_quarry))));
    // An explicit upgrade cost replaces the target's construction cost.
    let castle = d.building_by_key("town_hall").unwrap().upgrade.as_ref().unwrap();
    assert_eq!((castle.cost.gold, castle.hours), (1500, 48.0));
    assert!(d.building_by_key("castle").unwrap().buildable, "a castle can still be built anew");
}

#[test]
fn incompatible_upgrades_are_reported() {
    let err = load_err(&edited("buildings/buildings.ron", "upgrade: (to: \"large_mill\"),", "upgrade: (to: \"warehouse\"),"));
    assert_problem(&err, "buildings/buildings.ron", "building 'mill'", "production must stay production");
    let err = load_err(&edited("buildings/buildings.ron", "upgrade: (to: \"large_mill\"),", "upgrade: (to: \"large_bakery\"),"));
    assert_problem(&err, "buildings/buildings.ron", "building 'mill'", "same recipes");
    let err = load_err(&edited("buildings/buildings.ron", "upgrade: (to: \"stone_tower\"),", "upgrade: (to: \"town_hall\"),"));
    assert_problem(&err, "buildings/buildings.ron", "building 'watchtower'", "same role");
    let err = load_err(&edited("buildings/buildings.ron", "upgrade: (to: \"warehouse\", cost:", "upgrade: (to: \"well\", cost:"));
    assert_problem(&err, "buildings/buildings.ron", "building 'storehouse'", "a storage must stay a storage");
    let err = load_err(&edited("buildings/buildings.ron", "upgrade: (to: \"stone_tower\"),", "upgrade: (to: \"watchtower\"),"));
    assert_problem(&err, "buildings/buildings.ron", "building 'watchtower'", "cycle");
    let err = load_err(&edited("buildings/buildings.ron", "upgrade: (to: \"farmhouse\"),", "upgrade: (to: \"farmhous\"),"));
    assert!(err.problems().iter().any(|p| p.message.contains("farmhous")), "{err}");
}

#[test]
fn scenario_errors_are_reported() {
    let err = load_err(&edited(
        "scenarios/default.ron",
        "(building: \"town_hall\", offset: (-2, -2)),",
        "(building: \"well\", offset: (-2, -2)),",
    ));
    assert_problem(&err, "scenarios/default.ron", "scenario 'default'", "headquarters");
    let err = load_err(&edited("scenarios/default.ron", "\"bread\": 60,", "\"breads\": 60,"));
    assert_problem(&err, "scenarios/default.ron", "scenario 'default'", "unknown resource 'breads'");
    let err = load_err(&edited("scenarios/default.ron", "id: \"default\",", "id: \"standard\","));
    assert!(err.problems().iter().any(|p| p.message.contains("must match the file name")), "{err}");
}

#[test]
fn event_reference_errors_are_reported() {
    let err =
        load_err(&edited("events/events.ron", "LacksBuildings([\"hospital\"])", "LacksBuildings([\"hospitall\"])"));
    assert_problem(&err, "events/events.ron", "event 'plague'", "unknown building 'hospitall'");
    let err = load_err(&edited(
        "events/events.ron",
        "Resource(resource: \"spices\", amount: 5)",
        "Resource(resource: \"spice\", amount: 5)",
    ));
    assert_problem(&err, "events/events.ron", "event 'merchant_visit'", "unknown resource 'spice'");
    let err = load_err(&edited("events/events.ron", "focus_text: \"Im Gebäude „{building}“", "focus_text: \"Im Gebäude"));
    assert_problem(&err, "events/events.ron", "event 'fire'", "{building}");
}

#[test]
fn missing_files_and_directories_are_reported() {
    let files: Vec<DataFile> = shipped_files().into_iter().filter(|f| f.path != "units.ron").collect();
    let err = load_err(&files);
    assert!(err.problems().iter().any(|p| p.file == "units.ron"), "{err}");
    let mut files = shipped_files();
    files.push(DataFile { path: "misc/notes.ron".into(), text: "[]".into() });
    let err = load_err(&files);
    assert!(err.problems().iter().any(|p| p.message.contains("unrecognized")), "{err}");
    assert!(matches!(Defs::load(Path::new("/nonexistent/kingdoms/data")), Err(DefsError::MissingDirectory(_))));
}

#[test]
fn access_any_side_needs_a_single_tile_and_survives_upgrades() {
    let d = defs();
    for key in ["wall", "gate", "watchtower", "stone_tower"] {
        assert_eq!(d.building(d.building_id(key).unwrap()).access, Access::AnySide, "{key}");
    }
    assert_eq!(d.building(d.building_id("house_wood").unwrap()).access, Access::Entrance);
    // A 2×2 house entered from any side is refused.
    let err = load_err(&edited(
        "buildings/buildings.ron",
        r#"id: "house_wood", name: "Einfaches Holzhaus", category: Housing, size: (2, 2),"#,
        r#"id: "house_wood", name: "Einfaches Holzhaus", category: Housing, size: (2, 2), access: AnySide,"#,
    ));
    assert_problem(&err, "buildings/buildings.ron", "building 'house_wood'", "AnySide");
    // The stone tower must keep the access of the watchtower it is built from.
    let err = load_err(&edited(
        "buildings/buildings.ron",
        r#"material: Stone, buildable: false, access: AnySide,"#,
        r#"material: Stone, buildable: false,"#,
    ));
    assert!(err.to_string().contains("needs the same access"), "{err}");
    // The notice thresholds must be positive.
    let err = load_err(&edited("rules.ron", "waiting_resolved_hours: 24", "waiting_resolved_hours: 0"));
    assert!(err.to_string().contains("waiting_resolved_hours"), "{err}");
}
