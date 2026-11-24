//! Random event definitions (`data/events/*.ron`, docs/game_design.md §12).

use serde::{Deserialize, Serialize};

use super::building::BuildingCategory;
use super::error::Loc;
use super::ids::{BuildingTypeId, EventDefId, ResourceId};
use super::loader::Ctx;
use super::resource::ResourceCategory;
use super::rules::Weather;
use crate::event::Severity;
use crate::time::Season;
use crate::world::DepositKind;

/// Condition on the world state. Event triggers and choice requirements are lists of conditions
/// that must *all* hold; use [`EventCondition::AnyOf`] for alternatives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventCondition {
    /// Current season.
    Season(Season),
    /// At least this many whole days have passed since the game started.
    MinDay(u32),
    /// Population at least.
    MinPopulation(u32),
    /// Population at most.
    MaxPopulation(u32),
    /// At least one finished building of any of these types exists.
    HasAnyBuilding(Vec<BuildingTypeId>),
    /// No finished building of these types exists.
    LacksBuildings(Vec<BuildingTypeId>),
    /// At least this many finished buildings of a category.
    MinBuildingsInCategory {
        /// Category.
        category: BuildingCategory,
        /// Minimum count.
        count: u32,
    },
    /// At least `count` wooden buildings (with `unprotected`: outside any fire protection radius).
    MinWoodenBuildings {
        /// Minimum count.
        count: u32,
        /// Count only buildings without fire protection coverage.
        unprotected: bool,
    },
    /// At least `count` buildings within `distance` tiles of a river.
    MinBuildingsNearRiver {
        /// Minimum count.
        count: u32,
        /// Distance to the river in tiles.
        distance: u8,
    },
    /// Average satisfaction (0–100) at most.
    MaxSatisfaction(u8),
    /// Average satisfaction (0–100) at least.
    MinSatisfaction(u8),
    /// Tax rate in percent at least.
    MinTaxRate(u8),
    /// Free housing capacity at least.
    MinFreeHousing(u32),
    /// Gold in the treasury at least.
    MinTreasury(u32),
    /// Stored units of a resource at least.
    MinStock {
        /// Resource.
        resource: ResourceId,
        /// Minimum units.
        amount: u32,
    },
    /// Soldiers (all units) at least.
    MinSoldiers(u32),
    /// Share of houses covered by a safety service at most (percent).
    MaxSafetyCoverage(u8),
    /// Today's weather.
    Weather(Weather),
    /// Today's weather intensity in percent at least.
    MinWeatherIntensity(u8),
    /// A war is going on: the player's or one between AI kingdoms (docs/game_design.md §10).
    WarNearby,
    /// The bandits' grace period is over (`military.bandits`: `grace_days` of the difficulty
    /// and `grace_population` inhabitants; docs/game_design.md §11).
    RaidGraceOver,
    /// At least this many days in a row without rain, storm or snow, today included.
    MinDryDays(u16),
    /// Average condition of the sown fields in percent at least (false without sown fields).
    MinFieldCondition(u8),
    /// Average condition of the sown fields in percent at most (false without sown fields).
    MaxFieldCondition(u8),
    /// None of these events occurred in the current season (keeps contradicting events such as
    /// a bad and a bountiful harvest apart).
    NoEventThisSeason(Vec<EventDefId>),
    /// At least one of the nested conditions holds.
    AnyOf(Vec<EventCondition>),
}

/// What damaged a building ([`EventEffect::DamageBuildings`]); decides the goods lost inside
/// and the German texts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DamageCause {
    /// Burned down (`events.fire_goods_loss_percent` of the goods inside are lost).
    Fire,
    /// Flooded (`events.flood_goods_loss_percent` of the goods inside are lost).
    Flood,
    /// Storm damage (goods inside stay).
    Storm,
    /// Destroyed by enemies in battle (`events.flood_goods_loss_percent` of the goods inside
    /// are lost; docs/game_design.md §11).
    Attack,
}

impl DamageCause {
    /// German noun for reports, e.g. „Brand“.
    pub const fn display_name(self) -> &'static str {
        match self {
            DamageCause::Fire => "Brand",
            DamageCause::Flood => "Überschwemmung",
            DamageCause::Storm => "Sturm",
            DamageCause::Attack => "Angriff",
        }
    }

    /// German participle: „… ist niedergebrannt“ / „überschwemmt“ / „vom Sturm beschädigt“.
    pub const fn participle(self) -> &'static str {
        match self {
            DamageCause::Fire => "niedergebrannt",
            DamageCause::Flood => "überschwemmt",
            DamageCause::Storm => "vom Sturm beschädigt",
            DamageCause::Attack => "von Feinden zerstört",
        }
    }
}

