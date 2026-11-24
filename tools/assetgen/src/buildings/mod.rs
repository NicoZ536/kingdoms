//! Building sprites (`buildings` atlas): one finished sprite per building ID, construction
//! stages per footprint and rubble.
//!
//! Footprint convention (docs/assets.md): a building of `B×T` tiles is `(B + T) · 32` px wide,
//! its bottom edge is the front corner and the anchor is `(T / (B + T), 1.0)`. In the iso
//! canvas the `T` dimension therefore runs along `a` (the lit, lower-left facing side) and `B`
//! along `b`.

mod agriculture;
mod military;
mod production;

pub use agriculture::WINTER_VARIANTS;
pub use production::{WORK_FRAMES, WORK_LOOPS};
mod public;
mod residential;

use std::collections::BTreeMap;

use kingdoms_engine::rendering::texture::Sampling;

use crate::atlas::AtlasBuilder;
use crate::catalog;
use crate::draw::{Canvas, hash01, rgb, rgba, shade};
use crate::iso::{Axis, IsoCanvas, Pane, Roof, Side, v3};
use crate::mat::Mat;
use crate::palette::*;
use crate::props;

/// Pixels reserved above the back corner before trimming (towers, spires, sails).
const HEADROOM: u32 = 220;

/// Iso canvas for a `B×T` footprint (`T` along `a`, `B` along `b`), with the snow mask
/// (roofs and flat tops take snow in winter).
pub fn canvas(b: u32, t: u32) -> IsoCanvas {
    IsoCanvas::new(t, b, HEADROOM).with_snow_mask()
}

// --- Material shorthands -------------------------------------------------------------------

pub(crate) fn logs() -> Mat {
    Mat::Logs(rgb(LOG))
}
pub(crate) fn boards(c: u32) -> Mat {
    Mat::Boards(rgb(c))
}
pub(crate) fn timber(plaster: u32) -> Mat {
    Mat::Timber { plaster: rgb(plaster), beam: rgb(TIMBER) }
}
pub(crate) fn stone(c: u32) -> Mat {
    Mat::Stone(rgb(c))
}
pub(crate) fn thatch(c: u32) -> Mat {
    Mat::Thatch(rgb(c))
}
pub(crate) fn tiles(c: u32) -> Mat {
    Mat::Tiles(rgb(c))
}
pub(crate) fn slate(c: u32) -> Mat {
    Mat::Slate(rgb(c))
}
pub(crate) fn shingles() -> Mat {
    Mat::Shingles(rgb(SHINGLE))
}
pub(crate) fn flat(c: u32) -> Mat {
    Mat::Flat(rgb(c))
}

/// Gable-end material matching a wall material.
fn gable_for(wall: Mat) -> Mat {
    match wall {
        Mat::Logs(_) => boards(PLANK),
        other => other,
    }
}

/// Walls plus gable roof over `r = [a0, b0, a1, b1]`, with cast shadow. Returns the roof so
/// chimneys can be seated on it.
pub(crate) fn house(ic: &mut IsoCanvas, r: [f32; 4], h: f32, wall: Mat, roof: Mat, rise: f32, ridge: Axis) -> Roof {
    let [a0, b0, a1, b1] = r;
    ic.shadow(a0, b0, a1, b1, h + rise * 0.7);
    ic.block(v3(a0, b0, 0.0), v3(a1, b1, h), wall, wall);
    let gable = Roof::new(r, h, rise, ridge, roof, gable_for(wall));
    ic.gable(gable);
    gable
}

/// Walls plus hip roof.
pub(crate) fn hip_house(ic: &mut IsoCanvas, r: [f32; 4], h: f32, wall: Mat, roof: Mat, rise: f32) {
    let [a0, b0, a1, b1] = r;
    ic.shadow(a0, b0, a1, b1, h + rise * 0.7);
    ic.block(v3(a0, b0, 0.0), v3(a1, b1, h), wall, wall);
    ic.hip(Roof::new([a0, b0, a1, b1], h, rise, Axis::A, roof, wall));
}

