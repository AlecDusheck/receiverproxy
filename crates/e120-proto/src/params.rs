//! Real-time parameter packs (type 0x05).
//!
//! The vendor tool pushes these into the card's RAM at the start of every
//! session rather than relying on the copy in flash. They are what actually
//! configures the scan engine and initialises the panel's driver chips, so a
//! card that has never received them may not drive its outputs at all.
//!
//! Two packs are sent, distinguished by bytes inside the payload rather than by
//! the type: the chip pack carries `payload[3] = 1`, and the basic pack carries
//! the marker `payload[4] = 0xA8`.
//!
//! Field placement is reverse-engineered and incomplete. Offsets we cannot yet
//! attribute are left zero, and [`Unknowns`] lists them so they can be swept
//! against real hardware.

/// Bytes in a parameter pack, including the two leading type bytes.
pub const PACK_LEN: usize = 0x104;

/// Marks the basic-parameter pack.
const BASIC_MARKER: u8 = 0xa8;

/// Wrap a fully built pack into an Ethernet frame.
///
/// The pack's first two bytes are the type, matching every other frame in this
/// protocol.
#[must_use]
pub fn frame_for(pack: &[u8; PACK_LEN]) -> Vec<u8> {
    super::frame([pack[0], pack[1]], &pack[2..])
}

/// Read a big-endian u16 from a record payload, or zero past the end.
fn be16(src: &[u8], off: usize) -> u16 {
    match (src.get(off), src.get(off + 1)) {
        (Some(a), Some(b)) => u16::from_be_bytes([*a, *b]),
        _ => 0,
    }
}

fn byte(src: &[u8], off: usize) -> u8 {
    src.get(off).copied().unwrap_or(0)
}

/// Identifiers at or above this need an escape byte in the single-byte slot.
const CHIP_ID_ESCAPE: u8 = 0xfe;

/// Build the basic-parameter pack from a record 0x01 payload.
///
/// Placements come from joining the pack's store offsets to the record offsets
/// that feed them. Fields whose source is still unattributed stay zero.
#[must_use]
pub fn scan_pack(record_01: &[u8]) -> [u8; PACK_LEN] {
    let mut p = [0u8; PACK_LEN];
    p[0] = 0x05;
    // Pack sub-index: identifies this as the basic-parameter pack.
    p[3] = 0x02;
    p[4] = BASIC_MARKER;

    // Sources follow the joined table in docs/config-protocol.md §21.2, and
    // nothing else: fields that table does not resolve stay zero.
    p[0x05] = byte(record_01, 0x028);
    p[0x06] = byte(record_01, 0x029);
    p[0x07] = byte(record_01, 0x02a);
    // Module geometry. From an unresolved record, so stated directly; swap if
    // the image comes out transposed.
    p[0x08] = 0x80;
    p[0x09] = 0x40;
    // One module across the cabinet.
    p[0x0a] = 0x01;
    // Grey level.
    p[0x0c] = 0x10;
    // Scan denominator, big-endian.
    let scan = u16::from(byte(record_01, 0x020));
    p[0x0d..0x0f].copy_from_slice(&scan.to_be_bytes());
    // Clocks per scan line; the module folds 128x64 into a 256-wide chain,
    // per the 256X384 in the vendor's own file name.
    p[0x0f..0x11].copy_from_slice(&0x0100u16.to_be_bytes());
    p[0x15] = 0x99;
    p[0x16] = (byte(record_01, 0x018) >> 1) & 1;
    p[0x17] = byte(record_01, 0x018) & 1;
    p[0x26] = byte(record_01, 0x03d) & 0x0f;
    p[0x27] = byte(record_01, 0x03e);
    p[0x28] = byte(record_01, 0x03e);
    p[0x2c..0x2e].copy_from_slice(&be16(record_01, 0x045).to_be_bytes());
    p[0x3a] = byte(record_01, 0x04f);
    p[0x3b] = byte(record_01, 0x050);
    p[0x46] = byte(record_01, 0x24e);
    p[0x47] = byte(record_01, 0x24f);
    p[0x49] = byte(record_01, 0x030);
    p[0x4a] = byte(record_01, 0x031);
    // Hub type.
    p[0x4b] = byte(record_01, 0x058);
    p[0x90] = 0x01;
    // Current percent, from the 0.1 floats at R1+0xB4: ~10% of full scale.
    p[0xd8] = 0x1a;

    // The driver-chip identifier, at the offsets the vendor uses. Without it
    // the card is told chip type 0 — a plain shift register — and stops
    // emitting the smart chip's init sequence, which observably disarms the
    // drivers until a power cycle.
    let chip = u16::from(byte(record_01, 0x204)) << 8 | u16::from(byte(record_01, 0x036));
    p[0x01f] = if chip >= 0x100 {
        CHIP_ID_ESCAPE
    } else {
        chip as u8
    };
    p[0x0eb] = (chip >> 8) as u8;
    p[0x0ec] = (chip & 0xff) as u8;

    p
}

