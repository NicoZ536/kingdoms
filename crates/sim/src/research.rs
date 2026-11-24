//! Research: researched technologies, the research queue, research progress and the queries
//! other systems need for the effects of researched technologies (docs/game_design.md §9).
//!
//! ## Progress
//! Research points come from a finished headquarters (`research.base_points_per_hour`,
//! independent of its staff) and from buildings with `research_points_per_hour` (schools): such
//! a building adds its rate × its staffing (workers at work, weighted by health and happiness)
//! × the shift factor, like production (rates are round-the-clock values, people work their
//! shift). Researched `ResearchSpeedBonus` raises the total; bankruptcy halves the schools.
//! Every tick the current rate (milli-points per hour) is added to the technology at the front
//! of the queue; [`PROGRESS_PER_POINT`] progress units make one point, so the integer progress
//! is exact.
//!
//! ## Queue
//! [`Command::QueueResearch`](crate::command::Command::QueueResearch) appends a technology
//! whose prerequisites are researched or queued before it; the queue holds at most
//! `research.max_queue` entries. The gold cost is paid when a technology reaches the front and
//! starts (without enough gold it waits, with one warning); points produced while nothing can
//! be researched are lost. Cancelling keeps the progress and the payment, so queueing the
//! technology again continues where it stopped; it also removes queued technologies that
//! depend on it. Reordering keeps every technology behind its prerequisites.
//!
//! ## Completion
//! A finished technology is researched at once: unlocks become available, modifiers apply
//! (storage capacities and territory are re-applied to existing buildings, the job and housing
//! allocation runs again), and a German notification names what it brings.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::command::{CommandResult, RejectReason};
use crate::defs::{BuildingDef, BuildingTypeId, CropId, Defs, RoadTypeId, ServiceKind, TechDef, TechEffect, TechId};
use crate::economy::{LedgerCategory, storage};
use crate::economy::ledger::HISTORY_DAYS;
use crate::event::{Notification, Severity, SimEvent};
use crate::state::GameState;
use crate::time::{HOURS_PER_DAY, TICKS_PER_HOUR};

/// Progress units per research point: rates are milli-points per hour and are added every
/// tick, so one point is 1000 milli-points × the ticks of an hour.
pub const PROGRESS_PER_POINT: u64 = 1000 * TICKS_PER_HOUR;

/// A technology finished by research (not by the developer command).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedTech {
    /// The technology.
    pub tech: TechId,
    /// Tick of completion.
    pub tick: u64,
}

/// Research points of one closed day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchDay {
    /// Day number since the start of the game (0-based).
    pub day: u64,
    /// Points produced that day in milli-points (also those lost without a queued technology).
    pub points_milli: u64,
}

/// Technologies the player has researched, the research queue and its progress.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Research {
    researched: BTreeSet<TechId>,
    /// Queued technologies; the first one is being researched.
    queue: Vec<TechId>,
    /// Progress units per technology (kept when a technology is cancelled).
    progress: BTreeMap<TechId, u64>,
    /// Technologies whose gold cost has been paid (kept when cancelled).
    paid: BTreeSet<TechId>,
    /// Technologies finished by research, oldest first.
    completed: Vec<CompletedTech>,
    /// The technology the player was last warned about for missing gold.
    gold_warning: Option<TechId>,
    /// Progress units produced today.
    today: u64,
    /// Closed days, oldest first (at most [`HISTORY_DAYS`]).
    history: VecDeque<ResearchDay>,
}

/// What the research queue is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResearchStatus {
    /// Nothing is queued.
    Idle,
    /// The technology at the front waits until the treasury can pay its cost.
    WaitingForGold {
        /// The technology.
        tech: TechId,
        /// Gold still missing.
        missing: u32,
    },
    /// The technology at the front is being researched.
    Researching(TechId),
}

/// Contribution of one research building.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchSource {
    /// The building (a school).
    pub building: BuildingId,
    /// Milli-points per hour right now (staff at work × shift factor; before the bonus).
    pub current_milli: u64,
    /// Expected milli-points per day with the assigned staff working its shift (before the
    /// bonus).
    pub daily_milli: u64,
}

