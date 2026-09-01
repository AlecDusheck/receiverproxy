//! Record 0x01 — the main receiver-parameter record: named byte offsets and
//! typed accessors. The field dictionary is `docs/record-0x01-fields.md`.

use anyhow::{bail, Result};

/// Payload length of record 0x01.
pub const LEN: usize = 764;

/// Byte offsets into the record payload.
pub mod off {
    pub const MODULE_W: usize = 0x000;
    pub const MODULE_H_HALF: usize = 0x001;
    pub const GAMMA: usize = 0x01C;
    pub const SCAN: usize = 0x020;
    pub const SERIAL_CLOCK: usize = 0x021;
    pub const GRAY: usize = 0x023;
    pub const LUMINANCE: usize = 0x024;
    pub const LUMINANCE_LEVEL: usize = 0x026;
    /// Three bytes copied into the basic pack head (`ff ff ff` in practice).
    pub const PACK_HEAD3: usize = 0x028;
    pub const COLOR_SWAP: usize = 0x02B;
    pub const COLOR_SOURCE: usize = 0x02C;
    pub const GCLOCK: usize = 0x031;
    pub const GAINS: usize = 0x032;
    pub const CHIP_LO: usize = 0x036;
    pub const LINE_DIR: usize = 0x03C;
    pub const SPLIT: usize = 0x03E;
    pub const SERIAL_CLOCK_HALF: usize = 0x049;
    pub const SERIAL_CLOCK_DUP: usize = 0x04B;
    pub const GRID_W_LO: usize = 0x057;
    pub const GRID_H_LO: usize = 0x058;
    pub const HR_STYLE: usize = 0x059;
    pub const CHIP_CUSTOM: usize = 0x06A;
    pub const REFRESH: usize = 0x0AA;
    pub const MIN_OE: usize = 0x0AE;
    pub const HR_SCAN_STYLE: usize = 0x0B2;
    pub const CURRENT_PCT: usize = 0x0B4;
    pub const MAX_W: usize = 0x0C0;
    pub const MAX_H: usize = 0x0C2;
    pub const SEGMENTS_MINUS_1: usize = 0x0E5;
    pub const SWAP_RAMP: usize = 0x19A;
    pub const CHIP_HI: usize = 0x204;
    pub const GRID_W_HI: usize = 0x24E;
    pub const GRID_H_HI: usize = 0x24F;
}

/// Read-only typed view over a record 0x01 payload.
#[derive(Clone, Copy)]
pub struct View<'a>(&'a [u8]);

impl<'a> View<'a> {
    /// # Errors
    /// Rejects a payload shorter than the record.
    pub fn new(payload: &'a [u8]) -> Result<Self> {
        if payload.len() < LEN {
            bail!("record 0x01 payload is {} bytes, need {LEN}", payload.len());
        }
        Ok(Self(payload))
    }

    #[must_use]
    pub fn bytes(&self) -> &'a [u8] {
        self.0
    }
    #[must_use]
    pub fn u8(&self, at: usize) -> u8 {
        self.0[at]
    }
    #[must_use]
    pub fn u16_le(&self, at: usize) -> u16 {
        u16::from_le_bytes([self.0[at], self.0[at + 1]])
    }
    #[must_use]
    pub fn f32_le(&self, at: usize) -> f32 {
        f32::from_le_bytes([self.0[at], self.0[at + 1], self.0[at + 2], self.0[at + 3]])
    }

    #[must_use]
    pub fn module_width(&self) -> u16 {
        u16::from(self.u8(off::MODULE_W))
    }
    /// The record stores half the module height.
    #[must_use]
    pub fn module_height_stored(&self) -> u16 {
        u16::from(self.u8(off::MODULE_H_HALF))
    }
    #[must_use]
    pub fn scan(&self) -> u8 {
        self.u8(off::SCAN)
    }
    #[must_use]
    pub fn serial_clock(&self) -> u16 {
        self.u16_le(off::SERIAL_CLOCK)
    }
    #[must_use]
    pub fn gray(&self) -> u8 {
        self.u8(off::GRAY)
    }
    #[must_use]
    pub fn luminance_level(&self) -> u16 {
        self.u16_le(off::LUMINANCE_LEVEL)
    }
    #[must_use]
    pub fn max_width(&self) -> u16 {
        self.u16_le(off::MAX_W)
    }
    #[must_use]
    pub fn max_height(&self) -> u16 {
        self.u16_le(off::MAX_H)
    }
    /// The module-position grid unit (16x16 in practice), split across two
    /// byte pairs in the record.
    #[must_use]
    pub fn grid(&self) -> (u16, u16) {
        (
            u16::from_le_bytes([self.u8(off::GRID_W_LO), self.u8(off::GRID_W_HI)]),
            u16::from_le_bytes([self.u8(off::GRID_H_LO), self.u8(off::GRID_H_HI)]),
        )
    }
    /// Data-line direction: 0/1 vertical, 2/3 horizontal.
    #[must_use]
    pub fn line_dir(&self) -> u8 {
        self.u8(off::LINE_DIR)
    }
    /// Vendor `GetSplitSegment` from the +0x03E code.
    #[must_use]
    pub fn split_segment(&self) -> u8 {
        let c = self.u8(off::SPLIT);
        if c & 4 != 0 {
            4
        } else if c & 1 == 0 {
            1
        } else if c < 8 {
            2
        } else {
            c >> 3
        }
    }
    /// PWM segment count for the scan-table solver (+0x0E5 + 1).
    #[must_use]
    pub fn segments(&self) -> u32 {
        u32::from(self.u8(off::SEGMENTS_MINUS_1)) + 1
    }
    #[must_use]
    pub fn min_oe(&self) -> f32 {
        self.f32_le(off::MIN_OE)
    }
    /// High-refresh style with the vendor's runtime bit masked off.
    #[must_use]
    pub fn hr_style(&self) -> u8 {
        self.u8(off::HR_STYLE) & !0x20
    }
    #[must_use]
    pub fn hr_scan_style(&self) -> u8 {
        self.u8(off::HR_SCAN_STYLE)
    }
    #[must_use]
    pub fn swap_ramp(&self) -> &'a [u8] {
        &self.0[off::SWAP_RAMP..off::SWAP_RAMP + 64]
    }
    #[must_use]
    pub fn chip_custom(&self) -> &'a [u8] {
        &self.0[off::CHIP_CUSTOM..off::CHIP_CUSTOM + 16]
    }
    #[must_use]
    pub fn chip_id(&self) -> u16 {
        u16::from(self.u8(off::CHIP_HI)) << 8 | u16::from(self.u8(off::CHIP_LO))
    }
}
