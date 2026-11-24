//! Military buildings: barracks with training yard, wooden watchtower, archery post, stables
//! and the castle with curtain walls, round corner towers and a keep.

use crate::draw::rgb;
use crate::iso::{Axis, IsoCanvas, Roof, Side, v3};
use crate::mat::Mat;
use crate::palette::*;
use crate::props;

use super::{boards, flat, house, roof_chimney, shingles, slate, stone, thatch, tiles};

/// Draws military building `id`; returns `false` if `id` is not a military building.
pub fn draw(id: &str, ic: &mut IsoCanvas) -> bool {
    match id {
        "barracks" => barracks(ic),
        "garrison" => garrison(ic),
        "archer_tower" => archer_tower(ic),
        "stud_farm" => stud_farm(ic),
        "watchtower" => watchtower(ic),
        "stone_tower" => stone_tower(ic),
        "archer_post" => archer_post(ic),
        "stables" => stables(ic),
        "castle" => castle(ic),
        _ => return false,
    }
    true
}

fn barracks(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(0x9a8462)));
    let r = [0.3, 0.3, 2.7, 1.3];
    let r_roof = house(ic, r, 20.0, stone(STONE), slate(SLATE), 18.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.7, 0.45, 6.0, stone(STONE_DARK));
    roof_chimney(ic, &r_roof, 2.1, 0.45, 6.0, stone(STONE_DARK));
    ic.door(Side::Left, 1.3, 1.5, 0.28, 14.0);
    for a in [0.6, 0.95, 1.2, 1.85, 2.15, 2.45] {
        ic.window(Side::Left, 1.3, a, 8.0, 0.1, 6.0);
    }
    ic.window(Side::Right, 2.7, 0.8, 8.0, 0.1, 6.0);
    props::flag(ic, 1.5, 1.35, 20.0, 16.0, rgb(BANNER_RED));
    // Training yard.
    props::dummy(ic, 0.7, 1.8);
    props::dummy(ic, 1.2, 1.9);
    props::target(ic, 2.5, 1.7);
    // Weapon rack.
    props::post(ic, 1.8, 2.5, 0.0, 12.0, 0.04);
    props::post(ic, 2.4, 2.5, 0.0, 12.0, 0.04);
    ic.line3(v3(1.8, 2.5, 11.0), v3(2.4, 2.5, 11.0), rgb(TIMBER));
    for i in 0..5 {
        let a = 1.88 + i as f32 * 0.11;
        ic.line3(v3(a, 2.52, 0.0), v3(a, 2.52, 15.0), rgb(PLANK));
        ic.line3(v3(a, 2.52, 15.0), v3(a, 2.52, 18.0), rgb(STEEL));
    }
    props::dummy(ic, 1.0, 2.5);
    props::dummy(ic, 1.5, 2.3);
}

fn watchtower(ic: &mut IsoCanvas) {
    ic.plate(0.08, Mat::Earth(rgb(EARTH)));
    let (lo, hi) = (0.25, 0.75);
    let top = 46.0;
    ic.shadow(lo, lo, hi, hi, 70.0);
    // Back legs, braces, front legs.
    props::post(ic, lo, lo, 0.0, top, 0.06);
    props::post(ic, hi, lo, 0.0, top, 0.06);
    props::post(ic, lo, hi, 0.0, top, 0.06);
    let brace = rgb(TIMBER);
    for k in 0..2 {
        let z0 = k as f32 * 22.0;
        ic.line3(v3(lo, hi, z0), v3(hi, hi, z0 + 22.0), brace);
        ic.line3(v3(hi, lo, z0), v3(hi, hi, z0 + 22.0), brace);
        ic.line3(v3(hi, hi, z0), v3(lo, hi, z0 + 22.0), brace);
    }
    props::post(ic, hi, hi, 0.0, top, 0.06);
    // Ladder.
    ic.line3(v3(0.45, 0.85, 0.0), v3(0.45, 0.76, top), rgb(PLANK));
    ic.line3(v3(0.55, 0.85, 0.0), v3(0.55, 0.76, top), rgb(PLANK));
    // Platform with parapet and roof.
    ic.block(v3(0.15, 0.15, top), v3(0.85, 0.85, top + 3.0), boards(PLANK), boards(PLANK_LIGHT));
    ic.block(v3(0.15, 0.15, top + 3.0), v3(0.85, 0.85, top + 9.0), boards(PLANK), Mat::Flat(rgb(0x3a2a1a)));
    props::post(ic, 0.2, 0.2, top + 9.0, top + 18.0, 0.04);
    props::post(ic, 0.8, 0.2, top + 9.0, top + 18.0, 0.04);
    props::post(ic, 0.2, 0.8, top + 9.0, top + 18.0, 0.04);
    props::post(ic, 0.8, 0.8, top + 9.0, top + 18.0, 0.04);
    ic.hip(Roof::new([0.15, 0.15, 0.85, 0.85], top + 18.0, 16.0, Axis::A, shingles(), boards(PLANK)));
    props::flag(ic, 0.5, 0.5, top + 34.0, 10.0, rgb(BANNER_BLUE));
}

