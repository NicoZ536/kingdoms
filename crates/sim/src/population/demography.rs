//! Demography (docs/game_design.md §6): settlers, aging, education, births, deaths, marriages,
//! immigration and emigration. Randomness comes only from the state's `ChaCha8Rng`.
//!
//! * **Aging:** one year per season (§2); stages child → adult (`adult_age`) → elder
//!   (`elder_age`). Pupils (from `school_age`, school in reach) gain `education_per_day`; at
//!   adulthood a citizen with `educated_threshold` counts as educated.
//! * **Deaths:** old age (daily chance from the `mortality` table, per year of life = 6 days),
//!   disease (daily chance growing with the square of the missing health, lower with a
//!   hospital and `DiseaseResistance` research), starvation and freezing (health 0, see
//!   [`super::needs`]).
//! * **Births:** a couple in the same household, mother `fertile_min_age`–`fertile_max_age`,
//!   a free place in the house, fed and happy household, at most one birth per mother per
//!   `birth_interval_days`.
//! * **Marriages:** once a day single, childless adults of different households pair up (age
//!   gap ≤ `marriage_max_age_gap`); one partner joins the other's household if the house has
//!   room. Children inherit the family name of their household.
//! * **Immigration:** once an hour while the average happiness exceeds
//!   `immigration_happiness` (§6: > 70), nobody is homeless, a house has room beyond the
//!   `immigration_reserved_places` it keeps for births and marriages, and the stock holds
//!   `immigration_food_days` of food; at most `max(immigration_daily_base,
//!   immigration_daily_percent × population)` settlers a day. Immigrants arrive at the main
//!   headquarters and move into the best fitting house.
//! * **Emigration:** a household whose average happiness stays below `emigration_happiness`
//!   (§6: < 30) for `emigration_days` consecutive days leaves the kingdom.

use std::collections::{BTreeMap, BTreeSet};

use rand::RngExt;
use rand_chacha::ChaCha8Rng;

use crate::defs::{Defs, NameLists, PopulationRules, ServiceKind};
use crate::economy::storage;
use crate::event::{Notification, Severity, SimEvent};
use crate::state::GameState;
use crate::time::DAYS_PER_SEASON;

use super::movement::{self, Motion, WalkerId};
use super::{
    Activity, CENTI, Citizen, CitizenId, DeathCause, Household, HouseholdId, LifeStage, MAX_VALUE, Needs, Notice, Sex,
    SupplyState, housing,
};

/// Need level of new settlers and immigrants (hunger starts full).
const ARRIVAL_NEEDS: u8 = 80;
/// Happiness of new settlers and immigrants.
const ARRIVAL_HAPPINESS: u8 = 70;
/// Largest immigrant household.
const MAX_IMMIGRANT_HOUSEHOLD: u16 = 5;

/// Parts per million of a probability (0–1).
fn ppm(probability: f64) -> u32 {
    (probability.clamp(0.0, 1.0) * 1_000_000.0).round() as u32
}

/// True with probability `ppm` / 1 000 000.
fn roll(rng: &mut ChaCha8Rng, ppm: u32) -> bool {
    rng.random_range(0..1_000_000u32) < ppm
}

fn pick<'a>(rng: &mut ChaCha8Rng, list: &'a [String]) -> &'a str {
    if list.is_empty() { "" } else { list[rng.random_range(0..list.len())].as_str() }
}

fn first_name(rng: &mut ChaCha8Rng, names: &NameLists, sex: Sex) -> String {
    pick(rng, if sex == Sex::Male { &names.male } else { &names.female }).to_string()
}

/// A family name not used by another household if possible.
fn family_name(state: &mut GameState, names: &NameLists) -> String {
    for _ in 0..8 {
        let name = pick(&mut state.rng, &names.family).to_string();
        if !state.population.households.values().any(|h| h.family_name == name) {
            return name;
        }
    }
    pick(&mut state.rng, &names.family).to_string()
}

fn random_sex(rng: &mut ChaCha8Rng) -> Sex {
    if rng.random_range(0..2u32) == 0 { Sex::Male } else { Sex::Female }
}

/// Today as a signed day number.
fn today(state: &GameState) -> i64 {
    i64::try_from(state.clock.total_days()).unwrap_or(i64::MAX)
}

