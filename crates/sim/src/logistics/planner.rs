//! Creates transport jobs from the needs of production buildings and construction sites.
//!
//! Clients are served by building priority (high first), then by id. For every need the
//! nearest suitable storage is chosen by footprint distance (ties by id): it must be
//! operational, road-connected to the client, cover the client with its `carrier_radius` and
//! have the goods (supply) or room (collection) not yet claimed by other jobs.
//!
//! A storage takes new jobs only while fewer than [`BACKLOG_PER_CARRIER`] jobs per carrier
//! (at least that many) wait for its carriers; beyond that the next storage in reach is chosen
//! or the need waits for a later pass. The queue stays short and loads grow instead of piling
//! up as many small claims.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use super::{JobStage, TransportJob, TransportKind};
use crate::buildings::{BuildingId, BuildingPhase};
use crate::defs::{Defs, ResourceId};
use crate::economy::storage;
use crate::grid::TileRect;
use crate::policy::JobPriority;
use crate::production::ProductionStatus;
use crate::roads;
use crate::state::GameState;

/// Waiting jobs a storage may have per carrier before the planner looks elsewhere.
pub const BACKLOG_PER_CARRIER: u32 = 4;

/// A storage that can take part in transports.
struct StorageSite {
    id: BuildingId,
    rect: TileRect,
    component: u32,
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
            TransportKind::Supply | TransportKind::Construction => SourceBuffer::Storage,
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
}

impl Planner {
    /// True if `s` serves a building on network `component` with footprint `rect` and can
    /// take another job.
    fn in_reach(&self, s: &StorageSite, component: u32, rect: TileRect) -> bool {
        s.component == component
            && s.rect.distance_to_rect(&rect) <= s.radius
            && self.backlog.get(&s.id).copied().unwrap_or(0) < s.max_backlog
    }

    /// Nearest storage with unclaimed room for `resource`, and that room.
    fn storage_with_room(
        &self,
        state: &GameState,
        resource: ResourceId,
        component: u32,
        rect: TileRect,
    ) -> Option<(BuildingId, u32)> {
        self.storages
            .iter()
            .filter(|s| self.in_reach(s, component, rect))
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
        component: u32,
        rect: TileRect,
        except: Option<BuildingId>,
    ) -> Option<(BuildingId, u32)> {
        self.storages
            .iter()
            .filter(|s| Some(s.id) != except && self.in_reach(s, component, rect))
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
    fn plan_collection(&mut self, state: &GameState, id: BuildingId, component: u32, rect: TileRect, priority: JobPriority) {
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
            if let Some((dest, room)) = self.storage_with_room(state, resource, component, rect) {
                let n = collectible.min(self.capacity).min(room);
                self.push(TransportKind::Collect, resource, n, id, dest, dest, priority);
            }
        }
        for (resource, surplus) in surplus {
            let left = surplus.saturating_sub(self.claims.outgoing(id, resource, SourceBuffer::Input));
            if left == 0 {
                continue;
            }
            if let Some((dest, room)) = self.storage_with_room(state, resource, component, rect) {
                let n = left.min(self.capacity).min(room);
                self.push(TransportKind::Return, resource, n, id, dest, dest, priority);
            }
        }
    }

    /// Supply of a working production building's input buffer (refilled once at most half full).
    fn plan_supply(&mut self, state: &GameState, id: BuildingId, component: u32, rect: TileRect, priority: JobPriority) {
        let Some(p) = state.buildings.get(id).and_then(|b| b.production.as_ref()) else { return };
        let needs: Vec<(ResourceId, u32, u32)> =
            p.input().capacities().map(|(r, cap)| (r, cap, p.input().get(r))).collect();
        for (resource, capacity, amount) in needs {
            let have = amount + self.claims.incoming(id, resource);
            if have.saturating_mul(2) > capacity {
                continue;
            }
            let want = capacity - have;
            if let Some((src, units)) = self.storage_with_goods(state, resource, component, rect, None) {
                let n = want.min(self.capacity).min(units);
                self.push(TransportKind::Supply, resource, n, src, id, src, priority);
            }
        }
    }

    /// Stock of a selling building (tavern: beer), refilled once at most half full.
    fn plan_shop(&mut self, state: &GameState, id: BuildingId, component: u32, rect: TileRect, priority: JobPriority) {
        let Some(b) = state.buildings.get(id) else { return };
        let needs: Vec<(ResourceId, u32, u32)> = b.inventory().capacities().map(|(r, cap)| (r, cap, b.inventory().get(r))).collect();
        for (resource, capacity, amount) in needs {
            let have = amount + self.claims.incoming(id, resource);
            if have.saturating_mul(2) > capacity {
                continue;
            }
            if let Some((src, units)) = self.storage_with_goods(state, resource, component, rect, None) {
                let n = (capacity - have).min(self.capacity).min(units);
                self.push(TransportKind::Supply, resource, n, src, id, src, priority);
            }
        }
    }

    /// Material for a construction site or upgrade.
    fn plan_construction(&mut self, state: &GameState, id: BuildingId, component: u32, rect: TileRect, priority: JobPriority) {
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
                let Some((src, units)) = self.storage_with_goods(state, resource, component, rect, Some(id)) else {
                    break;
                };
                let n = missing.min(self.capacity).min(units);
                self.push(TransportKind::Construction, resource, n, src, id, src, priority);
                missing -= n;
            }
        }
    }
}

/// One planning pass: appends new waiting jobs to the queue.
pub(crate) fn plan(state: &mut GameState, defs: &Defs) {
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
            Some(StorageSite { id, rect: b.rect(), component, radius, max_backlog })
        })
        .collect();
    if storages.is_empty() {
        return;
    }
    let mut planner = Planner {
        storages,
        claims: Claims::default(),
        backlog: BTreeMap::new(),
        capacity: u32::from(defs.rules().carrier_capacity),
        tick: state.clock.tick,
        new_jobs: Vec::new(),
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
        let Some(component) = roads::building_component(&state.world, &state.roads, &state.buildings, id) else { continue };
        let rect = b.rect();
        match b.phase() {
            BuildingPhase::Construction(_) | BuildingPhase::Upgrading(_) if !b.is_paused() => {
                planner.plan_construction(state, id, component, rect, priority);
            }
            _ => {}
        }
        if b.is_complete() && b.production.is_some() {
            planner.plan_collection(state, id, component, rect, priority);
            if !b.is_paused() {
                planner.plan_supply(state, id, component, rect, priority);
            }
        } else if b.is_complete() && !b.is_paused() && defs.buildings().get(b.kind().index()).is_some_and(|d| !d.sells.is_empty()) {
            planner.plan_shop(state, id, component, rect, priority);
        }
    }
    for job in planner.new_jobs {
        state.logistics.push(job);
    }
}
