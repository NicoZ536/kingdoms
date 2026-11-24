//! Soft ground shadows (`overlay/blob_shadow`) under walkers, grazing animals and trees.
//!
//! Buildings have their cast shadow painted into their sprites; everything that moves or
//! changes with the seasons gets a blurred ellipse on the ground instead, drawn in the
//! transparent pass as a ground decal ([`SpriteDepth::ground`] with [`SHADOW_BIAS`]): above
//! terrain, fields and roads, below everything standing. The light comes from the upper left
//! (docs/assets.md), so the blob sits a little to the right of the feet. Its opacity follows
//! the light: full in sunshine, faint under heavy clouds and at night ([`shadow_alpha`]).

use glam::Vec2;
use kingdoms_engine::assets::{AssetManager, SpriteRegion};
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, SpriteDepth};
use kingdoms_engine::rendering::sprite::{RenderList, SpriteInstance};

use super::SHADOW_BIAS;
use super::daylight::{Daylight, luma};

/// Shift of a shadow from the feet in world pixels (light from the upper left).
const OFFSET: Vec2 = Vec2::new(3.0, 1.0);
/// Opacity left at night (moonlight) and under the darkest storm sky.
const NIGHT_SHARE: f32 = 0.25;
const OVERCAST_SHARE: f32 = 0.35;

/// The ground offset (tile space) that appears as screen offset `w` (world pixels).
fn screen_to_ground(w: Vec2) -> Vec2 {
    let along = w.x / iso::HALF_TILE_WIDTH;
    let down = w.y / iso::HALF_TILE_HEIGHT;
    Vec2::new((along + down) * 0.5, (down - along) * 0.5)
}

/// Opacity factor of the shadows (0–1) for the light of day `daylight` and the weather's
/// ambient light `weather` (linear RGB, 1 = clear sky).
pub fn shadow_alpha(daylight: &Daylight, weather: [f32; 3]) -> f32 {
    let day = 1.0 - (1.0 - NIGHT_SHARE) * daylight.night.clamp(0.0, 1.0);
    // Clear sky (luma 1) casts full shadows; a storm sky (luma ≈ 0.55) only faint ones.
    let sun = ((luma(weather) - 0.55) / 0.45).clamp(0.0, 1.0);
    let sky = OVERCAST_SHARE + (1.0 - OVERCAST_SHARE) * sun;
    (day * sky).clamp(0.0, 1.0)
}

/// The blob shadow sprite.
pub struct BlobShadows {
    region: Option<SpriteRegion>,
}

impl BlobShadows {
    pub fn load(assets: &AssetManager) -> Self {
        Self { region: assets.try_sprite("overlay/blob_shadow").map(|id| *assets.region(id)) }
    }

    /// Queues a shadow `width` world pixels wide (half as high) under the ground point
    /// `ground` (tile space) at `height` elevation levels, with opacity `alpha`.
    pub fn draw(&self, list: &mut RenderList, ground: Vec2, height: f32, width: f32, alpha: f32) {
        let Some(r) = self.region else { return };
        if alpha <= 0.01 || width <= 0.0 {
            return;
        }
        // Shifted on the ground, not on the screen: the decal's depth must belong to the ground
        // point its anchor shows (a pixel lower on screen is 1/16 tile deeper).
        let centre = ground + screen_to_ground(OFFSET);
        let world = iso::tile_to_world_elevated(centre, height);
        let instance = SpriteInstance::new(world, Vec2::new(width, width * 0.5), r.anchor, r.uv, SpriteDepth::ground(centre, SHADOW_BIAS))
            .with_color(Color::rgba(1.0, 1.0, 1.0, alpha.min(1.0)));
        list.draw_transparent(r.texture, instance);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::daylight::SunTimes;
    use kingdoms_sim::time::Season;

    fn at(hour: f32, night: f32) -> Daylight {
        Daylight { hour, sun: SunTimes::of(Season::Summer), light: [1.0; 3], night }
    }

    #[test]
    fn shadows_fade_at_night_and_under_clouds() {
        let clear = [1.0; 3];
        let storm = [0.52, 0.55, 0.64];
        let noon = shadow_alpha(&at(12.0, 0.0), clear);
        let midnight = shadow_alpha(&at(0.0, 1.0), clear);
        let stormy = shadow_alpha(&at(12.0, 0.0), storm);
        assert!((noon - 1.0).abs() < 1e-6);
        assert!((midnight - NIGHT_SHARE).abs() < 1e-6);
        assert!((OVERCAST_SHARE - 1e-6..0.5).contains(&stormy));
        let dusk = shadow_alpha(&at(20.0, 0.5), clear);
        assert!(midnight < dusk && dusk < noon);
    }

    #[test]
    fn the_offset_is_a_ground_offset() {
        let g = Vec2::new(7.25, 3.5);
        let moved = iso::tile_to_world(g + screen_to_ground(OFFSET)) - iso::tile_to_world(g);
        assert!((moved - OFFSET).length() < 1e-4, "{moved:?}");
        // At its anchor the shadow lies above the terrain block it is drawn on (a decal shifted
        // on screen only would sink below it and vanish in the depth test).
        let centre = g + screen_to_ground(OFFSET);
        let tile = centre.floor().as_ivec2();
        let terrain = SpriteDepth::terrain_block(tile);
        let rel_y = iso::tile_to_world(centre).y - iso::tile_center_world(tile, 0.0).y;
        let shadow = SpriteDepth::ground(centre, SHADOW_BIAS);
        assert!(shadow.evaluate(0.5, 0.0) > terrain.evaluate(0.5, rel_y));
    }

    #[test]
    fn shadows_lie_on_the_ground_below_standing_objects() {
        use kingdoms_engine::rendering::iso::OBJECT_DEPTH_BIAS;
        let p = Vec2::new(10.4, 7.6);
        let shadow = SpriteDepth::ground(p, SHADOW_BIAS);
        let walker = SpriteDepth::upright_point(p, OBJECT_DEPTH_BIAS);
        let road = SpriteDepth::ground(p, super::super::ROAD_BIAS);
        assert!(shadow.evaluate(0.5, 0.0) > road.evaluate(0.5, 0.0), "over roads and fields");
        // The walker's body (above its feet) covers the shadow.
        assert!(shadow.evaluate(0.5, -2.0) < walker.evaluate(0.5, -2.0), "under the walker itself");
    }
}
