//! Economics of trade and production (docs/game_design.md §6.3 „Produktionskosten“,
//! „Handelsüberschüsse/-defizite“): the trade balance per day from the ledger, and what a
//! production building costs (wages, upkeep, inputs, tool wear) against the value of what it
//! makes per day with its assigned crew, all at today's market prices.

use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::defs::{BuildingDef, Defs, ResourceId};
use crate::economy::{LedgerCategory, MILLI_PER_GOLD};
use crate::state::GameState;

/// Closed days the trade balance averages over.
pub const TRADE_BALANCE_DAYS: usize = 7;

/// Exports and imports of one day in milli-gold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeDay {
    /// Day number since the start of the game.
    pub day: u64,
    /// Earned by selling goods (≥ 0).
    pub exports_milli: i64,
    /// Spent on buying goods (≥ 0).
    pub imports_milli: i64,
}

impl TradeDay {
    /// Exports − imports (surplus > 0, deficit < 0).
    pub fn balance_milli(&self) -> i64 {
        self.exports_milli - self.imports_milli
    }
}

/// The trade balance: today, the closed days and their recent average.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeBalance {
    /// The running day so far.
    pub today: TradeDay,
    /// Closed days, oldest first (the ledger's history).
    pub days: Vec<TradeDay>,
    /// Average balance per day over the last [`TRADE_BALANCE_DAYS`] closed days (0 without
    /// history).
    pub average_milli: i64,
    /// German summary, e.g. „Handelsüberschuss: im Mittel +12 Gold je Tag“.
    pub text: String,
}

fn trade_day(ledger: &crate::economy::DayLedger) -> TradeDay {
    TradeDay {
        day: ledger.day,
        exports_milli: ledger.milli(LedgerCategory::Exports).max(0),
        imports_milli: (-ledger.milli(LedgerCategory::Imports)).max(0),
    }
}

/// German amount of gold from milli-gold with a sign (rounded to whole gold).
fn signed_gold(milli: i64) -> String {
    let gold = (milli as f64 / MILLI_PER_GOLD as f64).round() as i64;
    match gold {
        0 => "0".to_owned(),
        g if g > 0 => format!("+{g}"),
        g => format!("−{}", -g),
    }
}

/// Exports against imports per day (ledger categories „Ausfuhr“ and „Einfuhr“).
pub fn trade_balance(state: &GameState) -> TradeBalance {
    let ledger = state.economy().ledger();
    let today = trade_day(ledger.today());
    let days: Vec<TradeDay> = ledger.history().map(trade_day).collect();
    let recent: Vec<&TradeDay> = days.iter().rev().take(TRADE_BALANCE_DAYS).collect();
    let average_milli =
        if recent.is_empty() { 0 } else { recent.iter().map(|d| d.balance_milli()).sum::<i64>() / recent.len() as i64 };
    let traded = recent.iter().any(|d| d.exports_milli > 0 || d.imports_milli > 0);
    let span = if recent.len() == 1 { "am letzten Tag".to_owned() } else { format!("im Mittel der letzten {} Tage", recent.len()) };
    let text = if !traded {
        "Noch kein Handel in den letzten Tagen.".to_owned()
    } else if average_milli > 0 {
        format!("Handelsüberschuss: {} Gold je Tag ({span}).", signed_gold(average_milli))
    } else if average_milli < 0 {
        format!("Handelsdefizit: {} Gold je Tag ({span}).", signed_gold(average_milli))
    } else {
        format!("Ausgeglichener Handel ({span}).")
    };
    TradeBalance { today, days, average_milli, text }
}

/// How the yield of [`BuildingEconomics`] was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EconomicsBasis {
    /// From the work rate of the assigned crew over a whole day (buildings with recipes).
    Crew,
    /// From the harvests actually brought in, averaged over the last `days` closed days
    /// (field farms and orchards, whose yield follows the seasons; 0: none closed yet).
    Harvests { days: u32 },
}

