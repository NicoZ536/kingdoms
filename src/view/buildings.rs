//! Buildings, construction sites, walls and gates, drawn every frame from the simulation state.
//!
//! ## Sprite orientation
//! There is one sprite per building type. A sprite for a `B×T` type (`B` = width, `T` = depth of
//! the unrotated type, [`BuildingDef::size`](kingdoms_sim::defs::BuildingDef::size)) is drawn
//! for a footprint of `T` tiles along tile x and `B` along tile y, with the door on its lit,
//! lower-left wall (facing `+y`, docs/assets.md "Geometrie"). Mirroring it (`FLAG_FLIP_X`)
//! swaps the iso axes: it then covers `B` along x and `T` along y and the door wall faces `+x`.
//!
//! The simulation's footprint is `(B, T)` for rotations 0/2 (entrance south/north) and `(T, B)`
//! for rotations 1/3 (west/east) ([`Rotation::footprint_size`](kingdoms_sim::grid::Rotation)).
//! * Non-square types: the footprint decides — unmirrored for odd rotations, mirrored for even
//!   ones. The drawn door then sits on the visible wall next to the real entrance side (the
//!   sprite's door wall is the `T` side while the simulation's entrance is on the `B` side).
//! * Square types: mirrored when the entrance faces north or east, so the drawn door is on the
//!   visible wall next to the entrance (south: exact, east: exact, west → south wall, north →
//!   east wall).
//!
//! ## Phases
//! * New buildings and moves: `construction_<B>x<T>_<stage>` on the (new) footprint; a moved
//!   building's old spot is already free in the simulation.
//! * Buildings damaged by an event: `rubble_<B>x<T>` while the repair site is at stage 0, then
//!   the construction stages ([`Building::sprite`](kingdoms_sim::buildings::Building::sprite)).
//! * House upgrades: the footprint is reserved for the larger tier, so the target size's
//!   construction stage is drawn on the whole footprint and the current house (which keeps
//!   working) on its previous footprint in front of the scaffolding.
//! * Walls and gates: `roads/wall_<mask>` (mask of neighbouring wall segments and gates) and
//!   `roads/gate_<x|y>` (axis of the wall line); construction sites as usual.
//! * Winter: a type with the sprite `buildings/<id>_winter` (the orchard with bare trees) shows
//!   it in winter.
//! * Work loops: while its production runs, a building with the animation
//!   `buildings/<id>_work` in the atlas plays it at game time (mill sails turn, forge fires
//!   flicker; [`BuildingSprites::work_frame`]). The loop frames share the finished sprite's
//!   width and anchor, so picking, lights and smoke keep using the finished sprite.
//! * Paused buildings are drawn greyed, finished buildings without any of their workers dimmed.
//! * In snow the green parts of the plots (lawns, hedges) turn white
//!   ([`FLAG_SNOW_ON_GREEN`]), and so do roofs and flat tops, which the sprites mark for the
//!   shader ([`FLAG_SNOW_ON_ROOF`], docs/assets.md).
//! * The cast shadows painted into the sprites are marked as well ([`FLAG_SHADOW_MARK`]): they
//!   fade by daylight and weather like the soft ground shadows of walkers and trees.

use std::collections::BTreeMap;

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::{Animation, AssetManager, SpriteId};
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, OBJECT_DEPTH_BIAS, SpriteDepth};
use kingdoms_engine::rendering::sprite::{FLAG_FLIP_X, FLAG_SHADOW_MARK, FLAG_SNOW_ON_GREEN, FLAG_SNOW_ON_ROOF, RenderList, SpriteInstance};
use kingdoms_sim::buildings::{Building, BuildingId, BuildingPhase, SitePurpose, wall_mask};
use kingdoms_sim::defs::{BuildingRole, BuildingTypeId, Defs};
use kingdoms_sim::grid::{Direction, TileRect};
use kingdoms_sim::production::ProductionStatus;
use kingdoms_sim::time::Season;
use kingdoms_sim::state::GameState;
use kingdoms_sim::world::WorldMap;

