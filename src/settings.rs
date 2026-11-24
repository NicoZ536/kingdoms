//! The player's game settings (`settings.ron` in the user directory): gameplay (autosave,
//! automatic pause, camera, notification filter), display details the engine does not know
//! (particle density, day and night) and the key bindings.
//!
//! Engine-level settings – window mode, vsync, MSAA, UI scale and the audio volumes – stay in the
//! engine's `engine.ron` ([`EngineConfig`]); the settings window edits both files and applies
//! every change right away. Missing fields fall back to the defaults, unknown actions in the key
//! list are ignored, and actions without a binding get their default keys.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use kingdoms_engine::core::config::EngineConfig;
use kingdoms_engine::input::{ActionMap, Binding};
use kingdoms_sim::event::Severity;
use kingdoms_sim::time::TICKS_PER_DAY;
use serde::{Deserialize, Serialize};

use crate::actions::{Action, MAX_BINDINGS, default_bindings};

/// When the running game is saved automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutosaveInterval {
    Off,
    /// Every n game days (counted in simulation ticks).
    GameDays(u8),
    /// Every n minutes of real playing time (paused time does not count).
    RealMinutes(u16),
}

impl AutosaveInterval {
    /// The choices offered in the settings.
    pub const CHOICES: [AutosaveInterval; 8] = [
        AutosaveInterval::Off,
        AutosaveInterval::RealMinutes(5),
        AutosaveInterval::RealMinutes(10),
        AutosaveInterval::RealMinutes(15),
        AutosaveInterval::RealMinutes(30),
        AutosaveInterval::GameDays(1),
        AutosaveInterval::GameDays(3),
        AutosaveInterval::GameDays(7),
    ];

    /// German label ("alle 10 Minuten", "jeden Spieltag" …).
    pub fn label(self) -> String {
        match self {
            AutosaveInterval::Off => "Aus".to_owned(),
            AutosaveInterval::GameDays(1) => "Jeden Spieltag".to_owned(),
            AutosaveInterval::GameDays(n) => format!("Alle {n} Spieltage"),
            AutosaveInterval::RealMinutes(1) => "Jede Minute".to_owned(),
            AutosaveInterval::RealMinutes(n) => format!("Alle {n} Minuten"),
        }
    }

    /// Simulation ticks between autosaves (`GameDays`).
    pub fn ticks(self) -> Option<u64> {
        match self {
            AutosaveInterval::GameDays(n) => Some(u64::from(n.max(1)) * TICKS_PER_DAY),
            _ => None,
        }
    }

    /// Real seconds of play between autosaves (`RealMinutes`).
    pub fn seconds(self) -> Option<f64> {
        match self {
            AutosaveInterval::RealMinutes(n) => Some(f64::from(n.max(1)) * 60.0),
            _ => None,
        }
    }
}

/// Which notifications the feed shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationFilter {
    All,
    /// Good news, warnings and dangers (no plain information).
    Important,
    /// Warnings and dangers only.
    Warnings,
}

impl NotificationFilter {
    pub const ALL: [NotificationFilter; 3] = [NotificationFilter::All, NotificationFilter::Important, NotificationFilter::Warnings];

    pub const fn label(self) -> &'static str {
        match self {
            NotificationFilter::All => "Alle Meldungen",
            NotificationFilter::Important => "Ohne reine Informationen",
            NotificationFilter::Warnings => "Nur Warnungen und Gefahren",
        }
    }

    /// True if a notification of `severity` is shown.
    pub fn shows(self, severity: Severity) -> bool {
        match self {
            NotificationFilter::All => true,
            NotificationFilter::Important => severity != Severity::Info,
            NotificationFilter::Warnings => matches!(severity, Severity::Warning | Severity::Danger),
        }
    }
}

/// How many particles (smoke, dust, leaves, rain, snow) the map shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParticleDensity {
    Off,
    Low,
    Medium,
    High,
}

impl ParticleDensity {
    pub const ALL: [ParticleDensity; 4] = [ParticleDensity::Off, ParticleDensity::Low, ParticleDensity::Medium, ParticleDensity::High];

