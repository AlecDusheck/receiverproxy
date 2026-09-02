//! Sub-indices of the type-0x05 real-time parameter packs.
//!
//! Each pack is `[0x05, 0x00, 0x00, sub]` + a 256-byte body, the same blocks
//! the boot image carries (`docs/compiled-image-format.md`). Framing is done
//! by the CLI's `Pack` (`cli/src/params.rs`).

/// Basic-parameter pack (`GetBasicParam`).
pub const SUB_BASIC: u8 = 0x00;
/// Chip-register pack (record 0x84 verbatim).
pub const SUB_CHIP: u8 = 0x01;
/// Data-swap pack (`GetDataSwapEx2ParamPack`).
pub const SUB_DATA_SWAP: u8 = 0x02;
