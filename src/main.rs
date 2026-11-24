//! KINGDOMS – Rise of an Empire
//!
//! Entry point: resolves directories, sets up logging and configuration, then hands control to
//! the engine's main loop with the game's scene stack.

mod actions;
mod audio_report;
mod game;
mod save;
mod scenes;
mod session;
mod settings;
mod sound;
mod tools;
mod ui;
mod view;

use std::process::ExitCode;

use kingdoms_engine::core::config::EngineConfig;
use kingdoms_engine::core::paths::GamePaths;
use kingdoms_engine::core::{app, logging};

/// Window icon on Linux and Windows (the macOS bundle has its own `.icns`).
const APP_ICON: &str = "app_icon.png";

pub const APP_NAME: &str = "Kingdoms";

fn main() -> ExitCode {
    let paths = match GamePaths::discover(APP_NAME) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };
    let args: Vec<String> = std::env::args().collect();
    if audio_report::requested(std::env::var(audio_report::ENV_VAR).ok().as_deref(), &args) {
        return audio_report::run(&paths.assets.join("audio"));
    }
    let log = logging::init(Some(&paths.logs_dir().join("kingdoms.log")));
    log::info!("KINGDOMS {} starting", env!("CARGO_PKG_VERSION"));
    log::info!("content: {}  user data: {}", paths.root.display(), paths.user.display());

    let config_path = settings::engine_config_path(&paths);
    let mut config = EngineConfig::load_or_default(&config_path);
    config.window.icon = Some(APP_ICON.to_owned());

    match app::run(config, paths, log, game::create) {
        Ok(()) => {
            log::info!("goodbye");
            ExitCode::SUCCESS
        }
        Err(err) => {
            log::error!("fatal error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
