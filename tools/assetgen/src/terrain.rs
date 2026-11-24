//! Placeholder terrain tiles: 2:1 diamonds (64×32) with a 16 px skirt, edge fringes for smooth
//! transitions, cliff segments and grid overlays.

use kingdoms_engine::rendering::texture::Sampling;

use crate::atlas::AtlasBuilder;
use crate::draw::{Canvas, Px, hash01, mix, rgb, shade, value_noise, with_alpha};

/// Terrain kinds in transition priority order (later entries draw their fringe onto earlier ones).
/// Must match `kingdoms_sim::world::Terrain::sprite_key`.
pub const KINDS: &[&str] =
    &["deep_water", "shallow_water", "sand", "swamp", "steppe", "dirt", "grass", "meadow", "forest", "rock", "snow"];

pub const VARIANTS: u32 = 4;

struct Style {
    base: Px,
    alt: Px,
    side: Px,
    detail: fn(&mut Canvas, u32),
}

fn style(kind: &str) -> Style {
    match kind {
        "deep_water" => Style { base: rgb(0x24557f), alt: rgb(0x2b6290), side: rgb(0x1d3f5c), detail: detail_waves },
        "shallow_water" => Style { base: rgb(0x3b86b0), alt: rgb(0x4a98bf), side: rgb(0x6f6a4e), detail: detail_waves },
        "sand" => Style { base: rgb(0xdcc68e), alt: rgb(0xcfb67a), side: rgb(0xb59a62), detail: detail_speckle },
        "swamp" => Style { base: rgb(0x58683a), alt: rgb(0x4c5c33), side: rgb(0x4a3f2a), detail: detail_swamp },
        "steppe" => Style { base: rgb(0xb8a766), alt: rgb(0xa89757), side: rgb(0x8b6f45), detail: detail_dry_tufts },
        "dirt" => Style { base: rgb(0x8f6c45), alt: rgb(0x7f5f3c), side: rgb(0x6e5136), detail: detail_pebbles },
        "grass" => Style { base: rgb(0x6e9a3f), alt: rgb(0x618d37), side: rgb(0x7a5a3a), detail: detail_tufts },
        "meadow" => Style { base: rgb(0x78a83f), alt: rgb(0x6c9c38), side: rgb(0x7a5a3a), detail: detail_flowers },
        "forest" => Style { base: rgb(0x4d7531), alt: rgb(0x43692b), side: rgb(0x6a4d32), detail: detail_litter },
        "rock" => Style { base: rgb(0x8c8781), alt: rgb(0x7b7671), side: rgb(0x67625d), detail: detail_cracks },
        "snow" => Style { base: rgb(0xe9eff3), alt: rgb(0xdbe4ec), side: rgb(0x8a8f96), detail: detail_snow },
        _ => Style { base: rgb(0xff00ff), alt: rgb(0x000000), side: rgb(0x444444), detail: |_, _| {} },
    }
}

const TOP: [(f32, f32); 4] = [(32.0, 0.0), (64.0, 16.0), (32.0, 32.0), (0.0, 16.0)];

fn in_diamond(x: i32, y: i32) -> bool {
    let u = (x as f32 + 0.5 - 32.0) / 32.0;
    let v = (y as f32 + 0.5 - 16.0) / 16.0;
    u.abs() + v.abs() <= 1.0
}

/// Surface colour of `kind` at pixel (x, y) of the top face.
fn surface(kind_style: &Style, x: i32, y: i32, seed: u32) -> Px {
    // Fine-grained texture only: large-scale variation is added per tile in the renderer, so
    // tile borders do not form a visible grid.
    let n = value_noise(x as f32 / 3.0, y as f32 / 2.0, seed) * 0.55 + value_noise(x as f32 / 1.3, y as f32 / 1.0, seed + 9) * 0.45;
    mix(kind_style.base, kind_style.alt, 0.25 + n * 0.5)
}

fn draw_top(c: &mut Canvas, kind: &str, seed: u32) {
    let st = style(kind);
    c.fill_polygon(&TOP, |x, y| surface(&st, x, y, seed));
    (st.detail)(c, seed);
}

