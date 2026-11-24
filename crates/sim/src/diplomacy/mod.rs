//! Other kingdoms, diplomacy and trade caravans (docs/game_design.md §7, §10). Pipeline step 9
//! (after research, before events).
//!
//! ## Kingdoms
//! 3–5 AI kingdoms (by map size, `rules.ron` → `diplomacy.kingdoms`, at most the scenario's
//! `rival_kingdoms`) sit at the generator's settlement sites far from the player ([`setup`]).
//! Each owns the tiles within its territory radius around its seat ([`LandMap`]; the player
//! cannot build there), grows its population from food, its radius from its population and
//! its settlement — a list of [`Structure`]s using the sprites and footprints of building types
//! — from its town plan. Its abstract economy ([`economy`]) produces from the land it holds
//! (farmland, pastures, forest, water, deposits) and crafts from its people, consumes per
//! inhabitant, and so wants some goods and offers others at prices from supply and demand.
//!
//! ## Relations
//! Relations to the player are the sum of the personality's attitude, treaties, war, common
//! enemies, shared borders and remembered actions that fade daily ([`relations`]); they decide
//! the state (Feindlich < −50 ≤ Rivalisierend < −10 ≤ Neutral < 30 ≤ Freundschaftlich, Verbündet
//! only by alliance). AI kingdoms also hold relations to each other, quarrel at borders and wage
//! abstract wars.
//!
//! ## Diplomacy
//! The player acts with [`DiplomaticAction`]s (treaties, gifts, demands, war, peace); every
//! answer is a deterministic judgement with German reasons ([`actions`]). The kingdoms act on
//! their own once a day ([`initiatives`]): proposals, gifts, demands, offers to buy or sell,
//! border incidents, war declarations after a grace period, peace offers. What needs the
//! player's answer waits in the diplomatic inbox ([`Proposal`]) until it expires.
//!
//! ## Caravans
//! Merchants of kingdoms with access walk from the map edge in the direction of their kingdom
//! to the player's trading post, trade by the player's standing orders and walk back; the
//! player sends own caravans with chosen goods ([`caravans`]). Goods and gold on the way belong
//! to the caravan; robbers on dangerous routes hand them to their own kingdom, so nothing is
//! created or lost.

pub mod actions;
pub mod caravans;
pub mod economy;
pub mod initiatives;
mod land;
pub mod relations;
pub mod setup;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::VecDeque;
use std::fmt;

use glam::IVec2;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::defs::{BuildingTypeId, Defs, Personality, ResourceAmount, ResourceId};
use crate::event::SimEvent;
use crate::grid::{Rotation, TileRect};
use crate::state::GameState;

pub use caravans::{Caravan, CaravanKind, CaravanStage, RouteTally, trade_road_bonus_at};
pub use land::{LandMap, LandMapError};

/// Stream constant mixed into the seed for the diplomacy RNG.
const DIPLOMACY_RNG_STREAM: u64 = 0x4449_504c_4f4d_4143; // "DIPLOMAC"

/// Diplomatic records kept for the history.
pub const HISTORY_LENGTH: usize = 64;

/// Handle of an AI kingdom (its index; kingdoms are never removed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct KingdomId(u8);

impl KingdomId {
    pub(crate) const fn from_raw(raw: u8) -> KingdomId {
        KingdomId(raw)
    }

    pub(crate) const fn raw(self) -> u8 {
        self.0
    }

    /// Index into [`Diplomacy::kingdoms`].
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The id of the kingdom at `index` (`None` beyond the id range).
    pub fn from_index(index: usize) -> Option<KingdomId> {
        u8::try_from(index).ok().filter(|i| *i < 7).map(KingdomId)
    }
}

impl fmt::Display for KingdomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "kingdom#{}", self.0)
    }
}

/// Diplomatic state from the relation value (docs/game_design.md §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelationState {
    /// Below −50.
    Hostile,
    /// −50 to −11.
    Rival,
    /// −10 to 29.
    Neutral,
    /// 30 and more (70 and more without an alliance stays friendly).
    Friendly,
    /// With an alliance treaty.
    Allied,
}

