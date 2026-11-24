//! Creation of the AI kingdoms at game start (docs/game_design.md §10) and the daily growth of
//! their territory and settlements.
//!
//! Every kingdom sits at one of the world generator's settlement sites (far from the player's
//! start and from each other). Its territory is the disc of its radius around the seat, minus
//! tiles the player or another kingdom already owns. Its settlement follows the town plan of
//! `kingdoms.ron`: the seat (a castle) at the centre, houses and workshops in the town, farms
//! and mines in the fields, watchtowers at the border. A structure needs flat, buildable land of
//! the kingdom without deposits or boulders, keeps one tile of space to the others (walkers pass
//! between them) and faces the seat; trees and bushes on its footprint are cleared. One
//! structure is added per day while the plan asks for more.

use glam::IVec2;
use rand::RngExt;

use super::{Diplomacy, Kingdom, KingdomId, Structure, economy};
use crate::defs::{Defs, LandSource, Personality, TownZone};
use crate::event::{Notification, Severity, SimEvent};
use crate::grid::{Direction, Rotation, TileRect};
use crate::state::GameState;
use crate::world::{DepositKind, Terrain, Tile, Vegetation};

/// Fertility from which grass counts as farmland (meadows always do; poorer grass, steppe and
/// dirt are pasture).
const FARMLAND_FERTILITY: u8 = 168;

/// Structures placed at game start at most per kingdom (the established town).
const START_STRUCTURES: usize = 48;

/// Candidates collected before one is drawn (spread of the settlement).
const CANDIDATES: usize = 10;

/// Number of AI kingdoms for a new game: by map size, at most the scenario's limit.
pub fn kingdom_count(defs: &Defs, size: crate::world::WorldSize, scenario_limit: u8) -> u8 {
    defs.rules().diplomacy.kingdoms.get(size).min(scenario_limit).min(7)
}

/// The land source of a tile (`None`: deep water, rock, snow, sand, swamp without anything).
pub(crate) fn land_source(tile: &Tile) -> Option<LandSource> {
    if let Some(d) = tile.deposit {
        match d.kind {
            DepositKind::Stone => return Some(LandSource::Stone),
            DepositKind::Clay => return Some(LandSource::Clay),
            DepositKind::Iron => return Some(LandSource::Iron),
            DepositKind::Coal => return Some(LandSource::Coal),
            DepositKind::Gold => return Some(LandSource::Gold),
            DepositKind::Sand | DepositKind::Fish => {}
        }
    }
    if tile.terrain == Terrain::ShallowWater || (tile.is_water() && tile.has_fresh_water()) {
        return Some(LandSource::Water);
    }
    if tile.vegetation.is_tree() || tile.terrain == Terrain::Forest {
        return Some(LandSource::Forest);
    }
    match tile.terrain {
        Terrain::Meadow => Some(LandSource::Farmland),
        Terrain::Grass if tile.fertility >= FARMLAND_FERTILITY => Some(LandSource::Farmland),
        Terrain::Grass | Terrain::Steppe | Terrain::Dirt => Some(LandSource::Pasture),
        _ => None,
    }
}

/// Square around the seat that contains every tile within `radius`.
fn disc_bounds(seat: IVec2, radius: u16) -> TileRect {
    let r = i32::from(radius);
    TileRect::new(seat - IVec2::splat(r), IVec2::splat(2 * r + 1))
}

/// True if tile `p` lies within `radius` of `seat` (tile centres).
fn in_disc(seat: IVec2, radius: u16, p: IVec2) -> bool {
    let d = p - seat;
    let r = i32::from(radius);
    d.x * d.x + d.y * d.y <= r * r
}

