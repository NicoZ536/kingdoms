//! Recruitment, training and discharge (docs/game_design.md §11).
//!
//! `Recruit { building, unit, count }` at a finished military building that trains the unit:
//! the unit must be unlocked (research `UnlockUnit`), the building has `military.training_slots`
//! places for recruits at once, cavalry needs horses (four riders per finished cattle farm).
//! It costs the unit's gold (ledger „Militär“) and equipment from the storages, and
//! `population` adult citizens per unit: unemployed adults first, then workers, nearest to the
//! building first (nobody who is carrying goods). They leave their job — the workforce shrinks
//! — and walk to the building, where they train for the unit's training time; trainers (the
//! building's workers) speed it up (without any: `training_without_staff_percent`). Training
//! happens only while the recruit is inside, so it pauses at night. A siege engine's first
//! recruit is its operator, the others its crew.
//!
//! Discharged soldiers (`DismissSoldiers`, old age, a training building that is gone) return
//! to civilian life and look for work again; equipment is not returned.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{Soldier, SoldierStage, count_text};
use crate::buildings::BuildingId;
use crate::defs::{Defs, ResourceCategory, UnitTypeId};
use crate::economy::{LedgerCategory, storage};
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::population::{CitizenId, routine};
use crate::state::GameState;

/// Why a recruitment is not possible (German `Display` text for the player).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum RecruitProblem {
    /// The building does not exist.
    #[error("Dieses Gebäude gibt es nicht.")]
    UnknownBuilding,
    /// The unit type does not exist.
    #[error("Diese Einheit gibt es nicht.")]
    UnknownUnit,
    /// The building is not finished or not working.
    #[error("Das Gebäude „{building}“ ist noch nicht bereit.")]
    NotReady {
        /// Building name.
        building: String,
    },
    /// The building does not train this unit.
    #[error("{unit} werden nicht hier ausgebildet, sondern im Gebäude „{trained_at}“.")]
    WrongBuilding {
        /// Unit name (plural).
        unit: String,
        /// Name of the building that trains it.
        trained_at: String,
    },
    /// The unit is not researched yet.
    #[error("{unit} erfordert die Forschung „{tech}“.")]
    Locked {
        /// Unit name.
        unit: String,
        /// Technology name.
        tech: String,
    },
    /// The count is outside 1 ..= training slots.
    #[error("Es können 1 bis {max} Rekruten auf einmal ausgebildet werden.")]
    InvalidCount {
        /// Training slots.
        max: u8,
    },
    /// Not enough free training places.
    #[error("Das Gebäude „{building}“ hat {}.", match *.free {
        0 => "keinen freien Ausbildungsplatz mehr".to_owned(),
        1 => "nur noch 1 freien Ausbildungsplatz".to_owned(),
        n => format!("nur noch {n} freie Ausbildungsplätze"),
    })]
    NoSlots {
        /// Building name.
        building: String,
        /// Free places.
        free: u32,
    },
    /// Not enough gold.
    #[error("{}.", crate::report::missing_gold(i64::from(*.missing)))]
    NoGold {
        /// Missing gold.
        missing: u32,
    },
    /// Not enough equipment in the storages.
    #[error("{} in den Lagern.", crate::report::missing_goods(*.missing, .resource))]
    NoMaterials {
        /// Resource name.
        resource: String,
        /// Missing units.
        missing: u32,
    },
    /// Not enough adults to recruit.
    #[error("{}", if *.available == 1 {
        format!("Es steht nur 1 von {} Erwachsenen für den Dienst bereit.", .needed)
    } else {
        format!("Es stehen nur {} von {} Erwachsenen für den Dienst bereit.", .available, .needed)
    })]
    NoRecruits {
        /// Adults needed.
        needed: u32,
        /// Adults available.
        available: u32,
    },
    /// Not enough horses (cavalry).
    #[error("Es fehlen Pferde: {have} von {needed} Reitern sind versorgt (je {building} {per} Reiter).")]
    NoSupport {
        /// Support building name.
        building: String,
        /// Riders supported.
        have: u32,
        /// Riders needed.
        needed: u32,
        /// Riders per building.
        per: u16,
    },
}

fn building_name(defs: &Defs, kind: crate::defs::BuildingTypeId) -> String {
    defs.buildings().get(kind.index()).map_or_else(|| "Gebäude".to_owned(), |d| d.name.clone())
}

