//! Small isometric props shared by building sprites (barrels, crates, piles, fences, flags,
//! trees, figures). All functions draw into the current layer of an [`IsoCanvas`] at
//! footprint-local positions; callers draw back-to-front.

use crate::characters;
use crate::draw::{Canvas, Px, rgb, shade};
use crate::iso::{Axis, IsoCanvas, Side, v3};
use crate::mat::Mat;
use crate::nature::{self, Season, Tree};
use crate::palette;

/// Wooden barrel with iron hoops standing at (a, b) on height `z`.
pub fn barrel(ic: &mut IsoCanvas, a: f32, b: f32, z: f32) {
    let r = 0.075;
    ic.frustum(a, b, r, r, z, z + 9.0, Mat::Boards(rgb(palette::PLANK)));
    ic.disc(a, b, r * 0.85, z + 9.0, Mat::Flat(shade(rgb(palette::PLANK), 0.7)));
    for hz in [1.5, 6.5] {
        ic.tube(a, b, r + 0.006, r + 0.006, z + hz, z + hz + 1.0, Mat::Flat(rgb(palette::IRON)));
    }
}

/// Wooden crate with edge length `s` tiles and height `h` px.
pub fn crate_box(ic: &mut IsoCanvas, a: f32, b: f32, s: f32, h: f32, z: f32) {
    ic.block(v3(a, b, z), v3(a + s, b + s, z + h), Mat::Boards(rgb(palette::PLANK_LIGHT)), Mat::Boards(shade(rgb(palette::PLANK_LIGHT), 0.95)));
}

/// Cloth sack (2D blob) resting on the ground at (a, b, z).
pub fn sack(ic: &mut IsoCanvas, a: f32, b: f32, z: f32, color: Px) {
    let (x, y) = ic.project(v3(a, b, z));
    let c = ic.canvas();
    c.lit_ellipse(x, y - 3.0, 3.6, 3.2, color);
    c.fill_rect(x as i32 - 1, (y - 6.5) as i32, 2, 1, shade(color, 0.6));
}

/// Stack of logs lying along `axis`, occupying `(a0, b0)–(a1, b1)`, `h` px high.
pub fn log_pile(ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32, h: f32, axis: Axis) {
    let side = Mat::Logs(rgb(palette::LOG));
    let ends = Mat::EndGrain(rgb(palette::END_GRAIN));
    let top = Mat::Rough(rgb(palette::BARK));
    match axis {
        Axis::A => ic.block2(v3(a0, b0, 0.0), v3(a1, b1, h), side, ends, top),
        Axis::B => ic.block2(v3(a0, b0, 0.0), v3(a1, b1, h), ends, side, top),
    }
}

/// Pile of sawn planks.
pub fn plank_stack(ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32, h: f32) {
    let m = Mat::PlankStack(rgb(palette::PLANK_LIGHT));
    ic.block(v3(a0, b0, 0.0), v3(a1, b1, h), m, Mat::Boards(rgb(palette::PLANK_LIGHT)));
}

/// A few cut stone blocks.
pub fn stone_blocks(ic: &mut IsoCanvas, a: f32, b: f32) {
    let m = Mat::Stone(rgb(palette::STONE_WARM));
    let top = Mat::Rough(shade(rgb(palette::STONE_WARM), 1.08));
    ic.block(v3(a, b, 0.0), v3(a + 0.16, b + 0.16, 6.0), m, top);
    ic.block(v3(a + 0.2, b, 0.0), v3(a + 0.36, b + 0.16, 6.0), m, top);
    ic.block(v3(a + 0.1, b + 0.02, 6.0), v3(a + 0.26, b + 0.16, 11.0), m, top);
}

/// Rounded heap (ore, coal, sand, clay, hay) of radius `r` tiles and height `h` px.
pub fn heap(ic: &mut IsoCanvas, a: f32, b: f32, r: f32, h: f32, mat: Mat) {
    ic.dome(a, b, r, 0.0, h, mat);
}

/// Conical haystack.
pub fn haystack(ic: &mut IsoCanvas, a: f32, b: f32, r: f32, h: f32) {
    let straw = Mat::Thatch(rgb(palette::STRAW));
    ic.tube(a, b, r, r * 0.95, 0.0, h * 0.45, straw);
    ic.cone(a, b, r * 1.02, h * 0.45, h, straw);
}

