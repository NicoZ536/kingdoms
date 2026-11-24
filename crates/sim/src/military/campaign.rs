//! Campaigns and sieges against AI kingdoms (docs/game_design.md §11).
//!
//! A company ordered on a campaign (only against a kingdom at war with the player; a siege
//! needs at least one siege engine) gathers at the map-edge tile towards the kingdom within
//! `campaigns.muster_hours`; whoever arrives leaves the map (with the engines' crews), the rest
//! stays home. The march takes as long as a caravan's journey; provisions for the whole
//! campaign (`war_ration_milli` per soldier and day) are taken from the storages at departure.
//! Off the map every soldier eats his share of them hour by hour; whatever is left when the
//! company comes home – early after a retreat or a peace, or because soldiers fell – goes back
//! into the storages (lost only if nobody comes back).
//!
//! **Field battle** on arrival, abstract: the kingdom's field army (`field_share_percent` of
//! its soldiers in the unit mix of AI armies) against the company, in up to `max_rounds`
//! rounds with the combat formula of [`super::combat`]: shooters first, then melee; enemy
//! damage is spread evenly over the living soldiers (shooters last), own damage over the enemy
//! stacks by size. A side breaks once its losses reach 30 % + half its average morale. Fallen
//! soldiers die as citizens. A won battle takes `loot_percent` of the kingdom's treasury and
//! tires it (`exhaustion_victory`).
//!
//! **Siege** (after a won field battle, `siege_hours`): the siege engines (and every soldier's
//! building damage) batter the seat (`seat_hp_per_inhabitant` × inhabitants) while the
//! garrison — the kingdom's remaining soldiers — fights back from the walls
//! (`wall_defense_percent`); once the walls are broken the garrison is stormed. Success forces
//! a peace: the kingdom cedes border land and pays `siege_reparations_per_inhabitant` gold per
//! inhabitant (as far as its treasury allows).
//!
//! Then the company marches home and reappears at the map edge. A retreat order turns it back
//! at any time; a peace ends the campaign.

use glam::IVec2;
use serde::{Deserialize, Serialize};

use super::combat::{FighterStats, attack_against, fighter_stats, hit_damage, personal};
use super::{BattleKind, BattleOutcome, BattleReport, Campaign, FighterKind, Order, Tally, count_text, fallen_text};
use crate::defs::{Defs, UnitClass};
use crate::diplomacy::{KingdomId, MemoryCause, PeaceTerms};
use crate::economy::{LedgerCategory, MILLI_PER_GOLD, storage};
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::population::{CitizenId, Motion, WalkerId, movement, routine};
use crate::state::GameState;
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};

/// Stage of a campaign.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CampaignStage {
    /// Gathering at the map edge until `until_tick`.
    Mustering {
        /// End of the muster.
        until_tick: u64,
    },
    /// On the way to the kingdom; the battle is fought at `until_tick`.
    Marching {
        /// Arrival.
        until_tick: u64,
        /// Departure (a retreat takes as long back).
        since_tick: u64,
    },
    /// Besieging the seat until `until_tick`.
    Sieging {
        /// End of the siege.
        until_tick: u64,
    },
    /// On the way home; back at the map edge at `until_tick`.
    Returning {
        /// Return.
        until_tick: u64,
    },
}

impl CampaignStage {
    /// German name.
    pub const fn display_name(self) -> &'static str {
        match self {
            CampaignStage::Mustering { .. } => "Sammelt sich",
            CampaignStage::Marching { .. } => "Auf dem Marsch",
            CampaignStage::Sieging { .. } => "Belagert",
            CampaignStage::Returning { .. } => "Auf dem Rückweg",
        }
    }

    /// Tick the stage ends.
    pub const fn until_tick(self) -> u64 {
        match self {
            CampaignStage::Mustering { until_tick }
            | CampaignStage::Marching { until_tick, .. }
            | CampaignStage::Sieging { until_tick }
            | CampaignStage::Returning { until_tick } => until_tick,
        }
    }
}

fn kingdom_name(state: &GameState, id: KingdomId) -> String {
    state.diplomacy.kingdom(id).map_or_else(|| "?".to_owned(), |k| k.name.clone())
}

