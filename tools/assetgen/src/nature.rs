//! Vegetation, boulders and deposit markers (`nature` atlas).
//!
//! Trees keep the same silhouette across seasons (the seed depends only on kind and growth
//! stage), so switching seasons only recolours them: fresh green in spring, deep green in
//! summer, warm colours in autumn, bare branches (deciduous) or snow-dusted tiers (conifers)
//! in winter. Light comes from the top-left like everywhere else.

use std::f32::consts::{PI, TAU};

use kingdoms_engine::rendering::texture::Sampling;

use crate::atlas::AtlasBuilder;
use crate::draw::{Canvas, Px, hash01, mix, rgb, rgba, shade, value_noise, with_alpha};
use crate::palette;

/// Tree species (sprite name prefixes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tree {
    Oak,
    Pine,
    Birch,
    Willow,
}

impl Tree {
    pub const ALL: [Tree; 4] = [Tree::Oak, Tree::Pine, Tree::Birch, Tree::Willow];

    pub fn name(self) -> &'static str {
        match self {
            Tree::Oak => "oak",
            Tree::Pine => "pine",
            Tree::Birch => "birch",
            Tree::Willow => "willow",
        }
    }

    /// Mature height and crown half-width in pixels.
    fn dims(self) -> (f32, f32) {
        match self {
            Tree::Oak => (64.0, 23.0),
            Tree::Pine => (74.0, 15.0),
            Tree::Birch => (62.0, 15.0),
            Tree::Willow => (58.0, 27.0),
        }
    }
}

/// Seasons (sprite name parts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];

    pub fn name(self) -> &'static str {
        match self {
            Season::Spring => "spring",
            Season::Summer => "summer",
            Season::Autumn => "autumn",
            Season::Winter => "winter",
        }
    }
}

/// Growth stages with their scale relative to a mature tree.
pub const STAGES: [(&str, f32); 3] = [("sapling", 0.34), ("young", 0.64), ("mature", 1.0)];

/// Deposit kinds (`deposit_<kind>`), matching `DepositKind` in the simulation.
pub const DEPOSITS: [&str; 7] = ["stone", "iron", "coal", "clay", "sand", "gold", "fish"];

const SNOW: u32 = 0xf1f5f8;

/// Dark and light foliage colour, `None` for bare (winter deciduous) trees.
fn leaf_colors(kind: Tree, season: Season) -> Option<(Px, Px)> {
    let c = |d: u32, l: u32| Some((rgb(d), rgb(l)));
    match (kind, season) {
        (Tree::Pine, Season::Spring) => c(0x2a5030, 0x6a9c4c),
        (Tree::Pine, Season::Summer) => c(0x23452a, 0x4f7f45),
        (Tree::Pine, Season::Autumn) => c(0x26442a, 0x4b733f),
        (Tree::Pine, Season::Winter) => c(0x203c2a, 0x426a40),
        (_, Season::Winter) => None,
        (Tree::Oak, Season::Spring) => c(0x5a8c2c, 0xaad45c),
        (Tree::Oak, Season::Summer) => c(0x32602a, 0x6f9e3c),
        (Tree::Oak, Season::Autumn) => c(0x8a3a12, 0xe39a30),
        (Tree::Birch, Season::Spring) => c(0x669a30, 0xbbe06c),
        (Tree::Birch, Season::Summer) => c(0x467a2c, 0x88b64a),
        (Tree::Birch, Season::Autumn) => c(0xb07e16, 0xf4d24c),
        (Tree::Willow, Season::Spring) => c(0x7fa03a, 0xc4dc6c),
        (Tree::Willow, Season::Summer) => c(0x55843a, 0x9cc25a),
        (Tree::Willow, Season::Autumn) => c(0x9c8c30, 0xdccb5c),
    }
}

/// One foliage clump.
#[derive(Clone, Copy)]
struct Clump {
    x: f32,
    y: f32,
    r: f32,
}

/// Draws a ragged, top-left-lit foliage clump. `crown` = (centre x, centre y, rx, ry) of the
/// whole crown for global shading.
fn draw_clump(c: &mut Canvas, k: Clump, dark: Px, light: Px, crown: (f32, f32, f32, f32), seed: u32, raggedness: f32) {
    let gl = 0.5 + 0.5 * (-(k.x - crown.0) / crown.2 * 0.5 - (k.y - crown.1) / crown.3 * 0.7).clamp(-1.0, 1.0);
    c.fill_ellipse(k.x, k.y, k.r, k.r * 0.92, |x, y, d| {
        if d > 1.0 - raggedness && hash01(x, y, seed) > 0.5 {
            return [0, 0, 0, 0];
        }
        let nx = (x as f32 + 0.5 - k.x) / k.r;
        let ny = (y as f32 + 0.5 - k.y) / k.r;
        let l = 0.5 + 0.5 * (-0.6 * nx - 0.8 * ny);
        let tex = (hash01(x.div_euclid(2), y.div_euclid(2), seed + 1) - 0.5) * 0.28;
        mix(dark, light, (0.55 * l + 0.5 * gl + tex - 0.3 * d * d).clamp(0.0, 1.0))
    });
}

