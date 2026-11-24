//! Construction: placing sites, applying builder labour, completion, cancellation, demolition,
//! upgrades (manual and automatic), moves and pausing. Pipeline step 3 ([`run_construction`]).

use glam::IVec2;
use serde::{Deserialize, Serialize};

use super::placement::shortfall_error;
use super::{
    Building, BuildingId, BuildingPhase, ConstructionSite, DEMOLISH_REFUND_PERCENT, MOVE_WORK_PERCENT, Rotation,
    SitePurpose, UpgradeProject, check_move, check_placement, clear_vegetation, merge_amounts, upgrade_blockers,
};
use crate::command::{CommandResult, RejectReason};
use crate::defs::{BuildingDef, BuildingRole, BuildingTypeId, DamageCause, Defs, GameRules, ResourceAmount};
use crate::economy::{LedgerCategory, storage};
use crate::event::{Notification, Severity, SimEvent};
use crate::grid::TileRect;
use crate::production::ProductionStatus;
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;
use crate::workforce;

/// Work points one builder contributes per tick at normal speed.
pub const WORK_PER_BUILDER_TICK: u32 = 100;

/// Result of the crate-internal `apply_construction_work` (the single entry point for builder
/// labour).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkOutcome {
    /// Work points actually applied (limited by the remaining work and delivered materials).
    pub applied: u32,
    /// True if this work completed the site.
    pub completed: bool,
}

/// Work points per builder and tick now: [`WORK_PER_BUILDER_TICK`] × winter work speed
/// (`GameRules::winter_work_speed_percent`) × the weather's outdoor work speed × researched
/// build speed bonuses.
pub fn work_per_builder_tick(state: &GameState, defs: &Defs) -> u32 {
    let season = i64::from(crate::weather::construction_percent(state, defs));
    let bonus = i64::from(state.research.build_speed_bonus_percent(defs).max(-90));
    let work = i64::from(WORK_PER_BUILDER_TICK) * season * (100 + bonus) / 10_000;
    work.clamp(1, i64::from(u32::MAX)) as u32
}

/// Work points for `build_hours` of one builder, scaled by `percent` (100 = full build time)
/// and the clearing surcharge for `cleared_tiles`.
pub(crate) fn work_required(build_hours: f32, percent: u32, cleared_tiles: u32, rules: &GameRules) -> u32 {
    let base = f64::from(build_hours) * TICKS_PER_HOUR as f64 * f64::from(WORK_PER_BUILDER_TICK);
    let clearing = 100.0 + f64::from(rules.clearing_time_percent_per_tile) * f64::from(cleared_tiles);
    let total = base * f64::from(percent) / 100.0 * clearing / 100.0;
    total.round().clamp(1.0, f64::from(u32::MAX)) as u32
}

fn building_name(defs: &Defs, kind: BuildingTypeId) -> String {
    defs.buildings().get(kind.index()).map_or_else(|| kind.to_string(), |d| d.name.clone())
}

/// Recomputes the territory and reports a change.
fn refresh_territory(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    if state.territory.recompute(&state.buildings, defs, &state.diplomacy.land) {
        events.push(SimEvent::TerritoryChanged { owned_tiles: state.territory.owned_tiles() });
    }
}

fn claims_territory(defs: &Defs, kind: BuildingTypeId) -> bool {
    defs.buildings().get(kind.index()).is_some_and(|d| d.territory_radius > 0)
}

/// Adds construction work to a site (new building, move or upgrade) and completes it when done.
///
/// This is the single entry point for labour: the construction step calls it with the work of
/// the builder citizens assigned to the site. Paused
/// buildings take no work. Returns `None` if the building does not exist or has no site.
pub(crate) fn apply_construction_work(
    state: &mut GameState,
    defs: &Defs,
    building: BuildingId,
    work: u32,
    events: &mut Vec<SimEvent>,
) -> Option<WorkOutcome> {
    let now = state.clock.tick;
    let b = state.buildings.get_mut(building)?;
    let kind = b.kind;
    let paused = b.paused;
    let site = b.construction_site_mut()?;
    if paused {
        return Some(WorkOutcome { applied: 0, completed: false });
    }
    let was_started = site.started;
    let applied = site.add_work(work);
    if applied > 0 {
        site.mark_worked(now);
    }
    let completed = site.is_complete();
    let stalled = !completed && !site.can_progress();
    if !was_started && site.started {
        events.push(SimEvent::ConstructionStarted { building, kind });
    }
    if completed {
        finish(state, defs, building, events);
    } else if stalled {
        // Out of delivered material: the builders are released until more arrives.
        state.workforce.mark_dirty();
    }
    Some(WorkOutcome { applied, completed })
}

