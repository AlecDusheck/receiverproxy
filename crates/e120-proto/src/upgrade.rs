//! SDRAM-staged firmware upgrade: descriptor query, 1 KiB chunk uploads
//! (unacknowledged; the caller paces them), erase, program, completion poll.
//!
//! The card programs only 0x000000-0x02FFFF and 0x080000-0x0AFFFF from the
//! staged image; the 320 KB between reads back unchanged after an upgrade
//! (`docs/archive/firmware-16.53-bench-result.md`).

use super::{frame_with, indexed};

const SDRAM_TYPE: [u8; 2] = [0x1a, 0x00];

const OP_DATA: u8 = 0x01;
const OP_PROGRAM: u8 = 0x03;
const OP_ERASE: u8 = 0x05;

/// Bytes per upload chunk.
pub const CHUNK: usize = 1024;

/// Which stored image an erase or program operation targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Partition {
    /// The image the card normally runs.
    Primary,
    /// The golden backup; only on cards whose descriptor reports one.
    Golden,
}

impl Partition {
    const fn selector(self) -> u8 {
        match self {
            Self::Primary => 0x04,
            Self::Golden => 0x0d,
        }
    }
}

/// Any SDRAM operation frame: [1..3] receiver selector BE, [3] opcode,
/// [4] partition/flag, [5..8] 24-bit BE offset, [8..12] u32 BE length, data.
fn sdram_frame(sel: u16, op: u8, flag: u8, offset: u32, len: u32, data: &[u8]) -> Vec<u8> {
    // The vendor always allocates room for a full chunk, even when sending none.
    let body = data.len().max(CHUNK);
    frame_with(SDRAM_TYPE, 12 + body, |p| {
        indexed(p, sel, op);
        p[4] = flag;
        p[5..8].copy_from_slice(&offset.to_be_bytes()[1..]);
        p[8..12].copy_from_slice(&len.to_be_bytes());
        p[12..12 + data.len()].copy_from_slice(data);
    })
}

/// Upload one chunk into SDRAM; `offset` is its byte position in the image.
#[must_use]
pub fn sdram_chunk(sel: u16, offset: u32, data: &[u8]) -> Vec<u8> {
    sdram_frame(sel, OP_DATA, 0x00, offset, data.len() as u32, data)
}

/// Erase `len` bytes of the partition.
#[must_use]
pub fn sdram_erase(sel: u16, partition: Partition, len: u32) -> Vec<u8> {
    sdram_frame(sel, OP_ERASE, partition.selector(), 0, len, &[])
}

/// Program flash from the image staged in SDRAM.
#[must_use]
pub fn sdram_program(sel: u16, partition: Partition, len: u32) -> Vec<u8> {
    sdram_frame(sel, OP_PROGRAM, partition.selector(), 0, len, &[])
}

/// The card's reply to `upgrade_info`: image geometry and upgrade capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    /// Flash address the image starts at.
    pub start: u32,
    /// Bytes programmed into flash.
    pub image_len: u32,
    /// Required source-file size, including trailing padding.
    pub file_len: u32,
    /// Type byte for direct flash operations.
    pub flash_op_type: u8,
    capabilities: u8,
}

impl Descriptor {
    /// The card can stage an image in SDRAM and program itself.
    #[must_use]
    pub const fn supports_sdram(self) -> bool {
        self.capabilities & 0b0001 != 0
    }

    /// The card keeps a golden backup image.
    #[must_use]
    pub const fn has_golden(self) -> bool {
        self.capabilities & 0b0010 != 0
    }

    /// The card accepts a partition selection.
    #[must_use]
    pub const fn supports_select_part(self) -> bool {
        self.capabilities & 0b0100 != 0
    }

    /// The card accepts upgrades aimed at the golden bank.
    #[must_use]
    pub const fn supports_golden_upgrade(self) -> bool {
        self.capabilities & 0b1000 != 0
    }

    /// Chunks needed to stage an image of this size.
    #[must_use]
    pub const fn chunks(self) -> usize {
        (self.image_len as usize).div_ceil(CHUNK)
    }

    /// Vendor delay before the first completion poll: 150 ms per 64 KiB
    /// block, at least 1000 ms (`timings_match_the_vendor_formulas`).
    #[must_use]
    pub const fn first_poll_ms(self) -> u64 {
        let blocks = (self.image_len as u64).div_ceil(0x10000);
        let by_size = 150 * blocks;
        if by_size > 1000 {
            by_size
        } else {
            1000
        }
    }

    /// Vendor estimate of programming time: 500 ms per 64 KiB block plus
    /// 3 ms per 256-byte page.
    #[must_use]
    pub const fn estimated_ms(self) -> u64 {
        let blocks = (self.image_len as u64).div_ceil(0x10000);
        let pages = (self.image_len as u64).div_ceil(0x100);
        500 * blocks + 3 * pages
    }
}