/// Level 2 of the watchtower: a square stone tower with battlements, arrow slits and a banner.
fn stone_tower(ic: &mut IsoCanvas) {
    ic.plate(0.08, Mat::Earth(rgb(EARTH)));
    let (lo, hi) = (0.18, 0.82);
    let top = 62.0;
    ic.shadow(lo, lo, hi, hi, 90.0);
    let m = stone(STONE);
    ic.block(v3(lo, lo, 0.0), v3(hi, hi, top), m, Mat::Rough(rgb(STONE_WARM)));
    // Slightly wider parapet with battlements.
    ic.block(v3(lo - 0.04, lo - 0.04, top), v3(hi + 0.04, hi + 0.04, top + 4.0), stone(STONE_DARK), Mat::Rough(rgb(STONE_WARM)));
    ic.crenellations(lo - 0.04, lo - 0.04, hi + 0.04, hi + 0.04, top + 4.0, 0.08, 5.0, m);
    ic.door(Side::Left, hi, 0.5, 0.2, 12.0);
    for z in [24.0, 44.0] {
        ic.on_wall(Side::Left, hi, 0.47, 0.53, z, z + 8.0, flat(0x18140f));
        ic.on_wall(Side::Right, hi, 0.47, 0.53, z, z + 8.0, flat(0x18140f));
    }
    props::flag(ic, 0.5, 0.5, top + 4.0, 22.0, rgb(BANNER_BLUE));
}

fn archer_post(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(0x9a8462)));
    // Palisade of pointed logs along the back edges.
    let palisade = |ic: &mut IsoCanvas, a: f32, b: f32| {
        ic.cylinder(a, b, 0.05, 0.0, 16.0, Mat::Logs(rgb(LOG)));
        ic.cone(a, b, 0.05, 16.0, 20.0, Mat::Rough(rgb(PLANK)));
    };
    for i in 0..18 {
        palisade(ic, 0.12 + i as f32 * 0.1, 0.12);
    }
    for i in 1..18 {
        palisade(ic, 0.12, 0.12 + i as f32 * 0.1);
    }
    // Raised shooting platform.
    let (a0, b0, a1, b1) = (0.35, 0.35, 1.15, 1.1);
    for &(a, b) in &[(a0, b0), (a1, b0), (a0, b1)] {
        props::post(ic, a, b, 0.0, 14.0, 0.05);
    }
    props::post(ic, a1, b1, 0.0, 14.0, 0.05);
    ic.block(v3(a0 - 0.05, b0 - 0.05, 14.0), v3(a1 + 0.05, b1 + 0.05, 17.0), boards(PLANK), boards(PLANK_LIGHT));
    ic.block(v3(a0 - 0.05, b0 - 0.05, 17.0), v3(a1 + 0.05, b1 + 0.05, 22.0), Mat::Logs(rgb(LOG)), flat(0x3a2a1a));
    ic.line3(v3(1.0, 1.4, 0.0), v3(1.0, b1 + 0.05, 17.0), rgb(PLANK));
    ic.line3(v3(1.1, 1.4, 0.0), v3(1.1, b1 + 0.05, 17.0), rgb(PLANK));
    // A pennant on the platform (no painted archer: people on the map are walkers).
    props::flag(ic, 0.75, 0.7, 22.0, 14.0, rgb(BANNER_RED));
    // Targets in the range.
    props::target(ic, 1.55, 0.6);
    props::target(ic, 1.7, 1.2);
    props::target(ic, 1.6, 1.75);
    props::barrel(ic, 0.6, 1.6, 0.0);
    let hay = Mat::Thatch(rgb(STRAW));
    ic.block(v3(1.1, 1.7, 0.0), v3(1.35, 1.9, 6.0), hay, hay);
}

