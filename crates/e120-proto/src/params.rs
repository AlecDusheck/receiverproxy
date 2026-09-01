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

/// Build the basic-parameter pack from a record 0x01 payload.
///
/// Placements come from joining the pack's store offsets to the record offsets
/// that feed them. Fields whose source is still unattributed stay zero.
#[must_use]
pub fn basic_pack(record_01: &[u8]) -> [u8; PACK_LEN] {
    let mut p = [0u8; PACK_LEN];
    p[0] = 0x05;
    p[1] = 0x00;
    p[4] = BASIC_MARKER;
    p[0x0a] = 0x01;
    p[0x1e] = 0x80;

    // Grey level: the vendor writes a constant here.
    p[0x0c] = 0x10;

    // Scan mode, held in the record as the literal denominator.
    let scan = u16::from(byte(record_01, 0x020));
    p[0x0d..0x0f].copy_from_slice(&scan.to_be_bytes());

    // Fields whose record source is established.
    p[0x017] = byte(record_01, 0x018);
    p[0x019] = byte(record_01, 0x024);
    p[0x026] = byte(record_01, 0x03d);
    p[0x027] = byte(record_01, 0x03e);
    p[0x028] = byte(record_01, 0x03e);
    p[0x02c..0x02e].copy_from_slice(&be16(record_01, 0x045).to_be_bytes());
    p[0x03a] = byte(record_01, 0x04f);
    p[0x046] = byte(record_01, 0x24e);
    p[0x047] = byte(record_01, 0x24f);

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
