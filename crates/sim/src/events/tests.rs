use super::*;
use crate::buildings::BuildingStatus;
use crate::command::Command;
use crate::defs::{EventCondition, ServiceKind, Weather};
use crate::population::{HappinessFactor, service_bit};
use crate::test_support::{defs, depot, exec, flat_state, link, rejection, run, set_adults, spawn, supply};
use crate::time::{GameClock, Season, TICKS_PER_DAY, TICKS_PER_HOUR};
use crate::weather::WeatherState;
use crate::world::DepositKind;

/// A flat 48 × 48 test state with a town hall at (4, 4) connected to the depot (a wooden
/// warehouse) by a path (no events drawn by themselves).
fn base() -> GameState {
    let mut state = flat_state(48);
    let hall = spawn(&mut state, "town_hall", 4, 4);
    let depot = depot(&state);
    link(&mut state, hall, depot);
    state
}

fn at(state: &mut GameState, season: Season, day: u64) {
    state.clock = GameClock::from_date(1, season, day, 9, 0).expect("valid date");
}

fn def(key: &str) -> &'static EventDef {
    defs().event_by_key(key).expect("event")
}

fn eligible(state: &GameState, key: &str) -> bool {
    is_eligible(state, defs(), def(key))
}

fn trigger(state: &mut GameState, key: &str) -> PendingEvent {
    let e = exec(state, Command::DebugTriggerEvent { event: def(key).id });
    assert!(rejection(&e).is_none(), "{key}: {:?}", rejection(&e));
    assert!(e.iter().any(|e| matches!(e, SimEvent::EventStarted { .. })));
    state.events.pending.expect("pending event")
}

fn choose(state: &mut GameState, choice: u8) -> Vec<SimEvent> {
    let serial = state.events.pending.expect("pending event").serial;
    let e = exec(state, Command::ResolveEvent { serial, choice });
    assert!(rejection(&e).is_none(), "choice {choice}: {:?}", rejection(&e));
    e
}

fn stock(state: &GameState, key: &str) -> u32 {
    state.economy.stock().get(defs().resource_id(key).expect("resource"))
}

/// Gold booked on the events ledger category today.
fn event_gold(state: &GameState) -> i64 {
    state.economy.ledger().today().gold(crate::economy::LedgerCategory::Events)
}

fn outcome(state: &GameState) -> Vec<String> {
    state.events.history().last().map(|r| r.outcome.clone()).unwrap_or_default()
}

fn set_focus(state: &mut GameState, building: BuildingId) {
    if let Some(p) = state.events.pending.as_mut() {
        p.focus = Some(building);
    }
}

/// Marks a column of river tiles at `x` (rows 0..48).
fn river(state: &mut GameState, x: i32) {
    for y in 0..48 {
        state.world.tile_mut(IVec2::new(x, y)).river = true;
    }
}

#[test]
fn conditions_decide_eligibility() {
    let d = defs();
    // Harvest events read the sown fields in autumn: weakened ones rot, healthy ones bear richly
    // (harvest_events_follow_the_fields).
    let mut s = base();
    at(&mut s, Season::Autumn, 5);
    assert!(!eligible(&s, "bad_harvest") && !eligible(&s, "bountiful_harvest"), "no sown fields");
    // Drought: in summer from day 6, after a dry spell of three days (drought_follows_a_dry_spell).
    at(&mut s, Season::Summer, 5);
    s.weather = WeatherState::fixed(Weather::Clear, 60).with_dry_days(3);
    assert!(eligible(&s, "drought"));
    at(&mut s, Season::Spring, 5);
    assert!(!eligible(&s, "drought"));
    // Flood: spring, a building within 3 tiles of a river.
    let mut s = base();
    spawn(&mut s, "house_wood", 27, 20);
    at(&mut s, Season::Spring, 2);
    assert!(!eligible(&s, "flood"), "no river");
    river(&mut s, 31);
    assert!(eligible(&s, "flood"));
    at(&mut s, Season::Summer, 2);
    assert!(!eligible(&s, "flood"), "only in spring");
    // Plague: 60 citizens without a hospital.
    let mut s = base();
    set_adults(&mut s, 59);
    assert!(!eligible(&s, "plague"));
    set_adults(&mut s, 60);
    assert!(eligible(&s, "plague"));
    spawn(&mut s, "hospital", 30, 30);
    assert!(!eligible(&s, "plague"), "a hospital prevents epidemics");
    // Fire: three wooden buildings outside the reach of wells (the depot is one).
    let mut s = base();
    spawn(&mut s, "house_wood", 10, 20);
    assert!(!eligible(&s, "fire"));
    spawn(&mut s, "house_wood", 13, 20);
    assert!(eligible(&s, "fire"));
    assert_eq!(effective_weight(&s, d, def("fire")), 6000, "all wooden buildings unprotected");
    spawn(&mut s, "well", 13, 23);
    assert!(!eligible(&s, "fire"), "the well protects the houses");
    assert_eq!(effective_weight(&s, d, def("fire")), 2000, "one of three wooden buildings unprotected");
    s.research.mark_researched(d.tech_id("architecture_1").expect("tech"));
    s.research.mark_researched(d.tech_id("improved_roofs").expect("tech"));
    assert_eq!(effective_weight(&s, d, def("fire")), 1000, "better roofs halve the risk");
    // Bandits: after the grace period (normal: from the second spring and 40 inhabitants) and
    // little safety.
    let mut s = base();
    set_adults(&mut s, 40);
    at(&mut s, Season::Winter, 6);
    assert!(!eligible(&s, "bandit_raid"), "no bandits in the first year");
    s.clock = GameClock::from_date(2, Season::Spring, 1, 9, 0).expect("valid date");
    assert_eq!(s.clock.total_days(), 24);
    assert!(eligible(&s, "bandit_raid"));
    set_adults(&mut s, 39);
    assert!(!eligible(&s, "bandit_raid"), "a hamlet is not worth a raid");
    set_adults(&mut s, 40);
    // The difficulty moves the grace period: easy with the war grace (day 36), hard from the
    // first winter (day 18).
    s.settings.difficulty = crate::state::Difficulty::Easy;
    assert!(!eligible(&s, "bandit_raid"));
    s.settings.difficulty = crate::state::Difficulty::Hard;
    at(&mut s, Season::Winter, 1);
    assert!(eligible(&s, "bandit_raid"));
    s.settings.difficulty = crate::state::Difficulty::Normal;
    s.clock = GameClock::from_date(2, Season::Spring, 1, 9, 0).expect("valid date");
    for x in [10, 13] {
        spawn(&mut s, "house_wood", x, 20);
    }
    spawn(&mut s, "watchtower", 12, 24);
    assert!(!eligible(&s, "bandit_raid"), "the watchtower guards every house");
    // Merchant: a market from day 4.
    let mut s = base();
    at(&mut s, Season::Spring, 6);
    assert!(!eligible(&s, "merchant_visit"));
    spawn(&mut s, "market", 20, 20);
    assert!(eligible(&s, "merchant_visit"));
    at(&mut s, Season::Spring, 2);
    assert!(!eligible(&s, "merchant_visit"), "not before day 4");
    // Refugees: from day 8 with 4 free places.
    let mut s = base();
    set_adults(&mut s, 2);
    at(&mut s, Season::Summer, 3);
    assert!(!eligible(&s, "refugees"));
    spawn(&mut s, "house_wood", 20, 20);
    spawn(&mut s, "house_wood", 24, 20);
    run(&mut s, 1);
    assert!(eligible(&s, "refugees"));
    // Festival: a church.
    let mut s = base();
    assert!(!eligible(&s, "religious_festival"));
    spawn(&mut s, "church", 20, 20);
    assert!(eligible(&s, "religious_festival"));
    // Unrest: 20 citizens and high taxes or low happiness.
    let mut s = base();
    assert!(!eligible(&s, "political_unrest"), "happy citizens at 10 % tax");
    s.policy.tax_rate_percent = 22;
    assert!(eligible(&s, "political_unrest"));
    s.policy.tax_rate_percent = 10;
    for (_, c) in s.population.citizens.iter_mut() {
        c.happiness = 30 * crate::population::CENTI;
    }
    assert!(eligible(&s, "political_unrest"));
    // New deposits: a quarry or pit from day 12.
    let mut s = base();
    at(&mut s, Season::Autumn, 1);
    assert!(!eligible(&s, "new_deposit"));
    spawn(&mut s, "quarry", 20, 20);
    assert!(eligible(&s, "new_deposit"));
    at(&mut s, Season::Spring, 6);
    assert!(!eligible(&s, "new_deposit"));
    // Storm damage: a storm of at least 50 % and two houses.
    let mut s = base();
    for x in [10, 13] {
        spawn(&mut s, "house_wood", x, 20);
    }
    s.weather = WeatherState::fixed(Weather::Storm, 80);
    assert!(eligible(&s, "storm_damage"));
    s.weather = WeatherState::fixed(Weather::Storm, 40);
    assert!(!eligible(&s, "storm_damage"));
    s.weather = WeatherState::fixed(Weather::Rain, 90);
    assert!(!eligible(&s, "storm_damage"));
}

