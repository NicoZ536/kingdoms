//! Presentation layer: turns simulation state into sprites (terrain, vegetation, roads,
//! buildings, overlays) and resolves what lies under the cursor.
//!
//! ## Depth bias ladder for ground decals
//! All flat decals use [`SpriteDepth::ground`](kingdoms_engine::rendering::iso::SpriteDepth::ground)
//! with a bias above the terrain surface. Steps are ≥ 0.003 iso-depth units (several ulps of the
//! 32-bit depth buffer at the largest map depth), so the order below holds independently of the
//! draw order: terrain fringes (`0.010–0.014`) < elevation rims / AO (`0.015`) <
//! [`DEPOSIT_BIAS`] = [`FIELD_BIAS`] (never on the same tile) < [`ROAD_BIAS`] < [`SHADOW_BIAS`]
//! < [`BORDER_BIAS`] < [`GRID_BIAS`] < [`HIGHLIGHT_BIAS`] < [`PREVIEW_BIAS`].

pub mod animals;
pub mod buildings;
pub mod camera;
mod chunks;
pub mod citizens;
pub mod daylight;
pub mod effects;
pub mod fields;
mod ground;
pub mod kingdoms;
pub mod lights;
pub mod orders;
pub mod overlay;
pub mod picking;
pub mod preview;
pub mod projectiles;
pub mod roads;
pub mod shadows;
pub mod terrain;
pub mod vegetation;
pub mod world_view;

use glam::IVec2;

/// Ground markers of deposits.
pub const DEPOSIT_BIAS: f32 = 0.018;
/// Field and pasture tiles (farms never claim deposit tiles, so they share the step).
pub const FIELD_BIAS: f32 = DEPOSIT_BIAS;
/// Road surfaces.
pub const ROAD_BIAS: f32 = 0.022;
/// Soft shadows of walkers, animals and trees (above roads, below borders; both are blended
/// without writing depth, so only the step to the opaque roads below matters).
pub const SHADOW_BIAS: f32 = 0.025;
/// Game ticks per second of animation time: the simulation's design rate (20 ticks per second
/// at normal speed), so work loops, building loops and projectiles follow the game speed and
/// stop while the game is paused.
pub const ANIMATION_TICKS_PER_SECOND: f64 = 20.0;
/// Territory border lines.
pub const BORDER_BIAS: f32 = 0.026;
/// Developer grid.
pub const GRID_BIAS: f32 = 0.030;
/// Hover and selection highlights.
pub const HIGHLIGHT_BIAS: f32 = 0.034;
/// Tool previews above the highlights (planned roads, markers).
pub const PREVIEW_BIAS: f32 = 0.038;

/// Deterministic per-tile hash for art variants and placement jitter.
pub fn tile_hash(p: IVec2) -> u32 {
    let mut h = (p.x as u32).wrapping_mul(0x27d4_eb2d) ^ (p.y as u32).wrapping_mul(0x1656_67b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^ (h >> 13)
}

/// A value in `[0, 1)` derived from a hash and a salt (independent streams per salt).
pub fn hash01(hash: u32, salt: u32) -> f32 {
    let mut h = hash ^ salt.wrapping_mul(0x9e37_79b9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_deterministic_and_in_range() {
        let a = tile_hash(IVec2::new(3, 9));
        assert_eq!(a, tile_hash(IVec2::new(3, 9)));
        assert_ne!(a, tile_hash(IVec2::new(9, 3)));
        for salt in 0..16 {
            let v = hash01(a, salt);
            assert!((0.0..1.0).contains(&v));
        }
        assert_ne!(hash01(a, 1), hash01(a, 2));
    }

    #[test]
    fn bias_ladder_is_ordered_with_safe_steps() {
        let ladder = [0.015, FIELD_BIAS, ROAD_BIAS, BORDER_BIAS, GRID_BIAS, HIGHLIGHT_BIAS, PREVIEW_BIAS];
        assert!(ladder.windows(2).all(|w| w[1] - w[0] >= 0.003 - 1e-6));
    }
}
