//! The in-game world scene: runs the simulation session, shows the map, the HUD and the tool
//! previews, and turns the player's input into tool actions and simulation commands. It also
//! owns the dialogs of the pause menu (save, load, settings, leaving with unsaved progress),
//! the autosave and the automatic pause.

use std::path::PathBuf;
use std::sync::mpsc;

use glam::IVec2;
use kingdoms_engine::audio::MusicMood;
use kingdoms_engine::core::app::EngineContext;
use kingdoms_engine::core::scene::{Scene, Transition};
use kingdoms_engine::core::time::GameSpeed;
use kingdoms_engine::egui;
use kingdoms_engine::input::{ActionMap, Binding, Input, KeyCode, MouseButton};
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso;
use kingdoms_engine::rendering::sprite::RenderList;
use kingdoms_sim::command::Command;
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::population::{CitizenId, WalkerId};
use kingdoms_sim::time::{TICKS_PER_DAY, TICKS_PER_HOUR};

use crate::actions::Action;
use crate::game::GameShared;
use crate::save::{self, SaveJob, SaveWriter, SlotKind};
use crate::scenes::auto_pause::{AutoPause, PauseReason, pause_reason};
use crate::scenes::main_menu::MainMenuScene;
use crate::session::{EventCursor, FollowTarget, LaunchOptions, QuickStart, Session, ToolSpec};
use crate::ui::menu::save_browser::{BrowserAction, BrowserMode, SaveBrowser};
use crate::ui::menu::settings_panel::{SettingsPanel, SettingsTab};
use crate::ui::menu::{self, PAGE_NARROW, PAGE_WIDE};
use crate::sound::GameAudio;
use crate::tools::{Tool, ToolController, ToolInput, ToolOutput};
use crate::ui::debug::{self, DebugInfo};
use crate::ui::hud::{BuildTab, Closed, Hud, HudAction, HudInput, Icons, Minimap, RightPanel, ToolRequest, WindowTab};
use crate::view::camera::CameraController;
use crate::view::citizens::{self, WalkerMarks};
use crate::view::picking::{Hover, Selection};
use crate::view::world_view::{Highlights, WorldView};

pub struct WorldScene {
    session: Session,
    view: WorldView,
    camera: CameraController,
    hover: Hover,
    /// The hovered building (else tile) and since when (real seconds) the cursor rests on it;
    /// moving across the tiles of one building keeps the rest time.
    hover_since: (Option<Selection>, f64),
    /// The player's selection, shown in the info panel.
    selection: Option<Selection>,
    /// The citizen the camera follows (until the player pans, presses Esc or closes the
    /// citizen window).
    follow: Option<CitizenId>,
    tools: ToolController,
    hud: Hud,
    audio: GameAudio,
    /// The right button went down over the map (a release without drag cancels the tool).
    secondary_armed: bool,
    /// Speed before the pause menu opened.
    resume_speed: Option<GameSpeed>,
    /// Description of the quicksave file for the pause menu.
    quicksave_info: Option<String>,
    /// Developer aids: cursor tile override and a road drag held from a tile.
    cursor_override: Option<IVec2>,
    dev_drag: Option<IVec2>,
    /// Writes saves in the background.
    writer: SaveWriter,
    /// A dialog over the pause menu.
    overlay: Option<Overlay>,
    /// A save being read and checked in the background.
    pending_load: Option<PendingLoad>,
    /// Real seconds of running play since the last autosave, and its tick.
    autosave_seconds: f64,
    autosave_tick: u64,
    /// Name of the last named save (prefills the save dialog).
    last_save_name: Option<String>,
    /// Leave the game once the save the player started before leaving is written.
    leave_after_save: Option<ExitTarget>,
    /// Leave the game this frame.
    leave: Option<ExitTarget>,
    /// Reads events for the automatic pause.
    pause_cursor: EventCursor,
    /// Why the game paused itself and the speed to resume once the player dealt with it.
    auto_pause: AutoPause,
    /// Text for the pause menu: what is unsaved.
    save_status: String,
    /// The pause menu was opened for the lost game (once per session).
    defeat_shown: bool,
    /// Developer aid (`KINGDOMS_FOCUS=feind|karawane`): the camera follows the first enemy
    /// group or caravan.
    follow_dev: Option<FollowTarget>,
}

/// Where the player goes when leaving the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitTarget {
    MainMenu,
    Quit,
}

/// Dialogs opened from the pause menu.
enum Overlay {
    Browser(SaveBrowser),
    Settings(SettingsPanel),
    /// Leaving with unsaved progress: save first, leave anyway or stay.
    ConfirmExit(ExitTarget),
}

/// A save game read and checked on a worker thread.
struct PendingLoad {
    receiver: mpsc::Receiver<Result<save::LoadedGame, String>>,
    name: String,
    started: f64,
}

/// What a dialog asks the scene to do after it was drawn.
enum FollowUp {
    Save { path: PathBuf, name: String },
    Load { path: PathBuf, name: String },
    SaveDialog,
}

/// Answer to "leave with unsaved progress?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExitChoice {
    Save,
    Leave,
    Stay,
}

/// A page shown as a modal dialog (dimmed game behind it), `size` shrunk to fit the screen.
fn modal_page<R>(ctx: &egui::Context, id: &str, size: egui::Vec2, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let rect = menu::page_rect(ctx, size);
    let inner = rect.size() - egui::Vec2::splat(44.0);
    egui::Modal::new(egui::Id::unique(id))
        .frame(crate::ui::theme::parchment_frame().inner_margin(egui::Margin::same(20)))
        .show(ctx, |ui| {
            ui.set_width(inner.x);
            ui.set_height(inner.y);
            add(ui)
        })
        .inner
}

/// The question before leaving with unsaved progress.
fn confirm_exit(ui: &mut egui::Ui, target: ExitTarget, unsaved: &str) -> Option<ExitChoice> {
    let title = match target {
        ExitTarget::MainMenu => "Zum Hauptmenü?",
        ExitTarget::Quit => "Spiel beenden?",
    };
    menu::page_title(ui, title, None);
    ui.add_space(4.0);
    ui.add(egui::Label::new(format!("Seit dem letzten Speichern {unsaved} vergangen. Dieser Fortschritt geht verloren, wenn Ihr ohne Speichern geht.")).wrap());
    ui.add_space(14.0);
    let mut choice = None;
    ui.horizontal(|ui| {
        if menu::page_button(ui, "Speichern …", true, true).clicked() {
            choice = Some(ExitChoice::Save);
        }
        let leave = match target {
            ExitTarget::MainMenu => "Ohne Speichern verlassen",
            ExitTarget::Quit => "Ohne Speichern beenden",
        };
        if menu::danger_button(ui, leave, true).clicked() {
            choice = Some(ExitChoice::Leave);
        }
        if menu::page_button(ui, "Abbrechen", false, true).on_hover_text("Esc").clicked() || menu::escape_pressed(ui.ctx()) {
            choice = Some(ExitChoice::Stay);
        }
    });
    choice
}

/// Unsaved game time with the matching verb: "ist 1 Spielstunde", "sind 3 Spielstunden".
pub fn unsaved_phrase(ticks: u64) -> String {
    let text = unsaved_text(ticks);
    let singular = text.starts_with("weniger als") || ((text.starts_with("1 ")) && !text.contains(" und "));
    format!("{} {text}", if singular { "ist" } else { "sind" })
}

