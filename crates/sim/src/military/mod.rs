//! Military and war (docs/game_design.md §11): soldiers, companies and their orders, threats
//! (bandits, armies of AI kingdoms), combat on the map, campaigns and sieges against kingdoms.
//!
//! ## Soldiers
//! A soldier is a real adult citizen ([`crate::population::Citizen::is_soldier`]): recruited at
//! a military building ([`recruit`]) he leaves his civilian job (the workforce shrinks), stays a
//! member of his household and lives at home, trains inside the building for the unit's
//! training time and then serves: on duty during the day (at night only when enemies threaten),
//! paid every day (ledger „Militär“), fed by his household (at war additional rations from the
//! storages), with health, morale, equipment and experience ([`Soldier`]). Siege engines need
//! a crew: their operator is the soldier, the other citizens are crew who follow him.
//!
//! ## Companies and orders
//! Ready soldiers are grouped in companies ([`Company`]) with an [`Order`]: stand at their
//! posts (station), defend a building (garrison) or an area, patrol a route of tiles, march on
//! a campaign or a siege against a kingdom at war, or retreat. Unassigned soldiers stand at the
//! building they were trained at. The duty of every soldier (where he should be) is derived
//! every combat round (module `orders`); the daily routine follows it.
//!
//! ## Threats and combat
//! Bandits of the raid event and armies of kingdoms at war ([`Warband`]) gather off the map
//! (their arrival time is known), walk in from the map edge as walkers and go for their target:
//! unguarded storages (plunder, arson) or the headquarters (conquest). Every
//! `military.round_ticks` hostile groups that meet fight a deterministic combat round
//! (module `combat`): ranged shots first (soldiers, towers, archer posts and the castle), then
//! melee; defence bonuses for forest, height, walls and garrisons; research bonuses; morale and
//! retreat. Casualties are real deaths; damaged buildings become ruins, broken walls and gates
//! vanish. An army that holds the broken headquarters conquers the kingdom: the game is lost
//! ([`crate::ranks`]).
//!
//! ## Campaigns
//! A company on a campaign gathers at the map edge towards the kingdom, travels off the map and
//! fights an abstract battle against the kingdom's field army; with siege engines it can then
//! besiege the seat and force a peace with ceded land and reparations (module `campaign`).
//!
//! The module has its own `ChaCha8Rng` so that combat does not shift other random numbers.

mod campaign;
mod combat;
pub(crate) mod orders;
pub mod recruit;
mod threats;
mod upkeep;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use glam::IVec2;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::defs::{Defs, ResourceAmount, UnitClass, UnitTypeId};
use crate::diplomacy::KingdomId;
use crate::event::SimEvent;
use crate::grid::TileRect;
use crate::population::{CitizenId, CitizenRole, Destination, TravelerId, WalkerId};
use crate::state::GameState;

pub use campaign::{CampaignStage, campaign_problem};
pub(crate) use campaign::rehome_away;
pub use combat::{fighter_stats, unit_strength};
pub use orders::{order_problem, order_text};
pub use recruit::{RecruitProblem, recruit_problem};
pub(crate) use threats::{dispatch_army, spawn_bandits};
pub use threats::{bandit_arrival, bandit_band, raid_defenders, raid_grace_over, ranged_defences, warband_eta};
pub(crate) use upkeep::daily_pay_milli as upkeep_daily_pay_milli;

/// Stream constant mixed into the seed for the military RNG.
const MILITARY_RNG_STREAM: u64 = 0x4d49_4c49_5441_5259; // "MILITARY"

/// Battle reports kept in the history.
pub const BATTLE_HISTORY: usize = 32;

/// Training or serving.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoldierStage {
    /// Training inside `building`; `done_milli` thousandths of training ticks are done (a
    /// tick inside counts 1000 with full trainers, less without).
    Training {
        /// The training building.
        building: BuildingId,
        /// Training progress in thousandths of a tick.
        done_milli: u64,
    },
    /// Trained and serving.
    Ready,
}

/// A citizen serving as a soldier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Soldier {
    pub(crate) unit: UnitTypeId,
    pub(crate) stage: SoldierStage,
    pub(crate) company: Option<u32>,
    /// Where the soldier stands without other orders: the building he was trained at.
    pub(crate) post: Option<BuildingId>,
    /// Health in points (0 ..= the unit's hit points).
    pub(crate) hp: u16,
    /// Morale 0–100.
    pub(crate) morale: u16,
    /// Equipment in permille (1000 = fully equipped).
    pub(crate) equipment: u16,
    /// Experience 0–100.
    pub(crate) experience: u16,
    /// Enemies killed.
    pub(crate) kills: u16,
    /// Morale points remembered from the last fights (fade daily).
    pub(crate) battle_morale: i16,
    /// The siege engine operator this citizen serves as crew for.
    pub(crate) crew_of: Option<CitizenId>,
    /// Where the soldier should be now (derived every combat round; `None`: off duty, the
    /// normal daily routine applies).
    pub(crate) duty: Option<Destination>,
    /// The enemy the soldier fights or chases.
    pub(crate) engage: Option<TravelerId>,
    /// Index of the next patrol waypoint.
    pub(crate) waypoint: u16,
    /// Campaign the soldier is away on (off the map, hidden).
    pub(crate) away: Option<u32>,
    /// Tick of recruitment.
    pub(crate) recruited_tick: u64,
}

impl Soldier {
    /// Unit type.
    pub fn unit(&self) -> UnitTypeId {
        self.unit
    }

    /// Training or ready.
    pub fn stage(&self) -> SoldierStage {
        self.stage
    }

    /// True once trained.
    pub fn is_ready(&self) -> bool {
        self.stage == SoldierStage::Ready
    }

    /// Company, if assigned.
    pub fn company(&self) -> Option<u32> {
        self.company
    }