/// Why company `company` cannot march against `kingdom` now (German), or `None`.
pub fn campaign_problem(state: &GameState, defs: &Defs, company: u32, kingdom: KingdomId, siege: bool) -> Option<String> {
    let Some(c) = state.military.company(company) else { return Some("Diese Kompanie gibt es nicht.".to_owned()) };
    let Some(k) = state.diplomacy.kingdom(kingdom) else { return Some("Dieses Königreich gibt es nicht.".to_owned()) };
    if !k.at_war() {
        return Some(format!("Mit {} herrscht kein Krieg.", k.name));
    }
    if state.military.campaigns.iter().any(|camp| camp.company == company) {
        return Some(format!("{} ist bereits auf einem Feldzug.", c.name));
    }
    let ready: Vec<&super::Soldier> = c.members.iter().filter_map(|m| state.military.soldiers.get(m)).filter(|s| s.is_available()).collect();
    if ready.is_empty() {
        return Some(format!("{} hat keine einsatzbereiten Soldaten.", c.name));
    }
    if siege && !ready.iter().any(|s| defs.units().get(s.unit.index()).is_some_and(|u| u.class == UnitClass::Siege)) {
        return Some("Eine Belagerung braucht mindestens ein Belagerungsgerät.".to_owned());
    }
    let Some(hq) = storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b)) else {
        return Some("Ohne Hauptquartier kann kein Feldzug beginnen.".to_owned());
    };
    // The company gathers at the map edge: there must be a way out (checked for certain when
    // the order is given, see [`start`]).
    let from = hq.entrance();
    let (w, h) = (state.world.width(), state.world.height());
    let mut edge = (0..w).flat_map(|x| [IVec2::new(x, 0), IVec2::new(x, h - 1)]).chain((0..h).flat_map(|y| [IVec2::new(0, y), IVec2::new(w - 1, y)]));
    let nav = &state.navigation;
    if nav.components_known() && !edge.any(|p| nav.is_walkable(p) && nav.reachable_known(from, p) == Some(true)) {
        return Some(NO_WAY_OUT.to_owned());
    }
    None
}

/// Why a campaign cannot set out when no map edge can be reached.
const NO_WAY_OUT: &str = "Vom Hauptquartier führt kein Weg zum Kartenrand.";

/// Starts a campaign of `company` (checked by [`campaign_problem`]); `Err` (German) if no map
/// edge can be reached from the headquarters.
pub(crate) fn start(
    state: &mut GameState,
    defs: &Defs,
    company: u32,
    kingdom: KingdomId,
    siege: bool,
    events: &mut Vec<SimEvent>,
) -> Result<(), String> {
    let Some(seat) = state.diplomacy.kingdom(kingdom).map(|k| k.seat) else { return Err("Dieses Königreich gibt es nicht.".to_owned()) };
    let from = storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b)).map_or(state.world.start, |b| b.entrance());
    let Some(entry) = crate::diplomacy::caravans::edge_towards(state, defs, from, seat) else { return Err(NO_WAY_OUT.to_owned()) };
    let members = state.military.company(company).map(|c| c.members.clone()).unwrap_or_default();
    let mut soldiers: Vec<CitizenId> = members.into_iter().filter(|m| state.military.soldiers.get(m).is_some_and(|s| s.is_available())).collect();
    let crew: Vec<CitizenId> = state
        .military
        .soldiers
        .iter()
        .filter(|(_, s)| s.crew_of.is_some_and(|l| soldiers.contains(&l)) && s.away.is_none())
        .map(|(id, _)| *id)
        .collect();
    soldiers.extend(crew);
    soldiers.sort_unstable();
    let id = state.military.next_campaign;
    state.military.next_campaign = id.wrapping_add(1).max(1);
    let until_tick = state.clock.tick + u64::from(defs.rules().military.campaigns.muster_hours) * TICKS_PER_HOUR;
    state.military.campaigns.push(Campaign {
        id,
        company,
        kingdom,
        siege,
        stage: CampaignStage::Mustering { until_tick },
        entry,
        soldiers,
        started_tick: state.clock.tick,
        tally: Tally::default(),
        provisions: Vec::new(),
        soldier_hours: 0,
    });
    let name = state.military.company(company).map_or_else(String::new, |c| c.name.clone());
    let what = if siege { "Belagerung von" } else { "Feldzug gegen" };
    let text = format!("{name} sammelt sich am Rand des Reiches zum {what} {}.", kingdom_name(state, kingdom));
    events.push(SimEvent::CampaignChanged { campaign: id, company });
    events.push(SimEvent::Notification(Notification::new(Severity::Info, text).with_kind(NotificationKind::Military).at(entry)));
    Ok(())
}

/// A retreat order: the campaign at index `i` turns back.
pub(crate) fn turn_back(state: &mut GameState, defs: &Defs, i: usize, events: &mut Vec<SimEvent>) {
    let now = state.clock.tick;
    let Some(camp) = state.military.campaigns.get(i) else { return };
    let stage = match camp.stage {
        CampaignStage::Mustering { .. } => CampaignStage::Returning { until_tick: now },
        CampaignStage::Marching { since_tick, .. } => CampaignStage::Returning { until_tick: now + now.saturating_sub(since_tick) },
        CampaignStage::Sieging { .. } => {
            CampaignStage::Returning { until_tick: now + crate::diplomacy::caravans::travel_ticks(state, defs, camp.kingdom) }
        }
        returning @ CampaignStage::Returning { .. } => returning,
    };
    let (id, company) = (camp.id, camp.company);
    state.military.campaigns[i].stage = stage;
    events.push(SimEvent::CampaignChanged { campaign: id, company });
}