#[test]
fn cooldown_blocks_repeats() {
    let mut s = base();
    spawn(&mut s, "market", 20, 20);
    at(&mut s, Season::Spring, 5);
    assert!(eligible(&s, "merchant_visit"));
    trigger(&mut s, "merchant_visit");
    choose(&mut s, 0);
    assert!(!eligible(&s, "merchant_visit"), "cooldown of 8 days");
    let today = s.clock.total_days();
    assert_eq!(s.events.cooldown_until(def("merchant_visit"), today), Some(today + 8));
    at(&mut s, Season::Summer, 6);
    assert!(!eligible(&s, "merchant_visit"), "day 7 after");
    at(&mut s, Season::Autumn, 1);
    assert!(eligible(&s, "merchant_visit"), "8 days later");
}

/// Runs the events step for `days` days at the check hour; pending events time out at once.
/// Returns the days on which events occurred.
fn draw_days(state: &mut GameState, days: u64) -> Vec<(u64, EventDefId)> {
    let d = defs();
    let mut occurred = Vec::new();
    for day in 0..days {
        let tick = day * TICKS_PER_DAY + 9 * TICKS_PER_HOUR;
        state.clock = GameClock::at_tick(tick - crate::time::START_HOUR * TICKS_PER_HOUR);
        let mut events = Vec::new();
        run_events(state, d, &mut events);
        if let Some(p) = state.events.pending {
            occurred.push((state.clock.total_days(), p.event));
            state.clock = GameClock::at_tick(p.expires_tick);
            run_events(state, d, &mut events);
            assert!(state.events.pending.is_none());
        }
    }
    occurred
}

#[test]
fn daily_draw_respects_first_day_gap_and_switch() {
    let rules = &defs().rules().events;
    let mut s = base();
    spawn(&mut s, "market", 20, 20);
    assert!(draw_days(&mut s, 60).is_empty(), "events switched off in the fixture");
    let mut s = base();
    spawn(&mut s, "market", 20, 20);
    // A dry spell, so that the summer drought can occur.
    s.weather = WeatherState::fixed(Weather::Clear, 100).with_dry_days(5);
    s.settings.events_enabled = true;
    let occurred = draw_days(&mut s, 120);
    assert!(occurred.len() >= 10, "about a third of the days: {occurred:?}");
    assert!(occurred.iter().all(|(day, _)| *day >= u64::from(rules.first_day)));
    assert!(occurred.windows(2).all(|w| w[1].0 - w[0].0 >= u64::from(rules.min_days_between)), "{occurred:?}");
    // Only eligible events: the merchant (market) and the summer drought.
    let allowed = [def("merchant_visit").id, def("drought").id];
    assert!(occurred.iter().all(|(_, e)| allowed.contains(e)), "{occurred:?}");
    assert!(occurred.iter().any(|(_, e)| *e == def("drought").id));
    // Same seed, same draws.
    let mut again = base();
    spawn(&mut again, "market", 20, 20);
    again.weather = WeatherState::fixed(Weather::Clear, 100).with_dry_days(5);
    again.settings.events_enabled = true;
    assert_eq!(draw_days(&mut again, 120), occurred);
}

#[test]
fn pending_event_expires_with_the_default_choice() {
    let mut s = base();
    let p = trigger(&mut s, "merchant_visit");
    assert_eq!(p.expires_tick - p.started_tick, 12 * TICKS_PER_HOUR);
    let events = run(&mut s, 12 * TICKS_PER_HOUR);
    assert!(events.iter().any(|e| matches!(e, SimEvent::EventResolved { choice: 0, by_default: true, .. })));
    assert!(events.iter().filter_map(SimEvent::notification).any(|n| n.text.contains("Ohne Entscheidung: Kein Interesse")));
    assert!(s.events.pending.is_none());
    let record = s.events.history().last().expect("history");
    assert!(record.by_default && record.choice == 0 && record.serial == p.serial);
}

#[test]
fn commands_check_serial_choice_and_requirements() {
    let mut s = base();
    let p = trigger(&mut s, "merchant_visit");
    let treasury = s.economy.treasury();
    s.economy.add_gold(50 - treasury);
    let e = exec(&mut s, Command::ResolveEvent { serial: p.serial, choice: 1 });
    assert!(matches!(rejection(&e), Some(RejectReason::ChoiceUnavailable { reason }) if reason.starts_with("Es fehlen 1")));
    let e = exec(&mut s, Command::ResolveEvent { serial: p.serial + 1, choice: 0 });
    assert_eq!(rejection(&e), Some(&RejectReason::NoPendingEvent));
    let e = exec(&mut s, Command::ResolveEvent { serial: p.serial, choice: 9 });
    assert_eq!(rejection(&e), Some(&RejectReason::UnknownChoice));
    let e = exec(&mut s, Command::DebugTriggerEvent { event: def("fire").id });
    assert_eq!(rejection(&e), Some(&RejectReason::EventPending));
    let e = exec(&mut s, Command::DebugTriggerEvent { event: EventDefId::from_index(999).expect("id") });
    assert_eq!(rejection(&e), Some(&RejectReason::UnknownEvent));
    assert!(s.events.pending.is_some(), "rejections keep the event pending");
}

#[test]
fn merchant_trades_gold_for_goods() {
    let mut s = base();
    let treasury = s.economy.treasury();
    trigger(&mut s, "merchant_visit");
    choose(&mut s, 1);
    assert_eq!(s.economy.treasury(), treasury - 200);
    assert_eq!(s.economy.ledger().today().gold(crate::economy::LedgerCategory::Events), -200);
    assert_eq!(stock(&s, "spices"), 305);
    trigger(&mut s, "merchant_visit");
    choose(&mut s, 3);
    assert_eq!(stock(&s, "wood"), 260);
    assert_eq!(s.economy.treasury(), treasury + 20);
    let record = s.events.history().last().expect("history");
    assert_eq!(record.outcome, vec!["−40 Holz.".to_owned(), "+220 Gold.".to_owned()]);
}

