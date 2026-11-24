//! Weather look and particle effects, all derived from the simulation state each frame:
//!
//! * **Weather look** ([`WeatherLook`]): ambient light (darker under clouds, rain and storms,
//!   cool in snow, warm in droughts), snow cover and ground wetness of the terrain (from
//!   `WeatherState::snow_cover` / `ground_wetness`), the density of falling rain and snow, and
//!   the wind: `WeatherState::wind_vector_permille` projected onto the screen ([`ScreenWind`])
//!   and `wind_strength` set how far trees, bushes, crops and orchards sway and lean and which
//!   way, and how fast smoke, leaves, rain and snow drift – the same wind that carries fires
//!   downwind in the simulation. Changes are eased over a few seconds, so the hourly simulation
//!   steps and the weather change at midnight do not jump.
//! * **Weather particles** in screen space: rain streaks slanted by the wind (mirrored when it
//!   blows to the right; storms: more and faster), snowflakes swaying down; density from the
//!   weather's intensity.
//! * **World particles** at their sources: chimney smoke of production buildings while they
//!   work, of houses whose stove burns (`Population::heating`: residents, a cold day and fuel –
//!   a house out of firewood stays cold and smokeless) and of other operating buildings on cold
//!   days, dust over construction sites with builders at work, falling leaves from leaf trees
//!   in autumn.
//! * **War:** sparks and dust where the blows of the latest combat round land
//!   (`CombatRound::strikes`): between attacker and target fighter, or at the struck edge of the
//!   gate, wall or building an enemy hacks at; flames and thick smoke on buildings under attack
//!   (the more damage, the more fire).
//!
//! Smoke leaves the sprites at the emitter points of `assets/atlases/emitters.ron` (written by
//! the asset generator next to the atlases, docs/assets.md). Only sources near the view emit.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::{AssetManager, SpriteId, SpriteRegion};
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::camera::Camera2D;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, OBJECT_DEPTH_BIAS, SpriteDepth};
use kingdoms_engine::rendering::particles::{
    Emitter, FrameMode, ParticleSpace, ParticleStats, ParticleStyle, ParticleSystem, StyleId,
};
use kingdoms_engine::rendering::sprite::{Environment, RenderList};
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::defs::{BuildingCategory, Defs, Weather};
use kingdoms_sim::grid::TileRect;
use kingdoms_sim::military::StrikeTarget;
use kingdoms_sim::population::{Heating, WalkerId, walker_view};
use kingdoms_sim::production::ProductionStatus;
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::Season;
use serde::Deserialize;

use super::buildings::{BuildingSprites, Part};
use super::citizens::{Allegiances, crowd_position};
use super::vegetation::LeafSource;
use crate::session::Session;

/// Particles alive at most (all effects together).
const PARTICLE_CAPACITY: usize = 8000;
/// Seconds the weather look needs to move ~63 % of the way to a new target.
const EASE_SECONDS: f32 = 2.5;
/// File with the emitter points, relative to the assets root.
const EMITTERS_FILE: &str = "atlases/emitters.ron";
/// Leaf trees emitting at most per frame (sparser sampling when zoomed out).
const MAX_LEAF_TREES: usize = 500;

// Emitter key spaces (high byte) so keys of different effects never collide.
const KEY_RAIN: u64 = 1 << 56;
const KEY_SNOW: u64 = 2 << 56;
const KEY_SMOKE: u64 = 3 << 56;
const KEY_DUST: u64 = 4 << 56;
const KEY_LEAF: u64 = 5 << 56;
const KEY_FIGHT: u64 = 6 << 56;
const KEY_FIRE: u64 = 7 << 56;
/// Share of the way from the struck edge of a building back towards its attacker where the
/// sparks of the blow fly.
const BUILDING_SPARK_SHARE: f32 = 0.2;

/// Emitter points of sprites as written by the asset generator.
#[derive(Debug, Default, Deserialize)]
pub struct EmitterPoints {
    /// Pixel offsets from the anchor of the unmirrored sprite, keyed `<atlas>/<sprite>`.
    pub smoke: BTreeMap<String, Vec<(f32, f32)>>,
}

impl EmitterPoints {
    /// Parses the RON text of `emitters.ron`.
    pub fn parse(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }

    /// Reads `emitters.ron` below `root`; a missing or invalid file is logged and yields no
    /// points (no smoke).
    pub fn load(root: &Path) -> Self {
        let path = root.join(EMITTERS_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => match Self::parse(&text) {
                Ok(points) => points,
                Err(e) => {
                    log::warn!("{}: invalid emitter points: {e}", path.display());
                    Self::default()
                }
            },
            Err(e) => {
                log::warn!("{}: {e}; buildings show no smoke", path.display());
                Self::default()
            }
        }
    }
}

/// The simulation's wind as the map shows it: its direction projected onto the screen and its
/// strength.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenWind {
    /// Direction on screen: x from −1 (left) to 1 (right), y from −0.5 (up) to 0.5 (down) – a
    /// wind along the ground is foreshortened like the tile diamonds.
    pub dir: Vec2,
    /// Strength 0–1.
    pub strength: f32,
}

impl ScreenWind {
    /// From `WeatherState::wind_vector_permille` (map coordinates, north = −y) and
    /// `WeatherState::wind_strength` (0–100).
    pub fn from_sim(vector_permille: IVec2, strength: u8) -> Self {
        let v = vector_permille.as_vec2() / 1000.0;
        // Tile +x runs right-down on screen, tile +y left-down (64 × 32 diamonds).
        let dir = Vec2::new(v.x - v.y, (v.x + v.y) * 0.5) * std::f32::consts::FRAC_1_SQRT_2;
        Self { dir, strength: f32::from(strength.min(100)) / 100.0 }
    }

