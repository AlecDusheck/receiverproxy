//! A fixed white field tinted through the three channel gains of the latch
//! frame, the hue sweeping once a minute; after the first frame no pixel is
//! sent. Whether the card applies the gains at bytes 38..41 per channel is
//! not yet measured; this is the effect to measure it with.

use crate::effects::{Effect, Refresh};
use crate::util;
use e120_canvas::Frame;

const SWEEP_SECONDS: f32 = 60.0;

pub struct Cast {
    width: f32,
    height: f32,
    drawn: bool,
    held: bool,
    tint: [u8; 3],
}

pub fn build(width: u32, height: u32, _seed: u64) -> Box<dyn Effect> {
    Box::new(Cast {
        width: width as f32,
        height: height as f32,
        drawn: false,
        held: false,
        tint: [255; 3],
    })
}

impl Effect for Cast {
    fn step(&mut self, t: f32, _dt: f32, out: &mut Frame) {
        self.held = self.drawn;
        if !self.drawn {
            let (cx, cy) = ((self.width - 1.0) * 0.5, (self.height - 1.0) * 0.5);
            let r = cx.hypot(cy).max(1.0);
            for y in 0..out.height {
                for (x, px) in out.row_mut(y).iter_mut().enumerate() {
                    let d = (x as f32 - cx).hypot(y as f32 - cy);
                    *px = [util::level(0.5 + 0.5 * (1.0 - d / r)); 3];
                }
            }
            self.drawn = true;
        }
        self.tint = util::hsv(t / SWEEP_SECONDS, 0.6, 1.0);
    }

    fn refresh(&self) -> Refresh {
        Refresh {
            cast: self.tint,
            rows: self.held.then_some(0..0),
            ..Refresh::default()
        }
    }
}
