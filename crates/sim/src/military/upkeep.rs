//! Upkeep of the soldiers (docs/game_design.md §11): pay status, rations at war, morale,
//! refits, healing, discharge in old age and the repair of damaged structures.
//!
//! * **Pay** is booked hourly with the other money flows (`economy::finance`, ledger
//!   „Militär“: the units' daily upkeep). While the treasury is bankrupt at midnight the pay
//!   counts as unpaid.
//! * **Rations:** while the kingdom is at war, every ready soldier on the map eats
//!   `war_ration_milli` food per day from the storages on top of his household's meals (the
//!   largest food stocks first). Missing rations count as hunger.
//! * **Morale** moves every midnight by `morale_change_per_day` towards a target: the unit's
//!   morale, `morale_unpaid` while unpaid, `morale_hungry` while his household is not fed or
//!   rations are missing, `morale_happiness_per_10` per 10 points of happiness above or below
//!   50, the remembered results of fights (halved every day), war weariness (§10) and
//!   `morale_low_equipment` below half equipment.
//! * **Refit:** a soldier at his post below full equipment gets one unit of each of his unit's
//!   equipment goods from the storages, which restores `refit_permille`.
//! * **Healing:** every hour `heal_per_hour` health (twice that with a hospital in reach of his
//!   home) while not fighting and not away.
//! * Soldiers who grow old leave the service.

use super::recruit;
use crate::defs::{Defs, ResourceCategory, ServiceKind};
use crate::economy::storage;
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::population::{CitizenId, LifeStage, routine};
use crate::state::GameState;

/// Daily pay of all soldiers in milli-gold (negative; operators pay for their crew).
pub(crate) fn daily_pay_milli(state: &GameState, defs: &Defs) -> i64 {
    let gold: i64 = state
        .military
        .soldiers
        .values()
        .filter(|s| s.crew_of.is_none())
        .map(|s| defs.units().get(s.unit.index()).map_or(0, |u| i64::from(u.upkeep_per_day)))
        .sum();
    -gold * crate::economy::MILLI_PER_GOLD
}

/// Midnight: pay status, rations, morale, refits, discharges, structure repair.
pub(crate) fn run_day(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().military;
    super::threats::forget_missing(state);
    super::threats::repair(state, defs);
    if state.military.soldiers.is_empty() {
        state.military.unpaid = false;
        return;
    }
    let was_unpaid = state.military.unpaid;
    state.military.unpaid = state.economy.is_bankrupt();
    if state.military.unpaid && !was_unpaid {
        events.push(SimEvent::Notification(Notification::new(
            Severity::Danger,
            "Der Sold bleibt aus: Die Staatskasse ist leer, die Moral der Soldaten sinkt.",
        ).with_kind(NotificationKind::Treasury)));
    }
    // Old soldiers leave the service.
    let old: Vec<CitizenId> = state
        .military
        .soldiers
        .iter()
        .filter(|(id, s)| s.away.is_none() && state.population.citizen(**id).is_none_or(|c| c.stage() != LifeStage::Adult))
        .map(|(id, _)| *id)
        .collect();
    let mut venues = routine::Venues::default();
    for id in old {
        recruit::discharge(state, id);
        routine::update(state, defs, id, &mut venues);
    }
    // Rations at war.
    let at_war = crate::diplomacy::any_player_war(state);
    let mut short = false;
    if at_war {
        let eaters = state.military.soldiers.values().filter(|s| s.is_ready() && s.away.is_none()).count() as u64;
        let need = (eaters * u64::from(rules.war_ration_milli)).div_ceil(1000);
        let need = u32::try_from(need).unwrap_or(u32::MAX);
        short = eat(state, defs, need) < need;
    }
    // Morale.
    let weariness = crate::diplomacy::war_weariness(state, defs);
    let ids: Vec<CitizenId> = state.military.soldiers.keys().copied().collect();
    for id in ids {
        let Some(c) = state.population.citizen(id) else { continue };
        let fed = state.population.household(c.household()).is_some_and(|h| h.supply().fed);
        let happiness = i32::from(c.happiness());
        let Some(s) = state.military.soldiers.get(&id) else { continue };
        let base = defs.units().get(s.unit.index()).map_or(50, |u| i32::from(u.morale));
        let mut target = base + i32::from(s.battle_morale) + weariness;
        if state.military.unpaid {
            target += i32::from(rules.morale_unpaid);
        }
        if !fed || (short && s.away.is_none() && s.is_ready()) {
            target += i32::from(rules.morale_hungry);
        }
        target += (happiness - 50) / 10 * i32::from(rules.morale_happiness_per_10);
        if s.equipment < 500 {
            target += i32::from(rules.morale_low_equipment);
        }
        let target = target.clamp(0, 100);
        let step = i32::from(rules.morale_change_per_day);
        let now = i32::from(s.morale);
        let next = if now < target { (now + step).min(target) } else { (now - step).max(target) };
        if let Some(s) = state.military.soldiers.get_mut(&id) {
            s.morale = u16::try_from(next).unwrap_or(0);
            s.battle_morale /= 2;
        }
    }
    refit(state, defs);
}

