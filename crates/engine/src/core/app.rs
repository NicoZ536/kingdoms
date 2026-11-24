//! Application lifecycle and the main loop.
//!
//! Per rendered frame:
//! 1. measure time, compute due simulation ticks ([`super::time::Time`])
//! 2. [`Game::update`] — per-frame logic (input, camera)
//! 3. [`Game::fixed_update`] × due ticks — deterministic simulation
//! 4. [`Game::ui`] — egui pass
//! 5. [`Game::render`] — build a [`RenderList`], then the renderer draws world + UI
//! 6. per-frame input state is cleared

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Icon, Window, WindowId};

use super::config::EngineConfig;
use super::events::EventQueue;
use super::logging::LogBuffer;
use super::paths::GamePaths;
use super::time::Time;
use crate::assets::AssetManager;
use crate::debug::FrameMetrics;
use crate::input::Input;
use crate::rendering::sprite::RenderList;
use crate::rendering::{EguiPaint, GpuInfo, Renderer};

/// Engine-level events the game may react to.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
    Resized { width: u32, height: u32 },
    ScaleFactorChanged(f64),
    FocusChanged(bool),
    FileDropped(PathBuf),
}

/// The game implemented on top of the engine.
pub trait Game: 'static {
    /// Called once after the window and renderer exist.
    fn init(&mut self, ctx: &mut EngineContext) -> anyhow::Result<()>;
    /// Called once per fixed simulation tick (scaled by the game speed; not called when paused).
    fn fixed_update(&mut self, ctx: &mut EngineContext);
    /// Called once per rendered frame before the simulation ticks.
    fn update(&mut self, ctx: &mut EngineContext);
    /// Builds the egui user interface for this frame.
    fn ui(&mut self, ctx: &mut EngineContext, ui: &mut egui::Ui);
    /// Produces the sprites to draw this frame.
    fn render(&mut self, ctx: &mut EngineContext) -> RenderList;
    /// The window close button was pressed. Return `false` to keep running (e.g. to show a
    /// confirmation dialog).
    fn on_close_requested(&mut self, _ctx: &mut EngineContext) -> bool {
        true
    }
    /// Called right before the application exits.
    fn on_exit(&mut self, _ctx: &mut EngineContext) {}
}

/// Everything engine-owned that game code may use during a frame.
pub struct EngineContext {
    pub config: EngineConfig,
    pub paths: GamePaths,
    pub time: Time,
    pub input: Input,
    pub window: Arc<Window>,
    pub renderer: Renderer,
    pub assets: AssetManager,
    /// Sound effects, ambience and music (silent when no output device exists).
    pub audio: crate::audio::Audio,
    pub events: EventQueue<EngineEvent>,
    pub metrics: FrameMetrics,
    pub log: LogBuffer,
    pub gpu_info: GpuInfo,
    egui: egui::Context,
    exit_requested: bool,
    loading: bool,
    /// The window cannot be seen (macOS occlusion: covered, on another space, minimised).
    occluded: bool,
    /// Smallest screen in UI points the game's layout needs (see
    /// [`EngineContext::set_ui_min_size`]).
    ui_min_size: Option<glam::Vec2>,
}

impl EngineContext {
    /// Asks the application to quit at the end of the current frame.
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    pub fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    /// Tells the engine whether the game is showing a loading screen (decoding assets,
    /// generating a world, reading a save). Frames while loading do not count towards the
    /// automatic screenshot (`KINGDOMS_AUTOSHOT_FRAME`), so scripted captures show the same
    /// moment however long the loading took.
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// True while the game reports a loading screen ([`EngineContext::set_loading`]).
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn egui(&self) -> &egui::Context {
        &self.egui
    }

    /// Sets the smallest screen in UI points the game's layout is made for (`None`: no limit).
    /// The configured UI scale is then lowered – never raised – while the window would show
    /// fewer points, so a large scale on a small window does not break the layout.
    pub fn set_ui_min_size(&mut self, points: Option<glam::Vec2>) {
        self.ui_min_size = points;
    }

