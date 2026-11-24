//! Household supply (docs/game_design.md §6, §7): households restock their pantry on the
//! evening errand (step 3b), and once per game hour every household uses one hour of food,
//! fuel and goods.
//!
//! * **Restocking** happens where the household's errand runner (its first adult, §6) is:
//!   arriving at the **market** in reach of the house, the household gets food, winter fuel and
//!   the goods of its housing tier (and of the next tier, unless upgrades are locked) from the
//!   kingdom stock — the market's stalls draw on the reachable storages nearest to the house.
//!   Without a market in reach the runner fetches only food and fuel from the nearest
//!   reachable storage whose carrier radius covers the house ([`restock_on_errand`]).
//!   Homeless households camp at the main headquarters (a storage) and restock there every
//!   hour from any reachable storage.
//! * **Pantry:** about `pantry_days` of consumption (enough until the next evening's errand);
//!   taken units count as consumption in the resource statistics (`consumed` and
//!   `household`). Committed construction material is not taken – except fuel by a household
//!   that has none on a cold day (a day's worth; heating comes first, game_design §6.1).
//! * **Heating comes first:** in the heating season workshops get fuel only beyond
//!   `population.fuel_reserve_days` of the households' demand (`crate::logistics::planner`); a
//!   household out of fuel goes for some at once; fuel for fewer than
//!   `population.fuel_warning_days` is reported once per spell.
//! * **Use per hour:** food by saturation (§4: adults `food_per_day`, children
//!   `child_food_percent`, winter `winter_food_consumption_percent`), eaten from the kind with
//!   the most saturation left (so variety is kept); fuel by heating value on cold days (below
//!   `climate.temperature.heating_below_c`, × the weather's heating demand, [`crate::weather`]);
//!   goods at
//!   `household_use` per resident and day.
//! * The resulting [`SupplyState`] (fed, food variety, fuel, clothes, missing goods, service
//!   coverage, fresh water) drives the citizens' needs until the next hourly update.

use crate::buildings::{BuildingId, FRESH_WATER_RADIUS};
use crate::defs::{Defs, GoodsRequirement, ResourceCategory, ServiceKind};
use crate::economy::storage;
use crate::event::{Notification, NotificationKind, SimEvent, Severity};
use crate::grid::TileRect;
use crate::state::GameState;
use crate::time::{Season, TICKS_PER_HOUR};

use super::{HouseholdId, LifeStage, Notice, SupplyState, demography, service_bit};

/// Most units a household takes per resource kind and hour (keeps one update cheap).
const MAX_UNITS_PER_RESTOCK: u32 = 64;

/// A building providing an area service.
#[derive(Clone, Copy, Debug)]
struct Provider {
    kind: ServiceKind,
    rect: TileRect,
    radius: i32,
    staffed: bool,
    /// Has every good it sells in stock (true for buildings selling nothing).
    stocked: bool,
}

/// All operational service providers, collected once per use (cheap queries for many houses).
/// A provider with jobs serves only while staffed; the administration of a headquarters also
/// works unstaffed (taxes).
#[derive(Clone, Debug, Default)]
pub(crate) struct ServiceIndex {
    providers: Vec<Provider>,
}

impl ServiceIndex {
    /// Collects the operational providers of every service.
    pub(crate) fn collect(state: &GameState, defs: &Defs) -> Self {
        let mut providers = Vec::new();
        for (_, b) in state.buildings.iter() {
            if !b.is_operational() {
                continue;
            }
            let Some(def) = defs.buildings().get(b.kind().index()) else { continue };
            let staffed = def.workers == 0 || b.trade_staff() > 0;
            let stocked = def.sells.iter().all(|r| b.inventory.get(*r) > 0);
            for s in &def.services {
                // Researched `ServiceRadiusBonus` effects widen the radius.
                let radius = s.radius.saturating_add(state.research.service_radius_bonus(defs, def.id));
                providers.push(Provider { kind: s.kind, rect: b.rect(), radius: i32::from(radius), staffed, stocked });
            }
        }
        Self { providers }
    }

    /// True if a staffed provider of `kind` reaches `rect` (administration: any provider).
    pub(crate) fn covers(&self, kind: ServiceKind, rect: &TileRect) -> bool {
        let need_staff = kind != ServiceKind::Administration;
        self.providers
            .iter()
            .any(|p| p.kind == kind && (p.staffed || !need_staff) && p.rect.distance_to_rect(rect) <= p.radius)
    }

    /// True if a staffed provider of `kind` with its goods in stock reaches `rect`.
    pub(crate) fn covers_stocked(&self, kind: ServiceKind, rect: &TileRect) -> bool {
        self.providers.iter().any(|p| p.kind == kind && p.staffed && p.stocked && p.rect.distance_to_rect(rect) <= p.radius)
    }