/// The research rate (docs/game_design.md §9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchRate {
    /// Milli-points per hour from the headquarters (0 without a finished one).
    pub base_milli: u64,
    /// Milli-points per hour from research buildings right now.
    pub buildings_milli: u64,
    /// Researched `ResearchSpeedBonus` in percent.
    pub bonus_percent: i32,
    /// Total milli-points per hour right now, bonus included.
    pub current_milli: u64,
    /// Expected milli-points per day (headquarters round the clock, research buildings with
    /// their assigned staff), bonus included; the basis of the time estimates.
    pub daily_milli: u64,
    /// The research buildings in id order.
    pub sources: Vec<ResearchSource>,
}

impl Research {
    /// Nothing researched.
    pub fn new() -> Self {
        Self::default()
    }

    /// True if `tech` is researched.
    pub fn is_researched(&self, tech: TechId) -> bool {
        self.researched.contains(&tech)
    }

    /// All researched technologies in id order.
    pub fn researched(&self) -> impl Iterator<Item = TechId> + '_ {
        self.researched.iter().copied()
    }

    /// The research queue; the first entry is being researched (or waits for gold).
    pub fn queue(&self) -> &[TechId] {
        &self.queue
    }

    /// The technology at the front of the queue.
    pub fn active(&self) -> Option<TechId> {
        self.queue.first().copied()
    }

    /// True if `tech` is queued.
    pub fn is_queued(&self, tech: TechId) -> bool {
        self.queue.contains(&tech)
    }

    /// True if the gold cost of `tech` has been paid.
    pub fn is_paid(&self, tech: TechId) -> bool {
        self.paid.contains(&tech)
    }

    /// Research points gathered for `tech` so far, in milli-points.
    pub fn progress_milli(&self, tech: TechId) -> u64 {
        self.progress.get(&tech).copied().unwrap_or(0) * 1000 / PROGRESS_PER_POINT
    }

    /// Technologies finished by research, oldest first.
    pub fn completed(&self) -> &[CompletedTech] {
        &self.completed
    }

    /// Research points produced today so far, in milli-points.
    pub fn today_milli(&self) -> u64 {
        self.today * 1000 / PROGRESS_PER_POINT
    }

    /// Research points of the closed days, oldest first (at most 48 days).
    pub fn history(&self) -> impl DoubleEndedIterator<Item = &ResearchDay> + ExactSizeIterator + '_ {
        self.history.iter()
    }

    /// Marks a technology as researched; returns true if it was new. Removes it from the queue.
    pub(crate) fn mark_researched(&mut self, tech: TechId) -> bool {
        self.queue.retain(|t| *t != tech);
        self.progress.remove(&tech);
        self.paid.remove(&tech);
        if self.gold_warning == Some(tech) {
            self.gold_warning = None;
        }
        self.researched.insert(tech)
    }

    /// True if the building type is available (no unlocking technology, or it is researched).
    pub fn building_unlocked(&self, defs: &Defs, building: BuildingTypeId) -> bool {
        defs.buildings().get(building.index()).is_some_and(|b| b.unlocked_by.is_none_or(|t| self.is_researched(t)))
    }

    /// True if the road type is available.
    pub fn road_unlocked(&self, defs: &Defs, road: RoadTypeId) -> bool {
        defs.roads().get(road.index()).is_some_and(|r| r.unlocked_by.is_none_or(|t| self.is_researched(t)))
    }

    /// True if the unit type can be recruited (no unlocking technology, or it is researched).
    pub fn unit_unlocked(&self, defs: &Defs, unit: crate::defs::UnitTypeId) -> bool {
        defs.units().get(unit.index()).is_some_and(|u| u.unlocked_by.is_none_or(|t| self.is_researched(t)))
    }

    /// Researched [`TechEffect::UnitBonus`] for `unit`: attack and defense increase in percent.
    pub fn unit_bonus_percent(&self, defs: &Defs, unit: crate::defs::UnitTypeId) -> (i32, i32) {
        self.effects(defs).fold((0, 0), |(a, d), e| match e {
            TechEffect::UnitBonus { unit: u, attack_percent, defense_percent } if u.is_none_or(|u| u == unit) => {
                (a + i32::from(*attack_percent), d + i32::from(*defense_percent))
            }
            _ => (a, d),
        })
    }

    /// Sum of all researched [`TechEffect::WallStrengthBonus`] percentages (hit points of walls
    /// and gates).
    pub fn wall_strength_percent(&self, defs: &Defs) -> i32 {
        self.sum_percent(defs, |e| match e {
            TechEffect::WallStrengthBonus { percent } => Some(*percent),
            _ => None,
        })
    }

    /// Researched effects in id order.
    fn effects<'a>(&'a self, defs: &'a Defs) -> impl Iterator<Item = &'a TechEffect> + 'a {
        self.researched.iter().filter_map(|t| defs.techs().get(t.index())).flat_map(|t| t.effects.iter())
    }

    /// Sum of a percentage over the researched effects `pick` selects.
    fn sum_percent(&self, defs: &Defs, pick: impl Fn(&TechEffect) -> Option<i16>) -> i32 {
        self.effects(defs).filter_map(pick).map(i32::from).sum()
    }

    /// Sum of all researched [`TechEffect::ProductionBonus`] percentages that apply to
    /// buildings of type `def`.
    pub fn production_bonus_percent(&self, defs: &Defs, def: &BuildingDef) -> i32 {
        self.sum_percent(defs, |e| match e {
            TechEffect::ProductionBonus { target, percent } if target.matches(def) => Some(*percent),
            _ => None,
        })
    }

    /// Sum of all researched [`TechEffect::FieldYieldBonus`] percentages for `crop`.
    pub fn field_yield_bonus_percent(&self, defs: &Defs, crop: CropId) -> i32 {
        self.sum_percent(defs, |e| match e {
            TechEffect::FieldYieldBonus { crop: c, percent } if c.is_none_or(|c| c == crop) => Some(*percent),
            _ => None,
        })
    }

    /// Researched irrigation: the largest distance to fresh water and the summed bonus
    /// (`None` without [`TechEffect::IrrigationBonus`]).
    pub fn irrigation(&self, defs: &Defs) -> Option<(u8, i32)> {
        self.effects(defs).fold(None, |acc, e| match e {
            TechEffect::IrrigationBonus { max_distance, percent } => {
                let (d, p) = acc.unwrap_or((0, 0));
                Some((d.max(*max_distance), p + i32::from(*percent)))
            }
            _ => acc,
        })
    }

    /// Sum of all researched [`TechEffect::SoilRegeneration`] percentages.
    pub fn soil_regeneration_percent(&self, defs: &Defs) -> i32 {
        self.sum_percent(defs, |e| match e {
            TechEffect::SoilRegeneration { percent } => Some(*percent),
            _ => None,
        })
    }

    /// Sum of all researched [`TechEffect::BuildSpeedBonus`] percentages.
    pub fn build_speed_bonus_percent(&self, defs: &Defs) -> i32 {
        self.sum_percent(defs, |e| match e {
            TechEffect::BuildSpeedBonus { percent } => Some(*percent),
            _ => None,
        })
    }

    /// Sum of all researched [`TechEffect::ResearchSpeedBonus`] percentages.
    pub fn research_speed_bonus_percent(&self, defs: &Defs) -> i32 {
        self.sum_percent(defs, |e| match e {
            TechEffect::ResearchSpeedBonus { percent } => Some(*percent),
            _ => None,
        })
    }

    /// Sum of all researched [`TechEffect::FireRiskReduction`] percentages (0–100).
    pub fn fire_risk_reduction_percent(&self, defs: &Defs) -> i32 {
        self.sum_percent(defs, |e| match e {
            TechEffect::FireRiskReduction { percent } => Some(*percent),
            _ => None,
        })
        .clamp(0, 100)
    }

    /// Sum of all researched [`TechEffect::DiseaseResistance`] percentages (0–100).
    pub fn disease_resistance_percent(&self, defs: &Defs) -> i32 {
        self.sum_percent(defs, |e| match e {
            TechEffect::DiseaseResistance { percent } => Some(*percent),
            _ => None,
        })
        .clamp(0, 100)
    }

    /// Sum of all researched [`TechEffect::ToolWearReduction`] percentages (0–90).
    pub fn tool_wear_reduction_percent(&self, defs: &Defs) -> i32 {
        self.sum_percent(defs, |e| match e {
            TechEffect::ToolWearReduction { percent } => Some(*percent),
            _ => None,
        })
        .clamp(0, 90)
    }

    /// Additional service radius of building type `building` from researched
    /// [`TechEffect::ServiceRadiusBonus`] effects.
    pub fn service_radius_bonus(&self, defs: &Defs, building: BuildingTypeId) -> u16 {
        let Some(def) = defs.buildings().get(building.index()) else { return 0 };
        self.effects(defs)
            .map(|e| match e {
                // A bonus for a type also applies to its upgrades (Großer Markt, Spital, …).
                TechEffect::ServiceRadiusBonus { building: b, tiles } if def.descends_from(*b) => u16::from(*tiles),
                _ => 0,
            })
            .sum()
    }

    /// Radius of `service` for buildings of type `def` including research bonuses (`None` if
    /// the type does not provide it).
    pub fn service_radius(&self, defs: &Defs, def: &BuildingDef, service: ServiceKind) -> Option<u16> {
        def.service_radius(service).map(|r| r.saturating_add(self.service_radius_bonus(defs, def.id)))
    }

    /// Status of the queue's front for the treasury `treasury`.
    pub fn status(&self, defs: &Defs, treasury: i64) -> ResearchStatus {
        let Some(tech) = self.active() else { return ResearchStatus::Idle };
        if self.is_paid(tech) {
            return ResearchStatus::Researching(tech);
        }
        let cost = defs.techs().get(tech.index()).map_or(0, |t| t.cost_gold);
        let missing = i64::from(cost).saturating_sub(treasury.max(0));
        if missing > 0 {
            ResearchStatus::WaitingForGold { tech, missing: u32::try_from(missing).unwrap_or(u32::MAX) }
        } else {
            ResearchStatus::Researching(tech)
        }
    }

    /// Prerequisites of `tech` that are neither researched nor queued before position `before`
    /// of the queue.
    fn missing_prerequisites(&self, def: &TechDef, before: usize) -> Vec<TechId> {
        def.prerequisites
            .iter()
            .copied()
            .filter(|p| !self.is_researched(*p) && !self.queue.iter().take(before).any(|q| q == p))
            .collect()
    }

    /// Checks the stored values against the data (loaded saves).
    pub(crate) fn validate(&self, defs: &Defs) -> Result<(), String> {
        let known = |t: &TechId| t.index() < defs.techs().len();
        if let Some(t) = self.researched.iter().find(|t| !known(t)) {
            return Err(format!("unknown researched technology {t}"));
        }
        let ids = self.queue.iter().chain(self.progress.keys()).chain(self.paid.iter());
        if let Some(t) = ids.clone().find(|t| !known(t)) {
            return Err(format!("unknown technology {t} in the research queue"));
        }
        if self.completed.iter().any(|c| !known(&c.tech) || !self.is_researched(c.tech)) {
            return Err("completed technology not researched".into());
        }
        if self.queue.iter().chain(self.progress.keys()).chain(self.paid.iter()).any(|t| self.is_researched(*t)) {
            return Err("researched technology still queued, paid or in progress".into());
        }
        let unique: BTreeSet<TechId> = self.queue.iter().copied().collect();
        if unique.len() != self.queue.len() {
            return Err("technology queued twice".into());
        }
        // The queue order and the progress against rebalanced requirements are repaired by
        // `rederive` (prerequisites and point costs may change with the data).
        if self.history.len() > HISTORY_DAYS || self.history.iter().zip(self.history.iter().skip(1)).any(|(a, b)| a.day >= b.day) {
            return Err("malformed research history".into());
        }
        Ok(())
    }
}

