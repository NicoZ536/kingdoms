//! Creates transport jobs from the needs of production buildings and construction sites.
//!
//! Clients are served by building priority (high first), then by id. For every need the
//! nearest suitable storage is chosen by footprint distance (ties by id): it must be
//! operational, linked to the client (`Link`: the same road network, or for buildings entered
//! from any side a walking connection from its entrance to one of their access tiles), cover
//! the client with its `carrier_radius` and have the goods (supply) or room (collection) not
//! yet claimed by other jobs.
//!
//! A storage takes new jobs only while fewer than [`BACKLOG_PER_CARRIER`] jobs per carrier
//! (at least that many) wait for its carriers; beyond that the next storage in reach is chosen
//! or the need waits for a later pass. The queue stays short and loads grow instead of piling
//! up as many small claims.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use super::{JobStage, TransportJob, TransportKind};
use crate::buildings::{self, BuildingId, BuildingPhase};
use crate::defs::{Defs, ResourceId};
use crate::economy::storage;
use crate::grid::TileRect;
use crate::policy::JobPriority;
use crate::production::ProductionStatus;
use crate::roads;
use crate::state::GameState;

/// Waiting jobs a storage may have per carrier before the planner looks elsewhere.
pub const BACKLOG_PER_CARRIER: u32 = 4;

/// How a client is linked to the storages that serve it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Link {
    /// Road network component of its entrance (docs/game_design.md §7).
    Road(u32),
    /// Walking components of its access tiles: wall segments, gates and towers on a wall line
    /// ([`crate::defs::Access::AnySide`]) need no road, carriers walk to any side (§5.5).
    Walk(Vec<u32>),
}

/// A storage that can take part in transports.
struct StorageSite {
    id: BuildingId,
    rect: TileRect,
    component: u32,
    /// Walking component of its entrance (only known while open-sided clients are planned).
    walk: Option<u32>,
    radius: i32,
    /// Waiting jobs it may have ([`BACKLOG_PER_CARRIER`] per carrier, at least that many).
    max_backlog: u32,
}

/// Buffer the goods of a job are taken from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SourceBuffer {
    /// A storage inventory (supply, construction material).
    Storage,
    /// A production output buffer (collection).
    Output,
    /// Surplus in a production input buffer (return).
    Input,
}

impl SourceBuffer {
    fn of(kind: TransportKind) -> Self {
        match kind {
            TransportKind::Collect | TransportKind::Haul => SourceBuffer::Output,
            TransportKind::Return => SourceBuffer::Input,
            TransportKind::Supply | TransportKind::Construction | TransportKind::Restock => SourceBuffer::Storage,
        }
    }
}

/// Claims of existing jobs, so goods and room are not promised twice.
#[derive(Default)]
struct Claims {
    /// Units announced for a building (waiting or on the way).
    incoming: BTreeMap<(BuildingId, ResourceId), u32>,
    /// Units of waiting jobs still at their source, per source buffer.
    outgoing: BTreeMap<(BuildingId, ResourceId, SourceBuffer), u32>,
}

impl Claims {
    fn incoming(&self, b: BuildingId, r: ResourceId) -> u32 {
        self.incoming.get(&(b, r)).copied().unwrap_or(0)
    }

    fn outgoing(&self, b: BuildingId, r: ResourceId, buffer: SourceBuffer) -> u32 {
        self.outgoing.get(&(b, r, buffer)).copied().unwrap_or(0)
    }

    fn add(&mut self, job: &TransportJob) {
        *self.incoming.entry((job.target, job.resource)).or_insert(0) += job.amount;
        if job.goods_at_source() {
            let key = (job.source, job.resource, SourceBuffer::of(job.kind));
            *self.outgoing.entry(key).or_insert(0) += job.amount;
        }
    }
}

struct Planner {
    storages: Vec<StorageSite>,
    claims: Claims,
    /// Waiting jobs per carrier base.
    backlog: BTreeMap<BuildingId, u32>,
    capacity: u32,
    tick: u64,
    new_jobs: Vec<TransportJob>,
    /// Heating value (thousandths) workshops may still get as fuel input in the heating
    /// season, beyond the households' reserve (docs/game_design.md §6.1 Heizen geht vor);
    /// `None` outside it.
    fuel_budget: Option<u64>,
    /// Heating value per unit and resource, in thousandths (0: no fuel).
    heating_pm: Vec<u64>,
}

impl Planner {
    /// `n` units of `resource` for a workshop, cut to the fuel budget (fuel only).
    fn fuel_limit(&self, resource: ResourceId, n: u32) -> u32 {
        let per_unit = self.heating_pm.get(resource.index()).copied().unwrap_or(0);
        match self.fuel_budget {
            Some(budget) if per_unit > 0 => n.min(u32::try_from(budget / per_unit).unwrap_or(u32::MAX)),
            _ => n,
        }
    }

