//! Public buildings: town hall with clock tower, market stalls, church with spire, school,
//! hospital, well, tavern and storage buildings.

use crate::draw::{rgb, rgba};
use crate::iso::{Axis, IsoCanvas, Roof, Side, v3};
use crate::mat::Mat;
use crate::nature::Tree;
use crate::palette::*;
use crate::props;

use super::{boards, flat, hip_house, house, roof_chimney, shed, shingles, slate, stone, thatch, tiles, timber};

/// Draws public building `id`; returns `false` if `id` is not a public building.
pub fn draw(id: &str, ic: &mut IsoCanvas) -> bool {
    match id {
        "town_hall" => town_hall(ic),
        "market" => market(ic),
        "grand_market" => grand_market(ic),
        "church" => church(ic),
        "cathedral" => cathedral(ic),
        "school" => school(ic),
        "academy" => academy(ic),
        "hospital" => hospital(ic),
        "infirmary" => infirmary(ic),
        "inn" => inn(ic),
        "well" => well(ic),
        "tavern" => tavern(ic),
        "warehouse" => warehouse(ic),
        "storehouse" => storehouse(ic),
        "trading_post" => trading_post(ic),
        _ => return false,
    }
    true
}

fn town_hall(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.45, 0.5, 3.15, 2.2];
    ic.shadow(r[0], r[1], r[2], r[3], 60.0);
    ic.block(v3(r[0], r[1], 0.0), v3(r[2], r[3], 16.0), stone(STONE_WARM), stone(STONE_WARM));
    ic.block(v3(r[0], r[1], 16.0), v3(r[2], r[3], 32.0), timber(WHITEWASH), timber(WHITEWASH));
    let roof = Roof::new(r, 32.0, 30.0, Axis::A, tiles(TILE_RED), timber(WHITEWASH));
    ic.gable(roof);
    roof_chimney(ic, &roof, 0.9, 0.7, 6.0, stone(STONE_DARK));
    // Arcade and windows on the lit side.
    for a in [0.8, 1.25, 1.7] {
        ic.on_wall(Side::Left, 2.2, a - 0.14, a + 0.14, 0.0, 11.0, flat(0x2a241c));
        ic.window(Side::Left, 2.2, a, 21.0, 0.16, 6.0);
    }
    for b in [0.9, 1.45, 1.95] {
        ic.window(Side::Right, 3.15, b, 6.0, 0.16, 6.0);
        ic.window(Side::Right, 3.15, b, 21.0, 0.16, 6.0);
    }
    // Clock tower at the front-right corner.
    let (t0, t1) = ((2.3, 1.65), (3.0, 2.35));
    ic.shadow(t0.0, t0.1, t1.0, t1.1, 100.0);
    ic.block(v3(t0.0, t0.1, 0.0), v3(t1.0, t1.1, 66.0), stone(STONE_WARM), Mat::Rough(rgb(STONE)));
    ic.hip(Roof::new([t0.0, t0.1, t1.0, t1.1], 66.0, 30.0, Axis::A, slate(SLATE), stone(STONE)));
    ic.door(Side::Left, t1.1, 2.65, 0.26, 15.0);
    // Clock face.
    let (cx, cy) = ic.project(v3(2.65, t1.1, 52.0));
    let c = ic.canvas();
    c.fill_ellipse(cx, cy, 5.0, 5.5, |_, _, _| rgb(0xf0e8d0));
    c.fill_ellipse(cx, cy, 5.0, 5.5, |_, _, d| if d > 0.7 { rgb(GOLD) } else { rgba(0, 0) });
    c.linef(cx, cy, cx, cy - 3.0, rgb(INK));
    c.linef(cx, cy, cx + 2.0, cy + 1.0, rgb(INK));
    ic.window(Side::Right, t1.0, 2.0, 40.0, 0.14, 7.0);
    props::flag(ic, 2.65, 2.0, 96.0, 14.0, rgb(BANNER_RED));
    props::flag(ic, 0.6, 2.35, 0.0, 22.0, rgb(BANNER_BLUE));
    props::flag(ic, 3.35, 0.6, 0.0, 22.0, rgb(BANNER_BLUE));
}

