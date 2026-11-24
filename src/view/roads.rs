//! Road layer: road surfaces per type with their 16 connection masks and bridges, cached as
//! per-chunk static batches of the `roads` atlas.
//!
//! Change detection comes from the simulation: [`GameState::structure_revision`] tells cheaply
//! whether roads, walls or building footprints changed at all, and
//! [`GameState::take_structure_dirty_chunks`] names the chunks to rebuild (including the
//! neighbours whose connection masks depend on a change). Chunks whose terrain changed are
//! rebuilt as well (entrance reachability, surface height).

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::AssetManager;
use kingdoms_engine::rendering::Renderer;
use kingdoms_engine::rendering::iso::{self, SpriteDepth};
use kingdoms_engine::rendering::sprite::{FLAG_GROUND, FLAG_TRODDEN, RenderList, SpriteInstance};
use kingdoms_sim::defs::Defs;
use kingdoms_sim::state::GameState;

use super::ROAD_BIAS;
use super::chunks::{ChunkBatches, chunk_tiles};

/// Renderer of the road layer.
pub struct RoadRenderer {
    batches: ChunkBatches,
}

impl RoadRenderer {
    pub fn new(assets: &AssetManager) -> Self {
        let texture = assets.atlas_texture("roads").unwrap_or_else(|| assets.region(assets.missing_sprite()).texture);
        Self { batches: ChunkBatches::new(texture) }
    }

    /// Sprites in all chunks.
    pub fn instance_count(&self) -> usize {
        self.batches.instance_count()
    }

    /// Frees every GPU batch (before the layer is dropped).
    pub fn release(&mut self, renderer: &mut Renderer) {
        self.batches.release(renderer);
    }

    /// Rebuilds every chunk.
    pub fn rebuild_all(&mut self, state: &GameState, defs: &Defs, renderer: &mut Renderer, assets: &AssetManager) {
        self.batches.reset(state.world().chunk_count(), renderer);
        let chunks: Vec<IVec2> = self.batches.all_chunks().collect();
        for chunk in chunks {
            self.rebuild_chunk(chunk, state, defs, renderer, assets);
        }
    }

    /// Rebuilds the chunks whose structures (`structures`) or terrain (`terrain`) changed.
    pub fn refresh(
        &mut self,
        state: &GameState,
        defs: &Defs,
        structures: &[IVec2],
        terrain: &[IVec2],
        renderer: &mut Renderer,
        assets: &AssetManager,
    ) {
        if self.batches.chunk_count() != state.world().chunk_count() {
            self.rebuild_all(state, defs, renderer, assets);
            return;
        }
        let mut rebuilt: Vec<IVec2> = Vec::with_capacity(structures.len() + terrain.len());
        for &chunk in structures.iter().chain(terrain) {
            if !rebuilt.contains(&chunk) {
                rebuilt.push(chunk);
                self.rebuild_chunk(chunk, state, defs, renderer, assets);
            }
        }
    }

    fn rebuild_chunk(&mut self, chunk: IVec2, state: &GameState, defs: &Defs, renderer: &mut Renderer, assets: &AssetManager) {
        let world = state.world();
        let mut out = Vec::new();
        for p in chunk_tiles(chunk, world.size()) {
            let (Some(tile), Some(mask)) = (state.roads().get(p), state.road_mask(p)) else { continue };
            let elevation = f32::from(world.tile(p).elevation);
            let r = assets.region(assets.sprite(&tile.sprite(defs, mask)));
            let instance = if tile.is_bridge() {
                // Upright 1×1 sprite: bottom edge on the tile's front corner.
                let pos = iso::tile_to_world_elevated(p.as_vec2() + Vec2::ONE, elevation);
                SpriteInstance::new(pos, r.size, r.anchor, r.uv, SpriteDepth::footprint(p, glam::UVec2::ONE, ROAD_BIAS))
            } else {
                let centre = p.as_vec2() + Vec2::splat(0.5);
                let pos = iso::tile_to_world_elevated(centre, elevation);
                SpriteInstance::new(pos, r.size, r.anchor, r.uv, SpriteDepth::ground(centre, ROAD_BIAS))
                    .with_flags(FLAG_GROUND | FLAG_TRODDEN)
            };
            out.push(instance);
        }
        self.batches.set(chunk, &out, renderer);
    }

    /// Queues all chunk batches.
    pub fn draw(&self, list: &mut RenderList) {
        self.batches.draw(list);
    }
}