/// Weather on the sprites of buildings, walls and gates: snow on green plot parts and the roofs,
/// cast shadows by the light.
pub const SNOW_FLAGS: u32 = FLAG_SNOW_ON_GREEN | FLAG_SNOW_ON_ROOF | FLAG_SHADOW_MARK;

/// Extra depth bias of a house drawn in front of its own upgrade scaffolding.
const UPGRADE_HOUSE_BIAS: f32 = 0.01;
/// Construction stages per footprint size.
const STAGES: usize = 3;

/// True if the sprite of a `dims` (`B×T`) type must be mirrored on a footprint of
/// `footprint` tiles whose entrance faces `facing` (see the module docs).
pub fn sprite_flipped(dims: IVec2, footprint: IVec2, facing: Direction) -> bool {
    if dims.x == dims.y {
        matches!(facing, Direction::North | Direction::East)
    } else {
        // Unmirrored the sprite covers (x: T, y: B).
        footprint != IVec2::new(dims.y, dims.x)
    }
}

/// One sprite of a building placed on a footprint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub sprite: SpriteId,
    pub rect: TileRect,
    pub elevation: f32,
    pub flip: bool,
    pub bias: f32,
}

impl Part {
    fn on(sprite: SpriteId, rect: TileRect, elevation: f32, flip: bool) -> Self {
        Self { sprite, rect, elevation, flip, bias: OBJECT_DEPTH_BIAS }
    }

    /// World position of the footprint's front corner (the sprite's anchor).
    pub fn front_corner(&self) -> Vec2 {
        iso::tile_to_world_elevated((self.rect.origin + self.rect.size).as_vec2(), self.elevation)
    }

    /// Depth function of the sprite (tent over the footprint's front walls).
    pub fn depth(&self) -> SpriteDepth {
        SpriteDepth::footprint(self.rect.origin, self.rect.size.max(IVec2::ONE).as_uvec2(), self.bias)
    }

    /// Sprite instance with the anchor mirrored for flipped sprites.
    pub fn instance(&self, assets: &AssetManager) -> (SpriteInstance, kingdoms_engine::rendering::texture::TextureId) {
        let r = assets.region(self.sprite);
        let mut anchor = r.anchor;
        if self.flip {
            anchor.x = 1.0 - anchor.x;
        }
        let mut instance = SpriteInstance::new(self.front_corner(), r.size, anchor, r.uv, self.depth());
        if self.flip {
            instance = instance.with_flags(FLAG_FLIP_X);
        }
        (instance, r.texture)
    }

    /// True if world point `w` lies on the part's silhouette: the footprint diamond extruded
    /// upwards to the top of the sprite.
    pub fn silhouette_contains(&self, w: Vec2, sprite_height: f32) -> bool {
        let o = self.rect.origin.as_vec2();
        let s = self.rect.size.as_vec2();
        let corner = |t: Vec2| iso::tile_to_world_elevated(t, self.elevation);
        let (back, right, front, left) =
            (corner(o), corner(o + Vec2::new(s.x, 0.0)), corner(o + s), corner(o + Vec2::new(0.0, s.y)));
        if w.x < left.x || w.x > right.x {
            return false;
        }
        let along = |a: Vec2, b: Vec2| a.y + (b.y - a.y) * ((w.x - a.x) / (b.x - a.x).max(1e-3));
        let lower = if w.x <= front.x { along(left, front) } else { along(front, right) };
        let upper = if w.x <= back.x { along(left, back) } else { along(back, right) };
        let extrusion = (back.y - (front.y - sprite_height)).max(0.0);
        w.y <= lower && w.y >= upper - extrusion
    }

