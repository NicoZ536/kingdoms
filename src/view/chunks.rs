//! Per-chunk static sprite batches (one GPU batch per map chunk and layer).

use glam::IVec2;
use kingdoms_engine::rendering::Renderer;
use kingdoms_engine::rendering::sprite::{RenderList, SpriteInstance, StaticBatchHandle};
use kingdoms_engine::rendering::texture::TextureId;
use kingdoms_sim::world::CHUNK_SIZE;

/// Tiles of `chunk` inside a map of `map_size`, row-major.
pub fn chunk_tiles(chunk: IVec2, map_size: IVec2) -> impl Iterator<Item = IVec2> {
    let origin = chunk * CHUNK_SIZE;
    let end = (origin + IVec2::splat(CHUNK_SIZE)).min(map_size);
    (origin.y.max(0)..end.y).flat_map(move |y| (origin.x.max(0)..end.x).map(move |x| IVec2::new(x, y)))
}

/// Static batches of one layer, one per chunk, all using the same atlas texture.
pub struct ChunkBatches {
    texture: TextureId,
    chunk_count: IVec2,
    handles: Vec<Option<StaticBatchHandle>>,
    counts: Vec<usize>,
}

impl ChunkBatches {
    pub fn new(texture: TextureId) -> Self {
        Self { texture, chunk_count: IVec2::ZERO, handles: Vec::new(), counts: Vec::new() }
    }

    /// Drops all batches and prepares `chunk_count` empty slots.
    pub fn reset(&mut self, chunk_count: IVec2, renderer: &mut Renderer) {
        for handle in self.handles.drain(..).flatten() {
            renderer.remove_static_batch(handle);
        }
        self.chunk_count = chunk_count.max(IVec2::ZERO);
        let n = (self.chunk_count.x * self.chunk_count.y) as usize;
        self.handles = vec![None; n];
        self.counts = vec![0; n];
    }

    /// Frees every GPU batch (before the layer is dropped).
    pub fn release(&mut self, renderer: &mut Renderer) {
        self.reset(IVec2::ZERO, renderer);
    }

    pub fn chunk_count(&self) -> IVec2 {
        self.chunk_count
    }

    fn slot(&self, chunk: IVec2) -> Option<usize> {
        let inside = chunk.x >= 0 && chunk.y >= 0 && chunk.x < self.chunk_count.x && chunk.y < self.chunk_count.y;
        inside.then(|| (chunk.y * self.chunk_count.x + chunk.x) as usize)
    }

    /// Replaces the sprites of `chunk` (ignored outside the map).
    pub fn set(&mut self, chunk: IVec2, instances: &[SpriteInstance], renderer: &mut Renderer) {
        let Some(slot) = self.slot(chunk) else { return };
        match self.handles[slot] {
            Some(handle) if renderer.update_static_batch(handle, self.texture, instances) => {}
            _ => self.handles[slot] = Some(renderer.create_static_batch(self.texture, instances)),
        }
        self.counts[slot] = instances.len();
    }

    /// Queues every batch; the renderer culls the invisible ones.
    pub fn draw(&self, list: &mut RenderList) {
        for handle in self.handles.iter().flatten() {
            list.draw_static(*handle);
        }
    }

    /// Sprites in all batches.
    pub fn instance_count(&self) -> usize {
        self.counts.iter().sum()
    }

    /// All chunk coordinates.
    pub fn all_chunks(&self) -> impl Iterator<Item = IVec2> + use<> {
        let count = self.chunk_count;
        (0..count.y).flat_map(move |y| (0..count.x).map(move |x| IVec2::new(x, y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_tiles_are_clipped_to_the_map() {
        let size = IVec2::new(40, 33);
        assert_eq!(chunk_tiles(IVec2::ZERO, size).count(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
        let edge: Vec<IVec2> = chunk_tiles(IVec2::new(1, 1), size).collect();
        // 40 - 32 = 8 columns and 33 - 32 = 1 row.
        assert_eq!(edge.len(), 8);
        assert!(edge.iter().all(|p| p.x < size.x && p.y < size.y && p.x / CHUNK_SIZE == 1 && p.y / CHUNK_SIZE == 1));
    }
}
