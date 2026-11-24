//! Relations of the kingdoms to the player and to each other (docs/game_design.md §10).
//!
//! The relation to the player is the sum of named parts ([`relation_parts`]): the attitude of
//! the personality, treaties in force, war, common enemies, a shared border and remembered
//! actions ([`super::Memory`], one per cause) that fade every day by
//! `relations.memory_decay_percent` (at least `memory_decay_min` points). Relations between AI
//! kingdoms drift towards the mean of their attitudes (less with a shared border); hostile
//! kingdoms go to war when the aggressor is stronger, and wars between them end after a few
//! days with the loser ceding border land and paying reparations.

use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::{AiWar, Kingdom, KingdomId, Memory, MemoryCause, RelationState, TreatyKind, kingdom_strength, setup};
use crate::defs::Defs;
use crate::event::{Notification, Severity, SimEvent};
use crate::state::GameState;
use crate::time::TICKS_PER_DAY;

/// One part of a relation with a German label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationPart {
    /// German label, e.g. „Wesen: friedlich“ or „Geschenke“.
    pub label: String,
    /// Points.
    pub points: i32,
}

/// What a part of a relation stands for (labels are made only for reports).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PartKind {
    Personality(crate::defs::Personality),
    Treaty(TreatyKind),
    War,
    CommonEnemy(KingdomId),
    Border,
    Prestige,
    Memory(MemoryCause),
}

/// Calls `f` with every non-zero part of kingdom `id`'s relation to the player.
fn visit_parts(state: &GameState, defs: &Defs, id: KingdomId, f: &mut dyn FnMut(PartKind, i32)) {
    let Some(k) = state.diplomacy.kingdom(id) else { return };
    let rules = &defs.rules().diplomacy;
    let tick = state.clock.tick;
    let mut push = |kind: PartKind, points: i32| {
        if points != 0 {
            f(kind, points);
        }
    };
    push(PartKind::Personality(k.personality), i32::from(rules.personalities.get(k.personality).base_relation));
    for t in TreatyKind::ALL {
        if k.has_treaty(t, tick) {
            let points = match t {
                TreatyKind::TradeAgreement => rules.treaties.trade_relation,
                TreatyKind::NonAggression => rules.treaties.pact_relation,
                TreatyKind::Alliance => rules.treaties.alliance_relation,
            };
            push(PartKind::Treaty(t), i32::from(points));
        }
    }
    if k.at_war() {
        push(PartKind::War, i32::from(rules.relations.war));
    }
    for (other_id, other) in state.diplomacy.kingdoms() {
        if other.at_war() && other_id != id && state.diplomacy.ai_war_between(id, other_id).is_some() {
            push(PartKind::CommonEnemy(other_id), i32::from(rules.relations.common_enemy));
        }
    }
    if k.borders_player {
        let hard = matches!(k.personality, crate::defs::Personality::Expansive | crate::defs::Personality::Aggressive);
        let points = if hard { rules.relations.border_tension_hard } else { rules.relations.border_tension_soft };
        push(PartKind::Border, i32::from(points));
    }
    push(PartKind::Prestige, crate::ranks::prestige(state, defs));
    for m in &k.memories {
        push(PartKind::Memory(m.cause), m.centi / 100);
    }
}

/// The parts of kingdom `id`'s relation to the player (non-zero parts only) with German labels.
pub fn relation_parts(state: &GameState, defs: &Defs, id: KingdomId) -> Vec<RelationPart> {
    let mut parts = Vec::new();
    visit_parts(state, defs, id, &mut |kind, points| {
        let label = match kind {
            PartKind::Personality(p) => format!("Wesen: {}", p.display_name()),
            PartKind::Treaty(t) => t.display_name().to_owned(),
            PartKind::War => "Krieg".to_owned(),
            PartKind::CommonEnemy(e) => {
                format!("Gemeinsamer Feind: {}", state.diplomacy.kingdom(e).map_or("?", |k| k.name.as_str()))
            }
            PartKind::Border => "Gemeinsame Grenze".to_owned(),
            PartKind::Prestige => {
                format!("Ansehen: {}", crate::ranks::current_rank(state, defs).map_or("?", |r| r.name.as_str()))
            }
            PartKind::Memory(cause) => cause.label().to_owned(),
        };
        parts.push(RelationPart { label, points });
    });
    parts
}

/// Relation of kingdom `id` to the player (−100 … +100).
pub fn relation(state: &GameState, defs: &Defs, id: KingdomId) -> i32 {
    let mut sum = 0;
    visit_parts(state, defs, id, &mut |_, points| sum += points);
    sum.clamp(-100, 100)
}

