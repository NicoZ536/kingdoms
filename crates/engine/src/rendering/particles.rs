//! CPU particle system: a pool of short-lived sprites spawned by emitters (smoke, dust, rain,
//! snow, leaves) and drawn through the regular sprite pipeline in the transparent pass.
//!
//! ## Usage
//! Styles ([`ParticleStyle`]: sprites, lifetime, motion, size and opacity over life) are
//! registered once with [`ParticleSystem::add_style`]. Every frame the caller
//! 1. advances the pool with [`ParticleSystem::update`] (frame time, wind, visible world area),
//! 2. describes each active emitter with [`ParticleSystem::emit`] (immediate mode: an emitter
//!    that is not described in a frame simply stops spawning; its existing particles live on),
//! 3. queues the particles with [`ParticleSystem::draw`].
//!
//! ## Spaces and depth
//! * [`ParticleSpace::World`]: positions in world pixels. Particles are depth-tested like
//!   upright objects standing at the emitter's iso ground depth ([`Emitter::depth`]), so smoke
//!   from a chimney disappears behind buildings in front of its house and covers those behind.
//!   Emitters outside the visible area (plus a margin) do not spawn.
//! * [`ParticleSpace::Screen`]: positions in logical screen pixels (origin top-left), drawn on
//!   top of the world at a constant on-screen size regardless of the zoom (weather).
//!
//! ## Limits and determinism
//! The pool has a hard capacity; spawns beyond it are dropped and counted. Every emitter key
//! owns a small random generator seeded from the key, so the same calls produce the same
//! particles (presentation only – nothing here feeds back into game rules).

use std::collections::BTreeMap;
use std::f32::consts::TAU;

use glam::Vec2;

use super::color::Color;
use super::iso::{DEPTH_DISABLED, SpriteDepth};
use super::sprite::{RenderList, SpriteInstance};
use crate::assets::SpriteRegion;
use crate::math::Rect;

/// Longest frame step the simulation of particles accepts (avoids bursts after a stall).
const MAX_STEP: f32 = 0.1;
/// World emitters this far outside the visible area still spawn (their particles drift in).
const VIEW_MARGIN: f32 = 192.0;
/// Frames an emitter key is remembered without being emitted.
const EMITTER_GRACE_FRAMES: u64 = 2;

/// Coordinate space of an emitter and its particles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleSpace {
    /// World pixels, depth-tested at the emitter's ground depth.
    World,
    /// Logical screen pixels, drawn on top of the world with a constant on-screen size.
    Screen,
}

/// How a particle picks its sprite from the style's frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameMode {
    /// The frames are spread over the particle's life (first at birth, last before death).
    OverLife,
    /// The frames loop at `fps`, each particle starting at a random frame.
    Loop { fps: f32 },
    /// Every particle keeps one randomly chosen frame.
    Random,
}

/// Appearance and motion of the particles of one kind.
#[derive(Clone, Debug)]
pub struct ParticleStyle {
    pub space: ParticleSpace,
    /// Sprites (at least one; a style without frames draws nothing).
    pub frames: Vec<SpriteRegion>,
    pub frame_mode: FrameMode,
    /// Lifetime range in seconds (uniform).
    pub lifetime: (f32, f32),
    /// Initial velocity in pixels per second and its random spread (± per axis).
    pub velocity: Vec2,
    pub velocity_spread: Vec2,
    /// Constant acceleration in pixels per second² (gravity, buoyancy).
    pub acceleration: Vec2,
    /// Share of the system's wind added to the velocity (0 = unaffected).
    pub wind_response: f32,
    /// Horizontal sway: amplitude in pixels and frequency in Hz (snowflakes, leaves).
    pub sway: (f32, f32),
    /// Sprite size multiplier at birth and at death, and its random spread per particle (±).
    pub scale: (f32, f32),
    pub scale_spread: f32,
    /// Peak opacity and the shares of the life used to fade in and out.
    pub alpha: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    /// Tint (linear RGB; the alpha is taken from the opacity curve).
    pub color: Color,
    /// Sprite flags (`FLAG_*`).
    pub flags: u32,
}

impl ParticleStyle {
    /// A style with neutral defaults: 1 s life, no motion, constant size and full opacity.
    pub fn new(space: ParticleSpace, frames: Vec<SpriteRegion>) -> Self {
        Self {
            space,
            frames,
            frame_mode: FrameMode::Random,
            lifetime: (1.0, 1.0),
            velocity: Vec2::ZERO,
            velocity_spread: Vec2::ZERO,
            acceleration: Vec2::ZERO,
            wind_response: 0.0,
            sway: (0.0, 0.0),
            scale: (1.0, 1.0),
            scale_spread: 0.0,
            alpha: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            color: Color::WHITE,
            flags: 0,
        }
    }

