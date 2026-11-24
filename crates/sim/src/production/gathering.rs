//! Gathering from the surroundings: felling trees, planting saplings, extracting deposits,
//! pasture coverage and the bee bonus. Searches are deterministic: the nearest candidate by
//! Chebyshev distance to the building's footprint wins, ties are broken by position.
//!
//! Gatherers walk to their target (step 3b): they work on a walkable *work spot* — the target
//! itself, or for water and other impassable targets its first walkable neighbour (N, E, S,
//! W) — that walkers from the building's entrance can reach ([`work_spot`]). Targets under or
//! next to the footprint (mines) are worked from inside. Unreachable targets are skipped.

use std::collections::{BTreeMap, BTreeSet};

use glam::IVec2;

use crate::buildings::BuildingId;
use crate::defs::{Defs, GameRules};
use crate::grid::TileRect;
use crate::state::GameState;
use crate::world::{Biome, DepositKind, Terrain, Tile, TreeKind, Vegetation};

/// True if `vegetation` is a tree grown enough to be felled.
pub fn is_fellable(vegetation: Vegetation, rules: &GameRules) -> bool {
    matches!(vegetation, Vegetation::Tree { growth, .. } if growth >= rules.fellable_tree_growth)
}

/// True if a forester may plant a sapling at `p`: owned, natural soil (grass, meadow, forest
/// floor, dirt or steppe) without vegetation (a stump is replanted), deposit, road, building or
/// field/pasture claim, and not directly next to a building (keeps entrances and walls clear).
pub fn is_plantable(state: &GameState, p: IVec2) -> bool {
    let Some(tile) = state.world.get(p) else { return false };
    matches!(tile.terrain, Terrain::Grass | Terrain::Meadow | Terrain::Forest | Terrain::Dirt | Terrain::Steppe)
        && matches!(tile.vegetation, Vegetation::None | Vegetation::Stump { .. })
        && tile.deposit.is_none()
        && state.territory.contains(p)
        && !state.roads.has_road(p)
        && !crate::agriculture::is_claimed(state, p)
        && TileRect::tile(p).expanded(1).tiles().all(|q| state.buildings.id_at(q).is_none())
}

/// Tiles around `rect` within `radius`, clipped to the map; footprint tiles only if
/// `include_footprint`.
fn area(state: &GameState, rect: TileRect, radius: u8, include_footprint: bool) -> impl Iterator<Item = IVec2> + '_ {
    let r = i32::from(radius);
    rect.expanded(r)
        .clipped(state.world.width(), state.world.height())
        .tiles()
        .collect::<Vec<_>>()
        .into_iter()
        .filter(move |p| include_footprint || !rect.contains(*p))
        .filter(move |p| rect.distance_to(*p) <= r)
}

/// Scrambles a position into a stable pseudo-random order key (spreads saplings naturally).
fn scramble(p: IVec2) -> u32 {
    let mut h = (p.x as u32).wrapping_mul(0x9E37_79B1) ^ (p.y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE3D);
    h ^ (h >> 13)
}

/// Where gatherers of a building with footprint `rect` and entrance `entrance` work on
/// `target`: `Some(None)` inside the building (the target lies under or next to it),
/// `Some(Some(tile))` on a walkable work spot reachable from the entrance (the target, else its
/// first such neighbour in N, E, S, W order; never a bridge, which may be removed under the
/// worker's feet), `None` if the target cannot be reached. Reads the navigation components,
/// which the production step brings up to date first.
pub(crate) fn work_spot(state: &GameState, rect: TileRect, entrance: IVec2, target: IVec2) -> Option<Option<IVec2>> {
    if rect.expanded(1).contains(target) {
        return Some(None);
    }
    let nav = &state.navigation;
    let home = nav.component(entrance)?;
    std::iter::once(target)
        .chain(crate::grid::Direction::ALL.iter().map(|d| target + d.offset()))
        .find(|p| {
            nav.component(*p) == Some(home) && !rect.contains(*p) && !state.roads.get(*p).is_some_and(|t| t.is_bridge())
        })
        .map(Some)
}

/// Nearest fellable tree within `radius` that is not in `reserved` and passes `usable`.
pub(crate) fn find_tree(
    state: &GameState,
    rules: &GameRules,
    rect: TileRect,
    radius: u8,
    reserved: &BTreeSet<(i32, i32)>,
    usable: &dyn Fn(IVec2) -> bool,
) -> Option<IVec2> {
    let mut found: Vec<IVec2> = area(state, rect, radius, false)
        .filter(|p| is_fellable(state.world.tile(*p).vegetation, rules) && !reserved.contains(&(p.x, p.y)))
        .collect();
    found.sort_by_key(|p| (rect.distance_to(*p), p.y, p.x));
    found.into_iter().find(|p| usable(*p))
}

