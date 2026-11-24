//! Isometric (2:1 dimetric) projection and the sprite depth model.
//!
//! ## Coordinate systems
//! - **Tile space**: continuous grid coordinates `(tx, ty)`. Tile `(i, j)` covers `[i, i+1) × [j, j+1)`.
//!   `+x` runs to the lower right on screen, `+y` to the lower left.
//! - **World space**: 2D pixel space of the projected map, `y` grows downwards (like the screen).
//!   One tile is a diamond [`TILE_WIDTH`] × [`TILE_HEIGHT`] pixels; elevation lifts things by
//!   [`ELEVATION_STEP`] pixels per level.
//! - **Screen space**: physical window pixels; the [`super::camera::Camera2D`] maps world ↔ screen.
//!
//! ## Depth model
//! Sprites are drawn with a depth buffer instead of CPU painter sorting. The *iso depth* `D` of a
//! ground point is `tx + ty` (larger = closer to the viewer). Every sprite carries two depth
//! functions and the fragment shader uses the minimum of both:
//! - **flat**: `D(y) = d_anchor + slope · (world_y − anchor_y)` — ground-aligned surfaces
//!   (terrain tops, roads, fields). For ground `slope = 1 / HALF_TILE_HEIGHT`.
//! - **tent**: piecewise linear across the sprite width through `(0, d_left)`, `(split, d_center)`,
//!   `(1, d_right)` and constant vertically — upright objects standing on a footprint. The values
//!   are the depths of the footprint's left, front and right corners, so every pixel column gets
//!   the depth of the front-facing wall at that column.
//!
//! Disabling both (value [`DEPTH_DISABLED`]) yields an overlay that is always drawn on top.
//! Terrain blocks use both: flat for the top face and tent for the vertical skirt below it.
//! Sprites flagged [`FLAG_DEPTH_MAX`](super::sprite::FLAG_DEPTH_MAX) take the maximum instead:
//! ground cover with standing parts ([`SpriteDepth::ground_cover`]: crops, orchards) is a flat
//! decal on the front of its tile but never further back than a floor depth, so plants rising
//! above the tile stay in front of everything behind the tile.

use glam::{IVec2, UVec2, Vec2};

use crate::math::Rect;

/// Width of one tile diamond in world pixels.
pub const TILE_WIDTH: f32 = 64.0;
/// Height of one tile diamond in world pixels.
pub const TILE_HEIGHT: f32 = 32.0;
pub const HALF_TILE_WIDTH: f32 = TILE_WIDTH * 0.5;
pub const HALF_TILE_HEIGHT: f32 = TILE_HEIGHT * 0.5;
/// World pixels one elevation level lifts the terrain.
pub const ELEVATION_STEP: f32 = 16.0;
/// Depth values at or above this are treated as "disabled" / overlay.
pub const DEPTH_DISABLED: f32 = 1.0e7;
/// Iso depth range mapped onto the depth buffer `[0, 1]`. Supports maps up to ~4000 tiles per side.
pub const DEPTH_RANGE: f32 = 8192.0;
/// Small depth bias for objects that stand on the ground (walkers, props) so they win ties
/// against the terrain they stand on.
pub const OBJECT_DEPTH_BIAS: f32 = 0.02;
/// Bias for ground decals (roads, fields) drawn on top of terrain.
pub const DECAL_DEPTH_BIAS: f32 = 0.01;

/// Projects a tile-space point at elevation 0 to world space.
pub fn tile_to_world(t: Vec2) -> Vec2 {
    Vec2::new((t.x - t.y) * HALF_TILE_WIDTH, (t.x + t.y) * HALF_TILE_HEIGHT)
}

/// Projects a tile-space point lifted by `elevation` levels.
pub fn tile_to_world_elevated(t: Vec2, elevation: f32) -> Vec2 {
    tile_to_world(t) - Vec2::new(0.0, elevation * ELEVATION_STEP)
}

/// Inverse projection onto the elevation-0 ground plane.
pub fn world_to_tile(w: Vec2) -> Vec2 {
    let a = w.x / HALF_TILE_WIDTH;
    let b = w.y / HALF_TILE_HEIGHT;
    Vec2::new((b + a) * 0.5, (b - a) * 0.5)
}

/// Inverse projection onto the plane at `elevation` levels.
pub fn world_to_tile_elevated(w: Vec2, elevation: f32) -> Vec2 {
    world_to_tile(w + Vec2::new(0.0, elevation * ELEVATION_STEP))
}

