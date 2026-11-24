//! Trade caravans (docs/game_design.md §7, §10).
//!
//! **Foreign caravans:** a kingdom with access (no war; neutral or better, or a trade
//! agreement) sends merchants when the player has an operational, staffed trading post and its
//! standing orders match: the kingdom offers goods the player wants to buy (stock below the
//! buy threshold) or wants goods the player sells (stock above the sell threshold). The cargo
//! leaves the kingdom's stock and the purse its treasury. After an off-map leg (time from the
//! distance) a merchant and a cart walk from the map edge in the direction of their kingdom to
//! the trading post, trade there by the orders (each order's daily limit counts all caravans),
//! stay `trade_hours`, walk back and take what is left home. Intervals: neutral, friendly or
//! allied, trade agreement (`caravans.interval_*_hours`); without goods to trade the kingdom
//! checks again after `retry_hours`.
//!
//! **Own caravans** (`Command::SendCaravan`): goods from the storages near the trading post and
//! gold from the treasury walk from the post to the map edge towards the partner, travel there
//! off the map, sell what the partner wants, buy the wished goods with the purse, travel back
//! and walk to the post, where goods are stored and the purse returns to the treasury.
//!
//! **Danger:** every off-map leg can be robbed by a kingdom near the route (its seat within its
//! radius plus `route_margin` of the line between trading post and partner): at war with the
//! player, hostile or rival (`robbery_*_percent`, at most `robbery_max_percent`). Robbers take
//! cargo and purse to their own kingdom. War with the partner turns its caravans back.
//!
//! **Without kingdoms** (a scenario without rivals) the standing orders trade hourly with the
//! abstract world market as before ([`crate::economy::trade`]); with kingdoms all
//! trade goes through caravans.
//!
//! Goods and gold on the way belong to the caravan, so the sum over storages, kingdoms and
//! caravans changes only by production and consumption.

use glam::IVec2;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::relations::{self, remember};
use super::{KingdomId, MemoryCause, RelationState, economy, has_access};
use crate::buildings::BuildingId;
use crate::command::{CommandResult, RejectReason};
use crate::defs::{Defs, ResourceAmount, ResourceId};
use crate::economy::trade::{TradeDirection, TradeOrder, active_trading_post};
use crate::economy::{LedgerCategory, MILLI_PER_GOLD, storage};
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::population::movement::{self, Destination, Motion, Traveler, TravelerId, TripPhase, WalkerId};
use crate::population::CitizenRole;
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;

/// Own caravans on the way at most.
pub const MAX_OWN_CARAVANS: usize = 3;

/// Whose caravan it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CaravanKind {
    /// Merchants of a kingdom visiting the player's trading post.
    Foreign,
    /// The player's caravan to a kingdom.
    Own,
}

/// Where a caravan is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaravanStage {
    /// Off the map until `until_tick`; `outward`: foreign caravans on the way to the player,
    /// own caravans on the way to the partner (`false`: on the way back).
    Travelling {
        /// First tick of the next stage.
        until_tick: u64,
        /// Direction of the leg.
        outward: bool,
    },
    /// On the map, walking to the trading post (foreign) or back to it (own).
    Arriving,
    /// At the trading post until `until_tick` (foreign only).
    Trading {
        /// First tick of leaving.
        until_tick: u64,
    },
    /// On the map, walking to the map edge.
    Leaving,
}

/// A trade caravan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caravan {
    /// Id (unique per game).
    pub id: u32,
    /// The partner kingdom (owner of a foreign caravan).
    pub kingdom: KingdomId,
    /// Foreign or own.
    pub kind: CaravanKind,
    /// Where it is.
    pub stage: CaravanStage,
    /// Goods carried, by resource id, all positive.
    pub cargo: Vec<ResourceAmount>,
    /// Gold carried in milli-gold.
    pub purse_milli: i64,
    /// Own caravans: goods to buy at the partner.
    pub wishes: Vec<ResourceAmount>,
    /// Walkers on the map (merchant, then cart).
    pub walkers: Vec<TravelerId>,
    /// Map-edge tile it enters and leaves by.
    pub entry: IVec2,
    /// The trading post (or headquarters) it walks to.
    pub post: Option<BuildingId>,
    /// Tick the cart sets out (it follows the merchant).
    pub cart_due_tick: Option<u64>,
    /// Tick a walker found no path (it gives up after `stuck_hours`).
    pub stuck_since: Option<u64>,
    /// Tick it set out.
    pub created_tick: u64,
    /// Own caravans: gold earned at the partner (milli).
    pub sold_milli: i64,
    /// Own caravans: gold spent at the partner (milli).
    pub bought_milli: i64,
    /// Own caravans: gold taken from the treasury at departure (milli).
    pub start_purse_milli: i64,
    /// The merchant's walk on the map this leg (trade roads, docs/game_design.md §5.6).
    pub route: RouteTally,
}

/// Tiles the merchant of a caravan walked on its current leg on the map, and how many of them
/// were trade roads: the trade road bonus of a trade is the road's `trade_bonus_percent`
/// scaled by that share (all tiles on trade roads → the full bonus).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteTally {
    /// Tile the merchant stood on at the last count.
    last: Option<IVec2>,
    /// Tiles walked.
    pub tiles: u32,
    /// Sum of the trade bonus (percent) of the road under every walked tile.
    pub bonus_sum: u32,
}

impl RouteTally {
    /// Trade road bonus of the walked route in percent.
    pub fn bonus_percent(&self) -> i32 {
        self.bonus_sum.checked_div(self.tiles).and_then(|p| i32::try_from(p).ok()).unwrap_or(0)
    }

    /// Counts tile `tile` if the merchant moved onto it.
    fn walk(&mut self, state: &GameState, defs: &Defs, tile: IVec2) {
        if self.last == Some(tile) {
            return;
        }
        self.last = Some(tile);
        self.tiles = self.tiles.saturating_add(1);
        self.bonus_sum = self.bonus_sum.saturating_add(u32::from(trade_road_bonus_at(state, defs, tile)));
    }
}

/// Trade bonus (percent) of the road on `tile` (0 without a road).
pub fn trade_road_bonus_at(state: &GameState, defs: &Defs, tile: IVec2) -> u16 {
    state.roads.get(tile).and_then(|r| defs.roads().get(r.road.index())).map_or(0, |d| d.trade_bonus_percent)
}

/// Updates the route tally of caravan `index` with its merchant's position.
fn count_route(state: &mut GameState, defs: &Defs, index: usize) {
    let Some(&merchant) = state.diplomacy.caravans[index].walkers.first() else { return };
    let Some(tile) = state.population.walking.travelers.get(merchant).map(|t| t.motion.pos().tile()) else { return };
    let mut route = state.diplomacy.caravans[index].route;
    route.walk(state, defs, tile);
    state.diplomacy.caravans[index].route = route;
}

