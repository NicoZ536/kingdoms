//! Carriers: the walking executor of the transport queue (docs/game_design.md §6/§7).
//!
//! The workers of an operational storage are its carriers. Every tick (pipeline step 6):
//! 1. **Trips** advance, in ascending job id:
//!    * *fetching*: the carrier walks to the source; once inside it loads for
//!      `carrier_handling_ticks`, then the goods leave the source (`pick_up`) and it walks to
//!      the target,
//!    * *carrying*: once inside the target it unloads for `carrier_handling_ticks` (hauls at
//!      once) and the goods arrive (`unload`); a rest the target cannot take, a target that
//!      vanished or stopped accepting, or a target without a way there send the carrier to the
//!      nearest storage with room it can reach, without one to its own storage. A carrier in a
//!      storage target whose rest fits nowhere else stacks it there beyond the capacity, so it
//!      never blocks its storage's other jobs while waiting for room; only a carrier that
//!      cannot reach any of these waits [`RETRY_TICKS`],
//!    * a finished carrier returns to its daily routine: back to its storage during working
//!      time, home otherwise.
//! 2. **Assignments**: waiting jobs, highest priority first, then oldest, go to a free carrier
//!    of their storage: at the storage during working time outside the lunch break
//!    ([`crate::population::is_at_work`]), without a trip, lowest id first.
//!
//! A fetching carrier who leaves its storage's service (new job, retirement) gives the job back
//! to the queue; a carrying one finishes the delivery first. Jobs whose source vanished, lost
//! its road connection or became empty, or whose source the carrier cannot reach, are dropped;
//! the planner creates new ones as needed.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use glam::IVec2;

use super::{JobStage, RETRY_TICKS, TransportJob, TransportKind, hand_over, pick_up, target_accepts, unload};
use crate::buildings::{self, BuildingId};
use crate::defs::{Defs, ResourceAmount};
use crate::economy::storage;
use crate::event::SimEvent;
use crate::population::movement::{self, Destination, Purpose, TripPhase, WalkerId};
use crate::population::{CarryTrip, CitizenId, routine};
use crate::roads;
use crate::state::GameState;

/// Carriers a storage provides now (its staffed workers while operational; none while an
/// upgrade runs, builders hold the slots then).
pub fn carriers_of(state: &GameState, defs: &Defs, base: BuildingId) -> u32 {
    state
        .buildings
        .get(base)
        .filter(|b| storage::is_operational_storage(defs, b))
        .map_or(0, |b| u32::from(b.trade_staff()))
}

/// Carriers of `base` waiting at the storage for a job right now.
pub fn idle_carriers(state: &GameState, defs: &Defs, base: BuildingId) -> u32 {
    free_carriers(state, defs, base).len() as u32
}

/// Free carriers of `base` in id order (see the module documentation).
fn free_carriers(state: &GameState, defs: &Defs, base: BuildingId) -> Vec<CitizenId> {
    // While an upgrade runs, the slots belong to the builders.
    if !state.buildings.get(base).is_some_and(|b| b.trade_staff() > 0 && storage::is_operational_storage(defs, b)) {
        return Vec::new();
    }
    state
        .population
        .workers_of(base)
        .iter()
        .copied()
        .filter(|id| {
            state.population.citizen(*id).is_some_and(|c| c.carry.is_none() && crate::population::is_at_work(state, defs, c))
        })
        .collect()
}

/// Advances the carriers' trips, then hands waiting jobs to free carriers.
pub(crate) fn run(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let active: Vec<u64> = state.logistics.jobs().iter().filter(|j| j.occupies_carrier()).map(|j| j.id).collect();
    for id in active {
        let Some(job) = state.logistics.job(id).copied() else { continue };
        match job.stage {
            JobStage::Fetching { carrier, assigned_at } => fetch(state, defs, job, carrier, assigned_at, events),
            JobStage::Carrying { carrier, assigned_at } => carry(state, defs, job, carrier, assigned_at, events),
            JobStage::Waiting => {}
        }
    }
    assign(state, defs);
}

