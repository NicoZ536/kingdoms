//! Walkers on the map: citizens and emigrants leaving the kingdom, from the simulation's
//! spatial query ([`GameState::citizens_in`]) – presentation only.
//!
//! * **Culling:** only the chunks around the view are queried; walkers inside buildings are
//!   not listed by the simulation and never drawn.
//! * **Motion:** positions are interpolated between the previous and the current tick with the
//!   engine's fixed-step fraction ([`Time::alpha`](kingdoms_engine::core::time::Time::alpha)),
//!   so walkers glide at any frame rate and stand still while the game is paused.
//! * **Height:** walkers stand on their tile's elevation; within [`RAMP`] tiles of an edge to a
//!   tile of another level the height eases to the middle of both, so steps up and down and the
//!   ends of bridges (deck above the water) are a short climb instead of a jump.
//! * **Depth:** like every upright object, a walker is depth-tested at its ground point, so a
//!   house in front hides it and it hides the house when in front. On a bridge it is drawn at
//!   the bridge's front depth, otherwise the bridge's railings would cover it.
//! * **Art:** the animations of the `characters` atlas, resolved through
//!   [`AssetManager::animation`]: `<role>_<ne|se|sw|nw>_walk` – the walk frame follows the
//!   distance walked, one frame per [`TILES_PER_FRAME`], i.e. 8 frames per second at the base
//!   walking speed of 0.2 tiles per tick, and is [`STAND_FRAME`] (legs together) while
//!   standing. Walkers that act (`CitizenView::action`) play their role's loop at game time
//!   ([`act_frame`], out of step per walker): `<role>_<dir>_work` at a work spot (builders
//!   hammer, farmers hoe) and `<role>_<dir>_attack` in combat (spear, bow, sword, axe,
//!   catapult); roles without the loop stand. Gatherers use the tool of their workplace
//!   ([`GatherTool::of`]): woodcutters the axe of the work loop, quarries and mines
//!   `_pickaxe`, clay and sand pits and foresters `_shovel`, fisheries `_rod`. Carried goods
//!   (`CitizenView::carried`) appear as the good's icon held at the side.
//! * **Shadow:** a soft blob under the feet ([`BlobShadows`]), its opacity by daylight.
//! * **Level of detail:** below [`DETAIL_ZOOM`] walkers show the standing frame, no goods and
//!   no shadow.
//! * **Allegiance:** enemies (`CitizenView::hostile`) stand on a ring in their kingdom's colour
//!   (bandits: dark red) and soldiers of an enemy army are tinted in it; the first fighter of
//!   every enemy group, the cart of every caravan and the first soldier of every own company
//!   carry a banner (`characters/banner`) in their colour (own: gold). Own soldiers stand on a
//!   faint gold ring when zoomed in.
//! * **Crowds:** the fighters of an enemy group walk the same path and share one position in
//!   the simulation; drawn there, an army of 120 would look like one man. Figures of a group on
//!   the same spot are spread on a sunflower spiral around it ([`crowd_offset`], ranked by
//!   walker id), avoiding buildings and water, so the size of a group can be read on the map.
//!
//! The walkers drawn last are kept for picking ([`CitizenLayer::pick`]).

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::{Animation, AssetManager, SpriteId, SpriteRegion};
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, OBJECT_DEPTH_BIAS, SpriteDepth};
use kingdoms_engine::rendering::sprite::{RenderList, SpriteInstance};
use kingdoms_sim::defs::{BuildingDef, Defs, Gathering};
use kingdoms_sim::grid::{Direction, TileRect};
use std::collections::{BTreeMap, BTreeSet};

use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::population::{self, CitizenId, CitizenRole, CitizenView, Destination, Purpose, SubPos, TravelerId, WalkerAction, WalkerId};
use kingdoms_sim::state::GameState;
use kingdoms_sim::world::DepositKind;

use super::HIGHLIGHT_BIAS;
use super::kingdoms::kingdom_rgba;
use super::shadows::BlobShadows;
use crate::ui::theme::{BANDIT_COLOR, PLAYER_COLOR};

/// Sprite roles in the order of [`role_index`].
pub const ROLES: [CitizenRole; 13] = [
    CitizenRole::Man,
    CitizenRole::Woman,
    CitizenRole::Child,
    CitizenRole::Carrier,
    CitizenRole::Builder,
    CitizenRole::Farmer,
    CitizenRole::Soldier,
    CitizenRole::Merchant,
    CitizenRole::Cart,
    CitizenRole::Archer,
    CitizenRole::Knight,
    CitizenRole::Siege,
    CitizenRole::Raider,
];
/// Facings in the order of [`dir_index`].
const FACINGS: [Direction; 4] = [Direction::North, Direction::East, Direction::South, Direction::West];
/// Walk frames per direction.
const FRAMES: usize = 4;
/// Tiles walked per animation frame: 8 frames per second at the base walking speed of
/// 0.2 tiles per tick and 20 ticks per second (docs/game_design.md §2, docs/assets.md).
pub const TILES_PER_FRAME: f32 = 0.5;
/// Walk frame shown while standing: the frame with both legs together.
pub const STAND_FRAME: usize = 1;
/// Distance from a tile edge (tiles) over which the height eases towards the neighbour.
pub const RAMP: f32 = 0.35;
/// Height of a bridge deck above its water tile in elevation levels (6 px of 16, the deck of
/// `roads/bridge_<x|y>` in tools/assetgen).
pub const BRIDGE_DECK: f32 = 6.0 / iso::ELEVATION_STEP;
/// Neighbours whose height differs by more than this are cliffs, not steps.
const MAX_STEP: f32 = 1.5;
/// Below this zoom walkers are drawn without walk animation and carried goods.
pub const DETAIL_ZOOM: f32 = 0.45;
/// Edge of the carried goods icon in world pixels.
const GOODS_SIZE: f32 = 10.0;
/// Smallest clickable size of a walker on screen in pixels.
const PICK_MIN_SCREEN: f32 = 16.0;
/// Extra depth of walkers on a bridge: in front of the bridge's railings.
const BRIDGE_BIAS: f32 = 0.03;
/// Spacing of the figures of a crowd in tiles (radius step of the spiral; neighbours end up
/// about 1.8 × this apart).
const CROWD_SPACING: f32 = 0.2;

/// Offset in tiles of the `rank`-th figure of a group standing on one spot: rank 0 stays on
/// the spot, the others follow a sunflower spiral (golden angle), so `n` figures fill a disc of
/// radius ≈ `CROWD_SPACING`·√n evenly.
pub fn crowd_offset(rank: usize) -> Vec2 {
    if rank == 0 {
        return Vec2::ZERO;
    }
    const GOLDEN_ANGLE: f32 = 2.399_963;
    let k = rank as f32;
    Vec2::from_angle(k * GOLDEN_ANGLE) * (CROWD_SPACING * k.sqrt())
}

