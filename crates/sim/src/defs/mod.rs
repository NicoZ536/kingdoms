//! Data-driven game definitions loaded from `data/**.ron`.
//!
//! The files reference each other by canonical string ids (docs/game_design.md). [`Defs::load`]
//! parses every file, resolves the string references into compact typed ids
//! ([`ResourceId`], [`BuildingTypeId`], …) and validates all cross references and value ranges,
//! reporting *every* problem with file, definition and reason in one [`DefsError`].
//!
//! Layout of the data directory:
//!
//! | Path | Content |
//! |---|---|
//! | `resources/*.ron` | `[ResourceDef…]` |
//! | `buildings/*.ron` | `[BuildingDef…]` |
//! | `technologies/*.ron` | `[TechDef…]` (also the source of all unlocks) |
//! | `events/*.ron` | `[EventDef…]` |
//! | `scenarios/<id>.ron` | one `ScenarioDef` per file |
//! | `roads.ron`, `crops.ron`, `units.ron` | lists of road, crop and unit definitions |
//! | `rules.ron` | global balancing constants ([`GameRules`], incl. [`PopulationRules`]) |
//! | `names.ron` | German first and family names of citizens ([`NameLists`]) |
//! | `kingdoms.ron` | AI kingdoms: realms and rulers, economy profile, town plan ([`KingdomData`]) |
//!
//! A [`Defs`] value is immutable after loading and shared by the simulation and the UI.

mod building;
mod climate;
mod common;
mod crop;
mod diplomacy;
mod error;
mod events;
mod ids;
mod loader;
mod military;
mod population;
mod progression;
#[cfg(test)]
pub(crate) use loader::read_dir;
mod resource;
mod road;
mod rules;
mod scenario;
mod tech;
mod unit;
mod walking;

use std::collections::BTreeMap;
use std::path::Path;

pub use building::{
    BuildingCategory, BuildingDef, BuildingMaterial, BuildingRole, Gathering, GoodsRequirement, HousingDef, UpgradeDef,
    HousingRequirements, JobDef, MAX_BUILDING_SIZE, OptionalInput, PlacementRule, ProductionDef, RecipeDef,
    ServiceEffect, StorageDef, StorageFilter,
};
pub use climate::{AgricultureRules, ClimateRules, SeasonGathering, TemperatureRules, WeatherEffect, WeatherEffects};
pub use common::{Cost, JobCategory, JobTier, NeedKind, ResourceAmount, ResourceRate, ServiceKind, SocialClass};
pub use crop::CropDef;
pub use diplomacy::{
    AiRules, CaravanRules, Consumption, DiplomacyRules, KingdomData, LandSource, Personality, PersonalityRules,
    PersonalityValues, ProductionSource, RealmTemplate, RelationRules, TownPlanEntry, TownZone, TreatyRules, WarRules,
    WorldSizeValues,
};
pub use error::{DefsError, DefsProblem};
pub use military::{
    ArmyRules, BanditRules, CampaignRules, ClassBonus, DefenseDef, MilitaryRules, RankDef, RankRule, UnitClass,
};
pub use events::{BuildingFilter, DamageCause, EventChoice, EventCondition, EventDef, EventEffect, EventRisk};
pub use ids::{BuildingTypeId, CropId, EventDefId, ResourceId, RoadTypeId, ScenarioId, TechId, UnitTypeId};
pub use resource::{ResourceCategory, ResourceDef};
pub use road::{BridgeDef, RoadDef};
pub use population::{MortalityStep, NameLists, NeedWeights, PopulationRules, TrainingRules, WealthRules};
pub use progression::{DifficultyValues, EventRules, FestivalRules, ResearchRules};
pub use rules::{GameRules, SeasonValues, Wages, Weather};
pub use scenario::{ScenarioDef, StartBuilding};
pub use tech::{BonusTarget, TechArea, TechDef, TechEffect};
pub use unit::{UnitDef, UnitSupport};
pub use walking::{RoutineHours, WalkingRules};

/// Key of the scenario used for new games unless another one is chosen.
pub const DEFAULT_SCENARIO: &str = "default";

/// One data file in memory: path relative to the data directory (with `/` separators) and text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataFile {
    /// Relative path, e.g. `buildings/buildings.ron`.
    pub path: String,
    /// File content (RON).
    pub text: String,
}

/// All resolved definitions plus string-id lookup tables.
#[derive(Clone, Debug)]
pub struct Defs {
    resources: Vec<ResourceDef>,
    buildings: Vec<BuildingDef>,
    roads: Vec<RoadDef>,
    crops: Vec<CropDef>,
    techs: Vec<TechDef>,
    units: Vec<UnitDef>,
    events: Vec<EventDef>,
    scenarios: Vec<ScenarioDef>,
    rules: GameRules,
    names: NameLists,
    kingdoms: KingdomData,
    ranks: Vec<RankDef>,
    army_mix: Vec<(UnitTypeId, u16)>,
    default_scenario: ScenarioId,
    resource_keys: BTreeMap<String, ResourceId>,
    building_keys: BTreeMap<String, BuildingTypeId>,
    road_keys: BTreeMap<String, RoadTypeId>,
    crop_keys: BTreeMap<String, CropId>,
    tech_keys: BTreeMap<String, TechId>,
    unit_keys: BTreeMap<String, UnitTypeId>,
    event_keys: BTreeMap<String, EventDefId>,
    scenario_keys: BTreeMap<String, ScenarioId>,
    fingerprint: u64,
}

