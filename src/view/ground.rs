//! Which dirty chunks really changed their ground.
//!
//! The simulation marks a chunk dirty for any visible tile change, most often a tree that grew
//! a stage or was felled. Only the vegetation layer (and the minimap) depend on that; the
//! terrain, road, field and border layers read nothing but the terrain type and elevation of
//! the chunk's tiles and of the tiles around it (transition fringes, cliffs, rims, bridge and
//! field heights). [`GroundSignatures`] remembers exactly that data per chunk, so those layers
//! are rebuilt only when it changed – on a large forested map trees grow somewhere almost every
//! tick, which used to rebuild four static layers of several chunks per frame.

use glam::IVec2;
use kingdoms_sim::world::{CHUNK_SIZE, WorldMap};

/// Terrain type and elevation of every chunk (with a one-tile ring) as last seen.
#[derive(Debug, Default)]
pub struct GroundSignatures {
    chunk_count: IVec2,
    chunks: Vec<Option<Vec<u8>>>,
    scratch: Vec<u8>,
}

impl GroundSignatures {
    /// Of the `dirty` chunks, those whose ground differs from the last time they were seen
    /// (a chunk never seen counts as changed); remembers the current ground of all of them.
    pub fn changed(&mut self, map: &WorldMap, dirty: &[IVec2]) -> Vec<IVec2> {
        let count = map.chunk_count();
        if count != self.chunk_count {
            self.chunk_count = count;
            self.chunks = vec![None; (count.x.max(0) * count.y.max(0)) as usize];
        }
        let mut changed = Vec::new();
        for &chunk in dirty {
            if chunk.x < 0 || chunk.y < 0 || chunk.x >= count.x || chunk.y >= count.y {
                // Not a chunk of this map: let the layers decide as before.
                changed.push(chunk);
                continue;
            }
            signature(map, chunk, &mut self.scratch);
            let slot = &mut self.chunks[(chunk.y * count.x + chunk.x) as usize];
            if slot.as_deref() != Some(self.scratch.as_slice()) {
                changed.push(chunk);
                match slot {
                    Some(old) => old.clone_from(&self.scratch),
                    None => *slot = Some(self.scratch.clone()),
                }
            }
        }
        changed
    }
}

/// Terrain and elevation of the tiles of `chunk` and the ring around it, row by row (outside
/// the map: a marker), into `out`.
fn signature(map: &WorldMap, chunk: IVec2, out: &mut Vec<u8>) {
    out.clear();
    let origin = chunk * CHUNK_SIZE;
    for y in origin.y - 1..origin.y + CHUNK_SIZE + 1 {
        for x in origin.x - 1..origin.x + CHUNK_SIZE + 1 {
            match map.get(IVec2::new(x, y)) {
                Some(t) => out.extend([t.terrain as u8, t.elevation]),
                None => out.extend([u8::MAX, u8::MAX]),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use kingdoms_sim::world::{Biome, Terrain, Tile, Vegetation};

    use super::*;

    fn map() -> WorldMap {
        let tile = Tile {
            terrain: Terrain::Grass,
            biome: Biome::Grassland,
            elevation: 2,
            fertility: 0,
            moisture: 0,
            temperature: 0,
            vegetation: Vegetation::None,
            deposit: None,
            river: false,
        };
        WorldMap::new_filled(80, 70, 1, tile)
    }

    #[test]
    fn only_terrain_and_elevation_changes_count() {
        let mut map = map();
        let mut seen = GroundSignatures::default();
        let all: Vec<IVec2> = (0..3).flat_map(|y| (0..3).map(move |x| IVec2::new(x, y))).collect();
        assert_eq!(seen.changed(&map, &all), all, "the first sight of a chunk counts as a change");
        assert!(seen.changed(&map, &all).is_empty());
        // A tree is not ground.
        map.tile_mut(IVec2::new(40, 40)).vegetation = Vegetation::Stump { hours: 3 };
        assert!(seen.changed(&map, &[IVec2::new(1, 1)]).is_empty());
        // Elevation and terrain are.
        map.tile_mut(IVec2::new(40, 40)).elevation += 1;
        assert_eq!(seen.changed(&map, &[IVec2::new(1, 1)]), vec![IVec2::new(1, 1)]);
        map.tile_mut(IVec2::new(5, 5)).terrain = Terrain::Sand;
        assert_eq!(seen.changed(&map, &[IVec2::new(0, 0), IVec2::new(1, 1)]), vec![IVec2::new(0, 0)]);
        // A change on the ring of a neighbour chunk (its fringes and cliffs) counts for it too.
        let edge = IVec2::new(CHUNK_SIZE, 10);
        map.tile_mut(edge).elevation += 2;
        assert_eq!(seen.changed(&map, &[IVec2::new(0, 0), IVec2::new(1, 0)]), vec![IVec2::new(0, 0), IVec2::new(1, 0)]);
        // Chunks outside the map are passed on.
        assert_eq!(seen.changed(&map, &[IVec2::new(9, 9)]), vec![IVec2::new(9, 9)]);
    }
}
