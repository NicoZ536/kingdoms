//! Map previews of the player tools: the building ghost with per-tile markers, the entrance
//! and the suggested access road; planned roads while dragging; tiles a demolition drag clears;
//! the building or area a company is ordered to defend and the patrol route being picked; and
//! the pending orders that wait in the command queue for the next tick (while the game is
//! paused): placements and moves as bluish ghosts, roads as bluish road sprites.
//!
//! Everything is derived from the tool state ([`ToolController`]), which caches the
//! simulation's checks, and from the queued commands; nothing here queries the simulation's
//! validation itself.

use std::collections::BTreeSet;

use glam::{IVec2, Vec2};
use kingdoms_engine::assets::AssetManager;
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::{self, SpriteDepth};
use kingdoms_engine::rendering::sprite::{FLAG_EMISSIVE, RenderList, SpriteInstance};
use kingdoms_sim::buildings::{EntranceStatus, PlacementCheck, TileStatus};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::Defs;
use kingdoms_sim::grid::{Direction, TileRect};
use kingdoms_sim::roads::{RoadPlan, RoadTile};
use kingdoms_sim::state::GameState;

use super::buildings::{BuildingSprites, Part, footprint_elevation, sprite_flipped};
use super::overlay::{HighlightStyle, OverlaySprites};
use super::{HIGHLIGHT_BIAS, PREVIEW_BIAS};
use super::orders;
use crate::tools::{OrderPick, PlacementPreview, Tool, ToolController, l_line, queued_building};

const GHOST_OK: Color = Color::rgba(0.7, 1.0, 0.7, 0.86);
const GHOST_BAD: Color = Color::rgba(1.0, 0.5, 0.45, 0.86);
/// A free site no storage reaches (its material would never arrive).
const GHOST_UNSUPPLIED: Color = Color::rgba(1.0, 0.78, 0.4, 0.86);
/// World size of the cross on blocked tiles.
const MARKER_SIZE: f32 = 14.0;
const FILL_FREE: Color = Color::rgba(0.35, 0.95, 0.35, 0.26);
const FILL_CLEARING: Color = Color::rgba(1.0, 0.8, 0.2, 0.34);
const FILL_BLOCKED: Color = Color::rgba(1.0, 0.18, 0.12, 0.5);
const MARK_BLOCKED: Color = Color::rgba(1.0, 0.25, 0.2, 0.95);
const ROAD_OK: Color = Color::rgba(1.0, 1.0, 1.0, 0.8);
const ROAD_BAD: Color = Color::rgba(1.0, 0.45, 0.38, 0.8);
const ACCESS_ROAD: Color = Color::rgba(1.0, 0.95, 0.75, 0.6);
/// Orders waiting for the next tick: ghost and road tint.
const PENDING_GHOST: Color = Color::rgba(0.72, 0.84, 1.0, 0.72);
const PENDING_ROAD: Color = Color::rgba(0.72, 0.84, 1.0, 0.7);
/// Outline of a pending footprint.
const PENDING: HighlightStyle = HighlightStyle {
    outline: Color { r: 0.5, g: 0.75, b: 1.0, a: 0.95 },
    fill: Color { r: 0.5, g: 0.72, b: 1.0, a: 0.24 },
    ghost: Color { r: 0.5, g: 0.75, b: 1.0, a: 0.4 },
};

/// Outline style of a whole footprint.
const FOOTPRINT_OK: HighlightStyle = HighlightStyle {
    outline: Color { r: 0.45, g: 1.0, b: 0.45, a: 0.95 },
    fill: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 },
    ghost: Color { r: 0.45, g: 1.0, b: 0.45, a: 0.45 },
};
const FOOTPRINT_BAD: HighlightStyle = HighlightStyle {
    outline: Color { r: 1.0, g: 0.3, b: 0.25, a: 0.95 },
    fill: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 },
    ghost: Color { r: 1.0, g: 0.3, b: 0.25, a: 0.5 },
};
const FOOTPRINT_UNSUPPLIED: HighlightStyle = HighlightStyle {
    outline: Color { r: 1.0, g: 0.62, b: 0.15, a: 0.95 },
    fill: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 },
    ghost: Color { r: 1.0, g: 0.62, b: 0.15, a: 0.5 },
};

