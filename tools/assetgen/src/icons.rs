//! Icons (`icons` atlas, smooth sampling): resources (`res_*`, 32 px), buildings (`bld_*`,
//! 48 px, scaled-down building sprites), UI symbols (`ui_*`, 32 px) and the crests of the
//! kingdoms (`crest_<colour index>`, 40 × 48 px heater shields in the kingdom colours).
//!
//! Pictograms are drawn 4× supersampled on a 128 px canvas (coordinates below are in that
//! space), outlined, then filtered down, which gives clean anti-aliased edges.

use std::f32::consts::{PI, TAU};

use kingdoms_engine::rendering::texture::Sampling;

use crate::atlas::AtlasBuilder;
use crate::catalog::{self, BuildingInfo, Category};
use crate::draw::{Canvas, Px, rgb, rgba, shade};
use crate::palette::*;

/// Supersampling factor.
const SS: u32 = 4;
/// Final size of resource and UI icons.
pub const SMALL: u32 = 32;
/// Edge of the supersampled pictogram canvas.
const S: u32 = SMALL * SS;
/// Final size of building icons.
pub const LARGE: u32 = 48;

/// UI icon names (`ui_<name>`).
pub const UI_ICONS: &[&str] = &[
    "population",
    "gold",
    "happiness",
    "food",
    "wood",
    "stone",
    "iron",
    "date",
    "season_spring",
    "season_summer",
    "season_autumn",
    "season_winter",
    "pause",
    "play",
    "speed2",
    "speed4",
    "build",
    "roads",
    "farm",
    "production",
    "military",
    "research",
    "events",
    "notifications",
    "tasks",
    "overview",
    "demolish",
    "upgrade",
    "rotate",
    "move",
    "info",
    "close",
    "warning",
    "trade",
    "diplomacy",
    "finance",
    "statistics",
    "storage",
    "settings",
    "save",
    "load",
    "inspect",
    "grid",
    "territory",
    "economy",
    "medicine",
    "administration",
    "rank",
];

// --- Drawing kit (128 px space) --------------------------------------------------------------

/// Polygon with a top-left lit gradient.
fn shape(c: &mut Canvas, pts: &[(f32, f32)], base: u32) {
    let x0 = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let x1 = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let y0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let y1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let b = rgb(base);
    c.fill_polygon(pts, |x, y| {
        let tx = (x as f32 - x0) / (x1 - x0).max(1.0);
        let ty = (y as f32 - y0) / (y1 - y0).max(1.0);
        shade(b, 1.18 - 0.3 * ty - 0.12 * tx)
    });
}

fn rect(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, base: u32) {
    shape(c, &[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], base);
}

fn blob(c: &mut Canvas, cx: f32, cy: f32, rx: f32, ry: f32, base: u32) {
    c.lit_ellipse(cx, cy, rx, ry, rgb(base));
}

fn bar(c: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, col: u32) {
    c.thick_line(x0, y0, x1, y1, r, rgb(col));
}

fn ring(c: &mut Canvas, cx: f32, cy: f32, ro: f32, ri: f32, col: Px) {
    c.fill_ellipse(cx, cy, ro, ro, |x, y, _| {
        let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
        if d >= ri { col } else { [0, 0, 0, 0] }
    });
}

/// Arc of a ring between angles `a0..a1` (radians, screen orientation).
fn arc(c: &mut Canvas, cx: f32, cy: f32, r: f32, (a0, a1): (f32, f32), w: f32, col: u32) {
    let n = 40;
    for i in 0..n {
        let t0 = a0 + (a1 - a0) * i as f32 / n as f32;
        let t1 = a0 + (a1 - a0) * (i + 1) as f32 / n as f32;
        bar(c, cx + t0.cos() * r, cy + t0.sin() * r, cx + t1.cos() * r, cy + t1.sin() * r, w, col);
    }
}

fn tri(c: &mut Canvas, a: (f32, f32), b: (f32, f32), d: (f32, f32), base: u32) {
    shape(c, &[a, b, d], base);
}

/// Arrow from (x0, y0) to (x1, y1) with a triangular head.
fn arrow(c: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, col: u32) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let (ux, uy) = (dx / len, dy / len);
    let head = w * 3.2;
    let (bx, by) = (x1 - ux * head, y1 - uy * head);
    bar(c, x0, y0, bx, by, w, col);
    shape(c, &[(x1, y1), (bx - uy * head * 0.8, by + ux * head * 0.8), (bx + uy * head * 0.8, by - ux * head * 0.8)], col);
}

fn coin(c: &mut Canvas, cx: f32, cy: f32, r: f32) {
    c.fill_ellipse(cx, cy + r * 0.12, r, r * 0.92, |_, _, _| rgb(0xa87a18));
    blob(c, cx, cy, r, r * 0.9, GOLD);
    ring(c, cx, cy, r * 0.72, r * 0.6, rgba(0xb8861c, 255));
    c.fill_ellipse(cx - r * 0.3, cy - r * 0.35, r * 0.18, r * 0.12, |_, _, _| rgba(0xfff4c0, 220));
}

fn gear(c: &mut Canvas, cx: f32, cy: f32, r: f32, base: u32) {
    let teeth = 8;
    let pts: Vec<(f32, f32)> = (0..teeth * 4)
        .map(|i| {
            let a = TAU * i as f32 / (teeth * 4) as f32;
            let rr = if (i / 2) % 2 == 0 { r } else { r * 0.78 };
            (cx + a.cos() * rr, cy + a.sin() * rr)
        })
        .collect();
    shape(c, &pts, base);
    // Punch the axle hole (blending cannot erase, so clear the pixels explicitly).
    let (x0, x1) = ((cx - r * 0.3) as i32, (cx + r * 0.3) as i32 + 1);
    for y in (cy - r * 0.3) as i32..=(cy + r * 0.3) as i32 + 1 {
        for x in x0..=x1 {
            if ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt() < r * 0.3 {
                c.set(x, y, [0, 0, 0, 0]);
            }
        }
    }
}

fn log_end(c: &mut Canvas, cx: f32, cy: f32, r: f32) {
    blob(c, cx, cy, r, r, BARK);
    c.fill_ellipse(cx, cy, r * 0.8, r * 0.8, |_, _, d| {
        let ring = (d.sqrt() * 4.0).fract() < 0.22;
        if ring { rgb(0xa8804e) } else { rgb(END_GRAIN) }
    });
}

fn rock(c: &mut Canvas, cx: f32, cy: f32, rx: f32, ry: f32, base: u32) {
    let pts: Vec<(f32, f32)> = (0..9)
        .map(|i| {
            let a = TAU * i as f32 / 9.0;
            let k = 0.82 + 0.18 * ((i * 7 % 5) as f32 / 4.0);
            (cx + a.cos() * rx * k, cy + a.sin() * ry * k)
        })
        .collect();
    shape(c, &pts, base);
}

fn sack(c: &mut Canvas, cx: f32, top: f32, w: f32, h: f32, base: u32) {
    shape(c, &[(cx - w * 0.2, top + h * 0.12), (cx + w * 0.2, top + h * 0.12), (cx + w * 0.5, top + h * 0.6), (cx + w * 0.42, top + h), (cx - w * 0.42, top + h), (cx - w * 0.5, top + h * 0.6)], base);
    bar(c, cx - w * 0.22, top + h * 0.16, cx + w * 0.22, top + h * 0.16, 3.0, 0x6a4a2a);
    shape(c, &[(cx - w * 0.16, top), (cx + w * 0.16, top), (cx + w * 0.2, top + h * 0.14), (cx - w * 0.2, top + h * 0.14)], base);
}

