//! Discovery, layout, test mode, and the upgrade descriptor query.

use super::frame;

/// Discovery request: type 0x0700, 270 zero bytes.
pub fn discovery() -> Vec<u8> {
    frame([0x07, 0x00], &[0u8; 270])
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
/// The vendor's post-save reload: opcode 0x77 with three enable flags.
///
/// LEDVISION sends this after writing flash so the card picks the new
/// parameters up without a power cycle. Opcode 0x77 is in the data-carrying
/// set, so the flag bytes ride at payload offset 0x0a (index 8 here);
/// `01 01 01` means "reload all three parameter classes".
pub fn reload_params_full(rcv_index: u16) -> Vec<u8> {
    let mut p = [0u8; 126];
    p[1..3].copy_from_slice(&rcv_index.to_be_bytes());
    p[3] = 0x77;
    p[8] = 0x01;
    p[9] = 0x01;
    p[10] = 0x01;
    frame([0x06, 0x00], &p)
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
