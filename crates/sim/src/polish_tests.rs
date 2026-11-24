//! Tests of the presentation helpers and rules of sim-lane step 6: who is inside a building,
//! construction-site work, house heating, wind, herds, queueing research with prerequisites,
//! the game-over answer of the planners and positions of losses.

use glam::IVec2;

use crate::buildings::SiteWork;
use crate::command::Command;
use crate::defs::Weather;
use crate::event::SimEvent;
use crate::population::Heating;
use crate::test_support::{defs, exec, flat_state, rejection, run, run_until, spawn};
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
use crate::weather::WeatherState;

#[test]
fn occupants_count_the_people_inside_each_building() {
    let d = defs();
    let mut state = flat_state(128);
    let house = spawn(&mut state, "house_wood", 10, 10);
    crate::population::test_populate(&mut state, d, 24);
    run(&mut state, 1);
    let inside = |s: &crate::state::GameState, b| {
        s.population().citizens().filter(|(_, c)| c.motion().inside() == Some(b)).count() as u32
    };
    assert!(state.population().occupants(house) > 0);
    assert_eq!(state.population().occupants(house), inside(&state, house));
    // Over a day the count always matches the citizens (the index is rebuilt every tick).
    for _ in 0..TICKS_PER_DAY / 50 {
        run(&mut state, 50);
        for (id, _) in state.buildings().iter() {
            assert_eq!(state.population().occupants(id), inside(&state, id), "{id:?}");
        }
    }
    // A stale id (another generation of the slot) counts nobody.
    let stale = crate::buildings::BuildingId::from_raw_parts(house.index(), house.generation() + 1);
    assert_eq!(state.population().occupants(stale), 0);
    // Loading rebuilds the index.
    let header = crate::save::SaveHeader::for_state(&state, d, "occupancy", 0);
    let loaded = crate::save::decode(&crate::save::encode(&header, &state).unwrap(), d).unwrap().state;
    assert_eq!(loaded.population().occupants(house), state.population().occupants(house));
}

#[test]
fn sites_report_what_their_builders_do() {
    let d = defs();
    let mut state = flat_state(48);
    let kind = d.building_id("sawmill").unwrap();
    let events = exec(&mut state, Command::PlaceBuilding { kind, origin: IVec2::new(30, 30), rotation: Default::default() });
    let site = events
        .iter()
        .find_map(|e| match e {
            SimEvent::BuildingPlaced { building, .. } => Some(*building),
            _ => None,
        })
        .unwrap();
    // No materials delivered yet: nothing to build with.
    assert_eq!(crate::buildings::site_work(&state, d, site), Some(SiteWork::WaitingForMaterials));
    crate::test_support::supply(&mut state, site);
    run_until(&mut state, 2 * TICKS_PER_DAY, |s| crate::buildings::site_work(s, d, site) == Some(SiteWork::Working));
    let now = state.clock().tick;
    let b = state.building(site).unwrap().construction_site().unwrap();
    assert!(b.progressed_at(now) && b.last_work_tick() == Some(now));
    // Lunch, the evening or the finished building end the work.
    run_until(&mut state, TICKS_PER_DAY, |s| crate::buildings::site_work(s, d, site) != Some(SiteWork::Working));
    let after = crate::buildings::site_work(&state, d, site);
    assert!(
        matches!(after, None | Some(SiteWork::LunchBreak | SiteWork::OffShift | SiteWork::BuildersOnTheWay)),
        "{after:?}"
    );
    assert_eq!(SiteWork::OffShift.text(), "Bauarbeiter haben Feierabend");
    // Paused by the player.
    if after.is_some() {
        exec(&mut state, Command::SetBuildingPaused { building: site, paused: true });
        assert_eq!(crate::buildings::site_work(&state, d, site), Some(SiteWork::Paused));
    }
    assert_eq!(crate::buildings::site_work(&state, d, crate::test_support::depot(&state)), None, "not a site");
}

