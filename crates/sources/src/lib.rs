//! Frame sources: ffmpeg video, raw rgb24 streams from other processes, and
//! built-in test patterns. ffmpeg is asked for rgb24 already scaled to the
//! wall size, so nothing here parses containers or codecs.

pub mod font;
pub mod raw;

use anyhow::{Context, Result};
use wall::Frame;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::str::FromStr;

/// Anything that refills a caller-owned frame, one image per call.
pub trait FrameSource {
    /// Fill `frame` with the next image, resizing it to the source's size if
    /// needed. `Ok(false)` at the end of the stream.
    ///
    /// # Errors
    /// Fails if the underlying stream cannot be read.
    fn next_frame(&mut self, frame: &mut Frame) -> Result<bool>;
}

/// A name that is not one of the allowed spellings for a type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownName {
    what: &'static str,
    got: String,
    allowed: &'static str,
}

impl std::fmt::Display for UnknownName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown {} {:?} ({})", self.what, self.got, self.allowed)
    }
}

impl std::error::Error for UnknownName {}

/// How a source should fit its images to the wall.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename_all = "lowercase"))]
pub enum Fit {
    /// Fill the wall exactly, ignoring the source's aspect ratio.
    #[default]
    Stretch,
    /// Preserve aspect ratio, padding with black.
    Contain,
    /// Preserve aspect ratio, cropping the overflow.
    Cover,
}

impl FromStr for Fit {
    type Err = UnknownName;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "stretch" => Ok(Self::Stretch),
            "contain" => Ok(Self::Contain),
            "cover" => Ok(Self::Cover),
            _ => Err(UnknownName {
                what: "fit",
                got: s.to_owned(),
                allowed: "stretch|contain|cover",
            }),
        }
    }
}

impl Fit {
    fn filter(self, w: u32, h: u32) -> String {
        match self {
            Self::Stretch => format!("scale={w}:{h}:flags=lanczos"),
            Self::Contain => format!(
                "scale={w}:{h}:flags=lanczos:force_original_aspect_ratio=decrease,\
                 pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black"
            ),
            Self::Cover => format!(
                "scale={w}:{h}:flags=lanczos:force_original_aspect_ratio=increase,\
                 crop={w}:{h}"
            ),
        }
    }
}

/// Decodes a video file (or any URL ffmpeg understands) into wall-sized frames.
pub struct VideoSource {
    child: Child,
    frames: raw::RawSource<ChildStdout>,
}

impl VideoSource {
    /// Start decoding `input`, scaled to `width` x `height` at `fps`.
    ///
    /// # Errors
    /// Fails if ffmpeg is not installed or cannot start.
    pub fn open(
        input: &str,
        width: u32,
        height: u32,
        fps: u32,
        fit: Fit,
        repeat: bool,
    ) -> Result<Self> {
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-hide_banner").args(["-loglevel", "error"]);
        if repeat {
            cmd.args(["-stream_loop", "-1"]);
        }
        cmd.args(["-i", input])
            .args(["-vf", &format!("{},fps={fps}", fit.filter(width, height))])
            .args(["-f", "rawvideo"])
            .args(["-pix_fmt", "rgb24"])
            .arg("-")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .stdin(Stdio::null());

        let mut child = cmd
            .spawn()
            .context("could not start ffmpeg (is it installed?)")?;
        let stdout = child.stdout.take().context("ffmpeg stdout not piped")?;
        Ok(Self {
            child,
            frames: raw::RawSource::new(stdout, width, height),
        })
    }
}

impl FrameSource for VideoSource {
    fn next_frame(&mut self, frame: &mut Frame) -> Result<bool> {
        self.frames
            .read_frame(frame)
            .context("read frame from ffmpeg")
    }
}

