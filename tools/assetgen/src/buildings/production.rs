//! Production buildings: workshops with their characteristic tools and stock in the yard
//! (log piles, kilns, chimneys, mine entrances, sails …).

use std::f32::consts::{PI, TAU};

use crate::draw::{rgb, rgba};
use crate::iso::{Axis, IsoCanvas, Roof, Side, v3};
use crate::mat::Mat;
use crate::nature::Tree;
use crate::palette::*;
use crate::props;

use super::{boards, flat, house, mine, roof_chimney, shed, shingles, slate, stone, thatch, tiles, timber};

/// Draws production building `id`; returns `false` if `id` is not a production building.
pub fn draw(id: &str, ic: &mut IsoCanvas) -> bool {
    match id {
        "woodcutter" => woodcutter(ic),
        "lumber_camp" => lumber_camp(ic),
        "large_quarry" => large_quarry(ic),
        "blast_furnace" => blast_furnace(ic, 0),
        "large_brickworks" => large_brickworks(ic),
        "large_pottery" => large_pottery(ic),
        "large_weaver" => large_weaver(ic),
        "large_tailor" => large_tailor(ic),
        "large_tannery" => large_tannery(ic),
        "large_carpenter" => large_carpenter(ic),
        "large_brewery" => large_brewery(ic),
        "forester" => forester(ic),
        "sawmill" => sawmill(ic),
        "large_sawmill" => large_sawmill(ic),
        "quarry" => quarry(ic),
        "clay_pit" => clay_pit(ic),
        "sand_pit" => sand_pit(ic),
        "iron_mine" => mine(ic, 0x7a706a, 0x8a4a2a, Mat::Rock(rgb(0x8a4a2a))),
        "coal_mine" => {
            mine(ic, 0x6a6660, 0x26221e, Mat::Rough(rgb(0x2a2622)));
            props::heap(ic, 0.35, 1.7, 0.2, 7.0, Mat::Rough(rgb(0x2a2622)));
        }
        "charcoal_kiln" => charcoal_kiln(ic),
        "gold_mine" => gold_mine(ic),
        "smelter" => smelter(ic, 0),
        "smithy" => smithy(ic, 0),
        "master_smithy" => master_smithy(ic, 0),
        "weaponsmith" => weaponsmith(ic, 0),
        "brickworks" => brickworks(ic),
        "pottery" => pottery(ic),
        "weaver" => weaver(ic),
        "tailor" => tailor(ic),
        "tannery" => tannery(ic),
        "carpenter" => carpenter(ic),
        "mill" => mill(ic, 0),
        "large_mill" => large_mill(ic, 0),
        "bakery" => bakery(ic),
        "large_bakery" => large_bakery(ic),
        "brewery" => brewery(ic),
        "cheese_dairy" => cheese_dairy(ic),
        "winery" => winery(ic),
        "jeweler" => jeweler(ic),
        "artisan" => artisan(ic),
        _ => return false,
    }
    true
}

/// Buildings with a work loop (`<id>_work_<frame>`, played by the game while they work): mill
/// sails turning, forge fires flickering.
pub const WORK_LOOPS: [&str; 7] = ["mill", "large_mill", "smithy", "master_smithy", "smelter", "blast_furnace", "weaponsmith"];
/// Frames of a work loop (frame 0 is the finished sprite).
pub const WORK_FRAMES: u32 = 4;

/// Draws frame `frame` of the work loop of `id` (one of [`WORK_LOOPS`]); returns `false` for
/// other buildings.
pub fn draw_work_frame(id: &str, ic: &mut IsoCanvas, frame: u32) -> bool {
    match id {
        "mill" => mill(ic, frame),
        "large_mill" => large_mill(ic, frame),
        "smithy" => smithy(ic, frame),
        "master_smithy" => master_smithy(ic, frame),
        "smelter" => smelter(ic, frame),
        "blast_furnace" => blast_furnace(ic, frame),
        "weaponsmith" => weaponsmith(ic, frame),
        _ => return false,
    }
    true
}

/// Fire colour of a forge in frame `frame` of its work loop (frame 0: the finished sprite):
/// bright flare, glow, ember – far enough apart to read as flicker when zoomed out.
fn fire(frame: u32) -> u32 {
    [GLOW, 0xffc24a, 0xf59a30, 0xc8581a][(frame % 4) as usize]
}

/// Sparks over a forge fire at `(a, b, z)` in the bright frames of its work loop.
fn sparks(ic: &mut IsoCanvas, a: f32, b: f32, z: f32, frame: u32) {
    if frame == 0 || frame == 3 {
        return;
    }
    let (x, y) = ic.project(v3(a, b, z));
    let c = ic.canvas();
    for (i, (dx, dy)) in [(-2.0, -3.0), (1.0, -5.0), (3.0, -2.0), (-1.0, -7.0)].iter().enumerate() {
        if (i as u32 + frame) % 2 == 0 {
            c.set((x + dx) as i32, (y + dy - frame as f32) as i32, rgb(0xffe08a));
        }
    }
}

fn yard(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(EARTH)));
}

fn woodcutter(ic: &mut IsoCanvas) {
    yard(ic);
    props::tree(ic, 0.25, 1.6, Tree::Pine, 0.42, 71);
    house(ic, [0.35, 0.35, 1.2, 1.2], 14.0, boards(PLANK), shingles(), 15.0, Axis::B);
    ic.door(Side::Left, 1.2, 0.75, 0.2, 10.0);
    ic.window(Side::Right, 1.2, 0.75, 5.0, 0.14, 4.0);
    props::log_pile(ic, 1.35, 0.3, 1.9, 0.75, 9.0, Axis::A);
    props::log_pile(ic, 1.35, 0.78, 1.85, 1.05, 5.0, Axis::A);
    // Chopping block with an axe and a few stumps.
    ic.cylinder(1.45, 1.5, 0.1, 0.0, 5.0, Mat::Logs(rgb(LOG)));
    ic.disc(1.45, 1.5, 0.1, 5.0, Mat::EndGrain(rgb(END_GRAIN)));
    let (x, y) = ic.project(v3(1.45, 1.5, 5.0));
    ic.canvas().linef(x, y, x + 3.0, y - 7.0, rgb(PLANK));
    ic.canvas().fill_rect(x as i32 + 2, y as i32 - 9, 3, 2, rgb(STEEL));
    for &(a, b) in &[(0.6, 1.6), (1.0, 1.75), (1.75, 1.75)] {
        ic.cylinder(a, b, 0.07, 0.0, 3.0, Mat::Logs(rgb(BARK)));
        ic.disc(a, b, 0.07, 3.0, Mat::EndGrain(rgb(END_GRAIN)));
    }
}

fn forester(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.3, 1.3, 1.85, 1.9, 0.0, Mat::Earth(rgb(EARTH_DARK)));
    house(ic, [0.3, 0.3, 1.15, 1.1], 14.0, timber(DAUB), thatch(THATCH), 15.0, Axis::A);
    ic.door(Side::Left, 1.1, 0.7, 0.2, 10.0);
    ic.window(Side::Right, 1.15, 0.7, 5.0, 0.14, 4.0);
    props::tree(ic, 1.55, 0.55, Tree::Oak, 0.55, 18);
    // Nursery rows of saplings behind a low fence.
    let mut k = 0;
    for &b in &[1.45, 1.75] {
        for &a in &[0.5, 0.85, 1.2, 1.55] {
            let kind = if k % 2 == 0 { Tree::Pine } else { Tree::Oak };
            props::tree(ic, a, b, kind, 0.22, 90 + k);
            k += 1;
        }
    }
    props::fence(ic, 0.25, 1.95, 1.9, 1.95, 5.0);
    props::fence(ic, 1.9, 1.25, 1.9, 1.95, 5.0);
}

fn sawmill(ic: &mut IsoCanvas) {
    yard(ic);
    // Long open saw shed along `a`.
    shed(ic, [0.35, 0.35, 2.1, 1.25], 16.0, shingles(), 14.0, Axis::A, |ic| {
        ic.block(v3(0.6, 0.65, 0.0), v3(1.9, 0.95, 6.0), boards(TIMBER), boards(PLANK));
        props::log_pile(ic, 0.7, 0.72, 1.5, 0.9, 10.0, Axis::A);
        // Saw frame.
        ic.line3(v3(1.65, 0.8, 6.0), v3(1.65, 0.8, 15.0), rgb(TIMBER_DARK));
        ic.line3(v3(1.6, 0.8, 7.0), v3(1.6, 0.8, 14.0), rgb(STEEL));
    });
    props::log_pile(ic, 2.25, 0.3, 2.9, 0.9, 10.0, Axis::A);
    props::plank_stack(ic, 0.45, 1.4, 1.35, 1.75, 6.0);
    props::plank_stack(ic, 1.55, 1.45, 2.3, 1.8, 9.0);
    props::sack(ic, 2.6, 1.4, 0.0, rgb(0xd8c890));
}

