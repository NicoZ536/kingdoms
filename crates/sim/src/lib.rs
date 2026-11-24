//! # KINGDOMS simulation
//!
//! The complete, deterministic game simulation. It has no graphics, windowing or audio
//! dependencies, so it can be tested headless and run identically on every platform.
//!
//! ## Structure
//!
//! | Module | Content |
//! |---|---|
//! | [`world`] | tile map, terrain, biomes, vegetation, deposits, 10-stage world generator |
//! | [`ids`] | generational [`Arena`](ids::Arena) with typed [`Id`](ids::Id)s for entities |
//! | [`time`] | calendar: ticks → minute/hour/day/season/year ([`GameClock`](time::GameClock)) |
//! | [`defs`] | data definitions from `data/**.ron`, resolved to typed ids and validated ([`Defs`](defs::Defs)) |
//! | [`command`] | player/AI intents applied at the start of a tick ([`Command`](command::Command)) |
//! | [`event`] | events emitted by a tick for UI/audio ([`SimEvent`](event::SimEvent)) |
//! | [`events`] | random events with choices: draw, pending decision, consequences, modifiers |
//! | [`diplomacy`] | AI kingdoms (territory, settlements, economy), relations, treaties, wars, caravans |
//! | [`military`] | soldiers, companies and orders, bandits and AI armies, combat, campaigns and sieges |
//! | [`ranks`] | ruler ranks with their requirements, defeat (game over) |
//! | [`policy`] | player policies (tax rate, job priorities) |
//! | [`grid`] | tile geometry: directions, rectangles, building rotation |
//! | [`buildings`] | building entities, occupancy grid, placement rules, construction, upgrades, moves |
//! | [`roads`] | road grid, auto-connection masks, bridges, connectivity, road planner (A*) |
//! | [`territory`] | owned tiles claimed by headquarters and watchtowers |
//! | [`economy`] | storage inventories, kingdom stock view, treasury, ledger, prices, finance, trade |
//! | [`production`] | recipes, buffers, work cycles, productivity, queues, gathering |
//! | [`logistics`] | transport job queue and abstract carriers (storage workers; walking carriers later) |
//! | [`nature`] | tree growth, stumps and fish regeneration |
//! | [`navigation`] | walk-cost grid, A* pathfinding with integer costs, path cache (walkers) |
//! | [`agriculture`] | farms, exclusive field/pasture claims, crops, growth stages, sowing, harvest, frost, soil |
//! | [`population`] | citizens, households, homes: needs, happiness, supply, demography, house tiers, walking (positions, daily routine, spatial index) |
//! | [`report`] | read-only summaries for the UI windows (economy, production, finance, population) |
//! | [`research`] | research queue and progress, researched technologies and their effects |
//! | [`workforce`] | job allocation of citizens to workplaces and sites, housing occupancy, counts |
//! | [`state`] | the whole state ([`GameState`](state::GameState)) and the tick pipeline |
//! | [`statistics`] | pipeline step 10: hourly samples and the daily closing |
//! | [`weather`] | daily weather, its effects (growth, outdoor work, heating, disease), snow cover |
//! | [`save`] | versioned save games: bincode 2 + CRC32, migrations, atomic files |
//!
//! ## Usage
//!
//! ```no_run
//! use kingdoms_sim::defs::Defs;
//! use kingdoms_sim::state::{GameSettings, GameState};
//! use kingdoms_sim::world::WorldSize;
//!
//! let defs = Defs::load(std::path::Path::new("data")).expect("valid game data");
//! let mut state = GameState::new(&defs, GameSettings::new(&defs, 42, WorldSize::Medium));
//! let events = state.tick(&defs, Vec::new());
//! # let _ = events;
//! ```
//!
//! ## Determinism rules for all simulation code
//! - randomness only from seeded `ChaCha8Rng` instances stored in the state,
//! - no iteration over `HashMap`/`HashSet` where order influences results (use `BTreeMap` / `Vec`),
//! - no wall-clock time; time advances only through fixed ticks,
//! - the tick pipeline order (docs/architecture.md §3.3) is part of the guarantee.