/// Festival-goers drawn at most in front of one venue.
pub const FESTIVAL_CROWD_MAX: usize = 60;
/// Spacing of the festival crowd in tiles (looser than a marching group).
const FESTIVAL_SPACING: f32 = 0.32;

/// Citizens celebrating inside a venue (their leisure destination) during a festival, per
/// venue in building order, citizens in id order, at most [`FESTIVAL_CROWD_MAX`] each.
pub fn festival_guests(state: &GameState) -> Vec<(BuildingId, Vec<CitizenId>)> {
    let mut venues: Vec<(BuildingId, Vec<CitizenId>)> = Vec::new();
    for (id, c) in state.population().citizens() {
        let m = c.motion();
        let (Some(inside), Some(Destination::Building { building, purpose: Purpose::Leisure })) = (m.inside(), m.destination()) else {
            continue;
        };
        if inside != building {
            continue;
        }
        match venues.iter_mut().find(|(v, _)| *v == inside) {
            Some((_, guests)) if guests.len() < FESTIVAL_CROWD_MAX => guests.push(id),
            Some(_) => {}
            None => venues.push((inside, vec![id])),
        }
    }
    venues.sort_by_key(|(v, _)| v.index());
    venues
}

/// Place of the `rank`-th festival-goer around `entrance` (tile coordinates of its centre): the
/// next free point (`free`: no building, no water) of a loose sunflower spiral, so no two
/// figures share a spot; `None` if the spiral finds no room nearby.
pub fn festival_spot(entrance: Vec2, rank: usize, free: impl Fn(Vec2) -> bool) -> Option<Vec2> {
    const GOLDEN_ANGLE: f32 = 2.399_963;
    // Rank r owns the spiral points 2r and 2r + 1, and further out 2r + k·2·MAX: the first
    // free one is its place (deterministic; no two ranks share a point).
    let own = [rank * 2, rank * 2 + 1].into_iter().chain((1..12).map(|k| rank * 2 + k * 2 * FESTIVAL_CROWD_MAX));
    own.into_iter().find_map(|i| {
        let k = i as f32;
        let p = entrance + Vec2::from_angle(k * GOLDEN_ANGLE) * (FESTIVAL_SPACING * k.sqrt());
        free(p).then_some(p)
    })
}

/// The facing that looks most nearly along `d` (tile axes).
fn facing_towards(d: Vec2) -> Direction {
    if d.x.abs() >= d.y.abs() {
        if d.x >= 0.0 { Direction::East } else { Direction::West }
    } else if d.y >= 0.0 {
        Direction::South
    } else {
        Direction::North
    }
}

/// Facing of a walker standing at `pos` (tile space) that attacked in the current combat round:
/// towards the nearest point of its target (`None`: no target, or it stands on it).
pub fn attack_facing(state: &GameState, id: WalkerId, pos: Vec2) -> Option<Direction> {
    let target = state.military().last_round().target_of(id)?;
    let min = target.origin.as_vec2();
    let d = pos.clamp(min, min + target.size.max(IVec2::ONE).as_vec2()) - pos;
    (d.length_squared() > 1e-4).then(|| facing_towards(d))
}

/// Tool a gatherer works with at its spot, by the way its workplace gathers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GatherTool {
    /// Woodcutters (and anyone without a gathering of their own): the role's work loop.
    Axe,
    /// Quarries and mines.
    Pickaxe,
    /// Clay pits, sand pits and foresters planting saplings.
    Shovel,
    /// Fisheries.
    Rod,
}

impl GatherTool {
    /// Tools with a loop of their own, in the order of [`RoleArt::gather`].
    const LOOPS: [GatherTool; 3] = [GatherTool::Pickaxe, GatherTool::Shovel, GatherTool::Rod];

    /// Tool of the workers of `def`.
    pub fn of(def: &BuildingDef) -> GatherTool {
        match def.production.as_ref().and_then(|p| p.gathering.as_ref()) {
            Some(Gathering::Deposit { kind: DepositKind::Fish, .. }) => GatherTool::Rod,
            Some(Gathering::Deposit { kind: DepositKind::Clay | DepositKind::Sand, .. } | Gathering::PlantTrees { .. }) => GatherTool::Shovel,
            Some(Gathering::Deposit { .. }) => GatherTool::Pickaxe,
            _ => GatherTool::Axe,
        }
    }

    /// Loop kind in `characters/<role>_<dir>_<kind>` (`None`: the role's work loop).
    pub const fn loop_kind(self) -> Option<&'static str> {
        match self {
            GatherTool::Axe => None,
            GatherTool::Pickaxe => Some("pickaxe"),
            GatherTool::Shovel => Some("shovel"),
            GatherTool::Rod => Some("rod"),
        }
    }

    /// Index in [`GatherTool::LOOPS`].
    const fn loop_index(self) -> Option<usize> {
        match self {
            GatherTool::Axe => None,
            GatherTool::Pickaxe => Some(0),
            GatherTool::Shovel => Some(1),
            GatherTool::Rod => Some(2),
        }
    }
}

/// Index of `role` in [`ROLES`].
pub const fn role_index(role: CitizenRole) -> usize {
    match role {
        CitizenRole::Man => 0,
        CitizenRole::Woman => 1,
        CitizenRole::Child => 2,
        CitizenRole::Carrier => 3,
        CitizenRole::Builder => 4,
        CitizenRole::Farmer => 5,
        CitizenRole::Soldier => 6,
        CitizenRole::Merchant => 7,
        CitizenRole::Cart => 8,
        CitizenRole::Archer => 9,
        CitizenRole::Knight => 10,
        CitizenRole::Siege => 11,
        CitizenRole::Raider => 12,
    }
}

/// Role whose art stands in for `role` if the characters atlas has no sprites of its own for it
/// (an atlas from before siege engines and bandits were drawn, docs/assets.md).
pub const fn stand_in(role: CitizenRole) -> CitizenRole {
    match role {
        CitizenRole::Siege => CitizenRole::Soldier,
        CitizenRole::Raider => CitizenRole::Man,
        other => other,
    }
}

/// Sprite role key used for `role`: its own when the atlas has its first walk frame, else the
/// key of its [`stand_in`].
fn sprite_role_key(assets: &AssetManager, role: CitizenRole) -> &'static str {
    let own = role.sprite_key();
    if assets.has_sprite(&format!("characters/{own}_{}_0", FACINGS[0].sprite_dir())) {
        return own;
    }
    let fallback = stand_in(role).sprite_key();
    log::info!("characters atlas has no '{own}' walker sprites; drawing them as '{fallback}'");
    fallback
}

