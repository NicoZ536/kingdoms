//! Save slots in the saves directory: named saves, the quicksave and the rotating autosaves,
//! their file names, metadata, listing, deletion and the safe writing of a save file.
//!
//! Every save is `<stem>.ksav` (format of `kingdoms_sim::save`). Next to it a small RON file
//! `<stem>.kmeta` keeps what the simulation's header does not carry (population, treasury,
//! buildings, playing time and the last notifications of the „Meldungen“ list); it is only
//! trusted if its tick and save time match the header.
//! Writing goes to `<stem>.ksav.tmp` (flushed to disk), keeps the previous file as
//! `<stem>.ksav.bak`, renames the new file into place, checks it and only then drops the backup.
//! The listing repairs what an interrupted write leaves behind.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use kingdoms_sim::save::{
    MIN_SUPPORTED_FORMAT_VERSION, SAVE_EXTENSION, SAVE_FORMAT_VERSION, SaveError, SaveHeader, read_header_from_file,
};
use kingdoms_sim::event::Severity;
use kingdoms_sim::state::GameState;
use serde::{Deserialize, Serialize};

use super::{QUICKSAVE_NAME, describe_error};

/// File stem prefix of the autosaves (`autosave_1` …).
pub const AUTOSAVE_PREFIX: &str = "autosave_";
/// Number of rotating autosave slots.
pub const AUTOSAVE_SLOTS: u8 = 3;
/// Extension of the metadata file next to a save.
pub const META_EXTENSION: &str = "kmeta";
/// Longest save name in characters.
pub const MAX_NAME_CHARS: usize = 40;
/// Longest file stem derived from a name.
const MAX_STEM_CHARS: usize = 48;

/// What kind of save a file is (from its file name).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    /// Saved by the player under a name.
    Manual,
    /// The quicksave (F5).
    Quick,
    /// Autosave slot 1–3.
    Auto(u8),
}

impl SlotKind {
    pub fn of_stem(stem: &str) -> Self {
        if stem == QUICKSAVE_NAME {
            return SlotKind::Quick;
        }
        match stem.strip_prefix(AUTOSAVE_PREFIX).and_then(|n| n.parse::<u8>().ok()) {
            Some(n) if (1..=AUTOSAVE_SLOTS).contains(&n) => SlotKind::Auto(n),
            _ => SlotKind::Manual,
        }
    }
}

/// Why a save name cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    /// The name would overwrite the quicksave or an autosave.
    Reserved,
}

impl NameError {
    /// German explanation.
    pub const fn message(self) -> &'static str {
        match self {
            NameError::Empty => "Bitte einen Namen für den Spielstand eingeben.",
            NameError::TooLong => "Der Name ist zu lang (höchstens 40 Zeichen).",
            NameError::Reserved => "Dieser Name ist für den Schnellspielstand und die automatischen Spielstände reserviert.",
        }
    }
}

/// File stem for a save name: lower case ASCII, umlauts transliterated (`ä` → `ae`, `ß` →
/// `ss`), every other character a `_`, no leading, trailing or repeated `_`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        let piece = match c {
            'ä' => "ae",
            'ö' => "oe",
            'ü' => "ue",
            'ß' => "ss",
            'à' | 'á' | 'â' => "a",
            'è' | 'é' | 'ê' => "e",
            'ì' | 'í' | 'î' => "i",
            'ò' | 'ó' | 'ô' => "o",
            'ù' | 'ú' | 'û' => "u",
            c if c.is_ascii_alphanumeric() => {
                out.push(c);
                continue;
            }
            _ => "_",
        };
        if piece == "_" && (out.is_empty() || out.ends_with('_')) {
            continue;
        }
        out.push_str(piece);
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.chars().count() > MAX_STEM_CHARS {
        out = out.chars().take(MAX_STEM_CHARS).collect();
        while out.ends_with('_') {
            out.pop();
        }
    }
    out
}

/// Checks a save name and returns its file stem.
pub fn check_name(name: &str) -> Result<String, NameError> {
    let name = name.trim();
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(NameError::TooLong);
    }
    let stem = slug(name);
    if stem.is_empty() {
        return Err(NameError::Empty);
    }
    if SlotKind::of_stem(&stem) != SlotKind::Manual || stem.starts_with(AUTOSAVE_PREFIX) {
        return Err(NameError::Reserved);
    }
    Ok(stem)
}

