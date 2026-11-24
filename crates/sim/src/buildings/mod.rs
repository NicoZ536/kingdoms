//! Buildings: entities, life cycle, occupancy grid, placement rules, construction, upgrades.
//!
//! ## Life cycle
//! ```text
//!  PlaceBuilding ──► Construction(New) ──(work done)──► Active ◄──► paused flag
//!                        │ CancelConstruction               │ UpgradeBuilding / automatic
//!                        ▼                                  ▼ (houses, hourly, unless locked)
//!                     removed                     Upgrading(target) ──► Active (as target type)
//!  MoveBuilding (Active) ──► Construction(Move) at the new spot ──► Active
//!  event damage (Active: fire, flood, storm) ──► Construction(Repair) ──► Active
//!  DemolishBuilding (any phase) ──► removed (50 % material refund; a ruin returns only the
//!                                   unused repair material and its goods)
//! ```
//!
//! ## Workers
//! Every building has a `staffed` count: the workers (active buildings) or builders
//! (construction sites, upgrades) currently assigned. The demand is [`Building::job_demand`].
//! The job allocation in [`crate::workforce`] assigns citizens and keeps `staffed` equal to the
//! number of citizens employed at the building; every other system only *reads* `staffed`.
//!
//! ## Construction material
//! Gold is paid when a site is placed; materials are delivered by carriers from the storages
//! ([`crate::logistics`]). Builders can only work as far as the delivered materials allow, and a
//! site asks for builders only while it can progress ([`ConstructionSite::can_progress`]).
//!
//! ## Construction work
//! Work is measured in work points; one builder working one tick at normal speed contributes
//! [`WORK_PER_BUILDER_TICK`]. The crate-internal `construction::apply_construction_work(state,
//! defs, building, work, events)` is the single entry point for adding labour to a site
//! (the builder citizens assigned to the site, weighted by their work efficiency); it also
//! completes the site.

mod access;
pub(crate) mod construction;
mod placement;
mod upgrade;

use glam::IVec2;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::defs::{BuildingDef, BuildingRole, BuildingTypeId, Defs, JobCategory, ResourceAmount, ResourceId};
use crate::grid::{ChangeTracker, Direction, TileRect};
use crate::ids::{Arena, Id};
use crate::policy::JobPriority;
use crate::production::ProductionState;
use crate::world::{MAX_MAP_EDGE, WorldMap};

pub use access::{AccessPoint, access_of, access_points, is_open_sided};
pub(crate) use access::access_components;
pub use crate::economy::Inventory;
pub use crate::grid::{InvalidRotation, Rotation};
pub use construction::{
    DemolishRefund, SiteWork, WORK_PER_BUILDER_TICK, WorkOutcome, demolish_refund, move_allowed, pause_allowed, site_work,
    work_per_builder_tick,
};
pub(crate) use construction::{
    can_be_damaged, cancel_construction, damage_building, demolish_building, destroy_structure, move_building, place_building,
    run_construction, set_paused,
    set_upgrade_locked, spawn_finished, upgrade_building,
};
pub use placement::{
    Clearing, EntranceStatus, MIN_FARMLAND_PERCENT, PlacementCheck, PlacementError, PlacementWarning, TileBlock,
    TileStatus, check_move, check_placement,
};
pub(crate) use placement::{PlacementOptions, check_placement_with, clear_vegetation};
pub use upgrade::{FRESH_WATER_RADIUS, ServiceGap, UpgradeBlocker, upgrade_blockers, upgrade_service_gaps, upgrade_target};
pub(crate) use upgrade::service_reaches;

/// Handle of a building.
pub type BuildingId = Id<Building>;

/// Most builders that work on one construction site at the same time.
pub const MAX_BUILDERS_PER_SITE: u8 = 4;

/// Share of the build time needed to move a building, in percent (docs: 30 %).
pub const MOVE_WORK_PERCENT: u32 = 30;

/// Share of the material cost refunded on demolition, in percent.
pub const DEMOLISH_REFUND_PERCENT: u32 = 50;

/// What a construction site produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SitePurpose {
    /// A newly placed building.
    New,
    /// An existing building being re-erected at a new spot (no material cost).
    Move,
    /// A building damaged by an event (fire, flood, storm) being repaired: a ruin until the
    /// builders have done the first third (docs/game_design.md §12).
    Repair,
}

