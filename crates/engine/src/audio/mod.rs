//! Audio: separate volume channels (music, ambience, effects, UI, voice), a data-driven sound
//! manifest, cached decoding, music playlists with cross-fades, looping ambience layers with
//! smooth volume changes, stereo-panned, distance-attenuated effects and voice lines that duck
//! the music and ambience while they speak.
//!
//! Files are found by their stem with any supported extension ([`files`]): a delivered
//! `music/menu_theme.mp3` satisfies the manifest's `music/menu_theme.ogg`. The files are looked
//! up once at start; a summary is logged, the details by [`Audio::log_delivery`].
//!
//! Music: the tracks of the current mood play one after the other (a mood with one track loops
//! it); a track cross-fades into the next shortly before its end, and a mood change
//! cross-fades the current track into a track of the new mood.
//!
//! The system degrades gracefully: without an audio device (servers, CI) or with missing files the
//! game runs silently and logs the reason once.

pub mod files;
mod manifest;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rodio::source::{Buffered, ChannelVolume};
use rodio::{Decoder, MixerDeviceSink, Player, Source};
use serde::{Deserialize, Serialize};

pub use manifest::{AudioManifest, MusicTrackDef, SoundDef};

/// Independent volume channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AudioChannel {
    Music,
    Ambience,
    Effects,
    Ui,
    Voice,
}

impl AudioChannel {
    pub const ALL: [AudioChannel; 5] =
        [AudioChannel::Music, AudioChannel::Ambience, AudioChannel::Effects, AudioChannel::Ui, AudioChannel::Voice];

    pub fn display_name(self) -> &'static str {
        match self {
            AudioChannel::Music => "Musik",
            AudioChannel::Ambience => "Umgebung",
            AudioChannel::Effects => "Effekte",
            AudioChannel::Ui => "Oberfläche",
            AudioChannel::Voice => "Stimmen",
        }
    }

}

/// User volume settings (persisted with the game settings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub master: f32,
    pub music: f32,
    pub ambience: f32,
    pub effects: f32,
    pub ui: f32,
    pub voice: f32,
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { master: 0.8, music: 0.6, ambience: 0.7, effects: 0.8, ui: 0.7, voice: 0.9, muted: false }
    }
}

impl AudioSettings {
    pub fn channel(&self, channel: AudioChannel) -> f32 {
        match channel {
            AudioChannel::Music => self.music,
            AudioChannel::Ambience => self.ambience,
            AudioChannel::Effects => self.effects,
            AudioChannel::Ui => self.ui,
            AudioChannel::Voice => self.voice,
        }
    }

    pub fn channel_mut(&mut self, channel: AudioChannel) -> &mut f32 {
        match channel {
            AudioChannel::Music => &mut self.music,
            AudioChannel::Ambience => &mut self.ambience,
            AudioChannel::Effects => &mut self.effects,
            AudioChannel::Ui => &mut self.ui,
            AudioChannel::Voice => &mut self.voice,
        }
    }

    /// Effective gain of a channel (master × channel, 0 when muted).
    pub fn gain(&self, channel: AudioChannel) -> f32 {
        if self.muted { 0.0 } else { (self.master * self.channel(channel)).clamp(0.0, 1.0) }
    }
}

/// Where a positional effect plays relative to the listener.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Stereo position, -1 (left) .. 1 (right).
    pub pan: f32,
    /// Distance attenuation, 0 (inaudible) .. 1 (full volume).
    pub attenuation: f32,
}

impl Placement {
    pub const CENTER: Placement = Placement { pan: 0.0, attenuation: 1.0 };

    /// Computes pan/attenuation for a sound at `source` (screen pixels) heard by a listener whose
    /// screen is `viewport` pixels large. Sounds fade out over one screen width beyond the edge.
    pub fn from_screen(source: glam::Vec2, viewport: glam::Vec2, zoom: f32) -> Self {
        let center = viewport * 0.5;
        let rel = (source - center) / viewport.max(glam::Vec2::ONE);
        let pan = (rel.x * 1.6).clamp(-1.0, 1.0);
        let outside = (rel.abs() - glam::Vec2::splat(0.5)).max(glam::Vec2::ZERO).length();
        // Zoomed-out views hear the town more quietly.
        let zoom_factor = zoom.clamp(0.25, 1.5).sqrt().min(1.0);
        let attenuation = ((1.0 - outside * 1.5).clamp(0.0, 1.0)) * zoom_factor;
        Self { pan, attenuation }
    }

