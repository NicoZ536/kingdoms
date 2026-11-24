//! Production (docs/game_design.md §5.2, §7): recipes from the data, per-building input/output
//! buffers, work cycles over game time, productivity modifiers, status reasons, recipe
//! selection, production queues and gathering from the surroundings.
//!
//! ## Work cycles
//! A cycle of a recipe takes `hours` game hours of round-the-clock operation at full
//! productivity. People work only during their shift, so every tick a working building adds
//! `1000 × productivity × shift factor` milli-ticks of progress (shift factor = 24 h / shift
//! length, 3 with the shipped 8-hour shift: a full shift yields a day's output, commutes cost
//! output), where productivity is the product of
//! * staffing: sum of the *present* workers' efficiency (health and happiness, docs §6) /
//!   workers — a worker is present only inside the workplace during its working time
//!   ([`crate::population::is_at_work`]); without anyone present the cycle rests
//!   ([`ProductionStatus::WorkersAway`], [`ProductionStatus::LunchBreak`],
//!   [`ProductionStatus::OffShift`]),
//! * tools: 100 %, or `GameRules::no_tools_productivity_percent` (60 %) for buildings that wear
//!   tools but have none (one tool lasts `GameRules::tool_wear_cycles` = 20 cycles),
//! * research: `100 % + ProductionBonus`,
//! * environment: pasture coverage (cattle/sheep) or the bee bonus (apiary),
//! * season/weather: [`season_weather_permille`] (1.0 until the weather phase),
//! * bankruptcy: `GameRules::bankruptcy_productivity_percent` while the treasury is negative.
//!
//! A cycle starts only when its inputs are in the input buffer (fractional amounts such as
//! 0.25 tools are paid as whole units and credited to later cycles), the output buffer has room
//! for the whole result and — for gatherers — a target exists (grown tree, free tile for a
//! sapling, deposit). Inputs are consumed at the start, outputs appear at the end. Carriers
//! ([`crate::logistics`]) fill the input buffers and empty the output buffers.

mod cycle;
mod gathering;
mod output;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use glam::IVec2;
use serde::{Deserialize, Serialize};

pub use cycle::{crew_training_permille, season_weather_permille, staffing_permille, worker_permille};
pub(crate) use cycle::{crew_rate_permille, run_production, shift_progress, update_status, work_rate_permille};
pub use gathering::{is_fellable, is_plantable};
pub use output::{DayOutput, OUTPUT_HISTORY_DAYS, OutputLog};
#[cfg(test)]
pub(crate) use gathering::pasture_permille;

use crate::buildings::{Building, BuildingId};
use crate::command::{CommandResult, RejectReason};
use crate::defs::{BuildingDef, Defs, Gathering, ProductionDef, RecipeDef, ResourceId};
use crate::economy::Inventory;
use crate::event::SimEvent;
use crate::policy::JobPriority;
use crate::state::GameState;

/// Most entries in a production queue.
pub const MAX_QUEUE_LEN: usize = 16;

/// Milli-units per unit (fractional recipe amounts are handled in thousandths).
pub const MILLI_PER_UNIT: u32 = 1000;

/// Priority of a building for workers and carriers (docs/game_design.md §7: niedrig / normal /
/// hoch). Same type as the job category priority.
pub type ProductionPriority = JobPriority;

/// Why a production building is (not) working.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProductionStatus {
    /// The building is still a construction site.
    UnderConstruction,
    /// The building is being upgraded; it works again afterwards.
    Upgrading,
    /// Stopped by the player.
    Paused,
    /// No production order (empty queue).
    Idle,
    /// Nobody works here.
    NoWorkers,
    /// An input is missing in the input buffer.
    MissingInput(ResourceId),
    /// The output buffer has no room for the result.
    OutputFull,
    /// Nothing to gather within the radius (trees, deposits, pasture, farmland).
    NoResourcesInRange,
    /// A forester finds no free tile to plant a sapling on within its radius (the forest is
    /// dense; a wait, not a shortage).
    NoPlantingSpace,
    /// A field farm waits for its crops to ripen.
    Growing,
    /// A field farm has nothing to sow or harvest in this season (e.g. winter).
    OutOfSeason,
    /// A cycle (or a farm's sowing/harvesting) is running.
    Working,
    /// Workers are assigned but none has arrived yet during working time.
    WorkersAway,
    /// The workers have their lunch break.
    LunchBreak,
    /// Outside working time.
    OffShift,
}

