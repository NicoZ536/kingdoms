//! Diplomatic actions of the player and how the kingdoms judge them (docs/game_design.md §10).
//!
//! Every action is first checked for what makes it impossible ([`action_problem`], German
//! reason). Proposals that need the kingdom's consent — treaties, demands, peace — are judged
//! deterministically ([`judge`]): a score from named parts (relation, personality, interests,
//! relative strength, the terms) against a threshold from `rules.ron` → `diplomacy.ai`. The
//! answer comes at once with its reasons. Gifts are always accepted; a war declaration needs no
//! consent but cancels the trade agreement (a pact or an alliance must be cancelled first).
//! Answers to the kingdoms' own proposals ([`super::Proposal`]) are applied by `answer` (`Command::AnswerProposal`).

use serde::{Deserialize, Serialize};

use super::relations::{self, RelationPart, remember};
use super::{
    DiplomaticAction, Kingdom, KingdomId, MemoryCause, Party, PeaceTerms, Proposal, ProposalKind, Treaty, TreatyKind, War, caravans,
    economy, kingdom_strength, player_strength, setup,
};
use crate::command::{CommandResult, RejectReason};
use crate::defs::{Defs, Personality, ResourceAmount};
use crate::economy::trade::TradeDirection;
use crate::economy::{LedgerCategory, MILLI_PER_GOLD, storage};
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::state::GameState;
use crate::time::TICKS_PER_DAY;

/// A kingdom's judgement of a proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Judgement {
    /// True if the kingdom agrees.
    pub accepted: bool,
    /// Sum of the reasons.
    pub score: i32,
    /// Score needed.
    pub threshold: i32,
    /// The named parts of the score.
    pub reasons: Vec<RelationPart>,
}

impl Judgement {
    fn new(threshold: i16, reasons: Vec<RelationPart>) -> Judgement {
        let score = reasons.iter().map(|r| r.points).sum();
        let threshold = i32::from(threshold);
        Judgement { accepted: score >= threshold, score, threshold, reasons: reasons.into_iter().filter(|r| r.points != 0).collect() }
    }

    /// The strongest reasons against (negative parts, strongest first), as German text.
    pub fn objections(&self) -> String {
        let mut against: Vec<&RelationPart> = self.reasons.iter().filter(|r| r.points < 0).collect();
        against.sort_by_key(|r| r.points);
        let list: Vec<String> = against.iter().take(3).map(|r| format!("{} ({})", r.label, r.points)).collect();
        if list.is_empty() { format!("zu wenig Anreiz ({} von {})", self.score, self.threshold) } else { list.join(", ") }
    }

    /// The strongest reasons for (positive parts, strongest first), as German text.
    pub fn arguments(&self) -> String {
        let mut pro: Vec<&RelationPart> = self.reasons.iter().filter(|r| r.points > 0).collect();
        pro.sort_by_key(|r| -r.points);
        let list: Vec<String> = pro.iter().take(3).map(|r| format!("{} (+{})", r.label, r.points)).collect();
        list.join(", ")
    }
}

fn part(label: impl Into<String>, points: i32) -> RelationPart {
    RelationPart { label: label.into(), points }
}

/// Days a war with kingdom `id` has lasted.
fn war_days(state: &GameState, id: KingdomId) -> u64 {
    state.diplomacy.kingdom(id).and_then(|k| k.war).map_or(0, |w| state.clock.tick.saturating_sub(w.since_tick) / TICKS_PER_DAY)
}