impl RelationState {
    /// The state of relation `value`; `allied` if an alliance holds.
    pub fn of(value: i32, allied: bool) -> RelationState {
        if allied {
            RelationState::Allied
        } else if value < -50 {
            RelationState::Hostile
        } else if value < -10 {
            RelationState::Rival
        } else if value < 30 {
            RelationState::Neutral
        } else {
            RelationState::Friendly
        }
    }

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            RelationState::Hostile => "Feindlich",
            RelationState::Rival => "Rivalisierend",
            RelationState::Neutral => "Neutral",
            RelationState::Friendly => "Freundschaftlich",
            RelationState::Allied => "Verbündet",
        }
    }
}

/// Kinds of treaties with the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TreatyKind {
    /// Handelsvertrag: access, better prices, more caravans.
    TradeAgreement,
    /// Nichtangriffspakt: no war while it holds.
    NonAggression,
    /// Bündnis: allied state, best prices, allies stand by each other in wars.
    Alliance,
}

impl TreatyKind {
    /// All kinds.
    pub const ALL: [TreatyKind; 3] = [TreatyKind::TradeAgreement, TreatyKind::NonAggression, TreatyKind::Alliance];

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            TreatyKind::TradeAgreement => "Handelsvertrag",
            TreatyKind::NonAggression => "Nichtangriffspakt",
            TreatyKind::Alliance => "Bündnis",
        }
    }

    /// German name with the definite article in the accusative („den Handelsvertrag“).
    pub const fn accusative(self) -> &'static str {
        match self {
            TreatyKind::TradeAgreement => "den Handelsvertrag",
            TreatyKind::NonAggression => "den Nichtangriffspakt",
            TreatyKind::Alliance => "das Bündnis",
        }
    }

    /// Duration in days.
    pub fn days(self, defs: &Defs) -> u16 {
        let t = &defs.rules().diplomacy.treaties;
        match self {
            TreatyKind::TradeAgreement => t.trade_days,
            TreatyKind::NonAggression => t.pact_days,
            TreatyKind::Alliance => t.alliance_days,
        }
    }
}

/// A treaty in force.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Treaty {
    /// Kind.
    pub kind: TreatyKind,
    /// Tick it was signed.
    pub since_tick: u64,
    /// First tick it no longer holds.
    pub until_tick: u64,
}

/// What a kingdom remembers of the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MemoryCause {
    /// Gifts received.
    Gift,
    /// Caravans that traded.
    Trade,
    /// Tribute paid on demand.
    TributePaid,
    /// Demands the player refused.
    DemandRefused,
    /// Proposals and offers the player declined.
    ProposalDeclined,
    /// Tribute the player demanded.
    Demanded,
    /// The player declared war on the kingdom.
    WarDeclared,
    /// The player attacked a kingdom this one is friendly with.
    WarOnFriend,
    /// Peace was made.
    PeaceMade,
    /// Border incidents.
    BorderIncident,
    /// The player cancelled a treaty with this kingdom.
    TreatyBroken,
    /// The player cancelled a treaty with another kingdom.
    BrokenWord,
    /// Set by a developer command.
    Debug,
    /// The player beat the kingdom's soldiers (armies, field battles; docs/game_design.md §11).
    BattleLost,
    /// Envoys were guests at the player's festivals (docs/game_design.md §7).
    Festival,
}

impl MemoryCause {
    /// German label for the relation breakdown.
    pub const fn label(self) -> &'static str {
        match self {
            MemoryCause::Gift => "Geschenke",
            MemoryCause::Trade => "Handel mit Karawanen",
            MemoryCause::TributePaid => "Gezahlter Tribut",
            MemoryCause::DemandRefused => "Abgelehnte Forderungen",
            MemoryCause::ProposalDeclined => "Abgelehnte Vorschläge",
            MemoryCause::Demanded => "Eure Forderungen",
            MemoryCause::WarDeclared => "Kriegserklärung",
            MemoryCause::WarOnFriend => "Angriff auf ein befreundetes Reich",
            MemoryCause::PeaceMade => "Friedensschluss",
            MemoryCause::BorderIncident => "Grenzzwischenfälle",
            MemoryCause::TreatyBroken => "Vertragsbruch",
            MemoryCause::BrokenWord => "Wortbruch gegenüber anderen",
            MemoryCause::Debug => "Entwicklerwert",
            MemoryCause::BattleLost => "Verlorene Schlachten",
            MemoryCause::Festival => "Festgäste",
        }
    }
}

