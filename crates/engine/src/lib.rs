//! # KINGDOMS engine
//!
//! A small, modular 2D / isometric game framework written for KINGDOMS – Rise of an Empire.
//! It is deliberately game-agnostic: nothing in this crate knows about buildings, citizens or
//! economies. The game binary plugs into it through the [`core::app::Game`] trait (or the
//! higher-level [`core::scene::SceneStack`]).
//!
//! Module overview:
//! - [`core`]: application lifecycle, main loop, fixed timestep, events, configuration, paths, logging, scenes.
//! - [`input`]: keyboard / mouse state tracking and action mapping.
//! - [`rendering`]: wgpu renderer, orthographic camera, isometric projection, sprite batching, depth model.
//! - [`assets`]: texture atlases, sprite and animation registry, asset metadata, fallbacks.
//! - [`audio`]: sound channels, data-driven sound manifest, music moods with cross-fades, ambience layers.
//! - [`debug`]: frame metrics and lightweight profiling scopes.

pub mod assets;
pub mod audio;
pub mod core;
pub mod debug;
pub mod input;
pub mod math;
pub mod rendering;

pub use egui;
pub use glam;
pub use winit;

/// Commonly used engine types.
pub mod prelude {
    pub use crate::assets::{AnimationId, AssetManager, SpriteId, SpriteRegion};
    pub use crate::audio::{Audio, AudioChannel, AudioSettings, MusicMood, Placement};
    pub use crate::core::app::{EngineContext, EngineEvent, Game};
    pub use crate::core::config::EngineConfig;
    pub use crate::core::paths::GamePaths;
    pub use crate::core::scene::{Scene, SceneStack, Transition};
    pub use crate::core::time::{GameSpeed, Time};
    pub use crate::input::{ActionMap, Binding, Input, KeyCode, MouseButton};
    pub use crate::rendering::camera::Camera2D;
    pub use crate::rendering::color::Color;
    pub use crate::rendering::iso;
    pub use crate::rendering::sprite::{RenderList, SpriteInstance, StaticBatchHandle};
    pub use crate::rendering::texture::TextureId;
    pub use glam::{IVec2, UVec2, Vec2, Vec3, Vec4};
}
