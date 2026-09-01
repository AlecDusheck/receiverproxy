//! Colorlight receiver-card layer-2 protocol (as spoken by LEDVISION sender
//! cards / FPP's ColorLight-5a-75 output, reverse-engineered by the community).
//!
//! All frames are raw Ethernet:
//!   dst MAC 11:22:33:44:55:66, src MAC 22:22:33:44:55:66
//! "EtherType" is abused as a 2-byte packet type:
//!   0x0700       discovery request (270 zero bytes)
//!   0x0805       discovery response from the card (src 11:22:33:44:55:66)
//!   0x0107       display/vsync frame (98 bytes; carries brightness)
//!   0x0Abb       brightness (bb = brightness value; 63-byte payload)
//!   0x55rr       pixel row data (rr = row number MSB)

pub mod params;

pub const CARD_MAC: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
pub const SENDER_MAC: [u8; 6] = [0x22, 0x22, 0x33, 0x44, 0x55, 0x66];

fn frame(ethertype: [u8; 2], payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(14 + payload.len());
    f.extend_from_slice(&CARD_MAC);
    f.extend_from_slice(&SENDER_MAC);
    f.extend_from_slice(&ethertype);
    f.extend_from_slice(payload);
    f
}

/// Discovery request: type 0x0700, 270 zero bytes.
pub fn discovery() -> Vec<u8> {
    frame([0x07, 0x00], &[0u8; 270])
}

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

/// Opcode selecting a flash *read* in a card-flash operation frame.
///
/// This is the one byte that decides read versus write: the write paths use
/// other opcodes (0x85 for gamma tables, 0x66 for EEPROM) and always supply a
/// data buffer. Never send a flash-operation frame with a different value here
/// unless you intend to write.
const FLASH_OP_READ: u8 = 0x44;

/// Flash region holding the receiver's basic parameters, in 256-byte pages.
pub const FLASH_PAGE_BASIC_PARAM: u16 = 0x0780;

/// Pages advance by 4 per 1024-byte chunk read.
pub const FLASH_PAGES_PER_CHUNK: u16 = 4;

/// Read-only card-flash request: type 0x0600, 128-byte payload (140-byte frame).
///
/// Requests 1024 bytes starting at `page` (a 256-byte page index) from the
/// receiver at `rcv_index`. Carries no data of its own — the builder in the
/// vendor library skips its payload copy when there is nothing to write, which
/// is what makes this frame inherently incapable of modifying the card.
pub fn read_flash(rcv_index: u16, page: u16) -> Vec<u8> {
    let mut p = [0u8; 126];
    p[0] = 0x00;
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_READ;
    p[4] = 0x01;
    p[5..7].copy_from_slice(&page.to_be_bytes());
    frame([0x06, 0x00], &p)
}

/// Opcode that erases a flash block.
const FLASH_OP_ERASE: u8 = 0x23;

/// Opcode that writes one 256-byte flash page.
const FLASH_OP_WRITE: u8 = 0x85;

/// Bytes in one flash page.
pub const FLASH_PAGE_BYTES: usize = 256;

/// The only 64KB block this crate will ever touch.
///
/// Block 0x07 holds the receiver parameters. Firmware and the FPGA bitstream
/// live in other blocks; writing those could leave the card unable to answer at
/// all, so the constructors below refuse any other block outright. This is an
/// allowlist rather than a denylist on purpose.
pub const PARAM_BLOCK: u8 = 0x07;

/// Blocks holding the primary firmware image.
///
/// The card keeps two bitstreams: a primary at block 0x00 and a golden backup
/// at block 0x20. Only the primary may be written, so the golden bank always
/// remains as an in-hardware fallback.
pub const FIRMWARE_BLOCKS: std::ops::Range<u8> = 0x00..0x0b;

/// First block of the golden backup image, which must never be written.
pub const GOLDEN_BLOCK: u8 = 0x20;

/// Rejected before anything reaches the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// A block other than the parameter block was requested.
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

/// Erase the parameter block. Destroys the whole 64KB block, so the caller must
/// already hold a full copy of it to write back.
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
/// firmware is always an explicit, deliberate act.
///
/// # Errors
/// Refuses any block outside [`FIRMWARE_BLOCKS`].
pub fn erase_firmware_block(rcv_index: u16, block: u8) -> Result<Vec<u8>, WriteError> {
    if !FIRMWARE_BLOCKS.contains(&block) {
        return Err(WriteError::ForbiddenBlock(block));
    }
    Ok(erase_block_unchecked(rcv_index, block))
}