/// Why the player cannot take `action` towards kingdom `id` now (German), or `None`.
pub fn action_problem(state: &GameState, defs: &Defs, id: KingdomId, action: &DiplomaticAction) -> Option<String> {
    let Some(k) = state.diplomacy.kingdom(id) else { return Some("Dieses Königreich gibt es nicht.".to_owned()) };
    let tick = state.clock.tick;
    let name = &k.name;
    match action {
        DiplomaticAction::ProposeTreaty { treaty } => {
            if k.at_war() {
                return Some(format!("Ihr befindet Euch im Krieg mit {name}."));
            }
            if k.has_treaty(*treaty, tick) {
                return Some(format!("Mit {name} besteht bereits {}.", indefinite(*treaty)));
            }
            if *treaty == TreatyKind::Alliance && !k.has_treaty(TreatyKind::NonAggression, tick) {
                return Some("Ein Bündnis setzt einen Nichtangriffspakt voraus.".to_owned());
            }
            None
        }
        DiplomaticAction::CancelTreaty { treaty } => {
            (!k.has_treaty(*treaty, tick)).then(|| format!("Mit {name} besteht kein {}.", treaty.display_name()))
        }
        DiplomaticAction::Gift { gold, goods } => {
            if k.at_war() {
                return Some(format!("Im Krieg nimmt {name} keine Geschenke an."));
            }
            if *gold == 0 && goods.iter().all(|g| g.amount == 0) {
                return Some("Das Geschenk ist leer.".to_owned());
            }
            gold_problem(state, *gold).or_else(|| goods_problem(state, defs, goods))
        }
        DiplomaticAction::DemandTribute { gold } => {
            if k.at_war() {
                return Some(format!("Ihr befindet Euch im Krieg mit {name}."));
            }
            (*gold == 0).then(|| "Die Forderung muss mindestens 1 Gold betragen.".to_owned())
        }
        DiplomaticAction::DeclareWar => {
            if k.at_war() {
                return Some(format!("Ihr befindet Euch bereits im Krieg mit {name}."));
            }
            for t in [TreatyKind::NonAggression, TreatyKind::Alliance] {
                if k.has_treaty(t, tick) {
                    return Some(format!("Mit {name} besteht {} – kündigt {} zuerst.", indefinite(t), pronoun(t)));
                }
            }
            None
        }
        DiplomaticAction::ProposePeace { terms } => {
            if !k.at_war() {
                return Some(format!("Mit {name} herrscht kein Krieg."));
            }
            match terms {
                PeaceTerms::PlayerPays { gold } => gold_problem(state, *gold),
                PeaceTerms::KingdomPays { gold } if *gold == 0 => Some("Die Entschädigung muss mindestens 1 Gold betragen.".to_owned()),
                _ => None,
            }
        }
    }
}

/// „ein Handelsvertrag“ / „ein Bündnis“.
fn indefinite(t: TreatyKind) -> &'static str {
    match t {
        TreatyKind::TradeAgreement => "ein Handelsvertrag",
        TreatyKind::NonAggression => "ein Nichtangriffspakt",
        TreatyKind::Alliance => "ein Bündnis",
    }
}

/// „ihn“ / „es“.
fn pronoun(t: TreatyKind) -> &'static str {
    match t {
        TreatyKind::Alliance => "es",
        _ => "ihn",
    }
}

fn gold_problem(state: &GameState, gold: u32) -> Option<String> {
    let treasury = state.economy.treasury();
    (i64::from(gold) > treasury).then(|| format!("{}.", crate::report::missing_gold(i64::from(gold) - treasury.max(0))))
}

fn goods_problem(state: &GameState, defs: &Defs, goods: &[ResourceAmount]) -> Option<String> {
    goods.iter().find_map(|g| {
        let Some(r) = defs.resources().get(g.resource.index()) else { return Some("Diese Ware gibt es nicht.".to_owned()) };
        let available = state.economy.available(g.resource);
        (available < g.amount).then(|| format!("{}.", crate::report::missing_goods(g.amount - available, &r.name)))
    })
}

