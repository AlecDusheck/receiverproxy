//! Driving a wall of panels: turn frames into Colorlight packets and send them.
//!
//! This is the join between the topology in `e120-canvas`, the wire format in
//! `e120-proto`, and the transport in `e120-net`. Both the CLI and any server
//! use it, so frame delivery behaves identically wherever it is driven from.

use anyhow::Result;
use e120_canvas::{Canvas, Frame};
use e120_net::Bpf;
use e120_proto as proto;
use std::time::{Duration, Instant};

/// How a frame is cut into row packets. The card's pixel map covers the
/// stored height (half the panel) at double width, so the wire may want one
/// packet per panel row or one double-width packet per stored row; which
/// panel row supplies the second half is a bench measurement.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Raster {
    #[default]
    Rows,
    Halves,
    HalvesSwapped,
    Interleaved,
}

impl std::str::FromStr for Raster {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "rows" => Ok(Self::Rows),
            "halves" => Ok(Self::Halves),
            "halves-swapped" => Ok(Self::HalvesSwapped),
            "interleaved" => Ok(Self::Interleaved),
            o => Err(format!("unknown raster {o:?} (rows|halves|halves-swapped|interleaved)")),
        }
    }
}

/// How a wall should be driven.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub brightness: u8,
    pub color_order: proto::ColorOrder,
    pub raster: Raster,
    /// Send a layout frame before the first frame, so the card knows its size.
    pub announce_layout: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            brightness: 255,
            color_order: proto::ColorOrder::Bgr,
            raster: Raster::Rows,
            announce_layout: true,
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

    /// Set panel brightness.
    ///
    /// # Errors
    /// Fails if a frame cannot be sent.
    pub fn set_brightness(&mut self, brightness: u8) -> Result<()> {
        self.settings.brightness = brightness;
        self.dev.send(&proto::brightness(brightness))?;
        Ok(())
    }

    /// Render one canvas frame and push it to every receiver.
    ///
    /// # Errors
    /// Fails if a frame cannot be sent.
    pub fn show(&mut self, frame: &Frame) -> Result<()> {
        if self.settings.announce_layout && !self.announced {
            self.announce_layout()?;
        }
        self.dev
            .send(&proto::brightness(self.settings.brightness))?;

        for (_, fb) in self.canvas.render(frame) {
            let width = fb.width as usize;
            let height = fb.height as usize;
            // (wire row, panel rows it carries): one row, or two side by side.
            let lines: Vec<(u16, Vec<u32>)> = match self.settings.raster {
                Raster::Rows => (0..height).map(|y| (y as u16, vec![y as u32])).collect(),
                Raster::Halves => (0..height / 2).map(|r| (r as u16, vec![r as u32, (r + height / 2) as u32])).collect(),
                Raster::HalvesSwapped => (0..height / 2).map(|r| (r as u16, vec![(r + height / 2) as u32, r as u32])).collect(),
                Raster::Interleaved => (0..height / 2).map(|r| (r as u16, vec![2 * r as u32, 2 * r as u32 + 1])).collect(),
            };
            let mut row_pixels = vec![[0u8; 3]; width * 2];
            for (wire_row, panel_rows) in lines {
                let n = panel_rows.len() * width;
                for (k, &y) in panel_rows.iter().enumerate() {
                    for x in 0..width {
                        row_pixels[k * width + x] = fb.pixel(x as u32, y);
                    }
                }
                let mut offset = 0usize;
                for chunk in row_pixels[..n].chunks(proto::MAX_PIXELS_PER_PACKET) {
                    self.dev.send(&proto::pixel_row(
                        wire_row,
                        offset as u16,
                        chunk,
                        self.settings.color_order,
                    ))?;
                    offset += chunk.len();
                }
            }
        }

        // Latch after a short gap (the card otherwise latches before the
        // last row is stored and that row flickers), three times: two is
        // borderline on this card, three holds.
        std::thread::sleep(Duration::from_micros(500));
        for _ in 0..3 {
            self.dev.send(&proto::sync(self.settings.brightness))?;
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
    fn settings_default_to_full_brightness() {
        let s = Settings::default();
        assert_eq!(s.brightness, 255);
        assert!(s.announce_layout);
    }
}