/// What happens at a construction site right now (renderers: dust and sounds; info panels).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteWork {
    /// Builders added work in the last tick.
    Working,
    /// The player paused the site.
    Paused,
    /// The delivered materials allow no more work.
    WaitingForMaterials,
    /// No builders are assigned.
    NoBuilders,
    /// The builders have their lunch break.
    LunchBreak,
    /// Working time, but the builders are not at the site yet.
    BuildersOnTheWay,
    /// Outside the working hours.
    OffShift,
}

impl SiteWork {
    /// German status line.
    pub fn text(self) -> &'static str {
        match self {
            SiteWork::Working => "Bauarbeiter bei der Arbeit",
            SiteWork::Paused => "Bau angehalten",
            SiteWork::WaitingForMaterials => "Wartet auf Baumaterial",
            SiteWork::NoBuilders => "Keine Bauarbeiter",
            SiteWork::LunchBreak => "Bauarbeiter in der Mittagspause",
            SiteWork::BuildersOnTheWay => "Bauarbeiter auf dem Weg",
            SiteWork::OffShift => "Bauarbeiter haben Feierabend",
        }
    }
}

/// What happens at the construction site (new building, move, upgrade or repair) of
/// `building` (`None`: no site).
pub fn site_work(state: &GameState, defs: &Defs, building: BuildingId) -> Option<SiteWork> {
    let b = state.buildings.get(building)?;
    let site = b.construction_site()?;
    let status = if site.progressed_at(state.clock.tick) {
        SiteWork::Working
    } else if b.is_paused() {
        SiteWork::Paused
    } else if !site.can_progress() {
        SiteWork::WaitingForMaterials
    } else if b.staffed() == 0 {
        SiteWork::NoBuilders
    } else {
        match crate::population::absence_status(state, defs, building) {
            ProductionStatus::LunchBreak => SiteWork::LunchBreak,
            ProductionStatus::OffShift => SiteWork::OffShift,
            _ => SiteWork::BuildersOnTheWay,
        }
    };
    Some(status)
}

/// Turns a completed site into an active building.
fn finish(state: &mut GameState, defs: &Defs, id: BuildingId, events: &mut Vec<SimEvent>) {
    let Some(b) = state.buildings.get_mut(id) else { return };
    let position = b.entrance;
    // An upgrade from a claiming type changes the claim even if the target claimed nothing.
    let mut refresh_anyway = false;
    match std::mem::replace(&mut b.phase, BuildingPhase::Active) {
        BuildingPhase::Construction(site) => {
            let kind = b.kind;
            let name = building_name(defs, kind);
            if site.purpose == SitePurpose::Move {
                events.push(SimEvent::BuildingMoved { building: id, kind });
                events.push(SimEvent::Notification(
                    Notification::new(Severity::Info, format!("Umzug abgeschlossen: {name}")).at(position),
                ));
            } else if site.purpose == SitePurpose::Repair {
                events.push(SimEvent::BuildingRepaired { building: id, kind });
                events.push(SimEvent::Notification(
                    Notification::new(Severity::Good, format!("Wiederaufgebaut: {name}")).at(position),
                ));
            } else {
                events.push(SimEvent::BuildingCompleted { building: id, kind });
                events.push(SimEvent::Notification(
                    Notification::new(Severity::Good, format!("Fertiggestellt: {name}")).at(position),
                ));
            }
        }
        BuildingPhase::Upgrading(up) => {
            let from = b.kind;
            refresh_anyway = claims_territory(defs, from);
            b.kind = up.target;
            events.push(SimEvent::BuildingUpgraded { building: id, from, to: up.target });
            events.push(SimEvent::Notification(
                Notification::new(Severity::Good, format!("Ausgebaut: {}", building_name(defs, up.target))).at(position),
            ));
        }
        BuildingPhase::Active => return,
    }
    activate(state, defs, id);
    let kind = b_kind(state, id);
    state.workforce.mark_dirty();
    if refresh_anyway || kind.is_some_and(|k| claims_territory(defs, k)) {
        refresh_territory(state, defs, events);
    }
    storage::refresh_stock(state, defs);
}

fn b_kind(state: &GameState, id: BuildingId) -> Option<BuildingTypeId> {
    state.buildings.get(id).map(|b| b.kind)
}