/// How kingdom `id` judges a proposal of the player (`None` for actions that need no consent:
/// cancelling, gifts, war).
pub fn judge(state: &GameState, defs: &Defs, id: KingdomId, action: &DiplomaticAction) -> Option<Judgement> {
    let k = state.diplomacy.kingdom(id)?;
    let rules = &defs.rules().diplomacy;
    let p = rules.personalities.get(k.personality);
    let relation = relations::relation(state, defs, id);
    let ratio = super::strength_ratio_percent(state, defs, k);
    let mut reasons = Vec::new();
    match action {
        DiplomaticAction::ProposeTreaty { treaty } => {
            reasons.push(part(format!("Beziehung „{}“", relations::relation_state(state, defs, id).display_name()), relation));
            let threshold = match treaty {
                TreatyKind::TradeAgreement => {
                    reasons.push(part(format!("Handelsinteresse ({})", k.personality.display_name()), i32::from(p.trade_interest) / 2));
                    if crate::economy::trade::active_trading_post(state, defs).is_none() {
                        reasons.push(part("Ihr habt kein besetztes Handelskontor", -25));
                    }
                    let useful = defs
                        .resources()
                        .iter()
                        .filter(|r| economy::wanted_units(defs, k, r.id) > 0 && state.economy.available(r.id) >= 10)
                        .count();
                    reasons.push(part("Ihr habt Waren, die sie brauchen", i32::try_from(useful * 5).unwrap_or(0).min(15)));
                    rules.ai.trade_threshold
                }
                TreatyKind::NonAggression => {
                    let peace = (50 - i32::from(p.aggression)) / 2;
                    let label = if peace >= 0 { "Friedfertigkeit" } else { "Angriffslust" };
                    reasons.push(part(format!("{label} ({})", k.personality.display_name()), peace));
                    if ratio < 80 {
                        reasons.push(part("Sie fürchten Eure Stärke", 15));
                    } else if ratio >= u32::from(rules.ai.superiority_percent) && p.aggression >= 30 {
                        reasons.push(part("Sie halten Euch für schwach", -20));
                    }
                    rules.ai.pact_threshold
                }
                TreatyKind::Alliance => {
                    reasons.push(part("Ein Bündnis bindet", -20));
                    if k.personality == Personality::Aggressive {
                        reasons.push(part("Unabhängigkeitsdrang (aggressiv)", -10));
                    }
                    for (other_id, other) in state.diplomacy.kingdoms() {
                        if other.at_war() && state.diplomacy.ai_war_between(id, other_id).is_some() {
                            reasons.push(part(format!("Gemeinsamer Feind: {}", other.name), 25));
                        }
                    }
                    rules.ai.alliance_threshold
                }
            };
            Some(Judgement::new(threshold, reasons))
        }
        DiplomaticAction::DemandTribute { gold } => {
            let mine = i64::from(player_strength(state, defs)) * 100;
            let theirs = i64::from(kingdom_strength(k, defs)).max(1);
            let power = i32::try_from(((mine / theirs) - 100) / 4).unwrap_or(0).clamp(-40, 60);
            reasons.push(part("Eure Stärke im Vergleich", power));
            reasons.push(part(format!("Stolz ({})", k.personality.display_name()), -i32::from(p.aggression) / 2));
            reasons.push(part("Beziehung", relation / 4));
            let treasury = k.treasury().max(1);
            let share = i32::try_from(i64::from(*gold) * 100 / treasury).unwrap_or(i32::MAX);
            reasons.push(part("Höhe der Forderung", -(share / 2).min(100)));
            if i64::from(*gold) > k.treasury() {
                reasons.push(part("So viel Gold haben sie nicht", -100));
            }
            Some(Judgement::new(rules.ai.tribute_threshold, reasons))
        }
        DiplomaticAction::ProposePeace { terms } => {
            let war = k.war?;
            reasons.push(part("Kriegsmüdigkeit", i32::from(war.exhaustion) / 2));
            reasons.push(part("Dauer des Krieges", i32::try_from(war_days(state, id) * 2).unwrap_or(0).min(30)));
            reasons.push(part("Krieg ist ihr Ziel", -20 - i32::from(p.aggression) / 5));
            if ratio > 100 {
                reasons.push(part("Sie fühlen sich überlegen", -i32::try_from((ratio - 100) / 5).unwrap_or(40).min(40)));
            } else {
                reasons.push(part("Ihr seid stärker", i32::try_from((100 - ratio) / 3).unwrap_or(0).min(30)));
            }
            match terms {
                PeaceTerms::White => {}
                PeaceTerms::PlayerPays { gold } => reasons.push(part("Angebotenes Gold", i32::try_from(*gold / 10).unwrap_or(40).min(40))),
                PeaceTerms::KingdomPays { gold } => {
                    reasons.push(part("Geforderte Entschädigung", -i32::try_from(*gold / 5).unwrap_or(60).min(60)));
                    if i64::from(*gold) > k.treasury() {
                        reasons.push(part("So viel Gold haben sie nicht", -100));
                    }
                }
                PeaceTerms::BorderCeded => reasons.push(part("Gebietsabtretung", -30)),
            }
            Some(Judgement::new(rules.ai.peace_threshold, reasons))
        }
        DiplomaticAction::CancelTreaty { .. } | DiplomaticAction::Gift { .. } | DiplomaticAction::DeclareWar => None,
    }
}

