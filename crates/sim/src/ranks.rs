//! Ruler ranks and defeat (docs/game_design.md §1).
//!
//! **Ranks** (`rules.ron` → `ranks`): Siedlung → Dorf → Marktflecken → Stadt → Großstadt →
//! Königreich → Imperium. Every rank asks for inhabitants, wealth (treasury plus the kingdom
//! stock at base prices), finished buildings (without walls and gates), territory tiles and
//! possibly a finished building of a type. Checked every game hour; a rank once reached is
//! kept. Each rank brings prestige: relation points with every AI kingdom (§10).
//!
//! **Defeat:** the headquarters is conquered (an enemy army holds it, §11) or nobody lives in
//! the kingdom any more. The game is then over: [`GameState::tick`] applies no commands and
//! advances nothing; the report says why.

use serde::{Deserialize, Serialize};

use crate::defs::{BuildingTypeId, Defs, RankDef};
use crate::diplomacy::KingdomId;
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::state::GameState;

/// Why the game was lost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefeatReason {
    /// An army of the kingdom held the broken headquarters.
    Conquered {
        /// The conqueror.
        kingdom: KingdomId,
    },
    /// Nobody lives in the kingdom any more.
    Depopulated,
}

/// The end of a lost game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameOver {
    /// Why.
    pub reason: DefeatReason,
    /// Tick of the defeat.
    pub tick: u64,
}

/// Rank progress and the end of the game.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// Index of the current rank in `Defs::ranks`.
    pub(crate) rank: u8,
    /// Ranks reached and when (tick), in order.
    pub(crate) reached: Vec<(u8, u64)>,
    /// Set once the game is lost.
    pub(crate) game_over: Option<GameOver>,
}

/// Court upkeep (Hofhaltung, docs/game_design.md §7) of the current rank in gold per day.
pub fn court_upkeep(state: &GameState, defs: &Defs) -> u32 {
    defs.ranks().get(usize::from(state.progress.rank)).map_or(0, |r| r.court_upkeep)
}

impl Progress {
    /// Index of the current rank.
    pub fn rank(&self) -> u8 {
        self.rank
    }

    /// Ranks reached after the start, with their tick.
    pub fn reached(&self) -> &[(u8, u64)] {
        &self.reached
    }

    /// The defeat, if the game is lost.
    pub fn game_over(&self) -> Option<GameOver> {
        self.game_over
    }

    pub(crate) fn validate(&self, defs: &Defs) -> Result<(), String> {
        if usize::from(self.rank) >= defs.ranks().len().max(1) {
            return Err(format!("rank {} does not exist", self.rank));
        }
        if self.reached.windows(2).any(|w| w[0].0 >= w[1].0 || w[0].1 > w[1].1) || self.reached.iter().any(|(r, _)| *r > self.rank) {
            return Err("reached ranks out of order".into());
        }
        Ok(())
    }
}

/// What a rank requirement counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequirementKind {
    /// Inhabitants.
    Population,
    /// Treasury plus stock at base prices ([`wealth`]).
    Wealth,
    /// Finished buildings ([`counted_buildings`]).
    Buildings,
    /// Tiles of the own territory.
    Territory,
    /// A finished building of this type.
    Building(BuildingTypeId),
}

/// One requirement of a rank with the current value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankRequirement {
    /// What it counts.
    pub kind: RequirementKind,
    /// German label („Einwohner“, „Wohlstand (Gold und Waren)“, „Gebäude“, „Gebiet (Felder)“,
    /// „Marktplatz“).
    pub label: String,
    /// Current value.
    pub current: u64,
    /// Required value.
    pub required: u64,
    /// True once met.
    pub done: bool,
    /// Progress 0–100.
    pub percent: u8,
}

/// Wealth of the kingdom: treasury (at least 0) plus the kingdom stock at base prices.
pub fn wealth(state: &GameState, defs: &Defs) -> u64 {
    let stock: u64 = defs.resources().iter().map(|r| u64::from(state.economy.stock().get(r.id)) * u64::from(r.base_price)).sum();
    u64::try_from(state.economy.treasury().max(0)).unwrap_or(0) + stock
}

