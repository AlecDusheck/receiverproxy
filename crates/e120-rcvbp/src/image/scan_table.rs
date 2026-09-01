//! The scan table (`CalScanTalbeDefault` @ 0x14d710): the card's PWM bit-time schedule.
//!
//! Computed the way the vendor does: a field table of (segment id, enabled
//! slots) per gray level, bit times snapped to 8-unit quanta, then rendered
//! bucket by bucket. Reproduces the factory image byte-exact under test.
//!
//! Transcribed cases: default style, 16 segments, 14-bit gray. The vendor
//! hand-codes one field-table block per gray level; only 14 is transcribed.

use crate::record01::View;
use anyhow::{bail, Result};

pub const LEN: usize = 0x400;
const LEVELS: usize = 16;
const SLOTS: u32 = 32;
const MAX_ENTRIES: usize = 0xE7;

/// Field-table block for 16 segments at 14-bit gray, levels 0..=12
/// (`InitFieldTable16Segment`, jump table 0x1d722C entry 12): (segment id,
/// enabled slot bits). The top level gets all 16 slots with id 16.
const FIELD_TABLE_16SEG_GRAY14: [(u32, u32); 13] = [
    (1, 1 << 1),
    (1, 1 << 2),
    (1, 1 << 6),
    (1, 1 << 7),
    (1, 1 << 9),
    (1, 1 << 10),
    (1, 1 << 14),
    (1, 1 << 15),
    (2, (1 << 5) | (1 << 13)),
    (2, (1 << 3) | (1 << 11)),
    (4, 0x1111),
    (8, 0x5555),
    (8, 0xAAAA),
];

struct FieldTable {
    id: [u32; LEVELS],
    enable: [u32; LEVELS],
    value: [[i32; SLOTS as usize]; LEVELS],
}

/// # Errors
/// Fails for inputs whose vendor tables are not transcribed.
pub fn body(rec: &View, card_scan_len: u16) -> Result<[u8; LEN]> {
    let gray = u32::from(rec.gray());
    let n_seg = rec.segments();
    let style = rec.hr_style();
    if style != 0 || n_seg != 16 || gray != 14 {
        bail!(
            "scan-table solver: style {style}, {n_seg} segments, {gray}-bit gray is not a \
             transcribed case (only style 0 / 16 segments / 14-bit)"
        );
    }
    if rec.hr_scan_style() != 0 {
        bail!("scan-table solver: high-refresh scan style {} not transcribed", rec.hr_scan_style());
    }
    let line_time = (u32::from(card_scan_len) * u32::from(rec.serial_clock()) * 8) as f32;
    let mut ft = init_field_table(gray, line_time, rec.min_oe())?;
    fill_bit_times(&mut ft, gray, n_seg, rec.min_oe());
    Ok(render(&ft, n_seg, rec.scan()))
}

/// `InitFieldTable16Segment`, style 0: pick the level that gets the full
/// 16-slot segment, and take the rest from the hand-coded block.
fn init_field_table(gray: u32, line_time: f32, min_oe: f32) -> Result<FieldTable> {
    let mut k = 16u32;
    let (x0, mut x1) = (line_time * 1.2, min_oe);
    for e in 0..gray {
        if x1 > x0 {
            k = e;
            break;
        }
        x1 += x1;
    }
    let hi = k + 3;
    let c = gray - 1; // A[style 0] = -1
    let top = if hi > c { c.max(1) } else { hi };
    if top != gray - 1 {
        bail!("scan-table solver: two-level fill path not transcribed");
    }
    let mut ft = FieldTable {
        id: [0; LEVELS],
        enable: [0; LEVELS],
        value: [[0; SLOTS as usize]; LEVELS],
    };
    for (level, &(seg, bits)) in FIELD_TABLE_16SEG_GRAY14.iter().enumerate() {
        ft.id[level] = seg;
        ft.enable[level] = bits;
    }
    ft.id[top as usize] = 16;
    ft.enable[top as usize] = 0xFFFF;
    Ok(ft)
}

/// The vendor's 8-unit snap: round-half-away(x / 8) * 8, in f32.
fn snap8(x: f32) -> f32 {
    let t = x * 0.125;
    let t = t + 0.499_999_97_f32.copysign(t);
    t.trunc() * 8.0
}

/// `FromSegmentToFrameTime`: levels whose segment count equals nSeg move to
/// the upper slot half; then each enabled slot gets a snapped bit time.
fn fill_bit_times(ft: &mut FieldTable, gray: u32, n_seg: u32, min_oe: f32) {
    for i in 0..LEVELS {
        if ft.id[i] == n_seg {
            ft.enable[i] = 0xFFFF_0000;
        }
    }
    for level in (1..gray).rev() {
        let l = level as usize;
        let mut seg = ft.id[l];
        let t = ((1u64 << level) as f64 * f64::from(min_oe) / f64::from(seg)) as f32;
        if seg >= n_seg {
            let q = seg / n_seg;
            seg %= n_seg;
            let mut a = 0.0f32;
            for j in n_seg..2 * n_seg {
                a += q as f32 * t;
                let r = snap8(a);
                ft.value[l][j as usize] = r as i32;
                a -= r;
            }
        }
        if seg > 0 {
            let mut a = 0.0f32;
            for j in 0..n_seg {
                if ft.enable[l] & (1 << j) == 0 {
                    continue;
                }
                a += t;
                let r = snap8(a);
                ft.value[l][j as usize] = r as i32;
                a -= r;
            }
        }
    }
}

/// `FieldTableToScanTable` plus the common tail: bucket enabled slots by
/// slot % nSeg (highest level first), emit `(level, 24-bit BE value/8)`
/// entries per bucket with `(start, end)` pairs at +0x3C0, then the scan
/// mode and identity line order.
fn render(ft: &FieldTable, n_seg: u32, scan_mode: u8) -> [u8; LEN] {
    let mut buckets: Vec<Vec<(u8, i32)>> = vec![Vec::new(); n_seg as usize];
    for level in (0..LEVELS).rev() {
        for b in 0..SLOTS {
            if ft.enable[level] & (1 << b) != 0 {
                let v = ft.value[level][b as usize];
                let v8 = if v < 0 { (v + 7) >> 3 } else { v >> 3 };
                buckets[(b % n_seg) as usize].push((level as u8, v8));
            }
        }
    }
    let mut st = [0u8; LEN];
    st[0x39F] = n_seg as u8;
    let mut pos = 0usize;
    for (i, bucket) in buckets.iter().enumerate() {
        let start = pos;
        for &(level, v) in bucket {
            if pos >= MAX_ENTRIES {
                continue;
            }
            st[pos * 4] = level;
            st[pos * 4 + 1..pos * 4 + 4].copy_from_slice(&v.to_be_bytes()[1..4]);
            pos += 1;
        }
        st[0x3C0 + 2 * i] = start as u8;
        st[0x3C1 + 2 * i] = (pos - 1) as u8;
    }
    st[0x39E] = scan_mode;
    for i in 0..scan_mode {
        st[0x3A0 + usize::from(i)] = i;
    }
    st
}