    /// Takes `n` units of `resource` out of the fuel budget (fuel only).
    fn spend_fuel(&mut self, resource: ResourceId, n: u32) {
        let per_unit = self.heating_pm.get(resource.index()).copied().unwrap_or(0);
        if let Some(budget) = self.fuel_budget.as_mut() {
            *budget = budget.saturating_sub(per_unit * u64::from(n));
        }
    }

    /// True if `s` serves a building linked by `link` with footprint `rect` and can take
    /// another job.
    fn in_reach(&self, s: &StorageSite, link: &Link, rect: TileRect) -> bool {
        let linked = match link {
            Link::Road(component) => s.component == *component,
            Link::Walk(components) => s.walk.is_some_and(|w| components.contains(&w)),
        };
        linked
            && s.rect.distance_to_rect(&rect) <= s.radius
            && self.backlog.get(&s.id).copied().unwrap_or(0) < s.max_backlog
    }

    /// Nearest storage with unclaimed room for `resource`, and that room.
    fn storage_with_room(
        &self,
        state: &GameState,
        resource: ResourceId,
        link: &Link,
        rect: TileRect,
    ) -> Option<(BuildingId, u32)> {
        self.storages
            .iter()
            .filter(|s| self.in_reach(s, link, rect))
            .filter_map(|s| {
                let inv = &state.buildings.get(s.id)?.inventory;
                let room = inv.free_space(resource).saturating_sub(self.claims.incoming(s.id, resource));
                (room > 0).then_some((s.rect.distance_to_rect(&rect), s.id, room))
            })
            .min_by_key(|(d, id, _)| (*d, *id))
            .map(|(_, id, room)| (id, room))
    }

    /// Nearest storage with unclaimed units of `resource`, and those units.
    fn storage_with_goods(
        &self,
        state: &GameState,
        resource: ResourceId,
        link: &Link,
        rect: TileRect,
        except: Option<BuildingId>,
    ) -> Option<(BuildingId, u32)> {
        self.storages
            .iter()
            .filter(|s| Some(s.id) != except && self.in_reach(s, link, rect))
            .filter_map(|s| {
                let inv = &state.buildings.get(s.id)?.inventory;
                let units = inv.get(resource).saturating_sub(self.claims.outgoing(s.id, resource, SourceBuffer::Storage));
                (units > 0).then_some((s.rect.distance_to_rect(&rect), s.id, units))
            })
            .min_by_key(|(d, id, _)| (*d, *id))
            .map(|(_, id, units)| (id, units))
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        kind: TransportKind,
        resource: ResourceId,
        amount: u32,
        source: BuildingId,
        target: BuildingId,
        base: BuildingId,
        priority: JobPriority,
    ) {
        let job = TransportJob {
            id: 0,
            kind,
            resource,
            amount,
            source,
            target,
            carrier_base: base,
            priority,
            created_at: self.tick,
            stage: JobStage::Waiting,
        };
        self.claims.add(&job);
        *self.backlog.entry(base).or_insert(0) += 1;
        self.new_jobs.push(job);
    }

    /// Collection of output units and input surplus of a finished production building.
    fn plan_collection(&mut self, state: &GameState, id: BuildingId, link: &Link, rect: TileRect, priority: JobPriority) {
        let Some(p) = state.buildings.get(id).and_then(|b| b.production.as_ref()) else { return };
        let full = p.status() == ProductionStatus::OutputFull;
        let outputs: Vec<(ResourceId, u32, u32)> =
            p.output().iter().map(|m| (m.resource, m.amount, p.output().capacity(m.resource))).collect();
        let surplus: Vec<(ResourceId, u32)> =
            p.input().iter().map(|m| (m.resource, p.input().surplus(m.resource))).filter(|(_, s)| *s > 0).collect();
        for (resource, amount, capacity) in outputs {
            let collectible = amount.saturating_sub(self.claims.outgoing(id, resource, SourceBuffer::Output));
            let threshold = self.capacity.min((capacity / 2).max(1));
            if collectible == 0 || (collectible < threshold && !full) {
                continue;
            }
            if let Some((dest, room)) = self.storage_with_room(state, resource, link, rect) {
                let n = collectible.min(self.capacity).min(room);
                self.push(TransportKind::Collect, resource, n, id, dest, dest, priority);
            }
        }
        for (resource, surplus) in surplus {
            let left = surplus.saturating_sub(self.claims.outgoing(id, resource, SourceBuffer::Input));
            if left == 0 {
                continue;
            }
            if let Some((dest, room)) = self.storage_with_room(state, resource, link, rect) {
                let n = left.min(self.capacity).min(room);
                self.push(TransportKind::Return, resource, n, id, dest, dest, priority);
            }
        }
    }