/// Save state line of the pause menu: a game never saved says so; otherwise whether progress
/// since the last save would be lost.
pub fn save_status_text(on_disk: bool, unsaved_ticks: u64) -> String {
    match (on_disk, unsaved_ticks) {
        (false, 0) => "Noch nicht gespeichert.".to_owned(),
        (false, t) => format!("Noch nicht gespeichert ({} gespielt).", unsaved_text(t)),
        (true, 0) => "Alles gespeichert.".to_owned(),
        (true, t) => format!("Nicht gespeichert: {}", unsaved_text(t)),
    }
}

/// German description of unsaved game time ("3 Spielstunden", "1 Spieltag und 2 Spielstunden").
pub fn unsaved_text(ticks: u64) -> String {
    let hours = ticks / TICKS_PER_HOUR;
    let days = ticks / TICKS_PER_DAY;
    let rest = hours % (TICKS_PER_DAY / TICKS_PER_HOUR);
    let hours_text = |h: u64| if h == 1 { "1 Spielstunde".to_owned() } else { format!("{h} Spielstunden") };
    match (days, rest) {
        (0, 0) => "weniger als eine Spielstunde".to_owned(),
        (0, h) => hours_text(h),
        (d, 0) => if d == 1 { "1 Spieltag".to_owned() } else { format!("{d} Spieltage") },
        (d, h) => format!("{} und {}", if d == 1 { "1 Spieltag".to_owned() } else { format!("{d} Spieltage") }, hours_text(h)),
    }
}

impl WorldScene {
    /// Shows `session`; the developer options of `options` may move the camera, preselect
    /// something, activate a tool or open the pause menu.
    pub fn new(ctx: &mut EngineContext, mut session: Session, options: &LaunchOptions) -> Self {
        // A developer case names what to look at; the explicit options take precedence.
        let options = &match session.dev_view() {
            Some(dev) => LaunchOptions {
                focus: options.focus.or(Some(dev.focus)),
                select: options.select.or(dev.select),
                tool: options.tool.clone().or_else(|| dev.tool.clone()),
                cursor: options.cursor.or(dev.cursor),
                ..options.clone()
            },
            None => options.clone(),
        };
        let view = WorldView::new(&mut session, &mut ctx.renderer, &ctx.assets);
        let mut camera = CameraController::new(ctx);
        camera.set_map_bounds(session.state().world().size());
        if let Some(zoom) = options.zoom {
            camera.set_zoom(zoom);
        }
        let icons = Icons::load(&mut ctx.renderer, &ctx.assets, session.defs());
        let hud = Hud::new(icons, Minimap::new(&session), &session);
        let audio = GameAudio::new(&session);
        let quicksave_info = save::quicksave_info(&ctx.paths.saves_dir());
        let pause_cursor = session.events().cursor_at_end();
        let autosave_tick = session.state().clock().tick;
        let mut scene = Self {
            session,
            view,
            camera,
            hover: Hover::default(),
            hover_since: (None, 0.0),
            selection: None,
            follow: None,
            tools: ToolController::new(),
            hud,
            audio,
            secondary_armed: false,
            resume_speed: None,
            quicksave_info,
            cursor_override: options.cursor,
            dev_drag: None,
            writer: SaveWriter::default(),
            overlay: None,
            pending_load: None,
            autosave_seconds: 0.0,
            autosave_tick,
            last_save_name: None,
            leave_after_save: None,
            leave: None,
            pause_cursor,
            auto_pause: AutoPause::default(),
            save_status: String::new(),
            defeat_shown: false,
            follow_dev: options.follow,
        };
        // A quickload replaces the session (and resets camera, selection and tool): the other
        // options apply to the loaded game.
        if matches!(options.quick, Some(QuickStart::Load)) {
            scene.quick_load_now(ctx);
        }
        scene.selection = options.select.map(|p| match crate::view::picking::building_at_or_entrance(scene.session.state(), p) {
            Some(id) => Selection::Building(id),
            None => Selection::Tile(p),
        });
        match options.focus {
            Some(tile) => scene.focus(tile),
            None => scene.focus(scene.session.home_tile()),
        }
        if let Some(spec) = &options.tool {
            scene.apply_tool_spec(spec);
        }
        if matches!(options.quick, Some(QuickStart::Save)) {
            scene.quick_save(ctx, true);
        }
        if let Some(key) = options.panel.as_deref() {
            let key = key.trim().to_lowercase().replace('ä', "ae").replace('ß', "ss").replace('ü', "ue");
            if let Some(tab) = BuildTab::ALL.into_iter().find(|t| t.key() == key) {
                scene.hud.open_build(tab);
            } else if let Some(panel) = RightPanel::ALL.into_iter().find(|p| p.key() == key) {
                scene.hud.open_panel(panel);
            } else {
                log::warn!("KINGDOMS_PANEL: unknown panel '{key}'");
            }
        }
        if let Some(key) = options.window.as_deref() {
            match WindowTab::from_key(key) {
                Some(tab) => {
                    scene.hud.toggle_window(tab);
                }
                None => log::warn!(
                    "KINGDOMS_WINDOW: unknown window '{key}' (known: {})",
                    WindowTab::ALL.iter().map(|t| t.key()).collect::<Vec<_>>().join(", ")
                ),
            }
        }
        ctx.time.set_speed(options.speed.unwrap_or(GameSpeed::Normal));
        if options.menu {
            scene.open_menu(ctx);
        }
        ctx.audio.set_music_mood(MusicMood::Peaceful);
        ctx.audio.play("vo_welcome");
        scene
    }

    /// Shows a message (e.g. "Spielstand geladen") as a toast.
    pub fn toast(&mut self, ctx: &EngineContext, text: impl Into<String>) {
        self.hud.toast(text, ctx.time.elapsed(), false);
    }

    /// Activates a tool requested by `KINGDOMS_TOOL` (developer aid for screenshots).
    fn apply_tool_spec(&mut self, spec: &ToolSpec) {
        let defs = self.session.defs();
        match spec {
            ToolSpec::Build { building, rotation } => match defs.building_id(building) {
                Some(kind) => {
                    self.tools.select(Tool::Build { kind });
                    self.tools.rotate(*rotation as i8);
                }
                None => log::warn!("KINGDOMS_TOOL: unknown building '{building}'"),
            },
            ToolSpec::Road { road, drag_from } => match defs.road_id(road) {
                Some(road) => {
                    self.tools.select(Tool::Road { road });
                    self.dev_drag = *drag_from;
                }
                None => log::warn!("KINGDOMS_TOOL: unknown road '{road}'"),
            },
            ToolSpec::Demolish => {
                self.tools.select(Tool::Demolish);
            }
            ToolSpec::Move { tile } => match self.session.state().buildings().id_at(*tile) {
                Some(id) => {
                    if let Err(reason) = self.tools.start_move(self.session.state(), self.session.defs(), id) {
                        log::warn!("KINGDOMS_TOOL: {reason}");
                    }
                }
                None => log::warn!("KINGDOMS_TOOL: no building at {tile}"),
            },
        }
    }

    /// Centres the camera on `tile` (the headquarters for "home").
    fn focus(&mut self, tile: IVec2) {
        let level = self.session.state().world().get(tile).map_or(0.0, |t| f32::from(t.elevation));
        self.camera.focus_tile(tile, level);
    }

    fn open_menu(&mut self, ctx: &mut EngineContext) {
        if self.hud.menu_open() {
            return;
        }
        self.hud.set_menu_open(true);
        self.quicksave_info = save::quicksave_info(&ctx.paths.saves_dir());
        if self.resume_speed.is_none() {
            self.resume_speed = Some(ctx.time.speed());
        }
        ctx.time.set_speed(GameSpeed::Paused);
    }

