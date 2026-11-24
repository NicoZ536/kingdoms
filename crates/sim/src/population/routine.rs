//! The daily routine (docs/game_design.md §6) as destinations, and the activity that follows
//! from where a citizen actually is.
//!
//! | Time (shipped data) | Who | Destination |
//! |---|---|---|
//! | 22:00–06:00 | everybody | home (homeless: the camp at the main headquarters) |
//! | 08:00–17:00 | employed adults | workplace or construction site (work starts on arrival, lunch 12:00–13:00 at the workplace; farmers +1 h in summer, −1 h in winter) |
//! | 08:00–13:00 | children from `school_age` with a school in reach | school |
//! | 18:00–19:00 (also on festival days), or whenever the pantry holds no food (on cold days no fuel) | the first adult of a housed household | market in reach, else the storage covering the house (errands: the pantry is restocked on arrival) |
//! | 19:00–21:00 | every other evening, adults and elders | tavern or church in reach (alternating) |
//! | otherwise | everybody | home |
//!
//! The day changes with what happens (docs/game_design.md §6):
//! * **Festival days** (event modifier): from the end of the lunch break until 21:00 everybody
//!   celebrates at the church in reach, else the market, instead of working or going home.
//! * **Sick** citizens (health below `sick_health`) go to a hospital in reach during the day and
//!   stay at home otherwise; they neither work nor go to school or out (the household's errand
//!   is run by a healthy member if there is one).
//! * **Enemies:** while hostile fighters are on the map, a civilian within the military
//!   `alert_radius` of one, or heading for a place within it, shelters at home, or in the main
//!   headquarters if the home lies within the radius itself (else it stays at home).
//!
//! A worker counts as **present** ([`is_at_work`]) only while at its place of work during its
//! working time outside the lunch break ([`WorkArea`], step 3b): inside the workplace, or
//! outside on a work spot — builders on the tiles around their construction site, farmers on
//! their farm's fields, gatherers at the spot of the running cycle (tree, deposit, planting
//! spot, shore). Production, farm work and construction use present workers only, at the
//! shift factor (`RoutineHours::shift_factor_permille`): recipe and build times are given for
//! round-the-clock operation, so a full shift without a commute yields the same daily output;
//! every minute on the way costs output.
//!
//! A citizen on a transport ([`crate::logistics`]: carriers, gatherers bringing their yield
//! back) follows the transport before anything else, also after working time.

use glam::IVec2;

use crate::buildings::BuildingId;
use crate::defs::{BuildingDef, Defs, Gathering, ServiceKind};
use crate::economy::storage;
use crate::grid::TileRect;
use crate::logistics::JobStage;
use crate::production::ProductionStatus;
use crate::state::GameState;
use crate::time::{Season, TICKS_PER_HOUR};

use super::movement::{self, Destination, Purpose, TripPhase, WalkerId};
use super::{Activity, Citizen, CitizenId, Household, LifeStage};

/// Working time of a workplace in ticks of the day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkWindow {
    /// Workers leave home (work starts on arrival).
    pub start: u64,
    /// Lunch break starts.
    pub lunch_start: u64,
    /// Lunch break ends.
    pub lunch_end: u64,
    /// End of work.
    pub end: u64,
}

impl WorkWindow {
    /// True during the working time (from `start` to `end`, lunch included).
    pub fn contains(&self, tick_of_day: u64) -> bool {
        (self.start..self.end).contains(&tick_of_day)
    }

    /// True during the lunch break.
    pub fn is_lunch(&self, tick_of_day: u64) -> bool {
        (self.lunch_start..self.lunch_end).contains(&tick_of_day)
    }

    /// True while present workers work.
    pub fn is_working(&self, tick_of_day: u64) -> bool {
        self.contains(tick_of_day) && !self.is_lunch(tick_of_day)
    }
}

fn at(hour: u8) -> u64 {
    u64::from(hour) * TICKS_PER_HOUR
}

/// Working time at a building of type `def` in `season` (farmers work longer in summer and
/// shorter in winter, §6).
pub fn work_window(defs: &Defs, season: Season, def: Option<&BuildingDef>) -> WorkWindow {
    let r = &defs.rules().walking.routine;
    let farm = def.is_some_and(|d| crate::agriculture::farm_spec(d).is_some());
    let end = match season {
        Season::Summer if farm => r.work_end_hour.saturating_add(r.farmer_summer_extra_hours),
        Season::Winter if farm => r.work_end_hour.saturating_sub(r.farmer_winter_fewer_hours),
        _ => r.work_end_hour,
    };
    WorkWindow { start: at(r.work_start_hour), lunch_start: at(r.lunch_start_hour), lunch_end: at(r.lunch_end_hour), end: at(end) }
}