impl Caravan {
    /// Total units carried.
    pub fn units(&self) -> u32 {
        self.cargo.iter().map(|m| m.amount).sum()
    }

    /// Units of `resource` carried.
    pub fn carried(&self, resource: ResourceId) -> u32 {
        self.cargo.iter().find(|m| m.resource == resource).map_or(0, |m| m.amount)
    }

    fn add(&mut self, resource: ResourceId, amount: u32) {
        if amount == 0 {
            return;
        }
        match self.cargo.binary_search_by_key(&resource, |m| m.resource) {
            Ok(i) => self.cargo[i].amount += amount,
            Err(i) => self.cargo.insert(i, ResourceAmount { resource, amount }),
        }
    }

    fn remove(&mut self, resource: ResourceId, amount: u32) -> u32 {
        let Ok(i) = self.cargo.binary_search_by_key(&resource, |m| m.resource) else { return 0 };
        let taken = self.cargo[i].amount.min(amount);
        self.cargo[i].amount -= taken;
        if self.cargo[i].amount == 0 {
            self.cargo.remove(i);
        }
        taken
    }

    /// The good shown on the cart: the largest load (ties: lowest id).
    pub fn main_good(&self) -> Option<(ResourceId, u32)> {
        self.cargo.iter().max_by_key(|m| (m.amount, std::cmp::Reverse(m.resource))).map(|m| (m.resource, m.amount))
    }

    /// Ticks until the current stage ends (off the map and at the post), if known.
    pub fn ticks_left(&self, now: u64) -> Option<u64> {
        match self.stage {
            CaravanStage::Travelling { until_tick, .. } | CaravanStage::Trading { until_tick } => Some(until_tick.saturating_sub(now)),
            CaravanStage::Arriving | CaravanStage::Leaving => None,
        }
    }
}

/// Ticks of one off-map leg between the player's trading post (or headquarters) and the seat
/// of kingdom `id`.
pub fn travel_ticks(state: &GameState, defs: &Defs, id: KingdomId) -> u64 {
    let rules = &defs.rules().diplomacy.caravans;
    let Some(k) = state.diplomacy.kingdom(id) else { return u64::from(rules.min_travel_hours) * TICKS_PER_HOUR };
    let from = home_point(state, defs);
    let d = (k.seat - from).abs();
    let tiles = u64::try_from(d.x + d.y).unwrap_or(0);
    let hours = (tiles * u64::from(rules.travel_hours_per_100_tiles) / 100).max(u64::from(rules.min_travel_hours));
    hours * TICKS_PER_HOUR
}

/// The player's trading point: the active trading post, else the main headquarters, else the
/// world's start.
fn home_point(state: &GameState, defs: &Defs) -> IVec2 {
    active_trading_post(state, defs)
        .or_else(|| storage::main_headquarters(state, defs))
        .and_then(|b| state.buildings.get(b))
        .map_or(state.world.start, |b| b.entrance())
}

/// Squared distance from `p` to the segment `a`–`b`.
fn segment_distance2(p: IVec2, a: IVec2, b: IVec2) -> i64 {
    let ab = (b - a).as_i64vec2();
    let ap = (p - a).as_i64vec2();
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 == 0 {
        return ap.x * ap.x + ap.y * ap.y;
    }
    let t = (ap.x * ab.x + ap.y * ab.y).clamp(0, len2);
    // Closest point a + ab * t / len2, kept in exact rationals: distance² = |ap·len2 − ab·t|² / len2².
    let dx = ap.x * len2 - ab.x * t;
    let dy = ap.y * len2 - ab.y * t;
    (dx * dx + dy * dy) / (len2 * len2)
}

/// Robbery risk in percent of one off-map leg to kingdom `partner` and the kingdom that would
/// rob (the most dangerous one near the route).
pub fn route_danger(state: &GameState, defs: &Defs, partner: KingdomId) -> (u32, Option<KingdomId>) {
    let rules = &defs.rules().diplomacy.caravans;
    let Some(p) = state.diplomacy.kingdom(partner) else { return (0, None) };
    let from = home_point(state, defs);
    let mut total = 0u32;
    let mut worst: Option<(u32, KingdomId)> = None;
    for (id, k) in state.diplomacy.kingdoms() {
        if id == partner {
            continue;
        }
        let reach = i64::from(k.radius) + i64::from(rules.route_margin);
        if segment_distance2(k.seat, from, p.seat) > reach * reach {
            continue;
        }
        let risk = if k.at_war() {
            u32::from(rules.robbery_war_percent)
        } else {
            match relations::relation_state(state, defs, id) {
                RelationState::Hostile => u32::from(rules.robbery_hostile_percent),
                RelationState::Rival => u32::from(rules.robbery_rival_percent),
                _ => 0,
            }
        };
        if risk > 0 {
            total += risk;
            if worst.is_none_or(|(w, _)| risk > w) {
                worst = Some((risk, id));
            }
        }
    }
    (total.min(u32::from(rules.robbery_max_percent)), worst.map(|(_, id)| id))
}

/// The map-edge tile walkers reachable from `from` that lies closest to where the ray from
/// `from` towards `target` leaves the map (`None` without a reachable edge tile).
pub(crate) fn edge_towards(state: &mut GameState, defs: &Defs, from: IVec2, target: IVec2) -> Option<IVec2> {
    crate::navigation::prepare(state, defs);
    let edge = edge_tiles_towards(state, from, target);
    let nav = &mut state.navigation;
    edge.into_iter().find(|p| nav.is_walkable(*p) && nav.reachable(from, *p))
}

/// All map-edge tiles, nearest first to where the ray from `from` towards `target` leaves the
/// map (ties: row, column).
pub(crate) fn edge_tiles_towards(state: &GameState, from: IVec2, target: IVec2) -> Vec<IVec2> {
    let (w, h) = (state.world.width(), state.world.height());
    let d = (target - from).as_dvec2();
    let exit = if d.length_squared() < 0.5 {
        from.as_dvec2()
    } else {
        let tx = if d.x > 0.0 { f64::from(w - 1 - from.x) / d.x } else if d.x < 0.0 { f64::from(from.x) / -d.x } else { f64::INFINITY };
        let ty = if d.y > 0.0 { f64::from(h - 1 - from.y) / d.y } else if d.y < 0.0 { f64::from(from.y) / -d.y } else { f64::INFINITY };
        from.as_dvec2() + d * tx.min(ty)
    };
    let exit = IVec2::new(exit.x.round() as i32, exit.y.round() as i32).clamp(IVec2::ZERO, IVec2::new(w - 1, h - 1));
    let mut edge: Vec<IVec2> = Vec::with_capacity(2 * (w + h) as usize);
    for x in 0..w {
        edge.push(IVec2::new(x, 0));
        edge.push(IVec2::new(x, h - 1));
    }
    for y in 1..h - 1 {
        edge.push(IVec2::new(0, y));
        edge.push(IVec2::new(w - 1, y));
    }
    edge.sort_by_key(|p| ((*p - exit).abs().element_sum(), p.y, p.x));
    edge.dedup();
    edge
}

