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
