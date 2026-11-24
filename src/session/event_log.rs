//! Bounded log of the events the simulation emitted, read by several consumers (map view,
//! notifications, audio) at their own pace.

use std::collections::VecDeque;
use std::collections::vec_deque;

use kingdoms_sim::event::SimEvent;

/// One logged simulation event.
#[derive(Clone, Debug, PartialEq)]
pub struct LoggedEvent {
    /// Position in the log (strictly increasing, never reused).
    pub seq: u64,
    /// Simulation tick (clock value after the tick) that produced the event.
    pub tick: u64,
    /// The event.
    pub event: SimEvent,
}

/// Read position of one consumer of an [`EventLog`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventCursor {
    next: u64,
}

/// Events read with [`EventLog::read`].
pub struct ReadEvents<'a> {
    /// True if events were dropped from the log before this consumer read them; consumers that
    /// derive state from events must then refresh it completely.
    pub missed: bool,
    /// The unread events in order.
    pub events: vec_deque::Iter<'a, LoggedEvent>,
}

/// Ring buffer of the most recent simulation events.
#[derive(Debug)]
pub struct EventLog {
    entries: VecDeque<LoggedEvent>,
    capacity: usize,
    next_seq: u64,
}

impl EventLog {
    /// Empty log keeping at most `capacity` events (at least one).
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self { entries: VecDeque::with_capacity(capacity), capacity, next_seq: 0 }
    }

    /// Appends an event, dropping the oldest one when full.
    pub fn push(&mut self, tick: u64, event: SimEvent) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(LoggedEvent { seq: self.next_seq, tick, event });
        self.next_seq += 1;
    }

    /// Number of events currently held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Total number of events ever logged.
    pub fn total(&self) -> u64 {
        self.next_seq
    }

    /// A cursor that only sees events logged from now on.
    pub fn cursor_at_end(&self) -> EventCursor {
        EventCursor { next: self.next_seq }
    }

    /// Returns the events logged since `cursor` was last advanced and moves it to the end.
    pub fn read(&self, cursor: &mut EventCursor) -> ReadEvents<'_> {
        let first = self.next_seq - self.entries.len() as u64;
        let missed = cursor.next < first;
        let skip = cursor.next.saturating_sub(first).min(self.entries.len() as u64) as usize;
        cursor.next = self.next_seq;
        ReadEvents { missed, events: self.entries.range(skip..) }
    }

    /// The held events, newest first.
    pub fn recent(&self) -> impl Iterator<Item = &LoggedEvent> + '_ {
        self.entries.iter().rev()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn year(y: u64) -> SimEvent {
        SimEvent::YearChanged { year: y }
    }

    #[test]
    fn cursors_read_each_event_once() {
        let mut log = EventLog::new(8);
        let mut a = log.cursor_at_end();
        log.push(1, year(1));
        log.push(2, year(2));
        let read = log.read(&mut a);
        assert!(!read.missed);
        assert_eq!(read.events.map(|e| e.seq).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(log.read(&mut a).events.count(), 0);
        let mut late = log.cursor_at_end();
        log.push(3, year(3));
        assert_eq!(log.read(&mut late).events.map(|e| e.tick).collect::<Vec<_>>(), vec![3]);
        assert_eq!(log.read(&mut a).events.count(), 1);
    }

    #[test]
    fn overflow_drops_oldest_and_reports_missed_events() {
        let mut log = EventLog::new(3);
        let mut cursor = log.cursor_at_end();
        for i in 0..5 {
            log.push(i, year(i));
        }
        assert_eq!(log.len(), 3);
        assert_eq!(log.total(), 5);
        let read = log.read(&mut cursor);
        assert!(read.missed);
        assert_eq!(read.events.map(|e| e.seq).collect::<Vec<_>>(), vec![2, 3, 4]);
        assert_eq!(log.recent().next().map(|e| e.seq), Some(4));
        let again = log.read(&mut cursor);
        assert!(!again.missed);
        assert_eq!(again.events.count(), 0);
    }
}
