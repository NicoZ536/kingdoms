//! Developer aid `KINGDOMS_DEV_CASE=gasthaus|einarbeitung|lagerferne`: brings a fresh game into
//! a state that normal play rarely shows on screen, so it can be checked in screenshots. Like
//! the other developer plans it only sends regular commands and developer grants:
//!
//! * **gasthaus** – a tavern with beer in its stock and guests in the evening: all technologies
//!   (`DebugUnlockAllResearch`), gold, building material and beer in the headquarters
//!   (`DebugGrant`), a few households for the staff (`DebugSpawnHouseholds`), a tavern near the
//!   headquarters (`PlaceBuildingWithAccess`); carriers fill its stock, and the case stops once
//!   guests walk in between 19 and 21 o'clock.
//! * **einarbeitung** – a workshop whose new, skilled workers are still learning: households of
//!   newcomers (they bring no experience, unlike the founders), two sawmills near the
//!   headquarters; the case stops during the shift of a sawmill whose crew is not yet fully
//!   trained.
//! * **lagerferne** – a construction site no storage reaches: a watchtower at the edge of the
//!   land claims tiles beyond the reach of the headquarters' carriers; the case stops once it
//!   stands and the build tool for a woodcutter's hut rests on such a spot (amber ghost).
//!
//! The case runs after the warp ticks until its state is reached (at most [`MAX_DAYS`] days)
//! and then names where the camera looks, what is selected and – for `lagerferne` – the tool and
//! the cursor tile ([`DevView`]); `KINGDOMS_FOCUS`, `KINGDOMS_SELECT`, `KINGDOMS_TOOL` and
//! `KINGDOMS_CURSOR` take precedence.

use glam::IVec2;
use kingdoms_sim::buildings::{BuildingId, PlacementWarning, Rotation, check_placement};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingTypeId, Defs, ResourceAmount};
use kingdoms_sim::population::{Destination, Purpose};
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::{TICKS_PER_DAY, TICKS_PER_HOUR};

use super::ToolSpec;
use super::dev_settlement::{cheapest_road, headquarters_center, town_spot};

/// Longest a case runs before it gives up (game days).
pub const MAX_DAYS: u64 = 4;
/// Woodcutter's hut: the building whose ghost `lagerferne` shows.
const FAR_SITE_BUILDING: &str = "woodcutter";

/// A state to set up (`KINGDOMS_DEV_CASE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevCase {
    /// A tavern with beer and guests in the evening.
    Tavern,
    /// A workshop whose skilled workers were just hired.
    Training,
    /// A construction site out of every storage's reach.
    FarSite,
}

impl DevCase {
    /// Parses the German (or English) name of a case.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "gasthaus" | "tavern" => Some(Self::Tavern),
            "einarbeitung" | "training" => Some(Self::Training),
            "lagerferne" | "far-site" | "farsite" => Some(Self::FarSite),
            _ => None,
        }
    }

    /// Name in the log.
    pub fn key(self) -> &'static str {
        match self {
            Self::Tavern => "gasthaus",
            Self::Training => "einarbeitung",
            Self::FarSite => "lagerferne",
        }
    }

    /// Buildings the case places near the headquarters, one after the other.
    fn buildings(self) -> &'static [&'static str] {
        match self {
            Self::Tavern => &["tavern"],
            Self::Training => &["sawmill", "sawmill"],
            Self::FarSite => &["watchtower"],
        }
    }
}

/// What the world scene shows once a case is set up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevView {
    /// Tile the camera centres on.
    pub focus: IVec2,
    /// Tile of the building to select.
    pub select: Option<IVec2>,
    /// Tool to activate.
    pub tool: Option<ToolSpec>,
    /// Tile the cursor rests on.
    pub cursor: Option<IVec2>,
}

/// The running case.
#[derive(Debug)]
pub struct DevCasePlan {
    case: DevCase,
    /// Buildings placed so far (type and origin; the id is looked up on the map).
    placed: Vec<(BuildingTypeId, IVec2)>,
    /// The game has no headquarters, road type or building of the case: nothing to place.
    stuck: bool,
    next_tick: u64,
}

