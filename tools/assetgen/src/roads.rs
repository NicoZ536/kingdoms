//! Roads, bridges, town walls and gates (`roads` atlas).
//!
//! Road tiles are flat 64×32 diamonds (anchor = tile centre `(0.5, 0.5)`). The connection mask
//! uses tile directions N = 1 (−y, upper-right edge), E = 2 (+x, lower-right), S = 4 (+y,
//! lower-left), W = 8 (−x, upper-left); each set bit adds a strip from the tile centre to the
//! midpoint of that diamond edge. Textures are periodic per tile so neighbours join seamlessly.
//!
//! Walls, gates and bridges are upright 1×1 structures and follow the building convention:
//! 64 px wide, bottom edge = front corner of the tile, anchor `(0.5, 1.0)`.

use kingdoms_engine::rendering::texture::Sampling;

use crate::atlas::AtlasBuilder;
use crate::draw::{Canvas, Px, hash01, mix, periodic_noise, rgb, shade};
use crate::iso::{Axis, IsoCanvas, Side, v3};
use crate::mat::Mat;
use crate::palette;
use crate::props;

/// Road types in sprite-name order.
pub const ROAD_KINDS: [&str; 5] = ["path", "gravel", "road", "paved", "trade_road"];

/// Mask bit towards −y (upper-right edge).
pub const N: u8 = 1;
/// Mask bit towards +x (lower-right edge).
pub const E: u8 = 2;
/// Mask bit towards +y (lower-left edge).
pub const S: u8 = 4;
/// Mask bit towards −x (upper-left edge).
pub const W: u8 = 8;

struct Style {
    half_width: f32,
    ragged: f32,
}

fn style(kind: &str) -> Style {
    match kind {
        "path" => Style { half_width: 0.19, ragged: 0.3 },
        "gravel" => Style { half_width: 0.22, ragged: 0.12 },
        "road" => Style { half_width: 0.25, ragged: 0.06 },
        "paved" => Style { half_width: 0.27, ragged: 0.0 },
        _ => Style { half_width: 0.33, ragged: 0.0 },
    }
}

/// Tile coordinates `(u, v)` (0..1 along +x / +y) of the centre of pixel (x, y) in a 64×32 tile.
pub fn tile_uv(x: i32, y: i32) -> (f32, f32) {
    let d = (x as f32 + 0.5 - 32.0) / 32.0;
    let s = (y as f32 + 0.5) / 16.0;
    ((s + d) * 0.5, (s - d) * 0.5)
}

/// Signed distance (tiles, positive inside) of `(u, v)` to the road shape of `mask`.
fn road_shape(u: f32, v: f32, mask: u8, st: &Style, seed: u32) -> f32 {
    let hw = |t: f32, side: u32| st.half_width * (1.0 + st.ragged * (periodic_noise(t * 8.0, side as f32 * 3.7, 8, seed + side) - 0.5) * 2.0);
    let (du, dv) = (u - 0.5, v - 0.5);
    let mut d = st.half_width * if mask == 0 { 1.15 } else { 1.0 } - (du * du + dv * dv).sqrt();
    if mask & N != 0 && v <= 0.5 {
        d = d.max(hw(v, if du < 0.0 { 1 } else { 2 }) - du.abs());
    }
    if mask & S != 0 && v >= 0.5 {
        d = d.max(hw(v, if du < 0.0 { 1 } else { 2 }) - du.abs());
    }
    if mask & E != 0 && u >= 0.5 {
        d = d.max(hw(u, if dv < 0.0 { 3 } else { 4 }) - dv.abs());
    }
    if mask & W != 0 && u <= 0.5 {
        d = d.max(hw(u, if dv < 0.0 { 3 } else { 4 }) - dv.abs());
    }
    d
}

/// Distance from the strip axis that the texture should treat as "across" (for ruts / kerbs).
fn across(u: f32, v: f32) -> f32 {
    (u - 0.5).abs().min((v - 0.5).abs())
}

/// Periodic per-tile hash of a texel at `res` cells per tile.
fn cell_hash(u: f32, v: f32, res: f32, seed: u32) -> f32 {
    let n = res as i32;
    hash01(((u * res).floor() as i32).rem_euclid(n), ((v * res).floor() as i32).rem_euclid(n), seed)
}