/// Puts a caravan walker standing on `at`.
fn spawn_walker(state: &mut GameState, caravan: u32, role: CitizenRole, at: IVec2) -> TravelerId {
    let traveler = Traveler { motion: Motion::standing(at), role, speed_permille: 1000, caravan: Some(caravan), warband: None };
    let id = state.population.walking.travelers.insert(traveler);
    movement::index_insert(state, WalkerId::Traveler(id));
    id
}

/// Removes a caravan walker from the map (it entered the trading post or gave up).
fn remove_walker(state: &mut GameState, id: TravelerId) {
    movement::forget(state, WalkerId::Traveler(id));
    state.population.walking.travelers.remove(id);
}

/// The destination of the merchant (`cart`: the cart) walking to the post `post`.
fn post_destination(state: &GameState, post: BuildingId, cart: bool) -> Option<Destination> {
    let b = state.buildings.get(post)?;
    let entrance = b.entrance();
    let tile = if cart {
        crate::grid::Direction::ALL
            .iter()
            .map(|d| entrance + d.offset())
            .find(|p| state.navigation.is_walkable(*p) && state.buildings.at(*p).is_none())
            .unwrap_or(entrance)
    } else {
        entrance
    };
    Some(Destination::Spot { building: post, tile })
}

/// Index of caravan `id`.
fn index_of(state: &GameState, id: u32) -> Option<usize> {
    state.diplomacy.caravans.iter().position(|c| c.id == id)
}

/// Books a trade of the player with a kingdom (caravans, offers): gold in the ledger
/// (positive milli = income from exports), resource statistics and an event.
pub(crate) fn book_trade(state: &mut GameState, resource: ResourceId, amount: u32, milli: i64, direction: TradeDirection, events: &mut Vec<SimEvent>) {
    if amount == 0 {
        return;
    }
    match direction {
        TradeDirection::Export => {
            state.economy.book_milli(LedgerCategory::Exports, milli);
            state.economy.stats_mut().today_mut().exported.add(resource, amount);
        }
        TradeDirection::Import => {
            state.economy.book_milli(LedgerCategory::Imports, milli);
            state.economy.stats_mut().today_mut().imported.add(resource, amount);
        }
    }
    events.push(SimEvent::TradeExecuted { resource, amount, gold: milli / MILLI_PER_GOLD, direction });
}

/// `Command::SendCaravan`: sends an own caravan to kingdom `id` with `goods` (from the storages
/// near the trading post) and `gold` to buy `wishes` there.
pub(crate) fn send(
    state: &mut GameState,
    defs: &Defs,
    id: KingdomId,
    goods: &[ResourceAmount],
    gold: u32,
    wishes: &[ResourceAmount],
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    if let Some(reason) = send_problem(state, defs, id, goods, gold, wishes) {
        return Err(RejectReason::CaravanUnavailable { reason });
    }
    let post = active_trading_post(state, defs).ok_or(RejectReason::CaravanUnavailable { reason: "Kein Handelskontor.".into() })?;
    let at = state.buildings.get(post).map_or(state.world.start, |b| b.entrance());
    let seat = state.diplomacy.kingdom(id).map_or(at, |k| k.seat);
    let entry = edge_towards(state, defs, at, seat);
    let tick = state.clock.tick;
    let caravan_id = state.diplomacy.next_caravan;
    state.diplomacy.next_caravan = caravan_id.wrapping_add(1).max(1);
    let mut caravan = Caravan {
        id: caravan_id,
        kingdom: id,
        kind: CaravanKind::Own,
        stage: CaravanStage::Leaving,
        cargo: Vec::new(),
        purse_milli: i64::from(gold) * MILLI_PER_GOLD,
        wishes: crate::buildings::merge_amounts(wishes.to_vec()),
        walkers: Vec::new(),
        entry: entry.unwrap_or(at),
        post: Some(post),
        cart_due_tick: None,
        stuck_since: None,
        created_tick: tick,
        sold_milli: 0,
        bought_milli: 0,
        start_purse_milli: i64::from(gold) * MILLI_PER_GOLD,
        route: RouteTally::default(),
    };
    for g in crate::buildings::merge_amounts(goods.to_vec()) {
        let taken = storage::withdraw(state, defs, g.resource, g.amount, at);
        caravan.add(g.resource, taken);
    }
    if gold > 0 {
        state.economy.book_gold(LedgerCategory::Imports, -i64::from(gold));
    }
    match entry {
        Some(edge) => {
            let merchant = spawn_walker(state, caravan_id, CitizenRole::Merchant, at);
            caravan.walkers.push(merchant);
            caravan.cart_due_tick = Some(tick + u64::from(defs.rules().diplomacy.caravans.cart_delay_ticks));
            state.diplomacy.caravans.push(caravan);
            movement::set_destination(state, WalkerId::Traveler(merchant), Some(Destination::MapEdge(edge)));
        }
        None => {
            // No way to the map edge: the caravan leaves by paths unknown to the map.
            caravan.stage = CaravanStage::Travelling { until_tick: tick + travel_ticks(state, defs, id), outward: true };
            state.diplomacy.caravans.push(caravan);
        }
    }
    let name = state.diplomacy.kingdom(id).map(|k| k.name.clone()).unwrap_or_default();
    events.push(SimEvent::CaravanDeparted { caravan: caravan_id, kingdom: id, kind: CaravanKind::Own });
    let text = format!("Eure Karawane bricht nach {name} auf.");
    state.diplomacy.record(tick, Some(id), None, text.clone());
    events.push(SimEvent::Notification(Notification::new(Severity::Info, text).with_kind(NotificationKind::Trade).at(at)));
    Ok(())
}