/// Every tick: musters, marches, battles, sieges and returns.
pub(crate) fn run(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let now = state.clock.tick;
    if state.clock.is_new_hour() {
        eat_provisions(state);
    }
    let mut i = 0;
    while i < state.military.campaigns.len() {
        let camp = &state.military.campaigns[i];
        let at_war = state.diplomacy.kingdom(camp.kingdom).is_some_and(|k| k.at_war());
        if !at_war && !matches!(camp.stage, CampaignStage::Returning { .. }) {
            turn_back(state, defs, i, events);
        }
        let camp = &state.military.campaigns[i];
        match camp.stage {
            CampaignStage::Mustering { until_tick } => {
                depart_arrivals(state, defs, i);
                let camp = &state.military.campaigns[i];
                let all_away = camp.soldiers.iter().all(|s| state.military.soldiers.get(s).is_some_and(|x| x.away.is_some()));
                if all_away || now >= until_tick {
                    set_out(state, defs, i, events);
                }
            }
            CampaignStage::Marching { until_tick, .. } if now >= until_tick => {
                let won = field_battle(state, defs, i, events);
                let Some(camp) = state.military.campaigns.get(i) else { continue };
                let (id, company, kingdom, wants_siege) = (camp.id, camp.company, camp.kingdom, camp.siege);
                let travel = crate::diplomacy::caravans::travel_ticks(state, defs, kingdom);
                let siege_ticks = u64::from(defs.rules().military.campaigns.siege_hours) * TICKS_PER_HOUR;
                let siege = won && wants_siege && has_engine(state, defs, i);
                state.military.campaigns[i].stage = if siege {
                    CampaignStage::Sieging { until_tick: now + siege_ticks }
                } else {
                    CampaignStage::Returning { until_tick: now + travel }
                };
                events.push(SimEvent::CampaignChanged { campaign: id, company });
            }
            CampaignStage::Sieging { until_tick } if now >= until_tick => {
                siege(state, defs, i, events);
                let Some(camp) = state.military.campaigns.get(i) else { continue };
                let (id, company, kingdom) = (camp.id, camp.company, camp.kingdom);
                let travel = crate::diplomacy::caravans::travel_ticks(state, defs, kingdom);
                state.military.campaigns[i].stage = CampaignStage::Returning { until_tick: now + travel };
                events.push(SimEvent::CampaignChanged { campaign: id, company });
            }
            CampaignStage::Returning { until_tick } if now >= until_tick => {
                come_home(state, defs, i, events);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
}

fn has_engine(state: &GameState, defs: &Defs, i: usize) -> bool {
    state.military.campaigns[i].soldiers.iter().filter_map(|s| state.military.soldiers.get(s)).any(|s| {
        s.crew_of.is_none() && defs.units().get(s.unit.index()).is_some_and(|u| u.class == UnitClass::Siege)
    })
}

/// Soldiers of a mustering campaign standing at the entry tile leave the map.
fn depart_arrivals(state: &mut GameState, defs: &Defs, i: usize) {
    let (id, entry, soldiers) = {
        let c = &state.military.campaigns[i];
        (c.id, c.entry, c.soldiers.clone())
    };
    let hq = storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b).map(|bb| (b, bb.door())));
    let Some((hq, door)) = hq else { return };
    let centre = crate::population::SubPos::center(entry);
    for s in soldiers {
        let Some(soldier) = state.military.soldiers.get(&s) else { continue };
        if soldier.away.is_some() {
            continue;
        }
        let here = state.population.citizen(s).is_some_and(|c| c.motion().inside().is_none() && c.motion().pos() == centre);
        if !here {
            continue;
        }
        // Off the map: hidden (kept inside the headquarters) until the return.
        movement::place(state, WalkerId::Citizen(s), Motion::in_building(hq, door));
        if let Some(x) = state.military.soldiers.get_mut(&s) {
            x.away = Some(id);
            x.duty = None;
            x.engage = None;
        }
        routine::refresh_activity(state, defs, s);
    }
}

/// Soldiers away on a campaign wait hidden inside the main headquarters. When the building
/// they wait in is demolished (another headquarters still stands), they move into the main
/// headquarters that remains, so they stay off the map until the company returns.
pub(crate) fn rehome_away(state: &mut GameState, defs: &Defs, removed: crate::buildings::BuildingId) {
    let Some((hq, door)) = storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b).map(|bb| (b, bb.door()))) else {
        return;
    };
    let ids: Vec<CitizenId> = state
        .military
        .soldiers
        .iter()
        .filter(|(_, s)| s.away.is_some())
        .map(|(id, _)| *id)
        .filter(|id| state.population.citizen(*id).is_some_and(|c| c.motion().inside() == Some(removed)))
        .collect();
    for id in ids {
        movement::place(state, WalkerId::Citizen(id), Motion::in_building(hq, door));
    }
}

