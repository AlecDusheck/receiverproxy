//! Five to ten single-pixel lights wandering slowly and breathing at a few
//! percent. Low levels stay clean on an LED, so one pixel at 3% is still a light in a dark room.

use crate::effects::Effect;
use crate::util::{self, Rng};
use wall::Frame;
use std::f32::consts::TAU;

struct Fly {
    x: f32,
    y: f32,
    heading: f32,
    speed: f32,
    /// Peak brightness, `0.0..=1.0`.
    peak: f32,
    phase: f32,
    rate: f32,
}

pub struct Fireflies {
    rng: Rng,
    width: f32,
    height: f32,
    flies: Vec<Fly>,
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Fireflies::new(width, height, seed))
}

impl Fireflies {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let count = (width * height / 800).clamp(5, 10);
        let (w, h) = (width as f32, height as f32);
        let flies = (0..count)
            .map(|_| Fly {
                x: rng.range(0.0, w),
                y: rng.range(0.0, h),
                heading: rng.range(0.0, TAU),
                speed: rng.range(1.5, 4.0),
                peak: rng.range(0.02, 0.05),
                phase: rng.range(0.0, TAU),
                rate: TAU * rng.range(0.4, 1.0),
            })
            .collect();
        Self {
            rng,
            width: w,
            height: h,
            flies,
        }
    }
}

impl Effect for Fireflies {
    fn step(&mut self, t: f32, dt: f32, out: &mut Frame) {
        out.as_bytes_mut().fill(0);
        for fly in &mut self.flies {
            fly.heading += self.rng.range(-1.5, 1.5) * dt;
            fly.x = (fly.x + fly.heading.cos() * fly.speed * dt).rem_euclid(self.width.max(1.0));
            fly.y = (fly.y + fly.heading.sin() * fly.speed * dt).rem_euclid(self.height.max(1.0));
            let breath = 0.5 + 0.5 * (fly.phase + t * fly.rate).sin();
            let light = util::scaled([1.0, 0.85, 0.25], fly.peak * breath * breath);
            util::add_pixel(out, fly.x as i32, fly.y as i32, light);
        }
    }
}