fn draw_skirt(c: &mut Canvas, side: Px, top: Px, y_off: f32, seed: u32, grass_lip: bool) {
    let left = [(0.0, 16.0 + y_off), (32.0, 32.0 + y_off), (32.0, 48.0 + y_off), (0.0, 32.0 + y_off)];
    let right = [(32.0, 32.0 + y_off), (64.0, 16.0 + y_off), (64.0, 32.0 + y_off), (32.0, 48.0 + y_off)];
    let strata = |x: i32, y: i32, f: f32| {
        let n = value_noise(x as f32 / 9.0, y as f32 / 2.0, seed + 31);
        shade(side, f * (0.9 + n * 0.2))
    };
    // Light comes from the top-left: the left (south-west facing) skirt is lit, the right one
    // is in shade – the same convention as the building sprites.
    c.fill_polygon(&left, |x, y| strata(x, y, 0.86));
    c.fill_polygon(&right, |x, y| strata(x, y, 0.68));
    if grass_lip {
        // Thin overhang of the top surface along the upper edge of the faces.
        for x in 0..64 {
            let edge_y = if x < 32 { 16.0 + x as f32 * 0.5 } else { 32.0 - (x - 32) as f32 * 0.5 } + y_off;
            let len = 1 + (hash01(x, 0, seed + 77) * 2.5) as i32;
            for k in 0..len {
                c.blend(x, edge_y as i32 + k, shade(top, 0.8));
            }
        }
    }
}

/// Terrain block sprite (top diamond plus skirt) for `kind`, variant `variant`.
pub fn tile(kind: &str, variant: u32) -> Canvas {
    let mut c = Canvas::new(64, 48);
    let seed = 1000 + variant * 97 + KINDS.iter().position(|k| *k == kind).unwrap_or(0) as u32 * 13;
    let st = style(kind);
    let lip = matches!(kind, "grass" | "meadow" | "forest" | "swamp" | "steppe");
    draw_skirt(&mut c, st.side, st.base, 0.0, seed, lip);
    draw_top(&mut c, kind, seed);
    c
}

/// Additional cliff segment (one elevation level) placed below a tile's skirt.
pub fn cliff(kind: &str) -> Canvas {
    let mut c = Canvas::new(64, 32);
    let st = style(kind);
    draw_skirt(&mut c, st.side, st.base, -16.0, 4242, false);
    c
}

/// Fringe of `kind` covering the diamond edge towards direction `dir` (ne/se/sw/nw), with an
/// irregular noisy border, used to blend neighbouring terrain types.
pub fn edge(kind: &str, dir: &str) -> Canvas {
    let mut c = Canvas::new(64, 32);
    let st = style(kind);
    let seed = 5000 + KINDS.iter().position(|k| *k == kind).unwrap_or(0) as u32 * 31;
    for y in 0..32 {
        for x in 0..64 {
            if !in_diamond(x, y) {
                continue;
            }
            let u = (x as f32 + 0.5 - 32.0) / 32.0;
            let v = (y as f32 + 0.5 - 16.0) / 16.0;
            let (closeness, along) = match dir {
                "ne" => (u - v, u + v),
                "se" => (u + v, u - v),
                "sw" => (v - u, u + v),
                _ => (-u - v, u - v),
            };
            let n = value_noise(along * 6.0 + 3.0, 0.5, seed) * 0.22 + value_noise(along * 17.0, 1.5, seed + 3) * 0.08;
            // Taper towards the corners so neighbouring fringes join smoothly.
            let taper = 1.0 - along.abs().powi(2) * 0.35;
            let width = (0.30 + n * 1.4) * taper;
            if closeness > 1.0 - width {
                c.set(x, y, surface(&st, x, y, seed));
            }
        }
    }
    c
}

fn detail_tufts(c: &mut Canvas, seed: u32) {
    for i in 0..26 {
        let x = (hash01(i, 1, seed) * 64.0) as i32;
        let y = (hash01(i, 2, seed) * 32.0) as i32;
        if in_diamond(x, y) && in_diamond(x, y - 2) {
            let col = if hash01(i, 3, seed) > 0.5 { rgb(0x4f7a2c) } else { rgb(0x86b04c) };
            c.blend(x, y, col);
            c.blend(x, y - 1, col);
            if hash01(i, 4, seed) > 0.6 {
                c.blend(x + 1, y - 1, col);
            }
        }
    }
}

fn detail_flowers(c: &mut Canvas, seed: u32) {
    detail_tufts(c, seed);
    let colors = [rgb(0xf4f0e0), rgb(0xf2d34c), rgb(0xd77fa1), rgb(0x9fb7f0)];
    for i in 0..14 {
        let x = (hash01(i, 5, seed) * 64.0) as i32;
        let y = (hash01(i, 6, seed) * 32.0) as i32;
        if in_diamond(x, y) {
            c.set(x, y, colors[(hash01(i, 7, seed) * 4.0) as usize % 4]);
        }
    }
}

