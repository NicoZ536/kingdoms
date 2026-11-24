//! Threats (docs/game_design.md §11): bands of bandits and armies of AI kingdoms at war —
//! gathering off the map, arriving at the map edge, their decisions every combat round, plunder,
//! breaking walls and gates, the conquest of the headquarters, fleeing and the battle report.
//!
//! * **Bandits** (raid event, `military.bandits`): not before the grace period is over
//!   ([`raid_grace_over`]); `min` + one per `citizens_per_bandit` citizens (at most `max`) ×
//!   `size_percent` of the difficulty, one fewer per `defenders_per_bandit` defenders (at most
//!   `max_deterrence_percent` fewer; [`bandit_band`]), scaled by the event; they arrive
//!   `warning_hours` after the scouts' report (at least `delay_hours` after the decision)
//!   at a random point of the map edge and go for the nearest storage without safety coverage
//!   (without one: the nearest unguarded building, else the nearest storage). There every bandit
//!   carries off `steal_units` goods (food, goods, luxuries, the largest stocks first) and
//!   `steal_gold` gold; with `fire_percent` chance they set the place on fire. Then they flee
//!   the way they came; they also flee after losing `flee_losses_percent` of the band.
//! * **Armies** (every `diplomacy.war.raid_interval_days` of a war, `military.armies`):
//!   `share_percent` of the kingdom's soldiers (at most `max_size`, none below
//!   `min_soldiers`) in the unit mix of the data; they travel like caravans (the arrival time is
//!   known) and enter at the map edge towards their kingdom. Aggressive kingdoms (from
//!   `conquest_aggression`) march on the headquarters: once it is broken and no own soldier
//!   stands within `hq_defense_radius` tiles for `hq_capture_hours`, the kingdom is conquered
//!   (defeat). The others plunder the richest storage (`diplomacy.war.raid_percent` of its
//!   goods, arson like bandits) and withdraw. An army retreats after losing
//!   `retreat_losses_percent`; survivors and their plunder return to the kingdom; a beaten
//!   army tires the kingdom (`exhaustion_defeat`) and angers it (`relation_defeat`).
//! * Groups enter at the map-edge tile towards their origin from which the headquarters can be
//!   reached (a walled-in town: from which a wall or gate can be reached). They walk to the
//!   entrance of their target or, if it is built over, to the nearest free tile beside it, and
//!   prefer targets they can walk to.
//! * Enemies attack own soldiers within their reach that they can get at (bandits only next to
//!   them), break a gate that stands in their way (gates let only own people through) — also on
//!   the way to a soldier behind it or out of the map — and, without any path to their target,
//!   the wall or gate nearest to it. Without a path and without a wall they turn to another
//!   reachable target or withdraw.
//! * Peace: an approaching army turns back silently; one on the map withdraws
//!   ([`WarbandStage::Withdrawing`]: no more fighting, gates let it pass) and gets a battle
//!   report only if it fought.

use std::collections::{BTreeMap, BTreeSet};

use glam::IVec2;
use rand::RngExt;

use super::{
    BattleKind, BattleOutcome, BattleReport, Fighter, FighterKind, Tally, Warband, WarbandKind, WarbandStage, count_text,
    fighter_stats, tile_distance,
};
use crate::buildings::BuildingId;
use crate::defs::{BuildingRole, BuildingTypeId, DamageCause, Defs, ResourceAmount, ResourceCategory, ServiceKind};
use crate::diplomacy::{KingdomId, MemoryCause};
use crate::economy::{LedgerCategory, MILLI_PER_GOLD, storage};
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::population::{Destination, Motion, ServiceIndex, Traveler, TravelerId, TripPhase, WalkerId, movement};
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;

/// Ticks until an approaching group arrives (`None` once it is on the map).
pub fn warband_eta(state: &GameState, w: &Warband) -> Option<u64> {
    match w.stage {
        WarbandStage::Approaching { arrive_tick } => Some(arrive_tick.saturating_sub(state.clock.tick)),
        _ => None,
    }
}

fn building_name(state: &GameState, defs: &Defs, id: BuildingId) -> String {
    state
        .buildings
        .get(id)
        .and_then(|b| defs.buildings().get(b.kind().index()))
        .map_or_else(|| "Gebäude".to_owned(), |d| d.name.clone())
}

/// „Die Feinde haben das Tor durchbrochen!“ / „… die Stadtmauer …“: the broken wall piece or
/// gate in the accusative with its article (gates are neuter, wall segments feminine).
pub(super) fn breach_text(role: Option<BuildingRole>, name: &str) -> String {
    let article = if role == Some(BuildingRole::Gate) { "das" } else { "die" };
    format!("Die Feinde haben {article} {name} durchbrochen!")
}

/// „Räuber plündern das Gebäude „Rathaus“: 12 Holz und 30 Gold.“ / „Das Heer von Weidenthal
/// plündert das Gebäude „Lagerhaus“, findet aber nichts.“: the plunder of a building by bandits
/// (`army` `None`) or by the army of the named kingdom, with the verbs agreeing with the subject
/// and the building named like the other notices („das Gebäude „…““, as building names carry no
/// grammatical gender in the data).
pub(super) fn plunder_text(army: Option<&str>, name: &str, loot: &str) -> String {
    let (who, plunders, finds) = match army {
        None => ("Räuber".to_owned(), "plündern", "finden"),
        Some(kingdom) => (format!("Das Heer von {kingdom}"), "plündert", "findet"),
    };
    if loot.is_empty() {
        format!("{who} {plunders} das Gebäude „{name}“, {finds} aber nichts.")
    } else {
        format!("{who} {plunders} das Gebäude „{name}“: {loot}.")
    }
}

/// „Die Plünderer haben das Gebäude „Rathaus“ angezündet!“
pub(super) fn arson_text(name: &str) -> String {
    format!("Die Plünderer haben das Gebäude „{name}“ angezündet!")
}

fn kingdom_name(state: &GameState, id: KingdomId) -> String {
    state.diplomacy.kingdom(id).map_or_else(|| "?".to_owned(), |k| k.name.clone())
}

fn goods_text(defs: &Defs, goods: &[ResourceAmount]) -> String {
    goods.iter().map(|m| format!("{} {}", m.amount, defs.resource(m.resource).name)).collect::<Vec<_>>().join(", ")
}

/// A band of bandits gathers: `percent` of the size the town draws now ([`bandit_band`]; event
/// effect `Raiders`). The scouts reported it at `reported_tick` (the start of the raid event):
/// it reaches the map edge `warning_hours` after that, but at least `delay_hours` from now.
/// Returns the band's size (0 if no map edge can be reached) and its arrival tick.
pub(crate) fn spawn_bandits(state: &mut GameState, defs: &Defs, percent: u8, reported_tick: u64, events: &mut Vec<SimEvent>) -> (u16, u64) {
    let size = u16::try_from(u32::from(bandit_band(state, defs)) * u32::from(percent) / 100).unwrap_or(u16::MAX).max(1);
    let arrive = bandit_arrival(state, defs, reported_tick);
    let delay = arrive - state.clock.tick;
    (spawn_band(state, defs, size, delay, events), arrive)
}

/// Tick a band reported at `reported_tick` reaches the map edge if it sets out now:
/// `warning_hours` after the report, at least `delay_hours` from now.
pub fn bandit_arrival(state: &GameState, defs: &Defs, reported_tick: u64) -> u64 {
    let rules = &defs.rules().military.bandits;
    let now = state.clock.tick;
    (reported_tick + u64::from(rules.warning_hours) * TICKS_PER_HOUR).max(now + u64::from(rules.delay_hours) * TICKS_PER_HOUR)
}