/// Working time of building `building` today.
pub fn building_work_window(state: &GameState, defs: &Defs, building: BuildingId) -> WorkWindow {
    let def = state.buildings.get(building).and_then(|b| defs.buildings().get(b.kind().index()));
    work_window(defs, state.clock.season(), def)
}

/// Where the workers of a building work (step 3b).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkArea {
    /// Inside the building (workshops, storages, services, mines, pastures).
    Inside,
    /// Builders: the tiles around the footprint of a construction site or upgrade.
    Around(TileRect),
    /// A field farm's claimed tiles (inside the farm while it has no task).
    Fields,
    /// A gatherer's running cycle: the walkable tile at its target.
    Spot(IVec2),
}

/// Where the workers of `building` work right now.
pub fn work_area(state: &GameState, defs: &Defs, building: BuildingId) -> WorkArea {
    let Some(b) = state.buildings.get(building) else { return WorkArea::Inside };
    if b.construction_site().is_some() {
        return WorkArea::Around(b.rect());
    }
    let Some(p) = b.production.as_ref() else { return WorkArea::Inside };
    let gathering = defs.buildings().get(b.kind().index()).and_then(|d| d.production.as_ref()).and_then(|p| p.gathering.as_ref());
    if matches!(gathering, Some(Gathering::Fields { .. })) {
        return WorkArea::Fields;
    }
    match p.cycle().and_then(|c| c.spot) {
        Some(spot) => WorkArea::Spot(spot),
        None => WorkArea::Inside,
    }
}

/// True if a citizen standing (or inside a building) at `motion` is at a place of `area` of
/// `building`.
fn in_area(state: &GameState, area: WorkArea, building: BuildingId, citizen: &Citizen) -> bool {
    let m = &citizen.motion;
    let tile = m.pos.tile();
    match area {
        WorkArea::Inside => m.inside == Some(building),
        WorkArea::Around(rect) => m.inside.is_none() && rect.expanded(1).contains(tile) && !rect.contains(tile),
        // Farmers work on the fields near the task; while the farm has no task they wait
        // inside.
        WorkArea::Fields => match m.inside {
            None => {
                state.agriculture.owner_of(tile) == Some(building)
                    && state
                        .agriculture
                        .farm(building)
                        .and_then(|f| f.task())
                        .is_some_and(|task| (tile - task.tile).abs().max_element() <= FARMER_REACH)
            }
            Some(b) => b == building && state.agriculture.farm(building).is_none_or(|f| f.task().is_none()),
        },
        WorkArea::Spot(spot) => m.inside.is_none() && tile == spot,
    }
}

/// True if `citizen` works right now: at its place of work ([`work_area`]) during its working
/// time, outside the lunch break, not walking and not on a transport.
pub fn is_at_work(state: &GameState, defs: &Defs, citizen: &Citizen) -> bool {
    let Some(job) = citizen.job else { return false };
    citizen.carry.is_none()
        && !in_treatment_trip(citizen)
        && citizen.motion.phase == TripPhase::Still
        && building_work_window(state, defs, job.building).is_working(state.clock.tick_of_day())
        && in_area(state, work_area(state, defs, job.building), job.building, citizen)
}

/// True if the citizen is on the way to (or being treated in) a hospital: a sick hospital
/// worker treated in his own workplace does not work there (§6).
fn in_treatment_trip(citizen: &Citizen) -> bool {
    matches!(citizen.motion.dest, Some(Destination::Building { purpose: Purpose::Treatment, .. }))
}

/// Workers of gatherer `building` ready to start a cycle: during working time outside the lunch
/// break, not walking, not on a transport, inside the building or outside within `reach` tiles
/// of its footprint (still at the last cycle's spot).
pub fn on_duty_workers(state: &GameState, defs: &Defs, building: BuildingId, reach: i32) -> u8 {
    let Some(rect) = state.buildings.get(building).map(|b| b.rect()) else { return 0 };
    if !building_work_window(state, defs, building).is_working(state.clock.tick_of_day()) {
        return 0;
    }
    let n = state
        .population
        .workers_of(building)
        .iter()
        .filter_map(|c| state.population.citizen(*c))
        .filter(|c| c.carry.is_none() && !in_treatment_trip(c) && c.motion.phase == TripPhase::Still)
        .filter(|c| match c.motion.inside {
            Some(b) => b == building,
            None => rect.distance_to(c.motion.pos.tile()) <= reach,
        })
        .count();
    u8::try_from(n).unwrap_or(u8::MAX)
}