/// Relation points a gift of `value` gold is worth (before the cap of remembered gifts).
pub fn gift_points(defs: &Defs, value: i64) -> f32 {
    let per_100 = f64::from(defs.rules().diplomacy.relations.gift_points_per_100_gold);
    (value.max(0) as f64 / 100.0 * per_100) as f32
}

/// Where gifts and sold goods leave from and bought goods arrive: the trading post, else the
/// main headquarters, else the world's start.
fn goods_point(state: &GameState, defs: &Defs) -> glam::IVec2 {
    crate::economy::trade::active_trading_post(state, defs)
        .or_else(|| storage::main_headquarters(state, defs))
        .and_then(|b| state.buildings.get(b))
        .map_or(state.world.start, |b| b.entrance())
}

fn notify(state: &mut GameState, id: KingdomId, severity: Severity, text: String, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    let seat = state.diplomacy.kingdom(id).map(|k| k.seat);
    state.diplomacy.record(tick, Some(id), None, text.clone());
    let mut note = Notification::new(severity, text).with_kind(NotificationKind::Diplomacy);
    if let Some(p) = seat {
        note = note.at(p);
    }
    events.push(SimEvent::Notification(note));
}

/// `Command::Diplomacy`: applies `action` towards kingdom `id`.
pub(crate) fn apply(state: &mut GameState, defs: &Defs, id: KingdomId, action: &DiplomaticAction, events: &mut Vec<SimEvent>) -> CommandResult {
    if state.diplomacy.kingdom(id).is_none() {
        return Err(RejectReason::UnknownKingdom);
    }
    if let Some(reason) = action_problem(state, defs, id, action) {
        return Err(RejectReason::DiplomacyUnavailable { reason });
    }
    let name = state.diplomacy.kingdom(id).map(|k| k.name.clone()).unwrap_or_default();
    match action {
        DiplomaticAction::ProposeTreaty { treaty } => {
            let judgement = judge(state, defs, id, action).ok_or(RejectReason::UnknownKingdom)?;
            events.push(SimEvent::DiplomaticReply { kingdom: id, accepted: judgement.accepted });
            if judgement.accepted {
                sign_treaty(state, defs, id, *treaty, events);
            } else {
                let text = format!("{name} lehnt {} ab: {}.", treaty.accusative(), judgement.objections());
                notify(state, id, Severity::Info, text, events);
            }
        }
        DiplomaticAction::CancelTreaty { treaty } => cancel_treaty(state, defs, id, *treaty, events),
        DiplomaticAction::Gift { gold, goods } => {
            let at = goods_point(state, defs);
            let mut value = i64::from(*gold);
            let mut given = Vec::new();
            if *gold > 0 {
                state.economy.book_gold(LedgerCategory::Diplomacy, -i64::from(*gold));
            }
            for g in goods.iter().filter(|g| g.amount > 0) {
                let taken = storage::withdraw(state, defs, g.resource, g.amount, at);
                if taken > 0 {
                    given.push(ResourceAmount { resource: g.resource, amount: taken });
                }
            }
            let rules = &defs.rules().diplomacy.relations;
            if let Some(k) = state.diplomacy.kingdom_mut(id) {
                k.treasury_milli += i64::from(*gold) * MILLI_PER_GOLD;
                for g in &given {
                    value += i64::from(g.amount) * i64::from(economy::price_centi(defs, k, g.resource)) / 100;
                    economy::give(k, g.resource, g.amount);
                }
                remember(k, defs, MemoryCause::Gift, gift_points(defs, value), Some(rules.gift_max));
            }
            events.push(SimEvent::GiftSent { kingdom: id, gold: *gold, goods: given });
            let text = format!("{name} nimmt Euer Geschenk im Wert von etwa {value} Gold dankend an.");
            notify(state, id, Severity::Good, text, events);
        }
        DiplomaticAction::DemandTribute { gold } => {
            let judgement = judge(state, defs, id, action).ok_or(RejectReason::UnknownKingdom)?;
            events.push(SimEvent::DiplomaticReply { kingdom: id, accepted: judgement.accepted });
            let rules = &defs.rules().diplomacy.relations;
            if let Some(k) = state.diplomacy.kingdom_mut(id) {
                remember(k, defs, MemoryCause::Demanded, f32::from(rules.demanded), None);
            }
            if judgement.accepted {
                if let Some(k) = state.diplomacy.kingdom_mut(id) {
                    k.treasury_milli -= i64::from(*gold) * MILLI_PER_GOLD;
                }
                state.economy.book_gold(LedgerCategory::Diplomacy, i64::from(*gold));
                let text = format!("{name} beugt sich und zahlt {gold} Gold Tribut ({}).", judgement.arguments());
                notify(state, id, Severity::Good, text, events);
            } else {
                let text = format!("{name} weist Eure Forderung zurück: {}.", judgement.objections());
                notify(state, id, Severity::Warning, text, events);
            }
        }
        DiplomaticAction::DeclareWar => start_war(state, defs, id, true, events),
        DiplomaticAction::ProposePeace { terms } => {
            let judgement = judge(state, defs, id, action).ok_or(RejectReason::UnknownKingdom)?;
            events.push(SimEvent::DiplomaticReply { kingdom: id, accepted: judgement.accepted });
            if judgement.accepted {
                make_peace(state, defs, id, *terms, events);
            } else {
                let text = format!("{name} lehnt Euer Friedensangebot ab: {}.", judgement.objections());
                notify(state, id, Severity::Warning, text, events);
            }
        }
    }
    Ok(())
}

