//! Flash access, with allowlists that keep writes where they belong.

use super::frame;

/// Read opcode. A read frame carries no data, so it cannot modify the card
/// wherever it is pointed.
const FLASH_OP_READ: u8 = 0x44;

/// Flash region holding the receiver's basic parameters, in 256-byte pages.
pub const FLASH_PAGE_BASIC_PARAM: u16 = 0x0780;

/// Pages advance by 4 per 1024-byte chunk read.
pub const FLASH_PAGES_PER_CHUNK: u16 = 4;

/// Opcode that erases a flash block.
const FLASH_OP_ERASE: u8 = 0x23;

/// Opcode that writes one 256-byte flash page (also the EEPROM record write).
const FLASH_OP_WRITE: u8 = 0x85;

/// Bytes in one flash page.
pub const FLASH_PAGE_BYTES: usize = 256;

/// The 64KB block holding the receiver parameters: the only block the
/// parameter helpers (`erase_block`, `write_page`) accept.
pub const PARAM_BLOCK: u8 = 0x07;

/// Blocks holding the primary firmware image. The golden backup at
/// [`GOLDEN_BLOCK`] is never writable, so it stays as an in-hardware fallback.
pub const FIRMWARE_BLOCKS: std::ops::Range<u8> = 0x00..0x0b;

/// First block of the golden backup image.
pub const GOLDEN_BLOCK: u8 = 0x20;

/// Flash address of the screen-size record. It sits outside the window the
/// page-addressed frames reach, so it is only writable through the linear
/// frames below.
pub const SCREEN_RECORD_ADDR: u32 = 0x0007_f000;

/// Bytes in the screen-size record.
pub const SCREEN_RECORD_LEN: usize = 256;

/// Linear-address writes can reach firmware, so exactly one range is allowed.
const LINEAR_ALLOWED: std::ops::Range<u32> =
    SCREEN_RECORD_ADDR..SCREEN_RECORD_ADDR + SCREEN_RECORD_LEN as u32;

/// Frame type the card answers a flash read with.
pub const FLASH_REPLY_TYPE: [u8; 2] = [0x09, 0x01];

/// Payload bytes of flash data carried by each reply frame.
pub const FLASH_CHUNK_BYTES: usize = 1024;

/// Card-flash read: type 0x0600, 126-byte payload. Requests 1024 bytes
/// starting at `page` (a 256-byte page index).
pub fn read_flash(rcv_index: u16, page: u16) -> Vec<u8> {
    let mut p = [0u8; 126];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_READ;
    p[4] = 0x01;
    p[5..7].copy_from_slice(&page.to_be_bytes());
    frame([0x06, 0x00], &p)
}

/// Unlock or relock the write-protected program region.
///
/// Erases and page writes silently do nothing while locked. The vendor builder
/// negates the flag, so enable is 0xff. Always relock when finished, including
/// on failure.
#[must_use]
pub fn set_program_writable(rcv_index: u16, writable: bool) -> Vec<u8> {
    let mut p = [0u8; 126];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = if writable { 0xff } else { 0x00 };
    frame([0x23, 0x00], &p)
}

/// Rejected before anything reaches the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// A block outside the allowlist was requested.
    ForbiddenBlock(u8),
    /// A page payload was not exactly one page.
    WrongPageSize(usize),
    /// A linear-address frame targeted something outside the allowed range.
    ForbiddenAddress(u32),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForbiddenBlock(b) => write!(
                f,
                "refusing to touch flash block 0x{b:02x}; only block 0x{PARAM_BLOCK:02x} \
                 (receiver parameters) may be written"
            ),
            Self::WrongPageSize(n) => {
                write!(f, "page payload is {n} bytes, must be {FLASH_PAGE_BYTES}")
            }
            Self::ForbiddenAddress(a) => write!(
                f,
                "refusing linear flash access at 0x{a:08x}; only the screen-size \
                 record at 0x{SCREEN_RECORD_ADDR:08x} may be reached this way"
            ),
        }
    }
}

impl std::error::Error for WriteError {}

