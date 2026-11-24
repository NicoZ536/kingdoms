//! Vegetation and deposits: trees (species, season, growth stage), tree stumps, bushes, boulders
//! and deposit ground markers, cached as per-chunk static batches of the `nature` atlas.
//!
//! Chunks are rebuilt when their tiles change (felling, planting, growth stages, depletion) and
//! all chunks when the season changes. Every object gets a deterministic per-tile variation
//! (position jitter, mirroring, slight tint, wind phase) so forests do not look gridded.
//! In autumn the layer also lists the crowns of the leaf trees per chunk ([`LeafSource`]) for
//! the falling-leaves effect, and in every season the feet of trees and bushes
//! ([`TreeShadow`]) for their ground shadows, which the map view draws every frame (their
//! opacity follows the daylight).

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::{AssetManager, SpriteId};
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::Renderer;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, OBJECT_DEPTH_BIAS, SpriteDepth};
use kingdoms_engine::rendering::sprite::{FLAG_FLIP_X, FLAG_GROUND, FLAG_WIND, RenderList, SpriteInstance};
use kingdoms_sim::defs::GameRules;
use kingdoms_sim::nature::tree_stage;
use kingdoms_sim::time::Season;
use kingdoms_sim::world::{DepositKind, Tile, TreeKind, Vegetation, WorldMap};

use super::chunks::{ChunkBatches, chunk_tiles};
use super::{DEPOSIT_BIAS, hash01, tile_hash};

/// Tree species in sprite-table order.
const TREE_KINDS: [TreeKind; 4] = [TreeKind::Deciduous, TreeKind::Conifer, TreeKind::Birch, TreeKind::Willow];
/// Growth stage suffixes, indexed by [`tree_stage`] (0 sapling, 1 young, 2 fellable).
const STAGES: [&str; 3] = ["sapling", "young", "mature"];
/// Number of boulder variants.
const BOULDERS: usize = 3;
/// Largest offset of a tree's trunk from the tile centre (tiles); keeps it inside its tile.
const TREE_JITTER: f32 = 0.18;
/// Largest offset of bushes and boulders from the tile centre (tiles).
const SMALL_JITTER: f32 = 0.12;
// Jittered objects must stay on their own tile (their depth is that of the tile).
const _: () = assert!(TREE_JITTER < 0.5 && SMALL_JITTER < 0.5);

fn tree_index(kind: TreeKind) -> usize {
    TREE_KINDS.iter().position(|k| *k == kind).unwrap_or(0)
}

fn deposit_index(kind: DepositKind) -> usize {
    DepositKind::ALL.iter().position(|k| *k == kind).unwrap_or(0)
}

/// Resolved sprite ids of the `nature` atlas.
struct NatureSprites {
    /// `[species][season][stage]`
    trees: [[[SpriteId; 3]; 4]; 4],
    stump: SpriteId,
    bushes: [SpriteId; 4],
    boulders: [SpriteId; BOULDERS],
    deposits: [SpriteId; DepositKind::ALL.len()],
}

impl NatureSprites {
    fn load(assets: &AssetManager) -> Self {
        let trees = std::array::from_fn(|k| {
            std::array::from_fn(|s| {
                std::array::from_fn(|g| {
                    assets.sprite(&format!(
                        "nature/{}_{}_{}",
                        TREE_KINDS[k].sprite_key(),
                        Season::ALL[s].sprite_key(),
                        STAGES[g]
                    ))
                })
            })
        });
        let stump = assets.sprite("nature/stump");
        let bushes = std::array::from_fn(|s| assets.sprite(&format!("nature/bush_{}", Season::ALL[s].sprite_key())));
        let boulders = std::array::from_fn(|i| assets.sprite(&format!("nature/boulder_{i}")));
        let deposits = std::array::from_fn(|i| assets.sprite(&format!("nature/{}", DepositKind::ALL[i].sprite_key())));
        Self { trees, stump, bushes, boulders, deposits }
    }
}