fn road_color(kind: &str, u: f32, v: f32, d: f32, hw: f32) -> Px {
    let edge_px = d * 32.0;
    match kind {
        "path" => {
            let base = rgb(0x9a7c55);
            let n = periodic_noise(u * 8.0, v * 8.0, 8, 11);
            let mut c = shade(base, 0.9 + 0.18 * n);
            let h = cell_hash(u, v, 32.0, 12);
            if h > 0.94 {
                c = shade(c, 1.18);
            } else if h < 0.05 {
                c = shade(c, 0.82);
            }
            if edge_px < 1.6 {
                c = mix(c, rgb(palette::GRASS_DARK), 0.45);
            }
            c
        }
        "gravel" => {
            let base = rgb(0x9a9284);
            let h = cell_hash(u, v, 24.0, 21);
            let c = if h > 0.7 { rgb(0xb8b2a4) } else if h < 0.25 { rgb(0x7a7266) } else { base };
            let c = shade(c, 0.94 + 0.1 * periodic_noise(u * 8.0, v * 8.0, 8, 22));
            if edge_px < 1.4 { shade(c, 0.78) } else { c }
        }
        "road" => {
            let base = rgb(0x8f7250);
            let mut c = shade(base, 0.92 + 0.14 * periodic_noise(u * 8.0, v * 8.0, 8, 31));
            let a = across(u, v);
            if (a - hw * 0.5).abs() < 0.03 {
                c = shade(c, 0.78);
            }
            if edge_px < 2.2 {
                let h = cell_hash(u, v, 16.0, 32);
                c = if h > 0.4 { shade(rgb(palette::STONE), 0.9 + 0.2 * h) } else { shade(c, 0.8) };
            }
            c
        }
        "paved" => {
            let res = 8.0;
            let row = (v * res).floor();
            let off = if (row as i32).rem_euclid(2) == 0 { 0.0 } else { 0.5 };
            let fu = (u * res + off).fract();
            let fv = (v * res).fract();
            let col = ((u * res + off).floor() as i32).rem_euclid(8);
            let tone = 0.84 + 0.26 * hash01(col, (row as i32).rem_euclid(8), 41);
            let mut c = if fu < 0.14 || fv < 0.16 { shade(rgb(palette::COBBLE), 0.62) } else { shade(rgb(palette::COBBLE), tone) };
            if fu > 0.14 && fu < 0.4 && fv > 0.16 && fv < 0.4 {
                c = shade(c, 1.1);
            }
            if edge_px < 1.4 {
                c = shade(rgb(palette::STONE_DARK), 0.9);
            }
            c
        }
        _ => {
            let res = 4.0;
            let row = (v * res).floor();
            let off = if (row as i32).rem_euclid(2) == 0 { 0.0 } else { 0.5 };
            let fu = (u * res + off).fract();
            let fv = (v * res).fract();
            let col = ((u * res + off).floor() as i32).rem_euclid(4);
            let tone = 0.88 + 0.18 * hash01(col, (row as i32).rem_euclid(4), 51);
            let base = rgb(0xc8b48a);
            let mut c = if fu < 0.06 || fv < 0.08 { shade(base, 0.6) } else { shade(base, tone * (0.96 + 0.06 * periodic_noise(u * 16.0, v * 16.0, 16, 52))) };
            if edge_px < 2.6 {
                c = if edge_px < 0.9 { shade(rgb(palette::STONE_DARK), 0.7) } else { rgb(palette::STONE_DARK) };
            }
            c
        }
    }
}

/// Road tile `kind` with connection `mask` (0..16).
pub fn road(kind: &str, mask: u8) -> Canvas {
    let st = style(kind);
    let seed = 600 + kind.len() as u32 * 7;
    let mut c = Canvas::new(64, 32);
    for y in 0..32 {
        for x in 0..64 {
            let (u, v) = tile_uv(x, y);
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                continue;
            }
            let d = road_shape(u, v, mask, &st, seed);
            if d > 0.0 {
                c.set(x, y, road_color(kind, u, v, d, st.half_width));
            }
        }
    }
    c
}

