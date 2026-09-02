//! Black, then single-frame white flashes at random intervals through the
//! latch-frame gain, dimmer after-flashes, and a faint branching bolt for two
//! frames. Every pixel goes to full and back in one refresh; the panel lights the room.

use crate::effects::{Effect, Refresh};
use crate::util::Rng;
use e120_canvas::Frame;

/// Frame by frame after the trigger: pixel fill, whether the bolt shows, and
/// the gain. Pixels stay at full; the gain does the dimming.
const SCRIPT: [(u8, bool, u8); 9] = [
    (255, false, 255),
    (0, true, 220),
    (0, true, 100),
    (0, false, 255),
    (255, false, 110),
    (0, false, 255),
    (0, false, 255),
    (255, false, 45),
    (0, false, 255),
];

const BOLT: [u8; 3] = [200, 215, 255];

pub struct Lightning {
    rng: Rng,
    width: u32,
    height: u32,
    next_at: f32,
    /// Index into `SCRIPT` while a strike plays.
    frame: Option<usize>,
    gain: u8,
    /// Bolt pixels; capacity fixed at construction, a bolt that fills it is cut short.
    bolt: Vec<(u32, u32)>,
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Lightning::new(width, height, seed))
}

impl Lightning {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let next_at = rng.range(0.5, 1.5);
        Self {
            rng,
            width,
            height,
            next_at,
            frame: None,
            gain: 255,
            bolt: Vec::with_capacity((height * 6) as usize),
        }
    }

    fn grow_bolt(&mut self) {
        self.bolt.clear();
        let mut x = self.rng.below(self.width) as i32;
        for y in 0..self.height {
            x += self.rng.below(3) as i32 - 1;
            self.mark(x, y);
            if self.rng.chance(0.12) {
                self.branch(x, y);
            }
        }
    }

    fn branch(&mut self, mut x: i32, y: u32) {
        let dir = if self.rng.chance(0.5) { 1 } else { -1 };
        let len = self.rng.below(self.height / 2 + 1);
        for k in 1..=len {
            x += dir + self.rng.below(3) as i32 - 1;
            self.mark(x, y + k);
        }
    }

    fn mark(&mut self, x: i32, y: u32) {
        if self.bolt.len() == self.bolt.capacity() {
            return;
        }
        if let Ok(x) = u32::try_from(x) {
            if x < self.width && y < self.height {
                self.bolt.push((x, y));
            }
        }
    }
}

impl Effect for Lightning {
    fn step(&mut self, t: f32, _dt: f32, out: &mut Frame) {
        if self.frame.is_none() && t >= self.next_at {
            self.grow_bolt();
            self.frame = Some(0);
        }
        let Some(i) = self.frame else {
            out.as_bytes_mut().fill(0);
            self.gain = 255;
            return;
        };
        let (fill, bolt, gain) = SCRIPT[i];
        out.as_bytes_mut().fill(fill);
        if bolt {
            for &(x, y) in &self.bolt {
                out.set_pixel(x, y, BOLT);
            }
        }
        self.gain = gain;
        self.frame = if i + 1 < SCRIPT.len() {
            Some(i + 1)
        } else {
            self.next_at = t + self.rng.range(1.5, 6.0);
            None
        };
    }

    fn refresh(&self) -> Refresh {
        Refresh {
            gain: self.gain,
            ..Refresh::default()
        }
    }
}