/// True once the bandits' grace period is over (docs/game_design.md §11): from day
/// `bandits.grace_days` of the difficulty (0-based) and with at least
/// `bandits.grace_population` inhabitants.
pub fn raid_grace_over(state: &GameState, defs: &Defs) -> bool {
    let rules = &defs.rules().military.bandits;
    state.clock.total_days() >= u64::from(rules.grace_days.get(state.settings.difficulty))
        && state.population.len() >= usize::from(rules.grace_population)
}

/// Finished, working buildings with a ranged defence (towers, archer posts, the castle) by
/// type, in type order.
pub fn ranged_defences(state: &GameState, defs: &Defs) -> Vec<(BuildingTypeId, u32)> {
    let mut counts: BTreeMap<BuildingTypeId, u32> = BTreeMap::new();
    for (_, b) in state.buildings.iter().filter(|(_, b)| b.is_operational()) {
        if defs.buildings().get(b.kind().index()).and_then(|d| d.defense.as_ref()).is_some_and(|d| d.ranged > 0) {
            *counts.entry(b.kind()).or_insert(0) += 1;
        }
    }
    counts.into_iter().collect()
}

/// Defenders that deter bandits: ready soldiers on the map and the buildings with a ranged
/// defence ([`ranged_defences`]).
pub fn raid_defenders(state: &GameState, defs: &Defs) -> u32 {
    let towers: u32 = ranged_defences(state, defs).iter().map(|(_, n)| n).sum();
    state.military.available_count().saturating_add(towers)
}

/// Size of a band of bandits the town draws now (docs/game_design.md §11): `min` + one per
/// `citizens_per_bandit` citizens (at most `max`) × `size_percent` of the difficulty
/// (rounded), then one bandit fewer per `defenders_per_bandit` defenders ([`raid_defenders`]),
/// at most `max_deterrence_percent` of the band fewer; at least one.
pub fn bandit_band(state: &GameState, defs: &Defs) -> u16 {
    let rules = &defs.rules().military.bandits;
    let population = u32::try_from(state.population.len()).unwrap_or(u32::MAX);
    let normal = (u32::from(rules.min) + population / u32::from(rules.citizens_per_bandit.max(1))).min(u32::from(rules.max));
    let scaled = (normal * u32::from(rules.size_percent.get(state.settings.difficulty)) + 50) / 100;
    let deterrence =
        (raid_defenders(state, defs) / u32::from(rules.defenders_per_bandit.max(1))).min(scaled * u32::from(rules.max_deterrence_percent) / 100);
    u16::try_from((scaled - deterrence).max(1)).unwrap_or(u16::MAX)
}

/// A band of `size` bandits that arrives after `delay` ticks at a random point of the map edge.
pub(crate) fn spawn_band(state: &mut GameState, defs: &Defs, size: u16, delay: u64, events: &mut Vec<SimEvent>) -> u16 {
    let (w, h) = (state.world.width(), state.world.height());
    // A random point of the map edge; they enter at the reachable edge tile towards it.
    let side = state.military.rng.random_range(0..4u8);
    let along = state.military.rng.random_range(0..w.max(h));
    let toward = match side {
        0 => IVec2::new(along.min(w - 1), 0),
        1 => IVec2::new(w - 1, along.min(h - 1)),
        2 => IVec2::new(along.min(w - 1), h - 1),
        _ => IVec2::new(0, along.min(h - 1)),
    };
    let Some(entry) = entry_edge(state, defs, toward) else { return 0 };
    let arrive_tick = state.clock.tick + delay;
    let id = new_warband(state, WarbandKind::Bandits, entry, vec![(FighterKind::Bandit, size)], arrive_tick);
    events.push(SimEvent::ThreatApproaching { warband: id, kingdom: None, size, arrive_tick });
    let text = format!(
        "Späher melden eine Räuberbande von {} am Rand des Reiches. Sie greift in etwa {} an (gegen {}).",
        count_text(u32::from(size), "Mann", "Mann"),
        hours_text(delay.div_ceil(TICKS_PER_HOUR).max(1)),
        crate::report::deadline_clock_text(state.clock.tick, arrive_tick)
    );
    events.push(SimEvent::Notification(Notification::new(Severity::Danger, text).with_kind(NotificationKind::Threat).at(entry)));
    size
}

/// „1 h“, „3 h“, „1 Tag 6 h“ (the duration form of the game, [`crate::report::hours_text`]).
pub(crate) fn hours_text(hours: u64) -> String {
    crate::report::hours_text(hours)
}

fn new_warband(state: &mut GameState, kind: WarbandKind, entry: IVec2, planned: Vec<(FighterKind, u16)>, arrive_tick: u64) -> u32 {
    let m = &mut state.military;
    let id = m.next_warband;
    m.next_warband = m.next_warband.wrapping_add(1).max(1);
    let size = planned.iter().map(|(_, n)| *n).sum();
    m.warbands.push(Warband {
        id,
        kind,
        stage: WarbandStage::Approaching { arrive_tick },
        entry,
        planned,
        fighters: Vec::new(),
        target: None,
        size,
        escaped: 0,
        plundered: false,
        plunder_rounds: 0,
        hold_ticks: 0,
        stuck_ticks: 0,
        breach: None,
        defenders: Vec::new(),
        started_tick: state.clock.tick,
        tally: Tally::default(),
    });
    id
}

/// Kingdom `id` (at war with the player) sends an army if it has enough soldiers; returns true
/// if one set out.
pub(crate) fn dispatch_army(state: &mut GameState, defs: &Defs, id: KingdomId, events: &mut Vec<SimEvent>) -> bool {
    let rules = &defs.rules().military.armies;
    let Some(k) = state.diplomacy.kingdom(id) else { return false };
    if k.soldiers < u32::from(rules.min_soldiers) || defs.army_mix().is_empty() {
        return false;
    }
    let size = (k.soldiers * u32::from(rules.share_percent) / 100).clamp(1, u32::from(rules.max_size));
    let arrive_tick = state.clock.tick + crate::diplomacy::caravans::travel_ticks(state, defs, id);
    send_army(state, defs, id, size, arrive_tick, events)
}

/// Kingdom `id` sends `size` of its soldiers (at most as many as it has), arriving at the map
/// edge at `arrive_tick`; returns true if they set out.
pub(crate) fn send_army(state: &mut GameState, defs: &Defs, id: KingdomId, size: u32, arrive_tick: u64, events: &mut Vec<SimEvent>) -> bool {
    let rules = &defs.rules().military.armies;
    let Some(k) = state.diplomacy.kingdom(id) else { return false };
    let size = size.min(k.soldiers);
    if size == 0 || defs.army_mix().is_empty() {
        return false;
    }
    let seat = k.seat;
    let conquest = defs.rules().diplomacy.personalities.get(k.personality).aggression >= rules.conquest_aggression;
    let Some(entry) = entry_edge(state, defs, seat) else { return false };
    // Largest remainder over the weights (ties: order of the mix).
    let total: u32 = defs.army_mix().iter().map(|(_, w)| u32::from(*w)).sum::<u32>().max(1);
    let mut planned: Vec<(FighterKind, u16, u32)> = defs
        .army_mix()
        .iter()
        .map(|(u, w)| {
            let exact = size * u32::from(*w);
            (FighterKind::Unit(*u), u16::try_from(exact / total).unwrap_or(u16::MAX), exact % total)
        })
        .collect();
    let mut left = size - planned.iter().map(|(_, n, _)| u32::from(*n)).sum::<u32>();
    let mut order: Vec<usize> = (0..planned.len()).collect();
    order.sort_by_key(|i| std::cmp::Reverse(planned[*i].2));
    for i in order {
        if left == 0 {
            break;
        }
        planned[i].1 += 1;
        left -= 1;
    }
    let mut planned: Vec<(FighterKind, u16)> = planned.into_iter().filter(|(_, n, _)| *n > 0).map(|(k, n, _)| (k, n)).collect();
    planned.sort();
    if let Some(k) = state.diplomacy.kingdom_mut(id) {
        k.soldiers -= size;
    }
    let wid = new_warband(state, WarbandKind::Army { kingdom: id, conquest }, entry, planned, arrive_tick);
    let size = u16::try_from(size).unwrap_or(u16::MAX);
    events.push(SimEvent::ThreatApproaching { warband: wid, kingdom: Some(id), size, arrive_tick });
    let name = kingdom_name(state, id);
    let goal = if conquest { "auf Euer Hauptquartier" } else { "auf Eure Lager" };
    let hours = (arrive_tick - state.clock.tick).div_ceil(TICKS_PER_HOUR);
    let text = format!(
        "Ein Heer von {name} mit {} marschiert {goal} zu. Ankunft in etwa {}.",
        count_text(u32::from(size), "Soldat", "Soldaten"),
        hours_text(hours)
    );
    state.diplomacy.record(state.clock.tick, Some(id), None, text.clone());
    events.push(SimEvent::Notification(Notification::new(Severity::Danger, text).with_kind(NotificationKind::Threat).at(entry)));
    true
}