/// Birth day for `age` years today, at a random day within the year of life.
fn birth_day_for_age(state: &mut GameState, age: u32) -> i64 {
    let offset = state.rng.random_range(0..DAYS_PER_SEASON) as i64;
    today(state) - i64::from(age) * DAYS_PER_SEASON as i64 - offset
}

/// A new citizen without household links yet.
fn citizen(first_name: String, sex: Sex, birth_day: i64, stage: LifeStage, educated: bool) -> Citizen {
    let mut needs = Needs::uniform(ARRIVAL_NEEDS);
    needs.set_centi(crate::defs::NeedKind::Hunger, MAX_VALUE);
    Citizen {
        soldier: None,
        first_name,
        sex,
        birth_day,
        household: HouseholdId::from_raw_parts(u32::MAX, 0),
        partner: None,
        mother: None,
        father: None,
        job: None,
        needs,
        health: MAX_VALUE,
        happiness: u16::from(ARRIVAL_HAPPINESS) * CENTI,
        education: if educated { 100 } else { 0 },
        educated,
        activity: Activity::default(),
        stage,
        last_birth_day: None,
        motion: Motion::standing(glam::IVec2::ZERO),
        carry: None,
        friends: Vec::new(),
        experience: [0; crate::defs::JobCategory::ALL.len()],
        sick_since: None,
        sick_leave: false,
    }
}

/// Creates a homeless household from `members` (couples: the first two adults of opposite
/// sex become partners; children get them as parents).
fn spawn_household(state: &mut GameState, defs: &Defs, family_name: String, members: Vec<Citizen>) -> HouseholdId {
    let purse = super::Purse::with_savings(super::wealth::start_savings_milli(defs, members.len()));
    let id = state.population.households.insert(Household {
        family_name,
        members: Vec::new(),
        home: None,
        pantry: Vec::new(),
        supply: SupplyState::default(),
        unhappy_days: 0,
        purse,
    });
    let mut ids = Vec::with_capacity(members.len());
    for mut c in members {
        c.household = id;
        ids.push(state.population.citizens.insert(c));
    }
    let adults: Vec<CitizenId> =
        ids.iter().copied().filter(|c| state.population.citizen(*c).is_some_and(|c| c.stage != LifeStage::Child)).collect();
    let father = adults.iter().copied().find(|c| state.population.citizen(*c).is_some_and(|c| c.sex == Sex::Male));
    let mother = adults.iter().copied().find(|c| state.population.citizen(*c).is_some_and(|c| c.sex == Sex::Female));
    if let (Some(f), Some(m)) = (father, mother) {
        if let Some(c) = state.population.citizens.get_mut(f) {
            c.partner = Some(m);
        }
        if let Some(c) = state.population.citizens.get_mut(m) {
            c.partner = Some(f);
        }
    }
    for c in &ids {
        if let Some(cit) = state.population.citizens.get_mut(*c)
            && cit.stage == LifeStage::Child
        {
            cit.mother = mother;
            cit.father = father;
        }
    }
    if let Some(h) = state.population.households.get_mut(id) {
        h.members = ids;
    }
    state.workforce.mark_dirty();
    id
}

/// Builds a family: `adults` (1 or 2, a couple if 2) and `children`; ages and names random.
pub(crate) fn make_family(state: &mut GameState, defs: &Defs, adults: u16, children: u16) -> HouseholdId {
    let rules = &defs.rules().population;
    let names = defs.names();
    let family = family_name(state, names);
    let educated_ppm = ppm(f64::from(rules.immigrant_educated_percent) / 100.0);
    let mut members = Vec::new();
    let base_age = state.rng.random_range(20..=36u32);
    let mut mother_age = None;
    for i in 0..adults.min(2) {
        let sex = if adults >= 2 { if i == 0 { Sex::Male } else { Sex::Female } } else { random_sex(&mut state.rng) };
        let age = (base_age + state.rng.random_range(0..=4u32)).saturating_sub(2);
        if sex == Sex::Female {
            mother_age = Some(age);
        }
        let birth = birth_day_for_age(state, age);
        let educated = roll(&mut state.rng, educated_ppm);
        let name = first_name(&mut state.rng, names, sex);
        members.push(citizen(name, sex, birth, LifeStage::for_age(age, rules), educated));
    }
    let oldest_child = mother_age.unwrap_or(base_age).saturating_sub(u32::from(rules.fertile_min_age));
    let max_child_age = oldest_child.min(u32::from(rules.adult_age).saturating_sub(1));
    for _ in 0..children {
        let sex = random_sex(&mut state.rng);
        let age = state.rng.random_range(0..=max_child_age);
        let birth = birth_day_for_age(state, age);
        let name = first_name(&mut state.rng, names, sex);
        let mut child = citizen(name, sex, birth, LifeStage::for_age(age, rules), false);
        child.education = 0;
        members.push(child);
    }
    let id = spawn_household(state, defs, family, members);
    movement::place_inside_home(state, defs, id);
    id
}

