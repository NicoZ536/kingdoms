//! Jobs: assigning citizens to workplaces and construction sites (docs/game_design.md §6).
//!
//! Adults of working age (children and elders do not work) are assigned to the job demands
//! of all buildings ([`Building::job_demand`](crate::buildings::Building::job_demand)): builders
//! for construction sites and upgrades that can progress (category `Construction`), workers for
//! active buildings (storage workers are the carriers of [`crate::logistics`]). Demands are
//! served in this order:
//! 1. **Base staffing** ([`is_supply_building`]): the first slot of every storage (its first
//!    carrier) and of every market, before everything else and independent of priorities —
//!    without carriers no goods reach the storages and without a market worker no household
//!    is supplied. Likewise the first construction site in allocation order (building
//!    priority, then id) gets one builder: a town whose adults all work could otherwise never
//!    build the house its newcomers need.
//! 2. All other slots by the player's category priority (high → normal → low), then by
//!    building priority, then by category order (food, raw materials, processing,
//!    construction, services, military), then by building id.
//! 3. Last, production buildings that have nothing to do ([`is_idle_status`]: no orders,
//!    nothing to gather, fields growing or out of season): they keep their workers only while
//!    nobody else needs them.
//!
//! Educated jobs (school, hospital, jeweler, artisan) take only educated citizens.
//!
//! The allocation runs in the construction step whenever something changed (buildings,
//! priorities, sites running out of material, a production building becoming idle or busy,
//! citizens coming of age, dying, arriving or leaving). It is **stable**: citizens keep their
//! job as long as their building still wants them; vacancies go to the unemployed citizen
//! living nearest, surplus workers living farthest away are released first. Closing (pausing)
//! or demolishing a workplace releases its workers, and so does retiring (a citizen who
//! becomes an elder loses the job at once). The contract other systems rely on:
//! [`Building::staffed`](crate::buildings::Building::staffed) equals the number of citizens
//! whose [`Employment`] names the building, and never exceeds its demand.

use std::cmp::Reverse;

use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::defs::{BuildingDef, Defs, JobCategory, ServiceKind, TechEffect};
use crate::economy::storage;
use crate::grid::TileRect;
use crate::policy::JobPriority;
use crate::population::{CitizenId, Employment, Population};
use crate::production::ProductionStatus;
use crate::state::GameState;

/// Population and employment counts (refreshed every tick) plus the pending-work flags of the
/// job allocation and the housing step.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workforce {
    population: u32,
    available: u32,
    employed: u32,
    dirty: bool,
    housing_dirty: bool,
}

impl Workforce {
    /// Adults of working age (able to take a job).
    pub fn available(&self) -> u32 {
        self.available
    }

    /// All citizens (children, adults and elders).
    pub fn population(&self) -> u32 {
        self.population
    }

    /// Citizens currently assigned to a job or construction site.
    pub fn employed(&self) -> u32 {
        self.employed
    }

    /// Adults of working age without assignment.
    pub fn unemployed(&self) -> u32 {
        self.available.saturating_sub(self.employed)
    }

    /// Requests a new job allocation and housing check in the next steps.
    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
        self.housing_dirty = true;
    }

    /// Requests a new job allocation only (houses and households unchanged).
    pub(crate) fn mark_jobs_dirty(&mut self) {
        self.dirty = true;
    }

    /// True if a job allocation is pending.
    pub(crate) fn needs_allocation(&self) -> bool {
        self.dirty
    }

    /// True if houses or households changed since the last housing step.
    pub(crate) fn needs_housing(&self) -> bool {
        self.housing_dirty
    }

    pub(crate) fn housing_done(&mut self) {
        self.housing_dirty = false;
    }

    /// Recounts population and employment from the citizens.
    pub(crate) fn refresh_counts(&mut self, population: &Population) {
        let mut available = 0;
        let mut employed = 0;
        for (_, c) in population.citizens() {
            if c.can_take_job() {
                available += 1;
            }
            if c.job().is_some() {
                employed += 1;
            }
        }
        self.population = u32::try_from(population.len()).unwrap_or(u32::MAX);
        self.available = available;
        self.employed = employed;
    }
}

/// One building's demand for workers.
struct Demand {
    building: BuildingId,
    category: JobCategory,
    rect: TileRect,
    educated_only: bool,
    /// Slots served before all other demands (base staffing of supply buildings).
    base: u8,
    target: u8,
}

/// True for buildings whose first worker is staffed before everything else: storages (their
/// workers are the carriers) and markets (they supply the households).
pub fn is_supply_building(def: &BuildingDef) -> bool {
    def.storage.is_some() || def.services.iter().any(|s| s.kind == ServiceKind::Market)
}

