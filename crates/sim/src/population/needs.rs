//! Needs, health, happiness and the routine step of citizens (docs/game_design.md §6), updated
//! every [`CITIZEN_UPDATE_TICKS`] ticks per citizen.
//!
//! Every need follows a *supply level* given by the household's [`SupplyState`] and the
//! services reaching its home (rates in `PopulationRules`):
//!
//! | Need | Level |
//! |---|---|
//! | Nahrung | fed in the last hour → 100; otherwise it falls by `hunger_decay_per_hour` |
//! | Wasser | well in reach or river/lake nearby → 100, else `no_water_thirst` (during a drought `climate.drought_thirst_without_water`) |
//! | Unterkunft | house → 100, homeless → 0 |
//! | Wärme | warm days 100; on cold days (below `heating_below_c`, all winter) fuel → 100 else `winter_warmth_without_fuel`, + clothes |
//! | Sicherheit | watchtower/barracks/castle → 100, else `base_safety` |
//! | Gesundheit | `base_health_need` + food variety + hospital |
//! | Gemeinschaft | `base_social` + family + market + tavern (half without beer) + `friend_social_bonus` per friend |
//! | Unterhaltung | tavern with beer (in stock, or in the pantry) → 100, tavern without → `tavern_without_goods_entertainment`, else `base_entertainment` |
//! | Glaube | church → 100, else `base_faith` |
//!
//! **Health** follows the mean of hunger, thirst, warmth and the health need (elders lose
//! maximum health with age); starving and freezing cost extra health, and a citizen at 0
//! health dies. **Happiness** follows the weighted mean of the needs plus modifiers (taxes,
//! wages/unemployment, missing goods of the housing tier, poverty or wealth of the household,
//! temporary event modifiers). Sick citizens in a hospital (purpose treatment) recover
//! `hospital_recovery_percent` as fast.

use serde::{Deserialize, Serialize};

use crate::defs::{Defs, JobCategory, JobTier, NeedKind, PopulationRules, ServiceKind};
use crate::event::SimEvent;
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;

use super::{
    CENTI, CITIZEN_UPDATE_TICKS, Citizen, CitizenId, DeathCause, Household, LifeStage, MAX_VALUE, Needs,
    demography,
};

/// A component of the happiness target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HappinessFactor {
    /// Contribution of a need (weighted share of the mean).
    Need(NeedKind),
    /// Tax rate above or below the neutral rate.
    Taxes,
    /// Wages of employed adults.
    Wages,
    /// Adults without a job.
    Unemployment,
    /// Goods of the housing tier that are not supplied.
    MissingGoods,
    /// Temporary modifiers from events (festivals, unrest, raids …; docs/game_design.md §12).
    Events,
    /// War weariness while the kingdom is at war (docs/game_design.md §10).
    War,
    /// Poverty or wealth of the household (docs/game_design.md §6 Wohlstand).
    Wealth,
}

impl HappinessFactor {
    /// German label.
    pub fn label(self) -> &'static str {
        match self {
            HappinessFactor::Need(n) => n.display_name(),
            HappinessFactor::Taxes => "Steuern",
            HappinessFactor::Wages => "Lohn",
            HappinessFactor::Unemployment => "Arbeitslosigkeit",
            HappinessFactor::MissingGoods => "Fehlende Waren",
            HappinessFactor::Events => "Ereignisse",
            HappinessFactor::War => "Krieg",
            HappinessFactor::Wealth => "Wohlstand",
        }
    }
}

/// A factor and its contribution in happiness points.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HappinessPart {
    /// The factor.
    pub factor: HappinessFactor,
    /// Contribution in points (needs: weighted share, modifiers: signed).
    pub points: f32,
}

