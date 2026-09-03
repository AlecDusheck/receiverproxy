//! Panel topology: one logical image mapped onto a wall of panels, any size,
//! rotation or mirroring, across any number of receiving cards. Rendering
//! yields one screen-sized framebuffer: every card on the chain receives every
//! row and keeps the pixels inside its own window (docs/receiver-identity.md),
//! so a receiver's position here is the position it was provisioned with.
//! The wire protocol is not involved here.

use serde::{Deserialize, Serialize};

/// An RGB8 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Always exactly `width * height * 3` bytes, row major.
    data: Vec<u8>,
}

impl Frame {
    #[must_use]
    pub fn black(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width as usize) * (height as usize) * 3],
        }
    }

    /// Wrap existing RGB8 bytes.
    ///
    /// # Errors
    /// Fails if `data` is not exactly `width * height * 3` bytes.
    pub fn from_rgb(width: u32, height: u32, data: Vec<u8>) -> Result<Self, FrameError> {
        let want = (width as usize) * (height as usize) * 3;
        if data.len() == want {
            Ok(Self {
                width,
                height,
                data,
            })
        } else {
            Err(FrameError::WrongSize {
                got: data.len(),
                want,
            })
        }
    }

    /// The raw RGB8 bytes, row major.
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// The raw RGB8 bytes, row major, for in-place fills.
    #[inline]
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    #[inline]
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    /// One row of pixels. Panics if `y` is off the frame.
    #[inline]
    #[must_use]
    pub fn row(&self, y: u32) -> &[[u8; 3]] {
        let stride = (self.width as usize) * 3;
        let start = (y as usize) * stride;
        self.data[start..start + stride].as_chunks::<3>().0
    }

    /// One row of pixels, writable. Panics if `y` is off the frame.
    #[inline]
    pub fn row_mut(&mut self, y: u32) -> &mut [[u8; 3]] {
        let stride = (self.width as usize) * 3;
        let start = (y as usize) * stride;
        self.data[start..start + stride].as_chunks_mut::<3>().0
    }

    /// Every row, top to bottom.
    pub fn rows(&self) -> impl Iterator<Item = &[[u8; 3]]> + '_ {
        (0..self.height).map(|y| self.row(y))
    }

    /// The pixel at `(x, y)`; black when off the frame.
    #[inline]
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        if x >= self.width || y >= self.height {
            return [0; 3];
        }
        self.row(y)[x as usize]
    }

    /// Set the pixel at `(x, y)`; a no-op when off the frame.
    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, px: [u8; 3]) {
        if x >= self.width || y >= self.height {
            return;
        }
        self.row_mut(y)[x as usize] = px;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    WrongSize { got: usize, want: usize },
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSize { got, want } => {
                write!(f, "frame data is {got} bytes, expected {want}")
            }
        }
    }
}

impl std::error::Error for FrameError {}

/// How a panel is physically mounted relative to the image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum Rotation {
    #[default]
    None,
    /// Quarter turn clockwise.
    Cw90,
    /// Quarter turn counter-clockwise.
    Ccw90,
    Rot180,
}

/// One panel, positioned on the wall and assigned to a receiver.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Panel {
    /// Which receiving card drives this panel.
    pub receiver: u16,
    /// Where the panel's top-left sits in the receiver's own pixel space.
    #[serde(default)]
    pub receiver_x: u32,
    #[serde(default)]
    pub receiver_y: u32,
    /// Where the panel's top-left sits on the logical canvas.
    pub x: u32,
    pub y: u32,
    /// Panel size as it appears on the canvas, after rotation.
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub rotation: Rotation,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub flip_y: bool,
    /// The most brightness this panel may be sent, 0-255; the wall's
    /// brightness is clamped to the lowest cap among its panels.
    #[serde(default = "full")]
    pub max_brightness: u8,
}

const fn full() -> u8 {
    255
}

/// Where a panel's canvas rectangle lands in receiver space, as an affine map:
/// `origin + local_x * col_step + local_y * row_step`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placement {
    origin: (i64, i64),
    col_step: (i64, i64),
    row_step: (i64, i64),
}

impl Placement {
    /// True when receiver rows are canvas rows, in the same direction, so a
    /// panel row is one contiguous copy.
    const fn is_row_copy(self) -> bool {
        matches!(self.col_step, (1, 0)) && matches!(self.row_step, (0, 1))
    }
}

