//! Developer aid `KINGDOMS_DEV_TRADE=1`: sets up trade with the other kingdoms through regular
//! commands so caravans can be checked on the map and in the trade tab. It unlocks all
//! technologies (`DebugUnlockAllResearch`, the trading post needs research), grants gold and goods
//! (`DebugGrant`), lets households move in for the staff (`DebugSpawnHouseholds`), places a
//! trading post near the headquarters (`PlaceBuildingWithAccess`) and, once it works, sets
//! standing orders that match what the first kingdom with trade access offers and wants
//! (`SetTradeOrder`, so its merchants come) and sends an own caravan there (`SendCaravan`, checked
//! with `caravans::send_problem` first). Rejections are logged and retried an hour later.

use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{Defs, ResourceAmount};
use kingdoms_sim::diplomacy::caravans;
use kingdoms_sim::diplomacy::has_access;
use kingdoms_sim::economy::TradeOrder;
use kingdoms_sim::economy::trade::active_trading_post;
use kingdoms_sim::report;
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::TICKS_PER_HOUR;

use super::dev_settlement::{cheapest_road, headquarters_center, town_spot};

/// The developer trade plan.
#[derive(Debug, Default)]
pub struct DevTrade {
    placed: bool,
    orders_set: bool,
    caravan_sent: bool,
    next_tick: u64,
}

impl DevTrade {
    /// The plan and its first commands (gold, goods, settlers).
    pub fn start(defs: &Defs) -> (Self, Vec<Command>) {
        let goods = [("wood", 200), ("stone", 100), ("planks", 150), ("tools", 40), ("cloth", 40), ("bread", 150)]
            .into_iter()
            .filter_map(|(key, amount)| defs.resource_id(key).map(|resource| ResourceAmount { resource, amount }))
            .collect();
        log::info!("KINGDOMS_DEV_TRADE: setting up trade");
        let commands = vec![Command::DebugUnlockAllResearch, Command::DebugGrant { gold: 3000, goods }, Command::DebugSpawnHouseholds { count: 4 }];
        (Self::default(), commands)
    }

    /// Commands after a tick.
    pub fn after_tick(&mut self, state: &GameState, defs: &Defs) -> Vec<Command> {
        let now = state.clock().tick;
        if now < self.next_tick || self.caravan_sent {
            return Vec::new();
        }
        self.next_tick = now + TICKS_PER_HOUR;
        if !self.placed {
            self.placed = true;
            let (Some(center), Some(road), Some(kind)) = (headquarters_center(state, defs), cheapest_road(state, defs), defs.building_id("trading_post"))
            else {
                return Vec::new();
            };
            return match town_spot(state, defs, kind, center, 4..20, false, road, |_| 0) {
                Some((origin, rotation)) => {
                    log::info!("KINGDOMS_DEV_TRADE: trading post at {origin}");
                    vec![Command::PlaceBuildingWithAccess { kind, origin, rotation, road }]
                }
                None => {
                    log::warn!("KINGDOMS_DEV_TRADE: no spot for the trading post");
                    Vec::new()
                }
            };
        }
        if active_trading_post(state, defs).is_none() {
            return Vec::new();
        }
        let Some((partner, _)) = state.diplomacy().kingdoms().find(|(id, _)| has_access(state, defs, *id)) else { return Vec::new() };
        let prices = report::partner_prices(state, defs, partner);
        let mut commands = Vec::new();
        if !self.orders_set {
            self.orders_set = true;
            for p in prices.iter().filter(|p| p.offered > 0).take(2) {
                commands.push(Command::SetTradeOrder { order: TradeOrder { resource: p.resource, buy_below: Some(40), sell_above: None, daily_limit: 20 } });
            }
            for p in prices.iter().filter(|p| p.wanted > 0 && state.economy().available(p.resource) > 0).take(2) {
                commands.push(Command::SetTradeOrder { order: TradeOrder { resource: p.resource, buy_below: None, sell_above: Some(10), daily_limit: 20 } });
            }
        }
        let goods: Vec<ResourceAmount> = prices
            .iter()
            .filter(|p| p.wanted > 0)
            .filter_map(|p| {
                let amount = state.economy().available(p.resource).min(p.wanted).min(15);
                (amount > 0).then_some(ResourceAmount { resource: p.resource, amount })
            })
            .take(2)
            .collect();
        let buy: Vec<ResourceAmount> = prices.iter().filter(|p| p.offered > 0).take(1).map(|p| ResourceAmount { resource: p.resource, amount: p.offered.min(10) }).collect();
        match caravans::send_problem(state, defs, partner, &goods, 150, &buy) {
            None => {
                self.caravan_sent = true;
                log::info!("KINGDOMS_DEV_TRADE: caravan sent");
                commands.push(Command::SendCaravan { kingdom: partner, goods, gold: 150, buy });
            }
            Some(problem) => log::info!("KINGDOMS_DEV_TRADE: no caravan yet: {problem}"),
        }
        commands
    }
}