    /// The building the soldier stands at without other orders.
    pub fn post(&self) -> Option<BuildingId> {
        self.post
    }

    /// Health in points.
    pub fn hp(&self) -> u16 {
        self.hp
    }

    /// Morale 0–100.
    pub fn morale(&self) -> u16 {
        self.morale
    }

    /// Equipment in permille.
    pub fn equipment_permille(&self) -> u16 {
        self.equipment
    }

    /// Experience 0–100.
    pub fn experience(&self) -> u16 {
        self.experience
    }

    /// Enemies killed.
    pub fn kills(&self) -> u16 {
        self.kills
    }

    /// The siege engine this citizen serves as crew for.
    pub fn crew_of(&self) -> Option<CitizenId> {
        self.crew_of
    }

    /// Where the soldier should be now (`None`: off duty).
    pub fn duty(&self) -> Option<Destination> {
        self.duty
    }

    /// Campaign the soldier is away on.
    pub fn away(&self) -> Option<u32> {
        self.away
    }

    /// True for fighting soldiers (not crew).
    pub fn is_combatant(&self) -> bool {
        self.crew_of.is_none()
    }

    /// True while trained, on the map (not away) and fighting (not crew).
    pub fn is_available(&self) -> bool {
        self.is_ready() && self.away.is_none() && self.crew_of.is_none()
    }
}

/// Allowed radius in tiles of an area to defend ([`DefendTarget::Area`]).
pub const DEFEND_RADIUS: std::ops::RangeInclusive<u8> = 1..=12;

/// Target of a defence order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DefendTarget {
    /// Garrison a building (inside up to its garrison capacity, the others around it).
    Building(BuildingId),
    /// Hold an area: stand around `center` within `radius` tiles.
    Area {
        /// Centre tile.
        center: IVec2,
        /// Radius in tiles ([`DEFEND_RADIUS`]).
        radius: u8,
    },
}

/// Order of a company.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Order {
    /// Stand at the posts (the buildings the soldiers were trained at).
    Station,
    /// Verteidigen: garrison a building or hold an area.
    Defend(DefendTarget),
    /// Patrouillieren: walk the route of tiles in a loop.
    Patrol {
        /// Waypoints (walkable tiles), 2 ..= `military.max_patrol_points`.
        route: Vec<IVec2>,
    },
    /// Feldzug (`siege: false`) or Belagerung (`siege: true`) against a kingdom at war.
    Campaign {
        /// The enemy.
        kingdom: KingdomId,
        /// Besiege the seat after the field battle (needs siege engines).
        siege: bool,
    },
    /// Rückzug: back to the posts (from a campaign: turn back).
    Retreat,
}

impl Order {
    /// German name of the order kind.
    pub fn display_name(&self) -> &'static str {
        match self {
            Order::Station => "Bereitschaft",
            Order::Defend(DefendTarget::Building(_)) => "Verteidigen (Gebäude)",
            Order::Defend(DefendTarget::Area { .. }) => "Verteidigen (Gebiet)",
            Order::Patrol { .. } => "Patrouillieren",
            Order::Campaign { siege: false, .. } => "Feldzug",
            Order::Campaign { siege: true, .. } => "Belagerung",
            Order::Retreat => "Rückzug",
        }
    }
}

/// A company of soldiers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Company {
    pub(crate) id: u32,
    pub(crate) name: String,
    /// Members (soldiers, not crew), sorted.
    pub(crate) members: Vec<CitizenId>,
    pub(crate) order: Order,
}

impl Company {
    /// Id.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// German name („1. Kompanie“).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Members.
    pub fn members(&self) -> &[CitizenId] {
        &self.members
    }

    /// Current order.
    pub fn order(&self) -> &Order {
        &self.order
    }
}

/// Who an enemy group is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarbandKind {
    /// A band of bandits (raid event).
    Bandits,
    /// The army of a kingdom at war with the player; `conquest`: it marches on the
    /// headquarters (otherwise it plunders).
    Army {
        /// The kingdom.
        kingdom: KingdomId,
        /// True if it wants to conquer the headquarters.
        conquest: bool,
    },
}

/// Stage of an enemy group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarbandStage {
    /// Gathering off the map; arrives at the map edge at `arrive_tick`.
    Approaching {
        /// Arrival tick.
        arrive_tick: u64,
    },
    /// On the map, going for its target.
    Attacking,
    /// Running back to the map edge (beaten, done plundering, or without a way to its target).
    Fleeing,
    /// Leaving the map after its kingdom made peace: it fights no more and passes the gates.
    Withdrawing,
}

impl WarbandStage {
    /// True while the group is a threat on the map (attacking or fleeing; not before it
    /// arrives and not while it withdraws after a peace).
    pub fn is_hostile(self) -> bool {
        matches!(self, WarbandStage::Attacking | WarbandStage::Fleeing)
    }
}

/// What a fighter of an enemy group is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FighterKind {
    /// A bandit (`military.bandits`).
    Bandit,
    /// A soldier of a unit type.
    Unit(UnitTypeId),
}

/// One enemy on the map (a [`crate::population::Traveler`] with the warband's id).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fighter {
    pub(crate) walker: TravelerId,
    pub(crate) kind: FighterKind,
    pub(crate) hp: u16,
    /// Building it attacks (gate, wall, headquarters).
    pub(crate) attacking: Option<BuildingId>,
    /// Player soldier it fights or chases.
    pub(crate) engage: Option<CitizenId>,
    /// Plunder it carries.
    pub(crate) loot: Vec<ResourceAmount>,
    /// Gold it carries.
    pub(crate) gold: u32,
}

impl Fighter {
    /// The walker on the map.
    pub fn walker(&self) -> TravelerId {
        self.walker
    }