impl Panel {
    /// Map a point inside this panel's canvas rectangle to the pixel within the
    /// receiver's framebuffer that lights it. Requires a non-empty panel.
    fn receiver_coords(&self, local_x: u32, local_y: u32) -> (u32, u32) {
        let (max_x, max_y) = (self.width - 1, self.height - 1);
        // Mirror in canvas space, then undo the mounting rotation.
        let lx = if self.flip_x {
            max_x - local_x
        } else {
            local_x
        };
        let ly = if self.flip_y {
            max_y - local_y
        } else {
            local_y
        };
        let (px, py) = match self.rotation {
            Rotation::None => (lx, ly),
            Rotation::Cw90 => (ly, max_x - lx),
            Rotation::Ccw90 => (max_y - ly, lx),
            Rotation::Rot180 => (max_x - lx, max_y - ly),
        };
        (self.receiver_x + px, self.receiver_y + py)
    }

    /// Map a point inside this panel's canvas rectangle to the screen pixel
    /// that lights it, for a panel on the receiver at `(rx, ry)`.
    fn screen_coords(&self, receiver: (u32, u32), local_x: u32, local_y: u32) -> (u32, u32) {
        let (px, py) = self.receiver_coords(local_x, local_y);
        (receiver.0 + px, receiver.1 + py)
    }

    /// The mapping as an affine map; exact because every rotation/flip
    /// combination is a rigid motion (`every_mounting_matches_the_per_pixel_mapping`).
    fn placement(&self, receiver: (u32, u32)) -> Placement {
        let at = |x, y| {
            let (sx, sy) = self.screen_coords(receiver, x, y);
            (i64::from(sx), i64::from(sy))
        };
        let origin = at(0, 0);
        let step = |p: (i64, i64)| (p.0 - origin.0, p.1 - origin.1);
        // A one-pixel-wide or -high panel never takes the step; (0, 0) keeps
        // the arithmetic in range.
        let col_step = if self.width > 1 {
            step(at(1, 0))
        } else {
            (0, 0)
        };
        let row_step = if self.height > 1 {
            step(at(0, 1))
        } else {
            (0, 0)
        };
        Placement {
            origin,
            col_step,
            row_step,
        }
    }

    /// The panel's size in its own, unrotated pixel space.
    const fn native_size(&self) -> (u32, u32) {
        match self.rotation {
            Rotation::None | Rotation::Rot180 => (self.width, self.height),
            Rotation::Cw90 | Rotation::Ccw90 => (self.height, self.width),
        }
    }

    /// Copy this panel's canvas rectangle from `src` onto the screen `dst`,
    /// for a panel on the receiver at `receiver`. Off-frame source reads
    /// black; off-screen destination writes are dropped.
    fn blit(&self, receiver: (u32, u32), src: &Frame, dst: &mut Frame) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        let place = self.placement(receiver);
        if place.is_row_copy() {
            self.blit_rows(src, dst, place.origin);
            return;
        }
        let (ox, oy) = place.origin;
        let (cx, cy) = place.col_step;
        let (rx, ry) = place.row_step;
        for ly in 0..self.height {
            let sy = self.y + ly;
            let (mut x, mut y) = (ox + i64::from(ly) * rx, oy + i64::from(ly) * ry);
            for lx in 0..self.width {
                let px = src.pixel(self.x + lx, sy);
                dst.set_pixel(x as u32, y as u32, px);
                x += cx;
                y += cy;
            }
        }
    }

    fn blit_rows(&self, src: &Frame, dst: &mut Frame, origin: (i64, i64)) {
        let (rx, ry) = (origin.0 as u32, origin.1 as u32);
        // Clip to the destination; what the source does not cover is written black.
        let dst_w = self.width.min(dst.width.saturating_sub(rx)) as usize;
        let dst_rows = self.height.min(dst.height.saturating_sub(ry));
        let src_w = dst_w.min(src.width.saturating_sub(self.x) as usize);
        let src_rows = dst_rows.min(src.height.saturating_sub(self.y));
        let (sx, dx) = (self.x as usize, rx as usize);
        for ly in 0..dst_rows {
            let row = &mut dst.row_mut(ry + ly)[dx..dx + dst_w];
            if ly < src_rows {
                row[..src_w].copy_from_slice(&src.row(self.y + ly)[sx..sx + src_w]);
                row[src_w..].fill([0; 3]);
            } else {
                row.fill([0; 3]);
            }
        }
    }
}

