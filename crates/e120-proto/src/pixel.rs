//! Pixel data, latching, and brightness: the frames sent every refresh.
//!
//! Layouts follow FPP's ColorLight-5a-75 output, which drives this hardware
//! family in production: the type is a single byte at frame offset 12, offset
//! 13 is zero, and the payload begins at offset 14. Treating the type as two
//! meaningful bytes shifts every field by one and the card silently ignores
//! the frame — both earlier "corrections" of these layouts did exactly that.

use super::frame;

/// Max pixels per row packet (keeps the frame under the 1500-byte MTU).
pub const MAX_PIXELS_PER_PACKET: usize = 490;

/// Display/vsync frame: type 0x01, payload opens 0x07 ("PC sender"). Latches
/// the previously sent row data onto the panel and sets overall brightness.
pub fn sync(brightness: u8) -> Vec<u8> {
    let mut p = [0u8; 99];
    p[0] = 0x07;
    p[22] = brightness;
    p[23] = 0x05;
    p[25] = brightness;
    p[26] = brightness;
    p[27] = brightness;
    frame([0x01, 0x00], &p)
}
/// Brightness frame: type 0x0A, payload [b, b, b, 0xff].
pub fn brightness(b: u8) -> Vec<u8> {
    let mut p = [0u8; 64];
    p[0] = b;
    p[1] = b;
    p[2] = b;
    p[3] = 0xff;
    frame([0x0a, 0x00], &p)
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorOrder {
    Rgb,
    Bgr,
    Grb,
}
/// Pixel row frame: type 0x55, payload:
/// [row MSB, row LSB, offs MSB, offs LSB, count MSB, count LSB, 0x08, 0x88,
/// pixels...]
pub fn pixel_row(row: u16, pixel_offset: u16, rgb: &[[u8; 3]], order: ColorOrder) -> Vec<u8> {
    let count = rgb.len() as u16;
    let mut p = Vec::with_capacity(8 + rgb.len() * 3);
    p.extend_from_slice(&row.to_be_bytes());
    p.extend_from_slice(&pixel_offset.to_be_bytes());
    p.extend_from_slice(&count.to_be_bytes());
    p.push(0x08);
    p.push(0x88);
    for px in rgb {
        let [r, g, b] = *px;
        match order {
            ColorOrder::Rgb => p.extend_from_slice(&[r, g, b]),
            ColorOrder::Bgr => p.extend_from_slice(&[b, g, r]),
            ColorOrder::Grb => p.extend_from_slice(&[g, r, b]),
        }
    }
    frame([0x55, 0x00], &p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_rows_follow_the_fpp_layout() {
        let px = [[1u8, 2, 3], [4, 5, 6]];
        let f = pixel_row(0x0102, 5, &px, ColorOrder::Rgb);
        assert_eq!(&f[12..14], &[0x55, 0x00], "one type byte, then zero");
        assert_eq!(&f[14..16], &[0x01, 0x02], "16-bit row opens the payload");
        assert_eq!(&f[16..18], &5u16.to_be_bytes());
        assert_eq!(&f[18..20], &2u16.to_be_bytes());
        assert_eq!(&f[20..22], &[0x08, 0x88]);
        assert_eq!(&f[22..28], &[1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn colour_order_reorders_the_channels() {
        let px = [[1u8, 2, 3]];
        let bgr = pixel_row(0, 0, &px, ColorOrder::Bgr);
        assert_eq!(&bgr[22..25], &[3, 2, 1]);
        let grb = pixel_row(0, 0, &px, ColorOrder::Grb);
        assert_eq!(&grb[22..25], &[2, 1, 3]);
    }

    #[test]
    fn sync_frame_carries_brightness_where_fpp_puts_it() {
        let f = sync(0x7f);
        assert_eq!(&f[12..14], &[0x01, 0x00]);
        assert_eq!(f.len(), 14 + 99);
        assert_eq!(f[14], 0x07, "PC sender marker opens the payload");
        assert_eq!(f[14 + 22], 0x7f);
        assert_eq!(f[14 + 23], 0x05);
        assert_eq!(&f[14 + 25..14 + 28], &[0x7f; 3]);
    }

    #[test]
    fn brightness_frame_is_three_copies_and_a_terminator() {
        let f = brightness(0x40);
        assert_eq!(&f[12..14], &[0x0a, 0x00]);
        assert_eq!(&f[14..18], &[0x40, 0x40, 0x40, 0xff]);
    }
}
