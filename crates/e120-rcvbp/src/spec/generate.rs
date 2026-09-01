//! Turning a spec into the `.rcvbp` and basic pack: the template's records
//! with record 0x01 rewritten from the spec, the mapping generated, and the
//! chip registers taken from the named source.

use super::{basic_pack, PanelSpec};
use crate::record01::{off, View, LEN};
use crate::Rcvbp;
use anyhow::{bail, Context, Result};

pub struct Generated {
    pub rcvbp: Rcvbp,
    pub basic_pack: [u8; 256],
    /// One line per byte range placed, with its source.
    pub provenance: Vec<String>,
}

/// # Errors
/// Fails on an invalid spec or unusable templates.
pub fn generate(
    spec: &PanelSpec,
    template: &Rcvbp,
    reference_pack: &[u8],
    chip_regs: Option<&[u8]>,
    mapping: Option<&Rcvbp>,
) -> Result<Generated> {
    spec.validate(template)?;
    let mut prov = Vec::new();
    let mut out = Rcvbp {
        version: template.version,
        blob: Vec::new(),
        records: template.records.clone(),
    };
    prov.push(format!("rcvbp: {} records <- template (record 0x01 then edited)", out.records.len()));

    let r01 = out
        .records
        .iter_mut()
        .find(|r| r.rtype[1] == 0x01)
        .context("template has no record 0x01")?;
    apply_to_record01(spec, &mut r01.payload, &mut prov)?;
    let rec01 = r01.payload.clone();

    if let Some(regs) = chip_regs {
        let what = if spec.chip.library.is_some() { "chip.library defaults" } else { "chip.registers_from" };
        replace_record(&mut out, 0x84, regs.to_vec(), &mut prov, what)?;
    }
    match mapping {
        Some(src) => replace_record(
            &mut out,
            0x03,
            src.records
                .iter()
                .find(|r| r.rtype[1] == 0x03)
                .context("template.mapping_from has no record 0x03")?
                .payload
                .clone(),
            &mut prov,
            "template.mapping_from",
        )?,
        None => {
            let note = format!(
                "generated ({}x{} stored, 1/{}, groups {}, lines {})",
                spec.module.width,
                spec.module.height / 2,
                spec.module.scan,
                if spec.mapping.reversed_groups { "reversed" } else { "forward" },
                if spec.mapping.reversed_lines { "reversed" } else { "top-down" },
            );
            replace_record(&mut out, 0x03, spec.mapping_record(), &mut prov, &note)?;
        }
    }

    let basic_pack = basic_pack::body(spec, &View::new(&rec01)?, reference_pack, &mut prov)?;
    Ok(Generated {
        rcvbp: out,
        basic_pack,
        provenance: prov,
    })
}

/// Write the spec's fields into record 0x01, recording each edit.
fn apply_to_record01(spec: &PanelSpec, p: &mut [u8], prov: &mut Vec<String>) -> Result<()> {
    if p.len() < LEN {
        bail!("record 0x01 payload is {} bytes, need {LEN}", p.len());
    }
    let mut put = |at: usize, bytes: &[u8], what: &str| {
        p[at..at + bytes.len()].copy_from_slice(bytes);
        prov.push(format!("record01 +{at:#05x} <- {what}"));
    };
    let m = &spec.module;
    put(off::MODULE_W, &[m.width as u8], "module.width");
    put(off::MODULE_H_HALF, &[(m.height / 2) as u8], "module.height / 2");
    put(off::GAMMA, &spec.timing.gamma.to_le_bytes(), "timing.gamma (f32)");
    put(off::SCAN, &[m.scan], "module.scan");
    put(off::SERIAL_CLOCK, &m.serial_clock.to_le_bytes(), "module.serial_clock");
    put(off::SERIAL_CLOCK_HALF, &(m.serial_clock / 2).to_le_bytes(), "module.serial_clock / 2 (duplicate)");
    put(off::SERIAL_CLOCK_DUP, &m.serial_clock.to_le_bytes(), "module.serial_clock (duplicate)");
    put(off::GRAY, &[m.gray_bits], "module.gray_bits");
    put(off::COLOR_SWAP, &[spec.color.swap], "color.swap");
    put(off::COLOR_SOURCE, &spec.color.source, "color.source");
    put(off::GCLOCK, &[spec.timing.gclock], "timing.gclock");
    put(off::GAINS, &spec.current.gains, "current.gains");
    put(off::CHIP_LO, &[(spec.chip.id & 0xFF) as u8], "chip.id low byte");
    put(off::CHIP_HI, &[(spec.chip.id >> 8) as u8], "chip.id high byte");
    if let Some(sub) = spec.chip.sub_id {
        put(off::SUB_CHIP_LO, &[(sub & 0xFF) as u8], "chip.sub_id low byte");
        put(off::SUB_CHIP_HI, &[(sub >> 8) as u8], "chip.sub_id high byte");
    }
    put(off::LINE_DIR, &[m.line_dir], "module.line_dir");
    put(off::REFRESH, &spec.timing.refresh_hz.to_le_bytes(), "timing.refresh_hz (f32)");
    for (i, pct) in spec.current.percent.iter().enumerate() {
        put(off::CURRENT_PCT + 4 * i, &pct.to_le_bytes(), "current.percent (f32)");
    }
    put(off::MAX_W, &spec.screen.width.to_le_bytes(), "screen.width (MaxWidth)");
    put(off::MAX_H, &spec.screen.height.to_le_bytes(), "screen.height (MaxHeight)");
    Ok(())
}

fn replace_record(
    out: &mut Rcvbp,
    id: u8,
    payload: Vec<u8>,
    prov: &mut Vec<String>,
    what: &str,
) -> Result<()> {
    let slot = out
        .records
        .iter_mut()
        .find(|r| r.rtype[1] == id)
        .with_context(|| format!("template has no record 0x{id:02x} to replace"))?;
    slot.payload = payload;
    prov.push(format!("rcvbp record 0x{id:02x} <- {what}"));
    Ok(())
}
