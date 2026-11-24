//! Initiatives of the AI kingdoms and the player's wars (docs/game_design.md §10, §11).
//!
//! At `ai.initiative_hour` every kingdom whose initiative is due and that has no proposal
//! waiting rolls `ai.initiative_chance_percent` and then picks one eligible initiative by
//! weight (personality, relation, interests, relative strength). Initiatives that wait in the
//! player's inbox (proposals, demands, offers) are paced ([`inbox_open`]): none before
//! `ai.first_proposal_day`, at most `ai.max_open_proposals` waiting and at least
//! `ai.proposal_gap_hours` (in the first `ai.early_days`: `ai.early_proposal_gap_hours`)
//! after the last one; a peace offer of a kingdom at war always comes.
//! * propose a trade agreement, a non-aggression pact or an alliance,
//! * give a gift (gold or surplus goods) — friendly, or to appease a stronger player,
//! * demand tribute (aggressive, stronger, from `ai.demand_first_day`),
//! * offer to buy a good it needs or to sell a surplus,
//! * a border incident while territories touch (expansive and aggressive kingdoms),
//! * declare war (aggressive, hostile, stronger, from `ai.war_grace_days`, no pact).
//!
//! At war a kingdom grows weary every day and offers peace once its exhaustion reaches
//! `war.exhaustion_peace` or the player is clearly stronger. Every `war.raid_interval_days` it
//! sends an army against the player (docs/game_design.md §11, [`crate::military`]): beaten
//! armies tire it further.

use rand::RngExt;

use super::actions::{self, start_war};
use super::relations::{self, remember};
use super::{KingdomId, MemoryCause, PeaceTerms, Proposal, ProposalKind, TreatyKind, economy, strength_ratio_percent};
use crate::defs::{Defs, ResourceAmount};
use crate::economy::{LedgerCategory, MILLI_PER_GOLD, storage};
use crate::event::{Notification, Severity, SimEvent};
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;

/// Hour of the day enemy kingdoms send their armies.
const RAID_HOUR: u64 = 14;

/// Days between two initiatives of a kingdom at least.
const INITIATIVE_GAP_DAYS: u64 = 2;

/// Days between two peace offers of a kingdom at least.
const PEACE_OFFER_GAP_DAYS: u64 = 4;

/// Days between two demands of a kingdom at least.
const DEMAND_GAP_DAYS: u64 = 6;

/// An initiative a kingdom may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Initiative {
    Propose(TreatyKind),
    Gift,
    Demand,
    Purchase,
    Sale,
    BorderIncident,
    DeclareWar,
    OfferPeace,
}

impl Initiative {
    /// True for initiatives that wait in the player's inbox and are paced by
    /// [`inbox_open`] (a peace offer of a kingdom at war always comes).
    const fn is_paced_proposal(self) -> bool {
        matches!(self, Initiative::Propose(_) | Initiative::Demand | Initiative::Purchase | Initiative::Sale)
    }
}

/// True if the inbox takes a new proposal, demand or offer now: from
/// `ai.first_proposal_day`, while fewer than `ai.max_open_proposals` wait and at least
/// `ai.proposal_gap_hours` (in the first `ai.early_days`: `ai.early_proposal_gap_hours`)
/// after the last one.
pub fn inbox_open(state: &GameState, defs: &Defs) -> bool {
    let ai = &defs.rules().diplomacy.ai;
    let today = state.clock.total_days();
    if today < u64::from(ai.first_proposal_day) || state.diplomacy.proposals.len() >= usize::from(ai.max_open_proposals) {
        return false;
    }
    let gap = if today < u64::from(ai.early_days) { ai.early_proposal_gap_hours } else { ai.proposal_gap_hours };
    state.diplomacy.last_proposal_tick.is_none_or(|t| state.clock.tick >= t + u64::from(gap) * TICKS_PER_HOUR)
}

