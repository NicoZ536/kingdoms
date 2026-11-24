//! The running game: simulation state and game data, the command queue the UI fills and the
//! log of simulation events.
//!
//! The session is the only place that advances the simulation. Scenes call [`Session::tick`]
//! once per engine tick (the engine's [`GameSpeed`](kingdoms_engine::core::time::GameSpeed)
//! decides how many ticks a frame runs); every command queued with
//! [`Session::queue_command`] since the previous tick is applied at the start of the next one.

pub(crate) mod dev_army;
mod dev_case;
pub mod dev_crowd;
pub mod dev_showcase;
mod dev_settlement;
mod dev_walls;
mod dev_trade;
mod event_log;
mod launch;

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::IVec2;
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingRole, Defs};
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::state::{GameSettings, GameState};

pub use dev_settlement::DevPlan;
use dev_army::DevArmy;
pub use dev_case::{DevCase, DevView};
use dev_case::DevCasePlan;
use dev_walls::DevWalls;
use dev_trade::DevTrade;
use dev_settlement::DevSettlement;
pub use event_log::{EventCursor, EventLog};
pub use launch::{DevThreat, DevWeather, FollowTarget, LaunchOptions, QuickStart, StartMode, ToolSpec};

/// Events kept for the UI, audio and the map view.
pub const EVENT_LOG_CAPACITY: usize = 2048;
/// Ticks the tick-time statistics cover.
const TIMING_WINDOW: usize = 120;

/// Measured wall-clock cost of simulation ticks (presentation statistics only).
#[derive(Debug)]
pub struct TickTiming {
    recent: VecDeque<Duration>,
}

impl TickTiming {
    fn new() -> Self {
        Self { recent: VecDeque::with_capacity(TIMING_WINDOW) }
    }

    fn record(&mut self, elapsed: Duration) {
        if self.recent.len() == TIMING_WINDOW {
            self.recent.pop_front();
        }
        self.recent.push_back(elapsed);
    }

    /// Duration of the last tick in milliseconds.
    pub fn last_ms(&self) -> f64 {
        self.recent.back().map_or(0.0, |d| d.as_secs_f64() * 1000.0)
    }

    /// Average over the recent ticks in milliseconds.
    pub fn average_ms(&self) -> f64 {
        if self.recent.is_empty() {
            return 0.0;
        }
        self.recent.iter().map(Duration::as_secs_f64).sum::<f64>() * 1000.0 / self.recent.len() as f64
    }

    /// Slowest recent tick in milliseconds.
    pub fn max_ms(&self) -> f64 {
        self.recent.iter().max().map_or(0.0, |d| d.as_secs_f64() * 1000.0)
    }
}

/// A running game.
pub struct Session {
    defs: Arc<Defs>,
    state: GameState,
    queued: Vec<Command>,
    events: EventLog,
    timing: TickTiming,
    dev_settlement: Option<DevSettlement>,
    dev_army: Option<DevArmy>,
    dev_walls: Option<DevWalls>,
    dev_trade: Option<DevTrade>,
    dev_case: Option<DevCasePlan>,
    /// What the screen shows once the developer case is set up (`KINGDOMS_DEV_CASE`).
    dev_view: Option<DevView>,
    /// Real seconds played in this game (presentation statistics for the save list).
    play_seconds: f64,
    /// Simulation tick of the last save or load (or the start); progress after it is unsaved.
    saved_tick: u64,
    /// The game exists as a save on disk (it was loaded from one or saved at least once).
    on_disk: bool,
    /// The notification list the loaded save was written with (newest first), for the feed.
    restored_notifications: Vec<crate::save::slots::SavedNotification>,
}

