//! Tiny isometric 3D rasteriser for building-style sprites.
//!
//! Geometry is described in footprint-local space: `a` runs along tile +x (screen lower-right),
//! `b` along tile +y (screen lower-left), both in tiles, and `z` is the height in screen pixels.
//! The projection matches the engine (`kingdoms_engine::rendering::iso`): one tile is a 64×32
//! diamond. A canvas for a footprint of `sx × sy` tiles (extent along `a` × along `b`) is exactly
//! `(sx + sy) · 32` px wide; its left corner touches the left edge, its front corner
//! (`a = sx`, `b = sy`) sits on the bottom edge at `x = sx · 32`, so the anchor is
//! `(sx / (sx + sy), 1.0)`.
//!
//! Faces are planar polygons with a material. Back faces are culled, so convex solids can be
//! drawn in any face order; separate solids are drawn back to front by the caller. Lighting is
//! fixed and comes from the top-left of the screen: walls facing +b (screen lower-left) are lit,
//! walls facing +a (screen lower-right) are in shade.

use std::f32::consts::TAU;
use std::ops::{Add, Mul, Sub};

use crate::draw::{Canvas, Px, rgba, shade};
use crate::mat::Mat;
use crate::palette;

/// Screen pixels covered by a vertical edge as long as one tile edge (2:1 dimetric, 30°).
pub const Z_PER_TILE: f32 = 39.19;
/// Direction towards the viewer in iso units (`z` converted to tiles).
const VIEW: [f32; 3] = [1.0, 1.0, 32.0 / Z_PER_TILE];
/// Least upward component of a face's unit normal on which snow settles (roof slopes, cone
/// roofs and flat tops; walls are vertical).
const SNOW_MIN_UP: f32 = 0.3;
/// Alpha of the pixels on which snow settles in sprites with a snow mask (docs/assets.md): the
/// engine whitens them with the snow cover (`FLAG_SNOW_ON_ROOF`).
pub const SNOW_ALPHA: u8 = 254;
/// Alpha of the pixels darkened by a painted cast shadow in sprites with a snow mask
/// (docs/assets.md): the engine weakens the shadow like the soft ground shadows, by daylight and
/// weather (`FLAG_SHADOW_MARK`).
pub const SHADOW_ALPHA: u8 = 253;
/// Colour and opacity of painted cast shadows; the engine's shader undoes exactly this blend
/// (keep both in step, docs/assets.md).
pub const SHADOW_RGB: u32 = 0x1a1208;
pub const SHADOW_OPACITY: u8 = 72;

/// A point in footprint-local space (`a`, `b` in tiles, `z` in pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V3 {
    pub a: f32,
    pub b: f32,
    pub z: f32,
}

/// Shorthand constructor for [`V3`].
pub const fn v3(a: f32, b: f32, z: f32) -> V3 {
    V3 { a, b, z }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.a + o.a, self.b + o.b, self.z + o.z)
    }
}

impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.a - o.a, self.b - o.b, self.z - o.z)
    }
}

impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, k: f32) -> V3 {
        v3(self.a * k, self.b * k, self.z * k)
    }
}

/// Which canvas layer primitives draw into. The ground layer is not outlined and lies below
/// all objects; the object layer receives the dark silhouette outline in [`IsoCanvas::finish`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Ground,
    Objects,
}

/// Horizontal axis of a roof ridge or a linear structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Along `a` (tile x, parallel to the lit left-facing walls).
    A,
    /// Along `b` (tile y, parallel to the shaded right-facing walls).
    B,
}

/// A window pane: bounding box `(x0, y0, x1, y1)` in canvas pixels and the glazing colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pane {
    pub bounds: (f32, f32, f32, f32),
    pub color: u32,
}

