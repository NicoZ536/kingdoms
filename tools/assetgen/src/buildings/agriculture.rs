//! Agricultural buildings: barns, stables, coops and their yards with crops and animals.

use crate::draw::{rgb, shade};
use crate::iso::{Axis, IsoCanvas, Side, v3};
use crate::mat::Mat;
use crate::nature::Tree;
use crate::palette::*;
use crate::props;

use super::{boards, house, lean_to, roof_chimney, shed, shingles, stone, thatch, tiles, timber};

/// Draws agricultural building `id`; returns `false` if `id` is not agricultural.
pub fn draw(id: &str, ic: &mut IsoCanvas) -> bool {
    match id {
        "wheat_farm" => wheat_farm(ic),
        "vegetable_farm" => vegetable_farm(ic),
        "orchard" => orchard(ic, false),
        "cattle_farm" => cattle_farm(ic),
        "chicken_farm" => chicken_farm(ic),
        "sheep_farm" => sheep_farm(ic),
        "fishery" => fishery(ic),
        "large_fishery" => large_fishery(ic),
        "large_apiary" => large_apiary(ic),
        "apiary" => apiary(ic),
        _ => return false,
    }
    true
}

const BARN_RED: u32 = 0x7e3e2c;

fn farm_plate(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
}

/// Patch of ripe grain rows (ground decal plus stalks).
fn grain_patch(ic: &mut IsoCanvas, a0: f32, b0: f32, a1: f32, b1: f32, col: u32) {
    ic.ground_rect(a0, b0, a1, b1, 0.0, Mat::Earth(rgb(0x8a6a40)));
    let rows = ((b1 - b0) / 0.1) as i32;
    let cols = ((a1 - a0) / 0.06) as i32;
    for r in 0..rows {
        for k in 0..cols {
            let a = a0 + (k as f32 + 0.5) * (a1 - a0) / cols as f32;
            let b = b0 + (r as f32 + 0.5) * (b1 - b0) / rows as f32;
            let (x, y) = ic.project(v3(a, b, 0.0));
            let c = ic.canvas();
            c.fill_rect(x as i32, y as i32 - 5, 1, 5, shade(rgb(col), 0.85));
            c.set(x as i32, y as i32 - 6, shade(rgb(col), 1.12));
        }
    }
}

fn wheat_farm(ic: &mut IsoCanvas) {
    farm_plate(ic);
    ic.ground_rect(1.9, 1.5, 2.25, 2.96, 0.0, Mat::Earth(rgb(EARTH)));
    grain_patch(ic, 0.25, 1.95, 1.75, 2.8, 0xd8b450);
    // Big barn.
    let r = [0.3, 0.3, 2.1, 1.5];
    house(ic, r, 20.0, boards(BARN_RED), thatch(THATCH), 26.0, Axis::A);
    ic.on_wall(Side::Left, 1.5, 0.95, 1.45, 0.0, 15.0, Mat::Boards(rgb(0x5e3020)));
    ic.line3(v3(0.95, 1.51, 15.0), v3(1.45, 1.51, 0.0), rgb(0xc8b088));
    ic.line3(v3(0.95, 1.51, 0.0), v3(1.45, 1.51, 15.0), rgb(0xc8b088));
    ic.window(Side::Right, 2.1, 0.9, 26.0, 0.14, 5.0);
    // Granary on staddle stones.
    for &(a, b) in &[(2.3, 0.35), (2.7, 0.35), (2.3, 0.8), (2.7, 0.8)] {
        ic.cylinder(a, b, 0.05, 0.0, 4.0, Mat::Stone(rgb(STONE)));
    }
    ic.shadow(2.22, 0.28, 2.78, 0.88, 22.0);
    ic.block(v3(2.22, 0.28, 4.0), v3(2.78, 0.88, 14.0), boards(PLANK), boards(PLANK));
    ic.gable(crate::iso::Roof::new([2.22, 0.28, 2.78, 0.88], 14.0, 11.0, Axis::B, thatch(THATCH_OLD), boards(PLANK)));
    props::haystack(ic, 2.5, 1.45, 0.25, 22.0);
    props::sack(ic, 2.05, 1.75, 0.0, rgb(0xd8c890));
    props::sack(ic, 2.0, 2.3, 0.0, rgb(0xd0c088));
}