    /// Opacity at life fraction `t` (0 = birth, 1 = death).
    pub fn alpha_at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let fade_in = if self.fade_in > 0.0 { (t / self.fade_in).min(1.0) } else { 1.0 };
        let fade_out = if self.fade_out > 0.0 { ((1.0 - t) / self.fade_out).min(1.0) } else { 1.0 };
        (self.alpha * fade_in.min(fade_out)).clamp(0.0, 1.0)
    }

    /// Size multiplier at life fraction `t`.
    pub fn scale_at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        self.scale.0 + (self.scale.1 - self.scale.0) * t
    }

    /// Frame index for a particle of age `age` and life fraction `t` whose random frame is
    /// `own` (`None` without frames).
    fn frame_index(&self, age: f32, t: f32, own: usize) -> Option<usize> {
        let n = self.frames.len();
        if n == 0 {
            return None;
        }
        Some(match self.frame_mode {
            FrameMode::OverLife => ((t.clamp(0.0, 1.0) * n as f32) as usize).min(n - 1),
            FrameMode::Loop { fps } => ((age.max(0.0) * fps.max(0.0)) as usize + own) % n,
            FrameMode::Random => own % n,
        })
    }
}

/// Handle of a registered [`ParticleStyle`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StyleId(u16);

/// An emitter as described for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emitter {
    pub style: StyleId,
    /// Centre of the spawn area (world or logical screen pixels, see the style's space).
    pub position: Vec2,
    /// Half extents of the rectangular spawn area.
    pub spread: Vec2,
    /// Particles per second.
    pub rate: f32,
    /// World particles: iso depth they are drawn at (ground depth of the source plus a bias).
    pub depth: f32,
}

/// Statistics for the debug overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParticleStats {
    /// Live particles.
    pub live: usize,
    /// Emitters described in the last frame.
    pub emitters: usize,
    /// Particles spawned in the last frame.
    pub spawned: usize,
    /// Spawns dropped because the pool was full (since creation).
    pub dropped: u64,
}

#[derive(Clone, Copy, Debug)]
struct Particle {
    style: StyleId,
    pos: Vec2,
    vel: Vec2,
    age: f32,
    life: f32,
    scale: f32,
    phase: f32,
    frame: u16,
    depth: f32,
}

#[derive(Clone, Debug)]
struct EmitterState {
    rng: SplitMix64,
    /// Fractional particles carried over to the next frame.
    carry: f32,
    last_frame: u64,
}

/// SplitMix64: tiny, fast, good enough for visual randomness.
#[derive(Clone, Debug)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `[-1, 1)`.
    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

/// Pool of particles with their styles and emitter states.
pub struct ParticleSystem {
    styles: Vec<ParticleStyle>,
    particles: Vec<Particle>,
    emitters: BTreeMap<u64, EmitterState>,
    capacity: usize,
    frame: u64,
    dt: f32,
    view: Rect,
    stats: ParticleStats,
}

impl ParticleSystem {
    /// An empty system holding at most `capacity` particles.
    pub fn new(capacity: usize) -> Self {
        Self {
            styles: Vec::new(),
            particles: Vec::with_capacity(capacity.min(16_384)),
            emitters: BTreeMap::new(),
            capacity,
            frame: 0,
            dt: 0.0,
            view: Rect::EMPTY,
            stats: ParticleStats::default(),
        }
    }

    /// Registers a style.
    pub fn add_style(&mut self, style: ParticleStyle) -> StyleId {
        let id = StyleId(self.styles.len().min(usize::from(u16::MAX)) as u16);
        self.styles.push(style);
        id
    }

    /// A registered style.
    pub fn style(&self, id: StyleId) -> Option<&ParticleStyle> {
        self.styles.get(usize::from(id.0))
    }

    /// Most particles the pool holds.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Live particles.
    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    pub fn stats(&self) -> ParticleStats {
        ParticleStats { live: self.particles.len(), ..self.stats }
    }

    /// Removes every particle and forgets the emitters (styles stay registered).
    pub fn clear(&mut self) {
        self.particles.clear();
        self.emitters.clear();
    }

