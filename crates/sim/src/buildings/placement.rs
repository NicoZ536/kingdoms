//! Placement validation for the build tool (green/red preview) and for `PlaceBuilding` /
//! `MoveBuilding` commands.
//!
//! General rules (docs/game_design.md §3, §5): the footprint must be inside the map, on dry,
//! buildable land (no water, no swamp) of a single elevation level, inside the own territory,
//! and free of buildings and roads. Vegetation is cleared automatically (+10 % build time per
//! tile, 2 wood per tree). Type-specific rules come from the building's `placement` list and
//! its gathering (farms need free farmland nearby). Research, the `buildable` flag, the maximum
//! count and the cost are checked as well.

use glam::IVec2;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{BuildingId, Rotation};
use crate::defs::{BuildingDef, BuildingRole, BuildingTypeId, Cost, Defs, GameRules, PlacementRule};
use crate::economy::Shortfall;
use crate::grid::TileRect;
use crate::state::GameState;
use crate::territory::Territory;
use crate::world::{DepositKind, Terrain, Vegetation, WorldMap};

/// Farms need at least this share of their field/pasture tiles free nearby, in percent.
pub const MIN_FARMLAND_PERCENT: u32 = 50;

/// Why a single footprint tile cannot be built on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TileBlock {
    /// Outside the map.
    OutOfBounds,
    /// Another building stands there.
    Building,
    /// A road runs there.
    Road,
    /// Water.
    Water,
    /// Swamp.
    Swamp,
    /// Different elevation than the rest of the footprint.
    Uneven,
    /// Not owned by the kingdom.
    OutsideTerritory,
    /// Owned by an AI kingdom.
    ForeignKingdom,
}

impl TileBlock {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            TileBlock::OutOfBounds => "Außerhalb der Karte",
            TileBlock::Building => "Bebaut",
            TileBlock::Road => "Straße",
            TileBlock::Water => "Wasser",
            TileBlock::Swamp => "Sumpf",
            TileBlock::Uneven => "Uneben",
            TileBlock::OutsideTerritory => "Fremdes Gebiet",
            TileBlock::ForeignKingdom => "Anderes Königreich",
        }
    }
}

/// Preview state of one footprint tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TileStatus {
    /// Buildable as is.
    Free,
    /// Buildable; trees, bushes or a boulder will be cleared first.
    NeedsClearing,
    /// Not buildable.
    Blocked(TileBlock),
}

impl TileStatus {
    /// True unless blocked.
    pub fn is_ok(self) -> bool {
        !matches!(self, TileStatus::Blocked(_))
    }
}

/// State of the tile in front of the entrance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntranceStatus {
    /// A road already connects there.
    Road,
    /// Free land: a road can be built there.
    Free,
    /// Water, a building, a boulder, foreign territory, the map edge or a step of more than one
    /// elevation level from the door: no road can serve the building there.
    Blocked,
}

/// Vegetation removed from a footprint.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Clearing {
    /// Trees felled.
    pub trees: u32,
    /// Bushes and tree stumps removed.
    pub bushes: u32,
    /// Boulders removed.
    pub boulders: u32,
}

impl Clearing {
    /// Number of cleared tiles (each adds `clearing_time_percent_per_tile` build time).
    pub fn tiles(&self) -> u32 {
        self.trees + self.bushes + self.boulders
    }

    /// Wood gained (`wood_per_cleared_tree` per tree).
    pub fn wood(&self, rules: &GameRules) -> u32 {
        self.trees * u32::from(rules.wood_per_cleared_tree)
    }

    /// Build-time factor in percent (100 = no surcharge).
    pub fn time_percent(&self, rules: &GameRules) -> u32 {
        100 + u32::from(rules.clearing_time_percent_per_tile) * self.tiles()
    }
}

