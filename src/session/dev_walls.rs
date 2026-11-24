//! Developer aid `KINGDOMS_DEV_WALLS=<radius>`: rings the headquarters with a closed town wall
//! through regular commands, so sieges (strikes against walls and gates) can be checked on the
//! map. It unlocks all technologies (`DebugUnlockAllResearch`) and grants gold, stone and planks
//! (`DebugGrant`), places a wall segment on every tile of the square ring `radius` tiles around
//! the headquarters – the four corners included – entrance inwards (`PlaceBuilding`; a road
//! crossing the ring is removed first with `RemoveRoad`), and, once the segments stand as sites,
//! a gate on every road crossing of a side and in the middle of each side without one (or on
//! the standing piece of that side nearest to the middle, if the middle was refused). Wall
//! pieces and gates are entered from any side and need no road (docs/game_design.md §5.5): the
//! builders raise the ring from inside, the corners (whose entrance faces the next piece of
//! wall) from outside through the gates. Rejected spots (water, other buildings, outside the
//! territory) are logged and stay open.

use glam::IVec2;
use kingdoms_sim::buildings::Rotation;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingRole, Defs, ResourceAmount};
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::TICKS_PER_HOUR;

use super::dev_settlement::headquarters_center;

/// Smallest and largest ring radius in tiles.
pub const RADIUS_RANGE: std::ops::RangeInclusive<u32> = 4..=24;

/// Every tile of the square ring at Chebyshev distance `radius` around `center`, the corners
/// included, row by row.
pub fn ring(center: IVec2, radius: i32) -> Vec<IVec2> {
    let mut tiles = Vec::new();
    for y in -radius..=radius {
        for x in -radius..=radius {
            if x.abs() == radius || y.abs() == radius {
                tiles.push(center + IVec2::new(x, y));
            }
        }
    }
    tiles
}

/// True for the four corner tiles of the ring.
fn is_corner(center: IVec2, radius: i32, p: IVec2) -> bool {
    let d = (p - center).abs();
    d.x == radius && d.y == radius
}

/// Orientation of a wall piece on the ring at `p`: its entrance faces inwards (0: south,
/// 1: west, 2: north, 3: east). A corner counts to the north or south side, so its entrance is
/// the next piece of the west or east side.
fn inward(center: IVec2, radius: i32, p: IVec2) -> Rotation {
    Rotation::wrapping(match side_of(center, radius, p) {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => 3,
    })
}

/// The middle tiles of the four sides of the ring.
fn side_middles(center: IVec2, radius: i32) -> [IVec2; 4] {
    [IVec2::new(0, -radius), IVec2::new(radius, 0), IVec2::new(0, radius), IVec2::new(-radius, 0)].map(|d| center + d)
}

/// The side of the ring a tile lies on (0 north, 1 east, 2 south, 3 west; corners count to
/// north and south).
fn side_of(center: IVec2, radius: i32, p: IVec2) -> usize {
    let d = p - center;
    if d.y == -radius {
        0
    } else if d.y == radius {
        2
    } else if d.x == radius {
        1
    } else {
        3
    }
}

/// The gate tiles of the ring: every road crossing off the corners (a gate in a corner would
/// lead into the wall), and the middle of each side without one.
fn gate_tiles(center: IVec2, radius: i32, crossings: &[IVec2]) -> Vec<IVec2> {
    let mut gates: Vec<IVec2> = crossings.iter().copied().filter(|p| !is_corner(center, radius, *p)).collect();
    for (side, middle) in side_middles(center, radius).into_iter().enumerate() {
        if !gates.iter().any(|g| side_of(center, radius, *g) == side) {
            gates.push(middle);
        }
    }
    gates
}

/// The tile of the side of `p` nearest to `p` (`p` first, then alternately along the side,
/// corners excluded) for which `fits` holds.
fn nearest_on_side(center: IVec2, radius: i32, p: IVec2, fits: impl Fn(IVec2) -> bool) -> Option<IVec2> {
    let along = if matches!(side_of(center, radius, p), 0 | 2) { IVec2::X } else { IVec2::Y };
    (0..2 * radius)
        .flat_map(|i| [p + along * i, p - along * i])
        .filter(|q| (*q - center).abs().max_element() == radius && !is_corner(center, radius, *q))
        .find(|q| fits(*q))
}

/// The developer wall plan.
#[derive(Debug)]
pub struct DevWalls {
    radius: i32,
    /// Tiles that get a gate (filled when the ring is placed).
    gates: Vec<IVec2>,
    placed: bool,
    /// Rejected wall or gate commands so far.
    rejected: u32,
    next_tick: u64,
}

impl DevWalls {
    /// The plan and its first commands (research, gold, stone, planks).
    pub fn start(defs: &Defs, radius: u32) -> (Self, Vec<Command>) {
        let tiles = 8 * radius;
        let goods = [("stone", 3 * tiles + 60), ("planks", 30)]
            .into_iter()
            .filter_map(|(key, amount)| defs.resource_id(key).map(|resource| ResourceAmount { resource, amount }))
            .collect();
        log::info!("KINGDOMS_DEV_WALLS: town wall {radius} tiles around the headquarters");
        let commands = vec![Command::DebugUnlockAllResearch, Command::DebugGrant { gold: 3 * tiles + 200, goods }];
        let radius = i32::try_from(radius).unwrap_or(8);
        (Self { radius, gates: Vec::new(), placed: false, rejected: 0, next_tick: 0 }, commands)
    }

