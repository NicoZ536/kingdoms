//! Logistics: transport jobs between buildings and the carriers that walk them
//! (docs/game_design.md §6/§7).
//!
//! ## Job queue / executor split
//! * [`planner`] looks at buffers, storages and construction sites every
//!   [`PLAN_INTERVAL_TICKS`] and appends [`TransportJob`]s to the queue in [`Logistics`]:
//!   output buffer → nearest storage with room ([`TransportKind::Collect`]), surplus inputs →
//!   storage ([`TransportKind::Return`]), storage → input buffer ([`TransportKind::Supply`])
//!   and storage → construction site ([`TransportKind::Construction`]); material a site needs
//!   that no storage in its reach holds is first brought to the nearest storage that reaches it
//!   from a storage that reaches that one ([`TransportKind::Restock`]).
//! * The **executor** is [`carriers`]: the workers of a storage are its carriers. A carrier who
//!   is at its storage during working time takes a waiting job of that storage, walks to the
//!   source, loads (`pick_up`: the goods leave the source only now), walks to the target and
//!   unloads (the goods arrive only now); then it walks back to its storage.
//! * Gatherers bring the yield of a work cycle back to their building as a
//!   [`TransportKind::Haul`] job that starts loaded (see `start_haul`).
//!
//! ## Goods in transit
//! Goods between pick-up and delivery live on the carrier ([`crate::population::CarryTrip`]);
//! the job only names them ([`JobStage::Carrying`], `amount` = the load). Nothing is lost or
//! duplicated:
//! * a target that vanished, no longer accepts the goods or cannot be reached: the carrier takes
//!   them to the nearest storage with room it can reach, else to its own storage (the job
//!   becomes a [`TransportKind::Return`]); a storage target that is full while no other
//!   storage has room takes the load beyond its capacity (the surplus is used up first). Only
//!   a carrier that can reach none of these waits and tries again every [`RETRY_TICKS`],
//! * a carrier who loses its job finishes the delivery first; a carrier who dies or emigrates
//!   hands the goods over on the spot (`abandon`: target, its storage, nearest storage with
//!   room; only what fits nowhere is lost and reported),
//! * a source that vanished before the pick-up, or a carrier without a way to it: the job is
//!   dropped (the planner plans again).
//!
//! ## Rules
//! * Goods are only exchanged between buildings whose entrances lie on the same road network,
//!   and only with storages whose `carrier_radius` covers the building's footprint.
//! * A job is created only for goods and room that are not already claimed by other jobs, so
//!   nothing is moved twice.

pub mod carriers;
pub mod planner;
pub mod routes;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

pub use carriers::{carriers_of, idle_carriers};
pub(crate) use carriers::{abandon, start_haul};
pub use routes::travel_ticks;

use crate::buildings::BuildingId;
use crate::defs::{Defs, ResourceAmount, ResourceId};
use crate::economy::storage;
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::policy::JobPriority;
use crate::population::CitizenId;
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;

/// Ticks between two planning passes (6 game minutes).
pub const PLAN_INTERVAL_TICKS: u64 = 20;

/// Ticks a carrier with goods and no storage with room waits before it tries again (1 hour).
pub const RETRY_TICKS: u64 = TICKS_PER_HOUR;

/// Completed trips kept for the average trip time.
pub const TRIP_HISTORY: usize = 64;

/// What a transport job does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TransportKind {
    /// Production output buffer → storage.
    Collect,
    /// Input buffer surplus (inputs of a recipe no longer selected) → storage; also goods a
    /// carrier takes to a storage because their target cannot take them.
    Return,
    /// Storage → production input buffer.
    Supply,
    /// Storage → construction site or upgrade.
    Construction,
    /// A gatherer brings the yield of a work cycle from the work spot to its building's
    /// output buffer.
    Haul,
    /// Storage → storage: building material for a site that only the target storage reaches
    /// (an outpost warehouse at the border), brought from a storage that reaches the target.
    Restock,
}

