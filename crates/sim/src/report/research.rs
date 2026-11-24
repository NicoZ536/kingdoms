//! Research window: the tech tree with the state of every technology, the queue, the rate and
//! the statistics (docs/game_design.md §9).

use serde::{Deserialize, Serialize};

use crate::defs::{Defs, TechArea, TechId};
use crate::research::{self, ResearchDay, ResearchRate, ResearchStatus};
use crate::state::GameState;

/// State of a technology in the tech tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TechState {
    /// Researched; its effects apply.
    Researched,
    /// At the front of the queue (being researched or waiting for gold).
    InProgress,
    /// In the queue behind the active technology.
    Queued,
    /// All prerequisites researched; can be queued.
    Available,
    /// Prerequisites missing (they can be queued first).
    Locked,
}

impl TechState {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            TechState::Researched => "Erforscht",
            TechState::InProgress => "In Arbeit",
            TechState::Queued => "Vorgemerkt",
            TechState::Available => "Verfügbar",
            TechState::Locked => "Gesperrt",
        }
    }
}

/// A prerequisite of a technology.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrerequisiteRow {
    /// The prerequisite.
    pub tech: TechId,
    /// German name.
    pub name: String,
    /// True if researched.
    pub researched: bool,
}

/// One technology in the tech tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechRow {
    /// The technology.
    pub tech: TechId,
    /// Canonical id.
    pub key: String,
    /// German name.
    pub name: String,
    /// German description.
    pub description: String,
    /// Research area.
    pub area: TechArea,
    /// German name of the area.
    pub area_name: String,
    /// State.
    pub state: TechState,
    /// Gold paid when the research starts.
    pub cost_gold: u32,
    /// True if the cost is already paid (an interrupted research continues for free).
    pub paid: bool,
    /// Research points needed.
    pub points: u32,
    /// Research points gathered, in milli-points.
    pub progress_milli: u64,
    /// Progress in percent (0–100).
    pub progress_percent: u8,
    /// Position in the queue (0 = active).
    pub queue_position: Option<u8>,
    /// Game hours until it is researched if the queue keeps its order (queued technologies
    /// only; `None` without research rate).
    pub eta_hours: Option<u64>,
    /// Game hours its remaining points take at the expected rate (`None` when researched or
    /// without research rate).
    pub duration_hours: Option<u64>,
    /// Prerequisites in data order.
    pub prerequisites: Vec<PrerequisiteRow>,
    /// German names of the prerequisites not researched yet.
    pub missing: Vec<String>,
    /// German effect descriptions.
    pub effects: Vec<String>,
    /// Tick it was researched (by research, not by the developer command).
    pub researched_tick: Option<u64>,
}

/// One entry of the research queue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchQueueRow {
    /// The technology.
    pub tech: TechId,
    /// German name.
    pub name: String,
    /// Progress in percent.
    pub progress_percent: u8,
    /// Game hours until it is researched (`None` without research rate).
    pub eta_hours: Option<u64>,
}

/// The research window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchReport {
    /// What the queue is doing.
    pub status: ResearchStatus,
    /// German status line, e.g. „Erforscht Getreideanbau: 40 % – noch etwa 14 h“.
    pub status_text: String,
    /// The research rate.
    pub rate: ResearchRate,
    /// German rate line, e.g. „1,5 Punkte pro Stunde (Rathaus 1,0; Schulen 0,5)“.
    pub rate_text: String,
    /// The queue in order.
    pub queue: Vec<ResearchQueueRow>,
    /// Most entries the queue takes.
    pub max_queue: u8,
    /// Every technology, by area (in [`TechArea::ALL`] order), then id.
    pub techs: Vec<TechRow>,
    /// Researched technologies.
    pub researched: usize,
    /// All technologies.
    pub total: usize,
    /// Points produced today so far, in milli-points.
    pub today_milli: u64,
    /// Points of the closed days, oldest first.
    pub history: Vec<ResearchDay>,
}

/// German number with one decimal (decimal comma), e.g. „1,5“.
fn points(milli: u64) -> String {
    let tenths = (milli + 50) / 100;
    format!("{},{}", tenths / 10, tenths % 10)
}

/// German estimate from game hours in the duration form of the game (as the UI writes
/// estimates): „etwa 1 h“, „etwa 14 h“, from two days on whole days, rounded up („etwa
/// 3 Tage“).
pub fn duration_text(hours: u64) -> String {
    match hours {
        0..=47 => format!("etwa {} h", hours.max(1)),
        _ => format!("etwa {}", super::hours_text(hours.div_ceil(24) * 24)),
    }
}

