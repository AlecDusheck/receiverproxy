//! Driving a wall of panels: turn frames into Colorlight packets and send them.
//!
//! This is the join between the topology in `e120-canvas`, the wire format in
//! `e120-proto`, and the transport in `e120-net`. Every content command goes
//! through [`Wall::show`], so there is one frame recipe to measure.

use anyhow::Result;
use e120_canvas::{Canvas, Frame};
use e120_net::Bpf;
use e120_proto as proto;
use std::time::{Duration, Instant};

/// Per-refresh timing. The defaults were measured on the bench
/// (docs/rendering-recipe.md); change them only with a measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Latch frames after the rows. One never starts the display; two decays
    /// into noise on a ~10 s period; three hold.
    pub latches: u32,
    /// Pause between the last row and the first latch. Without it the card
    /// latches before the last row is stored and that row flickers.
    pub latch_gap: Duration,
    /// Pause between row packets. The card's receive FIFO is 1 KB, so a
    /// line-rate burst can drop its tail; zero has measured fine so far.
    pub row_gap: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            latches: 3,
            latch_gap: Duration::from_micros(500),
            row_gap: Duration::ZERO,
        }
    }
}

/// How a wall should be driven.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub brightness: u8,
    pub color_order: proto::ColorOrder,
    /// Send a layout frame before the first frame. Off by default: a
    /// provisioned card takes its control area from EEPROM, and the layout
    /// frame blanks it.
    pub announce_layout: bool,
    pub timing: Timing,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            brightness: 255,
            color_order: proto::ColorOrder::Bgr,
            announce_layout: false,
            timing: Timing::default(),
        }
    }
}

/// A wall of panels on one network interface.
pub struct Wall {
    dev: Bpf,
    canvas: Canvas,
    settings: Settings,
    announced: bool,
    frames_sent: u64,
}

impl Wall {
    /// Open the interface and bind it to a canvas.
    ///
    /// # Errors
    /// Fails if the interface cannot be opened, or the canvas is inconsistent.
    pub fn open(iface: &str, canvas: Canvas, settings: Settings) -> Result<Self> {
        if let Err(problems) = canvas.validate() {
            anyhow::bail!("canvas is not valid:\n  {}", problems.join("\n  "));
        }
        let dev = Bpf::open(iface, true, 200)?;
        Ok(Self {
            dev,
            canvas,
            settings,
            announced: false,
            frames_sent: 0,
        })
    }

    #[must_use]
    pub const fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    #[must_use]
    pub const fn frames_sent(&self) -> u64 {
        self.frames_sent
    }

    /// Tell every receiver its own size and the size of the whole wall.
    ///
    /// # Errors
    /// Fails if a frame cannot be sent.
    pub fn announce_layout(&mut self) -> Result<()> {
        for r in &self.canvas.receivers {
            let frame = proto::set_layout(
                r.index,
                r.width as u16,
                r.height as u16,
                0,
                0,
                self.canvas.width as u16,
                self.canvas.height as u16,
            );
            self.dev.send(&frame)?;
        }
        self.announced = true;
        Ok(())
    }

    /// Render one canvas frame and push it to every receiver:
    /// brightness, one row packet per panel row, the latch gap, the latches.
    /// The order matters: a card fresh from arming never starts displaying
    /// when the latch leads the rows; once woken, either order works, which
    /// hid the difference for a long time.
    ///
    /// # Errors
    /// Fails if a frame cannot be sent.
    pub fn show(&mut self, frame: &Frame) -> Result<()> {
        if self.settings.announce_layout && !self.announced {
            self.announce_layout()?;
        }
        let Settings {
            brightness,
            color_order,
            timing,
            ..
        } = self.settings;
        self.dev.send(&proto::brightness(brightness))?;

        for (_, fb) in self.canvas.render(frame) {
            let width = fb.width as usize;
            let mut row = vec![[0u8; 3]; width];
            for y in 0..fb.height {
                if y > 0 && !timing.row_gap.is_zero() {
                    std::thread::sleep(timing.row_gap);
                }
                for (x, px) in row.iter_mut().enumerate() {
                    *px = fb.pixel(x as u32, y);
                }
                let mut offset = 0usize;
                for chunk in row.chunks(proto::MAX_PIXELS_PER_PACKET) {
                    self.dev
                        .send(&proto::pixel_row(y as u16, offset as u16, chunk, color_order))?;
                    offset += chunk.len();
                }
            }
        }

        std::thread::sleep(timing.latch_gap);
        for _ in 0..timing.latches {
            self.dev.send(&proto::sync(brightness))?;
        }
        self.frames_sent += 1;
        Ok(())
    }

    /// Blank every panel.
    ///
    /// # Errors
    /// Fails if a frame cannot be sent.
    pub fn blank(&mut self) -> Result<()> {
        let black = Frame::black(self.canvas.width, self.canvas.height);
        self.show(&black)
    }
}

/// Paces a loop to a target frame rate, reporting the rate actually achieved.
pub struct Pacer {
    period: Duration,
    next: Instant,
    started: Instant,
    frames: u64,
}

impl Pacer {
    #[must_use]
    pub fn new(fps: u32) -> Self {
        let period = Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));
        Self {
            period,
            next: Instant::now(),
            started: Instant::now(),
            frames: 0,
        }
    }

    /// Sleep until the next frame is due.
    pub fn wait(&mut self) {
        self.frames += 1;
        self.next += self.period;
        let now = Instant::now();
        if self.next > now {
            std::thread::sleep(self.next - now);
        } else {
            // Running behind; give up on catching up rather than spiralling.
            self.next = now;
        }
    }

    /// Frames per second actually achieved so far.
    #[must_use]
    pub fn achieved_fps(&self) -> f64 {
        let secs = self.started.elapsed().as_secs_f64();
        if secs > 0.0 {
            self.frames as f64 / secs
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacer_reports_a_plausible_rate() {
        let mut p = Pacer::new(1000);
        for _ in 0..5 {
            p.wait();
        }
        assert!(p.achieved_fps() > 0.0);
    }

    #[test]
    fn settings_default_to_the_measured_recipe() {
        let s = Settings::default();
        assert_eq!(s.brightness, 255);
        assert!(!s.announce_layout);
        assert_eq!(s.timing.latches, 3);
        assert_eq!(s.timing.latch_gap, Duration::from_micros(500));
        assert_eq!(s.timing.row_gap, Duration::ZERO);
    }
}
