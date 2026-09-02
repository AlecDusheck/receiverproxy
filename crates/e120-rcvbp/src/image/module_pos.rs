//! The module-position table (`GetDefaultModulePos` @ 0x1558b0).
//!
//! The screen tiled by the record's grid unit, one 10-byte entry per tile:
//! `[outer idx, inner idx, x BE, y BE, w BE, h BE]`, built by one of four
//! direction variants.
//!
//! The vendor leaves the table all-zero when any of its gates fails, notably
//! more than 64 tiles: the seller's 256x384 wall has 384, which is why the
//! factory image carries zeros here.

use crate::record01::View;
use anyhow::{bail, Result};

pub const LEN: usize = 0x300;
const COUNT_AT: usize = 0x05;
const ENTRIES_AT: usize = 0x16;

/// # Errors
/// Fails for split layouts the builder does not implement.
pub fn region(rec: View<'_>) -> Result<([u8; LEN], String)> {
    let mut out = [0u8; LEN];
    let (mw, mh) = rec.grid();
    let (w, h) = (rec.max_width(), rec.max_height());
    let dir = rec.line_dir();
    let gated = mw == 0
        || mh == 0
        || !w.is_multiple_of(mw)
        || !h.is_multiple_of(mh)
        || (w / mw) * (h / mh) > 64
        || dir > 3;
    if gated {
        return Ok((
            out,
            format!("0x600: module positions all-zero (vendor gate: {w}x{h} screen / {mw}x{mh} grid)"),
        ));
    }
    let k = rec.split_segment();
    if k != 1 {
        bail!("module positions for split-segment layout {k} are not implemented");
    }

    // The four direction builders share x/y/w/h and differ only in the two
    // index bytes, the row/column limits, and whether dropped tiles leave
    // holes (positional) or are compacted.
    let (nx, ny) = (w / mw, h / mh);
    let mut written = 0u16;
    for row in 0..ny {
        for col in 0..nx {
            let (idx0, idx1, keep, slot) = match dir {
                0 => (row, nx - 1 - col, row < 32 && nx - 1 - col < 8, written),
                1 => (row, col, row < 32 && col < 8, written),
                2 => (col, row, row < 8 && col < 32, row * nx + col),
                _ => (nx - 1 - col, ny - 1 - row, ny - 1 - row < 8 && col < 32, row * nx + col),
            };
            if !keep {
                continue;
            }
            let e = ENTRIES_AT + usize::from(slot) * 10;
            out[e] = idx0 as u8;
            out[e + 1] = idx1 as u8;
            out[e + 2..e + 4].copy_from_slice(&(mw * col).to_be_bytes());
            out[e + 4..e + 6].copy_from_slice(&(mh * row).to_be_bytes());
            out[e + 6..e + 8].copy_from_slice(&mw.to_be_bytes());
            out[e + 8..e + 10].copy_from_slice(&mh.to_be_bytes());
            written += 1;
        }
    }
    out[COUNT_AT] = written as u8;
    Ok((
        out,
        format!("0x600: module positions ({nx}x{ny} tiles of {mw}x{mh}, line_dir {dir}, {written} entries)"),
    ))
}