/// Path of the save with file stem `stem`.
pub fn save_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.{SAVE_EXTENSION}"))
}

/// Path of the metadata file belonging to a save.
pub fn meta_path(save: &Path) -> PathBuf {
    save.with_extension(META_EXTENSION)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(std::ffi::OsString::from).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// The previous version kept while a save is replaced.
pub fn backup_path(save: &Path) -> PathBuf {
    with_suffix(save, ".bak")
}

fn temp_path(save: &Path) -> PathBuf {
    with_suffix(save, ".tmp")
}

/// Most notifications kept with a save.
pub const MAX_SAVED_NOTIFICATIONS: usize = 200;

/// A notification of the „Meldungen“ list as kept with a save: the list is the player's
/// memory of what happened, so it survives saving and loading.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedNotification {
    /// Tick of its last arrival.
    pub tick: u64,
    pub severity: Severity,
    pub text: String,
    /// Map tile it refers to.
    pub position: Option<(i32, i32)>,
    /// How often it came again (shown as „×2“ …).
    pub repeats: u32,
}

/// What the save list shows beyond the simulation's header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveMeta {
    /// Tick and save time of the save this belongs to (must match its header).
    pub tick: u64,
    pub saved_at_unix: u64,
    pub population: u32,
    /// Treasury in gold.
    pub treasury: i64,
    pub buildings: u32,
    /// Real seconds played in this game until the save.
    pub play_seconds: u64,
    /// The newest notifications, newest first (at most [`MAX_SAVED_NOTIFICATIONS`]; missing in
    /// metadata of older versions).
    #[serde(default)]
    pub notifications: Vec<SavedNotification>,
}

impl SaveMeta {
    pub fn of(state: &GameState, saved_at_unix: u64, play_seconds: u64) -> Self {
        Self {
            tick: state.clock().tick,
            saved_at_unix,
            population: u32::try_from(state.population().len()).unwrap_or(u32::MAX),
            treasury: state.economy().treasury(),
            buildings: u32::try_from(state.buildings().len()).unwrap_or(u32::MAX),
            play_seconds,
            notifications: Vec::new(),
        }
    }

    /// True if this metadata describes the save with `header`.
    pub fn matches(&self, header: &SaveHeader) -> bool {
        self.tick == header.summary.tick && self.saved_at_unix == header.saved_at_unix
    }
}

/// Reads the metadata of a save (`None` if missing or unreadable).
pub fn read_meta(save: &Path) -> Option<SaveMeta> {
    let text = fs::read_to_string(meta_path(save)).ok()?;
    match ron::from_str(&text) {
        Ok(meta) => Some(meta),
        Err(err) => {
            log::warn!("ignoring unreadable save metadata {}: {err}", meta_path(save).display());
            None
        }
    }
}

/// Writes the metadata of a save atomically.
pub fn write_meta(save: &Path, meta: &SaveMeta) -> std::io::Result<()> {
    let text = ron::ser::to_string_pretty(meta, ron::ser::PrettyConfig::default()).map_err(std::io::Error::other)?;
    crate::settings::write_text_atomic(&meta_path(save), &text).map_err(|(_, e)| e)
}

/// Whether a save can be loaded with the current game data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compatibility {
    Compatible,
    /// German reason.
    Incompatible(String),
}

/// Compatibility of a save from its header: format version and data fingerprint. The full
/// check (checksum, consistency) happens when loading.
pub fn compatibility(header: &SaveHeader, fingerprint: u64) -> Compatibility {
    if !(MIN_SUPPORTED_FORMAT_VERSION..=SAVE_FORMAT_VERSION).contains(&header.format_version) {
        return Compatibility::Incompatible(format!(
            "Gespeichert im Format {} (Version {}); diese Version liest nur Format {}.",
            header.format_version, header.game_version, SAVE_FORMAT_VERSION
        ));
    }
    if header.defs_fingerprint != fingerprint {
        return Compatibility::Incompatible(format!(
            "Mit anderen Spieldaten erstellt (Version {}); die Gebäude und Waren passen nicht zu diesem Spiel.",
            header.game_version
        ));
    }
    Compatibility::Compatible
}

/// One save file in the list.
#[derive(Clone, Debug)]
pub struct SaveEntry {
    pub path: PathBuf,
    pub stem: String,
    pub kind: SlotKind,
    /// The header, or the German reason it cannot be read.
    pub header: Result<SaveHeader, String>,
    pub meta: Option<SaveMeta>,
    pub compatibility: Compatibility,
    /// File modification time (Unix seconds; fallback for the save time).
    pub modified_unix: u64,
    pub size: u64,
}

