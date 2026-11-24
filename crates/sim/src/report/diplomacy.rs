//! Diplomacy window (docs/game_design.md §10): kingdoms overview, the available actions per
//! kingdom with costs and reasons, the diplomatic inbox, caravans and trade prices per partner.

use glam::IVec2;
use serde::{Deserialize, Serialize};

use super::walking::{minutes_text, ticks_to_minutes};
use crate::defs::{Defs, Personality, ResourceId};
use crate::diplomacy::actions::{self, Judgement};
use crate::diplomacy::caravans::{self, CaravanKind, CaravanStage};
use crate::diplomacy::relations::{self, RelationPart};
use crate::diplomacy::{
    DiplomaticAction, KingdomId, PeaceTerms, ProposalKind, RelationState, TreatyKind, economy, has_access, kingdom_strength,
    player_strength,
};
use crate::economy::trade::TradeDirection;
use crate::state::GameState;
use crate::time::{GameClock, TICKS_PER_DAY};

/// Goods a kingdom wants or offers listed at most.
const GOODS_LISTED: usize = 6;

/// Diplomatic records listed per kingdom.
const HISTORY_PER_KINGDOM: usize = 8;

/// A good a kingdom wants or offers, with the player's unit price.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodRow {
    /// The good.
    pub resource: ResourceId,
    /// German name.
    pub name: String,
    /// Units.
    pub amount: u32,
    /// Unit price for the player in centi-gold (wanted: what the player earns selling;
    /// offered: what the player pays buying).
    pub price_centi: u32,
}

/// A treaty in force.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatyRow {
    /// Kind.
    pub kind: TreatyKind,
    /// German name.
    pub name: String,
    /// Whole days left (rounded up).
    pub days_left: u64,
    /// „noch 12 Tage“.
    pub remaining_text: String,
}

/// The war with the player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarRow {
    /// Days since it began.
    pub days: u64,
    /// The kingdom's war exhaustion (it offers peace from `war.exhaustion_peace`).
    pub exhaustion: u16,
    /// Raids so far.
    pub raids: u16,
    /// True if the player declared it.
    pub declared_by_player: bool,
    /// German summary.
    pub text: String,
}

/// A line of the diplomatic history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRow {
    /// When.
    pub tick: u64,
    /// German date and time.
    pub date: String,
    /// German text.
    pub text: String,
}

/// One kingdom in the overview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KingdomRow {
    /// Id.
    pub id: KingdomId,
    /// Realm name.
    pub name: String,
    /// „König Adalbert“.
    pub ruler: String,
    /// Personality.
    pub personality: Personality,
    /// German personality („händlerisch“).
    pub personality_name: String,
    /// Colour index for the UI (0–6, distinct per kingdom).
    pub color: u8,
    /// Seat.
    pub seat: IVec2,
    /// Relation to the player (−100 … +100).
    pub relation: i32,
    /// Diplomatic state.
    pub state: RelationState,
    /// German state („Freundschaftlich“; „Im Krieg“ during a war).
    pub state_name: String,
    /// The parts of the relation with German labels.
    pub relation_parts: Vec<RelationPart>,
    /// The war with the player.
    pub war: Option<WarRow>,
    /// Treaties in force.
    pub treaties: Vec<TreatyRow>,
    /// Inhabitants.
    pub population: u32,
    /// Strength estimate (rounded to 5).
    pub strength: u32,
    /// German comparison with the player's strength („etwa doppelt so stark wie Ihr“).
    pub strength_text: String,
    /// Owned tiles.
    pub territory_tiles: u32,
    /// Its territory touches the player's.
    pub borders_player: bool,
    /// Structures of its settlement.
    pub structures: usize,
    /// Wealth estimate in gold (treasury and stock, rounded to 100).
    pub wealth: i64,
    /// Goods it wants (most valuable first).
    pub wanted: Vec<GoodRow>,
    /// Goods it offers (most valuable first).
    pub offered: Vec<GoodRow>,
    /// True if the player may trade with it.
    pub access: bool,
    /// German reason of the trade access.
    pub access_text: String,
    /// Robbery risk of a caravan leg to it, in percent.
    pub route_danger_percent: u32,
    /// Kingdoms it is at war with (names).
    pub enemies: Vec<String>,
    /// Recent diplomatic history with it, newest first.
    pub history: Vec<HistoryRow>,
}