    pub const fn label(self) -> &'static str {
        match self {
            ParticleDensity::Off => "Aus",
            ParticleDensity::Low => "Niedrig",
            ParticleDensity::Medium => "Mittel",
            ParticleDensity::High => "Hoch",
        }
    }

    /// Factor on every emitter's spawn rate.
    pub const fn factor(self) -> f32 {
        match self {
            ParticleDensity::Off => 0.0,
            ParticleDensity::Low => 0.35,
            ParticleDensity::Medium => 0.65,
            ParticleDensity::High => 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameplaySettings {
    pub autosave: AutosaveInterval,
    /// Pause when an event waits for a decision.
    pub pause_on_event: bool,
    /// Pause on a notification of the highest severity (fire, plague, raid, bankruptcy …).
    pub pause_on_danger: bool,
    /// The camera moves when the mouse touches the screen edge.
    pub edge_scroll: bool,
    /// Factor on the keyboard and edge pan speed (0.5–2).
    pub camera_speed: f32,
    /// Smooth two-finger scrolling (trackpad) moves the map instead of zooming; zoom with a
    /// pinch, the wheel or the keys. On by default on macOS.
    pub trackpad_pan: bool,
    pub notifications: NotificationFilter,
}

impl Default for GameplaySettings {
    fn default() -> Self {
        Self {
            autosave: AutosaveInterval::RealMinutes(10),
            pause_on_event: false,
            pause_on_danger: false,
            edge_scroll: true,
            camera_speed: 1.0,
            trackpad_pan: cfg!(target_os = "macos"),
            notifications: NotificationFilter::All,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    pub particles: ParticleDensity,
    /// Light follows the time of day; off: always daylight, no window lights.
    pub day_night: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self { particles: ParticleDensity::High, day_night: true }
    }
}

/// Everything in `settings.ron`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub game: GameplaySettings,
    pub display: DisplaySettings,
    /// Key bindings per action (sorted, so the file is stable).
    pub controls: BTreeMap<Action, Vec<Binding>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { game: GameplaySettings::default(), display: DisplaySettings::default(), controls: bindings_to_map(&default_bindings()) }
    }
}

/// Errors reading or writing the settings file.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("cannot access {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("invalid settings file {path}: {source}")]
    Parse { path: PathBuf, source: Box<ron::error::SpannedError> },
    #[error("cannot serialize the settings: {0}")]
    Serialize(#[from] ron::Error),
}

impl Settings {
    /// Reads `path`; a missing or broken file gives the defaults (logged).
    pub fn load_or_default(path: &Path) -> Self {
        if !path.exists() {
            log::info!("no settings at {}, using defaults", path.display());
            return Self::default();
        }
        match Self::load(path) {
            Ok(settings) => {
                log::info!("loaded settings from {}", path.display());
                settings
            }
            Err(err) => {
                log::warn!("{err}; using default settings");
                Self::default()
            }
        }
    }

    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let text = std::fs::read_to_string(path).map_err(|source| SettingsError::Io { path: path.to_path_buf(), source })?;
        let mut settings: Settings =
            ron::from_str(&text).map_err(|source| SettingsError::Parse { path: path.to_path_buf(), source: Box::new(source) })?;
        settings.sanitize();
        Ok(settings)
    }

    /// Writes the settings atomically (temporary file, flushed to disk, renamed over the old one).
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
        write_text_atomic(path, &text).map_err(|(path, source)| SettingsError::Io { path, source })
    }

    /// Clamps values and repairs the key list: unknown or mouse bindings of rebindable actions
    /// are kept as they are, missing actions get their defaults, overlong lists are cut.
    pub fn sanitize(&mut self) {
        let g = &mut self.game;
        g.camera_speed = if g.camera_speed.is_finite() { g.camera_speed.clamp(0.5, 2.0) } else { 1.0 };
        if let AutosaveInterval::GameDays(0) = g.autosave {
            g.autosave = AutosaveInterval::GameDays(1);
        }
        if let AutosaveInterval::RealMinutes(0) = g.autosave {
            g.autosave = AutosaveInterval::RealMinutes(1);
        }
        let defaults = default_bindings();
        for action in Action::ALL {
            let list = self.controls.entry(action).or_insert_with(|| defaults.bindings(action).to_vec());
            if !action.rebindable() {
                // Fixed actions always use their defaults (an edited file cannot take Esc away).
                *list = defaults.bindings(action).to_vec();
            }
            list.dedup();
            list.truncate(MAX_BINDINGS);
        }
    }

    /// The key bindings as the engine's action map.
    pub fn action_map(&self) -> ActionMap<Action> {
        let mut map = ActionMap::new();
        for (action, list) in &self.controls {
            for binding in list {
                map.bind(*action, *binding);
            }
        }
        map.merge_missing(&default_bindings());
        map
    }

    /// Stores `map` as the key list.
    pub fn set_bindings(&mut self, map: &ActionMap<Action>) {
        self.controls = bindings_to_map(map);
    }
}

fn bindings_to_map(map: &ActionMap<Action>) -> BTreeMap<Action, Vec<Binding>> {
    Action::ALL.into_iter().map(|a| (a, map.bindings(a).to_vec())).collect()
}

/// Path of the engine configuration (`engine.ron` in the user directory).
pub fn engine_config_path(paths: &kingdoms_engine::core::paths::GamePaths) -> PathBuf {
    paths.user.join("engine.ron")
}