/// A remembered action; fades daily towards 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    /// What happened.
    pub cause: MemoryCause,
    /// Relation points in hundredths.
    pub centi: i32,
}

/// A war between the player and a kingdom (abstract until the military phase).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct War {
    /// Tick it began.
    pub since_tick: u64,
    /// True if the player declared it.
    pub declared_by_player: bool,
    /// War exhaustion of the kingdom (it offers peace from `war.exhaustion_peace`).
    pub exhaustion: u16,
    /// Raids so far.
    pub raids: u16,
    /// Day (0-based) of the next raid.
    pub next_raid_day: u64,
}

/// A war between two AI kingdoms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiWar {
    /// The attacker.
    pub attacker: KingdomId,
    /// The defender.
    pub defender: KingdomId,
    /// Tick it began.
    pub since_tick: u64,
}

/// A building of an AI settlement (drawn with the building type's sprite; walkers cannot
/// enter its footprint).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Structure {
    /// Building type (sprite, footprint).
    pub kind: BuildingTypeId,
    /// Footprint origin.
    pub origin: IVec2,
    /// Orientation.
    pub rotation: Rotation,
}

impl Structure {
    /// Occupied tiles.
    pub fn rect(&self, defs: &Defs) -> TileRect {
        let size = defs.buildings().get(self.kind.index()).map_or(IVec2::ONE, |d| d.size);
        self.rotation.footprint(self.origin, size)
    }

    /// Sprite name (`buildings/<key>`).
    pub fn sprite(&self, defs: &Defs) -> String {
        defs.buildings().get(self.kind.index()).map_or_else(|| String::from("buildings/unknown"), |d| d.sprite())
    }
}

/// An AI kingdom.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kingdom {
    pub(crate) name: String,
    pub(crate) ruler: String,
    pub(crate) title: String,
    pub(crate) personality: Personality,
    /// Colour index for the UI (0–6, distinct per kingdom).
    pub(crate) color: u8,
    pub(crate) seat: IVec2,
    pub(crate) radius: u16,
    /// Territory radius lost in wars (the radius is the grown radius minus this).
    pub(crate) ceded: u16,
    pub(crate) population: u32,
    pub(crate) treasury_milli: i64,
    /// Stock per resource in milli-units.
    pub(crate) stock_milli: Vec<u64>,
    /// Daily production per resource in milli-units (from the land survey).
    pub(crate) production_milli: Vec<u32>,
    /// The part of `production_milli` from farm and pasture land (less in winter).
    pub(crate) farm_milli: Vec<u32>,
    /// Tiles per land source in the territory ([`crate::defs::LandSource::ALL`] order).
    pub(crate) land: Vec<u32>,
    pub(crate) territory_tiles: u32,
    pub(crate) soldiers: u32,
    /// Share of the food demand met on the last day, in percent.
    pub(crate) food_percent: u8,
    pub(crate) memories: Vec<Memory>,
    pub(crate) treaties: Vec<Treaty>,
    pub(crate) war: Option<War>,
    pub(crate) structures: Vec<Structure>,
    /// Its territory touches the player's.
    pub(crate) borders_player: bool,
    /// Diplomatic state towards the player at the last check (changes are announced).
    pub(crate) last_state: RelationState,
    /// Tick from which the next caravan may set out.
    pub(crate) next_caravan_tick: u64,
    /// Day from which the next initiative may happen.
    pub(crate) next_initiative_day: u64,
    /// Day from which the next demand may happen.
    pub(crate) next_demand_day: u64,
}

impl Kingdom {
    /// Name of the realm.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Name of the ruler.
    pub fn ruler(&self) -> &str {
        &self.ruler
    }