/// Recruits training at `building` now (operators and crews alike count as one place each).
pub(crate) fn in_training(state: &GameState, building: BuildingId) -> u32 {
    let n = state.military.soldiers.values().filter(|s| matches!(s.stage, SoldierStage::Training { building: b, .. } if b == building)).count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Adults who could be recruited now, best candidates first (unemployed, then workers; nearest
/// to `building` first; nobody carrying goods).
pub(crate) fn candidates(state: &GameState, building: BuildingId) -> Vec<CitizenId> {
    let Some(rect) = state.buildings.get(building).map(|b| b.rect()) else { return Vec::new() };
    let mut list: Vec<(bool, i32, CitizenId)> = state
        .population
        .citizens()
        .filter(|(_, c)| c.can_take_job() && c.carry().is_none())
        .map(|(id, c)| {
            let home = state
                .population
                .household(c.household())
                .and_then(|h| h.home())
                .and_then(|b| state.buildings.get(b))
                .map_or(c.motion().pos().tile(), |b| b.entrance());
            (c.job().is_some(), rect.distance_to(home), id)
        })
        .collect();
    list.sort();
    list.into_iter().map(|(.., id)| id).collect()
}

/// Riders the finished support buildings of `unit` can keep, and the riders there are.
fn support(state: &GameState, defs: &Defs, unit: UnitTypeId) -> Option<(u32, u32, u16, String)> {
    let def = defs.units().get(unit.index())?;
    let s = def.support?;
    let buildings = state.buildings.iter().filter(|(_, b)| b.kind() == s.building && b.is_complete()).count();
    let have = u32::try_from(buildings).unwrap_or(0) * u32::from(s.units_per_building);
    let riders = state.military.soldiers.values().filter(|x| x.unit == unit && x.crew_of.is_none()).count();
    Some((have, u32::try_from(riders).unwrap_or(0), s.units_per_building, building_name(defs, s.building)))
}

/// Why `count` units of `unit` cannot be recruited at `building` now, or `None`.
pub fn recruit_problem(state: &GameState, defs: &Defs, building: BuildingId, unit: UnitTypeId, count: u8) -> Option<RecruitProblem> {
    let rules = &defs.rules().military;
    let Some(b) = state.buildings.get(building) else { return Some(RecruitProblem::UnknownBuilding) };
    let Some(def) = defs.units().get(unit.index()) else { return Some(RecruitProblem::UnknownUnit) };
    let bname = building_name(defs, b.kind());
    // Upgraded training buildings (Garnison, Schützenturm, Gestüt) train what they trained before.
    if !defs.buildings().get(b.kind().index()).is_some_and(|d| d.trains.contains(&unit)) {
        return Some(RecruitProblem::WrongBuilding { unit: def.plural.clone(), trained_at: building_name(defs, def.trained_at) });
    }
    if !state.research.unit_unlocked(defs, unit) {
        let tech = def.unlocked_by.map_or_else(String::new, |t| crate::research::tech_name(defs, t));
        return Some(RecruitProblem::Locked { unit: def.name.clone(), tech });
    }
    if !b.is_operational() {
        return Some(RecruitProblem::NotReady { building: bname });
    }
    if count == 0 || count > rules.training_slots {
        return Some(RecruitProblem::InvalidCount { max: rules.training_slots });
    }
    let places = u32::from(count) * u32::from(def.population);
    let free = u32::from(rules.training_slots).saturating_sub(in_training(state, building));
    if places > free {
        return Some(RecruitProblem::NoSlots { building: bname, free });
    }
    if let Some((have, riders, per, name)) = support(state, defs, unit)
        && riders + u32::from(count) > have
    {
        return Some(RecruitProblem::NoSupport { building: name, have, needed: riders + u32::from(count), per });
    }
    let gold = def.cost.gold * u32::from(count);
    if i64::from(gold) > state.economy.treasury() {
        let missing = u32::try_from(i64::from(gold) - state.economy.treasury().max(0)).unwrap_or(u32::MAX);
        return Some(RecruitProblem::NoGold { missing });
    }
    for m in &def.cost.materials {
        let need = m.amount * u32::from(count);
        let have = state.economy.available(m.resource);
        if have < need {
            return Some(RecruitProblem::NoMaterials { resource: defs.resource(m.resource).name.clone(), missing: need - have });
        }
    }
    let available = u32::try_from(candidates(state, building).len()).unwrap_or(0);
    if available < places {
        return Some(RecruitProblem::NoRecruits { needed: places, available });
    }
    None
}

/// `Recruit`: pays, enlists the recruits and sends them to the building.
pub(crate) fn recruit(
    state: &mut GameState,
    defs: &Defs,
    building: BuildingId,
    unit: UnitTypeId,
    count: u8,
    events: &mut Vec<SimEvent>,
) -> Result<(), RecruitProblem> {
    if let Some(p) = recruit_problem(state, defs, building, unit, count) {
        return Err(p);
    }
    let def = defs.unit(unit);
    let at = state.buildings.get(building).map_or(state.world.start, |b| b.entrance());
    let materials: Vec<_> = def
        .cost
        .materials
        .iter()
        .map(|m| crate::defs::ResourceAmount { resource: m.resource, amount: m.amount * u32::from(count) })
        .collect();
    if storage::withdraw_materials(state, defs, &materials, at).is_err() {
        let first = materials.first().map_or_else(String::new, |m| defs.resource(m.resource).name.clone());
        return Err(RecruitProblem::NoMaterials { resource: first, missing: 1 });
    }
    state.economy.book_gold(LedgerCategory::Military, -i64::from(def.cost.gold * u32::from(count)));
    let mut pool = candidates(state, building).into_iter();
    let now = state.clock.tick;
    let mut enlisted = Vec::new();
    for _ in 0..count {
        let mut lead: Option<CitizenId> = None;
        for _ in 0..def.population {
            let Some(id) = pool.next() else { break };
            crate::population::release_job(state, id);
            if let Some(c) = state.population.citizens.get_mut(id) {
                c.soldier = Some(unit);
            }
            state.military.soldiers.insert(
                id,
                Soldier {
                    unit,
                    stage: SoldierStage::Training { building, done_milli: 0 },
                    company: None,
                    post: Some(building),
                    hp: def.hit_points,
                    morale: def.morale.min(100),
                    equipment: 1000,
                    experience: 0,
                    kills: 0,
                    battle_morale: 0,
                    crew_of: lead,
                    duty: None,
                    engage: None,
                    waypoint: 0,
                    away: None,
                    recruited_tick: now,
                },
            );
            lead.get_or_insert(id);
            enlisted.push(id);
        }
    }
    state.workforce.mark_dirty();
    state.workforce.refresh_counts(&state.population);
    events.push(SimEvent::SoldiersRecruited { building, unit, count });
    let trainer = state.buildings.get(building).map_or(def.trained_at, |b| b.kind());
    let text = format!("{} in Ausbildung ({}).", count_text(u32::from(count), &def.name, &def.plural), building_name(defs, trainer));
    events.push(SimEvent::Notification(Notification::new(Severity::Info, text).with_kind(NotificationKind::Military).at(at).for_building(building)));
    super::orders::assign_duties(state, defs);
    let mut venues = routine::Venues::default();
    for id in enlisted {
        routine::update(state, defs, id, &mut venues);
    }
    Ok(())
}

/// Every tick: recruits inside their building make progress; the trained become ready.
pub(crate) fn run_training(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().military;
    let mut done: Vec<(BuildingId, UnitTypeId, CitizenId)> = Vec::new();
    let mut lost: Vec<CitizenId> = Vec::new();
    let ids: Vec<CitizenId> =
        state.military.soldiers.iter().filter(|(_, s)| matches!(s.stage, SoldierStage::Training { .. }) && s.crew_of.is_none()).map(|(id, _)| *id).collect();
    for id in ids {
        let Some(s) = state.military.soldiers.get(&id) else { continue };
        let SoldierStage::Training { building, done_milli } = s.stage else { continue };
        let Some(b) = state.buildings.get(building) else {
            lost.push(id);
            continue;
        };
        let inside = state.population.citizen(id).is_some_and(|c| c.motion().inside() == Some(building));
        if !inside || !b.is_operational() {
            continue;
        }
        let def = defs.buildings().get(b.kind().index());
        let workers = def.map_or(0, |d| u32::from(d.workers));
        let base = u64::from(rules.training_without_staff_percent) * 10;
        let staff = if workers == 0 { 1000 } else { base + (1000 - base) * u64::from(b.trade_staff()).min(u64::from(workers)) / u64::from(workers) };
        let total = defs.units().get(s.unit.index()).map_or(1, |u| u.training_ticks.max(1)) * 1000;
        let next = done_milli + staff;
        let unit = s.unit;
        if let Some(s) = state.military.soldiers.get_mut(&id) {
            if next >= total {
                s.stage = SoldierStage::Ready;
                done.push((building, unit, id));
            } else {
                s.stage = SoldierStage::Training { building, done_milli: next };
            }
        }
    }
    // Crews are ready with their engine.
    for (_, _, lead) in &done {
        for s in state.military.soldiers.values_mut().filter(|s| s.crew_of == Some(*lead)) {
            s.stage = SoldierStage::Ready;
        }
    }
    for id in lost {
        if let Some(c) = state.population.citizen(id) {
            let name = state.population.full_name(id).unwrap_or_default();
            let at = c.motion().pos().tile();
            events.push(SimEvent::Notification(
                Notification::new(Severity::Warning, format!("Die Ausbildung von {name} ist abgebrochen: das Ausbildungsgebäude ist fort.")).with_kind(NotificationKind::Military).at(at),
            ));
        }
        discharge(state, id);
    }
    if done.is_empty() {
        return;
    }
    let mut groups: Vec<(BuildingId, UnitTypeId, u32)> = Vec::new();
    for (b, u, id) in &done {
        events.push(SimEvent::SoldierTrained { citizen: *id, unit: *u });
        match groups.iter_mut().find(|(gb, gu, _)| gb == b && gu == u) {
            Some(g) => g.2 += 1,
            None => groups.push((*b, *u, 1)),
        }
    }
    for (b, u, n) in groups {
        let at = state.buildings.get(b).map_or(state.world.start, |bb| bb.entrance());
        let text = format!("Ausbildung abgeschlossen: {} bereit.", super::unit_count_text(defs, u, n));
        events.push(SimEvent::Notification(Notification::new(Severity::Good, text).with_kind(NotificationKind::Military).at(at).for_building(b)));
    }
    super::orders::assign_duties(state, defs);
}

/// A soldier returns to civilian life (his crew with him).
pub(crate) fn discharge(state: &mut GameState, id: CitizenId) {
    super::forget_citizen(state, id);
    state.workforce.mark_dirty();
    state.workforce.refresh_counts(&state.population);
}

/// `DismissSoldiers`: the soldiers return to civilian life.
pub(crate) fn dismiss(state: &mut GameState, defs: &Defs, soldiers: &[CitizenId], events: &mut Vec<SimEvent>) -> Result<(), String> {
    if soldiers.is_empty() {
        return Err("Es wurden keine Soldaten gewählt.".to_owned());
    }
    for id in soldiers {
        let Some(s) = state.military.soldiers.get(id) else { return Err("Dieser Bewohner ist kein Soldat.".to_owned()) };
        if s.away.is_some() {
            return Err("Soldaten auf einem Feldzug können nicht entlassen werden.".to_owned());
        }
        if s.crew_of.is_some() {
            return Err("Die Bedienmannschaft wird mit ihrem Belagerungsgerät entlassen.".to_owned());
        }
    }
    let mut venues = routine::Venues::default();
    for id in soldiers {
        let crew: Vec<CitizenId> =
            state.military.soldiers.iter().filter(|(_, s)| s.crew_of == Some(*id)).map(|(c, _)| *c).collect();
        discharge(state, *id);
        events.push(SimEvent::SoldierDismissed { citizen: *id });
        for c in std::iter::once(*id).chain(crew) {
            routine::update(state, defs, c, &mut venues);
        }
    }
    Ok(())
}

/// The equipment goods of a unit: the Goods among its cost materials (one unit each refits it).
pub fn equipment_goods(defs: &Defs, unit: UnitTypeId) -> Vec<crate::defs::ResourceId> {
    defs.units()
        .get(unit.index())
        .map(|u| {
            u.cost
                .materials
                .iter()
                .filter(|m| defs.resource(m.resource).category == ResourceCategory::Goods)
                .map(|m| m.resource)
                .collect()
        })
        .unwrap_or_default()
}