/// The audio and graphics part of the engine configuration the settings window edits, saved
/// to `engine.ron` (other engine values stay as they are in the file).
pub fn save_engine_config(config: &EngineConfig, path: &Path) {
    match config.save(path) {
        Ok(()) => log::info!("engine settings saved to {}", path.display()),
        Err(err) => log::error!("cannot save the engine settings: {err}"),
    }
}

/// Writes `text` to a temporary file next to `path`, flushes it to disk and renames it over
/// `path`. On error the temporary file is removed.
pub fn write_text_atomic(path: &Path, text: &str) -> Result<(), (PathBuf, std::io::Error)> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| (parent.to_path_buf(), e))?;
    }
    let mut tmp_name = path.file_name().map(std::ffi::OsString::from).unwrap_or_default();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    let result = (|| {
        let mut file = std::fs::File::create(&tmp).map_err(|e| (tmp.clone(), e))?;
        file.write_all(text.as_bytes()).map_err(|e| (tmp.clone(), e))?;
        file.sync_all().map_err(|e| (tmp.clone(), e))?;
        drop(file);
        std::fs::rename(&tmp, path).map_err(|e| (path.to_path_buf(), e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use kingdoms_engine::input::{KeyCode, Modifiers};

    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kingdoms-settings-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("settings.ron")
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let path = temp_file("roundtrip");
        let mut settings = Settings::default();
        settings.game.autosave = AutosaveInterval::GameDays(3);
        settings.game.pause_on_event = true;
        settings.game.camera_speed = 1.5;
        settings.game.notifications = NotificationFilter::Warnings;
        settings.display.particles = ParticleDensity::Low;
        settings.display.day_night = false;
        let mut map = settings.action_map();
        map.clear(Action::Rotate);
        map.bind(Action::Rotate, Binding::KeyWith(KeyCode::KeyE, Modifiers { ctrl: true, ..Modifiers::NONE }));
        settings.set_bindings(&map);
        settings.save(&path).expect("save");
        assert!(!path.with_file_name("settings.ron.tmp").exists());
        let loaded = Settings::load(&path).expect("load");
        assert_eq!(loaded, settings);
        assert_eq!(loaded.action_map().bindings(Action::Rotate), map.bindings(Action::Rotate));
        let _ = std::fs::remove_dir_all(path.parent().unwrap_or(&path));
    }

    #[test]
    fn partial_and_broken_files_fall_back_to_defaults() {
        let path = temp_file("partial");
        write_text_atomic(&path, "(game: (edge_scroll: false, camera_speed: 9.0), controls: { Rotate: [Key(KeyT)], Cancel: [] })")
            .expect("write");
        let s = Settings::load(&path).expect("partial file loads");
        assert!(!s.game.edge_scroll);
        assert_eq!(s.game.camera_speed, 2.0, "clamped");
        assert_eq!(s.game.autosave, GameplaySettings::default().autosave);
        assert_eq!(s.action_map().bindings(Action::Rotate), &[Binding::Key(KeyCode::KeyT)]);
        // Fixed actions keep their default, missing ones get it.
        assert_eq!(s.action_map().bindings(Action::Cancel), &[Binding::Key(KeyCode::Escape)]);
        assert_eq!(s.action_map().bindings(Action::QuickSave), &[Binding::Key(KeyCode::F5)]);

        write_text_atomic(&path, "(game: (autosave: Sometimes))").expect("write");
        assert!(Settings::load(&path).is_err());
        assert_eq!(Settings::load_or_default(&path), Settings::default());
        assert_eq!(Settings::load_or_default(&path.with_file_name("missing.ron")), Settings::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap_or(&path));
    }

    #[test]
    fn autosave_and_filter_choices() {
        assert_eq!(AutosaveInterval::GameDays(2).ticks(), Some(2 * TICKS_PER_DAY));
        assert_eq!(AutosaveInterval::RealMinutes(5).seconds(), Some(300.0));
        assert_eq!(AutosaveInterval::Off.ticks(), None);
        assert_eq!(AutosaveInterval::GameDays(1).label(), "Jeden Spieltag");
        assert_eq!(AutosaveInterval::GameDays(7).label(), "Alle 7 Spieltage");
        assert_eq!(AutosaveInterval::RealMinutes(10).label(), "Alle 10 Minuten");
        assert!(NotificationFilter::All.shows(Severity::Info));
        assert!(!NotificationFilter::Important.shows(Severity::Info) && NotificationFilter::Important.shows(Severity::Good));
        assert!(!NotificationFilter::Warnings.shows(Severity::Good) && NotificationFilter::Warnings.shows(Severity::Danger));
        assert_eq!(ParticleDensity::Off.factor(), 0.0);
    }
}
