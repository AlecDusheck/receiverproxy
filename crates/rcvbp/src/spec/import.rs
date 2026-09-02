//! The reverse of `generate`: the spec that regenerates a `.rcvbp`. Record
//! 0x01 is read field by field, the chip library is chosen by the chip id,
//! the wiring knobs are fitted to record 0x03 the way `scripts/corpus-mine.py`
//! `fit_map` does, and whatever the regenerated file still differs in is
//! reported by name rather than guessed.

use super::{generate, mapping};
use crate::record01::{off, View};
use crate::Rcvbp;
use anyhow::{Context, Result};
use panelspec::{Boot, Chip, ChipLibrary, Color, Current, Mapping, Meta, Module, PanelSpec, Screen, Timing};
use std::collections::BTreeMap;

/// Maps a chip family id to an embedded chip library as `(path, text)`.
pub type ChipLookup<'a> = &'a dyn Fn(u16) -> Option<(String, String)>;

/// Record 0x01 bytes the spec carries as `[record01_overrides]` when a file
/// sets them off the generator's defaults: +0x02F (the bench needs 1, the
/// vendor's Reset writes 0) and +0x043 (a literal the vendor sets from
/// unnamed state).
const CARRIED: [usize; 2] = [0x02F, 0x043];

/// The spec that regenerates `bytes`, and the fields it could not recover.
///
/// A file with no library for its chip id comes back with an empty
/// `[chip].library`; the records that only regenerate from a library are
/// then listed as unresolved too.
///
/// # Errors
/// Fails when `bytes` is not a `.rcvbp` or lacks record 0x01.
pub fn spec_from_rcvbp(bytes: &[u8], chips: ChipLookup) -> Result<(PanelSpec, Vec<String>)> {
    let file = Rcvbp::from_bytes(bytes)?;
    let rec01 = file.record_01().context("no record 0x01")?;
    let v = View::new(&rec01.payload)?;
    let mut unresolved = vec!["meta".to_owned()];

    let chip_id = v.chip_id();
    let library = chips(chip_id);
    let chip = match &library {
        Some((path, text)) => Some(ChipLibrary::parse(text).with_context(|| format!("parse {path}"))?),
        None => {
            unresolved.push(format!("chip.library (no library for chip id {chip_id:#06x})"));
            None
        }
    };
    let library_path = library.as_ref().map(|(p, _)| p.as_str()).unwrap_or_default();
    let chip_slug = library_path
        .rsplit('/')
        .next()
        .and_then(|f| f.strip_suffix(".toml"))
        .map_or_else(|| format!("chip-{chip_id:#06x}"), str::to_owned);

    let (width, height, scan) = (v.module_width(), v.module_height_stored() * 2, v.scan());
    let sck = v.serial_clock();
    let gray = v.gray();
    let mut spec = PanelSpec {
        name: format!("{width}x{height}-{scan}s-{chip_slug}"),
        meta: Meta::default(),
        module: Module {
            width,
            height,
            scan,
            serial_clock: Some(sck).filter(|&s| chip.as_ref().is_none_or(|c| c.serial_clock != s)),
            gray_bits: Some(gray).filter(|&g| chip.as_ref().and_then(|c| c.gray_bits().ok()) != Some(g)),
            line_dir: v.line_dir(),
            data_groups: v.u8(off::DATA_GROUPS),
        },
        screen: Screen {
            width: v.max_width(),
            height: v.max_height(),
        },
        chip: Chip {
            library: library_path.to_owned(),
        },
        color: Color {
            swap: v.u8(off::COLOR_SWAP),
            source: [v.u8(off::COLOR_SOURCE), v.u8(off::COLOR_SOURCE + 1), v.u8(off::COLOR_SOURCE + 2)],
        },
        current: Current {
            gains: [v.u8(off::GAINS), v.u8(off::GAINS + 1), v.u8(off::GAINS + 2), v.u8(off::GAINS + 3)],
            percent: [
                v.f32_le(off::CURRENT_PCT),
                v.f32_le(off::CURRENT_PCT + 4),
                v.f32_le(off::CURRENT_PCT + 8),
            ],
        },
        timing: Timing {
            gamma: v.f32_le(off::GAMMA),
            refresh_hz: v.f32_le(off::REFRESH),
            gclock: v.u8(off::GCLOCK),
            min_oe: v.min_oe(),
            luminance_level: v.luminance_level(),
            oe_8ns: v.u8(off::OE_8NS) & 1 == 1,
        },
        mapping: Mapping::default(),
        boot: Boot::default(),
        record01_overrides: BTreeMap::new(),
    };

    if let Err(e) = spec.validate() {
        // Nothing past the geometry can be fitted or regenerated.
        unresolved.push(format!("module ({e:#})"));
        unresolved.extend(["mapping".to_owned(), "record01_overrides".to_owned()]);
        unresolved.extend(NOT_IN_FILE.iter().map(|&s| s.to_owned()));
        return Ok((spec, unresolved));
    }

    match file.find_by_id(0x03) {
        Some(r) => match fit_mapping(&mut spec, &r.payload) {
            Some(m) => spec.mapping = m,
            None => unresolved.push("mapping (record 0x03 fits no block, group or line order)".to_owned()),
        },
        None => unresolved.push("mapping (no record 0x03)".to_owned()),
    }

    match &chip {
        Some(chip) => match generate(&spec, chip) {
            Ok(g) => unresolved.extend(reconcile(&mut spec, &file, &g.rcvbp)),
            Err(e) => unresolved.push(format!("record01_overrides ({e:#})")),
        },
        None => unresolved.push("record01_overrides, record 0x84 (no chip library)".to_owned()),
    }
    unresolved.extend(NOT_IN_FILE.iter().map(|&s| s.to_owned()));
    Ok((spec, unresolved))
}

