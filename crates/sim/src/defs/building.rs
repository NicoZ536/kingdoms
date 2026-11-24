//! Building definitions (`data/buildings/*.ron`, docs/game_design.md §5.1–5.5).

use std::collections::{BTreeMap, BTreeSet};

use glam::IVec2;
use serde::{Deserialize, Serialize};

use super::common::{
    Cost, JobCategory, JobTier, NeedKind, RawCost, ResourceRate, ServiceKind, SocialClass, resolve_rates,
};
use super::error::Loc;
use super::ids::{BuildingTypeId, CropId, ResourceId, TechId, UnitTypeId};
use super::loader::Ctx;
use super::military::DefenseDef;
use super::resource::ResourceCategory;
use crate::world::{DepositKind, Terrain};

/// Largest allowed footprint edge in tiles.
pub const MAX_BUILDING_SIZE: u8 = 8;

/// Build-menu category of a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BuildingCategory {
    /// Homes (§5.1).
    Housing,
    /// Workshops, gatherers, mines (§5.2).
    Production,
    /// Farms, fishery, apiary (§5.3).
    Agriculture,
    /// Public services and storage (§5.4).
    Public,
    /// Military (§5.5).
    Military,
}

impl BuildingCategory {
    /// All categories in build-menu order.
    pub const ALL: [BuildingCategory; 5] = [
        BuildingCategory::Housing,
        BuildingCategory::Production,
        BuildingCategory::Agriculture,
        BuildingCategory::Public,
        BuildingCategory::Military,
    ];

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            BuildingCategory::Housing => "Wohnen",
            BuildingCategory::Production => "Produktion",
            BuildingCategory::Agriculture => "Landwirtschaft",
            BuildingCategory::Public => "Öffentlich",
            BuildingCategory::Military => "Militär",
        }
    }
}

/// Main construction material; wooden buildings can catch fire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildingMaterial {
    /// Timber construction (flammable).
    #[default]
    Wood,
    /// Stone/brick construction.
    Stone,
}

/// Special function the simulation looks for (instead of hard-coding building ids).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildingRole {
    /// No special function.
    #[default]
    General,
    /// Seat of government (town hall, castle): taxes, territory, defeat when lost.
    Headquarters,
    /// Enables long-distance trade with caravans (trading post).
    TradeHub,
    /// Wall segment, drawn like a road and auto-connected.
    WallSegment,
    /// Gate in a wall line; passable for own people.
    Gate,
}

/// How a building gathers raw resources from its surroundings. Radii are measured in tiles
/// (Chebyshev distance) from the building's footprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gathering {
    /// Fells mature trees within `radius` (one tree per work cycle).
    Trees {
        /// Search radius.
        radius: u8,
    },
    /// Plants saplings within `radius` (one per work cycle).
    PlantTrees {
        /// Planting radius.
        radius: u8,
    },
    /// Extracts a deposit within `radius`; each produced unit depletes the deposit.
    Deposit {
        /// Deposit kind.
        kind: DepositKind,
        /// Search radius (1 = under or adjacent to the building).
        radius: u8,
        /// Terrain that can be worked as an inexhaustible source as well (sand pit: beaches).
        or_terrain: Option<Terrain>,
    },
    /// Cultivates fields around the farm; yields come from the crop definition.
    Fields {
        /// Crops the farm can grow (the player picks one).
        crops: Vec<CropId>,
        /// Maximum number of field tiles.
        max_tiles: u16,
        /// Maximum distance of field tiles.
        radius: u8,
    },
    /// Keeps animals on pasture tiles around the farm.
    Pasture {
        /// Pasture tiles needed for full productivity.
        tiles: u16,
        /// Maximum distance of pasture tiles.
        radius: u8,
    },
    /// Bees: productivity bonus from meadows and orchards nearby.
    Bees {
        /// Radius counted for the bonus.
        radius: u8,
        /// Bonus with the radius fully covered by meadow/orchard tiles.
        max_bonus_percent: u16,
    },
}

/// Optional recipe input that raises the output when available.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptionalInput {
    /// The resource.
    pub resource: ResourceId,
    /// Units consumed per cycle when available.
    pub amount: f32,
    /// Output increase when consumed.
    pub output_bonus_percent: u16,
}

