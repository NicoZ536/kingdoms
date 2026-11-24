//! Scale measurement (master spec §18, docs/architecture.md §3.6): a large generated map with
//! AI kingdoms, a planned town of about 5000 citizens in more than 500 buildings, carriers,
//! caravans, soldiers and an enemy army that fights them. Run with
//! `cargo test --release -p kingdoms_sim scale_performance -- --ignored --nocapture`.

use std::time::Instant;

use glam::IVec2;

use crate::command::Command;
use crate::defs::{Defs, Personality};
use crate::diplomacy::{DiplomaticAction, KingdomId};
use crate::economy::TradeOrder;
use crate::grid::{Rotation, TileRect};
use crate::military::{DefendTarget, Order};
use crate::roads::RoadTile;
use crate::save::SaveHeader;
use crate::state::{GameSettings, GameState};
use crate::test_support::defs;
use crate::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
use crate::world::{Biome, Terrain, Vegetation, WorldSize};

/// Edge of a town block: a road line every 16 tiles, nine building slots of 4×4 tiles inside.
const BLOCK: i32 = 16;
/// Blocks per side of the town.
const BLOCKS: i32 = 11;
/// Slot offsets inside a block (4×4 cells with one-tile gaps for the entrances).
const SLOTS: [i32; 3] = [1, 6, 11];

/// What a block holds (by a fixed pattern over the town).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BlockKind {
    Homes,
    Mixed,
    Industry,
    Food,
    Farms,
}

const PATTERN: [BlockKind; 20] = {
    use BlockKind::*;
    [Homes, Industry, Homes, Mixed, Homes, Industry, Homes, Homes, Mixed, Industry, Homes, Food, Homes, Mixed, Industry, Homes, Farms, Homes, Mixed, Industry]
};

const HOUSES: [&str; 3] = ["farmhouse", "citizen_house", "townhouse"];
const WORKSHOPS: [&str; 11] = ["sawmill", "brickworks", "weaver", "tailor", "carpenter", "smithy", "pottery", "tannery", "bakery", "mill", "brewery"];

fn rss_kib(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with(field)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}

/// Places a finished building if its footprint and entrance are free, dry land of the player
/// (no road, no building, no AI land).
fn place(state: &mut GameState, d: &Defs, key: &str, origin: IVec2) -> Option<crate::buildings::BuildingId> {
    let kind = d.building_id(key)?;
    let def = d.building(kind);
    // Bottom-aligned in its 4×4 cell: the entrance lies on the gap row (a path).
    let origin = origin + IVec2::new(0, 4 - def.size.y);
    let rect: TileRect = Rotation::default().footprint(origin, def.size);
    let free = |p: IVec2| {
        state.world.in_bounds(p) && !state.diplomacy.land.is_foreign(p) && state.buildings.at(p).is_none() && !state.world.tile(p).terrain.is_water()
    };
    let entrance = Rotation::default().entrance(origin, def.size);
    if !rect.tiles().all(|p| free(p) && !state.roads.has_road(p)) || !free(entrance) || !state.roads.has_road(entrance) {
        return None;
    }
    crate::buildings::spawn_finished(state, d, kind, origin, Rotation::default())
}

struct Town {
    region: TileRect,
    barracks: crate::buildings::BuildingId,
    post: crate::buildings::BuildingId,
    hq: crate::buildings::BuildingId,
    counts: std::collections::BTreeMap<&'static str, usize>,
}

