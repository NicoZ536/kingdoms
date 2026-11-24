//! Walking reports (docs/game_design.md §6): commutes of a workplace ("Arbeitsweg"), citizens
//! without a path to a building, and an overview of everybody on the way.

use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::defs::Defs;
use crate::population::{Destination, TripPhase, building_work_window, is_at_work};
use crate::state::GameState;
use crate::time::SECONDS_PER_TICK;

/// Game minutes of `ticks` (18 s per tick).
pub fn ticks_to_minutes(ticks: u64) -> u32 {
    u32::try_from(ticks * SECONDS_PER_TICK / 60).unwrap_or(u32::MAX)
}

/// German game duration from minutes: „35 min“, „2 h“, „15 h 36 min“.
pub fn minutes_text(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// German game duration from whole hours in the one form of the game (the UI writes it the
/// same way): „1 h“, „18 h“ under a day, then days with the rest in hours: „1 Tag“,
/// „1 Tag 6 h“, „3 Tage“.
pub fn hours_text(hours: u64) -> String {
    let (days, rest) = (hours / 24, hours % 24);
    let days_part = match days {
        0 => return format!("{rest} h"),
        1 => "1 Tag".to_owned(),
        n => format!("{n} Tage"),
    };
    if rest == 0 { days_part } else { format!("{days_part} {rest} h") }
}

/// German game duration of `ticks` in the one form of the game: minutes under a day
/// ([`minutes_text`]: „45 min“, „7 h 30 min“), from a day on whole hours ([`hours_text`]:
/// „1 Tag 6 h“, rounded up).
pub fn ticks_text(ticks: u64) -> String {
    if ticks < crate::time::TICKS_PER_DAY {
        minutes_text(ticks_to_minutes(ticks))
    } else {
        hours_text(ticks.div_ceil(crate::time::TICKS_PER_HOUR))
    }
}

/// The commute of a workplace's workers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommuteReport {
    /// Workers assigned.
    pub assigned: u8,
    /// Workers at work right now.
    pub present: u8,
    /// Workers on the way (or waiting for a path) right now.
    pub walking: u8,
    /// Workers who find no path to the workplace.
    pub no_path: u8,
    /// Average duration of the workers' last walk to work in game minutes (`None` before the
    /// first arrival).
    pub average_minutes: Option<u32>,
    /// Longest of these walks in game minutes.
    pub longest_minutes: Option<u32>,
    /// Share of the shift the average walk costs, in percent (work starts on arrival).
    pub lost_percent: u32,
    /// German summary, e.g. "Arbeitsweg Ø 12 min (3 % der Schicht)".
    pub text: String,
}

/// Commute of the workers of `building` (also for markets, storages and sites).
pub fn commute_report(state: &GameState, defs: &Defs, building: BuildingId) -> CommuteReport {
    let pop = state.population();
    let workers: Vec<_> = pop.workers_of(building).iter().filter_map(|c| pop.citizen(*c)).collect();
    let goes_here =
        |dest: Option<Destination>| matches!(dest, Some(Destination::Building { building: b, .. }) if b == building);
    let count = |f: &dyn Fn(&crate::population::Citizen) -> bool| {
        u8::try_from(workers.iter().filter(|c| f(c)).count()).unwrap_or(u8::MAX)
    };
    let present = count(&|c| is_at_work(state, defs, c));
    let walking = count(&|c| {
        matches!(c.motion().phase(), TripPhase::Walking | TripPhase::Queued) && goes_here(c.motion().destination())
    });
    let no_path =
        count(&|c| matches!(c.motion().phase(), TripPhase::NoPath { .. }) && goes_here(c.motion().destination()));
    let walks: Vec<u64> = workers.iter().filter_map(|c| c.motion().last_commute_ticks()).map(u64::from).collect();
    let average = (!walks.is_empty()).then(|| walks.iter().sum::<u64>() / walks.len() as u64);
    let longest = walks.iter().max().copied();
    let window = building_work_window(state, defs, building);
    let shift = window.end.saturating_sub(window.start).saturating_sub(window.lunch_end - window.lunch_start).max(1);
    let lost_percent = average.map_or(0, |a| u32::try_from((a * 100 / shift).min(100)).unwrap_or(100));
    let text = match average {
        Some(a) => format!("Arbeitsweg Ø {} min ({lost_percent} % der Schicht)", ticks_to_minutes(a)),
        None if workers.is_empty() => "Keine Arbeiter".to_string(),
        None => "Arbeitsweg noch nicht gemessen".to_string(),
    };
    CommuteReport {
        assigned: u8::try_from(workers.len()).unwrap_or(u8::MAX),
        present,
        walking,
        no_path,
        average_minutes: average.map(ticks_to_minutes),
        longest_minutes: longest.map(ticks_to_minutes),
        lost_percent,
        text,
    }
}

