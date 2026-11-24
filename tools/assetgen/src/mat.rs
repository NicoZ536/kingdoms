//! Procedural surface materials evaluated in face-local texture coordinates.
//!
//! Coordinates are in pixels: for vertical walls `u` runs horizontally along the wall (32 px per
//! tile, so vertical joints land on whole screen columns) and `v` is the height above the
//! wall's foot; for roof slopes `u` runs along the ridge and `v` down the slope from the ridge;
//! for flat surfaces `(u, v)` are tile coordinates × 32. `ulen` / `vlen` are the face extents,
//! which lets patterns such as half-timbering align with the wall edges.

use std::f32::consts::PI;

use crate::draw::{Px, hash01, mix, rgb, shade, value_noise};

/// A surface material.
#[derive(Clone, Copy, Debug)]
pub enum Mat {
    /// Uniform colour.
    Flat(Px),
    /// Base colour with subtle low-frequency noise (plaster, cloth, painted wood).
    Rough(Px),
    /// Vertical boards with dark seams.
    Boards(Px),
    /// Boards running along `u` (seams every 4 px of `v`) – bridge decks, floors.
    BoardsAcross(Px),
    /// Stacked horizontal logs (log-cabin walls, log piles seen from the side).
    Logs(Px),
    /// Plaster panels between dark timber beams (half-timbering).
    Timber { plaster: Px, beam: Px },
    /// Staggered stone blocks with mortar joints.
    Stone(Px),
    /// Small staggered bricks with light mortar.
    Brick(Px),
    /// Straw thatch in overlapping layers.
    Thatch(Px),
    /// Rounded clay pan tiles.
    Tiles(Px),
    /// Thin rectangular slates.
    Slate(Px),
    /// Wooden shingles.
    Shingles(Px),
    /// Cobblestone paving (flat surfaces).
    Cobbles(Px),
    /// Packed earth with pebbles.
    Earth(Px),
    /// Grass with darker blades.
    Grass(Px),
    /// Water with light ripples.
    Water(Px),
    /// Sawn log ends (circles with growth rings), for the visible end of log piles.
    EndGrain(Px),
    /// Thin stacked planks (plank piles, lumber).
    PlankStack(Px),
    /// Natural rock: irregular facets, cracks and faint strata.
    Rock(Px),
    /// Awning cloth with stripes across `u` (two colours).
    Stripes(Px, Px),
}

