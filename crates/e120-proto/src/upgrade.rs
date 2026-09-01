//! Firmware upgrade over Ethernet.
//!
//! The card stages a whole firmware image into its own SDRAM, then programs its
//! flash from there under its own control. The host never writes the firmware
//! region directly — attempts to do so are silently ignored, because the
//! program area is write-protected and only the card's own agent unlocks it.
//!
//! The sequence is: ask the card what it expects, upload the image in 1 KiB
//! chunks, tell it to erase, tell it to program, then poll until it reports
//! done. None of the upload frames are acknowledged; the pacing delays are the
//! protocol.
//!
//! The whole image is uploaded, but the card programs only 0x000000-0x02FFFF
//! and 0x080000-0x0AFFFF from it. The 320KB in between is reserved for the
//! card's configuration and is not part of the loadable bitstream, so reading
//! it back and finding it unchanged means the upgrade worked, not that it
//! failed. See `third-party/README.md`.

use super::frame;

/// Frame type for every SDRAM staging operation.
const SDRAM_TYPE: [u8; 2] = [0x1a, 0x00];

/// Upload one chunk into SDRAM.
const OP_DATA: u8 = 0x01;
/// Program flash from the staged image.
const OP_PROGRAM: u8 = 0x03;
/// Erase the target region.
const OP_ERASE: u8 = 0x05;

/// Bytes per staging chunk.
pub const CHUNK: usize = 1024;

/// Address every receiver on the link. What the vendor uses for a single card.
pub const BROADCAST: u16 = 0xffff;

/// Which stored image an erase or program operation targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Partition {
    /// The image the card normally runs.
    Primary,
    /// The golden backup. Only reachable on cards that report one.
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

/// Build any SDRAM operation frame.
///
/// Layout, relative to the two type bytes: a zero, the receiver selector big
/// endian, the opcode, a partition or flag byte, a 24-bit big-endian byte
/// offset, a 32-bit big-endian length, then the data.
fn sdram_frame(sel: u16, op: u8, flag: u8, offset: u32, len: u32, data: &[u8]) -> Vec<u8> {
    // The vendor always allocates room for a full chunk, even when sending none.
    let body = data.len().max(CHUNK);
    let mut p = vec![0u8; 12 + body];
    p[1..3].copy_from_slice(&sel.to_be_bytes());
    p[3] = op;
    p[4] = flag;
    p[5] = (offset >> 16) as u8;
    p[6] = (offset >> 8) as u8;
    p[7] = offset as u8;
    p[8..12].copy_from_slice(&len.to_be_bytes());
    p[12..12 + data.len()].copy_from_slice(data);
    frame(SDRAM_TYPE, &p)
}

/// Upload one 1 KiB chunk of the image into the card's SDRAM.
///
/// `offset` is the chunk's byte position within the image.
#[must_use]
pub fn sdram_chunk(sel: u16, offset: u32, data: &[u8]) -> Vec<u8> {
    sdram_frame(sel, OP_DATA, 0x00, offset, data.len() as u32, data)
}

/// Erase the target region, in preparation for programming.
#[must_use]
pub fn sdram_erase(sel: u16, partition: Partition, len: u32) -> Vec<u8> {
    sdram_frame(sel, OP_ERASE, partition.selector(), 0, len, &[])
}

/// Program flash from the image staged in SDRAM.
#[must_use]
pub fn sdram_program(sel: u16, partition: Partition, len: u32) -> Vec<u8> {
    sdram_frame(sel, OP_PROGRAM, partition.selector(), 0, len, &[])
}

/// Ask the card to reconfigure from flash.
///
/// The only reconfiguration command in the vendor library. It carries no bank,
/// address or partition, so it reloads whatever the card boots by default.
#[must_use]
pub fn reload_firmware(sel: u16) -> Vec<u8> {
    let mut p = vec![0u8; 0x106];
    p[1..3].copy_from_slice(&sel.to_be_bytes());
    p[3] = 0x34;
    frame([0x26, 0x00], &p)
}

/// What the card says about the image it expects and how it can be upgraded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    /// Where the image begins in the card's own address space.
    pub start: u32,
    /// Bytes actually programmed into flash.
    pub image_len: u32,
    /// Size the source file must be, which includes trailing padding.
    pub file_len: u32,
    /// Type byte the card wants for direct flash operations.
    pub flash_op_type: u8,
    capabilities: u8,
}

impl Descriptor {
    /// The card can stage an image in SDRAM and program itself. When true this
    /// is the path the vendor takes, and the direct-write path is unused.
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

    /// How long to wait before the first completion poll, in milliseconds.
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

    /// The vendor's own estimate of how long programming takes, in
    /// milliseconds. Treat it as a guide, not a guarantee.
    #[must_use]
    pub const fn estimated_ms(self) -> u64 {
        let blocks = (self.image_len as u64).div_ceil(0x10000);
        let pages = (self.image_len as u64).div_ceil(0x100);
        500 * blocks + 3 * pages
    }
}

/// Decode the descriptor from a reply frame.
///
/// Offsets are relative to the start of the Ethernet payload, which begins at
/// the type bytes — so the first field sits two bytes further in than a naive
/// reading of the frame body would suggest.
#[must_use]
pub fn parse_descriptor(eth_frame: &[u8]) -> Option<Descriptor> {
    let p = eth_frame.get(12..)?;
    // The reply type's low byte doubles as a validity marker.
    if *p.first()? != 0x08 || p.get(1)? & 0b10 == 0 {
        return None;
    }
    let byte = |i: usize| p.get(i).copied();
    let image_len = u32::from(byte(0x18)?) << 16 | u32::from(byte(0x19)?) << 8;
    // The file's low length byte is encoded across two fields.
    let low = u32::from(byte(0x1b)?.wrapping_add(byte(0x1a)?.wrapping_mul(4)));
    Some(Descriptor {
        start: u32::from(byte(0x16)?) << 16 | u32::from(byte(0x17)?) << 8,
        image_len,
        file_len: image_len | (low & 0xff),
        flash_op_type: byte(0x12)?,
        capabilities: byte(0x13)?,
    })
}

/// Whether a completion poll reply says programming has finished.
///
/// The card reports done by setting a bit in each receiver's record; there is
/// no dedicated completion frame.
#[must_use]
pub fn programming_finished(eth_frame: &[u8]) -> bool {
    eth_frame
        .get(12 + 0xc0)
        .is_some_and(|status| status & 0b10 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reply this card actually sent, captured from the wire.
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
        // Program is byte-identical to erase but for the opcode.
        assert_eq!(&p[18..], &e[18..]);
    }

    #[test]
    fn the_golden_partition_uses_its_own_selector() {
        let g = sdram_erase(BROADCAST, Partition::Golden, 0x1000);
        assert_eq!(g[18], 0x0d);
    }

    #[test]
    fn reload_is_a_bare_command() {
        let f = reload_firmware(BROADCAST);
        assert_eq!(&f[12..14], &[0x26, 0x00]);
        assert_eq!(f[17], 0x34);
        assert!(f[18..].iter().all(|&b| b == 0), "carries no payload");
    }

    #[test]
    fn completion_is_read_from_the_status_bit() {
        let mut f = vec![0u8; 12 + 0xc1];
        assert!(!programming_finished(&f));
        f[12 + 0xc0] = 0b10;
        assert!(programming_finished(&f));
    }
}
