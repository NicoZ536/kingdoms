//! Pipeline step 5: work cycles of all production buildings.

use std::collections::BTreeSet;

use glam::IVec2;

use super::gathering::{self, DepositReservations, find_deposit, find_planting_spot, find_tree, tree_kind_for};
use super::{Cycle, MILLI_PER_UNIT, ProductionState, ProductionStatus, milli, production_def, recipe, tools_id};
use crate::buildings::BuildingId;
use crate::grid::TileRect;
use crate::defs::{BuildingDef, Defs, Gathering, ProductionDef, RecipeDef, ResourceAmount, ResourceId};
use crate::event::{Notification, NotificationKind, Severity, SimEvent};
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;
use crate::world::{DepositKind, Vegetation};

/// Season and weather factor on the production of buildings of type `def` in permille
/// (winter work speed, outdoor weather, seasonal fishing/pasture/bees; see
/// [`crate::weather::work_permille`]).
pub fn season_weather_permille(state: &GameState, defs: &Defs, def: &BuildingDef) -> u32 {
    crate::weather::work_permille(state, defs, def)
}

/// Units that must be taken from the buffer so that `credit` covers `need` milli-units.
fn units_needed(credit: u32, need: u32) -> u32 {
    need.saturating_sub(credit).div_ceil(MILLI_PER_UNIT)
}

/// Pays `need` milli-units of `resource` from the buffer (whole units, the rest is credited).
/// The caller has checked availability. Returns the whole units taken.
fn pay(p: &mut ProductionState, resource: ResourceId, need: u32) -> u32 {
    let credit = p.credits.get(&resource).copied().unwrap_or(0);
    let units = units_needed(credit, need);
    let taken = p.input.remove(resource, units);
    let remaining = (credit + taken * MILLI_PER_UNIT).saturating_sub(need);
    if remaining == 0 {
        p.credits.remove(&resource);
    } else {
        p.credits.insert(resource, remaining);
    }
    taken
}

/// Gathering targets reserved by running cycles of other buildings.
fn reserved_targets(state: &GameState, except: BuildingId) -> BTreeSet<(i32, i32)> {
    state
        .buildings
        .iter()
        .filter(|(id, _)| *id != except)
        .filter_map(|(_, b)| b.production.as_ref()?.cycle.as_ref()?.target)
        .map(|t| (t.x, t.y))
        .collect()
}

/// Most whole units a cycle of `recipe` with `bonus_percent` can yield (fractions rounded up).
fn cycle_yield_units(recipe: &RecipeDef, bonus_percent: u16) -> u32 {
    let total: u64 = recipe
        .outputs
        .iter()
        .map(|o| (u64::from(milli(o.amount)) * (100 + u64::from(bonus_percent)) / 100).div_ceil(u64::from(MILLI_PER_UNIT)))
        .sum();
    u32::try_from(total).unwrap_or(u32::MAX)
}

/// Deposit units claimed by the running cycles of other deposit gatherers, per target tile.
/// A cycle claims the most units it can yield; a new cycle only starts on a tile with units
/// left over, so several gatherers never start on the same last unit.
fn reserved_deposit_units(state: &GameState, defs: &Defs, except: BuildingId) -> DepositReservations {
    let mut out = DepositReservations::new();
    for (id, b) in state.buildings.iter() {
        if id == except {
            continue;
        }
        let Some(cycle) = b.production.as_ref().and_then(|p| p.cycle.as_ref()) else { continue };
        let Some(target) = cycle.target else { continue };
        let Some((_, pdef)) = production_def(defs, b) else { continue };
        if !matches!(pdef.gathering, Some(Gathering::Deposit { .. })) {
            continue;
        }
        let units = recipe(pdef, cycle.recipe).map_or(0, |r| cycle_yield_units(r, cycle.bonus_percent));
        let slot = out.entry((target.x, target.y)).or_insert(0);
        *slot = slot.saturating_add(units);
    }
    out
}

/// Gathering radius of a building type whose workers walk to their targets (trees, planting
/// spots, deposits, fish; step 3b), `None` for the others.
pub(crate) fn walking_radius(pdef: &ProductionDef) -> Option<u8> {
    match pdef.gathering {
        Some(Gathering::Trees { radius } | Gathering::PlantTrees { radius } | Gathering::Deposit { radius, .. }) => Some(radius),
        _ => None,
    }
}

