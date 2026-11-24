//! Tile-grid geometry shared by buildings, roads and territory.
//!
//! Map axes: `+x` points east, `+y` points south (docs/game_design.md §5.6). Distances between
//! tiles are Chebyshev distances (`max(|dx|, |dy|)`) unless stated otherwise.

use std::fmt;

use glam::IVec2;
use serde::{Deserialize, Serialize};

/// One of the four map directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Direction {
    /// `-y`.
    North,
    /// `+x`.
    East,
    /// `+y`.
    South,
    /// `-x`.
    West,
}

impl Direction {
    /// All directions in mask-bit order (N, E, S, W).
    pub const ALL: [Direction; 4] = [Direction::North, Direction::East, Direction::South, Direction::West];

    /// Unit step in this direction.
    pub const fn offset(self) -> IVec2 {
        match self {
            Direction::North => IVec2::new(0, -1),
            Direction::East => IVec2::new(1, 0),
            Direction::South => IVec2::new(0, 1),
            Direction::West => IVec2::new(-1, 0),
        }
    }

    /// Bit of this direction in a 4-neighbour connection mask (N=1, E=2, S=4, W=8).
    pub const fn mask_bit(self) -> u8 {
        match self {
            Direction::North => 1,
            Direction::East => 2,
            Direction::South => 4,
            Direction::West => 8,
        }
    }

    /// Position in [`Direction::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The opposite direction.
    pub const fn opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::East => Direction::West,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
        }
    }

    /// Axis this direction runs along.
    pub const fn axis(self) -> Axis {
        match self {
            Direction::North | Direction::South => Axis::Y,
            Direction::East | Direction::West => Axis::X,
        }
    }

    /// Direction of a unit step (`None` unless `step` is one of the four unit offsets).
    pub fn from_step(step: IVec2) -> Option<Direction> {
        Direction::ALL.into_iter().find(|d| d.offset() == step)
    }

    /// Direction suffix of the character sprites (docs/assets.md: `ne` = −y, `se` = +x,
    /// `sw` = +y, `nw` = −x, i.e. the screen diagonals of the isometric view).
    pub const fn sprite_dir(self) -> &'static str {
        match self {
            Direction::North => "ne",
            Direction::East => "se",
            Direction::South => "sw",
            Direction::West => "nw",
        }
    }
}

/// A map axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Axis {
    /// East–west.
    X,
    /// North–south.
    Y,
}

impl Axis {
    /// Suffix used by axis-dependent sprites (`bridge_x`, `gate_y`, …).
    pub const fn sprite_suffix(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
        }
    }
}

/// Axis-aligned rectangle of tiles: `origin` is the minimum corner, `size` the extent in tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileRect {
    /// Minimum corner (smallest x and y).
    pub origin: IVec2,
    /// Width (x) and depth (y) in tiles; both positive for non-empty rectangles.
    pub size: IVec2,
}

impl TileRect {
    /// Rectangle from its minimum corner and size.
    pub const fn new(origin: IVec2, size: IVec2) -> Self {
        Self { origin, size }
    }

    /// Single-tile rectangle.
    pub const fn tile(p: IVec2) -> Self {
        Self { origin: p, size: IVec2::ONE }
    }

    /// Smallest rectangle containing both tiles.
    pub fn spanning(a: IVec2, b: IVec2) -> Self {
        let min = a.min(b);
        let max = a.max(b);
        Self { origin: min, size: max - min + IVec2::ONE }
    }

    /// Largest tile coordinate inside the rectangle.
    pub fn max_inclusive(&self) -> IVec2 {
        self.origin + self.size - IVec2::ONE
    }

    /// Number of tiles.
    pub fn area(&self) -> i32 {
        self.size.x.max(0) * self.size.y.max(0)
    }

    /// True if `p` lies inside.
    pub fn contains(&self, p: IVec2) -> bool {
        p.x >= self.origin.x
            && p.y >= self.origin.y
            && p.x < self.origin.x + self.size.x
            && p.y < self.origin.y + self.size.y
    }