/// One work cycle of a production building.
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeDef {
    /// Recipe key, unique within the building.
    pub key: String,
    /// German display name.
    pub name: String,
    /// Consumed per cycle (fractions accumulate, e.g. 0.25 tools).
    pub inputs: Vec<ResourceRate>,
    /// Consumed if available, for a bonus.
    pub optional_inputs: Vec<OptionalInput>,
    /// Produced per cycle (may be empty only for tree planting).
    pub outputs: Vec<ResourceRate>,
    /// Duration of one cycle in game hours.
    pub hours: f32,
}

/// Production behaviour of a building.
#[derive(Clone, Debug, PartialEq)]
pub struct ProductionDef {
    /// Gathering from the surroundings, if any.
    pub gathering: Option<Gathering>,
    /// Selectable recipes (empty for field farms: yields come from crops).
    pub recipes: Vec<RecipeDef>,
    /// Consumes tools (1 per [`GameRules::tool_wear_cycles`](super::GameRules) cycles).
    pub uses_tools: bool,
    /// Capacity of each input/output buffer.
    pub buffer_capacity: u16,
}

/// Conditions for a housing tier (cumulative: they include the lower tiers' conditions).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HousingRequirements {
    /// Needs that must be satisfied.
    pub needs: Vec<NeedKind>,
    /// Services that must reach the house.
    pub services: Vec<ServiceKind>,
    /// Goods the household must be supplied with; each entry is satisfied by any of its resources.
    pub goods: Vec<GoodsRequirement>,
}

impl HousingRequirements {
    /// True if `self` contains every condition of `other`.
    pub fn includes(&self, other: &HousingRequirements) -> bool {
        other.needs.iter().all(|n| self.needs.contains(n))
            && other.services.iter().all(|s| self.services.contains(s))
            && other.goods.iter().all(|g| self.goods.contains(g))
    }
}

/// A supply requirement satisfied by any one of several resources (e.g. silk *or* spices).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoodsRequirement {
    /// Accepted resources (sorted by id).
    pub any_of: Vec<ResourceId>,
}

/// Housing properties.
#[derive(Clone, Debug, PartialEq)]
pub struct HousingDef {
    /// Residents.
    pub capacity: u16,
    /// Tier, 1 = simplest.
    pub tier: u8,
    /// Social class of the residents.
    pub class: SocialClass,
    /// Tax per resident and day at a 100 % tax rate (actual tax = this × tax rate).
    pub tax_per_resident: f32,
    /// Earnings per working member and day of a household living here, in gold (household
    /// savings, docs/game_design.md §6).
    pub income_per_worker: f32,
    /// Conditions to reach and keep this tier.
    pub requirements: HousingRequirements,
}

/// Upgrade of a building to its next level (docs/game_design.md §5): the building keeps its
/// id, workers, stock, production buffers, recipes and orders and becomes `to`.
#[derive(Clone, Debug, PartialEq)]
pub struct UpgradeDef {
    /// Building type after the upgrade.
    pub to: BuildingTypeId,
    /// Gold and materials (data default: the target type's construction cost).
    pub cost: Cost,
    /// Construction work in game hours for one builder (data default: the target type's
    /// `build_hours`).
    pub hours: f32,
}

/// Area service of a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceEffect {
    /// Service kind.
    pub kind: ServiceKind,
    /// Radius in tiles.
    pub radius: u16,
}

/// Which resources a storage building accepts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageFilter {
    /// Every resource.
    All,
    /// Only these categories.
    Only(Vec<ResourceCategory>),
}

impl StorageFilter {
    /// True if resources of `category` may be stored.
    pub fn accepts(&self, category: ResourceCategory) -> bool {
        match self {
            StorageFilter::All => true,
            StorageFilter::Only(list) => list.contains(&category),
        }
    }
}

/// Storage capacity of a building.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageDef {
    /// Units per resource type.
    pub capacity_per_resource: u32,
    /// Accepted resources.
    pub accepts: StorageFilter,
    /// Radius served by the building's carriers.
    pub carrier_radius: u16,
}

