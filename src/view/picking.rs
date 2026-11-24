//! What lies under the cursor, and what the player selected.

use glam::{IVec2, Vec2};
use kingdoms_engine::rendering::iso;
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::population::CitizenId;
use kingdoms_sim::state::GameState;
use kingdoms_sim::world::{MAX_ELEVATION, WorldMap};

/// The player's current selection (read by the info panel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    /// A building or construction site.
    Building(BuildingId),
    /// A map tile without a building.
    Tile(IVec2),
}

/// What the cursor points at this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hover {
    /// Ground tile under the cursor.
    pub tile: Option<IVec2>,
    /// Building under the cursor (footprint or sprite).
    pub building: Option<BuildingId>,
    /// Citizen under the cursor (in front of any building there).
    pub citizen: Option<CitizenId>,
}

impl Hover {
    /// The selection a click here makes.
    pub fn selection(&self) -> Option<Selection> {
        match (self.building, self.tile) {
            (Some(b), _) => Some(Selection::Building(b)),
            (None, Some(t)) => Some(Selection::Tile(t)),
            (None, None) => None,
        }
    }
}

/// Finds the tile whose top face is under the world position, taking elevation into account.
/// Higher levels are tested first because, at the same screen point, they are nearer the viewer.
pub fn pick_tile(map: &WorldMap, world: Vec2) -> Option<IVec2> {
    for e in (0..=MAX_ELEVATION).rev() {
        let t = iso::world_to_tile_elevated(world, f32::from(e)).floor().as_ivec2();
        if let Some(tile) = map.get(t)
            && tile.elevation == e
        {
            return Some(t);
        }
    }
    None
}

/// The building on tile `p` or, if none stands there, the one whose entrance is `p` (the tile
/// the logs and the developer tools name for a building).
pub fn building_at_or_entrance(state: &GameState, p: IVec2) -> Option<BuildingId> {
    state.buildings().id_at(p).or_else(|| state.buildings().iter().find(|(_, b)| b.entrance() == p).map(|(id, _)| id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_sim::world::{Biome, Terrain, Tile, Vegetation};

    #[test]
    fn a_building_is_found_by_its_footprint_and_its_entrance() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match kingdoms_sim::defs::Defs::load(&dir) {
            Ok(d) => d,
            Err(e) => panic!("shipped game data must load: {e}"),
        };
        let state = GameState::new(&defs, kingdoms_sim::state::GameSettings::new(&defs, 3, kingdoms_sim::world::WorldSize::Small));
        let Some((id, hq)) = state.buildings().iter().next() else { panic!("the start has a town hall") };
        assert_eq!(building_at_or_entrance(&state, hq.rect().origin), Some(id));
        assert!(!hq.rect().contains(hq.entrance()), "the entrance lies outside the footprint");
        assert_eq!(building_at_or_entrance(&state, hq.entrance()), Some(id));
        assert_eq!(building_at_or_entrance(&state, hq.rect().origin - IVec2::splat(20)), None);
    }

    fn flat_map(elevation: u8) -> WorldMap {
        let tile = Tile {
            terrain: Terrain::Grass,
            biome: Biome::Grassland,
            elevation,
            fertility: 0,
            moisture: 0,
            temperature: 0,
            vegetation: Vegetation::None,
            deposit: None,
            river: false,
        };
        WorldMap::new_filled(8, 8, 1, tile)
    }

    #[test]
    fn picks_tiles_on_elevated_ground() {
        for e in [0u8, 3] {
            let map = flat_map(e);
            let p = IVec2::new(4, 2);
            let w = iso::tile_center_world(p, f32::from(e));
            assert_eq!(pick_tile(&map, w), Some(p));
        }
        assert_eq!(pick_tile(&flat_map(0), Vec2::new(0.0, -50.0)), None);
    }

    #[test]
    fn hover_prefers_the_building() {
        let tile = IVec2::new(1, 2);
        assert_eq!(Hover { tile: Some(tile), ..Hover::default() }.selection(), Some(Selection::Tile(tile)));
        assert_eq!(Hover::default().selection(), None);
    }
}