/// Loading or unloading time at a building for a job of `kind`.
fn handling_ticks(defs: &Defs, kind: TransportKind) -> u64 {
    if kind == TransportKind::Haul { 0 } else { u64::from(defs.rules().carrier_handling_ticks) }
}

/// The carrier's trip state.
fn trip_of(state: &GameState, carrier: CitizenId) -> Option<CarryTrip> {
    state.population.citizen(carrier).and_then(|c| c.carry)
}

fn trip_mut(state: &mut GameState, carrier: CitizenId) -> Option<&mut CarryTrip> {
    state.population.citizens.get_mut(carrier).and_then(|c| c.carry.as_mut())
}

/// True if the carrier stands inside `building` (arrived, not walking).
fn is_inside(state: &GameState, carrier: CitizenId, building: BuildingId) -> bool {
    state
        .population
        .citizen(carrier)
        .is_some_and(|c| c.motion().inside() == Some(building) && c.motion().phase() == TripPhase::Still)
}

/// Sends the carrier to `building` (a no-op while it is already on the way there).
fn head_for(state: &mut GameState, carrier: CitizenId, building: BuildingId, purpose: Purpose) {
    movement::set_destination(state, WalkerId::Citizen(carrier), Some(Destination::Building { building, purpose }));
}

/// Ends the carrier's trip; it continues with its daily routine at once.
fn release(state: &mut GameState, defs: &Defs, carrier: CitizenId) {
    if let Some(c) = state.population.citizens.get_mut(carrier) {
        c.carry = None;
    }
    routine::update(state, defs, carrier, &mut routine::Venues::default());
}

/// True if the handling time at a building has passed; starts it on the first call.
fn handling_done(state: &mut GameState, carrier: CitizenId, ticks: u64) -> bool {
    let now = state.clock.tick;
    let Some(trip) = trip_mut(state, carrier) else { return false };
    let ready = *trip.ready_at.get_or_insert(now + ticks);
    now >= ready
}

fn storage_exists(state: &GameState, defs: &Defs, base: BuildingId) -> bool {
    state.buildings.get(base).is_some_and(|b| storage::is_operational_storage(defs, b))
}

/// True if goods can go between `a` and `b`: both entrances on the same road network, or – for
/// a building entered from any side (wall pieces) – a walking connection from the other one's
/// road-served entrance to one of its access tiles (docs/game_design.md §5.5, §7).
pub(crate) fn connected(state: &mut GameState, defs: &Defs, a: BuildingId, b: BuildingId) -> bool {
    let (open_a, open_b) = (buildings::is_open_sided(state, defs, a), buildings::is_open_sided(state, defs, b));
    if !open_a && !open_b {
        return roads::buildings_connected(&state.world, &state.roads, &state.buildings, a, b);
    }
    crate::navigation::prepare(state, defs);
    let components = |state: &GameState, id: BuildingId, open: bool| -> Vec<u32> {
        if open {
            buildings::access_components(state, defs, id)
        } else if roads::has_road_access(&state.world, &state.roads, &state.buildings, id) {
            state.buildings.get(id).and_then(|b| state.navigation.component(b.entrance())).into_iter().collect()
        } else {
            Vec::new()
        }
    };
    let of_b = components(state, b, open_b);
    components(state, a, open_a).iter().any(|c| of_b.contains(c))
}

