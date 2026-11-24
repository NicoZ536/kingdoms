//! Balance: a scripted "reasonable player" on the default scenario, using only commands, for a
//! whole year including winter (docs/game_design.md §8).
//!
//! The player builds what an attentive beginner would build with the buildings available at
//! the start (no research yet): a well, woodcutters and a forester (construction and winter
//! fuel), two vegetable farms sown in spring (two harvests for the early one), chicken farms
//! and a fishery, a market, storehouses and a warehouse for the harvest, a quarry, and new
//! houses whenever the existing ones fill up, each connected by a path. Beyond the basics they
//! only open workplaces for which adults are available (two stay free as builders), so carriers
//! and builders are never short. After the year the kingdom
//! must have grown, the fields must have been harvested twice, nobody may have starved or
//! frozen, the food (harvest in storage, animals, fish) must have carried everybody through
//! winter with households always fed, the treasury must not have collapsed at the default tax
//! rate and the average happiness must be in a sane range.
//!
//! Events with choices are on (docs/game_design.md §12, the default): the reasonable player
//! decides every event within the hour, preferring the choice that limits damage as long as
//! the treasury keeps a reserve of 250 gold, and at least one event must occur in the year.
//!
//! A second, naive player follows a fixed beginner build order that opens more workplaces than
//! there are adults: the base staffing of storages and markets must keep the goods moving and
//! the settlement fed (docs/game_design.md §6.1, Arbeit). It never decides events, so their
//! first choices apply when the time runs out.

mod common;

use common::{decide_event, kind, load_defs, next_goal, plan};
use kingdoms_sim::defs::Defs;
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::population::DeathCause;
use kingdoms_sim::report;
use kingdoms_sim::state::{GameSettings, GameState};
use kingdoms_sim::time::{Season, TICKS_PER_DAY, TICKS_PER_HOUR};
use kingdoms_sim::world::WorldSize;

/// A full year (24 days) plus the first day of the next spring.
const DAYS: u64 = 25;
const _: () = assert!(DAYS > 24, "the balance check covers a whole year including winter");