/// Why an own caravan cannot set out (German), or `None`.
pub fn send_problem(
    state: &GameState,
    defs: &Defs,
    id: KingdomId,
    goods: &[ResourceAmount],
    gold: u32,
    wishes: &[ResourceAmount],
) -> Option<String> {
    let Some(k) = state.diplomacy.kingdom(id) else { return Some("Dieses Königreich gibt es nicht.".to_owned()) };
    if k.at_war() {
        return Some(format!("Ihr befindet Euch im Krieg mit {}.", k.name));
    }
    if !has_access(state, defs, id) {
        return Some(format!("{} handelt nicht mit Euch (Beziehung zu schlecht, kein Handelsvertrag).", k.name));
    }
    if active_trading_post(state, defs).is_none() {
        return Some("Karawanen brauchen ein besetztes Handelskontor, das mit dem Lagernetz verbunden ist.".to_owned());
    }
    if state.diplomacy.caravans.iter().filter(|c| c.kind == CaravanKind::Own).count() >= MAX_OWN_CARAVANS {
        return Some(format!("Es sind bereits {MAX_OWN_CARAVANS} eigene Karawanen unterwegs."));
    }
    let units: u64 = goods.iter().map(|g| u64::from(g.amount)).sum();
    if units == 0 && gold == 0 {
        return Some("Die Karawane braucht Waren oder Gold.".to_owned());
    }
    let capacity = defs.rules().diplomacy.caravans.capacity;
    if units > u64::from(capacity) {
        return Some(format!("Eine Karawane trägt höchstens {capacity} Einheiten."));
    }
    if goods.iter().chain(wishes).any(|g| g.resource.index() >= defs.resources().len()) {
        return Some("Diese Ware gibt es nicht.".to_owned());
    }
    let merged = crate::buildings::merge_amounts(goods.to_vec());
    for g in &merged {
        let available = state.economy.available(g.resource);
        if available < g.amount {
            return Some(format!("{}.", crate::report::missing_goods(g.amount - available, &defs.resource(g.resource).name)));
        }
    }
    let treasury = state.economy.treasury();
    (i64::from(gold) > treasury).then(|| format!("{}.", crate::report::missing_gold(i64::from(gold) - treasury.max(0))))
}

/// Interval in ticks until kingdom `id` sends its next caravan.
fn interval_ticks(state: &GameState, defs: &Defs, id: KingdomId) -> u64 {
    let rules = &defs.rules().diplomacy.caravans;
    let tick = state.clock.tick;
    let Some(k) = state.diplomacy.kingdom(id) else { return u64::from(rules.interval_neutral_hours) * TICKS_PER_HOUR };
    let hours = if k.has_treaty(super::TreatyKind::TradeAgreement, tick) {
        rules.interval_treaty_hours
    } else if relations::relation_state(state, defs, id) >= RelationState::Friendly {
        rules.interval_friendly_hours
    } else {
        rules.interval_neutral_hours
    };
    let hours = u64::from(hours);
    let hours = if k.personality == crate::defs::Personality::Trader { hours * 3 / 4 } else { hours };
    hours.max(1) * TICKS_PER_HOUR
}

/// Units of an order the kingdom may still trade today.
fn order_left(state: &GameState, order: &TradeOrder) -> u32 {
    let v = state.economy.trade().volume_today(order.resource);
    order.daily_limit.saturating_sub(v.bought + v.sold)
}

/// The cargo a kingdom brings for the player's buy orders and the purse for its sell orders.
fn plan_visit(state: &GameState, defs: &Defs, id: KingdomId) -> (Vec<ResourceAmount>, i64) {
    let Some(k) = state.diplomacy.kingdom(id) else { return (Vec::new(), 0) };
    let rules = &defs.rules().diplomacy.caravans;
    let mut cargo = Vec::new();
    let mut room = rules.capacity;
    let mut purse = 0i64;
    for order in state.economy.trade().orders() {
        let r = order.resource;
        let left = order_left(state, order);
        if let Some(threshold) = order.buy_below {
            let need = threshold.saturating_sub(state.economy.stock().get(r)).min(left);
            let bring = need.min(economy::offered_units(defs, k, r)).min(room);
            if bring > 0 {
                cargo.push(ResourceAmount { resource: r, amount: bring });
                room -= bring;
            }
        }
        if let Some(threshold) = order.sell_above {
            let excess = state.economy.available(r).saturating_sub(threshold).min(left);
            let buy = excess.min(economy::wanted_units(defs, k, r));
            if buy > 0 {
                let unit = economy::unit_price_centi(state, defs, id, r, TradeDirection::Export);
                purse += i64::from(buy) * i64::from(unit) * (MILLI_PER_GOLD / 100);
            }
        }
    }
    let cap = k.treasury_milli.max(0) * i64::from(rules.purse_percent) / 100;
    (cargo, purse.min(cap))
}

/// Hourly: kingdoms with access whose next caravan is due send one if they have goods to trade.
fn dispatch(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let Some(post) = active_trading_post(state, defs) else { return };
    let tick = state.clock.tick;
    let retry = u64::from(defs.rules().diplomacy.caravans.retry_hours) * TICKS_PER_HOUR;
    for i in 0..state.diplomacy.kingdoms.len() {
        let id = KingdomId::from_raw(i as u8);
        if tick < state.diplomacy.kingdoms[i].next_caravan_tick
            || !has_access(state, defs, id)
            || state.diplomacy.caravans.iter().any(|c| c.kind == CaravanKind::Foreign && c.kingdom == id)
        {
            continue;
        }
        let (cargo, purse) = plan_visit(state, defs, id);
        if cargo.is_empty() && purse <= 0 {
            state.diplomacy.kingdoms[i].next_caravan_tick = tick + retry;
            continue;
        }
        let travel = travel_ticks(state, defs, id);
        let interval = interval_ticks(state, defs, id);
        let caravan_id = state.diplomacy.next_caravan;
        state.diplomacy.next_caravan = caravan_id.wrapping_add(1).max(1);
        let mut caravan = Caravan {
            id: caravan_id,
            kingdom: id,
            kind: CaravanKind::Foreign,
            stage: CaravanStage::Travelling { until_tick: tick + travel, outward: true },
            cargo: Vec::new(),
            purse_milli: purse.max(0),
            wishes: Vec::new(),
            walkers: Vec::new(),
            entry: state.world.start,
            post: Some(post),
            cart_due_tick: None,
            stuck_since: None,
            created_tick: tick,
            sold_milli: 0,
            bought_milli: 0,
            start_purse_milli: purse.max(0),
            route: RouteTally::default(),
        };
        let k = &mut state.diplomacy.kingdoms[i];
        k.treasury_milli -= purse.max(0);
        for m in cargo {
            let taken = economy::take(k, m.resource, m.amount);
            caravan.add(m.resource, taken);
        }
        k.next_caravan_tick = tick + interval + 2 * travel;
        state.diplomacy.caravans.push(caravan);
        events.push(SimEvent::CaravanDeparted { caravan: caravan_id, kingdom: id, kind: CaravanKind::Foreign });
    }
}

