//! Developer showcase of every building (`KINGDOMS_DEV_SHOWCASE=1`): one finished, staffed
//! instance of each building type around a castle, to check the art and the animations of
//! the buildings and their workers on the map (mill sails, forge fires, gatherer tools).
//!
//! Nothing is faked: like `dev_crowd`, the shipped game data is read and an extra scenario
//! `dev_showcase` is added in memory ([`Defs::from_files`]); the simulation starts it like any
//! scenario – the castle as headquarters, then every other building type (production and
//! farms first, then public and military buildings, then one house of every level and as many
//! town houses as the workers need) on a grid of cells in rings around it. The simulation
//! moves a building that does not fit its cell (water, cliffs, deposits for mines and pits,
//! water for the fishery) to the nearest valid spot on the castle's land and connects every
//! entrance by a free start path; a type without any valid spot is left out (logged). Every
//! technology is researched, the settlers (couples, all adults, as many as there are jobs)
//! move into the houses and take the jobs, the castle's storage holds the inputs of every
//! recipe, the goods the taverns sell, food and firewood. Rival kingdoms and random events are off, so no dialog covers the
//! view. The data with the extra scenario has its own fingerprint, so its saves only load in
//! the same mode.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use glam::IVec2;
use kingdoms_sim::defs::{BuildingCategory, BuildingDef, BuildingRole, DataFile, Defs};

/// Key (and file name) of the showcase scenario.
pub const SCENARIO_KEY: &str = "dev_showcase";
/// Grid cell edge in tiles: up to a 4×4 building plus a street.
const CELL: i32 = 5;
/// Rings of cells around the castle (ring 5 reaches 27 tiles: the corner cells lie beyond the
/// castle's land of 26, the simulation moves their buildings onto the land).
const LAST_RING: i32 = 5;
/// Residents of a town house (`townhouse` in data/buildings), the filler house.
const FILLER_HOUSE: &str = "townhouse";
/// Units of every recipe input in the start stock (the castle stores 300 of each).
const INPUT_STOCK: u32 = 80;

/// Order in which building types are laid out: production and farms in the inner rings, then
/// public and military buildings, houses outside.
fn layout_rank(def: &BuildingDef) -> u8 {
    match def.category {
        BuildingCategory::Production => 0,
        BuildingCategory::Agriculture => 1,
        BuildingCategory::Public => 2,
        BuildingCategory::Military => 3,
        BuildingCategory::Housing => 4,
    }
}

/// True if a building type belongs in the showcase: everything except headquarters (the
/// castle stands in the centre) and wall pieces (they need a wall line).
pub(crate) fn shown(def: &BuildingDef) -> bool {
    !matches!(def.role, BuildingRole::Headquarters | BuildingRole::WallSegment | BuildingRole::Gate)
}

/// Cells in square rings around the centre from ring 1 outwards (ring by ring, row-major within
/// a ring; deterministic), at most [`LAST_RING`].
fn ring_cells() -> Vec<IVec2> {
    (1..=LAST_RING)
        .flat_map(|ring| (-ring..=ring).flat_map(move |dy| (-ring..=ring).map(move |dx| IVec2::new(dx, dy))).filter(move |c| c.x.abs().max(c.y.abs()) == ring))
        .collect()
}

/// The buildings of the showcase in layout order, with the settlers they employ: every shown
/// type once, then town houses until every worker has a bed (couples: an even number).
fn showcase_buildings(defs: &Defs) -> (Vec<&BuildingDef>, u16) {
    let mut types: Vec<&BuildingDef> = defs.buildings().iter().filter(|d| shown(d)).collect();
    types.sort_by_key(|d| (layout_rank(d), d.id));
    let workers: u32 = types.iter().map(|d| u32::from(d.workers)).sum();
    let settlers = workers.div_ceil(2) * 2;
    let beds: u32 = types.iter().filter_map(|d| d.housing.as_ref()).map(|h| u32::from(h.capacity)).sum();
    if let Some(filler) = defs.buildings().iter().find(|d| d.key == FILLER_HOUSE)
        && let Some(capacity) = filler.housing.as_ref().map(|h| u32::from(h.capacity)).filter(|c| *c > 0)
    {
        for _ in 0..settlers.saturating_sub(beds).div_ceil(capacity) {
            types.push(filler);
        }
    }
    (types, u16::try_from(settlers).unwrap_or(u16::MAX))
}