/// Workers of `building` present right now (see [`is_at_work`]).
pub fn present_workers(state: &GameState, defs: &Defs, building: BuildingId) -> u8 {
    let n = state
        .population
        .workers_of(building)
        .iter()
        .filter_map(|c| state.population.citizen(*c))
        .filter(|c| is_at_work(state, defs, c))
        .count();
    u8::try_from(n).unwrap_or(u8::MAX)
}

/// Status of a building whose workers are assigned but not present: lunch break, on the way
/// (working time) or after work.
pub fn absence_status(state: &GameState, defs: &Defs, building: BuildingId) -> ProductionStatus {
    let window = building_work_window(state, defs, building);
    let t = state.clock.tick_of_day();
    if window.is_lunch(t) {
        ProductionStatus::LunchBreak
    } else if window.contains(t) {
        ProductionStatus::WorkersAway
    } else {
        ProductionStatus::OffShift
    }
}

/// A building providing a service a citizen may visit.
#[derive(Clone, Copy, Debug)]
struct Venue {
    id: BuildingId,
    rect: TileRect,
    radius: i32,
}

/// Visitable service buildings (operational, staffed if they have jobs), collected once per
/// tick on first use.
#[derive(Clone, Debug, Default)]
pub(crate) struct Venues {
    collected: bool,
    markets: Vec<Venue>,
    schools: Vec<Venue>,
    taverns: Vec<Venue>,
    churches: Vec<Venue>,
    hospitals: Vec<Venue>,
    /// Tiles of hostile fighters on the map (collected on first use).
    danger: Option<Vec<IVec2>>,
}

impl Venues {
    fn collect(&mut self, state: &GameState, defs: &Defs) {
        if self.collected {
            return;
        }
        self.collected = true;
        for (id, b) in state.buildings.iter() {
            if !b.is_operational() {
                continue;
            }
            let Some(def) = defs.buildings().get(b.kind().index()) else { continue };
            if def.workers > 0 && b.trade_staff() == 0 {
                continue;
            }
            let bonus = state.research.service_radius_bonus(defs, def.id);
            for s in &def.services {
                let venue = Venue { id, rect: b.rect(), radius: i32::from(s.radius.saturating_add(bonus)) };
                match s.kind {
                    ServiceKind::Market => self.markets.push(venue),
                    ServiceKind::Education => self.schools.push(venue),
                    ServiceKind::Entertainment => self.taverns.push(venue),
                    ServiceKind::Religion => self.churches.push(venue),
                    ServiceKind::Health => self.hospitals.push(venue),
                    _ => {}
                }
            }
        }
    }
}

impl Venues {
    /// Venues with the given enemy tiles (tests).
    #[cfg(test)]
    pub(crate) fn with_danger(tiles: Vec<IVec2>) -> Self {
        Self { danger: Some(tiles), ..Self::default() }
    }

    /// Tiles of the hostile fighters on the map (empty without enemies).
    fn danger(&mut self, state: &GameState) -> &[IVec2] {
        self.danger.get_or_insert_with(|| {
            if !state.military.enemies_on_map() {
                return Vec::new();
            }
            state
                .military
                .warbands()
                .iter()
                .filter(|w| w.stage().is_hostile())
                .flat_map(|w| w.fighters().iter())
                .filter_map(|f| state.population.walking.travelers.get(f.walker()))
                .map(|t| t.motion.pos().tile())
                .collect()
        })
    }
}

/// True if `rect` lies within `radius` of a tile in `danger`.
fn endangered(danger: &[IVec2], rect: &TileRect, radius: i32) -> bool {
    danger.iter().any(|p| TileRect::tile(*p).distance_to_rect(rect) <= radius)
}

/// True if citizen `c` is sick (health below `sick_health`).
pub fn is_sick(defs: &Defs, c: &Citizen) -> bool {
    c.health < u16::from(defs.rules().population.sick_health) * super::CENTI
}

/// True if citizen `c` is cared for in a hospital right now.
pub(crate) fn in_treatment(state: &GameState, defs: &Defs, c: &Citizen) -> bool {
    matches!(c.motion.dest, Some(Destination::Building { building, purpose: Purpose::Treatment }) if c.motion.inside == Some(building))
        && state
            .buildings
            .get(c.motion.inside.unwrap_or(BuildingId::from_raw_parts(u32::MAX, 0)))
            .and_then(|b| defs.buildings().get(b.kind().index()))
            .is_some_and(|d| d.service_radius(ServiceKind::Health).is_some())
}