    /// Bandit or unit type.
    pub fn kind(&self) -> FighterKind {
        self.kind
    }

    /// Health in points.
    pub fn hp(&self) -> u16 {
        self.hp
    }

    /// Plunder carried.
    pub fn loot(&self) -> &[ResourceAmount] {
        &self.loot
    }

    /// Gold carried.
    pub fn gold(&self) -> u32 {
        self.gold
    }
}

/// Counters of one engagement (bandit raid, army attack, field battle, siege) for its report.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    /// Own soldiers that fought.
    pub own_units: u32,
    /// Own soldiers killed.
    pub own_losses: u32,
    /// Enemies killed.
    pub enemy_losses: u32,
    /// Own buildings damaged or destroyed.
    pub buildings_damaged: u32,
    /// Goods carried off.
    pub goods_lost: Vec<ResourceAmount>,
    /// Gold carried off.
    pub gold_lost: u32,
    /// Plunder taken back from fallen enemies.
    pub recovered: u32,
}

/// An enemy group: bandits or an AI army.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warband {
    pub(crate) id: u32,
    pub(crate) kind: WarbandKind,
    pub(crate) stage: WarbandStage,
    /// Map-edge tile it enters and leaves through.
    pub(crate) entry: IVec2,
    /// Composition (per kind) before it arrives, sorted.
    pub(crate) planned: Vec<(FighterKind, u16)>,
    /// Fighters on the map.
    pub(crate) fighters: Vec<Fighter>,
    /// Building it goes for (storage to plunder, the headquarters).
    pub(crate) target: Option<BuildingId>,
    /// Size at arrival.
    pub(crate) size: u16,
    /// Fighters that left the map alive.
    pub(crate) escaped: u16,
    /// True once it plundered.
    pub(crate) plundered: bool,
    /// Combat rounds spent plundering at the target so far.
    pub(crate) plunder_rounds: u16,
    /// Ticks it has held the broken headquarters without defenders.
    pub(crate) hold_ticks: u32,
    /// Ticks a withdrawing army has found no way off the map (walled in without a gate).
    pub(crate) stuck_ticks: u32,
    /// Wall or gate it breaks through when no path leads to its target, and the tile it
    /// attacks from.
    pub(crate) breach: Option<(BuildingId, IVec2)>,
    /// Own soldiers who fought it, sorted.
    pub(crate) defenders: Vec<CitizenId>,
    pub(crate) started_tick: u64,
    pub(crate) tally: Tally,
}

impl Warband {
    /// Id.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Bandits or army.
    pub fn kind(&self) -> WarbandKind {
        self.kind
    }

    /// Stage.
    pub fn stage(&self) -> WarbandStage {
        self.stage
    }

    /// Map-edge tile it enters through.
    pub fn entry(&self) -> IVec2 {
        self.entry
    }

    /// Fighters on the map.
    pub fn fighters(&self) -> &[Fighter] {
        &self.fighters
    }

    /// Composition before arrival.
    pub fn planned(&self) -> &[(FighterKind, u16)] {
        &self.planned
    }

    /// Its target building.
    pub fn target(&self) -> Option<BuildingId> {
        self.target
    }

    /// Fighters it has now (planned before arrival, alive on the map afterwards).
    pub fn strength(&self) -> u16 {
        match self.stage {
            WarbandStage::Approaching { .. } => self.planned.iter().map(|(_, n)| *n).sum(),
            _ => u16::try_from(self.fighters.len()).unwrap_or(u16::MAX),
        }
    }

    /// The kingdom of an army.
    pub fn kingdom(&self) -> Option<KingdomId> {
        match self.kind {
            WarbandKind::Army { kingdom, .. } => Some(kingdom),
            WarbandKind::Bandits => None,
        }
    }

    /// True once it fought, damaged or plundered anything (a group without that gets no
    /// battle report when it withdraws after a peace).
    pub fn fought(&self) -> bool {
        let t = &self.tally;
        !self.defenders.is_empty() || self.plundered || t.own_losses > 0 || t.enemy_losses > 0 || t.buildings_damaged > 0
    }
}

/// A campaign or siege of one company against a kingdom.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Campaign {
    pub(crate) id: u32,
    pub(crate) company: u32,
    pub(crate) kingdom: KingdomId,
    pub(crate) siege: bool,
    pub(crate) stage: CampaignStage,
    /// Map-edge tile towards the kingdom.
    pub(crate) entry: IVec2,
    /// Soldiers (and crew) taking part, sorted; at the start the company with its crews.
    pub(crate) soldiers: Vec<CitizenId>,
    pub(crate) started_tick: u64,
    pub(crate) tally: Tally,
    /// Food taken from the storages as provisions at departure (whole units by resource, in
    /// the order taken).
    pub(crate) provisions: Vec<ResourceAmount>,
    /// Soldier-hours spent off the map so far: every hour each soldier (and crew member) away
    /// eats `war_ration_milli / 24` of the provisions; the rest comes home.
    pub(crate) soldier_hours: u64,
}

impl Campaign {
    /// Id.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The company.
    pub fn company(&self) -> u32 {
        self.company
    }

    /// The enemy kingdom.
    pub fn kingdom(&self) -> KingdomId {
        self.kingdom
    }

    /// True for a siege.
    pub fn is_siege(&self) -> bool {
        self.siege
    }

    /// Stage.
    pub fn stage(&self) -> CampaignStage {
        self.stage
    }

    /// Soldiers taking part.
    pub fn soldiers(&self) -> &[CitizenId] {
        &self.soldiers
    }

    /// Map-edge tile towards the kingdom.
    pub fn entry(&self) -> IVec2 {
        self.entry
    }

    /// Food carried as provisions (taken at departure).
    pub fn provisions(&self) -> &[ResourceAmount] {
        &self.provisions
    }