/// Pipeline step 5: advances or starts the cycle of every production building (ascending id).
pub(crate) fn run_production(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    // Work spots are chosen by reachability (navigation components).
    crate::navigation::prepare(state, defs);
    let ids: Vec<BuildingId> =
        state.buildings.iter().filter(|(_, b)| b.production.is_some()).map(|(id, _)| id).collect();
    for id in ids {
        step(state, defs, id, events);
    }
}

fn step(state: &mut GameState, defs: &Defs, id: BuildingId, events: &mut Vec<SimEvent>) {
    let Some(b) = state.buildings.get(id) else { return };
    let Some((def, pdef)) = production_def(defs, b) else { return };
    if b.production.is_none() {
        return;
    }
    let complete = b.is_complete();
    let paused = b.paused;
    let staffed = b.staffed();
    let has_cycle = b.production.as_ref().is_some_and(|p| p.cycle.is_some());

    // Finished field farms work through the agriculture step (pipeline step 4).
    if complete && matches!(pdef.gathering, Some(Gathering::Fields { .. })) {
        return;
    }
    if complete && !paused && has_cycle && !b.is_upgrading() && walking_radius(pdef).is_some() {
        check_target(state, defs, id, pdef);
    }
    let has_cycle = state.buildings.get(id).and_then(|b| b.production.as_ref()).is_some_and(|p| p.cycle.is_some());
    let status = if !complete {
        ProductionStatus::UnderConstruction
    } else if b_upgrading(state, id) {
        // The builders hold the worker slots; the running cycle resumes afterwards.
        ProductionStatus::Upgrading
    } else if paused {
        ProductionStatus::Paused
    } else if has_cycle {
        if staffed == 0 {
            ProductionStatus::NoWorkers
        } else if crate::population::present_workers(state, defs, id) == 0 {
            crate::population::absence_status(state, defs, id)
        } else {
            advance(state, defs, id, def, pdef, events)
        }
    } else {
        try_start(state, defs, id, pdef)
    };
    update_status(state, defs, id, def, status, events);
}

fn b_upgrading(state: &GameState, id: BuildingId) -> bool {
    state.buildings.get(id).is_some_and(|b| b.is_upgrading())
}

/// Stores the status of production building `id` (reporting changes), clears its
/// productivity while it is not working and warns about lasting problems.
pub(crate) fn update_status(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    def: &BuildingDef,
    status: ProductionStatus,
    events: &mut Vec<SimEvent>,
) {
    let tick = state.clock.tick;
    let Some(p) = state.buildings.get_mut(id).and_then(|b| b.production.as_mut()) else { return };
    if status != ProductionStatus::Working {
        p.last_productivity_permille = 0;
    }
    // A reported spell that has been over long enough ends before a new status is looked at.
    if p.resolved_since.is_some_and(|t| tick.saturating_sub(t) >= defs.rules().notices.resolved_ticks()) {
        p.reported = None;
        p.resolved_since = None;
    }
    if status != p.status {
        let idle_changed = crate::workforce::is_idle_status(p.status) != crate::workforce::is_idle_status(status);
        p.status = status;
        p.problem_since = is_waiting(status).then_some(tick);
        // The reported spell goes on while its status returns, and ends only after it stayed
        // away for `notices.waiting_resolved_hours` (see `report_waiting`).
        if p.reported == Some(status) {
            p.resolved_since = None;
        } else if p.reported.is_some() && p.resolved_since.is_none() {
            p.resolved_since = Some(tick);
        }
        events.push(SimEvent::ProductionStatusChanged { building: id, status });
        if idle_changed {
            // Idle buildings are staffed last (crate::workforce).
            state.workforce.mark_jobs_dirty();
        }
    }
    warn_if_stuck(state, defs, id, def, events);
}

/// True for the waiting states that are reported once per spell (docs/game_design.md §2
/// Meldungen): a missing input, nothing to gather in the radius, no room to plant.
fn is_waiting(status: ProductionStatus) -> bool {
    matches!(status, ProductionStatus::MissingInput(_) | ProductionStatus::NoResourcesInRange | ProductionStatus::NoPlantingSpace)
}

