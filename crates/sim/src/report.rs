//! Read-only summaries for the UI windows (economy, production, storage, finance, workforce,
//! trade, logistics, population: [`population_summary`], [`house_report`], [`citizen_detail`], …;
//! agriculture and weather: [`farm_report`], [`agriculture_summary`], [`weather_report`],
//! [`food_forecast`]; walking: [`commute_report`], [`path_problem`], [`walking_summary`];
//! research: [`research_report`]; events: [`events_report`], [`pending_event`]; kingdoms and
//! diplomacy: [`diplomacy_report`], [`kingdom_actions`], [`pending_proposals`],
//! [`caravans_report`], [`partner_prices`]; military: [`military_report`], [`recruit_options`],
//! [`threats_report`], [`battle_reports`], [`war_status`], [`campaigns_report`]; ranks and
//! defeat: [`rank_report`]; trade balance and production costs: [`trade_balance`],
//! [`building_economics`]).
//! Everything here is derived from the state on demand; nothing is cached.

mod agriculture;
mod diplomacy;
mod economics;
mod events;
mod military;
mod population;
mod research;
mod walking;

pub use agriculture::{
    FuelForecast, fuel_forecast,
    AgricultureSummary, CropChoice, CropRow, FarmReport, FieldRow, FoodForecast, WeatherReport, agriculture_summary,
    farm_report, food_forecast, weather_report,
};

pub use population::{
    CitizenDetail, CitizenLink, HappinessReport, HappinessRow, HouseReport, HouseholdRow, JobInfo, NeedRow, TrainingInfo,
    NeedsSummary, PopulationSummary, ResidentRow, StageRow, TavernSummary, WealthClassRow, WealthSummary, citizen_detail,
    happiness_report, house_report, needs_summary, population_summary, residents,
};
pub use diplomacy::{
    ActionRow, CaravanRow, DiplomacyReport, GoodRow, HistoryRow, KingdomRow, PartnerPriceRow, ProposalRow, TreatyRow, WarRow,
    caravans_report, diplomacy_report, kingdom_actions, kingdom_row, partner_prices, pending_proposals,
};
pub use economics::{
    BuildingEconomics, EconomicsBasis, MeasuredOutput, TRADE_BALANCE_DAYS, TradeBalance, TradeDay, UnitCost, building_economics,
    trade_balance,
};
pub use events::{EventChoiceRow, EventHistoryRow, EventsReport, ModifierRow, PendingEventReport, events_report, pending_event};
pub use military::{
    BattleRow, CampaignRow, CompanyRow, GameOverRow, MilitaryReport, NextRankRow, RankReport, RecruitOption, SoldierRow, ThreatRow,
    UnitRow, WarStatusRow, battle_reports, campaign_stage_text, campaigns_report, military_report, rank_report, recruit_options,
    threats_report, war_status,
};
pub use research::{
    PrerequisiteRow, ResearchQueueRow, ResearchReport, TechRow, TechState, duration_text, remaining_milli, research_report,
};
pub use walking::{
    CommuteReport, WalkingSummary, citizens_without_path, commute_report, hours_text, minutes_text, path_problem, ticks_text,
    ticks_to_minutes, walking_summary,
};

use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::defs::{BuildingTypeId, Defs, ResourceId, ResourceRate};
use crate::economy::finance::{self, DailyRates};
use crate::economy::trade::{self, TradeDirection, TradeOrder, TradeVolume};
use crate::economy::{DayLedger, storage};
use crate::logistics::{JobStage, TransportKind, carriers};
use crate::population::{Destination, Purpose, TripPhase};
use crate::policy::JobPriority;
use crate::production::{ProductionStatus, QueueEntry};
use crate::roads;
use crate::state::GameState;
use crate::workforce::{self, WorkforceSummary};

