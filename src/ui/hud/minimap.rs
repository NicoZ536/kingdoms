//! Minimap (bottom right, parchment frame): the map drawn as a diamond like the main view,
//! from terrain colours, vegetation, roads, buildings, the territory border and the other
//! kingdoms (their land tinted, borders and settlements in their colour). On top, redrawn every
//! frame: enemies (pulsing red, approaching ones at their entry point), own companies (gold) and
//! caravans (in their kingdom's colour).
//!
//! The pixels live in an egui texture with [`SCALE`]² pixels per tile. Only changed chunks are
//! recomputed and uploaded: chunks whose tiles changed (from the map view's dirty chunks), the
//! chunks of road changes and of buildings that appeared, vanished or changed their footprint
//! (found by comparing the building rectangles after structure events). A territory change
//! redraws everything, and so does a change of the season's look ([`SeasonLook`]): the snow
//! cover of the weather in a few steps (land turns white, forests keep dark crowns) and the
//! autumn colours of the leaf trees and meadows – a handful of redraws per season, never one
//! per frame.

use std::collections::{BTreeMap, BTreeSet};

use glam::{IVec2, Vec2};
use kingdoms_engine::egui::{self, Align2, Color32};
use kingdoms_sim::buildings::{Building, BuildingId};
use kingdoms_sim::defs::{BuildingCategory, BuildingRole, Defs};
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::grid::TileRect;
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::Season;
use kingdoms_sim::world::{CHUNK_SIZE, Terrain, Tile, TreeKind, Vegetation};

use super::{HudAction, HudInput, MARGIN};
use crate::session::{EventCursor, Session};
use crate::ui::theme::{self, Palette};

/// Texture pixels per tile edge.
pub const SCALE: i32 = 2;
/// Width of the minimap diamond in points (its height is half of it).
const WIDTH: f32 = 264.0;

/// Steps of the snow cover the minimap tells apart (a redraw for each step).
const SNOW_STEPS: u32 = 5;
/// Colour of snow on open land.
const SNOW_RGB: [f32; 3] = [236.0, 240.0, 246.0];
/// Colour of autumn leaves and of autumn meadows.
const AUTUMN_LEAVES: [f32; 3] = [168.0, 102.0, 40.0];
const AUTUMN_GRASS: [f32; 3] = [150.0, 138.0, 70.0];

/// How the season colours the minimap: the weather's snow cover in [`SNOW_STEPS`] steps and the
/// autumn foliage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeasonLook {
    /// Snow cover step, 0 (none) to [`SNOW_STEPS`] (closed).
    pub snow: u8,
    pub autumn: bool,
}

impl SeasonLook {
    pub fn of(state: &GameState) -> Self {
        Self::new(state.weather().snow_cover(), state.clock().season())
    }

    /// The look of `snow_cover` percent in `season` (a thin dusting below half a step shows no
    /// snow).
    pub fn new(snow_cover: u8, season: Season) -> Self {
        let snow = (u32::from(snow_cover.min(100)) * SNOW_STEPS + 50) / 100;
        Self { snow: u8::try_from(snow).unwrap_or(0), autumn: season == Season::Autumn }
    }

    /// Share of white on open land, 0–1.
    fn snow_share(self) -> f32 {
        f32::from(self.snow) / SNOW_STEPS as f32 * 0.95
    }
}

/// `rgb` moved towards `target` by `share` (0–1).
fn blend(rgb: [f32; 3], target: [f32; 3], share: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| rgb[i] + (target[i] - rgb[i]) * share)
}

/// Colour of a tile's terrain and vegetation in the season's look: autumn leaves and meadows,
/// then the snow cover (water stays open; trees and rocks keep some of their colour).
pub fn seasonal_rgb(tile: &Tile, look: SeasonLook) -> [f32; 3] {
    let mut rgb = natural_rgb(tile);
    if tile.is_water() {
        return rgb;
    }
    if look.autumn {
        rgb = match tile.vegetation {
            Vegetation::Tree { kind: TreeKind::Conifer, .. } => rgb,
            Vegetation::Tree { .. } => blend(rgb, AUTUMN_LEAVES, 0.6),
            Vegetation::Bush => blend(rgb, AUTUMN_LEAVES, 0.4),
            _ if matches!(tile.terrain, Terrain::Grass | Terrain::Meadow | Terrain::Steppe) => blend(rgb, AUTUMN_GRASS, 0.35),
            _ => rgb,
        };
    }
    if look.snow > 0 {
        let share = look.snow_share()
            * match tile.vegetation {
                Vegetation::Tree { .. } => 0.45,
                Vegetation::Boulder => 0.7,
                _ if tile.terrain == Terrain::Rock => 0.7,
                _ => 1.0,
            };
        rgb = blend(rgb, SNOW_RGB, share);
    }
    rgb
}