/// Map-edge tile where enemies coming from the direction of `toward` enter: the edge tile
/// nearest to that direction from which they can walk to the headquarters (the start without
/// one); if the town is walled in or cut off, the nearest one from which they reach a wall or
/// gate to break through (§11); else the nearest walkable one (`None`: no walkable edge tile).
fn entry_edge(state: &mut GameState, defs: &Defs, toward: IVec2) -> Option<IVec2> {
    crate::navigation::prepare(state, defs);
    let hq = storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b)).map(|b| (b.rect(), b.entrance()));
    let from = hq.map_or(state.world.start, |(_, entrance)| entrance);
    let edge = crate::diplomacy::caravans::edge_tiles_towards(state, from, toward);
    let around: Vec<IVec2> = match hq {
        Some((rect, entrance)) => std::iter::once(entrance).chain(rect.expanded(1).tiles().filter(|p| !rect.contains(*p))).collect(),
        None => vec![from],
    };
    let nav = &state.navigation;
    let home: BTreeSet<u32> = around.iter().filter_map(|p| nav.component(*p)).collect();
    let breachable: BTreeSet<u32> = state
        .buildings
        .iter()
        .filter(|(_, b)| b.is_complete() && crate::buildings::is_wall_or_gate(defs, b))
        .flat_map(|(_, b)| crate::grid::Direction::ALL.map(|d| b.rect().origin + d.offset()))
        .filter_map(|p| nav.component(p))
        .collect();
    let in_any = |set: &BTreeSet<u32>| edge.iter().copied().find(|p| nav.component(*p).is_some_and(|c| set.contains(&c)));
    in_any(&home).or_else(|| in_any(&breachable)).or_else(|| edge.iter().copied().find(|p| nav.is_walkable(*p)))
}

/// Buildings a group would go for, in order of preference: aggressive armies the headquarters;
/// other armies the storages, richest first; bandits the nearest storage without safety
/// coverage with goods, then any unguarded storage, then an unguarded building, then any
/// storage (nearest to their entry first).
fn target_candidates(state: &GameState, defs: &Defs, w: &Warband) -> Vec<BuildingId> {
    match w.kind {
        WarbandKind::Army { conquest: true, .. } => storage::main_headquarters(state, defs).into_iter().collect(),
        WarbandKind::Army { conquest: false, .. } => richest_storages(state, defs),
        WarbandKind::Bandits => {
            let index = ServiceIndex::collect(state, defs);
            let unguarded = |b: &crate::buildings::Building| !index.covers(ServiceKind::Safety, &b.rect());
            let loot = |b: &crate::buildings::Building| {
                b.inventory().iter().any(|m| {
                    matches!(defs.resource(m.resource).category, ResourceCategory::Food | ResourceCategory::Goods | ResourceCategory::Luxury)
                })
            };
            let tiers: [&dyn Fn(&crate::buildings::Building) -> bool; 4] = [
                &|b| storage::is_operational_storage(defs, b) && unguarded(b) && loot(b),
                &|b| storage::is_operational_storage(defs, b) && unguarded(b),
                &|b| {
                    matches!(b.phase(), crate::buildings::BuildingPhase::Active)
                        && unguarded(b)
                        && defs.buildings().get(b.kind().index()).is_some_and(crate::buildings::can_be_damaged)
                },
                &|b| storage::is_operational_storage(defs, b),
            ];
            let mut out: Vec<BuildingId> = Vec::new();
            for tier in tiers {
                let mut ids: Vec<(i32, BuildingId)> =
                    state.buildings.iter().filter(|(_, b)| tier(b)).map(|(id, b)| (b.rect().distance_to(w.entry), id)).collect();
                ids.sort();
                out.extend(ids.into_iter().map(|(_, id)| id).filter(|id| !out.contains(id)).collect::<Vec<_>>());
            }
            out
        }
    }
}

/// Operational storages, richest first (ties: lower id).
fn richest_storages(state: &GameState, defs: &Defs) -> Vec<BuildingId> {
    let mut ids: Vec<(std::cmp::Reverse<u64>, BuildingId)> = state
        .buildings
        .iter()
        .filter(|(_, b)| storage::is_operational_storage(defs, b))
        .map(|(id, b)| (std::cmp::Reverse(b.inventory().total()), id))
        .collect();
    ids.sort();
    ids.into_iter().map(|(_, id)| id).collect()
}

/// The building a group at `from` goes for: the first candidate (see [`target_candidates`]) it
/// can walk to, else the first one (it breaks through walls to get there).
fn choose_target(state: &mut GameState, defs: &Defs, w: &Warband, from: IVec2) -> Option<BuildingId> {
    let candidates = target_candidates(state, defs, w);
    crate::navigation::prepare(state, defs);
    candidates.iter().copied().find(|b| approach(state, from, *b).is_some()).or_else(|| candidates.first().copied())
}

/// A target a group at `from` can walk to after it found no way to its target `lost`: the next
/// reachable candidate; aggressive armies that cannot get at the headquarters plunder the
/// richest storage they reach instead. `None`: nothing within reach.
fn reachable_target(state: &mut GameState, defs: &Defs, w: &Warband, from: IVec2, lost: Option<BuildingId>) -> Option<BuildingId> {
    let candidates = match w.kind {
        WarbandKind::Army { conquest: true, .. } => richest_storages(state, defs),
        _ => target_candidates(state, defs, w),
    };
    candidates.into_iter().filter(|b| Some(*b) != lost).find(|b| approach(state, from, *b).is_some())
}

/// Tile from which a fighter at `from` gets at building `b`: its entrance, else the free
/// walkable tile next to it nearest to the entrance, that the fighter can walk to (gates count
/// as passable: enemies break them on the way). `None`: no way there.
fn approach(state: &mut GameState, from: IVec2, b: BuildingId) -> Option<IVec2> {
    let (rect, entrance) = state.buildings.get(b).map(|bb| (bb.rect(), bb.entrance()))?;
    let mut tiles: Vec<IVec2> = rect.expanded(1).tiles().filter(|p| !rect.contains(*p)).collect();
    if !tiles.contains(&entrance) {
        tiles.push(entrance);
    }
    tiles.sort_by_key(|p| (tile_distance(*p, entrance), p.y, p.x));
    let buildings = &state.buildings;
    let nav = &mut state.navigation;
    tiles.into_iter().find(|p| buildings.at(*p).is_none() && nav.is_walkable(*p) && nav.reachable(from, *p))
}

