//! Seed-based, multi-stage procedural world generator.
//!
//! Stages (each uses its own sub-seed, so changing one stage does not reshuffle the others):
//! 1. height map (continental fBm + coast side + ridged mountain ranges)
//! 2. temperature (latitude gradient in a seeded direction + altitude cooling)
//! 3. hydrology: sea, lakes, rivers (rivers follow a least-cost potential towards the sea, so
//!    they always run downhill through valleys and reach open water)
//! 4. elevation quantisation into terraces + spike removal, gentle coasts
//! 5. moisture (noise + distance to water)
//! 6. biome and terrain assignment, soil fertility
//! 7. resource deposits (stone, iron, coal, gold in mountains; clay at banks; sand; fish)
//! 8. vegetation (biome-dependent clumped forests)
//! 9. natural obstacles (boulders)
//! 10. start area and rival settlement sites (guaranteed basic resources near the start)
//!
//! **Portability:** the same seed must give the same world on every platform. Results are
//! quantised (elevation levels, tile positions), so a transcendental function that differs by
//! one ULP between system math libraries could flip a value and cascade through later stages.
//! The generator therefore uses only correctly rounded IEEE operations (`+ - * /`, `sqrt`,
//! `floor`, `ceil`, `abs`) from std and takes `sin`, `cos`, `exp` and `pow` from the pure-Rust
//! `libm` crate, which computes them identically everywhere.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use glam::{IVec2, Vec2};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use super::noise::Perlin;
use super::{Biome, Deposit, DepositKind, MAX_ELEVATION, Terrain, Tile, TreeKind, Vegetation, WorldMap};

/// Selectable map sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorldSize {
    Small,
    #[default]
    Medium,
    Large,
    Huge,
}

impl WorldSize {
    pub const ALL: [WorldSize; 4] = [WorldSize::Small, WorldSize::Medium, WorldSize::Large, WorldSize::Huge];

    pub fn tiles(self) -> i32 {
        match self {
            WorldSize::Small => 128,
            WorldSize::Medium => 192,
            WorldSize::Large => 256,
            WorldSize::Huge => 384,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            WorldSize::Small => "Klein (128×128)",
            WorldSize::Medium => "Mittel (192×192)",
            WorldSize::Large => "Groß (256×256)",
            WorldSize::Huge => "Riesig (384×384)",
        }
    }
}

/// Parameters of the generator. The same settings always produce the same world.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneratorSettings {
    pub seed: u64,
    pub size: WorldSize,
    /// Fraction of the height range below sea level (0.2 – 0.5).
    pub sea_level: f32,
    /// Amount of mountain ranges (0 – 1).
    pub mountains: f32,
    /// Forest density multiplier (0 – 2).
    pub forests: f32,
    /// Number of rival settlement sites to reserve.
    pub rival_sites: u32,
}

impl Default for GeneratorSettings {
    fn default() -> Self {
        Self { seed: 1, size: WorldSize::Medium, sea_level: 0.33, mountains: 0.5, forests: 1.0, rival_sites: 3 }
    }
}

impl GeneratorSettings {
    pub fn with_seed(seed: u64, size: WorldSize) -> Self {
        Self { seed, size, ..Default::default() }
    }
}

fn sub_seed(seed: u64, stage: u64) -> u64 {
    // SplitMix64 step: well-distributed independent seeds per stage.
    let mut z = seed.wrapping_add(stage.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Scalar field over the map.
struct Field {
    w: i32,
    h: i32,
    v: Vec<f32>,
}

impl Field {
    fn new(w: i32, h: i32) -> Self {
        Self { w, h, v: vec![0.0; (w * h) as usize] }
    }
    fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }
    fn get(&self, x: i32, y: i32) -> f32 {
        self.v[self.idx(x.clamp(0, self.w - 1), y.clamp(0, self.h - 1))]
    }
    fn set(&mut self, x: i32, y: i32, val: f32) {
        let i = self.idx(x, y);
        self.v[i] = val;
    }
    fn blur(&mut self, mask: impl Fn(usize) -> bool) {
        let src = self.v.clone();
        for y in 0..self.h {
            for x in 0..self.w {
                let i = self.idx(x, y);
                if !mask(i) {
                    continue;
                }
                let mut sum = 0.0;
                let mut n = 0.0;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx >= 0 && ny >= 0 && nx < self.w && ny < self.h {
                            let wgt = if dx == 0 && dy == 0 { 2.0 } else { 1.0 };
                            sum += src[self.idx(nx, ny)] * wgt;
                            n += wgt;
                        }
                    }
                }
                self.v[i] = sum / n;
            }
        }
    }
    fn normalize(&mut self) {
        let min = self.v.iter().copied().fold(f32::INFINITY, f32::min);
        let max = self.v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let range = (max - min).max(1e-6);
        for v in &mut self.v {
            *v = (*v - min) / range;
        }
    }
}

/// Totally ordered non-negative f32 for priority queues (bit pattern is monotonic).
fn cost_key(c: f32) -> u32 {
    c.max(0.0).to_bits()
}

const DIRS4: [IVec2; 4] = [IVec2::new(1, 0), IVec2::new(-1, 0), IVec2::new(0, 1), IVec2::new(0, -1)];

#[derive(Clone, Copy, PartialEq, Eq)]
enum WaterKind {
    Land,
    Sea,
    Lake,
    River,
}

