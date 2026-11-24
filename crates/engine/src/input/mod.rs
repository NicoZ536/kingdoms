//! Input handling: per-frame keyboard and mouse state plus configurable action bindings.
//!
//! Raw winit events are fed in by the application loop. When the UI (egui) wants the pointer or
//! keyboard, presses are not forwarded to the game, but releases always are, so keys can never
//! get "stuck" when focus moves between UI and world.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use glam::Vec2;
use serde::{Deserialize, Serialize};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::keyboard::{ModifiersState, PhysicalKey};

pub use winit::event::MouseButton;
pub use winit::keyboard::KeyCode;

/// Pixels of mouse movement after which a held button counts as a drag.
pub const DRAG_THRESHOLD_PX: f32 = 4.0;
/// Logical pixels of smooth scrolling that count as one wheel line ([`Input::scroll`]).
pub const PIXELS_PER_LINE: f32 = 40.0;
/// On macOS shortcuts are Command combinations: while Command is held, a key binding without
/// Command (`W`, `S`, …) does not fire, so `Cmd+S` saves without also panning down. Elsewhere a
/// key without modifiers fires with any modifier held, as before.
pub const COMMAND_BLOCKS_PLAIN_KEYS: bool = cfg!(target_os = "macos");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Command on macOS, Windows key elsewhere.
    pub logo: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers { shift: false, ctrl: false, alt: false, logo: false };

    fn from_winit(state: ModifiersState) -> Self {
        Self {
            shift: state.shift_key(),
            ctrl: state.control_key(),
            alt: state.alt_key(),
            logo: state.super_key(),
        }
    }

    /// "Primary" shortcut modifier: Cmd on macOS, Ctrl elsewhere.
    pub fn command(&self) -> bool {
        if cfg!(target_os = "macos") { self.logo } else { self.ctrl }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DragState {
    origin: Vec2,
    dragging: bool,
}

/// Snapshot of input for the current frame.
#[derive(Debug, Default)]
pub struct Input {
    keys_down: HashSet<KeyCode>,
    keys_pressed: HashSet<KeyCode>,
    keys_released: HashSet<KeyCode>,
    /// Keys pressed this frame in arrival order, including those the UI consumed.
    raw_pressed: Vec<KeyCode>,
    buttons_down: HashSet<MouseButton>,
    buttons_pressed: HashSet<MouseButton>,
    buttons_released: HashSet<MouseButton>,
    drags: HashMap<MouseButton, DragState>,
    cursor: Option<Vec2>,
    cursor_delta: Vec2,
    scroll: f32,
    /// Smooth (pixel) scrolling of this frame in logical pixels: trackpads and smooth-scrolling
    /// mice (`MouseScrollDelta::PixelDelta`).
    pixel_scroll: Vec2,
    /// Sum of this frame's pinch gestures (trackpad zoom; positive = magnify).
    pinch: f32,
    /// OS scale factor (physical pixels per logical pixel) for pixel deltas.
    scale_factor: f32,
    modifiers: Modifiers,
    focused: bool,
    ui_wants_pointer: bool,
    ui_wants_keyboard: bool,
}

impl Input {
    pub fn new() -> Self {
        Self { focused: true, scale_factor: 1.0, ..Default::default() }
    }

    /// Sets the OS scale factor (the app loop does at start; scale changes arrive as events).
    pub(crate) fn set_scale_factor(&mut self, scale: f64) {
        if scale.is_finite() && scale > 0.0 {
            self.scale_factor = scale as f32;
        }
    }

    /// Updates which input the UI currently claims (set by the app loop from egui each frame).
    pub(crate) fn set_ui_capture(&mut self, pointer: bool, keyboard: bool) {
        self.ui_wants_pointer = pointer;
        self.ui_wants_keyboard = keyboard;
    }

    /// Processes one window event. `ui_consumed` is true if egui used the event.
    pub(crate) fn handle_window_event(&mut self, event: &WindowEvent, ui_consumed: bool) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                match event.state {
                    ElementState::Pressed => {
                        if !event.repeat && !self.raw_pressed.contains(&code) {
                            self.raw_pressed.push(code);
                        }
                        if ui_consumed || self.ui_wants_keyboard {
                            return;
                        }
                        if !event.repeat && self.keys_down.insert(code) {
                            self.keys_pressed.insert(code);
                        }
                    }
                    ElementState::Released => {
                        if self.keys_down.remove(&code) {
                            self.keys_released.insert(code);
                        }
                    }
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = Modifiers::from_winit(mods.state());
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => {
                    if ui_consumed || self.ui_wants_pointer {
                        return;
                    }
                    if self.buttons_down.insert(*button) {
                        self.buttons_pressed.insert(*button);
                        let origin = self.cursor.unwrap_or_default();
                        self.drags.insert(*button, DragState { origin, dragging: false });
                    }
                }
                ElementState::Released => {
                    if self.buttons_down.remove(button) {
                        self.buttons_released.insert(*button);
                    }
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                let pos = Vec2::new(position.x as f32, position.y as f32);
                if let Some(prev) = self.cursor {
                    self.cursor_delta += pos - prev;
                }
                self.cursor = Some(pos);
                for drag in self.drags.values_mut() {
                    if !drag.dragging && drag.origin.distance(pos) > DRAG_THRESHOLD_PX {
                        drag.dragging = true;
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if ui_consumed || self.ui_wants_pointer {
                    return;
                }
                match delta {
                    MouseScrollDelta::LineDelta(_, y) => self.scroll += *y,
                    MouseScrollDelta::PixelDelta(p) => self.add_pixel_scroll(Vec2::new(p.x as f32, p.y as f32)),
                }
            }
            WindowEvent::PinchGesture { delta, .. } => {
                if ui_consumed || self.ui_wants_pointer {
                    return;
                }
                self.add_pinch(*delta);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.set_scale_factor(*scale_factor),
            WindowEvent::Focused(focused) => {
                self.focused = *focused;
                if !focused {
                    self.release_all();
                }
            }
            _ => {}
        }
    }

    /// Smooth scrolling by `physical` pixels: kept in logical pixels (the same swipe scrolls as
    /// far on a Retina screen) and counted as lines in [`Input::scroll`].
    fn add_pixel_scroll(&mut self, physical: Vec2) {
        if !physical.is_finite() {
            return;
        }
        let logical = physical / self.scale_factor.max(0.1);
        self.pixel_scroll += logical;
        self.scroll += logical.y / PIXELS_PER_LINE;
    }

    fn add_pinch(&mut self, delta: f64) {
        if delta.is_finite() {
            self.pinch += delta as f32;
        }
    }

    fn release_all(&mut self) {
        self.keys_released.extend(self.keys_down.drain());
        self.buttons_released.extend(self.buttons_down.drain());
        self.drags.clear();
    }

    /// Clears per-frame state. Called by the app loop after the frame has been processed.
    pub(crate) fn end_frame(&mut self) {
        self.keys_pressed.clear();
        self.raw_pressed.clear();
        self.keys_released.clear();
        self.buttons_pressed.clear();
        for button in self.buttons_released.drain() {
            self.drags.remove(&button);
        }
        self.cursor_delta = Vec2::ZERO;
        self.scroll = 0.0;
        self.pixel_scroll = Vec2::ZERO;
        self.pinch = 0.0;
    }

    pub fn key_down(&self, key: KeyCode) -> bool {
        self.keys_down.contains(&key)
    }

    /// True only in the frame the key went down.
    pub fn key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    /// Keys pressed this frame (no auto-repeat) in the order they arrived, including keys the
    /// UI consumed: for "press a key" prompts such as key binding settings.
    pub fn raw_keys_pressed(&self) -> &[KeyCode] {
        &self.raw_pressed
    }

    pub fn key_released(&self, key: KeyCode) -> bool {
        self.keys_released.contains(&key)
    }

    pub fn mouse_down(&self, button: MouseButton) -> bool {
        self.buttons_down.contains(&button)
    }

    pub fn mouse_pressed(&self, button: MouseButton) -> bool {
        self.buttons_pressed.contains(&button)
    }

    pub fn mouse_released(&self, button: MouseButton) -> bool {
        self.buttons_released.contains(&button)
    }

    /// True while `button` is held and the cursor moved beyond [`DRAG_THRESHOLD_PX`].
    pub fn is_dragging(&self, button: MouseButton) -> bool {
        self.drags.get(&button).is_some_and(|d| d.dragging)
    }

    /// Cursor position where the drag of `button` started (physical pixels).
    pub fn drag_origin(&self, button: MouseButton) -> Option<Vec2> {
        self.drags.get(&button).map(|d| d.origin)
    }

    /// A click is a press+release without exceeding the drag threshold.
    pub fn clicked(&self, button: MouseButton) -> bool {
        self.mouse_released(button) && !self.drags.get(&button).is_some_and(|d| d.dragging)
    }

    /// Cursor position in physical pixels, `None` if outside the window.
    pub fn cursor_position(&self) -> Option<Vec2> {
        self.cursor
    }

    /// Cursor movement this frame in physical pixels.
    pub fn cursor_delta(&self) -> Vec2 {
        self.cursor_delta
    }

    /// Scroll wheel movement this frame in "lines" (positive = away from the user).
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    /// Smooth scrolling of this frame in logical pixels (trackpad two-finger swipe, smooth
    /// mice; zero for wheel notches). Also counted in [`Input::scroll`] as lines.
    pub fn pixel_scroll(&self) -> Vec2 {
        self.pixel_scroll
    }

    /// Pinch-to-zoom of this frame (trackpad; positive = magnify, e.g. `0.1` = 10 % larger).
    pub fn pinch(&self) -> f32 {
        self.pinch
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn window_focused(&self) -> bool {
        self.focused
    }

    /// True if the pointer is currently over UI (the world should ignore hover/clicks).
    pub fn ui_wants_pointer(&self) -> bool {
        self.ui_wants_pointer
    }

    pub fn ui_wants_keyboard(&self) -> bool {
        self.ui_wants_keyboard
    }
}

/// A physical input that can trigger an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Binding {
    Key(KeyCode),
    /// Key that only triggers while the given modifiers are held (others may also be held).
    KeyWith(KeyCode, Modifiers),
    Mouse(MouseButton),
}

impl Binding {
    fn modifiers_ok(required: &Modifiers, actual: &Modifiers) -> bool {
        (!required.shift || actual.shift)
            && (!required.ctrl || actual.ctrl)
            && (!required.alt || actual.alt)
            && (!required.logo || actual.logo)
    }

    /// True if a key binding that requires `required` may fire with `actual` held: a binding
    /// without Command does not while Command is held where [`COMMAND_BLOCKS_PLAIN_KEYS`]
    /// (`command_blocks`, a parameter for tests).
    pub fn plain_key_allowed(required: &Modifiers, actual: &Modifiers, command_blocks: bool) -> bool {
        required.logo || !(command_blocks && actual.logo)
    }

    fn key_modifiers_ok(required: &Modifiers, actual: &Modifiers) -> bool {
        Self::modifiers_ok(required, actual) && Self::plain_key_allowed(required, actual, COMMAND_BLOCKS_PLAIN_KEYS)
    }

    pub fn down(&self, input: &Input) -> bool {
        match self {
            Binding::Key(k) => input.key_down(*k) && Self::key_modifiers_ok(&Modifiers::NONE, &input.modifiers),
            Binding::KeyWith(k, m) => input.key_down(*k) && Self::key_modifiers_ok(m, &input.modifiers),
            Binding::Mouse(b) => input.mouse_down(*b),
        }
    }

    pub fn pressed(&self, input: &Input) -> bool {
        match self {
            Binding::Key(k) => input.key_pressed(*k) && Self::key_modifiers_ok(&Modifiers::NONE, &input.modifiers),
            Binding::KeyWith(k, m) => input.key_pressed(*k) && Self::key_modifiers_ok(m, &input.modifiers),
            Binding::Mouse(b) => input.mouse_pressed(*b),
        }
    }

    /// Human readable label, e.g. for tooltips ("Shift+S").
    pub fn label(&self) -> String {
        fn key_name(k: &KeyCode) -> String {
            let raw = format!("{k:?}");
            raw.strip_prefix("Key")
                .or_else(|| raw.strip_prefix("Digit"))
                .map(str::to_owned)
                .unwrap_or(raw)
        }
        match self {
            Binding::Key(k) => key_name(k),
            Binding::KeyWith(k, m) => {
                let mut parts = Vec::new();
                if m.ctrl {
                    parts.push("Ctrl".to_owned());
                }
                if m.alt {
                    parts.push("Alt".to_owned());
                }
                if m.shift {
                    parts.push("Shift".to_owned());
                }
                if m.logo {
                    parts.push(if cfg!(target_os = "macos") { "Cmd" } else { "Super" }.to_owned());
                }
                parts.push(key_name(k));
                parts.join("+")
            }
            Binding::Mouse(b) => format!("Mouse {b:?}"),
        }
    }
}

/// Maps game-defined actions to one or more bindings.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "A: Serialize + Eq + Hash",
    deserialize = "A: Deserialize<'de> + Eq + Hash"
))]
pub struct ActionMap<A: Eq + Hash> {
    bindings: HashMap<A, Vec<Binding>>,
}