fn vegetable_farm(ic: &mut IsoCanvas) {
    farm_plate(ic);
    house(ic, [0.3, 0.3, 1.45, 1.3], 16.0, timber(DAUB), thatch(THATCH), 18.0, Axis::A);
    ic.door(Side::Left, 1.3, 0.85, 0.2, 11.0);
    ic.window(Side::Right, 1.45, 0.8, 7.0, 0.14, 4.0);
    house(ic, [1.6, 0.3, 2.2, 0.9], 11.0, boards(PLANK), shingles(), 9.0, Axis::B);
    props::bed(ic, 0.3, 1.6, 1.3, 2.7, rgb(0x5a9a3a), Axis::A);
    props::bed(ic, 1.5, 1.6, 2.7, 2.15, rgb(0x7aa84a), Axis::A);
    props::bed(ic, 1.5, 2.3, 2.7, 2.75, rgb(0x4a8a3a), Axis::A);
    props::bed(ic, 2.35, 0.3, 2.75, 1.4, rgb(0xd87a2a), Axis::B);
    // Scarecrow.
    let (x, y) = ic.project(v3(1.42, 2.2, 0.0));
    let c = ic.canvas();
    c.fill_rect(x as i32, y as i32 - 16, 1, 16, rgb(TIMBER));
    c.fill_rect(x as i32 - 5, y as i32 - 12, 11, 1, rgb(TIMBER));
    c.fill_rect(x as i32 - 2, y as i32 - 12, 5, 6, rgb(0x6a5a8a));
    c.lit_ellipse(x + 0.5, y - 15.0, 2.2, 2.0, rgb(STRAW));
    c.fill_rect(x as i32 - 3, y as i32 - 18, 7, 1, rgb(0x5a3a20));
    props::crate_box(ic, 2.3, 1.5, 0.2, 7.0, 0.0);
}

/// Buildings with a winter variant `<id>_winter` (same footprint and anchor), shown by the
/// game in winter.
pub const WINTER_VARIANTS: [&str; 1] = ["orchard"];

/// Draws the winter variant of building `id` (one of [`WINTER_VARIANTS`]); `false` otherwise.
pub fn draw_winter(id: &str, ic: &mut IsoCanvas) -> bool {
    match id {
        "orchard" => orchard(ic, true),
        _ => return false,
    }
    true
}

/// The orchard: the farmer's hut among fruit trees with baskets of apples; in `winter` the
/// trees are bare and the baskets empty.
fn orchard(ic: &mut IsoCanvas, winter: bool) {
    farm_plate(ic);
    house(ic, [0.25, 0.25, 1.0, 0.95], 13.0, boards(PLANK), shingles(), 13.0, Axis::A);
    ic.door(Side::Left, 0.95, 0.6, 0.2, 9.0);
    let trees = [(1.55, 0.5), (2.35, 0.55), (0.55, 1.45), (1.45, 1.4), (2.3, 1.45), (0.6, 2.35), (1.5, 2.3), (2.35, 2.35)];
    for (i, &(a, b)) in trees.iter().enumerate() {
        if winter {
            props::bare_fruit_tree(ic, a, b, 300 + i as u32);
        } else {
            props::fruit_tree(ic, a, b, 4, 300 + i as u32);
        }
    }
    // Baskets (full of apples outside winter).
    for &(a, b) in &[(1.95, 1.9), (2.1, 2.0)] {
        ic.frustum(a, b, 0.07, 0.09, 0.0, 5.0, Mat::Boards(rgb(0xb08a4a)));
        if !winter {
            let (x, y) = ic.project(v3(a, b, 5.0));
            ic.canvas().lit_ellipse(x, y, 3.0, 1.6, rgb(0xc8281e));
        }
    }
    let (x, y) = ic.project(v3(1.5, 1.55, 0.0));
    let c = ic.canvas();
    c.linef(x + 4.0, y, x + 1.0, y - 16.0, rgb(PLANK));
    c.linef(x + 7.0, y, x + 4.0, y - 16.0, rgb(PLANK));
    for k in 1..5 {
        let yy = y - k as f32 * 3.5;
        let off = k as f32 * 0.7;
        c.linef(x + 4.0 - off, yy, x + 7.0 - off, yy, rgb(PLANK));
    }
}