/// One resource in the economy window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRow {
    /// The resource.
    pub resource: ResourceId,
    /// Units in the kingdom stock.
    pub stock: u32,
    /// Storage capacity for it in the kingdom stock network.
    pub capacity: u32,
    /// Units owed to construction sites.
    pub committed: u32,
    /// Produced so far today.
    pub produced_today: u32,
    /// Consumed so far today.
    pub consumed_today: u32,
    /// Produced on the last closed day.
    pub produced_per_day: u32,
    /// Consumed on the last closed day.
    pub consumed_per_day: u32,
    /// Taken by households so far today (part of `consumed_today`).
    pub household_today: u32,
    /// Taken by households on the last closed day (part of `consumed_per_day`).
    pub household_per_day: u32,
    /// Current price in centi-gold.
    pub price_centi: u32,
    /// Base price in centi-gold.
    pub base_price_centi: u32,
    /// Price change against the last closed day in centi-gold.
    pub price_trend_centi: i64,
}

/// Economy window: treasury and all resources.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EconomySummary {
    /// Treasury in gold.
    pub treasury: i64,
    /// True while bankrupt.
    pub bankrupt: bool,
    /// One row per resource in id order.
    pub resources: Vec<ResourceRow>,
}

/// German clock time of a deadline seen from tick `now`: „18:00 Uhr“ on the same day,
/// „morgen 06:00 Uhr“ / „übermorgen 06:00 Uhr“, later „Sommer · Tag 6 · Jahr 1, 06:00 Uhr“ (the
/// calendar date as in the top bar).
pub fn deadline_clock_text(now: u64, until: u64) -> String {
    let (from, to) = (crate::time::GameClock::at_tick(now), crate::time::GameClock::at_tick(until));
    let time = to.time_string();
    match to.total_days().saturating_sub(from.total_days()) {
        0 => format!("{time} Uhr"),
        1 => format!("morgen {time} Uhr"),
        2 => format!("übermorgen {time} Uhr"),
        _ => format!("{}, {time} Uhr", to.date_string()),
    }
}

/// „Es fehlt 1 Gold“ / „Es fehlen 20 Gold“ (without the full stop).
pub fn missing_gold(n: i64) -> String {
    if n == 1 { "Es fehlt 1 Gold".to_owned() } else { format!("Es fehlen {n} Gold") }
}

/// „Es fehlt 1 Einheit Bretter“ / „Es fehlen 5 Bretter“ (without the full stop; `name` is the
/// good's German name).
pub fn missing_goods(n: u32, name: &str) -> String {
    if n == 1 { format!("Es fehlt 1 Einheit {name}") } else { format!("Es fehlen {n} {name}") }
}

/// „1 Feld“ / „3 Felder“ (nominative and accusative).
pub fn tiles_text(n: u32) -> String {
    if n == 1 { "1 Feld".to_owned() } else { format!("{n} Felder") }
}

/// „1 Feld“ / „3 Feldern“ (dative, e.g. after „im Umkreis von“).
pub fn tiles_dative_text(n: u32) -> String {
    if n == 1 { "1 Feld".to_owned() } else { format!("{n} Feldern") }
}

/// German "since" phrase for a span of whole days: „seit heute“, „seit 1 Tag“, „seit 3 Tagen“
/// (dative plural after „seit“).
pub fn since_days_text(days: u64) -> String {
    match days {
        0 => "seit heute".to_owned(),
        1 => "seit 1 Tag".to_owned(),
        n => format!("seit {n} Tagen"),
    }
}

/// Treasury, stock, production, consumption and prices of every resource.
pub fn economy_summary(state: &GameState, defs: &Defs) -> EconomySummary {
    let e = state.economy();
    let today = e.stats().today();
    let yesterday = e.stats().yesterday();
    let resources = defs
        .resources()
        .iter()
        .map(|r| {
            let price = e.prices().centi(r.id);
            ResourceRow {
                resource: r.id,
                stock: e.stock().get(r.id),
                capacity: e.capacity().get(r.id),
                committed: e.committed().get(r.id),
                produced_today: today.produced.get(r.id),
                consumed_today: today.consumed.get(r.id),
                produced_per_day: yesterday.map_or(0, |d| d.flows.produced.get(r.id)),
                consumed_per_day: yesterday.map_or(0, |d| d.flows.consumed.get(r.id)),
                household_today: today.household.get(r.id),
                household_per_day: yesterday.map_or(0, |d| d.flows.household.get(r.id)),
                price_centi: price,
                base_price_centi: r.base_price * 100,
                price_trend_centi: yesterday.map_or(0, |d| i64::from(price) - i64::from(d.price_centi.get(r.id))),
            }
        })
        .collect();
    EconomySummary { treasury: e.treasury(), bankrupt: e.is_bankrupt(), resources }
}