fn stables(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.25, 0.25, 2.75, 1.7, 0.0, Mat::Earth(rgb(EARTH)));
    let r = [0.3, 0.3, 2.7, 1.3];
    house(ic, r, 17.0, boards(0x6a4a30), thatch(THATCH), 20.0, Axis::A);
    // Stall doors with horse heads looking out.
    for (i, a) in [0.65, 1.15, 1.65, 2.15].iter().enumerate() {
        ic.on_wall(Side::Left, 1.3, a - 0.15, a + 0.15, 0.0, 7.0, Mat::Boards(rgb(0x4a3020)));
        ic.on_wall(Side::Left, 1.3, a - 0.15, a + 0.15, 7.0, 13.0, flat(0x1e160e));
        if i % 2 == 0 {
            let (x, y) = ic.project(v3(*a, 1.32, 9.0));
            let c = ic.canvas();
            c.lit_ellipse(x, y, 2.4, 3.2, rgb(0x6a4228));
            c.fill_rect(x as i32 - 1, y as i32 + 2, 3, 2, rgb(0x4a2e1c));
            c.set(x as i32 - 1, y as i32 - 3, rgb(0x2a1a10));
        }
    }
    let hay = Mat::Thatch(rgb(STRAW));
    ic.block(v3(2.75, 0.4, 0.0), v3(2.95, 0.7, 6.0), hay, hay);
    ic.block(v3(2.75, 0.75, 0.0), v3(2.95, 1.05, 6.0), hay, hay);
    // Paddock with a horse.
    props::fence(ic, 0.3, 1.8, 2.75, 1.8, 7.0);
    let mut horse = crate::draw::Canvas::new(26, 20);
    horse_sprite(&mut horse);
    ic.stamp(&horse, v3(1.5, 2.3, 0.0));
    props::fence(ic, 0.3, 2.8, 2.75, 2.8, 7.0);
    props::fence(ic, 2.75, 1.8, 2.75, 2.8, 7.0);
    props::haystack(ic, 2.2, 2.4, 0.16, 12.0);
}

/// Brown horse (side view) for the stables paddock.
fn horse_sprite(c: &mut crate::draw::Canvas) {
    let body = rgb(0x7a4a2a);
    for &(lx, sw) in &[(7.0f32, 1.0f32), (9.0, -1.0), (16.0, -1.0), (18.0, 1.0)] {
        c.thick_line(lx, 10.0, lx + sw, 18.5, 0.8, rgb(0x5a3420));
    }
    c.thick_line(4.0, 7.0, 2.0, 13.0, 1.0, rgb(0x2a1a10));
    c.lit_ellipse(12.0, 9.0, 8.0, 4.2, body);
    c.thick_line(18.0, 8.0, 21.0, 3.0, 1.8, body);
    c.lit_ellipse(22.0, 3.5, 2.8, 2.0, body);
    c.thick_line(17.0, 6.0, 20.0, 1.5, 0.8, rgb(0x2a1a10));
    c.set(22, 2, rgb(0x101010));
    c.outline(crate::draw::rgba(OUTLINE, 220));
}