/// Proposals whose time ran out count as declined.
pub(crate) fn expire(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    if !state.diplomacy.proposals.iter().any(|p| p.expires_tick <= tick) {
        return;
    }
    let (due, keep): (Vec<Proposal>, Vec<Proposal>) = state.diplomacy.proposals.iter().partition(|p| p.expires_tick <= tick);
    state.diplomacy.proposals = keep;
    for p in due {
        actions::resolve(state, defs, p, false, true, events);
    }
}

/// Hourly part: initiatives at the initiative hour, armies at the raid hour.
pub(crate) fn run_hour(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let hour = state.clock.hour();
    if hour == u64::from(defs.rules().diplomacy.ai.initiative_hour) {
        for i in 0..state.diplomacy.kingdoms.len() {
            initiative(state, defs, KingdomId::from_raw(i as u8), events);
        }
    }
    if hour == RAID_HOUR {
        for i in 0..state.diplomacy.kingdoms.len() {
            raid(state, defs, KingdomId::from_raw(i as u8), events);
        }
    }
}

/// The weighted initiatives kingdom `id` may take now.
fn candidates(state: &GameState, defs: &Defs, id: KingdomId) -> Vec<(Initiative, u32)> {
    let Some(k) = state.diplomacy.kingdom(id) else { return Vec::new() };
    let rules = &defs.rules().diplomacy;
    let p = rules.personalities.get(k.personality);
    let tick = state.clock.tick;
    let today = state.clock.total_days();
    let relation = relations::relation(state, defs, id);
    let ratio = strength_ratio_percent(state, defs, k);
    let superior = ratio >= u32::from(rules.ai.superiority_percent);
    let mut out = Vec::new();
    if let Some(war) = k.war {
        if war.exhaustion >= u16::from(rules.war.exhaustion_peace) || ratio < 70 {
            out.push((Initiative::OfferPeace, 100));
        }
        return out;
    }
    let pact = k.has_treaty(TreatyKind::NonAggression, tick);
    let alliance = k.has_treaty(TreatyKind::Alliance, tick);
    if !k.has_treaty(TreatyKind::TradeAgreement, tick)
        && relation >= 0
        && crate::economy::trade::active_trading_post(state, defs).is_some()
    {
        out.push((Initiative::Propose(TreatyKind::TradeAgreement), u32::from(p.trade_interest) + relation.max(0).unsigned_abs() / 2));
    }
    if !pact && relation >= -10 && (p.aggression <= 40 || ratio < 100) {
        out.push((Initiative::Propose(TreatyKind::NonAggression), (100 - u32::from(p.aggression)) / 2 + if ratio < 100 { 20 } else { 0 }));
    }
    if pact && !alliance && relation >= 70 {
        let common = state.diplomacy.kingdoms().any(|(o, ok)| ok.at_war() && state.diplomacy.ai_war_between(id, o).is_some());
        out.push((Initiative::Propose(TreatyKind::Alliance), 30 + if common { 40 } else { 0 }));
    }
    let gift_ok = k.treasury() >= 2 * i64::from(rules.ai.gift_min);
    if p.generosity > 0 && gift_ok && (relation >= 20 || (ratio < 80 && relation < 0)) {
        out.push((Initiative::Gift, u32::from(p.generosity)));
    }
    if today >= k.next_demand_day
        && p.aggression >= 30
        && superior
        && relation < 20
        && !pact
        && !alliance
        && state.economy.treasury() >= i64::from(rules.ai.demand_min)
    {
        out.push((Initiative::Demand, u32::from(p.aggression)));
    }
    if relation >= -10 {
        if purchase_offer(state, defs, id).is_some() {
            out.push((Initiative::Purchase, u32::from(p.trade_interest) / 2 + 5));
        }
        if sale_offer(state, defs, id).is_some() {
            out.push((Initiative::Sale, u32::from(p.trade_interest) / 2 + 5));
        }
    }
    if k.borders_player && p.expansion >= 40 {
        out.push((Initiative::BorderIncident, u32::from(p.expansion) / 2));
    }
    if today >= u64::from(rules.ai.war_grace_days) && !pact && !alliance && p.aggression >= 30 && relation < -50 && superior {
        out.push((Initiative::DeclareWar, u32::from(p.aggression) * 2));
    }
    out
}