    /// True if the rectangles share at least one tile.
    pub fn intersects(&self, other: &TileRect) -> bool {
        self.origin.x < other.origin.x + other.size.x
            && other.origin.x < self.origin.x + self.size.x
            && self.origin.y < other.origin.y + other.size.y
            && other.origin.y < self.origin.y + self.size.y
    }

    /// Iterates all tiles in row-major order.
    pub fn tiles(&self) -> impl Iterator<Item = IVec2> + '_ {
        let (o, s) = (self.origin, self.size);
        (0..s.y.max(0)).flat_map(move |dy| (0..s.x.max(0)).map(move |dx| o + IVec2::new(dx, dy)))
    }

    /// The rectangle grown by `margin` tiles on every side.
    pub fn expanded(&self, margin: i32) -> TileRect {
        TileRect { origin: self.origin - IVec2::splat(margin), size: self.size + IVec2::splat(2 * margin) }
    }

    /// The part of the rectangle inside a `width` × `height` map (may be empty).
    pub fn clipped(&self, width: i32, height: i32) -> TileRect {
        let min = self.origin.max(IVec2::ZERO);
        let max = (self.origin + self.size).min(IVec2::new(width, height));
        TileRect { origin: min, size: (max - min).max(IVec2::ZERO) }
    }

    /// Chebyshev distance from `p` to the nearest tile of the rectangle (0 inside).
    pub fn distance_to(&self, p: IVec2) -> i32 {
        self.distance_to_rect(&TileRect::tile(p))
    }

    /// Smallest Chebyshev distance between a tile of `self` and a tile of `other`
    /// (0 if they overlap, 1 if they touch).
    pub fn distance_to_rect(&self, other: &TileRect) -> i32 {
        let a_max = self.max_inclusive();
        let b_max = other.max_inclusive();
        let dx = (other.origin.x - a_max.x).max(self.origin.x - b_max.x).max(0);
        let dy = (other.origin.y - a_max.y).max(self.origin.y - b_max.y).max(0);
        dx.max(dy)
    }

    /// Twice the centre point (integer, so odd sizes stay exact).
    pub fn center_x2(&self) -> IVec2 {
        self.origin * 2 + self.size
    }
}

/// Change tracker for renderers: a revision counter and the list of chunks
/// ([`CHUNK_SIZE`](crate::world::CHUNK_SIZE) tiles square) touched since the last
/// [`ChangeTracker::take_chunks`]. Not part of the saved state (a loaded game starts clean).
#[derive(Clone, Debug, Default)]
pub struct ChangeTracker {
    revision: u64,
    chunks: Vec<IVec2>,
    /// Rectangles touched since the last [`ChangeTracker::take_rects`] (navigation grid).
    rects: Vec<TileRect>,
    /// More than [`MAX_TRACKED_RECTS`] changes piled up: the consumer rebuilds everything.
    rects_overflow: bool,
}

/// Most changed rectangles kept for [`ChangeTracker::take_rects`] before it reports an overflow.
const MAX_TRACKED_RECTS: usize = 512;

impl ChangeTracker {
    /// Records a change of the tiles in `rect` (clipped to nothing outside the map is needed:
    /// chunk coordinates of out-of-map tiles are simply never drawn).
    pub fn touch(&mut self, rect: TileRect) {
        self.revision = self.revision.wrapping_add(1);
        if self.rects.len() < MAX_TRACKED_RECTS {
            self.rects.push(rect);
        } else {
            self.rects_overflow = true;
        }
        let size = crate::world::CHUNK_SIZE;
        let lo = IVec2::new(rect.origin.x.div_euclid(size), rect.origin.y.div_euclid(size));
        let max = rect.max_inclusive();
        let hi = IVec2::new(max.x.div_euclid(size), max.y.div_euclid(size));
        for cy in lo.y..=hi.y {
            for cx in lo.x..=hi.x {
                let c = IVec2::new(cx, cy);
                if !self.chunks.contains(&c) {
                    self.chunks.push(c);
                }
            }
        }
    }