impl Research {
    /// After loading with possibly rebalanced data: drops queued technologies whose
    /// prerequisites are no longer researched or queued before them (a technology whose new
    /// point cost is already reached completes in its next tick).
    pub(crate) fn rederive(&mut self, defs: &Defs) {
        let mut kept: Vec<TechId> = Vec::with_capacity(self.queue.len());
        for t in std::mem::take(&mut self.queue) {
            let ready = defs.techs().get(t.index()).is_some_and(|d| {
                d.prerequisites.iter().all(|p| self.researched.contains(p) || kept.contains(p))
            });
            if ready {
                kept.push(t);
            }
        }
        self.queue = kept;
    }
}

/// Name of a technology for German texts.
pub fn tech_name(defs: &Defs, tech: TechId) -> String {
    defs.techs().get(tech.index()).map_or_else(|| tech.to_string(), |t| t.name.clone())
}

/// Expected work efficiency of the assigned staff of `building` in permille of full staff
/// (workers' health, happiness and training, [`crate::production::worker_permille`]).
fn assigned_staffing_permille(state: &GameState, defs: &Defs, def: &BuildingDef, building: BuildingId) -> u64 {
    let slots = u64::from(def.workers.max(1));
    let sum: u64 = state
        .population
        .workers_of(building)
        .iter()
        .take(usize::from(def.workers.max(1)))
        .filter_map(|c| state.population.citizen(*c))
        .map(|c| u64::from(crate::production::worker_permille(defs, def, c)))
        .sum();
    sum / slots
}

