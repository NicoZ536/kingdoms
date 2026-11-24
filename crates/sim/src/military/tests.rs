//! Tests of the military (docs/game_design.md §11) and the ranks (§1).

use glam::IVec2;

use super::*;
use crate::command::{Command, RejectReason};
use crate::event::SimEvent;
use crate::population::DeathCause;
use crate::test_support::{defs, exec, flat_state, rejection, run, run_until, spawn};
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};

fn unit(key: &str) -> UnitTypeId {
    defs().unit_id(key).unwrap()
}

fn research(state: &mut GameState, key: &str) {
    let t = defs().tech_id(key).unwrap();
    state.research.mark_researched(t);
}

/// A 48×48 flat kingdom with a town hall in the middle, the depot, and barracks with a
/// watchtower near the town hall (militia researched); returns (state, town hall, barracks).
fn garrison_town() -> (GameState, BuildingId, BuildingId) {
    let mut s = flat_state(48);
    research(&mut s, "militia_training");
    let hall = spawn(&mut s, "town_hall", 20, 20);
    let barracks = spawn(&mut s, "barracks", 26, 20);
    crate::test_support::link(&mut s, hall, barracks);
    let depot = crate::test_support::depot(&s);
    crate::test_support::link(&mut s, hall, depot);
    s.workforce.mark_dirty();
    run(&mut s, 2);
    (s, hall, barracks)
}

/// Recruits `count` militia at `barracks` and runs until they are trained.
fn trained_militia(state: &mut GameState, barracks: BuildingId, count: u8) -> Vec<CitizenId> {
    let events = exec(state, Command::Recruit { building: barracks, unit: unit("militia"), count });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    run_until(state, 3 * TICKS_PER_DAY, |s| s.military.soldiers.values().all(Soldier::is_ready));
    state.military.soldiers.keys().copied().collect()
}

/// Puts the clock at `hour` of the current day (or the next one).
fn run_to_hour(state: &mut GameState, hour: u64) {
    run_until(state, TICKS_PER_DAY + 1, |s| s.clock.tick_of_day() == hour * TICKS_PER_HOUR);
}

#[test]
fn recruitment_turns_citizens_into_soldiers_who_train_and_are_paid() {
    let d = defs();
    let (mut s, _, barracks) = garrison_town();
    run_to_hour(&mut s, 8);
    let available = s.workforce.available();
    let gold = s.economy.treasury();
    let wood = s.economy.stock().get(d.resource_id("wood").unwrap());
    let events = exec(&mut s, Command::Recruit { building: barracks, unit: unit("militia"), count: 3 });
    assert!(events.contains(&SimEvent::SoldiersRecruited { building: barracks, unit: unit("militia"), count: 3 }));
    assert!(
        events.iter().any(|e| matches!(e, SimEvent::Notification(n) if n.text.starts_with("3 Milizen in Ausbildung"))),
        "German plural in the notification: {events:?}"
    );
    assert_eq!(s.military.soldiers.len(), 3);
    assert_eq!(s.workforce.available(), available - 3, "recruits leave the workforce");
    assert_eq!(s.economy.treasury(), gold - 30);
    assert_eq!(s.economy.stock().get(d.resource_id("wood").unwrap()), wood - 9, "three wooden spears each");
    for (id, soldier) in s.military.soldiers() {
        let c = s.population.citizen(id).unwrap();
        assert!(c.is_soldier() && c.job().is_none() && !c.can_take_job());
        assert!(matches!(soldier.stage, SoldierStage::Training { building, .. } if building == barracks));
    }
    // Four hours inside the barracks (no trainers: half speed → eight hours).
    let ticks = run_until(&mut s, TICKS_PER_DAY, |s| s.military.soldiers.values().all(Soldier::is_ready));
    assert!(ticks >= 4 * TICKS_PER_HOUR, "training takes its time ({ticks} ticks)");
    assert_eq!(crate::economy::finance::daily_rates(&s, d).military, -3000, "1 gold per militia and day");
    s.validate_entities(d).unwrap();
    // Soldiers count for the diplomatic strength.
    assert!(strength(&s, d) > 0);
}

#[test]
fn recruitment_problems_are_explained() {
    let d = defs();
    let (mut s, hall, barracks) = garrison_town();
    let swordsman = unit("swordsman");
    let p = recruit_problem(&s, d, barracks, swordsman, 1).unwrap();
    assert!(p.to_string().contains("erfordert die Forschung"), "{p}");
    let p = recruit_problem(&s, d, hall, unit("militia"), 1).unwrap();
    assert!(matches!(p, RecruitProblem::WrongBuilding { .. }), "{p}");
    let p = recruit_problem(&s, d, barracks, unit("militia"), 7).unwrap();
    assert!(matches!(p, RecruitProblem::InvalidCount { max: 6 }), "{p}");
    s.economy.book_gold(crate::economy::LedgerCategory::Events, -s.economy.treasury());
    let p = recruit_problem(&s, d, barracks, unit("militia"), 2).unwrap();
    assert_eq!(p.to_string(), "Es fehlen 20 Gold.");
    s.economy.add_gold(1000);
    crate::test_support::set_adults(&mut s, 1);
    let p = recruit_problem(&s, d, barracks, unit("militia"), 2).unwrap();
    assert!(p.to_string().contains("nur 1 von 2 Erwachsenen"), "{p}");
    let events = exec(&mut s, Command::Recruit { building: barracks, unit: unit("militia"), count: 2 });
    assert!(matches!(rejection(&events), Some(RejectReason::Recruit(RecruitProblem::NoRecruits { .. }))));
    // Cavalry needs horses: four riders per cattle farm.
    research(&mut s, "cavalry");
    let stables = spawn(&mut s, "stables", 10, 30);
    let p = recruit_problem(&s, d, stables, unit("cavalry"), 1).unwrap();
    assert!(p.to_string().contains("Pferde"), "{p}");
}

/// Runs until no enemy group is left (at most `days`), collecting the events.
fn run_until_quiet(state: &mut GameState, days: u64) -> Vec<SimEvent> {
    let mut events = Vec::new();
    for _ in 0..days * TICKS_PER_DAY {
        events.extend(run(state, 1));
        if state.military.warbands.is_empty() {
            break;
        }
    }
    assert!(state.military.warbands.is_empty(), "the enemies are gone within {days} days");
    events
}

fn battle(events: &[SimEvent]) -> Option<(BattleKind, BattleOutcome)> {
    events.iter().find_map(|e| if let SimEvent::BattleEnded { kind, outcome, .. } = e { Some((*kind, *outcome)) } else { None })
}

#[test]
fn bandits_plunder_an_unguarded_storage_and_flee() {
    let d = defs();
    let mut s = flat_state(48);
    let hall = spawn(&mut s, "town_hall", 20, 20);
    let depot = crate::test_support::depot(&s);
    crate::test_support::link(&mut s, hall, depot);
    run(&mut s, 2);
    let gold = s.economy.treasury();
    let goods: u64 = s.building(depot).unwrap().inventory().total();
    let events = exec(&mut s, Command::DebugSpawnThreat { kingdom: None, size: 5 });
    assert!(events.iter().any(|e| matches!(e, SimEvent::ThreatApproaching { size: 5, kingdom: None, .. })));
    let w = &s.military.warbands()[0];
    assert_eq!(warband_eta(&s, w), Some(TICKS_PER_HOUR - 1));
    let events = run_until_quiet(&mut s, 2);
    assert!(events.iter().any(|e| matches!(e, SimEvent::ThreatArrived { size: 5, .. })));
    let lost: u32 = events.iter().filter_map(|e| if let SimEvent::StockLost { lost, .. } = e { Some(lost.iter().map(|m| m.amount).sum::<u32>()) } else { None }).sum();
    assert!(lost > 0 && lost <= 5 * 15, "every bandit carries off at most 15 goods: {lost}");
    assert!(s.economy.treasury() <= gold - 50 + 1, "and 10 gold each: {} → {}", gold, s.economy.treasury());
    assert!(s.building(depot).map_or(0, |b| b.inventory().total()) < goods || s.building(depot).is_none_or(|b| b.construction_site().is_some()));
    assert_eq!(battle(&events), Some((BattleKind::Raid, BattleOutcome::Defeat)));
    let report = s.military.battles().next().unwrap();
    assert_eq!(report.tally.enemy_losses, 0);
    assert!(report.text.contains("Geraubt"), "{}", report.text);
    assert!(s.population.walking.travelers().all(|(_, t)| t.warband().is_none()), "the bandits left the map");
    s.validate_entities(d).unwrap();
}