/// Number of people housed when families of `sizes` move into houses of `capacities`
/// (largest family first, best fit).
fn packed(sizes: &[u16], capacities: &[u16]) -> u32 {
    let mut free = capacities.to_vec();
    let mut order = sizes.to_vec();
    order.sort_by(|a, b| b.cmp(a));
    let mut housed = 0;
    for s in order {
        if let Some(slot) = free.iter_mut().filter(|f| **f >= s).min_by_key(|f| **f) {
            *slot -= s;
            housed += u32::from(s);
        }
    }
    housed
}

/// Creates the scenario's settlers (docs/game_design.md §13): `families` couples (singles if
/// there are too few settlers) and the remaining settlers as their children, distributed so
/// that the start houses hold as many people as possible; then moves them into the houses.
pub(crate) fn create_settlers(state: &mut GameState, defs: &Defs, settlers: u16, families: u16, events: &mut Vec<SimEvent>) {
    if settlers == 0 {
        return;
    }
    housing::sync_homes(state, defs, events);
    let families = families.clamp(1, settlers);
    let couples = (settlers - families).min(families);
    let adults: Vec<u16> = (0..families).map(|i| if i < couples { 2 } else { 1 }).collect();
    let children = settlers - adults.iter().sum::<u16>();
    let capacities: Vec<u16> = housing::free_places(state, defs).into_iter().map(|(_, free)| free).collect();
    let mut kids = vec![0u16; usize::from(families)];
    for _ in 0..children {
        let best = (0..kids.len())
            .max_by_key(|&i| {
                let mut sizes: Vec<u16> = adults.iter().zip(&kids).map(|(a, k)| a + k).collect();
                sizes[i] += 1;
                (packed(&sizes, &capacities), std::cmp::Reverse(i))
            })
            .unwrap_or(0);
        kids[best] += 1;
    }
    let households: Vec<HouseholdId> = adults.iter().zip(&kids).map(|(a, k)| make_family(state, defs, *a, *k)).collect();
    // The founders bring their trades along: the adults start fully trained in every job
    // category (docs §6 Qualifikation; newcomers learn on the job).
    let trained = defs.rules().population.training.full_after_days;
    for h in &households {
        let members = state.population.household(*h).map(|x| x.members.clone()).unwrap_or_default();
        for m in members {
            if let Some(c) = state.population.citizens.get_mut(m)
                && c.stage == LifeStage::Adult
            {
                c.experience = [trained; crate::defs::JobCategory::ALL.len()];
            }
        }
    }
    housing::sync_homes(state, defs, events);
    // The settlers start inside their houses (or the camp), not on the way there.
    for id in households {
        movement::place_inside_home(state, defs, id);
    }
}

