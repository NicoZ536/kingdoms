//! Ownership of the land by AI kingdoms and the tiles their structures occupy.

use glam::IVec2;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::KingdomId;
use crate::grid::{ChangeTracker, TileRect};
use crate::world::MAX_MAP_EDGE;

/// Owner bits of a cell (0 = nobody, otherwise kingdom index + 1).
const OWNER_MASK: u8 = 0x07;
/// Cell flag: a structure of an AI settlement stands here (walkers cannot enter).
const STRUCTURE: u8 = 0x80;

/// Per-tile owner (an AI kingdom or nobody) and structure flag, row-major.
///
/// Deserialization rejects sizes beyond [`MAX_MAP_EDGE`] and a cell count that does not match
/// ([`LandMapError`]); the cross-check against the kingdoms' structures happens in
/// `Diplomacy::validate` (via [`crate::state::GameState::validate`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "LandMapRepr")]
pub struct LandMap {
    width: i32,
    height: i32,
    cells: Vec<u8>,
    /// Structure and border changes for the navigation grid and renderers (not saved).
    #[serde(skip)]
    changes: ChangeTracker,
}

impl PartialEq for LandMap {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height && self.cells == other.cells
    }
}

/// A malformed land map in a damaged or crafted save.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum LandMapError {
    /// The stored size is outside 0..=[`MAX_MAP_EDGE`] or does not match the cells.
    #[error("land map size {width}x{height} does not match {cells} cells")]
    InvalidSize {
        /// Stored width.
        width: i32,
        /// Stored height.
        height: i32,
        /// Stored cells.
        cells: usize,
    },
}

#[derive(Deserialize)]
struct LandMapRepr {
    width: i32,
    height: i32,
    cells: Vec<u8>,
}

impl TryFrom<LandMapRepr> for LandMap {
    type Error = LandMapError;

    fn try_from(r: LandMapRepr) -> Result<Self, Self::Error> {
        let ok_edge = |e: i32| (0..=MAX_MAP_EDGE).contains(&e);
        let expected = (r.width.max(0) as usize) * (r.height.max(0) as usize);
        if !ok_edge(r.width) || !ok_edge(r.height) || r.cells.len() != expected {
            return Err(LandMapError::InvalidSize { width: r.width, height: r.height, cells: r.cells.len() });
        }
        Ok(LandMap { width: r.width, height: r.height, cells: r.cells, changes: ChangeTracker::default() })
    }
}

impl Default for LandMap {
    fn default() -> Self {
        LandMap::new(0, 0)
    }
}

impl LandMap {
    /// Nobody owns anything on a `width` × `height` map.
    pub fn new(width: i32, height: i32) -> Self {
        let len = (width.max(0) as usize) * (height.max(0) as usize);
        Self { width, height, cells: vec![0; len], changes: ChangeTracker::default() }
    }

    /// Map size covered.
    pub fn grid_size(&self) -> IVec2 {
        IVec2::new(self.width, self.height)
    }

    fn index(&self, p: IVec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height).then(|| (p.y * self.width + p.x) as usize)
    }

    /// The AI kingdom owning tile `p`.
    pub fn owner(&self, p: IVec2) -> Option<KingdomId> {
        let cell = self.cells[self.index(p)?];
        let owner = cell & OWNER_MASK;
        (owner != 0).then(|| KingdomId::from_raw(owner - 1))
    }

    /// True if an AI kingdom owns tile `p`.
    pub fn is_foreign(&self, p: IVec2) -> bool {
        self.owner(p).is_some()
    }

    /// True if a structure of an AI settlement stands on `p` (walkers cannot enter it).
    pub fn has_structure(&self, p: IVec2) -> bool {
        self.index(p).is_some_and(|i| self.cells[i] & STRUCTURE != 0)
    }

    /// Tiles owned by `kingdom`.
    pub fn count_owned(&self, kingdom: KingdomId) -> u32 {
        let code = kingdom.raw() + 1;
        u32::try_from(self.cells.iter().filter(|c| *c & OWNER_MASK == code).count()).unwrap_or(u32::MAX)
    }

    pub(crate) fn set_owner(&mut self, p: IVec2, owner: Option<KingdomId>) {
        let Some(i) = self.index(p) else { return };
        let code = owner.map_or(0, |k| k.raw() + 1);
        let cell = &mut self.cells[i];
        if *cell & OWNER_MASK != code {
            *cell = (*cell & !OWNER_MASK) | code;
            self.changes.touch(TileRect::tile(p));
        }
    }

    pub(crate) fn set_structure(&mut self, rect: TileRect, present: bool) {
        for p in rect.tiles() {
            if let Some(i) = self.index(p) {
                if present {
                    self.cells[i] |= STRUCTURE;
                } else {
                    self.cells[i] &= !STRUCTURE;
                }
            }
        }
        self.changes.touch(rect);
    }

    /// Tiles carrying the structure flag, row-major.
    pub(crate) fn structure_flags(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.cells.iter().enumerate().filter(|(_, c)| **c & STRUCTURE != 0).map(|(i, _)| {
            let i = i as i32;
            IVec2::new(i % self.width, i / self.width)
        })
    }

    /// Change tracker (navigation rectangles, renderer chunks).
    pub(crate) fn changes_mut(&mut self) -> &mut ChangeTracker {
        &mut self.changes
    }

    /// Counter of ownership and structure changes (wrapping; not saved).
    pub fn revision(&self) -> u64 {
        self.changes.revision()
    }

    /// The highest owner code stored (validation against the number of kingdoms).
    pub(crate) fn max_owner_code(&self) -> u8 {
        self.cells.iter().map(|c| c & OWNER_MASK).max().unwrap_or(0)
    }

    /// True if any stored bit besides owner and structure is set (malformed save).
    pub(crate) fn has_unknown_bits(&self) -> bool {
        self.cells.iter().any(|c| c & !(OWNER_MASK | STRUCTURE) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_and_structures_are_stored_per_tile() {
        let mut m = LandMap::new(8, 6);
        let k = KingdomId::from_raw(2);
        assert_eq!(m.owner(IVec2::new(3, 3)), None);
        m.set_owner(IVec2::new(3, 3), Some(k));
        m.set_structure(TileRect::new(IVec2::new(3, 3), IVec2::new(2, 1)), true);
        assert_eq!(m.owner(IVec2::new(3, 3)), Some(k));
        assert!(m.has_structure(IVec2::new(4, 3)) && !m.has_structure(IVec2::new(5, 3)));
        assert_eq!(m.count_owned(k), 1);
        assert_eq!(m.structure_flags().count(), 2);
        assert!(m.owner(IVec2::new(-1, 0)).is_none() && !m.has_structure(IVec2::new(9, 9)));
        m.set_structure(TileRect::new(IVec2::new(3, 3), IVec2::new(2, 1)), false);
        assert_eq!(m.owner(IVec2::new(3, 3)), Some(k), "removing a structure keeps the owner");
        assert!(m.revision() > 0);
        let config = bincode::config::standard();
        let bytes = bincode::serde::encode_to_vec(&m, config).unwrap();
        let (back, _): (LandMap, _) = bincode::serde::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(back, m);
        let mut broken = m;
        broken.cells.pop();
        let bytes = bincode::serde::encode_to_vec(&broken, config).unwrap();
        assert!(bincode::serde::decode_from_slice::<LandMap, _>(&bytes, config).is_err());
    }
}