/// A visible vertical wall plane: `Left` is the plane `b = const` facing +b (lit),
/// `Right` the plane `a = const` facing +a (shaded).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Brightness factor for a unit normal in iso space (light from the screen's top-left).
pub fn light(n: [f32; 3]) -> f32 {
    (0.75 - 0.09 * n[0] + 0.17 * n[1] + 0.25 * n[2]).clamp(0.45, 1.08)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn iso(p: V3) -> [f32; 3] {
    [p.a, p.b, p.z / Z_PER_TILE]
}

/// Gable or hip roof description.
#[derive(Clone, Copy, Debug)]
pub struct Roof {
    pub a0: f32,
    pub b0: f32,
    pub a1: f32,
    pub b1: f32,
    /// Eave height (top of the walls) in pixels.
    pub z: f32,
    /// Height of the ridge above the eaves in pixels.
    pub rise: f32,
    /// Horizontal overhang beyond the walls in tiles.
    pub over: f32,
    pub ridge: Axis,
    pub roof: Mat,
    /// Material of the gable triangles.
    pub gable: Mat,
}

impl Roof {
    /// Roof over the wall rectangle `r = [a0, b0, a1, b1]` with eaves at `z`.
    pub fn new(r: [f32; 4], z: f32, rise: f32, ridge: Axis, roof: Mat, gable: Mat) -> Self {
        let [a0, b0, a1, b1] = r;
        Self { a0, b0, a1, b1, z, rise, over: 0.06, ridge, roof, gable }
    }

    /// Height of the roof surface above (a, b) (gable roofs; used to seat chimneys).
    pub fn height_at(&self, a: f32, b: f32) -> f32 {
        let t = match self.ridge {
            Axis::A => 1.0 - ((b - (self.b0 + self.b1) * 0.5).abs() / ((self.b1 - self.b0) * 0.5)),
            Axis::B => 1.0 - ((a - (self.a0 + self.a1) * 0.5).abs() / ((self.a1 - self.a0) * 0.5)),
        };
        self.z + self.rise * t.clamp(0.0, 1.0)
    }
}

/// Isometric drawing surface for one footprint-sized sprite.
pub struct IsoCanvas {
    ground: Canvas,
    objects: Canvas,
    layer: Layer,
    sx: f32,
    sy: f32,
    ox: f32,
    oy: f32,
    /// Smoke emitter points (chimney tops, vents) in canvas pixels.
    smoke: Vec<(f32, f32)>,
    /// Window panes drawn so far (night lights).
    panes: Vec<Pane>,
    /// Per pixel of the object layer: the colour the last face drawn there gave it if snow
    /// settles on that face (it points up and was not drawn [`IsoCanvas::snow_free`]). `None`
    /// without a snow mask ([`IsoCanvas::with_snow_mask`]).
    snow: Option<Vec<Option<Px>>>,
    /// Per pixel of the ground layer: the colour a cast shadow ([`IsoCanvas::shadow`]) gave it.
    /// Overlapping shadows darken once; [`IsoCanvas::finish`] marks the pixels that still show
    /// that colour with [`SHADOW_ALPHA`] when the canvas has a snow mask.
    shade: Vec<Option<Px>>,
    /// Faces drawn now stay bare in winter (see [`IsoCanvas::snow_free`]).
    bare: bool,
}

impl IsoCanvas {
    /// Canvas for a footprint of `sx` tiles along `a` and `sy` tiles along `b`, with `headroom`
    /// pixels above the back corner for tall structures.
    pub fn new(sx: u32, sy: u32, headroom: u32) -> Self {
        let w = (sx + sy) * 32;
        let h = (sx + sy) * 16 + headroom;
        Self {
            ground: Canvas::new(w, h),
            objects: Canvas::new(w, h),
            layer: Layer::Objects,
            sx: sx as f32,
            sy: sy as f32,
            ox: sy as f32 * 32.0,
            oy: headroom as f32,
            smoke: Vec::new(),
            panes: Vec::new(),
            snow: None,
            shade: vec![None; ((w * h) as usize).max(1)],
            bare: false,
        }
    }

    /// Records the marks of a map sprite: [`IsoCanvas::finish`] gives the object-layer pixels
    /// whose last face points upwards (roofs, flat tops) the alpha [`SNOW_ALPHA`], unless 2D
    /// detail was painted over them afterwards (signs, loaves on a counter, eave lines), and the
    /// ground pixels that still show a cast shadow the alpha [`SHADOW_ALPHA`].
    pub fn with_snow_mask(mut self) -> Self {
        let (w, h) = (self.objects.width(), self.objects.height());
        self.snow = Some(vec![None; (w * h).max(0) as usize]);
        self
    }

    /// Draws `draw` with snow-free faces: warm surfaces (kilns, ovens, furnaces, chimney mouths)
    /// and dark openings stay bare in winter.
    pub fn snow_free(&mut self, draw: impl FnOnce(&mut Self)) {
        let before = std::mem::replace(&mut self.bare, true);
        draw(self);
        self.bare = before;
    }

    /// Footprint extent `(sx, sy)` in tiles.
    pub fn size(&self) -> (f32, f32) {
        (self.sx, self.sy)
    }

    /// Normalised sprite anchor: the front corner of the footprint.
    pub fn anchor(&self) -> (f32, f32) {
        (self.sx / (self.sx + self.sy), 1.0)
    }

    /// Projects a local point to canvas pixel coordinates.
    pub fn project(&self, p: V3) -> (f32, f32) {
        ((p.a - p.b) * 32.0 + self.ox, (p.a + p.b) * 16.0 - p.z + self.oy)
    }

    /// Inverse projection of canvas point `(x, y)` onto the plane at height `z`.
    #[cfg(test)]
    pub fn ground_point(&self, x: f32, y: f32, z: f32) -> (f32, f32) {
        let d = (x - self.ox) / 32.0;
        let s = (y - self.oy + z) / 16.0;
        ((s + d) * 0.5, (s - d) * 0.5)
    }

    /// Records a point where smoke leaves the sprite (chimney top, smoke hole, kiln vent).
    pub fn mark_smoke(&mut self, p: V3) {
        self.smoke.push(self.project(p));
    }

    /// The recorded smoke points as pixel offsets from the anchor (the footprint's front corner;
    /// x to the right, y downwards), rounded to half pixels. Trimming keeps the anchor, so the
    /// offsets hold for the finished sprite.
    pub fn smoke_points(&self) -> Vec<(f32, f32)> {
        let (ax, ay) = self.project(v3(self.sx, self.sy, 0.0));
        let half = |v: f32| (v * 2.0).round() / 2.0;
        self.smoke.iter().map(|&(x, y)| (half(x - ax), half(y - ay))).collect()
    }

    /// The window panes drawn with [`IsoCanvas::window`] or [`IsoCanvas::pane`] (later
    /// geometry may cover parts of them).
    pub fn panes(&self) -> &[Pane] {
        &self.panes
    }

    /// Selects the layer subsequent primitives draw into.
    pub fn set_layer(&mut self, layer: Layer) {
        self.layer = layer;
    }

    /// The canvas of the current layer (for 2D detail drawing).
    pub fn canvas(&mut self) -> &mut Canvas {
        match self.layer {
            Layer::Ground => &mut self.ground,
            Layer::Objects => &mut self.objects,
        }
    }

    /// Draws a planar polygon with material `mat` unless it faces away from the viewer.
    /// `inside` is any point inside the solid the face belongs to (orients the normal).
    /// Returns whether the face was drawn.
    pub fn face(&mut self, poly: &[V3], inside: V3, mat: Mat) -> bool {
        self.face_mul(poly, inside, mat, 1.0)
    }

    /// Like [`IsoCanvas::face`] with an extra brightness multiplier.
    pub fn face_mul(&mut self, poly: &[V3], inside: V3, mat: Mat, mul: f32) -> bool {
        if poly.len() < 3 {
            return false;
        }
        // Newell's method: robust normal for any planar polygon, even with repeated vertices.
        let mut n = [0.0f32; 3];
        for i in 0..poly.len() {
            let p = iso(poly[i]);
            let q = iso(poly[(i + 1) % poly.len()]);
            n[0] += (p[1] - q[1]) * (p[2] + q[2]);
            n[1] += (p[2] - q[2]) * (p[0] + q[0]);
            n[2] += (p[0] - q[0]) * (p[1] + q[1]);
        }
        let len = dot(n, n).sqrt();
        if len < 1e-6 {
            return false;
        }
        n = n.map(|x| x / len);
        let k = poly.len() as f32;
        let cen = poly.iter().map(|p| iso(*p)).fold([0.0; 3], |acc, p| [acc[0] + p[0] / k, acc[1] + p[1] / k, acc[2] + p[2] / k]);
        let ins = iso(inside);
        if dot(n, [cen[0] - ins[0], cen[1] - ins[1], cen[2] - ins[2]]) < 0.0 {
            n = n.map(|x| -x);
        }
        let facing = dot(n, VIEW);
        if facing <= 1e-3 {
            return false;
        }
        let lit = light(n) * mul;

        // Texture frame (see `mat` module docs).
        let horiz = (n[0] * n[0] + n[1] * n[1]).sqrt();
        let (da, db) = if horiz > 1e-3 {
            let d = (-n[1] / horiz, n[0] / horiz);
            if d.0 - d.1 < 0.0 { (-d.0, -d.1) } else { d }
        } else {
            (1.0, 0.0)
        };
        let flat = horiz < 0.05;
        let vertical = n[2].abs() < 0.05;
        let tex = move |p: [f32; 3]| -> (f32, f32) {
            if flat {
                (p[0] * 32.0, p[1] * 32.0)
            } else if vertical {
                ((p[0] * da + p[1] * db) * 32.0, p[2] * Z_PER_TILE)
            } else {
                ((p[0] * da + p[1] * db) * 32.0, -p[2] * Z_PER_TILE / horiz.max(0.2))
            }
        };
        let (mut umin, mut vmin, mut umax, mut vmax) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in poly {
            let (u, v) = tex(iso(*p));
            umin = umin.min(u);
            umax = umax.max(u);
            vmin = vmin.min(v);
            vmax = vmax.max(v);
        }
        let (ulen, vlen) = (umax - umin, vmax - vmin);
        let ao = vertical && poly.iter().map(|p| p.z).fold(f32::MAX, f32::min) <= 0.5;
        let q0 = iso(poly[0]);
        let (ox, oy) = (self.ox, self.oy);
        let pts: Vec<(f32, f32)> = poly.iter().map(|p| self.project(*p)).collect();
        // Object pixels this face covers, for the snow mask.
        let track = self.layer == Layer::Objects && self.snow.is_some();
        let mut covered: Vec<(i32, i32)> = Vec::new();
        self.canvas().fill_polygon(&pts, |x, y| {
            if track {
                covered.push((x, y));
            }
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = (px - ox) / 32.0;
            let s = (py - oy) / 16.0;
            let g = [(s + d) * 0.5, (s - d) * 0.5, 0.0];
            let t = dot(n, [q0[0] - g[0], q0[1] - g[1], q0[2] - g[2]]) / facing;
            let p = [g[0] + t * VIEW[0], g[1] + t * VIEW[1], g[2] + t * VIEW[2]];
            let (u, v) = tex(p);
            let mut s = lit;
            if ao {
                let z = p[2] * Z_PER_TILE;
                if z < 4.0 {
                    s *= 0.8 + 0.05 * z.max(0.0);
                }
            }
            shade(mat.eval(u - umin, v - vmin, ulen, vlen), s)
        });
        let holds_snow = !self.bare && n[2] >= SNOW_MIN_UP;
        let objects = &self.objects;
        let w = objects.width();
        if let Some(mask) = self.snow.as_mut() {
            for (x, y) in covered {
                if let Some(cell) = usize::try_from(y * w + x).ok().and_then(|i| mask.get_mut(i)) {
                    *cell = holds_snow.then(|| objects.get(x, y));
                }
            }
        }
        true
    }

    /// 1 px line between two local points.
    pub fn line3(&mut self, p: V3, q: V3, c: Px) {
        let (x0, y0) = self.project(p);
        let (x1, y1) = self.project(q);
        self.canvas().linef(x0, y0, x1, y1, c);
    }

    /// Draws a 2D sprite with its bottom centre at the projection of `at`.
    pub fn stamp(&mut self, sprite: &Canvas, at: V3) {
        let (x, y) = self.project(at);
        let px = (x - sprite.width() as f32 * 0.5).round() as i32;
        let py = (y - sprite.height() as f32).round() as i32;
        self.canvas().blit(sprite, px, py);
    }

    /// Flat rectangle on the ground layer at height `z` (yards, beds, ponds, plates).
    pub fn ground_rect(&mut self, a0: f32, b0: f32, a1: f32, b1: f32, z: f32, mat: Mat) {
        let prev = self.layer;
        self.layer = Layer::Ground;
        let c = v3((a0 + a1) * 0.5, (b0 + b1) * 0.5, z - 10.0);
        self.face(&[v3(a0, b0, z), v3(a1, b0, z), v3(a1, b1, z), v3(a0, b1, z)], c, mat);
        self.layer = prev;
    }

    /// The building plot: the footprint diamond inset by `inset` tiles.
    pub fn plate(&mut self, inset: f32, mat: Mat) {
        let (sx, sy) = (self.sx, self.sy);
        self.ground_rect(inset, inset, sx - inset, sy - inset, 0.0, mat);
    }

    /// Soft cast shadow of a box `(a0, b0)–(a1, b1)` of height `h` px onto already drawn
    /// ground-layer pixels (light from the top-left → shadow towards the right).
    pub fn shadow(&mut self, a0: f32, b0: f32, a1: f32, b1: f32, h: f32) {
        let k = h / Z_PER_TILE;
        let (oa, ob) = (0.7 * k, -0.2 * k);
        let pts = [
            (a0, b0),
            (a1, b0),
            (a1, b1),
            (a0, b1),
            (a0 + oa, b0 + ob),
            (a1 + oa, b0 + ob),
            (a1 + oa, b1 + ob),
            (a0 + oa, b1 + ob),
        ];
        let hull = convex_hull(&pts);
        let screen: Vec<(f32, f32)> = hull.iter().map(|&(a, b)| self.project(v3(a, b, 0.0))).collect();
        let mut mask = Canvas::new(self.ground.img.width(), self.ground.img.height());
        mask.poly(&screen, rgba(0, 255));
        let shadow = rgba(SHADOW_RGB, SHADOW_OPACITY);
        let w = self.ground.width();
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                let i = (y * w + x) as usize;
                // A pixel already in shadow stays as dark as one shadow makes it.
                if mask.get(x, y)[3] > 0 && self.ground.get(x, y)[3] > 0 && self.shade.get(i).is_some_and(Option::is_none) {
                    self.ground.blend(x, y, shadow);
                    self.shade[i] = Some(self.ground.get(x, y));
                }
            }
        }
    }

    /// Axis-aligned box with one wall material for both visible walls.
    pub fn block(&mut self, min: V3, max: V3, wall: Mat, top: Mat) {
        self.block2(min, max, wall, wall, top);
    }

    /// Axis-aligned box with separate materials for the left (+b) and right (+a) walls.
    pub fn block2(&mut self, min: V3, max: V3, left: Mat, right: Mat, top: Mat) {
        let c = (min + max) * 0.5;
        self.face(&[v3(max.a, min.b, min.z), v3(max.a, max.b, min.z), v3(max.a, max.b, max.z), v3(max.a, min.b, max.z)], c, right);
        self.face(&[v3(min.a, max.b, min.z), v3(max.a, max.b, min.z), v3(max.a, max.b, max.z), v3(min.a, max.b, max.z)], c, left);
        self.face(&[v3(min.a, min.b, max.z), v3(max.a, min.b, max.z), v3(max.a, max.b, max.z), v3(min.a, max.b, max.z)], c, top);
    }

    /// Gable roof (two slopes, triangular gable ends).
    pub fn gable(&mut self, r: Roof) {
        let Roof { a0, b0, a1, b1, z, rise, over, ridge, roof, gable } = r;
        let zt = z + rise;
        match ridge {
            Axis::A => {
                let bm = (b0 + b1) * 0.5;
                let half = (b1 - b0) * 0.5;
                let ze = z - rise * over / half.max(0.01);
                let inside = v3((a0 + a1) * 0.5, bm, z);
                self.face(&[v3(a1, b0, z), v3(a1, b1, z), v3(a1, bm, zt)], inside, gable);
                self.face(&[v3(a0, b0, z), v3(a0, b1, z), v3(a0, bm, zt)], inside, gable);
                let (l, r2) = (a0 - over, a1 + over);
                self.face(&[v3(l, b0 - over, ze), v3(r2, b0 - over, ze), v3(r2, bm, zt), v3(l, bm, zt)], inside, roof);
                self.face(&[v3(l, b1 + over, ze), v3(r2, b1 + over, ze), v3(r2, bm, zt), v3(l, bm, zt)], inside, roof);
                let eave = shade(roof_tone(roof), 0.5);
                self.line3(v3(l, b1 + over, ze), v3(r2, b1 + over, ze), eave);
                self.line3(v3(r2, b1 + over, ze), v3(r2, bm, zt), eave);
            }
            Axis::B => {
                let am = (a0 + a1) * 0.5;
                let half = (a1 - a0) * 0.5;
                let ze = z - rise * over / half.max(0.01);
                let inside = v3(am, (b0 + b1) * 0.5, z);
                self.face(&[v3(a0, b1, z), v3(a1, b1, z), v3(am, b1, zt)], inside, gable);
                self.face(&[v3(a0, b0, z), v3(a1, b0, z), v3(am, b0, zt)], inside, gable);
                let (f, bk) = (b1 + over, b0 - over);
                self.face(&[v3(a0 - over, bk, ze), v3(a0 - over, f, ze), v3(am, f, zt), v3(am, bk, zt)], inside, roof);
                self.face(&[v3(a1 + over, bk, ze), v3(a1 + over, f, ze), v3(am, f, zt), v3(am, bk, zt)], inside, roof);
                let eave = shade(roof_tone(roof), 0.5);
                self.line3(v3(a1 + over, bk, ze), v3(a1 + over, f, ze), eave);
                self.line3(v3(a0 - over, f, ze), v3(am, f, zt), eave);
            }
        }
    }

    /// Hip roof (four slopes); a square base yields a pyramid. `r.ridge` and `r.gable` are
    /// ignored – the ridge runs along the longer side.
    pub fn hip(&mut self, r: Roof) {
        let Roof { a0, b0, a1, b1, z, rise, over, roof, .. } = r;
        let zt = z + rise;
        let (la, lb) = (a1 - a0, b1 - b0);
        let (am, bm) = ((a0 + a1) * 0.5, (b0 + b1) * 0.5);
        let half = la.min(lb) * 0.5;
        let ze = z - rise * over / half.max(0.01);
        let (r0, r1) = if la >= lb { (v3(a0 + half, bm, zt), v3(a1 - half, bm, zt)) } else { (v3(am, b0 + half, zt), v3(am, b1 - half, zt)) };
        let (l, rr, bk, f) = (a0 - over, a1 + over, b0 - over, b1 + over);
        let inside = v3(am, bm, z);
        let (c00, c10, c11, c01) = (v3(l, bk, ze), v3(rr, bk, ze), v3(rr, f, ze), v3(l, f, ze));
        if la >= lb {
            self.face(&[c00, c10, r1, r0], inside, roof);
            self.face(&[c00, c01, r0], inside, roof);
            self.face(&[c10, c11, r1], inside, roof);
            self.face(&[c01, c11, r1, r0], inside, roof);
        } else {
            self.face(&[c00, c01, r1, r0], inside, roof);
            self.face(&[c00, c10, r0], inside, roof);
            self.face(&[c01, c11, r1], inside, roof);
            self.face(&[c10, c11, r1, r0], inside, roof);
        }
        let eave = shade(roof_tone(roof), 0.5);
        self.line3(c01, c11, eave);
        self.line3(c11, c10, eave);
        self.line3(c11, r1, eave);
    }

    /// Vertical cylinder.
    pub fn cylinder(&mut self, ca: f32, cb: f32, r: f32, z0: f32, z1: f32, mat: Mat) {
        self.frustum(ca, cb, r, r, z0, z1, mat);
    }

    /// Truncated cone from radius `r0` at `z0` to `r1` at `z1` (tiles / pixels), with top cap.
    #[allow(clippy::too_many_arguments)]
    pub fn frustum(&mut self, ca: f32, cb: f32, r0: f32, r1: f32, z0: f32, z1: f32, mat: Mat) {
        self.tube(ca, cb, r0, r1, z0, z1, mat);
        self.disc(ca, cb, r1, z1, mat);
    }

    /// Side surface of a truncated cone without caps (barrel hoops, open towers).
    #[allow(clippy::too_many_arguments)]
    pub fn tube(&mut self, ca: f32, cb: f32, r0: f32, r1: f32, z0: f32, z1: f32, mat: Mat) {
        let n = segments(r0.max(r1));
        let inside = v3(ca, cb, (z0 + z1) * 0.5);
        for i in 0..n {
            let (t0, t1) = (TAU * i as f32 / n as f32, TAU * (i + 1) as f32 / n as f32);
            let p = |t: f32, r: f32, z: f32| v3(ca + r * t.cos(), cb + r * t.sin(), z);
            self.face(&[p(t0, r0, z0), p(t1, r0, z0), p(t1, r1, z1), p(t0, r1, z1)], inside, mat);
        }
    }

    /// Cone with base radius `r` at `z0` and apex at height `z_apex`.
    pub fn cone(&mut self, ca: f32, cb: f32, r: f32, z0: f32, z_apex: f32, mat: Mat) {
        let n = segments(r);
        let inside = v3(ca, cb, z0 + (z_apex - z0) * 0.2);
        let apex = v3(ca, cb, z_apex);
        for i in 0..n {
            let (t0, t1) = (TAU * i as f32 / n as f32, TAU * (i + 1) as f32 / n as f32);
            let p = |t: f32| v3(ca + r * t.cos(), cb + r * t.sin(), z0);
            self.face(&[p(t0), p(t1), apex], inside, mat);
        }
    }

    /// Half-ellipsoid dome of radius `r` (tiles) and height `h` (px) on `z0`.
    pub fn dome(&mut self, ca: f32, cb: f32, r: f32, z0: f32, h: f32, mat: Mat) {
        let n = segments(r);
        let rings = 6;
        let inside = v3(ca, cb, z0 + h * 0.2);
        for j in 0..rings {
            let (p0, p1) = (
                std::f32::consts::FRAC_PI_2 * j as f32 / rings as f32,
                std::f32::consts::FRAC_PI_2 * (j + 1) as f32 / rings as f32,
            );
            for i in 0..n {
                let (t0, t1) = (TAU * i as f32 / n as f32, TAU * (i + 1) as f32 / n as f32);
                let p = |t: f32, phi: f32| v3(ca + r * phi.cos() * t.cos(), cb + r * phi.cos() * t.sin(), z0 + h * phi.sin());
                self.face(&[p(t0, p0), p(t1, p0), p(t1, p1), p(t0, p1)], inside, mat);
            }
        }
    }

    /// Horizontal disc (top cap of a cylinder, a pond, a well opening).
    pub fn disc(&mut self, ca: f32, cb: f32, r: f32, z: f32, mat: Mat) {
        let n = segments(r);
        let poly: Vec<V3> = (0..n).map(|i| {
            let t = TAU * i as f32 / n as f32;
            v3(ca + r * t.cos(), cb + r * t.sin(), z)
        }).collect();
        self.face(&poly, v3(ca, cb, z - 10.0), mat);
    }

    /// Rectangle on a visible wall plane (doors, windows, signs). `c0..c1` is the range along
    /// the wall (`a` for [`Side::Left`], `b` for [`Side::Right`]).
    #[allow(clippy::too_many_arguments)]
    pub fn on_wall(&mut self, side: Side, plane: f32, c0: f32, c1: f32, z0: f32, z1: f32, mat: Mat) {
        let e = 0.004;
        match side {
            Side::Left => {
                let b = plane + e;
                let inside = v3((c0 + c1) * 0.5, plane - 1.0, (z0 + z1) * 0.5);
                self.face(&[v3(c0, b, z0), v3(c1, b, z0), v3(c1, b, z1), v3(c0, b, z1)], inside, mat);
            }
            Side::Right => {
                let a = plane + e;
                let inside = v3(plane - 1.0, (c0 + c1) * 0.5, (z0 + z1) * 0.5);
                self.face(&[v3(a, c0, z0), v3(a, c1, z0), v3(a, c1, z1), v3(a, c0, z1)], inside, mat);
            }
        }
    }

    /// Glazing on a visible wall plane (like [`IsoCanvas::on_wall`] with a flat `color`),
    /// recorded as a window pane that lights up at night.
    #[allow(clippy::too_many_arguments)]
    pub fn pane(&mut self, side: Side, plane: f32, c0: f32, c1: f32, z0: f32, z1: f32, color: u32) {
        self.on_wall(side, plane, c0, c1, z0, z1, Mat::Flat(crate::draw::rgb(color)));
        let p = |cc: f32, z: f32| match side {
            Side::Left => v3(cc, plane, z),
            Side::Right => v3(plane, cc, z),
        };
        let corners = [p(c0, z0), p(c1, z0), p(c1, z1), p(c0, z1)].map(|q| self.project(q));
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in corners {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        self.panes.push(Pane { bounds: (x0, y0, x1, y1), color });
    }

    /// Small window centred at `c` along the wall: frame, dark glazing, mullion and a glint.
    #[allow(clippy::too_many_arguments)]
    pub fn window(&mut self, side: Side, plane: f32, c: f32, z0: f32, w: f32, h: f32) {
        let frame = Mat::Flat(crate::draw::rgb(palette::TIMBER_DARK));
        self.on_wall(side, plane, c - w * 0.5 - 0.025, c + w * 0.5 + 0.025, z0 - 1.0, z0 + h + 1.0, frame);
        self.pane(side, plane, c - w * 0.5, c + w * 0.5, z0, z0 + h, palette::WINDOW);
        let p = |cc: f32, z: f32| match side {
            Side::Left => v3(cc, plane + 0.01, z),
            Side::Right => v3(plane + 0.01, cc, z),
        };
        let fc = shade(crate::draw::rgb(palette::TIMBER_DARK), 1.1);
        self.line3(p(c, z0), p(c, z0 + h - 1.0), fc);
        let glint = rgba(0xbcd3e0, 200);
        let (gx, gy) = self.project(p(c - w * 0.3, z0 + h - 1.0));
        self.canvas().blend(gx as i32, gy as i32, glint);
    }

    /// Plank door centred at `c` along the wall, `w` tiles wide and `h` px high.
    pub fn door(&mut self, side: Side, plane: f32, c: f32, w: f32, h: f32) {
        let frame = Mat::Flat(crate::draw::rgb(palette::TIMBER_DARK));
        self.on_wall(side, plane, c - w * 0.5 - 0.03, c + w * 0.5 + 0.03, 0.0, h + 1.5, frame);
        self.on_wall(side, plane, c - w * 0.5, c + w * 0.5, 0.0, h, Mat::Boards(crate::draw::rgb(palette::DOOR)));
    }

    /// Merlons along the edges of the rectangle `(a0, b0)–(a1, b1)` standing on height `z`.
    #[allow(clippy::too_many_arguments)]
    pub fn crenellations(&mut self, a0: f32, b0: f32, a1: f32, b1: f32, z: f32, size: f32, h: f32, mat: Mat) {
        let top = Mat::Flat(shade(mat_tone(mat), 1.05));
        let step = size * 2.0;
        let along = |from: f32, to: f32| -> Vec<f32> {
            let n = ((to - from) / step).floor().max(1.0) as usize;
            let gap = (to - from - size) / (n as f32).max(1.0);
            (0..=n).map(|i| from + gap * i as f32).collect()
        };
        // Back edges first, then front edges.
        for &a in &along(a0, a1) {
            self.block(v3(a, b0, z), v3(a + size, b0 + size, z + h), mat, top);
        }
        for &b in &along(b0, b1) {
            self.block(v3(a0, b, z), v3(a0 + size, b + size, z + h), mat, top);
        }
        for &b in &along(b0, b1) {
            self.block(v3(a1 - size, b, z), v3(a1, b + size, z + h), mat, top);
        }
        for &a in &along(a0, a1) {
            self.block(v3(a, b1 - size, z), v3(a + size, b1, z + h), mat, top);
        }
    }

    /// Outlines the object layer, composites it over the ground layer and returns the sprite
    /// together with its anchor.
    pub fn finish(self) -> (Canvas, (f32, f32)) {
        let anchor = self.anchor();
        let mut obj = self.objects;
        obj.outline(rgba(palette::OUTLINE, 235));
        let mut out = self.ground;
        out.blit(&obj, 0, 0);
        if let Some(mask) = &self.snow {
            let w = out.width();
            for (i, face) in mask.iter().enumerate() {
                let Some(face) = face else { continue };
                let (x, y) = (i as i32 % w, i as i32 / w);
                let mut px = out.get(x, y);
                // Only where the face still shows: 2D detail painted over it stays bare.
                if px[3] == 255 && px == *face {
                    px[3] = SNOW_ALPHA;
                    out.set(x, y, px);
                }
            }
            // Cast shadows where nothing was drawn over them since.
            for (i, shaded) in self.shade.iter().enumerate() {
                let Some(shaded) = shaded else { continue };
                let (x, y) = (i as i32 % w, i as i32 / w);
                let mut px = out.get(x, y);
                if px[3] == 255 && px == *shaded {
                    px[3] = SHADOW_ALPHA;
                    out.set(x, y, px);
                }
            }
        }
        (out, anchor)
    }

    /// Like [`IsoCanvas::finish`] but removes empty rows above the content (keeping one), so
    /// generous headroom costs no atlas space. Width and the bottom edge – and thus the
    /// anchor – are unchanged, and the footprint diamond is never cut.
    pub fn finish_trimmed(self) -> (Canvas, (f32, f32)) {
        let diamond_top = self.oy as i32;
        let (c, anchor) = self.finish();
        let top = c.opaque_bounds().map_or(0, |(_, y, _, _)| (y - 1).max(0)).min(diamond_top);
        let h = (c.height() - top).max(1) as u32;
        (c.crop(0, top, c.img.width(), h), anchor)
    }
}