/// The research rate now (see the module documentation).
pub fn research_rate(state: &GameState, defs: &Defs) -> ResearchRate {
    let rules = defs.rules();
    let hq = storage::main_headquarters(state, defs).is_some();
    let base_milli = if hq { rules.research.base_milli_per_hour() } else { 0 };
    let bankrupt = if state.economy.is_bankrupt() { u64::from(rules.bankruptcy_productivity_percent) } else { 100 };
    let shift = rules.walking.routine.shift_factor_permille();
    let mut sources = Vec::new();
    for (id, b) in state.buildings.iter() {
        let Some(def) = defs.buildings().get(b.kind().index()) else { continue };
        let rph = def.research_milli_per_hour();
        if rph == 0 || !b.is_operational() || b.is_upgrading() {
            continue;
        }
        let present = u64::from(crate::production::staffing_permille(state, defs, def, id));
        let current_milli = rph * present / 1000 * shift / 1000 * bankrupt / 100;
        let daily_milli = rph * assigned_staffing_permille(state, defs, def, id) / 1000 * HOURS_PER_DAY * bankrupt / 100;
        sources.push(ResearchSource { building: id, current_milli, daily_milli });
    }
    let bonus_percent = state.research.research_speed_bonus_percent(defs);
    let factor = u64::try_from((100 + bonus_percent).max(0)).unwrap_or(0);
    let buildings_milli: u64 = sources.iter().map(|s| s.current_milli).sum();
    let buildings_daily: u64 = sources.iter().map(|s| s.daily_milli).sum();
    ResearchRate {
        base_milli,
        buildings_milli,
        bonus_percent,
        current_milli: (base_milli + buildings_milli) * factor / 100,
        daily_milli: (base_milli * HOURS_PER_DAY + buildings_daily) * factor / 100,
        sources,
    }
}