/// Sets up storage capacities and production buffers of a building that just became active.
pub(crate) fn activate(state: &mut GameState, defs: &Defs, id: BuildingId) {
    state.buildings.note_activation();
    let bonus = storage::storage_bonus_percent(&state.research, defs);
    if let Some(b) = state.buildings.get_mut(id) {
        storage::init_storage(b, defs, bonus);
        crate::production::init_production(b, defs);
    }
}

/// Pipeline step 3: assigns workers/builders if needed, lets builders work on every staffed,
/// unpaused site (ascending id order), completes finished sites and, once per hour, starts the
/// automatic house upgrades ([`run_auto_upgrades`]).
pub(crate) fn run_construction(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    if state.workforce.needs_allocation() {
        workforce::allocate(state, defs);
    }
    let per_builder = u64::from(work_per_builder_tick(state, defs));
    let rules = &defs.rules().population;
    let sites: Vec<(BuildingId, u32)> = state
        .buildings
        .iter()
        .filter(|(_, b)| !b.paused && b.staffed > 0 && b.construction_site().is_some())
        .map(|(id, _)| {
            // Every builder at the site adds its share, weighted by health and happiness
            // (docs §6), at the shift factor (build times are round-the-clock times).
            let permille: u64 = state
                .population
                .workers_of(id)
                .iter()
                .filter_map(|c| state.population.citizen(*c))
                .filter(|c| crate::population::is_at_work(state, defs, c))
                .map(|c| u64::from(crate::population::work_efficiency_permille(rules, c)))
                .sum();
            let work = crate::production::shift_progress(defs, per_builder * permille) / 1000;
            (id, u32::try_from(work).unwrap_or(u32::MAX))
        })
        .collect();
    for (id, work) in sites {
        apply_construction_work(state, defs, id, work, events);
    }
    run_auto_upgrades(state, defs, events);
    if state.workforce.needs_allocation() {
        workforce::allocate(state, defs);
    }
}

/// Automatic house upgrades (docs/game_design.md §5.1), from the construction step once per
/// game hour for every house, staggered over the hour (simulation LOD: a house is checked in
/// the tick whose slot `tick mod 200` equals its id index mod 200): every finished, unpaused
/// house whose upgrades are not locked starts upgrading to its next tier as soon as nothing
/// blocks it (requirements, space, gold and materials available). Houses of one slot are
/// visited in ascending id order; every started upgrade pays its gold and commits its
/// materials before the next house is checked, so the stock is never promised twice.
pub(crate) fn run_auto_upgrades(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let slot = state.clock.tick % crate::time::TICKS_PER_HOUR;
    let candidates: Vec<BuildingId> = state
        .buildings
        .iter()
        .filter(|(id, _)| u64::from(id.index()) % crate::time::TICKS_PER_HOUR == slot)
        .filter(|(_, b)| matches!(b.phase, BuildingPhase::Active) && !b.paused && !b.upgrade_locked)
        .filter(|(_, b)| {
            defs.buildings().get(b.kind.index()).is_some_and(|d| d.housing.is_some() && d.upgrade.is_some())
        })
        .map(|(id, _)| id)
        .collect();
    for id in candidates {
        // A rejection only means the house is not ready yet; it is checked again next hour.
        if upgrade_building(state, defs, id, events).is_ok()
            && let Some(b) = state.buildings.get(id)
            && let BuildingPhase::Upgrading(up) = &b.phase
        {
            let text = format!("Ausbau begonnen: Aus „{}“ wird „{}“.", building_name(defs, b.kind), building_name(defs, up.target));
            events.push(SimEvent::Notification(Notification::new(Severity::Info, text).at(b.entrance)));
        }
    }
}

/// `SetUpgradeLocked`: locks or unlocks the automatic upgrades of a house.
pub(crate) fn set_upgrade_locked(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    locked: bool,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let b = state.buildings.get_mut(id).ok_or(RejectReason::UnknownBuilding)?;
    if defs.buildings().get(b.kind.index()).is_none_or(|d| d.housing.is_none()) {
        return Err(RejectReason::Upgrade(super::UpgradeBlocker::NotHousing));
    }
    if b.upgrade_locked != locked {
        b.upgrade_locked = locked;
        events.push(SimEvent::UpgradeLockChanged { building: id, locked });
    }
    Ok(())
}