/// Wooden bridge segment with the deck running along `axis` (`A` = tile x, `B` = tile y).
pub fn bridge(axis: Axis) -> Canvas {
    let mut ic = IsoCanvas::new(1, 1, 40);
    let (w0, w1) = (0.2, 0.8);
    let deck_z = 6.0;
    let wood = Mat::Boards(rgb(palette::TIMBER));
    let at = |l: f32, w: f32| if axis == Axis::A { (l, w) } else { (w, l) };
    // Piles in the water (back ones first).
    for &(l, w) in &[(0.25, w0), (0.75, w0), (0.25, w1), (0.75, w1)] {
        let (a, b) = at(l, w);
        ic.block(v3(a - 0.04, b - 0.04, -2.0), v3(a + 0.04, b + 0.04, deck_z), wood, wood);
    }
    let (a0, b0) = at(0.0, w0);
    let (a1, b1) = at(1.0, w1);
    let deck_top = if axis == Axis::A { Mat::Boards(rgb(palette::PLANK)) } else { Mat::BoardsAcross(rgb(palette::PLANK)) };
    let inside = v3((a0 + a1) * 0.5, (b0 + b1) * 0.5, deck_z - 1.5);
    ic.face(&[v3(a0, b0, deck_z), v3(a1, b0, deck_z), v3(a1, b1, deck_z), v3(a0, b1, deck_z)], inside, deck_top);
    // Side beam on the visible long side only (the ends continue into the next segment).
    let beam = Mat::Boards(shade(rgb(palette::TIMBER), 0.9));
    if axis == Axis::A {
        ic.face(&[v3(a0, b1, deck_z - 3.0), v3(a1, b1, deck_z - 3.0), v3(a1, b1, deck_z), v3(a0, b1, deck_z)], inside, beam);
    } else {
        ic.face(&[v3(a1, b0, deck_z - 3.0), v3(a1, b1, deck_z - 3.0), v3(a1, b1, deck_z), v3(a1, b0, deck_z)], inside, beam);
    }
    // Railings: posts and a top rail on both sides.
    let rail_z = deck_z + 8.0;
    for &w in &[w0 + 0.03, w1 - 0.03] {
        for l in [0.08, 0.5, 0.92] {
            let (a, b) = at(l, w);
            props::post(&mut ic, a, b, deck_z, rail_z, 0.035);
        }
        let (pa, pb) = at(0.0, w);
        let (qa, qb) = at(1.0, w);
        ic.line3(v3(pa, pb, rail_z), v3(qa, qb, rail_z), rgb(palette::PLANK_LIGHT));
        ic.line3(v3(pa, pb, rail_z - 1.0), v3(qa, qb, rail_z - 1.0), rgb(palette::TIMBER_DARK));
    }
    ic.finish_trimmed().0
}

const WALL_T: f32 = 0.15;
const WALL_H: f32 = 26.0;

/// One straight wall piece between `(a0, b0)` and `(a1, b1)`; `skip_end` omits the end face on
/// the tile boundary (where the neighbouring segment continues).
fn wall_piece(ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32, h: f32, skip_end: bool) {
    let m = Mat::Stone(rgb(palette::STONE));
    let top = Mat::Rough(rgb(palette::STONE_WARM));
    let c = v3((a0 + a1) * 0.5, (b0 + b1) * 0.5, h * 0.5);
    let along_a = (a1 - a0) > (b1 - b0);
    // Right face (a = a1) is the boundary end for E pieces; left face (b = b1) for S pieces.
    if !(skip_end && along_a) {
        ic.face(&[v3(a1, b0, 0.0), v3(a1, b1, 0.0), v3(a1, b1, h), v3(a1, b0, h)], c, m);
    }
    if !(skip_end && !along_a) {
        ic.face(&[v3(a0, b1, 0.0), v3(a1, b1, 0.0), v3(a1, b1, h), v3(a0, b1, h)], c, m);
    }
    ic.face(&[v3(a0, b0, h), v3(a1, b0, h), v3(a1, b1, h), v3(a0, b1, h)], c, top);
    // Merlons along both long edges.
    let s = 0.09;
    let n = 3;
    for i in 0..n {
        let t = (i as f32 + 0.15) / n as f32;
        if along_a {
            let a = a0 + (a1 - a0) * t;
            ic.block(v3(a, b0, h), v3(a + s, b0 + s * 0.8, h + 5.0), m, top);
            ic.block(v3(a, b1 - s * 0.8, h), v3(a + s, b1, h + 5.0), m, top);
        } else {
            let b = b0 + (b1 - b0) * t;
            ic.block(v3(a0, b, h), v3(a0 + s * 0.8, b + s, h + 5.0), m, top);
            ic.block(v3(a1 - s * 0.8, b, h), v3(a1, b + s, h + 5.0), m, top);
        }
    }
}