/// Reports a waiting state once per spell (docs/game_design.md §2 Meldungen): when it has
/// lasted `notices.waiting_delay_hours` (a missing input only while none is on its way), and
/// again only after the reported state stayed away for `notices.waiting_resolved_hours` in a
/// row. The status itself stays visible in the building's report. A dense forest is a neutral
/// information, the others are warnings.
fn warn_if_stuck(state: &mut GameState, defs: &Defs, id: BuildingId, def: &BuildingDef, events: &mut Vec<SimEvent>) {
    let tick = state.clock.tick;
    let rules = &defs.rules().notices;
    let Some(b) = state.buildings.get(id) else { return };
    let position = b.entrance();
    let Some(p) = b.production.as_ref() else { return };
    let Some(since) = p.problem_since else { return };
    if tick.saturating_sub(since) < rules.delay_ticks() || p.reported == Some(p.status) {
        return;
    }
    let status = p.status;
    let (kind, text) = match status {
        ProductionStatus::MissingInput(resource) => {
            if state.logistics.incoming(id, resource) > 0 {
                return;
            }
            let name = defs.resources().get(resource.index()).map_or_else(|| resource.to_string(), |r| r.name.clone());
            events.push(SimEvent::Shortage { building: id, resource, position });
            (NotificationKind::MissingInput, format!("Dem Gebäude „{}“ fehlt {name}.", def.name))
        }
        ProductionStatus::NoResourcesInRange => (
            NotificationKind::NoResources,
            match crate::agriculture::farm_spec(def) {
                Some(_) => format!("{}: kein freies Acker- oder Weideland im Umkreis.", def.name),
                None => format!("{}: keine Rohstoffe im Umkreis.", def.name),
            },
        ),
        ProductionStatus::NoPlantingSpace => (
            NotificationKind::DenseForest,
            format!("{}: Der Wald im Umkreis ist dicht – gepflanzt wird wieder, sobald Platz frei wird.", def.name),
        ),
        _ => return,
    };
    let severity = if status == ProductionStatus::NoPlantingSpace { Severity::Info } else { Severity::Warning };
    events.push(SimEvent::Notification(Notification::new(severity, text).with_kind(kind).at(position).for_building(id)));
    if let Some(p) = state.buildings.get_mut(id).and_then(|b| b.production.as_mut()) {
        p.reported = Some(status);
        p.resolved_since = None;
    }
}

/// Productivity of a running cycle in permille.
fn productivity_permille(state: &GameState, defs: &Defs, def: &BuildingDef, building: BuildingId, cycle: &Cycle) -> u64 {
    work_rate_permille(state, defs, def, building, cycle.with_tools, cycle.environment_permille)
}

/// Productivity of one worker of a building of type `def` in permille: health and happiness
/// ([`crate::population::work_efficiency_permille`]) × training in the job
/// ([`crate::population::training_permille`]; docs §6).
pub fn worker_permille(defs: &Defs, def: &BuildingDef, citizen: &crate::population::Citizen) -> u32 {
    let rules = &defs.rules().population;
    let training = def.job.map_or(1000, |j| crate::population::training_permille(rules, citizen, j.category, j.tier));
    crate::population::work_efficiency_permille(rules, citizen) * training / 1000
}

/// Staffing factor of building `building` (of type `def`) in permille: every assigned worker
/// who is present ([`crate::population::is_at_work`]) adds its share of the slots weighted by
/// health, happiness and training ([`worker_permille`], docs §6). 0 while the workers are on
/// the way, at lunch or at home.
pub fn staffing_permille(state: &GameState, defs: &Defs, def: &BuildingDef, building: BuildingId) -> u32 {
    let workers = u32::from(def.workers.max(1));
    let sum: u32 = state
        .population
        .workers_of(building)
        .iter()
        .take(usize::from(def.workers.max(1)))
        .filter_map(|c| state.population.citizen(*c))
        .filter(|c| crate::population::is_at_work(state, defs, c))
        .map(|c| worker_permille(defs, def, c))
        .sum();
    sum / workers
}

