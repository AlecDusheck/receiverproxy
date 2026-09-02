//! The receiver's on-board EEPROM (cabinet identity) and the frames that
//! write it.
//!
//! Writes go one record at a time at the record's own address and length: the
//! card silently ignores a write that crosses a record boundary
//! (`docs/eeprom-map.md`). Frames are type `0x1900`, addressed to
//! [`BROADCAST`]; opcode `0x85` writes, `0x87` commits the EEPROM to its flash
//! mirror.

use super::{frame, BROADCAST};

/// One EEPROM record: address, length, name.
pub struct Record {
    pub addr: u16,
    pub len: u16,
    pub name: &'static str,
}

/// Every record the vendor device library reads or writes
/// (`docs/eeprom-map.md`). Lengths are load-bearing.
pub const RECORDS: &[Record] = &[
    Record { addr: 0x000, len: 2, name: "debug bytes" },
    Record { addr: 0x002, len: 42, name: "control area" },
    Record { addr: 0x02c, len: 18, name: "colour-gamut coefficients" },
    Record { addr: 0x03e, len: 1, name: "gamut-adjust enable" },
    Record { addr: 0x040, len: 1, name: "calibration status" },
    Record { addr: 0x041, len: 1, name: "no-input show info" },
    Record { addr: 0x042, len: 1, name: "turn-on screen show" },
    Record { addr: 0x043, len: 3, name: "white-balance adjust" },
    Record { addr: 0x04b, len: 1, name: "calibration-coefficient source" },
    Record { addr: 0x04c, len: 1, name: "seam enable" },
    Record { addr: 0x04d, len: 9, name: "unresolved (wall dims in the corpus)" },
    Record { addr: 0x056, len: 3, name: "void-line info" },
    Record { addr: 0x059, len: 1, name: "receiver-card light" },
    Record { addr: 0x05a, len: 20, name: "receiver card name" },
    Record { addr: 0x06e, len: 1, name: "14-way open flag" },
    Record { addr: 0x06f, len: 1, name: "gamma-calibration status" },
    Record { addr: 0x070, len: 1, name: "ROE current/bright flag" },
    Record { addr: 0x072, len: 1, name: "virtual-pixel param" },
    Record { addr: 0x076, len: 1, name: "full-screen seam-factor enable" },
    Record { addr: 0x077, len: 1, name: "four-deseam" },
    Record { addr: 0x07b, len: 1, name: "plus-module 7-way adjust enable" },
    Record { addr: 0x07c, len: 1, name: "double-cali chroma enable" },
    Record { addr: 0x07d, len: 1, name: "plus low-bright cali enable" },
    Record { addr: 0x07e, len: 1, name: "double-cali enable" },
    Record { addr: 0x07f, len: 2, name: "double-cali threshold" },
    Record { addr: 0x092, len: 32, name: "control-area blob, high half" },
    Record { addr: 0x0b2, len: 1, name: "parameter switch" },
    Record { addr: 0x0b3, len: 1, name: "plus-chip low-bright cali enable" },
    Record { addr: 0x0b4, len: 3, name: "plus-chip low-bright uniformity" },
    Record { addr: 0x0c1, len: 12, name: "GX custom FCCL" },
    Record { addr: 0x0c8, len: 1, name: "plus temperature-control enable" },
    Record { addr: 0x0ce, len: 16, name: "double-cali threshold (long form)" },
    Record { addr: 0x0e1, len: 1, name: "plus-module current-adjust enable" },
    Record { addr: 0x0f4, len: 2, name: "preset temperature / ROE fan" },
    Record { addr: 0x0f6, len: 1, name: "power-off bright coefficient" },
    Record { addr: 0x0f7, len: 2, name: "EMC info" },
    Record { addr: 0x0f9, len: 1, name: "module power switch" },
    Record { addr: 0x0fa, len: 1, name: "current/bright flag" },
    Record { addr: 0x0fd, len: 1, name: "screen-shake param" },
];