fn stall(ic: &mut IsoCanvas, a: f32, b: f32, c1: u32, c2: u32, goods: &[u32]) {
    let (a1, b1) = (a + 0.6, b + 0.45);
    props::post(ic, a + 0.03, b + 0.03, 0.0, 14.0, 0.03);
    props::post(ic, a1 - 0.03, b + 0.03, 0.0, 14.0, 0.03);
    ic.block(v3(a, b + 0.2, 0.0), v3(a1, b1, 6.0), boards(PLANK), boards(PLANK_LIGHT));
    for (i, g) in goods.iter().enumerate() {
        let t = (i as f32 + 0.5) / goods.len() as f32;
        let (x, y) = ic.project(v3(a + 0.6 * t, b + 0.32, 6.0));
        ic.canvas().lit_ellipse(x, y - 1.0, 2.6, 1.8, rgb(*g));
    }
    props::post(ic, a + 0.03, b1 - 0.03, 0.0, 10.0, 0.03);
    props::post(ic, a1 - 0.03, b1 - 0.03, 0.0, 10.0, 0.03);
    props::awning(ic, a - 0.04, b - 0.04, a1 + 0.04, b1 + 0.08, 15.0, 10.0, Side::Left, rgb(c1), rgb(c2));
}

fn market(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    ic.ground_rect(1.5, 1.5, 2.5, 2.5, 0.0, Mat::Cobbles(rgb(0xa49e92)));
    stall(ic, 0.4, 0.35, 0xb83a3a, 0xeee6d2, &[0xd8b030, 0xc8281e, 0x5a9a3a]);
    stall(ic, 1.6, 0.35, 0x3a5aa8, 0xeee6d2, &[0xe8c050, 0xe8c050, 0xd8a040]);
    stall(ic, 2.8, 0.4, 0x3a8a4a, 0xe8d070, &[0xb8c0c8, 0x9aa8b0, 0xb8c0c8]);
    stall(ic, 0.4, 1.6, 0xd8a030, 0x8a3a2a, &[0xc8843a, 0xc8843a, 0xe0b060]);
    // Market cross in the centre.
    ic.frustum(2.0, 2.0, 0.28, 0.22, 0.0, 4.0, stone(STONE));
    ic.cylinder(2.0, 2.0, 0.06, 4.0, 28.0, stone(STONE_WARM));
    ic.block(v3(1.88, 1.97, 22.0), v3(2.12, 2.03, 25.0), stone(STONE_WARM), Mat::Rough(rgb(STONE_WARM)));
    stall(ic, 2.8, 1.6, 0x6a3a8a, 0xeee6d2, &[0x5a2a5a, 0xd8d0b0, 0x8a3a6a]);
    stall(ic, 0.45, 2.85, 0xb83a3a, 0xe8d070, &[0xeee6d4, 0xe6dcc8, 0xd8c890]);
    // Crates, sacks and barrels.
    props::crate_box(ic, 1.7, 2.9, 0.22, 7.0, 0.0);
    props::barrel(ic, 2.2, 3.0, 0.0);
    props::sack(ic, 2.5, 2.95, 0.0, rgb(0xd8c890));
    stall(ic, 2.8, 2.85, 0x3a5aa8, 0xe8d070, &[0xc8281e, 0xd87a2a, 0x7aa84a]);
    props::sack(ic, 1.3, 1.55, 0.0, rgb(0xd8c890));
    props::crate_box(ic, 2.45, 1.35, 0.18, 6.0, 0.0);
    props::barrel(ic, 3.4, 2.4, 0.0);
}

fn church(ic: &mut IsoCanvas) {
    // Footprint 3×4: the nave runs along `a` (4 tiles), width along `b` (3 tiles).
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(3.0, 1.0, 3.96, 1.95, 0.0, Mat::Cobbles(rgb(COBBLE)));
    let nave = [0.35, 0.75, 2.95, 2.05];
    house(ic, nave, 24.0, stone(STONE_WARM), slate(SLATE), 26.0, Axis::A);
    // Tall arched windows along the nave.
    for a in [0.75, 1.3, 1.85, 2.4] {
        ic.pane(Side::Left, 2.05, a - 0.07, a + 0.07, 7.0, 19.0, 0x2a3448);
        ic.pane(Side::Left, 2.05, a - 0.04, a + 0.04, 19.0, 21.0, 0x2a3448);
        ic.line3(v3(a, 2.06, 7.0), v3(a, 2.06, 20.0), rgb(0x8a7a5a));
    }
    // Side chapel.
    house(ic, [1.4, 2.05, 2.1, 2.6], 16.0, stone(STONE_WARM), slate(SLATE), 12.0, Axis::B);
    ic.pane(Side::Left, 2.6, 1.7, 1.8, 5.0, 12.0, 0x2a3448);
    // West tower with spire at the front end.
    let (t0, t1) = ((2.95, 0.95), (3.7, 1.85));
    ic.shadow(t0.0, t0.1, t1.0, t1.1, 110.0);
    ic.block(v3(t0.0, t0.1, 0.0), v3(t1.0, t1.1, 58.0), stone(STONE_WARM), Mat::Rough(rgb(STONE)));
    ic.hip(Roof::new([t0.0, t0.1, t1.0, t1.1], 58.0, 48.0, Axis::A, slate(SLATE), stone(STONE)));
    ic.on_wall(Side::Right, t1.0, 1.25, 1.55, 0.0, 16.0, flat(0x2a1e14));
    ic.on_wall(Side::Right, t1.0, 1.29, 1.51, 0.0, 15.0, Mat::Boards(rgb(DOOR)));
    for (side, plane, c) in [(Side::Right, t1.0, 1.4), (Side::Left, t1.1, 3.32)] {
        ic.on_wall(side, plane, c - 0.08, c + 0.08, 40.0, 50.0, flat(0x1e1a16));
    }
    // Cross on the spire.
    let (x, y) = ic.project(v3(3.325, 1.4, 106.0));
    let c = ic.canvas();
    c.fill_rect(x as i32, y as i32 - 8, 1, 9, rgb(GOLD));
    c.fill_rect(x as i32 - 2, y as i32 - 6, 5, 1, rgb(GOLD));
    // Graveyard crosses and a yew.
    for &(a, b) in &[(0.6, 2.4), (0.95, 2.55), (0.65, 2.75), (1.0, 2.85)] {
        let (x, y) = ic.project(v3(a, b, 0.0));
        let c = ic.canvas();
        c.fill_rect(x as i32, y as i32 - 6, 1, 6, rgb(STONE));
        c.fill_rect(x as i32 - 1, y as i32 - 5, 3, 1, rgb(STONE));
    }
    props::tree(ic, 2.6, 2.6, Tree::Pine, 0.42, 17);
}

