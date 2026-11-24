//! Arrows and stones in flight: the ranged hits of the latest combat round
//! ([`Military::last_round`](kingdoms_sim::military::Military::last_round)), drawn from the
//! shooter (a soldier, or the top of a tower, archer post or castle) to the target along a
//! shallow arc. The flight starts at the round's tick and takes a few ticks by distance, so it
//! runs with the game speed and stands still while the game is paused. Arrows use the sprite
//! of the direction they fly in (`effects/arrow_<0..15>`), stones of siege engines
//! `effects/stone` with a higher arc.

use std::f32::consts::TAU;

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::{AssetManager, SpriteRegion};
use kingdoms_engine::rendering::iso::{self, OBJECT_DEPTH_BIAS, SpriteDepth};
use kingdoms_engine::rendering::sprite::{RenderList, SpriteInstance};
use kingdoms_sim::defs::Defs;
use kingdoms_sim::grid::TileRect;
use kingdoms_sim::military::{Shot, ShotKind};
use kingdoms_sim::state::GameState;

/// Arrow sprites (`effects/arrow_<k>` points `k · 22.5°` clockwise from screen right).
const ARROW_DIRS: usize = 16;
/// Height above the ground a shot leaves from: a soldier's hands, the top of a tower.
const HAND_HEIGHT: f32 = 12.0;
const TOWER_HEIGHT: f32 = 38.0;
/// Height of the point a shot hits on a walker.
const TARGET_HEIGHT: f32 = 10.0;

/// Ticks a projectile flies over `tiles` tiles, shorter than a combat round of `round_ticks`.
pub fn flight_ticks(tiles: f32, round_ticks: u16) -> f32 {
    (3.0 + tiles.max(0.0) * 0.8).min(f32::from(round_ticks.max(2)) - 1.0)
}

/// Position (world) and screen direction of a projectile flying from `start` to `end` (world
/// points) at flight fraction `t`, lifted by a parabola of height `arc` pixels.
pub fn flight_at(start: Vec2, end: Vec2, arc: f32, t: f32) -> (Vec2, Vec2) {
    let t = t.clamp(0.0, 1.0);
    let lift = arc * 4.0 * t * (1.0 - t);
    let pos = start + (end - start) * t - Vec2::new(0.0, lift);
    let velocity = (end - start) - Vec2::new(0.0, arc * (4.0 - 8.0 * t));
    (pos, velocity)
}

/// Index of the arrow sprite pointing along screen direction `v` (y down).
pub fn arrow_index(v: Vec2) -> usize {
    if v.length_squared() < 1e-6 {
        return 0;
    }
    let angle = v.y.atan2(v.x).rem_euclid(TAU);
    ((angle / TAU * ARROW_DIRS as f32).round() as usize) % ARROW_DIRS
}

fn centre(rect: TileRect) -> Vec2 {
    rect.origin.as_vec2() + rect.size.max(IVec2::ONE).as_vec2() * 0.5
}

/// Projectile art.
pub struct Projectiles {
    arrows: Option<[SpriteRegion; ARROW_DIRS]>,
    stone: Option<SpriteRegion>,
    drawn: usize,
}

impl Projectiles {
    pub fn load(assets: &AssetManager) -> Self {
        let ids: Vec<_> = (0..ARROW_DIRS).filter_map(|k| assets.try_sprite(&format!("effects/arrow_{k}"))).collect();
        let arrows = (ids.len() == ARROW_DIRS).then(|| std::array::from_fn(|k| *assets.region(ids[k])));
        if arrows.is_none() {
            log::info!("effects atlas has no arrow sprites; shots are not drawn");
        }
        let stone = assets.try_sprite("effects/stone").map(|id| *assets.region(id));
        Self { arrows, stone, drawn: 0 }
    }

    /// Projectiles drawn in the last frame.
    pub fn drawn(&self) -> usize {
        self.drawn
    }