/// A receiving card: where its window sits on the screen and how big it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Receiver {
    pub index: u16,
    /// The card's top-left on the screen: the same numbers given to
    /// `rxp provision --position x,y`, which is what the card's EEPROM
    /// control area keeps of the stream.
    #[serde(default)]
    pub x: u32,
    #[serde(default)]
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A complete wall.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub receivers: Vec<Receiver>,
    pub panels: Vec<Panel>,
    /// The most brightness the wall may be sent, 0-255.
    #[serde(default = "full")]
    pub max_brightness: u8,
}

/// Why a [`Canvas`] cannot be driven, one line per problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutError(pub Vec<String>);

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "canvas is not valid:\n  {}", self.0.join("\n  "))
    }
}

impl std::error::Error for LayoutError {}

impl Canvas {
    /// The common case: a single panel on a single receiver.
    #[must_use]
    pub fn single(width: u32, height: u32) -> Self {
        Self::grid(width, height, 1, 1)
    }

    /// A regular grid of identical panels across one receiver.
    #[must_use]
    pub fn grid(panel_w: u32, panel_h: u32, cols: u32, rows: u32) -> Self {
        let (width, height) = (panel_w * cols, panel_h * rows);
        let panels = (0..rows)
            .flat_map(|row| {
                (0..cols).map(move |col| Panel {
                    receiver: 0,
                    receiver_x: col * panel_w,
                    receiver_y: row * panel_h,
                    x: col * panel_w,
                    y: row * panel_h,
                    width: panel_w,
                    height: panel_h,
                    rotation: Rotation::None,
                    flip_x: false,
                    flip_y: false,
                    max_brightness: 255,
                })
            })
            .collect();
        Self {
            width,
            height,
            receivers: vec![Receiver {
                index: 0,
                x: 0,
                y: 0,
                width,
                height,
            }],
            panels,
            max_brightness: 255,
        }
    }

    /// A regular grid of identical panels, one receiving card per panel,
    /// cards numbered row-major from 0.
    #[must_use]
    pub fn cards(panel_w: u32, panel_h: u32, cols: u32, rows: u32) -> Self {
        let mut canvas = Self::grid(panel_w, panel_h, cols, rows);
        canvas.receivers = canvas
            .panels
            .iter()
            .enumerate()
            .map(|(i, p)| Receiver {
                index: i as u16,
                x: p.x,
                y: p.y,
                width: panel_w,
                height: panel_h,
            })
            .collect();
        for (i, p) in canvas.panels.iter_mut().enumerate() {
            p.receiver = i as u16;
            p.receiver_x = 0;
            p.receiver_y = 0;
        }
        canvas
    }

