//! Road definitions (`data/roads.ron`, docs/game_design.md §5.6).

use serde::Deserialize;

use super::common::{Cost, RawCost};
use super::error::Loc;
use super::ids::{RoadTypeId, TechId};
use super::loader::Ctx;

/// Bridge properties of a road type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeDef {
    /// Maximum number of consecutive water tiles.
    pub max_span: u8,
    /// Only shallow water can be bridged.
    pub shallow_water_only: bool,
}

/// A road type. Roads auto-connect via a 4-neighbour bitmask (N=1, E=2, S=4, W=8).
#[derive(Clone, Debug, PartialEq)]
pub struct RoadDef {
    /// Typed id.
    pub id: RoadTypeId,
    /// Canonical string id; also the sprite prefix (`roads/<key>_<mask>`).
    pub key: String,
    /// German display name.
    pub name: String,
    /// Cost per tile (per water tile for bridges).
    pub cost_per_tile: Cost,
    /// Walking speed multiplier relative to the base walking speed.
    pub speed: f32,
    /// Bonus on trade income for caravans using the road.
    pub trade_bonus_percent: u16,
    /// Present for bridges.
    pub bridge: Option<BridgeDef>,
    /// Technology unlocking the type; `None` = available from the start (derived from techs).
    pub unlocked_by: Option<TechId>,
}

impl RoadDef {
    /// Sprite for a connection bitmask (0–15).
    pub fn sprite(&self, mask: u8) -> String {
        format!("roads/{}_{}", self.key, mask & 15)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRoad {
    pub id: String,
    pub name: String,
    pub cost: RawCost,
    pub speed: f32,
    #[serde(default)]
    pub trade_bonus_percent: u16,
    #[serde(default)]
    pub bridge: Option<BridgeDef>,
}

impl RawRoad {
    pub fn resolve(&self, ctx: &mut Ctx, loc: &Loc<'_>, id: RoadTypeId) -> RoadDef {
        ctx.problems.check(!self.name.trim().is_empty(), loc, || "name must not be empty".into());
        ctx.problems.check(self.speed.is_finite() && self.speed >= 1.0, loc, || "speed must be >= 1.0".into());
        if let Some(b) = &self.bridge {
            ctx.problems.check(b.max_span > 0, loc, || "bridge max_span must be positive".into());
        }
        let cost_per_tile = self.cost.resolve(ctx, loc, "cost");
        RoadDef {
            id,
            key: self.id.clone(),
            name: self.name.clone(),
            cost_per_tile,
            speed: self.speed,
            trade_bonus_percent: self.trade_bonus_percent,
            bridge: self.bridge,
            unlocked_by: None,
        }
    }
}
