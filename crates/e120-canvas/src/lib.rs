//! Panel topology: mapping one logical image onto an arbitrary wall of panels.
//!
//! A wall is described independently of the protocol that drives it. Panels may
//! be any size, in any arrangement, rotated or mirrored, spread across any
//! number of receiving cards. Rendering a frame produces one framebuffer per
//! receiver, in that receiver's own pixel coordinates, ready to be sent.

use serde::{Deserialize, Serialize};

/// An RGB8 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// `width * height * 3` bytes, row major.
    pub data: Vec<u8>,
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

    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        if x >= self.width || y >= self.height {
            return [0; 3];
        }
        let i = ((y as usize) * (self.width as usize) + (x as usize)) * 3;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, px: [u8; 3]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y as usize) * (self.width as usize) + (x as usize)) * 3;
        self.data[i] = px[0];
        self.data[i + 1] = px[1];
        self.data[i + 2] = px[2];
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
}

impl Panel {
    /// Map a point inside this panel's canvas rectangle to the pixel within the
    /// receiver's framebuffer that lights it.
    fn receiver_coords(&self, local_x: u32, local_y: u32) -> (u32, u32) {
        // Apply mirroring in canvas space first.
        let lx = if self.flip_x {
            self.width.saturating_sub(1) - local_x
        } else {
            local_x
        };
        let ly = if self.flip_y {
            self.height.saturating_sub(1) - local_y
        } else {
            local_y
        };

        // Then undo the mounting rotation to reach panel-native coordinates.
        let (px, py) = match self.rotation {
            Rotation::None => (lx, ly),
            Rotation::Cw90 => (ly, self.width.saturating_sub(1) - lx),
            Rotation::Ccw90 => (self.height.saturating_sub(1) - ly, lx),
            Rotation::Rot180 => (
                self.width.saturating_sub(1) - lx,
                self.height.saturating_sub(1) - ly,
            ),
        };
        (self.receiver_x + px, self.receiver_y + py)
    }

    /// The panel's size in its own, unrotated pixel space.
    const fn native_size(&self) -> (u32, u32) {
        match self.rotation {
            Rotation::None | Rotation::Rot180 => (self.width, self.height),
            Rotation::Cw90 | Rotation::Ccw90 => (self.height, self.width),
        }
    }
}

/// A receiving card and the extent of its pixel space.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receiver {
    pub index: u16,
    pub width: u32,
    pub height: u32,
}

/// A complete wall.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub receivers: Vec<Receiver>,
    pub panels: Vec<Panel>,
}

impl Canvas {
    /// The common case: a single panel on a single receiver.
    #[must_use]
    pub fn single(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            receivers: vec![Receiver {
                index: 0,
                width,
                height,
            }],
            panels: vec![Panel {
                receiver: 0,
                receiver_x: 0,
                receiver_y: 0,
                x: 0,
                y: 0,
                width,
                height,
                rotation: Rotation::None,
                flip_x: false,
                flip_y: false,
            }],
        }
    }

    /// A regular grid of identical panels across one receiver.
    #[must_use]
    pub fn grid(panel_w: u32, panel_h: u32, cols: u32, rows: u32) -> Self {
        let mut panels = Vec::new();
        for row in 0..rows {
            for col in 0..cols {
                panels.push(Panel {
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
                });
            }
        }
        Self {
            width: panel_w * cols,
            height: panel_h * rows,
            receivers: vec![Receiver {
                index: 0,
                width: panel_w * cols,
                height: panel_h * rows,
            }],
            panels,
        }
    }

    /// Check the description is self-consistent before anything is driven.
    ///
    /// # Errors
    /// Reports panels that fall off the canvas, name an unknown receiver, or
    /// exceed their receiver's pixel space.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut problems = Vec::new();
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
            Err(problems)
        }
    }

    /// Split one canvas image into a framebuffer per receiver.
    #[must_use]
    pub fn render(&self, src: &Frame) -> Vec<(u16, Frame)> {
        let mut out: Vec<(u16, Frame)> = self
            .receivers
            .iter()
            .map(|r| (r.index, Frame::black(r.width, r.height)))
            .collect();

        for panel in &self.panels {
            let Some((_, dst)) = out.iter_mut().find(|(i, _)| *i == panel.receiver) else {
                continue;
            };
            for local_y in 0..panel.height {
                for local_x in 0..panel.width {
                    let px = src.pixel(panel.x + local_x, panel.y + local_y);
                    let (rx, ry) = panel.receiver_coords(local_x, local_y);
                    dst.set_pixel(rx, ry, px);
                }
            }
        }
        out
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

    #[test]
    fn a_single_panel_passes_the_image_through_unchanged() {
        let canvas = Canvas::single(8, 4);
        let src = gradient(8, 4);
        let out = canvas.render(&src);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, src);
    }

    #[test]
    fn a_grid_tiles_panels_across_the_canvas() {
        let canvas = Canvas::grid(4, 2, 2, 2);
        assert_eq!((canvas.width, canvas.height), (8, 4));
        assert_eq!(canvas.panels.len(), 4);
        canvas.validate().unwrap();
        let out = canvas.render(&gradient(8, 4));
        assert_eq!(out[0].1, gradient(8, 4));
    }

    #[test]
    fn rotation_maps_corners_where_expected() {
        // A panel mounted rotated 90 degrees clockwise: the canvas rectangle is
        // 2 wide by 4 high, the panel itself is 4 by 2.
        let canvas = Canvas {
            width: 2,
            height: 4,
            receivers: vec![Receiver {
                index: 0,
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
            }],
        };
        canvas.validate().unwrap();

        let mut src = Frame::black(2, 4);
        src.set_pixel(0, 0, [255, 0, 0]); // canvas top-left
        let out = canvas.render(&src);
        // Turning the panel clockwise sends the canvas top-left to the panel's
        // bottom-left.
        assert_eq!(out[0].1.pixel(0, 1), [255, 0, 0]);
    }

    #[test]
    fn flipping_mirrors_the_image() {
        let mut canvas = Canvas::single(4, 1);
        canvas.panels[0].flip_x = true;
        let mut src = Frame::black(4, 1);
        src.set_pixel(0, 0, [1, 2, 3]);
        let out = canvas.render(&src);
        assert_eq!(out[0].1.pixel(3, 0), [1, 2, 3]);
    }

    #[test]
    fn two_receivers_each_get_their_own_half() {
        let canvas = Canvas {
            width: 8,
            height: 2,
            receivers: vec![
                Receiver {
                    index: 0,
                    width: 4,
                    height: 2,
                },
                Receiver {
                    index: 1,
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
                },
            ],
        };
        canvas.validate().unwrap();
        let src = gradient(8, 2);
        let out = canvas.render(&src);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].1.pixel(0, 0), src.pixel(4, 0));
    }

    #[test]
    fn validation_rejects_a_panel_that_hangs_off_the_canvas() {
        let mut canvas = Canvas::single(8, 4);
        canvas.panels[0].x = 4;
        assert!(canvas.validate().is_err());
    }

    #[test]
    fn validation_rejects_an_unknown_receiver() {
        let mut canvas = Canvas::single(8, 4);
        canvas.panels[0].receiver = 7;
        assert!(canvas.validate().is_err());
    }
}