    fn close_menu(&mut self, ctx: &mut EngineContext) {
        self.hud.set_menu_open(false);
        if let Some(speed) = self.resume_speed.take() {
            ctx.time.set_speed(speed);
        }
    }

    fn set_speed(&mut self, ctx: &mut EngineContext, speed: GameSpeed) {
        if ctx.time.speed() == speed {
            return;
        }
        ctx.time.set_speed(speed);
        ctx.audio.play(if speed.is_paused() { "ui_pause" } else { "ui_speed" });
    }

    fn handle_keys(&mut self, shared: &mut GameShared, ctx: &mut EngineContext) {
        if ctx.input.ui_wants_keyboard() {
            return;
        }
        if self.hud.has_modal() {
            // The pause menu names the quicksave and quickload keys on its buttons: they work
            // there too (and close the menu like the buttons).
            if self.hud.only_menu_open() {
                if shared.actions.pressed(&ctx.input, Action::QuickSave) {
                    self.close_menu(ctx);
                    self.quick_save(ctx, false);
                } else if shared.actions.pressed(&ctx.input, Action::QuickLoad) {
                    self.close_menu(ctx);
                    self.quick_load(ctx);
                }
            }
            return;
        }
        let pressed = |action| shared.actions.pressed(&ctx.input, action);
        if pressed(Action::ToggleDebug) {
            shared.show_debug = !shared.show_debug;
        }
        if pressed(Action::ToggleGrid) {
            self.view.show_grid = !self.view.show_grid;
        }
        if pressed(Action::ToggleTerritory) {
            self.view.show_territory = !self.view.show_territory;
        }
        if pressed(Action::CenterHome) {
            self.follow = None;
            self.focus(self.session.home_tile());
        }
        let toggle_build = pressed(Action::ToggleBuildMenu);
        let toggle_events = pressed(Action::ToggleEvents);
        let demolish = pressed(Action::DemolishTool);
        let move_tool = pressed(Action::MoveTool);
        let quick_save = pressed(Action::QuickSave);
        let quick_load = pressed(Action::QuickLoad);
        let cancel = pressed(Action::Cancel);
        let screenshot = pressed(Action::Screenshot);
        let windows: Vec<WindowTab> = WindowTab::ALL.into_iter().filter(|t| pressed(t.action())).collect();
        let speed = ctx.time.speed();
        let new_speed = if pressed(Action::TogglePause) {
            Some(if speed.is_paused() { GameSpeed::Normal } else { GameSpeed::Paused })
        } else if pressed(Action::SpeedNormal) {
            Some(GameSpeed::Normal)
        } else if pressed(Action::SpeedDouble) {
            Some(GameSpeed::Double)
        } else if pressed(Action::SpeedQuadruple) {
            Some(GameSpeed::Quadruple)
        } else {
            None
        };
        if let Some(s) = new_speed {
            self.auto_pause.clear();
            self.set_speed(ctx, s);
        }
        if toggle_build {
            let open = self.hud.toggle_build_menu();
            ctx.audio.play(if open { "ui_open" } else { "ui_close" });
        }
        if toggle_events {
            let open = self.hud.toggle_events(self.hud.event_pending());
            ctx.audio.play(if open { "ui_open" } else { "ui_close" });
        }
        if demolish {
            let tool = if self.tools.tool() == Tool::Demolish { Tool::Inspect } else { Tool::Demolish };
            self.select_tool(ctx, tool);
        }
        if move_tool {
            match (self.tools.tool(), self.selection) {
                (Tool::Move { .. }, _) => self.select_tool(ctx, Tool::Inspect),
                (_, Some(Selection::Building(id))) => self.request_tool(ctx, ToolRequest::Move(Some(id))),
                _ => self.select_tool(ctx, Tool::Move { building: None }),
            }
        }
        if quick_save {
            self.quick_save(ctx, false);
        }
        if quick_load {
            self.quick_load(ctx);
        }
        for tab in windows {
            let open = self.hud.toggle_window(tab);
            ctx.audio.play(if open { "ui_open" } else { "ui_close" });
        }
        if cancel {
            // Innermost first: tool step, following a citizen, the event dialog, the panel of
            // the right bar or the build flyout, the management window, the citizen window,
            // the selection, then the pause menu.
            if self.tools.cancel()
                || self.follow.take().is_some()
                || self.close_hud_panel(ctx)
                || self.hud.close_window()
                || self.hud.close_citizen()
                || self.selection.take().is_some()
            {
                ctx.audio.play("ui_close");
            } else {
                self.open_menu(ctx);
                ctx.audio.play("ui_open");
            }
        }
        if screenshot {
            let path = ctx.screenshot();
            log::info!("screenshot requested: {}", path.display());
        }
    }

    /// Esc on the HUD's dialogs and panels; closing the proposal card postpones its proposals
    /// („Später“). Returns true if something was closed.
    fn close_hud_panel(&mut self, ctx: &mut EngineContext) -> bool {
        match self.hud.close_panel() {
            Closed::Nothing => false,
            Closed::ProposalCard => {
                self.release_pause(ctx, PauseReason::Proposal);
                true
            }
            Closed::Other => true,
        }
    }

    fn select_tool(&mut self, ctx: &mut EngineContext, tool: Tool) {
        if self.tools.select(tool) {
            ctx.audio.play(if tool == Tool::Inspect { "ui_close" } else { "ui_select" });
        }
    }

    fn request_tool(&mut self, ctx: &mut EngineContext, request: ToolRequest) {
        match request {
            ToolRequest::Inspect => self.select_tool(ctx, Tool::Inspect),
            ToolRequest::Build(kind) => self.select_tool(ctx, Tool::Build { kind }),
            ToolRequest::Road(road) => self.select_tool(ctx, Tool::Road { road }),
            ToolRequest::Demolish => self.select_tool(ctx, Tool::Demolish),
            ToolRequest::Move(None) => self.select_tool(ctx, Tool::Move { building: None }),
            ToolRequest::Move(Some(id)) => match self.tools.start_move(self.session.state(), self.session.defs(), id) {
                Ok(()) => ctx.audio.play("ui_select"),
                Err(reason) => {
                    self.hud.toast(reason, ctx.time.elapsed(), true);
                    ctx.audio.play("ui_error");
                }
            },
        }
    }

    /// Hovered tile and building; the developer cursor override replaces the mouse.
    fn update_hover(&mut self, ctx: &EngineContext) {
        let state = self.session.state();
        self.hover = if let Some(tile) = self.cursor_override.filter(|t| state.world().in_bounds(*t)) {
            Hover { tile: Some(tile), building: state.buildings().id_at(tile), citizen: None }
        } else {
            match ctx.input.cursor_position() {
                Some(c) if !ctx.input.ui_wants_pointer() => {
                    let w = self.camera.camera.screen_to_world(c);
                    self.view.hover_at(&self.session, &ctx.assets, w, self.camera.camera.zoom)
                }
                _ => Hover::default(),
            }
        };
        let target = self.hover.selection();
        if target != self.hover_since.0 {
            self.hover_since = (target, ctx.time.elapsed());
        }
    }

