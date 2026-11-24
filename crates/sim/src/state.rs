//! The complete simulation state and the tick pipeline.

use glam::IVec2;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::agriculture::{self, Agriculture};
use crate::buildings::{self, Building, BuildingId, Buildings};
use crate::command::{self, Command};
use crate::defs::{Defs, ScenarioId};
use crate::diplomacy::{self, Diplomacy};
use crate::economy::{self, Economy};
use crate::event::SimEvent;
use crate::events::{self, Events};
use crate::logistics::{self, Logistics};
use crate::military::{self, Military};
use crate::nature;
use crate::navigation::{self, Navigation};
use crate::policy::Policy;
use crate::population::{self, Population};
use crate::production;
use crate::ranks::{self, Progress};
use crate::research::{self, Research};
use crate::roads::{self, RoadNetwork};
use crate::start;
use crate::statistics::{self, Statistics};
use crate::territory::Territory;
use crate::time::{self, GameClock};
use crate::weather::{self, WeatherState};
use crate::workforce::Workforce;
use crate::world::{self, GeneratorSettings, WorldMap, WorldSize};

/// Stream constant mixed into the seed for the state RNG (keeps it independent of the world
/// generator's sub-seeds).
const STATE_RNG_STREAM: u64 = 0x5354_4154_455f_524e; // "STATE_RN"

/// Difficulty level of a game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    /// Forgiving economy and rare disasters.
    Easy,
    /// Balanced.
    #[default]
    Normal,
    /// Demanding economy and frequent threats.
    Hard,
}

impl Difficulty {
    /// All levels.
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            Difficulty::Easy => "Leicht",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Schwer",
        }
    }
}

/// Parameters chosen when starting a new game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSettings {
    /// World and simulation seed.
    pub seed: u64,
    /// Map size.
    pub world_size: WorldSize,
    /// Difficulty.
    pub difficulty: Difficulty,
    /// Starting scenario.
    pub scenario: ScenarioId,
    /// Random events with choices occur (docs/game_design.md §12). On by default; tests of
    /// single systems can switch the daily draw off (pending events and the developer command
    /// `DebugTriggerEvent` still work).
    pub events_enabled: bool,
    /// The game can be lost (docs/game_design.md §1: headquarters conquered, nobody left). On
    /// by default; tests of single systems that empty the kingdom switch it off.
    pub defeat_enabled: bool,
}

impl GameSettings {
    /// Settings with the default scenario, normal difficulty and events enabled.
    pub fn new(defs: &Defs, seed: u64, world_size: WorldSize) -> Self {
        Self {
            seed,
            world_size,
            difficulty: Difficulty::Normal,
            scenario: defs.default_scenario(),
            events_enabled: true,
            defeat_enabled: true,
        }
    }

    /// The same settings with the daily event draw switched on or off.
    pub fn with_events(mut self, enabled: bool) -> Self {
        self.events_enabled = enabled;
        self
    }

    /// The same settings with defeat (game over) switched on or off.
    pub fn with_defeat(mut self, enabled: bool) -> Self {
        self.defeat_enabled = enabled;
        self
    }
}

/// Structural inconsistency of a (deserialized) state.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum StateError {
    /// The scenario id does not exist in the loaded data.
    #[error("unknown scenario id {0}")]
    UnknownScenario(ScenarioId),
    /// Map dimensions do not match the settings.
    #[error("world is {actual_w}x{actual_h} tiles but the settings require {expected}x{expected}")]
    WorldSizeMismatch {
        /// Expected edge length.
        expected: i32,
        /// Actual width.
        actual_w: i32,
        /// Actual height.
        actual_h: i32,
    },
    /// The world's start position lies outside the map.
    #[error("start position {0} is outside the map")]
    StartOutOfBounds(IVec2),
    /// The tax rate exceeds the allowed maximum.
    #[error("tax rate {rate} % exceeds the maximum of {max} %")]
    TaxRateOutOfRange {
        /// Stored rate.
        rate: u8,
        /// Allowed maximum.
        max: u8,
    },
    /// A grid (buildings, roads, territory) does not match the map size.
    #[error("{grid} grid is {actual} but the map is {expected}")]
    GridSizeMismatch {
        /// Which grid.
        grid: &'static str,
        /// Map size.
        expected: IVec2,
        /// Grid size.
        actual: IVec2,
    },
    /// A building is inconsistent with the data.
    #[error("invalid building data: {0}")]
    InvalidBuilding(String),
    /// A road tile is inconsistent (unknown type, bridge on land, road under a building other
    /// than a gate).
    #[error("invalid road at {0}")]
    InvalidRoad(IVec2),
    /// The territory bitmap is malformed.
    #[error("malformed territory data")]
    InvalidTerritory,
    /// The stock does not cover exactly the defined resources.
    #[error("stock has {actual} resource slots, the data defines {expected}")]
    StockMismatch {
        /// Resource definitions.
        expected: usize,
        /// Stored slots.
        actual: usize,
    },
    /// A researched technology does not exist.
    #[error("unknown researched technology {0}")]
    UnknownTech(crate::defs::TechId),
    /// More workers employed than available.
    #[error("workforce employs {employed} of {available} workers")]
    InvalidWorkforce {
        /// Employed.
        employed: u32,
        /// Available.
        available: u32,
    },
    /// Economy data (prices, statistics, ledger, trade orders) does not match the data.
    #[error("malformed economy data")]
    InvalidEconomy,
    /// The cached kingdom stock view differs from the storages' contents.
    #[error("kingdom stock view does not match the storage contents")]
    StockViewMismatch,
    /// Citizens, households or homes are inconsistent with each other or the buildings.
    #[error("invalid population data: {0}")]
    InvalidPopulation(String),
    /// A transport job is malformed.
    #[error("invalid logistics data: {0}")]
    InvalidLogistics(String),
    /// The hourly history is malformed.
    #[error("malformed statistics history")]
    InvalidStatistics,
    /// The weather state is out of range.
    #[error("invalid weather data: {0}")]
    InvalidWeather(String),
    /// The research queue or progress is inconsistent with the data.
    #[error("invalid research data: {0}")]
    InvalidResearch(String),
    /// The event state is inconsistent with the data.
    #[error("invalid event data: {0}")]
    InvalidEvents(String),
    /// Kingdoms, their land, the diplomatic inbox or caravans are inconsistent.
    #[error("invalid diplomacy data: {0}")]
    InvalidDiplomacy(String),
    /// Soldiers, companies, enemy groups, campaigns or ranks are inconsistent.
    #[error("invalid military data: {0}")]
    InvalidMilitary(String),
    /// Fields and farms are inconsistent with each other, the buildings or the roads.
    #[error("invalid agriculture data: {0}")]
    InvalidAgriculture(String),
}