/// Groups whose arrival time has come walk onto the map at their entry tile.
pub(crate) fn run_arrivals(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let now = state.clock.tick;
    let due: Vec<usize> = state
        .military
        .warbands
        .iter()
        .enumerate()
        .filter(|(_, w)| matches!(w.stage, WarbandStage::Approaching { arrive_tick } if arrive_tick <= now))
        .map(|(i, _)| i)
        .collect();
    for wi in due {
        let (id, entry, planned) = {
            let w = &state.military.warbands[wi];
            (w.id, w.entry, w.planned.clone())
        };
        crate::navigation::prepare(state, defs);
        let entry = if state.navigation.is_walkable(entry) {
            entry
        } else {
            // Something was built on the entry tile meanwhile: the nearest suitable edge tile.
            entry_edge(state, defs, entry).unwrap_or(entry)
        };
        let mut fighters = Vec::new();
        for (kind, n) in planned {
            let stats = fighter_stats(state, defs, kind, false);
            let speed = match kind {
                FighterKind::Bandit => defs.rules().military.bandits.speed_permille,
                FighterKind::Unit(u) => defs.units().get(u.index()).map_or(1000, |d| d.speed_permille),
            };
            let role = match kind {
                FighterKind::Bandit => crate::population::CitizenRole::Raider,
                FighterKind::Unit(u) => super::unit_role(defs, u),
            };
            for _ in 0..n {
                let traveler = Traveler { motion: Motion::standing(entry), role, speed_permille: speed, caravan: None, warband: Some(id) };
                let walker = state.population.walking.travelers.insert(traveler);
                movement::index_insert(state, WalkerId::Traveler(walker));
                fighters.push(Fighter {
                    walker,
                    kind,
                    hp: stats.max_hp,
                    attacking: None,
                    engage: None,
                    loot: Vec::new(),
                    gold: 0,
                });
            }
        }
        let w = &mut state.military.warbands[wi];
        w.entry = entry;
        w.size = u16::try_from(fighters.len()).unwrap_or(u16::MAX);
        w.fighters = fighters;
        w.stage = WarbandStage::Attacking;
        let snapshot = w.clone();
        let target = choose_target(state, defs, &snapshot, entry);
        state.military.warbands[wi].target = target;
        let (who, their) = match snapshot.kind {
            WarbandKind::Bandits => ("Räuber überschreiten".to_owned(), "Ihr"),
            WarbandKind::Army { kingdom, .. } => (format!("Das Heer von {} überschreitet", kingdom_name(state, kingdom)), "Sein"),
        };
        let text = match target {
            Some(t) => format!("{who} die Grenze! {their} Ziel: {}.", building_name(state, defs, t)),
            None => format!("{who} die Grenze!"),
        };
        events.push(SimEvent::ThreatArrived { warband: id, size: snapshot.size });
        events.push(SimEvent::Notification(Notification::new(Severity::Danger, text).with_kind(NotificationKind::Threat).at(entry)));
    }
}

/// Removes enemies whose walker left the map (fleeing to the edge): bandits keep their plunder
/// (it is lost), survivors of an army return to their kingdom with their plunder.
pub(crate) fn prune(state: &mut GameState, _defs: &Defs, _events: &mut Vec<SimEvent>) {
    for wi in 0..state.military.warbands.len() {
        let gone: Vec<Fighter> = {
            let travelers = &state.population.walking.travelers;
            let w = &mut state.military.warbands[wi];
            let (gone, stay): (Vec<Fighter>, Vec<Fighter>) = std::mem::take(&mut w.fighters).into_iter().partition(|f| !travelers.contains(f.walker));
            w.fighters = stay;
            w.escaped = w.escaped.saturating_add(u16::try_from(gone.len()).unwrap_or(u16::MAX));
            gone
        };
        if gone.is_empty() {
            continue;
        }
        if let Some(kingdom) = state.military.warbands[wi].kingdom()
            && let Some(k) = state.diplomacy.kingdom_mut(kingdom)
        {
            for f in &gone {
                k.soldiers += 1;
                k.treasury_milli += i64::from(f.gold) * MILLI_PER_GOLD;
                for m in &f.loot {
                    crate::diplomacy::economy::give(k, m.resource, m.amount);
                }
            }
        }
    }
}

/// Damage a structure can take (walls and gates raised by researched `WallStrengthBonus`).
pub(crate) fn structure_hp(state: &GameState, defs: &Defs, id: BuildingId) -> u32 {
    let Some(b) = state.buildings.get(id) else { return 1 };
    let Some(def) = defs.buildings().get(b.kind().index()) else { return 1 };
    let base = def.defense.map_or_else(
        || defs.rules().military.building_hp_per_tile.saturating_mul(u32::try_from(b.rect().area()).unwrap_or(1)),
        |d| d.hit_points,
    );
    if crate::buildings::is_wall_or_gate(defs, b) {
        let bonus = state.research.wall_strength_percent(defs).max(-90);
        u32::try_from(i64::from(base) * i64::from(100 + bonus) / 100).unwrap_or(base).max(1)
    } else {
        base.max(1)
    }
}

/// Adds damage to a structure under attack: a broken wall or gate vanishes, a broken building
/// becomes a ruin; the headquarters stays broken until repaired (see the conquest rule).
pub(crate) fn damage_structure(state: &mut GameState, defs: &Defs, id: BuildingId, damage: u32, events: &mut Vec<SimEvent>) {
    let hp = structure_hp(state, defs, id);
    let Some(b) = state.buildings.get(id) else { return };
    let at = b.entrance();
    let role = defs.buildings().get(b.kind().index()).map(|d| d.role);
    let wall = crate::buildings::is_wall_or_gate(defs, b);
    let before = state.military.damage_of(id);
    let after = before.saturating_add(damage).min(hp);
    state.military.damage.insert(id, after);
    if after < hp || before >= hp {
        return;
    }
    let name = building_name(state, defs, id);
    let attacker = state.military.warbands.iter().position(|w| w.fighters.iter().any(|f| f.attacking == Some(id)));
    if let Some(wi) = attacker {
        state.military.warbands[wi].tally.buildings_damaged += 1;
    }
    if role == Some(BuildingRole::Headquarters) {
        let text = format!("Das Hauptquartier „{name}“ ist gestürmt! Haltet Soldaten in der Nähe, sonst geht das Reich verloren.");
        events.push(SimEvent::Notification(Notification::new(Severity::Danger, text).with_kind(NotificationKind::Threat).at(at).for_building(id)));
        return;
    }
    state.military.damage.remove(&id);
    if wall {
        crate::buildings::destroy_structure(state, defs, id, events);
        events.push(SimEvent::Notification(Notification::new(Severity::Danger, breach_text(role, &name)).with_kind(NotificationKind::Threat).at(at)));
    } else if crate::buildings::damage_building(state, defs, id, DamageCause::Attack, events).is_some() {
        let text = format!("Das Gebäude „{name}“ ist zerstört und muss wiederaufgebaut werden.");
        events.push(SimEvent::Notification(Notification::new(Severity::Danger, text).with_kind(NotificationKind::Threat).at(at).for_building(id)));
    }
    for w in &mut state.military.warbands {
        for f in &mut w.fighters {
            if f.attacking == Some(id) {
                f.attacking = None;
            }
        }
        if w.breach.is_some_and(|(b, _)| b == id) {
            w.breach = None;
        }
    }
}

/// An enemy falls: its plunder drops back into the kingdom's storages, its walker vanishes.
pub(crate) fn fighter_falls(state: &mut GameState, defs: &Defs, walker: TravelerId, events: &mut Vec<SimEvent>) {
    let Some((wi, fi)) = state.military.fighter_index(walker) else { return };
    let f = state.military.warbands[wi].fighters.remove(fi);
    let at = state.population.walking.traveler(walker).map_or(state.world.start, |t| t.motion().pos().tile());
    let recovered: u32 = f.loot.iter().map(|m| m.amount).sum();
    if !f.loot.is_empty() {
        storage::deposit_or_lose(state, defs, &f.loot, at, events);
    }
    if f.gold > 0 {
        state.economy.book_gold(LedgerCategory::Events, i64::from(f.gold));
    }
    movement::forget(state, WalkerId::Traveler(walker));
    state.population.walking.travelers.remove(walker);
    let w = &mut state.military.warbands[wi];
    w.tally.enemy_losses += 1;
    w.tally.recovered += recovered + f.gold;
    if let Some(k) = w.kingdom() {
        events.push(SimEvent::EnemyFell { warband: w.id, kingdom: Some(k) });
    } else {
        events.push(SimEvent::EnemyFell { warband: w.id, kingdom: None });
    }
}