impl SaveEntry {
    /// The name shown in lists.
    pub fn display_name(&self) -> String {
        match (&self.header, self.kind) {
            (_, SlotKind::Auto(n)) => format!("Autospeicherstand {n}"),
            (_, SlotKind::Quick) => "Schnellspielstand".to_owned(),
            (Ok(h), SlotKind::Manual) if !h.name.trim().is_empty() => h.name.clone(),
            _ => self.stem.clone(),
        }
    }

    /// When the save was written (Unix seconds).
    pub fn saved_at(&self) -> u64 {
        match &self.header {
            Ok(h) if h.saved_at_unix > 0 => h.saved_at_unix,
            _ => self.modified_unix,
        }
    }

    pub fn loadable(&self) -> bool {
        self.header.is_ok() && self.compatibility == Compatibility::Compatible
    }
}

/// Reads one save's header and metadata.
pub fn read_entry(path: &Path, fingerprint: u64) -> SaveEntry {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let fs_meta = fs::metadata(path).ok();
    let modified_unix = fs_meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let header = read_header_from_file(path).map_err(|err| {
        log::warn!("cannot read save header {}: {err}", path.display());
        describe_error(&err)
    });
    let compatibility = match &header {
        Ok(h) => compatibility(h, fingerprint),
        Err(reason) => Compatibility::Incompatible(reason.clone()),
    };
    let meta = header.as_ref().ok().and_then(|h| read_meta(path).filter(|m| m.matches(h)));
    SaveEntry {
        path: path.to_path_buf(),
        kind: SlotKind::of_stem(&stem),
        stem,
        header,
        meta,
        compatibility,
        modified_unix,
        size: fs_meta.map_or(0, |m| m.len()),
    }
}

/// Every save in `dir`, newest first. Repairs the leftovers of interrupted writes first.
pub fn list_saves(dir: &Path, fingerprint: u64) -> Vec<SaveEntry> {
    recover_backups(dir);
    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    let mut entries: Vec<SaveEntry> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == SAVE_EXTENSION))
        .map(|p| read_entry(&p, fingerprint))
        .collect();
    entries.sort_by(|a, b| b.saved_at().cmp(&a.saved_at()).then_with(|| a.stem.cmp(&b.stem)));
    entries
}

/// Repairs what an interrupted write can leave behind: a backup without its save is put back;
/// a backup next to a readable save is removed (the replacement completed).
pub fn recover_backups(dir: &Path) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let suffix = format!(".{SAVE_EXTENSION}.bak");
    for path in read.filter_map(Result::ok).map(|e| e.path()) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let Some(stem) = name.strip_suffix(&suffix) else { continue };
        let save = save_path(dir, stem);
        let restore = !save.exists() || read_header_from_file(&save).is_err();
        let result = if restore { fs::rename(&path, &save) } else { fs::remove_file(&path) };
        match result {
            Ok(()) if restore => log::warn!("restored {} from its backup after an interrupted save", save.display()),
            Ok(()) => log::info!("removed the leftover backup {}", path.display()),
            Err(err) => log::warn!("cannot repair {}: {err}", path.display()),
        }
    }
}

/// Removes temporary files of saves that were never finished (call when no save is running).
pub fn remove_stale_temp_files(dir: &Path) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let suffix = format!(".{SAVE_EXTENSION}.tmp");
    for path in read.filter_map(Result::ok).map(|e| e.path()) {
        if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&suffix)) {
            match fs::remove_file(&path) {
                Ok(()) => log::info!("removed unfinished save {}", path.display()),
                Err(err) => log::warn!("cannot remove {}: {err}", path.display()),
            }
        }
    }
}

/// True if two saves belong to the same game (the simulation's game id in their headers);
/// files whose header cannot be read count as one group.
pub fn same_game(a: &SaveEntry, b: &SaveEntry) -> bool {
    match (&a.header, &b.header) {
        (Ok(x), Ok(y)) => x.game_id == y.game_id,
        (Err(_), Err(_)) => true,
        _ => false,
    }
}