/// Creates the kingdoms at the world's settlement sites (game start, after the player's start
/// buildings): realms, personalities, people, territory, economy, an established settlement and
/// their relations to each other.
pub(crate) fn create_kingdoms(state: &mut GameState, defs: &Defs) {
    let data = defs.kingdom_data();
    let rules = &defs.rules().diplomacy;
    let sites: Vec<IVec2> = state.world.settlement_sites.clone();
    let n = sites.len().min(data.realms.len()).min(7);
    let mut realms: Vec<usize> = (0..data.realms.len()).collect();
    let mut drawn: Vec<Personality> = Vec::with_capacity(n);
    let tick = state.clock.tick;
    for (i, seat) in sites.into_iter().take(n).enumerate() {
        let rng = &mut state.diplomacy.rng;
        let realm = realms.remove(rng.random_range(0..realms.len()));
        // Personalities by weight; none more than twice.
        let options: Vec<(Personality, u32)> = Personality::ALL
            .iter()
            .filter(|p| drawn.iter().filter(|d| *d == *p).count() < 2)
            .map(|p| (*p, rules.personalities.get(*p).weight))
            .collect();
        let total: u32 = options.iter().map(|(_, w)| w).sum();
        let mut roll = rng.random_range(0..total.max(1));
        let mut personality = Personality::Peaceful;
        for (p, w) in &options {
            if roll < *w {
                personality = *p;
                break;
            }
            roll -= w;
        }
        drawn.push(personality);
        let population = rng.random_range(rules.start_population_min..=rules.start_population_max);
        let template = &data.realms[realm];
        let resources = defs.resources().len();
        let kingdom = Kingdom {
            name: template.name.clone(),
            ruler: template.ruler.clone(),
            title: template.title.clone(),
            personality,
            color: u8::try_from(i).unwrap_or(0),
            seat,
            radius: target_radius(defs, population, 0),
            ceded: 0,
            population,
            treasury_milli: i64::from(population) * i64::from(rules.start_gold_per_inhabitant) * crate::economy::MILLI_PER_GOLD,
            stock_milli: vec![0; resources],
            production_milli: vec![0; resources],
            farm_milli: vec![0; resources],
            land: vec![0; LandSource::ALL.len()],
            territory_tiles: 0,
            soldiers: 0,
            food_percent: 100,
            memories: Vec::new(),
            treaties: Vec::new(),
            war: None,
            structures: Vec::new(),
            borders_player: false,
            last_state: super::RelationState::Neutral,
            next_caravan_tick: tick + crate::time::TICKS_PER_HOUR * u64::from(rng.random_range(6..30u32)),
            next_initiative_day: u64::from(rules.ai.first_initiative_day) + i as u64,
            next_demand_day: u64::from(rules.ai.demand_first_day),
        };
        state.diplomacy.kingdoms.push(kingdom);
        let id = KingdomId::from_raw(u8::try_from(i).unwrap_or(0));
        claim_territory(state, defs, id);
        survey_land(state, id);
        economy::refresh_production(state, defs, id);
        if let Some(k) = state.diplomacy.kingdom_mut(id) {
            k.soldiers = economy::soldier_target(defs, k);
            // An established kingdom: stores for a few days of consumption and production.
            for c in &data.consumption {
                let daily = economy::consumption_milli(c, k.population);
                k.stock_milli[c.resource.index()] = daily * 6;
            }
            for (slot, prod) in k.stock_milli.iter_mut().zip(&k.production_milli) {
                *slot += u64::from(*prod) * 4;
            }
        }
        let mut failed = Vec::new();
        for _ in 0..START_STRUCTURES {
            if !add_structure(state, defs, id, &mut failed) {
                break;
            }
        }
    }
    let n = state.diplomacy.kingdoms.len();
    state.diplomacy.ai_relations = vec![0; n * n];
    for a in 0..n {
        for b in (a + 1)..n {
            let (ka, kb) = (KingdomId::from_raw(a as u8), KingdomId::from_raw(b as u8));
            let base = ai_base_relation(defs, &state.diplomacy, ka, kb);
            let jitter = state.diplomacy.rng.random_range(-10..=10);
            state.diplomacy.set_ai_relation(ka, kb, base + jitter);
        }
    }
    refresh_borders(state);
    for i in 0..n {
        let id = KingdomId::from_raw(i as u8);
        let now = super::relations::relation_state(state, defs, id);
        state.diplomacy.kingdoms[i].last_state = now;
    }
    log::info!("{n} AI kingdoms created");
}