/// A rule violation that prevents placement. The `Display` text is German.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum PlacementError {
    /// The game is lost: nothing can be built any more.
    #[error("Das Spiel ist verloren.")]
    GameOver,
    /// The building type id does not exist.
    #[error("Unbekannter Gebäudetyp.")]
    UnknownBuildingType,
    /// The type is reached only by upgrading.
    #[error("Das Gebäude „{name}“ entsteht nur durch Ausbau und kann nicht direkt gebaut werden.")]
    NotBuildable {
        /// German building name.
        name: String,
    },
    /// The type needs research first.
    #[error("Das Gebäude „{name}“ ist noch nicht verfügbar – benötigt die Forschung „{tech}“.")]
    NotUnlocked {
        /// German building name.
        name: String,
        /// German technology name.
        tech: String,
    },
    /// The maximum number of buildings of this type exists.
    #[error("Von „{name}“ kann es höchstens {max} geben.")]
    MaxCountReached {
        /// German building name.
        name: String,
        /// Allowed maximum.
        max: u16,
    },
    /// Part of the footprint is outside the map.
    #[error("Das Gebäude ragt über den Kartenrand hinaus.")]
    OutOfBounds,
    /// The footprint overlaps another building.
    #[error("Der Platz ist bereits bebaut.")]
    OverlapsBuilding,
    /// The footprint overlaps a road.
    #[error("Auf dem Bauplatz verläuft eine Straße.")]
    OverlapsRoad,
    /// The footprint covers water.
    #[error("Auf Wasser kann nicht gebaut werden.")]
    Water,
    /// The footprint covers swamp.
    #[error("Im Sumpf kann nicht gebaut werden.")]
    Swamp,
    /// The footprint spans several elevation levels.
    #[error("Der Untergrund ist nicht eben.")]
    NotFlat,
    /// Part of the footprint is outside the own territory.
    #[error("Der Bauplatz liegt außerhalb des eigenen Gebiets.")]
    OutsideTerritory,
    /// Part of the footprint belongs to an AI kingdom.
    #[error("Dieses Land gehört einem anderen Königreich.")]
    ForeignKingdom,
    /// Gates must replace a wall segment.
    #[error("Das Gebäude „{name}“ muss auf einen Mauerabschnitt gesetzt werden.")]
    NotOnWallLine {
        /// German building name.
        name: String,
    },
    /// The required deposit is not in range.
    #[error("Kein Vorkommen „{}“ im Umkreis von {}.", .deposit.display_name(), crate::report::tiles_dative_text(u32::from(*.max_distance)))]
    DepositMissing {
        /// Required deposit.
        deposit: DepositKind,
        /// Maximum distance.
        max_distance: u8,
    },
    /// The building must stand next to water.
    #[error("Das Gebäude muss am Wasser stehen (höchstens {} entfernt).", crate::report::tiles_text(u32::from(*.max_distance)))]
    WaterMissing {
        /// Maximum distance.
        max_distance: u8,
    },
    /// A farm lacks free field or pasture tiles.
    #[error("Zu wenig freies Acker- oder Weideland in der Nähe ({available} von {needed} Feldern).")]
    NotEnoughFarmland {
        /// Required free tiles.
        needed: u32,
        /// Free tiles found.
        available: u32,
    },
    /// The treasury is bankrupt: no new construction.
    #[error("Die Staatskasse ist bankrott – es kann nicht gebaut werden.")]
    Bankrupt,
    /// Not enough gold.
    #[error("Nicht genug Gold ({available} von {needed}).")]
    InsufficientGold {
        /// Required.
        needed: u32,
        /// In the treasury.
        available: i64,
    },
    /// Not enough of a material.
    #[error("Nicht genug {name} ({available} von {needed}).")]
    InsufficientMaterial {
        /// German resource name.
        name: String,
        /// Required.
        needed: u32,
        /// In stock.
        available: u32,
    },
    /// The building would shut a citizen into a pocket of land without any way out (between
    /// buildings and water, docs/game_design.md §5).
    #[error("Das Gebäude würde einen Bewohner einschließen.")]
    TrapsCitizen,
}

/// Most tiles the pocket search of [`traps_a_citizen`] visits: a larger region is no pocket.
const POCKET_LIMIT: usize = 96;

/// Citizens this many tiles around a footprint are checked by [`traps_a_citizen`].
const POCKET_REACH: i32 = 10;

