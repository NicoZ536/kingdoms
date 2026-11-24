//! Combat (docs/game_design.md §11): fighter values, the damage formula and the deterministic
//! combat round on the map.
//!
//! **Values.** Every fighter has attack, defense and ranged values in milli-points: the unit's
//! values (bandits: `military.bandits`), for the player's soldiers raised by researched
//! `UnitBonus` effects and experience (up to `experience_attack_percent`), scaled by equipment
//! (50 % … 100 %) and morale (70 % … 100 %).
//!
//! **Damage.** One hit deals `attack × damage_factor / (defense + defense_offset)` hit points
//! (at least 1), ± `variance_percent` drawn from the military RNG. The attacker's class bonus
//! (spearmen against riders, riders against shooters) raises the attack; the defender's
//! position raises the defense: forest, higher ground than the attacker, next to an own wall
//! or gate (player), inside a building (garrison).
//!
//! **Round** (every `military.round_ticks` while enemies are on the map): all fighters are
//! collected — the enemies on the map, the player's ready soldiers on the map or inside a
//! garrison building, and the buildings that shoot (towers, archer posts, castle; shooters
//! inside them add range). First every shooter fires at the nearest enemy in range (distance
//! ≥ 2; buildings also at adjacent ones), the hits are applied at once and the dead fall; then
//! every fighter strikes the nearest enemy next to it (soldiers inside a building strike out of
//! it, enemies next to the building strike in). Enemies that stand at the building they
//! attack (gate, wall, headquarters) with no soldier next to them damage it instead. Order is
//! fixed (player before enemies, by id), targets are chosen deterministically (nearest, then
//! id), all damage of a phase is simultaneous.

use std::collections::BTreeMap;

use glam::IVec2;
use rand::RngExt;

use super::{CombatRound, FighterKind, Shot, ShotKind, Strike, StrikeTarget};
use crate::buildings::BuildingId;
use crate::defs::{ClassBonus, Defs, UnitClass, UnitTypeId};
use crate::event::SimEvent;
use crate::grid::TileRect;
use crate::population::{CitizenId, TravelerId, WalkerId};
use crate::state::GameState;
use crate::world::Vegetation;

/// Combat values of a fighter (milli-points; see the module documentation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FighterStats {
    /// Melee attack.
    pub attack: u32,
    /// Defense.
    pub defense: u32,
    /// Ranged attack (0: none).
    pub ranged: u32,
    /// Range in tiles.
    pub range: i32,
    /// Class.
    pub class: UnitClass,
    /// Extra attack against a class.
    pub bonus: Option<ClassBonus>,
    /// Damage per round against structures.
    pub building_damage: u32,
    /// Full health.
    pub max_hp: u16,
    /// Morale 0–100 (enemies: the base morale of their kind).
    pub morale: u16,
}

/// Strength points of a unit type for diplomacy: attack + defense + ranged.
pub fn unit_strength(defs: &Defs, unit: UnitTypeId) -> u32 {
    defs.units().get(unit.index()).map_or(0, |u| u32::from(u.attack) + u32::from(u.defense) + u32::from(u.ranged))
}

/// Base combat values of an enemy or a unit type; `player` adds researched unit bonuses.
pub fn fighter_stats(state: &GameState, defs: &Defs, kind: FighterKind, player: bool) -> FighterStats {
    match kind {
        FighterKind::Bandit => {
            let b = &defs.rules().military.bandits;
            FighterStats {
                attack: u32::from(b.attack) * 1000,
                defense: u32::from(b.defense) * 1000,
                ranged: 0,
                range: 0,
                class: UnitClass::Infantry,
                bonus: None,
                building_damage: u32::from(b.building_damage),
                max_hp: b.hit_points,
                morale: b.morale,
            }
        }
        FighterKind::Unit(unit) => {
            let Some(u) = defs.units().get(unit.index()) else {
                return FighterStats {
                    attack: 1000,
                    defense: 0,
                    ranged: 0,
                    range: 0,
                    class: UnitClass::Infantry,
                    bonus: None,
                    building_damage: 1,
                    max_hp: 100,
                    morale: 50,
                };
            };
            let (atk, def) = if player { state.research.unit_bonus_percent(defs, unit) } else { (0, 0) };
            let scale = |v: u16, percent: i32| -> u32 {
                u32::try_from(i64::from(v) * 1000 * i64::from(100 + percent).max(0) / 100).unwrap_or(0)
            };
            FighterStats {
                attack: scale(u.attack, atk),
                defense: scale(u.defense, def),
                ranged: scale(u.ranged, atk),
                range: i32::from(u.range),
                class: u.class,
                bonus: u.bonus,
                building_damage: u32::from(u.building_damage),
                max_hp: u.hit_points,
                morale: u.morale,
            }
        }
    }
}