    /// Bit mask of every service reaching `rect` (see [`SupplyState::services`]).
    pub(crate) fn mask(&self, rect: &TileRect) -> u16 {
        let mut mask = 0;
        for p in &self.providers {
            let need_staff = p.kind != ServiceKind::Administration;
            if (p.staffed || !need_staff) && p.rect.distance_to_rect(rect) <= p.radius {
                mask |= service_bit(p.kind);
            }
        }
        mask
    }
}

/// A reachable storage.
#[derive(Clone, Copy, Debug)]
struct StorageSpot {
    id: BuildingId,
    rect: TileRect,
    carrier_radius: i32,
}

/// Data shared by the household updates of one tick (built on first use).
#[derive(Default)]
pub(crate) struct TickContext {
    services: Option<ServiceIndex>,
    storages: Option<Vec<StorageSpot>>,
    /// Units per resource that households may still take this tick (stock − committed).
    available: Option<Vec<u32>>,
    /// True if goods were taken from storages (the stock view needs a refresh).
    pub stock_changed: bool,
}

impl TickContext {
    fn prepare(&mut self, state: &GameState, defs: &Defs) {
        if self.services.is_none() {
            self.services = Some(ServiceIndex::collect(state, defs));
        }
        if self.storages.is_none() {
            let spots = storage::reachable_storages(state, defs)
                .into_iter()
                .filter_map(|id| {
                    let b = state.buildings.get(id)?;
                    let radius = defs.buildings().get(b.kind().index())?.storage.as_ref()?.carrier_radius;
                    Some(StorageSpot { id, rect: b.rect(), carrier_radius: i32::from(radius) })
                })
                .collect();
            self.storages = Some(spots);
        }
        if self.available.is_none() {
            self.available = Some(defs.resources().iter().map(|r| state.economy.available(r.id)).collect());
        }
    }
}

/// Runs the hourly update (and, once a day, the demography) of the households whose slot is
/// due this tick.
pub(crate) fn update_households(
    state: &mut GameState,
    defs: &Defs,
    ctx: &mut TickContext,
    events: &mut Vec<SimEvent>,
) {
    let slot = state.clock.tick % TICKS_PER_HOUR;
    let hour = state.clock.total_hours() % 24;
    let count = state.population.households.slot_count() as u64;
    let mut index = slot;
    while index < count {
        if let Some(id) = u32::try_from(index).ok().and_then(|i| state.population.households.id_at(i)) {
            ctx.prepare(state, defs);
            hourly(state, defs, ctx, id, events);
            if index % 24 == hour {
                demography::daily_household(state, defs, id, events);
            }
        }
        index += TICKS_PER_HOUR;
    }
}

/// Rect a household lives at: its house or, homeless, the camp at the headquarters.
pub(crate) fn household_rect(state: &GameState, defs: &Defs, household: HouseholdId) -> TileRect {
    state
        .population
        .household(household)
        .and_then(|h| h.home)
        .and_then(|b| state.buildings.get(b))
        .map_or_else(|| crate::workforce::camp_rect(state, defs), |b| b.rect())
}

/// Goods requirements a household wants supplied: those of its house tier and, unless the
/// house's upgrades are locked, those of the next tier (§5.1: upgrades need the goods).
pub(crate) fn wanted_goods<'a>(state: &GameState, defs: &'a Defs, home: Option<BuildingId>) -> (Vec<&'a GoodsRequirement>, usize) {
    let Some(b) = home.and_then(|id| state.buildings.get(id)) else { return (Vec::new(), 0) };
    let Some(def) = defs.buildings().get(b.kind().index()) else { return (Vec::new(), 0) };
    let Some(housing) = def.housing.as_ref() else { return (Vec::new(), 0) };
    let mut wanted: Vec<&GoodsRequirement> = housing.requirements.goods.iter().collect();
    let own = wanted.len();
    if !b.is_upgrade_locked()
        && let Some(next) = def.upgrade.as_ref().and_then(|u| defs.buildings().get(u.to.index())).and_then(|d| d.housing.as_ref())
    {
        for g in &next.requirements.goods {
            if !wanted.contains(&g) {
                wanted.push(g);
            }
        }
    }
    (wanted, own)
}

/// Milli-saturation a household eats per hour.
fn food_need_per_hour(state: &GameState, defs: &Defs, household: HouseholdId) -> u64 {
    let rules = defs.rules();
    let pop = &rules.population;
    let Some(h) = state.population.household(household) else { return 0 };
    let per_adult = f64::from(pop.food_per_day) * 1000.0 / 24.0;
    let winter = if state.clock.season() == Season::Winter { f64::from(rules.winter_food_consumption_percent) / 100.0 } else { 1.0 };
    let total: f64 = h
        .members
        .iter()
        .filter_map(|m| state.population.citizen(*m))
        .map(|c| if c.stage == LifeStage::Child { per_adult * f64::from(pop.child_food_percent) / 100.0 } else { per_adult })
        .sum();
    (total * winter).round() as u64
}

/// Saturation per unit of a resource in permille (0 = not edible).
fn nutrition_pm(defs: &Defs, r: crate::defs::ResourceId) -> u64 {
    defs.resources().get(r.index()).map_or(0, |d| (f64::from(d.nutrition) * 1000.0).round().max(0.0) as u64)
}