/// Supply levels (0–100) of every need for a citizen of `household`.
pub(crate) fn need_levels(state: &GameState, defs: &Defs, household: &Household) -> [u8; NeedKind::ALL.len()] {
    let rules = &defs.rules().population;
    let s = household.supply;
    let cold = state.weather.needs_heating(defs.rules(), state.clock);
    let drought = state.weather.current() == crate::defs::Weather::Drought;
    let has = |k: ServiceKind| s.has_service(k);
    let add = |base: u8, bonus: u8| base.saturating_add(bonus).min(100);
    let mut out = [0u8; NeedKind::ALL.len()];
    for need in NeedKind::ALL {
        let level = match need {
            // Hunger is handled separately (it decays at its own rate).
            NeedKind::Hunger => {
                if s.fed {
                    100
                } else {
                    0
                }
            }
            NeedKind::Thirst => {
                if has(ServiceKind::Water) || s.fresh_water {
                    100
                } else if drought {
                    defs.rules().climate.drought_thirst_without_water
                } else {
                    rules.no_water_thirst
                }
            }
            NeedKind::Shelter => {
                if household.home.is_some() {
                    100
                } else {
                    0
                }
            }
            NeedKind::Warmth => {
                if !cold {
                    100
                } else {
                    let base = if s.fuel { 100 } else { rules.winter_warmth_without_fuel };
                    add(base, if s.clothes { rules.clothes_warmth_bonus } else { 0 })
                }
            }
            NeedKind::Safety => {
                if has(ServiceKind::Safety) {
                    100
                } else {
                    rules.base_safety
                }
            }
            NeedKind::Health => {
                let variety = u32::from(s.food_variety.saturating_sub(1)) * u32::from(rules.variety_bonus_per_food);
                let variety = variety.min(u32::from(rules.max_variety_bonus)) as u8;
                let hospital = if has(ServiceKind::Health) { rules.hospital_health_bonus } else { 0 };
                add(add(rules.base_health_need, variety), hospital)
            }
            NeedKind::Social => {
                let family = if household.members.len() >= 2 { rules.family_social_bonus } else { 0 };
                let market = if has(ServiceKind::Market) { rules.market_social_bonus } else { 0 };
                let tavern = match (has(ServiceKind::Entertainment), s.tavern_goods) {
                    (true, true) => rules.tavern_social_bonus,
                    (true, false) => rules.tavern_social_bonus / 2,
                    (false, _) => 0,
                };
                add(add(add(rules.base_social, family), market), tavern)
            }
            NeedKind::Entertainment => {
                if has(ServiceKind::Entertainment) && s.tavern_goods {
                    100
                } else if has(ServiceKind::Entertainment) {
                    rules.tavern_without_goods_entertainment.max(rules.base_entertainment)
                } else {
                    rules.base_entertainment
                }
            }
            NeedKind::Faith => {
                if has(ServiceKind::Religion) {
                    100
                } else {
                    rules.base_faith
                }
            }
        };
        out[super::need_index(need)] = level;
    }
    out
}

/// Calls `f` with every part of a citizen's happiness target (needs `needs`, modifiers); the
/// target is the sum of the parts clamped to 0–100.
fn for_each_part(
    state: &GameState,
    defs: &Defs,
    household: &Household,
    citizen: &Citizen,
    needs: &Needs,
    mut f: impl FnMut(HappinessPart),
) {
    let rules = &defs.rules().population;
    let total = rules.need_weights.total().max(f32::EPSILON);
    for n in NeedKind::ALL {
        let points = rules.need_weights.get(n) * f32::from(needs.centi(n)) / f32::from(CENTI) / total;
        f(HappinessPart { factor: HappinessFactor::Need(n), points });
    }
    let tax = (f32::from(rules.neutral_tax_rate_percent) - f32::from(state.policy.tax_rate_percent))
        * rules.tax_happiness_per_percent;
    f(HappinessPart { factor: HappinessFactor::Taxes, points: tax });
    if citizen.stage == LifeStage::Adult {
        if citizen.job.is_some() || citizen.soldier.is_some() {
            // Soldiers draw their pay like a wage.
            f(HappinessPart { factor: HappinessFactor::Wages, points: f32::from(rules.employed_happiness) });
        } else {
            f(HappinessPart { factor: HappinessFactor::Unemployment, points: f32::from(rules.unemployed_happiness) });
        }
    }
    let missing = f32::from(household.supply.missing_goods) * f32::from(rules.missing_goods_happiness);
    if missing != 0.0 {
        f(HappinessPart { factor: HappinessFactor::MissingGoods, points: missing });
    }
    let events = state.events.happiness_modifier(state.clock.tick);
    if events != 0 {
        f(HappinessPart { factor: HappinessFactor::Events, points: events as f32 });
    }
    let war = crate::diplomacy::war_weariness(state, defs);
    if war != 0 {
        f(HappinessPart { factor: HappinessFactor::War, points: war as f32 });
    }
    let wealth = super::wealth::happiness_points(state, defs, citizen.household);
    if wealth != 0 {
        f(HappinessPart { factor: HappinessFactor::Wealth, points: f32::from(wealth) });
    }
}

