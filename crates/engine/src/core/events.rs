//! A small typed event queue.
//!
//! Systems push events during a frame and consumers drain them later in the same frame (or the
//! next one). This keeps modules decoupled: the producer does not need to know who listens.

use std::collections::VecDeque;

/// FIFO queue of events of type `T`.
#[derive(Debug)]
pub struct EventQueue<T> {
    events: VecDeque<T>,
    /// Hard limit to avoid unbounded growth if nobody drains the queue.
    capacity_limit: usize,
    dropped: u64,
}

impl<T> Default for EventQueue<T> {
    fn default() -> Self {
        Self::with_limit(4096)
    }
}

impl<T> EventQueue<T> {
    /// Creates a queue that keeps at most `capacity_limit` undrained events. When the limit is
    /// reached the oldest events are discarded and counted in [`EventQueue::dropped`].
    pub fn with_limit(capacity_limit: usize) -> Self {
        Self { events: VecDeque::new(), capacity_limit: capacity_limit.max(1), dropped: 0 }
    }

    pub fn push(&mut self, event: T) {
        if self.events.len() >= self.capacity_limit {
            self.events.pop_front();
            self.dropped += 1;
            if self.dropped.is_power_of_two() {
                log::warn!("event queue overflow: {} events dropped so far", self.dropped);
            }
        }
        self.events.push_back(event);
    }

    pub fn extend(&mut self, events: impl IntoIterator<Item = T>) {
        for e in events {
            self.push(e);
        }
    }

    /// Removes and returns all queued events in FIFO order.
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.events.drain(..)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.events.iter()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_order() {
        let mut q = EventQueue::default();
        q.push(1);
        q.push(2);
        q.extend([3, 4]);
        assert_eq!(q.drain().collect::<Vec<_>>(), vec![1, 2, 3, 4]);
        assert!(q.is_empty());
    }

    #[test]
    fn overflow_drops_oldest() {
        let mut q = EventQueue::with_limit(2);
        q.push(1);
        q.push(2);
        q.push(3);
        assert_eq!(q.dropped(), 1);
        assert_eq!(q.iter().copied().collect::<Vec<_>>(), vec![2, 3]);
    }
}