/// Diplomatic state of kingdom `id` towards the player.
pub fn relation_state(state: &GameState, defs: &Defs, id: KingdomId) -> RelationState {
    let allied = state.diplomacy.kingdom(id).is_some_and(|k| k.has_treaty(TreatyKind::Alliance, state.clock.tick));
    RelationState::of(relation(state, defs, id), allied)
}

/// Adds `points` to the memory of `cause` (merged with what is remembered of it, limited to
/// ±`cap` points if given); the weakest memory is forgotten beyond `relations.max_memories`.
pub(crate) fn remember(k: &mut Kingdom, defs: &Defs, cause: MemoryCause, points: f32, cap: Option<i8>) {
    let centi = (f64::from(points) * 100.0).round() as i32;
    if centi == 0 {
        return;
    }
    let limit = cap.map(|c| i32::from(c).abs() * 100);
    match k.memories.iter_mut().find(|m| m.cause == cause) {
        Some(m) => m.centi = m.centi.saturating_add(centi),
        None => k.memories.push(Memory { cause, centi }),
    }
    if let (Some(limit), Some(m)) = (limit, k.memories.iter_mut().find(|m| m.cause == cause)) {
        m.centi = m.centi.clamp(-limit, limit);
    }
    k.memories.retain(|m| m.centi != 0);
    let max = usize::from(defs.rules().diplomacy.relations.max_memories);
    while k.memories.len() > max {
        let weakest = k
            .memories
            .iter()
            .enumerate()
            .filter(|(_, m)| m.cause != cause)
            .min_by_key(|(_, m)| m.centi.abs())
            .map(|(i, _)| i);
        match weakest {
            Some(i) => {
                k.memories.remove(i);
            }
            None => break,
        }
    }
}

/// Sets kingdom `id`'s relation to the player to `value` through the developer memory.
pub(crate) fn set_relation(state: &mut GameState, defs: &Defs, id: KingdomId, value: i32) {
    if let Some(k) = state.diplomacy.kingdom_mut(id) {
        k.memories.retain(|m| m.cause != MemoryCause::Debug);
    }
    let mut rest = 0;
    visit_parts(state, defs, id, &mut |_, points| rest += points);
    if let Some(k) = state.diplomacy.kingdom_mut(id) {
        let centi = (value.clamp(-100, 100) - rest) * 100;
        if centi != 0 {
            k.memories.push(Memory { cause: MemoryCause::Debug, centi });
        }
    }
}

/// Lets every memory fade by one day.
fn decay(k: &mut Kingdom, defs: &Defs) {
    let rules = &defs.rules().diplomacy.relations;
    let min = (f64::from(rules.memory_decay_min) * 100.0).round().max(1.0) as i32;
    for m in &mut k.memories {
        let step = (m.centi.abs() * i32::from(rules.memory_decay_percent) / 100).max(min);
        m.centi = if m.centi > 0 { (m.centi - step).max(0) } else { (m.centi + step).min(0) };
    }
    k.memories.retain(|m| m.centi != 0);
}

/// Daily: memories fade, changes of the diplomatic state are announced, AI relations drift and
/// AI wars start and end.
pub(crate) fn run_day(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    for k in &mut state.diplomacy.kingdoms {
        decay(k, defs);
    }
    announce_states(state, defs, events);
    drift_ai_relations(state, defs, events);
    run_ai_wars(state, defs, events);
}

/// Removes treaties whose time ran out (checked every tick) and announces them.
pub(crate) fn expire_treaties(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    if !state.diplomacy.kingdoms.iter().any(|k| k.treaties.iter().any(|t| t.until_tick <= tick)) {
        return;
    }
    for i in 0..state.diplomacy.kingdoms.len() {
        let id = KingdomId::from_raw(i as u8);
        let k = &mut state.diplomacy.kingdoms[i];
        let expired: Vec<TreatyKind> = k.treaties.iter().filter(|t| t.until_tick <= tick).map(|t| t.kind).collect();
        k.treaties.retain(|t| t.until_tick > tick);
        let name = k.name.clone();
        let seat = k.seat;
        for kind in expired {
            let text = format!("{} mit {name} ist ausgelaufen.", kind.display_name());
            state.diplomacy.record(tick, Some(id), None, text.clone());
            events.push(SimEvent::TreatyEnded { kingdom: id, treaty: kind, broken: false });
            events.push(SimEvent::Notification(Notification::new(Severity::Info, text).at(seat)));
        }
    }
    announce_states(state, defs, events);
}