    /// Check the description is self-consistent before anything is driven.
    ///
    /// # Errors
    /// Reports overlapping receivers and panels that fall off the canvas,
    /// name an unknown receiver, or exceed their receiver's window.
    ///
    /// A receiver's rectangle is in the screen space the cards are addressed
    /// in, not on the canvas: a rotated wall has a 256x64 card behind a
    /// 64x256 canvas, so receivers are not checked against the canvas.
    pub fn validate(&self) -> Result<(), LayoutError> {
        let mut problems = Vec::new();
        for r in &self.receivers {
            // Rows and pixel offsets are u16 on the wire.
            if r.x + r.width > u32::from(u16::MAX) || r.y + r.height > u32::from(u16::MAX) {
                problems.push(format!(
                    "receiver {} at ({}, {}) size {}x{} exceeds the 65535 px screen space",
                    r.index, r.x, r.y, r.width, r.height
                ));
            }
        }
        for (i, a) in self.receivers.iter().enumerate() {
            for b in &self.receivers[i + 1..] {
                let apart = a.x + a.width <= b.x
                    || b.x + b.width <= a.x
                    || a.y + a.height <= b.y
                    || b.y + b.height <= a.y;
                if !apart {
                    problems.push(format!(
                        "receivers {} and {} overlap in screen space",
                        a.index, b.index
                    ));
                }
            }
        }
        for (i, p) in self.panels.iter().enumerate() {
            if p.x + p.width > self.width || p.y + p.height > self.height {
                problems.push(format!(
                    "panel {i} at ({}, {}) size {}x{} extends past the {}x{} canvas",
                    p.x, p.y, p.width, p.height, self.width, self.height
                ));
            }
            let Some(r) = self.receivers.iter().find(|r| r.index == p.receiver) else {
                problems.push(format!(
                    "panel {i} names receiver {}, which is not defined",
                    p.receiver
                ));
                continue;
            };
            let (nw, nh) = p.native_size();
            if p.receiver_x + nw > r.width || p.receiver_y + nh > r.height {
                problems.push(format!(
                    "panel {i} occupies ({}, {}) size {nw}x{nh} on receiver {}, which is only {}x{}",
                    p.receiver_x, p.receiver_y, r.index, r.width, r.height
                ));
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(LayoutError(problems))
        }
    }

    /// The brightness the wall may be sent: its own cap, or the lowest of
    /// its panels' when one is lower.
    #[must_use]
    pub fn brightness_cap(&self) -> u8 {
        self.panels
            .iter()
            .fold(self.max_brightness, |cap, p| cap.min(p.max_brightness))
    }

    /// `value`, held under [`brightness_cap`](Self::brightness_cap).
    #[must_use]
    pub fn clamp_brightness(&self, value: u8) -> u8 {
        value.min(self.brightness_cap())
    }

    /// The screen: the space the receivers' windows tile, which is what the
    /// cards are addressed in. It need not match the canvas: two panels
    /// mounted on their side make a 64x256 canvas on a 256x64 screen.
    #[must_use]
    pub fn screen_size(&self) -> (u32, u32) {
        self.receivers.iter().fold((0, 0), |(w, h), r| {
            (w.max(r.x + r.width), h.max(r.y + r.height))
        })
    }

    /// A black framebuffer the size of the screen.
    #[must_use]
    pub fn screen_frame(&self) -> Frame {
        let (w, h) = self.screen_size();
        Frame::black(w, h)
    }

    /// Map one canvas image onto the screen: each panel's pixels land where
    /// its receiver's window shows them.
    #[must_use]
    pub fn render(&self, src: &Frame) -> Frame {
        let mut out = self.screen_frame();
        self.render_into(src, &mut out);
        out
    }

    /// [`render`](Self::render) into a caller-owned screen frame, so a refresh
    /// loop allocates nothing. `out` is cleared to black and reused when it is
    /// the screen size, replaced otherwise.
    pub fn render_into(&self, src: &Frame, out: &mut Frame) {
        if (out.width, out.height) == self.screen_size() {
            out.data.fill(0);
        } else {
            *out = self.screen_frame();
        }
        for panel in &self.panels {
            let Some(r) = self.receivers.iter().find(|r| r.index == panel.receiver) else {
                continue;
            };
            panel.blit((r.x, r.y), src, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Frame {
        let mut f = Frame::black(w, h);
        for y in 0..h {
            for x in 0..w {
                f.set_pixel(x, y, [x as u8, y as u8, 0]);
            }
        }
        f
    }

    /// Reference per-pixel mapping the row-copy and affine paths must match.
    fn render_per_pixel(canvas: &Canvas, src: &Frame) -> Frame {
        let mut out = canvas.screen_frame();
        for panel in &canvas.panels {
            let Some(r) = canvas.receivers.iter().find(|r| r.index == panel.receiver) else {
                continue;
            };
            for ly in 0..panel.height {
                for lx in 0..panel.width {
                    let px = src.pixel(panel.x + lx, panel.y + ly);
                    let (sx, sy) = panel.screen_coords((r.x, r.y), lx, ly);
                    out.set_pixel(sx, sy, px);
                }
            }
        }
        out
    }

    #[test]
    fn a_single_panel_passes_the_image_through_unchanged() {
        let canvas = Canvas::single(8, 4);
        let src = gradient(8, 4);
        assert_eq!(canvas.render(&src), src);
    }

    #[test]
    fn single_is_a_one_by_one_grid() {
        assert_eq!(Canvas::single(8, 4), Canvas::grid(8, 4, 1, 1));
    }

    #[test]
    fn a_grid_tiles_panels_across_the_canvas() {
        let canvas = Canvas::grid(4, 2, 2, 2);
        assert_eq!((canvas.width, canvas.height), (8, 4));
        assert_eq!(canvas.panels.len(), 4);
        canvas.validate().unwrap();
        assert_eq!(canvas.render(&gradient(8, 4)), gradient(8, 4));
    }

    #[test]
    fn cards_put_one_receiver_under_each_panel_at_its_screen_position() {
        let canvas = Canvas::cards(4, 2, 3, 2);
        canvas.validate().unwrap();
        assert_eq!(canvas.receivers.len(), 6);
        let r = canvas.receivers[4];
        assert_eq!((r.index, r.x, r.y, r.width, r.height), (4, 4, 2, 4, 2));
        let p = &canvas.panels[4];
        assert_eq!(
            (p.receiver, p.receiver_x, p.receiver_y, p.x, p.y),
            (4, 0, 0, 4, 2)
        );
        assert_eq!(canvas.render(&gradient(12, 4)), gradient(12, 4));
    }

    #[test]
    fn a_receiver_position_defaults_to_the_origin_in_layout_files() {
        let r: Receiver = serde_json::from_str(r#"{"index":3,"width":8,"height":4}"#).unwrap();
        assert_eq!((r.x, r.y), (0, 0));
    }

    #[test]
    fn rows_are_contiguous_pixel_slices() {
        let f = gradient(4, 2);
        assert_eq!(f.row(1), &[[0, 1, 0], [1, 1, 0], [2, 1, 0], [3, 1, 0]]);
        assert_eq!(f.rows().count(), 2);
        assert_eq!(f.as_bytes().len(), 4 * 2 * 3);
        assert_eq!(f.clone().into_bytes(), f.as_bytes());
    }

    #[test]
    fn pixels_off_the_frame_read_black_and_ignore_writes() {
        let mut f = gradient(4, 2);
        assert_eq!(f.pixel(4, 0), [0; 3]);
        assert_eq!(f.pixel(0, 2), [0; 3]);
        f.set_pixel(4, 0, [9; 3]);
        f.set_pixel(0, 2, [9; 3]);
        assert_eq!(f, gradient(4, 2));
    }

    #[test]
    fn rotation_maps_corners_where_expected() {
        // Mounted 90 degrees clockwise: canvas rectangle 2x4, panel itself 4x2.
        let canvas = Canvas {
            width: 2,
            height: 4,
            receivers: vec![Receiver {
                index: 0,
                x: 0,
                y: 0,
                width: 4,
                height: 2,
            }],
            panels: vec![Panel {
                receiver: 0,
                receiver_x: 0,
                receiver_y: 0,
                x: 0,
                y: 0,
                width: 2,
                height: 4,
                rotation: Rotation::Cw90,
                flip_x: false,
                flip_y: false,
                max_brightness: 255,
            }],
            max_brightness: 255,
        };
        // A rotated wall is exactly this shape: a 4x2 card behind a 2x4
        // canvas, which is valid.
        assert!(canvas.validate().is_ok());

        let mut src = Frame::black(2, 4);
        src.set_pixel(0, 0, [255, 0, 0]); // canvas top-left
        let out = canvas.render(&src);
        // Canvas top-left lands at the panel's bottom-left.
        assert_eq!(out.pixel(0, 1), [255, 0, 0]);
    }

    #[test]
    fn flipping_mirrors_the_image() {
        let mut canvas = Canvas::single(4, 1);
        canvas.panels[0].flip_x = true;
        let mut src = Frame::black(4, 1);
        src.set_pixel(0, 0, [1, 2, 3]);
        assert_eq!(canvas.render(&src).pixel(3, 0), [1, 2, 3]);
    }

    #[test]
    fn every_mounting_matches_the_per_pixel_mapping() {
        // Two receivers at different screen positions, offset panels, odd
        // sizes, every rotation and flip, plus a panel hanging off both frames.
        let src = gradient(23, 17);
        let rotations = [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Ccw90,
            Rotation::Rot180,
        ];
        for rotation in rotations {
            for (flip_x, flip_y) in [(false, false), (true, false), (false, true), (true, true)] {
                let (w, h) = (7, 5);
                let (nw, nh) = match rotation {
                    Rotation::None | Rotation::Rot180 => (w, h),
                    Rotation::Cw90 | Rotation::Ccw90 => (h, w),
                };
                let panel = |receiver, receiver_x, receiver_y, x, y| Panel {
                    receiver,
                    receiver_x,
                    receiver_y,
                    x,
                    y,
                    width: w,
                    height: h,
                    rotation,
                    flip_x,
                    flip_y,
                    max_brightness: 255,
                };
                let canvas = Canvas {
                    width: 23,
                    height: 17,
                    receivers: vec![
                        Receiver {
                            index: 0,
                            x: 0,
                            y: 0,
                            width: nw + 3,
                            height: nh + 2,
                        },
                        Receiver {
                            index: 5,
                            x: 11,
                            y: 6,
                            width: nw + 1,
                            height: nh,
                        },
                    ],
                    panels: vec![
                        panel(0, 3, 2, 1, 4),
                        panel(5, 1, 0, 9, 11),
                        panel(5, 3, 2, 20, 15),
                    ],
                    max_brightness: 255,
                };
                assert_eq!(
                    canvas.render(&src),
                    render_per_pixel(&canvas, &src),
                    "{rotation:?} flip_x={flip_x} flip_y={flip_y}"
                );
            }
        }
    }

    #[test]
    fn render_into_reuses_a_screen_sized_frame() {
        let canvas = Canvas::grid(4, 2, 2, 2);
        let mut out = Frame::black(1, 1);
        canvas.render_into(&gradient(8, 4), &mut out);
        assert_eq!(out, canvas.render(&gradient(8, 4)));
        let before = out.as_bytes().as_ptr();
        canvas.render_into(&Frame::black(8, 4), &mut out);
        assert_eq!(out.as_bytes().as_ptr(), before);
        assert_eq!(out, Frame::black(8, 4));
    }

    #[test]
    fn two_receivers_side_by_side_render_at_their_screen_positions() {
        let canvas = Canvas {
            width: 8,
            height: 2,
            receivers: vec![
                Receiver {
                    index: 0,
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 2,
                },
                Receiver {
                    index: 1,
                    x: 4,
                    y: 0,
                    width: 4,
                    height: 2,
                },
            ],
            panels: vec![
                Panel {
                    receiver: 0,
                    receiver_x: 0,
                    receiver_y: 0,
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 2,
                    rotation: Rotation::None,
                    flip_x: false,
                    flip_y: false,
                    max_brightness: 255,
                },
                Panel {
                    receiver: 1,
                    receiver_x: 0,
                    receiver_y: 0,
                    x: 4,
                    y: 0,
                    width: 4,
                    height: 2,
                    rotation: Rotation::None,
                    flip_x: false,
                    flip_y: false,
                    max_brightness: 255,
                },
            ],
            max_brightness: 255,
        };
        canvas.validate().unwrap();
        let src = gradient(8, 2);
        assert_eq!(canvas.render(&src), src);

        // Swap the cards' screen positions: the image swaps halves.
        let mut swapped = canvas;
        swapped.receivers[0].x = 4;
        swapped.receivers[1].x = 0;
        let out = swapped.render(&src);
        assert_eq!(out.pixel(0, 0), src.pixel(4, 0));
        assert_eq!(out.pixel(4, 1), src.pixel(0, 1));
    }

    #[test]
    fn validation_rejects_a_panel_that_hangs_off_the_canvas() {
        let mut canvas = Canvas::single(8, 4);
        canvas.panels[0].x = 4;
        let err = canvas.validate().unwrap_err();
        assert!(err
            .to_string()
            .starts_with("canvas is not valid:\n  panel 0 at (4, 0)"));
    }

    #[test]
    fn validation_rejects_receivers_that_overlap_in_screen_space() {
        let mut canvas = Canvas::cards(4, 2, 2, 1);
        // Card 1 pulled back over card 0's window.
        canvas.receivers[1].x = 2;
        let err = canvas.validate().unwrap_err().to_string();
        assert!(
            err.contains("receivers 0 and 1 overlap in screen space"),
            "{err}"
        );
    }

    #[test]
    fn validation_rejects_a_panel_that_hangs_off_its_receiver() {
        let mut canvas = Canvas::cards(4, 2, 2, 1);
        canvas.panels[1].receiver_x = 1;
        let err = canvas.validate().unwrap_err().to_string();
        assert!(
            err.contains("panel 1 occupies (1, 0) size 4x2 on receiver 1, which is only 4x2"),
            "{err}"
        );
    }

    #[test]
    fn validation_rejects_an_unknown_receiver() {
        let mut canvas = Canvas::single(8, 4);
        canvas.panels[0].receiver = 7;
        assert!(canvas.validate().is_err());
    }
}