/// Post-and-rail fence from (a0, b0) to (a1, b1).
pub fn fence(ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32, h: f32) {
    let len = ((a1 - a0).powi(2) + (b1 - b0).powi(2)).sqrt();
    let n = ((len / 0.24).round() as usize).max(1);
    let wood = Mat::Flat(rgb(palette::TIMBER));
    for rail in [h * 0.4, h * 0.85] {
        ic.line3(v3(a0, b0, rail), v3(a1, b1, rail), rgb(palette::PLANK));
        ic.line3(v3(a0, b0, rail - 1.0), v3(a1, b1, rail - 1.0), rgb(palette::TIMBER));
    }
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let (a, b) = (a0 + (a1 - a0) * t, b0 + (b1 - b0) * t);
        ic.block(v3(a - 0.015, b - 0.015, 0.0), v3(a + 0.015, b + 0.015, h + 1.0), wood, wood);
    }
}

/// Vertical wooden post (square section `w` tiles).
pub fn post(ic: &mut IsoCanvas, a: f32, b: f32, z0: f32, z1: f32, w: f32) {
    let m = Mat::Boards(rgb(palette::TIMBER));
    ic.block(v3(a - w * 0.5, b - w * 0.5, z0), v3(a + w * 0.5, b + w * 0.5, z1), m, m);
}

/// Square chimney from `z0` to `z1`; its top is recorded as a smoke point. A warm chimney
/// takes no snow.
pub fn chimney(ic: &mut IsoCanvas, a: f32, b: f32, z0: f32, z1: f32, mat: Mat) {
    ic.snow_free(|ic| {
        ic.block(v3(a, b, z0), v3(a + 0.14, b + 0.14, z1), mat, Mat::Flat(rgb(0x2a2420)));
        ic.block(v3(a - 0.015, b - 0.015, z1 - 2.0), v3(a + 0.155, b + 0.155, z1), mat, Mat::Flat(rgb(0x2a2420)));
    });
    ic.mark_smoke(v3(a + 0.07, b + 0.07, z1));
}

/// Flag pole of height `h` with a waving flag in `color` at its top.
pub fn flag(ic: &mut IsoCanvas, a: f32, b: f32, z: f32, h: f32, color: Px) {
    let (x0, y0) = ic.project(v3(a, b, z));
    let (_, y1) = ic.project(v3(a, b, z + h));
    let c = ic.canvas();
    c.linef(x0, y0, x0, y1, rgb(palette::TIMBER_DARK));
    c.set(x0 as i32, y1 as i32 - 1, rgb(palette::GOLD));
    let pts = [(x0 + 1.0, y1), (x0 + 11.0, y1 + 1.5), (x0 + 9.0, y1 + 4.0), (x0 + 11.0, y1 + 6.5), (x0 + 1.0, y1 + 6.0)];
    c.fill_polygon(&pts, |x, _| shade(color, if (x as f32) < x0 + 5.0 { 1.05 } else { 0.88 }));
    c.fill_rect(x0 as i32 + 2, y1 as i32 + 2, 3, 2, rgb(palette::GOLD));
}

/// Hanging sign on a bracket fixed to a wall, showing a small symbol colour.
pub fn wall_sign(ic: &mut IsoCanvas, side: Side, plane: f32, c: f32, z: f32, symbol: Px) {
    let p = match side {
        Side::Left => v3(c, plane + 0.12, z),
        Side::Right => v3(plane + 0.12, c, z),
    };
    let base = match side {
        Side::Left => v3(c, plane, z + 2.0),
        Side::Right => v3(plane, c, z + 2.0),
    };
    ic.line3(base, v3(p.a, p.b, z + 2.0), rgb(palette::IRON));
    let (x, y) = ic.project(p);
    let cv = ic.canvas();
    cv.fill_rect(x as i32 - 3, y as i32 - 1, 7, 6, rgb(palette::PLANK));
    cv.fill_rect(x as i32 - 2, y as i32, 5, 4, symbol);
}

