//! Particle and weather sprites (`effects` atlas, smooth sampling): smoke puffs, flames, dust,
//! rain streak, snowflake, sparkle and autumn leaves. Drawn 4× supersampled and filtered down
//! for soft edges.

use std::f32::consts::{PI, TAU};

use kingdoms_engine::rendering::texture::Sampling;

use crate::atlas::AtlasBuilder;
use crate::draw::{Canvas, Px, hash01, mix, rgb, rgba, shade, with_alpha};

const SS: u32 = 4;

/// Smoke frames (growing, fading puff).
pub const SMOKE_FRAMES: u32 = 6;
/// Flame frames.
pub const FIRE_FRAMES: u32 = 4;
/// Dust frames.
pub const DUST_FRAMES: u32 = 3;
/// Falling leaf variants: orange, yellow, rust brown, red.
const LEAF_COLORS: [u32; 4] = [0xd8792a, 0xe2b23a, 0x9a5a2a, 0xb8402a];
pub const LEAF_VARIANTS: u32 = LEAF_COLORS.len() as u32;

/// Directions of the arrow sprites (`arrow_<k>` points `k · 360° / ARROW_DIRS` clockwise from
/// screen right).
pub const ARROW_DIRS: u32 = 16;

/// An arrow in flight, 16 × 16, pointing `k · 22.5°` clockwise from screen right: dark shaft,
/// steel head, pale fletching.
fn arrow(k: u32) -> Canvas {
    let size = 16 * SS;
    let mut c = Canvas::new(size, size);
    let a = TAU * k as f32 / ARROW_DIRS as f32;
    let (dx, dy) = (a.cos(), a.sin());
    let (nx, ny) = (-dy, dx);
    let m = size as f32 * 0.5;
    let s = SS as f32;
    let p = |l: f32, w: f32| (m + dx * l * s + nx * w * s, m + dy * l * s + ny * w * s);
    let (x0, y0) = p(-6.5, 0.0);
    let (x1, y1) = p(4.5, 0.0);
    c.thick_line(x0, y0, x1, y1, 0.55 * s, rgb(0x5a3e24));
    c.fill_polygon(&[p(7.0, 0.0), p(3.8, 1.6), p(3.8, -1.6)], |_, _| rgb(0xc8ccd2));
    for side in [-1.0, 1.0] {
        c.fill_polygon(&[p(-6.5, 0.0), p(-4.0, 0.0), p(-5.4, 1.9 * side), p(-7.4, 1.9 * side)], |_, _| rgb(0xe8e0d0));
    }
    c.downsample(SS)
}

/// A stone thrown by a siege engine, 8 × 8.
fn stone() -> Canvas {
    let size = 8 * SS;
    let mut c = Canvas::new(size, size);
    let m = size as f32 * 0.5;
    c.fill_ellipse(m, m, 3.2 * SS as f32, 2.9 * SS as f32, |x, y, d| {
        let lit = (-(x as f32 - m) - (y as f32 - m)) / (3.0 * SS as f32);
        shade(rgb(0x8a847a), (1.0 + 0.25 * lit) * (1.0 - 0.2 * d))
    });
    c.downsample(SS)
}

/// Soft round puff made of a few overlapping blobs.
fn puff(c: &mut Canvas, cx: f32, cy: f32, r: f32, color: Px, alpha: f32, seed: u32) {
    for k in 0..5 {
        let a = TAU * hash01(k, 1, seed);
        let d = r * 0.45 * hash01(k, 2, seed);
        let rr = r * (0.55 + 0.25 * hash01(k, 3, seed));
        let (bx, by) = (cx + a.cos() * d, cy + a.sin() * d * 0.8);
        c.fill_ellipse(bx, by, rr, rr, |x, y, dd| {
            // Lighter towards the top-left, softly fading edge.
            let lit = 1.0 + 0.12 * (-(x as f32 - bx) / rr - (y as f32 - by) / rr).clamp(-1.0, 1.0);
            let col = mix(color, rgb(0xffffff), (lit - 1.0).max(0.0));
            with_alpha(col, (alpha * (1.0 - dd).powf(0.7) * 255.0).clamp(0.0, 255.0) as u8)
        });
    }
}

fn smoke(frame: u32) -> Canvas {
    let size = 24 * SS;
    let mut c = Canvas::new(size, size);
    let t = frame as f32 / (SMOKE_FRAMES - 1) as f32;
    let r = (5.0 + 5.5 * t) * SS as f32;
    let alpha = 0.75 - 0.6 * t;
    let col = mix(rgb(0x8a857e), rgb(0xd0ccc4), t);
    puff(&mut c, size as f32 * 0.5, size as f32 * 0.5, r, col, alpha, 40 + frame);
    c.downsample(SS)
}