/// How the ghost of a placement looks: green where it can be built, amber where it can be
/// placed but no storage reaches it (it would never be finished), red where it cannot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GhostLook {
    Ok,
    Unsupplied,
    Blocked,
}

impl GhostLook {
    pub fn of(check: &PlacementCheck) -> Self {
        if !check.ok {
            GhostLook::Blocked
        } else if crate::tools::site_unsupplied(check) {
            GhostLook::Unsupplied
        } else {
            GhostLook::Ok
        }
    }

    fn footprint(self) -> &'static HighlightStyle {
        match self {
            GhostLook::Ok => &FOOTPRINT_OK,
            GhostLook::Unsupplied => &FOOTPRINT_UNSUPPLIED,
            GhostLook::Blocked => &FOOTPRINT_BAD,
        }
    }

    fn tint(self) -> Color {
        match self {
            GhostLook::Ok => GHOST_OK,
            GhostLook::Unsupplied => GHOST_UNSUPPLIED,
            GhostLook::Blocked => GHOST_BAD,
        }
    }
}
/// Demolition marks (hovered road tiles, drag line).
pub const DEMOLISH: HighlightStyle = HighlightStyle {
    outline: Color { r: 1.0, g: 0.3, b: 0.2, a: 1.0 },
    fill: Color { r: 1.0, g: 0.2, b: 0.1, a: 0.4 },
    ghost: Color { r: 1.0, g: 0.3, b: 0.2, a: 0.35 },
};

/// Sprites the previews are drawn with.
pub struct PreviewArt<'a> {
    pub buildings: &'a BuildingSprites,
    pub overlay: &'a OverlaySprites,
    pub assets: &'a AssetManager,
}

/// Draws the preview of the active tool.
pub fn draw(
    list: &mut RenderList,
    state: &GameState,
    defs: &Defs,
    tools: &ToolController,
    hovered_tile: Option<IVec2>,
    art: &PreviewArt<'_>,
) {
    let overlay = art.overlay;
    match tools.tool() {
        Tool::Inspect => {}
        Tool::Build { .. } | Tool::Move { building: Some(_) } => {
            if let Some(preview) = tools.placement() {
                draw_placement(list, state, defs, preview, art);
            }
        }
        Tool::Move { building: None } => {}
        Tool::Road { .. } => {
            if let Some(drag) = tools.road_drag() {
                draw_road_plan(list, state, defs, &drag.plan, None, art);
            } else if let Some(plan) = tools.road_hover() {
                draw_road_plan(list, state, defs, plan, None, art);
            }
        }
        Tool::Order { pick: OrderPick::Patrol, .. } => {
            orders::draw_route(list, state, overlay, tools.patrol_points(), hovered_tile, orders::PICK_COLOR);
            if let Some(tile) = hovered_tile {
                overlay.footprint(list, state.world(), TileRect::tile(tile), &HighlightStyle::HOVERED_TILE);
            }
        }
        Tool::Order { pick: OrderPick::Defend { radius }, .. } => {
            let Some(tile) = hovered_tile else { return };
            let building = state.buildings().id_at(tile).and_then(|b| state.building(b)).filter(|b| b.is_complete());
            match building {
                Some(b) => orders::draw_building(list, state, overlay, b.rect(), orders::PICK_COLOR),
                None => orders::draw_area(list, state, overlay, tile, radius, orders::PICK_COLOR),
            }
        }
        Tool::Demolish => {
            let map = state.world();
            if let Some(drag) = tools.demolish_drag().filter(|d| d.start != d.end) {
                for tile in l_line(drag.start, drag.end) {
                    let style = if state.roads().has_road(tile) { DEMOLISH } else { HighlightStyle::HOVERED_TILE };
                    overlay.footprint(list, map, TileRect::tile(tile), &style);
                }
            } else if let Some(tile) = hovered_tile.filter(|t| state.roads().has_road(*t) && state.buildings().id_at(*t).is_none()) {
                overlay.footprint(list, map, TileRect::tile(tile), &DEMOLISH);
            }
        }
    }
}

