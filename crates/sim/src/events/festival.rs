//! Festivals the player holds for gold (docs/game_design.md §7 Feste und Hofhaltung) — a gold
//! sink of the late game with a real effect:
//!
//! [`Command::HoldFestival`](crate::command::Command::HoldFestival) costs `festival.gold_base` +
//! `festival.gold_per_inhabitant` × inhabitants (ledger „Ereignisse“), needs a finished church
//! or market (where the town celebrates) and is possible at most every
//! `festival.cooldown_days` days. It adds the modifiers of the religious festival event (§12):
//! `festival_days` festival days (afternoons celebrated instead of worked) and happiness
//! `happiness` for `happiness_days` days; every kingdom at peace sends envoys and remembers it
//! (relation memory „Festgäste“, `relation` points, at most `relation_max`, fading like every
//! memory, §10).

use serde::{Deserialize, Serialize};

use super::{ModifierKind, ModifierSource};
use crate::command::{CommandResult, RejectReason};
use crate::defs::{Defs, ServiceKind};
use crate::diplomacy::MemoryCause;
use crate::economy::LedgerCategory;
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::state::GameState;

/// Title of the festival in modifier rows and texts.
pub const FESTIVAL_TITLE: &str = "Euer Fest";

/// What a festival costs and brings now and whether it can be held (for the UI).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FestivalReport {
    /// Gold it costs now.
    pub cost: u32,
    /// Why it cannot be held now (German), `None` if it can.
    pub problem: Option<String>,
    /// First day (0-based) the next festival is possible (`None`: now or never held).
    pub next_day: Option<u64>,
    /// Day (0-based) the last festival was held.
    pub last_day: Option<u64>,
    /// True while a festival (also one of an event) is celebrated.
    pub active: bool,
    /// German summary of the cost, e.g. „1060 Gold (100 Gold und 4 Gold je Einwohner)“.
    pub cost_text: String,
    /// German effects, one line each.
    pub effects: Vec<String>,
}

/// Gold a festival costs now: `gold_base` + `gold_per_inhabitant` per inhabitant.
pub fn festival_cost(state: &GameState, defs: &Defs) -> u32 {
    let rules = &defs.rules().festival;
    let people = u32::try_from(state.population.len()).unwrap_or(u32::MAX);
    rules.gold_base.saturating_add(rules.gold_per_inhabitant.saturating_mul(people))
}

/// First day (0-based) the next festival is possible, if one was held.
fn next_day(state: &GameState, defs: &Defs) -> Option<u64> {
    state.events.last_festival_day.map(|d| d.saturating_add(u64::from(defs.rules().festival.cooldown_days)))
}

/// True if a finished church or market (the venues of a festival, §6) stands.
fn has_venue(state: &GameState, defs: &Defs) -> bool {
    state.buildings.iter().any(|(_, b)| {
        b.is_operational()
            && defs
                .buildings()
                .get(b.kind().index())
                .is_some_and(|d| d.services.iter().any(|s| matches!(s.kind, ServiceKind::Religion | ServiceKind::Market)))
    })
}

/// Why no festival can be held now (German), or `None`.
pub fn festival_problem(state: &GameState, defs: &Defs) -> Option<String> {
    if state.game_over().is_some() {
        return Some("Das Spiel ist verloren.".to_owned());
    }
    if !has_venue(state, defs) {
        return Some("Für ein Fest braucht es eine Kirche oder einen Marktplatz.".to_owned());
    }
    if state.events.festival_active(state.clock.tick) {
        return Some("Es wird bereits gefeiert.".to_owned());
    }
    let today = state.clock.total_days();
    if let Some(next) = next_day(state, defs).filter(|d| *d > today) {
        let when = match next - today {
            1 => "morgen".to_owned(),
            2 => "übermorgen".to_owned(),
            n => format!("in {n} Tagen"),
        };
        return Some(format!("Das nächste Fest ist {when} möglich ({}).", crate::time::day_date_string(next)));
    }
    if state.economy.is_bankrupt() {
        return Some("Im Bankrott kann kein Fest ausgerichtet werden.".to_owned());
    }
    let cost = i64::from(festival_cost(state, defs));
    let treasury = state.economy.treasury();
    (treasury < cost).then(|| format!("{}.", crate::report::missing_gold(cost - treasury.max(0))))
}

