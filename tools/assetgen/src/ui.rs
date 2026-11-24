//! UI skin (`ui` atlas, smooth sampling): nine-slice frames and button states.
//!
//! Slice margins (documented in docs/assets.md and in the atlas `source` field):
//! `parchment`, `wood`, `gold_frame` are 96×96 with 24 px borders; `button_*` are 96×32 with
//! 12 px borders. `gold_frame` has a transparent centre, the others are filled panels.

use kingdoms_engine::rendering::texture::Sampling;

use crate::atlas::AtlasBuilder;
use crate::draw::{Canvas, Px, desaturate, hash01, mix, rgb, rgba, shade, value_noise, with_alpha};
use crate::palette::*;

/// Edge length of the nine-slice frames.
pub const FRAME: u32 = 96;
/// Border width of the nine-slice frames.
pub const FRAME_BORDER: u32 = 24;
/// Button size.
pub const BUTTON: (u32, u32) = (96, 32);
/// Border width of the button nine-slices.
pub const BUTTON_BORDER: u32 = 12;

/// Distance of pixel (x, y) to the nearest canvas edge and the index of that edge
/// (0 = top, 1 = right, 2 = bottom, 3 = left).
fn edge_dist(x: i32, y: i32, w: i32, h: i32) -> (f32, usize) {
    let d = [y as f32 + 0.5, (w - 1 - x) as f32 + 0.5, (h - 1 - y) as f32 + 0.5, x as f32 + 0.5];
    let mut best = 0;
    for i in 1..4 {
        if d[i] < d[best] {
            best = i;
        }
    }
    (d[best], best)
}

fn parchment() -> Canvas {
    let n = FRAME as i32;
    let mut c = Canvas::new(FRAME, FRAME);
    for y in 0..n {
        for x in 0..n {
            let (d, side) = edge_dist(x, y, n, n);
            let along = if side % 2 == 0 { x as f32 } else { y as f32 };
            let tear = 2.0 + 3.0 * value_noise(along * 0.22, side as f32 * 5.0, 401);
            let alpha = (d - tear).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            let blot = value_noise(x as f32 * 0.07, y as f32 * 0.07, 402);
            let fine = hash01(x, y, 403);
            let mut col = mix(rgb(PARCHMENT), rgb(PARCHMENT_DARK), blot * 0.45 + fine * 0.08);
            if d < 16.0 {
                col = mix(col, rgb(0x8a6236), ((16.0 - d) / 16.0).powi(2) * 0.75);
            }
            // Decorative ink rule inside the border.
            if (18.0..19.2).contains(&d) {
                col = mix(col, rgb(0x7a5a34), 0.7);
            }
            c.set(x, y, with_alpha(col, (alpha * 255.0) as u8));
        }
    }
    c
}

