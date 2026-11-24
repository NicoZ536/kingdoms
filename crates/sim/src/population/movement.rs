//! Walking (docs/game_design.md §6 "Bewegung", §2 walking speed): positions, trips, the path
//! request queue with its per-tick budget, movement, and the travellers who leave the
//! kingdom.
//!
//! ## Positions
//! A walker's position is in [`SUBTILE`] units per tile ([`SubPos`], integers only); tile
//! centres are the waypoints. Walkers move between the centres of 4-neighbouring tiles, so a
//! position always lies on such a segment. Besides the position the state keeps the position
//! of the previous tick (for render interpolation), the facing and the building the walker is
//! inside (hidden; its position is then the building's door).
//!
//! ## Trips
//! A trip goes to a [`Destination`]: a building (home, workplace, school, market, tavern or
//! church, the headquarters camp of the homeless) or a map-edge tile (emigrants). Starting a
//! trip queues a path request ([`TripPhase::Queued`]); every tick the queue is answered in
//! order until the pathfinder budget is used up — `walking.path_budget_per_tick` node
//! expansions plus `path_budget_per_queued` for every request waiting at the start of the tick,
//! at most `path_budget_max_per_tick` (a long queue at rush hour drains faster, a burst never
//! makes a tick spike beyond the cap); the rest wait. A path leads from the entrance in front of the start building's
//! door to the destination's entrance; the step out of the door and into the destination's
//! door are part of it. Without a path the walker stays where it is ([`TripPhase::NoPath`],
//! inside a building it stays inside), tries again after `walking.no_path_retry_ticks` and the
//! destination is reported ("Kein Weg …", once per spell of failed searches per building,
//! `crate::notices`).
//!
//! ## Movement
//! Every tick each walking walker advances by its speed: `base_walk_speed` (tiles per tick)
//! × 1000 / the cost of the step being walked (road speed or terrain, slopes) × its personal
//! modifier (age, health) × the weather (snow cover, rain, storm), all in integer permille
//! with a carried remainder. Before entering a tile the walker checks that the step is still
//! allowed (something may have been built on its path) and asks for a new path otherwise; a
//! changed destination also takes effect at the next tile centre.
//!
//! ## Travellers
//! Emigrants are no citizens any more: they walk to the map edge as [`Traveler`]s and vanish
//! there. Immigrants are citizens from the start; they appear on the map edge (see
//! [`entry_point`]) and walk to their new home.

use std::collections::{BTreeMap, VecDeque};

use glam::IVec2;
use serde::{Deserialize, Serialize};

use super::spatial::{CitizenRole, SpatialIndex};
use super::{CitizenId, LifeStage};
use crate::buildings::BuildingId;
use crate::defs::{Defs, Weather};
use crate::economy::storage;
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::grid::Direction;
use crate::ids::{Arena, Id};
use crate::navigation;
use crate::state::GameState;

/// Sub-tile units per tile edge: positions are stored in 1/256 tile.
pub const SUBTILE: i32 = 256;


/// A map position in 1/[`SUBTILE`] tile. Tile `(x, y)` covers `x·256 ..= x·256 + 255`; its
/// centre is `x·256 + 128`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubPos {
    /// East–west coordinate.
    pub x: i32,
    /// North–south coordinate.
    pub y: i32,
}

impl SubPos {
    /// Centre of `tile`.
    pub const fn center(tile: IVec2) -> SubPos {
        SubPos { x: tile.x * SUBTILE + SUBTILE / 2, y: tile.y * SUBTILE + SUBTILE / 2 }
    }

    /// Tile containing the position.
    pub const fn tile(self) -> IVec2 {
        IVec2::new(self.x.div_euclid(SUBTILE), self.y.div_euclid(SUBTILE))
    }

    /// True at a tile centre.
    pub const fn is_center(self) -> bool {
        self.x.rem_euclid(SUBTILE) == SUBTILE / 2 && self.y.rem_euclid(SUBTILE) == SUBTILE / 2
    }

    /// Position in tiles as floating point (for renderers; not used by the simulation).
    pub fn to_tiles(self) -> (f32, f32) {
        (self.x as f32 / SUBTILE as f32, self.y as f32 / SUBTILE as f32)
    }

    fn offset(self, d: Direction, units: i32) -> SubPos {
        let o = d.offset();
        SubPos { x: self.x + o.x * units, y: self.y + o.y * units }
    }
}

/// Why a citizen walks to a building (and what it does there).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Purpose {
    /// Home (sleep, meals, free time with the family).
    Home,
    /// The homeless camp at the main headquarters.
    Camp,
    /// Workplace or construction site.
    Work,
    /// School (pupils).
    School,
    /// Market errands.
    Errand,
    /// Tavern or church in the evening.
    Leisure,
    /// Carriers on the way to pick goods up (step 3b).
    Pickup,
    /// Carriers, and gatherers with their yield, on the way to hand goods over.
    Delivery,
    /// Civilians fleeing from enemies nearby to their home or the headquarters.
    Shelter,
    /// Sick citizens going to a hospital in reach.
    Treatment,
}

impl Purpose {
    /// German phrase for walking there ("Auf dem Weg zur Arbeit").
    pub const fn walking_text(self) -> &'static str {
        match self {
            Purpose::Home => "Auf dem Heimweg",
            Purpose::Camp => "Auf dem Weg zum Lager am Hauptquartier",
            Purpose::Work => "Auf dem Weg zur Arbeit",
            Purpose::School => "Auf dem Weg zur Schule",
            Purpose::Errand => "Auf dem Weg zum Markt",
            Purpose::Leisure => "Auf dem Weg zur Freizeit",
            Purpose::Pickup => "Auf dem Weg zur Abholung",
            Purpose::Delivery => "Bringt Waren",
            Purpose::Shelter => "Flieht vor den Feinden",
            Purpose::Treatment => "Auf dem Weg ins Krankenhaus",
        }
    }

    /// True for trips that belong to the job (the workplace, pick-ups and deliveries): a
    /// commute is measured only from a trip that did not.
    pub const fn is_work(self) -> bool {
        matches!(self, Purpose::Work | Purpose::Pickup | Purpose::Delivery)
    }
}

/// Where a walker is heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Destination {
    /// Into a building (through its door).
    Building {
        /// The building.
        building: BuildingId,
        /// Why.
        purpose: Purpose,
    },
    /// Off the map at this edge tile (emigrants).
    MapEdge(IVec2),
    /// A work spot outside (step 3b): stand on the centre of `tile` and work there for
    /// `building` — builders around a construction site, farmers on a field, gatherers at a
    /// tree, deposit or shore.
    Spot {
        /// The workplace the citizen works for.
        building: BuildingId,
        /// The tile worked on (walkable).
        tile: IVec2,
    },
    /// The centre of a map tile (soldiers on guard, on patrol or chasing an enemy, enemies on
    /// the march; docs/game_design.md §11).
    Tile(IVec2),
}

