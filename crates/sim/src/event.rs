//! Events emitted by the simulation for the UI, audio and statistics.
//!
//! Every [`GameState::tick`](crate::state::GameState::tick) returns the `SimEvent`s it produced.
//! They describe what happened; they never feed back into the simulation, so the UI may drop or
//! batch them freely.

use glam::IVec2;
use serde::{Deserialize, Serialize};

use crate::buildings::{BuildingId, Rotation};
use crate::command::{Command, RejectReason};
use crate::diplomacy::{CaravanKind, KingdomId, Party, PeaceTerms, RelationState, TreatyKind};
use crate::defs::{
    BuildingTypeId, CropId, DamageCause, EventDefId, JobCategory, ResourceAmount, ResourceId, RoadTypeId, TechId, UnitTypeId,
    Weather,
};
use crate::economy::{TradeDirection, TradeOrder};
use crate::logistics::TransportKind;
use crate::military::{BattleKind, BattleOutcome};
use crate::ranks::DefeatReason;
use crate::policy::JobPriority;
use crate::population::{CitizenId, DeathCause, HouseholdId};
use crate::production::ProductionStatus;
use crate::time::Season;
use crate::world::DepositKind;

/// Importance of a notification (colour and sound in the UI).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// Neutral information.
    Info,
    /// Something good happened.
    Good,
    /// Needs attention.
    Warning,
    /// Immediate danger.
    Danger,
}

impl Severity {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            Severity::Info => "Information",
            Severity::Good => "Erfreulich",
            Severity::Warning => "Warnung",
            Severity::Danger => "Gefahr",
        }
    }
}

/// What a notification is about, machine-readable (docs/game_design.md §2 Meldungen): the UI
/// groups, filters and links messages by it instead of reading the German text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NotificationKind {
    /// Anything without a more specific kind.
    #[default]
    General,
    /// A construction site, move, repair or upgrade finished; a site was cancelled.
    Construction,
    /// A workshop lacks an input that is not on its way (Rohstoffmangel).
    MissingInput,
    /// A gatherer or farm finds nothing to gather in its radius.
    NoResources,
    /// A forester waits for room to plant (a dense forest, no shortage).
    DenseForest,
    /// A storage has transports waiting but no carrier.
    NoCarriers,
    /// Goods were lost for lack of storage room.
    StorageFull,
    /// Walkers find no way to a building.
    NoPath,
    /// Households find no food.
    Famine,
    /// Households have no fuel on cold days.
    NoFuel,
    /// Citizens without a home.
    Homeless,
    /// Births, deaths, arrivals, departures, households splitting or moving down a tier.
    Population,
    /// Treasury: bankruptcy, empty or shrinking treasury, soldiers' pay.
    Treasury,
    /// Trade, merchants and caravans.
    Trade,
    /// Research finished, started or short of gold.
    Research,
    /// A random event (docs/game_design.md §12) and its consequences.
    Event,
    /// A festival.
    Festival,
    /// Weather and seasons.
    Weather,
    /// Fields, harvests and frost.
    Agriculture,
    /// Recruitment, training and the army's campaigns.
    Military,
    /// Bandits and enemy armies approaching, attacking, breaking through or plundering.
    Threat,
    /// A battle was won or lost.
    Battle,
    /// The AI kingdoms: relations, treaties, proposals, wars and peace.
    Diplomacy,
    /// The ruler rose in rank.
    Rank,
    /// The game is lost.
    GameOver,
}

impl NotificationKind {
    /// Every kind, in declaration order.
    pub const ALL: [NotificationKind; 25] = [
        NotificationKind::General,
        NotificationKind::Construction,
        NotificationKind::MissingInput,
        NotificationKind::NoResources,
        NotificationKind::DenseForest,
        NotificationKind::NoCarriers,
        NotificationKind::StorageFull,
        NotificationKind::NoPath,
        NotificationKind::Famine,
        NotificationKind::NoFuel,
        NotificationKind::Homeless,
        NotificationKind::Population,
        NotificationKind::Treasury,
        NotificationKind::Trade,
        NotificationKind::Research,
        NotificationKind::Event,
        NotificationKind::Festival,
        NotificationKind::Weather,
        NotificationKind::Agriculture,
        NotificationKind::Military,
        NotificationKind::Threat,
        NotificationKind::Battle,
        NotificationKind::Diplomacy,
        NotificationKind::Rank,
        NotificationKind::GameOver,
    ];