/// Personal factors of a soldier: equipment (50 % … 100 %), morale (70 % … 100 %) and, for the
/// attack, experience; returns the scaled values.
pub(crate) fn personal(stats: FighterStats, defs: &Defs, equipment: u16, morale: u16, experience: u16) -> FighterStats {
    let eq = 500 + u64::from(equipment.min(1000)) / 2;
    let mo = 700 + 3 * u64::from(morale.min(100));
    let xp = 100 + u64::from(experience.min(100)) * u64::from(defs.rules().military.experience_attack_percent) / 100;
    let att = |v: u32| u32::try_from(u64::from(v) * eq / 1000 * mo / 1000 * xp / 100).unwrap_or(u32::MAX);
    FighterStats {
        attack: att(stats.attack),
        ranged: att(stats.ranged),
        defense: u32::try_from(u64::from(stats.defense) * eq / 1000).unwrap_or(u32::MAX),
        morale,
        ..stats
    }
}

/// Damage of one hit before the random spread: `attack × damage_factor / (defense +
/// defense_offset)`, at least 1 (values in milli-points).
pub(crate) fn hit_damage(defs: &Defs, attack: u32, defense: u32) -> u32 {
    let r = &defs.rules().military;
    let damage = u64::from(attack) * u64::from(r.damage_factor) / (u64::from(defense) + u64::from(r.defense_offset) * 1000);
    u32::try_from(damage.max(1)).unwrap_or(u32::MAX)
}

/// Attack against a target of class `target` (class bonus applied).
pub(crate) fn attack_against(base: u32, bonus: Option<ClassBonus>, target: UnitClass) -> u32 {
    match bonus {
        Some(b) if b.against == target => u32::try_from(u64::from(base) * (100 + u64::from(b.percent)) / 100).unwrap_or(u32::MAX),
        _ => base,
    }
}

/// Who takes part in a round.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Soldier(CitizenId),
    Tower(BuildingId),
    Fighter(TravelerId),
}

/// A fighter in a round.
#[derive(Clone, Debug)]
struct Combatant {
    key: Key,
    player: bool,
    /// Tile it stands on (inside a building: the building's area counts for distances).
    area: TileRect,
    inside: bool,
    stats: FighterStats,
    hp: u32,
    elevation: u8,
    forest: bool,
    by_wall: bool,
    /// Enemies: index of the warband and of the fighter in it.
    slot: Option<(usize, usize)>,
    can_melee: bool,
    /// Enemies: the building it attacks.
    attacking: Option<BuildingId>,
    fought: bool,
    kills: u16,
}

impl Combatant {
    fn alive(&self) -> bool {
        self.hp > 0
    }
}

/// Tile a walker counts as standing on for combat (inside a building: its door).
fn walker_tile(m: &crate::population::Motion) -> IVec2 {
    m.pos().tile()
}

fn elevation(state: &GameState, p: IVec2) -> u8 {
    state.world.elevation(p).unwrap_or(0)
}

fn forest(state: &GameState, p: IVec2) -> bool {
    state.world.get(p).is_some_and(|t| matches!(t.vegetation, Vegetation::Tree { .. }))
}

/// True if an own wall or gate is next to `p`.
fn by_wall(state: &GameState, defs: &Defs, p: IVec2) -> bool {
    (-1..=1).any(|dy| {
        (-1..=1).any(|dx| {
            state.buildings.at(p + IVec2::new(dx, dy)).is_some_and(|(_, b)| b.is_complete() && crate::buildings::is_wall_or_gate(defs, b))
        })
    })
}