    /// The current wind of `state`.
    pub fn of(state: &GameState) -> Self {
        let w = state.weather();
        Self::from_sim(w.wind_vector_permille(), w.wind_strength())
    }
}

/// What the world looks like under the current weather (eased towards [`WeatherLook::target`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherLook {
    /// Ambient light (linear RGB).
    pub light: [f32; 3],
    /// Sway amplitude of trees, bushes, crops and orchards (1 = breeze).
    pub sway: f32,
    /// Steady lean of their tops in pixels (negative = to the left).
    pub lean: f32,
    /// Screen direction of the gusts, −1 (left) … 1 (right); 0 sways evenly.
    pub gust_dir: f32,
    /// Wind for particles in pixels per second (screen direction of the simulation's wind).
    pub wind: Vec2,
    /// Snow cover 0–1 and ground wetness 0–1.
    pub snow: f32,
    pub wetness: f32,
    /// Rain and snowfall density 0–1 (storm rain above 1).
    pub rain: f32,
    pub snowfall: f32,
}

impl WeatherLook {
    /// The look of `weather` at `intensity` percent with the given snow cover and wetness
    /// (both 0–100, as reported by the simulation) and the simulation's `wind`: its strength
    /// sets how much plants sway and lean and how fast smoke, rain and snow drift, its
    /// direction which way – the same wind that carries fires further downwind.
    pub fn target(weather: Weather, intensity: u8, snow_cover: u8, wetness: u8, wind: ScreenWind) -> Self {
        let i = f32::from(intensity.min(100)) / 100.0;
        // Light colour at full intensity; weaker weather lies closer to white.
        let (light, rain, snowfall) = match weather {
            Weather::Clear => ([1.0, 1.0, 1.0], 0.0, 0.0),
            Weather::Cloudy => ([0.86, 0.88, 0.92], 0.0, 0.0),
            Weather::Rain => ([0.72, 0.75, 0.83], i, 0.0),
            Weather::Storm => ([0.52, 0.55, 0.64], 1.0 + 0.6 * i, 0.0),
            Weather::Snow => ([0.93, 0.95, 1.0], 0.0, i),
            Weather::Drought => ([1.05, 1.0, 0.88], 0.0, 0.0),
        };
        let k = 0.4 + 0.6 * i;
        let s = if wind.strength.is_finite() { wind.strength.clamp(0.0, 1.0) } else { 0.0 };
        let dir = if wind.dir.is_finite() { wind.dir.clamp(Vec2::new(-1.0, -0.5), Vec2::new(1.0, 0.5)) } else { Vec2::ZERO };
        Self {
            light: light.map(|c| 1.0 + (c - 1.0) * k),
            sway: 0.6 + 2.6 * s,
            lean: dir.x * 5.0 * s * s,
            gust_dir: (dir.x * std::f32::consts::SQRT_2 * (s * 3.0).min(1.0)).clamp(-1.0, 1.0),
            wind: dir * (8.0 + 170.0 * s * s),
            snow: f32::from(snow_cover.min(100)) / 100.0,
            wetness: f32::from(wetness.min(100)) / 100.0,
            rain,
            snowfall,
        }
    }

    /// The target look of the current weather and wind of `state`.
    pub fn of(state: &GameState) -> Self {
        let w = state.weather();
        Self::target(w.current(), w.intensity(), w.snow_cover(), w.ground_wetness(), ScreenWind::of(state))
    }

    /// Moves every value towards `target` for `dt` seconds (exponential easing).
    pub fn approach(&mut self, target: &WeatherLook, dt: f32) {
        let k = 1.0 - (-dt.max(0.0) / EASE_SECONDS).exp();
        let ease = |a: &mut f32, b: f32| *a += (b - *a) * k;
        for (a, b) in self.light.iter_mut().zip(target.light) {
            ease(a, b);
        }
        ease(&mut self.sway, target.sway);
        ease(&mut self.lean, target.lean);
        ease(&mut self.gust_dir, target.gust_dir);
        ease(&mut self.wind.x, target.wind.x);
        ease(&mut self.wind.y, target.wind.y);
        ease(&mut self.snow, target.snow);
        ease(&mut self.wetness, target.wetness);
        ease(&mut self.rain, target.rain);
        ease(&mut self.snowfall, target.snowfall);
    }

    /// How fast the sway clock runs (stronger wind sways faster).
    pub fn sway_speed(&self) -> f32 {
        1.0 + 0.6 * (self.sway - 1.0).max(0.0)
    }
}

/// Why a building smokes, if it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmokeSource {
    /// A workshop with a fire while it works.
    Workshop,
    /// A heated building on a cold day (a house whose stove burns, tavern, town hall, …).
    Heating,
}

/// Whether a finished building with chimneys smokes now: workshops while their production
/// runs; houses while their stove burns (`heating` from `Population::heating`: residents, a
/// cold day and fuel for every household); other operating buildings on cold days (`cold`:
/// `WeatherState::needs_heating`). Paused buildings never.
pub fn smoke_source(
    category: BuildingCategory,
    production: Option<ProductionStatus>,
    heating: Option<Heating>,
    operational: bool,
    cold: bool,
) -> Option<SmokeSource> {
    if !operational {
        return None;
    }
    if production == Some(ProductionStatus::Working) {
        return Some(SmokeSource::Workshop);
    }
    match category {
        BuildingCategory::Housing => heating.is_some_and(Heating::smoking).then_some(SmokeSource::Heating),
        BuildingCategory::Production | BuildingCategory::Agriculture => None,
        _ => cold.then_some(SmokeSource::Heating),
    }
}

/// Registered particle styles.
struct Styles {
    /// Rain and storm rain slanting to the left (as drawn) and to the right (mirrored).
    rain: [StyleId; 2],
    storm: [StyleId; 2],
    snow: StyleId,
    smoke: StyleId,
    dust: StyleId,
    leaf: StyleId,
    spark: StyleId,
    fire: StyleId,
}