/// Heating value per unit in permille (0 = not a fuel).
fn heating_pm(defs: &Defs, r: crate::defs::ResourceId) -> u64 {
    defs.resources().get(r.index()).map_or(0, |d| (f64::from(d.heating_value) * 1000.0).round().max(0.0) as u64)
}

/// Value (milli-saturation or milli-heat) of the pantry items `per_unit` applies to.
fn pantry_value(state: &GameState, household: HouseholdId, per_unit: impl Fn(crate::defs::ResourceId) -> u64) -> u64 {
    state.population.household(household).map_or(0, |h| {
        h.pantry.iter().map(|p| u64::from(p.milli) * per_unit(p.resource) / 1000).sum()
    })
}

/// The errand runner of household `id` arrived at `place` (its market, or without one the
/// storage covering its house): the household restocks its pantry (see the module
/// documentation).
pub(crate) fn restock_on_errand(state: &mut GameState, defs: &Defs, id: HouseholdId, place: BuildingId) {
    restock_errands(state, defs, &[(id, place)]);
}

/// [`restock_on_errand`] for all errands that ended in one movement step, in arrival order,
/// sharing the service index, storage list and stock view of the step.
pub(crate) fn restock_errands(state: &mut GameState, defs: &Defs, errands: &[(HouseholdId, BuildingId)]) {
    if errands.is_empty() {
        return;
    }
    let mut ctx = TickContext::default();
    ctx.prepare(state, defs);
    for &(id, place) in errands {
        let Some(rect) = state.population.household(id).map(|_| household_rect(state, defs, id)) else { continue };
        let has_market = ctx.services.as_ref().is_some_and(|s| s.mask(&rect) & service_bit(ServiceKind::Market) != 0);
        let Some(sources) = errand_sources(state, defs, &ctx, &rect, has_market, place) else { continue };
        restock(state, defs, &mut ctx, id, has_market, &sources);
    }
    if ctx.stock_changed {
        crate::economy::storage::refresh_stock(state, defs);
    }
}

/// Where the errand runner of household `id` is right now: inside the building of its errand.
fn errand_place(state: &GameState, id: HouseholdId) -> Option<BuildingId> {
    let h = state.population.household(id)?;
    h.members.iter().filter_map(|m| state.population.citizen(*m)).find_map(|c| match c.motion.dest {
        Some(super::Destination::Building { building, purpose: super::Purpose::Errand })
            if c.motion.inside == Some(building) && c.motion.phase == super::TripPhase::Still =>
        {
            Some(building)
        }
        _ => None,
    })
}

/// Storages an errand at `place` draws from: at a market reaching the house the reachable
/// storages nearest to it, at a storage that storage; `None` elsewhere.
fn errand_sources(
    state: &GameState,
    defs: &Defs,
    ctx: &TickContext,
    rect: &TileRect,
    has_market: bool,
    place: BuildingId,
) -> Option<Vec<BuildingId>> {
    let at_market = state
        .buildings
        .get(place)
        .and_then(|b| defs.buildings().get(b.kind().index()))
        .is_some_and(|d| d.services.iter().any(|s| s.kind == ServiceKind::Market));
    if at_market && has_market {
        Some(supply_sources(ctx, rect, true, false))
    } else if ctx.storages.as_ref().is_some_and(|l| l.iter().any(|s| s.id == place)) {
        Some(vec![place])
    } else {
        None
    }
}

/// True if an errand at `place` would find food: at a market any edible good in the kingdom
/// stock, at a storage edible goods in it.
pub(crate) fn offers_food(state: &GameState, defs: &Defs, place: BuildingId) -> bool {
    let Some(b) = state.buildings.get(place) else { return false };
    let market = defs.buildings().get(b.kind().index()).is_some_and(|d| d.services.iter().any(|s| s.kind == ServiceKind::Market));
    defs.resources()
        .iter()
        .filter(|r| r.is_edible())
        .any(|r| if market { state.economy.available(r.id) > 0 } else { b.inventory.get(r.id) > 0 })
}

/// True if the pantry of household `id` holds no food (its runner goes shopping at once,
/// outside working time).
pub(crate) fn pantry_without_food(state: &GameState, defs: &Defs, id: HouseholdId) -> bool {
    state.population.household(id).is_some_and(|h| {
        !h.pantry.iter().any(|p| defs.resources().get(p.resource.index()).is_some_and(|r| r.is_edible()))
    })
}

/// True if a fuel (wood, coal) is in reach of an errand at `place`: at a market anywhere in
/// the kingdom stock, at a storage in it (construction material included: heating comes
/// first, docs/game_design.md §6.1).
pub(crate) fn offers_fuel(state: &GameState, defs: &Defs, place: BuildingId) -> bool {
    let Some(b) = state.buildings.get(place) else { return false };
    let market = defs.buildings().get(b.kind().index()).is_some_and(|d| d.services.iter().any(|s| s.kind == ServiceKind::Market));
    defs.resources()
        .iter()
        .filter(|r| r.heating_value > 0.0)
        .any(|r| if market { state.economy.stock().get(r.id) > 0 } else { b.inventory.get(r.id) > 0 })
}

