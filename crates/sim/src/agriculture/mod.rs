//! Agriculture (docs/game_design.md §5.3, §8.3): field and pasture tiles claimed by farms,
//! crops with growth stages, sowing and harvesting labour, frost, soil fertility and moisture.
//!
//! ## Claims
//! Field farms (`Gathering::Fields`) and animal farms (`Gathering::Pasture`) claim tiles in their
//! radius **exclusively** ([`Agriculture::field_at`]), nearest first, up to their maximum: owned,
//! free of buildings, roads, deposits, trees and boulders and not in front of an entrance; fields
//! need arable soil with at least `agriculture.min_field_fertility` (bushes and stumps are
//! cleared), pastures grass, meadow or steppe. Buildings or roads placed on a claimed tile,
//! territory loss, demolition or a move of the farm release tiles (standing crops are lost).
//!
//! ## Pipeline (step 4, after nature)
//! 1. Farms are synchronised with the buildings: finished farm buildings get a [`Farm`] and
//!    claim their tiles at once, removed or moved ones release theirs; after any change of roads
//!    or footprints claimed tiles under them are released.
//! 2. On the first tick of winter frost kills unripe annual crops and spoils unharvested fruit.
//! 3. Every farm has an hourly slot (`building index mod TICKS_PER_HOUR`): it tops up its claims
//!    and grows its crops by one hour; in its slot of the first hour of the day it also applies
//!    the day's moisture, withering, storm damage and fallow regeneration.
//! 4. Every tick the field farms work: harvest ripe tiles first, then sow, with the
//!    productivity of their workers × season/weather × research × bankruptcy (see
//!    [`crate::production`]). The harvest goes into the farm's output buffer, carriers take it
//!    to storage.
//!
//! Growth points: one hour at growth factor 1.0 adds [`GROWTH_PER_HOUR`]; a crop is ripe after
//! `growth_days × 24 ×` [`GROWTH_PER_HOUR`].

mod growth;
#[cfg(test)]
mod tests;
mod work;

use std::collections::{BTreeMap, BTreeSet};

use glam::IVec2;
use serde::{Deserialize, Serialize};

pub(crate) use growth::{destroy_crops, expected_harvest_milli, run_agriculture};
pub use growth::{growth_factor_permille, visual_stage};
pub(crate) use work::set_crop;

use crate::buildings::{Building, BuildingId};
use crate::defs::{BuildingDef, CropDef, CropId, Defs, Gathering, ResourceId};
use crate::grid::TileRect;
use crate::state::GameState;
use crate::territory::Territory;
use crate::time::{HOURS_PER_DAY, Season};
use crate::world::{CHUNK_SIZE, Terrain, Tile, Vegetation};

/// Growth points of one hour at growth factor 1.0.
pub const GROWTH_PER_HOUR: u32 = 1000;

/// Growth points of one day at growth factor 1.0.
pub const GROWTH_PER_DAY: u32 = GROWTH_PER_HOUR * HOURS_PER_DAY as u32;

/// Highest visual growth stage (sprites `<crop>_0..4`).
pub const MAX_STAGE: u8 = 4;

/// What a farm keeps on its tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FarmKind {
    /// Crops or orchard trees.
    Fields,
    /// Grazing animals.
    Pasture,
}

impl FarmKind {
    /// German display name of one tile.
    pub const fn tile_name(self) -> &'static str {
        match self {
            FarmKind::Fields => "Feld",
            FarmKind::Pasture => "Weide",
        }
    }
}

/// Farm kind, tile limit and radius of a building type (`None` if it does not farm land).
pub fn farm_spec(def: &BuildingDef) -> Option<(FarmKind, u16, u8)> {
    match def.production.as_ref()?.gathering.as_ref()? {
        Gathering::Fields { max_tiles, radius, .. } => Some((FarmKind::Fields, *max_tiles, *radius)),
        Gathering::Pasture { tiles, radius } => Some((FarmKind::Pasture, *tiles, *radius)),
        _ => None,
    }
}