/// The whole diplomacy window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiplomacyReport {
    /// All kingdoms in id order.
    pub kingdoms: Vec<KingdomRow>,
    /// The player's strength (same scale as the kingdoms').
    pub player_strength: u32,
    /// Proposals waiting for an answer.
    pub proposals: Vec<ProposalRow>,
    /// Caravans on the way.
    pub caravans: Vec<CaravanRow>,
    /// Wars between kingdoms (German).
    pub ai_wars: Vec<String>,
    /// Recent diplomatic history, newest first.
    pub history: Vec<HistoryRow>,
}

/// An action the player may take towards a kingdom.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRow {
    /// The action as `Command::Diplomacy` takes it (amounts are suggestions the UI may change).
    pub action: DiplomaticAction,
    /// German label.
    pub label: String,
    /// German costs and consequences.
    pub cost_text: String,
    /// Why it is impossible now (`None`: possible).
    pub problem: Option<String>,
    /// How the kingdom would judge it now (proposals only).
    pub prospect: Option<Judgement>,
}

/// A proposal in the diplomatic inbox.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalRow {
    /// Serial number (`Command::AnswerProposal`).
    pub serial: u32,
    /// From whom.
    pub kingdom: KingdomId,
    /// Its name.
    pub kingdom_name: String,
    /// The proposal.
    pub kind: ProposalKind,
    /// German title („Handelsvertrag angeboten“).
    pub title: String,
    /// German text.
    pub text: String,
    /// Tick from which it counts as declined.
    pub expires_tick: u64,
    /// „noch 5 h 12 min“.
    pub remaining_text: String,
    /// German label of accepting.
    pub accept_label: String,
    /// German label of declining.
    pub decline_label: String,
    /// Why accepting is impossible now (`None`: possible).
    pub accept_problem: Option<String>,
    /// German consequence of declining.
    pub decline_text: String,
}

/// A caravan on the way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaravanRow {
    /// Id.
    pub id: u32,
    /// The partner kingdom.
    pub kingdom: KingdomId,
    /// Its name.
    pub kingdom_name: String,
    /// Foreign (inbound) or own (outbound).
    pub kind: CaravanKind,
    /// German stage („Unterwegs zu Euch“, „Am Handelskontor“ …).
    pub stage_text: String,
    /// Game minutes until the current stage ends, if known.
    pub eta_minutes: Option<u32>,
    /// „noch 3 h 20 min“ or empty.
    pub eta_text: String,
    /// Goods carried.
    pub goods: Vec<GoodRow>,
    /// Gold carried.
    pub gold: i64,
    /// Position of its merchant on the map.
    pub position: Option<IVec2>,
    /// Robbery risk per off-map leg in percent.
    pub danger_percent: u32,
}

/// A good's prices with one partner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartnerPriceRow {
    /// The good.
    pub resource: ResourceId,
    /// German name.
    pub name: String,
    /// Price the player pays per unit, in centi-gold.
    pub buy_centi: u32,
    /// Price the player earns per unit, in centi-gold.
    pub sell_centi: u32,
    /// Units the partner wants.
    pub wanted: u32,
    /// Units the partner offers.
    pub offered: u32,
}

fn history_row(tick: u64, text: &str) -> HistoryRow {
    let clock = GameClock::at_tick(tick);
    HistoryRow { tick, date: format!("{} {}", clock.date_string(), clock.time_string()), text: text.to_owned() }
}

fn days_text(days: u64) -> String {
    if days == 1 { "noch 1 Tag".to_owned() } else { format!("noch {days} Tage") }
}