/// German effects of a festival.
fn effects_text(state: &GameState, defs: &Defs) -> Vec<String> {
    let rules = &defs.rules().festival;
    let days = |n: u16| if n == 1 { "1 Tag".to_owned() } else { format!("{n} Tage") };
    let feast = if rules.festival_days == 1 {
        "Ein Festtag: Nachmittags feiern die Bewohner an Kirche oder Markt, statt zu arbeiten".to_owned()
    } else {
        format!("{} Festtage: Nachmittags feiern die Bewohner an Kirche oder Markt, statt zu arbeiten", rules.festival_days)
    };
    let mut out = vec![feast, format!("Zufriedenheit {:+} für {}", rules.happiness, days(rules.happiness_days))];
    let guests = state.diplomacy.kingdoms().filter(|(_, k)| !k.at_war()).count();
    if rules.relation > 0 && guests > 0 {
        out.push(format!(
            "Gesandte als Gäste: Beziehung {:+} zu {} (Festgäste, höchstens {:+})",
            rules.relation,
            if guests == 1 { "1 Reich".to_owned() } else { format!("{guests} Reichen") },
            rules.relation_max
        ));
    }
    out.push(format!("Das nächste Fest frühestens nach {}", days(rules.cooldown_days)));
    out
}

/// The festival for the UI (see [`FestivalReport`]).
pub fn festival_report(state: &GameState, defs: &Defs) -> FestivalReport {
    let rules = &defs.rules().festival;
    let cost = festival_cost(state, defs);
    let today = state.clock.total_days();
    FestivalReport {
        cost,
        problem: festival_problem(state, defs),
        next_day: next_day(state, defs).filter(|d| *d > today),
        last_day: state.events.last_festival_day,
        active: state.events.festival_active(state.clock.tick),
        cost_text: format!("{cost} Gold ({} Gold und {} Gold je Einwohner)", rules.gold_base, rules.gold_per_inhabitant),
        effects: effects_text(state, defs),
    }
}