/// The 42-byte control-area record for a cabinet at `(x, y)` in the whole
/// screen, `w` by `h` pixels: `startX, startY, endX, endY` big-endian, a
/// reserved word, then a zero blob (the factory value).
#[must_use]
pub fn control_area(x: u16, y: u16, w: u16, h: u16) -> [u8; 42] {
    let mut r = [0u8; 42];
    r[0..2].copy_from_slice(&x.to_be_bytes());
    r[2..4].copy_from_slice(&y.to_be_bytes());
    r[4..6].copy_from_slice(&(x + w).to_be_bytes());
    r[6..8].copy_from_slice(&(y + h).to_be_bytes());
    r
}

/// Decode a control-area record: `(startX, startY, endX, endY)`.
#[must_use]
pub fn parse_control_area(r: &[u8]) -> Option<(u16, u16, u16, u16)> {
    if r.len() < 8 {
        return None;
    }
    let u = |i: usize| u16::from_be_bytes([r[i], r[i + 1]]);
    Some((u(0), u(2), u(4), u(6)))
}

fn payload(opcode: u8, addr: u32, data: &[u8]) -> Vec<u8> {
    // Payload length is max(0x80, len + 0x12), as the vendor builds it.
    let n = (data.len() + 0x12).max(0x80);
    let mut p = vec![0u8; n];
    p[1..3].copy_from_slice(&BROADCAST.to_be_bytes());
    p[3] = opcode;
    p[4..8].copy_from_slice(&addr.to_be_bytes());
    p[8..12].copy_from_slice(&(data.len() as u32).to_be_bytes());
    p[12..12 + data.len()].copy_from_slice(data);
    p
}

/// Write one record. `data.len()` must be the record's own length.
#[must_use]
pub fn write(addr: u16, data: &[u8]) -> Vec<u8> {
    frame([0x19, 0x00], &payload(0x85, u32::from(addr), data))
}

/// Commit the EEPROM to its flash mirror (`SaveEepromFlash`).
#[must_use]
pub fn save() -> Vec<u8> {
    frame([0x19, 0x00], &payload(0x87, 0, &[]))
}

/// `ReLoadLocalParam` as the vendor sends it after an EEPROM save: opcode
/// 0x77, flags `01 01 00` (the flash-save reloads live in `discovery`).
#[must_use]
pub fn reload() -> Vec<u8> {
    crate::discovery::command(BROADCAST, 0x77, &[0x01, 0x01, 0x00])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_area_is_big_endian_corners() {
        let r = control_area(0, 0, 128, 64);
        assert_eq!(&r[..8], &[0, 0, 0, 0, 0, 128, 0, 64]);
        assert_eq!(parse_control_area(&r), Some((0, 0, 128, 64)));
        let r = control_area(128, 64, 128, 64);
        assert_eq!(parse_control_area(&r), Some((128, 64, 256, 128)));
    }

    #[test]
    fn write_frame_matches_the_vendor_layout() {
        let f = write(0x02, &[0u8; 42]);
        assert_eq!(&f[12..14], &[0x19, 0x00]);
        assert_eq!(&f[15..17], &[0xff, 0xff], "broadcast index");
        assert_eq!(f[17], 0x85);
        assert_eq!(&f[18..22], &[0, 0, 0, 2]);
        assert_eq!(&f[22..26], &[0, 0, 0, 42]);
        assert_eq!(f.len(), 14 + 0x80, "payload padded to 0x80");
    }

    #[test]
    fn reload_matches_the_vendor_bytes() {
        let f = reload();
        assert_eq!(f.len(), 140);
        assert_eq!(&f[12..14], &[0x06, 0x00]);
        assert_eq!(&f[15..18], &[0xff, 0xff, 0x77]);
        assert_eq!(&f[22..25], &[0x01, 0x01, 0x00]);
        assert!(f[25..].iter().all(|&b| b == 0));
    }

    #[test]
    fn records_are_in_address_order() {
        // Only ordering: the vendor's map overlaps once (0x0C1 x12 spans 0x0C8).
        for w in RECORDS.windows(2) {
            assert!(w[1].addr > w[0].addr, "{} out of order", w[1].name);
        }
    }
}