/// World position of the centre of tile `t` at elevation `elevation`.
pub fn tile_center_world(t: IVec2, elevation: f32) -> Vec2 {
    tile_to_world_elevated(t.as_vec2() + Vec2::splat(0.5), elevation)
}

/// Iso depth of a ground point.
pub fn ground_depth(t: Vec2) -> f32 {
    t.x + t.y
}

/// World-space bounding box of the top diamond of a footprint.
pub fn footprint_world_bounds(origin: IVec2, size: UVec2, elevation: f32) -> Rect {
    let o = origin.as_vec2();
    let s = size.as_vec2();
    let top = tile_to_world_elevated(o, elevation);
    let right = tile_to_world_elevated(o + Vec2::new(s.x, 0.0), elevation);
    let bottom = tile_to_world_elevated(o + s, elevation);
    let left = tile_to_world_elevated(o + Vec2::new(0.0, s.y), elevation);
    Rect::new(Vec2::new(left.x, top.y), Vec2::new(right.x, bottom.y))
}

/// Per-sprite depth parameters (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteDepth {
    /// `[d_anchor, slope]`
    pub flat: [f32; 2],
    /// `[d_left, d_center, d_right, split_u]`
    pub tent: [f32; 4],
}

impl SpriteDepth {
    /// Always drawn on top of the world (placement previews, markers).
    pub const OVERLAY: SpriteDepth = SpriteDepth {
        flat: [DEPTH_DISABLED, 0.0],
        tent: [DEPTH_DISABLED, DEPTH_DISABLED, DEPTH_DISABLED, 0.5],
    };

    /// Flat ground-aligned sprite whose anchor lies on ground point `anchor_tile`.
    pub fn ground(anchor_tile: Vec2, bias: f32) -> Self {
        SpriteDepth {
            flat: [ground_depth(anchor_tile) + bias, 1.0 / HALF_TILE_HEIGHT],
            tent: Self::OVERLAY.tent,
        }
    }

    /// Terrain block for tile `t`: flat top face plus an upright skirt hanging below the front
    /// edges. The sprite's anchor must be the centre of the top diamond and its horizontal
    /// extent exactly one tile wide.
    pub fn terrain_block(t: IVec2) -> Self {
        let base = (t.x + t.y) as f32;
        SpriteDepth {
            flat: [base + 1.0, 1.0 / HALF_TILE_HEIGHT],
            tent: [base + 1.0, base + 2.0, base + 1.0, 0.5],
        }
    }

    /// Upright sprite standing on a single ground point (walkers, trees, props).
    pub fn upright_point(ground_tile: Vec2, bias: f32) -> Self {
        let d = ground_depth(ground_tile) + bias;
        SpriteDepth { flat: Self::OVERLAY.flat, tent: [d, d, d, 0.5] }
    }

    /// Upright sprite standing on a rectangular footprint (buildings). The sprite must span the
    /// footprint diamond horizontally: left edge at the footprint's left corner, right edge at its
    /// right corner.
    pub fn footprint(origin: IVec2, size: UVec2, bias: f32) -> Self {
        let x0 = origin.x as f32;
        let y0 = origin.y as f32;
        let w = size.x.max(1) as f32;
        let h = size.y.max(1) as f32;
        let left = x0 + y0 + h;
        let front = x0 + y0 + w + h;
        let right = x0 + y0 + w;
        SpriteDepth {
            flat: Self::OVERLAY.flat,
            // The front corner (x0 + w, y0 + h) lies w / (w + h) of the sprite width from the
            // left corner (x0, y0 + h): screen x grows with x and shrinks with y.
            tent: [left + bias, front + bias, right + bias, w / (w + h)],
        }
    }

    /// Ground cover on one tile whose parts rise above it (fields with plants or small trees);
    /// draw it with [`FLAG_DEPTH_MAX`](super::sprite::FLAG_DEPTH_MAX). The front of the tile
    /// is a flat decal anchored at `anchor_tile` (like [`SpriteDepth::ground`]); everything above
    /// or behind ground depth `floor` is drawn at `floor`, i.e. like an upright object standing
    /// at that depth.
    pub fn ground_cover(anchor_tile: Vec2, floor: f32, bias: f32) -> Self {
        let f = floor + bias;
        SpriteDepth { flat: [ground_depth(anchor_tile) + bias, 1.0 / HALF_TILE_HEIGHT], tent: [f, f, f, 0.5] }
    }

