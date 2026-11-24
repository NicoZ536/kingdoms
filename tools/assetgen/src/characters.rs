//! Walkers, animals and carts (`characters` atlas): 4 directions × 4 walk frames each.
//!
//! Directions are screen diagonals matching the tile axes: `se` = +x, `sw` = +y, `nw` = −x,
//! `ne` = −y. Figures facing `se`/`sw` show their front, `ne`/`nw` their back; `sw` and `nw`
//! are mirror images of `se` and `ne`.

use kingdoms_engine::rendering::texture::Sampling;

use crate::atlas::AtlasBuilder;
use crate::draw::{Canvas, Px, hash01, rgb, rgba, shade};
use crate::iso::{IsoCanvas, v3};
use crate::mat::Mat;
use crate::palette;

/// All roles of the `characters` atlas.
pub const ROLES: [&str; 16] = [
    "man", "woman", "child", "carrier", "builder", "farmer", "soldier", "archer", "knight", "merchant", "cart", "siege", "raider", "cow",
    "sheep", "chicken",
];

/// Walking directions in sprite-name order.
pub const DIRS: [Dir; 4] = [Dir::Ne, Dir::Se, Dir::Sw, Dir::Nw];

/// Walk frames per direction.
pub const FRAMES: u32 = 4;

/// Walk animation speed in frames per second.
pub const WALK_FPS: f32 = 8.0;

/// Roles with a work loop `<role>_<dir>_work_<0..3>` (played at their work spot): builders
/// hammer, farmers hoe, men and women (gatherers) swing an axe or pick.
pub const WORK_ROLES: [&str; 4] = ["builder", "farmer", "man", "woman"];
/// Roles with the gatherer loops `<role>_<dir>_<pickaxe|shovel|rod>_<0..3>` ([`Gather`]) next
/// to the axe of their work loop: the game picks the loop by the gatherer's workplace.
pub const GATHER_ROLES: [&str; 2] = ["man", "woman"];
/// Roles with an attack loop `<role>_<dir>_attack_<0..3>`: spear thrust, bow draw, sword
/// slash, axe blow, catapult throw.
pub const ATTACK_ROLES: [&str; 5] = ["soldier", "archer", "knight", "raider", "siege"];
/// Frames and speed of the work and attack loops.
pub const ACT_FRAMES: u32 = 4;
pub const ACT_FPS: f32 = 6.0;
/// Width of the work and attack frames of people: the 16 px figure centred, room for the tool
/// on both sides.
pub const ACT_WIDTH: u32 = 32;

/// What a figure does in a sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    /// Walk frame 0..4 (frame 1 is the standing pose).
    Walk(u32),
    /// Frame 0..4 of the work loop.
    Work(u32),
    /// Frame 0..4 of the attack loop.
    Attack(u32),
    /// Frame 0..4 of a gatherer loop.
    Gather(Gather, u32),
}

/// Gatherer loops of men and women besides the axe (woodcutters) of their work loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gather {
    /// Quarry and mines: a pickaxe struck into the rock in front, chips flying.
    Pickaxe,
    /// Clay pit, sand pit and forester: a spade pushed into the ground, levered and heaved.
    Shovel,
    /// Fishery: a rod cast out, then held with the float in the water, now and then a bite.
    Rod,
}

impl Gather {
    pub const ALL: [Gather; 3] = [Gather::Pickaxe, Gather::Shovel, Gather::Rod];

    /// Loop name in sprite and animation names.
    pub fn name(self) -> &'static str {
        match self {
            Gather::Pickaxe => "pickaxe",
            Gather::Shovel => "shovel",
            Gather::Rod => "rod",
        }
    }

    fn tool(self) -> Tool {
        match self {
            Gather::Pickaxe => Tool::Pickaxe,
            Gather::Shovel => Tool::Shovel,
            Gather::Rod => Tool::Rod,
        }
    }

    /// Order of the [`ACT_FRAMES`] drawn frames in the animation: the rod is cast once and then
    /// held, the float bobbing twice, so an angler does not cast one and a half times a second.
    pub fn sequence(self) -> &'static [u32] {
        match self {
            Gather::Pickaxe | Gather::Shovel => &[0, 1, 2, 3],
            Gather::Rod => &[0, 1, 2, 2, 3, 2, 2, 3],
        }
    }
}

/// Tool swung in a work or attack loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Hammer,
    Hoe,
    Axe,
    Spear,
    Bow,
    Sword,
    Pickaxe,
    Shovel,
    Rod,
}

/// Tool of `role` in its work or attack loop.
fn tool_of(role: &str) -> Option<Tool> {
    Some(match role {
        "builder" => Tool::Hammer,
        "farmer" => Tool::Hoe,
        "man" | "woman" | "raider" => Tool::Axe,
        "soldier" => Tool::Spear,
        "archer" => Tool::Bow,
        "knight" => Tool::Sword,
        _ => return None,
    })
}

/// Screen-diagonal walking direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Ne,
    Se,
    Sw,
    Nw,
}