impl Destination {
    /// The building walked to or worked for (a work spot's workplace), if any.
    pub fn building(self) -> Option<BuildingId> {
        match self {
            Destination::Building { building, .. } | Destination::Spot { building, .. } => Some(building),
            Destination::MapEdge(_) | Destination::Tile(_) => None,
        }
    }

    /// Why the walker goes there (work spots: [`Purpose::Work`]).
    pub fn purpose(self) -> Option<Purpose> {
        match self {
            Destination::Building { purpose, .. } => Some(purpose),
            Destination::Spot { .. } => Some(Purpose::Work),
            Destination::MapEdge(_) | Destination::Tile(_) => None,
        }
    }

    /// True for destinations that belong to the job (see [`Purpose::is_work`]).
    pub fn is_work(self) -> bool {
        self.purpose().is_some_and(Purpose::is_work)
    }
}

/// State of the current trip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TripPhase {
    /// Not travelling: inside a building or standing on a tile centre.
    #[default]
    Still,
    /// Waiting for the pathfinder (the request is in the queue).
    Queued,
    /// Following the path.
    Walking,
    /// No path to the destination; the next attempt is due at `retry_at` (tick).
    NoPath {
        /// Tick of the next attempt.
        retry_at: u64,
    },
}

/// Position and trip of a walker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Motion {
    pub(crate) pos: SubPos,
    pub(crate) prev: SubPos,
    pub(crate) facing: Direction,
    /// Inside this building (hidden); `pos` is then its door.
    pub(crate) inside: Option<BuildingId>,
    pub(crate) dest: Option<Destination>,
    pub(crate) phase: TripPhase,
    /// Steps of the current path; `path[next]` is being walked.
    pub(crate) path: Vec<Direction>,
    pub(crate) next: u16,
    /// Sub-tile units of the current step already walked (0 = at its start centre).
    pub(crate) progress: u16,
    /// Thousandths of a sub-tile unit carried to the next tick.
    pub(crate) frac: u16,
    /// The destination changed while walking: ask for a new path at the next tile centre.
    pub(crate) replan: bool,
    /// Tick the current trip was requested (commute time).
    pub(crate) departed_at: u64,
    /// The current trip is a commute (to work from somewhere else); it is measured on arrival.
    pub(crate) commuting: bool,
    /// Ticks the last walk to the workplace took (0 = none yet).
    pub(crate) last_commute: u32,
}

impl Motion {
    /// A walker inside `building` whose door is `door`.
    pub(crate) fn in_building(building: BuildingId, door: IVec2) -> Motion {
        let pos = SubPos::center(door);
        Motion {
            pos,
            prev: pos,
            facing: Direction::South,
            inside: Some(building),
            dest: None,
            phase: TripPhase::Still,
            path: Vec::new(),
            next: 0,
            progress: 0,
            frac: 0,
            replan: false,
            departed_at: 0,
            commuting: false,
            last_commute: 0,
        }
    }

    /// A walker standing on the centre of `tile`.
    pub(crate) fn standing(tile: IVec2) -> Motion {
        let pos = SubPos::center(tile);
        Motion {
            pos,
            prev: pos,
            facing: Direction::South,
            inside: None,
            dest: None,
            phase: TripPhase::Still,
            path: Vec::new(),
            next: 0,
            progress: 0,
            frac: 0,
            replan: false,
            departed_at: 0,
            commuting: false,
            last_commute: 0,
        }
    }

    /// True if the walker is at (or inside) `dest`.
    fn has_arrived(&self, dest: Option<Destination>) -> bool {
        match dest {
            Some(Destination::Building { building, .. }) => self.inside == Some(building),
            Some(Destination::MapEdge(tile) | Destination::Spot { tile, .. } | Destination::Tile(tile)) => {
                self.inside.is_none() && self.pos == SubPos::center(tile)
            }
            None => true,
        }
    }

    /// True if the walker stands at (or inside) its destination.
    pub(crate) fn is_at_destination(&self) -> bool {
        self.has_arrived(self.dest)
    }

    /// Current position.
    pub fn pos(&self) -> SubPos {
        self.pos
    }

    /// Position one tick ago (render interpolation).
    pub fn prev_pos(&self) -> SubPos {
        self.prev
    }

    /// Facing (direction of the last step).
    pub fn facing(&self) -> Direction {
        self.facing
    }

    /// The building the walker is inside (hidden), if any.
    pub fn inside(&self) -> Option<BuildingId> {
        self.inside
    }

    /// Current destination.
    pub fn destination(&self) -> Option<Destination> {
        self.dest
    }

    /// State of the current trip.
    pub fn phase(&self) -> TripPhase {
        self.phase
    }

    /// True while following a path.
    pub fn is_moving(&self) -> bool {
        self.phase == TripPhase::Walking
    }

    /// True on the map (not inside a building).
    pub fn is_visible(&self) -> bool {
        self.inside.is_none()
    }

    /// Steps of the current path still to walk (incl. the one in progress).
    pub fn remaining_steps(&self) -> usize {
        self.path.len().saturating_sub(usize::from(self.next))
    }

    /// Ticks the last walk to the workplace took (`None` before the first one).
    pub fn last_commute_ticks(&self) -> Option<u32> {
        (self.last_commute > 0).then_some(self.last_commute)
    }

    /// Tile the current step started on (the walker's tile when not walking).
    pub(crate) fn step_origin(&self) -> IVec2 {
        match (self.phase, self.path.get(usize::from(self.next))) {
            (TripPhase::Walking, Some(d)) => self.pos.offset(*d, -i32::from(self.progress)).tile(),
            _ => self.pos.tile(),
        }
    }
}

/// Handle of a traveller.
pub type TravelerId = Id<Traveler>;

/// A walker who is not a citizen: a former citizen walking off the map (emigrants), a
/// merchant or cart of a trade caravan ([`crate::diplomacy::Caravan`]) or an enemy: a bandit or
/// a soldier of an AI army ([`crate::military::Warband`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Traveler {
    pub(crate) motion: Motion,
    pub(crate) role: CitizenRole,
    /// Personal speed modifier in permille (age, health at departure; unit speed of enemies).
    pub(crate) speed_permille: u16,
    /// The caravan it belongs to (`None`: an emigrant).
    pub(crate) caravan: Option<u32>,
    /// The hostile band or army it belongs to (`None`: no enemy).
    pub(crate) warband: Option<u32>,
}

impl Traveler {
    /// Position and trip.
    pub fn motion(&self) -> &Motion {
        &self.motion
    }

    /// Sprite role.
    pub fn role(&self) -> CitizenRole {
        self.role
    }

