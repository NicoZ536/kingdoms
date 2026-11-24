//! Generational arenas and typed entity ids.
//!
//! Simulation entities (buildings, citizens, construction sites, …) live in [`Arena`]s and are
//! referenced by [`Id`]s. An id combines a slot index with a generation counter: when an entity is
//! removed, its slot's generation is bumped, so ids that still point at the old entity become
//! *stale* and every lookup through them returns `None` instead of silently hitting the slot's
//! new occupant.
//!
//! Ids are typed via `PhantomData`, so an `Id<Building>` can never be used to index an
//! `Arena<Citizen>`. Iteration always runs in ascending slot index order and the free list is part
//! of the serialized state, which makes arenas fully deterministic: the same sequence of
//! insertions and removals always yields the same ids, also across save/load.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

/// Typed, generational handle to a value stored in an [`Arena<T>`].
///
/// `Id` is `Copy` and cheap to compare, hash and order (ordering is by index, then generation).
#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Id<T> {
    index: u32,
    generation: u32,
    #[serde(skip)]
    marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    /// Builds an id from its raw parts.
    ///
    /// Intended for tooling, debugging and tests; regular code obtains ids from
    /// [`Arena::insert`]. An id built this way is simply stale if it does not match a live slot.
    pub const fn from_raw_parts(index: u32, generation: u32) -> Self {
        Self { index, generation, marker: PhantomData }
    }

    /// Slot index inside the arena.
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Generation of the slot at the time the id was created.
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<T> Eq for Id<T> {}

impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.index, self.generation).cmp(&(other.index, other.generation))
    }
}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({}v{})", self.index, self.generation)
    }
}

/// One storage cell of an arena.
#[derive(Clone, Debug, Serialize, Deserialize)]
enum Slot<T> {
    Occupied { generation: u32, value: T },
    Free { generation: u32, next_free: Option<u32> },
}

impl<T> Slot<T> {
    fn generation(&self) -> u32 {
        match self {
            Slot::Occupied { generation, .. } | Slot::Free { generation, .. } => *generation,
        }
    }
}