/// German comparison of two strengths.
fn strength_text(theirs: u32, ours: u32) -> String {
    let ratio = u64::from(theirs) * 100 / u64::from(ours.max(1));
    match ratio {
        0..=39 => "deutlich schwächer als Ihr".to_owned(),
        40..=79 => "schwächer als Ihr".to_owned(),
        80..=124 => "etwa so stark wie Ihr".to_owned(),
        125..=174 => "stärker als Ihr".to_owned(),
        175..=249 => "etwa doppelt so stark wie Ihr".to_owned(),
        _ => "weit stärker als Ihr".to_owned(),
    }
}

fn goods_rows(state: &GameState, defs: &Defs, id: KingdomId, wanted: bool) -> Vec<GoodRow> {
    let Some(k) = state.diplomacy.kingdom(id) else { return Vec::new() };
    let mut rows: Vec<(u64, GoodRow)> = defs
        .resources()
        .iter()
        .filter_map(|r| {
            let amount = if wanted { economy::wanted_units(defs, k, r.id) } else { economy::offered_units(defs, k, r.id) };
            if amount == 0 {
                return None;
            }
            let direction = if wanted { TradeDirection::Export } else { TradeDirection::Import };
            let price_centi = economy::unit_price_centi(state, defs, id, r.id, direction);
            let row = GoodRow { resource: r.id, name: r.name.clone(), amount, price_centi };
            Some((u64::from(amount) * u64::from(price_centi), row))
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.resource.cmp(&b.1.resource)));
    rows.into_iter().take(GOODS_LISTED).map(|(_, r)| r).collect()
}

/// One kingdom of the overview.
pub fn kingdom_row(state: &GameState, defs: &Defs, id: KingdomId) -> Option<KingdomRow> {
    let k = state.diplomacy.kingdom(id)?;
    let tick = state.clock().tick;
    let relation = relations::relation(state, defs, id);
    let rstate = relations::relation_state(state, defs, id);
    let ours = player_strength(state, defs);
    let theirs = kingdom_strength(k, defs);
    let war = k.war().map(|w| {
        let days = tick.saturating_sub(w.since_tick) / TICKS_PER_DAY;
        let who = if w.declared_by_player { "von Euch erklärt" } else { "von ihnen erklärt" };
        let raids = if w.raids == 1 { "1 Überfall".to_owned() } else { format!("{} Überfälle", w.raids) };
        WarRow {
            days,
            exhaustion: w.exhaustion,
            raids: w.raids,
            declared_by_player: w.declared_by_player,
            text: format!("Krieg {} ({who}), {raids}, Kriegsmüdigkeit {}", super::since_days_text(days), w.exhaustion),
        }
    });
    let treaties = k
        .treaties()
        .iter()
        .filter(|t| t.until_tick > tick)
        .map(|t| {
            let days_left = (t.until_tick - tick).div_ceil(TICKS_PER_DAY);
            TreatyRow { kind: t.kind, name: t.kind.display_name().to_owned(), days_left, remaining_text: days_text(days_left) }
        })
        .collect();
    let access = has_access(state, defs, id);
    let access_text = if k.at_war() {
        "Kein Handel: Krieg.".to_owned()
    } else if k.has_treaty(TreatyKind::TradeAgreement, tick) {
        "Handel mit Handelsvertrag (bessere Preise, mehr Karawanen).".to_owned()
    } else if access {
        "Handel möglich.".to_owned()
    } else {
        "Kein Handel: Die Beziehung ist zu schlecht (mindestens „Neutral“ oder ein Handelsvertrag).".to_owned()
    };
    let enemies = state.diplomacy.enemies_of(id).filter_map(|e| state.diplomacy.kingdom(e)).map(|e| e.name().to_owned()).collect();
    let history = state
        .diplomacy
        .history()
        .rev()
        .filter(|r| r.kingdom == Some(id) || r.other == Some(id))
        .take(HISTORY_PER_KINGDOM)
        .map(|r| history_row(r.tick, &r.text))
        .collect();
    Some(KingdomRow {
        id,
        name: k.name().to_owned(),
        ruler: format!("{} {}", k.title(), k.ruler()),
        personality: k.personality(),
        personality_name: k.personality().display_name().to_owned(),
        color: k.color(),
        seat: k.seat(),
        relation,
        state: rstate,
        state_name: if k.at_war() { "Im Krieg".to_owned() } else { rstate.display_name().to_owned() },
        relation_parts: relations::relation_parts(state, defs, id),
        war,
        treaties,
        population: k.population(),
        strength: theirs.div_ceil(5) * 5,
        strength_text: strength_text(theirs, ours),
        territory_tiles: k.territory_tiles(),
        borders_player: k.borders_player(),
        structures: k.structures().len(),
        wealth: (economy::wealth_estimate(defs, k) + 50) / 100 * 100,
        wanted: goods_rows(state, defs, id, true),
        offered: goods_rows(state, defs, id, false),
        access,
        access_text,
        route_danger_percent: caravans::route_danger(state, defs, id).0,
        enemies,
        history,
    })
}