    /// Id of the trade caravan it belongs to (`None`: an emigrant).
    pub fn caravan(&self) -> Option<u32> {
        self.caravan
    }

    /// Id of the hostile warband it belongs to (bandits, AI armies).
    pub fn warband(&self) -> Option<u32> {
        self.warband
    }

    /// True for enemies of the player.
    pub fn is_hostile(&self) -> bool {
        self.warband.is_some()
    }
}

/// Anything that walks: a citizen or a traveller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WalkerId {
    /// A citizen.
    Citizen(CitizenId),
    /// A traveller leaving the kingdom.
    Traveler(TravelerId),
}

/// Walking state of the population: the path request queue, the travellers and the
/// notification cooldowns (saved), plus the derived spatial index and list of active walkers.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Walking {
    /// Walkers waiting for a path, in request order.
    pub(crate) requests: VecDeque<WalkerId>,
    /// Node expansions earlier ticks spent on the first request (a long search continues).
    pub(crate) head_progress: u32,
    pub(crate) travelers: Arena<Traveler>,
    /// "Kein Weg" per destination building: reported once per spell (docs/game_design.md §2
    /// Meldungen), the tick of the last failed search.
    pub(crate) no_path_notices: crate::notices::Spells<BuildingId>,
    #[serde(skip)]
    pub(crate) index: SpatialIndex,
    /// Walkers moving or just stopped (prev ≠ pos), sorted; rebuilt after loading.
    #[serde(skip)]
    active: Vec<WalkerId>,
    #[serde(skip)]
    active_built: bool,
}

impl Walking {
    /// Walkers waiting for a path.
    pub fn queued_requests(&self) -> usize {
        self.requests.len()
    }

    /// A traveller by id.
    pub fn traveler(&self, id: TravelerId) -> Option<&Traveler> {
        self.travelers.get(id)
    }

    /// All travellers in id order.
    pub fn travelers(&self) -> impl Iterator<Item = (TravelerId, &Traveler)> + '_ {
        self.travelers.iter()
    }

    pub(crate) fn activate(&mut self, id: WalkerId) {
        if let Err(i) = self.active.binary_search(&id) {
            self.active.insert(i, id);
        }
    }

    pub(crate) fn deactivate(&mut self, id: WalkerId) {
        if let Ok(i) = self.active.binary_search(&id) {
            self.active.remove(i);
        }
    }

}

fn motion_of(state: &GameState, id: WalkerId) -> Option<&Motion> {
    match id {
        WalkerId::Citizen(c) => state.population.citizens.get(c).map(|c| &c.motion),
        WalkerId::Traveler(t) => state.population.walking.travelers.get(t).map(|t| &t.motion),
    }
}

fn motion_mut(state: &mut GameState, id: WalkerId) -> Option<&mut Motion> {
    match id {
        WalkerId::Citizen(c) => state.population.citizens.get_mut(c).map(|c| &mut c.motion),
        WalkerId::Traveler(t) => state.population.walking.travelers.get_mut(t).map(|t| &mut t.motion),
    }
}

/// Sets a new destination; queues a path request unless the walker is already there. While
/// walking, the change takes effect at the next tile centre.
pub(crate) fn set_destination(state: &mut GameState, id: WalkerId, dest: Option<Destination>) {
    let now = state.clock.tick;
    let Some(m) = motion_mut(state, id) else { return };
    if m.dest == dest {
        let due = match m.phase {
            TripPhase::NoPath { retry_at } => now >= retry_at,
            TripPhase::Still => !m.has_arrived(dest),
            TripPhase::Queued | TripPhase::Walking => false,
        };
        if due {
            m.phase = TripPhase::Queued;
            state.population.walking.requests.push_back(id);
        }
        return;
    }
    // A commute starts when a trip to work begins from somewhere that is not work; trips
    // between work places (spots, pick-ups, deliveries) continue it or are no commute.
    let was_work = m.dest.is_some_and(Destination::is_work);
    let to_work = dest.is_some_and(Destination::is_work);
    let restart = !(was_work && to_work);
    if restart {
        m.commuting = to_work;
    }
    m.dest = dest;
    match m.phase {
        TripPhase::Walking => {
            m.replan = true;
            if restart {
                m.departed_at = now;
            }
        }
        TripPhase::Queued => {
            if restart {
                m.departed_at = now;
            }
        }
        TripPhase::Still | TripPhase::NoPath { .. } => {
            if m.has_arrived(dest) {
                m.phase = TripPhase::Still;
                if to_work {
                    m.commuting = false;
                }
            } else {
                m.phase = TripPhase::Queued;
                if restart {
                    m.departed_at = now;
                }
                state.population.walking.requests.push_back(id);
            }
        }
    }
}

/// Personal speed modifier of a citizen in permille: age (children, elders) and health
/// (slower below `full_productivity_health`).
pub(crate) fn personal_speed_permille(defs: &Defs, stage: LifeStage, health: u16) -> u16 {
    let walking = &defs.rules().walking;
    let age = match stage {
        LifeStage::Child => u32::from(walking.child_speed_permille),
        LifeStage::Adult => 1000,
        LifeStage::Elder => u32::from(walking.elder_speed_permille),
    };
    let full = u32::from(defs.rules().population.full_productivity_health) * u32::from(super::CENTI);
    let sick = u32::from(walking.sick_speed_permille);
    let health = if full == 0 || u32::from(health) >= full {
        1000
    } else {
        sick + (1000u32.saturating_sub(sick)) * u32::from(health) / full
    };
    u16::try_from(age * health / 1000).unwrap_or(u16::MAX).max(1)
}

/// Weather factor on walking speed in permille: snow cover, rain and storm (scaled by the
/// weather's intensity).
pub(crate) fn weather_speed_permille(state: &GameState, defs: &Defs) -> u32 {
    let walking = &defs.rules().walking;
    let snow = u32::from(state.weather.snow_cover().min(100));
    let snow_factor = 1000 - (1000u32.saturating_sub(u32::from(walking.snow_speed_permille))) * snow / 100;
    let weather = match state.weather.current() {
        Weather::Rain => crate::weather::scale_percent(walking.rain_speed_permille / 10, state.weather.intensity()) * 10,
        Weather::Storm => crate::weather::scale_percent(walking.storm_speed_permille / 10, state.weather.intensity()) * 10,
        _ => 1000,
    };
    (snow_factor * weather / 1000).max(1)
}

/// Base speed in thousandths of a sub-tile unit per tick on a tile of cost 1000.
fn base_speed_milli(defs: &Defs) -> u64 {
    let speed = f64::from(defs.rules().base_walk_speed) * f64::from(SUBTILE) * 1000.0;
    speed.round().max(1.0) as u64
}

