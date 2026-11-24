//! Houses and their households (docs/game_design.md §5.1, §6).
//!
//! * **Homes:** every finished house (also while it is upgraded) is a [`Home`]. Houses that are
//!   demolished, moved or lose capacity (downgrade) send households out; homeless households
//!   (largest first) move into the house that fits them best. Homeless households camp at the
//!   main headquarters. A homeless household larger than any house the player can build
//!   splits up (adult children and the oldest children move out first), so nobody stays
//!   homeless for good.
//! * **Requirements:** a tier's `housing.requires` (needs, services, goods) are judged from the
//!   residents: each required need must average at least `need_met_threshold`, each service
//!   must reach the house, and every household of the house must have one of the accepted
//!   goods in its pantry. A house without residents meets nothing (it cannot be upgraded) and
//!   is never downgraded.
//! * **Downgrade:** once an hour every house of tier 2 or higher checks its own tier; after
//!   `downgrade_after_hours` consecutive unmet hours it falls back to the previous tier
//!   (smaller footprints shrink around the entrance), and households beyond the new capacity
//!   become homeless.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::buildings::{BuildingId, BuildingPhase};
use crate::defs::{BuildingTypeId, Defs, HousingRequirements, NeedKind, ResourceId, ServiceKind};
use crate::event::{Notification, Severity, SimEvent};
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;

use super::{CENTI, CitizenId, Household, HouseholdId, LifeStage, Notice, ServiceIndex};

/// Capacity of a house including research bonuses (0 for other buildings).
pub(crate) fn home_capacity(state: &GameState, defs: &Defs, building: BuildingId) -> u16 {
    state
        .buildings
        .get(building)
        .and_then(|b| defs.buildings().get(b.kind().index()))
        .and_then(|d| d.housing.as_ref())
        .map_or(0, |h| crate::workforce::housing_capacity(state, defs, h.capacity))
}

/// Free places of every home, in id order.
pub(crate) fn free_places(state: &GameState, defs: &Defs) -> Vec<(BuildingId, u16)> {
    state
        .population
        .homes
        .iter()
        .map(|(id, home)| (*id, home_capacity(state, defs, *id).saturating_sub(state.population.home_residents(home))))
        .collect()
}

/// The housing tier a house falls back to: the type whose upgrade leads to `kind`.
pub fn previous_tier(defs: &Defs, kind: BuildingTypeId) -> Option<BuildingTypeId> {
    defs.buildings()
        .iter()
        .find(|d| d.housing.is_some() && d.upgrade.as_ref().is_some_and(|u| u.to == kind))
        .map(|d| d.id)
}

fn set_home(state: &mut GameState, household: HouseholdId, home: Option<BuildingId>, events: &mut Vec<SimEvent>) {
    let Some(h) = state.population.households.get_mut(household) else { return };
    if h.home == home {
        return;
    }
    if let Some(old) = h.home
        && let Some(hm) = state.population.homes.get_mut(&old)
    {
        hm.households.retain(|x| *x != household);
    }
    h.home = home;
    if let Some(new) = home {
        state.population.homes.entry(new).or_default().households.push(household);
    }
    events.push(SimEvent::HouseholdMoved { household, home });
}

/// Moves a newly arrived household into the house that fits it best when every house keeps
/// `reserve` places free (for births and marriages); returns true if it found one.
pub(crate) fn assign_immigrants(
    state: &mut GameState,
    defs: &Defs,
    household: HouseholdId,
    reserve: u16,
    events: &mut Vec<SimEvent>,
) -> bool {
    let mut free: Vec<(BuildingId, u16)> =
        free_places(state, defs).into_iter().map(|(id, f)| (id, f.saturating_sub(reserve))).collect();
    assign_with(state, household, &mut free, events)
}

/// Free places in all finished houses (research bonuses included).
pub fn free_housing(state: &GameState, defs: &Defs) -> u32 {
    free_places(state, defs).into_iter().map(|(_, f)| u32::from(f)).sum()
}

/// Places open to immigrants: the free places of every house beyond the
/// `immigration_reserved_places` each house keeps for births and marriages.
pub fn immigrant_places(state: &GameState, defs: &Defs) -> u32 {
    let reserve = u16::from(defs.rules().population.immigration_reserved_places);
    free_places(state, defs).into_iter().map(|(_, f)| u32::from(f.saturating_sub(reserve))).sum()
}

/// Capacity of the largest house type the player can build (including research bonuses):
/// every household up to this size can get a home by building houses.
pub(crate) fn largest_buildable_capacity(state: &GameState, defs: &Defs) -> u16 {
    defs.buildings()
        .iter()
        .filter(|d| d.buildable)
        .filter_map(|d| d.housing.as_ref())
        .map(|h| crate::workforce::housing_capacity(state, defs, h.capacity))
        .max()
        .unwrap_or(0)
}