/// Game hours until `remaining_milli` points are researched at `daily_milli` per day
/// (rounded up; `None` without research).
pub fn hours_for(remaining_milli: u64, daily_milli: u64) -> Option<u64> {
    (daily_milli > 0).then(|| (remaining_milli * HOURS_PER_DAY).div_ceil(daily_milli))
}

/// `QueueResearch`: appends `tech` to the queue.
pub(crate) fn queue_research(state: &mut GameState, defs: &Defs, tech: TechId, events: &mut Vec<SimEvent>) -> CommandResult {
    let def = defs.techs().get(tech.index()).ok_or(RejectReason::UnknownTech)?;
    let r = &state.research;
    if r.is_researched(tech) {
        return Err(RejectReason::AlreadyResearched { name: def.name.clone() });
    }
    if r.is_queued(tech) {
        return Err(RejectReason::AlreadyQueued { name: def.name.clone() });
    }
    let max = defs.rules().research.max_queue;
    if r.queue.len() >= usize::from(max) {
        return Err(RejectReason::ResearchQueueFull { max });
    }
    let missing = r.missing_prerequisites(def, r.queue.len());
    if !missing.is_empty() {
        let names: Vec<String> = missing.iter().map(|t| format!("„{}“", tech_name(defs, *t))).collect();
        return Err(RejectReason::MissingPrerequisites { name: def.name.clone(), missing: names.join(", ") });
    }
    state.research.queue.push(tech);
    events.push(SimEvent::ResearchQueueChanged { queue: state.research.queue.clone() });
    Ok(())
}