/// The four walk frames of `key` facing `dir` from the animation `characters/<key>_<dir>_walk`
/// (the frame names `<key>_<dir>_<0..3>` if an atlas lacks it).
pub fn walk_frames(assets: &AssetManager, key: &str, dir: &str) -> [SpriteId; FRAMES] {
    let name = format!("characters/{key}_{dir}_walk");
    if let Some(anim) = assets.animation(&name).and_then(|id| assets.animation_data(id))
        && anim.frames.len() == FRAMES
    {
        return std::array::from_fn(|f| anim.frames[f]);
    }
    std::array::from_fn(|f| assets.sprite(&format!("characters/{key}_{dir}_{f}")))
}

/// The optional loop `characters/<key>_<dir>_<kind>` (`work` or `attack`).
fn act_loop(assets: &AssetManager, key: &str, dir: &str, kind: &str) -> Option<Animation> {
    let id = assets.try_animation(&format!("characters/{key}_{dir}_{kind}"))?;
    assets.animation_data(id).filter(|a| !a.frames.is_empty()).cloned()
}

/// Frame of a work or attack loop at game time `seconds` for walker `key`: every walker starts
/// at its own point of the loop, so a crew does not swing in step.
pub fn act_frame(anim: &Animation, seconds: f32, key: u64) -> SpriteId {
    let offset = (key % 997) as f32 / 997.0 * anim.duration();
    anim.frame_at(seconds.max(0.0) + offset)
}

/// Stable number of a walker for per-walker variation.
fn walker_key(id: WalkerId) -> u64 {
    match id {
        WalkerId::Citizen(c) => u64::from(c.index()) * 2,
        WalkerId::Traveler(t) => u64::from(t.index()) * 2 + 1,
    }
}

/// Width of a walker's ground shadow in world pixels.
fn shadow_width(role: CitizenRole) -> f32 {
    match role {
        CitizenRole::Cart | CitizenRole::Siege => 34.0,
        CitizenRole::Knight => 16.0,
        CitizenRole::Child => 10.0,
        _ => 13.0,
    }
}

/// Index of a facing in [`FACINGS`]: the sprite directions `ne`, `se`, `sw`, `nw`.
pub const fn dir_index(facing: Direction) -> usize {
    match facing {
        Direction::North => 0,
        Direction::East => 1,
        Direction::South => 2,
        Direction::West => 3,
    }
}

/// Position in tiles between the previous (`alpha` 0) and the current tick (`alpha` 1). A jump
/// of more than a tile (a walker appearing at a door) is not interpolated.
pub fn interpolate(prev: SubPos, pos: SubPos, alpha: f32) -> Vec2 {
    let (px, py) = prev.to_tiles();
    let (x, y) = pos.to_tiles();
    if (x - px).abs() + (y - py).abs() > 1.5 {
        return Vec2::new(x, y);
    }
    let a = alpha.clamp(0.0, 1.0);
    Vec2::new(px + (x - px) * a, py + (y - py) * a)
}

/// Walk frame at tile position `p` for a walker facing `facing`: one frame per
/// [`TILES_PER_FRAME`] along the walking direction, [`STAND_FRAME`] when standing.
pub fn walk_frame(p: Vec2, facing: Direction, walking: bool) -> usize {
    if !walking {
        return STAND_FRAME;
    }
    let along = match facing {
        Direction::East => p.x,
        Direction::West => -p.x,
        Direction::South => p.y,
        Direction::North => -p.y,
    };
    ((along / TILES_PER_FRAME).floor() as i64).rem_euclid(FRAMES as i64) as usize
}

/// Height in elevation levels of a walker at tile-space point `p`. `height(tile)` is the
/// walkable surface of a tile (`None` outside the map). Within [`RAMP`] tiles of an edge to a
/// neighbour of another height (at most [`MAX_STEP`] levels apart) the height eases linearly to
/// the middle of both, which is continuous across the edge.
pub fn ground_height(p: Vec2, height: impl Fn(IVec2) -> Option<f32>) -> f32 {
    let tile = p.floor().as_ivec2();
    let Some(h) = height(tile) else { return 0.0 };
    let f = p - tile.as_vec2();
    let edge = |offset: IVec2, distance: f32| -> f32 {
        if distance >= RAMP {
            return 0.0;
        }
        match height(tile + offset) {
            Some(n) if (n - h).abs() <= MAX_STEP => (n - h) * 0.5 * (1.0 - distance / RAMP),
            _ => 0.0,
        }
    };
    h + edge(IVec2::NEG_X, f.x) + edge(IVec2::X, 1.0 - f.x) + edge(IVec2::NEG_Y, f.y) + edge(IVec2::Y, 1.0 - f.y)
}

/// Walkable surface height of `tile`: its elevation, plus the deck on bridges.
pub fn surface_height(state: &GameState, tile: IVec2) -> Option<f32> {
    let t = state.world().get(tile)?;
    let bridge = state.roads().get(tile).is_some_and(|r| r.is_bridge());
    Some(f32::from(t.elevation) + if bridge { BRIDGE_DECK } else { 0.0 })
}

/// Where a walker stands this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Ground position in tiles.
    pub tile_pos: Vec2,
    /// Height in elevation levels.
    pub height: f32,
    /// World position of the feet (the sprite anchor).
    pub world: Vec2,
    /// Depth of the upright sprite.
    pub depth: SpriteDepth,
}

/// Interpolated placement of walker `v` at tick fraction `alpha`.
pub fn place(state: &GameState, v: &CitizenView, alpha: f32) -> Placement {
    place_at(state, interpolate(v.prev_pos, v.pos, alpha))
}

/// Placement of a walker `offset` tiles beside its interpolated position (a figure of a
/// crowd); the offset shrinks where it would put the figure into a building or the water.
pub fn place_in_crowd(state: &GameState, v: &CitizenView, alpha: f32, offset: Vec2) -> Placement {
    place_at(state, crowd_position(state, interpolate(v.prev_pos, v.pos, alpha), offset))
}

/// Where a figure of a crowd stands (tile space): `offset` tiles beside `base`, shortened where
/// the full offset would put it into a building or the water (see [`place_in_crowd`]).
pub fn crowd_position(state: &GameState, base: Vec2, offset: Vec2) -> Vec2 {
    if offset == Vec2::ZERO {
        return base;
    }
    let free = |p: Vec2| {
        let tile = p.floor().as_ivec2();
        state.buildings().at(tile).is_none() && state.world().get(tile).is_some_and(|t| !t.terrain.is_water())
    };
    base + [1.0, 0.5, 0.25].into_iter().map(|f| offset * f).find(|o| free(base + *o)).unwrap_or(Vec2::ZERO)
}

fn place_at(state: &GameState, tile_pos: Vec2) -> Placement {
    let height = ground_height(tile_pos, |t| surface_height(state, t));
    let tile = tile_pos.floor().as_ivec2();
    let depth = if state.roads().get(tile).is_some_and(|r| r.is_bridge()) {
        SpriteDepth::upright_point((tile + IVec2::ONE).as_vec2(), BRIDGE_BIAS)
    } else {
        SpriteDepth::upright_point(tile_pos, OBJECT_DEPTH_BIAS)
    };
    Placement { tile_pos, height, world: iso::tile_to_world_elevated(tile_pos, height), depth }
}