/// Draws the map changes still waiting in the command queue (`queued`): while the game is
/// paused a placement only takes effect with the next tick, so it shows as a bluish ghost on
/// its footprint (with the access road the build tool planned for it) and a road as bluish
/// road sprites.
pub fn draw_pending(
    list: &mut RenderList,
    state: &GameState,
    defs: &Defs,
    queued: &[Command],
    tools: &ToolController,
    art: &PreviewArt<'_>,
) {
    let map = state.world();
    for command in queued {
        if let Some(pending) = queued_building(state, defs, command) {
            if let Some(access) = tools.recorded_plan(command) {
                draw_road_plan(list, state, defs, access, Some(PENDING_ROAD), art);
            }
            let rect = pending.footprint;
            art.overlay.footprint(list, map, rect, &PENDING);
            if let Some((sprite, dims)) = art.buildings.finished(pending.kind) {
                let elevation = footprint_elevation(map, rect);
                let flip = sprite_flipped(dims, rect.size, pending.rotation.facing());
                let part = Part { sprite, rect, elevation, flip, bias: 0.05 };
                let (mut instance, texture) = part.instance(art.assets);
                instance.depth_flat = SpriteDepth::OVERLAY.flat;
                instance.depth_tent = SpriteDepth::OVERLAY.tent;
                list.draw_transparent(texture, instance.with_color(PENDING_GHOST).with_flags(FLAG_EMISSIVE));
            }
        } else if let Command::BuildRoad { tiles, .. } = command {
            match tools.recorded_plan(command) {
                Some(plan) => draw_road_plan(list, state, defs, plan, Some(PENDING_ROAD), art),
                None => {
                    for p in tiles {
                        let Some(tile) = map.get(*p) else { continue };
                        let fill = art.overlay.fill(*p, f32::from(tile.elevation), PENDING.fill, HIGHLIGHT_BIAS);
                        list.draw_transparent(art.overlay.texture, fill);
                    }
                }
            }
        }
    }
}

fn draw_placement(list: &mut RenderList, state: &GameState, defs: &Defs, preview: &PlacementPreview, art: &PreviewArt<'_>) {
    let overlay = art.overlay;
    let map = state.world();
    let check = &preview.check;
    let rect = check.footprint;
    // Suggested access road under everything else.
    if let Some(access) = preview.access.as_ref().filter(|a| a.valid) {
        draw_road_plan(list, state, defs, access, Some(ACCESS_ROAD), art);
    }
    // Per-tile state on the ground.
    for (p, status) in &check.tile_status {
        let Some(tile) = map.get(*p) else { continue };
        let color = match status {
            TileStatus::Free => FILL_FREE,
            TileStatus::NeedsClearing => FILL_CLEARING,
            TileStatus::Blocked(_) => FILL_BLOCKED,
        };
        list.draw_transparent(overlay.texture, overlay.fill(*p, f32::from(tile.elevation), color, HIGHLIGHT_BIAS));
    }
    let look = GhostLook::of(check);
    overlay.footprint(list, map, rect, look.footprint());
    // The ghost of the finished building, always on top so trees that will be cleared do not
    // hide it.
    if let Some((sprite, dims)) = art.buildings.finished(check.kind) {
        let elevation = footprint_elevation(map, rect);
        let part = Part { sprite, rect, elevation, flip: sprite_flipped(dims, rect.size, check.rotation.facing()), bias: 0.05 };
        let (mut instance, texture) = part.instance(art.assets);
        instance.depth_flat = SpriteDepth::OVERLAY.flat;
        instance.depth_tent = SpriteDepth::OVERLAY.tent;
        list.draw_transparent(texture, instance.with_color(look.tint()).with_flags(FLAG_EMISSIVE));
    }
    // Blocked tiles: red edges and a cross marker above the ghost (the icons atlas is drawn after
    // the buildings atlas among overlay sprites).
    let cross = art.assets.try_sprite("icons/ui_close").map(|id| *art.assets.region(id));
    for (p, status) in &check.tile_status {
        if !matches!(status, TileStatus::Blocked(_)) {
            continue;
        }
        let Some(tile) = map.get(*p) else { continue };
        let elevation = f32::from(tile.elevation);
        for dir in Direction::ALL {
            let mut edge = overlay.edge(*p, elevation, dir, MARK_BLOCKED, PREVIEW_BIAS);
            edge.depth_flat = SpriteDepth::OVERLAY.flat;
            edge.depth_tent = SpriteDepth::OVERLAY.tent;
            list.draw_transparent(overlay.texture, edge);
        }
        if let Some(r) = cross {
            let pos = iso::tile_center_world(*p, elevation);
            let size = Vec2::splat(MARKER_SIZE);
            let marker = SpriteInstance::new(pos, size, Vec2::splat(0.5), r.uv, SpriteDepth::OVERLAY).with_flags(FLAG_EMISSIVE);
            list.draw_transparent(r.texture, marker);
        }
    }
    // Entrance: where the road has to connect.
    let entrance_style = match check.entrance_status {
        EntranceStatus::Road => HighlightStyle {
            outline: Color::rgba(0.45, 1.0, 0.45, 1.0),
            fill: Color::rgba(0.45, 1.0, 0.45, 0.35),
            ghost: Color::rgba(0.45, 1.0, 0.45, 0.6),
        },
        EntranceStatus::Free => HighlightStyle {
            outline: Color::rgba(1.0, 0.9, 0.4, 1.0),
            fill: Color::rgba(1.0, 0.9, 0.4, 0.35),
            ghost: Color::rgba(1.0, 0.9, 0.4, 0.6),
        },
        EntranceStatus::Blocked => HighlightStyle {
            outline: MARK_BLOCKED,
            fill: Color::rgba(1.0, 0.2, 0.15, 0.45),
            ghost: Color::rgba(1.0, 0.25, 0.2, 0.7),
        },
    };
    if map.in_bounds(check.entrance) {
        overlay.footprint(list, map, TileRect::tile(check.entrance), &entrance_style);
    }
}