    /// German display name (filters and group headings).
    pub const fn display_name(self) -> &'static str {
        match self {
            NotificationKind::General => "Allgemein",
            NotificationKind::Construction => "Bau",
            NotificationKind::MissingInput => "Rohstoffmangel",
            NotificationKind::NoResources => "Keine Rohstoffe im Umkreis",
            NotificationKind::DenseForest => "Dichter Wald",
            NotificationKind::NoCarriers => "Keine Träger",
            NotificationKind::StorageFull => "Kein Lagerplatz",
            NotificationKind::NoPath => "Kein Weg",
            NotificationKind::Famine => "Nahrungsmangel",
            NotificationKind::NoFuel => "Brennstoffmangel",
            NotificationKind::Homeless => "Obdachlose",
            NotificationKind::Population => "Bevölkerung",
            NotificationKind::Treasury => "Staatskasse",
            NotificationKind::Trade => "Handel",
            NotificationKind::Research => "Forschung",
            NotificationKind::Event => "Ereignis",
            NotificationKind::Festival => "Fest",
            NotificationKind::Weather => "Wetter",
            NotificationKind::Agriculture => "Landwirtschaft",
            NotificationKind::Military => "Militär",
            NotificationKind::Threat => "Bedrohung",
            NotificationKind::Battle => "Schlacht",
            NotificationKind::Diplomacy => "Diplomatie",
            NotificationKind::Rank => "Rang",
            NotificationKind::GameOver => "Niederlage",
        }
    }

    /// True for a lasting waiting state of a building or the kingdom (reported once when it
    /// begins, not again until it ended; docs/game_design.md §2 Meldungen).
    pub const fn is_waiting_state(self) -> bool {
        matches!(
            self,
            NotificationKind::MissingInput
                | NotificationKind::NoResources
                | NotificationKind::DenseForest
                | NotificationKind::NoCarriers
                | NotificationKind::NoPath
                | NotificationKind::Famine
                | NotificationKind::NoFuel
                | NotificationKind::Homeless
        )
    }
}

/// A message for the player's notification feed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    /// Importance.
    pub severity: Severity,
    /// German message text.
    pub text: String,
    /// Map tile the message refers to (the UI offers to jump there).
    pub position: Option<IVec2>,
    /// What the message is about (group and filter by this, not by the text).
    #[serde(default)]
    pub kind: NotificationKind,
    /// The building the message is about, if any (the UI links to its info window).
    #[serde(default)]
    pub building: Option<BuildingId>,
}

impl Notification {
    /// Notification without a map position.
    pub fn new(severity: Severity, text: impl Into<String>) -> Self {
        Self { severity, text: text.into(), position: None, kind: NotificationKind::General, building: None }
    }

    /// Attaches a map position.
    pub fn at(mut self, position: IVec2) -> Self {
        self.position = Some(position);
        self
    }

    /// Sets what the message is about.
    pub fn with_kind(mut self, kind: NotificationKind) -> Self {
        self.kind = kind;
        self
    }

    /// Names the building the message is about.
    pub fn for_building(mut self, building: BuildingId) -> Self {
        self.building = Some(building);
        self
    }
}