/// Progress of construction work and material delivery.
///
/// Carriers deliver the required materials over time ([`ConstructionSite::delivered`]). Work
/// can only advance as far as the delivered materials allow ([`ConstructionSite::work_limit`]),
/// and materials count as *consumed* in proportion to the work done (the delivered but
/// unconsumed rest is refunded on cancellation).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructionSite {
    purpose: SitePurpose,
    work_done: u32,
    work_required: u32,
    gold_paid: u32,
    materials_required: Vec<ResourceAmount>,
    materials_delivered: Vec<ResourceAmount>,
    started: bool,
    /// Tick in which builders last added work (`u64::MAX`: never).
    last_work_tick: u64,
}

impl ConstructionSite {
    /// A site needing `work_required` work points (at least 1). `materials_delivered` lists the
    /// materials already on site (sorted by resource id, usually empty).
    pub(crate) fn new(
        purpose: SitePurpose,
        work_required: u32,
        gold_paid: u32,
        materials_required: Vec<ResourceAmount>,
        materials_delivered: Vec<ResourceAmount>,
    ) -> Self {
        Self {
            purpose,
            work_done: 0,
            work_required: work_required.max(1),
            gold_paid,
            materials_required,
            materials_delivered,
            started: false,
            last_work_tick: u64::MAX,
        }
    }

    /// What the site produces.
    pub fn purpose(&self) -> SitePurpose {
        self.purpose
    }

    /// Work points done so far.
    pub fn work_done(&self) -> u32 {
        self.work_done
    }

    /// Work points needed in total.
    pub fn work_required(&self) -> u32 {
        self.work_required
    }

    /// Remaining work points.
    pub fn remaining_work(&self) -> u32 {
        self.work_required - self.work_done
    }

    /// Progress 0.0–1.0.
    pub fn progress(&self) -> f32 {
        (f64::from(self.work_done) / f64::from(self.work_required)) as f32
    }

    /// True once all work is done.
    pub fn is_complete(&self) -> bool {
        self.work_done >= self.work_required
    }

    /// True once builders have worked on the site.
    pub fn is_started(&self) -> bool {
        self.started
    }

    /// Tick in which builders last added work (`None`: never).
    pub fn last_work_tick(&self) -> Option<u64> {
        (self.last_work_tick != u64::MAX).then_some(self.last_work_tick)
    }

    /// True if builders added work in tick `now` (e.g. `GameState::clock().tick` after a tick:
    /// dust, hammering sounds).
    pub fn progressed_at(&self, now: u64) -> bool {
        self.last_work_tick == now
    }

    /// Visual construction stage 0–2 (thirds of the work), for the `construction_<B>x<T>_<stage>`
    /// sprites.
    pub fn stage(&self) -> u8 {
        (u64::from(self.work_done) * 3 / u64::from(self.work_required)).min(2) as u8
    }

    /// Gold paid for the site.
    pub fn gold_paid(&self) -> u32 {
        self.gold_paid
    }

    /// Materials the finished building needs.
    pub fn materials_required(&self) -> &[ResourceAmount] {
        &self.materials_required
    }

    /// Units of `resource` delivered to the site.
    pub fn delivered(&self, resource: ResourceId) -> u32 {
        self.materials_delivered.iter().find(|m| m.resource == resource).map_or(0, |m| m.amount)
    }

    /// Highest `work_done` the delivered materials allow.
    pub fn work_limit(&self) -> u32 {
        let mut limit = u64::from(self.work_required);
        for m in &self.materials_required {
            if m.amount == 0 {
                continue;
            }
            let delivered = u64::from(self.delivered(m.resource).min(m.amount));
            limit = limit.min(u64::from(self.work_required) * delivered / u64::from(m.amount));
        }
        limit as u32
    }

    /// True if builders can work now (work remains and the delivered materials allow more).
    pub fn can_progress(&self) -> bool {
        self.work_done < self.work_limit().min(self.work_required)
    }

    /// Units of `resource` still to be delivered.
    pub fn outstanding(&self, resource: ResourceId) -> u32 {
        let required = self.materials_required.iter().find(|m| m.resource == resource).map_or(0, |m| m.amount);
        required.saturating_sub(self.delivered(resource))
    }

    /// True once every required material is on site.
    pub fn fully_supplied(&self) -> bool {
        self.materials_required.iter().all(|m| self.delivered(m.resource) >= m.amount)
    }