/// Crops a field farm type can grow (empty for other types).
pub fn farm_crops(def: &BuildingDef) -> &[CropId] {
    match def.production.as_ref().and_then(|p| p.gathering.as_ref()) {
        Some(Gathering::Fields { crops, .. }) => crops,
        _ => &[],
    }
}

/// One claimed tile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldTile {
    pub(crate) pos: IVec2,
    /// Crop growing here (`None`: fallow soil or pasture).
    pub(crate) crop: Option<CropId>,
    /// Growth points (annuals: until ripe; perennials: until the trees bear fruit).
    pub(crate) growth: u32,
    /// Perennials: fruit growth points since the last harvest.
    pub(crate) fruit: u32,
    /// Share of the full yield left after storms and withering, in percent.
    pub(crate) condition: u8,
    /// Soil fertility 0–255 (starts at the tile's natural fertility).
    pub(crate) fertility: u8,
    /// Soil moisture 0–255 (starts at the tile's natural moisture).
    pub(crate) moisture: u8,
    /// Visual stage last published to renderers.
    pub(crate) stage: u8,
}

impl FieldTile {
    fn new(pos: IVec2, tile: &Tile) -> Self {
        FieldTile {
            pos,
            crop: None,
            growth: 0,
            fruit: 0,
            condition: 100,
            fertility: tile.fertility,
            moisture: tile.moisture,
            stage: 0,
        }
    }

    /// Position on the map.
    pub fn pos(&self) -> IVec2 {
        self.pos
    }

    /// Crop growing here (`None`: fallow soil or pasture).
    pub fn crop(&self) -> Option<CropId> {
        self.crop
    }

    /// Growth points so far.
    pub fn growth(&self) -> u32 {
        self.growth
    }

    /// Perennials: fruit growth points since the last harvest.
    pub fn fruit(&self) -> u32 {
        self.fruit
    }

    /// Share of the full yield left after storms and withering, in percent.
    pub fn condition(&self) -> u8 {
        self.condition
    }

    /// Soil fertility 0–255.
    pub fn fertility(&self) -> u8 {
        self.fertility
    }

    /// Soil moisture 0–255.
    pub fn moisture(&self) -> u8 {
        self.moisture
    }

    /// Visual growth stage 0–4 (as drawn).
    pub fn stage(&self) -> u8 {
        self.stage
    }

    /// Growth towards ripeness (annuals) or bearing trees (perennials), 0.0–1.0.
    pub fn growth_fraction(&self, defs: &Defs) -> f32 {
        match self.crop.and_then(|c| defs.crops().get(c.index())) {
            Some(crop) => (f64::from(self.growth) / f64::from(required_growth(crop).max(1))).min(1.0) as f32,
            None => 0.0,
        }
    }

    /// True if the crop can be harvested now.
    pub fn is_ripe(&self, defs: &Defs, season: Season) -> bool {
        self.crop.and_then(|c| defs.crops().get(c.index())).is_some_and(|crop| is_ripe(self, crop, season))
    }
}

/// Growth points a crop needs (annuals: until ripe; perennials: until bearing).
pub fn required_growth(crop: &CropDef) -> u32 {
    days_to_points(crop.growth_days)
}

/// Fruit growth points perennials need for a full crop.
pub fn required_fruit(crop: &CropDef) -> u32 {
    days_to_points(crop.fruit_days)
}

fn days_to_points(days: f32) -> u32 {
    (f64::from(days) * f64::from(GROWTH_PER_DAY)).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// True if `tile` (growing `crop`) can be harvested in `season`.
pub(crate) fn is_ripe(tile: &FieldTile, crop: &CropDef, season: Season) -> bool {
    let grown = tile.growth >= required_growth(crop);
    if crop.perennial { grown && tile.fruit > 0 && crop.harvest_seasons.contains(&season) } else { grown }
}

/// Labour a farm performs on one tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskKind {
    /// Sowing an annual crop or planting fruit trees.
    Sow,
    /// Harvesting a ripe tile.
    Harvest,
}

