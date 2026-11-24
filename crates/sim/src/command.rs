//! Player/AI intents. Commands are collected between ticks and applied in pipeline step 1 of the
//! next tick (docs/architecture.md §3.3), which keeps the simulation replayable and testable.
//!
//! Every variant is validated against the current state; invalid commands are not applied and
//! produce a [`SimEvent::CommandRejected`] with a [`RejectReason`].

use glam::IVec2;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::buildings::{self, BuildingId, PlacementError, Rotation, UpgradeBlocker};
use crate::defs::{
    BuildingTypeId, CropId, Defs, EventDefId, JobCategory, ResourceAmount, ResourceId, RoadTypeId, TechId, UnitTypeId, Weather,
};
use crate::military::{Order, RecruitProblem};
use crate::population::CitizenId;
use crate::diplomacy::{DiplomaticAction, KingdomId};
use crate::economy::{TradeOrder, storage, trade};
use crate::event::SimEvent;
use crate::policy::JobPriority;
use crate::production;
use crate::{agriculture, research};
use crate::roads::{self, RoadProblem};
use crate::state::GameState;

/// An intent issued by the player (UI) or an AI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Sets the tax rate (0 ..= `GameRules::max_tax_rate_percent`).
    SetTaxRate {
        /// New rate in percent.
        percent: u8,
    },
    /// Sets the priority of a job category for the automatic worker assignment.
    SetWorkforcePriority {
        /// Category.
        category: JobCategory,
        /// New priority.
        priority: JobPriority,
    },
    /// Places a construction site (validated like
    /// [`check_placement`](crate::buildings::check_placement); cost is paid immediately).
    PlaceBuilding {
        /// Building type.
        kind: BuildingTypeId,
        /// Minimum corner of the rotated footprint.
        origin: IVec2,
        /// Orientation.
        rotation: Rotation,
    },
    /// Demolishes a building (50 % material refund) or cancels a new building's site.
    DemolishBuilding {
        /// The building.
        building: BuildingId,
    },
    /// Cancels a construction site or a running upgrade (unused cost refunded completely).
    CancelConstruction {
        /// The building.
        building: BuildingId,
    },
    /// Upgrades a building to its next level (houses: the next tier; every type with an
    /// `upgrade` in the data, docs/game_design.md §5).
    UpgradeBuilding {
        /// The building.
        building: BuildingId,
    },
    /// Locks or unlocks the automatic upgrades of a house (docs/game_design.md §5.1).
    SetUpgradeLocked {
        /// The house.
        building: BuildingId,
        /// True to stop automatic upgrades.
        locked: bool,
    },
    /// Moves a finished building (validated like [`check_move`](crate::buildings::check_move)).
    MoveBuilding {
        /// The building.
        building: BuildingId,
        /// New footprint origin.
        origin: IVec2,
        /// New orientation.
        rotation: Rotation,
    },
    /// Pauses or resumes a building or construction site.
    SetBuildingPaused {
        /// The building.
        building: BuildingId,
        /// True to pause.
        paused: bool,
    },
    /// Builds roads on a contiguous tile chain (validated like
    /// [`validate_road_tiles`](crate::roads::validate_road_tiles); water tiles become bridges).
    BuildRoad {
        /// Tiles in order, each a 4-neighbour of the previous one.
        tiles: Vec<IVec2>,
        /// Road type for the land tiles.
        road: RoadTypeId,
    },
    /// Removes roads (removing a bridge tile removes the whole bridge).
    RemoveRoad {
        /// Tiles to clear; tiles without a road are ignored.
        tiles: Vec<IVec2>,
    },
    /// Selects the recipe of a production building (replaces its queue with continuous
    /// production of that recipe).
    SetRecipe {
        /// The building.
        building: BuildingId,
        /// Index into the building type's recipes.
        recipe: u8,
    },
    /// Sets the priority of a building for workers and carriers.
    SetProductionPriority {
        /// The building.
        building: BuildingId,
        /// New priority.
        priority: JobPriority,
    },
    /// Adds a production order: `units` of the recipe's first output, or continuous (`None`).
    QueueProduction {
        /// The building.
        building: BuildingId,
        /// Index into the building type's recipes.
        recipe: u8,
        /// Units to produce; `None` = continuously.
        units: Option<u32>,
    },
    /// Removes all production orders of a building.
    ClearQueue {
        /// The building.
        building: BuildingId,
    },
    /// Creates or replaces the trade order for a resource.
    SetTradeOrder {
        /// The order.
        order: TradeOrder,
    },
    /// Removes the trade order for a resource.
    RemoveTradeOrder {
        /// The resource.
        resource: ResourceId,
    },
    /// Chooses the crop a field farm sows from now on (fields already sown keep their crop
    /// until harvest).
    SetCrop {
        /// The farm.
        building: BuildingId,
        /// One of the crops of the farm type.
        crop: CropId,
    },
    /// **Developer command** (the game sends it only behind a developer switch): marks every
    /// technology as researched.
    DebugUnlockAllResearch,
    /// **Developer command**: forces the weather from now on for `days` days (`days = 0` ends a
    /// forced weather; the normal draw resumes at the next midnight).
    DebugSetWeather {
        /// The weather.
        weather: Weather,
        /// Intensity in percent (1–100).
        intensity: u8,
        /// Days to keep it.
        days: u16,
    },
    /// Appends a technology to the research queue (its prerequisites must be researched or
    /// queued before it; docs/game_design.md §9).
    QueueResearch {
        /// The technology.
        tech: TechId,
    },
    /// Appends a technology together with its missing prerequisites (not researched, not
    /// queued; prerequisites first, [`crate::research::queue_plan`]) in one step: all or
    /// nothing (rejected if the queue cannot take them all).
    QueueResearchWithPrerequisites {
        /// The technology.
        tech: TechId,
    },
    /// Removes a technology and every queued technology depending on it from the research
    /// queue (progress and the paid cost are kept for a later restart).
    CancelResearch {
        /// The technology.
        tech: TechId,
    },
    /// Moves a queued technology to `position` (0 = research next; clamped to the queue). Every
    /// technology must stay behind its prerequisites.
    MoveResearch {
        /// The technology.
        tech: TechId,
        /// New position in the queue.
        position: u8,
    },
    /// Decides the pending event (docs/game_design.md §12) with one of its choices.
    ResolveEvent {
        /// Serial number of the pending event ([`crate::events::PendingEvent::serial`]).
        serial: u32,
        /// Index of the choice.
        choice: u8,
    },
    /// **Developer command**: starts an event now regardless of its conditions, cooldown and
    /// the daily draw (only while no other event is pending).
    DebugTriggerEvent {
        /// The event.
        event: EventDefId,
    },
    /// Places a construction site like `PlaceBuilding` and, only if that succeeded, builds the
    /// access road [`plan_access_road`](crate::roads::plan_access_road) plans for it with road
    /// type `road` (docs/game_design.md §5.6: the free footpath to the entrance). A rejected
    /// access road is reported as a rejected `BuildRoad`; the site stays.
    PlaceBuildingWithAccess {
        /// Building type.
        kind: BuildingTypeId,
        /// Minimum corner of the rotated footprint.
        origin: IVec2,
        /// Orientation.
        rotation: Rotation,
        /// Road type of the access road.
        road: RoadTypeId,
    },
    /// A diplomatic action towards an AI kingdom (docs/game_design.md §10): propose or cancel a
    /// treaty, gift, demand tribute, declare war, propose peace. Proposals are judged at once.
    Diplomacy {
        /// The kingdom.
        kingdom: KingdomId,
        /// The action.
        action: DiplomaticAction,
    },
    /// Answers a proposal of a kingdom waiting in the diplomatic inbox.
    AnswerProposal {
        /// Serial number ([`crate::diplomacy::Proposal::serial`]).
        serial: u32,
        /// True to accept.
        accept: bool,
    },
    /// Sends an own trade caravan from the trading post to a kingdom with access: it sells
    /// `goods` there and buys `buy` with `gold` and the proceeds (docs/game_design.md §7).
    SendCaravan {
        /// The partner.
        kingdom: KingdomId,
        /// Goods to sell (from the storages near the trading post).
        goods: Vec<ResourceAmount>,
        /// Gold taken along.
        gold: u32,
        /// Goods to buy at the partner.
        buy: Vec<ResourceAmount>,
    },
    /// **Developer command**: adds gold and goods (into the storages near the headquarters;
    /// what does not fit is reported lost).
    DebugGrant {
        /// Gold.
        gold: u32,
        /// Goods.
        goods: Vec<ResourceAmount>,
    },
    /// **Developer command**: `count` households of immigrants walk in from the map edge (like
    /// refugees: they may take every free place; whoever finds none camps at the headquarters).
    DebugSpawnHouseholds {
        /// Households (1–50).
        count: u16,
    },
    /// **Developer command**: sets a kingdom's relation to the player (−100 … +100) through a
    /// remembered value that fades like any other.
    DebugSetRelation {
        /// The kingdom.
        kingdom: KingdomId,
        /// New relation.
        value: i8,
    },
    /// Recruits `count` units of `unit` at a military building (docs/game_design.md §11):
    /// costs gold and equipment, adult citizens leave their jobs and train there.
    Recruit {
        /// The training building.
        building: BuildingId,
        /// Unit type.
        unit: UnitTypeId,
        /// Units (1 ..= `military.training_slots`).
        count: u8,
    },
    /// Soldiers return to civilian life.
    DismissSoldiers {
        /// The soldiers (operators of siege engines take their crew along).
        soldiers: Vec<CitizenId>,
    },
    /// Founds a new company („N. Kompanie“) with ready soldiers (may be empty).
    CreateCompany {
        /// Members.
        soldiers: Vec<CitizenId>,
    },
    /// Moves ready soldiers into a company (`None`: out of their company).
    AssignSoldiers {
        /// The soldiers.
        soldiers: Vec<CitizenId>,
        /// Target company.
        company: Option<u32>,
    },
    /// Disbands a company; its soldiers stay soldiers.
    DisbandCompany {
        /// The company.
        company: u32,
    },
    /// Gives a company an order: station, defend, patrol, campaign or siege, retreat.
    CompanyOrder {
        /// The company.
        company: u32,
        /// The order.
        order: Order,
    },
    /// **Developer command**: enemies gather at the map edge and arrive within the hour:
    /// bandits (`kingdom: None`) or an army of a kingdom at war with the player.
    DebugSpawnThreat {
        /// The kingdom sending an army (`None`: bandits).
        kingdom: Option<KingdomId>,
        /// Fighters (1–120).
        size: u16,
    },
    /// Holds a festival (docs/game_design.md §7 Feste und Hofhaltung): costs gold by the
    /// town's size, needs a church or a market, at most every `festival.cooldown_days` days.
    HoldFestival,
}