impl TransportKind {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            TransportKind::Collect => "Abholung",
            TransportKind::Return => "Rücktransport",
            TransportKind::Supply => "Belieferung",
            TransportKind::Construction => "Baumaterial",
            TransportKind::Haul => "Ertrag",
            TransportKind::Restock => "Nachschub",
        }
    }

    /// True if the target is a storage.
    pub const fn to_storage(self) -> bool {
        matches!(self, TransportKind::Collect | TransportKind::Return | TransportKind::Restock)
    }

    /// True if the goods are taken from a storage's shelves.
    pub const fn from_storage(self) -> bool {
        matches!(self, TransportKind::Supply | TransportKind::Construction | TransportKind::Restock)
    }
}

/// Progress of a job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStage {
    /// Waiting for a free carrier; the goods are still at the source.
    Waiting,
    /// A carrier walks to the source (the goods are still there).
    Fetching {
        /// The carrier.
        carrier: CitizenId,
        /// Tick the carrier took the job.
        assigned_at: u64,
    },
    /// The goods are on the carrier, on the way to the target.
    Carrying {
        /// The carrier.
        carrier: CitizenId,
        /// Tick the carrier took the job.
        assigned_at: u64,
    },
}

/// A transport order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportJob {
    /// Unique, increasing id.
    pub id: u64,
    /// Kind of transport.
    pub kind: TransportKind,
    /// Transported resource.
    pub resource: ResourceId,
    /// Units: planned while waiting or fetching, the carrier's load while carrying.
    pub amount: u32,
    /// Building the goods come from.
    pub source: BuildingId,
    /// Building the goods go to.
    pub target: BuildingId,
    /// Storage whose carriers do the job (hauls: the gatherer's building).
    pub carrier_base: BuildingId,
    /// Priority of the client building (high first).
    pub priority: JobPriority,
    /// Tick the job was created.
    pub created_at: u64,
    /// Progress.
    pub stage: JobStage,
}

impl TransportJob {
    /// True while the goods are on the way (on a carrier).
    pub fn is_delivering(&self) -> bool {
        matches!(self.stage, JobStage::Carrying { .. })
    }

    /// True while the job occupies a carrier.
    pub fn occupies_carrier(&self) -> bool {
        !matches!(self.stage, JobStage::Waiting)
    }

    /// The carrier doing the job.
    pub fn carrier(&self) -> Option<CitizenId> {
        match self.stage {
            JobStage::Waiting => None,
            JobStage::Fetching { carrier, .. } | JobStage::Carrying { carrier, .. } => Some(carrier),
        }
    }

    /// Tick a carrier took the job.
    pub fn assigned_at(&self) -> Option<u64> {
        match self.stage {
            JobStage::Waiting => None,
            JobStage::Fetching { assigned_at, .. } | JobStage::Carrying { assigned_at, .. } => Some(assigned_at),
        }
    }

    /// True if the goods have not been picked up yet (waiting or fetching).
    pub fn goods_at_source(&self) -> bool {
        !self.is_delivering()
    }
}

/// The transport job queue and trip statistics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Logistics {
    jobs: Vec<TransportJob>,
    next_id: u64,
    /// Duration in ticks (taking the job → delivery) of the last [`TRIP_HISTORY`] completed
    /// carrier trips (hauls not included).
    recent_trips: VecDeque<u32>,
    /// Completed carrier trips since the game started.
    trips_completed: u64,
    /// Storages reported for lacking carriers: once per spell (docs/game_design.md §2
    /// Meldungen), the tick they were last seen lacking.
    pub(crate) carrier_notices: crate::notices::Spells<BuildingId>,
    /// A storage's contents changed in this step: the stock view is refreshed once at its end
    /// (derived, never set between ticks).
    #[serde(skip)]
    stock_dirty: bool,
}

impl Logistics {
    /// All jobs in ascending id order.
    pub fn jobs(&self) -> &[TransportJob] {
        &self.jobs
    }

    /// A job by id.
    pub fn job(&self, id: u64) -> Option<&TransportJob> {
        self.index_of(id).map(|i| &self.jobs[i])
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.jobs.binary_search_by_key(&id, |j| j.id).ok()
    }