/// Level 2 of the sawmill: a two-storey saw hall on a stone base with a second saw shed and
/// more timber in the yard.
fn large_sawmill(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    // Saw hall along `a` with a stone ground floor.
    ic.shadow(0.3, 0.3, 2.25, 1.1, 34.0);
    ic.block(v3(0.3, 0.3, 0.0), v3(2.25, 1.1, 8.0), stone(STONE_WARM), Mat::Rough(rgb(STONE_WARM)));
    let r_roof = house(ic, [0.3, 0.3, 2.25, 1.1], 24.0, timber(DAUB), tiles(TILE_RED), 16.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.6, 0.6, 8.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.1, 0.75, 0.26, 13.0);
    for a in [1.35, 1.75, 2.1] {
        ic.window(Side::Left, 1.1, a, 14.0, 0.12, 5.0);
    }
    props::wall_sign(ic, Side::Left, 1.1, 1.1, 18.0, rgb(PLANK_LIGHT));
    // Open saw shed in front with a log on the bench and the saw frame.
    shed(ic, [1.3, 1.3, 2.7, 1.85], 15.0, shingles(), 10.0, Axis::A, |ic| {
        ic.block(v3(1.5, 1.45, 0.0), v3(2.5, 1.7, 5.0), boards(TIMBER), boards(PLANK));
        props::log_pile(ic, 1.55, 1.5, 2.4, 1.65, 8.0, Axis::A);
        ic.line3(v3(2.2, 1.57, 5.0), v3(2.2, 1.57, 14.0), rgb(TIMBER_DARK));
        ic.line3(v3(2.15, 1.57, 6.0), v3(2.15, 1.57, 13.0), rgb(STEEL));
    });
    props::log_pile(ic, 0.35, 1.25, 1.2, 1.85, 12.0, Axis::A);
    props::plank_stack(ic, 2.45, 0.3, 2.9, 0.75, 10.0);
    props::plank_stack(ic, 2.45, 0.8, 2.9, 1.2, 7.0);
    props::flag(ic, 2.85, 1.9, 0.0, 22.0, rgb(BANNER_BLUE));
}

fn quarry(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(0x9a9080)));
    let rock = Mat::Rock(rgb(0x8c8780));
    let top = Mat::Rock(rgb(0xa09a90));
    ic.shadow(0.2, 0.2, 1.3, 2.8, 30.0);
    // Stepped quarry face at the back.
    ic.block(v3(0.15, 0.15, 0.0), v3(0.9, 2.85, 30.0), rock, Mat::Grass(rgb(GRASS)));
    ic.block(v3(0.9, 0.15, 0.0), v3(1.4, 2.4, 18.0), rock, top);
    ic.block(v3(1.4, 0.15, 0.0), v3(1.85, 1.8, 8.0), rock, top);
    // Wooden derrick lifting a block.
    let base = v3(2.0, 0.6, 0.0);
    ic.line3(base, v3(2.0, 0.6, 42.0), rgb(TIMBER));
    ic.line3(v3(2.05, 0.6, 0.0), v3(2.05, 0.6, 42.0), rgb(TIMBER_DARK));
    ic.line3(v3(2.0, 0.6, 40.0), v3(1.35, 0.9, 30.0), rgb(TIMBER));
    ic.line3(v3(1.4, 0.88, 30.0), v3(1.4, 0.88, 14.0), rgba(0xd8cca8, 230));
    ic.block(v3(1.33, 0.82, 8.0), v3(1.47, 0.96, 14.0), stone(STONE_WARM), Mat::Rough(rgb(STONE_WARM)));
    // Cut blocks ready for transport.
    props::stone_blocks(ic, 1.7, 2.2);
    props::stone_blocks(ic, 2.2, 1.6);
    props::stone_blocks(ic, 2.3, 2.4);
}

fn clay_pit(ic: &mut IsoCanvas) {
    yard(ic);
    ic.ground_rect(0.25, 0.25, 1.45, 1.55, 0.0, Mat::Earth(rgb(0x7a4a32)));
    ic.ground_rect(0.45, 0.55, 1.0, 1.1, 0.0, Mat::Water(rgb(0x6a8a8a)));
    let clay = Mat::Earth(rgb(0xa8603e));
    props::heap(ic, 0.45, 1.35, 0.22, 9.0, clay);
    props::heap(ic, 1.25, 0.45, 0.25, 11.0, clay);
    // Shovel stuck in the heap.
    let (x, y) = ic.project(v3(1.25, 0.45, 11.0));
    ic.canvas().linef(x, y, x + 2.0, y - 9.0, rgb(PLANK));
    ic.canvas().fill_rect(x as i32 - 1, y as i32 - 1, 3, 3, rgb(IRON));
    shed(ic, [1.4, 1.2, 1.9, 1.85], 12.0, thatch(THATCH_OLD), 8.0, Axis::A, |ic| {
        props::heap(ic, 1.65, 1.5, 0.15, 5.0, Mat::Earth(rgb(0x9c5a3a)));
    });
    // Drying rack with clay blocks.
    for i in 0..4 {
        let a = 0.35 + i as f32 * 0.22;
        ic.block(v3(a, 1.7, 0.0), v3(a + 0.14, 1.84, 4.0), Mat::Rough(rgb(0xb06a48)), Mat::Rough(rgb(0xc07a58)));
    }
}

fn sand_pit(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(SAND)));
    ic.ground_rect(0.3, 0.3, 1.5, 1.4, 0.0, Mat::Earth(rgb(0xc8b07a)));
    let sand = Mat::Earth(rgb(0xe2cf96));
    props::heap(ic, 0.6, 0.6, 0.3, 12.0, sand);
    props::heap(ic, 1.2, 0.5, 0.22, 8.0, sand);
    props::heap(ic, 1.5, 1.45, 0.2, 7.0, sand);
    // Sieve on legs.
    let wood = boards(TIMBER);
    for &(a, b) in &[(0.9, 1.2), (1.2, 1.2)] {
        props::post(ic, a, b, 0.0, 8.0, 0.03);
    }
    ic.block(v3(0.82, 1.15, 8.0), v3(1.28, 1.45, 10.0), wood, Mat::Flat(rgb(0x8a8070)));
    // Small tool shed.
    house(ic, [1.45, 0.2, 1.9, 0.75], 10.0, boards(PLANK), shingles(), 8.0, Axis::B);
    // Wheelbarrow.
    ic.block(v3(0.45, 1.55, 3.0), v3(0.75, 1.75, 7.0), boards(PLANK), Mat::Earth(rgb(0xe2cf96)));
    let (x, y) = ic.project(v3(0.4, 1.65, 3.0));
    ic.canvas().fill_ellipse(x, y, 2.5, 2.5, |_, _, _| rgb(TIMBER_DARK));
}

fn charcoal_kiln(ic: &mut IsoCanvas) {
    yard(ic);
    house(ic, [0.25, 0.25, 0.85, 0.85], 11.0, boards(PLANK), thatch(THATCH_OLD), 10.0, Axis::A);
    props::log_pile(ic, 1.05, 0.25, 1.85, 0.6, 9.0, Axis::A);
    ic.shadow(0.5, 0.8, 1.6, 1.8, 20.0);
    // The smouldering mound melts the snow.
    ic.snow_free(|ic| {
        ic.dome(1.05, 1.25, 0.55, 0.0, 20.0, Mat::Earth(rgb(0x4a3e34)));
        ic.disc(1.05, 1.25, 0.08, 19.6, Mat::Flat(rgb(0x1a1410)));
    });
    ic.mark_smoke(v3(1.05, 1.25, 20.0));
    // Glowing vents around the mound.
    for t in [0.6f32, 1.2, 1.9] {
        let (a, b) = (1.05 + 0.5 * t.cos(), 1.25 + 0.5 * t.sin());
        let (x, y) = ic.project(v3(a, b, 4.0));
        ic.canvas().fill_rect(x as i32, y as i32, 2, 1, rgb(GLOW));
    }
    for &(a, b) in &[(1.7, 1.75), (1.85, 1.55)] {
        props::sack(ic, a, b, 0.0, rgb(0x3a3430));
    }
}

fn gold_mine(ic: &mut IsoCanvas) {
    mine(ic, 0x9a9488, 0xd9a930, Mat::Rock(rgb(0xb8a070)));
    // Strongbox and guard flag.
    ic.block(v3(0.3, 1.6, 0.0), v3(0.5, 1.75, 6.0), boards(TIMBER_DARK), Mat::Flat(rgb(GOLD)));
    props::flag(ic, 1.85, 0.3, 0.0, 26.0, rgb(GOLD));
}

fn smelter(ic: &mut IsoCanvas, frame: u32) {
    yard(ic);
    house(ic, [0.25, 0.3, 1.1, 1.35], 16.0, stone(STONE_DARK), slate(SLATE), 14.0, Axis::A);
    ic.door(Side::Left, 1.35, 0.7, 0.2, 11.0);
    // Bloomery furnace with glowing mouth.
    ic.shadow(1.2, 0.6, 1.8, 1.4, 34.0);
    ic.snow_free(|ic| {
        ic.frustum(1.5, 1.0, 0.34, 0.22, 0.0, 30.0, Mat::Brick(rgb(0x8a5040)));
        ic.tube(1.5, 1.0, 0.24, 0.24, 30.0, 34.0, Mat::Brick(rgb(0x6a4034)));
        ic.disc(1.5, 1.0, 0.18, 34.0, Mat::Flat(rgb(0x1a1210)));
    });
    ic.mark_smoke(v3(1.5, 1.0, 34.0));
    ic.on_wall(Side::Left, 1.33, 1.38, 1.6, 2.0, 9.0, Mat::Flat(rgb(fire(frame))));
    let core = if frame == 0 { 0xffd060 } else { [0xffd060, 0xffe48a, 0xffd870, 0xf0b040][(frame % 4) as usize] };
    ic.on_wall(Side::Left, 1.335, 1.42, 1.56, 3.0, 7.0, Mat::Flat(rgb(core)));
    props::heap(ic, 0.45, 1.65, 0.2, 8.0, Mat::Stone(rgb(0x8a4a2a)));
    props::heap(ic, 0.95, 1.7, 0.18, 6.0, Mat::Rough(rgb(0x2a2622)));
    ic.block(v3(1.55, 1.55, 0.0), v3(1.9, 1.72, 4.0), Mat::Flat(rgb(0x707880)), Mat::Rough(rgb(0x9aa0a8)));
}