/// Colour of one map tile on the minimap in the season's `look`.
fn tile_color_in(state: &GameState, defs: &Defs, p: IVec2, look: SeasonLook) -> Color32 {
    let Some(tile) = state.world().get(p) else { return Color32::TRANSPARENT };
    if let Some(id) = state.buildings().id_at(p)
        && let Some(b) = state.building(id)
    {
        return building_color(defs, b);
    }
    let land = state.diplomacy().land();
    let foreign = land.owner(p).and_then(|k| state.diplomacy().kingdom(k)).map(|k| theme::kingdom_color(k.color()));
    if let Some(color) = foreign
        && land.has_structure(p)
    {
        return color.gamma_multiply(0.75).to_opaque();
    }
    if let Some(road) = state.roads().get(p) {
        return if road.is_bridge() { Color32::from_rgb(150, 108, 64) } else { Color32::from_rgb(196, 170, 120) };
    }
    if let Some(color) = foreign {
        let owner = land.owner(p);
        if [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y].iter().any(|d| state.world().in_bounds(p + *d) && land.owner(p + *d) != owner) {
            return color;
        }
        let rgb = seasonal_rgb(tile, look);
        let mix = |v: f32, c: u8| (v * 0.62 + f32::from(c) * 0.38).clamp(0.0, 255.0) as u8;
        return Color32::from_rgb(mix(rgb[0], color.r()), mix(rgb[1], color.g()), mix(rgb[2], color.b()));
    }
    let owned = state.territory().contains(p);
    if owned && [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y].iter().any(|d| state.world().in_bounds(p + *d) && !state.territory().contains(p + *d)) {
        return Color32::from_rgb(255, 168, 48);
    }
    let rgb = seasonal_rgb(tile, look);
    let shade = if owned { 1.0 } else { 0.82 };
    let c = |v: f32| (v * shade).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(c(rgb[0]), c(rgb[1]), c(rgb[2]))
}

/// Colour of a tile's terrain and vegetation, shaded by its height (no buildings, roads or
/// territory; also used by the new-game map preview).
pub fn natural_rgb(tile: &Tile) -> [f32; 3] {
    let base: [i32; 3] = match tile.terrain {
        Terrain::DeepWater => [38, 74, 118],
        Terrain::ShallowWater => [66, 118, 150],
        Terrain::Sand => [212, 192, 134],
        Terrain::Swamp => [88, 100, 68],
        Terrain::Steppe => [164, 152, 96],
        Terrain::Dirt => [132, 106, 72],
        Terrain::Grass => [104, 142, 64],
        Terrain::Meadow => [122, 160, 70],
        Terrain::Forest => [78, 112, 56],
        Terrain::Rock => [132, 128, 122],
        Terrain::Snow => [232, 236, 242],
    };
    let base = match tile.vegetation {
        Vegetation::Tree { .. } => [48, 84, 42],
        Vegetation::Bush => [base[0] * 4 / 5, base[1] * 9 / 10, base[2] * 4 / 5],
        Vegetation::Boulder => [118, 114, 108],
        Vegetation::None | Vegetation::Stump { .. } => base,
    };
    let shade = if tile.is_water() { 1.0 } else { 0.84 + f32::from(tile.elevation) * 0.035 };
    base.map(|v| v as f32 * shade)
}

fn building_color(defs: &Defs, b: &Building) -> Color32 {
    let Some(def) = defs.buildings().get(b.kind().index()) else { return Color32::GRAY };
    if !b.is_complete() {
        return Color32::from_rgb(214, 200, 160);
    }
    match def.role {
        BuildingRole::Headquarters => return Color32::from_rgb(236, 196, 84),
        BuildingRole::WallSegment | BuildingRole::Gate => return Color32::from_rgb(170, 166, 158),
        _ => {}
    }
    match def.category {
        BuildingCategory::Housing => Color32::from_rgb(186, 82, 58),
        BuildingCategory::Production => Color32::from_rgb(150, 104, 64),
        BuildingCategory::Agriculture => Color32::from_rgb(206, 172, 66),
        BuildingCategory::Public => Color32::from_rgb(76, 112, 176),
        BuildingCategory::Military => Color32::from_rgb(156, 58, 58),
    }
}

