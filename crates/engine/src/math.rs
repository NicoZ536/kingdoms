//! Small math helpers shared by engine modules.

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Axis-aligned rectangle in world or screen space (`min` inclusive, `max` exclusive).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const EMPTY: Rect =
        Rect { min: Vec2::splat(f32::INFINITY), max: Vec2::splat(f32::NEG_INFINITY) };

    pub fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        Self { min: center - size * 0.5, max: center + size * 0.5 }
    }

    pub fn from_points(a: Vec2, b: Vec2) -> Self {
        Self { min: a.min(b), max: a.max(b) }
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn is_empty(&self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.y >= self.min.y && p.x < self.max.x && p.y < self.max.y
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
    }

    /// Smallest rectangle containing both.
    pub fn union(&self, other: &Rect) -> Rect {
        Rect { min: self.min.min(other.min), max: self.max.max(other.max) }
    }

    pub fn include_point(&mut self, p: Vec2) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    pub fn expanded(&self, margin: f32) -> Rect {
        Rect { min: self.min - Vec2::splat(margin), max: self.max + Vec2::splat(margin) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_and_union() {
        let a = Rect::new(Vec2::ZERO, Vec2::splat(10.0));
        let b = Rect::new(Vec2::splat(5.0), Vec2::splat(15.0));
        let c = Rect::new(Vec2::splat(20.0), Vec2::splat(30.0));
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
        assert_eq!(a.union(&c), Rect::new(Vec2::ZERO, Vec2::splat(30.0)));
        let mut e = Rect::EMPTY;
        e.include_point(Vec2::new(1.0, 2.0));
        e.include_point(Vec2::new(-1.0, 5.0));
        assert_eq!(e, Rect::new(Vec2::new(-1.0, 2.0), Vec2::new(1.0, 5.0)));
    }
}
