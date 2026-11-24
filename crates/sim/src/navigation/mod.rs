//! Navigation for walkers (docs/game_design.md §2, §3, §5.6, §6): a walk-cost grid, A*
//! pathfinding and a bounded path cache.
//!
//! ## Walk costs
//! Every tile has a cost in permille of a tile on a road with speed 1.0 (a path): roads cost
//! `1000 / speed` (`data/roads.ron`, e.g. paved 714), off-road tiles cost by terrain
//! (`rules.ron` → `walking.terrain_cost_permille`: easy ground 1250, forest 1750, rock 2250,
//! snow 2500, swamp 3750). Blocked: water without a bridge, boulders, building footprints
//! (except gates, the passages through walls) and wall segments. Neighbouring tiles may
//! differ by at most `max_step` elevation levels (one); such a step costs
//! `slope_cost_permille` extra. Bridges are entered, crossed and left only along their axis.
//! Movement is 4-neighbour (N, E, S, W), matching the four walking directions of the art.
//!
//! Building entrances lie outside the footprint (§5.6). Walkers path from entrance tile to
//! entrance tile; stepping through the door into the footprint is added by the movement
//! layer ([`crate::population`]).
//!
//! ## Updates
//! The grid is derived data (not saved): built on first use and after loading, then updated
//! incrementally from the rectangles the road and building change trackers report (placing,
//! removing, moving or upgrading buildings, building or removing roads and bridges). Its
//! revision counts effective changes. Connected components (for O(1) "no path" answers) are
//! relabelled when passability changed.
//!
//! ## Determinism and the budget
//! A* uses integer costs and full tie-breaking (`search`). Answers are remembered per
//! unordered pair of tiles in the path memory (`memory`: least recently used evicted first,
//! the way back is the reversed way there, kept across grid changes while still cheapest).
//! The memory is saved with the game, so a remembered answer costs only
//! `walking.path_cache_hit_cost` node expansions of the per-tick budget of the request queue
//! (movement step) and a loaded game answers exactly like the original. Searches longer than
//! the remaining budget of a tick are continued in the next ticks; when they finish depends
//! only on their total expansions, so a loaded game (no frontier: the search is repeated and
//! caught up) continues exactly. The grid is brought up to date at the end of every tick, so
//! a save never holds changes the memory has not seen.

mod grid;
mod memory;
mod search;
#[cfg(test)]
mod tests;

use glam::IVec2;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use grid::{BLOCKED, road_cost_permille};
pub(crate) use grid::{NavGrid, tile_enterable};
use memory::{GridChange, PathMemory, SavedPaths};
use search::Scratch;

use crate::defs::Defs;
use crate::grid::Direction;
use crate::state::GameState;

/// Counters of the pathfinder (since the game was started or loaded; not saved).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PathStats {
    /// Path requests answered.
    pub requests: u64,
    /// Requests answered from the cache.
    pub cache_hits: u64,
    /// Requests answered at once because start and goal lie in different components.
    pub unreachable: u64,
    /// A* searches run (cache misses).
    pub searches: u64,
    /// Nodes expanded by the searches that ran.
    pub expansions: u64,
    /// Most nodes one search expanded.
    pub max_expansions: u32,
    /// Requests without a path (unreachable or over the node limit).
    pub no_path: u64,
}

impl PathStats {
    /// Share of requests answered from the cache, in permille (0 without requests).
    pub fn cache_hit_permille(&self) -> u64 {
        (self.cache_hits * 1000).checked_div(self.requests).unwrap_or(0)
    }
}

/// Result of a path request (crate-internal; renderers read paths through walkers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchResult {
    /// Steps from start to goal (`None`: no path).
    pub steps: Option<Vec<Direction>>,
    /// Node expansions of the search (charged to the per-tick budget).
    pub expansions: u32,
}

/// The navigation state of a game: grid, path memory, search buffers and counters. The grid
/// is derived from the world, buildings and roads; only the path memory is saved (with the
/// fingerprint of its grid).
#[derive(Clone, Debug, Default)]
pub struct Navigation {
    grid: NavGrid,
    memory: PathMemory,
    /// Path memory of a loaded save, adopted once the grid is rebuilt.
    loaded: Option<SavedPaths>,
    scratch: Scratch,
    pending: Option<Pending>,
    stats: PathStats,
}

impl Serialize for Navigation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let saved = match &self.loaded {
            // Not rebuilt since loading: pass the loaded memory on unchanged.
            Some(saved) if !self.is_built() => saved.clone(),
            _ if self.is_built() => self.memory.to_saved(self.grid.fingerprint()),
            _ => SavedPaths::default(),
        };
        saved.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Navigation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = SavedPaths::deserialize(deserializer)?;
        Ok(Navigation { loaded: Some(saved), ..Navigation::default() })
    }
}

