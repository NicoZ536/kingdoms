//! Random events with choices (docs/game_design.md §12): drawing, the pending decision,
//! consequences and temporary modifiers. Pipeline step 9 (after research).
//!
//! ## Draw
//! At `events.check_hour` of every day from `events.first_day` on, if events are enabled
//! ([`GameSettings::events_enabled`](crate::state::GameSettings)), nothing is pending and the
//! last event lies at least `events.min_days_between` days back, an event occurs with
//! `events.daily_chance_percent` × the difficulty factor. The candidates are the events whose
//! conditions hold and whose cooldown has passed; they compete by weight, scaled by their risk
//! (fire: the share of wooden buildings without fire protection and researched
//! `FireRiskReduction`; disease: researched `DiseaseResistance`). All draws use a dedicated
//! [`ChaCha8Rng`] stored in the state, so events never shift the other random rolls.
//!
//! ## Decision
//! The drawn event waits for [`Command::ResolveEvent`](crate::command::Command::ResolveEvent)
//! with one of its choices (whose requirements must hold) for `events.decision_hours`; then
//! the first choice applies. Events that damage buildings pick the first building (the
//! "focus", e.g. the burning house) when they occur, so the warning can point at it.
//!
//! ## Consequences
//! Gold and goods (booked in the ledger as `Ereignisse`), temporary modifiers of happiness,
//! production speed and field yields (shown in the happiness and productivity breakdowns),
//! forced weather, refugees who walk in, illness, damaged buildings that become repair sites
//! ([`crate::buildings`]: fire spreads from the focus to unprotected wooden buildings within
//! `events.fire_spread_radius`), destroyed crops, stolen goods from storages without safety
//! coverage (the hook for military defence: [`raid_protected`]), new deposits and a cap on
//! the tax rate.

pub(crate) mod conditions;
pub(crate) mod effects;
pub mod festival;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, VecDeque};

use glam::IVec2;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::command::{CommandResult, RejectReason};
use crate::defs::{Defs, EventChoice, EventDef, EventDefId, EventRisk};
use crate::economy::storage;
use crate::event::{Notification, NotificationKind, SimEvent};
use crate::state::GameState;
use crate::time::{GameClock, TICKS_PER_DAY};

pub use conditions::{condition_holds, raid_protected, requirement_problem, wooden_buildings};
pub use effects::predicted_effects;

/// Stream constant mixed into the seed for the events RNG.
const EVENTS_RNG_STREAM: u64 = 0x4556_454e_5453_2121; // "EVENTS!!"

/// What a temporary modifier changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModifierKind {
    /// Happiness of every citizen, in points.
    Happiness,
    /// Work speed of production and farm work, in percent.
    Production,
    /// Field yields, in percent.
    FieldYield,
    /// Festival days (amount unused): citizens celebrate in the afternoon.
    Festival,
}

impl ModifierKind {
    /// German label.
    pub const fn display_name(self) -> &'static str {
        match self {
            ModifierKind::Happiness => "Zufriedenheit",
            ModifierKind::Production => "Produktion",
            ModifierKind::FieldYield => "Feldertrag",
            ModifierKind::Festival => "Festtag",
        }
    }
}

/// Where a temporary modifier comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModifierSource {
    /// A random event and the choice taken (§12).
    Event(EventDefId),
    /// A festival the player held ([`Command::HoldFestival`](crate::command::Command), §7).
    Festival,
}

/// A temporary modifier from an event or a festival.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveModifier {
    /// What it changes.
    pub kind: ModifierKind,
    /// Points (happiness) or percent (production, field yield).
    pub amount: i16,
    /// What it came from.
    pub source: ModifierSource,
    /// First tick without the modifier.
    pub until_tick: u64,
}

/// A cap on the tax rate promised in an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxCap {
    /// Highest allowed tax rate in percent.
    pub max_percent: u8,
    /// First tick without the cap.
    pub until_tick: u64,
    /// The event it came from.
    pub source: EventDefId,
}

