//! Military unit definitions (`data/units.ron`, docs/game_design.md §11).

use serde::Deserialize;

use super::common::{Cost, RawCost};
use super::error::Loc;
use super::ids::{BuildingTypeId, TechId, UnitTypeId};
use super::loader::Ctx;
use super::military::{ClassBonus, UnitClass};

/// Extra support a unit type needs (cavalry needs horses from cattle farms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitSupport {
    /// Building providing the support.
    pub building: BuildingTypeId,
    /// Units supported per finished building.
    pub units_per_building: u16,
}

/// A military unit type.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitDef {
    /// Typed id.
    pub id: UnitTypeId,
    /// Canonical string id.
    pub key: String,
    /// German display name.
    pub name: String,
    /// German plural of [`UnitDef::name`] („Bogenschützen“, „Reiter“).
    pub plural: String,
    /// German description.
    pub description: String,
    /// Class (sprite role, combat behaviour).
    pub class: UnitClass,
    /// Melee attack.
    pub attack: u16,
    /// Defense.
    pub defense: u16,
    /// Ranged attack (0 = melee only).
    pub ranged: u16,
    /// Range of the ranged attack in tiles (0 for melee units).
    pub range: u8,
    /// Hit points of one unit.
    pub hit_points: u16,
    /// Extra attack against a unit class.
    pub bonus: Option<ClassBonus>,
    /// Damage against walls and buildings per combat round.
    pub building_damage: u16,
    /// Movement speed multiplier (1.0 = citizen walking speed).
    pub speed: f32,
    /// [`UnitDef::speed`] in permille (integer, for the state).
    pub speed_permille: u16,
    /// Morale (0–100): units with low morale flee earlier.
    pub morale: u16,
    /// Recruitment cost.
    pub cost: Cost,
    /// Citizens leaving the workforce per unit.
    pub population: u8,
    /// Pay in gold per day.
    pub upkeep_per_day: u32,
    /// Training time in game hours.
    pub training_hours: f32,
    /// [`UnitDef::training_hours`] in ticks (integer, for the state).
    pub training_ticks: u64,
    /// Building that trains the unit.
    pub trained_at: BuildingTypeId,
    /// Additional support requirement.
    pub support: Option<UnitSupport>,
    /// Technology unlocking the type; `None` = available from the start (derived from techs).
    pub unlocked_by: Option<TechId>,
}

fn default_hit_points() -> u16 {
    100
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawUnitSupport {
    pub building: String,
    pub units_per_building: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawUnit {
    pub id: String,
    pub name: String,
    pub plural: String,
    #[serde(default)]
    pub description: String,
    pub class: UnitClass,
    pub attack: u16,
    pub defense: u16,
    #[serde(default)]
    pub ranged: u16,
    #[serde(default)]
    pub range: u8,
    #[serde(default = "default_hit_points")]
    pub hit_points: u16,
    #[serde(default)]
    pub bonus: Option<ClassBonus>,
    #[serde(default)]
    pub building_damage: u16,
    pub speed: f32,
    pub morale: u16,
    pub cost: RawCost,
    pub population: u8,
    pub upkeep: u32,
    pub training_hours: f32,
    pub trained_at: String,
    #[serde(default)]
    pub support: Option<RawUnitSupport>,
}

impl RawUnit {
    pub fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, id: UnitTypeId) -> Option<UnitDef> {
        let p = &mut ctx.problems;
        p.check(!self.name.trim().is_empty(), loc, || "name must not be empty".into());
        p.check(!self.plural.trim().is_empty(), loc, || "plural must not be empty".into());
        p.check(self.attack > 0 || self.ranged > 0, loc, || "unit needs attack or ranged strength".into());
        p.check(self.speed.is_finite() && self.speed > 0.0, loc, || "speed must be positive".into());
        p.check((1..=100).contains(&self.morale), loc, || "morale must be 1..=100".into());
        p.check(self.population > 0, loc, || "population must be at least 1".into());
        p.check(self.hit_points > 0, loc, || "hit_points must be positive".into());
        p.check((self.ranged > 0) == (self.range > 0) && self.range <= 16, loc, || {
            "ranged units need a range of 1..=16 tiles, melee units none".into()
        });
        p.check(self.speed <= 4.0, loc, || "speed must be at most 4.0".into());
        p.check(self.training_hours <= 500.0, loc, || "training_hours must be at most 500".into());
        p.check(self.training_hours.is_finite() && self.training_hours > 0.0, loc, || {
            "training_hours must be positive".into()
        });
        let cost = self.cost.resolve(ctx, loc, "cost");
        let support = match &self.support {
            Some(s) => {
                ctx.problems
                    .check(s.units_per_building > 0, loc, || "support units_per_building must be positive".into());
                ctx.building(loc, &s.building, "support")
                    .map(|building| UnitSupport { building, units_per_building: s.units_per_building })
            }
            None => None,
        };
        let trained_at = ctx.building(loc, &self.trained_at, "trained_at")?;
        Some(UnitDef {
            id,
            key: self.id.clone(),
            name: self.name.clone(),
            plural: self.plural.clone(),
            description: self.description.clone(),
            class: self.class,
            attack: self.attack,
            defense: self.defense,
            ranged: self.ranged,
            range: self.range,
            hit_points: self.hit_points,
            bonus: self.bonus,
            building_damage: self.building_damage,
            speed: self.speed,
            speed_permille: (f64::from(self.speed) * 1000.0).round().clamp(1.0, 4000.0) as u16,
            morale: self.morale,
            cost,
            population: self.population,
            upkeep_per_day: self.upkeep,
            training_hours: self.training_hours,
            training_ticks: (f64::from(self.training_hours) * crate::time::TICKS_PER_HOUR as f64).round().max(1.0) as u64,
            trained_at,
            support,
            unlocked_by: None,
        })
    }
}
