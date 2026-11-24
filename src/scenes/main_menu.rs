//! The main menu: a living kingdom in the background (a small world from a fixed seed,
//! generated on a worker thread and simulated while the camera drifts across it), the title and
//! the entries Fortsetzen (newest save), Neues Spiel, Spiel laden, Einstellungen, Credits and
//! Beenden. Sub-pages open as parchment pages over the same background. Mouse and keyboard
//! (arrow keys / W S, Enter, Esc).

use std::sync::Arc;
use std::sync::mpsc;

use glam::Vec2;
use kingdoms_engine::audio::MusicMood;
use kingdoms_engine::core::app::EngineContext;
use kingdoms_engine::core::scene::{Scene, Transition};
use kingdoms_engine::core::time::GameSpeed;
use kingdoms_engine::egui::{self, Align2, FontId, Pos2, RichText};
use kingdoms_engine::rendering::camera::Camera2D;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso;
use kingdoms_engine::rendering::sprite::RenderList;
use kingdoms_sim::defs::{Defs, Weather};
use kingdoms_sim::time::{TICKS_PER_DAY, TICKS_PER_HOUR};
use kingdoms_sim::world::WorldSize;

use crate::game::GameShared;
use crate::save::slots::{SaveEntry, list_saves, newest_loadable, saved_at_text};
use crate::scenes::loading::LoadingScene;
use crate::session::{DevPlan, DevWeather, LaunchOptions, Session};
use crate::ui::menu::new_game::{NewGameAction, NewGameForm};
use crate::ui::menu::save_browser::{BrowserAction, BrowserMode, SaveBrowser};
use crate::ui::menu::settings_panel::{SettingsPanel, SettingsTab};
use crate::ui::menu::{self, PAGE_NARROW, PAGE_WIDE, backdrop, credits, list_navigation, menu_button, paint_title, show_page};
use crate::ui::theme::Palette;
use crate::view::camera::CameraController;
use crate::view::citizens::WalkerMarks;
use crate::view::picking::Hover;
use crate::view::world_view::{Highlights, WorldView};

/// Seed of the background world (a river valley with forest around the start).
const BACKGROUND_SEED: u64 = 7;
/// Ticks simulated before the background shows: the developer town grows for two days and
/// the view starts on the third morning at 09:00, when everybody is out and about.
const BACKGROUND_WARP: u64 = 2 * TICKS_PER_DAY + 3 * TICKS_PER_HOUR;
/// Seconds of the fade-in of the background.
const FADE_SECONDS: f64 = 1.5;
/// Width of the main menu buttons.
const BUTTON_WIDTH: f32 = 330.0;

/// A page of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Main,
    NewGame,
    Load,
    Settings,
    Credits,
}

/// Entries of the main page in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Continue,
    NewGame,
    Load,
    Settings,
    Credits,
    Quit,
}

const ENTRIES: [Entry; 6] = [Entry::Continue, Entry::NewGame, Entry::Load, Entry::Settings, Entry::Credits, Entry::Quit];

impl Entry {
    const fn label(self) -> &'static str {
        match self {
            Entry::Continue => "Fortsetzen",
            Entry::NewGame => "Neues Spiel",
            Entry::Load => "Spiel laden",
            Entry::Settings => "Einstellungen",
            Entry::Credits => "Credits",
            Entry::Quit => "Beenden",
        }
    }
}

/// The simulated world behind the menu.
struct Background {
    job: Option<mpsc::Receiver<Session>>,
    world: Option<(Session, WorldView, CameraController)>,
    /// Real time the world became visible.
    shown_at: f64,
}

impl Background {
    fn new(defs: Arc<Defs>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("menu-background".into()).spawn(move || {
            // The developer town (built through regular commands) makes a lively picture; the
            // background is never saved or played.
            let options = LaunchOptions {
                seed: BACKGROUND_SEED,
                world_size: WorldSize::Small,
                warp_ticks: BACKGROUND_WARP,
                dev_settlement: Some(DevPlan::Town),
                dev_weather: Some(DevWeather { weather: Weather::Clear, intensity: 60, days: 3 }),
                ..LaunchOptions::default()
            };
            let session = Session::launch(defs, &options, || {});
            let _ = sender.send(session);
        });
        let job = match spawned {
            Ok(_) => Some(receiver),
            Err(err) => {
                log::warn!("no menu background ({err})");
                None
            }
        };
        Self { job, world: None, shown_at: 0.0 }
    }