impl Dir {
    pub fn name(self) -> &'static str {
        match self {
            Dir::Ne => "ne",
            Dir::Se => "se",
            Dir::Sw => "sw",
            Dir::Nw => "nw",
        }
    }

    /// Facing the viewer (front visible)?
    fn front(self) -> bool {
        matches!(self, Dir::Se | Dir::Sw)
    }

    /// Drawn mirrored (facing screen-left)?
    fn mirrored(self) -> bool {
        matches!(self, Dir::Sw | Dir::Nw)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hat {
    None,
    Scarf(u32),
    Straw,
    Cap(u32),
    Kettle,
    Hood(u32),
    GreatHelm,
    Chaperon(u32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    None,
    Sack,
    Hammer,
    Pitchfork,
    SpearShield,
    Bow,
    Sword,
    Purse,
    Basket,
    Axe,
}

#[derive(Clone, Copy)]
struct Look {
    tunic: u32,
    legs: u32,
    hair: u32,
    long: bool,
    child: bool,
    hat: Hat,
    item: Item,
    belt: u32,
    apron: Option<u32>,
    trim: Option<u32>,
    armour: bool,
}

const fn look(tunic: u32, legs: u32, hair: u32, hat: Hat, item: Item) -> Look {
    Look { tunic, legs, hair, long: false, child: false, hat, item, belt: 0x3a2818, apron: None, trim: None, armour: false }
}

fn human_look(role: &str) -> Look {
    match role {
        "woman" => Look { long: true, ..look(0x4f7a46, 0x3a2a1c, palette::HAIR_BROWN, Hat::Scarf(0xefe8d8), Item::Basket) },
        "child" => Look { child: true, ..look(0x4a6a9a, 0x5a4632, palette::HAIR_BLOND, Hat::None, Item::None) },
        "carrier" => look(0xa08a62, 0x5a4a36, palette::HAIR_BROWN, Hat::None, Item::Sack),
        "builder" => Look { apron: Some(0xc8b48a), ..look(0x7a4a30, 0x4a3a2a, palette::HAIR_BLACK, Hat::Cap(0x6a5030), Item::Hammer) },
        "farmer" => look(0x6a7a3a, 0x5a4632, palette::HAIR_BROWN, Hat::Straw, Item::Pitchfork),
        "soldier" => Look { armour: true, trim: Some(palette::BANNER_RED), ..look(0x8a8e94, 0x4a4a52, palette::HAIR_BROWN, Hat::Kettle, Item::SpearShield) },
        "archer" => look(0x3f6b3a, 0x5a4632, palette::HAIR_BROWN, Hat::Hood(0x35593a), Item::Bow),
        "knight" => Look { armour: true, trim: Some(palette::BANNER_BLUE), ..look(0xb9bec6, 0x9aa0a8, palette::HAIR_BROWN, Hat::GreatHelm, Item::Sword) },
        "merchant" => Look { long: true, trim: Some(palette::GOLD), ..look(0x7a2a3a, 0x3a2a1c, palette::HAIR_BLACK, Hat::Chaperon(0x3a2450), Item::Purse) },
        // Bandits: crimson hood, rust leather, a bone-white sash and an axe – unlike any citizen
        // and light enough to read on dark ground.
        "raider" => Look { trim: Some(0xe2d8bc), belt: 0x2a1c14, ..look(0x9a5228, 0x4e3c2c, palette::HAIR_BLACK, Hat::Hood(0xb02a22), Item::Axe) },
        _ => look(0x7a5a3a, 0x5a4a3a, palette::HAIR_BROWN, Hat::None, Item::None),
    }
}

/// Body layout (rows) for adults and children.
struct Rig {
    head_cy: f32,
    head_r: f32,
    torso_top: i32,
    hip: i32,
    feet: i32,
}

fn rig(child: bool) -> Rig {
    if child {
        Rig { head_cy: 11.5, head_r: 2.9, torso_top: 15, hip: 19, feet: 23 }
    } else {
        Rig { head_cy: 7.0, head_r: 3.2, torso_top: 11, hip: 17, feet: 23 }
    }
}

/// Draws a human facing screen-right (front view for `front`, back view otherwise): walking
/// in walk frame `frame`, or – `acting` – standing with the feet apart, without the front arm
/// and the held item (the work or attack loop draws them, [`draw_action`]).
fn draw_human(c: &mut Canvas, lk: &Look, front: bool, frame: u32, acting: bool) {
    let r = rig(lk.child);
    let bob = if frame % 2 == 1 && !acting { -1 } else { 0 };
    let skin = rgb(palette::SKIN);
    let tunic = rgb(lk.tunic);
    let legs = rgb(lk.legs);
    let shoe = rgb(0x2e2218);
    let cx = 8.0;

    // Back-mounted items first (drawn behind the body).
    if !front {
        // Nothing behind the body in back view: back items are drawn on top later.
    } else {
        match lk.item {
            Item::Sack => c.lit_ellipse(4.5, (r.torso_top + bob) as f32 + 1.5, 3.4, 3.8, rgb(0xcdb88a)),
            Item::SpearShield => {
                c.fill_rect(3, r.torso_top + bob, 3, 7, rgb(palette::BANNER_RED));
                c.fill_rect(4, r.torso_top + bob + 1, 1, 5, rgb(0xe8dcc0));
            }
            _ => {}
        }
    }

    // Legs (stride pattern by frame; a firm stance while working or fighting).
    let stride = match frame {
        _ if acting => 1.0,
        0 => 2.0,
        2 => -2.0,
        _ => 0.0,
    };
    if !lk.long {
        let hip_y = (r.hip + bob) as f32;
        let feet_y = r.feet as f32 + 0.5;
        let back_leg = (cx - 1.0 - stride * 0.5, cx - 1.0 - stride);
        let front_leg = (cx + 1.0 + stride * 0.5, cx + 1.0 + stride);
        c.thick_line(back_leg.0, hip_y, back_leg.1, feet_y - 1.0, 0.9, shade(legs, 0.8));
        c.thick_line(front_leg.0, hip_y, front_leg.1, feet_y - 1.0, 0.9, legs);
        c.fill_rect(back_leg.1.round() as i32 - 1, r.feet, 3, 1, shoe);
        c.fill_rect(front_leg.1.round() as i32 - 1, r.feet, 3, 1, shoe);
    } else {
        let s = stride as i32 / 2;
        c.fill_rect(6 - s, r.feet, 2, 1, shoe);
        c.fill_rect(9 + s, r.feet, 2, 1, shoe);
    }

    // Torso (long garments reach the ankles).
    let top = (r.torso_top + bob) as f32;
    let bottom = if lk.long { r.feet as f32 } else { (r.hip + bob) as f32 + 0.5 };
    let (half_top, half_bottom) = if lk.child { (2.4, 2.6) } else if lk.long { (3.0, 4.2) } else { (3.0, 3.0) };
    let torso = [(cx - half_top, top), (cx + half_top, top), (cx + half_bottom, bottom), (cx - half_bottom, bottom)];
    let armour = lk.armour;
    c.fill_polygon(&torso, |x, y| {
        let u = (x as f32 + 0.5 - (cx - half_bottom)) / (half_bottom * 2.0);
        let mut col = shade(tunic, if u < 0.35 { 1.12 } else if u > 0.72 { 0.78 } else { 0.95 });
        if armour && (x + y) % 2 == 0 {
            col = shade(col, 0.9);
        }
        col
    });
    if let Some(a) = lk.apron {
        c.fill_rect(cx as i32, r.torso_top + bob + 2, 3, (bottom - top) as i32 - 1, rgb(a));
    }
    if let Some(t) = lk.trim {
        // Tabard / trim stripe down the front.
        c.fill_rect(cx as i32 - 1, r.torso_top + bob + 1, 2, (bottom - top) as i32 - 1, rgb(t));
    }
    let belt_y = if lk.child { r.torso_top + bob + 3 } else { r.torso_top + bob + 4 };
    c.fill_rect((cx - half_top) as i32, belt_y, (half_top * 2.0) as i32, 1, rgb(lk.belt));

    // Arms swing opposite to the legs.
    let swing = if acting { 0.0 } else { -stride * 0.5 };
    let arm_len = if lk.child { 3.5 } else { 5.0 };
    let sh = top + 1.0;
    let arm_col = if armour { shade(tunic, 0.85) } else { shade(tunic, 0.9) };
    if !acting {
        c.thick_line(cx - half_top - 0.3, sh, cx - half_top - 0.3 - swing * 0.5, sh + arm_len, 0.8, shade(arm_col, 0.85));
        c.thick_line(cx + half_top + 0.3, sh, cx + half_top + 0.3 + swing * 0.5, sh + arm_len, 0.8, arm_col);
    }
    let hand = (cx + half_top + 0.3 + swing * 0.5, sh + arm_len);
    if !acting {
        c.set(hand.0 as i32, hand.1 as i32, if armour { rgb(0x8a8e94) } else { skin });
    }

    // Head.
    let hy = r.head_cy + bob as f32;
    c.fill_ellipse(cx, hy, r.head_r, r.head_r * 1.05, |x, _, _| shade(skin, if (x as f32) < cx - 1.0 { 1.06 } else { 0.97 }));
    let hair = rgb(lk.hair);
    if front {
        // Hair cap and back of the head, face to the right.
        c.fill_ellipse(cx - 0.6, hy - 1.4, r.head_r, r.head_r * 0.7, |_, y, _| if (y as f32) < hy - 0.8 { hair } else { [0, 0, 0, 0] });
        c.fill_rect((cx - r.head_r) as i32, (hy - 1.0) as i32, 2, 3, hair);
        c.set((cx + 1.6) as i32, hy as i32, rgb(0x2a1a10));
    } else {
        c.fill_ellipse(cx - 0.3, hy - 0.2, r.head_r, r.head_r * 1.05, |x, _, _| if (x as f32) < cx + 2.0 { hair } else { [0, 0, 0, 0] });
    }

    // Headwear.
    let ht = hy - r.head_r;
    match lk.hat {
        Hat::None => {}
        Hat::Scarf(col) => {
            c.fill_ellipse(cx - 0.4, hy - 0.8, r.head_r + 0.6, r.head_r + 0.3, |x, y, _| {
                if front && (x as f32) > cx - 0.5 && (y as f32) > hy - 1.5 { [0, 0, 0, 0] } else { shade(rgb(col), if (x as f32) < cx { 1.0 } else { 0.85 }) }
            });
        }
        Hat::Straw => {
            c.fill_rect((cx - 5.0) as i32, ht as i32 + 1, 11, 1, rgb(palette::STRAW));
            c.fill_rect((cx - 3.0) as i32, ht as i32 - 1, 6, 2, shade(rgb(palette::STRAW), 0.9));
        }
        Hat::Cap(col) => c.fill_rect((cx - 3.0) as i32, ht as i32, 6, 2, rgb(col)),
        Hat::Kettle => {
            c.fill_rect((cx - 4.0) as i32, ht as i32 + 2, 9, 1, rgb(0x6a6e74));
            c.fill_ellipse(cx, ht + 2.0, 3.2, 2.4, |x, _, _| shade(rgb(0x9aa0a8), if (x as f32) < cx { 1.15 } else { 0.85 }));
        }
        Hat::Hood(col) => {
            c.fill_ellipse(cx - 0.6, hy - 0.4, r.head_r + 0.8, r.head_r + 0.6, |x, y, _| {
                if front && (x as f32) > cx + 0.3 && (y as f32) > hy - 1.2 { [0, 0, 0, 0] } else { shade(rgb(col), if (x as f32) < cx { 1.05 } else { 0.85 }) }
            });
            c.poly(&[(cx - 3.5, hy + 1.0), (cx - 6.0, hy + 4.0), (cx - 2.5, hy + 3.0)], rgb(col));
        }
        Hat::GreatHelm => {
            c.fill_rect((cx - 3.5) as i32, ht as i32, 7, 7, rgb(0xa8aeb6));
            c.fill_rect((cx - 3.5) as i32, ht as i32, 2, 7, rgb(0xd0d4da));
            c.fill_rect((cx + 2.0) as i32, ht as i32, 2, 7, rgb(0x80868e));
            if front {
                c.fill_rect(cx as i32, ht as i32 + 3, 4, 1, rgb(0x1a1a1a));
            }
            c.fill_ellipse(cx - 1.0, ht - 1.0, 2.2, 1.6, |_, _, _| rgb(0xc0302a));
        }
        Hat::Chaperon(col) => {
            c.fill_ellipse(cx, ht + 0.8, 4.2, 2.2, |x, _, _| shade(rgb(col), if (x as f32) < cx { 1.1 } else { 0.85 }));
            c.poly(&[(cx - 4.0, ht + 1.0), (cx - 7.0, ht + 5.0), (cx - 4.5, ht + 4.0)], rgb(col));
        }
    }

    // Front / held items.
    if acting {
        return;
    }
    let (hx, hyy) = (hand.0, hand.1);
    match lk.item {
        Item::Sack if !front => c.lit_ellipse(cx, top + 2.5, 3.8, 4.0, rgb(0xcdb88a)),
        Item::Hammer => {
            c.linef(hx, hyy, hx + 1.0, hyy - 4.0, rgb(palette::PLANK));
            c.fill_rect(hx as i32, hyy as i32 - 5, 3, 2, rgb(palette::IRON));
        }
        Item::Pitchfork => {
            let px = hx + 1.0;
            c.linef(px, (r.feet - 1) as f32, px, 4.0 + bob as f32, rgb(palette::PLANK));
            for k in -1..=1 {
                c.linef(px + k as f32, 1.0 + bob as f32, px + k as f32, 3.0 + bob as f32, rgb(palette::STEEL));
            }
            c.linef(px - 1.0, 3.0 + bob as f32, px + 1.0, 3.0 + bob as f32, rgb(palette::STEEL));
        }
        Item::SpearShield => {
            let px = hx + 1.0;
            c.linef(px, (r.feet - 1) as f32, px, 2.0 + bob as f32, rgb(palette::PLANK));
            c.poly(&[(px - 1.0, 3.0 + bob as f32), (px + 2.0, 3.0 + bob as f32), (px + 0.5, bob as f32)], rgb(palette::STEEL));
            if !front {
                c.fill_rect(5, r.torso_top + bob, 6, 6, rgb(palette::BANNER_RED));
                c.fill_rect(7, r.torso_top + bob + 1, 2, 4, rgb(0xe8dcc0));
            }
        }
        Item::Bow => {
            let bx = hx + 1.5;
            for k in 0..9 {
                let t = k as f32 / 8.0;
                let y = top - 3.0 + t * 12.0;
                let x = bx + (t * std::f32::consts::PI).sin() * 1.6;
                c.set(x as i32, y as i32, rgb(0x6a4424));
            }
            c.linef(bx, top - 3.0, bx, top + 9.0, rgba(0xe8e0d0, 200));
            // Quiver on the back.
            let qx = if front { cx - 4.5 } else { cx - 1.0 };
            c.fill_rect(qx as i32, top as i32 - 1, 2, 6, rgb(0x6a4424));
            c.fill_rect(qx as i32, top as i32 - 3, 2, 2, rgb(0xe8e0d0));
        }
        Item::Sword => {
            c.linef(cx - half_top - 1.0, top + 5.0, cx - half_top - 1.0, top + 10.0, rgb(palette::STEEL));
            c.fill_rect((cx - half_top - 2.0) as i32, (top + 5.0) as i32, 3, 1, rgb(palette::GOLD));
        }
        Item::Purse => c.fill_ellipse(cx + 2.0, (belt_y + 2) as f32, 1.4, 1.6, |_, _, _| rgb(palette::GOLD)),
        Item::Basket => {
            c.fill_rect(hx as i32 - 1, hyy as i32 - 1, 4, 3, rgb(0xb08a4a));
            c.fill_rect(hx as i32 - 1, hyy as i32 - 1, 4, 1, rgb(0x5a8a3a));
        }
        Item::Axe => {
            // Raised axe: haft from the hand up, a broad steel blade at the top.
            c.linef(hx, hyy, hx + 2.0, hyy - 8.0, rgb(palette::TIMBER));
            c.poly(&[(hx + 1.0, hyy - 9.0), (hx + 5.0, hyy - 10.5), (hx + 5.0, hyy - 5.5), (hx + 2.0, hyy - 6.5)], rgb(palette::STEEL));
            c.set((hx + 4.0) as i32, (hyy - 9.0) as i32, rgb(0xe8ecf0));
        }
        _ => {}
    }
}

/// Arm angle (radians from straight down towards the front, π = straight up) and forward
/// shift of the hand in frame `f` of a tool's loop.
fn tool_motion(tool: Tool, f: u32) -> (f32, f32) {
    let f = (f % ACT_FRAMES) as usize;
    match tool {
        // Wind-up behind the head, overhead, strike, follow-through.
        Tool::Hammer => ([3.7, 3.0, 1.5, 0.9][f], 0.0),
        Tool::Axe => ([3.9, 3.1, 1.3, 0.6][f], 0.0),
        Tool::Sword => ([3.6, 2.8, 1.4, 0.7][f], 0.0),
        // Raise the hoe, chop it into the ground in front, pull it back.
        Tool::Hoe => ([3.3, 2.5, 0.75, 0.55][f], 0.0),
        // Draw back, thrust, hold, recover.
        Tool::Spear => (1.5, [-2.0, 2.5, 4.0, 1.0][f]),
        // The bow arm stays out front; the draw is drawn by the string hand.
        Tool::Bow => (1.45, 0.0),
        // Like the axe, but the point goes down into the rock in front of the feet.
        Tool::Pickaxe => ([3.8, 3.0, 1.15, 0.8][f], 0.0),
        // Ready, pushed into the ground, levered up with a load, heaved.
        Tool::Shovel => ([0.95, 0.55, 1.3, 2.3][f], [0.0, 1.0, -1.0, 0.0][f]),
        // Rod back over the shoulder, cast, held low while the float sits, a bite.
        Tool::Rod => ([2.6, 1.8, 0.9, 0.8][f], 0.0),
    }
}

/// Angle of the fishing rod itself (same convention as [`tool_motion`]) in frame `f`: the hands
/// stay low while the rod points up and out.
fn rod_angle(f: u32) -> f32 {
    [3.9, 2.4, 2.3, 2.0][(f % ACT_FRAMES) as usize]
}

/// The front arm and the tool of a work or attack loop on an act canvas ([`ACT_WIDTH`] wide,
/// figure centred), frame `f`; `front` for the views facing the viewer.
fn draw_action(c: &mut Canvas, lk: &Look, tool: Tool, f: u32, front: bool) {
    let r = rig(false);
    let ox = (ACT_WIDTH as f32 - 16.0) * 0.5;
    let cx = 8.0 + ox;
    let half_top = 3.0;
    let top = r.torso_top as f32;
    let sh = top + 1.0;
    let arm_len = 5.0;
    let tunic = rgb(lk.tunic);
    let arm_col = if lk.armour { shade(tunic, 0.85) } else { shade(tunic, 0.9) };
    let skin = if lk.armour { rgb(0x8a8e94) } else { rgb(palette::SKIN) };
    let back_shoulder = (cx - half_top - 0.3, sh);
    let shoulder = (cx + half_top + 0.3, sh);
    let (angle, shift) = tool_motion(tool, f);
    let dir = (angle.sin(), angle.cos());
    // Direction the tool head moves in a downward swing (the edge faces it).
    let edge = (-angle.cos(), angle.sin());
    let hand = (shoulder.0 + dir.0 * arm_len + shift, shoulder.1 + dir.1 * arm_len);
    let along = |l: f32| (hand.0 + dir.0 * l, hand.1 + dir.1 * l);
    match tool {
        Tool::Bow => {
            // Quiver on the back, bow held out front, string pulled back to the chest.
            c.fill_rect((cx - 4.5) as i32, top as i32 - 1, 2, 6, rgb(0x6a4424));
            c.fill_rect((cx - 4.5) as i32, top as i32 - 3, 2, 2, rgb(0xe8e0d0));
            let draw = [1.0, 3.0, 5.0, 0.5][(f % ACT_FRAMES) as usize];
            let bx = hand.0 + 1.0;
            let (bt, bb) = (hand.1 - 6.0, hand.1 + 6.0);
            for k in 0..13 {
                let t = k as f32 / 12.0;
                let y = bt + t * (bb - bt);
                let x = bx + (t * std::f32::consts::PI).sin() * (1.8 + draw * 0.15);
                c.set(x as i32, y as i32, rgb(0x6a4424));
            }
            let pull = (bx - draw, hand.1);
            c.linef(bx, bt, pull.0, pull.1, rgba(0xe8e0d0, 220));
            c.linef(pull.0, pull.1, bx, bb, rgba(0xe8e0d0, 220));
            // The string hand pulls with the back arm.
            c.thick_line(back_shoulder.0, back_shoulder.1, pull.0, pull.1, 0.8, shade(arm_col, 0.85));
            if f % ACT_FRAMES != 3 {
                c.linef(pull.0, pull.1, bx + 4.0, hand.1, rgb(0x8a6a44));
                c.set((bx + 4.0) as i32, hand.1 as i32, rgb(palette::STEEL));
            }
            c.thick_line(shoulder.0, shoulder.1, hand.0, hand.1, 0.8, arm_col);
            c.set(hand.0 as i32, hand.1 as i32, skin);
            return;
        }
        Tool::Rod => {
            draw_rod(c, f, front, hand, back_shoulder, shoulder, arm_col, skin);
            return;
        }
        Tool::Axe | Tool::Hoe | Tool::Pickaxe | Tool::Shovel => {
            // Two-handed: the back arm grips the haft too.
            c.thick_line(back_shoulder.0, back_shoulder.1, hand.0 - dir.0, hand.1 - dir.1, 0.8, shade(arm_col, 0.85));
        }
        _ => {}
    }
    c.thick_line(shoulder.0, shoulder.1, hand.0, hand.1, 0.8, arm_col);
    match tool {
        Tool::Hammer => {
            let (ex, ey) = along(4.0);
            c.linef(hand.0, hand.1, ex, ey, rgb(palette::PLANK));
            c.thick_line(ex - edge.0 * 1.5, ey - edge.1 * 1.5, ex + edge.0 * 1.5, ey + edge.1 * 1.5, 0.9, rgb(palette::IRON));
        }
        Tool::Axe => {
            let (ex, ey) = along(7.0);
            c.linef(hand.0 - dir.0, hand.1 - dir.1, ex, ey, rgb(palette::TIMBER));
            let (bx, by) = along(5.0);
            c.poly(&[(bx, by), (ex, ey), (ex + edge.0 * 3.5, ey + edge.1 * 3.5), (bx + edge.0 * 3.0, by + edge.1 * 3.0)], rgb(palette::STEEL));
        }
        Tool::Hoe => {
            let (ex, ey) = along(9.0);
            c.linef(hand.0 - dir.0, hand.1 - dir.1, ex, ey, rgb(palette::PLANK));
            c.thick_line(ex, ey, ex + edge.0 * 3.0, ey + edge.1 * 3.0, 0.7, rgb(palette::STEEL));
        }
        Tool::Sword => {
            let (ex, ey) = along(7.5);
            c.thick_line(hand.0, hand.1, ex, ey, 0.6, rgb(palette::STEEL));
            c.linef(hand.0 - edge.0 * 1.5, hand.1 - edge.1 * 1.5, hand.0 + edge.0 * 1.5, hand.1 + edge.1 * 1.5, rgb(palette::GOLD));
        }
        Tool::Spear => {
            let (bx, by) = along(-7.0);
            let (ex, ey) = along(7.0);
            c.linef(bx, by, ex, ey, rgb(palette::PLANK));
            let (tx, ty) = along(9.5);
            c.poly(&[(ex, ey - 1.2), (tx, ty), (ex, ey + 1.2)], rgb(palette::STEEL));
        }
        Tool::Pickaxe => {
            let (ex, ey) = along(7.0);
            c.linef(hand.0 - dir.0, hand.1 - dir.1, ex, ey, rgb(palette::TIMBER));
            // The head across the haft: a long point towards the strike, a short one behind.
            let point = (ex + edge.0 * 3.5, ey + edge.1 * 3.5);
            c.thick_line(ex - edge.0 * 2.0, ey - edge.1 * 2.0, point.0, point.1, 0.6, rgb(palette::IRON));
            c.set(point.0 as i32, point.1 as i32, rgb(palette::STEEL));
            if f % ACT_FRAMES == 2 {
                // Rock chips fly from the point.
                for (dx, dy) in [(1.5, -2.0), (3.0, -1.0), (2.0, -3.5)] {
                    c.set((point.0 + dx) as i32, (point.1 + dy) as i32, rgb(palette::STONE_WARM));
                }
            }
        }
        Tool::Shovel => {
            let (ex, ey) = along(5.0);
            c.linef(hand.0 - dir.0 * 2.0, hand.1 - dir.1 * 2.0, ex, ey, rgb(palette::PLANK));
            // The blade continues the haft: a flat iron plate, lit on its upper side.
            let n = (dir.1, -dir.0);
            let (tx, ty) = along(8.0);
            let blade = [(ex - n.0 * 1.3, ey - n.1 * 1.3), (ex + n.0 * 1.3, ey + n.1 * 1.3), (tx + n.0 * 1.1, ty + n.1 * 1.1), (tx - n.0 * 1.1, ty - n.1 * 1.1)];
            c.poly(&blade, rgb(palette::IRON));
            c.linef(ex + n.0 * 0.8, ey + n.1 * 0.8, tx + n.0 * 0.6, ty + n.1 * 0.6, rgb(palette::STEEL));
            match f % ACT_FRAMES {
                // A clod of earth on the lifted blade.
                2 => c.lit_ellipse(ex + dir.0 * 1.8 + n.0 * 1.2, ey + dir.1 * 1.8 + n.1 * 1.2 - 1.0, 1.8, 1.2, rgb(palette::EARTH)),
                // The heave throws it on.
                3 => {
                    for (dx, dy) in [(1.0, -1.5), (2.5, -0.5), (2.0, -3.0), (3.5, -2.0)] {
                        c.set((tx + dx) as i32, (ty + dy) as i32, rgb(palette::EARTH_DARK));
                    }
                }
                _ => {}
            }
        }
        Tool::Bow | Tool::Rod => {}
    }
    c.set(hand.0 as i32, hand.1 as i32, skin);
}

/// Both arms, the fishing rod and its line in frame `f` of the rod loop: back over the
/// shoulder, cast (the line streams out), held low with the float in the water ahead, a bite
/// (rod tip and float pulled down). Seen from behind (`front` false) the angler looks into the
/// picture, so the float lies higher up – further away – than in the front views.
#[allow(clippy::too_many_arguments)]
fn draw_rod(c: &mut Canvas, f: u32, front: bool, hand: (f32, f32), back_shoulder: (f32, f32), shoulder: (f32, f32), arm_col: Px, skin: Px) {
    let f = f % ACT_FRAMES;
    let a = rod_angle(f);
    let rdir = (a.sin(), a.cos());
    let butt = (hand.0 - rdir.0 * 2.0, hand.1 - rdir.1 * 2.0);
    let tip = (hand.0 + rdir.0 * 8.0, hand.1 + rdir.1 * 8.0);
    c.thick_line(back_shoulder.0, back_shoulder.1, butt.0, butt.1, 0.8, shade(arm_col, 0.85));
    // A faint line (no outline: below the outline's opacity threshold).
    let line = rgba(0xe8e0d0, 120);
    let float = (ACT_WIDTH as f32 - 2.5, if front { 21.0 } else { 17.0 });
    match f {
        0 => c.linef(tip.0, tip.1, tip.0 + 0.5, tip.1 + 3.0, line),
        1 => {
            c.linef(tip.0, tip.1, tip.0 + 1.0, tip.1 + 2.0, line);
            c.linef(tip.0 + 1.0, tip.1 + 2.0, tip.0 + 1.0, tip.1 + 5.0, line);
        }
        2 => {
            c.linef(tip.0, tip.1, float.0, float.1, line);
            c.set(float.0 as i32, float.1 as i32 - 1, rgb(0xf0ece0));
            c.set(float.0 as i32, float.1 as i32, rgb(0xc0302a));
        }
        _ => {
            c.linef(tip.0, tip.1, float.0, float.1 + 1.0, line);
            c.set(float.0 as i32, float.1 as i32 + 1, rgb(0xc0302a));
            for dx in [-1, 1] {
                c.set(float.0 as i32 + dx, float.1 as i32 + 1, rgba(0xcfe6f2, 150));
            }
        }
    }
    c.linef(butt.0, butt.1, tip.0, tip.1, rgb(0x8a6a3a));
    c.set(tip.0 as i32, tip.1 as i32, rgb(palette::TIMBER_DARK));
    c.thick_line(shoulder.0, shoulder.1, hand.0, hand.1, 0.8, arm_col);
    c.set(hand.0 as i32, hand.1 as i32, skin);
}

/// Generic quadruped seen from the side, facing screen-right.
#[allow(clippy::too_many_arguments)]
fn draw_quadruped(c: &mut Canvas, body: Px, patch: Option<Px>, head: Px, front: bool, frame: u32, cx: f32, cy: f32, rx: f32, ry: f32, seed: u32) {
    let leg_col = shade(head, 0.8);
    let swing = match frame {
        0 => 1.0,
        2 => -1.0,
        _ => 0.0,
    };
    let bob = if frame % 2 == 1 { -1.0 } else { 0.0 };
    let ground = c.height() as f32 - 1.0;
    let legs = [(cx - rx * 0.6, swing, 0.8), (cx - rx * 0.35, -swing, 1.0), (cx + rx * 0.45, -swing, 0.8), (cx + rx * 0.7, swing, 1.0)];
    for &(lx, sw, sh) in &legs {
        c.thick_line(lx, cy + bob, lx + sw, ground - 0.5, 0.75, shade(leg_col, sh));
    }
    // Tail.
    c.thick_line(cx - rx, cy - ry * 0.4 + bob, cx - rx - 1.0, cy + ry * 0.6 + bob, 0.6, shade(body, 0.7));
    c.lit_ellipse(cx, cy + bob, rx, ry, body);
    if let Some(p) = patch {
        for i in 0..3 {
            let px = cx + (hash01(i, 1, seed) - 0.5) * rx * 1.3;
            let py = cy + bob + (hash01(i, 2, seed) - 0.6) * ry;
            c.fill_ellipse(px, py, rx * 0.28, ry * 0.45, |_, _, _| p);
        }
    }
    // Head and neck.
    let (hx, hy) = if front { (cx + rx + 1.5, cy + 1.0 + bob) } else { (cx + rx + 0.5, cy - ry * 0.6 + bob) };
    c.thick_line(cx + rx * 0.7, cy - ry * 0.2 + bob, hx, hy, 1.4, shade(body, 0.95));
    c.lit_ellipse(hx + 0.5, hy, 2.6, 2.0, head);
    if front {
        c.set((hx + 1.5) as i32, (hy - 1.0) as i32, rgb(0x101010));
    }
    c.fill_rect((hx - 1.5) as i32, (hy - 2.5) as i32, 1, 1, shade(head, 0.7));
}

fn draw_cow(c: &mut Canvas, front: bool, frame: u32) {
    draw_quadruped(c, rgb(0xf0ece4), Some(rgb(0x3a2a20)), rgb(0xe8e0d6), front, frame, 11.0, 9.0, 7.5, 4.2, 11);
    if front {
        // Pink muzzle and small horns.
        c.fill_rect(21, 11, 2, 1, rgb(0xe8a8a0));
    }
    c.set(19, 5, rgb(0xe8e0c8));
    c.fill_rect(9, 13, 3, 1, rgb(0xe8a8a0));
}

fn draw_sheep(c: &mut Canvas, front: bool, frame: u32) {
    let wool = rgb(0xece6d4);
    let swing = match frame {
        0 => 1.0,
        2 => -1.0,
        _ => 0.0,
    };
    let bob = if frame % 2 == 1 { -1.0 } else { 0.0 };
    let ground = c.height() as f32 - 1.0;
    for &(lx, sw) in &[(5.0, swing), (7.0, -swing), (11.0, -swing), (13.0, swing)] {
        c.thick_line(lx, 9.0 + bob, lx + sw, ground - 0.5, 0.6, rgb(0x2a2420));
    }
    for i in 0..7 {
        let px = 5.0 + (i % 4) as f32 * 2.6;
        let py = 6.0 + (i / 4) as f32 * 2.6 + bob;
        c.lit_ellipse(px, py, 2.6, 2.4, wool);
    }
    let (hx, hy) = if front { (15.5, 7.5 + bob) } else { (14.5, 5.5 + bob) };
    c.lit_ellipse(hx, hy, 2.0, 1.8, rgb(0x2e2824));
    c.set(hx as i32 - 1, hy as i32 - 2, rgb(0x2e2824));
    if front {
        c.set(hx as i32 + 1, hy as i32 - 1, rgb(0xd0c8b0));
    }
}

fn draw_chicken(c: &mut Canvas, front: bool, frame: u32) {
    let bob = if frame % 2 == 1 { -1.0 } else { 0.0 };
    let step = if frame == 0 { 1.0 } else if frame == 2 { -1.0 } else { 0.0 };
    c.linef(5.0, 8.0, 5.0 + step, 10.0, rgb(0xd89a30));
    c.linef(7.0, 8.0, 7.0 - step, 10.0, rgb(0xd89a30));
    c.poly(&[(2.0, 4.0 + bob), (4.0, 6.0 + bob), (2.5, 7.0 + bob)], rgb(0xe8e2d6));
    c.lit_ellipse(6.0, 6.5 + bob, 3.6, 2.6, rgb(0xf2eee4));
    let (hx, hy) = if front { (9.0, 3.5 + bob) } else { (8.5, 3.0 + bob) };
    c.lit_ellipse(hx, hy, 1.6, 1.6, rgb(0xf2eee4));
    c.set(hx as i32, hy as i32 - 2, rgb(0xd0302a));
    c.set(hx as i32 + 1, hy as i32 - 2, rgb(0xd0302a));
    if front {
        c.set(hx as i32 + 2, hy as i32, rgb(0xe0a020));
        c.set(hx as i32 + 1, hy as i32 - 1, rgb(0x101010));
    }
}

/// Donkey pulling the cart (quadruped in donkey colours).
fn draw_donkey(c: &mut Canvas, front: bool, frame: u32) {
    draw_quadruped(c, rgb(0x8a7a6a), None, rgb(0x7a6a5a), front, frame, 10.0, 7.0, 6.0, 3.4, 5);
    // Long ears.
    let ey = if front { 4 } else { 1 };
    c.fill_rect(17, ey, 1, 3, rgb(0x5a4a3a));
    c.fill_rect(18, ey - 1, 1, 3, rgb(0x6a5a4a));
}

fn draw_cart(dir: Dir, frame: u32) -> Canvas {
    // Built facing se (+a) or ne (−b); sw / nw are mirrored by the caller.
    let along_a = matches!(dir, Dir::Se | Dir::Sw);
    let mut ic = IsoCanvas::new(1, 1, 18);
    let wood = Mat::Boards(rgb(palette::PLANK));
    let (l0, l1, w0, w1) = (0.22, 0.72, 0.36, 0.64);
    let bed = |l: f32, w: f32, z: f32| if along_a { v3(l, w, z) } else { v3(w, 1.0 - l, z) };
    let mut donkey = Canvas::new(22, 17);
    draw_donkey(&mut donkey, along_a, frame);
    let ahead = if along_a { v3(0.9, 0.5, 0.0) } else { v3(0.5, 0.0, 0.0) };
    if !along_a {
        // Moving away (towards −b): the donkey is behind the cart in depth, draw it first.
        ic.stamp(&donkey, ahead);
    }
    let (p0, p1) = (bed(l0, w0, 6.0), bed(l1, w1, 11.0));
    let min = v3(p0.a.min(p1.a), p0.b.min(p1.b), 6.0);
    let max = v3(p0.a.max(p1.a), p0.b.max(p1.b), 11.0);
    ic.block(min, max, wood, Mat::Boards(shade(rgb(palette::PLANK), 0.8)));
    // Cargo: two sacks and a crate.
    ic.block(v3(min.a + 0.06, min.b + 0.05, 11.0), v3(min.a + 0.24, min.b + 0.22, 16.0), Mat::Boards(rgb(palette::PLANK_LIGHT)), Mat::Boards(rgb(palette::PLANK_LIGHT)));
    let (sx, sy) = ic.project(v3((min.a + max.a) * 0.5 + 0.08, (min.b + max.b) * 0.5 + 0.06, 13.0));
    ic.canvas().lit_ellipse(sx, sy, 4.0, 3.0, rgb(0xcdb88a));
    // Shafts towards the donkey.
    let s0 = bed(l1, 0.42, 8.0);
    let s1 = bed(l1, 0.58, 8.0);
    let tip = |p: crate::iso::V3| if along_a { v3(p.a + 0.3, p.b, 6.0) } else { v3(p.a, p.b - 0.3, 6.0) };
    ic.line3(s0, tip(s0), rgb(palette::TIMBER));
    ic.line3(s1, tip(s1), rgb(palette::TIMBER));
    // Wheel on the visible side with rotating spokes.
    let hub = if along_a { v3(0.47, w1 + 0.03, 4.5) } else { v3(w1 + 0.03, 0.53, 4.5) };
    let r = 4.2;
    let rim = |t: f32, rr: f32| if along_a { v3(hub.a + rr * t.cos() / 32.0, hub.b, hub.z + rr * t.sin()) } else { v3(hub.a, hub.b - rr * t.cos() / 32.0, hub.z + rr * t.sin()) };
    let poly: Vec<crate::iso::V3> = (0..16).map(|i| rim(std::f32::consts::TAU * i as f32 / 16.0, r)).collect();
    let inside = if along_a { v3(hub.a, hub.b - 0.3, hub.z) } else { v3(hub.a - 0.3, hub.b, hub.z) };
    ic.face(&poly, inside, Mat::Flat(rgb(palette::TIMBER_DARK)));
    let inner: Vec<crate::iso::V3> = (0..16).map(|i| rim(std::f32::consts::TAU * i as f32 / 16.0, r - 1.3)).collect();
    ic.face(&inner, inside, Mat::Flat(rgb(palette::PLANK)));
    for k in 0..3 {
        let t = k as f32 * std::f32::consts::PI / 3.0 + frame as f32 * std::f32::consts::PI / 12.0;
        ic.line3(rim(t, r - 1.0), rim(t + std::f32::consts::PI, r - 1.0), rgb(palette::TIMBER_DARK));
    }
    if along_a {
        ic.stamp(&donkey, ahead);
    }
    let (canvas, _) = ic.finish();
    // Crop around the cart centre: ground point of (0.5, 0.5) is at (32, 34) in the 64×50 canvas.
    canvas.crop(4, 0, 56, 44)
}

/// A wheel on the visible side of a vehicle at `hub` with radius `r`, spokes turned by `frame`.
fn wheel(ic: &mut IsoCanvas, hub: crate::iso::V3, r: f32, along_a: bool, frame: u32) {
    let rim = |t: f32, rr: f32| {
        if along_a { v3(hub.a + rr * t.cos() / 32.0, hub.b, hub.z + rr * t.sin()) } else { v3(hub.a, hub.b - rr * t.cos() / 32.0, hub.z + rr * t.sin()) }
    };
    let poly: Vec<crate::iso::V3> = (0..16).map(|i| rim(std::f32::consts::TAU * i as f32 / 16.0, r)).collect();
    let inside = if along_a { v3(hub.a, hub.b - 0.3, hub.z) } else { v3(hub.a - 0.3, hub.b, hub.z) };
    ic.face(&poly, inside, Mat::Flat(rgb(palette::TIMBER_DARK)));
    let inner: Vec<crate::iso::V3> = (0..16).map(|i| rim(std::f32::consts::TAU * i as f32 / 16.0, r - 1.3)).collect();
    ic.face(&inner, inside, Mat::Flat(rgb(palette::PLANK)));
    for k in 0..3 {
        let t = k as f32 * std::f32::consts::PI / 3.0 + frame as f32 * std::f32::consts::PI / 12.0;
        ic.line3(rim(t, r - 1.0), rim(t + std::f32::consts::PI, r - 1.0), rgb(palette::TIMBER_DARK));
    }
}

/// Siege engine: a catapult on a wheeled timber frame, its arm cocked back with a stone in the
/// bucket and a counterweight box at the front. Built facing se (+a) or ne (−b); sw / nw are
/// mirrored by the caller.
///
/// `throw`: frame of the throwing loop (`None`: cocked with a stone in the bucket, as it
/// travels): cocked, swinging up, thrown forward (empty), falling back.
fn draw_siege_arm(dir: Dir, frame: u32, throw: Option<u32>) -> Canvas {
    let along_a = matches!(dir, Dir::Se | Dir::Sw);
    let mut ic = IsoCanvas::new(1, 1, 30);
    let at = |l: f32, w: f32, z: f32| if along_a { v3(l, w, z) } else { v3(w, 1.0 - l, z) };
    let block = |ic: &mut IsoCanvas, a: crate::iso::V3, b: crate::iso::V3, mat: Mat, top: Mat| {
        ic.block(v3(a.a.min(b.a), a.b.min(b.b), a.z.min(b.z)), v3(a.a.max(b.a), a.b.max(b.b), a.z.max(b.z)), mat, top);
    };
    let timber = Mat::Boards(rgb(palette::TIMBER));
    let timber_top = Mat::Boards(shade(rgb(palette::TIMBER), 1.15));
    // Base frame.
    block(&mut ic, at(0.18, 0.34, 4.0), at(0.82, 0.66, 8.0), timber, timber_top);
    // Two uprights carrying the axle of the arm.
    for w in [0.34, 0.6] {
        block(&mut ic, at(0.5, w, 8.0), at(0.58, w + 0.06, 20.0), timber, timber_top);
    }
    // Counterweight box (front) and the arm cocked back to a raised bucket (rear).
    block(&mut ic, at(0.62, 0.4, 10.0), at(0.78, 0.6, 17.0), Mat::Boards(rgb(palette::PLANK)), Mat::Flat(rgb(palette::STONE_DARK)));
    let pivot = at(0.54, 0.5, 19.0);
    // Arm angle from pointing backwards (0) over straight up (90°) to forward (180°).
    let (phi, loaded) = match throw {
        None => (0.55f32, true),
        Some(f) => ([0.55, 1.5, 2.7, 1.9][(f % ACT_FRAMES) as usize], f % ACT_FRAMES == 0),
    };
    let reach = |w: f32| at(0.54 - 0.42 * phi.cos(), w, 19.0 + 14.0 * phi.sin());
    let bucket = reach(0.5);
    ic.line3(pivot, bucket, rgb(palette::TIMBER_DARK));
    ic.line3(at(0.54, 0.48, 19.0), reach(0.48), rgb(palette::TIMBER));
    // The counterweight end swings the other way.
    ic.line3(pivot, at(0.54 + 0.16 * phi.cos(), 0.5, 19.0 - 4.0 * phi.sin().max(0.2)), rgb(palette::TIMBER_DARK));
    let (bx, by) = ic.project(bucket);
    if loaded {
        ic.canvas().lit_ellipse(bx, by - 1.0, 2.6, 2.0, rgb(palette::STONE));
    } else {
        ic.canvas().lit_ellipse(bx, by - 0.5, 2.0, 1.2, rgb(palette::TIMBER_DARK));
    }
    // Wheels on the visible side.
    for l in [0.3, 0.7] {
        let hub = if along_a { v3(l, 0.69, 4.0) } else { v3(0.69, 1.0 - l, 4.0) };
        wheel(&mut ic, hub, 3.8, along_a, frame);
    }
    let (canvas, _) = ic.finish();
    // Same frame as the cart: the ground point of (0.5, 0.5) (canvas row 30 + 16) lands at
    // (28, 34) of the 56 × 44 crop.
    canvas.crop(4, 12, 56, 44)
}

/// The banner groups carry on the map (`banner`): a dark pole with a white swallow-tailed cloth
/// the game tints in the kingdom's colour.
pub fn banner() -> (Canvas, (f32, f32)) {
    let mut c = Canvas::new(14, 34);
    c.fill_rect(2, 3, 1, 31, rgb(0x2e2218));
    c.fill_rect(1, 1, 3, 2, rgb(0x5a5048));
    let cloth = [(3.0, 3.0), (13.0, 3.0), (13.0, 14.0), (8.0, 11.0), (3.0, 14.0)];
    c.fill_polygon(&cloth, |x, y| {
        let wave = if (x + y / 3) % 4 == 0 { 0.86 } else { 1.0 };
        let fold = if x > 8 { 0.9 } else { 1.0 };
        shade(rgb(0xf4f0e8), wave * fold)
    });
    c.outline(rgba(palette::OUTLINE, 200));
    (c, (2.5 / 14.0, 1.0))
}

/// Renders one sprite of `role` facing `dir`, walk frame `frame` (0..4).
pub fn sprite(role: &str, dir: Dir, frame: u32) -> (Canvas, (f32, f32)) {
    posed(role, dir, Pose::Walk(frame))
}

/// Renders `role` facing `dir` in `pose`. Work and attack frames of people are
/// [`ACT_WIDTH`] wide with the figure centred (same anchor as the walk frames); the siege
/// engine keeps its size. Roles without such a loop draw their walk frame.
pub fn posed(role: &str, dir: Dir, pose: Pose) -> (Canvas, (f32, f32)) {
    let front = dir.front();
    let frame = match pose {
        Pose::Walk(f) => f,
        Pose::Work(_) | Pose::Attack(_) | Pose::Gather(..) => 1,
    };
    let act = match pose {
        Pose::Walk(_) => None,
        Pose::Work(f) | Pose::Attack(f) | Pose::Gather(_, f) => Some(f),
    };
    let tool = match pose {
        Pose::Gather(g, _) if GATHER_ROLES.contains(&role) => Some(g.tool()),
        _ => tool_of(role),
    };
    let (mut c, anchor) = match role {
        "cow" => {
            let mut c = Canvas::new(26, 18);
            draw_cow(&mut c, front, frame);
            (c, (0.5, 1.0))
        }
        "sheep" => {
            let mut c = Canvas::new(20, 15);
            draw_sheep(&mut c, front, frame);
            (c, (0.5, 1.0))
        }
        "chicken" => {
            let mut c = Canvas::new(12, 12);
            draw_chicken(&mut c, front, frame);
            (c, (0.5, 1.0))
        }
        "cart" => {
            let base = if dir.mirrored() { if dir == Dir::Sw { Dir::Se } else { Dir::Ne } } else { dir };
            (draw_cart(base, frame), (0.5, 34.0 / 44.0))
        }
        "siege" => {
            let base = if dir.mirrored() { if dir == Dir::Sw { Dir::Se } else { Dir::Ne } } else { dir };
            (draw_siege_arm(base, frame, act), (0.5, 34.0 / 44.0))
        }
        _ => match (act, tool) {
            (Some(f), Some(tool)) => {
                let lk = human_look(role);
                let mut body = Canvas::new(16, 24);
                draw_human(&mut body, &lk, front, frame, true);
                let mut c = Canvas::new(ACT_WIDTH, 24);
                c.blit(&body, ((ACT_WIDTH - 16) / 2) as i32, 0);
                draw_action(&mut c, &lk, tool, f, front);
                (c, (0.5, 1.0))
            }
            _ => {
                let mut c = Canvas::new(16, 24);
                draw_human(&mut c, &human_look(role), front, frame, false);
                (c, (0.5, 1.0))
            }
        },
    };
    if role != "cart" && role != "siege" {
        c.outline(rgba(palette::OUTLINE, 220));
    }
    if dir.mirrored() {
        c = c.mirrored();
    }
    (c, anchor)
}

/// Assembles the `characters` atlas including `<role>_<dir>_walk` animations.
pub fn atlas() -> AtlasBuilder {
    let mut atlas = AtlasBuilder::new("characters", Sampling::Pixel);
    for role in ROLES {
        for dir in DIRS {
            let mut frames = Vec::new();
            for f in 0..FRAMES {
                let (c, anchor) = sprite(role, dir, f);
                let name = format!("{role}_{}_{f}", dir.name());
                atlas.add(name.clone(), c, anchor);
                frames.push(name);
            }
            atlas.animation(&format!("{role}_{}_walk", dir.name()), frames, WALK_FPS, true);
            let loops = [("work", WORK_ROLES.contains(&role)), ("attack", ATTACK_ROLES.contains(&role))];
            for (kind, has) in loops {
                if !has {
                    continue;
                }
                let mut frames = Vec::new();
                for f in 0..ACT_FRAMES {
                    let pose = if kind == "work" { Pose::Work(f) } else { Pose::Attack(f) };
                    let (c, anchor) = posed(role, dir, pose);
                    let name = format!("{role}_{}_{kind}_{f}", dir.name());
                    atlas.add(name.clone(), c, anchor);
                    frames.push(name);
                }
                atlas.animation(&format!("{role}_{}_{kind}", dir.name()), frames, ACT_FPS, true);
            }
            if GATHER_ROLES.contains(&role) {
                for gather in Gather::ALL {
                    let kind = gather.name();
                    let names: Vec<String> = (0..ACT_FRAMES).map(|f| format!("{role}_{}_{kind}_{f}", dir.name())).collect();
                    for (f, name) in (0..ACT_FRAMES).zip(&names) {
                        let (c, anchor) = posed(role, dir, Pose::Gather(gather, f));
                        atlas.add(name.clone(), c, anchor);
                    }
                    let frames = gather.sequence().iter().map(|&f| names[f as usize].clone()).collect();
                    atlas.animation(&format!("{role}_{}_{kind}", dir.name()), frames, ACT_FPS, true);
                }
            }
        }
    }
    let (banner, anchor) = banner();
    atlas.add("banner".to_owned(), banner, anchor);
    atlas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_direction_and_frame_renders() {
        for role in ROLES {
            for dir in DIRS {
                for f in 0..FRAMES {
                    let (c, anchor) = sprite(role, dir, f);
                    assert!(c.coverage() > 0.08, "{role} {dir:?} {f} is nearly empty");
                    assert!((0.0..=1.0).contains(&anchor.0) && (0.0..=1.0).contains(&anchor.1));
                }
            }
        }
    }

    #[test]
    fn west_facing_sprites_mirror_east_facing() {
        let (se, _) = sprite("man", Dir::Se, 0);
        let (sw, _) = sprite("man", Dir::Sw, 0);
        assert_eq!(se.mirrored().img, sw.img);
    }

    #[test]
    fn walk_frames_differ() {
        let (a, _) = sprite("farmer", Dir::Se, 0);
        let (b, _) = sprite("farmer", Dir::Se, 2);
        assert_ne!(a.img, b.img);
        let (c0, _) = sprite("cart", Dir::Ne, 0);
        let (c1, _) = sprite("cart", Dir::Ne, 1);
        assert_ne!(c0.img, c1.img);
    }

    #[test]
    fn raiders_and_siege_engines_look_unlike_citizens() {
        let (raider, _) = sprite("raider", Dir::Se, 1);
        let (man, _) = sprite("man", Dir::Se, 1);
        assert_ne!(raider.img, man.img);
        let (siege, anchor) = sprite("siege", Dir::Se, 0);
        assert_eq!((siege.width(), siege.height()), (56, 44));
        assert!((anchor.1 - 34.0 / 44.0).abs() < 1e-6);
        let (b, (ax, ay)) = banner();
        assert!(b.coverage() > 0.2 && ay == 1.0 && ax < 0.3);
    }

    #[test]
    fn work_and_attack_loops_move_the_tool() {
        for role in WORK_ROLES.iter().chain(&ATTACK_ROLES) {
            for dir in DIRS {
                let frames: Vec<(Canvas, (f32, f32))> = (0..ACT_FRAMES)
                    .map(|f| posed(role, dir, if WORK_ROLES.contains(role) { Pose::Work(f) } else { Pose::Attack(f) }))
                    .collect();
                let (walk, walk_anchor) = sprite(role, dir, 1);
                for (c, anchor) in &frames {
                    assert_eq!(*anchor, walk_anchor, "{role} {dir:?}");
                    assert_eq!(c.height(), walk.height(), "{role} {dir:?}");
                    assert!(c.coverage() > 0.04, "{role} {dir:?} is nearly empty");
                }
                assert!(frames.windows(2).all(|w| w[0].0.img != w[1].0.img), "{role} {dir:?}: frames repeat");
            }
        }
        // People: wider canvas with the figure centred, so the feet stay at the anchor.
        let (work, _) = posed("builder", Dir::Se, Pose::Work(2));
        assert_eq!(work.width() as u32, ACT_WIDTH);
        let feet = |c: &Canvas| (0..c.width()).filter(|x| c.get(*x, c.height() - 1)[3] > 0).map(|x| x as f32).sum::<f32>()
            / (0..c.width()).filter(|x| c.get(*x, c.height() - 1)[3] > 0).count().max(1) as f32;
        assert!((feet(&work) / work.width() as f32 - 0.5).abs() < 0.1);
        // The siege engine throws: the stone is gone in the throw frame.
        let (cocked, _) = posed("siege", Dir::Se, Pose::Attack(0));
        let (thrown, _) = posed("siege", Dir::Se, Pose::Attack(2));
        assert_ne!(cocked.img, thrown.img);
        assert_eq!((thrown.width(), thrown.height()), (56, 44));
    }

    #[test]
    fn gatherer_loops_use_their_own_tools() {
        for role in GATHER_ROLES {
            for dir in DIRS {
                let (walk, walk_anchor) = sprite(role, dir, 1);
                let axe: Vec<Canvas> = (0..ACT_FRAMES).map(|f| posed(role, dir, Pose::Work(f)).0).collect();
                let mut loops = Vec::new();
                for gather in Gather::ALL {
                    let frames: Vec<(Canvas, (f32, f32))> = (0..ACT_FRAMES).map(|f| posed(role, dir, Pose::Gather(gather, f))).collect();
                    for (c, anchor) in &frames {
                        assert_eq!(*anchor, walk_anchor, "{role} {dir:?} {gather:?}");
                        assert_eq!((c.width() as u32, c.height()), (ACT_WIDTH, walk.height()), "{role} {dir:?} {gather:?}");
                        assert!(c.coverage() > 0.04, "{role} {dir:?} {gather:?} is nearly empty");
                    }
                    assert!(frames.windows(2).all(|w| w[0].0.img != w[1].0.img), "{role} {dir:?} {gather:?}: frames repeat");
                    for (f, (c, _)) in frames.iter().enumerate() {
                        assert_ne!(c.img, axe[f].img, "{role} {dir:?} {gather:?} {f} looks like the axe");
                    }
                    loops.push(frames);
                }
                // Every tool looks different from every other one.
                for f in 0..ACT_FRAMES as usize {
                    for a in 0..loops.len() {
                        for b in a + 1..loops.len() {
                            assert_ne!(loops[a][f].0.img, loops[b][f].0.img, "{role} {dir:?}: loops {a} and {b} share frame {f}");
                        }
                    }
                }
            }
        }
        // Each loop shows every drawn frame; the angler casts once and holds.
        for gather in Gather::ALL {
            let seq = gather.sequence();
            assert!((0..ACT_FRAMES).all(|f| seq.contains(&f)), "{gather:?}");
            assert_eq!(seq[0], 0);
        }
        assert!(Gather::Rod.sequence().len() > ACT_FRAMES as usize);
    }

    #[test]
    fn humans_stand_on_the_bottom_row() {
        for role in ["man", "woman", "child", "knight"] {
            let (c, _) = sprite(role, Dir::Se, 0);
            let bottom = (0..c.width()).any(|x| c.get(x, c.height() - 1)[3] > 0 || c.get(x, c.height() - 2)[3] > 0);
            assert!(bottom, "{role} floats above its anchor");
        }
    }
}