#[test]
fn houses_report_their_heating() {
    let d = defs();
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 10, 10);
    let empty = spawn(&mut state, "house_wood", 20, 10);
    crate::population::test_populate(&mut state, d, 5);
    run(&mut state, TICKS_PER_HOUR);
    let p = state.population();
    if p.residents_of(empty).count() == 0 {
        assert_eq!(p.heating(empty, false), Some(Heating::Empty));
    }
    assert_eq!(p.heating(house, false), Some(Heating::NotNeeded));
    assert_eq!(p.heating(crate::test_support::depot(&state), true), None, "not a house");
    // Winter with firewood in the pantry: the fire burns; without fuel the family freezes.
    state.clock = crate::time::GameClock::at_tick(3 * crate::time::TICKS_PER_SEASON + 6 * TICKS_PER_HOUR);
    run_until(&mut state, 2 * TICKS_PER_DAY, |s| s.population().heating(house, true) == Some(Heating::Burning));
    assert!(Heating::Burning.smoking());
    crate::test_support::empty_storages(&mut state);
    for (_, h) in state.population.households.iter_mut() {
        h.pantry.clear();
    }
    run(&mut state, TICKS_PER_HOUR);
    assert_eq!(state.population().heating(house, true), Some(Heating::OutOfFuel));
    assert_eq!(Heating::OutOfFuel.text(), "Kein Brennstoff – die Bewohner frieren");
}

#[test]
fn since_days_uses_the_dative() {
    assert_eq!(crate::report::since_days_text(0), "seit heute");
    assert_eq!(crate::report::since_days_text(1), "seit 1 Tag");
    assert_eq!(crate::report::since_days_text(3), "seit 3 Tagen");
}

#[test]
fn temperatures_follow_season_weather_and_climate() {
    let d = defs();
    let rules = d.rules();
    let t = &rules.climate.temperature;
    let at = |season: u64, day: u64| crate::time::GameClock::at_tick(season * crate::time::TICKS_PER_SEASON + day * TICKS_PER_DAY);
    let clear = crate::weather::WeatherState::fixed(crate::defs::Weather::Clear, 100);
    let snow = crate::weather::WeatherState::fixed(crate::defs::Weather::Snow, 100);
    for day in 0..6 {
        let spring = clear.temperature_c(rules, at(0, day));
        let summer = clear.temperature_c(rules, at(1, day));
        let winter = snow.temperature_c(rules, at(3, day));
        let spread = t.daily_spread_c as i8;
        assert!((spring - t.spring_c - 2).abs() <= spread, "spring {spring}");
        assert!((summer - t.summer_c - 2).abs() <= spread, "summer {summer}");
        assert!((winter - t.winter_c + 4).abs() <= spread, "winter {winter}");
        assert!(snow.needs_heating(rules, at(3, day)), "every winter day is cold");
        assert!(!clear.needs_heating(rules, at(1, day)), "summer is warm");
    }
    // The same day always has the same temperature (a hash, no random draw).
    assert_eq!(clear.temperature_c(rules, at(0, 2)), clear.temperature_c(rules, at(0, 2)));
    assert!(snow.temperature_text(rules, at(3, 1)).ends_with(" °C"));
    assert!(snow.temperature_text(rules, at(3, 1)).starts_with('−'), "{}", snow.temperature_text(rules, at(3, 1)));
    // Cold mountains and warm lowlands shift the climate.
    assert_eq!(crate::weather::WeatherState::climate_shift_c(rules, 128), 0);
    assert_eq!(crate::weather::WeatherState::climate_shift_c(rules, 0), -(t.climate_range_c as i8));
    let report = crate::report::weather_report(&flat_state(16), d);
    assert!(report.effects[0].starts_with("Temperatur: "), "{:?}", report.effects);
}

#[test]
fn every_game_has_a_deterministic_identity_in_its_saves() {
    let d = defs();
    let settings = crate::state::GameSettings::new(d, 7, crate::world::WorldSize::Small);
    let a = crate::state::game_identity(&settings);
    assert_eq!(a, crate::state::game_identity(&settings), "deterministic");
    assert_eq!(a.get_version_num(), 4);
    let other = crate::state::GameSettings::new(d, 8, crate::world::WorldSize::Small);
    assert_ne!(a, crate::state::game_identity(&other));
    let state = crate::state::GameState::new(d, settings);
    assert_eq!(state.game_id(), a);
    let header = crate::save::SaveHeader::for_state(&state, d, "Test", 0);
    assert_eq!(header.game_id, a);
    let bytes = crate::save::encode(&header, &state).unwrap();
    let loaded = crate::save::decode(&bytes, d).unwrap();
    assert_eq!(loaded.state.game_id(), a);
}