    /// Number of recorded changes (wraps around).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Takes the chunks changed since the last call.
    pub fn take_chunks(&mut self) -> Vec<IVec2> {
        std::mem::take(&mut self.chunks)
    }

    /// Takes the rectangles changed since the last call (independent of
    /// [`ChangeTracker::take_chunks`], which belongs to renderers); `None` if too many changes
    /// piled up and the consumer must treat the whole map as changed.
    pub(crate) fn take_rects(&mut self) -> Option<Vec<TileRect>> {
        let rects = std::mem::take(&mut self.rects);
        if std::mem::take(&mut self.rects_overflow) { None } else { Some(rects) }
    }
}

impl fmt::Display for TileRect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{} at [{}, {}]", self.size.x, self.size.y, self.origin.x, self.origin.y)
    }
}

/// A building's orientation in quarter turns (0–3). Rotating swaps the footprint's width and
/// depth for odd turns and moves the entrance side clockwise: 0 = front faces south (`+y`),
/// 1 = west, 2 = north, 3 = east.
///
/// The footprint origin is always the minimum corner of the *rotated* rectangle. Local building
/// coordinates `(lx, ly)` (unrotated, `0..width` × `0..depth`, front edge at `ly = depth`) map to
/// world tiles with [`Rotation::local_to_world`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Rotation(u8);

/// A rotation value outside 0–3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("rotation {0} is outside 0..=3")]
pub struct InvalidRotation(pub u8);

impl Rotation {
    /// All rotations in order.
    pub const ALL: [Rotation; 4] = [Rotation(0), Rotation(1), Rotation(2), Rotation(3)];

    /// Rotation from a number of quarter turns (`None` outside 0–3).
    pub const fn new(quarter_turns: u8) -> Option<Rotation> {
        if quarter_turns < 4 { Some(Rotation(quarter_turns)) } else { None }
    }

    /// Rotation from any number of quarter turns (taken modulo 4).
    pub const fn wrapping(quarter_turns: u8) -> Rotation {
        Rotation(quarter_turns % 4)
    }

    /// Number of clockwise quarter turns (0–3).
    pub const fn quarter_turns(self) -> u8 {
        self.0
    }

    /// The next rotation clockwise.
    pub const fn rotated_cw(self) -> Rotation {
        Rotation((self.0 + 1) % 4)
    }

    /// The next rotation counter-clockwise.
    pub const fn rotated_ccw(self) -> Rotation {
        Rotation((self.0 + 3) % 4)
    }

    /// Side the building's front (and entrance) faces.
    pub const fn facing(self) -> Direction {
        match self.0 {
            0 => Direction::South,
            1 => Direction::West,
            2 => Direction::North,
            _ => Direction::East,
        }
    }

    /// True for odd quarter turns (width and depth swapped).
    pub const fn swaps_axes(self) -> bool {
        self.0 % 2 == 1
    }

    /// Footprint size of a building with unrotated size `base`.
    pub const fn footprint_size(self, base: IVec2) -> IVec2 {
        if self.swaps_axes() { IVec2::new(base.y, base.x) } else { base }
    }

    /// Occupied rectangle of a building with unrotated size `base` placed at `origin`.
    pub fn footprint(self, origin: IVec2, base: IVec2) -> TileRect {
        TileRect::new(origin, self.footprint_size(base))
    }

    /// World tile of local building coordinate `local` (may lie outside the footprint, e.g. the
    /// entrance at `ly = depth`).
    pub fn local_to_world(self, origin: IVec2, base: IVec2, local: IVec2) -> IVec2 {
        let (w, d) = (base.x, base.y);
        let rel = match self.0 {
            0 => local,
            1 => IVec2::new(d - 1 - local.y, local.x),
            2 => IVec2::new(w - 1 - local.x, d - 1 - local.y),
            _ => IVec2::new(local.y, w - 1 - local.x),
        };
        origin + rel
    }