fn cattle_farm(ic: &mut IsoCanvas) {
    farm_plate(ic);
    let r = [0.3, 0.3, 2.0, 1.35];
    house(ic, r, 18.0, boards(0x6a4a30), thatch(THATCH_OLD), 22.0, Axis::A);
    for a in [0.6, 1.1, 1.6] {
        ic.on_wall(Side::Left, 1.35, a - 0.14, a + 0.14, 0.0, 12.0, Mat::Boards(rgb(0x4a3020)));
        ic.on_wall(Side::Left, 1.35, a - 0.14, a + 0.14, 6.0, 7.0, Mat::Flat(rgb(0xc8b088)));
    }
    props::haystack(ic, 2.45, 0.6, 0.25, 20.0);
    // Paddock with cows and a trough.
    ic.ground_rect(0.3, 1.6, 2.75, 2.75, 0.0, Mat::Grass(rgb(0x7aa648)));
    props::fence(ic, 0.3, 1.6, 2.75, 1.6, 7.0);
    props::fence(ic, 2.75, 1.6, 2.75, 2.75, 7.0);
    ic.block(v3(0.6, 1.75, 0.0), v3(1.2, 1.88, 4.0), boards(PLANK), Mat::Water(rgb(WATER)));
    props::animal(ic, 1.6, 2.0, props::Animal::Cow, false);
    props::animal(ic, 0.9, 2.4, props::Animal::Cow, true);
    props::animal(ic, 2.2, 2.45, props::Animal::Cow, false);
    props::fence(ic, 0.3, 2.75, 2.75, 2.75, 7.0);
    props::fence(ic, 0.3, 1.6, 0.3, 2.75, 7.0);
}

fn chicken_farm(ic: &mut IsoCanvas) {
    farm_plate(ic);
    ic.ground_rect(0.2, 0.2, 1.85, 1.85, 0.0, Mat::Earth(rgb(0x9a8058)));
    // Coop on stilts with a ramp.
    for &(a, b) in &[(0.4, 0.4), (1.1, 0.4), (0.4, 1.0), (1.1, 1.0)] {
        props::post(ic, a, b, 0.0, 6.0, 0.04);
    }
    ic.block(v3(0.35, 0.35, 6.0), v3(1.15, 1.05, 16.0), boards(PLANK), boards(PLANK));
    ic.gable(crate::iso::Roof::new([0.35, 0.35, 1.15, 1.05], 16.0, 11.0, Axis::A, shingles(), boards(PLANK)));
    ic.on_wall(Side::Left, 1.05, 0.65, 0.8, 6.0, 11.0, Mat::Flat(rgb(0x201810)));
    ic.face(&[v3(0.66, 1.05, 6.0), v3(0.8, 1.05, 6.0), v3(0.8, 1.45, 0.0), v3(0.66, 1.45, 0.0)], v3(0.73, 1.2, -5.0), Mat::BoardsAcross(rgb(PLANK_LIGHT)));
    for (i, &(a, b)) in [(1.4, 0.6), (1.2, 1.5), (1.6, 1.25), (0.5, 1.6)].iter().enumerate() {
        props::animal(ic, a, b, props::Animal::Chicken, i % 2 == 0);
    }
    props::fence(ic, 0.2, 1.88, 1.88, 1.88, 6.0);
    props::fence(ic, 1.88, 0.2, 1.88, 1.88, 6.0);
}

fn sheep_farm(ic: &mut IsoCanvas) {
    farm_plate(ic);
    house(ic, [0.3, 0.3, 1.7, 1.2], 14.0, boards(PLANK), thatch(THATCH), 18.0, Axis::A);
    ic.on_wall(Side::Left, 1.2, 0.8, 1.2, 0.0, 10.0, Mat::Flat(rgb(0x2a2018)));
    lean_to(ic, 1.7, 0.4, 2.1, 1.1, 12.0, 7.0, Side::Right, shingles());
    for i in 0..3 {
        let a = 1.9 + i as f32 * 0.25;
        ic.block(v3(a, 1.35, 0.0), v3(a + 0.2, 1.55, 6.0), Mat::Rough(rgb(0xeee8d8)), Mat::Rough(rgb(0xf6f0e2)));
    }
    ic.ground_rect(0.3, 1.6, 2.75, 2.75, 0.0, Mat::Grass(rgb(0x82ac4c)));
    props::fence(ic, 0.3, 1.6, 1.7, 1.6, 6.0);
    for (i, &(a, b)) in [(0.8, 1.95), (1.5, 2.1), (2.2, 2.0), (1.0, 2.5), (1.9, 2.55)].iter().enumerate() {
        props::animal(ic, a, b, props::Animal::Sheep, i % 2 == 1);
    }
    props::fence(ic, 0.3, 2.75, 2.75, 2.75, 6.0);
    props::fence(ic, 2.75, 1.6, 2.75, 2.75, 6.0);
}

