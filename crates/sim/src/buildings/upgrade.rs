//! Building upgrades: requirement checks (docs/game_design.md §5).
//!
//! Every building type with an `upgrade` in the data can be upgraded to its next level, paying
//! the upgrade cost (default: the target's construction cost) once the target type is
//! researched; the building keeps its id, workers, stock, production buffers, recipes and
//! orders (the data loader checks that the target is compatible).
//!
//! Houses additionally need the requirements of the *target* tier from the data
//! (`housing.requires`: needs, services, goods), judged from the house's residents
//! ([`crate::population::home_requirements`]):
//! * services: an operational, staffed building providing the service reaches the house
//!   (Chebyshev distance between footprints ≤ service radius),
//! * needs: the residents' average need value reaches `PopulationRules::need_met_threshold`,
//! * goods: every household of the house has one of the accepted goods in its pantry
//!   (supplied by a market).
//!
//! A house without residents cannot be upgraded.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{BuildingId, BuildingPhase, PlacementOptions, check_placement_with};
use crate::defs::{BuildingDef, BuildingTypeId, Defs, NeedKind, ServiceKind, UpgradeDef};
use crate::economy::Shortfall;
use crate::grid::TileRect;
use crate::state::GameState;

/// Distance within which a river or lake satisfies thirst (docs/game_design.md §6).
pub const FRESH_WATER_RADIUS: u8 = 8;

/// Why a building cannot be upgraded now. The `Display` text is German.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum UpgradeBlocker {
    /// Automatic upgrades exist only for houses (`SetUpgradeLocked` on another building).
    #[error("Nur Wohnhäuser werden automatisch ausgebaut.")]
    NotHousing,
    /// Highest level reached (the type is the end of an upgrade chain).
    #[error("Das Gebäude hat bereits die höchste Ausbaustufe.")]
    MaxTier,
    /// The type has no upgrade at all.
    #[error("Dieses Gebäude kann nicht ausgebaut werden.")]
    NoUpgrade,
    /// Under construction, being moved or repaired, already upgrading or paused.
    #[error("Nur fertige, nicht stillgelegte Gebäude können ausgebaut werden.")]
    NotReady,
    /// The target type may exist only so many times (castle).
    #[error("Von „{name}“ kann es höchstens {max} geben.")]
    MaxCountReached {
        /// German name of the target type.
        name: String,
        /// Allowed maximum.
        max: u16,
    },
    /// The next tier needs research.
    #[error("Das Gebäude „{name}“ ist noch nicht verfügbar – benötigt die Forschung „{tech}“.")]
    NotUnlocked {
        /// German name of the next tier.
        name: String,
        /// German technology name.
        tech: String,
    },
    /// Nobody lives in the house.
    #[error("Das Haus ist unbewohnt.")]
    NoResidents,
    /// A required service does not reach the house.
    #[error("Es fehlt die Versorgung: {}.", .service.display_name())]
    MissingService {
        /// The service.
        service: ServiceKind,
    },
    /// A required need is not satisfied.
    #[error("Bedürfnis nicht erfüllt: {}.", .need.display_name())]
    UnmetNeed {
        /// The need.
        need: NeedKind,
    },
    /// Required goods are not available.
    #[error("Es fehlen Waren: {names}.")]
    MissingGoods {
        /// German names of the missing goods ("A oder B").
        names: String,
    },
    /// The larger footprint of the next tier does not fit.
    #[error("Für den Ausbau ist nicht genug Platz.")]
    NoSpace,
    /// Not enough gold.
    #[error("Nicht genug Gold ({available} von {needed}).")]
    InsufficientGold {
        /// Required.
        needed: u32,
        /// In the treasury.
        available: i64,
    },
    /// The treasury is bankrupt: no upgrades.
    #[error("Die Staatskasse ist bankrott – es kann nicht ausgebaut werden.")]
    Bankrupt,
    /// Not enough of a material.
    #[error("Nicht genug {name} ({available} von {needed}).")]
    InsufficientMaterial {
        /// German resource name.
        name: String,
        /// Required.
        needed: u32,
        /// In stock.
        available: u32,
    },
}

/// True if an operational building providing `service` reaches `rect`.
pub(crate) fn service_reaches(state: &GameState, defs: &Defs, service: ServiceKind, rect: TileRect) -> bool {
    service_reaches_without(state, defs, service, rect, None)
}

/// [`service_reaches`] not counting building `except`.
fn service_reaches_without(state: &GameState, defs: &Defs, service: ServiceKind, rect: TileRect, except: Option<BuildingId>) -> bool {
    state.buildings.iter().filter(|(id, _)| Some(*id) != except).any(|(_, b)| {
        let Some(def) = defs.buildings().get(b.kind().index()) else { return false };
        let Some(radius) = state.research.service_radius(defs, def, service) else { return false };
        b.is_operational()
            && (def.workers == 0 || b.trade_staff() > 0)
            && b.rect().distance_to_rect(&rect) <= i32::from(radius)
    })
}

/// A service that inhabited houses lose while a building is upgraded (§5: its services pause
/// meanwhile) because no other operational building of that service reaches them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceGap {
    /// The service.
    pub service: ServiceKind,
    /// Inhabited houses only this building serves.
    pub houses: u32,
}

impl ServiceGap {
    /// German warning („Während des Ausbaus fehlt 12 Häusern die Versorgung „Markt“.“).
    pub fn text(&self) -> String {
        let who = if self.houses == 1 { "einem Haus".to_owned() } else { format!("{} Häusern", self.houses) };
        format!("Während des Ausbaus fehlt {who} die Versorgung „{}“.", self.service.display_name())
    }
}

