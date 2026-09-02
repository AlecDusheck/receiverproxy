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

/// Bytes of Ethernet header (two MACs and the type) in front of every payload.
pub const HEADER_LEN: usize = 14;

/// Write the two MACs and the type into the first [`HEADER_LEN`] bytes of `f`.
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

/// A type-0x0600 command frame with no data: `[1..3]` receiver index, `[3]`
/// opcode, flag bytes (if any) at payload offset 0x0a.
pub(crate) fn command(rcv_index: u16, opcode: u8, flags: &[u8]) -> Vec<u8> {
    frame_with([0x06, 0x00], 126, |p| {
        indexed(p, rcv_index, opcode);
        p[8..8 + flags.len()].copy_from_slice(flags);
    })
}
