//! Field layer: the claimed field and pasture tiles of all farms (`fields` atlas), cached as
//! per-chunk static batches and rebuilt only for the chunks the simulation reports through
//! [`GameState::take_field_dirty_chunks`](kingdoms_sim::state::GameState::take_field_dirty_chunks)
//! (claimed, released, sown, harvested, new growth stage) or whose terrain changed.
//!
//! Depth: pastures and fallow soil are flat ground decals; crops and orchards are ground cover
//! ([`SpriteDepth::ground_cover`]): the tile itself is a decal above the terrain, while plants
//! and trees rising above it are drawn like objects standing on the back half of the tile, so
//! they overlap the ground, trees and buildings behind the field and are covered by everything
//! standing in front of it. All field sprites receive snow and wetness on their tile
//! ([`FLAG_GROUND`]) and snow on their green plants and trees ([`FLAG_SNOW_ON_GREEN`]). Crops
//! and orchard trees sway in the wind ([`FLAG_WIND`] on a ground sprite: the shader shears the
//! plants above the tile diamond by their height, with a phase per tile); in winter orchards
//! past the sapling stage show bare trees
//! ([`FieldView::seasonal_sprite`](kingdoms_sim::agriculture::FieldView::seasonal_sprite),
//! `fields/orchard_bare`; the simulation reports their chunks dirty when winter begins and
//! ends).

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::AssetManager;
use kingdoms_engine::rendering::Renderer;
use kingdoms_engine::rendering::iso::{self, SpriteDepth};
use kingdoms_engine::rendering::sprite::{
    FLAG_DEPTH_MAX, FLAG_GROUND, FLAG_SNOW_ON_GREEN, FLAG_WIND, RenderList, SpriteInstance,
};
use kingdoms_sim::agriculture::{FarmKind, FieldView};
use kingdoms_sim::defs::Defs;
use kingdoms_sim::grid::TileRect;
use kingdoms_sim::state::GameState;
use kingdoms_sim::world::CHUNK_SIZE;

use super::{FIELD_BIAS, hash01, tile_hash};
use super::chunks::ChunkBatches;

/// Ground depth of the plant floor relative to the tile's back corner (a quarter into the tile),
/// so objects standing on most of a field (depth above it) cover its plants.
const PLANT_FLOOR: f32 = 0.5;

/// How a field tile is layered (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldDepth {
    /// Flat ground decal (pasture, fallow soil).
    Flat,
    /// Ground cover with standing plants (crops, orchards).
    Cover,
}

/// Depth mode of a field tile.
pub fn field_depth(view: &FieldView) -> FieldDepth {
    match (view.kind, view.crop) {
        (FarmKind::Fields, Some(_)) => FieldDepth::Cover,
        _ => FieldDepth::Flat,
    }
}

/// Sprite depth and extra flags of field tile `p`.
pub fn field_depth_params(p: IVec2, mode: FieldDepth) -> (SpriteDepth, u32) {
    let centre = p.as_vec2() + Vec2::splat(0.5);
    match mode {
        FieldDepth::Flat => (SpriteDepth::ground(centre, FIELD_BIAS), FLAG_GROUND),
        FieldDepth::Cover => {
            let floor = iso::ground_depth(p.as_vec2()) + PLANT_FLOOR;
            (SpriteDepth::ground_cover(centre, floor, FIELD_BIAS), FLAG_GROUND | FLAG_DEPTH_MAX | FLAG_SNOW_ON_GREEN | FLAG_WIND)
        }
    }
}

/// Renderer of the field layer.
pub struct FieldRenderer {
    batches: ChunkBatches,
}