/// `PlaceBuilding`: validates, pays the cost (gold and materials go to the site), clears the
/// footprint (wood to the stock) and creates a construction site.
pub(crate) fn place_building(
    state: &mut GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    origin: IVec2,
    rotation: Rotation,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let check = check_placement(state, defs, kind, origin, rotation);
    if let Some(e) = check.errors.first() {
        return Err(RejectReason::Placement(e.clone()));
    }
    let Some(def) = defs.buildings().get(kind.index()) else {
        return Err(RejectReason::Placement(super::PlacementError::UnknownBuildingType));
    };
    if let Err(missing) = state.economy.pay_gold(LedgerCategory::Construction, def.cost.gold) {
        return Err(RejectReason::Placement(shortfall_error(defs, missing)));
    }
    if let Some(wall) = check.replaces {
        remove_replaced_wall(state, defs, wall, events);
    }
    let work = work_required(def.build_hours, 100, check.clearing.tiles(), defs.rules());
    let site = ConstructionSite::new(SitePurpose::New, work, def.cost.gold, def.cost.materials.clone(), Vec::new());
    let id = match state.buildings.insert(Building::new(def, origin, rotation, BuildingPhase::Construction(site))) {
        Ok(id) => id,
        Err(_) => {
            state.economy.book_gold(LedgerCategory::Construction, i64::from(def.cost.gold));
            return Err(RejectReason::Placement(super::PlacementError::OverlapsBuilding));
        }
    };
    let cleared = clear_vegetation(&mut state.world, check.footprint);
    let wood = cleared.wood(defs.rules());
    events.push(SimEvent::BuildingPlaced { building: id, kind, origin, rotation, cleared_wood: wood });
    deposit_wood(state, defs, wood, origin, events);
    state.workforce.mark_dirty();
    storage::refresh_stock(state, defs);
    Ok(())
}

/// Removes the wall segment a gate replaces. A finished segment is torn down without refund
/// (its stones become part of the gate); a segment still under construction is cancelled
/// first, so the unused part of its gold and delivered material is refunded completely.
fn remove_replaced_wall(state: &mut GameState, defs: &Defs, wall: BuildingId, events: &mut Vec<SimEvent>) {
    let Some(b) = state.buildings.get(wall) else { return };
    let at = b.entrance;
    let site = match &b.phase {
        BuildingPhase::Construction(site) => Some(site.clone()),
        BuildingPhase::Active | BuildingPhase::Upgrading(_) => None,
    };
    let Some(mut old) = state.buildings.remove(wall) else { return };
    let contents = old.take_all_goods();
    storage::deposit_or_lose(state, defs, &contents, at, events);
    match site {
        Some(site) => {
            let refund = refund_unused(state, defs, &site, at, events);
            events.push(SimEvent::ConstructionCancelled { building: wall, kind: old.kind, refund });
        }
        None => events.push(SimEvent::BuildingDemolished {
            building: wall,
            kind: old.kind,
            origin: old.origin(),
            refund: contents,
        }),
    }
    state.workforce.mark_dirty();
}

/// Puts wood from cleared trees into the storages near `at`.
fn deposit_wood(state: &mut GameState, defs: &Defs, wood: u32, at: IVec2, events: &mut Vec<SimEvent>) {
    if let Some(wood_id) = defs.resource_id("wood")
        && wood > 0
    {
        storage::deposit_or_lose(state, defs, &[ResourceAmount { resource: wood_id, amount: wood }], at, events);
    }
}

/// Puts the unused part of a site's payment back: gold to the treasury, delivered but unused
/// materials to the storages near `at`.
fn refund_unused(
    state: &mut GameState,
    defs: &Defs,
    site: &ConstructionSite,
    at: IVec2,
    events: &mut Vec<SimEvent>,
) -> Vec<ResourceAmount> {
    let materials = site.unused_materials();
    state.economy.book_gold(LedgerCategory::Construction, i64::from(site.unused_gold()));
    storage::deposit_or_lose(state, defs, &materials, at, events);
    materials
}