/// Branch/limb with round caps, lit on its left side.
fn limb(c: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, col: Px) {
    c.thick_line(x0, y0, x1, y1, r.max(0.55), col);
    if r > 1.2 {
        // Highlight on the lit side.
        c.thick_line(x0 - r * 0.45, y0, x1 - r * 0.45, y1, (r * 0.35).max(0.5), shade(col, 1.25));
    }
}

/// Tapered trunk from (x, y) up to `top_y`, lit from the left.
fn trunk(c: &mut Canvas, x: f32, y: f32, top_y: f32, w_bottom: f32, w_top: f32, col: Px) {
    let pts = [
        (x - w_bottom * 0.5 - 1.0, y),
        (x + w_bottom * 0.5 + 1.0, y),
        (x + w_bottom * 0.5, y - 2.0),
        (x + w_top * 0.5, top_y),
        (x - w_top * 0.5, top_y),
        (x - w_bottom * 0.5, y - 2.0),
    ];
    c.fill_polygon(&pts, |px, py| {
        let t = ((py as f32 - top_y) / (y - top_y)).clamp(0.0, 1.0);
        let w = w_top + (w_bottom - w_top) * t;
        let u = (px as f32 + 0.5 - (x - w * 0.5)) / w.max(1.0);
        let s = if u < 0.4 { 1.12 } else if u > 0.75 { 0.74 } else { 0.92 };
        shade(col, s * (0.94 + 0.1 * hash01(px, py.div_euclid(2), 9)))
    });
}

/// Adds snow on upward-facing edges of opaque pixels inside `x0..x1`.
fn snow_dust(c: &mut Canvas, prob: f32, seed: u32) {
    let src = c.clone();
    for y in 1..c.height() {
        for x in 0..c.width() {
            let p = src.get(x, y);
            if p[3] > 128 && src.get(x, y - 1)[3] < 60 && hash01(x, y, seed) < prob {
                c.set(x, y, rgb(SNOW));
                if hash01(x, y, seed + 1) < 0.5 && src.get(x, y + 1)[3] > 128 {
                    c.set(x, y + 1, shade(rgb(SNOW), 0.88));
                }
            }
        }
    }
}

/// Draws a tree with its trunk base centred at (x, y) (`y` is the bottom edge of the trunk).
pub fn draw_tree(c: &mut Canvas, x: f32, y: f32, kind: Tree, season: Season, scale: f32, seed: u32) {
    match kind {
        Tree::Pine => pine(c, x, y, season, scale, seed),
        Tree::Willow => willow(c, x, y, season, scale, seed),
        Tree::Oak | Tree::Birch => deciduous(c, x, y, kind, season, scale, seed),
    }
}

fn deciduous(c: &mut Canvas, x: f32, y: f32, kind: Tree, season: Season, s: f32, seed: u32) {
    let (h, rw) = kind.dims();
    let (h, rx) = (h * s, rw * s);
    let birch = kind == Tree::Birch;
    let trunk_h = h * if birch { 0.55 } else { 0.42 };
    let ry = (h - trunk_h * 0.75) * 0.5;
    let (ccx, ccy) = (x, y - h + ry);
    let bark = if birch { rgb(0xe8e3d6) } else { rgb(palette::BARK) };
    let branch_col = if birch { rgb(0x6b5a4a) } else { shade(rgb(palette::BARK), 0.95) };

    // Crown layout: one central clump plus a ring.
    let ring = if birch { 9 } else { 7 };
    let mut clumps = vec![Clump { x: ccx, y: ccy, r: rx * if birch { 0.42 } else { 0.55 } }];
    for i in 0..ring {
        let t = -PI * 0.5 + i as f32 * TAU / ring as f32 + (hash01(i, 1, seed) - 0.5) * 0.5;
        let rr = 0.52 + 0.2 * hash01(i, 2, seed);
        let r = rx * if birch { 0.3 + 0.1 * hash01(i, 3, seed) } else { 0.42 + 0.12 * hash01(i, 3, seed) };
        clumps.push(Clump { x: ccx + t.cos() * rx * rr, y: ccy + t.sin() * ry * rr * 0.95, r });
    }

    // Skeleton: trunk, limbs to the ring clumps, twigs beyond them.
    let w_bottom = if birch { (3.4 * s).max(1.5) } else { (6.0 * s).max(2.0) };
    trunk(c, x, y, y - trunk_h, w_bottom, w_bottom * 0.6, bark);
    if birch {
        // Dark birch marks.
        for i in 0..((trunk_h / 3.0) as i32) {
            if hash01(i, 7, seed) > 0.45 {
                let yy = y - 2.0 - i as f32 * 3.0;
                let ww = (w_bottom * (0.4 + 0.5 * hash01(i, 8, seed))).max(1.0);
                c.fill_rect((x - w_bottom * 0.5 + hash01(i, 9, seed) * 1.5) as i32, yy as i32, ww as i32, 1, rgb(0x2b2622));
            }
        }
    }
    let top = (x, y - trunk_h);
    for (i, k) in clumps.iter().enumerate().skip(1) {
        let mid = (top.0 + (k.x - top.0) * 0.45, top.1 + (k.y - top.1) * 0.6);
        let r0 = (w_bottom * 0.3).max(0.6);
        limb(c, top.0, top.1, mid.0, mid.1, r0, branch_col);
        limb(c, mid.0, mid.1, k.x, k.y, (r0 * 0.6).max(0.55), branch_col);
        let away = (k.x - ccx).atan2(-(k.y - ccy));
        for j in 0..2 {
            let a = away + (j as f32 - 0.5) * 0.9 + (hash01(i as i32, j, seed + 5) - 0.5) * 0.4;
            let l = k.r * (0.8 + 0.4 * hash01(i as i32, j + 2, seed));
            let droop = if birch { 2.0 * s } else { 0.0 };
            c.thick_line(k.x, k.y, k.x + a.sin() * l, k.y - a.cos() * l + droop, 0.55, branch_col);
        }
    }

    match leaf_colors(kind, season) {
        Some((dark, light)) => {
            let crown = (ccx, ccy, rx, ry);
            let mut order: Vec<Clump> = clumps.clone();
            order.sort_by(|a, b| (b.y + 0.5 * b.x).total_cmp(&(a.y + 0.5 * a.x)));
            for (i, k) in order.iter().enumerate() {
                let (mut d, mut l) = (dark, light);
                if season == Season::Autumn {
                    let t = hash01(i as i32, 11, seed);
                    let alt = if birch { rgb(0xd9a524) } else if t > 0.5 { rgb(0xc8552a) } else { rgb(0xd9b13a) };
                    l = mix(l, alt, 0.45 * t);
                    d = mix(d, shade(alt, 0.6), 0.3 * t);
                }
                draw_clump(c, *k, d, l, crown, seed + i as u32 * 17, if birch { 0.4 } else { 0.28 });
            }
            if season == Season::Spring && kind == Tree::Oak {
                for i in 0..(14.0 * s) as i32 {
                    let px = ccx + (hash01(i, 21, seed) - 0.5) * rx * 1.6;
                    let py = ccy + (hash01(i, 22, seed) - 0.5) * ry * 1.4;
                    if c.get(px as i32, py as i32)[3] > 200 {
                        c.set(px as i32, py as i32, rgb(0xe4f2b8));
                    }
                }
            }
        }
        None => snow_dust(c, 0.55, seed + 77),
    }
}