/// A fetching carrier: walk to the source, load, pick the goods up.
fn fetch(
    state: &mut GameState,
    defs: &Defs,
    job: TransportJob,
    carrier: CitizenId,
    assigned_at: u64,
    events: &mut Vec<SimEvent>,
) {
    let Some(c) = state.population.citizen(carrier) else {
        // Removed citizens give their jobs back first (`abandon`); repair defensively.
        set_stage(state, job.id, JobStage::Waiting);
        return;
    };
    let in_service = c.job().is_some_and(|j| j.building == job.carrier_base);
    let no_path = matches!(c.motion().phase(), TripPhase::NoPath { .. });
    let valid = storage_exists(state, defs, job.carrier_base) && connected(state, defs, job.source, job.target);
    if !in_service || no_path || !valid {
        if in_service || !valid {
            state.logistics.remove(job.id);
        } else {
            // The carrier left the storage's service: another carrier takes over.
            set_stage(state, job.id, JobStage::Waiting);
        }
        release(state, defs, carrier);
        return;
    }
    if !is_inside(state, carrier, job.source) {
        head_for(state, carrier, job.source, Purpose::Pickup);
        return;
    }
    if !handling_done(state, carrier, handling_ticks(defs, job.kind)) {
        return;
    }
    let taken = pick_up(state, &job);
    if taken == 0 {
        state.logistics.remove(job.id);
        release(state, defs, carrier);
        return;
    }
    if let Some(j) = state.logistics.job_mut(job.id) {
        j.amount = taken;
        j.stage = JobStage::Carrying { carrier, assigned_at };
    }
    if let Some(trip) = trip_mut(state, carrier) {
        trip.load = Some(ResourceAmount { resource: job.resource, amount: taken });
        trip.ready_at = None;
    }
    head_for(state, carrier, job.target, Purpose::Delivery);
    events.push(SimEvent::TransportStarted {
        job: job.id,
        kind: job.kind,
        resource: job.resource,
        amount: taken,
        from: job.source,
        to: job.target,
        carrier,
    });
}

/// A carrier with goods: walk to the target and unload; take what the target cannot accept to
/// a storage with room.
fn carry(
    state: &mut GameState,
    defs: &Defs,
    job: TransportJob,
    carrier: CitizenId,
    assigned_at: u64,
    events: &mut Vec<SimEvent>,
) {
    let now = state.clock.tick;
    let Some(c) = state.population.citizen(carrier) else {
        log::error!("transport job {} lost its carrier and its goods", job.id);
        state.logistics.remove(job.id);
        return;
    };
    let no_path = matches!(c.motion().phase(), TripPhase::NoPath { .. });
    let inside_target = is_inside(state, carrier, job.target);
    if no_path || !target_accepts(state, defs, &job) {
        let retry_pending = trip_of(state, carrier).and_then(|t| t.ready_at).is_some_and(|t| now < t);
        if retry_pending && !inside_target {
            return;
        }
        if !redirect(state, defs, job.id, carrier)
            && let Some(trip) = trip_mut(state, carrier)
        {
            trip.ready_at = Some(now + RETRY_TICKS);
        }
        return;
    }
    if !inside_target {
        head_for(state, carrier, job.target, Purpose::Delivery);
        return;
    }
    if !handling_done(state, carrier, handling_ticks(defs, job.kind)) {
        return;
    }
    let mut accepted = unload(state, defs, &job, job.amount);
    if accepted < job.amount
        && job.kind.to_storage()
        && !has_room_elsewhere(state, defs, &job, carrier)
        && let Some(b) = state.buildings.get_mut(job.target)
    {
        // No storage it can reach has room: the carrier stacks the rest here beyond the
        // capacity instead of blocking this storage's other jobs while it waits.
        accepted += b.inventory.add_beyond_capacity(job.resource, job.amount - accepted);
        state.logistics.mark_stock_dirty();
    }
    if accepted > 0 {
        events.push(SimEvent::TransportCompleted { job: job.id, resource: job.resource, amount: accepted, target: job.target });
    }
    let rest = job.amount - accepted;
    if rest == 0 {
        state.logistics.remove(job.id);
        if job.kind != TransportKind::Haul {
            state.logistics.record_trip(now.saturating_sub(assigned_at));
        }
        if let Some(c) = state.population.citizens.get_mut(carrier) {
            c.carry = None;
        }
        if !take_return_load(state, defs, carrier, job.target) {
            release(state, defs, carrier);
        }
        return;
    }
    if let Some(j) = state.logistics.job_mut(job.id) {
        j.amount = rest;
    }
    if let Some(trip) = trip_mut(state, carrier) {
        trip.load = Some(ResourceAmount { resource: job.resource, amount: rest });
        trip.ready_at = None;
    }
    if !redirect(state, defs, job.id, carrier)
        && let Some(trip) = trip_mut(state, carrier)
    {
        // Nowhere to go: wait here and try again later.
        trip.ready_at = Some(now + RETRY_TICKS);
    }
}

