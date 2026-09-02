//! Value-noise plasma at 1-4% brightness in two slowly mixing hues.
//! Deep dark gradients: there is no backlight floor under an LED's black.

use crate::effects::Effect;
use crate::util::{self, Rng};
use wall::Frame;

pub struct Fog {
    seed: u32,
    /// Lattice units per pixel.
    scale: f32,
    hue_a: f32,
    hue_b: f32,
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Fog::new(width, height, seed))
}

impl Fog {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let hue_a = rng.unit();
        Self {
            seed: rng.next_u64() as u32,
            scale: 6.0 / width.min(height).max(1) as f32,
            hue_a,
            hue_b: hue_a + 0.4,
        }
    }
}

impl Effect for Fog {
    fn step(&mut self, t: f32, _dt: f32, out: &mut Frame) {
        let (hue_a, hue_b) = (self.hue_a + t * 0.008, self.hue_b - t * 0.011);
        let seed = self.seed;
        for y in 0..out.height {
            let fy = y as f32 * self.scale;
            for (x, px) in out.row_mut(y).iter_mut().enumerate() {
                let fx = x as f32 * self.scale;
                let n = 0.65 * util::noise(fx + t * 0.12, fy + t * 0.07, seed)
                    + 0.35
                        * util::noise(2.0 * fx - t * 0.09, 2.0 * fy + t * 0.1, seed ^ 0x5BD1_E995);
                let mix = util::noise(
                    fx * 0.6 + t * 0.04,
                    fy * 0.6 - t * 0.05,
                    seed.wrapping_add(17),
                );
                *px = util::hsv(util::lerp(hue_a, hue_b, mix), 0.85, 0.01 + 0.03 * n);
            }
        }
    }
}