/// True if housed household `id` needs heating today and its pantry holds no fuel (its runner
/// goes for some at once, outside working time).
pub(crate) fn pantry_without_fuel(state: &GameState, defs: &Defs, id: HouseholdId) -> bool {
    state.weather.needs_heating(defs.rules(), state.clock)
        && state.population.household(id).is_some_and(|h| {
            h.home.is_some() && !h.pantry.iter().any(|p| defs.resources().get(p.resource.index()).is_some_and(|r| r.heating_value > 0.0))
        })
}

/// True on days the households heat or stock up for it: cold days (below
/// `climate.temperature.heating_below_c`) and the last day of autumn.
pub(crate) fn heating_season(state: &GameState, defs: &Defs) -> bool {
    let clock = state.clock;
    state.weather.needs_heating(defs.rules(), clock)
        || (clock.season() == Season::Autumn && clock.day_of_season() == crate::time::DAYS_PER_SEASON)
}

/// Heating value all households burn per day at today's heating demand (in thousandths; on
/// the last day of autumn at plain demand).
pub fn heating_demand_milli_per_day(state: &GameState, defs: &Defs) -> u64 {
    let rules = defs.rules();
    let percent = if state.weather.needs_heating(rules, state.clock) { u64::from(state.weather.heating_percent(rules)) } else { 100 };
    let residents = state.population.len() as u64;
    (f64::from(rules.population.fuel_per_resident_day) * 1000.0).round() as u64 * residents * percent / 100
}

/// Heating value of the fuel in the kingdom stock, in thousandths (construction material
/// included, docs/game_design.md §6.1).
pub fn fuel_stock_milli(state: &GameState, defs: &Defs) -> u64 {
    defs.resources()
        .iter()
        .filter(|r| r.heating_value > 0.0)
        .map(|r| u64::from(state.economy.stock().get(r.id)) * heating_pm(defs, r.id))
        .sum()
}

/// Heating value (thousandths) the households keep from the workshops: in the heating season
/// `population.fuel_reserve_days` days of their demand, otherwise none (docs/game_design.md
/// §6.1 Heizen geht vor).
pub(crate) fn fuel_reserve_milli(state: &GameState, defs: &Defs) -> u64 {
    if !heating_season(state, defs) {
        return 0;
    }
    let days = f64::from(defs.rules().population.fuel_reserve_days);
    (heating_demand_milli_per_day(state, defs) as f64 * days).round() as u64
}

/// Warns once per spell (docs/game_design.md §2 Meldungen) when, in the heating season, the
/// fuel in stock lasts fewer than `population.fuel_warning_days` days at today's demand.
pub(crate) fn warn_fuel_shortage(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    if !heating_season(state, defs) {
        return;
    }
    let demand = heating_demand_milli_per_day(state, defs);
    if demand == 0 {
        return;
    }
    let stock = fuel_stock_milli(state, defs);
    let warning = (demand as f64 * f64::from(defs.rules().population.fuel_warning_days)).round() as u64;
    if stock >= warning {
        return;
    }
    let text = fuel_shortage_text(stock / demand, state.weather.needs_heating(defs.rules(), state.clock));
    let note = Notification::new(Severity::Warning, text).with_kind(NotificationKind::NoFuel);
    let tick = state.clock.tick;
    state.population.notify_waiting(defs, Notice::FuelShortage, tick, note, events);
}

/// German text of the fuel warning: `days` whole days of fuel left; `cold`: it is cold today
/// (else the winter is about to begin).
pub(crate) fn fuel_shortage_text(days: u64, cold: bool) -> String {
    let lasts = match days {
        0 => "nicht einmal mehr einen Tag".to_owned(),
        1 => "nur noch etwa einen Tag".to_owned(),
        n => format!("nur noch etwa {n} Tage"),
    };
    let head = if cold { "Brennholz wird knapp" } else { "Der Winter naht" };
    format!(
        "{head}: Holz und Kohle reichen zum Heizen {lasts}. Baut Holzfäller und Förster oder kauft Holz – Werkstätten bekommen Brennstoff nur über den Heizvorrat hinaus."
    )
}

/// Storages a household takes goods from, nearest to `rect` first: with a market all reachable
/// storages, otherwise only those whose carriers reach the house (homeless: all).
fn supply_sources(ctx: &TickContext, rect: &TileRect, has_market: bool, homeless: bool) -> Vec<BuildingId> {
    let mut sources: Vec<(i32, BuildingId)> = ctx
        .storages
        .as_ref()
        .map(|list| {
            list.iter()
                .filter(|s| has_market || homeless || s.rect.distance_to_rect(rect) <= s.carrier_radius)
                .map(|s| (s.rect.distance_to_rect(rect), s.id))
                .collect()
        })
        .unwrap_or_default();
    sources.sort();
    sources.into_iter().map(|(_, id)| id).collect()
}

