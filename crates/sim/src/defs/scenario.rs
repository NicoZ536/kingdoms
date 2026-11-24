//! Scenario definitions (`data/scenarios/*.ron`, docs/game_design.md §13).

use std::collections::BTreeMap;

use glam::IVec2;
use serde::Deserialize;

use super::common::ResourceAmount;
use crate::grid::Rotation;
use super::error::Loc;
use super::ids::{BuildingTypeId, ScenarioId, TechId};
use super::loader::Ctx;

/// A building placed at game start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartBuilding {
    /// Building type.
    pub building: BuildingTypeId,
    /// Preferred position of the footprint origin relative to the world's start point. If the
    /// spot is blocked, the nearest valid spot is used.
    pub offset: IVec2,
    /// Orientation (front/entrance side).
    pub rotation: Rotation,
}

/// Starting conditions of a game.
#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioDef {
    /// Typed id.
    pub id: ScenarioId,
    /// Canonical string id (file name without extension).
    pub key: String,
    /// German display name.
    pub name: String,
    /// German description.
    pub description: String,
    /// Buildings placed at the start (the first one is the headquarters).
    pub start_buildings: Vec<StartBuilding>,
    /// Number of settlers.
    pub settlers: u16,
    /// Number of families the settlers form.
    pub families: u16,
    /// Initial stock in the headquarters' storage.
    pub stock: Vec<ResourceAmount>,
    /// Initial treasury.
    pub gold: u32,
    /// Technologies researched at the start.
    pub researched: Vec<TechId>,
    /// Initial tax rate in percent.
    pub tax_rate_percent: u8,
    /// Most AI kingdoms (0–7; the map size decides how many there are up to this limit,
    /// `rules.ron` → `diplomacy.kingdoms`; 0: no kingdoms, trade orders use the abstract world
    /// market).
    pub rival_kingdoms: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawStartBuilding {
    pub building: String,
    pub offset: (i32, i32),
    /// Quarter turns 0–3 (0 = entrance faces south).
    #[serde(default)]
    pub rotation: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawScenario {
    /// Must equal the file name without extension.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub start_buildings: Vec<RawStartBuilding>,
    pub settlers: u16,
    pub families: u16,
    #[serde(default)]
    pub stock: BTreeMap<String, u32>,
    pub gold: u32,
    #[serde(default)]
    pub researched: Vec<String>,
    pub tax_rate_percent: u8,
    pub rival_kingdoms: u8,
}

impl RawScenario {
    pub fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, id: ScenarioId, max_tax: u8) -> ScenarioDef {
        let p = &mut ctx.problems;
        p.check(!self.name.trim().is_empty(), loc, || "name must not be empty".into());
        p.check(!self.start_buildings.is_empty(), loc, || {
            "at least one start building (the headquarters) is required".into()
        });
        p.check(self.families > 0 && self.settlers >= self.families * 2, loc, || {
            "families must be positive and each family needs at least two settlers".into()
        });
        p.check(self.tax_rate_percent <= max_tax, loc, || format!("tax_rate_percent exceeds the maximum of {max_tax}"));
        p.check(self.rival_kingdoms <= 7, loc, || "rival_kingdoms must be 0..=7".into());
        for s in &self.start_buildings {
            ctx.problems.check(Rotation::new(s.rotation).is_some(), loc, || {
                format!("start building '{}': rotation {} outside 0..=3", s.building, s.rotation)
            });
        }
        let start_buildings = self
            .start_buildings
            .iter()
            .filter_map(|s| {
                ctx.building(loc, &s.building, "start_buildings").map(|building| StartBuilding {
                    building,
                    offset: IVec2::new(s.offset.0, s.offset.1),
                    rotation: Rotation::wrapping(s.rotation),
                })
            })
            .collect();
        let mut stock = Vec::with_capacity(self.stock.len());
        for (key, &amount) in &self.stock {
            ctx.problems.check(amount > 0, loc, || format!("stock of '{key}' must be positive"));
            if let Some(resource) = ctx.resource(loc, key, "stock") {
                stock.push(ResourceAmount { resource, amount });
            }
        }
        stock.sort_by_key(|s| s.resource);
        let researched = self.researched.iter().filter_map(|k| ctx.tech(loc, k, "researched")).collect();
        ScenarioDef {
            id,
            key: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            start_buildings,
            settlers: self.settlers,
            families: self.families,
            stock,
            gold: self.gold,
            researched,
            tax_rate_percent: self.tax_rate_percent,
            rival_kingdoms: self.rival_kingdoms,
        }
    }
}