/// The technologies to queue so that `tech` can be researched: its prerequisites that are
/// neither researched nor queued (depth first, in the data's prerequisite order), then `tech`
/// itself; empty if `tech` is researched, queued or unknown.
pub fn queue_plan(state: &GameState, defs: &Defs, tech: TechId) -> Vec<TechId> {
    fn visit(state: &GameState, defs: &Defs, tech: TechId, plan: &mut Vec<TechId>) {
        let r = &state.research;
        if plan.contains(&tech) || r.is_researched(tech) || r.is_queued(tech) {
            return;
        }
        let Some(def) = defs.techs().get(tech.index()) else { return };
        for p in &def.prerequisites {
            visit(state, defs, *p, plan);
        }
        plan.push(tech);
    }
    let mut plan = Vec::new();
    visit(state, defs, tech, &mut plan);
    plan
}

/// `QueueResearchWithPrerequisites`: queues [`queue_plan`] as a whole or rejects it.
pub(crate) fn queue_with_prerequisites(state: &mut GameState, defs: &Defs, tech: TechId, events: &mut Vec<SimEvent>) -> CommandResult {
    let def = defs.techs().get(tech.index()).ok_or(RejectReason::UnknownTech)?;
    if state.research.is_researched(tech) {
        return Err(RejectReason::AlreadyResearched { name: def.name.clone() });
    }
    if state.research.is_queued(tech) {
        return Err(RejectReason::AlreadyQueued { name: def.name.clone() });
    }
    let plan = queue_plan(state, defs, tech);
    let max = defs.rules().research.max_queue;
    if state.research.queue.len() + plan.len() > usize::from(max) {
        return Err(RejectReason::ResearchQueueFull { max });
    }
    for t in plan {
        state.research.queue.push(t);
    }
    events.push(SimEvent::ResearchQueueChanged { queue: state.research.queue.clone() });
    Ok(())
}

/// `CancelResearch`: removes `tech` and every queued technology that depends on it (progress
/// and payments are kept).
pub(crate) fn cancel_research(state: &mut GameState, defs: &Defs, tech: TechId, events: &mut Vec<SimEvent>) -> CommandResult {
    if !state.research.is_queued(tech) {
        return Err(RejectReason::NotQueued);
    }
    let mut removed = vec![tech];
    // Dependants come after their prerequisites, so one pass in queue order finds them all.
    for t in state.research.queue.clone() {
        if defs.techs().get(t.index()).is_some_and(|d| d.prerequisites.iter().any(|p| removed.contains(p))) && !removed.contains(&t) {
            removed.push(t);
        }
    }
    state.research.queue.retain(|t| !removed.contains(t));
    for t in removed {
        events.push(SimEvent::ResearchCancelled { tech: t });
    }
    events.push(SimEvent::ResearchQueueChanged { queue: state.research.queue.clone() });
    Ok(())
}

/// `MoveResearch`: moves `tech` to `position` (clamped to the queue); every technology must
/// stay behind its prerequisites.
pub(crate) fn move_research(
    state: &mut GameState,
    defs: &Defs,
    tech: TechId,
    position: u8,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let r = &state.research;
    let from = r.queue.iter().position(|t| *t == tech).ok_or(RejectReason::NotQueued)?;
    let to = usize::from(position).min(r.queue.len() - 1);
    if from == to {
        return Ok(());
    }
    let mut queue = r.queue.clone();
    let t = queue.remove(from);
    queue.insert(to, t);
    let candidate = Research { queue, ..r.clone() };
    for (i, t) in candidate.queue.iter().enumerate() {
        let def = defs.tech(*t);
        if let Some(p) = candidate.missing_prerequisites(def, i).first() {
            return Err(RejectReason::InvalidResearchOrder { name: def.name.clone(), prerequisite: tech_name(defs, *p) });
        }
    }
    state.research.queue = candidate.queue;
    events.push(SimEvent::ResearchQueueChanged { queue: state.research.queue.clone() });
    Ok(())
}

/// Re-applies what researched effects change in existing buildings and allocations: storage
/// capacities, territory, the stock view, jobs and housing.
fn apply_effects(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let bonus = storage::storage_bonus_percent(&state.research, defs);
    for (_, b) in state.buildings.iter_mut() {
        if b.is_complete() {
            storage::init_storage(b, defs, bonus);
        }
    }
    if state.territory.recompute(&state.buildings, defs, &state.diplomacy.land) {
        events.push(SimEvent::TerritoryChanged { owned_tiles: state.territory.owned_tiles() });
    }
    storage::refresh_stock(state, defs);
    state.workforce.mark_dirty();
}