/// The storage the errand runner of a household without a market in reach fetches food and
/// fuel from: the nearest reachable storage whose carrier radius covers `rect` (ties: lower id).
pub(crate) fn errand_storage(state: &GameState, defs: &Defs, rect: &TileRect) -> Option<BuildingId> {
    storage::reachable_storages(state, defs)
        .into_iter()
        .filter_map(|id| {
            let b = state.buildings.get(id)?;
            let radius = i32::from(defs.buildings().get(b.kind().index())?.storage.as_ref()?.carrier_radius);
            let d = b.rect().distance_to_rect(rect);
            (d <= radius).then_some((d, id))
        })
        .min()
        .map(|(_, id)| id)
}

/// Fills the pantry of household `id` from `sources` up to `pantry_days` of food, winter fuel
/// and (with a market) the wanted goods.
fn restock(state: &mut GameState, defs: &Defs, ctx: &mut TickContext, id: HouseholdId, has_market: bool, sources: &[BuildingId]) {
    ctx.prepare(state, defs);
    let rules = defs.rules();
    let pop = &rules.population;
    let Some(h) = state.population.household(id) else { return };
    let home = h.home;
    let residents = u64::from(h.size());
    // Heating by the day's temperature (docs/game_design.md §8.1).
    let cold = state.weather.needs_heating(rules, state.clock);
    let (wanted, _) = wanted_goods(state, defs, home);
    let sources = sources.to_vec();
    // --- restock -------------------------------------------------------------------------
    let food_hour = food_need_per_hour(state, defs, id);
    let food_target = (food_hour as f64 * 24.0 * f64::from(pop.pantry_days)).round() as u64;
    // Kinds the pantry lacks first (then the scarcest), so meals vary over time.
    let mut edible: Vec<(u32, crate::defs::ResourceId)> = defs
        .resources()
        .iter()
        .filter(|r| r.is_edible())
        .map(|r| (state.population.household(id).map_or(0, |h| h.pantry_milli(r.id)), r.id))
        .collect();
    edible.sort();
    let edible: Vec<crate::defs::ResourceId> = edible.into_iter().map(|(_, r)| r).collect();
    let mut have = pantry_value(state, id, |r| nutrition_pm(defs, r));
    let mut rounds = 0;
    while have < food_target && rounds < MAX_UNITS_PER_RESTOCK {
        rounds += 1;
        let mut took_any = false;
        for r in &edible {
            if have >= food_target {
                break;
            }
            if take_unit(state, ctx, &sources, id, *r, Payment::Basic) {
                have += nutrition_pm(defs, *r);
                took_any = true;
            }
        }
        if !took_any {
            break;
        }
    }
    // Fuel in winter; on the last day of autumn already for the first winter night.
    let clock = state.clock;
    let heating_soon =
        cold || (clock.season() == Season::Autumn && clock.day_of_season() == crate::time::DAYS_PER_SEASON);
    let fuel_hour = if heating_soon {
        // Before the cold the weather's heating demand does not apply yet: plain demand.
        let heating = if cold { f64::from(state.weather.heating_percent(rules)) / 100.0 } else { 1.0 };
        (f64::from(pop.fuel_per_resident_day) * 1000.0 * residents as f64 * heating / 24.0).round() as u64
    } else {
        0
    };
    if heating_soon {
        let target = (fuel_hour as f64 * 24.0 * f64::from(pop.pantry_days)).round() as u64;
        let fuels: Vec<crate::defs::ResourceId> =
            defs.resources().iter().filter(|r| r.heating_value > 0.0).map(|r| r.id).collect();
        let mut have = pantry_value(state, id, |r| heating_pm(defs, r));
        let mut n = 0;
        while have < target && n < MAX_UNITS_PER_RESTOCK {
            n += 1;
            // The fuel with the most units available first (keeps wood for construction).
            let available = |r: &crate::defs::ResourceId| {
                ctx.available.as_ref().and_then(|a| a.get(r.index()).copied()).unwrap_or(0)
            };
            let mut order: Vec<(std::cmp::Reverse<u32>, crate::defs::ResourceId)> =
                fuels.iter().map(|r| (std::cmp::Reverse(available(r)), *r)).filter(|(a, _)| a.0 > 0).collect();
            order.sort();
            let Some(r) = order.into_iter().map(|(_, r)| r).find(|r| take_unit(state, ctx, &sources, id, *r, Payment::Basic)) else {
                break;
            };
            have += heating_pm(defs, r);
        }
        // Heating comes first (docs/game_design.md §6.1): a household without any fuel takes
        // a day's worth even from fuel promised to construction sites (they wait for more).
        if have == 0 && cold {
            let day = fuel_hour * 24;
            let mut n = 0;
            while have < day && n < MAX_UNITS_PER_RESTOCK {
                n += 1;
                let Some(r) = fuels.iter().copied().find(|r| take_committed_unit(state, ctx, &sources, id, *r)) else { break };
                have += heating_pm(defs, r);
            }
        }
    }
    if has_market {
        for g in &wanted {
            let use_milli = goods_use_per_day_milli(defs, g, state, id) * residents;
            let target = ((use_milli as f64) * f64::from(pop.pantry_days)).round().max(1000.0) as u64;
            let mut have: u64 =
                g.any_of.iter().map(|r| u64::from(state.population.household(id).map_or(0, |h| h.pantry_milli(*r)))).sum();
            let mut n = 0;
            while have < target && n < MAX_UNITS_PER_RESTOCK {
                n += 1;
                if !g.any_of.iter().any(|r| take_unit(state, ctx, &sources, id, *r, Payment::Goods)) {
                    break;
                }
                have += 1000;
            }
        }
    }

}