/// Collects the fighters of a round.
fn collect(state: &GameState, defs: &Defs) -> Vec<Combatant> {
    let rules = &defs.rules().military;
    let mut out = Vec::new();
    // Player soldiers: ready, on the map or inside a garrison building, not away, not crew.
    for (id, s) in state.military.soldiers.iter() {
        if !s.is_available() || s.hp == 0 {
            continue;
        }
        let Some(c) = state.population.citizen(*id) else { continue };
        let m = c.motion();
        let (area, inside) = match m.inside().and_then(|b| state.buildings.get(b).map(|bb| (b, bb))) {
            Some((_, bb)) => {
                let garrison = defs.buildings().get(bb.kind().index()).is_some_and(|d| d.garrison > 0) && bb.is_complete();
                if !garrison {
                    continue;
                }
                (bb.rect(), true)
            }
            None => (TileRect::tile(walker_tile(m)), false),
        };
        let base = fighter_stats(state, defs, FighterKind::Unit(s.unit), true);
        let mut stats = personal(base, defs, s.equipment, s.morale, s.experience);
        if inside && stats.ranged > 0 {
            stats.range += i32::from(rules.garrison_range_bonus);
        }
        let tile = area.origin;
        out.push(Combatant {
            key: Key::Soldier(*id),
            player: true,
            area,
            inside,
            stats,
            hp: u32::from(s.hp),
            elevation: elevation(state, tile),
            forest: !inside && forest(state, tile),
            by_wall: !inside && by_wall(state, defs, tile),
            slot: None,
            can_melee: true,
            attacking: None,
            fought: false,
            kills: 0,
        });
    }
    // Buildings that shoot.
    for (bid, b) in state.buildings.iter() {
        if !b.is_operational() {
            continue;
        }
        let Some(d) = defs.buildings().get(b.kind().index()).and_then(|d| d.defense) else { continue };
        if d.ranged == 0 {
            continue;
        }
        let tile = b.rect().origin;
        out.push(Combatant {
            key: Key::Tower(bid),
            player: true,
            area: b.rect(),
            inside: true,
            stats: FighterStats {
                attack: 0,
                defense: 0,
                ranged: u32::from(d.ranged) * 1000,
                range: i32::from(d.range),
                class: UnitClass::Ranged,
                bonus: None,
                building_damage: 0,
                max_hp: 1,
                morale: 100,
            },
            hp: 1,
            elevation: elevation(state, tile).saturating_add(1),
            forest: false,
            by_wall: false,
            slot: None,
            can_melee: false,
            attacking: None,
            fought: false,
            kills: 0,
        });
    }
    // Enemies on the map.
    for (wi, w) in state.military.warbands.iter().enumerate() {
        if !w.stage.is_hostile() {
            continue;
        }
        for (fi, f) in w.fighters.iter().enumerate() {
            let Some(t) = state.population.walking.traveler(f.walker) else { continue };
            let tile = walker_tile(t.motion());
            out.push(Combatant {
                key: Key::Fighter(f.walker),
                player: false,
                area: TileRect::tile(tile),
                inside: false,
                stats: fighter_stats(state, defs, f.kind, false),
                hp: u32::from(f.hp),
                elevation: elevation(state, tile),
                forest: forest(state, tile),
                by_wall: false,
                slot: Some((wi, fi)),
                can_melee: true,
                attacking: f.attacking,
                fought: false,
                kills: 0,
            });
        }
    }
    out
}

/// Defense of `target` against `attacker` (position bonuses applied).
fn defense_of(defs: &Defs, target: &Combatant, attacker: &Combatant) -> u32 {
    let r = &defs.rules().military;
    let mut percent = 100u64;
    if target.forest {
        percent += u64::from(r.forest_defense_percent);
    }
    if target.elevation > attacker.elevation {
        percent += u64::from(r.height_defense_percent);
    }
    if target.by_wall {
        percent += u64::from(r.wall_defense_percent);
    }
    if target.inside {
        percent += u64::from(r.garrison_defense_percent);
    }
    u32::try_from(u64::from(target.stats.defense) * percent / 100).unwrap_or(u32::MAX)
}

/// Index of the nearest living enemy of `who` within `min..=max` tiles (ties: order in the
/// list, which is sorted by key).
fn nearest_enemy(list: &[Combatant], who: usize, min: i32, max: i32) -> Option<usize> {
    let me = &list[who];
    let mut best: Option<(i32, usize)> = None;
    for (i, other) in list.iter().enumerate() {
        if other.player == me.player || !other.alive() || matches!(other.key, Key::Tower(_)) {
            continue;
        }
        let d = me.area.distance_to_rect(&other.area);
        if d < min || d > max {
            continue;
        }
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| i)
}

