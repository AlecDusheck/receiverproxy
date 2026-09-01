//! Pushing parameter packs into the card's RAM, generated from a panel spec.

use crate::util::open;
use crate::{protocol, rcvbp, Cli};
use anyhow::{Context, Result};
use std::time::Duration;

/// Push the real-time parameter packs for a panel spec: chip registers, data
/// swap, then basic parameters — the vendor's order. RAM only: no flash, no
/// reboot, and the chips latch the registers as they arrive.
pub fn send_params(cli: &Cli, spec_path: &str, chip_only: bool, gap_ms: u64) -> Result<()> {
    let spec = rcvbp::spec::PanelSpec::load(spec_path)?;
    let g = spec.generate()?;
    let gap = Duration::from_millis(gap_ms);
    let mut dev = open(cli)?;

    let chip = g
        .rcvbp
        .records
        .iter()
        .find(|r| r.rtype[1] == 0x84)
        .context("config has no chip-register record (0x84)")?;
    dev.send(&protocol::params::frame_for(&protocol::params::chip_pack(&chip.payload)))?;
    println!("chip-register pack");
    std::thread::sleep(gap);
    if chip_only {
        return Ok(());
    }

    let rec01 = &g.rcvbp.record_01().context("config has no record 0x01")?.payload;
    let swap = rcvbp::image::data_swap_body(rec01)?;
    dev.send(&protocol::params::frame_for(&protocol::params::pack(
        protocol::params::SUB_DATA_SWAP,
        &swap,
    )))?;
    println!("data-swap pack");
    std::thread::sleep(gap);

    dev.send(&protocol::params::frame_for(&protocol::params::pack(
        protocol::params::SUB_BASIC,
        &g.basic_pack,
    )))?;
    println!("basic-parameter pack ({})", spec.name);
    Ok(())
}
