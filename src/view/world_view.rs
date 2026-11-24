//! The map view: all render layers of the simulation state and their change tracking.
//!
//! Static layers (terrain, vegetation, fields, roads) are per-chunk GPU batches that are rebuilt
//! only when the simulation reports changes: dirty chunks ([`Session::take_dirty_chunks`]) for
//! every tile-dependent layer, field chunks ([`Session::take_field_dirty_chunks`]) for the
//! fields, structure chunks ([`Session::take_structure_dirty_chunks`], guarded by the structure
//! revision) for roads, the territory event (read through an [`EventCursor`]) for the border
//! and the season for vegetation. Buildings (with their work loops), grazing animals,
//! particles, projectiles of the current combat round ([`Projectiles`]), the ground shadows of
//! walkers, animals and trees ([`BlobShadows`]) and highlights are drawn every frame from the
//! current state, as are the walkers ([`CitizenLayer`]) and the lit windows at night
//! ([`WindowLights`]). The weather look ([`Effects`]) sets the wind, snow cover and wetness of
//! the frame; its light times the time of day ([`Daylight`]) is the ambient light of the world
//! and sets the shadows' opacity.

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::AssetManager;
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::Renderer;
use kingdoms_engine::rendering::camera::Camera2D;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, SpriteDepth};
use kingdoms_engine::rendering::sprite::{Environment, RenderList, SpriteInstance};
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::grid::TileRect;
use kingdoms_sim::world::MAX_ELEVATION;

use super::animals::Grazers;
use super::buildings::{self, BuildingSprites, Emphasis};
use super::citizens::{CitizenLayer, CitizenStats, WalkerFrame, WalkerMarks};
use super::daylight::Daylight;
use super::effects::{EffectStats, Effects, WeatherLook};
use super::fields::FieldRenderer;
use super::ground::GroundSignatures;
use super::kingdoms::KingdomLayer;
use super::lights::{LightStats, WindowLights};
use super::overlay::{HighlightStyle, OverlaySprites, TerritoryBorder};
use super::picking::{Hover, Selection};
use super::preview;
use super::projectiles::Projectiles;
use super::roads::RoadRenderer;
use super::shadows::{BlobShadows, shadow_alpha};
use super::terrain::TerrainRenderer;
use super::vegetation::VegetationRenderer;
use super::{ANIMATION_TICKS_PER_SECOND, GRID_BIAS};
use crate::session::{EventCursor, Session};
use crate::tools::ToolController;

/// Layer statistics for the debug window.
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewStats {
    pub terrain_sprites: usize,
    pub vegetation_sprites: usize,
    pub road_sprites: usize,
    pub field_sprites: usize,
    pub border_segments: usize,
    pub buildings_drawn: usize,
    pub animals_drawn: usize,
    /// Arrows and stones in flight.
    pub projectiles_drawn: usize,
    /// Chunks rebuilt by the last synchronisation: ground (terrain, roads, border; changed
    /// terrain or elevation), structures, fields. Vegetation follows every dirty chunk.
    pub rebuilt: (usize, usize, usize),
    pub effects: EffectStats,
    pub look: Option<WeatherLook>,
    pub citizens: CitizenStats,
    pub lights: LightStats,
    pub daylight: Option<Daylight>,
    /// Ambient light of the last frame (weather × time of day).
    pub ambient: [f32; 3],
}