    /// The tool input of this frame from the mouse, keyboard and developer overrides.
    fn tool_input(&mut self, actions: &ActionMap<Action>, input: &Input) -> ToolInput {
        let primary = mouse_button(actions, Action::Select).unwrap_or(MouseButton::Left);
        let secondary = mouse_button(actions, Action::ToolCancel).unwrap_or(MouseButton::Right);
        if input.mouse_pressed(secondary) {
            self.secondary_armed = true;
        }
        let mut secondary_click = false;
        if input.mouse_released(secondary) {
            secondary_click = self.secondary_armed && input.clicked(secondary) && !input.ui_wants_pointer();
            self.secondary_armed = false;
        }
        let shift = input.modifiers().shift;
        let mut rotate = 0;
        if !input.ui_wants_keyboard() && !self.hud.has_modal() && actions.pressed(input, Action::Rotate) {
            rotate = 1;
        }
        if shift && self.tools.uses_rotation() && input.scroll() != 0.0 {
            rotate = if input.scroll() > 0.0 { 1 } else { -1 };
        }
        let mut tool_input = ToolInput {
            tile: self.hover.tile,
            building: self.hover.building,
            citizen: self.hover.citizen,
            pressed: input.mouse_pressed(primary),
            down: input.mouse_down(primary),
            released: input.mouse_released(primary),
            dragged: input.mouse_released(primary) && !input.clicked(primary),
            over_ui: input.ui_wants_pointer(),
            secondary_click,
            rotate,
            shift,
        };
        if self.hud.has_modal() {
            tool_input = ToolInput { released: tool_input.released, over_ui: true, ..ToolInput::default() };
        }
        // Developer aid: hold a road drag from a tile to the cursor tile (screenshots).
        if let Some(from) = self.dev_drag {
            let started = self.tools.road_drag().is_some();
            tool_input = ToolInput {
                tile: if started { self.hover.tile } else { Some(from) },
                pressed: !started,
                down: true,
                ..ToolInput::default()
            };
        }
        tool_input
    }

    fn apply_tool_output(&mut self, ctx: &mut EngineContext, out: ToolOutput) {
        for command in out.commands {
            self.session.queue_command(command);
        }
        for sound in out.sounds {
            ctx.audio.play(sound);
        }
        if let Some(request) = out.confirm {
            self.hud.ask_demolish(request);
        }
        if let Some(selection) = out.selection {
            self.selection = selection;
        }
        if let Some(citizen) = out.inspect_citizen {
            // The citizen picked on the map answers.
            ctx.audio.play("vo_citizen_yes");
            self.hud.inspect_citizen(Some(citizen));
        }
        if let Some(message) = out.message {
            self.hud.toast(message, ctx.time.elapsed(), true);
        }
        if out.order_given {
            self.hud.open_window(WindowTab::Military);
        }
    }

    fn apply_hud_action(&mut self, ctx: &mut EngineContext, action: HudAction) {
        match action {
            HudAction::Command(command) => {
                // Deciding what the game paused for lets it continue (unless more waits).
                match command {
                    Command::ResolveEvent { .. } => self.release_pause(ctx, PauseReason::Event),
                    // The card moves on to the next proposal waiting for an answer, if any.
                    Command::AnswerProposal { .. } if !self.hud.proposal_card_open() => self.release_pause(ctx, PauseReason::Proposal),
                    _ => {}
                }
                self.session.queue_command(command);
            }
            HudAction::ProposalPostponed => self.release_pause(ctx, PauseReason::Proposal),
            HudAction::Tool(request) => self.request_tool(ctx, request),
            HudAction::Deselect => self.selection = None,
            HudAction::SelectBuilding(id) => {
                if let Some(b) = self.session.state().building(id) {
                    let rect = b.rect();
                    self.selection = Some(Selection::Building(id));
                    self.follow = None;
                    self.focus(rect.origin + rect.size / 2);
                }
            }
            HudAction::ToggleWindow(tab) => {
                let open = self.hud.toggle_window(tab);
                ctx.audio.play(if open { "ui_open" } else { "ui_close" });
            }
            HudAction::OpenWindow(tab) => self.hud.open_window(tab),
            HudAction::EventDialog(open) => self.hud.set_event_dialog(open),
            HudAction::ToggleGrid => self.view.show_grid = !self.view.show_grid,
            HudAction::ToggleTerritory => self.view.show_territory = !self.view.show_territory,
            HudAction::Focus(tile) => {
                self.follow = None;
                self.focus(tile);
            }
            HudAction::LookAt(world) => {
                self.follow = None;
                self.camera.camera.look_at(world);
            }
            HudAction::Speed(speed) => {
                self.resume_speed = None;
                self.auto_pause.clear();
                self.set_speed(ctx, speed);
            }
            HudAction::Menu(true) => self.open_menu(ctx),
            HudAction::Menu(false) => self.close_menu(ctx),
            HudAction::QuickSave => {
                self.close_menu(ctx);
                self.quick_save(ctx, false);
            }
            HudAction::QuickLoad => {
                self.close_menu(ctx);
                self.quick_load(ctx);
            }
            HudAction::OpenSave => self.open_save_dialog(ctx),
            HudAction::OpenLoad => {
                let browser = SaveBrowser::new(BrowserMode::Load, &ctx.paths.saves_dir(), self.session.defs().fingerprint(), String::new());
                self.overlay = Some(Overlay::Browser(browser));
                ctx.audio.play("ui_open");
            }
            HudAction::OpenSettings => {
                self.overlay = Some(Overlay::Settings(SettingsPanel::new(SettingsTab::Audio)));
                ctx.audio.play("ui_open");
            }
            HudAction::MainMenu => self.request_leave(ctx, ExitTarget::MainMenu),
            HudAction::Quit => self.request_leave(ctx, ExitTarget::Quit),
            HudAction::Sound(id) => ctx.audio.play(id),
            HudAction::InspectCitizen(id) => self.hud.inspect_citizen(Some(id)),
            HudAction::CloseCitizen => {
                self.hud.inspect_citizen(None);
                self.follow = None;
            }
            HudAction::FollowCitizen(follow) => {
                self.follow = follow;
                if follow.is_some() {
                    self.hud.inspect_citizen(follow);
                }
            }
            HudAction::LookAtCitizen(id) => {
                self.follow = None;
                let (state, defs) = (self.session.state(), self.session.defs());
                if let Some(at) = citizens::citizen_world_position(state, defs, id, ctx.time.alpha()) {
                    self.camera.camera.look_at(at);
                }
            }
            HudAction::PickOrder { company, pick } => {
                // The window would cover the map: it comes back once the order is given.
                self.hud.close_window();
                self.selection = None;
                self.select_tool(ctx, Tool::Order { company, pick });
            }
            HudAction::FinishPatrol => {
                let out = self.tools.finish_patrol(&self.session);
                self.apply_tool_output(ctx, out);
            }
            HudAction::OpenKingdom(id) => {
                self.hud.open_kingdom(id);
                ctx.audio.play("ui_open");
            }
            HudAction::ShowProposal(serial) => self.hud.show_proposal(serial),
            HudAction::OpenPanel(panel) => self.hud.open_panel(panel),
        }
    }

    /// Saves to the quicksave file: in the background, or right away with `wait` (developer
    /// start option, so the file exists when the game shows).
    fn quick_save(&mut self, ctx: &mut EngineContext, wait: bool) {
        let path = save::quicksave_path(&ctx.paths.saves_dir());
        self.submit_save(ctx, path, SlotKind::Quick, "Schnellspielstand");
        if wait {
            let done = self.writer.finish();
            self.handle_saves(ctx, done);
        }
    }

