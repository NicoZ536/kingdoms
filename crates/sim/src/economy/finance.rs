//! Money flows (docs/game_design.md §7) and bankruptcy.
//!
//! Every game hour one 24th of the daily rates is booked (remainders are carried, so 24 hourly
//! bookings add up to the daily amount exactly):
//! * **taxes:** per household: members × `tax_per_resident` of its house tier × tax rate ×
//!   (100 % + `TaxBonus`), for houses within the radius of an operational building with the
//!   `Administration` service (town hall, castle); homeless households pay nothing,
//! * **market fees:** residents of houses served by an operational, staffed market ×
//!   `GameRules::market_fee_per_resident`,
//!
//! Tax and fee come out of the households' savings ([`crate::population::wealth`]): the
//! treasury books exactly what they paid in the hour; the daily rates forecast what their
//! savings and income cover. The other flows:
//! * **wages:** every employed citizen by the job tier of the building (builders: unskilled),
//! * **upkeep:** every finished building, reduced by `UpkeepReduction`, and the court of the
//!   ruler's rank (`ranks[].court_upkeep`, Hofhaltung; not reduced),
//! * **military pay:** the daily upkeep of every soldier's unit (siege engines: the operator
//!   pays for the crew; docs/game_design.md §11).
//!
//! Households and residents come from [`crate::population`].
//! A negative treasury is a **bankruptcy**: warnings, production at
//! `GameRules::bankruptcy_productivity_percent` and no new construction, until the treasury is
//! positive again. Once per day — in the statistics step (10), after all bookings of the
//! midnight tick — the ledger is closed, prices are updated and statistics are stored
//! (`close_day`).

use serde::{Deserialize, Serialize};

use super::ledger::{LedgerCategory, MILLI_PER_GOLD};
use super::market::{smooth_price, target_price_centi};
use super::trade;
use crate::defs::{Defs, JobTier, TechEffect};
use crate::population::ServiceIndex;
use crate::event::{Notification, Severity, SimEvent};
use crate::state::GameState;

/// Days of expenses below which a shrinking treasury triggers a warning.
pub const LOW_TREASURY_DAYS: i64 = 3;

/// Daily money rates in milli-gold (income positive, expenses negative).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyRates {
    /// Tax income.
    pub taxes: i64,
    /// Market fee income.
    pub market_fees: i64,
    /// Wages (negative).
    pub wages: i64,
    /// Upkeep (negative).
    pub upkeep: i64,
    /// Soldiers' pay (negative).
    pub military: i64,
}

impl DailyRates {
    /// Net daily balance in milli-gold.
    pub fn net(&self) -> i64 {
        self.taxes + self.market_fees + self.wages + self.upkeep + self.military
    }
}

fn effect_sum(state: &GameState, defs: &Defs, f: impl Fn(&TechEffect) -> i32) -> i32 {
    state.research.researched().filter_map(|t| defs.techs().get(t.index())).flat_map(|t| t.effects.iter()).map(f).sum()
}

/// Researched upkeep reduction in percent (0–100).
pub(crate) fn upkeep_reduction_percent(state: &GameState, defs: &Defs) -> i32 {
    effect_sum(state, defs, |e| match e {
        TechEffect::UpkeepReduction { percent } => i32::from(*percent),
        _ => 0,
    })
    .clamp(0, 100)
}

/// Daily wages of a building's assigned workers in gold (builders are paid as unskilled
/// workers); also shown per building by `report::building_economics`.
pub(crate) fn building_wage_gold(defs: &Defs, b: &crate::buildings::Building, def: &crate::defs::BuildingDef) -> f64 {
    if b.staffed() == 0 {
        return 0.0;
    }
    let tier = if b.is_complete() && b.construction_site().is_none() {
        def.job.map_or(JobTier::Unskilled, |j| j.tier)
    } else {
        JobTier::Unskilled
    };
    f64::from(b.staffed()) * f64::from(defs.rules().wages.for_tier(tier))
}

/// The current daily rates.
pub fn daily_rates(state: &GameState, defs: &Defs) -> DailyRates {
    let upkeep_reduction = upkeep_reduction_percent(state, defs);
    // Households pay tax and fee from their savings (docs/game_design.md §6 Wohlstand): the
    // forecast counts what their savings and income cover.
    let services = ServiceIndex::collect(state, defs);
    let (taxes, fees) = crate::population::wealth::expected_daily_payments(state, defs, &services);
    let mut wages = 0.0;
    let mut upkeep: i64 = 0;
    for (_, b) in state.buildings.iter() {
        let Some(def) = defs.buildings().get(b.kind().index()) else { continue };
        wages += building_wage_gold(defs, b, def);
        if b.is_complete() {
            upkeep += i64::from(def.upkeep_per_day) * MILLI_PER_GOLD * i64::from(100 - upkeep_reduction) / 100;
        }
    }
    upkeep += i64::from(crate::ranks::court_upkeep(state, defs)) * MILLI_PER_GOLD;
    let to_milli = |gold: f64| (gold * MILLI_PER_GOLD as f64).round() as i64;
    let military = crate::military::upkeep_daily_pay_milli(state, defs);
    DailyRates { taxes, market_fees: fees, wages: -to_milli(wages), upkeep: -upkeep, military }
}