/// The tile where immigrants enter and emigrants leave: the map-edge tile nearest to the main
/// headquarters' entrance (Manhattan distance, ties by row and column) that walkers can reach
/// from it, preferring road tiles on the edge (a road leaving the map). Without a reachable
/// edge (an enclosed start) the headquarters' entrance itself; without a headquarters the
/// world's start point.
pub fn entry_point(state: &mut GameState, defs: &Defs) -> IVec2 {
    navigation::sync(state, defs);
    let hq = storage::main_headquarters(state, defs).and_then(|id| state.buildings.get(id)).map(|b| b.entrance());
    let Some(anchor) = hq else { return state.world.start };
    let (w, h) = (state.world.width(), state.world.height());
    let mut edge: Vec<IVec2> = Vec::with_capacity(2 * (w + h) as usize);
    for x in 0..w {
        edge.push(IVec2::new(x, 0));
        edge.push(IVec2::new(x, h - 1));
    }
    for y in 1..h - 1 {
        edge.push(IVec2::new(0, y));
        edge.push(IVec2::new(w - 1, y));
    }
    let key = |p: &IVec2| ((*p - anchor).abs().element_sum(), p.y, p.x);
    edge.sort_by_key(key);
    edge.dedup();
    let nav = &mut state.navigation;
    let reachable: Vec<IVec2> = edge.into_iter().filter(|p| nav.is_walkable(*p) && nav.reachable(anchor, *p)).collect();
    reachable.iter().copied().find(|p| state.roads.has_road(*p)).or_else(|| reachable.first().copied()).unwrap_or(anchor)
}

/// Turns the members of an emigrating household into travellers walking to the map edge
/// (called before the citizens are removed). Without a reachable map edge they leave at once.
pub(crate) fn depart_emigrants(state: &mut GameState, defs: &Defs, members: &[CitizenId]) {
    let exit = entry_point(state, defs);
    let (w, h) = (state.world.width(), state.world.height());
    if !(exit.x == 0 || exit.y == 0 || exit.x == w - 1 || exit.y == h - 1) {
        // No map edge in reach (enclosed start, no headquarters): they leave at once.
        return;
    }
    for id in members {
        let Some(c) = state.population.citizens.get(*id) else { continue };
        let role = super::spatial::role_of(state, defs, c);
        let speed = personal_speed_permille(defs, c.stage, c.health);
        let mut motion = c.motion.clone();
        let reachable = {
            let from = motion
                .inside
                .and_then(|b| state.buildings.get(b))
                .map_or(motion.step_origin(), |b| b.entrance());
            state.navigation.reachable(from, exit)
        };
        if !reachable {
            continue;
        }
        // Finish the current step first: travellers start at a tile centre (or a door).
        if motion.phase == TripPhase::Walking && motion.progress > 0 {
            let origin = motion.step_origin();
            motion.pos = SubPos::center(origin);
        }
        motion.prev = motion.pos;
        motion.path.clear();
        motion.next = 0;
        motion.progress = 0;
        motion.frac = 0;
        motion.replan = false;
        motion.dest = None;
        motion.phase = TripPhase::Still;
        let tid =
            state.population.walking.travelers.insert(Traveler { motion, role, speed_permille: speed, caravan: None, warband: None });
        let walker = WalkerId::Traveler(tid);
        index_insert(state, walker);
        set_destination(state, walker, Some(Destination::MapEdge(exit)));
    }
}

/// Puts every member of `household` inside its home (house, or the camp at the main
/// headquarters; standing at the world's start point without either). Game setup and new
/// families only: everybody else walks.
pub(crate) fn place_inside_home(state: &mut GameState, defs: &Defs, household: super::HouseholdId) {
    let Some(h) = state.population.households.get(household) else { return };
    let members = h.members.clone();
    let home = h.home.or_else(|| storage::main_headquarters(state, defs));
    let motion = match home.and_then(|b| state.buildings.get(b).map(|bb| (b, bb.door()))) {
        Some((b, door)) => Motion::in_building(b, door),
        None => Motion::standing(state.world.start),
    };
    for m in members {
        place(state, WalkerId::Citizen(m), motion.clone());
    }
}

/// Puts every member of `household` on the centre of `tile` (immigrants at the map edge) and
/// lets them pick their destination at once.
pub(crate) fn place_standing(state: &mut GameState, defs: &Defs, household: super::HouseholdId, tile: IVec2) {
    let Some(h) = state.population.households.get(household) else { return };
    let members = h.members.clone();
    let mut venues = super::routine::Venues::default();
    for m in members {
        place(state, WalkerId::Citizen(m), Motion::standing(tile));
        super::routine::update(state, defs, m, &mut venues);
    }
}

/// Replaces the motion of a walker, keeping the derived lists consistent.
pub(crate) fn place(state: &mut GameState, id: WalkerId, motion: Motion) {
    forget(state, id);
    let Some(m) = motion_mut(state, id) else { return };
    *m = motion;
    index_insert(state, id);
}

/// Inserts a visible walker into the spatial index.
pub(crate) fn index_insert(state: &mut GameState, id: WalkerId) {
    let Some(m) = motion_of(state, id) else { return };
    if m.inside.is_none() {
        let pos = m.pos;
        state.population.walking.index.insert(id, pos);
    }
}

/// Removes a walker from the derived lists before it is deleted.
pub(crate) fn forget(state: &mut GameState, id: WalkerId) {
    let Some(m) = motion_of(state, id) else { return };
    let (pos, visible) = (m.pos, m.inside.is_none());
    let walking = &mut state.population.walking;
    if visible {
        walking.index.remove(id, pos);
    }
    walking.deactivate(id);
    if walking.requests.front() == Some(&id) {
        walking.head_progress = 0;
    }
    walking.requests.retain(|r| *r != id);
}

/// Rebuilds the derived lists (after loading or rebalancing): the spatial index of visible
/// walkers and the sorted list of active walkers.
pub(crate) fn rebuild_derived(state: &mut GameState) {
    let mut index = SpatialIndex::new(state.world.width(), state.world.height());
    let mut active = Vec::new();
    for (id, c) in state.population.citizens.iter() {
        let walker = WalkerId::Citizen(id);
        if c.motion.inside.is_none() {
            index.insert(walker, c.motion.pos);
        }
        if c.motion.phase == TripPhase::Walking || c.motion.prev != c.motion.pos {
            active.push(walker);
        }
    }
    for (id, t) in state.population.walking.travelers.iter() {
        let walker = WalkerId::Traveler(id);
        index.insert(walker, t.motion.pos);
        if t.motion.phase == TripPhase::Walking || t.motion.prev != t.motion.pos {
            active.push(walker);
        }
    }
    active.sort();
    let walking = &mut state.population.walking;
    walking.index = index;
    walking.active = active;
    walking.active_built = true;
}