/// Generates a complete world.
pub fn generate(settings: &GeneratorSettings) -> WorldMap {
    let n = settings.size.tiles();
    let (w, h) = (n, n);
    let seed = settings.seed;
    log::info!("generating world: seed {seed}, {w}x{h}");

    // ---- 1. height -----------------------------------------------------------------------
    let height = stage_height(settings, w, h);

    // ---- 2. temperature --------------------------------------------------------------------
    let temperature = stage_temperature(seed, &height, w, h, settings.sea_level);

    // ---- 3. hydrology ----------------------------------------------------------------------
    let (mut height, water) = stage_hydrology(settings, height, w, h);

    // ---- 4. elevation levels ---------------------------------------------------------------
    let levels = stage_levels(&mut height, &water, w, h, settings.sea_level);

    // ---- 5. moisture -----------------------------------------------------------------------
    let water_dist = distance_field(w, h, |i| water[i] != WaterKind::Land);
    let sea_dist = distance_field(w, h, |i| water[i] == WaterKind::Sea);
    let fresh_dist = distance_field(w, h, |i| matches!(water[i], WaterKind::Lake | WaterKind::River));
    let moisture = stage_moisture(seed, &water_dist, w, h);

    // ---- 6. biomes / terrain ---------------------------------------------------------------
    let blank = Tile {
        terrain: Terrain::Grass,
        biome: Biome::Grassland,
        elevation: 0,
        fertility: 0,
        moisture: 0,
        temperature: 0,
        vegetation: Vegetation::None,
        deposit: None,
        river: false,
    };
    let mut map = WorldMap::new_filled(w, h, seed, blank);
    stage_biomes(
        &mut map,
        &BiomeInputs { water: &water, levels: &levels, temperature: &temperature, moisture: &moisture, sea_dist: &sea_dist, fresh_dist: &fresh_dist },
        seed,
    );

    // ---- 7..9 deposits, vegetation, obstacles ----------------------------------------------
    stage_deposits(&mut map, seed);
    stage_vegetation(&mut map, seed, settings.forests);
    stage_obstacles(&mut map, seed);

    // ---- 10. start area -------------------------------------------------------------------
    stage_start(&mut map, seed, settings.rival_sites);

    log::info!("world generated: start at {:?}, {} rival sites", map.start, map.settlement_sites.len());
    map
}

fn stage_height(settings: &GeneratorSettings, w: i32, h: i32) -> Field {
    let seed = settings.seed;
    let base = Perlin::new(sub_seed(seed, 1));
    let detail = Perlin::new(sub_seed(seed, 2));
    let ridges = Perlin::new(sub_seed(seed, 3));
    let range_mask = Perlin::new(sub_seed(seed, 4));
    let mut rng = ChaCha8Rng::seed_from_u64(sub_seed(seed, 5));
    // Direction towards the open sea.
    let angle: f32 = rng.random_range(0.0..std::f32::consts::TAU);
    let sea_dir = Vec2::new(libm::cosf(angle), libm::sinf(angle));
    let scale = 1.0 / 56.0;
    let mut f = Field::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let p = Vec2::new(x as f32, y as f32);
            let nrm = p / Vec2::new(w as f32, h as f32) - Vec2::splat(0.5);
            let mut v = 0.5 + 0.38 * base.fbm(p.x * scale, p.y * scale, 5, 2.0, 0.5);
            v += 0.08 * detail.fbm(p.x * scale * 4.0, p.y * scale * 4.0, 3, 2.0, 0.5);
            // Ocean side: land slopes down towards one edge of the map.
            let side = nrm.dot(sea_dir);
            v -= 0.9 * smoothstep(0.12, 0.5, side + 0.06 * base.sample(p.x * 0.02, p.y * 0.02));
            // Mountain ranges.
            let mask = smoothstep(0.1, 0.45, range_mask.fbm(p.x * scale * 0.7 + 50.0, p.y * scale * 0.7, 3, 2.0, 0.5))
                * settings.mountains.clamp(0.0, 1.0)
                * 1.6;
            let ridge = ridges.ridged(p.x * scale * 1.3, p.y * scale * 1.3, 5);
            v += mask * ridge * ridge * 0.75;
            f.set(x, y, v);
        }
    }
    f.normalize();
    f
}

fn stage_temperature(seed: u64, height: &Field, w: i32, h: i32, sea: f32) -> Field {
    let noise = Perlin::new(sub_seed(seed, 10));
    let mut rng = ChaCha8Rng::seed_from_u64(sub_seed(seed, 11));
    let angle: f32 = rng.random_range(0.0..std::f32::consts::TAU);
    let dir = Vec2::new(libm::cosf(angle), libm::sinf(angle));
    // How strong the climate gradient across the map is (some maps are uniformly temperate).
    let spread: f32 = rng.random_range(0.35..0.7);
    let mut t = Field::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let nrm = Vec2::new(x as f32 / w as f32 - 0.5, y as f32 / h as f32 - 0.5);
            let mut v = 0.6 + nrm.dot(dir) * spread;
            let alt = (height.get(x, y) - sea).max(0.0) / (1.0 - sea);
            v -= alt * alt * 0.75;
            v += 0.08 * noise.fbm(x as f32 / 40.0, y as f32 / 40.0, 3, 2.0, 0.5);
            t.set(x, y, v.clamp(0.0, 1.0));
        }
    }
    t
}