/// The diplomacy window: kingdoms, inbox, caravans, wars between kingdoms and the history.
pub fn diplomacy_report(state: &GameState, defs: &Defs) -> DiplomacyReport {
    let d = state.diplomacy();
    let ai_wars = d
        .ai_wars()
        .iter()
        .filter_map(|w| {
            let (a, b) = (d.kingdom(w.attacker)?, d.kingdom(w.defender)?);
            let days = state.clock().tick.saturating_sub(w.since_tick) / TICKS_PER_DAY;
            Some(format!("{} gegen {} ({})", a.name(), b.name(), super::since_days_text(days)))
        })
        .collect();
    DiplomacyReport {
        kingdoms: d.kingdoms().filter_map(|(id, _)| kingdom_row(state, defs, id)).collect(),
        player_strength: player_strength(state, defs),
        proposals: pending_proposals(state, defs),
        caravans: caravans_report(state, defs),
        ai_wars,
        history: d.history().rev().take(16).map(|r| history_row(r.tick, &r.text)).collect(),
    }
}

/// Suggested gift: a tenth of the treasury, 50 to 500 gold.
fn suggested_gift(state: &GameState) -> u32 {
    u32::try_from((state.economy().treasury() / 10).clamp(50, 500)).unwrap_or(50)
}

/// The actions the player may take towards kingdom `id` (with suggested amounts), each with
/// costs, the reason it is impossible and the kingdom's expected judgement.
pub fn kingdom_actions(state: &GameState, defs: &Defs, id: KingdomId) -> Vec<ActionRow> {
    let Some(k) = state.diplomacy().kingdom(id) else { return Vec::new() };
    let rules = &defs.rules().diplomacy;
    let tick = state.clock().tick;
    let mut actions_list: Vec<(DiplomaticAction, String, String)> = Vec::new();
    for t in TreatyKind::ALL {
        if k.has_treaty(t, tick) {
            let penalty = match t {
                TreatyKind::TradeAgreement => rules.treaties.break_trade,
                TreatyKind::NonAggression => rules.treaties.break_pact,
                TreatyKind::Alliance => rules.treaties.break_alliance,
            };
            actions_list.push((
                DiplomaticAction::CancelTreaty { treaty: t },
                format!("{} kündigen", t.display_name()),
                format!("Beziehung {penalty}, bei allen anderen Reichen {}", rules.relations.broken_word),
            ));
        } else {
            actions_list.push((
                DiplomaticAction::ProposeTreaty { treaty: t },
                format!("{} vorschlagen", t.display_name()),
                format!("{} Tage", t.days(defs)),
            ));
        }
    }
    let gift = suggested_gift(state);
    actions_list.push((
        DiplomaticAction::Gift { gold: gift, goods: Vec::new() },
        "Geschenk senden".to_owned(),
        format!("{gift} Gold, Beziehung etwa +{:.0}", actions::gift_points(defs, i64::from(gift))),
    ));
    let demand = u32::try_from(k.treasury().max(0) / 10).unwrap_or(0).clamp(rules.ai.demand_min, rules.ai.demand_max);
    actions_list.push((
        DiplomaticAction::DemandTribute { gold: demand },
        "Tribut fordern".to_owned(),
        format!("{demand} Gold, Beziehung {} auch bei Zahlung", rules.relations.demanded),
    ));
    if k.at_war() {
        let reparations = u32::try_from(k.treasury().max(0) * i64::from(rules.war.reparations_percent) / 100).unwrap_or(0).max(1);
        let payment = suggested_gift(state);
        for (terms, label) in [
            (PeaceTerms::White, "Frieden ohne Bedingungen anbieten".to_owned()),
            (PeaceTerms::KingdomPays { gold: reparations }, format!("Frieden gegen {reparations} Gold Entschädigung fordern")),
            (PeaceTerms::BorderCeded, "Frieden gegen Gebietsabtretung fordern".to_owned()),
            (PeaceTerms::PlayerPays { gold: payment }, format!("Frieden für {payment} Gold erkaufen")),
        ] {
            actions_list.push((DiplomaticAction::ProposePeace { terms }, label, format!("Beziehung +{} bei Frieden", rules.relations.peace_made)));
        }
    } else {
        actions_list.push((
            DiplomaticAction::DeclareWar,
            "Krieg erklären".to_owned(),
            format!("Beziehung {}, Handel ruht, Plünderungen, Kriegsmüdigkeit der Bürger", rules.relations.war_declared),
        ));
    }
    actions_list
        .into_iter()
        .map(|(action, label, cost_text)| {
            let problem = actions::action_problem(state, defs, id, &action);
            let prospect = if problem.is_none() { actions::judge(state, defs, id, &action) } else { None };
            ActionRow { action, label, cost_text, problem, prospect }
        })
        .collect()
}