#[test]
fn harvest_events_modify_field_yields() {
    let mut s = base();
    trigger(&mut s, "bad_harvest");
    choose(&mut s, 1);
    assert_eq!(event_gold(&s), -150);
    assert_eq!(outcome(&s)[..2], ["−150 Gold.".to_owned(), "+40 Brot.".to_owned()]);
    let tick = s.clock.tick;
    assert_eq!(s.events.field_yield_modifier_percent(tick), -30);
    assert_eq!(s.events.field_yield_modifier_percent(tick + 6 * TICKS_PER_DAY), 0, "for six days");
    trigger(&mut s, "bountiful_harvest");
    choose(&mut s, 0);
    let tick = s.clock.tick;
    assert_eq!(s.events.field_yield_modifier_percent(tick), 0, "-30 % and +30 % cancel out");
    assert_eq!(s.events.happiness_modifier(tick), 5);
    run(&mut s, 6 * TICKS_PER_DAY);
    assert!(s.events.modifiers(s.clock.tick).next().is_none(), "all modifiers ran out");
    assert!(s.events.modifiers.is_empty(), "and were removed");
}

/// Sows the first six claimed tiles of `farm` with vegetables in `condition` (percent).
fn sow(state: &mut GameState, farm: BuildingId, condition: u8) {
    let crop = defs().crop_id("vegetables").expect("crop");
    for t in state.agriculture.farm_mut(farm).expect("farm").tiles.iter_mut().take(6) {
        t.crop = Some(crop);
        t.growth = 100;
        t.condition = condition;
    }
}

/// Harvest events follow the state of the sown fields (§12): a bad harvest only where storms
/// and withering weakened them, a bountiful one only on fields in excellent condition – and
/// never both in the same autumn, whatever the fields do in between.
#[test]
fn harvest_events_follow_the_fields() {
    let d = defs();
    let mut s = base();
    let farm = spawn(&mut s, "vegetable_farm", 20, 30);
    run(&mut s, 2);
    assert!(s.agriculture.farm(farm).expect("farm").tiles().len() >= 6, "the farm claimed fields");
    at(&mut s, Season::Autumn, 2);
    assert_eq!(s.agriculture.average_field_condition(), None);
    assert!(!eligible(&s, "bad_harvest") && !eligible(&s, "bountiful_harvest"), "fallow fields");
    sow(&mut s, farm, 100);
    assert_eq!(s.agriculture.average_field_condition(), Some(100));
    assert!(eligible(&s, "bountiful_harvest") && !eligible(&s, "bad_harvest"), "healthy fields");
    sow(&mut s, farm, 70);
    assert!(eligible(&s, "bad_harvest") && !eligible(&s, "bountiful_harvest"), "weakened fields");
    sow(&mut s, farm, 90);
    assert!(!eligible(&s, "bad_harvest") && !eligible(&s, "bountiful_harvest"), "middling fields");
    at(&mut s, Season::Summer, 2);
    sow(&mut s, farm, 70);
    assert!(!eligible(&s, "bad_harvest"), "only in autumn");
    // A bad harvest rules out the bountiful one for the rest of the autumn.
    at(&mut s, Season::Autumn, 2);
    trigger(&mut s, "bad_harvest");
    choose(&mut s, 0);
    sow(&mut s, farm, 100);
    assert!(!eligible(&s, "bountiful_harvest"), "not in the same autumn");
    at(&mut s, Season::Autumn, 6);
    assert!(!eligible(&s, "bountiful_harvest"), "not even on its last day");
    s.clock = GameClock::from_date(2, Season::Autumn, 1, 9, 0).expect("valid date");
    assert!(eligible(&s, "bountiful_harvest"), "the next autumn is a new harvest");
    let text = crate::events::conditions::condition_text(d, &def("bountiful_harvest").conditions[2]);
    assert_eq!(text, "in dieser Jahreszeit noch kein Ereignis „Schlechte Ernte“ oder „Besonders erfolgreiche Ernte“");
    assert_eq!(
        crate::events::conditions::condition_text(d, &def("bad_harvest").conditions[1]),
        "Zustand der bestellten Felder im Schnitt höchstens 85 %"
    );
}

/// The drought event follows a dry spell (§12): three days without rain, storm or snow, and only
/// while the weather has not brought a drought of its own (which the weather already announces).
#[test]
fn drought_follows_a_dry_spell() {
    let d = defs();
    let mut s = base();
    at(&mut s, Season::Summer, 3);
    let weather = |w: Weather, dry: u16| WeatherState::fixed(w, 60).with_dry_days(dry);
    s.weather = weather(Weather::Clear, 1);
    assert!(!eligible(&s, "drought"), "one dry day is no dry spell");
    s.weather = weather(Weather::Cloudy, 2);
    assert!(!eligible(&s, "drought"));
    s.weather = weather(Weather::Cloudy, 3);
    assert!(eligible(&s, "drought"));
    s.weather = weather(Weather::Clear, 9);
    assert!(eligible(&s, "drought"));
    s.weather = weather(Weather::Drought, 5);
    assert!(!eligible(&s, "drought"), "the weather's own drought is already announced");
    s.weather = weather(Weather::Rain, 0);
    assert!(!eligible(&s, "drought"), "it rains");
    assert_eq!(
        crate::events::conditions::condition_text(d, &EventCondition::MinDryDays(3)),
        "seit mindestens 3 Tagen kein Regen"
    );
    assert_eq!(crate::events::conditions::condition_text(d, &EventCondition::MinDryDays(1)), "seit mindestens 1 Tag kein Regen");
    // The dry spell counts the days of the weather itself: rain ends it, a drought continues it.
    let mut w = WeatherState::fixed(Weather::Rain, 60);
    assert_eq!(w.dry_days(), 0);
    let today = s.clock.total_days();
    for (weather, dry) in [(Weather::Clear, 1), (Weather::Cloudy, 2), (Weather::Drought, 3), (Weather::Storm, 0), (Weather::Clear, 1)] {
        w.force(weather, 60, 1, today, None);
        assert_eq!(w.dry_days(), dry, "{weather:?}");
        assert!(w.validate(d).is_ok());
    }
}

#[test]
fn drought_forces_the_weather() {
    let mut s = base();
    trigger(&mut s, "drought");
    let events = choose(&mut s, 1);
    assert!(events.iter().any(|e| matches!(e, SimEvent::WeatherChanged { weather: Weather::Drought, .. })));
    assert_eq!(s.weather.current(), Weather::Drought);
    let forced = s.weather.forced().expect("forced weather");
    assert_eq!(forced.until_day, s.clock.total_days() + 4);
    assert_eq!(forced.event, Some(def("drought").id));
    assert_eq!(s.events.field_yield_modifier_percent(s.clock.tick), 25, "the irrigation choice");
    // The weather report names the event, not a developer command, and the dry spell.
    let d = defs();
    let report = crate::report::weather_report(&s, d);
    assert_eq!(report.forced_text.as_deref(), Some("Wetter vom Ereignis „Dürre“ vorgegeben (noch 4 Tage)"));
    let mut summer = s.clone();
    at(&mut summer, Season::Summer, 2);
    summer.weather = WeatherState::fixed(Weather::Clear, 60).with_dry_days(5);
    assert!(crate::report::weather_report(&summer, d).effects.iter().any(|l| l == "Seit 5 Tagen kein Regen."));
    let e = exec(&mut s, Command::DebugSetWeather { weather: Weather::Rain, intensity: 50, days: 1 });
    assert!(rejection(&e).is_none());
    let report = crate::report::weather_report(&s, d);
    assert_eq!(report.forced_text.as_deref(), Some("Wetter vom Entwicklerbefehl vorgegeben (nur noch heute)"));
    assert_eq!(report.dry_days, 0);
    assert!(!report.effects.iter().any(|l| l.contains("kein Regen")));
}