/// Plunder of the group at its target storage (or building): goods and gold are split among
/// its fighters, the place may catch fire, then the group withdraws.
fn plunder(state: &mut GameState, defs: &Defs, wi: usize, events: &mut Vec<SimEvent>) {
    let Some(target) = state.military.warbands[wi].target else { return };
    let kind = state.military.warbands[wi].kind;
    let alive = state.military.warbands[wi].fighters.len();
    if alive == 0 {
        return;
    }
    let rules = &defs.rules().military;
    let plunderable = |defs: &Defs, r: crate::defs::ResourceId| {
        matches!(defs.resource(r).category, ResourceCategory::Food | ResourceCategory::Goods | ResourceCategory::Luxury)
    };
    let mut taken: Vec<ResourceAmount> = Vec::new();
    let mut gold = 0u32;
    if let Some(b) = state.buildings.get_mut(target) {
        let mut goods: Vec<ResourceAmount> = b.inventory().iter().filter(|m| plunderable(defs, m.resource)).collect();
        goods.sort_by_key(|m| (std::cmp::Reverse(m.amount), m.resource));
        match kind {
            WarbandKind::Bandits => {
                let mut room = u32::from(rules.bandits.steal_units) * u32::try_from(alive).unwrap_or(u32::MAX);
                for m in goods {
                    if room == 0 {
                        break;
                    }
                    let got = b.inventory.remove(m.resource, m.amount.min(room));
                    room -= got;
                    if got > 0 {
                        taken.push(ResourceAmount { resource: m.resource, amount: got });
                    }
                }
            }
            WarbandKind::Army { .. } => {
                let percent = u32::from(defs.rules().diplomacy.war.raid_percent);
                for m in goods {
                    let got = b.inventory.remove(m.resource, m.amount * percent / 100);
                    if got > 0 {
                        taken.push(ResourceAmount { resource: m.resource, amount: got });
                    }
                }
            }
        }
    }
    if kind == WarbandKind::Bandits {
        let want = u32::from(rules.bandits.steal_gold) * u32::try_from(alive).unwrap_or(u32::MAX);
        gold = u32::try_from(i64::from(want).min(state.economy.treasury().max(0))).unwrap_or(0);
        if gold > 0 {
            state.economy.book_gold(LedgerCategory::Events, -i64::from(gold));
        }
    }
    taken.sort_by_key(|m| m.resource);
    storage::refresh_stock(state, defs);
    // Split among the fighters (the first ones carry the remainders).
    let n = u32::try_from(alive).unwrap_or(1);
    {
        let w = &mut state.military.warbands[wi];
        for (i, f) in w.fighters.iter_mut().enumerate() {
            let i = u32::try_from(i).unwrap_or(0);
            let share = |amount: u32| amount / n + u32::from(i < amount % n);
            for m in &taken {
                let a = share(m.amount);
                if a > 0 {
                    f.loot.push(ResourceAmount { resource: m.resource, amount: a });
                }
            }
            f.gold += share(gold);
        }
        w.plundered = true;
        w.stage = WarbandStage::Fleeing;
        // Plunder makes them slow.
        let loaded = u32::from(rules.bandits.loaded_speed_permille);
        for f in &w.fighters {
            if (!f.loot.is_empty() || f.gold > 0)
                && let Some(t) = state.population.walking.travelers.get_mut(f.walker)
            {
                t.speed_permille = u16::try_from((u32::from(t.speed_permille) * loaded / 1000).max(1)).unwrap_or(1);
            }
        }
        w.tally.goods_lost.extend(taken.iter().copied());
        w.tally.goods_lost = crate::buildings::merge_amounts(std::mem::take(&mut w.tally.goods_lost));
        w.tally.gold_lost += gold;
    }
    let name = building_name(state, defs, target);
    let at = state.buildings.get(target).map_or(state.world.start, |b| b.entrance());
    let army = match kind {
        WarbandKind::Bandits => None,
        WarbandKind::Army { kingdom, .. } => Some(kingdom_name(state, kingdom)),
    };
    let mut loot = goods_text(defs, &taken);
    if gold > 0 {
        if !loot.is_empty() {
            loot.push_str(" und ");
        }
        loot.push_str(&format!("{gold} Gold"));
    }
    let text = plunder_text(army.as_deref(), &name, &loot);
    if !taken.is_empty() {
        events.push(SimEvent::StockLost { lost: taken.clone(), buildings: vec![target], position: Some(at) });
    }
    events.push(SimEvent::Notification(Notification::new(Severity::Danger, text).with_kind(NotificationKind::Threat).at(at).for_building(target)));
    // Arson.
    let fire = state.military.rng.random_range(0..100u8) < rules.bandits.fire_percent;
    if fire && crate::buildings::damage_building(state, defs, target, DamageCause::Fire, events).is_some() {
        state.military.warbands[wi].tally.buildings_damaged += 1;
        events.push(SimEvent::Notification(Notification::new(Severity::Danger, arson_text(&name)).with_kind(NotificationKind::Threat).at(at).for_building(target)));
    }
}

/// Tile a walker stands on or is stepping onto.
pub(crate) fn standing_tile(m: &Motion) -> IVec2 {
    match (m.phase(), m.path.get(usize::from(m.next))) {
        (TripPhase::Walking, Some(d)) => m.step_origin() + d.offset(),
        _ => m.pos().tile(),
    }
}

/// The wall or gate a group breaks through when no path leads to its target: the one nearest
/// to the target with a walkable neighbour the fighter can reach, and that neighbour.
fn find_breach(state: &mut GameState, defs: &Defs, from: IVec2, goal: IVec2) -> Option<(BuildingId, IVec2)> {
    let walls: Vec<(BuildingId, IVec2)> = state
        .buildings
        .iter()
        .filter(|(_, b)| b.is_complete() && crate::buildings::is_wall_or_gate(defs, b))
        .map(|(id, b)| (id, b.rect().origin))
        .collect();
    let mut best: Option<(i32, i32, BuildingId, IVec2)> = None;
    for (id, p) in walls {
        for d in crate::grid::Direction::ALL {
            let n = p + d.offset();
            if !state.navigation.is_walkable(n) || !state.navigation.reachable(from, n) {
                continue;
            }
            let key = (tile_distance(p, goal), tile_distance(n, from));
            if best.is_none_or(|(a, b, ..)| key < (a, b)) {
                best = Some((key.0, key.1, id, n));
            }
        }
    }
    best.map(|(_, _, id, n)| (id, n))
}