/// `HoldFestival`: pays the festival and starts it.
pub(crate) fn hold(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) -> CommandResult {
    if let Some(reason) = festival_problem(state, defs) {
        return Err(RejectReason::FestivalUnavailable { reason });
    }
    let rules = defs.rules().festival;
    let gold = festival_cost(state, defs);
    if state.economy.pay_gold(LedgerCategory::Events, gold).is_err() {
        let missing = i64::from(gold) - state.economy.treasury().max(0);
        return Err(RejectReason::FestivalUnavailable { reason: format!("{}.", crate::report::missing_gold(missing)) });
    }
    super::effects::push_modifier(state, ModifierKind::Festival, 0, rules.festival_days, ModifierSource::Festival);
    super::effects::push_modifier(state, ModifierKind::Happiness, rules.happiness, rules.happiness_days, ModifierSource::Festival);
    state.events.last_festival_day = Some(state.clock.total_days());
    let mut guests = 0u32;
    for k in state.diplomacy.kingdoms.iter_mut().filter(|k| !k.at_war()) {
        crate::diplomacy::relations::remember(
            k,
            defs,
            MemoryCause::Festival,
            f32::from(rules.relation),
            Some(i8::try_from(rules.relation_max).unwrap_or(i8::MAX)),
        );
        guests += 1;
    }
    if guests > 0 {
        crate::diplomacy::relations::announce_states(state, defs, events);
    }
    events.push(SimEvent::FestivalHeld { gold, guests });
    let guests_text = match guests {
        0 => String::new(),
        1 => " Gesandte aus 1 Reich sind zu Gast.".to_owned(),
        n => format!(" Gesandte aus {n} Reichen sind zu Gast."),
    };
    let text = format!(
        "Ihr richtet ein Fest aus ({gold} Gold): Das Volk feiert, die Zufriedenheit steigt für {} um {}.{guests_text}",
        if rules.happiness_days == 1 { "1 Tag".to_owned() } else { format!("{} Tage", rules.happiness_days) },
        rules.happiness
    );
    let mut note = Notification::new(Severity::Good, text).with_kind(NotificationKind::Festival);
    if let Some(at) = crate::economy::storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b)).map(|b| b.entrance()) {
        note = note.at(at);
    }
    events.push(SimEvent::Notification(note));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::test_support::{defs, exec, flat_state, rejection, run, set_adults, spawn};
    use crate::time::TICKS_PER_DAY;

    fn rejected(events: &[SimEvent]) -> Option<String> {
        rejection(events).map(|r| r.to_string())
    }

    #[test]
    fn a_festival_costs_gold_by_the_town_needs_a_venue_and_waits_for_its_turn() {
        let d = defs();
        let mut s = flat_state(48);
        spawn(&mut s, "town_hall", 4, 4);
        set_adults(&mut s, 40);
        assert_eq!(festival_cost(&s, d), 100 + 4 * 40);
        let e = exec(&mut s, Command::HoldFestival);
        assert_eq!(rejected(&e).as_deref(), Some("Für ein Fest braucht es eine Kirche oder einen Marktplatz."));
        spawn(&mut s, "market", 20, 20);
        // Not enough gold.
        s.economy.book_gold(LedgerCategory::Events, -s.economy.treasury() + 100);
        let e = exec(&mut s, Command::HoldFestival);
        assert_eq!(rejected(&e).as_deref(), Some("Es fehlen 160 Gold."));
        s.economy.add_gold(1000);
        let gold = s.economy.treasury();
        let booked = s.economy.ledger().today().gold(LedgerCategory::Events);
        let cost = festival_cost(&s, d);
        let e = exec(&mut s, Command::HoldFestival);
        assert!(rejection(&e).is_none(), "{:?}", rejection(&e));
        assert!(e.iter().any(|e| matches!(e, SimEvent::FestivalHeld { gold, .. } if *gold == cost)));
        assert!(
            e.iter().any(|e| matches!(e, SimEvent::Notification(n) if n.text.starts_with("Ihr richtet ein Fest aus (260 Gold)"))),
            "{e:?}"
        );
        assert_eq!(s.economy.treasury(), gold - i64::from(cost));
        assert_eq!(s.economy.ledger().today().gold(LedgerCategory::Events) - booked, -i64::from(cost), "booked as „Ereignisse“");
        let tick = s.clock.tick;
        assert!(s.events.festival_active(tick));
        assert_eq!(s.events.happiness_modifier(tick), i32::from(d.rules().festival.happiness));
        let report = crate::report::events_report(&s, d);
        assert!(report.modifiers.iter().any(|m| m.source == FESTIVAL_TITLE), "{:?}", report.modifiers);
        // Once per festival and then after the cooldown.
        let e = exec(&mut s, Command::HoldFestival);
        assert_eq!(rejected(&e).as_deref(), Some("Es wird bereits gefeiert."));
        run(&mut s, 2 * TICKS_PER_DAY);
        let r = festival_report(&s, d);
        assert_eq!(r.next_day, Some(12));
        assert_eq!(r.problem.as_deref(), Some("Das nächste Fest ist in 10 Tagen möglich (Herbst · Tag 1 · Jahr 1)."));
        assert_eq!(r.cost_text, "260 Gold (100 Gold und 4 Gold je Einwohner)");
        run(&mut s, 10 * TICKS_PER_DAY);
        assert_eq!(festival_problem(&s, d), None);
        s.validate_entities(d).unwrap();
    }

    #[test]
    fn envoys_of_kingdoms_at_peace_remember_the_festival() {
        let d = defs();
        let mut s = crate::diplomacy::tests::kingdom_fixture(&[glam::IVec2::new(100, 40), glam::IVec2::new(100, 100)]);
        spawn(&mut s, "market", 30, 30);
        s.economy.add_gold(5000);
        let ids: Vec<crate::diplomacy::KingdomId> = s.diplomacy.kingdoms().map(|(id, _)| id).collect();
        assert_eq!(ids.len(), 2);
        let relation = |s: &GameState| -> Vec<i32> { ids.iter().map(|id| crate::diplomacy::relations::relation(s, d, *id)).collect() };
        let before = relation(&s);
        let e = exec(&mut s, Command::HoldFestival);
        assert!(rejection(&e).is_none(), "{:?}", rejection(&e));
        assert!(e.iter().any(|e| matches!(e, SimEvent::FestivalHeld { guests: 2, .. })));
        let after = relation(&s);
        for (b, a) in before.iter().zip(&after) {
            assert_eq!(a - b, i32::from(d.rules().festival.relation), "every kingdom at peace: {before:?} → {after:?}");
        }
        assert!(s.diplomacy.kingdoms().all(|(_, k)| k.memories().iter().any(|m| m.cause == MemoryCause::Festival)));
        let breakdown = crate::report::kingdom_row(&s, d, ids[0]);
        assert!(format!("{breakdown:?}").contains("Festgäste"), "named in the relation breakdown");
        s.validate(d).unwrap();
    }

    #[test]
    fn the_court_of_a_higher_rank_costs_daily_upkeep() {
        let d = defs();
        let mut s = flat_state(48);
        spawn(&mut s, "town_hall", 4, 4);
        let base = crate::economy::finance::daily_rates(&s, d).upkeep;
        let city = d.ranks().iter().position(|r| r.key == "city").expect("rank city");
        s.progress.rank = u8::try_from(city).expect("rank index");
        let court = d.ranks()[city].court_upkeep;
        assert!(court > 0);
        assert_eq!(crate::economy::finance::daily_rates(&s, d).upkeep, base - i64::from(court) * crate::economy::MILLI_PER_GOLD);
        let r = crate::report::court_report(&s, d);
        assert_eq!(r.upkeep_per_day, court);
        assert_eq!(r.upkeep_text, format!("Hofhaltung als Großstadt: {court} Gold je Tag (im Unterhalt enthalten)"));
        assert!(r.next_upkeep_per_day.is_some_and(|n| n > court), "a higher rank keeps a larger court");
        // Booked hourly with the upkeep.
        let before = s.economy.treasury_milli();
        run(&mut s, TICKS_PER_DAY);
        let spent = before - s.economy.treasury_milli();
        assert!(spent >= i64::from(court) * crate::economy::MILLI_PER_GOLD, "{spent}");
    }
}
