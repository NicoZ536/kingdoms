//! Developer benchmark town with a large population (`KINGDOMS_DEV_CROWD=<settlers>`), to
//! measure the frame cost of many walkers.
//!
//! Nothing is faked: the shipped game data is read and an extra scenario `dev_crowd` is added
//! in memory ([`Defs::from_files`]); the simulation starts it like any scenario – the start
//! buildings are placed finished near their preferred spots (on the headquarters' land) and
//! connected to it by free start paths, the settlers move into the houses. The town is a
//! dense grid of 2×2 buildings with one-tile streets around a castle (the headquarters with the
//! widest land, moved towards the map's interior): town houses for the settlers (couples, no
//! children), woodcutters and food stores whose carriers walk, plus food, firewood and gold for
//! a few days. In the evening every household's errand runner walks to its store. The data
//! with the extra scenario has its own fingerprint, so its saves only load in the same mode.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context as _;
use glam::IVec2;
use kingdoms_sim::defs::{DataFile, Defs};

/// Key (and file name) of the benchmark scenario.
pub const SCENARIO_KEY: &str = "dev_crowd";
/// Settlers allowed.
pub const SETTLERS: std::ops::RangeInclusive<u16> = 50..=6000;
/// Grid cell edge in tiles: a 2×2 building plus a one-tile street.
const CELL: i32 = 3;
/// Residents of a town house (`townhouse` in data/buildings).
const HOUSE_CAPACITY: u32 = 12;
/// Innermost ring of cells (the rings inside belong to the castle, 5×5 tiles).
const FIRST_RING: i32 = 2;
/// Outermost ring: the castle claims land within 26 tiles, start buildings must lie on it.
const LAST_RING: i32 = 8;

/// What stands in grid cell `k`: of every 10 cells 7 are houses (until all settlers have a
/// place), 2 woodcutters and 1 food store with a carrier.
fn cell_building(k: usize, houses_left: &mut u32) -> &'static str {
    match k % 10 {
        0..=6 if *houses_left > 0 => {
            *houses_left -= 1;
            "townhouse"
        }
        9 => "storehouse",
        _ => "woodcutter",
    }
}

/// Grid cells in square rings around the centre, from [`FIRST_RING`] outwards (ring by ring,
/// row-major within a ring; deterministic), at most up to [`LAST_RING`].
fn ring_cells(count: usize) -> Vec<IVec2> {
    let mut cells = Vec::with_capacity(count);
    let mut ring = FIRST_RING;
    while cells.len() < count && ring <= LAST_RING {
        for dy in -ring..=ring {
            for dx in -ring..=ring {
                if dx.abs().max(dy.abs()) == ring && cells.len() < count {
                    cells.push(IVec2::new(dx, dy));
                }
            }
        }
        ring += 1;
    }
    cells
}

/// The RON text of the benchmark scenario for `settlers` on a map of `map_size` tiles whose
/// start point is `start`.
pub fn scenario_text(settlers: u16, start: IVec2, map_size: IVec2) -> String {
    let settlers = settlers.clamp(*SETTLERS.start(), *SETTLERS.end());
    let mut houses = u32::from(settlers).div_ceil(HOUSE_CAPACITY);
    let cells = ring_cells((houses as usize * 10).div_ceil(7));
    // Keep the town on the map: move it towards the interior (the castle may stand up to 24
    // tiles from its preferred spot).
    let half = (LAST_RING + 1) * CELL;
    let clamp = |v: i32, size: i32| v.clamp(half, (size - half).max(half));
    let shift = (IVec2::new(clamp(start.x, map_size.x), clamp(start.y, map_size.y)) - start).clamp(IVec2::splat(-24), IVec2::splat(24));
    let mut text = String::new();
    text.push_str("// Developer benchmark town, generated in memory by KINGDOMS_DEV_CROWD.\n#![enable(implicit_some)]\n(\n");
    let _ = writeln!(text, "    id: \"{SCENARIO_KEY}\",");
    text.push_str("    name: \"Testlauf: große Stadt\",\n");
    text.push_str("    description: \"Entwickler-Messung mit vielen Bewohnern.\",\n");
    text.push_str("    start_buildings: [\n");
    let _ = writeln!(text, "        (building: \"castle\", offset: ({}, {})),", shift.x - 2, shift.y - 2);
    for (k, cell) in cells.iter().enumerate() {
        let building = cell_building(k, &mut houses);
        let at = shift + *cell * CELL - IVec2::ONE;
        let _ = writeln!(text, "        (building: \"{building}\", offset: ({}, {})),", at.x, at.y);
    }
    text.push_str("    ],\n");
    let _ = writeln!(text, "    settlers: {settlers},");
    let _ = writeln!(text, "    families: {},", settlers / 2);
    let food = u32::from(settlers) * 2;
    let _ = writeln!(
        text,
        "    stock: {{ \"bread\": {food}, \"vegetables\": {food}, \"meat\": {food}, \"wood\": 300, \"planks\": 200, \"stone\": 200, \"tools\": 100 }},"
    );
    text.push_str("    gold: 500000,\n    researched: [],\n    tax_rate_percent: 10,\n    rival_kingdoms: 3,\n)\n");
    text
}

