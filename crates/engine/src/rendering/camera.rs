//! Orthographic 2D camera with zoom, panning and world bounds.

use glam::{Mat4, Vec2, Vec4};

use crate::math::Rect;

/// Orthographic camera looking at the world plane.
///
/// `zoom` is the number of *logical* screen pixels per world pixel; the OS scale factor is
/// applied on top so the world looks equally large on Retina and non-Retina displays.
#[derive(Clone, Debug)]
pub struct Camera2D {
    /// World position at the centre of the viewport.
    pub position: Vec2,
    pub zoom: f32,
    pub min_zoom: f32,
    pub max_zoom: f32,
    /// World area the view centre is restricted to.
    pub bounds: Option<Rect>,
    viewport: Vec2,
    scale_factor: f32,
}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            zoom: 1.0,
            min_zoom: 0.25,
            max_zoom: 4.0,
            bounds: None,
            viewport: Vec2::new(1280.0, 720.0),
            scale_factor: 1.0,
        }
    }
}

impl Camera2D {
    pub fn new(viewport_px: Vec2, scale_factor: f32) -> Self {
        Self { viewport: viewport_px, scale_factor, ..Default::default() }
    }

    /// Updates the viewport size in physical pixels and the OS scale factor.
    pub fn set_viewport(&mut self, viewport_px: Vec2, scale_factor: f32) {
        self.viewport = viewport_px.max(Vec2::ONE);
        self.scale_factor = scale_factor.max(0.1);
    }

    pub fn viewport(&self) -> Vec2 {
        self.viewport
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Physical pixels per world pixel.
    pub fn pixels_per_world_unit(&self) -> f32 {
        self.zoom * self.scale_factor
    }

    /// Visible world rectangle.
    pub fn view_rect(&self) -> Rect {
        let half = self.viewport / (2.0 * self.pixels_per_world_unit());
        Rect::new(self.position - half, self.position + half)
    }

    /// View-projection matrix (world → clip space). The camera position is snapped to the
    /// physical pixel grid to avoid shimmering of pixel art while panning.
    pub fn view_projection(&self) -> Mat4 {
        let ppu = self.pixels_per_world_unit();
        let snapped = (self.position * ppu).round() / ppu;
        let half = self.viewport / (2.0 * ppu);
        // Offset by half a pixel when the viewport has odd dimensions so texels align.
        let odd = Vec2::new(self.viewport.x % 2.0, self.viewport.y % 2.0) * (0.5 / ppu);
        let c = snapped + odd;
        // World y grows downwards: the top of the screen is `c.y - half.y`. The z output is a
        // constant inside the clip volume; real depth comes from the fragment shader.
        let (l, r) = (c.x - half.x, c.x + half.x);
        let (t, b) = (c.y - half.y, c.y + half.y);
        Mat4::from_cols(
            Vec4::new(2.0 / (r - l), 0.0, 0.0, 0.0),
            Vec4::new(0.0, -2.0 / (b - t), 0.0, 0.0),
            Vec4::ZERO,
            Vec4::new(-(r + l) / (r - l), (b + t) / (b - t), 0.5, 1.0),
        )
    }

    /// Converts a screen position (physical pixels, origin top-left) to world coordinates.
    pub fn screen_to_world(&self, screen: Vec2) -> Vec2 {
        self.position + (screen - self.viewport * 0.5) / self.pixels_per_world_unit()
    }

    /// Converts a world position to screen coordinates (physical pixels).
    pub fn world_to_screen(&self, world: Vec2) -> Vec2 {
        (world - self.position) * self.pixels_per_world_unit() + self.viewport * 0.5
    }

    /// Moves the camera by a screen-space delta (physical pixels), e.g. from mouse dragging.
    pub fn pan_screen(&mut self, delta_px: Vec2) {
        self.position -= delta_px / self.pixels_per_world_unit();
        self.clamp_to_bounds();
    }

    /// Moves the camera by a world-space delta.
    pub fn pan_world(&mut self, delta: Vec2) {
        self.position += delta;
        self.clamp_to_bounds();
    }

    /// Sets the zoom level, keeping the world point under `anchor_screen` fixed on screen.
    pub fn zoom_to(&mut self, new_zoom: f32, anchor_screen: Option<Vec2>) {
        let new_zoom = new_zoom.clamp(self.min_zoom, self.max_zoom);
        if let Some(anchor) = anchor_screen {
            let before = self.screen_to_world(anchor);
            self.zoom = new_zoom;
            let after = self.screen_to_world(anchor);
            self.position += before - after;
        } else {
            self.zoom = new_zoom;
        }
        self.clamp_to_bounds();
    }

    /// Multiplies the zoom by `factor` around `anchor_screen`.
    pub fn zoom_by(&mut self, factor: f32, anchor_screen: Option<Vec2>) {
        self.zoom_to(self.zoom * factor, anchor_screen);
    }

    pub fn look_at(&mut self, world: Vec2) {
        self.position = world;
        self.clamp_to_bounds();
    }

    /// Restricts the view centre to [`Camera2D::bounds`].
    pub fn clamp_to_bounds(&mut self) {
        if let Some(b) = self.bounds {
            self.position = self.position.clamp(b.min, b.max.max(b.min));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_world_roundtrip() {
        let mut cam = Camera2D::new(Vec2::new(1600.0, 900.0), 2.0);
        cam.position = Vec2::new(100.0, -50.0);
        cam.zoom = 1.5;
        let p = Vec2::new(123.0, 456.0);
        let back = cam.world_to_screen(cam.screen_to_world(p));
        assert!((back - p).length() < 1e-3);
        // Viewport centre maps to the camera position.
        assert!((cam.screen_to_world(Vec2::new(800.0, 450.0)) - cam.position).length() < 1e-4);
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut cam = Camera2D::new(Vec2::new(1000.0, 800.0), 1.0);
        let anchor = Vec2::new(200.0, 100.0);
        let before = cam.screen_to_world(anchor);
        cam.zoom_by(2.0, Some(anchor));
        let after = cam.screen_to_world(anchor);
        assert!((before - after).length() < 1e-3);
        assert_eq!(cam.zoom, 2.0);
    }

    #[test]
    fn zoom_and_bounds_are_clamped() {
        let mut cam = Camera2D::new(Vec2::new(800.0, 600.0), 1.0);
        cam.bounds = Some(Rect::new(Vec2::splat(-10.0), Vec2::splat(10.0)));
        cam.zoom_to(100.0, None);
        assert_eq!(cam.zoom, cam.max_zoom);
        cam.pan_world(Vec2::new(1000.0, -1000.0));
        assert_eq!(cam.position, Vec2::new(10.0, -10.0));
    }

    #[test]
    fn projection_maps_view_rect_to_clip() {
        let mut cam = Camera2D::new(Vec2::new(800.0, 600.0), 1.0);
        cam.position = Vec2::new(40.0, 20.0);
        let m = cam.view_projection();
        let r = cam.view_rect();
        let tl = m.project_point3(r.min.extend(0.0));
        let br = m.project_point3(r.max.extend(0.0));
        assert!((tl.x + 1.0).abs() < 1e-3 && (tl.y - 1.0).abs() < 1e-3, "{tl:?}");
        assert!((br.x - 1.0).abs() < 1e-3 && (br.y + 1.0).abs() < 1e-3, "{br:?}");
    }
}
