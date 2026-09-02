//! Record 0x03, the pixel mapping: for every module pixel in raster order
//! (over the stored height), the scan line it belongs to and its slot in
//! that line's buffer.
//!
//! The count is width x stored height (`SaveBpToBuffer` @ 0x1cc404); the
//! entry formula is corpus-derived (docs/panel-wiring.md).

use super::PanelSpec;

#[must_use]
pub fn record(spec: &PanelSpec) -> Vec<u8> {
    let w = spec.module.width;
    let h = spec.module.height / 2;
    let scan = u16::from(spec.module.scan);
    let groups = h / scan;
    let n = w * h;
    // The chain is walked in blocks of `blk` columns; each block holds one
    // run per data group, so a group's columns are contiguous only within a
    // block. With `blk == w` this collapses to `group * w + col`.
    let blk = spec.mapping.block.unwrap_or(w).clamp(1, w);
    let mut out = Vec::with_capacity(2 + usize::from(n) * 3);
    out.extend_from_slice(&n.to_le_bytes());
    for i in 0..n {
        let (row, col) = (i / w, i % w);
        let line = if spec.mapping.reversed_lines {
            scan - 1 - row % scan
        } else {
            row % scan
        };
        let group = if spec.mapping.reversed_groups {
            groups - 1 - row / scan
        } else {
            row / scan
        };
        let slot = (col / blk) * (groups * blk) + group * blk + col % blk;
        out.push(line as u8);
        out.extend_from_slice(&slot.to_le_bytes());
    }
    out
}