impl Drop for VideoSource {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Built-in patterns, for checking wiring and colour order without any media.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename_all = "lowercase"))]
pub enum Pattern {
    /// Vertical red, green and blue bands: confirms colour order.
    Rgb,
    /// A one-pixel white border with coloured corners: confirms geometry.
    Border,
    /// Horizontal red/green/blue stripes: confirms row mapping.
    Rows,
    /// A two-axis gradient.
    Gradient,
    /// Solid white: maximum current draw, useful for power checks.
    White,
    /// Per-module marks for setting up a wall: see [`calibration`].
    Calibrate,
}

impl FromStr for Pattern {
    type Err = UnknownName;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "rgb" => Ok(Self::Rgb),
            "border" => Ok(Self::Border),
            "rows" => Ok(Self::Rows),
            "gradient" => Ok(Self::Gradient),
            "white" => Ok(Self::White),
            "calibrate" => Ok(Self::Calibrate),
            _ => Err(UnknownName {
                what: "pattern",
                got: s.to_owned(),
                allowed: "rgb|border|rows|gradient|white|calibrate",
            }),
        }
    }
}

impl Pattern {
    /// The spelling [`FromStr`] reads back.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rgb => "rgb",
            Self::Border => "border",
            Self::Rows => "rows",
            Self::Gradient => "gradient",
            Self::White => "white",
            Self::Calibrate => "calibrate",
        }
    }
}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const WHITE: [u8; 3] = [255; 3];

/// Draw a built-in pattern at the given size.
#[must_use]
pub fn pattern(p: Pattern, width: u32, height: u32) -> Frame {
    let mut f = Frame::black(width, height);
    match p {
        Pattern::Rgb => {
            if width > 0 && height > 0 {
                // Bands at w/3 and 2w/3, not 2*(w/3); the boundary column is
                // pinned by rgb_pattern_puts_red_left_and_blue_right.
                for (x, px) in f.row_mut(0).iter_mut().enumerate() {
                    let x = x as u32;
                    *px = if x < width / 3 {
                        RED
                    } else if x < 2 * width / 3 {
                        GREEN
                    } else {
                        BLUE
                    };
                }
                let stride = (width as usize) * 3;
                let (first, rest) = f.as_bytes_mut().split_at_mut(stride);
                for row in rest.chunks_exact_mut(stride) {
                    row.copy_from_slice(first);
                }
            }
        }
        Pattern::Border => {
            let (right, bottom) = (width.saturating_sub(1), height.saturating_sub(1));
            for y in 0..height {
                let row = f.row_mut(y);
                if y == 0 || y == bottom {
                    row.fill(WHITE);
                } else if let Some((l, rest)) = row.split_first_mut() {
                    *l = WHITE;
                    if let Some(r) = rest.last_mut() {
                        *r = WHITE;
                    }
                }
            }
            f.set_pixel(0, 0, RED);
            f.set_pixel(right, 0, GREEN);
            f.set_pixel(0, bottom, BLUE);
        }
        Pattern::Rows => {
            for y in 0..height {
                let c = match y % 3 {
                    0 => RED,
                    1 => GREEN,
                    _ => BLUE,
                };
                f.row_mut(y).fill(c);
            }
        }
        Pattern::Gradient => {
            for y in 0..height {
                let g = (y * 255 / height.max(1)) as u8;
                for (x, px) in f.row_mut(y).iter_mut().enumerate() {
                    let r = (x as u32 * 255 / width.max(1)) as u8;
                    *px = [r, g, 128];
                }
            }
        }
        Pattern::White => {
            f.as_bytes_mut().fill(255);
        }
        // No module size here, so the whole frame is one module.
        Pattern::Calibrate => f = calibration(width, height, (width, height), (0, 0)),
    }
    f
}

/// Tile edges, alternating in a checkerboard so no two neighbouring modules
/// share an edge colour.
const CYAN: [u8; 3] = [0, 255, 255];
const YELLOW: [u8; 3] = [255, 255, 0];
/// The screen diagonal, in the one colour no per-module mark uses.
const MAGENTA: [u8; 3] = [255, 0, 255];