#[test]
fn flood_damages_buildings_by_the_river_and_destroys_crops() {
    let d = defs();
    let mut s = base();
    river(&mut s, 34);
    let near_a = spawn(&mut s, "house_wood", 30, 10);
    let near_b = spawn(&mut s, "house_wood", 30, 14);
    let far = spawn(&mut s, "house_wood", 12, 30);
    let farm = spawn(&mut s, "vegetable_farm", 27, 22);
    run(&mut s, 2);
    let crop = d.crop_id("vegetables").or_else(|| d.crops().first().map(|c| c.id)).expect("crop");
    let sown: Vec<IVec2> = s.agriculture.farm(farm).expect("farm").tiles().iter().map(|t| t.pos).take(6).collect();
    assert_eq!(sown.len(), 6, "the farm claimed fields");
    for t in s.agriculture.farm_mut(farm).expect("farm").tiles.iter_mut().take(6) {
        t.crop = Some(crop);
        t.growth = 100;
    }
    s.agriculture.take_dirty_chunks();
    let p = trigger(&mut s, "flood");
    assert!(p.focus.is_some_and(|f| f == near_a || f == near_b || f == farm), "focus by the river: {p:?}");
    let events = choose(&mut s, 0);
    for id in [near_a, near_b] {
        assert_eq!(s.building(id).map(|b| b.status()), Some(BuildingStatus::Damaged), "{id:?}");
    }
    assert_eq!(s.building(far).map(|b| b.status()), Some(BuildingStatus::Active));
    assert!(events.iter().any(|e| matches!(e, SimEvent::FieldsDamaged { fields: 6 })));
    let tiles = s.agriculture.farm(farm).expect("farm").tiles();
    assert_eq!(tiles.iter().filter(|t| t.crop.is_some()).count(), 0, "the crops are gone");
    assert!(!s.agriculture.take_dirty_chunks().is_empty(), "the field layer is told");
    assert_eq!(s.weather.current(), Weather::Rain);
    s.validate_entities(d).unwrap();
}

/// The fire names the building it breaks out in (any wooden building, not just a house); without
/// a focus the text speaks of a wooden building in general.
#[test]
fn the_fire_text_names_the_burning_building() {
    let d = defs();
    let mut s = base();
    let hut = spawn(&mut s, "woodcutter", 30, 30);
    let e = exec(&mut s, Command::DebugTriggerEvent { event: def("fire").id });
    assert!(rejection(&e).is_none());
    let focus = s.events.pending.and_then(|p| p.focus).expect("a wooden building burns");
    let named = |b: BuildingId| format!("Im Gebäude „{}“ ist Feuer ausgebrochen!", d.building(s.buildings.get(b).expect("focus").kind()).name);
    let wanted = named(focus);
    assert!(
        e.iter().any(|e| matches!(e, SimEvent::Notification(n) if n.text.starts_with(&format!("Brand: {wanted}")))),
        "the notification names it: {e:?}"
    );
    set_focus(&mut s, hut);
    let report = crate::report::pending_event(&s, d).expect("pending");
    assert!(report.text.starts_with("Im Gebäude „Holzfällerhütte“ ist Feuer ausgebrochen!"), "{}", report.text);
    // Without a focus (nothing that burns, a developer trigger) the general text applies.
    if let Some(p) = s.events.pending.as_mut() {
        p.focus = None;
    }
    let report = crate::report::pending_event(&s, d).expect("pending");
    assert!(report.text.starts_with("In einem Holzgebäude ist Feuer ausgebrochen!"), "{}", report.text);
}

#[test]
fn fire_spreads_to_unprotected_wooden_neighbours_and_is_rebuilt() {
    let d = defs();
    let mut s = base();
    // Within the depot's carrier radius, so carriers can bring the repair material.
    let a = spawn(&mut s, "house_wood", 30, 34);
    let b = spawn(&mut s, "house_wood", 33, 34);
    let far = spawn(&mut s, "house_wood", 20, 34);
    let guarded = spawn(&mut s, "house_wood", 40, 20);
    spawn(&mut s, "well", 42, 18);
    let store = spawn(&mut s, "storehouse", 36, 34);
    let bread = d.resource_id("bread").expect("bread");
    run(&mut s, 2);
    assert!(s.population.resident_count(a) > 0, "the homeless moved in");
    if let Some(st) = s.buildings.get_mut(store) {
        st.inventory.add(bread, 50);
    }
    trigger(&mut s, "fire");
    // The fire starts in the first house.
    set_focus(&mut s, a);
    let events = choose(&mut s, 0);
    let status = |s: &GameState, id| s.building(id).map(|x| x.status());
    assert_eq!(status(&s, a), Some(BuildingStatus::Damaged));
    assert_eq!(status(&s, b), Some(BuildingStatus::Damaged), "spread to the neighbour");
    assert_eq!(status(&s, far), Some(BuildingStatus::Active), "too far away");
    assert_eq!(status(&s, guarded), Some(BuildingStatus::Active), "a well is near");
    assert_eq!(status(&s, store), Some(BuildingStatus::Active), "two buildings at most");
    assert_eq!(events.iter().filter(|e| matches!(e, SimEvent::BuildingDamaged { .. })).count(), 2);
    let house = d.building_by_key("house_wood").expect("house");
    let site = s.building(a).and_then(|x| x.construction_site()).expect("repair site").clone();
    assert_eq!(site.purpose(), crate::buildings::SitePurpose::Repair);
    assert_eq!(site.gold_paid(), 0);
    let needed: Vec<u32> = site.materials_required().iter().map(|m| m.amount).collect();
    let half: Vec<u32> = house.cost.materials.iter().map(|m| m.amount.div_ceil(2)).collect();
    assert_eq!(needed, half, "half the material cost");
    assert_eq!(s.building(a).map(|x| x.sprite(d)), Some("buildings/rubble_2x2".to_owned()));
    run(&mut s, 1);
    assert_eq!(s.population.resident_count(a), 0, "the residents lost their home");
    s.validate_entities(d).unwrap();
    // A burning storehouse loses its goods.
    let mut s2 = base();
    let store = spawn(&mut s2, "storehouse", 16, 20);
    for x in [10, 13] {
        spawn(&mut s2, "house_wood", x, 20);
    }
    if let Some(st) = s2.buildings.get_mut(store) {
        st.inventory.add(bread, 50);
    }
    trigger(&mut s2, "fire");
    set_focus(&mut s2, store);
    let events = choose(&mut s2, 1);
    assert!(events.iter().any(|e| matches!(e, SimEvent::StockLost { lost, .. } if lost[0].amount == 50)));
    assert_eq!(s2.building(store).map(|x| x.inventory().get(bread)), Some(0));
    // Builders rebuild the house.
    supply(&mut s, a);
    let mut repaired = false;
    for _ in 0..(3 * TICKS_PER_DAY) {
        if run(&mut s, 1).iter().any(|e| matches!(e, SimEvent::BuildingRepaired { building, .. } if *building == a)) {
            repaired = true;
            break;
        }
    }
    assert!(repaired, "the repair site was finished");
    assert_eq!(status(&s, a), Some(BuildingStatus::Active));
    s.validate_entities(d).unwrap();
}