impl Mat {
    /// Colour of the material at face coordinates `(u, v)` on a face of size `ulen × vlen`.
    pub fn eval(self, u: f32, v: f32, ulen: f32, vlen: f32) -> Px {
        match self {
            Mat::Flat(c) => c,
            Mat::Rough(c) => shade(c, 0.93 + 0.12 * value_noise(u * 0.35, v * 0.35, 7)),
            Mat::Boards(c) => {
                let w = 4.0;
                let idx = (u / w).floor() as i32;
                let f = u - idx as f32 * w;
                let tone = 0.86 + 0.22 * hash01(idx, 3, 11);
                let grain = 0.94 + 0.08 * value_noise(u * 1.3, v * 0.18, 12);
                let seam = if f < 0.9 { 0.6 } else { 1.0 };
                shade(c, tone * grain * seam)
            }
            Mat::BoardsAcross(c) => Mat::Boards(c).eval(v, u, vlen, ulen),
            Mat::Logs(c) => {
                let h = 4.0;
                let row = (v / h).floor() as i32;
                let f = (v - row as f32 * h) / h;
                let round = 0.7 + 0.42 * (f * PI).sin();
                let gap = if f < 0.14 { 0.55 } else { 1.0 };
                let tone = 0.9 + 0.16 * hash01(row, 5, 13);
                let grain = 0.95 + 0.07 * value_noise(u * 0.3, v, 14);
                shade(c, round * gap * tone * grain)
            }
            Mat::Timber { plaster, beam } => timber(plaster, beam, u, v, ulen, vlen),
            Mat::Stone(c) => masonry(c, u, v, 5.0, 10.0, shade(c, 0.66), 21),
            Mat::Brick(c) => masonry(c, u, v, 3.0, 6.0, mix(c, rgb(0xd6c9b0), 0.55), 23),
            Mat::Thatch(c) => {
                let layer = 6.0;
                let li = (v / layer).floor() as i32;
                let f = (v - li as f32 * layer) / layer;
                let streak = value_noise(u * 0.9 + li as f32 * 3.1, v * 0.12, 31);
                let fine = hash01(u.floor() as i32, li, 32);
                let mut s = 0.8 + 0.24 * streak + 0.08 * fine;
                if f > 0.78 {
                    s *= 0.8;
                }
                shade(c, s)
            }
            Mat::Tiles(c) => {
                let (rh, tw) = (4.0, 5.0);
                let row = (v / rh).floor() as i32;
                let f = (v - row as f32 * rh) / rh;
                let off = if row % 2 == 0 { 0.0 } else { tw * 0.5 };
                let col = ((u + off) / tw).floor() as i32;
                let fu = (u + off) - col as f32 * tw;
                let tone = 0.86 + 0.22 * hash01(col, row, 41);
                let curve = 0.84 + 0.26 * (fu / tw * PI).sin();
                let edge = if f > 0.76 { 0.62 } else { 1.0 };
                shade(c, tone * curve * edge)
            }
            Mat::Slate(c) => {
                let (rh, tw) = (3.5, 6.0);
                let row = (v / rh).floor() as i32;
                let f = (v - row as f32 * rh) / rh;
                let off = if row % 2 == 0 { 0.0 } else { tw * 0.5 };
                let col = ((u + off) / tw).floor() as i32;
                let fu = (u + off) - col as f32 * tw;
                let tone = 0.85 + 0.25 * hash01(col, row, 43);
                let edge = if f > 0.72 || fu < 0.8 { 0.68 } else { 1.0 };
                shade(c, tone * edge)
            }
            Mat::Shingles(c) => {
                let (rh, tw) = (4.0, 4.0);
                let row = (v / rh).floor() as i32;
                let f = (v - row as f32 * rh) / rh;
                let off = if row % 2 == 0 { 0.0 } else { tw * 0.5 };
                let col = ((u + off) / tw).floor() as i32;
                let fu = (u + off) - col as f32 * tw;
                let tone = 0.8 + 0.3 * hash01(col, row, 47);
                let edge = if f > 0.75 { 0.62 } else if fu < 0.8 { 0.78 } else { 1.0 };
                shade(c, tone * edge)
            }
            Mat::Cobbles(c) => {
                let (rh, cw) = (4.0, 5.0);
                let row = (v / rh).floor() as i32;
                let fv = v - row as f32 * rh;
                let off = if row % 2 == 0 { 0.0 } else { 2.5 };
                let col = ((u + off) / cw).floor() as i32;
                let fu = (u + off) - col as f32 * cw;
                if fv < 0.9 || fu < 0.9 {
                    return shade(c, 0.62);
                }
                let tone = 0.84 + 0.26 * hash01(col, row, 51);
                let hl = if fv < 1.9 && fu < 2.4 { 1.12 } else { 1.0 };
                shade(c, tone * hl)
            }
            Mat::Earth(c) => {
                let n = value_noise(u * 0.25, v * 0.25, 61) * 0.6 + value_noise(u * 0.8, v * 0.8, 62) * 0.4;
                let h = hash01(u.floor() as i32, v.floor() as i32, 63);
                let speck = if h > 0.96 { 1.18 } else if h < 0.04 { 0.8 } else { 1.0 };
                shade(c, (0.88 + 0.2 * n) * speck)
            }
            Mat::Grass(c) => {
                let n = value_noise(u * 0.3, v * 0.3, 71);
                let h = hash01(u.floor() as i32, v.floor() as i32, 72);
                let blade = if h > 0.88 { 0.82 } else if h < 0.06 { 1.15 } else { 1.0 };
                shade(c, (0.9 + 0.16 * n) * blade)
            }
            Mat::Water(c) => {
                let w = (u * 0.35 + value_noise(u * 0.1, v * 0.4, 81) * 6.0).sin() * (v * 0.9).sin();
                if w > 0.82 { mix(c, rgb(0xd5ecf5), 0.45) } else { shade(c, 0.95 + 0.08 * value_noise(u * 0.2, v * 0.2, 82)) }
            }
            Mat::EndGrain(c) => {
                let cell = 5.0;
                let row = (v / cell).floor();
                let off = if (row as i32) % 2 == 0 { 0.0 } else { cell * 0.5 };
                let col = ((u + off) / cell).floor();
                let cx = col * cell + cell * 0.5 - off;
                let cy = row * cell + cell * 0.5;
                let d = ((u - cx).powi(2) + (v - cy).powi(2)).sqrt();
                if d > cell * 0.5 {
                    shade(c, 0.45)
                } else if d > cell * 0.5 - 0.8 {
                    rgb(0x5b4030)
                } else if (d * 1.6).fract() < 0.3 {
                    shade(c, 0.85)
                } else {
                    c
                }
            }
            Mat::Rock(c) => {
                let n1 = value_noise(u * 0.12, v * 0.16, 121);
                let n2 = value_noise(u * 0.35, v * 0.4, 122);
                let facet = ((n1 * 5.0).floor() / 5.0 - 0.5) * 0.22;
                let crack = (n2 - 0.5).abs() < 0.025 || ((n1 * 5.0).fract() < 0.06);
                let strata = 0.97 + 0.06 * ((v * 0.5).sin());
                if crack { shade(c, 0.62) } else { shade(c, (0.95 + facet + 0.1 * (n2 - 0.5)) * strata) }
            }
            Mat::Stripes(a, b) => {
                let c = if ((u / 4.0).floor() as i32).rem_euclid(2) == 0 { a } else { b };
                shade(c, 0.95 + 0.07 * value_noise(u * 0.4, v * 0.4, 111))
            }
            Mat::PlankStack(c) => {
                let row = (v / 2.0).floor() as i32;
                let f = v - row as f32 * 2.0;
                let tone = 0.88 + 0.2 * hash01(row, (u / 13.0).floor() as i32, 91);
                shade(c, if f < 0.6 { 0.62 } else { tone })
            }
        }
    }
}