/// Risk an event stands for: its weight shrinks with the matching protection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EventRisk {
    /// Weight × share of wooden buildings without fire protection × (100 % − researched
    /// `FireRiskReduction`).
    Fire,
    /// Weight × (100 % − researched `DiseaseResistance`).
    Disease,
}

/// Buildings affected by [`EventEffect::DamageBuildings`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum BuildingFilter {
    /// Any building except the headquarters.
    Any,
    /// Wooden buildings.
    Wooden,
    /// Wooden buildings without fire protection coverage.
    WoodenUnprotected,
    /// Buildings within `distance` tiles of a river.
    NearRiver {
        /// Distance in tiles.
        distance: u8,
    },
    /// Buildings without safety coverage.
    Unguarded,
    /// Field tiles of farms.
    Fields,
}

/// Consequence of an event choice.
#[derive(Clone, Debug, PartialEq)]
pub enum EventEffect {
    /// Gold added to (positive) or removed from (negative, clamped at 0) the treasury.
    Gold(i32),
    /// Units added to or removed from storage.
    Resource {
        /// Resource.
        resource: ResourceId,
        /// Units (negative = loss, clamped at the stock).
        amount: i32,
    },
    /// A share of all stored resources of a category is lost.
    LoseStock {
        /// Category.
        category: ResourceCategory,
        /// Share in percent.
        percent: u8,
    },
    /// Festival days: in the afternoon the citizens celebrate at the church or market instead
    /// of working (docs/game_design.md §6, §12).
    Festival {
        /// Duration in days.
        days: u16,
    },
    /// Temporary satisfaction modifier for all citizens.
    Satisfaction {
        /// Points (−100..100).
        amount: i8,
        /// Duration in days.
        days: u16,
    },
    /// Temporary field yield modifier.
    FieldYield {
        /// Percent.
        percent: i16,
        /// Duration in days.
        days: u16,
    },
    /// Temporary production speed modifier.
    Production {
        /// Percent.
        percent: i16,
        /// Duration in days.
        days: u16,
    },
    /// Forces a weather condition from today.
    Weather {
        /// Weather.
        weather: Weather,
        /// Duration in days.
        days: u16,
        /// Intensity in percent (`None`: `events.weather_intensity`).
        intensity: Option<u8>,
    },
    /// New settlers arrive (if housing allows, the rest camp and leave after a while).
    Settlers(u16),
    /// A share of citizens falls ill.
    Disease {
        /// Share in percent.
        percent: u8,
    },
    /// Damages buildings (they become repair sites) or destroys the crops of field tiles.
    DamageBuildings {
        /// Affected buildings.
        filter: BuildingFilter,
        /// Number of buildings (field tiles for [`BuildingFilter::Fields`]).
        count: u8,
        /// What damages them.
        cause: DamageCause,
    },
    /// A new deposit of one of `kinds` (chosen randomly) appears near the settlement.
    SpawnDeposit {
        /// Candidate kinds.
        kinds: Vec<DepositKind>,
        /// Units.
        amount: u16,
        /// Maximum distance from the headquarters.
        radius: u16,
    },
    /// Lowers the tax rate to at most `max_percent` and keeps it there for `days` days.
    TaxRateCap {
        /// New maximum in percent.
        max_percent: u8,
        /// Duration in days.
        days: u16,
    },
    /// A band of bandits gathers at the map edge and raids the settlement (docs/game_design.md
    /// §11): `percent` of the normal band size (`military.bandits`).
    Raiders {
        /// Band size in percent of the normal size.
        percent: u8,
    },
    /// All ready soldiers hunt the enemies on the map, wherever they are, for `hours` hours.
    Alarm {
        /// Duration in game hours.
        hours: u8,
    },
    /// All ready soldiers march to the headquarters and stand guard there for `hours` hours.
    Muster {
        /// Duration in game hours.
        hours: u8,
    },
}

/// A decision offered to the player.
#[derive(Clone, Debug, PartialEq)]
pub struct EventChoice {
    /// German button label.
    pub label: String,
    /// Conditions for the choice to be selectable (e.g. enough gold).
    pub requires: Vec<EventCondition>,
    /// Consequences.
    pub effects: Vec<EventEffect>,
}

