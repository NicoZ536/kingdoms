//! Ground overlays: tile edges and fills (territory border, footprint highlights, hover, grid).
//!
//! Edge lines reuse the `overlay/tile_outline` sprite: each quarter of it holds exactly one
//! diamond edge, so a single tile edge is drawn as that quarter (32 × 16 px) next to the tile
//! centre. Edges and fills are alpha-blended ground decals.

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::AssetManager;
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, SpriteDepth};
use kingdoms_engine::rendering::sprite::{FLAG_EMISSIVE, RenderList, SpriteInstance};
use kingdoms_engine::rendering::texture::TextureId;
use kingdoms_sim::grid::{Direction, TileRect};
use kingdoms_sim::territory::Territory;
use kingdoms_sim::world::WorldMap;

use super::{BORDER_BIAS, HIGHLIGHT_BIAS};
use super::chunks::chunk_tiles;

/// Colour of the territory border (amber, distinct from the pale terrain rims).
const BORDER_COLOR: Color = Color { r: 1.0, g: 0.62, b: 0.12, a: 0.6 };

/// A quarter of the outline sprite: one tile edge.
#[derive(Clone, Copy, Debug)]
struct EdgeSprite {
    uv: [f32; 4],
    /// Anchor relative to the 32 × 16 quarter, placed at the tile centre.
    anchor: Vec2,
}

/// Sprites of the `overlay` atlas used for highlights.
pub struct OverlaySprites {
    pub texture: TextureId,
    /// Indexed by [`Direction::index`] (N, E, S, W).
    edges: [EdgeSprite; 4],
    solid: kingdoms_engine::assets::SpriteRegion,
    pub grid: kingdoms_engine::assets::SpriteRegion,
    /// Soft round dot (route lines, markers).
    pub dot: kingdoms_engine::assets::SpriteRegion,
}

impl OverlaySprites {
    pub fn load(assets: &AssetManager) -> Self {
        let outline = *assets.region(assets.sprite("overlay/tile_outline"));
        let [u0, v0, u1, v1] = outline.uv;
        let (um, vm) = ((u0 + u1) * 0.5, (v0 + v1) * 0.5);
        // N = upper-right edge, E = lower-right, S = lower-left, W = upper-left.
        let edges = [
            EdgeSprite { uv: [um, v0, u1, vm], anchor: Vec2::new(0.0, 1.0) },
            EdgeSprite { uv: [um, vm, u1, v1], anchor: Vec2::new(0.0, 0.0) },
            EdgeSprite { uv: [u0, vm, um, v1], anchor: Vec2::new(1.0, 0.0) },
            EdgeSprite { uv: [u0, v0, um, vm], anchor: Vec2::new(1.0, 1.0) },
        ];
        Self {
            texture: outline.texture,
            edges,
            solid: *assets.region(assets.sprite("overlay/tile_solid")),
            grid: *assets.region(assets.sprite("overlay/grid")),
            dot: *assets.region(assets.sprite("overlay/soft_dot")),
        }
    }

    /// One tile edge as a ground decal.
    pub fn edge(&self, p: IVec2, elevation: f32, dir: Direction, color: Color, bias: f32) -> SpriteInstance {
        let e = self.edges[dir.index()];
        let centre = p.as_vec2() + Vec2::splat(0.5);
        let size = Vec2::new(iso::HALF_TILE_WIDTH, iso::HALF_TILE_HEIGHT);
        SpriteInstance::new(iso::tile_to_world_elevated(centre, elevation), size, e.anchor, e.uv, SpriteDepth::ground(centre, bias))
            .with_color(color)
            .with_flags(FLAG_EMISSIVE)
    }

    /// A filled tile as a ground decal.
    pub fn fill(&self, p: IVec2, elevation: f32, color: Color, bias: f32) -> SpriteInstance {
        let centre = p.as_vec2() + Vec2::splat(0.5);
        let r = &self.solid;
        SpriteInstance::new(iso::tile_to_world_elevated(centre, elevation), r.size, r.anchor, r.uv, SpriteDepth::ground(centre, bias))
            .with_color(color)
            .with_flags(FLAG_EMISSIVE)
    }

    /// A flat dot of `size` world pixels on the ground at tile-space point `p` (route lines).
    pub fn ground_dot(&self, p: Vec2, elevation: f32, size: Vec2, color: Color, bias: f32) -> SpriteInstance {
        let d = &self.dot;
        SpriteInstance::new(iso::tile_to_world_elevated(p, elevation), size, Vec2::splat(0.5), d.uv, SpriteDepth::ground(p, bias))
            .with_color(color)
            .with_flags(FLAG_EMISSIVE)
    }

    /// Outline and translucent fill of a footprint. The outline lies on the ground (hidden
    /// where the building stands in front of it); `style.ghost` repeats it on top of everything
    /// so the whole footprint stays recognisable behind tall sprites.
    pub fn footprint(&self, list: &mut RenderList, map: &WorldMap, rect: TileRect, style: &HighlightStyle) {
        for p in rect.tiles() {
            let Some(tile) = map.get(p) else { continue };
            let elevation = f32::from(tile.elevation);
            if style.fill.a > 0.0 {
                list.draw_transparent(self.texture, self.fill(p, elevation, style.fill, HIGHLIGHT_BIAS));
            }
            for dir in Direction::ALL {
                if rect.contains(p + dir.offset()) {
                    continue;
                }
                let edge = self.edge(p, elevation, dir, style.outline, HIGHLIGHT_BIAS + 0.003);
                list.draw_transparent(self.texture, edge);
                if style.ghost.a > 0.0 {
                    let mut ghost = edge.with_color(style.ghost);
                    ghost.depth_flat = SpriteDepth::OVERLAY.flat;
                    ghost.depth_tent = SpriteDepth::OVERLAY.tent;
                    list.draw_transparent(self.texture, ghost);
                }
            }
        }
    }
}

