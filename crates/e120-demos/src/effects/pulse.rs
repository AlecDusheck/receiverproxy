//! A fixed soft disc breathing through the latch-frame gain alone; after the
//! first frame no pixel is sent. The gain is a hardware dimmer: the whole panel
//! follows within one refresh with the picture untouched.

use crate::effects::{Effect, Refresh};
use crate::util;
use e120_canvas::Frame;
use std::f32::consts::TAU;

const PERIOD: f32 = 4.0;

pub struct Pulse {
    width: f32,
    height: f32,
    drawn: bool,
    /// Whether this step could skip the pixels.
    held: bool,
    gain: u8,
}

pub fn build(width: u32, height: u32, _seed: u64) -> Box<dyn Effect> {
    Box::new(Pulse {
        width: width as f32,
        height: height as f32,
        drawn: false,
        held: false,
        gain: 0,
    })
}

impl Effect for Pulse {
    fn step(&mut self, t: f32, _dt: f32, out: &mut Frame) {
        self.held = self.drawn;
        if !self.drawn {
            let (cx, cy) = ((self.width - 1.0) * 0.5, (self.height - 1.0) * 0.5);
            let r = self.width.min(self.height) * 0.5;
            for y in 0..out.height {
                for (x, px) in out.row_mut(y).iter_mut().enumerate() {
                    let d = (x as f32 - cx).hypot(y as f32 - cy);
                    let v = (1.0 - d / r.max(1.0)).clamp(0.0, 1.0);
                    *px = util::scaled([1.0, 0.9, 0.7], v);
                }
            }
            self.drawn = true;
        }
        let breath = 0.5 - 0.5 * (TAU * t / PERIOD).cos();
        self.gain = util::level(breath * breath);
    }

    fn refresh(&self) -> Refresh {
        Refresh {
            gain: self.gain,
            rows: self.held.then_some(0..0),
            ..Refresh::default()
        }
    }
}