/// Reads every data file below `dir` like the simulation's loader (sorted, deterministic).
pub(super) fn read_data_files(dir: &Path) -> anyhow::Result<Vec<DataFile>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<DataFile>) -> anyhow::Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .with_context(|| format!("cannot list {}", dir.display()))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(root, &path, out)?;
            } else if path.extension().is_some_and(|e| e == "ron") {
                let rel = path.strip_prefix(root).with_context(|| format!("{} outside {}", path.display(), root.display()))?;
                let rel = rel.to_string_lossy().replace('\\', "/");
                let text = std::fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
                out.push(DataFile { path: rel, text });
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    Ok(files)
}

/// The shipped game data plus the benchmark scenario for `settlers`.
pub fn crowd_defs(data_dir: &Path, settlers: u16, start: IVec2, map_size: IVec2) -> anyhow::Result<Defs> {
    let mut files = read_data_files(data_dir)?;
    files.push(DataFile { path: format!("scenarios/{SCENARIO_KEY}.ron"), text: scenario_text(settlers, start, map_size) });
    Defs::from_files(&files).map_err(|e| anyhow::anyhow!("benchmark scenario rejected: {e}"))
}

#[cfg(test)]
mod tests {
    use kingdoms_sim::state::{GameSettings, GameState};
    use kingdoms_sim::world::WorldSize;

    use super::*;

    #[test]
    fn rings_and_cell_mix() {
        let cells = ring_cells(24);
        assert_eq!(cells.len(), 24);
        assert!(cells[..16].iter().all(|c| c.x.abs().max(c.y.abs()) == FIRST_RING));
        assert!(cells.iter().all(|c| c.x.abs().max(c.y.abs()) >= FIRST_RING), "the centre is the castle's");
        assert!(ring_cells(100_000).len() < 400, "the town stays on the castle's land");
        let mut houses = 3;
        let kinds: Vec<&str> = (0..10).map(|k| cell_building(k, &mut houses)).collect();
        assert_eq!(&kinds[..4], ["townhouse", "townhouse", "townhouse", "woodcutter"]);
        assert_eq!(kinds[9], "storehouse");
    }

    #[test]
    fn the_benchmark_town_starts_with_its_settlers() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match Defs::load(&dir) {
            Ok(d) => d,
            Err(e) => panic!("shipped data: {e}"),
        };
        let probe = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let crowd = match crowd_defs(&dir, 240, probe.world().start, probe.world().size()) {
            Ok(d) => d,
            Err(e) => panic!("{e}"),
        };
        let Some(scenario) = crowd.scenario_id(SCENARIO_KEY) else { panic!("scenario missing") };
        let mut settings = GameSettings::new(&crowd, 7, WorldSize::Small);
        settings.scenario = scenario;
        let state = GameState::new(&crowd, settings);
        assert_eq!(state.population().len(), 240);
        let houses = state.population().citizens().filter(|(_, c)| state.population().household(c.household()).is_some_and(|h| h.home().is_some())).count();
        assert!(houses >= 200, "{houses} of 240 settlers housed");
        // The shipped scenario is untouched.
        assert_eq!(crowd.default_scenario(), defs.default_scenario());
    }
}
