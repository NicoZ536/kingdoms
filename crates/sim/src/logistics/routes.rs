//! Travel times on the road network (Dijkstra over road tiles, weighted by road speed).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use glam::IVec2;

use crate::defs::Defs;
use crate::grid::Direction;
use crate::roads::RoadNetwork;

/// Milli-ticks a walker needs to enter a tile of road type with `speed` multiplier.
fn tile_cost_milli(defs: &Defs, speed: f32) -> u64 {
    let tiles_per_tick = f64::from(defs.rules().base_walk_speed) * f64::from(speed.max(0.01));
    (1000.0 / tiles_per_tick).round().max(1.0) as u64
}

/// Ticks to walk on roads from road tile `from` to road tile `to` (`None` if either has no
/// road or they are not connected). The `HashMap` is only used for lookups, never iterated,
/// so the result is deterministic.
pub fn travel_ticks(roads: &RoadNetwork, defs: &Defs, from: IVec2, to: IVec2) -> Option<u64> {
    if !roads.connected(from, to) {
        return None;
    }
    if from == to {
        return Some(0);
    }
    let key = |p: IVec2| (p.x, p.y);
    let mut best: HashMap<(i32, i32), u64> = HashMap::new();
    let mut heap = BinaryHeap::new();
    best.insert(key(from), 0);
    heap.push(Reverse((0u64, from.y, from.x)));
    while let Some(Reverse((cost, y, x))) = heap.pop() {
        let p = IVec2::new(x, y);
        if p == to {
            return Some(cost.div_ceil(1000));
        }
        if best.get(&key(p)).is_some_and(|b| *b < cost) {
            continue;
        }
        for d in Direction::ALL {
            if !roads.links(p, d) {
                continue;
            }
            let q = p + d.offset();
            let Some(tile) = roads.get(q) else { continue };
            let speed = defs.roads().get(tile.road.index()).map_or(1.0, |r| r.speed);
            let next = cost + tile_cost_milli(defs, speed);
            if best.get(&key(q)).is_none_or(|b| next < *b) {
                best.insert(key(q), next);
                heap.push(Reverse((next, q.y, q.x)));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roads::RoadTile;
    use crate::test_support::{defs, flat_state};

    #[test]
    fn travel_time_depends_on_length_and_road_type() {
        let d = defs();
        let mut state = flat_state(32);
        let path = d.road_id("path").unwrap();
        let paved = d.road_id("paved").unwrap();
        for x in 2..=12 {
            state.roads.set(IVec2::new(x, 5), Some(RoadTile { road: path, bridge_axis: None }));
        }
        state.roads.rebuild_components();
        // 10 steps on a path: 10 × 5 ticks.
        assert_eq!(travel_ticks(&state.roads, d, IVec2::new(2, 5), IVec2::new(12, 5)), Some(50));
        assert_eq!(travel_ticks(&state.roads, d, IVec2::new(2, 5), IVec2::new(2, 5)), Some(0));
        assert_eq!(travel_ticks(&state.roads, d, IVec2::new(2, 5), IVec2::new(2, 6)), None);
        for x in 3..=12 {
            state.roads.set(IVec2::new(x, 5), Some(RoadTile { road: paved, bridge_axis: None }));
        }
        state.roads.rebuild_components();
        let fast = travel_ticks(&state.roads, d, IVec2::new(2, 5), IVec2::new(12, 5)).unwrap();
        assert!((35..50).contains(&fast), "{fast}");
    }
}