    /// Units of the provisions eaten so far (`war_ration_milli` per soldier and day away).
    pub fn provisions_eaten(&self, defs: &Defs) -> u32 {
        let eaten = self.soldier_hours.saturating_mul(u64::from(defs.rules().military.war_ration_milli)).div_ceil(24 * 1000);
        u32::try_from(eaten).unwrap_or(u32::MAX)
    }
}

/// Kind of a battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BattleKind {
    /// Defence against bandits.
    Raid,
    /// Defence against an AI army.
    Defence,
    /// A field battle of a campaign.
    FieldBattle,
    /// A siege of a kingdom's seat.
    Siege,
}

impl BattleKind {
    /// German name.
    pub const fn display_name(self) -> &'static str {
        match self {
            BattleKind::Raid => "Räuberüberfall",
            BattleKind::Defence => "Verteidigung",
            BattleKind::FieldBattle => "Feldschlacht",
            BattleKind::Siege => "Belagerung",
        }
    }
}

/// Outcome of a battle for the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BattleOutcome {
    /// The enemy was beaten or driven off without loot.
    Victory,
    /// The enemy got what it wanted, or the player's soldiers were beaten.
    Defeat,
    /// Both sides lost and nobody won.
    Draw,
}

impl BattleOutcome {
    /// German name.
    pub const fn display_name(self) -> &'static str {
        match self {
            BattleOutcome::Victory => "Sieg",
            BattleOutcome::Defeat => "Niederlage",
            BattleOutcome::Draw => "Unentschieden",
        }
    }
}

/// A finished battle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleReport {
    /// Serial number.
    pub serial: u32,
    /// Kind.
    pub kind: BattleKind,
    /// Tick the enemy arrived or the battle began.
    pub start_tick: u64,
    /// Tick it ended.
    pub end_tick: u64,
    /// Where it happened (map tile; campaigns: the map edge towards the kingdom).
    pub position: IVec2,
    /// The kingdom fought (`None`: bandits).
    pub kingdom: Option<KingdomId>,
    /// German name of the enemy („Räuberbande“, „Heer von Eschenwald“).
    pub enemy: String,
    /// Enemies at the start.
    pub enemy_units: u32,
    /// Counters.
    pub tally: Tally,
    /// Outcome for the player.
    pub outcome: BattleOutcome,
    /// German summary.
    pub text: String,
}

/// What a projectile of a combat round is ([`Shot`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotKind {
    /// An arrow or bolt: archers, crossbowmen, towers, archer posts and the castle.
    Arrow,
    /// A stone thrown by a siege engine.
    Stone,
}

/// One ranged hit of the last combat round, for drawing the projectile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shot {
    /// Where the shooter stood (a soldier's tile, the building a tower or garrison shoots from).
    pub from: TileRect,
    /// Where the target stood.
    pub to: TileRect,
    pub kind: ShotKind,
    /// Shot by the player (towers, soldiers) – otherwise by an enemy.
    pub player: bool,
    /// The shooting walker (`None`: a tower, archer post or castle shooting by itself).
    pub shooter: Option<WalkerId>,
}

/// What a melee blow of a combat round hits ([`Strike`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrikeTarget {
    /// A soldier or an enemy fighter.
    Walker(WalkerId),
    /// A building under attack (gate, wall, headquarters, storage) – `tile` is its tile next to
    /// the attacker.
    Building { building: BuildingId, tile: IVec2 },
}

/// One melee blow of the last combat round: attacker and target, for turning the attack loop
/// towards the target and placing sparks between the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strike {
    /// The striking walker.
    pub attacker: WalkerId,
    /// Where the attacker stood (inside a garrison building: the building's area).
    pub from: TileRect,
    /// What was hit.
    pub target: StrikeTarget,
    /// Where the target stood (a walker's tile, a garrison building's area, the struck tile of
    /// a building).
    pub to: TileRect,
    /// Struck by the player's soldier – otherwise by an enemy.
    pub player: bool,
}

/// The last combat round on the map – presentation data only: who shot at whom and who struck
/// whom or which building. Rebuilt by every round, not saved (empty after loading), never read
/// by the simulation itself and not part of the state comparison.
#[derive(Clone, Debug, Default)]
pub struct CombatRound {
    tick: u64,
    shots: Vec<Shot>,
    strikes: Vec<Strike>,
    strikers: BTreeSet<WalkerId>,
}

impl CombatRound {
    /// Tick of the round (0: none yet).
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Ranged hits in the order they were fired.
    pub fn shots(&self) -> &[Shot] {
        &self.shots
    }

    /// Melee blows in the order they were struck: against walkers first (player before
    /// enemies, by id), then the blows of enemies against the buildings they attack.
    pub fn strikes(&self) -> &[Strike] {
        &self.strikes
    }

    /// True if walker `id` struck, shot at an enemy or hit a building in this round.
    pub fn struck(&self, id: WalkerId) -> bool {
        self.strikers.contains(&id)
    }

    /// Where walker `id` aimed in this round: the target of its first melee blow, else of its
    /// first shot (`None` if it did not attack).
    pub fn target_of(&self, id: WalkerId) -> Option<TileRect> {
        if !self.strikers.contains(&id) {
            return None;
        }
        self.strikes
            .iter()
            .find(|s| s.attacker == id)
            .map(|s| s.to)
            .or_else(|| self.shots.iter().find(|s| s.shooter == Some(id)).map(|s| s.to))
    }

    /// True if the round happened within the last `round_ticks` ticks before `now` (it is
    /// still the latest picture of the fighting).
    pub fn is_current(&self, now: u64, round_ticks: u16) -> bool {
        self.tick > 0 && now.saturating_sub(self.tick) < u64::from(round_ticks.max(1))
    }
}