    /// Normalised position of world point `w` inside the drawn sprite (`(0, 0)` top left of
    /// the texture region, mirrored for flipped sprites; outside 0..1 off the sprite), the
    /// inverse of the sprite shader's placement.
    pub fn sprite_local(&self, w: Vec2, assets: &AssetManager) -> Vec2 {
        let r = assets.region(self.sprite);
        let mut anchor = r.anchor;
        if self.flip {
            anchor.x = 1.0 - anchor.x;
        }
        let mut local = (w - self.front_corner()) / r.size.max(Vec2::ONE) + anchor;
        if self.flip {
            local.x = 1.0 - local.x;
        }
        local
    }

    /// True if the part's sprite has a visible (opaque) pixel at world point `w`. Sprites of
    /// textures without an alpha mask count as solid inside their silhouette.
    pub fn covers(&self, w: Vec2, assets: &AssetManager) -> bool {
        let height = assets.region(self.sprite).size.y;
        self.silhouette_contains(w, height) && assets.sprite_opaque_at(self.sprite, self.sprite_local(w, assets)) != Some(false)
    }

    /// Depth of the front walls at world x (for picking the nearest building).
    pub fn depth_at(&self, world_x: f32) -> f32 {
        let o = self.rect.origin.as_vec2();
        let s = self.rect.size.as_vec2();
        let left = iso::tile_to_world(o + Vec2::new(0.0, s.y)).x;
        let right = iso::tile_to_world(o + Vec2::new(s.x, 0.0)).x;
        let u = ((world_x - left) / (right - left).max(1e-3)).clamp(0.0, 1.0);
        SpriteDepth::footprint(self.rect.origin, self.rect.size.max(IVec2::ONE).as_uvec2(), 0.0).evaluate(u, 0.0)
    }
}

/// Everything drawn for one building.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Visual {
    pub main: Part,
    /// The current house in front of its upgrade scaffolding.
    pub extra: Option<Part>,
    pub tint: Color,
}

impl Visual {
    pub fn parts(&self) -> impl Iterator<Item = &Part> {
        std::iter::once(&self.main).chain(self.extra.as_ref())
    }
}

struct TypeSprites {
    finished: SpriteId,
    dims: IVec2,
    role: BuildingRole,
    /// `buildings/<id>_work`, if the atlas has a work loop for the type.
    work: Option<Animation>,
    /// `buildings/<id>_winter`, if the atlas has a winter look for the type (bare orchard).
    winter: Option<SpriteId>,
}

/// Resolved building sprites.
pub struct BuildingSprites {
    types: Vec<TypeSprites>,
    construction: BTreeMap<(i32, i32), [SpriteId; STAGES]>,
    /// `rubble_<B>x<T>` per footprint size (damaged buildings before their repair reaches stage 1).
    rubble: BTreeMap<(i32, i32), SpriteId>,
    walls: [SpriteId; 16],
    /// `[x, y]`
    gates: [SpriteId; 2],
}

impl BuildingSprites {
    pub fn load(defs: &Defs, assets: &AssetManager) -> Self {
        let types: Vec<TypeSprites> = defs
            .buildings()
            .iter()
            .map(|d| TypeSprites {
                finished: assets.sprite(&d.sprite()),
                dims: d.size,
                role: d.role,
                work: assets
                    .try_animation(&format!("{}_work", d.sprite()))
                    .and_then(|id| assets.animation_data(id))
                    .filter(|a| !a.frames.is_empty())
                    .cloned(),
                winter: assets.try_sprite(&format!("{}_winter", d.sprite())),
            })
            .collect();
        for (d, t) in defs.buildings().iter().zip(&types) {
            let width = assets.region(t.finished).size.x;
            if t.finished != assets.missing_sprite() && !sprite_width_matches(d.size, width) {
                log::warn!("sprite {} is {width} px wide, footprint {} needs {} px", d.sprite(), d.size, (d.size.x + d.size.y) * 32);
            }
        }
        let mut construction = BTreeMap::new();
        let mut rubble = BTreeMap::new();
        for d in defs.buildings() {
            construction.entry((d.size.x, d.size.y)).or_insert_with(|| {
                std::array::from_fn(|stage| {
                    assets.sprite(&format!("buildings/construction_{}x{}_{stage}", d.size.x, d.size.y))
                })
            });
            rubble
                .entry((d.size.x, d.size.y))
                .or_insert_with(|| assets.sprite(&format!("buildings/rubble_{}x{}", d.size.x, d.size.y)));
        }
        let walls = std::array::from_fn(|mask| assets.sprite(&format!("roads/wall_{mask}")));
        let gates = [assets.sprite("roads/gate_x"), assets.sprite("roads/gate_y")];
        Self { types, construction, rubble, walls, gates }
    }