/// The research window (see [`ResearchReport`]).
pub fn research_report(state: &GameState, defs: &Defs) -> ResearchReport {
    let r = state.research();
    let rate = research::research_rate(state, defs);
    let status = r.status(defs, state.economy().treasury());
    // Cumulative remaining points along the queue for the time estimates.
    let mut remaining_total = 0u64;
    let mut eta: Vec<(TechId, Option<u64>)> = Vec::new();
    for t in r.queue() {
        remaining_total += remaining_milli(state, defs, *t);
        eta.push((*t, research::hours_for(remaining_total, rate.daily_milli)));
    }
    let percent = |t: TechId| -> u8 {
        let needed = u64::from(defs.tech(t).research_points).max(1) * 1000;
        u8::try_from((r.progress_milli(t) * 100 / needed).min(100)).unwrap_or(100)
    };
    let queue: Vec<ResearchQueueRow> = eta
        .iter()
        .map(|(t, h)| ResearchQueueRow { tech: *t, name: research::tech_name(defs, *t), progress_percent: percent(*t), eta_hours: *h })
        .collect();
    let mut techs = Vec::with_capacity(defs.techs().len());
    for area in TechArea::ALL {
        for def in defs.techs_in(area) {
            let t = def.id;
            let position = r.queue().iter().position(|q| *q == t);
            let missing: Vec<String> =
                def.prerequisites.iter().filter(|p| !r.is_researched(**p)).map(|p| research::tech_name(defs, *p)).collect();
            let state_of = if r.is_researched(t) {
                TechState::Researched
            } else if position == Some(0) {
                TechState::InProgress
            } else if position.is_some() {
                TechState::Queued
            } else if missing.is_empty() {
                TechState::Available
            } else {
                TechState::Locked
            };
            let remaining = remaining_milli(state, defs, t);
            techs.push(TechRow {
                tech: t,
                key: def.key.clone(),
                name: def.name.clone(),
                description: def.description.clone(),
                area,
                area_name: area.display_name().to_owned(),
                state: state_of,
                cost_gold: def.cost_gold,
                paid: r.is_paid(t),
                points: def.research_points,
                progress_milli: if r.is_researched(t) { u64::from(def.research_points) * 1000 } else { r.progress_milli(t) },
                progress_percent: if r.is_researched(t) { 100 } else { percent(t) },
                queue_position: position.and_then(|p| u8::try_from(p).ok()),
                eta_hours: eta.iter().find(|(q, _)| *q == t).and_then(|(_, h)| *h),
                duration_hours: if r.is_researched(t) { None } else { research::hours_for(remaining, rate.daily_milli) },
                prerequisites: def
                    .prerequisites
                    .iter()
                    .map(|p| PrerequisiteRow { tech: *p, name: research::tech_name(defs, *p), researched: r.is_researched(*p) })
                    .collect(),
                missing,
                effects: def.effects.iter().map(|e| research::effect_text(defs, e)).collect(),
                researched_tick: r.completed().iter().find(|c| c.tech == t).map(|c| c.tick),
            });
        }
    }
    let status_text = match status {
        ResearchStatus::Idle if r.researched().count() >= defs.techs().len() => "Alle Technologien sind erforscht.".to_owned(),
        ResearchStatus::Idle => "Keine Forschung ausgewählt – die Forschungspunkte verfallen.".to_owned(),
        ResearchStatus::WaitingForGold { tech, missing } => {
            format!("„{}“ wartet auf Gold: {}.", research::tech_name(defs, tech), crate::report::missing_gold(i64::from(missing)))
        }
        ResearchStatus::Researching(tech) => {
            let eta = queue.first().and_then(|q| q.eta_hours).map_or_else(
                || " – ohne Forschungspunkte steht sie still".to_owned(),
                |h| format!(" – noch {}", duration_text(h)),
            );
            format!("Erforscht „{}“: {} %{eta}.", research::tech_name(defs, tech), percent(tech))
        }
    };
    let mut parts = vec![format!("Hauptquartier {}", points(rate.base_milli))];
    if !rate.sources.is_empty() {
        parts.push(format!("Schulen {}", points(rate.buildings_milli)));
    }
    if rate.bonus_percent != 0 {
        parts.push(format!("Forschung {:+} %", rate.bonus_percent));
    }
    let rate_text = format!(
        "{} Punkte pro Stunde ({}); etwa {} Punkte pro Tag",
        points(rate.current_milli),
        parts.join("; "),
        points(rate.daily_milli)
    );
    ResearchReport {
        status,
        status_text,
        rate,
        rate_text,
        queue,
        max_queue: defs.rules().research.max_queue,
        techs,
        researched: r.researched().count(),
        total: defs.techs().len(),
        today_milli: r.today_milli(),
        history: r.history().copied().collect(),
    }
}

/// Research points of `tech` still missing, in milli-points.
pub fn remaining_milli(state: &GameState, defs: &Defs, tech: TechId) -> u64 {
    (u64::from(defs.tech(tech).research_points) * 1000).saturating_sub(state.research().progress_milli(tech))
}