/// One closed day of a resource for charts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDay {
    /// Day (0-based).
    pub day: u64,
    /// Produced units.
    pub produced: u32,
    /// Consumed units.
    pub consumed: u32,
    /// Units taken by households (part of `consumed`).
    pub household: u32,
    /// Bought units.
    pub imported: u32,
    /// Sold units.
    pub exported: u32,
    /// Stock at the end of the day.
    pub stock: u32,
    /// Price at the end of the day in centi-gold.
    pub price_centi: u32,
}

/// History of one resource over the closed days (oldest first, at most 48).
pub fn resource_history(state: &GameState, resource: ResourceId) -> Vec<ResourceDay> {
    state
        .economy()
        .stats()
        .history()
        .map(|d| ResourceDay {
            day: d.day,
            produced: d.flows.produced.get(resource),
            consumed: d.flows.consumed.get(resource),
            household: d.flows.household.get(resource),
            imported: d.flows.imported.get(resource),
            exported: d.flows.exported.get(resource),
            stock: d.stock.get(resource),
            price_centi: d.price_centi.get(resource),
        })
        .collect()
}

/// A buffer or storage slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferSlot {
    /// Resource.
    pub resource: ResourceId,
    /// Units held.
    pub amount: u32,
    /// Capacity (0 = no longer accepted; the units are brought back to storage).
    pub capacity: u32,
    /// Units on the way here (waiting or delivering jobs).
    pub incoming: u32,
}

/// A recipe as shown in the production window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecipeInfo {
    /// Index for `SetRecipe` / `QueueProduction`.
    pub index: u8,
    /// German name.
    pub name: String,
    /// Inputs per cycle.
    pub inputs: Vec<ResourceRate>,
    /// Outputs per cycle.
    pub outputs: Vec<ResourceRate>,
    /// Cycle duration in game hours at full productivity.
    pub hours: f32,
}

/// Production window of one building.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProductionReport {
    /// The building.
    pub building: BuildingId,
    /// Its type.
    pub kind: BuildingTypeId,
    /// Status.
    pub status: ProductionStatus,
    /// German status text (names the missing resource).
    pub status_text: String,
    /// Selectable recipes.
    pub recipes: Vec<RecipeInfo>,
    /// Recipe of the next cycle.
    pub active_recipe: Option<u8>,
    /// Production orders.
    pub queue: Vec<QueueEntry>,
    /// Progress of the running cycle 0.0–1.0.
    pub progress: f32,
    /// Productivity of the last working tick in percent.
    pub productivity_percent: u32,
    /// Workers assigned.
    pub staffed: u8,
    /// Workers at the workplace right now (production uses only them).
    pub present: u8,
    /// Average training of the assigned workers in percent (docs §6 Qualifikation; 100 when
    /// fully trained, unskilled or unstaffed). Included in the staffing factor.
    pub training_percent: u32,
    /// Commute of the workers ("Arbeitsweg").
    pub commute: CommuteReport,
    /// Worker slots of the type.
    pub workers: u8,
    /// Priority for workers and carriers.
    pub priority: JobPriority,
    /// True if the type wears tools.
    pub uses_tools: bool,
    /// Cycles left on the current tool.
    pub tool_charge: u16,
    /// True if a road touches the entrance.
    pub has_road_access: bool,
    /// Operational storages that can exchange goods with the building.
    pub serving_storages: u32,
    /// Input buffer (capacity slots and held surplus).
    pub input: Vec<BufferSlot>,
    /// Output buffer.
    pub output: Vec<BufferSlot>,
    /// Completed cycles.
    pub cycles_completed: u64,
    /// Transports from or to the building waiting for a carrier.
    pub carrier_wait: Option<CarrierWait>,
    /// Production speed change from event modifiers in percent (docs/game_design.md §12;
    /// part of the productivity).
    pub event_modifier_percent: i32,
}