/// The daily part of a household's update: aging, education, deaths, births, emigration.
pub(crate) fn daily_household(state: &mut GameState, defs: &Defs, id: HouseholdId, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().population;
    let day = state.clock.total_days();
    let Some(h) = state.population.household(id) else { return };
    let members = h.members.clone();
    let pupils_served = h.supply.has_service(ServiceKind::Education);
    let hospital = h.supply.has_service(ServiceKind::Health);
    let mut stage_changed = false;
    let mut retired = Vec::new();
    for m in &members {
        let Some(c) = state.population.citizens.get_mut(*m) else { continue };
        let age = c.age_on(day);
        if c.stage == LifeStage::Child && pupils_served && age >= u32::from(rules.school_age) {
            c.education = c.education.saturating_add(rules.education_per_day).min(100);
        }
        let stage = LifeStage::for_age(age, rules);
        if stage != c.stage {
            if c.stage == LifeStage::Child {
                c.educated = c.educated || c.education >= rules.educated_threshold;
            }
            c.stage = stage;
            stage_changed = true;
            if !c.can_work() {
                retired.push(*m);
            }
        }
    }
    // Who can no longer work leaves the job at once (employed never exceeds available).
    for m in retired {
        release_job(state, m);
    }
    if stage_changed {
        state.workforce.mark_dirty();
    }
    // Deaths by old age and disease.
    let resistance = state.research.disease_resistance_percent(defs);
    let weather_disease = f64::from(state.weather.disease_percent(defs.rules())) / 100.0;
    for m in &members {
        let Some(c) = state.population.citizen(*m) else { continue };
        let age = c.age_on(day);
        let old_age = f64::from(rules.mortality_percent(age)) / 100.0 / DAYS_PER_SEASON as f64;
        let missing = (100.0 - f64::from(c.health()).min(100.0)) / 50.0;
        let mut disease = f64::from(rules.disease_death_per_mille) / 1000.0 * missing * missing;
        if hospital {
            disease *= f64::from(rules.hospital_disease_percent) / 100.0;
        }
        disease *= f64::from(100 - resistance) / 100.0;
        disease *= weather_disease;
        if c.stage != LifeStage::Adult {
            disease *= 1.5;
        }
        if roll(&mut state.rng, ppm(old_age)) {
            die(state, defs, *m, DeathCause::OldAge, events);
        } else if roll(&mut state.rng, ppm(disease)) {
            die(state, defs, *m, DeathCause::Disease, events);
        }
    }
    if state.population.household(id).is_none() {
        return;
    }
    try_birth(state, defs, id, events);
    // Emigration of a lastingly unhappy household.
    let Some(h) = state.population.household(id) else { return };
    let happiness = state.population.household_mean(h, |c| c.happiness);
    let unhappy = happiness < u16::from(rules.emigration_happiness) * CENTI;
    let days = if unhappy { h.unhappy_days.saturating_add(1) } else { 0 };
    if let Some(h) = state.population.households.get_mut(id) {
        h.unhappy_days = days;
    }
    if days >= rules.emigration_days {
        emigrate(state, defs, id, events);
    }
}

/// A child is born to the first eligible mother of the household (with probability).
fn try_birth(state: &mut GameState, defs: &Defs, id: HouseholdId, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().population;
    let day = state.clock.total_days();
    let Some(h) = state.population.household(id) else { return };
    let Some(home) = h.home else { return };
    let free = housing::home_capacity(state, defs, home).saturating_sub(state.population.resident_count(home));
    if free == 0 || !h.supply.fed {
        return;
    }
    if state.population.household_mean(h, |c| c.happiness) < u16::from(rules.birth_min_happiness) * CENTI {
        return;
    }
    let mother = h.members.iter().copied().find(|m| is_fertile_mother(state, rules, id, *m, day));
    let Some(mother) = mother else { return };
    if !roll(&mut state.rng, ppm(f64::from(rules.birth_chance_percent) / 100.0)) {
        return;
    }
    let father = state.population.citizen(mother).and_then(|c| c.partner);
    let sex = random_sex(&mut state.rng);
    let name = first_name(&mut state.rng, defs.names(), sex);
    let mut child = citizen(name, sex, today(state), LifeStage::Child, false);
    child.education = 0;
    child.household = id;
    child.mother = Some(mother);
    child.father = father;
    if let Some(m) = state.population.citizen(mother) {
        child.needs = m.needs;
        child.happiness = m.happiness;
    }
    if let Some(b) = state.buildings.get(home) {
        // Newborns appear inside their home.
        child.motion = Motion::in_building(home, b.door());
    }
    let child_id = state.population.citizens.insert(child);
    if let Some(h) = state.population.households.get_mut(id) {
        h.members.push(child_id);
    }
    let born = today(state);
    if let Some(m) = state.population.citizens.get_mut(mother) {
        m.last_birth_day = Some(born);
    }
    state.population.stats.today.births = state.population.stats.today.births.saturating_add(1);
    events.push(SimEvent::CitizenBorn { citizen: child_id, household: id });
    let text = match (state.population.full_name(child_id), state.population.household(id)) {
        (Some(name), Some(h)) => format!("Familie {} freut sich über die Geburt von {name}.", h.family_name),
        _ => String::from("Ein Kind wurde geboren."),
    };
    let at = state.buildings.get(home).map(|b| b.entrance());
    let mut n = Notification::new(Severity::Good, text).for_building(home);
    if let Some(at) = at {
        n = n.at(at);
    }
    let tick = state.clock.tick;
    state.population.notify(Notice::Birth, tick, n, events);
    state.workforce.mark_dirty();
}