/// Target relation between two AI kingdoms: the mean of their attitudes, less while their
/// territories touch.
pub(crate) fn ai_base_relation(defs: &Defs, d: &Diplomacy, a: KingdomId, b: KingdomId) -> i32 {
    let rules = &defs.rules().diplomacy;
    let (Some(ka), Some(kb)) = (d.kingdom(a), d.kingdom(b)) else { return 0 };
    let base = (i32::from(rules.personalities.get(ka.personality).base_relation)
        + i32::from(rules.personalities.get(kb.personality).base_relation))
        / 2;
    let touching = kingdoms_touch(d, a, b);
    base - if touching { 20 } else { 0 }
}

/// True if the territories of two kingdoms touch.
pub(crate) fn kingdoms_touch(d: &Diplomacy, a: KingdomId, b: KingdomId) -> bool {
    let (Some(ka), Some(kb)) = (d.kingdom(a), d.kingdom(b)) else { return false };
    let reach = i32::from(ka.radius) + i32::from(kb.radius) + 1;
    if (ka.seat - kb.seat).abs().max_element() > reach {
        return false;
    }
    let area = disc_bounds(ka.seat, ka.radius).clipped(d.land.grid_size().x, d.land.grid_size().y);
    area.tiles().any(|p| d.land.owner(p) == Some(a) && Direction::ALL.iter().any(|dir| d.land.owner(p + dir.offset()) == Some(b)))
}

/// Territory radius for `population` inhabitants after `ceded` tiles lost in wars.
pub(crate) fn target_radius(defs: &Defs, population: u32, ceded: u16) -> u16 {
    let rules = &defs.rules().diplomacy;
    let grown = u32::from(rules.base_radius) + population / u32::from(rules.inhabitants_per_radius).max(1);
    let r = u16::try_from(grown).unwrap_or(u16::MAX).min(rules.max_radius);
    r.saturating_sub(ceded).max(6)
}

/// Releases the kingdom's tiles outside its radius and claims the free tiles inside it (not
/// owned by the player or another kingdom); structures that no longer stand on its land are
/// removed. Returns true if the territory changed.
pub(crate) fn claim_territory(state: &mut GameState, defs: &Defs, id: KingdomId) -> bool {
    let Some(k) = state.diplomacy.kingdom(id) else { return false };
    let (seat, radius) = (k.seat, k.radius);
    let size = state.world.size();
    let area = disc_bounds(seat, radius + 4).clipped(size.x, size.y);
    let mut changed = false;
    for p in area.tiles() {
        let owner = state.diplomacy.land.owner(p);
        let inside = in_disc(seat, radius, p);
        if owner == Some(id) && !inside {
            state.diplomacy.land.set_owner(p, None);
            changed = true;
        } else if owner.is_none() && inside && !state.territory.contains(p) {
            state.diplomacy.land.set_owner(p, Some(id));
            changed = true;
        }
    }
    if changed {
        drop_lost_structures(state, defs, id);
        let tiles = state.diplomacy.land.count_owned(id);
        if let Some(k) = state.diplomacy.kingdom_mut(id) {
            k.territory_tiles = tiles;
        }
    }
    changed
}

/// Removes structures of `id` with a tile outside its land.
fn drop_lost_structures(state: &mut GameState, defs: &Defs, id: KingdomId) {
    let Some(k) = state.diplomacy.kingdoms.get(id.index()) else { return };
    let land = &state.diplomacy.land;
    let lost: Vec<usize> = k
        .structures
        .iter()
        .enumerate()
        .filter(|(_, s)| s.rect(defs).tiles().any(|p| land.owner(p) != Some(id)))
        .map(|(i, _)| i)
        .collect();
    for i in lost.into_iter().rev() {
        let s = state.diplomacy.kingdoms[id.index()].structures.remove(i);
        state.diplomacy.land.set_structure(s.rect(defs), false);
    }
}

/// Counts the land sources of the kingdom's territory.
pub(crate) fn survey_land(state: &mut GameState, id: KingdomId) {
    let Some(k) = state.diplomacy.kingdom(id) else { return };
    let size = state.world.size();
    let area = disc_bounds(k.seat, k.radius).clipped(size.x, size.y);
    let mut land = vec![0u32; LandSource::ALL.len()];
    for p in area.tiles() {
        if state.diplomacy.land.owner(p) != Some(id) {
            continue;
        }
        if let Some(source) = state.world.get(p).and_then(land_source) {
            land[source.index()] += 1;
        }
    }
    if let Some(k) = state.diplomacy.kingdom_mut(id) {
        k.land = land;
    }
}