fn fire(frame: u32) -> Canvas {
    let (w, h) = (16 * SS, 24 * SS);
    let mut c = Canvas::new(w, h);
    let layers = [(1.0, rgb(0xc8321e), 0.9), (0.72, rgb(0xf08a2a), 1.0), (0.42, rgb(0xffe070), 1.0)];
    for (li, &(scale, col, alpha)) in layers.iter().enumerate() {
        let base_w = w as f32 * 0.42 * scale;
        let height = h as f32 * (0.92 - 0.08 * li as f32) * (0.9 + 0.1 * ((frame as f32 * 1.7).sin()));
        let phase = frame as f32 * PI * 0.5 + li as f32;
        for y in 0..h as i32 {
            let t = (h as f32 - y as f32) / height; // 0 at the base, 1 at the tip
            if !(0.0..=1.0).contains(&t) {
                continue;
            }
            let half = base_w * (PI * (0.15 + 0.85 * t)).sin().powf(0.8) * (1.0 - t).powf(0.35);
            let sway = (t * 4.0 + phase).sin() * t * w as f32 * 0.12;
            let cx = w as f32 * 0.5 + sway;
            for x in 0..w as i32 {
                let d = (x as f32 + 0.5 - cx).abs();
                if d < half {
                    let edge = 1.0 - d / half.max(1.0);
                    c.blend(x, y, with_alpha(col, (alpha * edge.powf(0.25) * 255.0) as u8));
                }
            }
        }
    }
    c.downsample(SS)
}

fn dust(frame: u32) -> Canvas {
    let (w, h) = (24 * SS, 16 * SS);
    let mut c = Canvas::new(w, h);
    let t = frame as f32 / (DUST_FRAMES - 1) as f32;
    let alpha = 0.7 - 0.45 * t;
    for k in 0..3 {
        let x = w as f32 * (0.3 + 0.2 * k as f32);
        let r = (3.5 + 3.0 * t + k as f32 * 0.6) * SS as f32;
        puff(&mut c, x, h as f32 - r * 0.8, r, rgb(0xb8a07a), alpha, 70 + k as u32 * 3);
    }
    c.downsample(SS)
}

fn rain() -> Canvas {
    let (w, h) = (4 * SS, 12 * SS);
    let mut c = Canvas::new(w, h);
    c.thick_line(w as f32 * 0.75, 2.0, w as f32 * 0.25, h as f32 - 2.0, 0.6 * SS as f32, rgba(0xd8e8f4, 200));
    c.downsample(SS)
}

fn snowflake() -> Canvas {
    let s = 8 * SS;
    let mut c = Canvas::new(s, s);
    let m = s as f32 * 0.5;
    for k in 0..3 {
        let a = PI / 3.0 * k as f32;
        let (dx, dy) = (a.cos() * m * 0.85, a.sin() * m * 0.85);
        c.thick_line(m - dx, m - dy, m + dx, m + dy, 0.45 * SS as f32, rgba(0xffffff, 235));
    }
    c.fill_ellipse(m, m, m * 0.3, m * 0.3, |_, _, _| rgba(0xffffff, 255));
    c.downsample(SS)
}

/// A small falling leaf: tilted ellipse with a darker midrib, lit on its upper half.
fn leaf(variant: u32) -> Canvas {
    let s = 8 * SS;
    let mut c = Canvas::new(s, s);
    let m = s as f32 * 0.5;
    let angle = 0.5 + variant as f32 * 0.7;
    let (ca, sa) = (angle.cos(), angle.sin());
    let (rx, ry) = (m * 0.85, m * 0.42);
    let col = rgb(LEAF_COLORS[variant as usize % LEAF_COLORS.len()]);
    for y in 0..s as i32 {
        for x in 0..s as i32 {
            let (dx, dy) = (x as f32 + 0.5 - m, y as f32 + 0.5 - m);
            let (u, v) = (dx * ca + dy * sa, -dx * sa + dy * ca);
            if (u / rx).powi(2) + (v / ry).powi(2) > 1.0 {
                continue;
            }
            let rib = v.abs() < SS as f32 * 0.35 && u.abs() < rx * 0.9;
            let px = if rib { shade(col, 0.7) } else { shade(col, if v < 0.0 { 1.08 } else { 0.9 }) };
            c.set(x, y, px);
        }
    }
    c.downsample(SS)
}