fn willow(c: &mut Canvas, x: f32, y: f32, season: Season, s: f32, seed: u32) {
    let (h, rw) = Tree::Willow.dims();
    let (h, rx) = (h * s, rw * s);
    let trunk_h = h * 0.4;
    let ry = h * 0.22;
    let (ccx, ccy) = (x, y - h + ry);
    let bark = shade(rgb(palette::BARK), 0.9);
    let w_bottom = (7.0 * s).max(2.0);
    trunk(c, x, y, y - trunk_h, w_bottom, w_bottom * 0.7, bark);
    // Main limbs fanning out into the dome.
    let limbs = 5;
    let mut ends = Vec::new();
    for i in 0..limbs {
        let t = -PI + PI * (i as f32 + 0.5) / limbs as f32;
        let ex = ccx + t.cos() * rx * 0.75;
        let ey = ccy + t.sin() * ry * 0.7 + ry * 0.2;
        limb(c, x, y - trunk_h, ex, ey, (w_bottom * 0.25).max(0.6), bark);
        ends.push((ex, ey));
    }
    let leaves = leaf_colors(Tree::Willow, season);
    let crown = (ccx, ccy, rx, ry);
    if let Some((dark, light)) = leaves {
        for i in 0..7 {
            let t = -PI + PI * i as f32 / 6.0;
            let k = Clump { x: ccx + t.cos() * rx * 0.62, y: ccy + t.sin() * ry * 0.6, r: rx * 0.38 };
            draw_clump(c, k, dark, light, crown, seed + i as u32 * 13, 0.3);
        }
        draw_clump(c, Clump { x: ccx, y: ccy - ry * 0.1, r: rx * 0.45 }, dark, light, crown, seed + 99, 0.25);
    }
    // Hanging curtains (leafy strands, or bare twigs in winter).
    let bottom_limit = y - 6.0 * s;
    let step = if leaves.is_some() { 1 } else { 2 };
    let x0 = (ccx - rx) as i32;
    let x1 = (ccx + rx) as i32;
    for px in (x0..=x1).step_by(step) {
        let dx = (px as f32 + 0.5 - ccx) / rx;
        if dx.abs() > 0.98 {
            continue;
        }
        let start = ccy + ry * 0.35 * (1.0 - dx * dx).sqrt() - 2.0;
        let len = (0.45 + 0.4 * hash01(px, 3, seed)) * (bottom_limit - start);
        let lit = 0.5 + 0.5 * (-dx);
        // Every few columns a darker gap suggests separate strands without opening holes.
        let strand = if hash01(px, 5, seed) > 0.7 { 0.78 } else { 1.0 };
        for j in 0..(len as i32) {
            let yy = start + j as f32;
            let col = match leaves {
                Some((dark, light)) => {
                    let t = (lit * 0.6 + 0.3 * hash01(px, j / 3, seed + 4)).clamp(0.0, 1.0);
                    shade(mix(dark, light, t * (1.0 - 0.35 * j as f32 / len.max(1.0))), strand)
                }
                None => with_alpha(rgb(0x8a7a3a), 220),
            };
            c.blend(px, yy as i32, col);
        }
    }
    if leaves.is_none() {
        snow_dust(c, 0.35, seed + 3);
    }
}