/// The proposals waiting in the diplomatic inbox, oldest first.
pub fn pending_proposals(state: &GameState, defs: &Defs) -> Vec<ProposalRow> {
    let tick = state.clock().tick;
    state
        .diplomacy()
        .proposals()
        .iter()
        .filter_map(|p| {
            let k = state.diplomacy().kingdom(p.kingdom)?;
            let name = k.name().to_owned();
            let ruler = format!("{} {}", k.title(), k.ruler());
            let (title, text, accept_label, decline_label, decline_text) = match p.kind {
                ProposalKind::Treaty(t) => (
                    format!("{} angeboten", t.display_name()),
                    format!("{ruler} von {name} schlägt {} für {} Tage vor.", t.accusative(), t.days(defs)),
                    "Annehmen".to_owned(),
                    "Ablehnen".to_owned(),
                    format!("Beziehung {}", defs.rules().diplomacy.relations.proposal_declined),
                ),
                ProposalKind::Tribute { gold } => (
                    "Tributforderung".to_owned(),
                    format!("{ruler} von {name} fordert {gold} Gold Tribut."),
                    format!("{gold} Gold zahlen"),
                    "Verweigern".to_owned(),
                    format!("Beziehung {}; ein überlegenes, angriffslustiges Reich kann den Krieg erklären", defs.rules().diplomacy.relations.demand_refused),
                ),
                ProposalKind::Peace { terms } => (
                    "Friedensangebot".to_owned(),
                    format!("{name} bietet an: {}.", terms.text(&name)),
                    "Frieden schließen".to_owned(),
                    "Weiterkämpfen".to_owned(),
                    "Der Krieg geht weiter".to_owned(),
                ),
                ProposalKind::Purchase { resource, amount, gold } => (
                    "Kaufangebot".to_owned(),
                    format!("{name} möchte {amount} {} für {gold} Gold kaufen.", defs.resource(resource).name),
                    "Verkaufen".to_owned(),
                    "Ablehnen".to_owned(),
                    format!("Beziehung {}", defs.rules().diplomacy.relations.proposal_declined),
                ),
                ProposalKind::Sale { resource, amount, gold } => (
                    "Verkaufsangebot".to_owned(),
                    format!("{name} bietet {amount} {} für {gold} Gold an.", defs.resource(resource).name),
                    "Kaufen".to_owned(),
                    "Ablehnen".to_owned(),
                    format!("Beziehung {}", defs.rules().diplomacy.relations.proposal_declined),
                ),
            };
            Some(ProposalRow {
                serial: p.serial,
                kingdom: p.kingdom,
                kingdom_name: name,
                kind: p.kind,
                title,
                text,
                expires_tick: p.expires_tick,
                remaining_text: format!("noch {}", minutes_text(ticks_to_minutes(p.expires_tick.saturating_sub(tick)))),
                accept_label,
                decline_label,
                accept_problem: actions::proposal_problem(state, defs, p),
                decline_text,
            })
        })
        .collect()
}