fn school(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.3, 1.6, 2.7, 2.7, 0.0, Mat::Earth(rgb(0xa08a68)));
    let r = [0.3, 0.35, 2.4, 1.4];
    house(ic, r, 19.0, timber(PLASTER), tiles(TILE_RED), 22.0, Axis::A);
    for a in [0.6, 1.0, 1.75, 2.1] {
        ic.window(Side::Left, 1.4, a, 7.0, 0.16, 6.0);
    }
    ic.door(Side::Left, 1.4, 1.38, 0.24, 13.0);
    ic.window(Side::Right, 2.4, 0.85, 7.0, 0.16, 6.0);
    // Bell turret on the ridge.
    let (ba, bb) = (1.9, 0.87);
    ic.block(v3(ba - 0.12, bb - 0.12, 38.0), v3(ba + 0.12, bb + 0.12, 50.0), boards(PLANK), boards(PLANK));
    ic.on_wall(Side::Left, bb + 0.12, ba - 0.07, ba + 0.07, 42.0, 48.0, flat(0x1a1410));
    let (x, y) = ic.project(v3(ba, bb + 0.13, 44.0));
    ic.canvas().fill_rect(x as i32 - 1, y as i32, 3, 3, rgb(GOLD));
    ic.hip(Roof::new([ba - 0.16, bb - 0.16, ba + 0.16, bb + 0.16], 50.0, 12.0, Axis::A, slate(SLATE), boards(PLANK)));
    // Slate board and pupils.
    let (sx, sy) = ic.project(v3(0.7, 1.9, 0.0));
    let c = ic.canvas();
    c.fill_rect(sx as i32 - 5, sy as i32 - 12, 11, 8, rgb(TIMBER));
    c.fill_rect(sx as i32 - 4, sy as i32 - 11, 9, 6, rgb(0x2e3a36));
    c.linef(sx - 3.0, sy - 9.0, sx + 2.0, sy - 9.0, rgb(0xd8d8d0));
    c.linef(sx - 3.0, sy - 7.0, sx + 1.0, sy - 7.0, rgb(0xd8d8d0));
    c.fill_rect(sx as i32 - 4, sy as i32 - 4, 1, 4, rgb(TIMBER));
    c.fill_rect(sx as i32 + 4, sy as i32 - 4, 1, 4, rgb(TIMBER));
    props::tree(ic, 2.4, 2.3, Tree::Oak, 0.45, 23);
}