fn smithy(ic: &mut IsoCanvas, frame: u32) {
    yard(ic);
    let r = [0.3, 0.3, 1.3, 1.2];
    let r_roof = house(ic, r, 18.0, timber(DAUB), tiles(TILE_RED), 18.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.5, 0.45, 10.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.2, 0.6, 0.22, 12.0);
    ic.window(Side::Right, 1.3, 0.75, 7.0, 0.14, 5.0);
    // Open forge under a lean-to.
    shed(ic, [1.35, 0.45, 1.9, 1.3], 13.0, shingles(), 7.0, Axis::B, |ic| {
        // The forge stands at the open front of the lean-to, its fire mouth visible.
        ic.snow_free(|ic| ic.block(v3(1.45, 0.82, 0.0), v3(1.75, 1.12, 7.0), Mat::Brick(rgb(BRICK)), Mat::Flat(rgb(fire(frame)))));
        ic.on_wall(Side::Left, 1.12, 1.52, 1.68, 1.5, 5.0, Mat::Flat(rgb(fire(frame))));
        // A second mouth on the side that shows under the eave.
        ic.on_wall(Side::Right, 1.75, 0.88, 1.06, 1.5, 5.0, Mat::Flat(rgb(fire(frame))));
        sparks(ic, 1.6, 1.22, 6.0, frame);
    });
    // Anvil on a block.
    ic.block(v3(1.35, 1.45, 0.0), v3(1.5, 1.6, 4.0), Mat::Logs(rgb(LOG)), Mat::EndGrain(rgb(END_GRAIN)));
    ic.block(v3(1.3, 1.47, 4.0), v3(1.56, 1.58, 7.0), Mat::Flat(rgb(IRON)), Mat::Flat(rgb(0x8a9098)));
    props::barrel(ic, 1.75, 1.65, 0.0);
    props::barrel(ic, 0.45, 1.5, 0.0);
}

/// Level 2 of the smithy: a stone forge house with two chimneys, a second forge and two anvils.
fn master_smithy(ic: &mut IsoCanvas, frame: u32) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.28, 0.28, 1.35, 1.2];
    let r_roof = house(ic, r, 20.0, stone(STONE_DARK), slate(SLATE), 18.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.45, 0.45, 12.0, Mat::Brick(rgb(BRICK)));
    roof_chimney(ic, &r_roof, 1.1, 0.5, 9.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.2, 0.65, 0.22, 12.0);
    ic.window(Side::Left, 1.2, 1.05, 8.0, 0.14, 5.0);
    ic.window(Side::Right, 1.35, 0.75, 8.0, 0.14, 5.0);
    props::wall_sign(ic, Side::Left, 1.2, 0.4, 16.0, rgb(STEEL));
    // Two open forges under a long lean-to.
    shed(ic, [1.4, 0.3, 1.92, 1.5], 14.0, shingles(), 7.0, Axis::B, |ic| {
        for (k, b) in [0.42, 0.95].into_iter().enumerate() {
            // The two fires flicker out of step.
            let f = if frame == 0 { 0 } else { (frame + 2 * k as u32 - 1) % 3 + 1 };
            ic.snow_free(|ic| ic.block(v3(1.5, b, 0.0), v3(1.78, b + 0.3, 7.0), Mat::Brick(rgb(BRICK)), Mat::Flat(rgb(fire(f)))));
            ic.on_wall(Side::Left, b + 0.3, 1.56, 1.72, 1.5, 5.0, Mat::Flat(rgb(fire(f))));
            // The fire mouth facing the open side of the lean-to (visible under the eave).
            ic.on_wall(Side::Right, 1.78, b + 0.06, b + 0.24, 1.5, 5.0, Mat::Flat(rgb(fire(f))));
            sparks(ic, 1.64, b + 0.4, 6.0, f);
        }
    });
    // Anvils on blocks.
    for (a, b) in [(0.5, 1.45), (1.0, 1.62)] {
        ic.block(v3(a, b, 0.0), v3(a + 0.15, b + 0.15, 4.0), Mat::Logs(rgb(LOG)), Mat::EndGrain(rgb(END_GRAIN)));
        ic.block(v3(a - 0.04, b + 0.02, 4.0), v3(a + 0.21, b + 0.13, 7.0), Mat::Flat(rgb(IRON)), Mat::Flat(rgb(0x8a9098)));
    }
    props::barrel(ic, 1.75, 1.7, 0.0);
    props::crate_box(ic, 1.45, 1.72, 0.14, 6.0, 0.0);
}

fn weaponsmith(ic: &mut IsoCanvas, frame: u32) {
    yard(ic);
    let r = [0.3, 0.3, 1.35, 1.25];
    let r_roof = house(ic, r, 18.0, stone(STONE_DARK), slate(SLATE), 18.0, Axis::B);
    roof_chimney(ic, &r_roof, 1.0, 0.45, 10.0, stone(STONE_DARK));
    ic.door(Side::Left, 1.25, 0.8, 0.22, 12.0);
    props::flag(ic, 0.4, 1.3, 18.0, 14.0, rgb(BANNER_RED));
    // Weapon rack with swords and shields.
    let wood = boards(TIMBER);
    ic.block(v3(1.45, 0.4, 0.0), v3(1.55, 1.4, 2.0), wood, wood);
    props::post(ic, 1.5, 0.42, 0.0, 14.0, 0.04);
    props::post(ic, 1.5, 1.38, 0.0, 14.0, 0.04);
    ic.line3(v3(1.5, 0.42, 13.0), v3(1.5, 1.38, 13.0), rgb(TIMBER));
    for i in 0..5 {
        let b = 0.55 + i as f32 * 0.18;
        ic.line3(v3(1.53, b, 2.0), v3(1.53, b, 12.0), rgb(STEEL));
        ic.line3(v3(1.53, b - 0.03, 9.0), v3(1.53, b + 0.03, 9.0), rgb(GOLD));
    }
    for (i, col) in [BANNER_RED, BANNER_BLUE].iter().enumerate() {
        let (x, y) = ic.project(v3(1.0 + i as f32 * 0.4, 1.3, 9.0));
        ic.canvas().lit_ellipse(x, y, 3.5, 4.0, rgb(*col));
        ic.canvas().fill_rect(x as i32, y as i32 - 3, 1, 6, rgb(GOLD));
    }
    // The forge pit in the yard: its fire flickers and sparks fly while the smith works.
    ic.snow_free(|ic| ic.block(v3(1.6, 1.5, 0.0), v3(1.78, 1.68, 6.0), Mat::Brick(rgb(BRICK)), Mat::Flat(rgb(fire(frame)))));
    sparks(ic, 1.69, 1.59, 6.0, frame);
}

fn brickworks(ic: &mut IsoCanvas) {
    yard(ic);
    // Drying shed with brick stacks (long side along `a`).
    shed(ic, [0.3, 0.3, 1.9, 1.0], 13.0, thatch(THATCH_OLD), 10.0, Axis::A, |ic| {
        for i in 0..4 {
            let a = 0.45 + i as f32 * 0.36;
            ic.block(v3(a, 0.45, 0.0), v3(a + 0.26, 0.85, 7.0), Mat::Brick(rgb(0xb8705a)), Mat::Brick(rgb(0xc8806a)));
        }
    });
    // Kiln with tall chimney.
    ic.shadow(2.05, 0.3, 2.75, 1.2, 40.0);
    ic.snow_free(|ic| {
        ic.block(v3(2.05, 0.35, 0.0), v3(2.75, 1.15, 16.0), Mat::Brick(rgb(BRICK)), Mat::Brick(rgb(0x8a4a38)));
        ic.dome(2.4, 0.75, 0.38, 16.0, 8.0, Mat::Brick(rgb(BRICK)));
    });
    props::chimney(ic, 2.5, 0.45, 16.0, 46.0, Mat::Brick(rgb(BRICK)));
    ic.on_wall(Side::Left, 1.15, 2.25, 2.5, 0.0, 7.0, Mat::Flat(rgb(GLOW)));
    // Clay heap, sand heap and finished bricks.
    props::heap(ic, 0.5, 1.5, 0.25, 9.0, Mat::Earth(rgb(0xa8603e)));
    props::heap(ic, 1.15, 1.6, 0.2, 7.0, Mat::Earth(rgb(SAND)));
    for (i, &(a, b)) in [(1.75, 1.35), (2.2, 1.45), (2.6, 1.5)].iter().enumerate() {
        let h = 6.0 + i as f32 * 2.0;
        ic.block(v3(a, b, 0.0), v3(a + 0.3, b + 0.3, h), Mat::Brick(rgb(BRICK)), Mat::Brick(rgb(0xb0604a)));
    }
}

fn pottery(ic: &mut IsoCanvas) {
    yard(ic);
    house(ic, [0.3, 0.3, 1.25, 1.15], 15.0, timber(DAUB), thatch(THATCH), 16.0, Axis::A);
    ic.door(Side::Left, 1.15, 0.7, 0.2, 10.0);
    ic.window(Side::Right, 1.25, 0.7, 6.0, 0.14, 4.0);
    // Round kiln.
    ic.shadow(1.3, 0.35, 1.85, 0.95, 18.0);
    ic.snow_free(|ic| {
        ic.frustum(1.6, 0.65, 0.3, 0.26, 0.0, 8.0, Mat::Brick(rgb(0x9a5a44)));
        ic.dome(1.6, 0.65, 0.26, 8.0, 10.0, Mat::Brick(rgb(0x9a5a44)));
    });
    ic.mark_smoke(v3(1.6, 0.65, 18.0));
    ic.on_wall(Side::Left, 0.92, 1.52, 1.68, 1.0, 5.0, Mat::Flat(rgb(GLOW)));
    // Table with pots.
    ic.block(v3(1.3, 1.3, 0.0), v3(1.85, 1.55, 6.0), boards(TIMBER), boards(PLANK));
    for i in 0..4 {
        let a = 1.38 + i as f32 * 0.13;
        ic.frustum(a, 1.42, 0.035, 0.05, 6.0, 11.0, Mat::Rough(rgb(0xb8643c)));
    }
    for i in 0..3 {
        ic.frustum(0.45 + i as f32 * 0.2, 1.65, 0.06, 0.04, 0.0, 7.0, Mat::Rough(rgb(0xa85a38)));
    }
}