/// Removes the marks of a sprite made with a snow mask (alpha [`SNOW_ALPHA`] and
/// [`SHADOW_ALPHA`] back to opaque), for uses outside the map such as build-menu icons.
pub fn clear_snow_marks(c: &mut Canvas) {
    for y in 0..c.height() {
        for x in 0..c.width() {
            let mut px = c.get(x, y);
            if px[3] == SNOW_ALPHA || px[3] == SHADOW_ALPHA {
                px[3] = 255;
                c.set(x, y, px);
            }
        }
    }
}

/// Number of facets for a round shape of radius `r` tiles.
fn segments(r: f32) -> usize {
    ((r * 70.0) as usize).clamp(10, 32)
}

/// Representative colour of a material (for eave lines, merlon tops).
pub fn mat_tone(m: Mat) -> Px {
    match m {
        Mat::Flat(c)
        | Mat::Rough(c)
        | Mat::Boards(c)
        | Mat::BoardsAcross(c)
        | Mat::Logs(c)
        | Mat::Stone(c)
        | Mat::Brick(c)
        | Mat::Thatch(c)
        | Mat::Tiles(c)
        | Mat::Slate(c)
        | Mat::Shingles(c)
        | Mat::Cobbles(c)
        | Mat::Earth(c)
        | Mat::Grass(c)
        | Mat::Water(c)
        | Mat::EndGrain(c)
        | Mat::PlankStack(c)
        | Mat::Rock(c)
        | Mat::Stripes(c, _) => c,
        Mat::Timber { plaster, .. } => plaster,
    }
}

