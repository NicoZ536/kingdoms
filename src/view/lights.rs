//! Lit windows at night – presentation only.
//!
//! Every building sprite with windows has a night light in the `lights` atlas (generated with
//! the buildings, docs/assets.md): its window panes in warm light with a soft halo, anchored
//! like the building. At dusk the light fades in over the windows of buildings with people
//! inside – inhabited houses whose residents are at home (they go dark at bedtime, see
//! [`residents_awake`]), workshops with their workers, the tavern with its guests – and of
//! workshops whose production runs. The light ignores the ambient light (`FLAG_EMISSIVE`) and
//! has the depth of the building's walls ([`SpriteDepth::sub_span`]), so walkers and buildings
//! in front still cover it.
//!
//! Who is inside is counted from the citizens' public motion state once per simulation tick
//! (one pass into a flat array by building slot, only while some building in view needs it);
//! only buildings in view are looked at.

use std::collections::BTreeMap;

use glam::IVec2;
use kingdoms_engine::assets::{AssetManager, SpriteId, SpriteRegion};
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::sprite::{FLAG_EMISSIVE, FLAG_FLIP_X, RenderList, SpriteInstance};
use kingdoms_sim::buildings::{BuildingId, BuildingPhase};
use kingdoms_sim::defs::{BuildingCategory, Defs};
use kingdoms_sim::production::ProductionStatus;
use kingdoms_sim::state::GameState;

use super::buildings::{BuildingSprites, Part};
use super::daylight::{Daylight, residents_awake};
use super::{hash01, tile_hash};

/// Depth of the light in front of the walls it lies on.
const LIGHT_BIAS: f32 = 0.004;

/// Whether a building's windows are lit (before the time-of-day fade).
pub fn windows_lit(category: BuildingCategory, people_inside: u16, producing: bool, operational: bool, awake: bool) -> bool {
    if !operational {
        return false;
    }
    match category {
        BuildingCategory::Housing => people_inside > 0 && awake,
        _ => people_inside > 0 || producing,
    }
}

/// Horizontal span `(u0, u1)` of an overlay sprite within the building sprite it is drawn on,
/// both anchored at the same world point (`anchor_x` normalised, already mirrored for flipped
/// sprites), as fractions of the building sprite's width.
pub fn span_within(building_width: f32, building_anchor_x: f32, light_width: f32, light_anchor_x: f32) -> (f32, f32) {
    let width = building_width.max(1e-3);
    let left = building_anchor_x * building_width - light_anchor_x * light_width;
    (left / width, (left + light_width) / width)
}

/// Statistics for the debug window.
#[derive(Clone, Copy, Debug, Default)]
pub struct LightStats {
    /// Buildings with lit windows drawn this frame.
    pub lit: usize,
    /// Strength of the window lights (0–1).
    pub glow: f32,
}

/// Night lights of all buildings.
pub struct WindowLights {
    /// Light sprite by finished building sprite.
    by_sprite: BTreeMap<SpriteId, SpriteRegion>,
    /// People inside each building by building slot (`BuildingId::index`), counted at
    /// `counted_tick`.
    inside: Vec<u16>,
    counted_tick: Option<u64>,
    stats: LightStats,
}

impl WindowLights {
    pub fn new(defs: &Defs, assets: &AssetManager) -> Self {
        let mut by_sprite = BTreeMap::new();
        for def in defs.buildings() {
            let (Some(building), Some(light)) = (assets.try_sprite(&def.sprite()), assets.try_sprite(&format!("lights/{}", def.key)))
            else {
                continue;
            };
            by_sprite.insert(building, *assets.region(light));
        }
        Self { by_sprite, inside: Vec::new(), counted_tick: None, stats: LightStats::default() }
    }

    pub fn stats(&self) -> LightStats {
        self.stats
    }

    /// Recounts the people inside every building once per simulation tick.
    fn count_inside(&mut self, state: &GameState) {
        let tick = state.clock().tick;
        if self.counted_tick == Some(tick) {
            return;
        }
        self.counted_tick = Some(tick);
        self.inside.iter_mut().for_each(|n| *n = 0);
        for (_, c) in state.population().citizens() {
            if let Some(b) = c.motion().inside() {
                let slot = b.index() as usize;
                if slot >= self.inside.len() {
                    self.inside.resize(slot + 1, 0);
                }
                self.inside[slot] = self.inside[slot].saturating_add(1);
            }
        }
    }

    /// People inside building `id` as of the last count.
    fn inside_of(&self, id: BuildingId) -> u16 {
        self.inside.get(id.index() as usize).copied().unwrap_or(0)
    }

