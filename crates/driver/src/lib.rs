//! Driving a wall of panels: turn frames into Colorlight packets and send them.
//!
//! This is the join between the topology in `wall`, the wire format in
//! `colorlight`, and the transport in `rawlink`. Every content command goes
//! through [`Wall::show`], so there is one frame recipe to measure.

use anyhow::{Context, Result};
use rawlink::Link;
use std::ops::Range;
use std::time::{Duration, Instant};
use wall::{Canvas, Frame};

/// How long `recv` may block; only replies to the layout frame are ever read.
const RECV_TIMEOUT: Duration = Duration::from_millis(200);

/// Per-refresh timing. The defaults were measured on the bench
/// (docs/rendering.md); change them only with a measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Latch frames after the rows. One never starts the display; two decays
    /// into noise on a ~10 s period; three hold.
    pub latches: u32,
    /// Pause between the last row and the first latch. Without it the card
    /// latches before the last row is stored and that row flickers.
    pub latch_gap: Duration,
    /// Pause between row packets. The card's receive FIFO is 1 KB, so a
    /// line-rate burst can drop its tail; zero measured fine.
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
    pub color_order: colorlight::ColorOrder,
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
            color_order: colorlight::ColorOrder::Bgr,
            announce_layout: false,
            timing: Timing::default(),
        }
    }
}

/// Where a [`Wall`] sends its frames: the raw link in production, a
/// recording sink in tests so the frame recipe can be pinned offline.
pub trait FrameSink {
    /// Send one raw Ethernet frame.
    ///
    /// # Errors
    /// Fails if the frame cannot be sent.
    fn send(&mut self, frame: &[u8]) -> Result<()>;
}

impl FrameSink for Link {
    fn send(&mut self, frame: &[u8]) -> Result<()> {
        Self::send(self, frame)
    }
}

/// A wall of panels on one network interface.
pub struct Wall<S: FrameSink = Link> {
    dev: S,
    canvas: Canvas,
    settings: Settings,
    announced: bool,
    frames_sent: u64,
    /// The screen framebuffer, reused across refreshes.
    screen: Frame,
    /// Scratch buffer for the row packet being sent.
    packet: Vec<u8>,
    brightness_frame: [u8; 77],
    sync_frame: [u8; 112],
}

impl Wall<Link> {
    /// Open the interface and bind it to a canvas.
    ///
    /// # Errors
    /// Fails if the interface cannot be opened, or the canvas is inconsistent.
    pub fn open(iface: &str, canvas: Canvas, settings: Settings) -> Result<Self> {
        canvas.validate()?;
        let dev = Link::open(iface, RECV_TIMEOUT).with_context(|| format!("open {iface}"))?;
        Self::with_sink(dev, canvas, settings)
    }
}