fn is_fertile_mother(state: &GameState, rules: &PopulationRules, household: HouseholdId, id: CitizenId, day: u64) -> bool {
    let Some(c) = state.population.citizen(id) else { return false };
    let age = c.age_on(day);
    let partner_here = c.partner.and_then(|p| state.population.citizen(p)).is_some_and(|p| p.household == household);
    let rested = c.last_birth_day.is_none_or(|d| d.saturating_add(i64::from(rules.birth_interval_days)) <= today_of(day));
    c.sex == Sex::Female
        && partner_here
        && rested
        && (u32::from(rules.fertile_min_age)..=u32::from(rules.fertile_max_age)).contains(&age)
}

fn today_of(day: u64) -> i64 {
    i64::try_from(day).unwrap_or(i64::MAX)
}

/// A citizen dies: statistics, events, a (rate-limited) notification, removal.
pub(crate) fn die(state: &mut GameState, defs: &Defs, id: CitizenId, cause: DeathCause, events: &mut Vec<SimEvent>) {
    let Some(c) = state.population.citizen(id) else { return };
    let age = c.age_on(state.clock.total_days());
    let name = state.population.full_name(id).unwrap_or_default();
    let at = state
        .population
        .household(c.household)
        .and_then(|h| h.home)
        .and_then(|b| state.buildings.get(b))
        .map(|b| b.entrance());
    let deaths = &mut state.population.stats.today.deaths[cause.index()];
    *deaths = deaths.saturating_add(1);
    events.push(SimEvent::CitizenDied { citizen: id, cause, age });
    let (severity, topic, text) = match cause {
        DeathCause::OldAge => (Severity::Info, Notice::Death, format!("{name} ist im Alter von {age} Jahren gestorben.")),
        DeathCause::Disease => (Severity::Warning, Notice::Death, format!("{name} ist an einer Krankheit gestorben.")),
        DeathCause::Starvation => (Severity::Danger, Notice::Starvation, format!("{name} ist verhungert!")),
        DeathCause::Freezing => (Severity::Danger, Notice::Starvation, format!("{name} ist erfroren!")),
        DeathCause::Battle => (Severity::Danger, Notice::Death, format!("{name} ist im Kampf gefallen.")),
    };
    let mut n = Notification::new(severity, text);
    if let Some(at) = at {
        n = n.at(at);
    }
    let tick = state.clock.tick;
    state.population.notify(topic, tick, n, events);
    remove_citizen(state, defs, id, events);
}

/// Ends a citizen's job and lowers the building's staffing (the next job allocation refills
/// the slot).
pub(crate) fn release_job(state: &mut GameState, id: CitizenId) {
    let Some(job) = state.population.citizen(id).and_then(|c| c.job) else { return };
    if let Some(b) = state.buildings.get_mut(job.building) {
        b.set_staffed(b.staffed().saturating_sub(1));
    }
    state.population.set_job(id, None);
    state.workforce.mark_jobs_dirty();
}

/// Removes a citizen everywhere: transport (carried goods are handed over,
/// [`crate::logistics::abandon`]), job (and the building's staffing), partnership, household (an
/// empty household is removed with it).
pub(crate) fn remove_citizen(state: &mut GameState, defs: &Defs, id: CitizenId, events: &mut Vec<SimEvent>) {
    crate::logistics::abandon(state, defs, id, events);
    crate::military::forget_citizen(state, id);
    let Some(c) = state.population.citizen(id) else { return };
    let (partner, household) = (c.partner, c.household);
    super::friends::forget(state, id);
    movement::forget(state, WalkerId::Citizen(id));
    release_job(state, id);
    if let Some(p) = partner
        && let Some(pc) = state.population.citizens.get_mut(p)
    {
        pc.partner = None;
    }
    let empty = match state.population.households.get_mut(household) {
        Some(h) => {
            h.members.retain(|m| *m != id);
            h.members.is_empty()
        }
        None => false,
    };
    state.population.citizens.remove(id);
    if empty {
        remove_household(state, household);
    }
    state.workforce.mark_dirty();
}