/// World position of the feet of a walker between its positions of the last two ticks (`prev`,
/// `pos`) at tick fraction `alpha`: where the map draws it (without a crowd offset).
pub fn walker_world_position(state: &GameState, prev: SubPos, pos: SubPos, alpha: f32) -> Vec2 {
    place_at(state, interpolate(prev, pos, alpha)).world
}

/// World position of a citizen's feet at tick fraction `alpha` (the door while inside a
/// building), for the follow camera.
pub fn citizen_world_position(state: &GameState, defs: &Defs, citizen: CitizenId, alpha: f32) -> Option<Vec2> {
    population::citizen_view(state, defs, citizen).map(|v| place(state, &v, alpha).world)
}

/// A walker drawn this frame (for picking).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawnWalker {
    pub id: WalkerId,
    /// World bounds of the sprite.
    pub bounds: Rect,
    /// Depth of the sprite.
    pub depth: f32,
}

/// The frontmost walker whose sprite (grown to at least `min_size` world pixels) contains
/// world point `w`, with its depth.
pub fn pick_walker(drawn: &[DrawnWalker], w: Vec2, min_size: f32) -> Option<(WalkerId, f32)> {
    drawn
        .iter()
        .filter(|d| {
            let grow = ((Vec2::splat(min_size) - d.bounds.size()) * 0.5).max(Vec2::ZERO);
            Rect::new(d.bounds.min - grow, d.bounds.max + grow).contains(w)
        })
        .max_by(|a, b| a.depth.total_cmp(&b.depth))
        .map(|d| (d.id, d.depth))
}

/// Statistics of the last frame for the debug window.
#[derive(Clone, Copy, Debug, Default)]
pub struct CitizenStats {
    /// Walkers the spatial query returned for the view's chunks.
    pub queried: usize,
    /// Walkers drawn (on screen).
    pub drawn: usize,
    /// Of these, carrying goods.
    pub carrying: usize,
    /// Drawn with the walk animation and goods (zoomed in enough).
    pub detailed: bool,
}

/// Emphasis of walkers this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkerMarks {
    /// Under the cursor.
    pub hovered: Option<WalkerId>,
    /// Shown in the citizen window (or followed).
    pub selected: Option<CitizenId>,
}

/// Whose side a walker is on, for its ring, tint and banner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Allegiance {
    /// Colour of ring and banner.
    pub color: Color,
    /// Tint of the sprite (white: none).
    pub tint: Color,
    /// Carries the group's banner.
    pub banner: bool,
    /// Enemy of the player (ring always shown).
    pub hostile: bool,
    /// Offset in tiles from its position: its place in a crowd of its group on one spot.
    pub offset: Vec2,
}

fn srgb(c: [u8; 3]) -> Color {
    Color::from_srgb_u8(c[0], c[1], c[2], 255)
}

/// Sprite tint of an enemy army's soldier: white mixed with 45 % of the kingdom colour.
pub fn army_tint(color: u8) -> Color {
    let c = kingdom_rgba(color, 1.0);
    let mix = |v: f32| 0.55 + 0.45 * v;
    Color::rgb(mix(c.r), mix(c.g), mix(c.b))
}

/// Allegiance of the travellers (enemies, caravans) and the banner bearers of the own
/// companies this frame.
#[derive(Default)]
pub struct Allegiances {
    travelers: BTreeMap<TravelerId, Allegiance>,
    leaders: BTreeSet<CitizenId>,
}

impl Allegiances {
    pub fn of(state: &GameState) -> Self {
        let mut travelers = BTreeMap::new();
        let diplomacy = state.diplomacy();
        let walking = state.population().walking();
        for w in state.military().warbands() {
            let (color, tint) = match w.kingdom().and_then(|k| diplomacy.kingdom(k)) {
                Some(k) => (kingdom_rgba(k.color(), 1.0), army_tint(k.color())),
                None => (srgb(BANDIT_COLOR), Color::WHITE),
            };
            // Fighters on the same spot (now and last tick), ranked by walker id.
            let mut spots: Vec<((i32, i32, i32, i32), TravelerId)> = w
                .fighters()
                .iter()
                .filter_map(|f| walking.traveler(f.walker()).map(|t| (t.motion().pos(), t.motion().prev_pos(), f.walker())))
                .map(|(pos, prev, id)| ((pos.x, pos.y, prev.x, prev.y), id))
                .collect();
            spots.sort();
            let mut ranks: BTreeMap<TravelerId, usize> = BTreeMap::new();
            let mut first = 0;
            for (i, (spot, id)) in spots.iter().enumerate() {
                if i > 0 && spots[i - 1].0 != *spot {
                    first = i;
                }
                ranks.insert(*id, i - first);
            }
            for (i, f) in w.fighters().iter().enumerate() {
                let offset = crowd_offset(ranks.get(&f.walker()).copied().unwrap_or(0));
                travelers.insert(f.walker(), Allegiance { color, tint, banner: i == 0, hostile: true, offset });
            }
        }
        for c in diplomacy.caravans() {
            let color = diplomacy.kingdom(c.kingdom).map_or(Color::WHITE, |k| kingdom_rgba(k.color(), 1.0));
            for (i, w) in c.walkers.iter().enumerate() {
                // The cart (second walker) carries the banner; a lone merchant carries it himself.
                let banner = i == 1 || c.walkers.len() == 1;
                travelers.insert(*w, Allegiance { color, tint: Color::WHITE, banner, hostile: false, offset: Vec2::ZERO });
            }
        }
        let leaders = state.military().companies().iter().filter_map(|c| c.members().first().copied()).collect();
        Self { travelers, leaders }
    }

    /// Offset of walker `id` from its position: its place in the crowd of its band or army
    /// (zero for everybody else).
    pub fn crowd_offset_of(&self, id: WalkerId) -> Vec2 {
        match id {
            WalkerId::Traveler(t) => self.travelers.get(&t).map_or(Vec2::ZERO, |a| a.offset),
            WalkerId::Citizen(_) => Vec2::ZERO,
        }
    }

    /// The allegiance of walker `v`.
    pub fn get(&self, v: &CitizenView) -> Option<Allegiance> {
        match v.id {
            WalkerId::Traveler(t) => self.travelers.get(&t).copied(),
            WalkerId::Citizen(c) => {
                let soldier = matches!(v.role, CitizenRole::Soldier | CitizenRole::Archer | CitizenRole::Knight | CitizenRole::Siege);
                soldier.then(|| Allegiance {
                    color: srgb(PLAYER_COLOR),
                    tint: Color::WHITE,
                    banner: self.leaders.contains(&c),
                    hostile: false,
                    offset: Vec2::ZERO,
                })
            }
        }
    }
}