/// German status text, naming the missing resource.
pub fn status_text(defs: &Defs, status: ProductionStatus) -> String {
    match status {
        ProductionStatus::MissingInput(r) => {
            let name = defs.resources().get(r.index()).map_or_else(|| r.to_string(), |d| d.name.clone());
            format!("Rohstoffmangel: {name} fehlt")
        }
        other => other.display_name().to_string(),
    }
}

/// Storages that can exchange goods with `building` (operational, road-connected, in radius).
pub fn serving_storages(state: &GameState, defs: &Defs, building: BuildingId) -> Vec<BuildingId> {
    let Some(b) = state.building(building) else { return Vec::new() };
    let Some(component) = roads::building_component(state.world(), state.roads(), state.buildings(), building) else {
        return Vec::new();
    };
    state
        .buildings()
        .iter()
        .filter(|(id, s)| *id != building && storage::is_operational_storage(defs, s))
        .filter(|(id, s)| {
            let radius = defs.buildings().get(s.kind().index()).and_then(|d| d.storage.as_ref()).map_or(0, |d| d.carrier_radius);
            roads::building_component(state.world(), state.roads(), state.buildings(), *id) == Some(component)
                && s.rect().distance_to_rect(&b.rect()) <= i32::from(radius)
        })
        .map(|(id, _)| id)
        .collect()
}

/// Production window data of a building (`None` without production).
pub fn production_report(state: &GameState, defs: &Defs, building: BuildingId) -> Option<ProductionReport> {
    let b = state.building(building)?;
    let p = b.production()?;
    let def = defs.buildings().get(b.kind().index())?;
    let pdef = def.production.as_ref()?;
    let slots = |inv: &crate::economy::Inventory| -> Vec<BufferSlot> {
        let mut list: Vec<ResourceId> = inv.capacities().map(|(r, _)| r).chain(inv.iter().map(|m| m.resource)).collect();
        list.sort();
        list.dedup();
        list.into_iter()
            .map(|r| BufferSlot {
                resource: r,
                amount: inv.get(r),
                capacity: inv.capacity(r),
                incoming: state.logistics().incoming(building, r),
            })
            .collect()
    };
    Some(ProductionReport {
        building,
        kind: b.kind(),
        status: p.status(),
        status_text: status_text(defs, p.status()),
        recipes: pdef
            .recipes
            .iter()
            .enumerate()
            .map(|(i, r)| RecipeInfo {
                index: i as u8,
                name: r.name.clone(),
                inputs: r.inputs.clone(),
                outputs: r.outputs.clone(),
                hours: r.hours,
            })
            .collect(),
        active_recipe: p.active_recipe(),
        queue: p.queue().to_vec(),
        // Field farms work tile by tile instead of in cycles.
        progress: p.cycle().map_or_else(
            || state.agriculture().farm(building).and_then(|f| f.task()).map_or(0.0, |t| t.fraction()),
            |c| c.fraction(),
        ),
        productivity_percent: p.productivity_permille() / 10,
        staffed: b.staffed(),
        present: crate::population::present_workers(state, defs, building),
        training_percent: crate::production::crew_training_permille(state, defs, def, building) / 10,
        commute: commute_report(state, defs, building),
        workers: def.workers,
        priority: b.priority(),
        uses_tools: pdef.uses_tools,
        tool_charge: p.tool_charge(),
        has_road_access: state.has_road_access(building),
        serving_storages: serving_storages(state, defs, building).len() as u32,
        input: slots(p.input()),
        output: slots(p.output()),
        cycles_completed: p.cycles_completed(),
        carrier_wait: carrier_wait(state, building),
        event_modifier_percent: state.events().production_modifier_percent(state.clock().tick),
    })
}

/// One good a selling building (tavern) keeps for its guests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopGood {
    /// The good.
    pub resource: ResourceId,
    /// German name.
    pub name: String,
    /// Units in stock.
    pub stock: u32,
    /// Units it keeps at most.
    pub capacity: u32,
}

/// Stock of a building that sells goods to households (tavern: beer, docs/game_design.md
/// §5.4): carriers refill it from the storages, guests buy from it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopReport {
    /// Goods with stock and capacity.
    pub goods: Vec<ShopGood>,
    /// German problem line when a good is sold out ("Kein Bier vorrätig"); `None` if
    /// everything is in stock.
    pub problem: Option<String>,
}