/// Chimney seated on gable roof `roof` at (a, b), rising `above` px over the ridge.
pub(crate) fn roof_chimney(ic: &mut IsoCanvas, roof: &Roof, a: f32, b: f32, above: f32, mat: Mat) {
    // Seat at the lowest of the visible front corners so no gap shows.
    let z0 = roof.height_at(a + 0.14, b + 0.14).min(roof.height_at(a, b + 0.14)).min(roof.height_at(a + 0.14, b));
    props::chimney(ic, a, b, z0 - 1.0, roof.z + roof.rise + above, mat);
}

/// Open shed: four posts and a gable roof without walls. `contents` is drawn between the back
/// and front posts.
pub(crate) fn shed(ic: &mut IsoCanvas, r: [f32; 4], h: f32, roof: Mat, rise: f32, ridge: Axis, contents: impl FnOnce(&mut IsoCanvas)) {
    let [a0, b0, a1, b1] = r;
    ic.shadow(a0, b0, a1, b1, h);
    let w = 0.05;
    props::post(ic, a0 + w, b0 + w, 0.0, h, w);
    props::post(ic, a1 - w, b0 + w, 0.0, h, w);
    props::post(ic, a0 + w, b1 - w, 0.0, h, w);
    contents(ic);
    props::post(ic, a1 - w, b1 - w, 0.0, h, w);
    ic.gable(Roof::new([a0, b0, a1, b1], h, rise, ridge, roof, boards(PLANK)));
}

/// Mono-pitch roof over `(a0, b0)–(a1, b1)` falling from `z_high` towards `side`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lean_to(ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32, z_high: f32, z_low: f32, side: Side, roof: Mat) {
    let inside = v3((a0 + a1) * 0.5, (b0 + b1) * 0.5, z_low - 30.0);
    match side {
        Side::Right => ic.face(&[v3(a0, b0, z_high), v3(a0, b1, z_high), v3(a1, b1, z_low), v3(a1, b0, z_low)], inside, roof),
        Side::Left => ic.face(&[v3(a0, b0, z_high), v3(a1, b0, z_high), v3(a1, b1, z_low), v3(a0, b1, z_low)], inside, roof),
    };
}

/// Rocky hill with a timbered mine entrance, rails and a loaded mine cart.
pub(crate) fn mine(ic: &mut IsoCanvas, rock: u32, ore: u32, heap_mat: Mat) {
    ic.plate(0.04, Mat::Earth(rgb(EARTH_DARK)));
    let rock_m = Mat::Rock(rgb(rock));
    ic.shadow(0.2, 0.2, 1.4, 1.3, 30.0);
    ic.dome(0.75, 0.7, 0.62, 0.0, 34.0, rock_m);
    ic.dome(1.35, 0.45, 0.42, 0.0, 22.0, rock_m);
    ic.dome(0.45, 1.25, 0.38, 0.0, 18.0, rock_m);
    // Grass and shrubs on top of the hill.
    ic.dome(0.7, 0.62, 0.36, 22.0, 12.0, Mat::Grass(rgb(GRASS)));
    for i in 0..4 {
        let (x, y) = ic.project(v3(0.45 + hash01(i, 1, 5) * 0.55, 0.45 + hash01(i, 2, 5) * 0.35, 24.0 + hash01(i, 3, 5) * 6.0));
        ic.canvas().lit_ellipse(x, y, 3.0, 2.2, rgb(GRASS_DARK));
    }
    // Entrance: dark opening framed by timber, facing +b (lit side).
    let (e0, e1, eb) = (0.85, 1.25, 1.2);
    ic.block(v3(e0 - 0.08, eb - 0.25, 0.0), v3(e1 + 0.08, eb, 15.0), rock_m, rock_m);
    ic.on_wall(Side::Left, eb, e0, e1, 0.0, 12.0, Mat::Flat(rgb(0x14100c)));
    props::post(ic, e0 - 0.03, eb + 0.02, 0.0, 13.0, 0.05);
    props::post(ic, e1 + 0.03, eb + 0.02, 0.0, 13.0, 0.05);
    ic.block(v3(e0 - 0.1, eb - 0.02, 12.0), v3(e1 + 0.1, eb + 0.06, 15.0), boards(TIMBER), boards(TIMBER));
    // Rails towards the front.
    for off in [0.95, 1.15] {
        ic.set_layer(crate::iso::Layer::Ground);
        ic.line3(v3(off, eb, 0.0), v3(off, 1.95, 0.0), rgb(IRON));
        ic.set_layer(crate::iso::Layer::Objects);
    }
    // Ore heap at the right, cart on the rails.
    props::heap(ic, 1.6, 1.05, 0.3, 12.0, heap_mat);
    let cart = boards(TIMBER);
    ic.block(v3(0.92, 1.45, 2.0), v3(1.18, 1.72, 9.0), cart, Mat::Flat(rgb(ore)));
    let (x, y) = ic.project(v3(1.05, 1.58, 9.5));
    ic.canvas().lit_ellipse(x, y, 4.5, 2.2, rgb(ore));
    props::post(ic, 1.75, 1.75, 0.0, 14.0, 0.04);
    let (lx, ly) = ic.project(v3(1.75, 1.75, 14.0));
    ic.canvas().fill_rect(lx as i32 - 1, ly as i32, 3, 3, rgb(GLOW));
}