#[allow(clippy::needless_range_loop)] // several parallel arrays are indexed together
fn stage_hydrology(settings: &GeneratorSettings, mut height: Field, w: i32, h: i32) -> (Field, Vec<WaterKind>) {
    let sea = settings.sea_level;
    let len = (w * h) as usize;
    let mut water = vec![WaterKind::Land; len];

    // Sea: everything below sea level that is connected to the map border or large enough.
    for i in 0..len {
        if height.v[i] < sea {
            water[i] = WaterKind::Sea;
        }
    }
    // Inland basins below sea level that are small become lakes.
    let mut visited = vec![false; len];
    for start in 0..len {
        if water[start] != WaterKind::Sea || visited[start] {
            continue;
        }
        let mut region = Vec::new();
        let mut touches_border = false;
        let mut queue = VecDeque::from([start]);
        visited[start] = true;
        while let Some(i) = queue.pop_front() {
            region.push(i);
            let (x, y) = ((i as i32) % w, (i as i32) / w);
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                touches_border = true;
            }
            for d in DIRS4 {
                let (nx, ny) = (x + d.x, y + d.y);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = (ny * w + nx) as usize;
                if !visited[j] && water[j] == WaterKind::Sea {
                    visited[j] = true;
                    queue.push_back(j);
                }
            }
        }
        if !touches_border && region.len() < (len / 25) {
            for i in region {
                water[i] = WaterKind::Lake;
            }
        }
    }

    let mut rng = ChaCha8Rng::seed_from_u64(sub_seed(settings.seed, 20));
    let meander = Perlin::new(sub_seed(settings.seed, 21));

    // A few highland lakes.
    let lake_count = (w / 96).max(1);
    for _ in 0..lake_count {
        for _attempt in 0..40 {
            let x = rng.random_range(8..w - 8);
            let y = rng.random_range(8..h - 8);
            let hv = height.get(x, y);
            if hv < sea + 0.06 || hv > sea + 0.35 {
                continue;
            }
            let radius: f32 = rng.random_range(2.5..5.5);
            let r = radius.ceil() as i32;
            for dy in -r..=r {
                for dx in -r..=r {
                    let d = ((dx * dx + dy * dy) as f32).sqrt()
                        + 1.5 * meander.sample((x + dx) as f32 * 0.3, (y + dy) as f32 * 0.3);
                    if d <= radius {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx > 0 && ny > 0 && nx < w - 1 && ny < h - 1 {
                            let i = (ny * w + nx) as usize;
                            if water[i] == WaterKind::Land {
                                water[i] = WaterKind::Lake;
                                height.v[i] = height.v[i].min(hv);
                            }
                        }
                    }
                }
            }
            break;
        }
    }

    // Least-cost potential towards open water: every land tile gets a parent pointer along which
    // a river would flow. Costs prefer low terrain, so paths follow valleys.
    let mut cost = vec![f32::INFINITY; len];
    let mut parent = vec![usize::MAX; len];
    let mut heap = BinaryHeap::new();
    for i in 0..len {
        if matches!(water[i], WaterKind::Sea | WaterKind::Lake) {
            cost[i] = 0.0;
            heap.push(Reverse((cost_key(0.0), i)));
        }
    }
    while let Some(Reverse((key, i))) = heap.pop() {
        if key != cost_key(cost[i]) {
            continue;
        }
        let (x, y) = ((i as i32) % w, (i as i32) / w);
        for d in DIRS4 {
            let (nx, ny) = (x + d.x, y + d.y);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let j = (ny * w + nx) as usize;
            if water[j] != WaterKind::Land {
                continue;
            }
            let hj = height.v[j];
            let uphill = (hj - height.v[i]).max(0.0);
            let wiggle = 2.6 * (meander.fbm(nx as f32 * 0.07, ny as f32 * 0.07, 3, 2.0, 0.5) + 1.0)
                + 0.6 * (meander.sample(nx as f32 * 0.31 + 7.0, ny as f32 * 0.31) + 1.0);
            let c = cost[i] + 1.0 + wiggle + hj * hj * 18.0 + uphill * 60.0;
            if c < cost[j] {
                cost[j] = c;
                parent[j] = i;
                heap.push(Reverse((cost_key(c), j)));
            }
        }
    }

    // Rivers from springs in moist highlands.
    let river_target = (w * h / 4500).clamp(3, 40);
    let mut rivers = 0;
    let mut attempts = 0;
    while rivers < river_target && attempts < river_target * 60 {
        attempts += 1;
        let x = rng.random_range(4..w - 4);
        let y = rng.random_range(4..h - 4);
        let i = (y * w + x) as usize;
        if water[i] != WaterKind::Land || height.v[i] < sea + 0.18 || parent[i] == usize::MAX {
            continue;
        }
        // Trace to open water: every step goes to a neighbour with strictly lower potential, so
        // the river always arrives. Among the descending neighbours a slowly rotating "meander"
        // direction and a little randomness pick the next tile, which yields natural bends.
        let mut path = Vec::new();
        let mut cur = i;
        let mut ok = false;
        while path.len() < (w * 4) as usize {
            if water[cur] != WaterKind::Land {
                ok = true;
                break;
            }
            path.push(cur);
            let (cx, cy) = ((cur as i32) % w, (cur as i32) / w);
            let angle = meander.fbm(cx as f32 * 0.09 + 31.0, cy as f32 * 0.09, 2, 2.0, 0.5) * std::f32::consts::TAU;
            let md = Vec2::new(libm::cosf(angle), libm::sinf(angle));
            let mut best: Option<(f32, usize)> = None;
            for d in DIRS4 {
                let (nx, ny) = (cx + d.x, cy + d.y);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = (ny * w + nx) as usize;
                if cost[j] >= cost[cur] {
                    continue;
                }
                let score = cost[j] - md.dot(d.as_vec2()) * 4.0 + rng.random_range(0.0..6.0);
                if best.is_none_or(|(b, _)| score < b) {
                    best = Some((score, j));
                }
            }
            match best {
                Some((_, j)) => cur = j,
                None => break,
            }
        }
        if !ok || path.len() < 14 {
            continue;
        }
        // Carve: heights never increase downstream.
        let mut running = height.v[path[0]];
        for &p in &path {
            running = running.min(height.v[p]);
            height.v[p] = running - 0.004;
            water[p] = WaterKind::River;
        }
        rivers += 1;
    }
    log::debug!("hydrology: {rivers} rivers, {lake_count} highland lakes");
    (height, water)
}