    /// Accepts up to `amount` units of `resource` (limited to what is still outstanding);
    /// returns the units accepted.
    pub(crate) fn deliver(&mut self, resource: ResourceId, amount: u32) -> u32 {
        let accepted = amount.min(self.outstanding(resource));
        if accepted == 0 {
            return 0;
        }
        match self.materials_delivered.iter_mut().find(|m| m.resource == resource) {
            Some(m) => m.amount += accepted,
            None => {
                self.materials_delivered.push(ResourceAmount { resource, amount: accepted });
                self.materials_delivered.sort_by_key(|m| m.resource);
            }
        }
        accepted
    }

    /// Units of `resource` built into the structure so far (proportional to the work done).
    pub fn consumed(&self, resource: ResourceId) -> u32 {
        let required = self.materials_required.iter().find(|m| m.resource == resource).map_or(0, |m| m.amount);
        (u64::from(required) * u64::from(self.work_done) / u64::from(self.work_required)) as u32
    }

    /// Delivered materials not yet built in (refunded completely on cancellation).
    pub fn unused_materials(&self) -> Vec<ResourceAmount> {
        self.materials_delivered
            .iter()
            .map(|m| ResourceAmount { resource: m.resource, amount: m.amount.saturating_sub(self.consumed(m.resource)) })
            .filter(|m| m.amount > 0)
            .collect()
    }

    /// Paid gold attributable to the work not yet done (refunded on cancellation).
    pub fn unused_gold(&self) -> u32 {
        let used = u64::from(self.gold_paid) * u64::from(self.work_done) / u64::from(self.work_required);
        self.gold_paid - used as u32
    }

    /// Adds up to `work` points (limited by the remaining work and the delivered materials) and
    /// returns how many were applied.
    pub(crate) fn add_work(&mut self, work: u32) -> u32 {
        let limit = self.work_limit().min(self.work_required);
        let applied = work.min(limit.saturating_sub(self.work_done));
        self.work_done += applied;
        if applied > 0 {
            self.started = true;
        }
        applied
    }

    /// Records that builders worked in tick `now`.
    pub(crate) fn mark_worked(&mut self, now: u64) {
        self.last_work_tick = now;
    }
}

/// An upgrade of a building to its next level (houses: the next tier).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeProject {
    target: BuildingTypeId,
    site: ConstructionSite,
    previous_rect: TileRect,
    previous_entrance: IVec2,
}

impl UpgradeProject {
    /// Building type after the upgrade.
    pub fn target(&self) -> BuildingTypeId {
        self.target
    }

    /// Work and material progress.
    pub fn site(&self) -> &ConstructionSite {
        &self.site
    }

    /// Footprint before the upgrade (restored when the upgrade is cancelled).
    pub fn previous_rect(&self) -> TileRect {
        self.previous_rect
    }
}

/// Life-cycle phase of a building.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildingPhase {
    /// Construction site (new building or a building being moved).
    Construction(ConstructionSite),
    /// Finished and working (unless paused).
    Active,
    /// Finished and being upgraded: residents stay, stock and storage stay usable; the
    /// worker slots go to the builders, so the building's own trade rests meanwhile.
    Upgrading(UpgradeProject),
}

/// Summary status for the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildingStatus {
    /// New building under construction.
    UnderConstruction,
    /// Being re-erected at a new spot.
    Moving,
    /// Finished, being upgraded.
    Upgrading,
    /// Finished and working.
    Active,
    /// Finished and stopped by the player.
    Paused,
    /// Damaged by an event and waiting for (or under) repair.
    Damaged,
}

impl BuildingStatus {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            BuildingStatus::UnderConstruction => "Im Bau",
            BuildingStatus::Moving => "Wird versetzt",
            BuildingStatus::Upgrading => "Wird ausgebaut",
            BuildingStatus::Active => "In Betrieb",
            BuildingStatus::Paused => "Stillgelegt",
            BuildingStatus::Damaged => "Beschädigt",
        }
    }
}

/// A building on the map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Building {
    pub(crate) kind: BuildingTypeId,
    pub(crate) rect: TileRect,
    pub(crate) rotation: Rotation,
    pub(crate) entrance: IVec2,
    pub(crate) phase: BuildingPhase,
    pub(crate) paused: bool,
    pub(crate) staffed: u8,
    /// Storage contents (storage buildings only; capacities set on completion).
    pub(crate) inventory: Inventory,
    /// Production state (buildings whose type has production).
    pub(crate) production: Option<ProductionState>,
    /// Priority for workers and carriers.
    pub(crate) priority: JobPriority,
    /// Houses only: the player has locked automatic upgrades.
    pub(crate) upgrade_locked: bool,
}

