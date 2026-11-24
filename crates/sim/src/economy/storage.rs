//! Storage buildings and the kingdom stock view.
//!
//! A *storage* is a finished building whose type has a `StorageDef` (town hall, castle,
//! warehouse, storehouse, trading post). Its contents live in [`Building::inventory`] with a
//! capacity per accepted resource: `capacity_per_resource` × (100 % + researched
//! `StorageBonus`).
//!
//! **Reachability:** a storage takes part in the kingdom stock if it is operational (finished,
//! not paused) and is the main headquarters itself or is road-connected to it (its entrance lies
//! on the same road network as the headquarters' entrance). Without a finished headquarters
//! every operational storage counts. Buildings exchange goods with storages only through
//! carriers ([`crate::logistics`]); the functions here that add or remove goods directly
//! (payments for roads, refunds, cleared wood, trade) are deliberate abstractions of those
//! one-off transfers.

use glam::IVec2;

use super::{Shortfall, Stock};
use crate::buildings::{Building, BuildingId};
use crate::defs::{BuildingDef, BuildingRole, Defs, ResourceAmount, ResourceId, TechEffect};
use crate::event::{Notification, Severity, SimEvent};
use crate::research::Research;
use crate::roads;
use crate::state::GameState;

/// Sum of all researched [`TechEffect::StorageBonus`] percentages.
pub fn storage_bonus_percent(research: &Research, defs: &Defs) -> i32 {
    research
        .researched()
        .filter_map(|t| defs.techs().get(t.index()))
        .flat_map(|t| t.effects.iter())
        .map(|e| match e {
            TechEffect::StorageBonus { percent } => i32::from(*percent),
            _ => 0,
        })
        .sum()
}

/// Capacities of a storage building type per accepted resource, including the bonus.
pub fn storage_capacities(def: &BuildingDef, defs: &Defs, bonus_percent: i32) -> Vec<(ResourceId, u32)> {
    let Some(storage) = def.storage.as_ref() else { return Vec::new() };
    let factor = i64::from((100 + bonus_percent).max(0));
    let capacity = (i64::from(storage.capacity_per_resource) * factor / 100).clamp(0, i64::from(u32::MAX)) as u32;
    defs.resources().iter().filter(|r| storage.accepts.accepts(r.category)).map(|r| (r.id, capacity)).collect()
}

/// Sets the inventory capacities of a finished storage building (no-op for other types).
/// Called on completion; the research phase calls it for every storage when a
/// `StorageBonus` technology is researched.
pub(crate) fn init_storage(building: &mut Building, defs: &Defs, bonus_percent: i32) {
    let Some(def) = defs.buildings().get(building.kind().index()) else { return };
    if def.storage.is_some() {
        building.inventory.set_capacities(storage_capacities(def, defs, bonus_percent));
    } else if !def.sells.is_empty() {
        // A selling building (tavern) keeps a small stock of its goods (docs/game_design.md §5.4).
        let stock = u32::from(defs.rules().shop_stock);
        building.inventory.set_capacities(def.sells.iter().map(|r| (*r, stock)));
    }
}

/// True if `building` is a finished storage that is not paused.
pub fn is_operational_storage(defs: &Defs, building: &Building) -> bool {
    building.is_operational() && defs.buildings().get(building.kind().index()).is_some_and(|d| d.storage.is_some())
}

/// The main headquarters: the finished headquarters with the lowest id.
pub fn main_headquarters(state: &GameState, defs: &Defs) -> Option<BuildingId> {
    state.buildings.iter().find_map(|(id, b)| {
        let def = defs.buildings().get(b.kind().index())?;
        (def.role == BuildingRole::Headquarters && b.is_complete()).then_some(id)
    })
}

/// Operational storages that form the kingdom stock, in ascending id order (see the module
/// documentation).
pub fn reachable_storages(state: &GameState, defs: &Defs) -> Vec<BuildingId> {
    let hq = main_headquarters(state, defs);
    let hq_component = hq.and_then(|hq| roads::building_component(&state.world, &state.roads, &state.buildings, hq));
    state
        .buildings
        .iter()
        .filter(|(_, b)| is_operational_storage(defs, b))
        .filter(|(id, _)| match hq {
            None => true,
            Some(hq) if hq == *id => true,
            Some(_) => {
                hq_component.is_some()
                    && roads::building_component(&state.world, &state.roads, &state.buildings, *id) == hq_component
            }
        })
        .map(|(id, _)| id)
        .collect()
}