fn fishery(ic: &mut IsoCanvas) {
    farm_plate(ic);
    // Water inlet along the right edge with a jetty and a boat.
    ic.ground_rect(1.35, 0.08, 1.96, 1.96, 0.0, Mat::Water(rgb(WATER)));
    ic.ground_rect(1.3, 0.08, 1.38, 1.96, 0.0, Mat::Earth(rgb(SAND)));
    house(ic, [0.25, 0.3, 1.1, 1.1], 13.0, boards(PLANK), thatch(THATCH_OLD), 14.0, Axis::B);
    ic.door(Side::Left, 1.1, 0.7, 0.2, 9.0);
    props::rack(ic, 0.3, 1.45, 1.05, 1.45, 11.0, &[rgb(0xb8c0c8), rgb(0xa8b4bc), rgb(0xc0c8d0), rgb(0xb0bac2)]);
    for &(a, b) in &[(1.45, 1.0), (1.85, 1.0), (1.45, 1.3), (1.85, 1.3)] {
        props::post(ic, a, b, -1.0, 3.0, 0.04);
    }
    ic.block(v3(1.3, 1.0, 3.0), v3(1.9, 1.3, 4.5), Mat::BoardsAcross(rgb(PLANK)), Mat::Boards(rgb(PLANK)));
    // Boat.
    let hull = [v3(1.45, 0.4, 0.0), v3(1.85, 0.4, 0.0), v3(1.92, 0.55, 3.0), v3(1.85, 0.7, 0.0), v3(1.45, 0.7, 0.0), v3(1.4, 0.55, 3.0)];
    let pts: Vec<(f32, f32)> = hull.iter().map(|p| ic.project(*p)).collect();
    ic.canvas().poly(&pts, rgb(TIMBER));
    let (x, y) = ic.project(v3(1.65, 0.55, 3.0));
    ic.canvas().fill_rect(x as i32 - 6, y as i32 - 1, 12, 2, rgb(PLANK));
    // Net.
    let (nx, ny) = ic.project(v3(0.4, 1.75, 0.0));
    let c = ic.canvas();
    for k in 0..4 {
        c.linef(nx + k as f32 * 3.0, ny - 6.0, nx + k as f32 * 3.0 + 3.0, ny, rgb(0x6a6050));
        c.linef(nx + k as f32 * 3.0 + 3.0, ny - 6.0, nx + k as f32 * 3.0, ny, rgb(0x6a6050));
    }
}

fn apiary(ic: &mut IsoCanvas) {
    farm_plate(ic);
    house(ic, [0.25, 0.25, 0.95, 0.9], 12.0, timber(DAUB), shingles(), 12.0, Axis::A);
    ic.door(Side::Left, 0.9, 0.6, 0.2, 9.0);
    // Flower meadow patches.
    for i in 0..60 {
        let a = 0.2 + crate::draw::hash01(i, 1, 77) * 1.6;
        let b = 1.0 + crate::draw::hash01(i, 2, 77) * 0.9;
        let (x, y) = ic.project(v3(a, b, 0.0));
        let col = [0xe8d040u32, 0xd070a0, 0xf4f0e8, 0x9a70d0][i as usize % 4];
        ic.set_layer(crate::iso::Layer::Ground);
        ic.canvas().set(x as i32, y as i32, rgb(col));
        ic.set_layer(crate::iso::Layer::Objects);
    }
    for &(a, b) in &[(1.25, 0.35), (1.65, 0.35), (1.25, 0.75), (1.65, 0.75), (0.5, 1.35), (0.9, 1.35), (1.3, 1.35), (1.7, 1.35)] {
        props::skep(ic, a, b);
    }
    props::tree(ic, 0.45, 1.8, Tree::Birch, 0.35, 66);
    props::fence(ic, 1.0, 1.9, 1.9, 1.9, 5.0);
}

// --- Level 2 (reached by upgrading) --------------------------------------------------------

