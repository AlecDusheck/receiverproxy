//! Bring a receiver card to a working state in one command: snapshot,
//! firmware, configuration, cabinet identity, verification.
//!
//! Firmware takes both write paths because 16.53 guards blocks 0-2 and 8 from
//! the host path and its SDRAM self-program writes only those. The EEPROM
//! records are read before block 7 is written, because that write wipes their
//! mirror and the card then reports a healthy size while dropping every pixel
//! (docs/provisioning.md, docs/receiver-identity.md).

use crate::capture::{describe, discover_all, discover_one};
use crate::flash::{flash_firmware, read_primary_bank, restore_flash};
use crate::model::{bank_bytes, for_card};
use crate::util::{hex, open};
use crate::{check, config, protocol, restore, screen, upgrade, Ctx, Loader, Progress};
use anyhow::{bail, Context, Result};
use colorlight::{eeprom, BROADCAST};
use receivers::Version;
use std::time::{Duration, Instant};

fn version_of(info: &protocol::DiscoveryInfo) -> Version {
    Version(info.ver_major, info.ver_minor)
}

fn wait_for_version(ctx: &Ctx, want: Version, timeout: Duration) -> Result<protocol::DiscoveryInfo> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(info) = discover_one(ctx, 2)? {
            if version_of(&info) == want {
                return Ok(info);
            }
        }
        if Instant::now() > deadline {
            bail!("the card did not come back reporting firmware {want}");
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Install `image` into the primary bank through both write paths and
/// verify the whole bank. Returns true when a power-cycle is needed.
/// `guarded` are the blocks the running firmware keeps from the host path.
fn install_firmware(
    ctx: &Ctx,
    image: &str,
    backup: &str,
    guarded: &[u8],
    wait: u64,
    p: &mut dyn Progress,
) -> Result<bool> {
    let m = ctx.model()?;
    let img = crate::firmware::load(image, p)?.bytes;
    let want = &img[..bank_bytes(m).min(img.len())];

    let current = read_primary_bank(m, &mut open(ctx)?, 0, wait, p)?;
    let differing = |bank_bytes: &[u8]| -> Vec<u8> {
        m.memory
            .primary_blocks()
            .filter(|&b| b != m.memory.parameter_block)
            .filter(|&b| {
                let s = usize::from(b) * 0x10000;
                bank_bytes.get(s..s + 0x10000) != want.get(s..s + 0x10000)
            })
            .collect()
    };
    let before = differing(&current);
    if before.is_empty() {
        p.err(&format!("firmware: bank already holds {image}"));
        return Ok(false);
    }
    p.err(&format!("firmware: blocks {} differ", hex(&before, ",")));

    // The card programs the guarded sectors itself from SDRAM.
    p.err("firmware: sdram self-program");
    upgrade::install(
        ctx,
        image,
        true,
        colorlight::upgrade::Partition::Primary,
        120,
        3000,
        wait,
        p,
    )?;

    // The rest goes in through the host path, block by block.
    let after_sdram = read_primary_bank(m, &mut open(ctx)?, 0, wait, p)?;
    for &b in differing(&after_sdram).iter().filter(|b| !guarded.contains(b)) {
        p.err(&format!("firmware: host write 0x{b:02x}"));
        flash_firmware(ctx, image, backup, true, b..b + 1, 0, wait, p)?;
    }
    let final_bank = read_primary_bank(m, &mut open(ctx)?, 0, wait, p)?;
    let left = differing(&final_bank);
    if !left.is_empty() {
        bail!(
            "firmware: blocks {} still differ from {image} after both write paths",
            hex(&left, ",")
        );
    }
    p.err("firmware: bank verified");
    Ok(true)
}

/// What `rxp provision` takes.
#[derive(Clone, Debug)]
pub struct Args<'a> {
    /// Panel spec file.
    pub spec_path: &'a str,
    /// Vendor firmware image to install, a `config/firmware.toml` name or a
    /// path; skipped when absent.
    pub firmware: Option<&'a str>,
    /// Cabinet position in the whole screen, in pixels.
    pub position: (u16, u16),
    /// The card's position in the Ethernet chain, the receiver index the
    /// EEPROM frames carry; absent, they broadcast, and a chain of more than
    /// one card is refused.
    pub index: Option<u16>,
    /// Directory for the pre-provisioning snapshot; `build/snapshot-<time>`
    /// when absent.
    pub snapshot_dir: Option<&'a str>,
    /// Write it; without this only the plan is printed.
    pub commit: bool,
    /// Seconds to wait for each reply.
    pub wait: u64,
}