/// `CancelConstruction`: removes a new building's site or stops an upgrade; the unused part of
/// gold and materials is refunded completely. Moves cannot be cancelled.
pub(crate) fn cancel_construction(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let b = state.buildings.get(id).ok_or(RejectReason::UnknownBuilding)?;
    let rotation = b.rotation;
    let at = b.entrance;
    let kind = b.kind;
    match b.phase.clone() {
        BuildingPhase::Active => Err(RejectReason::NotUnderConstruction),
        BuildingPhase::Construction(site) if site.purpose == SitePurpose::Move => Err(RejectReason::CannotCancelMove),
        BuildingPhase::Construction(site) if site.purpose == SitePurpose::Repair => Err(RejectReason::CannotCancelRepair),
        BuildingPhase::Construction(site) => {
            let Some(mut old) = state.buildings.remove(id) else { return Err(RejectReason::UnknownBuilding) };
            let refund = refund_unused(state, defs, &site, at, events);
            let contents = old.take_all_goods();
            storage::deposit_or_lose(state, defs, &contents, at, events);
            events.push(SimEvent::ConstructionCancelled { building: id, kind: old.kind, refund });
            state.workforce.mark_dirty();
            Ok(())
        }
        BuildingPhase::Upgrading(up) => {
            let refund = refund_unused(state, defs, &up.site, at, events);
            state
                .buildings
                .relocate(id, up.previous_rect, rotation, up.previous_entrance)
                .map_err(|_| RejectReason::Placement(super::PlacementError::OverlapsBuilding))?;
            if let Some(b) = state.buildings.get_mut(id) {
                b.phase = BuildingPhase::Active;
                events.push(SimEvent::UpgradeCancelled { building: id, target: up.target, refund });
            }
            state.workforce.mark_dirty();
            // The claim is a disc around the footprint centre, which moves back.
            if claims_territory(defs, kind) {
                refresh_territory(state, defs, events);
            }
            Ok(())
        }
    }
}

/// `DemolishBuilding`: removes a building, refunding 50 % of its type's materials (rounded
/// down), the unused part of a running upgrade and its contents. A construction site of a new
/// building is cancelled instead (full refund of the unused part). The last finished
/// headquarters cannot be demolished.
pub(crate) fn demolish_building(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let b = state.buildings.get(id).ok_or(RejectReason::UnknownBuilding)?;
    if matches!(&b.phase, BuildingPhase::Construction(site) if site.purpose == SitePurpose::New) {
        return cancel_construction(state, defs, id, events);
    }
    let at = b.entrance;
    let def = defs.buildings().get(b.kind.index()).ok_or(RejectReason::UnknownBuilding)?;
    if is_last_headquarters(state, defs, id) {
        return Err(RejectReason::LastHeadquarters);
    }
    let mut refund = if is_ruin(b) { Vec::new() } else { material_refund(def) };
    let upgrade_site = match &b.phase {
        BuildingPhase::Upgrading(up) => Some(up.site.clone()),
        // A ruin returns the delivered repair material that is not built in yet.
        BuildingPhase::Construction(site) if site.purpose == SitePurpose::Repair => Some(site.clone()),
        _ => None,
    };
    let Some(mut old) = state.buildings.remove(id) else { return Err(RejectReason::UnknownBuilding) };
    storage::deposit_or_lose(state, defs, &refund, at, events);
    if let Some(site) = upgrade_site {
        // On top of the demolition refund: the unused part of the running upgrade.
        refund.extend(refund_unused(state, defs, &site, at, events));
    }
    let contents = old.take_all_goods();
    storage::deposit_or_lose(state, defs, &contents, at, events);
    refund.extend(contents);
    let refund = merge_amounts(refund);
    events.push(SimEvent::BuildingDemolished { building: id, kind: old.kind, origin: old.origin(), refund });
    if def.role == BuildingRole::Headquarters {
        // Soldiers away on a campaign wait hidden in the main headquarters.
        crate::military::rehome_away(state, defs, id);
    }
    state.workforce.mark_dirty();
    if claims_territory(defs, old.kind) {
        refresh_territory(state, defs, events);
    }
    Ok(())
}