fn weaver(ic: &mut IsoCanvas) {
    yard(ic);
    house(ic, [0.3, 0.3, 1.4, 1.15], 17.0, timber(PLASTER), tiles(TILE_RED), 17.0, Axis::A);
    ic.door(Side::Left, 1.15, 0.6, 0.2, 11.0);
    ic.window(Side::Left, 1.15, 1.05, 7.0, 0.16, 5.0);
    // Loom under an awning.
    props::awning(ic, 1.45, 0.4, 1.9, 1.2, 15.0, 10.0, Side::Right, rgb(0x6a4a8a), rgb(0xe0d8c8));
    let (x, y) = ic.project(v3(1.6, 1.25, 0.0));
    let c = ic.canvas();
    c.fill_rect(x as i32 - 6, y as i32 - 10, 1, 10, rgb(TIMBER));
    c.fill_rect(x as i32 + 4, y as i32 - 12, 1, 10, rgb(TIMBER));
    c.fill_rect(x as i32 - 6, y as i32 - 10, 11, 1, rgb(TIMBER));
    for k in 0..5 {
        c.fill_rect(x as i32 - 5 + k * 2, y as i32 - 9, 1, 6, rgb([0xc84040, 0x4060b0, 0xd8b030, 0x40904a, 0xe8e0d0][k as usize]));
    }
    // Wool bales and cloth bolts.
    for i in 0..3 {
        let a = 0.4 + i as f32 * 0.25;
        ic.block(v3(a, 1.45, 0.0), v3(a + 0.2, 1.65, 6.0), Mat::Rough(rgb(0xeae2d0)), Mat::Rough(rgb(0xf4eee0)));
    }
    props::rack(ic, 1.2, 1.8, 1.85, 1.8, 11.0, &[rgb(0xb83a3a), rgb(0x3a5aa8), rgb(0x3a8a4a)]);
}

fn tailor(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.3, 0.3, 1.4, 1.3];
    let r_roof = house(ic, r, 18.0, timber(WHITEWASH), tiles(TILE_DARK), 18.0, Axis::B);
    roof_chimney(ic, &r_roof, 1.05, 0.5, 5.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.3, 0.85, 0.2, 12.0);
    ic.window(Side::Left, 1.3, 0.5, 7.0, 0.16, 5.0);
    ic.window(Side::Right, 1.4, 0.8, 7.0, 0.16, 5.0);
    props::wall_sign(ic, Side::Left, 1.3, 1.2, 14.0, rgb(0x8a3a6a));
    // Clothes line with garments.
    props::rack(ic, 1.5, 0.35, 1.5, 1.85, 13.0, &[rgb(0x8a3a3a), rgb(0x3a5a8a), rgb(0xd8c8a0), rgb(0x4a7a3a)]);
    // Dressmaker's dummy.
    let (x, y) = ic.project(v3(0.6, 1.7, 0.0));
    let c = ic.canvas();
    c.fill_rect(x as i32, y as i32 - 4, 1, 4, rgb(TIMBER));
    c.lit_ellipse(x + 0.5, y - 8.0, 2.6, 4.0, rgb(0x6a2a4a));
}

fn tannery(ic: &mut IsoCanvas) {
    yard(ic);
    house(ic, [0.3, 0.3, 1.15, 1.1], 14.0, boards(PLANK), shingles(), 13.0, Axis::A);
    ic.door(Side::Left, 1.1, 0.7, 0.2, 10.0);
    // Drying frames with stretched hides.
    for (i, &(a0, b0, a1, b1)) in [(1.3, 0.3, 1.3, 1.0), (1.65, 0.45, 1.65, 1.15)].iter().enumerate() {
        props::rack(ic, a0, b0, a1, b1, 13.0, &[rgb(0x9a6a44 + i as u32 * 0x080604), rgb(0x8a5a3a)]);
    }
    // Tanning vats.
    for &(a, b, col) in &[(0.45, 1.5, 0x5a3a20u32), (0.85, 1.6, 0x6a4a2a), (1.25, 1.55, 0x4a3018)] {
        ic.frustum(a, b, 0.14, 0.14, 0.0, 6.0, boards(PLANK));
        ic.disc(a, b, 0.12, 6.0, Mat::Flat(rgb(col)));
    }
}

fn carpenter(ic: &mut IsoCanvas) {
    yard(ic);
    house(ic, [0.3, 0.3, 1.4, 1.2], 17.0, timber(DAUB), shingles(), 17.0, Axis::A);
    ic.on_wall(Side::Left, 1.2, 0.5, 0.95, 0.0, 12.0, Mat::Flat(rgb(0x2a2018)));
    ic.window(Side::Right, 1.4, 0.75, 7.0, 0.14, 5.0);
    props::plank_stack(ic, 1.5, 0.3, 1.9, 1.1, 9.0);
    // Sawhorse with a plank.
    let (x, y) = ic.project(v3(1.0, 1.55, 0.0));
    let c = ic.canvas();
    c.linef(x - 6.0, y, x - 4.0, y - 6.0, rgb(TIMBER));
    c.linef(x + 4.0, y - 1.0, x + 6.0, y - 7.0, rgb(TIMBER));
    c.fill_rect(x as i32 - 9, y as i32 - 8, 18, 2, rgb(PLANK_LIGHT));
    // A finished chair and table outside.
    ic.block(v3(1.5, 1.45, 4.0), v3(1.8, 1.7, 6.0), boards(PLANK), boards(PLANK_LIGHT));
    props::post(ic, 1.53, 1.48, 0.0, 4.0, 0.03);
    props::post(ic, 1.77, 1.67, 0.0, 4.0, 0.03);
    ic.block(v3(1.25, 1.75, 0.0), v3(1.38, 1.88, 4.0), boards(PLANK), boards(PLANK));
    ic.block(v3(1.25, 1.75, 4.0), v3(1.28, 1.88, 9.0), boards(PLANK), boards(PLANK));
}

fn mill(ic: &mut IsoCanvas, frame: u32) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.9, 1.2, 1.2, 1.96, 0.0, Mat::Earth(rgb(EARTH)));
    let (ca, cb) = (0.95, 0.95);
    ic.shadow(0.45, 0.45, 1.45, 1.45, 70.0);
    ic.frustum(ca, cb, 0.55, 0.42, 0.0, 44.0, Mat::Stone(rgb(STONE_WARM)));
    // Cap.
    ic.gable(Roof::new([ca - 0.42, cb - 0.36, ca + 0.42, cb + 0.36], 44.0, 16.0, Axis::A, shingles(), boards(PLANK)));
    ic.door(Side::Left, cb + 0.5, ca, 0.2, 11.0);
    ic.window(Side::Left, cb + 0.46, ca + 0.1, 26.0, 0.1, 5.0);
    // Four lattice sails in the plane facing the viewer, hub on the right gable.
    let (hx, hy) = ic.project(v3(ca + 0.55, cb + 0.1, 50.0));
    let c = ic.canvas();
    for k in 0..4 {
        // Frames turn the sails by an eighth of a half turn (four-fold symmetric: 4 frames
        // make a seamless loop).
        let ang = PI * 0.25 + 0.1 + k as f32 * TAU / 4.0 + frame as f32 * PI / 8.0;
        let (dx, dy) = (ang.cos(), ang.sin());
        let (nx, ny) = (-dy, dx);
        let (l0, l1, w) = (4.0, 40.0, 6.0);
        let p = |l: f32, s: f32| (hx + dx * l + nx * s, hy + dy * l + ny * s);
        // Cloth panel.
        let cloth = [p(l0 + 4.0, 0.5), p(l1, 0.5), p(l1, w), p(l0 + 4.0, w)];
        c.fill_polygon(&cloth, |x, y| if (x + y) % 3 == 0 { rgb(0xd8ccb0) } else { rgb(0xf0e8d6) });
        // Lattice.
        let (sx, sy) = p(0.0, 0.0);
        let (ex, ey) = p(l1 + 1.0, 0.0);
        c.thick_line(sx, sy, ex, ey, 0.9, rgb(TIMBER_DARK));
        let (ax, ay) = p(l0 + 4.0, w);
        let (bx, by) = p(l1, w);
        c.linef(ax, ay, bx, by, rgb(TIMBER));
        for i in 0..6 {
            let l = l0 + 4.0 + (l1 - l0 - 4.0) * i as f32 / 5.0;
            let (x0, y0) = p(l, 0.0);
            let (x1, y1) = p(l, w);
            c.linef(x0, y0, x1, y1, rgb(TIMBER));
        }
    }
    c.fill_ellipse(hx, hy, 2.5, 2.5, |_, _, _| rgb(TIMBER_DARK));
    props::sack(ic, 1.45, 1.7, 0.0, rgb(0xe8e0cc));
    props::sack(ic, 1.62, 1.55, 0.0, rgb(0xe0d6c0));
}

