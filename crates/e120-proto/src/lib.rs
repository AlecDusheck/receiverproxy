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

/// Receiver index addressing every card on the link; the vendor uses it even
/// for a single card, and it is the only index a card with a corrupt cabinet
/// record still answers to.
pub const BROADCAST: u16 = 0xffff;

pub fn frame(ethertype: [u8; 2], payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(14 + payload.len());
    f.extend_from_slice(&CARD_MAC);
    f.extend_from_slice(&SENDER_MAC);
    f.extend_from_slice(&ethertype);
    f.extend_from_slice(payload);
    f
}