    /// Jobs waiting for a carrier.
    pub fn waiting(&self) -> impl Iterator<Item = &TransportJob> + '_ {
        self.jobs.iter().filter(|j| j.stage == JobStage::Waiting)
    }

    /// Units of `resource` on the way to `target` (picked up, not yet delivered).
    pub fn in_transit_to(&self, target: BuildingId, resource: ResourceId) -> u32 {
        self.jobs
            .iter()
            .filter(|j| j.target == target && j.resource == resource && j.is_delivering())
            .map(|j| j.amount)
            .sum()
    }

    /// Units of `resource` announced for `target` (waiting, fetched or on the way).
    pub fn incoming(&self, target: BuildingId, resource: ResourceId) -> u32 {
        self.jobs.iter().filter(|j| j.target == target && j.resource == resource).map(|j| j.amount).sum()
    }

    /// Carriers of `base` currently on a trip.
    pub fn busy_carriers(&self, base: BuildingId) -> u32 {
        self.jobs
            .iter()
            .filter(|j| j.carrier_base == base && j.kind != TransportKind::Haul && j.occupies_carrier())
            .count() as u32
    }

    /// Goods on carriers right now, per resource in id order.
    pub fn goods_in_transit(&self) -> Vec<ResourceAmount> {
        let mut out: Vec<ResourceAmount> = Vec::new();
        for j in self.jobs.iter().filter(|j| j.is_delivering()) {
            match out.binary_search_by_key(&j.resource, |m| m.resource) {
                Ok(i) => out[i].amount += j.amount,
                Err(i) => out.insert(i, ResourceAmount { resource: j.resource, amount: j.amount }),
            }
        }
        out
    }

    /// Average duration of the recent carrier trips in ticks (`None` before the first one).
    pub fn average_trip_ticks(&self) -> Option<u64> {
        let n = self.recent_trips.len() as u64;
        (n > 0).then(|| self.recent_trips.iter().map(|t| u64::from(*t)).sum::<u64>() / n)
    }

    /// Completed carrier trips since the game started.
    pub fn trips_completed(&self) -> u64 {
        self.trips_completed
    }

    /// Adds a waiting job and returns its id.
    pub(crate) fn push(&mut self, mut job: TransportJob) -> u64 {
        job.id = self.next_id;
        self.next_id += 1;
        job.stage = JobStage::Waiting;
        self.jobs.push(job);
        job.id
    }

    /// Adds a job in the stage it was given (hauls start loaded) and returns its id.
    pub(crate) fn push_started(&mut self, mut job: TransportJob) -> u64 {
        job.id = self.next_id;
        self.next_id += 1;
        self.jobs.push(job);
        job.id
    }

    pub(crate) fn job_mut(&mut self, id: u64) -> Option<&mut TransportJob> {
        self.index_of(id).map(|i| &mut self.jobs[i])
    }

    pub(crate) fn remove(&mut self, id: u64) -> Option<TransportJob> {
        let index = self.index_of(id)?;
        Some(self.jobs.remove(index))
    }

    /// Marks the stock view for a refresh at the end of the logistics step.
    pub(crate) fn mark_stock_dirty(&mut self) {
        self.stock_dirty = true;
    }

    /// Records a completed carrier trip of `ticks`.
    pub(crate) fn record_trip(&mut self, ticks: u64) {
        if self.recent_trips.len() >= TRIP_HISTORY {
            self.recent_trips.pop_front();
        }
        self.recent_trips.push_back(u32::try_from(ticks).unwrap_or(u32::MAX));
        self.trips_completed = self.trips_completed.saturating_add(1);
    }
}