/// End of the muster: who is not there stays home; the others march (provisions taken).
fn set_out(state: &mut GameState, defs: &Defs, i: usize, events: &mut Vec<SimEvent>) {
    let now = state.clock.tick;
    let (id, company, kingdom) = {
        let c = &state.military.campaigns[i];
        (c.id, c.company, c.kingdom)
    };
    let soldiers = state.military.campaigns[i].soldiers.clone();
    let (gone, stay): (Vec<CitizenId>, Vec<CitizenId>) =
        soldiers.into_iter().partition(|s| state.military.soldiers.get(s).is_some_and(|x| x.away == Some(id)));
    state.military.campaigns[i].soldiers = gone.clone();
    let name = state.military.company(company).map_or_else(String::new, |c| c.name.clone());
    if !gone.iter().any(|s| state.military.soldiers.get(s).is_some_and(|x| x.crew_of.is_none())) {
        // Nobody came (or only crew): the campaign is called off.
        let entry = state.military.campaigns[i].entry;
        for s in &gone {
            reappear(state, defs, *s, state.military.campaigns[i].entry);
        }
        state.military.campaigns.remove(i);
        if let Some(c) = state.military.company_mut(company) {
            c.order = Order::Station;
        }
        events.push(SimEvent::CampaignChanged { campaign: id, company });
        events.push(SimEvent::Notification(
            Notification::new(Severity::Warning, format!("{name} ist nicht rechtzeitig aufgebrochen; der Feldzug fällt aus.")).with_kind(NotificationKind::Military).at(entry),
        ));
        return;
    }
    let travel = crate::diplomacy::caravans::travel_ticks(state, defs, kingdom);
    let siege_ticks = if state.military.campaigns[i].siege { u64::from(defs.rules().military.campaigns.siege_hours) * TICKS_PER_HOUR } else { 0 };
    let days = (2 * travel + siege_ticks).div_ceil(TICKS_PER_DAY).max(1);
    let need = u32::try_from((gone.len() as u64 * days * u64::from(defs.rules().military.war_ration_milli)).div_ceil(1000)).unwrap_or(u32::MAX);
    let provisions = super::upkeep::take_food(state, defs, need);
    let eaten: u32 = provisions.iter().map(|m| m.amount).sum();
    state.military.campaigns[i].provisions = provisions;
    if eaten < need {
        let shift = i32::from(defs.rules().military.morale_hungry);
        for s in &gone {
            if let Some(x) = state.military.soldiers.get_mut(s) {
                x.morale = u16::try_from((i32::from(x.morale) + shift).clamp(0, 100)).unwrap_or(0);
            }
        }
    }
    state.military.campaigns[i].stage = CampaignStage::Marching { until_tick: now + travel, since_tick: now };
    let fighters = gone.iter().filter(|s| state.military.soldiers.get(s).is_some_and(|x| x.crew_of.is_none())).count();
    let mut text = format!(
        "{name} bricht mit {} gegen {} auf. Ankunft in etwa {}.",
        count_text(u32::try_from(fighters).unwrap_or(0), "Soldat", "Soldaten"),
        kingdom_name(state, kingdom),
        super::threats::hours_text(travel.div_ceil(TICKS_PER_HOUR))
    );
    if !stay.is_empty() {
        text.push_str(&match stay.len() {
            1 => " 1 Soldat kam nicht rechtzeitig und bleibt daheim.".to_owned(),
            n => format!(" {n} Soldaten kamen nicht rechtzeitig und bleiben daheim."),
        });
    }
    if eaten < need {
        text.push_str(" Der Proviant reicht nicht für alle.");
    }
    let entry = state.military.campaigns[i].entry;
    events.push(SimEvent::CampaignChanged { campaign: id, company });
    events.push(SimEvent::Notification(Notification::new(Severity::Info, text).with_kind(NotificationKind::Military).at(entry)));
}

/// A soldier comes back onto the map at `entry`.
fn reappear(state: &mut GameState, defs: &Defs, id: CitizenId, entry: IVec2) {
    if let Some(s) = state.military.soldiers.get_mut(&id) {
        s.away = None;
        s.duty = None;
    }
    if state.population.citizen(id).is_some() {
        movement::place(state, WalkerId::Citizen(id), Motion::standing(entry));
    }
    let mut venues = routine::Venues::default();
    routine::update(state, defs, id, &mut venues);
}

/// Hourly: every soldier off the map eats his share of the provisions.
fn eat_provisions(state: &mut GameState) {
    let soldiers = &state.military.soldiers;
    for camp in &mut state.military.campaigns {
        if matches!(camp.stage, CampaignStage::Mustering { .. }) {
            continue;
        }
        let away = camp.soldiers.iter().filter(|s| soldiers.get(s).is_some_and(|x| x.away == Some(camp.id))).count();
        camp.soldier_hours = camp.soldier_hours.saturating_add(away as u64);
    }
}