/// Colours of a footprint highlight.
#[derive(Clone, Copy, Debug)]
pub struct HighlightStyle {
    /// Outline on the ground.
    pub outline: Color,
    /// Fill of the footprint tiles.
    pub fill: Color,
    /// Outline drawn over everything (alpha 0 = none).
    pub ghost: Color,
}

impl HighlightStyle {
    /// The player's selection.
    pub const SELECTED: HighlightStyle = HighlightStyle {
        outline: Color { r: 1.0, g: 0.78, b: 0.25, a: 1.0 },
        fill: Color { r: 1.0, g: 0.85, b: 0.4, a: 0.18 },
        ghost: Color { r: 1.0, g: 0.82, b: 0.35, a: 0.35 },
    };
    /// A building under the cursor.
    pub const HOVERED_BUILDING: HighlightStyle = HighlightStyle {
        outline: Color { r: 1.0, g: 0.97, b: 0.85, a: 0.85 },
        fill: Color { r: 1.0, g: 0.95, b: 0.7, a: 0.1 },
        ghost: Color { r: 1.0, g: 0.97, b: 0.85, a: 0.2 },
    };
    /// A tile under the cursor.
    pub const HOVERED_TILE: HighlightStyle = HighlightStyle {
        outline: Color { r: 1.0, g: 0.97, b: 0.85, a: 0.75 },
        fill: Color { r: 1.0, g: 0.95, b: 0.6, a: 0.3 },
        ghost: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 },
    };
}

/// Cached border lines of the player's territory, per chunk.
pub struct TerritoryBorder {
    chunks: Vec<(Rect, Vec<SpriteInstance>)>,
    chunk_count: IVec2,
}

impl TerritoryBorder {
    pub fn new() -> Self {
        Self { chunks: Vec::new(), chunk_count: IVec2::ZERO }
    }

    /// Number of border segments.
    pub fn segment_count(&self) -> usize {
        self.chunks.iter().map(|(_, c)| c.len()).sum()
    }

    /// Rebuilds the lines of every chunk.
    pub fn rebuild_all(&mut self, map: &WorldMap, territory: &Territory, sprites: &OverlaySprites) {
        self.chunk_count = map.chunk_count();
        self.chunks = vec![(Rect::EMPTY, Vec::new()); (self.chunk_count.x * self.chunk_count.y).max(0) as usize];
        for cy in 0..self.chunk_count.y {
            for cx in 0..self.chunk_count.x {
                self.rebuild_chunk(IVec2::new(cx, cy), map, territory, sprites);
            }
        }
    }

    /// Rebuilds the lines of the given chunks (terrain changed there).
    pub fn rebuild_chunks(&mut self, chunks: &[IVec2], map: &WorldMap, territory: &Territory, sprites: &OverlaySprites) {
        if self.chunk_count != map.chunk_count() {
            self.rebuild_all(map, territory, sprites);
            return;
        }
        for &chunk in chunks {
            self.rebuild_chunk(chunk, map, territory, sprites);
        }
    }

    fn rebuild_chunk(&mut self, chunk: IVec2, map: &WorldMap, territory: &Territory, sprites: &OverlaySprites) {
        if chunk.x < 0 || chunk.y < 0 || chunk.x >= self.chunk_count.x || chunk.y >= self.chunk_count.y {
            return;
        }
        let mut lines = Vec::new();
        for p in chunk_tiles(chunk, map.size()) {
            if !territory.contains(p) {
                continue;
            }
            let elevation = f32::from(map.tile(p).elevation);
            for dir in Direction::ALL {
                if !territory.contains(p + dir.offset()) {
                    lines.push(sprites.edge(p, elevation, dir, BORDER_COLOR, BORDER_BIAS));
                }
            }
        }
        let bounds = lines.iter().fold(Rect::EMPTY, |r, l| r.union(&l.world_bounds()));
        self.chunks[(chunk.y * self.chunk_count.x + chunk.x) as usize] = (bounds, lines);
    }

    /// Queues the lines of the visible chunks.
    pub fn draw(&self, list: &mut RenderList, texture: TextureId) {
        let view = list.visible_rect();
        for (bounds, lines) in &self.chunks {
            if bounds.intersects(&view) {
                for line in lines {
                    list.draw_transparent(texture, *line);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_quarters_touch_the_matching_diamond_edge() {
        // Each quarter, anchored at the tile centre, spans the box between the centre and the
        // two corners of its edge.
        let centre = iso::tile_to_world(Vec2::splat(0.5));
        let size = Vec2::new(32.0, 16.0);
        let anchors = [Vec2::new(0.0, 1.0), Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)];
        for (dir, anchor) in Direction::ALL.into_iter().zip(anchors) {
            let min = centre - anchor * size;
            let max = min + size;
            // Corners of the edge in tile space: the edge on side `dir` of tile (0,0).
            let (a, b) = match dir {
                Direction::North => (Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)),
                Direction::East => (Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)),
                Direction::South => (Vec2::new(0.0, 1.0), Vec2::new(1.0, 1.0)),
                Direction::West => (Vec2::new(0.0, 0.0), Vec2::new(0.0, 1.0)),
            };
            for corner in [a, b] {
                let w = iso::tile_to_world(corner);
                assert!(w.x >= min.x - 1e-3 && w.x <= max.x + 1e-3 && w.y >= min.y - 1e-3 && w.y <= max.y + 1e-3, "{dir:?}");
            }
        }
    }
}