/// Stone town wall segment for connection `mask` (same bit scheme as roads).
pub fn wall(mask: u8) -> Canvas {
    let mut ic = IsoCanvas::new(1, 1, 50).with_snow_mask();
    let (lo, hi) = (0.5 - WALL_T, 0.5 + WALL_T);
    ic.plate(0.32, Mat::Earth(rgb(palette::EARTH_DARK)));
    if mask & N != 0 {
        wall_piece(&mut ic, lo, 0.0, hi, 0.5, WALL_H, false);
    }
    if mask & W != 0 {
        wall_piece(&mut ic, 0.0, lo, 0.5, hi, WALL_H, false);
    }
    if mask != (N | S) && mask != (E | W) {
        // Tower at corners, junctions and ends.
        let r = 0.22;
        let m = Mat::Stone(rgb(palette::STONE));
        let top = Mat::Rough(rgb(palette::STONE_WARM));
        ic.block(v3(0.5 - r, 0.5 - r, 0.0), v3(0.5 + r, 0.5 + r, WALL_H + 6.0), m, top);
        ic.crenellations(0.5 - r, 0.5 - r, 0.5 + r, 0.5 + r, WALL_H + 6.0, 0.09, 5.0, m);
        ic.on_wall(Side::Left, 0.5 + r, 0.47, 0.53, WALL_H - 6.0, WALL_H, Mat::Flat(rgb(0x1a1612)));
    }
    if mask & E != 0 {
        wall_piece(&mut ic, 0.5, lo, 1.0, hi, WALL_H, true);
    }
    if mask & S != 0 {
        wall_piece(&mut ic, lo, 0.5, hi, 1.0, WALL_H, true);
    }
    ic.finish_trimmed().0
}

/// Gatehouse in a wall running along `axis` (`A`: wall along tile x, passage along y).
pub fn gate(axis: Axis) -> Canvas {
    let mut ic = IsoCanvas::new(1, 1, 60).with_snow_mask();
    let m = Mat::Stone(rgb(palette::STONE));
    let top = Mat::Rough(rgb(palette::STONE_WARM));
    ic.plate(0.3, Mat::Cobbles(rgb(palette::COBBLE)));
    let (lo, hi) = (0.5 - WALL_T, 0.5 + WALL_T);
    let h = 34.0;
    match axis {
        Axis::A => {
            wall_piece(&mut ic, 0.0, lo, 0.15, hi, WALL_H, false);
            ic.block(v3(0.12, 0.27, 0.0), v3(0.88, 0.73, h), m, top);
            ic.crenellations(0.12, 0.27, 0.88, 0.73, h, 0.09, 5.0, m);
            ic.on_wall(Side::Left, 0.73, 0.34, 0.66, 0.0, 17.0, Mat::Flat(rgb(0x18140f)));
            ic.on_wall(Side::Left, 0.73, 0.38, 0.62, 17.0, 20.0, Mat::Flat(rgb(0x18140f)));
            for k in 0..5 {
                let a = 0.36 + k as f32 * 0.07;
                ic.line3(v3(a, 0.735, 9.0), v3(a, 0.735, 19.0), rgb(palette::IRON));
            }
            ic.line3(v3(0.34, 0.735, 12.0), v3(0.66, 0.735, 12.0), rgb(palette::IRON));
            ic.line3(v3(0.34, 0.735, 16.0), v3(0.66, 0.735, 16.0), rgb(palette::IRON));
            wall_piece(&mut ic, 0.85, lo, 1.0, hi, WALL_H, true);
            props::flag(&mut ic, 0.8, 0.3, h + 5.0, 12.0, rgb(palette::BANNER_BLUE));
        }
        Axis::B => {
            wall_piece(&mut ic, lo, 0.0, hi, 0.15, WALL_H, false);
            ic.block(v3(0.27, 0.12, 0.0), v3(0.73, 0.88, h), m, top);
            ic.crenellations(0.27, 0.12, 0.73, 0.88, h, 0.09, 5.0, m);
            ic.on_wall(Side::Right, 0.73, 0.34, 0.66, 0.0, 17.0, Mat::Flat(rgb(0x18140f)));
            ic.on_wall(Side::Right, 0.73, 0.38, 0.62, 17.0, 20.0, Mat::Flat(rgb(0x18140f)));
            for k in 0..5 {
                let b = 0.36 + k as f32 * 0.07;
                ic.line3(v3(0.735, b, 9.0), v3(0.735, b, 19.0), rgb(palette::IRON));
            }
            ic.line3(v3(0.735, 0.34, 12.0), v3(0.735, 0.66, 12.0), rgb(palette::IRON));
            ic.line3(v3(0.735, 0.34, 16.0), v3(0.735, 0.66, 16.0), rgb(palette::IRON));
            wall_piece(&mut ic, lo, 0.85, hi, 1.0, WALL_H, true);
            props::flag(&mut ic, 0.3, 0.2, h + 5.0, 12.0, rgb(palette::BANNER_BLUE));
        }
    }
    ic.finish_trimmed().0
}