/// The daily initiative of kingdom `id` (and its war exhaustion).
fn initiative(state: &mut GameState, defs: &Defs, id: KingdomId, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy;
    let today = state.clock.total_days();
    let Some(k) = state.diplomacy.kingdom_mut(id) else { return };
    if let Some(war) = &mut k.war {
        war.exhaustion = war.exhaustion.saturating_add(u16::from(rules.war.exhaustion_per_day));
    }
    if today < k.next_initiative_day || state.diplomacy.proposals.iter().any(|p| p.kingdom == id) {
        return;
    }
    let at_war = state.diplomacy.kingdoms[id.index()].at_war();
    if !at_war && state.diplomacy.rng.random_range(0..100u8) >= rules.ai.initiative_chance_percent {
        return;
    }
    let mut options = candidates(state, defs, id);
    if !inbox_open(state, defs) {
        options.retain(|(o, _)| !o.is_paced_proposal());
    }
    let total: u32 = options.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return;
    }
    let mut roll = state.diplomacy.rng.random_range(0..total);
    let mut chosen = None;
    for (o, w) in options {
        if roll < w {
            chosen = Some(o);
            break;
        }
        roll -= w;
    }
    let Some(chosen) = chosen else { return };
    let gap = if chosen == Initiative::OfferPeace { PEACE_OFFER_GAP_DAYS } else { INITIATIVE_GAP_DAYS };
    state.diplomacy.kingdoms[id.index()].next_initiative_day = today + gap;
    take(state, defs, id, chosen, events);
}

/// The good kingdom `id` would like to buy from the player now: resource, units, gold.
fn purchase_offer(state: &GameState, defs: &Defs, id: KingdomId) -> Option<(crate::defs::ResourceId, u32, u32)> {
    let k = state.diplomacy.kingdom(id)?;
    let rules = &defs.rules().diplomacy.ai;
    defs.resources()
        .iter()
        .filter_map(|r| {
            let wanted = economy::wanted_units(defs, k, r.id);
            let available = state.economy.available(r.id);
            let units = wanted.min(available / 2).min(rules.offer_units);
            if units < 5 {
                return None;
            }
            let price = u64::from(economy::price_centi(defs, k, r.id));
            let gold = u32::try_from(u64::from(units) * price * (100 + u64::from(rules.offer_premium_percent)) / 10_000).ok()?;
            (gold > 0 && i64::from(gold) <= k.treasury()).then_some((r.id, units, gold))
        })
        .max_by_key(|(r, _, gold)| (*gold, std::cmp::Reverse(*r)))
}

/// The good kingdom `id` would like to sell to the player now: resource, units, gold.
fn sale_offer(state: &GameState, defs: &Defs, id: KingdomId) -> Option<(crate::defs::ResourceId, u32, u32)> {
    let k = state.diplomacy.kingdom(id)?;
    let rules = &defs.rules().diplomacy.ai;
    defs.resources()
        .iter()
        .filter(|r| r.import_only || state.economy.stock().get(r.id) < 20)
        .filter_map(|r| {
            let units = economy::offered_units(defs, k, r.id).min(rules.offer_units);
            if units < 5 {
                return None;
            }
            let price = u64::from(economy::price_centi(defs, k, r.id));
            let gold = u32::try_from(u64::from(units) * price * (100 - u64::from(rules.sale_discount_percent)) / 10_000).ok()?;
            (gold > 0).then_some((r.id, units, gold))
        })
        .max_by_key(|(r, _, gold)| (*gold, std::cmp::Reverse(*r)))
}