/// What a building actually made against what its crew could make (docs/game_design.md §6.3
/// „gemessen“ / „möglich“), from its output record
/// ([`crate::production::ProductionState::output_log`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasuredOutput {
    /// The good counted: the main output (field farms: the most valuable crop made in the
    /// record, else the first crop they can grow).
    pub resource: Option<ResourceId>,
    /// Work cycles (field farms: harvested field tiles) of the last closed day (`None` before
    /// the building's first midnight).
    pub cycles_yesterday: Option<u32>,
    /// Units of [`Self::resource`] made on the last closed day.
    pub units_yesterday: Option<u32>,
    /// Work cycles so far today.
    pub cycles_today: u32,
    /// Units of [`Self::resource`] so far today.
    pub units_today: u32,
    /// Work cycles possible per day with the assigned crew, in thousandths (`None` for field
    /// farms: their yield follows the seasons).
    pub possible_cycles_milli: Option<i64>,
    /// Units of [`Self::resource`] possible per day with the assigned crew, in thousandths.
    pub possible_units_milli: Option<i64>,
    /// Closed days in the record (at most [`crate::production::OUTPUT_HISTORY_DAYS`]).
    pub days: u32,
    /// Average units of [`Self::resource`] per closed day, in thousandths (0 without days).
    pub average_units_milli: i64,
    /// Market value of all goods made per closed day on average, at today's prices
    /// (milli-gold; 0 without days).
    pub average_value_milli: i64,
    /// German summary, e.g. „Gestern 5 Arbeitsgänge (10 Einheiten Bretter), heute bisher 2 –
    /// möglich sind 6,0 je Tag.“
    pub text: String,
}

/// Costs and yield of a production building per day at today's prices (milli-gold).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingEconomics {
    pub building: BuildingId,
    /// Recipe the figures are for (the active one, else the first; `None`: field farms).
    pub recipe: Option<u8>,
    /// How the yield was found: crew potential (recipes) or measured harvests (field farms).
    pub basis: EconomicsBasis,
    /// Work cycles per day at the work rate of the assigned crew (their efficiency, tools,
    /// research, season and weather, bankruptcy, events – independent of the hour), in
    /// thousandths.
    pub cycles_per_day_milli: i64,
    /// Wages of the assigned workers.
    pub wages_milli: i64,
    /// Upkeep (after research reductions).
    pub upkeep_milli: i64,
    /// Market value of the inputs used per day, tool wear included.
    pub inputs_milli: i64,
    /// Market value of the outputs made per day.
    pub outputs_milli: i64,
    /// Outputs − inputs − wages − upkeep.
    pub net_milli: i64,
    /// The main output (largest value per cycle) with its cost per unit (everything above
    /// divided by the units made; `None` while nothing is made) and its market price.
    pub main_output: Option<UnitCost>,
    /// German one-line verdict.
    pub text: String,
    /// What was actually made („gemessen“) against the crew potential („möglich“).
    pub measured: MeasuredOutput,
}

/// Production cost of one unit of a good against its price (milli-gold).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitCost {
    pub resource: ResourceId,
    /// Total daily cost divided by the units made per day (`None`: nothing made).
    pub cost_milli: Option<i64>,
    /// Today's market price.
    pub price_milli: i64,
}

/// Daily upkeep of a finished building of type `def` in milli-gold after research reductions.
fn building_upkeep_milli(state: &GameState, defs: &Defs, def: &BuildingDef) -> i64 {
    let reduction = crate::economy::finance::upkeep_reduction_percent(state, defs);
    i64::from(def.upkeep_per_day) * MILLI_PER_GOLD * i64::from(100 - reduction) / 100
}

/// German decimal with one place („6,0“).
fn decimal_milli(milli: i64) -> String {
    let tenths = (milli as f64 / 100.0).round() as i64;
    let sign = if tenths < 0 { "−" } else { "" };
    format!("{sign}{},{}", tenths.abs() / 10, tenths.abs() % 10)
}

/// „1 Einheit Bretter“ / „10 Einheiten Bretter“.
fn units_text(units: u32, name: &str) -> String {
    if units == 1 { format!("1 Einheit {name}") } else { format!("{units} Einheiten {name}") }
}

/// „1 Arbeitsgang“ / „5 Arbeitsgänge“.
fn cycles_text(cycles: u32) -> String {
    if cycles == 1 { "1 Arbeitsgang".to_owned() } else { format!("{cycles} Arbeitsgänge") }
}