    /// Title of the ruler.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// „König Adalbert von Eschenwald“.
    pub fn ruler_text(&self) -> String {
        format!("{} {} von {}", self.title, self.ruler, self.name)
    }

    /// Personality.
    pub fn personality(&self) -> Personality {
        self.personality
    }

    /// Colour index for the UI (0–6, distinct per kingdom).
    pub fn color(&self) -> u8 {
        self.color
    }

    /// Seat (centre of the territory).
    pub fn seat(&self) -> IVec2 {
        self.seat
    }

    /// Territory radius in tiles.
    pub fn radius(&self) -> u16 {
        self.radius
    }

    /// Inhabitants.
    pub fn population(&self) -> u32 {
        self.population
    }

    /// Treasury in whole gold.
    pub fn treasury(&self) -> i64 {
        self.treasury_milli.div_euclid(crate::economy::MILLI_PER_GOLD)
    }

    /// Units of `resource` in stock.
    pub fn stock(&self, resource: ResourceId) -> u32 {
        self.stock_milli.get(resource.index()).map_or(0, |m| u32::try_from(m / 1000).unwrap_or(u32::MAX))
    }

    /// Daily production of `resource` in milli-units (summer rate).
    pub fn production_milli(&self, resource: ResourceId) -> u32 {
        self.production_milli.get(resource.index()).copied().unwrap_or(0)
    }

    /// Soldiers.
    pub fn soldiers(&self) -> u32 {
        self.soldiers
    }

    /// Owned tiles.
    pub fn territory_tiles(&self) -> u32 {
        self.territory_tiles
    }

    /// Share of the food demand met on the last day, in percent.
    pub fn food_percent(&self) -> u8 {
        self.food_percent
    }

    /// Remembered actions.
    pub fn memories(&self) -> &[Memory] {
        &self.memories
    }

    /// Treaties in force with the player.
    pub fn treaties(&self) -> &[Treaty] {
        &self.treaties
    }

    /// The treaty of `kind` in force at `tick`.
    pub fn treaty(&self, kind: TreatyKind, tick: u64) -> Option<&Treaty> {
        self.treaties.iter().find(|t| t.kind == kind && t.until_tick > tick)
    }

    /// True if a treaty of `kind` holds at `tick`.
    pub fn has_treaty(&self, kind: TreatyKind, tick: u64) -> bool {
        self.treaty(kind, tick).is_some()
    }

    /// The war with the player.
    pub fn war(&self) -> Option<&War> {
        self.war.as_ref()
    }

    /// True while at war with the player.
    pub fn at_war(&self) -> bool {
        self.war.is_some()
    }

    /// Structures of its settlement, in building order.
    pub fn structures(&self) -> &[Structure] {
        &self.structures
    }

    /// True if its territory touches the player's.
    pub fn borders_player(&self) -> bool {
        self.borders_player
    }
}

/// A party to a war.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Party {
    /// The player's kingdom.
    Player,
    /// An AI kingdom.
    Kingdom(KingdomId),
}

/// Terms of a peace (from the player's view).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PeaceTerms {
    /// Peace without conditions.
    White,
    /// The player pays gold.
    PlayerPays {
        /// Gold.
        gold: u32,
    },
    /// The kingdom pays reparations.
    KingdomPays {
        /// Gold.
        gold: u32,
    },
    /// The kingdom draws its border back (territory radius − `war.border_ceded`).
    BorderCeded,
}

impl PeaceTerms {
    /// German description („Frieden ohne Bedingungen“).
    pub fn text(self, kingdom: &str) -> String {
        match self {
            PeaceTerms::White => "Frieden ohne Bedingungen".to_owned(),
            PeaceTerms::PlayerPays { gold } => format!("Frieden gegen {gold} Gold an {kingdom}"),
            PeaceTerms::KingdomPays { gold } => format!("Frieden gegen {gold} Gold Entschädigung von {kingdom}"),
            PeaceTerms::BorderCeded => format!("Frieden gegen Gebietsabtretung durch {kingdom}"),
        }
    }
}