fn castle(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS_DARK)));
    ic.ground_rect(0.5, 0.5, 4.5, 4.5, 0.0, Mat::Cobbles(rgb(COBBLE)));
    let m = stone(STONE);
    let top = Mat::Rough(rgb(STONE_WARM));
    let (lo, hi, t) = (0.45, 4.55, 0.32);
    let wall_h = 30.0;
    let tower = |ic: &mut IsoCanvas, a: f32, b: f32| {
        ic.shadow(a - 0.5, b - 0.5, a + 0.5, b + 0.5, 60.0);
        ic.cylinder(a, b, 0.5, 0.0, 46.0, stone(STONE));
        ic.tube(a, b, 0.54, 0.54, 46.0, 49.0, stone(STONE_DARK));
        ic.cone(a, b, 0.58, 49.0, 82.0, slate(SLATE_BLUE));
        ic.on_wall(Side::Left, b + 0.5, a - 0.04, a + 0.04, 26.0, 34.0, flat(0x18140f));
    };
    // Back tower, back walls.
    tower(ic, lo, lo);
    let wall = |ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32| {
        ic.shadow(a0, b0, a1, b1, wall_h);
        ic.block(v3(a0, b0, 0.0), v3(a1, b1, wall_h), m, top);
        ic.crenellations(a0, b0, a1, b1, wall_h, 0.1, 5.0, m);
    };
    wall(ic, lo + 0.4, lo - t * 0.5, hi - 0.4, lo + t * 0.5);
    wall(ic, lo - t * 0.5, lo + 0.4, lo + t * 0.5, hi - 0.4);
    // Keep.
    let (k0, k1) = ((1.6, 1.5), (3.2, 3.1));
    ic.shadow(k0.0, k0.1, k1.0, k1.1, 90.0);
    ic.block(v3(k0.0, k0.1, 0.0), v3(k1.0, k1.1, 64.0), m, top);
    ic.crenellations(k0.0, k0.1, k1.0, k1.1, 64.0, 0.12, 6.0, m);
    for z in [24.0, 44.0] {
        for a in [2.0, 2.4, 2.8] {
            ic.on_wall(Side::Left, k1.1, a - 0.05, a + 0.05, z, z + 8.0, flat(0x18140f));
        }
        for b in [2.0, 2.6] {
            ic.on_wall(Side::Right, k1.0, b - 0.05, b + 0.05, z, z + 8.0, flat(0x18140f));
        }
    }
    // Corner turret on the keep and banners.
    ic.cylinder(k1.0 - 0.2, k0.1 + 0.2, 0.22, 64.0, 80.0, m);
    ic.cone(k1.0 - 0.2, k0.1 + 0.2, 0.27, 80.0, 100.0, slate(SLATE_BLUE));
    props::flag(ic, k1.0 - 0.2, k0.1 + 0.2, 100.0, 14.0, rgb(BANNER_RED));
    props::flag(ic, k0.0 + 0.2, k1.1 - 0.2, 70.0, 14.0, rgb(BANNER_BLUE));
    // Side towers, front walls with the gatehouse, front tower.
    tower(ic, hi, lo);
    tower(ic, lo, hi);
    wall(ic, hi - t * 0.5, lo + 0.4, hi + t * 0.5, hi - 0.4);
    wall(ic, lo + 0.4, hi - t * 0.5, 2.05, hi + t * 0.5);
    // Gatehouse in the lit front wall.
    ic.shadow(2.0, hi - 0.35, 3.0, hi + 0.35, 44.0);
    ic.block(v3(2.0, hi - 0.35, 0.0), v3(3.0, hi + 0.35, 40.0), m, top);
    ic.crenellations(2.0, hi - 0.35, 3.0, hi + 0.35, 40.0, 0.1, 5.0, m);
    ic.on_wall(Side::Left, hi + 0.35, 2.3, 2.7, 0.0, 18.0, flat(0x18140f));
    ic.on_wall(Side::Left, hi + 0.35, 2.35, 2.65, 18.0, 21.0, flat(0x18140f));
    for k in 0..5 {
        let a = 2.33 + k as f32 * 0.085;
        ic.line3(v3(a, hi + 0.36, 8.0), v3(a, hi + 0.36, 20.0), rgb(IRON));
    }
    ic.line3(v3(2.3, hi + 0.36, 11.0), v3(2.7, hi + 0.36, 11.0), rgb(IRON));
    ic.line3(v3(2.3, hi + 0.36, 15.0), v3(2.7, hi + 0.36, 15.0), rgb(IRON));
    props::flag(ic, 2.5, hi, 45.0, 12.0, rgb(BANNER_BLUE));
    wall(ic, 2.95, hi - t * 0.5, hi - 0.4, hi + t * 0.5);
    tower(ic, hi, hi);
}