impl Session {
    /// Starts a new game (world generation and scenario).
    pub fn new(defs: Arc<Defs>, settings: GameSettings) -> Self {
        let started = Instant::now();
        let state = GameState::new(&defs, settings);
        log::info!(
            "new game: seed {}, {}×{} tiles, {} buildings, created in {:.0} ms",
            state.settings().seed,
            state.world().width(),
            state.world().height(),
            state.buildings().len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        Self {
            defs,
            state,
            queued: Vec::new(),
            events: EventLog::new(EVENT_LOG_CAPACITY),
            timing: TickTiming::new(),
            dev_settlement: None,
            dev_army: None,
            dev_walls: None,
            dev_trade: None,
            dev_case: None,
            dev_view: None,
            play_seconds: 0.0,
            saved_tick: 0,
            on_disk: false,
            restored_notifications: Vec::new(),
        }
    }

    /// Continues a loaded game (fresh command queue and event log) that was played for
    /// `play_seconds` so far.
    pub fn from_state(defs: Arc<Defs>, state: GameState, play_seconds: u64) -> Self {
        log::info!("game continued at {}", state.clock());
        let saved_tick = state.clock().tick;
        Self {
            defs,
            state,
            queued: Vec::new(),
            events: EventLog::new(EVENT_LOG_CAPACITY),
            timing: TickTiming::new(),
            dev_settlement: None,
            dev_army: None,
            dev_walls: None,
            dev_trade: None,
            dev_case: None,
            dev_view: None,
            play_seconds: play_seconds as f64,
            saved_tick,
            on_disk: true,
            restored_notifications: Vec::new(),
        }
    }

    /// Hands the notification list a save was written with to the feed of the continued game.
    pub fn with_restored_notifications(mut self, notifications: Vec<crate::save::slots::SavedNotification>) -> Self {
        self.restored_notifications = notifications;
        self
    }

    /// The notification list of the loaded save (newest first; empty for a new game).
    pub fn restored_notifications(&self) -> &[crate::save::slots::SavedNotification] {
        &self.restored_notifications
    }

    /// Real seconds played in this game.
    pub fn play_seconds(&self) -> u64 {
        self.play_seconds as u64
    }

    /// Adds real playing time.
    pub fn add_play_time(&mut self, seconds: f64) {
        if seconds.is_finite() && seconds > 0.0 {
            self.play_seconds += seconds;
        }
    }

    /// Remembers that the state at `tick` was saved.
    pub fn mark_saved(&mut self, tick: u64) {
        self.saved_tick = tick;
        self.on_disk = true;
    }

    /// True once the game was saved, or if it was loaded from a save.
    pub fn is_on_disk(&self) -> bool {
        self.on_disk
    }

    /// Game time (ticks) played since the last save, load or start.
    pub fn unsaved_ticks(&self) -> u64 {
        self.state.clock().tick.saturating_sub(self.saved_tick)
    }

    /// Starts a new game chosen in the menu (nothing to save yet).
    pub fn start(defs: Arc<Defs>, settings: GameSettings) -> Self {
        let mut session = Session::new(defs, settings);
        session.saved_tick = session.state.clock().tick;
        session
    }

    /// Starts a game as described by the launch options: creates it, applies the developer
    /// switches (unlock research, start the developer settlement), runs the warp ticks and then
    /// forces the developer weather (one tick, so the first frame already shows it).
    /// `generated` is called once the world exists (before the warp; loading screen stages).
    pub fn launch(defs: Arc<Defs>, options: &LaunchOptions, generated: impl FnOnce()) -> Self {
        let mut settings = GameSettings::new(&defs, options.seed, options.world_size);
        if options.dev_crowd.is_some() {
            match defs.scenario_id(dev_crowd::SCENARIO_KEY) {
                Some(scenario) => settings.scenario = scenario,
                None => log::warn!("KINGDOMS_DEV_CROWD: the game data has no benchmark scenario"),
            }
        } else if options.dev_showcase {
            match defs.scenario_id(dev_showcase::SCENARIO_KEY) {
                Some(scenario) => {
                    settings.scenario = scenario;
                    // No event dialog covers the buildings.
                    settings.events_enabled = false;
                }
                None => log::warn!("KINGDOMS_DEV_SHOWCASE: the game data has no showcase scenario"),
            }
        }
        let mut session = Session::new(defs, settings);
        if options.dev_showcase && options.dev_crowd.is_none() {
            session.log_showcase();
        }
        generated();
        if options.dev_unlock {
            session.queue_command(Command::DebugUnlockAllResearch);
        }
        if let Some(plan) = options.dev_settlement {
            session.start_dev_settlement(plan);
        }
        if let Some(soldiers) = options.dev_army {
            let (army, commands) = DevArmy::start(&session.defs, soldiers);
            for command in commands {
                session.queue_command(command);
            }
            session.dev_army = Some(army);
        }
        if let Some(radius) = options.dev_walls {
            let (walls, commands) = DevWalls::start(&session.defs, radius);
            for command in commands {
                session.queue_command(command);
            }
            session.dev_walls = Some(walls);
        }
        if options.dev_trade {
            let (trade, commands) = DevTrade::start(&session.defs);
            for command in commands {
                session.queue_command(command);
            }
            session.dev_trade = Some(trade);
        }
        if options.warp_ticks > 0 {
            session.warp(options.warp_ticks);
        }
        if let Some(case) = options.dev_case {
            session.run_dev_case(case);
        }
        if let Some(w) = options.dev_weather {
            session.queue_command(Command::DebugSetWeather { weather: w.weather, intensity: w.intensity, days: w.days });
            session.tick();
        }
        if let Some(hours) = options.dev_upgrade {
            session.apply_dev_upgrade(hours);
        }
        session.apply_dev_research(&options.dev_research);
        if let Some(dev) = &options.dev_event {
            session.apply_dev_event(dev);
        }
        if let Some(threat) = options.dev_threat {
            session.apply_dev_threat(threat);
        }
        // A fresh game has nothing to lose yet.
        session.saved_tick = session.state.clock().tick;
        session
    }

    /// `KINGDOMS_DEV_CASE`: starts the case and runs the game until its state is reached (at
    /// most [`dev_case::MAX_DAYS`] days); the view it names is kept for the world scene.
    fn run_dev_case(&mut self, case: DevCase) {
        let (plan, commands) = DevCasePlan::start(&self.defs, case);
        for command in commands {
            self.queue_command(command);
        }
        self.dev_case = Some(plan);
        let start = self.state.clock().tick;
        while self.state.clock().tick - start < dev_case::MAX_TICKS {
            self.tick();
            if let Some(view) = self.dev_case.as_ref().and_then(|p| p.reached(&self.state, &self.defs)) {
                log::info!("KINGDOMS_DEV_CASE: '{}' reached at {} (camera {}, cursor {:?})", case.key(), self.state.clock(), view.focus, view.cursor);
                self.dev_view = Some(view);
                break;
            }
        }
        if self.dev_view.is_none() {
            log::warn!("KINGDOMS_DEV_CASE: '{}' not reached within {} days (now {})", case.key(), dev_case::MAX_DAYS, self.state.clock());
        }
        self.dev_case = None;
    }

    /// What the screen shows once the developer case is set up (`KINGDOMS_DEV_CASE`).
    pub fn dev_view(&self) -> Option<&DevView> {
        self.dev_view.as_ref()
    }

    /// `KINGDOMS_DEV_SHOWCASE`: where every building stands (camera targets for
    /// `KINGDOMS_FOCUS`) and which types found no spot.
    fn log_showcase(&self) {
        let state = &self.state;
        for def in self.defs.buildings().iter().filter(|d| dev_showcase::shown(d)) {
            let placed: Vec<String> =
                state.buildings().iter().filter(|(_, b)| b.kind() == def.id).map(|(_, b)| b.entrance().to_string()).collect();
            if placed.is_empty() {
                log::info!("KINGDOMS_DEV_SHOWCASE: {} ({}) has no spot on this map", def.name, def.key);
            } else {
                log::info!("KINGDOMS_DEV_SHOWCASE: {} ({}) at {}", def.name, def.key, placed.join(", "));
            }
        }
    }

    /// `KINGDOMS_DEV_UPGRADE`: grants the costs of every possible upgrade (`DebugGrant`), sends
    /// `UpgradeBuilding` for every finished building with a next level (houses upgrade by
    /// themselves) and runs `hours` game hours so the builders can finish.
    fn apply_dev_upgrade(&mut self, hours: u32) {
        let defs = Arc::clone(&self.defs);
        let targets: Vec<(BuildingId, kingdoms_sim::defs::Cost)> = self
            .state
            .buildings()
            .iter()
            .filter(|(_, b)| b.is_complete() && defs.buildings().get(b.kind().index()).is_some_and(|d| d.housing.is_none()))
            .filter_map(|(id, b)| kingdoms_sim::buildings::upgrade_target(&defs, b.kind()).map(|(u, _)| (id, u.cost.clone())))
            .collect();
        let mut gold = 0u32;
        let mut goods: std::collections::BTreeMap<kingdoms_sim::defs::ResourceId, u32> = std::collections::BTreeMap::new();
        for (_, cost) in &targets {
            gold = gold.saturating_add(cost.gold);
            for m in &cost.materials {
                *goods.entry(m.resource).or_insert(0) += m.amount;
            }
        }
        let goods = goods.into_iter().map(|(resource, amount)| kingdoms_sim::defs::ResourceAmount { resource, amount }).collect();
        self.queue_command(Command::DebugGrant { gold, goods });
        self.tick();
        for (id, _) in &targets {
            self.queue_command(Command::UpgradeBuilding { building: *id });
        }
        self.tick();
        for (id, _) in &targets {
            if let Some(b) = self.state.building(*id).filter(|b| b.is_upgrading()) {
                let name = defs.buildings().get(b.kind().index()).map_or("?", |d| d.name.as_str());
                log::info!("KINGDOMS_DEV_UPGRADE: {name} at {} is being upgraded", b.entrance());
            }
        }
        log::info!("KINGDOMS_DEV_UPGRADE: {} upgrades sent, running {hours} h", targets.len());
        self.warp(u64::from(hours) * kingdoms_sim::time::TICKS_PER_HOUR);
    }

    /// Queues the technologies of `KINGDOMS_DEV_RESEARCH` (regular `QueueResearch` commands,
    /// one tick so that the first one starts and shows).
    fn apply_dev_research(&mut self, keys: &[String]) {
        if keys.is_empty() {
            return;
        }
        for key in keys {
            match self.defs.tech_id(key) {
                Some(tech) => self.queue_command(Command::QueueResearch { tech }),
                None => log::warn!("KINGDOMS_DEV_RESEARCH: unknown technology '{key}'"),
            }
        }
        self.tick();
    }

    /// Sends the enemies of `KINGDOMS_DEV_THREAT` (`DebugSpawnThreat`); a kingdom's army is
    /// preceded by its declaration of war (`Command::Diplomacy`).
    fn apply_dev_threat(&mut self, threat: DevThreat) {
        let found = threat.kingdom.map(|n| (n, self.state.diplomacy().kingdoms().nth(usize::from(n) - 1).map(|(id, k)| (id, k.at_war()))));
        let kingdom = match found {
            Some((n, found)) => match found {
                Some((id, at_war)) => {
                    if !at_war {
                        self.queue_command(Command::Diplomacy { kingdom: id, action: kingdoms_sim::diplomacy::DiplomaticAction::DeclareWar });
                        self.tick();
                    }
                    Some(id)
                }
                None => {
                    log::warn!("KINGDOMS_DEV_THREAT: there is no kingdom {n}");
                    return;
                }
            },
            None => None,
        };
        self.queue_command(Command::DebugSpawnThreat { kingdom, size: threat.size });
        self.tick();
        log::info!("KINGDOMS_DEV_THREAT: {} enemies of {:?} on their way", threat.size, kingdom);
    }

    /// Starts the event of `KINGDOMS_DEV_EVENT` with the developer command and, with a choice,
    /// decides it right away (`ResolveEvent`, one more tick).
    fn apply_dev_event(&mut self, dev: &launch::DevEvent) {
        let Some(event) = self.defs.event_id(&dev.event) else {
            log::warn!("KINGDOMS_DEV_EVENT: unknown event '{}'", dev.event);
            return;
        };
        self.queue_command(Command::DebugTriggerEvent { event });
        self.tick();
        if let Some(p) = self.state.events().pending() {
            log::info!("KINGDOMS_DEV_EVENT: '{}' started (serial {}) at {:?}", dev.event, p.serial, p.position);
        }
        let Some(choice) = dev.choice else { return };
        match self.state.events().pending() {
            Some(p) => {
                let serial = p.serial;
                self.queue_command(Command::ResolveEvent { serial, choice });
                self.tick();
            }
            None => log::warn!("KINGDOMS_DEV_EVENT: '{}' did not start (another event pending?)", dev.event),
        }
    }

    /// The game data.
    pub fn defs(&self) -> &Defs {
        &self.defs
    }

    /// The shared game data (for a session continued from a save).
    pub fn shared_defs(&self) -> Arc<Defs> {
        self.defs.clone()
    }

    /// The simulation state (read-only; changes go through commands).
    pub fn state(&self) -> &GameState {
        &self.state
    }

    /// Queues a command for the next tick.
    pub fn queue_command(&mut self, command: Command) {
        self.queued.push(command);
    }

    /// Commands waiting for the next tick.
    pub fn queued_commands(&self) -> &[Command] {
        &self.queued
    }

    /// Runs one simulation tick with the queued commands and logs its events.
    pub fn tick(&mut self) {
        let commands = std::mem::take(&mut self.queued);
        let started = Instant::now();
        let events = self.state.tick(&self.defs, commands);
        self.timing.record(started.elapsed());
        if let Some(dev) = self.dev_settlement.as_mut() {
            self.queued.extend(dev.after_tick(&self.state, &self.defs, &events));
        }
        if let Some(army) = self.dev_army.as_mut() {
            self.queued.extend(army.after_tick(&self.state, &self.defs, &events));
        }
        if let Some(walls) = self.dev_walls.as_mut() {
            self.queued.extend(walls.after_tick(&self.state, &self.defs, &events));
        }
        if let Some(trade) = self.dev_trade.as_mut() {
            self.queued.extend(trade.after_tick(&self.state, &self.defs));
        }
        if let Some(case) = self.dev_case.as_mut() {
            self.queued.extend(case.after_tick(&self.state, &self.defs));
        }
        let tick = self.state.clock().tick;
        for event in events {
            match &event {
                SimEvent::CommandRejected { command, reason } => log::info!("command rejected: {reason} ({command:?})"),
                SimEvent::RankAchieved { rank } => log::info!("rank {rank} reached at tick {tick}"),
                _ => {}
            }
            self.events.push(tick, event);
        }
    }

    /// Runs `ticks` ticks at once (developer/testing aid, `KINGDOMS_WARP_TICKS`).
    pub fn warp(&mut self, ticks: u64) {
        let started = Instant::now();
        for _ in 0..ticks {
            self.tick();
        }
        log::info!(
            "warped {ticks} ticks in {:.0} ms; now {}",
            started.elapsed().as_secs_f64() * 1000.0,
            self.state.clock()
        );
    }

    /// Queues the developer test settlement (`KINGDOMS_DEV_SETTLEMENT`).
    pub fn start_dev_settlement(&mut self, plan: DevPlan) {
        let (dev, commands) = DevSettlement::start(&self.state, &self.defs, plan);
        for command in commands {
            self.queue_command(command);
        }
        self.dev_settlement = Some(dev);
    }

    /// Events of the recent ticks.
    pub fn events(&self) -> &EventLog {
        &self.events
    }

    /// Measured tick durations.
    pub fn tick_timing(&self) -> &TickTiming {
        &self.timing
    }

    /// Map chunks whose tiles changed since the last call (for the renderers' caches).
    pub fn take_dirty_chunks(&mut self) -> Vec<IVec2> {
        self.state.take_dirty_chunks()
    }

    /// Map chunks whose roads, walls, gates or building footprints changed since the last call
    /// (see [`GameState::take_structure_dirty_chunks`]).
    pub fn take_structure_dirty_chunks(&mut self) -> Vec<IVec2> {
        self.state.take_structure_dirty_chunks()
    }

    /// Map chunks whose field tiles changed visibly since the last call (see
    /// [`GameState::take_field_dirty_chunks`]).
    pub fn take_field_dirty_chunks(&mut self) -> Vec<IVec2> {
        self.state.take_field_dirty_chunks()
    }

    /// Chunks whose kingdom borders or AI structures changed since the last call (see
    /// [`GameState::take_land_dirty_chunks`]).
    pub fn take_land_dirty_chunks(&mut self) -> Vec<IVec2> {
        self.state.take_land_dirty_chunks()
    }

    /// The kingdom's first headquarters (town hall or castle).
    pub fn headquarters(&self) -> Option<BuildingId> {
        self.state.buildings().with_role(&self.defs, BuildingRole::Headquarters).into_iter().next()
    }

    /// Tile the camera centres on for "home": the headquarters, else the world's start point.
    pub fn home_tile(&self) -> IVec2 {
        self.headquarters()
            .and_then(|id| self.state.building(id))
            .map(|b| b.rect().origin + b.rect().size / 2)
            .unwrap_or(self.state.world().start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_sim::world::WorldSize;

    fn defs() -> Arc<Defs> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => Arc::new(defs),
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    #[test]
    fn queued_commands_reach_the_simulation_once() {
        let defs = defs();
        let mut session = Session::new(defs.clone(), GameSettings::new(&defs, 3, WorldSize::Small));
        let mut cursor = session.events().cursor_at_end();
        session.queue_command(Command::SetTaxRate { percent: 12 });
        assert_eq!(session.queued_commands().len(), 1);
        session.tick();
        assert!(session.queued_commands().is_empty());
        assert_eq!(session.state().policy().tax_rate_percent, 12);
        let read = session.events().read(&mut cursor);
        assert!(!read.missed);
        let events: Vec<_> = read.events.collect();
        assert!(events.iter().any(|e| matches!(e.event, SimEvent::TaxRateChanged { percent: 12 })));
        assert!(events.iter().all(|e| e.tick == 1));
        assert!(session.tick_timing().last_ms() >= 0.0);
    }

    /// The game has no event dialog yet: an event nobody decides must not block anything but
    /// expire to its default choice after the decision time.
    #[test]
    fn undecided_events_expire_to_their_default_choice() {
        let defs = defs();
        let mut session = Session::new(defs.clone(), GameSettings::new(&defs, 7, WorldSize::Small));
        let Some(event) = defs.event_id("drought") else { panic!("the shipped data has the drought event") };
        let mut cursor = session.events().cursor_at_end();
        session.queue_command(Command::DebugTriggerEvent { event });
        session.tick();
        let Some(serial) = session.state().events().pending().map(|p| p.serial) else { panic!("the event waits for a decision") };
        for _ in 0..defs.rules().events.decision_ticks() {
            session.tick();
        }
        assert!(session.state().events().pending().is_none());
        let read = session.events().read(&mut cursor);
        let events: Vec<_> = read.events.map(|e| e.event.clone()).collect();
        assert!(events.iter().any(|e| matches!(e, SimEvent::EventResolved { serial: s, choice: 0, by_default: true, .. } if *s == serial)));
        // Start and outcome reach the notification feed.
        let notes = events.iter().filter(|e| matches!(e, SimEvent::Notification(n) if n.text.starts_with("Dürre"))).count();
        assert_eq!(notes, 2);
    }

    #[test]
    fn launch_applies_the_developer_switches() {
        use kingdoms_sim::defs::Weather;

        let defs = defs();
        let options = LaunchOptions {
            seed: 7,
            world_size: WorldSize::Small,
            warp_ticks: 10,
            dev_unlock: true,
            dev_weather: Some(launch::DevWeather { weather: Weather::Snow, intensity: 70, days: 2 }),
            ..Default::default()
        };
        let session = Session::launch(defs.clone(), &options, || {});
        let state = session.state();
        // The warp plus the one tick that applies the weather.
        assert_eq!(state.clock().tick, 11);
        assert_eq!((state.weather().current(), state.weather().intensity()), (Weather::Snow, 70));
        assert!(state.weather().forced().is_some());
        assert_eq!(state.research().researched().count(), defs.techs().len());
        assert!(session.events().recent().all(|e| !matches!(e.event, SimEvent::CommandRejected { .. })));
    }

    #[test]
    fn launch_starts_research_and_events_through_commands() {
        let defs = defs();
        let options = LaunchOptions {
            seed: 7,
            world_size: WorldSize::Small,
            dev_research: vec!["grain_farming".into(), "improved_plows".into(), "no_such_tech".into()],
            dev_event: Some(launch::DevEvent { event: "merchant_visit".into(), choice: Some(0) }),
            ..Default::default()
        };
        let session = Session::launch(defs.clone(), &options, || {});
        let state = session.state();
        let queue: Vec<String> = state.research().queue().iter().map(|t| defs.techs()[t.index()].key.clone()).collect();
        assert_eq!(queue, vec!["grain_farming", "improved_plows"]);
        // The event started and was decided with its first choice.
        assert!(state.events().pending().is_none());
        assert_eq!(state.events().history().count(), 1);
        assert!(session.events().recent().all(|e| !matches!(e.event, SimEvent::CommandRejected { .. })));
    }

    #[test]
    fn launch_warps_and_builds_the_dev_settlement_through_commands() {
        let defs = defs();
        let options =
            LaunchOptions { seed: 7, world_size: WorldSize::Small, warp_ticks: 3, dev_settlement: Some(DevPlan::Showcase), ..Default::default() };
        let start_buildings = Session::new(defs.clone(), GameSettings::new(&defs, 7, WorldSize::Small)).state().buildings().len();
        let session = Session::launch(defs, &options, || {});
        assert_eq!(session.state().clock().tick, 3);
        let placed = session.events().recent().filter(|e| matches!(e.event, SimEvent::BuildingPlaced { .. })).count();
        assert!(placed >= 2, "dev settlement placed {placed} buildings");
        assert_eq!(session.state().buildings().len(), start_buildings + placed);
        assert!(session.events().recent().any(|e| matches!(e.event, SimEvent::RoadsBuilt { .. })));
        let home = session.home_tile();
        assert!(session.state().world().in_bounds(home));
    }
}