fn roof_tone(m: Mat) -> Px {
    mat_tone(m)
}

/// Convex hull (monotone chain) of 2D points, counter-clockwise; degenerate input yields the
/// unique points.
pub fn convex_hull(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut p: Vec<(f32, f32)> = pts.to_vec();
    p.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let cross = |o: (f32, f32), a: (f32, f32), b: (f32, f32)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut lower: Vec<(f32, f32)> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<(f32, f32)> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Returns `true` when the pixel is (at least partly) opaque (test helper).
#[cfg(test)]
pub fn opaque(c: &Canvas, x: i32, y: i32) -> bool {
    c.get(x, y)[3] > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::rgb;

    #[test]
    fn footprint_corners_land_on_canvas_edges() {
        for (sx, sy) in [(1u32, 1u32), (2, 2), (3, 2), (4, 3), (5, 5)] {
            let ic = IsoCanvas::new(sx, sy, 40);
            let w = ((sx + sy) * 32) as f32;
            let h = ((sx + sy) * 16 + 40) as f32;
            assert_eq!(ic.project(v3(sx as f32, sy as f32, 0.0)), (sx as f32 * 32.0, h), "front corner");
            assert_eq!(ic.project(v3(0.0, sy as f32, 0.0)).0, 0.0, "left corner");
            assert_eq!(ic.project(v3(sx as f32, 0.0, 0.0)).0, w, "right corner");
            assert_eq!(ic.project(v3(0.0, 0.0, 40.0)).1, 0.0, "headroom");
            let (ax, ay) = ic.anchor();
            assert!((ax - sx as f32 / (sx + sy) as f32).abs() < 1e-6 && ay == 1.0);
        }
    }

    #[test]
    fn ground_point_inverts_projection() {
        let ic = IsoCanvas::new(3, 2, 30);
        for &(a, b, z) in &[(0.5, 0.5, 0.0), (2.2, 1.7, 12.0), (0.0, 2.0, 5.0)] {
            let (x, y) = ic.project(v3(a, b, z));
            let (ra, rb) = ic.ground_point(x, y, z);
            assert!((ra - a).abs() < 1e-4 && (rb - b).abs() < 1e-4);
        }
    }

    #[test]
    fn back_faces_are_culled() {
        let mut ic = IsoCanvas::new(2, 2, 30);
        let inside = v3(1.0, 1.0, 10.0);
        // Wall facing -a (away from the viewer).
        let back = [v3(0.5, 0.5, 0.0), v3(0.5, 1.5, 0.0), v3(0.5, 1.5, 20.0), v3(0.5, 0.5, 20.0)];
        assert!(!ic.face(&back, inside, Mat::Flat(rgb(0xff0000))));
        // Wall facing +a (towards the viewer).
        let front = [v3(1.5, 0.5, 0.0), v3(1.5, 1.5, 0.0), v3(1.5, 1.5, 20.0), v3(1.5, 0.5, 20.0)];
        assert!(ic.face(&front, inside, Mat::Flat(rgb(0xff0000))));
    }

    #[test]
    fn lit_left_walls_are_brighter_than_right_walls() {
        assert!(light([0.0, 1.0, 0.0]) > light([1.0, 0.0, 0.0]) + 0.15);
        assert!(light([0.0, 0.0, 1.0]) >= light([0.0, 1.0, 0.0]));
    }

    #[test]
    fn block_and_finish_produce_outlined_sprite() {
        let mut ic = IsoCanvas::new(1, 1, 30);
        ic.plate(0.0, Mat::Flat(rgb(0x806040)));
        ic.block(v3(0.3, 0.3, 0.0), v3(0.7, 0.7, 16.0), Mat::Flat(rgb(0xc0c0c0)), Mat::Flat(rgb(0xe0e0e0)));
        let (c, anchor) = ic.finish();
        assert_eq!((c.width(), c.height()), (64, 62));
        assert_eq!(anchor, (0.5, 1.0));
        // Bottom corner of the plate is drawn, top-left canvas corner stays empty.
        assert!(opaque(&c, 32, 61));
        assert!(!opaque(&c, 0, 0));
    }

    #[test]
    fn snow_mask_marks_the_faces_that_point_up() {
        let draw = |mask: bool| {
            let mut ic = IsoCanvas::new(2, 2, 60);
            if mask {
                ic = ic.with_snow_mask();
            }
            ic.plate(0.0, Mat::Flat(rgb(0x806040)));
            ic.block(v3(0.4, 0.4, 0.0), v3(1.6, 1.6, 20.0), Mat::Flat(rgb(0xc0b090)), Mat::Flat(rgb(0xc0b090)));
            ic.gable(Roof::new([0.4, 0.4, 1.6, 1.6], 20.0, 18.0, Axis::A, Mat::Flat(rgb(0xa04030)), Mat::Flat(rgb(0xc0b090))));
            let roof = ic.project(v3(1.0, 1.3, 30.0));
            let wall = ic.project(v3(1.6, 1.0, 8.0));
            let ground = ic.project(v3(1.9, 1.9, 0.0));
            (ic.finish().0, [roof, wall, ground].map(|(x, y)| (x as i32, y as i32)))
        };
        let (c, [roof, wall, ground]) = draw(true);
        assert_eq!(c.get(roof.0, roof.1)[3], SNOW_ALPHA, "the roof slope takes snow");
        assert_eq!(c.get(wall.0, wall.1)[3], 255, "walls stay bare");
        assert_eq!(c.get(ground.0, ground.1)[3], 255, "the ground layer stays as it is");
        // Without a mask the sprite is fully opaque, and the colours are the same either way.
        let (plain, _) = draw(false);
        assert_eq!(plain.get(roof.0, roof.1)[3], 255);
        for y in 0..c.height() {
            for x in 0..c.width() {
                let (a, b) = (c.get(x, y), plain.get(x, y));
                assert_eq!(a[..3], b[..3]);
                assert!(a[3] == b[3] || (a[3] == SNOW_ALPHA && b[3] == 255));
            }
        }
    }

    #[test]
    fn cast_shadows_are_marked_once_and_only_where_they_show() {
        let draw = |mask: bool| {
            let mut ic = IsoCanvas::new(3, 3, 60);
            if mask {
                ic = ic.with_snow_mask();
            }
            ic.plate(0.0, Mat::Flat(rgb(0x708040)));
            // Two overlapping shadows, then a box standing in one of them.
            ic.shadow(0.4, 0.4, 1.2, 1.2, 30.0);
            ic.shadow(0.8, 0.4, 1.6, 1.2, 30.0);
            ic.block(v3(0.4, 0.4, 0.0), v3(1.2, 1.2, 30.0), Mat::Flat(rgb(0xc0b090)), Mat::Flat(rgb(0xa04030)));
            let (shaded, lit) = (ic.project(v3(1.9, 1.0, 0.0)), ic.project(v3(2.6, 2.6, 0.0)));
            (ic.finish().0, [shaded, lit].map(|(x, y)| (x as i32, y as i32)))
        };
        let (c, [shaded, lit]) = draw(true);
        let plate = c.get(lit.0, lit.1);
        assert_eq!(plate[3], 255, "lit ground is not marked");
        let dark = c.get(shaded.0, shaded.1);
        assert_eq!(dark[3], SHADOW_ALPHA, "the shadow is marked");
        // Exactly one blend of the shadow colour over the plate, even where shadows overlap.
        let mut once = Canvas::new(1, 1);
        once.set(0, 0, rgb(0x708040));
        once.blend(0, 0, rgba(SHADOW_RGB, SHADOW_OPACITY));
        let mut marked = 0;
        let (plain, _) = draw(false);
        for y in 0..c.height() {
            for x in 0..c.width() {
                let (a, b) = (c.get(x, y), plain.get(x, y));
                assert_eq!(a[..3], b[..3], "the colours do not depend on the marks");
                if a[3] == SHADOW_ALPHA {
                    assert_eq!(a[..3], once.get(0, 0)[..3], "({x}, {y}) is darkened once");
                    assert_eq!(b[3], 255);
                    marked += 1;
                }
            }
        }
        assert!(marked > 50, "{marked} shadow pixels");
        // Icons drop the marks.
        let mut icon = c.clone();
        clear_snow_marks(&mut icon);
        assert!((0..icon.height()).all(|y| (0..icon.width()).all(|x| icon.get(x, y)[3] != SHADOW_ALPHA)));
    }

    #[test]
    fn snow_stays_off_warm_faces_and_painted_detail() {
        let mut ic = IsoCanvas::new(2, 2, 60).with_snow_mask();
        ic.block(v3(0.2, 0.2, 0.0), v3(0.9, 0.9, 10.0), Mat::Flat(rgb(0x806040)), Mat::Flat(rgb(0xa08060)));
        ic.snow_free(|ic| ic.block(v3(1.1, 1.1, 0.0), v3(1.8, 1.8, 10.0), Mat::Flat(rgb(0x806040)), Mat::Flat(rgb(0xa08060))));
        let cold = ic.project(v3(0.55, 0.55, 10.0));
        let warm = ic.project(v3(1.45, 1.45, 10.0));
        // A loaf painted on the cold top afterwards.
        let loaf = ic.project(v3(0.4, 0.55, 10.0));
        ic.canvas().set(loaf.0 as i32, loaf.1 as i32, rgb(0xc8843a));
        let (c, _) = ic.finish();
        assert_eq!(c.get(cold.0 as i32, cold.1 as i32)[3], SNOW_ALPHA, "flat tops take snow");
        assert_eq!(c.get(warm.0 as i32, warm.1 as i32)[3], 255, "warm faces stay bare");
        assert_eq!(c.get(loaf.0 as i32, loaf.1 as i32), rgb(0xc8843a), "painted detail stays bare");
    }

    #[test]
    fn hull_of_square_with_inner_point() {
        let h = convex_hull(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.5, 0.5)]);
        assert_eq!(h.len(), 4);
    }
}