/// Placement restriction checked in addition to the general rules (flat, dry, free land).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PlacementRule {
    /// At most this many buildings of the type may exist (incl. construction sites).
    MaxCount(u16),
    /// A water tile must be within `max_distance` of the footprint.
    NearWater {
        /// Maximum distance in tiles.
        max_distance: u8,
    },
    /// A deposit of `kind` (or a tile of `or_terrain`) must be within `max_distance` of the
    /// footprint (0 = under it).
    NearDeposit {
        /// Required deposit.
        kind: DepositKind,
        /// Maximum distance in tiles.
        max_distance: u8,
        /// Terrain accepted instead of a deposit.
        #[serde(default)]
        or_terrain: Option<Terrain>,
    },
    /// Must be placed onto an existing wall segment.
    OnWallLine,
}

/// Job offered by a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobDef {
    /// Priority category.
    pub category: JobCategory,
    /// Qualification and wage level.
    pub tier: JobTier,
}

/// A building type.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingDef {
    /// Typed id.
    pub id: BuildingTypeId,
    /// Canonical string id; also the sprite name in the `buildings` atlas.
    pub key: String,
    /// German display name.
    pub name: String,
    /// German plural of the name („Wachtürme“), given in the data for the buildings texts count
    /// (those that shoot, [`DefenseDef::ranged`]); `None` for the others.
    pub plural: Option<String>,
    /// German description for tooltips.
    pub description: String,
    /// Build-menu category.
    pub category: BuildingCategory,
    /// Footprint in tiles (x = width along +x, y = depth along +y).
    pub size: IVec2,
    /// Construction cost.
    pub cost: Cost,
    /// Construction work in game hours for one builder.
    pub build_hours: f32,
    /// Worker slots.
    pub workers: u8,
    /// Job description (present iff `workers > 0`).
    pub job: Option<JobDef>,
    /// Upkeep in gold per day.
    pub upkeep_per_day: u32,
    /// Can be placed by the player (false for housing tiers reached only by upgrading).
    pub buildable: bool,
    /// Construction material.
    pub material: BuildingMaterial,
    /// Work happens outdoors (fields, pastures, forests, open pits, fishing): weather changes
    /// the work speed (docs/game_design.md §8.2).
    pub outdoor: bool,
    /// Special function.
    pub role: BuildingRole,
    /// Production behaviour.
    pub production: Option<ProductionDef>,
    /// Housing behaviour.
    pub housing: Option<HousingDef>,
    /// Next level (houses: the next tier), if any.
    pub upgrade: Option<UpgradeDef>,
    /// Area services.
    pub services: Vec<ServiceEffect>,
    /// Storage.
    pub storage: Option<StorageDef>,
    /// Resources sold directly to households in the service radius (tavern: beer).
    pub sells: Vec<ResourceId>,
    /// Radius of claimed territory (0 = none).
    pub territory_radius: u16,
    /// Soldiers that can be stationed inside.
    pub garrison: u8,
    /// Defence values: ranged shots and the structure's hit points (docs/game_design.md §11).
    pub defense: Option<DefenseDef>,
    /// Research points per game hour at full staff (schools, docs/game_design.md §9): scaled by
    /// the staff at work like production; 0 for buildings that do not research.
    pub research_points_per_hour: f32,
    /// Extra placement rules.
    pub placement: Vec<PlacementRule>,
    /// Technology unlocking the type; `None` = available from the start. Derived from the
    /// technologies' `UnlockBuilding` effects (single source of truth).
    pub unlocked_by: Option<TechId>,
    /// Unit types trained here. Derived from the units' `trained_at`; an upgraded type trains
    /// everything the types it is built from trained.
    pub trains: Vec<UnitTypeId>,
    /// The type itself and the types it is reached from by upgrades, nearest first (derived):
    /// research bonuses for a type also apply to its upgrades.
    pub lineage: Vec<BuildingTypeId>,
}

