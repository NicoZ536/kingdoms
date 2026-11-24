//! Grazing animals on pastures.
//!
//! The simulation keeps the herd size of every pasture farm (`Farm::herd`: one animal per
//! `pasture_tiles_per_animal` claimed tiles at most, half of that for a new farm, growing day by
//! day outside winter and shrinking at once when pasture is lost). Every animal of that herd is
//! drawn as a cow or sheep (by farm type) wandering over the farm's own pasture tiles: each one
//! walks between random spots around a "home" tile and grazes in between. The individual
//! positions are presentation only – a pure function of the farm, the tile list and the game
//! time – so the animals stop while the game is paused and nothing is stored. The walk frames
//! come from the `<role>_<dir>_walk` animations; every animal casts a soft ground shadow.

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::{AssetManager, SpriteId};
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::iso::{self, OBJECT_DEPTH_BIAS, SpriteDepth};
use kingdoms_engine::rendering::sprite::{RenderList, SpriteInstance};
use kingdoms_sim::agriculture::{FarmKind, farm_spec};
use kingdoms_sim::defs::Defs;
use kingdoms_sim::state::GameState;

use super::citizens::walk_frames;
use super::shadows::BlobShadows;
use super::{hash01, tile_hash};

/// Animal art per farm type id: `(building id, characters role)`. Other pasture farms show
/// sheep.
const GRAZERS: &[(&str, &str)] = &[("cattle_farm", "cow"), ("sheep_farm", "sheep")];
const FALLBACK: &str = "sheep";
/// Shadow widths in world pixels.
const COW_SHADOW: f32 = 22.0;
const SHEEP_SHADOW: f32 = 16.0;
/// Game ticks of one walk-and-graze cycle and the share spent walking.
const CYCLE_TICKS: f64 = 140.0;
const WALK_SHARE: f64 = 0.45;
/// Walk animation frames per game second (20 ticks).
const WALK_FPS: f64 = 8.0;
/// Largest offset of a grazing spot from its tile centre (tiles).
const SPOT_JITTER: f32 = 0.3;

/// Facing in the `characters` atlas: `se` = +x, `sw` = +y, `nw` = −x, `ne` = −y.
const DIRS: [&str; 4] = ["se", "sw", "nw", "ne"];

/// Where an animal is and what it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Ground position in tile space.
    pub pos: Vec2,
    /// Index into [`DIRS`].
    pub dir: usize,
    /// Walk animation frame 0–3 (0 while grazing).
    pub frame: usize,
    /// Walking (else grazing on a spot).
    pub walking: bool,
}

/// Grazing spot `k` of an animal: a jittered point on its home tile or a pasture tile next to
/// it (the animal never leaves the farm's pasture).
fn spot(seed: u32, home: IVec2, near: &[IVec2], k: u64) -> Vec2 {
    let h = seed ^ (k as u32).wrapping_mul(0x9e37_79b9) ^ ((k >> 32) as u32).wrapping_mul(0x85eb_ca6b);
    let tile = near.get((tile_hash(IVec2::new(h as i32, 17)) as usize) % near.len().max(1)).copied().unwrap_or(home);
    let jitter = Vec2::new(hash01(h, 1) * 2.0 - 1.0, hash01(h, 2) * 2.0 - 1.0) * SPOT_JITTER;
    tile.as_vec2() + Vec2::splat(0.5) + jitter
}

/// Facing of a move by `d` in tile space.
fn facing(d: Vec2) -> usize {
    if d.x.abs() >= d.y.abs() {
        if d.x >= 0.0 { 0 } else { 2 }
    } else if d.y >= 0.0 {
        1
    } else {
        3
    }
}

/// Pose of animal `index` of a farm with pasture `tiles` at game time `ticks` (`None` without
/// pasture). `seed` identifies the farm.
pub fn pose(seed: u32, index: usize, tiles: &[IVec2], ticks: f64) -> Option<Pose> {
    if tiles.is_empty() {
        return None;
    }
    let seed = seed ^ (index as u32 + 1).wrapping_mul(0x27d4_eb2d);
    let home = tiles[(tile_hash(IVec2::new(seed as i32, index as i32)) as usize) % tiles.len()];
    let near: Vec<IVec2> = tiles.iter().copied().filter(|t| (*t - home).abs().max_element() <= 1).collect();
    // Animals are out of step with each other.
    let t = ticks.max(0.0) / CYCLE_TICKS + f64::from(hash01(seed, 3));
    let k = t.floor() as u64;
    let f = t - t.floor();
    let from = spot(seed, home, &near, k.wrapping_sub(1));
    let to = spot(seed, home, &near, k);
    let d = to - from;
    let dir = facing(if d.length_squared() > 1e-6 { d } else { spot(seed, home, &near, k.wrapping_sub(2)) - from });
    if f < WALK_SHARE && d.length_squared() > 1e-6 {
        let s = (f / WALK_SHARE) as f32;
        let walked = f * CYCLE_TICKS / 20.0;
        let frame = ((walked * WALK_FPS) as usize) % 4;
        Some(Pose { pos: from + d * s, dir, frame, walking: true })
    } else {
        Some(Pose { pos: to, dir, frame: 0, walking: false })
    }
}

/// Number of animals drawn for a herd of `herd` on `tiles` pasture tiles: the simulation's
/// herd, none without pasture to stand on.
pub fn animal_count(herd: u16, tiles: usize) -> usize {
    if tiles == 0 { 0 } else { usize::from(herd) }
}

/// Resolved art of one animal kind.
struct AnimalArt {
    /// `[dir][frame]`
    frames: [[SpriteId; 4]; 4],
    shadow: f32,
}