/// Generational arena: a `Vec` of slots with a free list and typed [`Id`]s.
///
/// * insertion reuses the most recently freed slot (LIFO free list), otherwise appends,
/// * removal bumps the slot generation so outstanding ids become stale,
/// * iteration is in ascending index order.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free_head: Option<u32>,
    len: u32,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    /// Creates an empty arena.
    pub const fn new() -> Self {
        Self { slots: Vec::new(), free_head: None, len: 0 }
    }

    /// Creates an empty arena with room for `capacity` values before reallocating.
    pub fn with_capacity(capacity: usize) -> Self {
        Self { slots: Vec::with_capacity(capacity), free_head: None, len: 0 }
    }

    /// Number of live values.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// True if the arena holds no live values.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of slots (live and free); an upper bound for [`Id::index`] of live ids.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Inserts a value and returns its id.
    ///
    /// # Panics
    /// Panics if the arena would exceed `u32::MAX` slots, which is far beyond any game state.
    pub fn insert(&mut self, value: T) -> Id<T> {
        self.insert_with(|_| value)
    }

    /// Inserts a value constructed from its own future id (for entities that store their id).
    ///
    /// # Panics
    /// Panics if the arena would exceed `u32::MAX` slots.
    pub fn insert_with(&mut self, make: impl FnOnce(Id<T>) -> T) -> Id<T> {
        if let Some(index) = self.free_head {
            if let Some(&Slot::Free { generation, next_free }) = self.slots.get(index as usize) {
                let id = Id::from_raw_parts(index, generation);
                self.slots[index as usize] = Slot::Occupied { generation, value: make(id) };
                self.free_head = next_free;
                self.len += 1;
                return id;
            }
            // Only reachable with unvalidated deserialized data (see `check_integrity`): drop the
            // broken free list instead of handing out an id to an occupied slot.
            log::warn!("arena free list is corrupted (head {index}); discarding it");
            self.free_head = None;
        }
        let index = u32::try_from(self.slots.len()).expect("arena exceeded u32::MAX slots");
        let id = Id::from_raw_parts(index, 0);
        self.slots.push(Slot::Occupied { generation: 0, value: make(id) });
        self.len += 1;
        id
    }

    /// Removes the value behind `id`; returns it, or `None` if the id is stale or invalid.
    pub fn remove(&mut self, id: Id<T>) -> Option<T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        match slot {
            Slot::Occupied { generation, .. } if *generation == id.generation => {
                let next_generation = generation.wrapping_add(1);
                let old =
                    std::mem::replace(slot, Slot::Free { generation: next_generation, next_free: self.free_head });
                self.free_head = Some(id.index);
                self.len -= 1;
                match old {
                    Slot::Occupied { value, .. } => Some(value),
                    Slot::Free { .. } => unreachable!("slot was checked to be occupied"),
                }
            }
            _ => None,
        }
    }

    /// Shared access to the value behind `id`, or `None` if the id is stale or invalid.
    pub fn get(&self, id: Id<T>) -> Option<&T> {
        match self.slots.get(id.index as usize)? {
            Slot::Occupied { generation, value } if *generation == id.generation => Some(value),
            _ => None,
        }
    }

    /// Mutable access to the value behind `id`, or `None` if the id is stale or invalid.
    pub fn get_mut(&mut self, id: Id<T>) -> Option<&mut T> {
        match self.slots.get_mut(id.index as usize)? {
            Slot::Occupied { generation, value } if *generation == id.generation => Some(value),
            _ => None,
        }
    }

    /// Mutable access to two distinct values at once. Returns `None` if either id is stale or
    /// both ids refer to the same slot.
    pub fn get2_mut(&mut self, a: Id<T>, b: Id<T>) -> Option<(&mut T, &mut T)> {
        if a.index == b.index || !self.contains(a) || !self.contains(b) {
            return None;
        }
        let (lo, hi, swapped) = if a.index < b.index { (a, b, false) } else { (b, a, true) };
        let (left, right) = self.slots.split_at_mut(hi.index as usize);
        let lo_ref = match &mut left[lo.index as usize] {
            Slot::Occupied { value, .. } => value,
            Slot::Free { .. } => return None,
        };
        let hi_ref = match &mut right[0] {
            Slot::Occupied { value, .. } => value,
            Slot::Free { .. } => return None,
        };
        Some(if swapped { (hi_ref, lo_ref) } else { (lo_ref, hi_ref) })
    }

    /// True if `id` refers to a live value.
    pub fn contains(&self, id: Id<T>) -> bool {
        self.get(id).is_some()
    }

    /// The current id of the live value in slot `index`, if any.
    pub fn id_at(&self, index: u32) -> Option<Id<T>> {
        match self.slots.get(index as usize)? {
            Slot::Occupied { generation, .. } => Some(Id::from_raw_parts(index, *generation)),
            Slot::Free { .. } => None,
        }
    }

    /// Iterates `(id, &value)` pairs in ascending index order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (Id<T>, &T)> + '_ {
        self.slots.iter().enumerate().filter_map(|(i, slot)| match slot {
            Slot::Occupied { generation, value } => Some((Id::from_raw_parts(i as u32, *generation), value)),
            Slot::Free { .. } => None,
        })
    }

    /// Iterates `(id, &mut value)` pairs in ascending index order.
    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = (Id<T>, &mut T)> + '_ {
        self.slots.iter_mut().enumerate().filter_map(|(i, slot)| match slot {
            Slot::Occupied { generation, value } => Some((Id::from_raw_parts(i as u32, *generation), value)),
            Slot::Free { .. } => None,
        })
    }

    /// Iterates the ids of all live values in ascending index order.
    pub fn ids(&self) -> impl DoubleEndedIterator<Item = Id<T>> + '_ {
        self.iter().map(|(id, _)| id)
    }

    /// Iterates all live values in ascending index order.
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &T> + '_ {
        self.iter().map(|(_, v)| v)
    }

    /// Removes every value for which `keep` returns false (in ascending index order).
    pub fn retain(&mut self, mut keep: impl FnMut(Id<T>, &mut T) -> bool) {
        let doomed: Vec<Id<T>> = self.iter_mut().filter_map(|(id, v)| (!keep(id, v)).then_some(id)).collect();
        for id in doomed {
            self.remove(id);
        }
    }

    /// Removes all values. Slots are kept (with bumped generations), so old ids stay stale.
    pub fn clear(&mut self) {
        let ids: Vec<Id<T>> = self.ids().collect();
        for id in ids {
            self.remove(id);
        }
    }

    /// Checks the internal invariants (free list integrity, live count). Used to validate
    /// deserialized data, which may come from a damaged or manipulated save file.
    pub fn check_integrity(&self) -> Result<(), ArenaIntegrityError> {
        let occupied = self.slots.iter().filter(|s| matches!(s, Slot::Occupied { .. })).count();
        if occupied != self.len as usize {
            return Err(ArenaIntegrityError::LenMismatch { stored: self.len as usize, actual: occupied });
        }
        let free_total = self.slots.len() - occupied;
        let mut visited = 0usize;
        let mut cursor = self.free_head;
        while let Some(index) = cursor {
            if visited >= free_total {
                return Err(ArenaIntegrityError::FreeListCycle);
            }
            match self.slots.get(index as usize) {
                Some(Slot::Free { next_free, .. }) => cursor = *next_free,
                Some(Slot::Occupied { .. }) => return Err(ArenaIntegrityError::FreeListHitsOccupied(index)),
                None => return Err(ArenaIntegrityError::FreeListOutOfBounds(index)),
            }
            visited += 1;
        }
        if visited != free_total {
            return Err(ArenaIntegrityError::UnreachableFreeSlots { listed: visited, free: free_total });
        }
        Ok(())
    }

    /// Highest generation stored in any slot (diagnostics).
    pub fn max_generation(&self) -> u32 {
        self.slots.iter().map(Slot::generation).max().unwrap_or(0)
    }
}