    /// Depth of a sprite that covers only the horizontal part `u0..u1` of this upright sprite
    /// (fractions of its width): the tent restricted to that span, re-parameterised to the
    /// smaller sprite's own width; the flat function is kept. An overlay drawn on top of a
    /// building (lit windows, a sign) then gets the depth of the walls it lies on.
    pub fn sub_span(&self, u0: f32, u1: f32) -> Self {
        let width = u1 - u0;
        let left = self.tent_at(u0);
        if width.abs() < 1e-6 {
            return SpriteDepth { flat: self.flat, tent: [left, left, left, 0.5] };
        }
        let right = self.tent_at(u1);
        // Position of the tent's peak within the span.
        let split = (self.tent[3] - u0) / width;
        let tent = if split <= 0.0 {
            // Entirely right of the peak: one linear segment (see `tent_at` for split 0).
            [left, left, right, 0.0]
        } else if split >= 1.0 {
            [left, right, right, 1.0]
        } else {
            [left, self.tent_at(self.tent[3]), right, split]
        };
        SpriteDepth { flat: self.flat, tent }
    }

    /// Evaluates the depth with `max` instead of `min` (sprites flagged `FLAG_DEPTH_MAX`).
    pub fn evaluate_max(&self, u: f32, rel_y: f32) -> f32 {
        self.flat_at(rel_y).max(self.tent_at(u))
    }

    /// Evaluates the depth function on the CPU (mirrors the shader; used by tests and sorting).
    pub fn evaluate(&self, u: f32, rel_y: f32) -> f32 {
        self.flat_at(rel_y).min(self.tent_at(u))
    }

    fn flat_at(&self, rel_y: f32) -> f32 {
        self.flat[0] + self.flat[1] * rel_y
    }

    fn tent_at(&self, u: f32) -> f32 {
        let [l, c, r, split] = self.tent;
        if u <= split {
            if split <= 0.0 { c } else { l + (c - l) * (u / split) }
        } else if split >= 1.0 {
            c
        } else {
            c + (r - c) * ((u - split) / (1.0 - split))
        }
    }

    /// Representative depth for back-to-front sorting of transparent sprites.
    pub fn sort_key(&self) -> f32 {
        self.evaluate(self.tent[3], 0.0).min(DEPTH_DISABLED)
    }
}