    /// The largest UI scale the window allows for the size set with
    /// [`EngineContext::set_ui_min_size`] (3.0 without a limit).
    pub fn ui_scale_limit(&self) -> f32 {
        let size = self.window_size() / self.scale_factor().max(0.01);
        effective_ui_scale(3.0, size, self.ui_min_size)
    }

    /// The UI scale in effect: the configured one
    /// ([`GraphicsConfig::ui_scale`](super::config::GraphicsConfig::ui_scale)), lowered to fit
    /// the window to the size set with [`EngineContext::set_ui_min_size`].
    pub fn effective_ui_scale(&self) -> f32 {
        let size = self.window_size() / self.scale_factor().max(0.01);
        effective_ui_scale(self.config.graphics.ui_scale, size, self.ui_min_size)
    }

    /// Window size in physical pixels.
    pub fn window_size(&self) -> glam::Vec2 {
        self.renderer.surface_size()
    }

    pub fn scale_factor(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    /// Captures the next frame into `<user>/screenshots/<timestamp>.png` and returns the path.
    pub fn screenshot(&mut self) -> PathBuf {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let path = self.paths.screenshots_dir().join(format!("kingdoms_{secs}_{}.png", self.time.frame_count()));
        self.renderer.request_screenshot(path.clone());
        path
    }

    /// Applies the window / graphics parts of a changed configuration (vsync, MSAA, UI scale,
    /// fullscreen) right away.
    pub fn apply_graphics_config(&mut self) {
        self.renderer.set_vsync(self.config.graphics.vsync);
        self.renderer.set_msaa_samples(self.config.graphics.msaa_samples);
        self.egui.set_zoom_factor(self.effective_ui_scale());
        let fullscreen = self.config.window.fullscreen.then_some(Fullscreen::Borderless(None));
        self.window.set_fullscreen(fullscreen);
    }
}

/// UI scale for a window of `window_points` (at scale 1): the `configured` scale, lowered so the
/// window shows at least `min_size` points (but not below 0.5).
pub fn effective_ui_scale(configured: f32, window_points: glam::Vec2, min_size: Option<glam::Vec2>) -> f32 {
    let fit = min_size
        .filter(|m| m.x > 0.0 && m.y > 0.0 && window_points.x > 0.0 && window_points.y > 0.0)
        .map_or(f32::MAX, |m| (window_points.x / m.x).min(window_points.y / m.y));
    configured.min(fit).clamp(0.5, 3.0)
}

/// Test / CI helper: `KINGDOMS_AUTOSHOT=<png path>` captures frame `KINGDOMS_AUTOSHOT_FRAME`
/// (default 90) and exits two frames later. A list of frames (`30,33`) captures each of them
/// into `<stem>_<frame>.<ext>` (animations, before/after comparisons). Frames while the game
/// reports loading ([`EngineContext::set_loading`]) are not counted.
struct AutoShot {
    path: PathBuf,
    /// Frames to capture, ascending, at least one.
    frames: Vec<u64>,
}

impl AutoShot {
    fn from_env() -> Option<Self> {
        let path = std::env::var_os("KINGDOMS_AUTOSHOT")?;
        let frames = parse_frames(std::env::var("KINGDOMS_AUTOSHOT_FRAME").ok().as_deref());
        Some(Self { path: PathBuf::from(path), frames })
    }

    /// File of frame `frame`: the path itself for a single frame, else `<stem>_<frame>.<ext>`.
    fn path_for(&self, frame: u64) -> PathBuf {
        if self.frames.len() < 2 {
            return self.path.clone();
        }
        let stem = self.path.file_stem().map_or_else(|| "shot".into(), |s| s.to_string_lossy().into_owned());
        let name = match self.path.extension() {
            Some(ext) => format!("{stem}_{frame}.{}", ext.to_string_lossy()),
            None => format!("{stem}_{frame}"),
        };
        self.path.with_file_name(name)
    }