#[test]
fn ruins_can_be_demolished_but_not_cancelled() {
    let d = defs();
    let mut s = base();
    let a = spawn(&mut s, "house_wood", 10, 20);
    crate::buildings::damage_building(&mut s, d, a, crate::defs::DamageCause::Storm, &mut Vec::new()).expect("damaged");
    let e = exec(&mut s, Command::CancelConstruction { building: a });
    assert_eq!(rejection(&e), Some(&RejectReason::CannotCancelRepair));
    let refund = crate::buildings::demolish_refund(&s, d, a).expect("refund");
    assert!(refund.materials.is_empty() && !refund.cancels_site, "a ruin returns no building material");
    let e = exec(&mut s, Command::DemolishBuilding { building: a });
    assert!(rejection(&e).is_none());
    assert!(s.building(a).is_none());
    let hall = crate::economy::storage::main_headquarters(&s, d).expect("hall");
    assert!(crate::buildings::damage_building(&mut s, d, hall, crate::defs::DamageCause::Fire, &mut Vec::new()).is_none());
}

#[test]
fn plague_makes_citizens_ill_and_a_hospital_reduces_the_loss() {
    let d = defs();
    let mut s = base();
    set_adults(&mut s, 60);
    // Half the households are covered by a hospital.
    let households: Vec<_> = s.population.households().map(|(id, _)| id).collect();
    for h in households.iter().step_by(2) {
        if let Some(h) = s.population.households.get_mut(*h) {
            h.supply.services |= service_bit(ServiceKind::Health);
        }
    }
    trigger(&mut s, "plague");
    let before: Vec<(crate::population::CitizenId, u8)> = s.population.citizens().map(|(id, c)| (id, c.health())).collect();
    let events = choose(&mut s, 0);
    assert!(events.iter().any(|e| matches!(e, SimEvent::CitizensFellIll { citizens: 9, .. })), "15 % of 60");
    let losses: Vec<u8> = before
        .iter()
        .filter_map(|(id, h)| s.population.citizen(*id).map(|c| h.saturating_sub(c.health())))
        .filter(|loss| *loss >= 9)
        .collect();
    assert_eq!(losses.len(), 9);
    assert!(losses.iter().all(|l| (39..=41).contains(l) || (9..=11).contains(l)), "40 points, with a hospital 25 % of that: {losses:?}");
    assert_eq!(production_percent(&s), 80);
    // Disease resistance research lowers the share.
    let mut s = base();
    set_adults(&mut s, 60);
    s.research.mark_researched(d.tech_id("herbalism").expect("tech"));
    trigger(&mut s, "plague");
    let events = choose(&mut s, 1);
    assert!(events.iter().any(|e| matches!(e, SimEvent::CitizensFellIll { citizens: 3, .. })), "6 % × 80 % of 60");
    assert_eq!(production_percent(&s), 60);
    assert_eq!(s.events.happiness_modifier(s.clock.tick), -8);
    assert_eq!(effective_weight(&s, d, def("plague")), 4000 * 80 / 100);
}

#[test]
fn bandit_raid_sends_real_raiders() {
    let d = defs();
    let mut s = base();
    set_adults(&mut s, 30);
    trigger(&mut s, "bandit_raid");
    let p = crate::report::pending_event(&s, d).expect("pending");
    // The scouts report the band well ahead: it comes 8 h after the event (the event starts at
    // the current hour of the test).
    let warning = u64::from(d.rules().military.bandits.warning_hours) * TICKS_PER_HOUR;
    let expected = crate::report::deadline_clock_text(s.clock.tick, s.clock.tick + warning);
    assert!(
        p.choices[0].effects.iter().any(|l| l.contains(&format!("Räuber greifen gegen {expected} unbewachte Lager an"))),
        "{:?}",
        p.choices[0].effects
    );
    assert!(p.choices[2].effects.iter().any(|l| l.contains("die Räuber nach ihrer Ankunft für 8 h")), "{:?}", p.choices[2].effects);
    // A town without towers is not promised any.
    assert!(p.choices[0].effects.iter().any(|l| l.ends_with("unbewachte Lager an; kein Soldat steht bereit")), "{:?}", p.choices[0].effects);
    let reported = s.events.pending.expect("pending").started_tick;
    let events = choose(&mut s, 0);
    // The band gathers at the map edge (the fight itself: military tests).
    let size = events
        .iter()
        .find_map(|e| if let SimEvent::ThreatApproaching { kingdom: None, size, .. } = e { Some(*size) } else { None })
        .expect("bandits approach");
    assert_eq!(size, crate::military::bandit_band(&s, d));
    assert_eq!(s.military.warbands().len(), 1);
    let arrival = events.iter().find_map(|e| if let SimEvent::ThreatApproaching { arrive_tick, .. } = e { Some(*arrive_tick) } else { None });
    assert_eq!(arrival, Some(reported + warning), "the band keeps the time the scouts reported");
    assert!(
        events.iter().any(|e| matches!(e, SimEvent::Notification(n) if n.text.contains("Sie greift in etwa 8 h an"))),
        "the warning names the time in the game's form: {events:?}"
    );
    assert!(outcome(&s).iter().any(|l| l.contains("Räuber nähern sich")), "{:?}", outcome(&s));
    assert_eq!(s.events.happiness_modifier(s.clock.tick), -3);
    // Soldiers are needed for the third choice.
    trigger(&mut s, "bandit_raid");
    let serial = s.events.pending.expect("pending").serial;
    let e = exec(&mut s, Command::ResolveEvent { serial, choice: 2 });
    assert!(matches!(rejection(&e), Some(RejectReason::ChoiceUnavailable { reason }) if reason == "Es steht kein Soldat bereit (nötig: 5 Soldaten)."));
    choose(&mut s, 1);
    assert_eq!(s.military.warbands().len(), 1, "the ransom keeps the second band away");
    // A late decision still leaves time: at least 2 h after it.
    trigger(&mut s, "bandit_raid");
    run(&mut s, 7 * TICKS_PER_HOUR);
    let events = choose(&mut s, 0);
    let arrival = events.iter().find_map(|e| if let SimEvent::ThreatApproaching { arrive_tick, .. } = e { Some(*arrive_tick) } else { None });
    let delay = u64::from(d.rules().military.bandits.delay_hours) * TICKS_PER_HOUR;
    assert!(arrival.is_some_and(|t| t >= s.clock.tick - 1 + delay), "{arrival:?}");
    s.validate_entities(d).unwrap();
}

#[test]
fn refugees_walk_in_from_the_map_edge() {
    let mut s = base();
    set_adults(&mut s, 2);
    spawn(&mut s, "house_wood", 20, 20);
    spawn(&mut s, "house_wood", 24, 20);
    run(&mut s, 1);
    let before = s.population.len();
    trigger(&mut s, "refugees");
    let events = choose(&mut s, 0);
    let arrived: u16 = events.iter().filter_map(|e| if let SimEvent::HouseholdArrived { members, .. } = e { Some(*members) } else { None }).sum();
    assert_eq!(arrived, 8);
    assert_eq!(s.population.len(), before + 8);
    let homes: Vec<bool> = events
        .iter()
        .filter_map(|e| if let SimEvent::HouseholdArrived { household, .. } = e { Some(*household) } else { None })
        .map(|h| s.population.household(h).is_some_and(|h| h.home().is_some()))
        .collect();
    assert_eq!(homes, vec![true, true], "two families sized to the free places, both housed");
    assert_eq!(s.events.happiness_modifier(s.clock.tick), 3);
    let walking = s.population.citizens().filter(|(_, c)| c.motion().destination().is_some()).count();
    assert!(walking >= 8, "the refugees walk to their new homes");
    s.validate_entities(defs()).unwrap();
}