fn wheat_ear(c: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32, col: u32) {
    bar(c, x0, y0, x1, y1, 2.2, 0xa8883a);
    let (dx, dy) = (x1 - x0, y1 - y0);
    for k in 0..6 {
        let t = 0.45 + k as f32 * 0.1;
        let (x, y) = (x0 + dx * t, y0 + dy * t);
        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
        blob(c, x + side * 5.0, y, 5.0, 7.5, col);
    }
    blob(c, x1, y1 - 2.0, 4.5, 7.0, col);
}

fn person(c: &mut Canvas, cx: f32, cy: f32, s: f32, body: u32) {
    shape(c, &[(cx - 26.0 * s, cy + 40.0 * s), (cx - 20.0 * s, cy + 12.0 * s), (cx + 20.0 * s, cy + 12.0 * s), (cx + 26.0 * s, cy + 40.0 * s)], body);
    blob(c, cx, cy - 4.0 * s, 14.0 * s, 15.0 * s, SKIN);
    c.fill_ellipse(cx, cy - 12.0 * s, 14.5 * s, 8.0 * s, |_, _, _| rgb(HAIR_BROWN));
}

fn book(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, cover: u32) {
    rect(c, x, y, w, h, cover);
    rect(c, x + w * 0.08, y + h - 10.0, w * 0.88, 8.0, 0xf0e6cc);
    rect(c, x + w * 0.12, y + 8.0, w * 0.06, h - 20.0, GOLD);
}

fn scroll(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
    rect(c, x + 6.0, y + 6.0, w - 12.0, h - 12.0, PARCHMENT);
    blob(c, x + w * 0.5, y + 6.0, w * 0.55, 8.0, PARCHMENT_DARK);
    blob(c, x + w * 0.5, y + h - 6.0, w * 0.55, 8.0, PARCHMENT_DARK);
}

/// Finishes a supersampled pictogram: outline and downsampling to 32 px.
fn finish(c: &Canvas) -> Canvas {
    c.outlined(5, rgba(OUTLINE, 255)).downsample(SS)
}

// --- Resources ---------------------------------------------------------------------------------

/// Pictogram for resource `id` (32 px).
pub fn resource(id: &str) -> Canvas {
    let mut c = Canvas::new(S, S);
    draw_resource(&mut c, id);
    finish(&c)
}