/// The parts of a citizen's happiness target for its current needs.
pub(crate) fn happiness_parts(state: &GameState, defs: &Defs, household: &Household, citizen: &Citizen) -> Vec<HappinessPart> {
    let mut parts = Vec::with_capacity(NeedKind::ALL.len() + 3);
    for_each_part(state, defs, household, citizen, &citizen.needs, |p| parts.push(p));
    parts
}

/// The happiness parts of one citizen (for the UI); `None` if the citizen does not exist.
pub fn citizen_happiness_parts(state: &GameState, defs: &Defs, citizen: CitizenId) -> Option<Vec<HappinessPart>> {
    let c = state.population.citizen(citizen)?;
    let h = state.population.household(c.household)?;
    Some(happiness_parts(state, defs, h, c))
}

/// Work efficiency of a citizen in permille: full at `full_productivity_health` health and
/// `full_productivity_happiness` happiness, down to 50 % each below (docs/game_design.md §6:
/// productivity = health × happiness factor × tools × training).
pub fn work_efficiency_permille(rules: &PopulationRules, citizen: &Citizen) -> u32 {
    let factor = |value: u16, full: u8| -> u32 {
        let full = u32::from(full) * u32::from(CENTI);
        if full == 0 || u32::from(value) >= full { 1000 } else { 500 + 500 * u32::from(value) / full }
    };
    factor(citizen.health, rules.full_productivity_health) * factor(citizen.happiness, rules.full_productivity_happiness)
        / 1000
}

/// Training factor in permille of a worker in a job of `tier` and `category` (docs §6
/// Qualifikation): unskilled jobs need none (1000); in skilled and educated jobs a worker new to
/// the category starts at `training.start_percent` (educated: `educated_start_percent`) and
/// reaches full productivity after `training.full_after_days` days worked in that category.
pub fn training_permille(rules: &PopulationRules, citizen: &Citizen, category: JobCategory, tier: JobTier) -> u32 {
    if tier == JobTier::Unskilled {
        return 1000;
    }
    let t = &rules.training;
    let start = u32::from(if citizen.educated { t.educated_start_percent } else { t.start_percent }).min(100) * 10;
    let full = u32::from(t.full_after_days.max(1));
    let days = u32::from(citizen.experience_days(category)).min(full);
    start + (1000 - start) * days / full
}

/// Moves `value` towards `target` by at most `step` (all in hundredths).
fn approach(value: u16, target: u16, step: u16) -> u16 {
    if value < target { value.saturating_add(step).min(target) } else { value.saturating_sub(step).max(target) }
}

/// Hundredths per update for a rate in points per game hour.
fn per_update(rate_per_hour: f32) -> u16 {
    let hours = CITIZEN_UPDATE_TICKS as f32 / TICKS_PER_HOUR as f32;
    (rate_per_hour * f32::from(CENTI) * hours).round().clamp(0.0, f32::from(MAX_VALUE)) as u16
}