/// Splits a homeless household larger than `limit` so that every part fits a house the
/// player can build: the core (the first couple, else the first adult or elder, else the
/// first member) keeps the family together with the youngest children; adult children (with
/// their partners), other adults and then the oldest children form new households of the
/// same family name. Returns the new households.
fn split_household(state: &mut GameState, id: HouseholdId, limit: u16, events: &mut Vec<SimEvent>) -> Vec<HouseholdId> {
    let limit = usize::from(limit.max(1));
    let pop = &state.population;
    let Some(h) = pop.household(id) else { return Vec::new() };
    if h.home.is_some() || h.members.len() <= limit {
        return Vec::new();
    }
    let members = h.members.clone();
    let family = h.family_name.clone();
    let (supply, unhappy_days) = (h.supply, h.unhappy_days);
    let day = state.clock.total_days();
    let in_household = |c: Option<CitizenId>| c.filter(|c| members.contains(c));
    let grown = |m: &CitizenId| pop.citizen(*m).is_some_and(|c| c.stage != LifeStage::Child);
    let head = members
        .iter()
        .copied()
        .find(|m| grown(m) && in_household(pop.citizen(*m).and_then(|c| c.partner)).is_some())
        .or_else(|| members.iter().copied().find(grown))
        .unwrap_or(members[0]);
    let mut core = vec![head];
    core.extend(in_household(pop.citizen(head).and_then(|c| c.partner)));
    // Children stay with the core, youngest first; everybody else moves out.
    let mut children: Vec<(u32, CitizenId)> = members
        .iter()
        .copied()
        .filter(|m| !core.contains(m) && !grown(m))
        .filter_map(|m| pop.citizen(m).map(|c| (c.age_on(day), m)))
        .collect();
    children.sort_unstable();
    let room = limit.saturating_sub(core.len());
    core.extend(children.iter().take(room).map(|(_, m)| *m));
    // Leaving: adults with their partners (in member order), then the remaining children,
    // oldest first; packed into new households of at most `limit` without splitting couples.
    let mut leaving: Vec<Vec<CitizenId>> = Vec::new();
    for m in members.iter().copied().filter(|m| !core.contains(m) && grown(m)) {
        if leaving.iter().flatten().any(|x| *x == m) {
            continue;
        }
        let mut group = vec![m];
        group.extend(in_household(pop.citizen(m).and_then(|c| c.partner)).filter(|p| !core.contains(p)));
        leaving.push(group);
    }
    leaving.extend(children.iter().skip(room).rev().map(|(_, m)| vec![*m]));
    let mut parts: Vec<Vec<CitizenId>> = Vec::new();
    for group in leaving {
        match parts.last_mut() {
            Some(part) if part.len() + group.len() <= limit => part.extend(group),
            _ => parts.push(group),
        }
    }
    let mut created = Vec::with_capacity(parts.len());
    for part in parts {
        // The part takes its members' share of the savings along.
        let before = state.population.household(id).map_or(1, |h| h.members.len());
        let purse = state.population.households.get_mut(id).map(|h| h.purse.split_off(part.len(), before)).unwrap_or_default();
        let new = state.population.households.insert(Household {
            family_name: family.clone(),
            members: Vec::new(),
            home: None,
            pantry: Vec::new(),
            supply,
            unhappy_days,
            purse,
        });
        for m in &part {
            if let Some(c) = state.population.citizens.get_mut(*m) {
                c.household = new;
            }
        }
        if let Some(h) = state.population.households.get_mut(id) {
            h.members.retain(|m| !part.contains(m));
        }
        if let Some(h) = state.population.households.get_mut(new) {
            h.members = part;
        }
        created.push(new);
    }
    events.push(SimEvent::Notification(Notification::new(
        Severity::Info,
        format!("Familie {family} ist zu groß für ein Haus und verteilt sich auf {} Haushalte.", created.len() + 1),
    )));
    state.workforce.mark_dirty();
    created
}

/// Moves a homeless household into the house with the least free space in `free` that fits it
/// (ties: lowest id) and updates `free`; returns true if it found one.
fn assign_with(
    state: &mut GameState,
    household: HouseholdId,
    free: &mut [(BuildingId, u16)],
    events: &mut Vec<SimEvent>,
) -> bool {
    let Some(size) = state.population.household(household).filter(|h| h.home.is_none()).map(|h| h.size()) else {
        return false;
    };
    let best = free.iter_mut().filter(|(_, f)| *f >= size).min_by_key(|(id, f)| (*f - size, *id));
    match best {
        Some((home, places)) => {
            *places -= size;
            let home = *home;
            set_home(state, household, Some(home), events);
            state.workforce.mark_dirty();
            true
        }
        None => false,
    }
}