/// True for production states in which workers have nothing to do for a longer time: no
/// production order, nothing to gather or plant in the radius, fields growing or out of season. Such
/// buildings are staffed last. Short waits (missing input, full output) do not count.
pub fn is_idle_status(status: ProductionStatus) -> bool {
    matches!(
        status,
        ProductionStatus::Idle
            | ProductionStatus::NoResourcesInRange
            | ProductionStatus::NoPlantingSpace
            | ProductionStatus::Growing
            | ProductionStatus::OutOfSeason
    )
}

/// Where a citizen lives, for distances: the home or, for the homeless, `camp` (see
/// [`camp_rect`]).
fn origin_of(state: &GameState, camp: TileRect, citizen: CitizenId) -> TileRect {
    state
        .population
        .citizen(citizen)
        .and_then(|c| state.population.household(c.household()))
        .and_then(|h| h.home())
        .and_then(|b| state.buildings.get(b))
        .map_or(camp, |b| b.rect())
}

/// Where homeless households camp: the main headquarters (or the world's start point).
pub(crate) fn camp_rect(state: &GameState, defs: &Defs) -> TileRect {
    storage::main_headquarters(state, defs)
        .and_then(|hq| state.buildings.get(hq))
        .map_or_else(|| TileRect::tile(state.world.start), |b| b.rect())
}

/// Assigns citizens to all job demands (see the module documentation) and writes `staffed` of
/// every building.
///
/// Cost (step 7): the camp of the homeless is looked up once, demands are found by building
/// index instead of a map, nothing is collected for buildings without surplus, the search for
/// workers is skipped when no demand has a vacancy, and vacancies are filled from the
/// unemployed grouped by where they live (one distance per house and demand instead of one per
/// citizen and vacancy). The result is exactly that of the plain search (differential tests
/// against the previous implementation in `workforce_reference.rs`).
pub(crate) fn allocate(state: &mut GameState, defs: &Defs) {
    let camp = camp_rect(state, defs);
    let mut demands = collect_demands(state, defs);
    set_targets(state, &mut demands);
    release_surplus(state, camp, &demands);
    if demands.iter().any(|d| state.population.workers_of(d.building).len() < usize::from(d.target)) {
        fill_vacancies(state, camp, &demands);
    }
    for (id, b) in state.buildings.iter_mut() {
        let n = state.population.workers_of(id).len();
        b.set_staffed(u8::try_from(n).unwrap_or(u8::MAX));
    }
    state.workforce.dirty = false;
    state.workforce.refresh_counts(&state.population);
}

fn collect_demands(state: &GameState, defs: &Defs) -> Vec<Demand> {
    type Key = (bool, Reverse<JobPriority>, Reverse<JobPriority>, usize, BuildingId);
    let mut keyed: Vec<(Key, Demand)> = state
        .buildings
        .iter()
        .filter_map(|(id, b)| {
            let (category, slots) = b.job_demand(defs)?;
            let def = defs.buildings().get(b.kind().index());
            let construction = category == JobCategory::Construction;
            let educated_only = !construction && def.and_then(|d| d.job).is_some_and(|j| j.tier.requires_education());
            let base = u8::from(!construction && def.is_some_and(is_supply_building)).min(slots);
            let idle = !construction && b.production().is_some_and(|p| is_idle_status(p.status()));
            let key = (idle, Reverse(state.policy.job_priority(category)), Reverse(b.priority()), category.index(), id);
            Some((key, Demand { building: id, category, rect: b.rect(), educated_only, base, target: slots }))
        })
        .collect();
    keyed.sort_by_key(|(k, _)| *k);
    let mut demands: Vec<Demand> = keyed.into_iter().map(|(_, d)| d).collect();
    // One builder for the first construction site, before all priorities (base staffing).
    if let Some(site) = demands.iter_mut().find(|d| d.category == JobCategory::Construction) {
        site.base = site.target.min(1);
    }
    demands
}

/// Workers of working age not yet promised to a demand.
struct WorkerPool {
    educated: u32,
    plain: u32,
}

impl WorkerPool {
    /// Takes up to `slots` workers for a demand: educated jobs draw from the educated, other
    /// jobs from the uneducated first, then from the educated. Returns the number taken.
    fn take(&mut self, slots: u32, educated_only: bool) -> u32 {
        if educated_only {
            let give = slots.min(self.educated);
            self.educated -= give;
            give
        } else {
            let from_plain = slots.min(self.plain);
            self.plain -= from_plain;
            let from_educated = (slots - from_plain).min(self.educated);
            self.educated -= from_educated;
            from_plain + from_educated
        }
    }
}