    /// The frame after which the run may end.
    fn last(&self) -> u64 {
        self.frames.last().copied().unwrap_or(0) + 2
    }
}

/// Frames of `KINGDOMS_AUTOSHOT_FRAME`: one number or a comma-separated list (sorted, without
/// duplicates); 90 if missing or invalid.
fn parse_frames(value: Option<&str>) -> Vec<u64> {
    let mut frames: Vec<u64> = value.map(|v| v.split(',').filter_map(|f| f.trim().parse().ok()).collect()).unwrap_or_default();
    frames.sort_unstable();
    frames.dedup();
    if frames.is_empty() {
        frames.push(90);
    }
    frames
}

/// Performance helper: `KINGDOMS_PROFILE=<frames>[@<start>]` keeps every sample of every
/// profiling section for `frames` frames from counted frame `start` (default 60, after the
/// loading screen like the screenshot frames), logs their mean, median, 95th percentile and
/// maximum ([`crate::debug::ProfileReport`]) and then ends the run (once any screenshots are
/// taken as well).
struct Profile {
    start: u64,
    frames: usize,
    done: bool,
}

impl Profile {
    fn from_env() -> Option<Self> {
        Self::parse(&std::env::var("KINGDOMS_PROFILE").ok()?)
    }

    fn parse(value: &str) -> Option<Self> {
        let (frames, start) = match value.split_once('@') {
            Some((f, s)) => (f, s.trim().parse().ok()?),
            None => (value, 60),
        };
        let frames = frames.trim().parse().ok().filter(|f| *f > 0)?;
        Some(Self { start, frames, done: false })
    }
}

/// Scripted runs: screenshots and profile captures at counted frames.
struct Script {
    /// Frames rendered so far without a loading screen.
    counted: u64,
    shot: Option<AutoShot>,
    profile: Option<Profile>,
}

impl Script {
    fn from_env() -> Option<Self> {
        let shot = AutoShot::from_env();
        let profile = Profile::from_env();
        if std::env::var_os("KINGDOMS_PROFILE").is_some() && profile.is_none() {
            log::warn!("KINGDOMS_PROFILE must be <frames>[@<start frame>]");
        }
        (shot.is_some() || profile.is_some()).then_some(Self { counted: 0, shot, profile })
    }

    /// True once every screenshot is taken and the profile is logged.
    fn finished(&self) -> bool {
        self.shot.as_ref().is_none_or(|s| self.counted >= s.last()) && self.profile.as_ref().is_none_or(|p| p.done)
    }
}

struct Running<G: Game> {
    ctx: EngineContext,
    game: G,
    egui_state: egui_winit::State,
    next_frame: Instant,
}

enum AppState<G: Game> {
    Pending(Box<dyn FnOnce() -> G>),
    Running(Box<Running<G>>),
    Finished,
}

struct App<G: Game> {
    config: EngineConfig,
    paths: GamePaths,
    log: LogBuffer,
    state: AppState<G>,
    error: Option<anyhow::Error>,
    script: Option<Script>,
}

/// Frame interval while the window is occluded (enough for the simulation's ticks: up to eight
/// per frame catch up at any game speed).
const OCCLUDED_FRAME_INTERVAL: Duration = Duration::from_millis(100);

/// The event loop. On macOS without winit's default menu bar menu: its "Quit" item (Cmd+Q)
/// terminates the application at once, without the game's close request (the question about
/// unsaved progress); the engine handles Cmd+Q, Cmd+H and Cmd+M itself ([`platform_shortcut`]).
fn build_event_loop() -> Result<EventLoop<()>, winit::error::EventLoopError> {
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_default_menu(false);
    }
    builder.build()
}

/// Window and application shortcuts the engine handles itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlatformShortcut {
    /// Like the window's close button: the game may ask first.
    Quit,
    /// Hide the application.
    Hide,
    /// Minimise the window.
    Minimize,
}