/// Maps between minimap points and world coordinates for a map of `size` tiles drawn into a
/// rectangle starting at `origin` ([`WIDTH`] wide).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    pub origin: egui::Pos2,
    /// Points per world unit.
    pub scale: f32,
    /// World x of the map's left corner (`-height × 32`).
    pub left: f32,
}

impl Projection {
    pub fn new(origin: egui::Pos2, size: IVec2) -> Self {
        let span = (size.x + size.y).max(1) as f32 * 32.0;
        Self { origin, scale: WIDTH / span, left: -(size.y as f32) * 32.0 }
    }

    pub fn to_minimap(self, world: Vec2) -> egui::Pos2 {
        self.origin + egui::vec2((world.x - self.left) * self.scale, world.y * self.scale)
    }

    pub fn to_world(self, p: egui::Pos2) -> Vec2 {
        let d = p - self.origin;
        Vec2::new(d.x / self.scale + self.left, d.y / self.scale)
    }
}

pub struct Minimap {
    texture: Option<egui::TextureHandle>,
    size: IVec2,
    pixels: Vec<Color32>,
    cursor: EventCursor,
    rects: BTreeMap<BuildingId, TileRect>,
    pending: BTreeSet<(i32, i32)>,
    full: bool,
    /// The season's look the texture shows.
    look: SeasonLook,
}

impl Minimap {
    pub fn new(session: &Session) -> Self {
        let mut minimap = Self {
            texture: None,
            size: IVec2::ZERO,
            pixels: Vec::new(),
            cursor: session.events().cursor_at_end(),
            rects: BTreeMap::new(),
            pending: BTreeSet::new(),
            full: true,
            look: SeasonLook::of(session.state()),
        };
        minimap.rects = building_rects(session.state());
        minimap
    }

    /// Starts over for another session (after loading).
    pub fn reset(&mut self, session: &Session) {
        *self = Self { texture: self.texture.take(), ..Self::new(session) };
    }

    /// Brings the texture up to date: `dirty` are the map chunks whose tiles changed.
    pub fn update(&mut self, ctx: &egui::Context, session: &Session, dirty: &[IVec2]) {
        let state = session.state();
        let defs = session.defs();
        if state.world().size() != self.size {
            self.full = true;
        }
        let look = SeasonLook::of(state);
        if look != self.look {
            self.look = look;
            self.full = true;
        }
        for c in dirty {
            self.pending.insert((c.x, c.y));
        }
        let read = session.events().read(&mut self.cursor);
        let mut structures = read.missed;
        for logged in read.events {
            match &logged.event {
                SimEvent::TerritoryChanged { .. } => self.full = true,
                SimEvent::RoadsBuilt { tiles, .. } | SimEvent::RoadsRemoved { tiles } => {
                    for t in tiles {
                        self.pending.insert(chunk_key(*t));
                    }
                }
                // Same footprint, new colour (site ⇄ finished building).
                SimEvent::BuildingCompleted { building, .. }
                | SimEvent::BuildingDamaged { building, .. }
                | SimEvent::BuildingRepaired { building, .. } => {
                    if let Some(b) = state.building(*building) {
                        self.mark_rect(b.rect());
                    }
                    structures = true;
                }
                SimEvent::BuildingPlaced { .. }
                | SimEvent::ConstructionCancelled { .. }
                | SimEvent::BuildingDemolished { .. }
                | SimEvent::UpgradeStarted { .. }
                | SimEvent::BuildingUpgraded { .. }
                | SimEvent::HouseDowngraded { .. }
                | SimEvent::UpgradeCancelled { .. }
                | SimEvent::MoveStarted { .. }
                | SimEvent::BuildingMoved { .. }
                | SimEvent::StructureDestroyed { .. } => structures = true,
                _ => {}
            }
        }
        if structures {
            let now = building_rects(state);
            let mut changed: Vec<TileRect> = Vec::new();
            for (id, rect) in self.rects.iter().chain(now.iter()) {
                if self.rects.get(id) != now.get(id) {
                    changed.push(*rect);
                }
            }
            // Construction stages change colours without changing rectangles.
            changed.extend(now.iter().filter(|(id, _)| state.building(**id).is_some_and(|b| !b.is_complete())).map(|(_, r)| *r));
            for rect in changed {
                self.mark_rect(rect);
            }
            self.rects = now;
        }
        if self.full {
            self.redraw_all(ctx, state, defs);
        } else if !self.pending.is_empty() {
            let chunks = std::mem::take(&mut self.pending);
            for (cx, cy) in chunks {
                self.redraw_chunk(state, defs, IVec2::new(cx, cy));
            }
        }
    }