fn draw_resource(c: &mut Canvas, id: &str) {
    match id {
        "wood" => {
            log_end(c, 40.0, 86.0, 25.0);
            log_end(c, 90.0, 86.0, 25.0);
            log_end(c, 65.0, 44.0, 25.0);
        }
        "stone" => {
            rock(c, 44.0, 82.0, 34.0, 26.0, STONE);
            rock(c, 86.0, 76.0, 30.0, 30.0, STONE_WARM);
            rock(c, 64.0, 46.0, 24.0, 18.0, 0xb0aba2);
        }
        "loam" => {
            shape(c, &[(10.0, 104.0), (30.0, 60.0), (64.0, 40.0), (98.0, 58.0), (118.0, 104.0)], 0x8a6a44);
            for (x, y) in [(44.0, 74.0), (70.0, 64.0), (84.0, 86.0), (54.0, 92.0)] {
                blob(c, x, y, 5.0, 4.0, 0x6a5034);
            }
        }
        "clay" => {
            shape(c, &[(20.0, 58.0), (64.0, 38.0), (108.0, 58.0), (64.0, 78.0)], 0xd08a64);
            shape(c, &[(20.0, 58.0), (64.0, 78.0), (64.0, 108.0), (20.0, 88.0)], 0xb0603e);
            shape(c, &[(64.0, 78.0), (108.0, 58.0), (108.0, 88.0), (64.0, 108.0)], 0x8a4a30);
        }
        "iron_ore" => {
            rock(c, 64.0, 70.0, 48.0, 40.0, 0x6a625c);
            for (x, y, r) in [(46.0, 60.0, 9.0), (78.0, 52.0, 7.0), (70.0, 84.0, 10.0), (40.0, 86.0, 6.0)] {
                blob(c, x, y, r, r * 0.8, 0xb4552e);
            }
        }
        "coal" => {
            for (x, y, r) in [(40.0, 88.0, 24.0), (88.0, 88.0, 24.0), (64.0, 54.0, 24.0)] {
                rock(c, x, y, r, r * 0.85, 0x2a2826);
                c.fill_ellipse(x - r * 0.35, y - r * 0.35, r * 0.25, r * 0.15, |_, _, _| rgba(0x9aa4b0, 200));
            }
        }
        "sand" => {
            shape(c, &[(8.0, 106.0), (40.0, 58.0), (64.0, 44.0), (88.0, 58.0), (120.0, 106.0)], 0xe2cf96);
            for i in 0..14 {
                let x = 30.0 + (i * 37 % 70) as f32;
                let y = 70.0 + (i * 23 % 30) as f32;
                c.fill_ellipse(x, y, 2.0, 2.0, |_, _, _| rgb(0xb8a06a));
            }
        }
        "wool" => {
            for (x, y, r) in [(44.0, 70.0, 26.0), (80.0, 66.0, 28.0), (60.0, 46.0, 24.0), (64.0, 88.0, 26.0)] {
                blob(c, x, y, r, r, 0xf0ebdc);
            }
            arc(c, 64.0, 66.0, 30.0, (0.2, 2.0), 2.5, 0xc8c0a8);
        }
        "grain" => {
            for k in -3..=3 {
                let x = 64.0 + k as f32 * 9.0;
                wheat_ear(c, 64.0 + k as f32 * 3.0, 118.0, x, 30.0 + (k * k) as f32 * 2.0, 0xd8c88a);
            }
            bar(c, 50.0, 88.0, 78.0, 88.0, 5.0, 0x8a5a2a);
        }
        "hides" => {
            shape(c, &[(30.0, 30.0), (52.0, 40.0), (76.0, 40.0), (98.0, 30.0), (92.0, 60.0), (108.0, 98.0), (80.0, 92.0), (64.0, 110.0), (48.0, 92.0), (20.0, 98.0), (36.0, 60.0)], 0x9a6a44);
            blob(c, 64.0, 68.0, 22.0, 26.0, 0xb88a5e);
        }
        "gold_ore" => {
            rock(c, 64.0, 70.0, 48.0, 40.0, 0x8a847a);
            for (x, y, r) in [(48.0, 62.0, 10.0), (78.0, 56.0, 8.0), (70.0, 86.0, 11.0)] {
                blob(c, x, y, r, r * 0.8, GOLD);
                c.fill_ellipse(x - r * 0.3, y - r * 0.3, r * 0.3, r * 0.2, |_, _, _| rgb(0xfff4c0));
            }
        }
        "wheat" => {
            wheat_ear(c, 40.0, 120.0, 34.0, 26.0, 0xe0b84a);
            wheat_ear(c, 64.0, 120.0, 64.0, 18.0, 0xe8c050);
            wheat_ear(c, 88.0, 120.0, 94.0, 26.0, 0xe0b84a);
        }
        "flour" => {
            sack(c, 64.0, 26.0, 84.0, 90.0, 0xe8dcc0);
            blob(c, 64.0, 36.0, 26.0, 10.0, 0xffffff);
            wheat_ear(c, 64.0, 100.0, 64.0, 62.0, 0xc8a040);
        }
        "bread" => {
            blob(c, 64.0, 72.0, 52.0, 32.0, 0xc8843a);
            for k in 0..3 {
                let x = 40.0 + k as f32 * 22.0;
                bar(c, x, 60.0, x + 12.0, 76.0, 3.0, 0xe8b870);
            }
        }
        "meat" => {
            bar(c, 92.0, 36.0, 66.0, 62.0, 7.0, 0xf0e8dc);
            blob(c, 98.0, 30.0, 8.0, 8.0, 0xf0e8dc);
            blob(c, 104.0, 38.0, 7.0, 7.0, 0xf0e8dc);
            blob(c, 52.0, 78.0, 36.0, 30.0, 0xf0e0d0);
            blob(c, 50.0, 80.0, 30.0, 25.0, 0xb8443a);
        }
        "fish" => {
            shape(c, &[(96.0, 64.0), (122.0, 40.0), (116.0, 64.0), (122.0, 88.0)], 0x6a8aa0);
            blob(c, 58.0, 64.0, 46.0, 22.0, 0x8ab0c8);
            shape(c, &[(50.0, 44.0), (74.0, 30.0), (72.0, 48.0)], 0x6a8aa0);
            c.fill_ellipse(30.0, 58.0, 4.5, 4.5, |_, _, _| rgb(0x101820));
            arc(c, 40.0, 64.0, 14.0, (-0.9, 0.9), 1.8, 0x5a7890);
        }
        "milk" => {
            // White glazed pitcher with a blue band and spout.
            arc(c, 92.0, 66.0, 20.0, (-1.3, 1.4), 5.0, 0xd8d4cc);
            shape(c, &[(36.0, 26.0), (84.0, 26.0), (82.0, 44.0), (96.0, 78.0), (88.0, 114.0), (32.0, 114.0), (24.0, 78.0), (38.0, 44.0)], 0xf4f0e6);
            shape(c, &[(26.0, 22.0), (42.0, 26.0), (36.0, 34.0)], 0xf4f0e6);
            rect(c, 26.0, 70.0, 68.0, 10.0, 0x4a6ab0);
            c.fill_ellipse(60.0, 27.0, 23.0, 5.0, |_, _, _| rgb(0xfffefa));
        }
        "cheese" => {
            // Wedge: triangular front face, rind on the right, holes.
            shape(c, &[(10.0, 104.0), (104.0, 104.0), (104.0, 30.0)], 0xf4d060);
            shape(c, &[(104.0, 30.0), (120.0, 40.0), (120.0, 112.0), (104.0, 104.0)], 0xd89a28);
            shape(c, &[(10.0, 104.0), (104.0, 104.0), (120.0, 112.0), (26.0, 112.0)], 0xc88a20);
            for (x, y, r) in [(76.0, 86.0, 8.0), (92.0, 62.0, 6.0), (52.0, 96.0, 5.0), (88.0, 92.0, 4.0)] {
                blob(c, x, y, r, r * 0.85, 0xc89a30);
            }
        }
        "vegetables" => {
            shape(c, &[(70.0, 112.0), (110.0, 46.0), (120.0, 52.0)], 0xe07a2a);
            for k in 0..3 {
                bar(c, 114.0, 48.0, 104.0 + k as f32 * 8.0, 22.0, 2.5, 0x5aa83a);
            }
            blob(c, 50.0, 76.0, 36.0, 32.0, 0x6aaa4a);
            arc(c, 50.0, 76.0, 20.0, (3.4, 5.8), 2.0, 0x4a8a3a);
            arc(c, 50.0, 80.0, 10.0, (3.2, 6.0), 2.0, 0x4a8a3a);
        }
        "fruit" => {
            blob(c, 64.0, 72.0, 40.0, 38.0, 0xc8281e);
            bar(c, 64.0, 38.0, 70.0, 20.0, 3.0, 0x5a3a20);
            shape(c, &[(70.0, 28.0), (96.0, 16.0), (88.0, 34.0)], 0x5a9a3a);
        }
        "honey" => {
            shape(c, &[(34.0, 44.0), (94.0, 44.0), (104.0, 80.0), (90.0, 112.0), (38.0, 112.0), (24.0, 80.0)], 0xb07a3a);
            rect(c, 30.0, 36.0, 68.0, 12.0, 0x8a5a2a);
            shape(c, &[(40.0, 48.0), (88.0, 48.0), (84.0, 60.0), (70.0, 58.0), (66.0, 72.0), (60.0, 58.0), (44.0, 60.0)], 0xf0b830);
            bar(c, 80.0, 40.0, 108.0, 10.0, 3.0, PLANK);
        }
        "planks" => {
            for k in 0..3 {
                let y = 44.0 + k as f32 * 20.0;
                shape(c, &[(12.0, y + 16.0), (84.0, y - 10.0), (116.0, y + 2.0), (44.0, y + 28.0)], PLANK_LIGHT);
                shape(c, &[(44.0, y + 28.0), (116.0, y + 2.0), (116.0, y + 8.0), (44.0, y + 34.0)], PLANK);
            }
        }
        "tools" => {
            bar(c, 30.0, 104.0, 92.0, 30.0, 5.0, PLANK);
            shape(c, &[(76.0, 18.0), (112.0, 46.0), (104.0, 54.0), (70.0, 26.0)], 0x8a9098);
            bar(c, 98.0, 104.0, 40.0, 40.0, 5.0, PLANK);
            shape(c, &[(22.0, 34.0), (44.0, 18.0), (60.0, 34.0), (38.0, 52.0)], 0x6a7078);
        }
        "bricks" => {
            for &(x, y) in &[(14.0, 74.0), (66.0, 74.0), (40.0, 44.0)] {
                rect(c, x, y, 48.0, 26.0, BRICK);
                rect(c, x, y, 48.0, 6.0, 0xc8705a);
            }
        }
        "iron_bars" => {
            for &(x, y) in &[(14.0, 80.0), (66.0, 80.0), (40.0, 50.0)] {
                shape(c, &[(x + 8.0, y), (x + 40.0, y), (x + 48.0, y + 24.0), (x, y + 24.0)], 0x8a9098);
                rect(c, x + 8.0, y, 32.0, 5.0, 0xc0c6ce);
            }
        }
        "cloth" => {
            rect(c, 20.0, 44.0, 76.0, 52.0, 0x3a5aa8);
            blob(c, 98.0, 70.0, 16.0, 26.0, 0x2a4a90);
            arc(c, 98.0, 70.0, 9.0, (0.0, 5.0), 2.0, 0x8aa0d8);
            bar(c, 24.0, 56.0, 92.0, 56.0, 2.0, 0x6a8ad0);
            bar(c, 24.0, 84.0, 92.0, 84.0, 2.0, 0x1a3a80);
        }
        "clothes" => {
            shape(c, &[(42.0, 22.0), (86.0, 22.0), (118.0, 44.0), (104.0, 62.0), (92.0, 52.0), (92.0, 112.0), (36.0, 112.0), (36.0, 52.0), (24.0, 62.0), (10.0, 44.0)], 0x8a3a3a);
            shape(c, &[(54.0, 22.0), (74.0, 22.0), (64.0, 36.0)], 0x5a2020);
            rect(c, 36.0, 74.0, 56.0, 8.0, 0x5a3a20);
        }
        "furniture" => {
            rect(c, 34.0, 16.0, 12.0, 96.0, 0x8a5a34);
            rect(c, 34.0, 62.0, 62.0, 12.0, PLANK_LIGHT);
            rect(c, 84.0, 74.0, 12.0, 38.0, 0x8a5a34);
            rect(c, 46.0, 24.0, 8.0, 36.0, 0x9a6a40);
            rect(c, 34.0, 24.0, 40.0, 8.0, 0x9a6a40);
        }
        "weapons" => {
            for flip in [false, true] {
                let (x0, x1) = if flip { (108.0, 24.0) } else { (20.0, 104.0) };
                bar(c, x0, 20.0, x1 * 0.85 + x0 * 0.15, 96.0, 5.0, 0xd0d6de);
                let (hx, hy) = (x1, 108.0);
                bar(c, x1 * 0.85 + x0 * 0.15, 96.0, hx, hy, 5.0, 0x6a4424);
                let gx = x1 * 0.8 + x0 * 0.2;
                bar(c, gx - 12.0, 88.0, gx + 12.0, 100.0, 4.0, GOLD);
            }
        }
        "armor" => {
            // Breastplate with shoulder guards and a riveted waist band.
            shape(c, &[(34.0, 22.0), (94.0, 22.0), (104.0, 40.0), (96.0, 104.0), (64.0, 114.0), (32.0, 104.0), (24.0, 40.0)], 0xa8b0ba);
            shape(c, &[(16.0, 24.0), (40.0, 18.0), (44.0, 44.0), (20.0, 50.0)], 0x8a929c);
            shape(c, &[(88.0, 18.0), (112.0, 24.0), (108.0, 50.0), (84.0, 44.0)], 0x8a929c);
            shape(c, &[(50.0, 22.0), (78.0, 22.0), (64.0, 38.0)], 0x5a5048);
            bar(c, 64.0, 40.0, 64.0, 104.0, 2.0, 0xd8dee6);
            rect(c, 34.0, 86.0, 60.0, 9.0, 0x6a4424);
            for x in [42.0, 54.0, 74.0, 86.0] {
                blob(c, x, 90.5, 2.6, 2.6, GOLD);
            }
        }
        "ceramics" => {
            blob(c, 64.0, 74.0, 34.0, 38.0, 0xb8643c);
            rect(c, 52.0, 22.0, 24.0, 22.0, 0xb8643c);
            rect(c, 46.0, 16.0, 36.0, 8.0, 0xa85a34);
            arc(c, 40.0, 46.0, 10.0, (1.6, 4.4), 3.5, 0xa85a34);
            arc(c, 88.0, 46.0, 10.0, (-1.3, 1.5), 3.5, 0xa85a34);
            bar(c, 34.0, 76.0, 94.0, 76.0, 3.0, 0x6a3a20);
        }
        "beer" => {
            rect(c, 30.0, 40.0, 56.0, 74.0, 0x9a6a3a);
            for x in [42.0, 58.0, 74.0] {
                bar(c, x, 44.0, x, 110.0, 1.5, 0x7a4a24);
            }
            arc(c, 88.0, 76.0, 18.0, (-1.5, 1.5), 6.0, 0x7a4a24);
            for (x, y, r) in [(36.0, 38.0, 12.0), (54.0, 32.0, 14.0), (74.0, 36.0, 13.0), (84.0, 42.0, 9.0)] {
                blob(c, x, y, r, r * 0.9, 0xfbf6ea);
            }
        }
        "leather" => {
            shape(c, &[(14.0, 40.0), (96.0, 40.0), (96.0, 96.0), (14.0, 96.0)], 0x8a5a34);
            blob(c, 96.0, 68.0, 18.0, 28.0, 0x6a4024);
            arc(c, 96.0, 68.0, 10.0, (0.5, 5.5), 2.0, 0xb08050);
            for k in 0..7 {
                let x = 22.0 + k as f32 * 11.0;
                bar(c, x, 48.0, x + 5.0, 48.0, 1.5, 0xe0c8a0);
            }
        }
        "jewelry" => {
            ring(c, 64.0, 74.0, 36.0, 26.0, rgb(GOLD));
            arc(c, 64.0, 74.0, 31.0, (3.6, 5.0), 2.5, GOLD_LIGHT);
            shape(c, &[(50.0, 34.0), (78.0, 34.0), (86.0, 44.0), (64.0, 62.0), (42.0, 44.0)], 0xc8202a);
            shape(c, &[(54.0, 38.0), (64.0, 38.0), (56.0, 46.0)], 0xff9aa0);
        }
        "wine" => {
            rect(c, 24.0, 40.0, 30.0, 74.0, 0x2a4a2a);
            rect(c, 32.0, 14.0, 14.0, 28.0, 0x2a4a2a);
            rect(c, 30.0, 10.0, 18.0, 8.0, 0x7a2a3a);
            rect(c, 28.0, 64.0, 22.0, 24.0, 0xe8dcc0);
            shape(c, &[(66.0, 46.0), (110.0, 46.0), (104.0, 76.0), (72.0, 76.0)], 0xd8e4ec);
            shape(c, &[(69.0, 58.0), (107.0, 58.0), (104.0, 76.0), (72.0, 76.0)], 0x8a1a2a);
            bar(c, 88.0, 76.0, 88.0, 104.0, 3.0, 0xd8e4ec);
            rect(c, 72.0, 104.0, 32.0, 6.0, 0xd8e4ec);
        }
        "spices" => {
            sack(c, 56.0, 40.0, 80.0, 76.0, 0xb89a6a);
            blob(c, 56.0, 46.0, 26.0, 12.0, 0xc8481e);
            blob(c, 50.0, 40.0, 12.0, 6.0, 0xe0702a);
            for (x, y) in [(100.0, 104.0), (108.0, 96.0), (96.0, 94.0)] {
                blob(c, x, y, 5.0, 5.0, 0x3a2a20);
            }
        }
        "silk" => {
            rect(c, 18.0, 40.0, 80.0, 52.0, 0x8a3aa0);
            blob(c, 100.0, 66.0, 16.0, 26.0, 0x6a2a80);
            shape(c, &[(36.0, 40.0), (56.0, 40.0), (40.0, 92.0), (20.0, 92.0)], 0xd8a0e8);
            arc(c, 100.0, 66.0, 9.0, (0.0, 5.0), 2.0, 0xd8a0e8);
        }
        "art" => {
            rect(c, 14.0, 20.0, 100.0, 88.0, GOLD);
            rect(c, 26.0, 32.0, 76.0, 64.0, 0x7ab0d8);
            shape(c, &[(26.0, 96.0), (26.0, 70.0), (52.0, 56.0), (78.0, 74.0), (102.0, 62.0), (102.0, 96.0)], 0x5a8a3a);
            blob(c, 82.0, 46.0, 8.0, 8.0, 0xf4d040);
        }
        _ => {
            ring(c, 64.0, 64.0, 40.0, 30.0, rgb(0xff00ff));
        }
    }
}