fn pine(c: &mut Canvas, x: f32, y: f32, season: Season, s: f32, seed: u32) {
    let (h, rw) = Tree::Pine.dims();
    let (h, rw) = (h * s, rw * s);
    let bark = rgb(palette::BARK);
    let trunk_top = y - 14.0 * s;
    trunk(c, x, y, trunk_top - 4.0, (4.0 * s).max(2.0), (2.5 * s).max(1.5), bark);
    let Some((dark, light)) = leaf_colors(Tree::Pine, season) else {
        return;
    };
    let n = if s < 0.5 { 3 } else { 5 };
    let (top, bottom) = (y - h, y - 8.0 * s);
    let span = bottom - top;
    let tier_h = span * 0.42;
    let step = (span - tier_h) / (n - 1) as f32;
    for i in 0..n {
        let base = bottom - i as f32 * step;
        let apex = base - tier_h;
        let hw = rw * (1.0 - 0.72 * i as f32 / (n - 1) as f32);
        let saw = 2.0 * s.max(0.5);
        let pts = [(x, apex), (x + hw, base), (x - hw, base)];
        let tier = i;
        c.fill_polygon(&pts, |px, py| {
            let dx = (px as f32 + 0.5 - x) / hw.max(1.0);
            let yy = py as f32 + 0.5;
            let jag = saw * ((px as f32 * 0.9 + tier as f32).sin() * 0.5 + 0.5);
            if yy > base - jag {
                return [0, 0, 0, 0];
            }
            let rel = (yy - apex) / tier_h;
            let edge_dist = (rel - dx.abs()) * tier_h; // ~ px below the sloped side
            let mut l = 0.55 - dx * 0.4 + (1.0 - rel) * 0.15;
            l += (hash01(px, py, seed + tier as u32) - 0.5) * 0.25;
            if yy > base - jag - 2.0 * s.max(0.5) {
                l -= 0.18;
            }
            let mut col = mix(dark, light, l.clamp(0.0, 1.0));
            match season {
                Season::Winter => {
                    if edge_dist < 1.8 && hash01(px, py, seed + 31) > 0.15 {
                        col = shade(rgb(SNOW), if dx > 0.2 { 0.86 } else { 1.0 });
                    } else if dx < 0.3 && hash01(px, py, seed + 33) > 0.82 {
                        col = shade(rgb(SNOW), 0.95);
                    }
                }
                Season::Spring if yy > base - jag - 1.5 && hash01(px, py, seed + 35) > 0.4 => col = rgb(0x9fd06a),
                _ => {}
            }
            col
        });
    }
}

/// Small fruit tree used by orchards; `stage` 0 = staked sapling … 4 = bearing fruit.
pub fn draw_fruit_tree(c: &mut Canvas, x: f32, y: f32, stage: u32, seed: u32) {
    let bark = rgb(palette::BARK);
    if stage == 0 {
        c.fill_rect(x as i32 + 2, (y - 10.0) as i32, 1, 10, rgb(palette::PLANK));
        trunk(c, x, y, y - 7.0, 1.6, 1.0, bark);
        draw_clump(c, Clump { x, y: y - 8.0, r: 2.6 }, rgb(0x4f8a2c), rgb(0x9ccc58), (x, y - 8.0, 3.0, 3.0), seed, 0.2);
        return;
    }
    let s = match stage {
        1 => 0.55,
        2 => 0.8,
        _ => 1.0,
    };
    let h = 26.0 * s;
    let r = 9.0 * s;
    trunk(c, x, y, y - h * 0.45, 3.0 * s + 0.8, 2.0 * s, bark);
    let (cx, cy) = (x, y - h + r);
    let crown = (cx, cy, r * 1.4, r);
    let (dark, light) = (rgb(0x3a6c2a), rgb(0x7fb04a));
    let ks = [
        Clump { x: cx + r * 0.5, y: cy + r * 0.25, r: r * 0.7 },
        Clump { x: cx - r * 0.55, y: cy + r * 0.2, r: r * 0.7 },
        Clump { x: cx, y: cy - r * 0.25, r: r * 0.8 },
    ];
    for (i, k) in ks.iter().enumerate() {
        draw_clump(c, *k, dark, light, crown, seed + i as u32 * 7, 0.25);
    }
    let dots = match stage {
        3 => Some([rgb(0xfbf2f4), rgb(0xf0b8c8)]),
        4 => Some([rgb(0xc8281e), rgb(0xe8a020)]),
        _ => None,
    };
    if let Some(cols) = dots {
        for i in 0..14 {
            let px = cx + (hash01(i, 41, seed) - 0.5) * r * 2.2;
            let py = cy + (hash01(i, 42, seed) - 0.5) * r * 1.6;
            if c.get(px as i32, py as i32)[3] > 200 {
                let col = cols[(hash01(i, 43, seed) * 2.0) as usize % 2];
                c.set(px as i32, py as i32, col);
                if stage == 4 {
                    c.set(px as i32 + 1, py as i32, shade(col, 0.8));
                }
            }
        }
    }
}