    /// Equal-power stereo gains `[left, right]`.
    pub fn stereo_gains(&self) -> [f32; 2] {
        let angle = (self.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        [angle.cos() * std::f32::consts::SQRT_2, angle.sin() * std::f32::consts::SQRT_2].map(|g| g.min(1.0))
    }
}

/// Mood that selects the music playlist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MusicMood {
    Menu,
    Peaceful,
    Prosperous,
    Tense,
    War,
    Winter,
    Night,
}

struct ActiveSound {
    player: Player,
    sound: String,
    channel: AudioChannel,
    base_volume: f32,
}

struct AmbientLayer {
    player: Player,
    current: f32,
    target: f32,
    base_volume: f32,
}

/// The music track playing.
struct Track {
    player: Player,
    id: String,
    volume: f32,
    /// Length, if the decoder knows it (cross-fade into the next track before the end).
    length: Option<Duration>,
}

struct MusicState {
    mood: MusicMood,
    current: Option<Track>,
    fading: Vec<(Player, f32)>,
    fade_in: f32,
    recent: Vec<String>,
    track_counter: u64,
}

const FADE_SECONDS: f32 = 2.5;
const AMBIENCE_FADE_PER_SECOND: f32 = 0.4;
const MAX_ACTIVE_SOUNDS: usize = 48;
/// Music volume while a voice line speaks (ambience ducks half as much).
const DUCK_LEVEL: f32 = 0.35;
/// Seconds to duck down when a voice starts, and to come back after it ended.
const DUCK_ATTACK_SECONDS: f32 = 0.25;
const DUCK_RELEASE_SECONDS: f32 = 1.2;

/// Music and ambience gain for the ducking level `duck` (1 = no voice).
fn ducked(duck: f32, channel: AudioChannel) -> f32 {
    match channel {
        AudioChannel::Music => duck,
        AudioChannel::Ambience => 1.0 - (1.0 - duck) * 0.5,
        _ => 1.0,
    }
}

/// Moves the ducking level one frame (`dt` seconds) towards its target.
fn duck_step(duck: f32, voice_speaking: bool, dt: f32) -> f32 {
    if voice_speaking {
        (duck - dt * (1.0 - DUCK_LEVEL) / DUCK_ATTACK_SECONDS).max(DUCK_LEVEL)
    } else {
        (duck + dt * (1.0 - DUCK_LEVEL) / DUCK_RELEASE_SECONDS).min(1.0)
    }
}

/// The audio system. Owned by the engine context; call [`Audio::update`] once per frame.
pub struct Audio {
    device: Option<MixerDeviceSink>,
    root: PathBuf,
    manifest: AudioManifest,
    settings: AudioSettings,
    cache: HashMap<PathBuf, Option<Buffered<Decoder<BufReader<File>>>>>,
    active: Vec<ActiveSound>,
    ambience: BTreeMap<String, AmbientLayer>,
    music: MusicState,
    last_played: HashMap<String, Instant>,
    reported: HashSet<String>,
    variation_counter: u64,
    /// Delivered file of every manifest file (`None`: not delivered).
    resolved: BTreeMap<String, Option<PathBuf>>,
    /// Ducking level of music and ambience (1 = no voice speaking).
    duck: f32,
}