/// Something that happened during a tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SimEvent {
    /// Message for the notification feed.
    Notification(Notification),
    /// A new day began (midnight).
    DayChanged {
        /// Day within the season, 1–6.
        day_of_season: u64,
        /// Season of the new day.
        season: Season,
        /// Year of the new day.
        year: u64,
    },
    /// A new season began.
    SeasonChanged {
        /// The new season.
        season: Season,
        /// Current year.
        year: u64,
    },
    /// A new year began.
    YearChanged {
        /// The new year.
        year: u64,
    },
    /// A command could not be applied.
    CommandRejected {
        /// The rejected command.
        command: Command,
        /// Why it was rejected.
        reason: RejectReason,
    },
    /// The tax rate was changed.
    TaxRateChanged {
        /// New rate in percent.
        percent: u8,
    },
    /// A job category priority of the worker assignment was changed.
    WorkforcePriorityChanged {
        /// Category.
        category: JobCategory,
        /// New priority.
        priority: JobPriority,
    },
    /// A construction site was placed (cost paid, footprint cleared).
    BuildingPlaced {
        /// The new building (a construction site).
        building: BuildingId,
        /// Its type.
        kind: BuildingTypeId,
        /// Footprint origin.
        origin: IVec2,
        /// Orientation.
        rotation: Rotation,
        /// Wood gained from cleared trees.
        cleared_wood: u32,
    },
    /// Builders started working on a site (new building, move or upgrade).
    ConstructionStarted {
        /// The building.
        building: BuildingId,
        /// Its current type.
        kind: BuildingTypeId,
    },
    /// A new building was finished.
    BuildingCompleted {
        /// The building.
        building: BuildingId,
        /// Its type.
        kind: BuildingTypeId,
    },
    /// A construction site was cancelled.
    ConstructionCancelled {
        /// The removed site.
        building: BuildingId,
        /// Its type.
        kind: BuildingTypeId,
        /// Refunded materials (gold is refunded as well).
        refund: Vec<ResourceAmount>,
    },
    /// A building was demolished (or replaced, e.g. a wall segment by a gate).
    BuildingDemolished {
        /// The removed building.
        building: BuildingId,
        /// Its type.
        kind: BuildingTypeId,
        /// Its footprint origin.
        origin: IVec2,
        /// Refunded materials and contents.
        refund: Vec<ResourceAmount>,
    },
    /// A house upgrade began.
    UpgradeStarted {
        /// The house.
        building: BuildingId,
        /// Current type.
        from: BuildingTypeId,
        /// Target type.
        to: BuildingTypeId,
    },
    /// A house upgrade finished.
    BuildingUpgraded {
        /// The house.
        building: BuildingId,
        /// Previous type.
        from: BuildingTypeId,
        /// New type.
        to: BuildingTypeId,
    },
    /// The automatic upgrades of a house were locked or unlocked.
    UpgradeLockChanged {
        /// The house.
        building: BuildingId,
        /// New state.
        locked: bool,
    },
    /// A running upgrade was cancelled.
    UpgradeCancelled {
        /// The house.
        building: BuildingId,
        /// The abandoned target type.
        target: BuildingTypeId,
        /// Refunded materials (gold is refunded as well).
        refund: Vec<ResourceAmount>,
    },
    /// A building is being moved (now a construction site at the new spot).
    MoveStarted {
        /// The building.
        building: BuildingId,
        /// Its type.
        kind: BuildingTypeId,
        /// Previous origin.
        from: IVec2,
        /// New origin.
        to: IVec2,
    },
    /// A move finished.
    BuildingMoved {
        /// The building.
        building: BuildingId,
        /// Its type.
        kind: BuildingTypeId,
    },
    /// A building or site was paused or resumed.
    BuildingPauseChanged {
        /// The building.
        building: BuildingId,
        /// New state.
        paused: bool,
    },
    /// Road tiles were built or upgraded.
    RoadsBuilt {
        /// Type of the land tiles (water tiles became bridges).
        road: RoadTypeId,
        /// Changed tiles.
        tiles: Vec<IVec2>,
    },
    /// Road tiles were removed.
    RoadsRemoved {
        /// Removed tiles.
        tiles: Vec<IVec2>,
    },
    /// The owned territory changed.
    TerritoryChanged {
        /// Owned tiles now.
        owned_tiles: u32,
    },
    /// A building's priority for workers and carriers changed.
    BuildingPriorityChanged {
        /// The building.
        building: BuildingId,
        /// New priority.
        priority: JobPriority,
    },
    /// A production building got a new recipe (continuous production).
    RecipeChanged {
        /// The building.
        building: BuildingId,
        /// Recipe index.
        recipe: u8,
    },
    /// A production queue was changed by the player.
    ProductionQueueChanged {
        /// The building.
        building: BuildingId,
    },
    /// The production status of a building changed.
    ProductionStatusChanged {
        /// The building.
        building: BuildingId,
        /// New status.
        status: ProductionStatus,
    },
    /// A work cycle finished.
    ProductionCompleted {
        /// The building.
        building: BuildingId,
        /// Units put into the output buffer (empty for planting or failed gathering).
        outputs: Vec<ResourceAmount>,
    },
    /// A production building lacks an input (warning; repeated at most twice a day).
    Shortage {
        /// The building.
        building: BuildingId,
        /// Missing resource.
        resource: ResourceId,
        /// Entrance of the building.
        position: IVec2,
    },
    /// A woodcutter felled a tree.
    TreeFelled {
        /// Tile of the tree.
        position: IVec2,
    },
    /// A forester planted a sapling.
    TreePlanted {
        /// Tile of the sapling.
        position: IVec2,
    },
    /// A deposit was used up.
    DepositDepleted {
        /// Tile of the deposit.
        position: IVec2,
        /// Its kind.
        kind: DepositKind,
    },
    /// A carrier picked up goods.
    TransportStarted {
        /// Job id.
        job: u64,
        /// Kind of transport.
        kind: TransportKind,
        /// Resource.
        resource: ResourceId,
        /// Units carried.
        amount: u32,
        /// Source building.
        from: BuildingId,
        /// Target building.
        to: BuildingId,
        /// The citizen carrying the goods.
        carrier: CitizenId,
    },
    /// A carrier delivered goods.
    TransportCompleted {
        /// Job id.
        job: u64,
        /// Resource.
        resource: ResourceId,
        /// Units the target accepted (a rest is taken on to a storage with room).
        amount: u32,
        /// Target building.
        target: BuildingId,
    },
    /// Goods were discarded because no storage had room.
    ResourcesLost {
        /// Resource.
        resource: ResourceId,
        /// Units lost.
        amount: u32,
        /// Where it happened.
        position: IVec2,
    },
    /// A trade order was set (`Some`) or removed (`None`).
    TradeOrderChanged {
        /// Resource.
        resource: ResourceId,
        /// The new order.
        order: Option<TradeOrder>,
    },
    /// Goods were bought or sold.
    TradeExecuted {
        /// Resource.
        resource: ResourceId,
        /// Units traded.
        amount: u32,
        /// Gold received (positive) or paid (negative).
        gold: i64,
        /// Import or export.
        direction: TradeDirection,
    },
    /// A day was closed in the ledger.
    DailyBalance {
        /// Closed day (0-based).
        day: u64,
        /// Income in gold.
        income: i64,
        /// Expenses in gold (negative).
        expenses: i64,
        /// Treasury after the day.
        treasury: i64,
    },
    /// The treasury will run out within a few days at the current balance.
    TreasuryLow {
        /// Treasury in gold.
        treasury: i64,
        /// Estimated days left.
        days_left: u32,
    },
    /// The treasury became negative.
    BankruptcyStarted {
        /// Treasury in gold.
        treasury: i64,
    },
    /// The treasury is positive again.
    BankruptcyEnded {
        /// Treasury in gold.
        treasury: i64,
    },
    /// A child was born.
    CitizenBorn {
        /// The child.
        citizen: CitizenId,
        /// Its household.
        household: HouseholdId,
    },
    /// A citizen died (the id is no longer valid afterwards).
    CitizenDied {
        /// The citizen.
        citizen: CitizenId,
        /// Why.
        cause: DeathCause,
        /// Age in years.
        age: u32,
    },
    /// Two citizens married; one of them moved into the other's household.
    CitizensMarried {
        /// The husband.
        a: CitizenId,
        /// The wife.
        b: CitizenId,
    },
    /// A household of immigrants arrived at the main headquarters.
    HouseholdArrived {
        /// The new household.
        household: HouseholdId,
        /// Its size.
        members: u16,
        /// Entrance of the headquarters.
        position: IVec2,
    },
    /// An unhappy household left the kingdom (the id is no longer valid afterwards).
    HouseholdEmigrated {
        /// The household.
        household: HouseholdId,
        /// Its size.
        members: u16,
    },
    /// A household moved into a house (`Some`) or became homeless (`None`).
    HouseholdMoved {
        /// The household.
        household: HouseholdId,
        /// The new home.
        home: Option<BuildingId>,
    },
    /// A house fell back to the previous tier after its requirements stayed unmet.
    HouseDowngraded {
        /// The house.
        building: BuildingId,
        /// Previous type.
        from: BuildingTypeId,
        /// New (lower) type.
        to: BuildingTypeId,
    },
    /// The day's weather was drawn (every midnight) or forced.
    WeatherChanged {
        /// Today's weather.
        weather: Weather,
        /// Its intensity in percent.
        intensity: u8,
    },
    /// The player chose another crop for a field farm.
    CropChanged {
        /// The farm.
        building: BuildingId,
        /// The crop sown from now on.
        crop: CropId,
    },
    /// A farm claimed or released field or pasture tiles.
    FieldsChanged {
        /// The farm.
        building: BuildingId,
        /// Tiles it holds now.
        fields: u16,
    },
    /// A farm finished harvesting all its ripe fields.
    HarvestCompleted {
        /// The farm.
        building: BuildingId,
        /// Harvested resource.
        resource: ResourceId,
        /// Units harvested in this round.
        amount: u32,
    },
    /// Frost killed unripe annual crops at the start of winter.
    CropsFrozen {
        /// Field tiles lost.
        fields: u32,
    },
    /// A developer command marked every technology as researched.
    DebugResearchUnlocked {
        /// Technologies newly researched.
        techs: u32,
    },
    /// The research queue changed (queued, cancelled or reordered).
    ResearchQueueChanged {
        /// The queue now; the first entry is researched next.
        queue: Vec<TechId>,
    },
    /// Research of a technology started (its cost was paid).
    ResearchStarted {
        /// The technology.
        tech: TechId,
        /// Gold paid.
        gold: u32,
    },
    /// A technology was removed from the research queue.
    ResearchCancelled {
        /// The technology.
        tech: TechId,
    },
    /// A technology was researched; its effects apply from now on.
    ResearchCompleted {
        /// The technology.
        tech: TechId,
    },
    /// An event occurred and waits for the player's decision.
    EventStarted {
        /// Serial number for `Command::ResolveEvent`.
        serial: u32,
        /// The event.
        event: EventDefId,
        /// Map tile the event refers to (a burning building, the river bank …).
        position: Option<IVec2>,
        /// Tick at which the first choice applies if the player does not decide.
        expires_tick: u64,
    },
    /// An event was decided (by the player or, after the time ran out, with its first choice).
    EventResolved {
        /// Serial number.
        serial: u32,
        /// The event.
        event: EventDefId,
        /// Index of the choice taken.
        choice: u8,
        /// True if the time ran out and the first choice applied.
        by_default: bool,
    },
    /// A building was damaged by an event and became a repair site.
    BuildingDamaged {
        /// The building.
        building: BuildingId,
        /// Its type.
        kind: BuildingTypeId,
        /// What damaged it.
        cause: DamageCause,
    },
    /// Builders finished repairing a damaged building.
    BuildingRepaired {
        /// The building.
        building: BuildingId,
        /// Its type.
        kind: BuildingTypeId,
    },
    /// Crops on field tiles were destroyed by an event.
    FieldsDamaged {
        /// Field tiles whose crops were lost.
        fields: u32,
    },
    /// Goods were destroyed or stolen by an event.
    StockLost {
        /// What was lost, per resource in id order.
        lost: Vec<ResourceAmount>,
        /// The buildings robbed or damaged, in id order.
        buildings: Vec<BuildingId>,
        /// Where it happened (entrance of the first building).
        position: Option<IVec2>,
    },
    /// Citizens fell ill in an epidemic.
    CitizensFellIll {
        /// Number of citizens.
        citizens: u32,
        /// Homes of the sick, in id order (empty: none of them has a home).
        homes: Vec<BuildingId>,
        /// Focus of the epidemic: the entrance of the home with the most sick (ties: lower id).
        position: Option<IVec2>,
    },
    /// A new deposit was discovered.
    DepositDiscovered {
        /// Kind of deposit.
        kind: DepositKind,
        /// Centre tile.
        position: IVec2,
        /// Tiles of the deposit.
        tiles: u8,
    },
    /// The territory of an AI kingdom grew or shrank (border overlay, minimap).
    KingdomBorderChanged {
        /// The kingdom.
        kingdom: KingdomId,
        /// Tiles it owns now.
        tiles: u32,
    },
    /// A kingdom's diplomatic state towards the player changed.
    RelationStateChanged {
        /// The kingdom.
        kingdom: KingdomId,
        /// The new state.
        state: RelationState,
    },
    /// A treaty was signed.
    TreatySigned {
        /// The kingdom.
        kingdom: KingdomId,
        /// Kind.
        treaty: TreatyKind,
    },
    /// A treaty ended (ran out, or `broken` by the player).
    TreatyEnded {
        /// The kingdom.
        kingdom: KingdomId,
        /// Kind.
        treaty: TreatyKind,
        /// True if the player cancelled it.
        broken: bool,
    },
    /// A kingdom answered a proposal or demand of the player.
    DiplomaticReply {
        /// The kingdom.
        kingdom: KingdomId,
        /// True if it agreed.
        accepted: bool,
    },
    /// The player sent a gift.
    GiftSent {
        /// The kingdom.
        kingdom: KingdomId,
        /// Gold.
        gold: u32,
        /// Goods.
        goods: Vec<ResourceAmount>,
    },
    /// The player held a festival (docs/game_design.md §7).
    FestivalHeld {
        /// Gold it cost.
        gold: u32,
        /// Kingdoms whose envoys came as guests.
        guests: u32,
    },
    /// A kingdom sent the player a gift.
    GiftReceived {
        /// The kingdom.
        kingdom: KingdomId,
        /// Gold.
        gold: u32,
        /// Goods stored.
        goods: Vec<ResourceAmount>,
    },
    /// A war between the player and a kingdom began.
    WarDeclared {
        /// Who declared it.
        attacker: Party,
        /// Who was attacked.
        defender: Party,
    },
    /// The war with a kingdom ended.
    PeaceMade {
        /// The kingdom.
        kingdom: KingdomId,
        /// The terms.
        terms: PeaceTerms,
    },
    /// A war between two AI kingdoms began.
    AiWarStarted {
        /// The attacker.
        attacker: KingdomId,
        /// The defender.
        defender: KingdomId,
    },
    /// A war between two AI kingdoms ended.
    AiPeace {
        /// The stronger side.
        winner: KingdomId,
        /// The side that ceded land and paid.
        loser: KingdomId,
    },
    /// A proposal of a kingdom arrived in the diplomatic inbox.
    ProposalReceived {
        /// Serial number for `Command::AnswerProposal`.
        serial: u32,
        /// From whom.
        kingdom: KingdomId,
        /// Tick from which it counts as declined.
        expires_tick: u64,
    },
    /// A proposal was answered (or ran out: `expired`, which counts as declined).
    ProposalAnswered {
        /// Serial number.
        serial: u32,
        /// From whom.
        kingdom: KingdomId,
        /// True if accepted.
        accepted: bool,
        /// True if the time ran out.
        expired: bool,
    },
    /// A border incident with a kingdom.
    BorderIncident {
        /// The kingdom.
        kingdom: KingdomId,
    },
    /// An enemy kingdom raided the player's storages.
    RaidHappened {
        /// The raiders' kingdom.
        kingdom: KingdomId,
        /// Goods taken (empty: the raid found nothing).
        lost: Vec<ResourceAmount>,
    },
    /// A caravan set out (foreign: from its kingdom; own: from the trading post).
    CaravanDeparted {
        /// Caravan id.
        caravan: u32,
        /// The partner kingdom.
        kingdom: KingdomId,
        /// Foreign or own.
        kind: CaravanKind,
    },
    /// Foreign merchants entered the map on their way to the trading post.
    CaravanArrived {
        /// Caravan id.
        caravan: u32,
        /// Their kingdom.
        kingdom: KingdomId,
    },
    /// Foreign merchants traded at the trading post.
    CaravanTraded {
        /// Caravan id.
        caravan: u32,
        /// Their kingdom.
        kingdom: KingdomId,
        /// Goods the player sold to them.
        sold: Vec<ResourceAmount>,
        /// Goods the player bought from them.
        bought: Vec<ResourceAmount>,
    },
    /// A caravan was robbed on its way.
    CaravanRobbed {
        /// Caravan id.
        caravan: u32,
        /// The partner kingdom.
        kingdom: KingdomId,
        /// The robbers' kingdom.
        robber: KingdomId,
        /// Goods taken.
        goods: Vec<ResourceAmount>,
    },
    /// An own caravan came back and unloaded at the trading post.
    CaravanReturned {
        /// Caravan id.
        caravan: u32,
        /// The partner kingdom.
        kingdom: KingdomId,
        /// Goods brought.
        goods: Vec<ResourceAmount>,
        /// Gold brought back.
        gold: i64,
    },
    /// A developer command added gold and goods.
    DebugGranted {
        /// Gold.
        gold: u32,
        /// Goods (what did not fit is reported with `ResourcesLost`).
        goods: Vec<ResourceAmount>,
    },
    /// Recruits enlisted at a military building (docs/game_design.md §11).
    SoldiersRecruited {
        /// The training building.
        building: BuildingId,
        /// Unit type.
        unit: UnitTypeId,
        /// Units.
        count: u8,
    },
    /// A recruit finished training.
    SoldierTrained {
        /// The soldier.
        citizen: CitizenId,
        /// Unit type.
        unit: UnitTypeId,
    },
    /// A soldier returned to civilian life.
    SoldierDismissed {
        /// The former soldier.
        citizen: CitizenId,
    },
    /// A company was created, changed (members, order) or disbanded.
    CompanyChanged {
        /// The company.
        company: u32,
    },
    /// A campaign started, changed its stage or ended.
    CampaignChanged {
        /// The campaign.
        campaign: u32,
        /// Its company.
        company: u32,
    },
    /// Enemies gather off the map: bandits (`kingdom: None`) or the army of a kingdom.
    ThreatApproaching {
        /// The enemy group ([`crate::military::Warband`]).
        warband: u32,
        /// The kingdom of an army.
        kingdom: Option<KingdomId>,
        /// Fighters.
        size: u16,
        /// Tick they reach the map edge.
        arrive_tick: u64,
    },
    /// Enemies walked onto the map.
    ThreatArrived {
        /// The enemy group.
        warband: u32,
        /// Fighters.
        size: u16,
    },
    /// An enemy fell.
    EnemyFell {
        /// Its group.
        warband: u32,
        /// The kingdom of an army.
        kingdom: Option<KingdomId>,
    },
    /// A battle ended (report in [`crate::military::Military::battles`]).
    BattleEnded {
        /// Serial number of the report.
        serial: u32,
        /// Kind.
        kind: BattleKind,
        /// Outcome for the player.
        outcome: BattleOutcome,
    },
    /// A wall segment or gate was broken down by enemies.
    StructureDestroyed {
        /// The former building.
        building: BuildingId,
        /// Its type.
        kind: BuildingTypeId,
        /// Its tile.
        origin: IVec2,
    },
    /// The kingdom reached a new ruler rank (index into `Defs::ranks`, docs/game_design.md §1).
    RankAchieved {
        /// The new rank.
        rank: u8,
    },
    /// The game is lost (the simulation stands still from now on).
    GameOver {
        /// Why.
        reason: DefeatReason,
    },
}