/// Statistics for the debug window.
#[derive(Clone, Copy, Debug, Default)]
pub struct EffectStats {
    pub particles: ParticleStats,
    pub smoke_sources: usize,
    pub dust_sources: usize,
    pub leaf_trees: usize,
    /// Melee blows of the current combat round shown with sparks.
    pub fights: usize,
    /// Buildings burning under attack.
    pub fires: usize,
}

/// Weather look and particles of the map view.
pub struct Effects {
    system: ParticleSystem,
    styles: Styles,
    /// Smoke points per building sprite (pixel offsets from the anchor).
    smoke_points: BTreeMap<SpriteId, Vec<Vec2>>,
    look: WeatherLook,
    /// Sway clock (runs faster in strong wind).
    wind_time: f32,
    stats: EffectStats,
    /// Factor on every spawn rate (particle density setting; 0 spawns nothing).
    density: f32,
}

fn regions(assets: &AssetManager, names: &[String]) -> Vec<SpriteRegion> {
    names.iter().map(|n| *assets.region(assets.sprite(n))).collect()
}

impl Effects {
    /// Sets up the styles from the `effects` atlas and the emitter points; the look starts at
    /// the current weather without easing.
    pub fn new(assets: &AssetManager, state: &GameState) -> Self {
        let mut system = ParticleSystem::new(PARTICLE_CAPACITY);
        let names = |prefix: &str, n: usize| (0..n).map(|i| format!("effects/{prefix}_{i}")).collect::<Vec<_>>();

        let mut rain = ParticleStyle::new(ParticleSpace::Screen, regions(assets, &["effects/rain".to_owned()]));
        rain.lifetime = (0.3, 0.55);
        // The slant comes from the wind (its screen direction picks the mirrored streak).
        rain.velocity = Vec2::new(0.0, 820.0);
        rain.velocity_spread = Vec2::new(15.0, 80.0);
        rain.wind_response = 1.5;
        rain.scale = (1.5, 1.5);
        rain.scale_spread = 0.25;
        rain.alpha = 0.55;
        rain.fade_in = 0.2;
        rain.fade_out = 0.3;
        rain.color = Color::rgb(0.8, 0.86, 0.95);
        let mut storm = rain.clone();
        storm.velocity = Vec2::new(0.0, 1100.0);
        storm.alpha = 0.6;
        storm.scale = (1.9, 1.9);
        let mirrored = |style: &ParticleStyle| {
            let mut m = style.clone();
            m.flags |= kingdoms_engine::rendering::sprite::FLAG_FLIP_X;
            m
        };
        let (rain_right, storm_right) = (mirrored(&rain), mirrored(&storm));

        let mut snow = ParticleStyle::new(ParticleSpace::Screen, regions(assets, &["effects/snowflake".to_owned()]));
        snow.lifetime = (3.0, 5.5);
        snow.velocity = Vec2::new(0.0, 55.0);
        snow.velocity_spread = Vec2::new(8.0, 18.0);
        snow.wind_response = 0.6;
        snow.sway = (10.0, 0.35);
        snow.scale = (1.15, 1.15);
        snow.scale_spread = 0.45;
        snow.alpha = 0.9;
        snow.fade_in = 0.15;
        snow.fade_out = 0.2;

        let mut smoke = ParticleStyle::new(ParticleSpace::World, regions(assets, &names("smoke", 6)));
        smoke.frame_mode = FrameMode::OverLife;
        smoke.lifetime = (4.0, 6.0);
        smoke.velocity = Vec2::new(0.0, -15.0);
        smoke.velocity_spread = Vec2::new(3.0, 3.0);
        smoke.acceleration = Vec2::new(0.0, -1.5);
        smoke.wind_response = 0.35;
        smoke.scale = (0.55, 2.0);
        smoke.scale_spread = 0.15;
        smoke.alpha = 0.9;
        smoke.fade_in = 0.1;
        smoke.fade_out = 0.45;
        // Darker than the sprite so it reads against snow and pale sky alike.
        smoke.color = Color::rgb(0.58, 0.58, 0.6);

        let mut dust = ParticleStyle::new(ParticleSpace::World, regions(assets, &names("dust", 3)));
        dust.frame_mode = FrameMode::OverLife;
        dust.lifetime = (1.0, 1.8);
        dust.velocity = Vec2::new(0.0, -9.0);
        dust.velocity_spread = Vec2::new(14.0, 5.0);
        dust.wind_response = 0.25;
        dust.scale = (1.0, 2.2);
        dust.scale_spread = 0.2;
        dust.alpha = 0.85;
        dust.fade_in = 0.1;
        dust.fade_out = 0.5;
        // Earthy brown, darker than the pale foundations it rises from.
        dust.color = Color::rgb(0.72, 0.6, 0.45);

        let mut leaf = ParticleStyle::new(ParticleSpace::World, regions(assets, &names("leaf", 4)));
        leaf.lifetime = (3.0, 4.5);
        leaf.velocity = Vec2::new(0.0, 13.0);
        leaf.velocity_spread = Vec2::new(5.0, 4.0);
        leaf.wind_response = 0.3;
        leaf.sway = (6.0, 0.55);
        leaf.scale = (1.3, 1.3);
        leaf.scale_spread = 0.2;
        leaf.fade_in = 0.1;
        leaf.fade_out = 0.25;

        let mut spark = ParticleStyle::new(ParticleSpace::World, regions(assets, &["effects/sparkle".to_owned()]));
        spark.lifetime = (0.18, 0.4);
        spark.velocity = Vec2::new(0.0, -28.0);
        spark.velocity_spread = Vec2::new(30.0, 18.0);
        spark.acceleration = Vec2::new(0.0, 60.0);
        spark.scale = (0.55, 0.2);
        spark.scale_spread = 0.3;
        spark.fade_out = 0.4;
        spark.color = Color::rgb(1.0, 0.82, 0.4);
        spark.flags = kingdoms_engine::rendering::sprite::FLAG_EMISSIVE;

        let mut fire = ParticleStyle::new(ParticleSpace::World, regions(assets, &names("fire", 4)));
        fire.frame_mode = FrameMode::Loop { fps: 10.0 };
        fire.lifetime = (0.5, 0.9);
        fire.velocity = Vec2::new(0.0, -10.0);
        fire.velocity_spread = Vec2::new(3.0, 3.0);
        fire.wind_response = 0.15;
        fire.scale = (1.1, 0.5);
        fire.scale_spread = 0.25;
        fire.fade_in = 0.1;
        fire.fade_out = 0.35;
        fire.flags = kingdoms_engine::rendering::sprite::FLAG_EMISSIVE;

        let styles = Styles {
            spark: system.add_style(spark),
            fire: system.add_style(fire),
            rain: [system.add_style(rain), system.add_style(rain_right)],
            storm: [system.add_style(storm), system.add_style(storm_right)],
            snow: system.add_style(snow),
            smoke: system.add_style(smoke),
            dust: system.add_style(dust),
            leaf: system.add_style(leaf),
        };
        let points = EmitterPoints::load(assets.root());
        let mut smoke_points = BTreeMap::new();
        for (name, list) in &points.smoke {
            match assets.try_sprite(name) {
                Some(id) => {
                    smoke_points.insert(id, list.iter().map(|&(x, y)| Vec2::new(x, y)).collect());
                }
                None => log::warn!("{EMITTERS_FILE}: unknown sprite {name}"),
            }
        }
        let look = WeatherLook::of(state);
        Self { system, styles, smoke_points, look, wind_time: 0.0, stats: EffectStats::default(), density: 1.0 }
    }

