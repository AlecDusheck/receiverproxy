//! Flash read, erase and write frames.
//!
//! Reads are unrestricted; every write builder is allowlisted: page-addressed
//! (type 0x0600) to the parameter block or the primary firmware blocks,
//! linear (type 0x1900) to the screen-size record only.

use super::{frame_with, indexed};

/// A read frame carries no data, so it cannot modify the card wherever it
/// is pointed.
const FLASH_OP_READ: u8 = 0x44;

/// Page index (256-byte pages) of the receiver's basic parameters.
pub const FLASH_PAGE_BASIC_PARAM: u16 = 0x0780;

/// Pages advance by 4 per 1024-byte chunk read.
pub const FLASH_PAGES_PER_CHUNK: u16 = 4;

const FLASH_OP_ERASE: u8 = 0x23;

/// Page write; the same opcode writes an EEPROM record (`eeprom::write`).
const FLASH_OP_WRITE: u8 = 0x85;

pub const FLASH_PAGE_BYTES: usize = 256;

/// The 64 KB block holding the receiver parameters: the only block
/// `erase_block` and `write_page` accept.
pub const PARAM_BLOCK: u8 = 0x07;

/// Blocks of the primary firmware image; the golden backup at
/// [`GOLDEN_BLOCK`] is outside every allowlist.
pub const FIRMWARE_BLOCKS: std::ops::Range<u8> = 0x00..0x0b;

/// First block of the golden backup image.
pub const GOLDEN_BLOCK: u8 = 0x20;

/// Flash address of the screen-size record, reachable only through the
/// linear-address frames.
pub const SCREEN_RECORD_ADDR: u32 = 0x0007_f000;

pub const SCREEN_RECORD_LEN: usize = 256;

/// Linear-address writes can reach firmware, so exactly one range is allowed.
const LINEAR_ALLOWED: std::ops::Range<u32> =
    SCREEN_RECORD_ADDR..SCREEN_RECORD_ADDR + SCREEN_RECORD_LEN as u32;

/// Frame type of a flash-read reply.
pub const FLASH_REPLY_TYPE: [u8; 2] = [0x09, 0x01];

/// Flash bytes per reply frame.
pub const FLASH_CHUNK_BYTES: usize = 1024;

/// Read 1024 bytes starting at `page` (a 256-byte page index): type 0x0600,
/// 126-byte payload.
#[must_use]
pub fn read_flash(rcv_index: u16, page: u16) -> Vec<u8> {
    paged(rcv_index, FLASH_OP_READ, page)
}

/// A 0x0600 read or erase: `[4]` = 0x01, `[5..7]` page BE, no data.
fn paged(rcv_index: u16, opcode: u8, page: u16) -> Vec<u8> {
    frame_with([0x06, 0x00], 126, |p| {
        indexed(p, rcv_index, opcode);
        p[4] = 0x01;
        p[5..7].copy_from_slice(&page.to_be_bytes());
    })
}

/// Unlock or relock the write-protected program region.
///
/// Erases and page writes silently do nothing while locked. The vendor negates
/// the flag, so enable is 0xff (`unlock_uses_the_negated_flag`). Relock on
/// every exit path.
#[must_use]
pub fn set_program_writable(rcv_index: u16, writable: bool) -> Vec<u8> {
    frame_with([0x23, 0x00], 126, |p| {
        indexed(p, rcv_index, if writable { 0xff } else { 0x00 });
    })
}

/// A write refused before any frame is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    ForbiddenBlock(u8),
    WrongPageSize(usize),
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

fn param_block(block: u8) -> Result<u8, WriteError> {
    if block == PARAM_BLOCK {
        Ok(block)
    } else {
        Err(WriteError::ForbiddenBlock(block))
    }
}

fn firmware_block(block: u8) -> Result<u8, WriteError> {
    if FIRMWARE_BLOCKS.contains(&block) {
        Ok(block)
    } else {
        Err(WriteError::ForbiddenBlock(block))
    }
}

fn one_page(data: &[u8]) -> Result<&[u8], WriteError> {
    if data.len() == FLASH_PAGE_BYTES {
        Ok(data)
    } else {
        Err(WriteError::WrongPageSize(data.len()))
    }
}

/// Erase the whole 64 KB parameter block; the caller must hold a full copy.
///
/// # Errors
/// Refuses any block other than [`PARAM_BLOCK`].
pub fn erase_block(rcv_index: u16, block: u8) -> Result<Vec<u8>, WriteError> {
    Ok(erase_block_unchecked(rcv_index, param_block(block)?))
}

/// Erase a firmware block. Kept apart from [`erase_block`] so a firmware
/// write is never reachable through the parameter path.
///
/// # Errors
/// Refuses any block outside [`FIRMWARE_BLOCKS`].
pub fn erase_firmware_block(rcv_index: u16, block: u8) -> Result<Vec<u8>, WriteError> {
    Ok(erase_block_unchecked(rcv_index, firmware_block(block)?))
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
    Ok(write_page_unchecked(rcv_index, firmware_block(block)?, page, one_page(data)?))
}

fn erase_block_unchecked(rcv_index: u16, block: u8) -> Vec<u8> {
    paged(rcv_index, FLASH_OP_ERASE, u16::from(block) << 8)
}

/// Write one 256-byte page within the parameter block.
///
/// # Errors
/// Refuses any block other than [`PARAM_BLOCK`], or a payload that is not
/// exactly one page.
pub fn write_page(rcv_index: u16, block: u8, page: u8, data: &[u8]) -> Result<Vec<u8>, WriteError> {
    Ok(write_page_unchecked(rcv_index, param_block(block)?, page, one_page(data)?))
}

fn write_page_unchecked(rcv_index: u16, block: u8, page: u8, data: &[u8]) -> Vec<u8> {
    // [1..3] index, [3] opcode, [4] flag, [5] block, [6] page, data at 8
    // (`write_frame_carries_the_page_data_at_the_documented_offset`).
    frame_with([0x06, 0x00], 8 + FLASH_PAGE_BYTES, |p| {
        indexed(p, rcv_index, FLASH_OP_WRITE);
        p[5] = block;
        p[6] = page;
        p[8..].copy_from_slice(data);
    })
}

/// Write the screen-size record (linear-address frame, type 0x1900).
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
    Ok(linear(rcv_index, FLASH_OP_WRITE, addr, SCREEN_RECORD_LEN as u32, data, 4))
}

/// Read `len` bytes at any flash address.
#[must_use]
pub fn read_flash_linear(rcv_index: u16, addr: u32, len: u32) -> Vec<u8> {
    linear(rcv_index, FLASH_OP_READ, addr, len, &[], 0)
}

/// A linear-address 0x1900 frame: `[4..8]` address, `[8..12]` length, data at
/// 12, then `tail` zero bytes.
fn linear(rcv_index: u16, opcode: u8, addr: u32, len: u32, data: &[u8], tail: usize) -> Vec<u8> {
    frame_with([0x19, 0x00], 12 + data.len() + tail, |p| {
        indexed(p, rcv_index, opcode);
        p[4..8].copy_from_slice(&addr.to_be_bytes());
        p[8..12].copy_from_slice(&len.to_be_bytes());
        p[12..12 + data.len()].copy_from_slice(data);
    })
}

/// The flash bytes of a reply: header, one status byte at 14, then up to
/// [`FLASH_CHUNK_BYTES`] of data.
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