/// `UpgradeBuilding`: starts upgrading a building to its next level (houses: requirements from
/// the data; cost and work from the type's `upgrade`). A larger target grows the footprint
/// keeping the front edge in place. The building keeps its id, stock, buffers and orders.
pub(crate) fn upgrade_building(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let blockers = upgrade_blockers(state, defs, id).ok_or(RejectReason::UnknownBuilding)?;
    if let Some(first) = blockers.into_iter().next() {
        return Err(RejectReason::Upgrade(first));
    }
    let b = state.buildings.get(id).ok_or(RejectReason::UnknownBuilding)?;
    let from = b.kind;
    let def = defs.buildings().get(from.index()).ok_or(RejectReason::UnknownBuilding)?;
    let (upgrade, target) =
        super::upgrade_target(defs, from).ok_or(RejectReason::Upgrade(super::UpgradeBlocker::NoUpgrade))?;
    let target_id = target.id;
    let rotation = b.rotation;
    let previous_rect = b.rect;
    let previous_entrance = b.entrance;
    let origin = rotation.grown_origin(b.rect.origin, def.size, target.size);
    let rect = rotation.footprint(origin, target.size);
    let entrance = rotation.entrance(origin, target.size);
    // Vegetation only exists on the added tiles.
    let cleared_tiles: u32 = rect
        .tiles()
        .filter(|p| !previous_rect.contains(*p))
        .filter(|p| state.world.get(*p).is_some_and(|t| t.vegetation.blocks_building()))
        .count() as u32;
    state
        .economy
        .pay_gold(LedgerCategory::Construction, upgrade.cost.gold)
        .map_err(|missing| RejectReason::Placement(shortfall_error(defs, missing)))?;
    if let Err(e) = state.buildings.relocate(id, rect, rotation, entrance) {
        // Unreachable after the space check; undo the payment to stay consistent.
        log::error!("upgrade of {id:?} could not reserve its footprint: {e}");
        state.economy.book_gold(LedgerCategory::Construction, i64::from(upgrade.cost.gold));
        return Err(RejectReason::Upgrade(super::UpgradeBlocker::NoSpace));
    }
    // The claim is a disc around the footprint centre: a grown footprint moves it at once
    // (a loaded save derives the territory from the new footprint as well).
    if claims_territory(defs, from) {
        refresh_territory(state, defs, events);
    }
    let cleared = clear_vegetation(&mut state.world, rect);
    deposit_wood(state, defs, cleared.wood(defs.rules()), entrance, events);
    let site = ConstructionSite::new(
        SitePurpose::New,
        work_required(upgrade.hours, 100, cleared_tiles, defs.rules()),
        upgrade.cost.gold,
        upgrade.cost.materials.clone(),
        Vec::new(),
    );
    if let Some(b) = state.buildings.get_mut(id) {
        b.phase = BuildingPhase::Upgrading(UpgradeProject { target: target_id, site, previous_rect, previous_entrance });
    }
    state.workforce.mark_dirty();
    events.push(SimEvent::UpgradeStarted { building: id, from, to: target_id });
    storage::refresh_stock(state, defs);
    Ok(())
}

/// `MoveBuilding`: re-erects a finished building at a new spot. The building keeps its id and
/// contents; it is inactive while builders do 30 % of its build time (plus clearing). The
/// headquarters cannot be moved.
pub(crate) fn move_building(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    origin: IVec2,
    rotation: Rotation,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    move_allowed(state, defs, id)?;
    let b = state.buildings.get(id).ok_or(RejectReason::UnknownBuilding)?;
    let kind = b.kind;
    let from = b.origin();
    let def = defs.buildings().get(kind.index()).ok_or(RejectReason::UnknownBuilding)?;
    let check = check_move(state, defs, id, origin, rotation).ok_or(RejectReason::UnknownBuilding)?;
    if let Some(e) = check.errors.first() {
        return Err(RejectReason::Placement(e.clone()));
    }
    state
        .buildings
        .relocate(id, check.footprint, rotation, check.entrance)
        .map_err(|_| RejectReason::Placement(super::PlacementError::OverlapsBuilding))?;
    let cleared = clear_vegetation(&mut state.world, check.footprint);
    deposit_wood(state, defs, cleared.wood(defs.rules()), check.entrance, events);
    let work = work_required(def.build_hours, MOVE_WORK_PERCENT, cleared.tiles(), defs.rules());
    if let Some(b) = state.buildings.get_mut(id) {
        b.phase = BuildingPhase::Construction(ConstructionSite::new(SitePurpose::Move, work, 0, Vec::new(), Vec::new()));
        b.staffed = 0;
    }
    state.workforce.mark_dirty();
    events.push(SimEvent::MoveStarted { building: id, kind, from, to: origin });
    if def.territory_radius > 0 {
        refresh_territory(state, defs, events);
    }
    storage::refresh_stock(state, defs);
    Ok(())
}

/// True if `id` is a finished headquarters and no other finished headquarters exists (it
/// cannot be demolished).
fn is_last_headquarters(state: &GameState, defs: &Defs, id: BuildingId) -> bool {
    let Some(b) = state.buildings.get(id) else { return false };
    let is_hq = defs.buildings().get(b.kind.index()).is_some_and(|d| d.role == BuildingRole::Headquarters);
    is_hq
        && b.is_complete()
        && !state
            .buildings
            .with_role(defs, BuildingRole::Headquarters)
            .into_iter()
            .any(|other| other != id && state.buildings.get(other).is_some_and(Building::is_complete))
}

/// True for a damaged building waiting for (or under) repair.
fn is_ruin(b: &Building) -> bool {
    matches!(&b.phase, BuildingPhase::Construction(site) if site.purpose == SitePurpose::Repair)
}