// --- Level 2 of the military buildings (reached by upgrading) ---------------------------------

/// Level 2 of the barracks: a stone garrison with a battlemented gate tower and a walled
/// training yard.
fn garrison(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(0x9a8462)));
    let r = [0.3, 0.3, 2.7, 1.3];
    let r_roof = house(ic, r, 26.0, stone(STONE), slate(SLATE), 18.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.7, 0.45, 6.0, stone(STONE_DARK));
    roof_chimney(ic, &r_roof, 1.9, 0.45, 6.0, stone(STONE_DARK));
    for a in [0.6, 0.95, 1.2, 1.85, 2.15] {
        ic.window(Side::Left, 1.3, a, 6.0, 0.1, 6.0);
        ic.window(Side::Left, 1.3, a, 17.0, 0.1, 5.0);
    }
    // Gate tower at the front corner.
    let (t0, t1) = ((2.2, 0.95), (2.85, 1.6));
    ic.shadow(t0.0, t0.1, t1.0, t1.1, 70.0);
    ic.block(v3(t0.0, t0.1, 0.0), v3(t1.0, t1.1, 40.0), stone(STONE), Mat::Rough(rgb(STONE_WARM)));
    ic.crenellations(t0.0, t0.1, t1.0, t1.1, 40.0, 0.08, 5.0, stone(STONE));
    ic.on_wall(Side::Left, t1.1, 2.4, 2.65, 0.0, 14.0, flat(0x2a1e14));
    for z in [22.0, 32.0] {
        ic.on_wall(Side::Right, t1.0, 1.24, 1.3, z, z + 7.0, flat(0x18140f));
    }
    props::flag(ic, 2.52, 1.27, 45.0, 18.0, rgb(BANNER_RED));
    // Walled yard with dummies and a weapon rack.
    ic.block(v3(0.25, 2.8, 0.0), v3(2.1, 2.88, 8.0), stone(STONE), Mat::Rough(rgb(STONE_WARM)));
    props::dummy(ic, 0.7, 1.85);
    props::dummy(ic, 1.2, 1.95);
    props::dummy(ic, 0.9, 2.4);
    props::target(ic, 1.8, 2.3);
    props::post(ic, 2.3, 2.0, 0.0, 12.0, 0.04);
    props::post(ic, 2.3, 2.6, 0.0, 12.0, 0.04);
    ic.line3(v3(2.3, 2.0, 11.0), v3(2.3, 2.6, 11.0), rgb(TIMBER));
    for i in 0..4 {
        let b = 2.08 + i as f32 * 0.13;
        ic.line3(v3(2.32, b, 0.0), v3(2.32, b, 15.0), rgb(PLANK));
        ic.line3(v3(2.32, b, 15.0), v3(2.32, b, 18.0), rgb(STEEL));
    }
}

