//! Three soft discs, one per channel, drifting and overlapping into additive
//! white. Each pixel is three narrow-band emitters, so the primaries are as saturated as the LEDs.

use crate::effects::Effect;
use crate::util::{self, Rng};
use wall::Frame;
use std::f32::consts::TAU;

/// Drift frequencies in Hz, one `(x, y)` pair per channel.
const DRIFT: [(f32, f32); 3] = [(0.21, 0.17), (0.13, 0.23), (0.19, 0.11)];

pub struct Primaries {
    width: f32,
    height: f32,
    phase: [f32; 3],
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Primaries::new(width, height, seed))
}

impl Primaries {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            width: width as f32,
            height: height as f32,
            phase: [0.0; 3].map(|_| rng.range(0.0, TAU)),
        }
    }

    fn centre(&self, c: usize, t: f32) -> (f32, f32) {
        let (fx, fy) = DRIFT[c];
        let p = self.phase[c];
        (
            (self.width - 1.0) * 0.5 + self.width * 0.3 * (TAU * fx * t + p).sin(),
            (self.height - 1.0) * 0.5 + self.height * 0.3 * (TAU * fy * t + 1.7 * p).cos(),
        )
    }
}

impl Effect for Primaries {
    fn step(&mut self, t: f32, _dt: f32, out: &mut Frame) {
        let r2 = (self.width.min(self.height) * 0.45).powi(2);
        let centres: [(f32, f32); 3] = std::array::from_fn(|c| self.centre(c, t));
        for y in 0..out.height {
            let fy = y as f32;
            for (x, px) in out.row_mut(y).iter_mut().enumerate() {
                let fx = x as f32;
                for (c, &(cx, cy)) in centres.iter().enumerate() {
                    let d2 = (fx - cx).powi(2) + (fy - cy).powi(2);
                    let k = (1.0 - d2 / r2).clamp(0.0, 1.0);
                    px[c] = util::level(k * k);
                }
            }
        }
    }
}
