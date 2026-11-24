//! Developer aid `KINGDOMS_DEV_WALLS=<radius>`: rings the headquarters with a town wall through
//! regular commands, so sieges (strikes against walls and gates) can be checked on the map. It
//! unlocks all technologies (`DebugUnlockAllResearch`) and grants gold, stone and planks
//! (`DebugGrant`), lays a road ring one tile inside the wall with a road from the town to it
//! (`BuildRoad`: construction sites need a road at their entrance) and places a wall segment on
//! every side tile of the square ring `radius` tiles around the headquarters, entrance inwards
//! (`PlaceBuilding`; a road crossing the ring is removed first with `RemoveRoad`), and, once the
//! segments stand as sites, a gate on every road crossing and in the middle of each side without
//! one. The four corner tiles stay open: both their inner neighbours are wall, so no road can
//! reach their entrance. The builders raise the wall like any other; rejected spots (water,
//! other buildings, outside the territory) are logged and stay open.

use glam::IVec2;
use kingdoms_sim::buildings::Rotation;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingRole, Defs, ResourceAmount};
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::TICKS_PER_HOUR;
use kingdoms_sim::world::Vegetation;

use super::dev_settlement::{cheapest_road, headquarters_center};

/// Smallest and largest ring radius in tiles.
pub const RADIUS_RANGE: std::ops::RangeInclusive<u32> = 4..=24;

/// The side tiles of the square ring at Chebyshev distance `radius` around `center` (corners
/// excluded), row by row.
pub fn ring(center: IVec2, radius: i32) -> Vec<IVec2> {
    let mut tiles = Vec::new();
    for y in -radius..=radius {
        for x in -radius..=radius {
            if (x.abs() == radius) != (y.abs() == radius) {
                tiles.push(center + IVec2::new(x, y));
            }
        }
    }
    tiles
}

/// The closed road loop at Chebyshev distance `radius` around `center` as four chains (one per
/// side, each a 4-neighbour path ending where the next begins).
fn road_loop(center: IVec2, radius: i32) -> [Vec<IVec2>; 4] {
    let r = radius;
    let line = |from: IVec2, step: IVec2| -> Vec<IVec2> { (0..=2 * r).map(|i| center + from + step * i).collect() };
    [line(IVec2::new(-r, -r), IVec2::X), line(IVec2::new(r, -r), IVec2::Y), line(IVec2::new(r, r), IVec2::NEG_X), line(IVec2::new(-r, r), IVec2::NEG_Y)]
}

/// Orientation of a wall piece on the ring at `p`: its entrance faces the road inside (0: south,
/// 1: west, 2: north, 3: east).
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
            match e {
                SimEvent::CommandRejected { command: Command::PlaceBuilding { origin, .. }, reason } if self.on_ring(state, defs, *origin) => {
                    self.rejected += 1;
                    log::info!("KINGDOMS_DEV_WALLS: no wall at {origin}: {reason} ({} so far)", self.rejected);
                }
                SimEvent::CommandRejected { command: Command::BuildRoad { tiles, .. }, reason } => {
                    log::info!("KINGDOMS_DEV_WALLS: no road from {:?}: {reason}", tiles.first());
                }
                _ => {}
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
            let Some(road) = cheapest_road(state, defs) else { return Vec::new() };
            let tiles = ring(center, self.radius);
            let roads: Vec<IVec2> = tiles.iter().copied().filter(|p| state.roads().get(*p).is_some()).collect();
            // A gate on every road crossing, and in the middle of each side without one.
            self.gates = roads.clone();
            for (side, middle) in side_middles(center, self.radius).into_iter().enumerate() {
                if !roads.iter().any(|r| side_of(center, self.radius, *r) == side) {
                    self.gates.push(middle);
                }
            }
            log::info!("KINGDOMS_DEV_WALLS: {} segments, gates at {:?}", tiles.len(), self.gates);
            let mut commands = Vec::with_capacity(tiles.len() + 6);
            if !roads.is_empty() {
                commands.push(Command::RemoveRoad { tiles: roads });
            }
            // The road ring inside the wall, and a road from it towards the headquarters until
            // it meets the town's roads.
            for chain in road_loop(center, self.radius - 1) {
                // Split where a road cannot go (boulders, water, buildings): the rest still helps.
                let open = |p: &IVec2| {
                    state.world().get(*p).is_some_and(|t| !t.is_water() && t.vegetation != Vegetation::Boulder) && state.buildings().at(*p).is_none()
                };
                for run in chain.split(|p| !open(p)).filter(|run| run.len() > 1) {
                    commands.push(Command::BuildRoad { tiles: run.to_vec(), road });
                }
            }
            let spoke: Vec<IVec2> = (1..self.radius)
                .rev()
                .map(|d| center + IVec2::new(0, d))
                .take_while(|p| state.roads().get(*p).is_none() && state.buildings().at(*p).is_none())
                .collect();
            if spoke.len() > 1 {
                commands.push(Command::BuildRoad { tiles: spoke, road });
            }
            commands.extend(tiles.into_iter().map(|origin| Command::PlaceBuilding { kind: wall, origin, rotation: inward(center, self.radius, origin) }));
            return commands;
        }
        // Gates replace standing segments (sites or finished).
        let role_at = |p: IVec2| state.buildings().at(p).and_then(|(_, b)| defs.buildings().get(b.kind().index())).map(|d| d.role);
        let mut commands = Vec::new();
        self.gates.retain(|p| match role_at(*p) {
            Some(BuildingRole::Gate) => false,
            Some(BuildingRole::WallSegment) => {
                commands.push(Command::PlaceBuilding { kind: gate, origin: *p, rotation: inward(center, self.radius, *p) });
                true
            }
            // Nothing stands there (the segment was rejected): no gate either.
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

    #[test]
    fn the_ring_has_its_sides_and_a_road_inside() {
        let c = IVec2::new(10, 10);
        let tiles = ring(c, 3);
        assert_eq!(tiles.len(), 20, "8 × radius tiles without the corners");
        assert!(tiles.iter().all(|p| (*p - c).abs().max_element() == 3 && (*p - c).abs().min_element() < 3));
        // Every piece faces a tile of the road loop one ring further in.
        let loop_tiles: Vec<IVec2> = road_loop(c, 2).concat();
        assert!(road_loop(c, 2).iter().all(|chain| chain.windows(2).all(|w| (w[1] - w[0]).abs().element_sum() == 1)));
        for p in &tiles {
            let entrance = *p + match inward(c, 3, *p).quarter_turns() {
                0 => IVec2::Y,
                1 => IVec2::NEG_X,
                2 => IVec2::NEG_Y,
                _ => IVec2::X,
            };
            assert!(loop_tiles.contains(&entrance), "{p} faces {entrance}");
        }
        let middles = side_middles(c, 3);
        assert!(middles.iter().all(|m| tiles.contains(m)));
        assert_eq!(middles.map(|m| side_of(c, 3, m)), [0, 1, 2, 3]);
        assert_eq!(side_of(c, 3, IVec2::new(13, 13)), 2, "corners belong to north and south");
    }
}