/// Assembles the `roads` atlas.
pub fn atlas() -> AtlasBuilder {
    let mut atlas = AtlasBuilder::new("roads", Sampling::Pixel);
    for kind in ROAD_KINDS {
        for mask in 0..16u8 {
            atlas.add(format!("{kind}_{mask}"), road(kind, mask), (0.5, 0.5));
        }
    }
    atlas.add("bridge_x", bridge(Axis::A), (0.5, 1.0));
    atlas.add("bridge_y", bridge(Axis::B), (0.5, 1.0));
    for mask in 0..16u8 {
        atlas.add(format!("wall_{mask}"), wall(mask), (0.5, 1.0));
    }
    atlas.add("gate_x", gate(Axis::A), (0.5, 1.0));
    atlas.add("gate_y", gate(Axis::B), (0.5, 1.0));
    atlas
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pixel just inside the diamond edge midpoint for each direction bit.
    fn probe(bit: u8) -> (i32, i32) {
        match bit {
            N => (47, 8),
            E => (47, 23),
            S => (16, 23),
            _ => (16, 8),
        }
    }

    #[test]
    fn strips_reach_edge_midpoints_iff_bit_set() {
        for kind in ROAD_KINDS {
            for mask in 0..16u8 {
                let c = road(kind, mask);
                assert!(c.get(32, 16)[3] > 0, "{kind}_{mask}: centre must be covered");
                for bit in [N, E, S, W] {
                    let (x, y) = probe(bit);
                    let covered = c.get(x, y)[3] > 0;
                    assert_eq!(covered, mask & bit != 0, "{kind}_{mask} bit {bit}");
                }
            }
        }
    }

    #[test]
    fn opposite_edges_join_seamlessly() {
        // The texture row at the N edge of one tile must equal the S edge of the tile above.
        for kind in ROAD_KINDS {
            let c = road(kind, N | S);
            // Points on the N edge (v ≈ 0) and S edge (v ≈ 1) with the same u.
            for k in 0..6 {
                let u = 0.42 + k as f32 * 0.03;
                let st = style(kind);
                let d_top = road_shape(u, 0.0, N | S, &st, 600 + kind.len() as u32 * 7);
                let d_bottom = road_shape(u, 1.0, N | S, &st, 600 + kind.len() as u32 * 7);
                assert!((d_top - d_bottom).abs() < 1e-4, "{kind}: widths differ at the seam");
                assert_eq!(road_color(kind, u, 0.0, 0.1, st.half_width), road_color(kind, u, 1.0, 0.1, st.half_width));
            }
            assert!(c.coverage() > 0.1);
        }
    }

    #[test]
    fn tile_uv_maps_corners() {
        let (u, v) = tile_uv(31, 15);
        assert!((u - 0.5).abs() < 0.05 && (v - 0.5).abs() < 0.05);
        let (u, v) = tile_uv(48, 8);
        assert!((u - 0.5).abs() < 0.06 && v.abs() < 0.06, "N edge midpoint {u} {v}");
    }

    #[test]
    fn walls_and_gates_are_upright_tiles() {
        for mask in 0..16u8 {
            let c = wall(mask);
            assert_eq!(c.width(), 64);
            assert!(c.height() > 32);
        }
        assert_ne!(wall(N | S).img, wall(E | W).img);
        assert_ne!(gate(Axis::A).img, gate(Axis::B).img);
        assert_ne!(bridge(Axis::A).img, bridge(Axis::B).img);
    }
}