fn hospital(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    let r = [0.3, 0.3, 2.6, 1.3];
    let r_roof = house(ic, r, 18.0, stone(0xd8d0c0), slate(SLATE), 20.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.6, 0.45, 6.0, stone(STONE_DARK));
    for a in [0.55, 0.9, 1.25, 1.95, 2.3] {
        ic.window(Side::Left, 1.3, a, 7.0, 0.14, 6.0);
    }
    ic.door(Side::Left, 1.3, 1.6, 0.26, 13.0);
    let wing = [1.85, 1.3, 2.6, 2.1];
    house(ic, wing, 16.0, stone(0xd8d0c0), slate(SLATE), 16.0, Axis::B);
    ic.window(Side::Right, 2.6, 1.7, 6.0, 0.14, 6.0);
    // Green cross banner (herbalist's sign).
    let (x, y) = ic.project(v3(1.6, 1.31, 16.0));
    let c = ic.canvas();
    c.fill_rect(x as i32 - 4, y as i32 - 4, 9, 9, rgb(0xeeeae0));
    c.fill_rect(x as i32 - 1, y as i32 - 3, 3, 7, rgb(0x2f7a3a));
    c.fill_rect(x as i32 - 3, y as i32 - 1, 7, 3, rgb(0x2f7a3a));
    // Herb garden.
    props::bed(ic, 0.3, 1.6, 1.0, 2.7, rgb(0x5a8a3a), Axis::B);
    props::bed(ic, 1.15, 1.6, 1.65, 2.7, rgb(0x8a6ab0), Axis::B);
    props::fence(ic, 0.3, 2.8, 1.7, 2.8, 5.0);
}

fn well(ic: &mut IsoCanvas) {
    ic.plate(0.08, Mat::Cobbles(rgb(COBBLE)));
    let (ca, cb) = (0.5, 0.5);
    ic.shadow(0.25, 0.25, 0.75, 0.75, 16.0);
    props::post(ic, ca, cb - 0.24, 0.0, 22.0, 0.05);
    ic.frustum(ca, cb, 0.24, 0.24, 0.0, 7.0, stone(STONE));
    // The shaft opening stays dark in winter.
    ic.snow_free(|ic| ic.disc(ca, cb, 0.18, 7.1, flat(0x1c2a36)));
    // Bucket on the rope.
    ic.line3(v3(ca, cb, 20.0), v3(ca, cb, 11.0), rgb(0xc8b890));
    ic.frustum(ca, cb, 0.05, 0.06, 7.0, 11.0, boards(PLANK));
    props::post(ic, ca, cb + 0.24, 0.0, 22.0, 0.05);
    ic.line3(v3(ca, cb - 0.24, 20.0), v3(ca, cb + 0.24, 20.0), rgb(TIMBER));
    ic.gable(Roof::new([0.28, 0.2, 0.72, 0.8], 22.0, 9.0, Axis::B, shingles(), boards(PLANK)));
}

fn tavern(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.3, 0.3, 2.3, 1.6];
    ic.shadow(r[0], r[1], r[2], r[3], 55.0);
    ic.block(v3(r[0], r[1], 0.0), v3(r[2], r[3], 15.0), stone(STONE_WARM), stone(STONE_WARM));
    let up = [0.26, 0.26, 2.34, 1.66];
    ic.block(v3(up[0], up[1], 15.0), v3(up[2], up[3], 30.0), timber(DAUB), timber(DAUB));
    let roof = Roof::new(up, 30.0, 24.0, Axis::B, thatch(THATCH), timber(DAUB));
    ic.gable(roof);
    roof_chimney(ic, &roof, 1.55, 0.4, 6.0, stone(STONE_DARK));
    ic.door(Side::Left, 1.6, 1.1, 0.26, 12.0);
    for a in [0.6, 1.65, 2.0] {
        ic.window(Side::Left, 1.6, a, 5.0, 0.16, 5.0);
    }
    for a in [0.55, 1.0, 1.45, 1.9] {
        ic.window(Side::Left, 1.66, a, 20.0, 0.14, 5.0);
    }
    ic.window(Side::Right, 2.34, 0.9, 20.0, 0.14, 5.0);
    props::wall_sign(ic, Side::Left, 1.66, 1.35, 13.0, rgb(0xd8a030));
    // Beer garden: benches, table and barrels.
    ic.block(v3(0.6, 2.0, 0.0), v3(1.4, 2.25, 6.0), boards(TIMBER), boards(PLANK));
    ic.block(v3(0.6, 1.85, 0.0), v3(1.4, 1.93, 3.5), boards(TIMBER), boards(PLANK));
    ic.block(v3(0.6, 2.33, 0.0), v3(1.4, 2.41, 3.5), boards(TIMBER), boards(PLANK));
    props::barrel(ic, 2.5, 0.5, 0.0);
    props::barrel(ic, 2.5, 0.75, 0.0);
    props::barrel(ic, 2.5, 0.62, 9.0);
    props::barrel(ic, 2.2, 2.5, 0.0);
}