/// The stock of selling building `building`; `None` if it sells nothing or does not exist.
pub fn shop_report(state: &GameState, defs: &Defs, building: BuildingId) -> Option<ShopReport> {
    let b = state.building(building)?;
    let def = defs.buildings().get(b.kind().index())?;
    if def.sells.is_empty() {
        return None;
    }
    let goods: Vec<ShopGood> = def
        .sells
        .iter()
        .map(|r| ShopGood {
            resource: *r,
            name: defs.resources().get(r.index()).map_or_else(|| r.to_string(), |d| d.name.clone()),
            stock: b.inventory().get(*r),
            capacity: b.inventory().capacity(*r),
        })
        .collect();
    let missing: Vec<&str> = goods.iter().filter(|g| g.stock == 0).map(|g| g.name.as_str()).collect();
    let problem = (!missing.is_empty() && b.is_complete()).then(|| format!("Kein {} vorrätig", missing.join(" und kein ")));
    Some(ShopReport { goods, problem })
}

/// One storage building in the storage window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageReport {
    /// The building.
    pub building: BuildingId,
    /// Its type.
    pub kind: BuildingTypeId,
    /// True if it is part of the kingdom stock.
    pub reachable: bool,
    /// Carriers it provides.
    pub carriers: u32,
    /// Carriers on a trip.
    pub busy_carriers: u32,
    /// Carriers waiting at the storage for a job.
    pub idle_carriers: u32,
    /// Jobs of the storage waiting for a carrier.
    pub waiting_jobs: u32,
    /// Contents with capacities (accepted resources and held units).
    pub contents: Vec<BufferSlot>,
}

/// All finished storage buildings.
pub fn storage_report(state: &GameState, defs: &Defs) -> Vec<StorageReport> {
    let reachable = storage::reachable_storages(state, defs);
    state
        .buildings()
        .iter()
        .filter(|(_, b)| b.is_complete() && defs.buildings().get(b.kind().index()).is_some_and(|d| d.storage.is_some()))
        .map(|(id, b)| {
            let inv = b.inventory();
            let contents = inv
                .capacities()
                .map(|(r, capacity)| BufferSlot {
                    resource: r,
                    amount: inv.get(r),
                    capacity,
                    incoming: state.logistics().incoming(id, r),
                })
                .collect();
            StorageReport {
                building: id,
                kind: b.kind(),
                reachable: reachable.contains(&id),
                carriers: carriers::carriers_of(state, defs, id),
                busy_carriers: state.logistics().busy_carriers(id),
                idle_carriers: carriers::idle_carriers(state, defs, id),
                waiting_jobs: u32::try_from(state.logistics().waiting().filter(|j| j.carrier_base == id).count())
                    .unwrap_or(u32::MAX),
                contents,
            }
        })
        .collect()
}

/// Finance window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceReport {
    /// Treasury in gold.
    pub treasury: i64,
    /// True while bankrupt.
    pub bankrupt: bool,
    /// Tax rate in percent.
    pub tax_rate_percent: u8,
    /// Running day.
    pub today: DayLedger,
    /// Closed days, oldest first (at most 48).
    pub history: Vec<DayLedger>,
    /// Current daily rates (projection of taxes, fees, wages, upkeep).
    pub rates: DailyRates,
}

/// Ledger, history and projected daily rates.
pub fn finance_report(state: &GameState, defs: &Defs) -> FinanceReport {
    let e = state.economy();
    FinanceReport {
        treasury: e.treasury(),
        bankrupt: e.is_bankrupt(),
        tax_rate_percent: state.policy().tax_rate_percent,
        today: *e.ledger().today(),
        history: e.ledger().history().copied().collect(),
        rates: finance::daily_rates(state, defs),
    }
}

pub use crate::events::festival::{FestivalReport, festival_report};