/// An event waiting for the player's decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingEvent {
    /// Serial number (for `Command::ResolveEvent`).
    pub serial: u32,
    /// The event.
    pub event: EventDefId,
    /// Tick it occurred.
    pub started_tick: u64,
    /// Tick at which the first choice applies.
    pub expires_tick: u64,
    /// The building the event starts at (e.g. the burning house), if any.
    pub focus: Option<BuildingId>,
    /// Map tile the event refers to.
    pub position: Option<IVec2>,
}

/// A decided event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    /// Serial number.
    pub serial: u32,
    /// The event.
    pub event: EventDefId,
    /// Tick it occurred.
    pub started_tick: u64,
    /// Tick it was decided.
    pub resolved_tick: u64,
    /// Index of the choice taken.
    pub choice: u8,
    /// True if the time ran out and the first choice applied.
    pub by_default: bool,
    /// German lines describing what actually happened.
    pub outcome: Vec<String>,
}

/// Event state: the pending decision, cooldowns, modifiers, the history and the RNG.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Events {
    pending: Option<PendingEvent>,
    /// Day (0-based) each event last occurred.
    last_occurred: BTreeMap<EventDefId, u64>,
    /// Day the last event occurred.
    last_event_day: Option<u64>,
    modifiers: Vec<ActiveModifier>,
    tax_cap: Option<TaxCap>,
    /// Day (0-based) the player last held a festival.
    last_festival_day: Option<u64>,
    /// Decided events, oldest first.
    history: VecDeque<EventRecord>,
    next_serial: u32,
    rng: ChaCha8Rng,
}