    /// Ambient light for the render list.
    pub fn ambient(&self) -> Color {
        Color::rgb(self.look.light[0], self.look.light[1], self.look.light[2])
    }

    /// Wind, snow and wetness for the sprite shader (painted shadows at full strength; the map
    /// view weakens them by the light).
    pub fn environment(&self) -> Environment {
        Environment {
            wind: self.look.sway,
            wind_lean: self.look.lean,
            wind_dir: self.look.gust_dir,
            wind_time: Some(self.wind_time),
            snow: self.look.snow,
            wetness: self.look.wetness * (1.0 - self.look.snow),
            shadow: 1.0,
        }
    }

    /// The current (eased) weather look.
    pub fn look(&self) -> &WeatherLook {
        &self.look
    }

    pub fn stats(&self) -> EffectStats {
        EffectStats { particles: self.system.stats(), ..self.stats }
    }

    /// Sets the factor on every spawn rate (0 = no new particles, 1 = full).
    pub fn set_density(&mut self, density: f32) {
        self.density = if density.is_finite() { density.clamp(0.0, 1.0) } else { 1.0 };
    }

    /// Drops every particle (after loading a game).
    pub fn clear(&mut self) {
        self.system.clear();
    }

    /// Advances the look and the particles by `dt` real seconds and spawns this frame's
    /// particles of all sources near the view.
    pub fn update(
        &mut self,
        dt: f32,
        session: &Session,
        camera: &Camera2D,
        sprites: &BuildingSprites,
        assets: &AssetManager,
        leaves: impl Iterator<Item = LeafSource>,
    ) {
        let (state, defs) = (session.state(), session.defs());
        let target = WeatherLook::of(state);
        self.look.approach(&target, dt);
        self.wind_time = (self.wind_time + dt.max(0.0) * self.look.sway_speed()) % 3600.0;
        let view = camera.view_rect();
        self.system.update(dt, self.look.wind, view);
        self.emit_weather(camera);
        self.emit_buildings(state, defs, sprites, assets, &view);
        self.emit_leaves(state.clock().season(), leaves);
        self.emit_war(state, defs, sprites, assets, &view);
    }

    /// Sparks and dust where the blows of the current combat round land; fire on buildings
    /// under attack.
    fn emit_war(&mut self, state: &GameState, defs: &Defs, sprites: &BuildingSprites, assets: &AssetManager, view: &Rect) {
        let fights = strike_spots(state, defs);
        self.stats.fights = fights.len();
        for (key, p) in fights {
            let level = state.world().get(p.floor().as_ivec2()).map_or(0.0, |t| f32::from(t.elevation));
            let position = iso::tile_to_world_elevated(p, level) - Vec2::new(0.0, 10.0);
            let depth = iso::SpriteDepth::upright_point(p, OBJECT_DEPTH_BIAS + 0.01).tent[0];
            let sparks = Emitter { style: self.styles.spark, position, spread: Vec2::new(7.0, 6.0), rate: 14.0, depth };
            emit_scaled(&mut self.system, self.density, KEY_FIGHT | key, &sparks);
            let dust = Emitter { style: self.styles.dust, position: position + Vec2::new(0.0, 10.0), spread: Vec2::new(10.0, 3.0), rate: 3.0, depth };
            emit_scaled(&mut self.system, self.density, KEY_FIGHT | (1 << 40) | key, &dust);
        }
        let area = view.expanded(256.0);
        let mut fires = 0;
        for (id, b) in state.buildings().iter() {
            let damage = state.military().damage_of(id);
            if damage == 0 {
                continue;
            }
            let Some(visual) = sprites.visual(b, defs, state, assets) else { continue };
            let part = &visual.main;
            let size = b.rect().size.max(glam::IVec2::ONE);
            let centre = iso::tile_to_world_elevated(b.rect().origin.as_vec2() + size.as_vec2() * 0.5, part.elevation);
            if !area.contains(centre) {
                continue;
            }
            let max = defs.buildings().get(b.kind().index()).and_then(|d| d.defense).map_or(100, |d| d.hit_points.max(1));
            let share = (damage as f32 / max as f32).clamp(0.15, 1.0);
            let extent = Vec2::new((size.x + size.y) as f32 * 8.0, (size.x + size.y) as f32 * 3.0);
            let depth = front_depth(part);
            let flames = Emitter { style: self.styles.fire, position: centre - Vec2::new(0.0, 8.0), spread: extent, rate: 6.0 + 22.0 * share, depth };
            emit_scaled(&mut self.system, self.density, KEY_FIRE | building_key(id), &flames);
            let smoke = Emitter { style: self.styles.smoke, position: centre - Vec2::new(0.0, 30.0), spread: extent * 0.6, rate: 2.0 + 8.0 * share, depth };
            emit_scaled(&mut self.system, self.density, KEY_FIRE | building_key(id) | 1, &smoke);
            fires += 1;
        }
        self.stats.fires = fires;
    }