#[test]
fn soldiers_and_towers_beat_bandits_and_the_fallen_are_real_deaths() {
    let d = defs();
    let (mut s, hall, barracks) = garrison_town();
    let depot = crate::test_support::depot(&s);
    let at = s.building(depot).unwrap().origin();
    spawn(&mut s, "watchtower", at.x - 3, at.y - 3);
    let militia = crate::military::test_enlist(&mut s, d, 8, unit("militia"), barracks);
    run_to_hour(&mut s, 10);
    run(&mut s, 2 * TICKS_PER_HOUR);
    let events = exec(&mut s, Command::DebugSpawnThreat { kingdom: None, size: 6 });
    assert!(rejection(&events).is_none());
    let events = run_until_quiet(&mut s, 2);
    let (kind, outcome) = battle(&events).expect("a battle report");
    assert_eq!(kind, BattleKind::Raid);
    let report = s.military.battles().next().unwrap().clone();
    eprintln!("{}", report.text);
    assert_eq!(outcome, BattleOutcome::Victory, "{}", report.text);
    assert!(report.tally.enemy_losses >= 1, "{}", report.text);
    assert!(report.tally.own_units >= 1, "{}", report.text);
    let fallen = events.iter().filter(|e| matches!(e, SimEvent::CitizenDied { cause: DeathCause::Battle, .. })).count();
    assert_eq!(fallen as u32, report.tally.own_losses);
    assert_eq!(s.population.stats.today.deaths_by(DeathCause::Battle) as usize, fallen);
    let alive = militia.iter().filter(|m| s.military.soldier(**m).is_some()).count();
    assert_eq!(alive + fallen, 8);
    // The survivors gained experience.
    assert!(s.military.soldiers().any(|(_, x)| x.experience() > 0));
    let _ = hall;
    s.validate_entities(d).unwrap();
}

#[test]
fn bands_grow_with_the_town_shrink_before_defenders_and_follow_the_difficulty() {
    let d = defs();
    let (mut s, _, barracks) = garrison_town();
    crate::test_support::set_adults(&mut s, 48);
    // 3 + 48 / 12 = 7 bandits for an undefended town of 48.
    assert_eq!(raid_defenders(&s, d), 0);
    assert_eq!(bandit_band(&s, d), 7);
    s.settings.difficulty = crate::state::Difficulty::Easy;
    assert_eq!(bandit_band(&s, d), 5, "75 % of 7, rounded");
    s.settings.difficulty = crate::state::Difficulty::Hard;
    assert_eq!(bandit_band(&s, d), 9, "125 % of 7, rounded");
    s.settings.difficulty = crate::state::Difficulty::Normal;
    // A watchtower and four militia: one bandit fewer per two defenders.
    let depot = crate::test_support::depot(&s);
    let at = s.building(depot).unwrap().origin();
    spawn(&mut s, "watchtower", at.x - 3, at.y - 3);
    crate::military::test_enlist(&mut s, d, 4, unit("militia"), barracks);
    assert_eq!(raid_defenders(&s, d), 5);
    assert_eq!(bandit_band(&s, d), 5);
    // Never fewer than half the band, however strong the town.
    crate::military::test_enlist(&mut s, d, 20, unit("militia"), barracks);
    assert_eq!(bandit_band(&s, d), 7 - 7 / 2);
}

#[test]
fn a_small_defended_town_repels_the_band_it_draws() {
    let d = defs();
    let (mut s, _, barracks) = garrison_town();
    crate::test_support::set_adults(&mut s, 48);
    let depot = crate::test_support::depot(&s);
    let at = s.building(depot).unwrap().origin();
    spawn(&mut s, "watchtower", at.x - 3, at.y - 3);
    let militia = crate::military::test_enlist(&mut s, d, 4, unit("militia"), barracks);
    run_to_hour(&mut s, 10);
    run(&mut s, 2 * TICKS_PER_HOUR);
    let size = bandit_band(&s, d);
    let events = exec(&mut s, Command::DebugSpawnThreat { kingdom: None, size });
    assert!(rejection(&events).is_none());
    let events = run_until_quiet(&mut s, 2);
    let report = s.military.battles().next().unwrap().clone();
    eprintln!("{}", report.text);
    assert_eq!(battle(&events), Some((BattleKind::Raid, BattleOutcome::Victory)), "{}", report.text);
    assert!(report.tally.own_losses <= 2, "a band of {size} costs a village with a tower and four militia little: {}", report.text);
    assert!(militia.iter().filter(|m| s.military.soldier(**m).is_some()).count() >= 2);
    s.validate_entities(d).unwrap();
}

/// Runs until the enemies are gone, calling `each` after every tick in which a combat round
/// happened.
fn watch_rounds(s: &mut GameState, mut each: impl FnMut(&GameState)) {
    for _ in 0..2 * TICKS_PER_DAY {
        run(s, 1);
        if s.military.last_round().tick() == s.clock.tick {
            each(s);
        }
        if s.military.warbands.is_empty() {
            break;
        }
    }
}

#[test]
fn the_last_round_records_shots_and_strikers_for_the_renderer() {
    let d = defs();
    // A watchtower built next to plundering bandits shoots at them.
    let mut s = flat_state(48);
    let hall = spawn(&mut s, "town_hall", 20, 20);
    let depot = crate::test_support::depot(&s);
    crate::test_support::link(&mut s, hall, depot);
    run(&mut s, 2);
    assert_eq!(s.military.last_round().tick(), 0, "no round without enemies");
    exec(&mut s, Command::DebugSpawnThreat { kingdom: None, size: 4 });
    // Wait until the band stands at its target.
    let standing = |s: &GameState| {
        s.military.warbands.first().and_then(|w| w.fighters.first()).and_then(|f| s.population.walking.traveler(f.walker))
            .filter(|t| !t.motion().is_moving() && t.motion().is_visible())
            .map(|t| t.motion().pos().tile())
    };
    run_until(&mut s, TICKS_PER_DAY, |s| standing(s).is_some());
    let Some(at) = standing(&s) else { panic!("the bandits never stood still") };
    let tower_kind = d.building_id("watchtower").unwrap();
    let tower = [(2, 0), (-2, 0), (0, 2), (0, -2), (3, 1), (-3, -1), (1, 3), (-1, -3)]
        .into_iter()
        .find_map(|(dx, dy)| crate::buildings::spawn_finished(&mut s, d, tower_kind, at + IVec2::new(dx, dy), Default::default()))
        .expect("room for a tower near the bandits");
    let tower_rect = s.building(tower).unwrap().rect();
    let mut arrows = 0;
    watch_rounds(&mut s, |s| {
        let round = s.military.last_round();
        arrows += round.shots().iter().filter(|shot| shot.player && shot.from == tower_rect && shot.kind == crate::military::ShotKind::Arrow).count();
        // A tower shoots by itself: no shooting walker.
        assert!(round.shots().iter().filter(|shot| shot.from == tower_rect).all(|shot| shot.shooter.is_none()));
        // Every shot flies from its shooter to a target standing elsewhere.
        assert!(round.shots().iter().all(|shot| shot.from != shot.to));
    });
    assert!(arrows > 0, "the watchtower shot");

    // Militia meeting bandits strike and are shown attacking (the scene of
    // `soldiers_and_towers_beat_bandits_and_the_fallen_are_real_deaths`).
    let (mut s, _hall, barracks) = garrison_town();
    let depot = crate::test_support::depot(&s);
    let at = s.building(depot).unwrap().origin();
    spawn(&mut s, "watchtower", at.x - 3, at.y - 3);
    crate::military::test_enlist(&mut s, d, 8, unit("militia"), barracks);
    run_to_hour(&mut s, 10);
    run(&mut s, 2 * TICKS_PER_HOUR);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: None, size: 6 });
    let round_ticks = d.rules().military.round_ticks;
    let view_rect = crate::grid::TileRect::new(glam::IVec2::ZERO, s.world.size());
    let (mut strikes, mut attack_views, mut pairs) = (0, 0, 0);
    watch_rounds(&mut s, |s| {
        let round = s.military.last_round();
        for v in crate::population::citizens_in(s, d, view_rect) {
            if v.action == crate::population::WalkerAction::Attack {
                assert!(round.struck(v.id), "{:?} attacks without striking", v.id);
                assert!(round.target_of(v.id).is_some(), "{:?} has a target", v.id);
                attack_views += 1;
            }
            strikes += usize::from(round.struck(v.id));
        }
        // Melee blows go between neighbours of opposite sides.
        for strike in round.strikes() {
            let crate::military::StrikeTarget::Walker(target) = strike.target else { continue };
            assert_ne!(strike.attacker, target);
            assert!(strike.from.distance_to_rect(&strike.to) <= 1, "{strike:?}");
            assert_eq!(strike.player, matches!(strike.attacker, crate::population::WalkerId::Citizen(_)));
            assert_eq!(strike.player, matches!(target, crate::population::WalkerId::Traveler(_)));
            let first = round.strikes().iter().find(|x| x.attacker == strike.attacker).map(|x| x.to);
            assert_eq!(round.target_of(strike.attacker), first, "the target is that of the first blow");
            pairs += 1;
        }
    });
    assert!(strikes > 0 && attack_views > 0, "fighters struck ({strikes}) and were shown attacking ({attack_views})");
    assert!(pairs > 0, "melee blows were recorded with their target");
    // A round long ago is no longer current; the record is not part of the compared state.
    let round = s.military.last_round().clone();
    assert!(round.is_current(round.tick(), round_ticks));
    assert!(!round.is_current(round.tick() + u64::from(round_ticks), round_ticks));
    let mut copy = s.military.clone();
    copy.last_round = crate::military::CombatRound::default();
    assert!(copy == s.military, "the record is presentation only");
}

