//! Sprite instances, per-frame render lists and static (cached) sprite batches.

use bytemuck::{Pod, Zeroable};
use glam::Vec2;

use super::camera::Camera2D;
use super::color::Color;
use super::iso::SpriteDepth;
use super::texture::TextureId;
use crate::math::Rect;

/// Sprite flag: sways in the wind. Upright sprites bend their top vertices (the bottom stays
/// fixed); together with [`FLAG_GROUND`] (ground cover such as crops and orchards) the vertices
/// stay put and only the pixels above the tile diamond are sheared by their height, so the
/// ground decal never moves while the plants on it bend.
pub const FLAG_WIND: u32 = 1 << 0;
/// Sprite flag: animated water shimmer.
pub const FLAG_WATER: u32 = 1 << 1;
/// Sprite flag: ignores ambient light (lit windows, fire, UI-like markers).
pub const FLAG_EMISSIVE: u32 = 1 << 2;
/// Sprite flag: mirror the texture horizontally.
pub const FLAG_FLIP_X: u32 = 1 << 3;
/// Sprite flag: a one-tile ground sprite anchored at the tile centre. The tile diamond around
/// the anchor receives the weather of [`Environment`] (snow cover, wetness).
pub const FLAG_GROUND: u32 = 1 << 4;
/// Sprite flag: combine the flat and the tent depth with `max` instead of `min` (ground cover
/// with standing parts, see [`SpriteDepth::ground_cover`]).
pub const FLAG_DEPTH_MAX: u32 = 1 << 5;
/// Sprite flag (with [`FLAG_GROUND`]): trodden ground such as roads – gets wet, but snow only
/// covers it thinly, so it stays visible.
pub const FLAG_TRODDEN: u32 = 1 << 6;
/// Sprite flag: green pixels anywhere on the sprite (lawns, hedges, leaves) receive the snow
/// cover of [`Environment`] – for objects standing on grass plots that have no winter art.
pub const FLAG_SNOW_ON_GREEN: u32 = 1 << 7;
/// Sprite flag: the pixels the sprite marks with alpha [`SNOW_MARK_ALPHA`] (roofs, flat tops)
/// receive the snow cover of [`Environment`]; they are drawn fully opaque. Linear filtering
/// between marked and other opaque pixels gives a partial cover, so the snow line stays soft
/// when zoomed out.
pub const FLAG_SNOW_ON_ROOF: u32 = 1 << 8;
/// Alpha (0–255) with which sprites mark where snow settles for [`FLAG_SNOW_ON_ROOF`]
/// (docs/assets.md).
pub const SNOW_MARK_ALPHA: u8 = 254;
/// Sprite flag: the pixels the sprite marks with alpha [`SHADOW_MARK_ALPHA`] are a painted cast
/// shadow ([`SHADOW_MARK_RGB`] at [`SHADOW_MARK_OPACITY`] over the ground, in sRGB); the shader
/// weakens it to [`Environment::shadow`] and draws the pixels fully opaque.
pub const FLAG_SHADOW_MARK: u32 = 1 << 9;
/// Alpha (0–255) of the pixels of a painted cast shadow for [`FLAG_SHADOW_MARK`].
pub const SHADOW_MARK_ALPHA: u8 = 253;
/// Colour (sRGB) and opacity (0–255) with which cast shadows are painted (docs/assets.md).
pub const SHADOW_MARK_RGB: [u8; 3] = [0x1a, 0x12, 0x08];
pub const SHADOW_MARK_OPACITY: u8 = 72;

/// One sprite as uploaded to the GPU (instance vertex data).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct SpriteInstance {
    /// World position of the sprite's anchor point.
    pub position: [f32; 2],
    /// Size in world pixels.
    pub size: [f32; 2],
    /// Anchor within the sprite, normalised (`(0.5, 1.0)` = bottom centre).
    pub anchor: [f32; 2],
    /// Texture coordinates `[u0, v0, u1, v1]`.
    pub uv_rect: [f32; 4],
    /// Linear RGBA tint (multiplied with the texture).
    pub color: [f32; 4],
    /// Tent depth function `[d_left, d_center, d_right, split_u]`.
    pub depth_tent: [f32; 4],
    /// Flat depth function `[d_anchor, slope_per_world_px]`.
    pub depth_flat: [f32; 2],
    /// Bit flags (`FLAG_*`).
    pub flags: u32,
    /// Per-sprite animation phase (e.g. wind offset).
    pub phase: f32,
}

impl SpriteInstance {
    /// Creates a sprite with a white tint and no flags.
    pub fn new(position: Vec2, size: Vec2, anchor: Vec2, uv_rect: [f32; 4], depth: SpriteDepth) -> Self {
        Self {
            position: position.to_array(),
            size: size.to_array(),
            anchor: anchor.to_array(),
            uv_rect,
            color: Color::WHITE.to_array(),
            depth_tent: depth.tent,
            depth_flat: depth.flat,
            flags: 0,
            phase: 0.0,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color.to_array();
        self
    }

    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags |= flags;
        self
    }