/// Opcode the firmware-upgrade path uses to write a chunk.
const FIRMWARE_OP_WRITE: u8 = 0x62;

/// Write one 256-byte chunk of firmware.
///
/// The upgrade path uses its own frame type and layout, distinct from the
/// parameter-flash frames: type 0x2600, opcode at payload+5, block and page at
/// payload+7 and +8, data from payload+0x0a.
///
/// # Errors
/// Refuses any block outside [`FIRMWARE_BLOCKS`], or a payload that is not
/// exactly one page.
pub fn write_firmware_chunk(
    rcv_index: u16,
    block: u8,
    page: u8,
    data: &[u8],
    opcode: u8,
) -> Result<Vec<u8>, WriteError> {
    if !FIRMWARE_BLOCKS.contains(&block) {
        return Err(WriteError::ForbiddenBlock(block));
    }
    if data.len() != FLASH_PAGE_BYTES {
        return Err(WriteError::WrongPageSize(data.len()));
    }
    let mut p = vec![0u8; 8 + FLASH_PAGE_BYTES];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = opcode;
    p[5] = block;
    p[6] = page;
    p[8..].copy_from_slice(data);
    Ok(frame([0x26, 0x00], &p))
}

/// The default firmware write opcode.
#[must_use]
pub const fn firmware_write_opcode() -> u8 {
    FIRMWARE_OP_WRITE
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
    Ok(write_page_unchecked(rcv_index, block, page, data, 0))
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
    write_page_flag(rcv_index, block, page, data, 0)
}

/// As [`write_page`], with control over the flag byte, for probing regions the
/// card refuses to write with the usual value.
///
/// # Errors
/// Refuses any block other than [`PARAM_BLOCK`], or a payload that is not
/// exactly one page.
pub fn write_page_flag(
    rcv_index: u16,
    block: u8,
    page: u8,
    data: &[u8],
    flag: u8,
) -> Result<Vec<u8>, WriteError> {
    if block != PARAM_BLOCK {
        return Err(WriteError::ForbiddenBlock(block));
    }
    if data.len() != FLASH_PAGE_BYTES {
        return Err(WriteError::WrongPageSize(data.len()));
    }
    Ok(write_page_unchecked(rcv_index, block, page, data, flag))
}

fn write_page_unchecked(rcv_index: u16, block: u8, page: u8, data: &[u8], flag: u8) -> Vec<u8> {
    // Layout mirrors the read frame, which is verified against hardware:
    // [0] zero, [1..3] receiver index, [3] opcode, [4] flag, [5..7] block/page.
    // The vendor builder copies payload data to offset 0x0a of the frame
    // payload, which is index 8 here, leaving index 7 reserved.
    let mut p = vec![0u8; 8 + FLASH_PAGE_BYTES];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_WRITE;
    p[4] = flag;
    p[5] = block;
    p[6] = page;
    p[8..].copy_from_slice(data);
    frame([0x06, 0x00], &p)
}

/// Set the receiver layout: this card's size and the size of the whole screen.
///
/// Field positions follow the type 0x02 packet documented by FPP, expressed
/// here relative to the payload that follows the two type bytes.
pub fn set_layout(
    rcv_index: u16,
    recv_w: u16,
    recv_h: u16,
    x_offset: u16,
    y_offset: u16,
    total_w: u16,
    total_h: u16,
) -> Vec<u8> {
    let mut p = [0u8; 98];
    p[0..2].copy_from_slice(&rcv_index.to_be_bytes());
    p[6..8].copy_from_slice(&recv_w.to_be_bytes());
    p[8..10].copy_from_slice(&recv_h.to_be_bytes());
    p[12..14].copy_from_slice(&x_offset.to_be_bytes());
    p[14..16].copy_from_slice(&y_offset.to_be_bytes());
    p[16..18].copy_from_slice(&total_w.to_be_bytes());
    p[18..20].copy_from_slice(&total_h.to_be_bytes());
    frame([0x02, 0x00], &p)
}