/// The leafless winter form of [`draw_fruit_tree`] at stage 2 (same trunk and crown outline):
/// limbs from the trunk to where the leaf clumps would be, twigs beyond them and a dusting of
/// snow on the upper edges.
pub fn draw_bare_fruit_tree(c: &mut Canvas, x: f32, y: f32, seed: u32) {
    let bark = rgb(palette::BARK);
    let s = 0.8;
    let h = 26.0 * s;
    let r = 9.0 * s;
    let top = (x, y - h * 0.45);
    trunk(c, x, y, top.1, 3.0 * s + 0.8, 2.0 * s, bark);
    let (cx, cy) = (x, y - h + r);
    let ends = [(cx + r * 0.5, cy + r * 0.25), (cx - r * 0.55, cy + r * 0.2), (cx, cy - r * 0.25)];
    let branch = shade(bark, 0.95);
    for (i, &(ex, ey)) in ends.iter().enumerate() {
        let mid = (top.0 + (ex - top.0) * 0.45, top.1 + (ey - top.1) * 0.6);
        limb(c, top.0, top.1, mid.0, mid.1, 1.0, branch);
        limb(c, mid.0, mid.1, ex, ey, 0.7, branch);
        let away = (ex - cx).atan2(-(ey - cy + r * 0.6));
        for j in 0..3 {
            let a = away + (j as f32 - 1.0) * 0.7 + (hash01(i as i32, j, seed) - 0.5) * 0.4;
            let l = r * (0.45 + 0.3 * hash01(i as i32, j + 3, seed));
            c.thick_line(ex, ey, ex + a.sin() * l, ey - a.cos() * l, 0.55, branch);
        }
    }
    snow_dust(c, 0.45, seed + 91);
}

/// Bush with its base centred at (x, y).
pub fn draw_bush(c: &mut Canvas, x: f32, y: f32, season: Season, s: f32, seed: u32) {
    let (rx, ry) = (11.0 * s, 7.0 * s);
    let (ccx, ccy) = (x, y - ry);
    let twig = rgb(0x5a4030);
    for i in 0..6 {
        let a = -1.1 + 0.44 * i as f32;
        c.thick_line(x, y - 1.0, x + a.sin() * rx * 0.9, y - 1.0 - a.cos() * ry * 1.6, 0.55, twig);
    }
    let cols = match season {
        Season::Spring => Some((rgb(0x4d8a32), rgb(0x9ccc58))),
        Season::Summer => Some((rgb(0x2f6a2c), rgb(0x67a043))),
        Season::Autumn => Some((rgb(0x8a2e18), rgb(0xd8742e))),
        Season::Winter => None,
    };
    match cols {
        Some((dark, light)) => {
            let ks = [
                Clump { x: ccx + rx * 0.45, y: ccy + ry * 0.2, r: ry * 0.95 },
                Clump { x: ccx - rx * 0.45, y: ccy + ry * 0.25, r: ry * 0.9 },
                Clump { x: ccx + rx * 0.1, y: ccy - ry * 0.25, r: ry * 1.05 },
            ];
            for (i, k) in ks.iter().enumerate() {
                draw_clump(c, *k, dark, light, (ccx, ccy, rx, ry), seed + i as u32 * 5, 0.25);
            }
            if season == Season::Spring {
                for i in 0..10 {
                    let px = ccx + (hash01(i, 51, seed) - 0.5) * rx * 1.7;
                    let py = ccy + (hash01(i, 52, seed) - 0.5) * ry * 1.6;
                    if c.get(px as i32, py as i32)[3] > 200 {
                        c.set(px as i32, py as i32, if i % 3 == 0 { rgb(0xf2b8d0) } else { rgb(0xfbf8ee) });
                    }
                }
            }
        }
        None => snow_dust(c, 0.6, seed),
    }
}

/// Irregular boulder lit from the top-left, bottom centre at (x, y).
/// `r` = (rx, ry) radii in pixels.
pub fn draw_boulder(c: &mut Canvas, x: f32, y: f32, (rx, ry): (f32, f32), base: Px, moss: bool, seed: u32) {
    let (ecx, ecy) = (x, y - ry);
    let n = 11;
    let pts: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let t = TAU * i as f32 / n as f32;
            let k = 0.78 + 0.24 * hash01(i, 1, seed);
            (ecx + t.cos() * rx * k, (ecy + t.sin() * ry * k).min(y))
        })
        .collect();
    c.fill_polygon(&pts, |px, py| {
        let nx = (px as f32 + 0.5 - ecx) / rx;
        let ny = (py as f32 + 0.5 - ecy) / ry;
        let facet = (hash01(((nx + 1.0) * 2.2) as i32, ((ny + 1.0) * 1.8) as i32, seed + 3) - 0.5) * 0.18;
        let l = 0.5 + 0.5 * (-0.55 * nx - 0.85 * ny);
        let mut col = shade(base, 0.6 + 0.5 * l + facet + (hash01(px, py, seed) - 0.5) * 0.06);
        if moss && ny < -0.25 && value_noise(px as f32 * 0.35, py as f32 * 0.35, seed + 9) > 0.45 {
            col = mix(col, rgb(0x5f7f34), 0.6);
        }
        col
    });
    // A crack for character.
    let cx0 = ecx - rx * 0.1;
    c.linef(cx0, ecy - ry * 0.3, cx0 + rx * 0.25, ecy + ry * 0.2, rgba(0x3a3632, 150));
}