fn detail_litter(c: &mut Canvas, seed: u32) {
    for i in 0..30 {
        let x = (hash01(i, 8, seed) * 64.0) as i32;
        let y = (hash01(i, 9, seed) * 32.0) as i32;
        if in_diamond(x, y) {
            let col = if hash01(i, 10, seed) > 0.5 { rgb(0x6b5232) } else { rgb(0x3a5a24) };
            c.blend(x, y, col);
        }
    }
}

fn detail_dry_tufts(c: &mut Canvas, seed: u32) {
    for i in 0..14 {
        let x = (hash01(i, 11, seed) * 64.0) as i32;
        let y = (hash01(i, 12, seed) * 32.0) as i32;
        if in_diamond(x, y) && in_diamond(x, y - 2) {
            let col = rgb(0x8f8a4a);
            c.blend(x, y, col);
            c.blend(x - 1, y - 1, col);
            c.blend(x + 1, y - 1, col);
        }
    }
}

fn detail_speckle(c: &mut Canvas, seed: u32) {
    for i in 0..40 {
        let x = (hash01(i, 13, seed) * 64.0) as i32;
        let y = (hash01(i, 14, seed) * 32.0) as i32;
        if in_diamond(x, y) {
            let p = c.get(x, y);
            c.set(x, y, shade(p, if hash01(i, 15, seed) > 0.5 { 0.9 } else { 1.08 }));
        }
    }
}

fn detail_pebbles(c: &mut Canvas, seed: u32) {
    for i in 0..18 {
        let x = (hash01(i, 16, seed) * 64.0) as i32;
        let y = (hash01(i, 17, seed) * 32.0) as i32;
        if in_diamond(x, y) && in_diamond(x + 1, y) {
            c.set(x, y, rgb(0xa08a6e));
            c.set(x + 1, y, rgb(0x6c5a45));
        }
    }
}

fn detail_cracks(c: &mut Canvas, seed: u32) {
    for i in 0..5 {
        let mut x = 10 + (hash01(i, 18, seed) * 44.0) as i32;
        let mut y = 6 + (hash01(i, 19, seed) * 20.0) as i32;
        for step in 0..7 {
            if in_diamond(x, y) {
                c.set(x, y, rgb(0x5f5a55));
            }
            x += if hash01(i, 20 + step, seed) > 0.5 { 1 } else { -1 };
            y += i32::from(hash01(i, 40 + step, seed) > 0.4);
        }
    }
    for i in 0..10 {
        let x = (hash01(i, 21, seed) * 64.0) as i32;
        let y = (hash01(i, 22, seed) * 32.0) as i32;
        if in_diamond(x, y) && in_diamond(x + 2, y + 1) {
            c.fill_rect(x, y, 2, 1, rgb(0xa7a29c));
            c.fill_rect(x, y + 1, 2, 1, rgb(0x6f6a64));
        }
    }
}

fn detail_snow(c: &mut Canvas, seed: u32) {
    for i in 0..10 {
        let x = (hash01(i, 23, seed) * 64.0) as i32;
        let y = (hash01(i, 24, seed) * 32.0) as i32;
        if in_diamond(x, y) {
            c.set(x, y, rgb(0xffffff));
        }
    }
}

fn detail_swamp(c: &mut Canvas, seed: u32) {
    for i in 0..3 {
        let cx = 14.0 + hash01(i, 25, seed) * 36.0;
        let cy = 9.0 + hash01(i, 26, seed) * 14.0;
        let rx = 4.0 + hash01(i, 27, seed) * 5.0;
        c.fill_ellipse(cx, cy, rx, rx * 0.5, |x, y, d| {
            if in_diamond(x, y) { mix(rgb(0x3d5a5c), rgb(0x4f7376), 1.0 - d) } else { [0, 0, 0, 0] }
        });
    }
    for i in 0..12 {
        let x = (hash01(i, 28, seed) * 64.0) as i32;
        let y = (hash01(i, 29, seed) * 32.0) as i32;
        if in_diamond(x, y) && in_diamond(x, y - 3) {
            for k in 0..3 {
                c.blend(x, y - k, rgb(0x7d8a3f));
            }
        }
    }
}

fn detail_waves(c: &mut Canvas, seed: u32) {
    for i in 0..6 {
        let x0 = 8 + (hash01(i, 30, seed) * 44.0) as i32;
        let y = 5 + (hash01(i, 31, seed) * 22.0) as i32;
        let len = 4 + (hash01(i, 32, seed) * 6.0) as i32;
        for k in 0..len {
            if in_diamond(x0 + k, y) {
                let p = c.get(x0 + k, y);
                c.set(x0 + k, y, mix(p, rgb(0xcfe6f2), 0.35));
            }
        }
    }
}