/// Put the card into its built-in test-pattern mode.
///
/// The card generates the pattern itself, so this exercises the panel without
/// any pixel data from us. RAM only: it writes nothing to flash.
pub fn test_mode(rcv_index: u16, pattern: u8) -> Vec<u8> {
    let mut p = vec![0u8; 0x109];
    p[0] = 0x00;
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = 0x09;
    p[4] = pattern;
    frame([0x33, 0x00], &p)
}

/// Ask the card to reload its parameters from flash, avoiding a power cycle.
///
/// Carries no data and uses an opcode outside the data-carrying set, so it
/// cannot write anything.
pub fn reload_params(rcv_index: u16) -> Vec<u8> {
    let mut p = [0u8; 126];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = 0x79;
    frame([0x06, 0x00], &p)
}

/// Flash address of the screen-size record.
///
/// This page sits outside the window the page-addressed frames can reach, so
/// it is only writable through the linear-address frames below.
pub const SCREEN_RECORD_ADDR: u32 = 0x0007_f000;

/// Bytes in the screen-size record.
pub const SCREEN_RECORD_LEN: usize = 256;

/// Linear-address frames can reach any byte of flash, including firmware, so
/// this crate permits exactly one address range: the screen-size record.
const LINEAR_ALLOWED: std::ops::Range<u32> =
    SCREEN_RECORD_ADDR..SCREEN_RECORD_ADDR + SCREEN_RECORD_LEN as u32;

fn linear_frame(rcv_index: u16, opcode: u8, addr: u32, data: &[u8]) -> Result<Vec<u8>, WriteError> {
    // Deliberately an allowlist of one range: a wrong address here could
    // overwrite firmware, which the page-addressed frames physically cannot do.
    let end = addr
        .checked_add(data.len().max(SCREEN_RECORD_LEN) as u32)
        .ok_or(WriteError::ForbiddenAddress(addr))?;
    if addr < LINEAR_ALLOWED.start || end > LINEAR_ALLOWED.end {
        return Err(WriteError::ForbiddenAddress(addr));
    }
    let mut p = vec![0u8; 12 + data.len() + 4];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = opcode;
    p[4..8].copy_from_slice(&addr.to_be_bytes());
    p[8..12].copy_from_slice(&(SCREEN_RECORD_LEN as u32).to_be_bytes());
    p[12..12 + data.len()].copy_from_slice(data);
    Ok(frame([0x19, 0x00], &p))
}

/// Write the screen-size record back to flash.
///
/// # Errors
/// Refuses any address outside the screen-size record, or a wrong length.
pub fn write_screen_record(rcv_index: u16, addr: u32, data: &[u8]) -> Result<Vec<u8>, WriteError> {
    if data.len() != SCREEN_RECORD_LEN {
        return Err(WriteError::WrongPageSize(data.len()));
    }
    linear_frame(rcv_index, FLASH_OP_WRITE, addr, data)
}

/// Read the screen-size record. Carries no data, so it cannot modify anything.
///
/// # Errors
/// Refuses any address outside the screen-size record.
pub fn read_screen_record(rcv_index: u16, addr: u32) -> Result<Vec<u8>, WriteError> {
    linear_frame(rcv_index, FLASH_OP_READ, addr, &[])
}

/// Read any flash address.
///
/// Unlike the write path this is unrestricted, because a read frame carries no
/// data and the opcode is not one the card will attach data to — it cannot
/// modify the card wherever it is pointed. Used to dump firmware and to survey
/// regions we have not mapped.
#[must_use]
pub fn read_flash_linear(rcv_index: u16, addr: u32, len: u32) -> Vec<u8> {
    let mut p = vec![0u8; 12];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_READ;
    p[4..8].copy_from_slice(&addr.to_be_bytes());
    p[8..12].copy_from_slice(&len.to_be_bytes());
    frame([0x19, 0x00], &p)
}

/// Ask the card to describe its firmware-upgrade capabilities.
///
/// A discovery-family frame distinguished by the `ff ff ff` marker and a magic
/// number. Read-only: it carries no data and no write opcode. The reply states
/// how long the card expects its firmware image to be, which tells us which
/// image format its bootloader wants.
#[must_use]
pub fn upgrade_info() -> Vec<u8> {
    let mut p = vec![0u8; 270];
    p[1..4].copy_from_slice(&[0xff, 0xff, 0xff]);
    p[4] = 0x01;
    p[6..10].copy_from_slice(&[0x43, 0x57, 0x83, 0x97]);
    p[10] = 0x09;
    frame([0x07, 0x00], &p)
}