/// Renders the finished sprite of building `id`, or `None` for unknown IDs and road-like
/// segments (`wall`, `gate`).
pub fn render(id: &str) -> Option<(Canvas, (f32, f32))> {
    render_with_smoke(id).map(|(c, anchor, _)| (c, anchor))
}

/// A rendered sprite: canvas, normalised anchor and smoke points (pixel offsets from the anchor).
pub type SmokySprite = (Canvas, (f32, f32), Vec<(f32, f32)>);

/// Draws building `id` into a fresh canvas (`None` for unknown IDs and road-like segments).
fn draw_building(id: &str) -> Option<IsoCanvas> {
    let info = catalog::BUILDINGS.iter().find(|b| b.id == id && !b.road_like)?;
    let mut ic = canvas(info.b, info.t);
    let drawn = residential::draw(id, &mut ic)
        || production::draw(id, &mut ic)
        || agriculture::draw(id, &mut ic)
        || public::draw(id, &mut ic)
        || military::draw(id, &mut ic);
    drawn.then_some(ic)
}

/// Frame `frame` of the work loop of building `id` (`None` for buildings without one, see
/// [`WORK_LOOPS`]); same width and anchor as the finished sprite.
pub fn render_work_frame(id: &str, frame: u32) -> Option<(Canvas, (f32, f32))> {
    let info = catalog::BUILDINGS.iter().find(|b| b.id == id && !b.road_like)?;
    let mut ic = canvas(info.b, info.t);
    production::draw_work_frame(id, &mut ic, frame).then(|| ic.finish_trimmed())
}

/// The winter variant of building `id` (`None` for buildings without one, see
/// [`WINTER_VARIANTS`]); same width and anchor as the finished sprite.
pub fn render_winter(id: &str) -> Option<(Canvas, (f32, f32))> {
    let info = catalog::BUILDINGS.iter().find(|b| b.id == id && !b.road_like)?;
    let mut ic = canvas(info.b, info.t);
    agriculture::draw_winter(id, &mut ic).then(|| ic.finish_trimmed())
}

/// Like [`render`], plus the smoke points of the sprite (see [`IsoCanvas::smoke_points`]).
pub fn render_with_smoke(id: &str) -> Option<SmokySprite> {
    let ic = draw_building(id)?;
    let smoke = ic.smoke_points();
    let (c, anchor) = ic.finish_trimmed();
    Some((c, anchor, smoke))
}

/// Light of a window pane at night (bottom row) and towards its top.
const LIGHT_LOW: u32 = 0xffd88a;
const LIGHT_HIGH: u32 = 0xf6a948;
/// Colour of the glow around lit panes.
const LIGHT_HALO: u32 = 0xffb35a;
/// Halo alpha by distance (pixels) from the nearest lit pane pixel.
const HALO_ALPHA: [u8; 3] = [120, 56, 20];

/// True if `px` is glazing of colour `glass` under any of the iso canvas' face lights.
fn is_glazing(px: [u8; 4], glass: u32) -> bool {
    let [r, g, b, a] = px;
    let w = rgb(glass);
    if a < 250 || w[2] == 0 {
        return false;
    }
    let f = f32::from(b) / f32::from(w[2]);
    let near = |c: u8, base: u8| (f32::from(c) - f32::from(base) * f).abs() <= 2.0;
    (0.4..=1.15).contains(&f) && near(r, w[0]) && near(g, w[1])
}