impl Building {
    /// A building of type `def` at `origin` with `rotation` in `phase`.
    pub(crate) fn new(def: &BuildingDef, origin: IVec2, rotation: Rotation, phase: BuildingPhase) -> Self {
        Self {
            kind: def.id,
            rect: rotation.footprint(origin, def.size),
            rotation,
            entrance: rotation.entrance(origin, def.size),
            phase,
            paused: false,
            staffed: 0,
            inventory: Inventory::default(),
            production: def.production.as_ref().map(ProductionState::new),
            priority: JobPriority::Normal,
            upgrade_locked: false,
        }
    }

    /// Building type.
    pub fn kind(&self) -> BuildingTypeId {
        self.kind
    }

    /// Occupied tiles.
    pub fn rect(&self) -> TileRect {
        self.rect
    }

    /// Minimum corner of the footprint.
    pub fn origin(&self) -> IVec2 {
        self.rect.origin
    }

    /// Footprint size after rotation.
    pub fn size(&self) -> IVec2 {
        self.rect.size
    }

    /// Orientation.
    pub fn rotation(&self) -> Rotation {
        self.rotation
    }

    /// Side the entrance faces.
    pub fn facing(&self) -> Direction {
        self.rotation.facing()
    }

    /// Tile in front of the entrance (where a road connects).
    pub fn entrance(&self) -> IVec2 {
        self.entrance
    }

    /// Footprint tile adjacent to the entrance.
    pub fn door(&self) -> IVec2 {
        self.entrance - self.rotation.facing().offset()
    }

    /// True if the entrance tile lies on the map at most one elevation level above or below
    /// the door (movement between neighbours needs Δh ≤ 1, docs/game_design.md §5.6, §6), so a
    /// road on it can serve the building.
    pub fn entrance_reachable(&self, world: &WorldMap) -> bool {
        match (world.get(self.door()), world.get(self.entrance)) {
            (Some(door), Some(entrance)) => door.elevation.abs_diff(entrance.elevation) <= 1,
            _ => false,
        }
    }

    /// Life-cycle phase.
    pub fn phase(&self) -> &BuildingPhase {
        &self.phase
    }

    /// Summary status. A paused construction site keeps its construction status; check
    /// [`Building::is_paused`] as well.
    pub fn status(&self) -> BuildingStatus {
        match &self.phase {
            BuildingPhase::Construction(site) if site.purpose == SitePurpose::Move => BuildingStatus::Moving,
            BuildingPhase::Construction(site) if site.purpose == SitePurpose::Repair => BuildingStatus::Damaged,
            BuildingPhase::Construction(_) => BuildingStatus::UnderConstruction,
            BuildingPhase::Upgrading(_) => BuildingStatus::Upgrading,
            BuildingPhase::Active if self.paused => BuildingStatus::Paused,
            BuildingPhase::Active => BuildingStatus::Active,
        }
    }

    /// True if stopped by the player (no workers/builders are assigned while paused).
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// True once finished (active or being upgraded).
    pub fn is_complete(&self) -> bool {
        matches!(self.phase, BuildingPhase::Active | BuildingPhase::Upgrading(_))
    }

    /// True if the building is finished, not paused and fully usable.
    pub fn is_operational(&self) -> bool {
        self.is_complete() && !self.paused
    }

    /// The construction site of a new/moving building or of a running upgrade.
    pub fn construction_site(&self) -> Option<&ConstructionSite> {
        match &self.phase {
            BuildingPhase::Construction(site) => Some(site),
            BuildingPhase::Upgrading(up) => Some(&up.site),
            BuildingPhase::Active => None,
        }
    }

    pub(crate) fn construction_site_mut(&mut self) -> Option<&mut ConstructionSite> {
        match &mut self.phase {
            BuildingPhase::Construction(site) => Some(site),
            BuildingPhase::Upgrading(up) => Some(&mut up.site),
            BuildingPhase::Active => None,
        }
    }

    /// Workers (or builders) currently assigned.
    pub fn staffed(&self) -> u8 {
        self.staffed
    }