/// An action of the player towards a kingdom (`Command::Diplomacy`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiplomaticAction {
    /// Proposes a treaty; the kingdom accepts or declines at once.
    ProposeTreaty {
        /// Kind.
        treaty: TreatyKind,
    },
    /// Cancels a treaty in force (relation penalty; other kingdoms remember the broken word).
    CancelTreaty {
        /// Kind.
        treaty: TreatyKind,
    },
    /// Sends gold and goods (from the kingdom stock) as a gift.
    Gift {
        /// Gold.
        gold: u32,
        /// Goods.
        goods: Vec<ResourceAmount>,
    },
    /// Demands tribute in gold.
    DemandTribute {
        /// Gold demanded.
        gold: u32,
    },
    /// Declares war (cancels the treaties with the kingdom).
    DeclareWar,
    /// Proposes peace with terms.
    ProposePeace {
        /// Terms.
        terms: PeaceTerms,
    },
}

/// What a kingdom asks the player to answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalKind {
    /// It proposes a treaty.
    Treaty(TreatyKind),
    /// It demands tribute.
    Tribute {
        /// Gold.
        gold: u32,
    },
    /// It offers peace.
    Peace {
        /// Terms.
        terms: PeaceTerms,
    },
    /// It wants to buy goods.
    Purchase {
        /// The good.
        resource: ResourceId,
        /// Units.
        amount: u32,
        /// Gold offered in total.
        gold: u32,
    },
    /// It offers goods for sale.
    Sale {
        /// The good.
        resource: ResourceId,
        /// Units.
        amount: u32,
        /// Gold asked in total.
        gold: u32,
    },
}

/// A proposal waiting in the diplomatic inbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    /// Serial number (`Command::AnswerProposal`).
    pub serial: u32,
    /// From whom.
    pub kingdom: KingdomId,
    /// What.
    pub kind: ProposalKind,
    /// Tick it arrived.
    pub created_tick: u64,
    /// First tick it counts as declined.
    pub expires_tick: u64,
}

/// A line of the diplomatic history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiplomacyRecord {
    /// When.
    pub tick: u64,
    /// The kingdom concerned (`None`: several or none).
    pub kingdom: Option<KingdomId>,
    /// A second kingdom concerned (wars between kingdoms).
    pub other: Option<KingdomId>,
    /// German text.
    pub text: String,
}

/// All diplomacy state: kingdoms, their land, relations among them, the inbox, caravans and
/// the history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Diplomacy {
    pub(crate) kingdoms: Vec<Kingdom>,
    pub(crate) land: LandMap,
    /// Relations between AI kingdoms (row-major, symmetric, diagonal 0).
    pub(crate) ai_relations: Vec<i16>,
    pub(crate) ai_wars: Vec<AiWar>,
    pub(crate) proposals: Vec<Proposal>,
    pub(crate) next_serial: u32,
    /// Tick the last proposal, demand or offer reached the inbox (pacing of the initiatives).
    pub(crate) last_proposal_tick: Option<u64>,
    pub(crate) caravans: Vec<Caravan>,
    pub(crate) next_caravan: u32,
    pub(crate) history: VecDeque<DiplomacyRecord>,
    pub(crate) rng: ChaCha8Rng,
}

impl Diplomacy {
    /// No kingdoms on a `width` × `height` map; the RNG is derived from `seed`.
    pub(crate) fn new(width: i32, height: i32, seed: u64) -> Self {
        Self {
            kingdoms: Vec::new(),
            land: LandMap::new(width, height),
            ai_relations: Vec::new(),
            ai_wars: Vec::new(),
            proposals: Vec::new(),
            next_serial: 1,
            last_proposal_tick: None,
            caravans: Vec::new(),
            next_caravan: 1,
            history: VecDeque::new(),
            rng: ChaCha8Rng::seed_from_u64(seed ^ DIPLOMACY_RNG_STREAM),
        }
    }

