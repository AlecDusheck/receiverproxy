//! The receiver's EEPROM (cabinet identity) and the type-0x1900 frames that
//! write it: opcode 0x85 writes one record, 0x87 commits to the flash mirror.
//!
//! A write must use a record's own address and length; the card silently
//! ignores one that crosses a record boundary (`docs/eeprom-map.md`).

use super::{command, frame_with, indexed, BROADCAST};

/// One EEPROM record: address, length, name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub addr: u16,
    pub len: u16,
    pub name: &'static str,
}

/// Every record the vendor device library reads or writes
/// (`docs/eeprom-map.md`); the lengths are what the card accepts.
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

/// The cabinet's rectangle in the whole screen, from the first 8 bytes of the
/// 42-byte control-area record: `startX, startY, endX, endY`, end exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlArea {
    pub start_x: u16,
    pub start_y: u16,
    pub end_x: u16,
    pub end_y: u16,
}

impl ControlArea {
    /// A cabinet at `(x, y)`, `w` by `h` pixels.
    #[must_use]
    pub const fn for_cabinet(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { start_x: x, start_y: y, end_x: x + w, end_y: y + h }
    }

    /// The record: four big-endian corners, then zeros (the factory value).
    #[must_use]
    pub fn to_record(self) -> [u8; 42] {
        let mut r = [0u8; 42];
        r[0..2].copy_from_slice(&self.start_x.to_be_bytes());
        r[2..4].copy_from_slice(&self.start_y.to_be_bytes());
        r[4..6].copy_from_slice(&self.end_x.to_be_bytes());
        r[6..8].copy_from_slice(&self.end_y.to_be_bytes());
        r
    }

    #[must_use]
    pub fn parse(r: &[u8]) -> Option<Self> {
        if r.len() < 8 {
            return None;
        }
        let u = |i: usize| u16::from_be_bytes([r[i], r[i + 1]]);
        Some(Self { start_x: u(0), start_y: u(2), end_x: u(4), end_y: u(6) })
    }
}

/// [`ControlArea::for_cabinet`] as the 42-byte record.
#[must_use]
pub fn control_area(x: u16, y: u16, w: u16, h: u16) -> [u8; 42] {
    ControlArea::for_cabinet(x, y, w, h).to_record()
}

/// [`ControlArea::parse`] as a `(startX, startY, endX, endY)` tuple.
#[must_use]
pub fn parse_control_area(r: &[u8]) -> Option<(u16, u16, u16, u16)> {
    ControlArea::parse(r).map(|a| (a.start_x, a.start_y, a.end_x, a.end_y))
}

/// A type-0x1900 record frame to receiver `index`: `[4..8]` address,
/// `[8..12]` length, data at 12.
fn record_frame(index: u16, opcode: u8, addr: u32, data: &[u8]) -> Vec<u8> {
    // Payload length max(0x80, len + 0x12), as the vendor builds it
    // (`write_frame_matches_the_vendor_layout`).
    let n = (data.len() + 0x12).max(0x80);
    frame_with([0x19, 0x00], n, |p| {
        indexed(p, index, opcode);
        p[4..8].copy_from_slice(&addr.to_be_bytes());
        p[8..12].copy_from_slice(&(data.len() as u32).to_be_bytes());
        p[12..12 + data.len()].copy_from_slice(data);
    })
}

/// Write one record to the card at chain position `index` (`BROADCAST` for
/// every card). `data.len()` must be the record's own length.
#[must_use]
pub fn write_to(index: u16, addr: u16, data: &[u8]) -> Vec<u8> {
    record_frame(index, 0x85, u32::from(addr), data)
}

/// [`write_to`] every card on the link.
#[must_use]
pub fn write(addr: u16, data: &[u8]) -> Vec<u8> {
    write_to(BROADCAST, addr, data)
}

/// Commit the EEPROM of card `index` to its flash mirror (`SaveEepromFlash`).
#[must_use]
pub fn save_to(index: u16) -> Vec<u8> {
    record_frame(index, 0x87, 0, &[])
}

/// [`save_to`] every card on the link.
#[must_use]
pub fn save() -> Vec<u8> {
    save_to(BROADCAST)
}

/// `ReLoadLocalParam` to card `index`, as the vendor sends it after an EEPROM
/// save: opcode 0x77, flags `01 01 00`. The flash-save reloads are
/// `discovery::reload_params*`.
#[must_use]
pub fn reload_to(index: u16) -> Vec<u8> {
    command(index, 0x77, &[0x01, 0x01, 0x00])
}

/// [`reload_to`] every card on the link.
#[must_use]
pub fn reload() -> Vec<u8> {
    reload_to(BROADCAST)
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
    fn control_area_round_trips_through_its_corners() {
        let area = ControlArea::for_cabinet(128, 64, 128, 64);
        assert_eq!(area, ControlArea { start_x: 128, start_y: 64, end_x: 256, end_y: 128 });
        assert_eq!(ControlArea::parse(&area.to_record()), Some(area));
        assert_eq!(ControlArea::parse(&[0; 7]), None);
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
    fn indexed_frames_carry_the_chain_position_big_endian() {
        // `BulidEepromFlashOperation`: payload[3..4] = rcvIdx BE (docs/eeprom-map.md).
        let f = write_to(0x0102, 0x02, &[0u8; 42]);
        assert_eq!(&f[15..17], &[0x01, 0x02]);
        assert_eq!(f[17], 0x85);
        let f = save_to(3);
        assert_eq!(&f[15..18], &[0x00, 0x03, 0x87]);
        assert_eq!(&f[18..26], &[0; 8], "save carries no address or length");
        let f = reload_to(3);
        assert_eq!(&f[12..14], &[0x06, 0x00]);
        assert_eq!(&f[15..18], &[0x00, 0x03, 0x77]);
        assert_eq!(&f[22..25], &[0x01, 0x01, 0x00]);
    }

    #[test]
    fn broadcast_forms_are_the_indexed_forms_at_0xffff() {
        assert_eq!(write(0x02, &[7u8; 42]), write_to(BROADCAST, 0x02, &[7u8; 42]));
        assert_eq!(save(), save_to(BROADCAST));
        assert_eq!(reload(), reload_to(BROADCAST));
        assert_ne!(write(0x02, &[0u8; 42]), write_to(0, 0x02, &[0u8; 42]));
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
        // Ordering only: the vendor map overlaps once (0x0c1 len 12 spans 0x0c8).
        for w in RECORDS.windows(2) {
            assert!(w[1].addr > w[0].addr, "{} out of order", w[1].name);
        }
    }
}