/// Removes an (empty) household and its place in a home.
fn remove_household(state: &mut GameState, id: HouseholdId) {
    if let Some(h) = state.population.households.remove(id)
        && let Some(home) = h.home
        && let Some(hm) = state.population.homes.get_mut(&home)
    {
        hm.households.retain(|x| *x != id);
    }
}

/// The whole household leaves the kingdom.
pub(crate) fn emigrate(state: &mut GameState, defs: &Defs, id: HouseholdId, events: &mut Vec<SimEvent>) {
    let Some(h) = state.population.household(id) else { return };
    let members = h.members.clone();
    let family = h.family_name.clone();
    let at = h.home.and_then(|b| state.buildings.get(b)).map(|b| b.entrance());
    let n = u16::try_from(members.len()).unwrap_or(u16::MAX);
    // They walk off the map as travellers.
    movement::depart_emigrants(state, defs, &members);
    for m in members {
        remove_citizen(state, defs, m, events);
    }
    state.population.stats.today.emigrants = state.population.stats.today.emigrants.saturating_add(u32::from(n));
    events.push(SimEvent::HouseholdEmigrated { household: id, members: n });
    let who = if n == 1 { "1 Person".to_owned() } else { format!("{n} Personen") };
    let mut note = Notification::new(Severity::Warning, format!("Familie {family} ({who}) hat das Reich unzufrieden verlassen."));
    if let Some(at) = at {
        note = note.at(at);
    }
    let tick = state.clock.tick;
    state.population.notify(Notice::Emigration, tick, note, events);
}

/// Saturation (permille) the kingdom stock holds in edible food.
fn stock_food(state: &GameState, defs: &Defs) -> f64 {
    defs.resources().iter().filter(|r| r.is_edible()).map(|r| f64::from(state.economy.stock().get(r.id)) * f64::from(r.nutrition)).sum()
}

/// Saturation the population eats per day.
fn daily_food_need(state: &GameState, defs: &Defs) -> f64 {
    let rules = &defs.rules().population;
    state
        .population
        .citizens()
        .map(|(_, c)| {
            let share = if c.stage == LifeStage::Child { f64::from(rules.child_food_percent) / 100.0 } else { 1.0 };
            f64::from(rules.food_per_day) * share
        })
        .sum()
}

/// Hourly immigration check (see the module documentation).
pub(crate) fn run_immigration(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().population;
    let Some(avg) = state.population.average_happiness() else { return };
    let Some(hq) = storage::main_headquarters(state, defs) else { return };
    if avg <= f32::from(rules.immigration_happiness) {
        return;
    }
    if state.population.households().any(|(_, h)| h.home.is_none()) {
        return;
    }
    let reserve = u16::from(rules.immigration_reserved_places);
    let max_free = housing::free_places(state, defs).into_iter().map(|(_, f)| f.saturating_sub(reserve)).max().unwrap_or(0);
    if max_free == 0 {
        return;
    }
    if stock_food(state, defs) < daily_food_need(state, defs) * f64::from(rules.immigration_food_days) {
        return;
    }
    let population = u32::try_from(state.population.len()).unwrap_or(u32::MAX);
    let cap = u32::from(rules.immigration_daily_base).max(population * u32::from(rules.immigration_daily_percent) / 100);
    let left = cap.saturating_sub(state.population.stats.today.immigrants);
    if left == 0 {
        return;
    }
    let chance = (f64::from(avg - f32::from(rules.immigration_happiness)) * f64::from(rules.immigration_chance_per_point))
        .min(f64::from(rules.immigration_max_chance_percent))
        / 100.0;
    if !roll(&mut state.rng, ppm(chance)) {
        return;
    }
    let limit = max_free.min(u16::try_from(left).unwrap_or(u16::MAX)).min(MAX_IMMIGRANT_HOUSEHOLD);
    let kind = state.rng.random_range(0..100u32);
    let (adults, children) = if limit == 1 || kind < 30 {
        (1, 0)
    } else if limit == 2 || kind < 60 {
        (2, 0)
    } else {
        let kids = state.rng.random_range(1..=(limit - 2).min(3));
        (2, kids)
    };
    let id = make_family(state, defs, adults, children);
    housing::assign_immigrants(state, defs, id, reserve, events);
    // Immigrants walk in from the map edge to their new home.
    let entry = movement::entry_point(state, defs);
    movement::place_standing(state, defs, id, entry);
    let size = adults + children;
    state.population.stats.today.immigrants = state.population.stats.today.immigrants.saturating_add(u32::from(size));
    let at = state.buildings.get(hq).map_or(state.world.start, |b| b.entrance());
    events.push(SimEvent::HouseholdArrived { household: id, members: size, position: at });
    let family = state.population.household(id).map(|h| h.family_name.clone()).unwrap_or_default();
    let text = if size == 1 {
        format!("Ein neuer Siedler ist eingetroffen: Familie {family}.")
    } else {
        format!("Neue Siedler: Familie {family} ({size} Personen) ist eingetroffen.")
    };
    let tick = state.clock.tick;
    state.population.notify(Notice::Immigration, tick, Notification::new(Severity::Good, text).at(at), events);
}

