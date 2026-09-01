//! Turning a spec into the `.rcvbp` and basic pack, with nothing copied
//! from a donor file: record 0x01 from defaults + spec + chip library, the
//! mapping from geometry, the chip registers from the library, the other
//! records from their decoded defaults, and the pack from the record.

use super::{basic_pack, record01, records, PanelSpec};
use crate::chips::ChipLibrary;
use crate::record01::View;
use crate::Rcvbp;
use anyhow::Result;

pub struct Generated {
    pub rcvbp: Rcvbp,
    pub basic_pack: [u8; 256],
    /// One line per byte range placed, with its source.
    pub provenance: Vec<String>,
}

/// # Errors
/// Fails on an invalid spec or a chip library the record cannot hold.
pub fn generate(spec: &PanelSpec, chip: &ChipLibrary) -> Result<Generated> {
    spec.validate()?;
    let mut prov = Vec::new();
    let rec01 = record01::build(spec, chip, &mut prov)?.to_vec();
    let mapping = spec.mapping_record();
    prov.push(format!(
        "rcvbp record 0x03 <- generated ({}x{} stored, 1/{}, groups {}, lines {})",
        spec.module.width,
        spec.module.height / 2,
        spec.module.scan,
        if spec.mapping.reversed_groups { "reversed" } else { "forward" },
        if spec.mapping.reversed_lines { "reversed" } else { "top-down" },
    ));
    let regs = chip.record_84(spec.module.scan)?.to_vec();
    prov.push(format!("rcvbp record 0x84 <- chip library {} (reg 0x02 = scan-1)", chip.name));
    prov.push("rcvbp other records <- decoded vendor defaults (records.rs)".into());
    let rcvbp = records::assemble(spec, rec01.clone(), mapping, regs);
    let basic_pack = basic_pack::body(spec, &View::new(&rec01)?, &mut prov);
    Ok(Generated {
        rcvbp,
        basic_pack,
        provenance: prov,
    })
}