/// Removes `percent` % (rounded down) of every good in `inventory`; returns what was removed.
fn lose_share(inventory: &mut crate::economy::Inventory, percent: u32) -> Vec<ResourceAmount> {
    let items: Vec<ResourceAmount> = inventory.iter().collect();
    items
        .into_iter()
        .map(|m| {
            let amount = inventory.remove(m.resource, m.amount * percent.min(100) / 100);
            ResourceAmount { resource: m.resource, amount }
        })
        .filter(|m| m.amount > 0)
        .collect()
}

/// Damages a finished, idle building (fire, flood or storm; docs/game_design.md §12): it stops
/// working and becomes a repair site that builders and carriers handle like any construction
/// site, needing `events.repair_work_percent` of the build time and
/// `events.repair_material_percent` of the material cost (no gold). Workers lose their jobs and
/// residents their home (the allocation runs again); a share of the goods inside is lost
/// (`events.fire_goods_loss_percent` / `flood_goods_loss_percent`, nothing by storm), the rest
/// stays inside until the repair is done. Returns the lost goods, or `None` if the building
/// cannot be damaged (not active, headquarters, wall segment or gate).
pub(crate) fn damage_building(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    cause: DamageCause,
    events: &mut Vec<SimEvent>,
) -> Option<Vec<ResourceAmount>> {
    let rules = &defs.rules().events;
    let b = state.buildings.get(id)?;
    let def = defs.buildings().get(b.kind.index())?;
    if !matches!(b.phase, BuildingPhase::Active) || !can_be_damaged(def) {
        return None;
    }
    let loss = match cause {
        DamageCause::Fire => u32::from(rules.fire_goods_loss_percent),
        DamageCause::Flood | DamageCause::Attack => u32::from(rules.flood_goods_loss_percent),
        DamageCause::Storm => 0,
    };
    let work = work_required(def.build_hours, u32::from(rules.repair_work_percent), 0, defs.rules());
    let materials: Vec<ResourceAmount> = def
        .cost
        .materials
        .iter()
        .map(|m| ResourceAmount {
            resource: m.resource,
            amount: (m.amount * u32::from(rules.repair_material_percent)).div_ceil(100),
        })
        .filter(|m| m.amount > 0)
        .collect();
    let kind = def.id;
    let claims = def.territory_radius > 0;
    let b = state.buildings.get_mut(id)?;
    let mut lost = lose_share(&mut b.inventory, loss);
    if let Some(p) = b.production.as_mut() {
        lost.extend(lose_share(&mut p.input, loss));
        lost.extend(lose_share(&mut p.output, loss));
    }
    b.phase = BuildingPhase::Construction(ConstructionSite::new(SitePurpose::Repair, work, 0, materials, Vec::new()));
    b.staffed = 0;
    state.buildings.note_activation();
    state.workforce.mark_dirty();
    events.push(SimEvent::BuildingDamaged { building: id, kind, cause });
    if claims {
        refresh_territory(state, defs, events);
    }
    storage::refresh_stock(state, defs);
    Some(merge_amounts(lost))
}

/// True if event damage can hit buildings of type `def` (not the headquarters, walls or gates).
pub(crate) fn can_be_damaged(def: &BuildingDef) -> bool {
    !matches!(def.role, BuildingRole::Headquarters | BuildingRole::WallSegment | BuildingRole::Gate)
}

/// A wall segment or gate broken down by enemies (docs/game_design.md §11): it vanishes
/// without a refund and leaves a gap.
pub(crate) fn destroy_structure(state: &mut GameState, defs: &Defs, id: BuildingId, events: &mut Vec<SimEvent>) {
    let Some(old) = state.buildings.remove(id) else { return };
    events.push(SimEvent::StructureDestroyed { building: id, kind: old.kind, origin: old.origin() });
    state.workforce.mark_dirty();
    if claims_territory(defs, old.kind) {
        refresh_territory(state, defs, events);
    }
}

/// The share of a building type's materials a demolition returns ([`DEMOLISH_REFUND_PERCENT`]).
fn material_refund(def: &BuildingDef) -> Vec<ResourceAmount> {
    def.cost
        .materials
        .iter()
        .map(|m| ResourceAmount { resource: m.resource, amount: m.amount * DEMOLISH_REFUND_PERCENT / 100 })
        .filter(|m| m.amount > 0)
        .collect()
}

/// What `DemolishBuilding` would return now (for the UI before demolishing).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemolishRefund {
    /// Gold back (unused gold of a cancelled site or a running upgrade).
    pub gold: u32,
    /// Materials and goods back, merged per resource in id order.
    pub materials: Vec<ResourceAmount>,
    /// True if demolishing cancels a new building's construction site instead.
    pub cancels_site: bool,
}

