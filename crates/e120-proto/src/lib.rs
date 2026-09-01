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

/// Rejected before anything reaches the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// A block other than the parameter block was requested.
    ForbiddenBlock(u8),
    /// A page payload was not exactly one page.
    WrongPageSize(usize),
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
    let mut p = [0u8; 126];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_ERASE;
    p[4] = 0x01;
    p[5..7].copy_from_slice(&(u16::from(block) << 8).to_be_bytes());
    Ok(frame([0x06, 0x00], &p))
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
    // Layout mirrors the read frame, which is verified against hardware:
    // [0] zero, [1..3] receiver index, [3] opcode, [4] flag, [5..7] block/page.
    // The vendor builder copies payload data to offset 0x0a of the frame
    // payload, which is index 8 here, leaving index 7 reserved.
    let mut p = vec![0u8; 8 + FLASH_PAGE_BYTES];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = FLASH_OP_WRITE;
    p[5] = block;
    p[6] = page;
    p[8..].copy_from_slice(data);
    Ok(frame([0x06, 0x00], &p))
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