/// Decisions of the enemies on the map (every combat round): flee, plunder, hold the
/// headquarters, fight own soldiers, break gates and walls, march on, give up.
pub(crate) fn decide(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().military;
    let round = u32::from(rules.round_ticks.max(1));
    // The reachability checks below need the walking grid as it is now.
    crate::navigation::prepare(state, defs);
    let soldiers: Vec<(crate::population::CitizenId, IVec2)> = state
        .military
        .soldiers
        .iter()
        .filter(|(_, s)| s.is_available() && s.hp > 0)
        .filter_map(|(id, _)| {
            let c = state.population.citizen(*id)?;
            c.motion().inside().is_none().then(|| (*id, c.motion().pos().tile()))
        })
        .collect();
    for wi in 0..state.military.warbands.len() {
        let w = &state.military.warbands[wi];
        if matches!(w.stage, WarbandStage::Approaching { .. }) || w.fighters.is_empty() {
            continue;
        }
        let (kind, size, alive) = (w.kind, u32::from(w.size.max(1)), u32::try_from(w.fighters.len()).unwrap_or(0));
        // Retreat after heavy losses.
        let flee_percent = match kind {
            WarbandKind::Bandits => u32::from(rules.bandits.flee_losses_percent),
            WarbandKind::Army { .. } => u32::from(rules.armies.retreat_losses_percent),
        };
        if w.stage == WarbandStage::Attacking && (size - alive.min(size)) * 100 >= size * flee_percent {
            state.military.warbands[wi].stage = WarbandStage::Fleeing;
            let text = match kind {
                WarbandKind::Bandits => "Die Räuber fliehen!".to_owned(),
                WarbandKind::Army { kingdom, .. } => format!("Das Heer von {} zieht sich zurück!", kingdom_name(state, kingdom)),
            };
            let at = state.military.warbands[wi].entry;
            events.push(SimEvent::Notification(Notification::new(Severity::Good, text).with_kind(NotificationKind::Threat).at(at)));
        }
        // The target may be gone (demolished, burned): choose again.
        if state.military.warbands[wi].stage == WarbandStage::Attacking
            && state.military.warbands[wi].target.is_none_or(|t| state.buildings.get(t).is_none())
        {
            let snapshot = state.military.warbands[wi].clone();
            let from = first_fighter_tile(state, &snapshot);
            state.military.warbands[wi].target = choose_target(state, defs, &snapshot, from);
            state.military.warbands[wi].breach = None;
        }
        let stage = state.military.warbands[wi].stage;
        let target = state.military.warbands[wi].target;
        let target_rect = target.and_then(|t| state.buildings.get(t)).map(|b| (b.rect(), b.entrance()));
        // Aggressive armies storm the headquarters (others plunder their target).
        let storming = matches!(kind, WarbandKind::Army { conquest: true, .. })
            && target.is_some()
            && target == storage::main_headquarters(state, defs);
        // Conquest: the broken headquarters is held without defenders nearby.
        if let (WarbandKind::Army { kingdom, .. }, Some(hq), true) = (kind, target, storming)
            && stage == WarbandStage::Attacking
        {
            let broken = state.military.damage_of(hq) >= structure_hp(state, defs, hq);
            let rect = target_rect.map(|(r, _)| r);
            let defended = rect.is_some_and(|r| {
                state.military.soldiers.iter().any(|(id, s)| {
                    s.is_available()
                        && state.population.citizen(*id).is_some_and(|c| {
                            let p = c.motion().inside().and_then(|b| state.buildings.get(b)).map_or(c.motion().pos().tile(), |b| b.entrance());
                            r.distance_to(p) <= i32::from(rules.hq_defense_radius)
                        })
                })
            });
            let at_hq = rect.is_some_and(|r| {
                state.military.warbands[wi].fighters.iter().any(|f| {
                    state.population.walking.traveler(f.walker).is_some_and(|t| r.distance_to(t.motion().pos().tile()) <= 1)
                })
            });
            let w = &mut state.military.warbands[wi];
            if broken && !defended && at_hq {
                w.hold_ticks += round;
                if u64::from(w.hold_ticks) >= u64::from(rules.hq_capture_hours) * TICKS_PER_HOUR {
                    crate::ranks::defeat(state, defs, crate::ranks::DefeatReason::Conquered { kingdom }, events);
                    return;
                }
            } else {
                w.hold_ticks = 0;
            }
        }
        // Each fighter.
        let reach = match kind {
            WarbandKind::Bandits => 1,
            WarbandKind::Army { .. } => i32::from(rules.armies.aggro_radius),
        };
        let mut plunder_now = false;
        // A fighter (at this tile) found no way to the target and no wall to break.
        let mut lost_way: Option<IVec2> = None;
        // Withdrawing fighters without any way off the map.
        let mut shut_in = 0usize;
        for fi in 0..state.military.warbands[wi].fighters.len() {
            let (walker, fkind, attacking) = {
                let f = &state.military.warbands[wi].fighters[fi];
                (f.walker, f.kind, f.attacking)
            };
            let Some(t) = state.population.walking.traveler(walker) else { continue };
            let m = t.motion().clone();
            let here = m.pos().tile();
            let id = WalkerId::Traveler(walker);
            // Stopped in front of a gate on its way (gates let only the player's people through).
            let gate_ahead = if m.phase() == TripPhase::Still && m.destination().is_some() && !m.is_at_destination() {
                state.buildings.at(here + m.facing().offset()).filter(|(_, b)| crate::buildings::is_gate(defs, b)).map(|(g, _)| g)
            } else {
                None
            };
            // The structure it is attacking, while it stands next to it.
            let breaking = attacking.filter(|b| state.buildings.get(*b).is_some_and(|bb| bb.rect().distance_to(here) <= 1));
            let breaking_wall = breaking.filter(|b| state.buildings.get(*b).is_some_and(|bb| crate::buildings::is_wall_or_gate(defs, bb)));
            if matches!(stage, WarbandStage::Fleeing | WarbandStage::Withdrawing) {
                // Fleeing enemies break the gates and walls in their way; an army withdrawing
                // after a peace passes the gates and fights no more.
                state.military.warbands[wi].fighters[fi].engage = None;
                if stage == WarbandStage::Fleeing
                    && let Some(b) = breaking_wall.or(gate_ahead)
                {
                    state.military.warbands[wi].fighters[fi].attacking = Some(b);
                    if m.destination().is_some() {
                        movement::set_destination(state, id, None);
                    }
                    continue;
                }
                state.military.warbands[wi].fighters[fi].attacking = None;
                if !leave_map(state, defs, wi, fi, here, stage == WarbandStage::Fleeing) {
                    shut_in += 1;
                }
                continue;
            }
            // Own soldiers within reach that it can get at: next to it, within range of its
            // bows, or on a way it can walk (a gate on that way is broken first).
            let stats = fighter_stats(state, defs, fkind, false);
            let mut near: Vec<(i32, crate::population::CitizenId, IVec2)> = soldiers
                .iter()
                .map(|(sid, p)| (tile_distance(*p, here), *sid, *p))
                .filter(|(d, ..)| *d <= reach.max(stats.range))
                .collect();
            near.sort_by_key(|(d, sid, _)| (*d, *sid));
            let holds = |d: i32| d <= 1 || (stats.ranged > 0 && d <= stats.range);
            let foe = near.into_iter().find(|(d, _, p)| holds(*d) || state.navigation.reachable(here, *p));
            if let Some((d, sid, p)) = foe {
                let f = &mut state.military.warbands[wi].fighters[fi];
                f.engage = Some(sid);
                if !holds(d)
                    && let Some(b) = breaking_wall.or(gate_ahead)
                {
                    // The soldier stands behind a gate or wall: break through first.
                    f.attacking = Some(b);
                    if m.destination().is_some() {
                        movement::set_destination(state, id, None);
                    }
                    continue;
                }
                f.attacking = None;
                let dest = if holds(d) { standing_tile(&m) } else { p };
                movement::set_destination(state, id, Some(Destination::Tile(dest)));
                continue;
            }
            state.military.warbands[wi].fighters[fi].engage = None;
            // Attacking a structure: keep at it while it stands.
            if breaking.is_some() {
                if m.destination().is_some() {
                    movement::set_destination(state, id, None);
                }
                continue;
            }
            state.military.warbands[wi].fighters[fi].attacking = None;
            let (Some(rect), Some(goal)) = (target_rect.map(|(r, _)| r), target) else {
                // Nothing to go for: withdraw.
                state.military.warbands[wi].stage = WarbandStage::Fleeing;
                continue;
            };
            // At the target.
            if rect.distance_to(here) <= 1 {
                if storming {
                    state.military.warbands[wi].fighters[fi].attacking = target;
                    if m.destination().is_some() {
                        movement::set_destination(state, id, None);
                    }
                } else {
                    plunder_now = true;
                }
                continue;
            }
            // A gate in the way: break it.
            if let Some(g) = gate_ahead {
                state.military.warbands[wi].fighters[fi].attacking = Some(g);
                movement::set_destination(state, id, None);
                continue;
            }
            // March on to the target (set every round: a failed search is retried when due).
            if let Some(stand) = approach(state, here, goal) {
                movement::set_destination(state, id, Some(Destination::Tile(stand)));
                continue;
            }
            // No way to the target: break through the wall nearest to it.
            let entrance = target_rect.map_or(here, |(_, e)| e);
            let breach = match state.military.warbands[wi].breach {
                Some((b, stand)) if state.buildings.get(b).is_some() && state.navigation.reachable(here, stand) => Some((b, stand)),
                _ => {
                    let found = find_breach(state, defs, here, entrance);
                    state.military.warbands[wi].breach = found;
                    found
                }
            };
            match breach {
                Some((b, stand)) => {
                    let wall = state.buildings.get(b).map(|bb| bb.rect());
                    if wall.is_some_and(|r| r.distance_to(here) <= 1) {
                        state.military.warbands[wi].fighters[fi].attacking = Some(b);
                        if m.destination().is_some() {
                            movement::set_destination(state, id, None);
                        }
                    } else {
                        movement::set_destination(state, id, Some(Destination::Tile(stand)));
                    }
                }
                None => {
                    lost_way.get_or_insert(here);
                }
            }
        }
        if let Some(from) = lost_way
            && state.military.warbands[wi].stage == WarbandStage::Attacking
        {
            give_up_target(state, defs, wi, from, events);
        }
        if matches!(stage, WarbandStage::Withdrawing | WarbandStage::Fleeing) {
            // Leaving with no way off the map (walled in after a peace, or fleeing from an
            // island whose bridge is gone): after a while the group disbands.
            let w = &mut state.military.warbands[wi];
            if shut_in > 0 && shut_in == w.fighters.len() {
                w.stuck_ticks = w.stuck_ticks.saturating_add(round);
                if u64::from(w.stuck_ticks) >= u64::from(rules.armies.withdraw_stuck_hours) * TICKS_PER_HOUR {
                    disband_shut_in(state, defs, wi, events);
                    continue;
                }
            } else {
                w.stuck_ticks = 0;
            }
        }
        if plunder_now && state.military.warbands[wi].stage == WarbandStage::Attacking {
            // Plundering takes `plunder_rounds` rounds at the target.
            let w = &mut state.military.warbands[wi];
            w.plunder_rounds += 1;
            if w.plunder_rounds >= u16::from(rules.bandits.plunder_rounds) {
                plunder(state, defs, wi, events);
            }
        }
    }
}

