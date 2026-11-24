//! The tile world: terrain, elevation, biomes, vegetation and natural resource deposits.

pub mod generator;
pub mod noise;

use glam::IVec2;
use serde::{Deserialize, Serialize};

pub use generator::{GeneratorSettings, WorldSize, generate};

/// Tiles per chunk side (used for rendering and spatial partitioning).
pub const CHUNK_SIZE: i32 = 32;
/// Highest terrain elevation level.
pub const MAX_ELEVATION: u8 = 8;
/// Longest map edge in tiles (the largest [`WorldSize`]). Grids read from save games are
/// rejected beyond it before anything is allocated.
pub const MAX_MAP_EDGE: i32 = 384;

/// Surface material of a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Terrain {
    DeepWater,
    ShallowWater,
    Sand,
    Swamp,
    Steppe,
    Dirt,
    Grass,
    Meadow,
    Forest,
    Rock,
    Snow,
}

impl Terrain {
    pub const ALL: [Terrain; 11] = [
        Terrain::DeepWater,
        Terrain::ShallowWater,
        Terrain::Sand,
        Terrain::Swamp,
        Terrain::Steppe,
        Terrain::Dirt,
        Terrain::Grass,
        Terrain::Meadow,
        Terrain::Forest,
        Terrain::Rock,
        Terrain::Snow,
    ];

    /// Sprite name prefix in the terrain atlas.
    pub fn sprite_key(self) -> &'static str {
        match self {
            Terrain::DeepWater => "deep_water",
            Terrain::ShallowWater => "shallow_water",
            Terrain::Sand => "sand",
            Terrain::Swamp => "swamp",
            Terrain::Steppe => "steppe",
            Terrain::Dirt => "dirt",
            Terrain::Grass => "grass",
            Terrain::Meadow => "meadow",
            Terrain::Forest => "forest",
            Terrain::Rock => "rock",
            Terrain::Snow => "snow",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Terrain::DeepWater => "Tiefes Wasser",
            Terrain::ShallowWater => "Flaches Wasser",
            Terrain::Sand => "Sand",
            Terrain::Swamp => "Sumpf",
            Terrain::Steppe => "Steppe",
            Terrain::Dirt => "Erde",
            Terrain::Grass => "Grasland",
            Terrain::Meadow => "Fruchtbare Wiese",
            Terrain::Forest => "Waldboden",
            Terrain::Rock => "Fels",
            Terrain::Snow => "Schnee",
        }
    }

    pub fn is_water(self) -> bool {
        matches!(self, Terrain::DeepWater | Terrain::ShallowWater)
    }

    /// Order used for blending neighbouring terrains: higher values draw their fringe onto lower.
    pub fn transition_priority(self) -> u8 {
        self as u8
    }

    /// Can buildings be placed on this terrain at all (further checks apply)?
    pub fn is_buildable(self) -> bool {
        !matches!(self, Terrain::DeepWater | Terrain::ShallowWater | Terrain::Swamp)
    }

    /// Movement cost multiplier for walkers off-road (1.0 = normal).
    pub fn walk_cost(self) -> f32 {
        match self {
            Terrain::DeepWater => f32::INFINITY,
            Terrain::ShallowWater => 6.0,
            Terrain::Swamp => 3.0,
            Terrain::Snow => 2.0,
            Terrain::Rock => 1.8,
            Terrain::Forest => 1.4,
            Terrain::Sand => 1.3,
            _ => 1.0,
        }
    }
}

/// Climate region a tile belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Biome {
    Ocean,
    Lake,
    TemperateForest,
    Grassland,
    FertilePlains,
    Mountains,
    Swamp,
    Coast,
    Arid,
    Snow,
}

impl Biome {
    pub fn display_name(self) -> &'static str {
        match self {
            Biome::Ocean => "Meer",
            Biome::Lake => "See",
            Biome::TemperateForest => "Gemäßigter Wald",
            Biome::Grassland => "Grasland",
            Biome::FertilePlains => "Fruchtbare Ebene",
            Biome::Mountains => "Gebirge",
            Biome::Swamp => "Sumpfgebiet",
            Biome::Coast => "Küste",
            Biome::Arid => "Trockengebiet",
            Biome::Snow => "Schneelandschaft",
        }
    }
}

