use super::*;
use crate::command::Command;
use crate::test_support::{defs, exec, flat_state, rejection, run, run_until_at_work, spawn};
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};

fn tech(key: &str) -> TechId {
    defs().tech_id(key).expect("technology")
}

/// A flat test state with a town hall (the headquarters that researches 1 point per hour).
fn state_with_hall() -> GameState {
    let mut state = flat_state(40);
    spawn(&mut state, "town_hall", 10, 10);
    state
}

#[test]
fn unlocks_follow_researched_techs() {
    let d = defs();
    let mut r = Research::new();
    let house = d.building_id("house_wood").unwrap();
    let farm = d.building_id("wheat_farm").unwrap();
    let road = d.road_id("road").unwrap();
    assert!(r.building_unlocked(d, house));
    assert!(!r.building_unlocked(d, farm));
    assert!(!r.road_unlocked(d, road));
    assert!(r.road_unlocked(d, d.road_id("path").unwrap()));
    assert_eq!(r.build_speed_bonus_percent(d), 0);
    assert!(r.mark_researched(tech("grain_farming")));
    assert!(!r.mark_researched(tech("grain_farming")));
    assert!(r.building_unlocked(d, farm));
    r.mark_researched(tech("architecture_1"));
    assert!(r.road_unlocked(d, road));
    assert_eq!(r.build_speed_bonus_percent(d), 10);
    assert_eq!(r.researched().count(), 2);
}

#[test]
fn queue_checks_prerequisites_duplicates_and_size() {
    let mut state = state_with_hall();
    let e = exec(&mut state, Command::QueueResearch { tech: tech("animal_husbandry") });
    assert!(matches!(
        rejection(&e),
        Some(RejectReason::MissingPrerequisites { missing, .. }) if missing == "„Getreideanbau“"
    ));
    // A prerequisite queued before it is enough.
    assert!(rejection(&exec(&mut state, Command::QueueResearch { tech: tech("grain_farming") })).is_none());
    let e = exec(&mut state, Command::QueueResearch { tech: tech("animal_husbandry") });
    assert!(rejection(&e).is_none());
    assert!(e.iter().any(|e| matches!(e, SimEvent::ResearchQueueChanged { queue } if queue.len() == 2)));
    assert!(matches!(
        rejection(&exec(&mut state, Command::QueueResearch { tech: tech("grain_farming") })),
        Some(RejectReason::AlreadyQueued { .. })
    ));
    let bad = TechId::from_index(9999).unwrap();
    assert_eq!(rejection(&exec(&mut state, Command::QueueResearch { tech: bad })), Some(&RejectReason::UnknownTech));
    let free: Vec<TechId> = defs().techs().iter().filter(|t| t.prerequisites.is_empty()).map(|t| t.id).collect();
    for t in &free {
        state.tick(defs(), vec![Command::QueueResearch { tech: *t }]);
    }
    assert_eq!(state.research.queue().len(), usize::from(defs().rules().research.max_queue));
    let e = exec(&mut state, Command::QueueResearch { tech: tech("improved_plows") });
    assert!(matches!(rejection(&e), Some(RejectReason::ResearchQueueFull { max: 8 })));
    state.research.mark_researched(tech("grain_farming"));
    let e = exec(&mut state, Command::QueueResearch { tech: tech("grain_farming") });
    assert!(matches!(rejection(&e), Some(RejectReason::AlreadyResearched { .. })));
    state.validate_entities(defs()).unwrap();
}

#[test]
fn research_pays_progresses_hourly_and_completes_with_its_effects() {
    let d = defs();
    let mut state = state_with_hall();
    let gold = state.economy.treasury();
    let grain = tech("grain_farming");
    assert_eq!(research_rate(&state, d).current_milli, 1000, "the town hall researches one point per hour");
    let events = exec(&mut state, Command::QueueResearch { tech: grain });
    assert!(events.iter().any(|e| matches!(e, SimEvent::ResearchStarted { gold: 60, .. })));
    assert_eq!(state.economy.treasury(), gold - 60);
    assert_eq!(state.economy.ledger().today().gold(LedgerCategory::Research), -60);
    assert_eq!(state.research.status(d, state.economy.treasury()), ResearchStatus::Researching(grain));
    run(&mut state, TICKS_PER_HOUR);
    assert_eq!(state.research.progress_milli(grain), 1000 + 5, "one point per hour (plus the start tick)");
    // 24 points at 1 point per hour: done after exactly 24 hours of ticks.
    let mut done = None;
    for t in 1..(25 * TICKS_PER_HOUR) {
        let events = run(&mut state, 1);
        if events.iter().any(|e| matches!(e, SimEvent::ResearchCompleted { tech } if *tech == grain)) {
            let note = events.iter().filter_map(SimEvent::notification).find(|n| n.text.starts_with("Forschung abgeschlossen"));
            let text = note.map(|n| n.text.clone()).unwrap_or_default();
            assert!(text.contains("Getreideanbau") && text.contains("Weizenfarm"), "{text}");
            done = Some(t);
            break;
        }
    }
    assert_eq!(done, Some(23 * TICKS_PER_HOUR - 1), "24 hours of points in total");
    assert!(state.research.is_researched(grain));
    assert!(state.research.queue().is_empty() && !state.research.is_paid(grain));
    assert!(state.research.building_unlocked(d, d.building_id("wheat_farm").unwrap()));
    assert_eq!(state.research.completed().len(), 1);
    state.validate_entities(d).unwrap();
}