/// Updates the citizens whose slot is due this tick; citizens reaching 0 health die.
pub(crate) fn update_citizens(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().population;
    let slot = state.clock.tick % CITIZEN_UPDATE_TICKS;
    let count = state.population.citizens.slot_count() as u64;
    let today = state.clock.total_days();
    let cold = state.weather.needs_heating(defs.rules(), state.clock);
    let need_step = per_update(rules.need_change_per_hour);
    let hunger_step = per_update(rules.hunger_decay_per_hour);
    let health_step = per_update(rules.health_change_per_hour);
    let starve_step = per_update(rules.starvation_damage_per_hour);
    let freeze_step = per_update(rules.freezing_damage_per_hour);
    let happy_step = per_update(rules.happiness_change_per_hour);
    let mut deaths: Vec<(CitizenId, DeathCause)> = Vec::new();
    let mut venues = super::routine::Venues::default();
    let mut index = slot;
    while index < count {
        let id = u32::try_from(index).ok().and_then(|i| state.population.citizens.id_at(i));
        index += CITIZEN_UPDATE_TICKS;
        let Some(id) = id else { continue };
        let Some(c) = state.population.citizens.get(id) else { continue };
        let Some(h) = state.population.households.get(c.household) else { continue };
        let mut levels = need_levels(state, defs, h);
        // Friends lift the social need (per citizen).
        let social = &mut levels[super::need_index(NeedKind::Social)];
        *social = social.saturating_add(super::friends::social_bonus(defs, c.friends.len())).min(100);
        let in_treatment = super::routine::in_treatment(state, defs, c);
        let age = c.age_on(today);
        let mut needs: Needs = c.needs;
        for need in NeedKind::ALL {
            let target = u16::from(levels[super::need_index(need)]) * CENTI;
            let current = needs.centi(need);
            let next = if need == NeedKind::Hunger && target == 0 {
                current.saturating_sub(hunger_step)
            } else {
                approach(current, target, need_step)
            };
            needs.set_centi(need, next);
        }
        // Health.
        let health_target = {
            let w = |n: NeedKind| f32::from(needs.centi(n));
            let mean = (3.0 * w(NeedKind::Hunger) + 2.0 * w(NeedKind::Thirst) + 2.0 * w(NeedKind::Warmth) + 3.0 * w(NeedKind::Health))
                / 10.0;
            let elder_years = age.saturating_sub(u32::from(rules.elder_age)) as f32;
            let cap = (100.0 - elder_years * rules.elder_health_loss_per_year).clamp(1.0, 100.0) * f32::from(CENTI);
            mean.min(cap).round() as u16
        };
        // Sick citizens in a hospital recover faster.
        let step = if in_treatment && health_target > c.health {
            u16::try_from(u32::from(health_step) * u32::from(rules.hospital_recovery_percent) / 100).unwrap_or(u16::MAX)
        } else {
            health_step
        };
        let mut health = approach(c.health, health_target, step);
        let starving = needs.centi(NeedKind::Hunger) == 0;
        let freezing = cold && needs.get(NeedKind::Warmth) < rules.freezing_warmth;
        if starving {
            health = health.saturating_sub(starve_step);
        }
        if freezing {
            health = health.saturating_sub(freeze_step);
        }
        // Happiness from the new needs.
        let mut target = 0.0f32;
        for_each_part(state, defs, h, c, &needs, |p| target += p.points);
        let happy_target = (target.clamp(0.0, 100.0) * f32::from(CENTI)).round() as u16;
        let happiness = approach(c.happiness, happy_target, happy_step);
        let now = state.clock.tick;
        let sick = health < u16::from(rules.sick_health) * CENTI;
        let mut leave_starts = false;
        let mut leave_ends = false;
        if let Some(c) = state.population.citizens.get_mut(id) {
            c.needs = needs;
            c.health = health;
            c.happiness = happiness;
            // Sick leave (docs §6): a worker ill for `sick_leave_after_hours` gives the job up;
            // healthy again, he looks for work like anybody else.
            match (sick, c.sick_since) {
                (true, None) => c.sick_since = Some(now),
                (true, Some(since)) => {
                    let due = now.saturating_sub(since) >= u64::from(rules.sick_leave_after_hours) * TICKS_PER_HOUR;
                    leave_starts = due && !c.sick_leave && c.stage == LifeStage::Adult && c.soldier.is_none();
                }
                (false, _) => {
                    c.sick_since = None;
                    leave_ends = std::mem::take(&mut c.sick_leave);
                }
            }
        }
        if leave_starts {
            demography::release_job(state, id);
            if let Some(c) = state.population.citizens.get_mut(id) {
                c.sick_leave = true;
            }
        }
        if leave_starts || leave_ends {
            state.workforce.mark_jobs_dirty();
        }
        super::routine::update(state, defs, id, &mut venues);
        if health == 0 {
            let cause = if starving {
                DeathCause::Starvation
            } else if freezing {
                DeathCause::Freezing
            } else {
                DeathCause::Disease
            };
            deaths.push((id, cause));
        }
    }
    for (id, cause) in deaths {
        demography::die(state, defs, id, cause, events);
    }
}