/// Crown of a leaf tree that drops leaves in autumn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeafSource {
    /// World position in the middle of the crown.
    pub crown: Vec2,
    /// Half extents of the crown in world pixels.
    pub spread: Vec2,
    /// Iso depth of the trunk (for the falling leaves).
    pub depth: f32,
    /// Stable per-tree key.
    pub key: u32,
}

/// Ground shadow of a tree or bush.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeShadow {
    /// Foot of the trunk in tile space.
    pub ground: Vec2,
    /// Elevation level of its tile.
    pub height: f32,
    /// Shadow width in world pixels.
    pub width: f32,
}

/// Shadow width of a tree of `kind` at growth stage `stage` (0–2) in `season`: by crown size,
/// narrower for conifers, thinner for bare leaf trees in winter.
pub fn tree_shadow_width(kind: TreeKind, stage: usize, season: Season) -> f32 {
    let base = [11.0, 20.0, 30.0][stage.min(2)];
    let shape = if kind == TreeKind::Conifer { 0.8 } else { 1.0 };
    let bare = if season == Season::Winter && kind != TreeKind::Conifer { 0.7 } else { 1.0 };
    base * shape * bare
}

/// Shadows of one chunk with the area they cover.
#[derive(Clone, Debug)]
struct ChunkShadows {
    bounds: Rect,
    list: Vec<TreeShadow>,
}

impl Default for ChunkShadows {
    fn default() -> Self {
        Self { bounds: Rect::EMPTY, list: Vec::new() }
    }
}

/// Leaf trees of one chunk with the area their crowns cover.
#[derive(Clone, Debug)]
struct ChunkLeaves {
    bounds: Rect,
    sources: Vec<LeafSource>,
}

impl Default for ChunkLeaves {
    fn default() -> Self {
        Self { bounds: Rect::EMPTY, sources: Vec::new() }
    }
}

/// Renderer of the vegetation/deposit layer.
pub struct VegetationRenderer {
    sprites: NatureSprites,
    batches: ChunkBatches,
    season: Season,
    /// Leaf trees per chunk (autumn only), row-major like the batches.
    leaves: Vec<ChunkLeaves>,
    /// Tree and bush shadows per chunk, row-major like the batches.
    shadows: Vec<ChunkShadows>,
}

impl VegetationRenderer {
    pub fn new(assets: &AssetManager, season: Season) -> Self {
        let texture = assets.atlas_texture("nature").unwrap_or_else(|| assets.region(assets.missing_sprite()).texture);
        Self {
            sprites: NatureSprites::load(assets),
            batches: ChunkBatches::new(texture),
            season,
            leaves: Vec::new(),
            shadows: Vec::new(),
        }
    }

    /// Sprites in all chunks.
    pub fn instance_count(&self) -> usize {
        self.batches.instance_count()
    }

    /// Frees every GPU batch (before the layer is dropped).
    pub fn release(&mut self, renderer: &mut Renderer) {
        self.batches.release(renderer);
        self.leaves.clear();
        self.shadows.clear();
    }

