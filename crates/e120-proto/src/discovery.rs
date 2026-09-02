//! Discovery, layout, test mode, parameter reload, and the upgrade-descriptor
//! query.
//!
//! The descriptor query is a discovery frame (type 0x0700) with `ff ff ff` at
//! payload 1..4; the card answers with type 0x08 instead of 0x0805.

use super::{command, frame, frame_with, indexed};

/// Discovery request: type 0x0700, 270 zero bytes.
#[must_use]
pub fn discovery() -> Vec<u8> {
    frame([0x07, 0x00], &[0u8; 270])
}
/// Receiver layout (FPP's type 0x02 packet): this card's size and offset,
/// and the whole screen's size. Offsets are into the payload after the type.
#[must_use]
pub fn set_layout(
    rcv_index: u16,
    recv_w: u16,
    recv_h: u16,
    x_offset: u16,
    y_offset: u16,
    total_w: u16,
    total_h: u16,
) -> Vec<u8> {
    frame_with([0x02, 0x00], 98, |p| {
        p[0..2].copy_from_slice(&rcv_index.to_be_bytes());
        p[6..8].copy_from_slice(&recv_w.to_be_bytes());
        p[8..10].copy_from_slice(&recv_h.to_be_bytes());
        p[12..14].copy_from_slice(&x_offset.to_be_bytes());
        p[14..16].copy_from_slice(&y_offset.to_be_bytes());
        p[16..18].copy_from_slice(&total_w.to_be_bytes());
        p[18..20].copy_from_slice(&total_h.to_be_bytes());
    })
}
/// Card-generated test pattern; not persisted.
#[must_use]
pub fn test_mode(rcv_index: u16, pattern: u8) -> Vec<u8> {
    frame_with([0x33, 0x00], 0x109, |p| {
        indexed(p, rcv_index, 0x09);
        p[4] = pattern;
    })
}
/// Reload parameters from flash, opcode 0x79.
#[must_use]
pub fn reload_params(rcv_index: u16) -> Vec<u8> {
    command(rcv_index, 0x79, &[])
}
/// The vendor's post-save reload: opcode 0x77, flags `01 01 01`. Which of the
/// two reloads the card needs after a flash save has not been measured.
#[must_use]
pub fn reload_params_full(rcv_index: u16) -> Vec<u8> {
    command(rcv_index, 0x77, &[0x01, 0x01, 0x01])
}
/// Upgrade-descriptor query; decode the reply with `upgrade::parse_descriptor`.
#[must_use]
pub fn upgrade_info() -> Vec<u8> {
    frame_with([0x07, 0x00], 270, |p| {
        p[1..4].copy_from_slice(&[0xff, 0xff, 0xff]);
        p[4] = 0x01;
        p[6..10].copy_from_slice(&[0x43, 0x57, 0x83, 0x97]);
        p[10] = 0x09;
    })
}
/// Type 0x0805 discovery reply; field offsets follow the community 5A-75B
/// decode (`e120 discover` reads firmware 16.53 from `ver_major.ver_minor`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryInfo {
    pub card_id: u8,
    pub ver_major: u8,
    pub ver_minor: u8,
    pub cols: u16,
    pub rows: u16,
    pub controller: u8,
    pub raw: Vec<u8>,
}
#[must_use]
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
        controller: p[62],
        raw: p.to_vec(),
    })
}
#[cfg(test)]
mod tests {
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
        assert!(upgrade_info()[25..].iter().all(|&b| b == 0));
    }

    #[test]
    fn reload_frames_keep_their_bytes() {
        let f = reload_params(0);
        assert_eq!(f.len(), 140);
        assert_eq!(&f[12..14], &[0x06, 0x00]);
        assert_eq!(f[17], 0x79);
        assert!(f[18..].iter().all(|&b| b == 0));
        let f = reload_params_full(1);
        assert_eq!(&f[15..18], &[0x00, 0x01, 0x77]);
        assert_eq!(&f[22..25], &[0x01, 0x01, 0x01]);
        assert!(f[25..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_mode_frame_matches_the_vendor_layout() {
        let f = test_mode(2, 0x05);
        assert_eq!(f.len(), 14 + 0x109);
        assert_eq!(&f[12..14], &[0x33, 0x00]);
        assert_eq!(&f[14..19], &[0x00, 0x00, 0x02, 0x09, 0x05]);
        assert!(f[19..].iter().all(|&b| b == 0));
    }
}
