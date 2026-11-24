//! Player tools on the map: inspect, build, roads, demolish, move and the order tool that picks
//! the target of a company's order (a building or area to defend, the waypoints of a patrol).
//!
//! The [`ToolController`] is a small state machine fed once per frame with the filtered input of
//! that frame ([`ToolInput`]: what lies under the cursor, button presses that began on the map,
//! rotation and cancel requests). It never changes the simulation itself: everything the player
//! does becomes a [`Command`] in the returned [`ToolOutput`], which the scene puts into the
//! session's command queue. The previews (placement check, planned road, suggested access road)
//! come from the simulation's own validation functions and are cached, so the expensive ones run
//! only when their inputs change (the road planner only when the drag's end tile changes).
//!
//! Rendering of the previews lives in [`crate::view::preview`], tooltips in the HUD.

#[cfg(test)]
mod tests;

use glam::IVec2;
use kingdoms_sim::buildings::{
    BuildingId, EntranceStatus, PlacementCheck, PlacementWarning, Rotation, SitePurpose, TileBlock, TileStatus, check_move,
    check_placement, move_allowed,
};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingRole, BuildingTypeId, Defs, RoadTypeId};
use kingdoms_sim::grid::TileRect;
use kingdoms_sim::military::{self, DefendTarget, Order};
use kingdoms_sim::roads::{RoadPlan, plan_access_road, plan_road, validate_road_tiles};
use kingdoms_sim::population::CitizenId;
use kingdoms_sim::state::GameState;

use crate::session::Session;
use crate::view::picking::Selection;

/// Ticks after which an unchanged placement preview is checked again (stock and treasury move).
const RECHECK_TICKS: u64 = 10;
/// Ticks after which an unchanged access-road suggestion is planned again.
const REPLAN_ACCESS_TICKS: u64 = 100;
/// Gold value from which demolishing asks for confirmation.
const CONFIRM_GOLD_VALUE: u32 = 100;
/// Why a spot is refused while an order for it waits for the next tick (the game is paused).
pub const QUEUED_CONFLICT: &str = "Hier ist bereits ein Bau vorgemerkt – er beginnt, sobald das Spiel weiterläuft.";

/// The active tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Hover information and selection (default).
    Inspect,
    /// Places buildings of a type.
    Build { kind: BuildingTypeId },
    /// Draws roads of a type by dragging.
    Road { road: RoadTypeId },
    /// Removes buildings (click) and roads (click or drag).
    Demolish,
    /// Moves a finished building: pick it up (`None`), then place it.
    Move { building: Option<BuildingId> },
    /// Picks the place of a company's order on the map.
    Order { company: u32, pick: OrderPick },
}

/// What the order tool picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderPick {
    /// A building to garrison (click on it) or an area of `radius` tiles (click on the ground).
    Defend { radius: u8 },
    /// The waypoints of a patrol, one per click; finished with Enter, the „Fertig“ button or a
    /// click on the first point.
    Patrol,
}

/// Input of one frame for the tools. Map interactions are only reported while the pointer is
/// over the map; releases are always reported (a drag may end over the UI).
#[derive(Clone, Copy, Debug, Default)]
pub struct ToolInput {
    /// Ground tile under the cursor (`None` over the UI or outside the map).
    pub tile: Option<IVec2>,
    /// Building under the cursor.
    pub building: Option<BuildingId>,
    /// Citizen under the cursor (in front of the building, if any).
    pub citizen: Option<CitizenId>,
    /// The primary button went down over the map this frame.
    pub pressed: bool,
    /// The primary button is held.
    pub down: bool,
    /// The primary button was released this frame.
    pub released: bool,
    /// The release ended a drag (the pointer moved beyond the click threshold).
    pub dragged: bool,
    /// The pointer is over the UI.
    pub over_ui: bool,
    /// Right-click on the map (without dragging the camera).
    pub secondary_click: bool,
    /// Rotation request: +1 clockwise, -1 counter-clockwise.
    pub rotate: i8,
    /// Shift is held (keep building after a placement).
    pub shift: bool,
}