/// The window panes of `sprite` that are still visible (inside the recorded `panes`, not
/// covered by later geometry) in warm light with a soft halo; the rest is transparent.
pub fn light_mask(sprite: &Canvas, panes: &[Pane]) -> Canvas {
    let (w, h) = (sprite.width(), sprite.height());
    let mut lit = vec![false; (w * h) as usize];
    let mut out = Canvas::new(w as u32, h as u32);
    for pane in panes {
        let (x0, y0, x1, y1) = pane.bounds;
        let (top, bottom) = (y0.floor() as i32, y1.ceil() as i32);
        for y in top..=bottom {
            for x in x0.floor() as i32..=x1.ceil() as i32 {
                if x < 0 || y < 0 || x >= w || y >= h || !is_glazing(sprite.get(x, y), pane.color) {
                    continue;
                }
                lit[(y * w + x) as usize] = true;
                let t = if bottom > top { (y - top) as f32 / (bottom - top) as f32 } else { 1.0 };
                out.set(x, y, crate::draw::mix(rgb(LIGHT_HIGH), rgb(LIGHT_LOW), t));
            }
        }
    }
    let reach = HALO_ALPHA.len() as i32;
    for y in 0..h {
        for x in 0..w {
            if lit[(y * w + x) as usize] {
                continue;
            }
            let mut nearest = i32::MAX;
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx >= 0 && ny >= 0 && nx < w && ny < h && lit[(ny * w + nx) as usize] {
                        nearest = nearest.min(dx.abs().max(dy.abs()));
                    }
                }
            }
            if let Some(&alpha) = usize::try_from(nearest - 1).ok().and_then(|i| HALO_ALPHA.get(i)) {
                out.set(x, y, rgba(LIGHT_HALO, alpha));
            }
        }
    }
    out
}

/// Night light of building `id` (`lights` atlas): the visible window panes in warm light with a
/// soft halo, cropped to the lit area. The anchor is the building sprite's anchor (the
/// footprint's front corner) relative to the crop, so it may lie outside `0..1`; drawn at the
/// building's anchor the panes cover the building's windows. `None` without visible windows.
pub fn light(id: &str) -> Option<(Canvas, (f32, f32))> {
    let ic = draw_building(id)?;
    let panes = ic.panes().to_vec();
    if panes.is_empty() {
        return None;
    }
    let (c, anchor) = ic.finish();
    let mask = light_mask(&c, &panes);
    let (x, y, w, h) = mask.opaque_bounds()?;
    let anchor_px = (anchor.0 * c.width() as f32, c.height() as f32);
    let crop = mask.crop(x, y, w as u32, h as u32);
    Some((crop, ((anchor_px.0 - x as f32) / w as f32, (anchor_px.1 - y as f32) / h as f32)))
}

/// Assembles the `lights` atlas: one night light per building with windows, named like the
/// building.
pub fn lights_atlas() -> AtlasBuilder {
    let mut atlas = AtlasBuilder::new("lights", Sampling::Pixel);
    for info in catalog::BUILDINGS.iter().filter(|b| !b.road_like) {
        if let Some((c, anchor)) = light(info.id) {
            atlas.add(info.id, c, anchor);
        }
    }
    atlas
}

/// IDs of the buildings with a night light.
#[cfg(test)]
pub fn lit_buildings() -> Vec<&'static str> {
    catalog::BUILDINGS.iter().filter(|b| !b.road_like && light(b.id).is_some()).map(|b| b.id).collect()
}

/// Smoke points of every finished building sprite that has any, keyed `buildings/<id>`.
pub fn smoke_points() -> BTreeMap<String, Vec<(f32, f32)>> {
    catalog::BUILDINGS
        .iter()
        .filter_map(|info| {
            let (_, _, smoke) = render_with_smoke(info.id)?;
            (!smoke.is_empty()).then(|| (format!("buildings/{}", info.id), smoke))
        })
        .collect()
}