/// Caps every demand's target by the workers left: first the base staffing of every demand,
/// then the remaining slots, both in demand order.
fn set_targets(state: &GameState, demands: &mut [Demand]) {
    let mut pool = WorkerPool { educated: 0, plain: 0 };
    for (_, c) in state.population.citizens() {
        if c.can_take_job() {
            if c.is_educated() {
                pool.educated += 1;
            } else {
                pool.plain += 1;
            }
        }
    }
    let given: Vec<u32> = demands.iter().map(|d| pool.take(u32::from(d.base.min(d.target)), d.educated_only)).collect();
    for (d, base) in demands.iter_mut().zip(given) {
        let rest = pool.take(u32::from(d.target) - base, d.educated_only);
        d.target = u8::try_from(base + rest).unwrap_or(u8::MAX);
    }
}

/// Ends jobs that are no longer wanted: workplace gone or without demand, other job category,
/// citizen no longer of working age or not qualified, and workers beyond the target (those
/// living farthest away first).
fn release_surplus(state: &mut GameState, camp: TileRect, demands: &[Demand]) {
    // Demand index by building arena index (`u32::MAX`: no demand).
    let slots = demands.iter().map(|d| d.building.index() as usize + 1).max().unwrap_or(0);
    let mut by_index = vec![u32::MAX; slots];
    for (i, d) in demands.iter().enumerate() {
        by_index[d.building.index() as usize] = u32::try_from(i).unwrap_or(u32::MAX);
    }
    let demand_of = |building: BuildingId| {
        by_index.get(building.index() as usize).and_then(|i| demands.get(*i as usize)).filter(|d| d.building == building)
    };
    let leaving: Vec<CitizenId> = state
        .population
        .citizens()
        .filter_map(|(id, c)| {
            let job = c.job()?;
            let keep = c.can_take_job()
                && demand_of(job.building)
                    .is_some_and(|d| d.category == job.category && (c.is_educated() || !d.educated_only) && d.target > 0);
            (!keep).then_some(id)
        })
        .collect();
    for id in leaving {
        state.population.set_job(id, None);
    }
    let mut ranked: Vec<(Reverse<i32>, Reverse<CitizenId>)> = Vec::new();
    for d in demands {
        let workers = state.population.workers_of(d.building);
        let surplus = workers.len().saturating_sub(usize::from(d.target));
        if surplus == 0 {
            continue;
        }
        ranked.clear();
        ranked.extend(workers.iter().map(|w| (Reverse(origin_of(state, camp, *w).distance_to_rect(&d.rect)), Reverse(*w))));
        // Keys are unique (citizen ids): the unstable sort is deterministic.
        ranked.sort_unstable();
        let released: Vec<CitizenId> = ranked.iter().take(surplus).map(|(_, Reverse(w))| *w).collect();
        for w in released {
            state.population.set_job(w, None);
        }
    }
}

/// Unemployed citizens able to work who live at the same place (one house, or the camp of the
/// homeless) and so have the same distance to every workplace: the uneducated members are
/// `Unemployed::ids[plain.0..plain.1]`, the educated `ids[educated.0..educated.1]`, both in
/// ascending id order; `next` counts how many of each are taken (always the first ones).
struct Group {
    origin: TileRect,
    plain: (usize, usize),
    educated: (usize, usize),
    next: [usize; 2],
}

/// The unemployed able to work, grouped by where they live (see [`Group`]).
struct Unemployed {
    ids: Vec<CitizenId>,
    groups: Vec<Group>,
}

impl Unemployed {
    /// Groups the unemployed by their house, or the camp of the homeless (`camp`).
    fn collect(state: &GameState, camp: TileRect) -> Unemployed {
        let mut groups: Vec<Group> = Vec::new();
        // Group index by the arena index of the house; the camp has its own group.
        let mut by_home: Vec<u32> = Vec::new();
        let mut camp_group: Option<usize> = None;
        let mut members: Vec<(u32, bool, CitizenId)> = Vec::new();
        for (id, c) in state.population.citizens() {
            if !c.can_take_job() || c.job().is_some() {
                continue;
            }
            let home = state
                .population
                .household(c.household())
                .and_then(|h| h.home())
                .and_then(|b| state.buildings.get(b).map(|bb| (b.index() as usize, bb.rect())));
            let gi = match home {
                Some((i, rect)) => {
                    if by_home.len() <= i {
                        by_home.resize(i + 1, u32::MAX);
                    }
                    if by_home[i] == u32::MAX {
                        groups.push(Group { origin: rect, plain: (0, 0), educated: (0, 0), next: [0, 0] });
                        by_home[i] = u32::try_from(groups.len() - 1).unwrap_or(u32::MAX);
                    }
                    by_home[i]
                }
                None => {
                    let gi = *camp_group.get_or_insert_with(|| {
                        groups.push(Group { origin: camp, plain: (0, 0), educated: (0, 0), next: [0, 0] });
                        groups.len() - 1
                    });
                    u32::try_from(gi).unwrap_or(u32::MAX)
                }
            };
            members.push((gi, c.is_educated(), id));
        }
        // Citizens come in id order and the sort is stable: ids stay ascending within a part.
        members.sort_by_key(|(gi, educated, _)| (*gi, *educated));
        let mut at = 0;
        for (gi, g) in groups.iter_mut().enumerate() {
            let count = |educated: bool, from: usize| {
                members[from..].iter().take_while(|(i, e, _)| *i as usize == gi && *e == educated).count()
            };
            let plain = count(false, at);
            g.plain = (at, at + plain);
            at += plain;
            let educated = count(true, at);
            g.educated = (at, at + educated);
            at += educated;
        }
        Unemployed { ids: members.into_iter().map(|(_, _, id)| id).collect(), groups }
    }