    /// Captures the state and hands it to the background writer.
    fn submit_save(&mut self, ctx: &mut EngineContext, path: PathBuf, kind: SlotKind, name: &str) {
        let started = std::time::Instant::now();
        let job = SaveJob::new(&self.session, path, kind, name, save::now_unix()).with_notifications(self.hud.notification_history());
        log::info!("save of tick {} captured in {:.1} ms", job.tick(), started.elapsed().as_secs_f64() * 1000.0);
        self.writer.submit(job);
        if !matches!(kind, SlotKind::Auto(_)) {
            ctx.audio.play("ui_save");
        }
    }

    /// Reports finished saves.
    fn handle_saves(&mut self, ctx: &mut EngineContext, done: Vec<save::SaveOutcome>) {
        let now = ctx.time.elapsed();
        for outcome in done {
            match &outcome.result {
                Ok(()) => {
                    log::info!("{} written in {:.0} ms", outcome.path.display(), outcome.elapsed.as_secs_f64() * 1000.0);
                    self.session.mark_saved(outcome.tick);
                    match outcome.kind {
                        SlotKind::Auto(_) => self.hud.toast("Automatisch gespeichert.", now, false),
                        SlotKind::Quick => {
                            self.hud.toast("Spiel gespeichert (Schnellspielstand).", now, false);
                            ctx.audio.play("vo_game_saved");
                            self.quicksave_info = save::quicksave_info(&ctx.paths.saves_dir());
                        }
                        SlotKind::Manual => {
                            self.hud.toast(format!("Spiel gespeichert: {}", outcome.name), now, false);
                            ctx.audio.play("vo_game_saved");
                        }
                    }
                }
                Err(err) => {
                    log::error!("saving {} failed: {err}", outcome.path.display());
                    self.hud.toast(format!("Speichern fehlgeschlagen: {}", save::describe_error(err)), now, true);
                    ctx.audio.play("ui_error");
                    // Leaving waits for a save that worked.
                    self.leave_after_save = None;
                }
            }
        }
    }

    /// Autosaves when the interval of the settings has passed (skipped while a save runs).
    fn autosave(&mut self, shared: &GameShared, ctx: &mut EngineContext) {
        let running = !ctx.time.speed().is_paused() && !self.hud.has_modal();
        if running {
            self.autosave_seconds += f64::from(ctx.time.delta());
        }
        let interval = shared.settings.game.autosave;
        let tick = self.session.state().clock().tick;
        let due = match (interval.seconds(), interval.ticks()) {
            (Some(seconds), _) => self.autosave_seconds >= seconds,
            (_, Some(ticks)) => tick.saturating_sub(self.autosave_tick) >= ticks,
            _ => false,
        };
        if !due || self.writer.busy() || self.pending_load.is_some() || self.session.unsaved_ticks() == 0 {
            return;
        }
        let path = save::slots::next_autosave_path(&ctx.paths.saves_dir());
        self.submit_save(ctx, path, SlotKind::Auto(0), "Automatisch gespeichert");
        self.autosave_seconds = 0.0;
        self.autosave_tick = tick;
    }

    /// Pauses on events that need the player (settings).
    fn auto_pause(&mut self, shared: &GameShared, ctx: &mut EngineContext) {
        let game = &shared.settings.game;
        let (text, reasons) = {
            let read = self.session.events().read(&mut self.pause_cursor);
            let events: Vec<&SimEvent> = read.events.map(|e| &e.event).collect();
            let reasons: Vec<PauseReason> = events.iter().filter_map(|e| PauseReason::of(e, game)).collect();
            (pause_reason(events.into_iter(), game, self.session.defs()), reasons)
        };
        if self.auto_pause.pause(&reasons, ctx.time.speed()) {
            self.set_speed(ctx, GameSpeed::Paused);
            if let Some(text) = text {
                self.hud.toast(text, ctx.time.elapsed(), true);
            }
        }
    }

    /// The player dealt with `reason`: resumes the speed before the automatic pause when
    /// nothing else holds it (see [`AutoPause::release`]).
    fn release_pause(&mut self, ctx: &mut EngineContext, reason: PauseReason) {
        if let Some(speed) = self.auto_pause.release(reason, ctx.time.speed(), self.hud.menu_open()) {
            self.set_speed(ctx, speed);
        }
    }

    /// Opens the pause menu once the game is lost: the simulation stands still and rejects every
    /// command, the menu names the reason and offers loading or leaving.
    fn show_defeat(&mut self, ctx: &mut EngineContext) {
        if self.defeat_shown || self.pending_load.is_some() {
            return;
        }
        if let Some(over) = self.session.state().game_over() {
            log::info!("game lost at tick {}: {:?}", over.tick, over.reason);
            self.defeat_shown = true;
            self.auto_pause.clear();
            self.open_menu(ctx);
            ctx.audio.play("sfx_death_bell");
        }
    }

    /// Starts reading the quicksave in the background (F9).
    fn quick_load(&mut self, ctx: &mut EngineContext) {
        let path = save::quicksave_path(&ctx.paths.saves_dir());
        if !path.exists() {
            self.hud.toast("Laden fehlgeschlagen: Es gibt noch keinen Schnellspielstand.", ctx.time.elapsed(), true);
            ctx.audio.play("ui_error");
            return;
        }
        self.start_load(ctx, path, "Schnellspielstand".to_owned());
    }

    /// Loads the quicksave right away (developer start option).
    fn quick_load_now(&mut self, ctx: &mut EngineContext) {
        let defs = self.session.shared_defs();
        match save::load(&save::quicksave_path(&ctx.paths.saves_dir()), &defs) {
            Ok(loaded) => {
                let toast = format!("Spielstand geladen: {}", loaded.game.header.summary.date);
                let session = loaded.into_session(defs);
                self.replace_session(ctx, session, toast);
            }
            Err(err) => {
                log::warn!("quickload failed: {err}");
                self.hud.toast(format!("Laden fehlgeschlagen: {}", save::describe_quickload_error(&err)), ctx.time.elapsed(), true);
                ctx.audio.play("ui_error");
            }
        }
    }

    /// Reads and checks a save on a worker thread; the game waits paused behind a message.
    fn start_load(&mut self, ctx: &mut EngineContext, path: PathBuf, name: String) {
        if self.pending_load.is_some() {
            return;
        }
        let defs = self.session.shared_defs();
        let (sender, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("save-loader".into()).spawn(move || {
            let result = save::load(&path, &defs).map_err(|err| {
                log::warn!("cannot load {}: {err}", path.display());
                save::describe_error(&err)
            });
            let _ = sender.send(result);
        });
        match spawned {
            Ok(_) => {
                self.pending_load = Some(PendingLoad { receiver, name, started: ctx.time.elapsed() });
                if self.resume_speed.is_none() {
                    self.resume_speed = Some(ctx.time.speed());
                }
                ctx.time.set_speed(GameSpeed::Paused);
            }
            Err(err) => {
                log::error!("cannot start the loading thread: {err}");
                self.hud.toast("Laden fehlgeschlagen: kein Hintergrund-Thread verfügbar.", ctx.time.elapsed(), true);
            }
        }
    }

    /// Takes a finished background load.
    fn poll_load(&mut self, ctx: &mut EngineContext) {
        let Some(pending) = &self.pending_load else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Beim Laden ist ein Fehler aufgetreten (Details im Protokoll).".to_owned()),
        };
        let name = pending.name.clone();
        self.pending_load = None;
        match result {
            Ok(loaded) => {
                let toast = format!("Spielstand geladen: {name} ({})", loaded.game.header.summary.date);
                let session = loaded.into_session(self.session.shared_defs());
                self.replace_session(ctx, session, toast);
                self.overlay = None;
                self.close_menu(ctx);
            }
            Err(message) => {
                self.hud.toast(format!("Laden fehlgeschlagen: {message}"), ctx.time.elapsed(), true);
                ctx.audio.play("ui_error");
                if let Some(Overlay::Browser(browser)) = &mut self.overlay {
                    browser.set_message(format!("„{name}“ kann nicht geladen werden: {message}"), true);
                }
            }
        }
    }

