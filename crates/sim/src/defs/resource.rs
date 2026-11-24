//! Resource definitions (`data/resources/*.ron`, docs/game_design.md §4).

use serde::{Deserialize, Serialize};

use super::error::Loc;
use super::ids::ResourceId;
use super::loader::Ctx;

/// Economic category of a resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ResourceCategory {
    /// Raw materials (Rohstoff).
    Raw,
    /// Food (Nahrung).
    Food,
    /// Processed goods (Ware).
    Goods,
    /// Luxury goods (Luxus).
    Luxury,
}

impl ResourceCategory {
    /// All categories in canonical order.
    pub const ALL: [ResourceCategory; 4] =
        [ResourceCategory::Raw, ResourceCategory::Food, ResourceCategory::Goods, ResourceCategory::Luxury];

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            ResourceCategory::Raw => "Rohstoff",
            ResourceCategory::Food => "Nahrung",
            ResourceCategory::Goods => "Ware",
            ResourceCategory::Luxury => "Luxus",
        }
    }
}

/// A storable resource. Gold is not a resource but the treasury.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceDef {
    /// Typed id.
    pub id: ResourceId,
    /// Canonical string id (e.g. `wood`); also the icon name `icons/res_<key>`.
    pub key: String,
    /// German display name.
    pub name: String,
    /// Economic category.
    pub category: ResourceCategory,
    /// Base market price in gold.
    pub base_price: u32,
    /// Saturation per unit when eaten (0 = not edible directly).
    pub nutrition: f32,
    /// Heating value per unit for the warmth need in winter (0 = not a fuel).
    pub heating_value: f32,
    /// Cannot be produced, only imported (spices, silk).
    pub import_only: bool,
    /// Units a household uses per resident and day while its housing tier asks for the good
    /// (goods and luxuries such as clothes, beer or wine; 0 = not a household good).
    pub household_use: f32,
}

impl ResourceDef {
    /// True for directly edible food.
    pub fn is_edible(&self) -> bool {
        self.nutrition > 0.0
    }

    /// Sprite name of the resource icon.
    pub fn icon_sprite(&self) -> String {
        format!("icons/res_{}", self.key)
    }
}

/// File form of a resource.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawResource {
    pub id: String,
    pub name: String,
    pub category: ResourceCategory,
    pub base_price: u32,
    #[serde(default)]
    pub nutrition: f32,
    #[serde(default)]
    pub heating_value: f32,
    #[serde(default)]
    pub import_only: bool,
    #[serde(default)]
    pub household_use: f32,
}

impl RawResource {
    pub fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, id: ResourceId) -> ResourceDef {
        let p = &mut ctx.problems;
        p.check(!self.name.trim().is_empty(), loc, || "name must not be empty".into());
        p.check(self.base_price > 0, loc, || "base_price must be positive".into());
        p.check(self.nutrition.is_finite() && self.nutrition >= 0.0, loc, || "nutrition must be >= 0".into());
        p.check(self.nutrition == 0.0 || self.category == ResourceCategory::Food, loc, || {
            format!("only food can have nutrition (category is {:?})", self.category)
        });
        p.check(self.heating_value.is_finite() && self.heating_value >= 0.0, loc, || {
            "heating_value must be >= 0".into()
        });
        p.check(self.household_use.is_finite() && self.household_use >= 0.0, loc, || {
            "household_use must be >= 0".into()
        });
        p.check(
            self.household_use == 0.0 || matches!(self.category, ResourceCategory::Goods | ResourceCategory::Luxury),
            loc,
            || format!("only goods and luxuries can have a household_use (category is {:?})", self.category),
        );
        ResourceDef {
            id,
            key: self.id.clone(),
            name: self.name.clone(),
            category: self.category,
            base_price: self.base_price,
            nutrition: self.nutrition,
            heating_value: self.heating_value,
            import_only: self.import_only,
            household_use: self.household_use,
        }
    }
}