/// Level 2 of the mill: a taller stone tower mill with a gallery, longer sails and a flour store.
fn large_mill(ic: &mut IsoCanvas, frame: u32) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.9, 1.2, 1.2, 1.96, 0.0, Mat::Earth(rgb(EARTH)));
    // Flour store at the back right.
    shed(ic, [1.35, 0.2, 1.9, 0.75], 12.0, shingles(), 7.0, Axis::B, |ic| {
        props::sack(ic, 1.5, 0.35, 0.0, rgb(0xeee6d4));
        props::sack(ic, 1.68, 0.45, 0.0, rgb(0xe6dcc8));
    });
    let (ca, cb) = (0.9, 0.95);
    ic.shadow(0.4, 0.45, 1.4, 1.45, 84.0);
    ic.frustum(ca, cb, 0.6, 0.44, 0.0, 54.0, Mat::Stone(rgb(STONE)));
    // Gallery around the tower.
    ic.tube(ca, cb, 0.62, 0.62, 30.0, 32.0, boards(PLANK));
    ic.gable(Roof::new([ca - 0.44, cb - 0.38, ca + 0.44, cb + 0.38], 54.0, 18.0, Axis::A, slate(SLATE), boards(PLANK)));
    ic.door(Side::Left, cb + 0.56, ca, 0.22, 12.0);
    ic.window(Side::Left, cb + 0.5, ca + 0.1, 20.0, 0.1, 5.0);
    ic.window(Side::Left, cb + 0.47, ca - 0.1, 38.0, 0.1, 5.0);
    let (hx, hy) = ic.project(v3(ca + 0.57, cb + 0.1, 61.0));
    let c = ic.canvas();
    for k in 0..4 {
        // Frames turn the sails by an eighth of a half turn (four-fold symmetric: 4 frames
        // make a seamless loop).
        let ang = PI * 0.25 + 0.1 + k as f32 * TAU / 4.0 + frame as f32 * PI / 8.0;
        let (dx, dy) = (ang.cos(), ang.sin());
        let (nx, ny) = (-dy, dx);
        let (l0, l1, w) = (4.0, 48.0, 7.0);
        let p = |l: f32, s: f32| (hx + dx * l + nx * s, hy + dy * l + ny * s);
        let cloth = [p(l0 + 4.0, 0.5), p(l1, 0.5), p(l1, w), p(l0 + 4.0, w)];
        c.fill_polygon(&cloth, |x, y| if (x + y) % 3 == 0 { rgb(0xd8ccb0) } else { rgb(0xf0e8d6) });
        let (sx, sy) = p(0.0, 0.0);
        let (ex, ey) = p(l1 + 1.0, 0.0);
        c.thick_line(sx, sy, ex, ey, 1.0, rgb(TIMBER_DARK));
        let (ax, ay) = p(l0 + 4.0, w);
        let (bx, by) = p(l1, w);
        c.linef(ax, ay, bx, by, rgb(TIMBER));
        for i in 0..7 {
            let l = l0 + 4.0 + (l1 - l0 - 4.0) * i as f32 / 6.0;
            let (x0, y0) = p(l, 0.0);
            let (x1, y1) = p(l, w);
            c.linef(x0, y0, x1, y1, rgb(TIMBER));
        }
    }
    c.fill_ellipse(hx, hy, 3.0, 3.0, |_, _, _| rgb(TIMBER_DARK));
    props::sack(ic, 1.45, 1.7, 0.0, rgb(0xe8e0cc));
    props::sack(ic, 1.62, 1.55, 0.0, rgb(0xe0d6c0));
    props::sack(ic, 1.7, 1.75, 0.0, rgb(0xeee6d4));
}

/// Level 2 of the bakery: a larger bakehouse with two ovens and a sales counter.
fn large_bakery(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.25, 0.25, 1.3, 1.25];
    let r_roof = house(ic, r, 22.0, timber(PLASTER), tiles(TILE_RED), 18.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.5, 0.5, 9.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.25, 0.65, 0.2, 12.0);
    ic.window(Side::Left, 1.25, 1.05, 7.0, 0.16, 5.0);
    ic.window(Side::Left, 1.25, 0.65, 16.0, 0.12, 4.0);
    props::wall_sign(ic, Side::Left, 1.25, 0.35, 16.0, rgb(0xc89040));
    // Two bread ovens on the right with one chimney.
    ic.shadow(1.32, 0.3, 1.92, 1.3, 22.0);
    ic.snow_free(|ic| ic.block(v3(1.32, 0.3, 0.0), v3(1.88, 1.3, 5.0), stone(STONE), Mat::Rough(rgb(STONE))));
    for b in [0.55, 1.05] {
        ic.snow_free(|ic| ic.dome(1.6, b, 0.22, 5.0, 10.0, Mat::Brick(rgb(0xa86048))));
        ic.on_wall(Side::Right, 1.89, b - 0.07, b + 0.07, 6.0, 9.0, Mat::Flat(rgb(GLOW)));
    }
    props::chimney(ic, 1.48, 0.33, 5.0, 36.0, Mat::Brick(rgb(BRICK)));
    // Sales counter with loaves and an awning.
    ic.block(v3(0.35, 1.45, 0.0), v3(1.15, 1.7, 6.0), boards(TIMBER), boards(PLANK));
    for i in 0..5 {
        let (x, y) = ic.project(v3(0.45 + i as f32 * 0.15, 1.57, 6.0));
        ic.canvas().lit_ellipse(x, y - 1.0, 2.4, 1.5, rgb(0xc8843a));
    }
    props::awning(ic, 0.3, 1.42, 1.2, 1.78, 15.0, 11.0, Side::Left, rgb(0xb83a2a), rgb(0xf0e6d0));
    props::sack(ic, 1.45, 1.6, 0.0, rgb(0xeee6d4));
    props::sack(ic, 1.65, 1.7, 0.0, rgb(0xe6dcc8));
}

fn bakery(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.3, 0.3, 1.35, 1.2];
    house(ic, r, 18.0, timber(PLASTER), tiles(TILE_RED), 18.0, Axis::A);
    ic.door(Side::Left, 1.2, 0.7, 0.2, 12.0);
    ic.window(Side::Left, 1.2, 1.1, 7.0, 0.16, 5.0);
    props::wall_sign(ic, Side::Left, 1.2, 0.4, 15.0, rgb(0xc89040));
    // Bread oven with chimney attached on the right.
    ic.shadow(1.35, 0.4, 1.9, 1.0, 22.0);
    ic.snow_free(|ic| {
        ic.block(v3(1.35, 0.4, 0.0), v3(1.85, 1.0, 5.0), stone(STONE), Mat::Rough(rgb(STONE)));
        ic.dome(1.6, 0.7, 0.26, 5.0, 11.0, Mat::Brick(rgb(0xa86048)));
    });
    ic.on_wall(Side::Right, 1.86, 0.62, 0.78, 6.0, 10.0, Mat::Flat(rgb(GLOW)));
    props::chimney(ic, 1.5, 0.42, 5.0, 34.0, Mat::Brick(rgb(BRICK)));
    props::sack(ic, 0.5, 1.5, 0.0, rgb(0xeee6d4));
    props::sack(ic, 0.72, 1.6, 0.0, rgb(0xe6dcc8));
    ic.block(v3(1.2, 1.45, 0.0), v3(1.7, 1.7, 6.0), boards(TIMBER), boards(PLANK));
    for i in 0..3 {
        let (x, y) = ic.project(v3(1.3 + i as f32 * 0.14, 1.55, 6.0));
        ic.canvas().lit_ellipse(x, y - 1.0, 2.4, 1.5, rgb(0xc8843a));
    }
}

fn brewery(ic: &mut IsoCanvas) {
    yard(ic);
    let r = [0.3, 0.3, 2.3, 1.2];
    let r_roof = house(ic, r, 20.0, stone(STONE_WARM), tiles(TILE_DARK), 18.0, Axis::A);
    roof_chimney(ic, &r_roof, 1.8, 0.45, 8.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.2, 1.2, 0.3, 13.0);
    for a in [0.6, 1.7, 2.05] {
        ic.window(Side::Left, 1.2, a, 8.0, 0.14, 6.0);
    }
    // Copper kettle under a lean-to.
    shed(ic, [2.35, 0.4, 2.9, 1.1], 14.0, shingles(), 8.0, Axis::B, |ic| {
        ic.frustum(2.62, 0.75, 0.2, 0.17, 0.0, 10.0, Mat::Rough(rgb(0xb8683a)));
        ic.dome(2.62, 0.75, 0.17, 10.0, 5.0, Mat::Rough(rgb(0xc87a48)));
    });
    // Barrel pyramid.
    for (i, &(a, b, z)) in [(0.5, 1.45, 0.0), (0.68, 1.45, 0.0), (0.86, 1.45, 0.0), (0.59, 1.45, 9.0), (0.77, 1.45, 9.0)].iter().enumerate() {
        let _ = i;
        props::barrel(ic, a, b, z);
    }
    props::barrel(ic, 1.6, 1.6, 0.0);
    props::barrel(ic, 1.8, 1.55, 0.0);
    props::sack(ic, 2.3, 1.6, 0.0, rgb(0xd8c890));
}