/// The whole military state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Military {
    pub(crate) soldiers: BTreeMap<CitizenId, Soldier>,
    /// Companies in id order.
    pub(crate) companies: Vec<Company>,
    pub(crate) next_company: u32,
    /// Enemy groups in id order.
    pub(crate) warbands: Vec<Warband>,
    pub(crate) next_warband: u32,
    /// Campaigns in id order.
    pub(crate) campaigns: Vec<Campaign>,
    pub(crate) next_campaign: u32,
    /// Finished battles, oldest first (at most [`BATTLE_HISTORY`]).
    pub(crate) battles: VecDeque<BattleReport>,
    pub(crate) next_battle: u32,
    /// Damage of buildings under attack (walls, gates, headquarters, others), by building.
    pub(crate) damage: BTreeMap<BuildingId, u32>,
    /// Every ready soldier hunts the enemies on the map until this tick (event alarm).
    pub(crate) alarm_until: u64,
    /// Ready soldiers gather at the headquarters until this tick (event muster).
    pub(crate) muster_until: u64,
    /// The pay could not be paid at the last midnight (bankrupt treasury).
    pub(crate) unpaid: bool,
    pub(crate) rng: ChaCha8Rng,
    /// The last combat round for the renderer ([`CombatRound`]; not saved).
    #[serde(skip)]
    pub(crate) last_round: CombatRound,
}

impl Default for Military {
    fn default() -> Self {
        Military::new(0)
    }
}

impl PartialEq for Military {
    fn eq(&self, other: &Self) -> bool {
        self.soldiers == other.soldiers
            && self.companies == other.companies
            && self.next_company == other.next_company
            && self.warbands == other.warbands
            && self.next_warband == other.next_warband
            && self.campaigns == other.campaigns
            && self.next_campaign == other.next_campaign
            && self.battles == other.battles
            && self.next_battle == other.next_battle
            && self.damage == other.damage
            && self.alarm_until == other.alarm_until
            && self.muster_until == other.muster_until
            && self.unpaid == other.unpaid
            && self.rng == other.rng
    }
}

impl Military {
    /// An empty military with its RNG seeded from the game seed.
    pub fn new(seed: u64) -> Self {
        Self {
            soldiers: BTreeMap::new(),
            companies: Vec::new(),
            next_company: 1,
            warbands: Vec::new(),
            next_warband: 1,
            campaigns: Vec::new(),
            next_campaign: 1,
            battles: VecDeque::new(),
            next_battle: 1,
            damage: BTreeMap::new(),
            alarm_until: 0,
            muster_until: 0,
            unpaid: false,
            rng: ChaCha8Rng::seed_from_u64(seed ^ MILITARY_RNG_STREAM),
            last_round: CombatRound::default(),
        }
    }

    /// The last combat round on the map (shots and who struck; presentation only).
    pub fn last_round(&self) -> &CombatRound {
        &self.last_round
    }

    /// True if citizen `id` serves as a soldier.
    pub fn is_soldier(&self, id: CitizenId) -> bool {
        self.soldiers.contains_key(&id)
    }

    /// A soldier by citizen id.
    pub fn soldier(&self, id: CitizenId) -> Option<&Soldier> {
        self.soldiers.get(&id)
    }