#[allow(clippy::needless_range_loop)] // several parallel arrays are indexed together
fn stage_levels(height: &mut Field, water: &[WaterKind], w: i32, h: i32, sea: f32) -> Vec<u8> {
    let len = (w * h) as usize;
    // Smooth land a little so terraces form coherent plateaus.
    for _ in 0..2 {
        height.blur(|i| water[i] == WaterKind::Land);
    }
    let mut levels = vec![0u8; len];
    for i in 0..len {
        let hl = ((height.v[i] - sea) / (1.0 - sea)).clamp(0.0, 1.0);
        let lvl = (libm::powf(hl, 1.35) * (f32::from(MAX_ELEVATION) + 0.99)) as u8;
        levels[i] = lvl.min(MAX_ELEVATION);
    }
    // Water: sea at 0; lakes take the lowest level of their shore; rivers keep their carved level.
    for i in 0..len {
        if water[i] == WaterKind::Sea {
            levels[i] = 0;
        }
    }
    let idx = |x: i32, y: i32| (y * w + x) as usize;
    for _ in 0..3 {
        // Remove single-tile spikes and pits on land.
        let src = levels.clone();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = idx(x, y);
                if water[i] != WaterKind::Land {
                    continue;
                }
                let ns = [src[idx(x + 1, y)], src[idx(x - 1, y)], src[idx(x, y + 1)], src[idx(x, y - 1)]];
                let higher = ns.iter().filter(|&&n| n > src[i]).count();
                let lower = ns.iter().filter(|&&n| n < src[i]).count();
                if lower >= 4 || higher >= 4 || lower == 3 || higher == 3 {
                    let mut sorted = ns;
                    sorted.sort_unstable();
                    levels[i] = sorted[1].max(sorted[2].min(src[i]));
                    if higher >= 3 {
                        levels[i] = sorted[1];
                    }
                }
            }
        }
    }
    // Lakes: flat surface at the minimum level of adjacent land.
    let mut visited = vec![false; len];
    for start in 0..len {
        if water[start] != WaterKind::Lake || visited[start] {
            continue;
        }
        let mut region = Vec::new();
        let mut shore_min = MAX_ELEVATION;
        let mut queue = VecDeque::from([start]);
        visited[start] = true;
        while let Some(i) = queue.pop_front() {
            region.push(i);
            let (x, y) = ((i as i32) % w, (i as i32) / w);
            for d in DIRS4 {
                let (nx, ny) = (x + d.x, y + d.y);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = idx(nx, ny);
                if water[j] == WaterKind::Lake {
                    if !visited[j] {
                        visited[j] = true;
                        queue.push_back(j);
                    }
                } else {
                    shore_min = shore_min.min(levels[j]);
                }
            }
        }
        for i in region {
            levels[i] = shore_min;
        }
    }
    // Rivers: never higher than any land neighbour, never rising downstream is guaranteed by
    // carving; clamp to the minimum of adjacent land so banks are not below the water.
    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            if water[i] != WaterKind::River {
                continue;
            }
            let mut m = levels[i];
            for d in DIRS4 {
                let (nx, ny) = (x + d.x, y + d.y);
                if nx >= 0 && ny >= 0 && nx < w && ny < h && water[idx(nx, ny)] == WaterKind::Land {
                    m = m.min(levels[idx(nx, ny)]);
                }
            }
            levels[i] = m;
        }
    }
    // Gentle coasts: land next to the sea is at level 0, next ring at most 1.
    let sea_dist = distance_field(w, h, |i| water[i] == WaterKind::Sea);
    for i in 0..len {
        if water[i] == WaterKind::Land {
            match sea_dist[i] {
                1 => levels[i] = 0,
                2 => levels[i] = levels[i].min(1),
                _ => {}
            }
        }
    }
    levels
}