/// Provision a card: snapshot, firmware, configuration, EEPROM, verify.
/// Cancellation is honoured between the five steps.
///
/// # Errors
/// Fails at the first step whose result cannot be verified.
#[allow(clippy::too_many_lines)]
pub fn provision(ctx: &Ctx, a: &Args, load: Loader, p: &mut dyn Progress) -> Result<()> {
    let Args {
        spec_path,
        firmware,
        position,
        index,
        snapshot_dir,
        commit,
        wait,
    } = *a;
    let spec = panelspec::PanelSpec::load(spec_path)?;
    let (w, h) = (spec.module.width, spec.module.height);
    let cards = discover_all(ctx, wait, |i| p.err(&describe(i)))?;
    let Some(info) = cards.first() else {
        bail!("no response on {} within {wait}s", ctx.iface);
    };
    // A broadcast EEPROM write gives every card on the chain the same window.
    if cards.len() > 1 && index.is_none() {
        bail!("{} cards answered discovery; pass --index", cards.len());
    }
    let rcv = index.unwrap_or(BROADCAST);
    // The discovered id byte picks the model; `--card` stands in for an id
    // no file carries, but a known id that disagrees with it is refused.
    let m = match ctx.model {
        Some(named) => {
            if let Some(known) = receivers::by_id(info.card_id) {
                anyhow::ensure!(
                    std::ptr::eq(known, named),
                    "the card answers as {} (id 0x{:02x}), not {}",
                    known.name,
                    info.card_id,
                    named.name
                );
            }
            named
        }
        None => for_card(info)?,
    };
    let ctx = &Ctx { model: Some(m), ..ctx.clone() };
    let running = version_of(info);
    p.err(&format!(
        "card: {} (id 0x{:02x}), firmware {running}, reports {}x{}",
        m.name, info.card_id, info.cols, info.rows
    ));
    p.err(&format!(
        "plan: spec {spec_path} ({w}x{h}), cabinet at {},{}",
        position.0, position.1
    ));
    p.err(&format!("plan: {}", eeprom_target(index)));
    let want_version = match firmware {
        Some(fw) => {
            let r = crate::firmware::resolve(fw)?;
            let want = match r.image {
                Some(i) => i.version,
                None => m
                    .firmware
                    .version_in_name(fw)
                    .with_context(|| format!("no version in the firmware file name {fw} ({})", m.firmware.image_pattern))?,
            };
            p.err(&format!(
                "plan: firmware {} ({}), card to report {want} afterwards",
                r.path.display(),
                if r.image.is_some() { "in config/firmware.toml" } else { "not in config/firmware.toml" }
            ));
            Some(want)
        }
        None => None,
    };
    if !commit {
        p.out("dry run: nothing written (add --commit)");
        return Ok(());
    }

    // 1. Snapshot: the only copy of what this card held.
    check(p)?;
    let snap = snapshot_dir.map_or_else(
        || format!("build/snapshot-{}", unix_seconds()),
        ToString::to_string,
    );
    p.err(&format!("[1/5] snapshot: {snap}"));
    restore::snapshot(ctx, &snap, 0, wait, p)?;
    let backup = format!("{snap}/primary-region.bin");

    // 2. Firmware.
    check(p)?;
    if let (Some(fw), Some(want)) = (firmware, want_version) {
        p.err(&format!("[2/5] firmware: {fw}"));
        // The host page writes run under the firmware the card is on now.
        let guarded = m.memory.guarded_blocks(running);
        if !guarded.is_empty() {
            p.err(&format!("firmware: {running} guards blocks {} from host writes", hex(guarded, ",")));
        }
        if install_firmware(ctx, fw, &backup, guarded, wait, p)? {
            p.err(&format!("firmware: power-cycle the card now; waiting for {want}"));
            let info = wait_for_version(ctx, want, Duration::from_mins(10))?;
            p.err(&format!("firmware: card back on {}", version_of(&info)));
            // The card answers discovery before it has finished loading its
            // parameters; flash writes sent before then are unreliable.
            std::thread::sleep(Duration::from_secs(12));
        }
    } else {
        p.err("[2/5] firmware: skipped (no --firmware)");
    }

    // 3. Read the EEPROM records before block 7 wipes their mirror.
    check(p)?;
    p.err("[3/5] eeprom: reading records");
    let before = {
        let mut dev = open(ctx)?;
        screen::read(m, &mut dev, 0, wait)?
    };
    let erased = screen::looks_erased(&before);
    if erased {
        p.err("eeprom: record reads as erased; only the control area will be written");
    }

    // 4. Configuration image.
    check(p)?;
    p.err(&format!("[4/5] config: {spec_path}"));
    let out = format!("{snap}/config");
    config::gen_config(m, spec_path, &out, "rcvbp", load, p)?;
    let img = format!("{out}/{}-block7.bin", spec.name);
    restore_flash(ctx, &img, true, 0, p)?;

    // 5. EEPROM: every record back, control area set for this cabinet.
    check(p)?;
    p.err("[5/5] eeprom: writing records");
    let mut dev = open(ctx)?;
    let ca = eeprom::control_area(position.0, position.1, w, h);
    let kept = if erased { &[][..] } else { &before[..] };
    for f in eeprom_writes(rcv, &ca, kept) {
        dev.send(&f)?;
        // An EEPROM write takes the card milliseconds; back-to-back records
        // are dropped.
        std::thread::sleep(Duration::from_millis(500));
    }
    dev.send(&eeprom::save_to(rcv))?;
    std::thread::sleep(Duration::from_millis(500));
    dev.send(&eeprom::reload_to(rcv))?;
    std::thread::sleep(Duration::from_secs(1));

    // Verify.
    let after = screen::read(m, &mut dev, 0, wait)?;
    match eeprom::parse_control_area(&after[2..]) {
        Some((x0, y0, x1, y1))
            if (x0, y0, x1, y1) == (position.0, position.1, position.0 + w, position.1 + h) =>
        {
            p.err(&format!(
                "eeprom: control area verified {x0},{y0}-{x1},{y1}"
            ));
        }
        other => bail!("eeprom: control area reads back as {other:?}"),
    }
    drop(dev);
    match discover_one(ctx, wait)? {
        Some(i) if (i.cols, i.rows) == (w, h) => {
            p.err(&format!(
                "discovery: {}x{} on firmware {}.{}",
                i.cols, i.rows, i.ver_major, i.ver_minor
            ));
        }
        Some(i) => p.err(&format!(
            "discovery: {}x{} (expected {w}x{h}); usually corrects after the power-cycle",
            i.cols, i.rows
        )),
        None => bail!("the card stopped answering discovery"),
    }

    p.err("power-cycle the card to apply");
    Ok(())
}