/// The nearest venue whose radius reaches `home` (ties: lower id).
fn nearest(list: &[Venue], home: &TileRect) -> Option<BuildingId> {
    list.iter()
        .filter(|v| v.rect.distance_to_rect(home) <= v.radius)
        .min_by_key(|v| (v.rect.distance_to_rect(home), v.id))
        .map(|v| v.id)
}

/// Home of a household: its house, or the camp at the main headquarters.
fn home_of(state: &GameState, defs: &Defs, household: &Household) -> Option<(BuildingId, Purpose)> {
    match household.home {
        Some(home) => Some((home, Purpose::Home)),
        None => storage::main_headquarters(state, defs).map(|hq| (hq, Purpose::Camp)),
    }
}

/// Where a citizen on a transport goes: the source to pick the goods up, then the target
/// (`None` while the target is gone and the carrier waits for a storage with room).
fn carry_destination(state: &GameState, job: u64) -> Option<Option<Destination>> {
    let job = state.logistics.job(job)?;
    Some(match job.stage {
        JobStage::Fetching { .. } => Some(Destination::Building { building: job.source, purpose: Purpose::Pickup }),
        JobStage::Carrying { .. } => state
            .buildings
            .get(job.target)
            .map(|_| Destination::Building { building: job.target, purpose: Purpose::Delivery }),
        JobStage::Waiting => return None,
    })
}

/// True if a worker can stand on `p` to work for a building whose entrance lies in navigation
/// component `home`: walkable, reachable from the entrance (same component) and no bridge (a
/// bridge may be removed under the worker's feet). Without a component for the entrance
/// (blocked), any walkable tile that is no bridge.
fn workable_spot(state: &GameState, home: Option<u32>, p: IVec2) -> bool {
    let nav = &state.navigation;
    nav.is_walkable(p)
        && home.is_none_or(|h| nav.component(p) == Some(h))
        && !state.roads.get(p).is_some_and(|t| t.is_bridge())
}

/// Tiles around `rect` (distance 1, the corners included) a builder can stand on and reach
/// from the site's access ([`workable_spot`] in navigation component `home`), nearest to
/// `entrance` first (ties by row, column).
fn builder_spots(state: &GameState, rect: TileRect, entrance: IVec2, home: Option<u32>) -> Vec<IVec2> {
    let mut ring: Vec<IVec2> =
        rect.expanded(1).tiles().filter(|p| !rect.contains(*p) && workable_spot(state, home, *p)).collect();
    ring.sort_by_key(|p| ((*p - entrance).abs().element_sum(), p.y, p.x));
    ring
}

/// Navigation component builders reach a site from: its entrance's; for a site entered from
/// any side (wall pieces, [`crate::defs::Access::AnySide`]) the component of an access tile in
/// the town (that of the main headquarters' entrance), else of its first access tile.
fn site_component(state: &GameState, defs: &Defs, building: BuildingId, entrance: IVec2) -> Option<u32> {
    if !crate::buildings::is_open_sided(state, defs, building) {
        return state.navigation.component(entrance);
    }
    let comps: Vec<u32> = crate::buildings::access_points(state, defs, building)
        .into_iter()
        .filter_map(|a| state.navigation.component(a.tile))
        .collect();
    let town = storage::main_headquarters(state, defs)
        .and_then(|hq| state.buildings.get(hq))
        .and_then(|b| state.navigation.component(b.entrance()));
    town.filter(|t| comps.contains(t)).or_else(|| comps.first().copied())
}

/// True if a farmer of `farm` can reach field tile `p` on foot from the farm's entrance (the
/// farm only works such tiles, `crate::agriculture`).
pub(crate) fn field_reachable(state: &GameState, farm: BuildingId, p: IVec2) -> bool {
    let home = state.buildings.get(farm).and_then(|b| state.navigation.component(b.entrance()));
    home.is_some() && workable_spot(state, home, p)
}

/// The field tile a farmer of `farm` works on: where it stands if that is a field of the farm
/// near the current task (farmers do not walk after every tile), else the reachable field tile
/// within [`FARMER_REACH`] of the task that is `index`-th nearest to it (ties in claim order).
/// `None` without a task.
fn farmer_spot(state: &GameState, farm: BuildingId, citizen: &Citizen, index: usize) -> Option<IVec2> {
    let f = state.agriculture.farm(farm)?;
    let task = f.task()?.tile;
    let near = |p: IVec2| (p - task).abs().max_element();
    let here = citizen.motion.pos.tile();
    if citizen.motion.inside.is_none() && state.agriculture.owner_of(here) == Some(farm) && near(here) <= FARMER_REACH {
        return Some(here);
    }
    let mut spots: Vec<(i32, usize, IVec2)> = f
        .tiles()
        .iter()
        .enumerate()
        .filter(|(_, t)| near(t.pos) <= FARMER_REACH && field_reachable(state, farm, t.pos))
        .map(|(i, t)| (near(t.pos), i, t.pos))
        .collect();
    spots.sort_unstable_by_key(|(d, i, _)| (*d, *i));
    (!spots.is_empty()).then(|| spots[index % spots.len()].2)
}