/// The tile index's top-left, inside the border and below the corner marks.
const LABEL: (u32, u32) = (2, 3);
/// Grey ramp steps across a tile, black to white.
const RAMP_STEPS: u32 = 16;
/// Grey ramp height in rows.
const RAMP_ROWS: u32 = 3;

/// The calibration pattern: one tile per module, plus the screen diagonal.
///
/// `width` x `height` is the rectangle being drawn and `origin` is its
/// top-left on the screen, so a card whose window starts elsewhere gets the
/// same tiles the whole-wall drawing would put there. The tile grid is
/// anchored at the screen origin; a rectangle that starts mid-module gets
/// only the tiles that start inside it. The diagonal runs from the screen's
/// top-left to this rectangle's bottom-right, so drawing the whole screen
/// (`origin` `(0, 0)`) gives the wall's corner-to-corner line.
#[must_use]
pub fn calibration(width: u32, height: u32, module: (u32, u32), origin: (u32, u32)) -> Frame {
    let mut f = Frame::black(width, height);
    let (mw, mh) = (module.0.max(1), module.1.max(1));
    let (ox, oy) = origin;
    for row in oy.div_ceil(mh)..(oy + height).div_ceil(mh) {
        for col in ox.div_ceil(mw)..(ox + width).div_ceil(mw) {
            calibration_tile(&mut f, (col * mw - ox, row * mh - oy), (mw, mh), (col, row));
        }
    }
    diagonal(&mut f, origin, (ox + width, oy + height));
    f
}

/// One module's marks, for a tile of `size` whose top-left is at `at` in `f`
/// and which is the `tile.0`-th across and `tile.1`-th down the wall.
///
/// A one-pixel border, cyan or yellow by `(col + row)` parity; three-pixel
/// corner marks inside it, red top-left, green top-right, blue bottom-left,
/// white bottom-right; a 16-step grey ramp, black on the left, across the
/// tile's middle rows; and the two-digit index `row * 10 + col`, in white,
/// at the top left. Anything that does not fit the tile is left out.
pub fn calibration_tile(f: &mut Frame, at: (u32, u32), size: (u32, u32), tile: (u32, u32)) {
    let ((x0, y0), (w, h)) = (at, size);
    if w == 0 || h == 0 {
        return;
    }
    let edge = if (tile.0 + tile.1).is_multiple_of(2) {
        CYAN
    } else {
        YELLOW
    };
    for x in 0..w {
        f.set_pixel(x0 + x, y0, edge);
        f.set_pixel(x0 + x, y0 + h - 1, edge);
    }
    for y in 0..h {
        f.set_pixel(x0, y0 + y, edge);
        f.set_pixel(x0 + w - 1, y0 + y, edge);
    }

    // The ramp first: on a short tile the label reads over it.
    if w >= 4 && h >= 2 + RAMP_ROWS {
        let inner = w - 2;
        let top = y0 + (h - RAMP_ROWS) / 2;
        for x in 0..inner {
            let grey = (x * RAMP_STEPS / inner * 255 / (RAMP_STEPS - 1)) as u8;
            for y in 0..RAMP_ROWS {
                f.set_pixel(x0 + 1 + x, top + y, [grey; 3]);
            }
        }
    }

    if w >= 6 && h >= 6 {
        let (r, b) = (w - 2, h - 2);
        for (x, y, c) in [
            (1, 1, RED),
            (2, 1, RED),
            (1, 2, RED),
            (r, 1, GREEN),
            (r - 1, 1, GREEN),
            (r, 2, GREEN),
            (1, b, BLUE),
            (2, b, BLUE),
            (1, b - 1, BLUE),
            (r, b, WHITE),
            (r - 1, b, WHITE),
            (r, b - 1, WHITE),
        ] {
            f.set_pixel(x0 + x, y0 + y, c);
        }
    }

    let label = (LABEL.0 + 2 * font::WIDTH + 2, LABEL.1 + font::HEIGHT + 1);
    if w >= label.0 && h >= label.1 {
        let n = (tile.1 % 10) * 10 + tile.0 % 10;
        font::draw(f, x0 + LABEL.0, y0 + LABEL.1, font::digit(n / 10), WHITE);
        let second = x0 + LABEL.0 + font::WIDTH + 1;
        font::draw(f, second, y0 + LABEL.1, font::digit(n % 10), WHITE);
    }
}