/// Refugees of an event (docs/game_design.md §12): `count` people walk in from the map edge
/// like immigrants. They come as families (couples with children, five people at most) sized
/// to the free places of the houses, largest first, and may take every free place (immigrants
/// leave one per house); whoever finds no place camps at the headquarters. Returns the
/// households that arrived.
pub(crate) fn arrive_refugees(state: &mut GameState, defs: &Defs, count: u16, events: &mut Vec<SimEvent>) -> Vec<HouseholdId> {
    let mut arrived = Vec::new();
    if count == 0 {
        return arrived;
    }
    let entry = movement::entry_point(state, defs);
    let at = storage::main_headquarters(state, defs).and_then(|id| state.buildings.get(id)).map_or(state.world.start, |b| b.entrance());
    let mut free: Vec<u16> = housing::free_places(state, defs).into_iter().map(|(_, f)| f).filter(|f| *f > 0).collect();
    free.sort_by(|a, b| b.cmp(a));
    let mut places = free.into_iter();
    let mut left = count;
    while left > 0 {
        let size = places.next().unwrap_or(MAX_IMMIGRANT_HOUSEHOLD).min(left).min(MAX_IMMIGRANT_HOUSEHOLD);
        let (adults, children) = if size <= 2 { (size, 0) } else { (2, size - 2) };
        let id = make_family(state, defs, adults, children);
        housing::assign_immigrants(state, defs, id, 0, events);
        movement::place_standing(state, defs, id, entry);
        state.population.stats.today.immigrants = state.population.stats.today.immigrants.saturating_add(u32::from(size));
        events.push(SimEvent::HouseholdArrived { household: id, members: size, position: at });
        arrived.push(id);
        left -= size;
    }
    arrived
}

/// Developer command `DebugSpawnHouseholds`: `count` households walk in from the map edge like
/// refugees (sizes cycle through 3, 2, 4 and 1 people: couples with children and singles; they
/// may take every free place, whoever finds none camps at the headquarters). Returns the
/// households that arrived.
pub(crate) fn arrive_households(state: &mut GameState, defs: &Defs, count: u16, events: &mut Vec<SimEvent>) -> Vec<HouseholdId> {
    let entry = movement::entry_point(state, defs);
    let at = storage::main_headquarters(state, defs).and_then(|id| state.buildings.get(id)).map_or(state.world.start, |b| b.entrance());
    let mut arrived = Vec::with_capacity(usize::from(count));
    for i in 0..count {
        let (adults, children) = [(2, 1), (2, 0), (2, 2), (1, 0)][usize::from(i % 4)];
        let id = make_family(state, defs, adults, children);
        housing::assign_immigrants(state, defs, id, 0, events);
        movement::place_standing(state, defs, id, entry);
        let size = adults + children;
        state.population.stats.today.immigrants = state.population.stats.today.immigrants.saturating_add(u32::from(size));
        events.push(SimEvent::HouseholdArrived { household: id, members: size, position: at });
        arrived.push(id);
    }
    arrived
}

/// True for an unmarried adult without living children (`parents`) who may marry.
fn is_single(rules: &PopulationRules, parents: &BTreeSet<CitizenId>, id: CitizenId, c: &Citizen, day: u64) -> bool {
    c.partner.is_none()
        && c.stage == LifeStage::Adult
        && c.age_on(day) >= u32::from(rules.marriage_min_age)
        && !parents.contains(&id)
}

/// Unmarried men by age, each age in id order (married men are removed).
struct Bachelors(BTreeMap<u32, Vec<(CitizenId, HouseholdId)>>);