/// Tree from the nature set at (a, b).
pub fn tree(ic: &mut IsoCanvas, a: f32, b: f32, kind: Tree, scale: f32, seed: u32) {
    let w = (60.0 * scale) as u32 + 8;
    let h = (80.0 * scale) as u32 + 8;
    let mut c = Canvas::new(w, h);
    nature::draw_tree(&mut c, w as f32 * 0.5, h as f32, kind, Season::Summer, scale, seed);
    ic.stamp(&c, v3(a, b, 0.0));
}

/// Fruit tree (orchard) at growth stage 0..=4.
pub fn fruit_tree(ic: &mut IsoCanvas, a: f32, b: f32, stage: u32, seed: u32) {
    let mut c = Canvas::new(32, 34);
    nature::draw_fruit_tree(&mut c, 16.0, 34.0, stage, seed);
    ic.stamp(&c, v3(a, b, 0.0));
}

/// Leafless fruit tree (orchard in winter) at (a, b).
pub fn bare_fruit_tree(ic: &mut IsoCanvas, a: f32, b: f32, seed: u32) {
    let mut c = Canvas::new(32, 34);
    nature::draw_bare_fruit_tree(&mut c, 16.0, 34.0, seed);
    ic.stamp(&c, v3(a, b, 0.0));
}

/// Bush at (a, b).
pub fn bush(ic: &mut IsoCanvas, a: f32, b: f32, scale: f32, seed: u32) {
    let mut c = Canvas::new(30, 22);
    nature::draw_bush(&mut c, 15.0, 22.0, Season::Summer, scale, seed);
    ic.stamp(&c, v3(a, b, 0.0));
}

/// Farm animals painted into building sprites. People are never painted into buildings: on the
/// map they would look like walkers one could click (the game draws the real citizens).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Animal {
    Cow,
    Sheep,
    Chicken,
}

impl Animal {
    fn role(self) -> &'static str {
        match self {
            Animal::Cow => "cow",
            Animal::Sheep => "sheep",
            Animal::Chicken => "chicken",
        }
    }
}

/// An animal from the characters set, standing at (a, b).
pub fn animal(ic: &mut IsoCanvas, a: f32, b: f32, animal: Animal, mirrored: bool) {
    let dir = if mirrored { characters::Dir::Sw } else { characters::Dir::Se };
    let (f, (ax, ay)) = characters::sprite(animal.role(), dir, 1);
    let (x, y) = ic.project(v3(a, b, 0.0));
    let px = (x - f.width() as f32 * ax).round() as i32;
    let py = (y - f.height() as f32 * ay).round() as i32;
    ic.canvas().blit(&f, px, py);
}

/// Sloped awning (cloth roof) from a high back edge down towards +b (`Side::Left`) or +a
/// (`Side::Right`), with stripes.
#[allow(clippy::too_many_arguments)]
pub fn awning(ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32, z_high: f32, z_low: f32, side: Side, c1: Px, c2: Px) {
    let m = Mat::Stripes(c1, c2);
    let inside = v3((a0 + a1) * 0.5, (b0 + b1) * 0.5, z_low - 20.0);
    match side {
        Side::Left => ic.face(&[v3(a0, b0, z_high), v3(a1, b0, z_high), v3(a1, b1, z_low), v3(a0, b1, z_low)], inside, m),
        Side::Right => ic.face(&[v3(a0, b0, z_high), v3(a0, b1, z_high), v3(a1, b1, z_low), v3(a1, b0, z_low)], inside, m),
    };
    // Scalloped valance along the low edge.
    let (p, q) = match side {
        Side::Left => (v3(a0, b1, z_low), v3(a1, b1, z_low)),
        Side::Right => (v3(a1, b1, z_low), v3(a1, b0, z_low)),
    };
    let n = 6;
    for i in 0..n {
        let t0 = i as f32 / n as f32;
        let pt = v3(p.a + (q.a - p.a) * t0, p.b + (q.b - p.b) * t0, z_low);
        let (x, y) = ic.project(pt);
        let col = if i % 2 == 0 { c1 } else { c2 };
        ic.canvas().fill_ellipse(x + 2.0, y + 0.5, 2.6, 1.6, |_, _, _| shade(col, 0.85));
    }
}