    /// Builds the view once the world is ready.
    fn update(&mut self, ctx: &mut EngineContext) {
        let Some(job) = &self.job else { return };
        match job.try_recv() {
            Ok(mut session) => {
                self.job = None;
                let mut view = WorldView::new(&mut session, &mut ctx.renderer, &ctx.assets);
                view.show_territory = false;
                let mut camera = CameraController::new(ctx);
                camera.set_map_bounds(session.state().world().size());
                camera.set_zoom(1.15);
                self.world = Some((session, view, camera));
                self.shown_at = ctx.time.elapsed();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                log::warn!("the menu background could not be created");
                self.job = None;
            }
        }
    }

    fn fixed_update(&mut self) {
        if let Some((session, ..)) = &mut self.world {
            session.tick();
        }
    }

    fn render(&mut self, ctx: &mut EngineContext) -> RenderList {
        let Some((session, view, camera)) = &mut self.world else {
            let mut list = ctx.renderer.begin_list(Camera2D::new(ctx.window_size(), ctx.scale_factor()));
            list.clear_color = Color::from_srgb_u8(20, 14, 9, 255);
            return list;
        };
        camera.camera.set_viewport(ctx.window_size(), ctx.scale_factor());
        // The town sits right of the menu buttons; the view drifts slowly on an ellipse around
        // it (one round in about four minutes).
        let t = (ctx.time.elapsed() - self.shown_at) as f32;
        let angle = t * 0.026;
        let home = session.home_tile();
        let level = session.state().world().get(home).map_or(0.0, |t| f32::from(t.elevation));
        let town = iso::tile_to_world_elevated(home.as_vec2() + Vec2::splat(0.5), level);
        let logical_width = ctx.window_size().x / ctx.scale_factor().max(0.1);
        let shift = Vec2::new(-0.2 * logical_width / camera.camera.zoom, 0.0);
        let drift = Vec2::new(angle.cos() * 150.0, angle.sin() * 70.0);
        camera.camera.look_at(town + shift + drift);
        view.sync(session, &mut ctx.renderer, &ctx.assets);
        view.update_effects(ctx.time.delta(), session, &camera.camera, &ctx.assets);
        let mut list = ctx.renderer.begin_list(camera.camera.clone());
        list.clear_color = Color::from_srgb_u8(18, 30, 44, 255);
        let highlights = Highlights { hover: Hover::default(), selection: None, demolish: false, hover_outline: false, walkers: WalkerMarks::default(), orders: false };
        let alpha = ctx.time.alpha();
        let ticks = session.state().clock().tick as f64 + f64::from(alpha);
        view.draw(&mut list, session, &ctx.assets, &highlights, ticks, alpha);
        list
    }

    /// Fade-in factor 0–1 of the world.
    fn visibility(&self, now: f64) -> f32 {
        if self.world.is_none() { 0.0 } else { ((now - self.shown_at) / FADE_SECONDS).clamp(0.0, 1.0) as f32 }
    }

    fn release(&mut self, ctx: &mut EngineContext) {
        if let Some((_, view, _)) = &mut self.world {
            view.release(&mut ctx.renderer);
        }
        self.world = None;
        self.job = None;
    }
}

/// Line under „Fortsetzen“ without a loadable save: none at all, or only saves this version
/// cannot load.
pub fn continue_missing_text(saves: usize) -> String {
    match saves {
        0 => "Noch kein Spielstand vorhanden".to_owned(),
        1 => "Ein Spielstand vorhanden, aber nicht ladbar".to_owned(),
        n => format!("{n} Spielstände vorhanden, aber keiner ladbar"),
    }
}

pub struct MainMenuScene {
    defs: Arc<Defs>,
    page: Page,
    focus: usize,
    background: Background,
    /// Saves of the saves directory (for "Fortsetzen"), newest first.
    saves: Vec<SaveEntry>,
    new_game: Option<NewGameForm>,
    browser: Option<SaveBrowser>,
    settings: SettingsPanel,
}

impl MainMenuScene {
    pub fn new(_shared: &mut GameShared, ctx: &mut EngineContext, defs: Arc<Defs>) -> Self {
        let saves = list_saves(&ctx.paths.saves_dir(), defs.fingerprint());
        let focus = if newest_loadable(&saves).is_some() { 0 } else { 1 };
        Self {
            background: Background::new(defs.clone()),
            defs,
            page: Page::Main,
            focus,
            saves,
            new_game: None,
            browser: None,
            settings: SettingsPanel::new(SettingsTab::Audio),
        }
    }