impl Audio {
    /// Opens the default output device and loads `audio/manifest.ron` below `assets_root`.
    pub fn new(assets_root: &Path, settings: AudioSettings) -> Self {
        let root = assets_root.join("audio");
        let manifest = AudioManifest::load_or_default(&root.join("manifest.ron"));
        let resolved = resolve_all(&root, &manifest);
        let device = match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                log::info!("audio: output device opened");
                Some(sink)
            }
            Err(err) => {
                log::warn!("audio disabled: no output device ({err})");
                None
            }
        };
        Self {
            device,
            root,
            manifest,
            settings,
            cache: HashMap::new(),
            active: Vec::new(),
            ambience: BTreeMap::new(),
            music: MusicState {
                mood: MusicMood::Menu,
                current: None,
                fading: Vec::new(),
                fade_in: 0.0,
                recent: Vec::new(),
                track_counter: 0,
            },
            last_played: HashMap::new(),
            reported: HashSet::new(),
            variation_counter: 0,
            resolved,
            duck: 1.0,
        }
    }

    /// Logs which manifest files were found under another name or format and which ids have
    /// no file yet (the start only logs a summary).
    pub fn log_delivery(&self) {
        let files = files::FileResolver::new(&self.root);
        for (file, path) in &self.resolved {
            if let Some(path) = path
                && files.relative(path) != *file
            {
                log::info!("audio: {file} ← {}", files.relative(path));
            }
        }
        let missing = |list: &[String]| list.iter().any(|f| self.resolved.get(f).is_none_or(Option::is_none));
        let mut ids: Vec<&str> = self.manifest.music.iter().filter(|t| missing(std::slice::from_ref(&t.file))).map(|t| t.id.as_str()).collect();
        ids.extend(self.manifest.sounds.iter().filter(|(_, s)| missing(&s.files)).map(|(id, _)| id.as_str()));
        if ids.is_empty() {
            log::info!("audio: every sound id has its files");
        } else {
            log::info!("audio: {} ids without (all) files: {}", ids.len(), ids.join(", "));
        }
    }

    /// Delivered file for manifest path `file` (found at start).
    fn delivered(&self, file: &str) -> Option<&Path> {
        self.resolved.get(file).and_then(|p| p.as_deref())
    }

    /// True when sound can actually be heard (device present).
    pub fn is_available(&self) -> bool {
        self.device.is_some()
    }

    pub fn settings(&self) -> &AudioSettings {
        &self.settings
    }

    pub fn manifest(&self) -> &AudioManifest {
        &self.manifest
    }

    /// Applies new volume settings immediately to everything that is playing.
    pub fn set_settings(&mut self, settings: AudioSettings) {
        self.settings = settings;
        for s in &self.active {
            s.player.set_volume(s.base_volume * self.settings.gain(s.channel));
        }
        let amb_gain = self.settings.gain(AudioChannel::Ambience);
        for layer in self.ambience.values() {
            layer.player.set_volume(layer.current * layer.base_volume * amb_gain);
        }
        self.apply_music_volume();
    }

    /// Plays a sound effect by manifest id at full volume, centred.
    pub fn play(&mut self, id: &str) {
        self.play_placed(id, Placement::CENTER);
    }

    /// Plays a sound effect with stereo pan and distance attenuation.
    pub fn play_placed(&mut self, id: &str, placement: Placement) {
        let Some(def) = self.manifest.sounds.get(id).cloned() else {
            self.report_once(format!("sound:{id}"), || format!("audio: unknown sound id '{id}'"));
            return;
        };
        if self.device.is_none() || placement.attenuation <= 0.01 || def.files.is_empty() {
            return;
        }
        let now = Instant::now();
        if def.cooldown_ms > 0
            && let Some(last) = self.last_played.get(id)
            && now.duration_since(*last) < Duration::from_millis(u64::from(def.cooldown_ms))
        {
            return;
        }
        let playing = self.active.iter().filter(|s| s.sound == id).count() as u32;
        if def.max_instances > 0 && playing >= def.max_instances {
            return;
        }
        if self.active.len() >= MAX_ACTIVE_SOUNDS {
            return;
        }
        // Rotate deterministically through file variations.
        self.variation_counter = self.variation_counter.wrapping_add(1);
        let file = &def.files[(self.variation_counter as usize) % def.files.len()];
        let Some(path) = self.delivered(file).map(Path::to_path_buf) else { return };
        let Some(source) = self.cached_source(&path) else { return };
        let Some(device) = self.device.as_ref() else { return };
        let player = Player::connect_new(device.mixer());
        let pitch = if def.pitch_variation > 0.0 {
            let t = ((self.variation_counter.wrapping_mul(2_654_435_761) % 1000) as f32 / 1000.0) * 2.0 - 1.0;
            1.0 + t * def.pitch_variation
        } else {
            1.0
        };
        let [l, r] = placement.stereo_gains();
        let base_volume = def.volume * placement.attenuation;
        if source.channels().get() >= 2 || placement.pan.abs() < 0.05 {
            player.append(source.speed(pitch));
        } else {
            player.append(ChannelVolume::new(source.speed(pitch), vec![l, r]));
        }
        player.set_volume(base_volume * self.settings.gain(def.channel));
        self.active.push(ActiveSound { player, sound: id.to_owned(), channel: def.channel, base_volume });
        self.last_played.insert(id.to_owned(), now);
    }

    /// Sets the target volume (0..1) of a looping ambience layer; layers fade smoothly and are
    /// started/stopped automatically.
    pub fn set_ambience(&mut self, id: &str, target: f32) {
        let target = target.clamp(0.0, 1.0);
        if let Some(layer) = self.ambience.get_mut(id) {
            layer.target = target;
            return;
        }
        if target <= 0.001 || self.device.is_none() {
            return;
        }
        let Some(def) = self.manifest.sounds.get(id).cloned() else {
            self.report_once(format!("sound:{id}"), || format!("audio: unknown ambience id '{id}'"));
            return;
        };
        let Some(path) = def.files.first().and_then(|f| self.delivered(f)).map(Path::to_path_buf) else { return };
        let Some(source) = self.cached_source(&path) else { return };
        let Some(device) = self.device.as_ref() else { return };
        let player = Player::connect_new(device.mixer());
        player.append(source.repeat_infinite());
        player.set_volume(0.0);
        self.ambience.insert(id.to_owned(), AmbientLayer { player, current: 0.0, target, base_volume: def.volume });
    }

    /// Selects the music mood; the current track finishes with a cross-fade into the new mood.
    pub fn set_music_mood(&mut self, mood: MusicMood) {
        if self.music.mood != mood {
            log::debug!("music mood: {:?} -> {mood:?}", self.music.mood);
            self.music.mood = mood;
            if let Some(track) = self.music.current.take() {
                self.music.fading.push((track.player, track.volume * self.music.fade_in));
            }
        }
    }

    pub fn music_mood(&self) -> MusicMood {
        self.music.mood
    }

    /// Name of the playing music track, if any.
    pub fn current_track(&self) -> Option<&str> {
        self.music.current.as_ref().map(|t| t.id.as_str())
    }

    /// Per-frame housekeeping: fades, cleanup of finished sounds, next music track.
    pub fn update(&mut self, dt: f32) {
        if self.device.is_none() {
            return;
        }
        self.active.retain(|s| !s.player.empty());
        let speaking = self.active.iter().any(|s| s.channel == AudioChannel::Voice);
        self.duck = duck_step(self.duck, speaking, dt);

        let amb_gain = self.settings.gain(AudioChannel::Ambience) * ducked(self.duck, AudioChannel::Ambience);
        let step = AMBIENCE_FADE_PER_SECOND * dt;
        self.ambience.retain(|_, layer| {
            if (layer.current - layer.target).abs() <= step {
                layer.current = layer.target;
            } else if layer.current < layer.target {
                layer.current += step;
            } else {
                layer.current -= step;
            }
            layer.player.set_volume(layer.current * layer.base_volume * amb_gain);
            !(layer.current <= 0.0 && layer.target <= 0.0)
        });

        // Music: fade out old tracks while the current one fades in (cross-fade); a track
        // close to its end fades out into the next one.
        let music_gain = self.settings.gain(AudioChannel::Music) * ducked(self.duck, AudioChannel::Music);
        let fade = dt / FADE_SECONDS;
        self.music.fading.retain_mut(|(player, vol)| {
            *vol -= fade;
            player.set_volume(vol.max(0.0) * music_gain);
            let keep = *vol > 0.0 && !player.empty();
            if !keep {
                player.stop();
            }
            keep
        });
        let ending = self.music.current.as_ref().is_some_and(|t| {
            t.player.empty() || t.length.is_some_and(|len| t.player.get_pos() + Duration::from_secs_f32(FADE_SECONDS) >= len)
        });
        if ending && let Some(track) = self.music.current.take() {
            self.music.fading.push((track.player, track.volume * self.music.fade_in));
        }
        if self.music.current.is_none() {
            self.start_next_track();
        }
        if let Some(track) = self.music.current.as_mut() {
            if self.music.fade_in < 1.0 {
                self.music.fade_in = (self.music.fade_in + fade).min(1.0);
            }
            track.player.set_volume(track.volume * self.music.fade_in * music_gain);
        }
    }

    fn apply_music_volume(&mut self) {
        let gain = self.settings.gain(AudioChannel::Music) * ducked(self.duck, AudioChannel::Music);
        if let Some(track) = self.music.current.as_ref() {
            track.player.set_volume(track.volume * self.music.fade_in * gain);
        }
        for (player, vol) in &self.music.fading {
            player.set_volume(vol.max(0.0) * gain);
        }
    }

    fn start_next_track(&mut self) {
        let mood = self.music.mood;
        // Only delivered tracks: a mood without files stays silent without retrying.
        let candidates: Vec<&MusicTrackDef> =
            self.manifest.music.iter().filter(|t| t.moods.contains(&mood) && self.delivered(&t.file).is_some()).collect();
        if candidates.is_empty() {
            return;
        }
        // Avoid repeating the most recent tracks when possible.
        let fresh: Vec<&MusicTrackDef> =
            candidates.iter().copied().filter(|t| !self.music.recent.contains(&t.id)).collect();
        let pool = if fresh.is_empty() { candidates } else { fresh };
        self.music.track_counter = self.music.track_counter.wrapping_add(1);
        let track = pool[(self.music.track_counter as usize * 7) % pool.len()].clone();
        let Some(path) = self.delivered(&track.file).map(Path::to_path_buf) else { return };
        let source = match files::open_decoder(&path) {
            Ok(decoder) => decoder,
            Err(err) => {
                self.report_once(path.display().to_string(), || format!("audio: cannot decode {}: {err}", path.display()));
                // Not again: the file stays undecodable.
                self.resolved.insert(track.file.clone(), None);
                return;
            }
        };
        let Some(device) = self.device.as_ref() else { return };
        let length = source.total_duration().filter(|d| d.as_secs_f32() > 3.0 * FADE_SECONDS);
        let player = Player::connect_new(device.mixer());
        player.append(source);
        player.set_volume(0.0);
        log::info!("music: playing '{}' ({mood:?})", track.id);
        self.music.recent.push(track.id.clone());
        if self.music.recent.len() > 3 {
            self.music.recent.remove(0);
        }
        self.music.fade_in = 0.0;
        self.music.current = Some(Track { player, id: track.id, volume: track.volume, length });
    }

    fn cached_source(&mut self, path: &Path) -> Option<Buffered<Decoder<BufReader<File>>>> {
        if let Some(entry) = self.cache.get(path) {
            return entry.clone();
        }
        let loaded = match files::open_decoder(path) {
            Ok(decoder) => Some(decoder.buffered()),
            Err(err) => {
                log::warn!("audio: cannot decode {}: {err}", path.display());
                None
            }
        };
        self.cache.insert(path.to_path_buf(), loaded.clone());
        loaded
    }

    fn report_once(&mut self, key: String, msg: impl FnOnce() -> String) {
        if self.reported.insert(key) {
            log::info!("{}", msg());
        }
    }

    /// Stops all sound (e.g. when quitting to the menu).
    pub fn stop_all(&mut self) {
        for s in self.active.drain(..) {
            s.player.stop();
        }
        for (_, layer) in std::mem::take(&mut self.ambience) {
            layer.player.stop();
        }
        if let Some(track) = self.music.current.take() {
            track.player.stop();
        }
        for (p, _) in self.music.fading.drain(..) {
            p.stop();
        }
    }

    /// Number of currently playing effect instances (diagnostics).
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