/// Farmers work on field tiles at most this many tiles (Chebyshev) from the current task; only
/// there they count as present.
pub const FARMER_REACH: i32 = 2;

/// Where worker `id` of `building` works now: a work spot outside or the building itself.
fn work_destination(state: &GameState, defs: &Defs, id: CitizenId, building: BuildingId) -> Destination {
    let inside = Destination::Building { building, purpose: Purpose::Work };
    let Some(c) = state.population.citizens.get(id) else { return inside };
    let index = state.population.workers_of(building).iter().position(|w| *w == id).unwrap_or(0);
    let spot = match work_area(state, defs, building) {
        WorkArea::Inside => None,
        WorkArea::Spot(tile) => Some(tile),
        WorkArea::Fields => farmer_spot(state, building, c, index),
        WorkArea::Around(rect) => {
            let entrance = state.buildings.get(building).map_or(rect.origin, |b| b.entrance());
            let spots = builder_spots(state, rect, entrance, site_component(state, defs, building, entrance));
            Some(if spots.is_empty() { entrance } else { spots[index % spots.len()] })
        }
    };
    spot.map_or(inside, |tile| Destination::Spot { building, tile })
}

/// Where citizen `id` wants to be now: the routine (see the module documentation), unless
/// enemies are near — then a civilian shelters.
pub(crate) fn desired_destination(state: &GameState, defs: &Defs, id: CitizenId, venues: &mut Venues) -> Option<Destination> {
    let dest = routine_destination(state, defs, id, venues);
    let c = state.population.citizens.get(id)?;
    if c.soldier.is_some() || c.carry.is_some() {
        return dest;
    }
    shelter(state, defs, c, dest, venues).or(dest)
}

/// The shelter of civilian `c` if it or its destination `dest` is within the alert radius of
/// an enemy: its home, or the main headquarters if the home is endangered too (`None`: no
/// danger).
fn shelter(state: &GameState, defs: &Defs, c: &Citizen, dest: Option<Destination>, venues: &mut Venues) -> Option<Destination> {
    let radius = i32::from(defs.rules().military.alert_radius);
    let danger = venues.danger(state);
    if danger.is_empty() {
        return None;
    }
    let here = TileRect::tile(c.motion.pos().tile());
    let target = dest.and_then(|d| match d {
        Destination::Building { building, .. } | Destination::Spot { building, .. } => state.buildings.get(building).map(|b| b.rect()),
        Destination::Tile(t) => Some(TileRect::tile(t)),
        Destination::MapEdge(_) => None,
    });
    let threatened = endangered(danger, &here, radius) || target.is_some_and(|r| endangered(danger, &r, radius));
    if !threatened {
        return None;
    }
    let h = state.population.households.get(c.household)?;
    let home = h.home.and_then(|b| state.buildings.get(b).map(|bb| (b, bb.rect())));
    if let Some((home, rect)) = home
        && !endangered(danger, &rect, radius)
    {
        return Some(Destination::Building { building: home, purpose: Purpose::Shelter });
    }
    let hq = storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b).map(|bb| (b, bb.rect())));
    match (hq, home) {
        (Some((hq, rect)), _) if !endangered(danger, &rect, radius) => Some(Destination::Building { building: hq, purpose: Purpose::Shelter }),
        (_, Some((home, _))) => Some(Destination::Building { building: home, purpose: Purpose::Shelter }),
        (Some((hq, _)), None) => Some(Destination::Building { building: hq, purpose: Purpose::Shelter }),
        (None, None) => None,
    }
}