/// Grazing animals of all pasture farms.
pub struct Grazers {
    /// Art per building type index (`None` for types without pasture).
    by_type: Vec<Option<AnimalArt>>,
}

impl Grazers {
    pub fn load(defs: &Defs, assets: &AssetManager) -> Self {
        let by_type = defs
            .buildings()
            .iter()
            .map(|def| {
                let (kind, _, _) = farm_spec(def)?;
                if kind != FarmKind::Pasture {
                    return None;
                }
                let role = GRAZERS.iter().find(|(id, _)| *id == def.key).map_or(FALLBACK, |(_, role)| *role);
                let frames = std::array::from_fn(|d| walk_frames(assets, role, DIRS[d]));
                Some(AnimalArt { frames, shadow: if role == "cow" { COW_SHADOW } else { SHEEP_SHADOW } })
            })
            .collect();
        Self { by_type }
    }

    /// Draws the animals of every pasture farm near the view at game time `ticks`, with their
    /// shadows at opacity `shadow_alpha`. Returns the number of animals drawn.
    pub fn draw(
        &self,
        list: &mut RenderList,
        state: &GameState,
        assets: &AssetManager,
        shadows: &BlobShadows,
        ticks: f64,
        shadow_alpha: f32,
    ) -> usize {
        let view = list.visible_rect().expanded(64.0);
        let world = state.world();
        let mut drawn = 0;
        let mut tiles: Vec<IVec2> = Vec::new();
        for (id, farm) in state.agriculture().farms() {
            if farm.kind() != FarmKind::Pasture || farm.tiles().is_empty() || farm.herd() == 0 {
                continue;
            }
            let Some(b) = state.building(id) else { continue };
            let Some(Some(art)) = self.by_type.get(b.kind().index()) else { continue };
            tiles.clear();
            tiles.extend(farm.tiles().iter().map(|t| t.pos()));
            if !tiles_visible(&tiles, &view) {
                continue;
            }
            let seed = tile_hash(b.rect().origin);
            for index in 0..animal_count(farm.herd(), tiles.len()) {
                let Some(p) = pose(seed, index, &tiles, ticks) else { continue };
                let tile = p.pos.floor().as_ivec2();
                let elevation = world.get(tile).map_or(0.0, |t| f32::from(t.elevation));
                let r = assets.region(art.frames[p.dir][p.frame]);
                let instance = SpriteInstance::new(
                    iso::tile_to_world_elevated(p.pos, elevation),
                    r.size,
                    r.anchor,
                    r.uv,
                    SpriteDepth::upright_point(p.pos, OBJECT_DEPTH_BIAS),
                );
                list.draw(r.texture, instance);
                shadows.draw(list, p.pos, elevation, art.shadow, shadow_alpha);
                drawn += 1;
            }
        }
        drawn
    }
}

/// Cheap test whether any of `tiles` may be on screen.
fn tiles_visible(tiles: &[IVec2], view: &Rect) -> bool {
    tiles.iter().any(|t| view.contains(iso::tile_center_world(*t, 0.0)) || view.contains(iso::tile_center_world(*t, 8.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasture() -> Vec<IVec2> {
        let mut tiles = Vec::new();
        for y in 10..14 {
            for x in 20..24 {
                tiles.push(IVec2::new(x, y));
            }
        }
        tiles.retain(|t| *t != IVec2::new(21, 11));
        tiles
    }

    #[test]
    fn animals_stay_on_their_pasture() {
        let tiles = pasture();
        for index in 0..4 {
            for step in 0..600 {
                let ticks = f64::from(step) * 7.3;
                let Some(p) = pose(99, index, &tiles, ticks) else { panic!("no pose") };
                // Grazing spots lie on pasture tiles; walks between neighbouring spots stay
                // within the pasture's bounding box.
                assert!(p.pos.x > 20.0 - 0.01 && p.pos.x < 24.01 && p.pos.y > 10.0 - 0.01 && p.pos.y < 14.01, "{p:?}");
                assert!(p.dir < 4 && p.frame < 4);
            }
        }
        // Grazing animals stand on a pasture tile.
        let grazing: Vec<Pose> = (0..400).filter_map(|s| pose(99, 1, &tiles, f64::from(s) * 11.0)).filter(|p| !p.walking).collect();
        assert!(!grazing.is_empty());
        for p in grazing {
            assert!(tiles.contains(&p.pos.floor().as_ivec2()), "{p:?}");
        }
    }

    #[test]
    fn poses_are_deterministic_and_animals_move() {
        let tiles = pasture();
        assert_eq!(pose(5, 2, &tiles, 1234.5), pose(5, 2, &tiles, 1234.5));
        let positions: Vec<Vec2> = (0..50).filter_map(|s| pose(5, 2, &tiles, f64::from(s) * 40.0)).map(|p| p.pos).collect();
        assert!(positions.windows(2).any(|w| (w[0] - w[1]).length() > 0.05), "the animal never moves");
        assert_ne!(pose(5, 0, &tiles, 500.0), pose(5, 1, &tiles, 500.0));
        assert_eq!(pose(5, 0, &[], 500.0), None);
    }

    #[test]
    fn the_simulated_herd_is_drawn() {
        assert_eq!(animal_count(0, 12), 0);
        assert_eq!(animal_count(3, 12), 3);
        assert_eq!(animal_count(14, 28), 14);
        assert_eq!(animal_count(5, 0), 0);
        assert_eq!(facing(Vec2::new(1.0, 0.2)), 0);
        assert_eq!(facing(Vec2::new(0.1, 1.0)), 1);
        assert_eq!(facing(Vec2::new(-1.0, 0.2)), 2);
        assert_eq!(facing(Vec2::new(0.1, -1.0)), 3);
    }
}