/// Caravans on the way, in id order.
pub fn caravans_report(state: &GameState, defs: &Defs) -> Vec<CaravanRow> {
    let tick = state.clock().tick;
    state
        .diplomacy()
        .caravans()
        .iter()
        .filter_map(|c| {
            let k = state.diplomacy().kingdom(c.kingdom)?;
            let name = k.name();
            let stage_text = match (c.kind, c.stage) {
                (CaravanKind::Foreign, CaravanStage::Travelling { outward: true, .. }) => format!("Unterwegs aus {name} zu Euch"),
                (CaravanKind::Foreign, CaravanStage::Travelling { outward: false, .. }) => format!("Auf dem Heimweg nach {name}"),
                (CaravanKind::Foreign, CaravanStage::Arriving) => "Auf dem Weg zum Handelskontor".to_owned(),
                (_, CaravanStage::Trading { .. }) => "Am Handelskontor".to_owned(),
                (CaravanKind::Foreign, CaravanStage::Leaving) => "Verlässt Euer Land".to_owned(),
                (CaravanKind::Own, CaravanStage::Leaving) => format!("Bricht nach {name} auf"),
                (CaravanKind::Own, CaravanStage::Travelling { outward: true, .. }) => format!("Unterwegs nach {name}"),
                (CaravanKind::Own, CaravanStage::Travelling { outward: false, .. }) => format!("Auf dem Rückweg aus {name}"),
                (CaravanKind::Own, CaravanStage::Arriving) => "Zurück im Land, auf dem Weg zum Handelskontor".to_owned(),
            };
            let eta_minutes = c.ticks_left(tick).map(ticks_to_minutes);
            let position = c
                .walkers
                .first()
                .and_then(|w| state.population().walking().traveler(*w))
                .map(|t| t.motion().pos().tile());
            Some(CaravanRow {
                id: c.id,
                kingdom: c.kingdom,
                kingdom_name: name.to_owned(),
                kind: c.kind,
                stage_text,
                eta_minutes,
                eta_text: eta_minutes.map_or_else(String::new, |m| format!("noch {}", minutes_text(m))),
                goods: c
                    .cargo
                    .iter()
                    .map(|m| GoodRow { resource: m.resource, name: defs.resource(m.resource).name.clone(), amount: m.amount, price_centi: 0 })
                    .collect(),
                gold: c.purse_milli / crate::economy::MILLI_PER_GOLD,
                position,
                danger_percent: caravans::route_danger(state, defs, c.kingdom).0,
            })
        })
        .collect()
}

/// Prices of every good with kingdom `id` (the player's buy and sell prices, what it wants and
/// offers), in resource id order.
pub fn partner_prices(state: &GameState, defs: &Defs, id: KingdomId) -> Vec<PartnerPriceRow> {
    let Some(k) = state.diplomacy().kingdom(id) else { return Vec::new() };
    defs.resources()
        .iter()
        .map(|r| PartnerPriceRow {
            resource: r.id,
            name: r.name.clone(),
            buy_centi: economy::unit_price_centi(state, defs, id, r.id, TradeDirection::Import),
            sell_centi: economy::unit_price_centi(state, defs, id, r.id, TradeDirection::Export),
            wanted: economy::wanted_units(defs, k, r.id),
            offered: economy::offered_units(defs, k, r.id),
        })
        .collect()
}