/// Signs a treaty with kingdom `id` (replacing one of the same kind).
pub(crate) fn sign_treaty(state: &mut GameState, defs: &Defs, id: KingdomId, treaty: TreatyKind, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    let days = treaty.days(defs);
    let Some(k) = state.diplomacy.kingdom_mut(id) else { return };
    k.treaties.retain(|t| t.kind != treaty);
    k.treaties.push(Treaty { kind: treaty, since_tick: tick, until_tick: tick + u64::from(days) * TICKS_PER_DAY });
    let name = k.name.clone();
    events.push(SimEvent::TreatySigned { kingdom: id, treaty });
    let text = format!("{} mit {name} geschlossen ({days} Tage).", treaty.display_name());
    notify(state, id, Severity::Good, text, events);
    relations::announce_states(state, defs, events);
}

/// Cancels a treaty in force: the kingdom remembers the broken treaty, all others the broken
/// word.
pub(crate) fn cancel_treaty(state: &mut GameState, defs: &Defs, id: KingdomId, treaty: TreatyKind, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy;
    let penalty = match treaty {
        TreatyKind::TradeAgreement => rules.treaties.break_trade,
        TreatyKind::NonAggression => rules.treaties.break_pact,
        TreatyKind::Alliance => rules.treaties.break_alliance,
    };
    let Some(k) = state.diplomacy.kingdom_mut(id) else { return };
    k.treaties.retain(|t| t.kind != treaty);
    remember(k, defs, MemoryCause::TreatyBroken, f32::from(penalty), None);
    let name = k.name.clone();
    for (i, other) in state.diplomacy.kingdoms.iter_mut().enumerate() {
        if i != id.index() {
            remember(other, defs, MemoryCause::BrokenWord, f32::from(rules.relations.broken_word), None);
        }
    }
    events.push(SimEvent::TreatyEnded { kingdom: id, treaty, broken: true });
    let text = format!("Ihr habt {} mit {name} gekündigt. Die anderen Reiche merken sich den Wortbruch.", treaty.accusative());
    notify(state, id, Severity::Warning, text, events);
    relations::announce_states(state, defs, events);
}