impl Events {
    /// No events yet; the RNG is derived from the game seed.
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            pending: None,
            last_occurred: BTreeMap::new(),
            last_event_day: None,
            modifiers: Vec::new(),
            tax_cap: None,
            last_festival_day: None,
            history: VecDeque::new(),
            next_serial: 1,
            rng: ChaCha8Rng::seed_from_u64(seed ^ EVENTS_RNG_STREAM),
        }
    }

    /// The event waiting for a decision.
    pub fn pending(&self) -> Option<&PendingEvent> {
        self.pending.as_ref()
    }

    /// Decided events, oldest first.
    pub fn history(&self) -> impl DoubleEndedIterator<Item = &EventRecord> + ExactSizeIterator + '_ {
        self.history.iter()
    }

    /// Modifiers in effect at `tick`, in the order they were applied.
    pub fn modifiers(&self, tick: u64) -> impl Iterator<Item = &ActiveModifier> + '_ {
        self.modifiers.iter().filter(move |m| m.until_tick > tick)
    }

    /// Sum of the modifiers of `kind` in effect at `tick`.
    pub fn modifier_sum(&self, kind: ModifierKind, tick: u64) -> i32 {
        self.modifiers(tick).filter(|m| m.kind == kind).map(|m| i32::from(m.amount)).sum()
    }

    /// Happiness points all citizens gain (or lose) from events at `tick`.
    pub fn happiness_modifier(&self, tick: u64) -> i32 {
        self.modifier_sum(ModifierKind::Happiness, tick)
    }

    /// Adds a modifier (tests of the systems reading it).
    #[cfg(test)]
    pub(crate) fn push_modifier(&mut self, m: ActiveModifier) {
        self.modifiers.push(m);
    }

    /// Removes every modifier (tests).
    #[cfg(test)]
    pub(crate) fn clear_modifiers(&mut self) {
        self.modifiers.clear();
    }

    /// True on festival days (afternoons at the church or market instead of work).
    pub fn festival_active(&self, tick: u64) -> bool {
        self.modifiers(tick).any(|m| m.kind == ModifierKind::Festival)
    }

    /// Production and farm work speed change from events at `tick`, in percent.
    pub fn production_modifier_percent(&self, tick: u64) -> i32 {
        self.modifier_sum(ModifierKind::Production, tick)
    }

    /// Field yield change from events at `tick`, in percent.
    pub fn field_yield_modifier_percent(&self, tick: u64) -> i32 {
        self.modifier_sum(ModifierKind::FieldYield, tick)
    }

    /// The tax cap in effect at `tick`.
    pub fn tax_cap(&self, tick: u64) -> Option<TaxCap> {
        self.tax_cap.filter(|c| c.until_tick > tick)
    }

    /// Day (0-based) `event` last occurred.
    pub fn last_occurred(&self, event: EventDefId) -> Option<u64> {
        self.last_occurred.get(&event).copied()
    }

    /// Day the last event occurred.
    pub fn last_event_day(&self) -> Option<u64> {
        self.last_event_day
    }

    /// Day (0-based) the player last held a festival.
    pub fn last_festival_day(&self) -> Option<u64> {
        self.last_festival_day
    }

    /// First day `def` may occur again (`None`: no cooldown running on `today`).
    pub fn cooldown_until(&self, def: &EventDef, today: u64) -> Option<u64> {
        let until = self.last_occurred(def.id)?.saturating_add(u64::from(def.cooldown_days));
        (until > today).then_some(until)
    }

    /// Checks references and value ranges against the game clock (loaded saves): nothing
    /// happened in the future, and no modifier or tax cap lasts longer than an event or a
    /// festival can make it (`u16` days).
    pub(crate) fn validate(&self, defs: &Defs, clock: GameClock) -> Result<(), String> {
        let known = |e: EventDefId| e.index() < defs.events().len();
        let (tick, today) = (clock.tick, clock.total_days());
        if let Some(p) = &self.pending {
            if !known(p.event) || p.expires_tick < p.started_tick || p.started_tick > tick || p.serial >= self.next_serial {
                return Err("malformed pending event".into());
            }
        }
        if self.last_occurred.values().any(|d| *d > today) || self.last_event_day.is_some_and(|d| d > today) {
            return Err("event day in the future".into());
        }
        if self.last_festival_day.is_some_and(|d| d > today) {
            return Err("festival day in the future".into());
        }
        let longest = tick.saturating_add(u64::from(u16::MAX) * TICKS_PER_DAY);
        if self.modifiers.iter().any(|m| m.until_tick > longest) || self.tax_cap.is_some_and(|c| c.until_tick > longest) {
            return Err("modifier or tax cap lasts too long".into());
        }
        let known_source = |s: ModifierSource| match s {
            ModifierSource::Event(e) => known(e),
            ModifierSource::Festival => true,
        };
        if self.last_occurred.keys().any(|e| !known(*e)) || self.modifiers.iter().any(|m| !known_source(m.source)) {
            return Err("unknown event id".into());
        }
        if self.tax_cap.is_some_and(|c| !known(c.source)) {
            return Err("malformed tax cap".into());
        }
        if self
            .history
            .iter()
            .any(|r| !known(r.event) || r.serial >= self.next_serial || r.started_tick > r.resolved_tick || r.resolved_tick > tick)
        {
            return Err("malformed event history".into());
        }
        Ok(())
    }

    /// After loading with possibly rebalanced data: trims the history to
    /// `events.history_length`.
    pub(crate) fn rederive(&mut self, defs: &Defs) {
        while self.history.len() > usize::from(defs.rules().events.history_length) {
            self.history.pop_front();
        }
    }
}

/// Production speed of all production and farm work from events in percent of normal (at
/// least 0).
pub fn production_percent(state: &GameState) -> u64 {
    u64::try_from((100 + state.events.production_modifier_percent(state.clock.tick)).max(0)).unwrap_or(0)
}

