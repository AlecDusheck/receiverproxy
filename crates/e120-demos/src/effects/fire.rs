//! The cooling-map fire, palette from deep red through orange to near white.
//! Pure red at low duty stays red on an LED rather than going grey.

use crate::effects::Effect;
use crate::util::{self, Rng};
use e120_canvas::Frame;

/// Simulation ticks per second; the classic effect is stepped per frame.
const TICK_HZ: f32 = 30.0;

pub struct Fire {
    rng: Rng,
    width: usize,
    height: usize,
    /// Cooling per row, scaled so the flames reach about half way up.
    cool: u32,
    heat: Vec<u8>,
    acc: f32,
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Fire::new(width, height, seed))
}

impl Fire {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
            width: width as usize,
            height: height as usize,
            cool: (3 * 255 / height.max(1)).max(2),
            heat: vec![0; (width * height) as usize],
            acc: 0.0,
        }
    }

    fn tick(&mut self) {
        let (w, h) = (self.width, self.height);
        if w == 0 || h == 0 {
            return;
        }
        // Fuel along the bottom row.
        let base = (h - 1) * w;
        for cell in &mut self.heat[base..] {
            *cell = 160 + self.rng.below(96) as u8;
        }
        for y in 0..h - 1 {
            let below = (y + 1) * w;
            let far = (y + 2).min(h - 1) * w;
            for x in 0..w {
                let l = below + x.saturating_sub(1);
                let r = below + (x + 1).min(w - 1);
                let sum = u32::from(self.heat[l])
                    + u32::from(self.heat[below + x])
                    + u32::from(self.heat[r])
                    + u32::from(self.heat[far + x]);
                let cooled = (sum / 4).saturating_sub(self.rng.below(self.cool));
                self.heat[y * w + x] = cooled.min(255) as u8;
            }
        }
    }
}

fn palette(heat: u8) -> [u8; 3] {
    let v = f32::from(heat) / 255.0;
    let (r, g, b) = if v < 0.4 {
        (v / 0.4 * 0.75, 0.0, 0.0)
    } else if v < 0.75 {
        let u = (v - 0.4) / 0.35;
        (0.75 + 0.25 * u, 0.5 * u, 0.0)
    } else {
        let u = (v - 0.75) / 0.25;
        (1.0, 0.5 + 0.45 * u, 0.8 * u)
    };
    [util::level(r), util::level(g), util::level(b)]
}

impl Effect for Fire {
    fn step(&mut self, _t: f32, dt: f32, out: &mut Frame) {
        self.acc += dt * TICK_HZ;
        let ticks = (self.acc as u32).min(4);
        self.acc -= ticks as f32;
        for _ in 0..ticks {
            self.tick();
        }
        let pixels = out.as_bytes_mut().as_chunks_mut::<3>().0;
        for (px, &h) in pixels.iter_mut().zip(&self.heat) {
            *px = palette(h);
        }
    }
}