/// Announces kingdoms whose diplomatic state towards the player changed.
pub(crate) fn announce_states(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    for i in 0..state.diplomacy.kingdoms.len() {
        let id = KingdomId::from_raw(i as u8);
        let now = relation_state(state, defs, id);
        let k = &mut state.diplomacy.kingdoms[i];
        if k.last_state == now {
            continue;
        }
        let worse = now < k.last_state;
        k.last_state = now;
        let text = format!("Die Beziehung zu {} ist nun „{}“.", k.name, now.display_name());
        let seat = k.seat;
        state.diplomacy.record(tick, Some(id), None, text.clone());
        events.push(SimEvent::RelationStateChanged { kingdom: id, state: now });
        let severity = if worse { Severity::Warning } else { Severity::Good };
        events.push(SimEvent::Notification(Notification::new(severity, text).at(seat)));
    }
}

/// Relations between AI kingdoms move one point a day towards their target (with a little
/// chance either way); at war they keep falling.
fn drift_ai_relations(state: &mut GameState, defs: &Defs, _events: &mut Vec<SimEvent>) {
    let n = state.diplomacy.kingdoms.len();
    for a in 0..n {
        for b in (a + 1)..n {
            let (ka, kb) = (KingdomId::from_raw(a as u8), KingdomId::from_raw(b as u8));
            let current = state.diplomacy.ai_relation(ka, kb);
            let target = setup::ai_base_relation(defs, &state.diplomacy, ka, kb);
            let jitter = state.diplomacy.rng.random_range(-2..=2);
            let step = if state.diplomacy.ai_war_between(ka, kb).is_some() { -1 } else { (target - current).signum() };
            state.diplomacy.set_ai_relation(ka, kb, current + step + jitter);
        }
    }
}

/// Starts wars between hostile kingdoms (the more aggressive one attacks if it is stronger) and
/// resolves running ones: daily losses, and after `war.ai_war_min_days` a daily chance of
/// peace in which the weaker side cedes border land and pays reparations.
fn run_ai_wars(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy;
    let tick = state.clock.tick;
    let today = state.clock.total_days();
    let n = state.diplomacy.kingdoms.len();
    // Losses of running wars.
    let wars = state.diplomacy.ai_wars.clone();
    for w in &wars {
        let (sa, sd) = (state.diplomacy.kingdoms[w.attacker.index()].soldiers, state.diplomacy.kingdoms[w.defender.index()].soldiers);
        let loss = |enemy: u32| enemy * u32::from(rules.war.ai_losses_permille) / 1000 + 1;
        for (side, enemy) in [(w.attacker, sd), (w.defender, sa)] {
            let k = &mut state.diplomacy.kingdoms[side.index()];
            k.soldiers = k.soldiers.saturating_sub(loss(enemy));
            k.population = k.population.saturating_sub((k.population / 200).max(1)).max(20);
        }
    }
    // Peace.
    let mut ended = Vec::new();
    for (i, w) in wars.iter().enumerate() {
        let days = tick.saturating_sub(w.since_tick) / TICKS_PER_DAY;
        if days < u64::from(rules.war.ai_war_min_days) {
            continue;
        }
        if state.diplomacy.rng.random_range(0..100u8) >= rules.war.ai_peace_chance_percent {
            continue;
        }
        ended.push(i);
        let (ka, kd) = (&state.diplomacy.kingdoms[w.attacker.index()], &state.diplomacy.kingdoms[w.defender.index()]);
        let (winner, loser) =
            if kingdom_strength(ka, defs) >= kingdom_strength(kd, defs) { (w.attacker, w.defender) } else { (w.defender, w.attacker) };
        let reparations = {
            let l = &mut state.diplomacy.kingdoms[loser.index()];
            l.ceded = l.ceded.saturating_add(u16::from(rules.war.border_ceded));
            let pay = l.treasury_milli.max(0) * i64::from(rules.war.reparations_percent) / 100;
            l.treasury_milli -= pay;
            pay
        };
        state.diplomacy.kingdoms[winner.index()].treasury_milli += reparations;
        state.diplomacy.set_ai_relation(winner, loser, -30);
        let (wn, ln) = (state.diplomacy.kingdoms[winner.index()].name.clone(), state.diplomacy.kingdoms[loser.index()].name.clone());
        let seat = state.diplomacy.kingdoms[loser.index()].seat;
        let text = format!("Frieden zwischen {wn} und {ln}: {ln} tritt Land ab und zahlt Entschädigung.");
        state.diplomacy.record(tick, Some(winner), Some(loser), text.clone());
        events.push(SimEvent::AiPeace { winner, loser });
        events.push(SimEvent::Notification(Notification::new(Severity::Info, text).at(seat)));
    }
    for i in ended.into_iter().rev() {
        state.diplomacy.ai_wars.remove(i);
    }
    // New wars.
    if today < u64::from(rules.ai.ai_war_first_day) {
        return;
    }
    for a in 0..n {
        for b in (a + 1)..n {
            let (ka, kb) = (KingdomId::from_raw(a as u8), KingdomId::from_raw(b as u8));
            if state.diplomacy.ai_relation(ka, kb) >= -50 || state.diplomacy.ai_war_between(ka, kb).is_some() {
                continue;
            }
            let aggr = |id: KingdomId| u32::from(rules.personalities.get(state.diplomacy.kingdoms[id.index()].personality).aggression);
            let (attacker, defender) = if aggr(ka) >= aggr(kb) { (ka, kb) } else { (kb, ka) };
            let aggression = aggr(attacker);
            let stronger = kingdom_strength(&state.diplomacy.kingdoms[attacker.index()], defs) * 100
                >= kingdom_strength(&state.diplomacy.kingdoms[defender.index()], defs) * u32::from(rules.ai.superiority_percent);
            if aggression < 30 || !stronger || state.diplomacy.rng.random_range(0..100u32) >= aggression / 5 {
                continue;
            }
            start_ai_war(state, attacker, defender, events);
        }
    }
}