    fn mark_rect(&mut self, rect: TileRect) {
        // One tile around: the territory/road look of neighbours may depend on it.
        let grown = rect.expanded(1);
        for p in [grown.origin, grown.origin + grown.size - IVec2::ONE, grown.origin + IVec2::new(grown.size.x - 1, 0), grown.origin + IVec2::new(0, grown.size.y - 1)] {
            self.pending.insert(chunk_key(p));
        }
    }

    fn redraw_all(&mut self, ctx: &egui::Context, state: &GameState, defs: &Defs) {
        self.full = false;
        self.pending.clear();
        self.size = state.world().size();
        let (w, h) = ((self.size.x * SCALE).max(1) as usize, (self.size.y * SCALE).max(1) as usize);
        self.pixels = vec![Color32::TRANSPARENT; w * h];
        for y in 0..self.size.y {
            for x in 0..self.size.x {
                self.put(IVec2::new(x, y), tile_color_in(state, defs, IVec2::new(x, y), self.look));
            }
        }
        let image = egui::ColorImage::new([w, h], self.pixels.clone());
        match self.texture.as_mut() {
            Some(texture) if texture.size() == [w, h] => texture.set(image, egui::TextureOptions::LINEAR),
            _ => self.texture = Some(ctx.load_texture("minimap", image, egui::TextureOptions::LINEAR)),
        }
    }

    fn redraw_chunk(&mut self, state: &GameState, defs: &Defs, chunk: IVec2) {
        let origin = chunk * CHUNK_SIZE;
        let end = (origin + IVec2::splat(CHUNK_SIZE)).min(self.size);
        if origin.x < 0 || origin.y < 0 || end.x <= origin.x || end.y <= origin.y {
            return;
        }
        for y in origin.y..end.y {
            for x in origin.x..end.x {
                self.put(IVec2::new(x, y), tile_color_in(state, defs, IVec2::new(x, y), self.look));
            }
        }
        let Some(texture) = self.texture.as_mut() else { return };
        let width = (self.size.x * SCALE) as usize;
        let (px, py) = ((origin.x * SCALE) as usize, (origin.y * SCALE) as usize);
        let (pw, ph) = (((end.x - origin.x) * SCALE) as usize, ((end.y - origin.y) * SCALE) as usize);
        let mut part = Vec::with_capacity(pw * ph);
        for row in py..py + ph {
            part.extend_from_slice(&self.pixels[row * width + px..row * width + px + pw]);
        }
        texture.set_partial([px, py], egui::ColorImage::new([pw, ph], part), egui::TextureOptions::LINEAR);
    }

    fn put(&mut self, p: IVec2, color: Color32) {
        let width = self.size.x * SCALE;
        for dy in 0..SCALE {
            for dx in 0..SCALE {
                let i = ((p.y * SCALE + dy) * width + p.x * SCALE + dx) as usize;
                if let Some(px) = self.pixels.get_mut(i) {
                    *px = color;
                }
            }
        }
    }