/// Weight of `def` in the draw now: its data weight scaled by its risk (permille-exact).
pub fn effective_weight(state: &GameState, defs: &Defs, def: &EventDef) -> u64 {
    let base = u64::from(def.weight) * 1000;
    match def.risk {
        None => base,
        Some(EventRisk::Fire) => {
            let (wooden, unprotected) = wooden_buildings(state, defs);
            let reduction = u64::try_from(100 - state.research.fire_risk_reduction_percent(defs)).unwrap_or(0);
            (base * unprotected).checked_div(wooden).unwrap_or(0) * reduction / 100
        }
        Some(EventRisk::Disease) => {
            let resistance = u64::try_from(100 - state.research.disease_resistance_percent(defs)).unwrap_or(0);
            base * resistance / 100
        }
    }
}

/// The text of `def` for the player: with a focus building that still stands its
/// `focus_text` naming it („Im Gebäude „Holzfällerhütte“ ist Feuer ausgebrochen!“), otherwise
/// the general `text`.
pub fn event_text(state: &GameState, defs: &Defs, def: &EventDef, focus: Option<BuildingId>) -> String {
    match (&def.focus_text, focus.filter(|b| state.buildings.get(*b).is_some())) {
        (Some(text), Some(b)) => text.replace("{building}", &effects::building_name(state, defs, b)),
        _ => def.text.clone(),
    }
}

/// True if `def` may occur now: conditions hold and its cooldown has passed.
pub fn is_eligible(state: &GameState, defs: &Defs, def: &EventDef) -> bool {
    let today = state.clock.total_days();
    state.events.cooldown_until(def, today).is_none() && def.conditions.iter().all(|c| condition_holds(state, defs, c))
}

/// Pipeline step 9 (events part): ends expired modifiers, applies the first choice of an
/// event whose time ran out and, once a day, draws a new event.
pub(crate) fn run_events(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    state.events.modifiers.retain(|m| m.until_tick > tick);
    if state.events.tax_cap.is_some_and(|c| c.until_tick <= tick) {
        state.events.tax_cap = None;
    }
    if let Some(p) = state.events.pending
        && tick >= p.expires_tick
    {
        apply_choice(state, defs, p, 0, true, events);
    }
    let clock = state.clock;
    let rules = &defs.rules().events;
    if !state.settings.events_enabled
        || !clock.is_new_hour()
        || clock.hour() != u64::from(rules.check_hour)
        || state.events.pending.is_some()
    {
        return;
    }
    let today = clock.total_days();
    if today < u64::from(rules.first_day)
        || state.events.last_event_day.is_some_and(|d| today < d.saturating_add(u64::from(rules.min_days_between)))
    {
        return;
    }
    let chance = u64::from(rules.daily_chance_percent) * u64::from(rules.difficulty_chance_percent.get(state.settings.difficulty));
    if state.events.rng.random_range(0..10_000u64) >= chance {
        return;
    }
    let candidates: Vec<(EventDefId, u64)> = defs
        .events()
        .iter()
        .filter(|d| is_eligible(state, defs, d))
        .map(|d| (d.id, effective_weight(state, defs, d)))
        .filter(|(_, w)| *w > 0)
        .collect();
    let total: u64 = candidates.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return;
    }
    let mut roll = state.events.rng.random_range(0..total);
    for (id, weight) in candidates {
        if roll < weight {
            start(state, defs, id, events);
            return;
        }
        roll -= weight;
    }
}

/// `DebugTriggerEvent` (developer command): starts `event` now, ignoring its conditions,
/// cooldown and the daily draw.
pub(crate) fn trigger(state: &mut GameState, defs: &Defs, event: EventDefId, events: &mut Vec<SimEvent>) -> CommandResult {
    if event.index() >= defs.events().len() {
        return Err(RejectReason::UnknownEvent);
    }
    if state.events.pending.is_some() {
        return Err(RejectReason::EventPending);
    }
    start(state, defs, event, events);
    Ok(())
}

