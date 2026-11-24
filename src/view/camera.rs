//! Strategy-game camera controller: keyboard / edge / drag panning and cursor-anchored zoom.

use glam::{IVec2, Vec2};
use kingdoms_engine::core::app::EngineContext;
use kingdoms_engine::input::{ActionMap, PIXELS_PER_LINE};
use kingdoms_engine::math::Rect;
use kingdoms_engine::rendering::camera::Camera2D;
use kingdoms_engine::rendering::iso;

use crate::actions::Action;

pub struct CameraController {
    pub camera: Camera2D,
    /// Zoom the camera is easing towards.
    target_zoom: f32,
    zoom_anchor: Option<Vec2>,
    /// Keyboard pan speed in screen pixels per second.
    pub pan_speed: f32,
    pub edge_scroll: bool,
    pub edge_margin: f32,
    /// The mouse wheel zooms (off while a tool uses Shift + wheel for rotation).
    pub wheel_zoom: bool,
    /// Smooth two-finger scrolling (trackpad) pans instead of zooming (settings).
    pub trackpad_pan: bool,
    /// A modal dialog (pause menu, demolish confirmation) owns the input: no panning or
    /// zooming by keys, wheel, edge or drag.
    pub input_blocked: bool,
}

/// Keyboard and edge pan speed in screen pixels per second (camera speed setting 100 %).
pub const PAN_SPEED: f32 = 1100.0;

/// Zoom levels snap to these factors so pixel art stays crisp at common zooms.
const ZOOM_STEP: f32 = 1.18;

impl CameraController {
    pub fn new(ctx: &EngineContext) -> Self {
        let mut camera = Camera2D::new(ctx.window_size(), ctx.scale_factor());
        camera.min_zoom = 0.2;
        camera.max_zoom = 4.0;
        camera.zoom = 1.0;
        Self {
            camera,
            target_zoom: 1.0,
            zoom_anchor: None,
            pan_speed: PAN_SPEED,
            edge_scroll: true,
            edge_margin: 6.0,
            wheel_zoom: true,
            trackpad_pan: false,
            input_blocked: false,
        }
    }

    /// Restricts the view centre to the projected map diamond's bounding box.
    pub fn set_map_bounds(&mut self, map_size: IVec2) {
        let s = map_size.as_vec2();
        let top = iso::tile_to_world(Vec2::ZERO);
        let right = iso::tile_to_world(Vec2::new(s.x, 0.0));
        let bottom = iso::tile_to_world(s);
        let left = iso::tile_to_world(Vec2::new(0.0, s.y));
        self.camera.bounds = Some(Rect::new(Vec2::new(left.x, top.y - 128.0), Vec2::new(right.x, bottom.y)));
        self.camera.clamp_to_bounds();
    }

    /// Sets the zoom immediately (clamped to the allowed range).
    pub fn set_zoom(&mut self, zoom: f32) {
        let z = zoom.clamp(self.camera.min_zoom, self.camera.max_zoom);
        self.target_zoom = z;
        self.camera.zoom_to(z, None);
    }

    pub fn focus_tile(&mut self, tile: IVec2, elevation: f32) {
        self.camera.look_at(iso::tile_center_world(tile, elevation));
    }