/// Erase the parameter block (all 64KB; the caller must hold a full copy).
///
/// # Errors
/// Refuses any block other than [`PARAM_BLOCK`].
pub fn erase_block(rcv_index: u16, block: u8) -> Result<Vec<u8>, WriteError> {
    if block != PARAM_BLOCK {
        return Err(WriteError::ForbiddenBlock(block));
    }
    Ok(erase_block_unchecked(rcv_index, block))
}

/// Erase a firmware block. Separate from [`erase_block`] so that writing
/// firmware is always an explicit act.
///
/// # Errors
/// Refuses any block outside [`FIRMWARE_BLOCKS`].
pub fn erase_firmware_block(rcv_index: u16, block: u8) -> Result<Vec<u8>, WriteError> {
    if !FIRMWARE_BLOCKS.contains(&block) {
        return Err(WriteError::ForbiddenBlock(block));
    }
    Ok(erase_block_unchecked(rcv_index, block))
}

/// Write one page of a firmware block.
///
/// # Errors
/// Refuses any block outside [`FIRMWARE_BLOCKS`], or a payload that is not
/// exactly one page.
pub fn write_firmware_page(
    rcv_index: u16,
    block: u8,
    page: u8,
    data: &[u8],
) -> Result<Vec<u8>, WriteError> {
    if !FIRMWARE_BLOCKS.contains(&block) {
        return Err(WriteError::ForbiddenBlock(block));
    }
    if data.len() != FLASH_PAGE_BYTES {
        return Err(WriteError::WrongPageSize(data.len()));
    }
    Ok(write_page_unchecked(rcv_index, block, page, data))
}

fn erase_block_unchecked(rcv_index: u16, block: u8) -> Vec<u8> {
    let mut p = [0u8; 126];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_ERASE;
    p[4] = 0x01;
    p[5..7].copy_from_slice(&(u16::from(block) << 8).to_be_bytes());
    frame([0x06, 0x00], &p)
}

/// Write one 256-byte page within the parameter block.
///
/// # Errors
/// Refuses any block other than [`PARAM_BLOCK`], or a payload that is not
/// exactly one page.
pub fn write_page(rcv_index: u16, block: u8, page: u8, data: &[u8]) -> Result<Vec<u8>, WriteError> {
    if block != PARAM_BLOCK {
        return Err(WriteError::ForbiddenBlock(block));
    }
    if data.len() != FLASH_PAGE_BYTES {
        return Err(WriteError::WrongPageSize(data.len()));
    }
    Ok(write_page_unchecked(rcv_index, block, page, data))
}

fn write_page_unchecked(rcv_index: u16, block: u8, page: u8, data: &[u8]) -> Vec<u8> {
    // Mirrors the read frame: [1..3] index, [3] opcode, [4] flag, [5] block,
    // [6] page; the vendor copies data to payload offset 0x0a (index 8).
    let mut p = vec![0u8; 8 + FLASH_PAGE_BYTES];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_WRITE;
    p[5] = block;
    p[6] = page;
    p[8..].copy_from_slice(data);
    frame([0x06, 0x00], &p)
}

/// Write the screen-size record back to flash (linear-address frame 0x1900).
///
/// # Errors
/// Refuses any address outside the screen-size record, or a wrong length.
pub fn write_screen_record(rcv_index: u16, addr: u32, data: &[u8]) -> Result<Vec<u8>, WriteError> {
    if data.len() != SCREEN_RECORD_LEN {
        return Err(WriteError::WrongPageSize(data.len()));
    }
    let end = addr
        .checked_add(SCREEN_RECORD_LEN as u32)
        .ok_or(WriteError::ForbiddenAddress(addr))?;
    if addr < LINEAR_ALLOWED.start || end > LINEAR_ALLOWED.end {
        return Err(WriteError::ForbiddenAddress(addr));
    }
    let mut p = vec![0u8; 12 + data.len() + 4];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_WRITE;
    p[4..8].copy_from_slice(&addr.to_be_bytes());
    p[8..12].copy_from_slice(&(SCREEN_RECORD_LEN as u32).to_be_bytes());
    p[12..12 + data.len()].copy_from_slice(data);
    Ok(frame([0x19, 0x00], &p))
}

