//! Bookkeeping of the automatic pause (settings „Bei Entscheidungen anhalten“ and „Bei Gefahr
//! anhalten“): why the game paused itself and the speed it ran at before. The game resumes
//! that speed once the player has dealt with every reason – decided the event, answered or
//! postponed („Später“) the proposals – unless a danger paused it too (only the player resumes
//! after a danger) or the player changed the speed in between (then the pause is theirs).

use kingdoms_engine::core::time::GameSpeed;
use kingdoms_sim::defs::Defs;
use kingdoms_sim::event::{Severity, SimEvent};

use crate::settings::GameplaySettings;

/// Why the game paused itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseReason {
    /// An event waits for a decision (released by deciding it).
    Event,
    /// A kingdom's proposal waits for an answer (released when the proposal card closes: all
    /// answered, „Später“ or Esc).
    Proposal,
    /// A danger notification (fire, attack): held until the player changes the speed.
    Danger,
}

impl PauseReason {
    /// The reason `event` gives to pause the game with the settings `game`, if any.
    pub fn of(event: &SimEvent, game: &GameplaySettings) -> Option<Self> {
        match event {
            SimEvent::EventStarted { .. } if game.pause_on_event => Some(Self::Event),
            SimEvent::ProposalReceived { .. } if game.pause_on_event => Some(Self::Proposal),
            SimEvent::Notification(n) if game.pause_on_danger && n.severity == Severity::Danger => Some(Self::Danger),
            _ => None,
        }
    }
}

/// Why the game should pause after `events` (settings: event decisions, dangers), as a German
/// message for the toast; the last reason wins.
pub fn pause_reason<'a>(events: impl Iterator<Item = &'a SimEvent>, game: &GameplaySettings, defs: &Defs) -> Option<String> {
    let mut text = None;
    for event in events {
        text = match (PauseReason::of(event, game), event) {
            (Some(PauseReason::Event), SimEvent::EventStarted { event, .. }) => {
                let title = defs.events().get(event.index()).map_or("Ereignis", |e| e.title.as_str());
                Some(format!("Angehalten: „{title}“ verlangt eine Entscheidung."))
            }
            (Some(PauseReason::Proposal), _) => Some("Angehalten: Eine Gesandtschaft wartet auf Antwort.".to_owned()),
            (Some(PauseReason::Danger), SimEvent::Notification(n)) => Some(format!("Angehalten: {}", n.text)),
            _ => text,
        };
    }
    text
}

/// The automatic pause in progress, if any.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoPause {
    /// Speed before the game paused itself; `None` while no automatic pause holds.
    resume: Option<GameSpeed>,
    event: bool,
    proposal: bool,
    danger: bool,
}

impl AutoPause {
    /// New `reasons` arrived while the game runs at `current`. Returns true if the game must
    /// pause now (it was running). Reasons arriving during an automatic pause join it; a pause
    /// of the player stays the player's (nothing is recorded, nothing resumes by itself).
    pub fn pause(&mut self, reasons: &[PauseReason], current: GameSpeed) -> bool {
        if reasons.is_empty() {
            return false;
        }
        let start = !current.is_paused();
        if start {
            *self = Self { resume: Some(current), ..Self::default() };
        } else if self.resume.is_none() {
            return false;
        }
        for &reason in reasons {
            *self.flag(reason) = true;
        }
        start
    }

    /// The player dealt with `reason`. Returns the speed to resume when it was the last reason
    /// of the automatic pause, the game still stands still and the pause menu is closed
    /// (`menu_open`); otherwise `None`. Once no reason is left the automatic pause is over.
    pub fn release(&mut self, reason: PauseReason, current: GameSpeed, menu_open: bool) -> Option<GameSpeed> {
        *self.flag(reason) = false;
        if self.event || self.proposal || self.danger {
            return None;
        }
        let resume = self.resume.take()?;
        (current.is_paused() && !menu_open).then_some(resume)
    }

    /// The player changed the speed, a game was loaded or lost: the automatic pause is over.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// True while an automatic pause holds for `reason`.
    #[cfg(test)]
    fn holds(&self, reason: PauseReason) -> bool {
        let held = match reason {
            PauseReason::Event => self.event,
            PauseReason::Proposal => self.proposal,
            PauseReason::Danger => self.danger,
        };
        self.resume.is_some() && held
    }