    /// Workers of the building's own trade (production, service, carriers, trainers): the
    /// assigned staff while the building is active, 0 while builders hold its slots (a site
    /// or a running upgrade — an upgraded workshop rests until the upgrade is done).
    pub fn trade_staff(&self) -> u8 {
        if matches!(self.phase, BuildingPhase::Active) { self.staffed } else { 0 }
    }

    /// True while an upgrade is running.
    pub fn is_upgrading(&self) -> bool {
        matches!(self.phase, BuildingPhase::Upgrading(_))
    }

    /// Job category and number of slots this building currently wants filled: builders
    /// (category [`JobCategory::Construction`]) while a site or upgrade can progress (materials
    /// delivered), the building's own workers while active, nothing while paused.
    pub fn job_demand(&self, defs: &Defs) -> Option<(JobCategory, u8)> {
        if self.paused {
            return None;
        }
        match &self.phase {
            BuildingPhase::Construction(site) => {
                site.can_progress().then(|| (JobCategory::Construction, builders_wanted(self.rect)))
            }
            BuildingPhase::Upgrading(up) => {
                up.site.can_progress().then(|| (JobCategory::Construction, builders_wanted(self.rect)))
            }
            BuildingPhase::Active => {
                let def = defs.buildings().get(self.kind.index())?;
                def.job.filter(|_| def.workers > 0).map(|job| (job.category, def.workers))
            }
        }
    }

    /// Worker (or builder) slots wanted now (0 without demand).
    pub fn workers_wanted(&self, defs: &Defs) -> u8 {
        self.job_demand(defs).map_or(0, |(_, n)| n)
    }

    /// Storage contents (empty for buildings without storage).
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// Production state (`None` for buildings without production).
    pub fn production(&self) -> Option<&ProductionState> {
        self.production.as_ref()
    }

    /// Priority for workers and carriers.
    pub fn priority(&self) -> JobPriority {
        self.priority
    }

    /// True if the player has locked automatic upgrades of this house (manual upgrades stay
    /// possible).
    pub fn is_upgrade_locked(&self) -> bool {
        self.upgrade_locked
    }

    /// Every good held by the building (storage contents and production buffers), merged per
    /// resource in id order.
    pub fn all_goods(&self) -> Vec<ResourceAmount> {
        let mut list: Vec<ResourceAmount> = self.inventory.iter().collect();
        if let Some(p) = &self.production {
            list.extend(p.input().iter());
            list.extend(p.output().iter());
        }
        merge_amounts(list)
    }

    /// Removes and returns every good held (storage contents and production buffers).
    pub(crate) fn take_all_goods(&mut self) -> Vec<ResourceAmount> {
        let mut list = self.inventory.take_all();
        if let Some(p) = self.production.as_mut() {
            list.extend(p.input.take_all());
            list.extend(p.output.take_all());
        }
        merge_amounts(list)
    }

    /// Housing tier of the building type (`None` for non-housing).
    pub fn housing_tier(&self, defs: &Defs) -> Option<u8> {
        defs.buildings().get(self.kind.index())?.housing.as_ref().map(|h| h.tier)
    }

    /// True if the footprint covers `p`.
    pub fn occupies(&self, p: IVec2) -> bool {
        self.rect.contains(p)
    }

    /// Sprite to draw: `buildings/construction_<B>x<T>_<stage>` (unrotated type size) while a
    /// new building or a move is under construction, `buildings/rubble_<B>x<T>` for a damaged
    /// building until its repair reaches stage 1, otherwise the type's sprite
    /// (`buildings/<key>`; upgrading houses keep their current look).
    pub fn sprite(&self, defs: &Defs) -> String {
        let Some(def) = defs.buildings().get(self.kind.index()) else { return String::from("buildings/unknown") };
        match &self.phase {
            BuildingPhase::Construction(site) if site.purpose == SitePurpose::Repair && site.stage() == 0 => {
                format!("buildings/rubble_{}x{}", def.size.x, def.size.y)
            }
            BuildingPhase::Construction(site) => {
                format!("buildings/construction_{}x{}_{}", def.size.x, def.size.y, site.stage())
            }
            _ => def.sprite(),
        }
    }

    pub(crate) fn set_staffed(&mut self, staffed: u8) {
        self.staffed = staffed;
    }
}