/// Recomputes which kingdoms' territories touch the player's.
pub(crate) fn refresh_borders(state: &mut GameState) {
    let size = state.world.size();
    for i in 0..state.diplomacy.kingdoms.len() {
        let id = KingdomId::from_raw(i as u8);
        let k = &state.diplomacy.kingdoms[i];
        let area = disc_bounds(k.seat, k.radius + 1).clipped(size.x, size.y);
        let land = &state.diplomacy.land;
        let territory = &state.territory;
        let touching =
            area.tiles().any(|p| land.owner(p) == Some(id) && Direction::ALL.iter().any(|d| territory.contains(p + d.offset())));
        state.diplomacy.kingdoms[i].borders_player = touching;
    }
}

/// Daily growth: the territory follows the population (one tile of radius per day), new land
/// is surveyed, one structure is added if the town plan asks for more, and the shared borders
/// with the player are refreshed (a new one is announced).
pub(crate) fn grow_settlements(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    for i in 0..state.diplomacy.kingdoms.len() {
        let id = KingdomId::from_raw(i as u8);
        let k = &state.diplomacy.kingdoms[i];
        let target = target_radius(defs, k.population, k.ceded);
        let radius = k.radius;
        let new_radius = if target > radius { radius + 1 } else { target };
        if new_radius != radius {
            state.diplomacy.kingdoms[i].radius = new_radius;
            if claim_territory(state, defs, id) {
                survey_land(state, id);
                let tiles = state.diplomacy.kingdoms[i].territory_tiles;
                events.push(SimEvent::KingdomBorderChanged { kingdom: id, tiles });
            }
        }
        add_structure(state, defs, id, &mut Vec::new());
    }
    let before: Vec<bool> = state.diplomacy.kingdoms.iter().map(|k| k.borders_player).collect();
    refresh_borders(state);
    for (i, was) in before.into_iter().enumerate() {
        let k = &state.diplomacy.kingdoms[i];
        if k.borders_player && !was {
            let text = format!("Die Grenzen von {} reichen nun bis an Euer Gebiet.", k.name);
            let seat = k.seat;
            state.diplomacy.record(tick, Some(KingdomId::from_raw(i as u8)), None, text.clone());
            events.push(SimEvent::Notification(Notification::new(Severity::Info, text).at(seat)));
        }
    }
}