impl SimEvent {
    /// The notification carried by this event, if any.
    pub fn notification(&self) -> Option<&Notification> {
        match self {
            SimEvent::Notification(n) => Some(n),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_builder_and_serde() {
        let n = Notification::new(Severity::Warning, "Rohstoffmangel").at(IVec2::new(3, 4));
        assert_eq!(n.position, Some(IVec2::new(3, 4)));
        assert_eq!((n.kind, n.building), (NotificationKind::General, None));
        let building = BuildingId::from_raw_parts(7, 1);
        let n = n.with_kind(NotificationKind::MissingInput).for_building(building);
        assert_eq!((n.kind, n.building), (NotificationKind::MissingInput, Some(building)));
        let e = SimEvent::Notification(n.clone());
        assert_eq!(e.notification(), Some(&n));
        let text = ron::to_string(&e).unwrap();
        let back: SimEvent = ron::from_str(&text).unwrap();
        assert_eq!(back, e);
        assert!(SimEvent::YearChanged { year: 2 }.notification().is_none());
        assert!(Severity::Danger > Severity::Info);
        // Every kind is listed once and has a German name.
        for (i, k) in NotificationKind::ALL.iter().enumerate() {
            assert!(!k.display_name().is_empty());
            assert!(!NotificationKind::ALL[..i].contains(k), "{k:?} twice");
        }
        assert!(NotificationKind::NoResources.is_waiting_state() && !NotificationKind::Construction.is_waiting_state());
        // A notification saved without kind and building still loads.
        let old: Notification = ron::from_str("(severity: Info, text: \"x\", position: None)").unwrap();
        assert_eq!((old.kind, old.building), (NotificationKind::General, None));
    }
}