/// The plan line for the EEPROM step's addressing.
fn eeprom_target(index: Option<u16>) -> String {
    index.map_or_else(
        || "eeprom: broadcast (every card on the chain)".to_string(),
        |i| format!("eeprom: card index {i}"),
    )
}

/// The record writes of step 5 to receiver `rcv`, in `RECORDS` order: the
/// control area `ca` at 0x002, every other record from `kept` (the read-back
/// set; empty when it read as erased, then only the control area goes).
fn eeprom_writes(rcv: u16, ca: &[u8; 42], kept: &[u8]) -> Vec<Vec<u8>> {
    eeprom::RECORDS
        .iter()
        .filter_map(|r| {
            let (a, n) = (usize::from(r.addr), usize::from(r.len));
            let data: &[u8] = if r.addr == 0x002 { ca } else { kept.get(a..a + n)? };
            Some(eeprom::write_to(rcv, r.addr, data))
        })
        .collect()
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_back() -> Vec<u8> {
        (0..=255u8).collect()
    }

    #[test]
    fn single_card_default_broadcasts_byte_for_byte() {
        let ca = eeprom::control_area(0, 0, 128, 64);
        let before = read_back();
        let frames = eeprom_writes(BROADCAST, &ca, &before);
        assert_eq!(frames.len(), eeprom::RECORDS.len());
        for (f, r) in frames.iter().zip(eeprom::RECORDS) {
            let (a, n) = (usize::from(r.addr), usize::from(r.len));
            let data: &[u8] = if r.addr == 0x002 { &ca } else { &before[a..a + n] };
            assert_eq!(*f, eeprom::write(r.addr, data), "{}", r.name);
            assert_eq!(&f[15..17], &[0xff, 0xff], "{}", r.name);
        }
        assert_eq!(eeprom::save_to(BROADCAST), eeprom::save());
        assert_eq!(eeprom::reload_to(BROADCAST), eeprom::reload());
    }

    #[test]
    fn an_index_addresses_every_frame() {
        let ca = eeprom::control_area(128, 0, 128, 64);
        for f in eeprom_writes(2, &ca, &read_back()) {
            assert_eq!(&f[15..18], &[0x00, 0x02, 0x85]);
        }
        assert_eq!(&eeprom::save_to(2)[15..18], &[0x00, 0x02, 0x87]);
        assert_eq!(&eeprom::reload_to(2)[15..18], &[0x00, 0x02, 0x77]);
    }

    #[test]
    fn an_erased_record_set_writes_only_the_control_area() {
        let ca = eeprom::control_area(0, 0, 128, 64);
        let frames = eeprom_writes(BROADCAST, &ca, &[]);
        assert_eq!(frames.len(), 1);
        assert_eq!(&frames[0][18..22], &[0, 0, 0, 2]);
        assert_eq!(&frames[0][26..34], &ca[..8]);
    }

    #[test]
    fn plan_line_names_the_target() {
        assert_eq!(eeprom_target(None), "eeprom: broadcast (every card on the chain)");
        assert_eq!(eeprom_target(Some(3)), "eeprom: card index 3");
    }
}