/// The first of `tiles` (sorted by row, column) a visible walker stands on or is stepping onto
/// or off right now, in walker order (citizens, then travellers, by id). Used to keep bridges
/// from vanishing under walkers' feet.
pub(crate) fn walker_on(state: &GameState, tiles: &[IVec2]) -> Option<IVec2> {
    let hit = |m: &Motion| -> Option<IVec2> {
        if m.inside.is_some() {
            return None;
        }
        let here = m.pos.tile();
        let origin = m.step_origin();
        let target = match (m.phase, m.path.get(usize::from(m.next))) {
            (TripPhase::Walking, Some(d)) => origin + d.offset(),
            _ => here,
        };
        [here, origin, target].into_iter().find(|p| tiles.binary_search_by_key(&(p.y, p.x), |t| (t.y, t.x)).is_ok())
    };
    let citizens = state.population.citizens.iter().map(|(_, c)| &c.motion);
    let travelers = state.population.walking.travelers.iter().map(|(_, t)| &t.motion);
    citizens.chain(travelers).find_map(hit)
}

/// Pipeline step 7, movement part (every tick): updates the navigation grid, lets citizens out
/// of buildings that vanished, answers path requests within the budget and moves every
/// walking walker.
pub(crate) fn run_movement(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let structure = state.buildings.revision();
    navigation::sync(state, defs);
    if !state.population.walking.active_built || !state.population.walking.index.fits(state.world.width(), state.world.height()) {
        rebuild_derived(state);
    }
    if structure != state.population.walking_seen_revision {
        state.population.walking_seen_revision = structure;
        refresh_insiders(state);
    }
    process_requests(state, defs, events);
    move_walkers(state, defs, events);
    let errands = std::mem::take(&mut state.population.errands_arrived);
    super::supply::restock_errands(state, defs, &errands);
    let guests = std::mem::take(&mut state.population.guests_arrived);
    super::supply::buy_at_venues(state, defs, &guests);
    if state.clock.is_new_hour() {
        let now = state.clock.tick;
        let gap = defs.rules().notices.resolved_ticks();
        let buildings = &state.buildings;
        state.population.walking.no_path_notices.prune(now, gap, |b| buildings.get(b).is_some());
    }
}

/// Citizens and travellers inside a building that no longer exists, or whose door moved (the
/// building was moved or enlarged), step out where they are — at the old door — and walk on
/// from there (no teleporting to the new door; a door tile now under a footprint is left by
/// the shortest way out, see [`trip_start`]).
fn refresh_insiders(state: &mut GameState) {
    let buildings = &state.buildings;
    let left = |m: &mut Motion| -> bool {
        let Some(b) = m.inside else { return false };
        let still_there = buildings.get(b).is_some_and(|building| SubPos::center(building.door()) == m.pos);
        if !still_there {
            m.inside = None;
        }
        !still_there
    };
    let mut expelled = Vec::new();
    for (id, c) in state.population.citizens.iter_mut() {
        if left(&mut c.motion) {
            expelled.push(WalkerId::Citizen(id));
        }
    }
    for (id, t) in state.population.walking.travelers.iter_mut() {
        if left(&mut t.motion) {
            expelled.push(WalkerId::Traveler(id));
        }
    }
    for id in expelled {
        index_insert(state, id);
        if let Some(m) = motion_mut(state, id)
            && m.dest.is_some()
            && m.phase != TripPhase::Queued
        {
            m.phase = TripPhase::Queued;
            state.population.walking.requests.push_back(id);
        }
    }
}

/// Answers queued path requests in order until the tick's budget of node expansions is used
/// up. A request whose search does not finish within the rest of the budget stays at the
/// front and continues in the next tick (`head_progress` counts what it already spent). Every
/// answered request costs its search's expansions plus one, whether it came from the cache
/// or not, so the timing does not depend on the cache.
fn process_requests(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let budget = defs.rules().walking.path_budget(state.population.walking.requests.len());
    let mut used = 0u32;
    while used < budget {
        let Some(&id) = state.population.walking.requests.front() else { break };
        let already = state.population.walking.head_progress;
        let allowance = budget - used;
        match plan_trip(state, defs, id, already, allowance, events) {
            Some(total) => {
                let walking = &mut state.population.walking;
                walking.requests.pop_front();
                walking.head_progress = 0;
                used = used.saturating_add(total.saturating_sub(already)).saturating_add(1);
            }
            None => {
                state.population.walking.head_progress = already.saturating_add(allowance);
                break;
            }
        }
    }
}

/// Start tile of a trip and the step out of the door (if the walker is inside a building).
/// Where the path search of a trip starts, the steps before it and the door the walker leaves
/// through (if it is inside a building).
struct TripStart {
    from: IVec2,
    prefix: Vec<Direction>,
    door: Option<IVec2>,
}

/// Where a trip can begin, most preferred first: out of the building the walker is in onto one
/// of its access tiles (the entrance; for buildings entered from any side also the other
/// walkable neighbours, [`crate::buildings::access_points`]), or the tile it stands on (out of
/// a footprint built under its feet first).
fn trip_starts(state: &GameState, defs: &Defs, m: &Motion) -> Vec<TripStart> {
    if let Some((id, b)) = m.inside.and_then(|id| state.buildings.get(id).map(|b| (id, b))) {
        // Out through the door onto an access tile.
        let door = b.door();
        return crate::buildings::access_points(state, defs, id)
            .into_iter()
            .map(|a| TripStart { from: a.tile, prefix: vec![a.into.opposite()], door: Some(door) })
            .collect();
    }
    let tile = m.pos.tile();
    if state.navigation.is_walkable(tile) || state.buildings.at(tile).is_none() {
        return vec![TripStart { from: tile, prefix: Vec::new(), door: None }];
    }
    // Something was built where the walker stands (or it stands on a door it came out of): it
    // walks out of the footprint first, by the shortest way that leads on to the goal.
    escape_routes(state, tile).into_iter().map(|(prefix, from)| TripStart { from, prefix, door: None }).collect()
}

/// The start and goal of a trip: with several candidates (buildings entered from any side) the
/// first start, in order, from which some goal is reachable, with the first such goal; else the
/// first of each (the search then reports that there is no way).
fn pick_trip(state: &mut GameState, starts: Vec<TripStart>, goals: Vec<Goal>) -> Option<(TripStart, Goal)> {
    if starts.len() > 1 || goals.len() > 1 {
        let found = starts.iter().enumerate().find_map(|(i, s)| {
            goals.iter().position(|g| state.navigation.reachable(s.from, g.0)).map(|j| (i, j))
        });
        if let Some((i, j)) = found {
            return starts.into_iter().nth(i).zip(goals.into_iter().nth(j));
        }
    }
    starts.into_iter().next().zip(goals.into_iter().next())
}

/// A goal tile and the step into the destination building's door after it, if any.
type Goal = (IVec2, Option<Direction>);