/// Sums amounts per resource, sorted by resource id.
pub(crate) fn merge_amounts(mut list: Vec<ResourceAmount>) -> Vec<ResourceAmount> {
    list.sort_by_key(|m| m.resource);
    let mut out: Vec<ResourceAmount> = Vec::with_capacity(list.len());
    for m in list.into_iter().filter(|m| m.amount > 0) {
        match out.last_mut() {
            Some(last) if last.resource == m.resource => last.amount = last.amount.saturating_add(m.amount),
            _ => out.push(m),
        }
    }
    out
}

/// Builders a site of this footprint uses: half the area, 1 ..= [`MAX_BUILDERS_PER_SITE`].
pub fn builders_wanted(rect: TileRect) -> u8 {
    let half = (rect.area() + 1) / 2;
    half.clamp(1, i32::from(MAX_BUILDERS_PER_SITE)) as u8
}

/// A footprint could not be registered in the occupancy grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum OccupancyError {
    /// Part of the footprint is outside the map.
    #[error("footprint {0} leaves the map")]
    OutOfBounds(TileRect),
    /// A tile is already occupied by another building.
    #[error("tile {0} is already occupied")]
    Occupied(IVec2),
    /// The stored grid size is not positive or exceeds [`MAX_MAP_EDGE`] (damaged save).
    #[error("occupancy grid size {width}x{height} is outside 1..={MAX_MAP_EDGE}")]
    InvalidGridSize {
        /// Stored width.
        width: i32,
        /// Stored height.
        height: i32,
    },
}

/// All buildings plus the occupancy grid (tile → building).
///
/// The grid is derived data: it is not serialized and is rebuilt from the buildings'
/// footprints when a save is loaded (`try_from` below), which also rejects grid sizes beyond
/// [`MAX_MAP_EDGE`] and overlapping or out-of-map footprints in damaged saves.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "BuildingsRepr")]
pub struct Buildings {
    arena: Arena<Building>,
    width: i32,
    height: i32,
    #[serde(skip)]
    occupancy: Vec<Option<BuildingId>>,
    #[serde(skip)]
    changes: ChangeTracker,
    /// Buildings that became active (finished, moved, upgraded) so far; not saved.
    #[serde(skip)]
    activations: u64,
}

/// Serialized form of [`Buildings`] (same field order as its non-skipped fields).
#[derive(Deserialize)]
struct BuildingsRepr {
    arena: Arena<Building>,
    width: i32,
    height: i32,
}

impl TryFrom<BuildingsRepr> for Buildings {
    type Error = OccupancyError;

    fn try_from(repr: BuildingsRepr) -> Result<Self, Self::Error> {
        // Bound the size before allocating the grid (a few bytes in a save could otherwise
        // request an absurd allocation).
        if !(1..=MAX_MAP_EDGE).contains(&repr.width) || !(1..=MAX_MAP_EDGE).contains(&repr.height) {
            return Err(OccupancyError::InvalidGridSize { width: repr.width, height: repr.height });
        }
        let mut out = Buildings::new(repr.width, repr.height);
        let entries: Vec<(BuildingId, TileRect)> = repr.arena.iter().map(|(id, b)| (id, b.rect)).collect();
        out.arena = repr.arena;
        for (id, rect) in entries {
            out.mark(id, rect)?;
        }
        Ok(out)
    }
}

impl Buildings {
    /// No buildings on a `width` × `height` map.
    pub fn new(width: i32, height: i32) -> Self {
        let len = (width.max(0) as usize) * (height.max(0) as usize);
        Self {
            arena: Arena::new(),
            width,
            height,
            occupancy: vec![None; len],
            changes: ChangeTracker::default(),
            activations: 0,
        }
    }

    /// Map size the grid was built for.
    pub fn grid_size(&self) -> IVec2 {
        IVec2::new(self.width, self.height)
    }

    /// Number of buildings (incl. construction sites).
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    /// True if there are no buildings.
    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// The building behind `id` (`None` if it no longer exists).
    pub fn get(&self, id: BuildingId) -> Option<&Building> {
        self.arena.get(id)
    }

    pub(crate) fn get_mut(&mut self, id: BuildingId) -> Option<&mut Building> {
        self.arena.get_mut(id)
    }

