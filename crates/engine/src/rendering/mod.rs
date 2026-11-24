//! GPU rendering: device setup, camera, isometric projection, sprites, particles, textures and the
//! frame renderer.

pub mod camera;
mod capture;
pub mod color;
mod gpu;
pub mod iso;
pub mod particles;
mod renderer;
pub mod sprite;
pub mod texture;

pub use gpu::GpuInfo;
pub use renderer::{EguiPaint, RenderError, RenderStats, Renderer};