// --- UI ------------------------------------------------------------------------------------------

/// Pictogram for UI symbol `name` (32 px).
pub fn ui(name: &str) -> Canvas {
    let mut c = Canvas::new(S, S);
    draw_ui(&mut c, name);
    finish(&c)
}

fn draw_ui(c: &mut Canvas, name: &str) {
    match name {
        "population" => {
            person(c, 82.0, 46.0, 1.0, 0x5a6a8a);
            person(c, 48.0, 60.0, 1.15, 0x8a5a3a);
        }
        "gold" => {
            for k in 0..3 {
                coin(c, 52.0, 96.0 - k as f32 * 14.0, 30.0);
            }
            coin(c, 84.0, 78.0, 30.0);
        }
        "happiness" => {
            blob(c, 64.0, 64.0, 50.0, 50.0, 0xf4c840);
            blob(c, 46.0, 52.0, 6.0, 9.0, 0x3a2a1a);
            blob(c, 82.0, 52.0, 6.0, 9.0, 0x3a2a1a);
            arc(c, 64.0, 66.0, 28.0, (0.35, PI - 0.35), 4.5, 0x3a2a1a);
        }
        "food" => {
            draw_resource(c, "bread");
            blob(c, 96.0, 40.0, 20.0, 19.0, 0xc8281e);
            bar(c, 96.0, 22.0, 100.0, 12.0, 2.5, 0x5a3a20);
        }
        "wood" => draw_resource(c, "wood"),
        "stone" => draw_resource(c, "stone"),
        "iron" => draw_resource(c, "iron_bars"),
        "date" => {
            rect(c, 18.0, 24.0, 92.0, 88.0, PARCHMENT);
            rect(c, 18.0, 24.0, 92.0, 22.0, 0xa8322a);
            for &x in &[38.0, 90.0] {
                rect(c, x - 4.0, 14.0, 8.0, 18.0, 0x6a6a6a);
            }
            for r in 0..3 {
                for k in 0..4 {
                    rect(c, 28.0 + k as f32 * 20.0, 56.0 + r as f32 * 16.0, 12.0, 10.0, if r == 1 && k == 2 { 0xa8322a } else { 0xc8b07a });
                }
            }
        }
        "season_spring" => {
            bar(c, 64.0, 70.0, 60.0, 120.0, 4.0, 0x4a8a3a);
            shape(c, &[(62.0, 100.0), (30.0, 84.0), (40.0, 108.0)], 0x5aa84a);
            for k in 0..5 {
                let a = TAU * k as f32 / 5.0 - PI / 2.0;
                blob(c, 64.0 + a.cos() * 24.0, 52.0 + a.sin() * 24.0, 17.0, 17.0, 0xf0a0c0);
            }
            blob(c, 64.0, 52.0, 13.0, 13.0, 0xf4d040);
        }
        "season_summer" => {
            for k in 0..12 {
                let a = TAU * k as f32 / 12.0;
                let (r0, r1) = (36.0, if k % 2 == 0 { 58.0 } else { 50.0 });
                bar(c, 64.0 + a.cos() * r0, 64.0 + a.sin() * r0, 64.0 + a.cos() * r1, 64.0 + a.sin() * r1, 4.5, 0xf0a020);
            }
            blob(c, 64.0, 64.0, 32.0, 32.0, 0xf8d040);
        }
        "season_autumn" => {
            let pts: Vec<(f32, f32)> = (0..10)
                .map(|i| {
                    let a = TAU * i as f32 / 10.0 - PI / 2.0;
                    let r = if i % 2 == 0 { 52.0 } else { 26.0 };
                    (64.0 + a.cos() * r, 58.0 + a.sin() * r)
                })
                .collect();
            shape(c, &pts, 0xd8642a);
            bar(c, 64.0, 60.0, 64.0, 120.0, 4.0, 0x7a3a1a);
            for k in 0..4 {
                let a = -PI / 2.0 + (k as f32 - 1.5) * 0.9;
                bar(c, 64.0, 64.0, 64.0 + a.cos() * 36.0, 64.0 + a.sin() * 36.0, 1.8, 0xa8401a);
            }
        }
        "season_winter" => {
            for k in 0..3 {
                let a = PI / 3.0 * k as f32 + PI / 6.0;
                let (dx, dy) = (a.cos() * 52.0, a.sin() * 52.0);
                bar(c, 64.0 - dx, 64.0 - dy, 64.0 + dx, 64.0 + dy, 5.0, 0xcfe8f8);
                for s in [-1.0, 1.0] {
                    let (bx, by) = (64.0 + dx * 0.6 * s, 64.0 + dy * 0.6 * s);
                    let b1 = a + PI * 0.25;
                    let b2 = a - PI * 0.25;
                    bar(c, bx, by, bx + b1.cos() * 14.0 * s, by + b1.sin() * 14.0 * s, 3.0, 0xcfe8f8);
                    bar(c, bx, by, bx + b2.cos() * 14.0 * s, by + b2.sin() * 14.0 * s, 3.0, 0xcfe8f8);
                }
            }
        }
        "pause" => {
            rect(c, 32.0, 24.0, 22.0, 80.0, PARCHMENT);
            rect(c, 74.0, 24.0, 22.0, 80.0, PARCHMENT);
        }
        "play" => tri(c, (36.0, 20.0), (104.0, 64.0), (36.0, 108.0), PARCHMENT),
        "speed2" => {
            tri(c, (16.0, 28.0), (64.0, 64.0), (16.0, 100.0), PARCHMENT);
            tri(c, (62.0, 28.0), (110.0, 64.0), (62.0, 100.0), PARCHMENT);
        }
        "speed4" => {
            for k in 0..3 {
                let x = 6.0 + k as f32 * 38.0;
                tri(c, (x, 32.0), (x + 42.0, 64.0), (x, 96.0), if k == 2 { GOLD_LIGHT } else { PARCHMENT });
            }
        }
        "build" => {
            bar(c, 28.0, 108.0, 84.0, 46.0, 6.0, PLANK);
            shape(c, &[(62.0, 18.0), (110.0, 58.0), (96.0, 72.0), (48.0, 32.0)], 0x8a9098);
        }
        "roads" => {
            shape(c, &[(46.0, 16.0), (82.0, 16.0), (118.0, 112.0), (10.0, 112.0)], 0x9a8a72);
            for k in 0..3 {
                let y = 26.0 + k as f32 * 30.0;
                rect(c, 61.0, y, 6.0 + k as f32 * 2.0, 16.0, 0xf0e6c8);
            }
        }
        "farm" => draw_resource(c, "wheat"),
        "production" => {
            gear(c, 46.0, 50.0, 34.0, 0x8a9098);
            gear(c, 88.0, 86.0, 28.0, 0xb08a4a);
        }
        "military" => {
            shape(c, &[(24.0, 24.0), (104.0, 24.0), (100.0, 74.0), (64.0, 112.0), (28.0, 74.0)], BANNER_RED);
            shape(c, &[(56.0, 24.0), (72.0, 24.0), (72.0, 104.0), (64.0, 112.0), (56.0, 104.0)], 0xe8dcc0);
            bar(c, 24.0, 58.0, 104.0, 58.0, 6.0, 0xe8dcc0);
        }
        "rank" => {
            // Royal crown: gold band with five points, jewels and a red velvet cap.
            blob(c, 64.0, 62.0, 36.0, 24.0, 0x8a1a2a);
            shape(c, &[(18.0, 44.0), (38.0, 66.0), (50.0, 30.0), (64.0, 60.0), (78.0, 30.0), (90.0, 66.0), (110.0, 44.0), (104.0, 96.0), (24.0, 96.0)], GOLD);
            rect(c, 22.0, 92.0, 84.0, 16.0, 0xc8981e);
            for (x, col) in [(40.0, 0xc8202a), (64.0, 0x2a5ac8), (88.0, 0xc8202a)] {
                blob(c, x, 100.0, 5.5, 5.0, col);
            }
            for (x, y) in [(18.0, 42.0), (50.0, 28.0), (78.0, 28.0), (110.0, 42.0)] {
                blob(c, x, y, 5.0, 5.0, GOLD_LIGHT);
            }
            blob(c, 64.0, 22.0, 6.0, 6.0, GOLD_LIGHT);
            bar(c, 64.0, 10.0, 64.0, 20.0, 2.5, GOLD);
            bar(c, 58.0, 15.0, 70.0, 15.0, 2.5, GOLD);
        }
        "research" => {
            shape(c, &[(8.0, 40.0), (62.0, 48.0), (62.0, 112.0), (8.0, 102.0)], 0xf0e6cc);
            shape(c, &[(66.0, 48.0), (120.0, 40.0), (120.0, 102.0), (66.0, 112.0)], 0xe8dcbc);
            for k in 0..4 {
                let y = 60.0 + k as f32 * 11.0;
                bar(c, 16.0, y, 54.0, y + 4.0, 1.6, 0x8a7a5a);
                bar(c, 74.0, y + 4.0, 112.0, y, 1.6, 0x8a7a5a);
            }
            bar(c, 92.0, 8.0, 64.0, 50.0, 3.0, 0xf4f0e8);
            shape(c, &[(92.0, 6.0), (110.0, 14.0), (74.0, 42.0)], 0xe8e2d4);
        }
        "events" => {
            scroll(c, 22.0, 14.0, 84.0, 100.0);
            rect(c, 58.0, 34.0, 12.0, 44.0, BANNER_RED);
            blob(c, 64.0, 92.0, 7.0, 7.0, BANNER_RED);
        }
        "notifications" => {
            shape(c, &[(64.0, 16.0), (90.0, 30.0), (96.0, 80.0), (110.0, 96.0), (18.0, 96.0), (32.0, 80.0), (38.0, 30.0)], GOLD);
            blob(c, 64.0, 106.0, 12.0, 10.0, 0xa87a18);
            blob(c, 64.0, 14.0, 7.0, 7.0, GOLD);
        }
        "tasks" => {
            rect(c, 22.0, 14.0, 84.0, 102.0, PARCHMENT);
            for k in 0..3 {
                let y = 34.0 + k as f32 * 26.0;
                rect(c, 32.0, y, 14.0, 14.0, 0xf8f0dc);
                bar(c, 56.0, y + 7.0, 94.0, y + 7.0, 2.5, 0x8a7a5a);
                if k < 2 {
                    bar(c, 33.0, y + 7.0, 39.0, y + 13.0, 3.0, 0x3a8a3a);
                    bar(c, 39.0, y + 13.0, 50.0, y - 2.0, 3.0, 0x3a8a3a);
                }
            }
        }
        "overview" => {
            shape(c, &[(10.0, 30.0), (44.0, 20.0), (84.0, 30.0), (118.0, 20.0), (118.0, 98.0), (84.0, 108.0), (44.0, 98.0), (10.0, 108.0)], PARCHMENT);
            bar(c, 44.0, 20.0, 44.0, 98.0, 1.5, 0xb8a070);
            bar(c, 84.0, 30.0, 84.0, 108.0, 1.5, 0xb8a070);
            for k in 0..6 {
                let t = k as f32 / 5.0;
                blob(c, 24.0 + t * 70.0, 90.0 - t * 40.0 + (t * 9.0).sin() * 8.0, 3.0, 3.0, 0x8a5a3a);
            }
            bar(c, 90.0, 40.0, 104.0, 54.0, 3.5, BANNER_RED);
            bar(c, 104.0, 40.0, 90.0, 54.0, 3.5, BANNER_RED);
        }
        "demolish" => {
            for &(x, y) in &[(20.0, 80.0), (60.0, 80.0), (40.0, 58.0)] {
                rect(c, x, y, 38.0, 20.0, STONE);
            }
            bar(c, 64.0, 20.0, 108.0, 112.0, 5.0, PLANK);
            shape(c, &[(36.0, 30.0), (70.0, 8.0), (108.0, 22.0), (72.0, 26.0)], 0x8a9098);
        }
        "upgrade" => {
            shape(c, &[(64.0, 12.0), (112.0, 62.0), (84.0, 62.0), (84.0, 114.0), (44.0, 114.0), (44.0, 62.0), (16.0, 62.0)], 0x4aa84a);
        }
        "rotate" => {
            arc(c, 64.0, 66.0, 38.0, (-PI * 0.15, PI * 1.45), 7.0, PARCHMENT);
            let a = -PI * 0.15;
            let (x, y) = (64.0 + a.cos() * 38.0, 66.0 + a.sin() * 38.0);
            shape(c, &[(x - 18.0, y - 2.0), (x + 18.0, y - 2.0), (x, y + 22.0)], PARCHMENT);
        }
        "move" => {
            for &(dx, dy) in &[(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)] {
                arrow(c, 64.0, 64.0, 64.0 + dx * 54.0, 64.0 + dy * 54.0, 5.0, PARCHMENT);
            }
        }
        "info" => {
            blob(c, 64.0, 64.0, 52.0, 52.0, 0x3a6ab0);
            rect(c, 56.0, 54.0, 16.0, 46.0, 0xf8f4ec);
            blob(c, 64.0, 38.0, 9.0, 9.0, 0xf8f4ec);
        }
        "close" => {
            bar(c, 28.0, 28.0, 100.0, 100.0, 11.0, 0xc8302a);
            bar(c, 100.0, 28.0, 28.0, 100.0, 11.0, 0xc8302a);
        }
        "warning" => {
            shape(c, &[(64.0, 10.0), (122.0, 112.0), (6.0, 112.0)], 0xf4c030);
            rect(c, 57.0, 42.0, 14.0, 40.0, 0x2a1a10);
            blob(c, 64.0, 96.0, 8.0, 8.0, 0x2a1a10);
        }
        "trade" => {
            bar(c, 64.0, 18.0, 64.0, 108.0, 4.0, 0x8a6a3a);
            bar(c, 20.0, 34.0, 108.0, 34.0, 4.0, GOLD);
            rect(c, 40.0, 104.0, 48.0, 10.0, 0x8a6a3a);
            for &x in &[28.0, 100.0] {
                bar(c, x, 34.0, x - 16.0, 74.0, 1.5, 0x6a5a40);
                bar(c, x, 34.0, x + 16.0, 74.0, 1.5, 0x6a5a40);
                shape(c, &[(x - 22.0, 74.0), (x + 22.0, 74.0), (x + 14.0, 86.0), (x - 14.0, 86.0)], GOLD);
            }
            blob(c, 64.0, 18.0, 8.0, 8.0, GOLD);
        }
        "diplomacy" => {
            scroll(c, 14.0, 20.0, 90.0, 88.0);
            for k in 0..4 {
                let y = 40.0 + k as f32 * 12.0;
                bar(c, 30.0, y, 86.0, y, 2.0, 0x8a7a5a);
            }
            blob(c, 92.0, 90.0, 18.0, 18.0, 0xb8202a);
            ring(c, 92.0, 90.0, 11.0, 8.0, rgba(0x8a1018, 255));
            bar(c, 86.0, 104.0, 80.0, 122.0, 4.0, 0xb8202a);
            bar(c, 98.0, 104.0, 104.0, 122.0, 4.0, 0xb8202a);
        }
        "finance" => {
            sack(c, 60.0, 22.0, 84.0, 92.0, 0x9a7a4a);
            coin(c, 60.0, 78.0, 18.0);
            coin(c, 100.0, 100.0, 18.0);
        }
        "statistics" => {
            rect(c, 18.0, 70.0, 22.0, 44.0, 0x3a6ab0);
            rect(c, 52.0, 44.0, 22.0, 70.0, 0x4aa84a);
            rect(c, 86.0, 24.0, 22.0, 90.0, 0xd8a030);
            bar(c, 12.0, 116.0, 116.0, 116.0, 3.0, 0x3a2a1a);
        }
        "storage" => {
            shape(c, &[(16.0, 46.0), (64.0, 26.0), (112.0, 46.0), (64.0, 66.0)], PLANK_LIGHT);
            shape(c, &[(16.0, 46.0), (64.0, 66.0), (64.0, 116.0), (16.0, 96.0)], PLANK);
            shape(c, &[(64.0, 66.0), (112.0, 46.0), (112.0, 96.0), (64.0, 116.0)], 0x6a4a2c);
            bar(c, 18.0, 50.0, 62.0, 112.0, 3.0, 0x5a3a20);
            bar(c, 110.0, 50.0, 66.0, 112.0, 3.0, 0x4a2a14);
        }
        "settings" => gear(c, 64.0, 64.0, 52.0, 0x9aa0a8),
        "save" => {
            book(c, 22.0, 14.0, 70.0, 92.0, 0x7a2a2a);
            arrow(c, 96.0, 50.0, 96.0, 120.0, 6.0, 0x3a9a3a);
        }
        "load" => {
            book(c, 22.0, 14.0, 70.0, 92.0, 0x2a4a7a);
            arrow(c, 96.0, 120.0, 96.0, 48.0, 6.0, 0x3a7ac8);
        }
        "inspect" => {
            // Magnifying glass over a small pointer arrow.
            shape(c, &[(14.0, 12.0), (52.0, 46.0), (36.0, 50.0), (46.0, 72.0), (36.0, 76.0), (26.0, 54.0), (14.0, 64.0)], PARCHMENT);
            bar(c, 92.0, 92.0, 116.0, 116.0, 9.0, PLANK);
            ring(c, 76.0, 76.0, 32.0, 23.0, rgba(0x8a9098, 255));
            blob(c, 76.0, 76.0, 23.0, 23.0, 0xb8d8e8);
            blob(c, 68.0, 68.0, 7.0, 6.0, 0xf4f8fc);
        }
        "grid" => {
            shape(c, &[(64.0, 14.0), (120.0, 64.0), (64.0, 114.0), (8.0, 64.0)], 0x6e9a3f);
            for k in 1..4 {
                let t = k as f32 / 4.0;
                bar(c, 64.0 - 56.0 * t, 14.0 + 50.0 * t, 120.0 - 56.0 * t, 64.0 + 50.0 * t, 2.2, 0xf0e6c8);
                bar(c, 64.0 + 56.0 * t, 14.0 + 50.0 * t, 8.0 + 56.0 * t, 64.0 + 50.0 * t, 2.2, 0xf0e6c8);
            }
        }
        "territory" => {
            shape(c, &[(64.0, 34.0), (120.0, 74.0), (64.0, 114.0), (8.0, 74.0)], 0x6e9a3f);
            for k in 0..8 {
                let t0 = k as f32 / 8.0;
                let t1 = t0 + 0.07;
                let (x0, y0) = (8.0 + 56.0 * t0, 74.0 + 40.0 * t0);
                let (x1, y1) = (8.0 + 56.0 * t1, 74.0 + 40.0 * t1);
                bar(c, x0, y0, x1, y1, 3.0, GOLD_LIGHT);
                bar(c, 120.0 - 56.0 * t0, 74.0 + 40.0 * t0, 120.0 - 56.0 * t1, 74.0 + 40.0 * t1, 3.0, GOLD_LIGHT);
            }
            bar(c, 60.0, 8.0, 60.0, 80.0, 3.5, PLANK);
            shape(c, &[(62.0, 10.0), (100.0, 22.0), (62.0, 36.0)], BANNER_RED);
        }
        "economy" => {
            // Goods crate and grain sack with a rising and a falling arrow.
            sack(c, 40.0, 50.0, 46.0, 62.0, 0xc8a46a);
            rect(c, 66.0, 72.0, 46.0, 40.0, PLANK);
            bar(c, 66.0, 92.0, 112.0, 92.0, 2.5, 0x5a3a20);
            arrow(c, 30.0, 44.0, 30.0, 8.0, 6.0, 0x3a9a3a);
            arrow(c, 96.0, 10.0, 96.0, 58.0, 6.0, 0xc8302a);
        }
        "medicine" => {
            // Mortar with pestle and a healing herb.
            shape(c, &[(22.0, 62.0), (106.0, 62.0), (92.0, 108.0), (36.0, 108.0)], 0x9c968b);
            rect(c, 18.0, 56.0, 92.0, 10.0, 0xb3a78e);
            bar(c, 70.0, 60.0, 104.0, 14.0, 7.0, PLANK_LIGHT);
            bar(c, 40.0, 58.0, 30.0, 18.0, 2.5, 0x3a7a2a);
            blob(c, 26.0, 26.0, 12.0, 7.0, 0x5aa84a);
            blob(c, 44.0, 34.0, 11.0, 6.0, 0x5aa84a);
        }
        "administration" => {
            // Sealed charter with a quill.
            scroll(c, 14.0, 18.0, 84.0, 96.0);
            for k in 0..4 {
                let y = 38.0 + k as f32 * 13.0;
                bar(c, 30.0, y, 82.0, y, 2.0, 0x8a7a5a);
            }
            blob(c, 56.0, 98.0, 13.0, 13.0, 0xb8202a);
            bar(c, 112.0, 10.0, 78.0, 86.0, 3.0, 0xf4f0e8);
            shape(c, &[(112.0, 8.0), (122.0, 22.0), (88.0, 64.0)], 0xe8e2d4);
        }
        _ => ring(c, 64.0, 64.0, 40.0, 30.0, rgb(0xff00ff)),
    }
}

