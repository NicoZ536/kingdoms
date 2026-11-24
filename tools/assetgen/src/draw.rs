//! Minimal raster drawing helpers for procedural placeholder art.
//!
//! Everything here is deterministic: no randomness besides the integer hash [`hash01`], no
//! platform-dependent behaviour besides ordinary IEEE float arithmetic.

use image::{Rgba, RgbaImage};

/// An RGBA pixel (straight, non-premultiplied alpha).
pub type Px = [u8; 4];

/// Fully transparent pixel.
pub const CLEAR: Px = [0, 0, 0, 0];

/// Deterministic integer hash → [0, 1).
pub fn hash01(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Smooth value noise in [0, 1).
pub fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let fx = x - xi as f32;
    let fy = y - yi as f32;
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let a = hash01(xi, yi, seed);
    let b = hash01(xi + 1, yi, seed);
    let c = hash01(xi, yi + 1, seed);
    let d = hash01(xi + 1, yi + 1, seed);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

/// Value noise whose lattice wraps every `period` cells in both axes, so textures built from it
/// tile seamlessly (used for road and field tiles that must join their neighbours).
pub fn periodic_noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let p = period.max(1);
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let fx = x - xi as f32;
    let fy = y - yi as f32;
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let h = |i: i32, j: i32| hash01(i.rem_euclid(p), j.rem_euclid(p), seed);
    let top = h(xi, yi) + (h(xi + 1, yi) - h(xi, yi)) * sx;
    let bottom = h(xi, yi + 1) + (h(xi + 1, yi + 1) - h(xi, yi + 1)) * sx;
    top + (bottom - top) * sy
}

/// Opaque colour from a `0xRRGGBB` literal.
pub fn rgb(hex: u32) -> Px {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255]
}

/// Colour from a `0xRRGGBB` literal with explicit alpha.
pub fn rgba(hex: u32, a: u8) -> Px {
    with_alpha(rgb(hex), a)
}

/// Multiplies the colour channels by `f` (alpha unchanged).
pub fn shade(c: Px, f: f32) -> Px {
    let m = |v: u8| (f32::from(v) * f).clamp(0.0, 255.0) as u8;
    [m(c[0]), m(c[1]), m(c[2]), c[3]]
}

/// Linear interpolation between two colours (including alpha).
pub fn mix(a: Px, b: Px, t: f32) -> Px {
    let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).clamp(0.0, 255.0) as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3])]
}

/// Replaces the alpha channel.
pub fn with_alpha(c: Px, a: u8) -> Px {
    [c[0], c[1], c[2], a]
}

/// Converts towards grey by `t` (0 = unchanged, 1 = fully desaturated).
pub fn desaturate(c: Px, t: f32) -> Px {
    let l = (0.3 * f32::from(c[0]) + 0.59 * f32::from(c[1]) + 0.11 * f32::from(c[2])) as u8;
    mix(c, [l, l, l, c[3]], t)
}

/// An RGBA raster with alpha-blending primitives.
#[derive(Clone)]
pub struct Canvas {
    pub img: RgbaImage,
}

impl Canvas {
    /// Transparent canvas of `w` × `h` pixels.
    pub fn new(w: u32, h: u32) -> Self {
        Self { img: RgbaImage::from_pixel(w, h, Rgba(CLEAR)) }
    }

    pub fn width(&self) -> i32 {
        self.img.width() as i32
    }

    pub fn height(&self) -> i32 {
        self.img.height() as i32
    }