impl BuildingDef {
    /// `n` buildings of this type in German: „1 Wachturm“, „3 Wachtürme“ (types without a
    /// plural in the data keep their name).
    pub fn count_text(&self, n: u32) -> String {
        if n == 1 { format!("1 {}", self.name) } else { format!("{n} {}", self.plural.as_deref().unwrap_or(&self.name)) }
    }

    /// True if this type is `kind` or an upgrade of it (see [`BuildingDef::lineage`]).
    pub fn descends_from(&self, kind: BuildingTypeId) -> bool {
        self.id == kind || self.lineage.contains(&kind)
    }

    /// [`BuildingDef::research_points_per_hour`] in milli-points (integer, for the state).
    pub fn research_milli_per_hour(&self) -> u64 {
        (f64::from(self.research_points_per_hour) * 1000.0).round().max(0.0) as u64
    }

    /// Maximum count from [`PlacementRule::MaxCount`], if limited.
    pub fn max_count(&self) -> Option<u16> {
        self.placement.iter().find_map(|r| match r {
            PlacementRule::MaxCount(n) => Some(*n),
            _ => None,
        })
    }

    /// Radius of a service provided by this building.
    pub fn service_radius(&self, kind: ServiceKind) -> Option<u16> {
        self.services.iter().find(|s| s.kind == kind).map(|s| s.radius)
    }

    /// Recipe by key.
    pub fn recipe(&self, key: &str) -> Option<&RecipeDef> {
        self.production.as_ref()?.recipes.iter().find(|r| r.key == key)
    }

    /// Sprite name of the finished building.
    pub fn sprite(&self) -> String {
        format!("buildings/{}", self.key)
    }

    /// Sprite name of the build-menu icon.
    pub fn icon_sprite(&self) -> String {
        format!("icons/bld_{}", self.key)
    }
}