pub mod agriculture;
pub mod buildings;
pub mod command;
pub mod defs;
pub mod diplomacy;
pub mod economy;
pub mod event;
pub mod events;
pub mod grid;
pub mod ids;
pub mod logistics;
pub mod military;
pub mod nature;
pub mod navigation;
pub mod policy;
pub mod population;
pub mod production;
pub mod ranks;
pub mod report;
pub mod research;
pub mod roads;
pub mod save;
mod start;
pub mod state;
pub mod statistics;
pub mod territory;
pub mod time;
pub mod weather;
pub mod workforce;
pub mod world;

#[cfg(test)]
mod polish_tests;
#[cfg(test)]
mod scale_tests;

#[cfg(test)]
pub(crate) mod test_support {
    //! Shared fixtures for unit tests.

    use std::path::Path;
    use std::sync::OnceLock;

    use crate::defs::Defs;

    /// Path of the shipped game data.
    pub const DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data");

    /// The shipped game data, loaded once per test binary.
    pub fn defs() -> &'static Defs {
        static DEFS: OnceLock<Defs> = OnceLock::new();
        DEFS.get_or_init(|| match Defs::load(Path::new(DATA_DIR)) {
            Ok(defs) => defs,
            Err(e) => panic!("shipped game data must load: {e}"),
        })
    }

    /// The shipped game data with `edit` applied to the text of `file` (path relative to the
    /// data directory); `edit` returns the new text.
    pub fn defs_edited(file: &str, edit: impl Fn(&str) -> String) -> Defs {
        let mut files = crate::defs::read_dir(Path::new(DATA_DIR)).expect("shipped data readable");
        let entry = files.iter_mut().find(|f| f.path == file).expect("data file exists");
        entry.text = edit(&entry.text);
        Defs::from_files(&files).expect("edited data must load")
    }

    /// A `size` × `size` state on flat grass (elevation 2) without roads; the whole map is
    /// owned, the treasury holds 10 000 gold and 20 educated, single adult citizens live there
    /// (homeless until houses exist; needs at 100). The only building is
    /// the *depot*: a finished warehouse in the bottom-right corner (entrance facing north at
    /// `(size - 2, size - 4)`) filled with 300 of every resource (capacity 400), which forms the
    /// kingdom stock.
    /// Tests shape terrain and territory as needed and connect sites to the depot by road.
    pub fn flat_state(size: i32) -> GameState {
        let defs = defs();
        let tile = Tile {
            terrain: Terrain::Grass,
            biome: Biome::Grassland,
            elevation: 2,
            fertility: 128,
            moisture: 128,
            temperature: 128,
            vegetation: Vegetation::None,
            deposit: None,
            river: false,
        };
        let mut world = WorldMap::new_filled(size, size, 1, tile);
        world.start = IVec2::splat(size / 2);
        // Unit tests of single systems run without the daily event draw.
        let mut settings = GameSettings::new(defs, 1, WorldSize::Small).with_events(false).with_defeat(false);
        settings.scenario = defs.default_scenario();
        let mut economy = Economy::new(defs);
        economy.add_gold(10_000);
        let mut territory = Territory::new(size, size);
        territory.claim(TileRect::new(IVec2::ZERO, IVec2::splat(size)), size as u16);
        let mut state = GameState {
            game_id: crate::state::game_identity(&settings),
            settings,
            world,
            clock: GameClock::new(),
            rng: ChaCha8Rng::seed_from_u64(1),
            weather: crate::weather::WeatherState::fixed(crate::defs::Weather::Clear, 100),
            policy: Policy::new(10),
            economy,
            research: Research::new(),
            events: crate::events::Events::new(1),
            diplomacy: crate::diplomacy::Diplomacy::new(size, size, 1),
            military: crate::military::Military::new(1),
            progress: crate::ranks::Progress::default(),
            buildings: Buildings::new(size, size),
            roads: RoadNetwork::new(size, size),
            territory,
            agriculture: crate::agriculture::Agriculture::default(),
            population: crate::population::Population::default(),
            workforce: Workforce::default(),
            logistics: Logistics::default(),
            statistics: crate::statistics::Statistics::default(),
            pending_commands: Vec::new(),
            navigation: crate::navigation::Navigation::default(),
        };
        let warehouse = defs.building_id("warehouse").expect("warehouse type");
        let depot = crate::buildings::spawn_finished(&mut state, defs, warehouse, IVec2::splat(size - 3), Rotation::new(2).expect("rotation"))
            .expect("depot fits");
        if let Some(b) = state.buildings.get_mut(depot) {
            for r in defs.resources() {
                b.inventory.add(r.id, 300);
            }
        }
        crate::economy::storage::refresh_stock(&mut state, defs);
        set_adults(&mut state, 20);
        state
    }

    /// Adds homeless single adults (educated, age 30, needs and health at 100, happiness 80)
    /// or removes the adults with the highest ids until `n` adults live in the kingdom.
    pub fn set_adults(state: &mut GameState, n: usize) {
        use crate::population::{Activity, Citizen, Household, LifeStage, Needs, Sex, SupplyState};
        let adults: Vec<_> = state.population.citizens().filter(|(_, c)| c.can_work()).map(|(id, _)| id).collect();
        for id in adults.iter().skip(n) {
            crate::population::remove_citizen(state, defs(), *id, &mut Vec::new());
        }
        let names = defs().names();
        for i in adults.len()..n {
            let household = state.population.households.insert(Household {
                family_name: names.family[i % names.family.len()].clone(),
                members: Vec::new(),
                home: None,
                pantry: Vec::new(),
                supply: SupplyState { fed: true, ..SupplyState::default() },
                unhappy_days: 0,
                purse: crate::population::Purse::with_savings(crate::population::wealth::start_savings_milli(defs(), 1)),
            });
            let day = i64::try_from(state.clock.total_days()).expect("day fits");
            let citizen = state.population.citizens.insert(Citizen {
                first_name: names.male[i % names.male.len()].clone(),
                sex: Sex::Male,
                birth_day: day - 30 * crate::time::DAYS_PER_SEASON as i64,
                household,
                partner: None,
                mother: None,
                father: None,
                job: None,
                needs: Needs::uniform(100),
                health: crate::population::MAX_VALUE,
                happiness: 80 * crate::population::CENTI,
                education: 100,
                educated: true,
                activity: Activity::default(),
                stage: LifeStage::Adult,
                last_birth_day: None,
                motion: crate::population::Motion::standing(state.world.start),
                carry: None,
                soldier: None,
                friends: Vec::new(),
                // Fixture workers know every trade (full productivity from the start).
                experience: [u8::MAX; crate::defs::JobCategory::ALL.len()],
                sick_since: None,
                sick_leave: false,
            });
            crate::population::movement::index_insert(state, crate::population::WalkerId::Citizen(citizen));
            state.population.households.get_mut(household).expect("new household").members.push(citizen);
        }
        state.workforce.mark_dirty();
        state.workforce.refresh_counts(&state.population);
    }

    /// The fixture depot of [`flat_state`] (the first storage building).
    pub fn depot(state: &GameState) -> BuildingId {
        let defs = defs();
        state
            .buildings
            .iter()
            .find(|(_, b)| defs.building(b.kind()).key == "warehouse")
            .map(|(id, _)| id)
            .expect("fixture depot")
    }

    /// Spawns a finished building of type `key` at `(x, y)` (rotation 0, entrance south).
    pub fn spawn(state: &mut GameState, key: &str, x: i32, y: i32) -> BuildingId {
        let kind = defs().building_id(key).expect("building type");
        crate::buildings::spawn_finished(state, defs(), kind, IVec2::new(x, y), Rotation::default()).expect("free spot")
    }

    /// Lays a free path from road tile `from` to `to` along the planner's route (panics if
    /// there is none) and refreshes connectivity and the stock view.
    pub fn lay_path(state: &mut GameState, from: IVec2, to: IVec2) {
        let defs = defs();
        let path = defs.road_id("path").expect("path type");
        let plan = crate::roads::plan_road(state, defs, from, to, path);
        assert!(plan.valid, "no path from {from} to {to}: {:?}", plan.problems);
        for t in &plan.tiles {
            if !state.roads.has_road(t.pos) {
                state.roads.set(t.pos, Some(crate::roads::RoadTile { road: t.road, bridge_axis: t.bridge_axis }));
            }
        }
        state.roads.rebuild_components();
        crate::economy::storage::refresh_stock(state, defs);
    }

    /// Connects the entrances of two buildings with a path.
    pub fn link(state: &mut GameState, a: BuildingId, b: BuildingId) {
        let from = state.building(a).expect("building a").entrance();
        let to = state.building(b).expect("building b").entrance();
        lay_path(state, from, to);
    }

    /// Connects `id` to the fixture depot by road and runs ticks until its construction site
    /// (or upgrade) has all materials; returns the ticks run.
    pub fn supply(state: &mut GameState, id: BuildingId) -> u64 {
        let depot = depot(state);
        link(state, id, depot);
        for t in 0..10_000 {
            if state.building(id).and_then(|b| b.construction_site()).is_none_or(|s| s.fully_supplied()) {
                return t;
            }
            run(state, 1);
        }
        panic!("site {id:?} was not supplied");
    }

    /// Removes every good from all storages (empty kingdom stock).
    pub fn empty_storages(state: &mut GameState) {
        for (_, b) in state.buildings.iter_mut() {
            b.inventory.take_all();
        }
        crate::economy::storage::refresh_stock(state, defs());
    }

    /// Runs `n` ticks without commands and collects the events.
    pub fn run(state: &mut GameState, n: u64) -> Vec<SimEvent> {
        let mut events = Vec::new();
        for _ in 0..n {
            events.extend(state.tick(defs(), Vec::new()));
        }
        events
    }

    /// Runs ticks until `done` holds (checked before every tick); panics after `limit` ticks.
    /// Returns the ticks run.
    pub fn run_until(state: &mut GameState, limit: u64, done: impl Fn(&GameState) -> bool) -> u64 {
        for t in 0..limit {
            if done(state) {
                return t;
            }
            run(state, 1);
        }
        assert!(done(state), "condition not reached within {limit} ticks");
        limit
    }

    /// Runs ticks until it is working time and every employed citizen is at work (at most two
    /// days); returns the ticks run.
    pub fn run_until_at_work(state: &mut GameState) -> u64 {
        let defs = defs();
        run_until(state, 2 * crate::time::TICKS_PER_DAY, |s| {
            let employed: Vec<_> = s.population.citizens().filter(|(_, c)| c.job().is_some()).collect();
            !employed.is_empty() && employed.iter().all(|(_, c)| crate::population::is_at_work(s, defs, c))
        })
    }

    /// Applies one command in a tick and returns that tick's events.
    pub fn exec(state: &mut GameState, command: Command) -> Vec<SimEvent> {
        state.tick(defs(), vec![command])
    }

    /// The rejection reason in `events`, if any.
    pub fn rejection(events: &[SimEvent]) -> Option<&RejectReason> {
        events.iter().find_map(|e| match e {
            SimEvent::CommandRejected { reason, .. } => Some(reason),
            _ => None,
        })
    }

    use glam::IVec2;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use crate::buildings::{BuildingId, Buildings, Rotation};
    use crate::command::{Command, RejectReason};
    use crate::logistics::Logistics;
    use crate::economy::Economy;
    use crate::event::SimEvent;
    use crate::grid::TileRect;
    use crate::policy::Policy;
    use crate::research::Research;
    use crate::roads::RoadNetwork;
    use crate::state::{GameSettings, GameState};
    use crate::territory::Territory;
    use crate::time::GameClock;
    use crate::workforce::Workforce;
    use crate::world::{Biome, Terrain, Tile, Vegetation, WorldMap, WorldSize};
}