/// Consistency of the queue with the carriers (loaded states): ids in order, valid resources
/// and amounts, every assigned job's carrier exists and names the job back, carried goods equal
/// the job's amount (and only carrying jobs have goods on a carrier), hauls are always loaded.
pub(crate) fn validate(state: &GameState, defs: &Defs) -> Result<(), String> {
    let l = &state.logistics;
    if !l.jobs.windows(2).all(|w| w[0].id < w[1].id) || l.jobs.last().is_some_and(|j| j.id >= l.next_id) {
        return Err("transport job ids out of order".into());
    }
    if l.recent_trips.len() > TRIP_HISTORY {
        return Err("trip history too long".into());
    }
    l.carrier_notices.validate(state.clock.tick, "carrier notices")?;
    let mut carriers = BTreeSet::new();
    for j in &l.jobs {
        if j.resource.index() >= defs.resources().len() || j.amount == 0 {
            return Err(format!("transport job {} is malformed", j.id));
        }
        if j.kind == TransportKind::Haul && !j.is_delivering() {
            return Err(format!("haul {} is not loaded", j.id));
        }
        let Some(carrier) = j.carrier() else { continue };
        if !carriers.insert(carrier) {
            return Err(format!("carrier {carrier:?} has two jobs"));
        }
        if j.assigned_at().is_some_and(|t| t > state.clock.tick) {
            return Err(format!("transport job {} was taken in the future", j.id));
        }
        let trip = state.population.citizen(carrier).and_then(|c| c.carry());
        let Some(trip) = trip.filter(|t| t.job() == j.id) else {
            return Err(format!("carrier of transport job {} does not carry it", j.id));
        };
        let expected = j.is_delivering().then_some(ResourceAmount { resource: j.resource, amount: j.amount });
        if trip.load() != expected {
            return Err(format!("load of the carrier of transport job {} does not match", j.id));
        }
    }
    for (id, c) in state.population.citizens() {
        if let Some(trip) = c.carry()
            && l.job(trip.job()).and_then(TransportJob::carrier) != Some(id)
        {
            return Err(format!("citizen {id:?} carries for a missing job"));
        }
    }
    Ok(())
}

/// True if `b` sells goods to households (tavern) and keeps a stock of them.
fn sells(defs: &Defs, b: &crate::buildings::Building) -> bool {
    defs.buildings().get(b.kind().index()).is_some_and(|d| !d.sells.is_empty())
}

/// Takes the goods of a job from its source (the carrier finished loading there) and returns
/// the units picked up (0 if nothing is left; the caller drops the job then). A storage source
/// marks the stock view for a refresh at the end of the logistics step.
pub(crate) fn pick_up(state: &mut GameState, job: &TransportJob) -> u32 {
    let Some(b) = state.buildings.get_mut(job.source) else { return 0 };
    let taken = match job.kind {
        TransportKind::Collect => b.production.as_mut().map_or(0, |p| p.output.remove(job.resource, job.amount)),
        TransportKind::Return => b.production.as_mut().map_or(0, |p| {
            let surplus = p.input.surplus(job.resource);
            p.input.remove(job.resource, job.amount.min(surplus))
        }),
        TransportKind::Supply | TransportKind::Construction | TransportKind::Restock => b.inventory.remove(job.resource, job.amount),
        // Hauls start loaded.
        TransportKind::Haul => 0,
    };
    if taken > 0 && job.kind.from_storage() {
        state.logistics.stock_dirty = true;
    }
    taken
}

/// True if the target of `job` still takes goods of its kind (exists; a storage is
/// operational, a production building finished, a construction site still a site).
pub(crate) fn target_accepts(state: &GameState, defs: &Defs, job: &TransportJob) -> bool {
    let Some(b) = state.buildings.get(job.target) else { return false };
    match job.kind {
        TransportKind::Collect | TransportKind::Return | TransportKind::Restock => storage::is_operational_storage(defs, b),
        TransportKind::Supply => b.is_complete() && (b.production.is_some() || sells(defs, b)),
        TransportKind::Construction => b.construction_site().is_some(),
        TransportKind::Haul => b.production.is_some(),
    }
}

/// Units of the job's goods its target could take right now (storage or buffer room, material
/// the site still needs); 0 if it no longer accepts them.
pub fn target_room(state: &GameState, defs: &Defs, job: &TransportJob) -> u32 {
    if !target_accepts(state, defs, job) {
        return 0;
    }
    let Some(b) = state.buildings.get(job.target) else { return 0 };
    match job.kind {
        TransportKind::Collect | TransportKind::Return | TransportKind::Restock => b.inventory.free_space(job.resource),
        TransportKind::Supply => match b.production.as_ref() {
            Some(p) => p.input().free_space(job.resource),
            None => b.inventory.free_space(job.resource),
        },
        TransportKind::Haul => b.production.as_ref().map_or(0, |p| p.output().free_space(job.resource)),
        TransportKind::Construction => b.construction_site().map_or(0, |site| site.outstanding(job.resource)),
    }
}