/// Why a command was not applied. The `Display` text is German and meant for the player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum RejectReason {
    /// Requested tax rate exceeds the allowed maximum.
    #[error("Der Steuersatz von {requested} % ist zu hoch (höchstens {max} %).")]
    TaxRateTooHigh {
        /// Requested rate.
        requested: u8,
        /// Allowed maximum.
        max: u8,
    },
    /// The building cannot be placed or moved there.
    #[error(transparent)]
    Placement(PlacementError),
    /// The road cannot be built.
    #[error(transparent)]
    Road(RoadProblem),
    /// The house cannot be upgraded now.
    #[error(transparent)]
    Upgrade(UpgradeBlocker),
    /// The building id is stale (the building no longer exists).
    #[error("Dieses Gebäude existiert nicht mehr.")]
    UnknownBuilding,
    /// Cancelling needs a construction site or running upgrade.
    #[error("Hier wird nichts gebaut.")]
    NotUnderConstruction,
    /// Moves cannot be cancelled.
    #[error("Ein Umzug kann nicht abgebrochen werden.")]
    CannotCancelMove,
    /// A damaged building can only be repaired or demolished.
    #[error("Eine Ruine kann nur wiederaufgebaut oder abgerissen werden.")]
    CannotCancelRepair,
    /// The last headquarters must stay.
    #[error("Das letzte Hauptquartier kann nicht abgerissen werden.")]
    LastHeadquarters,
    /// Headquarters cannot be moved.
    #[error("Das Hauptquartier kann nicht versetzt werden.")]
    CannotMoveHeadquarters,
    /// Only finished, idle buildings can be moved.
    #[error("Nur fertige Gebäude, die nicht umgebaut werden, können versetzt werden.")]
    NotMovable,
    /// Finished houses cannot be paused (inhabited or not).
    #[error("Wohnhäuser können nicht stillgelegt werden.")]
    CannotPauseHousing,
    /// None of the tiles has a road.
    #[error("Hier gibt es keine Straße.")]
    NoRoadToRemove,
    /// Somebody is on the bridge that would be removed.
    #[error("Auf der Brücke bei {pos} ist gerade jemand unterwegs – sie kann erst abgerissen werden, wenn sie frei ist.")]
    BridgeInUse {
        /// A bridge tile with a walker on it.
        pos: IVec2,
    },
    /// The building has no production.
    #[error("Dieses Gebäude stellt nichts her.")]
    NoProduction,
    /// The recipe index does not exist for the building.
    #[error("Dieses Rezept gibt es hier nicht.")]
    UnknownRecipe {
        /// Requested index.
        recipe: u8,
    },
    /// A quantity of zero was requested.
    #[error("Die Menge muss mindestens 1 sein.")]
    InvalidQuantity,
    /// The production queue is full.
    #[error("Die Warteschlange ist voll (höchstens {max} Aufträge).")]
    QueueFull {
        /// Maximum entries.
        max: u8,
    },
    /// The resource id does not exist.
    #[error("Diese Ware gibt es nicht.")]
    UnknownResource,
    /// A trade order needs a buy or a sell threshold.
    #[error("Ein Handelsauftrag braucht eine Kauf- oder Verkaufsgrenze.")]
    TradeOrderEmpty,
    /// The buy threshold lies above the sell threshold.
    #[error("Die Kaufgrenze ({buy_below}) darf nicht über der Verkaufsgrenze ({sell_above}) liegen.")]
    TradeThresholdsOverlap {
        /// Buy threshold.
        buy_below: u32,
        /// Sell threshold.
        sell_above: u32,
    },
    /// The daily limit is zero or too high.
    #[error("Die Tagesmenge muss zwischen 1 und {max} liegen.")]
    TradeLimitOutOfRange {
        /// Highest allowed limit.
        max: u32,
    },
    /// There is no trade order for the resource.
    #[error("Für diese Ware gibt es keinen Handelsauftrag.")]
    NoTradeOrder,
    /// The building does not cultivate fields.
    #[error("Dieses Gebäude bestellt keine Felder.")]
    NotAFieldFarm,
    /// The farm is still being built or moved.
    #[error("Die Farm ist noch nicht fertig.")]
    FarmNotReady,
    /// The farm cannot grow this crop.
    #[error("Diese Feldfrucht kann hier nicht angebaut werden.")]
    CropNotGrownHere,
    /// The intensity is outside 1–100 %.
    #[error("Die Stärke muss zwischen 1 und 100 % liegen.")]
    InvalidIntensity,
    /// The technology id does not exist.
    #[error("Diese Technologie gibt es nicht.")]
    UnknownTech,
    /// The technology is already researched.
    #[error("„{name}“ ist bereits erforscht.")]
    AlreadyResearched {
        /// German name.
        name: String,
    },
    /// The technology is already in the queue.
    #[error("„{name}“ steht bereits in der Forschungsliste.")]
    AlreadyQueued {
        /// German name.
        name: String,
    },
    /// The research queue is full.
    #[error("Die Forschungsliste ist voll (höchstens {max} Einträge).")]
    ResearchQueueFull {
        /// Maximum entries.
        max: u8,
    },
    /// Prerequisites are neither researched nor queued before the technology.
    #[error("„{name}“ braucht zuerst {missing}.")]
    MissingPrerequisites {
        /// German name of the technology.
        name: String,
        /// German names of the missing prerequisites, quoted and comma-separated.
        missing: String,
    },
    /// The technology is not in the research queue.
    #[error("Diese Technologie steht nicht in der Forschungsliste.")]
    NotQueued,
    /// A technology would come before one of its prerequisites.
    #[error("„{name}“ kann nicht vor „{prerequisite}“ erforscht werden.")]
    InvalidResearchOrder {
        /// German name of the technology.
        name: String,
        /// German name of its prerequisite.
        prerequisite: String,
    },
    /// No event with this serial number is waiting for a decision.
    #[error("Dieses Ereignis wartet nicht mehr auf eine Entscheidung.")]
    NoPendingEvent,
    /// The choice index does not exist.
    #[error("Diese Entscheidung gibt es nicht.")]
    UnknownChoice,
    /// The requirements of the choice are not met.
    #[error("Diese Entscheidung ist nicht möglich: {reason}")]
    ChoiceUnavailable {
        /// German reason, e.g. „Es fehlen 50 Gold.“
        reason: String,
    },
    /// The event id does not exist.
    #[error("Dieses Ereignis gibt es nicht.")]
    UnknownEvent,
    /// Another event is waiting for a decision.
    #[error("Ein anderes Ereignis wartet noch auf eine Entscheidung.")]
    EventPending,
    /// The kingdom id does not exist.
    #[error("Dieses Königreich gibt es nicht.")]
    UnknownKingdom,
    /// A diplomatic action is not possible now.
    #[error("{reason}")]
    DiplomacyUnavailable {
        /// German reason.
        reason: String,
    },
    /// No proposal with this serial number waits for an answer.
    #[error("Dieser Vorschlag wartet nicht mehr auf eine Antwort.")]
    NoProposal,
    /// The caravan cannot set out.
    #[error("{reason}")]
    CaravanUnavailable {
        /// German reason.
        reason: String,
    },
    /// The relation is outside −100 … +100.
    #[error("Die Beziehung muss zwischen −100 und +100 liegen.")]
    InvalidRelation,
    /// The number of households is outside 1–50.
    #[error("Es können 1 bis 50 Haushalte auf einmal kommen.")]
    InvalidHouseholdCount,
    /// The tax rate is capped by a promise made in an event.
    #[error("Den Bürgern wurden niedrige Steuern versprochen: höchstens {max} % bis {until}.")]
    TaxRateCapped {
        /// Highest allowed rate.
        max: u8,
        /// German date until which the cap holds.
        until: String,
    },
    /// A recruitment is not possible.
    #[error("{0}")]
    Recruit(RecruitProblem),
    /// A military command (companies, orders, dismissal) is not possible.
    #[error("{reason}")]
    Military {
        /// German reason.
        reason: String,
    },
    /// The game is lost; nothing can be done any more.
    #[error("Das Spiel ist verloren.")]
    GameOver,
    /// No festival can be held now.
    #[error("{reason}")]
    FestivalUnavailable {
        /// German reason.
        reason: String,
    },
}