impl DevCasePlan {
    /// The plan and its first commands (research, grants, households).
    pub fn start(defs: &Defs, case: DevCase) -> (Self, Vec<Command>) {
        let goods: &[(&str, u32)] = match case {
            DevCase::Tavern => &[("planks", 60), ("stone", 40), ("wood", 40), ("beer", 60), ("bread", 120)],
            DevCase::Training => &[("wood", 120), ("stone", 30), ("tools", 20), ("bread", 160)],
            DevCase::FarSite => &[("planks", 30), ("stone", 30), ("wood", 40)],
        };
        let goods = goods
            .iter()
            .filter_map(|&(key, amount)| defs.resource_id(key).map(|resource| ResourceAmount { resource, amount }))
            .collect();
        // Newcomers bring no experience: they are the skilled workers still learning.
        let households = match case {
            DevCase::Tavern => 3,
            DevCase::Training => 6,
            DevCase::FarSite => 0,
        };
        let mut commands = vec![Command::DebugUnlockAllResearch, Command::DebugGrant { gold: 3000, goods }];
        if households > 0 {
            commands.push(Command::DebugSpawnHouseholds { count: households });
        }
        log::info!("KINGDOMS_DEV_CASE: setting up '{}'", case.key());
        (Self { case, placed: Vec::new(), stuck: false, next_tick: 0 }, commands)
    }

    /// Commands after a tick: places the next building of the case (an hour apart, so the
    /// access road of the one before is taken into account).
    pub fn after_tick(&mut self, state: &GameState, defs: &Defs) -> Vec<Command> {
        let now = state.clock().tick;
        let wanted = self.case.buildings();
        if self.stuck || now < self.next_tick || self.placed.len() >= wanted.len() {
            return Vec::new();
        }
        self.next_tick = now + TICKS_PER_HOUR;
        let key = wanted[self.placed.len()];
        let (Some(center), Some(road), Some(kind)) = (headquarters_center(state, defs), cheapest_road(state, defs), defs.building_id(key)) else {
            log::warn!("KINGDOMS_DEV_CASE: no headquarters, road type or building '{key}'");
            self.stuck = true;
            return Vec::new();
        };
        // The watchtower goes to the edge of the land (the farthest spot the headquarters'
        // carriers still supply); everything else as close as possible, in front of the
        // headquarters (towards the viewer) so that it does not hide the building.
        let spot = match self.case {
            DevCase::FarSite => town_spot(state, defs, kind, center, 12..18, true, road, |o| (o - center).abs().max_element()),
            _ => town_spot(state, defs, kind, center, 4..16, false, road, |o| o.x + o.y),
        };
        match spot {
            Some((origin, rotation)) => {
                log::info!("KINGDOMS_DEV_CASE: {key} at {origin}");
                self.placed.push((kind, origin));
                vec![Command::PlaceBuildingWithAccess { kind, origin, rotation, road }]
            }
            None => {
                log::warn!("KINGDOMS_DEV_CASE: no spot for {key} (next try in an hour)");
                Vec::new()
            }
        }
    }

    /// The placed buildings that stand on the map.
    fn buildings<'a>(&'a self, state: &'a GameState) -> impl Iterator<Item = BuildingId> + 'a {
        self.placed.iter().filter_map(|&(kind, origin)| state.buildings().id_at(origin).filter(|id| state.building(*id).is_some_and(|b| b.kind() == kind)))
    }

