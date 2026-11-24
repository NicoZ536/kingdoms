//! Residential buildings: roof material and wall quality rise with the tier
//! (thatch & logs → half-timbering → tiles → stone & slate → manor with tower).

use crate::draw::rgb;
use crate::iso::{Axis, IsoCanvas, Side, v3};
use crate::mat::Mat;
use crate::palette::*;
use crate::props;

use super::{boards, flat, house, lean_to, logs, roof_chimney, shingles, slate, stone, thatch, tiles, timber};

/// Draws residential building `id`; returns `false` if `id` is not residential.
pub fn draw(id: &str, ic: &mut IsoCanvas) -> bool {
    match id {
        "house_wood" => house_wood(ic),
        "farmhouse" => farmhouse(ic),
        "citizen_house" => citizen_house(ic),
        "townhouse" => townhouse(ic),
        "residence" => residence(ic),
        "manor" => manor(ic),
        _ => return false,
    }
    true
}

fn house_wood(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.8, 1.4, 1.05, 1.96, 0.0, Mat::Earth(rgb(EARTH)));
    let r = [0.45, 0.5, 1.6, 1.45];
    house(ic, r, 15.0, logs(), thatch(THATCH), 19.0, Axis::A);
    // Smoke hole in the thatch near the back end of the ridge (no chimney).
    ic.mark_smoke(v3(0.8, 0.975, 34.0));
    ic.door(Side::Left, 1.45, 0.92, 0.2, 11.0);
    ic.window(Side::Left, 1.45, 1.35, 6.0, 0.14, 4.0);
    ic.window(Side::Right, 1.6, 0.95, 6.0, 0.14, 4.0);
    props::log_pile(ic, 1.68, 0.55, 1.92, 1.05, 7.0, Axis::B);
    props::bush(ic, 0.35, 1.75, 0.7, 3);
}

fn farmhouse(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.7, 1.4, 0.95, 1.96, 0.0, Mat::Earth(rgb(EARTH)));
    props::bed(ic, 1.65, 1.45, 1.95, 1.95, rgb(0x5a9a3a), Axis::B);
    let r = [0.35, 0.4, 1.5, 1.4];
    house(ic, r, 17.0, timber(DAUB), thatch(THATCH_OLD), 21.0, Axis::A);
    // Smoke hole in the thatch (no chimney).
    ic.mark_smoke(v3(0.75, 0.9, 38.0));
    // Lean-to shed on the right.
    ic.block(v3(1.5, 0.55, 0.0), v3(1.82, 1.2, 10.0), boards(PLANK), boards(PLANK));
    lean_to(ic, 1.48, 0.5, 1.88, 1.26, 15.0, 9.0, Side::Right, shingles());
    ic.door(Side::Left, 1.4, 0.8, 0.2, 12.0);
    ic.window(Side::Left, 1.4, 1.22, 7.0, 0.14, 4.0);
    ic.window(Side::Left, 1.4, 0.5, 7.0, 0.12, 4.0);
    props::haystack(ic, 0.25, 1.75, 0.14, 12.0);
    props::fence(ic, 1.0, 1.92, 1.6, 1.92, 6.0);
}

fn citizen_house(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.85, 1.55, 1.15, 1.96, 0.0, Mat::Cobbles(rgb(COBBLE)));
    let r = [0.4, 0.4, 1.6, 1.55];
    let (h, rise) = (20.0, 20.0);
    let r_roof = house(ic, r, h, timber(WHITEWASH), tiles(TILE_RED), rise, Axis::B);
    roof_chimney(ic, &r_roof, 1.15, 0.55, 6.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.55, 1.0, 0.2, 12.0);
    ic.window(Side::Left, 1.55, 0.6, 7.0, 0.14, 5.0);
    ic.window(Side::Left, 1.55, 1.4, 7.0, 0.14, 5.0);
    ic.window(Side::Left, 1.55, 1.0, 24.0, 0.12, 5.0);
    ic.window(Side::Right, 1.6, 0.75, 7.0, 0.14, 5.0);
    ic.window(Side::Right, 1.6, 1.25, 7.0, 0.14, 5.0);
    props::bush(ic, 0.45, 1.8, 0.6, 7);
    props::bush(ic, 1.75, 1.6, 0.55, 8);
}

fn townhouse(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Cobbles(rgb(COBBLE)));
    let ground = stone(STONE_WARM);
    ic.shadow(0.35, 0.35, 1.65, 1.65, 60.0);
    ic.block(v3(0.35, 0.35, 0.0), v3(1.65, 1.6, 14.0), ground, ground);
    // Jettied upper floor.
    let upper = [0.3, 0.3, 1.7, 1.66];
    ic.block(v3(upper[0], upper[1], 14.0), v3(upper[2], upper[3], 30.0), timber(PLASTER), timber(PLASTER));
    let roof = crate::iso::Roof::new(upper, 30.0, 28.0, Axis::A, tiles(TILE_DARK), timber(PLASTER));
    ic.gable(roof);
    roof_chimney(ic, &roof, 0.6, 0.55, 5.0, Mat::Brick(rgb(BRICK)));
    ic.door(Side::Left, 1.6, 1.0, 0.2, 11.0);
    ic.window(Side::Left, 1.6, 0.6, 5.0, 0.16, 5.0);
    ic.window(Side::Left, 1.6, 1.4, 5.0, 0.16, 5.0);
    for a in [0.55, 1.0, 1.45] {
        ic.window(Side::Left, 1.66, a, 19.0, 0.14, 6.0);
    }
    for b in [0.7, 1.3] {
        ic.window(Side::Right, 1.65, b, 5.0, 0.14, 5.0);
        ic.window(Side::Right, 1.7, b, 19.0, 0.14, 6.0);
    }
    props::wall_sign(ic, Side::Left, 1.66, 0.4, 16.0, rgb(GOLD));
    props::barrel(ic, 1.8, 1.25, 0.0);
}