/// Rolls the robbery of caravan `index` on an off-map leg; robbed goods and gold go to the
/// robbing kingdom. Returns true if it was robbed.
fn roll_robbery(state: &mut GameState, defs: &Defs, index: usize, events: &mut Vec<SimEvent>) -> bool {
    let c = &state.diplomacy.caravans[index];
    if c.cargo.is_empty() && c.purse_milli <= 0 {
        return false;
    }
    let (chance, robber) = route_danger(state, defs, c.kingdom);
    let Some(robber) = robber else { return false };
    if state.diplomacy.rng.random_range(0..100u32) >= chance {
        return false;
    }
    let c = &mut state.diplomacy.caravans[index];
    let cargo = std::mem::take(&mut c.cargo);
    let purse = std::mem::take(&mut c.purse_milli);
    let (id, kind, partner) = (c.id, c.kind, c.kingdom);
    let k = &mut state.diplomacy.kingdoms[robber.index()];
    for m in &cargo {
        economy::give(k, m.resource, m.amount);
    }
    k.treasury_milli += purse;
    let robber_name = k.name.clone();
    let robber_seat = k.seat;
    let partner_name = state.diplomacy.kingdoms[partner.index()].name.clone();
    let tick = state.clock.tick;
    events.push(SimEvent::CaravanRobbed { caravan: id, kingdom: partner, robber, goods: cargo });
    let text = match kind {
        CaravanKind::Own => format!("Eure Karawane nach {partner_name} wurde von Leuten aus {robber_name} überfallen und ausgeraubt."),
        CaravanKind::Foreign => format!("Eine Karawane aus {partner_name} wurde auf dem Weg zu Euch von Leuten aus {robber_name} ausgeraubt."),
    };
    state.diplomacy.record(tick, Some(partner), Some(robber), text.clone());
    events.push(SimEvent::Notification(Notification::new(Severity::Warning, text).with_kind(NotificationKind::Trade).at(robber_seat)));
    true
}

/// Sets every walker of caravan `index` on its way to the map edge.
fn leave(state: &mut GameState, index: usize) {
    let c = &mut state.diplomacy.caravans[index];
    c.stage = CaravanStage::Leaving;
    c.stuck_since = None;
    c.route = RouteTally::default();
    if c.kind == CaravanKind::Foreign {
        // A cart that has not entered the map yet stays away.
        c.cart_due_tick = None;
    }
    let (walkers, edge) = (c.walkers.clone(), c.entry);
    for w in walkers {
        movement::set_destination(state, WalkerId::Traveler(w), Some(Destination::MapEdge(edge)));
    }
}

/// Turns every caravan of kingdom `id` back (war): foreign merchants leave without trading,
/// own caravans on the way there return with their goods.
pub(crate) fn turn_back_all(state: &mut GameState, defs: &Defs, id: KingdomId, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    for i in 0..state.diplomacy.caravans.len() {
        let c = &state.diplomacy.caravans[i];
        if c.kingdom != id {
            continue;
        }
        match (c.kind, c.stage) {
            (CaravanKind::Foreign, CaravanStage::Arriving | CaravanStage::Trading { .. }) => leave(state, i),
            (CaravanKind::Foreign, CaravanStage::Travelling { outward: true, .. }) => {
                let travel = travel_ticks(state, defs, id);
                state.diplomacy.caravans[i].stage = CaravanStage::Travelling { until_tick: tick + travel, outward: false };
            }
            (CaravanKind::Own, CaravanStage::Travelling { outward: true, until_tick }) => {
                // Half way back at most.
                let back = until_tick.saturating_sub(tick).min(travel_ticks(state, defs, id));
                state.diplomacy.caravans[i].stage = CaravanStage::Travelling { until_tick: tick + back, outward: false };
            }
            _ => {}
        }
        let _ = events;
    }
}

/// Pipeline part: hourly dispatch, then every caravan's stage.
pub(crate) fn run(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    if state.clock.is_new_hour() {
        dispatch(state, defs, events);
    }
    let ids: Vec<u32> = state.diplomacy.caravans.iter().map(|c| c.id).collect();
    for id in ids {
        step(state, defs, id, events);
    }
}

/// True if caravan walker `walker` stands at its destination.
fn walker_arrived(state: &GameState, walker: TravelerId) -> bool {
    state
        .population
        .walking
        .travelers
        .get(walker)
        .is_some_and(|t| t.motion.phase() == TripPhase::Still && t.motion.destination().is_some() && t.motion.is_at_destination())
}

/// True if the merchant (first walker) of caravan `index` stands at its destination.
fn merchant_arrived(state: &GameState, index: usize) -> bool {
    state.diplomacy.caravans[index].walkers.first().is_some_and(|w| walker_arrived(state, *w))
}

/// Spawns the cart once it is due and keeps walkers without a path retrying; returns true if
/// the caravan has been stuck for `stuck_hours`.
fn tend_walkers(state: &mut GameState, defs: &Defs, index: usize, dest: Option<Destination>, from: IVec2) -> bool {
    let tick = state.clock.tick;
    let rules = &defs.rules().diplomacy.caravans;
    let c = &state.diplomacy.caravans[index];
    if c.cart_due_tick.is_some_and(|t| tick >= t) {
        let caravan = c.id;
        let cart = spawn_walker(state, caravan, CitizenRole::Cart, from);
        let c = &mut state.diplomacy.caravans[index];
        c.walkers.push(cart);
        c.cart_due_tick = None;
        let cart_dest = match (dest, c.post) {
            (Some(Destination::Spot { .. }), Some(post)) => post_destination(state, post, true).or(dest),
            _ => dest,
        };
        movement::set_destination(state, WalkerId::Traveler(cart), cart_dest);
    }
    let walkers = state.diplomacy.caravans[index].walkers.clone();
    let mut stuck = false;
    for w in walkers {
        let Some(t) = state.population.walking.travelers.get(w) else { continue };
        if let TripPhase::NoPath { retry_at } = t.motion.phase() {
            stuck = true;
            if tick >= retry_at {
                let d = t.motion.destination();
                movement::set_destination(state, WalkerId::Traveler(w), d);
            }
        }
    }
    let c = &mut state.diplomacy.caravans[index];
    if stuck {
        let since = *c.stuck_since.get_or_insert(tick);
        tick.saturating_sub(since) >= u64::from(rules.stuck_hours) * TICKS_PER_HOUR
    } else {
        c.stuck_since = None;
        false
    }
}