/// Recomputes the cached views of [`Economy`](super::Economy): stock and capacity of the
/// reachable storages, and the construction material still owed to sites.
pub(crate) fn refresh_stock(state: &mut GameState, defs: &Defs) {
    let mut stock = Stock::new(defs);
    let mut capacity = Stock::new(defs);
    for id in reachable_storages(state, defs) {
        if let Some(b) = state.buildings.get(id) {
            for m in b.inventory.iter() {
                stock.add(m.resource, m.amount);
            }
            for (r, c) in b.inventory.capacities() {
                capacity.add(r, c);
            }
        }
    }
    let mut committed = Stock::new(defs);
    for (id, b) in state.buildings.iter() {
        let Some(site) = b.construction_site() else { continue };
        for m in site.materials_required() {
            let owed = m
                .amount
                .saturating_sub(site.delivered(m.resource))
                .saturating_sub(state.logistics.in_transit_to(id, m.resource));
            committed.add(m.resource, owed);
        }
    }
    state.economy.set_views(stock, capacity, committed);
}

/// Reachable storages ordered by distance of their footprint to `near` (ties by id).
fn storages_near(state: &GameState, defs: &Defs, near: IVec2) -> Vec<BuildingId> {
    let mut list: Vec<(i32, BuildingId)> = reachable_storages(state, defs)
        .into_iter()
        .filter_map(|id| state.buildings.get(id).map(|b| (b.rect().distance_to(near), id)))
        .collect();
    list.sort();
    list.into_iter().map(|(_, id)| id).collect()
}

/// Removes up to `amount` units of `resource` from the reachable storages, nearest to `near`
/// first; returns the units removed. Refreshes the stock view.
pub(crate) fn withdraw(state: &mut GameState, defs: &Defs, resource: ResourceId, amount: u32, near: IVec2) -> u32 {
    let mut taken = 0;
    for id in storages_near(state, defs, near) {
        if taken == amount {
            break;
        }
        if let Some(b) = state.buildings.get_mut(id) {
            taken += b.inventory.remove(resource, amount - taken);
        }
    }
    refresh_stock(state, defs);
    taken
}

/// Removes all `materials` from the reachable storages, or nothing if any part is not
/// available (stock minus committed material).
pub(crate) fn withdraw_materials(
    state: &mut GameState,
    defs: &Defs,
    materials: &[ResourceAmount],
    near: IVec2,
) -> Result<(), Vec<Shortfall>> {
    let missing: Vec<Shortfall> = materials
        .iter()
        .filter(|m| state.economy.available(m.resource) < m.amount)
        .map(|m| Shortfall::Material { resource: m.resource, needed: m.amount, available: state.economy.available(m.resource) })
        .collect();
    if !missing.is_empty() {
        return Err(missing);
    }
    for m in materials {
        withdraw(state, defs, m.resource, m.amount, near);
    }
    Ok(())
}

/// Stores up to `amount` units in the reachable storages with free space, nearest to `near`
/// first; returns the units stored. Room that transport jobs already claimed for goods on the
/// way (`Logistics::incoming`) is used only when no storage has unclaimed room left, so a
/// carrier rarely finds its target filled by other inflows (cleared trees, purchases,
/// refunds). Refreshes the stock view.
pub(crate) fn deposit(state: &mut GameState, defs: &Defs, resource: ResourceId, amount: u32, near: IVec2) -> u32 {
    let mut stored = 0;
    let storages = storages_near(state, defs, near);
    for respect_claims in [true, false] {
        for &id in &storages {
            if stored == amount {
                break;
            }
            let claimed = if respect_claims { state.logistics.incoming(id, resource) } else { 0 };
            if let Some(b) = state.buildings.get_mut(id) {
                let room = b.inventory.free_space(resource).saturating_sub(claimed);
                stored += b.inventory.add(resource, (amount - stored).min(room));
            }
        }
    }
    refresh_stock(state, defs);
    stored
}

/// Stores `items` like [`deposit`]; what does not fit is lost and reported with
/// [`SimEvent::ResourcesLost`] and a warning at `near`.
pub(crate) fn deposit_or_lose(
    state: &mut GameState,
    defs: &Defs,
    items: &[ResourceAmount],
    near: IVec2,
    events: &mut Vec<SimEvent>,
) {
    for m in items.iter().filter(|m| m.amount > 0) {
        let stored = deposit(state, defs, m.resource, m.amount, near);
        if stored < m.amount {
            report_loss(defs, m.resource, m.amount - stored, near, events);
        }
    }
}