/// Plantable tile at least two tiles away from the forester, nearest first (spread by a stable
/// scramble among equally near tiles), not in `reserved`, passing `usable`.
pub(crate) fn find_planting_spot(
    state: &GameState,
    rect: TileRect,
    radius: u8,
    reserved: &BTreeSet<(i32, i32)>,
    usable: &dyn Fn(IVec2) -> bool,
) -> Option<IVec2> {
    let mut found: Vec<IVec2> = area(state, rect, radius, false)
        .filter(|p| rect.distance_to(*p) >= 2 && !reserved.contains(&(p.x, p.y)) && is_plantable(state, *p))
        .collect();
    found.sort_by_key(|p| (rect.distance_to(*p), scramble(*p), p.y, p.x));
    found.into_iter().find(|p| usable(*p))
}

/// Deposit units per tile that running cycles have already claimed (see
/// [`find_deposit`]).
pub(crate) type DepositReservations = BTreeMap<(i32, i32), u32>;

/// Nearest tile with deposit units of `kind` left over after `reserved` (units claimed by
/// running cycles of other gatherers), or of terrain `or_terrain`, which never depletes,
/// within `radius`, including the footprint ("under or next to the mine"), passing `usable`.
pub(crate) fn find_deposit(
    state: &GameState,
    rect: TileRect,
    kind: DepositKind,
    radius: u8,
    or_terrain: Option<Terrain>,
    reserved: &DepositReservations,
    usable: &dyn Fn(IVec2) -> bool,
) -> Option<IVec2> {
    let mut found: Vec<IVec2> = area(state, rect, radius, true)
        .filter(|p| {
            let t = state.world.tile(*p);
            let claimed = reserved.get(&(p.x, p.y)).copied().unwrap_or(0);
            t.deposit.is_some_and(|d| d.kind == kind && u32::from(d.amount) > claimed)
                || or_terrain.is_some_and(|o| t.terrain == o)
        })
        .collect();
    found.sort_by_key(|p| (rect.distance_to(*p), p.y, p.x));
    found.into_iter().find(|p| usable(*p))
}

/// Pasture coverage in permille: the land the herd grazes (its animals ×
/// `pasture_tiles_per_animal`, at most the tiles the farm claims as pasture, see
/// [`crate::agriculture`]) relative to the `tiles` a full herd needs (capped at 1000).
pub(crate) fn pasture_permille(state: &GameState, defs: &Defs, building: BuildingId, tiles: u16) -> u16 {
    let count = crate::agriculture::pasture_tiles(state, building);
    let herd = state.agriculture.farm(building).map_or(0, |f| usize::from(f.herd()));
    let grazed = (herd * usize::from(defs.rules().agriculture.pasture_tiles_per_animal.max(1))).min(count);
    let needed = usize::from(tiles.max(1));
    (grazed.min(needed) * 1000 / needed) as u16
}

/// Bee factor in permille: 1000 plus `max_bonus_percent` scaled by the share of flowering tiles
/// within `radius`: open meadow (not built over, paved or ploughed; pastures count) and orchard
/// fields.
pub(crate) fn bee_permille(state: &GameState, defs: &Defs, rect: TileRect, radius: u8, max_bonus_percent: u16) -> u16 {
    let mut total = 0usize;
    let mut flowers = 0usize;
    for p in area(state, rect, radius, false) {
        total += 1;
        let ploughed = state.agriculture.field_at(p).is_some_and(|v| v.kind == crate::agriculture::FarmKind::Fields);
        let meadow = state.world.tile(p).terrain == Terrain::Meadow
            && !ploughed
            && state.buildings.id_at(p).is_none()
            && !state.roads.has_road(p);
        if meadow || crate::agriculture::is_orchard_field(state, defs, p) {
            flowers += 1;
        }
    }
    let bonus = (flowers * usize::from(max_bonus_percent) * 10).checked_div(total).unwrap_or(0);
    (1000 + bonus).min(usize::from(u16::MAX)) as u16
}

/// Tree species a forester plants on `tile` (cold or mountainous land gets conifers, swamps
/// willows, elsewhere a stable mix of deciduous trees, birches and conifers).
pub(crate) fn tree_kind_for(tile: &Tile, p: IVec2) -> TreeKind {
    match tile.biome {
        Biome::Mountains | Biome::Snow => TreeKind::Conifer,
        Biome::Swamp => TreeKind::Willow,
        _ if tile.temperature < 90 => TreeKind::Conifer,
        _ => match scramble(p) % 20 {
            0..=11 => TreeKind::Deciduous,
            12..=16 => TreeKind::Birch,
            _ => TreeKind::Conifer,
        },
    }
}