/// Stream constant mixed into the seed for the game identity (keeps the state RNG untouched).
const GAME_ID_STREAM: u64 = 0x4741_4D45_4944_2121; // "GAMEID!!"

/// The identity of a game started with `settings` (docs/architecture.md §3.4): a version-4
/// UUID whose bytes come from a ChaCha stream of the seed, the map size, the scenario, the
/// difficulty and the switches, so it is deterministic (a replay of the same start gets the
/// same identity) and drawing it shifts no random roll of the simulation.
pub fn game_identity(settings: &GameSettings) -> uuid::Uuid {
    use rand::Rng;
    let mut key = settings.seed ^ GAME_ID_STREAM;
    for part in [
        settings.world_size as u64,
        settings.scenario.index() as u64,
        settings.difficulty as u64,
        u64::from(settings.events_enabled),
        u64::from(settings.defeat_enabled),
    ] {
        key = key.rotate_left(13) ^ part.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    let mut rng = ChaCha8Rng::seed_from_u64(key);
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    uuid::Builder::from_random_bytes(bytes).into_uuid()
}

/// The whole simulation state. Everything needed to continue a game exactly is in here
/// (including the RNG state), so serializing it is a complete save game.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameState {
    /// Identity of the game (the same for every save of it; [`game_identity`]).
    pub(crate) game_id: uuid::Uuid,
    pub(crate) settings: GameSettings,
    pub(crate) world: WorldMap,
    pub(crate) clock: GameClock,
    pub(crate) rng: ChaCha8Rng,
    /// Today's weather, snow cover and ground wetness.
    pub(crate) weather: WeatherState,
    pub(crate) policy: Policy,
    /// Treasury and kingdom stock.
    pub(crate) economy: Economy,
    /// Researched technologies, the research queue and its progress.
    pub(crate) research: Research,
    /// Pending event, cooldowns, modifiers and history of events with choices.
    pub(crate) events: Events,
    /// AI kingdoms, their land, relations, the diplomatic inbox and caravans.
    pub(crate) diplomacy: Diplomacy,
    /// Soldiers, companies, enemy groups, campaigns and battle reports.
    pub(crate) military: Military,
    /// Ruler rank and the end of a lost game.
    pub(crate) progress: Progress,
    /// Buildings and occupancy grid.
    pub(crate) buildings: Buildings,
    /// Road grid.
    pub(crate) roads: RoadNetwork,
    /// Owned tiles.
    pub(crate) territory: Territory,
    /// Farms and their claimed field and pasture tiles.
    pub(crate) agriculture: Agriculture,
    /// Citizens, households and homes.
    pub(crate) population: Population,
    /// Population/employment counts and pending job/housing work.
    pub(crate) workforce: Workforce,
    /// Transport job queue.
    pub(crate) logistics: Logistics,
    /// Hourly history (pipeline step 10).
    pub(crate) statistics: Statistics,
    /// Commands queued via [`GameState::queue_command`] for the next tick.
    pub(crate) pending_commands: Vec<Command>,
    /// Walk-cost grid (derived), path memory (saved with the fingerprint of its grid) and
    /// pathfinder counters.
    pub(crate) navigation: Navigation,
}

/// Generator settings of the world a new game with `settings` is played on: its seed and size
/// and one settlement site per AI kingdom (an unknown scenario counts as the default one, as in
/// [`GameState::new`]). Landscape previews use the same, so they show the same map.
pub fn world_generator_settings(defs: &Defs, settings: &GameSettings) -> GeneratorSettings {
    let scenario = defs.scenarios().get(settings.scenario.index()).unwrap_or_else(|| defs.scenario(defs.default_scenario()));
    let kingdoms = diplomacy::setup::kingdom_count(defs, settings.world_size, scenario.rival_kingdoms);
    GeneratorSettings { rival_sites: u32::from(kingdoms), ..GeneratorSettings::with_seed(settings.seed, settings.world_size) }
}