/// The start stock: the inputs of every recipe, the goods taverns sell, food for the settlers
/// and firewood.
fn start_stock(defs: &Defs, settlers: u16) -> BTreeMap<String, u32> {
    let mut stock = BTreeMap::new();
    for def in defs.buildings() {
        for sold in &def.sells {
            if let Some(r) = defs.resources().get(sold.index()) {
                stock.insert(r.key.clone(), INPUT_STOCK);
            }
        }
        for recipe in def.production.iter().flat_map(|p| &p.recipes) {
            for input in &recipe.inputs {
                if let Some(r) = defs.resources().get(input.resource.index()) {
                    stock.insert(r.key.clone(), INPUT_STOCK);
                }
            }
        }
    }
    let food = (u32::from(settlers) * 2).min(300);
    for key in ["bread", "vegetables", "meat", "fish"] {
        stock.insert(key.to_owned(), food);
    }
    for (key, amount) in [("wood", 300), ("planks", 200), ("stone", 200), ("tools", 100)] {
        stock.insert(key.to_owned(), amount);
    }
    stock.retain(|key, _| defs.resource_id(key).is_some());
    stock
}

/// The RON text of the showcase scenario for the shipped `defs` on a map of `map_size` tiles
/// whose start point is `start`.
pub fn scenario_text(defs: &Defs, start: IVec2, map_size: IVec2) -> String {
    let (buildings, settlers) = showcase_buildings(defs);
    let cells = ring_cells();
    // Keep the showcase on the map: move it towards the interior (the castle may stand up to
    // 24 tiles from its preferred spot).
    let half = (LAST_RING + 1) * CELL;
    let clamp = |v: i32, size: i32| v.clamp(half, (size - half).max(half));
    let shift = (IVec2::new(clamp(start.x, map_size.x), clamp(start.y, map_size.y)) - start).clamp(IVec2::splat(-24), IVec2::splat(24));
    let mut text = String::new();
    text.push_str("// Developer showcase of every building, generated in memory by KINGDOMS_DEV_SHOWCASE.\n#![enable(implicit_some)]\n(\n");
    let _ = writeln!(text, "    id: \"{SCENARIO_KEY}\",");
    text.push_str("    name: \"Testlauf: alle Gebäude\",\n");
    text.push_str("    description: \"Entwickler-Ansicht mit jedem Gebäude, fertig und besetzt.\",\n");
    text.push_str("    start_buildings: [\n");
    let _ = writeln!(text, "        (building: \"castle\", offset: ({}, {})),", shift.x - 2, shift.y - 2);
    for (def, cell) in buildings.iter().zip(&cells) {
        // Centred in its cell.
        let at = shift + *cell * CELL - def.size / 2;
        let _ = writeln!(text, "        (building: \"{}\", offset: ({}, {})),", def.key, at.x, at.y);
    }
    if buildings.len() > cells.len() {
        log::warn!("KINGDOMS_DEV_SHOWCASE: {} buildings do not fit the {} cells", buildings.len() - cells.len(), cells.len());
    }
    text.push_str("    ],\n");
    let _ = writeln!(text, "    settlers: {settlers},");
    let _ = writeln!(text, "    families: {},", settlers / 2);
    text.push_str("    stock: {\n");
    for (key, amount) in start_stock(defs, settlers) {
        let _ = writeln!(text, "        \"{key}\": {amount},");
    }
    text.push_str("    },\n    gold: 50000,\n    researched: [\n");
    for tech in defs.techs() {
        let _ = writeln!(text, "        \"{}\",", tech.key);
    }
    text.push_str("    ],\n    tax_rate_percent: 10,\n    rival_kingdoms: 0,\n)\n");
    text
}