/// The macOS conventions Cmd+Q, Cmd+H and Cmd+M (`macos` false: none – other platforms leave
/// such keys to the window manager). Like every macOS application shortcut they follow the
/// keyboard layout: `key` is the logical key (on AZERTY, Cmd+Q is the key labelled Q, which
/// sits where QWERTY has A).
fn platform_shortcut(key: &winit::keyboard::Key, mods: crate::input::Modifiers, macos: bool) -> Option<PlatformShortcut> {
    use winit::keyboard::Key;
    if !macos || !mods.logo || mods.ctrl || mods.alt {
        return None;
    }
    let Key::Character(text) = key else { return None };
    match text.to_lowercase().as_str() {
        "q" => Some(PlatformShortcut::Quit),
        "h" if !mods.shift => Some(PlatformShortcut::Hide),
        "m" if !mods.shift => Some(PlatformShortcut::Minimize),
        _ => None,
    }
}

/// Runs the game until the window is closed. Blocks the calling (main) thread.
pub fn run<G: Game>(
    config: EngineConfig,
    paths: GamePaths,
    log: LogBuffer,
    factory: impl FnOnce() -> G + 'static,
) -> anyhow::Result<()> {
    let event_loop = build_event_loop()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        config,
        paths,
        log,
        state: AppState::Pending(Box::new(factory)),
        error: None,
        script: Script::from_env(),
    };
    event_loop.run_app(&mut app)?;
    match app.error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Edge length of the window icon in pixels: window managers and the taskbar scale it down;
/// a full-size image would make a property of several megabytes on X11.
const WINDOW_ICON_SIZE: u32 = 128;

/// Reads the window icon from `path`, scaled to [`WINDOW_ICON_SIZE`]; `None` (logged) if the
/// file is missing or no image, so the window simply opens without an icon.
fn load_window_icon(path: &Path) -> Option<Icon> {
    let image = match image::open(path) {
        Ok(image) => image.into_rgba8(),
        Err(err) => {
            log::warn!("no window icon: cannot read {}: {err}", path.display());
            return None;
        }
    };
    let image = if image.width() > WINDOW_ICON_SIZE || image.height() > WINDOW_ICON_SIZE {
        image::imageops::resize(&image, WINDOW_ICON_SIZE, WINDOW_ICON_SIZE, image::imageops::FilterType::Lanczos3)
    } else {
        image
    };
    let (width, height) = image.dimensions();
    match Icon::from_rgba(image.into_raw(), width, height) {
        Ok(icon) => Some(icon),
        Err(err) => {
            log::warn!("no window icon: {}: {err}", path.display());
            None
        }
    }
}