    /// Members of group `gi` of the kind not taken yet (ascending ids).
    fn left(&self, gi: usize, educated: bool) -> &[CitizenId] {
        let g = &self.groups[gi];
        let (range, taken) = if educated { (g.educated, g.next[1]) } else { (g.plain, g.next[0]) };
        &self.ids[range.0 + taken..range.1]
    }

    /// Takes the `k` candidates of one kind nearest to `rect` (by distance, then id – the order
    /// of repeatedly taking the nearest one) and appends them to `out`; `near` is scratch
    /// space.
    fn take_nearest(&mut self, rect: &TileRect, educated: bool, k: usize, near: &mut Vec<(i32, usize)>, out: &mut Vec<CitizenId>) {
        if k == 0 {
            return;
        }
        near.clear();
        for gi in 0..self.groups.len() {
            if !self.left(gi, educated).is_empty() {
                near.push((self.groups[gi].origin.distance_to_rect(rect), gi));
            }
        }
        // Every listed group holds at least one candidate, so the groups up to the k-th nearest
        // distance hold at least k (all groups at that distance count: ties go by id).
        if near.len() > k {
            near.select_nth_unstable(k - 1);
            let limit = near[k - 1].0;
            near.retain(|(d, _)| *d <= limit);
        }
        near.sort_unstable();
        let wanted = out.len() + k;
        let mut i = 0;
        while i < near.len() && out.len() < wanted {
            let level = near[i].0;
            let j = i + near[i..].iter().take_while(|(d, _)| *d == level).count();
            // Merge the groups at this distance by id.
            while out.len() < wanted {
                let best = near[i..j].iter().filter_map(|(_, gi)| self.left(*gi, educated).first().map(|id| (*id, *gi))).min();
                let Some((id, gi)) = best else { break };
                self.groups[gi].next[usize::from(educated)] += 1;
                out.push(id);
            }
            i = j;
        }
    }
}

fn fill_vacancies(state: &mut GameState, camp: TileRect, demands: &[Demand]) {
    let mut pool = Unemployed::collect(state, camp);
    let mut near = Vec::new();
    let mut picked = Vec::new();
    // Educated jobs first: they may take an educated worker from a plain job, whose slot is
    // refilled in the second pass.
    for d in demands.iter().filter(|d| d.educated_only) {
        let vacant = usize::from(d.target).saturating_sub(state.population.workers_of(d.building).len());
        if vacant == 0 {
            continue;
        }
        picked.clear();
        pool.take_nearest(&d.rect, true, vacant, &mut near, &mut picked);
        let job = Some(Employment { building: d.building, category: d.category });
        for id in &picked {
            state.population.set_job(*id, job);
        }
        for _ in picked.len()..vacant {
            let Some(w) = educated_in_plain_job(state, demands) else { break };
            state.population.set_job(w, job);
        }
    }
    for d in demands.iter().filter(|d| !d.educated_only) {
        let vacant = usize::from(d.target).saturating_sub(state.population.workers_of(d.building).len());
        if vacant == 0 {
            continue;
        }
        picked.clear();
        pool.take_nearest(&d.rect, false, vacant, &mut near, &mut picked);
        let rest = vacant - picked.len();
        pool.take_nearest(&d.rect, true, rest, &mut near, &mut picked);
        let job = Some(Employment { building: d.building, category: d.category });
        for id in &picked {
            state.population.set_job(*id, job);
        }
    }
}

/// An educated citizen working in a job that does not need education, taken from the lowest
/// priority demand first.
fn educated_in_plain_job(state: &GameState, demands: &[Demand]) -> Option<CitizenId> {
    demands.iter().rev().filter(|d| !d.educated_only).find_map(|d| {
        state.population.workers_of(d.building).iter().rev().copied().find(|w| {
            state.population.citizen(*w).is_some_and(|c| c.is_educated())
        })
    })
}