    /// Entrance tile: the tile directly in front of the middle of the front edge (outside the
    /// footprint). Roads connect to the building there.
    pub fn entrance(self, origin: IVec2, base: IVec2) -> IVec2 {
        self.local_to_world(origin, base, IVec2::new(base.x / 2, base.y))
    }

    /// Door tile: the footprint tile adjacent to the entrance.
    pub fn door(self, origin: IVec2, base: IVec2) -> IVec2 {
        self.local_to_world(origin, base, IVec2::new(base.x / 2, base.y - 1))
    }

    /// Origin after resizing a building from `old_base` to `new_base` (house upgrades to a
    /// larger tier) such that the entrance tile and the front edge stay in place: the building
    /// grows backwards and sideways around its entrance. For a size that does not shrink the
    /// new footprint contains the old one (`n / 2` and `n - n / 2` both grow with `n`).
    pub fn grown_origin(self, origin: IVec2, old_base: IVec2, new_base: IVec2) -> IVec2 {
        let old_anchor = self.entrance(IVec2::ZERO, old_base);
        let new_anchor = self.entrance(IVec2::ZERO, new_base);
        origin + old_anchor - new_anchor
    }
}

impl TryFrom<u8> for Rotation {
    type Error = InvalidRotation;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Rotation::new(value).ok_or(InvalidRotation(value))
    }
}

impl From<Rotation> for u8 {
    fn from(r: Rotation) -> u8 {
        r.0
    }
}

