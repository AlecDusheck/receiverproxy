//! Record 0x01 built from first principles: the vendor's write-side defaults
//! (`CHWParamRcvGeneral::Reset`/`ResetIS`/`ResetSwapData`, serialised by
//! `SaveBpToBuffer`), then the spec's fields, then the chip-derived blocks.
//! Every byte is accounted for in `docs/record-0x01-fields.md`.

use super::PanelSpec;
use crate::chips::ChipLibrary;
use crate::record01::{off, LEN};
use anyhow::Result;

/// Non-zero bytes of a freshly constructed config before any user setting:
/// (offset, bytes). Ramps and floats are spelled out where the constructor
/// stores them.
const DEFAULTS: &[(usize, &[u8])] = &[
    (0x000, &[0x20, 0x20, 0x01]),                 // module 32x32(stored), OBJ+0x6c = 1
    (0x008, &[0x02]),
    (0x024, &[0x01]),                             // luminance scalar
    (0x028, &[0xff, 0xff, 0xff]),                 // pack head triple
    (0x02B, &[0x03, 0x02, 0x01, 0x00]),           // colour swap 3, source (2,1,0)
    (0x030, &[0x32, 0x14]),                       // OBJ+0xD3C1, GCLK
    (0x032, &[0x2b, 0x2b, 0x2b, 0x2b]),           // current gains
    (0x03D, &[0xF2]),                             // scan method 2 | OBJ+0xC2 (0x0F) << 4
    (0x053, &[0x00, 0x00, 0x70, 0x42]),           // f32 60.0
    (0x057, &[0x10, 0x10]),                       // module-position grid unit 16x16
    (0x0AA, &[0x00, 0x00, 0x70, 0x42]),           // f32 60.0 base refresh
    (0x0D8, &[0x00, 0x00, 0x80, 0x3f]),           // f32 1.0
    (0x0E5, &[0x0F]),                             // PWM segments - 1
    (0x0F3, &[0x64]),
    (0x0F6, &[0x80]),
    (0x0FD, &[0x80, 0x80, 0x80, 0x80]),
    (0x102, &[0xff, 0xff, 0, 0, 0, 0, 0xff, 0xff, 0, 0, 0, 0, 0xff, 0xff, 0, 0]),
    (0x178, &[0x0F]),
    (0x17B, &[0xe8, 0x03, 0x00, 0x00]),           // 1000
    (0x182, &[0x19, 0x00, 0x7d, 0x00, 0x7d, 0x00, 0x3e, 0x00]), // close-time quad
    (0x18C, &[0xe8, 0x03, 0x00, 0x00]),           // 1000
    (0x194, &[0xe8, 0x03, 0x00, 0x00]),           // 1000
    (0x1DB, &[0x08, 0x20]),                       // flag word 2: gamma-calc method, always-set bit 13
    (0x1FC, &[0xe8, 0x03, 0x00, 0x00]),           // 1000
    (0x246, &[0x14]),
    (0x257, &[0x13, 0x27, 0x3a, 0x00, 0x00, 0x88, 0x10, 0x98]), // 5000 / 10000 / 15000 split hi/lo
    (0x26C, &[0x01, 0x01]),
    (0x273, &[0x00, 0x00, 0x80, 0x3f]),           // f32 1.0
    (0x277, &[0x01, 0x01]),
    (0x27E, &[0x01, 0x00, 0x01, 0x00]),
    (0x282, &[0x01]),
];

/// Bytes our working config carries that the vendor code sets from state not
/// yet named (provenance known, meaning NOT RESOLVED). Carried as literals so
/// the generator reproduces a config the card is known to accept.
const LITERALS: &[(usize, &[u8])] = &[
    (0x043, &[0x60]),                             // OBJ+0xB8
    (0x04F, &[0x01]),                             // OBJ+0xBC
    (0x0FC, &[0x01]),                             // OBJ+0xDF8D
    (0x0E7, &[0x20, 0x3e]),                       // vt+0x278 / OBJ+0xDF16
    (0x0EA, &[0x00, 0x40, 0x00, 0x40, 0x00, 0x40]), // vt+0x178: three u16 0x4000
    (0x17F, &[0x0F]),                             // packed flags (OBJ+0xE08F.. )
    (0x1E1, &[0xff, 0x1f, 0x00, 0x00, 0xfc, 0x7f, 0x00, 0x00, 0xc0, 0xff, 0x07, 0x00]), // masked pairs
    (0x1F8, &[0xcd, 0xcc, 0xcc, 0x3d]),           // f32 0.1 (vt+0x630)
    (0x269, &[0x06]),                             // OBJ+0xE6EC = 3
];

/// Flag word 1 (+0x018): the geometry-source bit (OBJ+0xD6EA) is set — module
/// width/height come from record 0xCA, which we emit.
const FLAG1_GEOMETRY_FROM_RECORD_CA: u32 = 1 << 20;