/// Looks up the delivered file of every manifest file and logs a summary.
fn resolve_all(root: &Path, manifest: &AudioManifest) -> BTreeMap<String, Option<PathBuf>> {
    let mut resolver = files::FileResolver::new(root);
    let resolved: BTreeMap<String, Option<PathBuf>> = manifest.all_files().into_iter().map(|f| {
        let found = resolver.resolve(&f);
        (f, found)
    }).collect();
    let found = resolved.values().filter(|p| p.is_some()).count();
    let renamed = resolved.iter().filter(|(f, p)| p.as_ref().is_some_and(|p| resolver.relative(p) != **f)).count();
    log::info!("audio: {found} of {} files present ({renamed} under another name or format)", resolved.len());
    for (file, path) in resolved.iter().filter_map(|(f, p)| p.as_ref().map(|p| (f, p))) {
        if resolver.relative(path) != *file {
            log::debug!("audio: {file} ← {}", resolver.relative(path));
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voices_duck_music_and_ambience() {
        // Down within the attack time, back within the release time.
        let mut duck = 1.0;
        for _ in 0..30 {
            duck = duck_step(duck, true, 1.0 / 60.0);
        }
        assert!((duck - DUCK_LEVEL).abs() < 1e-4, "{duck}");
        assert!((ducked(duck, AudioChannel::Music) - DUCK_LEVEL).abs() < 1e-4);
        assert!(ducked(duck, AudioChannel::Ambience) > DUCK_LEVEL && ducked(duck, AudioChannel::Ambience) < 1.0);
        assert_eq!(ducked(duck, AudioChannel::Effects), 1.0);
        let after_attack = duck_step(1.0, false, 0.5);
        assert_eq!(after_attack, 1.0);
        let mut back = duck;
        for _ in 0..(DUCK_RELEASE_SECONDS * 60.0).ceil() as usize {
            back = duck_step(back, false, 1.0 / 60.0);
        }
        assert!((back - 1.0).abs() < 1e-4, "{back}");
    }

    #[test]
    fn delivered_files_are_used_under_their_own_extension() {
        let root = files::tests::temp_root("engine-resolve");
        let audio_dir = root.join("audio");
        for sub in ["sfx", "music"] {
            std::fs::create_dir_all(audio_dir.join(sub)).unwrap();
        }
        std::fs::write(audio_dir.join("sfx/click.wav"), files::tests::wav_bytes(8000, 0.1, 0.0)).unwrap();
        std::fs::write(
            audio_dir.join("manifest.ron"),
            r#"(sounds: { "ui_click": (files: ["sfx/click.ogg"], channel: Ui), "ui_open": (files: ["sfx/open.ogg"], channel: Ui) },
                music: [ (id: "menu_theme", file: "music/menu_theme.ogg", moods: [Menu]) ])"#,
        )
        .unwrap();
        let audio = Audio::new(&root, AudioSettings::default());
        assert_eq!(audio.delivered("sfx/click.ogg"), Some(audio_dir.join("sfx/click.wav").as_path()));
        assert_eq!(audio.delivered("sfx/open.ogg"), None);
        assert_eq!(audio.delivered("music/menu_theme.ogg"), None);
        audio.log_delivery();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gains_respect_master_and_mute() {
        let mut s = AudioSettings { master: 0.5, effects: 0.8, ..Default::default() };
        assert!((s.gain(AudioChannel::Effects) - 0.4).abs() < 1e-6);
        s.muted = true;
        assert_eq!(s.gain(AudioChannel::Effects), 0.0);
    }

    #[test]
    fn placement_pan_and_attenuation() {
        let vp = glam::Vec2::new(1000.0, 600.0);
        let center = Placement::from_screen(vp * 0.5, vp, 1.0);
        assert!(center.pan.abs() < 1e-6 && (center.attenuation - 1.0).abs() < 1e-6);
        let right = Placement::from_screen(glam::Vec2::new(950.0, 300.0), vp, 1.0);
        assert!(right.pan > 0.5);
        let far = Placement::from_screen(glam::Vec2::new(5000.0, 300.0), vp, 1.0);
        assert_eq!(far.attenuation, 0.0);
        let [l, r] = Placement::CENTER.stereo_gains();
        assert!((l - r).abs() < 1e-6 && (l - 1.0).abs() < 1e-3);
    }

    #[test]
    fn works_without_device_or_files() {
        // In CI there is usually no audio device; the system must stay silent but functional.
        let dir = std::env::temp_dir().join("kingdoms_audio_test_missing");
        let mut audio = Audio::new(&dir, AudioSettings::default());
        audio.play("ui_click");
        audio.set_ambience("amb_birds_day", 1.0);
        audio.set_music_mood(MusicMood::Peaceful);
        audio.update(0.016);
        assert_eq!(audio.active_count(), 0);
    }
}