// --- Crests -------------------------------------------------------------------------------------

/// Width and height of a crest.
pub const CREST: (u32, u32) = (40, 48);

/// Crest of the kingdom with colour index `index`: a heater shield in the kingdom colour with a
/// gold rim and a silver charge of its own (tower, star, chevron, crown, cross, crescent, three
/// roundels), drawn 4× supersampled.
pub fn crest(index: usize) -> Canvas {
    let (w, h) = (CREST.0 * SS, CREST.1 * SS);
    let mut c = Canvas::new(w, h);
    let field = KINGDOM_COLORS[index % KINGDOM_COLORS.len()];
    let (fw, fh) = (w as f32, h as f32);
    let shield = |inset: f32| {
        vec![
            (inset, inset),
            (fw - inset, inset),
            (fw - inset, fh * 0.52),
            (fw * 0.5, fh - inset),
            (inset, fh * 0.52),
        ]
    };
    shape(&mut c, &shield(6.0), GOLD);
    shape(&mut c, &shield(16.0), field);
    let silver = 0xeeeae2;
    let (cx, cy) = (fw * 0.5, fh * 0.42);
    match index % 7 {
        0 => {
            // Tower with battlements and a dark gate.
            rect(&mut c, cx - 24.0, cy - 22.0, 48.0, 58.0, silver);
            for k in 0..3 {
                rect(&mut c, cx - 28.0 + k as f32 * 22.0, cy - 36.0, 12.0, 16.0, silver);
            }
            rect(&mut c, cx - 9.0, cy + 14.0, 18.0, 22.0, 0x3a2a1a);
        }
        1 => {
            let pts: Vec<(f32, f32)> = (0..10)
                .map(|i| {
                    let a = -PI / 2.0 + TAU * i as f32 / 10.0;
                    let r = if i % 2 == 0 { 44.0 } else { 18.0 };
                    (cx + a.cos() * r, cy + 6.0 + a.sin() * r)
                })
                .collect();
            shape(&mut c, &pts, silver);
        }
        2 => {
            shape(&mut c, &[(cx - 54.0, cy + 34.0), (cx, cy - 26.0), (cx + 54.0, cy + 34.0), (cx + 54.0, cy + 58.0), (cx, cy - 2.0), (cx - 54.0, cy + 58.0)], silver);
        }
        3 => {
            shape(&mut c, &[(cx - 40.0, cy - 18.0), (cx - 22.0, cy + 2.0), (cx, cy - 30.0), (cx + 22.0, cy + 2.0), (cx + 40.0, cy - 18.0), (cx + 34.0, cy + 30.0), (cx - 34.0, cy + 30.0)], GOLD_LIGHT);
            rect(&mut c, cx - 36.0, cy + 26.0, 72.0, 12.0, silver);
        }
        4 => {
            bar(&mut c, cx, cy - 44.0, cx, cy + 56.0, 9.0, silver);
            bar(&mut c, cx - 44.0, cy, cx + 44.0, cy, 9.0, silver);
        }
        5 => {
            blob(&mut c, cx, cy + 8.0, 40.0, 40.0, silver);
            c.fill_ellipse(cx + 16.0, cy - 2.0, 34.0, 34.0, |_, _, _| rgb(field));
        }
        _ => {
            for (dx, dy) in [(-24.0, -14.0), (24.0, -14.0), (0.0, 28.0)] {
                blob(&mut c, cx + dx, cy + dy, 17.0, 17.0, silver);
            }
        }
    }
    c.outlined(5, rgba(OUTLINE, 255)).downsample(SS)
}