#[test]
fn reasonable_player_grows_a_viable_kingdom() {
    let defs = load_defs();
    // Seed 7 by default; other seeds can be checked with KINGDOMS_BALANCE_SEED=<n>.
    let seed = std::env::var("KINGDOMS_BALANCE_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(7);
    let mut state = GameState::new(&defs, GameSettings::new(&defs, seed, WorldSize::Small));
    let start_population = state.population().len();
    let vegetables = defs.resource_id("vegetables").expect("vegetables");
    let wood = defs.resource_id("wood").expect("wood");
    let mut deaths = Vec::new();
    let mut blocked = Vec::new();
    let mut harvests: Vec<(u64, u32)> = Vec::new();
    let mut frozen_fields = 0;
    let mut min_treasury = state.economy().treasury();
    let mut min_hunger = 100;
    let mut min_warmth = 100;
    let mut winter_forecast = None;
    let mut weather_days = std::collections::BTreeMap::new();
    let mut occurred = Vec::new();
    for tick in 0..DAYS * TICKS_PER_DAY {
        // The player acts every game hour; goals without a valid spot are tried again later.
        let mut commands = if tick % TICKS_PER_HOUR == 1 { plan(&state, &defs, &mut blocked, next_goal) } else { Vec::new() };
        if tick % TICKS_PER_HOUR == 1 {
            commands.extend(decide_event(&state, &defs));
        }
        for e in state.tick(&defs, commands) {
            match e {
                SimEvent::EventStarted { event, .. } => occurred.push((tick / TICKS_PER_DAY, defs.event(event).key.clone())),
                SimEvent::CitizenDied { cause, age, .. } => deaths.push((tick / TICKS_PER_DAY, cause, age)),
                SimEvent::HarvestCompleted { resource, amount, .. } if resource == vegetables => {
                    harvests.push((tick / TICKS_PER_DAY, amount));
                }
                SimEvent::CropsFrozen { fields } => frozen_fields += fields,
                SimEvent::WeatherChanged { weather, .. } => *weather_days.entry(format!("{weather:?}")).or_insert(0) += 1,
                _ => {}
            }
        }
        min_treasury = min_treasury.min(state.economy().treasury());
        if tick % TICKS_PER_DAY == 0 && tick > 0 {
            let s = report::population_summary(&state, &defs);
            let needs = report::needs_summary(&state, &defs);
            let hunger = needs.needs[0].average;
            let warmth = needs.needs[3].average;
            min_hunger = min_hunger.min(hunger);
            min_warmth = min_warmth.min(warmth);
            let forecast = report::food_forecast(&state, &defs);
            if state.clock().season() == Season::Winter && winter_forecast.is_none() {
                winter_forecast = Some(forecast.clone());
            }
            let fields = report::agriculture_summary(&state, &defs);
            let logistics = report::logistics_report(&state, &defs);
            eprintln!(
                "day {:>2} {:<8} {:<20}: pop {:>3} (k {}, a {}, e {}), homeless {}, unempl {}, food {:>5.0} sat (barns {:.0}, \
                 expected {:.0}), veg {}, wood {}, hunger {hunger}, warmth {warmth}, gold {}, happy {}, health {}, \
                 fields {} (fallow {}), forecast {:.0}, carriers {}/{} waiting {} trip {:?} min, woodcutters {}, food stores {}",
                tick / TICKS_PER_DAY,
                state.clock().season().display_name(),
                state.weather().description(),
                s.population,
                s.stages[0].total(),
                s.stages[1].total(),
                s.stages[2].total(),
                s.homeless,
                s.unemployed,
                forecast.in_stock,
                forecast.in_barns,
                forecast.expected_harvest,
                state.economy().stock().get(vegetables),
                state.economy().stock().get(wood),
                state.economy().treasury(),
                s.average_happiness,
                s.average_health,
                fields.fields,
                fields.fallow,
                forecast.after_winter,
                logistics.busy_carriers,
                logistics.carriers,
                logistics.waiting,
                logistics.average_trip_minutes,
                state.buildings().count_of(kind(&defs, "woodcutter")),
                state.buildings().count_of(kind(&defs, "storehouse")),
            );
        }
    }
    let summary = report::population_summary(&state, &defs);
    let happiness = report::happiness_report(&state, &defs);
    let factors: Vec<String> = happiness.factors.iter().map(|f| format!("{} {:.1}", f.label, f.points)).collect();
    eprintln!("happiness factors: {}", factors.join(", "));
    eprintln!("vegetable harvests (day, units): {harvests:?}; frozen fields: {frozen_fields}; weather days: {weather_days:?}");
    eprintln!("deaths: {deaths:?}; goals without a spot at the end: {blocked:?}; lowest treasury {min_treasury}");
    eprintln!("forecast at the start of winter: {winter_forecast:?}");
    eprintln!("events (day, id): {occurred:?}");
    for r in state.events().history() {
        let def = defs.event(r.event);
        eprintln!("  {} -> {} (default: {}): {}", def.title, def.choices[usize::from(r.choice)].label, r.by_default, r.outcome.join(" "));
    }
    let walking = report::walking_summary(&state);
    eprintln!("walking: {walking:?}; pathfinder: {:?}", state.navigation().stats());
    state.validate(&defs).expect("consistent state");
    // Events (docs/game_design.md §12) were on and something happened.
    assert!(state.settings().events_enabled);
    assert!(!occurred.is_empty(), "at least one event in a year");
    // Walking (docs/game_design.md §6): commutes are noticeable but not crippling, and
    // everybody finds a way.
    let commute = walking.average_commute_minutes.expect("workers walked to work");
    assert!((3..=60).contains(&commute), "average commute {commute} min");
    assert_eq!(walking.no_path, 0, "nobody is stuck without a path");

    assert!(summary.population as usize >= start_population + 10, "population grew from {start_population} to {}", summary.population);
    assert!(summary.homeless == 0 && summary.today.emigrants == 0);
    assert!(summary.history.iter().all(|d| d.counts.emigrants == 0), "nobody left the kingdom");
    // Agriculture: the vegetable fields sown at the start of spring were harvested early in
    // summer, sown again and harvested a second time (ripe before the frost).
    assert!(harvests.iter().any(|(day, _)| (6..12).contains(day)), "summer harvest: {harvests:?}");
    assert!(harvests.len() >= 2, "a second harvest from the summer sowing: {harvests:?}");
    assert_eq!(frozen_fields, 0, "nothing was sown too late to ripen before the frost");
    let harvested: u32 = harvests.iter().map(|(_, n)| n).sum();
    assert!(harvested >= 900, "the fields fed the stores: {harvested} vegetables");
    // Winter: the stores (harvest, animals, fish) carried everybody through; nobody starved or
    // froze and the households stayed fed and warm.
    let forecast = winter_forecast.expect("winter was reached");
    assert!(forecast.covers_winter, "the forecast at the start of winter expected enough food: {forecast:?}");
    assert!(min_hunger >= 80, "households stayed fed (lowest average hunger {min_hunger})");
    assert!(min_warmth >= 60, "households had firewood in winter (lowest average warmth {min_warmth})");
    assert!(
        deaths.iter().all(|(_, cause, _)| !matches!(cause, DeathCause::Starvation | DeathCause::Freezing)),
        "nobody starved or froze: {deaths:?}"
    );
    // Money: no collapse at the default tax rate, and the running costs are covered.
    assert_eq!(state.policy().tax_rate_percent, defs.scenario(defs.default_scenario()).tax_rate_percent);
    assert!(!state.economy().is_bankrupt() && min_treasury > 0, "treasury did not collapse (lowest {min_treasury})");
    let rates = report::finance_report(&state, &defs).rates;
    eprintln!("daily rates (milli-gold): {rates:?}, net {}", rates.net());
    assert!(rates.net() > 0, "taxes and fees cover wages and upkeep: {rates:?}");
    assert!(
        (55..=95).contains(&summary.average_happiness),
        "average happiness {} in a sane range",
        summary.average_happiness
    );
}

/// A beginner's build order that ignores how many adults there are (review finding: with
/// carriers staffed last this starved the whole start settlement within a week).
fn naive_next_goal(state: &GameState, defs: &Defs, blocked: &[&str]) -> Option<&'static str> {
    let count = |key: &str| state.buildings().count_of(kind(defs, key));
    let open = |key: &str| !blocked.contains(&key);
    let house = kind(defs, "house_wood");
    let building_houses = state.buildings().iter().filter(|(_, b)| b.kind() == house && b.construction_site().is_some()).count();
    if report::population_summary(state, defs).homeless > 0 && building_houses == 0 && open("house_wood") {
        return Some("house_wood");
    }
    let order: [(&'static str, usize); 9] = [
        ("well", 1),
        ("woodcutter", 1),
        ("chicken_farm", 1),
        ("market", 1),
        ("vegetable_farm", 1),
        ("woodcutter", 2),
        ("forester", 1),
        ("chicken_farm", 2),
        ("fishery", 1),
    ];
    order
        .iter()
        .find(|(key, n)| {
            count(key) < *n && open(key) && state.economy().treasury() >= i64::from(defs.building(kind(defs, key)).cost.gold) + 20
        })
        .map(|(key, _)| *key)
}

#[test]
fn naive_build_order_keeps_carriers_and_feeds_the_settlement() {
    const NAIVE_DAYS: u64 = 10;
    let defs = load_defs();
    let mut state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
    let start_population = state.population().len();
    let market = kind(&defs, "market");
    let hall = kind(&defs, "town_hall");
    let mut blocked = Vec::new();
    let mut deaths = Vec::new();
    let mut min_hunger = 100;
    let mut max_workplaces = 0;
    for tick in 0..NAIVE_DAYS * TICKS_PER_DAY {
        let commands = if tick % TICKS_PER_HOUR == 1 { plan(&state, &defs, &mut blocked, naive_next_goal) } else { Vec::new() };
        for e in state.tick(&defs, commands) {
            if let SimEvent::CitizenDied { cause, age, .. } = e {
                deaths.push((tick / TICKS_PER_DAY, cause, age));
            }
        }
        if tick % TICKS_PER_HOUR == 0 && tick > 0 {
            let staffed = |k| state.buildings().iter().find(|(_, b)| b.kind() == k && b.is_complete()).map(|(_, b)| b.staffed());
            assert!(staffed(hall).is_some_and(|n| n >= 1), "tick {tick}: the town hall keeps a carrier");
            assert!(staffed(market).is_none_or(|n| n >= 1), "tick {tick}: a finished market keeps a worker");
            let summary = kingdoms_sim::workforce::summary(&state, &defs);
            let wanted: u32 = summary.categories.iter().map(|c| c.wanted).sum();
            max_workplaces = max_workplaces.max(wanted.saturating_sub(summary.working_age));
        }
        if tick % TICKS_PER_DAY == 0 && tick > 0 {
            min_hunger = min_hunger.min(report::needs_summary(&state, &defs).needs[0].average);
        }
    }
    let summary = report::population_summary(&state, &defs);
    eprintln!(
        "naive player after {NAIVE_DAYS} days: pop {} (start {start_population}), deaths {deaths:?}, lowest hunger {min_hunger}, \
         most workplaces beyond the adults {max_workplaces}",
        summary.population
    );
    assert!(max_workplaces > 0, "the script opens more workplaces than there are adults");
    assert!(
        deaths.iter().all(|(_, cause, _)| !matches!(cause, DeathCause::Starvation | DeathCause::Freezing)),
        "nobody starved: {deaths:?}"
    );
    assert!(min_hunger >= 80, "households stayed fed (lowest average hunger {min_hunger})");
    assert!(summary.population as usize >= start_population, "the settlement did not shrink");
}