/// Natural growth on a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Vegetation {
    None,
    /// Harvestable tree; `growth` 0..=255 (255 = mature).
    Tree { kind: TreeKind, growth: u8 },
    Bush,
    /// Natural obstacle that must be cleared before building.
    Boulder,
    /// Stump of a felled tree; rots away after `GameRules::stump_decay_hours` (`hours` since
    /// felling). Foresters plant on it, fields and buildings clear it.
    Stump { hours: u8 },
}

impl Vegetation {
    pub fn is_tree(self) -> bool {
        matches!(self, Vegetation::Tree { .. })
    }

    /// True for the stump of a felled tree.
    pub fn is_stump(self) -> bool {
        matches!(self, Vegetation::Stump { .. })
    }

    /// Blocks construction until removed.
    pub fn blocks_building(self) -> bool {
        !matches!(self, Vegetation::None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TreeKind {
    Deciduous,
    Conifer,
    Birch,
    Willow,
}

impl TreeKind {
    pub fn sprite_key(self) -> &'static str {
        match self {
            TreeKind::Deciduous => "oak",
            TreeKind::Conifer => "pine",
            TreeKind::Birch => "birch",
            TreeKind::Willow => "willow",
        }
    }
}

/// Mineable / gatherable natural resource found in the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DepositKind {
    Stone,
    Iron,
    Coal,
    Clay,
    Sand,
    Gold,
    Fish,
}

impl DepositKind {
    pub const ALL: [DepositKind; 7] = [
        DepositKind::Stone,
        DepositKind::Iron,
        DepositKind::Coal,
        DepositKind::Clay,
        DepositKind::Sand,
        DepositKind::Gold,
        DepositKind::Fish,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            DepositKind::Stone => "Steinvorkommen",
            DepositKind::Iron => "Eisenerz",
            DepositKind::Coal => "Kohle",
            DepositKind::Clay => "Lehm & Ton",
            DepositKind::Sand => "Sand",
            DepositKind::Gold => "Gold",
            DepositKind::Fish => "Fischgründe",
        }
    }

    pub fn sprite_key(self) -> &'static str {
        match self {
            DepositKind::Stone => "deposit_stone",
            DepositKind::Iron => "deposit_iron",
            DepositKind::Coal => "deposit_coal",
            DepositKind::Clay => "deposit_clay",
            DepositKind::Sand => "deposit_sand",
            DepositKind::Gold => "deposit_gold",
            DepositKind::Fish => "deposit_fish",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Deposit {
    pub kind: DepositKind,
    /// Remaining units; depletes when mined.
    pub amount: u16,
}

/// One map cell.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: Terrain,
    pub biome: Biome,
    /// Discrete height level, `0..=MAX_ELEVATION`. Water surfaces sit at their level too.
    pub elevation: u8,
    /// Soil fertility 0..=255 (farm yields).
    pub fertility: u8,
    /// Ground moisture 0..=255.
    pub moisture: u8,
    /// Mean annual temperature, 0 (arctic) ..=255 (hot).
    pub temperature: u8,
    pub vegetation: Vegetation,
    pub deposit: Option<Deposit>,
    /// Part of a river (fresh water source).
    pub river: bool,
}

impl Tile {
    pub fn is_water(&self) -> bool {
        self.terrain.is_water()
    }