fn set_stage(state: &mut GameState, id: u64, stage: JobStage) {
    if let Some(j) = state.logistics.job_mut(id) {
        j.stage = stage;
    }
}

/// Where a carrier stands for distances and reachability: the entrance of the building it is
/// in, or its tile.
fn carrier_tile(state: &GameState, carrier: CitizenId) -> Option<IVec2> {
    let c = state.population.citizen(carrier)?;
    Some(match c.motion().inside().and_then(|b| state.buildings.get(b)) {
        Some(b) => b.entrance(),
        None => c.motion().pos().tile(),
    })
}

/// Operational storages other than the target of `job` with unclaimed room for its goods, the
/// nearest to `from` first (ties by id), with their entrances.
fn storages_with_room(state: &GameState, defs: &Defs, job: &TransportJob, from: IVec2) -> Vec<(BuildingId, IVec2)> {
    let mut candidates: Vec<(i32, BuildingId, IVec2)> = state
        .buildings
        .iter()
        .filter(|(sid, b)| *sid != job.target && storage::is_operational_storage(defs, b))
        .filter(|(sid, b)| b.inventory.free_space(job.resource).saturating_sub(state.logistics.incoming(*sid, job.resource)) > 0)
        .map(|(sid, b)| (b.rect().distance_to(from), sid, b.entrance()))
        .collect();
    candidates.sort_by_key(|(d, sid, _)| (*d, *sid));
    candidates.into_iter().map(|(_, sid, entrance)| (sid, entrance)).collect()
}

/// True if the carrier of `job` can reach another storage with unclaimed room for its goods.
fn has_room_elsewhere(state: &mut GameState, defs: &Defs, job: &TransportJob, carrier: CitizenId) -> bool {
    let Some(from) = carrier_tile(state, carrier) else { return false };
    let candidates = storages_with_room(state, defs, job, from);
    crate::navigation::sync(state, defs);
    candidates.into_iter().any(|(_, entrance)| state.navigation.reachable(from, entrance))
}

/// Sends a carrying carrier with the goods of job `id` to the nearest operational storage
/// (other than the job's target) with unclaimed room that it can reach; if there is none, to
/// its own storage (if that takes the goods at all and is not the target), where it stacks the
/// load even beyond the capacity. The job becomes a [`TransportKind::Return`]. Returns false if
/// there is nowhere to go.
fn redirect(state: &mut GameState, defs: &Defs, id: u64, carrier: CitizenId) -> bool {
    let Some(job) = state.logistics.job(id).copied() else { return false };
    let Some(from) = carrier_tile(state, carrier) else { return false };
    let mut candidates = storages_with_room(state, defs, &job, from);
    let base = job.carrier_base;
    if base != job.target
        && !candidates.iter().any(|(sid, _)| *sid == base)
        && let Some(b) = state.buildings.get(base).filter(|b| storage::is_operational_storage(defs, b))
        && b.inventory.accepts(job.resource)
    {
        // Last resort, after every storage with room.
        candidates.push((base, b.entrance()));
    }
    crate::navigation::sync(state, defs);
    let Some(target) = candidates.into_iter().find(|(_, entrance)| state.navigation.reachable(from, *entrance)).map(|(s, _)| s)
    else {
        return false;
    };
    if let Some(j) = state.logistics.job_mut(id) {
        j.kind = TransportKind::Return;
        j.target = target;
    }
    if let Some(trip) = trip_mut(state, carrier) {
        trip.ready_at = None;
    }
    head_for(state, carrier, target, Purpose::Delivery);
    true
}

/// Farthest a carrier walks from where it unloaded to a return load (Chebyshev distance of the
/// footprints in tiles).
pub const RETURN_LOAD_REACH: i32 = 6;

