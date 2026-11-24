//! Deterministic, seedable 2D gradient (Perlin) noise and fractal helpers.

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Classic 2D Perlin noise with a seeded permutation table.
#[derive(Clone, Debug)]
pub struct Perlin {
    perm: [u8; 512],
}

const GRADIENTS: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2),
    (-std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2),
    (std::f32::consts::FRAC_1_SQRT_2, -std::f32::consts::FRAC_1_SQRT_2),
    (-std::f32::consts::FRAC_1_SQRT_2, -std::f32::consts::FRAC_1_SQRT_2),
];

impl Perlin {
    pub fn new(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut p: Vec<u8> = (0..=255).collect();
        for i in (1..256).rev() {
            let j = rng.random_range(0..=i);
            p.swap(i, j);
        }
        let mut perm = [0u8; 512];
        for i in 0..512 {
            perm[i] = p[i & 255];
        }
        Self { perm }
    }

    fn grad(&self, ix: i32, iy: i32) -> (f32, f32) {
        let h = self.perm[(self.perm[(ix & 255) as usize] as i32 + (iy & 255)) as usize & 511];
        GRADIENTS[(h & 7) as usize]
    }

    /// Noise value in roughly `[-1, 1]`.
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let x0 = x.floor();
        let y0 = y.floor();
        let (ix, iy) = (x0 as i32, y0 as i32);
        let (fx, fy) = (x - x0, y - y0);
        let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        let dot = |gx: i32, gy: i32, dx: f32, dy: f32| {
            let g = self.grad(gx, gy);
            g.0 * dx + g.1 * dy
        };
        let n00 = dot(ix, iy, fx, fy);
        let n10 = dot(ix + 1, iy, fx - 1.0, fy);
        let n01 = dot(ix, iy + 1, fx, fy - 1.0);
        let n11 = dot(ix + 1, iy + 1, fx - 1.0, fy - 1.0);
        let u = fade(fx);
        let v = fade(fy);
        let a = n00 + (n10 - n00) * u;
        let b = n01 + (n11 - n01) * u;
        (a + (b - a) * v) * std::f32::consts::SQRT_2
    }

    /// Fractal Brownian motion normalised to roughly `[-1, 1]`.
    pub fn fbm(&self, x: f32, y: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
        let mut sum = 0.0;
        let mut amp = 1.0;
        let mut freq = 1.0;
        let mut norm = 0.0;
        for o in 0..octaves {
            // Offset octaves so they don't share lattice points.
            let off = o as f32 * 17.31;
            sum += self.sample(x * freq + off, y * freq - off) * amp;
            norm += amp;
            amp *= gain;
            freq *= lacunarity;
        }
        sum / norm
    }

    /// Ridged multifractal in `[0, 1]` (sharp crests, good for mountain ranges).
    pub fn ridged(&self, x: f32, y: f32, octaves: u32) -> f32 {
        let mut sum = 0.0;
        let mut amp = 0.5;
        let mut freq = 1.0;
        let mut norm = 0.0;
        for o in 0..octaves {
            let off = o as f32 * 31.7;
            let n = 1.0 - self.sample(x * freq + off, y * freq + off).abs();
            sum += n * n * amp;
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_per_seed() {
        let a = Perlin::new(42);
        let b = Perlin::new(42);
        let c = Perlin::new(43);
        let pa: Vec<f32> = (0..50).map(|i| a.fbm(i as f32 * 0.37, i as f32 * 0.11, 4, 2.0, 0.5)).collect();
        let pb: Vec<f32> = (0..50).map(|i| b.fbm(i as f32 * 0.37, i as f32 * 0.11, 4, 2.0, 0.5)).collect();
        let pc: Vec<f32> = (0..50).map(|i| c.fbm(i as f32 * 0.37, i as f32 * 0.11, 4, 2.0, 0.5)).collect();
        assert_eq!(pa, pb);
        assert_ne!(pa, pc);
    }

    #[test]
    fn range_is_bounded() {
        let p = Perlin::new(7);
        for i in 0..2000 {
            let v = p.sample(i as f32 * 0.173, i as f32 * 0.071);
            assert!((-1.01..=1.01).contains(&v), "{v}");
            let r = p.ridged(i as f32 * 0.05, i as f32 * 0.09, 4);
            assert!((0.0..=1.0).contains(&r));
        }
    }
}