    pub fn has_fresh_water(&self) -> bool {
        self.river || self.biome == Biome::Lake
    }
}

/// A deserialized map that is internally inconsistent (damaged or crafted save game).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorldMapError {
    /// Width or height is not positive or exceeds [`MAX_MAP_EDGE`].
    #[error("map size {width}x{height} is outside 1..={MAX_MAP_EDGE}")]
    InvalidSize {
        /// Stored width.
        width: i32,
        /// Stored height.
        height: i32,
    },
    /// The tile list does not have `width × height` entries.
    #[error("map has {actual} tiles but its size requires {expected}")]
    TileCountMismatch {
        /// Tiles required by the size.
        expected: usize,
        /// Tiles stored.
        actual: usize,
    },
    /// A tile is higher than [`MAX_ELEVATION`].
    #[error("tile {index} has elevation {elevation} (maximum {MAX_ELEVATION})")]
    ElevationOutOfRange {
        /// Row-major tile index.
        index: usize,
        /// Stored elevation.
        elevation: u8,
    },
    /// The start position or a settlement site lies outside the map.
    #[error("position {0} is outside the map")]
    PositionOutOfBounds(IVec2),
}

/// The whole map.
///
/// Deserialization checks the size against [`MAX_MAP_EDGE`], the tile count against the size,
/// elevations and the stored positions ([`WorldMapError`]), so a loaded map can be indexed
/// without panics.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "WorldMapRepr")]
pub struct WorldMap {
    width: i32,
    height: i32,
    seed: u64,
    tiles: Vec<Tile>,
    /// Suggested starting position for the player (centre of a good settlement area).
    pub start: IVec2,
    /// Additional good settlement sites (used for rival kingdoms).
    pub settlement_sites: Vec<IVec2>,
    /// Incremented whenever tiles change; renderers use it to rebuild caches.
    #[serde(skip)]
    revision: u64,
    #[serde(skip)]
    dirty_chunks: Vec<IVec2>,
}

/// Serialized form of [`WorldMap`] (same field order as its non-skipped fields).
#[derive(Deserialize)]
struct WorldMapRepr {
    width: i32,
    height: i32,
    seed: u64,
    tiles: Vec<Tile>,
    start: IVec2,
    settlement_sites: Vec<IVec2>,
}

impl TryFrom<WorldMapRepr> for WorldMap {
    type Error = WorldMapError;

    fn try_from(repr: WorldMapRepr) -> Result<Self, Self::Error> {
        let (width, height) = (repr.width, repr.height);
        if !(1..=MAX_MAP_EDGE).contains(&width) || !(1..=MAX_MAP_EDGE).contains(&height) {
            return Err(WorldMapError::InvalidSize { width, height });
        }
        // Both edges are at most MAX_MAP_EDGE, so the product cannot overflow.
        let expected = width as usize * height as usize;
        if repr.tiles.len() != expected {
            return Err(WorldMapError::TileCountMismatch { expected, actual: repr.tiles.len() });
        }
        if let Some((index, tile)) = repr.tiles.iter().enumerate().find(|(_, t)| t.elevation > MAX_ELEVATION) {
            return Err(WorldMapError::ElevationOutOfRange { index, elevation: tile.elevation });
        }
        let inside = |p: IVec2| p.x >= 0 && p.y >= 0 && p.x < width && p.y < height;
        if let Some(p) = std::iter::once(repr.start).chain(repr.settlement_sites.iter().copied()).find(|p| !inside(*p)) {
            return Err(WorldMapError::PositionOutOfBounds(p));
        }
        Ok(WorldMap {
            width,
            height,
            seed: repr.seed,
            tiles: repr.tiles,
            start: repr.start,
            settlement_sites: repr.settlement_sites,
            revision: 0,
            dirty_chunks: Vec::new(),
        })
    }
}