    /// The frame of the work loop to draw for building `b` (`id`) at game time `seconds`: only
    /// for finished buildings of a type with a loop while their production runs; each
    /// building starts at its own point of the loop.
    pub fn work_frame(&self, id: BuildingId, b: &Building, seconds: f32) -> Option<SpriteId> {
        let anim = self.types.get(b.kind().index())?.work.as_ref()?;
        let working = matches!(b.phase(), BuildingPhase::Active)
            && !b.is_paused()
            && b.production().is_some_and(|p| p.status() == ProductionStatus::Working);
        working.then(|| {
            let offset = (id.index() % 17) as f32 / 17.0 * anim.duration();
            anim.frame_at(seconds.max(0.0) + offset)
        })
    }

    /// Sprite of the finished building of type `kind` and the type's unrotated size.
    pub fn finished(&self, kind: BuildingTypeId) -> Option<(SpriteId, IVec2)> {
        self.types.get(kind.index()).map(|t| (t.finished, t.dims))
    }

    fn construction(&self, dims: IVec2, stage: u8, assets: &AssetManager) -> SpriteId {
        self.construction
            .get(&(dims.x, dims.y))
            .map(|s| s[usize::from(stage).min(STAGES - 1)])
            .unwrap_or_else(|| assets.missing_sprite())
    }

    fn rubble(&self, dims: IVec2, assets: &AssetManager) -> SpriteId {
        self.rubble.get(&(dims.x, dims.y)).copied().unwrap_or_else(|| assets.missing_sprite())
    }

    /// What to draw for building `b`.
    pub fn visual(&self, b: &Building, defs: &Defs, state: &GameState, assets: &AssetManager) -> Option<Visual> {
        let ty = self.types.get(b.kind().index())?;
        let world = state.world();
        let rect = b.rect();
        let elevation = footprint_elevation(world, rect);
        let facing = b.facing();
        let (main, extra) = match b.phase() {
            BuildingPhase::Construction(site) => {
                let sprite = if site.purpose() == SitePurpose::Repair && site.stage() == 0 {
                    self.rubble(ty.dims, assets)
                } else {
                    self.construction(ty.dims, site.stage(), assets)
                };
                (Part::on(sprite, rect, elevation, sprite_flipped(ty.dims, rect.size, facing)), None)
            }
            BuildingPhase::Upgrading(up) => {
                let target_dims = self.types.get(up.target().index()).map_or(ty.dims, |t| t.dims);
                let scaffold = self.construction(target_dims, up.site().stage(), assets);
                let main = Part::on(scaffold, rect, elevation, sprite_flipped(target_dims, rect.size, facing));
                let old = up.previous_rect();
                let mut house =
                    Part::on(ty.finished, old, footprint_elevation(world, old), sprite_flipped(ty.dims, old.size, facing));
                house.bias += UPGRADE_HOUSE_BIAS;
                (main, Some(house))
            }
            BuildingPhase::Active => {
                let sprite = match ty.role {
                    BuildingRole::WallSegment => {
                        self.walls[usize::from(wall_mask(state.buildings(), defs, rect.origin).unwrap_or(0) & 15)]
                    }
                    BuildingRole::Gate => {
                        let mask = wall_mask(state.buildings(), defs, rect.origin).unwrap_or(0);
                        self.gates[gate_axis_index(mask, facing)]
                    }
                    _ => match ty.winter {
                        Some(winter) if state.clock().season() == Season::Winter => winter,
                        _ => ty.finished,
                    },
                };
                let flip = match ty.role {
                    BuildingRole::WallSegment | BuildingRole::Gate => false,
                    _ => sprite_flipped(ty.dims, rect.size, facing),
                };
                (Part::on(sprite, rect, elevation, flip), None)
            }
        };
        Some(Visual { main, extra, tint: status_tint(b, defs) })
    }
}