/// Read `len` bytes at any flash address (used to dump firmware and survey
/// unmapped regions).
#[must_use]
pub fn read_flash_linear(rcv_index: u16, addr: u32, len: u32) -> Vec<u8> {
    let mut p = vec![0u8; 12];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_READ;
    p[4..8].copy_from_slice(&addr.to_be_bytes());
    p[8..12].copy_from_slice(&len.to_be_bytes());
    frame([0x19, 0x00], &p)
}

/// Extract the flash bytes from a reply frame: Ethernet header, a one-byte
/// status, then the data, zero-padded to a fixed frame size.
pub fn flash_reply_data(eth_frame: &[u8]) -> Option<&[u8]> {
    if eth_frame.len() < 15 || eth_frame[12..14] != FLASH_REPLY_TYPE {
        return None;
    }
    let data = &eth_frame[15..];
    Some(&data[..data.len().min(FLASH_CHUNK_BYTES)])
}

#[cfg(test)]
mod linear_tests {
    use super::*;

    #[test]
    fn screen_record_write_matches_the_documented_layout() {
        let data = vec![0xabu8; SCREEN_RECORD_LEN];
        let f = write_screen_record(0, SCREEN_RECORD_ADDR, &data).unwrap();
        assert_eq!(&f[12..14], &[0x19, 0x00]);
        assert_eq!(f[17], FLASH_OP_WRITE);
        assert_eq!(&f[18..22], &SCREEN_RECORD_ADDR.to_be_bytes());
        assert_eq!(&f[22..26], &(SCREEN_RECORD_LEN as u32).to_be_bytes());
        assert_eq!(&f[26..26 + SCREEN_RECORD_LEN], &data[..]);
        assert_eq!(f.len(), 286);
    }

    #[test]
    fn linear_frames_refuse_every_address_but_the_screen_record() {
        let data = vec![0u8; SCREEN_RECORD_LEN];
        for addr in [
            0x0000_0000,
            0x0007_0000,
            0x0007_efff,
            0x0007_f001,
            0x0008_0000,
            0xffff_ffff,
        ] {
            assert_eq!(
                write_screen_record(0, addr, &data),
                Err(WriteError::ForbiddenAddress(addr)),
                "address 0x{addr:08x} must be refused"
            );
        }
    }

    #[test]
    fn the_screen_record_address_is_allowed() {
        let data = vec![0u8; SCREEN_RECORD_LEN];
        assert!(write_screen_record(0, SCREEN_RECORD_ADDR, &data).is_ok());
    }

    #[test]
    fn a_wrong_length_payload_is_refused() {
        assert_eq!(
            write_screen_record(0, SCREEN_RECORD_ADDR, &[0; 128]),
            Err(WriteError::WrongPageSize(128))
        );
    }

    #[test]
    fn a_linear_read_carries_no_data() {
        let f = read_flash_linear(0, SCREEN_RECORD_ADDR, SCREEN_RECORD_LEN as u32);
        assert_eq!(&f[12..14], &[0x19, 0x00]);
        assert_eq!(f[17], FLASH_OP_READ);
        assert_eq!(&f[18..22], &SCREEN_RECORD_ADDR.to_be_bytes());
        assert_eq!(&f[22..26], &256u32.to_be_bytes());
        assert_eq!(f.len(), 26);
    }
}

#[cfg(test)]
mod firmware_tests {
    use super::*;

    #[test]
    fn firmware_writes_are_confined_to_the_primary_image() {
        let page = [0u8; FLASH_PAGE_BYTES];
        for block in FIRMWARE_BLOCKS {
            assert!(erase_firmware_block(0, block).is_ok());
            assert!(write_firmware_page(0, block, 0, &page).is_ok());
        }
        // The golden bank and everything past the primary image are refused.
        for block in [GOLDEN_BLOCK, 0x0b, 0x0c, 0x21, 0xff] {
            assert_eq!(
                erase_firmware_block(0, block),
                Err(WriteError::ForbiddenBlock(block)),
                "block 0x{block:02x} must be refused"
            );
            assert_eq!(
                write_firmware_page(0, block, 0, &page),
                Err(WriteError::ForbiddenBlock(block))
            );
        }
    }