    /// The view once the case's state is reached; `None` while it is not.
    pub fn reached(&self, state: &GameState, defs: &Defs) -> Option<DevView> {
        let hour = state.clock().hour();
        let at = |id: BuildingId| state.building(id).map(|b| b.entrance());
        match self.case {
            DevCase::Tavern => self.buildings(state).find_map(|id| {
                let b = state.building(id).filter(|b| b.is_operational())?;
                let def = defs.buildings().get(b.kind().index())?;
                let stocked = def.sells.iter().all(|r| b.inventory().get(*r) > 0);
                let (inside, walking) = guests(state, id);
                (stocked && (19..21).contains(&hour) && walking > 0 && inside + walking >= 3).then(|| {
                    log::info!("KINGDOMS_DEV_CASE: {inside} guests inside, {walking} on the way");
                    let entrance = at(id).unwrap_or_default();
                    DevView { focus: entrance, select: Some(entrance), tool: None, cursor: None }
                })
            }),
            DevCase::Training => self.buildings(state).find_map(|id| {
                let b = state.building(id).filter(|b| b.is_operational())?;
                let def = defs.buildings().get(b.kind().index())?;
                let training = kingdoms_sim::production::crew_training_permille(state, defs, def, id);
                let present = kingdoms_sim::population::present_workers(state, defs, id);
                (!state.population().workers_of(id).is_empty() && training < 1000 && present > 0 && (9..16).contains(&hour)).then(|| {
                    log::info!("KINGDOMS_DEV_CASE: crew trained to {} %", training / 10);
                    let entrance = at(id).unwrap_or_default();
                    DevView { focus: entrance, select: Some(entrance), tool: None, cursor: None }
                })
            }),
            DevCase::FarSite => {
                let tower = self.buildings(state).find(|id| state.building(*id).is_some_and(|b| b.is_complete()))?;
                let kind = defs.building_id(FAR_SITE_BUILDING)?;
                let spot = unsupplied_spot(state, defs, kind, state.building(tower)?.rect().origin)?;
                Some(DevView {
                    focus: spot,
                    select: None,
                    tool: Some(ToolSpec::Build { building: FAR_SITE_BUILDING.to_owned(), rotation: 0 }),
                    cursor: Some(spot),
                })
            }
        }
    }
}

/// Guests of venue `id`: inside and on the way (visible on the map).
fn guests(state: &GameState, id: BuildingId) -> (u32, u32) {
    let mut inside = 0;
    let mut walking = 0;
    for (_, c) in state.population().citizens() {
        let m = c.motion();
        if !matches!(m.destination(), Some(Destination::Building { building, purpose: Purpose::Leisure }) if building == id) {
            continue;
        }
        if m.inside() == Some(id) {
            inside += 1;
        } else if m.is_visible() {
            walking += 1;
        }
    }
    (inside, walking)
}

/// The own tile nearest to `near` where a site of `kind` (unrotated, its origin on the tile)
/// could be placed although no storage reaches it.
fn unsupplied_spot(state: &GameState, defs: &Defs, kind: BuildingTypeId, near: IVec2) -> Option<IVec2> {
    let mut best: Option<(i32, IVec2)> = None;
    for tile in state.territory().owned_positions() {
        let distance = (tile - near).abs().max_element();
        if best.is_some_and(|(d, t)| (d, t.y, t.x) <= (distance, tile.y, tile.x)) {
            continue;
        }
        let check = check_placement(state, defs, kind, tile, Rotation::default());
        if check.ok && check.warnings.contains(&PlacementWarning::NoStorageInReach) {
            best = Some((distance, tile));
        }
    }
    best.map(|(_, tile)| tile)
}

/// Ticks a case may run.
pub const MAX_TICKS: u64 = MAX_DAYS * TICKS_PER_DAY;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_from_the_variable() {
        assert_eq!(DevCase::parse("gasthaus"), Some(DevCase::Tavern));
        assert_eq!(DevCase::parse(" Einarbeitung "), Some(DevCase::Training));
        assert_eq!(DevCase::parse("LAGERFERNE"), Some(DevCase::FarSite));
        assert_eq!(DevCase::parse("training"), Some(DevCase::Training));
        assert_eq!(DevCase::parse("1"), None);
        for case in [DevCase::Tavern, DevCase::Training, DevCase::FarSite] {
            assert_eq!(DevCase::parse(case.key()), Some(case));
        }
    }
}