    pub fn with_phase(mut self, phase: f32) -> Self {
        self.phase = phase;
        self
    }

    pub fn depth(&self) -> SpriteDepth {
        SpriteDepth { flat: self.depth_flat, tent: self.depth_tent }
    }

    /// World-space bounding box (ignoring wind sway, which stays within a few pixels).
    pub fn world_bounds(&self) -> Rect {
        let pos = Vec2::from(self.position);
        let size = Vec2::from(self.size);
        let min = pos - Vec2::from(self.anchor) * size;
        Rect::new(min, min + size)
    }

    pub(crate) const ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
        0 => Float32x2, // position
        1 => Float32x2, // size
        2 => Float32x2, // anchor
        3 => Float32x4, // uv_rect
        4 => Float32x4, // color
        5 => Float32x4, // depth_tent
        6 => Float32x2, // depth_flat
        7 => Uint32,    // flags
        8 => Float32,   // phase
    ];

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Handle to a GPU-resident batch of sprites that rarely changes (e.g. a terrain chunk).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StaticBatchHandle {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

/// Global weather look of the world, read by the sprite shader. The default is neutral.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    /// Sway amplitude of [`FLAG_WIND`] sprites (1 = light breeze).
    pub wind: f32,
    /// Steady bend of the top of [`FLAG_WIND`] sprites in pixels (scaled like the sway); the
    /// sign gives the direction on screen (negative = to the left).
    pub wind_lean: f32,
    /// Screen direction the wind blows towards, −1 (left) … 1 (right). Gusts push
    /// [`FLAG_WIND`] sprites that way (0: they sway evenly to both sides) and travel across the
    /// map in that direction (to the left for 0).
    pub wind_dir: f32,
    /// Time base of the sway in seconds; `None` uses the renderer clock. A caller that wants
    /// faster sway advances its own clock faster (changing a multiplier instead would make the
    /// phase jump).
    pub wind_time: Option<f32>,
    /// Snow cover 0..1 on the tile diamond of [`FLAG_GROUND`] sprites (patchy below 1).
    pub snow: f32,
    /// Wetness 0..1: darkens the tile diamond of [`FLAG_GROUND`] sprites.
    pub wetness: f32,
    /// Strength 0..1 of the painted cast shadows of [`FLAG_SHADOW_MARK`] sprites (1 = as
    /// painted, 0 = none).
    pub shadow: f32,
}

impl Default for Environment {
    fn default() -> Self {
        Self { wind: 1.0, wind_lean: 0.0, wind_dir: 0.0, wind_time: None, snow: 0.0, wetness: 0.0, shadow: 1.0 }
    }
}

/// A sprite queued for this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DrawItem {
    pub texture: TextureId,
    pub instance: SpriteInstance,
}

/// Everything the game wants drawn in one frame.
///
/// Opaque sprites use alpha-to-coverage / alpha testing and write depth, so their submission
/// order does not matter. Transparent sprites are blended and sorted back-to-front.
pub struct RenderList {
    pub camera: Camera2D,
    pub clear_color: Color,
    /// Global light tint (day/night, weather). RGB multiplies lit sprites; alpha unused.
    pub ambient: Color,
    /// Wind, snow cover and wetness.
    pub environment: Environment,
    /// Seconds of the shader clock (water shimmer, wind without its own time base); `None`:
    /// real time since the renderer started. The engine sets it in fixed-frame mode.
    pub shader_time: Option<f32>,
    pub(crate) statics: Vec<StaticBatchHandle>,
    pub(crate) opaque: Vec<DrawItem>,
    pub(crate) transparent: Vec<DrawItem>,
    cull_rect: Rect,
    culled: usize,
}

/// The buffers of a [`RenderList`], kept by a [`RenderListPool`] after the list was drawn.
#[derive(Default)]
struct SpareBuffers {
    statics: Vec<StaticBatchHandle>,
    opaque: Vec<DrawItem>,
    transparent: Vec<DrawItem>,
}

/// Buffers of drawn [`RenderList`]s for the next frame's lists, so a list per frame allocates
/// nothing once its buffers have grown. The renderer owns one: lists come from
/// [`Renderer::begin_list`](super::Renderer::begin_list) and go back with
/// [`Renderer::recycle_list`](super::Renderer::recycle_list) after they were drawn. A list
/// made with [`RenderList::new`] starts with empty buffers and can be recycled the same way.
#[derive(Default)]
pub struct RenderListPool {
    spare: Vec<SpareBuffers>,
}

impl RenderListPool {
    /// Buffers kept at most (one or two lists exist per frame).
    const MAX_SPARE: usize = 2;

    /// An empty list for `camera` with the buffers of a recycled list.
    pub fn list(&mut self, camera: Camera2D) -> RenderList {
        RenderList::with_buffers(camera, self.spare.pop().unwrap_or_default())
    }