/// Starts a war between the player and kingdom `id`: treaties end, the kingdom's caravans turn
/// back, its proposals are void, its friends resent an attack by the player and the player's
/// allies stand by the player when attacked.
pub(crate) fn start_war(state: &mut GameState, defs: &Defs, id: KingdomId, by_player: bool, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy;
    let tick = state.clock.tick;
    let today = state.clock.total_days();
    let Some(k) = state.diplomacy.kingdom_mut(id) else { return };
    k.treaties.clear();
    k.war = Some(War {
        since_tick: tick,
        declared_by_player: by_player,
        exhaustion: 0,
        raids: 0,
        next_raid_day: today + u64::from(rules.war.raid_interval_days),
    });
    if by_player {
        remember(k, defs, MemoryCause::WarDeclared, f32::from(rules.relations.war_declared), None);
    }
    let name = k.name.clone();
    let seat = k.seat;
    state.diplomacy.proposals.retain(|p| p.kingdom != id);
    if by_player {
        for i in 0..state.diplomacy.kingdoms.len() {
            let other = KingdomId::from_raw(i as u8);
            if other != id && state.diplomacy.ai_relation(other, id) >= 30 {
                remember(&mut state.diplomacy.kingdoms[i], defs, MemoryCause::WarOnFriend, f32::from(rules.relations.war_on_friend), None);
            }
        }
    } else {
        let allies: Vec<KingdomId> = state
            .diplomacy
            .kingdoms()
            .filter(|(other, o)| *other != id && o.has_treaty(TreatyKind::Alliance, tick))
            .map(|(other, _)| other)
            .collect();
        for ally in allies {
            relations::start_ai_war(state, ally, id, events);
        }
    }
    caravans::turn_back_all(state, defs, id, events);
    let attacker = if by_player { Party::Player } else { Party::Kingdom(id) };
    let defender = if by_player { Party::Kingdom(id) } else { Party::Player };
    events.push(SimEvent::WarDeclared { attacker, defender });
    let text = if by_player {
        format!("Ihr habt {name} den Krieg erklärt. Der Handel mit {name} ruht.")
    } else {
        format!("{name} erklärt Euch den Krieg! Plünderer bedrohen unbewachte Lager, der Handel mit {name} ruht.")
    };
    state.diplomacy.record(tick, Some(id), None, text.clone());
    events.push(SimEvent::Notification(Notification::new(Severity::Danger, text).with_kind(NotificationKind::Diplomacy).at(seat)));
    relations::announce_states(state, defs, events);
}

/// Ends the war with kingdom `id` on `terms` (payments only as far as the treasuries allow).
pub(crate) fn make_peace(state: &mut GameState, defs: &Defs, id: KingdomId, terms: PeaceTerms, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy;
    match terms {
        PeaceTerms::White => {}
        PeaceTerms::PlayerPays { gold } => {
            let paid = i64::from(gold).min(state.economy.treasury().max(0));
            state.economy.book_gold(LedgerCategory::Diplomacy, -paid);
            if let Some(k) = state.diplomacy.kingdom_mut(id) {
                k.treasury_milli += paid * MILLI_PER_GOLD;
            }
        }
        PeaceTerms::KingdomPays { gold } => {
            let paid = state.diplomacy.kingdom(id).map_or(0, |k| i64::from(gold).min(k.treasury().max(0)));
            if let Some(k) = state.diplomacy.kingdom_mut(id) {
                k.treasury_milli -= paid * MILLI_PER_GOLD;
            }
            state.economy.book_gold(LedgerCategory::Diplomacy, paid);
        }
        PeaceTerms::BorderCeded => {
            if let Some(k) = state.diplomacy.kingdom_mut(id) {
                k.ceded = k.ceded.saturating_add(u16::from(rules.war.border_ceded));
                k.radius = setup::target_radius(defs, k.population, k.ceded).min(k.radius);
            }
            if setup::claim_territory(state, defs, id) {
                setup::survey_land(state, id);
                let tiles = state.diplomacy.kingdom(id).map_or(0, |k| k.territory_tiles);
                events.push(SimEvent::KingdomBorderChanged { kingdom: id, tiles });
            }
        }
    }
    state.diplomacy.proposals.retain(|p| !(p.kingdom == id && matches!(p.kind, ProposalKind::Peace { .. })));
    let Some(k) = state.diplomacy.kingdom_mut(id) else { return };
    k.war = None;
    remember(k, defs, MemoryCause::PeaceMade, f32::from(rules.relations.peace_made), None);
    let name = k.name.clone();
    events.push(SimEvent::PeaceMade { kingdom: id, terms });
    let text = format!("Frieden mit {name}: {}.", terms.text(&name));
    notify(state, id, Severity::Good, text, events);
    relations::announce_states(state, defs, events);
}