/// Where citizen `id` wants to be according to the routine alone (see the module table).
fn routine_destination(state: &GameState, defs: &Defs, id: CitizenId, venues: &mut Venues) -> Option<Destination> {
    let c = state.population.citizens.get(id)?;
    if let Some(dest) = c.carry.and_then(|t| carry_destination(state, t.job)) {
        return dest;
    }
    // Soldiers follow their duty (docs/game_design.md §11); off duty the normal routine.
    if let Some(dest) = crate::military::orders::soldier_destination(state, id) {
        return dest;
    }
    let h = state.population.households.get(c.household)?;
    let go = |building: BuildingId, purpose: Purpose| Some(Destination::Building { building, purpose });
    let home = home_of(state, defs, h);
    let home_dest = home.and_then(|(b, purpose)| go(b, purpose));
    let r = &defs.rules().walking.routine;
    let t = state.clock.tick_of_day();
    if t < at(r.wake_hour) || t >= at(r.sleep_hour) {
        return home_dest;
    }
    let home_rect = home.and_then(|(b, _)| state.buildings.get(b)).map(|b| b.rect());
    let runner = errand_runner(state, defs, h);
    let errand_due = |venues: &mut Venues| -> Option<BuildingId> {
        let rect = home_rect?;
        if runner != Some(id) || h.home.is_none() || c.stage == LifeStage::Child {
            return None;
        }
        let evening = (at(r.errand_hour)..at(r.leisure_hour)).contains(&t);
        let hungry = super::supply::pantry_without_food(state, defs, c.household);
        let freezing = super::supply::pantry_without_fuel(state, defs, c.household);
        if !evening && !hungry && !freezing {
            return None;
        }
        venues.collect(state, defs);
        // The market in reach, without one the storage covering the house: the household
        // restocks its pantry there (step 3b, `supply`).
        let place = nearest(&venues.markets, &rect).or_else(|| super::supply::errand_storage(state, defs, &rect));
        place.filter(|p| {
            evening
                || (hungry && super::supply::offers_food(state, defs, *p))
                || (freezing && super::supply::offers_fuel(state, defs, *p))
        })
    };
    // Festival afternoons: celebrate at the church (else the market) in reach – after the
    // errand: a feast must not leave the pantry empty (in winter the house would freeze).
    if state.events.festival_active(state.clock.tick)
        && (at(r.lunch_end_hour)..at(r.home_hour)).contains(&t)
        && !is_sick(defs, c)
        && let Some(rect) = home_rect
    {
        if let Some(place) = errand_due(venues) {
            return go(place, Purpose::Errand);
        }
        venues.collect(state, defs);
        if let Some(venue) = nearest(&venues.churches, &rect).or_else(|| nearest(&venues.markets, &rect)) {
            return go(venue, Purpose::Leisure);
        }
    }
    if is_sick(defs, c) {
        // The household still eats: a sick runner runs the errand if nobody healthy can.
        if let Some(place) = errand_due(venues) {
            return go(place, Purpose::Errand);
        }
        if let Some(rect) = home_rect {
            venues.collect(state, defs);
            if let Some(hospital) = nearest(&venues.hospitals, &rect) {
                return go(hospital, Purpose::Treatment);
            }
        }
        return home_dest;
    }
    if let Some(job) = c.job
        && building_work_window(state, defs, job.building).contains(t)
    {
        return Some(work_destination(state, defs, id, job.building));
    }
    // Services are looked up around the home (without one: nowhere to go but work).
    let Some(home_rect) = home_rect else { return home_dest };
    let day = state.clock.total_days();
    match c.stage {
        LifeStage::Child => {
            let pupil = c.age_on(day) >= u32::from(defs.rules().population.school_age);
            if pupil && (at(r.school_start_hour)..at(r.school_end_hour)).contains(&t) {
                venues.collect(state, defs);
                if let Some(school) = nearest(&venues.schools, &home_rect) {
                    return go(school, Purpose::School);
                }
            }
        }
        LifeStage::Adult | LifeStage::Elder => {
            // Errands in the evening, or at once when the pantry holds no food (on a cold day no
            // fuel) and the market (or storage) has some.
            if let Some(place) = errand_due(venues) {
                return go(place, Purpose::Errand);
            }
            if (at(r.leisure_hour)..at(r.home_hour)).contains(&t) && (u64::from(id.index()) + day) % 2 == 0 {
                venues.collect(state, defs);
                let tavern = nearest(&venues.taverns, &home_rect);
                let church = nearest(&venues.churches, &home_rect);
                let prefer_tavern = (u64::from(id.index()) / 2 + day) % 2 == 0;
                let venue = if prefer_tavern { tavern.or(church) } else { church.or(tavern) };
                if let Some(venue) = venue {
                    return go(venue, Purpose::Leisure);
                }
            }
        }
    }
    home_dest
}

