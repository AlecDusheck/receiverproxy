//! Falling sand: a wandering pour of coloured grains settling into heaps,
//! cleared when the heap blocks the pour. Each pixel is a physical light, so a grain is a grain.

use crate::effects::Effect;
use crate::util::Rng;
use e120_canvas::Frame;

const TICK_HZ: f32 = 60.0;

/// Grain colours; a grid cell holds an index into this plus one, 0 for empty.
const PALETTE: [[u8; 3]; 6] = [
    [255, 180, 0],
    [255, 40, 0],
    [0, 200, 255],
    [170, 0, 255],
    [0, 255, 60],
    [255, 255, 255],
];

pub struct Sand {
    rng: Rng,
    width: usize,
    height: usize,
    grid: Vec<u8>,
    acc: f32,
    pour_x: f32,
    colour: u8,
    colour_until: f32,
    /// Ticks in a row on which the pour found its cell occupied.
    blocked: u32,
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Sand::new(width, height, seed))
}

impl Sand {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
            width: width as usize,
            height: height as usize,
            grid: vec![0; (width * height) as usize],
            acc: 0.0,
            pour_x: width as f32 * 0.5,
            colour: 1,
            colour_until: 0.0,
            blocked: 0,
        }
    }

    fn tick(&mut self, t: f32) {
        let (w, h) = (self.width, self.height);
        if w == 0 || h == 0 {
            return;
        }
        self.pour_x = (self.pour_x + self.rng.range(-1.0, 1.0)).clamp(0.0, (w - 1) as f32);
        if t >= self.colour_until {
            self.colour = 1 + self.rng.below(PALETTE.len() as u32) as u8;
            self.colour_until = t + 1.5;
        }
        if self.rng.chance(0.8) {
            let x = self.pour_x as usize;
            if self.grid[x] == 0 {
                self.grid[x] = self.colour;
                self.blocked = 0;
            } else {
                self.blocked += 1;
            }
        }
        if self.blocked >= 30 {
            self.grid.fill(0);
            self.blocked = 0;
            return;
        }
        for y in (0..h - 1).rev() {
            for x in 0..w {
                self.settle(x, y);
            }
        }
    }

    /// Drop the grain at `(x, y)` straight down, else diagonally, if there is room.
    fn settle(&mut self, x: usize, y: usize) {
        let w = self.width;
        let i = y * w + x;
        let grain = self.grid[i];
        if grain == 0 {
            return;
        }
        let side = if self.rng.chance(0.5) { 1 } else { -1 };
        let x = x as i32;
        for cx in [x, x + side, x - side] {
            if cx < 0 || cx >= w as i32 {
                continue;
            }
            let j = (y + 1) * w + cx as usize;
            if self.grid[j] == 0 {
                self.grid[j] = grain;
                self.grid[i] = 0;
                return;
            }
        }
    }
}

impl Effect for Sand {
    fn step(&mut self, t: f32, dt: f32, out: &mut Frame) {
        self.acc += dt * TICK_HZ;
        let ticks = (self.acc as u32).min(4);
        self.acc -= ticks as f32;
        for _ in 0..ticks {
            self.tick(t);
        }
        let pixels = out.as_bytes_mut().as_chunks_mut::<3>().0;
        for (px, &g) in pixels.iter_mut().zip(&self.grid) {
            *px = PALETTE
                .get(usize::from(g).wrapping_sub(1))
                .copied()
                .unwrap_or([0; 3]);
        }
    }
}