/// Tile of the first fighter of a group on the map (its entry otherwise).
fn first_fighter_tile(state: &GameState, w: &Warband) -> IVec2 {
    w.fighters.iter().find_map(|f| state.population.walking.traveler(f.walker)).map_or(w.entry, |t| t.motion().pos().tile())
}

/// A fighter of the group found no way to its target and no wall or gate to break: the group
/// turns to the next target it can reach, or withdraws without plunder.
fn give_up_target(state: &mut GameState, defs: &Defs, wi: usize, from: IVec2, events: &mut Vec<SimEvent>) {
    let snapshot = state.military.warbands[wi].clone();
    if let Some(next) = reachable_target(state, defs, &snapshot, from, snapshot.target) {
        let w = &mut state.military.warbands[wi];
        w.target = Some(next);
        w.breach = None;
        w.hold_ticks = 0;
        return;
    }
    state.military.warbands[wi].stage = WarbandStage::Fleeing;
    let text = match snapshot.kind {
        WarbandKind::Bandits => "Die Räuber finden keinen Weg zu ihrem Ziel und ziehen ab.".to_owned(),
        WarbandKind::Army { kingdom, .. } => format!("Das Heer von {} findet keinen Weg zu seinem Ziel und zieht ab.", kingdom_name(state, kingdom)),
    };
    events.push(SimEvent::Notification(Notification::new(Severity::Good, text).with_kind(NotificationKind::Threat).at(from)));
}

/// A withdrawing or fleeing group that has found no way off the map for
/// `armies.withdraw_stuck_hours` (walled in without a gate after the peace, or on an island whose
/// bridge was torn down) disbands: its fighters slip away one by one and make their way home
/// with what they carry (they leave the map; [`prune`] counts them, [`finish`] writes the
/// report).
fn disband_shut_in(state: &mut GameState, defs: &Defs, wi: usize, events: &mut Vec<SimEvent>) {
    let w = &state.military.warbands[wi];
    let at = first_fighter_tile(state, w);
    let walkers: Vec<TravelerId> = w.fighters.iter().map(|f| f.walker).collect();
    let (kind, entry) = (w.kind, w.entry);
    // Walls hold it if breaking through would open a way (only withdrawing armies do not break).
    let from = if find_breach(state, defs, at, entry).is_some() { "aus Euren Mauern" } else { "aus Eurem Land" };
    let text = match kind {
        WarbandKind::Army { kingdom, .. } => format!(
            "Das Heer von {} findet keinen Weg {from} und löst sich auf: Seine Soldaten schlagen sich einzeln in die Heimat durch.",
            kingdom_name(state, kingdom)
        ),
        WarbandKind::Bandits => format!("Die Räuber finden keinen Weg {from} und zerstreuen sich."),
    };
    for walker in walkers {
        movement::forget(state, WalkerId::Traveler(walker));
        state.population.walking.travelers.remove(walker);
    }
    state.military.warbands[wi].stuck_ticks = 0;
    events.push(SimEvent::Notification(Notification::new(Severity::Info, text).with_kind(NotificationKind::Threat).at(at)));
}

/// A fleeing or withdrawing fighter heads for the map edge it came through; if it cannot get
/// there any more, for the nearest edge tile it can reach; walled in, a fleeing one breaks
/// through the wall or gate nearest to its exit (a withdrawing one waits for a way, see
/// [`disband_shut_in`]). Returns false if it has no way off the map.
fn leave_map(state: &mut GameState, defs: &Defs, wi: usize, fi: usize, here: IVec2, may_break: bool) -> bool {
    let Some(walker) = state.military.warbands[wi].fighters.get(fi).map(|f| f.walker) else { return true };
    let id = WalkerId::Traveler(walker);
    let exit = state.military.warbands[wi].entry;
    let goal = if state.navigation.is_walkable(exit) && state.navigation.reachable(here, exit) {
        Some(exit)
    } else {
        crate::diplomacy::caravans::edge_towards(state, defs, here, exit)
    };
    if let Some(edge) = goal {
        let standing = state.population.walking.traveler(walker).is_some_and(|t| t.motion().pos() == crate::population::SubPos::center(edge));
        if standing {
            // Already at the edge (it gave up right where it entered): it leaves the map.
            movement::forget(state, id);
            state.population.walking.travelers.remove(walker);
            return true;
        }
        // Set every round: a failed search is retried when due.
        movement::set_destination(state, id, Some(Destination::MapEdge(edge)));
        return true;
    }
    if !may_break {
        return false;
    }
    match find_breach(state, defs, here, exit) {
        Some((b, stand)) => {
            if state.buildings.get(b).is_some_and(|bb| bb.rect().distance_to(here) <= 1) {
                state.military.warbands[wi].fighters[fi].attacking = Some(b);
                movement::set_destination(state, id, None);
            } else {
                movement::set_destination(state, id, Some(Destination::Tile(stand)));
            }
            true
        }
        None => false,
    }
}