/// The ways out of the footprint covering `start`, shortest first (breadth-first inside the
/// footprint, directions in N, E, S, W order): the steps and the enterable tile they end on,
/// each exit tile once. Several, so that the trip can leave on the side its goal is reachable
/// from (one side may face a pocket, [`pick_trip`]).
fn escape_routes(state: &GameState, start: IVec2) -> Vec<(Vec<Direction>, IVec2)> {
    let Some((_, building)) = state.buildings.at(start) else { return Vec::new() };
    let rect = building.rect();
    let mut came: BTreeMap<(i32, i32), Direction> = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    let back = |came: &BTreeMap<(i32, i32), Direction>, mut p: IVec2| {
        let mut steps = Vec::new();
        while p != start {
            let Some(d) = came.get(&(p.x, p.y)).copied() else { break };
            steps.push(d);
            p -= d.offset();
        }
        steps.reverse();
        steps
    };
    let mut exits: Vec<(Vec<Direction>, IVec2)> = Vec::new();
    while let Some(p) = queue.pop_front() {
        for d in Direction::ALL {
            let q = p + d.offset();
            if rect.contains(q) {
                if q != start && !came.contains_key(&(q.x, q.y)) {
                    came.insert((q.x, q.y), d);
                    queue.push_back(q);
                }
            } else if state.navigation.step_cost(p, d).is_some() && !exits.iter().any(|(_, t)| *t == q) {
                let mut steps = back(&came, p);
                steps.push(d);
                exits.push((steps, q));
            }
        }
    }
    exits
}

/// True for a step out of (or through) a footprint the walker is caught in: from a blocked
/// tile to a tile of the same building.
fn escape_step(buildings: &crate::buildings::Buildings, nav: &navigation::Navigation, from: IVec2, d: Direction) -> bool {
    let to = from + d.offset();
    !nav.is_walkable(from) && buildings.id_at(from).is_some() && buildings.id_at(from) == buildings.id_at(to)
}

/// Answers the path request of `id` if its search finishes within `already + allowance`
/// expansions in total; returns the search's total expansions, or `None` if it goes on in the
/// next tick (the request stays queued).
fn plan_trip(
    state: &mut GameState,
    defs: &Defs,
    id: WalkerId,
    already: u32,
    allowance: u32,
    events: &mut Vec<SimEvent>,
) -> Option<u32> {
    let Some(m) = motion_of(state, id) else { return Some(0) };
    if m.phase != TripPhase::Queued {
        return Some(0);
    }
    if m.inside.is_some_and(|b| state.buildings.get(b).is_none()) {
        // The building vanished around the walker: it stands where the door was.
        if let Some(m) = motion_mut(state, id) {
            m.inside = None;
        }
        index_insert(state, id);
    }
    let Some(m) = motion_of(state, id) else { return Some(0) };
    let now = state.clock.tick;
    let dest = m.dest;
    // Goal tiles: empty if there is no way to stand there or enter.
    let goals: Option<Vec<Goal>> = match dest {
        None => None,
        Some(Destination::MapEdge(tile)) => Some(vec![(tile, None)]),
        Some(Destination::Tile(tile)) => Some(if state.navigation.is_walkable(tile) { vec![(tile, None)] } else { Vec::new() }),
        Some(Destination::Spot { building, tile }) => state
            .buildings
            .get(building)
            .map(|_| if state.navigation.is_walkable(tile) { vec![(tile, None)] } else { Vec::new() }),
        Some(Destination::Building { building, .. }) => state.buildings.get(building).map(|_| {
            crate::buildings::access_points(state, defs, building).into_iter().map(|a| (a.tile, Some(a.into))).collect()
        }),
    };
    let Some(goals) = goals else {
        // The destination vanished: stay; the routine picks a new one.
        if let Some(m) = motion_mut(state, id) {
            m.dest = None;
            m.phase = TripPhase::Still;
        }
        return Some(0);
    };
    if let (Some(inside), Some(Destination::Building { building, .. })) = (m.inside, dest)
        && inside == building
    {
        if let Some(m) = motion_mut(state, id) {
            m.phase = TripPhase::Still;
        }
        return Some(0);
    }
    let starts = trip_starts(state, defs, m);
    let (result, expansions) = match pick_trip(state, starts, goals) {
        Some((start, (to, enter))) => {
            let answer = match state.navigation.find_path_sliced(start.from, to, already, allowance, defs) {
                navigation::PathAnswer::Done(answer) => answer,
                navigation::PathAnswer::Pending => return None,
            };
            let path = answer.steps.map(|steps| {
                let mut path = start.prefix;
                path.extend(steps);
                path.extend(enter);
                (path, start.door)
            });
            (path, answer.expansions)
        }
        _ => (None, 0),
    };
    match result {
        Some((path, exit_door)) => start_walking(state, defs, id, path, exit_door, events),
        None => {
            let retry = u64::from(defs.rules().walking.no_path_retry_ticks);
            if let Some(m) = motion_mut(state, id) {
                m.phase = TripPhase::NoPath { retry_at: now + retry };
            }
            match dest {
                Some(Destination::Building { building, purpose }) => notify_no_path(state, defs, building, purpose, events),
                Some(Destination::Spot { building, .. }) => notify_no_path(state, defs, building, Purpose::Work, events),
                _ => {}
            }
        }
    }
    Some(expansions)
}

fn start_walking(
    state: &mut GameState,
    defs: &Defs,
    id: WalkerId,
    path: Vec<Direction>,
    exit_door: Option<IVec2>,
    events: &mut Vec<SimEvent>,
) {
    let Some(m) = motion_mut(state, id) else { return };
    let left_building = exit_door.is_some();
    if let Some(door) = exit_door {
        m.inside = None;
        m.pos = SubPos::center(door);
        m.prev = m.pos;
    }
    if let Some(d) = path.first() {
        m.facing = *d;
    }
    m.path = path;
    m.next = 0;
    m.progress = 0;
    m.frac = 0;
    m.replan = false;
    m.phase = TripPhase::Walking;
    let empty = m.path.is_empty();
    if left_building {
        index_insert(state, id);
    }
    state.population.walking.activate(id);
    if empty {
        arrive(state, defs, id, events);
    }
}