/// A demolition the player has to confirm first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemolishRequest {
    pub building: BuildingId,
}

/// What a tool did this frame.
#[derive(Debug, Default, PartialEq)]
pub struct ToolOutput {
    /// Commands for the session's queue, in order.
    pub commands: Vec<Command>,
    /// UI sound ids to play.
    pub sounds: Vec<&'static str>,
    /// Ask before demolishing.
    pub confirm: Option<DemolishRequest>,
    /// New selection (`Some(None)` clears it).
    pub selection: Option<Option<Selection>>,
    /// Open the citizen window for this citizen (inspect tool clicked a walker).
    pub inspect_citizen: Option<CitizenId>,
    /// Short message for the player (toast).
    pub message: Option<String>,
    /// The order tool sent its command (the military tab opens again).
    pub order_given: bool,
}

/// Cached placement (or move) check under the cursor.
#[derive(Clone, Debug)]
pub struct PlacementPreview {
    /// Building type shown.
    pub kind: BuildingTypeId,
    /// The building being moved (moves only).
    pub moving: Option<BuildingId>,
    /// The simulation's verdict, tiles, cost and reasons.
    pub check: PlacementCheck,
    /// Suggested free access road from the entrance to the network (new buildings only).
    pub access: Option<RoadPlan>,
    /// True if `access` was planned (a valid spot with a free entrance); false if planning did
    /// not apply then.
    access_planned: bool,
    /// Part of the footprint is claimed by a building or road still waiting in the command
    /// queue ([`QUEUED_CONFLICT`]); `check` marks those tiles as blocked.
    pub queued_conflict: bool,
    checked_at: u64,
    planned_at: u64,
}

/// A road being dragged.
#[derive(Clone, Debug)]
pub struct RoadDrag {
    pub start: IVec2,
    pub end: IVec2,
    /// The planner's route from `start` to `end` with cost and problems.
    pub plan: RoadPlan,
    /// The route crosses a building still waiting in the command queue ([`QUEUED_CONFLICT`]);
    /// those tiles are marked in `plan`.
    pub queued_conflict: bool,
}

/// A demolition drag (removes the roads along an L-shaped line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DemolishDrag {
    pub start: IVec2,
    pub end: IVec2,
}

/// The tool state machine.
pub struct ToolController {
    tool: Tool,
    rotation: Rotation,
    /// The primary button went down over the map and may become a click.
    armed: bool,
    placement: Option<PlacementPreview>,
    road_drag: Option<RoadDrag>,
    /// Single-tile validation of the hovered tile while not dragging: (tile, tick, plan, the
    /// tile lies on a queued building).
    road_hover: Option<(IVec2, u64, RoadPlan, bool)>,
    demolish_drag: Option<DemolishDrag>,
    /// Waypoints set so far with the patrol order tool.
    patrol: Vec<IVec2>,
    /// Road plans behind commands this controller sent that still wait in the session's queue
    /// (the access road of a placement, a dragged road): the map shows them as pending.
    recorded: Vec<(Command, RoadPlan)>,
}

impl Default for ToolController {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolController {
    pub fn new() -> Self {
        Self {
            tool: Tool::Inspect,
            rotation: Rotation::default(),
            armed: false,
            placement: None,
            road_drag: None,
            road_hover: None,
            demolish_drag: None,
            patrol: Vec::new(),
            recorded: Vec::new(),
        }
    }

    /// The active tool.
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// Rotation used by the build and move tools.
    pub fn rotation(&self) -> Rotation {
        self.rotation
    }

    /// Placement or move preview under the cursor.
    pub fn placement(&self) -> Option<&PlacementPreview> {
        self.placement.as_ref()
    }

    /// The road being dragged.
    pub fn road_drag(&self) -> Option<&RoadDrag> {
        self.road_drag.as_ref()
    }

