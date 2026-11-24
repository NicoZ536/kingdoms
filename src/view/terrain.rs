//! Terrain rendering: converts the simulation's tile map into cached per-chunk sprite batches.

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::{AssetManager, SpriteId};
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::Renderer;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, SpriteDepth};
use kingdoms_engine::rendering::sprite::{FLAG_GROUND, FLAG_WATER, RenderList, SpriteInstance, StaticBatchHandle};
use kingdoms_engine::rendering::texture::TextureId;
use kingdoms_sim::world::{CHUNK_SIZE, Terrain, WorldMap};

use super::tile_hash;

/// Number of art variants per terrain type (see the asset generator).
const VARIANTS: usize = 4;
/// Neighbour offsets in the order of the edge sprite suffixes.
const EDGE_DIRS: [(IVec2, &str); 4] =
    [(IVec2::new(0, -1), "ne"), (IVec2::new(1, 0), "se"), (IVec2::new(0, 1), "sw"), (IVec2::new(-1, 0), "nw")];

struct TerrainSprites {
    texture: TextureId,
    /// `[ne, nw]` light rims on plateau back edges and ambient occlusion at cliff feet.
    rims: [SpriteId; 2],
    ao: [SpriteId; 2],
    tiles: Vec<[SpriteId; VARIANTS]>,
    cliffs: Vec<SpriteId>,
    edges: Vec<[SpriteId; 4]>,
}

impl TerrainSprites {
    fn load(assets: &AssetManager) -> Self {
        let mut tiles = Vec::new();
        let mut cliffs = Vec::new();
        let mut edges = Vec::new();
        for t in Terrain::ALL {
            let key = t.sprite_key();
            tiles.push(std::array::from_fn(|v| assets.sprite(&format!("terrain/{key}_{v}"))));
            cliffs.push(assets.sprite(&format!("terrain/cliff_{key}")));
            edges.push(std::array::from_fn(|d| assets.sprite(&format!("terrain/{key}_edge_{}", EDGE_DIRS[d].1))));
        }
        let texture = assets
            .atlas_texture("terrain")
            .unwrap_or_else(|| assets.region(assets.missing_sprite()).texture);
        let rims = [assets.sprite("terrain/rim_ne"), assets.sprite("terrain/rim_nw")];
        let ao = [assets.sprite("terrain/ao_ne"), assets.sprite("terrain/ao_nw")];
        Self { texture, rims, ao, tiles, cliffs, edges }
    }
}

pub struct TerrainRenderer {
    sprites: TerrainSprites,
    chunks: Vec<Option<StaticBatchHandle>>,
    /// Alpha-blended decals per chunk (drawn each frame when the chunk is visible).
    decals: Vec<(Rect, Vec<SpriteInstance>)>,
    chunk_count: IVec2,
    /// Instances per chunk batch (statistics).
    chunk_instances: Vec<usize>,
}

impl TerrainRenderer {
    pub fn new(assets: &AssetManager) -> Self {
        Self {
            sprites: TerrainSprites::load(assets),
            chunks: Vec::new(),
            decals: Vec::new(),
            chunk_count: IVec2::ZERO,
            chunk_instances: Vec::new(),
        }
    }

    pub fn instance_count(&self) -> usize {
        self.chunk_instances.iter().sum()
    }

    /// Frees every GPU batch (before the layer is dropped).
    pub fn release(&mut self, renderer: &mut Renderer) {
        for handle in self.chunks.drain(..).flatten() {
            renderer.remove_static_batch(handle);
        }
        self.chunk_count = IVec2::ZERO;
        self.chunk_instances.clear();
        self.decals.clear();
    }

    /// (Re)builds every chunk of `map`.
    pub fn rebuild_all(&mut self, map: &WorldMap, renderer: &mut Renderer, assets: &AssetManager) {
        for handle in self.chunks.drain(..).flatten() {
            renderer.remove_static_batch(handle);
        }
        self.chunk_count = map.chunk_count();
        self.chunks = vec![None; (self.chunk_count.x * self.chunk_count.y) as usize];
        self.chunk_instances = vec![0; self.chunks.len()];
        self.decals = vec![(Rect::EMPTY, Vec::new()); self.chunks.len()];
        for cy in 0..self.chunk_count.y {
            for cx in 0..self.chunk_count.x {
                self.rebuild_chunk(IVec2::new(cx, cy), map, renderer, assets);
            }
        }
        log::info!("terrain: built {} chunks, {} sprites", self.chunks.len(), self.instance_count());
    }

    /// Rebuilds the given chunks (those whose tiles changed).
    pub fn rebuild_chunks(&mut self, chunks: &[IVec2], map: &WorldMap, renderer: &mut Renderer, assets: &AssetManager) {
        if self.chunk_count != map.chunk_count() {
            self.rebuild_all(map, renderer, assets);
            return;
        }
        for &chunk in chunks {
            self.rebuild_chunk(chunk, map, renderer, assets);
        }
    }

    fn rebuild_chunk(&mut self, chunk: IVec2, map: &WorldMap, renderer: &mut Renderer, assets: &AssetManager) {
        if chunk.x < 0 || chunk.y < 0 || chunk.x >= self.chunk_count.x || chunk.y >= self.chunk_count.y {
            return;
        }
        let (instances, decals) = self.build_chunk_instances(chunk, map, assets);
        let slot = (chunk.y * self.chunk_count.x + chunk.x) as usize;
        let bounds = decals.iter().fold(Rect::EMPTY, |r, d| r.union(&d.world_bounds()));
        self.decals[slot] = (bounds, decals);
        match self.chunks[slot] {
            Some(handle) if renderer.update_static_batch(handle, self.sprites.texture, &instances) => {}
            _ => self.chunks[slot] = Some(renderer.create_static_batch(self.sprites.texture, &instances)),
        }
        self.chunk_instances[slot] = instances.len();
    }

