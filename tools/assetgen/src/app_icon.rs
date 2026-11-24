//! The application icon (`assets/app_icon.png`, 1024 × 1024): the castle of the `buildings`
//! atlas, enlarged pixel for pixel, on a royal-blue rounded square with a gold rim. The macOS
//! bundle script (`tools/macos/make_app.sh`) turns it into `Kingdoms.icns`.
//!
//! The body follows Apple's icon grid: an 824 px rounded square (corner radius 185 px) centred
//! in the 1024 px canvas, the rest transparent (room for the system's shadow).

use crate::draw::{Canvas, Px, mix, rgb, shade};
use crate::palette;

/// Edge of the icon in pixels.
pub const SIZE: u32 = 1024;
/// Margin around the rounded square body.
const INSET: f32 = 100.0;
/// Corner radius of the body.
const RADIUS: f32 = 185.0;
/// Width of the gold rim.
const RIM: f32 = 14.0;

/// Signed distance of pixel centre (x, y) to the rounded square body (negative inside).
fn body_distance(x: f32, y: f32) -> f32 {
    let half = (SIZE as f32 - 2.0 * INSET) * 0.5;
    let c = SIZE as f32 * 0.5;
    let q = ((x - c).abs() - (half - RADIUS), (y - c).abs() - (half - RADIUS));
    let outside = (q.0.max(0.0).powi(2) + q.1.max(0.0).powi(2)).sqrt();
    outside + q.0.max(q.1).min(0.0) - RADIUS
}

/// `src` enlarged `k` times without smoothing (crisp pixel art).
fn enlarged(src: &Canvas, k: u32) -> Canvas {
    let k = k.max(1) as i32;
    let mut out = Canvas::new(src.width() as u32 * k as u32, src.height() as u32 * k as u32);
    for y in 0..out.height() {
        for x in 0..out.width() {
            out.set(x, y, src.get(x / k, y / k));
        }
    }
    out
}

/// Renders the icon.
pub fn render() -> Canvas {
    let mut c = Canvas::new(SIZE, SIZE);
    let top = rgb(palette::KINGDOM_COLORS[1]);
    let bottom = shade(top, 0.45);
    let gold = rgb(palette::GOLD);
    let centre = SIZE as f32 * 0.5;
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = body_distance(px, py);
            let coverage = (0.5 - d).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            // Sky gradient with a soft light behind the castle.
            let t = ((py - INSET) / (SIZE as f32 - 2.0 * INSET)).clamp(0.0, 1.0);
            let glow = (1.0 - ((px - centre).powi(2) + (py - centre * 0.95).powi(2)).sqrt() / 420.0).clamp(0.0, 1.0);
            let mut col: Px = mix(mix(top, bottom, t), rgb(0x6f8fd0), glow * 0.45);
            col[3] = (coverage * 255.0).round() as u8;
            c.set(x, y, col);
        }
    }
    // A green hill the castle stands on.
    let hill_y = SIZE as f32 * 0.74;
    c.fill_ellipse(centre, hill_y, 330.0, 92.0, |_, y, _| {
        let t = ((y as f32 - (hill_y - 92.0)) / 184.0).clamp(0.0, 1.0);
        mix(rgb(palette::GRASS), rgb(palette::GRASS_DARK), t)
    });
    // The castle, twice its sprite size (crisp pixels), standing on the hill.
    if let Some((castle, _)) = crate::buildings::render("castle")
        && let Some((x, y, w, h)) = castle.opaque_bounds()
    {
        let sprite = enlarged(&castle.crop(x, y, w as u32, h as u32), 2);
        let (sw, sh) = (sprite.width(), sprite.height());
        c.blit(&sprite, (SIZE as i32 - sw) / 2, (hill_y + 40.0) as i32 - sh);
    }
    // The gold rim (with a darker outer edge) last, over anything that reaches the edge.
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let d = body_distance(x as f32 + 0.5, y as f32 + 0.5);
            if d > -RIM {
                let mut rim = if d > -RIM * 0.35 { shade(gold, 0.8) } else { gold };
                rim[3] = ((0.5 - d).clamp(0.0, 1.0) * 255.0).round() as u8;
                c.set(x, y, rim);
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_is_a_rounded_square_with_the_castle() {
        let icon = render();
        assert_eq!((icon.width(), icon.height()), (SIZE as i32, SIZE as i32));
        // Transparent outside the body (corners and margin), opaque inside.
        for (x, y) in [(0, 0), (1023, 1023), (110, 110), (50, 512)] {
            assert_eq!(icon.get(x, y)[3], 0, "({x}, {y}) is outside the body");
        }
        for (x, y) in [(512, 150), (150, 512), (512, 900)] {
            assert_eq!(icon.get(x, y)[3], 255, "({x}, {y}) is inside the body");
        }
        // The rim is gold, the castle's grey stone stands in the middle.
        let rim = icon.get(512, 105);
        assert!(rim[0] > 150 && rim[2] < 120, "rim {rim:?}");
        let middle = icon.get(512, 560);
        assert!((i32::from(middle[0]) - i32::from(middle[2])).abs() < 60, "castle stone, not sky: {middle:?}");
        // Deterministic.
        assert_eq!(render().img, icon.img);
    }
}