    /// Validation of the hovered tile in the road tool (before dragging).
    pub fn road_hover(&self) -> Option<&RoadPlan> {
        self.road_hover.as_ref().map(|(_, _, plan, _)| plan)
    }

    /// The road being dragged (or the hovered tile) crosses a building still waiting in the
    /// command queue ([`QUEUED_CONFLICT`]).
    pub fn road_queued_conflict(&self) -> bool {
        match (&self.road_drag, &self.road_hover) {
            (Some(drag), _) => drag.queued_conflict,
            (None, Some((_, _, _, conflict))) => *conflict,
            (None, None) => false,
        }
    }

    /// The demolition drag.
    pub fn demolish_drag(&self) -> Option<DemolishDrag> {
        self.demolish_drag
    }

    /// Waypoints of the patrol being picked.
    pub fn patrol_points(&self) -> &[IVec2] {
        &self.patrol
    }

    /// The road plan behind `command` while it waits in the queue: the access road of a
    /// placement or a dragged road (`None` for commands sent elsewhere).
    pub fn recorded_plan(&self, command: &Command) -> Option<&RoadPlan> {
        self.recorded.iter().find(|(c, _)| c == command).map(|(_, plan)| plan)
    }

    /// Switches to `tool`, dropping drags and previews of the previous one. Returns true if
    /// the tool changed.
    pub fn select(&mut self, tool: Tool) -> bool {
        if self.tool == tool {
            return false;
        }
        self.tool = tool;
        self.armed = false;
        self.placement = None;
        self.road_drag = None;
        self.road_hover = None;
        self.demolish_drag = None;
        self.patrol.clear();
        true
    }

    /// Picks up `building` with the move tool (from the info panel or `M`). Returns the reason
    /// in German if the building cannot be moved.
    pub fn start_move(&mut self, state: &GameState, defs: &Defs, building: BuildingId) -> Result<(), String> {
        movable(state, defs, building)?;
        if let Some(b) = state.building(building) {
            self.rotation = b.rotation();
        }
        self.select(Tool::Move { building: Some(building) });
        Ok(())
    }

    /// Cancels the innermost step: a drag, a picked-up building, then the tool itself. Returns
    /// false if there was nothing to cancel (inspect without drag).
    pub fn cancel(&mut self) -> bool {
        if self.road_drag.take().is_some() || self.demolish_drag.take().is_some() {
            self.armed = false;
            return true;
        }
        match self.tool {
            Tool::Inspect => false,
            Tool::Order { pick: OrderPick::Patrol, .. } if !self.patrol.is_empty() => {
                self.patrol.pop();
                true
            }
            Tool::Move { building: Some(_) } => {
                self.select(Tool::Move { building: None });
                true
            }
            _ => {
                self.select(Tool::Inspect);
                true
            }
        }
    }

    /// Rotates the build/move preview. Returns true if the active tool uses rotation.
    pub fn rotate(&mut self, steps: i8) -> bool {
        if !self.uses_rotation() || steps == 0 {
            return false;
        }
        for _ in 0..steps.unsigned_abs() {
            self.rotation = if steps > 0 { self.rotation.rotated_cw() } else { self.rotation.rotated_ccw() };
        }
        true
    }

    /// True if the tool rotates with `R` / Shift + mouse wheel.
    pub fn uses_rotation(&self) -> bool {
        matches!(self.tool, Tool::Build { .. } | Tool::Move { building: Some(_) })
    }