#[test]
fn festival_shows_in_the_happiness_breakdown() {
    let d = defs();
    let mut s = base();
    spawn(&mut s, "church", 20, 20);
    trigger(&mut s, "religious_festival");
    choose(&mut s, 1);
    let citizen = s.population.citizens().map(|(id, _)| id).next().expect("citizen");
    let parts = crate::population::citizen_happiness_parts(&s, d, citizen).expect("parts");
    let events = parts.iter().find(|p| p.factor == HappinessFactor::Events).map(|p| p.points);
    assert_eq!(events, Some(12.0));
    assert_eq!(HappinessFactor::Events.label(), "Ereignisse");
    run(&mut s, 8 * TICKS_PER_DAY);
    assert_eq!(s.events.happiness_modifier(s.clock.tick), 0, "after eight days");
    if let Some(citizen) = s.population.citizens().map(|(id, _)| id).next() {
        let parts = crate::population::citizen_happiness_parts(&s, d, citizen).expect("parts");
        assert!(parts.iter().all(|p| p.factor != HappinessFactor::Events));
    }
}

#[test]
fn unrest_caps_taxes_and_slows_production() {
    let mut s = base();
    s.policy.tax_rate_percent = 25;
    trigger(&mut s, "political_unrest");
    let events = choose(&mut s, 1);
    assert!(events.iter().any(|e| matches!(e, SimEvent::TaxRateChanged { percent: 10 })));
    let e = exec(&mut s, Command::SetTaxRate { percent: 15 });
    assert!(matches!(rejection(&e), Some(RejectReason::TaxRateCapped { max: 10, .. })));
    assert!(rejection(&exec(&mut s, Command::SetTaxRate { percent: 8 })).is_none());
    run(&mut s, 6 * TICKS_PER_DAY);
    assert!(rejection(&exec(&mut s, Command::SetTaxRate { percent: 15 })).is_none(), "the promise ran out");
    trigger(&mut s, "political_unrest");
    choose(&mut s, 0);
    assert_eq!(production_percent(&s), 85);
    assert_eq!(s.events.happiness_modifier(s.clock.tick), -10);
}

#[test]
fn new_deposit_appears_near_the_headquarters() {
    let d = defs();
    let mut s = base();
    s.world.take_dirty_chunks();
    trigger(&mut s, "new_deposit");
    let events = choose(&mut s, 0);
    let (kind, position, tiles) = events
        .iter()
        .find_map(|e| if let SimEvent::DepositDiscovered { kind, position, tiles } = e { Some((*kind, *position, *tiles)) } else { None })
        .expect("deposit discovered");
    assert!([DepositKind::Stone, DepositKind::Iron, DepositKind::Coal].contains(&kind), "no clay without water: {kind:?}");
    assert!((1..=5).contains(&tiles));
    let hall = s.building(crate::economy::storage::main_headquarters(&s, d).expect("hall")).expect("hall").rect();
    assert!(hall.distance_to(position) <= 18);
    assert_eq!(s.world.tile(position).deposit.map(|x| (x.kind, x.amount)), Some((kind, 400)));
    assert!(!s.world.take_dirty_chunks().is_empty(), "renderers redraw the deposit");
}

#[test]
fn storm_damages_wooden_buildings_only() {
    let d = defs();
    let mut s = base();
    let houses = [spawn(&mut s, "house_wood", 10, 20), spawn(&mut s, "house_wood", 13, 20), spawn(&mut s, "house_wood", 16, 20)];
    let church = spawn(&mut s, "church", 20, 20);
    s.weather = WeatherState::fixed(Weather::Storm, 80);
    trigger(&mut s, "storm_damage");
    set_focus(&mut s, houses[0]);
    choose(&mut s, 1);
    assert_eq!(event_gold(&s), -60);
    assert_eq!(stock(&s, "wood"), 290);
    let damaged = |s: &GameState| houses.iter().filter(|h| s.building(**h).map(|b| b.status()) == Some(BuildingStatus::Damaged)).count();
    assert_eq!(damaged(&s), 1, "securing the roofs limits the damage");
    trigger(&mut s, "storm_damage");
    set_focus(&mut s, houses[1]);
    choose(&mut s, 0);
    assert_eq!(damaged(&s), 3, "the focus and its nearest wooden neighbour");
    assert_eq!(s.building(church).map(|b| b.status()), Some(BuildingStatus::Active), "stone withstands the storm");
    s.validate_entities(d).unwrap();
}

#[test]
fn reports_describe_choices_modifiers_and_history() {
    let d = defs();
    let mut s = base();
    let a = spawn(&mut s, "house_wood", 10, 20);
    spawn(&mut s, "house_wood", 13, 20);
    spawn(&mut s, "house_wood", 16, 20);
    trigger(&mut s, "fire");
    set_focus(&mut s, a);
    let treasury = s.economy.treasury();
    s.economy.add_gold(30 - treasury);
    let report = crate::report::pending_event(&s, d).expect("pending");
    assert_eq!(report.title, "Brand");
    assert!(report.deadline_text.starts_with("Entscheidung bis ") && report.deadline_text.ends_with(", sonst gilt „Löschketten bilden“."), "{}", report.deadline_text);
    assert!(!report.deadline_text.contains("noch"), "the time left is minutes_left: {}", report.deadline_text);
    assert_eq!(report.choices.len(), 2);
    assert!(report.choices[0].is_default && report.choices[0].available);
    assert_eq!(report.choices[0].effects, vec!["Bis zu 2 Holzgebäude ohne Brunnen in der Nähe brennen nieder, zuerst das Gebäude „Einfaches Holzhaus“ – Wiederaufbau durch Bauarbeiter".to_owned()]);
    assert!(!report.choices[1].available);
    assert_eq!(report.choices[1].problem.as_deref(), Some("Es fehlen 30 Gold."));
    assert_eq!(report.choices[1].effects[0], "−60 Gold");
    choose(&mut s, 0);
    s.economy.add_gold(1000);
    spawn(&mut s, "church", 30, 30);
    trigger(&mut s, "religious_festival");
    choose(&mut s, 1);
    let report = crate::report::events_report(&s, d);
    assert!(report.pending.is_none() && !report.enabled);
    assert_eq!(report.happiness_points, 12);
    assert_eq!(report.modifiers.len(), 2);
    assert!(report.modifiers[0].text.starts_with("Festtag (Religiöses Fest), noch 2 Tage"), "{}", report.modifiers[0].text);
    assert!(report.modifiers[1].text.starts_with("Zufriedenheit +12 (Religiöses Fest), noch 8 Tage"), "{}", report.modifiers[1].text);
    assert!(s.events.festival_active(s.clock.tick));
    // Under a day the time left reads in hours and minutes, like every duration of the game.
    let mut copy = s.clone();
    let until = copy.events.modifiers.iter().map(|m| m.until_tick).min().expect("a modifier");
    copy.clock.tick = until - 7 * TICKS_PER_HOUR - TICKS_PER_HOUR / 2;
    let text = crate::report::events_report(&copy, d).modifiers[0].text.clone();
    assert!(text.ends_with(", noch 7 h 30 min"), "{text}");
    assert_eq!(report.history.len(), 2);
    assert_eq!(report.history[0].title, "Religiöses Fest", "newest first");
    assert!(report.history[1].outcome.iter().any(|l| l.contains("niedergebrannt")));
}

