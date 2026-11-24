//! Versioned save games (docs/architecture.md §3.4).
//!
//! Binary layout (all integers little-endian):
//!
//! | Offset | Size | Field |
//! |---|---|---|
//! | 0 | 8 | magic [`SAVE_MAGIC`] |
//! | 8 | 4 | format version |
//! | 12 | 4 | CRC32 of every byte from offset 16 to the end |
//! | 16 | 4 | header length `H` |
//! | 20 | 8 | body length `B` |
//! | 28 | `H` | [`SaveHeader`] (bincode 2, serde) |
//! | 28+`H` | `B` | [`GameState`] (bincode 2, serde) |
//!
//! The header is separate from the body so save lists can be shown without decoding the whole
//! state ([`peek_header`]). Loading validates magic, version (running [`MIGRATIONS`] for older
//! supported versions), checksum, lengths, the header/body agreement, the data fingerprint and the
//! structural consistency of the state. The complete state including the RNG is stored, so a
//! loaded game continues exactly as the saved one would have.

mod file;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use file::{load_from_file, read_header_from_file, save_to_file, write_atomic};

use crate::defs::Defs;
use crate::state::{Difficulty, GameState, StateError};
use crate::world::WorldSize;

/// File magic: identifies KINGDOMS save games.
pub const SAVE_MAGIC: [u8; 8] = *b"KNGDMSAV";
/// Current save format version. Bump it whenever the serialized layout of [`SaveHeader`] or
/// [`GameState`] changes, and add a [`Migration`] from the previous version if old saves should
/// stay loadable.
///
/// History: 1 = foundation (world, clock, policy); 2 = buildings, roads, territory, economy
/// stock/treasury, research, workforce; 3 = storage inventories, production states, transport
/// jobs, treasury in milli-gold, ledger, prices, statistics, trade orders; 4 = upgrade lock of
/// houses, hourly statistics history, data-shape fingerprint; 5 = citizens, households,
/// homes, job index, population statistics, household consumption statistics; 6 = weather
/// (incl. its RNG, snow cover, ground wetness), farms with their claimed field/pasture tiles
/// and crops, tree stumps; 7 = citizen positions and trips (walking, path requests,
/// travellers leaving the kingdom, "Kein Weg" cooldowns); 8 = walking carriers (transport
/// stages with the carrier, goods on carriers, trip statistics), gatherers' work spots and
/// commute flags; 9 = research queue, progress, payments and statistics, events (pending
/// decision, cooldowns, modifiers, tax cap, history, RNG), the events switch in the settings,
/// repair sites, ledger categories for research and events; 10 = AI kingdoms (territory and
/// land map, structures, economy, memories, treaties, wars), relations and wars among them,
/// the diplomatic inbox, caravans (goods, purse, walkers) and the diplomatic history, caravan
/// walkers among the travellers, the ledger category for diplomacy; 11 = military (soldiers
/// with unit, training, health, morale, equipment, experience and duty, companies and orders,
/// enemy groups with their fighters and plunder, campaigns, battle reports, structure damage,
/// alarm and muster, its RNG), the soldier unit of citizens, enemy walkers among the
/// travellers, the destination kind `Tile`, ruler rank and game over, the defeat switch in the
/// settings, the resource `armor`; 12 = remembered paths, wind, herds, reachability hours,
/// last work tick of sites; 13 = household purses, friends, upgrade targets of all building
/// kinds, caravan routes, the game id; 14 = the output record of production buildings
/// (cycles and goods per day), campaign provisions and soldier-hours, the stuck counter of
/// withdrawing armies; 15 = work experience per job category and sick leave of citizens, the
/// production status `NoPlantingSpace`, household savings in the daily population history,
/// the long-term season history of the statistics; 16 = the source of temporary modifiers
/// (event or the player's festival) and the day of the last festival, the relation memory
/// „Festgäste“, the tick of the last AI proposal (inbox pacing); 17 = the dry spell of the
/// weather (days without rain, read by the drought event) and the event behind a forced
/// weather. Versions 1–16 are internal
/// development builds only and are not migrated.
pub const SAVE_FORMAT_VERSION: u32 = 17;
/// Oldest format version that can still be loaded (via migrations).
pub const MIN_SUPPORTED_FORMAT_VERSION: u32 = 17;
/// Recommended file extension.
pub const SAVE_EXTENSION: &str = "ksav";
/// Version of the simulation crate, recorded in every save for diagnostics.
pub const GAME_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Size of the fixed prefix before the header bytes.
const PREFIX_LEN: usize = 28;
/// Upper bound for decoded allocations (protects against absurd lengths in damaged files).
const DECODE_LIMIT: usize = 1 << 30;