/// Resolved lists handed from the loader to [`Defs::from_parts`].
pub(crate) struct DefsParts {
    pub resources: Vec<ResourceDef>,
    pub buildings: Vec<BuildingDef>,
    pub roads: Vec<RoadDef>,
    pub crops: Vec<CropDef>,
    pub techs: Vec<TechDef>,
    pub units: Vec<UnitDef>,
    pub events: Vec<EventDef>,
    pub scenarios: Vec<ScenarioDef>,
    pub rules: GameRules,
    pub names: NameLists,
    pub kingdoms: KingdomData,
    pub ranks: Vec<RankDef>,
    pub army_mix: Vec<(UnitTypeId, u16)>,
    pub default_scenario: ScenarioId,
}

/// 64-bit FNV-1a, stable across Rust versions and platforms (unlike `DefaultHasher`).
struct Fnv1a(u64);

impl Fnv1a {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn write_list<'a>(&mut self, tag: &str, keys: impl Iterator<Item = &'a str>) {
        self.write(tag.as_bytes());
        self.write(&[0xff]);
        for key in keys {
            self.write(key.as_bytes());
            self.write(&[0]);
        }
    }
}

fn key_map<I: Copy>(items: impl Iterator<Item = (String, I)>) -> BTreeMap<String, I> {
    items.collect()
}