fn cheese_dairy(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.6, 1.2, 1.4, 1.96, 0.0, Mat::Earth(rgb(EARTH)));
    house(ic, [0.3, 0.3, 1.4, 1.2], 16.0, timber(WHITEWASH), shingles(), 17.0, Axis::B);
    ic.door(Side::Left, 1.2, 0.95, 0.2, 11.0);
    ic.window(Side::Left, 1.2, 0.55, 7.0, 0.14, 4.0);
    ic.window(Side::Right, 1.4, 0.7, 7.0, 0.14, 4.0);
    // Cheese rack.
    let wood = boards(TIMBER);
    ic.block(v3(1.5, 0.4, 0.0), v3(1.75, 1.2, 1.5), wood, wood);
    for z in [1.5, 7.0] {
        ic.block(v3(1.5, 0.4, z), v3(1.75, 1.2, z + 1.0), wood, boards(PLANK));
        for i in 0..4 {
            let b = 0.5 + i as f32 * 0.18;
            ic.frustum(1.62, b, 0.08, 0.08, z + 1.0, z + 4.5, Mat::Rough(rgb(0xe8c050)));
        }
    }
    props::post(ic, 1.52, 0.42, 0.0, 12.0, 0.03);
    props::post(ic, 1.73, 1.18, 0.0, 12.0, 0.03);
    // Milk cans.
    for &(a, b) in &[(0.5, 1.5), (0.68, 1.58)] {
        ic.frustum(a, b, 0.06, 0.05, 0.0, 7.0, Mat::Rough(rgb(STEEL)));
        ic.disc(a, b, 0.03, 8.0, Mat::Flat(rgb(IRON)));
    }
}

fn winery(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    let r = [0.3, 0.3, 1.7, 1.15];
    house(ic, r, 18.0, stone(STONE_WARM), tiles(TILE_RED), 17.0, Axis::A);
    ic.door(Side::Left, 1.15, 0.9, 0.24, 12.0);
    ic.window(Side::Left, 1.15, 0.5, 8.0, 0.14, 5.0);
    ic.window(Side::Left, 1.15, 1.35, 8.0, 0.14, 5.0);
    // Cellar entrance.
    ic.block(v3(1.75, 0.4, 0.0), v3(2.05, 0.9, 5.0), stone(STONE), Mat::Rough(rgb(STONE)));
    ic.on_wall(Side::Right, 2.05, 0.5, 0.8, 0.0, 4.0, Mat::Flat(rgb(0x1a1410)));
    // Vine rows with grapes.
    for row in 0..4 {
        let a = 2.1 + row as f32 * 0.22;
        props::fence(ic, a, 0.25, a, 1.8, 7.0);
        for i in 0..7 {
            let b = 0.35 + i as f32 * 0.2;
            let (x, y) = ic.project(v3(a, b, 6.0));
            let c = ic.canvas();
            c.lit_ellipse(x, y, 2.6, 2.2, rgb(0x4a7a2c));
            if (i + row) % 2 == 0 {
                c.fill_rect(x as i32, y as i32 + 1, 2, 2, rgb(0x5a2a5a));
            }
        }
    }
    props::barrel(ic, 0.6, 1.5, 0.0);
    props::barrel(ic, 0.8, 1.55, 0.0);
    props::barrel(ic, 0.7, 1.52, 9.0);
}

fn jeweler(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.35, 0.35, 1.5, 1.4];
    let r_roof = house(ic, r, 20.0, stone(STONE_WARM), slate(SLATE_BLUE), 20.0, Axis::B);
    roof_chimney(ic, &r_roof, 1.1, 0.5, 5.0, stone(STONE_DARK));
    ic.door(Side::Left, 1.4, 1.0, 0.2, 13.0);
    // Display window with goods.
    ic.on_wall(Side::Left, 1.4, 0.45, 0.8, 4.0, 11.0, flat(TIMBER_DARK));
    ic.on_wall(Side::Left, 1.4, 0.48, 0.77, 5.0, 10.0, flat(0x3a3428));
    for i in 0..3 {
        let (x, y) = ic.project(v3(0.53 + i as f32 * 0.09, 1.41, 6.0));
        ic.canvas().set(x as i32, y as i32, rgb(GOLD_LIGHT));
    }
    ic.window(Side::Right, 1.5, 0.85, 8.0, 0.14, 6.0);
    props::wall_sign(ic, Side::Left, 1.4, 1.3, 16.0, rgb(GOLD));
    ic.block(v3(1.6, 1.5, 0.0), v3(1.8, 1.65, 6.0), boards(TIMBER_DARK), Mat::Flat(rgb(GOLD)));
}

fn artisan(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    house(ic, [0.3, 0.3, 1.35, 1.15], 17.0, timber(PLASTER), tiles(TILE_RED), 17.0, Axis::A);
    ic.door(Side::Left, 1.15, 0.65, 0.2, 11.0);
    props::awning(ic, 0.95, 1.15, 1.35, 1.4, 15.0, 11.0, Side::Left, rgb(0x2f6a8a), rgb(0xe8d070));
    // Statue on a plinth.
    ic.block(v3(1.45, 1.35, 0.0), v3(1.75, 1.65, 6.0), stone(STONE_WARM), Mat::Rough(rgb(0xd8d0c0)));
    let (x, y) = ic.project(v3(1.6, 1.5, 6.0));
    let (ex, ey) = ic.project(v3(0.6, 1.6, 0.0));
    let c = ic.canvas();
    c.fill_rect(x as i32 - 1, y as i32 - 12, 3, 10, rgb(0xe8e4dc));
    c.fill_rect(x as i32 - 1, y as i32 - 12, 1, 10, rgb(0xf8f6f0));
    c.fill_ellipse(x + 0.5, y - 14.0, 2.0, 2.0, |_, _, _| rgb(0xeeeae2));
    c.linef(x + 2.0, y - 10.0, x + 4.0, y - 14.0, rgb(0xe0dcd4));
    // Easel with a painting.
    c.linef(ex - 3.0, ey, ex, ey - 12.0, rgb(TIMBER));
    c.linef(ex + 3.0, ey, ex, ey - 12.0, rgb(TIMBER));
    c.fill_rect(ex as i32 - 4, ey as i32 - 11, 8, 6, rgb(0xc8a860));
    c.fill_rect(ex as i32 - 3, ey as i32 - 10, 6, 4, rgb(0x4a7aa0));
    c.fill_rect(ex as i32 - 3, ey as i32 - 8, 6, 2, rgb(0x5a8a3a));
    props::barrel(ic, 1.8, 0.8, 0.0);
}

// --- Level 2 of the workshops and gatherers (reached by upgrading) ----------------------------

/// Level 2 of the woodcutter: a log lodge on a stone base, a covered saw horse and long cord
/// wood stacks.
fn lumber_camp(ic: &mut IsoCanvas) {
    yard(ic);
    props::tree(ic, 0.2, 1.75, Tree::Pine, 0.4, 72);
    ic.block(v3(0.28, 0.28, 0.0), v3(1.22, 1.15, 5.0), stone(STONE_WARM), Mat::Rough(rgb(STONE_WARM)));
    let r_roof = house(ic, [0.3, 0.3, 1.2, 1.13], 18.0, super::logs(), shingles(), 15.0, Axis::B);
    roof_chimney(ic, &r_roof, 0.45, 0.75, 5.0, stone(STONE_DARK));
    ic.door(Side::Left, 1.13, 0.75, 0.2, 11.0);
    ic.window(Side::Right, 1.2, 0.6, 7.0, 0.14, 5.0);
    ic.window(Side::Left, 1.13, 1.0, 7.0, 0.12, 4.0);
    // Saw horse under a small roof with a log on the trestle.
    shed(ic, [1.3, 1.25, 1.9, 1.85], 13.0, shingles(), 7.0, Axis::A, |ic| {
        props::log_pile(ic, 1.4, 1.48, 1.8, 1.62, 6.0, Axis::A);
    });
    props::log_pile(ic, 1.32, 0.25, 1.92, 0.7, 12.0, Axis::A);
    props::log_pile(ic, 1.32, 0.75, 1.9, 1.1, 8.0, Axis::A);
    for &(a, b) in &[(0.55, 1.5), (0.85, 1.75)] {
        ic.cylinder(a, b, 0.08, 0.0, 4.0, Mat::Logs(rgb(BARK)));
        ic.disc(a, b, 0.08, 4.0, Mat::EndGrain(rgb(END_GRAIN)));
    }
    props::flag(ic, 1.9, 1.9, 0.0, 18.0, rgb(BANNER_BLUE));
}

/// Level 2 of the quarry: a deeper, terraced face, a treadwheel crane and a mason's lodge.
fn large_quarry(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Earth(rgb(0x9a9080)));
    let rock = Mat::Rock(rgb(0x8c8780));
    let top = Mat::Rock(rgb(0xa09a90));
    ic.shadow(0.2, 0.2, 1.4, 2.8, 36.0);
    ic.block(v3(0.12, 0.12, 0.0), v3(0.75, 2.88, 36.0), rock, Mat::Grass(rgb(GRASS)));
    ic.block(v3(0.75, 0.12, 0.0), v3(1.2, 2.6, 24.0), rock, top);
    ic.block(v3(1.2, 0.12, 0.0), v3(1.6, 2.1, 12.0), rock, top);
    ic.block(v3(1.6, 0.12, 0.0), v3(1.9, 1.4, 5.0), rock, top);
    // Treadwheel crane: wheel, jib and a block on the rope.
    let (wa, wb) = (2.25, 0.55);
    ic.block(v3(wa - 0.25, wb - 0.08, 0.0), v3(wa + 0.25, wb + 0.08, 4.0), boards(TIMBER), boards(PLANK));
    let (x, y) = ic.project(v3(wa, wb + 0.1, 16.0));
    let c = ic.canvas();
    c.ring(x, y, 12.0, 10.5, rgb(TIMBER_DARK));
    for k in 0..6 {
        let ang = k as f32 * PI / 3.0;
        c.linef(x, y, x + ang.cos() * 11.0, y + ang.sin() * 11.0, rgb(TIMBER));
    }
    ic.line3(v3(wa, wb, 0.0), v3(wa, wb, 52.0), rgb(TIMBER));
    ic.line3(v3(wa + 0.05, wb, 0.0), v3(wa + 0.05, wb, 52.0), rgb(TIMBER_DARK));
    ic.line3(v3(wa, wb, 50.0), v3(1.45, 1.1, 38.0), rgb(TIMBER));
    ic.line3(v3(1.5, 1.08, 38.0), v3(1.5, 1.08, 18.0), rgba(0xd8cca8, 230));
    ic.block(v3(1.43, 1.01, 12.0), v3(1.57, 1.15, 18.0), stone(STONE_WARM), Mat::Rough(rgb(STONE_WARM)));
    // Mason's lodge and dressed blocks.
    let r_roof = house(ic, [2.1, 1.85, 2.85, 2.7], 13.0, timber(DAUB), tiles(TILE_RED), 12.0, Axis::B);
    let _ = r_roof;
    ic.door(Side::Left, 2.7, 2.45, 0.18, 9.0);
    props::stone_blocks(ic, 1.75, 2.25);
    props::stone_blocks(ic, 2.25, 1.35);
    props::stone_blocks(ic, 1.55, 2.6);
    props::stone_blocks(ic, 2.55, 1.0);
}