/// Measured parts of [`WorldView::draw`].
#[derive(Clone, Copy, Debug)]
pub struct DrawTimes {
    /// Time per layer (profiling sections `draw: <layer>`), in drawing order; the first
    /// `count` entries are set.
    layers: [(&'static str, std::time::Duration); MAX_LAPS],
    count: usize,
    last: std::time::Instant,
}

/// Layers timed by [`DrawTimes`].
const MAX_LAPS: usize = 12;

impl DrawTimes {
    fn start() -> Self {
        Self { layers: [("", std::time::Duration::ZERO); MAX_LAPS], count: 0, last: std::time::Instant::now() }
    }

    /// Ends the step `name` (since the previous lap); laps beyond [`MAX_LAPS`] are dropped.
    fn lap(&mut self, name: &'static str) {
        let now = std::time::Instant::now();
        if let Some(slot) = self.layers.get_mut(self.count) {
            *slot = (name, now - self.last);
            self.count += 1;
        }
        self.last = now;
    }

    /// The timed layers in drawing order.
    pub fn layers(&self) -> &[(&'static str, std::time::Duration)] {
        &self.layers[..self.count]
    }
}

/// What to emphasise on the map this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Highlights {
    pub hover: Hover,
    pub selection: Option<Selection>,
    /// The demolish tool is active: the hovered building is marked red.
    pub demolish: bool,
    /// Hover outlines are drawn (off while a tool preview shows the cursor position).
    pub hover_outline: bool,
    /// Hovered and inspected walkers.
    pub walkers: WalkerMarks,
    /// Show where the companies' orders lead (military tab open).
    pub orders: bool,
}

/// All render layers of the map.
pub struct WorldView {
    terrain: TerrainRenderer,
    vegetation: VegetationRenderer,
    fields: FieldRenderer,
    roads: RoadRenderer,
    border: TerritoryBorder,
    building_sprites: BuildingSprites,
    kingdoms: KingdomLayer,
    grazers: Grazers,
    citizens: CitizenLayer,
    shadows: BlobShadows,
    projectiles: Projectiles,
    lights: WindowLights,
    daylight: Option<Daylight>,
    ambient: [f32; 3],
    effects: Effects,
    overlay: OverlaySprites,
    events: EventCursor,
    /// Structure revision of the simulation the road layer reflects.
    structure_revision: u64,
    buildings_drawn: usize,
    animals_drawn: usize,
    rebuilt: (usize, usize, usize),
    /// Terrain and elevation per chunk as the ground-dependent layers show them.
    ground: GroundSignatures,
    /// Developer grid (G).
    pub show_grid: bool,
    /// Territory border (T).
    pub show_territory: bool,
    /// Light follows the time of day (off: always the light of noon, no window lights).
    pub day_night: bool,
}

impl WorldView {
    /// Builds every layer for the current state.
    pub fn new(session: &mut Session, renderer: &mut Renderer, assets: &AssetManager) -> Self {
        let started = std::time::Instant::now();
        // Everything is built from scratch; pending dirty chunks are covered by that.
        session.take_dirty_chunks();
        session.take_structure_dirty_chunks();
        session.take_field_dirty_chunks();
        session.take_land_dirty_chunks();
        let state = session.state();
        let defs = session.defs();
        let season = state.clock().season();
        let overlay = OverlaySprites::load(assets);
        let mut view = Self {
            terrain: TerrainRenderer::new(assets),
            vegetation: VegetationRenderer::new(assets, season),
            fields: FieldRenderer::new(assets),
            roads: RoadRenderer::new(assets),
            border: TerritoryBorder::new(),
            building_sprites: BuildingSprites::load(defs, assets),
            kingdoms: KingdomLayer::new(assets),
            grazers: Grazers::load(defs, assets),
            citizens: CitizenLayer::new(defs, assets),
            shadows: BlobShadows::load(assets),
            projectiles: Projectiles::load(assets),
            lights: WindowLights::new(defs, assets),
            daylight: None,
            ambient: [1.0; 3],
            effects: Effects::new(assets, state),
            overlay,
            events: session.events().cursor_at_end(),
            structure_revision: state.structure_revision(),
            buildings_drawn: 0,
            animals_drawn: 0,
            rebuilt: (0, 0, 0),
            ground: GroundSignatures::default(),
            show_grid: false,
            show_territory: true,
            day_night: true,
        };
        let map = state.world();
        view.terrain.rebuild_all(map, renderer, assets);
        view.vegetation.rebuild_all(map, defs.rules(), season, renderer, assets);
        view.fields.rebuild_all(state, defs, renderer, assets);
        view.roads.rebuild_all(state, defs, renderer, assets);
        view.border.rebuild_all(map, state.territory(), &view.overlay);
        view.kingdoms.rebuild_all(state, &view.overlay);
        for (_, k) in state.diplomacy().kingdoms() {
            log::info!("kingdom {} seated at {}: {} structures, {} tiles", k.name(), k.seat(), k.structures().len(), k.territory_tiles());
        }
        log::info!(
            "map view built in {:.0} ms: {} terrain, {} vegetation, {} field, {} road sprites",
            started.elapsed().as_secs_f64() * 1000.0,
            view.terrain.instance_count(),
            view.vegetation.instance_count(),
            view.fields.instance_count(),
            view.roads.instance_count()
        );
        view
    }

    /// Frees the GPU batches of every static layer. The renderer keeps static batches until they
    /// are removed, so a view must be released before it is replaced (quickload).
    pub fn release(&mut self, renderer: &mut Renderer) {
        self.terrain.release(renderer);
        self.vegetation.release(renderer);
        self.fields.release(renderer);
        self.roads.release(renderer);
        self.effects.clear();
    }

    /// Applies the simulation changes since the last call to the cached layers. Returns the map
    /// chunks whose tiles, kingdom borders or kingdom structures changed (for other caches such
    /// as the minimap).
    pub fn sync(&mut self, session: &mut Session, renderer: &mut Renderer, assets: &AssetManager) -> Vec<IVec2> {
        let dirty = session.take_dirty_chunks();
        let land = session.take_land_dirty_chunks();
        // The revision tells cheaply whether any road, wall or footprint changed at all.
        let revision = session.state().structure_revision();
        let structures = if revision != self.structure_revision {
            self.structure_revision = revision;
            session.take_structure_dirty_chunks()
        } else {
            Vec::new()
        };
        let mut fields = session.take_field_dirty_chunks();
        let read = session.events().read(&mut self.events);
        let mut events = read.events;
        let territory = read.missed || events.any(|e| matches!(e.event, SimEvent::TerritoryChanged { .. }));
        let state = session.state();
        let defs = session.defs();
        let map = state.world();
        // Vegetation (and the minimap) show every tile change; the other layers only depend on
        // terrain and elevation, which most changes (trees growing or felled) leave alone.
        let ground = self.ground.changed(map, &dirty);
        for c in &ground {
            if !fields.contains(c) {
                fields.push(*c);
            }
        }
        if !ground.is_empty() {
            self.terrain.rebuild_chunks(&ground, map, renderer, assets);
        }
        self.vegetation.update(&dirty, map, defs.rules(), state.clock().season(), renderer, assets);
        if !fields.is_empty() {
            self.fields.rebuild_chunks(&fields, state, defs, renderer, assets);
        }
        if !structures.is_empty() || !ground.is_empty() {
            self.roads.refresh(state, defs, &structures, &ground, renderer, assets);
        }
        self.rebuilt = (ground.len(), structures.len(), fields.len());
        if territory {
            self.border.rebuild_all(map, state.territory(), &self.overlay);
        } else if !ground.is_empty() {
            self.border.rebuild_chunks(&ground, map, state.territory(), &self.overlay);
        }
        if !land.is_empty() {
            self.kingdoms.rebuild_chunks(&land, state, &self.overlay);
        }
        let mut changed = dirty;
        for c in land {
            if !changed.contains(&c) {
                changed.push(c);
            }
        }
        changed
    }

    /// Resolves the tile, building and citizen under world point `w` (camera `zoom` sets how
    /// generously walkers are hit). A citizen counts unless a building is drawn (opaque) in
    /// front of it right there — walkers seen through the gaps of a building stay pickable.
    pub fn hover_at(&self, session: &Session, assets: &AssetManager, w: Vec2, zoom: f32) -> Hover {
        let state = session.state();
        let defs = session.defs();
        let tile = super::picking::pick_tile(state.world(), w);
        let ground = tile.and_then(|p| state.world().get(p).map(|t| (p, f32::from(t.elevation))));
        let building = buildings::pick_building(state, defs, &self.building_sprites, assets, w, ground);
        let citizen = self
            .citizens
            .pick(w, zoom)
            .filter(|(_, depth)| {
                buildings::opaque_front_depth(state, defs, &self.building_sprites, assets, w)
                    .is_none_or(|front| *depth + 0.05 >= front)
            })
            .map(|(id, _)| id);
        Hover { tile, building: building.map(|(id, _)| id), citizen }
    }

    /// Statistics of the layers.
    pub fn stats(&self) -> ViewStats {
        ViewStats {
            terrain_sprites: self.terrain.instance_count(),
            vegetation_sprites: self.vegetation.instance_count(),
            road_sprites: self.roads.instance_count(),
            field_sprites: self.fields.instance_count(),
            border_segments: self.border.segment_count() + self.kingdoms.segment_count(),
            buildings_drawn: self.buildings_drawn + self.kingdoms.drawn(),
            animals_drawn: self.animals_drawn,
            projectiles_drawn: self.projectiles.drawn(),
            rebuilt: self.rebuilt,
            effects: self.effects.stats(),
            look: Some(*self.effects.look()),
            citizens: self.citizens.stats(),
            lights: self.lights.stats(),
            daylight: self.daylight,
            ambient: self.ambient,
        }
    }

    /// Factor on the particle spawn rates (settings: particle density).
    pub fn set_particle_density(&mut self, density: f32) {
        self.effects.set_density(density);
    }

    /// Advances the weather look and the particles by `dt` real seconds and spawns the
    /// particles of this frame (smoke, dust, leaves, rain, snow) for the view of `camera`.
    pub fn update_effects(&mut self, dt: f32, session: &Session, camera: &Camera2D, assets: &AssetManager) {
        let view = camera.view_rect().expanded(96.0);
        let leaves = self.vegetation.leaf_sources(view).copied();
        self.effects.update(dt, session, camera, &self.building_sprites, assets, leaves);
    }

    /// Queues every layer. `ticks` is the game time (ticks plus the interpolation fraction
    /// `alpha`) the light of day and the grazing animals follow; walkers are interpolated with
    /// `alpha`.
    pub fn draw(
        &mut self,
        list: &mut RenderList,
        session: &Session,
        assets: &AssetManager,
        highlights: &Highlights,
        ticks: f64,
        alpha: f32,
    ) -> DrawTimes {
        let state = session.state();
        let defs = session.defs();
        let daylight = if self.day_night { Daylight::at(ticks) } else { Daylight::at(noon_of(ticks)) };
        let weather = self.effects.ambient();
        self.ambient = [weather.r * daylight.light[0], weather.g * daylight.light[1], weather.b * daylight.light[2]];
        self.daylight = Some(daylight);
        list.ambient = Color::rgb(self.ambient[0], self.ambient[1], self.ambient[2]);
        let seconds = (ticks / ANIMATION_TICKS_PER_SECOND) as f32;
        let shade = shadow_alpha(&daylight, self.effects.look().light);
        // Painted building shadows fade like the soft ground shadows.
        list.environment = Environment { shadow: shade, ..self.effects.environment() };
        let mut laps = DrawTimes::start();
        self.terrain.draw(list);
        self.vegetation.draw(list);
        laps.lap("draw: static layers");
        let view = list.visible_rect();
        for t in self.vegetation.shadows(view) {
            self.shadows.draw(list, t.ground, t.height, t.width, shade);
        }
        laps.lap("draw: tree shadows");
        self.fields.draw(list);
        self.roads.draw(list);
        laps.lap("draw: fields, roads");
        let hovered = highlights.hover.building;
        let selected = match highlights.selection {
            Some(Selection::Building(id)) => Some(id),
            _ => None,
        };
        self.buildings_drawn = buildings::draw_buildings(list, state, defs, &self.building_sprites, assets, seconds, |id| {
            if Some(id) == hovered && highlights.demolish {
                Emphasis::Demolish
            } else if Some(id) == selected {
                Emphasis::Selected
            } else if Some(id) == hovered {
                Emphasis::Hovered
            } else {
                Emphasis::None
            }
        });
        laps.lap("draw: buildings");
        self.kingdoms.draw(list, state, defs, &self.building_sprites, assets, &self.overlay);
        laps.lap("draw: kingdoms");
        self.animals_drawn = self.grazers.draw(list, state, assets, &self.shadows, ticks, shade);
        laps.lap("draw: animals");
        let frame = WalkerFrame { alpha, seconds, marks: highlights.walkers, shadow_alpha: shade };
        self.citizens.draw(list, state, defs, assets, &self.shadows, frame);
        laps.lap("citizens");
        self.projectiles.draw(list, state, defs, ticks);
        self.lights.draw(list, state, defs, &self.building_sprites, assets, &daylight);
        laps.lap("draw: lights, projectiles");
        self.effects.draw(list);
        laps.lap("draw: particles");
        if self.show_territory {
            self.border.draw(list, self.overlay.texture);
        }
        if self.show_grid {
            self.draw_grid(list, session);
        }
        if highlights.orders {
            super::orders::draw_company_orders(list, state, &self.overlay);
        }
        self.draw_highlights(list, session, highlights);
        laps.lap("draw: overlays");
        laps
    }

    /// The light of the last frame.
    pub fn daylight(&self) -> Option<Daylight> {
        self.daylight
    }

    fn draw_highlights(&self, list: &mut RenderList, session: &Session, highlights: &Highlights) {
        let state = session.state();
        let map = state.world();
        let selected_rect = |sel: Option<Selection>| -> Option<TileRect> {
            match sel? {
                Selection::Building(id) => state.building(id).map(|b| b.rect()),
                Selection::Tile(p) => Some(TileRect::tile(p)),
            }
        };
        let selected = selected_rect(highlights.selection);
        if let Some(rect) = selected {
            self.overlay.footprint(list, map, rect, &HighlightStyle::SELECTED);
        }
        if !highlights.hover_outline {
            return;
        }
        if let Some(rect) = selected_rect(highlights.hover.selection()).filter(|r| Some(*r) != selected) {
            let style =
                if highlights.hover.building.is_some() { HighlightStyle::HOVERED_BUILDING } else { HighlightStyle::HOVERED_TILE };
            self.overlay.footprint(list, map, rect, &style);
        }
    }

    /// Queues the preview of the active tool (ghost, planned roads, demolition marks) and the
    /// pending orders that wait for the next tick.
    pub fn draw_tool_preview(
        &self,
        list: &mut RenderList,
        session: &Session,
        assets: &AssetManager,
        tools: &ToolController,
        hovered_tile: Option<IVec2>,
    ) {
        let art = preview::PreviewArt { buildings: &self.building_sprites, overlay: &self.overlay, assets };
        preview::draw_pending(list, session.state(), session.defs(), session.queued_commands(), tools, &art);
        preview::draw(list, session.state(), session.defs(), tools, hovered_tile, &art);
    }

    fn draw_grid(&self, list: &mut RenderList, session: &Session) {
        let map = session.state().world();
        let grid = &self.overlay.grid;
        let (min_t, max_t) = visible_tile_range(&list.visible_rect(), map.size());
        for y in min_t.y..max_t.y {
            for x in min_t.x..max_t.x {
                let p = IVec2::new(x, y);
                let level = f32::from(map.tile(p).elevation);
                let pos = iso::tile_center_world(p, level);
                let depth = SpriteDepth::ground(p.as_vec2() + Vec2::splat(0.5), GRID_BIAS);
                list.draw_transparent(self.overlay.texture, SpriteInstance::new(pos, grid.size, grid.anchor, grid.uv, depth));
            }
        }
    }
}

/// Conservative range of tile coordinates whose sprites may intersect `view`.
pub(crate) fn visible_tile_range(view: &Rect, size: IVec2) -> (IVec2, IVec2) {
    let corners = [view.min, Vec2::new(view.max.x, view.min.y), view.max, Vec2::new(view.min.x, view.max.y)];
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for c in corners {
        // Account for elevation: tiles up to MAX_ELEVATION levels below may be lifted into view.
        for e in [0.0, f32::from(MAX_ELEVATION)] {
            let t = iso::world_to_tile_elevated(c, e);
            min = min.min(t);
            max = max.max(t);
        }
    }
    let lo = (min.floor().as_ivec2() - IVec2::ONE).max(IVec2::ZERO);
    let hi = (max.ceil().as_ivec2() + IVec2::ONE).min(size);
    (lo, hi)
}

/// Game time (ticks) of noon on the day of `ticks` (constant light without day and night).
fn noon_of(ticks: f64) -> f64 {
    use kingdoms_sim::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
    let day = (ticks.max(0.0) / TICKS_PER_DAY as f64).floor();
    day * TICKS_PER_DAY as f64 + 12.0 * TICKS_PER_HOUR as f64
}
