//! Technology definitions (`data/technologies/*.ron`, docs/game_design.md §9).

use serde::{Deserialize, Serialize};

use super::building::BuildingCategory;
use super::error::Loc;
use super::ids::{BuildingTypeId, CropId, RoadTypeId, TechId, UnitTypeId};
use super::loader::Ctx;

/// Research area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TechArea {
    /// Landwirtschaft.
    Agriculture,
    /// Architektur.
    Architecture,
    /// Handwerk.
    Crafts,
    /// Bergbau.
    Mining,
    /// Handel.
    Trade,
    /// Medizin.
    Medicine,
    /// Militär.
    Military,
    /// Verwaltung.
    Administration,
}

impl TechArea {
    /// All areas in canonical order.
    pub const ALL: [TechArea; 8] = [
        TechArea::Agriculture,
        TechArea::Architecture,
        TechArea::Crafts,
        TechArea::Mining,
        TechArea::Trade,
        TechArea::Medicine,
        TechArea::Military,
        TechArea::Administration,
    ];

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            TechArea::Agriculture => "Landwirtschaft",
            TechArea::Architecture => "Architektur",
            TechArea::Crafts => "Handwerk",
            TechArea::Mining => "Bergbau",
            TechArea::Trade => "Handel",
            TechArea::Medicine => "Medizin",
            TechArea::Military => "Militär",
            TechArea::Administration => "Verwaltung",
        }
    }
}

/// Buildings affected by a production bonus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BonusTarget {
    /// Every production building.
    All,
    /// All buildings of a category.
    Category(BuildingCategory),
    /// One building type.
    Building(BuildingTypeId),
}

impl BonusTarget {
    /// True if buildings of type `def` are affected (a bonus for a type also applies to its
    /// upgrades, [`crate::defs::BuildingDef::descends_from`]).
    pub fn matches(self, def: &crate::defs::BuildingDef) -> bool {
        match self {
            BonusTarget::All => true,
            BonusTarget::Category(c) => c == def.category,
            BonusTarget::Building(b) => def.descends_from(b),
        }
    }
}

/// Concrete effect of a researched technology. Percentages are additive with other bonuses of
/// the same kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechEffect {
    /// Makes a building type available.
    UnlockBuilding(BuildingTypeId),
    /// Makes a road type available.
    UnlockRoad(RoadTypeId),
    /// Makes a unit type available.
    UnlockUnit(UnitTypeId),
    /// Faster work cycles.
    ProductionBonus {
        /// Affected buildings.
        target: BonusTarget,
        /// Speed increase.
        percent: i16,
    },
    /// Higher field yields (`None` = all crops).
    FieldYieldBonus {
        /// Affected crop.
        crop: Option<CropId>,
        /// Yield increase.
        percent: i16,
    },
    /// Fields within `max_distance` of fresh water yield more.
    IrrigationBonus {
        /// Distance to water in tiles.
        max_distance: u8,
        /// Yield increase.
        percent: i16,
    },
    /// Faster regeneration of soil fertility (crop rotation).
    SoilRegeneration {
        /// Regeneration speed increase.
        percent: i16,
    },
    /// More storage capacity in all storage buildings.
    StorageBonus {
        /// Capacity increase.
        percent: i16,
    },
    /// Larger service radius of a building type.
    ServiceRadiusBonus {
        /// Affected building type.
        building: BuildingTypeId,
        /// Additional radius.
        tiles: u8,
    },
    /// More residents per house.
    HousingCapacityBonus {
        /// Capacity increase.
        percent: i16,
    },
    /// Higher tax income.
    TaxBonus {
        /// Income increase.
        percent: i16,
    },
    /// Better prices when trading with other kingdoms.
    TradePriceBonus {
        /// Price advantage.
        percent: i16,
    },
    /// Lower building upkeep.
    UpkeepReduction {
        /// Reduction (0–100).
        percent: i16,
    },
    /// Faster construction.
    BuildSpeedBonus {
        /// Speed increase.
        percent: i16,
    },
    /// Faster research.
    ResearchSpeedBonus {
        /// Speed increase.
        percent: i16,
    },
    /// Lower chance of fires.
    FireRiskReduction {
        /// Reduction (0–100).
        percent: i16,
    },
    /// Lower chance of disease and plague.
    DiseaseResistance {
        /// Reduction (0–100).
        percent: i16,
    },
    /// Tools wear out more slowly.
    ToolWearReduction {
        /// Reduction (0–100).
        percent: i16,
    },
    /// Stronger units (`None` = all units).
    UnitBonus {
        /// Affected unit type.
        unit: Option<UnitTypeId>,
        /// Attack increase.
        attack_percent: i16,
        /// Defense increase.
        defense_percent: i16,
    },
    /// Sturdier walls and gates.
    WallStrengthBonus {
        /// Hit point increase.
        percent: i16,
    },
}