    /// All kingdoms in id order.
    pub fn kingdoms(&self) -> impl ExactSizeIterator<Item = (KingdomId, &Kingdom)> + '_ {
        self.kingdoms.iter().enumerate().map(|(i, k)| (KingdomId::from_raw(i as u8), k))
    }

    /// A kingdom by id.
    pub fn kingdom(&self, id: KingdomId) -> Option<&Kingdom> {
        self.kingdoms.get(id.index())
    }

    pub(crate) fn kingdom_mut(&mut self, id: KingdomId) -> Option<&mut Kingdom> {
        self.kingdoms.get_mut(id.index())
    }

    /// Number of kingdoms.
    pub fn len(&self) -> usize {
        self.kingdoms.len()
    }

    /// True without kingdoms (trade orders then use the abstract world market).
    pub fn is_empty(&self) -> bool {
        self.kingdoms.is_empty()
    }

    /// Who owns which tile, and where structures stand.
    pub fn land(&self) -> &LandMap {
        &self.land
    }

    /// Counter of border and structure changes (renderers rebuild when it changes; not saved).
    pub fn land_revision(&self) -> u64 {
        self.land.revision()
    }

    /// Chunks whose borders or structures changed since the last call (renderers; not saved).
    pub(crate) fn take_dirty_chunks(&mut self) -> Vec<IVec2> {
        self.land.changes_mut().take_chunks()
    }

    /// Relation between two AI kingdoms (0 for the same kingdom).
    pub fn ai_relation(&self, a: KingdomId, b: KingdomId) -> i32 {
        let n = self.kingdoms.len();
        if a == b || a.index() >= n || b.index() >= n {
            return 0;
        }
        i32::from(self.ai_relations[a.index() * n + b.index()])
    }

    pub(crate) fn set_ai_relation(&mut self, a: KingdomId, b: KingdomId, value: i32) {
        let n = self.kingdoms.len();
        if a == b || a.index() >= n || b.index() >= n {
            return;
        }
        let v = i16::try_from(value.clamp(-100, 100)).unwrap_or(0);
        self.ai_relations[a.index() * n + b.index()] = v;
        self.ai_relations[b.index() * n + a.index()] = v;
    }

    /// Wars between AI kingdoms.
    pub fn ai_wars(&self) -> &[AiWar] {
        &self.ai_wars
    }

    /// The war between two AI kingdoms, if any.
    pub fn ai_war_between(&self, a: KingdomId, b: KingdomId) -> Option<&AiWar> {
        self.ai_wars.iter().find(|w| (w.attacker == a && w.defender == b) || (w.attacker == b && w.defender == a))
    }

    /// Kingdoms at war with `k` (AI wars only).
    pub fn enemies_of(&self, k: KingdomId) -> impl Iterator<Item = KingdomId> + '_ {
        self.ai_wars.iter().filter_map(move |w| {
            if w.attacker == k {
                Some(w.defender)
            } else if w.defender == k {
                Some(w.attacker)
            } else {
                None
            }
        })
    }

    /// Proposals waiting for the player's answer, oldest first.
    pub fn proposals(&self) -> &[Proposal] {
        &self.proposals
    }

    /// Caravans on the way, in id order.
    pub fn caravans(&self) -> &[Caravan] {
        &self.caravans
    }

    /// A caravan by id.
    pub fn caravan(&self, id: u32) -> Option<&Caravan> {
        self.caravans.iter().find(|c| c.id == id)
    }

    /// Diplomatic history, oldest first.
    pub fn history(&self) -> impl DoubleEndedIterator<Item = &DiplomacyRecord> + ExactSizeIterator + '_ {
        self.history.iter()
    }

    /// Structures of all kingdoms whose footprint intersects `rect` (renderers), with their
    /// kingdom; kingdoms in id order, structures in building order.
    pub fn structures_in<'a>(&'a self, defs: &'a Defs, rect: TileRect) -> impl Iterator<Item = (KingdomId, &'a Structure)> + 'a {
        self.kingdoms().flat_map(move |(id, k)| {
            let reach = TileRect::new(k.seat - IVec2::splat(i32::from(k.radius) + 6), IVec2::splat(2 * i32::from(k.radius) + 13));
            let near = reach.intersects(&rect);
            k.structures.iter().filter(move |s| near && s.rect(defs).intersects(&rect)).map(move |s| (id, s))
        })
    }

    pub(crate) fn record(&mut self, tick: u64, kingdom: Option<KingdomId>, other: Option<KingdomId>, text: impl Into<String>) {
        self.history.push_back(DiplomacyRecord { tick, kingdom, other, text: text.into() });
        while self.history.len() > HISTORY_LENGTH {
            self.history.pop_front();
        }
    }

    /// Checks sizes, references and the land map against the kingdoms (loaded saves).
    pub(crate) fn validate(&self, state: &GameState, defs: &Defs) -> Result<(), String> {
        let n = self.kingdoms.len();
        if n > 7 || self.ai_relations.len() != n * n {
            return Err("malformed kingdom list or relation matrix".into());
        }
        if self.land.grid_size() != state.world.size() {
            return Err("land map size differs from the map".into());
        }
        if self.land.has_unknown_bits() || usize::from(self.land.max_owner_code()) > n {
            return Err("land map names an unknown kingdom".into());
        }
        for a in 0..n {
            if self.ai_relations[a * n + a] != 0 {
                return Err("relation of a kingdom to itself".into());
            }
            for b in 0..n {
                let v = self.ai_relations[a * n + b];
                if v != self.ai_relations[b * n + a] || !(-100..=100).contains(&v) {
                    return Err("asymmetric or out-of-range relation".into());
                }
            }
        }
        let resources = defs.resources().len();
        let mut structure_tiles: Vec<IVec2> = Vec::new();
        for (id, k) in self.kingdoms() {
            if k.stock_milli.len() != resources
                || k.production_milli.len() != resources
                || k.farm_milli.len() != resources
                || k.land.len() != crate::defs::LandSource::ALL.len()
            {
                return Err(format!("{id}: economy slots do not match the resources"));
            }
            if k.farm_milli.iter().zip(&k.production_milli).any(|(f, p)| f > p) {
                return Err(format!("{id}: farm production above the total"));
            }
            if !state.world.in_bounds(k.seat) || k.population == 0 || usize::from(k.color) >= 7 {
                return Err(format!("{id}: seat, population or colour out of range"));
            }
            if k.territory_tiles != self.land.count_owned(id) {
                return Err(format!("{id}: territory size does not match the land map"));
            }
            if k.memories.iter().enumerate().any(|(i, m)| k.memories[..i].iter().any(|o| o.cause == m.cause)) {
                return Err(format!("{id}: a cause is remembered twice"));
            }
            if k.treaties.iter().enumerate().any(|(i, t)| t.until_tick <= t.since_tick || k.treaties[..i].iter().any(|o| o.kind == t.kind))
            {
                return Err(format!("{id}: malformed treaties"));
            }
            if k.war.is_some() && !k.treaties.is_empty() {
                return Err(format!("{id}: treaties during a war"));
            }
            for s in &k.structures {
                if s.kind.index() >= defs.buildings().len() {
                    return Err(format!("{id}: structure of an unknown building type"));
                }
                let rect = s.rect(defs);
                for p in rect.tiles() {
                    if self.land.owner(p) != Some(id) {
                        return Err(format!("{id}: structure at {} outside its territory", s.origin));
                    }
                    if state.world.get(p).is_none_or(|t| t.is_water()) || state.buildings.at(p).is_some() || state.roads.has_road(p) {
                        return Err(format!("{id}: structure at {} on water, a building or a road", s.origin));
                    }
                    structure_tiles.push(p);
                }
            }
        }
        structure_tiles.sort_by_key(|p| (p.y, p.x));
        let flagged: Vec<IVec2> = self.land.structure_flags().collect();
        if flagged != structure_tiles {
            return Err("structure flags of the land map do not match the structures (or structures overlap)".into());
        }
        for p in state.territory.owned_positions() {
            if self.land.is_foreign(p) {
                return Err(format!("tile {p} is owned by the player and a kingdom"));
            }
        }
        for w in &self.ai_wars {
            if w.attacker == w.defender || w.attacker.index() >= n || w.defender.index() >= n {
                return Err("malformed war between kingdoms".into());
            }
        }
        if self.proposals.iter().any(|p| p.kingdom.index() >= n || p.serial >= self.next_serial || p.expires_tick < p.created_tick) {
            return Err("malformed proposal".into());
        }
        if self.last_proposal_tick.is_some_and(|t| t > state.clock.tick) {
            return Err("last proposal in the future".into());
        }
        if self.history.len() > HISTORY_LENGTH {
            return Err("diplomatic history too long".into());
        }
        caravans::validate(state, defs)
    }
}