/// `gate_x` (0) when the wall line runs along x (neighbours east/west), `gate_y` (1) along y;
/// without a clear line the gate's orientation decides (front south/north = passage along y =
/// wall along x).
fn gate_axis_index(mask: u8, facing: Direction) -> usize {
    let along_x = mask & (Direction::East.mask_bit() | Direction::West.mask_bit()) != 0;
    let along_y = mask & (Direction::North.mask_bit() | Direction::South.mask_bit()) != 0;
    match (along_x, along_y) {
        (true, false) => 0,
        (false, true) => 1,
        _ => match facing {
            Direction::North | Direction::South => 0,
            Direction::East | Direction::West => 1,
        },
    }
}

/// Subtle tint for buildings that are stopped (paused) or lack all of their workers.
fn status_tint(b: &Building, defs: &Defs) -> Color {
    if !b.is_complete() {
        return Color::WHITE;
    }
    if b.is_paused() {
        Color::rgb(0.66, 0.68, 0.76)
    } else if b.workers_wanted(defs) > 0 && b.staffed() == 0 {
        Color::rgb(0.82, 0.82, 0.84)
    } else {
        Color::WHITE
    }
}

/// Highest elevation level under a footprint (buildings never sink into a terrace).
pub fn footprint_elevation(world: &WorldMap, rect: TileRect) -> f32 {
    rect.tiles().filter_map(|p| world.get(p)).map(|t| t.elevation).max().map_or(0.0, f32::from)
}

/// How a building is emphasised this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emphasis {
    None,
    Hovered,
    Selected,
    /// Under the cursor of the demolish tool.
    Demolish,
}

/// Draws all buildings intersecting the view; working buildings play their work loop at game
/// time `seconds`.
pub fn draw_buildings(
    list: &mut RenderList,
    state: &GameState,
    defs: &Defs,
    sprites: &BuildingSprites,
    assets: &AssetManager,
    seconds: f32,
    emphasis: impl Fn(BuildingId) -> Emphasis,
) -> usize {
    let view = list.visible_rect();
    let mut drawn = 0;
    for (id, b) in state.buildings().iter() {
        if !footprint_may_be_visible(b.rect(), &view) {
            continue;
        }
        let Some(visual) = sprites.visual(b, defs, state, assets) else { continue };
        let tint = match emphasis(id) {
            Emphasis::None => visual.tint,
            Emphasis::Hovered => scaled(visual.tint, 1.12),
            Emphasis::Selected => scaled(visual.tint, 1.22),
            Emphasis::Demolish => Color::rgb(1.0, 0.42, 0.36),
        };
        let work = sprites.work_frame(id, b, seconds);
        for part in visual.parts() {
            let part = match work {
                Some(frame) if visual.extra.is_none() => Part { sprite: frame, ..*part },
                _ => *part,
            };
            let (instance, texture) = part.instance(assets);
            // Lawns and hedges of the plots and the roofs take on the snow cover.
            list.draw(texture, instance.with_color(tint).with_flags(SNOW_FLAGS));
            drawn += 1;
        }
    }
    drawn
}

fn scaled(c: Color, f: f32) -> Color {
    Color::rgba(c.r * f, c.g * f, c.b * f, 1.0)
}

/// Highest sprite top above a footprint's back corner, in world pixels (tall towers and the
/// highest elevation included); used for cheap rejection before resolving sprites.
const MAX_SPRITE_RISE: f32 = 400.0;