    /// Processes one frame of input.
    pub fn update(&mut self, session: &Session, input: &ToolInput) -> ToolOutput {
        let mut out = ToolOutput::default();
        if input.rotate != 0 && self.rotate(input.rotate) {
            out.sounds.push("ui_rotate");
        }
        if input.secondary_click && self.cancel() {
            out.sounds.push("ui_close");
        }
        let click = self.track_click(input);
        let state = session.state();
        let defs = session.defs();
        let tick = state.clock().tick;
        let queued = session.queued_commands();
        self.recorded.retain(|(command, _)| queued.contains(command));
        let claims = QueuedClaims::new(state, defs, queued);
        match self.tool {
            Tool::Inspect => {
                // A walker in front of everything else is inspected; the selection stays.
                if click && let Some(citizen) = input.citizen {
                    out.sounds.push("ui_select");
                    out.inspect_citizen = Some(citizen);
                } else if click {
                    let hovered = match (input.building, input.tile) {
                        (Some(b), _) => Some(Selection::Building(b)),
                        (None, Some(t)) => Some(Selection::Tile(t)),
                        (None, None) => None,
                    };
                    if hovered.is_some() {
                        out.sounds.push("ui_select");
                    }
                    out.selection = Some(hovered);
                }
            }
            Tool::Build { kind } => self.update_build(state, defs, &claims, kind, input, click, tick, &mut out),
            Tool::Road { road } => self.update_road(state, defs, &claims, road, input, tick, &mut out),
            Tool::Demolish => self.update_demolish(state, defs, input, click, &mut out),
            Tool::Move { building } => self.update_move(state, defs, &claims, building, input, click, tick, &mut out),
            Tool::Order { company, pick } => self.update_order(state, defs, company, pick, input, click, &mut out),
        }
        out
    }

    /// Finishes a patrol being picked (Enter, the „Fertig“ button): sends the order if it has
    /// enough waypoints, else explains what is missing.
    pub fn finish_patrol(&mut self, session: &Session) -> ToolOutput {
        let mut out = ToolOutput::default();
        let Tool::Order { company, pick: OrderPick::Patrol } = self.tool else { return out };
        let order = Order::Patrol { route: self.patrol.clone() };
        match military::order_problem(session.state(), session.defs(), company, &order) {
            Some(problem) => {
                out.message = Some(problem);
                out.sounds.push("ui_error");
            }
            None => {
                out.commands.push(Command::CompanyOrder { company, order });
                out.sounds.push("ui_click");
                out.order_given = true;
                self.select(Tool::Inspect);
            }
        }
        out
    }

    /// The order tool: a click picks the building or area to defend, or adds a patrol
    /// waypoint (a click on the first waypoint closes the route).
    #[allow(clippy::too_many_arguments)]
    fn update_order(
        &mut self,
        state: &GameState,
        defs: &Defs,
        company: u32,
        pick: OrderPick,
        input: &ToolInput,
        click: bool,
        out: &mut ToolOutput,
    ) {
        if state.military().company(company).is_none() {
            self.select(Tool::Inspect);
            return;
        }
        if !click {
            return;
        }
        let Some(tile) = input.tile else { return };
        match pick {
            OrderPick::Defend { radius } => {
                let order = defend_order(state, input.building, tile, radius);
                match military::order_problem(state, defs, company, &order) {
                    Some(problem) => {
                        out.message = Some(problem);
                        out.sounds.push("ui_error");
                    }
                    None => {
                        out.commands.push(Command::CompanyOrder { company, order });
                        out.sounds.push("ui_click");
                        out.order_given = true;
                        self.select(Tool::Inspect);
                    }
                }
            }
            OrderPick::Patrol => {
                let max = usize::from(defs.rules().military.max_patrol_points);
                if self.patrol.len() >= 2 && self.patrol.first() == Some(&tile) {
                    let route = Order::Patrol { route: self.patrol.clone() };
                    match military::order_problem(state, defs, company, &route) {
                        Some(problem) => {
                            out.message = Some(problem);
                            out.sounds.push("ui_error");
                        }
                        None => {
                            out.commands.push(Command::CompanyOrder { company, order: route });
                            out.sounds.push("ui_click");
                            out.order_given = true;
                            self.select(Tool::Inspect);
                        }
                    }
                } else if !state.navigation().is_walkable(tile) {
                    out.message = Some("Dieser Wegpunkt ist nicht begehbar.".to_owned());
                    out.sounds.push("ui_error");
                } else if self.patrol.len() >= max {
                    out.message = Some(format!("Eine Patrouille hat höchstens {max} Wegpunkte – Eingabe oder „Fertig“ gibt den Befehl."));
                    out.sounds.push("ui_error");
                } else if self.patrol.last() != Some(&tile) {
                    self.patrol.push(tile);
                    out.sounds.push("ui_select");
                }
            }
        }
    }

