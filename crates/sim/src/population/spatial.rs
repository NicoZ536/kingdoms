//! What renderers see of walkers: a spatial index by chunk (updated incrementally as walkers
//! cross tiles, enter and leave buildings) and [`CitizenView`]s with the sprite role of the
//! `characters` atlas (docs/assets.md).

use glam::IVec2;
use serde::{Deserialize, Serialize};

use super::movement::{Motion, SubPos, WalkerId};
use super::{Citizen, CitizenId, LifeStage, Sex};
use crate::defs::{Defs, JobCategory, ResourceId, ServiceKind};
use crate::grid::{Direction, TileRect};
use crate::state::GameState;
use crate::world::CHUNK_SIZE;

/// Sprite role of a walker (`characters` atlas: `<role>_<dir>_<frame>`, docs/assets.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CitizenRole {
    /// Adult or elder man without a role below.
    Man,
    /// Adult or elder woman without a role below.
    Woman,
    /// Child.
    Child,
    /// Storage worker (carrier).
    Carrier,
    /// Builder (construction sites and upgrades).
    Builder,
    /// Worker of a field or pasture farm.
    Farmer,
    /// Worker of a military building; foot soldier (militia, swordsman, spearman) of the
    /// player or of an AI army.
    Soldier,
    /// Worker of a market or trading post; the merchant of a trade caravan.
    Merchant,
    /// The cart (with a donkey) of a trade caravan.
    Cart,
    /// Archer or crossbowman.
    Archer,
    /// Rider (cavalry).
    Knight,
    /// Siege engine (ram or catapult) and its crew.
    Siege,
    /// Bandit of a raiding band.
    Raider,
}

impl CitizenRole {
    /// Role name in the sprite names of the `characters` atlas.
    pub const fn sprite_key(self) -> &'static str {
        match self {
            CitizenRole::Man => "man",
            CitizenRole::Woman => "woman",
            CitizenRole::Child => "child",
            CitizenRole::Carrier => "carrier",
            CitizenRole::Builder => "builder",
            CitizenRole::Farmer => "farmer",
            CitizenRole::Soldier => "soldier",
            CitizenRole::Merchant => "merchant",
            CitizenRole::Cart => "cart",
            CitizenRole::Archer => "archer",
            CitizenRole::Knight => "knight",
            CitizenRole::Siege => "siege",
            CitizenRole::Raider => "raider",
        }
    }

    /// The role of soldiers of a unit class.
    pub const fn of_class(class: crate::defs::UnitClass) -> CitizenRole {
        match class {
            crate::defs::UnitClass::Infantry => CitizenRole::Soldier,
            crate::defs::UnitClass::Ranged => CitizenRole::Archer,
            crate::defs::UnitClass::Cavalry => CitizenRole::Knight,
            crate::defs::UnitClass::Siege => CitizenRole::Siege,
        }
    }
}

/// Sprite role of a citizen: children; anybody on a transport except gatherers bringing their
/// yield back is a carrier; otherwise by job (builders, carriers, farmers, soldiers, merchants),
/// else man or woman.
pub fn role_of(state: &GameState, defs: &Defs, citizen: &Citizen) -> CitizenRole {
    if citizen.stage == LifeStage::Child {
        return CitizenRole::Child;
    }
    if let Some(unit) = citizen.soldier {
        return crate::military::unit_role(defs, unit);
    }
    let transport = citizen.carry.and_then(|t| state.logistics.job(t.job)).map(|j| j.kind);
    if transport.is_some_and(|k| k != crate::logistics::TransportKind::Haul) {
        return CitizenRole::Carrier;
    }
    let by_sex = if citizen.sex == Sex::Female { CitizenRole::Woman } else { CitizenRole::Man };
    let Some(job) = citizen.job else { return by_sex };
    if job.category == JobCategory::Construction {
        return CitizenRole::Builder;
    }
    let Some(def) = state.buildings.get(job.building).and_then(|b| defs.buildings().get(b.kind().index())) else {
        return by_sex;
    };
    if def.storage.is_some() {
        CitizenRole::Carrier
    } else if crate::agriculture::farm_spec(def).is_some() {
        CitizenRole::Farmer
    } else if job.category == JobCategory::Military {
        CitizenRole::Soldier
    } else if def.role == crate::defs::BuildingRole::TradeHub || def.services.iter().any(|s| s.kind == ServiceKind::Market) {
        CitizenRole::Merchant
    } else {
        by_sex
    }
}