/// Structural defect found by [`Arena::check_integrity`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ArenaIntegrityError {
    /// The stored live count does not match the number of occupied slots.
    #[error("arena length mismatch: stored {stored}, actual {actual}")]
    LenMismatch {
        /// Count stored in the arena.
        stored: usize,
        /// Count of occupied slots.
        actual: usize,
    },
    /// The free list loops.
    #[error("arena free list contains a cycle")]
    FreeListCycle,
    /// The free list references an occupied slot.
    #[error("arena free list references occupied slot {0}")]
    FreeListHitsOccupied(u32),
    /// The free list references a slot that does not exist.
    #[error("arena free list references missing slot {0}")]
    FreeListOutOfBounds(u32),
    /// Some free slots are not reachable through the free list.
    #[error("arena free list lists {listed} of {free} free slots")]
    UnreachableFreeSlots {
        /// Free slots reachable from the list head.
        listed: usize,
        /// Free slots in total.
        free: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
    struct Building(u32);

    #[derive(Debug)]
    struct Citizen;

    #[test]
    fn insert_get_remove() {
        let mut arena = Arena::new();
        let a = arena.insert(Building(1));
        let b = arena.insert(Building(2));
        assert_eq!(arena.len(), 2);
        assert_eq!(arena.get(a), Some(&Building(1)));
        assert_eq!(arena.get(b), Some(&Building(2)));
        arena.get_mut(a).unwrap().0 = 10;
        assert_eq!(arena.remove(a), Some(Building(10)));
        assert_eq!(arena.len(), 1);
        assert!(!arena.contains(a));
        assert_eq!(arena.remove(a), None, "double remove must fail");
        assert!(arena.contains(b));
    }

    #[test]
    fn reuse_bumps_generation_and_stale_ids_fail() {
        let mut arena = Arena::new();
        let a = arena.insert(Building(1));
        arena.remove(a);
        let c = arena.insert(Building(3));
        assert_eq!(c.index(), a.index(), "freed slot is reused");
        assert_eq!(c.generation(), a.generation() + 1);
        assert_ne!(a, c);
        assert_eq!(arena.get(a), None);
        assert_eq!(arena.get_mut(a), None);
        assert_eq!(arena.get(c), Some(&Building(3)));
    }

    #[test]
    fn free_list_is_lifo() {
        let mut arena = Arena::new();
        let ids: Vec<_> = (0..4).map(|i| arena.insert(Building(i))).collect();
        arena.remove(ids[1]);
        arena.remove(ids[3]);
        assert_eq!(arena.insert(Building(9)).index(), 3);
        assert_eq!(arena.insert(Building(8)).index(), 1);
        assert_eq!(arena.insert(Building(7)).index(), 4);
        arena.check_integrity().unwrap();
    }

    #[test]
    fn iteration_is_in_index_order() {
        let mut arena = Arena::new();
        let ids: Vec<_> = (0..6).map(|i| arena.insert(Building(i))).collect();
        arena.remove(ids[2]);
        arena.remove(ids[0]);
        let new = arena.insert(Building(100)); // reuses slot 0
        assert_eq!(new.index(), 0);
        let order: Vec<u32> = arena.values().map(|b| b.0).collect();
        assert_eq!(order, vec![100, 1, 3, 4, 5]);
        let id_order: Vec<u32> = arena.ids().map(Id::index).collect();
        assert_eq!(id_order, vec![0, 1, 3, 4, 5]);
        for (_, b) in arena.iter_mut() {
            b.0 += 1;
        }
        assert_eq!(arena.values().map(|b| b.0).collect::<Vec<_>>(), vec![101, 2, 4, 5, 6]);
    }

    #[test]
    fn insert_with_sees_own_id() {
        let mut arena: Arena<(Id<Building>, u32)> = Arena::new();
        // Types differ deliberately: the stored id refers to another arena type.
        let id = arena.insert_with(|own| (Id::from_raw_parts(own.index(), own.generation()), 5));
        let (stored, _) = arena.get(id).unwrap();
        assert_eq!(stored.index(), id.index());
    }

    #[test]
    fn get2_mut_returns_distinct_values() {
        let mut arena = Arena::new();
        let a = arena.insert(Building(1));
        let b = arena.insert(Building(2));
        {
            let (x, y) = arena.get2_mut(b, a).unwrap();
            assert_eq!((x.0, y.0), (2, 1));
            std::mem::swap(&mut x.0, &mut y.0);
        }
        assert_eq!(arena.get(a), Some(&Building(2)));
        assert!(arena.get2_mut(a, a).is_none());
        arena.remove(b);
        assert!(arena.get2_mut(a, b).is_none());
    }

    #[test]
    fn retain_and_clear() {
        let mut arena = Arena::new();
        let ids: Vec<_> = (0..10).map(|i| arena.insert(Building(i))).collect();
        arena.retain(|_, b| b.0 % 2 == 0);
        assert_eq!(arena.len(), 5);
        assert!(arena.contains(ids[4]) && !arena.contains(ids[5]));
        arena.clear();
        assert!(arena.is_empty());
        assert!(ids.iter().all(|id| !arena.contains(*id)));
        arena.check_integrity().unwrap();
        // Slots are reused after clear, with bumped generations.
        let n = arena.insert(Building(0));
        assert!(n.generation() >= 1);
    }

    #[test]
    fn invalid_ids_are_rejected() {
        let mut arena: Arena<Building> = Arena::new();
        let bogus = Id::from_raw_parts(42, 0);
        assert!(arena.get(bogus).is_none());
        assert!(arena.remove(bogus).is_none());
        assert!(arena.id_at(0).is_none());
        let a = arena.insert(Building(1));
        assert_eq!(arena.id_at(0), Some(a));
    }

    #[test]
    fn ids_are_typed_ordered_and_hashable() {
        let a: Id<Building> = Id::from_raw_parts(1, 0);
        let b: Id<Building> = Id::from_raw_parts(1, 1);
        let c: Id<Building> = Id::from_raw_parts(2, 0);
        assert!(a < b && b < c);
        let set: std::collections::BTreeSet<_> = [c, a, b].into_iter().collect();
        assert_eq!(set.into_iter().collect::<Vec<_>>(), vec![a, b, c]);
        let hs: std::collections::HashSet<Id<Citizen>> = [Id::from_raw_parts(0, 0)].into_iter().collect();
        assert_eq!(hs.len(), 1);
        assert_eq!(format!("{a:?}"), "Id(1v0)");
    }

    #[test]
    fn serde_roundtrip_preserves_ids_and_free_list() {
        let mut arena = Arena::new();
        let ids: Vec<_> = (0..5).map(|i| arena.insert(Building(i))).collect();
        arena.remove(ids[1]);
        arena.remove(ids[3]);
        let cfg = bincode::config::standard();
        let bytes = bincode::serde::encode_to_vec(&arena, cfg).unwrap();
        let (mut restored, _): (Arena<Building>, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
        restored.check_integrity().unwrap();
        assert_eq!(restored.len(), 3);
        assert_eq!(restored.get(ids[2]), Some(&Building(2)));
        assert!(restored.get(ids[1]).is_none());
        // Same free-list order as the original.
        assert_eq!(restored.insert(Building(7)), arena.insert(Building(7)));
        let id_bytes = bincode::serde::encode_to_vec(ids[4], cfg).unwrap();
        let (id, _): (Id<Building>, usize) = bincode::serde::decode_from_slice(&id_bytes, cfg).unwrap();
        assert_eq!(id, ids[4]);
    }

    #[test]
    fn integrity_check_detects_damage() {
        let mut arena = Arena::new();
        let a = arena.insert(Building(0));
        arena.insert(Building(1));
        arena.remove(a);
        arena.check_integrity().unwrap();
        let mut broken = arena.clone();
        broken.len = 5;
        assert!(matches!(broken.check_integrity(), Err(ArenaIntegrityError::LenMismatch { .. })));
        let mut broken = arena.clone();
        broken.free_head = Some(1);
        assert!(broken.check_integrity().is_err());
        let mut broken = arena.clone();
        broken.free_head = Some(99);
        assert!(matches!(broken.check_integrity(), Err(ArenaIntegrityError::FreeListOutOfBounds(99))));
        let mut broken = arena;
        broken.free_head = None;
        assert!(matches!(broken.check_integrity(), Err(ArenaIntegrityError::UnreachableFreeSlots { .. })));
    }
}