/// `DebugUnlockAllResearch` (developer command): researches every technology and re-applies
/// what research changes in existing buildings (storage capacities, territory).
pub(crate) fn unlock_all(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let mut new = 0u32;
    for tech in defs.techs() {
        if state.research.mark_researched(tech.id) {
            new += 1;
        }
    }
    if new == 0 {
        return;
    }
    apply_effects(state, defs, events);
    events.push(SimEvent::DebugResearchUnlocked { techs: new });
}

/// Short German summary of what a technology brings, for the completion notification:
/// the unlocked buildings, roads and units, otherwise its first effect.
fn completion_summary(defs: &Defs, def: &TechDef) -> String {
    let unlocks: Vec<String> = def
        .effects
        .iter()
        .filter_map(|e| match e {
            TechEffect::UnlockBuilding(b) => defs.buildings().get(b.index()).map(|d| d.name.clone()),
            TechEffect::UnlockRoad(r) => defs.roads().get(r.index()).map(|d| d.name.clone()),
            TechEffect::UnlockUnit(u) => defs.units().get(u.index()).map(|d| d.name.clone()),
            _ => None,
        })
        .collect();
    if !unlocks.is_empty() {
        return format!("Neu verfügbar: {}.", unlocks.join(", "));
    }
    def.effects.first().map(|e| format!("{}.", effect_text(defs, e))).unwrap_or_default()
}

/// German description of a technology effect, e.g. „+15 % Ertrag auf allen Feldern“.
pub fn effect_text(defs: &Defs, effect: &TechEffect) -> String {
    let building = |b: &BuildingTypeId| defs.buildings().get(b.index()).map_or_else(|| b.to_string(), |d| d.name.clone());
    match effect {
        TechEffect::UnlockBuilding(b) => format!("Gebäude: {}", building(b)),
        TechEffect::UnlockRoad(r) => {
            format!("Straßentyp: {}", defs.roads().get(r.index()).map_or_else(|| r.to_string(), |d| d.name.clone()))
        }
        TechEffect::UnlockUnit(u) => {
            format!("Einheit: {}", defs.units().get(u.index()).map_or_else(|| u.to_string(), |d| d.name.clone()))
        }
        TechEffect::ProductionBonus { target, percent } => match target {
            crate::defs::BonusTarget::All => format!("{percent:+} % Tempo in allen Betrieben"),
            crate::defs::BonusTarget::Category(c) => format!("{percent:+} % Tempo: {}", c.display_name()),
            crate::defs::BonusTarget::Building(b) => format!("{percent:+} % Tempo: {}", building(b)),
        },
        TechEffect::FieldYieldBonus { crop: None, percent } => format!("{percent:+} % Ertrag auf allen Feldern"),
        TechEffect::FieldYieldBonus { crop: Some(c), percent } => format!(
            "{percent:+} % Ertrag: {}",
            defs.crops().get(c.index()).map_or_else(|| c.to_string(), |d| d.name.clone())
        ),
        TechEffect::IrrigationBonus { max_distance, percent } => {
            format!("{percent:+} % Feldertrag bis {max_distance} Felder vom Süßwasser")
        }
        TechEffect::SoilRegeneration { percent } => format!("Bodenfruchtbarkeit erholt sich {percent} % schneller"),
        TechEffect::StorageBonus { percent } => format!("{percent:+} % Lagerkapazität"),
        TechEffect::ServiceRadiusBonus { building: b, tiles } => format!("{}: Reichweite +{tiles} Felder", building(b)),
        TechEffect::HousingCapacityBonus { percent } => format!("{percent:+} % Platz in Wohnhäusern"),
        TechEffect::TaxBonus { percent } => format!("{percent:+} % Steuereinnahmen"),
        TechEffect::TradePriceBonus { percent } => format!("Handelspreise {percent} Prozentpunkte besser"),
        TechEffect::UpkeepReduction { percent } => format!("−{percent} % Gebäudeunterhalt"),
        TechEffect::BuildSpeedBonus { percent } => format!("{percent:+} % Baugeschwindigkeit"),
        TechEffect::ResearchSpeedBonus { percent } => format!("{percent:+} % Forschungsgeschwindigkeit"),
        TechEffect::FireRiskReduction { percent } => format!("−{percent} % Brandgefahr"),
        TechEffect::DiseaseResistance { percent } => format!("−{percent} % Seuchen- und Krankheitsrisiko"),
        TechEffect::ToolWearReduction { percent } => format!("−{percent} % Werkzeugverschleiß"),
        TechEffect::UnitBonus { unit, attack_percent, defense_percent } => {
            let who = unit.map_or_else(
                || "Alle Einheiten".to_owned(),
                |u| defs.units().get(u.index()).map_or_else(|| u.to_string(), |d| d.name.clone()),
            );
            let mut parts = Vec::new();
            if *attack_percent != 0 {
                parts.push(format!("{attack_percent:+} % Angriff"));
            }
            if *defense_percent != 0 {
                parts.push(format!("{defense_percent:+} % Verteidigung"));
            }
            format!("{who}: {}", parts.join(", "))
        }
        TechEffect::WallStrengthBonus { percent } => format!("Mauern und Tore {percent:+} % stärker"),
    }
}