fn stump() -> Canvas {
    let mut c = Canvas::new(18, 14);
    let bark = rgb(palette::BARK);
    // Roots.
    c.thick_line(3.0, 12.5, 6.0, 10.0, 0.9, bark);
    c.thick_line(15.0, 12.5, 12.0, 10.0, 0.9, shade(bark, 0.8));
    c.fill_polygon(&[(4.0, 5.0), (14.0, 5.0), (14.5, 13.0), (3.5, 13.0)], |x, _| {
        shade(bark, if x < 7 { 1.1 } else if x > 11 { 0.75 } else { 0.92 })
    });
    c.fill_ellipse(9.0, 13.0, 5.5, 1.2, |_, _, _| shade(bark, 0.8));
    c.fill_ellipse(9.0, 5.0, 5.0, 2.4, |_, _, d| {
        let ring = (d.sqrt() * 3.0).fract() < 0.3;
        if d > 0.75 { shade(rgb(palette::END_GRAIN), 0.75) } else if ring { shade(rgb(palette::END_GRAIN), 0.85) } else { rgb(palette::END_GRAIN) }
    });
    c.outline(rgba(palette::OUTLINE, 200));
    c
}

fn in_tile(x: i32, y: i32) -> bool {
    let u = (x as f32 + 0.5 - 32.0) / 32.0;
    let v = (y as f32 + 0.5 - 16.0) / 16.0;
    u.abs() + v.abs() <= 0.97
}

/// Clears everything outside the tile diamond.
fn clip_to_tile(c: &mut Canvas) {
    for y in 0..c.height() {
        for x in 0..c.width() {
            if !in_tile(x, y) {
                c.set(x, y, [0, 0, 0, 0]);
            }
        }
    }
}

