//! Colorlight receiver-card layer-2 protocol: raw Ethernet frames whose
//! 2-byte "EtherType" is a packet type. Frame builders and reply parsers only;
//! sequencing lives in the callers. Byte layouts: `docs/pixel-protocol.md`.

pub mod discovery;
pub mod eeprom;
pub mod flash;
pub mod params;
pub mod pixel;
pub mod upgrade;

pub use discovery::*;
pub use flash::*;
pub use pixel::*;

pub const CARD_MAC: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
pub const SENDER_MAC: [u8; 6] = [0x22, 0x22, 0x33, 0x44, 0x55, 0x66];

/// Receiver index addressing every card on the link. The vendor uses it even
/// for one card, and a card with a corrupt cabinet record answers only to it.
pub const BROADCAST: u16 = 0xffff;

/// Two MACs and the type.
pub const HEADER_LEN: usize = 14;

pub(crate) fn write_header(f: &mut [u8], ethertype: [u8; 2]) {
    f[..6].copy_from_slice(&CARD_MAC);
    f[6..12].copy_from_slice(&SENDER_MAC);
    f[12..14].copy_from_slice(&ethertype);
}

/// A frame with a zeroed `payload_len`-byte payload that `fill` writes in place.
#[must_use]
pub fn frame_with(ethertype: [u8; 2], payload_len: usize, fill: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut f = vec![0u8; HEADER_LEN + payload_len];
    write_header(&mut f, ethertype);
    fill(&mut f[HEADER_LEN..]);
    f
}

#[must_use]
pub fn frame(ethertype: [u8; 2], payload: &[u8]) -> Vec<u8> {
    frame_with(ethertype, payload.len(), |p| p.copy_from_slice(payload))
}

/// The header shared by the 0x0600/0x1900/0x2300 command payloads: `[1..3]`
/// receiver index BE, `[3]` opcode.
pub(crate) fn indexed(p: &mut [u8], rcv_index: u16, opcode: u8) {
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = opcode;
}

/// A type-0x0600 command with no data; `flags` land at payload offset 8.
pub(crate) fn command(rcv_index: u16, opcode: u8, flags: &[u8]) -> Vec<u8> {
    frame_with([0x06, 0x00], 126, |p| {
        indexed(p, rcv_index, opcode);
        p[8..8 + flags.len()].copy_from_slice(flags);
    })
}

/// One vendor's wire protocol as frame builders and reply parsers, the
/// surface `ops` drives a card through.
///
/// [`Colorlight`] is the only implementation; a second vendor implements
/// this in its own crate (docs/cards.md). Sequencing and sockets stay with
/// the caller.
pub trait Protocol {
    /// The discovery request.
    fn discover(&self) -> Vec<u8>;
    /// The card a discovery reply describes, if `frame` is one.
    fn discovery_reply(&self, frame: &[u8]) -> Option<DiscoveryInfo>;
    /// One row packet into `buf`: screen row `row`, pixels from `x`.
    fn row(&self, buf: &mut Vec<u8>, row: u16, x: u16, rgb: &[[u8; 3]], order: ColorOrder);
    /// The latch frame that applies the rows sent since the last one.
    fn latch(&self, brightness: u8) -> Vec<u8>;
    /// The brightness frame sent before the rows.
    fn brightness(&self, brightness: u8) -> Vec<u8>;
    /// Read one chunk of flash at a 256-byte page index.
    fn flash_read(&self, index: u16, page: u16) -> Vec<u8>;
    /// The flash bytes a reply carries, if `frame` is a flash reply.
    fn flash_reply<'a>(&self, frame: &'a [u8]) -> Option<&'a [u8]>;
    /// Write one page of the parameter block; refused outside `map`.
    ///
    /// # Errors
    /// Refuses a block outside the map or a payload that is not one page.
    fn flash_write(&self, map: &FlashMap, index: u16, block: u8, page: u8, data: &[u8]) -> Result<Vec<u8>, WriteError>;
    /// Write one EEPROM record at its own address and length.
    fn eeprom_write(&self, addr: u16, data: &[u8]) -> Vec<u8>;
}

/// The Colorlight receiving-card protocol: the free functions of this crate
/// behind [`Protocol`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Colorlight;

impl Protocol for Colorlight {
    fn discover(&self) -> Vec<u8> {
        discovery()
    }

    fn discovery_reply(&self, frame: &[u8]) -> Option<DiscoveryInfo> {
        parse_discovery_response(frame)
    }

    fn row(&self, buf: &mut Vec<u8>, row: u16, x: u16, rgb: &[[u8; 3]], order: ColorOrder) {
        pixel_row_into(buf, row, x, rgb, order);
    }

    fn latch(&self, brightness: u8) -> Vec<u8> {
        sync(brightness).to_vec()
    }

    fn brightness(&self, brightness: u8) -> Vec<u8> {
        pixel::brightness(brightness).to_vec()
    }

    fn flash_read(&self, index: u16, page: u16) -> Vec<u8> {
        read_flash(index, page)
    }

    fn flash_reply<'a>(&self, frame: &'a [u8]) -> Option<&'a [u8]> {
        flash_reply_data(frame)
    }

    fn flash_write(&self, map: &FlashMap, index: u16, block: u8, page: u8, data: &[u8]) -> Result<Vec<u8>, WriteError> {
        map.write_page(index, block, page, data)
    }

    fn eeprom_write(&self, addr: u16, data: &[u8]) -> Vec<u8> {
        eeprom::write(addr, data)
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;

    #[test]
    fn the_trait_builds_the_same_frames_as_the_functions() {
        let p = Colorlight;
        assert_eq!(p.discover(), discovery());
        assert_eq!(p.latch(40), sync(40).to_vec());
        assert_eq!(p.brightness(40), pixel::brightness(40).to_vec());
        assert_eq!(p.flash_read(0, FLASH_PAGE_BASIC_PARAM), read_flash(0, FLASH_PAGE_BASIC_PARAM));
        assert_eq!(p.eeprom_write(0x02, &[0; 42]), eeprom::write(0x02, &[0; 42]));
        let page = [0u8; FLASH_PAGE_BYTES];
        assert_eq!(p.flash_write(&E120, 0, PARAM_BLOCK, 0x80, &page), E120.write_page(0, PARAM_BLOCK, 0x80, &page));
        assert_eq!(p.flash_write(&E120, 0, 0x00, 0, &page), Err(WriteError::ForbiddenBlock(0)));
        let mut a = Vec::new();
        p.row(&mut a, 3, 0, &[[1, 2, 3]; 4], ColorOrder::Bgr);
        assert_eq!(a, pixel_row(3, 0, &[[1, 2, 3]; 4], ColorOrder::Bgr));
        assert!(p.discovery_reply(&[0; 20]).is_none());
        assert!(p.flash_reply(&[0; 20]).is_none());
    }
}
