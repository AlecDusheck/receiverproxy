//! Sparse white points on true black with a slow twinkle, and a meteor now
//! and then. An LED that is off emits nothing, so one lit pixel sits in real black.

use crate::effects::Effect;
use crate::util::{self, Rng};
use e120_canvas::Frame;
use std::f32::consts::TAU;

struct Star {
    x: i32,
    y: i32,
    level: f32,
    phase: f32,
    rate: f32,
}

struct Meteor {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

pub struct Stars {
    rng: Rng,
    width: u32,
    height: u32,
    points: Vec<Star>,
    meteor: Option<Meteor>,
    /// The meteor's trail, one heat value per pixel, decaying exponentially.
    trail: Vec<f32>,
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Stars::new(width, height, seed))
}

impl Stars {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let count = (width * height / 60).max(1);
        let points = (0..count)
            .map(|_| Star {
                x: rng.below(width) as i32,
                y: rng.below(height) as i32,
                level: rng.range(0.15, 1.0),
                phase: rng.range(0.0, TAU),
                rate: rng.range(0.3, 1.5),
            })
            .collect();
        Self {
            rng,
            width,
            height,
            points,
            meteor: None,
            trail: vec![0.0; (width * height) as usize],
        }
    }

    fn launch(&mut self) -> Meteor {
        let speed = (self.width + self.height) as f32 * 0.6;
        let angle = self.rng.range(0.35, 1.2);
        let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        Meteor {
            x: self.rng.range(0.0, self.width as f32),
            y: -1.0,
            vx: angle.cos() * dir * speed,
            vy: angle.sin() * speed,
        }
    }

    /// Move the meteor by `dt`, marking every pixel it crosses.
    fn fly(&mut self, dt: f32) {
        let Some(m) = &mut self.meteor else {
            return;
        };
        let steps = (m.vx.hypot(m.vy) * dt).ceil().max(1.0) as u32;
        let (sx, sy) = (m.vx * dt / steps as f32, m.vy * dt / steps as f32);
        for _ in 0..steps {
            m.x += sx;
            m.y += sy;
            let inside =
                m.x >= 0.0 && m.y >= 0.0 && (m.x as u32) < self.width && (m.y as u32) < self.height;
            if inside {
                self.trail[(m.y as u32 * self.width + m.x as u32) as usize] = 1.0;
            }
        }
        let gone = m.y > self.height as f32 + 2.0 || m.x < -2.0 || m.x > self.width as f32 + 2.0;
        if gone {
            self.meteor = None;
        }
    }
}

impl Effect for Stars {
    fn step(&mut self, t: f32, dt: f32, out: &mut Frame) {
        // The trail fades with a 0.4 s time constant.
        let decay = (-dt / 0.4).exp();
        for h in &mut self.trail {
            *h *= decay;
        }
        if self.meteor.is_none() && self.rng.chance(0.4 * dt) {
            self.meteor = Some(self.launch());
        }
        self.fly(dt);

        let pixels = out.as_bytes_mut().as_chunks_mut::<3>().0;
        for (px, &h) in pixels.iter_mut().zip(&self.trail) {
            *px = if h > 0.004 {
                [util::level(h * h); 3]
            } else {
                [0; 3]
            };
        }
        for s in &self.points {
            let v = s.level * (0.7 + 0.3 * (s.phase + t * s.rate).sin());
            util::add_pixel(out, s.x, s.y, [util::level(v); 3]);
        }
    }
}
