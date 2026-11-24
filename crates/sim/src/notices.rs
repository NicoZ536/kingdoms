//! Spells of lasting waiting states (docs/game_design.md §2 Meldungen).
//!
//! A building nobody can reach, a storage without carriers, hunger, cold or homelessness are
//! states, not events: the player is told once when such a state begins, and again only after
//! it has not been seen for `notices.waiting_resolved_hours` in a row (`rules.ron`). The state
//! itself stays visible in the reports. [`Spells`] remembers per key when the state was last
//! seen; production buildings keep their own spell next to their status
//! (`crate::production`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Per key the tick a waiting state was last seen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Spells<K: Ord> {
    seen: BTreeMap<K, u64>,
}

impl<K: Ord> Default for Spells<K> {
    fn default() -> Self {
        Self { seen: BTreeMap::new() }
    }
}

impl<K: Ord + Copy> Spells<K> {
    /// The state `key` holds at `now`. True if this begins a new spell (never seen, or not seen
    /// for more than `gap` ticks): the caller reports it.
    pub(crate) fn begins(&mut self, key: K, now: u64, gap: u64) -> bool {
        let new = self.seen.get(&key).is_none_or(|t| now.saturating_sub(*t) > gap);
        self.seen.insert(key, now);
        new
    }

    /// True while a spell of `key` lasts at `now` (seen within the last `gap` ticks).
    #[cfg(test)]
    pub(crate) fn holds(&self, key: K, now: u64, gap: u64) -> bool {
        self.seen.get(&key).is_some_and(|t| now.saturating_sub(*t) <= gap)
    }

    /// Forgets the spells that are over at `now` and the keys `keep` rejects.
    pub(crate) fn prune(&mut self, now: u64, gap: u64, keep: impl Fn(K) -> bool) {
        self.seen.retain(|k, t| now.saturating_sub(*t) <= gap && keep(*k));
    }

    /// Error if a spell was seen after `now` (a crafted save).
    pub(crate) fn validate(&self, now: u64, what: &str) -> Result<(), String> {
        if self.seen.values().any(|t| *t > now) {
            return Err(format!("{what}: a waiting state seen in the future"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spell_is_reported_once_and_again_only_after_a_break() {
        let mut s: Spells<u8> = Spells::default();
        assert!(s.begins(1, 100, 50), "first sighting");
        assert!(!s.begins(1, 140, 50), "seen again within the gap: same spell");
        assert!(!s.begins(1, 190, 50), "the gap counts from the last sighting");
        assert!(s.holds(1, 240, 50) && !s.holds(1, 241, 50));
        assert!(s.begins(1, 241, 50), "not seen for longer than the gap: a new spell");
        assert!(s.begins(2, 260, 50), "keys are independent");
        s.prune(300, 50, |_| true);
        assert!(!s.holds(1, 300, 50) && s.holds(2, 300, 50), "key 1 was last seen at 241, key 2 at 260");
        s.prune(400, 50, |_| true);
        assert_eq!(s, Spells::default(), "finished spells are forgotten");
        assert!(s.validate(400, "x").is_ok());
        s.begins(3, 500, 50);
        assert!(s.validate(400, "x").is_err());
        s.prune(500, 50, |k| k != 3);
        assert_eq!(s, Spells::default());
    }
}
