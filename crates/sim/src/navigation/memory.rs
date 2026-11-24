//! The path memory (docs/architecture.md §3.3, §3.6): answered paths per unordered pair of
//! tiles, kept across grid changes as long as they stay the cheapest path, evicted least
//! recently used first, and saved with the game.
//!
//! * **Invalidation** after every grid update ([`PathMemory::invalidate`]): a path whose
//!   bounding box contains a tile that became dearer or blocked (or whose bridge or elevation
//!   changed) is forgotten; a tile that became cheaper or enterable is a possible shortcut, so
//!   every path is forgotten that a route through the changed rectangle could beat — its lower
//!   bound (Manhattan distance via the rectangle × the cheapest tile cost) is below the
//!   remembered cost. Remembered "no path" answers are forgotten on every change. What stays
//!   is therefore still a cheapest path.
//! * **Saving:** the entries and a fingerprint of the grid they belong to are part of the save.
//!   After loading, the rebuilt grid is compared with the fingerprint and every entry is
//!   checked step by step (bounds, allowed steps, end tile, cost); only then is it adopted. So
//!   a loaded game answers exactly like the original one, and a damaged or edited memory is
//!   simply forgotten.

use std::collections::BTreeMap;

use glam::IVec2;
use serde::{Deserialize, Serialize};

use super::grid::NavGrid;
use crate::grid::{Direction, TileRect};

/// A rectangle of tiles whose walking changed in one grid update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GridChange {
    /// Bounding box of the changed tiles.
    pub rect: TileRect,
    /// Some tile became cheaper or enterable (a possible shortcut).
    pub cheaper: bool,
    /// Some tile became dearer or blocked, or a bridge or elevation changed.
    pub dearer: bool,
}

/// Collects the changed tiles of one rectangle into a [`GridChange`].
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ChangeCollector {
    bounds: Option<(IVec2, IVec2)>,
    cheaper: bool,
    dearer: bool,
}

impl ChangeCollector {
    pub(crate) fn add(&mut self, p: IVec2, cheaper: bool, dearer: bool) {
        self.bounds = Some(match self.bounds {
            Some((lo, hi)) => (lo.min(p), hi.max(p)),
            None => (p, p),
        });
        self.cheaper |= cheaper;
        self.dearer |= dearer;
    }

    pub(crate) fn finish(self) -> Option<GridChange> {
        let (lo, hi) = self.bounds?;
        Some(GridChange { rect: TileRect::new(lo, hi - lo + IVec2::ONE), cheaper: self.cheaper, dearer: self.dearer })
    }
}

/// One remembered answer, from the lower tile index of the pair to the higher one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Remembered {
    /// Steps, four per byte (two bits each, direction index); `None`: no path.
    packed: Option<Vec<u8>>,
    /// Number of steps.
    len: u32,
    /// Path cost (sum of the step costs).
    cost: u32,
    /// Bounding box of all tiles of the path: min x, min y, max x, max y.
    bounds: [i32; 4],
    /// Node expansions of the search that found it (statistics).
    expansions: u32,
    /// Use serial (least recently used is evicted first).
    used: u64,
}

impl Remembered {
    fn steps(&self) -> Option<Vec<Direction>> {
        let packed = self.packed.as_ref()?;
        Some((0..self.len as usize).map(|i| Direction::ALL[usize::from((packed[i / 4] >> ((i % 4) * 2)) & 3)]).collect())
    }

    fn intersects(&self, r: &TileRect) -> bool {
        let [x0, y0, x1, y1] = self.bounds;
        x0 < r.origin.x + r.size.x && r.origin.x <= x1 && y0 < r.origin.y + r.size.y && r.origin.y <= y1
    }
}

/// The answered paths (see the module documentation).
#[derive(Clone, Debug, Default)]
pub(crate) struct PathMemory {
    entries: BTreeMap<(u32, u32), Remembered>,
    by_use: BTreeMap<u64, (u32, u32)>,
    serial: u64,
}

/// What a save holds of the path memory.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedPaths {
    /// [`NavGrid::fingerprint`] of the grid the paths belong to (0: no grid).
    pub grid: u64,
    serial: u64,
    entries: Vec<((u32, u32), Remembered)>,
}