    /// Pixel at (x, y); transparent outside the canvas.
    pub fn get(&self, x: i32, y: i32) -> Px {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            return CLEAR;
        }
        self.img.get_pixel(x as u32, y as u32).0
    }

    /// Alpha-blends `c` over the pixel.
    pub fn blend(&mut self, x: i32, y: i32, c: Px) {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() || c[3] == 0 {
            return;
        }
        let dst = self.img.get_pixel_mut(x as u32, y as u32);
        let a = f32::from(c[3]) / 255.0;
        let da = f32::from(dst.0[3]) / 255.0;
        let out_a = a + da * (1.0 - a);
        if out_a <= 0.0 {
            return;
        }
        for (d, s) in dst.0.iter_mut().zip(c.iter()).take(3) {
            let v = (f32::from(*s) * a + f32::from(*d) * da * (1.0 - a)) / out_a;
            *d = v.clamp(0.0, 255.0) as u8;
        }
        dst.0[3] = (out_a * 255.0).round() as u8;
    }

    /// Overwrites the pixel (no blending).
    pub fn set(&mut self, x: i32, y: i32, c: Px) {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            return;
        }
        self.img.put_pixel(x as u32, y as u32, Rgba(c));
    }

    /// Fills a convex or concave polygon (even-odd rule), sampling pixel centres. `color` is
    /// evaluated per pixel so callers can apply noise / gradients.
    pub fn fill_polygon(&mut self, pts: &[(f32, f32)], mut color: impl FnMut(i32, i32) -> Px) {
        if pts.len() < 3 {
            return;
        }
        let min_y = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min).floor().max(0.0) as i32;
        let max_y = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max).ceil().min(self.height() as f32) as i32;
        let mut xs: Vec<f32> = Vec::new();
        for y in min_y..max_y {
            let cy = y as f32 + 0.5;
            xs.clear();
            for i in 0..pts.len() {
                let (x0, y0) = pts[i];
                let (x1, y1) = pts[(i + 1) % pts.len()];
                if (y0 <= cy && y1 > cy) || (y1 <= cy && y0 > cy) {
                    xs.push(x0 + (cy - y0) / (y1 - y0) * (x1 - x0));
                }
            }
            xs.sort_by(f32::total_cmp);
            for pair in xs.chunks_exact(2) {
                let start = (pair[0] - 0.5).ceil().max(0.0) as i32;
                let end = (pair[1] - 0.5).floor().min(self.width() as f32 - 1.0) as i32;
                for x in start..=end {
                    let c = color(x, y);
                    self.blend(x, y, c);
                }
            }
        }
    }

    /// Fills a polygon with one colour.
    pub fn poly(&mut self, pts: &[(f32, f32)], c: Px) {
        self.fill_polygon(pts, |_, _| c);
    }

    /// Blends an axis-aligned rectangle.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Px) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.blend(xx, yy, c);
            }
        }
    }

    /// Fills an ellipse; `color` receives the pixel and the normalised squared distance
    /// `d ∈ [0, 1]` from the centre.
    pub fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, mut color: impl FnMut(i32, i32, f32) -> Px) {
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let x0 = (cx - rx).floor() as i32;
        let x1 = (cx + rx).ceil() as i32;
        let y0 = (cy - ry).floor() as i32;
        let y1 = (cy + ry).ceil() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let dy = (y as f32 + 0.5 - cy) / ry;
                let d = dx * dx + dy * dy;
                if d <= 1.0 {
                    let c = color(x, y, d);
                    self.blend(x, y, c);
                }
            }
        }
    }

    /// Circular ring between radii `ri` and `ro` around `(cx, cy)` (wheels, hoops).
    pub fn ring(&mut self, cx: f32, cy: f32, ro: f32, ri: f32, c: Px) {
        self.fill_ellipse(cx, cy, ro, ro, |x, y, _| {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            if d >= ri { c } else { [0, 0, 0, 0] }
        });
    }

    /// Ellipse lit from the top-left: lighter towards the upper-left, darker at the lower-right
    /// rim. Reads as a rounded volume (foliage clumps, sacks, stones).
    pub fn lit_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, base: Px) {
        self.fill_ellipse(cx, cy, rx, ry, |x, y, d| {
            let nx = (x as f32 + 0.5 - cx) / rx;
            let ny = (y as f32 + 0.5 - cy) / ry;
            let l = (-nx * 0.6 - ny * 0.8) * 0.5 + 0.5; // 0 (shadow side) … 1 (lit side)
            shade(base, 0.72 + 0.42 * l - 0.12 * d)
        });
    }

    /// 1 px line (Bresenham).
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Px) {
        let (mut x, mut y) = (x0, y0);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.blend(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Line between float endpoints (rounded to pixel centres).
    pub fn linef(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Px) {
        self.line(x0.floor() as i32, y0.floor() as i32, x1.floor() as i32, y1.floor() as i32, c);
    }

    /// Thick line with round caps (a capsule of radius `r`).
    pub fn thick_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, c: Px) {
        let minx = (x0.min(x1) - r).floor() as i32;
        let maxx = (x0.max(x1) + r).ceil() as i32;
        let miny = (y0.min(y1) - r).floor() as i32;
        let maxy = (y0.max(y1) + r).ceil() as i32;
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        for y in miny..=maxy {
            for x in minx..=maxx {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = (((px - x0) * dx + (py - y0) * dy) / len2).clamp(0.0, 1.0);
                let (qx, qy) = (x0 + dx * t - px, y0 + dy * t - py);
                if qx * qx + qy * qy <= r * r {
                    self.blend(x, y, c);
                }
            }
        }
    }

    /// Draws a dark 1 px outline around opaque regions (improves readability when zoomed out).
    pub fn outline(&mut self, c: Px) {
        let src = self.img.clone();
        let (w, h) = (self.width(), self.height());
        let opaque = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && src.get_pixel(x as u32, y as u32).0[3] > 128;
        for y in 0..h {
            for x in 0..w {
                if !opaque(x, y) && (opaque(x - 1, y) || opaque(x + 1, y) || opaque(x, y - 1) || opaque(x, y + 1)) {
                    self.blend(x, y, c);
                }
            }
        }
    }

    /// Returns a copy with a soft outline of radius `r` pixels drawn *behind* the content
    /// (used on supersampled icons before downsampling).
    pub fn outlined(&self, r: i32, c: Px) -> Canvas {
        let (w, h) = (self.width(), self.height());
        let mut out = Canvas::new(w as u32, h as u32);
        let offsets: Vec<(i32, i32)> =
            (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).filter(|(dx, dy)| dx * dx + dy * dy <= r * r).collect();
        for y in 0..h {
            for x in 0..w {
                let a = offsets.iter().map(|(dx, dy)| self.get(x + dx, y + dy)[3]).max().unwrap_or(0);
                if a > 0 {
                    out.set(x, y, with_alpha(c, ((u32::from(a) * u32::from(c[3])) / 255) as u8));
                }
            }
        }
        out.blit(self, 0, 0);
        out
    }

    /// Alpha-composites `src` with its top-left corner at (x, y).
    pub fn blit(&mut self, src: &Canvas, x: i32, y: i32) {
        for sy in 0..src.height() {
            for sx in 0..src.width() {
                let p = src.get(sx, sy);
                if p[3] > 0 {
                    self.blend(x + sx, y + sy, p);
                }
            }
        }
    }

    /// Horizontally mirrored copy.
    pub fn mirrored(&self) -> Canvas {
        let mut out = Canvas::new(self.img.width(), self.img.height());
        let w = self.width();
        for y in 0..self.height() {
            for x in 0..w {
                out.set(w - 1 - x, y, self.get(x, y));
            }
        }
        out
    }

    /// Bounding box `(x, y, w, h)` of pixels with alpha > 0, or `None` for an empty canvas.
    pub fn opaque_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for y in 0..self.height() {
            for x in 0..self.width() {
                if self.get(x, y)[3] > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 <= x1).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    }

    /// Copy of the region `(x, y, w, h)` (pixels outside the canvas are transparent).
    pub fn crop(&self, x: i32, y: i32, w: u32, h: u32) -> Canvas {
        let mut out = Canvas::new(w, h);
        for yy in 0..h as i32 {
            for xx in 0..w as i32 {
                out.set(xx, yy, self.get(x + xx, y + yy));
            }
        }
        out
    }

    /// Area-averaging resample to exactly `w` × `h` pixels with premultiplied alpha, so
    /// transparent pixels do not darken edges. Works for both up- and downscaling.
    pub fn resample(&self, w: u32, h: u32) -> Canvas {
        let mut out = Canvas::new(w.max(1), h.max(1));
        let (sw, sh) = (self.width() as f32, self.height() as f32);
        let (fx, fy) = (sw / w.max(1) as f32, sh / h.max(1) as f32);
        for oy in 0..h.max(1) as i32 {
            let (y0, y1) = (oy as f32 * fy, (oy + 1) as f32 * fy);
            for ox in 0..w.max(1) as i32 {
                let (x0, x1) = (ox as f32 * fx, (ox + 1) as f32 * fx);
                let mut acc = [0.0f32; 4];
                let mut area = 0.0f32;
                let mut sy = y0.floor() as i32;
                while (sy as f32) < y1 {
                    let wy = (y1.min(sy as f32 + 1.0) - y0.max(sy as f32)).max(0.0);
                    let mut sx = x0.floor() as i32;
                    while (sx as f32) < x1 {
                        let wx = (x1.min(sx as f32 + 1.0) - x0.max(sx as f32)).max(0.0);
                        let wgt = wx * wy;
                        let p = self.get(sx, sy);
                        let a = f32::from(p[3]) / 255.0;
                        acc[0] += f32::from(p[0]) * a * wgt;
                        acc[1] += f32::from(p[1]) * a * wgt;
                        acc[2] += f32::from(p[2]) * a * wgt;
                        acc[3] += a * wgt;
                        area += wgt;
                        sx += 1;
                    }
                    sy += 1;
                }
                if area > 0.0 && acc[3] > 0.0 {
                    let a = acc[3] / area;
                    let px = [
                        (acc[0] / acc[3]).round().clamp(0.0, 255.0) as u8,
                        (acc[1] / acc[3]).round().clamp(0.0, 255.0) as u8,
                        (acc[2] / acc[3]).round().clamp(0.0, 255.0) as u8,
                        (a * 255.0).round().clamp(0.0, 255.0) as u8,
                    ];
                    out.set(ox, oy, px);
                }
            }
        }
        out
    }

    /// Crops to the opaque content and scales it (keeping the aspect ratio) to fit inside a
    /// `size` × `size` square with `margin` pixels free on each side, centred.
    pub fn fit_square(&self, size: u32, margin: u32) -> Canvas {
        let mut out = Canvas::new(size, size);
        let Some((x, y, w, h)) = self.opaque_bounds() else {
            return out;
        };
        let inner = size.saturating_sub(margin * 2).max(1) as f32;
        let scale = (inner / w as f32).min(inner / h as f32);
        let nw = ((w as f32 * scale).round() as u32).max(1);
        let nh = ((h as f32 * scale).round() as u32).max(1);
        let scaled = self.crop(x, y, w as u32, h as u32).resample(nw, nh);
        out.blit(&scaled, (size as i32 - nw as i32) / 2, (size as i32 - nh as i32) / 2);
        out
    }

    /// Moves alpha-weighted coverage into a new canvas scaled down by an integer factor.
    pub fn downsample(&self, factor: u32) -> Canvas {
        let f = factor.max(1);
        self.resample(self.img.width() / f, self.img.height() / f)
    }

    /// Fraction of pixels with non-zero alpha (used by tests).
    #[cfg(test)]
    pub fn coverage(&self) -> f32 {
        let total = (self.width() * self.height()).max(1) as f32;
        let n = self.img.pixels().filter(|p| p.0[3] > 0).count() as f32;
        n / total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_in_range() {
        for i in -50..50 {
            let a = hash01(i, i * 7, 3);
            assert_eq!(a, hash01(i, i * 7, 3));
            assert!((0.0..1.0).contains(&a));
        }
    }

    #[test]
    fn periodic_noise_wraps() {
        for k in 0..20 {
            let x = k as f32 * 0.37;
            let y = k as f32 * 0.91;
            assert!((periodic_noise(x, y, 8, 5) - periodic_noise(x + 8.0, y - 16.0, 8, 5)).abs() < 1e-5);
        }
    }

    #[test]
    fn polygon_fill_covers_expected_area() {
        let mut c = Canvas::new(20, 20);
        c.poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], rgb(0xffffff));
        let filled = c.img.pixels().filter(|p| p.0[3] == 255).count();
        assert_eq!(filled, 100);
    }

    #[test]
    fn blend_over_transparent_keeps_colour() {
        let mut c = Canvas::new(1, 1);
        c.blend(0, 0, rgba(0x804020, 128));
        let p = c.get(0, 0);
        assert_eq!(&p[..3], &[0x80, 0x40, 0x20]);
        assert_eq!(p[3], 128);
    }

    #[test]
    fn resample_preserves_coverage_and_colour() {
        let mut c = Canvas::new(8, 8);
        c.fill_rect(0, 0, 4, 8, rgb(0xff0000));
        let s = c.downsample(2);
        assert_eq!(s.width(), 4);
        // Left half opaque red, right half transparent; no dark fringe from transparent pixels.
        assert_eq!(s.get(0, 0), rgb(0xff0000));
        assert_eq!(s.get(3, 0)[3], 0);
        let mut edge = Canvas::new(2, 1);
        edge.set(0, 0, rgb(0x00ff00));
        let e = edge.resample(1, 1);
        assert_eq!(&e.get(0, 0)[..3], &[0, 255, 0]);
        assert!((i32::from(e.get(0, 0)[3]) - 128).abs() <= 1);
    }

    #[test]
    fn mirror_and_bounds() {
        let mut c = Canvas::new(10, 4);
        c.set(1, 2, rgb(0xffffff));
        let m = c.mirrored();
        assert_eq!(m.get(8, 2), rgb(0xffffff));
        assert_eq!(c.opaque_bounds(), Some((1, 2, 1, 1)));
        assert_eq!(Canvas::new(3, 3).opaque_bounds(), None);
    }

    #[test]
    fn fit_square_centres_content() {
        let mut c = Canvas::new(100, 50);
        c.fill_rect(10, 10, 80, 20, rgb(0x123456));
        let f = c.fit_square(48, 2);
        let (x, _, w, _) = f.opaque_bounds().unwrap_or((0, 0, 0, 0));
        assert_eq!(w, 44);
        assert_eq!(x, 2);
    }
}