    /// Queues all particles.
    pub fn draw(&self, list: &mut RenderList) {
        self.system.draw(list);
    }

    /// Rain and snow over the whole screen (logical pixels).
    fn emit_weather(&mut self, camera: &Camera2D) {
        let screen = camera.viewport() / camera.scale_factor().max(0.1);
        // Spawn over the screen plus a margin upwind and above, so drifting particles fill it.
        let centre = screen * 0.5 + Vec2::new(60.0, -30.0);
        let spread = screen * 0.5 + Vec2::new(90.0, 40.0);
        let area = (spread.x * spread.y * 4.0) / 1_000_000.0;
        let rain = self.look.rain;
        if rain > 0.01 {
            let side = usize::from(self.look.wind.x > 0.0);
            let (style, density) =
                if rain > 1.0 { (self.styles.storm[side], 1500.0 * rain) } else { (self.styles.rain[side], 1100.0 * rain) };
            let e = Emitter { style, position: centre, spread, rate: density * area, depth: 0.0 };
            emit_scaled(&mut self.system, self.density, KEY_RAIN, &e);
        }
        if self.look.snowfall > 0.01 {
            let e = Emitter { style: self.styles.snow, position: centre, spread, rate: 320.0 * self.look.snowfall * area, depth: 0.0 };
            emit_scaled(&mut self.system, self.density, KEY_SNOW, &e);
        }
    }

    /// Chimney smoke and construction dust of the buildings near the view.
    fn emit_buildings(&mut self, state: &GameState, defs: &Defs, sprites: &BuildingSprites, assets: &AssetManager, view: &Rect) {
        let cold = state.weather().needs_heating(defs.rules(), state.clock());
        let area = view.expanded(256.0);
        let (mut smoke_sources, mut dust_sources) = (0, 0);
        for (id, b) in state.buildings().iter() {
            let bounds = iso::footprint_world_bounds(b.rect().origin, b.rect().size.max(glam::IVec2::ONE).as_uvec2(), 0.0);
            if !Rect::new(bounds.min - Vec2::new(0.0, 160.0), bounds.max).intersects(&area) {
                continue;
            }
            let Some(def) = defs.buildings().get(b.kind().index()) else { continue };
            let Some(visual) = sprites.visual(b, defs, state, assets) else { continue };
            // Construction sites and upgrades with builders at work (standing at the site
            // during their shift) raise dust.
            if let Some(site) = b.construction_site()
                && site.can_progress()
                && !b.is_paused()
                && kingdoms_sim::population::present_workers(state, defs, id) > 0
            {
                let part = &visual.main;
                let size = b.rect().size.max(glam::IVec2::ONE);
                let centre = iso::tile_to_world_elevated(b.rect().origin.as_vec2() + size.as_vec2() * 0.5, part.elevation);
                let extent = Vec2::new((size.x + size.y) as f32 * 9.0, (size.x + size.y) as f32 * 4.5);
                let rate = 2.0 + 0.6 * (size.x * size.y) as f32;
                let e = Emitter { style: self.styles.dust, position: centre, spread: extent, rate, depth: front_depth(part) };
                emit_scaled(&mut self.system, self.density, KEY_DUST | building_key(id), &e);
                dust_sources += 1;
            }
            // The finished house keeps smoking in front of its upgrade scaffolding.
            let house = match (b.is_complete(), visual.extra.as_ref()) {
                (true, Some(extra)) => extra,
                (true, None) => &visual.main,
                (false, _) => continue,
            };
            let Some(points) = self.smoke_points.get(&house.sprite) else { continue };
            let production = b.production().map(|p| p.status());
            let heating = state.population().heating(id, cold);
            let Some(source) = smoke_source(def.category, production, heating, b.is_operational(), cold) else { continue };
            let rate = match source {
                SmokeSource::Workshop => 3.5,
                SmokeSource::Heating => 2.5,
            };
            let corner = house.front_corner();
            for (i, p) in points.iter().enumerate() {
                let offset = if house.flip { Vec2::new(-p.x, p.y) } else { *p };
                let e = Emitter {
                    style: self.styles.smoke,
                    position: corner + offset,
                    spread: Vec2::new(1.5, 1.0),
                    rate,
                    depth: front_depth(house),
                };
                emit_scaled(&mut self.system, self.density, KEY_SMOKE | building_key(id) | i as u64, &e);
                smoke_sources += 1;
            }
        }
        self.stats.smoke_sources = smoke_sources;
        self.stats.dust_sources = dust_sources;
    }