/// Flattens the land around the start (except AI land), lays a road grid and fills the blocks.
fn build_town(state: &mut GameState, d: &Defs) -> Town {
    let hq = crate::economy::storage::main_headquarters(state, d).expect("headquarters");
    let hq_entrance = state.buildings.get(hq).expect("hq").entrance();
    let elevation = state.world.tile(hq_entrance).elevation;
    let size = BLOCK * BLOCKS;
    let (w, h) = (state.world.width(), state.world.height());
    let half = size / 2;
    let origin = IVec2::new((hq_entrance.x - half).clamp(4, w - size - 5), (hq_entrance.y - half).clamp(4, h - size - 5));
    let region = TileRect::new(origin, IVec2::splat(size + 1));
    for p in region.tiles() {
        if state.diplomacy.land.is_foreign(p) || state.buildings.at(p).is_some() {
            continue;
        }
        let t = state.world.tile_mut(p);
        t.terrain = Terrain::Grass;
        t.biome = Biome::Grassland;
        t.elevation = elevation;
        t.vegetation = Vegetation::None;
        t.deposit = None;
        t.river = false;
    }
    let path = d.road_id("path").expect("path");
    let paved = d.road_id("paved").unwrap_or(path);
    for i in 0..=size {
        for line in (0..=size).step_by(BLOCK as usize) {
            for p in [origin + IVec2::new(i, line), origin + IVec2::new(line, i)] {
                if state.buildings.at(p).is_none() && !state.diplomacy.land.is_foreign(p) {
                    let road = if (line / BLOCK) % 2 == 0 { paved } else { path };
                    state.roads.set(p, Some(RoadTile { road, bridge_axis: None }));
                }
            }
        }
    }
    // Every gap row between the slots is a path (the entrances lie on them).
    for by in 0..BLOCKS {
        for row in [5, 10, 15] {
            for x in 1..size {
                let p = origin + IVec2::new(x, by * BLOCK + row);
                if state.buildings.at(p).is_none() && !state.diplomacy.land.is_foreign(p) && !state.roads.has_road(p) {
                    state.roads.set(p, Some(RoadTile { road: path, bridge_axis: None }));
                }
            }
        }
    }
    state.roads.rebuild_components();
    let mut counts = std::collections::BTreeMap::new();
    let mut add = |state: &mut GameState, key: &'static str, at: IVec2| -> Option<crate::buildings::BuildingId> {
        let id = place(state, d, key, at)?;
        *counts.entry(key).or_insert(0) += 1;
        Some(id)
    };
    let center = IVec2::splat(BLOCKS / 2);
    let mut barracks = None;
    let mut post = None;
    let mut trading_post = None;
    let (mut houses, mut shops) = (0usize, 0usize);
    for by in 0..BLOCKS {
        for bx in 0..BLOCKS {
            let block = origin + IVec2::new(bx, by) * BLOCK;
            let slot = |k: usize| block + IVec2::new(SLOTS[k % 3], SLOTS[k / 3]);
            let kind = PATTERN[((bx * 3 + by * 7) as usize) % PATTERN.len()];
            let at_center = IVec2::new(bx, by) == center + IVec2::new(1, 0);
            for k in 0..9 {
                let key: &'static str = if at_center {
                    match k {
                        0 => "barracks",
                        1 => "archer_post",
                        2 => "trading_post",
                        3 | 5 => "watchtower",
                        _ => "warehouse",
                    }
                } else {
                    match (kind, k) {
                        (BlockKind::Homes, 4) if (bx + by) % 2 == 0 => "market",
                        (BlockKind::Homes, 4) => "well",
                        (BlockKind::Homes, _) => {
                            houses += 1;
                            HOUSES[houses % HOUSES.len()]
                        }
                        (BlockKind::Mixed, 0) => "market",
                        (BlockKind::Mixed, 2) => "church",
                        (BlockKind::Mixed, 6) => "tavern",
                        (BlockKind::Mixed, 8) => "school",
                        (BlockKind::Mixed, _) => {
                            houses += 1;
                            HOUSES[houses % HOUSES.len()]
                        }
                        (BlockKind::Industry, 4) => "warehouse",
                        (BlockKind::Industry, _) => {
                            shops += 1;
                            WORKSHOPS[shops % WORKSHOPS.len()]
                        }
                        (BlockKind::Food, 0 | 8) => "storehouse",
                        (BlockKind::Food, 4) => "market",
                        (BlockKind::Food, 1) => "chicken_farm",
                        (BlockKind::Food, _) => ["bakery", "mill", "cheese_dairy", "apiary"][k % 4],
                        (BlockKind::Farms, 0) => "vegetable_farm",
                        (BlockKind::Farms, 4) => "watchtower",
                        (BlockKind::Farms, 8) => "wheat_farm",
                        (BlockKind::Farms, _) => continue,
                    }
                };
                let id = add(state, key, slot(k));
                match (key, id) {
                    ("barracks", Some(b)) => barracks = Some(b),
                    ("archer_post", Some(b)) => post = Some(b),
                    ("trading_post", Some(b)) => trading_post = Some(b),
                    _ => {}
                }
            }
        }
    }
    // Every storage starts full; the treasury pays wages and soldiers for days.
    for (_, b) in state.buildings.iter_mut() {
        if d.building(b.kind()).storage.is_some() {
            for r in d.resources() {
                b.inventory.add(r.id, 300);
            }
        }
    }
    state.economy.add_gold(5_000_000);
    state.territory.recompute(&state.buildings, d, &state.diplomacy.land);
    crate::economy::storage::refresh_stock(state, d);
    crate::navigation::rebuild(state, d);
    assert!(trading_post.is_some(), "trading post placed");
    Town { region, barracks: barracks.expect("barracks placed"), post: post.expect("archer post placed"), hq, counts }
}

