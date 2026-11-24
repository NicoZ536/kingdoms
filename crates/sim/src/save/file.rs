//! File helpers for save games: atomic writes and validated loads.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::{PREFIX_LEN, SaveError, SaveGame, SaveHeader, decode, encode, peek_header};
use crate::defs::Defs;
use crate::state::GameState;

fn io_error(path: &Path, source: std::io::Error) -> SaveError {
    SaveError::Io { path: path.to_path_buf(), source }
}

/// Path of the temporary file used while writing `path` (`<name>.tmp` in the same directory, so
/// the final rename stays on one file system).
fn temp_path(path: &Path) -> PathBuf {
    let mut name: OsString = path.file_name().map(OsString::from).unwrap_or_else(|| OsString::from("save"));
    name.push(".tmp");
    path.with_file_name(name)
}

/// Writes `bytes` to `path` atomically: the data goes to a temporary file in the same directory,
/// is flushed to disk and then renamed over the target. A crash never leaves a half-written file
/// under `path`. Missing parent directories are created.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    }
    let tmp = temp_path(path);
    let result = (|| {
        let mut file = File::create(&tmp).map_err(|e| io_error(&tmp, e))?;
        file.write_all(bytes).map_err(|e| io_error(&tmp, e))?;
        file.sync_all().map_err(|e| io_error(&tmp, e))?;
        drop(file);
        fs::rename(&tmp, path).map_err(|e| io_error(path, e))
    })();
    if result.is_err() {
        // Best effort cleanup; the original error is what matters.
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Encodes and atomically writes a save game.
pub fn save_to_file(path: &Path, header: &SaveHeader, state: &GameState) -> Result<(), SaveError> {
    let bytes = encode(header, state)?;
    write_atomic(path, &bytes)?;
    log::info!("game saved to {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

/// Reads and fully validates a save game.
pub fn load_from_file(path: &Path, defs: &Defs) -> Result<SaveGame, SaveError> {
    let bytes = fs::read(path).map_err(|e| io_error(path, e))?;
    let save = decode(&bytes, defs)?;
    log::info!("game loaded from {} ({} bytes)", path.display(), bytes.len());
    Ok(save)
}

/// Reads only the header of a save file (for load menus) without reading the whole state. A
/// file whose length differs from the length its prefix declares (cut off, or with bytes
/// appended) is reported as [`SaveError::LengthMismatch`]; the checksum is checked on loading.
pub fn read_header_from_file(path: &Path) -> Result<SaveHeader, SaveError> {
    let mut file = File::open(path).map_err(|e| io_error(path, e))?;
    let mut prefix = vec![0u8; PREFIX_LEN];
    let mut filled = 0;
    while filled < PREFIX_LEN {
        let n = file.read(&mut prefix[filled..]).map_err(|e| io_error(path, e))?;
        if n == 0 {
            return Err(SaveError::Truncated { len: filled });
        }
        filled += n;
    }
    let header_len = u32::from_le_bytes([prefix[16], prefix[17], prefix[18], prefix[19]]) as u64;
    // A file cut off after its header (or with garbage appended) would otherwise be listed as
    // loadable: compare the length the prefix declares with the file's length.
    if let Ok(p) = super::parse_prefix(&prefix)
        && p.version == super::SAVE_FORMAT_VERSION
    {
        let actual = file.metadata().map_err(|e| io_error(path, e))?.len();
        let declared = (PREFIX_LEN as u64).checked_add(p.header_len as u64).and_then(|n| n.checked_add(p.body_len)).unwrap_or(u64::MAX);
        if declared != actual {
            return Err(SaveError::LengthMismatch { declared, actual });
        }
    }
    let mut rest = Vec::new();
    file.take(header_len).read_to_end(&mut rest).map_err(|e| io_error(path, e))?;
    prefix.extend_from_slice(&rest);
    match peek_header(&prefix) {
        // Older formats need the full file for migration.
        Err(SaveError::Truncated { .. }) | Err(SaveError::LengthMismatch { .. }) => {
            let bytes = fs::read(path).map_err(|e| io_error(path, e))?;
            peek_header(&bytes)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::GameSettings;
    use crate::test_support::defs;
    use crate::world::WorldSize;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kingdoms_sim_save_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_load_file_roundtrip_and_header_only_read() {
        let defs = defs();
        let dir = scratch_dir("roundtrip");
        let path = dir.join("slots").join("slot1.ksav");
        let mut state = GameState::new(defs, GameSettings::new(defs, 5, WorldSize::Small));
        for _ in 0..30 {
            state.tick(defs, Vec::new());
        }
        let header = SaveHeader::for_state(&state, defs, "Mein Reich", 42);
        save_to_file(&path, &header, &state).unwrap();
        assert!(!temp_path(&path).exists(), "temporary file must be renamed away");
        assert_eq!(read_header_from_file(&path).unwrap(), header);
        let loaded = load_from_file(&path, defs).unwrap();
        assert_eq!(loaded.header, header);
        assert_eq!(loaded.state.clock(), state.clock());

        // Overwriting keeps a valid file.
        state.tick(defs, Vec::new());
        let header2 = SaveHeader::for_state(&state, defs, "Mein Reich", 43);
        save_to_file(&path, &header2, &state).unwrap();
        assert_eq!(load_from_file(&path, defs).unwrap().header.saved_at_unix, 43);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_and_damaged_files_report_errors() {
        let defs = defs();
        let dir = scratch_dir("errors");
        let missing = dir.join("nope.ksav");
        assert!(matches!(load_from_file(&missing, defs), Err(SaveError::Io { .. })));
        assert!(matches!(read_header_from_file(&missing), Err(SaveError::Io { .. })));
        let short = dir.join("short.ksav");
        write_atomic(&short, b"KNGD").unwrap();
        assert!(matches!(read_header_from_file(&short), Err(SaveError::Truncated { .. })));
        assert!(matches!(load_from_file(&short, defs), Err(SaveError::Truncated { .. })));
        // A save cut off after its header is not listed as loadable.
        let state = GameState::new(defs, GameSettings::new(defs, 5, WorldSize::Small));
        let header = SaveHeader::for_state(&state, defs, "Abgeschnitten", 1);
        let bytes = encode(&header, &state).unwrap();
        let cut = dir.join("cut.ksav");
        write_atomic(&cut, &bytes[..PREFIX_LEN + 2000.min(bytes.len() - PREFIX_LEN - 1)]).unwrap();
        assert!(matches!(read_header_from_file(&cut), Err(SaveError::LengthMismatch { .. })), "{:?}", read_header_from_file(&cut));
        let mut longer = bytes.clone();
        longer.extend_from_slice(b"garbage");
        write_atomic(&cut, &longer).unwrap();
        assert!(matches!(read_header_from_file(&cut), Err(SaveError::LengthMismatch { .. })));
        write_atomic(&cut, &bytes).unwrap();
        assert_eq!(read_header_from_file(&cut).unwrap(), header);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn temp_path_is_next_to_target() {
        assert_eq!(temp_path(Path::new("/a/b/game.ksav")), PathBuf::from("/a/b/game.ksav.tmp"));
    }
}