/// Average training factor of the assigned workers of `building` in permille (1000 without
/// workers or for unskilled jobs; docs §6 Qualifikation).
pub fn crew_training_permille(state: &GameState, defs: &Defs, def: &BuildingDef, building: BuildingId) -> u32 {
    let Some(job) = def.job else { return 1000 };
    let rules = &defs.rules().population;
    let factors: Vec<u32> = state
        .population
        .workers_of(building)
        .iter()
        .filter_map(|c| state.population.citizen(*c))
        .map(|c| crate::population::training_permille(rules, c, job.category, job.tier))
        .collect();
    if factors.is_empty() { 1000 } else { factors.iter().sum::<u32>() / factors.len() as u32 }
}

/// Progress per tick of a present crew working at `rate_permille` (productivity): recipe,
/// farm-task and build times are given for round-the-clock operation, but people work only
/// during the shift, so present workers advance by the shift factor
/// (`RoutineHours::shift_factor_permille`, 3000 for an 8-hour shift).
pub(crate) fn shift_progress(defs: &Defs, rate_permille: u64) -> u64 {
    rate_permille * defs.rules().walking.routine.shift_factor_permille() / 1000
}

/// Work progress of building `building` per tick in permille of full speed: staffing (workers'
/// efficiency, docs §6) × tools × research × environment × season/weather × bankruptcy × event
/// modifiers (docs §12).
pub(crate) fn work_rate_permille(
    state: &GameState,
    defs: &Defs,
    def: &BuildingDef,
    building: BuildingId,
    with_tools: bool,
    environment_permille: u16,
) -> u64 {
    let staffing = staffing_permille(state, defs, def, building);
    rate_with_staffing(state, defs, def, staffing, with_tools, environment_permille)
}

/// The work rate of building `building` while its whole assigned crew works (as
/// [`work_rate_permille`] during the shift, with tools; for daily figures that should not
/// depend on the hour, e.g. `report::building_economics`).
pub(crate) fn crew_rate_permille(state: &GameState, defs: &Defs, def: &BuildingDef, building: BuildingId) -> u64 {
    let workers = u32::from(def.workers.max(1));
    let staffing: u32 = state
        .population
        .workers_of(building)
        .iter()
        .take(usize::from(def.workers.max(1)))
        .filter_map(|c| state.population.citizen(*c))
        .map(|c| worker_permille(defs, def, c))
        .sum::<u32>()
        / workers;
    rate_with_staffing(state, defs, def, staffing, true, 1000)
}

/// [`work_rate_permille`] for a given staffing factor.
fn rate_with_staffing(
    state: &GameState,
    defs: &Defs,
    def: &BuildingDef,
    staffing_permille: u32,
    with_tools: bool,
    environment_permille: u16,
) -> u64 {
    let rules = defs.rules();
    let staffing = u64::from(staffing_permille);
    let tools = if with_tools { 100 } else { u64::from(rules.no_tools_productivity_percent) };
    let research = u64::try_from((100 + state.research.production_bonus_percent(defs, def)).max(0)).unwrap_or(0);
    let bankrupt = if state.economy.is_bankrupt() { u64::from(rules.bankruptcy_productivity_percent) } else { 100 };
    let season = u64::from(season_weather_permille(state, defs, def));
    let events = crate::events::production_percent(state);
    staffing * tools / 100 * research / 100 * u64::from(environment_permille) / 1000 * season / 1000 * bankrupt / 100 * events
        / 100
}

fn advance(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    def: &BuildingDef,
    pdef: &ProductionDef,
    events: &mut Vec<SimEvent>,
) -> ProductionStatus {
    let Some(b) = state.buildings.get(id) else { return ProductionStatus::Idle };
    let Some(cycle) = b.production.as_ref().and_then(|p| p.cycle) else { return ProductionStatus::Idle };
    let rate = productivity_permille(state, defs, def, id, &cycle);
    let Some(p) = state.buildings.get_mut(id).and_then(|b| b.production.as_mut()) else {
        return ProductionStatus::Idle;
    };
    p.last_productivity_permille = rate.min(u64::from(u32::MAX)) as u32;
    let Some(c) = p.cycle.as_mut() else { return ProductionStatus::Idle };
    c.progress = c.progress.saturating_add(shift_progress(defs, rate));
    if c.progress >= c.required {
        complete(state, defs, id, pdef, events);
    }
    ProductionStatus::Working
}