fn notify_no_path(state: &mut GameState, defs: &Defs, building: BuildingId, purpose: Purpose, events: &mut Vec<SimEvent>) {
    // Walkers try again every few minutes: one notice per spell of failed searches.
    let now = state.clock.tick;
    let gap = defs.rules().notices.resolved_ticks();
    if !state.population.walking.no_path_notices.begins(building, now, gap) {
        return;
    }
    let Some(b) = state.buildings.get(building) else { return };
    let name = defs.buildings().get(b.kind().index()).map_or("Gebäude", |d| d.name.as_str());
    let text = match purpose {
        Purpose::Work => format!("Kein Weg zur Arbeit: Das Gebäude „{name}“ ist für die Arbeiter nicht erreichbar."),
        Purpose::Home => format!("Kein Weg nach Hause: Das Gebäude „{name}“ ist für die Bewohner nicht erreichbar."),
        Purpose::Camp => format!("Kein Weg: Das Gebäude „{name}“ ist für die Obdachlosen nicht erreichbar."),
        Purpose::School => format!("Kein Weg zur Schule: Das Gebäude „{name}“ ist für die Kinder nicht erreichbar."),
        Purpose::Errand | Purpose::Leisure | Purpose::Shelter | Purpose::Treatment => {
            format!("Kein Weg: Das Gebäude „{name}“ ist für die Bewohner nicht erreichbar.")
        }
        Purpose::Pickup | Purpose::Delivery => format!("Kein Weg: Das Gebäude „{name}“ ist für die Träger nicht erreichbar."),
    };
    events.push(SimEvent::Notification(
        Notification::new(Severity::Warning, text).with_kind(NotificationKind::NoPath).at(b.entrance()).for_building(building),
    ));
}

/// What happened to a walker this tick.
enum Outcome {
    Moving,
    Arrived,
    Replan,
    /// An enemy would step onto one of the player's gates.
    Blocked,
}

/// Moves every active walker one tick (ascending id order).
fn move_walkers(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let active = std::mem::take(&mut state.population.walking.active);
    let base = base_speed_milli(defs);
    let weather = u64::from(weather_speed_permille(state, defs));
    let mut keep = Vec::with_capacity(active.len());
    for id in active {
        let personal = match id {
            WalkerId::Citizen(c) => state.population.citizens.get(c).map(|cz| {
                let own = u64::from(personal_speed_permille(defs, cz.stage, cz.health));
                // Soldiers march at the speed of their unit (riders faster, siege engines slower).
                if cz.soldier.is_some() { own * u64::from(crate::military::speed_permille(state, defs, c)) / 1000 } else { own }
            }),
            WalkerId::Traveler(t) => state.population.walking.travelers.get(t).map(|t| u64::from(t.speed_permille)),
        };
        let Some(personal) = personal else { continue };
        // Enemies are stopped by gates; an army withdrawing after a peace passes them.
        let hostile = matches!(id, WalkerId::Traveler(t) if state.population.walking.travelers.get(t).and_then(Traveler::warband).is_some_and(|w| {
            state.military.warband(w).is_none_or(|wb| wb.stage().is_hostile())
        }));
        let speed = base * personal / 1000 * weather / 1000;
        let outcome = {
            let nav = &state.navigation;
            let (motion, index) = match id {
                WalkerId::Citizen(c) => {
                    (state.population.citizens.get_mut(c).map(|c| &mut c.motion), &mut state.population.walking.index)
                }
                WalkerId::Traveler(t) => {
                    let walking = &mut state.population.walking;
                    (walking.travelers.get_mut(t).map(|t| &mut t.motion), &mut walking.index)
                }
            };
            let Some(m) = motion else { continue };
            m.prev = m.pos;
            if m.phase != TripPhase::Walking {
                // Stopped last tick: prev caught up, nothing more to do.
                continue;
            }
            let before = m.pos;
            let outcome = advance(nav, &state.buildings, defs, hostile, m, speed);
            if m.inside.is_none() && before.tile() != m.pos.tile() {
                index.moved(id, before, m.pos);
            }
            outcome
        };
        match outcome {
            Outcome::Moving => keep.push(id),
            Outcome::Blocked => {
                // An enemy stands in front of a gate: it stops at the tile centre; the military
                // step lets it attack the gate.
                if let Some(m) = motion_mut(state, id) {
                    m.path.clear();
                    m.next = 0;
                    m.progress = 0;
                    m.frac = 0;
                    m.replan = false;
                    m.phase = TripPhase::Still;
                }
                keep.push(id);
            }
            Outcome::Arrived => {
                arrive(state, defs, id, events);
                if motion_of(state, id).is_some() {
                    keep.push(id);
                }
            }
            Outcome::Replan => {
                if let Some(m) = motion_mut(state, id) {
                    m.path.clear();
                    m.next = 0;
                    m.progress = 0;
                    m.frac = 0;
                    m.replan = false;
                    m.phase = TripPhase::Queued;
                    state.population.walking.requests.push_back(id);
                }
                keep.push(id);
            }
        }
    }
    // Walkers activated during this pass (none in practice) are merged back in order.
    let added = std::mem::take(&mut state.population.walking.active);
    keep.extend(added);
    keep.sort();
    keep.dedup();
    state.population.walking.active = keep;
}

/// Advances a walking walker by `speed` (thousandths of a sub-tile unit per tick on a tile
/// of cost 1000).
fn advance(
    nav: &navigation::Navigation,
    buildings: &crate::buildings::Buildings,
    defs: &Defs,
    hostile: bool,
    m: &mut Motion,
    speed: u64,
) -> Outcome {
    let entering_door = |m: &Motion| {
        usize::from(m.next) + 1 == m.path.len() && matches!(m.dest, Some(Destination::Building { .. }))
    };
    let mut available: Option<u64> = None;
    loop {
        let Some(&d) = m.path.get(usize::from(m.next)) else { return Outcome::Arrived };
        let from = m.pos.offset(d, -i32::from(m.progress)).tile();
        let door = entering_door(m) || escape_step(buildings, nav, from, d);
        let step = if door { None } else { nav.step_cost(from, d) };
        if m.progress == 0 {
            if m.replan {
                return Outcome::Replan;
            }
            if !door && step.is_none() {
                // Something was built on the way (or the step is no longer allowed).
                return Outcome::Replan;
            }
            m.facing = d;
            if hostile && buildings.at(from + d.offset()).is_some_and(|(_, b)| crate::buildings::is_gate(defs, b)) {
                // Gates let only the player's own people through (docs/game_design.md §11).
                return Outcome::Blocked;
            }
        }
        let cost = u64::from(step.unwrap_or(1000).max(1));
        let budget = available.unwrap_or_else(|| speed * 1000 / cost + u64::from(m.frac));
        let units = budget / 1000;
        let remaining = u64::from((SUBTILE as u16).saturating_sub(m.progress));
        if units >= remaining {
            m.pos = m.pos.offset(d, remaining as i32);
            m.progress = 0;
            m.next += 1;
            available = Some(budget - remaining * 1000);
            if usize::from(m.next) >= m.path.len() {
                m.frac = 0;
                return Outcome::Arrived;
            }
        } else {
            m.pos = m.pos.offset(d, units as i32);
            m.progress += units as u16;
            m.frac = (budget % 1000) as u16;
            return Outcome::Moving;
        }
    }
}