// --- Buildings -----------------------------------------------------------------------------------

/// Badge colour behind building icons, per build-menu category.
fn category_tint(c: Category) -> u32 {
    match c {
        Category::Residential => 0x5a8a3a,
        Category::Production => 0x8a5a2a,
        Category::Agriculture => 0xc8a030,
        Category::Public => 0x3a6ab0,
        Category::Military => 0x9b2d2d,
    }
}

/// Building icon: the finished sprite scaled into a 48 px square over a soft badge tinted by
/// the building's category.
pub fn building(info: &BuildingInfo) -> Option<Canvas> {
    let mut sprite = match info.id {
        "wall" => crate::roads::wall(crate::roads::E | crate::roads::W),
        "gate" => crate::roads::gate(crate::iso::Axis::A),
        id => crate::buildings::render(id)?.0,
    };
    // Snow marks are for the map's shader only.
    crate::iso::clear_snow_marks(&mut sprite);
    let mut out = Canvas::new(LARGE, LARGE);
    let half = LARGE as f32 * 0.5;
    let tint = category_tint(info.category);
    out.fill_ellipse(half, half + 2.0, half - 1.0, half - 5.0, |_, _, d| rgba(tint, ((1.0 - d).sqrt() * 120.0) as u8));
    out.blit(&sprite.fit_square(LARGE, 2), 0, 0);
    Some(out)
}