/// Puts up to `amount` units of the job's goods into its target only and returns the units
/// it accepted (storage room, buffer room, material the site still needs). Marks the stock
/// view for a refresh at the end of the logistics step.
pub(crate) fn unload(state: &mut GameState, defs: &Defs, job: &TransportJob, amount: u32) -> u32 {
    if !target_accepts(state, defs, job) {
        return 0;
    }
    let mut site_became_workable = false;
    let Some(b) = state.buildings.get_mut(job.target) else { return 0 };
    let accepted = match job.kind {
        TransportKind::Collect | TransportKind::Return | TransportKind::Restock => b.inventory.add(job.resource, amount),
        TransportKind::Supply => match b.production.as_mut() {
            Some(p) => p.input.add(job.resource, amount),
            // A selling building's stock (tavern).
            None => b.inventory.add(job.resource, amount),
        },
        TransportKind::Haul => b.production.as_mut().map_or(0, |p| p.output.add(job.resource, amount)),
        TransportKind::Construction => match b.construction_site_mut() {
            Some(site) => {
                let was_workable = site.can_progress();
                let n = site.deliver(job.resource, amount);
                site_became_workable = !was_workable && site.can_progress();
                n
            }
            None => 0,
        },
    };
    if job.kind == TransportKind::Construction && accepted > 0 {
        state.economy.record_consumed(job.resource, accepted);
    }
    if site_became_workable {
        state.workforce.mark_dirty();
    }
    if accepted > 0 {
        state.logistics.stock_dirty = true;
    }
    accepted
}

/// Hands the goods of `job` over on the spot (a carrier died or left with them): into the
/// target, else the carrier's storage, else the nearest storage with room; only what fits
/// nowhere is lost (reported). Returns the units the target accepted.
pub(crate) fn hand_over(state: &mut GameState, defs: &Defs, job: &TransportJob, events: &mut Vec<SimEvent>) -> u32 {
    let accepted = unload(state, defs, job, job.amount);
    let mut rest = job.amount - accepted;
    if rest > 0
        && let Some(base) = state.buildings.get_mut(job.carrier_base)
        && storage::is_operational_storage(defs, base)
    {
        rest -= base.inventory.add(job.resource, rest);
    }
    let at = state
        .buildings
        .get(job.target)
        .or_else(|| state.buildings.get(job.carrier_base))
        .map_or(state.world.start, |b| b.entrance());
    if rest > 0 {
        rest -= storage::deposit(state, defs, job.resource, rest, at);
    }
    if rest > 0 {
        storage::report_loss(defs, job.resource, rest, at, events);
    }
    storage::refresh_stock(state, defs);
    state.logistics.stock_dirty = false;
    events.push(SimEvent::TransportCompleted { job: job.id, resource: job.resource, amount: accepted, target: job.target });
    accepted
}

/// Pipeline step 6: plans new jobs every [`PLAN_INTERVAL_TICKS`], advances the carriers' trips
/// and hands waiting jobs to free carriers; warns about storages without carriers every
/// [`NO_CARRIER_CHECK_TICKS`].
pub(crate) fn run_logistics(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    if state.clock.tick % PLAN_INTERVAL_TICKS == 0 {
        carriers::drop_orphaned(state, defs);
        supply_own_upgrades(state, defs);
        planner::plan(state, defs);
    }
    carriers::run(state, defs, events);
    if std::mem::take(&mut state.logistics.stock_dirty) {
        // Once for all pick-ups and deliveries of the tick.
        storage::refresh_stock(state, defs);
    }
    if state.clock.tick > 0 && state.clock.tick % NO_CARRIER_CHECK_TICKS == 0 {
        warn_missing_carriers(state, defs, events);
    }
}