    /// Draws the minimap at the bottom right; returns its rectangle (empty before the texture
    /// exists).
    pub fn show(&mut self, ctx: &egui::Context, input: &HudInput<'_>, actions: &mut Vec<HudAction>) -> egui::Rect {
        let Some(texture) = self.texture.as_ref() else { return egui::Rect::NOTHING };
        let size = self.size;
        egui::Area::new(egui::Id::unique("hud_minimap"))
            .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-MARGIN, -MARGIN))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::parchment_frame().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(WIDTH, WIDTH * 0.5), egui::Sense::click_and_drag());
                    let projection = Projection::new(rect.min, size);
                    let painter = ui.painter_at(rect.expand(2.0));
                    // The map diamond: tile corners (0,0), (w,0), (w,h), (0,h).
                    let corner = |t: Vec2| projection.to_minimap(kingdoms_engine::rendering::iso::tile_to_world(t));
                    let s = size.as_vec2();
                    let corners = [corner(Vec2::ZERO), corner(Vec2::new(s.x, 0.0)), corner(s), corner(Vec2::new(0.0, s.y))];
                    let uvs = [egui::pos2(0.0, 0.0), egui::pos2(1.0, 0.0), egui::pos2(1.0, 1.0), egui::pos2(0.0, 1.0)];
                    let mut mesh = egui::Mesh::with_texture(texture.id());
                    for (pos, uv) in corners.iter().zip(uvs) {
                        mesh.vertices.push(egui::epaint::Vertex { pos: *pos, uv, color: Color32::WHITE });
                    }
                    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                    painter.add(egui::Shape::mesh(mesh));
                    painter.add(egui::Shape::closed_line(corners.to_vec(), egui::Stroke::new(1.0, Palette::WOOD)));
                    markers(&painter, input, projection);
                    // Camera frame.
                    let view = egui::Rect::from_two_pos(projection.to_minimap(input.view.min), projection.to_minimap(input.view.max));
                    painter.rect_stroke(view.intersect(rect), egui::CornerRadius::ZERO, egui::Stroke::new(1.5, Palette::GOLD_BRIGHT), egui::StrokeKind::Middle);
                    if (response.clicked() || response.dragged())
                        && let Some(p) = response.interact_pointer_pos()
                    {
                        actions.push(HudAction::LookAt(projection.to_world(p)));
                    }
                    response.on_hover_text("Karte: klicken oder ziehen, um die Ansicht zu verschieben");
                });
            })
            .response
            .rect
    }
}

/// Enemies, own companies and caravans as dots on the minimap.
fn markers(painter: &egui::Painter, input: &HudInput<'_>, projection: Projection) {
    let state = input.session.state();
    let at = |tile: IVec2| projection.to_minimap(kingdoms_engine::rendering::iso::tile_center_world(tile, 0.0));
    let walking = state.population().walking();
    let centre = |tiles: &[IVec2]| (!tiles.is_empty()).then(|| tiles.iter().copied().fold(IVec2::ZERO, |a, b| a + b) / tiles.len() as i32);
    for c in state.diplomacy().caravans() {
        let Some(t) = c.walkers.first().and_then(|w| walking.traveler(*w)) else { continue };
        let color = state.diplomacy().kingdom(c.kingdom).map_or(Palette::GOLD, |k| theme::kingdom_color(k.color()));
        let p = at(t.motion().pos().tile());
        painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(5.0, 5.0)), egui::CornerRadius::same(1), color);
        painter.rect_stroke(egui::Rect::from_center_size(p, egui::vec2(5.0, 5.0)), egui::CornerRadius::same(1), egui::Stroke::new(1.0, Palette::GOLD_BRIGHT), egui::StrokeKind::Outside);
    }
    for company in state.military().companies() {
        let tiles: Vec<IVec2> = company
            .members()
            .iter()
            .filter_map(|id| state.population().citizen(*id))
            .filter(|c| c.motion().is_visible())
            .map(|c| c.motion().pos().tile())
            .collect();
        if let Some(t) = centre(&tiles) {
            let p = at(t);
            painter.circle_filled(p, 3.5, Color32::from_rgb(theme::PLAYER_COLOR[0], theme::PLAYER_COLOR[1], theme::PLAYER_COLOR[2]));
            painter.circle_stroke(p, 3.5, egui::Stroke::new(1.0, Palette::WOOD_DARK));
        }
    }
    let pulse = 0.55 + 0.45 * ((input.now * 4.0).sin() as f32 * 0.5 + 0.5);
    for w in state.military().warbands() {
        let tiles: Vec<IVec2> = w.fighters().iter().filter_map(|f| walking.traveler(f.walker())).map(|t| t.motion().pos().tile()).collect();
        let (tile, approaching) = match centre(&tiles) {
            Some(t) => (t, false),
            None => (w.entry(), true),
        };
        let p = at(tile);
        let r = 3.0 + (f32::from(w.strength()).sqrt()).min(5.0);
        painter.circle_filled(p, r, Palette::BAD.gamma_multiply(if approaching { 0.5 } else { pulse }));
        painter.circle_stroke(p, r + 1.5, egui::Stroke::new(1.5, Palette::BAD_LIGHT.gamma_multiply(pulse)));
    }
    if !state.military().warbands().is_empty() {
        painter.ctx().request_repaint();
    }
}

