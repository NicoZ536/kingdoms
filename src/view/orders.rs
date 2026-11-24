//! Orders of the companies on the map: patrol routes as dotted loops with their waypoints,
//! defended areas as outlined squares and defended buildings as outlined footprints. Drawn for
//! the route the player is picking with the order tool (with the leg to the cursor) and, while
//! the military tab is open, for the orders every company has.

use glam::{IVec2, Vec2};
use kingdoms_engine::rendering::color::Color;
use kingdoms_engine::rendering::iso::SpriteDepth;
use kingdoms_engine::rendering::sprite::RenderList;
use kingdoms_sim::grid::TileRect;
use kingdoms_sim::military::{DefendTarget, Order};
use kingdoms_sim::state::GameState;

use super::overlay::{HighlightStyle, OverlaySprites};
use super::PREVIEW_BIAS;

/// Distance between the dots of a route line, in tiles.
pub const DOT_STEP: f32 = 0.38;
/// Colour of routes being picked.
pub const PICK_COLOR: Color = Color::rgba(1.0, 0.86, 0.35, 0.95);
/// Colour of existing orders.
pub const ORDER_COLOR: Color = Color::rgba(0.55, 0.78, 1.0, 0.85);

/// Points of a dotted line from tile `a` to tile `b` (tile centres), `step` tiles apart, without
/// the end point.
pub fn dots_between(a: IVec2, b: IVec2, step: f32) -> Vec<Vec2> {
    let (pa, pb) = (a.as_vec2() + Vec2::splat(0.5), b.as_vec2() + Vec2::splat(0.5));
    let length = (pb - pa).length();
    if length < 1e-3 || step <= 0.0 {
        return Vec::new();
    }
    let n = (length / step).ceil().max(1.0) as usize;
    (0..n).map(|i| pa + (pb - pa) * (i as f32 / n as f32)).collect()
}

fn elevation_at(state: &GameState, p: Vec2) -> f32 {
    state.world().get(p.floor().as_ivec2()).map_or(0.0, |t| f32::from(t.elevation))
}

/// A route through `points` (closed back to the first), with an open leg from the last point to
/// `cursor` while picking.
pub fn draw_route(list: &mut RenderList, state: &GameState, overlay: &OverlaySprites, points: &[IVec2], cursor: Option<IVec2>, color: Color) {
    let mut legs: Vec<(IVec2, IVec2, f32)> = points.windows(2).map(|w| (w[0], w[1], 1.0)).collect();
    match (points.last(), cursor) {
        (Some(last), Some(c)) if *last != c => legs.push((*last, c, 0.8)),
        _ => {}
    }
    if let (Some(first), Some(last)) = (points.first(), points.last())
        && points.len() >= 2
    {
        // The way back to the start, fainter.
        legs.push((*last, *first, 0.55));
    }
    for (a, b, alpha) in legs {
        let c = Color::rgba(color.r, color.g, color.b, color.a * alpha);
        for p in dots_between(a, b, DOT_STEP) {
            list.draw_transparent(overlay.texture, overlay.ground_dot(p, elevation_at(state, p), Vec2::new(10.0, 5.0), c, PREVIEW_BIAS));
        }
    }
    for (i, p) in points.iter().enumerate() {
        let centre = p.as_vec2() + Vec2::splat(0.5);
        let size = if i == 0 { Vec2::new(22.0, 11.0) } else { Vec2::new(15.0, 7.5) };
        let mut dot = overlay.ground_dot(centre, elevation_at(state, centre), size, color, PREVIEW_BIAS + 0.002);
        // Waypoints stay visible behind buildings.
        dot.depth_flat = SpriteDepth::OVERLAY.flat;
        dot.depth_tent = SpriteDepth::OVERLAY.tent;
        list.draw_transparent(overlay.texture, dot);
    }
}

/// The square of tiles within `radius` of `center` (the simulation's area of a defence order).
pub fn area_rect(center: IVec2, radius: u8) -> TileRect {
    TileRect::tile(center).expanded(i32::from(radius))
}

/// Outline and light fill of a defended area.
pub fn draw_area(list: &mut RenderList, state: &GameState, overlay: &OverlaySprites, center: IVec2, radius: u8, color: Color) {
    let size = state.world().size();
    let rect = area_rect(center, radius).clipped(size.x, size.y);
    if rect.area() <= 0 {
        return;
    }
    let style = HighlightStyle { outline: color, fill: Color::rgba(color.r, color.g, color.b, 0.10), ghost: Color::rgba(0.0, 0.0, 0.0, 0.0) };
    overlay.footprint(list, state.world(), rect, &style);
}

/// Highlight of a building a company defends.
pub fn draw_building(list: &mut RenderList, state: &GameState, overlay: &OverlaySprites, rect: TileRect, color: Color) {
    let style = HighlightStyle { outline: color, fill: Color::rgba(color.r, color.g, color.b, 0.16), ghost: Color::rgba(color.r, color.g, color.b, 0.4) };
    overlay.footprint(list, state.world(), rect, &style);
}

/// The places of every company's order (military tab open).
pub fn draw_company_orders(list: &mut RenderList, state: &GameState, overlay: &OverlaySprites) {
    for c in state.military().companies() {
        match c.order() {
            Order::Patrol { route } => draw_route(list, state, overlay, route, None, ORDER_COLOR),
            Order::Defend(DefendTarget::Area { center, radius }) => draw_area(list, state, overlay, *center, *radius, ORDER_COLOR),
            Order::Defend(DefendTarget::Building(b)) => {
                if let Some(b) = state.building(*b) {
                    draw_building(list, state, overlay, b.rect(), ORDER_COLOR);
                }
            }
            Order::Station | Order::Campaign { .. } | Order::Retreat => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_dots_run_from_centre_to_centre() {
        let dots = dots_between(IVec2::new(0, 0), IVec2::new(4, 0), 0.5);
        assert_eq!(dots.len(), 8);
        assert_eq!(dots[0], Vec2::new(0.5, 0.5));
        assert!(dots.iter().all(|d| d.y == 0.5 && d.x < 4.5));
        assert!(dots_between(IVec2::ONE, IVec2::ONE, 0.5).is_empty());
        // The defended square matches the simulation's area (radius in tiles around the centre).
        let r = area_rect(IVec2::new(10, 10), 3);
        assert_eq!((r.origin, r.size), (IVec2::new(7, 7), IVec2::new(7, 7)));
    }
}