/// Advances caravan `id` by one tick.
fn step(state: &mut GameState, defs: &Defs, id: u32, events: &mut Vec<SimEvent>) {
    let Some(i) = index_of(state, id) else { return };
    let tick = state.clock.tick;
    let c = state.diplomacy.caravans[i].clone();
    // Walkers that left the map are gone.
    state.diplomacy.caravans[i].walkers.retain(|w| state.population.walking.travelers.contains(*w));
    match (c.kind, c.stage) {
        (_, CaravanStage::Travelling { until_tick, .. }) if tick < until_tick => {}
        (CaravanKind::Foreign, CaravanStage::Travelling { outward: true, .. }) => {
            if roll_robbery(state, defs, i, events) {
                // The robbed merchants go home empty-handed.
                finish(state, i);
                return;
            }
            let post = active_trading_post(state, defs);
            if !has_access(state, defs, c.kingdom) || post.is_none() {
                let travel = travel_ticks(state, defs, c.kingdom);
                state.diplomacy.caravans[i].stage = CaravanStage::Travelling { until_tick: tick + travel, outward: false };
                return;
            }
            arrive_on_map(state, defs, i, post, events);
        }
        (CaravanKind::Foreign, CaravanStage::Travelling { outward: false, .. }) => {
            if !roll_robbery(state, defs, i, events) {
                let c = &mut state.diplomacy.caravans[i];
                let cargo = std::mem::take(&mut c.cargo);
                let purse = std::mem::take(&mut c.purse_milli);
                let k = &mut state.diplomacy.kingdoms[c.kingdom.index()];
                for m in cargo {
                    economy::give(k, m.resource, m.amount);
                }
                k.treasury_milli += purse;
            }
            finish(state, i);
        }
        (CaravanKind::Own, CaravanStage::Travelling { outward: true, .. }) => {
            if !roll_robbery(state, defs, i, events) {
                trade_at_partner(state, defs, i, events);
            }
            let travel = travel_ticks(state, defs, c.kingdom);
            state.diplomacy.caravans[i].stage = CaravanStage::Travelling { until_tick: tick + travel, outward: false };
        }
        (CaravanKind::Own, CaravanStage::Travelling { outward: false, .. }) => {
            if roll_robbery(state, defs, i, events) {
                settle_own(state, defs, i, events);
                finish(state, i);
                return;
            }
            let post = c.post.filter(|p| state.buildings.get(*p).is_some()).or_else(|| storage::main_headquarters(state, defs));
            arrive_on_map(state, defs, i, post, events);
        }
        (kind, CaravanStage::Arriving) => {
            let post = c.post.filter(|p| state.buildings.get(*p).is_some());
            let dest = post.and_then(|p| post_destination(state, p, false));
            let entry = c.entry;
            let stuck = tend_walkers(state, defs, i, dest, entry);
            count_route(state, defs, i);
            let foreign_blocked = kind == CaravanKind::Foreign
                && (!has_access(state, defs, c.kingdom) || active_trading_post(state, defs) != post || post.is_none());
            if foreign_blocked {
                leave(state, i);
                return;
            }
            let everyone_in = state.diplomacy.caravans[i].cart_due_tick.is_none()
                && state.diplomacy.caravans[i].walkers.iter().all(|w| walker_arrived(state, *w));
            if merchant_arrived(state, i) && (kind == CaravanKind::Foreign || everyone_in) {
                match kind {
                    CaravanKind::Foreign => {
                        trade_at_post(state, defs, i, events);
                        let hours = u64::from(defs.rules().diplomacy.caravans.trade_hours);
                        state.diplomacy.caravans[i].stage = CaravanStage::Trading { until_tick: tick + hours * TICKS_PER_HOUR };
                    }
                    CaravanKind::Own => {
                        for w in state.diplomacy.caravans[i].walkers.clone() {
                            remove_walker(state, w);
                        }
                        settle_own(state, defs, i, events);
                        finish(state, i);
                    }
                }
                return;
            }
            let walkers_gone = state.diplomacy.caravans[i].walkers.is_empty() && state.diplomacy.caravans[i].cart_due_tick.is_none();
            if stuck || walkers_gone || dest.is_none() {
                // No way to the post (or it vanished): foreign merchants turn back, an own
                // caravan unloads where it can.
                for w in state.diplomacy.caravans[i].walkers.clone() {
                    if stuck || dest.is_none() {
                        remove_walker(state, w);
                    }
                }
                match kind {
                    CaravanKind::Foreign => {
                        state.diplomacy.caravans[i].walkers.clear();
                        state.diplomacy.caravans[i].cart_due_tick = None;
                        let travel = travel_ticks(state, defs, c.kingdom);
                        state.diplomacy.caravans[i].stage = CaravanStage::Travelling { until_tick: tick + travel, outward: false };
                    }
                    CaravanKind::Own => {
                        state.diplomacy.caravans[i].walkers.clear();
                        settle_own(state, defs, i, events);
                        finish(state, i);
                    }
                }
            }
        }
        (_, CaravanStage::Trading { until_tick }) => {
            let post = c.post.filter(|p| state.buildings.get(*p).is_some());
            let dest = post.and_then(|p| post_destination(state, p, false));
            let entry = c.entry;
            tend_walkers(state, defs, i, dest, entry);
            if tick >= until_tick || !has_access(state, defs, c.kingdom) || active_trading_post(state, defs) != post {
                leave(state, i);
            }
        }
        (kind, CaravanStage::Leaving) => {
            let from = c.post.and_then(|p| state.buildings.get(p)).map_or(c.entry, |b| b.entrance());
            let stuck = tend_walkers(state, defs, i, Some(Destination::MapEdge(c.entry)), from);
            count_route(state, defs, i);
            let c = &state.diplomacy.caravans[i];
            if stuck {
                for w in c.walkers.clone() {
                    remove_walker(state, w);
                }
                state.diplomacy.caravans[i].walkers.clear();
                state.diplomacy.caravans[i].cart_due_tick = None;
            }
            let c = &state.diplomacy.caravans[i];
            if c.walkers.is_empty() && c.cart_due_tick.is_none() {
                let travel = travel_ticks(state, defs, c.kingdom);
                let outward = kind == CaravanKind::Own;
                state.diplomacy.caravans[i].stage = CaravanStage::Travelling { until_tick: tick + travel, outward };
            }
        }
    }
}

/// Puts the walkers of caravan `index` on the map edge towards its kingdom, heading for `post`
/// (no reachable edge or post: the stage resolves without walkers next tick).
fn arrive_on_map(state: &mut GameState, defs: &Defs, index: usize, post: Option<BuildingId>, events: &mut Vec<SimEvent>) {
    let c = &state.diplomacy.caravans[index];
    let seat = state.diplomacy.kingdoms[c.kingdom.index()].seat;
    let target = post.and_then(|p| state.buildings.get(p)).map(|b| b.entrance());
    let entry = target.and_then(|t| edge_towards(state, defs, t, seat));
    let tick = state.clock.tick;
    let c = &mut state.diplomacy.caravans[index];
    c.post = post;
    c.stage = CaravanStage::Arriving;
    c.stuck_since = None;
    c.route = RouteTally::default();
    let (Some(entry), Some(post)) = (entry, post) else {
        c.walkers.clear();
        c.cart_due_tick = None;
        return;
    };
    c.entry = entry;
    let caravan = c.id;
    let (kingdom, kind) = (c.kingdom, c.kind);
    let merchant = spawn_walker(state, caravan, CitizenRole::Merchant, entry);
    let c = &mut state.diplomacy.caravans[index];
    c.walkers = vec![merchant];
    c.cart_due_tick = Some(tick + u64::from(defs.rules().diplomacy.caravans.cart_delay_ticks));
    let dest = post_destination(state, post, false);
    movement::set_destination(state, WalkerId::Traveler(merchant), dest);
    if kind == CaravanKind::Foreign {
        let name = state.diplomacy.kingdoms[kingdom.index()].name.clone();
        events.push(SimEvent::CaravanArrived { caravan, kingdom });
        let text = format!("Händler aus {name} nähern sich Eurem Handelskontor.");
        events.push(SimEvent::Notification(Notification::new(Severity::Info, text).with_kind(NotificationKind::Trade).at(entry)));
    }
}

