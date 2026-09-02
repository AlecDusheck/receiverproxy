//! One bright row sweeping top to bottom fast, then a bright column left to
//! right, at 240 fps sending only the rows that changed. Point a phone camera
//! at it: its rolling shutter slices the sweep into bands the eye never sees.
//! What the panel can follow is set by the card's own scan, not this loop.

use crate::effects::{Effect, Refresh};
use wall::Frame;
use std::ops::Range;

/// Passes per second, in either direction.
const PASSES_PER_SECOND: f32 = 3.0;

pub struct Scanner {
    width: u32,
    height: u32,
    pos: f32,
    column: bool,
    prev: Option<u32>,
    rows: Option<Range<u32>>,
}

pub fn build(width: u32, height: u32, _seed: u64) -> Box<dyn Effect> {
    Box::new(Scanner {
        width,
        height,
        pos: 0.0,
        column: false,
        prev: None,
        rows: None,
    })
}

impl Effect for Scanner {
    fn step(&mut self, _t: f32, dt: f32, out: &mut Frame) {
        out.as_bytes_mut().fill(0);
        let len = if self.column { self.width } else { self.height } as f32;
        if len < 1.0 {
            return;
        }
        self.pos += len * PASSES_PER_SECOND * dt;
        let switched = self.pos >= len;
        if switched {
            self.column = !self.column;
            self.pos = (self.pos - len).clamp(0.0, len - 1.0);
        }
        let i = self.pos as u32;
        if self.column {
            for y in 0..self.height {
                out.set_pixel(i, y, [255; 3]);
            }
        } else if i < out.height {
            out.row_mut(i).fill([255; 3]);
        }
        // The column touches every row; the row phase only its old and new row.
        self.rows = if self.column || switched {
            None
        } else {
            let lo = self.prev.map_or(i, |p| p.min(i));
            let end = self.prev.map_or(i, |p| p.max(i)) + 1;
            Some(lo..end)
        };
        self.prev = Some(i);
    }

    fn refresh(&self) -> Refresh {
        Refresh {
            rows: self.rows.clone(),
            ..Refresh::default()
        }
    }

    fn fps(&self) -> Option<u32> {
        Some(240)
    }
}