impl<G: Game> App<G> {
    fn start(&mut self, event_loop: &ActiveEventLoop, factory: Box<dyn FnOnce() -> G>) -> anyhow::Result<Running<G>> {
        let wc = &self.config.window;
        let mut attrs = Window::default_attributes()
            .with_title(wc.title.clone())
            .with_inner_size(LogicalSize::new(wc.width, wc.height))
            .with_min_inner_size(LogicalSize::new(800, 480))
            .with_resizable(wc.resizable)
            .with_maximized(wc.maximized);
        if wc.fullscreen {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        // macOS shows the icon of the app bundle; winit ignores a window icon there.
        if cfg!(not(target_os = "macos"))
            && let Some(icon) = wc.icon.as_deref().and_then(|file| load_window_icon(&self.paths.assets.join(file)))
        {
            attrs = attrs.with_window_icon(Some(icon));
        }
        let window = Arc::new(event_loop.create_window(attrs)?);
        let display: Box<dyn wgpu::wgt::WgpuHasDisplayHandle> = Box::new(event_loop.owned_display_handle());
        let mut renderer = Renderer::new(
            window.clone(),
            Some(display),
            self.config.graphics.vsync,
            self.config.graphics.msaa_samples,
        )?;
        let gpu_info = renderer.info();
        let assets = AssetManager::new(&self.paths.assets, &mut renderer);

        let egui_ctx = egui::Context::default();
        egui_ctx.set_zoom_factor(self.config.graphics.ui_scale);
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            event_loop,
            Some(window.scale_factor() as f32),
            None,
            Some(renderer.max_texture_size() as usize),
        );

        let sim = &self.config.simulation;
        let mut ctx = EngineContext {
            time: Time::new(sim.tick_rate, sim.max_ticks_per_frame),
            input: Input::new(),
            window,
            renderer,
            assets,
            audio: crate::audio::Audio::new(&self.paths.assets, self.config.audio.clone()),
            events: EventQueue::default(),
            metrics: FrameMetrics::default(),
            log: self.log.clone(),
            gpu_info,
            config: self.config.clone(),
            paths: self.paths.clone(),
            egui: egui_ctx,
            exit_requested: false,
            loading: false,
            occluded: false,
            ui_min_size: None,
        };
        ctx.input.set_scale_factor(ctx.window.scale_factor());
        // Scripted captures: `KINGDOMS_FIXED_DT=<seconds>` makes every frame last exactly that
        // long, so frame N shows the same moment in every run.
        if let Ok(value) = std::env::var("KINGDOMS_FIXED_DT") {
            match value.trim().parse::<f64>().ok().filter(|s| s.is_finite() && *s > 0.0 && *s <= 1.0) {
                Some(seconds) => {
                    log::info!("fixed frame time: {seconds} s per frame");
                    ctx.time.set_fixed_frame(Some(seconds));
                }
                None => log::warn!("KINGDOMS_FIXED_DT={value:?} is invalid (seconds per frame, 0 < s ≤ 1)"),
            }
        }
        let mut game = factory();
        let init_start = Instant::now();
        game.init(&mut ctx)?;
        log::info!("game initialised in {:.0} ms", init_start.elapsed().as_secs_f64() * 1000.0);
        ctx.time.reset_accumulator();
        Ok(Running { ctx, game, egui_state, next_frame: Instant::now() })
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, err: anyhow::Error) {
        log::error!("fatal: {err:#}");
        self.error = Some(err);
        self.state = AppState::Finished;
        event_loop.exit();
    }
}