#[test]
fn validate_rejects_broken_event_state() {
    let d = defs();
    let mut s = base();
    trigger(&mut s, "merchant_visit");
    s.validate_entities(d).unwrap();
    if let Some(p) = s.events.pending.as_mut() {
        p.event = EventDefId::from_index(999).expect("id");
    }
    assert!(matches!(s.validate_entities(d), Err(crate::state::StateError::InvalidEvents(_))));
    let mut s = base();
    s.events.tax_cap = Some(TaxCap { max_percent: 10, until_tick: 10, source: EventDefId::from_index(999).expect("id") });
    assert!(s.validate_entities(d).is_err());
}

/// Crafted saves must not date anything into the future: a festival day far ahead would block
/// festivals for ages and one near `u64::MAX` overflowed the cooldown arithmetic (panic in the
/// festival report and in `HoldFestival`).
#[test]
fn validate_rejects_event_days_and_ticks_in_the_future() {
    let d = defs();
    let mut s = base();
    at(&mut s, Season::Summer, 3);
    let today = s.clock.total_days();
    let tick = s.clock.tick;
    let invalid = |s: &GameState| matches!(s.validate_entities(d), Err(crate::state::StateError::InvalidEvents(_)));
    let mut ok = s.clone();
    ok.events.last_festival_day = Some(today);
    ok.events.last_event_day = Some(today);
    ok.events.last_occurred.insert(def("fire").id, today);
    ok.validate_entities(d).expect("today is fine");
    for future in [today + 1, 1000, u64::MAX - 1] {
        let mut b = s.clone();
        b.events.last_festival_day = Some(future);
        assert!(invalid(&b), "festival day {future}");
        let mut b = s.clone();
        b.events.last_event_day = Some(future);
        assert!(invalid(&b), "last event day {future}");
        let mut b = s.clone();
        b.events.last_occurred.insert(def("fire").id, future);
        assert!(invalid(&b), "fire last occurred on day {future}");
    }
    let mut b = s.clone();
    trigger(&mut b, "merchant_visit");
    b.validate_entities(d).expect("a fresh pending event");
    let now = b.clock.tick;
    if let Some(p) = b.events.pending.as_mut() {
        p.started_tick = now + 1;
        p.expires_tick = p.expires_tick.max(p.started_tick);
    }
    assert!(invalid(&b), "pending event started in the future");
    let mut b = s.clone();
    trigger(&mut b, "merchant_visit");
    choose(&mut b, 0);
    b.validate_entities(d).expect("a decided event");
    let record = b.events.history.back_mut().expect("history");
    record.resolved_tick = u64::MAX;
    assert!(invalid(&b), "event decided in the future");
    let mut b = s.clone();
    b.events.push_modifier(ActiveModifier {
        kind: ModifierKind::Happiness,
        amount: 5,
        source: ModifierSource::Festival,
        until_tick: tick + u64::from(u16::MAX) * TICKS_PER_DAY,
    });
    b.validate_entities(d).expect("the longest possible modifier");
    b.events.modifiers[0].until_tick += 1;
    assert!(invalid(&b), "a modifier longer than any event makes it");
    // A weather forced by an event names a known one.
    let mut b = s.clone();
    b.weather.force(Weather::Drought, 60, 2, today, Some(def("drought").id));
    b.validate_entities(d).expect("forced by the drought");
    b.weather.force(Weather::Drought, 60, 2, today, EventDefId::from_index(999));
    assert!(matches!(b.validate_entities(d), Err(crate::state::StateError::InvalidWeather(_))));
}

/// The festival day of a crafted save near `u64::MAX` is rejected on load (it used to load
/// and then panic in `festival_report`); the cooldown arithmetic saturates regardless.
#[test]
fn a_save_with_a_festival_day_far_in_the_future_is_rejected_and_the_report_cannot_overflow() {
    use crate::save::{SaveError, SaveHeader, decode, encode};
    let d = defs();
    let mut game = GameState::new(d, crate::state::GameSettings::new(d, 7, crate::world::WorldSize::Small));
    let header = SaveHeader::for_state(&game, d, "Fest", 0);
    decode(&encode(&header, &game).expect("encode"), d).expect("the untouched game loads");
    for day in [1000, u64::MAX - 1] {
        game.events.last_festival_day = Some(day);
        let loaded = decode(&encode(&header, &game).expect("encode"), d);
        assert!(
            matches!(loaded, Err(SaveError::InvalidState(crate::state::StateError::InvalidEvents(_)))),
            "festival day {day} rejected on load: {:?}",
            loaded.err()
        );
    }
    let mut s = base();
    spawn(&mut s, "market", 20, 20);
    s.events.last_festival_day = Some(u64::MAX - 1);
    // The arithmetic itself cannot overflow any more (defence in depth).
    let report = festival::festival_report(&s, d);
    assert_eq!(report.next_day, Some(u64::MAX));
    assert!(report.problem.is_some_and(|p| p.starts_with("Das nächste Fest ist in ")));
    let e = exec(&mut s, Command::HoldFestival);
    assert!(rejection(&e).is_some(), "no festival before the cooldown is over");
    s.events.last_occurred.insert(def("fire").id, u64::MAX - 1);
    s.events.last_event_day = Some(u64::MAX - 1);
    assert!(!eligible(&s, "fire"), "cooldown saturates instead of overflowing");
    run(&mut s, TICKS_PER_DAY);
}

/// The raid warning names the defences that really stand, by type and with the plural.
#[test]
fn the_raid_warning_counts_the_towers_and_posts_that_stand() {
    let d = defs();
    let mut s = base();
    set_adults(&mut s, 30);
    let raid_line = |s: &GameState| -> String {
        let p = crate::report::pending_event(s, d).expect("pending");
        p.choices[0].effects.iter().find(|l| l.contains("Räuber")).cloned().expect("raid line")
    };
    spawn(&mut s, "watchtower", 30, 30);
    trigger(&mut s, "bandit_raid");
    assert!(raid_line(&s).ends_with("; kein Soldat steht bereit, dazu 1 Wachturm"), "{}", raid_line(&s));
    s.events.pending = None;
    spawn(&mut s, "watchtower", 32, 30);
    spawn(&mut s, "archer_post", 34, 34);
    trigger(&mut s, "bandit_raid");
    assert!(raid_line(&s).ends_with(", dazu 2 Wachtürme und 1 Bogenschützenposten"), "{}", raid_line(&s));
    assert_eq!(crate::military::raid_defenders(&s, d), 3);
    s.events.pending = None;
    spawn(&mut s, "stone_tower", 36, 30);
    trigger(&mut s, "bandit_raid");
    assert!(raid_line(&s).ends_with(", dazu 2 Wachtürme, 1 Wehrturm und 1 Bogenschützenposten"), "{}", raid_line(&s));
}

/// A town where every choice of every event is possible: 60 adults, 10 000 gold, wooden houses,
/// houses by a river, church, market, quarry, a vegetable farm and four barracks with 8 ready
/// soldiers (far from the depot).
fn rich_town() -> GameState {
    let mut s = flat_state(48);
    set_adults(&mut s, 60);
    river(&mut s, 34);
    for (key, x, y) in [
        ("house_wood", 30, 10),
        ("house_wood", 30, 14),
        ("house_wood", 10, 20),
        ("house_wood", 13, 20),
        ("house_wood", 16, 20),
        ("church", 20, 28),
        ("market", 24, 20),
        ("quarry", 10, 28),
        ("vegetable_farm", 14, 34),
        ("barracks", 2, 40),
        ("barracks", 6, 40),
        ("barracks", 2, 44),
        ("barracks", 6, 44),
    ] {
        spawn(&mut s, key, x, y);
    }
    let hall = spawn(&mut s, "town_hall", 4, 4);
    let depot = depot(&s);
    link(&mut s, hall, depot);
    let barracks = s.buildings.iter().find(|(_, b)| d_key(b) == "barracks").map(|(id, _)| id).expect("barracks");
    crate::military::test_enlist(&mut s, defs(), 8, defs().unit_id("militia").expect("militia"), barracks);
    run(&mut s, 2);
    s
}