/// Starts a war between two AI kingdoms and announces it.
pub(crate) fn start_ai_war(state: &mut GameState, attacker: KingdomId, defender: KingdomId, events: &mut Vec<SimEvent>) {
    if attacker == defender || state.diplomacy.ai_war_between(attacker, defender).is_some() {
        return;
    }
    let tick = state.clock.tick;
    state.diplomacy.ai_wars.push(AiWar { attacker, defender, since_tick: tick });
    let current = state.diplomacy.ai_relation(attacker, defender);
    state.diplomacy.set_ai_relation(attacker, defender, current.min(-60));
    let (an, dn) = (state.diplomacy.kingdoms[attacker.index()].name.clone(), state.diplomacy.kingdoms[defender.index()].name.clone());
    let seat = state.diplomacy.kingdoms[defender.index()].seat;
    let text = format!("{an} hat {dn} den Krieg erklärt.");
    state.diplomacy.record(tick, Some(attacker), Some(defender), text.clone());
    events.push(SimEvent::AiWarStarted { attacker, defender });
    events.push(SimEvent::Notification(Notification::new(Severity::Info, text).at(seat)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::defs;

    fn kingdom() -> Kingdom {
        crate::diplomacy::tests::blank_kingdom(defs())
    }

    #[test]
    fn memories_merge_cap_and_fade() {
        let d = defs();
        let mut k = kingdom();
        remember(&mut k, d, MemoryCause::Gift, 20.0, Some(30));
        remember(&mut k, d, MemoryCause::Gift, 20.0, Some(30));
        assert_eq!(k.memories, vec![Memory { cause: MemoryCause::Gift, centi: 3000 }], "merged and capped at 30 points");
        remember(&mut k, d, MemoryCause::DemandRefused, -15.0, None);
        decay(&mut k, d);
        // 3 % of 30 points = 0.9; 3 % of 15 = 0.45 (at least 0.2).
        assert_eq!(k.memories[0].centi, 3000 - 90);
        assert_eq!(k.memories[1].centi, -1500 + 45);
        for _ in 0..400 {
            decay(&mut k, d);
        }
        assert!(k.memories.is_empty(), "everything fades");
        for (i, cause) in [MemoryCause::Gift, MemoryCause::Trade, MemoryCause::TributePaid, MemoryCause::DemandRefused].into_iter().enumerate() {
            remember(&mut k, d, cause, 1.0 + i as f32, None);
        }
        assert_eq!(k.memories.len(), 4);
    }

    #[test]
    fn states_follow_the_bands() {
        assert_eq!(RelationState::of(-51, false), RelationState::Hostile);
        assert_eq!(RelationState::of(-50, false), RelationState::Rival);
        assert_eq!(RelationState::of(-11, false), RelationState::Rival);
        assert_eq!(RelationState::of(-10, false), RelationState::Neutral);
        assert_eq!(RelationState::of(29, false), RelationState::Neutral);
        assert_eq!(RelationState::of(30, false), RelationState::Friendly);
        assert_eq!(RelationState::of(100, false), RelationState::Friendly, "allied only by treaty");
        assert_eq!(RelationState::of(-80, true), RelationState::Allied);
        assert_eq!(RelationState::Allied.display_name(), "Verbündet");
    }
}
