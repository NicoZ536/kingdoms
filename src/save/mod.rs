//! Save games of the running game in the user's saves directory (`GamePaths::saves_dir`):
//! named saves, the quicksave (F5/F9) and three rotating autosaves ([`slots`]), written in the
//! background ([`writer`]). Format, checksums and validation are the simulation's
//! (`kingdoms_sim::save`); this module names files, keeps the list metadata and explains errors
//! in German.

pub mod slots;
mod writer;

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kingdoms_sim::defs::Defs;
use kingdoms_sim::save::{SAVE_EXTENSION, SaveError, SaveGame, load_from_file, read_header_from_file};

pub use slots::SlotKind;
pub use writer::{SaveJob, SaveOutcome, SaveWriter};


/// File name (without extension) of the quicksave.
pub const QUICKSAVE_NAME: &str = "quicksave";

/// Current wall-clock time in Unix seconds (stored in save headers; the simulation never reads
/// the clock itself).
pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Path of the quicksave file in `saves_dir`.
pub fn quicksave_path(saves_dir: &Path) -> PathBuf {
    saves_dir.join(format!("{QUICKSAVE_NAME}.{SAVE_EXTENSION}"))
}

/// Short German description of the quicksave ("Schnellspielstand: Frühling · Tag 3 · …"), or
/// `None` if there is none or its header cannot be read.
pub fn quicksave_info(saves_dir: &Path) -> Option<String> {
    let header = read_header_from_file(&quicksave_path(saves_dir)).ok()?;
    Some(format!("Schnellspielstand: {}", header.summary.date))
}

/// A loaded save: the validated game plus the playing time and the notification list from its
/// metadata.
pub struct LoadedGame {
    pub game: SaveGame,
    pub play_seconds: u64,
    pub notifications: Vec<slots::SavedNotification>,
}

impl LoadedGame {
    /// The game as a session, with the playing time and the notification list it was saved with.
    pub fn into_session(self, defs: std::sync::Arc<Defs>) -> crate::session::Session {
        crate::session::Session::from_state(defs, self.game.state, self.play_seconds).with_restored_notifications(self.notifications)
    }
}

/// Reads and fully validates a save (version, checksum, data fingerprint, consistency) and its
/// metadata.
pub fn load(path: &Path, defs: &Defs) -> Result<LoadedGame, SaveError> {
    let game = load_from_file(path, defs)?;
    let (play_seconds, notifications) = trusted_meta(path, &game.header);
    Ok(LoadedGame { game, play_seconds, notifications })
}

/// Playing time and notification list from the metadata of the save at `path`, if it belongs
/// to the save with `header` (else none: 0 seconds, no notifications).
pub fn trusted_meta(path: &Path, header: &kingdoms_sim::save::SaveHeader) -> (u64, Vec<slots::SavedNotification>) {
    slots::read_meta(path).filter(|m| m.matches(header)).map_or((0, Vec::new()), |m| (m.play_seconds, m.notifications))
}

/// German explanation of a save/load error for the player (the raw error, often an English
/// operating-system text, goes to the log only).
pub fn describe_error(err: &SaveError) -> String {
    match err {
        SaveError::Io { source, .. } => match source.kind() {
            ErrorKind::NotFound => "Die Spielstand-Datei ist nicht (mehr) vorhanden.".to_owned(),
            ErrorKind::PermissionDenied => "Keine Berechtigung für den Spielstand-Ordner.".to_owned(),
            ErrorKind::StorageFull => "Der Datenträger ist voll.".to_owned(),
            ErrorKind::ReadOnlyFilesystem => "Der Spielstand-Ordner ist schreibgeschützt.".to_owned(),
            ErrorKind::IsADirectory => "Anstelle der Spielstand-Datei liegt dort ein Ordner.".to_owned(),
            ErrorKind::NotADirectory => "Der Pfad zum Spielstand-Ordner führt nicht zu einem Ordner.".to_owned(),
            ErrorKind::InvalidFilename => "Der Name der Spielstand-Datei ist ungültig.".to_owned(),
            ErrorKind::InvalidData => "Der Spielstand wurde nicht vollständig geschrieben.".to_owned(),
            _ => "Die Spielstand-Datei ist nicht zugänglich.".to_owned(),
        },
        SaveError::Truncated { .. }
        | SaveError::ChecksumMismatch { .. }
        | SaveError::LengthMismatch { .. }
        | SaveError::Decode(_) => "Der Spielstand ist beschädigt.".to_owned(),
        SaveError::BadMagic => "Die Datei ist kein KINGDOMS-Spielstand.".to_owned(),
        SaveError::UnsupportedVersion { found, min, current } => {
            if min == current {
                format!("Der Spielstand hat das Format {found}; dieses Spiel liest nur Format {current}.")
            } else {
                format!("Der Spielstand hat das Format {found}; unterstützt werden {min} bis {current}.")
            }
        }
        SaveError::MissingMigration { from } | SaveError::Migration { from, .. } => {
            format!("Der Spielstand im Format {from} kann nicht übernommen werden.")
        }
        SaveError::Encode(_) => "Der Spielstand konnte nicht erstellt werden.".to_owned(),
        SaveError::HeaderMismatch(_) => "Der Spielstand ist in sich widersprüchlich.".to_owned(),
        SaveError::DataMismatch { .. } => "Der Spielstand stammt aus einer anderen Version der Spieldaten.".to_owned(),
        SaveError::InvalidState(_) => "Der Spielstand enthält einen ungültigen Spielzustand.".to_owned(),
    }
}