    /// Starts a frame: moves every particle by `dt` seconds (at most `MAX_STEP`) with `wind`
    /// (pixels per second, scaled by each style's `wind_response`), removes the dead ones and
    /// remembers `view` (the visible world area) for culling world emitters.
    pub fn update(&mut self, dt: f32, wind: Vec2, view: Rect) {
        let dt = if dt.is_finite() { dt.clamp(0.0, MAX_STEP) } else { 0.0 };
        self.frame += 1;
        self.dt = dt;
        self.view = view;
        self.stats.emitters = 0;
        self.stats.spawned = 0;
        let styles = &self.styles;
        self.particles.retain_mut(|p| {
            p.age += dt;
            if p.age >= p.life {
                return false;
            }
            let Some(style) = styles.get(usize::from(p.style.0)) else { return false };
            p.vel += style.acceleration * dt;
            p.pos += (p.vel + wind * style.wind_response) * dt;
            true
        });
        let frame = self.frame;
        self.emitters.retain(|_, e| frame - e.last_frame <= EMITTER_GRACE_FRAMES);
    }

    /// Spawns the particles emitter `key` produces during the current frame (call after
    /// [`ParticleSystem::update`]). Returns how many were spawned.
    pub fn emit(&mut self, key: u64, emitter: &Emitter) -> usize {
        let Some(style) = self.styles.get(usize::from(emitter.style.0)) else { return 0 };
        self.stats.emitters += 1;
        let state = self.emitters.entry(key).or_insert_with(|| EmitterState {
            rng: SplitMix64(key ^ 0x5eed_0f9a_271c_1e00),
            carry: 0.0,
            last_frame: self.frame,
        });
        state.last_frame = self.frame;
        if style.space == ParticleSpace::World {
            let area = Rect::new(emitter.position - emitter.spread, emitter.position + emitter.spread);
            if !area.intersects(&self.view.expanded(VIEW_MARGIN)) {
                state.carry = 0.0;
                return 0;
            }
        }
        let wanted = state.carry + emitter.rate.max(0.0) * self.dt;
        let count = wanted.floor();
        state.carry = wanted - count;
        let count = count as usize;
        let mut spawned = 0;
        for _ in 0..count {
            if self.particles.len() >= self.capacity {
                self.stats.dropped += (count - spawned) as u64;
                break;
            }
            let rng = &mut state.rng;
            let pos = emitter.position + Vec2::new(rng.signed(), rng.signed()) * emitter.spread;
            let vel = style.velocity + Vec2::new(rng.signed(), rng.signed()) * style.velocity_spread;
            let (lo, hi) = style.lifetime;
            let life = (lo + (hi - lo) * rng.unit()).max(0.01);
            let scale = (1.0 + rng.signed() * style.scale_spread).max(0.05);
            let phase = rng.unit() * TAU;
            let frame = (rng.unit() * f32::from(u16::MAX)) as u16;
            // Spread the births over the frame so low frame rates do not spawn in clumps.
            let age = rng.unit() * self.dt;
            let vel_now = vel + style.acceleration * age;
            self.particles.push(Particle {
                style: emitter.style,
                pos: pos + vel * age,
                vel: vel_now,
                age,
                life,
                scale,
                phase,
                frame,
                depth: emitter.depth,
            });
            spawned += 1;
        }
        self.stats.spawned += spawned;
        spawned
    }

    /// Queues every particle in `list` (transparent pass; off-screen ones are culled there).
    pub fn draw(&self, list: &mut RenderList) {
        let camera = list.camera.clone();
        let scale_factor = camera.scale_factor();
        let screen_scale = 1.0 / camera.zoom.max(1e-3);
        for p in &self.particles {
            let Some(style) = self.styles.get(usize::from(p.style.0)) else { continue };
            let t = p.age / p.life;
            let Some(frame) = style.frame_index(p.age, t, usize::from(p.frame)) else { continue };
            let region = &style.frames[frame];
            let alpha = style.alpha_at(t);
            if alpha <= 0.0 {
                continue;
            }
            let (amp, freq) = style.sway;
            let sway = if amp != 0.0 { (p.age * freq * TAU + p.phase).sin() * amp } else { 0.0 };
            let size = region.size * style.scale_at(t) * p.scale;
            let pos = p.pos + Vec2::new(sway, 0.0);
            let (world, size, depth) = match style.space {
                ParticleSpace::World => (pos, size, upright(p.depth)),
                ParticleSpace::Screen => {
                    (camera.screen_to_world(pos * scale_factor), size * screen_scale, SpriteDepth::OVERLAY)
                }
            };
            let color = Color::rgba(style.color.r, style.color.g, style.color.b, alpha);
            let instance = SpriteInstance::new(world, size, region.anchor, region.uv, depth)
                .with_color(color)
                .with_flags(style.flags);
            list.draw_transparent(region.texture, instance);
        }
    }
}