    /// Replaces the running game with `session` (after loading).
    fn replace_session(&mut self, ctx: &mut EngineContext, mut session: Session, toast: String) {
        // The renderer keeps static batches until they are removed: free the old view's
        // layers before replacing it, and keep the player's view toggles.
        let (show_grid, show_territory) = (self.view.show_grid, self.view.show_territory);
        let batches_before = ctx.renderer.static_batch_count();
        self.view.release(&mut ctx.renderer);
        self.view = WorldView::new(&mut session, &mut ctx.renderer, &ctx.assets);
        self.view.show_grid = show_grid;
        self.view.show_territory = show_territory;
        log::info!("load: static batches {batches_before} before, {} after", ctx.renderer.static_batch_count());
        self.session = session;
        self.camera.set_map_bounds(self.session.state().world().size());
        self.focus(self.session.home_tile());
        self.tools = ToolController::new();
        self.selection = None;
        self.follow = None;
        self.hover = Hover::default();
        self.dev_drag = None;
        self.hud.reset(&self.session);
        self.hud.minimap.reset(&self.session);
        self.audio.reset(&self.session);
        self.pause_cursor = self.session.events().cursor_at_end();
        self.auto_pause.clear();
        self.defeat_shown = false;
        self.autosave_seconds = 0.0;
        self.autosave_tick = self.session.state().clock().tick;
        self.hud.toast(toast, ctx.time.elapsed(), false);
        ctx.time.reset_accumulator();
        ctx.audio.play("ui_open");
    }

    /// Leaves the game, asking first if progress would be lost.
    fn request_leave(&mut self, ctx: &mut EngineContext, target: ExitTarget) {
        if self.session.unsaved_ticks() > 0 {
            self.open_menu(ctx);
            self.overlay = Some(Overlay::ConfirmExit(target));
            ctx.audio.play("ui_open");
        } else {
            self.leave = Some(target);
        }
    }

    /// Opens the save dialog (prefilled with the last name or the kingdom's year).
    fn open_save_dialog(&mut self, ctx: &mut EngineContext) {
        let name = self.last_save_name.clone().unwrap_or_else(|| {
            let clock = self.session.state().clock();
            format!("Mein Reich – {} Jahr {}", clock.season().display_name(), clock.year())
        });
        let browser = SaveBrowser::new(BrowserMode::Save, &ctx.paths.saves_dir(), self.session.defs().fingerprint(), name);
        self.overlay = Some(Overlay::Browser(browser));
        ctx.audio.play("ui_open");
    }

    /// The dialogs over the pause menu and the loading message.
    fn show_overlays(&mut self, shared: &mut GameShared, ctx: &mut EngineContext, egui_ctx: &egui::Context) {
        if let Some(pending) = &self.pending_load {
            let elapsed = ctx.time.elapsed() - pending.started;
            let name = pending.name.clone();
            egui::Modal::new(egui::Id::unique("world_loading")).frame(crate::ui::theme::parchment_frame()).show(egui_ctx, |ui| {
                ui.set_width(320.0);
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                    menu::spinner(ui.painter(), rect.center(), 11.0, elapsed);
                    ui.label(egui::RichText::new(format!("Lade „{name}“ …")).font(crate::ui::theme::heading_font(18.0)));
                });
            });
            egui_ctx.request_repaint();
            return;
        }
        let Some(overlay) = &mut self.overlay else { return };
        let mut close = false;
        let dir = ctx.paths.saves_dir();
        let fingerprint = self.session.defs().fingerprint();
        let mut follow_up = None;
        match overlay {
            Overlay::Browser(browser) => {
                let busy = self.writer.busy();
                let mode = browser.mode();
                let id = if mode == BrowserMode::Save { "world_save" } else { "world_load" };
                let action = modal_page(egui_ctx, id, PAGE_WIDE, |ui| browser.show(ui, &dir, fingerprint, busy));
                match action {
                    BrowserAction::None => {}
                    BrowserAction::Close => {
                        close = true;
                        self.leave_after_save = None;
                        ctx.audio.play("ui_close");
                    }
                    BrowserAction::Save { path, name } => {
                        close = true;
                        follow_up = Some(FollowUp::Save { path, name });
                    }
                    BrowserAction::Load { path, name } => follow_up = Some(FollowUp::Load { path, name }),
                }
            }
            Overlay::Settings(panel) => {
                close = modal_page(egui_ctx, "world_settings", PAGE_NARROW, |ui| shared.settings_panel(panel, ui, ctx));
            }
            Overlay::ConfirmExit(target) => {
                let target = *target;
                let unsaved = unsaved_phrase(self.session.unsaved_ticks());
                let choice = modal_page(egui_ctx, "world_confirm_exit", egui::vec2(560.0, 205.0), |ui| confirm_exit(ui, target, &unsaved));
                match choice {
                    Some(ExitChoice::Save) => {
                        self.leave_after_save = Some(target);
                        follow_up = Some(FollowUp::SaveDialog);
                    }
                    Some(ExitChoice::Leave) => {
                        close = true;
                        self.leave = Some(target);
                    }
                    Some(ExitChoice::Stay) => {
                        close = true;
                        ctx.audio.play("ui_close");
                    }
                    None => {}
                }
            }
        }
        if close {
            self.overlay = None;
        }
        match follow_up {
            Some(FollowUp::Save { path, name }) => {
                self.last_save_name = Some(name.clone());
                self.submit_save(ctx, path, SlotKind::Manual, &name);
                if let Some(target) = self.leave_after_save {
                    // Leaving waits for the save (a failure keeps the game open).
                    let done = self.writer.finish();
                    self.handle_saves(ctx, done);
                    if self.leave_after_save.is_some() {
                        self.leave = Some(target);
                    }
                }
            }
            Some(FollowUp::Load { path, name }) => self.start_load(ctx, path, name),
            Some(FollowUp::SaveDialog) => self.open_save_dialog(ctx),
            None => {}
        }
    }

    /// Applies the settings that act every frame (camera, particles, day and night, feed).
    fn apply_settings(&mut self, shared: &GameShared) {
        let s = &shared.settings;
        self.camera.edge_scroll = s.game.edge_scroll;
        self.camera.trackpad_pan = s.game.trackpad_pan;
        self.camera.pan_speed = crate::view::camera::PAN_SPEED * s.game.camera_speed;
        self.view.day_night = s.display.day_night;
        self.view.set_particle_density(s.display.particles.factor());
        self.hud.set_notification_filter(s.game.notifications);
        self.save_status = save_status_text(self.session.is_on_disk(), self.session.unsaved_ticks());
    }

    /// The scene change the player asked for (main menu or quit).
    fn leave_transition(&mut self, shared: &mut GameShared, ctx: &mut EngineContext) -> Transition<GameShared> {
        match self.leave.take() {
            Some(ExitTarget::Quit) => Transition::Quit,
            Some(ExitTarget::MainMenu) => match shared.defs() {
                Ok(defs) => Transition::Replace(Box::new(MainMenuScene::new(shared, ctx, defs))),
                Err(err) => {
                    log::error!("{err:#}");
                    Transition::Quit
                }
            },
            None => Transition::None,
        }
    }

    /// Moves the camera onto what it follows – the followed citizen or, as a developer aid,
    /// the first enemy group or caravan – at this frame's interpolated position (`alpha`).
    /// Runs after the frame's ticks and before the HUD, so labels anchored to the map and the
    /// map itself use the same camera.
    fn follow_camera(&mut self, alpha: f32) {
        if let Some(target) = self.follow_dev {
            let state = self.session.state();
            let walking = state.population().walking();
            let walkers: Vec<kingdoms_sim::population::TravelerId> = match target {
                FollowTarget::Enemy => state
                    .military()
                    .warbands()
                    .iter()
                    .find(|w| !w.fighters().is_empty())
                    .map(|w| w.fighters().iter().map(|f| f.walker()).collect())
                    .unwrap_or_default(),
                FollowTarget::Caravan => state
                    .diplomacy()
                    .caravans()
                    .iter()
                    .find(|c| c.walkers.iter().any(|w| walking.traveler(*w).is_some_and(|t| t.motion().is_visible())))
                    .map(|c| c.walkers.clone())
                    .unwrap_or_default(),
            };
            let feet: Vec<glam::Vec2> = walkers
                .iter()
                .filter_map(|w| walking.traveler(*w))
                .map(|t| citizens::walker_world_position(state, t.motion().prev_pos(), t.motion().pos(), alpha))
                .collect();
            if !feet.is_empty() {
                self.camera.camera.look_at(feet.iter().copied().sum::<glam::Vec2>() / feet.len() as f32);
            }
        }
        if let Some(id) = self.follow {
            let (state, defs) = (self.session.state(), self.session.defs());
            match citizens::citizen_world_position(state, defs, id, alpha) {
                Some(at) => self.camera.camera.look_at(at),
                None => self.follow = None,
            }
        }
    }

    /// Screen position (points) of the pointer for the world tooltip.
    fn pointer_points(&self, ctx: &EngineContext) -> Option<egui::Pos2> {
        let scale = ctx.scale_factor().max(0.1);
        let px = match (self.cursor_override, self.hover.tile) {
            (Some(_), Some(tile)) => {
                let level = self.session.state().world().get(tile).map_or(0.0, |t| f32::from(t.elevation));
                self.camera.camera.world_to_screen(iso::tile_center_world(tile, level))
            }
            _ => ctx.input.cursor_position()?,
        };
        Some(egui::pos2(px.x / scale, px.y / scale))
    }
}