/// Level 2 of the smelter: a tall stone blast furnace with bellows house and ore bins; the
/// furnace mouth flickers while it works.
fn blast_furnace(ic: &mut IsoCanvas, frame: u32) {
    yard(ic);
    house(ic, [0.2, 0.25, 1.0, 1.3], 18.0, stone(STONE_DARK), slate(SLATE), 14.0, Axis::A);
    ic.door(Side::Left, 1.3, 0.6, 0.2, 12.0);
    ic.window(Side::Right, 1.0, 0.75, 9.0, 0.12, 5.0);
    // Square stone furnace stack with an iron band and a hot mouth.
    ic.shadow(1.1, 0.35, 1.85, 1.3, 70.0);
    ic.snow_free(|ic| {
        ic.block(v3(1.15, 0.4, 0.0), v3(1.8, 1.25, 26.0), stone(STONE), Mat::Rough(rgb(STONE_WARM)));
        ic.frustum(1.48, 0.82, 0.3, 0.2, 26.0, 52.0, stone(STONE));
        ic.tube(1.48, 0.82, 0.22, 0.22, 46.0, 49.0, Mat::Flat(rgb(0x4a4c52)));
        ic.disc(1.48, 0.82, 0.16, 52.0, Mat::Flat(rgb(0x1a1210)));
    });
    ic.mark_smoke(v3(1.48, 0.82, 52.0));
    ic.on_wall(Side::Left, 1.25, 1.32, 1.64, 1.0, 11.0, Mat::Flat(rgb(fire(frame))));
    let core = if frame == 0 { 0xffd060 } else { [0xffd060, 0xffe48a, 0xffd870, 0xf0b040][(frame % 4) as usize] };
    ic.on_wall(Side::Left, 1.255, 1.38, 1.58, 3.0, 8.0, Mat::Flat(rgb(core)));
    sparks(ic, 1.48, 1.3, 10.0, frame);
    // Ore and charcoal bins, finished bars.
    props::heap(ic, 0.4, 1.6, 0.2, 9.0, Mat::Stone(rgb(0x8a4a2a)));
    props::heap(ic, 0.85, 1.7, 0.18, 7.0, Mat::Rough(rgb(0x2a2622)));
    ic.block(v3(1.3, 1.5, 0.0), v3(1.88, 1.7, 4.0), Mat::Flat(rgb(0x707880)), Mat::Rough(rgb(0x9aa0a8)));
    ic.block(v3(1.35, 1.52, 4.0), v3(1.8, 1.68, 6.0), Mat::Flat(rgb(0x707880)), Mat::Rough(rgb(0xb0b8c0)));
}

/// Level 2 of the brickworks: a long ring kiln with two chimneys and covered drying racks.
fn large_brickworks(ic: &mut IsoCanvas) {
    yard(ic);
    shed(ic, [0.25, 0.25, 1.85, 1.0], 15.0, tiles(TILE_RED), 10.0, Axis::A, |ic| {
        for i in 0..4 {
            let a = 0.4 + i as f32 * 0.36;
            ic.block(v3(a, 0.4, 0.0), v3(a + 0.26, 0.85, 9.0), Mat::Brick(rgb(0xb8705a)), Mat::Brick(rgb(0xc8806a)));
        }
    });
    // Ring kiln: a long brick body with two stacks.
    ic.shadow(1.95, 0.25, 2.85, 1.35, 50.0);
    ic.snow_free(|ic| ic.block(v3(1.95, 0.3, 0.0), v3(2.85, 1.3, 18.0), Mat::Brick(rgb(BRICK)), Mat::Brick(rgb(0x8a4a38))));
    props::chimney(ic, 2.2, 0.45, 18.0, 56.0, Mat::Brick(rgb(BRICK)));
    props::chimney(ic, 2.6, 0.45, 18.0, 50.0, Mat::Brick(rgb(BRICK)));
    for a in [2.1, 2.4, 2.7] {
        ic.on_wall(Side::Left, 1.3, a - 0.06, a + 0.06, 0.0, 7.0, Mat::Flat(rgb(GLOW)));
    }
    props::heap(ic, 0.45, 1.45, 0.25, 10.0, Mat::Earth(rgb(0xa8603e)));
    props::heap(ic, 1.05, 1.6, 0.2, 8.0, Mat::Earth(rgb(SAND)));
    props::heap(ic, 1.45, 1.35, 0.16, 6.0, Mat::Rough(rgb(0x2a2622)));
    for (i, &(a, b)) in [(1.7, 1.55), (2.1, 1.6), (2.5, 1.55), (2.3, 1.85)].iter().enumerate() {
        let h = 6.0 + (i % 2) as f32 * 3.0;
        ic.block(v3(a, b, 0.0), v3(a + 0.3, b + 0.25, h), Mat::Brick(rgb(BRICK)), Mat::Brick(rgb(0xb0604a)));
    }
    props::flag(ic, 2.85, 1.95, 0.0, 20.0, rgb(BANNER_RED));
}

/// Level 2 of the pottery: a brick workshop with a second kiln and shelves of finished ware.
fn large_pottery(ic: &mut IsoCanvas) {
    yard(ic);
    let r_roof = house(ic, [0.25, 0.25, 1.2, 1.15], 17.0, timber(PLASTER), tiles(TILE_RED), 16.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.5, 0.45, 5.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.15, 0.65, 0.2, 11.0);
    ic.window(Side::Left, 1.15, 1.0, 7.0, 0.14, 5.0);
    ic.window(Side::Right, 1.2, 0.7, 7.0, 0.14, 4.0);
    for (b, h) in [(0.45, 11.0), (0.95, 9.0)] {
        ic.shadow(1.3, b - 0.25, 1.85, b + 0.25, h + 10.0);
        ic.snow_free(|ic| {
            ic.frustum(1.6, b, 0.26, 0.22, 0.0, h - 2.0, Mat::Brick(rgb(0x9a5a44)));
            ic.dome(1.6, b, 0.22, h - 2.0, 9.0, Mat::Brick(rgb(0x9a5a44)));
        });
        ic.mark_smoke(v3(1.6, b, h + 7.0));
    }
    ic.on_wall(Side::Left, 1.17, 1.52, 1.68, 1.0, 5.0, Mat::Flat(rgb(GLOW)));
    // Shelves with pots and jugs.
    ic.block(v3(0.3, 1.3, 0.0), v3(1.2, 1.5, 10.0), boards(TIMBER), boards(PLANK));
    for row in 0..2 {
        for i in 0..5 {
            let a = 0.38 + i as f32 * 0.17;
            let z = 3.0 + row as f32 * 5.0;
            ic.frustum(a, 1.52, 0.035, 0.045, z, z + 4.0, Mat::Rough(rgb([0xb8643c, 0xa85a38, 0xc87a4a][(i + row) % 3])));
        }
    }
    for i in 0..3 {
        ic.frustum(1.35 + i as f32 * 0.18, 1.7, 0.06, 0.045, 0.0, 8.0, Mat::Rough(rgb(0xa85a38)));
    }
}

/// Level 2 of the weaver: a two-storey cloth hall with two looms under an awning and bolts of
/// cloth on racks.
fn large_weaver(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    ic.block(v3(0.25, 0.25, 0.0), v3(1.4, 1.15, 12.0), stone(STONE_WARM), stone(STONE_WARM));
    let r_roof = house(ic, [0.25, 0.25, 1.4, 1.15], 26.0, timber(PLASTER), tiles(TILE_RED), 18.0, Axis::A);
    roof_chimney(ic, &r_roof, 1.1, 0.45, 5.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.15, 0.6, 0.2, 11.0);
    for a in [0.95, 1.25] {
        ic.window(Side::Left, 1.15, a, 5.0, 0.12, 5.0);
    }
    for a in [0.5, 0.85, 1.2] {
        ic.window(Side::Left, 1.15, a, 16.0, 0.12, 5.0);
    }
    props::wall_sign(ic, Side::Left, 1.15, 0.35, 16.0, rgb(0x6a4a8a));
    props::awning(ic, 1.45, 0.3, 1.92, 1.25, 16.0, 10.0, Side::Right, rgb(0x6a4a8a), rgb(0xe0d8c8));
    for b in [0.7, 1.15] {
        let (x, y) = ic.project(v3(1.62, b, 0.0));
        let c = ic.canvas();
        c.fill_rect(x as i32 - 5, y as i32 - 10, 1, 10, rgb(TIMBER));
        c.fill_rect(x as i32 + 4, y as i32 - 12, 1, 10, rgb(TIMBER));
        c.fill_rect(x as i32 - 5, y as i32 - 10, 10, 1, rgb(TIMBER));
        for k in 0..4 {
            c.fill_rect(x as i32 - 4 + k * 2, y as i32 - 9, 1, 6, rgb([0xc84040, 0x4060b0, 0xd8b030, 0x40904a][k as usize]));
        }
    }
    for i in 0..4 {
        let a = 0.35 + i as f32 * 0.25;
        ic.block(v3(a, 1.4, 0.0), v3(a + 0.2, 1.6, 6.0), Mat::Rough(rgb(0xeae2d0)), Mat::Rough(rgb(0xf4eee0)));
    }
    props::rack(ic, 1.1, 1.8, 1.88, 1.8, 12.0, &[rgb(0xb83a3a), rgb(0x3a5aa8), rgb(0x3a8a4a), rgb(0xd8b030)]);
}

