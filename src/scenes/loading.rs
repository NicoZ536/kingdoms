//! Loading screen for starting or loading a game: world generation (or reading and checking a
//! save) runs on a worker thread while the screen shows the stages; the map view is then built
//! on the main thread and the world scene takes over. Errors are shown in German with a way
//! back to the main menu.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use kingdoms_engine::core::app::EngineContext;
use kingdoms_engine::core::scene::{Scene, Transition};
use kingdoms_engine::egui;
use kingdoms_engine::rendering::camera::Camera2D;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::sprite::RenderList;
use kingdoms_sim::defs::Defs;
use kingdoms_sim::save::{SaveError, decode};
use kingdoms_sim::state::GameSettings;

use crate::game::GameShared;
use crate::save;
use crate::scenes::main_menu::MainMenuScene;
use crate::scenes::world::WorldScene;
use crate::session::{LaunchOptions, Session};
use crate::ui::menu::loading_screen::{self, LoadingView, StageState};

/// What is being prepared.
enum Job {
    /// A game from launch options (developer variables; boxed: the options are large).
    Launch(Box<LaunchOptions>),
    /// A new game chosen in the menu.
    NewGame(GameSettings),
    /// A save game.
    Load { path: PathBuf, name: String },
}

/// The worker's result: the session plus what the world scene needs to show it.
struct Prepared {
    session: Session,
    options: LaunchOptions,
    toast: Option<String>,
}

pub struct LoadingScene {
    heading: String,
    stages: Vec<String>,
    /// Stage the worker is in (the last stage, building the map, runs on the main thread).
    stage: Arc<AtomicUsize>,
    receiver: Option<mpsc::Receiver<Result<Prepared, String>>>,
    prepared: Option<Prepared>,
    /// Frames shown since the worker finished (the map is built after one frame showing it).
    build_frames: u8,
    error: Option<String>,
    started: f64,
}

impl LoadingScene {
    /// Starts the game the launch options describe (developer and test runs).
    pub fn launch(ctx: &mut EngineContext, defs: Arc<Defs>, options: LaunchOptions) -> Self {
        let mut stages = vec!["Landschaft und Startsiedlung werden erzeugt".to_owned()];
        if options.warp_ticks > 0 || options.dev_settlement.is_some() {
            stages.push("Die Zeit wird vorgespult".to_owned());
        }
        let data_dir = ctx.paths.data.clone();
        Self::spawn(ctx, "Spiel wird gestartet", stages, Job::Launch(Box::new(options)), defs, data_dir)
    }

    /// Starts a new game chosen in the menu.
    pub fn new_game(ctx: &mut EngineContext, defs: Arc<Defs>, settings: GameSettings) -> Self {
        let stages = vec!["Landschaft und Startsiedlung werden erzeugt".to_owned()];
        let data_dir = ctx.paths.data.clone();
        Self::spawn(ctx, "Neues Spiel", stages, Job::NewGame(settings), defs, data_dir)
    }

    /// Loads a save game.
    pub fn load(ctx: &mut EngineContext, defs: Arc<Defs>, path: PathBuf, name: String) -> Self {
        let stages = vec!["Spielstand wird gelesen".to_owned(), "Spielstand wird geprüft".to_owned()];
        let heading = format!("Lade „{name}“");
        let data_dir = ctx.paths.data.clone();
        Self::spawn(ctx, &heading, stages, Job::Load { path, name }, defs, data_dir)
    }

    fn spawn(ctx: &mut EngineContext, heading: &str, mut stages: Vec<String>, job: Job, defs: Arc<Defs>, data_dir: PathBuf) -> Self {
        stages.push("Karte wird aufgebaut".to_owned());
        let stage = Arc::new(AtomicUsize::new(0));
        let (sender, receiver) = mpsc::channel();
        let worker_stage = stage.clone();
        let spawned = std::thread::Builder::new().name("game-loader".into()).spawn(move || {
            let result = prepare(job, defs, &data_dir, &worker_stage);
            let _ = sender.send(result);
        });
        let (receiver, error) = match spawned {
            Ok(_) => (Some(receiver), None),
            Err(err) => {
                log::error!("cannot start the loading thread: {err}");
                (None, Some("Das Spiel konnte nicht vorbereitet werden (kein Hintergrund-Thread verfügbar).".to_owned()))
            }
        };
        Self {
            heading: heading.to_owned(),
            stages,
            stage,
            receiver,
            prepared: None,
            build_frames: 0,
            error,
            started: ctx.time.elapsed(),
        }
    }
}