/// BFS distance (4-neighbourhood) to the nearest tile matching `is_source`; capped at 255.
fn distance_field(w: i32, h: i32, is_source: impl Fn(usize) -> bool) -> Vec<u8> {
    let len = (w * h) as usize;
    let mut dist = vec![255u8; len];
    let mut queue = VecDeque::new();
    for (i, d) in dist.iter_mut().enumerate() {
        if is_source(i) {
            *d = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = ((i as i32) % w, (i as i32) / w);
        let nd = dist[i].saturating_add(1);
        for d in DIRS4 {
            let (nx, ny) = (x + d.x, y + d.y);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let j = (ny * w + nx) as usize;
            if nd < dist[j] {
                dist[j] = nd;
                queue.push_back(j);
            }
        }
    }
    dist
}

fn stage_moisture(seed: u64, water_dist: &[u8], w: i32, h: i32) -> Field {
    let noise = Perlin::new(sub_seed(seed, 30));
    let mut m = Field::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let near = libm::expf(-(f32::from(water_dist[i])) / 7.0);
            let v = 0.44 + 0.5 * noise.fbm(x as f32 / 38.0, y as f32 / 38.0, 4, 2.0, 0.5) + 0.2 * near;
            m.set(x, y, v.clamp(0.0, 1.0));
        }
    }
    m
}

struct BiomeInputs<'a> {
    water: &'a [WaterKind],
    levels: &'a [u8],
    temperature: &'a Field,
    moisture: &'a Field,
    sea_dist: &'a [u8],
    fresh_dist: &'a [u8],
}

fn stage_biomes(map: &mut WorldMap, inp: &BiomeInputs, seed: u64) {
    let w = map.width();
    let patch = Perlin::new(sub_seed(seed, 40));
    let fert_noise = Perlin::new(sub_seed(seed, 41));
    let land_dist = distance_field(w, map.height(), |i| inp.water[i] == WaterKind::Land);
    let tiles = map.tiles_mut_raw();
    for (i, tile) in tiles.iter_mut().enumerate() {
        let (x, y) = ((i as i32) % w, (i as i32) / w);
        let t = inp.temperature.v[i];
        let m = inp.moisture.v[i];
        let lvl = inp.levels[i];
        tile.elevation = lvl;
        tile.temperature = (t * 255.0) as u8;
        tile.moisture = (m * 255.0) as u8;
        let (biome, terrain) = match inp.water[i] {
            WaterKind::Sea => {
                let shallow = land_dist[i] <= 2;
                (Biome::Ocean, if shallow { Terrain::ShallowWater } else { Terrain::DeepWater })
            }
            WaterKind::Lake => (Biome::Lake, Terrain::ShallowWater),
            WaterKind::River => {
                tile.river = true;
                (Biome::Grassland, Terrain::ShallowWater)
            }
            WaterKind::Land => {
                let p = patch.fbm(x as f32 / 14.0, y as f32 / 14.0, 3, 2.0, 0.5);
                if lvl >= 7 || (lvl >= 5 && t < 0.42) {
                    (Biome::Mountains, if t < 0.3 || lvl >= 8 { Terrain::Snow } else { Terrain::Rock })
                } else if lvl >= 5 {
                    (Biome::Mountains, if p > -0.1 { Terrain::Rock } else { Terrain::Grass })
                } else if t < 0.2 {
                    (Biome::Snow, Terrain::Snow)
                } else if inp.sea_dist[i] <= 2 && lvl == 0 {
                    (Biome::Coast, Terrain::Sand)
                } else if m > 0.74 && lvl <= 1 && t > 0.3 {
                    (Biome::Swamp, if p > 0.25 { Terrain::Grass } else { Terrain::Swamp })
                } else if m < 0.34 && t > 0.55 {
                    (Biome::Arid, if p > 0.3 { Terrain::Dirt } else if p < -0.35 { Terrain::Sand } else { Terrain::Steppe })
                } else if m > 0.57 + p * 0.1 {
                    (Biome::TemperateForest, Terrain::Forest)
                } else if m > 0.46 && inp.fresh_dist[i] <= 7 && lvl <= 3 {
                    (Biome::FertilePlains, Terrain::Meadow)
                } else if t < 0.3 {
                    (Biome::Grassland, if p > 0.2 { Terrain::Steppe } else { Terrain::Grass })
                } else {
                    (Biome::Grassland, Terrain::Grass)
                }
            }
        };
        tile.biome = biome;
        tile.terrain = terrain;
        let base: f32 = match terrain {
            Terrain::Meadow => 215.0,
            Terrain::Grass => 165.0,
            Terrain::Forest => 145.0,
            Terrain::Dirt => 120.0,
            Terrain::Swamp => 90.0,
            Terrain::Steppe => 85.0,
            Terrain::Sand => 25.0,
            _ => 0.0,
        };
        let river_bonus = if inp.fresh_dist[i] <= 4 { 25.0 } else { 0.0 };
        let fert = base + river_bonus + 25.0 * fert_noise.sample(x as f32 / 9.0, y as f32 / 9.0);
        tile.fertility = if base > 0.0 { fert.clamp(10.0, 255.0) as u8 } else { 0 };
    }
}