/// The gathering target (tree, sapling spot, deposit) with the gatherers' work spot
/// ([`gathering::work_spot`]) and the environment factor (pasture, bees) for the next cycle of
/// building `id`, or [`ProductionStatus::NoResourcesInRange`] (also when no target can be
/// reached on foot).
fn gathering_target(
    state: &GameState,
    defs: &Defs,
    id: BuildingId,
    rect: TileRect,
    pdef: &ProductionDef,
) -> Result<(Option<IVec2>, Option<IVec2>, u16), ProductionStatus> {
    let rules = defs.rules();
    let mut target = None;
    let mut environment: u16 = 1000;
    let entrance = state.buildings.get(id).map_or(rect.origin, |b| b.entrance());
    let usable = |p: IVec2| gathering::work_spot(state, rect, entrance, p).is_some();
    match &pdef.gathering {
        Some(Gathering::Trees { radius }) => {
            let reserved = reserved_targets(state, id);
            target = Some(find_tree(state, rules, rect, *radius, &reserved, &usable).ok_or(ProductionStatus::NoResourcesInRange)?);
        }
        Some(Gathering::PlantTrees { radius }) => {
            let reserved = reserved_targets(state, id);
            target = Some(find_planting_spot(state, rect, *radius, &reserved, &usable).ok_or(ProductionStatus::NoPlantingSpace)?);
        }
        Some(Gathering::Deposit { kind, radius, or_terrain }) => {
            let reserved = reserved_deposit_units(state, defs, id);
            target = Some(
                find_deposit(state, rect, *kind, *radius, *or_terrain, &reserved, &usable)
                    .ok_or(ProductionStatus::NoResourcesInRange)?,
            );
        }
        Some(Gathering::Pasture { tiles, .. }) => {
            environment = gathering::pasture_permille(state, defs, id, *tiles);
            if environment == 0 {
                return Err(ProductionStatus::NoResourcesInRange);
            }
        }
        Some(Gathering::Bees { radius, max_bonus_percent }) => {
            environment = gathering::bee_permille(state, defs, rect, *radius, *max_bonus_percent);
        }
        Some(Gathering::Fields { .. }) | None => {}
    }
    let spot = target.and_then(|t| gathering::work_spot(state, rect, entrance, t)).flatten();
    Ok((target, spot, environment))
}

/// Keeps the target of a walking gatherer's running cycle workable: if the target vanished
/// (felled, built over, exhausted) or its work spot can no longer be reached, the cycle moves
/// to the next target (progress kept) and the workers walk there; without one the cycle ends
/// without a yield.
fn check_target(state: &mut GameState, defs: &Defs, id: BuildingId, pdef: &ProductionDef) {
    let Some(b) = state.buildings.get(id) else { return };
    let Some(cycle) = b.production.as_ref().and_then(|p| p.cycle) else { return };
    // Cycles worked from inside (mines) keep their target; `gather` handles a vanished one.
    let (Some(target), Some(_)) = (cycle.target, cycle.spot) else { return };
    let rect = b.rect();
    let rules = defs.rules();
    let still_there = state.world.get(target).is_some_and(|t| match &pdef.gathering {
        Some(Gathering::Trees { .. }) => gathering::is_fellable(t.vegetation, rules),
        Some(Gathering::PlantTrees { .. }) => gathering::is_plantable(state, target),
        Some(Gathering::Deposit { kind, or_terrain, .. }) => {
            t.deposit.is_some_and(|d| d.kind == *kind && d.amount > 0) || or_terrain.is_some_and(|o| t.terrain == o)
        }
        _ => true,
    });
    let spot = gathering::work_spot(state, rect, b.entrance(), target);
    if still_there && spot == Some(cycle.spot) {
        return;
    }
    let next = if still_there && let Some(spot) = spot {
        Some((target, spot))
    } else {
        match gathering_target(state, defs, id, rect, pdef) {
            Ok((Some(t), spot, _)) => Some((t, spot)),
            _ => None,
        }
    };
    let Some(p) = state.buildings.get_mut(id).and_then(|b| b.production.as_mut()) else { return };
    match (next, p.cycle.as_mut()) {
        (Some((t, spot)), Some(c)) => {
            c.target = Some(t);
            c.spot = spot;
        }
        _ => p.cycle = None,
    }
    crate::population::routine::refresh_workers(state, defs, id);
}