/// Errors of saving and loading.
#[derive(Debug, Error)]
pub enum SaveError {
    /// File system error.
    #[error("I/O error on {path}: {source}")]
    Io {
        /// Affected file.
        path: std::path::PathBuf,
        /// Cause.
        #[source]
        source: std::io::Error,
    },
    /// The data is shorter than the fixed prefix or the declared lengths.
    #[error("save data truncated ({len} bytes)")]
    Truncated {
        /// Available bytes.
        len: usize,
    },
    /// Not a KINGDOMS save game.
    #[error("not a KINGDOMS save game (wrong magic)")]
    BadMagic,
    /// Format version outside the supported range.
    #[error("unsupported save format version {found} (supported: {min}..={current})")]
    UnsupportedVersion {
        /// Version in the file.
        found: u32,
        /// Oldest supported version.
        min: u32,
        /// Current version.
        current: u32,
    },
    /// No migration step registered for a supported old version.
    #[error("no migration from save format version {from}")]
    MissingMigration {
        /// Version without a migration step.
        from: u32,
    },
    /// A migration step failed.
    #[error("migration from save format version {from} failed: {message}")]
    Migration {
        /// Source version of the failed step.
        from: u32,
        /// What went wrong.
        message: String,
    },
    /// The checksum does not match: the file is damaged.
    #[error("save data corrupted (checksum {computed:08x}, expected {stored:08x})")]
    ChecksumMismatch {
        /// Checksum stored in the file.
        stored: u32,
        /// Checksum of the actual data.
        computed: u32,
    },
    /// Declared lengths do not match the data.
    #[error("save data length mismatch (declared {declared} bytes, found {actual})")]
    LengthMismatch {
        /// Length according to the prefix.
        declared: u64,
        /// Actual length.
        actual: u64,
    },
    /// Serialization failed.
    #[error("cannot encode save game: {0}")]
    Encode(#[from] bincode::error::EncodeError),
    /// Deserialization failed.
    #[error("cannot decode save game: {0}")]
    Decode(#[from] bincode::error::DecodeError),
    /// Header and body (or prefix) disagree.
    #[error("inconsistent save game: {0}")]
    HeaderMismatch(&'static str),
    /// The save was made with different game data (ids would be misinterpreted).
    #[error("save game was created with different game data (fingerprint {found:016x}, current {expected:016x})")]
    DataMismatch {
        /// Fingerprint of the loaded data.
        expected: u64,
        /// Fingerprint stored in the save.
        found: u64,
    },
    /// The state is structurally inconsistent.
    #[error("invalid game state: {0}")]
    InvalidState(#[from] StateError),
}

/// Short description of a save for load menus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSummary {
    /// Simulation tick.
    pub tick: u64,
    /// German date, e.g. `Herbst · Tag 2 · Jahr 3 · 14:30`.
    pub date: String,
    /// Map size.
    pub world_size: WorldSize,
    /// Difficulty.
    pub difficulty: Difficulty,
    /// German scenario name.
    pub scenario: String,
}

/// Metadata stored in front of the state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveHeader {
    /// Copy of [`SAVE_MAGIC`].
    pub magic: [u8; 8],
    /// Copy of the format version.
    pub format_version: u32,
    /// Version of the game that wrote the save.
    pub game_version: String,
    /// Wall-clock time of saving (Unix seconds), supplied by the caller: the simulation itself
    /// never reads the clock.
    pub saved_at_unix: u64,
    /// World seed.
    pub seed: u64,
    /// Player-chosen save name.
    pub name: String,
    /// Description for load menus.
    pub summary: SaveSummary,
    /// [`Defs::fingerprint`] of the data the state refers to.
    pub defs_fingerprint: u64,
    /// Identity of the game ([`GameState::game_id`]): saves of the same game share it.
    pub game_id: uuid::Uuid,
}

impl SaveHeader {
    /// Builds the header for saving `state` now.
    pub fn for_state(state: &GameState, defs: &Defs, name: impl Into<String>, saved_at_unix: u64) -> Self {
        let settings = state.settings();
        SaveHeader {
            magic: SAVE_MAGIC,
            format_version: SAVE_FORMAT_VERSION,
            game_version: GAME_VERSION.to_string(),
            saved_at_unix,
            seed: settings.seed,
            name: name.into(),
            summary: SaveSummary {
                tick: state.clock().tick,
                date: state.clock().display(),
                world_size: settings.world_size,
                difficulty: settings.difficulty,
                scenario: defs.scenario(settings.scenario).name.clone(),
            },
            defs_fingerprint: defs.fingerprint(),
            game_id: state.game_id(),
        }
    }
}

/// A complete save game.
#[derive(Clone, Debug)]
pub struct SaveGame {
    /// Metadata.
    pub header: SaveHeader,
    /// Simulation state.
    pub state: GameState,
}

impl SaveGame {
    /// Encodes the save game (see [`encode`]).
    pub fn encode(&self) -> Result<Vec<u8>, SaveError> {
        encode(&self.header, &self.state)
    }
}

/// Header and body bytes of a payload, used by migrations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payload {
    /// Encoded [`SaveHeader`] of the payload's format version.
    pub header: Vec<u8>,
    /// Encoded [`GameState`] of the payload's format version.
    pub body: Vec<u8>,
}

/// One migration step: converts a payload of format version `from` into version `from + 1`
/// (typically by decoding legacy types and re-encoding the current ones).
#[derive(Clone, Copy, Debug)]
pub struct Migration {
    /// Source version.
    pub from: u32,
    /// Conversion.
    pub apply: fn(Payload) -> Result<Payload, SaveError>,
}

/// Registered migrations, one per supported old format version. Empty while the oldest
/// supported version is the current one.
pub const MIGRATIONS: &[Migration] = &[];

fn config() -> impl bincode::config::Config {
    bincode::config::standard()
}

fn decode_config() -> impl bincode::config::Config {
    bincode::config::standard().with_limit::<DECODE_LIMIT>()
}

/// Encodes a header and a state into the binary save format.
pub fn encode(header: &SaveHeader, state: &GameState) -> Result<Vec<u8>, SaveError> {
    let header_bytes = bincode::serde::encode_to_vec(header, config())?;
    let body = bincode::serde::encode_to_vec(state, config())?;
    let header_len = u32::try_from(header_bytes.len()).map_err(|_| SaveError::HeaderMismatch("header too large"))?;
    let mut out = Vec::with_capacity(PREFIX_LEN + header_bytes.len() + body.len());
    out.extend_from_slice(&SAVE_MAGIC);
    out.extend_from_slice(&header.format_version.to_le_bytes());
    out.extend_from_slice(&[0; 4]); // checksum placeholder
    out.extend_from_slice(&header_len.to_le_bytes());
    out.extend_from_slice(&(body.len() as u64).to_le_bytes());
    out.extend_from_slice(&header_bytes);
    out.extend_from_slice(&body);
    let crc = crc32fast::hash(&out[16..]);
    out[12..16].copy_from_slice(&crc.to_le_bytes());
    Ok(out)
}

/// The parsed fixed prefix.
struct Prefix {
    version: u32,
    crc: u32,
    header_len: usize,
    body_len: u64,
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    let mut b = [0; 4];
    b.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(b)
}

fn read_u64(bytes: &[u8], at: usize) -> u64 {
    let mut b = [0; 8];
    b.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(b)
}

/// Parses and checks the fixed prefix (magic, supported version).
fn parse_prefix(bytes: &[u8]) -> Result<Prefix, SaveError> {
    if bytes.len() < PREFIX_LEN {
        return Err(SaveError::Truncated { len: bytes.len() });
    }
    if bytes[..8] != SAVE_MAGIC {
        return Err(SaveError::BadMagic);
    }
    let version = read_u32(bytes, 8);
    if !(MIN_SUPPORTED_FORMAT_VERSION..=SAVE_FORMAT_VERSION).contains(&version) {
        return Err(SaveError::UnsupportedVersion {
            found: version,
            min: MIN_SUPPORTED_FORMAT_VERSION,
            current: SAVE_FORMAT_VERSION,
        });
    }
    Ok(Prefix {
        version,
        crc: read_u32(bytes, 12),
        header_len: read_u32(bytes, 16) as usize,
        body_len: read_u64(bytes, 20),
    })
}

fn decode_exact<T: serde::de::DeserializeOwned>(bytes: &[u8], what: &'static str) -> Result<T, SaveError> {
    let (value, used) = bincode::serde::decode_from_slice::<T, _>(bytes, decode_config())?;
    if used != bytes.len() {
        return Err(SaveError::HeaderMismatch(what));
    }
    Ok(value)
}

/// Runs the migration chain from `version` up to [`SAVE_FORMAT_VERSION`].
fn migrate(mut version: u32, mut payload: Payload, chain: &[Migration]) -> Result<Payload, SaveError> {
    while version < SAVE_FORMAT_VERSION {
        let step = chain.iter().find(|m| m.from == version).ok_or(SaveError::MissingMigration { from: version })?;
        payload = (step.apply)(payload)?;
        version += 1;
    }
    Ok(payload)
}

/// Checks the checksum and lengths, migrates and decodes, without validating against game data.
fn decode_with(bytes: &[u8], chain: &[Migration]) -> Result<SaveGame, SaveError> {
    let prefix = parse_prefix(bytes)?;
    let actual = bytes.len() as u64;
    // Crafted prefixes may declare lengths whose sum overflows: report them as a mismatch.
    let declared = (PREFIX_LEN as u64)
        .checked_add(prefix.header_len as u64)
        .and_then(|n| n.checked_add(prefix.body_len))
        .ok_or(SaveError::LengthMismatch { declared: u64::MAX, actual })?;
    if declared != actual {
        return Err(SaveError::LengthMismatch { declared, actual });
    }
    let truncated = || SaveError::Truncated { len: bytes.len() };
    let computed = crc32fast::hash(bytes.get(16..).ok_or_else(truncated)?);
    if computed != prefix.crc {
        return Err(SaveError::ChecksumMismatch { stored: prefix.crc, computed });
    }
    let header_end = PREFIX_LEN.checked_add(prefix.header_len).ok_or_else(truncated)?;
    let header = bytes.get(PREFIX_LEN..header_end).ok_or_else(truncated)?;
    let body = bytes.get(header_end..).ok_or_else(truncated)?;
    let payload = Payload { header: header.to_vec(), body: body.to_vec() };
    let payload = migrate(prefix.version, payload, chain)?;
    let header: SaveHeader = decode_exact(&payload.header, "trailing bytes after header")?;
    if header.magic != SAVE_MAGIC {
        return Err(SaveError::HeaderMismatch("header magic differs from file magic"));
    }
    if header.format_version != SAVE_FORMAT_VERSION {
        return Err(SaveError::HeaderMismatch("header format version differs from the payload version"));
    }
    let state: GameState = decode_exact(&payload.body, "trailing bytes after state")?;
    if header.seed != state.settings().seed {
        return Err(SaveError::HeaderMismatch("header seed differs from state seed"));
    }
    if header.game_id != state.game_id() {
        return Err(SaveError::HeaderMismatch("header game id differs from the state's"));
    }
    Ok(SaveGame { header, state })
}

/// Decodes and fully validates a save game against the loaded game data, then re-applies the
/// values derived from the data (capacities, territory, stock view; see [`Defs::fingerprint`]
/// for which data changes a save survives).
pub fn decode(bytes: &[u8], defs: &Defs) -> Result<SaveGame, SaveError> {
    let mut save = decode_with(bytes, MIGRATIONS)?;
    if save.header.defs_fingerprint != defs.fingerprint() {
        return Err(SaveError::DataMismatch { expected: defs.fingerprint(), found: save.header.defs_fingerprint });
    }
    save.state.validate(defs)?;
    save.state.rederive(defs);
    Ok(save)
}

/// Reads only the header (for load menus). Checks magic and version; the checksum is *not*
/// verified because that needs the whole file, which [`decode`] does.
pub fn peek_header(bytes: &[u8]) -> Result<SaveHeader, SaveError> {
    let prefix = parse_prefix(bytes)?;
    if prefix.version != SAVE_FORMAT_VERSION {
        // Old formats: the header layout may differ, so take the full (migrating) path.
        return decode_with(bytes, MIGRATIONS).map(|s| s.header);
    }
    let truncated = || SaveError::Truncated { len: bytes.len() };
    let end = PREFIX_LEN.checked_add(prefix.header_len).ok_or_else(truncated)?;
    let header_bytes = bytes.get(PREFIX_LEN..end).ok_or_else(truncated)?;
    decode_exact(header_bytes, "trailing bytes after header")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::state::GameSettings;
    use crate::test_support::defs;

    /// A valid spot for `kind` near the start, scanning outwards.
    fn find_spot(state: &GameState, kind: crate::defs::BuildingTypeId) -> (glam::IVec2, crate::grid::Rotation) {
        let rotation = crate::grid::Rotation::default();
        for r in 4..30 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let origin = state.world().start + glam::IVec2::new(dx, dy);
                    if crate::buildings::check_placement(state, defs(), kind, origin, rotation).ok {
                        return (origin, rotation);
                    }
                }
            }
        }
        panic!("no spot for {kind}");
    }

    /// A game after a little more than a day: a placed house and chicken farm with access
    /// roads, a trade order, production, transports and one closed day in the ledger.
    fn sample_save() -> (SaveHeader, GameState) {
        let defs = defs();
        let mut state = GameState::new(defs, GameSettings::new(defs, 99, WorldSize::Small));
        let house = defs.building_id("house_wood").unwrap();
        let farm = defs.building_id("chicken_farm").unwrap();
        let fields = defs.building_id("vegetable_farm").unwrap();
        let path = defs.road_id("path").unwrap();
        let access = |state: &GameState, kind| -> Vec<Command> {
            let (_, b) = state.buildings().iter().next_back().unwrap();
            assert_eq!(b.kind(), kind);
            crate::roads::plan_access_road(state, defs, kind, b.origin(), b.rotation(), path)
                .and_then(|plan| plan.to_command())
                .into_iter()
                .collect()
        };
        for i in 0..5200u32 {
            let cmds = match i {
                10 => vec![Command::SetTaxRate { percent: 17 }],
                20 => {
                    let (origin, rotation) = find_spot(&state, house);
                    vec![Command::PlaceBuilding { kind: house, origin, rotation }]
                }
                21 => access(&state, house),
                30 => {
                    let (origin, rotation) = find_spot(&state, farm);
                    vec![Command::PlaceBuilding { kind: farm, origin, rotation }]
                }
                31 => access(&state, farm),
                35 => {
                    let (origin, rotation) = find_spot(&state, fields);
                    vec![Command::PlaceBuilding { kind: fields, origin, rotation }]
                }
                36 => access(&state, fields),
                40 => vec![Command::SetTradeOrder {
                    order: crate::economy::TradeOrder {
                        resource: defs.resource_id("wood").unwrap(),
                        buy_below: None,
                        sell_above: Some(50),
                        daily_limit: 5,
                    },
                }],
                _ => Vec::new(),
            };
            state.tick(defs, cmds);
        }
        assert_eq!(state.buildings().len(), 6, "start buildings + house + two farms");
        assert!(state.agriculture().claimed_tiles() > 0, "the vegetable farm claimed fields");
        assert!(state.economy().ledger().history().len() >= 1, "a day was closed");
        let header = SaveHeader::for_state(&state, defs, "Testspiel", 1_700_000_000);
        (header, state)
    }

    #[test]
    fn roundtrip_is_byte_identical() {
        let defs = defs();
        let (header, state) = sample_save();
        let bytes = encode(&header, &state).unwrap();
        let loaded = decode(&bytes, defs).unwrap();
        assert_eq!(loaded.header, header);
        assert_eq!(loaded.state.clock(), state.clock());
        assert_eq!(loaded.state.policy(), state.policy());
        assert_eq!(loaded.state.world().tiles(), state.world().tiles());
        assert_eq!(loaded.state.economy(), state.economy());
        assert_eq!(loaded.state.logistics(), state.logistics());
        assert_eq!(loaded.state.territory(), state.territory());
        // Derived grids are rebuilt on load.
        for (id, b) in state.buildings().iter() {
            assert_eq!(loaded.state.building(id), Some(b));
            for p in b.rect().tiles() {
                assert_eq!(loaded.state.buildings().id_at(p), Some(id));
            }
        }
        assert_eq!(loaded.state.roads().len(), state.roads().len());
        for (p, _) in state.roads().iter() {
            assert_eq!(loaded.state.roads().component(p), state.roads().component(p));
            assert_eq!(loaded.state.road_mask(p), state.road_mask(p));
        }
        let again = loaded.encode().unwrap();
        assert_eq!(again, bytes, "load → save must reproduce identical bytes");
    }

    #[test]
    fn loaded_game_continues_identically() {
        let defs = defs();
        let (header, mut original) = sample_save();
        let bytes = encode(&header, &original).unwrap();
        let mut loaded = decode(&bytes, defs).unwrap().state;
        for _ in 0..500 {
            original.tick(defs, Vec::new());
            loaded.tick(defs, Vec::new());
        }
        let a = bincode::serde::encode_to_vec(&original, config()).unwrap();
        let b = bincode::serde::encode_to_vec(&loaded, config()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn header_summary_and_peek() {
        let (header, state) = sample_save();
        assert_eq!(header.summary.tick, 5200);
        assert_eq!(header.summary.date, state.clock().display());
        assert_eq!(header.seed, 99);
        let bytes = encode(&header, &state).unwrap();
        assert_eq!(peek_header(&bytes).unwrap(), header);
    }

    #[test]
    fn every_corrupted_byte_is_detected() {
        let defs = defs();
        let (header, state) = sample_save();
        let bytes = encode(&header, &state).unwrap();
        // Flip one bit at a spread of positions across prefix, header and body.
        let step = (bytes.len() / 97).max(1);
        for pos in (0..bytes.len()).step_by(step).chain([12, 16, 20, 27, bytes.len() - 1]) {
            let mut damaged = bytes.clone();
            damaged[pos] ^= 0x10;
            assert!(decode(&damaged, defs).is_err(), "corruption at byte {pos} not detected");
        }
        let mut damaged = bytes.clone();
        let mid = PREFIX_LEN + header_len(&bytes) + 100;
        damaged[mid] = damaged[mid].wrapping_add(1);
        assert!(matches!(decode(&damaged, defs), Err(SaveError::ChecksumMismatch { .. })));
    }

    fn header_len(bytes: &[u8]) -> usize {
        read_u32(bytes, 16) as usize
    }

    #[test]
    fn truncation_and_extension_are_detected() {
        let defs = defs();
        let (header, state) = sample_save();
        let bytes = encode(&header, &state).unwrap();
        assert!(matches!(decode(&bytes[..10], defs), Err(SaveError::Truncated { .. })));
        assert!(matches!(decode(&bytes[..bytes.len() - 1], defs), Err(SaveError::LengthMismatch { .. })));
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(matches!(decode(&longer, defs), Err(SaveError::LengthMismatch { .. })));
    }

    /// A save whose prefix is rewritten to `header_len`/`body_len`, with a matching checksum
    /// (CRC32 only detects accidental damage; anyone can recompute it).
    fn with_lengths(mut bytes: Vec<u8>, header_len: u32, body_len: u64) -> Vec<u8> {
        bytes[16..20].copy_from_slice(&header_len.to_le_bytes());
        bytes[20..28].copy_from_slice(&body_len.to_le_bytes());
        let crc = crc32fast::hash(&bytes[16..]);
        bytes[12..16].copy_from_slice(&crc.to_le_bytes());
        bytes
    }

    #[test]
    fn crafted_lengths_are_errors_not_panics() {
        let defs = defs();
        let (header, state) = sample_save();
        let bytes = encode(&header, &state).unwrap();
        // The sum of the declared lengths wraps around to the real file length.
        let small = bytes[..100].to_vec();
        let wrapping = 100u64.wrapping_sub(PREFIX_LEN as u64).wrapping_sub(1000);
        let crafted = with_lengths(small.clone(), 1000, wrapping);
        assert!(matches!(decode(&crafted, defs), Err(SaveError::LengthMismatch { .. })));
        assert!(peek_header(&crafted).is_err());
        let crafted = with_lengths(small, u32::MAX, u64::MAX);
        assert!(matches!(decode(&crafted, defs), Err(SaveError::LengthMismatch { .. })));
        assert!(peek_header(&crafted).is_err());
        // A header length beyond the data.
        let len = bytes.len() as u64;
        let crafted = with_lengths(bytes, 5_000_000, len);
        assert!(decode(&crafted, defs).is_err());
        assert!(matches!(peek_header(&crafted), Err(SaveError::Truncated { .. })));
    }

    #[test]
    fn inconsistent_world_tiles_are_rejected() {
        let defs = defs();
        let (header, mut state) = sample_save();
        let quarter = state.world.tiles().len() / 4;
        state.world.truncate_tiles_for_test(quarter);
        let bytes = encode(&header, &state).unwrap();
        assert!(matches!(decode(&bytes, defs), Err(SaveError::Decode(_))));
    }

    #[test]
    fn wrong_magic_and_versions_are_rejected() {
        let defs = defs();
        let (header, state) = sample_save();
        let bytes = encode(&header, &state).unwrap();
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(matches!(decode(&bad, defs), Err(SaveError::BadMagic)));
        assert!(matches!(peek_header(&bad), Err(SaveError::BadMagic)));

        let mut newer = bytes.clone();
        newer[8..12].copy_from_slice(&(SAVE_FORMAT_VERSION + 1).to_le_bytes());
        assert!(
            matches!(decode(&newer, defs), Err(SaveError::UnsupportedVersion { found, .. }) if found == SAVE_FORMAT_VERSION + 1)
        );
        let mut older = bytes;
        older[8..12].copy_from_slice(&(MIN_SUPPORTED_FORMAT_VERSION - 1).to_le_bytes());
        assert!(matches!(decode(&older, defs), Err(SaveError::UnsupportedVersion { .. })));
    }

    #[test]
    fn different_game_data_is_rejected() {
        let defs = defs();
        let (mut header, state) = sample_save();
        header.defs_fingerprint ^= 1;
        let bytes = encode(&header, &state).unwrap();
        assert!(matches!(decode(&bytes, defs), Err(SaveError::DataMismatch { .. })));
    }

    /// A replacement that must occur exactly once in `text`.
    fn replace_once(text: &str, from: &str, to: &str) -> String {
        assert_eq!(text.matches(from).count(), 1, "{from}");
        text.replacen(from, to, 1)
    }

    #[test]
    fn rebalanced_data_is_reapplied_on_load() {
        let d = defs();
        let mut state = GameState::new(d, GameSettings::new(d, 99, WorldSize::Small));
        let sawmill = d.building_id("sawmill").unwrap();
        let (origin, rotation) = find_spot(&state, sawmill);
        let mill = crate::buildings::spawn_finished(&mut state, d, sawmill, origin, rotation).unwrap();
        for _ in 0..50 {
            state.tick(d, Vec::new());
        }
        let header = SaveHeader::for_state(&state, d, "Balancing", 0);
        let bytes = encode(&header, &state).unwrap();

        // Same ids and shape: the sawmill now saws stone, the town hall stores 300 per resource.
        let rebalanced = crate::test_support::defs_edited("buildings/buildings.ron", |text| {
            let text = replace_once(
                text,
                r#"name: "Bretter sägen", inputs: {"wood": 2.0}, outputs: {"planks": 3.0}, hours: 2.0"#,
                r#"name: "Bretter sägen", inputs: {"stone": 2.0}, outputs: {"planks": 3.0}, hours: 2.0"#,
            );
            replace_once(&text, "capacity_per_resource: 200, accepts: All", "capacity_per_resource: 300, accepts: All")
        });
        assert_eq!(rebalanced.fingerprint(), d.fingerprint(), "balancing does not change the fingerprint");
        let loaded = decode(&bytes, &rebalanced).unwrap().state;
        let (wood, stone) = (d.resource_id("wood").unwrap(), d.resource_id("stone").unwrap());
        let input = loaded.building(mill).unwrap().production().unwrap().input();
        assert_eq!(input.capacity(stone), 8, "the new input gets a buffer");
        assert_eq!(input.capacity(wood), 0, "the old input becomes surplus");
        let hall = crate::economy::storage::main_headquarters(&loaded, &rebalanced).unwrap();
        assert_eq!(loaded.building(hall).unwrap().inventory().capacity(wood), 300);
        assert!(loaded.economy().capacity().get(wood) >= 300, "stock view refreshed");
        loaded.validate(&rebalanced).unwrap();
        // With unchanged data the load is exact (see roundtrip_is_byte_identical as well).
        let same = decode(&bytes, d).unwrap().state;
        assert_eq!(bincode::serde::encode_to_vec(&same, config()).unwrap(), bincode::serde::encode_to_vec(&state, config()).unwrap());

        // A changed footprint changes the shape: the save is refused as made with other data
        // (its upgrade grows along, an upgrade never shrinks a building).
        let resized = crate::test_support::defs_edited("buildings/buildings.ron", |text| {
            let text = replace_once(text, r#"id: "sawmill", name: "Sägewerk", category: Production, size: (2, 3)"#, r#"id: "sawmill", name: "Sägewerk", category: Production, size: (3, 3)"#);
            replace_once(&text, r#"id: "large_sawmill", name: "Großes Sägewerk", category: Production, size: (2, 3)"#, r#"id: "large_sawmill", name: "Großes Sägewerk", category: Production, size: (3, 3)"#)
        });
        assert_ne!(resized.fingerprint(), d.fingerprint());
        assert!(matches!(decode(&bytes, &resized), Err(SaveError::DataMismatch { .. })));
    }

    #[test]
    fn header_body_disagreement_is_rejected() {
        let defs = defs();
        let (mut header, state) = sample_save();
        header.seed += 1;
        let bytes = encode(&header, &state).unwrap();
        assert!(matches!(decode(&bytes, defs), Err(SaveError::HeaderMismatch(_))));
    }

    /// The format before the current one, for the migration test: a header without
    /// `defs_fingerprint`.
    const OLD: u32 = SAVE_FORMAT_VERSION - 1;

    #[derive(Serialize, Deserialize)]
    struct HeaderV0 {
        magic: [u8; 8],
        format_version: u32,
        game_version: String,
        saved_at_unix: u64,
        seed: u64,
        name: String,
        summary: SaveSummary,
    }

    fn migrate_v0_to_v1(p: Payload) -> Result<Payload, SaveError> {
        let old: HeaderV0 = decode_exact(&p.header, "old header")
            .map_err(|e| SaveError::Migration { from: OLD, message: e.to_string() })?;
        let new = SaveHeader {
            magic: old.magic,
            format_version: SAVE_FORMAT_VERSION,
            game_version: old.game_version,
            saved_at_unix: old.saved_at_unix,
            seed: old.seed,
            name: old.name,
            summary: old.summary,
            defs_fingerprint: 0,
            game_id: uuid::Uuid::nil(),
        };
        Ok(Payload { header: bincode::serde::encode_to_vec(&new, config())?, body: p.body })
    }

    #[test]
    fn migration_chain_upgrades_old_payloads() {
        let (header, state) = sample_save();
        let v0 = HeaderV0 {
            magic: SAVE_MAGIC,
            format_version: OLD,
            game_version: "0.0.1".into(),
            saved_at_unix: 5,
            seed: header.seed,
            name: "Alt".into(),
            summary: header.summary.clone(),
        };
        let payload = Payload {
            header: bincode::serde::encode_to_vec(&v0, config()).unwrap(),
            body: bincode::serde::encode_to_vec(&state, config()).unwrap(),
        };
        let chain = [Migration { from: OLD, apply: migrate_v0_to_v1 }];
        let migrated = migrate(OLD, payload.clone(), &chain).unwrap();
        let h: SaveHeader = decode_exact(&migrated.header, "migrated").unwrap();
        assert_eq!(h.name, "Alt");
        assert_eq!(h.format_version, SAVE_FORMAT_VERSION);
        assert_eq!(migrated.body, payload.body);
        // A gap in the chain is reported.
        assert!(matches!(migrate(OLD, payload.clone(), &[]), Err(SaveError::MissingMigration { from: OLD })));
        // Current payloads pass through untouched.
        assert_eq!(migrate(SAVE_FORMAT_VERSION, payload.clone(), &[]).unwrap(), payload);
    }
}