/// A random event.
#[derive(Clone, Debug, PartialEq)]
pub struct EventDef {
    /// Typed id.
    pub id: EventDefId,
    /// Canonical string id.
    pub key: String,
    /// German title.
    pub title: String,
    /// German event text.
    pub text: String,
    /// German event text naming the building the event starts at (the focus, e.g. the burning
    /// house): `{building}` stands for its name. Used instead of `text` when there is a focus.
    pub focus_text: Option<String>,
    /// Notification severity.
    pub severity: Severity,
    /// Trigger conditions (all must hold).
    pub conditions: Vec<EventCondition>,
    /// Relative weight among eligible events.
    pub weight: u32,
    /// Days before the event can occur again.
    pub cooldown_days: u32,
    /// Risk that scales the weight with the matching protection.
    pub risk: Option<EventRisk>,
    /// Choices (the first one is the default if the player does not decide).
    pub choices: Vec<EventChoice>,
}

// ------------------------------------------------------------------------------------------------
// File form
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum RawCondition {
    Season(Season),
    MinDay(u32),
    MinPopulation(u32),
    MaxPopulation(u32),
    HasAnyBuilding(Vec<String>),
    LacksBuildings(Vec<String>),
    MinBuildingsInCategory {
        category: BuildingCategory,
        count: u32,
    },
    MinWoodenBuildings {
        count: u32,
        #[serde(default)]
        unprotected: bool,
    },
    MinBuildingsNearRiver {
        count: u32,
        distance: u8,
    },
    MaxSatisfaction(u8),
    MinSatisfaction(u8),
    MinTaxRate(u8),
    MinFreeHousing(u32),
    MinTreasury(u32),
    MinStock {
        resource: String,
        amount: u32,
    },
    MinSoldiers(u32),
    MaxSafetyCoverage(u8),
    Weather(Weather),
    MinWeatherIntensity(u8),
    WarNearby,
    RaidGraceOver,
    MinDryDays(u16),
    MinFieldCondition(u8),
    MaxFieldCondition(u8),
    NoEventThisSeason(Vec<String>),
    AnyOf(Vec<RawCondition>),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum RawEffect {
    Gold(i32),
    Resource { resource: String, amount: i32 },
    LoseStock { category: ResourceCategory, percent: u8 },
    Satisfaction { amount: i8, days: u16 },
    Festival { days: u16 },
    FieldYield { percent: i16, days: u16 },
    Production { percent: i16, days: u16 },
    Weather {
        weather: Weather,
        days: u16,
        #[serde(default)]
        intensity: Option<u8>,
    },
    Settlers(u16),
    Disease { percent: u8 },
    DamageBuildings { filter: BuildingFilter, count: u8, cause: DamageCause },
    SpawnDeposit { kinds: Vec<DepositKind>, amount: u16, radius: u16 },
    TaxRateCap { max_percent: u8, days: u16 },
    Raiders { percent: u8 },
    Alarm { hours: u8 },
    Muster { hours: u8 },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawChoice {
    pub label: String,
    #[serde(default)]
    pub requires: Vec<RawCondition>,
    #[serde(default)]
    pub effects: Vec<RawEffect>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawEvent {
    pub id: String,
    pub title: String,
    pub text: String,
    #[serde(default)]
    pub focus_text: Option<String>,
    pub severity: Severity,
    #[serde(default)]
    pub conditions: Vec<RawCondition>,
    pub weight: u32,
    pub cooldown_days: u32,
    #[serde(default)]
    pub risk: Option<EventRisk>,
    pub choices: Vec<RawChoice>,
}

fn percent_ok(ctx: &mut Ctx, loc: &Loc<'_>, value: u8, what: &str) {
    ctx.problems.check(value <= 100, loc, || format!("{what}: {value} is not a percentage"));
}

impl RawCondition {
    fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>) -> Option<EventCondition> {
        use EventCondition as C;
        use RawCondition as R;
        let buildings = |ctx: &mut Ctx, keys: &[String], what: &str| -> Option<Vec<BuildingTypeId>> {
            ctx.problems.check(!keys.is_empty(), loc, || format!("{what}: building list must not be empty"));
            let ids: Vec<_> = keys.iter().filter_map(|k| ctx.building(loc, k, what)).collect();
            (ids.len() == keys.len()).then_some(ids)
        };
        Some(match self {
            R::Season(s) => C::Season(*s),
            R::MinDay(d) => C::MinDay(*d),
            R::MinPopulation(n) => C::MinPopulation(*n),
            R::MaxPopulation(n) => C::MaxPopulation(*n),
            R::HasAnyBuilding(keys) => C::HasAnyBuilding(buildings(ctx, keys, "HasAnyBuilding")?),
            R::LacksBuildings(keys) => C::LacksBuildings(buildings(ctx, keys, "LacksBuildings")?),
            R::MinBuildingsInCategory { category, count } => {
                C::MinBuildingsInCategory { category: *category, count: *count }
            }
            R::MinWoodenBuildings { count, unprotected } => {
                C::MinWoodenBuildings { count: *count, unprotected: *unprotected }
            }
            R::MinBuildingsNearRiver { count, distance } => {
                C::MinBuildingsNearRiver { count: *count, distance: *distance }
            }
            R::MaxSatisfaction(v) => {
                percent_ok(ctx, loc, *v, "MaxSatisfaction");
                C::MaxSatisfaction(*v)
            }
            R::MinSatisfaction(v) => {
                percent_ok(ctx, loc, *v, "MinSatisfaction");
                C::MinSatisfaction(*v)
            }
            R::MinTaxRate(v) => {
                percent_ok(ctx, loc, *v, "MinTaxRate");
                C::MinTaxRate(*v)
            }
            R::MinFreeHousing(n) => C::MinFreeHousing(*n),
            R::MinTreasury(n) => C::MinTreasury(*n),
            R::MinStock { resource, amount } => {
                ctx.problems.check(*amount > 0, loc, || "MinStock: amount must be positive".into());
                C::MinStock { resource: ctx.resource(loc, resource, "MinStock")?, amount: *amount }
            }
            R::MinSoldiers(n) => C::MinSoldiers(*n),
            R::WarNearby => C::WarNearby,
            R::RaidGraceOver => C::RaidGraceOver,
            R::MinDryDays(n) => {
                ctx.problems.check(*n > 0, loc, || "MinDryDays: days must be positive".into());
                C::MinDryDays(*n)
            }
            R::MinFieldCondition(v) => {
                percent_ok(ctx, loc, *v, "MinFieldCondition");
                C::MinFieldCondition(*v)
            }
            R::MaxFieldCondition(v) => {
                percent_ok(ctx, loc, *v, "MaxFieldCondition");
                C::MaxFieldCondition(*v)
            }
            R::NoEventThisSeason(keys) => {
                ctx.problems.check(!keys.is_empty(), loc, || "NoEventThisSeason: event list must not be empty".into());
                let ids: Vec<_> = keys.iter().filter_map(|k| ctx.event(loc, k, "NoEventThisSeason")).collect();
                if ids.len() != keys.len() {
                    return None;
                }
                C::NoEventThisSeason(ids)
            }
            R::MaxSafetyCoverage(v) => {
                percent_ok(ctx, loc, *v, "MaxSafetyCoverage");
                C::MaxSafetyCoverage(*v)
            }
            R::Weather(w) => C::Weather(*w),
            R::MinWeatherIntensity(v) => {
                percent_ok(ctx, loc, *v, "MinWeatherIntensity");
                C::MinWeatherIntensity(*v)
            }
            R::AnyOf(list) => {
                ctx.problems.check(list.len() >= 2, loc, || "AnyOf needs at least two alternatives".into());
                let resolved: Vec<_> = list.iter().filter_map(|c| c.resolve(ctx, loc)).collect();
                if resolved.len() != list.len() {
                    return None;
                }
                C::AnyOf(resolved)
            }
        })
    }
}

impl RawEffect {
    fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>) -> Option<EventEffect> {
        use EventEffect as E;
        use RawEffect as R;
        Some(match self {
            R::Gold(g) => {
                ctx.problems.check(*g != 0, loc, || "Gold effect of 0".into());
                E::Gold(*g)
            }
            R::Resource { resource, amount } => {
                ctx.problems.check(*amount != 0, loc, || "Resource effect of 0".into());
                E::Resource { resource: ctx.resource(loc, resource, "Resource effect")?, amount: *amount }
            }
            R::LoseStock { category, percent } => {
                ctx.problems.check((1..=100).contains(percent), loc, || "LoseStock: percent must be 1..=100".into());
                E::LoseStock { category: *category, percent: *percent }
            }
            R::Festival { days } => {
                ctx.problems.check(*days > 0, loc, || "Festival: days must be positive".into());
                E::Festival { days: *days }
            }
            R::Satisfaction { amount, days } => {
                ctx.problems
                    .check(*amount != 0 && *days > 0, loc, || "Satisfaction: amount and days must be non-zero".into());
                E::Satisfaction { amount: *amount, days: *days }
            }
            R::FieldYield { percent, days } => {
                ctx.problems.check(*percent >= -100 && *percent != 0 && *days > 0, loc, || {
                    "FieldYield: percent must be >= -100 and non-zero, days positive".into()
                });
                E::FieldYield { percent: *percent, days: *days }
            }
            R::Production { percent, days } => {
                ctx.problems.check(*percent >= -100 && *percent != 0 && *days > 0, loc, || {
                    "Production: percent must be >= -100 and non-zero, days positive".into()
                });
                E::Production { percent: *percent, days: *days }
            }
            R::Weather { weather, days, intensity } => {
                ctx.problems.check(*days > 0, loc, || "Weather: days must be positive".into());
                ctx.problems.check(intensity.is_none_or(|i| (1..=100).contains(&i)), loc, || {
                    "Weather: intensity must be 1..=100".into()
                });
                E::Weather { weather: *weather, days: *days, intensity: *intensity }
            }
            R::Settlers(n) => {
                ctx.problems.check(*n > 0, loc, || "Settlers: count must be positive".into());
                E::Settlers(*n)
            }
            R::Disease { percent } => {
                ctx.problems.check((1..=100).contains(percent), loc, || "Disease: percent must be 1..=100".into());
                E::Disease { percent: *percent }
            }
            R::DamageBuildings { filter, count, cause } => {
                ctx.problems.check(*count > 0, loc, || "DamageBuildings: count must be positive".into());
                E::DamageBuildings { filter: *filter, count: *count, cause: *cause }
            }
            R::SpawnDeposit { kinds, amount, radius } => {
                ctx.problems.check(!kinds.is_empty() && *amount > 0 && *radius > 0, loc, || {
                    "SpawnDeposit: kinds must not be empty, amount and radius must be positive".into()
                });
                E::SpawnDeposit { kinds: kinds.clone(), amount: *amount, radius: *radius }
            }
            R::TaxRateCap { max_percent, days } => {
                percent_ok(ctx, loc, *max_percent, "TaxRateCap");
                ctx.problems.check(*days > 0, loc, || "TaxRateCap: days must be positive".into());
                E::TaxRateCap { max_percent: *max_percent, days: *days }
            }
            R::Raiders { percent } => {
                ctx.problems.check((10..=250).contains(percent), loc, || "Raiders: percent must be 10..=250".into());
                E::Raiders { percent: *percent }
            }
            R::Alarm { hours } => {
                ctx.problems.check((1..=48).contains(hours), loc, || "Alarm: hours must be 1..=48".into());
                E::Alarm { hours: *hours }
            }
            R::Muster { hours } => {
                ctx.problems.check((1..=48).contains(hours), loc, || "Muster: hours must be 1..=48".into());
                E::Muster { hours: *hours }
            }
        })
    }
}

impl RawEvent {
    pub fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, id: EventDefId) -> EventDef {
        ctx.problems.check(!self.title.trim().is_empty() && !self.text.trim().is_empty(), loc, || {
            "title and text must not be empty".into()
        });
        ctx.problems.check(self.weight > 0, loc, || "weight must be positive".into());
        ctx.problems.check(!self.choices.is_empty(), loc, || "an event needs at least one choice".into());
        let conditions = self.conditions.iter().filter_map(|c| c.resolve(ctx, loc)).collect();
        let mut choices = Vec::with_capacity(self.choices.len());
        for raw in &self.choices {
            ctx.problems.check(!raw.label.trim().is_empty(), loc, || "choice label must not be empty".into());
            choices.push(EventChoice {
                label: raw.label.clone(),
                requires: raw.requires.iter().filter_map(|c| c.resolve(ctx, loc)).collect(),
                effects: raw.effects.iter().filter_map(|e| e.resolve(ctx, loc)).collect(),
            });
        }
        if let Some(first) = choices.first() {
            ctx.problems.check(first.requires.is_empty(), loc, || {
                "the first choice is the default and must not have requirements".into()
            });
        }
        if let Some(text) = &self.focus_text {
            // Only events whose default choice damages buildings start at one (the focus).
            let has_focus = choices.first().is_some_and(|c| {
                c.effects
                    .iter()
                    .any(|e| matches!(e, EventEffect::DamageBuildings { filter, .. } if *filter != BuildingFilter::Fields))
            });
            ctx.problems.check(has_focus, loc, || "focus_text needs a first choice that damages buildings".into());
            ctx.problems.check(text.matches("{building}").count() == 1, loc, || {
                "focus_text must name {building} exactly once".into()
            });
        }
        EventDef {
            id,
            key: self.id.clone(),
            title: self.title.clone(),
            text: self.text.clone(),
            focus_text: self.focus_text.clone(),
            severity: self.severity,
            conditions,
            weight: self.weight,
            cooldown_days: self.cooldown_days,
            risk: self.risk,
            choices,
        }
    }
}