impl super::Campaign {
    /// The provisions not eaten yet, taken from the last goods taken backwards (whole units).
    fn leftover_provisions(&self, defs: &Defs) -> Vec<crate::defs::ResourceAmount> {
        let mut eaten = self.provisions_eaten(defs);
        let mut left: Vec<crate::defs::ResourceAmount> = Vec::new();
        for m in self.provisions.iter().rev() {
            let used = eaten.min(m.amount);
            eaten -= used;
            if m.amount > used {
                left.push(crate::defs::ResourceAmount { resource: m.resource, amount: m.amount - used });
            }
        }
        left.reverse();
        left
    }
}

/// The company is back: it reappears at the map edge and stands at its posts again; the
/// provisions it did not eat go back into the storages.
fn come_home(state: &mut GameState, defs: &Defs, i: usize, events: &mut Vec<SimEvent>) {
    let camp = state.military.campaigns.remove(i);
    crate::navigation::prepare(state, defs);
    let entry = if state.navigation.is_walkable(camp.entry) {
        camp.entry
    } else {
        let from = storage::main_headquarters(state, defs).and_then(|b| state.buildings.get(b)).map_or(state.world.start, |b| b.entrance());
        crate::diplomacy::caravans::edge_towards(state, defs, from, camp.entry).unwrap_or(camp.entry)
    };
    for s in &camp.soldiers {
        if state.military.soldiers.get(s).is_some_and(|x| x.away.is_some()) {
            reappear(state, defs, *s, entry);
        }
    }
    if let Some(c) = state.military.company_mut(camp.company) {
        c.order = Order::Station;
    }
    let name = state.military.company(camp.company).map_or_else(String::new, |c| c.name.clone());
    let back = camp.soldiers.iter().filter(|s| state.military.soldiers.get(s).is_some_and(|x| x.crew_of.is_none())).count();
    let mut text = format!("{name} ist mit {} heimgekehrt.", count_text(u32::try_from(back).unwrap_or(0), "Soldat", "Soldaten"));
    // Whoever comes back brings the unused provisions home (with nobody back they are lost).
    let returned = if camp.soldiers.is_empty() { Vec::new() } else { camp.leftover_provisions(defs) };
    let units: u32 = returned.iter().map(|m| m.amount).sum();
    if units > 0 {
        storage::deposit_or_lose(state, defs, &returned, entry, events);
        let what = if units == 1 { "kommt 1 Einheit Nahrung".to_owned() } else { format!("kommen {units} Einheiten Nahrung") };
        text.push_str(&format!(" Vom Proviant {what} zurück ins Lager."));
    }
    events.push(SimEvent::CampaignChanged { campaign: camp.id, company: camp.company });
    events.push(SimEvent::Notification(Notification::new(Severity::Info, text).with_kind(NotificationKind::Military).at(entry)));
    super::orders::assign_duties(state, defs);
}

/// Enemy stack of an abstract battle.
struct Stack {
    stats: FighterStats,
    count: u32,
    /// Damage taken that has not killed a whole unit yet.
    wounds: u32,
}

/// Own soldier of an abstract battle.
struct Own {
    id: CitizenId,
    stats: FighterStats,
    hp: u32,
}

/// The kingdom's soldiers as stacks in the unit mix of AI armies.
fn enemy_stacks(state: &GameState, defs: &Defs, soldiers: u32) -> Vec<Stack> {
    let total: u32 = defs.army_mix().iter().map(|(_, w)| u32::from(*w)).sum::<u32>().max(1);
    let mut stacks: Vec<Stack> = defs
        .army_mix()
        .iter()
        .map(|(u, w)| Stack { stats: fighter_stats(state, defs, FighterKind::Unit(*u), false), count: soldiers * u32::from(*w) / total, wounds: 0 })
        .collect();
    let mut left = soldiers - stacks.iter().map(|s| s.count).sum::<u32>();
    for s in stacks.iter_mut() {
        if left == 0 {
            break;
        }
        s.count += 1;
        left -= 1;
    }
    stacks.retain(|s| s.count > 0);
    stacks
}

/// The company's fighting soldiers with their personal values.
fn own_soldiers(state: &GameState, defs: &Defs, i: usize) -> Vec<Own> {
    state.military.campaigns[i]
        .soldiers
        .iter()
        .filter_map(|id| state.military.soldiers.get(id).map(|s| (*id, s)))
        .filter(|(_, s)| s.crew_of.is_none() && s.hp > 0)
        .map(|(id, s)| {
            let base = fighter_stats(state, defs, FighterKind::Unit(s.unit), true);
            Own { id, stats: personal(base, defs, s.equipment, s.morale, s.experience), hp: u32::from(s.hp) }
        })
        .collect()
}

