//! Abstract economy of the AI kingdoms (docs/game_design.md §10).
//!
//! Production per day comes from the land sources of the territory (per tile) and from crafts
//! (per 100 inhabitants, only with enough land of the required source); trading kingdoms also
//! bring goods from distant lands. Farm and pasture output drops in winter. Consumption is per
//! 100 inhabitants. Stock is kept in milli-units; what exceeds `stock_limit_days` of production
//! or consumption leaves for distant markets. A kingdom *wants* goods of which it holds less
//! than `reserve_days` of consumption (up to `surplus_days`) and *offers* what exceeds
//! `surplus_days`. Its prices follow the player's formula (docs/game_design.md §7) with supply =
//! stock + daily production and demand = `reserve_days` of consumption. Food decides growth and
//! decline; taxes pay the soldiers.

use super::{Kingdom, KingdomId, TreatyKind, relations};
use crate::defs::{Consumption, Defs, LandSource, Personality, ResourceId};
use crate::economy::market::target_price_centi;
use crate::economy::trade::{TradeDirection, trade_bonus_percent};
use crate::economy::MILLI_PER_GOLD;
use crate::event::SimEvent;
use crate::state::GameState;
use crate::time::Season;

/// Milli-units of `c` that `population` inhabitants consume per day.
pub(crate) fn consumption_milli(c: &Consumption, population: u32) -> u64 {
    u64::from(c.per_100_milli) * u64::from(population) / 100
}

/// Daily consumption of `resource` by kingdom `k` in milli-units.
pub fn daily_consumption_milli(defs: &Defs, k: &Kingdom, resource: ResourceId) -> u64 {
    let list = &defs.kingdom_data().consumption;
    list.binary_search_by_key(&resource, |c| c.resource).map_or(0, |i| consumption_milli(&list[i], k.population))
}

/// Soldiers a kingdom keeps: per 100 inhabitants by personality, half again during a war.
pub(crate) fn soldier_target(defs: &Defs, k: &Kingdom) -> u32 {
    let per_100 = u32::from(defs.rules().diplomacy.personalities.get(k.personality).soldiers_per_100);
    let base = k.population * per_100 / 100;
    if k.war.is_some() { base * 3 / 2 } else { base }
}

/// Recomputes the daily production profile of kingdom `id` from its land and inhabitants.
pub(crate) fn refresh_production(state: &mut GameState, defs: &Defs, id: KingdomId) {
    let Some(k) = state.diplomacy.kingdom_mut(id) else { return };
    let min_tiles = u32::from(defs.rules().diplomacy.craft_min_tiles);
    k.production_milli.iter_mut().for_each(|v| *v = 0);
    k.farm_milli.iter_mut().for_each(|v| *v = 0);
    for p in &defs.kingdom_data().production {
        if p.requires.is_some_and(|req| k.land.get(req.index()).copied().unwrap_or(0) < min_tiles) {
            continue;
        }
        let amount = match p.source {
            LandSource::Inhabitants => u64::from(p.per_unit_milli) * u64::from(k.population) / 100,
            LandSource::FarTrade if k.personality == Personality::Trader => u64::from(p.per_unit_milli) * u64::from(k.population) / 100,
            LandSource::FarTrade => 0,
            land => u64::from(p.per_unit_milli) * u64::from(k.land.get(land.index()).copied().unwrap_or(0)),
        };
        let amount = u32::try_from(amount).unwrap_or(u32::MAX);
        let i = p.resource.index();
        if let Some(slot) = k.production_milli.get_mut(i) {
            *slot = slot.saturating_add(amount);
        }
        if p.source.is_farming()
            && let Some(slot) = k.farm_milli.get_mut(i)
        {
            *slot = slot.saturating_add(amount);
        }
    }
}