/// The pack that arms the driver chips, exactly as first reverse-engineered.
///
/// Its sub-index is zero — probably the vendor's data-swap slot rather than
/// the basic pack — and several placements disagree with the §21.2 table, yet
/// this is the only pack that observably arms the SM16269S drivers (PSU jumps
/// from ~0.32 A to ~0.79 A). It is preserved byte-for-byte until each of its
/// fields is understood, because being right by the book and dark is worse
/// than being wrong by the book and armed.
#[must_use]
pub fn basic_pack(record_01: &[u8]) -> [u8; PACK_LEN] {
    let mut p = [0u8; PACK_LEN];
    p[0] = 0x05;
    p[1] = 0x00;
    p[4] = BASIC_MARKER;
    p[0x0a] = 0x01;
    p[0x1e] = 0x80;
    p[0x0c] = 0x10;
    let scan = u16::from(byte(record_01, 0x020));
    p[0x0d..0x0f].copy_from_slice(&scan.to_be_bytes());
    p[0x017] = byte(record_01, 0x018);
    p[0x019] = byte(record_01, 0x024);
    p[0x026] = byte(record_01, 0x03d);
    p[0x027] = byte(record_01, 0x03e);
    p[0x028] = byte(record_01, 0x03e);
    p[0x02c..0x02e].copy_from_slice(&be16(record_01, 0x045).to_be_bytes());
    p[0x03a] = byte(record_01, 0x04f);
    p[0x046] = byte(record_01, 0x24e);
    p[0x047] = byte(record_01, 0x24f);
    let chip = u16::from(byte(record_01, 0x204)) << 8 | u16::from(byte(record_01, 0x036));
    p[0x01f] = if chip >= 0x100 {
        CHIP_ID_ESCAPE
    } else {
        chip as u8
    };
    p[0x0eb] = (chip >> 8) as u8;
    p[0x0ec] = (chip & 0xff) as u8;
    p
}

/// Build the chip-register pack from a record 0x84 payload.
///
/// The record holds `(register, R, G, B)` quads describing the panel's driver
/// chips. Without these a PWM driver emits nothing regardless of scan config.
#[must_use]
pub fn chip_pack(record_84: &[u8]) -> [u8; PACK_LEN] {
    let mut p = [0u8; PACK_LEN];
    p[0] = 0x05;
    p[1] = 0x00;
    p[3] = 0x01;

    // The record's 256 bytes are copied in whole, starting at offset 4: the
    // loader stores them in the chip sub-object and the pack builder copies
    // them straight back out, a constant +4 shift across every block.
    let n = record_84.len().min(PACK_LEN - 4);
    p[4..4 + n].copy_from_slice(&record_84[..n]);
    p
}

/// Build a pack by copying a record's payload in whole at offset 4.
///
/// The chip pack works exactly this way: the loader stores record 0x84's bytes
/// and the pack builder copies them straight back out, a constant shift at both
/// ends. Other records plausibly follow the same shape, so this lets us send
/// packs for records whose fields we have not decoded.
#[must_use]
pub fn verbatim_pack(sub_index: u8, record: &[u8]) -> [u8; PACK_LEN] {
    let mut p = [0u8; PACK_LEN];
    p[0] = 0x05;
    p[1] = 0x00;
    p[3] = sub_index;
    let n = record.len().min(PACK_LEN - 4);
    p[4..4 + n].copy_from_slice(&record[..n]);
    p
}

/// Split a long record into pack-sized chunks, each carrying its index.
///
/// Records such as the pixel mapping table are far larger than one pack, so
/// they must arrive in pieces. The chunk index goes where a single-pack record
/// carries its sub-index.
#[must_use]
pub fn chunked_packs(sub_index: u8, record: &[u8]) -> Vec<[u8; PACK_LEN]> {
    const BODY: usize = PACK_LEN - 8;
    record
        .chunks(BODY)
        .enumerate()
        .map(|(i, chunk)| {
            let mut p = [0u8; PACK_LEN];
            p[0] = 0x05;
            p[1] = 0x00;
            p[3] = sub_index;
            p[4..6].copy_from_slice(&(i as u16).to_be_bytes());
            p[6..8].copy_from_slice(&(chunk.len() as u16).to_be_bytes());
            p[8..8 + chunk.len()].copy_from_slice(chunk);
            p
        })
        .collect()
}

/// Pack offsets whose source is not yet attributed, for sweeping on hardware.
#[derive(Clone, Copy, Debug)]
pub struct Unknowns;

impl Unknowns {
    /// Offsets in the basic pack that are still written as zero but are known
    /// to be written by the vendor tool.
    pub const BASIC: &'static [usize] = &[
        0x00a, 0x00b, 0x00f, 0x011, 0x013, 0x014, 0x015, 0x016, 0x018, 0x022, 0x023, 0x029, 0x02e,
        0x030, 0x031, 0x032, 0x033, 0x038, 0x03b, 0x03c, 0x03d, 0x03f, 0x040, 0x041, 0x048, 0x049,
        0x04a, 0x04b, 0x04c, 0x04d, 0x04e, 0x04f, 0x050, 0x051,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_are_the_right_size_and_type() {
        let b = basic_pack(&[0u8; 764]);
        assert_eq!(b.len(), PACK_LEN);
        assert_eq!(b[0], 0x05);
        let f = frame_for(&b);
        assert_eq!(f.len(), 272, "260-byte pack becomes a 272-byte frame");
        assert_eq!(&f[12..14], &[0x05, 0x00]);
    }