/// The ruler's court (docs/game_design.md §7 Feste und Hofhaltung): the daily court upkeep of
/// the rank (part of [`DailyRates::upkeep`]) and the festival the player can hold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourtReport {
    /// Name of the current rank.
    pub rank: String,
    /// Court upkeep of the rank in gold per day (booked with the upkeep).
    pub upkeep_per_day: u32,
    /// Court upkeep of the next rank (`None` at the top).
    pub next_upkeep_per_day: Option<u32>,
    /// German line, e.g. „Hofhaltung als Großstadt: 80 Gold je Tag (im Unterhalt enthalten)“.
    pub upkeep_text: String,
    /// The festival.
    pub festival: FestivalReport,
}

/// The court window (see [`CourtReport`]).
pub fn court_report(state: &GameState, defs: &Defs) -> CourtReport {
    let rank = usize::from(state.progress().rank());
    let name = defs.ranks().get(rank).map_or_else(String::new, |r| r.name.clone());
    let upkeep = crate::ranks::court_upkeep(state, defs);
    let upkeep_text = if upkeep == 0 {
        format!("Hofhaltung als {name}: keine Kosten")
    } else {
        format!("Hofhaltung als {name}: {upkeep} Gold je Tag (im Unterhalt enthalten)")
    };
    CourtReport {
        rank: name,
        upkeep_per_day: upkeep,
        next_upkeep_per_day: defs.ranks().get(rank + 1).map(|r| r.court_upkeep),
        upkeep_text,
        festival: festival_report(state, defs),
    }
}

/// Population, housing and jobs.
pub fn workforce_report(state: &GameState, defs: &Defs) -> WorkforceSummary {
    workforce::summary(state, defs)
}

/// One trade order with its prices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeRow {
    /// The order.
    pub order: TradeOrder,
    /// Kingdom stock of the resource.
    pub stock: u32,
    /// Market price in centi-gold.
    pub price_centi: u32,
    /// What one unit costs when bought, in centi-gold.
    pub buy_price_centi: u32,
    /// What one unit earns when sold, in centi-gold.
    pub sell_price_centi: u32,
    /// Units traded today.
    pub today: TradeVolume,
}

/// Trade window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeReport {
    /// The trading post executing the orders (`None`: orders are inactive).
    pub trading_post: Option<BuildingId>,
    /// All orders in resource order.
    pub orders: Vec<TradeRow>,
    /// Trade road bonus of the post's road route to the map edge in percent (world market;
    /// caravans get the share of their own walk, [`crate::diplomacy::Caravan::route`]).
    pub trade_road_percent: i32,
    /// German line explaining the trade road bonus.
    pub trade_road_text: String,
}

/// Trade orders, prices and the active trading post.
pub fn trade_report(state: &GameState, defs: &Defs) -> TradeReport {
    let e = state.economy();
    let margin = defs.rules().trade_margin_percent;
    let post = trade::active_trading_post(state, defs);
    // The route of the active post, else of the first finished one (not staffed yet).
    let route_post = post.or_else(|| {
        state.buildings().iter().find_map(|(id, b)| {
            (b.is_complete() && defs.buildings().get(b.kind().index()).is_some_and(|d| d.role == crate::defs::BuildingRole::TradeHub))
                .then_some(id)
        })
    });
    let road = route_post.map_or(0, |p| trade::trade_route_percent(state, defs, p));
    let bonus = trade::trade_bonus_percent(state.research(), defs) + road;
    let orders = e
        .trade()
        .orders()
        .map(|o| {
            let price = e.prices().centi(o.resource);
            TradeRow {
                order: *o,
                stock: e.stock().get(o.resource),
                price_centi: price,
                buy_price_centi: trade::trade_unit_price_centi(price, TradeDirection::Import, margin, bonus),
                sell_price_centi: trade::trade_unit_price_centi(price, TradeDirection::Export, margin, bonus),
                today: e.trade().volume_today(o.resource),
            }
        })
        .collect();
    let trade_road_text = if road > 0 {
        format!("Handelsstraßen zum Kartenrand: Preise {road} % besser.")
    } else {
        "Keine Handelsstraße vom Handelskontor zum Kartenrand – Handelsstraßen verbessern die Preise.".to_owned()
    };
    TradeReport { trading_post: post, orders, trade_road_percent: road, trade_road_text }
}

