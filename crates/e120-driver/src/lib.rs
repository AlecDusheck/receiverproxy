//! Driving a wall of panels: turn frames into Colorlight packets and send them.
//!
//! This is the join between the topology in `e120-canvas`, the wire format in
//! `e120-proto`, and the transport in `e120-net`. Every content command goes
//! through [`Wall::show`], so there is one frame recipe to measure.

use anyhow::{Context, Result};
use e120_canvas::{Canvas, Frame};
use e120_net::Bpf;
use e120_proto as proto;
use std::time::{Duration, Instant};

/// How long `recv` may block; only replies to the layout frame are ever read.
const RECV_TIMEOUT: Duration = Duration::from_millis(200);

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Where a [`Wall`] sends its frames: the BPF device in production, a
/// recording sink in tests so the frame recipe can be pinned offline.
pub trait FrameSink {
    /// Send one raw Ethernet frame.
    ///
    /// # Errors
    /// Fails if the frame cannot be sent.
    fn send(&mut self, frame: &[u8]) -> Result<()>;
}

impl FrameSink for Bpf {
    fn send(&mut self, frame: &[u8]) -> Result<()> {
        Self::send(self, frame)
    }
}

/// A wall of panels on one network interface.
pub struct Wall<S: FrameSink = Bpf> {
    dev: S,
    canvas: Canvas,
    settings: Settings,
    announced: bool,
    frames_sent: u64,
    /// Per-receiver framebuffers, reused across refreshes.
    fbs: Vec<(u16, Frame)>,
    /// Scratch buffer for the row packet being sent.
    packet: Vec<u8>,
    brightness_frame: [u8; 77],
    sync_frame: [u8; 112],
}

impl Wall<Bpf> {
    /// Open the interface and bind it to a canvas.
    ///
    /// # Errors
    /// Fails if the interface cannot be opened, or the canvas is inconsistent.
    pub fn open(iface: &str, canvas: Canvas, settings: Settings) -> Result<Self> {
        canvas.validate()?;
        let dev = Bpf::open(iface, RECV_TIMEOUT).with_context(|| format!("open {iface}"))?;
        Self::with_sink(dev, canvas, settings)
    }
}