/// Orders saves listed newest first ([`list_saves`]) by game: the game with the newest save
/// first, each game's saves newest first, files without a readable header last. The order
/// within a game and of the games keeps the time order of `list_saves`.
pub fn group_by_game(entries: &mut Vec<SaveEntry>) {
    let mut firsts: Vec<usize> = Vec::new();
    let ranks: Vec<usize> = (0..entries.len())
        .map(|i| {
            if entries[i].header.is_err() {
                return usize::MAX;
            }
            match firsts.iter().position(|f| same_game(&entries[*f], &entries[i])) {
                Some(group) => group,
                None => {
                    firsts.push(i);
                    firsts.len() - 1
                }
            }
        })
        .collect();
    let mut tagged: Vec<(usize, SaveEntry)> = ranks.into_iter().zip(entries.drain(..)).collect();
    tagged.sort_by_key(|(rank, _)| *rank);
    entries.extend(tagged.into_iter().map(|(_, e)| e));
}

/// The newest save that can be loaded ("Fortsetzen").
pub fn newest_loadable(entries: &[SaveEntry]) -> Option<&SaveEntry> {
    entries.iter().filter(|e| e.loadable()).max_by_key(|e| e.saved_at())
}

/// Deletes a save with its metadata and backup.
pub fn delete_save(save: &Path) -> std::io::Result<()> {
    fs::remove_file(save)?;
    for extra in [meta_path(save), backup_path(save)] {
        match fs::remove_file(&extra) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => log::warn!("cannot remove {}: {err}", extra.display()),
        }
    }
    log::info!("deleted save {}", save.display());
    Ok(())
}

/// The autosave slot to write next: the first free slot, else the one written longest ago
/// (an unreadable slot counts as oldest).
pub fn next_autosave_path(dir: &Path) -> PathBuf {
    let mut oldest: Option<(u64, PathBuf)> = None;
    for n in 1..=AUTOSAVE_SLOTS {
        let path = save_path(dir, &format!("{AUTOSAVE_PREFIX}{n}"));
        if !path.exists() {
            return path;
        }
        let saved_at = read_header_from_file(&path).map_or(0, |h| h.saved_at_unix);
        if oldest.as_ref().is_none_or(|(t, _)| saved_at < *t) {
            oldest = Some((saved_at, path));
        }
    }
    oldest.map_or_else(|| save_path(dir, &format!("{AUTOSAVE_PREFIX}1")), |(_, p)| p)
}

fn io_error(path: &Path, source: std::io::Error) -> SaveError {
    SaveError::Io { path: path.to_path_buf(), source }
}

/// Writes save bytes safely: to a temporary file that is flushed to disk, the old save (if
/// any) kept as backup, the new file renamed into place and read back; the backup is removed
/// only after the new file checked out, otherwise it is put back.
pub fn write_save_bytes(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|e| io_error(dir, e))?;
    let tmp = temp_path(path);
    let written = (|| {
        let mut file = fs::File::create(&tmp).map_err(|e| io_error(&tmp, e))?;
        file.write_all(bytes).map_err(|e| io_error(&tmp, e))?;
        file.sync_all().map_err(|e| io_error(&tmp, e))
    })();
    if let Err(err) = written {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    let backup = backup_path(path);
    let had_old = path.is_file();
    if had_old {
        let _ = fs::remove_file(&backup);
        // A hard link costs nothing; file systems without links get a copy.
        if fs::hard_link(path, &backup).is_err()
            && let Err(err) = fs::copy(path, &backup)
        {
            let _ = fs::remove_file(&tmp);
            return Err(io_error(&backup, err));
        }
    }
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_file(&backup);
        return Err(io_error(path, err));
    }
    sync_dir(dir);
    let verified = match fs::read(path) {
        Ok(read) if read == bytes => Ok(()),
        Ok(_) => Err(io_error(path, std::io::Error::new(ErrorKind::InvalidData, "written save differs from the data"))),
        Err(err) => Err(io_error(path, err)),
    };
    match verified {
        Ok(()) => {
            if had_old {
                let _ = fs::remove_file(&backup);
            }
            Ok(())
        }
        Err(err) => {
            if had_old {
                // Put the previous save back.
                if let Err(restore) = fs::rename(&backup, path) {
                    log::error!("cannot restore {} from its backup: {restore}", path.display());
                }
            }
            Err(err)
        }
    }
}