fn d_key(b: &crate::buildings::Building) -> &'static str {
    defs().building(b.kind()).key.as_str()
}

#[test]
fn every_choice_of_every_event_applies_its_effects() {
    use crate::defs::EventEffect;
    let d = defs();
    let town = rich_town();
    assert!(conditions::soldiers(&town) >= 8, "the soldiers are ready");
    for def in d.events() {
        for (i, choice) in def.choices.iter().enumerate() {
            let case = format!("{} / {}", def.key, choice.label);
            let mut s = town.clone();
            let p = trigger(&mut s, &def.key);
            assert_eq!(predicted_effects(&s, d, def, i, p.focus).len(), choice.effects.len(), "{case}");
            let population = s.population.len();
            let events = choose(&mut s, u8::try_from(i).expect("index"));
            let tick = s.clock.tick;
            let lines = outcome(&s);
            assert_eq!(lines.len(), choice.effects.len(), "{case}: one line per effect {lines:?}");
            let gold: i64 = choice.effects.iter().map(|e| if let EventEffect::Gold(g) = e { i64::from(*g) } else { 0 }).sum();
            assert_eq!(event_gold(&s), gold, "{case}");
            for kind in [ModifierKind::Happiness, ModifierKind::Production, ModifierKind::FieldYield] {
                let expected: i32 = choice
                    .effects
                    .iter()
                    .map(|e| match (kind, e) {
                        (ModifierKind::Happiness, EventEffect::Satisfaction { amount, .. }) => i32::from(*amount),
                        (ModifierKind::Production, EventEffect::Production { percent, .. }) => i32::from(*percent),
                        (ModifierKind::FieldYield, EventEffect::FieldYield { percent, .. }) => i32::from(*percent),
                        _ => 0,
                    })
                    .sum();
                assert_eq!(s.events.modifier_sum(kind, tick), expected, "{case}: {kind:?}");
            }
            for effect in &choice.effects {
                match effect {
                    EventEffect::Weather { weather, .. } => assert_eq!(s.weather.current(), *weather, "{case}"),
                    EventEffect::Settlers(n) => assert!(s.population.len() >= population + usize::from(*n), "{case}"),
                    EventEffect::Disease { .. } => {
                        assert!(events.iter().any(|e| matches!(e, SimEvent::CitizensFellIll { citizens, .. } if *citizens > 0)), "{case}");
                    }
                    EventEffect::DamageBuildings { filter, .. } if *filter != crate::defs::BuildingFilter::Fields => {
                        assert!(events.iter().any(|e| matches!(e, SimEvent::BuildingDamaged { .. })), "{case}");
                    }
                    EventEffect::SpawnDeposit { .. } => {
                        assert!(events.iter().any(|e| matches!(e, SimEvent::DepositDiscovered { .. })), "{case}");
                    }
                    EventEffect::TaxRateCap { max_percent, .. } => {
                        assert_eq!(s.events.tax_cap(tick).map(|c| c.max_percent), Some(*max_percent), "{case}");
                    }
                    _ => {}
                }
            }
            s.validate_entities(d).unwrap_or_else(|e| panic!("{case}: {e}"));
        }
    }
}

/// Cost of the research and events steps (and of their reports) in a town of about 900
/// buildings and 1500 citizens with events on and a research queue. Run with
/// `cargo test --release -p kingdoms_sim progression_performance -- --ignored --nocapture`.
#[test]
#[ignore = "performance measurement; run in release mode"]
fn progression_performance() {
    use std::time::Instant;
    let d = defs();
    let mut state = flat_state(192);
    set_adults(&mut state, 1500);
    spawn(&mut state, "town_hall", 2, 2);
    let mut n = 0;
    for y in (10..186).step_by(6) {
        for x in (10..186).step_by(6) {
            let key = match n % 40 {
                0 => "school",
                1 | 21 => "well",
                7 => "market",
                13 => "church",
                _ => "house_wood",
            };
            n += 1;
            let kind = d.building_id(key).expect("type");
            crate::buildings::spawn_finished(&mut state, d, kind, IVec2::new(x, y), crate::buildings::Rotation::default());
        }
    }
    river(&mut state, 100);
    state.settings.events_enabled = true;
    for key in ["grain_farming", "improved_plows", "bookkeeping", "schooling", "mining"] {
        state.tick(d, vec![Command::QueueResearch { tech: d.tech_id(key).expect("tech") }]);
    }
    at(&mut state, Season::Summer, 2);
    run(&mut state, TICKS_PER_HOUR);
    let ticks = 2 * TICKS_PER_DAY;
    let mut sums = [(0u128, 0u128); 2];
    let mut occurred = 0;
    for _ in 0..ticks {
        let steps = state.tick_timed(d);
        for (i, name) in ["research", "events"].into_iter().enumerate() {
            let ns = steps.iter().find(|(n, _)| *n == name).map_or(0, |(_, ns)| *ns);
            sums[i].0 += ns;
            sums[i].1 = sums[i].1.max(ns);
        }
        if let Some(p) = state.events.pending {
            occurred += 1;
            let serial = p.serial;
            state.tick(d, vec![Command::ResolveEvent { serial, choice: 0 }]);
        }
    }
    let start = Instant::now();
    let reports = 20;
    for _ in 0..reports {
        let _ = crate::report::research_report(&state, d);
        let _ = crate::report::events_report(&state, d);
    }
    let report_us = start.elapsed().as_nanos() as f64 / f64::from(reports) / 1000.0;
    // The candidates of a successful daily draw: every condition and weight.
    let start = Instant::now();
    let mut eligible = 0;
    for _ in 0..reports {
        let weights: Vec<u64> =
            d.events().iter().filter(|e| is_eligible(&state, d, e)).map(|e| effective_weight(&state, d, e)).collect();
        eligible = weights.len();
    }
    let draw_us = start.elapsed().as_nanos() as f64 / f64::from(reports) / 1000.0;
    // The heaviest consequences, each in its own tick (trigger + resolve).
    let mut resolve = Vec::new();
    for key in ["fire", "plague", "flood", "storm_damage", "refugees", "new_deposit", "bandit_raid"] {
        state.tick(d, vec![Command::DebugTriggerEvent { event: def(key).id }]);
        let serial = state.events.pending.expect("pending").serial;
        let start = Instant::now();
        state.tick(d, vec![Command::ResolveEvent { serial, choice: 0 }]);
        resolve.push(format!("{key} {:.0} µs", start.elapsed().as_nanos() as f64 / 1000.0));
    }
    eprintln!(
        "{} buildings, {} citizens; over {ticks} ticks: research step avg {:.2} µs, max {:.1} µs; events step avg {:.2} µs, max \
         {:.1} µs; {occurred} drawn events; full draw evaluation ({eligible} candidates) {draw_us:.1} µs; research + events \
         report {report_us:.1} µs; whole tick with a decision: {}",
        state.buildings.len(),
        state.population.len(),
        sums[0].0 as f64 / ticks as f64 / 1000.0,
        sums[0].1 as f64 / 1000.0,
        sums[1].0 as f64 / ticks as f64 / 1000.0,
        sums[1].1 as f64 / 1000.0,
        resolve.join(", "),
    );
    state.validate_entities(d).unwrap();
}
