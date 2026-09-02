//! The effect interface and the registry the binary runs from.

use e120_canvas::Frame;
use std::ops::Range;

mod cast;
mod comet;
mod fire;
mod fireflies;
mod fog;
mod life;
mod lightning;
mod primaries;
mod pulse;
mod rain;
mod sand;
mod scanner;
mod stars;

/// How the frame just stepped should reach the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refresh {
    /// Whole-panel gain, 0-255, as a fraction of the wall's brightness.
    pub gain: u8,
    /// Per-channel gains on top of `gain`; `[255; 3]` is no cast.
    pub cast: [u8; 3],
    /// Rows to send; the whole frame when `None`.
    pub rows: Option<Range<u32>>,
}

impl Default for Refresh {
    fn default() -> Self {
        Self {
            gain: 255,
            cast: [255; 3],
            rows: None,
        }
    }
}

pub trait Effect {
    /// Draw the frame for `t` seconds, `dt` after the previous one. `out`
    /// still holds the previous frame; an effect that does not fade it clears it.
    fn step(&mut self, t: f32, dt: f32, out: &mut Frame);

    /// How to send what `step` drew: the whole frame at full gain by default.
    fn refresh(&self) -> Refresh {
        Refresh::default()
    }

    /// The frame rate the effect wants; the command line's otherwise.
    fn fps(&self) -> Option<u32> {
        None
    }
}

/// Build an effect for a frame of the given size from a seed.
pub type Build = fn(u32, u32, u64) -> Box<dyn Effect>;

pub struct Entry {
    pub name: &'static str,
    pub blurb: &'static str,
    pub build: Build,
}

pub static REGISTRY: &[Entry] = &[
    Entry {
        name: "stars",
        blurb: "white points on black, a slow twinkle, a meteor now and then",
        build: stars::build,
    },
    Entry {
        name: "fireflies",
        blurb: "a few single-pixel lights wandering and breathing at a few percent",
        build: fireflies::build,
    },
    Entry {
        name: "lightning",
        blurb: "single-frame white flashes through the gain, after-flashes, a branching bolt",
        build: lightning::build,
    },
    Entry {
        name: "primaries",
        blurb: "three pure R, G, B discs drifting into additive white",
        build: primaries::build,
    },
    Entry {
        name: "comet",
        blurb: "one full-white pixel on a curved path with a colour-cycled trail, 240 fps",
        build: comet::build,
    },
    Entry {
        name: "fog",
        blurb: "value-noise plasma at 1-4% in two slowly mixing hues",
        build: fog::build,
    },
    Entry {
        name: "fire",
        blurb: "cooling-map fire, deep red through orange to near white",
        build: fire::build,
    },
    Entry {
        name: "life",
        blurb: "Conway's Life, cells coloured by age, reseeded when it stalls",
        build: life::build,
    },
    Entry {
        name: "sand",
        blurb: "falling sand poured in colours, cleared when the heap blocks the pour",
        build: sand::build,
    },
    Entry {
        name: "scanner",
        blurb: "a bright row then a column sweeping at 240 fps; point a phone camera at it",
        build: scanner::build,
    },
    Entry {
        name: "rain",
        blurb: "falling 3x5 glyphs, bright head and fading tail",
        build: rain::build,
    },
    Entry {
        name: "pulse",
        blurb: "a fixed disc breathing through the latch-frame gain alone",
        build: pulse::build,
    },
    Entry {
        name: "cast",
        blurb: "a fixed white field tinted through the three channel gains",
        build: cast::build,
    },
];

pub fn find(name: &str) -> Option<&'static Entry> {
    REGISTRY.iter().find(|e| e.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sixty frames at 30 fps; the frame keeps its size and something lights up.
    fn exercise(entry: &Entry, w: u32, h: u32) {
        let mut effect = (entry.build)(w, h, 7);
        let mut frame = Frame::black(w, h);
        let mut lit = false;
        for i in 0..60 {
            effect.step(i as f32 / 30.0, 1.0 / 30.0, &mut frame);
            assert_eq!((frame.width, frame.height), (w, h), "{}", entry.name);
            assert_eq!(
                frame.as_bytes().len(),
                (w * h * 3) as usize,
                "{}",
                entry.name
            );
            lit |= frame.as_bytes().iter().any(|&b| b > 0);
            if let Some(rows) = effect.refresh().rows {
                assert!(
                    rows.start <= rows.end && rows.end <= h,
                    "{} at {w}x{h}: rows {rows:?}",
                    entry.name
                );
            }
        }
        assert!(lit, "{} at {w}x{h} never lit a pixel", entry.name);
    }

    #[test]
    fn every_effect_steps_sixty_frames_at_two_sizes_and_stays_in_the_frame() {
        for entry in REGISTRY {
            exercise(entry, 128, 64);
            exercise(entry, 7, 5);
        }
    }

    #[test]
    fn effects_are_deterministic_from_their_seed() {
        for entry in REGISTRY {
            let mut a = (entry.build)(16, 8, 99);
            let mut b = (entry.build)(16, 8, 99);
            let (mut fa, mut fb) = (Frame::black(16, 8), Frame::black(16, 8));
            for i in 0..30 {
                a.step(i as f32 / 30.0, 1.0 / 30.0, &mut fa);
                b.step(i as f32 / 30.0, 1.0 / 30.0, &mut fb);
                assert_eq!(fa, fb, "{} frame {i}", entry.name);
                assert_eq!(a.refresh(), b.refresh(), "{} frame {i}", entry.name);
            }
        }
    }

    #[test]
    fn names_are_unique_and_found() {
        for (i, entry) in REGISTRY.iter().enumerate() {
            assert!(std::ptr::eq(find(entry.name).unwrap(), entry));
            assert!(
                REGISTRY[..i].iter().all(|e| e.name != entry.name),
                "{} twice",
                entry.name
            );
            assert!(!entry.blurb.is_empty());
        }
        assert!(find("nope").is_none());
        assert!(find("list").is_none());
        assert!(find("cycle").is_none());
    }
}