/// Damage of `count` attackers with `stats` against an average defense (`ranged`: shots).
fn volley(defs: &Defs, stats: &FighterStats, count: u32, ranged: bool, target_class: UnitClass, defense: u32) -> u32 {
    let base = if ranged { stats.ranged } else { stats.attack };
    if base == 0 {
        return 0;
    }
    hit_damage(defs, attack_against(base, stats.bonus, target_class), defense).saturating_mul(count)
}

/// Spreads `damage` evenly over the living own soldiers (melee before shooters); returns the
/// fallen.
fn hurt_own(own: &mut [Own], damage: u32) {
    let front: Vec<usize> = (0..own.len()).filter(|i| own[*i].hp > 0 && own[*i].stats.ranged == 0).collect();
    let back: Vec<usize> = (0..own.len()).filter(|i| own[*i].hp > 0 && own[*i].stats.ranged > 0).collect();
    let targets = if front.is_empty() { back } else { front };
    if targets.is_empty() {
        return;
    }
    let n = u32::try_from(targets.len()).unwrap_or(1);
    for (k, i) in targets.iter().enumerate() {
        let share = damage / n + u32::from(u32::try_from(k).unwrap_or(0) < damage % n);
        own[*i].hp = own[*i].hp.saturating_sub(share);
    }
}

/// Spreads `damage` over the enemy stacks by size; units die per full hit points.
fn hurt_enemy(stacks: &mut [Stack], damage: u32) {
    let total: u32 = stacks.iter().map(|s| s.count).sum();
    if total == 0 {
        return;
    }
    for s in stacks.iter_mut() {
        let share = u32::try_from(u64::from(damage) * u64::from(s.count) / u64::from(total)).unwrap_or(0);
        s.wounds += share;
        let hp = u32::from(s.stats.max_hp.max(1));
        let dead = (s.wounds / hp).min(s.count);
        s.count -= dead;
        s.wounds %= hp;
    }
}

/// Average defense and the main class of the enemy (for the own damage).
fn enemy_target(stacks: &[Stack], extra_percent: u32) -> (u32, UnitClass) {
    let total: u64 = stacks.iter().map(|s| u64::from(s.count)).sum();
    if total == 0 {
        return (0, UnitClass::Infantry);
    }
    let defense = stacks.iter().map(|s| u64::from(s.stats.defense) * u64::from(s.count)).sum::<u64>() / total;
    let class = stacks.iter().max_by_key(|s| s.count).map_or(UnitClass::Infantry, |s| s.stats.class);
    (u32::try_from(defense * u64::from(100 + extra_percent) / 100).unwrap_or(u32::MAX), class)
}

fn own_target(own: &[Own]) -> (u32, UnitClass) {
    let alive: Vec<&Own> = own.iter().filter(|o| o.hp > 0).collect();
    if alive.is_empty() {
        return (0, UnitClass::Infantry);
    }
    let defense = alive.iter().map(|o| u64::from(o.stats.defense)).sum::<u64>() / alive.len() as u64;
    (u32::try_from(defense).unwrap_or(u32::MAX), alive[0].stats.class)
}

/// One round of an abstract battle; `walls`: the enemy fights from walls (siege), `storm`:
/// whether own melee reaches the enemy.
fn round(defs: &Defs, own: &mut [Own], stacks: &mut [Stack], wall_percent: u32, melee_reaches: bool) {
    for ranged in [true, false] {
        let (def_e, class_e) = enemy_target(stacks, wall_percent);
        let (def_o, class_o) = own_target(own);
        let to_enemy: u32 = if ranged || melee_reaches {
            own.iter().filter(|o| o.hp > 0).map(|o| volley(defs, &o.stats, 1, ranged, class_e, def_e)).sum()
        } else {
            0
        };
        let to_own: u32 = stacks.iter().map(|s| volley(defs, &s.stats, s.count, ranged, class_o, def_o)).sum();
        hurt_enemy(stacks, to_enemy);
        hurt_own(own, to_own);
    }
}

/// True if a side with `start` units and `left` units and average morale `morale` breaks.
fn breaks(start: u32, left: u32, morale: u32) -> bool {
    start > 0 && (start - left.min(start)) * 100 >= start * (30 + morale.min(100) / 2)
}

/// Applies the losses of an abstract battle to the soldiers: the fallen die, survivors keep
/// their wounds and gain experience and morale.
fn apply_own(state: &mut GameState, defs: &Defs, own: &[Own], won: bool, events: &mut Vec<SimEvent>) -> u32 {
    let rules = &defs.rules().military;
    let mut dead = 0;
    for o in own {
        if o.hp == 0 {
            dead += 1;
            crate::population::die_in_battle(state, defs, o.id, events);
        } else if let Some(s) = state.military.soldiers.get_mut(&o.id) {
            s.hp = u16::try_from(o.hp).unwrap_or(u16::MAX);
            s.experience = (s.experience + 10).min(100);
            s.equipment = s.equipment.saturating_sub(rules.wear_per_round_permille * 10);
            let shift = if won { rules.morale_victory } else { rules.morale_defeat };
            s.battle_morale = (s.battle_morale + i16::from(shift)).clamp(-50, 50);
            s.morale = u16::try_from((i32::from(s.morale) + i32::from(shift)).clamp(0, 100)).unwrap_or(0);
        }
    }
    dead
}