    /// All buildings in ascending id order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (BuildingId, &Building)> + '_ {
        self.arena.iter()
    }

    pub(crate) fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = (BuildingId, &mut Building)> + '_ {
        self.arena.iter_mut()
    }

    fn index(&self, p: IVec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height).then(|| (p.y * self.width + p.x) as usize)
    }

    /// Id of the building covering tile `p`.
    pub fn id_at(&self, p: IVec2) -> Option<BuildingId> {
        self.index(p).and_then(|i| self.occupancy[i])
    }

    /// The building covering tile `p`.
    pub fn at(&self, p: IVec2) -> Option<(BuildingId, &Building)> {
        let id = self.id_at(p)?;
        self.arena.get(id).map(|b| (id, b))
    }

    /// Number of buildings of a type (incl. construction sites and upgrades towards it).
    pub fn count_of(&self, kind: BuildingTypeId) -> usize {
        self.arena
            .values()
            .filter(|b| b.kind == kind || matches!(&b.phase, BuildingPhase::Upgrading(u) if u.target == kind))
            .count()
    }

    /// Ids of all buildings intersecting `rect`, ascending.
    pub fn ids_in(&self, rect: TileRect) -> Vec<BuildingId> {
        let mut ids: Vec<BuildingId> = rect.tiles().filter_map(|p| self.id_at(p)).collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Ids of finished buildings whose type has `role`, ascending.
    pub fn with_role(&self, defs: &Defs, role: BuildingRole) -> Vec<BuildingId> {
        self.arena
            .iter()
            .filter(|(_, b)| defs.buildings().get(b.kind.index()).is_some_and(|d| d.role == role))
            .map(|(id, _)| id)
            .collect()
    }

    /// Adds a building whose footprint must be free.
    pub(crate) fn insert(&mut self, building: Building) -> Result<BuildingId, OccupancyError> {
        self.check_free(building.rect, None)?;
        let rect = building.rect;
        let id = self.arena.insert(building);
        self.mark(id, rect)?;
        self.changes.touch(rect.expanded(1));
        Ok(id)
    }

    /// Removes a building and frees its tiles.
    pub(crate) fn remove(&mut self, id: BuildingId) -> Option<Building> {
        let building = self.arena.remove(id)?;
        self.unmark(id, building.rect);
        self.changes.touch(building.rect.expanded(1));
        Some(building)
    }

    /// Changes footprint and entrance of a building (move, upgrade); the new footprint may
    /// overlap only the building's own old tiles.
    pub(crate) fn relocate(
        &mut self,
        id: BuildingId,
        rect: TileRect,
        rotation: Rotation,
        entrance: IVec2,
    ) -> Result<(), OccupancyError> {
        let Some(old) = self.arena.get(id).map(|b| b.rect) else { return Ok(()) };
        self.check_free(rect, Some(id))?;
        self.unmark(id, old);
        self.mark(id, rect)?;
        self.changes.touch(old.expanded(1));
        self.changes.touch(rect.expanded(1));
        if let Some(b) = self.arena.get_mut(id) {
            b.rect = rect;
            b.rotation = rotation;
            b.entrance = entrance;
        }
        Ok(())
    }

    fn check_free(&self, rect: TileRect, owner: Option<BuildingId>) -> Result<(), OccupancyError> {
        for p in rect.tiles() {
            let i = self.index(p).ok_or(OccupancyError::OutOfBounds(rect))?;
            if let Some(other) = self.occupancy[i]
                && Some(other) != owner
            {
                return Err(OccupancyError::Occupied(p));
            }
        }
        Ok(())
    }

    fn mark(&mut self, id: BuildingId, rect: TileRect) -> Result<(), OccupancyError> {
        self.check_free(rect, Some(id))?;
        for p in rect.tiles() {
            if let Some(i) = self.index(p) {
                self.occupancy[i] = Some(id);
            }
        }
        Ok(())
    }

    fn unmark(&mut self, id: BuildingId, rect: TileRect) {
        for p in rect.tiles() {
            if let Some(i) = self.index(p)
                && self.occupancy[i] == Some(id)
            {
                self.occupancy[i] = None;
            }
        }
    }

    /// Number of footprint changes (placements, removals, moves, upgrades) so far; not saved,
    /// 0 after loading.
    pub fn revision(&self) -> u64 {
        self.changes.revision()
    }

    pub(crate) fn changes_mut(&mut self) -> &mut ChangeTracker {
        &mut self.changes
    }

    /// Counts a building that became active (systems that react to new buildings compare it).
    pub(crate) fn note_activation(&mut self) {
        self.activations = self.activations.wrapping_add(1);
    }

    /// Buildings that became active so far (not saved; 0 after loading).
    pub(crate) fn activations(&self) -> u64 {
        self.activations
    }

    /// Checks every building against the data (type ids, footprint/rotation/entrance
    /// consistency, inventories); used when validating a loaded state.
    pub(crate) fn validate(&self, defs: &Defs) -> Result<(), String> {
        self.arena.check_integrity().map_err(|e| e.to_string())?;
        for (id, b) in self.arena.iter() {
            if defs.buildings().get(b.kind.index()).is_none() {
                return Err(format!("building {id:?} has unknown type {}", b.kind));
            }
            if let BuildingPhase::Upgrading(up) = &b.phase
                && defs.buildings().get(b.kind.index()).and_then(|d| d.upgrade.as_ref()).map(|u| u.to) != Some(up.target)
            {
                return Err(format!("building {id:?} upgrades to {}, which is not its type's next level", up.target));
            }
            let size = footprint_base_size(defs, b);
            if b.rect != b.rotation.footprint(b.rect.origin, size) || b.entrance != b.rotation.entrance(b.rect.origin, size)
            {
                return Err(format!("building {id:?} has an inconsistent footprint or entrance"));
            }
            if let Some(site) = b.construction_site()
                && (site.work_done > site.work_required || site.work_required == 0)
            {
                return Err(format!("building {id:?} has invalid construction progress"));
            }
            let resources = defs.resources().len();
            if !b.inventory.is_canonical() || b.inventory.max_resource_index().is_some_and(|i| i >= resources) {
                return Err(format!("building {id:?} has a malformed inventory"));
            }
            if let Some(site) = b.construction_site()
                && site.materials_required.iter().any(|m| site.delivered(m.resource) > m.amount)
            {
                return Err(format!("building {id:?} has more material than required"));
            }
            let def = defs.buildings().get(b.kind.index());
            match (def.and_then(|d| d.production.as_ref()), &b.production) {
                (None, None) => {}
                (Some(pdef), Some(p)) => {
                    p.validate(pdef, resources).map_err(|e| format!("building {id:?}: {e}"))?;
                }
                _ => return Err(format!("building {id:?} has production state that does not match its type")),
            }
        }
        Ok(())
    }
}