// ------------------------------------------------------------------------------------------------
// File form
// ------------------------------------------------------------------------------------------------

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawBuilding {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub plural: Option<String>,
    #[serde(default)]
    pub description: String,
    pub category: BuildingCategory,
    pub size: (u8, u8),
    pub cost: RawCost,
    pub build_hours: f32,
    #[serde(default)]
    pub workers: u8,
    #[serde(default)]
    pub job: Option<JobDef>,
    #[serde(default)]
    pub upkeep: u32,
    #[serde(default = "default_true")]
    pub buildable: bool,
    #[serde(default)]
    pub material: BuildingMaterial,
    #[serde(default)]
    pub outdoor: bool,
    #[serde(default)]
    pub role: BuildingRole,
    #[serde(default)]
    pub production: Option<RawProduction>,
    #[serde(default)]
    pub housing: Option<RawHousing>,
    #[serde(default)]
    pub upgrade: Option<RawUpgrade>,
    #[serde(default)]
    pub services: Vec<ServiceEffect>,
    #[serde(default)]
    pub storage: Option<StorageDef>,
    #[serde(default)]
    pub sells: Vec<String>,
    #[serde(default)]
    pub territory_radius: u16,
    #[serde(default)]
    pub garrison: u8,
    #[serde(default)]
    pub defense: Option<DefenseDef>,
    #[serde(default)]
    pub research_points_per_hour: f32,
    #[serde(default)]
    pub placement: Vec<PlacementRule>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawProduction {
    #[serde(default)]
    pub gathering: Option<RawGathering>,
    #[serde(default)]
    pub recipes: Vec<RawRecipe>,
    #[serde(default)]
    pub uses_tools: bool,
    #[serde(default)]
    pub buffer: Option<u16>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum RawGathering {
    Trees {
        radius: u8,
    },
    PlantTrees {
        radius: u8,
    },
    Deposit {
        kind: DepositKind,
        radius: u8,
        #[serde(default)]
        or_terrain: Option<Terrain>,
    },
    Fields {
        crops: Vec<String>,
        max_tiles: u16,
        radius: u8,
    },
    Pasture {
        tiles: u16,
        radius: u8,
    },
    Bees {
        radius: u8,
        max_bonus_percent: u16,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRecipe {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, f32>,
    #[serde(default)]
    pub optional_inputs: Vec<RawOptionalInput>,
    #[serde(default)]
    pub outputs: BTreeMap<String, f32>,
    pub hours: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawOptionalInput {
    pub resource: String,
    pub amount: f32,
    pub output_bonus_percent: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawHousing {
    pub capacity: u16,
    pub tier: u8,
    pub class: SocialClass,
    pub tax_per_resident: f32,
    pub income_per_worker: f32,
    #[serde(default)]
    pub requires: RawHousingRequirements,
}

/// File form of [`UpgradeDef`]: `upgrade: (to: "warehouse", cost: (…), hours: 6.0)`; cost and
/// hours default to the target type's construction cost and build hours.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawUpgrade {
    pub to: String,
    #[serde(default)]
    pub cost: Option<RawCost>,
    #[serde(default)]
    pub hours: Option<f32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawHousingRequirements {
    #[serde(default)]
    pub needs: Vec<NeedKind>,
    #[serde(default)]
    pub services: Vec<ServiceKind>,
    /// Each inner list is satisfied by any one of its resources.
    #[serde(default)]
    pub goods: Vec<Vec<String>>,
}

impl RawBuilding {
    pub fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, id: BuildingTypeId, default_buffer: u16) -> BuildingDef {
        let (w, d) = self.size;
        {
            let p = &mut ctx.problems;
            p.check(!self.name.trim().is_empty(), loc, || "name must not be empty".into());
            p.check((1..=MAX_BUILDING_SIZE).contains(&w) && (1..=MAX_BUILDING_SIZE).contains(&d), loc, || {
                format!("size {w}x{d} outside 1..={MAX_BUILDING_SIZE}")
            });
            p.check(self.build_hours.is_finite() && self.build_hours > 0.0, loc, || {
                "build_hours must be positive".into()
            });
            p.check((self.workers > 0) == self.job.is_some(), loc, || {
                "a job (category/tier) is required exactly when workers > 0".into()
            });
            p.check((self.category == BuildingCategory::Housing) == self.housing.is_some(), loc, || {
                "housing data is required exactly for category Housing".into()
            });
            if matches!(self.role, BuildingRole::WallSegment | BuildingRole::Gate) {
                p.check(self.size == (1, 1), loc, || "wall segments and gates must be 1x1".into());
            }
            if self.role == BuildingRole::Headquarters {
                p.check(self.storage.is_some(), loc, || "headquarters must have storage".into());
            }
            if let Some(d) = &self.defense {
                p.check(d.hit_points > 0 && (d.ranged > 0) == (d.range > 0) && d.range <= 16, loc, || {
                    "defense needs hit_points > 0 and a range of 1..=16 exactly when it shoots".into()
                });
                // The raid warning counts the defences by type (docs/game_design.md §11).
                p.check(d.ranged == 0 || self.plural.is_some(), loc, || "a building that shoots needs a plural".into());
            }
            p.check(self.plural.as_ref().is_none_or(|n| !n.trim().is_empty()), loc, || "plural must not be empty".into());
            let mut kinds = BTreeSet::new();
            for s in &self.services {
                p.check(s.radius > 0, loc, || format!("service {:?}: radius must be positive", s.kind));
                p.check(kinds.insert(s.kind), loc, || format!("service {:?} listed twice", s.kind));
            }
            if let Some(storage) = &self.storage {
                p.check(storage.capacity_per_resource > 0, loc, || "storage capacity must be positive".into());
                if let StorageFilter::Only(list) = &storage.accepts {
                    p.check(!list.is_empty(), loc, || "storage filter must list at least one category".into());
                }
            }
            let max_counts = self.placement.iter().filter(|r| matches!(r, PlacementRule::MaxCount(_))).count();
            p.check(max_counts <= 1, loc, || "MaxCount given more than once".into());
            p.check(!self.placement.contains(&PlacementRule::MaxCount(0)), loc, || {
                "MaxCount must be at least 1".into()
            });
        }

        ctx.problems.check(
            self.research_points_per_hour.is_finite()
                && self.research_points_per_hour >= 0.0
                && (self.research_points_per_hour == 0.0 || self.workers > 0),
            loc,
            || "research_points_per_hour must be >= 0 and needs workers".into(),
        );
        let cost = self.cost.resolve(ctx, loc, "cost");
        let sells = self.sells.iter().filter_map(|k| ctx.resource(loc, k, "sells")).collect();
        let production = self.production.as_ref().map(|p| p.resolve(ctx, loc, self.workers, default_buffer));
        let housing = self.housing.as_ref().map(|h| h.resolve(ctx, loc));
        // Defaults (the target's cost and build hours) are filled in by the cross check.
        let upgrade = self.upgrade.as_ref().and_then(|u| {
            if let Some(h) = u.hours {
                ctx.problems.check(h.is_finite() && h > 0.0, loc, || "upgrade hours must be positive".into());
            }
            let cost = u.cost.as_ref().map(|c| c.resolve(ctx, loc, "upgrade cost")).unwrap_or_default();
            let to = ctx.building(loc, &u.to, "upgrade.to")?;
            Some(UpgradeDef { to, cost, hours: u.hours.unwrap_or(0.0) })
        });
        if housing.is_some() {
            ctx.problems.check(self.workers == 0 && self.production.is_none(), loc, || {
                "housing cannot have workers or production".into()
            });
        }

        BuildingDef {
            id,
            key: self.id.clone(),
            name: self.name.clone(),
            plural: self.plural.clone(),
            description: self.description.clone(),
            category: self.category,
            size: IVec2::new(i32::from(w), i32::from(d)),
            cost,
            build_hours: self.build_hours,
            workers: self.workers,
            job: self.job,
            upkeep_per_day: self.upkeep,
            buildable: self.buildable,
            material: self.material,
            outdoor: self.outdoor,
            role: self.role,
            production,
            housing,
            upgrade,
            services: self.services.clone(),
            storage: self.storage.clone(),
            sells,
            territory_radius: self.territory_radius,
            garrison: self.garrison,
            defense: self.defense,
            research_points_per_hour: self.research_points_per_hour,
            placement: self.placement.clone(),
            unlocked_by: None,
            trains: Vec::new(),
            lineage: Vec::new(),
        }
    }
}

impl RawProduction {
    fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, workers: u8, default_buffer: u16) -> ProductionDef {
        ctx.problems.check(workers > 0, loc, || "production requires workers".into());
        let gathering = self.gathering.as_ref().map(|g| g.resolve(ctx, loc));
        let is_fields = matches!(gathering, Some(Gathering::Fields { .. }));
        let plants = matches!(gathering, Some(Gathering::PlantTrees { .. }));
        if is_fields {
            ctx.problems.check(self.recipes.is_empty(), loc, || {
                "field farms take their yield from crops; recipes not allowed".into()
            });
        } else {
            ctx.problems.check(!self.recipes.is_empty(), loc, || "production needs at least one recipe".into());
        }
        let mut keys = BTreeSet::new();
        let mut recipes = Vec::with_capacity(self.recipes.len());
        for raw in &self.recipes {
            ctx.problems.check(!raw.id.is_empty() && keys.insert(raw.id.as_str()), loc, || {
                format!("recipe id '{}' empty or duplicate", raw.id)
            });
            recipes.push(raw.resolve(ctx, loc, plants));
        }
        let buffer_capacity = self.buffer.unwrap_or(default_buffer);
        ctx.problems.check(buffer_capacity > 0, loc, || "buffer must be positive".into());
        ProductionDef { gathering, recipes, uses_tools: self.uses_tools, buffer_capacity }
    }
}

impl RawRecipe {
    fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, outputs_optional: bool) -> RecipeDef {
        let what = |part: &str| format!("recipe '{}' {part}", self.id);
        ctx.problems.check(!self.name.trim().is_empty(), loc, || what("needs a name"));
        ctx.problems.check(self.hours.is_finite() && self.hours > 0.0, loc, || what("hours must be positive"));
        ctx.problems.check(outputs_optional || !self.outputs.is_empty(), loc, || what("has no outputs"));
        let inputs = resolve_rates(ctx, loc, &self.inputs, &what("inputs"));
        let outputs = resolve_rates(ctx, loc, &self.outputs, &what("outputs"));
        let mut optional_inputs = Vec::with_capacity(self.optional_inputs.len());
        for o in &self.optional_inputs {
            ctx.problems
                .check(o.amount.is_finite() && o.amount > 0.0, loc, || what("optional input amount must be positive"));
            ctx.problems.check(o.output_bonus_percent > 0, loc, || what("optional input bonus must be positive"));
            if let Some(resource) = ctx.resource(loc, &o.resource, &what("optional inputs")) {
                optional_inputs.push(OptionalInput {
                    resource,
                    amount: o.amount,
                    output_bonus_percent: o.output_bonus_percent,
                });
            }
        }
        for out in &outputs {
            ctx.problems.check(!inputs.iter().any(|i| i.resource == out.resource), loc, || {
                what("uses the same resource as input and output")
            });
        }
        RecipeDef { key: self.id.clone(), name: self.name.clone(), inputs, optional_inputs, outputs, hours: self.hours }
    }
}

impl RawGathering {
    fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>) -> Gathering {
        let radius_ok = |r: u8| r > 0;
        match self {
            RawGathering::Trees { radius } => {
                ctx.problems.check(radius_ok(*radius), loc, || "gathering radius must be positive".into());
                Gathering::Trees { radius: *radius }
            }
            RawGathering::PlantTrees { radius } => {
                ctx.problems.check(radius_ok(*radius), loc, || "planting radius must be positive".into());
                Gathering::PlantTrees { radius: *radius }
            }
            RawGathering::Deposit { kind, radius, or_terrain } => {
                ctx.problems.check(radius_ok(*radius), loc, || "deposit radius must be positive".into());
                Gathering::Deposit { kind: *kind, radius: *radius, or_terrain: *or_terrain }
            }
            RawGathering::Fields { crops, max_tiles, radius } => {
                ctx.problems.check(!crops.is_empty(), loc, || "field farm needs at least one crop".into());
                ctx.problems.check(*max_tiles > 0 && radius_ok(*radius), loc, || {
                    "max_tiles and radius must be positive".into()
                });
                let crops = crops.iter().filter_map(|c| ctx.crop(loc, c, "fields")).collect();
                Gathering::Fields { crops, max_tiles: *max_tiles, radius: *radius }
            }
            RawGathering::Pasture { tiles, radius } => {
                ctx.problems.check(*tiles > 0 && radius_ok(*radius), loc, || {
                    "pasture tiles and radius must be positive".into()
                });
                Gathering::Pasture { tiles: *tiles, radius: *radius }
            }
            RawGathering::Bees { radius, max_bonus_percent } => {
                ctx.problems.check(radius_ok(*radius), loc, || "bee radius must be positive".into());
                Gathering::Bees { radius: *radius, max_bonus_percent: *max_bonus_percent }
            }
        }
    }
}

impl RawHousing {
    fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>) -> HousingDef {
        ctx.problems.check(self.capacity > 0, loc, || "housing capacity must be positive".into());
        ctx.problems.check(self.tier >= 1, loc, || "housing tier starts at 1".into());
        ctx.problems.check(self.tax_per_resident.is_finite() && self.tax_per_resident >= 0.0, loc, || {
            "tax_per_resident must be >= 0".into()
        });
        ctx.problems.check(self.income_per_worker.is_finite() && self.income_per_worker > 0.0, loc, || {
            "income_per_worker must be positive".into()
        });
        let mut goods = Vec::with_capacity(self.requires.goods.len());
        for group in &self.requires.goods {
            ctx.problems.check(!group.is_empty(), loc, || "empty goods requirement".into());
            let mut any_of: Vec<ResourceId> =
                group.iter().filter_map(|k| ctx.resource(loc, k, "housing goods")).collect();
            any_of.sort();
            any_of.dedup();
            goods.push(GoodsRequirement { any_of });
        }
        let mut needs = self.requires.needs.clone();
        needs.sort();
        needs.dedup();
        let mut services = self.requires.services.clone();
        services.sort();
        services.dedup();
        HousingDef {
            capacity: self.capacity,
            tier: self.tier,
            class: self.class,
            tax_per_resident: self.tax_per_resident,
            income_per_worker: self.income_per_worker,
            requirements: HousingRequirements { needs, services, goods },
        }
    }
}