/// Brings the homes in line with the buildings: removes homes of houses that are gone (or no
/// longer finished), adds new houses, moves households out of overfull houses and finds
/// homes for the homeless.
pub(crate) fn sync_homes(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let houses: BTreeSet<BuildingId> = state
        .buildings
        .iter()
        .filter(|(_, b)| b.is_complete() && defs.buildings().get(b.kind().index()).is_some_and(|d| d.housing.is_some()))
        .map(|(id, _)| id)
        .collect();
    let stale: Vec<BuildingId> = state.population.homes.keys().copied().filter(|id| !houses.contains(id)).collect();
    for id in stale {
        let households = state.population.homes.get(&id).map(|h| h.households.clone()).unwrap_or_default();
        for hh in households {
            set_home(state, hh, None, events);
        }
        state.population.homes.remove(&id);
    }
    for id in &houses {
        state.population.homes.entry(*id).or_default();
    }
    let homes: Vec<BuildingId> = state.population.homes.keys().copied().collect();
    for id in homes {
        let capacity = home_capacity(state, defs, id);
        while state.population.resident_count(id) > capacity {
            let Some(last) = state.population.homes.get(&id).and_then(|h| h.households.last().copied()) else { break };
            set_home(state, last, None, events);
        }
    }
    let mut homeless: Vec<(u16, HouseholdId)> =
        state.population.households().filter(|(_, h)| h.home.is_none()).map(|(id, h)| (h.size(), id)).collect();
    homeless.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    if !homeless.is_empty() {
        let mut free = free_places(state, defs);
        let limit = largest_buildable_capacity(state, defs);
        let mut oversized = Vec::new();
        for (size, id) in homeless {
            if !assign_with(state, id, &mut free, events) && limit > 0 && size > limit {
                oversized.push(id);
            }
        }
        // Households no buildable house could ever take split up (never staying homeless for
        // good); the parts move in where they fit.
        for id in oversized {
            let parts = split_household(state, id, limit, events);
            for part in std::iter::once(id).chain(parts) {
                assign_with(state, part, &mut free, events);
            }
        }
    }
    state.workforce.housing_done();
    if state.clock.is_new_hour() {
        let homeless: u32 = state.population.households().filter(|(_, h)| h.home.is_none()).map(|(_, h)| u32::from(h.size())).sum();
        if homeless > 0 {
            let text = if homeless == 1 {
                String::from("Ein Bewohner ist obdachlos. Baut weitere Wohnhäuser.")
            } else {
                format!("{homeless} Bewohner sind obdachlos. Baut weitere Wohnhäuser.")
            };
            let tick = state.clock.tick;
            // The homeless camp at the headquarters.
            let camp = crate::economy::storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b)).map(|b| b.entrance());
            let mut note = Notification::new(Severity::Warning, text);
            if let Some(at) = camp {
                note = note.at(at);
            }
            state.population.notify(Notice::Homeless, tick, note, events);
        }
    }
}

/// A required need and the residents' average.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedRequirement {
    /// The need.
    pub need: NeedKind,
    /// Average of the residents in points (0 without residents).
    pub average: u8,
    /// Average reaches `need_met_threshold`.
    pub met: bool,
}

/// State of one housing requirement (service or goods).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementStatus<T> {
    /// What is required.
    pub what: T,
    /// Fulfilled now.
    pub met: bool,
}

/// How a house meets the requirements of a housing tier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeRequirements {
    /// The tier (building type) checked.
    pub tier: BuildingTypeId,
    /// Residents of the house.
    pub residents: u16,
    /// Required needs.
    pub needs: Vec<NeedRequirement>,
    /// Required services.
    pub services: Vec<RequirementStatus<ServiceKind>>,
    /// Required goods (each satisfied by any of its resources).
    pub goods: Vec<RequirementStatus<Vec<ResourceId>>>,
}

impl HomeRequirements {
    /// True if the house has residents and every requirement is met.
    pub fn all_met(&self) -> bool {
        self.residents > 0
            && self.needs.iter().all(|n| n.met)
            && self.services.iter().all(|s| s.met)
            && self.goods.iter().all(|g| g.met)
    }
}