/// Indices of the town plan entries the kingdom lacks a structure of, in plan order.
fn missing_entries(defs: &Defs, k: &Kingdom) -> Vec<usize> {
    let min_tiles = u32::from(defs.rules().diplomacy.craft_min_tiles);
    defs.kingdom_data()
        .town_plan
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            let have = k.structures.iter().filter(|s| s.kind == e.building).count();
            let allowed = e.needs.is_none_or(|n| k.land.get(n.index()).copied().unwrap_or(0) >= min_tiles);
            allowed && have < usize::from(e.target(k.population))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Distance band (in tiles from the seat) of a zone for territory radius `r`.
fn zone_band(zone: TownZone, r: i32) -> (i32, i32) {
    match zone {
        TownZone::Center => (0, 6),
        TownZone::Town => (3, (r / 2).max(5)),
        TownZone::Fields => ((r / 3).max(4), (r - 3).max(5)),
        TownZone::Border => ((r - 4).max(3), (r - 1).max(4)),
    }
}

/// The rotation whose front faces the seat from a footprint centred at `center`.
fn facing_seat(center: IVec2, seat: IVec2) -> Rotation {
    let d = seat - center;
    let quarter = if d.x.abs() > d.y.abs() {
        if d.x > 0 { 3 } else { 1 }
    } else if d.y >= 0 {
        0
    } else {
        2
    };
    Rotation::wrapping(quarter)
}

/// True if a structure of `rect` may stand there for kingdom `id`.
fn spot_ok(state: &GameState, id: KingdomId, rect: TileRect, entrance: IVec2) -> bool {
    let land = &state.diplomacy.land;
    let mut level = None;
    for p in rect.tiles() {
        let Some(tile) = state.world.get(p) else { return false };
        if land.owner(p) != Some(id)
            || tile.is_water()
            || !tile.terrain.is_buildable()
            || tile.deposit.is_some()
            || matches!(tile.vegetation, Vegetation::Boulder)
            || state.buildings.at(p).is_some()
            || state.roads.has_road(p)
        {
            return false;
        }
        if *level.get_or_insert(tile.elevation) != tile.elevation {
            return false;
        }
    }
    // One tile of space to every other structure, and a usable entrance.
    if rect.expanded(1).tiles().any(|p| land.has_structure(p)) {
        return false;
    }
    state.world.get(entrance).is_some_and(|t| !t.is_water()) && land.owner(entrance) == Some(id)
}

/// Adds a structure of the first town plan entry the kingdom lacks one of and that finds a
/// spot (entries in `failed` are skipped; entries without a spot are added to it). Returns true
/// if one was added.
pub(crate) fn add_structure(state: &mut GameState, defs: &Defs, id: KingdomId, failed: &mut Vec<usize>) -> bool {
    let Some(k) = state.diplomacy.kingdom(id) else { return false };
    for index in missing_entries(defs, k) {
        if failed.contains(&index) {
            continue;
        }
        if place_entry(state, defs, id, index) {
            return true;
        }
        failed.push(index);
    }
    false
}

/// Places one structure of town plan entry `index` for kingdom `id`.
fn place_entry(state: &mut GameState, defs: &Defs, id: KingdomId, index: usize) -> bool {
    let Some(entry) = defs.kingdom_data().town_plan.get(index).copied() else { return false };
    let Some(def) = defs.buildings().get(entry.building.index()) else { return false };
    let Some(k) = state.diplomacy.kingdom(id) else { return false };
    let (seat, r) = (k.seat, i32::from(k.radius));
    let (near, far) = zone_band(entry.zone, r);
    let sources: Vec<IVec2> = match entry.needs {
        Some(n) if !matches!(n, LandSource::Farmland | LandSource::Pasture | LandSource::Forest) => {
            let size = state.world.size();
            disc_bounds(seat, k.radius)
                .clipped(size.x, size.y)
                .tiles()
                .filter(|p| state.diplomacy.land.owner(*p) == Some(id) && state.world.get(*p).and_then(land_source) == Some(n))
                .collect()
        }
        _ => Vec::new(),
    };
    let mut candidates: Vec<(TileRect, Rotation)> = Vec::new();
    let mut near_source: Vec<(TileRect, Rotation)> = Vec::new();
    for offset in crate::start::spiral(far) {
        let center = seat + offset;
        let dist2 = offset.x * offset.x + offset.y * offset.y;
        if dist2 < near * near || dist2 > far * far {
            continue;
        }
        let rotation = facing_seat(center, seat);
        let size = rotation.footprint_size(def.size);
        let origin = center - size / 2;
        let rect = TileRect::new(origin, size);
        let entrance = rotation.entrance(origin, def.size);
        if !spot_ok(state, id, rect, entrance) {
            continue;
        }
        if !sources.is_empty() && sources.iter().any(|s| rect.distance_to(*s) <= 4) {
            near_source.push((rect, rotation));
        }
        candidates.push((rect, rotation));
        if candidates.len() >= CANDIDATES * 4 && (sources.is_empty() || near_source.len() >= CANDIDATES) {
            break;
        }
    }
    let pool = if near_source.is_empty() { candidates } else { near_source };
    // The seat stands at the first spot; other structures spread over the nearest candidates.
    let take = if entry.zone == TownZone::Center { pool.len().min(1) } else { CANDIDATES.min(pool.len()) };
    let mut order: Vec<usize> = (0..take).collect();
    while !order.is_empty() {
        let pick = order.remove(state.diplomacy.rng.random_range(0..order.len()));
        let (rect, rotation) = pool[pick];
        let mut tiles: Vec<IVec2> = rect.tiles().collect();
        tiles.sort_by_key(|p| (p.y, p.x));
        if crate::population::movement::walker_on(state, &tiles).is_some() {
            continue;
        }
        crate::buildings::clear_vegetation(&mut state.world, rect);
        state.diplomacy.land.set_structure(rect, true);
        if let Some(k) = state.diplomacy.kingdom_mut(id) {
            k.structures.push(Structure { kind: entry.building, origin: rect.origin, rotation });
        }
        return true;
    }
    false
}
