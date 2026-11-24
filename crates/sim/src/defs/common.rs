//! Types shared by several kinds of definitions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::error::Loc;
use super::ids::ResourceId;
use super::loader::Ctx;

/// An integer amount of a resource (costs, stock).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAmount {
    /// The resource.
    pub resource: ResourceId,
    /// Units.
    pub amount: u32,
}

/// A possibly fractional amount of a resource (recipe inputs/outputs; fractions accumulate).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceRate {
    /// The resource.
    pub resource: ResourceId,
    /// Units per work cycle.
    pub amount: f32,
}

/// Price of a building, road tile or unit: gold from the treasury plus materials from storage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cost {
    /// Gold from the treasury.
    pub gold: u32,
    /// Materials, sorted by resource id.
    pub materials: Vec<ResourceAmount>,
}

impl Cost {
    /// Required amount of `resource` (0 if not part of the cost).
    pub fn material(&self, resource: ResourceId) -> u32 {
        self.materials.iter().find(|m| m.resource == resource).map_or(0, |m| m.amount)
    }
}

/// File form of [`Cost`]: `(gold: 20, materials: {"wood": 12})`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawCost {
    #[serde(default)]
    pub gold: u32,
    #[serde(default)]
    pub materials: BTreeMap<String, u32>,
}

impl RawCost {
    pub fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, what: &str) -> Cost {
        let mut materials = Vec::with_capacity(self.materials.len());
        for (key, &amount) in &self.materials {
            ctx.problems.check(amount > 0, loc, || format!("{what}: amount of '{key}' must be positive"));
            if let Some(resource) = ctx.resource(loc, key, what) {
                materials.push(ResourceAmount { resource, amount });
            }
        }
        materials.sort_by_key(|m| m.resource);
        Cost { gold: self.gold, materials }
    }
}

/// Resolves a `{"resource": amount}` map of fractional amounts, checking `amount > 0`.
pub(crate) fn resolve_rates(
    ctx: &mut Ctx,
    loc: &Loc<'_>,
    map: &BTreeMap<String, f32>,
    what: &str,
) -> Vec<ResourceRate> {
    let mut out = Vec::with_capacity(map.len());
    for (key, &amount) in map {
        ctx.problems
            .check(amount.is_finite() && amount > 0.0, loc, || format!("{what}: amount of '{key}' must be positive"));
        if let Some(resource) = ctx.resource(loc, key, what) {
            out.push(ResourceRate { resource, amount });
        }
    }
    out.sort_by_key(|r| r.resource);
    out
}

/// Citizen needs (0–100 each, docs/game_design.md §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NeedKind {
    /// Food from the household supply.
    Hunger,
    /// Water from wells or rivers.
    Thirst,
    /// A home.
    Shelter,
    /// Firewood/coal in winter, clothes.
    Warmth,
    /// Watchtowers/barracks, no bandits.
    Safety,
    /// Hospital, varied food.
    Health,
    /// Family, tavern, market.
    Social,
    /// Tavern, festivals.
    Entertainment,
    /// Church.
    Faith,
}

impl NeedKind {
    /// All needs in canonical order.
    pub const ALL: [NeedKind; 9] = [
        NeedKind::Hunger,
        NeedKind::Thirst,
        NeedKind::Shelter,
        NeedKind::Warmth,
        NeedKind::Safety,
        NeedKind::Health,
        NeedKind::Social,
        NeedKind::Entertainment,
        NeedKind::Faith,
    ];

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            NeedKind::Hunger => "Nahrung",
            NeedKind::Thirst => "Wasser",
            NeedKind::Shelter => "Unterkunft",
            NeedKind::Warmth => "Wärme",
            NeedKind::Safety => "Sicherheit",
            NeedKind::Health => "Gesundheit",
            NeedKind::Social => "Gemeinschaft",
            NeedKind::Entertainment => "Unterhaltung",
            NeedKind::Faith => "Glaube",
        }
    }
}

/// Area services provided by public and military buildings within a radius.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ServiceKind {
    /// Drinking water (well).
    Water,
    /// Fire fighting (well).
    FireProtection,
    /// Distribution of food and goods to households (market).
    Market,
    /// Religion (church).
    Religion,
    /// Education (school).
    Education,
    /// Health care and plague protection (hospital).
    Health,
    /// Entertainment (tavern).
    Entertainment,
    /// Security (watchtower, barracks, castle).
    Safety,
    /// Administration and tax collection (town hall).
    Administration,
}

impl ServiceKind {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            ServiceKind::Water => "Wasser",
            ServiceKind::FireProtection => "Brandschutz",
            ServiceKind::Market => "Markt",
            ServiceKind::Religion => "Religion",
            ServiceKind::Education => "Bildung",
            ServiceKind::Health => "Gesundheit",
            ServiceKind::Entertainment => "Unterhaltung",
            ServiceKind::Safety => "Sicherheit",
            ServiceKind::Administration => "Verwaltung",
        }
    }
}

/// Job categories whose priority the player can set (docs/game_design.md §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JobCategory {
    /// Farms, fishery, food processing.
    Food,
    /// Woodcutters, quarries, mines.
    RawMaterials,
    /// Workshops turning raw materials into goods.
    Processing,
    /// Builders.
    Construction,
    /// Public services, storage, carriers, trade.
    Services,
    /// Military buildings.
    Military,
}

impl JobCategory {
    /// All categories in canonical order.
    pub const ALL: [JobCategory; 6] = [
        JobCategory::Food,
        JobCategory::RawMaterials,
        JobCategory::Processing,
        JobCategory::Construction,
        JobCategory::Services,
        JobCategory::Military,
    ];

    /// Position in [`JobCategory::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            JobCategory::Food => "Nahrung",
            JobCategory::RawMaterials => "Rohstoffe",
            JobCategory::Processing => "Verarbeitung",
            JobCategory::Construction => "Bau",
            JobCategory::Services => "Dienste",
            JobCategory::Military => "Militär",
        }
    }
}

/// Qualification level of a job; determines the wage (see [`Wages`](super::Wages)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JobTier {
    /// Simple labour.
    Unskilled,
    /// Craftsmen.
    Skilled,
    /// Requires education (school, hospital, jeweler, artisan).
    Educated,
}

impl JobTier {
    /// True if workers need education for this job.
    pub const fn requires_education(self) -> bool {
        matches!(self, JobTier::Educated)
    }

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            JobTier::Unskilled => "Hilfskraft",
            JobTier::Skilled => "Fachkraft",
            JobTier::Educated => "Gelehrter",
        }
    }
}

/// Social class of a household (determines tax and needs of higher housing tiers).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SocialClass {
    /// Ordinary people (tiers 1–4).
    Commoner,
    /// Wealthy citizens (residence).
    Wealthy,
    /// Nobility (manor).
    Noble,
}

impl SocialClass {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            SocialClass::Commoner => "Volk",
            SocialClass::Wealthy => "Wohlhabende",
            SocialClass::Noble => "Adel",
        }
    }
}