/// Construction site of a `B×T` footprint at stage 0 (staked out), 1 (foundation and first
/// posts) or 2 (full frame with scaffolding).
pub fn construction(b: u32, t: u32, stage: u32) -> (Canvas, (f32, f32)) {
    let mut ic = canvas(b, t);
    let (sx, sy) = ic.size();
    ic.plate(0.04, Mat::Earth(rgb(EARTH)));
    let m = 0.3;
    let (a0, b0, a1, b1) = (m, m, sx - m, sy - m);
    let h = 14.0 + 5.0 * sx.min(sy);
    // Excavation / foundation trench.
    ic.ground_rect(a0 - 0.05, b0 - 0.05, a1 + 0.05, b1 + 0.05, 0.0, Mat::Earth(rgb(EARTH_DARK)));
    let corners = [(a0, b0), (a1, b0), (a0, b1), (a1, b1)];
    match stage {
        0 => {
            for &(a, b) in &corners {
                props::post(&mut ic, a, b, 0.0, 7.0, 0.035);
            }
            let line = rgba(0xf0e6c8, 220);
            ic.line3(v3(a0, b0, 5.0), v3(a1, b0, 5.0), line);
            ic.line3(v3(a0, b0, 5.0), v3(a0, b1, 5.0), line);
            ic.line3(v3(a1, b0, 5.0), v3(a1, b1, 5.0), line);
            ic.line3(v3(a0, b1, 5.0), v3(a1, b1, 5.0), line);
            props::log_pile(&mut ic, a1 - 0.45, b1 + 0.02, a1 - 0.05, b1 + 0.22, 5.0, Axis::A);
            props::stone_blocks(&mut ic, a0 + 0.05, b1 + 0.02);
        }
        _ => {
            let found = Mat::Stone(rgb(STONE));
            ic.block(v3(a0, b0, 0.0), v3(a1, b1, 4.0), found, Mat::Rough(rgb(STONE_WARM)));
            let frame_h = if stage == 1 { h * 0.55 } else { h };
            // Posts along all edges, back to front.
            let mut posts: Vec<(f32, f32, f32)> = Vec::new();
            let na = ((a1 - a0) / 0.45).round().max(1.0) as i32;
            let nb = ((b1 - b0) / 0.45).round().max(1.0) as i32;
            for i in 0..=na {
                let a = a0 + (a1 - a0) * i as f32 / na as f32;
                for &b in &[b0, b1] {
                    posts.push((a, b, frame_h));
                }
            }
            for j in 1..nb {
                let b = b0 + (b1 - b0) * j as f32 / nb as f32;
                for &a in &[a0, a1] {
                    posts.push((a, b, frame_h));
                }
            }
            if stage == 1 {
                // Only the back half is standing yet.
                posts.retain(|&(a, b, _)| a + b < (a0 + a1 + b0 + b1) * 0.5 + 0.3);
            }
            posts.sort_by(|p, q| (p.0 + p.1).total_cmp(&(q.0 + q.1)));
            let beam = rgb(TIMBER);
            if stage == 2 {
                // Back walls partly planked, top plates, rafters.
                ic.on_wall(Side::Left, b0 + 0.04, a0, a1, 4.0, frame_h * 0.6, boards(PLANK));
                for &(a, b, ph) in &posts {
                    props::post(&mut ic, a, b, 4.0, ph + 4.0, 0.05);
                }
                let zt = frame_h + 4.0;
                ic.line3(v3(a0, b0, zt), v3(a1, b0, zt), beam);
                ic.line3(v3(a0, b1, zt), v3(a1, b1, zt), beam);
                ic.line3(v3(a0, b0, zt), v3(a0, b1, zt), beam);
                ic.line3(v3(a1, b0, zt), v3(a1, b1, zt), beam);
                let rise = 10.0 + 5.0 * sx.min(sy);
                let bm = (b0 + b1) * 0.5;
                let nr = ((a1 - a0) / 0.3).round().max(2.0) as i32;
                for i in 0..=nr {
                    let a = a0 + (a1 - a0) * i as f32 / nr as f32;
                    ic.line3(v3(a, b0, zt), v3(a, bm, zt + rise), beam);
                    ic.line3(v3(a, b1, zt), v3(a, bm, zt + rise), shade(beam, 1.15));
                }
                ic.line3(v3(a0, bm, zt + rise), v3(a1, bm, zt + rise), beam);
                // Scaffolding in front of the left wall with a plank walkway and a ladder.
                let sb = b1 + 0.14;
                let pole = rgb(0x9a7a50);
                for i in 0..=2 {
                    let a = a0 + (a1 - a0) * i as f32 / 2.0;
                    ic.line3(v3(a, sb, 0.0), v3(a, sb, zt + 2.0), pole);
                }
                ic.block(v3(a0, sb - 0.06, zt * 0.5), v3(a1, sb + 0.04, zt * 0.5 + 2.0), boards(PLANK_LIGHT), boards(PLANK_LIGHT));
                let la = a1 - 0.2;
                ic.line3(v3(la, sb + 0.1, 0.0), v3(la, sb, zt * 0.5 + 2.0), pole);
                ic.line3(v3(la + 0.1, sb + 0.1, 0.0), v3(la + 0.1, sb, zt * 0.5 + 2.0), pole);
                for k in 1..5 {
                    let z = k as f32 * zt * 0.1;
                    let t = z / (zt * 0.5 + 2.0);
                    ic.line3(v3(la, sb + 0.1 - 0.1 * t, z), v3(la + 0.1, sb + 0.1 - 0.1 * t, z), pole);
                }
            } else {
                for &(a, b, ph) in &posts {
                    props::post(&mut ic, a, b, 4.0, ph + 4.0, 0.05);
                }
                // Diagonal scaffold poles and material piles.
                ic.line3(v3(a0, b1 + 0.12, 0.0), v3(a0 + 0.3, b1 + 0.02, frame_h), rgb(0x9a7a50));
                props::plank_stack(&mut ic, a1 - 0.5, b1 + 0.04, a1 - 0.1, b1 + 0.22, 5.0);
                props::stone_blocks(&mut ic, a0 + 0.02, b1 + 0.04);
            }
        }
    }
    ic.finish_trimmed()
}