/// Runs on the worker thread.
fn prepare(job: Job, defs: Arc<Defs>, data_dir: &std::path::Path, stage: &AtomicUsize) -> Result<Prepared, String> {
    let started = std::time::Instant::now();
    let prepared = match job {
        Job::Launch(options) => {
            let defs = match options.dev_crowd {
                Some(settlers) => crowd_defs(&defs, data_dir, settlers, &options).map_err(|err| {
                    log::error!("KINGDOMS_DEV_CROWD: {err:#}");
                    "Die Messstadt konnte nicht angelegt werden.".to_owned()
                })?,
                None if options.dev_showcase => showcase_defs(&defs, data_dir, &options).map_err(|err| {
                    log::error!("KINGDOMS_DEV_SHOWCASE: {err:#}");
                    "Die Gebäudeschau konnte nicht angelegt werden.".to_owned()
                })?,
                None => defs,
            };
            let warps = options.warp_ticks > 0 || options.dev_settlement.is_some();
            let session = Session::launch(defs, &options, || {
                if warps {
                    stage.store(1, Ordering::Relaxed);
                }
            });
            Prepared { session, options: *options, toast: None }
        }
        Job::NewGame(settings) => {
            let session = Session::start(defs, settings.clone());
            let options = LaunchOptions { seed: settings.seed, world_size: settings.world_size, ..LaunchOptions::default() };
            Prepared { session, options, toast: Some("Willkommen in Eurem neuen Reich!".to_owned()) }
        }
        Job::Load { path, name } => {
            let bytes = std::fs::read(&path).map_err(|source| load_error(&SaveError::Io { path: path.clone(), source }))?;
            stage.store(1, Ordering::Relaxed);
            let game = decode(&bytes, &defs).map_err(|err| {
                log::warn!("cannot load {}: {err}", path.display());
                load_error(&err)
            })?;
            let (play_seconds, notifications) = save::trusted_meta(&path, &game.header);
            let toast = format!("Spielstand geladen: {name} ({})", game.header.summary.date);
            let options = LaunchOptions { seed: game.header.seed, world_size: game.header.summary.world_size, ..LaunchOptions::default() };
            let session = Session::from_state(defs, game.state, play_seconds).with_restored_notifications(notifications);
            Prepared { session, options, toast: Some(toast) }
        }
    };
    log::info!("game prepared in {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
    Ok(prepared)
}

fn load_error(err: &SaveError) -> String {
    format!("Der Spielstand kann nicht geladen werden: {}", save::describe_error(err))
}

/// The game data plus the developer benchmark scenario, laid out around the start point of the
/// world the options generate (`KINGDOMS_DEV_CROWD`).
fn crowd_defs(defs: &Defs, data_dir: &std::path::Path, settlers: u16, options: &LaunchOptions) -> anyhow::Result<Arc<Defs>> {
    let probe = kingdoms_sim::state::GameState::new(defs, GameSettings::new(defs, options.seed, options.world_size));
    let world = probe.world();
    let crowd = crate::session::dev_crowd::crowd_defs(data_dir, settlers, world.start, world.size())?;
    log::info!("KINGDOMS_DEV_CROWD: benchmark town with {settlers} settlers");
    Ok(Arc::new(crowd))
}

/// The game data plus the developer showcase of every building, laid out around the start
/// point of the world the options generate (`KINGDOMS_DEV_SHOWCASE`).
fn showcase_defs(defs: &Defs, data_dir: &std::path::Path, options: &LaunchOptions) -> anyhow::Result<Arc<Defs>> {
    let probe = kingdoms_sim::state::GameState::new(defs, GameSettings::new(defs, options.seed, options.world_size));
    let world = probe.world();
    let showcase = crate::session::dev_showcase::showcase_defs(defs, data_dir, world.start, world.size())?;
    log::info!("KINGDOMS_DEV_SHOWCASE: every building around a castle");
    Ok(Arc::new(showcase))
}

impl Scene<GameShared> for LoadingScene {
    fn name(&self) -> &'static str {
        "loading"
    }

    fn on_enter(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) {
        ctx.set_loading(true);
    }

    fn on_exit(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) {
        ctx.set_loading(false);
    }

    fn update(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) -> Transition<GameShared> {
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(Ok(prepared)) => {
                    self.receiver = None;
                    self.prepared = Some(prepared);
                    self.stage.store(self.stages.len() - 1, Ordering::Relaxed);
                }
                Ok(Err(message)) => {
                    self.receiver = None;
                    self.error = Some(message);
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    log::error!("the loading thread ended without a result");
                    self.error = Some("Beim Vorbereiten des Spiels ist ein Fehler aufgetreten (Details im Protokoll).".to_owned());
                }
            }
        }
        if self.prepared.is_some() {
            // One frame shows "Karte wird aufgebaut" before the main thread is busy with it.
            self.build_frames += 1;
            if self.build_frames >= 2
                && let Some(prepared) = self.prepared.take()
            {
                let started = std::time::Instant::now();
                let mut world = WorldScene::new(ctx, prepared.session, &prepared.options);
                log::info!("map view built in {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
                if let Some(toast) = prepared.toast {
                    world.toast(ctx, toast);
                }
                return Transition::Replace(Box::new(world));
            }
        }
        Transition::None
    }

    fn ui(&mut self, shared: &mut GameShared, ctx: &mut EngineContext, ui: &mut egui::Ui) -> Transition<GameShared> {
        let current = self.stage.load(Ordering::Relaxed);
        let stages = self
            .stages
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let state = match i.cmp(&current) {
                    std::cmp::Ordering::Less => StageState::Done,
                    std::cmp::Ordering::Equal => StageState::Active,
                    std::cmp::Ordering::Greater => StageState::Waiting,
                };
                (s.clone(), state)
            })
            .collect();
        let view = LoadingView {
            heading: &self.heading,
            stages,
            progress: None,
            elapsed: ctx.time.elapsed() - self.started,
            error: self.error.as_deref(),
        };
        let back = loading_screen::show(ui.ctx(), &view, &shared.actions);
        ui.ctx().request_repaint();
        if back && let Ok(defs) = shared.defs() {
            ctx.set_loading(false);
            return Transition::Replace(Box::new(MainMenuScene::new(shared, ctx, defs)));
        }
        Transition::None
    }

    fn render(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) -> Option<RenderList> {
        let mut list = ctx.renderer.begin_list(Camera2D::new(ctx.window_size(), ctx.scale_factor()));
        list.clear_color = Color::from_srgb_u8(20, 14, 9, 255);
        Some(list)
    }
}