/// Takes `units` food from the storages (largest stocks first); returns what was eaten.
pub(crate) fn eat(state: &mut GameState, defs: &Defs, units: u32) -> u32 {
    take_food(state, defs, units).iter().map(|m| m.amount).sum()
}

/// Takes `units` food from the storages (largest stocks first); returns the goods taken in
/// the order taken.
pub(crate) fn take_food(state: &mut GameState, defs: &Defs, units: u32) -> Vec<crate::defs::ResourceAmount> {
    let mut taken = Vec::new();
    if units == 0 {
        return taken;
    }
    let at = storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b)).map_or(state.world.start, |b| b.entrance());
    let mut foods: Vec<(u32, crate::defs::ResourceId)> = defs
        .resources()
        .iter()
        .filter(|r| r.category == ResourceCategory::Food && r.nutrition > 0.0)
        .map(|r| (state.economy.available(r.id), r.id))
        .filter(|(n, _)| *n > 0)
        .collect();
    foods.sort_by_key(|(n, r)| (std::cmp::Reverse(*n), *r));
    let mut eaten = 0;
    for (have, r) in foods {
        if eaten == units {
            break;
        }
        let got = storage::withdraw(state, defs, r, have.min(units - eaten), at);
        if got > 0 {
            taken.push(crate::defs::ResourceAmount { resource: r, amount: got });
        }
        eaten += got;
    }
    taken
}

/// Soldiers at their post below full equipment are refitted from the storages.
fn refit(state: &mut GameState, defs: &Defs) {
    let rules = &defs.rules().military;
    let ids: Vec<CitizenId> = state
        .military
        .soldiers
        .iter()
        .filter(|(_, s)| s.is_ready() && s.away.is_none() && s.crew_of.is_none() && s.engage.is_none())
        .filter(|(_, s)| s.equipment <= 1000 - rules.refit_permille / 2)
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        let Some(unit) = state.military.soldiers.get(&id).map(|s| s.unit) else { continue };
        let goods: Vec<crate::defs::ResourceAmount> =
            recruit::equipment_goods(defs, unit).into_iter().map(|resource| crate::defs::ResourceAmount { resource, amount: 1 }).collect();
        let at = state.population.citizen(id).map_or(state.world.start, |c| c.motion().pos().tile());
        if !goods.is_empty() && storage::withdraw_materials(state, defs, &goods, at).is_err() {
            continue;
        }
        if let Some(s) = state.military.soldiers.get_mut(&id) {
            s.equipment = (s.equipment + rules.refit_permille).min(1000);
        }
    }
}

/// Hourly healing.
pub(crate) fn run_hour(state: &mut GameState, defs: &Defs, _events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().military;
    let ids: Vec<CitizenId> = state
        .military
        .soldiers
        .iter()
        .filter(|(_, s)| s.engage.is_none() && s.away.is_none())
        .filter(|(_, s)| defs.units().get(s.unit.index()).is_some_and(|u| s.hp < u.hit_points))
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        let hospital = state
            .population
            .citizen(id)
            .and_then(|c| state.population.household(c.household()))
            .is_some_and(|h| h.supply().has_service(ServiceKind::Health));
        let heal = u16::from(rules.heal_per_hour) * if hospital { 2 } else { 1 };
        let Some(s) = state.military.soldiers.get_mut(&id) else { continue };
        let max = defs.units().get(s.unit.index()).map_or(100, |u| u.hit_points);
        s.hp = s.hp.saturating_add(heal).min(max);
    }
}