    /// Queues the projectiles of the current combat round at game time `ticks` (whole ticks
    /// plus the frame's fraction).
    pub fn draw(&mut self, list: &mut RenderList, state: &GameState, defs: &Defs, ticks: f64) {
        self.drawn = 0;
        let round = state.military().last_round();
        let round_ticks = defs.rules().military.round_ticks;
        if !round.is_current(state.clock().tick, round_ticks) {
            return;
        }
        let age = (ticks - round.tick() as f64) as f32;
        if age < 0.0 {
            return;
        }
        let view = list.visible_rect().expanded(64.0);
        for shot in round.shots() {
            if let Some((region, instance)) = self.projectile(state, shot, age, round_ticks)
                && instance.world_bounds().intersects(&view)
            {
                list.draw_transparent(region.texture, instance);
                self.drawn += 1;
            }
        }
    }

    /// The sprite of `shot` `age` ticks after the round, if it is still in the air.
    fn projectile(&self, state: &GameState, shot: &Shot, age: f32, round_ticks: u16) -> Option<(SpriteRegion, SpriteInstance)> {
        let (from, to) = (centre(shot.from), centre(shot.to));
        let duration = flight_ticks(from.distance(to), round_ticks);
        if age > duration {
            return None;
        }
        let level = |p: Vec2| state.world().get(p.floor().as_ivec2()).map_or(0.0, |t| f32::from(t.elevation));
        // Towers (no shooting walker) and garrisons shoot from their building's area.
        let from_building =
            shot.shooter.is_none() || state.buildings().at(shot.from.origin).is_some_and(|(_, b)| b.rect() == shot.from);
        let lift = if from_building { TOWER_HEIGHT } else { HAND_HEIGHT };
        let start = iso::tile_to_world_elevated(from, level(from)) - Vec2::new(0.0, lift);
        let end = iso::tile_to_world_elevated(to, level(to)) - Vec2::new(0.0, TARGET_HEIGHT);
        let tiles = from.distance(to);
        let t = age / duration.max(0.1);
        let (arc, region) = match shot.kind {
            ShotKind::Arrow => {
                let arc = 4.0 + tiles * 2.5;
                let (_, velocity) = flight_at(start, end, arc, t);
                (arc, self.arrows.as_ref()?[arrow_index(velocity)])
            }
            ShotKind::Stone => (10.0 + tiles * 5.0, self.stone?),
        };
        let (pos, _) = flight_at(start, end, arc, t);
        let ground = from + (to - from) * t;
        let depth = SpriteDepth::upright_point(ground, OBJECT_DEPTH_BIAS + 0.02);
        Some((region, SpriteInstance::new(pos, region.size, region.anchor, region.uv, depth)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_point_along_their_flight() {
        assert_eq!(arrow_index(Vec2::new(1.0, 0.0)), 0);
        assert_eq!(arrow_index(Vec2::new(0.0, 1.0)), 4, "down the screen");
        assert_eq!(arrow_index(Vec2::new(-1.0, 0.0)), 8);
        assert_eq!(arrow_index(Vec2::new(0.0, -1.0)), 12);
        assert_eq!(arrow_index(Vec2::new(1.0, -0.01)), 0, "wraps around");
        // Shot to the right: the arrow climbs first, flies level at the top and drops at the end.
        let (a, b) = (Vec2::ZERO, Vec2::new(200.0, 0.0));
        let up = arrow_index(flight_at(a, b, 30.0, 0.05).1);
        let level = arrow_index(flight_at(a, b, 30.0, 0.5).1);
        let down = arrow_index(flight_at(a, b, 30.0, 0.95).1);
        assert!(up >= 13, "climbing: {up}");
        assert_eq!(level, 0);
        assert!((1..=3).contains(&down), "falling: {down}");
    }

    #[test]
    fn flights_start_at_the_shooter_and_end_at_the_target() {
        let (a, b) = (Vec2::new(10.0, 20.0), Vec2::new(90.0, 60.0));
        assert_eq!(flight_at(a, b, 25.0, 0.0).0, a);
        assert!((flight_at(a, b, 25.0, 1.0).0 - b).length() < 1e-4);
        // The arc lifts the middle of the flight.
        let mid = flight_at(a, b, 25.0, 0.5).0;
        assert!((mid - ((a + b) * 0.5 - Vec2::new(0.0, 25.0))).length() < 1e-4);
        // Longer shots fly longer, but every flight ends before the next round.
        assert!(flight_ticks(8.0, 10) > flight_ticks(2.0, 10));
        assert!(flight_ticks(40.0, 10) <= 9.0);
    }
}