fn residence(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(1.1, 1.4, 1.45, 2.96, 0.0, Mat::Earth(rgb(0xa89878)));
    let main = [0.35, 0.4, 2.65, 1.4];
    let main_roof = house(ic, main, 22.0, stone(STONE), slate(SLATE), 22.0, Axis::A);
    roof_chimney(ic, &main_roof, 0.7, 0.55, 6.0, stone(STONE_DARK));
    for a in [0.6, 0.95, 1.7, 2.05] {
        ic.window(Side::Left, 1.4, a, 9.0, 0.14, 7.0);
    }
    ic.door(Side::Left, 1.4, 1.32, 0.24, 14.0);
    let wing = [1.85, 1.4, 2.65, 2.45];
    house(ic, wing, 20.0, stone(STONE), slate(SLATE), 18.0, Axis::B);
    ic.window(Side::Left, 2.45, 2.25, 8.0, 0.14, 7.0);
    ic.window(Side::Right, 2.65, 1.75, 8.0, 0.14, 7.0);
    ic.window(Side::Right, 2.65, 2.15, 8.0, 0.14, 7.0);
    // Hedge and garden tree.
    let hedge = Mat::Rough(rgb(GRASS_DARK));
    ic.block(v3(0.25, 2.7, 0.0), v3(1.05, 2.85, 6.0), hedge, hedge);
    props::tree(ic, 0.6, 2.0, crate::nature::Tree::Oak, 0.5, 33);
    props::bush(ic, 2.85, 2.8, 0.6, 9);
}

fn manor(ic: &mut IsoCanvas) {
    ic.plate(0.04, Mat::Grass(rgb(GRASS)));
    ic.ground_rect(0.3, 1.7, 3.7, 3.7, 0.0, Mat::Cobbles(rgb(COBBLE)));
    let hall = [0.45, 0.45, 3.3, 1.7];
    let hall_roof = house(ic, hall, 26.0, stone(STONE_WARM), slate(SLATE_BLUE), 26.0, Axis::A);
    roof_chimney(ic, &hall_roof, 0.9, 0.6, 6.0, stone(STONE_DARK));
    roof_chimney(ic, &hall_roof, 2.4, 0.6, 6.0, stone(STONE_DARK));
    for a in [1.95, 2.35, 2.75] {
        ic.window(Side::Left, 1.7, a, 6.0, 0.14, 8.0);
        ic.window(Side::Left, 1.7, a, 18.0, 0.12, 5.0);
    }
    let wing = [0.45, 1.7, 1.7, 3.4];
    house(ic, wing, 24.0, stone(STONE_WARM), slate(SLATE_BLUE), 22.0, Axis::B);
    for b in [2.1, 2.55, 3.0] {
        ic.window(Side::Right, 1.7, b, 6.0, 0.14, 8.0);
        ic.window(Side::Right, 1.7, b, 17.0, 0.12, 5.0);
    }
    ic.door(Side::Left, 3.4, 1.05, 0.28, 15.0);
    // Corner tower with conical roof and banner.
    let (ta, tb) = (3.25, 1.85);
    ic.shadow(ta - 0.4, tb - 0.4, ta + 0.4, tb + 0.4, 70.0);
    ic.cylinder(ta, tb, 0.4, 0.0, 46.0, stone(STONE_WARM));
    ic.cone(ta, tb, 0.48, 46.0, 82.0, slate(SLATE_BLUE));
    ic.window(Side::Left, tb + 0.4, ta - 0.05, 30.0, 0.1, 6.0);
    props::flag(ic, ta, tb, 82.0, 14.0, rgb(BANNER_BLUE));
    // Fountain in the courtyard.
    ic.frustum(2.6, 2.75, 0.32, 0.32, 0.0, 4.0, stone(STONE));
    ic.disc(2.6, 2.75, 0.26, 4.1, Mat::Water(rgb(WATER)));
    ic.cylinder(2.6, 2.75, 0.05, 4.0, 12.0, stone(STONE_WARM));
    ic.disc(2.6, 2.75, 0.1, 12.0, flat(STONE_WARM));
    props::bush(ic, 3.6, 2.4, 0.6, 21);
    props::bush(ic, 2.0, 3.7, 0.6, 22);
    props::tree(ic, 3.5, 3.4, crate::nature::Tree::Birch, 0.55, 44);
}