/// Adds an AI kingdom at the east edge of the map at war with the player (aggressive: its armies march on
/// the headquarters), with `soldiers` soldiers.
fn enemy_kingdom(state: &mut GameState, soldiers: u32, personality: crate::defs::Personality) -> KingdomId {
    let mut k = crate::diplomacy::tests::blank_kingdom(defs());
    k.seat = IVec2::new(state.world.width() - 1, state.world.height() / 2);
    k.soldiers = soldiers;
    k.personality = personality;
    k.treasury_milli = 5_000_000;
    k.war = Some(crate::diplomacy::War { since_tick: 0, declared_by_player: false, exhaustion: 0, raids: 0, next_raid_day: 999 });
    state.diplomacy.kingdoms.push(k);
    let n = state.diplomacy.kingdoms.len();
    state.diplomacy.ai_relations = vec![0; n * n];
    KingdomId::from_index(n - 1).unwrap()
}

#[test]
fn an_army_conquers_an_undefended_headquarters_and_the_game_is_lost() {
    let d = defs();
    let mut s = flat_state(48);
    s.settings.defeat_enabled = true;
    let hall = spawn(&mut s, "town_hall", 20, 20);
    run(&mut s, 2);
    let k = enemy_kingdom(&mut s, 30, crate::defs::Personality::Aggressive);
    let events = exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 20 });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    let w = &s.military.warbands()[0];
    assert_eq!(w.kind(), WarbandKind::Army { kingdom: k, conquest: true });
    assert_eq!(w.strength(), 20);
    assert_eq!(s.diplomacy.kingdoms[k.index()].soldiers, 30, "the army left its kingdom (debug soldiers added)");
    let mut over = None;
    for _ in 0..2 * TICKS_PER_DAY {
        for e in run(&mut s, 1) {
            if let SimEvent::GameOver { reason } = e {
                over = Some(reason);
            }
        }
        if over.is_some() {
            break;
        }
    }
    assert_eq!(over, Some(crate::ranks::DefeatReason::Conquered { kingdom: k }));
    assert!(s.military.damage_of(hall) >= threats::structure_hp(&s, d, hall));
    // Nothing moves any more; commands are rejected.
    let tick = s.clock.tick;
    let events = exec(&mut s, Command::SetTaxRate { percent: 5 });
    assert_eq!(rejection(&events), Some(&RejectReason::GameOver));
    assert_eq!(s.clock.tick, tick);
    assert!(crate::ranks::defeat_text(&s, s.game_over().unwrap().reason).contains("erobert"));
}

#[test]
fn defenders_beat_an_army_and_the_kingdom_tires() {
    let d = defs();
    let (mut s, _, barracks) = garrison_town();
    research(&mut s, "weapon_smithing");
    let k = enemy_kingdom(&mut s, 12, crate::defs::Personality::Aggressive);
    crate::military::test_enlist(&mut s, d, 14, unit("swordsman"), barracks);
    run_to_hour(&mut s, 9);
    let events = exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 8 });
    assert!(rejection(&events).is_none());
    let before = s.diplomacy.kingdoms[k.index()].clone();
    let events = run_until_quiet(&mut s, 2);
    let report = s.military.battles().next().unwrap().clone();
    eprintln!("{}", report.text);
    assert_eq!(report.kind, BattleKind::Defence);
    assert_eq!(report.outcome, BattleOutcome::Victory, "{}", report.text);
    let after = &s.diplomacy.kingdoms[k.index()];
    assert!(after.war.unwrap().exhaustion >= before.war.unwrap().exhaustion + d.rules().military.armies.exhaustion_defeat);
    assert!(after.memories.iter().any(|m| m.cause == crate::diplomacy::MemoryCause::BattleLost));
    assert!(after.soldiers < before.soldiers + 8, "fallen soldiers do not return");
    assert!(events.iter().any(|e| matches!(e, SimEvent::EnemyFell { kingdom: Some(_), .. })));
    s.validate_entities(d).unwrap();
}

#[test]
fn gates_stop_enemies_until_they_break_them() {
    let d = defs();
    let mut s = flat_state(48);
    research(&mut s, "city_walls");
    let hall = spawn(&mut s, "town_hall", 20, 20);
    // A wall ring around the town hall with a gate in the east.
    let (lo, hi) = (16, 27);
    let mut gate = None;
    for i in lo..=hi {
        for p in [IVec2::new(i, lo), IVec2::new(i, hi), IVec2::new(lo, i), IVec2::new(hi, i)] {
            if s.buildings.at(p).is_some() {
                continue;
            }
            if p == IVec2::new(hi, 22) {
                gate = Some(spawn(&mut s, "gate", p.x, p.y));
            } else {
                spawn(&mut s, "wall", p.x, p.y);
            }
        }
    }
    let gate = gate.unwrap();
    run(&mut s, 2);
    let k = enemy_kingdom(&mut s, 10, crate::defs::Personality::Aggressive);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 6 });
    let mut broke = false;
    let mut attacked_gate = false;
    let mut gate_blows = 0;
    let gate_rect = s.building(gate).unwrap().rect();
    let view_rect = crate::grid::TileRect::new(IVec2::ZERO, s.world.size());
    for _ in 0..TICKS_PER_DAY {
        for e in run(&mut s, 1) {
            if let SimEvent::StructureDestroyed { building, .. } = e {
                broke |= building == gate;
            }
        }
        attacked_gate |= s.military.warbands.iter().flat_map(|w| w.fighters.iter()).any(|f| f.attacking == Some(gate));
        // Blows against the gate are strikes from the attacker to the gate tile next to it, and
        // the attacker is shown attacking, turned towards the gate.
        let round = s.military.last_round();
        if round.tick() == s.clock.tick {
            for strike in round.strikes() {
                if let StrikeTarget::Building { building, tile } = strike.target {
                    assert_eq!(building, gate);
                    assert!(gate_rect.contains(tile) && strike.to == crate::grid::TileRect::tile(tile));
                    assert_eq!(strike.from.distance_to(tile), 1, "the attacker stands next to the struck tile");
                    assert!(!strike.player && round.struck(strike.attacker));
                    let view = crate::population::walker_view(&s, d, strike.attacker).unwrap();
                    assert_eq!(view.action, crate::population::WalkerAction::Attack);
                    let step = tile - view.pos.tile();
                    let facing = view.facing.offset();
                    assert!(facing.x * step.x + facing.y * step.y > 0, "{:?} faces {:?}, the gate lies at {step}", strike.attacker, view.facing);
                    gate_blows += 1;
                }
            }
            assert!(crate::population::citizens_in(&s, d, view_rect).all(|v| v.action != crate::population::WalkerAction::Attack || round.struck(v.id)));
        }
        // No enemy ever stands inside the ring while the gate holds.
        if !broke {
            for w in &s.military.warbands {
                for f in &w.fighters {
                    let p = s.population.walking.traveler(f.walker).unwrap().motion().pos().tile();
                    assert!(!(p.x > lo && p.x < hi && p.y > lo && p.y < hi), "an enemy passed the gate at {p}");
                }
            }
        }
        if broke {
            break;
        }
    }
    assert!(attacked_gate, "the enemies attacked the gate");
    assert!(gate_blows > 0, "the blows against the gate were recorded");
    assert!(broke, "the gate was broken down");
    assert!(s.building(gate).is_none());
    let _ = hall;
    s.validate_entities(d).unwrap();
}