/// Resolved animations of one role.
struct RoleArt {
    /// `[dir][frame]` of the walk cycle.
    walk: [[SpriteId; FRAMES]; 4],
    /// Work and attack loops per direction, if the role has them.
    work: [Option<Animation>; 4],
    attack: [Option<Animation>; 4],
    /// Gatherer loops per tool of [`GatherTool::LOOPS`] and direction, if the role has them.
    gather: [[Option<Animation>; 4]; 3],
}

impl RoleArt {
    /// The loop of a walker standing at work with `tool` facing `d`: the tool's own loop, else
    /// the role's work loop.
    fn work_loop(&self, tool: GatherTool, d: usize) -> Option<&Animation> {
        tool.loop_index().and_then(|i| self.gather[i][d].as_ref()).or(self.work[d].as_ref())
    }
}

/// What the layer draws in one frame besides the state.
#[derive(Clone, Copy, Debug)]
pub struct WalkerFrame {
    /// Fraction of the current tick (interpolation).
    pub alpha: f32,
    /// Game time in seconds of animation time (work and attack loops).
    pub seconds: f32,
    pub marks: WalkerMarks,
    /// Opacity of the ground shadows (daylight).
    pub shadow_alpha: f32,
}

/// Resolved art of the walkers.
pub struct CitizenLayer {
    /// Per role in [`ROLES`] order.
    art: Vec<RoleArt>,
    /// Gatherer tool of the workers of every building type (by index).
    tools: Vec<GatherTool>,
    /// Icon of every resource (by index), if the icons atlas has it.
    goods: Vec<Option<SpriteRegion>>,
    marker: SpriteRegion,
    /// Banner of groups (`characters/banner`), if the atlas has it.
    banner: Option<SpriteRegion>,
    drawn: Vec<DrawnWalker>,
    stats: CitizenStats,
}

impl CitizenLayer {
    pub fn new(defs: &Defs, assets: &AssetManager) -> Self {
        let art = ROLES
            .iter()
            .map(|&role| {
                let key = sprite_role_key(assets, role);
                let dir = |d: usize| FACINGS[d].sprite_dir();
                RoleArt {
                    walk: std::array::from_fn(|d| walk_frames(assets, key, dir(d))),
                    work: std::array::from_fn(|d| act_loop(assets, key, dir(d), "work")),
                    attack: std::array::from_fn(|d| act_loop(assets, key, dir(d), "attack")),
                    gather: std::array::from_fn(|t| {
                        let kind = GatherTool::LOOPS[t].loop_kind().unwrap_or("work");
                        std::array::from_fn(|d| act_loop(assets, key, dir(d), kind))
                    }),
                }
            })
            .collect();
        let tools = defs.buildings().iter().map(GatherTool::of).collect();
        let goods = defs
            .resources()
            .iter()
            .map(|r| assets.try_sprite(&format!("icons/res_{}", r.key)).map(|id| *assets.region(id)))
            .collect();
        let marker = *assets.region(assets.sprite("overlay/soft_dot"));
        let banner = assets.try_sprite("characters/banner").map(|id| *assets.region(id));
        Self { art, tools, goods, marker, banner, drawn: Vec::new(), stats: CitizenStats::default() }
    }

    pub fn stats(&self) -> CitizenStats {
        self.stats
    }

    /// The frontmost citizen under world point `w` drawn last frame, with its depth (camera
    /// `zoom` sets the smallest clickable size). Travellers leaving the kingdom are not
    /// pickable.
    pub fn pick(&self, w: Vec2, zoom: f32) -> Option<(CitizenId, f32)> {
        let citizens: Vec<DrawnWalker> = self.drawn.iter().filter(|d| matches!(d.id, WalkerId::Citizen(_))).copied().collect();
        match pick_walker(&citizens, w, PICK_MIN_SCREEN / zoom.max(0.05))? {
            (WalkerId::Citizen(id), depth) => Some((id, depth)),
            (WalkerId::Traveler(_), _) => None,
        }
    }

    /// Queues the walkers in the view at tick fraction `frame.alpha`, with their shadows.
    pub fn draw(
        &mut self,
        list: &mut RenderList,
        state: &GameState,
        defs: &Defs,
        assets: &AssetManager,
        shadows: &BlobShadows,
        frame: WalkerFrame,
    ) {
        let (alpha, marks) = (frame.alpha, frame.marks);
        self.drawn.clear();
        let view = list.visible_rect();
        let detailed = list.camera.zoom >= DETAIL_ZOOM;
        let mut stats = CitizenStats { detailed, ..CitizenStats::default() };
        let rect = view_tiles(&view, state.world().size());
        let sides = Allegiances::of(state);
        for v in state.citizens_in(defs, rect) {
            stats.queried += 1;
            let side = sides.get(&v);
            let at = match side.map(|s| s.offset) {
                Some(offset) if offset != Vec2::ZERO => place_in_crowd(state, &v, alpha, offset),
                _ => place(state, &v, alpha),
            };
            let walking = detailed && v.moving && v.pos != v.prev_pos;
            let art = &self.art[role_index(v.role)];
            // A figure drawn at its place in a crowd turns to its target from where it stands.
            let facing = match (v.action, side.map(|s| s.offset)) {
                (WalkerAction::Attack, Some(offset)) if offset != Vec2::ZERO => attack_facing(state, v.id, at.tile_pos).unwrap_or(v.facing),
                _ => v.facing,
            };
            let d = dir_index(facing);
            let act = match v.action {
                _ if !detailed || walking => None,
                WalkerAction::None => None,
                WalkerAction::Work => art.work_loop(self.tool_of(state, v.id), d),
                WalkerAction::Attack => art.attack[d].as_ref(),
            };
            let sprite = match act {
                Some(anim) => act_frame(anim, frame.seconds, walker_key(v.id)),
                None => art.walk[d][walk_frame(at.tile_pos, facing, walking)],
            };
            let r = assets.region(sprite);
            let mut instance = SpriteInstance::new(at.world, r.size, r.anchor, r.uv, at.depth);
            let bounds = instance.world_bounds();
            if !bounds.intersects(&view) {
                continue;
            }
            if detailed {
                shadows.draw(list, at.tile_pos, at.height, shadow_width(v.role), frame.shadow_alpha);
            }
            if let Some(s) = side {
                instance = instance.with_color(s.tint);
                if s.hostile || detailed {
                    self.draw_ring(list, &at, s.color, if s.hostile { 0.9 } else { 0.45 });
                }
                if s.banner {
                    self.draw_banner(list, &at, s.color, v.role == CitizenRole::Cart);
                }
            }
            list.draw(r.texture, instance);
            stats.drawn += 1;
            let selected = matches!(v.id, WalkerId::Citizen(c) if Some(c) == marks.selected);
            if selected || marks.hovered == Some(v.id) {
                self.draw_marker(list, &at, selected);
            }
            if let Some((resource, _)) = v.carried {
                stats.carrying += 1;
                if detailed {
                    self.draw_goods(list, resource.index(), v.facing, &at);
                }
            }
            self.drawn.push(DrawnWalker { id: v.id, bounds, depth: at.depth.tent[1] });
        }
        if state.events().festival_active(state.clock().tick) {
            self.draw_festival(list, state, defs, assets, shadows, frame, detailed, &mut stats);
        }
        self.stats = stats;
    }