/// Strength of the player's kingdom: the ready soldiers weighted by their unit
/// ([`crate::military::strength`]: an average soldier counts `war.strength_per_soldier`) plus
/// one point per `war.inhabitants_per_strength` inhabitants.
pub fn player_strength(state: &GameState, defs: &Defs) -> u32 {
    let w = &defs.rules().diplomacy.war;
    let people = u32::try_from(state.population.len()).unwrap_or(u32::MAX);
    crate::military::strength(state, defs) + people / u32::from(w.inhabitants_per_strength).max(1)
}

/// Strength of a kingdom (same formula as [`player_strength`]).
pub fn kingdom_strength(k: &Kingdom, defs: &Defs) -> u32 {
    let w = &defs.rules().diplomacy.war;
    k.soldiers.saturating_mul(u32::from(w.strength_per_soldier)) + k.population / u32::from(w.inhabitants_per_strength).max(1)
}

/// Strength of `kingdom` in percent of the player's (at least 1 % of a strength of 1).
pub fn strength_ratio_percent(state: &GameState, defs: &Defs, kingdom: &Kingdom) -> u32 {
    kingdom_strength(kingdom, defs).saturating_mul(100) / player_strength(state, defs).max(1)
}

/// True if the player may trade with `kingdom`: no war, and neutral or better or a trade
/// agreement.
pub fn has_access(state: &GameState, defs: &Defs, kingdom: KingdomId) -> bool {
    let Some(k) = state.diplomacy.kingdom(kingdom) else { return false };
    if k.at_war() {
        return false;
    }
    k.has_treaty(TreatyKind::TradeAgreement, state.clock.tick) || relations::relation(state, defs, kingdom) >= -10
}