/// Evaluates `requirements` (of tier `tier`) for the residents of `building`; `covers` tells
/// whether a service reaches the house.
pub(crate) fn evaluate(
    state: &GameState,
    defs: &Defs,
    building: BuildingId,
    tier: BuildingTypeId,
    requirements: &HousingRequirements,
    covers: impl Fn(ServiceKind) -> bool,
) -> HomeRequirements {
    let pop = &state.population;
    let threshold = defs.rules().population.need_met_threshold;
    let residents: Vec<_> = pop.residents_of(building).filter_map(|c| pop.citizen(c)).collect();
    let needs = requirements
        .needs
        .iter()
        .map(|need| {
            let average = if residents.is_empty() {
                0
            } else {
                (residents.iter().map(|c| u32::from(c.needs.centi(*need))).sum::<u32>() / residents.len() as u32
                    / u32::from(CENTI)) as u8
            };
            NeedRequirement { need: *need, average, met: !residents.is_empty() && average >= threshold }
        })
        .collect();
    let services = requirements.services.iter().map(|s| RequirementStatus { what: *s, met: covers(*s) }).collect();
    let households: Vec<_> = pop
        .homes
        .get(&building)
        .map(|h| h.households.iter().filter_map(|hh| pop.household(*hh)).collect())
        .unwrap_or_default();
    let goods = requirements
        .goods
        .iter()
        .map(|g| {
            let met = !households.is_empty()
                && households.iter().all(|h| g.any_of.iter().any(|r| h.pantry_milli(*r) > 0));
            RequirementStatus { what: g.any_of.clone(), met }
        })
        .collect();
    HomeRequirements { tier, residents: u16::try_from(residents.len()).unwrap_or(u16::MAX), needs, services, goods }
}

/// How the residents of house `building` meet the requirements of housing tier `tier` (its own
/// type to keep it, the next type to upgrade). `None` if the building or tier is no house.
pub fn home_requirements(state: &GameState, defs: &Defs, building: BuildingId, tier: BuildingTypeId) -> Option<HomeRequirements> {
    let b = state.buildings.get(building)?;
    let requirements = &defs.buildings().get(tier.index())?.housing.as_ref()?.requirements;
    let rect = b.rect();
    Some(evaluate(state, defs, building, tier, requirements, |s| {
        crate::buildings::service_reaches(state, defs, s, rect)
    }))
}

/// Hourly tier check of the houses whose slot is due this tick (see the module documentation).
pub(crate) fn update_homes(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let slot = state.clock.tick % TICKS_PER_HOUR;
    let due: Vec<BuildingId> =
        state.population.homes.keys().copied().filter(|id| u64::from(id.index()) % TICKS_PER_HOUR == slot).collect();
    if due.is_empty() {
        return;
    }
    let index = ServiceIndex::collect(state, defs);
    let limit = defs.rules().population.downgrade_after_hours;
    for id in due {
        let Some(b) = state.buildings.get(id) else { continue };
        let kind = b.kind();
        let active = matches!(b.phase(), BuildingPhase::Active);
        let rect = b.rect();
        let Some(housing) = defs.buildings().get(kind.index()).and_then(|d| d.housing.as_ref()) else { continue };
        let check = active && housing.tier > 1 && state.population.resident_count(id) > 0;
        let unmet = check && !evaluate(state, defs, id, kind, &housing.requirements, |s| index.covers(s, &rect)).all_met();
        let Some(home) = state.population.homes.get_mut(&id) else { continue };
        home.unmet_hours = if unmet { home.unmet_hours.saturating_add(1) } else { 0 };
        if home.unmet_hours >= limit {
            downgrade(state, defs, id, events);
        }
    }
}

/// Lowers a house to the previous tier.
fn downgrade(state: &mut GameState, defs: &Defs, id: BuildingId, events: &mut Vec<SimEvent>) {
    let Some(b) = state.buildings.get(id) else { return };
    let from = b.kind();
    let Some(to) = previous_tier(defs, from) else { return };
    let (Some(cur), Some(prev)) = (defs.buildings().get(from.index()), defs.buildings().get(to.index())) else { return };
    let rotation = b.rotation();
    let origin = rotation.grown_origin(b.origin(), cur.size, prev.size);
    let rect = rotation.footprint(origin, prev.size);
    let entrance = rotation.entrance(origin, prev.size);
    if let Err(e) = state.buildings.relocate(id, rect, rotation, entrance) {
        // A smaller tier lies inside the old footprint, so this cannot fail.
        log::error!("downgrade of {id:?} could not shrink its footprint: {e}");
        return;
    }
    if let Some(b) = state.buildings.get_mut(id) {
        b.kind = to;
    }
    if let Some(home) = state.population.homes.get_mut(&id) {
        home.unmet_hours = 0;
    }
    events.push(SimEvent::HouseDowngraded { building: id, from, to });
    let text = format!("Abstieg: {} wird wieder zu {} – die Bewohner waren zu lange unversorgt.", cur.name, prev.name);
    let tick = state.clock.tick;
    state.population.notify(Notice::Downgrade, tick, Notification::new(Severity::Warning, text).at(entrance), events);
    state.workforce.mark_dirty();
    sync_homes(state, defs, events);
}