/// Pipeline step 8: hourly money flows, trade and the bankruptcy check. The daily closing
/// ([`close_day`]) runs in the statistics step (10) after everything else of the tick.
pub(crate) fn run_economy(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let clock = state.clock;
    if !clock.is_new_hour() {
        return;
    }
    let rates = daily_rates(state, defs);
    // Tax and market fee: exactly what the households paid from their savings this hour.
    let paid = crate::population::wealth::run_hour(state, defs);
    let e = &mut state.economy;
    e.book_milli(LedgerCategory::Taxes, paid.taxes);
    e.book_milli(LedgerCategory::MarketFees, paid.fees);
    e.accrue_hour(LedgerCategory::Wages, rates.wages);
    e.accrue_hour(LedgerCategory::Upkeep, rates.upkeep);
    e.accrue_hour(LedgerCategory::Military, rates.military);
    // With AI kingdoms all trade runs through caravans (docs/game_design.md §7); the abstract
    // world market only serves games without kingdoms.
    if state.diplomacy.is_empty() {
        trade::run_trade(state, defs, events);
    }
    check_bankruptcy(state, events);
}

/// Enters or leaves bankruptcy depending on the treasury.
pub(crate) fn check_bankruptcy(state: &mut GameState, events: &mut Vec<SimEvent>) {
    let treasury = state.economy.treasury();
    let bankrupt = state.economy.is_bankrupt();
    if treasury < 0 && !bankrupt {
        state.economy.set_bankrupt(true);
        events.push(SimEvent::BankruptcyStarted { treasury });
        events.push(SimEvent::Notification(Notification::new(
            Severity::Danger,
            "Staatsbankrott! Die Produktion verlangsamt sich und es kann nicht mehr gebaut werden.",
        )));
    } else if treasury >= 0 && bankrupt {
        state.economy.set_bankrupt(false);
        events.push(SimEvent::BankruptcyEnded { treasury });
        events.push(SimEvent::Notification(Notification::new(
            Severity::Good,
            "Die Staatskasse ist wieder gedeckt. Der Bankrott ist überwunden.",
        )));
    }
}

/// Closes the day that just ended and starts day `new_day`: ledger, balance events and
/// warnings, price update, resource statistics snapshot, trade limits. Called by the
/// statistics step ([`crate::statistics`]) at midnight.
pub(crate) fn close_day(state: &mut GameState, defs: &Defs, new_day: u64, events: &mut Vec<SimEvent>) {
    let closed = state.economy.ledger_mut().close_day(new_day);
    let treasury = state.economy.treasury();
    let net = closed.net_milli() / MILLI_PER_GOLD;
    events.push(SimEvent::DailyBalance {
        day: closed.day,
        income: closed.income_milli() / MILLI_PER_GOLD,
        expenses: closed.expenses_milli() / MILLI_PER_GOLD,
        treasury,
    });
    if state.economy.is_bankrupt() {
        events.push(SimEvent::Notification(Notification::new(
            Severity::Danger,
            format!("Die Staatskasse ist leer ({treasury} Gold). Senkt die Ausgaben oder erhöht die Einnahmen."),
        )));
    } else if net < 0 && treasury + LOW_TREASURY_DAYS * net < 0 {
        let days_left = (treasury / -net).max(0) as u32;
        events.push(SimEvent::TreasuryLow { treasury, days_left });
        events.push(SimEvent::Notification(Notification::new(
            Severity::Warning,
            treasury_warning(treasury, days_left),
        )));
    }
    update_prices(state, defs);
    let stock = state.economy.stock().to_counts(defs);
    let prices = state.economy.prices().all().clone();
    state.economy.stats_mut().close_day(defs, closed.day, stock, prices);
    state.economy.trade_mut().reset_day();
}

/// The warning for a treasury that lasts only `days_left` more days at the current balance.
pub fn treasury_warning(treasury: i64, days_left: u32) -> String {
    if treasury <= 0 {
        return "Die Staatskasse ist leer.".to_owned();
    }
    match days_left {
        0 => "Die Staatskasse reicht nicht einmal mehr für 1 Tag.".to_owned(),
        1 => "Die Staatskasse reicht nur noch für etwa 1 Tag.".to_owned(),
        n => format!("Die Staatskasse reicht nur noch für etwa {n} Tage."),
    }
}

/// Moves every price towards its supply/demand target (docs/game_design.md §7).
pub(crate) fn update_prices(state: &mut GameState, defs: &Defs) {
    let rules = defs.rules();
    for r in defs.resources() {
        let today = state.economy.stats().today();
        let supply = u64::from(state.economy.stock().get(r.id))
            + u64::from(today.produced.get(r.id))
            + u64::from(today.imported.get(r.id));
        let demand = u64::from(today.consumed.get(r.id))
            + u64::from(today.exported.get(r.id))
            + u64::from(state.economy.committed().get(r.id));
        let target = target_price_centi(r.base_price, supply, demand, rules);
        let current = state.economy.prices().centi(r.id);
        state.economy.prices_mut().set_centi(r.id, smooth_price(current, target, rules.price_smoothing_percent));
    }
}