impl GameState {
    /// Starts a new game: generates the world from the seed and applies the scenario: research,
    /// gold, stock and settlers, the headquarters near the world's start point, the other start
    /// buildings (finished) and a path connecting them.
    ///
    /// If `settings.scenario` is not an id of `defs` (e.g. settings restored from an outdated
    /// configuration), the default scenario is used instead.
    pub fn new(defs: &Defs, mut settings: GameSettings) -> GameState {
        if settings.scenario.index() >= defs.scenarios().len() {
            log::warn!("unknown scenario {}; using the default scenario", settings.scenario);
            settings.scenario = defs.default_scenario();
        }
        let scenario = defs.scenario(settings.scenario);
        let generator = world_generator_settings(defs, &settings);
        let world = world::generate(&generator);
        let rng = ChaCha8Rng::seed_from_u64(settings.seed ^ STATE_RNG_STREAM);
        let climate = world.get(world.start).map_or(0, |t| WeatherState::climate_shift_c(defs.rules(), t.temperature));
        let weather = WeatherState::new(defs, settings.seed, GameClock::new(), climate);
        let events = Events::new(settings.seed);
        let settings_seed = settings.seed;
        let (w, h) = (world.width(), world.height());
        let scenario_id = settings.scenario;
        let mut state = GameState {
            game_id: game_identity(&settings),
            policy: Policy::new(scenario.tax_rate_percent),
            settings,
            world,
            clock: GameClock::new(),
            rng,
            weather,
            economy: Economy::new(defs),
            research: Research::new(),
            events,
            diplomacy: Diplomacy::new(w, h, settings_seed),
            military: Military::new(settings_seed),
            progress: Progress::default(),
            buildings: Buildings::new(w, h),
            roads: RoadNetwork::new(w, h),
            territory: Territory::new(w, h),
            agriculture: Agriculture::default(),
            population: Population::default(),
            workforce: Workforce::default(),
            logistics: Logistics::default(),
            statistics: Statistics::default(),
            pending_commands: Vec::new(),
            navigation: Navigation::default(),
        };
        navigation::rebuild(&mut state, defs);
        start::apply_scenario(&mut state, defs, scenario_id);
        diplomacy::setup::create_kingdoms(&mut state, defs);
        economy::storage::refresh_stock(&mut state, defs);
        navigation::sync(&mut state, defs);
        population::movement::rebuild_derived(&mut state);
        state.population.refresh_occupancy();
        state
    }

    /// Advances the simulation by one tick and returns what happened.
    ///
    /// `commands` are applied after commands queued with [`GameState::queue_command`]. The
    /// pipeline order is part of the determinism guarantee (docs/architecture.md §3.3): commands
    /// (1), calendar and weather (2), construction incl. job allocation and automatic house
    /// upgrades (3), nature — trees, stumps, fish — and agriculture — farms, claims, growth,
    /// frost, sowing and harvest (4), production (5), logistics (6),
    /// population — housing, household supply, needs, routine, demography, house tiers (7),
    /// movement — navigation grid, path requests, walking (7b), economy —
    /// hourly money flows, trade, bankruptcy (8), research — progress, start and completion —
    /// diplomacy — kingdoms, relations, initiatives, caravans —, the military — training,
    /// threats, combat, campaigns —, events — expiry, the daily draw — and ranks (9),
    /// statistics — hourly samples and the daily closing (10).
    ///
    /// Once the game is lost ([`GameState::game_over`]) a tick changes nothing: commands are
    /// rejected with [`command::RejectReason::GameOver`] and the clock stands still.
    pub fn tick(&mut self, defs: &Defs, commands: Vec<Command>) -> Vec<SimEvent> {
        self.tick_probed(defs, commands, &mut |_| {})
    }