/// An upgrading storage builds with what it holds: its carriers pause while builders hold
/// the slots, so before every planning pass the material its own site still needs (less what
/// carriers already bring) moves straight from its shelves into the site. Goods promised to
/// jobs still waiting at the storage stay. Carriers of other storages bring only the rest.
pub(crate) fn supply_own_upgrades(state: &mut GameState, defs: &Defs) {
    let sites: Vec<BuildingId> = state
        .buildings
        .iter()
        .filter(|(_, b)| b.is_upgrading() && !b.is_paused())
        .filter(|(_, b)| defs.buildings().get(b.kind().index()).is_some_and(|d| d.storage.is_some()))
        .map(|(id, _)| id)
        .collect();
    for id in sites {
        let Some(site) = state.buildings.get(id).and_then(|b| b.construction_site()) else { continue };
        let owed: Vec<(ResourceId, u32)> = site
            .materials_required()
            .iter()
            .map(|m| (m.resource, site.outstanding(m.resource).saturating_sub(state.logistics.incoming(id, m.resource))))
            .filter(|(_, n)| *n > 0)
            .collect();
        for (resource, owed) in owed {
            let promised: u32 = state
                .logistics
                .jobs()
                .iter()
                .filter(|j| j.source == id && j.resource == resource && j.goods_at_source())
                .filter(|j| matches!(j.kind, TransportKind::Supply | TransportKind::Construction))
                .map(|j| j.amount)
                .sum();
            let Some(b) = state.buildings.get_mut(id) else { break };
            let n = owed.min(b.inventory.get(resource).saturating_sub(promised));
            if n == 0 {
                continue;
            }
            let taken = b.inventory.remove(resource, n);
            let Some(site) = b.construction_site_mut() else { break };
            let was_workable = site.can_progress();
            let accepted = site.deliver(resource, taken);
            let became_workable = !was_workable && site.can_progress();
            if accepted < taken {
                // Unreachable (at most the outstanding amount is taken); keep the rest on the shelf.
                b.inventory.add(resource, taken - accepted);
            }
            state.economy.record_consumed(resource, accepted);
            state.logistics.stock_dirty = true;
            if became_workable {
                state.workforce.mark_dirty();
            }
        }
    }
}

/// Ticks between two checks for storages without carriers (6 game hours).
pub const NO_CARRIER_CHECK_TICKS: u64 = 6 * TICKS_PER_HOUR;

/// Storages with transport jobs waiting for at least an hour while they have no carrier, in
/// id order.
pub fn storages_without_carriers(state: &GameState, defs: &Defs) -> Vec<BuildingId> {
    let now = state.clock.tick;
    let waiting: BTreeSet<BuildingId> = state
        .logistics
        .waiting()
        .filter(|j| now.saturating_sub(j.created_at) >= TICKS_PER_HOUR)
        .map(|j| j.carrier_base)
        .collect();
    waiting.into_iter().filter(|b| carriers_of(state, defs, *b) == 0).collect()
}

/// One warning naming the first storage whose spell without carriers begins (see
/// [`storages_without_carriers`]; once per spell and storage, docs/game_design.md §2
/// Meldungen), counting the others that begin with it.
fn warn_missing_carriers(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let now = state.clock.tick;
    let gap = defs.rules().notices.resolved_ticks();
    let lacking = storages_without_carriers(state, defs);
    let spells = &mut state.logistics.carrier_notices;
    let new: Vec<BuildingId> = lacking.into_iter().filter(|id| spells.begins(*id, now, gap)).collect();
    let buildings = &state.buildings;
    spells.prune(now, gap, |id| buildings.get(id).is_some());
    let Some((id, b)) = new.first().and_then(|id| state.buildings.get(*id).map(|b| (*id, b))) else { return };
    let name = defs.buildings().get(b.kind().index()).map_or("Ein Lager", |d| d.name.as_str());
    let text = match new.len() {
        1 => format!("Keine Träger: {name} hat keine Arbeiter, Transporte bleiben liegen."),
        n => format!("Keine Träger: {name} und {} weitere Lager haben keine Arbeiter, Transporte bleiben liegen.", n - 1),
    };
    events.push(SimEvent::Notification(
        Notification::new(Severity::Warning, text).with_kind(NotificationKind::NoCarriers).at(b.entrance()).for_building(id),
    ));
}