impl Navigation {
    /// Replaces the grid (revisions restart). After loading, the saved path memory is adopted
    /// if it belongs to this grid; otherwise remembered paths the change may have made wrong
    /// are forgotten (all of them when the map size changed).
    fn reset(&mut self, grid: NavGrid, defs: &Defs) {
        let capacity = defs.rules().walking.path_cache_size as usize;
        if let Some(saved) = self.loaded.take() {
            self.memory = PathMemory::adopt(saved, &grid, capacity);
        } else if self.is_built() && self.grid.fits(grid.width(), grid.height()) {
            let changes = grid.diff(&self.grid);
            self.memory.invalidate(&grid, &changes);
        } else {
            self.memory.clear();
        }
        self.grid = grid;
        self.pending = None;
    }

    /// Applies the changes of a grid update to the path memory.
    fn changed(&mut self, changes: &[GridChange]) {
        self.memory.invalidate(&self.grid, changes);
    }

    /// Number of remembered paths.
    pub fn remembered_paths(&self) -> usize {
        self.memory.len()
    }

    /// True once the grid has been built for the map.
    pub fn is_built(&self) -> bool {
        self.grid.len() > 0
    }

    /// Path cost of entering tile `p` in permille of a path tile (`None`: blocked).
    pub fn walk_cost(&self, p: IVec2) -> Option<u16> {
        self.grid.cost(p)
    }

    /// True if a walker may stand on `p`.
    pub fn is_walkable(&self, p: IVec2) -> bool {
        self.grid.cost(p).is_some()
    }

    /// Cost of one step from `p` in direction `d` (`None`: not allowed).
    pub fn step_cost(&self, p: IVec2, d: Direction) -> Option<u32> {
        self.grid.step_cost(p, d)
    }

    /// Connected component of `p` as of the last update (`None`: blocked or not computed).
    pub fn component(&self, p: IVec2) -> Option<u32> {
        self.grid.component(p)
    }

    /// Number of effective grid changes since it was built (wrapping; not saved).
    pub fn revision(&self) -> u64 {
        self.grid.revision()
    }

    /// Pathfinder counters.
    pub fn stats(&self) -> PathStats {
        self.stats
    }

    /// True if a walker standing on `from` can reach `to` (same component; `from` may be
    /// blocked, then one of its enterable neighbours counts).
    pub(crate) fn reachable(&mut self, from: IVec2, to: IVec2) -> bool {
        self.grid.ensure_components();
        self.reachable_now(from, to)
    }

    /// True while the connected components match the grid (see [`Navigation::reachable_known`]).
    pub(crate) fn components_known(&self) -> bool {
        self.grid.components_known()
    }

    /// [`Navigation::reachable`] without relabelling the connected components: `None` while
    /// they are out of date (for read-only checks between ticks).
    pub(crate) fn reachable_known(&self, from: IVec2, to: IVec2) -> Option<bool> {
        self.grid.components_known().then(|| self.reachable_now(from, to))
    }

    fn reachable_now(&self, from: IVec2, to: IVec2) -> bool {
        let Some(goal) = self.grid.component(to) else { return false };
        if self.grid.component(from) == Some(goal) {
            return true;
        }
        Direction::ALL
            .iter()
            .any(|d| self.grid.step_cost(from, *d).is_some() && self.grid.component(from + d.offset()) == Some(goal))
    }

    /// Cheapest path from `from` to `to` without a budget (see [`Navigation::find_path_sliced`]).
    #[cfg(test)]
    pub(crate) fn find_path(&mut self, from: IVec2, to: IVec2, defs: &Defs) -> SearchResult {
        let limit = defs.rules().walking.max_search_nodes;
        match self.find_path_sliced(from, to, 0, limit, defs) {
            PathAnswer::Done(result) => result,
            PathAnswer::Pending => SearchResult { steps: None, expansions: limit },
        }
    }