/// Depth of an upright sprite standing at iso depth `d` (disabled for overlay values).
fn upright(d: f32) -> SpriteDepth {
    if d >= DEPTH_DISABLED {
        return SpriteDepth::OVERLAY;
    }
    SpriteDepth { flat: SpriteDepth::OVERLAY.flat, tent: [d, d, d, 0.5] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::camera::Camera2D;
    use crate::rendering::texture::TextureId;

    fn region() -> SpriteRegion {
        SpriteRegion { texture: TextureId(3), uv: [0.0, 0.0, 1.0, 1.0], size: Vec2::new(8.0, 8.0), anchor: Vec2::splat(0.5) }
    }

    fn world_style() -> ParticleStyle {
        let mut s = ParticleStyle::new(ParticleSpace::World, vec![region(), region()]);
        s.lifetime = (1.0, 1.0);
        s
    }

    fn view() -> Rect {
        Rect::new(Vec2::splat(-500.0), Vec2::splat(500.0))
    }

    fn emitter(style: StyleId, rate: f32) -> Emitter {
        Emitter { style, position: Vec2::ZERO, spread: Vec2::splat(4.0), rate, depth: 12.0 }
    }

    #[test]
    fn spawn_rate_accumulates_over_frames() {
        let mut sys = ParticleSystem::new(1000);
        let s = sys.add_style(world_style());
        let mut total = 0;
        for _ in 0..40 {
            sys.update(0.025, Vec2::ZERO, view());
            total += sys.emit(7, &emitter(s, 10.0));
        }
        // 40 × 0.025 s = 1 s at 10 per second.
        assert!((9..=10).contains(&total), "spawned {total}");
    }

    #[test]
    fn particles_die_after_their_lifetime() {
        let mut sys = ParticleSystem::new(100);
        let s = sys.add_style(world_style());
        sys.update(0.1, Vec2::ZERO, view());
        sys.emit(1, &emitter(s, 50.0));
        assert_eq!(sys.len(), 5);
        for _ in 0..9 {
            sys.update(0.1, Vec2::ZERO, view());
        }
        assert!(!sys.is_empty(), "some particles must still be alive before 1 s");
        for _ in 0..2 {
            sys.update(0.1, Vec2::ZERO, view());
        }
        assert!(sys.is_empty());
    }

    #[test]
    fn the_pool_is_capped() {
        let mut sys = ParticleSystem::new(6);
        let s = sys.add_style(world_style());
        sys.update(0.1, Vec2::ZERO, view());
        assert_eq!(sys.emit(1, &emitter(s, 100.0)), 6);
        assert_eq!(sys.len(), 6);
        assert_eq!(sys.stats().dropped, 4);
        sys.update(0.1, Vec2::ZERO, view());
        assert_eq!(sys.emit(2, &emitter(s, 100.0)), 0);
        assert_eq!(sys.stats().dropped, 14);
    }

    #[test]
    fn same_calls_give_same_particles_and_keys_differ() {
        let run = |key: u64| {
            let mut sys = ParticleSystem::new(100);
            let s = sys.add_style(world_style());
            for _ in 0..5 {
                sys.update(0.05, Vec2::new(3.0, 0.0), view());
                sys.emit(key, &emitter(s, 40.0));
            }
            sys.particles.iter().map(|p| (p.pos, p.vel, p.life, p.phase)).collect::<Vec<_>>()
        };
        assert_eq!(run(11), run(11));
        assert_ne!(run(11), run(12));
    }

    #[test]
    fn motion_follows_velocity_acceleration_and_wind() {
        let mut sys = ParticleSystem::new(10);
        let mut style = world_style();
        style.lifetime = (10.0, 10.0);
        style.velocity = Vec2::new(0.0, -10.0);
        style.acceleration = Vec2::new(0.0, 4.0);
        style.wind_response = 0.5;
        let s = sys.add_style(style);
        sys.particles.push(Particle {
            style: s,
            pos: Vec2::ZERO,
            vel: Vec2::new(0.0, -10.0),
            age: 0.0,
            life: 10.0,
            scale: 1.0,
            phase: 0.0,
            frame: 0,
            depth: 0.0,
        });
        sys.update(0.1, Vec2::new(20.0, 0.0), view());
        let p = sys.particles[0];
        // v = -10 + 4 · 0.1 = -9.6; x drifts with half the wind.
        assert!((p.vel.y + 9.6).abs() < 1e-4);
        assert!((p.pos.y + 0.96).abs() < 1e-4);
        assert!((p.pos.x - 1.0).abs() < 1e-4);
    }

    #[test]
    fn world_emitters_outside_the_view_do_not_spawn() {
        let mut sys = ParticleSystem::new(100);
        let s = sys.add_style(world_style());
        sys.update(0.1, Vec2::ZERO, view());
        let far = Emitter { position: Vec2::splat(5_000.0), ..emitter(s, 100.0) };
        assert_eq!(sys.emit(1, &far), 0);
        // Screen-space emitters are never culled against the world view.
        let mut screen = world_style();
        screen.space = ParticleSpace::Screen;
        let s2 = sys.add_style(screen);
        assert!(sys.emit(2, &Emitter { style: s2, ..far }) > 0);
    }

    #[test]
    fn forgotten_emitters_are_dropped() {
        let mut sys = ParticleSystem::new(100);
        let s = sys.add_style(world_style());
        sys.update(0.01, Vec2::ZERO, view());
        sys.emit(5, &emitter(s, 1.0));
        assert_eq!(sys.emitters.len(), 1);
        for _ in 0..=EMITTER_GRACE_FRAMES {
            sys.update(0.01, Vec2::ZERO, view());
        }
        assert!(sys.emitters.is_empty());
        assert_eq!(sys.stats().emitters, 0);
    }

    #[test]
    fn opacity_size_and_frames_over_life() {
        let mut style = world_style();
        style.alpha = 0.8;
        style.fade_in = 0.25;
        style.fade_out = 0.5;
        style.scale = (1.0, 3.0);
        assert!(style.alpha_at(0.0).abs() < 1e-6);
        assert!((style.alpha_at(0.125) - 0.4).abs() < 1e-5);
        assert!((style.alpha_at(0.4) - 0.8).abs() < 1e-5);
        assert!((style.alpha_at(0.75) - 0.4).abs() < 1e-5);
        assert!((style.scale_at(0.5) - 2.0).abs() < 1e-6);
        style.frame_mode = FrameMode::OverLife;
        assert_eq!(style.frame_index(0.0, 0.0, 7), Some(0));
        assert_eq!(style.frame_index(0.0, 0.99, 7), Some(1));
        style.frame_mode = FrameMode::Loop { fps: 10.0 };
        assert_eq!(style.frame_index(0.15, 0.0, 0), Some(1));
        assert_eq!(style.frame_index(0.25, 0.0, 1), Some(1));
        style.frame_mode = FrameMode::Random;
        assert_eq!(style.frame_index(5.0, 0.5, 3), Some(1));
        style.frames.clear();
        assert_eq!(style.frame_index(0.0, 0.0, 0), None);
    }

    #[test]
    fn draw_uses_ground_depth_in_the_world_and_overlays_on_screen() {
        let mut sys = ParticleSystem::new(10);
        let s = sys.add_style(world_style());
        let mut screen = world_style();
        screen.space = ParticleSpace::Screen;
        let s2 = sys.add_style(screen);
        for (style, pos) in [(s, Vec2::new(10.0, 20.0)), (s2, Vec2::new(400.0, 300.0))] {
            sys.particles.push(Particle {
                style,
                pos,
                vel: Vec2::ZERO,
                age: 0.5,
                life: 1.0,
                scale: 1.0,
                phase: 0.0,
                frame: 0,
                depth: 42.0,
            });
        }
        let mut cam = Camera2D::new(Vec2::new(800.0, 600.0), 1.0);
        cam.zoom = 2.0;
        let mut list = RenderList::new(cam.clone());
        sys.draw(&mut list);
        assert_eq!(list.sprite_count(), 2);
        let world = list.transparent.iter().find(|d| d.instance.position == [10.0, 20.0]);
        assert!(world.is_some_and(|d| d.instance.depth_tent == [42.0, 42.0, 42.0, 0.5]));
        // The screen particle sits at the screen centre and keeps its on-screen size.
        let centre = cam.screen_to_world(Vec2::new(400.0, 300.0));
        let screen_item = list.transparent.iter().find(|d| d.instance.position == centre.to_array());
        assert!(screen_item.is_some_and(|d| d.instance.size == [4.0, 4.0] && d.instance.depth() == SpriteDepth::OVERLAY));
    }
}
