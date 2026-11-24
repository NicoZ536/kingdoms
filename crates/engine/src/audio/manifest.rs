//! Audio manifest (`assets/audio/manifest.ron`): maps sound ids to files, channels and playback rules.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{AudioChannel, MusicMood};

/// One sound effect / ambience loop / voice line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundDef {
    /// Files relative to `assets/audio/`; several files are played as variations.
    pub files: Vec<String>,
    pub channel: AudioChannel,
    #[serde(default = "one")]
    pub volume: f32,
    /// Minimum time between two plays of this sound.
    #[serde(default)]
    pub cooldown_ms: u32,
    /// Maximum simultaneous instances (0 = unlimited).
    #[serde(default)]
    pub max_instances: u32,
    /// Random pitch variation (e.g. 0.05 = ±5 %).
    #[serde(default)]
    pub pitch_variation: f32,
    /// Free text describing the intended sound (used to brief sound designers).
    #[serde(default)]
    pub description: String,
    /// Voice lines: the exact words to speak, one per file of `files` (or one for all).
    #[serde(default)]
    pub text: Vec<String>,
}

/// A music track and the moods it may play in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MusicTrackDef {
    pub id: String,
    pub file: String,
    pub moods: Vec<MusicMood>,
    #[serde(default = "one")]
    pub volume: f32,
    #[serde(default)]
    pub description: String,
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioManifest {
    #[serde(default)]
    pub sounds: BTreeMap<String, SoundDef>,
    #[serde(default)]
    pub music: Vec<MusicTrackDef>,
}

impl AudioManifest {
    /// Loads the manifest; a missing or invalid file yields an empty manifest (logged).
    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match ron::from_str::<AudioManifest>(&text) {
                Ok(m) => {
                    log::info!("audio manifest: {} sounds, {} music tracks", m.sounds.len(), m.music.len());
                    m
                }
                Err(err) => {
                    log::error!("invalid audio manifest {}: {err}", path.display());
                    Self::default()
                }
            },
            Err(_) => {
                log::info!("no audio manifest at {}", path.display());
                Self::default()
            }
        }
    }

    /// Every file the manifest names (music and sounds), sorted, without duplicates.
    pub fn all_files(&self) -> Vec<String> {
        let mut files: Vec<String> = self.sounds.values().flat_map(|s| s.files.iter()).chain(self.music.iter().map(|m| &m.file)).cloned().collect();
        files.sort();
        files.dedup();
        files
    }

    /// Lists manifest entries without a delivered file below `audio_root`, found by stem with
    /// any supported extension (see [`super::files`]; for tooling/diagnostics).
    pub fn missing_files(&self, audio_root: &Path) -> Vec<String> {
        let mut resolver = super::files::FileResolver::new(audio_root);
        self.all_files().into_iter().filter(|f| resolver.resolve(f).is_none()).collect()
    }

    /// Problems of the voice lines: a voice sound without the text to speak, or with a number
    /// of lines that is neither one nor one per file.
    pub fn voice_text_problems(&self) -> Vec<String> {
        self.sounds
            .iter()
            .filter(|(_, s)| s.channel == AudioChannel::Voice)
            .filter_map(|(id, s)| {
                if s.text.iter().all(|t| t.trim().is_empty()) {
                    Some(format!("{id}: no text to speak"))
                } else if s.text.len() != 1 && s.text.len() != s.files.len() {
                    Some(format!("{id}: {} lines for {} files", s.text.len(), s.files.len()))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest() {
        let text = r#"(
            sounds: {
                "ui_click": (files: ["ui/click.ogg"], channel: Ui, volume: 0.6, cooldown_ms: 40),
                "amb_birds_day": (files: ["ambience/birds.ogg"], channel: Ambience),
            },
            music: [ (id: "menu_theme", file: "music/menu.ogg", moods: [Menu]) ],
        )"#;
        let m: AudioManifest = ron::from_str(text).unwrap();
        assert_eq!(m.sounds["ui_click"].channel, AudioChannel::Ui);
        assert_eq!(m.sounds["amb_birds_day"].volume, 1.0);
        assert_eq!(m.music[0].moods, vec![MusicMood::Menu]);
        let missing = m.missing_files(Path::new("/nonexistent"));
        assert_eq!(missing.len(), 3);
        assert!(m.sounds["ui_click"].text.is_empty());
    }

    #[test]
    fn voice_lines_need_their_text() {
        let text = r#"(
            sounds: {
                "vo_a": (files: ["voice/a.ogg"], channel: Voice, text: ["Jawohl, Majestät."]),
                "vo_b": (files: ["voice/b.ogg"], channel: Voice),
                "vo_c": (files: ["voice/c_1.ogg", "voice/c_2.ogg", "voice/c_3.ogg"], channel: Voice, text: ["Eins", "Zwei"]),
                "sfx_d": (files: ["sfx/d.ogg"], channel: Effects),
            },
        )"#;
        let m: AudioManifest = ron::from_str(text).unwrap();
        assert_eq!(m.voice_text_problems(), ["vo_b: no text to speak", "vo_c: 2 lines for 3 files"]);
    }
}
