//! Pixel data, latching, and brightness: the frames sent every refresh.
//!
//! Layouts follow FPP's ColorLight-5a-75 output, byte-verified against the
//! vendor DLL (`docs/pixel-protocol.md` §3). The type is one byte at frame
//! offset 12 and the first data byte rides at offset 13, inside what would be
//! the second EtherType byte; shifting data by one turns the panel into a 5 Hz
//! strobe.

use super::{write_header, HEADER_LEN};

/// Max pixels per row packet (FPP's CL_MAX_PIXL_PER_PACKET, hard-coded in the vendor DLL).
pub const MAX_PIXELS_PER_PACKET: usize = 497;

/// Display/vsync frame: wire type 0x01, data[0] = 0x07 ("PC sender").
///
/// Latches the previously sent row data onto the panel and carries the master
/// brightness at frame offset 35 and three channel gains at 38..41. Callers
/// send three per refresh (`docs/rendering-recipe.md`).
#[must_use]
pub fn sync(brightness: u8) -> [u8; 112] {
    let mut f = [0u8; 112];
    write_header(&mut f, [0x01, 0x07]);
    f[35] = brightness;
    f[36] = 0x05;
    // The vendor derives the three gains from separate bytes of its brightness
    // block; that derivation is unresolved (docs/pixel-protocol.md §2.2), so
    // they follow the master value.
    f[38..41].fill(brightness);
    f
}

/// Brightness frame: wire type 0x0A, data = [b, b, b, 0xFF] starting at frame
/// offset 13 (so the first copy of b is the second EtherType byte).
#[must_use]
pub fn brightness(b: u8) -> [u8; 77] {
    let mut f = [0u8; 77];
    write_header(&mut f, [0x0a, b]);
    f[14] = b;
    f[15] = b;
    f[16] = 0xff;
    f
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorOrder {
    Rgb,
    Bgr,
    Grb,
}

impl ColorOrder {
    const NAMES: [(&'static str, Self); 3] = [("rgb", Self::Rgb), ("bgr", Self::Bgr), ("grb", Self::Grb)];

    /// Index into an `[r, g, b]` pixel for each wire channel.
    const fn permutation(self) -> [usize; 3] {
        match self {
            Self::Rgb => [0, 1, 2],
            Self::Bgr => [2, 1, 0],
            Self::Grb => [1, 0, 2],
        }
    }
}

impl std::str::FromStr for ColorOrder {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(s))
            .map(|&(_, order)| order)
            .ok_or_else(|| format!("unknown color order {s:?} (rgb|bgr|grb)"))
    }
}

/// Bytes between the frame start and the first pixel of a row packet.
const ROW_PIXELS_AT: usize = HEADER_LEN + 7;

/// Pixel row frame: wire type 0x55, then data at offset 13:
/// [row MSB, row LSB, offs MSB, offs LSB, count MSB, count LSB, 0x08, 0x88,
/// pixels...]
#[must_use]
pub fn pixel_row(row: u16, pixel_offset: u16, rgb: &[[u8; 3]], order: ColorOrder) -> Vec<u8> {
    let mut f = Vec::new();
    pixel_row_into(&mut f, row, pixel_offset, rgb, order);
    f
}

/// [`pixel_row`] written into a reused buffer, so a refresh loop allocates
/// nothing per packet. `buf` is cleared first.
pub fn pixel_row_into(buf: &mut Vec<u8>, row: u16, pixel_offset: u16, rgb: &[[u8; 3]], order: ColorOrder) {
    let count = rgb.len() as u16;
    buf.clear();
    buf.resize(ROW_PIXELS_AT + rgb.len() * 3, 0);
    write_header(buf, [0x55, (row >> 8) as u8]);
    buf[14] = (row & 0xff) as u8;
    buf[15..17].copy_from_slice(&pixel_offset.to_be_bytes());
    buf[17..19].copy_from_slice(&count.to_be_bytes());
    buf[19] = 0x08;
    buf[20] = 0x88;
    let [a, b, c] = order.permutation();
    for (dst, px) in buf[ROW_PIXELS_AT..].chunks_exact_mut(3).zip(rgb) {
        dst[0] = px[a];
        dst[1] = px[b];
        dst[2] = px[c];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_rows_follow_the_fpp_layout() {
        // FPP: data[0] = row MSB lives at frame offset 13.
        let px = [[1u8, 2, 3], [4, 5, 6]];
        let f = pixel_row(0x0102, 5, &px, ColorOrder::Rgb);
        assert_eq!(f[12], 0x55, "type byte");
        assert_eq!(&f[13..15], &[0x01, 0x02], "row u16 BE starting at offset 13");
        assert_eq!(&f[15..17], &5u16.to_be_bytes());
        assert_eq!(&f[17..19], &2u16.to_be_bytes());
        assert_eq!(&f[19..21], &[0x08, 0x88]);
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
    fn a_reused_buffer_gives_the_same_frame() {
        let px: Vec<[u8; 3]> = (0..MAX_PIXELS_PER_PACKET as u16)
            .map(|i| [i as u8, (i >> 3) as u8, !(i as u8)])
            .collect();
        let mut buf = vec![0xeeu8; 4096];
        for order in [ColorOrder::Rgb, ColorOrder::Bgr, ColorOrder::Grb] {
            pixel_row_into(&mut buf, 0x0203, 497, &px, order);
            assert_eq!(buf, pixel_row(0x0203, 497, &px, order));
            assert_eq!(buf.len(), 21 + 3 * MAX_PIXELS_PER_PACKET);
        }
    }

    #[test]
    fn colour_order_parses_case_insensitively() {
        assert_eq!("BGR".parse::<ColorOrder>(), Ok(ColorOrder::Bgr));
        assert_eq!("rgb".parse::<ColorOrder>(), Ok(ColorOrder::Rgb));
        assert_eq!("grb".parse::<ColorOrder>(), Ok(ColorOrder::Grb));
        assert!("rbg".parse::<ColorOrder>().is_err());
    }

    #[test]
    fn sync_frame_matches_fpp_byte_for_byte() {
        // FPP: 112-byte packet, data[0]=0x07 at offset 13, brightness at
        // data[22] (offset 35) and data[25..28] (offsets 38..41), 0x05 at 36.
        let f = sync(0x7f);
        assert_eq!(f.len(), 112);
        assert_eq!(&f[12..14], &[0x01, 0x07]);
        assert_eq!(f[35], 0x7f);
        assert_eq!(f[36], 0x05);
        assert_eq!(&f[38..41], &[0x7f; 3]);
    }

    #[test]
    fn brightness_frame_matches_fpp() {
        // FPP: 77-byte packet, data[0..4] = [b, b, b, 0xFF] from offset 13.
        let f = brightness(0x40);
        assert_eq!(f.len(), 77);
        assert_eq!(f[12], 0x0a);
        assert_eq!(&f[13..17], &[0x40, 0x40, 0x40, 0xff]);
    }
}