impl fmt::Display for Rotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}°", u16::from(self.0) * 90)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_are_consistent() {
        for d in Direction::ALL {
            assert_eq!(d.opposite().opposite(), d);
            assert_eq!(d.offset() + d.opposite().offset(), IVec2::ZERO);
            assert_eq!(Direction::from_step(d.offset()), Some(d));
            assert_eq!(Direction::ALL[d.index()], d);
        }
        let bits: u8 = Direction::ALL.iter().map(|d| d.mask_bit()).sum();
        assert_eq!(bits, 15);
        assert_eq!(Direction::North.offset(), IVec2::new(0, -1));
        assert_eq!(Direction::East.mask_bit(), 2);
        assert_eq!(Direction::from_step(IVec2::new(1, 1)), None);
        assert_eq!(Direction::West.axis(), Axis::X);
    }

    #[test]
    fn rect_geometry() {
        let r = TileRect::new(IVec2::new(2, 3), IVec2::new(3, 2));
        assert_eq!(r.area(), 6);
        assert_eq!(r.tiles().count(), 6);
        assert_eq!(r.tiles().next(), Some(IVec2::new(2, 3)));
        assert_eq!(r.max_inclusive(), IVec2::new(4, 4));
        assert!(r.contains(IVec2::new(4, 4)) && !r.contains(IVec2::new(5, 4)));
        assert_eq!(r.distance_to(IVec2::new(3, 3)), 0);
        assert_eq!(r.distance_to(IVec2::new(6, 2)), 2);
        assert_eq!(r.distance_to(IVec2::new(5, 5)), 1);
        let other = TileRect::new(IVec2::new(5, 3), IVec2::ONE);
        assert_eq!(r.distance_to_rect(&other), 1);
        assert!(!r.intersects(&other));
        assert!(r.intersects(&r.expanded(1)));
        assert_eq!(r.expanded(1).area(), 20);
        assert_eq!(TileRect::spanning(IVec2::new(5, 1), IVec2::new(2, 4)), TileRect::new(IVec2::new(2, 1), IVec2::new(4, 4)));
        let clipped = TileRect::new(IVec2::new(-2, -2), IVec2::new(5, 5)).clipped(2, 10);
        assert_eq!(clipped, TileRect::new(IVec2::ZERO, IVec2::new(2, 3)));
        assert_eq!(r.center_x2(), IVec2::new(7, 8));
    }

    #[test]
    fn rotation_footprint_and_entrance() {
        let base = IVec2::new(2, 3); // e.g. a sawmill
        let o = IVec2::new(10, 10);
        // 0: front faces south.
        let r0 = Rotation::new(0).unwrap();
        assert_eq!(r0.footprint(o, base), TileRect::new(o, IVec2::new(2, 3)));
        assert_eq!(r0.entrance(o, base), IVec2::new(11, 13));
        assert_eq!(r0.facing(), Direction::South);
        // 1: width/depth swapped, front faces west.
        let r1 = r0.rotated_cw();
        assert_eq!(r1.footprint_size(base), IVec2::new(3, 2));
        assert_eq!(r1.entrance(o, base), IVec2::new(9, 11));
        assert_eq!(r1.facing(), Direction::West);
        // 2: front faces north.
        let r2 = r1.rotated_cw();
        assert_eq!(r2.entrance(o, base), IVec2::new(10, 9));
        // 3: front faces east.
        let r3 = r2.rotated_cw();
        assert_eq!(r3.footprint_size(base), IVec2::new(3, 2));
        assert_eq!(r3.entrance(o, base), IVec2::new(13, 10));
        assert_eq!(r3.rotated_cw(), r0);
        assert_eq!(r0.rotated_ccw(), r3);
        for r in Rotation::ALL {
            let rect = r.footprint(o, base);
            // Every local tile maps into the footprint, bijectively.
            let mut seen: Vec<IVec2> = (0..base.y)
                .flat_map(|y| (0..base.x).map(move |x| IVec2::new(x, y)))
                .map(|l| r.local_to_world(o, base, l))
                .collect();
            assert!(seen.iter().all(|p| rect.contains(*p)), "{r}");
            seen.sort_by_key(|p| (p.y, p.x));
            seen.dedup();
            assert_eq!(seen.len(), 6);
            // The entrance is just outside, in front of the door, on the facing side.
            let entrance = r.entrance(o, base);
            let door = r.door(o, base);
            assert!(!rect.contains(entrance) && rect.contains(door));
            assert_eq!(entrance - door, r.facing().offset());
        }
    }

    #[test]
    fn grown_origin_keeps_the_front_edge_and_the_entrance() {
        let o = IVec2::new(20, 20);
        // House tiers: 2x2 -> 3x3 (residence) -> 4x4 (manor), plus a non-square case.
        for (old, new) in [((2, 2), (3, 3)), ((3, 3), (4, 4)), ((2, 2), (4, 4)), ((2, 3), (3, 4))] {
            let (old, new) = (IVec2::new(old.0, old.1), IVec2::new(new.0, new.1));
            for r in Rotation::ALL {
                let g = r.grown_origin(o, old, new);
                let old_rect = r.footprint(o, old);
                let new_rect = r.footprint(g, new);
                let what = format!("{old} -> {new} at {r}");
                assert!(new_rect.contains(old_rect.origin) && new_rect.contains(old_rect.max_inclusive()), "{what}");
                // The front edge line and the entrance tile are unchanged.
                let front = |rect: &TileRect| match r.facing() {
                    Direction::South => rect.max_inclusive().y,
                    Direction::North => rect.origin.y,
                    Direction::West => rect.origin.x,
                    Direction::East => rect.max_inclusive().x,
                };
                assert_eq!(front(&old_rect), front(&new_rect), "{what}");
                assert_eq!(r.entrance(o, old), r.entrance(g, new), "{what}");
            }
        }
    }

    #[test]
    fn rotation_serde_rejects_invalid_values() {
        assert_eq!(ron::to_string(&Rotation::wrapping(7)).unwrap(), "3");
        assert_eq!(ron::from_str::<Rotation>("2").unwrap(), Rotation::new(2).unwrap());
        assert!(ron::from_str::<Rotation>("4").is_err());
        assert_eq!(Rotation::try_from(9), Err(InvalidRotation(9)));
        assert_eq!(Rotation::wrapping(1).to_string(), "90°");
    }
}