/// End of a trip: into the destination building (if it still exists), off the map, or
/// standing at the last tile.
fn arrive(state: &mut GameState, defs: &Defs, id: WalkerId, events: &mut Vec<SimEvent>) {
    let now = state.clock.tick;
    let Some(m) = motion_of(state, id) else { return };
    let dest = m.dest;
    let pos = m.pos;
    match dest {
        Some(Destination::MapEdge(_)) => {
            forget(state, id);
            if let WalkerId::Traveler(t) = id {
                state.population.walking.travelers.remove(t);
            }
            return;
        }
        Some(Destination::Building { building, purpose }) => {
            let at_door = state.buildings.get(building).is_some_and(|b| SubPos::center(b.door()) == pos);
            if at_door {
                state.population.walking.index.remove(id, pos);
                if let Some(m) = motion_mut(state, id) {
                    m.inside = Some(building);
                }
                if let WalkerId::Citizen(c) = id
                    && let Some(household) = state.population.citizens.get(c).map(|c| c.household)
                {
                    match purpose {
                        // Errands: the household restocks its pantry here (at the end of the step).
                        Purpose::Errand => state.population.errands_arrived.push((household, building)),
                        // Guests buy what the venue sells (tavern: beer), also at the end of the step.
                        Purpose::Leisure => state.population.guests_arrived.push((household, building)),
                        _ => {}
                    }
                }
            }
        }
        Some(Destination::Spot { .. } | Destination::Tile(_)) | None => {}
    }
    if let Some(m) = motion_mut(state, id)
        && m.commuting
        && m.has_arrived(m.dest)
    {
        // The first arrival at work (workplace or work spot) ends the commute.
        m.commuting = false;
        m.last_commute = u32::try_from(now.saturating_sub(m.departed_at)).unwrap_or(u32::MAX).max(1);
    }
    if let Some(m) = motion_mut(state, id) {
        m.path.clear();
        m.next = 0;
        m.progress = 0;
        m.frac = 0;
        m.phase = TripPhase::Still;
        m.replan = false;
        if !m.has_arrived(m.dest) {
            // The destination changed, or its door moved or vanished on the way: try again
            // from here.
            m.phase = TripPhase::Queued;
            state.population.walking.requests.push_back(id);
        }
    }
    if let WalkerId::Citizen(c) = id {
        super::routine::refresh_activity(state, defs, c);
    }
    let _ = events;
}

/// Consistency of positions, trips, travellers and the request queue (loaded states): positions
/// on the map and at most one step from the previous tick's, a walking path that matches the
/// position, buildings walkers are inside exist, visible walkers stand on enterable tiles (or
/// on a building's footprint: doors, or something built under their feet), and the queue lists
/// exactly the walkers waiting for a path.
pub(crate) fn validate(state: &GameState, defs: &Defs) -> Result<(), String> {
    let (w, h) = (state.world.width() * SUBTILE, state.world.height() * SUBTILE);
    let in_map = |p: SubPos| p.x >= 0 && p.y >= 0 && p.x < w && p.y < h;
    let walking = &state.population.walking;
    let check = |id: WalkerId, m: &Motion| -> Result<(), String> {
        if !in_map(m.pos) || !in_map(m.prev) {
            return Err(format!("{id:?} is outside the map"));
        }
        if (m.pos.x - m.prev.x).abs() + (m.pos.y - m.prev.y).abs() > SUBTILE {
            return Err(format!("{id:?} moved more than a tile in one tick"));
        }
        if m.frac >= 1000 || i32::from(m.progress) >= SUBTILE || usize::from(m.next) > m.path.len() {
            return Err(format!("{id:?} has a malformed walking state"));
        }
        if let Some(b) = m.inside
            && state.buildings.get(b).is_none()
        {
            return Err(format!("{id:?} is inside a missing building"));
        }
        match m.phase {
            TripPhase::Walking => {
                let Some(d) = m.path.get(usize::from(m.next)) else {
                    return Err(format!("{id:?} walks without a path"));
                };
                if m.inside.is_some() || !m.pos.offset(*d, -i32::from(m.progress)).is_center() {
                    return Err(format!("{id:?} is off its path"));
                }
            }
            TripPhase::Still | TripPhase::Queued | TripPhase::NoPath { .. } => {
                if m.progress != 0 || !m.path.is_empty() || (m.inside.is_none() && !m.pos.is_center()) {
                    return Err(format!("{id:?} stands between tiles"));
                }
            }
        }
        let tile = m.pos.tile();
        if m.inside.is_none()
            && state.buildings.at(tile).is_none()
            && !navigation::tile_enterable(&state.world, &state.buildings, &state.roads, &state.diplomacy.land, defs, tile)
        {
            return Err(format!("{id:?} stands on an impassable tile {tile}"));
        }
        let queued = walking.requests.iter().filter(|r| **r == id).count();
        if queued != usize::from(m.phase == TripPhase::Queued) {
            return Err(format!("{id:?} is listed {queued} times in the path queue"));
        }
        Ok(())
    };
    for (id, c) in state.population.citizens.iter() {
        check(WalkerId::Citizen(id), &c.motion)?;
    }
    walking.travelers.check_integrity().map_err(|e| format!("traveller arena: {e}"))?;
    for (id, t) in walking.travelers.iter() {
        check(WalkerId::Traveler(id), &t.motion)?;
        let edge = |p: IVec2| p.x == 0 || p.y == 0 || p.x == state.world.width() - 1 || p.y == state.world.height() - 1;
        let to_edge = matches!(t.motion.dest, Some(Destination::MapEdge(p)) if edge(p) && state.world.in_bounds(p));
        // Caravan walkers also walk to the trading post or wait for their cart; enemies march
        // to tiles or stand.
        let caravan_trip = t.caravan.is_some() && matches!(t.motion.dest, None | Some(Destination::Spot { .. }));
        let enemy = t.warband.is_some() && matches!(t.motion.dest, None | Some(Destination::Tile(_)));
        if !(to_edge || caravan_trip || enemy) || t.speed_permille == 0 || t.motion.inside.is_some() {
            return Err(format!("traveller {id:?} is not heading for the map edge"));
        }
    }
    for id in &walking.requests {
        let exists = match id {
            WalkerId::Citizen(c) => state.population.citizens.contains(*c),
            WalkerId::Traveler(t) => walking.travelers.contains(*t),
        };
        if !exists {
            return Err(format!("path queue lists missing walker {id:?}"));
        }
    }
    if walking.head_progress > 0 && (walking.requests.is_empty() || walking.head_progress > defs.rules().walking.max_search_nodes)
    {
        return Err("path search progress without a matching request".into());
    }
    walking.no_path_notices.validate(state.clock.tick, "no-path notices")?;
    Ok(())
}