/// The field battle on arrival; returns true if the company won.
fn field_battle(state: &mut GameState, defs: &Defs, i: usize, events: &mut Vec<SimEvent>) -> bool {
    let rules = &defs.rules().military.campaigns;
    let kingdom = state.military.campaigns[i].kingdom;
    let Some(k) = state.diplomacy.kingdom(kingdom) else { return false };
    let field = k.soldiers * u32::from(rules.field_share_percent) / 100;
    let mut stacks = enemy_stacks(state, defs, field);
    let mut own = own_soldiers(state, defs, i);
    let start_own = u32::try_from(own.len()).unwrap_or(0);
    let morale_own = if own.is_empty() { 0 } else { own.iter().map(|o| u32::from(o.stats.morale)).sum::<u32>() / start_own };
    let morale_enemy = if stacks.is_empty() {
        0
    } else {
        stacks.iter().map(|s| u32::from(s.stats.morale) * s.count).sum::<u32>() / field.max(1)
    };
    let mut won = field == 0;
    if field > 0 {
        for _ in 0..rules.max_rounds {
            round(defs, &mut own, &mut stacks, 0, true);
            let left_enemy: u32 = stacks.iter().map(|s| s.count).sum();
            let left_own = u32::try_from(own.iter().filter(|o| o.hp > 0).count()).unwrap_or(0);
            if breaks(field, left_enemy, morale_enemy) || left_enemy == 0 {
                won = true;
                break;
            }
            if breaks(start_own, left_own, morale_own) || left_own == 0 {
                break;
            }
        }
    }
    let enemy_dead = field - stacks.iter().map(|s| s.count).sum::<u32>();
    let own_dead = apply_own(state, defs, &own, won, events);
    // The kingdom's losses and the loot.
    let mut loot = 0i64;
    let war_rules = &defs.rules().diplomacy;
    if let Some(k) = state.diplomacy.kingdom_mut(kingdom) {
        k.soldiers = k.soldiers.saturating_sub(enemy_dead);
        k.population = k.population.saturating_sub(enemy_dead / 2).max(war_rules.start_population_min / 2).max(1);
        if won {
            loot = k.treasury().max(0) * i64::from(rules.loot_percent) / 100;
            k.treasury_milli -= loot * MILLI_PER_GOLD;
            if let Some(w) = &mut k.war {
                w.exhaustion = w.exhaustion.saturating_add(rules.exhaustion_victory);
            }
            crate::diplomacy::relations::remember(k, defs, MemoryCause::BattleLost, f32::from(defs.rules().military.armies.relation_defeat), Some(30));
        }
    }
    if loot > 0 {
        state.economy.book_gold(LedgerCategory::Military, loot);
    }
    let outcome = if won { BattleOutcome::Victory } else if own_dead == start_own { BattleOutcome::Defeat } else { BattleOutcome::Draw };
    let outcome = if !won && enemy_dead * 2 < field { BattleOutcome::Defeat } else { outcome };
    let name = kingdom_name(state, kingdom);
    let mut text = if field == 0 {
        format!("Feldzug gegen {name}: Kein Heer stellte sich entgegen.")
    } else {
        format!(
            "Feldschlacht gegen {name}: {} gegen {}. {}",
            count_text(start_own, "Soldat", "Soldaten"),
            count_text(field, "Feind", "Feinde"),
            fallen_text(own_dead, enemy_dead)
        )
    };
    if loot > 0 {
        text.push_str(&format!(" Kriegsbeute: {loot} Gold."));
    }
    let camp = &mut state.military.campaigns[i];
    camp.tally.own_units = start_own;
    camp.tally.own_losses += own_dead;
    camp.tally.enemy_losses += enemy_dead;
    let (entry, tally, start) = (camp.entry, camp.tally.clone(), camp.started_tick);
    record(state, kingdom, (BattleKind::FieldBattle, outcome), field, entry, (tally, start), text.clone(), events);
    state.diplomacy.record(state.clock.tick, Some(kingdom), None, text);
    // Fallen soldiers leave the campaign list (they are gone from the military).
    let soldiers = &state.military.soldiers;
    state.military.campaigns[i].soldiers.retain(|s| soldiers.contains_key(s));
    crate::diplomacy::relations::announce_states(state, defs, events);
    won
}