impl TaskKind {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            TaskKind::Sow => "Aussaat",
            TaskKind::Harvest => "Ernte",
        }
    }
}

/// The running piece of farm work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FarmTask {
    /// Sowing or harvesting.
    pub kind: TaskKind,
    /// The tile worked on.
    pub tile: IVec2,
    /// Progress in milli-ticks (like production cycles).
    pub progress: u64,
    /// Milli-ticks needed at full productivity.
    pub required: u64,
}

impl FarmTask {
    /// Progress 0.0–1.0.
    pub fn fraction(&self) -> f32 {
        (self.progress as f64 / self.required.max(1) as f64).min(1.0) as f32
    }
}

/// A finished or running harvest: everything harvested since the first ripe tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarvestRecord {
    /// Calendar day ([`GameClock::total_days`](crate::time::GameClock::total_days)) of the last
    /// harvested tile.
    pub day: u64,
    /// Harvested resource.
    pub resource: ResourceId,
    /// Units.
    pub amount: u32,
}

/// Agricultural state of one farm building.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Farm {
    pub(crate) kind: FarmKind,
    /// Footprint when the farm started working (a moved farm starts over).
    pub(crate) rect: TileRect,
    /// Crop sown from now on (field farms).
    pub(crate) crop: Option<CropId>,
    /// Claimed tiles in claim order (nearest first).
    pub(crate) tiles: Vec<FieldTile>,
    pub(crate) task: Option<FarmTask>,
    /// Harvested milli-units not yet whole units, per resource.
    pub(crate) carry: BTreeMap<ResourceId, u32>,
    /// The harvest in progress.
    pub(crate) round: Option<HarvestRecord>,
    /// The last completed harvest.
    pub(crate) last_harvest: Option<HarvestRecord>,
    /// Animals of a pasture farm (0 for field farms).
    pub(crate) herd: u16,
    /// Hours a claimed field tile has been out of the farmers' reach, by tile (released after
    /// a day; field farms only).
    pub(crate) unreachable_hours: BTreeMap<TileKey, u8>,
}

impl Farm {
    /// Animals grazing on a pasture farm's land (0 for field farms).
    pub fn herd(&self) -> u16 {
        self.herd
    }

    /// Animals the claimed pasture feeds (`pasture_tiles_per_animal`); 0 for field farms.
    pub fn herd_capacity(&self, defs: &Defs) -> u16 {
        if self.kind != FarmKind::Pasture {
            return 0;
        }
        let per = usize::from(defs.rules().agriculture.pasture_tiles_per_animal.max(1));
        u16::try_from(self.tiles.len() / per).unwrap_or(u16::MAX)
    }

    /// Fields or pasture.
    pub fn kind(&self) -> FarmKind {
        self.kind
    }

    /// Crop sown from now on (field farms).
    pub fn crop(&self) -> Option<CropId> {
        self.crop
    }

    /// Claimed tiles (nearest first).
    pub fn tiles(&self) -> &[FieldTile] {
        &self.tiles
    }

    /// Running work.
    pub fn task(&self) -> Option<&FarmTask> {
        self.task.as_ref()
    }

    /// Harvest in progress (units so far).
    pub fn harvest_in_progress(&self) -> Option<HarvestRecord> {
        self.round
    }

    /// The last completed harvest.
    pub fn last_harvest(&self) -> Option<HarvestRecord> {
        self.last_harvest
    }

    fn tile_index(&self, p: IVec2) -> Option<usize> {
        self.tiles.iter().position(|t| t.pos == p)
    }
}

/// What a renderer needs to draw one claimed tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldView {
    /// The farm owning the tile.
    pub farm: BuildingId,
    /// Field or pasture.
    pub kind: FarmKind,
    /// Crop growing here.
    pub crop: Option<CropId>,
    /// Visual stage 0–4.
    pub stage: u8,
}