/// Emits the events for goods that had to be discarded.
pub(crate) fn report_loss(defs: &Defs, resource: ResourceId, amount: u32, at: IVec2, events: &mut Vec<SimEvent>) {
    let name = defs.resources().get(resource.index()).map_or_else(|| resource.to_string(), |r| r.name.clone());
    events.push(SimEvent::ResourcesLost { resource, amount, position: at });
    events.push(SimEvent::Notification(
        Notification::new(Severity::Warning, format!("Kein Lagerplatz: {amount} {name} {} verloren.", if amount == 1 { "ging" } else { "gingen" }))
            .at(at),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{defs, flat_state, spawn};

    fn res(key: &str) -> ResourceId {
        defs().resource_id(key).unwrap()
    }

    #[test]
    fn capacities_follow_filter_and_research() {
        let d = defs();
        let store = d.building_by_key("storehouse").unwrap();
        let caps = storage_capacities(store, d, 0);
        assert!(caps.iter().all(|(r, c)| d.resource(*r).category == crate::defs::ResourceCategory::Food && *c == 150));
        assert!(!caps.iter().any(|(r, _)| *r == res("wood")));
        let hall = d.building_by_key("town_hall").unwrap();
        assert_eq!(storage_capacities(hall, d, 50).first().map(|(_, c)| *c), Some(300));
        assert!(storage_capacities(d.building_by_key("sawmill").unwrap(), d, 0).is_empty());
        let mut research = Research::new();
        assert_eq!(storage_bonus_percent(&research, d), 0);
        let tech = d.techs().iter().find(|t| t.effects.iter().any(|e| matches!(e, TechEffect::StorageBonus { .. }))).unwrap();
        research.mark_researched(tech.id);
        assert!(storage_bonus_percent(&research, d) > 0);
    }

    #[test]
    fn stock_view_counts_only_storages_connected_to_the_headquarters() {
        let d = defs();
        let mut state = flat_state(48);
        let depot = crate::test_support::depot(&state);
        let wood = res("wood");
        let before = state.economy().stock().get(wood);
        assert!(before > 0, "the fixture depot counts while there is no headquarters");
        // A headquarters without roads: only the headquarters itself counts.
        let hall = spawn(&mut state, "town_hall", 10, 10);
        state.buildings.get_mut(hall).unwrap().inventory.add(wood, 7);
        refresh_stock(&mut state, d);
        assert_eq!(state.economy().stock().get(wood), 7);
        assert_eq!(reachable_storages(&state, d), vec![hall]);
        // Connecting the depot by road adds it.
        let (a, b) = (state.building(hall).unwrap().entrance(), state.building(depot).unwrap().entrance());
        crate::test_support::lay_path(&mut state, a, b);
        refresh_stock(&mut state, d);
        assert_eq!(reachable_storages(&state, d), vec![depot, hall]);
        assert_eq!(state.economy().stock().get(wood), before + 7);
        // A paused storage drops out.
        state.buildings.get_mut(depot).unwrap().paused = true;
        refresh_stock(&mut state, d);
        assert_eq!(state.economy().stock().get(wood), 7);
        assert_eq!(state.economy().capacity().get(wood), 200);
    }

    #[test]
    fn withdraw_and_deposit_prefer_near_storages() {
        let d = defs();
        let mut state = flat_state(48);
        let depot = crate::test_support::depot(&state);
        let near = spawn(&mut state, "warehouse", 5, 5);
        refresh_stock(&mut state, d);
        let stone = res("stone");
        assert_eq!(deposit(&mut state, d, stone, 30, IVec2::new(4, 4)), 30);
        assert_eq!(state.building(near).unwrap().inventory().get(stone), 30);
        let depot_stone = state.building(depot).unwrap().inventory().get(stone);
        assert_eq!(withdraw(&mut state, d, stone, 40, IVec2::new(4, 4)), 40);
        assert_eq!(state.building(near).unwrap().inventory().get(stone), 0);
        assert_eq!(state.building(depot).unwrap().inventory().get(stone), depot_stone - 10);
        // All-or-nothing withdrawal.
        let total = state.economy().stock().get(stone);
        let too_much = [ResourceAmount { resource: stone, amount: total + 1 }];
        assert!(withdraw_materials(&mut state, d, &too_much, IVec2::ZERO).is_err());
        assert_eq!(state.economy().stock().get(stone), total);
        // Overflow is lost and reported.
        let mut events = Vec::new();
        let capacity = state.economy().capacity().get(stone);
        let free = capacity - total;
        deposit_or_lose(&mut state, d, &[ResourceAmount { resource: stone, amount: free + 5 }], IVec2::ZERO, &mut events);
        assert!(events.iter().any(|e| matches!(e, SimEvent::ResourcesLost { amount: 5, .. })));
        assert_eq!(state.economy().stock().get(stone), capacity);
    }
}