/// True if a citizen standing near `rect` (within [`POCKET_REACH`] tiles) would be shut in by a
/// footprint there: before, its
/// tile reaches a road or a building entrance (or many tiles); with `rect` blocked it reaches
/// neither within [`POCKET_LIMIT`] tiles. Citizens on the footprint itself walk out of it
/// (`population::movement`). Reads the walking grid and the walkers' spatial index.
fn traps_a_citizen(state: &GameState, rect: TileRect, ignore: Option<BuildingId>) -> bool {
    let nav = &state.navigation;
    if !nav.is_built() {
        return false;
    }
    // A pocket holds at most POCKET_LIMIT tiles: its citizens stand within this many tiles.
    let near = rect.expanded(POCKET_REACH);
    let way_out = |p: IVec2| {
        state.roads.get(p).is_some()
            || crate::grid::Direction::ALL.iter().any(|d| {
                state.buildings.at(p + d.offset()).is_some_and(|(id, b)| Some(id) != ignore && b.entrance() == p)
            })
    };
    // Breadth-first over walkable tiles (and not `blocked`): true if the region reaches a way out
    // or is large.
    let open_region = |start: IVec2, blocked: Option<TileRect>| -> bool {
        let mut seen = std::collections::BTreeSet::from([(start.x, start.y)]);
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(p) = queue.pop_front() {
            if way_out(p) || seen.len() > POCKET_LIMIT {
                return true;
            }
            for d in crate::grid::Direction::ALL {
                let q = p + d.offset();
                if nav.step_cost(p, d).is_some() && !blocked.is_some_and(|r| r.contains(q)) && seen.insert((q.x, q.y)) {
                    queue.push_back(q);
                }
            }
        }
        false
    };
    state.population.walking.index.in_rect(near).any(|id| {
        let crate::population::WalkerId::Citizen(c) = id else { return false };
        let Some(m) = state.population.citizen(c).map(|c| c.motion()) else { return false };
        let tile = m.pos().tile();
        m.inside().is_none() && near.contains(tile) && !rect.contains(tile) && nav.is_walkable(tile) && open_region(tile, None) && !open_region(tile, Some(rect))
    })
}

/// Something the player should know, which does not prevent placement. German `Display`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum PlacementWarning {
    /// No road at the entrance yet.
    #[error("Kein Straßenanschluss am Eingang.")]
    NoRoadAccess,
    /// A road can never reach the entrance (rotate the building).
    #[error("Der Eingang ist versperrt.")]
    EntranceBlocked,
    /// Vegetation will be removed.
    #[error("{}", clearing_text(*.trees, *.bushes, *.boulders, *.wood))]
    ClearsVegetation {
        /// Trees.
        trees: u32,
        /// Bushes.
        bushes: u32,
        /// Boulders.
        boulders: u32,
        /// Wood gained.
        wood: u32,
    },
    /// A wall segment is replaced (gates).
    #[error("Der Mauerabschnitt wird ersetzt.")]
    ReplacesWall,
    /// The footprint covers the entrance tile of another building.
    #[error("Das Gebäude versperrt den Eingang eines anderen Gebäudes.")]
    BlocksEntrance,
    /// No storage with carriers reaches the site, so its building material would never arrive
    /// (construction sites are supplied only from storages whose carrier radius covers them).
    #[error("Kein Lager erreicht diesen Bauplatz: Das Baumaterial käme nie an. Baut zuerst ein Lagerhaus in der Nähe.")]
    NoStorageInReach,
}