/// One phase: every eligible fighter picks a target; all hits are applied at once. Returns the
/// kills and every hit (attacker, target), both as indices into `list`.
#[allow(clippy::type_complexity)]
fn phase(state: &mut GameState, defs: &Defs, list: &mut [Combatant], ranged: bool) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let variance = u32::from(defs.rules().military.variance_percent);
    let mut hits: Vec<(usize, usize, u32)> = Vec::new();
    for i in 0..list.len() {
        let me = &list[i];
        if !me.alive() {
            continue;
        }
        let target = if ranged {
            if me.stats.ranged == 0 {
                continue;
            }
            // Buildings also shoot at enemies right next to them; soldiers fight those in melee.
            let min = if matches!(me.key, Key::Tower(_)) { 0 } else { 2 };
            nearest_enemy(list, i, min, me.stats.range)
        } else {
            if !me.can_melee || me.stats.attack == 0 {
                continue;
            }
            nearest_enemy(list, i, 0, 1)
        };
        let Some(t) = target else { continue };
        let base = if ranged { me.stats.ranged } else { me.stats.attack };
        let attack = attack_against(base, me.stats.bonus, list[t].stats.class);
        let defense = defense_of(defs, &list[t], me);
        let mut damage = hit_damage(defs, attack, defense);
        if variance > 0 {
            let roll = state.military.rng.random_range(0..=2 * variance);
            damage = (damage * (100 - variance + roll) / 100).max(1);
        }
        hits.push((i, t, damage));
    }
    let mut kills = Vec::new();
    let pairs = hits.iter().map(|&(a, t, _)| (a, t)).collect();
    for (attacker, target, damage) in hits {
        list[attacker].fought = true;
        list[target].fought = true;
        let was_alive = list[target].alive();
        list[target].hp = list[target].hp.saturating_sub(damage);
        if was_alive && !list[target].alive() {
            list[attacker].kills = list[attacker].kills.saturating_add(1);
            kills.push((attacker, target));
        }
    }
    (kills, pairs)
}

/// Walker of a combatant (towers have none).
fn walker_of(key: Key) -> Option<WalkerId> {
    match key {
        Key::Soldier(c) => Some(WalkerId::Citizen(c)),
        Key::Fighter(t) => Some(WalkerId::Traveler(t)),
        Key::Tower(_) => None,
    }
}

/// A blow of an enemy against the building it attacks: attacker index, building, struck tile.
type StructureStrike = (usize, BuildingId, IVec2);

/// The presentation record of this round: the ranged hits as shots, the melee blows against
/// fighters and buildings as strikes, and everybody who attacked.
fn record_round(
    tick: u64,
    list: &[Combatant],
    shots: &[(usize, usize)],
    strikes: &[(usize, usize)],
    structure: &[StructureStrike],
) -> CombatRound {
    let shots_out = shots
        .iter()
        .map(|&(a, t)| Shot {
            from: list[a].area,
            to: list[t].area,
            kind: if list[a].stats.class == UnitClass::Siege { ShotKind::Stone } else { ShotKind::Arrow },
            player: list[a].player,
            shooter: walker_of(list[a].key),
        })
        .collect();
    let melee = strikes.iter().filter_map(|&(a, t)| {
        Some(Strike {
            attacker: walker_of(list[a].key)?,
            from: list[a].area,
            target: StrikeTarget::Walker(walker_of(list[t].key)?),
            to: list[t].area,
            player: list[a].player,
        })
    });
    let on_buildings = structure.iter().filter_map(|&(a, building, tile)| {
        Some(Strike {
            attacker: walker_of(list[a].key)?,
            from: list[a].area,
            target: StrikeTarget::Building { building, tile },
            to: TileRect::tile(tile),
            player: list[a].player,
        })
    });
    let strikes_out: Vec<Strike> = melee.chain(on_buildings).collect();
    let strikers = shots.iter().filter_map(|&(a, _)| walker_of(list[a].key)).chain(strikes_out.iter().map(|s| s.attacker)).collect();
    CombatRound { tick, shots: shots_out, strikes: strikes_out, strikers }
}