    /// Queues the lit windows of the buildings in view for the light of `daylight`.
    pub fn draw(
        &mut self,
        list: &mut RenderList,
        state: &GameState,
        defs: &Defs,
        sprites: &BuildingSprites,
        assets: &AssetManager,
        daylight: &Daylight,
    ) {
        let glow = daylight.window_glow();
        self.stats = LightStats { lit: 0, glow };
        if glow <= 0.01 {
            return;
        }
        let view = list.visible_rect();
        let in_view: Vec<BuildingId> = state
            .buildings()
            .iter()
            .filter(|(_, b)| {
                b.is_operational()
                    && !matches!(b.phase(), BuildingPhase::Construction(_))
                    && super::buildings::footprint_may_be_visible(b.rect(), &view)
            })
            .map(|(id, _)| id)
            .collect();
        if in_view.is_empty() {
            return;
        }
        self.count_inside(state);
        for id in in_view {
            let Some(b) = state.buildings().get(id) else { continue };
            let Some(def) = defs.buildings().get(b.kind().index()) else { continue };
            let Some(visual) = sprites.visual(b, defs, state, assets) else { continue };
            // The finished house keeps its lights in front of its upgrade scaffolding.
            let house = visual.extra.as_ref().unwrap_or(&visual.main);
            let Some(light) = self.by_sprite.get(&house.sprite) else { continue };
            let inside = self.inside_of(id);
            let producing = b.production().is_some_and(|p| p.status() == ProductionStatus::Working);
            let awake = residents_awake(daylight.hour, hash01(tile_hash(b.rect().origin), 7));
            if !windows_lit(def.category, inside, producing, b.is_operational(), awake) {
                continue;
            }
            self.draw_light(list, assets, house, light, glow, tile_hash(b.rect().origin + IVec2::ONE));
            self.stats.lit += 1;
        }
    }

    fn draw_light(&self, list: &mut RenderList, assets: &AssetManager, part: &Part, light: &SpriteRegion, glow: f32, seed: u32) {
        let building = assets.region(part.sprite);
        let mut building_anchor = building.anchor;
        let mut anchor = light.anchor;
        if part.flip {
            building_anchor.x = 1.0 - building_anchor.x;
            anchor.x = 1.0 - anchor.x;
        }
        let (u0, u1) = span_within(building.size.x, building_anchor.x, light.size.x, anchor.x);
        let mut depth = part.depth().sub_span(u0, u1);
        for d in &mut depth.tent[..3] {
            *d += LIGHT_BIAS;
        }
        // Candle light varies a little from house to house.
        let warmth = 0.9 + 0.1 * hash01(seed, 3);
        let mut instance = SpriteInstance::new(part.front_corner(), light.size, anchor, light.uv, depth)
            .with_color(Color::rgba(1.0, warmth, warmth * warmth, glow))
            .with_flags(FLAG_EMISSIVE);
        if part.flip {
            instance = instance.with_flags(FLAG_FLIP_X);
        }
        list.draw_transparent(light.texture, instance);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_has_lights() {
        // Houses: residents at home and awake.
        assert!(windows_lit(BuildingCategory::Housing, 3, false, true, true));
        assert!(!windows_lit(BuildingCategory::Housing, 3, false, true, false));
        assert!(!windows_lit(BuildingCategory::Housing, 0, false, true, true));
        // Workshops: workers inside or production running; never while paused.
        assert!(windows_lit(BuildingCategory::Production, 1, false, true, false));
        assert!(windows_lit(BuildingCategory::Production, 0, true, true, false));
        assert!(!windows_lit(BuildingCategory::Production, 0, false, true, true));
        assert!(!windows_lit(BuildingCategory::Production, 2, true, false, true));
        // The tavern with guests.
        assert!(windows_lit(BuildingCategory::Public, 4, false, true, false));
    }

    #[test]
    fn light_span_matches_the_building_columns() {
        // A 128 px building anchored at 0.5; a 40 px light whose anchor lies 10 px right of
        // its own right edge (1.25) starts 50 px left of the anchor: columns 14..54.
        let (u0, u1) = span_within(128.0, 0.5, 40.0, 1.25);
        assert!((u0 - 14.0 / 128.0).abs() < 1e-6 && (u1 - 54.0 / 128.0).abs() < 1e-6);
        // Mirrored: both anchors are mirrored, the span is mirrored around the anchor.
        let (m0, m1) = span_within(128.0, 0.5, 40.0, 1.0 - 1.25);
        assert!((m0 - (1.0 - u1)).abs() < 1e-6 && (m1 - (1.0 - u0)).abs() < 1e-6);
    }
}