/// The member of household `h` who runs its errands: a healthy civilian adult or elder first,
/// then a healthy one, then any grown-up (soldiers only if nobody else can).
fn errand_runner(state: &GameState, defs: &Defs, h: &Household) -> Option<CitizenId> {
    let member = |m: &CitizenId| state.population.citizens.get(*m);
    let grown = |m: &CitizenId| member(m).is_some_and(|c| c.stage != LifeStage::Child);
    let healthy = |m: &CitizenId| member(m).is_some_and(|c| !is_sick(defs, c));
    let civilian = |m: &CitizenId| member(m).is_some_and(|c| c.soldier.is_none());
    let find = |f: &dyn Fn(&CitizenId) -> bool| h.members.iter().copied().find(|m| grown(m) && f(m));
    find(&|m| healthy(m) && civilian(m)).or_else(|| find(&|m| healthy(m))).or_else(|| find(&|_| true))
}

/// Routine step of a citizen (part of its staggered update): sets the destination the
/// routine asks for and refreshes the activity.
pub(crate) fn update(state: &mut GameState, defs: &Defs, id: CitizenId, venues: &mut Venues) {
    let dest = desired_destination(state, defs, id, venues);
    let before = state.population.citizens.get(id).and_then(|c| c.motion.dest);
    movement::set_destination(state, WalkerId::Citizen(id), dest);
    if let Some(Destination::Building { building, purpose: Purpose::Errand }) = dest
        && before != dest
        && let Some(c) = state.population.citizens.get(id)
        && c.motion.inside == Some(building)
    {
        // Already there (e.g. a carrier whose storage is the household's errand storage).
        let household = c.household;
        super::supply::restock_on_errand(state, defs, household, building);
    }
    refresh_activity(state, defs, id);
}

/// Activity of a citizen from where it is and what it does there.
pub fn current_activity(state: &GameState, defs: &Defs, c: &Citizen) -> Activity {
    if c.soldier.is_some() && matches!(c.activity, Activity::Training | Activity::OnDuty | Activity::Fighting | Activity::Campaign)
    {
        // The military keeps the activity of soldiers on duty current (it knows the duty).
        return c.activity;
    }
    routine_activity(state, defs, c)
}

/// Activity from the daily routine alone (where the citizen is and what the routine has it do).
fn routine_activity(state: &GameState, defs: &Defs, c: &Citizen) -> Activity {
    let r = &defs.rules().walking.routine;
    let t = state.clock.tick_of_day();
    match c.motion.phase {
        TripPhase::Walking | TripPhase::Queued => return Activity::Walking,
        TripPhase::NoPath { .. } => return Activity::NoPath,
        TripPhase::Still => {}
    }
    let inside = c.motion.inside;
    if c.carry.is_some() {
        // Loading, unloading or waiting for room with goods.
        return Activity::Working;
    }
    if let Some(job) = c.job
        && !in_treatment_trip(c)
        && (inside == Some(job.building) || matches!(c.motion.dest, Some(Destination::Spot { building, .. }) if building == job.building))
    {
        let window = building_work_window(state, defs, job.building);
        if window.is_lunch(t) {
            return Activity::LunchBreak;
        }
        if window.is_working(t) {
            return Activity::Working;
        }
    }
    match c.motion.dest {
        Some(Destination::Building { building, purpose }) if inside == Some(building) => match purpose {
            Purpose::School => return Activity::School,
            Purpose::Errand => return Activity::Errand,
            Purpose::Leisure if state.events.festival_active(state.clock.tick) => return Activity::Festival,
            Purpose::Leisure => return Activity::Leisure,
            Purpose::Treatment => return Activity::Treatment,
            Purpose::Shelter => return Activity::Sheltering,
            Purpose::Pickup | Purpose::Delivery => return Activity::Working,
            Purpose::Home | Purpose::Camp | Purpose::Work => {}
        },
        _ => {}
    }
    if t < at(r.wake_hour) || t >= at(r.sleep_hour) {
        Activity::Sleeping
    } else if is_sick(defs, c) {
        Activity::Sick
    } else if c.stage == LifeStage::Adult
        && c.job.is_none()
        && (at(r.work_start_hour)..at(r.work_end_hour)).contains(&t)
    {
        Activity::Idle
    } else {
        Activity::Leisure
    }
}

/// Runs the routine step of every worker of `building` at once (its work spots changed: a new
/// cycle target, a new farm task).
pub(crate) fn refresh_workers(state: &mut GameState, defs: &Defs, building: BuildingId) {
    let workers = state.population.workers_of(building).to_vec();
    let mut venues = Venues::default();
    for w in workers {
        update(state, defs, w, &mut venues);
    }
}