/// Logistics overview (carriers walk with the goods, step 3b).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogisticsReport {
    /// Jobs waiting for a carrier.
    pub waiting: u32,
    /// Carriers on the way to pick goods up.
    pub fetching: u32,
    /// Carriers with goods on the way to the target.
    pub delivering: u32,
    /// Storage carriers walking back to their storage.
    pub returning: u32,
    /// Carriers of all operational storages.
    pub carriers: u32,
    /// Carriers on a trip (fetching or delivering).
    pub busy_carriers: u32,
    /// Carriers waiting at their storage for a job.
    pub idle_carriers: u32,
    /// Units on carriers right now (per resource: [`crate::logistics::Logistics::goods_in_transit`]).
    pub units_in_transit: u32,
    /// Gatherers carrying their yield back to their building.
    pub hauls: u32,
    /// Average duration of the recent carrier trips (taking the job → delivery) in game
    /// minutes (`None` before the first one).
    pub average_trip_minutes: Option<u32>,
    /// Carrier trips completed since the game started.
    pub trips_completed: u64,
}

/// Counts of jobs and carriers, goods in transit and the average trip time.
pub fn logistics_report(state: &GameState, defs: &Defs) -> LogisticsReport {
    let mut r = LogisticsReport::default();
    let l = state.logistics();
    for job in l.jobs() {
        match (job.kind, job.stage) {
            (TransportKind::Haul, _) => r.hauls += 1,
            (_, JobStage::Waiting) => r.waiting += 1,
            (_, JobStage::Fetching { .. }) => r.fetching += 1,
            (_, JobStage::Carrying { .. }) => r.delivering += 1,
        }
        if job.is_delivering() {
            r.units_in_transit += job.amount;
        }
    }
    for (id, b) in state.buildings().iter() {
        if storage::is_operational_storage(defs, b) {
            r.carriers += carriers::carriers_of(state, defs, id);
            r.busy_carriers += l.busy_carriers(id);
            r.idle_carriers += carriers::idle_carriers(state, defs, id);
            r.returning += returning_carriers(state, id);
        }
    }
    r.average_trip_minutes = l.average_trip_ticks().map(ticks_to_minutes);
    r.trips_completed = l.trips_completed();
    r
}

/// Carriers of storage `base` without a trip who walk back to it.
fn returning_carriers(state: &GameState, base: BuildingId) -> u32 {
    let pop = state.population();
    let n = pop
        .workers_of(base)
        .iter()
        .filter_map(|c| pop.citizen(*c))
        .filter(|c| {
            c.carry().is_none()
                && matches!(c.motion().phase(), TripPhase::Walking | TripPhase::Queued)
                && c.motion().destination() == Some(Destination::Building { building: base, purpose: Purpose::Work })
        })
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Transports of a building waiting for a carrier ("Wartet auf Träger").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarrierWait {
    /// Waiting jobs from or to the building.
    pub jobs: u32,
    /// Units they move.
    pub units: u32,
    /// Game minutes the oldest of them waits.
    pub oldest_minutes: u32,
    /// German summary, e.g. "Wartet auf Träger: 2 Transporte (12 Einheiten) seit 35 min"
    /// ("seit 15 h 36 min" from an hour on).
    pub text: String,
}

/// Transports from or to `building` (production, construction site, storage) waiting for a
/// carrier; `None` if none waits.
pub fn carrier_wait(state: &GameState, building: BuildingId) -> Option<CarrierWait> {
    let now = state.clock().tick;
    let waiting: Vec<_> = state.logistics().waiting().filter(|j| j.source == building || j.target == building).collect();
    let oldest = waiting.iter().map(|j| j.created_at).min()?;
    let jobs = u32::try_from(waiting.len()).unwrap_or(u32::MAX);
    let units = waiting.iter().map(|j| j.amount).sum();
    let oldest_minutes = ticks_to_minutes(now.saturating_sub(oldest));
    let what = if jobs == 1 { "1 Transport".to_string() } else { format!("{jobs} Transporte") };
    let amount = if units == 1 { "1 Einheit".to_string() } else { format!("{units} Einheiten") };
    let text = format!("Wartet auf Träger: {what} ({amount}) seit {}", minutes_text(oldest_minutes));
    Some(CarrierWait { jobs, units, oldest_minutes, text })
}