/// Generic staggered masonry (stone blocks or bricks).
fn masonry(c: Px, u: f32, v: f32, rh: f32, bw: f32, mortar: Px, seed: u32) -> Px {
    let row = (v / rh).floor() as i32;
    let fv = v - row as f32 * rh;
    let off = if row % 2 == 0 { 0.0 } else { bw * 0.5 };
    let uu = u + off;
    let col = (uu / bw).floor() as i32;
    let fu = uu - col as f32 * bw;
    if fv < 0.9 || fu < 0.9 {
        return mortar;
    }
    let tone = 0.86 + 0.22 * hash01(col, row, seed);
    let n = 0.96 + 0.08 * value_noise(u * 0.7, v * 0.7, seed + 1);
    let hl = if fv > rh - 1.2 { 1.08 } else { 1.0 };
    shade(c, tone * n * hl)
}

/// Half-timbering: posts at the wall ends and every ~15 px, sill / top plate / mid rail and
/// diagonal braces in the outer bays.
fn timber(plaster: Px, beam: Px, u: f32, v: f32, ulen: f32, vlen: f32) -> Px {
    let beam_px = |s: f32| shade(beam, s * (0.92 + 0.12 * value_noise(u * 0.5, v * 0.5, 101)));
    let n = (ulen / 15.0).round().max(1.0);
    let bay = ulen / n;
    let bi = (u / bay).floor().clamp(0.0, n - 1.0);
    let fu = u - bi * bay;
    let dpost = fu.min(bay - fu);
    if dpost < 1.0 || v < 2.0 || v > vlen - 1.6 {
        return beam_px(1.0);
    }
    let mid = vlen > 15.0;
    if mid && (v - vlen * 0.5).abs() < 1.0 {
        return beam_px(1.0);
    }
    // Braces in the first and last bay of the lower storey.
    let top = if mid { vlen * 0.5 } else { vlen };
    if v < top && (bi == 0.0 || bi == n - 1.0) && n >= 2.0 {
        let t = (v - 2.0) / (top - 2.0);
        let target = if bi == 0.0 { t * bay } else { bay - t * bay };
        if (fu - target).abs() < 1.1 {
            return beam_px(0.9);
        }
    }
    shade(plaster, 0.93 + 0.1 * value_noise(u * 0.3, v * 0.3, 102))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materials_are_opaque_and_deterministic() {
        let base = rgb(0x998877);
        let mats = [
            Mat::Flat(base),
            Mat::Rough(base),
            Mat::Boards(base),
            Mat::BoardsAcross(base),
            Mat::Logs(base),
            Mat::Timber { plaster: base, beam: rgb(0x332211) },
            Mat::Stone(base),
            Mat::Brick(base),
            Mat::Thatch(base),
            Mat::Tiles(base),
            Mat::Slate(base),
            Mat::Shingles(base),
            Mat::Cobbles(base),
            Mat::Earth(base),
            Mat::Grass(base),
            Mat::Water(base),
            Mat::EndGrain(base),
            Mat::PlankStack(base),
            Mat::Stripes(base, rgb(0xffffff)),
            Mat::Rock(base),
        ];
        for m in mats {
            for i in 0..40 {
                let (u, v) = (i as f32 * 1.7, i as f32 * 0.9);
                let p = m.eval(u, v, 64.0, 24.0);
                assert_eq!(p[3], 255, "{m:?}");
                assert_eq!(p, m.eval(u, v, 64.0, 24.0));
            }
        }
    }

    #[test]
    fn timber_frame_has_posts_at_wall_edges() {
        let plaster = rgb(0xeeeeee);
        let beam = rgb(0x202020);
        let m = Mat::Timber { plaster, beam };
        let at_edge = m.eval(0.3, 8.0, 60.0, 20.0);
        let in_panel = m.eval(22.0, 6.5, 60.0, 20.0);
        assert!(at_edge[0] < 80, "post expected at the wall edge");
        assert!(in_panel[0] > 180, "plaster expected inside a panel");
    }
}