/// Fuel (milli heating value) the household of `residents` burns per hour in winter (× the
/// weather's heating demand).
fn fuel_need_per_hour(state: &GameState, defs: &Defs, residents: u64) -> u64 {
    let rules = defs.rules();
    let heating = f64::from(state.weather.heating_percent(rules)) / 100.0;
    (f64::from(rules.population.fuel_per_resident_day) * 1000.0 * residents as f64 * heating / 24.0).round() as u64
}

/// True if no member of the household is grown up (nobody can run errands).
fn only_children(state: &GameState, id: HouseholdId) -> bool {
    state.population.household(id).is_some_and(|h| {
        h.members.iter().all(|m| state.population.citizen(*m).is_none_or(|c| c.stage == super::LifeStage::Child))
    })
}

fn hourly(state: &mut GameState, defs: &Defs, ctx: &mut TickContext, id: HouseholdId, events: &mut Vec<SimEvent>) {
    let Some(h) = state.population.household(id) else { return };
    let home = h.home;
    let residents = u64::from(h.size());
    let rect = household_rect(state, defs, id);
    let services = ctx.services.as_ref().map_or(0, |s| s.mask(&rect));
    let cold = state.weather.needs_heating(defs.rules(), state.clock);
    let (wanted, own_goods) = wanted_goods(state, defs, home);
    let has_market = services & service_bit(ServiceKind::Market) != 0;
    if home.is_none() {
        // The homeless camp at the headquarters and take what they need there.
        let sources = supply_sources(ctx, &rect, has_market, true);
        restock(state, defs, ctx, id, has_market, &sources);
    } else if let Some(place) = errand_place(state, id) {
        // The runner waits at the market (or storage) for goods to arrive.
        if let Some(sources) = errand_sources(state, defs, ctx, &rect, has_market, place) {
            restock(state, defs, ctx, id, has_market, &sources);
        }
    } else if only_children(state, id) {
        // Children without a grown-up (parents fallen or dead): the neighbours bring them what
        // they need from the markets and storages that serve the house.
        let sources = supply_sources(ctx, &rect, has_market, false);
        restock(state, defs, ctx, id, has_market, &sources);
    }
    let food_hour = food_need_per_hour(state, defs, id);
    let fuel_hour = if cold { fuel_need_per_hour(state, defs, residents) } else { 0 };

    // --- use one hour --------------------------------------------------------------------
    let fed = eat(state, defs, id, food_hour);
    let fuel_ok = if cold { burn(state, defs, id, fuel_hour) } else { true };
    for g in &wanted {
        let per_hour = goods_use_per_day_milli(defs, g, state, id) * residents / 24;
        let mut left = u32::try_from(per_hour).unwrap_or(u32::MAX);
        if let Some(h) = state.population.households.get_mut(id) {
            for r in &g.any_of {
                if left == 0 {
                    break;
                }
                left -= h.take_from_pantry(*r, left);
            }
        }
    }

    // --- supply state --------------------------------------------------------------------
    let fresh_water = {
        let area = rect.expanded(i32::from(FRESH_WATER_RADIUS)).clipped(state.world.width(), state.world.height());
        area.tiles().any(|p| state.world.tile(p).has_fresh_water())
    };
    let clothes = defs.resource_id("clothes");
    let tavern_in_stock = ctx.services.as_ref().is_some_and(|s| s.covers_stocked(ServiceKind::Entertainment, &rect));
    // Goods taverns sell, used at home at their household rate (also by tiers that do not
    // require them).
    let tavern_goods = tavern_goods(defs);
    for r in tavern_goods.iter().filter(|r| !wanted.iter().any(|g| g.any_of.contains(r))) {
        let per_hour = defs.resources().get(r.index()).map_or(0, |d| (f64::from(d.household_use) * 1000.0).round().max(0.0) as u64)
            * residents
            / 24;
        if let Some(h) = state.population.households.get_mut(id) {
            h.take_from_pantry(*r, u32::try_from(per_hour).unwrap_or(u32::MAX));
        }
    }
    let Some(h) = state.population.households.get_mut(id) else { return };
    let tavern_goods = tavern_in_stock || tavern_goods.iter().any(|r| h.pantry_milli(*r) > 0);
    let variety = h
        .pantry
        .iter()
        .filter(|p| defs.resources().get(p.resource.index()).is_some_and(|d| d.category == ResourceCategory::Food && d.is_edible()))
        .count();
    let missing_goods = wanted
        .iter()
        .take(own_goods)
        .filter(|g| g.any_of.iter().all(|r| h.pantry_milli(*r) == 0))
        .count();
    h.supply = SupplyState {
        services,
        fresh_water,
        fed,
        food_variety: u8::try_from(variety).unwrap_or(u8::MAX),
        fuel: fuel_ok,
        clothes: clothes.is_some_and(|c| h.pantry_milli(c) > 0),
        missing_goods: u8::try_from(missing_goods).unwrap_or(u8::MAX),
        tavern_goods,
    };
    let family = h.family_name.clone();
    let at = home.and_then(|b| state.buildings.get(b)).map(|b| b.entrance());
    let tick = state.clock.tick;
    // A family whose runner is just on the way to food in reach is not starving.
    let shopping_soon = home.is_some() && {
        let sources = supply_sources(ctx, &rect, has_market, false);
        defs.resources().iter().filter(|r| r.is_edible()).any(|r| {
            ctx.available.as_ref().and_then(|a| a.get(r.id.index())).is_some_and(|n| *n > 0)
                && sources.iter().any(|s| state.buildings.get(*s).is_some_and(|b| b.inventory.get(r.id) > 0))
        })
    };
    // Hunger and cold are reported once per spell (docs/game_design.md §2 Meldungen); the
    // houses show them meanwhile (`report::house_report`, heating).
    if !fed && !shopping_soon {
        let mut n = Notification::new(
            Severity::Danger,
            format!("Nahrungsmangel: Familie {family} findet nichts mehr zu essen. Sorgt für Nahrung und einen Markt oder ein Lager in der Nähe."),
        );
        if let Some(at) = at {
            n = n.at(at);
        }
        if let Some(home) = home {
            n = n.for_building(home);
        }
        state.population.notify_waiting(defs, Notice::Famine, tick, n, events);
    }
    if !fuel_ok {
        let mut n = Notification::new(
            Severity::Warning,
            format!("Familie {family} friert – es fehlt Holz oder Kohle."),
        );
        if let Some(at) = at {
            n = n.at(at);
        }
        if let Some(home) = home {
            n = n.for_building(home);
        }
        state.population.notify_waiting(defs, Notice::NoFuel, tick, n, events);
    }
}

