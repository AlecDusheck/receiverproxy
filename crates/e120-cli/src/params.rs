//! Pushing parameter packs into the card's RAM, generated from a panel spec.
//!
//! The vendor tool sends the card's whole raster state at the start of every
//! session, not just the chip registers: the lookup tables the scan engine
//! reads from (pixel sequence, scan table, void and anti-void lines) live in
//! RAM and are re-sent every time. Sending only the three type-0x05 packs
//! leaves those tables at whatever the card booted with.

use crate::util::open;
use crate::{protocol, rcvbp, Cli};
use anyhow::{Context, Result};
use e120_net::Bpf;
use rcvbp::image;
use std::time::Duration;

/// One real-time pack: wire type, sub-index, header length, body.
struct Pack<'a> {
    kind: u8,
    sub: u8,
    header: usize,
    body: &'a [u8],
    what: &'a str,
}

impl Pack<'_> {
    /// Frame the pack the way `SendRealTimePacks` does: the pack's first two
    /// bytes are the EtherType, the body sits at the type's header offset.
    fn send(&self, dev: &mut Bpf, gap: Duration) -> Result<()> {
        let mut p = vec![0u8; self.header - 2 + self.body.len()];
        p[1] = self.sub;
        p[self.header - 2..].copy_from_slice(self.body);
        dev.send(&protocol::frame([self.kind, 0x00], &p))?;
        std::thread::sleep(gap);
        Ok(())
    }
}

/// Push the real-time parameter packs for a panel spec, in the vendor's
/// order. RAM only: no flash, no reboot.
pub fn send_params(cli: &Cli, spec_path: &str, chip_only: bool, gap_ms: u64) -> Result<()> {
    let spec = rcvbp::spec::PanelSpec::load(spec_path)?;
    let g = spec.generate()?;
    let gap = Duration::from_millis(gap_ms);
    let mut dev = open(cli)?;

    // Addressed-register chips get their table as the chip pack. A
    // non-addressed chip carries its configuration inside the basic pack's
    // chip-custom block and has no record 0x84 to send.
    match g.rcvbp.records.iter().find(|r| r.rtype[1] == 0x84) {
        Some(r) => {
            let chip = r.payload.clone();
            Pack { kind: 0x05, sub: protocol::params::SUB_CHIP, header: 4, body: &chip, what: "chip registers" }
                .send(&mut dev, gap)?;
            println!("chip-register pack");
        }
        None => println!("no chip-register pack: this chip is configured through the basic pack"),
    }
    if chip_only {
        return Ok(());
    }

    // The rest of the raster state comes from the same regions the boot image
    // carries, so the card gets in RAM exactly what it would boot with.
    let rec01 = g.rcvbp.record_01().context("config has no record 0x01")?.payload.clone();
    let mut b = image::Block7Builder::erased();
    b.zero_regions();
    b.basic_pack(&g.basic_pack)?;
    b.data_swap_from(&rec01)?;
    b.module_positions_from(&rec01)?;
    b.anti_void_lines();
    if spec.mapping.gate_phantom_positions {
        b.void_line_columns(spec.module.width, spec.module.width * 2);
    }
    b.mapping_from(&g.rcvbp)?;
    b.scan_table_from(&rec01, spec.card_scan_len())?;
    let (img, _, _) = b.finish();

    let mut packs: Vec<Pack> = vec![
        Pack { kind: 0x05, sub: protocol::params::SUB_DATA_SWAP, header: 4,
               body: &img[image::DATA_SWAP_OFFSET..image::DATA_SWAP_OFFSET + 0x100], what: "data swap" },
        Pack { kind: 0x05, sub: protocol::params::SUB_BASIC, header: 4,
               body: &g.basic_pack, what: "basic parameters" },
        Pack { kind: 0x10, sub: 0, header: 4, body: &img[0x0100..0x0500], what: "void table" },
        Pack { kind: 0x17, sub: 0, header: 5, body: &img[0x0600..0x0900], what: "module positions" },
    ];
    // Pixel sequence: the mapping table, sliced into 16 packs of 0x300.
    for k in 0..16 {
        let at = image::MAPPING_OFFSET + k * 0x300;
        packs.push(Pack { kind: 0x03, sub: k as u8, header: 4,
                          body: &img[at..at + 0x300], what: "pixel sequence" });
    }
    // Void-line packs 0-1 live at 0x1000, packs 2-3 at 0x6800.
    for k in 0..4usize {
        let at = if k < 2 { 0x1000 + k * 0x400 } else { 0x6800 + (k - 2) * 0x400 };
        packs.push(Pack { kind: 0x1F, sub: k as u8, header: 8,
                          body: &img[at..at + 0x400], what: "void line" });
    }
    // Anti-void packs 0-3 at 0x1800, packs 4-7 at 0x7000.
    for k in 0..8usize {
        let at = if k < 4 { 0x1800 + k * 0x400 } else { 0x7000 + (k - 4) * 0x400 };
        packs.push(Pack { kind: 0x32, sub: k as u8, header: 8,
                          body: &img[at..at + 0x400], what: "anti-void line" });
    }
    packs.push(Pack { kind: 0x18, sub: 0, header: 4,
                      body: &img[image::SCAN_TABLE_OFFSET..image::SCAN_TABLE_OFFSET + 0x400],
                      what: "scan table" });

    let mut counts: Vec<(&str, usize)> = Vec::new();
    for p in &packs {
        p.send(&mut dev, gap)?;
        match counts.last_mut() {
            Some((what, n)) if *what == p.what => *n += 1,
            _ => counts.push((p.what, 1)),
        }
    }
    for (what, n) in counts {
        if n == 1 {
            println!("{what} pack");
        } else {
            println!("{what}: {n} packs");
        }
    }
    println!("sent {} real-time packs for {}", packs.len() + 1, spec.name);
    Ok(())
}