fn warehouse(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(EARTH)));
    let r = [0.3, 0.3, 2.55, 1.85];
    house(ic, r, 24.0, boards(PLANK), shingles(), 24.0, Axis::A);
    ic.on_wall(Side::Left, 1.85, 1.0, 1.7, 0.0, 18.0, Mat::Boards(rgb(0x6a4a30)));
    ic.line3(v3(1.35, 1.86, 0.0), v3(1.35, 1.86, 18.0), rgb(TIMBER_DARK));
    ic.on_wall(Side::Right, 2.55, 0.8, 1.35, 0.0, 16.0, Mat::Boards(rgb(0x6a4a30)));
    ic.window(Side::Left, 1.85, 0.55, 14.0, 0.14, 5.0);
    // Hoist beam above the door.
    ic.line3(v3(1.35, 1.85, 24.0), v3(1.35, 2.15, 24.0), rgb(TIMBER));
    ic.line3(v3(1.35, 2.15, 24.0), v3(1.35, 2.15, 14.0), rgb(0xc8b890));
    props::crate_box(ic, 0.5, 2.2, 0.22, 7.0, 0.0);
    props::crate_box(ic, 0.76, 2.2, 0.22, 7.0, 0.0);
    props::crate_box(ic, 0.6, 2.2, 0.22, 7.0, 7.0);
    props::barrel(ic, 2.0, 2.35, 0.0);
    props::barrel(ic, 2.2, 2.3, 0.0);
    props::sack(ic, 1.55, 2.5, 0.0, rgb(0xd8c890));
    props::sack(ic, 1.75, 2.55, 0.0, rgb(0xd0c088));
    props::crate_box(ic, 2.5, 2.3, 0.2, 7.0, 0.0);
}

fn storehouse(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(EARTH)));
    for &(a, b) in &[(0.45, 0.45), (1.5, 0.45), (0.45, 1.3), (1.5, 1.3)] {
        ic.cylinder(a, b, 0.06, 0.0, 4.0, stone(STONE));
        ic.disc(a, b, 0.1, 4.0, stone(STONE_WARM));
    }
    ic.shadow(0.35, 0.35, 1.6, 1.4, 34.0);
    ic.block(v3(0.35, 0.35, 4.0), v3(1.6, 1.4, 18.0), boards(PLANK), boards(PLANK));
    ic.gable(Roof::new([0.35, 0.35, 1.6, 1.4], 18.0, 16.0, Axis::A, thatch(THATCH), boards(PLANK)));
    ic.door(Side::Left, 1.4, 0.95, 0.22, 0.0);
    ic.on_wall(Side::Left, 1.4, 0.85, 1.07, 4.0, 15.0, Mat::Boards(rgb(DOOR)));
    ic.face(&[v3(0.85, 1.4, 4.0), v3(1.07, 1.4, 4.0), v3(1.07, 1.7, 0.0), v3(0.85, 1.7, 0.0)], v3(0.96, 1.5, -4.0), Mat::BoardsAcross(rgb(PLANK_LIGHT)));
    props::sack(ic, 1.55, 1.7, 0.0, rgb(0xd8c890));
    props::sack(ic, 1.75, 1.55, 0.0, rgb(0xe0d098));
    props::sack(ic, 0.45, 1.7, 0.0, rgb(0xd0c088));
}

fn trading_post(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.3, 0.3, 2.0, 1.5];
    hip_house(ic, r, 20.0, timber(PLASTER), tiles(TILE_DARK), 20.0);
    ic.door(Side::Left, 1.5, 0.75, 0.26, 13.0);
    ic.window(Side::Left, 1.5, 1.35, 8.0, 0.16, 6.0);
    ic.window(Side::Right, 2.0, 0.9, 8.0, 0.16, 6.0);
    props::wall_sign(ic, Side::Left, 1.5, 1.75, 15.0, rgb(GOLD));
    // Goods under a large awning.
    shed(ic, [2.1, 0.35, 2.8, 1.5], 14.0, Mat::Stripes(rgb(0x8a3a2a), rgb(0xe8d8b0)), 6.0, Axis::B, |ic| {
        props::crate_box(ic, 2.25, 0.5, 0.2, 6.0, 0.0);
        props::crate_box(ic, 2.25, 0.75, 0.2, 6.0, 0.0);
        props::barrel(ic, 2.6, 1.1, 0.0);
    });
    // Caravan goods and flags of foreign houses.
    props::sack(ic, 1.0, 1.85, 0.0, rgb(0xb8582a));
    props::sack(ic, 1.2, 1.95, 0.0, rgb(0x7a4a8a));
    props::crate_box(ic, 1.5, 1.8, 0.25, 8.0, 0.0);
    props::flag(ic, 0.4, 2.6, 0.0, 26.0, rgb(0x2f7a6a));
    props::flag(ic, 2.7, 1.8, 0.0, 26.0, rgb(0xb88a20));
    props::sack(ic, 2.0, 2.4, 0.0, rgb(0xd0c088));
    props::crate_box(ic, 2.3, 2.5, 0.22, 8.0, 0.0);
}