impl ProductionStatus {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            ProductionStatus::UnderConstruction => "Im Bau",
            ProductionStatus::Upgrading => "Wird ausgebaut",
            ProductionStatus::Paused => "Stillgelegt",
            ProductionStatus::Idle => "Keine Aufträge",
            ProductionStatus::NoWorkers => "Keine Arbeiter",
            ProductionStatus::MissingInput(_) => "Rohstoffmangel",
            ProductionStatus::OutputFull => "Ausgangslager voll",
            ProductionStatus::NoResourcesInRange => "Keine Rohstoffe im Umkreis",
            ProductionStatus::NoPlantingSpace => "Kein freier Platz zum Pflanzen",
            ProductionStatus::Growing => "Felder wachsen",
            ProductionStatus::OutOfSeason => "Ruht bis zur nächsten Saison",
            ProductionStatus::Working => "In Arbeit",
            ProductionStatus::WorkersAway => "Arbeiter auf dem Weg",
            ProductionStatus::LunchBreak => "Mittagspause",
            ProductionStatus::OffShift => "Feierabend",
        }
    }

    /// True if a cycle is progressing.
    pub const fn is_working(self) -> bool {
        matches!(self, ProductionStatus::Working)
    }
}

/// One production order: a recipe for a number of units or continuously.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEntry {
    /// Index into the building type's recipes.
    pub recipe: u8,
    /// Units of the recipe's first output still to produce (cycles for recipes without
    /// outputs); `None` = continuous (never finishes).
    pub remaining: Option<u32>,
}

/// A running work cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cycle {
    /// Recipe being produced (fixed when the cycle started).
    pub recipe: u8,
    /// Progress in milli-ticks.
    pub progress: u64,
    /// Milli-ticks needed at 100 % productivity.
    pub required: u64,
    /// True if a tool is used for this cycle (otherwise reduced productivity).
    pub with_tools: bool,
    /// Output bonus from consumed optional inputs, in percent.
    pub bonus_percent: u16,
    /// Environment factor in permille (pasture coverage, bee bonus; 1000 = neutral).
    pub environment_permille: u16,
    /// Tile worked by a gatherer (tree, sapling spot, deposit).
    pub target: Option<IVec2>,
    /// Walkable tile the gatherers stand on while working on `target` (`None`: they work
    /// inside the building, e.g. mines; step 3b).
    pub spot: Option<IVec2>,
}

impl Cycle {
    /// Progress 0.0–1.0.
    pub fn fraction(&self) -> f32 {
        (self.progress as f64 / self.required.max(1) as f64).min(1.0) as f32
    }
}

/// Production state of a building whose type has a [`ProductionDef`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionState {
    pub(crate) queue: Vec<QueueEntry>,
    pub(crate) cycle: Option<Cycle>,
    pub(crate) status: ProductionStatus,
    pub(crate) input: Inventory,
    pub(crate) output: Inventory,
    /// Input milli-units paid ahead (whole units consumed for fractional recipe amounts).
    pub(crate) credits: BTreeMap<ResourceId, u32>,
    /// Output milli-units produced but not yet whole units.
    pub(crate) pending: BTreeMap<ResourceId, u32>,
    /// Cycles left on the current tool.
    pub(crate) tool_charge: u16,
    pub(crate) cycles_completed: u64,
    /// Cycles and goods of today and the last closed days (measured output, §6.3).
    pub(crate) made: OutputLog,
    pub(crate) last_productivity_permille: u32,
    /// Tick since which the current problem status (missing input, nothing to gather) lasts.
    pub(crate) problem_since: Option<u64>,
    pub(crate) last_warning_tick: Option<u64>,
}

impl ProductionState {
    /// Initial state: continuous production of the first recipe (if any), empty buffers.
    pub(crate) fn new(def: &ProductionDef) -> Self {
        let queue = if def.recipes.is_empty() { Vec::new() } else { vec![QueueEntry { recipe: 0, remaining: None }] };
        Self {
            queue,
            cycle: None,
            status: ProductionStatus::UnderConstruction,
            input: Inventory::new(),
            output: Inventory::new(),
            credits: BTreeMap::new(),
            pending: BTreeMap::new(),
            tool_charge: 0,
            cycles_completed: 0,
            made: OutputLog::default(),
            last_productivity_permille: 0,
            problem_since: None,
            last_warning_tick: None,
        }
    }

    /// Current status.
    pub fn status(&self) -> ProductionStatus {
        self.status
    }

    /// The running cycle.
    pub fn cycle(&self) -> Option<&Cycle> {
        self.cycle.as_ref()
    }

    /// Production orders; the first one is active.
    pub fn queue(&self) -> &[QueueEntry] {
        &self.queue
    }

    /// Index of the recipe the next cycle will use (`None` without orders).
    pub fn active_recipe(&self) -> Option<u8> {
        self.queue.first().map(|e| e.recipe)
    }