/// Starts `event`: picks the focus building, records the cooldown and announces it.
fn start(state: &mut GameState, defs: &Defs, event: EventDefId, events: &mut Vec<SimEvent>) {
    let def = defs.event(event);
    let tick = state.clock.tick;
    let focus = effects::pick_focus(state, defs, def);
    let position = focus.and_then(|b| state.buildings.get(b)).map(|b| b.entrance());
    let serial = state.events.next_serial;
    state.events.next_serial = serial.wrapping_add(1).max(1);
    let expires_tick = tick + defs.rules().events.decision_ticks();
    state.events.pending = Some(PendingEvent { serial, event, started_tick: tick, expires_tick, focus, position });
    let today = state.clock.total_days();
    state.events.last_occurred.insert(event, today);
    state.events.last_event_day = Some(today);
    events.push(SimEvent::EventStarted { serial, event, position, expires_tick });
    let text = format!(
        "{}: {} Entscheidung bis {}.",
        def.title,
        event_text(state, defs, def, focus),
        crate::report::deadline_clock_text(tick, expires_tick)
    );
    let mut note = Notification::new(def.severity, text).with_kind(NotificationKind::Event);
    if let Some(p) = position {
        note = note.at(p);
    }
    if let Some(b) = focus {
        note = note.for_building(b);
    }
    events.push(SimEvent::Notification(note));
}

/// `ResolveEvent`: decides the pending event `serial` with `choice`.
pub(crate) fn resolve(state: &mut GameState, defs: &Defs, serial: u32, choice: u8, events: &mut Vec<SimEvent>) -> CommandResult {
    let pending = state.events.pending.filter(|p| p.serial == serial).ok_or(RejectReason::NoPendingEvent)?;
    let def = defs.event(pending.event);
    let option = def.choices.get(usize::from(choice)).ok_or(RejectReason::UnknownChoice)?;
    if let Some(reason) = choice_problem(state, defs, option) {
        return Err(RejectReason::ChoiceUnavailable { reason });
    }
    apply_choice(state, defs, pending, choice, false, events);
    Ok(())
}

/// Why `choice` cannot be taken now (German), or `None` if it can.
pub fn choice_problem(state: &GameState, defs: &Defs, choice: &EventChoice) -> Option<String> {
    choice.requires.iter().find_map(|c| requirement_problem(state, defs, c))
}

/// Applies `choice` of the pending event, records it and reports the outcome.
fn apply_choice(state: &mut GameState, defs: &Defs, pending: PendingEvent, choice: u8, by_default: bool, events: &mut Vec<SimEvent>) {
    state.events.pending = None;
    let def = defs.event(pending.event);
    let mut outcome = Vec::new();
    if let Some(option) = def.choices.get(usize::from(choice)) {
        let mut ctx = effects::EffectCtx {
            event: pending.event,
            focus: pending.focus,
            position: pending.position,
            started_tick: pending.started_tick,
            raid_arrival: None,
        };
        for effect in &option.effects {
            effects::apply(state, defs, effect, &mut ctx, events, &mut outcome);
        }
    }
    storage::refresh_stock(state, defs);
    let label = def.choices.get(usize::from(choice)).map_or("", |c| c.label.as_str());
    let record = EventRecord {
        serial: pending.serial,
        event: pending.event,
        started_tick: pending.started_tick,
        resolved_tick: state.clock.tick,
        choice,
        by_default,
        outcome: outcome.clone(),
    };
    let history = &mut state.events.history;
    history.push_back(record);
    while history.len() > usize::from(defs.rules().events.history_length) {
        history.pop_front();
    }
    events.push(SimEvent::EventResolved { serial: pending.serial, event: pending.event, choice, by_default });
    let how = if by_default { "Ohne Entscheidung" } else { "Entschieden" };
    let details = if outcome.is_empty() { String::new() } else { format!(" {}", outcome.join(" ")) };
    let text = format!("{} – {how}: {label}.{details}", def.title);
    let mut note = Notification::new(def.severity, text).with_kind(NotificationKind::Event);
    if let Some(p) = pending.position {
        note = note.at(p);
    }
    if let Some(b) = pending.focus.filter(|b| state.buildings.get(*b).is_some()) {
        note = note.for_building(b);
    }
    events.push(SimEvent::Notification(note));
}