    fn flag(&mut self, reason: PauseReason) -> &mut bool {
        match reason {
            PauseReason::Event => &mut self.event,
            PauseReason::Proposal => &mut self.proposal,
            PauseReason::Danger => &mut self.danger,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kingdoms_sim::event::Notification;

    use super::*;

    const PAUSED: GameSpeed = GameSpeed::Paused;

    fn defs() -> Arc<Defs> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => Arc::new(defs),
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    #[test]
    fn postponing_the_only_proposal_resumes_the_speed_before() {
        let mut pause = AutoPause::default();
        assert!(pause.pause(&[PauseReason::Proposal], GameSpeed::Double), "the running game pauses");
        assert!(pause.holds(PauseReason::Proposal));
        assert_eq!(pause.release(PauseReason::Proposal, PAUSED, false), Some(GameSpeed::Double));
        assert_eq!(pause, AutoPause::default(), "the automatic pause is over");
        // Releasing again (an answer after „Später“) changes nothing.
        assert_eq!(pause.release(PauseReason::Proposal, PAUSED, false), None);
    }

    #[test]
    fn an_open_event_decision_keeps_the_game_paused() {
        let mut pause = AutoPause::default();
        assert!(pause.pause(&[PauseReason::Event, PauseReason::Proposal], GameSpeed::Normal));
        assert_eq!(pause.release(PauseReason::Proposal, PAUSED, false), None, "the event still waits");
        assert!(pause.holds(PauseReason::Event));
        assert_eq!(pause.release(PauseReason::Event, PAUSED, false), Some(GameSpeed::Normal));
        // The other order: deciding the event first leaves the proposal.
        assert!(pause.pause(&[PauseReason::Event, PauseReason::Proposal], GameSpeed::Quadruple));
        assert_eq!(pause.release(PauseReason::Event, PAUSED, false), None);
        assert_eq!(pause.release(PauseReason::Proposal, PAUSED, false), Some(GameSpeed::Quadruple));
    }

    #[test]
    fn a_danger_is_resumed_only_by_the_player() {
        let mut pause = AutoPause::default();
        assert!(pause.pause(&[PauseReason::Proposal], GameSpeed::Normal));
        // A danger in the same pause (several ticks per frame) joins it.
        assert!(!pause.pause(&[PauseReason::Danger], PAUSED));
        assert_eq!(pause.release(PauseReason::Proposal, PAUSED, false), None);
        assert!(pause.holds(PauseReason::Danger));
        pause.clear();
        assert!(!pause.holds(PauseReason::Danger));
    }

    #[test]
    fn the_players_own_pause_stays() {
        let mut pause = AutoPause::default();
        // Paused by the player when the proposal arrived: nothing to resume.
        assert!(!pause.pause(&[PauseReason::Proposal], PAUSED));
        assert!(!pause.holds(PauseReason::Proposal));
        assert_eq!(pause.release(PauseReason::Proposal, PAUSED, false), None);
        // Paused automatically, then the player changed the speed (pause key twice).
        assert!(pause.pause(&[PauseReason::Proposal], GameSpeed::Normal));
        pause.clear();
        assert_eq!(pause.release(PauseReason::Proposal, PAUSED, false), None);
        // Running again already, or the pause menu is open: nothing resumes, the pause is over.
        assert!(pause.pause(&[PauseReason::Event], GameSpeed::Double));
        assert_eq!(pause.release(PauseReason::Event, GameSpeed::Normal, false), None);
        assert!(pause.pause(&[PauseReason::Event], GameSpeed::Double));
        assert_eq!(pause.release(PauseReason::Event, PAUSED, true), None);
        assert_eq!(pause, AutoPause::default());
        // No reason, no pause.
        assert!(!pause.pause(&[], GameSpeed::Normal));
    }

    #[test]
    fn automatic_pause_follows_the_settings() {
        let defs = defs();
        let Some(drought) = defs.event_id("drought") else { panic!("the shipped data has the drought event") };
        let started = SimEvent::EventStarted { serial: 1, event: drought, position: None, expires_tick: 100 };
        let danger = SimEvent::Notification(Notification::new(Severity::Danger, "Feuer in der Bäckerei!"));
        let info = SimEvent::Notification(Notification::new(Severity::Info, "Ein Haus wurde gebaut."));
        let Some(kingdom) = kingdoms_sim::diplomacy::KingdomId::from_index(0) else { panic!("kingdom 0") };
        let proposal = SimEvent::ProposalReceived { serial: 4, kingdom, expires_tick: 900 };
        let events = [started.clone(), info.clone(), danger.clone(), proposal.clone()];
        let off = GameplaySettings::default();
        assert_eq!(pause_reason(events.iter(), &off, &defs), None);
        assert!(events.iter().all(|e| PauseReason::of(e, &off).is_none()));
        let on_event = GameplaySettings { pause_on_event: true, ..GameplaySettings::default() };
        let reason = pause_reason([started.clone(), info.clone()].iter(), &on_event, &defs);
        assert!(reason.is_some_and(|r| r.starts_with("Angehalten: „") && r.contains("Entscheidung")));
        assert_eq!(PauseReason::of(&started, &on_event), Some(PauseReason::Event));
        assert_eq!(PauseReason::of(&danger, &on_event), None);
        assert_eq!(PauseReason::of(&proposal, &on_event), Some(PauseReason::Proposal));
        assert_eq!(pause_reason([started.clone(), proposal].iter(), &on_event, &defs).as_deref(), Some("Angehalten: Eine Gesandtschaft wartet auf Antwort."));
        let on_danger = GameplaySettings { pause_on_danger: true, ..GameplaySettings::default() };
        assert_eq!(pause_reason([info.clone(), danger.clone()].iter(), &on_danger, &defs).as_deref(), Some("Angehalten: Feuer in der Bäckerei!"));
        assert_eq!(PauseReason::of(&danger, &on_danger), Some(PauseReason::Danger));
        assert_eq!(pause_reason([info].iter(), &on_danger, &defs), None);
    }
}