    fn open(&mut self, page: Page, ctx: &mut EngineContext) {
        self.page = page;
        match page {
            Page::Main => {
                self.saves = list_saves(&ctx.paths.saves_dir(), self.defs.fingerprint());
                ctx.audio.play("ui_close");
            }
            Page::NewGame => {
                if self.new_game.is_none() {
                    self.new_game = Some(NewGameForm::new(&self.defs));
                }
                ctx.audio.play("ui_open");
            }
            Page::Load => {
                self.browser = Some(SaveBrowser::new(BrowserMode::Load, &ctx.paths.saves_dir(), self.defs.fingerprint(), String::new()));
                ctx.audio.play("ui_open");
            }
            Page::Settings => {
                self.settings = SettingsPanel::new(SettingsTab::Audio);
                ctx.audio.play("ui_open");
            }
            Page::Credits => ctx.audio.play("ui_open"),
        }
    }

    fn enabled(&self, entry: Entry) -> bool {
        entry != Entry::Continue || newest_loadable(&self.saves).is_some()
    }

    fn activate(&mut self, entry: Entry, ctx: &mut EngineContext) -> Transition<GameShared> {
        match entry {
            Entry::Continue => match newest_loadable(&self.saves) {
                Some(save) => {
                    ctx.audio.play("ui_click");
                    let (path, name) = (save.path.clone(), save.display_name());
                    Transition::Replace(Box::new(LoadingScene::load(ctx, self.defs.clone(), path, name)))
                }
                None => Transition::None,
            },
            Entry::NewGame => {
                self.open(Page::NewGame, ctx);
                Transition::None
            }
            Entry::Load => {
                self.open(Page::Load, ctx);
                Transition::None
            }
            Entry::Settings => {
                self.open(Page::Settings, ctx);
                Transition::None
            }
            Entry::Credits => {
                self.open(Page::Credits, ctx);
                Transition::None
            }
            Entry::Quit => Transition::Quit,
        }
    }

    /// The title and the main buttons on the left.
    fn main_page(&mut self, ctx: &mut EngineContext, egui_ctx: &egui::Context) -> Transition<GameShared> {
        let screen = egui_ctx.content_rect();
        let scale = (screen.height() / 900.0).clamp(0.72, 1.15);
        let left = screen.left() + (screen.width() * 0.07).max(36.0);
        let painter = egui_ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("menu_title")));
        let title = paint_title(&painter, Pos2::new(left, screen.top() + screen.height() * 0.1), scale);
        let mut clicked = None;
        let entries: Vec<(Entry, bool)> = ENTRIES.iter().map(|e| (*e, self.enabled(*e))).collect();
        if let Some(i) = list_navigation(egui_ctx, &mut self.focus, entries.len(), |i| entries[i].1) {
            clicked = Some(entries[i].0);
        }
        let continue_info = newest_loadable(&self.saves)
            .map(|s| format!("„{}“ · {}", s.display_name(), saved_at_text(s.saved_at())));
        egui::Area::new(egui::Id::unique("main_menu_buttons"))
            .order(egui::Order::Middle)
            .fixed_pos(Pos2::new(left, (title.bottom() + 46.0 * scale).max(screen.top() + screen.height() * 0.36)))
            .show(egui_ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0 * scale.min(1.0);
                for (i, (entry, enabled)) in entries.iter().enumerate() {
                    let response = menu_button(ui, entry.label(), BUTTON_WIDTH, *enabled, self.focus == i);
                    if response.hovered() && *enabled {
                        self.focus = i;
                    }
                    if response.clicked() {
                        clicked = Some(*entry);
                    }
                    if *entry == Entry::Continue {
                        let text = continue_info.clone().unwrap_or_else(|| continue_missing_text(self.saves.len()));
                        ui.add_space(-6.0);
                        ui.label(RichText::new(text).font(FontId::proportional(14.0)).color(Palette::ON_WOOD_SOFT));
                        ui.add_space(4.0);
                    }
                    if *entry == Entry::Load {
                        ui.add_space(6.0);
                    }
                }
            });
        let footer = egui_ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("menu_footer")));
        let version = format!("Version {} · Prototyp mit Platzhalter-Grafik", env!("CARGO_PKG_VERSION"));
        footer.text(Pos2::new(left, screen.bottom() - 22.0), Align2::LEFT_BOTTOM, version, FontId::proportional(14.0), Palette::ON_WOOD_SOFT);
        match clicked {
            Some(entry) => self.activate(entry, ctx),
            None => Transition::None,
        }
    }
}

