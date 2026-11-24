//! First scene: decodes the sprite atlases on worker threads behind the loading screen (the
//! textures are uploaded a few per frame, so the window stays responsive), then opens the main
//! menu or starts a game directly ([`StartMode`]).

use kingdoms_engine::assets::AtlasLoading;
use kingdoms_engine::core::app::EngineContext;
use kingdoms_engine::core::scene::{Scene, Transition};
use kingdoms_engine::egui;
use kingdoms_engine::rendering::camera::Camera2D;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::sprite::RenderList;

use crate::game::GameShared;
use crate::scenes::loading::LoadingScene;
use crate::scenes::main_menu::MainMenuScene;
use crate::session::StartMode;
use crate::ui::menu::loading_screen::{self, LoadingView, StageState};

/// Upload time per frame while loading.
const UPLOAD_BUDGET: std::time::Duration = std::time::Duration::from_millis(12);

pub struct BootScene {
    loading: AtlasLoading,
    start: Option<StartMode>,
    started: f64,
    finished: bool,
}

impl BootScene {
    pub fn new(ctx: &mut EngineContext, start: StartMode) -> Self {
        let loading = ctx.assets.start_loading_atlases(&ctx.renderer);
        Self { loading, start: Some(start), started: ctx.time.elapsed(), finished: false }
    }
}

impl Scene<GameShared> for BootScene {
    fn name(&self) -> &'static str {
        "boot"
    }

    fn on_enter(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) {
        ctx.set_loading(true);
    }

    fn update(&mut self, shared: &mut GameShared, ctx: &mut EngineContext) -> Transition<GameShared> {
        if !self.finished {
            self.finished = ctx.assets.continue_loading_atlases(&mut self.loading, &mut ctx.renderer, Some(UPLOAD_BUDGET));
            if !self.finished {
                return Transition::None;
            }
            if self.loading.loaded() == 0 {
                log::warn!("no sprite atlases loaded - the game will render fallback sprites");
            }
        }
        let Ok(defs) = shared.defs() else { return Transition::Quit };
        match self.start.take() {
            Some(StartMode::Menu) => {
                ctx.set_loading(false);
                Transition::Replace(Box::new(MainMenuScene::new(shared, ctx, defs)))
            }
            Some(StartMode::Game(options)) => Transition::Replace(Box::new(LoadingScene::launch(ctx, defs, *options))),
            None => Transition::None,
        }
    }

    fn ui(&mut self, shared: &mut GameShared, ctx: &mut EngineContext, ui: &mut egui::Ui) -> Transition<GameShared> {
        let total = self.loading.total();
        let graphics = if self.finished || total == 0 {
            ("Grafiken geladen".to_owned(), StageState::Done)
        } else {
            (format!("Grafiken werden geladen ({} von {total})", self.loading.done()), StageState::Active)
        };
        let view = LoadingView {
            heading: "Das Reich erwacht",
            stages: vec![("Spieldaten geprüft".to_owned(), StageState::Done), graphics],
            progress: Some(self.loading.progress()),
            elapsed: ctx.time.elapsed() - self.started,
            error: None,
        };
        loading_screen::show(ui.ctx(), &view, &shared.actions);
        ui.ctx().request_repaint();
        Transition::None
    }

    fn render(&mut self, _shared: &mut GameShared, ctx: &mut EngineContext) -> Option<RenderList> {
        let mut list = ctx.renderer.begin_list(Camera2D::new(ctx.window_size(), ctx.scale_factor()));
        list.clear_color = Color::from_srgb_u8(20, 14, 9, 255);
        Some(list)
    }
}