/// The first mouse button bound to `action`.
fn mouse_button(actions: &ActionMap<Action>, action: Action) -> Option<MouseButton> {
    actions.bindings(action).iter().find_map(|b| match b {
        Binding::Mouse(m) => Some(*m),
        _ => None,
    })
}

impl Scene<GameShared> for WorldScene {
    fn name(&self) -> &'static str {
        "world"
    }

    fn fixed_update(&mut self, _shared: &mut GameShared, _ctx: &mut EngineContext) {
        self.session.tick();
    }

    fn on_enter(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) {
        // Building the map took a while: no burst of catch-up ticks.
        ctx.time.reset_accumulator();
    }

    fn on_exit(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) {
        let done = self.writer.finish();
        self.handle_saves(ctx, done);
        self.view.release(&mut ctx.renderer);
        ctx.audio.stop_all();
    }

    fn on_close_requested(&mut self, shared: &mut GameShared, ctx: &mut EngineContext) -> bool {
        // Settings changed in an open settings panel are written before the exit question
        // replaces the panel (whatever the answer).
        shared.flush_settings(ctx);
        // A second click on the window's close button while the question is open quits.
        if self.session.unsaved_ticks() == 0 || matches!(self.overlay, Some(Overlay::ConfirmExit(ExitTarget::Quit))) {
            return true;
        }
        self.request_leave(ctx, ExitTarget::Quit);
        false
    }

    fn on_app_exit(&mut self, _shared: &mut GameShared, _ctx: &mut EngineContext) {
        // Saves still being written finish before the process ends.
        for outcome in self.writer.finish() {
            if let Err(err) = outcome.result {
                log::error!("saving {} failed: {err}", outcome.path.display());
            }
        }
    }

    fn update(&mut self, shared: &mut GameShared, ctx: &mut EngineContext) -> Transition<GameShared> {
        self.apply_settings(shared);
        let done = self.writer.poll();
        self.handle_saves(ctx, done);
        self.poll_load(ctx);
        self.hud.set_blocking(self.overlay.is_some() || self.pending_load.is_some());
        self.camera.wheel_zoom = !(ctx.input.modifiers().shift && self.tools.uses_rotation());
        self.camera.input_blocked = self.hud.has_modal();
        // Panning the map ends following a citizen.
        if self.camera.update(ctx, &shared.actions) {
            self.follow = None;
        }
        self.handle_keys(shared, ctx);
        self.update_hover(ctx);
        let input = self.tool_input(&shared.actions, &ctx.input);
        let out = self.tools.update(&self.session, &input);
        self.apply_tool_output(ctx, out);
        let enter = ctx.input.key_pressed(KeyCode::Enter) || ctx.input.key_pressed(KeyCode::NumpadEnter);
        if enter && !ctx.input.ui_wants_keyboard() && !self.hud.has_modal() && self.tools.patrol_points().len() >= 2 {
            let out = self.tools.finish_patrol(&self.session);
            self.apply_tool_output(ctx, out);
        }
        // Forget buildings that no longer exist.
        if let Some(Selection::Building(id)) = self.selection
            && self.session.state().building(id).is_none()
        {
            self.selection = None;
        }
        let night = self.view.daylight().map_or(0.0, |d| d.night);
        self.audio.update(&mut ctx.audio, &self.session, &self.camera.camera, night, ctx.time.delta());
        self.session.add_play_time(f64::from(ctx.time.delta()));
        self.autosave(shared, ctx);
        self.auto_pause(shared, ctx);
        self.show_defeat(ctx);
        self.leave_transition(shared, ctx)
    }

    fn ui(&mut self, shared: &mut GameShared, ctx: &mut EngineContext, ui: &mut egui::Ui) -> Transition<GameShared> {
        let egui_ctx = ui.ctx().clone();
        // The ticks of this frame ran: the camera's final place before anything uses it.
        self.follow_camera(ctx.time.alpha());
        let actions = {
            let input = HudInput {
                session: &self.session,
                tools: &self.tools,
                selection: self.selection,
                hover: self.hover,
                hover_rest: (ctx.time.elapsed() - self.hover_since.1) as f32,
                speed: ctx.time.speed(),
                view: self.camera.camera.view_rect(),
                alpha: ctx.time.alpha(),
                actions: &shared.actions,
                now: ctx.time.elapsed(),
                pointer: self.pointer_points(ctx),
                quicksave: self.quicksave_info.as_deref(),
                save_status: Some(&self.save_status),
                following: self.follow,
                show_grid: self.view.show_grid,
                show_territory: self.view.show_territory,
            };
            self.hud.show(&egui_ctx, &input)
        };
        for action in actions {
            self.apply_hud_action(ctx, action);
        }
        if crate::ui::widgets::take_hover_sound(&egui_ctx) {
            ctx.audio.play("ui_hover");
        }
        if shared.show_debug {
            let cam = &self.camera.camera;
            let info = DebugInfo {
                session: &self.session,
                hover: self.hover,
                selection: self.selection,
                view: self.view.stats(),
                extra: vec![
                    ("Kamera".into(), format!("{:.0}, {:.0} · Zoom {:.2}", cam.position.x, cam.position.y, cam.zoom)),
                    ("Werkzeug".into(), format!("{:?}", self.tools.tool())),
                    ("Raster (G)".into(), on_off(self.view.show_grid)),
                    ("Gebietsgrenze (T)".into(), on_off(self.view.show_territory)),
                    ("Folgt".into(), self.follow.map_or_else(|| "–".to_owned(), |c| format!("Bürger {}", c.index()))),
                ],
            };
            debug::show(ctx, ui, &info);
        }
        self.show_overlays(shared, ctx, &egui_ctx);
        self.leave_transition(shared, ctx)
    }

    fn render(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) -> Option<RenderList> {
        // Ticks of this frame ran before; bring the cached layers up to date first.
        let started = std::time::Instant::now();
        let dirty = self.view.sync(&mut self.session, &mut ctx.renderer, &ctx.assets);
        ctx.metrics.record_section("map sync", started.elapsed());
        let started = std::time::Instant::now();
        let egui_ctx = ctx.egui().clone();
        self.hud.minimap.update(&egui_ctx, &self.session, &dirty);
        ctx.metrics.record_section("minimap", started.elapsed());
        let started = std::time::Instant::now();
        self.view.update_effects(ctx.time.delta(), &self.session, &self.camera.camera, &ctx.assets);
        ctx.metrics.record_section("effects", started.elapsed());
        let mut list = ctx.renderer.begin_list(self.camera.camera.clone());
        list.clear_color = Color::from_srgb_u8(18, 30, 44, 255);
        let tool = self.tools.tool();
        let highlights = Highlights {
            hover: self.hover,
            selection: self.selection,
            demolish: tool == Tool::Demolish,
            hover_outline: matches!(tool, Tool::Inspect | Tool::Demolish | Tool::Move { building: None })
                && !(tool == Tool::Inspect && self.hover.citizen.is_some()),
            walkers: WalkerMarks {
                hovered: self.hover.citizen.filter(|_| tool == Tool::Inspect).map(WalkerId::Citizen),
                selected: self.hud.inspected_citizen(),
            },
            orders: self.hud.military_open() || matches!(tool, Tool::Order { .. }),
        };
        let alpha = ctx.time.alpha();
        let ticks = self.session.state().clock().tick as f64 + f64::from(alpha);
        let started = std::time::Instant::now();
        let times = self.view.draw(&mut list, &self.session, &ctx.assets, &highlights, ticks, alpha);
        ctx.metrics.record_section("map draw", started.elapsed());
        for &(name, elapsed) in times.layers() {
            ctx.metrics.record_section(name, elapsed);
        }
        self.view.draw_tool_preview(&mut list, &self.session, &ctx.assets, &self.tools, self.hover.tile);
        Some(list)
    }
}

