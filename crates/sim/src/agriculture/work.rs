//! Farm labour every tick: harvesting ripe tiles and sowing (planting) fallow ones.

use glam::IVec2;

use super::growth::{harvest_milli, refresh_stage, ripens_before_winter};
use super::{FarmKind, FarmTask, FieldTile, HarvestRecord, TaskKind, farm_building, farm_crops, is_ripe};
use crate::buildings::BuildingId;
use crate::command::{CommandResult, RejectReason};
use crate::defs::{BuildingDef, CropDef, CropId, Defs, ResourceAmount};
use crate::event::{Notification, Severity, SimEvent};
use crate::production::{MILLI_PER_UNIT, ProductionStatus, update_status, work_rate_permille};
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;

/// Labour of all field farms in this tick (ascending building id) and their status.
pub(super) fn run_work(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    // Farmers only work field tiles they can reach on foot (navigation components).
    crate::navigation::prepare(state, defs);
    let ids: Vec<BuildingId> =
        state.agriculture.farms.iter().filter(|(_, f)| f.kind == FarmKind::Fields).map(|(id, _)| *id).collect();
    for id in ids {
        let Some(def) = state.buildings.get(id).and_then(|b| defs.buildings().get(b.kind().index())) else { continue };
        let task_tile = |state: &GameState| state.agriculture.farms.get(&id).and_then(|f| f.task).map(|t| t.tile);
        let before = task_tile(state);
        if let Some(status) = farm_tick(state, defs, id, def, events) {
            update_status(state, defs, id, def, status, events);
        }
        if task_tile(state) != before {
            // Farmers work on the fields: they follow the task (step 3b).
            crate::population::routine::refresh_workers(state, defs, id);
        }
    }
}

/// Milli-ticks of `hours` farm hours at full productivity.
fn task_ticks(hours: f32) -> u64 {
    (f64::from(hours) * TICKS_PER_HOUR as f64 * f64::from(MILLI_PER_UNIT)).round().max(1.0) as u64
}

fn crop_def(defs: &Defs, crop: Option<CropId>) -> Option<&CropDef> {
    crop.and_then(|c| defs.crops().get(c.index()))
}

/// True if `tile` may be sown with `crop` now as far as season and soil go: fallow, sowing
/// season and (annuals) enough fertility left — below `fallow_below_percent` of the natural
/// fertility it lies fallow.
fn may_sow(state: &GameState, defs: &Defs, tile: &FieldTile, crop: &CropDef) -> bool {
    if tile.crop.is_some() || !crop.can_sow_in(state.clock.season()) {
        return false;
    }
    if crop.perennial {
        return true;
    }
    let natural = u32::from(state.world.tile(tile.pos).fertility);
    u32::from(tile.fertility) * 100 >= natural * u32::from(defs.rules().agriculture.fallow_below_percent)
}

/// [`may_sow`] and — for frost-sensitive annuals — expected to ripen before winter.
fn can_sow(state: &GameState, defs: &Defs, tile: &FieldTile, crop: &CropDef) -> bool {
    may_sow(state, defs, tile, crop) && ripens_before_winter(state.clock, defs.rules(), tile, crop)
}

/// Whole units a harvest of `tile` would add to the output buffer (incl. the carried fraction).
fn harvest_units(state: &GameState, defs: &Defs, carry: u32, tile: &FieldTile, crop: &CropDef) -> u32 {
    let milli = u64::from(carry) + harvest_milli(state, defs, tile, crop, tile.fruit);
    u32::try_from(milli / u64::from(MILLI_PER_UNIT)).unwrap_or(u32::MAX)
}

/// Ticks between two searches for work while a farm is idle (staggered by building index).
const IDLE_CHECK_TICKS: u64 = 20;

/// True if the farmers of farm `id` can walk to field tile `p` (claimed tiles enclosed by
/// buildings are left alone until a way leads there again).
fn reachable(state: &GameState, id: BuildingId, p: IVec2) -> bool {
    crate::population::routine::field_reachable(state, id, p)
}