/// Pipeline step 9 (research part): closes the day's statistics at midnight, starts the
/// technology at the front of the queue (pays its gold), adds this tick's research points and
/// completes the technology when it has all its points.
pub(crate) fn run_research(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let clock = state.clock;
    if clock.is_new_day() && clock.tick > 0 {
        let day = clock.total_days().saturating_sub(1);
        let points_milli = state.research.today * 1000 / PROGRESS_PER_POINT;
        let history = &mut state.research.history;
        history.push_back(ResearchDay { day, points_milli });
        while history.len() > HISTORY_DAYS {
            history.pop_front();
        }
        state.research.today = 0;
    }
    let rate = research_rate(state, defs).current_milli;
    state.research.today = state.research.today.saturating_add(rate);
    let Some(tech) = state.research.active() else { return };
    let Some(def) = defs.techs().get(tech.index()) else { return };
    if !state.research.is_paid(tech) && !start(state, def, events) {
        return;
    }
    let required = u64::from(def.research_points) * PROGRESS_PER_POINT;
    let progress = state.research.progress.entry(tech).or_insert(0);
    *progress = progress.saturating_add(rate);
    if *progress >= required {
        complete(state, defs, tech, events);
    }
}

/// Pays the cost of the queue's front and starts it; returns false (with one warning per
/// technology) if the treasury cannot pay.
fn start(state: &mut GameState, def: &TechDef, events: &mut Vec<SimEvent>) -> bool {
    if state.economy.pay_gold(LedgerCategory::Research, def.cost_gold).is_err() {
        if state.research.gold_warning != Some(def.id) {
            state.research.gold_warning = Some(def.id);
            let missing = i64::from(def.cost_gold) - state.economy.treasury().max(0);
            let verb = if missing == 1 { "fehlt" } else { "fehlen" };
            let text = format!("Für die Forschung „{}“ {verb} noch {missing} Gold.", def.name);
            events.push(SimEvent::Notification(Notification::new(Severity::Warning, text)));
        }
        return false;
    }
    state.research.paid.insert(def.id);
    state.research.gold_warning = None;
    events.push(SimEvent::ResearchStarted { tech: def.id, gold: def.cost_gold });
    let text = if def.cost_gold > 0 {
        format!("Forschung begonnen: {} ({} Gold).", def.name, def.cost_gold)
    } else {
        format!("Forschung begonnen: {}.", def.name)
    };
    events.push(SimEvent::Notification(Notification::new(Severity::Info, text)));
    true
}

/// Marks `tech` researched and applies its effects.
fn complete(state: &mut GameState, defs: &Defs, tech: TechId, events: &mut Vec<SimEvent>) {
    let def = defs.tech(tech);
    state.research.mark_researched(tech);
    state.research.completed.push(CompletedTech { tech, tick: state.clock.tick });
    apply_effects(state, defs, events);
    events.push(SimEvent::ResearchCompleted { tech });
    let next = match state.research.active() {
        Some(t) => format!(" Als Nächstes: {}.", tech_name(defs, t)),
        None => " Die Forschungsliste ist leer.".to_owned(),
    };
    let text = format!("Forschung abgeschlossen: {}. {}{next}", def.name, completion_summary(defs, def));
    events.push(SimEvent::Notification(Notification::new(Severity::Good, text)));
}

#[cfg(test)]
mod tests;