/// Assembles the `icons` atlas.
pub fn atlas() -> AtlasBuilder {
    let mut atlas = AtlasBuilder::new("icons", Sampling::Smooth);
    for id in catalog::RESOURCES {
        atlas.add(format!("res_{id}"), resource(id), (0.5, 0.5));
    }
    for info in catalog::BUILDINGS {
        if let Some(c) = building(info) {
            atlas.add(format!("bld_{}", info.id), c, (0.5, 0.5));
        }
    }
    for name in UI_ICONS {
        atlas.add(format!("ui_{name}"), ui(name), (0.5, 0.5));
    }
    for i in 0..KINGDOM_COLORS.len() {
        atlas.add(format!("crest_{i}"), crest(i), (0.5, 0.5));
    }
    atlas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_resource_has_a_distinct_pictogram() {
        let mut seen: Vec<Vec<u8>> = Vec::new();
        for id in catalog::RESOURCES {
            let c = resource(id);
            assert_eq!((c.width(), c.height()), (32, 32));
            assert!(c.coverage() > 0.15, "{id} is too empty");
            // No fallback magenta.
            assert!(!c.img.pixels().any(|p| p.0[0] > 240 && p.0[1] < 20 && p.0[2] > 240), "{id} uses the fallback");
            let bytes = c.img.as_raw().clone();
            assert!(!seen.contains(&bytes), "{id} duplicates another icon");
            seen.push(bytes);
        }
    }

    #[test]
    fn every_ui_icon_renders() {
        for name in UI_ICONS {
            let c = ui(name);
            assert_eq!((c.width(), c.height()), (32, 32));
            assert!(c.coverage() > 0.1, "{name} is too empty");
            assert!(!c.img.pixels().any(|p| p.0[0] > 240 && p.0[1] < 20 && p.0[2] > 240), "{name} uses the fallback");
        }
    }

    #[test]
    fn crests_show_the_kingdom_colours() {
        let mut seen: Vec<Vec<u8>> = Vec::new();
        for (i, color) in KINGDOM_COLORS.iter().enumerate() {
            let c = crest(i);
            assert_eq!((c.width() as u32, c.height() as u32), CREST);
            // The field left of the charge carries the kingdom colour.
            let p = c.get(8, 14);
            let [r, g, b] = [(color >> 16) as u8, (color >> 8) as u8, *color as u8];
            let close = |a: u8, b: u8| (i32::from(a) - i32::from(b)).abs() <= 40;
            assert!(close(p[0], r) && close(p[1], g) && close(p[2], b), "crest {i}: {p:?} vs {color:06x}");
            let bytes = c.img.as_raw().clone();
            assert!(!seen.contains(&bytes), "crest {i} duplicates another");
            seen.push(bytes);
        }
    }

    #[test]
    fn building_icons_cover_all_ids() {
        for info in catalog::BUILDINGS {
            let c = building(info).unwrap_or_else(|| panic!("no icon for {}", info.id));
            assert_eq!((c.width(), c.height()), (48, 48));
            assert!(c.coverage() > 0.2, "{} icon too small", info.id);
        }
    }
}