/// Puts a proposal of kingdom `id` into the inbox and announces it.
fn propose(state: &mut GameState, defs: &Defs, id: KingdomId, kind: ProposalKind, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    let serial = state.diplomacy.next_serial;
    state.diplomacy.next_serial = serial.wrapping_add(1).max(1);
    let expires_tick = tick + u64::from(defs.rules().diplomacy.ai.decision_hours) * TICKS_PER_HOUR;
    state.diplomacy.proposals.push(Proposal { serial, kingdom: id, kind, created_tick: tick, expires_tick });
    state.diplomacy.last_proposal_tick = Some(tick);
    let Some(k) = state.diplomacy.kingdom(id) else { return };
    let ruler = format!("{} {}", k.title, k.ruler);
    let name = k.name.clone();
    let seat = k.seat;
    let hours = defs.rules().diplomacy.ai.decision_hours;
    let until = crate::report::deadline_clock_text(tick, expires_tick);
    let what = match kind {
        ProposalKind::Treaty(t) => format!("{ruler} von {name} schlägt {} vor.", t.accusative()),
        ProposalKind::Tribute { gold } => format!("{ruler} von {name} fordert {gold} Gold Tribut."),
        ProposalKind::Peace { terms } => format!("{name} bietet Frieden an: {}.", terms.text(&name)),
        ProposalKind::Purchase { resource, amount, gold } => {
            format!("{name} möchte {amount} {} für {gold} Gold kaufen.", defs.resource(resource).name)
        }
        ProposalKind::Sale { resource, amount, gold } => {
            format!("{name} bietet {amount} {} für {gold} Gold an.", defs.resource(resource).name)
        }
    };
    let severity = match kind {
        ProposalKind::Tribute { .. } => Severity::Warning,
        ProposalKind::Peace { .. } => Severity::Good,
        _ => Severity::Info,
    };
    let text = format!("{what} Antwort binnen {} (bis {until}).", crate::report::hours_text(u64::from(hours)));
    state.diplomacy.record(tick, Some(id), None, what);
    events.push(SimEvent::ProposalReceived { serial, kingdom: id, expires_tick });
    events.push(SimEvent::Notification(Notification::new(severity, text).at(seat)));
}