    /// Supply of a working production building's input buffer (refilled once at most half full).
    fn plan_supply(&mut self, state: &GameState, id: BuildingId, link: &Link, rect: TileRect, priority: JobPriority) {
        let Some(p) = state.buildings.get(id).and_then(|b| b.production.as_ref()) else { return };
        let needs: Vec<(ResourceId, u32, u32)> =
            p.input().capacities().map(|(r, cap)| (r, cap, p.input().get(r))).collect();
        for (resource, capacity, amount) in needs {
            let have = amount + self.claims.incoming(id, resource);
            if have.saturating_mul(2) > capacity {
                continue;
            }
            let want = capacity - have;
            if let Some((src, units)) = self.storage_with_goods(state, resource, link, rect, None) {
                // Heating comes first: in the heating season fuel only beyond the reserve.
                let n = self.fuel_limit(resource, want.min(self.capacity).min(units));
                if n == 0 {
                    continue;
                }
                self.spend_fuel(resource, n);
                self.push(TransportKind::Supply, resource, n, src, id, src, priority);
            }
        }
    }

    /// Stock of a selling building (tavern: beer), refilled once at most half full.
    fn plan_shop(&mut self, state: &GameState, id: BuildingId, link: &Link, rect: TileRect, priority: JobPriority) {
        let Some(b) = state.buildings.get(id) else { return };
        let needs: Vec<(ResourceId, u32, u32)> = b.inventory().capacities().map(|(r, cap)| (r, cap, b.inventory().get(r))).collect();
        for (resource, capacity, amount) in needs {
            let have = amount + self.claims.incoming(id, resource);
            if have.saturating_mul(2) > capacity {
                continue;
            }
            if let Some((src, units)) = self.storage_with_goods(state, resource, link, rect, None) {
                let n = (capacity - have).min(self.capacity).min(units);
                self.push(TransportKind::Supply, resource, n, src, id, src, priority);
            }
        }
    }

    /// Material for a construction site or upgrade.
    fn plan_construction(&mut self, state: &GameState, id: BuildingId, link: &Link, rect: TileRect, priority: JobPriority) {
        let Some(site) = state.buildings.get(id).and_then(|b| b.construction_site()) else { return };
        let owed: Vec<(ResourceId, u32)> = site
            .materials_required()
            .iter()
            .map(|m| (m.resource, m.amount.saturating_sub(site.delivered(m.resource))))
            .collect();
        for (resource, owed) in owed {
            let mut missing = owed.saturating_sub(self.claims.incoming(id, resource));
            // An upgrading storage is not in the list; its own stock went in first.
            while missing > 0 {
                let Some((src, units)) = self.storage_with_goods(state, resource, link, rect, Some(id)) else {
                    break;
                };
                let n = missing.min(self.capacity).min(units);
                self.push(TransportKind::Construction, resource, n, src, id, src, priority);
                missing -= n;
            }
            if missing > 0 {
                self.plan_restock(state, resource, missing, link, rect, id, priority);
            }
        }
    }

    /// Material a site still needs that no storage in its reach holds: brought to the nearest
    /// storage that serves the site (and has room) from storages on its road network whose
    /// carriers reach it – an outpost warehouse at the border is filled from the town
    /// (`TransportKind::Restock`). What is already on its way there counts.
    #[allow(clippy::too_many_arguments)]
    fn plan_restock(
        &mut self,
        state: &GameState,
        resource: ResourceId,
        missing: u32,
        link: &Link,
        rect: TileRect,
        site: BuildingId,
        priority: JobPriority,
    ) {
        let serves = |s: &StorageSite| {
            let linked = match link {
                Link::Road(component) => s.component == *component,
                Link::Walk(components) => s.walk.is_some_and(|w| components.contains(&w)),
            };
            linked && s.id != site && s.rect.distance_to_rect(&rect) <= s.radius
        };
        let relay = self
            .storages
            .iter()
            .filter(|s| serves(s))
            .filter_map(|s| {
                let room = state.buildings.get(s.id)?.inventory.free_space(resource).saturating_sub(self.claims.incoming(s.id, resource));
                (room > 0).then_some((s.rect.distance_to_rect(&rect), s.id, s.rect, s.component, room))
            })
            .min_by_key(|(d, id, ..)| (*d, *id));
        let Some((_, relay, relay_rect, relay_component, room)) = relay else { return };
        let mut need = missing.saturating_sub(self.claims.incoming(relay, resource)).min(room);
        while need > 0 {
            let source = self
                .storages
                .iter()
                .filter(|s| {
                    s.id != relay
                        && s.id != site
                        && s.component == relay_component
                        && s.rect.distance_to_rect(&relay_rect) <= s.radius
                        && self.backlog.get(&s.id).copied().unwrap_or(0) < s.max_backlog
                })
                .filter_map(|s| {
                    let inv = &state.buildings.get(s.id)?.inventory;
                    let units = inv.get(resource).saturating_sub(self.claims.outgoing(s.id, resource, SourceBuffer::Storage));
                    (units > 0).then_some((s.rect.distance_to_rect(&relay_rect), s.id, units))
                })
                .min_by_key(|(d, id, _)| (*d, *id));
            let Some((_, src, units)) = source else { break };
            let n = need.min(self.capacity).min(units);
            self.push(TransportKind::Restock, resource, n, src, relay, src, priority);
            need -= n;
        }
    }
}