/// Decode a `Descriptor` from the 0x08xx reply; offsets below are relative to
/// frame offset 12.
#[must_use]
pub fn parse_descriptor(eth_frame: &[u8]) -> Option<Descriptor> {
    let p = eth_frame.get(12..)?;
    // Bit 1 of the type's second byte marks a valid descriptor.
    if *p.first()? != 0x08 || p.get(1)? & 0b10 == 0 {
        return None;
    }
    let byte = |i: usize| p.get(i).copied();
    let image_len = u32::from(byte(0x18)?) << 16 | u32::from(byte(0x19)?) << 8;
    // File length low byte = [0x1b] + 4 * [0x1a], as the vendor computes it.
    let low = u32::from(byte(0x1b)?.wrapping_add(byte(0x1a)?.wrapping_mul(4)));
    Some(Descriptor {
        start: u32::from(byte(0x16)?) << 16 | u32::from(byte(0x17)?) << 8,
        image_len,
        file_len: image_len | (low & 0xff),
        flash_op_type: byte(0x12)?,
        capabilities: byte(0x13)?,
    })
}

/// Completion is polled with `upgrade_info`; bit 1 at payload offset 0xc0 of
/// the reply is set once programming has finished.
#[must_use]
pub fn programming_finished(eth_frame: &[u8]) -> bool {
    eth_frame
        .get(12 + 0xc0)
        .is_some_and(|status| status & 0b10 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BROADCAST;

    /// The `upgrade_info` reply captured from the bench card (fw 16.53).
    fn real_reply() -> Vec<u8> {
        let mut f = vec![0u8; 12];
        f.extend_from_slice(&[0x08, 0x02]);
        f.extend_from_slice(&[
            0x00, 0x1a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0x00, 0x36, 0x05, 0x00, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x20, 0x00,
        ]);
        f.resize(1070, 0);
        f
    }

    #[test]
    fn the_real_reply_decodes_as_the_card_reported() {
        let d = parse_descriptor(&real_reply()).expect("should decode");
        assert_eq!(d.start, 0x0000_0000, "image starts at zero");
        assert_eq!(d.image_len, 0x000b_0000, "720896 bytes programmed");
        assert_eq!(d.file_len, 0x000b_0080, "721024 bytes of source file");
        assert_eq!(d.flash_op_type, 0x36);
        assert!(d.supports_sdram(), "this card stages via SDRAM");
        assert!(!d.has_golden(), "and reports no golden bank");
        assert!(d.supports_select_part());
        assert!(!d.supports_golden_upgrade());
    }

    #[test]
    fn the_image_needs_704_chunks() {
        let d = parse_descriptor(&real_reply()).unwrap();
        assert_eq!(d.chunks(), 704);
    }

    #[test]
    fn timings_match_the_vendor_formulas() {
        let d = parse_descriptor(&real_reply()).unwrap();
        // 11 blocks, 2816 pages
        assert_eq!(d.first_poll_ms(), 1650);
        assert_eq!(d.estimated_ms(), 500 * 11 + 3 * 2816);
    }

    #[test]
    fn a_reply_of_the_wrong_type_is_rejected() {
        let mut f = real_reply();
        f[12] = 0x09;
        assert!(parse_descriptor(&f).is_none());
    }

    #[test]
    fn a_chunk_frame_matches_the_documented_layout() {
        let data: Vec<u8> = (0..CHUNK).map(|i| i as u8).collect();
        let f = sdram_chunk(BROADCAST, 0x000c_0400, &data);
        assert_eq!(f.len(), 1050, "12 MAC + 2 type + 12 header + 1024 data");
        assert_eq!(&f[12..14], &SDRAM_TYPE);
        assert_eq!(&f[15..17], &BROADCAST.to_be_bytes());
        assert_eq!(f[17], OP_DATA);
        assert_eq!(f[18], 0x00);
        assert_eq!(&f[19..22], &[0x0c, 0x04, 0x00], "24-bit big-endian offset");
        assert_eq!(&f[22..26], &1024u32.to_be_bytes());
        assert_eq!(&f[26..], &data[..]);
    }

    #[test]
    fn erase_and_program_carry_the_partition_and_length() {
        let len = 0x000b_0000;
        let e = sdram_erase(BROADCAST, Partition::Primary, len);
        assert_eq!(e[17], OP_ERASE);
        assert_eq!(e[18], 0x04, "primary partition selector");
        assert_eq!(&e[19..22], &[0, 0, 0], "no address on erase");
        assert_eq!(&e[22..26], &len.to_be_bytes());

        let p = sdram_program(BROADCAST, Partition::Primary, len);
        assert_eq!(p[17], OP_PROGRAM);
        // Program differs from erase only in the opcode.
        assert_eq!(&p[18..], &e[18..]);
    }

    #[test]
    fn the_golden_partition_uses_its_own_selector() {
        let g = sdram_erase(BROADCAST, Partition::Golden, 0x1000);
        assert_eq!(g[18], 0x0d);
    }

    #[test]
    fn completion_is_read_from_the_status_bit() {
        let mut f = vec![0u8; 12 + 0xc1];
        assert!(!programming_finished(&f));
        f[12 + 0xc0] = 0b10;
        assert!(programming_finished(&f));
    }
}