/// The shipped game data (`defs`, read from `data_dir`) plus the showcase scenario.
pub fn showcase_defs(defs: &Defs, data_dir: &Path, start: IVec2, map_size: IVec2) -> anyhow::Result<Defs> {
    let mut files = super::dev_crowd::read_data_files(data_dir)?;
    files.push(DataFile { path: format!("scenarios/{SCENARIO_KEY}.ron"), text: scenario_text(defs, start, map_size) });
    Defs::from_files(&files).map_err(|e| anyhow::anyhow!("showcase scenario rejected: {e}"))
}

#[cfg(test)]
mod tests {
    use kingdoms_sim::state::{GameSettings, GameState};
    use kingdoms_sim::world::WorldSize;

    use super::*;

    fn shipped() -> (Defs, std::path::PathBuf) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(d) => (d, dir),
            Err(e) => panic!("shipped data: {e}"),
        }
    }

    #[test]
    fn rings_hold_every_building_type_and_enough_houses() {
        let (defs, _) = shipped();
        let cells = ring_cells();
        assert_eq!(cells.len(), 8 + 16 + 24 + 32 + 40);
        assert!(cells.iter().all(|c| (1..=LAST_RING).contains(&c.x.abs().max(c.y.abs()))));
        let (buildings, settlers) = showcase_buildings(&defs);
        assert!(buildings.len() <= cells.len(), "{} buildings for {} cells", buildings.len(), cells.len());
        for def in defs.buildings().iter().filter(|d| shown(d)) {
            assert!(buildings.iter().any(|b| b.id == def.id), "{} is missing", def.key);
        }
        assert!(!buildings.iter().any(|b| b.key == "castle" || b.key == "wall" || b.key == "gate" || b.key == "town_hall"));
        let beds: u32 = buildings.iter().filter_map(|d| d.housing.as_ref()).map(|h| u32::from(h.capacity)).sum();
        let jobs: u32 = buildings.iter().map(|d| u32::from(d.workers)).sum();
        assert!(u32::from(settlers) >= jobs && beds >= u32::from(settlers), "{jobs} jobs, {settlers} settlers, {beds} beds");
        // Production first.
        assert_eq!(buildings[0].category, BuildingCategory::Production);
    }

    #[test]
    fn the_showcase_starts_with_finished_staffed_workshops() {
        let (defs, dir) = shipped();
        let probe = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Medium));
        let showcase = match showcase_defs(&defs, &dir, probe.world().start, probe.world().size()) {
            Ok(d) => d,
            Err(e) => panic!("{e}"),
        };
        let Some(scenario) = showcase.scenario_id(SCENARIO_KEY) else { panic!("scenario missing") };
        let mut settings = GameSettings::new(&showcase, 7, WorldSize::Medium);
        settings.scenario = scenario;
        settings.events_enabled = false;
        let state = GameState::new(&showcase, settings);
        let placed = |key: &str| {
            let Some(kind) = showcase.building_id(key) else { return false };
            state.buildings().iter().any(|(_, b)| b.kind() == kind && b.is_complete())
        };
        for key in ["mill", "large_mill", "smithy", "master_smithy", "smelter", "sawmill", "woodcutter", "bakery", "church", "manor"] {
            assert!(placed(key), "{key} not placed");
        }
        let staffed = state.buildings().iter().filter(|(_, b)| b.staffed() > 0).count();
        assert!(staffed >= 40, "only {staffed} buildings staffed");
        // The shipped scenario is untouched.
        assert_eq!(showcase.default_scenario(), defs.default_scenario());
    }
}
