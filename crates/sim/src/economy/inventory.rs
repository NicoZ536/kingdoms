//! Goods held by one building, with a capacity per resource.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::defs::{ResourceAmount, ResourceId};

/// Goods held by a building (storage contents, production input or output buffer) together
/// with the capacity per resource.
///
/// A resource without a capacity entry has capacity 0: the inventory does not accept it, but
/// may still hold units of it (e.g. inputs of a recipe that is no longer selected); such
/// surplus is collected by carriers. Amounts may exceed the capacity after a capacity
/// reduction; nothing new is accepted until the amount drops below it again.
///
/// Both maps only store positive values, so equal contents always compare and serialize
/// identically.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    items: BTreeMap<ResourceId, u32>,
    capacities: BTreeMap<ResourceId, u32>,
}

impl Inventory {
    /// Empty inventory that accepts nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty inventory with the given capacities (zero capacities are ignored; a resource
    /// listed twice keeps the last value).
    pub fn with_capacities(capacities: impl IntoIterator<Item = (ResourceId, u32)>) -> Self {
        let mut inventory = Self::new();
        inventory.set_capacities(capacities);
        inventory
    }

    /// Units of `resource`.
    pub fn get(&self, resource: ResourceId) -> u32 {
        self.items.get(&resource).copied().unwrap_or(0)
    }

    /// Capacity for `resource` (0 = not accepted).
    pub fn capacity(&self, resource: ResourceId) -> u32 {
        self.capacities.get(&resource).copied().unwrap_or(0)
    }

    /// True if the inventory accepts `resource` at all.
    pub fn accepts(&self, resource: ResourceId) -> bool {
        self.capacity(resource) > 0
    }

    /// Units of `resource` that still fit.
    pub fn free_space(&self, resource: ResourceId) -> u32 {
        self.capacity(resource).saturating_sub(self.get(resource))
    }

    /// Units above the capacity (held but no longer accepted).
    pub fn surplus(&self, resource: ResourceId) -> u32 {
        self.get(resource).saturating_sub(self.capacity(resource))
    }

    /// All held resources in id order (positive amounts only).
    pub fn iter(&self) -> impl Iterator<Item = ResourceAmount> + '_ {
        self.items.iter().map(|(&resource, &amount)| ResourceAmount { resource, amount })
    }

    /// All accepted resources with their capacity, in id order.
    pub fn capacities(&self) -> impl Iterator<Item = (ResourceId, u32)> + '_ {
        self.capacities.iter().map(|(&r, &c)| (r, c))
    }

    /// True if nothing is held.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Sum of all units.
    pub fn total(&self) -> u64 {
        self.items.values().map(|a| u64::from(*a)).sum()
    }

    /// Adds up to `amount` units, limited by the free space; returns the units stored.
    pub(crate) fn add(&mut self, resource: ResourceId, amount: u32) -> u32 {
        let stored = amount.min(self.free_space(resource));
        if stored > 0 {
            let slot = self.items.entry(resource).or_insert(0);
            *slot = slot.saturating_add(stored);
        }
        stored
    }

    /// Adds `amount` units of an accepted resource even beyond the capacity (a carrier who
    /// finds no room anywhere stacks its load in the storage; the surplus is used up first and
    /// blocks further deliveries until it is gone). Returns the units stored: `amount`, or 0
    /// if the resource is not accepted at all.
    pub(crate) fn add_beyond_capacity(&mut self, resource: ResourceId, amount: u32) -> u32 {
        if amount == 0 || !self.accepts(resource) {
            return 0;
        }
        let slot = self.items.entry(resource).or_insert(0);
        *slot = slot.saturating_add(amount);
        amount
    }

    /// Removes up to `amount` units; returns the units removed.
    pub(crate) fn remove(&mut self, resource: ResourceId, amount: u32) -> u32 {
        let Some(slot) = self.items.get_mut(&resource) else { return 0 };
        let taken = (*slot).min(amount);
        *slot -= taken;
        if *slot == 0 {
            self.items.remove(&resource);
        }
        taken
    }

    /// Replaces all capacities (contents are kept, also above the new limits).
    pub(crate) fn set_capacities(&mut self, capacities: impl IntoIterator<Item = (ResourceId, u32)>) {
        self.capacities.clear();
        for (resource, capacity) in capacities {
            if capacity > 0 {
                self.capacities.insert(resource, capacity);
            } else {
                self.capacities.remove(&resource);
            }
        }
    }

    /// Empties the inventory and returns its contents in id order (capacities are kept).
    pub(crate) fn take_all(&mut self) -> Vec<ResourceAmount> {
        std::mem::take(&mut self.items).into_iter().map(|(resource, amount)| ResourceAmount { resource, amount }).collect()
    }

    /// True if no zero entries are stored (invariant of loaded data).
    pub(crate) fn is_canonical(&self) -> bool {
        self.items.values().all(|a| *a > 0) && self.capacities.values().all(|c| *c > 0)
    }

    /// Highest resource index referenced by contents or capacities.
    pub(crate) fn max_resource_index(&self) -> Option<usize> {
        let a = self.items.keys().next_back().map(|r| r.index());
        let b = self.capacities.keys().next_back().map(|r| r.index());
        a.max(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::defs;

    fn res(key: &str) -> ResourceId {
        defs().resource_id(key).unwrap()
    }

    #[test]
    fn capacities_limit_what_is_stored() {
        let (wood, stone) = (res("wood"), res("stone"));
        let mut inv = Inventory::with_capacities([(wood, 10), (stone, 0)]);
        assert!(inv.accepts(wood) && !inv.accepts(stone));
        assert_eq!(inv.add(wood, 7), 7);
        assert_eq!(inv.add(wood, 7), 3, "only the free space is used");
        assert_eq!(inv.add(stone, 5), 0);
        assert_eq!((inv.get(wood), inv.free_space(wood), inv.total()), (10, 0, 10));
        assert_eq!(inv.remove(wood, 4), 4);
        assert_eq!(inv.remove(wood, 40), 6);
        assert!(inv.is_empty());
        assert_eq!(inv.iter().count(), 0, "no zero entries remain");
        assert!(inv.is_canonical());
    }

    #[test]
    fn shrinking_capacity_keeps_contents_as_surplus() {
        let (wood, planks) = (res("wood"), res("planks"));
        let mut inv = Inventory::with_capacities([(wood, 8)]);
        inv.add(wood, 6);
        inv.set_capacities([(planks, 8)]);
        assert_eq!(inv.get(wood), 6);
        assert_eq!(inv.surplus(wood), 6);
        assert_eq!(inv.add(wood, 1), 0);
        assert_eq!(inv.capacities().collect::<Vec<_>>(), vec![(planks, 8)]);
        assert_eq!(inv.take_all(), vec![ResourceAmount { resource: wood, amount: 6 }]);
        assert!(inv.is_empty() && inv.accepts(planks));
        assert_eq!(inv.max_resource_index(), Some(planks.index()));
    }
}