/// Removes caravan `index` (its walkers are gone or removed).
fn finish(state: &mut GameState, index: usize) {
    let c = state.diplomacy.caravans.remove(index);
    for w in c.walkers {
        if state.population.walking.travelers.contains(w) {
            remove_walker(state, w);
        }
    }
}

/// Foreign merchants at the trading post: they sell what the player's buy orders want and buy
/// what the sell orders offer, as far as cargo, purse, storage room, the treasury and the
/// daily limits allow.
fn trade_at_post(state: &mut GameState, defs: &Defs, index: usize, events: &mut Vec<SimEvent>) {
    let c = &state.diplomacy.caravans[index];
    let kingdom = c.kingdom;
    // The merchants' walk in: trade roads improve the prices.
    let route = c.route.bonus_percent();
    let Some(at) = c.post.and_then(|p| state.buildings.get(p)).map(|b| b.entrance()) else { return };
    let capacity = defs.rules().diplomacy.caravans.capacity;
    let orders: Vec<TradeOrder> = state.economy.trade().orders().copied().collect();
    let mut sold = Vec::new();
    let mut bought = Vec::new();
    for order in orders {
        let r = order.resource;
        let mut left = order_left(state, &order);
        if let Some(threshold) = order.buy_below
            && left > 0
            && !state.economy.is_bankrupt()
        {
            let unit = economy::unit_price_with_route_centi(state, defs, kingdom, r, TradeDirection::Import, route);
            let unit_milli = i64::from(unit) * (MILLI_PER_GOLD / 100);
            let stock = state.economy.stock().get(r);
            let room = state.economy.capacity().get(r).saturating_sub(stock);
            let affordable = u32::try_from(state.economy.treasury_milli().max(0) / unit_milli.max(1)).unwrap_or(u32::MAX);
            let carried = state.diplomacy.caravans[index].carried(r);
            let n = threshold.saturating_sub(stock).min(left).min(room).min(affordable).min(carried);
            if n > 0 {
                let stored = storage::deposit(state, defs, r, n, at);
                if stored > 0 {
                    let milli = i64::from(stored) * unit_milli;
                    let c = &mut state.diplomacy.caravans[index];
                    c.remove(r, stored);
                    c.purse_milli += milli;
                    book_trade(state, r, stored, -milli, TradeDirection::Import, events);
                    state.economy.trade_mut().record(r, TradeDirection::Import, stored);
                    bought.push(ResourceAmount { resource: r, amount: stored });
                    left -= stored;
                }
            }
        }
        if let Some(threshold) = order.sell_above
            && left > 0
        {
            let unit = economy::unit_price_with_route_centi(state, defs, kingdom, r, TradeDirection::Export, route);
            let unit_milli = i64::from(unit) * (MILLI_PER_GOLD / 100);
            let c = &state.diplomacy.caravans[index];
            let affordable = u32::try_from(c.purse_milli.max(0) / unit_milli.max(1)).unwrap_or(u32::MAX);
            let space = capacity.saturating_sub(c.units());
            let wanted = economy::wanted_units(defs, &state.diplomacy.kingdoms[kingdom.index()], r).saturating_sub(c.carried(r));
            let excess = state.economy.available(r).saturating_sub(threshold);
            let n = excess.min(left).min(affordable).min(space).min(wanted);
            if n > 0 {
                let taken = storage::withdraw(state, defs, r, n, at);
                if taken > 0 {
                    let milli = i64::from(taken) * unit_milli;
                    let c = &mut state.diplomacy.caravans[index];
                    c.add(r, taken);
                    c.purse_milli -= milli;
                    book_trade(state, r, taken, milli, TradeDirection::Export, events);
                    state.economy.trade_mut().record(r, TradeDirection::Export, taken);
                    sold.push(ResourceAmount { resource: r, amount: taken });
                }
            }
        }
    }
    let name = state.diplomacy.kingdoms[kingdom.index()].name.clone();
    let caravan = state.diplomacy.caravans[index].id;
    if sold.is_empty() && bought.is_empty() {
        let text = format!("Die Händler aus {name} fanden am Handelskontor nichts zu handeln.");
        events.push(SimEvent::Notification(Notification::new(Severity::Info, text).with_kind(NotificationKind::Trade).at(at)));
        return;
    }
    let points = defs.rules().diplomacy.relations.trade_points;
    let cap = defs.rules().diplomacy.relations.trade_max;
    remember(&mut state.diplomacy.kingdoms[kingdom.index()], defs, MemoryCause::Trade, points, Some(cap));
    let list = |items: &[ResourceAmount]| items.iter().map(|m| format!("{} {}", m.amount, defs.resource(m.resource).name)).collect::<Vec<_>>().join(", ");
    let mut parts = Vec::new();
    if !bought.is_empty() {
        parts.push(format!("Ihr kauft {}", list(&bought)));
    }
    if !sold.is_empty() {
        parts.push(format!("Ihr verkauft {}", list(&sold)));
    }
    let text = format!("Händler aus {name} am Handelskontor: {}.", parts.join("; "));
    let tick = state.clock.tick;
    state.diplomacy.record(tick, Some(kingdom), None, text.clone());
    events.push(SimEvent::CaravanTraded { caravan, kingdom, sold, bought });
    events.push(SimEvent::Notification(Notification::new(Severity::Good, text).with_kind(NotificationKind::Trade).at(at)));
}