/// Daily use of a goods requirement per resident in thousandths: the rate of the first
/// accepted resource the household has (or of the first accepted resource).
fn goods_use_per_day_milli(defs: &Defs, g: &GoodsRequirement, state: &GameState, id: HouseholdId) -> u64 {
    let h = state.population.household(id);
    let r = g.any_of.iter().copied().find(|r| h.is_some_and(|h| h.pantry_milli(*r) > 0)).or_else(|| g.any_of.first().copied());
    r.and_then(|r| defs.resources().get(r.index()))
        .map_or(0, |d| (f64::from(d.household_use) * 1000.0).round().max(0.0) as u64)
}

/// How a household pays for what it takes (docs/game_design.md §6 Wohlstand).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Payment {
    /// Food and fuel: taken even without money; the household pays what it has.
    Basic,
    /// Goods: only bought when the household can pay the price.
    Goods,
}

/// True if household `id` can buy one unit of `resource` with `payment`; pays the market
/// price (as far as its savings go for basic needs).
fn pay_for(state: &mut GameState, id: HouseholdId, resource: crate::defs::ResourceId, payment: Payment, commit: bool) -> bool {
    let price = super::wealth::unit_price_milli(state, resource);
    let Some(h) = state.population.households.get_mut(id) else { return false };
    if payment == Payment::Goods && h.purse.savings_milli < price {
        return false;
    }
    if commit {
        h.purse.pay(price);
    }
    true
}

/// Takes one unit of `resource` from the first source storage holding it into the pantry,
/// paying the market price.
fn take_unit(
    state: &mut GameState,
    ctx: &mut TickContext,
    sources: &[BuildingId],
    household: HouseholdId,
    resource: crate::defs::ResourceId,
    payment: Payment,
) -> bool {
    let Some(available) = ctx.available.as_ref().and_then(|a| a.get(resource.index())).copied() else { return false };
    if available == 0 || !pay_for(state, household, resource, payment, false) {
        return false;
    }
    for s in sources {
        if let Some(b) = state.buildings.get_mut(*s)
            && b.inventory.remove(resource, 1) == 1
        {
            if let Some(a) = ctx.available.as_mut().and_then(|a| a.get_mut(resource.index())) {
                *a -= 1;
            }
            pay_for(state, household, resource, payment, true);
            if let Some(h) = state.population.households.get_mut(household) {
                h.add_to_pantry(resource, 1000);
            }
            state.economy.record_household_use(resource, 1);
            ctx.stock_changed = true;
            return true;
        }
    }
    false
}