impl WorldMap {
    pub fn new_filled(width: i32, height: i32, seed: u64, tile: Tile) -> Self {
        assert!(width > 0 && height > 0);
        Self {
            width,
            height,
            seed,
            tiles: vec![tile; (width * height) as usize],
            start: IVec2::new(width / 2, height / 2),
            settlement_sites: Vec::new(),
            revision: 0,
            dirty_chunks: Vec::new(),
        }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn size(&self) -> IVec2 {
        IVec2::new(self.width, self.height)
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn in_bounds(&self, p: IVec2) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height
    }

    fn index(&self, p: IVec2) -> usize {
        (p.y * self.width + p.x) as usize
    }

    pub fn get(&self, p: IVec2) -> Option<&Tile> {
        self.in_bounds(p).then(|| &self.tiles[self.index(p)])
    }

    /// Tile at `p`; panics when out of bounds (use [`WorldMap::get`] for unchecked input).
    pub fn tile(&self, p: IVec2) -> &Tile {
        &self.tiles[self.index(p)]
    }

    /// Mutable access; marks the containing chunk dirty for renderers.
    pub fn tile_mut(&mut self, p: IVec2) -> &mut Tile {
        let chunk = IVec2::new(p.x.div_euclid(CHUNK_SIZE), p.y.div_euclid(CHUNK_SIZE));
        if !self.dirty_chunks.contains(&chunk) {
            self.dirty_chunks.push(chunk);
        }
        // Neighbouring chunks may display transition fringes / cliffs of this tile.
        for d in [IVec2::new(-1, 0), IVec2::new(0, -1), IVec2::new(1, 0), IVec2::new(0, 1)] {
            let q = p + d;
            let c = IVec2::new(q.x.div_euclid(CHUNK_SIZE), q.y.div_euclid(CHUNK_SIZE));
            if c != chunk && self.in_bounds(q) && !self.dirty_chunks.contains(&c) {
                self.dirty_chunks.push(c);
            }
        }
        self.revision += 1;
        let i = self.index(p);
        &mut self.tiles[i]
    }

    /// Mutable access without marking anything dirty, for changes renderers do not show
    /// (deposit amounts, tree growth within a visual stage). Panics when out of bounds.
    pub(crate) fn tile_mut_quiet(&mut self, p: IVec2) -> &mut Tile {
        let i = self.index(p);
        &mut self.tiles[i]
    }

    /// Takes the list of chunks changed since the last call.
    pub fn take_dirty_chunks(&mut self) -> Vec<IVec2> {
        std::mem::take(&mut self.dirty_chunks)
    }

    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    /// Drops tiles from the end (tests of damaged save data only).
    #[cfg(test)]
    pub(crate) fn truncate_tiles_for_test(&mut self, len: usize) {
        self.tiles.truncate(len);
    }

    pub(crate) fn tiles_mut_raw(&mut self) -> &mut [Tile] {
        self.revision += 1;
        &mut self.tiles
    }

    /// Number of chunks along x and y.
    pub fn chunk_count(&self) -> IVec2 {
        IVec2::new((self.width + CHUNK_SIZE - 1) / CHUNK_SIZE, (self.height + CHUNK_SIZE - 1) / CHUNK_SIZE)
    }

    /// Elevation at `p`, or `None` outside the map.
    pub fn elevation(&self, p: IVec2) -> Option<u8> {
        self.get(p).map(|t| t.elevation)
    }

    /// Iterates all tile positions in row-major order.
    pub fn positions(&self) -> impl Iterator<Item = IVec2> + '_ {
        (0..self.height).flat_map(move |y| (0..self.width).map(move |x| IVec2::new(x, y)))
    }