/// One planning pass: appends new waiting jobs to the queue.
pub(crate) fn plan(state: &mut GameState, defs: &Defs) {
    // Sites entered from any side are linked by walking: the components must be current.
    let open_sites = state.buildings.iter().any(|(id, b)| b.construction_site().is_some() && buildings::is_open_sided(state, defs, id));
    if open_sites {
        crate::navigation::prepare(state, defs);
    }
    let storages: Vec<StorageSite> = state
        .buildings
        .iter()
        // An upgrading storage has no carriers (builders hold the slots); it supplies only its
        // own site (`supply_own_upgrades`).
        .filter(|(_, b)| storage::is_operational_storage(defs, b) && !b.is_upgrading())
        .filter_map(|(id, b)| {
            let component = roads::building_component(&state.world, &state.roads, &state.buildings, id)?;
            let def = defs.buildings().get(b.kind().index())?;
            let radius = i32::from(def.storage.as_ref()?.carrier_radius);
            let max_backlog = BACKLOG_PER_CARRIER * u32::from(b.staffed()).max(1);
            let walk = if open_sites { state.navigation.component(b.entrance()) } else { None };
            Some(StorageSite { id, rect: b.rect(), component, walk, radius, max_backlog })
        })
        .collect();
    if storages.is_empty() {
        return;
    }
    let heating_pm: Vec<u64> =
        defs.resources().iter().map(|r| (f64::from(r.heating_value) * 1000.0).round().max(0.0) as u64).collect();
    // The fuel workshops may get: what the stock (less construction material) holds beyond the
    // households' heating reserve, less what is already on its way to workshops.
    let reserve = crate::population::supply::fuel_reserve_milli(state, defs);
    let fuel_budget = (reserve > 0).then(|| {
        let available: u64 = defs.resources().iter().map(|r| u64::from(state.economy.available(r.id)) * heating_pm[r.id.index()]).sum();
        let underway: u64 = state
            .logistics
            .jobs()
            .iter()
            .filter(|j| j.kind == TransportKind::Supply && j.goods_at_source())
            .map(|j| u64::from(j.amount) * heating_pm.get(j.resource.index()).copied().unwrap_or(0))
            .sum();
        available.saturating_sub(reserve).saturating_sub(underway)
    });
    let mut planner = Planner {
        storages,
        claims: Claims::default(),
        backlog: BTreeMap::new(),
        capacity: u32::from(defs.rules().carrier_capacity),
        tick: state.clock.tick,
        new_jobs: Vec::new(),
        fuel_budget,
        heating_pm,
    };
    for job in state.logistics.jobs() {
        planner.claims.add(job);
        if job.stage == JobStage::Waiting {
            *planner.backlog.entry(job.carrier_base).or_insert(0) += 1;
        }
    }
    let mut clients: Vec<(Reverse<JobPriority>, BuildingId)> =
        state.buildings.iter().map(|(id, b)| (Reverse(b.priority()), id)).collect();
    clients.sort();
    for (Reverse(priority), id) in clients {
        let Some(b) = state.buildings.get(id) else { continue };
        let link = if open_sites && buildings::is_open_sided(state, defs, id) {
            let components = buildings::access_components(state, defs, id);
            if components.is_empty() {
                continue;
            }
            Link::Walk(components)
        } else {
            let Some(component) = roads::building_component(&state.world, &state.roads, &state.buildings, id) else { continue };
            Link::Road(component)
        };
        let rect = b.rect();
        match b.phase() {
            BuildingPhase::Construction(_) | BuildingPhase::Upgrading(_) if !b.is_paused() => {
                planner.plan_construction(state, id, &link, rect, priority);
            }
            _ => {}
        }
        if b.is_complete() && b.production.is_some() {
            planner.plan_collection(state, id, &link, rect, priority);
            if !b.is_paused() {
                planner.plan_supply(state, id, &link, rect, priority);
            }
        } else if b.is_complete() && !b.is_paused() && defs.buildings().get(b.kind().index()).is_some_and(|d| !d.sells.is_empty()) {
            planner.plan_shop(state, id, &link, rect, priority);
        }
    }
    for job in planner.new_jobs {
        state.logistics.push(job);
    }
}