/// Ground marker for a deposit kind on a 64×32 tile diamond.
pub fn deposit(kind: &str) -> Canvas {
    let mut c = Canvas::new(64, 32);
    let seed = 700 + kind.len() as u32 * 13 + u32::from(kind.as_bytes()[0]);
    match kind {
        "stone" => {
            c.fill_ellipse(32.0, 17.0, 22.0, 9.0, |x, y, _| rgba(0x8a857c, (60.0 * hash01(x, y, seed)) as u8 + 40));
            for (i, &(x, y, rx, ry)) in [(22.0, 18.0, 7.0, 5.0), (36.0, 14.0, 9.0, 6.0), (44.0, 21.0, 6.0, 4.0), (30.0, 23.0, 5.0, 3.5), (16.0, 14.0, 4.0, 3.0)].iter().enumerate() {
                draw_boulder(&mut c, x, y, (rx, ry), rgb(palette::STONE), i == 1, seed + i as u32);
            }
        }
        "iron" => {
            c.fill_ellipse(32.0, 17.0, 23.0, 10.0, |x, y, d| rgba(0x8a3c22, ((1.0 - d) * 90.0 + 20.0 * hash01(x, y, seed)) as u8));
            for (i, &(x, y, rx, ry)) in [(24.0, 18.0, 8.0, 5.5), (38.0, 15.0, 9.0, 6.0), (43.0, 22.0, 5.0, 3.5), (18.0, 13.0, 4.0, 3.0)].iter().enumerate() {
                draw_boulder(&mut c, x, y, (rx, ry), rgb(0x6f645c), false, seed + i as u32);
            }
            // Rust-coloured ore veins and specks.
            for i in 0..26 {
                let x = 14 + (hash01(i, 1, seed) * 36.0) as i32;
                let y = 8 + (hash01(i, 2, seed) * 16.0) as i32;
                if c.get(x, y)[3] > 200 {
                    let col = if i % 3 == 0 { rgb(0xd88a4a) } else { rgb(0xa8482a) };
                    c.set(x, y, col);
                    c.set(x + 1, y, shade(col, 0.8));
                }
            }
            c.linef(31.0, 11.0, 42.0, 16.0, rgb(0xb4552e));
            c.linef(19.0, 16.0, 27.0, 19.0, rgb(0xb4552e));
        }
        "coal" => {
            c.fill_ellipse(32.0, 17.0, 23.0, 10.0, |x, y, d| rgba(0x1a1614, ((1.0 - d) * 140.0 + 30.0 * hash01(x, y, seed)) as u8));
            draw_boulder(&mut c, 38.0, 16.0, (9.0, 6.0), rgb(0x4a4642), false, seed);
            c.linef(31.0, 13.0, 46.0, 13.0, rgb(0x141210));
            c.linef(31.0, 14.0, 46.0, 15.0, rgb(0x141210));
            for (i, &(x, y, r)) in [(22.0, 18.0, 3.2), (27.0, 21.0, 2.6), (18.0, 15.0, 2.2), (33.0, 22.0, 2.4), (44.0, 22.0, 2.8), (25.0, 14.0, 2.0)].iter().enumerate() {
                c.lit_ellipse(x, y, r, r * 0.75, rgb(0x2a2724));
                c.set((x - r * 0.4) as i32, (y - r * 0.4) as i32, rgb(0x9a9a9a));
                let _ = i;
            }
        }
        "clay" => {
            for (i, &(x, y, rx, ry)) in [(26.0, 16.0, 14.0, 6.5), (40.0, 19.0, 12.0, 5.5), (32.0, 11.0, 8.0, 3.5)].iter().enumerate() {
                c.fill_ellipse(x, y, rx, ry, |px, py, d| {
                    let col = mix(rgb(0x9c5a3a), rgb(0xb8704a), value_noise(px as f32 * 0.3, py as f32 * 0.3, seed + i as u32));
                    with_alpha(shade(col, 1.05 - 0.2 * d), 235)
                });
            }
            // Drying cracks and a puddle.
            for i in 0..6 {
                let x0 = 18.0 + hash01(i, 4, seed) * 28.0;
                let y0 = 12.0 + hash01(i, 5, seed) * 9.0;
                c.linef(x0, y0, x0 + 3.0, y0 + 1.0, rgba(0x5a3020, 200));
                c.linef(x0 + 3.0, y0 + 1.0, x0 + 4.0, y0 + 3.0, rgba(0x5a3020, 200));
            }
            c.fill_ellipse(37.0, 17.0, 6.0, 2.4, |_, py, _| if py < 17 { rgb(0x7fa6b8) } else { rgb(0x5e8698) });
            c.lit_ellipse(22.0, 19.0, 3.0, 2.0, rgb(0xa86444));
            c.lit_ellipse(46.0, 21.0, 2.5, 1.7, rgb(0xa86444));
        }
        "sand" => {
            c.fill_ellipse(32.0, 17.0, 24.0, 10.5, |px, py, d| {
                let ripple = ((px as f32 * 0.5 + py as f32 * 1.4).sin() * 0.5 + 0.5) > 0.82;
                let base = mix(rgb(0xe9d7a2), rgb(0xd6bf82), d);
                let col = if ripple { shade(base, 0.86) } else { base };
                with_alpha(col, if d > 0.8 { 160 } else { 240 })
            });
            c.lit_ellipse(28.0, 15.0, 9.0, 4.0, rgb(0xeedfae));
            c.lit_ellipse(40.0, 19.0, 7.0, 3.0, rgb(0xe8d6a0));
        }
        "gold" => {
            c.fill_ellipse(32.0, 17.0, 22.0, 9.0, |x, y, _| rgba(0x8a7a5a, (50.0 * hash01(x, y, seed)) as u8 + 40));
            for (i, &(x, y, rx, ry)) in [(25.0, 18.0, 8.0, 5.5), (39.0, 15.0, 8.5, 6.0), (44.0, 22.0, 5.0, 3.5)].iter().enumerate() {
                draw_boulder(&mut c, x, y, (rx, ry), rgb(0xb8b0a0), false, seed + i as u32);
            }
            for i in 0..22 {
                let x = 16 + (hash01(i, 1, seed) * 34.0) as i32;
                let y = 8 + (hash01(i, 2, seed) * 16.0) as i32;
                if c.get(x, y)[3] > 200 {
                    c.set(x, y, rgb(palette::GOLD_LIGHT));
                    c.set(x + 1, y, rgb(palette::GOLD));
                }
            }
            for &(x, y) in &[(30, 10), (42, 13), (22, 16)] {
                c.set(x, y, rgb(0xffffff));
                c.set(x - 1, y, rgb(0xfff2b0));
                c.set(x + 1, y, rgb(0xfff2b0));
                c.set(x, y - 1, rgb(0xfff2b0));
                c.set(x, y + 1, rgb(0xfff2b0));
            }
        }
        _ => {
            // Fish: ripple rings and dark silhouettes in the water.
            c.fill_ellipse(32.0, 16.0, 22.0, 9.5, |_, _, d| rgba(0x1d4a66, ((1.0 - d) * 90.0) as u8));
            for (k, &(x, y, r)) in [(25.0, 15.0, 11.0), (41.0, 19.0, 8.0)].iter().enumerate() {
                for ring in 0..3 {
                    let rr = r - ring as f32 * 3.4;
                    c.fill_ellipse(x, y, rr, rr * 0.5, |px, py, d| {
                        if d > 0.7 && hash01(px, py, seed + k as u32) > 0.15 { rgba(0xeaf6fb, 200) } else { [0, 0, 0, 0] }
                    });
                }
            }
            for &(x, y, flip) in &[(24.0f32, 16.0f32, false), (39.0, 20.0, true), (34.0, 11.0, false), (46.0, 14.0, true)] {
                let d = if flip { -1.0 } else { 1.0 };
                c.fill_ellipse(x, y, 3.6, 1.5, |_, _, _| rgba(0x10283a, 220));
                c.poly(&[(x - 3.0 * d, y), (x - 5.5 * d, y - 1.8), (x - 5.5 * d, y + 1.8)], rgba(0x10283a, 220));
                c.set((x + 2.0 * d) as i32, (y - 1.0) as i32, rgba(0xc8e0ec, 220));
            }
            for &(x, y) in &[(30, 9), (45, 15), (20, 21)] {
                c.set(x, y, rgba(0xffffff, 200));
            }
        }
    }
    clip_to_tile(&mut c);
    c
}