/// Every BattleLost memory of kingdom `k` (in centi-points).
fn battle_lost(state: &GameState, k: KingdomId) -> usize {
    state.diplomacy.kingdoms[k.index()].memories.iter().filter(|m| m.cause == crate::diplomacy::MemoryCause::BattleLost).count()
}

#[test]
fn peace_turns_back_an_approaching_army_without_a_battle_report() {
    let d = defs();
    let mut s = flat_state(48);
    spawn(&mut s, "town_hall", 20, 20);
    run(&mut s, 2);
    let k = enemy_kingdom(&mut s, 30, crate::defs::Personality::Aggressive);
    let events = exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 12 });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    let soldiers = s.diplomacy.kingdoms[k.index()].soldiers;
    let mut events = Vec::new();
    crate::diplomacy::actions::make_peace(&mut s, d, k, crate::diplomacy::PeaceTerms::White, &mut events);
    events.extend(run(&mut s, 30));
    assert!(s.military.warbands().is_empty(), "the army turned back");
    assert_eq!(s.diplomacy.kingdoms[k.index()].soldiers, soldiers + 12, "its soldiers are home again");
    assert!(battle(&events).is_none(), "no battle was fought: {events:?}");
    assert_eq!(s.military.battles().count(), 0);
    assert_eq!(battle_lost(&s, k), 0, "a peace is no lost battle");
    s.validate_entities(d).unwrap();
}

#[test]
fn peace_withdraws_an_army_from_the_map_without_a_fight_or_a_report() {
    let d = defs();
    let mut s = flat_state(48);
    research(&mut s, "city_walls");
    spawn(&mut s, "town_hall", 20, 20);
    run(&mut s, 2);
    let k = enemy_kingdom(&mut s, 30, crate::defs::Personality::Aggressive);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 12 });
    run_until(&mut s, 2 * TICKS_PER_HOUR, |s| s.military.enemies_on_map());
    run(&mut s, 20);
    assert!(s.military.enemies_on_map(), "the army is on the map");
    let soldiers = s.diplomacy.kingdoms[k.index()].soldiers;
    let mut events = Vec::new();
    crate::diplomacy::actions::make_peace(&mut s, d, k, crate::diplomacy::PeaceTerms::White, &mut events);
    run(&mut s, 10);
    assert_eq!(s.military.warbands()[0].stage(), WarbandStage::Withdrawing);
    assert!(!s.military.enemies_on_map() && !alert(&s), "a withdrawing army is no threat");
    events.extend(run_until_quiet(&mut s, 1));
    assert!(battle(&events).is_none(), "no battle was fought: {:?}", s.military.battles().map(|b| b.text.clone()).collect::<Vec<_>>());
    assert_eq!(battle_lost(&s, k), 0);
    assert_eq!(s.diplomacy.kingdoms[k.index()].soldiers, soldiers + 12, "all of them went home");
    s.validate_entities(d).unwrap();
}

/// Three wells around the entrance of the fixture depot (placement only warns about that).
fn wall_in_entrance(s: &mut GameState, building: BuildingId) {
    let d = defs();
    let entrance = s.building(building).unwrap().entrance();
    let well = d.building_id("well").unwrap();
    for dir in crate::grid::Direction::ALL {
        let p = entrance + dir.offset();
        if s.buildings.at(p).is_none() && s.world.in_bounds(p) {
            let check = crate::buildings::check_placement(s, d, well, p, crate::buildings::Rotation::default());
            assert!(check.ok, "{:?}", check.errors);
            crate::buildings::spawn_finished(s, d, well, p, crate::buildings::Rotation::default()).unwrap();
        }
    }
}

#[test]
fn bandits_reach_a_storage_whose_entrance_is_built_over() {
    let d = defs();
    let mut s = flat_state(48);
    let depot = crate::test_support::depot(&s);
    wall_in_entrance(&mut s, depot);
    run(&mut s, 2);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: None, size: 4 });
    let events = run_until_quiet(&mut s, 2);
    assert!(events.iter().any(|e| matches!(e, SimEvent::StockLost { buildings, .. } if buildings.contains(&depot))), "they plundered the depot from its side");
    assert!(!s.military.enemies_on_map() && !alert(&s));
    s.validate_entities(d).unwrap();
}

#[test]
fn enemies_without_any_way_to_a_target_withdraw() {
    let d = defs();
    let mut s = flat_state(48);
    // The only storage stands on an island in the corner of the map.
    let depot = crate::test_support::depot(&s);
    let rect = s.building(depot).unwrap().rect();
    for p in rect.expanded(2).clipped(48, 48).tiles() {
        if rect.expanded(1).contains(p) && !rect.contains(p) {
            s.world.tile_mut(p).terrain = crate::world::Terrain::ShallowWater;
        }
    }
    crate::navigation::rebuild(&mut s, d);
    run(&mut s, 2);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: None, size: 4 });
    let mut events = Vec::new();
    for _ in 0..TICKS_PER_DAY {
        events.extend(run(&mut s, 1));
        if s.military.warbands().is_empty() {
            break;
        }
    }
    assert!(s.military.warbands().is_empty(), "the bandits gave up and left the map");
    let notes: Vec<String> = events.iter().filter_map(|e| if let SimEvent::Notification(n) = e { Some(n.text.clone()) } else { None }).collect();
    assert!(notes.iter().any(|t| t.contains("keinen Weg")), "{notes:?}");
    assert!(!events.iter().any(|e| matches!(e, SimEvent::StockLost { .. })));
    assert!(!alert(&s));
    s.validate_entities(d).unwrap();
}

#[test]
fn blocked_enemies_try_again_once_a_way_opens() {
    let d = defs();
    let mut s = flat_state(48);
    let hall = spawn(&mut s, "town_hall", 20, 20);
    run(&mut s, 2);
    let k = enemy_kingdom(&mut s, 30, crate::defs::Personality::Aggressive);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 4 });
    run_until(&mut s, 2 * TICKS_PER_HOUR, |s| s.military.enemies_on_map());
    // A fighter whose search failed (`NoPath`) asks again once the retry is due.
    let walker = s.military.warbands()[0].fighters()[0].walker();
    let retry_at = s.clock.tick + 1;
    s.population.walking.travelers.get_mut(walker).unwrap().motion.phase = crate::population::TripPhase::NoPath { retry_at };
    run(&mut s, 2 * u64::from(d.rules().military.round_ticks));
    let phase = s.population.walking.traveler(walker).unwrap().motion().phase();
    assert!(!matches!(phase, crate::population::TripPhase::NoPath { .. }), "the fighter is on its way again: {phase:?}");
    let _ = hall;
}

/// A wall ring around (16..27)² with a gate in the east (or none).
fn wall_ring(s: &mut GameState, with_gate: bool) -> Option<BuildingId> {
    let (lo, hi) = (16, 27);
    let mut gate = None;
    for i in lo..=hi {
        for p in [IVec2::new(i, lo), IVec2::new(i, hi), IVec2::new(lo, i), IVec2::new(hi, i)] {
            if s.buildings.at(p).is_some() {
                continue;
            }
            if with_gate && p == IVec2::new(hi, 22) {
                gate = Some(spawn(s, "gate", p.x, p.y));
            } else {
                spawn(s, "wall", p.x, p.y);
            }
        }
    }
    gate
}