/// A carrier who just unloaded at `here` (away from its storage) takes a waiting job of its
/// storage whose goods wait there or close by ([`RETURN_LOAD_REACH`]; a return load such as
/// bread back from the bakery that got flour) instead of walking back empty, if it is in its
/// working time outside the lunch break. The nearest source wins (then priority, age, id).
/// Returns true if it took one.
fn take_return_load(state: &mut GameState, defs: &Defs, carrier: CitizenId, here: BuildingId) -> bool {
    let Some(base) = state.population.citizen(carrier).and_then(|c| c.job()).map(|j| j.building) else { return false };
    if here == base
        || !storage_exists(state, defs, base)
        || !routine::building_work_window(state, defs, base).is_working(state.clock.tick_of_day())
    {
        return false;
    }
    let Some(here_rect) = state.buildings.get(here).map(|b| b.rect()) else { return false };
    let distance = |b: BuildingId| state.buildings.get(b).map(|b| b.rect().distance_to_rect(&here_rect));
    let next = state
        .logistics
        .waiting()
        .filter(|j| j.carrier_base == base)
        .filter_map(|j| distance(j.source).filter(|d| *d <= RETURN_LOAD_REACH).map(|d| (d, j)))
        .min_by_key(|(d, j)| (*d, Reverse(j.priority), j.created_at, j.id))
        .map(|(_, j)| *j);
    let Some(job) = next else { return false };
    if !connected(state, defs, job.source, job.target) {
        return false;
    }
    let now = state.clock.tick;
    merge_waiting(state, defs, job.id);
    set_stage(state, job.id, JobStage::Fetching { carrier, assigned_at: now });
    let at_source = job.source == here;
    if let Some(c) = state.population.citizens.get_mut(carrier) {
        let ready_at = at_source.then(|| now + handling_ticks(defs, job.kind));
        c.carry = Some(CarryTrip { job: job.id, load: None, ready_at });
    }
    if at_source {
        routine::refresh_activity(state, defs, carrier);
    } else {
        head_for(state, carrier, job.source, Purpose::Pickup);
    }
    true
}

/// Before a carrier sets off with job `id`, other waiting jobs for the same goods between the
/// same buildings join it up to the carrier's capacity (one trip instead of several small ones;
/// in id order, a partly fitting job keeps its rest).
fn merge_waiting(state: &mut GameState, defs: &Defs, id: u64) {
    let Some(job) = state.logistics.job(id).copied() else { return };
    let capacity = u32::from(defs.rules().carrier_capacity);
    let mut room = capacity.saturating_sub(job.amount);
    if room == 0 {
        return;
    }
    let same: Vec<(u64, u32)> = state
        .logistics
        .waiting()
        .filter(|j| {
            j.id != id
                && j.kind == job.kind
                && j.resource == job.resource
                && j.source == job.source
                && j.target == job.target
                && j.carrier_base == job.carrier_base
        })
        .map(|j| (j.id, j.amount))
        .collect();
    let mut added = 0;
    for (other, amount) in same {
        if room == 0 {
            break;
        }
        let take = amount.min(room);
        room -= take;
        added += take;
        if take == amount {
            state.logistics.remove(other);
        } else if let Some(j) = state.logistics.job_mut(other) {
            j.amount -= take;
        }
    }
    if let Some(j) = state.logistics.job_mut(id) {
        j.amount += added;
    }
}