/// Spec fields the file does not carry; they stay at their defaults.
const NOT_IN_FILE: [&str; 2] = ["mapping.gate_phantom_positions", "boot.arm_at_boot"];

/// `fit_map` of `scripts/corpus-mine.py`: the first of block `w, w/2, w/4,
/// w/8`, reversed groups then not, top-down lines then reversed, whose
/// generated record 0x03 equals `table`.
fn fit_mapping(spec: &mut PanelSpec, table: &[u8]) -> Option<Mapping> {
    let w = spec.module.width;
    let mut blocks = vec![w, w / 2, w / 4, w / 8];
    blocks.retain(|&b| b > 0);
    blocks.dedup();
    for block in blocks {
        for reversed_groups in [true, false] {
            for reversed_lines in [false, true] {
                spec.mapping = Mapping {
                    reversed_groups,
                    reversed_lines,
                    block: Some(block),
                    ..Mapping::default()
                };
                if mapping::record(spec) == table {
                    // The vendor consensus wiring is the spec's default; leave it unwritten.
                    spec.mapping.block = Some(block).filter(|&b| b != w);
                    return Some(spec.mapping.clone());
                }
            }
        }
    }
    None
}

/// Compare the regenerated file with the imported one record by record:
/// carried record 0x01 bytes become overrides, every other difference is
/// named.
fn reconcile(spec: &mut PanelSpec, file: &Rcvbp, ours: &Rcvbp) -> Vec<String> {
    let mut out = Vec::new();
    for rec in &ours.records {
        let id = rec.id();
        let Some(theirs) = file.find_by_id(id) else {
            out.push(format!("record 0x{id:02x} (not in the file; the spec regenerates it)"));
            continue;
        };
        if id == 0x01 {
            for (at, (a, b)) in rec.payload.iter().zip(&theirs.payload).enumerate() {
                if a == b {
                    continue;
                }
                if CARRIED.contains(&at) {
                    spec.record01_overrides.insert(at, *b);
                } else {
                    out.push(format!("record 0x01 +{at:#05x} (file {b:#04x}, regenerated {a:#04x})"));
                }
            }
            if rec.payload.len() != theirs.payload.len() {
                out.push(format!(
                    "record 0x01 ({} bytes in the file, {} regenerated)",
                    theirs.payload.len(),
                    rec.payload.len()
                ));
            }
            continue;
        }
        if rec.payload != theirs.payload {
            let n = rec.payload.iter().zip(&theirs.payload).filter(|(a, b)| a != b).count()
                + rec.payload.len().abs_diff(theirs.payload.len());
            out.push(format!("record 0x{id:02x} ({n} bytes differ from the regenerated record)"));
        }
    }
    for rec in &file.records {
        if ours.find_by_id(rec.id()).is_none() {
            out.push(format!("record 0x{:02x} (in the file; the spec does not regenerate it)", rec.id()));
        }
    }
    out
}