/// The measured output of a building from its record; `possible` are the crew's cycles and
/// units of `resource` per day in thousandths (`None` for field farms).
fn measured_output(
    state: &GameState,
    defs: &Defs,
    log: &crate::production::OutputLog,
    resource: Option<ResourceId>,
    possible: Option<(i64, i64)>,
) -> MeasuredOutput {
    let prices = state.economy().prices();
    let units = |d: &crate::production::DayOutput| resource.map_or(0, |r| d.units_of(r));
    let days = u32::try_from(log.days().len()).unwrap_or(u32::MAX);
    let (mut total_units, mut total_value) = (0i64, 0i64);
    for d in log.days() {
        total_units += i64::from(units(d));
        total_value += d.goods.iter().map(|m| i64::from(m.amount) * i64::from(prices.centi(m.resource)) * 10).sum::<i64>();
    }
    let per_day = |total: i64| if days == 0 { 0 } else { total * 1000 / i64::from(days) };
    let average_units_milli = per_day(total_units);
    let average_value_milli = if days == 0 { 0 } else { total_value / i64::from(days) };
    let name = resource.and_then(|r| defs.resources().get(r.index())).map_or("Waren", |r| r.name.as_str());
    let today = log.today();
    let yesterday = log.yesterday();
    let text = match possible {
        Some((cycles_milli, _)) => {
            let made = match yesterday {
                Some(y) => format!(
                    "Gestern {} ({}), heute bisher {}",
                    cycles_text(y.cycles),
                    units_text(units(y), name),
                    today.cycles
                ),
                None => format!("Heute bisher {} ({})", cycles_text(today.cycles), units_text(units(today), name)),
            };
            if cycles_milli > 0 {
                format!("{made} – möglich sind {} je Tag.", decimal_milli(cycles_milli))
            } else {
                format!("{made} – derzeit ist keine Arbeit möglich.")
            }
        }
        None if days == 0 && units(today) == 0 => "Noch keine Ernte gemessen.".to_owned(),
        None if days == 0 => format!("Heute bisher {} geerntet.", units_text(units(today), name)),
        None => {
            let span = if days == 1 { "am letzten Tag".to_owned() } else { format!("im Mittel der letzten {days} Tage") };
            let unit = if average_units_milli == 1000 { "Einheit" } else { "Einheiten" };
            format!("Ernte {span}: {} {unit} {name} je Tag.", decimal_milli(average_units_milli))
        }
    };
    MeasuredOutput {
        resource,
        cycles_yesterday: yesterday.map(|d| d.cycles),
        units_yesterday: yesterday.map(units),
        cycles_today: today.cycles,
        units_today: units(today),
        possible_cycles_milli: possible.map(|p| p.0),
        possible_units_milli: possible.map(|p| p.1),
        days,
        average_units_milli,
        average_value_milli,
        text,
    }
}