/// [`describe_error`] for the quickload key: a missing file means there is no quicksave yet.
pub fn describe_quickload_error(err: &SaveError) -> String {
    match err {
        SaveError::Io { source, .. } if source.kind() == ErrorKind::NotFound => "Es gibt noch keinen Schnellspielstand.".to_owned(),
        other => describe_error(other),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kingdoms_sim::command::Command;
    use kingdoms_sim::save::{SaveHeader, encode};
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    use super::slots::{Compatibility, SaveEntry, list_saves, newest_loadable, next_autosave_path, save_path};
    use super::*;
    use crate::session::Session;

    /// Writes the quicksave right away.
    fn quicksave(session: &Session, saves_dir: &Path) -> Result<PathBuf, SaveError> {
        let path = quicksave_path(saves_dir);
        let outcome = SaveJob::new(session, path, SlotKind::Quick, "Schnellspielstand", now_unix()).run();
        outcome.result.map(|()| outcome.path)
    }

    fn quickload(defs: &Defs, saves_dir: &Path) -> Result<SaveGame, SaveError> {
        load_from_file(&quicksave_path(saves_dir), defs)
    }

    fn defs() -> Arc<Defs> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => Arc::new(defs),
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kingdoms-save-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn quicksave_round_trip() {
        let defs = defs();
        let mut session = Session::new(defs.clone(), GameSettings::new(&defs, 5, WorldSize::Small));
        session.queue_command(Command::SetTaxRate { percent: 14 });
        for _ in 0..30 {
            session.tick();
        }
        let dir = temp_dir("roundtrip").join("saves");
        assert!(quicksave_info(&dir).is_none());
        let missing = quickload(&defs, &dir).expect_err("no save yet");
        assert_eq!(describe_quickload_error(&missing), "Es gibt noch keinen Schnellspielstand.");
        assert_eq!(describe_error(&missing), "Die Spielstand-Datei ist nicht (mehr) vorhanden.");

        let path = quicksave(&session, &dir).expect("quicksave");
        assert_eq!(path, quicksave_path(&dir));
        assert!(quicksave_info(&dir).is_some_and(|i| i.starts_with("Schnellspielstand: ")));
        let loaded = quickload(&defs, &dir).expect("quickload");
        assert_eq!(loaded.state.clock(), session.state().clock());
        assert_eq!(loaded.state.policy().tax_rate_percent, 14);

        // A damaged file is reported, not loaded.
        let mut bytes = std::fs::read(&path).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&path, bytes).expect("write");
        let err = quickload(&defs, &dir).expect_err("corrupt");
        assert_eq!(describe_error(&err), "Der Spielstand ist beschädigt.");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(&dir));
    }

    #[test]
    fn io_errors_are_explained_in_german_only() {
        // A directory where the quicksave file belongs: saving fails, the text has no English
        // operating-system part.
        let defs = defs();
        let session = Session::new(defs.clone(), GameSettings::new(&defs, 5, WorldSize::Small));
        let dir = temp_dir("dir-in-the-way").join("saves");
        std::fs::create_dir_all(quicksave_path(&dir)).expect("directory in the way");
        let err = quicksave(&session, &dir).expect_err("cannot overwrite a directory");
        let text = describe_error(&err);
        assert!(!text.contains("os error") && !text.contains('('), "{text}");
        let other = SaveError::Io { path: dir.clone(), source: std::io::Error::other("Something English") };
        assert_eq!(describe_error(&other), "Die Spielstand-Datei ist nicht zugänglich.");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(&dir));
    }

    /// The list shows every save newest first, marks incompatible and damaged files with a
    /// German reason, and loading one of them reports the error instead of crashing.
    #[test]
    fn save_list_with_metadata_and_incompatible_saves() {
        let defs = defs();
        let mut session = Session::new(defs.clone(), GameSettings::new(&defs, 9, WorldSize::Small));
        session.tick();
        let dir = temp_dir("list").join("saves");
        let mut writer = SaveWriter::default();
        writer.submit(SaveJob::new(&session, save_path(&dir, "alt"), SlotKind::Manual, "Alter Stand", 1_000));
        writer.submit(SaveJob::new(&session, save_path(&dir, "neu"), SlotKind::Manual, "Neuer Stand", 3_000));
        writer.submit(SaveJob::new(&session, save_path(&dir, "autosave_1"), SlotKind::Auto(1), "Automatisch", 2_000));
        assert!(writer.finish().iter().all(|o| o.result.is_ok()));
        // Saved with other game data: same format, different fingerprint.
        let mut header = SaveHeader::for_state(session.state(), &defs, "Fremde Daten", 4_000);
        header.defs_fingerprint ^= 1;
        let bytes = encode(&header, session.state()).expect("encode");
        std::fs::write(save_path(&dir, "fremd"), bytes).expect("write");
        // Not a save at all.
        std::fs::write(save_path(&dir, "kaputt"), b"this is certainly not a kingdoms save game file").expect("write");

        let entries = list_saves(&dir, defs.fingerprint());
        let names: Vec<String> = entries.iter().map(SaveEntry::display_name).collect();
        // The damaged file has no header: its file time (now) sorts it first.
        assert_eq!(names, ["kaputt", "Fremde Daten", "Neuer Stand", "Autospeicherstand 1", "Alter Stand"]);
        let foreign = &entries[1];
        assert!(!foreign.loadable());
        assert!(matches!(&foreign.compatibility, Compatibility::Incompatible(r) if r.starts_with("Mit anderen Spieldaten")));
        assert!(foreign.meta.is_none());
        let broken = &entries[0];
        assert!(!broken.loadable());
        assert!(matches!(&broken.compatibility, Compatibility::Incompatible(r) if r == "Die Datei ist kein KINGDOMS-Spielstand."));
        assert!(entries[2].loadable() && entries[2].meta.is_some());
        assert_eq!(newest_loadable(&entries).map(SaveEntry::display_name).as_deref(), Some("Neuer Stand"));

        let err = load(&foreign.path, &defs).err().expect("foreign data is refused");
        assert_eq!(describe_error(&err), "Der Spielstand stammt aus einer anderen Version der Spieldaten.");
        assert!(load(&broken.path, &defs).is_err());
        let loaded = load(&entries[2].path, &defs).expect("load");
        assert_eq!(loaded.game.state.clock(), session.state().clock());

        slots::delete_save(&entries[2].path).expect("delete");
        assert!(!entries[2].path.exists() && !slots::meta_path(&entries[2].path).exists());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(&dir));
    }

    #[test]
    fn autosaves_rotate_through_three_slots() {
        let defs = defs();
        let session = Session::new(defs.clone(), GameSettings::new(&defs, 4, WorldSize::Small));
        let dir = temp_dir("autosave").join("saves");
        let mut written = Vec::new();
        for (i, at) in [100u64, 200, 300, 400, 500].into_iter().enumerate() {
            let path = next_autosave_path(&dir);
            written.push(path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            let outcome = SaveJob::new(&session, path, SlotKind::Auto(0), "Automatisch", at).run();
            assert!(outcome.result.is_ok(), "autosave {i}");
        }
        assert_eq!(written, ["autosave_1", "autosave_2", "autosave_3", "autosave_1", "autosave_2"]);
        let entries = list_saves(&dir, defs.fingerprint());
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|e| matches!(e.kind, SlotKind::Auto(_))));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(&dir));
    }
}
