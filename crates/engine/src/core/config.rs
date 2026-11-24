//! Engine configuration (window, graphics, simulation timing, debug), stored as RON.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub maximized: bool,
    pub resizable: bool,
    /// Image of the window icon (Linux, Windows), relative to the assets directory; set by the
    /// game at start-up, never stored. macOS takes the icon of the app bundle instead.
    #[serde(skip)]
    pub icon: Option<String>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "KINGDOMS – Rise of an Empire".to_owned(),
            width: 1600,
            height: 900,
            fullscreen: false,
            maximized: false,
            resizable: true,
            icon: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsConfig {
    /// Wait for vertical sync (no tearing, lowest power usage).
    pub vsync: bool,
    /// Frame limiter used when vsync is off. `0` disables the limiter.
    pub max_fps: u32,
    /// Multisample anti-aliasing sample count (1 or 4). Smooths sprite edges via alpha-to-coverage.
    pub msaa_samples: u32,
    /// UI scale multiplier on top of the OS scale factor.
    pub ui_scale: f32,
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        Self { vsync: true, max_fps: 144, msaa_samples: 4, ui_scale: 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulationConfig {
    /// Fixed simulation ticks per real second at normal speed.
    pub tick_rate: u32,
    /// Upper bound of ticks executed in a single rendered frame.
    pub max_ticks_per_frame: u32,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self { tick_rate: 20, max_ticks_per_frame: 12 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugConfig {
    pub show_debug_hud: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub window: WindowConfig,
    /// Volume settings per audio channel.
    pub audio: crate::audio::AudioSettings,
    pub graphics: GraphicsConfig,
    pub simulation: SimulationConfig,
    pub debug: DebugConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read config file {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("invalid config file {path}: {source}")]
    Parse { path: String, source: Box<ron::error::SpannedError> },
    #[error("could not serialize config: {0}")]
    Serialize(#[from] Box<ron::Error>),
}

impl EngineConfig {
    /// Loads the configuration from `path`. Missing fields fall back to defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)
            .map_err(|source| ConfigError::Io { path: path.display().to_string(), source })?;
        let mut cfg: EngineConfig = ron::from_str(&text)
            .map_err(|source| ConfigError::Parse { path: path.display().to_string(), source: Box::new(source) })?;
        cfg.sanitize();
        Ok(cfg)
    }

    /// Loads the configuration or returns defaults (logging the reason) if that fails.
    pub fn load_or_default(path: &Path) -> Self {
        if !path.exists() {
            log::info!("no engine config at {}, using defaults", path.display());
            return Self::default();
        }
        match Self::load(path) {
            Ok(cfg) => {
                log::info!("loaded engine config from {}", path.display());
                cfg
            }
            Err(err) => {
                log::warn!("{err}; using defaults");
                Self::default()
            }
        }
    }

    /// Writes the configuration as pretty RON (atomically via a temporary file).
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).map_err(Box::new)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| ConfigError::Io { path: parent.display().to_string(), source })?;
        }
        let tmp = path.with_extension("ron.tmp");
        // Flushed to disk before the rename (on macOS `sync_all` is a full flush), so a crash
        // leaves either the old or the new file, never an empty one.
        let written = (|| {
            use std::io::Write as _;
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })();
        if let Err(source) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(ConfigError::Io { path: tmp.display().to_string(), source });
        }
        std::fs::rename(&tmp, path)
            .map_err(|source| ConfigError::Io { path: path.display().to_string(), source })?;
        Ok(())
    }

    /// Clamps values into supported ranges.
    pub fn sanitize(&mut self) {
        self.window.width = self.window.width.clamp(640, 16384);
        self.window.height = self.window.height.clamp(360, 16384);
        self.graphics.msaa_samples = if self.graphics.msaa_samples >= 4 { 4 } else { 1 };
        self.graphics.ui_scale = self.graphics.ui_scale.clamp(0.5, 3.0);
        self.simulation.tick_rate = self.simulation.tick_rate.clamp(1, 240);
        self.simulation.max_ticks_per_frame = self.simulation.max_ticks_per_frame.clamp(1, 1000);
        let a = &mut self.audio;
        for v in [&mut a.master, &mut a.music, &mut a.ambience, &mut a.effects, &mut a.ui, &mut a.voice] {
            *v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ron() {
        let cfg = EngineConfig::default();
        let text = ron::ser::to_string_pretty(&cfg, Default::default()).unwrap();
        let back: EngineConfig = ron::from_str(&text).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn partial_config_uses_defaults() {
        let cfg: EngineConfig = ron::from_str("(graphics: (vsync: false))").unwrap();
        assert!(!cfg.graphics.vsync);
        assert_eq!(cfg.simulation.tick_rate, 20);
        assert_eq!(cfg.window.width, 1600);
    }

    #[test]
    fn sanitize_clamps() {
        let mut cfg = EngineConfig::default();
        cfg.graphics.msaa_samples = 3;
        cfg.simulation.tick_rate = 0;
        cfg.sanitize();
        assert_eq!(cfg.graphics.msaa_samples, 1);
        assert_eq!(cfg.simulation.tick_rate, 1);
    }
}
