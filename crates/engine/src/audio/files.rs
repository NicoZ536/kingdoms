//! Finding the delivered audio files of the manifest and checking them.
//!
//! The manifest names one file per sound (e.g. `music/menu_theme.ogg`). Delivered files often
//! come in another format or with a slightly different spelling (`music/Menu Theme.mp3`), so a
//! file is found by its **stem** in the manifest's folder: the exact path first, then a file
//! whose stem matches after [`stem_key`] (lower case, every run of other characters than
//! letters and digits one `_`) with a supported extension – the manifest's extension
//! preferred, then [`SUPPORTED_EXTENSIONS`] in order. Folder listings are read once.
//!
//! [`audit`] decodes every expected file without an output device (duration, format, peak level
//! and silence at the ends) for the developer report of the game.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rodio::{Decoder, Source};

use super::{AudioChannel, AudioManifest};

/// Formats the decoder reads, in order of preference after the manifest's own extension.
pub const SUPPORTED_EXTENSIONS: [&str; 4] = ["ogg", "mp3", "wav", "flac"];

/// Samples quieter than this count as silence (−60 dBFS).
const SILENCE: f32 = 0.001;

/// Comparison key of a file stem: lower case, runs of characters other than letters and digits
/// become one `_`, none at the ends (`"Menu Theme (1)"` → `"menu_theme_1"`).
pub fn stem_key(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len());
    for c in stem.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// A file of an audio folder.
#[derive(Clone, Debug)]
struct FolderFile {
    name: String,
    stem: String,
    key: String,
    /// Lower-case extension.
    ext: String,
}

/// Resolves manifest paths to delivered files below the audio root (see the module docs).
#[derive(Debug)]
pub struct FileResolver {
    root: PathBuf,
    folders: BTreeMap<PathBuf, Vec<FolderFile>>,
}

impl FileResolver {
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf(), folders: BTreeMap::new() }
    }

    /// The file delivered for manifest path `file` (relative to the audio root), if any.
    pub fn resolve(&mut self, file: &str) -> Option<PathBuf> {
        let wanted = self.root.join(file);
        if wanted.is_file() {
            return Some(wanted);
        }
        let folder = wanted.parent()?.to_path_buf();
        let stem = wanted.file_stem()?.to_string_lossy().into_owned();
        let own_ext = wanted.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let key = stem_key(&stem);
        let files = self.folders.entry(folder.clone()).or_insert_with(|| list_folder(&folder));
        let rank = |f: &FolderFile| {
            let ext_rank = if f.ext == own_ext { 0 } else { 1 + SUPPORTED_EXTENSIONS.iter().position(|e| *e == f.ext).unwrap_or(SUPPORTED_EXTENSIONS.len()) };
            (ext_rank, f.stem != stem, f.name.clone())
        };
        files.iter().filter(|f| f.key == key && SUPPORTED_EXTENSIONS.contains(&f.ext.as_str())).min_by_key(|f| rank(f)).map(|f| folder.join(&f.name))
    }

    /// Path of `found` relative to the audio root, with `/` separators (for reports).
    pub fn relative(&self, found: &Path) -> String {
        found.strip_prefix(&self.root).unwrap_or(found).to_string_lossy().replace('\\', "/")
    }
}

fn list_folder(folder: &Path) -> Vec<FolderFile> {
    let Ok(read) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut files: Vec<FolderFile> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            let stem = p.file_stem()?.to_string_lossy().into_owned();
            let ext = p.extension()?.to_string_lossy().to_lowercase();
            Some(FolderFile { key: stem_key(&stem), name, stem, ext })
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// Opens `path` with the decoder (format hint from the extension).
pub fn open_decoder(path: &Path) -> Result<Decoder<BufReader<File>>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let mut builder = Decoder::builder().with_data(BufReader::new(file)).with_byte_len(len).with_seekable(true);
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        builder = builder.with_hint(ext);
    }
    builder.build().map_err(|e| e.to_string())
}

/// What decoding a delivered file found.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedInfo {
    pub duration: Duration,
    pub sample_rate: u32,
    pub channels: u16,
    /// Highest absolute sample value (1.0 = full scale).
    pub peak: f32,
    /// Silence at the start and at the end.
    pub leading_silence: Duration,
    pub trailing_silence: Duration,
}

