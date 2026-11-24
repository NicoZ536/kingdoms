//! A* on 4-neighbour tile steps with integer costs and full deterministic tie-breaking. A
//! search can be run in slices ([`Scratch::run`]): long searches spread over several ticks.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use glam::IVec2;

use super::grid::NavGrid;
use crate::grid::Direction;

/// Per-tile search data, valid while `seen` equals the search stamp.
#[derive(Clone, Copy, Debug, Default)]
struct Node {
    seen: u32,
    closed: u32,
    g: u32,
    came: u8,
}

/// How a search ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The cheapest path.
    Found(Vec<Direction>),
    /// Every reachable tile was expanded: there is no path.
    Exhausted,
}

/// A started search: its query and the nodes expanded so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Search {
    pub from: IVec2,
    pub to: IVec2,
    start: usize,
    goal: usize,
    unit: u32,
    stamp: u32,
    /// Nodes expanded so far.
    pub expansions: u32,
}

/// Reusable search buffers (one entry per tile, valid while their stamp matches) and the
/// frontier of the current search.
#[derive(Clone, Debug, Default)]
pub(crate) struct Scratch {
    nodes: Vec<Node>,
    stamp: u32,
    heap: BinaryHeap<Reverse<(u32, u32, u32)>>,
}

impl Scratch {
    /// Starts a search from `from` to `to` (both on the map). `from` may be blocked (walkers
    /// leaving a door or a tile that was just built over).
    pub(crate) fn begin(&mut self, grid: &NavGrid, from: IVec2, to: IVec2) -> Search {
        let len = grid.len();
        if self.nodes.len() != len || self.stamp == u32::MAX {
            self.nodes = vec![Node::default(); len];
            self.stamp = 0;
        }
        self.stamp += 1;
        self.heap.clear();
        let start = grid.index(from).unwrap_or(0);
        let goal = grid.index(to).unwrap_or(0);
        let search = Search { from, to, start, goal, unit: u32::from(grid.min_cost().max(1)), stamp: self.stamp, expansions: 0 };
        self.nodes[start] = Node { seen: self.stamp, closed: 0, g: 0, came: 0 };
        let h0 = search.h(from);
        self.heap.push(Reverse((h0, h0, start as u32)));
        search
    }

    /// True if the buffers still hold the frontier of `search`.
    pub(crate) fn holds(&self, search: &Search) -> bool {
        self.stamp == search.stamp && self.nodes.len() > search.start.max(search.goal)
    }

    /// Continues `search` until it ends or has expanded `max_total` nodes in total (`None`:
    /// not finished yet; call again with a higher limit to go on).
    ///
    /// A*: heuristic = Manhattan distance × the lowest tile cost, so paths are optimal. Ties
    /// are broken by (f, h, tile index), neighbours are expanded in the order N, E, S, W and a
    /// node keeps its first cheapest predecessor, so the result depends only on the grid.
    pub(crate) fn run(&mut self, grid: &NavGrid, search: &mut Search, max_total: u32) -> Option<Outcome> {
        if search.from == search.to {
            return Some(Outcome::Found(Vec::new()));
        }
        if grid.cost(search.to).is_none() {
            return Some(Outcome::Exhausted);
        }
        let stamp = search.stamp;
        while search.expansions < max_total {
            let Some(Reverse((_, _, index))) = self.heap.pop() else { return Some(Outcome::Exhausted) };
            let i = index as usize;
            if self.nodes[i].closed == stamp {
                continue;
            }
            self.nodes[i].closed = stamp;
            search.expansions += 1;
            if i == search.goal {
                return Some(Outcome::Found(self.reconstruct(grid, search.start, search.goal)));
            }
            let p = grid.pos(i);
            let g = self.nodes[i].g;
            for d in Direction::ALL {
                let Some((j, cost)) = grid.step_from(i, p.x, d) else { continue };
                let node = &mut self.nodes[j];
                if node.closed == stamp {
                    continue;
                }
                let ng = g.saturating_add(cost);
                if node.seen != stamp || ng < node.g {
                    *node = Node { seen: stamp, closed: node.closed, g: ng, came: d.index() as u8 };
                    let hq = search.h(p + d.offset());
                    self.heap.push(Reverse((ng.saturating_add(hq), hq, j as u32)));
                }
            }
        }
        None
    }

    fn reconstruct(&self, grid: &NavGrid, start: usize, goal: usize) -> Vec<Direction> {
        let mut steps = Vec::new();
        let mut i = goal;
        while i != start {
            let d = Direction::ALL[usize::from(self.nodes[i].came & 3)];
            steps.push(d);
            let Some(prev) = grid.index(grid.pos(i) - d.offset()) else { break };
            i = prev;
        }
        steps.reverse();
        steps
    }
}

impl Search {
    fn h(&self, p: IVec2) -> u32 {
        ((p - self.to).abs().element_sum() as u32).saturating_mul(self.unit)
    }
}