/// Rubble of a destroyed building of footprint `B×T`.
pub fn rubble(b: u32, t: u32) -> (Canvas, (f32, f32)) {
    let mut ic = canvas(b, t);
    let (sx, sy) = ic.size();
    let seed = b * 31 + t * 7;
    ic.plate(0.04, Mat::Earth(rgb(EARTH_DARK)));
    // Ash patches.
    for i in 0..(sx * sy) as i32 {
        let a = 0.3 + hash01(i, 1, seed) * (sx - 0.6);
        let bb = 0.3 + hash01(i, 2, seed) * (sy - 0.6);
        ic.ground_rect(a - 0.25, bb - 0.2, a + 0.25, bb + 0.2, 0.0, Mat::Earth(rgb(0x4a4440)));
    }
    // Stone chunks and charred beams sorted back to front.
    let n = (6.0 * sx * sy).min(60.0) as i32;
    let mut items: Vec<(f32, f32, i32)> = (0..n)
        .map(|i| (0.2 + hash01(i, 3, seed) * (sx - 0.5), 0.2 + hash01(i, 4, seed) * (sy - 0.5), i))
        .collect();
    items.sort_by(|p, q| (p.0 + p.1).total_cmp(&(q.0 + q.1)));
    for &(a, bb, i) in &items {
        let s = 0.06 + hash01(i, 5, seed) * 0.12;
        let hgt = 3.0 + hash01(i, 6, seed) * 6.0;
        if i % 4 == 0 {
            let beam = Mat::Flat(rgb(0x2e2420));
            if i % 8 == 0 {
                ic.block(v3(a, bb, 0.0), v3(a + 0.5, bb + 0.05, 3.0), beam, beam);
            } else {
                ic.block(v3(a, bb, 0.0), v3(a + 0.05, bb + 0.45, 3.0), beam, beam);
            }
        } else {
            let tone = if i % 3 == 0 { STONE_DARK } else { STONE };
            ic.block(v3(a, bb, 0.0), v3(a + s, bb + s * 0.8, hgt), stone(tone), Mat::Rough(rgb(tone)));
        }
    }
    // A leaning, broken wall stump at the back.
    ic.block(v3(0.3, 0.3, 0.0), v3(0.5, sy * 0.5, 10.0 + 3.0 * sy), stone(STONE), Mat::Rough(rgb(STONE_DARK)));
    ic.finish_trimmed()
}