#[test]
fn the_wind_changes_by_hour_and_follows_the_weather() {
    let d = defs();
    let mut state = crate::state::GameState::new(d, crate::state::GameSettings::new(d, 5, crate::world::WorldSize::Small));
    let mut seen = std::collections::BTreeSet::new();
    let mut last = state.weather().wind_direction();
    let mut max_veer = 0;
    for _ in 0..3 * 24 {
        crate::test_support::run(&mut state, TICKS_PER_HOUR);
        let w = state.weather();
        assert!(w.wind_direction() < 16 && w.wind_strength() <= 100);
        assert_eq!(w.wind_degrees(), u16::from(w.wind_direction()) * 45 / 2);
        let v = w.wind_vector_permille();
        assert!((v.x * v.x + v.y * v.y - 1_000_000).abs() < 5_000, "unit vector {v}");
        let step = (i32::from(w.wind_direction()) - i32::from(last)).rem_euclid(16);
        max_veer = max_veer.max(step.min(16 - step));
        last = w.wind_direction();
        seen.insert(w.wind_direction());
    }
    assert!(seen.len() > 1, "the wind turns");
    // Within a day it veers at most one point per hour (a new day may bring a new direction).
    assert!(max_veer <= 8);
    // Storms blow hard.
    let mut stormy = state.clone();
    stormy.weather = WeatherState::fixed(Weather::Storm, 100);
    crate::test_support::run(&mut stormy, TICKS_PER_HOUR);
    assert!(stormy.weather().wind_strength() >= 90, "{}", stormy.weather().wind_strength());
}

#[test]
fn fire_spreads_further_downwind() {
    use crate::events::effects::fire_reach;
    let east = IVec2::new(1000, 0);
    // A house 5 tiles east (downwind) burns in a strong wind, one 5 tiles west does not.
    assert_eq!(fire_reach(4, 5, IVec2::new(10, 0), east, 80), (6, 3));
    assert_eq!(fire_reach(4, 5, IVec2::new(-10, 0), east, 80), (3, 6));
    // Calm: the plain radius in every direction.
    assert_eq!(fire_reach(4, 3, IVec2::new(0, 6), east, 10), (4, 3));
    assert_eq!(fire_reach(4, 3, IVec2::new(6, 0), east, 10), (4, 3));
}

#[test]
fn research_is_queued_with_its_prerequisites_in_one_step() {
    let d = defs();
    let mut state = flat_state(48);
    // A technology with a prerequisite chain.
    let tech = d
        .techs()
        .iter()
        .find(|t| t.prerequisites.iter().any(|p| !d.tech(*p).prerequisites.is_empty()))
        .map(|t| t.id)
        .expect("a tech two levels deep");
    let plan = crate::research::queue_plan(&state, d, tech);
    assert!(plan.len() >= 3, "{plan:?}");
    assert_eq!(plan.last(), Some(&tech));
    for (i, t) in plan.iter().enumerate() {
        for p in &d.tech(*t).prerequisites {
            assert!(plan[..i].contains(p) || state.research().is_researched(*p), "{} before {}", d.tech(*p).name, d.tech(*t).name);
        }
    }
    let events = exec(&mut state, Command::QueueResearchWithPrerequisites { tech });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    assert_eq!(state.research().queue(), plan.as_slice());
    // Again: already queued.
    let events = exec(&mut state, Command::QueueResearchWithPrerequisites { tech });
    assert!(matches!(rejection(&events), Some(crate::command::RejectReason::AlreadyQueued { .. })));
    assert!(crate::research::queue_plan(&state, d, tech).is_empty());
    // A plan that does not fit into the queue is rejected as a whole.
    let mut full = flat_state(48);
    let max = usize::from(d.rules().research.max_queue);
    let roots: Vec<_> = d
        .techs()
        .iter()
        .map(|t| t.id)
        .filter(|t| d.tech(*t).prerequisites.is_empty() && !full.research().is_researched(*t) && !plan.contains(t))
        .take(max - 1)
        .collect();
    for t in &roots {
        exec(&mut full, Command::QueueResearch { tech: *t });
    }
    let before = full.research().queue().to_vec();
    let events = exec(&mut full, Command::QueueResearchWithPrerequisites { tech });
    assert!(matches!(rejection(&events), Some(crate::command::RejectReason::ResearchQueueFull { .. })), "{:?}", rejection(&events));
    assert_eq!(full.research().queue(), before.as_slice(), "nothing queued");
}