    /// Takes the buffers of a drawn list back for a later [`list`](Self::list).
    pub fn recycle(&mut self, mut list: RenderList) {
        if self.spare.len() >= Self::MAX_SPARE {
            return;
        }
        let mut spare = SpareBuffers {
            statics: std::mem::take(&mut list.statics),
            opaque: std::mem::take(&mut list.opaque),
            transparent: std::mem::take(&mut list.transparent),
        };
        spare.statics.clear();
        spare.opaque.clear();
        spare.transparent.clear();
        self.spare.push(spare);
    }
}

impl RenderList {
    /// An empty list for `camera` with new buffers (a renderer's pool reuses buffers:
    /// [`Renderer::begin_list`](super::Renderer::begin_list)).
    pub fn new(camera: Camera2D) -> Self {
        Self::with_buffers(camera, SpareBuffers::default())
    }

    fn with_buffers(camera: Camera2D, spare: SpareBuffers) -> Self {
        let cull_rect = camera.view_rect().expanded(64.0);
        Self {
            camera,
            clear_color: Color::rgb(0.02, 0.025, 0.03),
            ambient: Color::WHITE,
            environment: Environment::default(),
            shader_time: None,
            statics: spare.statics,
            opaque: spare.opaque,
            transparent: spare.transparent,
            cull_rect,
            culled: 0,
        }
    }

    /// Visible world area (slightly expanded) — use it to skip work for off-screen objects.
    pub fn visible_rect(&self) -> Rect {
        self.cull_rect
    }

    /// Queues a static batch (culled against the view by the renderer).
    pub fn draw_static(&mut self, handle: StaticBatchHandle) {
        self.statics.push(handle);
    }

    /// Queues an opaque (alpha-tested) sprite. Off-screen sprites are dropped immediately.
    pub fn draw(&mut self, texture: TextureId, instance: SpriteInstance) {
        if instance.world_bounds().intersects(&self.cull_rect) {
            self.opaque.push(DrawItem { texture, instance });
        } else {
            self.culled += 1;
        }
    }

    /// Queues an alpha-blended sprite (smoke, shadows, previews, water overlays).
    pub fn draw_transparent(&mut self, texture: TextureId, instance: SpriteInstance) {
        if instance.world_bounds().intersects(&self.cull_rect) {
            self.transparent.push(DrawItem { texture, instance });
        } else {
            self.culled += 1;
        }
    }

    pub fn sprite_count(&self) -> usize {
        self.opaque.len() + self.transparent.len()
    }

    pub fn culled_count(&self) -> usize {
        self.culled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_layout_is_tightly_packed() {
        // 22 floats/u32s = 88 bytes; attribute offsets must match the struct layout.
        assert_eq!(std::mem::size_of::<SpriteInstance>(), 88);
        let attrs = &SpriteInstance::ATTRIBUTES;
        assert_eq!(attrs[0].offset, 0);
        assert_eq!(attrs[3].offset, 24);
        assert_eq!(attrs[6].offset, 72);
        assert_eq!(attrs[7].offset, 80);
        assert_eq!(attrs[8].offset, 84);
    }

    #[test]
    fn culls_offscreen_sprites() {
        let cam = Camera2D::new(Vec2::new(800.0, 600.0), 1.0);
        let mut list = RenderList::new(cam);
        let depth = SpriteDepth::OVERLAY;
        list.draw(TextureId(0), SpriteInstance::new(Vec2::ZERO, Vec2::splat(10.0), Vec2::ZERO, [0.0; 4], depth));
        list.draw(
            TextureId(0),
            SpriteInstance::new(Vec2::splat(10_000.0), Vec2::splat(10.0), Vec2::ZERO, [0.0; 4], depth),
        );
        assert_eq!(list.sprite_count(), 1);
        assert_eq!(list.culled_count(), 1);
    }

    #[test]
    fn recycled_lists_lend_their_buffers_to_the_next_one() {
        let cam = Camera2D::new(Vec2::new(800.0, 600.0), 1.0);
        let mut pool = RenderListPool::default();
        let mut list = pool.list(cam.clone());
        for i in 0..500 {
            list.draw(TextureId(0), SpriteInstance::new(Vec2::splat(i as f32 * 0.1), Vec2::ONE, Vec2::ZERO, [0.0; 4], SpriteDepth::OVERLAY));
        }
        let capacity = list.opaque.capacity();
        pool.recycle(list);
        let next = pool.list(cam.clone());
        assert_eq!(next.sprite_count(), 0, "a new list starts empty");
        assert_eq!(next.culled_count(), 0);
        assert!(next.opaque.capacity() >= capacity.min(500), "the buffer was reused");
        // Without the pool nothing is shared: a list of its own starts with empty buffers.
        assert_eq!(RenderList::new(cam.clone()).opaque.capacity(), 0);
        // The pool keeps at most two sets of buffers.
        for _ in 0..4 {
            pool.recycle(RenderList::new(cam.clone()));
        }
        assert_eq!(pool.spare.len(), RenderListPool::MAX_SPARE);
    }
}