/// Vegetable / herb bed with rows of plants (ground decal plus small plant dots).
pub fn bed(ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32, plant: Px, rows_along: Axis) {
    ic.ground_rect(a0, b0, a1, b1, 0.0, Mat::Earth(rgb(palette::EARTH_DARK)));
    let n = match rows_along {
        Axis::A => ((b1 - b0) / 0.14) as i32,
        Axis::B => ((a1 - a0) / 0.14) as i32,
    };
    for r in 0..n.max(1) {
        let t = (r as f32 + 0.5) / n.max(1) as f32;
        let steps = 7;
        for k in 0..steps {
            let s = (k as f32 + 0.5) / steps as f32;
            let (a, b) = match rows_along {
                Axis::A => (a0 + (a1 - a0) * s, b0 + (b1 - b0) * t),
                Axis::B => (a0 + (a1 - a0) * t, b0 + (b1 - b0) * s),
            };
            let (x, y) = ic.project(v3(a, b, 0.0));
            let cv = ic.canvas();
            cv.lit_ellipse(x, y - 1.0, 1.8, 1.4, plant);
        }
    }
}

/// Wooden drying rack with items hanging from it (hides, fish, cloth).
pub fn rack(ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32, h: f32, items: &[Px]) {
    post(ic, a0, b0, 0.0, h, 0.03);
    post(ic, a1, b1, 0.0, h, 0.03);
    ic.line3(v3(a0, b0, h), v3(a1, b1, h), rgb(palette::TIMBER));
    let n = items.len().max(1);
    for (i, col) in items.iter().enumerate() {
        let t = (i as f32 + 0.5) / n as f32;
        let (a, b) = (a0 + (a1 - a0) * t, b0 + (b1 - b0) * t);
        let (x, y) = ic.project(v3(a, b, h));
        let cv = ic.canvas();
        cv.fill_rect(x as i32 - 2, y as i32 + 1, 4, (h * 0.55) as i32, *col);
        cv.fill_rect(x as i32 - 2, y as i32 + 1, 1, (h * 0.55) as i32, shade(*col, 1.15));
    }
}

/// Round straw archery target on a stand.
pub fn target(ic: &mut IsoCanvas, a: f32, b: f32) {
    let (x, y) = ic.project(v3(a, b, 0.0));
    let c = ic.canvas();
    c.linef(x - 3.0, y, x, y - 9.0, rgb(palette::TIMBER));
    c.linef(x + 3.0, y, x, y - 9.0, rgb(palette::TIMBER));
    c.fill_ellipse(x, y - 9.0, 4.5, 5.0, |_, _, _| rgb(palette::STRAW));
    c.fill_ellipse(x, y - 9.0, 3.0, 3.4, |_, _, _| rgb(0xf0ead8));
    c.fill_ellipse(x, y - 9.0, 1.6, 1.8, |_, _, _| rgb(palette::BANNER_RED));
}

/// Straw training dummy on a post.
pub fn dummy(ic: &mut IsoCanvas, a: f32, b: f32) {
    let (x, y) = ic.project(v3(a, b, 0.0));
    let c = ic.canvas();
    c.fill_rect(x as i32, (y - 14.0) as i32, 1, 14, rgb(palette::TIMBER));
    c.fill_rect(x as i32 - 4, (y - 11.0) as i32, 9, 1, rgb(palette::TIMBER));
    c.lit_ellipse(x + 0.5, y - 9.0, 2.6, 3.6, rgb(palette::STRAW));
    c.lit_ellipse(x + 0.5, y - 14.0, 2.0, 2.0, rgb(palette::STRAW));
}

/// Beehive skep (straw dome) on a small stand.
pub fn skep(ic: &mut IsoCanvas, a: f32, b: f32) {
    let wood = Mat::Boards(rgb(palette::TIMBER));
    ic.block(v3(a - 0.1, b - 0.1, 0.0), v3(a + 0.1, b + 0.1, 3.0), wood, wood);
    ic.dome(a, b, 0.09, 3.0, 9.0, Mat::Thatch(rgb(palette::STRAW)));
    let (x, y) = ic.project(v3(a + 0.06, b + 0.06, 4.5));
    ic.canvas().fill_rect(x as i32 - 1, y as i32 - 1, 2, 1, rgb(0x2a2018));
}