impl<S: FrameSink> Wall<S> {
    /// Bind a canvas to any frame sink.
    ///
    /// # Errors
    /// Fails if the canvas is inconsistent or too large for the row packet's
    /// u16 coordinates (every receiver lies inside it, so they fit too).
    pub fn with_sink(dev: S, canvas: Canvas, settings: Settings) -> Result<Self> {
        canvas.validate()?;
        let (sw, sh) = canvas.screen_size();
        fits_u16(sw, sh).context("screen is larger than 65535 px")?;
        Ok(Self {
            dev,
            screen: canvas.screen_frame(),
            canvas,
            settings,
            announced: false,
            frames_sent: 0,
            packet: Vec::new(),
            brightness_frame: colorlight::brightness(settings.brightness),
            sync_frame: colorlight::sync(settings.brightness),
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

    #[must_use]
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Change the brightness every later refresh carries. Nothing is sent
    /// here; the cached brightness and latch frames are rebuilt.
    pub fn set_brightness(&mut self, brightness: u8) {
        self.set_gains(brightness, [brightness; 3]);
    }

    /// [`set_brightness`](Self::set_brightness) with the latch frame's three
    /// channel gains given separately (`colorlight::sync_gains`); the brightness
    /// frame carries only the master.
    pub fn set_gains(&mut self, brightness: u8, gains: [u8; 3]) {
        self.settings.brightness = brightness;
        self.brightness_frame = colorlight::brightness(brightness);
        self.sync_frame = colorlight::sync_gains(brightness, gains);
    }

    /// Tell every receiver its own window and the size of the whole wall.
    /// The `as u16` casts are lossless: sizes were checked in `with_sink`.
    ///
    /// # Errors
    /// Fails if a frame cannot be sent.
    pub fn announce_layout(&mut self) -> Result<()> {
        for r in &self.canvas.receivers {
            let frame = colorlight::set_layout(
                r.index,
                r.width as u16,
                r.height as u16,
                r.x as u16,
                r.y as u16,
                self.screen.width as u16,
                self.screen.height as u16,
            );
            self.dev.send(&frame)?;
        }
        self.announced = true;
        Ok(())
    }

    /// Render one canvas frame onto the screen and push the screen to the
    /// chain: brightness, one row packet per screen row (chunked at 497
    /// pixels), the latch gap, the latches. Row and pixel offset are screen
    /// coordinates; every card keeps its own window of them
    /// (docs/receiver-identity.md), so the stream is the same however many
    /// cards listen.
    ///
    /// A card fresh from arming never starts when the latch leads the rows;
    /// once woken either order works (docs/rendering.md).
    ///
    /// # Errors
    /// Fails if a frame cannot be sent.
    pub fn show(&mut self, frame: &Frame) -> Result<()> {
        let all = 0..self.screen.height;
        self.show_rows(frame, all)
    }

    /// [`show`](Self::show) sending only the screen rows in `rows` (clipped
    /// to the screen) between the brightness frame and the latches. Rows are
    /// addressed and the card keeps its frame memory, so the rest of the
    /// picture stays as last sent; an empty range sends brightness and
    /// latches alone. The card's own scan bounds how fast a band can change.
    ///
    /// # Errors
    /// Fails if a frame cannot be sent.
    pub fn show_rows(&mut self, frame: &Frame, rows: Range<u32>) -> Result<()> {
        if self.settings.announce_layout && !self.announced {
            self.announce_layout()?;
        }
        let Settings {
            color_order,
            timing,
            ..
        } = self.settings;
        self.dev.send(&self.brightness_frame)?;

        self.canvas.render_into(frame, &mut self.screen);
        let height = self.screen.height;
        for (i, y) in (rows.start.min(height)..rows.end.min(height)).enumerate() {
            if i > 0 && !timing.row_gap.is_zero() {
                std::thread::sleep(timing.row_gap);
            }
            let row = self.screen.row(y);
            for (j, chunk) in row.chunks(colorlight::MAX_PIXELS_PER_PACKET).enumerate() {
                let offset = j * colorlight::MAX_PIXELS_PER_PACKET;
                colorlight::pixel_row_into(
                    &mut self.packet,
                    y as u16,
                    offset as u16,
                    chunk,
                    color_order,
                );
                self.dev.send(&self.packet)?;
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
            assert_eq!(
                f,
                &colorlight::pixel_row(y as u16, 0, frame.row(y as u32), settings.color_order)
            );
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
            color_order: colorlight::ColorOrder::Rgb,
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
                assert_eq!(
                    (f[12], be16(f, 13), be16(f, 15), be16(f, 17)),
                    (0x55, y, offset, count)
                );
                let px = &frame.row(u32::from(y))[usize::from(offset)..][..usize::from(count)];
                assert_eq!(
                    f,
                    &colorlight::pixel_row(y, offset, px, settings.color_order)
                );
            }
        }
        assert_eq!(sent[7][35], 40);
        assert_eq!(&sent[7][38..41], &[40; 3]);
    }

    #[test]
    fn two_cards_share_one_screen_stream_and_the_layout_frame_leads_when_asked() {
        let settings = Settings {
            announce_layout: true,
            ..quick()
        };
        let canvas = Canvas::cards(8, 4, 2, 1);
        let mut wall = Wall::with_sink(Vec::new(), canvas, settings).unwrap();
        let frame = gradient(16, 4);
        wall.show(&frame).unwrap();
        wall.show(&frame).unwrap();
        let sent = &wall.dev;

        // Layout per card, then two refreshes of brightness + 4 screen rows
        // + 3 latches: the second card is told its window, not sent its own rows.
        assert_eq!(sent.len(), 2 + 2 * (1 + 4 + 3));
        assert_eq!(sent[0], colorlight::set_layout(0, 8, 4, 0, 0, 16, 4));
        assert_eq!(sent[1], colorlight::set_layout(1, 8, 4, 8, 0, 16, 4));
        for refresh in [&sent[2..10], &sent[10..18]] {
            assert_eq!(refresh[0][12], 0x0a);
            for (y, f) in refresh[1..5].iter().enumerate() {
                assert_eq!(be16(f, 17), 16, "the whole screen row");
                assert_eq!(
                    f,
                    &colorlight::pixel_row(y as u16, 0, frame.row(y as u32), settings.color_order)
                );
            }
            assert!(refresh[5..].iter().all(|f| f[12] == 0x01));
        }
    }

    #[test]
    fn show_rows_sends_only_that_band_between_brightness_and_latches() {
        let settings = quick();
        let mut wall = Wall::with_sink(Vec::new(), Canvas::single(128, 64), settings).unwrap();
        let frame = gradient(128, 64);
        wall.show_rows(&frame, 10..13).unwrap();
        let sent = &wall.dev;

        assert_eq!(sent.len(), 1 + 3 + 3);
        assert_eq!(sent[0], colorlight::brightness(255));
        for (f, y) in sent[1..4].iter().zip(10u16..) {
            assert_eq!(be16(f, 13), y, "row");
            assert_eq!(
                f,
                &colorlight::pixel_row(y, 0, frame.row(u32::from(y)), settings.color_order)
            );
        }
        assert!(sent[4..].iter().all(|f| f == &colorlight::sync(255)));
        assert_eq!(wall.frames_sent(), 1);

        // Clipped to the screen; an empty band still latches.
        wall.dev.clear();
        wall.show_rows(&frame, 60..100).unwrap();
        assert_eq!(wall.dev.len(), 1 + 4 + 3);
        assert_eq!(be16(&wall.dev[4], 13), 63);
        wall.dev.clear();
        wall.show_rows(&frame, 0..0).unwrap();
        assert_eq!(wall.dev.len(), 1 + 3);
        assert_eq!(wall.frames_sent(), 3);
    }

    #[test]
    fn show_is_show_rows_over_the_whole_screen() {
        let frame = gradient(1000, 2);
        let mut whole = Wall::with_sink(Vec::new(), Canvas::single(1000, 2), quick()).unwrap();
        let mut band = Wall::with_sink(Vec::new(), Canvas::single(1000, 2), quick()).unwrap();
        whole.show(&frame).unwrap();
        band.show_rows(&frame, 0..2).unwrap();
        assert_eq!(whole.dev, band.dev);
    }

    #[test]
    fn set_brightness_rebuilds_the_brightness_and_latch_frames() {
        let mut wall = Wall::with_sink(Vec::new(), Canvas::single(8, 1), quick()).unwrap();
        let frame = gradient(8, 1);
        wall.show(&frame).unwrap();
        wall.set_brightness(40);
        assert_eq!(wall.settings().brightness, 40);
        wall.show(&frame).unwrap();
        wall.set_gains(40, [10, 20, 30]);
        wall.show(&frame).unwrap();
        let sent = &wall.dev;

        // Three refreshes of brightness + 1 row + 3 latches.
        assert_eq!(sent.len(), 3 * 5);
        assert_eq!(sent[0], colorlight::brightness(255));
        assert_eq!(sent[2], colorlight::sync(255));
        assert_eq!(sent[5], colorlight::brightness(40));
        assert_eq!(&sent[5][13..17], &[40, 40, 40, 0xff]);
        assert!(sent[7..10].iter().all(|f| f == &colorlight::sync(40)));
        assert_eq!(sent[10], colorlight::brightness(40));
        assert!(sent[12..15]
            .iter()
            .all(|f| f == &colorlight::sync_gains(40, [10, 20, 30])));
        assert_eq!(&sent[12][38..41], &[10, 20, 30]);
        assert_eq!(
            sent[6], sent[11],
            "the row packet does not carry brightness"
        );
    }

    #[test]
    fn a_card_placed_further_along_the_screen_gets_its_pixels_at_that_offset() {
        let mut canvas = Canvas::cards(8, 4, 2, 1);
        canvas.receivers.swap(0, 1);
        canvas.receivers[0].index = 0;
        canvas.receivers[1].index = 1;
        // Card 0 now sits at x=8, card 1 at x=0; panel 0 still shows the
        // canvas's left half, so it must arrive at screen x 8..16.
        let mut wall = Wall::with_sink(Vec::new(), canvas, quick()).unwrap();
        let frame = gradient(16, 4);
        wall.show(&frame).unwrap();
        let row0 = &wall.dev[1];
        let px = row0[21..].as_chunks::<3>().0;
        assert_eq!(px.len(), 16);
        for x in 0..8u32 {
            let [r, g, b] = frame.pixel(x, 0);
            assert_eq!(
                px[8 + x as usize],
                [b, g, r],
                "canvas x {x} lands at screen x {}",
                8 + x
            );
        }
    }

    /// The old per-receiver loop, kept here so the single-card stream is
    /// pinned byte for byte: for one card at the origin the receiver
    /// framebuffer was the image itself, and every row went out with local
    /// coordinates from 0.
    fn old_single_card_stream(frame: &Frame, settings: Settings) -> Vec<u8> {
        let mut stream = Vec::new();
        let mut packet = Vec::new();
        stream.extend_from_slice(&colorlight::brightness(settings.brightness));
        for (y, row) in frame.rows().enumerate() {
            for (i, chunk) in row.chunks(colorlight::MAX_PIXELS_PER_PACKET).enumerate() {
                let offset = i * colorlight::MAX_PIXELS_PER_PACKET;
                colorlight::pixel_row_into(
                    &mut packet,
                    y as u16,
                    offset as u16,
                    chunk,
                    settings.color_order,
                );
                stream.extend_from_slice(&packet);
            }
        }
        for _ in 0..settings.timing.latches {
            stream.extend_from_slice(&colorlight::sync(settings.brightness));
        }
        stream
    }

    #[test]
    fn a_single_card_at_the_origin_gets_the_same_bytes_as_before() {
        for (w, h) in [(128, 64), (1000, 2)] {
            let settings = Settings {
                brightness: 40,
                ..quick()
            };
            let frame = gradient(w, h);
            let mut wall = Wall::with_sink(Vec::new(), Canvas::single(w, h), settings).unwrap();
            wall.show(&frame).unwrap();
            let new: Vec<u8> = wall.dev.concat();
            assert_eq!(new, old_single_card_stream(&frame, settings), "{w}x{h}");
        }
    }

    #[test]
    fn oversized_walls_are_refused_before_anything_is_sent() {
        let mut canvas = Canvas::single(8, 8);
        canvas.receivers[0].width = 70_000;
        let err = Wall::with_sink(Vec::new(), canvas, quick()).err().unwrap();
        assert!(
            err.to_string()
                .contains("receiver 0 at (0, 0) size 70000x8 exceeds the 65535 px screen space"),
            "{err}"
        );

        // A canvas past the wire's u16 space is refused by the same check,
        // which its single receiver trips first.
        let huge = Canvas::single(70_000, 1);
        let err = Wall::with_sink(Vec::new(), huge, quick()).err().unwrap();
        assert!(
            err.to_string()
                .contains("exceeds the 65535 px screen space"),
            "{err}"
        );

        let mut bad = Canvas::single(8, 8);
        bad.panels[0].x = 4;
        let err = Wall::with_sink(Vec::new(), bad, quick()).err().unwrap();
        assert!(err.to_string().starts_with("canvas is not valid:"), "{err}");
    }

    /// Counts frames and bytes; what the raw link would see, minus the syscall.
    struct Counting {
        packets: u64,
        bytes: u64,
    }

    impl FrameSink for Counting {
        fn send(&mut self, frame: &[u8]) -> Result<()> {
            self.packets += 1;
            self.bytes += frame.len() as u64;
            Ok(())
        }
    }

    /// Fifty cards (10 x 5 of 128x64, a 1280x320 screen), 300 frames:
    /// microseconds per frame to render and to pack, and the packet count.
    /// Run with `cargo test --release -p driver -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing; run in release with --nocapture"]
    fn fifty_cards_render_and_pack_time() {
        const FRAMES: u32 = 300;
        let canvas = Canvas::cards(128, 64, 10, 5);
        let frame = gradient(1280, 320);
        let sink = Counting {
            packets: 0,
            bytes: 0,
        };
        let mut wall = Wall::with_sink(sink, canvas.clone(), quick()).unwrap();

        let mut screen = canvas.screen_frame();
        let t = Instant::now();
        for _ in 0..FRAMES {
            canvas.render_into(&frame, &mut screen);
        }
        let render_us = t.elapsed().as_secs_f64() * 1e6 / f64::from(FRAMES);
        std::hint::black_box(&screen);

        let t = Instant::now();
        for _ in 0..FRAMES {
            wall.show(&frame).unwrap();
        }
        let show_us = t.elapsed().as_secs_f64() * 1e6 / f64::from(FRAMES);

        let packets = wall.dev.packets / u64::from(FRAMES);
        let bytes = wall.dev.bytes / u64::from(FRAMES);
        println!(
            "50 cards, 1280x320: render {render_us:.0} us/frame, pack {:.0} us/frame, {packets} packets/frame ({} row packets), {bytes} bytes/frame",
            show_us - render_us,
            packets - 1 - u64::from(quick().timing.latches),
        );
    }
}