#[derive(Default)]
struct Steps {
    names: Vec<&'static str>,
    sum: Vec<u128>,
    max: Vec<u128>,
}

impl Steps {
    fn add(&mut self, timed: &[(&'static str, u128)]) {
        if self.names.is_empty() {
            self.names = timed.iter().map(|(n, _)| *n).collect();
            self.sum = vec![0; timed.len()];
            self.max = vec![0; timed.len()];
        }
        for (i, (_, ns)) in timed.iter().enumerate() {
            self.sum[i] += ns;
            self.max[i] = self.max[i].max(*ns);
        }
    }
}

fn percentile(sorted: &[u128], p: usize) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[(sorted.len() * p / 100).min(sorted.len() - 1)]
}

#[test]
#[ignore = "performance measurement; run in release mode"]
fn scale_performance() {
    let d = defs();
    let rss0 = rss_kib("VmRSS:");
    let started = Instant::now();
    let mut state = GameState::new(d, GameSettings::new(d, 7, WorldSize::Large));
    for key in ["militia_training", "weapon_smithing", "archery"] {
        if let Some(t) = d.tech_id(key) {
            state.research.mark_researched(t);
        }
    }
    let town = build_town(&mut state, d);
    crate::population::test_populate(&mut state, d, 5000);
    let enlisted = {
        let sword = d.unit_id("swordsman").expect("swordsman");
        let archer = d.unit_id("archer").expect("archer");
        let mut ids = crate::military::test_enlist(&mut state, d, 90, sword, town.barracks);
        ids.extend(crate::military::test_enlist(&mut state, d, 40, archer, town.post));
        ids
    };
    let setup = started.elapsed();
    // Caravans: standing orders and friendly kingdoms that send caravans at once.
    for r in d.resources() {
        let order = TradeOrder { resource: r.id, buy_below: Some(20_000), sell_above: None, daily_limit: 60 };
        let events = state.tick(d, vec![Command::SetTradeOrder { order }]);
        assert!(crate::test_support::rejection(&events).is_none(), "{:?}", crate::test_support::rejection(&events));
    }
    let kingdoms = state.diplomacy.len();
    for i in 1..kingdoms {
        let id = KingdomId::from_index(i).expect("kingdom");
        state.tick(d, vec![Command::DebugSetRelation { kingdom: id, value: 40 }]);
        state.diplomacy.kingdoms[i].next_caravan_tick = 0;
    }
    // One aggressive kingdom is at war with the player.
    let enemy = KingdomId::from_index(0).expect("kingdom");
    state.diplomacy.kingdoms[0].personality = Personality::Aggressive;
    state.diplomacy.kingdoms[0].soldiers = 300;
    state.tick(d, vec![Command::Diplomacy { kingdom: enemy, action: DiplomaticAction::DeclareWar }]);
    // Companies of at most 40 defend the town around the headquarters.
    let hq = state.buildings.get(town.hq).expect("hq").entrance();
    for chunk in enlisted.chunks(40) {
        let events = state.tick(d, vec![Command::CreateCompany { soldiers: chunk.to_vec() }]);
        assert!(crate::test_support::rejection(&events).is_none(), "{:?}", crate::test_support::rejection(&events));
    }
    let companies: Vec<_> = state.military.companies().iter().map(|c| c.id()).collect();
    for (i, c) in companies.into_iter().enumerate() {
        let order = Order::Defend(DefendTarget::Area { center: hq + IVec2::new(8 * i as i32 - 12, 4), radius: 12 });
        let events = state.tick(d, vec![Command::CompanyOrder { company: c, order }]);
        assert!(crate::test_support::rejection(&events).is_none(), "{:?}", crate::test_support::rejection(&events));
    }
    // Warm-up: everybody leaves home, the first carriers start.
    for _ in 0..TICKS_PER_HOUR {
        state.tick(d, Vec::new());
    }
    eprintln!(
        "setup {setup:?}: map {}×{}, town {:?}, {} buildings ({:?}), {} citizens, {} households, {} soldiers, {} kingdoms",
        state.world.width(),
        state.world.height(),
        town.region,
        state.buildings.len(),
        town.counts,
        state.population.len(),
        state.population.households.len(),
        state.military.soldiers().count(),
        kingdoms,
    );
    let ticks = 3 * TICKS_PER_DAY;
    let mut steps = Steps::default();
    let mut totals = Vec::with_capacity(ticks as usize);
    let (mut peak_walkers, mut peak_enemies, mut peak_caravans, mut peak_queue) = (0usize, 0usize, 0usize, 0usize);
    let mut worst_at = 0;
    let mut worst = Vec::new();
    let mut queued_since = std::collections::BTreeMap::new();
    let (mut max_wait, mut sum_wait, mut waits) = (0u64, 0u64, 0u64);
    let army_at = state.clock.tick + 26 * TICKS_PER_HOUR;
    for _ in 0..ticks {
        if state.clock.tick == army_at {
            state.tick(d, vec![Command::DebugSpawnThreat { kingdom: Some(enemy), size: 120 }]);
            continue;
        }
        let timed = state.tick_timed(d);
        let total: u128 = timed.iter().map(|(_, ns)| ns).sum();
        if totals.iter().all(|t| *t < total) {
            worst_at = state.clock.tick;
        }
        totals.push(total);
        steps.add(&timed);
        worst.push((total, state.clock.tick, timed.iter().filter(|(_, ns)| *ns > 300_000).map(|(n, ns)| (*n, (*ns / 1000) as u32)).collect::<Vec<_>>()));
        worst.sort_by_key(|w| std::cmp::Reverse(w.0));
        worst.truncate(12);
        peak_walkers = peak_walkers.max(state.population.citizens().filter(|(_, c)| c.motion().is_moving()).count() + state.population.walking.travelers.len());
        peak_enemies = peak_enemies.max(state.military.warbands().iter().map(|w| w.fighters.len()).sum());
        peak_caravans = peak_caravans.max(state.diplomacy.caravans().len());
        peak_queue = peak_queue.max(state.population.walking.queued_requests());
        let now = state.clock.tick;
        for (id, c) in state.population.citizens() {
            let queued = c.motion().phase() == crate::population::TripPhase::Queued;
            match (queued, queued_since.get(&id).copied()) {
                (true, None) => {
                    queued_since.insert(id, now);
                }
                (false, Some(since)) => {
                    queued_since.remove(&id);
                    max_wait = max_wait.max(now - since);
                    sum_wait += now - since;
                    waits += 1;
                }
                _ => {}
            }
        }
    }
    eprintln!("path queue: {waits} waits, mean {:.1} ticks, max {max_wait} ticks", sum_wait as f64 / waits.max(1) as f64);
    let mut sorted = totals.clone();
    sorted.sort_unstable();
    let n = totals.len().max(1) as f64;
    let avg = totals.iter().sum::<u128>() as f64 / n / 1000.0;
    eprintln!(
        "{} ticks: avg {avg:.0} µs, p50 {:.0} µs, p99 {:.0} µs, max {:.0} µs (tick {worst_at}, hour {}), ticks over 8 ms: {}",
        totals.len(),
        percentile(&sorted, 50) as f64 / 1000.0,
        percentile(&sorted, 99) as f64 / 1000.0,
        sorted.last().copied().unwrap_or(0) as f64 / 1000.0,
        (worst_at % TICKS_PER_DAY) / TICKS_PER_HOUR,
        sorted.iter().filter(|t| **t > 8_000_000).count(),
    );
    for (ns, tick, parts) in &worst {
        eprintln!("  slow tick {tick} ({:02}:{:02}): {} µs {parts:?}", (tick % TICKS_PER_DAY) / TICKS_PER_HOUR, (tick % TICKS_PER_HOUR) * 60 / TICKS_PER_HOUR, ns / 1000);
    }
    for (i, name) in steps.names.iter().enumerate() {
        eprintln!("  {name:<13} avg {:>7.1} µs  max {:>7.0} µs", steps.sum[i] as f64 / n / 1000.0, steps.max[i] as f64 / 1000.0);
    }
    let battles: Vec<String> = state.military.battles().map(|b| b.text.clone()).collect();
    eprintln!(
        "peaks: {peak_walkers} walkers, {peak_enemies} enemies on the map, {peak_caravans} caravans, path queue {peak_queue}; path stats {:?}; battles {battles:?}",
        state.navigation().stats()
    );
    let mut days: Vec<crate::population::DayCounts> = state.population.stats().history().map(|d| d.counts).collect();
    days.push(*state.population.stats().today());
    for (i, c) in days.iter().enumerate() {
        eprintln!(
            "day {i}: births {} deaths {:?} immigrants {} emigrants {}",
            c.births,
            crate::population::DeathCause::ALL.iter().map(|k| (*k, c.deaths_by(*k))).filter(|(_, n)| *n > 0).collect::<Vec<_>>(),
            c.immigrants,
            c.emigrants
        );
    }
    eprintln!("game over: {:?} at tick {:?}", state.game_over().map(|g| g.reason), state.game_over().map(|g| g.tick));
    eprintln!(
        "after: {} citizens, {} buildings, {} soldiers, treasury {}",
        state.population.len(),
        state.buildings.len(),
        state.military.soldiers().count(),
        state.economy.treasury()
    );
    let header = SaveHeader::for_state(&state, d, "scale", 0);
    let t0 = Instant::now();
    let bytes = crate::save::encode(&header, &state).expect("encode");
    let encode = t0.elapsed();
    let t0 = Instant::now();
    let loaded = crate::save::decode(&bytes, d).expect("decode");
    let decode = t0.elapsed();
    let rss = rss_kib("VmRSS:");
    eprintln!(
        "save {:.2} MiB, encode {encode:?}, decode + validate + rederive {decode:?}; memory: RSS {:.0} MiB (before {:.0} MiB, peak {:.0} MiB)",
        bytes.len() as f64 / 1_048_576.0,
        rss as f64 / 1024.0,
        rss0 as f64 / 1024.0,
        rss_kib("VmHWM:") as f64 / 1024.0,
    );
    let mut a = state;
    let mut b = loaded.state;
    for _ in 0..TICKS_PER_HOUR {
        let ea = a.tick(d, Vec::new());
        let eb = b.tick(d, Vec::new());
        assert_eq!(ea, eb, "the loaded game continues identically");
    }
    a.validate(d).expect("valid state");
}

/// Cost of the full job allocation ([`crate::workforce::allocate`]) in the scale town: with
/// nothing changed, and after a workplace closes or opens (its workers move). Run with
/// `cargo test --release -p kingdoms_sim allocation_performance -- --ignored --nocapture`.
#[test]
#[ignore = "performance measurement; run in release mode"]
fn allocation_performance() {
    let d = defs();
    let mut state = GameState::new(d, GameSettings::new(d, 7, WorldSize::Large));
    build_town(&mut state, d);
    crate::population::test_populate(&mut state, d, 5000);
    for _ in 0..TICKS_PER_HOUR {
        state.tick(d, Vec::new());
    }
    let workshops: Vec<crate::buildings::BuildingId> =
        state.buildings.iter().filter(|(_, b)| b.production().is_some() && b.staffed() > 0).map(|(id, _)| id).collect();
    let summary = crate::workforce::summary(&state, d);
    eprintln!(
        "{} citizens, {} buildings, {} working age, {} employed, {} unemployed, {} vacancies",
        state.population.len(),
        state.buildings.len(),
        summary.working_age,
        summary.employed,
        summary.unemployed,
        summary.vacancies
    );
    let mut measure = |label: &str, change: &mut dyn FnMut(&mut GameState, usize)| {
        let mut times = Vec::new();
        for i in 0..60 {
            change(&mut state, i);
            state.workforce.mark_jobs_dirty();
            let t0 = Instant::now();
            crate::workforce::allocate(&mut state, d);
            times.push(t0.elapsed().as_nanos());
        }
        times.sort_unstable();
        let avg = times.iter().sum::<u128>() as f64 / times.len() as f64 / 1000.0;
        eprintln!("allocate ({label}): avg {avg:.0} µs, p50 {:.0} µs, max {:.0} µs", percentile(&times, 50) as f64 / 1000.0, *times.last().unwrap_or(&0) as f64 / 1000.0);
    };
    measure("nothing changed", &mut |_, _| {});
    measure("a workplace closes, the last one opens", &mut |s, i| {
        let close = workshops[i % workshops.len()];
        let open = workshops[(i + workshops.len() - 1) % workshops.len()];
        s.buildings.get_mut(open).expect("workshop").paused = false;
        s.buildings.get_mut(close).expect("workshop").paused = true;
    });
    measure("twenty workplaces close, twenty open", &mut |s, i| {
        for k in 0..20 {
            let close = workshops[(20 * i + k) % workshops.len()];
            let open = workshops[(20 * i + k + workshops.len() - 20) % workshops.len()];
            s.buildings.get_mut(open).expect("workshop").paused = false;
            s.buildings.get_mut(close).expect("workshop").paused = true;
        }
    });
    measure("everybody looks for work anew", &mut |s, _| {
        let employed: Vec<crate::population::CitizenId> = s.population.citizens().filter(|(_, c)| c.job().is_some()).map(|(id, _)| id).collect();
        for id in employed {
            s.population.set_job(id, None);
        }
    });
}