/// Ends groups that are gone (beaten, fled, withdrawn): the battle report, morale, relations
/// and war exhaustion. Armies of kingdoms that made peace turn back before they arrive (no
/// report) or withdraw from the map without fighting on (a report only if they fought).
pub(crate) fn finish(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    // Peace: armies turn back or withdraw.
    for wi in 0..state.military.warbands.len() {
        let w = &state.military.warbands[wi];
        let Some(k) = w.kingdom() else { continue };
        if state.diplomacy.kingdom(k).is_some_and(|kk| kk.at_war()) {
            continue;
        }
        match w.stage {
            WarbandStage::Approaching { .. } => {
                let planned: u32 = w.planned.iter().map(|(_, n)| u32::from(*n)).sum();
                if let Some(kk) = state.diplomacy.kingdom_mut(k) {
                    kk.soldiers += planned;
                }
                let w = &mut state.military.warbands[wi];
                w.planned.clear();
                // It never set foot on the map: nothing to report.
                w.size = 0;
                w.stage = WarbandStage::Fleeing;
            }
            WarbandStage::Attacking => {
                let w = &mut state.military.warbands[wi];
                w.stage = WarbandStage::Withdrawing;
                for f in &mut w.fighters {
                    f.attacking = None;
                    f.engage = None;
                }
            }
            WarbandStage::Fleeing | WarbandStage::Withdrawing => {}
        }
    }
    let mut wi = 0;
    while wi < state.military.warbands.len() {
        let w = &state.military.warbands[wi];
        let over = !matches!(w.stage, WarbandStage::Approaching { .. }) && w.fighters.is_empty();
        if !over {
            wi += 1;
            continue;
        }
        let w = state.military.warbands.remove(wi);
        if w.size == 0 {
            // Turned back before it arrived.
            continue;
        }
        if w.stage == WarbandStage::Withdrawing && !w.fought() {
            // Withdrew after the peace without a fight.
            continue;
        }
        report(state, defs, w, events);
    }
}

/// The report of a finished group and its consequences.
fn report(state: &mut GameState, defs: &Defs, w: Warband, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().military;
    let t = &w.tally;
    let gained = t.goods_lost.iter().map(|m| m.amount).sum::<u32>() + t.gold_lost;
    let kept = gained.saturating_sub(t.recovered);
    let beaten = u32::from(w.escaped) * 2 < u32::from(w.size);
    // An army that withdrew after the peace was neither beaten nor driven off.
    let withdrawn = w.stage == WarbandStage::Withdrawing;
    let outcome = if w.plundered && kept > 0 {
        BattleOutcome::Defeat
    } else if withdrawn {
        BattleOutcome::Draw
    } else if t.enemy_losses > 0 || !w.plundered {
        if t.own_losses > t.enemy_losses && !beaten { BattleOutcome::Draw } else { BattleOutcome::Victory }
    } else {
        BattleOutcome::Draw
    };
    let (kind, enemy) = match w.kind {
        WarbandKind::Bandits => (BattleKind::Raid, "Räuberbande".to_owned()),
        WarbandKind::Army { kingdom, .. } => (BattleKind::Defence, format!("Heer von {}", kingdom_name(state, kingdom))),
    };
    let mut text = format!(
        "{}: {} – {} gefallen, {} entkommen; eigene Verluste: {}.",
        enemy,
        count_text(u32::from(w.size), "Feind", "Feinde"),
        t.enemy_losses,
        w.escaped,
        count_text(t.own_losses, "Soldat", "Soldaten"),
    );
    if kept > 0 {
        let mut lost = goods_text(defs, &t.goods_lost);
        if t.gold_lost > 0 {
            if !lost.is_empty() {
                lost.push_str(", ");
            }
            lost.push_str(&format!("{} Gold", t.gold_lost));
        }
        text.push_str(&format!(" Geraubt: {lost}"));
        if t.recovered > 0 {
            text.push_str(&format!(" (davon {} zurückgewonnen)", t.recovered));
        }
        text.push('.');
    }
    if t.buildings_damaged > 0 {
        text.push_str(&format!(" {} beschädigt.", count_text(t.buildings_damaged, "Gebäude", "Gebäude")));
    }
    if withdrawn {
        text.push_str(" Nach dem Friedensschluss ist das Heer abgezogen.");
    }
    let tally = Tally { own_units: u32::try_from(w.defenders.len()).unwrap_or(u32::MAX), ..w.tally.clone() };
    let serial = state.military.record_battle(BattleReport {
        serial: 0,
        kind,
        start_tick: w.started_tick,
        end_tick: state.clock.tick,
        position: w.entry,
        kingdom: w.kingdom(),
        enemy,
        enemy_units: u32::from(w.size),
        tally,
        outcome,
        text: text.clone(),
    });
    // Morale of the defenders.
    let shift = match outcome {
        BattleOutcome::Victory => rules.morale_victory,
        BattleOutcome::Defeat => rules.morale_defeat,
        BattleOutcome::Draw => 0,
    };
    for id in &w.defenders {
        if let Some(s) = state.military.soldiers.get_mut(id) {
            s.battle_morale = (s.battle_morale + i16::from(shift)).clamp(-50, 50);
            s.morale = u16::try_from((i32::from(s.morale) + i32::from(shift)).clamp(0, 100)).unwrap_or(0);
        }
    }
    // A beaten army tires and angers its kingdom; its plunder is reported to diplomacy.
    if let WarbandKind::Army { kingdom, .. } = w.kind {
        let war_rules = &defs.rules().diplomacy.war;
        if let Some(k) = state.diplomacy.kingdom_mut(kingdom) {
            if let Some(war) = &mut k.war {
                war.raids = war.raids.saturating_add(1);
                if outcome == BattleOutcome::Victory {
                    war.exhaustion = war.exhaustion.saturating_add(rules.armies.exhaustion_defeat);
                } else if !w.plundered {
                    war.exhaustion = war.exhaustion.saturating_add(u16::from(war_rules.exhaustion_failed_raid));
                }
            }
            if outcome == BattleOutcome::Victory && !withdrawn {
                crate::diplomacy::relations::remember(k, defs, MemoryCause::BattleLost, f32::from(rules.armies.relation_defeat), Some(30));
            }
            k.population = k.population.saturating_sub(t.enemy_losses / 2).max(1);
        }
        if w.plundered {
            events.push(SimEvent::RaidHappened { kingdom, lost: t.goods_lost.clone() });
        }
        state.diplomacy.record(state.clock.tick, Some(kingdom), None, text.clone());
        crate::diplomacy::relations::announce_states(state, defs, events);
    }
    events.push(SimEvent::BattleEnded { serial, kind, outcome });
    let severity = match outcome {
        BattleOutcome::Victory => Severity::Good,
        BattleOutcome::Defeat => Severity::Danger,
        BattleOutcome::Draw => Severity::Warning,
    };
    let title = match (kind, outcome) {
        (BattleKind::Raid, BattleOutcome::Victory) => "Räuberüberfall abgewehrt",
        (BattleKind::Raid, _) => "Räuberüberfall",
        (_, BattleOutcome::Victory) => "Angriff abgewehrt",
        _ => "Angriff",
    };
    events.push(SimEvent::Notification(Notification::new(severity, format!("{title}. {text}")).with_kind(NotificationKind::Battle).at(w.entry)));
}

/// Daily repair of damage on structures while no enemies are on the map.
pub(crate) fn repair(state: &mut GameState, defs: &Defs) {
    if state.military.enemies_on_map() {
        return;
    }
    let percent = u32::from(defs.rules().military.repair_percent_per_day);
    let ids: Vec<BuildingId> = state.military.damage.keys().copied().collect();
    for id in ids {
        let hp = structure_hp(state, defs, id);
        let fix = (hp * percent / 100).max(1);
        let left = state.military.damage_of(id).saturating_sub(fix);
        if left == 0 || state.buildings.get(id).is_none() {
            state.military.damage.remove(&id);
        } else {
            state.military.damage.insert(id, left);
        }
    }
}

/// Removes damage records of buildings that are gone.
pub(crate) fn forget_missing(state: &mut GameState) {
    let buildings = &state.buildings;
    state.military.damage.retain(|b, _| buildings.get(*b).is_some());
}
