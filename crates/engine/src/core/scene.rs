//! Scene management: a stack of scenes sharing game-wide state.
//!
//! The top scene receives updates, simulation ticks and UI. Rendering falls through to the first
//! scene (from the top) that produces a render list, so overlay scenes such as a pause menu can
//! leave the world visible underneath.

use super::app::{EngineContext, Game};
use crate::rendering::camera::Camera2D;
use crate::rendering::sprite::RenderList;

/// What the scene stack should do after a scene callback.
pub enum Transition<S> {
    None,
    Push(Box<dyn Scene<S>>),
    Pop,
    Replace(Box<dyn Scene<S>>),
    /// Removes all scenes and starts fresh with the given one.
    Reset(Box<dyn Scene<S>>),
    Quit,
}

impl<S> Transition<S> {
    pub fn is_none(&self) -> bool {
        matches!(self, Transition::None)
    }
}

/// One game screen (main menu, world, loading screen, ...). `S` is shared game state.
pub trait Scene<S>: 'static {
    fn name(&self) -> &'static str;
    fn on_enter(&mut self, _shared: &mut S, _ctx: &mut EngineContext) {}
    fn on_exit(&mut self, _shared: &mut S, _ctx: &mut EngineContext) {}
    /// Another scene was pushed on top of this one.
    fn on_pause(&mut self, _shared: &mut S, _ctx: &mut EngineContext) {}
    /// The scene above was popped.
    fn on_resume(&mut self, _shared: &mut S, _ctx: &mut EngineContext) {}
    fn fixed_update(&mut self, _shared: &mut S, _ctx: &mut EngineContext) {}
    fn update(&mut self, _shared: &mut S, _ctx: &mut EngineContext) -> Transition<S> {
        Transition::None
    }
    fn ui(&mut self, _shared: &mut S, _ctx: &mut EngineContext, _ui: &mut egui::Ui) -> Transition<S> {
        Transition::None
    }
    /// Returns `None` to let the scene below render (overlays).
    fn render(&mut self, _shared: &mut S, _ctx: &mut EngineContext) -> Option<RenderList> {
        None
    }
    /// Window close requested while this scene is on top. Return `false` to veto.
    fn on_close_requested(&mut self, _shared: &mut S, _ctx: &mut EngineContext) -> bool {
        true
    }
    /// Called before exit (e.g. to autosave).
    fn on_app_exit(&mut self, _shared: &mut S, _ctx: &mut EngineContext) {}
}

type InitialScene<S> = Box<dyn FnOnce(&mut S, &mut EngineContext) -> anyhow::Result<Box<dyn Scene<S>>>>;
type SharedInit<S> = Box<dyn FnOnce(&mut S, &mut EngineContext) -> anyhow::Result<()>>;
type ExitHook<S> = Box<dyn FnMut(&mut S, &mut EngineContext)>;

/// A [`Game`] implementation driven by a stack of scenes.
pub struct SceneStack<S: 'static> {
    shared: S,
    stack: Vec<Box<dyn Scene<S>>>,
    initial: Option<InitialScene<S>>,
    shared_init: Option<SharedInit<S>>,
    exit_hook: Option<ExitHook<S>>,
}