/// Runs one combat round (see the module documentation) and applies its results.
pub(crate) fn run_round(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let mut list = collect(state, defs);
    if !list.iter().any(|c| !c.player) {
        return;
    }
    let (mut kills, shots) = phase(state, defs, &mut list, true);
    let (melee_kills, strikes) = phase(state, defs, &mut list, false);
    kills.extend(melee_kills);
    // Enemies at the building they attack, with no soldier next to them, damage it.
    let mut structure_hits: BTreeMap<BuildingId, u32> = BTreeMap::new();
    let mut structure_strikes: Vec<StructureStrike> = Vec::new();
    for i in 0..list.len() {
        let c = &list[i];
        if c.player || !c.alive() {
            continue;
        }
        let Some(b) = c.attacking else { continue };
        let Some(rect) = state.buildings.get(b).map(|bb| bb.rect()) else { continue };
        if c.area.distance_to_rect(&rect) > 1 || nearest_enemy(&list, i, 0, 1).is_some() {
            continue;
        }
        *structure_hits.entry(b).or_insert(0) += c.stats.building_damage.max(1);
        structure_strikes.push((i, b, c.area.origin.clamp(rect.origin, rect.max_inclusive())));
    }
    state.military.last_round = record_round(state.clock.tick, &list, &shots, &strikes, &structure_strikes);
    apply_results(state, defs, &list, &kills, events);
    for (b, damage) in structure_hits {
        super::threats::damage_structure(state, defs, b, damage, events);
    }
}

/// Writes health, wear, experience and kills back; the dead die.
fn apply_results(state: &mut GameState, defs: &Defs, list: &[Combatant], kills: &[(usize, usize)], events: &mut Vec<SimEvent>) {
    let rules = &defs.rules().military;
    // Who killed whom: a fallen soldier counts for the warband of his killer.
    let mut killer_band: BTreeMap<Key, Option<usize>> = BTreeMap::new();
    for (a, t) in kills {
        killer_band.insert(list[*t].key, list[*a].slot.map(|(wi, _)| wi));
    }
    // Defenders who fought are listed in every warband on the map.
    let fighting_bands: Vec<usize> =
        list.iter().filter(|c| !c.player && c.fought).filter_map(|c| c.slot.map(|(wi, _)| wi)).collect();
    for c in list {
        match c.key {
            Key::Soldier(id) => {
                if c.fought {
                    for wi in &fighting_bands {
                        if let Some(w) = state.military.warbands.get_mut(*wi)
                            && let Err(pos) = w.defenders.binary_search(&id)
                        {
                            w.defenders.insert(pos, id);
                        }
                    }
                }
                let Some(s) = state.military.soldiers.get_mut(&id) else { continue };
                s.hp = u16::try_from(c.hp).unwrap_or(u16::MAX);
                if c.fought {
                    s.equipment = s.equipment.saturating_sub(rules.wear_per_round_permille);
                    let xp = u16::from(rules.experience_per_round) + c.kills * u16::from(rules.experience_per_kill);
                    s.experience = (s.experience + xp).min(100);
                    s.kills = s.kills.saturating_add(c.kills);
                }
            }
            Key::Fighter(walker) => {
                if let Some((wi, fi)) = c.slot
                    && let Some(f) = state.military.warbands.get_mut(wi).and_then(|w| w.fighters.get_mut(fi))
                    && f.walker == walker
                {
                    f.hp = u16::try_from(c.hp).unwrap_or(u16::MAX);
                }
            }
            Key::Tower(_) => {}
        }
    }
    // The dead: soldiers die as citizens, enemies drop their plunder and vanish.
    for c in list.iter().filter(|c| !c.alive()) {
        match c.key {
            Key::Soldier(id) => {
                if let Some(Some(wi)) = killer_band.get(&c.key)
                    && let Some(w) = state.military.warbands.get_mut(*wi)
                {
                    w.tally.own_losses += 1;
                }
                crate::population::die_in_battle(state, defs, id, events);
            }
            Key::Fighter(walker) => super::threats::fighter_falls(state, defs, walker, events),
            Key::Tower(_) => {}
        }
    }
}

#[cfg(test)]
pub(crate) fn test_hit(defs: &Defs, attack: u32, defense: u32) -> u32 {
    hit_damage(defs, attack, defense)
}