/// The next piece of work of farm `id`: the first ripe tile whose harvest fits into the
/// barn, else the first tile that can be sown; only tiles the farmers can reach. Also returns
/// whether a ripe tile waits for room in the barn.
fn pick_task(state: &GameState, defs: &Defs, id: BuildingId) -> (Option<FarmTask>, bool) {
    let season = state.clock.season();
    let Some(farm) = state.agriculture.farms.get(&id) else { return (None, false) };
    let output = state.buildings.get(id).and_then(|b| b.production.as_ref()).map(|p| p.output());
    let room = |resource| output.map_or(0, |o| o.capacity(resource).saturating_sub(o.get(resource)));
    let mut blocked = false;
    for tile in farm.tiles.iter().filter(|t| reachable(state, id, t.pos)) {
        let Some(crop) = crop_def(defs, tile.crop) else { continue };
        if is_ripe(tile, crop, season) {
            let carry = farm.carry.get(&crop.output).copied().unwrap_or(0);
            if harvest_units(state, defs, carry, tile, crop) <= room(crop.output) {
                let task = FarmTask { kind: TaskKind::Harvest, tile: tile.pos, progress: 0, required: task_ticks(crop.harvest_hours) };
                return (Some(task), blocked);
            }
            blocked = true;
        }
    }
    let sow = crop_def(defs, farm.crop).and_then(|crop| {
        farm.tiles
            .iter()
            .find(|t| can_sow(state, defs, t, crop) && reachable(state, id, t.pos))
            .map(|t| FarmTask { kind: TaskKind::Sow, tile: t.pos, progress: 0, required: task_ticks(crop.sow_hours) })
    });
    (sow, blocked)
}

/// Status of a farm without a task: a ripe tile waits for room in the barn, crops are
/// growing, or there is nothing to do in this season.
fn no_task_status(blocked: bool, growing: bool) -> ProductionStatus {
    if blocked {
        ProductionStatus::OutputFull
    } else if growing {
        ProductionStatus::Growing
    } else {
        ProductionStatus::OutOfSeason
    }
}

