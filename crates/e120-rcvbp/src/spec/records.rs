//! The other records of a `.rcvbp`, from their decoded defaults (the loader
//! pre-fills each record's buffer with these and applies them whether or not
//! the record is present, so a config with exactly these bytes behaves like
//! one that omits them). Order and sizes follow the vendor's files.

use super::PanelSpec;
use crate::{Rcvbp, Record};

/// Multi-calibration ratio defaults: 0.75 (high) and 0.35 (low), u16/65535.
const CALI_HIGH: u16 = 0xBFFF;
const CALI_LOW: u16 = 0x5999;

fn secondary_params(spec: &PanelSpec) -> Vec<u8> {
    let mut r = vec![0u8; 256];
    r[0x00..0x04].copy_from_slice(&1u32.to_le_bytes()); // OBJ+0xE108 flag word
    r[0x04..0x06].copy_from_slice(&50u16.to_le_bytes());
    r[0x06] = 31;
    r[0x07] = 2; // seam switch, bit 1
    r[0x10..0x12].copy_from_slice(&spec.screen.height.to_le_bytes()); // MaxHeight + void rows
    r[0x12..0x14].copy_from_slice(&spec.screen.width.to_le_bytes()); // MaxWidth + void cols
    r[0x17] = 7; // gamma bits extend
    for at in [0x1D, 0x1F, 0x21] {
        r[at..at + 2].copy_from_slice(&CALI_HIGH.to_le_bytes());
    }
    for at in [0x23, 0x25, 0x27] {
        r[at..at + 2].copy_from_slice(&CALI_LOW.to_le_bytes());
    }
    for (at, v) in [(0x3B, 89u16), (0x3D, 127), (0x3F, 191), (0x41, 229)] {
        r[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    r[0x4E..0x52].copy_from_slice(&30.1f32.to_le_bytes()); // thermal low
    r[0x52..0x56].copy_from_slice(&70.0f32.to_le_bytes()); // thermal high
    r
}

fn rgb_trio(value: u8) -> Vec<u8> {
    let mut r = vec![0u8; 10];
    r[6..9].fill(value);
    r
}

fn geometry_wide(spec: &PanelSpec) -> Vec<u8> {
    let mut r = vec![0u8; 256];
    r[0..2].copy_from_slice(&spec.module.width.to_le_bytes());
    r[2..4].copy_from_slice(&(spec.module.height / 2).to_le_bytes());
    r[4..6].copy_from_slice(&1u16.to_le_bytes()); // OBJ+0x6c
    r
}

fn table_8e() -> Vec<u8> {
    let mut r = vec![0u8; 2591];
    r[0xA1C..0xA1F].fill(0x14); // per-colour triple, clamped to 20 when zero
    r
}

/// Assemble the config in the vendor's record order.
#[must_use]
pub fn assemble(spec: &PanelSpec, rec01: Vec<u8>, mapping: Vec<u8>, chip_regs: Vec<u8>) -> Rcvbp {
    let rec = |t: u16, payload: Vec<u8>| Record::new(t, payload);
    let records = vec![
        rec(0x0a01, rec01),
        rec(0x0a8d, vec![0; 4096]),   // void row/column table
        rec(0x0a91, vec![0; 6144]),   // gamma-cali gray
        rec(0x0a95, vec![0; 6144]),   // gamma-cali delta
        rec(0x0ad8, vec![0; 18433]),  // write-only filler the loader never dispatches
        rec(0x0ada, vec![0; 36865]),  // gamma-cali new-delta
        rec(0x0a8e, table_8e()),
        rec(0x0a03, mapping),
        rec(0x0a07, vec![0; 32]),     // 36-byte blob incl. header; contents unresolved, zero
        rec(0x0a83, rgb_trio(0xFF)),
        rec(0x0a89, rgb_trio(0x80)),
        rec(0x0a86, vec![0; 5]),      // switch-status bitfield
        rec(0x0a8a, secondary_params(spec)),
        rec(0x0a84, chip_regs),
        rec(0x0acd, vec![0; 270]),    // cabinet identity strings
        rec(0x008f, vec![0; 580]),    // 8-bit gamma control points
        rec(0x0aca, geometry_wide(spec)),
    ];
    Rcvbp {
        version: 4,
        blob: Vec::new(),
        records,
    }
}