impl<G: Game> ApplicationHandler for App<G> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !matches!(self.state, AppState::Pending(_)) {
            return;
        }
        let AppState::Pending(factory) = std::mem::replace(&mut self.state, AppState::Finished) else {
            return;
        };
        match self.start(event_loop, factory) {
            Ok(running) => self.state = AppState::Running(Box::new(running)),
            Err(err) => self.fail(event_loop, err),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let AppState::Running(running) = &mut self.state else { return };
        let Running { ctx, game, egui_state, .. } = running.as_mut();

        let response = egui_state.on_window_event(&ctx.window, &event);
        ctx.input.handle_window_event(&event, response.consumed);

        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == winit::event::ElementState::Pressed
            && !key.repeat
        {
            match platform_shortcut(&key.logical_key, ctx.input.modifiers(), cfg!(target_os = "macos")) {
                Some(PlatformShortcut::Quit) => {
                    if game.on_close_requested(ctx) {
                        ctx.request_exit();
                    }
                }
                Some(PlatformShortcut::Minimize) => ctx.window.set_minimized(true),
                Some(PlatformShortcut::Hide) => {
                    #[cfg(target_os = "macos")]
                    {
                        use winit::platform::macos::ActiveEventLoopExtMacOS;
                        event_loop.hide_application();
                    }
                }
                None => {}
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                if game.on_close_requested(ctx) {
                    ctx.request_exit();
                }
            }
            WindowEvent::Resized(size) => {
                ctx.renderer.resize(size.width, size.height);
                ctx.events.push(EngineEvent::Resized { width: size.width, height: size.height });
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                ctx.events.push(EngineEvent::ScaleFactorChanged(scale_factor));
            }
            WindowEvent::Focused(focused) => ctx.events.push(EngineEvent::FocusChanged(focused)),
            WindowEvent::Occluded(occluded) => ctx.occluded = occluded,
            WindowEvent::DroppedFile(path) => ctx.events.push(EngineEvent::FileDropped(path)),
            WindowEvent::RedrawRequested => {
                let result = run_frame(running.as_mut(), self.script.as_mut());
                if let Err(err) = result {
                    self.fail(event_loop, err);
                    return;
                }
            }
            _ => {}
        }

        if let AppState::Running(running) = &mut self.state
            && running.ctx.exit_requested
        {
            running.game.on_exit(&mut running.ctx);
            log::info!("exiting after {} frames", running.ctx.time.frame_count());
            self.state = AppState::Finished;
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let AppState::Running(running) = &mut self.state else { return };
        let size = running.ctx.window.inner_size();
        if size.width == 0 || size.height == 0 {
            // Minimised: don't spin.
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50)));
            return;
        }
        if running.ctx.occluded {
            // Hidden but not resized (macOS keeps the size of covered and minimised windows):
            // a few frames per second keep the game running without spending power on frames
            // nobody sees.
            let now = Instant::now();
            if now >= running.next_frame {
                running.next_frame = now + OCCLUDED_FRAME_INTERVAL;
                running.ctx.window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(running.next_frame));
            return;
        }
        let g = &running.ctx.config.graphics;
        if !g.vsync && g.max_fps > 0 {
            let now = Instant::now();
            if now >= running.next_frame {
                let period = Duration::from_secs_f64(1.0 / f64::from(g.max_fps));
                running.next_frame = (running.next_frame + period).max(now);
                running.ctx.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::Poll);
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(running.next_frame));
            }
        } else {
            event_loop.set_control_flow(ControlFlow::Poll);
            running.ctx.window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let AppState::Running(running) = &mut self.state {
            running.game.on_exit(&mut running.ctx);
        }
        log::logger().flush();
    }
}

