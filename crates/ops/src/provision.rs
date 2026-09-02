//! Bring a receiver card to a working state in one command: snapshot,
//! firmware, configuration, cabinet identity, verification.
//!
//! Firmware takes both write paths because 16.53 guards blocks 0-2 and 8 from
//! the host path and its SDRAM self-program writes only those. The EEPROM
//! records are read before block 7 is written, because that write wipes their
//! mirror and the card then reports a healthy size while dropping every pixel
//! (docs/provisioning.md, docs/receiver-identity.md).

use crate::capture::discover_one;
use crate::flash::{flash_firmware, read_primary_bank, restore_flash};
use crate::model::{bank_bytes, for_card};
use crate::util::{hex, open};
use crate::{check, config, protocol, restore, screen, upgrade, Ctx, Loader, Progress};
use anyhow::{bail, Context, Result};
use colorlight::eeprom;
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
    let img = std::fs::read(image).with_context(|| format!("read {image}"))?;
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

/// What `e120 provision` takes.
#[derive(Clone, Debug)]
pub struct Args<'a> {
    /// Panel spec file.
    pub spec_path: &'a str,
    /// Vendor firmware image to install; skipped when absent.
    pub firmware: Option<&'a str>,
    /// Cabinet position in the whole screen, in pixels.
    pub position: (u16, u16),
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
        snapshot_dir,
        commit,
        wait,
    } = *a;
    let spec = panelspec::PanelSpec::load(spec_path)?;
    let (w, h) = (spec.module.width, spec.module.height);
    let Some(info) = discover_one(ctx, wait)? else {
        bail!("no response on {} within {wait}s", ctx.iface);
    };
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
        None => for_card(&info)?,
    };
    let ctx = &Ctx { model: Some(m), ..ctx.clone() };
    let running = version_of(&info);
    p.err(&format!(
        "card: {} (id 0x{:02x}), firmware {running}, reports {}x{}",
        m.name, info.card_id, info.cols, info.rows
    ));
    p.err(&format!(
        "plan: spec {spec_path} ({w}x{h}), cabinet at {},{}",
        position.0, position.1
    ));
    let want_version = match firmware {
        Some(fw) => {
            let want = m
                .firmware
                .version_in_name(fw)
                .with_context(|| format!("no version in the firmware file name {fw} ({})", m.firmware.image_pattern))?;
            p.err(&format!("plan: firmware {fw}, card to report {want} afterwards"));
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
    for r in eeprom::RECORDS {
        let (a, n) = (usize::from(r.addr), usize::from(r.len));
        let data: &[u8] = if r.addr == 0x002 {
            &ca
        } else if erased || a + n > before.len() {
            continue;
        } else {
            &before[a..a + n]
        };
        dev.send(&eeprom::write(r.addr, data))?;
        // An EEPROM write takes the card milliseconds; back-to-back records
        // are dropped.
        std::thread::sleep(Duration::from_millis(500));
    }
    dev.send(&eeprom::save())?;
    std::thread::sleep(Duration::from_millis(500));
    dev.send(&eeprom::reload())?;
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

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