    /// Falling leaves from leaf trees in autumn (not while it snows).
    fn emit_leaves(&mut self, season: Season, leaves: impl Iterator<Item = LeafSource>) {
        self.stats.leaf_trees = 0;
        if season != Season::Autumn || self.look.snowfall > 0.2 {
            return;
        }
        let sources: Vec<LeafSource> = leaves.collect();
        // Thin out when many trees are visible; each emitting tree drops proportionally more.
        let stride = sources.len().div_ceil(MAX_LEAF_TREES).max(1);
        let rate = 0.15 * stride as f32 * (1.0 + 0.5 * (self.look.sway - 1.0).max(0.0));
        for s in sources.iter().step_by(stride) {
            let e = Emitter { style: self.styles.leaf, position: s.crown, spread: s.spread, rate, depth: s.depth };
            emit_scaled(&mut self.system, self.density, KEY_LEAF | u64::from(s.key), &e);
            self.stats.leaf_trees += 1;
        }
    }
}

/// Where the melee blows of the current combat round land (tile space), one spot per attacker
/// keyed by its walker id: halfway between attacker and target fighter, or just in front of
/// the struck tile of a gate, wall or building. The fighters' current places are used – an
/// enemy in the crowd of its band at its place in the crowd, as it is drawn – so the sparks
/// stay between the pair; a fighter inside a garrison building strikes from the edge of its
/// building. Empty when no round is current.
pub fn strike_spots(state: &GameState, defs: &Defs) -> Vec<(u64, Vec2)> {
    let round = state.military().last_round();
    if !round.is_current(state.clock().tick, defs.rules().military.round_ticks) || round.strikes().is_empty() {
        return Vec::new();
    }
    let sides = Allegiances::of(state);
    let position = |id: WalkerId| {
        walker_view(state, defs, id).filter(|v| v.visible).map(|v| {
            let (x, y) = v.pos.to_tiles();
            crowd_position(state, Vec2::new(x, y), sides.crowd_offset_of(id))
        })
    };
    let mut attackers = BTreeSet::new();
    round
        .strikes()
        .iter()
        .filter(|s| attackers.insert(s.attacker))
        .map(|s| {
            let from = position(s.attacker).unwrap_or_else(|| nearest_point(s.from, rect_centre(s.to)));
            let to = match s.target {
                StrikeTarget::Walker(target) => position(target).unwrap_or_else(|| nearest_point(s.to, from)),
                StrikeTarget::Building { tile, .. } => nearest_point(TileRect::tile(tile), from),
            };
            (fight_key(s.attacker), spark_spot(from, to, s.target))
        })
        .collect()
}

/// Where the sparks of a blow from `from` against `to` fly: halfway between two fighters, close
/// to the struck edge of a building.
fn spark_spot(from: Vec2, to: Vec2, target: StrikeTarget) -> Vec2 {
    match target {
        StrikeTarget::Walker(_) => (from + to) * 0.5,
        StrikeTarget::Building { .. } => to + (from - to) * BUILDING_SPARK_SHARE,
    }
}

/// The point of the area of `rect` (tile space, tile `t` spans `t..t + 1`) nearest to `p`.
fn nearest_point(rect: TileRect, p: Vec2) -> Vec2 {
    let min = rect.origin.as_vec2();
    p.clamp(min, min + rect.size.max(IVec2::ONE).as_vec2())
}

fn rect_centre(rect: TileRect) -> Vec2 {
    rect.origin.as_vec2() + rect.size.max(IVec2::ONE).as_vec2() * 0.5
}

/// Emitter key part of a fighter (citizens and travellers apart).
fn fight_key(id: WalkerId) -> u64 {
    match id {
        WalkerId::Citizen(c) => u64::from(c.index()) << 1,
        WalkerId::Traveler(t) => (u64::from(t.index()) << 1) | 1,
    }
}

/// Spawns from `e` with the rate scaled by the particle density setting (0: nothing).
fn emit_scaled(system: &mut ParticleSystem, density: f32, key: u64, e: &Emitter) {
    if density > 0.0 {
        system.emit(key, &Emitter { rate: e.rate * density, ..*e });
    }
}

/// Emitter key part of a building (index and generation; the low byte is free for the point).
fn building_key(id: BuildingId) -> u64 {
    (u64::from(id.index()) << 24) | ((u64::from(id.generation()) & 0xffff) << 8)
}