impl PathMemory {
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.by_use.clear();
    }

    /// The remembered answer for `key` (marked as used): steps (`None`: no path) and the
    /// expansions of the search that found it.
    pub(crate) fn get(&mut self, key: (u32, u32)) -> Option<(Option<Vec<Direction>>, u32)> {
        let entry = self.entries.get_mut(&key)?;
        self.by_use.remove(&entry.used);
        self.serial += 1;
        entry.used = self.serial;
        self.by_use.insert(self.serial, key);
        Some((entry.steps(), entry.expansions))
    }

    /// Remembers the answer for `key` (steps from the tile with index `key.0`), evicting the
    /// least recently used entries beyond `capacity`.
    pub(crate) fn insert(&mut self, grid: &NavGrid, key: (u32, u32), steps: Option<&[Direction]>, expansions: u32, capacity: usize) {
        let start = grid.pos(key.0 as usize);
        let (packed, len, cost, bounds) = match steps {
            Some(steps) => {
                let mut packed = vec![0u8; steps.len().div_ceil(4)];
                let (mut p, mut cost) = (start, 0u32);
                let (mut lo, mut hi) = (start, start);
                for (i, d) in steps.iter().enumerate() {
                    packed[i / 4] |= (d.index() as u8) << ((i % 4) * 2);
                    cost = cost.saturating_add(grid.step_cost(p, *d).unwrap_or(0));
                    p += d.offset();
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
                (Some(packed), steps.len() as u32, cost, [lo.x, lo.y, hi.x, hi.y])
            }
            None => (None, 0, 0, [start.x, start.y, start.x, start.y]),
        };
        if let Some(old) = self.entries.remove(&key) {
            self.by_use.remove(&old.used);
        }
        while self.entries.len() >= capacity.max(1) {
            let Some((_, oldest)) = self.by_use.pop_first() else { break };
            self.entries.remove(&oldest);
        }
        self.serial += 1;
        self.by_use.insert(self.serial, key);
        self.entries.insert(key, Remembered { packed, len, cost, bounds, expansions, used: self.serial });
    }

    /// Forgets every answer `changes` may have made wrong or beaten (module documentation);
    /// `grid` is the updated grid.
    pub(crate) fn invalidate(&mut self, grid: &NavGrid, changes: &[GridChange]) {
        if changes.is_empty() || self.entries.is_empty() {
            return;
        }
        let unit = i64::from(grid.min_cost().max(1));
        let stale: Vec<(u32, u32)> = self
            .entries
            .iter()
            .filter(|(key, e)| {
                e.packed.is_none()
                    || changes.iter().any(|c| {
                        (c.dearer && e.intersects(&c.rect)) || (c.cheaper && shortcut_bound(grid, **key, &c.rect, unit) < i64::from(e.cost))
                    })
            })
            .map(|(key, _)| *key)
            .collect();
        for key in stale {
            if let Some(e) = self.entries.remove(&key) {
                self.by_use.remove(&e.used);
            }
        }
    }

    /// The memory for a save (`grid`: fingerprint of the grid it belongs to).
    pub(crate) fn to_saved(&self, grid: u64) -> SavedPaths {
        SavedPaths { grid, serial: self.serial, entries: self.entries.iter().map(|(k, e)| (*k, e.clone())).collect() }
    }

    /// Adopts the paths of a save for `grid` (whose fingerprint must match) after checking
    /// every entry; entries that do not fit the grid are dropped.
    pub(crate) fn adopt(saved: SavedPaths, grid: &NavGrid, capacity: usize) -> PathMemory {
        let mut memory = PathMemory::default();
        if saved.grid == 0 || saved.grid != grid.fingerprint() {
            return memory;
        }
        let len = grid.len();
        for (key, entry) in saved.entries {
            let (a, b) = (key.0 as usize, key.1 as usize);
            if a >= len || b >= len || entry.used > saved.serial || memory.by_use.contains_key(&entry.used) || !consistent(grid, key, &entry) {
                continue;
            }
            memory.by_use.insert(entry.used, key);
            memory.entries.insert(key, entry);
        }
        while memory.entries.len() > capacity.max(1) {
            let Some((_, oldest)) = memory.by_use.pop_first() else { break };
            memory.entries.remove(&oldest);
        }
        memory.serial = saved.serial;
        memory
    }
}

/// Lower bound of the cost of a path between the tiles of `key` that passes through `rect`:
/// the Manhattan distance via the rectangle × the cheapest tile cost.
fn shortcut_bound(grid: &NavGrid, key: (u32, u32), rect: &TileRect, unit: i64) -> i64 {
    let (s, g) = (grid.pos(key.0 as usize), grid.pos(key.1 as usize));
    let (lo, hi) = (s.min(g), s.max(g));
    let (r0, r1) = (rect.origin, rect.origin + rect.size - IVec2::ONE);
    let gap = |a0: i32, a1: i32, b0: i32, b1: i32| i64::from((b0 - a1).max(0) + (a0 - b1).max(0));
    let detour = gap(lo.x, hi.x, r0.x, r1.x) + gap(lo.y, hi.y, r0.y, r1.y);
    (i64::from((g - s).abs().element_sum()) + 2 * detour) * unit
}

/// True if `entry` describes a walkable path of the grid from `key.0` to `key.1` with its cost
/// and bounds (loading).
fn consistent(grid: &NavGrid, key: (u32, u32), entry: &Remembered) -> bool {
    let (start, goal) = (grid.pos(key.0 as usize), grid.pos(key.1 as usize));
    match &entry.packed {
        None => return entry.len == 0 && entry.cost == 0,
        Some(packed) if entry.len as usize > grid.len() || packed.len() != (entry.len as usize).div_ceil(4) => return false,
        Some(_) => {}
    }
    let Some(steps) = entry.steps() else { return false };
    let (mut p, mut cost) = (start, 0u32);
    let (mut lo, mut hi) = (start, start);
    for d in &steps {
        let Some(c) = grid.step_cost(p, *d) else { return false };
        cost = cost.saturating_add(c);
        p += d.offset();
        lo = lo.min(p);
        hi = hi.max(p);
    }
    p == goal && cost == entry.cost && entry.bounds == [lo.x, lo.y, hi.x, hi.y]
}