/// Why the player cannot accept `proposal` now (German), or `None`.
pub fn proposal_problem(state: &GameState, defs: &Defs, proposal: &Proposal) -> Option<String> {
    let k = state.diplomacy.kingdom(proposal.kingdom)?;
    match proposal.kind {
        ProposalKind::Treaty(t) => {
            if k.at_war() {
                Some(format!("Ihr befindet Euch im Krieg mit {}.", k.name))
            } else if t == TreatyKind::Alliance && !k.has_treaty(TreatyKind::NonAggression, state.clock.tick) {
                Some("Ein Bündnis setzt einen Nichtangriffspakt voraus.".to_owned())
            } else {
                None
            }
        }
        ProposalKind::Tribute { gold } | ProposalKind::Sale { gold, .. } => gold_problem(state, gold),
        ProposalKind::Peace { .. } if !k.at_war() => Some(format!("Mit {} herrscht kein Krieg mehr.", k.name)),
        ProposalKind::Peace { terms } => match terms {
            PeaceTerms::PlayerPays { gold } => gold_problem(state, gold),
            _ => None,
        },
        ProposalKind::Purchase { amount, gold, .. } if affordable_units(k, amount, gold) == 0 => {
            Some(format!("{} kann den Kauf nicht mehr bezahlen.", k.name))
        }
        ProposalKind::Purchase { resource, amount, .. } => goods_problem(state, defs, &[ResourceAmount { resource, amount }]),
    }
}

/// Units of a purchase offer (`amount` units for `gold` gold) that kingdom `k` can still pay for
/// from its treasury now (the offer was checked against the treasury only when it was made).
fn affordable_units(k: &Kingdom, amount: u32, gold: u32) -> u32 {
    if gold == 0 {
        return amount;
    }
    let budget_milli = k.treasury_milli.max(0);
    let units = i128::from(amount) * i128::from(budget_milli) / (i128::from(gold) * i128::from(MILLI_PER_GOLD));
    u32::try_from(units.min(i128::from(amount))).unwrap_or(amount)
}

/// `Command::AnswerProposal`: accepts or declines proposal `serial`.
pub(crate) fn answer(state: &mut GameState, defs: &Defs, serial: u32, accept: bool, events: &mut Vec<SimEvent>) -> CommandResult {
    let proposal = state.diplomacy.proposals.iter().find(|p| p.serial == serial).copied().ok_or(RejectReason::NoProposal)?;
    if accept && let Some(reason) = proposal_problem(state, defs, &proposal) {
        return Err(RejectReason::DiplomacyUnavailable { reason });
    }
    state.diplomacy.proposals.retain(|p| p.serial != serial);
    resolve(state, defs, proposal, accept, false, events);
    Ok(())
}