/// Level 2 of the fishery: a fisher's house with a smokehouse, a long jetty, two boats and
/// drying racks.
fn large_fishery(ic: &mut IsoCanvas) {
    farm_plate(ic);
    ic.ground_rect(1.3, 0.08, 1.96, 1.96, 0.0, Mat::Water(rgb(WATER)));
    ic.ground_rect(1.25, 0.08, 1.33, 1.96, 0.0, Mat::Earth(rgb(SAND)));
    let r_roof = house(ic, [0.2, 0.25, 1.05, 1.0], 16.0, boards(PLANK), tiles(TILE_RED), 14.0, Axis::B);
    roof_chimney(ic, &r_roof, 0.35, 0.45, 5.0, stone(STONE_DARK));
    ic.door(Side::Left, 1.0, 0.7, 0.2, 10.0);
    ic.window(Side::Right, 1.05, 0.6, 7.0, 0.12, 4.0);
    // Smokehouse.
    house(ic, [0.25, 1.15, 0.7, 1.55], 12.0, stone(STONE_WARM), shingles(), 9.0, Axis::A);
    ic.mark_smoke(v3(0.47, 1.35, 26.0));
    props::rack(ic, 0.8, 1.2, 0.8, 1.85, 11.0, &[rgb(0xb8c0c8), rgb(0xa8b4bc), rgb(0xc0c8d0)]);
    // Long jetty with two boats.
    for &(a, b) in &[(1.4, 0.9), (1.9, 0.9), (1.4, 1.25), (1.9, 1.25)] {
        props::post(ic, a, b, -1.0, 3.0, 0.04);
    }
    ic.block(v3(1.25, 0.9, 3.0), v3(1.95, 1.25, 4.5), Mat::BoardsAcross(rgb(PLANK)), Mat::Boards(rgb(PLANK)));
    for b0 in [0.25, 1.45] {
        let hull = [
            v3(1.45, b0, 0.0),
            v3(1.85, b0, 0.0),
            v3(1.92, b0 + 0.15, 3.0),
            v3(1.85, b0 + 0.3, 0.0),
            v3(1.45, b0 + 0.3, 0.0),
            v3(1.4, b0 + 0.15, 3.0),
        ];
        let pts: Vec<(f32, f32)> = hull.iter().map(|p| ic.project(*p)).collect();
        ic.canvas().poly(&pts, rgb(TIMBER));
        let (x, y) = ic.project(v3(1.65, b0 + 0.15, 3.0));
        ic.canvas().fill_rect(x as i32 - 6, y as i32 - 1, 12, 2, rgb(PLANK));
    }
    props::barrel(ic, 1.05, 1.75, 0.0);
    props::crate_box(ic, 0.35, 1.75, 0.18, 6.0, 0.0);
}

/// Level 2 of the apiary: a bee house with shelved hives, more skeps and a larger flower
/// meadow.
fn large_apiary(ic: &mut IsoCanvas) {
    farm_plate(ic);
    house(ic, [0.2, 0.2, 0.9, 0.85], 14.0, timber(DAUB), tiles(TILE_RED), 12.0, Axis::A);
    ic.door(Side::Left, 0.85, 0.55, 0.2, 10.0);
    ic.window(Side::Right, 0.9, 0.5, 7.0, 0.12, 4.0);
    for i in 0..90 {
        let a = 0.15 + crate::draw::hash01(i, 1, 78) * 1.7;
        let b = 1.0 + crate::draw::hash01(i, 2, 78) * 0.92;
        let (x, y) = ic.project(v3(a, b, 0.0));
        let col = [0xe8d040u32, 0xd070a0, 0xf4f0e8, 0x9a70d0][i as usize % 4];
        ic.set_layer(crate::iso::Layer::Ground);
        ic.canvas().set(x as i32, y as i32, rgb(col));
        ic.set_layer(crate::iso::Layer::Objects);
    }
    // Bee house: an open shed with shelved box hives.
    shed(ic, [1.05, 0.2, 1.9, 0.75], 13.0, shingles(), 6.0, Axis::A, |ic| {
        for i in 0..4 {
            let a = 1.15 + i as f32 * 0.18;
            ic.block(v3(a, 0.35, 0.0), v3(a + 0.14, 0.6, 5.0), boards(0xd8b060), boards(0xe8c070));
            ic.block(v3(a, 0.35, 5.0), v3(a + 0.14, 0.6, 10.0), boards(0xc8a050), boards(0xe8c070));
        }
    });
    for &(a, b) in &[(1.2, 0.95), (1.6, 0.95), (0.45, 1.3), (0.85, 1.35), (1.25, 1.35), (1.65, 1.35), (0.65, 1.7)] {
        props::skep(ic, a, b);
    }
    props::tree(ic, 1.75, 1.8, Tree::Birch, 0.32, 67);
    props::fence(ic, 0.15, 1.95, 1.4, 1.95, 5.0);
}