    /// Input buffer.
    pub fn input(&self) -> &Inventory {
        &self.input
    }

    /// Output buffer.
    pub fn output(&self) -> &Inventory {
        &self.output
    }

    /// Cycles left on the current tool (0 = the next cycle needs a new tool).
    pub fn tool_charge(&self) -> u16 {
        self.tool_charge
    }

    /// Completed cycles since the building was finished.
    pub fn cycles_completed(&self) -> u64 {
        self.cycles_completed
    }

    /// Work cycles and goods made today and on the last closed days (measured output).
    pub fn output_log(&self) -> &OutputLog {
        &self.made
    }

    /// Productivity of the last working tick in permille (0 while not working).
    pub fn productivity_permille(&self) -> u32 {
        self.last_productivity_permille
    }

    /// Sets the buffer capacities for the active recipe: every input and optional input of the
    /// recipe gets the building's buffer capacity, tools (for tool-wearing buildings) at least
    /// `GameRules::tools_buffer`; outputs of all recipes get the buffer capacity. Contents are
    /// kept; inputs no longer needed become surplus that carriers bring back to storage.
    pub(crate) fn apply_capacities(&mut self, def: &ProductionDef, defs: &Defs, tools: Option<ResourceId>) {
        let rules = defs.rules();
        let buffer = u32::from(def.buffer_capacity);
        let mut inputs: BTreeMap<ResourceId, u32> = BTreeMap::new();
        if let Some(recipe) = self.active_recipe().and_then(|i| def.recipes.get(usize::from(i))) {
            for r in &recipe.inputs {
                inputs.insert(r.resource, buffer);
            }
            for o in &recipe.optional_inputs {
                inputs.insert(o.resource, buffer);
            }
        }
        if def.uses_tools
            && let Some(tools) = tools
        {
            let slot = inputs.entry(tools).or_insert(0);
            *slot = (*slot).max(u32::from(rules.tools_buffer));
        }
        self.input.set_capacities(inputs);
        let mut outputs: BTreeMap<ResourceId, u32> =
            def.recipes.iter().flat_map(|r| r.outputs.iter()).map(|o| (o.resource, buffer)).collect();
        // Field farms: every crop they can grow, with room for a harvest.
        if let Some(Gathering::Fields { crops, .. }) = &def.gathering {
            let harvest = u32::from(rules.agriculture.harvest_buffer);
            for crop in crops.iter().filter_map(|c| defs.crops().get(c.index())) {
                outputs.insert(crop.output, harvest);
            }
        }
        self.output.set_capacities(outputs);
    }

    /// Checks indices against the building type (used when validating a loaded state).
    pub(crate) fn validate(&self, def: &ProductionDef, resources: usize) -> Result<(), String> {
        if self.queue.len() > MAX_QUEUE_LEN {
            return Err("production queue too long".into());
        }
        if let Some(e) = self.queue.iter().find(|e| usize::from(e.recipe) >= def.recipes.len()) {
            return Err(format!("unknown recipe index {}", e.recipe));
        }
        if self.queue.iter().any(|e| e.remaining == Some(0)) {
            return Err("finished queue entry kept".into());
        }
        if let Some(c) = &self.cycle
            && (usize::from(c.recipe) >= def.recipes.len() || c.required == 0)
        {
            return Err("invalid running cycle".into());
        }
        let ids_ok = |inv: &Inventory| inv.is_canonical() && inv.max_resource_index().is_none_or(|i| i < resources);
        if !ids_ok(&self.input) || !ids_ok(&self.output) {
            return Err("malformed production buffer".into());
        }
        if self.credits.keys().chain(self.pending.keys()).any(|r| r.index() >= resources) {
            return Err("unknown resource in production accumulators".into());
        }
        self.made.validate(resources)
    }
}

/// Milli-units of a fractional recipe amount.
pub(crate) fn milli(amount: f32) -> u32 {
    (f64::from(amount) * f64::from(MILLI_PER_UNIT)).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// The tools resource of the data (`None` if the data has none).
pub(crate) fn tools_id(defs: &Defs) -> Option<ResourceId> {
    defs.resource_id("tools")
}

/// Building type and production definition of a building.
pub(crate) fn production_def<'a>(defs: &'a Defs, building: &Building) -> Option<(&'a BuildingDef, &'a ProductionDef)> {
    let def = defs.buildings().get(building.kind().index())?;
    def.production.as_ref().map(|p| (def, p))
}

/// Recipe of a building type by index.
pub(crate) fn recipe(def: &ProductionDef, index: u8) -> Option<&RecipeDef> {
    def.recipes.get(usize::from(index))
}

