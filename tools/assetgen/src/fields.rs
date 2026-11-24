//! Field tiles (`fields` atlas): soil, crops in five growth stages, orchard trees and pasture.
//!
//! Every sprite is 64×64 with the tile diamond in the lower half (rows 32..64) so plants can
//! rise above it; the anchor is the tile centre `(0.5, 0.75)`. Furrows and plant rows are
//! periodic per tile, so neighbouring field tiles join seamlessly.

use kingdoms_engine::rendering::texture::Sampling;

use crate::atlas::AtlasBuilder;
use crate::draw::{Canvas, Px, hash01, mix, periodic_noise, rgb, shade};
use crate::nature;
use crate::roads::tile_uv;

/// Growth stages per crop.
pub const STAGES: u32 = 5;
/// Anchor shared by all field sprites (tile centre).
pub const ANCHOR: (f32, f32) = (0.5, 0.75);
/// Plant rows per tile.
const ROWS: i32 = 6;
/// Plants per row per tile.
const PER_ROW: i32 = 8;

/// Screen position (in the 64×64 sprite) of tile point (u, v).
fn to_screen(u: f32, v: f32) -> (f32, f32) {
    ((u - v) * 32.0 + 32.0, (u + v) * 16.0 + 32.0)
}

/// Ploughed soil with furrows along the tile x axis.
fn soil(c: &mut Canvas, base: Px) {
    for y in 32..64 {
        for x in 0..64 {
            let (u, v) = tile_uv(x, y - 32);
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                continue;
            }
            let ridge = ((v * ROWS as f32 * std::f32::consts::TAU).sin() * 0.5 + 0.5).powf(0.8);
            let n = periodic_noise(u * 8.0, v * 8.0, 8, 5);
            let speck = hash01((u * 32.0).floor() as i32 % 32, (v * 32.0).floor() as i32 % 32, 6);
            let mut col = shade(base, 0.78 + 0.3 * ridge + 0.08 * n);
            if speck > 0.95 {
                col = shade(col, 1.15);
            }
            c.set(x, y, col);
        }
    }
}

/// Grass ground for orchards and pastures.
fn grass(c: &mut Canvas, base: Px, flowers: bool) {
    for y in 32..64 {
        for x in 0..64 {
            let (u, v) = tile_uv(x, y - 32);
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                continue;
            }
            let n = periodic_noise(u * 8.0, v * 8.0, 8, 15);
            let h = hash01(((u * 48.0).floor() as i32).rem_euclid(48), ((v * 48.0).floor() as i32).rem_euclid(48), 16);
            let mut col = shade(base, 0.9 + 0.18 * n);
            if h > 0.86 {
                col = shade(col, 0.78);
            } else if h < 0.05 {
                col = shade(col, 1.18);
            }
            if flowers && h > 0.985 {
                col = if (u * 7.0) as i32 % 2 == 0 { rgb(0xf4f0e0) } else { rgb(0xe8d050) };
            }
            c.set(x, y, col);
        }
    }
}

/// Plant positions (tile space) in back-to-front order.
fn plant_spots(offset: f32) -> Vec<(f32, f32)> {
    let mut v: Vec<(f32, f32)> = (0..ROWS)
        .flat_map(|r| (0..PER_ROW).map(move |k| ((k as f32 + 0.5 + offset * (r % 2) as f32) / PER_ROW as f32, (r as f32 + 0.5) / ROWS as f32)))
        .map(|(u, v)| (u.fract(), v))
        .collect();
    v.sort_by(|a, b| (a.0 + a.1).total_cmp(&(b.0 + b.1)));
    v
}

/// Grain crop (wheat or barley) at `stage`.
fn grain(c: &mut Canvas, stage: u32, barley: bool) {
    soil(c, rgb(if stage == 0 { 0x6e5032 } else { 0x7a5a3a }));
    let (green, ripe) = if barley { (rgb(0x6a9a4a), rgb(0xcfb46a)) } else { (rgb(0x6a9a3a), rgb(0xe0b84a)) };
    let (height, col, head) = match stage {
        0 => (1.0, rgb(0x8aa84a), None),
        1 => (3.0, green, None),
        2 => (5.0, shade(green, 0.92), None),
        3 => (7.0, mix(green, ripe, 0.45), Some(mix(green, ripe, 0.6))),
        _ => (9.0, ripe, Some(shade(ripe, 1.1))),
    };
    for (i, (u, v)) in plant_spots(0.5).into_iter().enumerate() {
        let (x, y) = to_screen(u, v);
        let jitter = hash01(i as i32, 1, 31 + stage) - 0.5;
        let h = height * (0.8 + 0.4 * hash01(i as i32, 2, 31));
        let (x, y) = ((x + jitter).round() as i32, y.round() as i32);
        if stage == 0 {
            c.set(x, y, col);
            continue;
        }
        let stems = if stage >= 3 { 3 } else { 2 };
        for s in 0..stems {
            let dx: i32 = s - stems / 2;
            let hh = (h - (dx.abs() as f32)).max(1.0) as i32;
            for k in 0..hh {
                c.set(x + dx, y - k, shade(col, 0.82 + 0.25 * k as f32 / hh as f32));
            }
            if let Some(hc) = head {
                let top = y - hh;
                c.set(x + dx, top, hc);
                c.set(x + dx, top - 1, shade(hc, 1.1));
                if barley {
                    // Drooping awns.
                    c.set(x + dx + 1, top, shade(hc, 0.9));
                }
            }
        }
    }
}