    /// Tool of walker `id` at its work spot: by its workplace's gathering (citizens only).
    fn tool_of(&self, state: &GameState, id: WalkerId) -> GatherTool {
        let WalkerId::Citizen(c) = id else { return GatherTool::Axe };
        state
            .population()
            .citizen(c)
            .and_then(|c| c.job())
            .and_then(|job| state.building(job.building))
            .and_then(|b| self.tools.get(b.kind().index()).copied())
            .unwrap_or(GatherTool::Axe)
    }

    /// Festival-goers celebrating inside their venue (church or market; the simulation keeps
    /// them inside like any visitor) as a crowd in front of its entrance: on the sunflower
    /// spiral of [`festival_spot`], facing the venue, every other one stepping in place. They
    /// are the real citizens (pickable, at most [`FESTIVAL_CROWD_MAX`] per venue).
    #[allow(clippy::too_many_arguments)]
    fn draw_festival(
        &mut self,
        list: &mut RenderList,
        state: &GameState,
        defs: &Defs,
        assets: &AssetManager,
        shadows: &BlobShadows,
        frame: WalkerFrame,
        detailed: bool,
        stats: &mut CitizenStats,
    ) {
        let view = list.visible_rect();
        for (venue, guests) in festival_guests(state) {
            let Some(b) = state.building(venue) else { continue };
            let centre = b.rect().origin.as_vec2() + b.rect().size.as_vec2() * 0.5;
            let entrance = b.entrance().as_vec2() + Vec2::splat(0.5);
            let free = |p: Vec2| {
                let tile = p.floor().as_ivec2();
                state.buildings().at(tile).is_none() && state.world().get(tile).is_some_and(|t| !t.terrain.is_water())
            };
            for (rank, id) in guests.into_iter().enumerate() {
                let Some(c) = state.population().citizen(id) else { continue };
                let Some(spot) = festival_spot(entrance, rank, free) else { continue };
                let at = place_at(state, spot);
                let facing = facing_towards(centre - spot);
                let art = &self.art[role_index(population::role_of(state, defs, c))];
                // Half of the crowd dances (steps in place), out of step with each other.
                let key = u64::from(id.index());
                let dancing = detailed && key % 2 == 0;
                let step = if dancing { ((frame.seconds * 4.0) as usize + key as usize) % FRAMES } else { STAND_FRAME };
                let r = assets.region(art.walk[dir_index(facing)][step]);
                let instance = SpriteInstance::new(at.world, r.size, r.anchor, r.uv, at.depth);
                let bounds = instance.world_bounds();
                if !bounds.intersects(&view) {
                    continue;
                }
                if detailed {
                    shadows.draw(list, at.tile_pos, at.height, shadow_width(CitizenRole::Man), frame.shadow_alpha);
                }
                list.draw(r.texture, instance);
                stats.drawn += 1;
                let walker = WalkerId::Citizen(id);
                if frame.marks.selected == Some(id) || frame.marks.hovered == Some(walker) {
                    self.draw_marker(list, &at, frame.marks.selected == Some(id));
                }
                self.drawn.push(DrawnWalker { id: walker, bounds, depth: at.depth.tent[1] });
            }
        }
    }

    /// The good's icon held at the walker's side: in front of the body in the front views,
    /// behind it in the back views.
    fn draw_goods(&self, list: &mut RenderList, resource: usize, facing: Direction, at: &Placement) {
        let Some(Some(icon)) = self.goods.get(resource) else { return };
        let (offset, depth_shift) = goods_offset(facing);
        let mut depth = at.depth;
        for d in &mut depth.tent[..3] {
            *d += depth_shift;
        }
        let instance = SpriteInstance::new(at.world + offset, Vec2::splat(GOODS_SIZE), Vec2::splat(0.5), icon.uv, depth);
        list.draw(icon.texture, instance);
    }

    /// A coloured ring under the feet (allegiance).
    fn draw_ring(&self, list: &mut RenderList, at: &Placement, color: Color, alpha: f32) {
        let depth = SpriteDepth::ground(at.tile_pos, HIGHLIGHT_BIAS - 0.003);
        let c = Color::rgba(color.r, color.g, color.b, alpha);
        let instance = SpriteInstance::new(at.world, Vec2::new(16.0, 8.0), self.marker.anchor, self.marker.uv, depth).with_color(c);
        list.draw_transparent(self.marker.texture, instance);
    }

    /// The group's banner held up behind the walker (on a cart: at its back).
    fn draw_banner(&self, list: &mut RenderList, at: &Placement, color: Color, cart: bool) {
        let Some(b) = self.banner else { return };
        let offset = if cart { Vec2::new(-10.0, -14.0) } else { Vec2::new(5.0, -2.0) };
        let mut depth = at.depth;
        for d in &mut depth.tent[..3] {
            *d -= 0.002;
        }
        let instance = SpriteInstance::new(at.world + offset, b.size, b.anchor, b.uv, depth).with_color(color);
        list.draw(b.texture, instance);
    }

    /// A soft ring under the feet of the hovered or inspected walker.
    fn draw_marker(&self, list: &mut RenderList, at: &Placement, selected: bool) {
        let color = if selected { Color::rgba(1.0, 0.82, 0.3, 0.95) } else { Color::rgba(1.0, 1.0, 1.0, 0.7) };
        let depth = SpriteDepth::ground(at.tile_pos, HIGHLIGHT_BIAS);
        let size = Vec2::new(20.0, 10.0);
        let instance = SpriteInstance::new(at.world, size, self.marker.anchor, self.marker.uv, depth).with_color(color);
        list.draw_transparent(self.marker.texture, instance);
    }
}

/// Offset of the carried goods icon from the feet (world pixels) and its depth shift relative
/// to the walker: held at the side towards which the walker faces on screen, in front of the
/// body when it faces the viewer (`se`, `sw`), behind it otherwise.
pub fn goods_offset(facing: Direction) -> (Vec2, f32) {
    match facing {
        Direction::East => (Vec2::new(4.0, -9.0), 0.002),
        Direction::South => (Vec2::new(-4.0, -9.0), 0.002),
        Direction::North => (Vec2::new(4.0, -13.0), -0.002),
        Direction::West => (Vec2::new(-4.0, -13.0), -0.002),
    }
}