/// Carries out `initiative` of kingdom `id`.
fn take(state: &mut GameState, defs: &Defs, id: KingdomId, initiative: Initiative, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy;
    let tick = state.clock.tick;
    let today = state.clock.total_days();
    match initiative {
        Initiative::Propose(t) => propose(state, defs, id, ProposalKind::Treaty(t), events),
        Initiative::Demand => {
            let gold = u32::try_from(state.economy.treasury().max(0) * i64::from(rules.ai.demand_percent) / 100)
                .unwrap_or(u32::MAX)
                .clamp(rules.ai.demand_min, rules.ai.demand_max);
            state.diplomacy.kingdoms[id.index()].next_demand_day = today + DEMAND_GAP_DAYS;
            propose(state, defs, id, ProposalKind::Tribute { gold }, events);
        }
        Initiative::Purchase => {
            if let Some((resource, amount, gold)) = purchase_offer(state, defs, id) {
                propose(state, defs, id, ProposalKind::Purchase { resource, amount, gold }, events);
            }
        }
        Initiative::Sale => {
            if let Some((resource, amount, gold)) = sale_offer(state, defs, id) {
                propose(state, defs, id, ProposalKind::Sale { resource, amount, gold }, events);
            }
        }
        Initiative::OfferPeace => {
            let Some(k) = state.diplomacy.kingdom(id) else { return };
            let ratio = strength_ratio_percent(state, defs, k);
            let terms = if ratio >= 150 {
                let gold = u32::try_from(state.economy.treasury().max(0) * i64::from(rules.ai.demand_percent) / 100)
                    .unwrap_or(u32::MAX)
                    .clamp(rules.ai.demand_min, rules.ai.demand_max);
                PeaceTerms::PlayerPays { gold }
            } else if ratio < 70 {
                let gold = u32::try_from(k.treasury().max(0) * i64::from(rules.war.reparations_percent) / 100).unwrap_or(0);
                if gold > 0 { PeaceTerms::KingdomPays { gold } } else { PeaceTerms::White }
            } else {
                PeaceTerms::White
            };
            propose(state, defs, id, ProposalKind::Peace { terms }, events);
        }
        Initiative::Gift => {
            let k = &state.diplomacy.kingdoms[id.index()];
            let gold = u32::try_from(k.treasury().max(0) * i64::from(rules.ai.gift_percent) / 100)
                .unwrap_or(u32::MAX)
                .clamp(rules.ai.gift_min, rules.ai.gift_max);
            let goods = sale_offer(state, defs, id).filter(|(_, _, value)| *value >= rules.ai.gift_min);
            let name = k.name.clone();
            let seat = k.seat;
            let use_goods = goods.is_some() && state.diplomacy.rng.random_range(0..2u8) == 0;
            let text = match goods.filter(|_| use_goods) {
                Some((resource, amount, _)) => {
                    let at = crate::economy::trade::active_trading_post(state, defs)
                        .or_else(|| storage::main_headquarters(state, defs))
                        .and_then(|b| state.buildings.get(b))
                        .map_or(state.world.start, |b| b.entrance());
                    let available = state.diplomacy.kingdoms[id.index()].stock(resource).min(amount);
                    let stored = storage::deposit(state, defs, resource, available, at);
                    economy::take(&mut state.diplomacy.kingdoms[id.index()], resource, stored);
                    events.push(SimEvent::GiftReceived { kingdom: id, gold: 0, goods: vec![ResourceAmount { resource, amount: stored }] });
                    format!("{name} schickt Euch ein Geschenk: {stored} {}.", defs.resource(resource).name)
                }
                None => {
                    state.diplomacy.kingdoms[id.index()].treasury_milli -= i64::from(gold) * MILLI_PER_GOLD;
                    state.economy.book_gold(LedgerCategory::Diplomacy, i64::from(gold));
                    events.push(SimEvent::GiftReceived { kingdom: id, gold, goods: Vec::new() });
                    format!("{name} schickt Euch ein Geschenk: {gold} Gold.")
                }
            };
            state.diplomacy.record(tick, Some(id), None, text.clone());
            events.push(SimEvent::Notification(Notification::new(Severity::Good, text).at(seat)));
        }
        Initiative::BorderIncident => {
            let points = f32::from(rules.relations.border_incident);
            let k = &mut state.diplomacy.kingdoms[id.index()];
            remember(k, defs, MemoryCause::BorderIncident, points, None);
            let text = format!("Grenzzwischenfall: Bewaffnete aus {} haben Eure Grenzsteine versetzt.", k.name);
            let seat = k.seat;
            state.diplomacy.record(tick, Some(id), None, text.clone());
            events.push(SimEvent::BorderIncident { kingdom: id });
            events.push(SimEvent::Notification(Notification::new(Severity::Warning, text).at(seat)));
            relations::announce_states(state, defs, events);
        }
        Initiative::DeclareWar => start_war(state, defs, id, false, events),
    }
}

/// After the player refused a demand: an aggressive, clearly stronger kingdom without a pact
/// declares war once the grace period is over.
pub(crate) fn after_refused_tribute(state: &mut GameState, defs: &Defs, id: KingdomId, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy;
    let Some(k) = state.diplomacy.kingdom(id) else { return };
    let tick = state.clock.tick;
    let aggressive = rules.personalities.get(k.personality).aggression >= 50;
    let superior = strength_ratio_percent(state, defs, k) >= u32::from(rules.ai.superiority_percent);
    let protected = k.has_treaty(TreatyKind::NonAggression, tick) || k.has_treaty(TreatyKind::Alliance, tick);
    if aggressive && superior && !protected && !k.at_war() && state.clock.total_days() >= u64::from(rules.ai.war_grace_days) {
        start_war(state, defs, id, false, events);
    }
}

/// An army of kingdom `id` against the player if it is at war and its next attack is due
/// ([`crate::military`] fights it out on the map; without enough soldiers it waits a day).
fn raid(state: &mut GameState, defs: &Defs, id: KingdomId, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().diplomacy.war;
    let today = state.clock.total_days();
    let Some(war) = state.diplomacy.kingdom(id).and_then(|k| k.war) else { return };
    if today < war.next_raid_day {
        return;
    }
    let sent = crate::military::dispatch_army(state, defs, id, events);
    if let Some(w) = state.diplomacy.kingdom_mut(id).and_then(|k| k.war.as_mut()) {
        w.next_raid_day = today + if sent { u64::from(rules.raid_interval_days) } else { 1 };
    }
}