impl DecodedInfo {
    /// Peak level in dBFS (−∞ for digital silence).
    pub fn peak_dbfs(&self) -> f32 {
        if self.peak > 0.0 { 20.0 * self.peak.log10() } else { f32::NEG_INFINITY }
    }
}

/// Decodes `path` completely and measures it.
pub fn decode_info(path: &Path) -> Result<DecodedInfo, String> {
    let decoder = open_decoder(path)?;
    let sample_rate = decoder.sample_rate().get();
    let channels = decoder.channels().get();
    let (mut frames, mut peak) = (0u64, 0.0f32);
    let (mut first_loud, mut last_loud) = (None::<u64>, None::<u64>);
    let mut frame_loud = false;
    for (i, sample) in decoder.enumerate() {
        let s = sample.abs();
        peak = peak.max(s);
        frame_loud |= s > SILENCE;
        if (i as u64 + 1) % u64::from(channels) == 0 {
            if frame_loud {
                first_loud.get_or_insert(frames);
                last_loud = Some(frames);
            }
            frames += 1;
            frame_loud = false;
        }
    }
    if frames == 0 {
        return Err("keine Samples".to_owned());
    }
    let seconds = |n: u64| Duration::from_secs_f64(n as f64 / f64::from(sample_rate));
    let leading = first_loud.unwrap_or(frames);
    let trailing = last_loud.map_or(frames, |l| frames - 1 - l);
    Ok(DecodedInfo {
        duration: seconds(frames),
        sample_rate,
        channels,
        peak,
        leading_silence: seconds(leading),
        trailing_silence: seconds(trailing),
    })
}

/// One expected file of the manifest in the delivery report.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditRow {
    /// Sound id or music track id.
    pub id: String,
    pub channel: AudioChannel,
    /// Variation number (1-based) of a sound with several files, else 0.
    pub variation: usize,
    /// The path the manifest names.
    pub expected: String,
    /// The delivered file (relative to the audio root), if found.
    pub found: Option<String>,
    /// Decoding result of the found file.
    pub decoded: Option<Result<DecodedInfo, String>>,
    /// The sound loops (music and ambience): silence at its ends is an audible gap.
    pub loops: bool,
}

impl AuditRow {
    /// True if the file is there and decodes.
    pub fn ok(&self) -> bool {
        matches!(self.decoded, Some(Ok(_)))
    }
}