/// One day of every kingdom's economy at midnight: production, consumption, stock limits,
/// taxes and soldiers' pay, growth or decline by food, soldiers towards their target.
pub(crate) fn run_day(state: &mut GameState, defs: &Defs, _events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy;
    let winter = state.clock.season() == Season::Winter;
    for i in 0..state.diplomacy.kingdoms.len() {
        let id = KingdomId::from_raw(i as u8);
        refresh_production(state, defs, id);
        let k = &mut state.diplomacy.kingdoms[i];
        for (r, slot) in k.stock_milli.iter_mut().enumerate() {
            let farm = u64::from(k.farm_milli[r]);
            let total = u64::from(k.production_milli[r]);
            let made = if winter { total - farm + farm * u64::from(rules.winter_farm_percent) / 100 } else { total };
            *slot = slot.saturating_add(made);
        }
        let (mut food_need, mut food_got) = (0u64, 0u64);
        for c in &defs.kingdom_data().consumption {
            let need = consumption_milli(c, k.population);
            let slot = &mut k.stock_milli[c.resource.index()];
            let take = need.min(*slot);
            *slot -= take;
            if defs.resource(c.resource).is_edible() {
                food_need += need;
                food_got += take;
            }
        }
        // Missing food is made up from other food in stock (resource id order).
        for r in defs.resources().iter().filter(|r| r.is_edible()) {
            let missing = food_need - food_got;
            if missing == 0 {
                break;
            }
            let slot = &mut k.stock_milli[r.id.index()];
            let take = missing.min(*slot);
            *slot -= take;
            food_got += take;
        }
        k.food_percent = u8::try_from((food_got * 100).checked_div(food_need).unwrap_or(100).min(100)).unwrap_or(100);
        for (r, slot) in k.stock_milli.iter_mut().enumerate() {
            let resource = ResourceId::from_index(r);
            let cons = resource.map_or(0, |res| {
                let list = &defs.kingdom_data().consumption;
                list.binary_search_by_key(&res, |c| c.resource).map_or(0, |j| consumption_milli(&list[j], k.population))
            });
            let cap = cons.max(u64::from(k.production_milli[r])) * u64::from(rules.stock_limit_days) + 50_000;
            *slot = (*slot).min(cap);
        }
        let income = (f64::from(rules.tax_per_inhabitant) * f64::from(k.population) * MILLI_PER_GOLD as f64).round() as i64;
        let pay = (f64::from(rules.soldier_upkeep) * f64::from(k.soldiers) * MILLI_PER_GOLD as f64).round() as i64;
        k.treasury_milli = k.treasury_milli.saturating_add(income - pay);
        if k.food_percent >= 90 {
            let grow = (k.population * u32::from(rules.growth_permille) / 1000).max(1);
            k.population = (k.population + grow).min(rules.max_population);
        } else if k.food_percent < 50 {
            let shrink = (k.population * u32::from(rules.starvation_permille) / 1000).max(1);
            k.population = k.population.saturating_sub(shrink).max(20);
        }
        let target = soldier_target(defs, k);
        if k.treasury_milli < 0 {
            k.soldiers = k.soldiers * 9 / 10;
        } else if k.soldiers < target {
            k.soldiers += ((target - k.soldiers) / 5).max(1);
        } else if k.soldiers > target {
            k.soldiers -= ((k.soldiers - target) / 5).max(1);
        }
    }
}

/// The kingdom's own price of `resource` in centi-gold (supply and demand of its economy).
pub fn price_centi(defs: &Defs, k: &Kingdom, resource: ResourceId) -> u32 {
    let rules = &defs.rules().diplomacy;
    let base = defs.resources().get(resource.index()).map_or(1, |r| r.base_price);
    let supply = u64::from(k.stock(resource)) + u64::from(k.production_milli(resource)) / 1000;
    let demand = daily_consumption_milli(defs, k, resource) * u64::from(rules.reserve_days) / 1000;
    target_price_centi(base, supply, demand, defs.rules())
}

/// Units of `resource` the kingdom wants to buy (it holds less than `reserve_days` of
/// consumption; it buys up to `surplus_days`).
pub fn wanted_units(defs: &Defs, k: &Kingdom, resource: ResourceId) -> u32 {
    let rules = &defs.rules().diplomacy;
    let daily = daily_consumption_milli(defs, k, resource);
    let stock = k.stock_milli.get(resource.index()).copied().unwrap_or(0);
    if daily == 0 || stock >= daily * u64::from(rules.reserve_days) {
        return 0;
    }
    u32::try_from((daily * u64::from(rules.surplus_days)).saturating_sub(stock) / 1000).unwrap_or(u32::MAX)
}

