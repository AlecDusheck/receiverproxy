//! Record 0x03, the pixel mapping: per module pixel in raster order over the
//! stored height, its scan line and slot in that line's buffer. Count is
//! width x stored height (`SaveBpToBuffer` @ 0x1cc404); the entry formula is
//! corpus-derived (docs/panel-wiring.md).

use panelspec::PanelSpec;

#[must_use]
pub fn record(spec: &PanelSpec) -> Vec<u8> {
    let w = spec.module.width;
    let h = spec.module.height / 2;
    let scan = u16::from(spec.module.scan);
    let groups = h / scan;
    let n = w * h;
    // Each block of `blk` columns holds one run per data group; with
    // `blk == w` the slot collapses to `group * w + col`.
    let blk = spec.mapping.block.unwrap_or(w).clamp(1, w);
    let mut out = Vec::with_capacity(2 + usize::from(n) * 3);
    out.extend_from_slice(&n.to_le_bytes());
    for row in 0..h {
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
        for col in 0..w {
            let slot = (col / blk) * (groups * blk) + group * blk + col % blk;
            out.push(line as u8);
            out.extend_from_slice(&slot.to_le_bytes());
        }
    }
    out
}
