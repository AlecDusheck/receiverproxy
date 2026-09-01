//! Pixel data, latching, and brightness: the frames sent every refresh.

use super::frame;

/// Max pixels per row packet (keeps the frame under the 1500-byte MTU).
pub const MAX_PIXELS_PER_PACKET: usize = 497;

/// Display/vsync frame: type 0x0107, 98 bytes. Latches the previously sent
/// row data onto the panel and sets overall brightness.
pub fn sync(brightness: u8) -> Vec<u8> {
    let mut p = [0u8; 98];
    p[21] = brightness;
    p[22] = 0x05;
    p[24] = brightness;
    p[25] = brightness;
    p[26] = brightness;
    frame([0x01, 0x07], &p)
}
/// Brightness frame: type 0x0A<brightness>, 63-byte payload.
pub fn brightness(b: u8) -> Vec<u8> {
    let mut p = [0u8; 63];
    p[0] = b;
    p[1] = b;
    p[2] = 0xff;
    frame([0x0a, b], &p)
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorOrder {
    Rgb,
    Bgr,
    Grb,
}
/// Pixel row frame: type 0x55<row MSB>, payload:
/// [row LSB, offs MSB, offs LSB, count MSB, count LSB, 0x08, 0x88, pixels...]
pub fn pixel_row(row: u16, pixel_offset: u16, rgb: &[[u8; 3]], order: ColorOrder) -> Vec<u8> {
    let count = rgb.len() as u16;
    let mut p = Vec::with_capacity(7 + rgb.len() * 3);
    p.push((row & 0xff) as u8);
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
    frame([0x55, (row >> 8) as u8], &p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_rows_carry_the_row_in_the_type_and_payload() {
        let px = [[1u8, 2, 3], [4, 5, 6]];
        let f = pixel_row(0x0102, 5, &px, ColorOrder::Rgb);
        assert_eq!(&f[12..14], &[0x55, 0x01]); // type carries the row high byte
        assert_eq!(f[14], 0x02); // row low byte
        assert_eq!(&f[15..17], &5u16.to_be_bytes());
        assert_eq!(&f[17..19], &2u16.to_be_bytes());
        assert_eq!(&f[21..27], &[1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn colour_order_reorders_the_channels() {
        let px = [[1u8, 2, 3]];
        let bgr = pixel_row(0, 0, &px, ColorOrder::Bgr);
        assert_eq!(&bgr[21..24], &[3, 2, 1]);
        let grb = pixel_row(0, 0, &px, ColorOrder::Grb);
        assert_eq!(&grb[21..24], &[2, 1, 3]);
    }

    #[test]
    fn sync_frame_carries_brightness_where_the_card_expects_it() {
        let f = sync(0x7f);
        assert_eq!(&f[12..14], &[0x01, 0x07]);
        assert_eq!(f.len(), 14 + 98);
        assert_eq!(f[14 + 21], 0x7f);
        assert_eq!(f[14 + 22], 0x05);
    }
}