fn on_off(on: bool) -> String {
    if on { "an".into() } else { "aus".into() }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kingdoms_sim::defs::Defs;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    use super::*;

    fn defs() -> Arc<Defs> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => Arc::new(defs),
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    #[test]
    fn unsaved_time_reads_naturally() {
        assert_eq!(unsaved_text(0), "weniger als eine Spielstunde");
        assert_eq!(unsaved_text(TICKS_PER_HOUR), "1 Spielstunde");
        assert_eq!(unsaved_text(3 * TICKS_PER_HOUR + 10), "3 Spielstunden");
        assert_eq!(unsaved_text(TICKS_PER_DAY), "1 Spieltag");
        assert_eq!(unsaved_text(2 * TICKS_PER_DAY + TICKS_PER_HOUR), "2 Spieltage und 1 Spielstunde");
        assert_eq!(save_status_text(false, 0), "Noch nicht gespeichert.");
        assert_eq!(save_status_text(false, 3 * TICKS_PER_HOUR), "Noch nicht gespeichert (3 Spielstunden gespielt).");
        assert_eq!(save_status_text(true, 0), "Alles gespeichert.");
        assert_eq!(save_status_text(true, TICKS_PER_DAY), "Nicht gespeichert: 1 Spieltag");
        assert_eq!(unsaved_phrase(10), "ist weniger als eine Spielstunde");
        assert_eq!(unsaved_phrase(TICKS_PER_HOUR), "ist 1 Spielstunde");
        assert_eq!(unsaved_phrase(TICKS_PER_DAY), "ist 1 Spieltag");
        assert_eq!(unsaved_phrase(3 * TICKS_PER_HOUR), "sind 3 Spielstunden");
        assert_eq!(unsaved_phrase(TICKS_PER_DAY + TICKS_PER_HOUR), "sind 1 Spieltag und 1 Spielstunde");
    }

    /// A game saved through the background writer and loaded again continues exactly like the
    /// original: both run on and end in byte-identical states.
    #[test]
    fn a_loaded_game_continues_exactly() {
        let defs = defs();
        let mut original = Session::start(defs.clone(), GameSettings::new(&defs, 21, WorldSize::Small));
        for _ in 0..300 {
            original.tick();
        }
        let dir = std::env::temp_dir().join(format!("kingdoms-continue-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = save::slots::save_path(&dir, "fortsetzung");
        let mut writer = SaveWriter::default();
        let news = vec![save::slots::SavedNotification {
            tick: 250,
            severity: kingdoms_sim::event::Severity::Warning,
            text: "Rohstoffmangel: Dem Gebäude „Sägewerk“ fehlt Holz.".to_owned(),
            position: Some((12, 30)),
            repeats: 3,
            kind: kingdoms_sim::event::NotificationKind::MissingInput,
            building: None,
        }];
        writer.submit(SaveJob::new(&original, path.clone(), SlotKind::Manual, "Fortsetzung", 1).with_notifications(news.clone()));
        assert!(writer.finish().iter().all(|o| o.result.is_ok()));
        let loaded = match save::load(&path, &defs) {
            Ok(l) => l,
            Err(e) => panic!("load: {e}"),
        };
        // The notification list comes back with the game, but only from the matching metadata.
        assert_eq!(loaded.notifications, news);
        let header = loaded.game.header.clone();
        let stale = save::slots::SaveMeta { tick: header.summary.tick + 1, ..save::slots::SaveMeta::of(&loaded.game.state, 1, 0) };
        let meta_path = save::slots::meta_path(&path);
        let Ok(text) = std::fs::read_to_string(&meta_path) else { panic!("metadata written") };
        assert!(save::slots::write_meta(&path, &stale).is_ok());
        assert!(save::trusted_meta(&path, &header).1.is_empty(), "metadata of another save is ignored");
        // Metadata from before the list existed still reads.
        let old = text.lines().take_while(|l| !l.contains("notifications")).collect::<Vec<_>>().join("\n") + "\n)";
        assert!(std::fs::write(&meta_path, old).is_ok());
        assert!(save::slots::read_meta(&path).is_some_and(|m| m.notifications.is_empty() && m.matches(&header)));
        let restored = loaded.into_session(defs.clone());
        assert_eq!(restored.restored_notifications(), news.as_slice(), "the session hands the list to the feed");
        let mut continued = Session::from_state(defs.clone(), restored.state().clone(), restored.play_seconds());
        assert_eq!(continued.unsaved_ticks(), 0);
        for _ in 0..400 {
            original.tick();
            continued.tick();
        }
        let encode = |s: &Session| {
            let header = kingdoms_sim::save::SaveHeader::for_state(s.state(), s.defs(), "x", 0);
            match kingdoms_sim::save::encode(&header, s.state()) {
                Ok(bytes) => bytes,
                Err(e) => panic!("encode: {e}"),
            }
        };
        assert!(encode(&original) == encode(&continued), "the loaded game diverged");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