macro_rules! accessors {
    ($(#[$doc:meta])* $list:ident, $one:ident, $by_key:ident, $id_of:ident, $keys:ident, $id:ty, $def:ty) => {
        $(#[$doc])*
        pub fn $list(&self) -> &[$def] {
            &self.$list
        }

        /// Definition for a typed id.
        ///
        /// # Panics
        /// Panics if the id does not belong to this `Defs` (ids from the same `Defs`, or from a
        /// save game whose fingerprint matched, are always valid).
        pub fn $one(&self, id: $id) -> &$def {
            &self.$list[id.index()]
        }

        /// Definition for a canonical string id.
        pub fn $by_key(&self, key: &str) -> Option<&$def> {
            self.$keys.get(key).map(|id| &self.$list[id.index()])
        }

        /// Typed id for a canonical string id.
        pub fn $id_of(&self, key: &str) -> Option<$id> {
            self.$keys.get(key).copied()
        }
    };
}

impl Defs {
    /// Loads and validates all definitions from `data_dir`.
    pub fn load(data_dir: &Path) -> Result<Defs, DefsError> {
        let files = loader::read_dir(data_dir)?;
        let defs = Defs::from_files(&files)?;
        log::info!(
            "game data loaded from {}: {} resources, {} buildings, {} roads, {} crops, {} technologies, {} units, {} events, {} scenarios",
            data_dir.display(),
            defs.resources.len(),
            defs.buildings.len(),
            defs.roads.len(),
            defs.crops.len(),
            defs.techs.len(),
            defs.units.len(),
            defs.events.len(),
            defs.scenarios.len()
        );
        Ok(defs)
    }

    /// Loads and validates definitions from in-memory files (tests, embedded data, mods).
    pub fn from_files(files: &[DataFile]) -> Result<Defs, DefsError> {
        let raw = loader::RawData::parse(files)?;
        loader::resolve(&raw)
    }

    pub(crate) fn from_parts(parts: DefsParts) -> Defs {
        let mut fp = Fnv1a::new();
        fp.write_list("resources", parts.resources.iter().map(|d| d.key.as_str()));
        fp.write_list("buildings", parts.buildings.iter().map(|d| d.key.as_str()));
        fp.write_list("roads", parts.roads.iter().map(|d| d.key.as_str()));
        fp.write_list("crops", parts.crops.iter().map(|d| d.key.as_str()));
        fp.write_list("techs", parts.techs.iter().map(|d| d.key.as_str()));
        fp.write_list("units", parts.units.iter().map(|d| d.key.as_str()));
        fp.write_list("events", parts.events.iter().map(|d| d.key.as_str()));
        fp.write_list("scenarios", parts.scenarios.iter().map(|d| d.key.as_str()));
        fp.write_list("ranks", parts.ranks.iter().map(|d| d.key.as_str()));
        // The data shape saved states depend on structurally: footprints (occupancy grid,
        // entrances), production slots and recipe indices, storages, housing and bridge road
        // types. Changing it invalidates saves, so it is part of the fingerprint.
        fp.write(b"shape\xff");
        for b in &parts.buildings {
            fp.write(&b.size.x.to_le_bytes());
            fp.write(&b.size.y.to_le_bytes());
            let recipes = b.production.as_ref().map_or(u32::MAX, |p| u32::try_from(p.recipes.len()).unwrap_or(u32::MAX - 1));
            fp.write(&recipes.to_le_bytes());
            fp.write(&[u8::from(b.storage.is_some()), u8::from(b.housing.is_some())]);
        }
        for r in &parts.roads {
            fp.write(&[u8::from(r.bridge.is_some())]);
        }
        Defs {
            resource_keys: key_map(parts.resources.iter().map(|d| (d.key.clone(), d.id))),
            building_keys: key_map(parts.buildings.iter().map(|d| (d.key.clone(), d.id))),
            road_keys: key_map(parts.roads.iter().map(|d| (d.key.clone(), d.id))),
            crop_keys: key_map(parts.crops.iter().map(|d| (d.key.clone(), d.id))),
            tech_keys: key_map(parts.techs.iter().map(|d| (d.key.clone(), d.id))),
            unit_keys: key_map(parts.units.iter().map(|d| (d.key.clone(), d.id))),
            event_keys: key_map(parts.events.iter().map(|d| (d.key.clone(), d.id))),
            scenario_keys: key_map(parts.scenarios.iter().map(|d| (d.key.clone(), d.id))),
            resources: parts.resources,
            buildings: parts.buildings,
            roads: parts.roads,
            crops: parts.crops,
            techs: parts.techs,
            units: parts.units,
            events: parts.events,
            scenarios: parts.scenarios,
            ranks: parts.ranks,
            army_mix: parts.army_mix,
            rules: parts.rules,
            names: parts.names,
            kingdoms: parts.kingdoms,
            default_scenario: parts.default_scenario,
            fingerprint: fp.0,
        }
    }

    accessors!(
        /// All resources in id order.
        resources, resource, resource_by_key, resource_id, resource_keys, ResourceId, ResourceDef
    );
    accessors!(
        /// All building types in id order.
        buildings, building, building_by_key, building_id, building_keys, BuildingTypeId, BuildingDef
    );
    accessors!(
        /// All road types in id order.
        roads, road, road_by_key, road_id, road_keys, RoadTypeId, RoadDef
    );
    accessors!(
        /// All crops in id order.
        crops, crop, crop_by_key, crop_id, crop_keys, CropId, CropDef
    );
    accessors!(
        /// All technologies in id order.
        techs, tech, tech_by_key, tech_id, tech_keys, TechId, TechDef
    );
    accessors!(
        /// All unit types in id order.
        units, unit, unit_by_key, unit_id, unit_keys, UnitTypeId, UnitDef
    );
    accessors!(
        /// All events in id order.
        events, event, event_by_key, event_id, event_keys, EventDefId, EventDef
    );
    accessors!(
        /// All scenarios in id order.
        scenarios, scenario, scenario_by_key, scenario_id, scenario_keys, ScenarioId, ScenarioDef
    );

    /// Global balancing rules.
    pub fn rules(&self) -> &GameRules {
        &self.rules
    }

    /// German citizen names.
    pub fn names(&self) -> &NameLists {
        &self.names
    }

    /// AI kingdom data: realms, economy profile and town plan (`kingdoms.ron`).
    pub fn kingdom_data(&self) -> &KingdomData {
        &self.kingdoms
    }

    /// Ruler ranks in order (§1); the first one is the start (guaranteed to exist).
    pub fn ranks(&self) -> &[RankDef] {
        &self.ranks
    }

    /// Unit types of AI armies with their weights (`rules.ron` → `military.armies.mix`).
    pub fn army_mix(&self) -> &[(UnitTypeId, u16)] {
        &self.army_mix
    }

    /// The scenario with key [`DEFAULT_SCENARIO`] (guaranteed to exist).
    pub fn default_scenario(&self) -> ScenarioId {
        self.default_scenario
    }

    /// Stable hash of all canonical ids in id order plus the data shape a saved state depends
    /// on structurally (building sizes, production presence and recipe counts, storage and
    /// housing presence, bridge road types). Ids and structures stored in a save game are only
    /// meaningful for data with the same fingerprint. Other balancing values (costs, rates,
    /// capacities, recipe contents) may change: loading re-applies the values derived from
    /// them to the state (`save::decode`).
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Building types available without research.
    pub fn start_buildings(&self) -> impl Iterator<Item = &BuildingDef> + '_ {
        self.buildings.iter().filter(|b| b.unlocked_by.is_none() && b.buildable)
    }

    /// Building types of a category in id order.
    pub fn buildings_in(&self, category: BuildingCategory) -> impl Iterator<Item = &BuildingDef> + '_ {
        self.buildings.iter().filter(move |b| b.category == category)
    }

    /// Technologies of an area in id order.
    pub fn techs_in(&self, area: TechArea) -> impl Iterator<Item = &TechDef> + '_ {
        self.techs.iter().filter(move |t| t.area == area)
    }
}

#[cfg(test)]
mod tests;