/// Unrotated size that determines a building's footprint: the upgrade target's size while
/// upgrading (the footprint is reserved for the larger tier), otherwise the type's size.
pub(crate) fn footprint_base_size(defs: &Defs, b: &Building) -> IVec2 {
    let kind = match &b.phase {
        BuildingPhase::Upgrading(up) => up.target,
        _ => b.kind,
    };
    defs.buildings().get(kind.index()).map_or(IVec2::ONE, |d| d.size)
}

/// True if a road may run over the building's tile: gates are the passage through a wall line
/// (docs/game_design.md §5.5), so a road crossing the wall continues on the gate tile. Every
/// other building blocks roads.
pub fn carries_road(defs: &Defs, building: &Building) -> bool {
    defs.buildings().get(building.kind.index()).is_some_and(|d| d.role == BuildingRole::Gate)
}

/// True for a finished gate: it lets the player's people through but stops enemies
/// (docs/game_design.md §11).
pub fn is_gate(defs: &Defs, building: &Building) -> bool {
    building.is_complete() && carries_road(defs, building)
}

/// True for wall segments and gates (finished or not).
pub fn is_wall_or_gate(defs: &Defs, building: &Building) -> bool {
    defs.buildings()
        .get(building.kind.index())
        .is_some_and(|d| matches!(d.role, BuildingRole::WallSegment | BuildingRole::Gate))
}

/// Connection mask (N=1, E=2, S=4, W=8) of a wall segment or gate at `p` towards neighbouring
/// wall segments and gates; `None` if `p` holds neither.
pub fn wall_mask(buildings: &Buildings, defs: &Defs, p: IVec2) -> Option<u8> {
    let is_wall = |q: IVec2| {
        buildings.at(q).is_some_and(|(_, b)| {
            defs.buildings()
                .get(b.kind.index())
                .is_some_and(|d| matches!(d.role, BuildingRole::WallSegment | BuildingRole::Gate))
        })
    };
    if !is_wall(p) {
        return None;
    }
    Some(Direction::ALL.iter().filter(|d| is_wall(p + d.offset())).map(|d| d.mask_bit()).sum())
}

#[cfg(test)]
mod tests;