    #[test]
    fn the_golden_bank_is_outside_the_writable_range() {
        assert!(!FIRMWARE_BLOCKS.contains(&GOLDEN_BLOCK));
    }

    #[test]
    fn the_parameter_helpers_still_refuse_firmware_blocks() {
        assert_eq!(erase_block(0, 0x00), Err(WriteError::ForbiddenBlock(0x00)));
        assert_eq!(
            write_page(0, 0x00, 0, &[0u8; FLASH_PAGE_BYTES]),
            Err(WriteError::ForbiddenBlock(0x00))
        );
    }
}

#[cfg(test)]
mod writable_tests {
    use super::*;

    #[test]
    fn unlock_uses_the_negated_flag() {
        let f = set_program_writable(0, true);
        assert_eq!(&f[12..14], &[0x23, 0x00]);
        assert_eq!(f[17], 0xff, "enable is 0xff, not 0x01");
        assert_eq!(f.len(), 140);
    }

    #[test]
    fn relock_clears_it() {
        assert_eq!(set_program_writable(0, false)[17], 0x00);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CARD_MAC, SENDER_MAC};

    #[test]
    fn read_frame_matches_the_documented_layout() {
        let f = read_flash(0, FLASH_PAGE_BASIC_PARAM);
        assert_eq!(f.len(), 140);
        assert_eq!(&f[0..6], &CARD_MAC);
        assert_eq!(&f[6..12], &SENDER_MAC);
        assert_eq!(&f[12..14], &[0x06, 0x00]);
        assert_eq!(f[17], FLASH_OP_READ);
        assert_eq!(&f[19..21], &[0x07, 0x80]);
    }

    #[test]
    fn a_read_frame_carries_no_data() {
        let f = read_flash(0, FLASH_PAGE_BASIC_PARAM);
        assert!(f[21..].iter().all(|&b| b == 0));
    }

    #[test]
    fn writes_outside_the_parameter_block_are_refused() {
        for block in [0x00, 0x01, 0x06, 0x08, 0xff] {
            assert_eq!(
                erase_block(0, block),
                Err(WriteError::ForbiddenBlock(block)),
                "block 0x{block:02x} must be refused"
            );
            assert_eq!(
                write_page(0, block, 0, &[0; FLASH_PAGE_BYTES]),
                Err(WriteError::ForbiddenBlock(block))
            );
        }
    }

    #[test]
    fn the_parameter_block_is_allowed() {
        assert!(erase_block(0, PARAM_BLOCK).is_ok());
        assert!(write_page(0, PARAM_BLOCK, 0x80, &[0; FLASH_PAGE_BYTES]).is_ok());
    }

    #[test]
    fn a_page_write_must_be_exactly_one_page() {
        assert_eq!(
            write_page(0, PARAM_BLOCK, 0, &[0; 255]),
            Err(WriteError::WrongPageSize(255))
        );
        assert_eq!(
            write_page(0, PARAM_BLOCK, 0, &[0; 257]),
            Err(WriteError::WrongPageSize(257))
        );
    }

    #[test]
    fn write_frame_carries_the_page_data_at_the_documented_offset() {
        let data: Vec<u8> = (0..=255u8).collect();
        let f = write_page(1, PARAM_BLOCK, 0x81, &data).unwrap();
        // 12 MAC + 2 type + 8 header + 256 data.
        assert_eq!(f.len(), 278);
        assert_eq!(f[17], FLASH_OP_WRITE);
        assert_eq!(f[18], 0x00, "flag byte");
        assert_eq!(f[19], PARAM_BLOCK);
        assert_eq!(f[20], 0x81);
        assert_eq!(&f[22..], &data[..]);
    }

    #[test]
    fn erase_frame_carries_the_block_in_the_page_high_byte() {
        let f = erase_block(0, PARAM_BLOCK).unwrap();
        assert_eq!(f.len(), 140);
        assert_eq!(f[17], FLASH_OP_ERASE);
        assert_eq!(f[18], 0x01);
        assert_eq!(&f[19..21], &[PARAM_BLOCK, 0x00]);
    }
}