/// The services inhabited houses would lose while `building` is upgraded (empty: every house
/// it serves is also reached by another building of the same service, or it serves none).
pub fn upgrade_service_gaps(state: &GameState, defs: &Defs, building: BuildingId) -> Vec<ServiceGap> {
    let Some(b) = state.buildings.get(building) else { return Vec::new() };
    let Some(def) = defs.buildings().get(b.kind().index()) else { return Vec::new() };
    if !b.is_operational() || (def.workers > 0 && b.trade_staff() == 0) {
        return Vec::new();
    }
    let houses: Vec<TileRect> = state
        .buildings
        .iter()
        .filter(|(id, h)| h.housing_tier(defs).is_some() && state.population.households().any(|(_, hh)| hh.home() == Some(*id)))
        .map(|(_, h)| h.rect())
        .collect();
    let mut gaps = Vec::new();
    for service in def.services.iter().map(|s| s.kind) {
        let Some(radius) = state.research.service_radius(defs, def, service) else { continue };
        let lost = houses
            .iter()
            .filter(|rect| b.rect().distance_to_rect(rect) <= i32::from(radius))
            .filter(|rect| !service_reaches_without(state, defs, service, **rect, Some(building)))
            .count();
        if lost > 0 && !gaps.iter().any(|g: &ServiceGap| g.service == service) {
            gaps.push(ServiceGap { service, houses: u32::try_from(lost).unwrap_or(u32::MAX) });
        }
    }
    gaps
}

/// The next level of building type `kind`: its upgrade (cost, hours) and the target type.
pub fn upgrade_target(defs: &Defs, kind: BuildingTypeId) -> Option<(&UpgradeDef, &BuildingDef)> {
    let up = defs.buildings().get(kind.index())?.upgrade.as_ref()?;
    Some((up, defs.buildings().get(up.to.index())?))
}

/// Everything that prevents upgrading `building` now (empty = the upgrade can start).
/// `None` if the building does not exist.
pub fn upgrade_blockers(state: &GameState, defs: &Defs, building: BuildingId) -> Option<Vec<UpgradeBlocker>> {
    let b = state.buildings.get(building)?;
    let def = defs.buildings().get(b.kind().index())?;
    let Some((upgrade, target)) = upgrade_target(defs, def.id) else {
        // The end of a chain has the highest level; other types have none.
        let in_chain = def.housing.is_some() || defs.buildings().iter().any(|o| o.upgrade.as_ref().is_some_and(|u| u.to == def.id));
        return Some(vec![if in_chain { UpgradeBlocker::MaxTier } else { UpgradeBlocker::NoUpgrade }]);
    };
    let target_id = target.id;
    let mut out = Vec::new();
    if !matches!(b.phase(), BuildingPhase::Active) || b.is_paused() {
        out.push(UpgradeBlocker::NotReady);
    }
    if let Some(tech) = target.unlocked_by.filter(|t| !state.research.is_researched(*t)) {
        let tech = defs.techs().get(tech.index()).map_or_else(|| tech.to_string(), |t| t.name.clone());
        out.push(UpgradeBlocker::NotUnlocked { name: target.name.clone(), tech });
    }
    if let Some(max) = target.max_count()
        && state.buildings.count_of(target_id) >= usize::from(max)
    {
        out.push(UpgradeBlocker::MaxCountReached { name: target.name.clone(), max });
    }
    if let Some(status) = crate::population::home_requirements(state, defs, building, target_id) {
        if status.residents == 0 {
            out.push(UpgradeBlocker::NoResidents);
        }
        for s in status.services.iter().filter(|s| !s.met) {
            out.push(UpgradeBlocker::MissingService { service: s.what });
        }
        for n in status.needs.iter().filter(|n| status.residents > 0 && !n.met) {
            out.push(UpgradeBlocker::UnmetNeed { need: n.need });
        }
        let missing: Vec<String> = status
            .goods
            .iter()
            .filter(|g| !g.met)
            .map(|g| {
                g.what
                    .iter()
                    .filter_map(|r| defs.resources().get(r.index()).map(|d| d.name.clone()))
                    .collect::<Vec<_>>()
                    .join(" oder ")
            })
            .collect();
        if !missing.is_empty() {
            out.push(UpgradeBlocker::MissingGoods { names: missing.join(", ") });
        }
    }
    if target.size != def.size {
        let origin = b.rotation().grown_origin(b.origin(), def.size, target.size);
        let options =
            PlacementOptions { ignore: Some(building), skip_cost: true, skip_unlock: true, ..Default::default() };
        let check = check_placement_with(state, defs, target_id, origin, b.rotation(), options);
        if !check.ok {
            out.push(UpgradeBlocker::NoSpace);
        }
    }
    if state.economy.is_bankrupt() {
        out.push(UpgradeBlocker::Bankrupt);
    }
    for s in state.economy.shortfalls(&upgrade.cost) {
        out.push(match s {
            Shortfall::Gold { needed, available } => UpgradeBlocker::InsufficientGold { needed, available },
            Shortfall::Material { resource, needed, available } => UpgradeBlocker::InsufficientMaterial {
                name: defs.resources().get(resource.index()).map_or_else(|| resource.to_string(), |r| r.name.clone()),
                needed,
                available,
            },
        });
    }
    Some(out)
}