/// An own caravan at the partner: it sells what the partner wants and buys the wished goods
/// the partner offers with its purse (no trade if access was lost on the way).
fn trade_at_partner(state: &mut GameState, defs: &Defs, index: usize, events: &mut Vec<SimEvent>) {
    let kingdom = state.diplomacy.caravans[index].kingdom;
    if !has_access(state, defs, kingdom) {
        return;
    }
    // The caravan's walk out from the trading post: trade roads improve the prices.
    let route = state.diplomacy.caravans[index].route.bonus_percent();
    let cargo = state.diplomacy.caravans[index].cargo.clone();
    for m in cargo {
        let k = &state.diplomacy.kingdoms[kingdom.index()];
        let unit = i64::from(economy::unit_price_with_route_centi(state, defs, kingdom, m.resource, TradeDirection::Export, route)) * (MILLI_PER_GOLD / 100);
        let affordable = u32::try_from(k.treasury_milli.max(0) / unit.max(1)).unwrap_or(u32::MAX);
        let n = m.amount.min(economy::wanted_units(defs, k, m.resource)).min(affordable);
        if n == 0 {
            continue;
        }
        let milli = i64::from(n) * unit;
        let k = &mut state.diplomacy.kingdoms[kingdom.index()];
        economy::give(k, m.resource, n);
        k.treasury_milli -= milli;
        let c = &mut state.diplomacy.caravans[index];
        c.remove(m.resource, n);
        c.purse_milli += milli;
        c.sold_milli += milli;
        state.economy.stats_mut().today_mut().exported.add(m.resource, n);
        events.push(SimEvent::TradeExecuted { resource: m.resource, amount: n, gold: milli / MILLI_PER_GOLD, direction: TradeDirection::Export });
    }
    let capacity = defs.rules().diplomacy.caravans.capacity;
    let wishes = state.diplomacy.caravans[index].wishes.clone();
    for w in wishes {
        let k = &state.diplomacy.kingdoms[kingdom.index()];
        let unit = i64::from(economy::unit_price_with_route_centi(state, defs, kingdom, w.resource, TradeDirection::Import, route)) * (MILLI_PER_GOLD / 100);
        let c = &state.diplomacy.caravans[index];
        let affordable = u32::try_from(c.purse_milli.max(0) / unit.max(1)).unwrap_or(u32::MAX);
        let space = capacity.saturating_sub(c.units());
        let n = w.amount.min(economy::offered_units(defs, k, w.resource)).min(affordable).min(space);
        if n == 0 {
            continue;
        }
        let milli = i64::from(n) * unit;
        let k = &mut state.diplomacy.kingdoms[kingdom.index()];
        let taken = economy::take(k, w.resource, n);
        let milli = milli * i64::from(taken) / i64::from(n);
        k.treasury_milli += milli;
        let c = &mut state.diplomacy.caravans[index];
        c.add(w.resource, taken);
        c.purse_milli -= milli;
        c.bought_milli += milli;
        state.economy.stats_mut().today_mut().imported.add(w.resource, taken);
        events.push(SimEvent::TradeExecuted { resource: w.resource, amount: taken, gold: -(milli / MILLI_PER_GOLD), direction: TradeDirection::Import });
    }
    let points = defs.rules().diplomacy.relations.trade_points;
    let cap = defs.rules().diplomacy.relations.trade_max;
    remember(&mut state.diplomacy.kingdoms[kingdom.index()], defs, MemoryCause::Trade, points, Some(cap));
}

/// An own caravan is back (or ends on the way): goods into the storages near its post (what
/// does not fit is lost and reported), the purse back to the treasury, booked as sales
/// (exports) and the unspent part of the starting purse (imports).
fn settle_own(state: &mut GameState, defs: &Defs, index: usize, events: &mut Vec<SimEvent>) {
    let c = &mut state.diplomacy.caravans[index];
    let cargo = std::mem::take(&mut c.cargo);
    let purse = std::mem::take(&mut c.purse_milli);
    let (sold, bought, start) = (c.sold_milli, c.bought_milli, c.start_purse_milli);
    let (kingdom, caravan) = (c.kingdom, c.id);
    let at = c.post.and_then(|p| state.buildings.get(p)).map_or_else(|| home_point(state, defs), |b| b.entrance());
    storage::deposit_or_lose(state, defs, &cargo, at, events);
    // purse = start + sold − bought − robbed; book sales and the refund so that the ledger
    // shows the net trade.
    let refund = purse - sold.min(purse);
    if purse > 0 {
        state.economy.book_milli(LedgerCategory::Exports, sold.min(purse));
        state.economy.book_milli(LedgerCategory::Imports, refund);
    }
    let name = state.diplomacy.kingdoms[kingdom.index()].name.clone();
    let tick = state.clock.tick;
    let robbed = purse == 0 && cargo.is_empty() && (start > 0 || sold > 0);
    let text = if robbed {
        format!("Eure Karawane aus {name} kehrt mit leeren Händen zurück.")
    } else {
        let goods: Vec<String> = cargo.iter().map(|m| format!("{} {}", m.amount, defs.resource(m.resource).name)).collect();
        let goods = if goods.is_empty() { String::from("ohne Waren") } else { format!("mit {}", goods.join(", ")) };
        format!(
            "Eure Karawane ist aus {name} zurück: {goods}, {} Gold in der Kasse (verkauft für {}, gekauft für {} Gold).",
            purse / MILLI_PER_GOLD,
            sold / MILLI_PER_GOLD,
            bought / MILLI_PER_GOLD
        )
    };
    state.diplomacy.record(tick, Some(kingdom), None, text.clone());
    events.push(SimEvent::CaravanReturned { caravan, kingdom, goods: cargo, gold: purse / MILLI_PER_GOLD });
    events.push(SimEvent::Notification(Notification::new(Severity::Good, text).with_kind(NotificationKind::Trade).at(at)));
}

/// Consistency of the caravans and their walkers (loaded saves): unique ids below the next id,
/// known kingdoms, sorted positive cargo within limits, no negative purse, walkers that exist
/// and name their caravan, and every caravan walker listed by its caravan.
pub(crate) fn validate(state: &GameState, defs: &Defs) -> Result<(), String> {
    let d = &state.diplomacy;
    let resources = defs.resources().len();
    for (i, c) in d.caravans.iter().enumerate() {
        if d.caravans[..i].iter().any(|o| o.id == c.id) || c.id >= d.next_caravan || c.kingdom.index() >= d.kingdoms.len() {
            return Err(format!("caravan {}: duplicate id or unknown kingdom", c.id));
        }
        let sorted = c.cargo.windows(2).all(|w| w[0].resource < w[1].resource);
        if !sorted || c.cargo.iter().any(|m| m.amount == 0 || m.resource.index() >= resources) || c.purse_milli < 0 {
            return Err(format!("caravan {}: malformed cargo or purse", c.id));
        }
        if c.wishes.iter().any(|m| m.resource.index() >= resources) {
            return Err(format!("caravan {}: unknown wished good", c.id));
        }
        if !state.world.in_bounds(c.entry) || c.route.last.is_some_and(|p| !state.world.in_bounds(p)) {
            return Err(format!("caravan {}: entry or route outside the map", c.id));
        }
        for w in &c.walkers {
            match state.population.walking.travelers.get(*w) {
                Some(t) if t.caravan == Some(c.id) => {}
                _ => return Err(format!("caravan {}: walker {w:?} missing or not its own", c.id)),
            }
        }
        if matches!(c.stage, CaravanStage::Travelling { .. }) && !c.walkers.is_empty() {
            return Err(format!("caravan {}: walkers on the map while off the map", c.id));
        }
    }
    for (tid, t) in state.population.walking.travelers.iter() {
        if let Some(cid) = t.caravan
            && !d.caravans.iter().any(|c| c.id == cid && c.walkers.contains(&tid))
        {
            return Err(format!("caravan walker {tid:?} without its caravan {cid}"));
        }
    }
    Ok(())
}