/// Level 2 of the archer post: a square stone tower with battlements over the palisade and a
/// longer shooting range.
fn archer_tower(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(0x9a8462)));
    let palisade = |ic: &mut IsoCanvas, a: f32, b: f32| {
        ic.cylinder(a, b, 0.05, 0.0, 16.0, Mat::Logs(rgb(LOG)));
        ic.cone(a, b, 0.05, 16.0, 20.0, Mat::Rough(rgb(PLANK)));
    };
    for i in 0..18 {
        palisade(ic, 0.12 + i as f32 * 0.1, 0.12);
    }
    for i in 1..18 {
        palisade(ic, 0.12, 0.12 + i as f32 * 0.1);
    }
    let (lo, hi) = (0.3, 1.05);
    let top = 52.0;
    ic.shadow(lo, lo, hi, hi, 80.0);
    ic.block(v3(lo, lo, 0.0), v3(hi, hi, top), stone(STONE), Mat::Rough(rgb(STONE_WARM)));
    ic.block(v3(lo - 0.05, lo - 0.05, top), v3(hi + 0.05, hi + 0.05, top + 4.0), stone(STONE_DARK), Mat::Rough(rgb(STONE_WARM)));
    ic.crenellations(lo - 0.05, lo - 0.05, hi + 0.05, hi + 0.05, top + 4.0, 0.09, 5.0, stone(STONE));
    ic.door(Side::Left, hi, 0.7, 0.2, 12.0);
    for z in [22.0, 38.0] {
        ic.on_wall(Side::Left, hi, 0.64, 0.7, z, z + 8.0, flat(0x18140f));
        ic.on_wall(Side::Right, hi, 0.64, 0.7, z, z + 8.0, flat(0x18140f));
    }
    props::flag(ic, 0.68, 0.68, top + 4.0, 20.0, rgb(BANNER_RED));
    props::target(ic, 1.5, 0.45);
    props::target(ic, 1.7, 1.0);
    props::target(ic, 1.55, 1.55);
    props::target(ic, 1.0, 1.75);
    props::barrel(ic, 0.45, 1.5, 0.0);
    props::barrel(ic, 0.62, 1.62, 0.0);
    let hay = Mat::Thatch(rgb(STRAW));
    ic.block(v3(1.15, 1.4, 0.0), v3(1.4, 1.6, 6.0), hay, hay);
}

/// Level 2 of the stables: a stud farm with a stone stable block, a hay loft and a fenced
/// riding ring with two horses.
fn stud_farm(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.25, 0.25, 2.75, 1.7, 0.0, Mat::Earth(rgb(EARTH)));
    let r = [0.3, 0.3, 2.7, 1.3];
    ic.block(v3(r[0], r[1], 0.0), v3(r[2], r[3], 8.0), stone(STONE_WARM), stone(STONE_WARM));
    let r_roof = house(ic, r, 22.0, boards(0x6a4a30), tiles(TILE_RED), 20.0, Axis::A);
    let _ = r_roof;
    for (i, a) in [0.6, 1.05, 1.5, 1.95, 2.4].iter().enumerate() {
        ic.on_wall(Side::Left, 1.3, a - 0.15, a + 0.15, 0.0, 7.0, Mat::Boards(rgb(0x4a3020)));
        ic.on_wall(Side::Left, 1.3, a - 0.15, a + 0.15, 7.0, 13.0, flat(0x1e160e));
        if i % 2 == 1 {
            let (x, y) = ic.project(v3(*a, 1.32, 9.0));
            let c = ic.canvas();
            c.lit_ellipse(x, y, 2.4, 3.2, rgb(0x6a4228));
            c.fill_rect(x as i32 - 1, y as i32 + 2, 3, 2, rgb(0x4a2e1c));
            c.set(x as i32 - 1, y as i32 - 3, rgb(0x2a1a10));
        }
    }
    // Hay loft door in the gable.
    ic.on_wall(Side::Right, 2.7, 0.65, 0.95, 26.0, 34.0, Mat::Boards(rgb(0x4a3020)));
    // Riding ring with two horses.
    props::fence(ic, 0.3, 1.8, 2.75, 1.8, 7.0);
    let mut horse = crate::draw::Canvas::new(26, 20);
    horse_sprite(&mut horse);
    ic.stamp(&horse, v3(1.0, 2.3, 0.0));
    ic.stamp(&horse.mirrored(), v3(2.0, 2.45, 0.0));
    props::fence(ic, 0.3, 2.85, 2.75, 2.85, 7.0);
    props::fence(ic, 2.75, 1.8, 2.75, 2.85, 7.0);
    props::fence(ic, 0.3, 1.8, 0.3, 2.85, 7.0);
    props::haystack(ic, 2.9, 0.5, 0.12, 10.0);
    props::flag(ic, 2.75, 1.8, 7.0, 18.0, rgb(BANNER_BLUE));
}