/// Applies the answer to `proposal` (`expired`: the time ran out, which counts as declining).
pub(crate) fn resolve(state: &mut GameState, defs: &Defs, proposal: Proposal, accept: bool, expired: bool, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy;
    let id = proposal.kingdom;
    let Some(name) = state.diplomacy.kingdom(id).map(|k| k.name.clone()) else { return };
    events.push(SimEvent::ProposalAnswered { serial: proposal.serial, kingdom: id, accepted: accept, expired });
    let declined_note = if expired { " (keine Antwort)" } else { "" };
    if !accept {
        let points = match proposal.kind {
            ProposalKind::Tribute { .. } => rules.relations.demand_refused,
            ProposalKind::Peace { .. } => 0,
            _ => rules.relations.proposal_declined,
        };
        let cause = if matches!(proposal.kind, ProposalKind::Tribute { .. }) { MemoryCause::DemandRefused } else { MemoryCause::ProposalDeclined };
        if let Some(k) = state.diplomacy.kingdom_mut(id) {
            remember(k, defs, cause, f32::from(points), None);
        }
        let text = match proposal.kind {
            ProposalKind::Treaty(t) => format!("Ihr habt {} mit {name} abgelehnt{declined_note}.", t.accusative()),
            ProposalKind::Tribute { gold } => format!("Ihr verweigert {name} den Tribut von {gold} Gold{declined_note}."),
            ProposalKind::Peace { .. } => format!("Ihr habt das Friedensangebot von {name} abgelehnt{declined_note}. Der Krieg geht weiter."),
            ProposalKind::Purchase { .. } | ProposalKind::Sale { .. } => format!("Ihr habt das Handelsangebot von {name} abgelehnt{declined_note}."),
        };
        notify(state, id, Severity::Info, text, events);
        if matches!(proposal.kind, ProposalKind::Tribute { .. }) {
            super::initiatives::after_refused_tribute(state, defs, id, events);
        }
        relations::announce_states(state, defs, events);
        return;
    }
    match proposal.kind {
        ProposalKind::Treaty(t) => sign_treaty(state, defs, id, t, events),
        ProposalKind::Tribute { gold } => {
            state.economy.book_gold(LedgerCategory::Diplomacy, -i64::from(gold));
            if let Some(k) = state.diplomacy.kingdom_mut(id) {
                k.treasury_milli += i64::from(gold) * MILLI_PER_GOLD;
                remember(k, defs, MemoryCause::TributePaid, f32::from(rules.relations.tribute_paid), None);
            }
            notify(state, id, Severity::Info, format!("Ihr zahlt {name} {gold} Gold Tribut."), events);
        }
        ProposalKind::Peace { terms } => make_peace(state, defs, id, terms, events),
        ProposalKind::Purchase { resource, amount, gold } => {
            // The kingdom may have spent gold since the offer: it buys only what it can pay for.
            let payable = state.diplomacy.kingdom(id).map_or(0, |k| affordable_units(k, amount, gold));
            let at = goods_point(state, defs);
            let sold = storage::withdraw(state, defs, resource, payable, at);
            let paid = i64::from(gold) * i64::from(sold) / i64::from(amount.max(1));
            if let Some(k) = state.diplomacy.kingdom_mut(id) {
                economy::give(k, resource, sold);
                k.treasury_milli -= paid * MILLI_PER_GOLD;
            }
            caravans::book_trade(state, resource, sold, paid * MILLI_PER_GOLD, TradeDirection::Export, events);
            let what = defs.resource(resource).name.clone();
            let mut text = format!("Ihr verkauft {name} {sold} {what} für {paid} Gold.");
            if payable < amount {
                text.push_str(&format!(" Mehr kann {name} nicht mehr bezahlen."));
            }
            notify(state, id, Severity::Good, text, events);
        }
        ProposalKind::Sale { resource, amount, gold } => {
            let at = goods_point(state, defs);
            let available = state.diplomacy.kingdom(id).map_or(0, |k| k.stock(resource)).min(amount);
            let stored = storage::deposit(state, defs, resource, available, at);
            let paid = i64::from(gold) * i64::from(stored) / i64::from(amount.max(1));
            if let Some(k) = state.diplomacy.kingdom_mut(id) {
                economy::take(k, resource, stored);
                k.treasury_milli += paid * MILLI_PER_GOLD;
            }
            caravans::book_trade(state, resource, stored, -paid * MILLI_PER_GOLD, TradeDirection::Import, events);
            let what = defs.resource(resource).name.clone();
            let mut text = format!("Ihr kauft von {name} {stored} {what} für {paid} Gold.");
            if stored < amount {
                text.push_str(" Für den Rest fehlte Lagerplatz.");
            }
            notify(state, id, Severity::Good, text, events);
        }
    }
}