/// Draws planned road tiles as translucent road sprites (bridges included) with a fill per
/// tile: green for valid, red for invalid tiles. `tint` overrides the sprite colour (access
/// road suggestions).
fn draw_road_plan(
    list: &mut RenderList,
    state: &GameState,
    defs: &Defs,
    plan: &RoadPlan,
    tint: Option<Color>,
    art: &PreviewArt<'_>,
) {
    let (overlay, assets) = (art.overlay, art.assets);
    let map = state.world();
    let planned: BTreeSet<(i32, i32)> = plan.tiles.iter().map(|t| (t.pos.x, t.pos.y)).collect();
    for t in &plan.tiles {
        let Some(tile) = map.get(t.pos) else { continue };
        let elevation = f32::from(tile.elevation);
        let ok = t.ok && (plan.valid || tint.is_some());
        if tint.is_none() {
            let fill = if t.ok { FILL_FREE } else { FILL_BLOCKED };
            list.draw_transparent(overlay.texture, overlay.fill(t.pos, elevation, fill, HIGHLIGHT_BIAS));
        }
        let mut mask = 0u8;
        for dir in Direction::ALL {
            let n = t.pos + dir.offset();
            if planned.contains(&(n.x, n.y)) || state.roads().has_road(n) {
                mask |= dir.mask_bit();
            }
        }
        let name = RoadTile { road: t.road, bridge_axis: t.bridge_axis }.sprite(defs, mask);
        let r = assets.region(assets.sprite(&name));
        let instance = if t.bridge_axis.is_some() {
            let pos = iso::tile_to_world_elevated(t.pos.as_vec2() + Vec2::ONE, elevation);
            SpriteInstance::new(pos, r.size, r.anchor, r.uv, SpriteDepth::footprint(t.pos, glam::UVec2::ONE, PREVIEW_BIAS))
        } else {
            let centre = t.pos.as_vec2() + Vec2::splat(0.5);
            let pos = iso::tile_to_world_elevated(centre, elevation);
            SpriteInstance::new(pos, r.size, r.anchor, r.uv, SpriteDepth::ground(centre, PREVIEW_BIAS))
        };
        let color = tint.unwrap_or(if ok { ROAD_OK } else { ROAD_BAD });
        list.draw_transparent(r.texture, instance.with_color(color).with_flags(FLAG_EMISSIVE));
        if !t.ok && tint.is_none() {
            for dir in Direction::ALL {
                list.draw_transparent(overlay.texture, overlay.edge(t.pos, elevation, dir, MARK_BLOCKED, PREVIEW_BIAS + 0.003));
            }
        }
    }
}