    /// All soldiers (and crew) in citizen id order.
    pub fn soldiers(&self) -> impl Iterator<Item = (CitizenId, &Soldier)> + '_ {
        self.soldiers.iter().map(|(id, s)| (*id, s))
    }

    /// Fighting soldiers (without crew).
    pub fn combatants(&self) -> impl Iterator<Item = (CitizenId, &Soldier)> + '_ {
        self.soldiers().filter(|(_, s)| s.is_combatant())
    }

    /// Ready soldiers on the map (event requirements count these).
    pub fn available_count(&self) -> u32 {
        u32::try_from(self.soldiers.values().filter(|s| s.is_available()).count()).unwrap_or(u32::MAX)
    }

    /// Companies in id order.
    pub fn companies(&self) -> &[Company] {
        &self.companies
    }

    /// A company by id.
    pub fn company(&self, id: u32) -> Option<&Company> {
        self.companies.iter().find(|c| c.id == id)
    }

    /// Enemy groups (approaching, on the map, fleeing) in id order.
    pub fn warbands(&self) -> &[Warband] {
        &self.warbands
    }

    /// An enemy group by id.
    pub fn warband(&self, id: u32) -> Option<&Warband> {
        self.warbands.iter().find(|w| w.id == id)
    }

    /// Campaigns in id order.
    pub fn campaigns(&self) -> &[Campaign] {
        &self.campaigns
    }

    /// A campaign by id.
    pub fn campaign(&self, id: u32) -> Option<&Campaign> {
        self.campaigns.iter().find(|c| c.id == id)
    }

    /// Finished battles, newest first.
    pub fn battles(&self) -> impl DoubleEndedIterator<Item = &BattleReport> + ExactSizeIterator + '_ {
        self.battles.iter().rev()
    }

    /// Damage of a building under attack (0 if none).
    pub fn damage_of(&self, building: BuildingId) -> u32 {
        self.damage.get(&building).copied().unwrap_or(0)
    }

    /// True while enemies are on the map.
    pub fn enemies_on_map(&self) -> bool {
        self.warbands.iter().any(|w| w.stage.is_hostile() && !w.fighters.is_empty())
    }

    /// True while the pay could not be paid.
    pub fn is_unpaid(&self) -> bool {
        self.unpaid
    }

    /// Tick until which the event alarm lasts (0 = never).
    pub fn alarm_until(&self) -> u64 {
        self.alarm_until
    }

    /// Tick until which the soldiers gather at the headquarters (0 = never).
    pub fn muster_until(&self) -> u64 {
        self.muster_until
    }

    pub(crate) fn company_mut(&mut self, id: u32) -> Option<&mut Company> {
        self.companies.iter_mut().find(|c| c.id == id)
    }

    /// The fighter whose walker is `walker`, with its warband's index.
    pub(crate) fn fighter_index(&self, walker: TravelerId) -> Option<(usize, usize)> {
        self.warbands
            .iter()
            .enumerate()
            .find_map(|(wi, w)| w.fighters.iter().position(|f| f.walker == walker).map(|fi| (wi, fi)))
    }

    /// Stores a finished battle (the history keeps the newest [`BATTLE_HISTORY`]).
    pub(crate) fn record_battle(&mut self, mut report: BattleReport) -> u32 {
        report.serial = self.next_battle;
        self.next_battle = self.next_battle.wrapping_add(1).max(1);
        let serial = report.serial;
        self.battles.push_back(report);
        while self.battles.len() > BATTLE_HISTORY {
            self.battles.pop_front();
        }
        serial
    }

    /// Structural consistency against the population, buildings and data.
    pub(crate) fn validate(&self, state: &GameState, defs: &Defs) -> Result<(), String> {
        for (id, s) in &self.soldiers {
            let c = state.population.citizen(*id).ok_or_else(|| format!("soldier {id:?} is no citizen"))?;
            if c.soldier != Some(s.unit) || c.job().is_some() {
                return Err(format!("soldier {id:?} is not marked as one or has a civilian job"));
            }
            let unit = defs.units().get(s.unit.index()).ok_or_else(|| format!("soldier {id:?} has an unknown unit"))?;
            if s.hp > unit.hit_points || s.morale > 100 || s.equipment > 1000 || s.experience > 100 {
                return Err(format!("soldier {id:?} has values out of range"));
            }
            if let SoldierStage::Training { building, done_milli } = s.stage {
                if state.buildings.get(building).is_none() || done_milli >= unit.training_ticks.max(1) * 1000 {
                    return Err(format!("soldier {id:?} trains at a missing building or is done"));
                }
                if s.company.is_some() || s.away.is_some() {
                    return Err(format!("soldier {id:?} in training belongs to a company"));
                }
            }
            if let Some(c) = s.company {
                let listed = self.company(c).is_some_and(|co| co.members.binary_search(id).is_ok());
                if !listed || s.crew_of.is_some() {
                    return Err(format!("soldier {id:?} names company {c}, which does not list him"));
                }
            }
            if let Some(lead) = s.crew_of
                && self.soldiers.get(&lead).is_none_or(|l| l.crew_of.is_some())
            {
                return Err(format!("crew {id:?} serves a missing engine"));
            }
            if let Some(a) = s.away
                && self.campaign(a).is_none_or(|c| c.soldiers.binary_search(id).is_err())
            {
                return Err(format!("soldier {id:?} is away on campaign {a}, which does not list him"));
            }
        }
        if self.companies.windows(2).any(|w| w[0].id >= w[1].id) || self.companies.iter().any(|c| c.id >= self.next_company) {
            return Err("companies out of order".into());
        }
        let max = usize::from(defs.rules().military.max_company_size);
        for c in &self.companies {
            if c.members.windows(2).any(|w| w[0] >= w[1]) || c.members.len() > max {
                return Err(format!("company {} has malformed members", c.id));
            }
            if c.members.iter().any(|m| self.soldiers.get(m).is_none_or(|s| s.company != Some(c.id))) {
                return Err(format!("company {} lists a soldier who does not name it", c.id));
            }
            if let Order::Patrol { route } = &c.order
                && (route.len() < 2 || route.iter().any(|p| !state.world.in_bounds(*p)))
            {
                return Err(format!("company {} has a malformed patrol route", c.id));
            }
            if let Order::Defend(DefendTarget::Area { center, radius }) = c.order
                && (!state.world.in_bounds(center) || !DEFEND_RADIUS.contains(&radius))
            {
                return Err(format!("company {} defends an area outside the map or with a radius out of range", c.id));
            }
        }
        if self.warbands.windows(2).any(|w| w[0].id >= w[1].id) || self.warbands.iter().any(|w| w.id >= self.next_warband) {
            return Err("warbands out of order".into());
        }
        for w in &self.warbands {
            if !state.world.in_bounds(w.entry) {
                return Err(format!("warband {} enters outside the map", w.id));
            }
            if matches!(w.stage, WarbandStage::Approaching { .. }) != w.fighters.is_empty()
                && !matches!(w.stage, WarbandStage::Fleeing | WarbandStage::Withdrawing)
            {
                return Err(format!("warband {} has fighters that do not match its stage", w.id));
            }
            if let Some(k) = w.kingdom()
                && state.diplomacy.kingdom(k).is_none()
            {
                return Err(format!("warband {} belongs to an unknown kingdom", w.id));
            }
            for f in &w.fighters {
                let t = state
                    .population
                    .walking
                    .traveler(f.walker)
                    .ok_or_else(|| format!("fighter {:?} of warband {} has no walker", f.walker, w.id))?;
                if t.warband != Some(w.id) || f.hp == 0 {
                    return Err(format!("fighter {:?} does not belong to warband {}", f.walker, w.id));
                }
                if let FighterKind::Unit(u) = f.kind
                    && u.index() >= defs.units().len()
                {
                    return Err(format!("fighter {:?} has an unknown unit", f.walker));
                }
            }
        }
        for (tid, t) in state.population.walking.travelers() {
            if let Some(w) = t.warband
                && self.warband(w).is_none_or(|wb| !wb.fighters.iter().any(|f| f.walker == tid))
            {
                return Err(format!("walker {tid:?} names warband {w}, which does not list it"));
            }
        }
        if self.campaigns.windows(2).any(|w| w[0].id >= w[1].id) || self.campaigns.iter().any(|c| c.id >= self.next_campaign) {
            return Err("campaigns out of order".into());
        }
        for c in &self.campaigns {
            if self.company(c.company).is_none() || state.diplomacy.kingdom(c.kingdom).is_none() {
                return Err(format!("campaign {} has an unknown company or kingdom", c.id));
            }
            if c.soldiers.windows(2).any(|w| w[0] >= w[1]) || c.soldiers.iter().any(|s| !self.soldiers.contains_key(s)) {
                return Err(format!("campaign {} lists unknown soldiers", c.id));
            }
            if c.provisions.iter().any(|m| m.amount == 0 || m.resource.index() >= defs.resources().len()) {
                return Err(format!("campaign {} carries malformed provisions", c.id));
            }
            // At most one hour per soldier (a company with its crews stays far below u16::MAX)
            // and hour since the campaign started.
            let hours = state.clock.tick.saturating_sub(c.started_tick) / crate::time::TICKS_PER_HOUR + 1;
            if c.started_tick > state.clock.tick || c.soldier_hours > hours.saturating_mul(u64::from(u16::MAX)) {
                return Err(format!("campaign {} has impossible soldier-hours", c.id));
            }
        }
        if self.battles.len() > BATTLE_HISTORY {
            return Err("battle history too long".into());
        }
        if self.damage.keys().any(|b| state.buildings.get(*b).is_none()) {
            return Err("damage recorded for a missing building".into());
        }
        Ok(())
    }
}