impl<A: Eq + Hash> Default for ActionMap<A> {
    fn default() -> Self {
        Self { bindings: HashMap::new() }
    }
}

impl<A: Copy + Eq + Hash> ActionMap<A> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(&mut self, action: A, binding: Binding) -> &mut Self {
        let list = self.bindings.entry(action).or_default();
        if !list.contains(&binding) {
            list.push(binding);
        }
        self
    }

    pub fn with(mut self, action: A, binding: Binding) -> Self {
        self.bind(action, binding);
        self
    }

    pub fn clear(&mut self, action: A) {
        self.bindings.remove(&action);
    }

    pub fn bindings(&self, action: A) -> &[Binding] {
        self.bindings.get(&action).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn down(&self, input: &Input, action: A) -> bool {
        self.bindings(action).iter().any(|b| b.down(input))
    }

    pub fn pressed(&self, input: &Input, action: A) -> bool {
        self.bindings(action).iter().any(|b| b.pressed(input))
    }

    /// Fills in defaults for actions that have no binding (e.g. after loading an old config).
    pub fn merge_missing(&mut self, defaults: &ActionMap<A>) {
        for (action, list) in &defaults.bindings {
            self.bindings.entry(*action).or_insert_with(|| list.clone());
        }
    }

    /// Label of the first binding for UI hints.
    pub fn hint(&self, action: A) -> Option<String> {
        self.bindings(action).first().map(Binding::label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event(input: &mut Input, code: KeyCode, pressed: bool) {
        if pressed {
            if input.keys_down.insert(code) {
                input.keys_pressed.insert(code);
            }
        } else if input.keys_down.remove(&code) {
            input.keys_released.insert(code);
        }
    }

    #[test]
    fn smooth_scrolling_counts_logical_pixels_and_pinches_add_up() {
        let mut input = Input::new();
        input.set_scale_factor(2.0);
        input.add_pixel_scroll(Vec2::new(30.0, 160.0));
        assert_eq!(input.pixel_scroll(), Vec2::new(15.0, 80.0), "Retina: physical pixels halved");
        assert!((input.scroll() - 2.0).abs() < 1e-6, "80 logical pixels are two lines");
        input.add_pixel_scroll(Vec2::new(f32::NAN, 1.0));
        assert_eq!(input.pixel_scroll(), Vec2::new(15.0, 80.0));
        input.add_pinch(0.1);
        input.add_pinch(-0.04);
        input.add_pinch(f64::INFINITY);
        assert!((input.pinch() - 0.06).abs() < 1e-6);
        input.end_frame();
        assert_eq!((input.pixel_scroll(), input.scroll(), input.pinch()), (Vec2::ZERO, 0.0, 0.0));
        input.set_scale_factor(0.0);
        input.add_pixel_scroll(Vec2::new(0.0, 40.0));
        assert!((input.scroll() - 0.5).abs() < 1e-6, "an invalid scale factor is ignored");
    }

    #[test]
    fn command_blocks_plain_keys_only_where_configured() {
        let cmd = Modifiers { logo: true, ..Modifiers::NONE };
        let none = Modifiers::NONE;
        assert!(Binding::plain_key_allowed(&none, &cmd, false), "Linux and Windows: plain keys fire with any modifier");
        assert!(!Binding::plain_key_allowed(&none, &cmd, true), "macOS: Cmd+S is a shortcut, not S");
        assert!(Binding::plain_key_allowed(&none, &Modifiers { shift: true, ..none }, true));
        assert!(Binding::plain_key_allowed(&cmd, &cmd, true), "a Command binding fires with Command");
        let shift = Modifiers { shift: true, ..none };
        assert!(!Binding::plain_key_allowed(&shift, &Modifiers { logo: true, shift: true, ..none }, true));
        assert_eq!(COMMAND_BLOCKS_PLAIN_KEYS, cfg!(target_os = "macos"));
        let mut input = Input::new();
        key_event(&mut input, KeyCode::KeyS, true);
        input.modifiers = cmd;
        let save = Binding::KeyWith(KeyCode::KeyS, cmd);
        assert!(save.pressed(&input));
        assert_eq!(Binding::Key(KeyCode::KeyS).pressed(&input), !COMMAND_BLOCKS_PLAIN_KEYS);
    }

    #[test]
    fn pressed_is_single_frame() {
        let mut input = Input::new();
        key_event(&mut input, KeyCode::KeyW, true);
        assert!(input.key_pressed(KeyCode::KeyW));
        assert!(input.key_down(KeyCode::KeyW));
        input.end_frame();
        assert!(!input.key_pressed(KeyCode::KeyW));
        assert!(input.key_down(KeyCode::KeyW));
        key_event(&mut input, KeyCode::KeyW, false);
        assert!(input.key_released(KeyCode::KeyW));
        assert!(!input.key_down(KeyCode::KeyW));
    }

    #[test]
    fn focus_loss_releases_everything() {
        let mut input = Input::new();
        key_event(&mut input, KeyCode::KeyA, true);
        input.buttons_down.insert(MouseButton::Left);
        input.release_all();
        assert!(!input.key_down(KeyCode::KeyA));
        assert!(input.key_released(KeyCode::KeyA));
        assert!(!input.mouse_down(MouseButton::Left));
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    enum TestAction {
        Pause,
        Save,
    }

    #[test]
    fn action_map_with_modifiers() {
        let map = ActionMap::new()
            .with(TestAction::Pause, Binding::Key(KeyCode::Space))
            .with(
                TestAction::Save,
                Binding::KeyWith(KeyCode::KeyS, Modifiers { ctrl: true, ..Modifiers::NONE }),
            );
        let mut input = Input::new();
        key_event(&mut input, KeyCode::Space, true);
        key_event(&mut input, KeyCode::KeyS, true);
        assert!(map.pressed(&input, TestAction::Pause));
        assert!(!map.pressed(&input, TestAction::Save));
        input.modifiers.ctrl = true;
        assert!(map.pressed(&input, TestAction::Save));
        assert_eq!(map.hint(TestAction::Save).unwrap(), "Ctrl+S");
    }

    #[test]
    fn action_map_serde_roundtrip() {
        let map = ActionMap::new().with(TestAction::Pause, Binding::Key(KeyCode::Space));
        let text = ron::to_string(&map).unwrap();
        let back: ActionMap<TestAction> = ron::from_str(&text).unwrap();
        assert_eq!(back.bindings(TestAction::Pause), &[Binding::Key(KeyCode::Space)]);
    }
}