// --- Level 2 of the public buildings (reached by upgrading) -----------------------------------

/// Level 2 of the market: an open market hall on stone pillars in the back, more stalls, a
/// fountain instead of the market cross.
fn grand_market(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    ic.ground_rect(1.6, 1.6, 2.6, 2.6, 0.0, Mat::Cobbles(rgb(0xa49e92)));
    // Market hall: stone arcade below, timbered hall above.
    let r = [0.3, 0.3, 3.7, 1.25];
    ic.shadow(r[0], r[1], r[2], r[3], 50.0);
    for i in 0..6 {
        let a = 0.4 + i as f32 * 0.64;
        ic.block(v3(a, 1.05, 0.0), v3(a + 0.14, 1.2, 14.0), stone(STONE_WARM), Mat::Rough(rgb(STONE_WARM)));
    }
    ic.block(v3(r[0], r[1], 0.0), v3(r[2], 0.45, 14.0), stone(STONE_WARM), stone(STONE_WARM));
    for i in 0..3 {
        let a = 0.8 + i as f32 * 1.1;
        props::crate_box(ic, a, 0.7, 0.2, 7.0, 0.0);
        props::sack(ic, a + 0.35, 0.75, 0.0, rgb(0xd8c890));
    }
    ic.block(v3(r[0], r[1], 14.0), v3(r[2], r[3], 28.0), timber(WHITEWASH), timber(WHITEWASH));
    let roof = Roof::new(r, 28.0, 22.0, Axis::A, tiles(TILE_RED), timber(WHITEWASH));
    ic.gable(roof);
    for a in [0.7, 1.3, 1.9, 2.5, 3.1] {
        ic.window(Side::Left, 1.25, a, 18.0, 0.14, 6.0);
    }
    // Fountain in the centre.
    ic.frustum(2.1, 2.1, 0.42, 0.42, 0.0, 4.0, stone(STONE));
    ic.disc(2.1, 2.1, 0.36, 4.0, Mat::Water(rgb(WATER)));
    ic.cylinder(2.1, 2.1, 0.06, 4.0, 18.0, stone(STONE_WARM));
    ic.dome(2.1, 2.1, 0.14, 16.0, 4.0, stone(STONE_WARM));
    // Stalls around the square.
    stall(ic, 0.4, 1.55, 0xd8a030, 0x8a3a2a, &[0xc8843a, 0xc8843a, 0xe0b060]);
    stall(ic, 2.95, 1.55, 0x6a3a8a, 0xeee6d2, &[0x5a2a5a, 0xd8d0b0, 0x8a3a6a]);
    stall(ic, 0.4, 2.85, 0xb83a3a, 0xe8d070, &[0xeee6d4, 0xe6dcc8, 0xd8c890]);
    stall(ic, 1.65, 2.95, 0x3a8a4a, 0xe8d070, &[0xb8c0c8, 0x9aa8b0, 0xb8c0c8]);
    stall(ic, 2.95, 2.85, 0x3a5aa8, 0xe8d070, &[0xc8281e, 0xd87a2a, 0x7aa84a]);
    props::barrel(ic, 1.4, 2.55, 0.0);
    props::sack(ic, 2.75, 2.5, 0.0, rgb(0xd8c890));
    props::flag(ic, 3.6, 1.35, 0.0, 30.0, rgb(BANNER_BLUE));
}