    fn build_chunk_instances(
        &self,
        chunk: IVec2,
        map: &WorldMap,
        assets: &AssetManager,
    ) -> (Vec<SpriteInstance>, Vec<SpriteInstance>) {
        let mut out = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize * 2);
        let mut decals = Vec::new();
        let origin = chunk * CHUNK_SIZE;
        for y in origin.y..(origin.y + CHUNK_SIZE).min(map.height()) {
            for x in origin.x..(origin.x + CHUNK_SIZE).min(map.width()) {
                let p = IVec2::new(x, y);
                self.push_tile(&mut out, map, p, assets);
                self.push_elevation_decals(&mut decals, map, p, assets);
            }
        }
        (out, decals)
    }

    /// Light rims where the terrain drops behind a tile and shadows at the foot of cliffs.
    fn push_elevation_decals(&self, out: &mut Vec<SpriteInstance>, map: &WorldMap, p: IVec2, assets: &AssetManager) {
        let tile = map.tile(p);
        if tile.terrain.is_water() {
            return;
        }
        let center = iso::tile_center_world(p, f32::from(tile.elevation));
        for (i, back) in [IVec2::new(0, -1), IVec2::new(-1, 0)].into_iter().enumerate() {
            let Some(n) = map.get(p + back) else { continue };
            let sprite = if n.elevation < tile.elevation {
                self.sprites.rims[i]
            } else if n.elevation > tile.elevation {
                self.sprites.ao[i]
            } else {
                continue;
            };
            let r = assets.region(sprite);
            let depth = SpriteDepth::ground(p.as_vec2() + Vec2::splat(0.5), iso::DECAL_DEPTH_BIAS * 1.5);
            out.push(SpriteInstance::new(center, r.size, r.anchor, r.uv, depth));
        }
    }

    fn push_tile(&self, out: &mut Vec<SpriteInstance>, map: &WorldMap, p: IVec2, assets: &AssetManager) {
        let tile = map.tile(p);
        let t_index = tile.terrain as usize;
        let elev = f32::from(tile.elevation);
        let center = iso::tile_center_world(p, elev);
        let hash = tile_hash(p);
        // Land receives snow cover and wetness on its top face.
        let flags = if tile.terrain.is_water() { FLAG_WATER } else { FLAG_GROUND };

        // Top face + first skirt level.
        let sprite = self.sprites.tiles[t_index][hash as usize % VARIANTS];
        let region = assets.region(sprite);
        // Higher terraces are slightly brighter so elevation reads well from far away; a smooth
        // low-frequency variation breaks up large uniform areas without revealing the grid.
        let macro_var = ((p.x as f32 * 0.21).sin() + (p.y as f32 * 0.17).cos() + (p.x as f32 * 0.07 + p.y as f32 * 0.05).sin()) / 3.0;
        let tint = 0.86 + elev.min(8.0) * 0.04 + macro_var * 0.035;
        out.push(
            SpriteInstance::new(center, region.size, region.anchor, region.uv, SpriteDepth::terrain_block(p))
                .with_color(Color::rgb(tint, tint, tint))
                .with_flags(flags),
        );

        // Additional cliff segments where the front neighbours are more than one level lower.
        // Beyond the map edge the terrain is shown down to level 0 as a solid base.
        let front_level = |q: IVec2| map.get(q).map(|t| t.elevation).unwrap_or(0);
        let lowest_front = front_level(p + IVec2::new(1, 0)).min(front_level(p + IVec2::new(0, 1)));
        let drop = tile.elevation.saturating_sub(lowest_front);
        if drop > 1 {
            let cliff = assets.region(self.sprites.cliffs[t_index]);
            let depth = SpriteDepth { flat: SpriteDepth::OVERLAY.flat, tent: SpriteDepth::terrain_block(p).tent };
            for k in 1..drop {
                let pos = center + Vec2::new(0.0, iso::HALF_TILE_HEIGHT * 2.0 + iso::ELEVATION_STEP * (k - 1) as f32);
                out.push(SpriteInstance::new(pos, cliff.size, cliff.anchor, cliff.uv, depth));
            }
        }

        // Transition fringes from neighbouring terrain with higher priority on the same level.
        let mut bias = iso::DECAL_DEPTH_BIAS;
        for (d, (offset, _)) in EDGE_DIRS.iter().enumerate() {
            let Some(n) = map.get(p + *offset) else { continue };
            if n.elevation != tile.elevation || n.terrain == tile.terrain {
                continue;
            }
            if n.terrain.transition_priority() <= tile.terrain.transition_priority() {
                continue;
            }
            let edge = assets.region(self.sprites.edges[n.terrain as usize][d]);
            let depth = SpriteDepth::ground(p.as_vec2() + Vec2::splat(0.5), bias);
            bias += 0.001;
            let edge_flags = if n.terrain.is_water() { FLAG_WATER } else { FLAG_GROUND };
            out.push(SpriteInstance::new(center, edge.size, edge.anchor, edge.uv, depth).with_flags(edge_flags));
        }
    }

    /// Queues all terrain chunks (the renderer culls invisible ones) and the decals of visible chunks.
    pub fn draw(&self, list: &mut RenderList) {
        for handle in self.chunks.iter().flatten() {
            list.draw_static(*handle);
        }
        let view = list.visible_rect();
        for (bounds, decals) in &self.decals {
            if bounds.intersects(&view) {
                for d in decals {
                    list.draw_transparent(self.sprites.texture, *d);
                }
            }
        }
    }
}
