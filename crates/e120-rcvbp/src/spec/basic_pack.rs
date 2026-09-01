//! The basic-parameter pack body (page 0 of the boot image; the real-time
//! pack with sub-index 0), built the way `GetBasicParam` @ 0x1dfb50 does
//! from record 0x01 — every derived field below reproduces the factory
//! bytes under test. Bytes not yet derived are carried from the reference
//! pack and reported.

use super::PanelSpec;
use crate::record01::{off, View};
use anyhow::{bail, Result};

// Body offsets (pack offset - 4).
const HEAD3: usize = 0x01;
const MODULE_DIMS: usize = 0x04;
const MODULES_IN_LINE: usize = 0x06;
const SCAN: usize = 0x07;
const GRAY: usize = 0x08;
const SERIAL_CLOCK: usize = 0x09;
const ONE_SCAN_LEN: usize = 0x0B;
const CARD_SCAN_LEN: usize = 0x0D;
const COLOR: usize = 0x10;
const LUMINANCE: usize = 0x15;
const LUMINANCE_LEVEL: usize = 0x17;
const GAINS: usize = 0x30;
const CHIP_CUSTOM: usize = 0x70;
const MAX_W: usize = 0x88;
const MAX_H: usize = 0x8A;
const CHIP_ID: usize = 0xE7;

/// # Errors
/// Rejects a reference pack that is not one page.
pub fn body(
    spec: &PanelSpec,
    rec: &View,
    reference: &[u8],
    prov: &mut Vec<String>,
) -> Result<[u8; 256]> {
    if reference.len() != 256 {
        bail!("reference basic pack is {} bytes, need 256", reference.len());
    }
    let mut b = [0u8; 256];
    b.copy_from_slice(reference);
    prov.push("basicpack: bytes not listed below <- reference pack".into());
    let mut put = |at: usize, bytes: &[u8], what: &str| {
        b[at..at + bytes.len()].copy_from_slice(bytes);
        prov.push(format!("basicpack +{at:#04x} <- {what}"));
    };

    put(HEAD3, &rec.bytes()[off::PACK_HEAD3..off::PACK_HEAD3 + 3], "record01 +0x028 (3 bytes)");
    let (w, h2) = (spec.module.width as u8, (spec.module.height / 2) as u8);
    if spec.module.line_dir >= 2 {
        put(MODULE_DIMS, &[w, h2], "module width, height/2 (line_dir horizontal)");
    } else {
        put(MODULE_DIMS, &[h2, w], "module height/2, width (line_dir vertical)");
    }
    put(
        MODULES_IN_LINE,
        &[spec.modules_in_line_dir() as u8],
        "modules in line dir = screen extent / module extent",
    );
    put(SCAN, &[spec.module.scan], "module.scan");
    put(GRAY, &[spec.module.gray_bits], "module.gray_bits");
    put(SERIAL_CLOCK, &spec.module.serial_clock.to_be_bytes(), "module.serial_clock (BE)");
    put(ONE_SCAN_LEN, &spec.one_scan_len().to_be_bytes(), "OneScanLen = W x H/2 / scan (BE)");
    put(
        CARD_SCAN_LEN,
        &spec.card_scan_len().to_be_bytes(),
        "CardScanLen = OneScanLen x modules in line dir (BE)",
    );
    let [s0, s1, s2] = spec.color.source;
    put(
        COLOR,
        &[(spec.color.swap << 6) | (s2 << 4) | (s1 << 2) | s0],
        "color.swap<<6 | source[2]<<4 | source[1]<<2 | source[0]",
    );
    put(LUMINANCE, &[rec.u8(off::LUMINANCE)], "record01 +0x024 low byte");
    put(LUMINANCE_LEVEL, &[rec.u8(off::LUMINANCE_LEVEL)], "record01 +0x026 low byte");
    put(GAINS, &spec.current.gains, "current.gains");
    put(CHIP_CUSTOM, rec.chip_custom(), "record01 +0x06A chip-custom block");
    put(MAX_W, &spec.screen.width.to_be_bytes(), "screen.width (BE)");
    put(MAX_H, &spec.screen.height.to_be_bytes(), "screen.height (BE)");
    put(CHIP_ID, &spec.chip.id.to_be_bytes(), "chip.id (BE)");
    Ok(b)
}