#[test]
fn research_waits_for_gold_with_one_warning() {
    let d = defs();
    let mut state = state_with_hall();
    let treasury = state.economy.treasury();
    state.economy.add_gold(-treasury + 20);
    let grain = tech("grain_farming");
    let mut events = exec(&mut state, Command::QueueResearch { tech: grain });
    events.extend(run(&mut state, 3 * TICKS_PER_HOUR));
    let warnings = events.iter().filter_map(SimEvent::notification).filter(|n| n.text.contains("fehlen noch 40 Gold")).count();
    assert_eq!(warnings, 1);
    assert_eq!(state.research.progress_milli(grain), 0, "no progress before the cost is paid");
    assert!(matches!(
        state.research.status(d, state.economy.treasury()),
        ResearchStatus::WaitingForGold { tech, missing } if tech == grain && missing >= 40
    ));
    state.economy.add_gold(100);
    let events = run(&mut state, 1);
    assert!(events.iter().any(|e| matches!(e, SimEvent::ResearchStarted { .. })));
    assert!(state.research.progress_milli(grain) > 0);
}

#[test]
fn cancel_keeps_progress_and_payment_and_removes_dependants() {
    let mut state = state_with_hall();
    let grain = tech("grain_farming");
    let plows = tech("improved_plows");
    let rotation = tech("crop_rotation");
    for t in [grain, plows, rotation] {
        exec(&mut state, Command::QueueResearch { tech: t });
    }
    run(&mut state, 5 * TICKS_PER_HOUR);
    let progress = state.research.progress_milli(grain);
    let treasury = state.economy.treasury();
    let events = exec(&mut state, Command::CancelResearch { tech: grain });
    let cancelled: Vec<TechId> =
        events.iter().filter_map(|e| if let SimEvent::ResearchCancelled { tech } = e { Some(*tech) } else { None }).collect();
    assert_eq!(cancelled, vec![grain, plows, rotation], "dependants leave the queue too");
    assert!(state.research.queue().is_empty());
    assert!(state.research.is_paid(grain));
    // Queued again: continues without paying again.
    let events = exec(&mut state, Command::QueueResearch { tech: grain });
    assert!(!events.iter().any(|e| matches!(e, SimEvent::ResearchStarted { .. })));
    assert_eq!(state.economy.treasury(), treasury);
    assert!(state.research.progress_milli(grain) > progress);
    assert_eq!(rejection(&exec(&mut state, Command::CancelResearch { tech: plows })), Some(&RejectReason::NotQueued));
    state.validate_entities(defs()).unwrap();
}

#[test]
fn reordering_keeps_prerequisites_first() {
    let mut state = state_with_hall();
    let (grain, plows, mining) = (tech("grain_farming"), tech("improved_plows"), tech("mining"));
    for t in [grain, plows, mining] {
        exec(&mut state, Command::QueueResearch { tech: t });
    }
    let e = exec(&mut state, Command::MoveResearch { tech: plows, position: 0 });
    assert!(matches!(
        rejection(&e),
        Some(RejectReason::InvalidResearchOrder { name, prerequisite }) if name == "Verbesserte Pflüge" && prerequisite == "Getreideanbau"
    ));
    let e = exec(&mut state, Command::MoveResearch { tech: mining, position: 0 });
    assert!(rejection(&e).is_none());
    assert_eq!(state.research.queue(), &[mining, grain, plows]);
    // The previous front keeps its payment; the new one pays when it starts.
    run(&mut state, 1);
    assert!(state.research.is_paid(grain) && state.research.is_paid(mining));
    let e = exec(&mut state, Command::MoveResearch { tech: grain, position: 200 });
    assert!(matches!(rejection(&e), Some(RejectReason::InvalidResearchOrder { .. })), "grain must stay before plows");
    state.validate_entities(defs()).unwrap();
}

#[test]
fn schools_add_research_points_with_their_staff_at_work() {
    let d = defs();
    let mut state = state_with_hall();
    let school = spawn(&mut state, "school", 18, 10);
    run_until_at_work(&mut state);
    let rate = research_rate(&state, d);
    assert_eq!(rate.sources.len(), 1);
    assert_eq!(rate.sources[0].building, school);
    // Two healthy scholars at work: 2 points per hour × shift factor 3.
    assert_eq!(rate.sources[0].current_milli, 6000);
    assert_eq!(rate.current_milli, 7000);
    assert_eq!(rate.daily_milli, 24 * 1000 + 24 * 2000, "expected day: hall round the clock, school's full shift");
    // Researched scholarship speeds research up by 25 %.
    state.research.mark_researched(tech("scholarship"));
    assert_eq!(research_rate(&state, d).current_milli, 7000 * 125 / 100);
    // Without the headquarters only the school researches (and only while staffed).
    let hall = crate::economy::storage::main_headquarters(&state, d).unwrap();
    state.buildings.remove(hall);
    assert_eq!(research_rate(&state, d).base_milli, 0);
}