#[test]
fn enemies_break_the_gate_to_soldiers_behind_it() {
    let d = defs();
    let mut s = flat_state(48);
    research(&mut s, "militia_training");
    spawn(&mut s, "town_hall", 20, 20);
    let gate = wall_ring(&mut s, true).unwrap();
    let post = spawn(&mut s, "well", 24, 22);
    run(&mut s, 2);
    run_to_hour(&mut s, 9);
    // Soldiers who do not fight (retreat order) stand at their post inside the walls, within
    // the army's reach.
    let ids = crate::military::test_enlist(&mut s, d, 4, unit("militia"), post);
    exec(&mut s, Command::CreateCompany { soldiers: ids });
    exec(&mut s, Command::CompanyOrder { company: 1, order: Order::Retreat });
    run(&mut s, TICKS_PER_HOUR);
    let k = enemy_kingdom(&mut s, 20, crate::defs::Personality::Aggressive);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 3 });
    let mut broke = false;
    for _ in 0..TICKS_PER_DAY {
        for e in run(&mut s, 1) {
            broke |= matches!(e, SimEvent::StructureDestroyed { building, .. } if building == gate);
        }
        if broke {
            break;
        }
    }
    assert!(broke, "the enemies broke the gate instead of waiting in front of it");
    s.validate_entities(d).unwrap();
}

#[test]
fn a_walled_in_headquarters_is_attacked_through_a_breach() {
    let d = defs();
    let mut s = flat_state(48);
    research(&mut s, "militia_training");
    let hall = spawn(&mut s, "town_hall", 20, 20);
    wall_ring(&mut s, false);
    run(&mut s, 2);
    // Threats still come: they enter where they can reach the walls.
    let now = s.clock.tick;
    assert!(threats::spawn_bandits(&mut s, d, 100, now, &mut Vec::new()).0 > 0);
    s.military.warbands.clear();
    let k = enemy_kingdom(&mut s, 30, crate::defs::Personality::Aggressive);
    assert!(threats::dispatch_army(&mut s, d, k, &mut Vec::new()));
    s.military.warbands.clear();
    let events = exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 10 });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    let mut breach = false;
    let mut stormed = false;
    for _ in 0..TICKS_PER_DAY {
        for e in run(&mut s, 1) {
            breach |= matches!(e, SimEvent::StructureDestroyed { .. });
        }
        stormed |= s.military.warbands.iter().flat_map(|w| w.fighters.iter()).any(|f| f.attacking == Some(hall));
        if stormed {
            break;
        }
    }
    assert!(breach && stormed, "the army broke through the wall (breach {breach}) and stormed the town hall");
    // No company can leave the ring for a campaign: the order is refused, not accepted silently.
    let mut s = flat_state(48);
    research(&mut s, "militia_training");
    let hall = spawn(&mut s, "town_hall", 20, 20);
    wall_ring(&mut s, false);
    run(&mut s, 2);
    let k = enemy_kingdom(&mut s, 30, crate::defs::Personality::Aggressive);
    let ids = crate::military::test_enlist(&mut s, d, 2, unit("militia"), hall);
    exec(&mut s, Command::CreateCompany { soldiers: ids });
    // The read-only check needs the walking grid's components up to date (between ticks they
    // usually are; otherwise the order itself is refused, see below).
    crate::navigation::prepare(&mut s, d);
    let problem = campaign_problem(&s, d, 1, k, false);
    assert!(problem.as_deref().is_some_and(|p| p.contains("Kartenrand")), "{problem:?}");
    let events = exec(&mut s, Command::CompanyOrder { company: 1, order: Order::Campaign { kingdom: k, siege: false } });
    assert!(rejection(&events).is_some());
    assert!(s.military.campaigns().is_empty());
    assert_eq!(s.military.company(1).unwrap().order(), &Order::Station);
}

#[test]
fn companies_defend_buildings_and_patrol_their_route() {
    let d = defs();
    let (mut s, hall, barracks) = garrison_town();
    let tower = spawn(&mut s, "watchtower", 30, 30);
    let soldiers = crate::military::test_enlist(&mut s, d, 6, unit("militia"), barracks);
    run_to_hour(&mut s, 9);
    let events = exec(&mut s, Command::CreateCompany { soldiers: soldiers[..4].to_vec() });
    assert!(events.contains(&SimEvent::CompanyChanged { company: 1 }));
    let company = s.military.company(1).unwrap();
    assert_eq!(company.name(), "1. Kompanie");
    assert_eq!(company.members().len(), 4);
    // Defend the watchtower: one fits inside, the others stand around it.
    exec(&mut s, Command::CompanyOrder { company: 1, order: Order::Defend(DefendTarget::Building(tower)) });
    run(&mut s, TICKS_PER_HOUR);
    let inside = soldiers[..4].iter().filter(|id| s.population.citizen(**id).unwrap().motion().inside() == Some(tower)).count();
    assert_eq!(inside, 1, "the watchtower holds one soldier");
    let tower_rect = s.building(tower).unwrap().rect();
    for id in &soldiers[..4] {
        let c = s.population.citizen(*id).unwrap();
        let p = c.motion().inside().map_or(c.motion().pos().tile(), |b| s.building(b).unwrap().entrance());
        assert!(tower_rect.distance_to(p) <= 3, "{id:?} stands at the tower ({p})");
        assert_eq!(c.activity(), crate::population::Activity::OnDuty);
    }
    assert!(crate::population::activity_text(&s, d, soldiers[0]).is_some_and(|t| t.contains("Wache") || t.contains("Dienst")));
    // The other two stay at their post: the barracks.
    let at_post = soldiers[4..].iter().all(|id| s.population.citizen(*id).unwrap().motion().inside() == Some(barracks));
    assert!(at_post, "unassigned soldiers stand at their training building");
    // Patrol: the soldiers walk from waypoint to waypoint.
    let route = vec![IVec2::new(10, 10), IVec2::new(10, 36)];
    exec(&mut s, Command::CompanyOrder { company: 1, order: Order::Patrol { route: route.clone() } });
    let mut reached = [false, false];
    for _ in 0..6 * TICKS_PER_HOUR {
        run(&mut s, 10);
        for (i, p) in route.iter().enumerate() {
            reached[i] |= soldiers[..4].iter().any(|id| s.population.citizen(*id).unwrap().motion().pos().tile() == *p);
        }
    }
    assert_eq!(reached, [true, true], "the patrol walked its route");
    // Problems: bad routes, unknown company, campaign without war.
    let bad = Command::CompanyOrder { company: 1, order: Order::Patrol { route: vec![IVec2::new(1, 1)] } };
    assert!(matches!(rejection(&exec(&mut s, bad)), Some(RejectReason::Military { reason }) if reason.contains("2 bis 8 Wegpunkte")));
    let k = KingdomId::from_index(0).unwrap();
    assert!(order_problem(&s, d, 1, &Order::Campaign { kingdom: k, siege: false }).is_some());
    // Moving soldiers between companies, disbanding, dismissing.
    exec(&mut s, Command::CreateCompany { soldiers: Vec::new() });
    exec(&mut s, Command::AssignSoldiers { soldiers: vec![soldiers[0]], company: Some(2) });
    assert_eq!(s.military.company(2).unwrap().members(), &[soldiers[0]]);
    assert_eq!(s.military.company(1).unwrap().members().len(), 3);
    exec(&mut s, Command::DisbandCompany { company: 1 });
    assert!(s.military.company(1).is_none());
    assert!(soldiers[1..4].iter().all(|id| s.military.soldier(*id).unwrap().company().is_none()));
    let before = s.workforce.available();
    exec(&mut s, Command::DismissSoldiers { soldiers: vec![soldiers[5]] });
    assert!(s.military.soldier(soldiers[5]).is_none());
    assert!(!s.population.citizen(soldiers[5]).unwrap().is_soldier());
    assert_eq!(s.workforce.available(), before + 1, "back in the workforce");
    let _ = hall;
    s.validate_entities(d).unwrap();
}