/// Everything in the buildings atlas: `(name, sprite, anchor)`.
pub fn all_sprites() -> Vec<(String, Canvas, (f32, f32))> {
    let mut out = Vec::new();
    for info in catalog::BUILDINGS.iter().filter(|b| !b.road_like) {
        if let Some((c, anchor)) = render(info.id) {
            out.push((info.id.to_owned(), c, anchor));
        }
    }
    for (b, t) in catalog::footprints() {
        for stage in 0..3 {
            let (c, a) = construction(b, t, stage);
            out.push((format!("construction_{b}x{t}_{stage}"), c, a));
        }
        let (c, a) = rubble(b, t);
        out.push((format!("rubble_{b}x{t}"), c, a));
    }
    for id in WORK_LOOPS {
        for frame in 0..WORK_FRAMES {
            if let Some((c, a)) = render_work_frame(id, frame) {
                out.push((format!("{id}_work_{frame}"), c, a));
            }
        }
    }
    for id in WINTER_VARIANTS {
        if let Some((c, a)) = render_winter(id) {
            out.push((format!("{id}_winter"), c, a));
        }
    }
    // Aliases so a generic "buildings/<id>" lookup also works for wall and gate.
    out.push(("wall".to_owned(), crate::roads::wall(5), (0.5, 1.0)));
    out.push(("gate".to_owned(), crate::roads::gate(Axis::A), (0.5, 1.0)));
    out
}

/// Assembles the `buildings` atlas.
pub fn atlas() -> AtlasBuilder {
    let mut atlas = AtlasBuilder::new("buildings", Sampling::Pixel);
    for (name, c, anchor) in all_sprites() {
        atlas.add(name, c, anchor);
    }
    for id in WORK_LOOPS {
        let frames = (0..WORK_FRAMES).map(|f| format!("{id}_work_{f}")).collect();
        // Mill sails turn slowly, forge fires flicker.
        let fps = if id.ends_with("mill") { 3.0 } else { 6.0 };
        atlas.animation(&format!("{id}_work"), frames, fps, true);
    }
    atlas
}