/// Units of `resource` the kingdom offers (beyond `surplus_days` of consumption).
pub fn offered_units(defs: &Defs, k: &Kingdom, resource: ResourceId) -> u32 {
    let rules = &defs.rules().diplomacy;
    let daily = daily_consumption_milli(defs, k, resource);
    let stock = k.stock_milli.get(resource.index()).copied().unwrap_or(0);
    u32::try_from(stock.saturating_sub(daily * u64::from(rules.surplus_days)) / 1000).unwrap_or(u32::MAX)
}

/// Price advantage of the player with kingdom `id` in percent: researched trade bonuses, the
/// relation (`relations.price_percent_per_10` per 10 points, negative when relations are bad)
/// and the best treaty bonus (trade agreement or alliance).
pub fn price_bonus_percent(state: &GameState, defs: &Defs, id: KingdomId) -> i32 {
    let Some(k) = state.diplomacy.kingdom(id) else { return 0 };
    let rules = &defs.rules().diplomacy;
    let tick = state.clock.tick;
    let relation = relations::relation(state, defs, id);
    let mut treaty = 0;
    if k.has_treaty(TreatyKind::TradeAgreement, tick) {
        treaty = treaty.max(i32::from(rules.treaties.trade_price_bonus_percent));
    }
    if k.has_treaty(TreatyKind::Alliance, tick) {
        treaty = treaty.max(i32::from(rules.treaties.alliance_price_bonus_percent));
    }
    trade_bonus_percent(&state.research, defs) + relation / 10 * i32::from(rules.relations.price_percent_per_10) + treaty
}

/// Unit price in centi-gold of a trade with kingdom `id`: `Export` = the player sells to it,
/// `Import` = the player buys from it; the kingdom's price minus/plus the caravan margin,
/// improved by [`price_bonus_percent`] (−30 … +50 %) at most down to the kingdom's own price
/// ([`crate::economy::trade::trade_percent`]: buying never costs less than selling fetches).
pub fn unit_price_centi(state: &GameState, defs: &Defs, id: KingdomId, resource: ResourceId, direction: TradeDirection) -> u32 {
    unit_price_with_route_centi(state, defs, id, resource, direction, 0)
}

/// [`unit_price_centi`] for a caravan whose walk on the map gave a trade road bonus of
/// `route_percent` (docs/game_design.md §5.6), added to the price bonus.
pub fn unit_price_with_route_centi(
    state: &GameState,
    defs: &Defs,
    id: KingdomId,
    resource: ResourceId,
    direction: TradeDirection,
    route_percent: i32,
) -> u32 {
    let Some(k) = state.diplomacy.kingdom(id) else { return 1 };
    let price = i64::from(price_centi(defs, k, resource));
    let margin = defs.rules().diplomacy.caravans.margin_percent;
    let bonus = price_bonus_percent(state, defs, id).saturating_add(route_percent).clamp(-30, 50);
    let percent = crate::economy::trade::trade_percent(direction, margin, bonus);
    (price * percent / 100).clamp(1, i64::from(u32::MAX)) as u32
}

/// Value of the kingdom's stock at its own prices plus its treasury, in gold (estimate for the
/// report).
pub fn wealth_estimate(defs: &Defs, k: &Kingdom) -> i64 {
    let goods: i64 = defs
        .resources()
        .iter()
        .map(|r| i64::from(k.stock(r.id)) * i64::from(price_centi(defs, k, r.id)) / 100)
        .sum();
    k.treasury() + goods
}

/// Removes up to `amount` units of `resource` from the kingdom's stock; returns the units
/// removed.
pub(crate) fn take(k: &mut Kingdom, resource: ResourceId, amount: u32) -> u32 {
    let Some(slot) = k.stock_milli.get_mut(resource.index()) else { return 0 };
    let units = u32::try_from(*slot / 1000).unwrap_or(u32::MAX).min(amount);
    *slot -= u64::from(units) * 1000;
    units
}

/// Adds `amount` units of `resource` to the kingdom's stock.
pub(crate) fn give(k: &mut Kingdom, resource: ResourceId, amount: u32) {
    if let Some(slot) = k.stock_milli.get_mut(resource.index()) {
        *slot = slot.saturating_add(u64::from(amount) * 1000);
    }
}
