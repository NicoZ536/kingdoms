//! The other kingdoms on the map: their settlements drawn from the simulation's structure list
//! (`Kingdom::structures`, building sprites in the orientation of their rotation, lightly tinted
//! in the kingdom's colour, a banner in front of castles and watchtowers) and their borders as
//! lines in the kingdom's colour over a light band of tinted border tiles (`LandMap::owner`),
//! cached per chunk and rebuilt for the chunks
//! the simulation reports as changed ([`Session::take_land_dirty_chunks`]).

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::{AssetManager, SpriteRegion};
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, OBJECT_DEPTH_BIAS, SpriteDepth};
use kingdoms_engine::rendering::sprite::{RenderList, SpriteInstance};
use kingdoms_sim::defs::{BuildingRole, Defs};
use kingdoms_sim::diplomacy::LandMap;
use kingdoms_sim::grid::Direction;
use kingdoms_sim::state::GameState;
use kingdoms_sim::world::WorldMap;

use super::BORDER_BIAS;
use super::buildings::{BuildingSprites, Part, footprint_elevation, footprint_may_be_visible, sprite_flipped};
use super::chunks::chunk_tiles;
use super::overlay::OverlaySprites;
use crate::ui::theme::KINGDOM_COLORS;

/// Share of the kingdom colour in the tint of its buildings.
const TINT: f32 = 0.16;
/// Opacity of the border lines.
const BORDER_ALPHA: f32 = 1.0;
/// Opacity of the band of tiles along the inside of a border.
const BAND_ALPHA: f32 = 0.24;