/// Stores the current activity of citizen `id`.
pub(crate) fn refresh_activity(state: &mut GameState, defs: &Defs, id: CitizenId) {
    let Some(c) = state.population.citizens.get(id) else { return };
    let activity = if c.soldier.is_some() {
        // On duty the military knows what the soldier does; off duty as everybody else.
        crate::military::orders::soldier_activity(state, id).unwrap_or_else(|| routine_activity(state, defs, c))
    } else {
        current_activity(state, defs, c)
    };
    if let Some(c) = state.population.citizens.get_mut(id) {
        c.activity = activity;
    }
}

/// German description of what citizen `id` is doing, naming the building ("Arbeitet
/// (Sägewerk)", "Auf dem Weg zur Arbeit (Sägewerk)", "Findet keinen Weg (Ziel: Sägewerk)").
pub fn activity_text(state: &GameState, defs: &Defs, id: CitizenId) -> Option<String> {
    let c = state.population.citizens.get(id)?;
    if let Some(text) = crate::military::orders::soldier_activity_text(state, defs, id) {
        return Some(text);
    }
    let name = |b: BuildingId| {
        state.buildings.get(b).and_then(|b| defs.buildings().get(b.kind().index())).map(|d| d.name.clone())
    };
    let dest = c.motion.dest;
    let dest_name = dest.and_then(Destination::building).and_then(name);
    let activity = current_activity(state, defs, c);
    if let Some(text) = carry_text(state, defs, c, activity, &name) {
        return Some(text);
    }
    Some(match (activity, dest) {
        (Activity::Walking, Some(Destination::Building { purpose, .. })) => match dest_name {
            Some(n) => format!("{} ({n})", purpose.walking_text()),
            None => purpose.walking_text().to_string(),
        },
        (Activity::Walking, _) => "Unterwegs".to_string(),
        (Activity::NoPath, _) => match dest_name {
            Some(n) => format!("Findet keinen Weg (Ziel: {n})"),
            None => Activity::NoPath.display_name().to_string(),
        },
        (Activity::Working, Some(Destination::Spot { building, .. })) => match name(building) {
            Some(n) => format!("{} ({n})", activity.display_name()),
            None => activity.display_name().to_string(),
        },
        (Activity::Working | Activity::LunchBreak | Activity::School | Activity::Errand, _)
        | (Activity::Leisure, Some(Destination::Building { purpose: Purpose::Leisure, .. })) => {
            match c.motion.inside.and_then(name) {
                Some(n) => format!("{} ({n})", activity.display_name()),
                None => activity.display_name().to_string(),
            }
        }
        _ => activity.display_name().to_string(),
    })
}

/// German description of a transport ("Holt Holz (Holzfällerhütte)", "Bringt 8 Holz
/// (Lagerhaus)", "Lädt ab (Lagerhaus)", "Wartet mit 8 Holz auf Lagerplatz (Werkstatt)" — at
/// a workshop or site that cannot take the goods, or anywhere without a target).
fn carry_text(
    state: &GameState,
    defs: &Defs,
    c: &Citizen,
    activity: Activity,
    name: &dyn Fn(BuildingId) -> Option<String>,
) -> Option<String> {
    let trip = c.carry?;
    let job = state.logistics.job(trip.job)?;
    let resource = defs.resources().get(job.resource.index()).map_or_else(|| job.resource.to_string(), |r| r.name.clone());
    let with_place = |text: String, b: BuildingId| match name(b) {
        Some(n) => format!("{text} ({n})"),
        None => text,
    };
    Some(match (job.stage, activity) {
        (JobStage::Fetching { .. }, Activity::Walking) => with_place(format!("Holt {resource}"), job.source),
        (JobStage::Fetching { .. }, _) => with_place("Lädt auf".to_string(), job.source),
        (JobStage::Carrying { .. }, Activity::Walking) => with_place(format!("Bringt {} {resource}", job.amount), job.target),
        (JobStage::Carrying { .. }, _) if c.motion.inside == Some(job.target) => {
            // A storage target always takes the load in the end (beyond its capacity if
            // nothing else has room); other targets without room keep the carrier waiting.
            if job.kind.to_storage() || crate::logistics::target_room(state, defs, job) > 0 {
                with_place("Lädt ab".to_string(), job.target)
            } else {
                with_place(format!("Wartet mit {} {resource} auf Lagerplatz", job.amount), job.target)
            }
        }
        (JobStage::Carrying { .. }, _) => format!("Wartet mit {} {resource} auf Lagerplatz", job.amount),
        (JobStage::Waiting, _) => return None,
    })
}
