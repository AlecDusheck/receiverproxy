//! Spec to `.rcvbp` and basic pack: record 0x01 from defaults + spec + chip
//! library, the mapping from geometry, record 0x84 from the library, the rest
//! from decoded defaults, and the pack from the finished record 0x01.

use super::{basic_pack, mapping, record01, records};
use crate::record01::View;
use crate::Rcvbp;
use anyhow::Result;
use panelspec::{ChipLibrary, PanelSpec};

pub struct Generated {
    pub rcvbp: Rcvbp,
    pub basic_pack: [u8; 256],
    /// One line per byte range placed, with its source.
    pub sources: Vec<String>,
}

/// # Errors
/// Fails on an invalid spec or a chip library the record cannot hold.
pub fn generate(spec: &PanelSpec, chip: &ChipLibrary) -> Result<Generated> {
    spec.validate()?;
    let mut prov = Vec::new();
    let rec01 = record01::build(spec, chip, &mut prov)?.to_vec();
    let mapping = mapping::record(spec);
    prov.push(format!(
        "rcvbp record 0x03 <- generated ({}x{} stored, 1/{}, groups {}, lines {})",
        spec.module.width,
        spec.module.height / 2,
        spec.module.scan,
        if spec.mapping.reversed_groups { "reversed" } else { "forward" },
        if spec.mapping.reversed_lines { "reversed" } else { "top-down" },
    ));
    let regs = chip.record_84(spec.module.scan)?.map(|r| r.to_vec());
    match &regs {
        Some(_) => prov.push(format!("rcvbp record 0x84 <- chip library {} (reg 0x02 = scan-1)", chip.name)),
        None => prov.push(format!("rcvbp record 0x84 omitted: {} has no addressed register table", chip.name)),
    }
    prov.push("rcvbp other records <- decoded vendor defaults (records.rs)".into());
    let basic_pack = basic_pack::body(spec, View::new(&rec01)?, &mut prov);
    let rcvbp = records::assemble(spec, rec01, mapping, regs);
    Ok(Generated {
        rcvbp,
        basic_pack,
        sources: prov,
    })
}