#[test]
fn daily_history_records_the_points() {
    let mut state = state_with_hall();
    let ticks_to_midnight = TICKS_PER_DAY - state.clock.tick_of_day();
    run(&mut state, ticks_to_midnight + TICKS_PER_DAY + 1);
    let days: Vec<ResearchDay> = state.research.history().copied().collect();
    assert_eq!(days.len(), 2);
    assert_eq!(days[1].points_milli, 24_000, "a full day at one point per hour");
    assert!(days[0].points_milli > 0 && days[0].points_milli < 24_000);
}

#[test]
fn completion_applies_storage_and_radius_bonuses() {
    let d = defs();
    let mut state = state_with_hall();
    let depot = crate::test_support::depot(&state);
    let wood = d.resource_id("wood").unwrap();
    assert_eq!(state.building(depot).unwrap().inventory().capacity(wood), 400);
    for key in ["bookkeeping", "extended_storage"] {
        let t = tech(key);
        state.research.queue.push(t);
        state.research.paid.insert(t);
        state.research.progress.insert(t, u64::from(d.tech(t).research_points) * PROGRESS_PER_POINT - 1);
        run(&mut state, 1);
        assert!(state.research.is_researched(t), "{key}");
    }
    assert_eq!(state.building(depot).unwrap().inventory().capacity(wood), 600, "+50 % storage");
    let market = d.building_by_key("market").unwrap();
    assert_eq!(state.research.service_radius(d, market, ServiceKind::Market), Some(18));
    state.research.mark_researched(tech("trading"));
    state.research.mark_researched(tech("efficient_markets"));
    assert_eq!(state.research.service_radius(d, market, ServiceKind::Market), Some(24));
    state.research.mark_researched(tech("smelting"));
    state.research.mark_researched(tech("steel_tools"));
    assert_eq!(state.research.tool_wear_reduction_percent(d), 50);
}

#[test]
fn debug_unlock_clears_the_queue() {
    let mut state = state_with_hall();
    exec(&mut state, Command::QueueResearch { tech: tech("grain_farming") });
    let events = exec(&mut state, Command::DebugUnlockAllResearch);
    assert!(events.iter().any(|e| matches!(e, SimEvent::DebugResearchUnlocked { .. })));
    assert!(state.research.queue().is_empty());
    assert_eq!(state.research.researched().count(), defs().techs().len());
    state.validate_entities(defs()).unwrap();
}

#[test]
fn every_effect_has_a_german_description() {
    let d = defs();
    for t in d.techs() {
        for e in &t.effects {
            let text = effect_text(d, e);
            assert!(!text.is_empty() && !text.contains('?'), "{}: {text}", t.key);
        }
    }
    let plows = d.tech_by_key("improved_plows").unwrap();
    assert_eq!(effect_text(d, &plows.effects[0]), "+15 % Ertrag auf allen Feldern");
    let walls = d.tech_by_key("fortification").unwrap();
    assert_eq!(effect_text(d, &walls.effects[1]), "Mauern und Tore +25 % stärker", "military effects work since step 5b");
    let crossbows = d.tech_by_key("crossbows").unwrap();
    assert_eq!(effect_text(d, &crossbows.effects[1]), "Bogenschütze: +10 % Angriff");
}

#[test]
fn validate_rejects_inconsistent_research_and_rederive_repairs_the_order() {
    let d = defs();
    let mut state = state_with_hall();
    state.research.queue = vec![tech("grain_farming"), tech("grain_farming")];
    assert!(matches!(state.validate_entities(d), Err(crate::state::StateError::InvalidResearch(_))));
    state.research.queue = vec![TechId::from_index(999).unwrap()];
    assert!(state.validate_entities(d).is_err());
    state.research.queue.clear();
    state.research.mark_researched(tech("mining"));
    state.research.paid.insert(tech("mining"));
    assert!(state.validate_entities(d).is_err(), "researched and paid at once");
    // A queue out of order (e.g. after the data changed a prerequisite) loses the entries that
    // cannot be researched yet.
    let mut state = state_with_hall();
    state.research.queue = vec![tech("improved_plows"), tech("grain_farming"), tech("animal_husbandry")];
    state.validate_entities(d).unwrap();
    state.rederive(d);
    assert_eq!(state.research.queue(), &[tech("grain_farming"), tech("animal_husbandry")]);
}

#[test]
fn the_status_says_when_everything_is_researched() {
    let d = defs();
    let mut state = state_with_hall();
    let idle = crate::report::research_report(&state, d).status_text;
    assert!(idle.contains("verfallen"), "{idle}");
    crate::test_support::exec(&mut state, crate::command::Command::DebugUnlockAllResearch);
    let done = crate::report::research_report(&state, d).status_text;
    assert_eq!(done, "Alle Technologien sind erforscht.");
}
