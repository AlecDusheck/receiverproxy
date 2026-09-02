//! Frame sources: anything that can produce a stream of images to display.
//!
//! Video decoding is delegated to `ffmpeg`, which is asked for raw RGB24 frames
//! already scaled to the wall's size, so nothing here has to understand
//! container or codec formats.

use anyhow::{Context, Result};
use e120_canvas::Frame;
use std::io::Read;
use std::process::{Child, Command, Stdio};

/// Something that produces frames until it runs out.
pub trait FrameSource {
    /// The next frame, or `None` at the end of the stream.
    ///
    /// # Errors
    /// Fails if the underlying source cannot be read.
    fn next_frame(&mut self) -> Result<Option<Frame>>;
}

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

        let child = cmd
            .spawn()
            .context("could not start ffmpeg (is it installed?)")?;
        Ok(Self {
            child,
            width,
            height,
        })
    }
}

impl FrameSource for VideoSource {
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        let Some(stdout) = self.child.stdout.as_mut() else {
            return Ok(None);
        };
        let mut buf = vec![0; (self.width as usize) * (self.height as usize) * 3];
        match stdout.read_exact(&mut buf) {
            Ok(()) => Ok(Some(
                Frame::from_rgb(self.width, self.height, buf).map_err(|e| anyhow::anyhow!(e))?,
            )),
            // A short read means the stream ended.
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e).context("read frame from ffmpeg"),
        }
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

impl std::str::FromStr for Pattern {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "rgb" => Ok(Self::Rgb),
            "border" => Ok(Self::Border),
            "rows" => Ok(Self::Rows),
            "gradient" => Ok(Self::Gradient),
            "white" => Ok(Self::White),
            _ => Err(format!(
                "unknown pattern {s:?} (rgb|border|rows|gradient|white)"
            )),
        }
    }
}

/// Draw a built-in pattern at the given size.
#[must_use]
pub fn pattern(p: Pattern, width: u32, height: u32) -> Frame {
    let mut f = Frame::black(width, height);
    match p {
        Pattern::Rgb => {
            for y in 0..height {
                for x in 0..width {
                    // Bands at w/3 and 2w/3 (not 2*(w/3)): what `e120 test rgb`
                    // has always drawn, so the boundary column does not move.
                    let c = if x < width / 3 {
                        [255, 0, 0]
                    } else if x < 2 * width / 3 {
                        [0, 255, 0]
                    } else {
                        [0, 0, 255]
                    };
                    f.set_pixel(x, y, c);
                }
            }
        }
        Pattern::Border => {
            for y in 0..height {
                for x in 0..width {
                    if x == 0 || y == 0 || x == width - 1 || y == height - 1 {
                        f.set_pixel(x, y, [255, 255, 255]);
                    }
                }
            }
            f.set_pixel(0, 0, [255, 0, 0]);
            f.set_pixel(width - 1, 0, [0, 255, 0]);
            f.set_pixel(0, height - 1, [0, 0, 255]);
        }
        Pattern::Rows => {
            for y in 0..height {
                let c = match y % 3 {
                    0 => [255, 0, 0],
                    1 => [0, 255, 0],
                    _ => [0, 0, 255],
                };
                for x in 0..width {
                    f.set_pixel(x, y, c);
                }
            }
        }
        Pattern::Gradient => {
            for y in 0..height {
                for x in 0..width {
                    let r = (x * 255 / width.max(1)) as u8;
                    let g = (y * 255 / height.max(1)) as u8;
                    f.set_pixel(x, y, [r, g, 128]);
                }
            }
        }
        Pattern::White => {
            f.data.fill(255);
        }
    }
    f
}

/// Yields one frame forever: a still image held on the wall.
pub struct StillSource {
    frame: Frame,
}

impl StillSource {
    #[must_use]
    pub const fn new(frame: Frame) -> Self {
        Self { frame }
    }
}

impl FrameSource for StillSource {
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        Ok(Some(self.frame.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_are_the_requested_size() {
        let f = pattern(Pattern::Gradient, 16, 8);
        assert_eq!((f.width, f.height), (16, 8));
        assert_eq!(f.data.len(), 16 * 8 * 3);
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
        assert!(f.data.iter().all(|&b| b == 255));
    }

    #[test]
    fn a_still_source_repeats_its_frame() {
        let mut s = StillSource::new(pattern(Pattern::White, 2, 2));
        assert!(s.next_frame().unwrap().is_some());
        assert!(s.next_frame().unwrap().is_some());
    }

    #[test]
    fn fit_filters_mention_the_target_size() {
        assert!(Fit::Stretch.filter(128, 64).contains("128:64"));
        assert!(Fit::Contain.filter(128, 64).contains("pad=128:64"));
        assert!(Fit::Cover.filter(128, 64).contains("crop=128:64"));
    }
}