/// Citizens heading for `building` who find no path there.
pub fn citizens_without_path(state: &GameState, building: BuildingId) -> u32 {
    let n = state
        .population()
        .citizens()
        .filter(|(_, c)| {
            matches!(c.motion().phase(), TripPhase::NoPath { .. })
                && matches!(c.motion().destination(), Some(Destination::Building { building: b, .. }) if b == building)
        })
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// German status line for a building that citizens cannot reach ("Kein Weg: 2 Bewohner
/// erreichen das Gebäude nicht."); `None` if everybody heading there finds a path.
pub fn path_problem(state: &GameState, building: BuildingId) -> Option<String> {
    match citizens_without_path(state, building) {
        0 => None,
        1 => Some("Kein Weg: 1 Bewohner erreicht das Gebäude nicht.".to_string()),
        n => Some(format!("Kein Weg: {n} Bewohner erreichen das Gebäude nicht.")),
    }
}

/// Everybody on the way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalkingSummary {
    /// Citizens walking right now.
    pub walking: u32,
    /// Citizens waiting for the pathfinder.
    pub waiting_for_path: u32,
    /// Citizens without a path to their destination.
    pub no_path: u32,
    /// Citizens inside buildings.
    pub inside: u32,
    /// Emigrants walking to the map edge.
    pub leaving: u32,
    /// Average of all workers' last walk to work in game minutes (`None` before any).
    pub average_commute_minutes: Option<u32>,
}

/// Overview of the walking population.
pub fn walking_summary(state: &GameState) -> WalkingSummary {
    let mut s = WalkingSummary {
        walking: 0,
        waiting_for_path: 0,
        no_path: 0,
        inside: 0,
        leaving: u32::try_from(state.population().walking().travelers().count()).unwrap_or(u32::MAX),
        average_commute_minutes: None,
    };
    let (mut sum, mut n) = (0u64, 0u64);
    for (_, c) in state.population().citizens() {
        let m = c.motion();
        match m.phase() {
            TripPhase::Walking => s.walking += 1,
            TripPhase::Queued => s.waiting_for_path += 1,
            TripPhase::NoPath { .. } => s.no_path += 1,
            TripPhase::Still => {}
        }
        if m.inside().is_some() {
            s.inside += 1;
        }
        if c.job().is_some()
            && let Some(t) = m.last_commute_ticks()
        {
            sum += u64::from(t);
            n += 1;
        }
    }
    s.average_commute_minutes = (n > 0).then(|| ticks_to_minutes(sum / n));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_in_hours_and_minutes() {
        assert_eq!(minutes_text(0), "0 min");
        assert_eq!(minutes_text(35), "35 min");
        assert_eq!(minutes_text(120), "2 h");
        assert_eq!(minutes_text(936), "15 h 36 min");
    }

    #[test]
    fn longer_durations_read_in_days_and_hours() {
        use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
        assert_eq!(hours_text(0), "0 h");
        assert_eq!(hours_text(1), "1 h");
        assert_eq!(hours_text(18), "18 h");
        assert_eq!(hours_text(24), "1 Tag");
        assert_eq!(hours_text(30), "1 Tag 6 h");
        assert_eq!(hours_text(72), "3 Tage");
        assert_eq!(hours_text(50), "2 Tage 2 h");
        assert_eq!(ticks_text(19 * TICKS_PER_HOUR / 2), "9 h 30 min");
        assert_eq!(ticks_text(TICKS_PER_DAY), "1 Tag");
        // From a day on, a started hour counts.
        assert_eq!(ticks_text(TICKS_PER_DAY + 1), "1 Tag 1 h");
    }
}
