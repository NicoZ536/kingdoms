//! Territory: the tiles the player's kingdom owns.
//!
//! Every *finished* building with a `territory_radius` (town hall 20, castle 26, watchtower 8)
//! claims the disc of tiles whose centres lie within that radius of the footprint centre,
//! except tiles an AI kingdom owns ([`LandMap`], docs/game_design.md §10). Buildings and roads
//! may only be placed on owned tiles. The territory is recomputed whenever a claiming building
//! is completed, moved or removed.

use glam::IVec2;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::buildings::{BuildingId, Buildings};
use crate::defs::Defs;
use crate::diplomacy::LandMap;
use crate::grid::TileRect;
use crate::world::MAX_MAP_EDGE;

/// Per-tile ownership as a packed bitmap (row-major).
///
/// Deserialization rejects sizes beyond [`MAX_MAP_EDGE`] and malformed bitmaps
/// ([`TerritoryError`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TerritoryRepr")]
pub struct Territory {
    width: i32,
    height: i32,
    bits: Vec<u64>,
    owned: u32,
}

/// A malformed territory bitmap in a damaged or crafted save.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum TerritoryError {
    /// The stored size is not positive or exceeds [`MAX_MAP_EDGE`].
    #[error("territory size {width}x{height} is outside 1..={MAX_MAP_EDGE}")]
    InvalidSize {
        /// Stored width.
        width: i32,
        /// Stored height.
        height: i32,
    },
    /// Bitmap length, padding bits or owned-tile count do not match.
    #[error("malformed territory bitmap")]
    Malformed,
}

/// Serialized form of [`Territory`] (same field order).
#[derive(Deserialize)]
struct TerritoryRepr {
    width: i32,
    height: i32,
    bits: Vec<u64>,
    owned: u32,
}

impl TryFrom<TerritoryRepr> for Territory {
    type Error = TerritoryError;

    fn try_from(repr: TerritoryRepr) -> Result<Self, Self::Error> {
        if !(1..=MAX_MAP_EDGE).contains(&repr.width) || !(1..=MAX_MAP_EDGE).contains(&repr.height) {
            return Err(TerritoryError::InvalidSize { width: repr.width, height: repr.height });
        }
        let t = Territory { width: repr.width, height: repr.height, bits: repr.bits, owned: repr.owned };
        if t.is_consistent() { Ok(t) } else { Err(TerritoryError::Malformed) }
    }
}

impl Territory {
    /// Nothing owned on a `width` × `height` map.
    pub fn new(width: i32, height: i32) -> Self {
        let len = (width.max(0) as usize) * (height.max(0) as usize);
        Self { width, height, bits: vec![0; len.div_ceil(64)], owned: 0 }
    }

    /// Map size the bitmap covers.
    pub fn grid_size(&self) -> IVec2 {
        IVec2::new(self.width, self.height)
    }

