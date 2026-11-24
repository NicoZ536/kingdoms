//! Events window: the pending decision with its choices and predicted effects, active
//! modifiers and the history (docs/game_design.md §12).

use glam::IVec2;
use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::defs::{Defs, EventDefId};
use crate::event::Severity;
use crate::events::{self, ModifierKind, ModifierSource};
use crate::state::GameState;
use crate::time::GameClock;

/// A choice of the pending event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventChoiceRow {
    /// Index for `Command::ResolveEvent`.
    pub index: u8,
    /// German button label.
    pub label: String,
    /// True for the choice that applies when the time runs out.
    pub is_default: bool,
    /// True if its requirements hold now.
    pub available: bool,
    /// Why it is not available (German).
    pub problem: Option<String>,
    /// German lines describing its effects (estimates from the current state).
    pub effects: Vec<String>,
}

/// The event waiting for a decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingEventReport {
    /// Serial number for `Command::ResolveEvent`.
    pub serial: u32,
    /// The event.
    pub event: EventDefId,
    /// Canonical id.
    pub key: String,
    /// German title.
    pub title: String,
    /// German text.
    pub text: String,
    /// Importance.
    pub severity: Severity,
    /// Tick it occurred.
    pub started_tick: u64,
    /// Tick at which the default choice applies.
    pub expires_tick: u64,
    /// Game minutes left to decide.
    pub minutes_left: u64,
    /// German deadline with the default choice, e.g. „Entscheidung bis morgen 06:00 Uhr,
    /// sonst gilt „Kein Interesse“.“ (the time left is [`Self::minutes_left`]; the clock time
    /// follows [`crate::report::deadline_clock_text`]).
    pub deadline_text: String,
    /// The building the event starts at.
    pub focus: Option<BuildingId>,
    /// Map tile the event refers to.
    pub position: Option<IVec2>,
    /// The choices in data order (the first one is the default).
    pub choices: Vec<EventChoiceRow>,
}

/// A temporary modifier in effect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierRow {
    /// What it changes.
    pub kind: ModifierKind,
    /// Points (happiness) or percent.
    pub amount: i16,
    /// German line, e.g. „Zufriedenheit +12 (Religiöses Fest), noch 3 Tage“.
    pub text: String,
    /// Title of the event it came from.
    pub source: String,
    /// First tick without the modifier.
    pub until_tick: u64,
}

/// A decided event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventHistoryRow {
    /// Serial number.
    pub serial: u32,
    /// The event.
    pub event: EventDefId,
    /// German title.
    pub title: String,
    /// Importance.
    pub severity: Severity,
    /// German date and time it occurred.
    pub date: String,
    /// German label of the choice taken.
    pub choice: String,
    /// True if the time ran out and the first choice applied.
    pub by_default: bool,
    /// German lines describing what happened.
    pub outcome: Vec<String>,
}

/// The events window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsReport {
    /// True if events are drawn in this game.
    pub enabled: bool,
    /// The event waiting for a decision.
    pub pending: Option<PendingEventReport>,
    /// Modifiers in effect.
    pub modifiers: Vec<ModifierRow>,
    /// Sum of the happiness modifiers in points.
    pub happiness_points: i32,
    /// Sum of the production modifiers in percent.
    pub production_percent: i32,
    /// Sum of the field yield modifiers in percent.
    pub field_yield_percent: i32,
    /// Tax cap in effect: highest rate and German deadline.
    pub tax_cap: Option<(u8, String)>,
    /// Decided events, newest first.
    pub history: Vec<EventHistoryRow>,
}

/// Time left until `until_tick` in the one duration form of the game
/// ([`crate::report::ticks_text`]): „noch 45 min“, „noch 7 h 30 min“, „noch 2 Tage 6 h“.
fn remaining_text(state: &GameState, until_tick: u64) -> String {
    format!("noch {}", crate::report::ticks_text(until_tick.saturating_sub(state.clock().tick)))
}

/// The event waiting for a decision, with its choices (`None` if nothing is pending).
pub fn pending_event(state: &GameState, defs: &Defs) -> Option<PendingEventReport> {
    let p = state.events().pending()?;
    let def = defs.events().get(p.event.index())?;
    let choices = def
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let problem = events::choice_problem(state, defs, c);
            EventChoiceRow {
                index: u8::try_from(i).unwrap_or(u8::MAX),
                label: c.label.clone(),
                is_default: i == 0,
                available: problem.is_none(),
                problem,
                effects: events::predicted_effects(state, defs, def, i, p.focus),
            }
        })
        .collect();
    let left = p.expires_tick.saturating_sub(state.clock().tick);
    let deadline_text = format!(
        "Entscheidung bis {}, sonst gilt „{}“.",
        crate::report::deadline_clock_text(state.clock().tick, p.expires_tick),
        def.choices.first().map_or("", |c| c.label.as_str())
    );
    Some(PendingEventReport {
        serial: p.serial,
        event: p.event,
        key: def.key.clone(),
        title: def.title.clone(),
        text: events::event_text(state, defs, def, p.focus),
        severity: def.severity,
        started_tick: p.started_tick,
        expires_tick: p.expires_tick,
        minutes_left: left * crate::time::SECONDS_PER_TICK / 60,
        deadline_text,
        focus: p.focus,
        position: p.position,
        choices,
    })
}

/// The events window (see [`EventsReport`]).
pub fn events_report(state: &GameState, defs: &Defs) -> EventsReport {
    let tick = state.clock().tick;
    let e = state.events();
    let title = |source: ModifierSource| match source {
        ModifierSource::Event(id) => defs.events().get(id.index()).map_or_else(|| id.to_string(), |d| d.title.clone()),
        ModifierSource::Festival => crate::events::festival::FESTIVAL_TITLE.to_owned(),
    };
    let modifiers = e
        .modifiers(tick)
        .map(|m| {
            let value = match m.kind {
                ModifierKind::Happiness => format!(" {:+}", m.amount),
                ModifierKind::Production | ModifierKind::FieldYield => format!(" {:+} %", m.amount),
                ModifierKind::Festival => String::new(),
            };
            ModifierRow {
                kind: m.kind,
                amount: m.amount,
                text: format!("{}{value} ({}), {}", m.kind.display_name(), title(m.source), remaining_text(state, m.until_tick)),
                source: title(m.source),
                until_tick: m.until_tick,
            }
        })
        .collect();
    let history = e
        .history()
        .rev()
        .map(|r| {
            let def = defs.events().get(r.event.index());
            EventHistoryRow {
                serial: r.serial,
                event: r.event,
                title: title(ModifierSource::Event(r.event)),
                severity: def.map_or(Severity::Info, |d| d.severity),
                date: GameClock::at_tick(r.started_tick).display(),
                choice: def.and_then(|d| d.choices.get(usize::from(r.choice))).map_or_else(String::new, |c| c.label.clone()),
                by_default: r.by_default,
                outcome: r.outcome.clone(),
            }
        })
        .collect();
    EventsReport {
        enabled: state.settings().events_enabled,
        pending: pending_event(state, defs),
        modifiers,
        happiness_points: e.happiness_modifier(tick),
        production_percent: e.production_modifier_percent(tick),
        field_yield_percent: e.field_yield_modifier_percent(tick),
        tax_cap: e.tax_cap(tick).map(|c| (c.max_percent, GameClock::at_tick(c.until_tick).display())),
        history,
    }
}