impl Scene<GameShared> for MainMenuScene {
    fn name(&self) -> &'static str {
        "main_menu"
    }

    fn on_enter(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) {
        ctx.time.set_speed(GameSpeed::Normal);
        ctx.time.reset_accumulator();
        ctx.audio.set_music_mood(MusicMood::Menu);
    }

    fn on_exit(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) {
        self.background.release(ctx);
    }

    fn fixed_update(&mut self, _shared: &mut GameShared, _ctx: &mut EngineContext) {
        self.background.fixed_update();
    }

    fn update(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) -> Transition<GameShared> {
        self.background.update(ctx);
        Transition::None
    }

    fn ui(&mut self, shared: &mut GameShared, ctx: &mut EngineContext, ui: &mut egui::Ui) -> Transition<GameShared> {
        let egui_ctx = ui.ctx().clone();
        let now = ctx.time.elapsed();
        let shown = self.background.visibility(now);
        // Before the world is ready the backdrop is dark wood, then the world fades in.
        if shown < 1.0 {
            menu::wood_backdrop(&egui_ctx, 1.0 - shown);
        }
        let page_shade = if self.page == Page::Main { 0.0 } else { 0.35 };
        backdrop(&egui_ctx, if self.page == Page::Main { 1.0 } else { 0.4 }, shown * page_shade);
        if shown < 1.0 {
            egui_ctx.request_repaint();
        }
        let mut transition = Transition::None;
        match self.page {
            Page::Main => transition = self.main_page(ctx, &egui_ctx),
            Page::NewGame => {
                let defs = self.defs.clone();
                let action = match self.new_game.as_mut() {
                    Some(form) => show_page(&egui_ctx, "menu_new_game", PAGE_WIDE, |ui| form.show(ui, &defs, now)),
                    None => NewGameAction::Back,
                };
                match action {
                    NewGameAction::None => {}
                    NewGameAction::Back => self.open(Page::Main, ctx),
                    NewGameAction::Start(settings) => {
                        ctx.audio.play("ui_click");
                        transition = Transition::Replace(Box::new(LoadingScene::new_game(ctx, defs, settings)));
                    }
                }
            }
            Page::Load => {
                let dir = ctx.paths.saves_dir();
                let fingerprint = self.defs.fingerprint();
                let action = match self.browser.as_mut() {
                    Some(browser) => show_page(&egui_ctx, "menu_load", PAGE_WIDE, |ui| browser.show(ui, &dir, fingerprint, false)),
                    None => BrowserAction::Close,
                };
                match action {
                    BrowserAction::None | BrowserAction::Save { .. } => {}
                    BrowserAction::Close => self.open(Page::Main, ctx),
                    BrowserAction::Load { path, name } => {
                        ctx.audio.play("ui_click");
                        transition = Transition::Replace(Box::new(LoadingScene::load(ctx, self.defs.clone(), path, name)));
                    }
                }
            }
            Page::Settings => {
                let closed = show_page(&egui_ctx, "menu_settings", PAGE_NARROW, |ui| shared.settings_panel(&mut self.settings, ui, ctx));
                if closed {
                    self.page = Page::Main;
                }
            }
            Page::Credits => {
                if show_page(&egui_ctx, "menu_credits", PAGE_WIDE, credits::show) {
                    self.open(Page::Main, ctx);
                }
            }
        }
        if self.background.world.is_none() {
            // A quiet hint while the background world is still being generated.
            let screen = egui_ctx.content_rect();
            let painter = egui_ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("menu_wait")));
            menu::spinner(&painter, Pos2::new(screen.right() - 40.0, screen.bottom() - 40.0), 12.0, now);
            egui_ctx.request_repaint();
        }
        transition
    }

    fn render(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) -> Option<RenderList> {
        Some(self.background.render(ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continue_line_tells_missing_from_unloadable_saves() {
        assert_eq!(continue_missing_text(0), "Noch kein Spielstand vorhanden");
        assert_eq!(continue_missing_text(1), "Ein Spielstand vorhanden, aber nicht ladbar");
        assert_eq!(continue_missing_text(4), "4 Spielstände vorhanden, aber keiner ladbar");
    }
}