    fn index(&self, p: IVec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height).then(|| (p.y * self.width + p.x) as usize)
    }

    /// True if the kingdom owns tile `p`.
    pub fn contains(&self, p: IVec2) -> bool {
        self.index(p).is_some_and(|i| self.bits[i / 64] & (1 << (i % 64)) != 0)
    }

    /// True if every tile of `rect` is owned.
    pub fn contains_rect(&self, rect: TileRect) -> bool {
        rect.tiles().all(|p| self.contains(p))
    }

    /// Number of owned tiles.
    pub fn owned_tiles(&self) -> u32 {
        self.owned
    }

    /// Owned tiles, row-major.
    pub fn owned_positions(&self) -> impl Iterator<Item = IVec2> + '_ {
        let w = self.width.max(1);
        let len = (self.width.max(0) as usize) * (self.height.max(0) as usize);
        (0..len).filter(move |i| self.bits[i / 64] & (1 << (i % 64)) != 0).map(move |i| {
            let i = i as i32;
            IVec2::new(i % w, i / w)
        })
    }

    fn unset(&mut self, p: IVec2) {
        if let Some(i) = self.index(p) {
            let mask = 1u64 << (i % 64);
            if self.bits[i / 64] & mask != 0 {
                self.bits[i / 64] &= !mask;
                self.owned -= 1;
            }
        }
    }

    fn set(&mut self, p: IVec2) {
        if let Some(i) = self.index(p) {
            let mask = 1u64 << (i % 64);
            if self.bits[i / 64] & mask == 0 {
                self.bits[i / 64] |= mask;
                self.owned += 1;
            }
        }
    }

    /// Claims all tiles whose centre lies within `radius` of the centre of `rect`.
    pub(crate) fn claim(&mut self, rect: TileRect, radius: u16) {
        let r = i32::from(radius);
        let c2 = rect.center_x2();
        let r2 = (2 * r) * (2 * r);
        let area = rect.expanded(r + 1).clipped(self.width, self.height);
        for p in area.tiles() {
            let d = p * 2 + IVec2::ONE - c2;
            if d.x * d.x + d.y * d.y <= r2 {
                self.set(p);
            }
        }
    }

    /// Territory claimed by all finished buildings except `exclude`, without the tiles AI
    /// kingdoms own (`land`).
    pub fn compute_without_foreign(buildings: &Buildings, defs: &Defs, exclude: Option<BuildingId>, land: &LandMap) -> Territory {
        let mut t = Territory::compute(buildings, defs, exclude);
        t.remove_foreign(land);
        t
    }

    /// Gives up the tiles AI kingdoms own (`land`).
    pub(crate) fn remove_foreign(&mut self, land: &LandMap) {
        let foreign: Vec<IVec2> = self.owned_positions().filter(|p| land.is_foreign(*p)).collect();
        for p in foreign {
            self.unset(p);
        }
    }

    /// Territory claimed by all finished buildings except `exclude`, ignoring other kingdoms
    /// (see [`Territory::compute_without_foreign`] for the territory the player really owns).
    pub fn compute(buildings: &Buildings, defs: &Defs, exclude: Option<BuildingId>) -> Territory {
        let size = buildings.grid_size();
        let mut t = Territory::new(size.x, size.y);
        for (id, b) in buildings.iter() {
            if Some(id) == exclude || !b.is_complete() {
                continue;
            }
            let radius = defs.buildings().get(b.kind().index()).map_or(0, |d| d.territory_radius);
            if radius > 0 {
                t.claim(b.rect(), radius);
            }
        }
        t
    }

    /// Recomputes from the buildings (without the tiles of AI kingdoms); returns true if
    /// ownership changed.
    pub(crate) fn recompute(&mut self, buildings: &Buildings, defs: &Defs, land: &LandMap) -> bool {
        let fresh = Territory::compute_without_foreign(buildings, defs, None, land);
        let changed = fresh != *self;
        *self = fresh;
        changed
    }

    /// True if the bitmap is well-formed (length matches the size, count matches the bits).
    pub(crate) fn is_consistent(&self) -> bool {
        let len = (self.width.max(0) as usize) * (self.height.max(0) as usize);
        self.bits.len() == len.div_ceil(64)
            && self.bits.iter().map(|w| w.count_ones()).sum::<u32>() == self.owned
            && self.bits.last().is_none_or(|w| len % 64 == 0 || w >> (len % 64) == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_is_a_disc_around_the_footprint_centre() {
        let mut t = Territory::new(64, 64);
        let rect = TileRect::new(IVec2::new(30, 30), IVec2::new(4, 4));
        t.claim(rect, 20);
        // Centre at (32, 32): tiles up to 20 away along an axis are owned, 21 are not.
        assert!(t.contains(IVec2::new(32 + 19, 32)));
        assert!(t.contains(IVec2::new(32 - 20, 32)));
        assert!(!t.contains(IVec2::new(32 + 21, 32)));
        // Diagonal corners of the bounding square are outside the disc.
        assert!(!t.contains(IVec2::new(32 + 19, 32 + 19)));
        assert!(t.contains_rect(rect));
        let owned = t.owned_tiles();
        // Roughly π r² tiles.
        assert!((1200..=1330).contains(&owned), "{owned}");
        assert!(t.is_consistent());
        // Claiming again changes nothing.
        t.claim(rect, 20);
        assert_eq!(t.owned_tiles(), owned);
        assert!(!t.contains(IVec2::new(-1, 0)));
    }

    #[test]
    fn claims_are_clipped_to_the_map() {
        let mut t = Territory::new(10, 10);
        t.claim(TileRect::tile(IVec2::new(0, 0)), 8);
        assert!(t.contains(IVec2::new(0, 0)) && t.contains(IVec2::new(7, 0)));
        assert!(t.owned_tiles() < 100);
        assert!(t.is_consistent());
    }

    #[test]
    fn deserialization_rejects_malformed_bitmaps() {
        let config = bincode::config::standard();
        let decode = |t: &Territory| {
            let bytes = bincode::serde::encode_to_vec(t, config).unwrap();
            bincode::serde::decode_from_slice::<Territory, _>(&bytes, config).map(|(t, _)| t)
        };
        let mut t = Territory::new(10, 10);
        t.claim(TileRect::tile(IVec2::new(3, 3)), 2);
        assert_eq!(decode(&t).unwrap(), t);
        let mut wrong_count = t.clone();
        wrong_count.owned += 1;
        assert!(decode(&wrong_count).is_err());
        let mut short = t.clone();
        short.bits.pop();
        assert!(decode(&short).is_err());
        let mut huge = t;
        huge.width = i32::MAX;
        assert!(decode(&huge).is_err());
    }
}