/// Level 2 of the church: a cathedral with a raised nave, buttresses, a rose window and two
/// west towers.
fn cathedral(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let nave = [0.3, 0.65, 2.8, 2.15];
    house(ic, nave, 32.0, stone(STONE_WARM), slate(SLATE), 30.0, Axis::A);
    // Buttresses and tall windows along the nave.
    for a in [0.55, 1.1, 1.65, 2.2] {
        ic.block(v3(a - 0.05, 2.15, 0.0), v3(a + 0.05, 2.32, 22.0), stone(STONE), Mat::Rough(rgb(STONE_WARM)));
        let w = a + 0.28;
        ic.pane(Side::Left, 2.15, w - 0.07, w + 0.07, 8.0, 26.0, 0x2a3448);
        ic.pane(Side::Left, 2.15, w - 0.04, w + 0.04, 26.0, 28.0, 0x2a3448);
    }
    // Transept on the visible side.
    house(ic, [1.2, 2.15, 1.85, 2.75], 24.0, stone(STONE_WARM), slate(SLATE), 18.0, Axis::B);
    ic.pane(Side::Left, 2.75, 1.45, 1.6, 8.0, 18.0, 0x2a3448);
    // Two west towers with spires and a rose window between them.
    for (b0, b1) in [(0.6, 1.2), (1.6, 2.2)] {
        ic.shadow(2.8, b0, 3.6, b1, 130.0);
        ic.block(v3(2.8, b0, 0.0), v3(3.6, b1, 66.0), stone(STONE_WARM), Mat::Rough(rgb(STONE)));
        ic.hip(Roof::new([2.8, b0, 3.6, b1], 66.0, 46.0, Axis::A, slate(SLATE), stone(STONE)));
        ic.on_wall(Side::Right, 3.6, b0 + 0.22, b1 - 0.22, 44.0, 54.0, flat(0x1e1a16));
    }
    ic.block(v3(2.8, 1.2, 0.0), v3(3.5, 1.6, 40.0), stone(STONE_WARM), Mat::Rough(rgb(STONE)));
    ic.on_wall(Side::Right, 3.5, 1.28, 1.52, 0.0, 18.0, flat(0x2a1e14));
    ic.on_wall(Side::Right, 3.5, 1.31, 1.49, 0.0, 17.0, Mat::Boards(rgb(DOOR)));
    let (x, y) = ic.project(v3(3.5, 1.4, 30.0));
    let c = ic.canvas();
    c.ring(x, y, 5.0, 3.5, rgb(STONE));
    c.fill_ellipse(x, y, 3.5, 3.5, |_, _, _| rgb(0x6a4a8a));
    for (b, top) in [(0.9, 117.0), (1.9, 117.0)] {
        let (x, y) = ic.project(v3(3.2, b, top));
        let c = ic.canvas();
        c.fill_rect(x as i32, y as i32 - 8, 1, 9, rgb(GOLD));
        c.fill_rect(x as i32 - 2, y as i32 - 6, 5, 1, rgb(GOLD));
    }
    props::tree(ic, 0.6, 2.6, Tree::Pine, 0.4, 19);
}

/// Level 2 of the school: an academy with a library wing, an observatory dome and a cloister
/// garden.
fn academy(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.3, 1.65, 1.5, 2.7, 0.0, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.3, 0.3, 2.5, 1.4];
    ic.block(v3(r[0], r[1], 0.0), v3(r[2], r[3], 12.0), stone(STONE_WARM), stone(STONE_WARM));
    let r_roof = house(ic, r, 26.0, timber(PLASTER), slate(SLATE), 22.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.6, 0.5, 6.0, stone(STONE_DARK));
    for a in [0.6, 0.95, 1.75, 2.1] {
        ic.window(Side::Left, 1.4, a, 4.0, 0.14, 6.0);
        ic.window(Side::Left, 1.4, a, 16.0, 0.12, 6.0);
    }
    ic.door(Side::Left, 1.4, 1.38, 0.26, 13.0);
    // Library wing with a domed observatory.
    house(ic, [1.75, 1.4, 2.6, 2.4], 18.0, stone(STONE_WARM), slate(SLATE), 14.0, Axis::B);
    for b in [1.65, 2.05] {
        ic.pane(Side::Right, 2.6, b - 0.06, b + 0.06, 5.0, 14.0, 0x2a3448);
    }
    ic.cylinder(2.15, 0.8, 0.22, 48.0, 56.0, stone(STONE));
    ic.dome(2.15, 0.8, 0.22, 56.0, 10.0, Mat::Rough(rgb(0x5a8a7a)));
    let (x, y) = ic.project(v3(2.15, 0.8, 66.0));
    ic.canvas().fill_rect(x as i32, y as i32 - 5, 1, 5, rgb(GOLD));
    // Cloister garden with a well and benches.
    props::bed(ic, 0.4, 1.75, 0.85, 2.6, rgb(0x5a8a3a), Axis::B);
    ic.frustum(1.15, 2.2, 0.16, 0.16, 0.0, 5.0, stone(STONE));
    ic.disc(1.15, 2.2, 0.12, 5.0, Mat::Water(rgb(WATER)));
    props::tree(ic, 2.4, 2.7, Tree::Oak, 0.42, 29);
}