/// Finished buildings counted for the ranks (without walls and gates).
pub fn counted_buildings(state: &GameState, defs: &Defs) -> u32 {
    let n = state.buildings.iter().filter(|(_, b)| b.is_complete() && !crate::buildings::is_wall_or_gate(defs, b)).count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The requirements of `rank` with the kingdom's current values.
pub fn requirements(state: &GameState, defs: &Defs, rank: &RankDef) -> Vec<RankRequirement> {
    let row = |kind: RequirementKind, label: &str, current: u64, required: u64| RankRequirement {
        kind,
        label: label.to_owned(),
        current,
        required,
        done: current >= required,
        percent: (current.min(required) * 100).checked_div(required).map_or(100, |p| u8::try_from(p.min(100)).unwrap_or(100)),
    };
    let mut out = Vec::new();
    if rank.population > 0 {
        out.push(row(RequirementKind::Population, "Einwohner", state.population.len() as u64, u64::from(rank.population)));
    }
    if rank.wealth > 0 {
        out.push(row(RequirementKind::Wealth, "Wohlstand (Gold und Waren)", wealth(state, defs), u64::from(rank.wealth)));
    }
    if rank.buildings > 0 {
        out.push(row(RequirementKind::Buildings, "Gebäude", u64::from(counted_buildings(state, defs)), u64::from(rank.buildings)));
    }
    if rank.territory > 0 {
        out.push(row(RequirementKind::Territory, "Gebiet (Felder)", u64::from(state.territory.owned_tiles()), u64::from(rank.territory)));
    }
    if let Some(kind) = rank.building {
        let has = state.buildings.iter().any(|(_, b)| b.kind() == kind && b.is_complete());
        let name = defs.buildings().get(kind.index()).map_or("Gebäude", |d| d.name.as_str());
        out.push(row(RequirementKind::Building(kind), name, u64::from(has), 1));
    }
    out
}

/// The current rank.
pub fn current_rank<'a>(state: &GameState, defs: &'a Defs) -> Option<&'a RankDef> {
    defs.ranks().get(usize::from(state.progress.rank))
}

/// Relation points of the current rank with every AI kingdom.
pub fn prestige(state: &GameState, defs: &Defs) -> i32 {
    current_rank(state, defs).map_or(0, |r| i32::from(r.prestige))
}

/// German text of a defeat.
pub fn defeat_text(state: &GameState, reason: DefeatReason) -> String {
    match reason {
        DefeatReason::Conquered { kingdom } => {
            let name = state.diplomacy.kingdom(kingdom).map_or("einem feindlichen Reich", |k| k.name.as_str());
            format!("Das Hauptquartier ist gefallen: {name} hat Euer Reich erobert.")
        }
        DefeatReason::Depopulated => "Niemand lebt mehr in Eurem Reich.".to_owned(),
    }
}

/// The game is lost.
pub(crate) fn defeat(state: &mut GameState, _defs: &Defs, reason: DefeatReason, events: &mut Vec<SimEvent>) {
    if state.progress.game_over.is_some() || !state.settings.defeat_enabled {
        return;
    }
    state.progress.game_over = Some(GameOver { reason, tick: state.clock.tick });
    events.push(SimEvent::GameOver { reason });
    let text = format!("Das Spiel ist verloren. {}", defeat_text(state, reason));
    events.push(SimEvent::Notification(Notification::new(Severity::Danger, text).with_kind(NotificationKind::GameOver)));
    log::info!("game over at tick {}: {reason:?}", state.clock.tick);
}

/// Pipeline step 9 (last part): every game hour defeat by depopulation and the next ranks.
pub(crate) fn run(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    if state.progress.game_over.is_some() || !state.clock.is_new_hour() {
        return;
    }
    if state.population.is_empty() {
        defeat(state, defs, DefeatReason::Depopulated, events);
        return;
    }
    loop {
        let next = usize::from(state.progress.rank) + 1;
        let Some(rank) = defs.ranks().get(next) else { break };
        if !requirements(state, defs, rank).iter().all(|r| r.done) {
            break;
        }
        let index = u8::try_from(next).unwrap_or(u8::MAX);
        state.progress.rank = index;
        state.progress.reached.push((index, state.clock.tick));
        events.push(SimEvent::RankAchieved { rank: index });
        let text = format!(
            "Euer Reich ist zum Rang „{}“ aufgestiegen! Euer Ansehen bei den anderen Königreichen wächst ({:+} Beziehung).",
            rank.name, rank.prestige
        );
        events.push(SimEvent::Notification(Notification::new(Severity::Good, text).with_kind(NotificationKind::Rank)));
        crate::diplomacy::relations::announce_states(state, defs, events);
    }
}