fn wood() -> Canvas {
    let n = FRAME as i32;
    let b = FRAME_BORDER as f32;
    let mut c = Canvas::new(FRAME, FRAME);
    for y in 0..n {
        for x in 0..n {
            let (d, side) = edge_dist(x, y, n, n);
            let col = if d < b {
                // Mitred frame planks with grain along the edge.
                let (along, across) = if side % 2 == 0 { (x as f32, d) } else { (y as f32, d) };
                let grain = value_noise(along * 0.04 + side as f32 * 9.0, across * 0.7, 410);
                let mut cc = shade(rgb(WOOD_UI), 0.85 + 0.3 * grain);
                // Bevel: lit on the outer top/left, shaded bottom/right; groove at the inner edge.
                let lit = if side == 0 || side == 3 { 1.18 } else { 0.78 };
                if d < 3.0 {
                    cc = shade(cc, lit);
                } else if d > b - 3.0 {
                    cc = shade(cc, 2.0 - lit);
                }
                if d > b - 1.0 {
                    cc = shade(cc, 0.55);
                }
                cc
            } else {
                // Dark inner panel with vertical boards.
                let seam = (x - FRAME_BORDER as i32).rem_euclid(16) == 0;
                let grain = value_noise(x as f32 * 0.5, y as f32 * 0.03, 411);
                let cc = shade(rgb(0x3a2616), 0.9 + 0.2 * grain);
                if seam { shade(cc, 0.6) } else { cc }
            };
            c.set(x, y, if d < 1.0 { rgb(OUTLINE) } else { col });
        }
    }
    // Iron corner brackets with rivets.
    for &(cx, cy, sx, sy) in &[(0, 0, 1, 1), (n - 1, 0, -1, 1), (0, n - 1, 1, -1), (n - 1, n - 1, -1, -1)] {
        for i in 2..20 {
            for k in 2..8 {
                c.set(cx + sx * i, cy + sy * k, shade(rgb(IRON), 1.0 - 0.01 * i as f32));
                c.set(cx + sx * k, cy + sy * i, shade(rgb(IRON), 1.0 - 0.01 * i as f32));
            }
        }
        for &(i, k) in &[(5, 5), (15, 5), (5, 15)] {
            c.set(cx + sx * i, cy + sy * k, rgb(0xb8bec6));
            c.set(cx + sx * i + 1, cy + sy * k, rgb(0x2a2a2e));
        }
    }
    c
}

fn gold_frame() -> Canvas {
    let n = FRAME as i32;
    let mut c = Canvas::new(FRAME, FRAME);
    let (outer, inner) = (4.0, 18.0);
    for y in 0..n {
        for x in 0..n {
            let (d, side) = edge_dist(x, y, n, n);
            if d < outer - 1.0 || d > inner + 1.0 {
                continue;
            }
            let col = if d < outer || d > inner {
                rgb(0x4a3410)
            } else {
                let t = (d - outer) / (inner - outer);
                let profile = (t * std::f32::consts::PI).sin();
                let lit = if side == 0 || side == 3 { 1.0 } else { 0.8 };
                let along = if side % 2 == 0 { x } else { y };
                let bead = if (6.0..8.0).contains(&(d - outer)) && along % 6 < 3 { 1.2 } else { 1.0 };
                shade(mix(rgb(0x8a6418), rgb(GOLD_LIGHT), profile), lit * bead)
            };
            c.set(x, y, col);
        }
    }
    // Rosettes on the corners.
    for &(cx, cy) in &[(11.0, 11.0), (84.0, 11.0), (11.0, 84.0), (84.0, 84.0)] {
        c.fill_ellipse(cx, cy, 8.5, 8.5, |_, _, _| rgb(0x4a3410));
        c.lit_ellipse(cx, cy, 7.5, 7.5, rgb(GOLD));
        c.lit_ellipse(cx, cy, 3.0, 3.0, rgb(0xb02028));
    }
    c
}

/// Button states in sprite-name order.
pub const BUTTON_STATES: [&str; 4] = ["normal", "hover", "pressed", "disabled"];