/// Hands waiting jobs to free carriers of their storage (priority, then age, then id). Only
/// jobs of storages with a free carrier are looked at.
fn assign(state: &mut GameState, defs: &Defs) {
    if state.logistics.waiting().next().is_none() {
        return;
    }
    let mut free: BTreeMap<BuildingId, Vec<CitizenId>> = state
        .buildings
        .iter()
        .filter(|(_, b)| b.trade_staff() > 0 && storage::is_operational_storage(defs, b))
        .map(|(id, _)| (id, free_carriers(state, defs, id)))
        .filter(|(_, list)| !list.is_empty())
        .collect();
    if free.is_empty() {
        return;
    }
    let mut waiting: Vec<(Reverse<crate::policy::JobPriority>, u64, u64)> = state
        .logistics
        .waiting()
        .filter(|j| free.contains_key(&j.carrier_base))
        .map(|j| (Reverse(j.priority), j.created_at, j.id))
        .collect();
    waiting.sort();
    let now = state.clock.tick;
    for (_, _, id) in waiting {
        let Some(job) = state.logistics.job(id).copied() else { continue };
        let Some(list) = free.get_mut(&job.carrier_base).filter(|l| !l.is_empty()) else { continue };
        if !connected(state, defs, job.source, job.target) {
            state.logistics.remove(id);
            continue;
        }
        let carrier = list.remove(0);
        let at_source = is_inside(state, carrier, job.source);
        merge_waiting(state, defs, id);
        set_stage(state, id, JobStage::Fetching { carrier, assigned_at: now });
        if let Some(c) = state.population.citizens.get_mut(carrier) {
            let ready_at = at_source.then(|| now + handling_ticks(defs, job.kind));
            c.carry = Some(CarryTrip { job: id, load: None, ready_at });
        }
        if !at_source {
            head_for(state, carrier, job.source, Purpose::Pickup);
        }
    }
}

/// Drops waiting jobs whose storage stopped working or started an upgrade (its carriers pause)
/// or whose buildings vanished (run before every planning pass; the planner plans again, with
/// other storages if they reach).
pub(crate) fn drop_orphaned(state: &mut GameState, defs: &Defs) {
    let orphaned: Vec<u64> = state
        .logistics
        .waiting()
        .filter(|j| {
            !storage_exists(state, defs, j.carrier_base)
                || state.buildings.get(j.carrier_base).is_some_and(|b| b.is_upgrading())
                || state.buildings.get(j.source).is_none()
                || state.buildings.get(j.target).is_none()
        })
        .map(|j| j.id)
        .collect();
    for id in orphaned {
        state.logistics.remove(id);
    }
}

/// Starts a haul: gatherer `carrier` (standing at its work spot) carries `amount` units of the
/// cycle's yield to its building's output buffer, whose room the caller reserved. Returns
/// false if the citizen cannot take the load (it is already carrying).
pub(crate) fn start_haul(state: &mut GameState, building: BuildingId, carrier: CitizenId, load: ResourceAmount) -> bool {
    if trip_of(state, carrier).is_some() || load.amount == 0 {
        return false;
    }
    let Some(b) = state.buildings.get(building) else { return false };
    let now = state.clock.tick;
    let job = TransportJob {
        id: 0,
        kind: TransportKind::Haul,
        resource: load.resource,
        amount: load.amount,
        source: building,
        target: building,
        carrier_base: building,
        priority: b.priority(),
        created_at: now,
        stage: JobStage::Carrying { carrier, assigned_at: now },
    };
    let id = state.logistics.push_started(job);
    if let Some(c) = state.population.citizens.get_mut(carrier) {
        c.carry = Some(CarryTrip { job: id, load: Some(load), ready_at: None });
    }
    head_for(state, carrier, building, Purpose::Delivery);
    true
}

/// A citizen who dies or leaves the kingdom gives its transport up: a job not yet picked up
/// goes back to the queue, carried goods are handed over on the spot ([`hand_over`]).
pub(crate) fn abandon(state: &mut GameState, defs: &Defs, citizen: CitizenId, events: &mut Vec<SimEvent>) {
    let Some(trip) = state.population.citizens.get_mut(citizen).and_then(|c| c.carry.take()) else { return };
    let Some(job) = state.logistics.job(trip.job).copied() else { return };
    match job.stage {
        JobStage::Fetching { .. } => {
            if storage_exists(state, defs, job.carrier_base) {
                set_stage(state, job.id, JobStage::Waiting);
            } else {
                state.logistics.remove(job.id);
            }
        }
        JobStage::Carrying { .. } => {
            state.logistics.remove(job.id);
            hand_over(state, defs, &job, events);
        }
        JobStage::Waiting => {}
    }
}