/// Residents of one house.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HouseOccupancy {
    /// The house.
    pub building: BuildingId,
    /// People living there.
    pub residents: u16,
    /// Capacity including research bonuses.
    pub capacity: u16,
}

/// Capacity of a house type including researched `HousingCapacityBonus`.
pub fn housing_capacity(state: &GameState, defs: &Defs, base: u16) -> u16 {
    let bonus: i32 = state
        .research
        .researched()
        .filter_map(|t| defs.techs().get(t.index()))
        .flat_map(|t| t.effects.iter())
        .map(|e| match e {
            TechEffect::HousingCapacityBonus { percent } => i32::from(*percent),
            _ => 0,
        })
        .sum();
    (i32::from(base) * (100 + bonus).max(0) / 100).clamp(0, i32::from(u16::MAX)) as u16
}

/// Residents and capacity of every finished house, in id order.
pub fn housing_occupancy(state: &GameState, defs: &Defs) -> Vec<HouseOccupancy> {
    state
        .population
        .homes()
        .map(|(id, home)| HouseOccupancy {
            building: id,
            residents: state.population.home_residents(home),
            capacity: crate::population::home_capacity(state, defs, id),
        })
        .collect()
}

/// Workers wanted and staffed in one job category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryStaffing {
    /// Category.
    pub category: JobCategory,
    /// Player priority of the category.
    pub priority: JobPriority,
    /// Slots wanted by buildings of the category.
    pub wanted: u32,
    /// Slots filled.
    pub staffed: u32,
}

/// Overview of population, housing and jobs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkforceSummary {
    /// Population.
    pub population: u32,
    /// People with a place in a house.
    pub housed: u32,
    /// People without a place.
    pub homeless: u32,
    /// Places in all finished houses.
    pub housing_capacity: u32,
    /// Adults of working age.
    pub working_age: u32,
    /// People with a job or construction assignment.
    pub employed: u32,
    /// Adults of working age without assignment.
    pub unemployed: u32,
    /// Unfilled job slots.
    pub vacancies: u32,
    /// Per job category, in category order.
    pub categories: Vec<CategoryStaffing>,
}

/// Current population, housing and job overview.
pub fn summary(state: &GameState, defs: &Defs) -> WorkforceSummary {
    let houses = housing_occupancy(state, defs);
    let housed: u32 = houses.iter().map(|h| u32::from(h.residents)).sum();
    let capacity: u32 = houses.iter().map(|h| u32::from(h.capacity)).sum();
    let mut categories: Vec<CategoryStaffing> = JobCategory::ALL
        .iter()
        .map(|c| CategoryStaffing { category: *c, priority: state.policy.job_priority(*c), wanted: 0, staffed: 0 })
        .collect();
    for (_, b) in state.buildings.iter() {
        if let Some((category, slots)) = b.job_demand(defs) {
            let row = &mut categories[category.index()];
            row.wanted += u32::from(slots);
            row.staffed += u32::from(b.staffed().min(slots));
        }
    }
    let wanted: u32 = categories.iter().map(|c| c.wanted).sum();
    let staffed: u32 = categories.iter().map(|c| c.staffed).sum();
    let population = u32::try_from(state.population.len()).unwrap_or(u32::MAX);
    WorkforceSummary {
        population,
        housed,
        homeless: population.saturating_sub(housed),
        housing_capacity: capacity,
        working_age: state.workforce.available(),
        employed: state.workforce.employed(),
        unemployed: state.workforce.unemployed(),
        vacancies: wanted.saturating_sub(staffed),
        categories,
    }
}

#[cfg(test)]
#[path = "workforce_reference.rs"]
mod reference;