fn button(state: &str) -> Canvas {
    const K: u32 = 4;
    let (w, h) = (BUTTON.0 * K, BUTTON.1 * K);
    let r = 7.0 * K as f32;
    let (top, bottom, border) = match state {
        "hover" => (rgb(0xa87444), rgb(0x6e4826), rgb(GOLD_LIGHT)),
        "pressed" => (rgb(0x4a3018), rgb(0x6a4626), rgb(0xa88030)),
        "disabled" => (desaturate(rgb(0x7a5a3a), 0.8), desaturate(rgb(0x4a3a2a), 0.8), rgb(0x8a8680)),
        _ => (rgb(0x8a5a34), rgb(0x5a3a20), rgb(0xc8a040)),
    };
    let mut c = Canvas::new(w, h);
    let inside = |x: f32, y: f32, inset: f32| -> bool {
        let (x0, y0, x1, y1) = (inset, inset, w as f32 - inset, h as f32 - inset);
        let rr = (r - inset).max(1.0);
        let cx = x.clamp(x0 + rr, x1 - rr);
        let cy = y.clamp(y0 + rr, y1 - rr);
        x >= x0 && x <= x1 && y >= y0 && y <= y1 && (x - cx).powi(2) + (y - cy).powi(2) <= rr * rr
    };
    let k = K as f32;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            if !inside(px, py, 0.0) {
                continue;
            }
            let t = py / h as f32;
            let col: Px = if !inside(px, py, 1.0 * k) {
                rgb(OUTLINE)
            } else if !inside(px, py, 3.0 * k) {
                shade(border, 1.1 - 0.3 * t)
            } else {
                let mut cc = mix(top, bottom, t);
                let hl = if state == "pressed" { py > h as f32 - 5.0 * k } else { py < 5.0 * k };
                if hl {
                    cc = mix(cc, rgb(0xffffff), 0.12);
                }
                cc
            };
            c.set(x, y, col);
        }
    }
    let mut out = c.downsample(K);
    if state == "hover" {
        // Soft golden glow along the inside of the border.
        let glow = rgba(GOLD_LIGHT, 40);
        for y in 4..BUTTON.1 as i32 - 4 {
            out.blend(4, y, glow);
            out.blend(BUTTON.0 as i32 - 5, y, glow);
        }
    }
    out
}

/// Assembles the `ui` atlas.
pub fn atlas() -> AtlasBuilder {
    let source = format!(
        "placeholder art generated by kingdoms_assetgen (replace with final art); nine-slice margins: \
         parchment/wood/gold_frame {FRAME}x{FRAME} px with {FRAME_BORDER} px borders (gold_frame centre transparent), \
         button_* {}x{} px with {BUTTON_BORDER} px borders",
        BUTTON.0, BUTTON.1
    );
    let mut atlas = AtlasBuilder::new("ui", Sampling::Smooth).with_source(source);
    atlas.add("parchment", parchment(), (0.5, 0.5));
    atlas.add("wood", wood(), (0.5, 0.5));
    atlas.add("gold_frame", gold_frame(), (0.5, 0.5));
    for state in BUTTON_STATES {
        atlas.add(format!("button_{state}"), button(state), (0.5, 0.5));
    }
    atlas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_have_documented_sizes() {
        for c in [parchment(), wood(), gold_frame()] {
            assert_eq!((c.img.width(), c.img.height()), (FRAME, FRAME));
        }
        for s in BUTTON_STATES {
            let b = button(s);
            assert_eq!((b.img.width(), b.img.height()), BUTTON);
        }
    }

    #[test]
    fn gold_frame_centre_is_transparent_and_panels_are_filled() {
        let m = FRAME as i32 / 2;
        assert_eq!(gold_frame().get(m, m)[3], 0);
        assert_eq!(wood().get(m, m)[3], 255);
        assert_eq!(parchment().get(m, m)[3], 255);
    }

    #[test]
    fn nine_slice_edges_are_uniform_enough_to_stretch() {
        // Along the middle of the top border the wood frame keeps its bevel structure: the
        // outermost rows are darker/lighter consistently over the stretchable span.
        let w = wood();
        let row_lit: Vec<u8> = (30..66).map(|x| w.get(x, 1)[0]).collect();
        let row_dark: Vec<u8> = (30..66).map(|x| w.get(x, FRAME_BORDER as i32 - 1)[0]).collect();
        let avg = |v: &Vec<u8>| v.iter().map(|&p| u32::from(p)).sum::<u32>() / v.len() as u32;
        assert!(avg(&row_lit) > avg(&row_dark));
    }

    #[test]
    fn button_states_differ() {
        let imgs: Vec<_> = BUTTON_STATES.iter().map(|s| button(s).img).collect();
        for i in 0..imgs.len() {
            for j in i + 1..imgs.len() {
                assert_ne!(imgs[i], imgs[j]);
            }
        }
    }
}