impl FieldRenderer {
    pub fn new(assets: &AssetManager) -> Self {
        let texture = assets.atlas_texture("fields").unwrap_or_else(|| assets.region(assets.missing_sprite()).texture);
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

    /// Rebuilds the given chunks (everything if the map size changed).
    pub fn rebuild_chunks(
        &mut self,
        chunks: &[IVec2],
        state: &GameState,
        defs: &Defs,
        renderer: &mut Renderer,
        assets: &AssetManager,
    ) {
        if self.batches.chunk_count() != state.world().chunk_count() {
            self.rebuild_all(state, defs, renderer, assets);
            return;
        }
        for &chunk in chunks {
            self.rebuild_chunk(chunk, state, defs, renderer, assets);
        }
    }

    fn rebuild_chunk(&mut self, chunk: IVec2, state: &GameState, defs: &Defs, renderer: &mut Renderer, assets: &AssetManager) {
        let world = state.world();
        let rect = TileRect::new(chunk * CHUNK_SIZE, IVec2::splat(CHUNK_SIZE));
        let season = state.clock().season();
        let mut out = Vec::new();
        for (p, view) in state.agriculture().fields_in(rect) {
            let Some(tile) = world.get(p) else { continue };
            let r = assets.region(assets.sprite(&view.seasonal_sprite(defs, season)));
            let pos = iso::tile_center_world(p, f32::from(tile.elevation));
            let (depth, flags) = field_depth_params(p, field_depth(&view));
            // Neighbouring tiles sway out of step.
            let phase = hash01(tile_hash(p), 7) * std::f32::consts::TAU;
            out.push(SpriteInstance::new(pos, r.size, r.anchor, r.uv, depth).with_flags(flags).with_phase(phase));
        }
        self.batches.set(chunk, &out, renderer);
    }

    /// Queues all chunk batches.
    pub fn draw(&self, list: &mut RenderList) {
        self.batches.draw(list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_engine::rendering::iso::OBJECT_DEPTH_BIAS;
    use kingdoms_sim::buildings::BuildingId;
    use kingdoms_sim::defs::CropId;

    fn view(kind: FarmKind, crop: Option<CropId>) -> FieldView {
        FieldView { farm: BuildingId::from_raw_parts(0, 0), kind, crop, stage: 2 }
    }

    #[test]
    fn crops_are_ground_cover_and_pastures_flat() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match Defs::load(&dir) {
            Ok(d) => d,
            Err(e) => panic!("shipped data must load: {e}"),
        };
        let wheat = defs.crops().first().map(|c| c.id);
        assert!(wheat.is_some());
        assert_eq!(field_depth(&view(FarmKind::Pasture, None)), FieldDepth::Flat);
        assert_eq!(field_depth(&view(FarmKind::Fields, None)), FieldDepth::Flat);
        assert_eq!(field_depth(&view(FarmKind::Fields, wheat)), FieldDepth::Cover);
        assert_eq!(view(FarmKind::Pasture, None).sprite(&defs), "fields/pasture");
        let (_, flat_flags) = field_depth_params(IVec2::new(3, 4), FieldDepth::Flat);
        assert_eq!(flat_flags, FLAG_GROUND);
        let (_, cover_flags) = field_depth_params(IVec2::new(3, 4), FieldDepth::Cover);
        assert_eq!(cover_flags, FLAG_GROUND | FLAG_DEPTH_MAX | FLAG_SNOW_ON_GREEN | FLAG_WIND);
    }

    #[test]
    fn orchards_are_bare_in_winter_and_the_sprite_exists() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match Defs::load(&dir) {
            Ok(d) => d,
            Err(e) => panic!("shipped data must load: {e}"),
        };
        let Some(orchard) = defs.crops().iter().find(|c| c.perennial) else { panic!("no orchard crop") };
        let grown = FieldView { stage: 3, ..view(FarmKind::Fields, Some(orchard.id)) };
        let sapling = FieldView { stage: 0, ..grown };
        use kingdoms_sim::time::Season;
        let bare = grown.seasonal_sprite(&defs, Season::Winter);
        assert_eq!(grown.seasonal_sprite(&defs, Season::Summer), grown.sprite(&defs));
        assert_eq!(sapling.seasonal_sprite(&defs, Season::Winter), sapling.sprite(&defs));
        assert_ne!(bare, grown.sprite(&defs));
        // The atlas ships the bare sprite under exactly that name.
        let atlas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/atlases/fields.ron");
        let text = match std::fs::read_to_string(&atlas) {
            Ok(t) => t,
            Err(e) => panic!("fields.ron must exist: {e}"),
        };
        let name = bare.trim_start_matches("fields/");
        assert!(text.contains(&format!("\"{name}\"")), "{name} missing in fields.ron");
    }

    #[test]
    fn plants_cover_what_stands_behind_and_not_what_stands_in_front() {
        let p = IVec2::new(10, 12);
        let (depth, _) = field_depth_params(p, FieldDepth::Cover);
        let terrain = SpriteDepth::terrain_block(p);
        // The whole tile diamond stays above its own terrain (front corner +16 px, back −16 px).
        for rel_y in [-16.0, -8.0, 0.0, 8.0, 16.0] {
            assert!(depth.evaluate_max(0.5, rel_y) > terrain.evaluate(0.5, rel_y), "rel_y {rel_y}");
        }
        // A tree on the tile behind and a building whose front corner touches the field's back
        // corner are covered by the plants rising 30 px above the tile.
        let plants = depth.evaluate_max(0.5, -30.0);
        let tree_behind = SpriteDepth::upright_point(Vec2::new(9.5, 12.5), OBJECT_DEPTH_BIAS);
        let house_behind = SpriteDepth::footprint(IVec2::new(8, 10), glam::UVec2::new(2, 2), OBJECT_DEPTH_BIAS);
        assert!(plants > tree_behind.evaluate(0.5, 0.0));
        assert!(plants > house_behind.evaluate(0.5, 0.0));
        // An object standing on the front half of the tile or on the tile in front wins.
        let walker = SpriteDepth::upright_point(Vec2::new(10.6, 12.7), OBJECT_DEPTH_BIAS);
        let tree_front = SpriteDepth::upright_point(Vec2::new(11.5, 12.5), OBJECT_DEPTH_BIAS);
        assert!(walker.evaluate(0.5, 0.0) > plants);
        assert!(tree_front.evaluate(0.5, 0.0) > depth.evaluate_max(0.5, 16.0));
    }
}