    /// Cheapest path from `from` to `to` if its search needs at most `already + allowance`
    /// node expansions in total — `already` is what earlier ticks spent on this request (cache,
    /// component check, then A*). Otherwise [`PathAnswer::Pending`]: the search keeps its
    /// frontier and goes on in the next call with the same query (after loading, or when
    /// another search ran in between, it is restarted and caught up).
    ///
    /// Between two enterable tiles the search always runs from the one with the lower index
    /// and the other direction is the reversed path (step costs are symmetric up to the end
    /// tiles, so the reversed cheapest path is the cheapest one back). A remembered answer
    /// (path memory) is a cheapest path of the current grid. `expansions` is the total work of
    /// the search, or `walking.path_cache_hit_cost` for a remembered answer. A search reaching
    /// `max_search_nodes` counts as "no path".
    pub(crate) fn find_path_sliced(&mut self, from: IVec2, to: IVec2, already: u32, allowance: u32, defs: &Defs) -> PathAnswer {
        let walking = &defs.rules().walking;
        let (Some(a), Some(b)) = (self.grid.index(from), self.grid.index(to)) else {
            self.stats.requests += 1;
            self.stats.no_path += 1;
            return PathAnswer::Done(SearchResult { steps: None, expansions: 0 });
        };
        let reversed = a > b && self.grid.cost(from).is_some() && self.grid.cost(to).is_some();
        let (qa, qb, qfrom, qto) = if reversed { (b, a, to, from) } else { (a, b, from, to) };
        let key = (qa as u32, qb as u32);
        let revision = self.grid.revision();
        let max_nodes = walking.max_search_nodes;
        let capacity = walking.path_cache_size as usize;
        let cap = already.saturating_add(allowance).min(max_nodes);
        let hit_cost = walking.path_cache_hit_cost;
        let answer = if let Some((steps, _)) = self.memory.get(key) {
            if hit_cost > cap {
                return PathAnswer::Pending;
            }
            self.stats.cache_hits += 1;
            SearchResult { steps, expansions: hit_cost }
        } else if qfrom != qto && !self.reachable(qfrom, qto) {
            // Answered at once from the components; nothing to remember.
            self.stats.unreachable += 1;
            SearchResult { steps: None, expansions: 0 }
        } else {
            let resumed = self.pending.take().filter(|p| p.key == key && p.revision == revision && self.scratch.holds(&p.search));
            let mut search = match resumed {
                Some(p) => p.search,
                None => {
                    self.stats.searches += 1;
                    self.scratch.begin(&self.grid, qfrom, qto)
                }
            };
            let before = search.expansions;
            let outcome = self.scratch.run(&self.grid, &mut search, cap);
            self.stats.expansions += u64::from(search.expansions - before);
            let steps = match outcome {
                Some(search::Outcome::Found(steps)) => Some(steps),
                Some(search::Outcome::Exhausted) => None,
                None if search.expansions >= max_nodes => None,
                None => {
                    self.pending = Some(Pending { key, revision, search });
                    return PathAnswer::Pending;
                }
            };
            let expansions = search.expansions;
            self.stats.max_expansions = self.stats.max_expansions.max(expansions);
            self.memory.insert(&self.grid, key, steps.as_deref(), expansions, capacity);
            SearchResult { steps, expansions }
        };
        self.stats.requests += 1;
        if answer.steps.is_none() {
            self.stats.no_path += 1;
        }
        if reversed {
            let steps = answer.steps.map(|s| s.iter().rev().map(|d| d.opposite()).collect());
            return PathAnswer::Done(SearchResult { steps, expansions: answer.expansions });
        }
        PathAnswer::Done(answer)
    }
}

/// A search that did not finish within its budget and goes on in the next tick.
#[derive(Clone, Debug)]
struct Pending {
    key: (u32, u32),
    revision: u64,
    search: search::Search,
}

/// Answer of a budgeted path request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PathAnswer {
    /// The path, or definitely none (unreachable or over `max_search_nodes`).
    Done(SearchResult),
    /// The search needs more expansions than allowed so far; ask again next tick.
    Pending,
}

/// Brings the navigation grid of `state` up to date: a full build on first use or after the
/// map size changed (loading), otherwise the tiles of the rectangles the road and building
/// change trackers reported since the last call.
pub(crate) fn sync(state: &mut GameState, defs: &Defs) {
    let roads = state.roads.changes_mut().take_rects();
    let buildings = state.buildings.changes_mut().take_rects();
    let land = state.diplomacy.land.changes_mut().take_rects();
    let (w, h) = (state.world.width(), state.world.height());
    if !state.navigation.grid.fits(w, h) || roads.is_none() || buildings.is_none() || land.is_none() {
        state.navigation.reset(NavGrid::build(&state.world, &state.buildings, &state.roads, &state.diplomacy.land, defs), defs);
        return;
    }
    let mut rects = roads.unwrap_or_default();
    rects.extend(buildings.unwrap_or_default());
    rects.extend(land.unwrap_or_default());
    if !rects.is_empty() {
        let changes = state.navigation.grid.update(&state.world, &state.buildings, &state.roads, &state.diplomacy.land, defs, &rects);
        state.navigation.changed(&changes);
    }
}

/// Brings the grid ([`sync`]) and its connected components up to date, so that
/// [`Navigation::component`] answers for the current buildings and roads. Steps that compare
/// components call this first (the labels then only depend on the current passability, so a
/// loaded game sees the same labels).
pub(crate) fn prepare(state: &mut GameState, defs: &Defs) {
    sync(state, defs);
    state.navigation.grid.ensure_components();
}

/// Rebuilds the grid from scratch (after loading, rebalanced data, or direct world edits in
/// tests); pending change rectangles are dropped.
pub(crate) fn rebuild(state: &mut GameState, defs: &Defs) {
    state.roads.changes_mut().take_rects();
    state.buildings.changes_mut().take_rects();
    state.diplomacy.land.changes_mut().take_rects();
    state.navigation.reset(NavGrid::build(&state.world, &state.buildings, &state.roads, &state.diplomacy.land, defs), defs);
}