    /// The tick pipeline; `probe` is called with the name of every step right after it ran
    /// (performance measurements in tests; it cannot touch the state).
    fn tick_probed(&mut self, defs: &Defs, commands: Vec<Command>, probe: &mut dyn FnMut(&'static str)) -> Vec<SimEvent> {
        let mut events = Vec::new();
        let mut batch = std::mem::take(&mut self.pending_commands);
        batch.extend(commands);
        if self.progress.game_over.is_some() {
            for command in batch {
                events.push(SimEvent::CommandRejected { command, reason: command::RejectReason::GameOver });
            }
            return events;
        }
        command::apply_all(self, defs, batch, &mut events);
        probe("commands");
        time::run_calendar(&mut self.clock, &mut events);
        weather::run_weather(self, defs, &mut events);
        probe("calendar");
        buildings::run_construction(self, defs, &mut events);
        probe("construction");
        nature::run_nature(self, defs);
        probe("nature");
        agriculture::run_agriculture(self, defs, &mut events);
        probe("agriculture");
        production::run_production(self, defs, &mut events);
        probe("production");
        logistics::run_logistics(self, defs, &mut events);
        probe("logistics");
        population::run_population(self, defs, &mut events);
        probe("population");
        population::run_movement(self, defs, &mut events);
        probe("movement");
        economy::finance::run_economy(self, defs, &mut events);
        economy::storage::refresh_stock(self, defs);
        probe("economy");
        research::run_research(self, defs, &mut events);
        probe("research");
        diplomacy::run_diplomacy(self, defs, &mut events);
        probe("diplomacy");
        military::run_military(self, defs, &mut events);
        probe("military");
        events::run_events(self, defs, &mut events);
        ranks::run(self, defs, &mut events);
        probe("events");
        statistics::run_statistics(self, defs, &mut events);
        // The grid follows every change of this tick, so a save never holds changes the path
        // memory has not seen; renderers read who is inside the buildings now.
        navigation::sync(self, defs);
        self.population.refresh_occupancy();
        probe("statistics");
        events
    }

    /// One tick without commands, returning the nanoseconds every pipeline step took (tests).
    #[cfg(test)]
    pub(crate) fn tick_timed(&mut self, defs: &Defs) -> Vec<(&'static str, u128)> {
        let start = std::time::Instant::now();
        let mut marks: Vec<(&'static str, u128)> = Vec::with_capacity(10);
        self.tick_probed(defs, Vec::new(), &mut |name| marks.push((name, start.elapsed().as_nanos())));
        let mut last = 0;
        marks
            .into_iter()
            .map(|(name, at)| {
                let spent = at - last;
                last = at;
                (name, spent)
            })
            .collect()
    }

    /// Queues a command for the next tick.
    pub fn queue_command(&mut self, command: Command) {
        self.pending_commands.push(command);
    }

    /// Commands waiting for the next tick.
    pub fn pending_commands(&self) -> &[Command] {
        &self.pending_commands
    }

    /// Game settings.
    pub fn settings(&self) -> &GameSettings {
        &self.settings
    }

    /// Identity of the game: every save of the same game carries it (save lists group by it).
    pub fn game_id(&self) -> uuid::Uuid {
        self.game_id
    }

    /// The map.
    pub fn world(&self) -> &WorldMap {
        &self.world
    }

    /// Chunks whose tiles changed since the last call (for renderer cache invalidation).
    pub fn take_dirty_chunks(&mut self) -> Vec<IVec2> {
        self.world.take_dirty_chunks()
    }

    /// Chunks whose roads, walls, gates or building footprints/entrances changed since the last
    /// call, including the neighbouring tiles whose road or wall masks depend on them (for the
    /// road/wall layer of renderers). Not saved: a freshly loaded game reports nothing.
    pub fn take_structure_dirty_chunks(&mut self) -> Vec<IVec2> {
        let mut chunks = self.roads.changes_mut().take_chunks();
        for c in self.buildings.changes_mut().take_chunks() {
            if !chunks.contains(&c) {
                chunks.push(c);
            }
        }
        chunks.sort_by_key(|c| (c.y, c.x));
        chunks
    }

    /// Counter of road and building footprint changes (wrapping; not saved). A renderer that
    /// remembers it can tell cheaply whether anything structural changed.
    pub fn structure_revision(&self) -> u64 {
        self.roads.revision().wrapping_add(self.buildings.revision())
    }

    /// Chunks whose field tiles changed visibly since the last call (claimed, released, sown,
    /// harvested, new growth stage) for the field layer of renderers. Not saved.
    pub fn take_field_dirty_chunks(&mut self) -> Vec<IVec2> {
        self.agriculture.take_dirty_chunks()
    }

    /// The calendar.
    pub fn clock(&self) -> GameClock {
        self.clock
    }

    /// Today's weather, snow cover and ground wetness.
    pub fn weather(&self) -> &WeatherState {
        &self.weather
    }

    /// Player policies.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub(crate) fn policy_mut(&mut self) -> &mut Policy {
        &mut self.policy
    }

    /// Treasury and kingdom stock.
    pub fn economy(&self) -> &Economy {
        &self.economy
    }

    /// Researched technologies, unlocks, the research queue and its progress.
    pub fn research(&self) -> &Research {
        &self.research
    }

    /// Pending event, temporary modifiers, cooldowns and the event history.
    pub fn events(&self) -> &Events {
        &self.events
    }

    /// AI kingdoms, their land and structures, relations among them, the diplomatic inbox,
    /// caravans and the diplomatic history.
    pub fn diplomacy(&self) -> &Diplomacy {
        &self.diplomacy
    }

    /// Soldiers, companies, enemy groups (bandits, AI armies), campaigns and battle reports.
    pub fn military(&self) -> &Military {
        &self.military
    }

    /// Ruler rank progress and the end of a lost game.
    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    /// The defeat, once the game is lost (the simulation stands still).
    pub fn game_over(&self) -> Option<ranks::GameOver> {
        self.progress.game_over
    }

    /// Chunks whose kingdom borders or AI structures changed since the last call (for the
    /// border overlay and the AI town layer of renderers). Not saved.
    pub fn take_land_dirty_chunks(&mut self) -> Vec<IVec2> {
        self.diplomacy.take_dirty_chunks()
    }

    /// All buildings and the occupancy grid.
    pub fn buildings(&self) -> &Buildings {
        &self.buildings
    }

    /// A building by id (`None` if it no longer exists).
    pub fn building(&self, id: BuildingId) -> Option<&Building> {
        self.buildings.get(id)
    }

    /// The road grid.
    pub fn roads(&self) -> &RoadNetwork {
        &self.roads
    }

    /// Auto-connection mask of the road at `p` including building entrances (see
    /// [`roads::road_mask`]); `None` without a road.
    pub fn road_mask(&self, p: IVec2) -> Option<u8> {
        roads::road_mask(&self.world, &self.roads, &self.buildings, p)
    }

    /// True if a road touches the building's entrance.
    pub fn has_road_access(&self, building: BuildingId) -> bool {
        roads::has_road_access(&self.world, &self.roads, &self.buildings, building)
    }

    /// Owned tiles.
    pub fn territory(&self) -> &Territory {
        &self.territory
    }

    /// Farms and their claimed field and pasture tiles (crops, growth stages, soil).
    pub fn agriculture(&self) -> &Agriculture {
        &self.agriculture
    }

    /// Height level a building stands on. Placement only allows flat footprints (all tiles at
    /// the same level), so this is the level of every footprint tile; `None` for unknown ids.
    pub fn building_elevation(&self, building: BuildingId) -> Option<u8> {
        let b = self.buildings.get(building)?;
        b.rect().tiles().filter_map(|p| self.world.elevation(p)).max()
    }

    /// Population, working-age and employed counts.
    pub fn workforce(&self) -> &Workforce {
        &self.workforce
    }

    /// Citizens, households and homes.
    pub fn population(&self) -> &Population {
        &self.population
    }

    /// Transport jobs.
    pub fn logistics(&self) -> &Logistics {
        &self.logistics
    }

    /// Hourly history (treasury, stock, population) for charts.
    pub fn statistics(&self) -> &Statistics {
        &self.statistics
    }

    /// Walk-cost grid and pathfinder counters as of the last tick (derived, not saved).
    pub fn navigation(&self) -> &Navigation {
        &self.navigation
    }

    /// Visible walkers (citizens on the map, emigrants leaving) whose tile lies in `rect`, for
    /// renderers; see [`population::citizens_in`].
    pub fn citizens_in<'a>(
        &'a self,
        defs: &'a Defs,
        rect: crate::grid::TileRect,
    ) -> impl Iterator<Item = population::CitizenView> + 'a {
        population::citizens_in(self, defs, rect)
    }

    /// Re-applies the values a save stores but that derive from the game data, after loading
    /// with possibly rebalanced data of the same fingerprint: storage capacities (incl. research
    /// bonus) and production buffer capacities of every finished building, the territory, the
    /// kingdom stock view, life stages and house capacities (`population::rederive`), the
    /// research queue order (prerequisites may change) and the length of the event history.
    /// Contents
    /// are kept; goods beyond a lowered capacity become surplus that carriers take away. A no-op
    /// for unchanged data, so saving and loading stays exact.
    pub(crate) fn rederive(&mut self, defs: &Defs) {
        let bonus = economy::storage::storage_bonus_percent(&self.research, defs);
        for (_, b) in self.buildings.iter_mut() {
            if b.is_complete() {
                economy::storage::init_storage(b, defs, bonus);
                production::init_production(b, defs);
            }
        }
        self.territory.recompute(&self.buildings, defs, &self.diplomacy.land);
        economy::storage::refresh_stock(self, defs);
        population::rederive(self, defs);
        self.agriculture.rederive(&self.buildings, defs);
        self.research.rederive(defs);
        self.events.rederive(defs);
        military::rederive(self, defs);
    }

    /// Checks structural consistency against the loaded data (used after loading a save).
    pub fn validate(&self, defs: &Defs) -> Result<(), StateError> {
        if self.settings.scenario.index() >= defs.scenarios().len() {
            return Err(StateError::UnknownScenario(self.settings.scenario));
        }
        let expected = self.settings.world_size.tiles();
        if self.world.width() != expected || self.world.height() != expected {
            return Err(StateError::WorldSizeMismatch {
                expected,
                actual_w: self.world.width(),
                actual_h: self.world.height(),
            });
        }
        if !self.world.in_bounds(self.world.start) {
            return Err(StateError::StartOutOfBounds(self.world.start));
        }
        let max = defs.rules().max_tax_rate_percent;
        if self.policy.tax_rate_percent > max {
            return Err(StateError::TaxRateOutOfRange { rate: self.policy.tax_rate_percent, max });
        }
        self.validate_entities(defs)
    }

    /// Consistency of buildings, roads, territory, economy, research, population and workforce.
    pub(crate) fn validate_entities(&self, defs: &Defs) -> Result<(), StateError> {
        let map = self.world.size();
        for (grid, actual) in [
            ("building", self.buildings.grid_size()),
            ("road", self.roads.grid_size()),
            ("territory", self.territory.grid_size()),
        ] {
            if actual != map {
                return Err(StateError::GridSizeMismatch { grid, expected: map, actual });
            }
        }
        self.buildings.validate(defs).map_err(StateError::InvalidBuilding)?;
        if let Some((id, _)) = self
            .buildings
            .iter()
            .find(|(_, b)| b.construction_site().and_then(|s| s.last_work_tick()).is_some_and(|t| t > self.clock.tick))
        {
            return Err(StateError::InvalidBuilding(format!("site {id:?} was worked on in the future")));
        }
        for (p, tile) in self.roads.iter() {
            let on_water = self.world.tile(p).is_water();
            let def = defs.roads().get(tile.road.index()).ok_or(StateError::InvalidRoad(p))?;
            let under_building = self.buildings.at(p).is_some_and(|(_, b)| !buildings::carries_road(defs, b));
            if under_building || on_water != tile.is_bridge() || tile.is_bridge() != def.bridge.is_some() {
                return Err(StateError::InvalidRoad(p));
            }
        }
        if !self.territory.is_consistent() {
            return Err(StateError::InvalidTerritory);
        }
        if self.economy.stock().slot_count() != defs.resources().len() {
            return Err(StateError::StockMismatch {
                expected: defs.resources().len(),
                actual: self.economy.stock().slot_count(),
            });
        }
        if !self.economy.is_consistent(defs) {
            return Err(StateError::InvalidEconomy);
        }
        let mut recomputed = self.clone();
        economy::storage::refresh_stock(&mut recomputed, defs);
        if recomputed.economy != self.economy {
            return Err(StateError::StockViewMismatch);
        }
        logistics::validate(self, defs).map_err(StateError::InvalidLogistics)?;
        if !self.statistics.is_consistent(defs.resources().len()) {
            return Err(StateError::InvalidStatistics);
        }
        self.weather.validate(defs).map_err(StateError::InvalidWeather)?;
        self.agriculture.validate(self, defs).map_err(StateError::InvalidAgriculture)?;
        if let Some(t) = self.research.researched().find(|t| t.index() >= defs.techs().len()) {
            return Err(StateError::UnknownTech(t));
        }
        self.research.validate(defs).map_err(StateError::InvalidResearch)?;
        self.events.validate(defs, self.clock).map_err(StateError::InvalidEvents)?;
        self.diplomacy.validate(self, defs).map_err(StateError::InvalidDiplomacy)?;
        self.military.validate(self, defs).map_err(StateError::InvalidMilitary)?;
        self.progress.validate(defs).map_err(StateError::InvalidMilitary)?;
        if self.workforce.employed() > self.workforce.available() {
            return Err(StateError::InvalidWorkforce {
                employed: self.workforce.employed(),
                available: self.workforce.available(),
            });
        }
        self.population.validate(self, defs).map_err(StateError::InvalidPopulation)?;
        let mut counts = self.workforce.clone();
        counts.refresh_counts(&self.population);
        if counts != self.workforce {
            return Err(StateError::InvalidWorkforce {
                employed: self.workforce.employed(),
                available: self.workforce.available(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::RejectReason;
    use crate::defs::JobCategory;
    use crate::policy::JobPriority;
    use crate::test_support::defs;
    use crate::time::{Season, TICKS_PER_DAY};

    fn small_state(seed: u64) -> GameState {
        let defs = defs();
        GameState::new(defs, GameSettings::new(defs, seed, WorldSize::Small))
    }

    #[test]
    fn new_game_uses_scenario_and_generates_world() {
        let defs = defs();
        let state = small_state(7);
        let scenario = defs.scenario(defs.default_scenario());
        assert_eq!(state.world().width(), WorldSize::Small.tiles());
        // Small maps hold three kingdoms (rules.ron → diplomacy.kingdoms; the scenario allows up to five).
        let kingdoms = crate::diplomacy::setup::kingdom_count(defs, WorldSize::Small, scenario.rival_kingdoms);
        assert_eq!(kingdoms, 3);
        assert_eq!(state.world().settlement_sites.len(), usize::from(kingdoms));
        assert_eq!(state.diplomacy().len(), usize::from(kingdoms));
        assert_eq!(state.policy().tax_rate_percent, scenario.tax_rate_percent);
        assert_eq!(state.clock(), GameClock::new());
        state.validate(defs).unwrap();
    }

    #[test]
    fn tick_applies_commands_then_advances_calendar() {
        let defs = defs();
        let mut state = small_state(1);
        state.queue_command(Command::SetWorkforcePriority { category: JobCategory::Food, priority: JobPriority::High });
        let events = state.tick(defs, vec![Command::SetTaxRate { percent: 20 }]);
        assert_eq!(state.clock().tick, 1);
        assert_eq!(state.policy().tax_rate_percent, 20);
        assert_eq!(state.policy().job_priority(JobCategory::Food), JobPriority::High);
        assert!(state.pending_commands().is_empty());
        // Queued commands run before the ones passed to `tick`.
        assert_eq!(
            events,
            vec![
                SimEvent::WorkforcePriorityChanged { category: JobCategory::Food, priority: JobPriority::High },
                SimEvent::TaxRateChanged { percent: 20 },
            ]
        );
        // Re-applying the same values changes nothing and emits nothing.
        let events = state.tick(defs, vec![Command::SetTaxRate { percent: 20 }]);
        assert!(events.is_empty());
    }

    #[test]
    fn invalid_commands_are_rejected_with_reason() {
        let defs = defs();
        let mut state = small_state(1);
        let before = state.policy().tax_rate_percent;
        let max = defs.rules().max_tax_rate_percent;
        let cmd = Command::SetTaxRate { percent: max + 1 };
        let events = state.tick(defs, vec![cmd.clone()]);
        assert_eq!(state.policy().tax_rate_percent, before);
        let reason = RejectReason::TaxRateTooHigh { requested: max + 1, max };
        assert_eq!(events, vec![SimEvent::CommandRejected { command: cmd, reason: reason.clone() }]);
        assert!(reason.to_string().contains("zu hoch"));
    }

    #[test]
    fn calendar_events_flow_through_tick() {
        let defs = defs();
        let mut state = small_state(3);
        let mut seasons = Vec::new();
        let mut days = 0;
        // Run until the start of summer (6 days minus the 6 hours already elapsed at start).
        let ticks = 6 * TICKS_PER_DAY - 6 * crate::time::TICKS_PER_HOUR;
        for _ in 0..ticks {
            for e in state.tick(defs, Vec::new()) {
                match e {
                    SimEvent::SeasonChanged { season, .. } => seasons.push(season),
                    SimEvent::DayChanged { .. } => days += 1,
                    _ => {}
                }
            }
        }
        assert_eq!(state.clock().season(), Season::Summer);
        assert_eq!(seasons, vec![Season::Summer]);
        assert_eq!(days, 6);
    }

    #[test]
    fn validate_detects_inconsistencies() {
        let defs = defs();
        let mut state = small_state(2);
        state.policy.tax_rate_percent = 200;
        assert!(matches!(state.validate(defs), Err(StateError::TaxRateOutOfRange { .. })));
        let mut state = small_state(2);
        state.settings.world_size = WorldSize::Huge;
        assert!(matches!(state.validate(defs), Err(StateError::WorldSizeMismatch { .. })));
        let mut state = small_state(2);
        state.world.start = IVec2::new(-1, 0);
        assert!(matches!(state.validate(defs), Err(StateError::StartOutOfBounds(_))));
    }

    #[test]
    fn validate_detects_inconsistent_entities() {
        let defs = defs();
        let state = small_state(5);
        state.validate(defs).unwrap();
        // A road under a building.
        let mut broken = state.clone();
        let (_, hall) = broken.buildings.iter().next().unwrap();
        let p = hall.origin();
        let path = defs.road_id("path").unwrap();
        broken.roads.set(p, Some(crate::roads::RoadTile { road: path, bridge_axis: None }));
        assert_eq!(broken.validate(defs), Err(StateError::InvalidRoad(p)));
        // A bridge on land.
        let mut broken = state.clone();
        let land = broken.world.start + IVec2::new(0, 8);
        let bridge = defs.road_id("bridge").unwrap();
        broken.roads.set(land, Some(crate::roads::RoadTile { road: bridge, bridge_axis: Some(crate::grid::Axis::X) }));
        assert!(matches!(broken.validate(defs), Err(StateError::InvalidRoad(_))));
        // Grid size, stock size, unknown tech, workforce.
        let mut broken = state.clone();
        broken.territory = Territory::new(4, 4);
        assert!(matches!(broken.validate(defs), Err(StateError::GridSizeMismatch { grid: "territory", .. })));
        let mut broken = state.clone();
        broken.economy = Economy::default();
        assert!(matches!(broken.validate(defs), Err(StateError::StockMismatch { actual: 0, .. })));
        let mut broken = state.clone();
        broken.research.mark_researched(crate::defs::TechId::from_index(9999).unwrap());
        assert!(matches!(broken.validate(defs), Err(StateError::UnknownTech(_))));
        let mut broken = state.clone();
        broken.workforce.refresh_counts(&Population::default());
        assert!(matches!(broken.validate(defs), Err(StateError::InvalidWorkforce { .. })));
        // Population cross references: job without staffing, member of a foreign household,
        // home that does not list its household.
        let worker = state.population.citizens().find(|(_, c)| c.job().is_some()).map(|(id, _)| id).unwrap();
        let mut broken = state.clone();
        let job = broken.population.citizen(worker).unwrap().job().unwrap();
        broken.buildings.get_mut(job.building).unwrap().set_staffed(0);
        assert!(matches!(broken.validate(defs), Err(StateError::InvalidPopulation(_))));
        let mut broken = state.clone();
        let (a, b) = {
            let mut ids = broken.population.households().map(|(id, _)| id);
            (ids.next().unwrap(), ids.next().unwrap())
        };
        let moved = broken.population.household(a).unwrap().members()[0];
        broken.population.households.get_mut(b).unwrap().members.push(moved);
        assert!(matches!(broken.validate(defs), Err(StateError::InvalidPopulation(_))));
        let mut broken = state.clone();
        let housed = broken.population.households().find(|(_, h)| h.home().is_some()).map(|(id, _)| id).unwrap();
        let home = broken.population.household(housed).unwrap().home().unwrap();
        broken.population.homes.get_mut(&home).unwrap().households.retain(|h| *h != housed);
        assert!(matches!(broken.validate(defs), Err(StateError::InvalidPopulation(_))));
        // A stock view that does not match the storages, malformed economy and logistics data.
        let mut broken = state.clone();
        let hall = broken.buildings.iter().next().map(|(id, _)| id).unwrap();
        broken.buildings.get_mut(hall).unwrap().inventory.remove(defs.resource_id("wood").unwrap(), 1);
        assert_eq!(broken.validate(defs), Err(StateError::StockViewMismatch));
        let mut broken = state.clone();
        broken.economy.accrue_hour(crate::economy::LedgerCategory::Taxes, 5);
        broken.economy.ledger_mut().close_day(1);
        assert_eq!(broken.validate(defs), Ok(()), "ledger bookings are valid data");
        let mut broken = state.clone();
        let wood = defs.resource_id("wood").unwrap();
        let bogus = crate::economy::TradeOrder { resource: wood, buy_below: None, sell_above: None, daily_limit: 1 };
        broken.economy.trade_mut().set(bogus);
        assert_eq!(broken.validate(defs), Err(StateError::InvalidEconomy));
        let mut broken = state.clone();
        broken.logistics.push(crate::logistics::TransportJob {
            id: 0,
            kind: crate::logistics::TransportKind::Supply,
            resource: wood,
            amount: 0,
            source: hall,
            target: hall,
            carrier_base: hall,
            priority: crate::policy::JobPriority::Normal,
            created_at: 0,
            stage: crate::logistics::JobStage::Waiting,
        });
        assert!(matches!(broken.validate(defs), Err(StateError::InvalidLogistics(_))));
        // An hourly history out of order.
        let mut broken = state.clone();
        let sample = |hour| crate::statistics::HourSample { hour, treasury: 0, stock_total: 0, population: 0, employed: 0 };
        broken.statistics.record(sample(9));
        broken.statistics.record(sample(8));
        assert_eq!(broken.validate(defs), Err(StateError::InvalidStatistics));
        // A building whose footprint does not match its type.
        let mut broken = state;
        let id = broken.buildings.iter().next().map(|(id, _)| id).unwrap();
        broken.buildings.get_mut(id).unwrap().rotation = crate::grid::Rotation::new(1).unwrap();
        broken.buildings.get_mut(id).unwrap().kind = defs.building_id("sawmill").unwrap();
        assert!(matches!(broken.validate(defs), Err(StateError::InvalidBuilding(_))));
    }

    #[test]
    fn unknown_scenario_falls_back_to_default() {
        let defs = defs();
        let mut settings = GameSettings::new(defs, 4, WorldSize::Small);
        settings.scenario = ScenarioId::from_index(200).unwrap();
        let state = GameState::new(defs, settings);
        assert_eq!(state.settings().scenario, defs.default_scenario());
        let mut broken = state.clone();
        broken.settings.scenario = ScenarioId::from_index(200).unwrap();
        assert_eq!(broken.validate(defs), Err(StateError::UnknownScenario(ScenarioId::from_index(200).unwrap())));
    }

    #[test]
    fn structure_changes_are_tracked_for_renderers() {
        let defs = defs();
        let mut state = crate::test_support::flat_state(40);
        state.take_structure_dirty_chunks();
        let before = state.structure_revision();
        let id = crate::test_support::spawn(&mut state, "sawmill", 30, 33);
        let chunks = state.take_structure_dirty_chunks();
        // Footprint 30..=31 × 33..=35 plus its neighbours lies in chunks (0, 1) and (1, 1).
        assert_eq!(chunks, vec![IVec2::new(0, 1), IVec2::new(1, 1)]);
        assert!(state.structure_revision() > before);
        assert!(state.take_structure_dirty_chunks().is_empty());
        let path = defs.road_id("path").unwrap();
        state.roads.set(IVec2::new(5, 5), Some(crate::roads::RoadTile { road: path, bridge_axis: None }));
        assert_eq!(state.take_structure_dirty_chunks(), vec![IVec2::new(0, 0)]);
        assert_eq!(state.building_elevation(id), Some(2));
    }

    #[test]
    fn debug_commands_unlock_research_and_force_weather() {
        let defs = defs();
        let mut state = small_state(4);
        let events = state.tick(defs, vec![Command::DebugUnlockAllResearch]);
        assert!(events.contains(&SimEvent::DebugResearchUnlocked { techs: defs.techs().len() as u32 }));
        assert_eq!(state.research().researched().count(), defs.techs().len());
        let farm = defs.building_id("wheat_farm").unwrap();
        assert!(state.research().building_unlocked(defs, farm));
        let cmd = Command::DebugSetWeather { weather: crate::defs::Weather::Snow, intensity: 90, days: 2 };
        let events = state.tick(defs, vec![cmd]);
        assert!(events.contains(&SimEvent::WeatherChanged { weather: crate::defs::Weather::Snow, intensity: 90 }));
        for _ in 0..TICKS_PER_DAY {
            state.tick(defs, Vec::new());
        }
        assert_eq!(state.weather().current(), crate::defs::Weather::Snow, "forced for two days");
        let bad = Command::DebugSetWeather { weather: crate::defs::Weather::Rain, intensity: 0, days: 1 };
        let events = state.tick(defs, vec![bad.clone()]);
        assert_eq!(events, vec![SimEvent::CommandRejected { command: bad, reason: RejectReason::InvalidIntensity }]);
        state.validate(defs).unwrap();
    }

    #[test]
    fn difficulty_names() {
        assert_eq!(Difficulty::ALL.map(Difficulty::display_name), ["Leicht", "Normal", "Schwer"]);
    }
}