/// German text of [`PlacementWarning::ClearsVegetation`], e.g. „Bewuchs wird gerodet (1 Baum,
/// 2 Büsche): +3 Holz, längere Bauzeit.“ (singular and plural, counts of 0 left out).
fn clearing_text(trees: u32, bushes: u32, boulders: u32, wood: u32) -> String {
    let counted = |n: u32, one: &str, many: &str| (n > 0).then(|| format!("{n} {}", if n == 1 { one } else { many }));
    let parts: Vec<String> = [
        counted(trees, "Baum", "Bäume"),
        counted(bushes, "Busch", "Büsche"),
        counted(boulders, "Felsbrocken", "Felsbrocken"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let what = if parts.is_empty() { String::new() } else { format!(" ({})", parts.join(", ")) };
    let gain = if wood > 0 { format!("+{wood} Holz, ") } else { String::new() };
    format!("Bewuchs wird gerodet{what}: {gain}längere Bauzeit.")
}

/// Full result of a placement check, enough to draw the preview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacementCheck {
    /// Checked type.
    pub kind: BuildingTypeId,
    /// Requested origin (minimum corner of the rotated footprint).
    pub origin: IVec2,
    /// Requested rotation.
    pub rotation: Rotation,
    /// Tiles the building would occupy.
    pub footprint: TileRect,
    /// Tile in front of the entrance.
    pub entrance: IVec2,
    /// State of the entrance tile.
    pub entrance_status: EntranceStatus,
    /// True if the building can be placed (no errors).
    pub ok: bool,
    /// Violations, in check order (the first one is used as the rejection reason).
    pub errors: Vec<PlacementError>,
    /// Hints that do not prevent placement.
    pub warnings: Vec<PlacementWarning>,
    /// Price (empty for moves).
    pub cost: Cost,
    /// Vegetation that will be cleared.
    pub clearing: Clearing,
    /// Construction work in builder-hours including the clearing surcharge.
    pub build_hours: f32,
    /// Status of every footprint tile in row-major order.
    pub tile_status: Vec<(IVec2, TileStatus)>,
    /// Wall segment replaced by a gate.
    pub replaces: Option<BuildingId>,
}

/// Switches for the shared checker.
#[derive(Clone, Copy, Default)]
pub(crate) struct PlacementOptions<'a> {
    /// Tiles of this building count as free (moves, upgrades).
    pub ignore: Option<BuildingId>,
    /// Do not check gold/materials (moves, scenario start).
    pub skip_cost: bool,
    /// Do not check research, `buildable` and the maximum count (moves, upgrades, start).
    pub skip_unlock: bool,
    /// Do not require territory (placing the first headquarters).
    pub skip_territory: bool,
    /// Territory to use instead of the state's (moves of territory-claiming buildings).
    pub territory: Option<&'a Territory>,
}

/// Checks whether a building of type `kind` can be placed at `origin` with `rotation` now.
pub fn check_placement(
    state: &GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    origin: IVec2,
    rotation: Rotation,
) -> PlacementCheck {
    check_placement_with(state, defs, kind, origin, rotation, PlacementOptions::default())
}

/// Checks whether an existing building can be moved to `origin`/`rotation` (its own tiles count
/// as free; no cost, research or count limits; a moved territory building must stay inside the
/// territory claimed by the others). `None` if the building does not exist.
pub fn check_move(
    state: &GameState,
    defs: &Defs,
    building: BuildingId,
    origin: IVec2,
    rotation: Rotation,
) -> Option<PlacementCheck> {
    let b = state.buildings.get(building)?;
    let claims = defs.buildings().get(b.kind().index()).is_some_and(|d| d.territory_radius > 0);
    let own_territory = claims.then(|| Territory::compute_without_foreign(&state.buildings, defs, Some(building), &state.diplomacy.land));
    let options = PlacementOptions {
        ignore: Some(building),
        skip_cost: true,
        skip_unlock: true,
        skip_territory: false,
        territory: own_territory.as_ref(),
    };
    let mut check = check_placement_with(state, defs, b.kind(), origin, rotation, options);
    check.cost = Cost::default();
    check.build_hours *= super::MOVE_WORK_PERCENT as f32 / 100.0;
    Some(check)
}

/// Most common elevation among the footprint's dry in-map tiles (ties: the lower level).
fn reference_elevation(world: &WorldMap, rect: TileRect) -> Option<u8> {
    let mut counts = [0u32; 256];
    for p in rect.tiles() {
        if let Some(t) = world.get(p)
            && !t.is_water()
        {
            counts[usize::from(t.elevation)] += 1;
        }
    }
    let (level, n) = counts.iter().enumerate().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(&a.0)))?;
    (*n > 0).then_some(level as u8)
}

fn any_tile_near(world: &WorldMap, rect: TileRect, distance: u8, pred: impl Fn(&crate::world::Tile) -> bool) -> bool {
    let area = rect.expanded(i32::from(distance)).clipped(world.width(), world.height());
    area.tiles().any(|p| pred(world.tile(p)))
}

