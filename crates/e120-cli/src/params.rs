//! Pushing parameter packs into the card's RAM.

use crate::util::open;
use crate::{protocol, rcvbp, Cli};
use anyhow::{Context, Result};
use std::time::Duration;

/// Push real-time parameter packs into the card's RAM.
///
/// This is what the vendor tool does at the start of every session rather than
/// relying on the copy in flash, and it needs no reboot.
pub fn send_params(
    cli: &Cli,
    config: &str,
    chip_only: bool,
    all_records: bool,
    gap_ms: u64,
    _index: u16,
) -> Result<()> {
    let f = rcvbp::Rcvbp::load(config)?;
    let mut dev = open(cli)?;
    let gap = Duration::from_millis(gap_ms);

    let chip = f
        .records
        .iter()
        .find(|r| r.rtype[1] == 0x84)
        .context("this config has no chip-register record (0x84)")?;
    dev.send(&protocol::params::frame_for(&protocol::params::chip_pack(
        &chip.payload,
    )))?;
    println!("chip-register pack: {} bytes", chip.payload.len());
    std::thread::sleep(gap);
    if chip_only {
        return Ok(());
    }

    let basic = f.record_01().context("this config has no record 0x01")?;
    dev.send(&protocol::params::frame_for(&protocol::params::basic_pack(
        &basic.payload,
    )))?;
    println!("basic-parameter pack (partially decoded)");
    std::thread::sleep(gap);

    if all_records {
        // Send everything else we hold, on the hypothesis that packs are
        // records copied whole the way the chip pack turned out to be.
        for (n, r) in f
            .records
            .iter()
            .filter(|r| !matches!(r.rtype[1], 0x84 | 0x01) && !r.is_empty_table())
            .enumerate()
        {
            let sub = (n + 2) as u8;
            let packs = if r.payload.len() <= protocol::params::PACK_LEN - 4 {
                vec![protocol::params::verbatim_pack(sub, &r.payload)]
            } else {
                protocol::params::chunked_packs(sub, &r.payload)
            };
            println!(
                "record 0x{:04x} as {} pack(s), sub-index {sub} ({} bytes)",
                r.type_u16(),
                packs.len(),
                r.payload.len()
            );
            for p in &packs {
                dev.send(&protocol::params::frame_for(p))?;
                std::thread::sleep(Duration::from_millis(2));
            }
            std::thread::sleep(gap);
        }
    }
    Ok(())
}

/// Send one record's pack under each sub-index in turn, pausing so the panel
/// can be watched. Everything here is RAM-only.
pub fn sweep_packs(cli: &Cli, config: &str, record: &str, max: u8, secs: u64) -> Result<()> {
    let want = u16::from_str_radix(record.trim_start_matches("0x"), 16)
        .with_context(|| format!("bad record type {record:?}"))?;
    let f = rcvbp::Rcvbp::load(config)?;
    let rec = f
        .find(want)
        .with_context(|| format!("no record 0x{want:04x} in {config}"))?;
    let mut dev = open(cli)?;
    for sub in 0..=max {
        println!("sub-index {sub} (0x{sub:02x})");
        dev.send(&protocol::params::frame_for(
            &protocol::params::verbatim_pack(sub, &rec.payload),
        ))?;
        std::thread::sleep(Duration::from_secs(secs));
    }
    println!("sweep complete");
    Ok(())
}