fn chunk_key(p: IVec2) -> (i32, i32) {
    (p.x.div_euclid(CHUNK_SIZE), p.y.div_euclid(CHUNK_SIZE))
}

fn building_rects(state: &GameState) -> BTreeMap<BuildingId, TileRect> {
    state.buildings().iter().map(|(id, b)| (id, b.rect())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_round_trips_and_spans_the_diamond() {
        let size = IVec2::new(128, 128);
        let p = Projection::new(egui::pos2(10.0, 20.0), size);
        let left = p.to_minimap(kingdoms_engine::rendering::iso::tile_to_world(Vec2::new(0.0, 128.0)));
        let right = p.to_minimap(kingdoms_engine::rendering::iso::tile_to_world(Vec2::new(128.0, 0.0)));
        let bottom = p.to_minimap(kingdoms_engine::rendering::iso::tile_to_world(Vec2::new(128.0, 128.0)));
        assert!((left.x - 10.0).abs() < 1e-3 && (right.x - (10.0 + WIDTH)).abs() < 1e-3);
        assert!((bottom.y - (20.0 + WIDTH * 0.5)).abs() < 1e-3);
        let w = Vec2::new(123.0, 456.0);
        assert!((p.to_world(p.to_minimap(w)) - w).length() < 1e-2);
        assert_eq!(chunk_key(IVec2::new(33, -1)), (1, -1));
    }

    #[test]
    fn the_season_colours_land_but_not_water() {
        assert_eq!(SeasonLook::new(0, Season::Winter), SeasonLook { snow: 0, autumn: false });
        assert_eq!(SeasonLook::new(9, Season::Winter).snow, 0, "a dusting shows no snow");
        assert_eq!(SeasonLook::new(50, Season::Winter).snow, 3);
        assert_eq!(SeasonLook::new(100, Season::Winter).snow, SNOW_STEPS as u8);
        assert_eq!(SeasonLook::new(250, Season::Winter).snow, SNOW_STEPS as u8);
        assert!(SeasonLook::new(0, Season::Autumn).autumn);
        let tile = |terrain: Terrain, vegetation: Vegetation| Tile {
            terrain,
            biome: kingdoms_sim::world::Biome::Grassland,
            elevation: 2,
            fertility: 128,
            moisture: 128,
            temperature: 128,
            vegetation,
            deposit: None,
            river: false,
        };
        let grass = tile(Terrain::Grass, Vegetation::None);
        let oak = tile(Terrain::Forest, Vegetation::Tree { kind: TreeKind::Deciduous, growth: 255 });
        let pine = tile(Terrain::Forest, Vegetation::Tree { kind: TreeKind::Conifer, growth: 255 });
        let water = tile(Terrain::DeepWater, Vegetation::None);
        let winter = SeasonLook::new(100, Season::Winter);
        let autumn = SeasonLook::new(0, Season::Autumn);
        let bright = |rgb: [f32; 3]| rgb.iter().sum::<f32>();
        assert_eq!(seasonal_rgb(&grass, SeasonLook::default()), natural_rgb(&grass));
        assert!(bright(seasonal_rgb(&grass, winter)) > 650.0, "snow-white meadow");
        assert!(bright(seasonal_rgb(&oak, winter)) > bright(natural_rgb(&oak)), "snow on the crowns");
        assert!(bright(seasonal_rgb(&oak, winter)) < bright(seasonal_rgb(&grass, winter)), "forests stay darker than open land");
        assert_eq!(seasonal_rgb(&water, winter), natural_rgb(&water), "water stays open");
        let [r, g, _] = seasonal_rgb(&oak, autumn);
        assert!(r > g, "autumn leaves turn orange: {r} {g}");
        assert_eq!(seasonal_rgb(&pine, autumn), natural_rgb(&pine), "conifers stay green");
    }
}