impl<S: FrameSink> Wall<S> {
    /// Bind a canvas to any frame sink.
    ///
    /// # Errors
    /// Fails if the canvas is inconsistent or too large for the layout frame.
    pub fn with_sink(dev: S, canvas: Canvas, settings: Settings) -> Result<Self> {
        canvas.validate()?;
        for r in &canvas.receivers {
            fits_u16(r.width, r.height).with_context(|| format!("receiver {} is larger than 65535 px", r.index))?;
        }
        fits_u16(canvas.width, canvas.height).context("canvas is larger than 65535 px")?;
        Ok(Self {
            dev,
            fbs: canvas.receiver_frames(),
            canvas,
            settings,
            announced: false,
            frames_sent: 0,
            packet: Vec::new(),
            brightness_frame: proto::brightness(settings.brightness),
            sync_frame: proto::sync(settings.brightness),
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
    /// The `as u16` casts are lossless: sizes were checked in `with_sink`.
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
            color_order, timing, ..
        } = self.settings;
        self.dev.send(&self.brightness_frame)?;

        self.canvas.render_into(frame, &mut self.fbs);
        for (_, fb) in &self.fbs {
            for (y, row) in fb.rows().enumerate() {
                if y > 0 && !timing.row_gap.is_zero() {
                    std::thread::sleep(timing.row_gap);
                }
                for (i, chunk) in row.chunks(proto::MAX_PIXELS_PER_PACKET).enumerate() {
                    let offset = i * proto::MAX_PIXELS_PER_PACKET;
                    proto::pixel_row_into(&mut self.packet, y as u16, offset as u16, chunk, color_order);
                    self.dev.send(&self.packet)?;
                }
            }
        }

        std::thread::sleep(timing.latch_gap);
        for _ in 0..timing.latches {
            self.dev.send(&self.sync_frame)?;
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

fn fits_u16(w: u32, h: u32) -> Result<()> {
    u16::try_from(w)?;
    u16::try_from(h)?;
    Ok(())
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
        let now = Instant::now();
        Self {
            period,
            next: now,
            started: now,
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

    impl FrameSink for Vec<Vec<u8>> {
        fn send(&mut self, frame: &[u8]) -> Result<()> {
            self.push(frame.to_vec());
            Ok(())
        }
    }

    fn gradient(w: u32, h: u32) -> Frame {
        let mut f = Frame::black(w, h);
        for y in 0..h {
            for x in 0..w {
                f.set_pixel(x, y, [x as u8, y as u8, (x ^ y) as u8]);
            }
        }
        f
    }

    /// No sleeps, so the recipe tests run instantly; the counts are the
    /// measured defaults.
    fn quick() -> Settings {
        Settings {
            timing: Timing {
                latch_gap: Duration::ZERO,
                ..Timing::default()
            },
            ..Settings::default()
        }
    }

    fn be16(f: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([f[at], f[at + 1]])
    }

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
        assert_eq!(
            s.timing,
            Timing {
                latches: 3,
                latch_gap: Duration::from_micros(500),
                row_gap: Duration::ZERO,
            }
        );
    }

    #[test]
    fn a_refresh_is_brightness_then_rows_then_three_latches() {
        let settings = quick();
        let mut wall = Wall::with_sink(Vec::new(), Canvas::single(128, 64), settings).unwrap();
        let frame = gradient(128, 64);
        wall.show(&frame).unwrap();
        let sent = &wall.dev;

        assert_eq!(sent.len(), 1 + 64 + 3);
        assert_eq!(sent[0].len(), 77);
        assert_eq!(&sent[0][12..17], &[0x0a, 0xff, 0xff, 0xff, 0xff]);
        for (y, f) in sent[1..65].iter().enumerate() {
            assert_eq!(f[12], 0x55);
            assert_eq!(be16(f, 13), y as u16, "row");
            assert_eq!(be16(f, 15), 0, "offset");
            assert_eq!(be16(f, 17), 128, "count");
            assert_eq!(&f[19..21], &[0x08, 0x88]);
            assert_eq!(f.len(), 21 + 128 * 3);
            assert_eq!(f, &proto::pixel_row(y as u16, 0, frame.row(y as u32), settings.color_order));
        }
        for f in &sent[65..] {
            assert_eq!(f.len(), 112);
            assert_eq!(&f[12..14], &[0x01, 0x07]);
            assert_eq!(f[35], 0xff);
        }
        assert_eq!(wall.frames_sent(), 1);
    }

    #[test]
    fn wide_rows_are_chunked_at_the_packet_limit() {
        let settings = Settings {
            brightness: 40,
            color_order: proto::ColorOrder::Rgb,
            ..quick()
        };
        let mut wall = Wall::with_sink(Vec::new(), Canvas::single(1000, 2), settings).unwrap();
        let frame = gradient(1000, 2);
        wall.show(&frame).unwrap();
        let sent = &wall.dev;

        assert_eq!(sent.len(), 1 + 2 * 3 + 3);
        assert_eq!(&sent[0][13..17], &[40, 40, 40, 0xff]);
        let mut rows = sent[1..7].iter();
        for y in 0..2u16 {
            for (offset, count) in [(0, 497), (497, 497), (994, 6)] {
                let f = rows.next().unwrap();
                assert_eq!((f[12], be16(f, 13), be16(f, 15), be16(f, 17)), (0x55, y, offset, count));
                let px = &frame.row(u32::from(y))[usize::from(offset)..][..usize::from(count)];
                assert_eq!(f, &proto::pixel_row(y, offset, px, settings.color_order));
            }
        }
        assert_eq!(sent[7][35], 40);
        assert_eq!(&sent[7][38..41], &[40; 3]);
    }

    #[test]
    fn every_receiver_gets_its_own_rows_and_the_layout_frame_leads_when_asked() {
        let settings = Settings {
            announce_layout: true,
            ..quick()
        };
        let mut canvas = Canvas::grid(8, 4, 2, 1);
        canvas.receivers = vec![
            e120_canvas::Receiver { index: 0, width: 8, height: 4 },
            e120_canvas::Receiver { index: 1, width: 8, height: 4 },
        ];
        canvas.panels[1].receiver = 1;
        canvas.panels[1].receiver_x = 0;
        let mut wall = Wall::with_sink(Vec::new(), canvas.clone(), settings).unwrap();
        let frame = gradient(16, 4);
        wall.show(&frame).unwrap();
        wall.show(&frame).unwrap();
        let sent = &wall.dev;

        // Layout once, then two refreshes of brightness + 2 x 4 rows + 3 latches.
        assert_eq!(sent.len(), 2 + 2 * (1 + 8 + 3));
        assert_eq!(sent[0], proto::set_layout(0, 8, 4, 0, 0, 16, 4));
        assert_eq!(sent[1], proto::set_layout(1, 8, 4, 0, 0, 16, 4));
        let fbs = canvas.render(&frame);
        for refresh in [&sent[2..14], &sent[14..26]] {
            assert_eq!(refresh[0][12], 0x0a);
            for (i, f) in refresh[1..9].iter().enumerate() {
                let (fb, y) = (&fbs[i / 4].1, (i % 4) as u32);
                assert_eq!(f, &proto::pixel_row(y as u16, 0, fb.row(y), settings.color_order));
            }
            assert!(refresh[9..].iter().all(|f| f[12] == 0x01));
        }
    }

    #[test]
    fn oversized_walls_are_refused_before_anything_is_sent() {
        let mut canvas = Canvas::single(8, 8);
        canvas.receivers[0].width = 70_000;
        let err = Wall::with_sink(Vec::new(), canvas, quick()).err().unwrap();
        assert!(err.to_string().contains("receiver 0 is larger than 65535 px"), "{err}");

        let mut bad = Canvas::single(8, 8);
        bad.panels[0].x = 4;
        let err = Wall::with_sink(Vec::new(), bad, quick()).err().unwrap();
        assert!(err.to_string().starts_with("canvas is not valid:"), "{err}");
    }
}