fn try_start(state: &mut GameState, defs: &Defs, id: BuildingId, pdef: &ProductionDef) -> ProductionStatus {
    let rules = defs.rules();
    let Some(b) = state.buildings.get(id) else { return ProductionStatus::Idle };
    let Some(p) = b.production.as_ref() else { return ProductionStatus::Idle };
    let Some(index) = p.active_recipe() else { return ProductionStatus::Idle };
    let Some(recipe) = recipe(pdef, index) else { return ProductionStatus::Idle };
    if b.staffed() == 0 {
        // An empty gathering area is reported without workers too: such buildings are staffed
        // last (crate::workforce), and their status must not flip when they lose the workers.
        return match gathering_target(state, defs, id, b.rect(), pdef) {
            Err(status) => status,
            Ok(_) => ProductionStatus::NoWorkers,
        };
    }
    // Room for the largest possible result.
    let max_bonus: u32 = recipe.optional_inputs.iter().map(|o| u32::from(o.output_bonus_percent)).sum();
    for out in &recipe.outputs {
        let pending = p.pending.get(&out.resource).copied().unwrap_or(0);
        let units = (pending + milli(out.amount) * (100 + max_bonus) / 100) / MILLI_PER_UNIT;
        // Yield on its way back (gatherers' hauls) has its room reserved.
        let hauled = state.logistics.in_transit_to(id, out.resource);
        if p.output.get(out.resource) + hauled + units > p.output.capacity(out.resource) {
            return ProductionStatus::OutputFull;
        }
    }
    // Inputs.
    for input in &recipe.inputs {
        let credit = p.credits.get(&input.resource).copied().unwrap_or(0);
        if p.input.get(input.resource) < units_needed(credit, milli(input.amount)) {
            return ProductionStatus::MissingInput(input.resource);
        }
    }
    // Gathering target and environment.
    let (target, spot, environment) = match gathering_target(state, defs, id, b.rect(), pdef) {
        Ok(found) => found,
        Err(status) => return status,
    };
    let ready = match walking_radius(pdef) {
        // Gatherers start from the building or wherever the last cycle left them.
        Some(radius) => crate::population::routine::on_duty_workers(state, defs, id, i32::from(radius) + 1) > 0,
        None => crate::population::present_workers(state, defs, id) > 0,
    };
    if !ready {
        // Nobody at work: no cycle starts (inputs stay in the buffer). Full outputs, missing
        // inputs and empty gathering areas are reported first.
        return crate::population::absence_status(state, defs, id);
    }
    let required = (f64::from(recipe.hours) * TICKS_PER_HOUR as f64 * f64::from(MILLI_PER_UNIT)).round().max(1.0) as u64;
    // Researched `ToolWearReduction` makes a tool last longer (50 % less wear: twice the cycles).
    let reduction = u32::try_from(state.research.tool_wear_reduction_percent(defs)).unwrap_or(0);
    let tool_wear = u16::try_from(u32::from(rules.tool_wear_cycles) * 100 / (100 - reduction).max(10)).unwrap_or(u16::MAX);
    let tools = tools_id(defs);

    // Start: consume inputs, optional inputs and tools.
    let mut consumed: Vec<ResourceAmount> = Vec::new();
    let Some(p) = state.buildings.get_mut(id).and_then(|b| b.production.as_mut()) else {
        return ProductionStatus::Idle;
    };
    for input in &recipe.inputs {
        let taken = pay(p, input.resource, milli(input.amount));
        consumed.push(ResourceAmount { resource: input.resource, amount: taken });
    }
    let mut bonus: u16 = 0;
    for opt in &recipe.optional_inputs {
        let need = milli(opt.amount);
        let credit = p.credits.get(&opt.resource).copied().unwrap_or(0);
        if p.input.get(opt.resource) >= units_needed(credit, need) {
            let taken = pay(p, opt.resource, need);
            consumed.push(ResourceAmount { resource: opt.resource, amount: taken });
            bonus = bonus.saturating_add(opt.output_bonus_percent);
        }
    }
    let mut with_tools = true;
    if pdef.uses_tools {
        if p.tool_charge == 0
            && let Some(tools) = tools
            && p.input.remove(tools, 1) == 1
        {
            p.tool_charge = tool_wear;
            consumed.push(ResourceAmount { resource: tools, amount: 1 });
        }
        with_tools = p.tool_charge > 0;
        if with_tools {
            p.tool_charge -= 1;
        }
    }
    p.cycle = Some(Cycle {
        recipe: index,
        progress: 0,
        required,
        with_tools,
        bonus_percent: bonus,
        environment_permille: environment,
        target,
        spot,
    });
    for c in consumed.into_iter().filter(|c| c.amount > 0) {
        state.economy.record_consumed(c.resource, c.amount);
    }
    if spot.is_some() {
        // The workers walk to the new work spot.
        crate::population::routine::refresh_workers(state, defs, id);
    }
    ProductionStatus::Working
}