impl<S: 'static> SceneStack<S> {
    /// `init_shared` runs once when the engine is ready (load data, assets, settings); then
    /// `initial` creates the first scene.
    pub fn new(
        shared: S,
        init_shared: impl FnOnce(&mut S, &mut EngineContext) -> anyhow::Result<()> + 'static,
        initial: impl FnOnce(&mut S, &mut EngineContext) -> anyhow::Result<Box<dyn Scene<S>>> + 'static,
    ) -> Self {
        Self {
            shared,
            stack: Vec::new(),
            initial: Some(Box::new(initial)),
            shared_init: Some(Box::new(init_shared)),
            exit_hook: None,
        }
    }

    /// Runs `hook` on the shared state when the application exits, after every scene's
    /// [`Scene::on_app_exit`] (e.g. to write settings that are still unsaved).
    pub fn with_exit_hook(mut self, hook: impl FnMut(&mut S, &mut EngineContext) + 'static) -> Self {
        self.exit_hook = Some(Box::new(hook));
        self
    }

    pub fn shared(&self) -> &S {
        &self.shared
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn top_name(&self) -> Option<&'static str> {
        self.stack.last().map(|s| s.name())
    }

    fn apply(&mut self, transition: Transition<S>, ctx: &mut EngineContext) {
        match transition {
            Transition::None => {}
            Transition::Push(mut scene) => {
                if let Some(top) = self.stack.last_mut() {
                    top.on_pause(&mut self.shared, ctx);
                }
                log::info!("scene push: {}", scene.name());
                scene.on_enter(&mut self.shared, ctx);
                self.stack.push(scene);
            }
            Transition::Pop => {
                if let Some(mut top) = self.stack.pop() {
                    log::info!("scene pop: {}", top.name());
                    top.on_exit(&mut self.shared, ctx);
                }
                match self.stack.last_mut() {
                    Some(top) => top.on_resume(&mut self.shared, ctx),
                    None => ctx.request_exit(),
                }
            }
            Transition::Replace(mut scene) => {
                if let Some(mut top) = self.stack.pop() {
                    top.on_exit(&mut self.shared, ctx);
                }
                log::info!("scene replace: {}", scene.name());
                scene.on_enter(&mut self.shared, ctx);
                self.stack.push(scene);
            }
            Transition::Reset(mut scene) => {
                while let Some(mut top) = self.stack.pop() {
                    top.on_exit(&mut self.shared, ctx);
                }
                log::info!("scene reset: {}", scene.name());
                scene.on_enter(&mut self.shared, ctx);
                self.stack.push(scene);
            }
            Transition::Quit => ctx.request_exit(),
        }
    }
}

impl<S: 'static> Game for SceneStack<S> {
    fn init(&mut self, ctx: &mut EngineContext) -> anyhow::Result<()> {
        if let Some(init) = self.shared_init.take() {
            init(&mut self.shared, ctx)?;
        }
        if let Some(initial) = self.initial.take() {
            let scene = initial(&mut self.shared, ctx)?;
            self.apply(Transition::Push(scene), ctx);
        }
        Ok(())
    }

    fn fixed_update(&mut self, ctx: &mut EngineContext) {
        if let Some(top) = self.stack.last_mut() {
            top.fixed_update(&mut self.shared, ctx);
        }
    }

    fn update(&mut self, ctx: &mut EngineContext) {
        let t = match self.stack.last_mut() {
            Some(top) => top.update(&mut self.shared, ctx),
            None => Transition::None,
        };
        self.apply(t, ctx);
    }

    fn ui(&mut self, ctx: &mut EngineContext, ui: &mut egui::Ui) {
        let t = match self.stack.last_mut() {
            Some(top) => top.ui(&mut self.shared, ctx, ui),
            None => Transition::None,
        };
        self.apply(t, ctx);
    }

    fn render(&mut self, ctx: &mut EngineContext) -> RenderList {
        for scene in self.stack.iter_mut().rev() {
            if let Some(list) = scene.render(&mut self.shared, ctx) {
                return list;
            }
        }
        let size = ctx.window_size();
        let camera = Camera2D::new(size, ctx.scale_factor());
        ctx.renderer.begin_list(camera)
    }

    fn on_close_requested(&mut self, ctx: &mut EngineContext) -> bool {
        match self.stack.last_mut() {
            Some(top) => top.on_close_requested(&mut self.shared, ctx),
            None => true,
        }
    }

    fn on_exit(&mut self, ctx: &mut EngineContext) {
        for scene in self.stack.iter_mut().rev() {
            scene.on_app_exit(&mut self.shared, ctx);
        }
        if let Some(hook) = self.exit_hook.as_mut() {
            hook(&mut self.shared, ctx);
        }
    }
}