#[test]
fn a_campaign_beats_the_field_army_and_a_siege_forces_peace() {
    let d = defs();
    let (mut s, _, barracks) = garrison_town();
    research(&mut s, "weapon_smithing");
    research(&mut s, "siege_engineering");
    let k = enemy_kingdom(&mut s, 10, crate::defs::Personality::Expansive);
    s.diplomacy.kingdoms[k.index()].population = 120;
    let mut army = crate::military::test_enlist(&mut s, d, 16, unit("swordsman"), barracks);
    army.extend(crate::military::test_enlist(&mut s, d, 2, unit("siege"), barracks));
    run_to_hour(&mut s, 8);
    exec(&mut s, Command::CreateCompany { soldiers: army.clone() });
    let gold = s.economy.treasury();
    let events = exec(&mut s, Command::CompanyOrder { company: 1, order: Order::Campaign { kingdom: k, siege: true } });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    assert_eq!(s.military.campaigns().len(), 1);
    // Muster, march, battle, siege, peace, return.
    let mut kinds = Vec::new();
    let mut peace = false;
    for _ in 0..6 * TICKS_PER_DAY {
        for e in run(&mut s, 1) {
            match e {
                SimEvent::BattleEnded { kind, outcome, .. } => kinds.push((kind, outcome)),
                SimEvent::PeaceMade { kingdom, .. } if kingdom == k => peace = true,
                _ => {}
            }
        }
        if s.military.campaigns().is_empty() && !kinds.is_empty() {
            break;
        }
    }
    let texts: Vec<String> = s.military.battles().map(|b| b.text.clone()).collect();
    assert_eq!(kinds.first(), Some(&(BattleKind::FieldBattle, BattleOutcome::Victory)), "{texts:?}");
    assert_eq!(kinds.get(1), Some(&(BattleKind::Siege, BattleOutcome::Victory)), "{texts:?}");
    assert!(peace, "the siege forced a peace");
    assert!(!s.diplomacy.kingdoms[k.index()].at_war());
    assert!(s.economy.treasury() > gold, "loot and reparations");
    assert!(s.military.campaigns().is_empty(), "the company came home");
    assert!(s.military.soldiers().all(|(_, x)| x.away().is_none()));
    assert_eq!(s.military.company(1).unwrap().order(), &Order::Station);
    s.validate_entities(d).unwrap();
}

/// Food units in all storages.
fn food_in_store(s: &GameState) -> u32 {
    defs()
        .resources()
        .iter()
        .filter(|r| r.category == crate::defs::ResourceCategory::Food && r.nutrition > 0.0)
        .map(|r| s.economy.stock().get(r.id))
        .sum()
}

#[test]
fn a_retreating_campaign_brings_its_unused_provisions_home() {
    let d = defs();
    let (mut s, _, barracks) = garrison_town();
    let k = enemy_kingdom(&mut s, 10, crate::defs::Personality::Expansive);
    let army = crate::military::test_enlist(&mut s, d, 8, unit("militia"), barracks);
    run_to_hour(&mut s, 8);
    exec(&mut s, Command::CreateCompany { soldiers: army });
    let events = exec(&mut s, Command::CompanyOrder { company: 1, order: Order::Campaign { kingdom: k, siege: false } });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    run_until(&mut s, TICKS_PER_DAY, |s| s.military.campaigns().first().is_some_and(|c| matches!(c.stage(), CampaignStage::Marching { .. })));
    let camp = s.military.campaigns()[0].clone();
    let taken: u32 = camp.provisions().iter().map(|m| m.amount).sum();
    assert!(taken > 0, "provisions for the whole campaign were taken");
    // The provisions and the soldier-hours are part of the saved state.
    let bytes = bincode::serde::encode_to_vec(&s.military, bincode::config::standard()).expect("encode");
    let (loaded, _): (Military, usize) = bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).expect("decode");
    assert_eq!(loaded.campaigns()[0].provisions(), camp.provisions());
    assert!(loaded == s.military);
    // Two hours on the march, then the company turns back and is home two hours later.
    run(&mut s, 2 * TICKS_PER_HOUR);
    let before = food_in_store(&s);
    let events = exec(&mut s, Command::CompanyOrder { company: 1, order: Order::Retreat });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    let mut back_text = None;
    for _ in 0..TICKS_PER_DAY {
        for e in run(&mut s, 1) {
            if let SimEvent::Notification(n) = e
                && n.text.contains("heimgekehrt")
            {
                back_text = Some(n.text.clone());
            }
        }
        if s.military.campaigns().is_empty() {
            break;
        }
    }
    let text = back_text.expect("the company came home");
    let eaten = s.military.campaigns().first().map_or(0, |c| c.provisions_eaten(d));
    assert_eq!(eaten, 0, "the campaign is over");
    // About four hours of rations for eight soldiers were eaten (0.5 a day each: 1 unit).
    let returned = food_in_store(&s).saturating_sub(before);
    assert!(returned + 2 >= taken && returned < taken, "{returned} of {taken} provisions came back: {text}");
    assert!(text.contains("Vom Proviant kommen") && text.contains("Einheiten Nahrung zurück ins Lager"), "{text}");
    s.validate_entities(d).unwrap();
}

#[test]
fn soldiers_away_move_into_the_remaining_headquarters_when_theirs_is_demolished() {
    let d = defs();
    let (mut s, hall, barracks) = garrison_town();
    research(&mut s, "fortification");
    let castle = spawn(&mut s, "castle", 32, 32);
    let k = enemy_kingdom(&mut s, 10, crate::defs::Personality::Expansive);
    let army = crate::military::test_enlist(&mut s, d, 4, unit("militia"), barracks);
    run_to_hour(&mut s, 8);
    exec(&mut s, Command::CreateCompany { soldiers: army.clone() });
    exec(&mut s, Command::CompanyOrder { company: 1, order: Order::Campaign { kingdom: k, siege: false } });
    run_until(&mut s, TICKS_PER_DAY, |s| s.military.campaigns().first().is_some_and(|c| matches!(c.stage(), CampaignStage::Marching { .. })));
    let away: Vec<CitizenId> = army.iter().copied().filter(|id| s.military.soldier(*id).is_some_and(|x| x.away().is_some())).collect();
    assert!(!away.is_empty(), "soldiers left the map");
    assert!(away.iter().all(|id| s.population.citizen(*id).unwrap().motion().inside() == Some(hall)), "hidden in the main headquarters");
    let events = exec(&mut s, Command::DemolishBuilding { building: hall });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    for id in &away {
        let c = s.population.citizen(*id).unwrap();
        assert_eq!(c.motion().inside(), Some(castle), "{id:?} waits in the castle now");
        let view = crate::population::citizen_view(&s, d, *id).unwrap();
        assert!(!view.visible, "{id:?} stays off the map");
    }
    s.validate_entities(d).unwrap();
    run(&mut s, TICKS_PER_HOUR);
    assert!(s.military.campaigns().first().is_some_and(|c| matches!(c.stage(), CampaignStage::Marching { .. })));
    assert!(away.iter().all(|id| crate::population::citizen_view(&s, d, *id).is_some_and(|v| !v.visible)), "still hidden while away");
    s.validate_entities(d).unwrap();
}