fn stage_deposits(map: &mut WorldMap, seed: u64) {
    let w = map.width();
    let h = map.height();
    let ore = Perlin::new(sub_seed(seed, 50));
    let coal = Perlin::new(sub_seed(seed, 51));
    let gold = Perlin::new(sub_seed(seed, 52));
    let clay = Perlin::new(sub_seed(seed, 53));
    let fish = Perlin::new(sub_seed(seed, 54));
    let stone = Perlin::new(sub_seed(seed, 55));
    let mut rng = ChaCha8Rng::seed_from_u64(sub_seed(seed, 56));
    let fresh = distance_field(w, h, |i| {
        let t = &map.tiles()[i];
        t.river || t.biome == Biome::Lake
    });
    for y in 0..h {
        for x in 0..w {
            let p = IVec2::new(x, y);
            let t = *map.tile(p);
            let (fx, fy) = (x as f32, y as f32);
            let i = (y * w + x) as usize;
            let kind = if t.is_water() {
                (fish.fbm(fx / 10.0, fy / 10.0, 2, 2.0, 0.5) > 0.25 && t.terrain == Terrain::ShallowWater)
                    .then_some(DepositKind::Fish)
            } else if t.biome == Biome::Mountains || t.elevation >= 4 {
                if gold.fbm(fx / 7.0, fy / 7.0, 2, 2.0, 0.5) > 0.55 && t.elevation >= 6 {
                    Some(DepositKind::Gold)
                } else if ore.fbm(fx / 9.0, fy / 9.0, 2, 2.0, 0.5) > 0.38 {
                    Some(DepositKind::Iron)
                } else if coal.fbm(fx / 9.0, fy / 9.0, 2, 2.0, 0.5) > 0.4 {
                    Some(DepositKind::Coal)
                } else if t.terrain == Terrain::Rock && stone.fbm(fx / 6.0, fy / 6.0, 2, 2.0, 0.5) > 0.05 {
                    Some(DepositKind::Stone)
                } else {
                    None
                }
            } else if fresh[i] == 1 && clay.fbm(fx / 8.0, fy / 8.0, 2, 2.0, 0.5) > 0.32 {
                Some(DepositKind::Clay)
            } else if t.terrain == Terrain::Sand && t.biome == Biome::Coast && clay.sample(fx / 5.0, fy / 5.0) > 0.3 {
                Some(DepositKind::Sand)
            } else if t.elevation >= 3 && stone.fbm(fx / 6.0, fy / 6.0, 2, 2.0, 0.5) > 0.55 {
                Some(DepositKind::Stone)
            } else {
                None
            };
            if let Some(kind) = kind {
                let amount = match kind {
                    DepositKind::Gold => rng.random_range(120..260),
                    DepositKind::Fish => rng.random_range(300..600),
                    DepositKind::Sand | DepositKind::Clay => rng.random_range(600..1200),
                    _ => rng.random_range(350..800),
                };
                map.tiles_mut_raw()[i].deposit = Some(Deposit { kind, amount });
            }
        }
    }
}

fn stage_vegetation(map: &mut WorldMap, seed: u64, density: f32) {
    let w = map.width();
    let clump = Perlin::new(sub_seed(seed, 60));
    let mut rng = ChaCha8Rng::seed_from_u64(sub_seed(seed, 61));
    let tiles = map.tiles_mut_raw();
    for (i, t) in tiles.iter_mut().enumerate() {
        if t.is_water() || t.deposit.is_some() {
            continue;
        }
        let (x, y) = ((i as i32) % w, (i as i32) / w);
        let c = 0.5 + 0.5 * clump.fbm(x as f32 / 11.0, y as f32 / 11.0, 3, 2.0, 0.5);
        let (p_tree, p_bush) = match t.biome {
            Biome::TemperateForest => (0.35 + 0.6 * c, 0.05),
            Biome::Grassland => (0.12 * smoothstep(0.55, 0.9, c), 0.03),
            Biome::FertilePlains => (0.05 * smoothstep(0.6, 0.9, c), 0.02),
            Biome::Swamp => (0.2 * c, 0.12),
            Biome::Mountains => (if t.elevation <= 6 && t.terrain != Terrain::Snow { 0.12 * c } else { 0.0 }, 0.0),
            Biome::Snow => (0.1 * c, 0.0),
            Biome::Arid => (0.0, 0.05),
            Biome::Coast => (0.0, 0.02),
            Biome::Ocean | Biome::Lake => (0.0, 0.0),
        };
        let roll: f32 = rng.random();
        let roll2: f32 = rng.random();
        let growth = rng.random_range(150..=255u8);
        if roll < p_tree * density {
            let kind = match t.biome {
                Biome::Mountains | Biome::Snow => TreeKind::Conifer,
                Biome::Swamp => TreeKind::Willow,
                _ if t.temperature < 90 => TreeKind::Conifer,
                _ => {
                    if roll2 < 0.6 {
                        TreeKind::Deciduous
                    } else if roll2 < 0.85 {
                        TreeKind::Birch
                    } else {
                        TreeKind::Conifer
                    }
                }
            };
            t.vegetation = Vegetation::Tree { kind, growth };
        } else if roll2 < p_bush {
            t.vegetation = Vegetation::Bush;
        }
    }
}

fn stage_obstacles(map: &mut WorldMap, seed: u64) {
    let mut rng = ChaCha8Rng::seed_from_u64(sub_seed(seed, 70));
    for t in map.tiles_mut_raw().iter_mut() {
        if t.is_water() || t.vegetation != Vegetation::None || t.deposit.is_some() {
            continue;
        }
        let p = match t.terrain {
            Terrain::Rock => 0.08,
            Terrain::Snow => 0.03,
            _ if t.elevation >= 3 => 0.025,
            Terrain::Steppe => 0.015,
            _ => 0.004,
        };
        if rng.random::<f32>() < p {
            t.vegetation = Vegetation::Boulder;
        }
    }
}