impl FieldView {
    /// Sprite in the `fields` atlas: `fields/pasture`, `fields/soil` (fallow) or
    /// `fields/<crop sprite>_<stage>`.
    pub fn sprite(&self, defs: &Defs) -> String {
        match (self.kind, self.crop.and_then(|c| defs.crops().get(c.index()))) {
            (FarmKind::Pasture, _) => "fields/pasture".to_owned(),
            (FarmKind::Fields, Some(crop)) => crop.stage_sprite(self.stage),
            (FarmKind::Fields, None) => "fields/soil".to_owned(),
        }
    }

    /// True if the tile shows bare trees in `season`: an orchard (perennial crop) past the
    /// sapling stage in winter. The chunks of such tiles are reported dirty when winter begins
    /// and ends.
    pub fn is_bare(&self, defs: &Defs, season: Season) -> bool {
        season == Season::Winter
            && self.stage >= 1
            && self.crop.and_then(|c| defs.crops().get(c.index())).is_some_and(|c| c.perennial)
    }

    /// [`FieldView::sprite`] for `season`: leafless orchard trees in winter
    /// (`fields/<crop sprite>_bare`, see [`FieldView::is_bare`]).
    pub fn seasonal_sprite(&self, defs: &Defs, season: Season) -> String {
        match self.crop.and_then(|c| defs.crops().get(c.index())) {
            Some(crop) if self.is_bare(defs, season) => format!("fields/{}_bare", crop.sprite),
            _ => self.sprite(defs),
        }
    }
}

/// Map key of a tile: row-major order (y, x).
type TileKey = (i32, i32);

fn key(p: IVec2) -> TileKey {
    (p.y, p.x)
}

/// All farms and the exclusive tile claims.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Agriculture {
    farms: BTreeMap<BuildingId, Farm>,
    claims: BTreeMap<TileKey, BuildingId>,
    #[serde(skip)]
    dirty_chunks: Vec<IVec2>,
    /// Structure revision (roads + footprints) and building activations seen by the last
    /// synchronisation with the buildings.
    #[serde(skip)]
    seen_revision: u64,
    #[serde(skip)]
    seen_activations: u64,
}

impl Agriculture {
    /// The farm state of a building.
    pub fn farm(&self, building: BuildingId) -> Option<&Farm> {
        self.farms.get(&building)
    }

    /// Mutable farm state (tests that prepare fields).
    #[cfg(test)]
    pub(crate) fn farm_mut(&mut self, building: BuildingId) -> Option<&mut Farm> {
        self.farms.get_mut(&building)
    }