/// Tiles whose walkers may appear in the world rectangle `view` (walkers stand at most a
/// sprite's height below the view's top edge; elevated ground is lifted into view).
pub fn view_tiles(view: &Rect, map_size: IVec2) -> TileRect {
    let grown = Rect::new(view.min - Vec2::new(16.0, 8.0), view.max + Vec2::new(16.0, 40.0));
    let (lo, hi) = super::world_view::visible_tile_range(&grown, map_size);
    TileRect::new(lo, (hi - lo).max(IVec2::ZERO))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_moves_from_the_previous_to_the_current_tick() {
        let prev = SubPos { x: 10 * 256 + 128, y: 5 * 256 + 128 };
        let pos = SubPos { x: 10 * 256 + 179, y: 5 * 256 + 128 };
        assert_eq!(interpolate(prev, pos, 0.0), Vec2::new(10.5, 5.5));
        assert!((interpolate(prev, pos, 1.0) - Vec2::new(10.0 + 179.0 / 256.0, 5.5)).length() < 1e-6);
        let half = interpolate(prev, pos, 0.5);
        assert!((half.x - (10.5 + 25.5 / 256.0)).abs() < 1e-5 && (half.y - 5.5).abs() < 1e-6);
        // Out-of-range fractions are clamped; a standing walker stays put.
        assert_eq!(interpolate(prev, pos, 7.0), interpolate(prev, pos, 1.0));
        assert_eq!(interpolate(pos, pos, 0.3), interpolate(pos, pos, 0.9));
        // A walker appearing at a door far away is not dragged across the map.
        let door = SubPos { x: 40 * 256, y: 2 * 256 };
        assert_eq!(interpolate(prev, door, 0.25), Vec2::new(40.0, 2.0));
    }

    #[test]
    fn facings_map_to_the_four_iso_sprite_directions() {
        // docs/assets.md: se = +x, sw = +y, nw = −x, ne = −y.
        let names: Vec<&str> = FACINGS.iter().map(|d| d.sprite_dir()).collect();
        assert_eq!(names, ["ne", "se", "sw", "nw"]);
        for d in FACINGS {
            assert_eq!(FACINGS[dir_index(d)], d);
        }
        assert_eq!(FACINGS[dir_index(Direction::East)].sprite_dir(), "se");
        assert_eq!(FACINGS[dir_index(Direction::South)].sprite_dir(), "sw");
        for role in ROLES {
            assert_eq!(ROLES[role_index(role)], role);
        }
    }

    #[test]
    fn walk_frames_follow_the_distance_walked() {
        assert_eq!(walk_frame(Vec2::new(3.3, 7.0), Direction::East, false), STAND_FRAME);
        // One frame per half tile, cycling through all four frames over two tiles.
        let frames: Vec<usize> = (0..8).map(|i| walk_frame(Vec2::new(i as f32 * 0.5 + 0.01, 4.0), Direction::East, true)).collect();
        assert_eq!(frames, [0, 1, 2, 3, 0, 1, 2, 3]);
        // Walking west the cycle also runs forwards.
        let west: Vec<usize> = (0..4).map(|i| walk_frame(Vec2::new(10.0 - i as f32 * 0.5 - 0.01, 4.0), Direction::West, true)).collect();
        assert_eq!(west, [0, 1, 2, 3].map(|f| (f + walk_frame(Vec2::new(9.99, 4.0), Direction::West, true)) % 4));
        // Moving along y only counts for north/south facings.
        assert_eq!(walk_frame(Vec2::new(0.2, 1.3), Direction::South, true), 2);
        assert_eq!(walk_frame(Vec2::new(5.0, 1.3), Direction::South, true), 2);
    }

    #[test]
    fn height_eases_across_steps_and_is_continuous() {
        // Tiles with x >= 5 are one level higher; x = 9 is a cliff of three levels.
        let height = |t: IVec2| {
            (t.x >= 0 && t.y >= 0 && t.x < 12 && t.y < 12).then_some(match t.x {
                ..=4 => 1.0,
                5..=8 => 2.0,
                _ => 5.0,
            })
        };
        // Tile centres keep their level.
        assert_eq!(ground_height(Vec2::new(2.5, 3.5), height), 1.0);
        assert_eq!(ground_height(Vec2::new(6.5, 3.5), height), 2.0);
        // At the edge both sides meet half-way.
        let below = ground_height(Vec2::new(4.9999, 3.5), height);
        let above = ground_height(Vec2::new(5.0001, 3.5), height);
        assert!((below - 1.5).abs() < 1e-3 && (above - 1.5).abs() < 1e-3, "{below} {above}");
        // Monotonic climb, no jumps along the way.
        let mut last = ground_height(Vec2::new(4.0, 3.5), height);
        for i in 1..=200 {
            let h = ground_height(Vec2::new(4.0 + i as f32 * 0.01, 3.5), height);
            assert!(h >= last - 1e-6 && h - last < 0.05, "step at {i}: {last} -> {h}");
            last = h;
        }
        // Cliffs are not ramped.
        assert_eq!(ground_height(Vec2::new(8.95, 3.5), height), 2.0);
        // Outside the map: ground level.
        assert_eq!(ground_height(Vec2::new(-3.0, 3.5), height), 0.0);
    }

    #[test]
    fn picking_prefers_the_frontmost_walker_and_grows_small_sprites() {
        let a = WalkerId::Citizen(CitizenId::from_raw_parts(1, 0));
        let b = WalkerId::Citizen(CitizenId::from_raw_parts(2, 0));
        let drawn = [
            DrawnWalker { id: a, bounds: Rect::new(Vec2::new(0.0, 0.0), Vec2::new(16.0, 24.0)), depth: 10.0 },
            DrawnWalker { id: b, bounds: Rect::new(Vec2::new(6.0, 4.0), Vec2::new(22.0, 28.0)), depth: 11.0 },
        ];
        assert_eq!(pick_walker(&drawn, Vec2::new(8.0, 10.0), 0.0), Some((b, 11.0)));
        assert_eq!(pick_walker(&drawn, Vec2::new(2.0, 10.0), 0.0), Some((a, 10.0)));
        assert_eq!(pick_walker(&drawn, Vec2::new(-4.0, 10.0), 0.0), None);
        // Zoomed out, a 16 px figure is clickable within a 32 px box.
        assert_eq!(pick_walker(&drawn, Vec2::new(-4.0, 10.0), 32.0), Some((a, 10.0)));
    }

    #[test]
    fn enemies_and_caravans_carry_their_colours() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = Defs::load(&dir).unwrap_or_else(|e| panic!("shipped game data must load: {e}"));
        let mut state = GameState::new(&defs, kingdoms_sim::state::GameSettings::new(&defs, 7, kingdoms_sim::world::WorldSize::Small));
        // Bandits within the hour: their fighters walk in once they arrive.
        state.tick(&defs, vec![kingdoms_sim::command::Command::DebugSpawnThreat { kingdom: None, size: 4 }]);
        for _ in 0..(kingdoms_sim::time::TICKS_PER_HOUR * 2) {
            state.tick(&defs, Vec::new());
        }
        let sides = Allegiances::of(&state);
        let bandits: Vec<&Allegiance> = sides.travelers.values().filter(|a| a.hostile).collect();
        assert!(!bandits.is_empty(), "the band is on the map");
        assert!(bandits.iter().all(|a| a.color == srgb(BANDIT_COLOR) && a.tint == Color::WHITE));
        assert_eq!(bandits.iter().filter(|a| a.banner).count(), 1, "one banner per band");
        // An army's soldiers are tinted towards their kingdom's colour.
        let t = army_tint(1);
        assert!(t.b > t.r, "colour 1 is blue: {t:?}");
    }

    #[test]
    fn festival_crowds_stand_apart_on_free_ground() {
        let entrance = Vec2::new(10.5, 10.5);
        let spots: Vec<Vec2> = (0..FESTIVAL_CROWD_MAX).filter_map(|r| festival_spot(entrance, r, |_| true)).collect();
        assert_eq!(spots.len(), FESTIVAL_CROWD_MAX);
        assert_eq!(spots[0], entrance);
        for (i, a) in spots.iter().enumerate() {
            assert!(a.distance(entrance) < 4.0, "{a}");
            for b in &spots[i + 1..] {
                assert!(a.distance(*b) > 0.25, "{a} {b}");
            }
        }
        // Blocked ground (the venue behind the entrance) is avoided; nobody shares a spot.
        let free = |p: Vec2| p.y >= 10.0;
        let spots: Vec<Vec2> = (0..FESTIVAL_CROWD_MAX).filter_map(|r| festival_spot(entrance, r, free)).collect();
        assert!(spots.len() > FESTIVAL_CROWD_MAX / 2 && spots.iter().all(|p| free(*p)));
        for (i, a) in spots.iter().enumerate() {
            assert!(spots[i + 1..].iter().all(|b| a.distance(*b) > 0.05));
        }
        assert_eq!(festival_spot(entrance, 3, |_| false), None);
        assert_eq!(facing_towards(Vec2::new(-2.0, 0.5)), Direction::West);
        assert_eq!(facing_towards(Vec2::new(0.2, -1.0)), Direction::North);
    }

    #[test]
    fn crowds_spread_evenly_around_their_spot() {
        assert_eq!(crowd_offset(0), Vec2::ZERO, "the first figure stays on the spot");
        let offsets: Vec<Vec2> = (0..120).map(crowd_offset).collect();
        let radius = offsets.iter().map(|o| o.length()).fold(0.0, f32::max);
        assert!(radius <= 0.2 * 120f32.sqrt() + 1e-3, "an army of 120 stays within {radius} tiles");
        let mut closest = f32::MAX;
        for (i, a) in offsets.iter().enumerate() {
            for b in &offsets[i + 1..] {
                closest = closest.min(a.distance(*b));
            }
        }
        assert!(closest > 0.15, "no two figures stand on each other: {closest}");
    }

    #[test]
    fn a_group_on_one_spot_is_drawn_as_a_crowd() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = Defs::load(&dir).unwrap_or_else(|e| panic!("shipped game data must load: {e}"));
        let mut state = GameState::new(&defs, kingdoms_sim::state::GameSettings::new(&defs, 7, kingdoms_sim::world::WorldSize::Small));
        state.tick(&defs, vec![kingdoms_sim::command::Command::DebugSpawnThreat { kingdom: None, size: 8 }]);
        for _ in 0..kingdoms_sim::time::TICKS_PER_HOUR {
            state.tick(&defs, Vec::new());
        }
        let sides = Allegiances::of(&state);
        let offsets: BTreeSet<(i32, i32)> =
            sides.travelers.values().filter(|a| a.hostile).map(|a| ((a.offset.x * 1000.0) as i32, (a.offset.y * 1000.0) as i32)).collect();
        let band = sides.travelers.values().filter(|a| a.hostile).count();
        assert!(band >= 2, "the band is on the map");
        assert_eq!(offsets.len(), band, "every bandit of the band gets its own place");
    }

    #[test]
    fn act_loops_run_out_of_step_and_follow_game_time() {
        let frames: Vec<SpriteId> = (0..4).map(SpriteId).collect();
        let anim = Animation { frames, fps: 6.0, looping: true };
        // Game time drives the loop; the same walker at the same time shows the same frame.
        assert_eq!(act_frame(&anim, 1.0, 5), act_frame(&anim, 1.0, 5));
        let over_time: BTreeSet<u32> = (0..12).map(|i| act_frame(&anim, i as f32 / 6.0, 5).0).collect();
        assert_eq!(over_time.len(), 4, "the loop cycles through all frames");
        // Different walkers start at different points of the loop.
        let crew: BTreeSet<u32> = (0..20).map(|k| act_frame(&anim, 0.0, k * 37).0).collect();
        assert!(crew.len() > 1);
        assert_eq!(walker_key(WalkerId::Citizen(CitizenId::from_raw_parts(3, 0))) % 2, 0);
        assert!(shadow_width(CitizenRole::Cart) > shadow_width(CitizenRole::Man));
    }

    #[test]
    fn gatherers_use_the_tool_of_their_workplace() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match Defs::load(&dir) {
            Ok(defs) => defs,
            Err(e) => panic!("shipped game data must load: {e}"),
        };
        let tool = |id: &str| match defs.buildings().iter().find(|b| b.key == id) {
            Some(def) => GatherTool::of(def),
            None => panic!("no building {id}"),
        };
        assert_eq!(tool("woodcutter"), GatherTool::Axe);
        for id in ["quarry", "iron_mine", "coal_mine", "gold_mine"] {
            assert_eq!(tool(id), GatherTool::Pickaxe, "{id}");
        }
        for id in ["clay_pit", "sand_pit", "forester"] {
            assert_eq!(tool(id), GatherTool::Shovel, "{id}");
        }
        assert_eq!(tool("fishery"), GatherTool::Rod);
        assert_eq!(tool("wheat_farm"), GatherTool::Axe);
        assert_eq!(tool("bakery"), GatherTool::Axe);
        for (i, t) in GatherTool::LOOPS.iter().enumerate() {
            assert_eq!(t.loop_index(), Some(i));
            assert!(t.loop_kind().is_some());
        }
        assert_eq!(GatherTool::Axe.loop_kind(), None);
    }

    #[test]
    fn goods_are_held_in_front_only_in_front_views() {
        assert!(goods_offset(Direction::East).1 > 0.0 && goods_offset(Direction::South).1 > 0.0);
        assert!(goods_offset(Direction::North).1 < 0.0 && goods_offset(Direction::West).1 < 0.0);
        // On the side the walker faces on screen: se/ne right, sw/nw left.
        assert!(goods_offset(Direction::East).0.x > 0.0 && goods_offset(Direction::West).0.x < 0.0);
    }
}