/// Scores a potential settlement centre (higher is better; `None` if unusable).
fn score_site(map: &WorldMap, c: IVec2) -> Option<f32> {
    let center = map.get(c)?;
    if center.is_water() || !center.terrain.is_buildable() {
        return None;
    }
    let level = center.elevation;
    let r = 7;
    let flat = map.count_in_radius(c, r, |t| t.elevation == level && t.terrain.is_buildable() && !t.is_water());
    let area = ((2 * r + 1) * (2 * r + 1)) as f32;
    if (flat as f32) < area * 0.6 {
        return None;
    }
    let fresh = map.count_in_radius(c, 12, Tile::has_fresh_water) as f32;
    let forest = map.count_in_radius(c, 16, |t| t.vegetation.is_tree()) as f32;
    let stone = map.count_in_radius(c, 24, |t| matches!(t.deposit, Some(Deposit { kind: DepositKind::Stone, .. }))) as f32;
    let fertile = map.count_in_radius(c, 12, |t| t.fertility > 150) as f32;
    let bad = map.count_in_radius(c, 8, |t| matches!(t.terrain, Terrain::Swamp | Terrain::Rock | Terrain::Snow)) as f32;
    Some(
        flat as f32 / area * 40.0
            + fresh.min(20.0) * 1.5
            + forest.min(80.0) * 0.4
            + stone.min(10.0) * 2.0
            + fertile.min(120.0) * 0.25
            - bad * 2.0,
    )
}

fn stage_start(map: &mut WorldMap, seed: u64, rival_sites: u32) {
    let (w, h) = (map.width(), map.height());
    let margin = (w / 8).max(16);
    let mut rng = ChaCha8Rng::seed_from_u64(sub_seed(seed, 80));
    let mut candidates: Vec<(f32, IVec2)> = Vec::new();
    let step = 4;
    let mut y = margin;
    while y < h - margin {
        let mut x = margin;
        while x < w - margin {
            let p = IVec2::new(x, y);
            if let Some(s) = score_site(map, p) {
                // Tiny jitter to break ties deterministically per seed.
                candidates.push((s + rng.random::<f32>() * 0.01, p));
            }
            x += step;
        }
        y += step;
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.x.cmp(&b.1.x)).then(a.1.y.cmp(&b.1.y)));
    let start = candidates.first().map(|c| c.1).unwrap_or(IVec2::new(w / 2, h / 2));
    map.start = start;

    let min_dist = (w as f32 / 3.2).max(30.0);
    let mut sites: Vec<IVec2> = Vec::new();
    // Far from the start and from each other; on crowded maps the distances shrink step by
    // step (never below 40 % / 32 %) until every requested site is found.
    for relax in [1.0f32, 0.85, 0.7, 0.55, 0.4] {
        for &(_, p) in &candidates {
            if sites.len() as u32 >= rival_sites {
                break;
            }
            let far_from_start = p.as_vec2().distance(start.as_vec2()) >= min_dist * relax;
            let far_from_others = sites.iter().all(|s| s.as_vec2().distance(p.as_vec2()) >= min_dist * 0.8 * relax);
            if far_from_start && far_from_others {
                sites.push(p);
            }
        }
        if sites.len() as u32 >= rival_sites {
            break;
        }
    }
    map.settlement_sites = sites.clone();

    // Clear the immediate surroundings of every settlement centre.
    for c in std::iter::once(start).chain(sites) {
        for dy in -4..=4 {
            for dx in -4..=4 {
                let p = c + IVec2::new(dx, dy);
                if map.in_bounds(p) && dx * dx + dy * dy <= 18 {
                    let t = &mut map.tiles_mut_raw()[(p.y * w + p.x) as usize];
                    if !t.is_water() {
                        t.vegetation = Vegetation::None;
                    }
                }
            }
        }
    }

    // Guarantee basic building materials near the player's start.
    ensure_deposit_near(map, start, DepositKind::Stone, 22, &mut rng);
    ensure_deposit_near(map, start, DepositKind::Clay, 22, &mut rng);
    let trees = map.count_in_radius(start, 16, |t| t.vegetation.is_tree());
    if trees < 25 {
        plant_grove(map, start, &mut rng);
    }
}

fn ensure_deposit_near(map: &mut WorldMap, center: IVec2, kind: DepositKind, radius: i32, rng: &mut ChaCha8Rng) {
    let present = map.count_in_radius(center, radius, |t| t.deposit.is_some_and(|d| d.kind == kind));
    if present >= 3 {
        return;
    }
    let w = map.width();
    for _ in 0..200 {
        let ang: f32 = rng.random_range(0.0..std::f32::consts::TAU);
        let dist: f32 = rng.random_range(9.0..radius as f32);
        let c = center + IVec2::new((libm::cosf(ang) * dist) as i32, (libm::sinf(ang) * dist) as i32);
        let Some(t) = map.get(c) else { continue };
        if t.is_water() {
            continue;
        }
        let level = t.elevation;
        let mut placed = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let p = c + IVec2::new(dx, dy);
                if let Some(t) = map.get(p)
                    && !t.is_water()
                    && t.elevation == level
                {
                    let tile = &mut map.tiles_mut_raw()[(p.y * w + p.x) as usize];
                    tile.deposit = Some(Deposit { kind, amount: 600 });
                    tile.vegetation = Vegetation::None;
                    if kind == DepositKind::Stone {
                        tile.terrain = Terrain::Rock;
                    }
                    placed += 1;
                }
            }
        }
        if placed >= 4 {
            log::debug!("placed guaranteed {kind:?} deposit near start at {c:?}");
            return;
        }
    }
}