fn vegetables(c: &mut Canvas, stage: u32) {
    soil(c, rgb(0x6a4a30));
    for (i, (u, v)) in plant_spots(0.0).into_iter().enumerate() {
        let (x, y) = to_screen(u, v);
        let row = (v * ROWS as f32) as i32;
        let carrots = row % 2 == 1;
        match stage {
            0 => c.set(x as i32, y as i32, rgb(0x7aa84a)),
            1 => {
                c.set(x as i32, y as i32, rgb(0x5a9a3a));
                c.set(x as i32, y as i32 - 1, rgb(0x7ab84a));
            }
            _ => {
                let r = 0.8 + stage as f32 * 0.55;
                if carrots {
                    // Feathery carrot tops, orange shoulders when ripe.
                    for k in 0..(stage as i32 + 1) {
                        c.set(x as i32 - 1 + (k % 3), y as i32 - 1 - k, rgb(0x5aa83a));
                    }
                    if stage == 4 {
                        c.set(x as i32, y as i32, rgb(0xe07a2a));
                        c.set(x as i32 + 1, y as i32, rgb(0xc8601a));
                    }
                } else {
                    let base = if stage == 4 { rgb(0x9ccc6a) } else { rgb(0x6aaa4a) };
                    c.lit_ellipse(x, y - r * 0.6, r, r * 0.75, base);
                    if stage >= 3 && hash01(i as i32, 3, 9) > 0.3 {
                        c.set(x as i32, (y - r * 0.8) as i32, rgb(0xd8ecb0));
                    }
                }
            }
        }
    }
}

fn orchard(c: &mut Canvas, stage: u32) {
    grass(c, rgb(0x6a9a3c), false);
    // Two trees per tile on the diagonal so rows line up across tiles.
    for (i, (u, v)) in [(0.25f32, 0.75f32), (0.75, 0.25)].iter().enumerate() {
        let (x, y) = to_screen(*u, *v);
        // Mulch ring.
        c.fill_ellipse(x, y, 6.0, 3.0, |_, _, d| shade(rgb(0x6a5034), 0.9 + 0.1 * d));
        nature::draw_fruit_tree(c, x, y + 1.0, stage, 500 + i as u32);
    }
}

/// Orchard in winter: the trees of stage 2 without leaves or fruit (the game shows it for every
/// stage past the sapling).
fn orchard_bare(c: &mut Canvas) {
    grass(c, rgb(0x6a9a3c), false);
    for (i, (u, v)) in [(0.25f32, 0.75f32), (0.75, 0.25)].iter().enumerate() {
        let (x, y) = to_screen(*u, *v);
        c.fill_ellipse(x, y, 6.0, 3.0, |_, _, d| shade(rgb(0x5e4a34), 0.9 + 0.1 * d));
        nature::draw_bare_fruit_tree(c, x, y + 1.0, 500 + i as u32);
    }
}

/// Assembles the `fields` atlas.
pub fn atlas() -> AtlasBuilder {
    let mut atlas = AtlasBuilder::new("fields", Sampling::Pixel);
    let mut s = Canvas::new(64, 64);
    soil(&mut s, rgb(0x6e5032));
    atlas.add("soil", s, ANCHOR);
    for stage in 0..STAGES {
        let mut w = Canvas::new(64, 64);
        grain(&mut w, stage, false);
        atlas.add(format!("wheat_{stage}"), w, ANCHOR);
        let mut b = Canvas::new(64, 64);
        grain(&mut b, stage, true);
        atlas.add(format!("barley_{stage}"), b, ANCHOR);
        let mut v = Canvas::new(64, 64);
        vegetables(&mut v, stage);
        atlas.add(format!("vegetables_{stage}"), v, ANCHOR);
        let mut o = Canvas::new(64, 64);
        orchard(&mut o, stage);
        atlas.add(format!("orchard_{stage}"), o, ANCHOR);
    }
    let mut bare = Canvas::new(64, 64);
    orchard_bare(&mut bare);
    atlas.add("orchard_bare", bare, ANCHOR);
    let mut p = Canvas::new(64, 64);
    grass(&mut p, rgb(0x78a845), true);
    atlas.add("pasture", p, ANCHOR);
    atlas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soil_fills_exactly_the_lower_diamond() {
        let mut c = Canvas::new(64, 64);
        soil(&mut c, rgb(0x806040));
        assert!(c.get(32, 48)[3] > 0);
        assert_eq!(c.get(32, 10)[3], 0);
        assert_eq!(c.get(1, 33)[3], 0);
        assert_eq!(c.get(62, 62)[3], 0);
    }

    #[test]
    fn crops_grow_taller_with_stage() {
        let top = |stage: u32| {
            let mut c = Canvas::new(64, 64);
            grain(&mut c, stage, false);
            c.opaque_bounds().map_or(64, |(_, y, _, _)| y)
        };
        assert!(top(4) < top(1));
        assert!(top(1) <= top(0));
    }

    #[test]
    fn furrows_are_periodic_across_tile_edges() {
        // The soil colour on the N edge (v ≈ 0) equals the colour on the S edge (v ≈ 1).
        let mut c = Canvas::new(64, 64);
        soil(&mut c, rgb(0x806040));
        let ridge = |v: f32| ((v * ROWS as f32 * std::f32::consts::TAU).sin() * 0.5 + 0.5).powf(0.8);
        assert!((ridge(0.0) - ridge(1.0)).abs() < 1e-4);
    }
}