/// Converts a missing part of a cost into a placement error with the German resource name.
pub(crate) fn shortfall_error(defs: &Defs, s: Shortfall) -> PlacementError {
    match s {
        Shortfall::Gold { needed, available } => PlacementError::InsufficientGold { needed, available },
        Shortfall::Material { resource, needed, available } => PlacementError::InsufficientMaterial {
            name: defs.resources().get(resource.index()).map_or_else(|| resource.to_string(), |r| r.name.clone()),
            needed,
            available,
        },
    }
}

fn push_unique(errors: &mut Vec<PlacementError>, e: PlacementError) {
    if !errors.contains(&e) {
        errors.push(e);
    }
}

/// Type-specific rules from the data (placement list and farmland for field/pasture farms).
fn check_type_rules(
    state: &GameState,
    defs: &Defs,
    def: &BuildingDef,
    rect: TileRect,
    territory: &Territory,
    ignore: Option<BuildingId>,
    errors: &mut Vec<PlacementError>,
) {
    let world = &state.world;
    for rule in &def.placement {
        match *rule {
            PlacementRule::NearWater { max_distance } => {
                if !any_tile_near(world, rect, max_distance, |t| t.is_water()) {
                    push_unique(errors, PlacementError::WaterMissing { max_distance });
                }
            }
            PlacementRule::NearDeposit { kind, max_distance, or_terrain } => {
                let found = any_tile_near(world, rect, max_distance, |t| {
                    t.deposit.is_some_and(|d| d.kind == kind && d.amount > 0) || Some(t.terrain) == or_terrain
                });
                if !found {
                    push_unique(errors, PlacementError::DepositMissing { deposit: kind, max_distance });
                }
            }
            PlacementRule::MaxCount(_) | PlacementRule::OnWallLine => {}
        }
    }
    if let Some((kind, tiles, radius)) = crate::agriculture::farm_spec(def) {
        let needed = (u32::from(tiles) * MIN_FARMLAND_PERCENT).div_ceil(100).max(1);
        let available = crate::agriculture::farmland_around(state, defs, rect, kind, radius, ignore, Some(territory)).len() as u32;
        if available < needed {
            push_unique(errors, PlacementError::NotEnoughFarmland { needed, available });
        }
    }
}