/// What demolishing building `id` returns now — a new building's site is cancelled (unused gold
/// and materials back), a finished building returns [`DEMOLISH_REFUND_PERCENT`] of its
/// materials, the unused part of a running upgrade and its contents — or why `DemolishBuilding`
/// would be rejected (the last headquarters).
pub fn demolish_refund(state: &GameState, defs: &Defs, id: BuildingId) -> Result<DemolishRefund, RejectReason> {
    let b = state.buildings.get(id).ok_or(RejectReason::UnknownBuilding)?;
    if let BuildingPhase::Construction(site) = &b.phase
        && site.purpose == SitePurpose::New
    {
        return Ok(DemolishRefund { gold: site.unused_gold(), materials: site.unused_materials(), cancels_site: true });
    }
    if is_last_headquarters(state, defs, id) {
        return Err(RejectReason::LastHeadquarters);
    }
    let def = defs.buildings().get(b.kind.index()).ok_or(RejectReason::UnknownBuilding)?;
    let mut materials = if is_ruin(b) { Vec::new() } else { material_refund(def) };
    let mut gold = 0;
    let running = match &b.phase {
        BuildingPhase::Upgrading(up) => Some(&up.site),
        BuildingPhase::Construction(site) if site.purpose == SitePurpose::Repair => Some(site),
        _ => None,
    };
    if let Some(site) = running {
        materials.extend(site.unused_materials());
        gold += site.unused_gold();
    }
    materials.extend(b.all_goods());
    Ok(DemolishRefund { gold, materials: merge_amounts(materials), cancels_site: false })
}

/// Why `SetBuildingPaused { paused: true }` would be rejected for building `id` (`Ok` if it may
/// be paused): finished houses keep their residents and cannot be paused, inhabited or not.
pub fn pause_allowed(state: &GameState, defs: &Defs, id: BuildingId) -> CommandResult {
    let b = state.buildings.get(id).ok_or(RejectReason::UnknownBuilding)?;
    let is_house = defs.buildings().get(b.kind.index()).is_some_and(|d| d.housing.is_some());
    if is_house && b.is_complete() {
        return Err(RejectReason::CannotPauseHousing);
    }
    Ok(())
}

/// Why building `id` cannot be moved at all (`Ok` if it can; the new spot is checked by
/// [`super::check_move`]): headquarters stay, and only finished, idle buildings move.
pub fn move_allowed(state: &GameState, defs: &Defs, id: BuildingId) -> CommandResult {
    let b = state.buildings.get(id).ok_or(RejectReason::UnknownBuilding)?;
    let def = defs.buildings().get(b.kind.index()).ok_or(RejectReason::UnknownBuilding)?;
    if def.role == BuildingRole::Headquarters {
        return Err(RejectReason::CannotMoveHeadquarters);
    }
    if !matches!(b.phase, BuildingPhase::Active) {
        return Err(RejectReason::NotMovable);
    }
    Ok(())
}

/// `SetBuildingPaused`: stops or resumes a building or construction site (no workers while
/// paused). Finished houses cannot be paused ([`pause_allowed`]).
pub(crate) fn set_paused(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    paused: bool,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let b = state.buildings.get(id).ok_or(RejectReason::UnknownBuilding)?;
    if b.paused == paused {
        return Ok(());
    }
    if paused {
        pause_allowed(state, defs, id)?;
    }
    let b = state.buildings.get_mut(id).ok_or(RejectReason::UnknownBuilding)?;
    b.paused = paused;
    if paused {
        b.staffed = 0;
    }
    state.workforce.mark_dirty();
    events.push(SimEvent::BuildingPauseChanged { building: id, paused });
    storage::refresh_stock(state, defs);
    Ok(())
}

/// Creates a finished building directly (scenario start). The footprint is cleared without
/// wood yield so the starting stock matches the scenario exactly.
pub(crate) fn spawn_finished(
    state: &mut GameState,
    defs: &Defs,
    kind: BuildingTypeId,
    origin: IVec2,
    rotation: Rotation,
) -> Option<BuildingId> {
    let def = defs.buildings().get(kind.index())?;
    let rect: TileRect = rotation.footprint(origin, def.size);
    let id = state.buildings.insert(Building::new(def, origin, rotation, BuildingPhase::Active)).ok()?;
    clear_vegetation(&mut state.world, rect);
    activate(state, defs, id);
    state.workforce.mark_dirty();
    storage::refresh_stock(state, defs);
    Some(id)
}
