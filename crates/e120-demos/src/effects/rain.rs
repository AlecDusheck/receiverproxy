//! Falling 3x5 glyphs, a white head and a green tail fading to nothing.
//! The head sits on black that emits nothing, so the contrast is the LED's own.

use crate::effects::Effect;
use crate::util::{self, Rng};
use e120_canvas::Frame;

const CELL_W: u32 = 4;
const CELL_H: u32 = 6;

/// 3x5 glyphs, one byte per row, bit 2 the left column.
const FONT: [[u8; 5]; 24] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
    [0b010, 0b101, 0b111, 0b101, 0b101],
    [0b111, 0b100, 0b100, 0b100, 0b111],
    [0b111, 0b100, 0b111, 0b100, 0b111],
    [0b111, 0b100, 0b111, 0b100, 0b100],
    [0b101, 0b101, 0b111, 0b101, 0b101],
    [0b001, 0b001, 0b001, 0b101, 0b111],
    [0b100, 0b100, 0b100, 0b100, 0b111],
    [0b111, 0b101, 0b111, 0b100, 0b100],
    [0b101, 0b101, 0b101, 0b101, 0b111],
    [0b101, 0b101, 0b010, 0b010, 0b010],
    [0b111, 0b001, 0b010, 0b100, 0b111],
    [0b000, 0b010, 0b111, 0b010, 0b000],
    [0b000, 0b000, 0b111, 0b000, 0b000],
    [0b000, 0b111, 0b000, 0b111, 0b000],
];

struct Stream {
    /// Head position in cells; negative while still above the frame.
    head: f32,
    speed: f32,
    len: u32,
}

pub struct Rain {
    rng: Rng,
    seed: u32,
    rows: u32,
    streams: Vec<Stream>,
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Rain::new(width, height, seed))
}

impl Rain {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let cols = (width / CELL_W).max(1);
        let rows = height.div_ceil(CELL_H).max(1);
        let streams = (0..cols).map(|_| Self::fresh(&mut rng, rows)).collect();
        let seed = rng.next_u64() as u32;
        Self {
            rng,
            seed,
            rows,
            streams,
        }
    }

    fn fresh(rng: &mut Rng, rows: u32) -> Stream {
        Stream {
            head: -rng.range(0.0, rows as f32 + 1.0),
            speed: rng.range(4.0, 12.0),
            len: 3 + rng.below(rows + 3),
        }
    }

    /// The glyph at a cell: stable, with a third of the cells flickering
    /// to another every quarter second.
    fn glyph(&self, col: i32, row: i32, t: f32) -> [u8; 5] {
        let stable = util::hash(col, row, self.seed);
        let flicker = util::hash(col, row, self.seed ^ (t * 4.0) as u32);
        let pick = if flicker.is_multiple_of(3) {
            flicker >> 8
        } else {
            stable
        };
        FONT[pick as usize % FONT.len()]
    }
}

fn draw_glyph(out: &mut Frame, x0: u32, y0: u32, glyph: [u8; 5], colour: [u8; 3]) {
    for (dy, bits) in glyph.iter().enumerate() {
        for dx in 0..3u32 {
            if (bits >> (2 - dx)) & 1 != 0 {
                out.set_pixel(x0 + dx, y0 + dy as u32, colour);
            }
        }
    }
}

impl Effect for Rain {
    fn step(&mut self, t: f32, dt: f32, out: &mut Frame) {
        out.as_bytes_mut().fill(0);
        let rows = self.rows;
        for stream in &mut self.streams {
            stream.head += stream.speed * dt;
            if stream.head - stream.len as f32 > rows as f32 {
                *stream = Self::fresh(&mut self.rng, rows);
            }
        }
        for c in 0..self.streams.len() {
            let stream = &self.streams[c];
            let head = stream.head.floor() as i32;
            for k in 0..=stream.len {
                let row = head - k as i32;
                if row < 0 || row >= rows as i32 {
                    continue;
                }
                let fade = 1.0 - k as f32 / (stream.len as f32 + 1.0);
                let colour = if k == 0 {
                    [255; 3]
                } else {
                    util::scaled([0.0, 1.0, 0.25], fade * fade)
                };
                let glyph = self.glyph(c as i32, row, t);
                draw_glyph(out, c as u32 * CELL_W, row as u32 * CELL_H, glyph, colour);
            }
        }
    }
}