/// The shared checker behind [`check_placement`], [`check_move`], upgrades and the scenario start.
pub(crate) fn check_placement_with(
    state: &GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    origin: IVec2,
    rotation: Rotation,
    opts: PlacementOptions<'_>,
) -> PlacementCheck {
    let mut check = PlacementCheck {
        kind,
        origin,
        rotation,
        footprint: TileRect::tile(origin),
        entrance: origin,
        entrance_status: EntranceStatus::Blocked,
        ok: false,
        errors: Vec::new(),
        warnings: Vec::new(),
        cost: Cost::default(),
        clearing: Clearing::default(),
        build_hours: 0.0,
        tile_status: Vec::new(),
        replaces: None,
    };
    if state.progress.game_over().is_some() {
        check.errors.push(PlacementError::GameOver);
    }
    let Some(def) = defs.buildings().get(kind.index()) else {
        check.errors.push(PlacementError::UnknownBuildingType);
        return check;
    };
    let rect = rotation.footprint(origin, def.size);
    check.footprint = rect;
    check.entrance = rotation.entrance(origin, def.size);
    check.cost = def.cost.clone();
    let territory = opts.territory.unwrap_or(&state.territory);
    let world = &state.world;

    if !opts.skip_unlock {
        if !def.buildable {
            check.errors.push(PlacementError::NotBuildable { name: def.name.clone() });
        }
        if let Some(tech) = def.unlocked_by.filter(|t| !state.research.is_researched(*t)) {
            let tech = defs.techs().get(tech.index()).map_or_else(|| tech.to_string(), |t| t.name.clone());
            check.errors.push(PlacementError::NotUnlocked { name: def.name.clone(), tech });
        }
        if let Some(max) = def.max_count() {
            let existing = state.buildings.count_of(kind);
            if existing >= usize::from(max) {
                check.errors.push(PlacementError::MaxCountReached { name: def.name.clone(), max });
            }
        }
    }

    // Per-tile rules.
    let on_wall_line = def.placement.contains(&PlacementRule::OnWallLine);
    let level = reference_elevation(world, rect);
    let mut blocks: Vec<TileBlock> = Vec::new();
    let mut missing_wall = false;
    for p in rect.tiles() {
        let status = match world.get(p) {
            None => TileStatus::Blocked(TileBlock::OutOfBounds),
            Some(tile) => {
                let other = state.buildings.at(p).filter(|(id, _)| Some(*id) != opts.ignore);
                let wall = other.filter(|(_, b)| {
                    defs.buildings().get(b.kind().index()).is_some_and(|d| d.role == BuildingRole::WallSegment)
                });
                if on_wall_line {
                    match wall {
                        Some((id, _)) => check.replaces = Some(id),
                        None => missing_wall = true,
                    }
                }
                if other.is_some() && !(on_wall_line && wall.is_some()) {
                    TileStatus::Blocked(TileBlock::Building)
                } else if state.roads.has_road(p) {
                    TileStatus::Blocked(TileBlock::Road)
                } else if tile.is_water() {
                    TileStatus::Blocked(TileBlock::Water)
                } else if tile.terrain == Terrain::Swamp || !tile.terrain.is_buildable() {
                    TileStatus::Blocked(TileBlock::Swamp)
                } else if Some(tile.elevation) != level {
                    TileStatus::Blocked(TileBlock::Uneven)
                } else if state.diplomacy.land.is_foreign(p) {
                    TileStatus::Blocked(TileBlock::ForeignKingdom)
                } else if !opts.skip_territory && !territory.contains(p) {
                    TileStatus::Blocked(TileBlock::OutsideTerritory)
                } else if tile.vegetation.blocks_building() && !(on_wall_line && wall.is_some()) {
                    match tile.vegetation {
                        Vegetation::Tree { .. } => check.clearing.trees += 1,
                        Vegetation::Bush | Vegetation::Stump { .. } => check.clearing.bushes += 1,
                        Vegetation::Boulder => check.clearing.boulders += 1,
                        Vegetation::None => {}
                    }
                    TileStatus::NeedsClearing
                } else {
                    TileStatus::Free
                }
            }
        };
        if let TileStatus::Blocked(block) = status
            && !blocks.contains(&block)
        {
            blocks.push(block);
        }
        check.tile_status.push((p, status));
    }
    blocks.sort();
    for block in blocks {
        check.errors.push(match block {
            TileBlock::OutOfBounds => PlacementError::OutOfBounds,
            TileBlock::Building => PlacementError::OverlapsBuilding,
            TileBlock::Road => PlacementError::OverlapsRoad,
            TileBlock::Water => PlacementError::Water,
            TileBlock::Swamp => PlacementError::Swamp,
            TileBlock::Uneven => PlacementError::NotFlat,
            TileBlock::OutsideTerritory => PlacementError::OutsideTerritory,
            TileBlock::ForeignKingdom => PlacementError::ForeignKingdom,
        });
    }
    if on_wall_line && missing_wall {
        check.replaces = None;
        check.errors.push(PlacementError::NotOnWallLine { name: def.name.clone() });
    }

    check_type_rules(state, defs, def, rect, territory, opts.ignore, &mut check.errors);
    if check.errors.is_empty() && traps_a_citizen(state, rect, opts.ignore) {
        check.errors.push(PlacementError::TrapsCitizen);
    }

    if !opts.skip_cost {
        if state.economy.is_bankrupt() {
            check.errors.push(PlacementError::Bankrupt);
        }
        for s in state.economy.shortfalls(&def.cost) {
            check.errors.push(shortfall_error(defs, s));
        }
    }

    // Entrance and warnings.
    let entrance = check.entrance;
    // Walking from the door to the entrance tile: at most one level up or down (§5.6, §6).
    let step_ok = |tile: &crate::world::Tile| level.is_some_and(|l| tile.elevation.abs_diff(l) <= 1);
    check.entrance_status = match (world.get(entrance), state.roads.get(entrance)) {
        (Some(tile), Some(road)) if !road.is_bridge() && step_ok(tile) => EntranceStatus::Road,
        (Some(tile), None)
            if step_ok(tile)
                && !tile.is_water()
                && tile.vegetation != Vegetation::Boulder
                && (opts.skip_territory || territory.contains(entrance))
                && state.buildings.id_at(entrance).is_none_or(|id| Some(id) == opts.ignore) =>
        {
            EntranceStatus::Free
        }
        _ => EntranceStatus::Blocked,
    };
    // Wall pieces and towers on a wall line are entered from any side and need no road.
    if def.needs_road() {
        match check.entrance_status {
            EntranceStatus::Road => {}
            EntranceStatus::Free => check.warnings.push(PlacementWarning::NoRoadAccess),
            EntranceStatus::Blocked => check.warnings.push(PlacementWarning::EntranceBlocked),
        }
    }
    let c = check.clearing;
    if c.tiles() > 0 {
        check.warnings.push(PlacementWarning::ClearsVegetation {
            trees: c.trees,
            bushes: c.bushes,
            boulders: c.boulders,
            wood: c.wood(defs.rules()),
        });
    }
    if check.replaces.is_some() {
        check.warnings.push(PlacementWarning::ReplacesWall);
    }
    if blocks_an_entrance(state, rect, opts.ignore.or(check.replaces)) {
        check.warnings.push(PlacementWarning::BlocksEntrance);
    }
    if !opts.skip_cost && !supplied_site(state, defs, &def.cost, rect, opts.ignore) {
        check.warnings.push(PlacementWarning::NoStorageInReach);
    }
    check.build_hours = def.build_hours * c.time_percent(defs.rules()) as f32 / 100.0;
    check.ok = check.errors.is_empty();
    check
}

