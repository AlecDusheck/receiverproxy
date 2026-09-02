//! Conway's Game of Life, cells coloured by age, reseeded when the population
//! stalls. Each pixel is a physical light, so a grid of discrete cells reads literally.

use crate::effects::Effect;
use crate::util::{self, Rng};
use wall::Frame;

const GENERATIONS_PER_SECOND: f32 = 8.0;

pub struct Life {
    rng: Rng,
    width: usize,
    height: usize,
    /// Generations alive, 0 for dead.
    age: Vec<u8>,
    next: Vec<u8>,
    acc: f32,
    /// Population of the last two generations.
    history: [usize; 2],
    stall: u32,
}

pub fn build(width: u32, height: u32, seed: u64) -> Box<dyn Effect> {
    Box::new(Life::new(width, height, seed))
}

impl Life {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        let cells = (width * height) as usize;
        let mut life = Self {
            rng: Rng::new(seed),
            width: width as usize,
            height: height as usize,
            age: vec![0; cells],
            next: vec![0; cells],
            acc: 0.0,
            history: [0; 2],
            stall: 0,
        };
        life.reseed();
        life
    }

    fn reseed(&mut self) {
        for cell in &mut self.age {
            *cell = u8::from(self.rng.chance(0.3));
        }
        self.stall = 0;
    }

    fn generation(&mut self) {
        let (w, h) = (self.width, self.height);
        if w == 0 || h == 0 {
            return;
        }
        for y in 0..h {
            let (yu, yd) = ((y + h - 1) % h, (y + 1) % h);
            for x in 0..w {
                let (xl, xr) = ((x + w - 1) % w, (x + 1) % w);
                let alive = |xx: usize, yy: usize| u8::from(self.age[yy * w + xx] > 0);
                let n = alive(xl, yu)
                    + alive(x, yu)
                    + alive(xr, yu)
                    + alive(xl, y)
                    + alive(xr, y)
                    + alive(xl, yd)
                    + alive(x, yd)
                    + alive(xr, yd);
                let a = self.age[y * w + x];
                self.next[y * w + x] = match (a > 0, n) {
                    (true, 2 | 3) => a.saturating_add(1),
                    (false, 3) => 1,
                    _ => 0,
                };
            }
        }
        std::mem::swap(&mut self.age, &mut self.next);

        // A population repeating with period one or two for forty
        // generations is a still life or a set of blinkers: start over.
        let pop = self.age.iter().filter(|&&a| a > 0).count();
        if self.history.contains(&pop) {
            self.stall += 1;
        } else {
            self.stall = 0;
        }
        self.history = [pop, self.history[0]];
        if pop == 0 || self.stall >= 40 {
            self.reseed();
        }
    }
}

fn colour(age: u8) -> [u8; 3] {
    if age == 0 {
        return [0; 3];
    }
    // Newborn near white, then hue by age.
    let a = f32::from(age);
    util::hsv(0.55 + a * 0.02, (a / 5.0).min(1.0), 1.0)
}

impl Effect for Life {
    fn step(&mut self, _t: f32, dt: f32, out: &mut Frame) {
        self.acc += dt * GENERATIONS_PER_SECOND;
        let ticks = (self.acc as u32).min(3);
        self.acc -= ticks as f32;
        for _ in 0..ticks {
            self.generation();
        }
        let pixels = out.as_bytes_mut().as_chunks_mut::<3>().0;
        for (px, &age) in pixels.iter_mut().zip(&self.age) {
            *px = colour(age);
        }
    }
}