    #[test]
    fn the_basic_pack_carries_its_marker() {
        let b = basic_pack(&[0u8; 764]);
        assert_eq!(b[4], BASIC_MARKER);
    }

    #[test]
    fn the_chip_pack_is_distinguished_by_its_sub_index() {
        let c = chip_pack(&[0u8; 256]);
        assert_eq!(c[3], 1);
        assert_ne!(c[4], BASIC_MARKER);
    }

    #[test]
    fn scan_mode_is_carried_big_endian() {
        let mut rec = vec![0u8; 764];
        rec[0x20] = 16;
        let b = basic_pack(&rec);
        assert_eq!(&b[0x0d..0x0f], &16u16.to_be_bytes());
    }

    #[test]
    fn chip_registers_are_copied_verbatim_from_offset_four() {
        let rec: Vec<u8> = (0..=255u8).collect();
        let c = chip_pack(&rec);
        assert_eq!(&c[4..], &rec[..]);
    }

    #[test]
    fn short_records_do_not_panic() {
        let _ = basic_pack(&[]);
        let _ = chip_pack(&[]);
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn verbatim_packs_copy_the_whole_record() {
        let rec: Vec<u8> = (0..=255u8).collect();
        let p = verbatim_pack(7, &rec);
        assert_eq!(p[3], 7);
        assert_eq!(&p[4..], &rec[..]);
    }

    #[test]
    fn chunking_covers_a_long_record_exactly_once() {
        let rec: Vec<u8> = (0..5000u32).map(|i| i as u8).collect();
        let packs = chunked_packs(3, &rec);
        let mut rebuilt = Vec::new();
        for p in &packs {
            let len = u16::from_be_bytes([p[6], p[7]]) as usize;
            rebuilt.extend_from_slice(&p[8..8 + len]);
        }
        assert_eq!(rebuilt, rec);
    }

    #[test]
    fn chunk_indices_are_sequential() {
        let rec = vec![0u8; 1000];
        for (i, p) in chunked_packs(3, &rec).iter().enumerate() {
            assert_eq!(u16::from_be_bytes([p[4], p[5]]) as usize, i);
        }
    }
}

#[cfg(test)]
mod scan_pack_tests {
    use super::*;

    fn record() -> Vec<u8> {
        let mut r = vec![0u8; 764];
        r[0x018] = 0b11;
        r[0x020] = 16;
        r[0x03d] = 0xf2;
        r[0x058] = 0x10;
        r
    }

    #[test]
    fn the_pack_identifies_itself_as_the_basic_pack() {
        let p = scan_pack(&record());
        assert_eq!(p[0], 0x05);
        assert_eq!(p[3], 0x02, "pack sub-index");
        assert_eq!(p[4], BASIC_MARKER);
    }

    #[test]
    fn geometry_and_scan_follow_the_joined_table() {
        let p = scan_pack(&record());
        assert_eq!((p[0x08], p[0x09]), (0x80, 0x40), "module 128x64");
        assert_eq!(u16::from_be_bytes([p[0x0d], p[0x0e]]), 16, "scan");
        assert_eq!(u16::from_be_bytes([p[0x0f], p[0x10]]), 256, "scan-line clocks");
        assert_eq!(p[0x4b], 0x10, "hub type from R1+0x058");
    }

    /// The arming pack is preserved byte-for-byte: this recipe is the one
    /// that observably arms the SM16269S drivers, so any change to it must be
    /// deliberate.
    #[test]
    fn the_arming_pack_keeps_its_empirical_layout() {
        let mut r = vec![0u8; 764];
        r[0x036] = 0x4c;
        r[0x204] = 0x01;
        r[0x020] = 16;
        let p = basic_pack(&r);
        assert_eq!(p[3], 0, "sub-index stays zero");
        assert_eq!(p[4], BASIC_MARKER);
        assert_eq!(p[0x1e], 0x80);
        assert_eq!(u16::from_be_bytes([p[0x0d], p[0x0e]]), 16);
        assert_eq!(p[0x01f], CHIP_ID_ESCAPE);
        assert_eq!((p[0x0eb], p[0x0ec]), (0x01, 0x4c));
    }

    #[test]
    fn flag_bits_are_split_and_masked() {
        let p = scan_pack(&record());
        assert_eq!(p[0x16], 1, "R1+0x018 bit 1");
        assert_eq!(p[0x17], 1, "R1+0x018 bit 0");
        assert_eq!(p[0x26], 0x02, "R1+0x03D low nibble only");
    }
}