/// The siege of the seat after a won field battle.
fn siege(state: &mut GameState, defs: &Defs, i: usize, events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().military.campaigns;
    let kingdom = state.military.campaigns[i].kingdom;
    let Some(k) = state.diplomacy.kingdom(kingdom) else { return };
    let mut walls = k.population.saturating_mul(u32::from(rules.seat_hp_per_inhabitant));
    let garrison = k.soldiers;
    let population = k.population;
    let mut stacks = enemy_stacks(state, defs, garrison);
    let mut own = own_soldiers(state, defs, i);
    let start_own = u32::try_from(own.len()).unwrap_or(0);
    let morale_own = if own.is_empty() { 0 } else { own.iter().map(|o| u32::from(o.stats.morale)).sum::<u32>() / start_own };
    let morale_enemy = if stacks.is_empty() { 0 } else { stacks.iter().map(|s| u32::from(s.stats.morale) * s.count).sum::<u32>() / garrison.max(1) };
    let battering: u32 = state.military.campaigns[i]
        .soldiers
        .iter()
        .filter_map(|s| state.military.soldiers.get(s))
        .filter(|s| s.crew_of.is_none())
        .map(|s| defs.units().get(s.unit.index()).map_or(0, |u| u32::from(u.building_damage)))
        .sum();
    let mut success = garrison == 0 && walls == 0;
    for _ in 0..rules.max_rounds.saturating_mul(2) {
        let broken = walls == 0;
        let wall_percent = if broken { 0 } else { u32::from(rules.wall_defense_percent) };
        round(defs, &mut own, &mut stacks, wall_percent, broken);
        walls = walls.saturating_sub(battering);
        let left_enemy: u32 = stacks.iter().map(|s| s.count).sum();
        let left_own = u32::try_from(own.iter().filter(|o| o.hp > 0).count()).unwrap_or(0);
        if walls == 0 && (left_enemy == 0 || breaks(garrison, left_enemy, morale_enemy)) {
            success = true;
            break;
        }
        if left_own == 0 || breaks(start_own, left_own, morale_own) {
            break;
        }
    }
    let enemy_dead = garrison - stacks.iter().map(|s| s.count).sum::<u32>();
    let own_dead = apply_own(state, defs, &own, success, events);
    if let Some(k) = state.diplomacy.kingdom_mut(kingdom) {
        k.soldiers = k.soldiers.saturating_sub(enemy_dead);
    }
    let name = kingdom_name(state, kingdom);
    let entry = state.military.campaigns[i].entry;
    let text = if success {
        let reparations = state
            .diplomacy
            .kingdom(kingdom)
            .map_or(0, |k| i64::from(population * u32::from(rules.siege_reparations_per_inhabitant)).min(k.treasury().max(0)));
        if let Some(k) = state.diplomacy.kingdom_mut(kingdom) {
            k.treasury_milli -= reparations * MILLI_PER_GOLD;
        }
        state.economy.book_gold(LedgerCategory::Diplomacy, reparations);
        crate::diplomacy::actions::make_peace(state, defs, kingdom, PeaceTerms::BorderCeded, events);
        format!(
            "Die Burg von {name} ist gefallen! {name} muss Frieden schließen, Land abtreten und {reparations} Gold zahlen. {}",
            fallen_text(own_dead, enemy_dead)
        )
    } else {
        format!("Die Belagerung von {name} ist gescheitert. {}", fallen_text(own_dead, enemy_dead))
    };
    let outcome = if success { BattleOutcome::Victory } else { BattleOutcome::Defeat };
    let camp = &mut state.military.campaigns[i];
    camp.tally.own_losses += own_dead;
    camp.tally.enemy_losses += enemy_dead;
    let (tally, start) = (camp.tally.clone(), camp.started_tick);
    record(state, kingdom, (BattleKind::Siege, outcome), garrison, entry, (tally, start), text.clone(), events);
    state.diplomacy.record(state.clock.tick, Some(kingdom), None, text);
    let soldiers = &state.military.soldiers;
    state.military.campaigns[i].soldiers.retain(|s| soldiers.contains_key(s));
}

/// Stores the report of a campaign battle (kind and outcome, enemies at the start, the
/// campaign's counters and start) and announces it.
#[allow(clippy::too_many_arguments)]
fn record(
    state: &mut GameState,
    kingdom: KingdomId,
    (kind, outcome): (BattleKind, BattleOutcome),
    enemies: u32,
    position: IVec2,
    (tally, start_tick): (Tally, u64),
    text: String,
    events: &mut Vec<SimEvent>,
) {
    let serial = state.military.record_battle(BattleReport {
        serial: 0,
        kind,
        start_tick,
        end_tick: state.clock.tick,
        position,
        kingdom: Some(kingdom),
        enemy: format!("Heer von {}", kingdom_name(state, kingdom)),
        enemy_units: enemies,
        tally,
        outcome,
        text: text.clone(),
    });
    events.push(SimEvent::BattleEnded { serial, kind, outcome });
    let severity = match outcome {
        BattleOutcome::Victory => Severity::Good,
        BattleOutcome::Defeat => Severity::Danger,
        BattleOutcome::Draw => Severity::Warning,
    };
    events.push(SimEvent::Notification(Notification::new(severity, text).with_kind(NotificationKind::Military).at(position)));
}