    /// All farms in building id order.
    pub fn farms(&self) -> impl Iterator<Item = (BuildingId, &Farm)> + '_ {
        self.farms.iter().map(|(id, f)| (*id, f))
    }

    /// Average condition of all field tiles with a crop in percent (the share of the full yield
    /// that storms and withering left), `None` without sown tiles. Harvest events read it
    /// (docs/game_design.md §12).
    pub fn average_field_condition(&self) -> Option<u8> {
        let (mut tiles, mut sum) = (0u64, 0u64);
        for t in self.farms.values().flat_map(|f| f.tiles.iter()).filter(|t| t.crop.is_some()) {
            tiles += 1;
            sum += u64::from(t.condition);
        }
        (tiles > 0).then(|| u8::try_from(sum / tiles).unwrap_or(u8::MAX))
    }

    /// Number of claimed tiles.
    pub fn claimed_tiles(&self) -> usize {
        self.claims.len()
    }

    /// The farm claiming `p`.
    pub fn owner_of(&self, p: IVec2) -> Option<BuildingId> {
        self.claims.get(&key(p)).copied()
    }

    /// The claimed tile at `p`.
    pub fn tile_at(&self, p: IVec2) -> Option<&FieldTile> {
        let farm = self.farms.get(self.claims.get(&key(p))?)?;
        farm.tiles.iter().find(|t| t.pos == p)
    }

    /// What to draw at `p` (`None` if no farm claims it).
    pub fn field_at(&self, p: IVec2) -> Option<FieldView> {
        let id = *self.claims.get(&key(p))?;
        let farm = self.farms.get(&id)?;
        let tile = farm.tiles.iter().find(|t| t.pos == p)?;
        Some(FieldView { farm: id, kind: farm.kind, crop: tile.crop, stage: tile.stage })
    }

    /// Every claimed tile inside `rect` in row-major order (e.g. one render chunk).
    pub fn fields_in(&self, rect: TileRect) -> impl Iterator<Item = (IVec2, FieldView)> + '_ {
        let max = rect.max_inclusive();
        let from = (rect.origin.y, rect.origin.x);
        let to = (max.y, max.x);
        self.claims
            .range(from..=to)
            .filter(move |((_, x), _)| *x >= rect.origin.x && *x <= max.x)
            .filter_map(|((y, x), _)| {
                let p = IVec2::new(*x, *y);
                self.field_at(p).map(|v| (p, v))
            })
    }

    /// Takes the chunks whose field tiles changed visibly since the last call.
    pub(crate) fn take_dirty_chunks(&mut self) -> Vec<IVec2> {
        std::mem::take(&mut self.dirty_chunks)
    }

    pub(crate) fn mark_dirty(&mut self, p: IVec2) {
        let c = IVec2::new(p.x.div_euclid(CHUNK_SIZE), p.y.div_euclid(CHUNK_SIZE));
        if !self.dirty_chunks.contains(&c) {
            self.dirty_chunks.push(c);
        }
    }

    /// Adds a claimed tile to `farm` (the caller checked eligibility).
    fn claim(&mut self, id: BuildingId, tile: FieldTile) {
        let p = tile.pos;
        if let Some(farm) = self.farms.get_mut(&id) {
            self.claims.insert(key(p), id);
            farm.tiles.push(tile);
            self.mark_dirty(p);
        }
    }

    /// Releases the claim on `p` (crops on it are lost). Returns the farm that held it.
    fn release(&mut self, p: IVec2) -> Option<BuildingId> {
        let id = self.claims.remove(&key(p))?;
        if let Some(farm) = self.farms.get_mut(&id) {
            farm.tiles.retain(|t| t.pos != p);
            if farm.task.is_some_and(|t| t.tile == p) {
                farm.task = None;
            }
        }
        self.mark_dirty(p);
        Some(id)
    }

    /// Removes a farm and releases all its tiles.
    fn remove_farm(&mut self, id: BuildingId) {
        if let Some(farm) = self.farms.remove(&id) {
            for t in &farm.tiles {
                self.claims.remove(&key(t.pos));
                self.mark_dirty(t.pos);
            }
        }
    }

    /// Checks claims, farms and tiles against each other, the buildings and the roads.
    pub(crate) fn validate(&self, state: &GameState, defs: &Defs) -> Result<(), String> {
        let mut listed = 0usize;
        for (id, farm) in &self.farms {
            let b = state.buildings.get(*id).ok_or_else(|| format!("farm {id:?} has no building"))?;
            let def = defs.buildings().get(b.kind().index()).ok_or_else(|| format!("farm {id:?} has an unknown type"))?;
            let (kind, max, _) = farm_spec(def).ok_or_else(|| format!("{id:?} ({}) does not farm land", def.key))?;
            if kind != farm.kind || !b.is_complete() || b.rect() != farm.rect {
                return Err(format!("farm {id:?} does not match its building"));
            }
            if farm.tiles.len() > usize::from(max) {
                return Err(format!("farm {id:?} holds {} of at most {max} tiles", farm.tiles.len()));
            }
            let crops = farm_crops(def);
            match (kind, farm.crop) {
                (FarmKind::Fields, Some(c)) if crops.contains(&c) => {}
                (FarmKind::Pasture, None) => {}
                _ => return Err(format!("farm {id:?} has an invalid crop selection")),
            }
            let mut seen = BTreeSet::new();
            for t in &farm.tiles {
                if !seen.insert(key(t.pos)) {
                    return Err(format!("farm {id:?} lists {} twice", t.pos));
                }
                if self.claims.get(&key(t.pos)) != Some(id) {
                    return Err(format!("tile {} of farm {id:?} is not claimed by it", t.pos));
                }
                if !state.world.in_bounds(t.pos) || state.buildings.id_at(t.pos).is_some() || state.roads.has_road(t.pos) {
                    return Err(format!("field {} lies outside the map or under a building or road", t.pos));
                }
                let crop_ok = match t.crop {
                    None => t.growth == 0 && t.fruit == 0,
                    Some(c) => kind == FarmKind::Fields && c.index() < defs.crops().len(),
                };
                if !crop_ok || t.condition > 100 || t.stage > MAX_STAGE {
                    return Err(format!("field {} has invalid values", t.pos));
                }
            }
            if usize::from(farm.herd) > farm.tiles.len() || (kind == FarmKind::Fields && farm.herd > 0) {
                return Err(format!("farm {id:?} has an impossible herd of {}", farm.herd));
            }
            if farm.unreachable_hours.iter().any(|(k, h)| !seen.contains(k) || *h >= 24) {
                return Err(format!("farm {id:?} counts unreachable hours of tiles it does not hold"));
            }
            if let Some(task) = farm.task
                && (farm.tile_index(task.tile).is_none() || task.required == 0)
            {
                return Err(format!("farm {id:?} works on a tile it does not hold"));
            }
            if farm.carry.keys().any(|r| r.index() >= defs.resources().len()) {
                return Err(format!("farm {id:?} carries an unknown resource"));
            }
            listed += farm.tiles.len();
        }
        if listed != self.claims.len() {
            return Err(format!("{} claims but {listed} farm tiles", self.claims.len()));
        }
        Ok(())
    }

    /// Re-applies data-derived limits after loading rebalanced data: farms beyond their tile
    /// limit release the farthest tiles, a crop selection the farm type no longer offers falls
    /// back to the first crop. A no-op for unchanged data.
    pub(crate) fn rederive(&mut self, state_buildings: &crate::buildings::Buildings, defs: &Defs) {
        let ids: Vec<BuildingId> = self.farms.keys().copied().collect();
        for id in ids {
            let Some(def) = state_buildings.get(id).and_then(|b| defs.buildings().get(b.kind().index())) else { continue };
            let Some((_, max, _)) = farm_spec(def) else { continue };
            let extra: Vec<IVec2> =
                self.farms.get(&id).map(|f| f.tiles.iter().skip(usize::from(max)).map(|t| t.pos).collect()).unwrap_or_default();
            for p in extra {
                self.release(p);
            }
            let crops = farm_crops(def);
            if let Some(farm) = self.farms.get_mut(&id)
                && farm.kind == FarmKind::Fields
                && farm.crop.is_none_or(|c| !crops.contains(&c))
            {
                farm.crop = crops.first().copied();
            }
            // A herd larger than its (possibly smaller) pasture feeds is sold down.
            if let Some(farm) = self.farms.get_mut(&id) {
                farm.herd = farm.herd.min(farm.herd_capacity(defs));
            }
        }
    }

    /// Marks the chunks of every orchard (perennial crop) tile for redrawing (the trees drop or
    /// grow their leaves when winter begins or ends).
    pub(crate) fn mark_perennials_dirty(&mut self, defs: &Defs) {
        let tiles: Vec<IVec2> = self
            .farms
            .values()
            .flat_map(|f| f.tiles.iter())
            .filter(|t| t.crop.and_then(|c| defs.crops().get(c.index())).is_some_and(|c| c.perennial))
            .map(|t| t.pos)
            .collect();
        for p in tiles {
            self.mark_dirty(p);
        }
    }
}