/// A walker as renderers see it. Positions are in 1/256 tile ([`SubPos`]); interpolate
/// between `prev_pos` and `pos` with the frame's tick fraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CitizenView {
    /// Stable id (citizen, or traveller: emigrant leaving the kingdom, caravan merchant or cart).
    pub id: WalkerId,
    /// Current position.
    pub pos: SubPos,
    /// Position one tick ago.
    pub prev_pos: SubPos,
    /// Facing (sprite direction: [`Direction::sprite_dir`]); while attacking (`action`
    /// [`WalkerAction::Attack`]) turned towards the target of the blow or shot.
    pub facing: Direction,
    /// Sprite role.
    pub role: CitizenRole,
    /// True while following a path (animate the walk cycle).
    pub moving: bool,
    /// False while inside a building (do not draw).
    pub visible: bool,
    /// Goods carried (carriers between pick-up and delivery, gatherers bringing their yield
    /// back, the cart of a trade caravan with its largest load): resource and units.
    pub carried: Option<(ResourceId, u32)>,
    /// True for enemies of the player (bandits, soldiers of AI armies): renderers mark them.
    pub hostile: bool,
    /// What it visibly does while standing (work and attack animations).
    pub action: WalkerAction,
}

/// What a walker visibly does (for animations; the [`CitizenRole`] tells the tool).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WalkerAction {
    /// Walking, waiting, idle or anything not shown with a tool.
    #[default]
    None,
    /// Working at an outdoor spot during the shift: builders at a construction site, farmers
    /// on their field, gatherers at a tree, rock, clay pit or shore.
    Work,
    /// Struck or shot at an enemy in the latest combat round
    /// ([`crate::military::Military::last_round`]).
    Attack,
}

impl CitizenView {
    pub(crate) fn of(id: WalkerId, motion: &Motion, role: CitizenRole, carried: Option<(ResourceId, u32)>) -> CitizenView {
        CitizenView {
            id,
            pos: motion.pos,
            prev_pos: motion.prev,
            facing: motion.facing,
            role,
            moving: motion.is_moving(),
            visible: motion.is_visible(),
            carried,
            hostile: false,
            action: WalkerAction::None,
        }
    }
}

/// Where walker `id` aimed if it struck, shot or hit a building in the current combat round.
fn attack_target(state: &GameState, defs: &Defs, id: WalkerId) -> Option<TileRect> {
    let round = state.military.last_round();
    if !round.is_current(state.clock.tick, defs.rules().military.round_ticks) {
        return None;
    }
    round.target_of(id)
}

/// Facing from tile `from` towards the nearest tile of `target` along the dominant axis
/// (ties: the x axis; `None` if `from` lies inside `target`).
fn facing_towards(from: IVec2, target: TileRect) -> Option<Direction> {
    let d = from.clamp(target.origin, target.max_inclusive()) - from;
    if d == IVec2::ZERO {
        None
    } else if d.x.abs() >= d.y.abs() {
        Some(if d.x > 0 { Direction::East } else { Direction::West })
    } else {
        Some(if d.y > 0 { Direction::South } else { Direction::North })
    }
}

/// Action of a walker that attacked in the current combat round, turned towards its target.
fn attack_action(state: &GameState, defs: &Defs, id: WalkerId, motion: &Motion) -> Option<(WalkerAction, Direction)> {
    let target = attack_target(state, defs, id)?;
    let facing = facing_towards(motion.pos.tile(), target).unwrap_or(motion.facing);
    Some((WalkerAction::Attack, facing))
}

/// Action of a citizen standing on the map (with its facing): attacking in the current combat
/// round (turned towards the target), or working on its own work spot during the shift (the
/// stored activity, refreshed with the routine).
fn citizen_action(state: &GameState, defs: &Defs, id: CitizenId, c: &Citizen) -> (WalkerAction, Direction) {
    let idle = (WalkerAction::None, c.motion.facing);
    if !c.motion.is_visible() || c.motion.is_moving() {
        return idle;
    }
    if let Some(attack) = attack_action(state, defs, WalkerId::Citizen(id), &c.motion) {
        return attack;
    }
    let on_spot = matches!(c.motion.dest, Some(super::Destination::Spot { building, tile })
        if c.job.is_some_and(|j| j.building == building) && c.motion.pos.tile() == tile);
    if on_spot && c.activity == super::Activity::Working && c.carry.is_none() {
        (WalkerAction::Work, c.motion.facing)
    } else {
        idle
    }
}

/// Visible walkers per chunk ([`CHUNK_SIZE`] tiles square), each list sorted by id. Derived
/// data: rebuilt after loading, updated incrementally otherwise.
#[derive(Clone, Debug, Default)]
pub(crate) struct SpatialIndex {
    chunks_x: i32,
    chunks_y: i32,
    cells: Vec<Vec<WalkerId>>,
}

