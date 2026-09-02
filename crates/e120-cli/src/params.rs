//! Pushing parameter packs into the card's RAM, generated from a panel spec.
//!
//! The vendor tool re-sends the whole raster state on every push, not only the
//! chip registers: pixel sequence, scan table and void tables live in RAM, and
//! sending only the three type-0x05 packs leaves them as the card booted.

use crate::util::open;
use crate::{protocol, rcvbp, Cli};
use anyhow::Result;
use e120_net::Link;
use rcvbp::image;
use std::time::Duration;

/// Image offsets of the second halves of the void-line and anti-void tables,
/// which `e120_rcvbp::image` does not name.
const VOID_LINE_HIGH_OFFSET: usize = 0x6800;
const ANTI_VOID_HIGH_OFFSET: usize = 0x7000;

/// One real-time pack: wire type, sub-index, header length, body.
struct Pack<'a> {
    kind: u8,
    sub: u8,
    header: usize,
    body: &'a [u8],
}

impl Pack<'_> {
    /// Frame the pack the way `SendRealTimePacks` does: the pack's first two
    /// bytes are the EtherType, the body sits at the type's header offset.
    fn send(&self, dev: &mut Link, gap: Duration) -> Result<()> {
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
#[rustfmt::skip] // one pack per line reads as the vendor's send table
pub fn send_params(cli: &Cli, spec_path: &str, chip_only: bool, gap_ms: u64) -> Result<()> {
    let spec = rcvbp::spec::PanelSpec::load(spec_path)?;
    let g = spec.generate()?;
    let gap = Duration::from_millis(gap_ms);
    let mut dev = open(cli)?;

    // Addressed-register chips get their table as the chip pack. A
    // non-addressed chip carries its configuration inside the basic pack's
    // chip-custom block and has no record 0x84 to send.
    if let Some(r) = g.rcvbp.find_by_id(0x84) {
        Pack { kind: 0x05, sub: protocol::params::SUB_CHIP, header: 4, body: &r.payload }
            .send(&mut dev, gap)?;
    }
    if chip_only {
        return Ok(());
    }

    // The rest of the raster state comes from the same regions the boot image
    // carries, so the card gets in RAM exactly what it would boot with.
    let img = image::Block7Builder::from_generated(&spec, &g)?.finish().image;

    let mut packs: Vec<Pack> = vec![
        Pack { kind: 0x05, sub: protocol::params::SUB_DATA_SWAP, header: 4,
               body: &img[image::DATA_SWAP_OFFSET..image::DATA_SWAP_OFFSET + 0x100] },
        Pack { kind: 0x05, sub: protocol::params::SUB_BASIC, header: 4, body: &g.basic_pack },
        Pack { kind: 0x10, sub: 0, header: 4, body: &img[0x0100..0x0500] }, // void table
        Pack { kind: 0x17, sub: 0, header: 5, body: &img[0x0600..0x0900] }, // module positions
    ];
    // Pixel sequence: the mapping table, sliced into 16 packs of 0x300.
    for k in 0..16 {
        let at = image::MAPPING_OFFSET + k * 0x300;
        packs.push(Pack { kind: 0x03, sub: k as u8, header: 4, body: &img[at..at + 0x300] });
    }
    // Void-line and anti-void tables each split across two image regions
    // (docs/compiled-image-format.md); the packs follow that split.
    for k in 0..4usize {
        let at = if k < 2 { image::VOID_LINE_OFFSET + k * 0x400 } else { VOID_LINE_HIGH_OFFSET + (k - 2) * 0x400 };
        packs.push(Pack { kind: 0x1F, sub: k as u8, header: 8, body: &img[at..at + 0x400] });
    }
    for k in 0..8usize {
        let at = if k < 4 { image::ANTI_VOID_OFFSET + k * 0x400 } else { ANTI_VOID_HIGH_OFFSET + (k - 4) * 0x400 };
        packs.push(Pack { kind: 0x32, sub: k as u8, header: 8, body: &img[at..at + 0x400] });
    }
    packs.push(Pack { kind: 0x18, sub: 0, header: 4,
                      body: &img[image::SCAN_TABLE_OFFSET..image::SCAN_TABLE_OFFSET + 0x400] });

    for p in &packs {
        p.send(&mut dev, gap)?;
    }
    Ok(())
}