/// Level 2 of the tailor: a corner house with a shop window, a second floor and racks of
/// garments.
fn large_tailor(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.25, 0.25, 1.4, 1.35];
    ic.block(v3(r[0], r[1], 0.0), v3(r[2], r[3], 12.0), stone(STONE_WARM), stone(STONE_WARM));
    let r_roof = house(ic, r, 26.0, timber(WHITEWASH), tiles(TILE_DARK), 18.0, Axis::B);
    roof_chimney(ic, &r_roof, 1.05, 0.45, 5.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.35, 0.85, 0.2, 11.0);
    ic.window(Side::Left, 1.35, 0.45, 4.0, 0.22, 6.0);
    ic.window(Side::Left, 1.35, 1.2, 4.0, 0.14, 6.0);
    for a in [0.45, 0.85, 1.2] {
        ic.window(Side::Left, 1.35, a, 16.0, 0.12, 5.0);
    }
    ic.window(Side::Right, 1.4, 0.8, 16.0, 0.14, 5.0);
    props::wall_sign(ic, Side::Left, 1.35, 1.0, 17.0, rgb(0x8a3a6a));
    props::awning(ic, 0.3, 1.4, 0.75, 1.62, 11.0, 8.0, Side::Left, rgb(0x8a3a6a), rgb(0xeee6d2));
    props::rack(ic, 1.55, 0.3, 1.55, 1.85, 13.0, &[rgb(0x8a3a3a), rgb(0x3a5a8a), rgb(0xd8c8a0), rgb(0x4a7a3a), rgb(0x6a2a4a)]);
    for a in [0.95, 1.25] {
        let (x, y) = ic.project(v3(a, 1.75, 0.0));
        let c = ic.canvas();
        c.fill_rect(x as i32, y as i32 - 4, 1, 4, rgb(TIMBER));
        c.lit_ellipse(x + 0.5, y - 8.0, 2.6, 4.0, rgb(if a < 1.0 { 0x6a2a4a } else { 0x2a4a6a }));
    }
}

/// Level 2 of the tannery: a larger workshop with a drying loft and a row of tanning vats.
fn large_tannery(ic: &mut IsoCanvas) {
    yard(ic);
    let r_roof = house(ic, [0.25, 0.25, 1.2, 1.1], 18.0, timber(DAUB), shingles(), 15.0, Axis::A);
    let _ = r_roof;
    ic.door(Side::Left, 1.1, 0.65, 0.2, 11.0);
    ic.window(Side::Left, 1.1, 1.0, 8.0, 0.12, 4.0);
    // Open drying loft with hides.
    shed(ic, [1.3, 0.25, 1.9, 1.15], 18.0, shingles(), 8.0, Axis::B, |ic| {
        props::rack(ic, 1.45, 0.35, 1.45, 1.05, 15.0, &[rgb(0x9a6a44), rgb(0x8a5a3a), rgb(0xa87a50)]);
        props::rack(ic, 1.75, 0.35, 1.75, 1.05, 12.0, &[rgb(0x8a5a3a), rgb(0x9a6a44)]);
    });
    for (i, &(a, b)) in [(0.4, 1.4), (0.75, 1.5), (1.1, 1.45), (0.55, 1.78), (0.95, 1.8), (1.35, 1.75)].iter().enumerate() {
        ic.frustum(a, b, 0.14, 0.14, 0.0, 6.0, boards(PLANK));
        ic.disc(a, b, 0.12, 6.0, Mat::Flat(rgb([0x5a3a20u32, 0x6a4a2a, 0x4a3018][i % 3])));
    }
    props::barrel(ic, 1.65, 1.6, 0.0);
    props::barrel(ic, 1.8, 1.75, 0.0);
}

/// Level 2 of the carpenter: a workshop hall with a wide gate, a planing bench in the yard and
/// finished furniture.
fn large_carpenter(ic: &mut IsoCanvas) {
    yard(ic);
    ic.block(v3(0.25, 0.25, 0.0), v3(1.45, 1.2, 8.0), stone(STONE_WARM), Mat::Rough(rgb(STONE_WARM)));
    let r_roof = house(ic, [0.25, 0.25, 1.45, 1.2], 22.0, timber(DAUB), tiles(TILE_RED), 18.0, Axis::A);
    roof_chimney(ic, &r_roof, 0.5, 0.45, 5.0, Mat::Brick(rgb(BRICK)));
    ic.on_wall(Side::Left, 1.2, 0.45, 1.0, 0.0, 14.0, Mat::Flat(rgb(0x2a2018)));
    ic.on_wall(Side::Left, 1.2, 0.48, 0.72, 0.0, 13.0, Mat::Boards(rgb(DOOR)));
    ic.window(Side::Left, 1.2, 1.25, 8.0, 0.12, 5.0);
    ic.window(Side::Right, 1.45, 0.75, 8.0, 0.14, 5.0);
    props::wall_sign(ic, Side::Left, 1.2, 1.25, 17.0, rgb(0xc89040));
    props::plank_stack(ic, 1.55, 0.25, 1.92, 0.75, 11.0);
    props::plank_stack(ic, 1.55, 0.8, 1.9, 1.15, 7.0);
    // Planing bench.
    ic.block(v3(0.5, 1.4, 0.0), v3(1.2, 1.55, 6.0), boards(TIMBER), boards(PLANK_LIGHT));
    // Finished table, chairs and a chest.
    ic.block(v3(1.45, 1.45, 4.0), v3(1.8, 1.72, 6.0), boards(PLANK), boards(PLANK_LIGHT));
    props::post(ic, 1.48, 1.48, 0.0, 4.0, 0.03);
    props::post(ic, 1.77, 1.69, 0.0, 4.0, 0.03);
    ic.block(v3(1.25, 1.75, 0.0), v3(1.38, 1.88, 4.0), boards(PLANK), boards(PLANK));
    ic.block(v3(1.25, 1.75, 4.0), v3(1.28, 1.88, 9.0), boards(PLANK), boards(PLANK));
    ic.block(v3(0.45, 1.7, 0.0), v3(0.8, 1.88, 6.0), boards(0x8a5a30), boards(0x9a6a3a));
}

/// Level 2 of the brewery: a stone brewhouse with a malt tower, two copper kettles and a cart
/// of barrels.
fn large_brewery(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.25, 0.25, 2.3, 1.2];
    let r_roof = house(ic, r, 22.0, stone(STONE_WARM), tiles(TILE_DARK), 18.0, Axis::A);
    roof_chimney(ic, &r_roof, 1.85, 0.45, 9.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.2, 1.2, 0.3, 14.0);
    for a in [0.6, 0.9, 1.65, 2.0] {
        ic.window(Side::Left, 1.2, a, 9.0, 0.12, 6.0);
    }
    props::wall_sign(ic, Side::Left, 1.2, 1.45, 15.0, rgb(0xd8a030));
    // Malt tower at the back.
    ic.shadow(0.3, 0.3, 0.75, 0.75, 60.0);
    ic.block(v3(0.3, 0.3, 22.0), v3(0.75, 0.75, 40.0), timber(DAUB), timber(DAUB));
    ic.hip(Roof::new([0.28, 0.28, 0.77, 0.77], 40.0, 14.0, Axis::A, tiles(TILE_DARK), timber(DAUB)));
    // Two copper kettles under a lean-to.
    shed(ic, [2.35, 0.3, 2.92, 1.15], 15.0, shingles(), 8.0, Axis::B, |ic| {
        for b in [0.55, 0.92] {
            ic.frustum(2.63, b, 0.17, 0.14, 0.0, 9.0, Mat::Rough(rgb(0xb8683a)));
            ic.dome(2.63, b, 0.14, 9.0, 4.0, Mat::Rough(rgb(0xc87a48)));
        }
    });
    for &(a, b, z) in &[(0.45, 1.45, 0.0), (0.63, 1.45, 0.0), (0.81, 1.45, 0.0), (0.54, 1.45, 9.0), (0.72, 1.45, 9.0), (0.63, 1.45, 18.0)] {
        props::barrel(ic, a, b, z);
    }
    // Barrel cart.
    ic.block(v3(1.5, 1.45, 3.0), v3(2.2, 1.8, 5.0), boards(TIMBER), boards(PLANK));
    for &(a, b) in &[(1.6, 1.82), (2.1, 1.82)] {
        let (x, y) = ic.project(v3(a, b, 3.0));
        ic.canvas().ring(x, y, 3.5, 2.5, rgb(TIMBER_DARK));
    }
    props::barrel(ic, 1.7, 1.62, 5.0);
    props::barrel(ic, 1.95, 1.62, 5.0);
    props::sack(ic, 2.45, 1.6, 0.0, rgb(0xd8c890));
}