#[test]
fn planners_say_the_game_is_lost() {
    let d = defs();
    let mut state = flat_state(48);
    let kind = d.building_id("house_wood").unwrap();
    assert!(crate::buildings::check_placement(&state, d, kind, IVec2::new(10, 10), Default::default()).ok);
    state.settings.defeat_enabled = true;
    crate::ranks::defeat(&mut state, d, crate::ranks::DefeatReason::Depopulated, &mut Vec::new());
    let check = crate::buildings::check_placement(&state, d, kind, IVec2::new(10, 10), Default::default());
    assert!(!check.ok);
    assert_eq!(check.errors.first().map(ToString::to_string).as_deref(), Some("Das Spiel ist verloren."));
    let path = d.road_id("path").unwrap();
    let plan = crate::roads::plan_road(&state, d, IVec2::new(5, 5), IVec2::new(15, 5), path);
    assert!(!plan.valid);
    assert_eq!(plan.problems.first().map(ToString::to_string).as_deref(), Some("Das Spiel ist verloren."));
}

#[test]
fn losses_and_epidemics_report_where_they_happened() {
    let d = defs();
    let mut state = flat_state(48);
    let house = spawn(&mut state, "house_wood", 10, 10);
    crate::population::test_populate(&mut state, d, 5);
    run(&mut state, TICKS_PER_HOUR);
    let disease = d.event_id("plague").expect("plague event");
    let mut events = exec(&mut state, Command::DebugTriggerEvent { event: disease });
    let serial = state.events().pending().map(|p| p.serial);
    if let Some(serial) = serial {
        let choices = d.event(disease).choices.len() as u8;
        for choice in 0..choices {
            let mut s = state.clone();
            events = exec(&mut s, Command::ResolveEvent { serial, choice });
            if let Some(SimEvent::CitizensFellIll { citizens, homes, position }) =
                events.iter().find(|e| matches!(e, SimEvent::CitizensFellIll { .. }))
            {
                if *citizens > 0 {
                    assert_eq!(homes.as_slice(), &[house]);
                    assert_eq!(*position, Some(state.building(house).unwrap().entrance()));
                    return;
                }
            }
        }
    }
    panic!("the plague made somebody ill: {events:?}");
}