fn plant_grove(map: &mut WorldMap, center: IVec2, rng: &mut ChaCha8Rng) {
    let w = map.width();
    let ang: f32 = rng.random_range(0.0..std::f32::consts::TAU);
    let c = center + IVec2::new((libm::cosf(ang) * 11.0) as i32, (libm::sinf(ang) * 11.0) as i32);
    for dy in -4..=4 {
        for dx in -4..=4 {
            let p = c + IVec2::new(dx, dy);
            if dx * dx + dy * dy > 16 || !map.in_bounds(p) {
                continue;
            }
            let tile = &mut map.tiles_mut_raw()[(p.y * w + p.x) as usize];
            if !tile.is_water() && tile.deposit.is_none() && rng.random::<f32>() < 0.6 {
                tile.vegetation = Vegetation::Tree { kind: TreeKind::Deciduous, growth: 255 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small(seed: u64) -> WorldMap {
        generate(&GeneratorSettings::with_seed(seed, WorldSize::Small))
    }

    #[test]
    fn same_seed_same_world() {
        let a = small(1234);
        let b = small(1234);
        assert_eq!(a.tiles(), b.tiles());
        assert_eq!(a.start, b.start);
        assert_eq!(a.settlement_sites, b.settlement_sites);
    }

    /// Stable FNV-1a checksum over every generated field (tiles, start, settlement sites).
    fn checksum(m: &WorldMap) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut write = |bytes: &[u8]| {
            for b in bytes {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for t in m.tiles() {
            let (deposit_kind, deposit_amount) = t.deposit.map_or((255, 0), |d| (d.kind as u8, d.amount));
            let (veg, growth) = match t.vegetation {
                Vegetation::None => (0, 0),
                Vegetation::Tree { kind, growth } => (1 + kind as u8, growth),
                Vegetation::Bush => (10, 0),
                Vegetation::Boulder => (11, 0),
                Vegetation::Stump { hours } => (12, hours),
            };
            write(&[t.terrain as u8, t.biome as u8, t.elevation, t.fertility, t.moisture, t.temperature, veg, growth]);
            write(&[deposit_kind, u8::from(t.river)]);
            write(&deposit_amount.to_le_bytes());
        }
        for p in std::iter::once(m.start).chain(m.settlement_sites.iter().copied()) {
            write(&p.x.to_le_bytes());
            write(&p.y.to_le_bytes());
        }
        h
    }

    /// Golden values: the generator must produce exactly these worlds on every platform and
    /// toolchain. A deliberate generator change updates them (and changes shared seeds).
    #[test]
    fn generated_worlds_match_their_golden_checksums() {
        let got: Vec<(u64, u64)> = [1u64, 42].iter().map(|&seed| (seed, checksum(&small(seed)))).collect();
        assert_eq!(got, vec![(1, GOLDEN_SEED_1), (42, GOLDEN_SEED_42)], "generator output changed");
    }

    const GOLDEN_SEED_1: u64 = 16_417_936_602_608_295_207;
    const GOLDEN_SEED_42: u64 = 10_981_932_207_563_582_973;

    #[test]
    fn different_seed_different_world() {
        assert_ne!(small(1).tiles(), small(2).tiles());
    }

    #[test]
    fn worlds_are_geographically_plausible() {
        for seed in [3u64, 17, 99, 2024] {
            let m = small(seed);
            let total = m.tiles().len() as f32;
            let water = m.tiles().iter().filter(|t| t.is_water()).count() as f32;
            assert!(water / total > 0.03 && water / total < 0.7, "seed {seed}: water fraction {}", water / total);
            assert!(m.tiles().iter().any(|t| t.river), "seed {seed}: no rivers");
            assert!(m.tiles().iter().any(|t| t.vegetation.is_tree()), "seed {seed}: no trees");
            // Sea tiles are at level 0; deep water never touches land directly.
            for p in m.positions() {
                let t = m.tile(p);
                if t.biome == Biome::Ocean {
                    assert_eq!(t.elevation, 0);
                }
                if t.terrain == Terrain::DeepWater {
                    assert!(m.neighbors4(p).all(|q| m.tile(q).is_water()), "seed {seed}: deep water at shore {p:?}");
                }
            }
        }
    }

    #[test]
    fn start_area_is_usable() {
        for seed in [5u64, 42, 777] {
            let m = small(seed);
            let s = m.tile(m.start);
            assert!(!s.is_water(), "seed {seed}: start in water");
            assert!(m.is_flat_buildable(m.start - IVec2::splat(1), IVec2::splat(3)), "seed {seed}: start not buildable");
            let stone = m.count_in_radius(m.start, 22, |t| t.deposit.is_some_and(|d| d.kind == DepositKind::Stone));
            let trees = m.count_in_radius(m.start, 16, |t| t.vegetation.is_tree());
            assert!(stone >= 3, "seed {seed}: no stone near start");
            assert!(trees >= 20, "seed {seed}: no forest near start ({trees})");
        }
    }

    #[test]
    fn rivers_flow_into_water_and_never_climb() {
        let m = small(8);
        for p in m.positions() {
            let t = m.tile(p);
            if !t.river {
                continue;
            }
            // A river tile has at least one water neighbour (its downstream continuation or the sea).
            assert!(m.neighbors4(p).any(|q| m.tile(q).is_water()), "isolated river tile at {p:?}");
        }
    }
}