/// Cheap test before resolving sprites: can anything of a building on `rect` touch `area`?
pub(crate) fn footprint_may_be_visible(rect: TileRect, area: &Rect) -> bool {
    let bounds = iso::footprint_world_bounds(rect.origin, rect.size.max(IVec2::ONE).as_uvec2(), 0.0);
    let grown = Rect::new(bounds.min - Vec2::new(0.0, MAX_SPRITE_RISE), bounds.max);
    grown.intersects(area)
}

/// The building whose sprite is frontmost at world point `w`, with its depth there, given the
/// ground tile `ground` under the cursor (with its elevation). A building is hit through its
/// footprint or the silhouette of its sprite when nothing nearer (the ground in front of it)
/// covers the point.
pub fn pick_building(
    state: &GameState,
    defs: &Defs,
    sprites: &BuildingSprites,
    assets: &AssetManager,
    w: Vec2,
    ground: Option<(IVec2, f32)>,
) -> Option<(BuildingId, f32)> {
    let ground_depth = ground.map(|(_, e)| {
        let t = iso::world_to_tile_elevated(w, e);
        t.x + t.y
    });
    let mut best: Option<(f32, BuildingId)> =
        ground.and_then(|(p, _)| state.buildings().id_at(p)).zip(ground_depth).map(|(id, d)| (d, id));
    let probe = Rect::new(w, w);
    for (id, b) in state.buildings().iter() {
        if !footprint_may_be_visible(b.rect(), &probe) {
            continue;
        }
        let Some(visual) = sprites.visual(b, defs, state, assets) else { continue };
        for part in visual.parts() {
            // Only where the sprite is drawn: not the air beside a gable roof or between
            // towers.
            if !part.covers(w, assets) {
                continue;
            }
            let depth = part.depth_at(w.x);
            // The ground in front of the building covers it.
            if ground_depth.is_some_and(|g| depth + 0.05 < g) {
                continue;
            }
            if best.is_none_or(|(d, _)| depth > d) {
                best = Some((depth, id));
            }
        }
    }
    best.map(|(depth, id)| (id, depth))
}

/// Depth of the frontmost building pixel drawn at world point `w` (`None`: no building sprite
/// is opaque there). A walker is hidden at `w` only by a building that is opaque there and in
/// front of it.
pub fn opaque_front_depth(
    state: &GameState,
    defs: &Defs,
    sprites: &BuildingSprites,
    assets: &AssetManager,
    w: Vec2,
) -> Option<f32> {
    let probe = Rect::new(w, w);
    let mut front: Option<f32> = None;
    for (_, b) in state.buildings().iter() {
        if !footprint_may_be_visible(b.rect(), &probe) {
            continue;
        }
        let Some(visual) = sprites.visual(b, defs, state, assets) else { continue };
        for part in visual.parts().filter(|p| p.covers(w, assets)) {
            let depth = part.depth_at(w.x);
            front = Some(front.map_or(depth, |f| f.max(depth)));
        }
    }
    front
}

