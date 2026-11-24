//! Household money — Wohlstand (docs/game_design.md §6, §7).
//!
//! Every household keeps a [`Purse`](super::Purse) in milli-gold. Once per game hour, in the economy step
//! (pipeline step 8, `run_hour`), every household in id order
//! * **earns** one 24th of its daily income: per working member (employed adult or soldier) the
//!   `income_per_worker` of its house tier (homeless: `wealth.homeless_income_per_worker`) plus
//!   the crown's wage of the job tier (builders: unskilled) or the soldier's pay (not while the
//!   pay is unpaid); elders earn `elder_income_percent`, adults without a job
//!   `unemployed_income_percent` of a worker's earnings, children nothing;
//! * **pays** one 24th of the head tax (members × `tax_per_resident` × tax rate × (100 % +
//!   `TaxBonus`), only within reach of the administration) and of the market fee (members ×
//!   `market_fee_per_resident`, only with a staffed market in reach) — as far as its savings
//!   go. The treasury books exactly what the households paid (categories *Steuern* and
//!   *Marktgebühren*).
//!
//! Hourly shares split a daily amount exactly (`daily × (h + 1) / 24 − daily × h / 24`), so 24
//! hours add up to the day. At midnight the day's sums move to "yesterday".
//!
//! **Purchases** ([`crate::population`] supply): every unit a household takes from the kingdom
//! stock or a tavern costs the current market price. Food and fuel are taken even without
//! money (the household pays what it has; the poor are not left to starve), goods and beer only
//! when the household can pay for them.
//!
//! **Poverty and wealth:** savings below `poor_savings_per_member` × members make a household
//! *arm* (happiness `poverty_happiness`), from `wealthy_savings_per_member` × members it is
//! *wohlhabend* (`wealth_happiness`). Lasting unhappiness lets a household emigrate (§6).
//! New households bring `start_savings_per_member` per member; a member moving to another
//! household (marriage, a split family) takes its share along.

use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::defs::{Defs, JobTier, ResourceId, ServiceKind, TechEffect};
use crate::economy::MILLI_PER_GOLD;
use crate::state::GameState;

use super::{HouseholdId, LifeStage, ServiceIndex};

/// Wealth class of a household.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WealthClass {
    /// Savings below the poverty line.
    Poor,
    /// Between the lines.
    Modest,
    /// Savings from the wealth line on.
    Wealthy,
}

impl WealthClass {
    /// German label.
    pub const fn display_name(self) -> &'static str {
        match self {
            WealthClass::Poor => "Arm",
            WealthClass::Modest => "Auskömmlich",
            WealthClass::Wealthy => "Wohlhabend",
        }
    }
}

/// Gold amount (data) in milli-gold.
pub(crate) fn gold_milli(gold: f32) -> i64 {
    (f64::from(gold) * MILLI_PER_GOLD as f64).round() as i64
}

/// Daily money of a household in milli-gold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Dues {
    /// Earnings and wages.
    pub income: i64,
    /// Head tax.
    pub tax: i64,
    /// Market fee.
    pub fee: i64,
}

/// Tax factor in permille of `tax_per_resident`: tax rate × (100 % + `TaxBonus`).
fn tax_permille(state: &GameState, defs: &Defs) -> i64 {
    let bonus: i32 = state
        .research
        .researched()
        .filter_map(|t| defs.techs().get(t.index()))
        .flat_map(|t| t.effects.iter())
        .map(|e| match e {
            TechEffect::TaxBonus { percent } => i32::from(*percent),
            _ => 0,
        })
        .sum();
    i64::from(state.policy.tax_rate_percent) * 10 * i64::from((100 + bonus).max(0)) / 100
}

/// Daily income, tax and fee of household `id` (`services`: the current service index).
pub(crate) fn dues(state: &GameState, defs: &Defs, services: &ServiceIndex, id: HouseholdId) -> Dues {
    let rules = defs.rules();
    let wealth = &rules.population.wealth;
    let Some(h) = state.population.household(id) else { return Dues::default() };
    let home = h.home.and_then(|b| state.buildings.get(b).map(|bb| (b, bb)));
    let housing = home.and_then(|(_, b)| defs.buildings().get(b.kind().index())).and_then(|d| d.housing.as_ref());
    let earnings = gold_milli(housing.map_or(wealth.homeless_income_per_worker, |h| h.income_per_worker));
    let mut income = 0i64;
    for c in h.members.iter().filter_map(|m| state.population.citizen(*m)) {
        if let Some(unit) = c.soldier {
            let pay = if state.military.is_unpaid() { 0 } else { defs.units().get(unit.index()).map_or(0, |u| i64::from(u.upkeep_per_day)) };
            income += earnings + pay * MILLI_PER_GOLD;
        } else if let Some(job) = c.job {
            income += earnings + gold_milli(rules.wages.for_tier(job_tier(state, defs, job.building)));
        } else {
            match c.stage {
                LifeStage::Adult => income += earnings * i64::from(wealth.unemployed_income_percent) / 100,
                LifeStage::Elder => income += earnings * i64::from(wealth.elder_income_percent) / 100,
                LifeStage::Child => {}
            }
        }
    }
    let members = i64::try_from(h.members.len()).unwrap_or(i64::MAX);
    let (mut tax, mut fee) = (0, 0);
    if let (Some((_, b)), Some(housing)) = (home, housing) {
        let rect = b.rect();
        if services.covers(ServiceKind::Administration, &rect) {
            tax = members * gold_milli(housing.tax_per_resident) * tax_permille(state, defs) / 1000;
        }
        if services.covers(ServiceKind::Market, &rect) {
            fee = members * gold_milli(rules.market_fee_per_resident);
        }
    }
    Dues { income, tax, fee }
}