/// Removes a citizen from the military (death, emigration): its company, its crew (who go
/// back to civilian life) and campaigns.
pub(crate) fn forget_citizen(state: &mut GameState, id: CitizenId) {
    let Some(s) = state.military.soldiers.remove(&id) else { return };
    if let Some(c) = s.company.and_then(|c| state.military.company_mut(c)) {
        c.members.retain(|m| *m != id);
    }
    for camp in &mut state.military.campaigns {
        camp.soldiers.retain(|m| *m != id);
    }
    let crew: Vec<CitizenId> =
        state.military.soldiers.iter().filter(|(_, c)| c.crew_of == Some(id)).map(|(cid, _)| *cid).collect();
    for c in crew {
        recruit::discharge(state, c);
    }
    for w in &mut state.military.warbands {
        for f in &mut w.fighters {
            if f.engage == Some(id) {
                f.engage = None;
            }
        }
    }
    if let Some(c) = state.population.citizens.get_mut(id) {
        c.soldier = None;
    }
}

/// Walking speed of soldier `id` in permille of a citizen's (the unit's speed; crew walk with
/// their engine).
pub(crate) fn speed_permille(state: &GameState, defs: &Defs, id: CitizenId) -> u16 {
    let Some(s) = state.military.soldiers.get(&id) else { return 1000 };
    let unit = s.crew_of.and_then(|lead| state.military.soldiers.get(&lead)).map_or(s.unit, |l| l.unit);
    defs.units().get(unit.index()).map_or(1000, |u| u.speed_permille)
}

/// Sprite role of soldiers of a unit type.
pub fn unit_role(defs: &Defs, unit: UnitTypeId) -> CitizenRole {
    defs.units().get(unit.index()).map_or(CitizenRole::Soldier, |u| CitizenRole::of_class(u.class))
}

/// German name of a unit type.
pub(crate) fn unit_name(defs: &Defs, unit: UnitTypeId) -> String {
    defs.units().get(unit.index()).map_or_else(|| unit.to_string(), |u| u.name.clone())
}

/// „1 Bogenschütze“ / „6 Bogenschützen“: a number of units of a type with the right German
/// form.
pub fn unit_count_text(defs: &Defs, unit: UnitTypeId, n: u32) -> String {
    match defs.units().get(unit.index()) {
        Some(u) => count_text(n, &u.name, &u.plural),
        None => format!("{n} {unit}"),
    }
}

/// German name of an enemy kind.
pub fn fighter_name(defs: &Defs, kind: FighterKind) -> String {
    match kind {
        FighterKind::Bandit => "Räuber".to_owned(),
        FighterKind::Unit(u) => unit_name(defs, u),
    }
}

/// „1 Räuber“ / „3 Speerträger“ / „2 Bogenschützen“: a number of enemies of a kind.
pub fn fighter_count_text(defs: &Defs, kind: FighterKind, n: u32) -> String {
    match kind {
        FighterKind::Bandit => count_text(n, "Räuber", "Räuber"),
        FighterKind::Unit(u) => unit_count_text(defs, u, n),
    }
}

/// Class of an enemy kind (bandits fight like infantry).
pub fn fighter_class(defs: &Defs, kind: FighterKind) -> UnitClass {
    match kind {
        FighterKind::Bandit => UnitClass::Infantry,
        FighterKind::Unit(u) => defs.units().get(u.index()).map_or(UnitClass::Infantry, |d| d.class),
    }
}

/// Chebyshev distance of two tiles.
pub(crate) fn tile_distance(a: IVec2, b: IVec2) -> i32 {
    (a - b).abs().max_element()
}

/// Strength of the player's military for diplomacy (§10): ready soldiers weighted by their unit
/// (attack + defense + ranged) × `war.strength_per_soldier` / 12 — an average soldier counts
/// like a soldier of an AI kingdom.
pub fn strength(state: &GameState, defs: &Defs) -> u32 {
    let per = u64::from(defs.rules().diplomacy.war.strength_per_soldier);
    let power: u64 = state
        .military
        .soldiers
        .values()
        .filter(|s| s.is_ready() && s.crew_of.is_none())
        .map(|s| u64::from(unit_strength(defs, s.unit)))
        .sum();
    u32::try_from(power * per / 12).unwrap_or(u32::MAX)
}