/// The wall's corner-to-corner diagonal on a whole-screen frame: a break at a
/// seam is a mapping or chain-order error.
pub fn calibration_diagonal(f: &mut Frame) {
    diagonal(f, (0, 0), (f.width, f.height));
}

/// The `screen`-sized wall's diagonal, drawn on the rectangle whose top-left
/// is `origin`. One pixel per step of the longer axis, so it never dots.
fn diagonal(f: &mut Frame, origin: (u32, u32), screen: (u32, u32)) {
    let (w, h) = screen;
    if w == 0 || h == 0 {
        return;
    }
    let steps = w.max(h);
    let last = (steps - 1).max(1);
    for i in 0..steps {
        let (x, y) = (i * (w - 1) / last, i * (h - 1) / last);
        if x >= origin.0 && y >= origin.1 {
            f.set_pixel(x - origin.0, y - origin.1, MAGENTA);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_are_the_requested_size() {
        let f = pattern(Pattern::Gradient, 16, 8);
        assert_eq!((f.width, f.height), (16, 8));
        assert_eq!(f.as_bytes().len(), 16 * 8 * 3);
    }

    #[test]
    fn rgb_pattern_puts_red_left_and_blue_right() {
        let f = pattern(Pattern::Rgb, 30, 2);
        assert_eq!(f.pixel(0, 0), [255, 0, 0]);
        assert_eq!(f.pixel(29, 0), [0, 0, 255]);
        // 2w/3 = 85 on the 128-wide panel; 2*(w/3) would give 84.
        let f = pattern(Pattern::Rgb, 128, 1);
        assert_eq!(f.pixel(84, 0), [0, 255, 0]);
        assert_eq!(f.pixel(85, 0), [0, 0, 255]);
    }

    #[test]
    fn border_pattern_marks_the_corners() {
        let f = pattern(Pattern::Border, 8, 4);
        assert_eq!(f.pixel(0, 0), [255, 0, 0]);
        assert_eq!(f.pixel(7, 0), [0, 255, 0]);
        assert_eq!(f.pixel(0, 3), [0, 0, 255]);
        assert_eq!(f.pixel(3, 2), [0, 0, 0]);
    }

    #[test]
    fn white_pattern_is_fully_lit() {
        let f = pattern(Pattern::White, 4, 4);
        assert!(f.as_bytes().iter().all(|&b| b == 255));
    }

    /// The per-pixel drawing this crate used before the row-wise version.
    fn pattern_per_pixel(p: Pattern, width: u32, height: u32) -> Frame {
        let mut f = Frame::black(width, height);
        for y in 0..height {
            for x in 0..width {
                let c = match p {
                    Pattern::Rgb if x < width / 3 => RED,
                    Pattern::Rgb if x < 2 * width / 3 => GREEN,
                    Pattern::Rgb => BLUE,
                    Pattern::Border if x == 0 || y == 0 || x == width - 1 || y == height - 1 => {
                        WHITE
                    }
                    Pattern::Border => continue,
                    Pattern::Rows => [RED, GREEN, BLUE][(y % 3) as usize],
                    Pattern::Gradient => [
                        (x * 255 / width.max(1)) as u8,
                        (y * 255 / height.max(1)) as u8,
                        128,
                    ],
                    Pattern::White => WHITE,
                    // Not a row-wise rewrite of anything; its own tests below.
                    Pattern::Calibrate => continue,
                };
                f.set_pixel(x, y, c);
            }
        }
        if p == Pattern::Border && width > 0 && height > 0 {
            f.set_pixel(0, 0, RED);
            f.set_pixel(width - 1, 0, GREEN);
            f.set_pixel(0, height - 1, BLUE);
        }
        f
    }

    const ALL: [Pattern; 5] = [
        Pattern::Rgb,
        Pattern::Border,
        Pattern::Rows,
        Pattern::Gradient,
        Pattern::White,
    ];

    #[test]
    fn every_pattern_matches_the_per_pixel_drawing() {
        for p in ALL {
            for (w, h) in [(128, 64), (30, 2), (1, 1), (7, 5), (1, 9), (0, 4), (4, 0), (0, 0)] {
                assert_eq!(pattern(p, w, h), pattern_per_pixel(p, w, h), "{p:?} {w}x{h}");
            }
        }
    }

    #[test]
    fn names_parse_case_insensitively_and_reject_strangers() {
        assert_eq!("RGB".parse::<Pattern>(), Ok(Pattern::Rgb));
        assert_eq!("contain".parse::<Fit>(), Ok(Fit::Contain));
        assert_eq!(
            "Blob".parse::<Pattern>().unwrap_err().to_string(),
            "unknown pattern \"Blob\" (rgb|border|rows|gradient|white|calibrate)"
        );
        assert_eq!(
            "fill".parse::<Fit>().unwrap_err().to_string(),
            "unknown fit \"fill\" (stretch|contain|cover)"
        );
    }

    /// Two by two 64x32 modules, the size a bench wall of four cards has.
    fn wall() -> Frame {
        calibration(128, 64, (64, 32), (0, 0))
    }

    #[test]
    fn every_tile_gets_a_border_and_no_two_neighbours_share_its_colour() {
        let f = wall();
        assert_eq!(f.pixel(5, 0), CYAN, "tile 00");
        assert_eq!(f.pixel(70, 0), YELLOW, "tile 01");
        assert_eq!(f.pixel(5, 32), YELLOW, "tile 10");
        assert_eq!(f.pixel(70, 32), CYAN, "tile 11");
        // The two columns either side of the vertical seam.
        assert_eq!(f.pixel(63, 10), CYAN);
        assert_eq!(f.pixel(64, 10), YELLOW);
        // The border is one pixel: inside it is the tile's own content.
        assert_ne!(f.pixel(5, 1), CYAN);
    }

    #[test]
    fn every_tile_is_labelled_with_its_row_and_column() {
        let f = wall();
        // Tile 01: '0' at (66, 3), '1' at (70, 3), both 3x5.
        assert_eq!(f.pixel(66, 3), WHITE);
        assert_eq!(f.pixel(67, 3), WHITE);
        assert_eq!(f.pixel(68, 3), WHITE);
        assert_ne!(f.pixel(67, 4), WHITE, "the nought is hollow");
        assert_ne!(f.pixel(70, 3), WHITE, "the one is one column in");
        assert_eq!(f.pixel(71, 3), WHITE);
        assert_eq!(f.pixel(70, 4), WHITE);
        // Tile 10 carries the other order: '1' then '0'.
        assert_eq!(f.pixel(3, 35), WHITE);
        assert_ne!(f.pixel(2, 35), WHITE);
        assert_eq!(f.pixel(6, 35), WHITE);
        assert_eq!(f.pixel(8, 35), WHITE);
    }

    #[test]
    fn each_tile_names_its_own_corners() {
        // Tile 10, the one no diagonal pixel lands in.
        let f = wall();
        for (x, y) in [(1, 33), (2, 33), (1, 34)] {
            assert_eq!(f.pixel(x, y), RED, "top-left {x},{y}");
        }
        for (x, y) in [(62, 33), (61, 33), (62, 34)] {
            assert_eq!(f.pixel(x, y), GREEN, "top-right {x},{y}");
        }
        for (x, y) in [(1, 62), (2, 62), (1, 61)] {
            assert_eq!(f.pixel(x, y), BLUE, "bottom-left {x},{y}");
        }
        for (x, y) in [(62, 62), (61, 62), (62, 61)] {
            assert_eq!(f.pixel(x, y), WHITE, "bottom-right {x},{y}");
        }
    }

    #[test]
    fn the_grey_ramp_climbs_in_sixteen_steps_across_the_middle_rows() {
        let f = wall();
        // Rows 14..17 of the tile: (32 - 3) / 2 = 14.
        assert_eq!(f.pixel(1, 15), [0; 3], "the ramp starts black");
        assert_eq!(f.pixel(62, 15), [255; 3], "and ends white");
        assert_eq!(f.pixel(62, 13), [0; 3], "above it the tile is black");
        assert_eq!(f.pixel(62, 17), [0; 3], "and below it too");

        // The diagonal crosses the ramp; the greys it leaves are 16 levels,
        // never descending.
        let mut greys: Vec<u8> = (1..63)
            .map(|x| f.pixel(x, 15))
            .filter(|p| p[0] == p[1] && p[1] == p[2])
            .map(|p| p[0])
            .collect();
        assert!(greys.windows(2).all(|w| w[0] <= w[1]), "{greys:?}");
        greys.dedup();
        assert_eq!(greys.len(), RAMP_STEPS as usize, "{greys:?}");
        assert_eq!((greys[0], *greys.last().unwrap()), (0, 255));
    }

    #[test]
    fn the_diagonal_crosses_the_seams_unbroken() {
        let f = wall();
        assert_eq!(f.pixel(0, 0), MAGENTA, "it starts at the top-left corner");
        assert_eq!(f.pixel(127, 63), MAGENTA, "and ends at the bottom-right");
        // The vertical seam is at x = 64: the line is on both sides of it.
        assert_eq!(f.pixel(63, 31), MAGENTA);
        assert_eq!(f.pixel(64, 31), MAGENTA);
        // And it carries on over the horizontal seam at y = 32.
        assert_eq!(f.pixel(65, 32), MAGENTA);
        // One pixel per column, so nothing dots.
        for x in 0..128 {
            assert!(
                (0..64).any(|y| f.pixel(x, y) == MAGENTA),
                "column {x} has no diagonal pixel"
            );
        }
    }

    #[test]
    fn an_origin_draws_the_tiles_that_a_whole_wall_would_put_there() {
        // The right-hand card of the wall above, drawn on its own.
        let card = calibration(64, 64, (64, 32), (64, 0));
        let whole = wall();
        for y in 0..64 {
            for x in 0..64 {
                let (a, b) = (card.pixel(x, y), whole.pixel(64 + x, y));
                // The diagonal differs: the card's runs to its own corner.
                if a != MAGENTA && b != MAGENTA {
                    assert_eq!(a, b, "{x},{y}");
                }
            }
        }
        assert_eq!(card.pixel(5, 0), YELLOW, "tile 01 keeps its edge colour");
        assert_eq!(card.pixel(2, 3), WHITE, "and its label");
    }

    #[test]
    fn without_a_module_size_the_whole_frame_is_one_module() {
        assert_eq!(
            pattern(Pattern::Calibrate, 128, 64),
            calibration(128, 64, (128, 64), (0, 0))
        );
    }

    #[test]
    fn a_tile_smaller_than_its_marks_draws_what_fits() {
        // No panic, and the border still says which tile it is.
        for (w, h) in [(0, 0), (1, 1), (2, 3), (5, 5), (9, 8)] {
            let f = calibration(w, h, (w.max(1), h.max(1)), (0, 0));
            assert_eq!((f.width, f.height), (w, h));
        }
        let f = calibration(4, 4, (4, 4), (0, 0));
        assert_eq!(f.pixel(1, 0), CYAN);
    }

    #[test]
    fn fit_filters_mention_the_target_size() {
        assert!(Fit::Stretch.filter(128, 64).contains("128:64"));
        assert!(Fit::Contain.filter(128, 64).contains("pad=128:64"));
        assert!(Fit::Cover.filter(128, 64).contains("crop=128:64"));
    }
}