/// What the card reports about its firmware image and recovery options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpgradeInfo {
    /// Size the bootloader expects the firmware image to be.
    pub declared_len: u32,
    pub capabilities: u8,
}

impl UpgradeInfo {
    /// True when the card keeps a golden image it can fall back to.
    #[must_use]
    pub const fn has_golden(self) -> bool {
        self.capabilities & 0b0010 != 0
    }

    /// True when the card accepts golden-bank upgrades.
    #[must_use]
    pub const fn supports_golden_upgrade(self) -> bool {
        self.capabilities & 0b1000 != 0
    }

    /// True when the card can stage an image in SDRAM before committing it.
    #[must_use]
    pub const fn supports_sdram_staging(self) -> bool {
        self.capabilities & 0b0001 != 0
    }
}

/// Locate the upgrade descriptor inside a reply.
///
/// The absolute offset depends on framing we have not pinned down, but the
/// spacings between fields come from fixed instruction offsets. So anchor on
/// the length's high byte — a ~721 KB image starts `0b 00`, which is rare in an
/// otherwise sparse reply — and read the neighbours relative to it.
#[must_use]
pub fn parse_upgrade_info(reply: &[u8]) -> Option<UpgradeInfo> {
    for i in 5..reply.len().saturating_sub(2) {
        if reply[i] != 0x0b || reply[i + 1] != 0x00 {
            continue;
        }
        let declared_len =
            u32::from(reply[i]) << 16 | u32::from(reply[i + 1]) << 8 | u32::from(reply[i + 2]);
        // Only 0x0b0000 and 0x0b0080 are plausible image lengths.
        if declared_len != 0x000b_0000 && declared_len != 0x000b_0080 {
            continue;
        }
        return Some(UpgradeInfo {
            declared_len,
            capabilities: reply[i - 5],
        });
    }
    None
}

/// Frame type the card answers a flash read with.
pub const FLASH_REPLY_TYPE: [u8; 2] = [0x09, 0x01];

/// Payload bytes of flash data carried by each reply frame.
pub const FLASH_CHUNK_BYTES: usize = 1024;

/// Extract the flash bytes from a reply frame.
///
/// A reply is the Ethernet header, a one-byte status, then the flash data,
/// zero-padded out to a fixed frame size.
pub fn flash_reply_data(eth_frame: &[u8]) -> Option<&[u8]> {
    if eth_frame.len() < 15 || eth_frame[12..14] != FLASH_REPLY_TYPE {
        return None;
    }
    let data = &eth_frame[15..];
    Some(&data[..data.len().min(FLASH_CHUNK_BYTES)])
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

impl std::str::FromStr for ColorOrder {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "rgb" => Ok(Self::Rgb),
            "bgr" => Ok(Self::Bgr),
            "grb" => Ok(Self::Grb),
            _ => Err(format!("unknown color order {s:?} (rgb|bgr|grb)")),
        }
    }
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

/// Max pixels per row packet (keeps the frame under the 1500-byte MTU).
pub const MAX_PIXELS_PER_PACKET: usize = 497;

/// Parsed discovery response (best effort — field meanings from community
/// reverse engineering of 5A-75B; other cards may differ).
pub struct DiscoveryInfo {
    pub card_id: u8,
    pub ver_major: u8,
    pub ver_minor: u8,
    pub cols: u16,
    pub rows: u16,
    pub controller: u8,
    pub raw: Vec<u8>,
}