/// Linear colour of kingdom colour index `color` with `alpha`.
pub fn kingdom_rgba(color: u8, alpha: f32) -> Color {
    let [r, g, b] = KINGDOM_COLORS[usize::from(color) % KINGDOM_COLORS.len()];
    Color::from_srgb_u8(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Tint of a kingdom's buildings: white with a touch of its colour.
pub fn building_tint(color: u8) -> Color {
    let c = kingdom_rgba(color, 1.0);
    let mix = |w: f32| 1.0 - TINT + TINT * w;
    Color::rgb(mix(c.r), mix(c.g), mix(c.b))
}

/// Border edges of the tiles in `tiles`: (tile, side, owner colour) wherever a tile owned by a
/// kingdom meets a tile it does not own.
pub fn border_edges(land: &LandMap, colors: &[u8], tiles: impl Iterator<Item = IVec2>) -> Vec<(IVec2, Direction, u8)> {
    let mut edges = Vec::new();
    for p in tiles {
        let Some(owner) = land.owner(p) else { continue };
        let color = colors.get(owner.index()).copied().unwrap_or(0);
        for dir in Direction::ALL {
            if land.owner(p + dir.offset()) != Some(owner) {
                edges.push((p, dir, color));
            }
        }
    }
    edges
}

/// The kingdoms layer.
pub struct KingdomLayer {
    /// Border lines per chunk with their bounds.
    chunks: Vec<(Rect, Vec<SpriteInstance>)>,
    chunk_count: IVec2,
    /// Banner in front of castles and watchtowers (`characters/banner`, white cloth tinted).
    banner: Option<SpriteRegion>,
    drawn: usize,
}

impl KingdomLayer {
    pub fn new(assets: &AssetManager) -> Self {
        let banner = assets.try_sprite("characters/banner").map(|id| *assets.region(id));
        Self { chunks: Vec::new(), chunk_count: IVec2::ZERO, banner, drawn: 0 }
    }

    /// Structures drawn last frame.
    pub fn drawn(&self) -> usize {
        self.drawn
    }

    /// Border segments cached.
    pub fn segment_count(&self) -> usize {
        self.chunks.iter().map(|(_, c)| c.len()).sum()
    }

    /// Rebuilds the border lines of every chunk.
    pub fn rebuild_all(&mut self, state: &GameState, overlay: &OverlaySprites) {
        let map = state.world();
        self.chunk_count = map.chunk_count();
        self.chunks = vec![(Rect::EMPTY, Vec::new()); (self.chunk_count.x * self.chunk_count.y).max(0) as usize];
        for cy in 0..self.chunk_count.y {
            for cx in 0..self.chunk_count.x {
                self.rebuild_chunk(IVec2::new(cx, cy), state, overlay);
            }
        }
    }

    /// Rebuilds the border lines of `chunks` and their neighbours (a border runs along the
    /// edge between two chunks).
    pub fn rebuild_chunks(&mut self, chunks: &[IVec2], state: &GameState, overlay: &OverlaySprites) {
        if self.chunk_count != state.world().chunk_count() {
            self.rebuild_all(state, overlay);
            return;
        }
        let mut todo: Vec<IVec2> = Vec::new();
        for c in chunks {
            for d in [IVec2::ZERO, IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
                if !todo.contains(&(*c + d)) {
                    todo.push(*c + d);
                }
            }
        }
        for c in todo {
            self.rebuild_chunk(c, state, overlay);
        }
    }

    fn rebuild_chunk(&mut self, chunk: IVec2, state: &GameState, overlay: &OverlaySprites) {
        if chunk.x < 0 || chunk.y < 0 || chunk.x >= self.chunk_count.x || chunk.y >= self.chunk_count.y {
            return;
        }
        let map: &WorldMap = state.world();
        let colors: Vec<u8> = state.diplomacy().kingdoms().map(|(_, k)| k.color()).collect();
        let edges = border_edges(state.diplomacy().land(), &colors, chunk_tiles(chunk, map.size()));
        let mut lines: Vec<SpriteInstance> = Vec::with_capacity(edges.len() * 2);
        let mut banded: Vec<IVec2> = Vec::new();
        for (p, dir, color) in edges {
            let elevation = f32::from(map.tile(p).elevation);
            // A light tinted band on the border tiles, then the line in the kingdom colour.
            if banded.last() != Some(&p) {
                banded.push(p);
                lines.push(overlay.fill(p, elevation, kingdom_rgba(color, BAND_ALPHA), BORDER_BIAS - 0.002));
            }
            lines.push(overlay.edge(p, elevation, dir, kingdom_rgba(color, BORDER_ALPHA), BORDER_BIAS));
        }
        let bounds = lines.iter().fold(Rect::EMPTY, |r, l| r.union(&l.world_bounds()));
        self.chunks[(chunk.y * self.chunk_count.x + chunk.x) as usize] = (bounds, lines);
    }

    /// Queues the border lines of the visible chunks and the structures in view.
    pub fn draw(&mut self, list: &mut RenderList, state: &GameState, defs: &Defs, sprites: &BuildingSprites, assets: &AssetManager, overlay: &OverlaySprites) {
        let view = list.visible_rect();
        for (bounds, lines) in &self.chunks {
            if bounds.intersects(&view) {
                for line in lines {
                    list.draw_transparent(overlay.texture, *line);
                }
            }
        }
        self.drawn = 0;
        let map = state.world();
        for (_, k) in state.diplomacy().kingdoms() {
            let tint = building_tint(k.color());
            for s in k.structures() {
                let rect = s.rect(defs);
                if !footprint_may_be_visible(rect, &view) {
                    continue;
                }
                let Some((sprite, dims)) = sprites.finished(s.kind) else { continue };
                let elevation = footprint_elevation(map, rect);
                let part = Part { sprite, rect, elevation, flip: sprite_flipped(dims, rect.size, s.rotation.facing()), bias: OBJECT_DEPTH_BIAS };
                let (instance, texture) = part.instance(assets);
                list.draw(texture, instance.with_color(tint).with_flags(super::buildings::SNOW_FLAGS));
                self.drawn += 1;
                let role = defs.buildings().get(s.kind.index()).map(|d| (d.role, d.key.as_str()));
                let flagged = matches!(role, Some((BuildingRole::Headquarters, _)) | Some((_, "watchtower" | "stone_tower")));
                if let (true, Some(banner)) = (flagged, self.banner) {
                    // Planted just in front of the footprint's front corner.
                    let foot = (rect.origin + rect.size).as_vec2() + Vec2::splat(0.2);
                    let pos = iso::tile_to_world_elevated(foot, elevation);
                    let depth = SpriteDepth::upright_point(foot, OBJECT_DEPTH_BIAS);
                    let flag = SpriteInstance::new(pos, banner.size, banner.anchor, banner.uv, depth).with_color(kingdom_rgba(k.color(), 1.0));
                    list.draw(banner.texture, flag);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    #[test]
    fn borders_follow_the_land_map() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = Defs::load(&dir).unwrap_or_else(|e| panic!("shipped game data must load: {e}"));
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let colors: Vec<u8> = state.diplomacy().kingdoms().map(|(_, k)| k.color()).collect();
        let size = state.world().size();
        let all = (0..size.y).flat_map(|y| (0..size.x).map(move |x| IVec2::new(x, y)));
        let edges = border_edges(state.diplomacy().land(), &colors, all);
        assert!(!edges.is_empty(), "kingdoms own land at the start");
        // Every edge separates an owned tile from one of another owner, in the owner's colour.
        let land = state.diplomacy().land();
        for (p, d, color) in &edges {
            let owner = land.owner(*p).unwrap_or_else(|| panic!("edge of an unowned tile"));
            assert_ne!(land.owner(*p + d.offset()), Some(owner));
            assert_eq!(*color, colors[owner.index()]);
        }
        // Each kingdom's seat lies inside its own land.
        for (id, k) in state.diplomacy().kingdoms() {
            assert_eq!(land.owner(k.seat()), Some(id));
        }
        // The tint keeps buildings light, with a hint of the colour.
        let t = building_tint(0);
        assert!(t.r > 0.8 && t.g > 0.8 && t.b > 0.8 && t.r > t.b);
    }
}
