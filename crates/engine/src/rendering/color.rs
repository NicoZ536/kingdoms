//! Linear RGBA colour type with sRGB conversion helpers.

use serde::{Deserialize, Serialize};

/// A colour in *linear* RGB space with straight (non-premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

impl Color {
    pub const WHITE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::rgba(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
    pub const RED: Color = Color::rgba(1.0, 0.0, 0.0, 1.0);
    pub const GREEN: Color = Color::rgba(0.0, 1.0, 0.0, 1.0);

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    /// Creates a colour from 8-bit sRGB components (as found in image editors / CSS).
    pub fn from_srgb_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: srgb_to_linear(f32::from(r) / 255.0),
            g: srgb_to_linear(f32::from(g) / 255.0),
            b: srgb_to_linear(f32::from(b) / 255.0),
            a: f32::from(a) / 255.0,
        }
    }

    /// Parses `#rrggbb` or `#rrggbbaa` (sRGB).
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        match hex.len() {
            6 => Some(Self::from_srgb_u8(byte(0)?, byte(2)?, byte(4)?, 255)),
            8 => Some(Self::from_srgb_u8(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// Converts to 8-bit sRGB components.
    pub fn to_srgb_u8(self) -> [u8; 4] {
        let q = |c: f32| (linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
        [q(self.r), q(self.g), q(self.b), (self.a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8]
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Component-wise linear interpolation.
    pub fn lerp(self, other: Color, t: f32) -> Self {
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// Multiplies RGB by `f` (alpha unchanged).
    pub fn scaled(self, f: f32) -> Self {
        Self { r: self.r * f, g: self.g * f, b: self.b * f, a: self.a }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_wgpu(self) -> wgpu::Color {
        wgpu::Color { r: self.r as f64, g: self.g as f64, b: self.b as f64, a: self.a as f64 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Color::from_hex("#c8a04b").unwrap();
        assert_eq!(c.to_srgb_u8(), [0xc8, 0xa0, 0x4b, 0xff]);
        assert!(Color::from_hex("#12345").is_none());
    }
}