/// Outcome of applying a single command.
pub type CommandResult = Result<(), RejectReason>;

/// Applies one command to the state, emitting events for visible changes.
pub(crate) fn apply_command(
    state: &mut GameState,
    defs: &Defs,
    command: &Command,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    match *command {
        Command::SetTaxRate { percent } => {
            let max = defs.rules().max_tax_rate_percent;
            if percent > max {
                return Err(RejectReason::TaxRateTooHigh { requested: percent, max });
            }
            if let Some(cap) = state.events.tax_cap(state.clock.tick)
                && percent > cap.max_percent
            {
                let until = crate::time::GameClock::at_tick(cap.until_tick).display();
                return Err(RejectReason::TaxRateCapped { max: cap.max_percent, until });
            }
            let policy = state.policy_mut();
            if policy.tax_rate_percent != percent {
                policy.tax_rate_percent = percent;
                events.push(SimEvent::TaxRateChanged { percent });
            }
            Ok(())
        }
        Command::SetWorkforcePriority { category, priority } => {
            if state.policy_mut().set_job_priority(category, priority) {
                state.workforce.mark_dirty();
                events.push(SimEvent::WorkforcePriorityChanged { category, priority });
            }
            Ok(())
        }
        Command::PlaceBuilding { kind, origin, rotation } => {
            buildings::place_building(state, defs, kind, origin, rotation, events)
        }
        Command::PlaceBuildingWithAccess { kind, origin, rotation, road } => {
            buildings::place_building(state, defs, kind, origin, rotation, events)?;
            storage::refresh_stock(state, defs);
            let access = roads::plan_access_road(state, defs, kind, origin, rotation, road)
                .filter(|plan| plan.new_tile_count() > 0)
                .and_then(|plan| plan.to_command());
            if let Some(build) = access
                && let Err(reason) = apply_command(state, defs, &build, events)
            {
                events.push(SimEvent::CommandRejected { command: build, reason });
            }
            Ok(())
        }
        Command::DemolishBuilding { building } => buildings::demolish_building(state, defs, building, events),
        Command::CancelConstruction { building } => buildings::cancel_construction(state, defs, building, events),
        Command::UpgradeBuilding { building } => buildings::upgrade_building(state, defs, building, events),
        Command::SetUpgradeLocked { building, locked } => {
            buildings::set_upgrade_locked(state, defs, building, locked, events)
        }
        Command::MoveBuilding { building, origin, rotation } => {
            buildings::move_building(state, defs, building, origin, rotation, events)
        }
        Command::SetBuildingPaused { building, paused } => buildings::set_paused(state, defs, building, paused, events),
        Command::BuildRoad { ref tiles, road } => roads::build_roads(state, defs, tiles, road, events),
        Command::RemoveRoad { ref tiles } => roads::remove_roads(state, tiles, events),
        Command::SetRecipe { building, recipe } => production::set_recipe(state, defs, building, recipe, events),
        Command::SetProductionPriority { building, priority } => {
            production::set_priority(state, building, priority, events)
        }
        Command::QueueProduction { building, recipe, units } => {
            production::queue_production(state, defs, building, recipe, units, events)
        }
        Command::ClearQueue { building } => production::clear_queue(state, defs, building, events),
        Command::SetTradeOrder { order } => trade::set_order(state, defs, order, events),
        Command::RemoveTradeOrder { resource } => trade::remove_order(state, resource, events),
        Command::SetCrop { building, crop } => agriculture::set_crop(state, defs, building, crop, events),
        Command::DebugUnlockAllResearch => {
            research::unlock_all(state, defs, events);
            Ok(())
        }
        Command::QueueResearch { tech } => research::queue_research(state, defs, tech, events),
        Command::QueueResearchWithPrerequisites { tech } => research::queue_with_prerequisites(state, defs, tech, events),
        Command::CancelResearch { tech } => research::cancel_research(state, defs, tech, events),
        Command::MoveResearch { tech, position } => research::move_research(state, defs, tech, position, events),
        Command::ResolveEvent { serial, choice } => crate::events::resolve(state, defs, serial, choice, events),
        Command::DebugTriggerEvent { event } => crate::events::trigger(state, defs, event, events),
        Command::Diplomacy { kingdom, ref action } => crate::diplomacy::actions::apply(state, defs, kingdom, action, events),
        Command::AnswerProposal { serial, accept } => crate::diplomacy::actions::answer(state, defs, serial, accept, events),
        Command::SendCaravan { kingdom, ref goods, gold, ref buy } => {
            crate::diplomacy::caravans::send(state, defs, kingdom, goods, gold, buy, events)
        }
        Command::DebugGrant { gold, ref goods } => {
            if goods.iter().any(|g| g.resource.index() >= defs.resources().len()) {
                return Err(RejectReason::UnknownResource);
            }
            state.economy.add_gold(i64::from(gold));
            let at = storage::main_headquarters(state, defs)
                .and_then(|b| state.buildings.get(b))
                .map_or(state.world.start, |b| b.entrance());
            storage::deposit_or_lose(state, defs, goods, at, events);
            events.push(SimEvent::DebugGranted { gold, goods: goods.clone() });
            Ok(())
        }
        Command::DebugSpawnHouseholds { count } => {
            if !(1..=50).contains(&count) {
                return Err(RejectReason::InvalidHouseholdCount);
            }
            crate::population::arrive_households(state, defs, count, events);
            Ok(())
        }
        Command::DebugSetRelation { kingdom, value } => {
            if state.diplomacy.kingdom(kingdom).is_none() {
                return Err(RejectReason::UnknownKingdom);
            }
            if !(-100..=100).contains(&value) {
                return Err(RejectReason::InvalidRelation);
            }
            crate::diplomacy::relations::set_relation(state, defs, kingdom, i32::from(value));
            crate::diplomacy::relations::announce_states(state, defs, events);
            Ok(())
        }
        Command::Recruit { building, unit, count } => {
            crate::military::recruit::recruit(state, defs, building, unit, count, events).map_err(RejectReason::Recruit)
        }
        Command::DismissSoldiers { ref soldiers } => {
            crate::military::recruit::dismiss(state, defs, soldiers, events).map_err(|reason| RejectReason::Military { reason })
        }
        Command::CreateCompany { ref soldiers } => crate::military::orders::create_company(state, defs, soldiers, events)
            .map(|_| ())
            .map_err(|reason| RejectReason::Military { reason }),
        Command::AssignSoldiers { ref soldiers, company } => {
            crate::military::orders::assign(state, defs, soldiers, company, events).map_err(|reason| RejectReason::Military { reason })
        }
        Command::DisbandCompany { company } => {
            crate::military::orders::disband(state, company, events).map_err(|reason| RejectReason::Military { reason })
        }
        Command::CompanyOrder { company, ref order } => {
            crate::military::orders::give_order(state, defs, company, order, events).map_err(|reason| RejectReason::Military { reason })
        }
        Command::HoldFestival => crate::events::festival::hold(state, defs, events),
        Command::DebugSpawnThreat { kingdom, size } => {
            crate::military::debug_spawn_threat(state, defs, kingdom, size, events).map_err(|reason| RejectReason::Military { reason })
        }
        Command::DebugSetWeather { weather, intensity, days } => {
            if !(1..=100).contains(&intensity) {
                return Err(RejectReason::InvalidIntensity);
            }
            let today = state.clock.total_days();
            state.weather.force(weather, intensity, days, today, None);
            events.push(SimEvent::WeatherChanged { weather: state.weather.current(), intensity: state.weather.intensity() });
            Ok(())
        }
    }
}

/// Pipeline step 1: applies commands in order; rejected ones become [`SimEvent::CommandRejected`].
pub(crate) fn apply_all(state: &mut GameState, defs: &Defs, commands: Vec<Command>, events: &mut Vec<SimEvent>) {
    for command in commands {
        let result = apply_command(state, defs, &command, events);
        // Keeps the stock view exact for the next command (roads and pauses change reachability).
        storage::refresh_stock(state, defs);
        if let Err(reason) = result {
            log::debug!("command rejected: {command:?}: {reason}");
            events.push(SimEvent::CommandRejected { command, reason });
        }
    }
}
