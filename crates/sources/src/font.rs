//! The 3x5 glyph table the calibration pattern and the demos draw with.
//! Small enough to read on a P2.5 module from across the room.

use wall::Frame;

/// Glyph width in pixels; bit 2 of each row byte is the left column.
pub const WIDTH: u32 = 3;
/// Glyph height in pixels, one byte per row.
pub const HEIGHT: u32 = 5;

/// The ten digits, then `A C E F H J L P U Y Z + - =`.
pub const GLYPHS: [[u8; 5]; 24] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
    [0b010, 0b101, 0b111, 0b101, 0b101],
    [0b111, 0b100, 0b100, 0b100, 0b111],
    [0b111, 0b100, 0b111, 0b100, 0b111],
    [0b111, 0b100, 0b111, 0b100, 0b100],
    [0b101, 0b101, 0b111, 0b101, 0b101],
    [0b001, 0b001, 0b001, 0b101, 0b111],
    [0b100, 0b100, 0b100, 0b100, 0b111],
    [0b111, 0b101, 0b111, 0b100, 0b100],
    [0b101, 0b101, 0b101, 0b101, 0b111],
    [0b101, 0b101, 0b010, 0b010, 0b010],
    [0b111, 0b001, 0b010, 0b100, 0b111],
    [0b000, 0b010, 0b111, 0b010, 0b000],
    [0b000, 0b000, 0b111, 0b000, 0b000],
    [0b000, 0b111, 0b000, 0b111, 0b000],
];

/// The glyph for one decimal digit; anything above 9 reads as `0`.
#[must_use]
pub fn digit(d: u32) -> [u8; 5] {
    GLYPHS[if d < 10 { d as usize } else { 0 }]
}

/// Draw one glyph with its top-left at `(x, y)`. Pixels off the frame are
/// dropped, so a glyph at the edge is clipped rather than wrapped.
pub fn draw(f: &mut Frame, x: u32, y: u32, glyph: [u8; 5], colour: [u8; 3]) {
    for (dy, bits) in glyph.iter().enumerate() {
        for dx in 0..WIDTH {
            if (bits >> (WIDTH - 1 - dx)) & 1 != 0 {
                f.set_pixel(x + dx, y + dy as u32, colour);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digit_lands_where_it_is_put() {
        let mut f = Frame::black(8, 8);
        draw(&mut f, 1, 2, digit(1), [255; 3]);
        // '1': one pixel, then the flag, then the stem, then the base.
        assert_eq!(f.pixel(2, 2), [255; 3]);
        assert_eq!(f.pixel(1, 2), [0; 3]);
        assert_eq!(f.pixel(1, 3), [255; 3]);
        assert_eq!(f.row(6), &[[0; 3], [255; 3], [255; 3], [255; 3], [0; 3], [0; 3], [0; 3], [0; 3]]);
    }

    #[test]
    fn digits_are_the_first_ten_glyphs_and_anything_else_is_zero() {
        for d in 0..10 {
            assert_eq!(digit(d), GLYPHS[d as usize]);
        }
        assert_eq!(digit(10), GLYPHS[0]);
    }

    #[test]
    fn a_glyph_at_the_edge_is_clipped() {
        let mut f = Frame::black(2, 2);
        draw(&mut f, 1, 1, digit(8), [255; 3]);
        assert_eq!(f.pixel(1, 1), [255; 3]);
        assert_eq!(f.pixel(0, 0), [0; 3]);
    }
}