/// Applies the gathering effect of a finished cycle of building `id`; returns the most whole
/// units the cycle may emit (`None` = unlimited, `Some(0)` = nothing was gathered).
fn gather(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    pdef: &ProductionDef,
    target: Option<IVec2>,
    wanted_units: u32,
    events: &mut Vec<SimEvent>,
) -> Option<u32> {
    let Some(rect) = state.buildings.get(id).map(|b| b.rect()) else { return Some(0) };
    match &pdef.gathering {
        Some(Gathering::Trees { radius }) => {
            let rules = defs.rules();
            let p = target.filter(|p| state.world.get(*p).is_some_and(|t| gathering::is_fellable(t.vegetation, rules)));
            let p = p.or_else(|| find_tree(state, rules, rect, *radius, &BTreeSet::new(), &|_| true));
            match p {
                Some(p) => {
                    state.world.tile_mut(p).vegetation = Vegetation::Stump { hours: 0 };
                    events.push(SimEvent::TreeFelled { position: p });
                    None
                }
                None => Some(0),
            }
        }
        Some(Gathering::PlantTrees { .. }) => {
            if let Some(p) = target
                && gathering::is_plantable(state, p)
            {
                let kind = tree_kind_for(state.world.tile(p), p);
                state.world.tile_mut(p).vegetation = Vegetation::Tree { kind, growth: 0 };
                events.push(SimEvent::TreePlanted { position: p });
            }
            None
        }
        Some(Gathering::Deposit { kind, radius, or_terrain }) => {
            let valid = |state: &GameState, p: IVec2| {
                state.world.get(p).is_some_and(|t| {
                    t.deposit.is_some_and(|d| d.kind == *kind && d.amount > 0) || or_terrain.is_some_and(|o| t.terrain == o)
                })
            };
            // Reservations keep the target valid; if it vanished anyway (outside changes), fall
            // back to another tile with unclaimed units. Without one the cycle yields nothing.
            let fallback = |state: &GameState| {
                let reserved = reserved_deposit_units(state, defs, id);
                find_deposit(state, rect, *kind, *radius, *or_terrain, &reserved, &|_| true)
            };
            let Some(p) = target.filter(|p| valid(state, *p)).or_else(|| fallback(state)) else {
                return Some(0);
            };
            let tile = state.world.tile(p);
            let Some(deposit) = tile.deposit.filter(|d| d.kind == *kind && d.amount > 0) else {
                // Inexhaustible terrain source (beach sand).
                return None;
            };
            let take = u32::from(deposit.amount).min(wanted_units);
            let left = deposit.amount - take as u16;
            if left == 0 && *kind != DepositKind::Fish {
                state.world.tile_mut(p).deposit = None;
                events.push(SimEvent::DepositDepleted { position: p, kind: *kind });
            } else if let Some(d) = state.world.tile_mut_quiet(p).deposit.as_mut() {
                d.amount = left;
            }
            Some(take)
        }
        _ => None,
    }
}