/// True while the soldiers are on alert day and night (enemies on the map or arriving within
/// the hour, or an event alarm).
pub fn alert_active(state: &GameState) -> bool {
    alert(state)
}

/// See [`alert_active`].
pub(crate) fn alert(state: &GameState) -> bool {
    let now = state.clock.tick;
    now < state.military.alarm_until
        || state.military.warbands.iter().any(|w| match w.stage {
            WarbandStage::Approaching { arrive_tick } => arrive_tick <= now + crate::time::TICKS_PER_HOUR,
            WarbandStage::Attacking => true,
            WarbandStage::Fleeing => !w.fighters.is_empty(),
            WarbandStage::Withdrawing => false,
        })
}

/// Pipeline step 9 (military part): midnight upkeep (pay status, morale, rations, refits,
/// discharges), hourly healing; every tick training; enemy groups arriving; every
/// `military.round_ticks` while enemies are on the map: duties and engagement, enemy decisions,
/// a combat round and the end of beaten or departed groups; campaigns; the duties of the
/// soldiers in quiet times every round as well.
pub(crate) fn run_military(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let clock = state.clock;
    if clock.is_new_day() {
        upkeep::run_day(state, defs, events);
    }
    if clock.is_new_hour() {
        upkeep::run_hour(state, defs, events);
    }
    recruit::run_training(state, defs, events);
    threats::run_arrivals(state, defs, events);
    // Fighters who walked off the map this tick (movement runs before this step) leave their
    // group at once, so the state at the end of every tick is consistent.
    threats::prune(state, defs, events);
    let round = u64::from(defs.rules().military.round_ticks.max(1));
    if clock.tick % round == 0 {
        let on_map = state.military.warbands.iter().any(|w| !matches!(w.stage, WarbandStage::Approaching { .. }) && !w.fighters.is_empty());
        if on_map {
            // Groups withdrawing after a peace only walk off the map; the others fight.
            threats::decide(state, defs, events);
            if state.military.enemies_on_map() {
                combat::run_round(state, defs, events);
            }
            threats::prune(state, defs, events);
        }
        orders::assign_duties(state, defs);
        threats::finish(state, defs, events);
    }
    campaign::run(state, defs, events);
}

/// `DebugSpawnThreat`: bandits (`kingdom: None`) or an army of a kingdom at war arrive at the
/// map edge within the hour.
pub(crate) fn debug_spawn_threat(
    state: &mut GameState,
    defs: &Defs,
    kingdom: Option<KingdomId>,
    size: u16,
    events: &mut Vec<SimEvent>,
) -> Result<(), String> {
    if !(1..=120).contains(&size) {
        return Err("Es können 1 bis 120 Feinde auf einmal kommen.".to_owned());
    }
    let arrive = state.clock.tick + crate::time::TICKS_PER_HOUR;
    match kingdom {
        None => {
            if threats::spawn_band(state, defs, size, crate::time::TICKS_PER_HOUR, events) == 0 {
                return Err("Kein Kartenrand ist erreichbar.".to_owned());
            }
        }
        Some(k) => {
            let Some(kk) = state.diplomacy.kingdom(k) else { return Err("Dieses Königreich gibt es nicht.".to_owned()) };
            if !kk.at_war() {
                return Err(format!("Mit {} herrscht kein Krieg.", kk.name));
            }
            if let Some(kk) = state.diplomacy.kingdom_mut(k) {
                kk.soldiers += u32::from(size);
            }
            if !threats::send_army(state, defs, k, u32::from(size), arrive, events) {
                return Err("Kein Kartenrand ist erreichbar.".to_owned());
            }
        }
    }
    Ok(())
}

/// Re-applies balancing values after loading with changed data: health above a lowered
/// maximum is cut.
pub(crate) fn rederive(state: &mut GameState, defs: &Defs) {
    for s in state.military.soldiers.values_mut() {
        if let Some(u) = defs.units().get(s.unit.index()) {
            s.hp = s.hp.min(u.hit_points);
        }
    }
}

/// Plural-aware German count: „1 Soldat“ / „3 Soldaten“.
pub(crate) fn count_text(n: u32, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// „Gefallen: 1 eigener Soldat, 3 feindliche Soldaten.“ (`kein` for none).
pub(crate) fn fallen_text(own: u32, enemy: u32) -> String {
    let side = |n: u32, one: &str, many: &str| match n {
        0 => format!("kein {one} Soldat"),
        1 => format!("1 {one} Soldat"),
        n => format!("{n} {many} Soldaten"),
    };
    format!("Gefallen: {}, {}.", side(own, "eigener", "eigene"), side(enemy, "feindlicher", "feindliche"))
}

/// Test helper: `count` adults (unemployed first) become ready soldiers of `unit` posted at
/// `post`.
#[cfg(test)]
pub(crate) fn test_enlist(state: &mut GameState, defs: &Defs, count: usize, unit: UnitTypeId, post: BuildingId) -> Vec<CitizenId> {
    let def = defs.unit(unit);
    let ids: Vec<CitizenId> = recruit::candidates(state, post).into_iter().take(count).collect();
    for id in &ids {
        crate::population::release_job(state, *id);
        if let Some(c) = state.population.citizens.get_mut(*id) {
            c.soldier = Some(unit);
        }
        state.military.soldiers.insert(
            *id,
            Soldier {
                unit,
                stage: SoldierStage::Ready,
                company: None,
                post: Some(post),
                hp: def.hit_points,
                morale: def.morale,
                equipment: 1000,
                experience: 0,
                kills: 0,
                battle_morale: 0,
                crew_of: None,
                duty: None,
                engage: None,
                waypoint: 0,
                away: None,
                recruited_tick: state.clock.tick,
            },
        );
    }
    state.workforce.mark_dirty();
    state.workforce.refresh_counts(&state.population);
    ids
}