/// The sprite of a footprint must be exactly as wide as the footprint diamond
/// (docs/assets.md "Geometrie").
pub fn sprite_width_matches(rect_size: IVec2, sprite_width: f32) -> bool {
    ((rect_size.x + rect_size.y) as f32 * iso::HALF_TILE_WIDTH - sprite_width).abs() < 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_sim::grid::Rotation;

    /// Simulated footprint, facing and flip for a type of size `dims` at every rotation.
    fn orientations(dims: IVec2) -> Vec<(IVec2, Direction, bool)> {
        Rotation::ALL
            .iter()
            .map(|r| {
                let size = r.footprint_size(dims);
                (size, r.facing(), sprite_flipped(dims, size, r.facing()))
            })
            .collect()
    }

    /// Footprint the sprite covers: `(T, B)` unmirrored, `(B, T)` mirrored.
    fn covered(dims: IVec2, flip: bool) -> IVec2 {
        if flip { dims } else { IVec2::new(dims.y, dims.x) }
    }

    #[test]
    fn non_square_sprites_always_cover_the_simulated_footprint() {
        for dims in [IVec2::new(2, 3), IVec2::new(3, 4)] {
            for (size, facing, flip) in orientations(dims) {
                assert_eq!(covered(dims, flip), size, "{dims} facing {facing:?}");
                // Sprite width (B + T) · 32 always equals the footprint diamond width.
                assert!(sprite_width_matches(size, (dims.x + dims.y) as f32 * 32.0));
            }
            // Rotations 0/2 mirrored, 1/3 not.
            let flips: Vec<bool> = orientations(dims).iter().map(|o| o.2).collect();
            assert_eq!(flips, vec![true, false, true, false]);
        }
    }

    #[test]
    fn square_sprites_show_the_door_next_to_the_entrance() {
        let flips: Vec<bool> = orientations(IVec2::new(2, 2)).iter().map(|o| o.2).collect();
        // South: door wall faces south (exact). West: door on the south wall (adjacent).
        // North: mirrored, door on the east wall (adjacent). East: mirrored (exact).
        assert_eq!(flips, vec![false, false, true, true]);
    }

    #[test]
    fn mirrored_anchor_stays_on_the_front_corner() {
        // A 2×3 sprite (B = 2, T = 3) is 160 px wide with the front corner 96 px from the left.
        let anchor_x = 3.0 / 5.0;
        // Mirrored on a (x: 2, y: 3) footprint the front corner lies 2 · 32 px from the left
        // corner of the footprint diamond.
        let rect = TileRect::new(IVec2::new(10, 10), IVec2::new(2, 3));
        let left = iso::tile_to_world(Vec2::new(10.0, 13.0)).x;
        let front = iso::tile_to_world(Vec2::new(12.0, 13.0)).x;
        assert!(((front - left) - (1.0 - anchor_x) * 160.0).abs() < 1e-3);
        // And the footprint tent splits at the same place.
        let part = Part::on(SpriteId(0), rect, 0.0, true);
        assert!((part.depth().tent[3] - (1.0 - anchor_x)).abs() < 1e-5);
    }

    #[test]
    fn gate_axis_follows_the_wall_line() {
        let ew = Direction::East.mask_bit() | Direction::West.mask_bit();
        let ns = Direction::North.mask_bit() | Direction::South.mask_bit();
        assert_eq!(gate_axis_index(ew, Direction::East), 0);
        assert_eq!(gate_axis_index(ns, Direction::South), 1);
        assert_eq!(gate_axis_index(Direction::West.mask_bit(), Direction::North), 0);
        assert_eq!(gate_axis_index(0, Direction::South), 0);
        assert_eq!(gate_axis_index(0, Direction::West), 1);
        assert_eq!(gate_axis_index(ew | ns, Direction::East), 1);
    }

    #[test]
    fn silhouette_covers_roof_but_not_the_ground_in_front() {
        let part = Part::on(SpriteId(0), TileRect::new(IVec2::new(4, 4), IVec2::new(2, 2)), 0.0, false);
        // Footprint diamond: back (0, 128), front (0, 192), left (-64, 160), right (64, 160).
        assert!(part.silhouette_contains(Vec2::new(0.0, 150.0), 100.0));
        // Above the back corner, inside the sprite height (roof).
        assert!(part.silhouette_contains(Vec2::new(0.0, 100.0), 100.0));
        assert!(!part.silhouette_contains(Vec2::new(0.0, 80.0), 100.0));
        // Below the front corner (ground in front) and beside the footprint.
        assert!(!part.silhouette_contains(Vec2::new(0.0, 200.0), 100.0));
        assert!(!part.silhouette_contains(Vec2::new(70.0, 160.0), 100.0));
        // Front walls are nearer at the front corner than at the sides.
        assert!(part.depth_at(0.0) > part.depth_at(-60.0));
    }
}