    /// The 4-neighbourhood of `p` that lies inside the map.
    pub fn neighbors4(&self, p: IVec2) -> impl Iterator<Item = IVec2> + '_ {
        [IVec2::new(1, 0), IVec2::new(-1, 0), IVec2::new(0, 1), IVec2::new(0, -1)]
            .into_iter()
            .map(move |d| p + d)
            .filter(move |q| self.in_bounds(*q))
    }

    /// The 8-neighbourhood of `p` that lies inside the map.
    pub fn neighbors8(&self, p: IVec2) -> impl Iterator<Item = IVec2> + '_ {
        (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| IVec2::new(dx, dy)))
            .filter(|d| *d != IVec2::ZERO)
            .map(move |d| p + d)
            .filter(move |q| self.in_bounds(*q))
    }

    /// True if every tile of the rectangle is inside the map, dry, buildable, free of
    /// vegetation and on one elevation level.
    pub fn is_flat_buildable(&self, origin: IVec2, size: IVec2) -> bool {
        let Some(first) = self.get(origin) else { return false };
        let level = first.elevation;
        for y in origin.y..origin.y + size.y {
            for x in origin.x..origin.x + size.x {
                match self.get(IVec2::new(x, y)) {
                    Some(t) if t.terrain.is_buildable() && t.elevation == level && !t.vegetation.blocks_building() => {}
                    _ => return false,
                }
            }
        }
        true
    }

    /// Counts tiles matching `pred` within a square radius.
    pub fn count_in_radius(&self, center: IVec2, radius: i32, mut pred: impl FnMut(&Tile) -> bool) -> usize {
        let mut n = 0;
        for y in center.y - radius..=center.y + radius {
            for x in center.x - radius..=center.x + radius {
                if let Some(t) = self.get(IVec2::new(x, y))
                    && pred(t)
                {
                    n += 1;
                }
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile() -> Tile {
        Tile {
            terrain: Terrain::Grass,
            biome: Biome::Grassland,
            elevation: 2,
            fertility: 0,
            moisture: 0,
            temperature: 0,
            vegetation: Vegetation::None,
            deposit: None,
            river: false,
        }
    }

    /// Same layout as the serialized map, with every field under the test's control.
    #[derive(Serialize)]
    struct RawMap {
        width: i32,
        height: i32,
        seed: u64,
        tiles: Vec<Tile>,
        start: IVec2,
        settlement_sites: Vec<IVec2>,
    }

    fn decode(raw: &RawMap) -> Result<WorldMap, bincode::error::DecodeError> {
        let config = bincode::config::standard();
        let bytes = bincode::serde::encode_to_vec(raw, config).expect("encodable");
        bincode::serde::decode_from_slice::<WorldMap, _>(&bytes, config).map(|(map, _)| map)
    }

    fn raw(width: i32, height: i32) -> RawMap {
        let len = (width.max(0) * height.max(0)) as usize;
        RawMap { width, height, seed: 3, tiles: vec![tile(); len], start: IVec2::ZERO, settlement_sites: vec![IVec2::ONE] }
    }

    #[test]
    fn max_map_edge_is_the_largest_world_size() {
        assert_eq!(WorldSize::ALL.iter().map(|s| s.tiles()).max(), Some(MAX_MAP_EDGE));
    }

    #[test]
    fn deserialization_rejects_inconsistent_maps() {
        let map = decode(&raw(4, 3)).expect("valid map");
        assert_eq!((map.width(), map.height(), map.tiles().len()), (4, 3, 12));
        assert_eq!(map.settlement_sites, vec![IVec2::ONE]);
        // Roundtrip of a real map.
        let mut m = WorldMap::new_filled(5, 5, 9, tile());
        m.settlement_sites.push(IVec2::new(4, 4));
        let config = bincode::config::standard();
        let bytes = bincode::serde::encode_to_vec(&m, config).unwrap();
        let (back, _): (WorldMap, usize) = bincode::serde::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(back.tiles(), m.tiles());

        let mut short = raw(4, 3);
        short.tiles.pop();
        assert!(decode(&short).is_err(), "one tile short");
        assert!(decode(&raw(0, 3)).is_err());
        assert!(decode(&raw(MAX_MAP_EDGE + 1, 1)).is_err());
        let mut huge = raw(1, 1);
        huge.width = i32::MAX;
        huge.height = i32::MAX;
        assert!(decode(&huge).is_err());
        let mut high = raw(2, 2);
        high.tiles[3].elevation = MAX_ELEVATION + 1;
        assert!(decode(&high).is_err());
        let mut outside = raw(2, 2);
        outside.start = IVec2::new(2, 0);
        assert!(decode(&outside).is_err());
        let mut site = raw(2, 2);
        site.settlement_sites.push(IVec2::new(-1, 1));
        assert!(decode(&site).is_err());
        assert_eq!(
            WorldMapError::TileCountMismatch { expected: 12, actual: 11 }.to_string(),
            "map has 11 tiles but its size requires 12"
        );
    }
}