/// True if the building material of `cost` can reach a site on `rect`: some working storage
/// (other than `ignore`, not being upgraded) that takes every material covers the site with
/// its carrier radius (docs/game_design.md §7 Träger; the road is checked separately). Without
/// any storage (the first headquarters) and without materials there is nothing to warn about.
fn supplied_site(state: &GameState, defs: &Defs, cost: &crate::defs::Cost, rect: TileRect, ignore: Option<BuildingId>) -> bool {
    if cost.materials.is_empty() {
        return true;
    }
    let mut any_storage = false;
    let reaches = state.buildings.iter().filter(|(id, _)| Some(*id) != ignore).any(|(_, b)| {
        let Some(storage) = defs.buildings().get(b.kind().index()).and_then(|d| d.storage.as_ref()) else { return false };
        any_storage = true;
        b.is_operational()
            && !b.is_upgrading()
            && b.rect().distance_to_rect(&rect) <= i32::from(storage.carrier_radius)
            && cost.materials.iter().all(|m| defs.resources().get(m.resource.index()).is_some_and(|r| storage.accepts.accepts(r.category)))
    });
    reaches || !any_storage
}

/// True if a tile of `rect` is the entrance of an adjacent building (other than `ignore`). Only
/// buildings next to the footprint can have their entrance on it, so only neighbours are checked.
fn blocks_an_entrance(state: &GameState, rect: TileRect, ignore: Option<BuildingId>) -> bool {
    rect.tiles().any(|p| {
        crate::grid::Direction::ALL.iter().any(|d| {
            state
                .buildings
                .at(p + d.offset())
                .is_some_and(|(id, b)| Some(id) != ignore && b.entrance() == p)
        })
    })
}

/// Removes all vegetation inside `rect` and reports what was removed.
pub(crate) fn clear_vegetation(world: &mut WorldMap, rect: TileRect) -> Clearing {
    let mut c = Clearing::default();
    for p in rect.tiles() {
        let Some(tile) = world.get(p) else { continue };
        let veg = tile.vegetation;
        match veg {
            Vegetation::None => continue,
            Vegetation::Tree { .. } => c.trees += 1,
            Vegetation::Bush | Vegetation::Stump { .. } => c.bushes += 1,
            Vegetation::Boulder => c.boulders += 1,
        }
        world.tile_mut(p).vegetation = Vegetation::None;
    }
    c
}