    /// Click detection: a press over the map arms, a release without drag over the map fires.
    fn track_click(&mut self, input: &ToolInput) -> bool {
        if input.pressed {
            self.armed = true;
        }
        if input.released {
            let click = self.armed && !input.dragged && !input.over_ui;
            self.armed = false;
            return click;
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn update_build(
        &mut self,
        state: &GameState,
        defs: &Defs,
        claims: &QueuedClaims,
        kind: BuildingTypeId,
        input: &ToolInput,
        click: bool,
        tick: u64,
        out: &mut ToolOutput,
    ) {
        let Some(def) = defs.buildings().get(kind.index()) else {
            self.select(Tool::Inspect);
            return;
        };
        let Some(tile) = input.tile else {
            self.placement = None;
            return;
        };
        let origin = centred_origin(tile, self.rotation.footprint_size(def.size));
        self.refresh_placement(state, defs, claims, kind, None, origin, tick);
        if !click {
            return;
        }
        let Some(preview) = self.placement.as_ref() else { return };
        if preview.queued_conflict {
            // The simulation would answer only once the game runs again.
            out.message = Some(QUEUED_CONFLICT.to_owned());
            out.sounds.push("ui_error");
            return;
        }
        let rotation = self.rotation;
        // The suggested access road is built by the simulation together with the building (and
        // only if the building is placed), planned anew for the state of that tick.
        let access = preview.access.as_ref().filter(|p| p.valid && p.new_tile_count() > 0);
        let command = match access {
            Some(plan) if preview.check.ok => {
                let command = Command::PlaceBuildingWithAccess { kind, origin, rotation, road: plan.road };
                self.recorded.push((command.clone(), plan.clone()));
                command
            }
            _ => Command::PlaceBuilding { kind, origin, rotation },
        };
        out.commands.push(command);
        if preview.check.ok {
            if !input.shift {
                self.select(Tool::Inspect);
            }
            // The placed building may change what is valid around it.
            self.placement = None;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn refresh_placement(
        &mut self,
        state: &GameState,
        defs: &Defs,
        claims: &QueuedClaims,
        kind: BuildingTypeId,
        moving: Option<BuildingId>,
        origin: IVec2,
        tick: u64,
    ) {
        let rotation = self.rotation;
        let same_spot = self.placement.as_ref().is_some_and(|p| {
            p.kind == kind && p.moving == moving && p.check.origin == origin && p.check.rotation == rotation
        });
        let fresh = self.placement.as_ref().is_some_and(|p| tick.saturating_sub(p.checked_at) < RECHECK_TICKS);
        if same_spot && fresh {
            return;
        }
        let mut check = match moving {
            Some(building) => match check_move(state, defs, building, origin, rotation) {
                Some(check) => check,
                None => {
                    // The building is gone.
                    self.select(Tool::Inspect);
                    return;
                }
            },
            None => check_placement(state, defs, kind, origin, rotation),
        };
        let queued_conflict = claims.mark_placement(&mut check, moving);
        // Wall pieces and towers on a wall line are entered from any side: no access road.
        let needs_road = defs.buildings().get(kind.index()).is_some_and(|d| d.needs_road());
        let plan_access = moving.is_none() && needs_road && check.ok && check.entrance_status == EntranceStatus::Free;
        let access = match self.placement.take() {
            // The last plan is kept for a while, but only while planning still applies the
            // same way (a spot that became valid needs a plan, an invalid one shows none).
            Some(old)
                if same_spot
                    && old.access_planned == plan_access
                    && tick.saturating_sub(old.planned_at) < REPLAN_ACCESS_TICKS =>
            {
                self.placement = Some(PlacementPreview { check, checked_at: tick, queued_conflict, ..old });
                return;
            }
            _ if plan_access => access_road_type(state, defs)
                .and_then(|road| plan_access_road(state, defs, kind, origin, rotation, road)),
            _ => None,
        };
        self.placement = Some(PlacementPreview {
            kind,
            moving,
            check,
            access,
            access_planned: plan_access,
            queued_conflict,
            checked_at: tick,
            planned_at: tick,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn update_road(
        &mut self,
        state: &GameState,
        defs: &Defs,
        claims: &QueuedClaims,
        road: RoadTypeId,
        input: &ToolInput,
        tick: u64,
        out: &mut ToolOutput,
    ) {
        if input.pressed
            && let Some(tile) = input.tile
        {
            let mut plan = plan_between(state, defs, tile, tile, road);
            let queued_conflict = claims.mark_road(&mut plan);
            self.road_drag = Some(RoadDrag { start: tile, end: tile, plan, queued_conflict });
        }
        if let Some(drag) = self.road_drag.as_mut() {
            if input.down
                && let Some(tile) = input.tile
                && tile != drag.end
            {
                // The planner is expensive: replan only when the end tile changes.
                drag.end = tile;
                drag.plan = plan_between(state, defs, drag.start, tile, road);
                drag.queued_conflict = claims.mark_road(&mut drag.plan);
            }
            if input.released {
                if let Some(drag) = self.road_drag.take() {
                    if input.over_ui {
                        out.sounds.push("ui_close");
                    } else if drag.queued_conflict {
                        out.message = Some(QUEUED_CONFLICT.to_owned());
                        out.sounds.push("ui_error");
                    } else if !drag.plan.tiles.is_empty() {
                        let command = Command::BuildRoad { tiles: drag.plan.positions(), road };
                        self.recorded.push((command.clone(), drag.plan));
                        out.commands.push(command);
                    }
                }
                self.road_hover = None;
            }
            return;
        }
        self.road_hover = match input.tile {
            Some(tile) => match self.road_hover.take() {
                Some(cached) if cached.0 == tile && tick.saturating_sub(cached.1) < RECHECK_TICKS => Some(cached),
                _ => {
                    let mut plan = validate_road_tiles(state, defs, &[tile], road);
                    let conflict = claims.mark_road(&mut plan);
                    Some((tile, tick, plan, conflict))
                }
            },
            None => None,
        };
    }

    fn update_demolish(&mut self, state: &GameState, defs: &Defs, input: &ToolInput, click: bool, out: &mut ToolOutput) {
        if input.pressed
            && let Some(tile) = input.tile
        {
            self.demolish_drag = Some(DemolishDrag { start: tile, end: tile });
        }
        if let Some(drag) = self.demolish_drag.as_mut()
            && let Some(tile) = input.tile
        {
            drag.end = tile;
        }
        if !input.released {
            return;
        }
        let drag = self.demolish_drag.take();
        if click {
            if let Some(building) = input.building {
                if needs_confirmation(state, defs, building) {
                    out.confirm = Some(DemolishRequest { building });
                    out.sounds.push("ui_open");
                } else {
                    out.commands.push(Command::DemolishBuilding { building });
                }
            } else if let Some(tile) = input.tile.filter(|t| state.roads().has_road(*t)) {
                out.commands.push(Command::RemoveRoad { tiles: vec![tile] });
            } else {
                out.sounds.push("ui_error");
            }
        } else if let Some(drag) = drag.filter(|_| input.dragged && !input.over_ui) {
            let tiles: Vec<IVec2> = l_line(drag.start, drag.end).into_iter().filter(|t| state.roads().has_road(*t)).collect();
            if !tiles.is_empty() {
                out.commands.push(Command::RemoveRoad { tiles });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_move(
        &mut self,
        state: &GameState,
        defs: &Defs,
        claims: &QueuedClaims,
        building: Option<BuildingId>,
        input: &ToolInput,
        click: bool,
        tick: u64,
        out: &mut ToolOutput,
    ) {
        let Some(id) = building else {
            if click && let Some(target) = input.building {
                match self.start_move(state, defs, target) {
                    Ok(()) => out.sounds.push("ui_select"),
                    Err(reason) => {
                        out.sounds.push("ui_error");
                        out.message = Some(reason);
                    }
                }
            }
            return;
        };
        let Some(b) = state.building(id) else {
            self.select(Tool::Inspect);
            return;
        };
        let Some(def) = defs.buildings().get(b.kind().index()) else { return };
        let Some(tile) = input.tile else {
            self.placement = None;
            return;
        };
        let origin = centred_origin(tile, self.rotation.footprint_size(def.size));
        self.refresh_placement(state, defs, claims, b.kind(), Some(id), origin, tick);
        if !click {
            return;
        }
        if self.placement.as_ref().is_some_and(|p| p.queued_conflict) {
            out.message = Some(QUEUED_CONFLICT.to_owned());
            out.sounds.push("ui_error");
            return;
        }
        out.commands.push(Command::MoveBuilding { building: id, origin, rotation: self.rotation });
        if self.placement.as_ref().is_some_and(|p| p.check.ok) {
            self.select(Tool::Inspect);
            out.selection = Some(Some(Selection::Building(id)));
        }
    }
}

/// A building that a command waiting in the session's queue places or moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedBuilding {
    pub kind: BuildingTypeId,
    pub rotation: Rotation,
    /// The tiles it will occupy.
    pub footprint: TileRect,
    /// The building being moved (`None` for a new building).
    pub moved: Option<BuildingId>,
}

/// The building `command` places or moves (`None` for other commands).
pub fn queued_building(state: &GameState, defs: &Defs, command: &Command) -> Option<QueuedBuilding> {
    let (kind, origin, rotation, moved) = match *command {
        Command::PlaceBuilding { kind, origin, rotation } | Command::PlaceBuildingWithAccess { kind, origin, rotation, .. } => {
            (kind, origin, rotation, None)
        }
        Command::MoveBuilding { building, origin, rotation } => (state.building(building)?.kind(), origin, rotation, Some(building)),
        _ => return None,
    };
    let def = defs.buildings().get(kind.index())?;
    Some(QueuedBuilding { kind, rotation, footprint: rotation.footprint(origin, def.size), moved })
}

/// Tiles claimed by map changes still waiting in the session's command queue. While the game
/// is paused, placements and roads wait for the next tick and the simulation's checks do not
/// see them yet; the build tools treat their tiles as taken, so a second click on the same
/// spot is refused at once instead of being rejected when the game runs again.
struct QueuedClaims {
    /// Footprints of new or moved buildings (with the moved building).
    footprints: Vec<(Option<BuildingId>, TileRect)>,
    /// Tiles of new roads.
    roads: Vec<IVec2>,
}

impl QueuedClaims {
    fn new(state: &GameState, defs: &Defs, queued: &[Command]) -> Self {
        let mut claims = Self { footprints: Vec::new(), roads: Vec::new() };
        for command in queued {
            if let Some(b) = queued_building(state, defs, command) {
                claims.footprints.push((b.moved, b.footprint));
            } else if let Command::BuildRoad { tiles, .. } = command {
                claims.roads.extend_from_slice(tiles);
            }
        }
        claims
    }

    /// Marks the footprint tiles of `check` that queued orders claim as blocked (and the check
    /// as failed). `moving` is the building being moved: its own queued move does not count.
    /// Returns true if any tile was claimed.
    fn mark_placement(&self, check: &mut PlacementCheck, moving: Option<BuildingId>) -> bool {
        let mut conflict = false;
        for (p, status) in &mut check.tile_status {
            let building = self.footprints.iter().any(|(moved, rect)| (moved.is_none() || *moved != moving) && rect.contains(*p));
            let block = if building {
                TileBlock::Building
            } else if self.roads.contains(p) {
                TileBlock::Road
            } else {
                continue;
            };
            *status = TileStatus::Blocked(block);
            conflict = true;
        }
        if conflict {
            check.ok = false;
        }
        conflict
    }

    /// Marks the tiles of `plan` on queued building footprints as invalid (and the plan).
    /// Returns true if any tile was claimed.
    fn mark_road(&self, plan: &mut RoadPlan) -> bool {
        let mut conflict = false;
        for t in &mut plan.tiles {
            if self.footprints.iter().any(|(_, rect)| rect.contains(t.pos)) {
                t.ok = false;
                conflict = true;
            }
        }
        if conflict {
            plan.valid = false;
        }
        conflict
    }
}

/// The defend order for a click: the clicked finished building, else the area around the tile.
pub fn defend_order(state: &GameState, building: Option<BuildingId>, tile: IVec2, radius: u8) -> Order {
    match building.filter(|b| state.building(*b).is_some_and(|b| b.is_complete())) {
        Some(b) => Order::Defend(DefendTarget::Building(b)),
        None => Order::Defend(DefendTarget::Area { center: tile, radius }),
    }
}

/// True if no storage reaches a placement's site, so its building material would never arrive
/// (`PlacementWarning::NoStorageInReach`): the map preview shows such a site amber, the tooltip
/// does not call it suitable and names the reason in red.
pub fn site_unsupplied(check: &PlacementCheck) -> bool {
    check.warnings.contains(&PlacementWarning::NoStorageInReach)
}

/// Footprint origin that centres a footprint of `size` on the cursor tile.
pub fn centred_origin(tile: IVec2, size: IVec2) -> IVec2 {
    tile - (size - IVec2::ONE) / 2
}

/// Tiles of the L-shaped line from `from` to `to` (first along x, then along y).
pub fn l_line(from: IVec2, to: IVec2) -> Vec<IVec2> {
    let mut out = vec![from];
    let mut p = from;
    while p.x != to.x {
        p.x += (to.x - p.x).signum();
        out.push(p);
    }
    while p.y != to.y {
        p.y += (to.y - p.y).signum();
        out.push(p);
    }
    out
}

/// The planner's route (or the single tile) between two tiles.
fn plan_between(state: &GameState, defs: &Defs, start: IVec2, end: IVec2, road: RoadTypeId) -> RoadPlan {
    if start == end { validate_road_tiles(state, defs, &[start], road) } else { plan_road(state, defs, start, end, road) }
}

/// The free, unlocked land road type used for suggested access roads (the footpath).
pub fn access_road_type(state: &GameState, defs: &Defs) -> Option<RoadTypeId> {
    defs.roads()
        .iter()
        .filter(|r| r.bridge.is_none() && state.research().road_unlocked(defs, r.id))
        .find(|r| r.cost_per_tile.gold == 0 && r.cost_per_tile.materials.is_empty())
        .map(|r| r.id)
}

/// Checks the simulation's move preconditions for the pick-up step
/// ([`kingdoms_sim::buildings::move_allowed`]); the error is the German rejection text.
pub fn movable(state: &GameState, defs: &Defs, building: BuildingId) -> Result<(), String> {
    move_allowed(state, defs, building).map_err(|reason| reason.to_string())
}

/// True for demolitions the player confirms first: headquarters, inhabited houses, buildings
/// with goods inside and expensive buildings.
pub fn needs_confirmation(state: &GameState, defs: &Defs, building: BuildingId) -> bool {
    let Some(b) = state.building(building) else { return false };
    let Some(def) = defs.buildings().get(b.kind().index()) else { return false };
    if !b.is_complete() && b.construction_site().is_some_and(|s| s.purpose() == SitePurpose::New) {
        return false;
    }
    def.role == BuildingRole::Headquarters
        || state.population().resident_count(building) > 0
        || !b.all_goods().is_empty()
        || def.cost.gold >= CONFIRM_GOLD_VALUE
}