/// One tick of farm `id`: keeps or picks a task and works on it. Returns the new status, or
/// `None` to keep the current one (an idle farm looks for work every [`IDLE_CHECK_TICKS`]).
fn farm_tick(
    state: &mut GameState,
    defs: &Defs,
    id: BuildingId,
    def: &BuildingDef,
    events: &mut Vec<SimEvent>,
) -> Option<ProductionStatus> {
    let b = state.buildings.get(id)?;
    if b.paused {
        return Some(ProductionStatus::Paused);
    }
    let staffed = b.staffed();
    let status = b.production.as_ref().map_or(ProductionStatus::Idle, |p| p.status());
    let season = state.clock.season();
    let farm = state.agriculture.farms.get(&id)?;
    if farm.tiles.is_empty() {
        return Some(ProductionStatus::NoResourcesInRange);
    }
    if farm.task.is_some_and(|task| !reachable(state, id, task.tile)) {
        // Something was built around the task's tile: nobody can get there any more.
        if let Some(f) = state.agriculture.farms.get_mut(&id) {
            f.task = None;
        }
    }
    let farm = state.agriculture.farms.get(&id)?;
    let idle = matches!(status, ProductionStatus::Growing | ProductionStatus::OutOfSeason | ProductionStatus::OutputFull);
    let idle_check_due = (state.clock.tick + u64::from(id.index())) % IDLE_CHECK_TICKS == 0;
    if staffed == 0 {
        // The status tells whether there is work even without workers: farms with nothing to
        // do are staffed last (crate::workforce), so it must not flip when they lose them.
        if idle && !idle_check_due {
            return None;
        }
        let (task, blocked) = pick_task(state, defs, id);
        let growing = farm.tiles.iter().any(|t| t.crop.is_some());
        return Some(if task.is_some() { ProductionStatus::NoWorkers } else { no_task_status(blocked, growing) });
    }
    if crate::population::present_workers(state, defs, id) == 0 {
        // Workers on the way, at lunch or at home: the task rests; a farm without work keeps
        // its idle status as above.
        if idle && !idle_check_due {
            return None;
        }
        let (task, blocked) = pick_task(state, defs, id);
        let growing = farm.tiles.iter().any(|t| t.crop.is_some());
        let away = crate::population::absence_status(state, defs, id);
        let planned = farm.task.is_some();
        if !planned && let Some(f) = state.agriculture.farms.get_mut(&id) {
            // The next task is planned (and reported) even before the workers arrive.
            f.task = task;
        }
        return Some(if task.is_some() || planned { away } else { no_task_status(blocked, growing) });
    }
    // Is the running task still meaningful? (Sowing was checked for the season's remaining
    // growth when it was picked.)
    let valid = farm.task.is_some_and(|task| {
        farm.tiles.iter().find(|t| t.pos == task.tile).is_some_and(|tile| match task.kind {
            TaskKind::Sow => crop_def(defs, farm.crop).is_some_and(|c| may_sow(state, defs, tile, c)),
            TaskKind::Harvest => crop_def(defs, tile.crop).is_some_and(|crop| is_ripe(tile, crop, season)),
        })
    });
    let had_task = farm.task.is_some();
    let task = if valid {
        farm.task
    } else {
        if idle && !had_task && !idle_check_due {
            return None;
        }
        let (task, blocked) = pick_task(state, defs, id);
        if task.is_none() {
            let growing = farm.tiles.iter().any(|t| t.crop.is_some());
            if let Some(f) = state.agriculture.farms.get_mut(&id) {
                f.task = None;
            }
            return Some(no_task_status(blocked, growing));
        }
        task
    };
    let mut task = task?;
    let rate = work_rate_permille(state, defs, def, id, true, 1000);
    if let Some(p) = state.buildings.get_mut(id).and_then(|b| b.production.as_mut()) {
        p.last_productivity_permille = u32::try_from(rate).unwrap_or(u32::MAX);
    }
    task.progress = task.progress.saturating_add(crate::production::shift_progress(defs, rate));
    let done = task.progress >= task.required;
    if let Some(f) = state.agriculture.farms.get_mut(&id) {
        f.task = if done { None } else { Some(task) };
    }
    if done {
        match task.kind {
            TaskKind::Sow => sow(state, defs, id, task.tile),
            TaskKind::Harvest => harvest(state, defs, id, def, task.tile, events),
        }
        // Straight on to the next tile.
        let (next, _) = pick_task(state, defs, id);
        if let Some(f) = state.agriculture.farms.get_mut(&id) {
            f.task = next;
        }
    }
    Some(ProductionStatus::Working)
}

fn sow(state: &mut GameState, defs: &Defs, id: BuildingId, p: IVec2) {
    let season = state.clock.season();
    let Some(farm) = state.agriculture.farms.get_mut(&id) else { return };
    let crop = farm.crop;
    let Some(tile) = farm.tiles.iter_mut().find(|t| t.pos == p) else { return };
    tile.crop = crop;
    tile.growth = 0;
    tile.fruit = 0;
    tile.condition = 100;
    if refresh_stage(tile, defs, season) {
        state.agriculture.mark_dirty(p);
    }
}