pub fn parse_discovery_response(eth_frame: &[u8]) -> Option<DiscoveryInfo> {
    if eth_frame.len() < 14 + 63 {
        return None;
    }
    if eth_frame[12] != 0x08 || eth_frame[13] != 0x05 {
        return None;
    }
    let p = &eth_frame[14..];
    Some(DiscoveryInfo {
        card_id: p[0],
        ver_major: p[1],
        ver_minor: p[2],
        cols: u16::from_be_bytes([p[20], p[21]]),
        rows: u16::from_be_bytes([p[22], p[23]]),
        controller: *p.get(62).unwrap_or(&0),
        raw: p.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Everything past the small header must be zero: a frame with no data
        // cannot modify the card.
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
        // 12 MAC + 2 type + 8 header + 256 data, matching the vendor builder
        // which copies data to payload offset 0x0a.
        assert_eq!(f.len(), 278);
        assert_eq!(f[17], FLASH_OP_WRITE);
        assert_eq!(f[19], PARAM_BLOCK);
        assert_eq!(f[20], 0x81);
        assert_eq!(&f[22..], &data[..]);
    }

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
    fn a_screen_record_read_carries_no_data() {
        let f = read_screen_record(0, SCREEN_RECORD_ADDR).unwrap();
        assert_eq!(f[17], FLASH_OP_READ);
        assert!(f[26..].iter().all(|&b| b == 0));
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
            assert_eq!(
                read_screen_record(0, addr),
                Err(WriteError::ForbiddenAddress(addr))
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
        // Config writes must not stray into the firmware image.
        assert_eq!(erase_block(0, 0x00), Err(WriteError::ForbiddenBlock(0x00)));
        assert_eq!(
            write_page(0, 0x00, 0, &[0u8; FLASH_PAGE_BYTES]),
            Err(WriteError::ForbiddenBlock(0x00))
        );
    }
}

#[cfg(test)]
mod firmware_frame_tests {
    use super::*;

    #[test]
    fn firmware_chunk_matches_the_documented_layout() {
        let data: Vec<u8> = (0..=255u8).collect();
        let f = write_firmware_chunk(0, 0x03, 0x40, &data, FIRMWARE_OP_WRITE).unwrap();
        assert_eq!(f.len(), 278, "12 MAC + 266 payload");
        assert_eq!(&f[12..14], &[0x26, 0x00]);
        assert_eq!(f[17], FIRMWARE_OP_WRITE);
        assert_eq!(f[19], 0x03, "block at payload+7");
        assert_eq!(f[20], 0x40, "page at payload+8");
        assert_eq!(&f[22..], &data[..], "data from payload+0x0a");
    }

    #[test]
    fn firmware_chunks_refuse_the_golden_bank() {
        let data = vec![0u8; FLASH_PAGE_BYTES];
        assert_eq!(
            write_firmware_chunk(0, GOLDEN_BLOCK, 0, &data, FIRMWARE_OP_WRITE),
            Err(WriteError::ForbiddenBlock(GOLDEN_BLOCK))
        );
    }
}

#[cfg(test)]
mod upgrade_info_tests {
    use super::*;

    #[test]
    fn the_query_matches_the_documented_layout() {
        let f = upgrade_info();
        assert_eq!(f.len(), 284);
        assert_eq!(&f[12..14], &[0x07, 0x00]);
        assert_eq!(&f[15..18], &[0xff, 0xff, 0xff], "upgrade-info marker");
        assert_eq!(&f[20..24], &[0x43, 0x57, 0x83, 0x97], "magic");
        assert_eq!(f[24], 0x09, "sub-command");
    }

    #[test]
    fn the_query_carries_no_write_opcode() {
        // Every byte past the fixed header is zero, so this cannot modify.
        assert!(upgrade_info()[25..].iter().all(|&b| b == 0));
    }

    #[test]
    fn a_pwm_family_length_is_recognised() {
        let mut reply = vec![0u8; 64];
        reply[20] = 0b1010; // capabilities: golden + supports-golden
        reply[25] = 0x0b;
        reply[26] = 0x00;
        reply[27] = 0x00;
        let info = parse_upgrade_info(&reply).unwrap();
        assert_eq!(info.declared_len, 0x000b_0000);
        assert!(info.has_golden());
        assert!(info.supports_golden_upgrade());
        assert!(!info.supports_sdram_staging());
    }

    #[test]
    fn a_normal_family_length_is_recognised() {
        let mut reply = vec![0u8; 64];
        reply[25] = 0x0b;
        reply[26] = 0x00;
        reply[27] = 0x80;
        assert_eq!(
            parse_upgrade_info(&reply).unwrap().declared_len,
            0x000b_0080
        );
    }

    #[test]
    fn implausible_lengths_are_ignored() {
        let mut reply = vec![0u8; 64];
        reply[25] = 0x0b;
        reply[26] = 0x00;
        reply[27] = 0x42; // not a known image length
        assert!(parse_upgrade_info(&reply).is_none());
    }
}