/// True if tile `p` may be claimed for a farm of `kind` (claims of `owner` count as free):
/// in `territory`, no building, road, deposit, tree or boulder, not claimed by another farm,
/// not in `entrances`, and arable soil with enough fertility (fields) or grass, meadow or
/// steppe (pasture).
fn is_farmland(
    state: &GameState,
    defs: &Defs,
    p: IVec2,
    kind: FarmKind,
    owner: Option<BuildingId>,
    entrances: &BTreeSet<TileKey>,
    territory: &Territory,
) -> bool {
    let Some(tile) = state.world.get(p) else { return false };
    let terrain_ok = match kind {
        FarmKind::Fields => is_arable(defs, tile),
        FarmKind::Pasture => matches!(tile.terrain, Terrain::Grass | Terrain::Meadow | Terrain::Steppe),
    };
    terrain_ok
        && matches!(tile.vegetation, Vegetation::None | Vegetation::Bush | Vegetation::Stump { .. })
        && tile.deposit.is_none()
        && territory.contains(p)
        && state.buildings.id_at(p).is_none()
        && !state.roads.has_road(p)
        && state.agriculture.owner_of(p).is_none_or(|o| Some(o) == owner)
        && !entrances.contains(&key(p))
}

/// True if fields can grow on the soil of `tile`: grass, meadow, dirt or steppe with a
/// fertility (raw value 0–255) of at least `min_field_fertility` (docs/game_design.md §8.3).
pub fn is_arable(defs: &Defs, tile: &Tile) -> bool {
    matches!(tile.terrain, Terrain::Grass | Terrain::Meadow | Terrain::Dirt | Terrain::Steppe)
        && tile.fertility >= defs.rules().agriculture.min_field_fertility
}