/// Like [`take_unit`] for fuel, but also from units committed to construction sites (a
/// household without fuel on a cold day; docs/game_design.md §6.1 Heizen geht vor).
fn take_committed_unit(
    state: &mut GameState,
    ctx: &mut TickContext,
    sources: &[BuildingId],
    household: HouseholdId,
    resource: crate::defs::ResourceId,
) -> bool {
    for s in sources {
        if let Some(b) = state.buildings.get_mut(*s)
            && b.inventory.remove(resource, 1) == 1
        {
            if let Some(a) = ctx.available.as_mut().and_then(|a| a.get_mut(resource.index())) {
                *a = a.saturating_sub(1);
            }
            pay_for(state, household, resource, Payment::Basic, true);
            if let Some(h) = state.population.households.get_mut(household) {
                h.add_to_pantry(resource, 1000);
            }
            state.economy.record_household_use(resource, 1);
            ctx.stock_changed = true;
            return true;
        }
    }
    false
}

/// Goods sold by the buildings providing entertainment (tavern: beer).
fn tavern_goods(defs: &Defs) -> Vec<crate::defs::ResourceId> {
    let mut list: Vec<crate::defs::ResourceId> = defs
        .buildings()
        .iter()
        .filter(|d| d.service_radius(ServiceKind::Entertainment).is_some())
        .flat_map(|d| d.sells.iter().copied())
        .collect();
    list.sort();
    list.dedup();
    list
}

/// Guests that arrived at a leisure venue this step: at a venue that sells goods (tavern:
/// beer) the household buys up to `pantry_days` of its use of each from the venue's stock,
/// paying the market price (docs/game_design.md §5.4).
pub(crate) fn buy_at_venues(state: &mut GameState, defs: &Defs, guests: &[(HouseholdId, BuildingId)]) {
    let pantry_days = f64::from(defs.rules().population.pantry_days);
    for &(household, venue) in guests {
        let Some(def) = state.buildings.get(venue).and_then(|b| defs.buildings().get(b.kind().index())) else { continue };
        if def.sells.is_empty() || !state.buildings.get(venue).is_some_and(|b| b.is_operational() && b.trade_staff() > 0) {
            continue;
        }
        let residents = state.population.household(household).map_or(0, |h| u64::from(h.size()));
        for &r in &def.sells {
            let use_day = defs.resources().get(r.index()).map_or(0.0, |d| f64::from(d.household_use));
            let target = ((use_day * 1000.0 * residents as f64) * pantry_days).round().max(1000.0) as u64;
            for _ in 0..MAX_UNITS_PER_RESTOCK {
                let have = state.population.household(household).map_or(0, |h| u64::from(h.pantry_milli(r)));
                let in_stock = state.buildings.get(venue).is_some_and(|b| b.inventory.get(r) > 0);
                if have >= target || !in_stock || !pay_for(state, household, r, Payment::Goods, false) {
                    break;
                }
                if let Some(b) = state.buildings.get_mut(venue) {
                    b.inventory.remove(r, 1);
                }
                pay_for(state, household, r, Payment::Goods, true);
                if let Some(h) = state.population.households.get_mut(household) {
                    h.add_to_pantry(r, 1000);
                }
                state.economy.record_household_use(r, 1);
            }
        }
    }
}

/// Eats `need` milli-saturation from the pantry, always from the food with the most
/// saturation left; returns true if the need was covered completely.
fn eat(state: &mut GameState, defs: &Defs, id: HouseholdId, need: u64) -> bool {
    consume_value(state, id, need, |r| nutrition_pm(defs, r))
}

/// Burns `need` milli-heat of fuel; true if covered.
fn burn(state: &mut GameState, defs: &Defs, id: HouseholdId, need: u64) -> bool {
    consume_value(state, id, need, |r| heating_pm(defs, r))
}

fn consume_value(state: &mut GameState, id: HouseholdId, need: u64, per_unit: impl Fn(crate::defs::ResourceId) -> u64) -> bool {
    let Some(h) = state.population.households.get_mut(id) else { return need == 0 };
    let mut left = need;
    while left > 0 {
        let best = h
            .pantry
            .iter()
            .filter(|p| per_unit(p.resource) > 0)
            .max_by_key(|p| (u64::from(p.milli) * per_unit(p.resource), std::cmp::Reverse(p.resource)))
            .map(|p| (p.resource, p.milli, per_unit(p.resource)));
        let Some((r, milli, value)) = best else { break };
        let worth = u64::from(milli) * value / 1000;
        if worth <= left {
            h.take_from_pantry(r, milli);
            left -= worth.max(1).min(left);
        } else {
            // ceil(left * 1000 / value) thousandths cover the rest.
            let units = (left * 1000).div_ceil(value);
            h.take_from_pantry(r, u32::try_from(units).unwrap_or(u32::MAX));
            left = 0;
        }
    }
    left == 0
}