/// Depth just in front of a building part's front walls (its own smoke and dust stay in front
/// of it; buildings standing further in front still cover them).
fn front_depth(part: &Part) -> f32 {
    let size = part.rect.size.max(glam::IVec2::ONE);
    SpriteDepth::footprint(part.rect.origin, size.as_uvec2(), 0.0).tent[1] + OBJECT_DEPTH_BIAS + 0.01
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calm() -> ScreenWind {
        ScreenWind { dir: Vec2::new(-0.7, 0.35), strength: 0.2 }
    }

    #[test]
    fn weather_look_follows_weather_and_intensity() {
        let clear = WeatherLook::target(Weather::Clear, 80, 0, 0, ScreenWind { dir: Vec2::ZERO, strength: 0.0 });
        assert_eq!(clear.light, [1.0, 1.0, 1.0]);
        assert_eq!((clear.rain, clear.snowfall, clear.lean, clear.gust_dir), (0.0, 0.0, 0.0, 0.0));
        let light_storm = WeatherLook::target(Weather::Storm, 20, 0, 50, calm());
        let heavy_storm = WeatherLook::target(Weather::Storm, 100, 0, 90, calm());
        // Storms are darker and rain harder the stronger they are.
        assert!(heavy_storm.light[0] < light_storm.light[0] && light_storm.light[0] < 1.0);
        assert!(heavy_storm.rain > 1.0);
        assert!((heavy_storm.wetness - 0.9).abs() < 1e-6);
        let rain = WeatherLook::target(Weather::Rain, 50, 0, 40, calm());
        assert!((rain.rain - 0.5).abs() < 1e-6 && rain.rain < heavy_storm.rain);
        let snow = WeatherLook::target(Weather::Snow, 60, 75, 30, calm());
        assert!((snow.snowfall - 0.6).abs() < 1e-6 && (snow.snow - 0.75).abs() < 1e-6);
        assert_eq!(snow.rain, 0.0);
        // Out-of-range inputs are clamped.
        let over = WeatherLook::target(Weather::Snow, 250, 200, 200, ScreenWind { dir: Vec2::splat(9.0), strength: 7.0 });
        assert_eq!((over.snowfall, over.snow, over.wetness), (1.0, 1.0, 1.0));
        assert!(over.sway <= 3.2 + 1e-6 && over.lean <= 5.0 + 1e-6 && over.gust_dir <= 1.0);
    }

    #[test]
    fn the_wind_of_the_simulation_sets_direction_and_strength() {
        // Map north (−y) points up-right on screen, east (+x) down-right, west (−x) up-left.
        let north = ScreenWind::from_sim(IVec2::new(0, -1000), 50);
        assert!(north.dir.x > 0.6 && north.dir.y < -0.3 && (north.strength - 0.5).abs() < 1e-6);
        let west = ScreenWind::from_sim(IVec2::new(-1000, 0), 50);
        assert!(west.dir.x < -0.6 && west.dir.y < -0.3);
        // South-east (+x +y) blows straight down the screen: no sideways lean.
        let towards_viewer = ScreenWind::from_sim(IVec2::new(707, 707), 80);
        assert!(towards_viewer.dir.x.abs() < 1e-3 && towards_viewer.dir.y > 0.45);
        let breeze = WeatherLook::target(Weather::Clear, 50, 0, 0, ScreenWind::from_sim(IVec2::new(1000, 0), 20));
        let gale = WeatherLook::target(Weather::Storm, 100, 0, 0, ScreenWind::from_sim(IVec2::new(1000, 0), 100));
        let gale_west = WeatherLook::target(Weather::Storm, 100, 0, 0, ScreenWind::from_sim(IVec2::new(-1000, 0), 100));
        // Plants sway more and lean further in strong wind, towards the side the wind blows to;
        // particles drift that way faster.
        assert!(gale.sway > breeze.sway && breeze.sway > 1.0);
        assert!(gale.lean > breeze.lean && breeze.lean > 0.0 && gale.lean > 3.0);
        assert!(gale_west.lean < -3.0 && gale_west.gust_dir < -0.9 && gale.gust_dir > 0.9);
        assert!(gale.wind.x > breeze.wind.x && breeze.wind.x > 0.0 && gale_west.wind.x < -100.0);
        // Rain and smoke drift the way the fire spreads: east is to the right and down.
        assert!(gale.wind.y > 0.0);
    }

    #[test]
    fn the_look_eases_towards_its_target() {
        let mut look = WeatherLook::target(Weather::Clear, 50, 0, 0, calm());
        let target = WeatherLook::target(Weather::Storm, 100, 0, 100, ScreenWind { dir: Vec2::new(0.7, 0.35), strength: 1.0 });
        look.approach(&target, 0.0);
        assert_eq!(look, WeatherLook::target(Weather::Clear, 50, 0, 0, calm()));
        let start = look.sway;
        look.approach(&target, EASE_SECONDS);
        // One time constant: about 63 % of the way.
        let share = (look.sway - start) / (target.sway - start);
        assert!((share - 0.632).abs() < 0.01, "share {share}");
        for _ in 0..200 {
            look.approach(&target, 0.1);
        }
        assert!((look.wetness - 1.0).abs() < 1e-3 && (look.light[2] - target.light[2]).abs() < 1e-3);
        assert!((look.wind - target.wind).length() < 0.1 && (look.gust_dir - target.gust_dir).abs() < 1e-3);
        assert!(look.sway_speed() > 1.0);
    }

    #[test]
    fn smoke_follows_work_heating_and_cold() {
        use BuildingCategory::*;
        let working = Some(ProductionStatus::Working);
        let idle = Some(ProductionStatus::Idle);
        assert_eq!(smoke_source(Production, working, None, true, false), Some(SmokeSource::Workshop));
        assert_eq!(smoke_source(Production, idle, None, true, true), None);
        assert_eq!(smoke_source(Production, working, None, false, false), None);
        // Houses smoke only while their stove burns: not when the households ran out of fuel
        // (they freeze), not when nobody lives there and not on warm days.
        assert_eq!(smoke_source(Housing, None, Some(Heating::Burning), true, true), Some(SmokeSource::Heating));
        assert_eq!(smoke_source(Housing, None, Some(Heating::OutOfFuel), true, true), None);
        assert_eq!(smoke_source(Housing, None, Some(Heating::Empty), true, true), None);
        assert_eq!(smoke_source(Housing, None, Some(Heating::NotNeeded), true, false), None);
        assert_eq!(smoke_source(Housing, None, None, true, true), None);
        assert_eq!(smoke_source(Public, None, None, true, true), Some(SmokeSource::Heating));
        assert_eq!(smoke_source(Public, None, None, true, false), None);
        assert_eq!(smoke_source(Public, None, None, false, true), None);
    }

    #[test]
    fn shipped_emitter_points_parse_and_name_buildings() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let text = match std::fs::read_to_string(root.join(EMITTERS_FILE)) {
            Ok(t) => t,
            Err(e) => panic!("{EMITTERS_FILE} must exist: {e}"),
        };
        let points = match EmitterPoints::parse(&text) {
            Ok(p) => p,
            Err(e) => panic!("{EMITTERS_FILE} must parse: {e}"),
        };
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match Defs::load(&dir) {
            Ok(d) => d,
            Err(e) => panic!("shipped data must load: {e}"),
        };
        for key in ["house_wood", "bakery", "smithy", "charcoal_kiln"] {
            let Some(def) = defs.buildings().iter().find(|d| d.key == key) else { panic!("no {key}") };
            let list = points.smoke.get(&def.sprite());
            assert!(list.is_some_and(|l| !l.is_empty() && l.iter().all(|p| p.1 < 0.0)), "{key}");
        }
        assert!(points.smoke.keys().all(|k| defs.buildings().iter().any(|d| d.sprite() == *k)));
    }

    #[test]
    fn sparks_fly_between_the_fighters_or_at_the_struck_wall() {
        let walker = StrikeTarget::Walker(WalkerId::Citizen(kingdoms_sim::population::CitizenId::from_raw_parts(1, 0)));
        assert_eq!(spark_spot(Vec2::new(2.0, 2.0), Vec2::new(3.0, 2.0), walker), Vec2::new(2.5, 2.0));
        // A blow against a gate: the struck tile's edge nearest the attacker, a little towards it.
        let gate = StrikeTarget::Building { building: BuildingId::from_raw_parts(4, 0), tile: IVec2::new(5, 5) };
        let attacker = Vec2::new(3.5, 5.5);
        let edge = nearest_point(TileRect::tile(IVec2::new(5, 5)), attacker);
        assert_eq!(edge, Vec2::new(5.0, 5.5));
        let spot = spark_spot(attacker, edge, gate);
        assert!(spot.x < 5.0 && spot.x > 4.5 && spot.y == 5.5, "{spot}");
        // A point inside the area stays where it is; one outside lands on the border.
        let hall = TileRect::new(IVec2::new(10, 10), IVec2::new(3, 2));
        assert_eq!(nearest_point(hall, Vec2::new(11.0, 11.0)), Vec2::new(11.0, 11.0));
        assert_eq!(nearest_point(hall, Vec2::new(20.0, 0.0)), Vec2::new(13.0, 10.0));
        assert_eq!(rect_centre(hall), Vec2::new(11.5, 11.0));
        // Citizens and travellers with the same index get different emitters.
        let c = fight_key(WalkerId::Citizen(kingdoms_sim::population::CitizenId::from_raw_parts(3, 0)));
        let t = fight_key(WalkerId::Traveler(kingdoms_sim::population::TravelerId::from_raw_parts(3, 0)));
        assert_ne!(c, t);
    }

    #[test]
    fn the_blows_of_a_raid_get_sparks_next_to_their_attackers() {
        use kingdoms_sim::time::TICKS_PER_HOUR;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match Defs::load(&dir) {
            Ok(d) => d,
            Err(e) => panic!("shipped data must load: {e}"),
        };
        let mut state = GameState::new(&defs, kingdoms_sim::state::GameSettings::new(&defs, 7, kingdoms_sim::world::WorldSize::Small));
        assert!(strike_spots(&state, &defs).is_empty(), "no fighting yet");
        // Soldiers on patrol around the headquarters (the developer army), then a band of robbers
        // too large for the archers to stop before it reaches the soldiers.
        let (mut army, mut commands) = crate::session::dev_army::DevArmy::start(&defs, 10);
        for _ in 0..(2 * 24 * TICKS_PER_HOUR) {
            let events = state.tick(&defs, std::mem::take(&mut commands));
            commands = army.after_tick(&state, &defs, &events);
        }
        commands.push(kingdoms_sim::command::Command::DebugSpawnThreat { kingdom: None, size: 30 });
        let (mut checked, mut turned) = (0, 0);
        for _ in 0..(TICKS_PER_HOUR * 12) {
            let events = state.tick(&defs, std::mem::take(&mut commands));
            commands = army.after_tick(&state, &defs, &events);
            let round = state.military().last_round();
            if round.tick() != state.clock().tick || round.strikes().is_empty() {
                continue;
            }
            let spots = strike_spots(&state, &defs);
            let sides = Allegiances::of(&state);
            let mut attackers = BTreeSet::new();
            let first_blows: Vec<_> = round.strikes().iter().filter(|s| attackers.insert(s.attacker)).collect();
            assert_eq!(spots.len(), first_blows.len(), "one spot per attacker");
            for (strike, (key, spot)) in first_blows.iter().zip(&spots) {
                assert_eq!(*key, fight_key(strike.attacker));
                // The sparks fly between the pair, within reach of both (robbers stand at their
                // place in the crowd of the band, at most a little over a tile away).
                let near = |r: TileRect| nearest_point(r, *spot).distance(*spot);
                assert!(near(strike.from) <= 2.0 && near(strike.to) <= 2.0, "{strike:?} → {spot}");
                // A robber in the crowd turns from its place towards the soldier it strikes.
                if let (WalkerId::Traveler(_), Some(v)) = (strike.attacker, walker_view(&state, &defs, strike.attacker)) {
                    let (x, y) = v.pos.to_tiles();
                    let at = crowd_position(&state, Vec2::new(x, y), sides.crowd_offset_of(strike.attacker));
                    let towards = nearest_point(strike.to, at) - at;
                    if let Some(facing) = crate::view::citizens::attack_facing(&state, strike.attacker, at) {
                        assert!(facing.offset().as_vec2().dot(towards) > 0.0, "{facing:?} towards {towards}");
                        turned += 1;
                    }
                }
            }
            checked += 1;
            if checked >= 5 {
                break;
            }
        }
        assert!(checked > 0, "soldiers and robbers fought within twelve hours");
        assert!(turned > 0, "robbers struck from their places in the crowd");
    }

    #[test]
    fn building_keys_are_unique_per_id() {
        let a = building_key(BuildingId::from_raw_parts(1, 0));
        let b = building_key(BuildingId::from_raw_parts(1, 1));
        let c = building_key(BuildingId::from_raw_parts(2, 0));
        assert!(a != b && a != c && b != c);
        assert_eq!(a & 0xff, 0);
        assert!(a < (1 << 56));
    }
}