/// Converts an iso depth into a depth-buffer value (`0` = nearest).
pub fn depth_to_ndc(d: f32) -> f32 {
    (1.0 - d / DEPTH_RANGE).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_roundtrip() {
        for &(x, y) in &[(0.0, 0.0), (3.5, 7.25), (100.0, 2.0), (-4.0, 9.0)] {
            let t = Vec2::new(x, y);
            let back = world_to_tile(tile_to_world(t));
            assert!((back - t).length() < 1e-4, "{t:?} -> {back:?}");
            let back_e = world_to_tile_elevated(tile_to_world_elevated(t, 3.0), 3.0);
            assert!((back_e - t).length() < 1e-4);
        }
    }

    #[test]
    fn tile_diamond_geometry() {
        // Tile (0,0): top corner at origin, right corner at (+32, 16), bottom at (0, 32).
        assert_eq!(tile_to_world(Vec2::new(1.0, 0.0)), Vec2::new(32.0, 16.0));
        assert_eq!(tile_to_world(Vec2::new(1.0, 1.0)), Vec2::new(0.0, 32.0));
        assert_eq!(tile_to_world(Vec2::new(0.0, 1.0)), Vec2::new(-32.0, 16.0));
    }

    #[test]
    fn flat_depth_matches_ground_depth() {
        // A flat sprite anchored at tile (2,3) evaluated 16 px lower is one depth unit closer.
        let d = SpriteDepth::ground(Vec2::new(2.0, 3.0), 0.0);
        assert!((d.evaluate(0.5, 0.0) - 5.0).abs() < 1e-5);
        assert!((d.evaluate(0.5, 16.0) - 6.0).abs() < 1e-5);
    }

    #[test]
    fn footprint_tent_corners() {
        let d = SpriteDepth::footprint(IVec2::new(4, 4), UVec2::new(2, 2), 0.0);
        assert!((d.evaluate(0.0, -100.0) - 10.0).abs() < 1e-5);
        assert!((d.evaluate(0.5, -100.0) - 12.0).abs() < 1e-5);
        assert!((d.evaluate(1.0, -100.0) - 10.0).abs() < 1e-5);
    }

    #[test]
    fn footprint_split_matches_front_corner_for_non_square() {
        // 3 tiles along x, 1 along y at the origin: left corner (0,1), front (3,1), right (3,0).
        let origin = IVec2::ZERO;
        let d = SpriteDepth::footprint(origin, UVec2::new(3, 1), 0.0);
        let left_x = tile_to_world(Vec2::new(0.0, 1.0)).x;
        let right_x = tile_to_world(Vec2::new(3.0, 0.0)).x;
        let front_x = tile_to_world(Vec2::new(3.0, 1.0)).x;
        let u_front = (front_x - left_x) / (right_x - left_x);
        assert!((d.tent[3] - u_front).abs() < 1e-5, "split {} vs front corner {}", d.tent[3], u_front);
        assert!((d.evaluate(u_front, -100.0) - 4.0).abs() < 1e-5);
    }

    #[test]
    fn walker_in_front_of_wall_wins() {
        // Building at (4,4) 2x2; walker standing just in front of the right-front wall near the
        // right corner, at (6.2, 4.2). Screen x of the walker relative to the sprite.
        let b = SpriteDepth::footprint(IVec2::new(4, 4), UVec2::new(2, 2), 0.0);
        let walker = Vec2::new(6.2, 4.2);
        let w = SpriteDepth::upright_point(walker, OBJECT_DEPTH_BIAS);
        // Sprite spans from the left corner (4,6) to the right corner (6,4): world x -64..64.
        let wx = tile_to_world(walker).x;
        let u = (wx - tile_to_world(Vec2::new(4.0, 6.0)).x) / 128.0;
        assert!(w.evaluate(0.5, 0.0) > b.evaluate(u, 0.0));
    }

    #[test]
    fn terrain_skirt_is_behind_front_neighbour() {
        // The skirt of tile (0,0) hangs over tile (1,1)'s top face area at equal elevation.
        let a = SpriteDepth::terrain_block(IVec2::new(0, 0));
        let b = SpriteDepth::terrain_block(IVec2::new(1, 1));
        // Pixel 8 px below the front corner of (0,0): relative to a's anchor (centre) that is
        // 16 + 8 = 24 px; relative to b's anchor (16 px below the front corner... centre of (1,1)
        // is 32 px below a's centre) it is -8 px.
        let da = a.evaluate(0.5, 24.0);
        let db = b.evaluate(0.5, -8.0);
        assert!(db > da, "front neighbour {db} must be nearer than skirt {da}");
    }

    #[test]
    fn ground_cover_is_a_decal_in_front_and_upright_behind() {
        // Field on tile (4, 4): centre depth 9, floor at the back half of the tile.
        let d = SpriteDepth::ground_cover(Vec2::new(4.5, 4.5), 8.5, 0.0);
        // Front corner (16 px below the centre): flat decal depth 10, above its own terrain.
        assert!((d.evaluate_max(0.5, 16.0) - 10.0).abs() < 1e-5);
        // Plants 30 px above the centre stay at the floor instead of sinking to depth 7.
        assert!((d.evaluate_max(0.5, -30.0) - 8.5).abs() < 1e-5);
        // A tree on the tile behind (depth 8 + object bias) is covered, one in front is not.
        let behind = SpriteDepth::upright_point(Vec2::new(3.5, 4.5), OBJECT_DEPTH_BIAS);
        let front = SpriteDepth::upright_point(Vec2::new(5.5, 4.5), OBJECT_DEPTH_BIAS);
        assert!(d.evaluate_max(0.5, -30.0) > behind.evaluate(0.5, 0.0));
        assert!(front.evaluate(0.5, 0.0) > d.evaluate_max(0.5, 16.0));
    }

    #[test]
    fn sub_span_keeps_the_depth_of_the_covered_columns() {
        let b = SpriteDepth::footprint(IVec2::new(4, 4), UVec2::new(3, 2), 0.0);
        for (u0, u1) in [(0.1, 0.35), (0.2, 0.9), (0.7, 0.95), (0.0, 1.0)] {
            let part = b.sub_span(u0, u1);
            for k in 0..=10 {
                let v = k as f32 / 10.0;
                let u = u0 + (u1 - u0) * v;
                let (expected, got) = (b.evaluate(u, -50.0), part.evaluate(v, -50.0));
                assert!((expected - got).abs() < 1e-4, "span {u0}..{u1} at {v}: {got} != {expected}");
            }
        }
        let point = b.sub_span(0.4, 0.4);
        assert!((point.evaluate(0.3, 0.0) - b.evaluate(0.4, 0.0)).abs() < 1e-5);
    }

    #[test]
    fn overlay_is_always_on_top() {
        assert_eq!(depth_to_ndc(SpriteDepth::OVERLAY.evaluate(0.5, 0.0)), 0.0);
        assert!(depth_to_ndc(10.0) > 0.0);
    }
}