    /// Per-frame update from input. Returns true if the player panned the view (keys, screen
    /// edge or drag); zooming does not count.
    pub fn update(&mut self, ctx: &EngineContext, actions: &ActionMap<Action>) -> bool {
        let mut panned = false;
        let dt = ctx.time.delta();
        let input = &ctx.input;
        self.camera.set_viewport(ctx.window_size(), ctx.scale_factor());
        let keys = !input.ui_wants_keyboard() && !self.input_blocked;
        let pointer = !input.ui_wants_pointer() && !self.input_blocked;

        // Keyboard panning (screen-space speed, independent of zoom).
        let mut dir = Vec2::ZERO;
        if keys {
            if actions.down(input, Action::PanUp) {
                dir.y -= 1.0;
            }
            if actions.down(input, Action::PanDown) {
                dir.y += 1.0;
            }
            if actions.down(input, Action::PanLeft) {
                dir.x -= 1.0;
            }
            if actions.down(input, Action::PanRight) {
                dir.x += 1.0;
            }
        }
        // Edge scrolling.
        if self.edge_scroll
            && input.window_focused()
            && pointer
            && let Some(c) = input.cursor_position()
        {
            let vp = self.camera.viewport();
            let m = self.edge_margin * ctx.scale_factor();
            if c.x <= m {
                dir.x -= 1.0;
            } else if c.x >= vp.x - m {
                dir.x += 1.0;
            }
            if c.y <= m {
                dir.y -= 1.0;
            } else if c.y >= vp.y - m {
                dir.y += 1.0;
            }
        }
        if dir != Vec2::ZERO {
            let boost = if input.modifiers().shift { 2.2 } else { 1.0 };
            let px = dir.normalize() * self.pan_speed * boost * dt * ctx.scale_factor();
            self.camera.pan_screen(-px);
            panned = true;
        }

        // Drag panning.
        if !self.input_blocked && actions.down(input, Action::PanDrag) && input.cursor_delta() != Vec2::ZERO {
            self.camera.pan_screen(input.cursor_delta());
            panned = true;
        }

        // Trackpad: two fingers move the map with them (pixels are logical; the camera pans in
        // physical pixels).
        let swipe = input.pixel_scroll();
        let swipe_pans = self.trackpad_pan && swipe != Vec2::ZERO;
        if swipe_pans && self.wheel_zoom && !self.input_blocked {
            self.camera.pan_screen(swipe * ctx.scale_factor());
            panned = true;
        }
        // Zoom: wheel and pinch anchored at the cursor, keys anchored at the screen centre.
        let wheel = scroll_zoom_lines(input.scroll(), swipe, self.trackpad_pan);
        if wheel != 0.0 && self.wheel_zoom && !self.input_blocked {
            self.target_zoom = (self.target_zoom * ZOOM_STEP.powf(wheel)).clamp(self.camera.min_zoom, self.camera.max_zoom);
            self.zoom_anchor = input.cursor_position();
        }
        let pinch = input.pinch();
        if pinch != 0.0 && !self.input_blocked {
            self.target_zoom = pinch_zoom(self.target_zoom, pinch).clamp(self.camera.min_zoom, self.camera.max_zoom);
            self.zoom_anchor = input.cursor_position();
        }
        if keys {
            if actions.pressed(input, Action::ZoomIn) {
                self.target_zoom = (self.target_zoom * ZOOM_STEP).min(self.camera.max_zoom);
                self.zoom_anchor = None;
            }
            if actions.pressed(input, Action::ZoomOut) {
                self.target_zoom = (self.target_zoom / ZOOM_STEP).max(self.camera.min_zoom);
                self.zoom_anchor = None;
            }
        }
        // Ease towards the target zoom (frame-rate independent).
        if (self.camera.zoom - self.target_zoom).abs() > 1e-4 {
            let t = 1.0 - (-dt * 14.0).exp();
            let z = self.camera.zoom + (self.target_zoom - self.camera.zoom) * t;
            let z = if (z - self.target_zoom).abs() < 1e-3 { self.target_zoom } else { z };
            self.camera.zoom_to(z, self.zoom_anchor);
        }
        panned
    }
}

/// Wheel lines of a frame that zoom: all of them, unless smooth scrolling pans the map
/// (`trackpad_pan`) – then only the lines of wheel notches (the smooth part `swipe`, counted in
/// `lines` at [`PIXELS_PER_LINE`], is taken out).
pub fn scroll_zoom_lines(lines: f32, swipe: Vec2, trackpad_pan: bool) -> f32 {
    if trackpad_pan { lines - swipe.y / PIXELS_PER_LINE } else { lines }
}

/// Zoom after a pinch of `delta` (positive = spread the fingers = magnify), at most halving or
/// doubling in one frame.
pub fn pinch_zoom(zoom: f32, delta: f32) -> f32 {
    zoom * (1.0 + delta).clamp(0.5, 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trackpad_swipes_pan_or_zoom_by_the_setting_and_pinches_zoom() {
        // A wheel notch (1 line) plus a swipe of 80 logical pixels (2 lines) in one frame.
        let swipe = Vec2::new(10.0, 80.0);
        assert!((scroll_zoom_lines(3.0, swipe, false) - 3.0).abs() < 1e-6, "without panning everything zooms");
        assert!((scroll_zoom_lines(3.0, swipe, true) - 1.0).abs() < 1e-6, "with panning only the notch zooms");
        assert_eq!(scroll_zoom_lines(0.0, Vec2::ZERO, true), 0.0);
        assert!((pinch_zoom(1.0, 0.1) - 1.1).abs() < 1e-6);
        assert!((pinch_zoom(2.0, -0.25) - 1.5).abs() < 1e-6);
        assert!((pinch_zoom(1.0, 5.0) - 2.0).abs() < 1e-6 && (pinch_zoom(1.0, -5.0) - 0.5).abs() < 1e-6);
    }
}