impl Bachelors {
    /// The man closest in age to `age` (at most `max_gap` apart, ties: lowest id) who does not
    /// belong to `household`.
    fn best_match(&self, age: u32, max_gap: u32, household: HouseholdId) -> Option<(CitizenId, HouseholdId, u32)> {
        let first_of = |a: u32| {
            self.0.get(&a).and_then(|men| men.iter().find(|(_, h)| *h != household)).map(|(id, h)| (*id, *h, a))
        };
        (0..=max_gap).find_map(|gap| {
            let younger = age.checked_sub(gap).and_then(first_of);
            let older = if gap == 0 { None } else { age.checked_add(gap).and_then(first_of) };
            match (younger, older) {
                (Some(y), Some(o)) => Some(if o.0 < y.0 { o } else { y }),
                (y, o) => y.or(o),
            }
        })
    }

    fn remove(&mut self, id: CitizenId, age: u32) {
        if let Some(men) = self.0.get_mut(&age) {
            men.retain(|(m, _)| *m != id);
        }
    }
}

/// Daily marriages (see the module documentation). Every single woman (in id order) is
/// matched with the unmarried man of another household closest in age (ties: lowest id).
pub(crate) fn run_marriages(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().population;
    let day = state.clock.total_days();
    let parents: BTreeSet<CitizenId> =
        state.population.citizens().flat_map(|(_, c)| [c.mother, c.father]).flatten().collect();
    let mut women: Vec<(CitizenId, u32, HouseholdId)> = Vec::new();
    let mut men = Bachelors(BTreeMap::new());
    for (id, c) in state.population.citizens() {
        if !is_single(rules, &parents, id, c, day) {
            continue;
        }
        let age = c.age_on(day);
        match c.sex {
            Sex::Female => women.push((id, age, c.household)),
            Sex::Male => men.0.entry(age).or_default().push((id, c.household)),
        }
    }
    for list in men.0.values_mut() {
        list.sort_unstable_by_key(|(id, _)| *id);
    }
    let chance = ppm(f64::from(rules.marriage_chance_percent) / 100.0);
    let max_gap = u32::from(rules.marriage_max_age_gap);
    for (woman, age, household) in women {
        let Some((man, his_household, his_age)) = men.best_match(age, max_gap, household) else { continue };
        if !roll(&mut state.rng, chance) {
            continue;
        }
        // She joins his household if his house has room (or both are homeless), else he joins hers.
        let joined = if can_join(state, defs, his_household, household) {
            move_to_household(state, woman, his_household);
            true
        } else if can_join(state, defs, household, his_household) {
            move_to_household(state, man, household);
            true
        } else {
            false
        };
        if !joined {
            continue;
        }
        if let Some(c) = state.population.citizens.get_mut(woman) {
            c.partner = Some(man);
        }
        if let Some(c) = state.population.citizens.get_mut(man) {
            c.partner = Some(woman);
        }
        men.remove(man, his_age);
        events.push(SimEvent::CitizensMarried { a: man, b: woman });
    }
}

/// True if a member of `from` may move into household `to`: its house has a free place, or
/// both households are homeless.
fn can_join(state: &GameState, defs: &Defs, to: HouseholdId, from: HouseholdId) -> bool {
    let to_home = state.population.household(to).and_then(|h| h.home);
    let from_home = state.population.household(from).and_then(|h| h.home);
    match to_home {
        Some(home) => housing::home_capacity(state, defs, home) > state.population.resident_count(home),
        None => from_home.is_none(),
    }
}

fn move_to_household(state: &mut GameState, id: CitizenId, to: HouseholdId) {
    let Some(from) = state.population.citizen(id).map(|c| c.household) else { return };
    // The member takes its share of the savings along (all of them if it was alone).
    let before = state.population.household(from).map_or(1, |h| h.members.len());
    super::wealth::move_share(state, from, before, to);
    let empty = match state.population.households.get_mut(from) {
        Some(h) => {
            h.members.retain(|m| *m != id);
            h.members.is_empty()
        }
        None => false,
    };
    if empty {
        remove_household(state, from);
    }
    if let Some(h) = state.population.households.get_mut(to) {
        h.members.push(id);
    }
    if let Some(c) = state.population.citizens.get_mut(id) {
        c.household = to;
    }
    state.workforce.mark_dirty();
}
