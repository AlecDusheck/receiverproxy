//! Frame sources: ffmpeg video decoded into wall-sized frames, and built-in
//! test patterns.
//!
//! Video decoding is delegated to `ffmpeg`, which is asked for raw RGB24 frames
//! already scaled to the wall's size, so nothing here has to understand
//! container or codec formats.

use anyhow::{Context, Result};
use e120_canvas::Frame;
use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::str::FromStr;

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
    stdout: ChildStdout,
    width: u32,
    height: u32,
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
            stdout,
            width,
            height,
        })
    }

    /// Fill `frame` with the next decoded image, resizing it to the source's
    /// size if needed. Returns `false` at the end of the stream.
    ///
    /// # Errors
    /// Fails if the ffmpeg pipe cannot be read.
    pub fn next_frame(&mut self, frame: &mut Frame) -> Result<bool> {
        if (frame.width, frame.height) != (self.width, self.height) {
            *frame = Frame::black(self.width, self.height);
        }
        read_frame(&mut self.stdout, frame)
    }
}

/// Read one raw RGB24 image into `frame`; `false` on a clean end of stream.
fn read_frame(r: &mut impl Read, frame: &mut Frame) -> Result<bool> {
    match r.read_exact(frame.as_bytes_mut()) {
        Ok(()) => Ok(true),
        // A short read means the stream ended.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e).context("read frame from ffmpeg"),
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
            _ => Err(UnknownName {
                what: "pattern",
                got: s.to_owned(),
                allowed: "rgb|border|rows|gradient|white",
            }),
        }
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
                // Bands at w/3 and 2w/3 (not 2*(w/3)): what `e120 test rgb`
                // has always drawn, so the boundary column does not move.
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
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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
        // The second boundary is at 2w/3, not 2*(w/3): on the 128-wide panel
        // column 84 is green and 85 is blue.
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
        // Interior stays dark.
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
            "unknown pattern \"Blob\" (rgb|border|rows|gradient|white)"
        );
        assert_eq!(
            "fill".parse::<Fit>().unwrap_err().to_string(),
            "unknown fit \"fill\" (stretch|contain|cover)"
        );
    }

    #[test]
    fn frames_are_read_back_to_back_until_the_stream_ends() {
        let (w, h) = (4, 2);
        let a = pattern(Pattern::Rgb, w, h);
        let b = pattern(Pattern::Gradient, w, h);
        let mut bytes = a.as_bytes().to_vec();
        bytes.extend_from_slice(b.as_bytes());
        bytes.extend_from_slice(&[7; 5]); // a truncated trailing frame
        let mut r = Cursor::new(bytes);
        let mut f = Frame::black(w, h);
        assert!(read_frame(&mut r, &mut f).unwrap());
        assert_eq!(f, a);
        assert!(read_frame(&mut r, &mut f).unwrap());
        assert_eq!(f, b);
        assert!(!read_frame(&mut r, &mut f).unwrap());
    }

    #[test]
    fn fit_filters_mention_the_target_size() {
        assert!(Fit::Stretch.filter(128, 64).contains("128:64"));
        assert!(Fit::Contain.filter(128, 64).contains("pad=128:64"));
        assert!(Fit::Cover.filter(128, 64).contains("crop=128:64"));
    }
}
