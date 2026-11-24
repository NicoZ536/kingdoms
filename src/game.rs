//! Game-wide shared state and scene stack construction.
//!
//! Start: the game data, settings, fonts and theme load during initialisation; the boot scene
//! then decodes the sprite atlases on worker threads behind a loading screen and continues with
//! the main menu – or, when developer or test variables ask for it (see
//! [`StartMode`](crate::session::StartMode)), straight with a game.

use std::sync::Arc;

use anyhow::Context as _;
use kingdoms_engine::core::app::EngineContext;
use kingdoms_engine::core::scene::{Scene, SceneStack};
use kingdoms_engine::egui;
use kingdoms_engine::input::ActionMap;
use kingdoms_sim::defs::Defs;

use crate::actions::Action;
use crate::scenes::boot::BootScene;
use crate::session::StartMode;
use crate::settings::{self, Settings};
use crate::ui::menu::settings_panel::{SettingsContext, SettingsPanel};

/// State shared by all scenes.
pub struct GameShared {
    pub actions: ActionMap<Action>,
    pub show_debug: bool,
    /// The player's settings (`settings.ron`); engine-level ones live in `ctx.config`.
    pub settings: Settings,
    /// Audio files present under `assets/audio` (the settings mention when there are none).
    pub audio_files: usize,
    /// The settings panel changed settings that are not written yet. They apply at once and are
    /// saved when the panel closes – or by [`GameShared::flush_settings`] on every other way
    /// out: the window's close button, Cmd+Q, quitting the game.
    settings_unsaved: bool,
    /// Game data, loaded once at startup (`None` only before initialisation).
    defs: Option<Arc<Defs>>,
}

impl GameShared {
    /// The validated game data.
    pub fn defs(&self) -> anyhow::Result<Arc<Defs>> {
        self.defs.clone().context("game data has not been loaded")
    }

    /// Writes `settings.ron` and the engine part (`engine.ron`).
    pub fn save_settings(&self, ctx: &EngineContext) {
        let path = ctx.paths.settings_file();
        match self.settings.save(&path) {
            Ok(()) => log::info!("settings saved to {}", path.display()),
            Err(err) => log::error!("cannot save the settings: {err}"),
        }
        settings::save_engine_config(&ctx.config, &settings::engine_config_path(&ctx.paths));
    }

    /// Writes the settings if the panel changed them since the last save.
    pub fn flush_settings(&mut self, ctx: &EngineContext) {
        if std::mem::take(&mut self.settings_unsaved) {
            self.save_settings(ctx);
        }
    }

    /// Shows the settings panel in `ui`, applies changes to the engine right away and saves
    /// the files when it closes. Returns true once it closed.
    pub fn settings_panel(&mut self, panel: &mut SettingsPanel, ui: &mut egui::Ui, ctx: &mut EngineContext) -> bool {
        let msaa_supported = ctx.renderer.msaa_supported();
        let audio_device = ctx.audio.is_available();
        let ui_scale_limit = ctx.ui_scale_limit();
        let raw_keys = ctx.input.raw_keys_pressed().to_vec();
        let modifiers = ctx.input.modifiers();
        let changes = {
            let mut cx = SettingsContext {
                settings: &mut self.settings,
                actions: &mut self.actions,
                engine: &mut ctx.config,
                msaa_supported,
                audio_device,
                audio_files: self.audio_files,
                ui_scale_limit,
                raw_keys: &raw_keys,
                modifiers,
            };
            panel.show(ui, &mut cx)
        };
        if changes.audio {
            ctx.audio.set_settings(ctx.config.audio.clone());
        }
        if changes.graphics {
            ctx.config.sanitize();
            ctx.apply_graphics_config();
        }
        self.settings_unsaved |= panel.take_dirty();
        if changes.close {
            self.flush_settings(ctx);
            ctx.audio.play("ui_close");
        }
        changes.close
    }
}

pub fn create() -> SceneStack<GameShared> {
    let settings = Settings::default();
    let shared = GameShared {
        actions: settings.action_map(),
        show_debug: false,
        settings,
        audio_files: 0,
        settings_unsaved: false,
        defs: None,
    };
    // Settings changed in a panel that is still open when the game ends are written too.
    SceneStack::new(shared, init_shared, initial_scene).with_exit_hook(|shared: &mut GameShared, ctx| shared.flush_settings(ctx))
}

fn init_shared(shared: &mut GameShared, ctx: &mut EngineContext) -> anyhow::Result<()> {
    shared.show_debug = ctx.config.debug.show_debug_hud || std::env::var_os("KINGDOMS_DEBUG").is_some();
    shared.settings = Settings::load_or_default(&ctx.paths.settings_file());
    shared.actions = shared.settings.action_map();
    shared.audio_files = count_audio_files(&ctx.paths.assets.join("audio"));
    if std::env::var_os("KINGDOMS_DEBUG").is_some() {
        // Which audio files were found under another name or format, which ids lack files.
        ctx.audio.log_delivery();
    }
    // The error's message lists every problem found in the data files.
    let defs = Defs::load(&ctx.paths.data)
        .with_context(|| format!("cannot load the game data from {}", ctx.paths.data.display()))?;
    shared.defs = Some(Arc::new(defs));
    // The HUD is laid out for 1280 × 720 points: a large UI scale on a small window is lowered.
    ctx.set_ui_min_size(Some(crate::ui::theme::LAYOUT_MIN_SIZE));
    crate::ui::theme::install_fonts(ctx.egui(), &ctx.paths.assets.join("fonts"));
    crate::ui::theme::apply(ctx.egui());
    // No save can be running yet: unfinished temporary files of a crash can go.
    crate::save::slots::remove_stale_temp_files(&ctx.paths.saves_dir());
    Ok(())
}

fn initial_scene(_shared: &mut GameShared, ctx: &mut EngineContext) -> anyhow::Result<Box<dyn Scene<GameShared>>> {
    let start = StartMode::from_env();
    log::info!("start: {start:?}");
    Ok(Box::new(BootScene::new(ctx, start)))
}

/// Number of files in the audio folders (sound effects, music, ambience, voices).
fn count_audio_files(dir: &std::path::Path) -> usize {
    let Ok(read) = std::fs::read_dir(dir) else { return 0 };
    read.filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|p| std::fs::read_dir(p).ok())
        .map(|r| r.filter_map(Result::ok).filter(|e| e.path().is_file()).count())
        .sum()
}