/// Level 2 of the hospital: a hospice with a long ward, a chapel bay and a walled herb garden.
fn infirmary(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    let r = [0.25, 0.3, 2.7, 1.3];
    ic.block(v3(r[0], r[1], 0.0), v3(r[2], r[3], 10.0), stone(STONE_WARM), stone(STONE_WARM));
    let r_roof = house(ic, r, 24.0, stone(0xd8d0c0), slate(SLATE), 20.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.55, 0.45, 6.0, stone(STONE_DARK));
    roof_chimney(ic, &r_roof, 2.2, 0.45, 6.0, stone(STONE_DARK));
    for a in [0.55, 0.9, 1.25, 1.95, 2.3] {
        ic.window(Side::Left, 1.3, a, 4.0, 0.12, 6.0);
        ic.window(Side::Left, 1.3, a, 15.0, 0.12, 6.0);
    }
    ic.door(Side::Left, 1.3, 1.6, 0.26, 13.0);
    // Chapel bay with a small bell turret.
    house(ic, [1.8, 1.3, 2.7, 2.2], 18.0, stone(0xd8d0c0), slate(SLATE), 16.0, Axis::B);
    ic.pane(Side::Right, 2.7, 1.68, 1.82, 5.0, 14.0, 0x2a3448);
    ic.block(v3(2.15, 1.65, 34.0), v3(2.35, 1.85, 42.0), stone(STONE), stone(STONE));
    ic.hip(Roof::new([2.13, 1.63, 2.37, 1.87], 42.0, 10.0, Axis::A, slate(SLATE), stone(STONE)));
    let (x, y) = ic.project(v3(1.6, 1.31, 22.0));
    let c = ic.canvas();
    c.fill_rect(x as i32 - 4, y as i32 - 4, 9, 9, rgb(0xeeeae0));
    c.fill_rect(x as i32 - 1, y as i32 - 3, 3, 7, rgb(0x2f7a3a));
    c.fill_rect(x as i32 - 3, y as i32 - 1, 7, 3, rgb(0x2f7a3a));
    // Walled herb garden.
    props::bed(ic, 0.3, 1.6, 0.75, 2.7, rgb(0x5a8a3a), Axis::B);
    props::bed(ic, 0.9, 1.6, 1.35, 2.7, rgb(0x8a6ab0), Axis::B);
    props::bed(ic, 1.45, 2.35, 1.7, 2.75, rgb(0xd8b030), Axis::B);
    ic.block(v3(0.25, 2.8, 0.0), v3(1.75, 2.88, 6.0), stone(STONE), Mat::Rough(rgb(STONE_WARM)));
}

/// Level 2 of the tavern: an inn with a stone ground floor, a guest wing, a stable for
/// travellers' horses and a bigger beer garden.
fn inn(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.25, 0.25, 2.3, 1.6];
    ic.shadow(r[0], r[1], r[2], r[3], 64.0);
    ic.block(v3(r[0], r[1], 0.0), v3(r[2], r[3], 15.0), stone(STONE_WARM), stone(STONE_WARM));
    let up = [0.21, 0.21, 2.34, 1.66];
    ic.block(v3(up[0], up[1], 15.0), v3(up[2], up[3], 30.0), timber(DAUB), timber(DAUB));
    ic.block(v3(up[0], up[1], 30.0), v3(up[2], up[3], 40.0), timber(DAUB), timber(DAUB));
    let roof = Roof::new(up, 40.0, 24.0, Axis::B, tiles(TILE_RED), timber(DAUB));
    ic.gable(roof);
    roof_chimney(ic, &roof, 1.55, 0.4, 6.0, stone(STONE_DARK));
    roof_chimney(ic, &roof, 0.5, 0.4, 6.0, stone(STONE_DARK));
    ic.door(Side::Left, 1.6, 1.1, 0.26, 12.0);
    for a in [0.55, 1.65, 2.0] {
        ic.window(Side::Left, 1.6, a, 5.0, 0.16, 5.0);
    }
    for z in [20.0, 33.0] {
        for a in [0.5, 0.9, 1.3, 1.7, 2.05] {
            ic.window(Side::Left, 1.66, a, z, 0.12, 5.0);
        }
    }
    ic.window(Side::Right, 2.34, 0.9, 20.0, 0.14, 5.0);
    props::wall_sign(ic, Side::Left, 1.66, 1.35, 13.0, rgb(0xd8a030));
    // Guest stable on the right.
    shed(ic, [2.4, 0.3, 2.9, 1.3], 15.0, thatch(THATCH), 9.0, Axis::A, |ic| {
        let hay = Mat::Thatch(rgb(STRAW));
        ic.block(v3(2.5, 0.45, 0.0), v3(2.8, 0.75, 6.0), hay, hay);
    });
    // Beer garden.
    for b in [2.0, 2.55] {
        ic.block(v3(0.5, b, 0.0), v3(1.4, b + 0.25, 6.0), boards(TIMBER), boards(PLANK));
        ic.block(v3(0.5, b - 0.15, 0.0), v3(1.4, b - 0.07, 3.5), boards(TIMBER), boards(PLANK));
    }
    props::barrel(ic, 2.0, 2.3, 0.0);
    props::barrel(ic, 2.2, 2.45, 0.0);
    props::barrel(ic, 2.1, 2.37, 9.0);
    props::tree(ic, 2.6, 2.6, Tree::Oak, 0.45, 41);
}