/// A technology.
#[derive(Clone, Debug, PartialEq)]
pub struct TechDef {
    /// Typed id.
    pub id: TechId,
    /// Canonical string id.
    pub key: String,
    /// German display name.
    pub name: String,
    /// German description.
    pub description: String,
    /// Research area.
    pub area: TechArea,
    /// Gold paid when research starts.
    pub cost_gold: u32,
    /// Research points needed; research time = points / research rate (points per game hour,
    /// see [`ResearchRules`](super::ResearchRules) and `research_points_per_hour` of buildings).
    pub research_points: u32,
    /// Technologies that must be researched first.
    pub prerequisites: Vec<TechId>,
    /// Effects once researched.
    pub effects: Vec<TechEffect>,
}

// ------------------------------------------------------------------------------------------------
// File form
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum RawBonusTarget {
    All,
    Category(BuildingCategory),
    Building(String),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum RawTechEffect {
    UnlockBuilding(String),
    UnlockRoad(String),
    UnlockUnit(String),
    ProductionBonus {
        target: RawBonusTarget,
        percent: i16,
    },
    FieldYieldBonus {
        #[serde(default)]
        crop: Option<String>,
        percent: i16,
    },
    IrrigationBonus {
        max_distance: u8,
        percent: i16,
    },
    SoilRegeneration {
        percent: i16,
    },
    StorageBonus {
        percent: i16,
    },
    ServiceRadiusBonus {
        building: String,
        tiles: u8,
    },
    HousingCapacityBonus {
        percent: i16,
    },
    TaxBonus {
        percent: i16,
    },
    TradePriceBonus {
        percent: i16,
    },
    UpkeepReduction {
        percent: i16,
    },
    BuildSpeedBonus {
        percent: i16,
    },
    ResearchSpeedBonus {
        percent: i16,
    },
    FireRiskReduction {
        percent: i16,
    },
    DiseaseResistance {
        percent: i16,
    },
    ToolWearReduction {
        percent: i16,
    },
    UnitBonus {
        #[serde(default)]
        unit: Option<String>,
        attack_percent: i16,
        defense_percent: i16,
    },
    WallStrengthBonus {
        percent: i16,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawTech {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub area: TechArea,
    pub cost_gold: u32,
    pub research_points: u32,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    pub effects: Vec<RawTechEffect>,
}

/// Checks a bonus percentage: positive and at most +500 %.
fn bonus(ctx: &mut Ctx, loc: &Loc<'_>, percent: i16, what: &str) {
    ctx.problems.check((1..=500).contains(&percent), loc, || format!("{what}: percent {percent} outside 1..=500"));
}

/// Checks a reduction percentage: 1–100 %.
fn reduction(ctx: &mut Ctx, loc: &Loc<'_>, percent: i16, what: &str) {
    ctx.problems.check((1..=100).contains(&percent), loc, || format!("{what}: percent {percent} outside 1..=100"));
}

impl RawTechEffect {
    fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>) -> Option<TechEffect> {
        use RawTechEffect as R;
        use TechEffect as E;
        Some(match self {
            R::UnlockBuilding(k) => E::UnlockBuilding(ctx.building(loc, k, "UnlockBuilding")?),
            R::UnlockRoad(k) => E::UnlockRoad(ctx.road(loc, k, "UnlockRoad")?),
            R::UnlockUnit(k) => E::UnlockUnit(ctx.unit(loc, k, "UnlockUnit")?),
            R::ProductionBonus { target, percent } => {
                bonus(ctx, loc, *percent, "ProductionBonus");
                let target = match target {
                    RawBonusTarget::All => BonusTarget::All,
                    RawBonusTarget::Category(c) => BonusTarget::Category(*c),
                    RawBonusTarget::Building(k) => BonusTarget::Building(ctx.building(loc, k, "ProductionBonus")?),
                };
                E::ProductionBonus { target, percent: *percent }
            }
            R::FieldYieldBonus { crop, percent } => {
                bonus(ctx, loc, *percent, "FieldYieldBonus");
                let crop = match crop {
                    Some(k) => Some(ctx.crop(loc, k, "FieldYieldBonus")?),
                    None => None,
                };
                E::FieldYieldBonus { crop, percent: *percent }
            }
            R::IrrigationBonus { max_distance, percent } => {
                bonus(ctx, loc, *percent, "IrrigationBonus");
                ctx.problems.check(*max_distance > 0, loc, || "IrrigationBonus: max_distance must be positive".into());
                E::IrrigationBonus { max_distance: *max_distance, percent: *percent }
            }
            R::SoilRegeneration { percent } => {
                bonus(ctx, loc, *percent, "SoilRegeneration");
                E::SoilRegeneration { percent: *percent }
            }
            R::StorageBonus { percent } => {
                bonus(ctx, loc, *percent, "StorageBonus");
                E::StorageBonus { percent: *percent }
            }
            R::ServiceRadiusBonus { building, tiles } => {
                ctx.problems.check(*tiles > 0, loc, || "ServiceRadiusBonus: tiles must be positive".into());
                E::ServiceRadiusBonus { building: ctx.building(loc, building, "ServiceRadiusBonus")?, tiles: *tiles }
            }
            R::HousingCapacityBonus { percent } => {
                bonus(ctx, loc, *percent, "HousingCapacityBonus");
                E::HousingCapacityBonus { percent: *percent }
            }
            R::TaxBonus { percent } => {
                bonus(ctx, loc, *percent, "TaxBonus");
                E::TaxBonus { percent: *percent }
            }
            R::TradePriceBonus { percent } => {
                bonus(ctx, loc, *percent, "TradePriceBonus");
                E::TradePriceBonus { percent: *percent }
            }
            R::UpkeepReduction { percent } => {
                reduction(ctx, loc, *percent, "UpkeepReduction");
                E::UpkeepReduction { percent: *percent }
            }
            R::BuildSpeedBonus { percent } => {
                bonus(ctx, loc, *percent, "BuildSpeedBonus");
                E::BuildSpeedBonus { percent: *percent }
            }
            R::ResearchSpeedBonus { percent } => {
                bonus(ctx, loc, *percent, "ResearchSpeedBonus");
                E::ResearchSpeedBonus { percent: *percent }
            }
            R::FireRiskReduction { percent } => {
                reduction(ctx, loc, *percent, "FireRiskReduction");
                E::FireRiskReduction { percent: *percent }
            }
            R::DiseaseResistance { percent } => {
                reduction(ctx, loc, *percent, "DiseaseResistance");
                E::DiseaseResistance { percent: *percent }
            }
            R::ToolWearReduction { percent } => {
                reduction(ctx, loc, *percent, "ToolWearReduction");
                E::ToolWearReduction { percent: *percent }
            }
            R::UnitBonus { unit, attack_percent, defense_percent } => {
                ctx.problems.check(
                    (0..=500).contains(attack_percent)
                        && (0..=500).contains(defense_percent)
                        && (*attack_percent > 0 || *defense_percent > 0),
                    loc,
                    || "UnitBonus: percentages must be 0..=500 and not both 0".into(),
                );
                let unit = match unit {
                    Some(k) => Some(ctx.unit(loc, k, "UnitBonus")?),
                    None => None,
                };
                E::UnitBonus { unit, attack_percent: *attack_percent, defense_percent: *defense_percent }
            }
            R::WallStrengthBonus { percent } => {
                bonus(ctx, loc, *percent, "WallStrengthBonus");
                E::WallStrengthBonus { percent: *percent }
            }
        })
    }
}

impl RawTech {
    pub fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, id: TechId) -> TechDef {
        ctx.problems.check(!self.name.trim().is_empty(), loc, || "name must not be empty".into());
        ctx.problems.check(!self.description.trim().is_empty(), loc, || "description must not be empty".into());
        ctx.problems.check(self.research_points > 0, loc, || "research_points must be positive".into());
        ctx.problems.check(!self.effects.is_empty(), loc, || "a technology needs at least one effect".into());
        let mut prerequisites = Vec::with_capacity(self.prerequisites.len());
        for k in &self.prerequisites {
            if k == &self.id {
                ctx.problems.at(loc, "technology lists itself as prerequisite");
            } else if let Some(t) = ctx.tech(loc, k, "prerequisites") {
                if prerequisites.contains(&t) {
                    ctx.problems.at(loc, format!("prerequisite '{k}' listed twice"));
                } else {
                    prerequisites.push(t);
                }
            }
        }
        let effects = self.effects.iter().filter_map(|e| e.resolve(ctx, loc)).collect();
        TechDef {
            id,
            key: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            area: self.area,
            cost_gold: self.cost_gold,
            research_points: self.research_points,
            prerequisites,
            effects,
        }
    }
}
