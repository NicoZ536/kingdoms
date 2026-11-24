//! Crop definitions (`data/crops.ron`, docs/game_design.md §5.3 and §8).

use serde::Deserialize;

use super::error::Loc;
use super::ids::{CropId, ResourceId};
use super::loader::Ctx;
use crate::time::Season;

/// A field crop or orchard tree.
#[derive(Clone, Debug, PartialEq)]
pub struct CropDef {
    /// Typed id.
    pub id: CropId,
    /// Canonical string id.
    pub key: String,
    /// German display name.
    pub name: String,
    /// Harvested resource.
    pub output: ResourceId,
    /// Units harvested per field tile at full fertility.
    pub yield_per_tile: f32,
    /// Growth needed until ripe (perennials: until the trees bear fruit), in days at growth
    /// factor 1.0 (season, soil, moisture and weather modify the hourly growth).
    pub growth_days: f32,
    /// Perennials: fruit growth since the last harvest needed for a full crop, in days at
    /// growth factor 1.0 (0 for annuals).
    pub fruit_days: f32,
    /// Perennial plants (orchards) survive winter and yield every harvest season once mature.
    pub perennial: bool,
    /// Seasons in which the crop can be sown (annuals) or planted (perennials).
    pub sow_seasons: Vec<Season>,
    /// Seasons in which mature perennials are harvested (empty for annuals: harvest when ripe).
    pub harvest_seasons: Vec<Season>,
    /// Frost (winter) kills unripe plants.
    pub frost_kills: bool,
    /// Farm hours (at full staffing) to sow or plant one field tile.
    pub sow_hours: f32,
    /// Farm hours (at full staffing) to harvest one field tile.
    pub harvest_hours: f32,
    /// Soil fertility (0–255) one harvest takes from the field.
    pub soil_exhaustion: u8,
    /// Sprite prefix in the `fields` atlas (`<sprite>_<0..4>`).
    pub sprite: String,
}

impl CropDef {
    /// Sprite for a growth stage 0–4.
    pub fn stage_sprite(&self, stage: u8) -> String {
        format!("fields/{}_{}", self.sprite, stage.min(4))
    }

    /// True if the crop may be sown or planted in `season`.
    pub fn can_sow_in(&self, season: Season) -> bool {
        self.sow_seasons.contains(&season)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawCrop {
    pub id: String,
    pub name: String,
    pub output: String,
    pub yield_per_tile: f32,
    pub growth_days: f32,
    #[serde(default)]
    pub fruit_days: f32,
    #[serde(default)]
    pub perennial: bool,
    #[serde(default)]
    pub sow_seasons: Vec<Season>,
    #[serde(default)]
    pub harvest_seasons: Vec<Season>,
    #[serde(default)]
    pub frost_kills: bool,
    pub sow_hours: f32,
    pub harvest_hours: f32,
    #[serde(default)]
    pub soil_exhaustion: u8,
    pub sprite: String,
}

impl RawCrop {
    pub fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, id: CropId) -> Option<CropDef> {
        let p = &mut ctx.problems;
        p.check(!self.name.trim().is_empty(), loc, || "name must not be empty".into());
        p.check(self.yield_per_tile.is_finite() && self.yield_per_tile > 0.0, loc, || {
            "yield_per_tile must be positive".into()
        });
        p.check(self.growth_days.is_finite() && self.growth_days > 0.0, loc, || "growth_days must be positive".into());
        p.check(!self.sprite.is_empty(), loc, || "sprite must not be empty".into());
        p.check(!self.sow_seasons.is_empty(), loc, || "crops need sow_seasons".into());
        p.check(!self.sow_seasons.contains(&Season::Winter), loc, || "nothing can be sown in winter".into());
        for (what, hours) in [("sow_hours", self.sow_hours), ("harvest_hours", self.harvest_hours)] {
            p.check(hours.is_finite() && hours > 0.0, loc, || format!("{what} must be positive"));
        }
        if self.perennial {
            p.check(!self.harvest_seasons.is_empty(), loc, || "perennial crops need harvest_seasons".into());
            p.check(!self.harvest_seasons.contains(&Season::Winter), loc, || "fruit cannot ripen in winter".into());
            p.check(self.fruit_days.is_finite() && self.fruit_days > 0.0, loc, || {
                "perennial crops need positive fruit_days".into()
            });
            p.check(!self.frost_kills, loc, || "perennial crops survive frost; remove frost_kills".into());
        } else {
            p.check(self.harvest_seasons.is_empty(), loc, || {
                "annual crops are harvested when ripe; remove harvest_seasons".into()
            });
            p.check(self.fruit_days == 0.0, loc, || "fruit_days is only for perennial crops".into());
        }
        let output = ctx.resource(loc, &self.output, "output")?;
        Some(CropDef {
            id,
            key: self.id.clone(),
            name: self.name.clone(),
            output,
            yield_per_tile: self.yield_per_tile,
            growth_days: self.growth_days,
            fruit_days: self.fruit_days,
            perennial: self.perennial,
            sow_seasons: self.sow_seasons.clone(),
            harvest_seasons: self.harvest_seasons.clone(),
            frost_kills: self.frost_kills,
            sow_hours: self.sow_hours,
            harvest_hours: self.harvest_hours,
            soil_exhaustion: self.soil_exhaustion,
            sprite: self.sprite.clone(),
        })
    }
}