fn sparkle() -> Canvas {
    let s = 12 * SS;
    let mut c = Canvas::new(s, s);
    let m = s as f32 * 0.5;
    let pts = [(m, 0.0), (m + m * 0.18, m - m * 0.18), (s as f32, m), (m + m * 0.18, m + m * 0.18), (m, s as f32), (m - m * 0.18, m + m * 0.18), (0.0, m), (m - m * 0.18, m - m * 0.18)];
    c.fill_polygon(&pts, |x, y| {
        let d = ((x as f32 - m).powi(2) + (y as f32 - m).powi(2)).sqrt() / m;
        mix(rgba(0xffffff, 255), rgba(0xf4c840, 200), d.min(1.0))
    });
    c.downsample(SS)
}

/// Assembles the `effects` atlas with `smoke`, `fire` and `dust` animations.
pub fn atlas() -> AtlasBuilder {
    let mut atlas = AtlasBuilder::new("effects", Sampling::Smooth);
    let mut names = Vec::new();
    for f in 0..SMOKE_FRAMES {
        atlas.add(format!("smoke_{f}"), smoke(f), (0.5, 0.5));
        names.push(format!("smoke_{f}"));
    }
    atlas.animation("smoke", names, 6.0, true);
    let mut names = Vec::new();
    for f in 0..FIRE_FRAMES {
        atlas.add(format!("fire_{f}"), fire(f), (0.5, 1.0));
        names.push(format!("fire_{f}"));
    }
    atlas.animation("fire", names, 10.0, true);
    let mut names = Vec::new();
    for f in 0..DUST_FRAMES {
        atlas.add(format!("dust_{f}"), dust(f), (0.5, 1.0));
        names.push(format!("dust_{f}"));
    }
    atlas.animation("dust", names, 10.0, false);
    atlas.add("rain", rain(), (0.5, 0.5));
    atlas.add("snowflake", snowflake(), (0.5, 0.5));
    atlas.add("sparkle", sparkle(), (0.5, 0.5));
    for v in 0..LEAF_VARIANTS {
        atlas.add(format!("leaf_{v}"), leaf(v), (0.5, 0.5));
    }
    for k in 0..ARROW_DIRS {
        atlas.add(format!("arrow_{k}"), arrow(k), (0.5, 0.5));
    }
    atlas.add("stone", stone(), (0.5, 0.5));
    atlas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total_alpha(c: &Canvas) -> u64 {
        c.img.pixels().map(|p| u64::from(p.0[3])).sum()
    }

    #[test]
    fn arrows_point_their_way() {
        // The steel head (light, bluish) lies on the side the arrow points to.
        let head_side = |c: &Canvas| {
            let (mut sx, mut sy, mut n) = (0.0f32, 0.0f32, 0.0f32);
            for (x, y, p) in c.img.enumerate_pixels() {
                if p.0[3] > 100 && p.0[2] > 170 && p.0[2] > p.0[0] {
                    sx += x as f32 - 8.0;
                    sy += y as f32 - 8.0;
                    n += 1.0;
                }
            }
            (sx / n.max(1.0), sy / n.max(1.0))
        };
        let right = head_side(&arrow(0));
        let down = head_side(&arrow(ARROW_DIRS / 4));
        let left = head_side(&arrow(ARROW_DIRS / 2));
        assert!(right.0 > 2.0 && right.1.abs() < 1.0, "{right:?}");
        assert!(down.1 > 2.0 && down.0.abs() < 1.0, "{down:?}");
        assert!(left.0 < -2.0, "{left:?}");
        assert!(stone().coverage() > 0.4);
    }

    #[test]
    fn smoke_fades_while_growing() {
        let first = smoke(0);
        let last = smoke(SMOKE_FRAMES - 1);
        let max_alpha = |c: &Canvas| c.img.pixels().map(|p| p.0[3]).max().unwrap_or(0);
        assert!(max_alpha(&last) < max_alpha(&first));
        assert!(last.coverage() > first.coverage());
    }

    #[test]
    fn fire_frames_differ_and_are_visible() {
        let frames: Vec<Canvas> = (0..FIRE_FRAMES).map(fire).collect();
        for f in &frames {
            assert!(total_alpha(f) > 10_000);
        }
        assert_ne!(frames[0].img, frames[1].img);
    }

    #[test]
    fn weather_sprites_have_content() {
        for c in [rain(), snowflake(), sparkle(), dust(0), leaf(0), leaf(3)] {
            assert!(total_alpha(&c) > 500);
        }
        assert_ne!(leaf(0).img, leaf(1).img);
    }
}