#[test]
fn deadlines_counts_and_shortfalls_read_as_german() {
    use crate::time::TICKS_PER_HOUR;
    let hour = |h: u64| h * TICKS_PER_HOUR;
    // The clock starts at 06:00 of day 1.
    assert_eq!(crate::report::deadline_clock_text(0, hour(12)), "18:00 Uhr");
    assert_eq!(crate::report::deadline_clock_text(hour(12), hour(26)), "morgen 08:00 Uhr");
    assert_eq!(crate::report::deadline_clock_text(0, hour(50)), "übermorgen 08:00 Uhr");
    assert_eq!(crate::report::deadline_clock_text(0, hour(100)), "Frühling · Tag 5 · Jahr 1, 10:00 Uhr");
    // Beyond the first season the calendar date, never an absolute day count.
    assert_eq!(crate::report::deadline_clock_text(0, hour(24 * 30)), "Sommer · Tag 1 · Jahr 2, 06:00 Uhr");
    assert_eq!(crate::report::missing_gold(1), "Es fehlt 1 Gold");
    assert_eq!(crate::report::missing_gold(20), "Es fehlen 20 Gold");
    assert_eq!(crate::report::missing_goods(1, "Bretter"), "Es fehlt 1 Einheit Bretter");
    assert_eq!(crate::report::missing_goods(5, "Bretter"), "Es fehlen 5 Bretter");
    assert_eq!(crate::report::tiles_text(1), "1 Feld");
    assert_eq!(crate::report::tiles_dative_text(8), "8 Feldern");
    use crate::military::RecruitProblem;
    let slots = |free| RecruitProblem::NoSlots { building: "Kaserne".to_owned(), free }.to_string();
    assert_eq!(slots(0), "Das Gebäude „Kaserne“ hat keinen freien Ausbildungsplatz mehr.");
    assert_eq!(slots(1), "Das Gebäude „Kaserne“ hat nur noch 1 freien Ausbildungsplatz.");
    assert_eq!(slots(3), "Das Gebäude „Kaserne“ hat nur noch 3 freie Ausbildungsplätze.");
    assert_eq!(
        RecruitProblem::NoRecruits { needed: 2, available: 1 }.to_string(),
        "Es steht nur 1 von 2 Erwachsenen für den Dienst bereit."
    );
    assert_eq!(RecruitProblem::NoGold { missing: 1 }.to_string(), "Es fehlt 1 Gold.");
    assert_eq!(
        RecruitProblem::NoMaterials { resource: "Waffen".to_owned(), missing: 1 }.to_string(),
        "Es fehlt 1 Einheit Waffen in den Lagern."
    );
    let water = crate::buildings::PlacementError::WaterMissing { max_distance: 1 }.to_string();
    assert_eq!(water, "Das Gebäude muss am Wasser stehen (höchstens 1 Feld entfernt).");
}

#[test]
fn soldier_and_family_counts_agree_in_number() {
    use crate::defs::EventCondition;
    let d = defs();
    assert_eq!(crate::military::fallen_text(1, 3), "Gefallen: 1 eigener Soldat, 3 feindliche Soldaten.");
    assert_eq!(crate::military::fallen_text(0, 1), "Gefallen: kein eigener Soldat, 1 feindlicher Soldat.");
    assert_eq!(crate::military::fallen_text(2, 0), "Gefallen: 2 eigene Soldaten, kein feindlicher Soldat.");
    let mut state = flat_state(32);
    let problem = |s: &crate::state::GameState, n| crate::events::conditions::requirement_problem(s, d, &EventCondition::MinSoldiers(n));
    assert_eq!(problem(&state, 5).as_deref(), Some("Es steht kein Soldat bereit (nötig: 5 Soldaten)."));
    let barracks = spawn(&mut state, "barracks", 10, 10);
    let militia = d.unit_id("militia").unwrap();
    crate::military::test_enlist(&mut state, d, 1, militia, barracks);
    assert_eq!(problem(&state, 5).as_deref(), Some("Es steht nur 1 von 5 Soldaten bereit."));
    crate::military::test_enlist(&mut state, d, 2, militia, barracks);
    assert_eq!(problem(&state, 5).as_deref(), Some("Es stehen nur 3 von 5 Soldaten bereit."));
    let text = |c| crate::events::conditions::condition_text(d, &c);
    assert_eq!(text(EventCondition::MinSoldiers(1)), "mindestens 1 Soldat");
    assert_eq!(text(EventCondition::MinFreeHousing(1)), "mindestens 1 freier Wohnplatz");
    assert_eq!(text(EventCondition::MinBuildingsNearRiver { count: 2, distance: 1 }), "mindestens 2 Gebäude höchstens 1 Feld vom Fluss");
    // A single settler leaving: „(1 Person)“.
    let single = state.population.households().find(|(_, h)| h.size() == 1).map(|(id, _)| id).unwrap();
    let mut events = Vec::new();
    crate::population::demography::emigrate(&mut state, d, single, &mut events);
    let note = events.iter().find_map(|e| if let SimEvent::Notification(n) = e { Some(n.text.clone()) } else { None }).unwrap();
    assert!(note.ends_with("(1 Person) hat das Reich unzufrieden verlassen."), "{note}");
}
