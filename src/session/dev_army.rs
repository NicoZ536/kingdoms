//! Developer aid `KINGDOMS_DEV_ARMY=<soldiers>`: raises an army through regular commands so the
//! military screens and the soldiers on the map can be checked. It unlocks all technologies
//! (`DebugUnlockAllResearch`), grants gold and equipment (`DebugGrant`), lets households move in
//! (`DebugSpawnHouseholds`), places a barracks and an archer post near the headquarters
//! (`PlaceBuildingWithAccess`), recruits at them once they stand (`Recruit`, the option that is
//! possible now), founds companies from the ready soldiers (`CreateCompany`) and sends the first
//! company on a patrol around the headquarters (`CompanyOrder`). Everything goes through the
//! simulation's own validation; rejections are logged and retried an hour later.

use glam::IVec2;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{Defs, ResourceAmount};
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::military::Order;
use kingdoms_sim::population::CitizenId;
use kingdoms_sim::report;
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::TICKS_PER_HOUR;

use super::dev_settlement::{cheapest_road, headquarters_center, town_spot};

/// Military buildings the plan places.
const BUILDINGS: [&str; 2] = ["barracks", "archer_post"];
/// Soldiers per company.
const COMPANY_SIZE: usize = 8;

/// The developer army plan.
#[derive(Debug)]
pub struct DevArmy {
    /// Soldiers wanted.
    target: u32,
    /// Military buildings placed.
    placed: bool,
    /// The first company got its patrol.
    patrol_sent: bool,
    /// Next tick at which the plan acts.
    next_tick: u64,
}

impl DevArmy {
    /// The plan and its first commands (research, gold, goods, settlers).
    pub fn start(defs: &Defs, target: u32) -> (Self, Vec<Command>) {
        let goods = [("wood", 300), ("stone", 120), ("planks", 120), ("weapons", 40), ("armor", 30), ("leather", 30), ("tools", 20), ("bread", 150)]
            .into_iter()
            .filter_map(|(key, amount)| defs.resource_id(key).map(|resource| ResourceAmount { resource, amount }))
            .collect();
        let households = u16::try_from(target / 2 + 3).unwrap_or(50).min(50);
        let commands = vec![
            Command::DebugUnlockAllResearch,
            Command::DebugGrant { gold: 3000 + 80 * target, goods },
            Command::DebugSpawnHouseholds { count: households },
        ];
        log::info!("KINGDOMS_DEV_ARMY: raising {target} soldiers");
        (Self { target, placed: false, patrol_sent: false, next_tick: 0 }, commands)
    }

    /// Commands after a tick: places the buildings, recruits, founds companies, orders a patrol.
    pub fn after_tick(&mut self, state: &GameState, defs: &Defs, events: &[SimEvent]) -> Vec<Command> {
        for e in events {
            if let SimEvent::CommandRejected { command, reason } = e
                && matches!(command, Command::Recruit { .. } | Command::CreateCompany { .. } | Command::CompanyOrder { .. })
            {
                log::info!("KINGDOMS_DEV_ARMY: {reason}");
            }
        }
        let now = state.clock().tick;
        if now < self.next_tick {
            return Vec::new();
        }
        self.next_tick = now + TICKS_PER_HOUR / 4;
        let mut commands = Vec::new();
        if !self.placed {
            self.placed = true;
            commands.extend(self.place(state, defs));
            return commands;
        }
        let soldiers = u32::try_from(state.military().soldiers().count()).unwrap_or(u32::MAX);
        if soldiers < self.target {
            commands.extend(self.recruit(state, defs, self.target - soldiers));
        }
        let free: Vec<CitizenId> = state
            .military()
            .soldiers()
            .filter(|(_, s)| s.is_ready() && s.company().is_none() && s.crew_of().is_none())
            .map(|(id, _)| id)
            .collect();
        if free.len() >= COMPANY_SIZE.min(self.target as usize) || (soldiers >= self.target && !free.is_empty()) {
            commands.push(Command::CreateCompany { soldiers: free.into_iter().take(COMPANY_SIZE).collect() });
        }
        if !self.patrol_sent
            && let Some(company) = state.military().companies().first()
            && let Some(route) = patrol_route(state, defs)
        {
            self.patrol_sent = true;
            commands.push(Command::CompanyOrder { company: company.id(), order: Order::Patrol { route } });
        }
        commands
    }

    /// Places the barracks and the archer post near the headquarters.
    fn place(&self, state: &GameState, defs: &Defs) -> Vec<Command> {
        let (Some(center), Some(road)) = (headquarters_center(state, defs), cheapest_road(state, defs)) else { return Vec::new() };
        let mut commands = Vec::new();
        for key in BUILDINGS {
            let Some(kind) = defs.building_id(key) else { continue };
            match town_spot(state, defs, kind, center, 4..20, false, road, |_| 0) {
                Some((origin, rotation)) => {
                    log::info!("KINGDOMS_DEV_ARMY: {key} at {origin}");
                    commands.push(Command::PlaceBuildingWithAccess { kind, origin, rotation, road });
                }
                None => log::warn!("KINGDOMS_DEV_ARMY: no spot for {key}"),
            }
        }
        commands
    }

    /// Recruits at every finished military building the first unit possible now.
    fn recruit(&self, state: &GameState, defs: &Defs, missing: u32) -> Vec<Command> {
        let mut left = missing;
        let mut commands = Vec::new();
        for (id, b) in state.buildings().iter().filter(|(_, b)| b.is_complete()) {
            if left == 0 {
                break;
            }
            if defs.buildings().get(b.kind().index()).is_none_or(|d| d.trains.is_empty()) {
                continue;
            }
            let options = report::recruit_options(state, defs, id);
            if let Some(o) = options.iter().filter(|o| o.max_now > 0 && o.population == 1).max_by_key(|o| (o.attack + o.ranged, o.max_now)) {
                let count = o.max_now.min(u8::try_from(left).unwrap_or(u8::MAX));
                left -= u32::from(count);
                commands.push(Command::Recruit { building: id, unit: o.unit, count });
            }
        }
        commands
    }
}

/// Four walkable waypoints around the headquarters (a patrol for the first company).
fn patrol_route(state: &GameState, defs: &Defs) -> Option<Vec<IVec2>> {
    let center = headquarters_center(state, defs)?;
    let corners = [IVec2::new(-7, -7), IVec2::new(7, -7), IVec2::new(7, 7), IVec2::new(-7, 7)];
    let route: Vec<IVec2> = corners
        .iter()
        .filter_map(|c| {
            // The nearest walkable tile to each corner, searching inwards.
            (0..6).map(|i| center + *c - c.signum() * i).find(|p| state.world().in_bounds(*p) && state.navigation().is_walkable(*p))
        })
        .collect();
    (route.len() >= 2).then_some(route)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    /// The plan raises soldiers and companies within two days.
    #[test]
    fn an_army_is_raised_through_regular_commands() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = Defs::load(&dir).unwrap_or_else(|e| panic!("shipped game data must load: {e}"));
        let mut state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let (mut plan, mut commands) = DevArmy::start(&defs, 10);
        for _ in 0..(2 * 24 * TICKS_PER_HOUR) {
            let events = state.tick(&defs, std::mem::take(&mut commands));
            commands = plan.after_tick(&state, &defs, &events);
        }
        let soldiers = state.military().soldiers().count();
        assert!(soldiers >= 6, "{soldiers} soldiers");
        assert!(!state.military().companies().is_empty(), "a company was founded");
        assert!(plan.patrol_sent);
        assert!(matches!(state.military().companies()[0].order(), Order::Patrol { .. }));
    }
}
