//! Shared by the effects: a deterministic PRNG, value noise, colour arithmetic.

use e120_canvas::Frame;

/// One part in 2^24, the resolution of [`Rng::unit`] and [`noise`].
const UNIT: f32 = 1.0 / 16_777_216.0;

/// xorshift64*: deterministic from its seed, and never at zero.
pub struct Rng(u64);

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * UNIT
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform in `0..n`; 0 when `n` is 0.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        ((self.next_u64() >> 32) % u64::from(n)) as u32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

/// A mixed 32-bit hash of a lattice point.
pub fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ seed.wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// Value noise in `[0, 1]`, smooth between lattice points one unit apart.
pub fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (sx, sy) = (smooth(x - x0), smooth(y - y0));
    let (ix, iy) = (x0 as i32, y0 as i32);
    let at = |dx: i32, dy: i32| (hash(ix + dx, iy + dy, seed) >> 8) as f32 * UNIT;
    lerp(
        lerp(at(0, 0), at(1, 0), sx),
        lerp(at(0, 1), at(1, 1), sx),
        sy,
    )
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `0.0..=1.0` to a pixel level, rounded to nearest.
pub fn level(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// A colour in `0.0..=1.0` per channel, scaled by `k`, as pixel levels.
pub fn scaled(rgb: [f32; 3], k: f32) -> [u8; 3] {
    rgb.map(|c| level(c * k))
}

/// Hue in turns, saturation and value in `0.0..=1.0`.
pub fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let f = h - h.floor();
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match h as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [level(r), level(g), level(b)]
}

/// Add light to the pixel at `(x, y)`, saturating; off the frame it is dropped.
pub fn add_pixel(out: &mut Frame, x: i32, y: i32, light: [u8; 3]) {
    let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
        return;
    };
    let mut px = out.pixel(x, y);
    for (a, b) in px.iter_mut().zip(light) {
        *a = a.saturating_add(b);
    }
    out.set_pixel(x, y, px);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rng_is_deterministic_and_in_range() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..1000 {
            let u = a.unit();
            assert!((0.0..1.0).contains(&u));
            assert!(a.below(5) < 5);
        }
        assert_eq!(Rng::new(0).next_u64(), Rng::new(1).next_u64());
        assert_eq!(b.next_u64(), Rng::new(7).next_u64());
    }

    #[test]
    fn noise_is_continuous_and_bounded() {
        let mut prev = noise(0.0, 0.0, 1);
        for i in 1..200 {
            let n = noise(i as f32 * 0.05, 0.3, 1);
            assert!((0.0..=1.0).contains(&n));
            assert!((n - prev).abs() < 0.1, "step {i}: {prev} -> {n}");
            prev = n;
        }
    }

    #[test]
    fn hsv_hits_the_primaries_and_white() {
        assert_eq!(hsv(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv(1.0 / 3.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv(2.0 / 3.0, 1.0, 1.0), [0, 0, 255]);
        assert_eq!(hsv(0.9, 0.0, 1.0), [255; 3]);
        assert_eq!(level(0.01), 3);
    }

    #[test]
    fn adding_light_saturates_and_drops_off_frame_writes() {
        let mut f = Frame::black(2, 1);
        add_pixel(&mut f, 1, 0, [200, 10, 0]);
        add_pixel(&mut f, 1, 0, [100, 10, 0]);
        add_pixel(&mut f, -1, 0, [9; 3]);
        add_pixel(&mut f, 0, 5, [9; 3]);
        assert_eq!(f.pixel(1, 0), [255, 20, 0]);
        assert_eq!(f.pixel(0, 0), [0; 3]);
    }
}