    /// Commands after a tick: the ring of walls, then the gates.
    pub fn after_tick(&mut self, state: &GameState, defs: &Defs, events: &[SimEvent]) -> Vec<Command> {
        for e in events {
            if let SimEvent::CommandRejected { command: Command::PlaceBuilding { origin, .. }, reason } = e
                && self.on_ring(state, defs, *origin)
            {
                self.rejected += 1;
                log::info!("KINGDOMS_DEV_WALLS: no wall at {origin}: {reason} ({} so far)", self.rejected);
            }
        }
        let now = state.clock().tick;
        if now < self.next_tick {
            return Vec::new();
        }
        self.next_tick = now + TICKS_PER_HOUR / 4;
        let (Some(wall), Some(gate)) = (defs.building_id("wall"), defs.building_id("gate")) else { return Vec::new() };
        let Some(center) = headquarters_center(state, defs) else { return Vec::new() };
        if !self.placed {
            self.placed = true;
            let tiles = ring(center, self.radius);
            let roads: Vec<IVec2> = tiles.iter().copied().filter(|p| state.roads().get(*p).is_some()).collect();
            self.gates = gate_tiles(center, self.radius, &roads);
            log::info!("KINGDOMS_DEV_WALLS: {} segments, gates at {:?}", tiles.len(), self.gates);
            let mut commands = Vec::with_capacity(tiles.len() + 1);
            if !roads.is_empty() {
                commands.push(Command::RemoveRoad { tiles: roads });
            }
            commands.extend(tiles.into_iter().map(|origin| Command::PlaceBuilding { kind: wall, origin, rotation: inward(center, self.radius, origin) }));
            return commands;
        }
        // Gates replace standing segments (sites or finished).
        let role_at = |p: IVec2| state.buildings().at(p).and_then(|(_, b)| defs.buildings().get(b.kind().index())).map(|d| d.role);
        let radius = self.radius;
        let planned = self.gates.clone();
        let mut commands = Vec::new();
        for p in &mut self.gates {
            if role_at(*p).is_none() {
                // The segment was refused (water, a building): the nearest standing piece of
                // that side gets the gate instead.
                let free = |q: IVec2| role_at(q) == Some(BuildingRole::WallSegment) && !planned.contains(&q);
                if let Some(q) = nearest_on_side(center, radius, *p, free) {
                    log::info!("KINGDOMS_DEV_WALLS: gate at {q} instead of {p}");
                    *p = q;
                }
            }
        }
        self.gates.retain(|p| match role_at(*p) {
            Some(BuildingRole::Gate) => false,
            Some(BuildingRole::WallSegment) => {
                commands.push(Command::PlaceBuilding { kind: gate, origin: *p, rotation: inward(center, radius, *p) });
                true
            }
            // Nothing stands on that side: no gate either.
            _ => false,
        });
        commands
    }

    fn on_ring(&self, state: &GameState, defs: &Defs, p: IVec2) -> bool {
        headquarters_center(state, defs).is_some_and(|c| {
            let d = (p - c).abs();
            d.x.max(d.y) == self.radius
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tile a piece's entrance lies on.
    fn entrance(c: IVec2, radius: i32, p: IVec2) -> IVec2 {
        p + match inward(c, radius, p).quarter_turns() {
            0 => IVec2::Y,
            1 => IVec2::NEG_X,
            2 => IVec2::NEG_Y,
            _ => IVec2::X,
        }
    }

    #[test]
    fn the_ring_is_closed_and_faces_inwards() {
        let c = IVec2::new(10, 10);
        let tiles = ring(c, 3);
        assert_eq!(tiles.len(), 24, "8 × radius tiles, the corners included");
        assert!(tiles.iter().all(|p| (*p - c).abs().max_element() == 3));
        // Closed: every piece has exactly two 4-neighbours on the ring.
        for p in &tiles {
            let neighbours = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y].iter().filter(|d| tiles.contains(&(*p + **d))).count();
            assert_eq!(neighbours, 2, "{p}");
        }
        // A side piece faces the inside; a corner faces the next piece of wall.
        for p in &tiles {
            let e = entrance(c, 3, *p);
            if is_corner(c, 3, *p) {
                assert!(tiles.contains(&e), "corner {p} faces {e}");
            } else {
                assert_eq!((e - c).abs().max_element(), 2, "{p} faces {e}");
            }
        }
        let middles = side_middles(c, 3);
        assert!(middles.iter().all(|m| tiles.contains(m)));
        assert_eq!(middles.map(|m| side_of(c, 3, m)), [0, 1, 2, 3]);
        assert_eq!(side_of(c, 3, IVec2::new(13, 13)), 2, "corners belong to north and south");
    }

    #[test]
    fn gates_stand_on_crossings_and_side_middles_never_in_a_corner() {
        let c = IVec2::new(10, 10);
        // A road crosses the east side at (13, 11) and the north-west corner.
        let gates = gate_tiles(c, 3, &[IVec2::new(13, 11), IVec2::new(7, 7)]);
        assert_eq!(gates, vec![IVec2::new(13, 11), IVec2::new(10, 7), IVec2::new(10, 13), IVec2::new(7, 10)]);
        assert!(gates.iter().all(|g| !is_corner(c, 3, *g)));
        // A refused middle: the nearest standing piece of the side, never a corner.
        let standing = |q: IVec2| q != IVec2::new(13, 10) && q != IVec2::new(13, 11);
        assert_eq!(nearest_on_side(c, 3, IVec2::new(13, 10), standing), Some(IVec2::new(13, 9)));
        assert_eq!(nearest_on_side(c, 3, IVec2::new(10, 7), |q| q.x >= 12), Some(IVec2::new(12, 7)));
        assert_eq!(nearest_on_side(c, 3, IVec2::new(10, 7), |q| q.x == 13), None, "corner");
    }
}