fn run_frame<G: Game>(running: &mut Running<G>, mut script: Option<&mut Script>) -> anyhow::Result<()> {
    let Running { ctx, game, egui_state, .. } = running;
    let frame_start = Instant::now();
    let ticks = ctx.time.begin_frame();
    if let Some(s) = script.as_deref_mut()
        && !ctx.loading
    {
        s.counted += 1;
        if let Some(p) = s.profile.as_ref()
            && s.counted == p.start
        {
            log::info!("profile: capturing {} frames from frame {}", p.frames, p.start);
            ctx.metrics.begin_capture();
        }
    }
    ctx.metrics.record_frame(Duration::from_secs_f32(ctx.time.delta()));
    ctx.audio.update(ctx.time.delta());

    // Route input: the world ignores pointer/keyboard while egui uses them.
    let egui_ctx = ctx.egui.clone();
    // The window may have changed: keep the UI scale within what the layout needs.
    let zoom = ctx.effective_ui_scale();
    if (egui_ctx.zoom_factor() - zoom).abs() > 1e-4 {
        egui_ctx.set_zoom_factor(zoom);
    }
    ctx.input.set_ui_capture(
        egui_ctx.egui_wants_pointer_input() || egui_ctx.is_pointer_over_egui(),
        egui_ctx.egui_wants_keyboard_input(),
    );

    let t = Instant::now();
    game.update(ctx);
    ctx.metrics.record_section("update", t.elapsed());

    let t = Instant::now();
    for _ in 0..ticks {
        game.fixed_update(ctx);
        ctx.time.record_tick();
    }
    if ticks > 0 {
        ctx.metrics.record_section("simulation", t.elapsed());
    }

    let t = Instant::now();
    let raw_input = egui_state.take_egui_input(&ctx.window);
    let full_output = egui_ctx.run_ui(raw_input, |ui| game.ui(ctx, ui));
    egui_state.handle_platform_output(&ctx.window, full_output.platform_output);
    let primitives = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
    ctx.metrics.record_section("ui", t.elapsed());

    let t = Instant::now();
    let mut list = game.render(ctx);
    ctx.metrics.record_section("build render list", t.elapsed());
    // Fixed frames: the shader clock counts the frames after the loading screen, like the
    // screenshot frames, so the water looks the same however long the loading took.
    if let Some(dt) = ctx.time.fixed_frame() {
        let frames = script.as_deref().map_or(ctx.time.frame_count(), |s| s.counted);
        list.shader_time = Some((frames as f64 * dt) as f32);
    }

    if let Some(s) = script.as_deref()
        && let Some(shot) = s.shot.as_ref()
        && shot.frames.contains(&s.counted)
        && !ctx.loading
    {
        ctx.renderer.request_screenshot(shot.path_for(s.counted));
    }

    let t = Instant::now();
    let paint = EguiPaint {
        primitives,
        textures_delta: full_output.textures_delta,
        pixels_per_point: full_output.pixels_per_point,
    };
    ctx.renderer.render(&list, Some(paint))?;
    ctx.renderer.recycle_list(list);
    ctx.metrics.record_section("render", t.elapsed());
    let steps = ctx.renderer.stats();
    for (name, ms) in [
        ("render: acquire", steps.acquire_ms),
        ("render: prepare", steps.prepare_ms),
        ("render: encode", steps.encode_ms),
        ("render: submit", steps.submit_ms),
    ] {
        ctx.metrics.record_section(name, Duration::from_secs_f32(ms.max(0.0) / 1000.0));
    }
    ctx.metrics.record_section("frame (cpu)", frame_start.elapsed());

    ctx.input.end_frame();
    if let Some(s) = script {
        if let Some(p) = s.profile.as_mut()
            && !p.done
            && ctx.metrics.captured_frames() >= p.frames
            && let Some(report) = ctx.metrics.end_capture()
        {
            let r = ctx.renderer.stats();
            log::info!(
                "{}  last frame: {} dynamic + {} static sprites, {} draw calls, {} of {} static batches drawn, {} culled on the CPU",
                report.table(),
                r.sprites_dynamic,
                r.sprites_static,
                r.draw_calls,
                r.static_batches_drawn,
                r.static_batches_drawn + r.static_batches_culled,
                r.sprites_culled_cpu
            );
            p.done = true;
        }
        if s.finished() {
            ctx.request_exit();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The icon is read and scaled down; a missing or broken file means no icon, no error.
    #[test]
    fn window_icons_load_scaled_or_not_at_all() {
        let dir = std::env::temp_dir().join(format!("kingdoms-window-icon-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let big = dir.join("big.png");
        let small = dir.join("small.png");
        let broken = dir.join("broken.png");
        let saved = image::RgbaImage::from_pixel(300, 300, image::Rgba([200, 30, 30, 255])).save(&big).is_ok()
            && image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 0, 0, 255])).save(&small).is_ok()
            && std::fs::write(&broken, b"no png").is_ok();
        assert!(saved, "test files written");
        assert!(load_window_icon(&big).is_some());
        assert!(load_window_icon(&small).is_some());
        assert!(load_window_icon(&broken).is_none());
        assert!(load_window_icon(&dir.join("missing.png")).is_none());
        // The shipped icon loads.
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/app_icon.png");
        assert!(load_window_icon(&shipped).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mac_shortcuts_need_command_and_only_apply_on_macos() {
        use crate::input::Modifiers;
        use winit::keyboard::{Key, NamedKey};
        let key = |c: &str| Key::Character(c.into());
        let cmd = Modifiers { logo: true, ..Modifiers::NONE };
        assert_eq!(platform_shortcut(&key("q"), cmd, true), Some(PlatformShortcut::Quit));
        assert_eq!(platform_shortcut(&key("Q"), Modifiers { shift: true, ..cmd }, true), Some(PlatformShortcut::Quit));
        assert_eq!(platform_shortcut(&key("h"), cmd, true), Some(PlatformShortcut::Hide));
        assert_eq!(platform_shortcut(&key("m"), cmd, true), Some(PlatformShortcut::Minimize));
        // The layout decides: on AZERTY the physical A key types „q“ and quits, the physical
        // Q key types „a“ and does not.
        assert_eq!(platform_shortcut(&key("a"), cmd, true), None);
        assert_eq!(platform_shortcut(&key("s"), cmd, true), None, "Cmd+S is the game's");
        assert_eq!(platform_shortcut(&Key::Named(NamedKey::Escape), cmd, true), None);
        assert_eq!(platform_shortcut(&key("q"), Modifiers::NONE, true), None);
        assert_eq!(platform_shortcut(&key("h"), Modifiers { alt: true, ..cmd }, true), None, "Cmd+Alt+H: hide others");
        assert_eq!(platform_shortcut(&key("q"), cmd, false), None, "other platforms: the window manager's");
    }

    #[test]
    fn scripted_frames_and_profiles_parse() {
        assert_eq!(parse_frames(None), vec![90]);
        assert_eq!(parse_frames(Some("30")), vec![30]);
        assert_eq!(parse_frames(Some("33, 30,30")), vec![30, 33]);
        assert_eq!(parse_frames(Some("x")), vec![90]);
        let one = AutoShot { path: PathBuf::from("/tmp/a/shot.png"), frames: vec![30] };
        assert_eq!(one.path_for(30), PathBuf::from("/tmp/a/shot.png"));
        assert_eq!(one.last(), 32);
        let two = AutoShot { path: PathBuf::from("/tmp/a/shot.png"), frames: vec![30, 33] };
        assert_eq!(two.path_for(33), PathBuf::from("/tmp/a/shot_33.png"));
        assert_eq!(two.last(), 35);
        let p = Profile::parse("300@40");
        assert!(p.as_ref().is_some_and(|p| p.frames == 300 && p.start == 40));
        assert!(Profile::parse("200").is_some_and(|p| p.start == 60));
        assert!(Profile::parse("0").is_none() && Profile::parse("x@3").is_none() && Profile::parse("5@x").is_none());
        let script = Script { counted: 40, shot: Some(two), profile: Some(Profile { start: 1, frames: 2, done: true }) };
        assert!(script.finished());
        let pending = Script { counted: 34, shot: None, profile: Some(Profile { start: 1, frames: 2, done: false }) };
        assert!(!pending.finished());
    }

    #[test]
    fn the_ui_scale_is_lowered_to_fit_the_layout_never_raised() {
        let layout = Some(glam::Vec2::new(1280.0, 720.0));
        let small = glam::Vec2::new(1280.0, 720.0);
        let large = glam::Vec2::new(1920.0, 1080.0);
        assert_eq!(effective_ui_scale(1.5, small, layout), 1.0, "150 % do not fit 1280 × 720");
        assert_eq!(effective_ui_scale(1.25, large, layout), 1.25);
        assert_eq!(effective_ui_scale(1.5, large, layout), 1.5);
        assert_eq!(effective_ui_scale(0.75, small, layout), 0.75, "never raised");
        assert!((effective_ui_scale(1.0, glam::Vec2::new(1024.0, 600.0), layout) - 0.8).abs() < 1e-6);
        assert_eq!(effective_ui_scale(1.5, small, None), 1.5, "no limit without a layout size");
        assert_eq!(effective_ui_scale(1.0, glam::Vec2::new(100.0, 100.0), layout), 0.5, "at least 50 %");
        assert_eq!(effective_ui_scale(1.5, glam::Vec2::ZERO, layout), 1.5, "a minimised window keeps its scale");
    }
}