#[cfg(test)]
mod tests {
    use glam::IVec2;
    use rand::{RngExt, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    use crate::defs::{Defs, JobCategory};
    use crate::policy::JobPriority;
    use crate::state::GameState;
    use crate::test_support::{defs, depot, exec, flat_state, run, set_adults, spawn};

    /// Runs the optimised and the reference allocation on copies of `state` and checks that
    /// both leave exactly the same state (every job, staff count and counter).
    /// Returns the number of citizens whose job changed.
    fn assert_same_allocation(state: &GameState, d: &Defs, context: &str) -> usize {
        let mut fast = state.clone();
        let mut slow = state.clone();
        super::allocate(&mut fast, d);
        super::reference::allocate(&mut slow, d);
        let jobs = |s: &GameState| s.population.citizens().map(|(id, c)| (id, c.job())).collect::<Vec<_>>();
        assert_eq!(jobs(&fast), jobs(&slow), "{context}: same jobs");
        let bytes = |s: &GameState| bincode::serde::encode_to_vec(s, bincode::config::standard()).expect("encode");
        assert!(bytes(&fast) == bytes(&slow), "{context}: same state");
        jobs(state).iter().zip(jobs(&fast)).filter(|(a, b)| **a != *b).count()
    }

    /// A random town for the differential test: houses, workshops, farms, storages, markets,
    /// a school and a hospital (educated jobs) on a flat map, `citizens` people (some homeless,
    /// some educated) and random priorities.
    fn random_town(seed: u64, rng: &mut ChaCha8Rng) -> GameState {
        let d = defs();
        const KINDS: [&str; 16] = [
            "farmhouse", "farmhouse", "citizen_house", "townhouse", "woodcutter", "sawmill", "bakery", "mill", "chicken_farm",
            "market", "storehouse", "school", "hospital", "well", "brickworks", "tavern",
        ];
        let mut state = flat_state(96);
        let wanted = 20 + rng.random_range(0..40);
        let mut placed = 0;
        for _ in 0..wanted * 6 {
            if placed >= wanted {
                break;
            }
            let key = KINDS[rng.random_range(0..KINDS.len())];
            let at = IVec2::new(rng.random_range(2..88), rng.random_range(2..88));
            let kind = d.building_id(key).expect("building type");
            if crate::buildings::spawn_finished(&mut state, d, kind, at, Default::default()).is_some() {
                placed += 1;
            }
        }
        let citizens = 40 + rng.random_range(0..(seed as usize % 4 + 1) * 120);
        crate::population::test_populate(&mut state, d, citizens);
        for (_, c) in state.population.citizens.iter_mut() {
            c.educated = rng.random_range(0..100) < 30;
        }
        state
    }

    /// One random change that can move jobs: a building paused or reopened, a category or
    /// building priority changed, citizens educated or not, someone dying, new adults, a
    /// building demolished or a new one built, a production status flipping to idle.
    fn random_change(state: &mut GameState, rng: &mut ChaCha8Rng) -> String {
        let d = defs();
        let ids: Vec<crate::buildings::BuildingId> = state.buildings.iter().map(|(id, _)| id).collect();
        let pick = |rng: &mut ChaCha8Rng| ids[rng.random_range(0..ids.len())];
        match rng.random_range(0..9) {
            0 => {
                let b = pick(rng);
                let paused = state.buildings.get(b).is_some_and(|x| x.paused);
                state.buildings.get_mut(b).expect("building").paused = !paused;
                format!("pause {b:?} {}", !paused)
            }
            1 => {
                let category = JobCategory::ALL[rng.random_range(0..JobCategory::ALL.len())];
                let priority = [JobPriority::Low, JobPriority::Normal, JobPriority::High][rng.random_range(0..3)];
                state.policy.set_job_priority(category, priority);
                format!("category {category:?} {priority:?}")
            }
            2 => {
                let b = pick(rng);
                let priority = [JobPriority::Low, JobPriority::Normal, JobPriority::High][rng.random_range(0..3)];
                state.buildings.get_mut(b).expect("building").priority = priority;
                format!("building {b:?} {priority:?}")
            }
            3 => {
                for (_, c) in state.population.citizens.iter_mut() {
                    if rng.random_range(0..10) == 0 {
                        c.educated = !c.educated;
                    }
                }
                "education".to_owned()
            }
            4 => {
                let all: Vec<_> = state.population.citizens().map(|(id, _)| id).collect();
                for _ in 0..rng.random_range(1..6) {
                    let id = all[rng.random_range(0..all.len())];
                    crate::population::remove_citizen(state, d, id, &mut Vec::new());
                }
                "deaths".to_owned()
            }
            5 => {
                let adults = state.population.citizens().filter(|(_, c)| c.can_work()).count();
                set_adults(state, adults + rng.random_range(1..12));
                "new adults".to_owned()
            }
            6 => {
                let b = pick(rng);
                if b == depot(state) {
                    return "keep the depot".to_owned();
                }
                crate::buildings::construction::demolish_building(state, d, b, &mut Vec::new()).ok();
                format!("demolish {b:?}")
            }
            7 => {
                let key = ["sawmill", "market", "school", "farmhouse", "woodcutter"][rng.random_range(0..5)];
                let at = IVec2::new(rng.random_range(2..88), rng.random_range(2..88));
                let kind = d.building_id(key).expect("building type");
                let built = crate::buildings::spawn_finished(state, d, kind, at, Default::default());
                format!("build {key} {built:?}")
            }
            _ => {
                let producing: Vec<_> = state.buildings.iter().filter(|(_, b)| b.production().is_some()).map(|(id, _)| id).collect();
                if producing.is_empty() {
                    return "no production".to_owned();
                }
                let b = producing[rng.random_range(0..producing.len())];
                let status = [crate::production::ProductionStatus::Idle, crate::production::ProductionStatus::Working][rng.random_range(0..2)];
                if let Some(p) = state.buildings.get_mut(b).and_then(|x| x.production.as_mut()) {
                    p.status = status;
                }
                format!("status {b:?} {status:?}")
            }
        }
    }

    #[test]
    fn the_optimised_allocation_matches_the_reference_over_random_changes() {
        let d = defs();
        let (mut checks, mut moving, mut moved) = (0, 0, 0);
        for seed in 0..24u64 {
            let mut rng = ChaCha8Rng::seed_from_u64(0xA110C ^ seed);
            let mut state = random_town(seed, &mut rng);
            moved += assert_same_allocation(&state, d, &format!("seed {seed}: first allocation"));
            super::allocate(&mut state, d);
            for step in 0..30 {
                let change = random_change(&mut state, &mut rng);
                let n = assert_same_allocation(&state, d, &format!("seed {seed} step {step}: {change}"));
                checks += 1;
                moving += usize::from(n > 0);
                moved += n;
                super::allocate(&mut state, d);
                if step % 10 == 9 {
                    // Let the town live a little: routines, production states, arrivals.
                    run(&mut state, 40);
                    assert_same_allocation(&state, d, &format!("seed {seed} step {step}: after ticks"));
                }
            }
            // Everybody looks for work anew (the costliest case).
            let employed: Vec<_> = state.population.citizens().filter(|(_, c)| c.job().is_some()).map(|(id, _)| id).collect();
            for id in employed {
                state.population.set_job(id, None);
            }
            moved += assert_same_allocation(&state, d, &format!("seed {seed}: all anew"));
            super::allocate(&mut state, d);
            state.validate_entities(d).expect("valid");
        }
        assert_eq!(checks, 24 * 30);
        eprintln!("{checks} changes, {moving} moved jobs, {moved} job changes in total");
        // The changes really moved people between jobs.
        assert!(moving > checks / 3 && moved > 2000, "{moving} of {checks} changes moved {moved} jobs");
    }

    #[test]
    fn allocation_follows_priorities_and_keeps_jobs() {
        let d = defs();
        let mut state = flat_state(64);
        let depot = depot(&state); // 3 workers, Services, storage
        let hall = spawn(&mut state, "town_hall", 2, 2); // 2 workers, Services, storage
        let mill = spawn(&mut state, "sawmill", 10, 2); // 2 workers, Processing
        set_adults(&mut state, 5);
        run(&mut state, 1);
        // Base staffing: one carrier per storage first; then category order: Processing (2) <
        // Services (4); services by id: depot before hall.
        assert_eq!(state.building(mill).unwrap().staffed(), 2);
        assert_eq!(state.building(depot).unwrap().staffed(), 2);
        assert_eq!(state.building(hall).unwrap().staffed(), 1);
        assert_eq!((state.workforce().employed(), state.workforce().unemployed()), (5, 0));
        let millers = state.population.workers_of(mill).to_vec();
        exec(&mut state, crate::command::Command::SetWorkforcePriority { category: JobCategory::Services, priority: JobPriority::High });
        assert_eq!(state.building(depot).unwrap().staffed(), 3);
        assert_eq!(state.building(hall).unwrap().staffed(), 2);
        assert_eq!(state.building(mill).unwrap().staffed(), 0, "lower priority loses its workers");
        // Back to normal: the sawmill is refilled, the storages keep their base carriers.
        exec(&mut state, crate::command::Command::SetWorkforcePriority { category: JobCategory::Services, priority: JobPriority::Normal });
        assert_eq!(state.building(mill).unwrap().staffed(), 2);
        assert_eq!(state.building(depot).unwrap().staffed(), 2);
        assert_eq!(state.building(hall).unwrap().staffed(), 1);
        assert_eq!(state.population.workers_of(mill).len(), millers.len());
        set_adults(&mut state, 20);
        run(&mut state, 1);
        assert_eq!(state.workforce().employed(), 7);
        assert_eq!(state.workforce().unemployed(), 13);
        state.validate_entities(d).unwrap();
    }

    #[test]
    fn storages_and_markets_get_a_base_worker_before_all_priorities() {
        let d = defs();
        let mut state = flat_state(64);
        let depot = depot(&state);
        let market = spawn(&mut state, "market", 2, 2); // 3 workers, Services
        let farms = [spawn(&mut state, "chicken_farm", 10, 2), spawn(&mut state, "chicken_farm", 14, 2)];
        exec(&mut state, crate::command::Command::SetWorkforcePriority { category: JobCategory::Food, priority: JobPriority::High });
        exec(&mut state, crate::command::Command::SetWorkforcePriority { category: JobCategory::Services, priority: JobPriority::Low });
        set_adults(&mut state, 3);
        run(&mut state, 1);
        // Even with food first and services last, the depot keeps a carrier and the market a
        // worker; the third adult goes to the first farm.
        assert_eq!(state.building(depot).unwrap().staffed(), 1);
        assert_eq!(state.building(market).unwrap().staffed(), 1);
        assert_eq!(state.building(farms[0]).unwrap().staffed(), 1);
        assert_eq!(state.building(farms[1]).unwrap().staffed(), 0);
        // A paused storage has no demand at all.
        exec(&mut state, crate::command::Command::SetBuildingPaused { building: depot, paused: true });
        assert_eq!(state.building(depot).unwrap().staffed(), 0);
        assert_eq!(state.building(farms[1]).unwrap().staffed(), 1);
        state.validate_entities(d).unwrap();
    }

    #[test]
    fn idle_production_is_staffed_last() {
        use crate::production::ProductionStatus;
        use crate::world::{TreeKind, Vegetation};
        let d = defs();
        let mut state = flat_state(64);
        let depot = depot(&state);
        let cutter = spawn(&mut state, "woodcutter", 10, 10); // Raw materials, no trees around
        let mill = spawn(&mut state, "sawmill", 30, 10); // Processing
        set_adults(&mut state, 3);
        run(&mut state, 1);
        // Before its first production step the woodcutter counts as busy (raw before processing).
        assert_eq!(state.building(cutter).unwrap().staffed(), 2);
        run(&mut state, 3);
        let status = |s: &crate::state::GameState, id| s.building(id).unwrap().production().unwrap().status();
        assert_eq!(status(&state, cutter), ProductionStatus::NoResourcesInRange);
        assert_eq!(state.building(depot).unwrap().staffed(), 1);
        assert_eq!(state.building(mill).unwrap().staffed(), 2, "the sawmill takes the idle woodcutters");
        assert_eq!(state.building(cutter).unwrap().staffed(), 0);
        // Without workers the woodcutter still reports the empty area (no flip-flopping).
        run(&mut state, 50);
        assert_eq!(status(&state, cutter), ProductionStatus::NoResourcesInRange);
        assert_eq!(state.building(mill).unwrap().staffed(), 2);
        // A grown tree in reach: the woodcutter has work again and is staffed by its category.
        state.world.tile_mut(glam::IVec2::new(14, 10)).vegetation = Vegetation::Tree { kind: TreeKind::Conifer, growth: 255 };
        run(&mut state, 3);
        assert_eq!(state.building(cutter).unwrap().staffed(), 2);
        assert_eq!(state.building(mill).unwrap().staffed(), 0);
        // The woodcutters walk over from the sawmill and start.
        crate::test_support::run_until(&mut state, crate::time::TICKS_PER_DAY, |s| {
            status(s, cutter) == ProductionStatus::Working
        });
        state.validate_entities(d).unwrap();
    }

    #[test]
    fn paused_buildings_release_their_workers() {
        let mut state = flat_state(32);
        let cutter = spawn(&mut state, "woodcutter", 0, 0);
        run(&mut state, 1);
        assert_eq!(state.building(cutter).unwrap().staffed(), 2);
        exec(&mut state, crate::command::Command::SetBuildingPaused { building: cutter, paused: true });
        assert_eq!(state.building(cutter).unwrap().staffed(), 0);
        assert!(state.population.workers_of(cutter).is_empty());
        assert!(state.population.citizens().all(|(_, c)| c.job().is_none_or(|j| j.building != cutter)));
    }

    #[test]
    fn educated_jobs_need_educated_citizens() {
        let d = defs();
        let mut state = flat_state(48);
        let school = spawn(&mut state, "school", 2, 2);
        for (_, c) in state.population.citizens.iter_mut() {
            c.educated = false;
        }
        state.workforce.mark_dirty();
        run(&mut state, 1);
        assert_eq!(state.building(school).unwrap().staffed(), 0);
        // One educated citizen working at the depot is moved to the school.
        let depot = depot(&state);
        let at_depot = state.population.workers_of(depot)[0];
        state.population.citizens.get_mut(at_depot).unwrap().educated = true;
        state.workforce.mark_dirty();
        run(&mut state, 1);
        assert_eq!(state.population.workers_of(school), &[at_depot]);
        assert_eq!(state.building(depot).unwrap().staffed(), 3, "the depot slot is refilled");
        state.validate_entities(d).unwrap();
    }
}