/// Assembles the `nature` atlas.
pub fn atlas() -> AtlasBuilder {
    let mut atlas = AtlasBuilder::new("nature", Sampling::Pixel);
    for (ki, kind) in Tree::ALL.iter().enumerate() {
        let (h, rw) = kind.dims();
        for (si, (stage, scale)) in STAGES.iter().enumerate() {
            let w = ((rw * scale * 2.0).ceil() as u32 + 8) & !1;
            let hh = (h * scale).ceil() as u32 + 8;
            let seed = 9000 + ki as u32 * 100 + si as u32 * 10;
            for season in Season::ALL {
                let mut c = Canvas::new(w, hh);
                draw_tree(&mut c, w as f32 * 0.5, hh as f32, *kind, season, *scale, seed);
                c.outline(rgba(palette::OUTLINE, 190));
                atlas.add(format!("{}_{}_{}", kind.name(), season.name(), stage), c, (0.5, 1.0));
            }
        }
    }
    atlas.add("stump", stump(), (0.5, 1.0));
    for season in Season::ALL {
        let mut c = Canvas::new(28, 20);
        draw_bush(&mut c, 14.0, 20.0, season, 1.0, 4242);
        c.outline(rgba(palette::OUTLINE, 190));
        atlas.add(format!("bush_{}", season.name()), c, (0.5, 1.0));
    }
    for (i, &(w, h, rx, ry, moss)) in [(32u32, 22u32, 14.0, 10.0, false), (24, 17, 10.0, 7.5, true), (42, 28, 19.0, 13.0, false)].iter().enumerate() {
        let mut c = Canvas::new(w, h);
        draw_boulder(&mut c, w as f32 * 0.5, h as f32, (rx, ry), rgb(palette::STONE), moss, 300 + i as u32);
        if i == 2 {
            draw_boulder(&mut c, w as f32 * 0.5 + 12.0, h as f32, (7.0, 5.0), shade(rgb(palette::STONE), 0.95), true, 377);
        }
        c.outline(rgba(palette::OUTLINE, 200));
        atlas.add(format!("boulder_{i}"), c, (0.5, 1.0));
    }
    for kind in DEPOSITS {
        atlas.add(format!("deposit_{kind}"), deposit(kind), (0.5, 0.5));
    }
    atlas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_silhouette_is_stable_across_green_seasons() {
        // Same seed → summer and autumn crowns cover (almost) the same pixels.
        let mut a = Canvas::new(60, 70);
        let mut b = Canvas::new(60, 70);
        draw_tree(&mut a, 30.0, 70.0, Tree::Oak, Season::Summer, 1.0, 5);
        draw_tree(&mut b, 30.0, 70.0, Tree::Oak, Season::Autumn, 1.0, 5);
        let diff = a.img.pixels().zip(b.img.pixels()).filter(|(p, q)| (p.0[3] > 0) != (q.0[3] > 0)).count();
        assert!(diff < 40, "silhouettes differ in {diff} pixels");
    }

    #[test]
    fn winter_deciduous_trees_are_bare() {
        let mut summer = Canvas::new(60, 70);
        let mut winter = Canvas::new(60, 70);
        draw_tree(&mut summer, 30.0, 70.0, Tree::Oak, Season::Summer, 1.0, 5);
        draw_tree(&mut winter, 30.0, 70.0, Tree::Oak, Season::Winter, 1.0, 5);
        assert!(winter.coverage() < summer.coverage() * 0.6);
    }

    #[test]
    fn bare_fruit_trees_keep_the_outline_without_leaves() {
        let mut leaves = Canvas::new(32, 34);
        let mut bare = Canvas::new(32, 34);
        draw_fruit_tree(&mut leaves, 16.0, 34.0, 2, 3);
        draw_bare_fruit_tree(&mut bare, 16.0, 34.0, 3);
        assert!(bare.coverage() > 0.0 && bare.coverage() < leaves.coverage() * 0.6);
        // Same trunk foot and no pixel above the leafy crown.
        assert!(bare.get(16, 33)[3] > 0);
        let top = |c: &Canvas| c.opaque_bounds().map_or(99, |(_, y, _, _)| y);
        assert!(top(&bare) >= top(&leaves));
        // Nothing green is left.
        let green = bare.img.pixels().filter(|p| p.0[3] > 0 && i32::from(p.0[1]) > i32::from(p.0[0]) + 20).count();
        assert_eq!(green, 0);
    }

    #[test]
    fn trunk_base_is_at_anchor() {
        let mut c = Canvas::new(40, 80);
        draw_tree(&mut c, 20.0, 80.0, Tree::Pine, Season::Summer, 1.0, 1);
        // Bottom row is opaque at the centre column (trunk) and empty at the edges.
        assert!(c.get(20, 79)[3] > 0 || c.get(19, 79)[3] > 0);
        assert_eq!(c.get(0, 79)[3], 0);
    }

    #[test]
    fn deposits_stay_inside_the_tile() {
        for kind in DEPOSITS {
            let c = deposit(kind);
            assert!(c.coverage() > 0.05, "{kind} marker is too faint");
            for (x, y) in [(0, 0), (63, 0), (0, 31), (63, 31), (2, 2)] {
                assert_eq!(c.get(x, y)[3], 0, "{kind} leaks outside the diamond");
            }
        }
    }
}