/// Happiness points the player's citizens lose to war weariness (0 in peace).
pub fn war_weariness(state: &GameState, defs: &Defs) -> i32 {
    let w = &defs.rules().diplomacy.war;
    let tick = state.clock.tick;
    let mut wars = 0;
    let mut longest = 0;
    for (_, k) in state.diplomacy.kingdoms() {
        if let Some(war) = &k.war {
            wars += 1;
            longest = longest.max(tick.saturating_sub(war.since_tick) / crate::time::TICKS_PER_DAY);
        }
    }
    if wars == 0 {
        return 0;
    }
    let points = u64::from(w.weariness_base) + longest / u64::from(w.weariness_days_per_point).max(1) + 2 * (wars - 1);
    -i32::try_from(points.min(u64::from(w.weariness_max))).unwrap_or(0)
}

/// True while the player is at war with any kingdom.
pub fn any_player_war(state: &GameState) -> bool {
    state.diplomacy.kingdoms.iter().any(Kingdom::at_war)
}

/// True if any war is going on (the player or kingdoms among themselves; event conditions).
pub fn any_war(state: &GameState) -> bool {
    !state.diplomacy.ai_wars.is_empty() || state.diplomacy.kingdoms.iter().any(Kingdom::at_war)
}

/// Pipeline step 9 (diplomacy part): at midnight the kingdoms' economy and growth, fading
/// memories, relations and wars among kingdoms; every tick expired treaties and proposals;
/// hourly initiatives (at the initiative hour) and armies sent against the player (at the raid
/// hour, [`crate::military`]); then the caravans.
pub(crate) fn run_diplomacy(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    if state.diplomacy.kingdoms.is_empty() {
        return;
    }
    let clock = state.clock;
    if clock.is_new_day() {
        economy::run_day(state, defs, events);
        setup::grow_settlements(state, defs, events);
        relations::run_day(state, defs, events);
    }
    relations::expire_treaties(state, defs, events);
    initiatives::expire(state, defs, events);
    if clock.is_new_hour() {
        initiatives::run_hour(state, defs, events);
    }
    caravans::run(state, defs, events);
}