/// Checks every file of `manifest` below `root`: found (by stem) and decodable, with its
/// measurements. Music first, then the sounds by id.
pub fn audit(root: &Path, manifest: &AudioManifest) -> Vec<AuditRow> {
    let mut resolver = FileResolver::new(root);
    let mut rows = Vec::new();
    let mut check = |id: &str, channel: AudioChannel, variation: usize, file: &str, loops: bool| {
        let found = resolver.resolve(file);
        let decoded = found.as_deref().map(decode_info);
        rows.push(AuditRow {
            id: id.to_owned(),
            channel,
            variation,
            expected: file.to_owned(),
            found: found.map(|p| resolver.relative(&p)),
            decoded,
            loops,
        });
    };
    for track in &manifest.music {
        check(&track.id, AudioChannel::Music, 0, &track.file, true);
    }
    for (id, sound) in &manifest.sounds {
        let several = sound.files.len() > 1;
        for (i, file) in sound.files.iter().enumerate() {
            check(id, sound.channel, if several { i + 1 } else { 0 }, file, sound.channel == AudioChannel::Ambience);
        }
    }
    rows
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A mono 16-bit PCM WAV file of `seconds` at `rate` Hz: a quiet start, then a 440 Hz tone.
    pub(crate) fn wav_bytes(rate: u32, seconds: f32, silent_start: f32) -> Vec<u8> {
        let frames = (rate as f32 * seconds) as u32;
        let silent = (rate as f32 * silent_start) as u32;
        let data_len = frames * 2;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..frames {
            let v = if i < silent { 0.0 } else { (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5 };
            out.extend_from_slice(&((v * f32::from(i16::MAX)) as i16).to_le_bytes());
        }
        out
    }

    pub(crate) fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kingdoms-audio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["music", "sfx", "voice"] {
            if let Err(e) = std::fs::create_dir_all(dir.join(sub)) {
                panic!("temp dir: {e}");
            }
        }
        dir
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Err(e) = std::fs::write(path, bytes) {
            panic!("write {}: {e}", path.display());
        }
    }

    #[test]
    fn stems_compare_without_case_and_punctuation() {
        assert_eq!(stem_key("menu_theme"), "menu_theme");
        assert_eq!(stem_key("Menu Theme"), "menu_theme");
        assert_eq!(stem_key("menu-theme (1)"), "menu_theme_1");
        assert_eq!(stem_key("  __Glocke__ "), "glocke");
        assert_eq!(stem_key("Bäckerei"), "bäckerei");
    }

    #[test]
    fn files_are_found_by_stem_with_any_supported_extension() {
        let root = temp_root("resolve");
        let wav = wav_bytes(8000, 0.1, 0.0);
        write(&root.join("music/Menu Theme.mp3"), b"not really mp3");
        write(&root.join("music/menu_theme.wav"), &wav);
        write(&root.join("sfx/click.ogg"), b"x");
        write(&root.join("sfx/bell.txt"), b"x");
        write(&root.join("voice/welcome.flac"), b"x");
        let mut r = FileResolver::new(&root);
        // The exact file wins.
        assert_eq!(r.resolve("sfx/click.ogg"), Some(root.join("sfx/click.ogg")));
        // Another extension: listed order (mp3 before wav), whatever the spelling of the stem.
        assert_eq!(r.resolve("music/menu_theme.ogg"), Some(root.join("music/Menu Theme.mp3")));
        // The manifest's own extension is preferred.
        assert_eq!(r.resolve("music/menu_theme.wav"), Some(root.join("music/menu_theme.wav")));
        assert_eq!(r.resolve("voice/welcome.ogg"), Some(root.join("voice/welcome.flac")));
        // Unsupported formats and missing files are not found.
        assert_eq!(r.resolve("sfx/bell.ogg"), None);
        assert_eq!(r.resolve("sfx/nothing.ogg"), None);
        assert_eq!(r.relative(&root.join("music/Menu Theme.mp3")), "music/Menu Theme.mp3");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_generated_wav_decodes_with_duration_peak_and_silence() {
        let root = temp_root("decode");
        let path = root.join("sfx/tone.wav");
        write(&path, &wav_bytes(8000, 0.5, 0.125));
        let info = match decode_info(&path) {
            Ok(info) => info,
            Err(e) => panic!("decode: {e}"),
        };
        assert_eq!((info.sample_rate, info.channels), (8000, 1));
        assert!((info.duration.as_secs_f32() - 0.5).abs() < 0.01, "{:?}", info.duration);
        assert!((info.peak - 0.5).abs() < 0.02, "{}", info.peak);
        assert!((info.peak_dbfs() + 6.0).abs() < 0.3);
        assert!((info.leading_silence.as_secs_f32() - 0.125).abs() < 0.01, "{:?}", info.leading_silence);
        assert!(info.trailing_silence < Duration::from_millis(5));
        // Garbage is reported, not a panic.
        write(&root.join("sfx/broken.wav"), b"RIFF garbage");
        assert!(decode_info(&root.join("sfx/broken.wav")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_audit_lists_every_expected_file() {
        let root = temp_root("audit");
        write(&root.join("sfx/Click.WAV"), &wav_bytes(8000, 0.2, 0.0));
        let text = r#"(
            sounds: {
                "ui_click": (files: ["sfx/click.ogg"], channel: Ui),
                "vo_yes": (files: ["voice/yes_1.ogg", "voice/yes_2.ogg"], channel: Voice, text: ["Jawohl!", "Sofort!"]),
            },
            music: [ (id: "menu_theme", file: "music/menu_theme.ogg", moods: [Menu]) ],
        )"#;
        let manifest: AudioManifest = match ron::from_str(text) {
            Ok(m) => m,
            Err(e) => panic!("manifest: {e}"),
        };
        let rows = audit(&root, &manifest);
        let ids: Vec<(&str, usize)> = rows.iter().map(|r| (r.id.as_str(), r.variation)).collect();
        assert_eq!(ids, [("menu_theme", 0), ("ui_click", 0), ("vo_yes", 1), ("vo_yes", 2)]);
        assert!(rows[0].loops && !rows[0].ok() && rows[0].found.is_none());
        assert_eq!(rows[1].found.as_deref(), Some("sfx/Click.WAV"));
        assert!(rows[1].ok());
        let _ = std::fs::remove_dir_all(&root);
    }
}