/// Frames per second of the work loop of building `id`, if it has one.
#[cfg(test)]
fn work_loop_fps(id: &str) -> Option<f32> {
    WORK_LOOPS.contains(&id).then(|| if id.ends_with("mill") { 3.0 } else { 6.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_building_has_a_footprint_correct_sprite() {
        for info in catalog::BUILDINGS.iter().filter(|b| !b.road_like) {
            let Some((c, anchor)) = render(info.id) else {
                panic!("no sprite for {}", info.id);
            };
            assert_eq!(c.width() as u32, (info.b + info.t) * 32, "{} width", info.id);
            let expect = info.t as f32 / (info.b + info.t) as f32;
            assert!((anchor.0 - expect).abs() < 1e-6 && anchor.1 == 1.0, "{} anchor", info.id);
            // Front corner pixel row is part of the plot, the canvas corners are not.
            let fx = (info.t * 32) as i32;
            let bottom = c.height() - 1;
            let near_front = (bottom - 5..=bottom).any(|y| c.get(fx, y)[3] > 0 || c.get(fx - 1, y)[3] > 0);
            assert!(near_front, "{} front corner", info.id);
            assert_eq!(c.get(0, bottom)[3], 0, "{} bottom-left corner must stay empty", info.id);
            assert!(c.height() as u32 >= (info.b + info.t) * 16, "{} is cut off", info.id);
        }
    }

    #[test]
    fn work_loops_match_their_building_and_move() {
        for id in WORK_LOOPS {
            let Some((base, anchor)) = render(id) else { panic!("{id} is not rendered") };
            let frames: Vec<(Canvas, (f32, f32))> =
                (0..WORK_FRAMES).filter_map(|f| render_work_frame(id, f)).collect();
            assert_eq!(frames.len() as u32, WORK_FRAMES, "{id}");
            // Frame 0 is the finished sprite; all frames keep its width and anchor.
            assert_eq!(frames[0].0.img, base.img, "{id} frame 0");
            for (c, a) in &frames {
                assert_eq!(c.width(), base.width(), "{id} width");
                assert_eq!(*a, anchor, "{id} anchor");
            }
            assert!(frames.windows(2).all(|w| w[0].0.img != w[1].0.img), "{id} frames repeat");
            assert!(work_loop_fps(id).is_some_and(|fps| fps > 0.0));
        }
        assert!(render_work_frame("bakery", 1).is_none());
        let names: Vec<String> = atlas().animation_names().into_iter().map(str::to_owned).collect();
        assert!(names.contains(&"mill_work".to_owned()) && names.contains(&"smithy_work".to_owned()));
    }

    #[test]
    fn winter_variants_keep_the_footprint() {
        for id in WINTER_VARIANTS {
            let (Some((base, anchor)), Some((winter, winter_anchor))) = (render(id), render_winter(id)) else {
                panic!("{id} has no winter variant")
            };
            assert_eq!(winter.width(), base.width(), "{id}");
            assert_eq!(winter_anchor, anchor, "{id}");
            assert!(winter.coverage() < base.coverage(), "{id}: bare trees cover less");
        }
        assert!(render_winter("bakery").is_none());
    }

    #[test]
    fn unknown_and_segment_ids_are_not_rendered_here() {
        assert!(render("no_such_building").is_none());
        assert!(render("wall").is_none());
    }

    #[test]
    fn construction_stages_grow() {
        for (b, t) in catalog::footprints() {
            let cov: Vec<f32> = (0..3).map(|s| construction(b, t, s).0.coverage()).collect();
            let heights: Vec<i32> = (0..3).map(|s| construction(b, t, s).0.height()).collect();
            assert!(heights[0] <= heights[1] && heights[1] <= heights[2], "{b}x{t}: {heights:?}");
            assert!(cov.iter().all(|c| *c > 0.2), "{b}x{t}: {cov:?}");
            let (r, ra) = rubble(b, t);
            assert_eq!(r.width() as u32, (b + t) * 32);
            assert!((ra.0 - t as f32 / (b + t) as f32).abs() < 1e-6);
        }
    }

    #[test]
    fn smoke_points_sit_on_the_sprites() {
        let points = smoke_points();
        for id in ["house_wood", "farmhouse", "citizen_house", "smithy", "bakery", "brickworks", "charcoal_kiln", "smelter", "pottery", "tavern"] {
            assert!(points.contains_key(&format!("buildings/{id}")), "{id} has no smoke point");
        }
        assert!(!points.contains_key("buildings/well"));
        for (name, list) in &points {
            let id = name.trim_start_matches("buildings/");
            let Some((c, anchor)) = render(id) else { panic!("{name} is not rendered") };
            for &(x, y) in list {
                // Inside the sprite and above the ground: the point lies on drawn pixels.
                let px = (anchor.0 * c.width() as f32 + x).round() as i32;
                let py = (c.height() as f32 + y).round() as i32;
                assert!(y < 0.0 && px >= 0 && px < c.width() && py >= 0, "{name}: ({x}, {y}) outside");
                let near = (-2..=2).any(|dy| (-2..=2).any(|dx| c.get(px + dx, py + dy)[3] > 0));
                assert!(near, "{name}: ({x}, {y}) is not on the sprite");
            }
        }
    }

    #[test]
    fn night_lights_cover_the_visible_window_panes() {
        let lit = lit_buildings();
        for id in ["house_wood", "farmhouse", "citizen_house", "townhouse", "residence", "manor", "bakery", "tavern", "church"] {
            assert!(lit.contains(&id), "{id} has no night light");
        }
        for id in ["well", "orchard"] {
            assert!(!lit.contains(&id), "{id} has no windows but a light");
        }
        for id in ["house_wood", "townhouse", "bakery"] {
            let (Some((sprite, anchor)), Some((light, la))) = (render(id), light(id)) else { panic!("{id} not rendered") };
            // Map light pixels to sprite pixels through the shared anchor (the sprite is
            // trimmed at the top, the anchor is its bottom edge).
            let ox = anchor.0 * sprite.width() as f32 - la.0 * light.width() as f32;
            let oy = sprite.height() as f32 - la.1 * light.height() as f32;
            let mut panes = 0;
            for y in 0..light.height() {
                for x in 0..light.width() {
                    let px = light.get(x, y);
                    if px[3] == 255 {
                        panes += 1;
                        let under = sprite.get(x + ox.round() as i32, y + oy.round() as i32);
                        assert!(is_glazing(under, WINDOW), "{id}: lit pixel ({x}, {y}) is not over a pane: {under:?}");
                    }
                }
            }
            assert!(panes >= 8, "{id}: only {panes} lit pixels");
        }
    }

    #[test]
    fn sprites_are_deterministic() {
        let a = render("mill").map(|r| r.0.img);
        let b = render("mill").map(|r| r.0.img);
        assert!(a.is_some() && a == b);
    }
}