/// Entrance tiles of all buildings near `area` (kept free of fields and pastures).
pub(crate) fn entrances_near(state: &GameState, area: TileRect) -> BTreeSet<TileKey> {
    state
        .buildings
        .ids_in(area.expanded(1).clipped(state.world.width(), state.world.height()))
        .into_iter()
        .filter_map(|id| state.buildings.get(id))
        .map(|b| key(b.entrance()))
        .collect()
}

/// Tiles around a footprint usable by a farm of `kind` within `radius`, nearest first
/// (placement check and claiming use the same rule; `territory` overrides the kingdom's
/// territory for placement previews).
pub(crate) fn farmland_around(
    state: &GameState,
    defs: &Defs,
    rect: TileRect,
    kind: FarmKind,
    radius: u8,
    owner: Option<BuildingId>,
    territory: Option<&Territory>,
) -> Vec<IVec2> {
    let territory = territory.unwrap_or(&state.territory);
    let r = i32::from(radius);
    let area = rect.expanded(r).clipped(state.world.width(), state.world.height());
    let entrances = entrances_near(state, area);
    let mut tiles: Vec<IVec2> = area
        .tiles()
        .filter(|p| !rect.contains(*p) && rect.distance_to(*p) <= r)
        .filter(|p| is_farmland(state, defs, *p, kind, owner, &entrances, territory))
        .collect();
    tiles.sort_by_key(|p| (rect.distance_to(*p), p.y, p.x));
    tiles
}

/// Number of claimed tiles of a pasture farm (0 for unknown buildings).
pub(crate) fn pasture_tiles(state: &GameState, building: BuildingId) -> usize {
    state.agriculture.farms.get(&building).map_or(0, |f| f.tiles.len())
}

/// True if `p` holds an orchard field (counts for the bee bonus).
pub(crate) fn is_orchard_field(state: &GameState, defs: &Defs, p: IVec2) -> bool {
    state
        .agriculture
        .tile_at(p)
        .and_then(|t| t.crop)
        .and_then(|c| defs.crops().get(c.index()))
        .is_some_and(|c| c.perennial)
}

/// True if a farm claims `p`.
pub(crate) fn is_claimed(state: &GameState, p: IVec2) -> bool {
    state.agriculture.owner_of(p).is_some()
}

/// The farm def and spec of a building that farms land.
fn farm_building<'a>(defs: &'a Defs, b: &Building) -> Option<(&'a BuildingDef, FarmKind, u16, u8)> {
    let def = defs.buildings().get(b.kind().index())?;
    let (kind, max, radius) = farm_spec(def)?;
    Some((def, kind, max, radius))
}