/// Diamond-shaped overlays used for hover / placement / grid display.
fn overlay_tile(fill_alpha: u8, outline: bool) -> Canvas {
    let mut c = Canvas::new(64, 32);
    c.fill_polygon(&TOP, |_, _| with_alpha(rgb(0xffffff), fill_alpha));
    if outline {
        let pts = [(32, 0), (63, 15), (32, 31), (0, 15)];
        for i in 0..4 {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % 4];
            c.line(x0, y0, x1, y1, rgb(0xffffff));
        }
    }
    c
}

/// Assembles the `terrain` atlas.
pub fn atlas() -> AtlasBuilder {
    let mut atlas = AtlasBuilder::new("terrain", Sampling::Pixel);
    // Anchor = centre of the top diamond (32, 16) in a 64×48 sprite.
    let tile_anchor = (0.5, 16.0 / 48.0);
    for kind in KINDS {
        for v in 0..VARIANTS {
            atlas.add(format!("{kind}_{v}"), tile(kind, v), tile_anchor);
        }
        // Cliff segment: anchor at the front-top point (32, 16) of a 64×32 sprite.
        atlas.add(format!("cliff_{kind}"), cliff(kind), (0.5, 0.5));
        for dir in ["ne", "se", "sw", "nw"] {
            atlas.add(format!("{kind}_edge_{dir}"), edge(kind, dir), (0.5, 0.5));
        }
    }
    // Elevation readability decals (drawn alpha-blended by the renderer).
    for (name, closeness) in [("ne", 0u8), ("nw", 1u8)] {
        let mut rim = Canvas::new(64, 32);
        let mut ao = Canvas::new(64, 32);
        for y in 0..32 {
            for x in 0..64 {
                if !in_diamond(x, y) {
                    continue;
                }
                let u = (x as f32 + 0.5 - 32.0) / 32.0;
                let v = (y as f32 + 0.5 - 16.0) / 16.0;
                // Distance (in diamond units) to the NE (u - v = 1) or NW (-u - v = 1) edge.
                let d = 1.0 - if closeness == 0 { u - v } else { -u - v };
                let px = d * 16.0; // approx. pixels from the edge
                if px < 2.2 {
                    rim.set(x, y, with_alpha(rgb(0xfff4d8), if px < 1.1 { 170 } else { 80 }));
                }
                if px < 9.0 {
                    let a = ((1.0 - px / 9.0).powf(1.6) * 120.0) as u8;
                    ao.set(x, y, with_alpha(rgb(0x000000), a));
                }
            }
        }
        atlas.add(format!("rim_{name}"), rim, (0.5, 0.5));
        atlas.add(format!("ao_{name}"), ao, (0.5, 0.5));
    }
    atlas
}

/// Assembles the `overlay` atlas (hover / placement / grid decals and soft shapes).
pub fn overlay() -> AtlasBuilder {
    let mut overlay = AtlasBuilder::new("overlay", Sampling::Smooth);
    overlay.add("tile_outline", overlay_tile(0, true), (0.5, 0.5));
    overlay.add("tile_fill", overlay_tile(110, true), (0.5, 0.5));
    overlay.add("tile_solid", overlay_tile(255, false), (0.5, 0.5));
    let mut grid = Canvas::new(64, 32);
    for (x0, y0, x1, y1) in [(32, 0, 63, 15), (63, 16, 32, 31), (31, 31, 0, 16), (0, 15, 31, 0)] {
        grid.line(x0, y0, x1, y1, with_alpha(rgb(0xffffff), 70));
    }
    overlay.add("grid", grid, (0.5, 0.5));
    let mut dot = Canvas::new(16, 16);
    dot.fill_ellipse(8.0, 8.0, 7.5, 7.5, |_, _, d| with_alpha(rgb(0xffffff), ((1.0 - d) * 255.0) as u8));
    overlay.add("soft_dot", dot, (0.5, 0.5));
    let mut shadow = Canvas::new(48, 24);
    shadow.fill_ellipse(24.0, 12.0, 23.5, 11.5, |_, _, d| with_alpha(rgb(0x000000), ((1.0 - d).powf(0.7) * 110.0) as u8));
    overlay.add("blob_shadow", shadow, (0.5, 0.5));
    overlay
}
