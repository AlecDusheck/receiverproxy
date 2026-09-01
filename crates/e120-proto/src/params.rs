//! Real-time parameter packs (type 0x05).
//!
//! What the vendor tool pushes into the card's RAM at the start of every
//! session, in the order chip registers → data swap → basic parameters. The
//! 256-byte bodies are the same blocks the boot image carries
//! (`docs/compiled-image-format.md`); on the wire each gets a 4-byte header
//! `[0x05, 0x00, 0x00, sub-index]`.

/// Bytes in a parameter pack, including the two leading type bytes.
pub const PACK_LEN: usize = 0x104;
/// Body bytes in a pack.
pub const BODY_LEN: usize = PACK_LEN - 4;

/// Sub-index of the basic-parameter pack (`GetBasicParam`).
pub const SUB_BASIC: u8 = 0x00;
/// Sub-index of the chip-register pack (record 0x84 verbatim).
pub const SUB_CHIP: u8 = 0x01;
/// Sub-index of the data-swap pack (`GetDataSwapEx2ParamPack`).
pub const SUB_DATA_SWAP: u8 = 0x02;

/// Wrap a fully built pack into an Ethernet frame.
///
/// The pack's first two bytes are the type, matching every other frame in this
/// protocol.
#[must_use]
pub fn frame_for(pack: &[u8; PACK_LEN]) -> Vec<u8> {
    super::frame([pack[0], pack[1]], &pack[2..])
}

/// A pack carrying `body` (at most 256 bytes, zero-padded) under `sub_index`.
#[must_use]
pub fn pack(sub_index: u8, body: &[u8]) -> [u8; PACK_LEN] {
    let mut p = [0u8; PACK_LEN];
    p[0] = 0x05;
    p[3] = sub_index;
    let n = body.len().min(BODY_LEN);
    p[4..4 + n].copy_from_slice(&body[..n]);
    p
}

/// The chip-register pack: record 0x84's payload, verbatim.
#[must_use]
pub fn chip_pack(record_84: &[u8]) -> [u8; PACK_LEN] {
    pack(SUB_CHIP, record_84)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_frame_as_type_05_with_the_body_at_offset_four() {
        let body: Vec<u8> = (0..=255u8).collect();
        let p = pack(SUB_BASIC, &body);
        assert_eq!(&p[..4], &[0x05, 0x00, 0x00, 0x00]);
        assert_eq!(&p[4..], &body[..]);
        let f = frame_for(&p);
        assert_eq!(f.len(), 272, "260-byte pack becomes a 272-byte frame");
        assert_eq!(&f[12..16], &[0x05, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn the_chip_pack_is_distinguished_by_its_sub_index() {
        let c = chip_pack(&[0xAB; 256]);
        assert_eq!(c[3], SUB_CHIP);
        assert!(c[4..].iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn short_bodies_are_zero_padded() {
        let p = pack(SUB_DATA_SWAP, &[1, 2, 3]);
        assert_eq!(&p[4..7], &[1, 2, 3]);
        assert!(p[7..].iter().all(|&b| b == 0));
    }
}