/// Production economics of building `building` (`None` for buildings without production).
/// Buildings with recipes are valued at the crew's work rate over a whole day; field farms and
/// orchards at the harvests of their recent days ([`EconomicsBasis`]). Both carry the measured
/// output ([`MeasuredOutput`]).
pub fn building_economics(state: &GameState, defs: &Defs, building: BuildingId) -> Option<BuildingEconomics> {
    let b = state.building(building)?;
    let p = b.production()?;
    let def = defs.buildings().get(b.kind().index())?;
    let pdef = def.production.as_ref()?;
    let Some(recipe) = pdef.recipes.get(usize::from(p.active_recipe().unwrap_or(0))) else {
        return field_economics(state, defs, building);
    };
    let index = p.active_recipe().unwrap_or(0);
    let prices = state.economy().prices();
    let price = |r: ResourceId| i64::from(prices.centi(r)) * 10;
    // Recipe hours hold for round-the-clock work (the shift factor makes up for the hours off),
    // so the assigned crew's rate gives the cycles of a whole day.
    let rate_permille = if b.is_complete() && !b.is_paused() {
        crate::production::crew_rate_permille(state, defs, def, building)
    } else {
        0
    };
    let hours = f64::from(recipe.hours.max(0.01));
    let cycles = 24.0 / hours * rate_permille as f64 / 1000.0;
    let per_day = |amount: f32| f64::from(amount) * cycles;
    let mut inputs = recipe.inputs.iter().map(|r| per_day(r.amount) * price(r.resource) as f64).sum::<f64>();
    if pdef.uses_tools
        && let Some(tools) = defs.resource_id("tools")
    {
        inputs += cycles / f64::from(defs.rules().tool_wear_cycles.max(1)) * price(tools) as f64;
    }
    let outputs = recipe.outputs.iter().map(|r| per_day(r.amount) * price(r.resource) as f64).sum::<f64>();
    let wages = (crate::economy::finance::building_wage_gold(defs, b, def) * MILLI_PER_GOLD as f64).round() as i64;
    let upkeep = if b.is_complete() { building_upkeep_milli(state, defs, def) } else { 0 };
    let (inputs, outputs) = (inputs.round() as i64, outputs.round() as i64);
    let net = outputs - inputs - wages - upkeep;
    let main = recipe.outputs.iter().max_by_key(|r| (f64::from(r.amount) * price(r.resource) as f64).round() as i64);
    let main_output = main.map(|r| {
        let units = per_day(r.amount);
        let cost = (units > 1e-6).then(|| ((inputs + wages + upkeep) as f64 / units).round() as i64);
        UnitCost { resource: r.resource, cost_milli: cost, price_milli: price(r.resource) }
    });
    let text = if cycles <= 0.0 {
        format!("Steht still – kostet {} Gold je Tag ohne Ertrag.", signed_gold(-(wages + upkeep)).trim_start_matches('+'))
    } else if net > 0 {
        format!("Lohnt sich: {} Gold je Tag über den Kosten.", signed_gold(net))
    } else if net < 0 {
        format!("Kostet mehr, als es einbringt: {} Gold je Tag.", signed_gold(net))
    } else {
        "Deckt gerade seine Kosten.".to_owned()
    };
    let possible_units = main.map_or(0, |r| (per_day(r.amount) * 1000.0).round() as i64);
    let cycles_milli = (cycles * 1000.0).round() as i64;
    let measured = measured_output(state, defs, p.output_log(), main.map(|r| r.resource), Some((cycles_milli, possible_units)));
    Some(BuildingEconomics {
        building,
        recipe: Some(index),
        basis: EconomicsBasis::Crew,
        cycles_per_day_milli: cycles_milli,
        wages_milli: wages,
        upkeep_milli: upkeep,
        inputs_milli: inputs,
        outputs_milli: outputs,
        net_milli: net,
        main_output,
        text,
        measured,
    })
}