impl SpatialIndex {
    pub(crate) fn new(width: i32, height: i32) -> Self {
        let chunks_x = (width.max(1) + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let chunks_y = (height.max(1) + CHUNK_SIZE - 1) / CHUNK_SIZE;
        Self { chunks_x, chunks_y, cells: vec![Vec::new(); (chunks_x * chunks_y) as usize] }
    }

    /// True if the index was built for a `width` × `height` map.
    pub(crate) fn fits(&self, width: i32, height: i32) -> bool {
        let expected = |edge: i32| (edge.max(1) + CHUNK_SIZE - 1) / CHUNK_SIZE;
        !self.cells.is_empty() && self.chunks_x == expected(width) && self.chunks_y == expected(height)
    }

    fn cell(&self, pos: SubPos) -> Option<usize> {
        let c = pos.tile().div_euclid(IVec2::splat(CHUNK_SIZE));
        (c.x >= 0 && c.y >= 0 && c.x < self.chunks_x && c.y < self.chunks_y).then(|| (c.y * self.chunks_x + c.x) as usize)
    }

    pub(crate) fn insert(&mut self, id: WalkerId, pos: SubPos) {
        let Some(i) = self.cell(pos) else { return };
        let list = &mut self.cells[i];
        if let Err(at) = list.binary_search(&id) {
            list.insert(at, id);
        }
    }

    pub(crate) fn remove(&mut self, id: WalkerId, pos: SubPos) {
        let Some(i) = self.cell(pos) else { return };
        let list = &mut self.cells[i];
        if let Ok(at) = list.binary_search(&id) {
            list.remove(at);
        }
    }

    pub(crate) fn moved(&mut self, id: WalkerId, from: SubPos, to: SubPos) {
        if self.cell(from) != self.cell(to) {
            self.remove(id, from);
            self.insert(id, to);
        }
    }

    /// Ids in the chunks overlapping `rect` (chunk by chunk, row-major; sorted within a chunk).
    pub(crate) fn in_rect(&self, rect: TileRect) -> impl Iterator<Item = WalkerId> + '_ {
        let max = rect.max_inclusive();
        let lo = rect.origin.div_euclid(IVec2::splat(CHUNK_SIZE)).max(IVec2::ZERO);
        let hi = max.div_euclid(IVec2::splat(CHUNK_SIZE)).min(IVec2::new(self.chunks_x - 1, self.chunks_y - 1));
        let empty = rect.size.x <= 0 || rect.size.y <= 0 || self.cells.is_empty();
        let (lo, hi) = if empty { (IVec2::ONE, IVec2::ZERO) } else { (lo, hi) };
        (lo.y..=hi.y)
            .flat_map(move |cy| (lo.x..=hi.x).map(move |cx| (cy * self.chunks_x + cx) as usize))
            .flat_map(move |i| self.cells[i].iter().copied())
    }

    /// Number of indexed walkers.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.cells.iter().map(Vec::len).sum()
    }
}

impl PartialEq for SpatialIndex {
    fn eq(&self, other: &Self) -> bool {
        self.chunks_x == other.chunks_x && self.chunks_y == other.chunks_y && self.cells == other.cells
    }
}

/// View of one walker (`None` if it does not exist).
pub fn walker_view(state: &GameState, defs: &Defs, id: WalkerId) -> Option<CitizenView> {
    match id {
        WalkerId::Citizen(c) => citizen_view(state, defs, c),
        WalkerId::Traveler(t) => state.population.walking.travelers.get(t).map(|t| {
            // A caravan's cart shows its largest load.
            let carried = match (t.role, t.caravan) {
                (CitizenRole::Cart, Some(c)) => state.diplomacy.caravan(c).and_then(|c| c.main_good()),
                _ => None,
            };
            let attack = if !t.motion.is_moving() && t.warband.is_some() { attack_action(state, defs, id, &t.motion) } else { None };
            let (action, facing) = attack.unwrap_or((WalkerAction::None, t.motion.facing));
            CitizenView { hostile: t.warband.is_some(), action, facing, ..CitizenView::of(id, &t.motion, t.role, carried) }
        }),
    }
}

/// View of one citizen (also while inside a building: `visible` is false, `pos` is the door).
pub fn citizen_view(state: &GameState, defs: &Defs, id: CitizenId) -> Option<CitizenView> {
    let c = state.population.citizens.get(id)?;
    let carried = c.carried().map(|m| (m.resource, m.amount));
    let (action, facing) = citizen_action(state, defs, id, c);
    Some(CitizenView { action, facing, ..CitizenView::of(WalkerId::Citizen(id), &c.motion, role_of(state, defs, c), carried) })
}

/// Visible walkers whose tile lies in `rect` (citizens and travellers on the map; citizens
/// inside buildings are not listed). Cost: the walkers in the chunks overlapping `rect`, so
/// O(visible) for a screen-sized rectangle. Order: chunk by chunk (row-major), by id within a
/// chunk. Before the first tick of a game assembled by hand, all walkers are scanned instead.
pub fn citizens_in<'a>(state: &'a GameState, defs: &'a Defs, rect: TileRect) -> Box<dyn Iterator<Item = CitizenView> + 'a> {
    let walking = &state.population.walking;
    let view = move |id: WalkerId| walker_view(state, defs, id).filter(|v| v.visible && rect.contains(v.pos.tile()));
    if walking.index.fits(state.world.width(), state.world.height()) {
        Box::new(walking.index.in_rect(rect).filter_map(view))
    } else {
        let citizens = state.population.citizens.iter().map(|(id, _)| WalkerId::Citizen(id));
        let travelers = walking.travelers.iter().map(|(id, _)| WalkerId::Traveler(id));
        Box::new(citizens.chain(travelers).filter_map(view))
    }
}