#[test]
fn a_walled_in_army_disbands_after_the_peace() {
    let d = defs();
    let mut s = flat_state(48);
    research(&mut s, "city_walls");
    spawn(&mut s, "town_hall", 20, 20);
    run(&mut s, 2);
    let k = enemy_kingdom(&mut s, 30, crate::defs::Personality::Aggressive);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 8 });
    run_until(&mut s, 2 * TICKS_PER_HOUR, |s| s.military.enemies_on_map());
    run(&mut s, 20);
    let soldiers = s.diplomacy.kingdoms[k.index()].soldiers;
    let mut events = Vec::new();
    crate::diplomacy::actions::make_peace(&mut s, d, k, crate::diplomacy::PeaceTerms::White, &mut events);
    // The army stands inside a closed ring of walls (no gate) around the town hall.
    let walkers: Vec<TravelerId> = s.military.warbands()[0].fighters().iter().map(|f| f.walker()).collect();
    for (i, w) in walkers.iter().enumerate() {
        let tile = IVec2::new(18 + (i as i32 % 2), 25);
        crate::population::movement::place(&mut s, WalkerId::Traveler(*w), crate::population::Motion::standing(tile));
    }
    wall_ring(&mut s, false);
    run(&mut s, 10);
    assert_eq!(s.military.warbands()[0].stage(), WarbandStage::Withdrawing);
    let hours = u64::from(d.rules().military.armies.withdraw_stuck_hours);
    let mut notice = None;
    let mut ticks = 0;
    for _ in 0..(hours + 2) * TICKS_PER_HOUR {
        ticks += 1;
        for e in run(&mut s, 1) {
            if let SimEvent::Notification(n) = e
                && n.text.contains("löst sich auf")
            {
                notice = Some(n.text.clone());
            }
        }
        if s.military.warbands().is_empty() {
            break;
        }
    }
    assert!(s.military.warbands().is_empty(), "the army is gone");
    assert!(ticks >= hours * TICKS_PER_HOUR - u64::from(d.rules().military.round_ticks) * 2, "it waited for a way first ({ticks} ticks)");
    let notice = notice.expect("a notice");
    assert!(notice.starts_with("Das Heer von ") && notice.contains("findet keinen Weg aus Euren Mauern"), "{notice}");
    assert_eq!(s.diplomacy.kingdoms[k.index()].soldiers, soldiers + 8, "its soldiers went home");
    assert!(s.population.walking.travelers().all(|(_, t)| t.warband().is_none()));
    s.validate_entities(d).unwrap();
}

#[test]
fn fleeing_bandits_without_a_way_off_the_map_scatter() {
    let d = defs();
    let mut s = flat_state(48);
    run(&mut s, 2);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: None, size: 4 });
    run_until(&mut s, 2 * TICKS_PER_HOUR, |s| s.military.enemies_on_map());
    // The band flees on an island (its bridge was torn down): water all around (14..20)².
    for p in crate::grid::TileRect::new(IVec2::splat(12), IVec2::splat(10)).tiles() {
        if !crate::grid::TileRect::new(IVec2::splat(14), IVec2::splat(6)).contains(p) {
            s.world.tile_mut(p).terrain = crate::world::Terrain::ShallowWater;
        }
    }
    crate::navigation::rebuild(&mut s, d);
    let walkers: Vec<TravelerId> = s.military.warbands()[0].fighters().iter().map(|f| f.walker()).collect();
    for (i, w) in walkers.iter().enumerate() {
        let tile = IVec2::new(15 + i as i32 % 3, 16);
        crate::population::movement::place(&mut s, WalkerId::Traveler(*w), crate::population::Motion::standing(tile));
    }
    s.military.warbands[0].stage = WarbandStage::Fleeing;
    let hours = u64::from(d.rules().military.armies.withdraw_stuck_hours);
    let mut notes = Vec::new();
    let mut ticks = 0;
    for _ in 0..(hours + 2) * TICKS_PER_HOUR {
        ticks += 1;
        for e in run(&mut s, 1) {
            if let SimEvent::Notification(n) = e {
                notes.push(n.text.clone());
            }
        }
        if s.military.warbands().is_empty() {
            break;
        }
    }
    assert!(s.military.warbands().is_empty(), "the band is gone, no endless alarm");
    assert!(!s.military.enemies_on_map());
    assert!(ticks >= hours * TICKS_PER_HOUR - u64::from(d.rules().military.round_ticks) * 2, "it looked for a way first ({ticks} ticks)");
    assert!(notes.iter().any(|t| t == "Die Räuber finden keinen Weg aus Eurem Land und zerstreuen sich."), "{notes:?}");
    assert!(s.population.walking.travelers().all(|(_, t)| t.warband().is_none()));
    s.validate_entities(d).unwrap();
}

#[test]
fn research_strengthens_units_and_walls() {
    let d = defs();
    let mut s = flat_state(40);
    let archer = FighterKind::Unit(unit("archer"));
    let before = fighter_stats(&s, d, archer, true);
    research(&mut s, "crossbows");
    let after = fighter_stats(&s, d, archer, true);
    assert_eq!(after.ranged, before.ranged * 110 / 100, "UnitBonus +10 % attack");
    assert_eq!(fighter_stats(&s, d, archer, false), before, "AI units get no player research");
    let wall = spawn(&mut s, "wall", 5, 5);
    let hp = threats::structure_hp(&s, d, wall);
    research(&mut s, "siege_engineering");
    assert_eq!(threats::structure_hp(&s, d, wall), hp * 125 / 100, "WallStrengthBonus +25 %");
    assert!(s.research.unit_unlocked(d, unit("siege")));
    assert!(!s.research.unit_unlocked(d, unit("cavalry")));
    // Damage formula: 3 × 30 / (2 + 10) = 7.5 → 7.
    assert_eq!(combat::test_hit(d, 3000, 2000), 7);
    assert_eq!(combat::test_hit(d, 1, 50_000), 1, "at least one hit point");
}

#[test]
fn ranks_rise_with_the_kingdom() {
    let d = defs();
    let (mut s, _, _) = garrison_town();
    assert_eq!(crate::ranks::current_rank(&s, d).map(|r| r.name.as_str()), Some("Siedlung"));
    let report = crate::report::rank_report(&s, d);
    let next = report.next.unwrap();
    assert_eq!(next.name, "Dorf");
    assert!(next.requirements.iter().any(|r| r.label == "Einwohner" && r.required == 25));
    crate::test_support::set_adults(&mut s, 30);
    for x in 0..12 {
        spawn(&mut s, "well", 2 + 2 * x, 2);
    }
    let events = run_until_quiet_hour(&mut s);
    assert!(events.contains(&SimEvent::RankAchieved { rank: 1 }), "{:?}", crate::report::rank_report(&s, d));
    assert_eq!(crate::ranks::prestige(&s, d), 2);
    assert_eq!(crate::report::rank_report(&s, d).reached.len(), 1);
}

/// Runs one game hour and returns the events.
fn run_until_quiet_hour(state: &mut GameState) -> Vec<SimEvent> {
    run(state, TICKS_PER_HOUR)
}

#[test]
fn nobody_left_means_defeat() {
    let mut s = flat_state(32);
    s.settings.defeat_enabled = true;
    crate::test_support::set_adults(&mut s, 0);
    let events = run(&mut s, TICKS_PER_HOUR);
    assert!(events.contains(&SimEvent::GameOver { reason: crate::ranks::DefeatReason::Depopulated }));
    let report = crate::report::rank_report(&s, defs());
    assert!(report.game_over.is_some_and(|g| g.text.contains("Niemand")));
}

#[test]
fn event_soldiers_hunt_and_muster() {
    let d = defs();
    let (mut s, hall, barracks) = garrison_town();
    let soldiers = crate::military::test_enlist(&mut s, d, 8, unit("militia"), barracks);
    run_to_hour(&mut s, 9);
    let unrest = d.event_id("political_unrest").unwrap();
    exec(&mut s, Command::DebugTriggerEvent { event: unrest });
    let serial = s.events.pending().unwrap().serial;
    let events = exec(&mut s, Command::ResolveEvent { serial, choice: 3 });
    assert!(rejection(&events).is_none(), "{:?}", rejection(&events));
    assert!(s.military.muster_until() > s.clock.tick);
    run(&mut s, TICKS_PER_HOUR);
    let hall_rect = s.building(hall).unwrap().rect();
    let near = soldiers.iter().filter(|id| {
        let c = s.population.citizen(**id).unwrap();
        let p = c.motion().inside().map_or(c.motion().pos().tile(), |b| s.building(b).unwrap().entrance());
        hall_rect.distance_to(p) <= 3
    });
    assert_eq!(near.count(), 8, "all soldiers stand at the town hall");
    let report = crate::report::military_report(&s, d);
    assert!(report.alarm_text.is_some_and(|t| t.contains("Hauptquartier")));
}