/// Economics of a field farm or orchard from its recent harvests: the average value of the
/// goods brought in per closed day against wages and upkeep.
fn field_economics(state: &GameState, defs: &Defs, building: BuildingId) -> Option<BuildingEconomics> {
    let b = state.building(building)?;
    let p = b.production()?;
    let def = defs.buildings().get(b.kind().index())?;
    let crate::defs::Gathering::Fields { crops, .. } = def.production.as_ref()?.gathering.as_ref()? else { return None };
    let log = p.output_log();
    let prices = state.economy().prices();
    let price = |r: ResourceId| i64::from(prices.centi(r)) * 10;
    // The crop worth most in the record, else the first the farm can grow.
    let mut made: Vec<(ResourceId, i64)> = Vec::new();
    for m in log.days().flat_map(|d| d.goods.iter()) {
        match made.iter_mut().find(|(r, _)| *r == m.resource) {
            Some((_, n)) => *n += i64::from(m.amount),
            None => made.push((m.resource, i64::from(m.amount))),
        }
    }
    let resource = made
        .iter()
        .max_by_key(|(r, n)| (n * price(*r), std::cmp::Reverse(*r)))
        .map(|(r, _)| *r)
        .or_else(|| crops.iter().filter_map(|c| defs.crops().get(c.index())).map(|c| c.output).next());
    let measured = measured_output(state, defs, log, resource, None);
    let wages = (crate::economy::finance::building_wage_gold(defs, b, def) * MILLI_PER_GOLD as f64).round() as i64;
    let upkeep = if b.is_complete() { building_upkeep_milli(state, defs, def) } else { 0 };
    let outputs = measured.average_value_milli;
    let net = outputs - wages - upkeep;
    let main_output = resource.map(|r| {
        let cost = (measured.average_units_milli > 0).then(|| (wages + upkeep) * 1000 / measured.average_units_milli);
        UnitCost { resource: r, cost_milli: cost, price_milli: price(r) }
    });
    let text = if measured.days == 0 {
        "Noch kein voller Tag gemessen – die Ernte folgt den Jahreszeiten.".to_owned()
    } else if net > 0 {
        format!("Lohnt sich: {} Gold je Tag über den Kosten (Mittel der Ernten).", signed_gold(net))
    } else if net < 0 {
        format!("Kostet mehr, als die Ernten einbringen: {} Gold je Tag.", signed_gold(net))
    } else {
        "Die Ernten decken gerade die Kosten.".to_owned()
    };
    Some(BuildingEconomics {
        building,
        recipe: None,
        basis: EconomicsBasis::Harvests { days: measured.days },
        cycles_per_day_milli: if measured.days == 0 {
            0
        } else {
            log.days().map(|d| i64::from(d.cycles)).sum::<i64>() * 1000 / i64::from(measured.days)
        },
        wages_milli: wages,
        upkeep_milli: upkeep,
        inputs_milli: 0,
        outputs_milli: outputs,
        net_milli: net,
        main_output,
        text,
        measured,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::test_support::{defs, exec, flat_state, run, spawn};

    #[test]
    fn the_trade_balance_follows_exports_and_imports() {
        let mut s = flat_state(32);
        let b = trade_balance(&s);
        assert_eq!(b.average_milli, 0);
        assert!(b.text.contains("Noch kein Handel"), "{}", b.text);
        // Two closed days: one with a surplus of 30 gold, one with a deficit of 10.
        s.economy.ledger_mut().book(LedgerCategory::Exports, 40_000);
        s.economy.ledger_mut().book(LedgerCategory::Imports, -10_000);
        s.economy.ledger_mut().close_day(1);
        s.economy.ledger_mut().book(LedgerCategory::Imports, -10_000);
        s.economy.ledger_mut().close_day(2);
        s.economy.ledger_mut().book(LedgerCategory::Exports, 5_000);
        let b = trade_balance(&s);
        assert_eq!(b.days.len(), 2);
        assert_eq!(b.days[0].balance_milli(), 30_000);
        assert_eq!(b.days[1].balance_milli(), -10_000);
        assert_eq!(b.today.exports_milli, 5_000);
        assert_eq!(b.average_milli, 10_000);
        assert_eq!(b.text, "Handelsüberschuss: +10 Gold je Tag (im Mittel der letzten 2 Tage).");
        s.economy.ledger_mut().book(LedgerCategory::Imports, -200_000);
        s.economy.ledger_mut().close_day(3);
        let b = trade_balance(&s);
        assert!(b.average_milli < 0 && b.text.starts_with("Handelsdefizit: −"), "{}", b.text);
    }

    #[test]
    fn production_costs_are_weighed_against_the_output() {
        let d = defs();
        let mut s = flat_state(48);
        let hall = spawn(&mut s, "town_hall", 20, 20);
        let sawmill = spawn(&mut s, "sawmill", 26, 20);
        crate::test_support::link(&mut s, hall, sawmill);
        s.workforce.mark_dirty();
        run(&mut s, 2);
        let e = building_economics(&s, d, sawmill).expect("a sawmill has a recipe");
        let def = d.buildings().get(s.building(sawmill).unwrap().kind().index()).unwrap();
        assert_eq!(e.upkeep_milli, i64::from(def.upkeep_per_day) * MILLI_PER_GOLD);
        assert_eq!(e.net_milli, e.outputs_milli - e.inputs_milli - e.wages_milli - e.upkeep_milli);
        let main = e.main_output.expect("planks");
        assert_eq!(Some(main.resource), d.resource_id("planks"));
        assert!(main.price_milli > 0);
        let staffed = s.building(sawmill).unwrap().staffed();
        assert!(staffed > 0, "the town's people work at the sawmill");
        // With its crew the sawmill makes planks all day; the figures do not depend on the hour.
        assert!(e.cycles_per_day_milli > 0 && e.outputs_milli > 0 && e.inputs_milli > 0, "{e:?}");
        assert!(main.cost_milli.is_some());
        run(&mut s, crate::time::TICKS_PER_HOUR * 9);
        let later = building_economics(&s, d, sawmill).expect("still a sawmill");
        if s.building(sawmill).unwrap().staffed() == staffed {
            assert_eq!(later.cycles_per_day_milli, e.cycles_per_day_milli);
        }
        // Paused: no cycles, only the running costs.
        exec(&mut s, Command::SetBuildingPaused { building: sawmill, paused: true });
        let paused = building_economics(&s, d, sawmill).expect("still a sawmill");
        assert_eq!(paused.cycles_per_day_milli, 0);
        assert_eq!(paused.outputs_milli, 0);
        assert!(paused.text.starts_with("Steht still"), "{}", paused.text);
        // Buildings without production have no economics.
        assert!(building_economics(&s, d, hall).is_none());
    }

    #[test]
    fn measured_output_counts_what_was_made_against_the_possible() {
        let d = defs();
        let mut s = flat_state(48);
        let hall = spawn(&mut s, "town_hall", 20, 20);
        let sawmill = spawn(&mut s, "sawmill", 26, 20);
        crate::test_support::link(&mut s, hall, sawmill);
        let depot = crate::test_support::depot(&s);
        crate::test_support::link(&mut s, sawmill, depot);
        s.workforce.mark_dirty();
        run(&mut s, 2);
        let e = building_economics(&s, d, sawmill).unwrap();
        assert_eq!(e.basis, EconomicsBasis::Crew);
        assert_eq!(e.measured.cycles_yesterday, None, "no midnight yet");
        assert_eq!(e.measured.possible_cycles_milli, Some(e.cycles_per_day_milli));
        assert!(e.measured.text.starts_with("Heute bisher 0 Arbeitsgänge (0 Einheiten Bretter) – möglich sind"), "{}", e.measured.text);
        // Two days later: the record holds every completed cycle and the planks they made.
        run(&mut s, 2 * crate::time::TICKS_PER_DAY);
        let p = s.building(sawmill).unwrap().production().unwrap();
        let log = p.output_log();
        assert_eq!(log.days().len(), 2);
        let cycles: u64 = log.days().chain([log.today()]).map(|d| u64::from(d.cycles)).sum();
        assert_eq!(cycles, p.cycles_completed(), "every cycle is counted once");
        let planks = d.resource_id("planks").unwrap();
        let e = building_economics(&s, d, sawmill).unwrap();
        let yesterday = log.yesterday().unwrap();
        assert!(yesterday.cycles > 0, "the sawmill worked yesterday: {log:?}");
        assert_eq!(e.measured.cycles_yesterday, Some(yesterday.cycles));
        assert_eq!(e.measured.units_yesterday, Some(yesterday.units_of(planks)));
        assert_eq!(e.measured.resource, Some(planks));
        assert!(e.measured.average_value_milli > 0 && e.measured.average_units_milli > 0);
        // Measured cannot beat the crew's round-the-clock potential by much (the potential
        // already allows for the hours off).
        assert!(i64::from(yesterday.cycles) * 1000 <= e.cycles_per_day_milli * 3 / 2 + 1000, "{e:?}");
        assert!(e.measured.text.starts_with("Gestern "), "{}", e.measured.text);
        assert!(e.measured.text.contains("heute bisher") && e.measured.text.contains("möglich sind"), "{}", e.measured.text);
        s.validate_entities(d).unwrap();
    }

    #[test]
    fn german_counts_and_decimals() {
        assert_eq!(decimal_milli(6000), "6,0");
        assert_eq!(decimal_milli(2_449), "2,4");
        assert_eq!(decimal_milli(-1_250), "−1,3");
        assert_eq!(units_text(1, "Bretter"), "1 Einheit Bretter");
        assert_eq!(units_text(0, "Bretter"), "0 Einheiten Bretter");
        assert_eq!(cycles_text(1), "1 Arbeitsgang");
        assert_eq!(cycles_text(5), "5 Arbeitsgänge");
    }
}