    /// Shadows of trees and bushes whose chunk may be visible in `view`.
    pub fn shadows(&self, view: Rect) -> impl Iterator<Item = &TreeShadow> + '_ {
        self.shadows.iter().filter(move |c| !c.list.is_empty() && c.bounds.intersects(&view)).flat_map(|c| c.list.iter())
    }

    /// Leaf trees whose chunk may be visible in `view` (empty outside autumn).
    pub fn leaf_sources(&self, view: Rect) -> impl Iterator<Item = &LeafSource> + '_ {
        self.leaves.iter().filter(move |c| !c.sources.is_empty() && c.bounds.intersects(&view)).flat_map(|c| c.sources.iter())
    }

    /// Rebuilds every chunk for `season`.
    pub fn rebuild_all(&mut self, map: &WorldMap, rules: &GameRules, season: Season, renderer: &mut Renderer, assets: &AssetManager) {
        self.season = season;
        self.batches.reset(map.chunk_count(), renderer);
        let count = map.chunk_count();
        self.leaves = vec![ChunkLeaves::default(); (count.x.max(0) * count.y.max(0)) as usize];
        self.shadows = vec![ChunkShadows::default(); (count.x.max(0) * count.y.max(0)) as usize];
        let chunks: Vec<IVec2> = self.batches.all_chunks().collect();
        for chunk in chunks {
            self.rebuild_chunk(chunk, map, rules, renderer, assets);
        }
    }

    /// Brings the layer up to date: everything on a season change, else the dirty chunks.
    pub fn update(
        &mut self,
        dirty: &[IVec2],
        map: &WorldMap,
        rules: &GameRules,
        season: Season,
        renderer: &mut Renderer,
        assets: &AssetManager,
    ) {
        if season != self.season || self.batches.chunk_count() != map.chunk_count() {
            self.rebuild_all(map, rules, season, renderer, assets);
            return;
        }
        for &chunk in dirty {
            self.rebuild_chunk(chunk, map, rules, renderer, assets);
        }
    }

    fn rebuild_chunk(&mut self, chunk: IVec2, map: &WorldMap, rules: &GameRules, renderer: &mut Renderer, assets: &AssetManager) {
        let mut out = Vec::new();
        let mut leaves = ChunkLeaves::default();
        let mut shadows = ChunkShadows::default();
        for p in chunk_tiles(chunk, map.size()) {
            if let Some(shadow) = self.push_tile(&mut out, p, map.tile(p), rules, assets) {
                let w = iso::tile_to_world_elevated(shadow.ground, shadow.height);
                let half = Vec2::new(shadow.width * 0.5 + 4.0, shadow.width * 0.25 + 2.0);
                shadows.bounds = shadows.bounds.union(&Rect::new(w - half, w + half));
                shadows.list.push(shadow);
            }
            if self.season == Season::Autumn
                && let Some(source) = self.leaf_source(p, map.tile(p), rules, assets)
            {
                let area = Rect::new(source.crown - source.spread, source.crown + source.spread);
                leaves.bounds = leaves.bounds.union(&area);
                leaves.sources.push(source);
            }
        }
        self.batches.set(chunk, &out, renderer);
        let count = self.batches.chunk_count();
        if chunk.x >= 0 && chunk.y >= 0 && chunk.x < count.x && chunk.y < count.y
            && let Some(slot) = self.leaves.get_mut((chunk.y * count.x + chunk.x) as usize)
        {
            *slot = leaves;
        }
        if chunk.x >= 0 && chunk.y >= 0 && chunk.x < count.x && chunk.y < count.y
            && let Some(slot) = self.shadows.get_mut((chunk.y * count.x + chunk.x) as usize)
        {
            *slot = shadows;
        }
    }

    /// The crown of a young or grown leaf tree on tile `p` (same jitter as its sprite).
    fn leaf_source(&self, p: IVec2, tile: &Tile, rules: &GameRules, assets: &AssetManager) -> Option<LeafSource> {
        let Vegetation::Tree { kind, growth } = tile.vegetation else { return None };
        let stage = usize::from(tree_stage(growth, rules)).min(STAGES.len() - 1);
        if kind == TreeKind::Conifer || stage == 0 {
            return None;
        }
        let hash = tile_hash(p);
        let centre = p.as_vec2() + Vec2::splat(0.5);
        let foot = centre + Vec2::new(hash01(hash, 1) * 2.0 - 1.0, hash01(hash, 2) * 2.0 - 1.0) * TREE_JITTER;
        let size = assets.region(self.sprites.trees[tree_index(kind)][Season::Autumn.index()][stage]).size;
        let base = iso::tile_to_world_elevated(foot, f32::from(tile.elevation));
        Some(LeafSource {
            crown: base - Vec2::new(0.0, size.y * 0.62),
            spread: Vec2::new(size.x * 0.32, size.y * 0.18),
            depth: iso::ground_depth(foot) + OBJECT_DEPTH_BIAS + 0.005,
            key: hash,
        })
    }

    /// Instances for one tile: the deposit marker, then the plant or rock standing on it.
    /// Returns the ground shadow of a tree or bush.
    fn push_tile(&self, out: &mut Vec<SpriteInstance>, p: IVec2, tile: &Tile, rules: &GameRules, assets: &AssetManager) -> Option<TreeShadow> {
        let elevation = f32::from(tile.elevation);
        let hash = tile_hash(p);
        let centre = p.as_vec2() + Vec2::splat(0.5);
        if let Some(deposit) = tile.deposit {
            let r = assets.region(self.sprites.deposits[deposit_index(deposit.kind)]);
            let pos = iso::tile_to_world_elevated(centre, elevation);
            let flags = if tile.terrain.is_water() { 0 } else { FLAG_GROUND };
            out.push(SpriteInstance::new(pos, r.size, r.anchor, r.uv, SpriteDepth::ground(centre, DEPOSIT_BIAS)).with_flags(flags));
        }
        let season = self.season.index();
        let (sprite, jitter, sways, shadow) = match tile.vegetation {
            Vegetation::None => return None,
            Vegetation::Tree { kind, growth } => {
                let stage = usize::from(tree_stage(growth, rules)).min(STAGES.len() - 1);
                (self.sprites.trees[tree_index(kind)][season][stage], TREE_JITTER, true, tree_shadow_width(kind, stage, self.season))
            }
            Vegetation::Stump { .. } => (self.sprites.stump, SMALL_JITTER, false, 0.0),
            Vegetation::Bush => (self.sprites.bushes[season], SMALL_JITTER, true, 14.0),
            Vegetation::Boulder => (self.sprites.boulders[hash as usize % BOULDERS], SMALL_JITTER, false, 0.0),
        };
        let foot = centre + Vec2::new(hash01(hash, 1) * 2.0 - 1.0, hash01(hash, 2) * 2.0 - 1.0) * jitter;
        // Brightness and a faint warm/cool shift per object.
        let light = 0.9 + hash01(hash, 3) * 0.14;
        let warm = (hash01(hash, 4) - 0.5) * 0.06;
        let r = assets.region(sprite);
        let pos = iso::tile_to_world_elevated(foot, elevation);
        let mut instance = SpriteInstance::new(pos, r.size, r.anchor, r.uv, SpriteDepth::upright_point(foot, OBJECT_DEPTH_BIAS))
            .with_color(Color::rgb(light + warm, light, light - warm));
        if hash01(hash, 5) < 0.5 {
            instance = instance.with_flags(FLAG_FLIP_X);
        }
        if sways {
            instance = instance.with_flags(FLAG_WIND).with_phase(hash01(hash, 6) * std::f32::consts::TAU);
        }
        out.push(instance);
        (shadow > 0.0 && !tile.terrain.is_water()).then_some(TreeShadow { ground: foot, height: elevation, width: shadow })
    }

    /// Queues all chunk batches.
    pub fn draw(&self, list: &mut RenderList) {
        self.batches.draw(list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadows_grow_with_the_tree() {
        let w = |stage| tree_shadow_width(TreeKind::Deciduous, stage, Season::Summer);
        assert!(w(0) < w(1) && w(1) < w(2));
        assert!(tree_shadow_width(TreeKind::Deciduous, 2, Season::Winter) < w(2), "bare crowns cast less shade");
        assert_eq!(tree_shadow_width(TreeKind::Conifer, 2, Season::Winter), tree_shadow_width(TreeKind::Conifer, 2, Season::Summer));
        assert_eq!(w(9), w(2));
    }

    #[test]
    fn sprite_tables_follow_the_asset_names() {
        // Index helpers must agree with the order used to build the tables.
        for (i, k) in TREE_KINDS.iter().enumerate() {
            assert_eq!(tree_index(*k), i);
        }
        for (i, k) in DepositKind::ALL.iter().enumerate() {
            assert_eq!(deposit_index(*k), i);
        }
        assert_eq!(TREE_KINDS.map(TreeKind::sprite_key), ["oak", "pine", "birch", "willow"]);
    }
}