/// Job tier a worker of `building` is paid for (builders on a site: unskilled), as the
/// treasury's wage bill counts it.
fn job_tier(state: &GameState, defs: &Defs, building: BuildingId) -> JobTier {
    let Some(b) = state.buildings.get(building) else { return JobTier::Unskilled };
    if b.is_complete() && b.construction_site().is_none() {
        defs.buildings().get(b.kind().index()).and_then(|d| d.job).map_or(JobTier::Unskilled, |j| j.tier)
    } else {
        JobTier::Unskilled
    }
}

/// Hour `hour` (0–23) of a daily amount: the 24 shares add up to `daily` exactly.
fn hourly_share(daily: i64, hour: u64) -> i64 {
    let h = i64::try_from(hour % 24).unwrap_or(0);
    daily * (h + 1) / 24 - daily * h / 24
}

/// What the households paid the treasury in one hour, in milli-gold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HourPayments {
    /// Head tax.
    pub taxes: i64,
    /// Market fees.
    pub fees: i64,
}

/// The hourly money step of every household (see the module documentation); returns what the
/// households paid. Called by the economy step at the full hour.
pub(crate) fn run_hour(state: &mut GameState, defs: &Defs) -> HourPayments {
    let hour = state.clock.total_hours() % 24;
    let services = ServiceIndex::collect(state, defs);
    let ids: Vec<HouseholdId> = state.population.households.iter().map(|(id, _)| id).collect();
    let mut paid = HourPayments::default();
    for id in ids {
        let d = dues(state, defs, &services, id);
        let Some(h) = state.population.households.get_mut(id) else { continue };
        if hour == 0 {
            h.purse.close_day();
        }
        h.purse.earn(hourly_share(d.income, hour));
        paid.taxes += h.purse.pay(hourly_share(d.tax, hour));
        paid.fees += h.purse.pay(hourly_share(d.fee, hour));
    }
    paid
}

/// Expected daily tax and fee income of the treasury: every household's dues as far as its
/// savings and income cover them (for the finance forecast).
pub(crate) fn expected_daily_payments(state: &GameState, defs: &Defs, services: &ServiceIndex) -> (i64, i64) {
    let mut taxes = 0;
    let mut fees = 0;
    for (id, h) in state.population.households.iter() {
        let d = dues(state, defs, services, id);
        let mut money = h.purse.savings_milli.saturating_add(d.income);
        let tax = d.tax.min(money.max(0));
        money -= tax;
        taxes = i64::saturating_add(taxes, tax);
        fees = i64::saturating_add(fees, d.fee.min(money.max(0)));
    }
    (taxes, fees)
}

/// Price of one unit of `resource` at the current market price, in milli-gold.
pub(crate) fn unit_price_milli(state: &GameState, resource: ResourceId) -> i64 {
    i64::from(state.economy.prices().centi(resource)) * (MILLI_PER_GOLD / 100)
}

/// Savings a household of `members` needs for `class` lines, in milli-gold.
fn line_milli(per_member: f32, members: usize) -> i64 {
    gold_milli(per_member) * i64::try_from(members).unwrap_or(i64::MAX)
}

/// Wealth class of household `id` (`None` if it does not exist).
pub fn wealth_class(state: &GameState, defs: &Defs, id: HouseholdId) -> Option<WealthClass> {
    let h = state.population.household(id)?;
    let w = &defs.rules().population.wealth;
    let savings = h.purse.savings_milli;
    let members = h.members.len().max(1);
    Some(if savings < line_milli(w.poor_savings_per_member, members) {
        WealthClass::Poor
    } else if savings >= line_milli(w.wealthy_savings_per_member, members) {
        WealthClass::Wealthy
    } else {
        WealthClass::Modest
    })
}

/// Happiness modifier of the members of household `id` from its wealth class.
pub(crate) fn happiness_points(state: &GameState, defs: &Defs, id: HouseholdId) -> i8 {
    let w = &defs.rules().population.wealth;
    match wealth_class(state, defs, id) {
        Some(WealthClass::Poor) => w.poverty_happiness,
        Some(WealthClass::Wealthy) => w.wealth_happiness,
        _ => 0,
    }
}

/// Start savings of a new household with `members` members, in milli-gold.
pub(crate) fn start_savings_milli(defs: &Defs, members: usize) -> i64 {
    line_milli(defs.rules().population.wealth.start_savings_per_member, members)
}

/// Moves member's share of the savings of household `from` (with `members_before` members)
/// to household `to`.
pub(crate) fn move_share(state: &mut GameState, from: HouseholdId, members_before: usize, to: HouseholdId) {
    let share = match state.population.households.get_mut(from) {
        Some(h) => h.purse.split_off(1, members_before),
        None => return,
    };
    if let Some(h) = state.population.households.get_mut(to) {
        h.purse.savings_milli = h.purse.savings_milli.saturating_add(share.savings_milli);
    }
}