/// # Errors
/// Fails if the chip library lacks the registers gray depth derives from.
pub fn build(spec: &PanelSpec, chip: &ChipLibrary, prov: &mut Vec<String>) -> Result<[u8; LEN]> {
    let mut p = [0u8; LEN];
    for &(at, bytes) in DEFAULTS {
        p[at..at + bytes.len()].copy_from_slice(bytes);
    }
    // Swap tables: one identity ramp 0x00..0x3F laid across two regions
    // (the chip-custom block sits between them), the 96-entry identity map,
    // and the 0x40..0x7F lane map.
    for (i, b) in p[0x05A..0x06A].iter_mut().enumerate() {
        *b = i as u8;
    }
    for (i, b) in p[0x07A..0x0AA].iter_mut().enumerate() {
        *b = 0x10 + i as u8;
    }
    for (i, b) in p[0x114..0x174].iter_mut().enumerate() {
        *b = i as u8;
    }
    for (i, b) in p[0x19A..0x1DA].iter_mut().enumerate() {
        *b = 0x40 + i as u8;
    }
    for &(at, bytes) in LITERALS {
        p[at..at + bytes.len()].copy_from_slice(bytes);
    }
    prov.push("record01: vendor write-side defaults + documented literals".into());

    let mut put = |at: usize, bytes: &[u8], what: &str| {
        p[at..at + bytes.len()].copy_from_slice(bytes);
        prov.push(format!("record01 +{at:#05x} <- {what}"));
    };
    let m = &spec.module;
    let sck = spec.serial_clock(chip);
    put(off::MODULE_W, &[m.width as u8], "module.width");
    put(off::MODULE_H_HALF, &[(m.height / 2) as u8], "module.height / 2");
    put(0x018, &FLAG1_GEOMETRY_FROM_RECORD_CA.to_le_bytes(), "flag1: geometry from record 0xCA");
    put(off::GAMMA, &spec.timing.gamma.to_le_bytes(), "timing.gamma (f32)");
    put(off::SCAN, &[m.scan], "module.scan");
    put(off::SERIAL_CLOCK, &sck.to_le_bytes(), "serial clock");
    let gray = spec.gray_bits(chip)?;
    put(off::GRAY, &[gray], "gray bits (from chip registers 0x07/0x03)");
    put(off::LUMINANCE_LEVEL, &spec.timing.luminance_level.to_le_bytes(), "timing.luminance_level");
    put(off::COLOR_SWAP, &[spec.color.swap], "color.swap");
    put(off::COLOR_SOURCE, &spec.color.source, "color.source");
    put(off::GCLOCK, &[spec.timing.gclock], "timing.gclock");
    put(off::GAINS, &spec.current.gains, "current.gains");
    let family = chip.family_id;
    let sub = chip.sub_id.unwrap_or(0);
    put(off::CHIP_LO, &[(family & 0xFF) as u8], "chip family id low byte");
    put(off::CHIP_HI, &[(family >> 8) as u8], "chip family id high byte");
    put(off::SUB_CHIP_LO, &[(sub & 0xFF) as u8], "chip sub-id low byte");
    put(off::SUB_CHIP_HI, &[(sub >> 8) as u8], "chip sub-id high byte");
    put(off::LINE_DIR, &[m.line_dir], "module.line_dir");
    put(0x044, &[spec.module.data_groups], "module.data_groups");
    put(off::SERIAL_CLOCK_HALF, &(sck / 2).to_le_bytes(), "serial clock / 2");
    put(off::SERIAL_CLOCK_DUP, &sck.to_le_bytes(), "serial clock (duplicate)");
    put(0x050, &[u8::from(spec.timing.oe_8ns)], "timing.oe_8ns");
    // Chip-custom block: PWM flag | serial clock, as the chip reset leaves it
    // (the vendor does not refresh it when the clock is edited later).
    let reset_sck = chip.serial_clock;
    put(
        off::CHIP_CUSTOM,
        &[0x80 | ((reset_sck >> 8) & 0x7F) as u8, (reset_sck & 0xFF) as u8],
        "chip-custom: PWM flag | chip reset serial clock",
    );
    put(off::REFRESH, &spec.timing.refresh_hz.to_le_bytes(), "timing.refresh_hz (f32)");
    put(off::MIN_OE, &spec.timing.min_oe.to_le_bytes(), "timing.min_oe (f32)");
    for (i, pct) in spec.current.percent.iter().enumerate() {
        put(off::CURRENT_PCT + 4 * i, &pct.to_le_bytes(), "current.percent (f32)");
    }
    put(off::MAX_W, &spec.screen.width.to_le_bytes(), "screen.width (MaxWidth)");
    put(off::MAX_H, &spec.screen.height.to_le_bytes(), "screen.height (MaxHeight)");
    if let Some(block) = chip.chip_custom_block(m.scan) {
        put(off::CHIP_CUSTOM, &block, "chip library chip_custom (SChipCustom, scan-patched)");
    }
    if let Some(ex) = &chip.chip_custom_ex {
        put(0x0E0, ex, "chip library chip_custom_ex (SChipCustomEX)");
    }
    put(0x0C4, &chip.chip_control, "chip library chip_control (SChipControl)");
    for (&at, &value) in chip.record01_overrides.iter().chain(&spec.record01_overrides) {
        put(at, &[value], "record01_overrides (chip library, then spec)");
    }
    Ok(p)
}