#[test]
fn military_report_lists_units_companies_threats_and_recruits() {
    let d = defs();
    let (mut s, _, barracks) = garrison_town();
    trained_militia(&mut s, barracks, 2);
    let ids: Vec<CitizenId> = s.military.soldiers().map(|(id, _)| id).collect();
    exec(&mut s, Command::CreateCompany { soldiers: ids });
    exec(&mut s, Command::DebugSpawnThreat { kingdom: None, size: 4 });
    let r = crate::report::military_report(&s, d);
    assert_eq!((r.soldiers, r.ready, r.in_training), (2, 2, 0));
    assert_eq!(r.units.len(), 1);
    assert_eq!(r.units[0].name, "Miliz");
    assert_eq!(r.pay_per_day, 2);
    assert_eq!(r.companies[0].units_text, "2 Milizen");
    assert_eq!(unit_count_text(d, unit("archer"), 1), "1 Bogenschütze");
    assert_eq!(unit_count_text(d, unit("crossbowman"), 6), "6 Armbrustschützen");
    assert_eq!(fighter_count_text(d, FighterKind::Unit(unit("siege")), 2), "2 Belagerungsgeräte");
    assert_eq!(r.companies[0].order_text, "In Bereitschaft an ihren Posten");
    assert_eq!(r.threats.len(), 1);
    assert_eq!(r.threats[0].kind_text, "Räuberbande");
    assert_eq!(r.threats[0].composition_text, "4 Räuber");
    assert!(r.threats[0].eta_text.starts_with("in "), "{}", r.threats[0].eta_text);
    assert!(r.alert, "enemies arrive within the hour");
    let options = crate::report::recruit_options(&s, d, barracks);
    let names: Vec<&str> = options.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["Miliz", "Schwertkämpfer", "Speerträger", "Belagerungsgerät"]);
    assert!(options[0].problem.is_none() && options[0].max_now > 0);
    assert!(options[1].problem.as_ref().is_some_and(|p| p.contains("Forschung")));
}

#[test]
fn validate_catches_broken_military_data() {
    let d = defs();
    let (mut s, _, barracks) = garrison_town();
    let soldiers = crate::military::test_enlist(&mut s, d, 2, unit("militia"), barracks);
    exec(&mut s, Command::CreateCompany { soldiers: soldiers.clone() });
    s.validate_entities(d).unwrap();
    let mut broken = s.clone();
    broken.population.citizens.get_mut(soldiers[0]).unwrap().soldier = None;
    assert!(matches!(broken.validate_entities(d), Err(crate::state::StateError::InvalidMilitary(_))));
    let mut broken = s.clone();
    broken.military.companies[0].members.pop();
    assert!(matches!(broken.validate_entities(d), Err(crate::state::StateError::InvalidMilitary(_))));
    let mut broken = s.clone();
    broken.military.soldiers.get_mut(&soldiers[0]).unwrap().morale = 300;
    assert!(matches!(broken.validate_entities(d), Err(crate::state::StateError::InvalidMilitary(_))));
    let mut broken = s.clone();
    broken.progress.rank = 99;
    assert!(matches!(broken.validate_entities(d), Err(crate::state::StateError::InvalidMilitary(_))));
    // A crafted save must not hold an area to defend off the map (its tiles would overflow) or
    // with a radius outside 1–12.
    for (center, radius) in [(IVec2::new(i32::MAX - 2, 5), 12), (IVec2::new(-1, 5), 3), (IVec2::new(10, 10), 255), (IVec2::new(10, 10), 0)] {
        let mut broken = s.clone();
        broken.military.companies[0].order = Order::Defend(DefendTarget::Area { center, radius });
        assert!(matches!(broken.validate_entities(d), Err(crate::state::StateError::InvalidMilitary(_))), "{center} {radius}");
    }
    let mut fine = s.clone();
    fine.military.companies[0].order = Order::Defend(DefendTarget::Area { center: IVec2::new(0, 0), radius: 12 });
    fine.validate_entities(d).unwrap();
}

#[test]
fn crafted_soldier_hours_are_rejected_and_never_overflow() {
    let d = defs();
    let (mut s, _, barracks) = garrison_town();
    let k = enemy_kingdom(&mut s, 10, crate::defs::Personality::Expansive);
    let army = crate::military::test_enlist(&mut s, d, 3, unit("militia"), barracks);
    run_to_hour(&mut s, 8);
    exec(&mut s, Command::CreateCompany { soldiers: army });
    exec(&mut s, Command::CompanyOrder { company: 1, order: Order::Campaign { kingdom: k, siege: false } });
    run_until(&mut s, TICKS_PER_DAY, |s| s.military.campaigns().first().is_some_and(|c| matches!(c.stage(), CampaignStage::Marching { .. })));
    run(&mut s, 2 * TICKS_PER_HOUR);
    s.validate_entities(d).unwrap();
    let mut broken = s.clone();
    broken.military.campaigns[0].soldier_hours = u64::MAX / 4;
    assert!(matches!(broken.validate_entities(d), Err(crate::state::StateError::InvalidMilitary(_))));
    // The shown value saturates instead of overflowing.
    assert_eq!(broken.military.campaigns()[0].provisions_eaten(d), u32::MAX);
    broken.military.campaigns[0].soldier_hours = u64::MAX;
    assert_eq!(broken.military.campaigns()[0].provisions_eaten(d), u32::MAX);
}

/// Cost of a battle of about 200 fighters in a town of 2000 citizens: 100 own swordsmen and
/// archers against an army of 100. Run with
/// `cargo test --release -p kingdoms_sim battle_performance -- --ignored --nocapture`.
#[test]
#[ignore = "performance measurement; run in release mode"]
fn battle_performance() {
    use std::time::Instant;
    let d = defs();
    let mut s = flat_state(192);
    crate::test_support::set_adults(&mut s, 2000);
    research(&mut s, "weapon_smithing");
    research(&mut s, "archery");
    let hall = spawn(&mut s, "town_hall", 90, 90);
    let barracks = spawn(&mut s, "barracks", 100, 90);
    let post = spawn(&mut s, "archer_post", 100, 96);
    crate::test_support::link(&mut s, hall, barracks);
    crate::military::test_enlist(&mut s, d, 70, unit("swordsman"), barracks);
    crate::military::test_enlist(&mut s, d, 30, unit("archer"), post);
    let k = enemy_kingdom(&mut s, 0, crate::defs::Personality::Aggressive);
    run_to_hour(&mut s, 9);
    exec(&mut s, Command::DebugSpawnThreat { kingdom: Some(k), size: 100 });
    run_until(&mut s, 2 * TICKS_PER_DAY, |s| s.military.enemies_on_map());
    let ticks = 1500;
    let (mut military, mut total, mut max_military, mut max_total) = (0u128, 0u128, 0u128, 0u128);
    let mut fighters_max = 0;
    let started = Instant::now();
    let mut ran = 0u128;
    for _ in 0..ticks {
        ran += 1;
        let steps = s.tick_timed(d);
        let m = steps.iter().find(|(n, _)| *n == "military").map_or(0, |(_, ns)| *ns);
        let t: u128 = steps.iter().map(|(_, ns)| ns).sum();
        military += m;
        total += t;
        max_military = max_military.max(m);
        max_total = max_total.max(t);
        let on_map: usize = s.military.warbands.iter().map(|w| w.fighters.len()).sum();
        fighters_max = fighters_max.max(on_map + s.military.soldiers.len());
        if !s.military.enemies_on_map() {
            break;
        }
    }
    let elapsed = started.elapsed();
    let n = ran.max(1);
    eprintln!(
        "battle: {} citizens, up to {fighters_max} fighters: military avg {:.1} µs/tick (max {:.1} µs), whole tick avg {:.1} µs (max {:.1} µs), {elapsed:?} for {ran} ticks; battles {:?}",
        s.population.len(),
        military as f64 / n as f64 / 1000.0,
        max_military as f64 / 1000.0,
        total as f64 / n as f64 / 1000.0,
        max_total as f64 / 1000.0,
        s.military.battles().map(|b| b.text.clone()).collect::<Vec<_>>()
    );
    s.validate_entities(d).unwrap();
}

#[test]
fn a_breach_names_the_wall_or_gate_with_its_article() {
    use crate::defs::BuildingRole;
    assert_eq!(threats::breach_text(Some(BuildingRole::Gate), "Tor"), "Die Feinde haben das Tor durchbrochen!");
    assert_eq!(threats::breach_text(Some(BuildingRole::WallSegment), "Stadtmauer"), "Die Feinde haben die Stadtmauer durchbrochen!");
}