/// Flushes a directory entry change (the rename) to disk where the platform allows it.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = fs::File::open(dir)
        && let Err(err) = handle.sync_all()
    {
        log::debug!("cannot sync {}: {err}", dir.display());
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Day of a save in local time: today, yesterday or a date ("21.11.2026"), and the time
/// ("14:30"); `None` for an impossible timestamp.
fn saved_at_local(unix: u64) -> Option<(Option<bool>, String, String)> {
    let ts = jiff::Timestamp::from_second(i64::try_from(unix).unwrap_or(i64::MAX)).ok()?;
    let tz = jiff::tz::TimeZone::system();
    let when = ts.to_zoned_datetime(tz.clone());
    let today = jiff::Timestamp::now().to_zoned_datetime(tz).date();
    let time = format!("{:02}:{:02}", when.hour(), when.minute());
    let date = format!("{:02}.{:02}.{}", when.day(), when.month(), when.year());
    let recent = if when.date() == today {
        Some(true)
    } else if today.yesterday().is_ok_and(|y| y == when.date()) {
        Some(false)
    } else {
        None
    };
    Some((recent, date, time))
}

/// Local date and time of a save for lists: "Heute, 14:30", "Gestern, 09:05" or
/// "21.11.2026, 18:42" (system time zone).
pub fn saved_at_text(unix: u64) -> String {
    match saved_at_local(unix) {
        Some((Some(true), _, time)) => format!("Heute, {time}"),
        Some((Some(false), _, time)) => format!("Gestern, {time}"),
        Some((None, date, time)) => format!("{date}, {time}"),
        None => "–".to_owned(),
    }
}

/// Local date and time of a save inside a sentence: "heute um 14:30", "gestern um 09:05" or
/// "am 21.11.2026 um 18:42".
pub fn saved_at_phrase(unix: u64) -> String {
    match saved_at_local(unix) {
        Some((Some(true), _, time)) => format!("heute um {time}"),
        Some((Some(false), _, time)) => format!("gestern um {time}"),
        Some((None, date, time)) => format!("am {date} um {time}"),
        None => "zu unbekannter Zeit".to_owned(),
    }
}

/// Playing time as "3 h 05 min" or "12 min".
pub fn play_time_text(seconds: u64) -> String {
    let minutes = seconds / 60;
    if minutes < 60 { format!("{minutes} min") } else { format!("{} h {:02} min", minutes / 60, minutes % 60) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kingdoms-slots-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn names_become_safe_file_stems() {
        assert_eq!(slug("Mein Königreich"), "mein_koenigreich");
        assert_eq!(slug("  Straße nach Süden!  "), "strasse_nach_sueden");
        assert_eq!(slug("Burg / Tor: 2"), "burg_tor_2");
        assert_eq!(slug("ÄÖÜ"), "aeoeue");
        assert_eq!(slug("..."), "");
        assert_eq!(slug(&"x".repeat(80)).len(), MAX_STEM_CHARS);
        assert_eq!(check_name("Mein Reich"), Ok("mein_reich".to_owned()));
        assert_eq!(check_name("   "), Err(NameError::Empty));
        assert_eq!(check_name("!!"), Err(NameError::Empty));
        assert_eq!(check_name(&"a".repeat(41)), Err(NameError::TooLong));
        assert_eq!(check_name("Quicksave"), Err(NameError::Reserved));
        assert_eq!(check_name("Autosave 2"), Err(NameError::Reserved));
        assert_eq!(check_name("Autosave 9"), Err(NameError::Reserved));
        assert!(NameError::Reserved.message().contains("reserviert"));
    }

    #[test]
    fn saves_are_grouped_by_game_newest_first() {
        use kingdoms_sim::defs::Defs;
        use kingdoms_sim::state::{GameSettings, GameState};
        use kingdoms_sim::world::WorldSize;
        let defs = match Defs::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("data")) {
            Ok(d) => d,
            Err(e) => panic!("data: {e}"),
        };
        let a = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let b = GameState::new(&defs, GameSettings::new(&defs, 8, WorldSize::Small));
        assert_ne!(a.game_id(), b.game_id());
        let entry = |stem: &str, state: Option<&GameState>, at: u64| SaveEntry {
            path: PathBuf::from(format!("{stem}.ksav")),
            stem: stem.to_owned(),
            kind: SlotKind::of_stem(stem),
            header: state.map(|s| SaveHeader::for_state(s, &defs, stem, at)).ok_or_else(|| "kaputt".to_owned()),
            meta: None,
            compatibility: Compatibility::Compatible,
            modified_unix: at,
            size: 0,
        };
        // Newest first, as `list_saves` returns them.
        let mut list = vec![
            entry("b3", Some(&b), 60),
            entry("broken", None, 55),
            entry("a2", Some(&a), 50),
            entry("b2", Some(&b), 40),
            entry("a1", Some(&a), 30),
            entry("b1", Some(&b), 20),
        ];
        group_by_game(&mut list);
        let stems: Vec<&str> = list.iter().map(|e| e.stem.as_str()).collect();
        assert_eq!(stems, ["b3", "b2", "b1", "a2", "a1", "broken"]);
        assert!(same_game(&list[0], &list[2]) && !same_game(&list[2], &list[3]) && !same_game(&list[4], &list[5]));
        // „Fortsetzen“ still takes the newest loadable save, whatever the order.
        assert_eq!(newest_loadable(&list).map(|e| e.stem.as_str()), Some("b3"));
    }

    #[test]
    fn slot_kinds_from_file_names() {
        assert_eq!(SlotKind::of_stem("quicksave"), SlotKind::Quick);
        assert_eq!(SlotKind::of_stem("autosave_2"), SlotKind::Auto(2));
        assert_eq!(SlotKind::of_stem("autosave_7"), SlotKind::Manual);
        assert_eq!(SlotKind::of_stem("mein_reich"), SlotKind::Manual);
        let dir = Path::new("saves");
        assert_eq!(meta_path(&save_path(dir, "a")), dir.join("a.kmeta"));
        assert_eq!(backup_path(&save_path(dir, "a")), dir.join("a.ksav.bak"));
    }

    #[test]
    fn safe_write_replaces_and_cleans_up() {
        let dir = temp_dir("write");
        let path = save_path(&dir, "a");
        write_save_bytes(&path, b"first").expect("first write");
        assert_eq!(fs::read(&path).expect("read"), b"first");
        write_save_bytes(&path, b"second version").expect("second write");
        assert_eq!(fs::read(&path).expect("read"), b"second version");
        assert!(!backup_path(&path).exists() && !temp_path(&path).exists());
        // A directory where the file belongs: the error is reported, nothing is left over.
        let blocked = save_path(&dir, "blocked");
        fs::create_dir_all(&blocked).expect("dir");
        assert!(write_save_bytes(&blocked, b"x").is_err());
        assert!(!temp_path(&blocked).exists() && !backup_path(&blocked).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn interrupted_writes_are_repaired() {
        let dir = temp_dir("recover");
        // Crash after the old save became the backup but before the new one was in place.
        let lost = save_path(&dir, "lost");
        fs::write(backup_path(&lost), b"old").expect("backup");
        // Crash after the new save was in place but before the backup was removed (the save
        // itself is unreadable here, so the backup wins).
        let broken = save_path(&dir, "broken");
        fs::write(&broken, b"garbage").expect("save");
        fs::write(backup_path(&broken), b"previous").expect("backup");
        fs::write(temp_path(&save_path(&dir, "unfinished")), b"half").expect("tmp");
        recover_backups(&dir);
        assert_eq!(fs::read(&lost).expect("restored"), b"old");
        assert_eq!(fs::read(&broken).expect("restored"), b"previous");
        assert!(!backup_path(&lost).exists() && !backup_path(&broken).exists());
        remove_stale_temp_files(&dir);
        assert!(!temp_path(&save_path(&dir, "unfinished")).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn play_time_and_dates_read_naturally() {
        assert_eq!(play_time_text(59), "0 min");
        assert_eq!(play_time_text(12 * 60 + 5), "12 min");
        assert_eq!(play_time_text(3 * 3600 + 5 * 60), "3 h 05 min");
        let now = jiff::Timestamp::now().as_second() as u64;
        assert!(saved_at_text(now).starts_with("Heute, "));
        assert!(saved_at_text(now - 86_400).starts_with("Gestern, ") || saved_at_text(now - 86_400).starts_with("Heute, "));
        let old = saved_at_text(1_000_000_000);
        assert!(old.contains(".2001, "), "{old}");
        assert!(saved_at_phrase(now).starts_with("heute um "), "{}", saved_at_phrase(now));
        let phrase = saved_at_phrase(1_000_000_000);
        assert!(phrase.starts_with("am ") && phrase.contains(".2001 um "), "{phrase}");
    }
}