fn harvest(state: &mut GameState, defs: &Defs, id: BuildingId, def: &BuildingDef, p: IVec2, events: &mut Vec<SimEvent>) {
    let season = state.clock.season();
    let day = state.clock.total_days();
    let Some(farm) = state.agriculture.farms.get(&id) else { return };
    let Some(tile) = farm.tiles.iter().find(|t| t.pos == p) else { return };
    let Some(crop) = crop_def(defs, tile.crop) else { return };
    let resource = crop.output;
    let total = u64::from(farm.carry.get(&resource).copied().unwrap_or(0)) + harvest_milli(state, defs, tile, crop, tile.fruit);
    let units = u32::try_from(total / u64::from(MILLI_PER_UNIT)).unwrap_or(u32::MAX);
    let stored = state
        .buildings
        .get_mut(id)
        .and_then(|b| b.production.as_mut())
        .map_or(0, |prod| {
            let stored = prod.output.add(resource, units);
            prod.made.record_harvest(resource, stored);
            stored
        });
    // Units beyond the buffer stay in the carry (the room was checked when the task started).
    let rest = total - u64::from(stored) * u64::from(MILLI_PER_UNIT);
    let (perennial, exhaustion) = (crop.perennial, crop.soil_exhaustion);
    let Some(farm) = state.agriculture.farms.get_mut(&id) else { return };
    farm.carry.insert(resource, u32::try_from(rest).unwrap_or(u32::MAX));
    let mut redraw = false;
    if let Some(tile) = farm.tiles.iter_mut().find(|t| t.pos == p) {
        if perennial {
            tile.fruit = 0;
        } else {
            tile.crop = None;
            tile.growth = 0;
            tile.fertility = tile.fertility.saturating_sub(exhaustion);
        }
        tile.condition = 100;
        redraw = refresh_stage(tile, defs, season);
    }
    if redraw {
        state.agriculture.mark_dirty(p);
    }
    let Some(farm) = state.agriculture.farms.get_mut(&id) else { return };
    let round = farm.round.get_or_insert(HarvestRecord { day, resource, amount: 0 });
    if round.resource == resource {
        round.amount = round.amount.saturating_add(stored);
        round.day = day;
    }
    // The round lasts while tiles are ripe or about to ripen (stage 3), so a field sown over a
    // few hours is one harvest.
    let more = farm.tiles.iter().any(|t| {
        crop_def(defs, t.crop).is_some_and(|c| is_ripe(t, c, season) || (!c.perennial && t.stage >= 3))
    });
    let finished = if more { None } else { farm.round.take() };
    if let Some(r) = finished {
        farm.last_harvest = Some(r);
    }
    if stored > 0 {
        state.economy.record_produced(resource, stored);
        events.push(SimEvent::ProductionCompleted { building: id, outputs: vec![ResourceAmount { resource, amount: stored }] });
    }
    if let Some(r) = finished {
        let position = state.buildings.get(id).map(|b| b.entrance());
        events.push(SimEvent::HarvestCompleted { building: id, resource: r.resource, amount: r.amount });
        let name = defs.resources().get(r.resource.index()).map_or_else(|| r.resource.to_string(), |d| d.name.clone());
        let mut note = Notification::new(Severity::Good, format!("{}: Ernte eingebracht – {} {name}.", def.name, r.amount));
        if let Some(position) = position {
            note = note.at(position);
        }
        events.push(SimEvent::Notification(note));
    }
}

/// `SetCrop`: the crop a finished field farm sows from now on.
pub(crate) fn set_crop(
    state: &mut GameState,
    defs: &Defs,
    building: BuildingId,
    crop: CropId,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let b = state.buildings.get(building).ok_or(RejectReason::UnknownBuilding)?;
    let (def, kind, ..) = farm_building(defs, b).ok_or(RejectReason::NotAFieldFarm)?;
    if kind != FarmKind::Fields {
        return Err(RejectReason::NotAFieldFarm);
    }
    if !farm_crops(def).contains(&crop) {
        return Err(RejectReason::CropNotGrownHere);
    }
    let farm = state.agriculture.farms.get_mut(&building).ok_or(RejectReason::FarmNotReady)?;
    if farm.crop == Some(crop) {
        return Ok(());
    }
    farm.crop = Some(crop);
    if farm.task.is_some_and(|t| t.kind == TaskKind::Sow) {
        farm.task = None;
    }
    events.push(SimEvent::CropChanged { building, crop });
    Ok(())
}
