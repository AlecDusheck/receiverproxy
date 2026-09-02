//! One pixel at full white on a curved path with a long exponential trail
//! that cycles once through the hues, at 240 fps sending only the rows the
//! trail covers. Peak brightness against black; the eye's bloom does the rest.
//! What the panel can follow is set by the card's own scan, not this loop.

use crate::effects::{Effect, Refresh};
use crate::util::{self, Rng};
use e120_canvas::Frame;
use std::f32::consts::TAU;
use std::ops::Range;

/// Heat below this is black and its row is not sent.
const FLOOR: f32 = 0.003;

pub struct Comet {
    width: u32,
    height: u32,
    phase: (f32, f32),
    prev: Option<(f32, f32)>,
    /// One heat value per pixel, decaying exponentially.
    heat: Vec<f32>,
    /// Rows lit by the last frame, so rows that just went dark are sent too.
    lit: Range<u32>,
    rows: Range<u32>,
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Comet::new(width, height, seed))
}

impl Comet {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            width,
            height,
            phase: (rng.range(0.0, TAU), rng.range(0.0, TAU)),
            prev: None,
            heat: vec![0.0; (width * height) as usize],
            lit: 0..0,
            rows: 0..height,
        }
    }

    fn at(&self, t: f32) -> (f32, f32) {
        let (w, h) = (
            (self.width as f32 - 1.0).max(0.0),
            (self.height as f32 - 1.0).max(0.0),
        );
        (
            w * 0.5 + w * 0.45 * (0.9 * t + self.phase.0).sin(),
            h * 0.5 + h * 0.45 * (1.3 * t + self.phase.1).sin(),
        )
    }

    fn deposit(&mut self, (x, y): (f32, f32), v: f32) {
        let (Ok(x), Ok(y)) = (
            u32::try_from(x.round() as i32),
            u32::try_from(y.round() as i32),
        ) else {
            return;
        };
        if x < self.width && y < self.height {
            let i = (y * self.width + x) as usize;
            self.heat[i] = self.heat[i].max(v);
        }
    }

    /// Paint the heat map; returns the band of rows with any light in them.
    fn paint(&self, out: &mut Frame) -> Range<u32> {
        let (mut lo, mut hi) = (self.height, 0);
        for y in 0..self.height {
            let start = (y * self.width) as usize;
            let heat = &self.heat[start..start + self.width as usize];
            for (px, &h) in out.row_mut(y).iter_mut().zip(heat) {
                *px = if h < FLOOR {
                    [0; 3]
                } else if h > 0.97 {
                    [255; 3]
                } else {
                    util::hsv(1.0 - h, ((1.0 - h) * 4.0).min(1.0), h)
                };
            }
            if heat.iter().any(|&h| h >= FLOOR) {
                lo = lo.min(y);
                hi = y + 1;
            }
        }
        lo.min(hi)..hi
    }
}

impl Effect for Comet {
    fn step(&mut self, t: f32, dt: f32, out: &mut Frame) {
        // The trail fades with a 0.5 s time constant.
        let decay = (-dt / 0.5).exp();
        for h in &mut self.heat {
            *h *= decay;
        }
        let cur = self.at(t);
        let prev = self.prev.unwrap_or(cur);
        let n = (cur.0 - prev.0).hypot(cur.1 - prev.1).ceil().max(1.0) as u32;
        for i in 1..=n {
            let k = i as f32 / n as f32;
            let p = (util::lerp(prev.0, cur.0, k), util::lerp(prev.1, cur.1, k));
            self.deposit(p, decay.powf(1.0 - k));
        }
        self.prev = Some(cur);

        let lit = self.paint(out);
        self.rows = lit.start.min(self.lit.start)..lit.end.max(self.lit.end);
        self.lit = lit;
    }

    fn refresh(&self) -> Refresh {
        Refresh {
            rows: Some(self.rows.clone()),
            ..Refresh::default()
        }
    }

    fn fps(&self) -> Option<u32> {
        Some(240)
    }
}