fn complete(state: &mut GameState, defs: &Defs, id: BuildingId, pdef: &ProductionDef, events: &mut Vec<SimEvent>) {
    let Some(b) = state.buildings.get(id) else { return };
    let Some(cycle) = b.production.as_ref().and_then(|p| p.cycle) else { return };
    let Some(recipe) = recipe(pdef, cycle.recipe) else { return };
    // Whole units this cycle would emit.
    let mut pending: Vec<(ResourceId, u32)> = Vec::with_capacity(recipe.outputs.len());
    if let Some(p) = b.production.as_ref() {
        for out in &recipe.outputs {
            let before = p.pending.get(&out.resource).copied().unwrap_or(0);
            let add = milli(out.amount) * (100 + u32::from(cycle.bonus_percent)) / 100;
            pending.push((out.resource, before + add));
        }
    }
    let wanted: u32 = pending.iter().map(|(_, m)| m / MILLI_PER_UNIT).sum();
    let limit = gather(state, defs, id, pdef, cycle.target, wanted, events);
    // Gatherers working outside carry the yield back: the first worker at the spot takes the
    // first output (its room in the buffer is reserved); further outputs go straight in.
    let hauler = cycle.spot.and_then(|_| {
        state
            .population
            .workers_of(id)
            .iter()
            .copied()
            .find(|c| state.population.citizen(*c).is_some_and(|c| crate::population::is_at_work(state, defs, c)))
    });
    let hauled_before: Vec<u32> = pending.iter().map(|(r, _)| state.logistics.in_transit_to(id, *r)).collect();
    let mut haul: Option<ResourceAmount> = None;

    let Some(p) = state.buildings.get_mut(id).and_then(|b| b.production.as_mut()) else { return };
    let mut allowed = limit.unwrap_or(u32::MAX);
    let mut produced: Vec<ResourceAmount> = Vec::new();
    for ((resource, total), hauled) in pending.into_iter().zip(hauled_before) {
        if limit == Some(0) {
            // Nothing gathered (tree vanished, deposit exhausted): the cycle yields nothing.
            continue;
        }
        let available = total / MILLI_PER_UNIT;
        let whole = available.min(allowed);
        let stored = if hauler.is_some() && haul.is_none() {
            let room = p.output.capacity(resource).saturating_sub(p.output.get(resource) + hauled);
            let n = whole.min(room);
            haul = (n > 0).then_some(ResourceAmount { resource, amount: n });
            n
        } else {
            p.output.add(resource, whole)
        };
        allowed -= stored;
        // Units an exhausted deposit did not yield are discarded; units that found no room in
        // the buffer stay pending.
        let cut = available - whole;
        let rest = total - (stored + cut) * MILLI_PER_UNIT;
        if rest == 0 {
            p.pending.remove(&resource);
        } else {
            p.pending.insert(resource, rest);
        }
        if stored > 0 {
            produced.push(ResourceAmount { resource, amount: stored });
        }
    }
    p.cycle = None;
    p.cycles_completed += 1;
    p.made.record_cycle(&produced);
    // Queue bookkeeping: count units of the first output (cycles for recipes without outputs).
    let primary = recipe.outputs.first().map(|o| o.resource);
    let done = match primary {
        Some(r) => produced.iter().find(|m| m.resource == r).map_or(0, |m| m.amount),
        None => 1,
    };
    let mut recipe_changed = false;
    if let Some(front) = p.queue.first_mut()
        && front.recipe == cycle.recipe
        && let Some(remaining) = front.remaining.as_mut()
    {
        *remaining = remaining.saturating_sub(done);
        if *remaining == 0 {
            p.queue.remove(0);
            recipe_changed = true;
        }
    }
    if recipe_changed {
        p.apply_capacities(pdef, defs, tools_id(defs));
    }
    if let (Some(load), Some(carrier)) = (haul, hauler)
        && !crate::logistics::start_haul(state, id, carrier, load)
        && let Some(p) = state.buildings.get_mut(id).and_then(|b| b.production.as_mut())
    {
        // Cannot happen (workers at work carry nothing); keep the yield in any case.
        p.output.add(load.resource, load.amount);
    }
    for m in &produced {
        state.economy.record_produced(m.resource, m.amount);
    }
    events.push(SimEvent::ProductionCompleted { building: id, outputs: produced });
}