/// Sets up the buffers of a building that just became active (completion, scenario start).
pub(crate) fn init_production(building: &mut Building, defs: &Defs) {
    let Some(pdef) = defs.buildings().get(building.kind().index()).and_then(|d| d.production.as_ref()) else {
        return;
    };
    let tools = tools_id(defs);
    if let Some(p) = building.production.as_mut() {
        p.apply_capacities(pdef, defs, tools);
    }
}

fn production_mut<'a>(
    state: &'a mut GameState,
    defs: &'a Defs,
    building: BuildingId,
) -> Result<(&'a mut ProductionState, &'a ProductionDef), RejectReason> {
    let b = state.buildings.get_mut(building).ok_or(RejectReason::UnknownBuilding)?;
    let pdef = defs.buildings().get(b.kind().index()).and_then(|d| d.production.as_ref()).ok_or(RejectReason::NoProduction)?;
    let p = b.production.as_mut().ok_or(RejectReason::NoProduction)?;
    Ok((p, pdef))
}

fn check_recipe(pdef: &ProductionDef, recipe: u8) -> CommandResult {
    if usize::from(recipe) < pdef.recipes.len() { Ok(()) } else { Err(RejectReason::UnknownRecipe { recipe }) }
}

/// `SetRecipe`: replaces the queue with continuous production of `recipe`.
pub(crate) fn set_recipe(
    state: &mut GameState,
    defs: &Defs,
    building: BuildingId,
    recipe: u8,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let tools = tools_id(defs);
    let (p, pdef) = production_mut(state, defs, building)?;
    check_recipe(pdef, recipe)?;
    let queue = vec![QueueEntry { recipe, remaining: None }];
    if p.queue == queue {
        return Ok(());
    }
    p.queue = queue;
    p.apply_capacities(pdef, defs, tools);
    events.push(SimEvent::RecipeChanged { building, recipe });
    Ok(())
}

/// `QueueProduction`: adds an order. Finite orders run before a continuous order at the end of
/// the queue; a continuous order replaces an existing one.
pub(crate) fn queue_production(
    state: &mut GameState,
    defs: &Defs,
    building: BuildingId,
    recipe: u8,
    units: Option<u32>,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let tools = tools_id(defs);
    let (p, pdef) = production_mut(state, defs, building)?;
    check_recipe(pdef, recipe)?;
    if units == Some(0) {
        return Err(RejectReason::InvalidQuantity);
    }
    let entry = QueueEntry { recipe, remaining: units };
    match units {
        None => {
            p.queue.retain(|e| e.remaining.is_some());
            if p.queue.len() >= MAX_QUEUE_LEN {
                return Err(RejectReason::QueueFull { max: MAX_QUEUE_LEN as u8 });
            }
            p.queue.push(entry);
        }
        Some(_) => {
            if p.queue.len() >= MAX_QUEUE_LEN {
                return Err(RejectReason::QueueFull { max: MAX_QUEUE_LEN as u8 });
            }
            let at = p.queue.iter().position(|e| e.remaining.is_none()).unwrap_or(p.queue.len());
            p.queue.insert(at, entry);
        }
    }
    p.apply_capacities(pdef, defs, tools);
    events.push(SimEvent::ProductionQueueChanged { building });
    Ok(())
}

/// `ClearQueue`: removes every order; the building idles after the running cycle.
pub(crate) fn clear_queue(
    state: &mut GameState,
    defs: &Defs,
    building: BuildingId,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let tools = tools_id(defs);
    let (p, pdef) = production_mut(state, defs, building)?;
    if p.queue.is_empty() {
        return Ok(());
    }
    p.queue.clear();
    p.apply_capacities(pdef, defs, tools);
    events.push(SimEvent::ProductionQueueChanged { building });
    Ok(())
}

/// `SetProductionPriority`: priority of a building for workers and carriers.
pub(crate) fn set_priority(
    state: &mut GameState,
    building: BuildingId,
    priority: JobPriority,
    events: &mut Vec<SimEvent>,
) -> CommandResult {
    let b = state.buildings.get_mut(building).ok_or(RejectReason::UnknownBuilding)?;
    if b.priority == priority {
        return Ok(());
    }
    b.priority = priority;
    state.workforce.mark_dirty();
    events.push(SimEvent::BuildingPriorityChanged { building, priority });
    Ok(())
}

/// Closes the day of every finished production building's output record (statistics step at
/// midnight; buildings under construction start counting once finished).
pub(crate) fn close_output_day(state: &mut GameState) {
    for (_, b) in state.buildings.iter_mut() {
        if !b.is_complete() {
            continue;
        }
        if let Some(p) = b.production.as_mut() {
            p.made.close_day();
        }
    }
}
