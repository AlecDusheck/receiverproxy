//! Bring a receiver card to a working state in one command: snapshot,
//! firmware, configuration, cabinet identity, verification.
//!
//! Firmware takes both write paths because 16.53 guards blocks 0-2 and 8 from
//! the host path and its SDRAM self-program writes only those. The EEPROM
//! records are read before block 7 is written, because that write wipes their
//! mirror and the card then reports a healthy size while dropping every pixel
//! (docs/provisioning.md, docs/receiver-identity.md).

use crate::capture::discover_one;
use crate::flash::{flash_firmware, read_primary_bank, restore_flash, BANK_BYTES};
use crate::util::{hex, open};
use crate::{config, protocol, restore, screen, upgrade, Cli};
use anyhow::{bail, Context, Result};
use e120_proto::eeprom;
use std::time::{Duration, Instant};

/// Expected firmware version from a vendor image name like
/// `E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex`.
fn version_in_name(path: &str) -> Option<(u8, u8)> {
    let name = std::path::Path::new(path).file_name()?.to_str()?;
    let i = name.find("FPGA")? + 4;
    let rest = &name[i..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let mut it = rest[..end].split('.');
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

fn wait_for_version(
    cli: &Cli,
    want: (u8, u8),
    timeout: Duration,
) -> Result<protocol::DiscoveryInfo> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(info) = discover_one(cli, 2)? {
            if (info.ver_major, info.ver_minor) == want {
                return Ok(info);
            }
        }
        if Instant::now() > deadline {
            bail!(
                "the card did not come back reporting firmware {}.{}",
                want.0,
                want.1
            );
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Blocks 16.53 write-protects from the host path; only the SDRAM
/// self-program can write these.
const HOST_GUARDED_BLOCKS: [u8; 4] = [0, 1, 2, 8];

/// Install `image` into the primary bank through both write paths and
/// verify the whole bank. Returns true when a power-cycle is needed.
fn install_firmware(cli: &Cli, image: &str, backup: &str, wait: u64) -> Result<bool> {
    let img = std::fs::read(image).with_context(|| format!("read {image}"))?;
    let want = &img[..BANK_BYTES.min(img.len())];

    let current = read_primary_bank(&mut open(cli)?, 0, wait)?;
    let differing = |bank_bytes: &[u8]| -> Vec<u8> {
        protocol::FIRMWARE_BLOCKS
            .filter(|&b| b != protocol::PARAM_BLOCK)
            .filter(|&b| {
                let s = usize::from(b) * 0x10000;
                bank_bytes.get(s..s + 0x10000) != want.get(s..s + 0x10000)
            })
            .collect()
    };
    let before = differing(&current);
    if before.is_empty() {
        eprintln!("firmware: bank already holds {image}");
        return Ok(false);
    }
    eprintln!("firmware: blocks {} differ", hex(&before, ","));

    // The card programs the guarded sectors itself from SDRAM.
    eprintln!("firmware: sdram self-program");
    upgrade::install(
        cli,
        image,
        true,
        e120_proto::upgrade::Partition::Primary,
        120,
        3000,
        wait,
    )?;

    // The rest goes in through the host path, block by block.
    let after_sdram = read_primary_bank(&mut open(cli)?, 0, wait)?;
    for &b in differing(&after_sdram)
        .iter()
        .filter(|b| !HOST_GUARDED_BLOCKS.contains(b))
    {
        eprintln!("firmware: host write 0x{b:02x}");
        flash_firmware(cli, image, backup, true, b..b + 1, 0, wait)?;
    }
    let final_bank = read_primary_bank(&mut open(cli)?, 0, wait)?;
    let left = differing(&final_bank);
    if !left.is_empty() {
        bail!(
            "firmware: blocks {} still differ from {image} after both write paths",
            hex(&left, ",")
        );
    }
    eprintln!("firmware: bank verified");
    Ok(true)
}

/// Provision a card: snapshot, firmware, configuration, EEPROM, verify.
///
/// # Errors
/// Fails at the first step whose result cannot be verified.
#[allow(clippy::too_many_lines)]
pub fn provision(
    cli: &Cli,
    spec_path: &str,
    firmware: Option<&str>,
    position: (u16, u16),
    snapshot_dir: Option<&str>,
    commit: bool,
    wait: u64,
) -> Result<()> {
    let spec = e120_rcvbp::spec::PanelSpec::load(spec_path)?;
    let (w, h) = (spec.module.width, spec.module.height);
    let Some(info) = discover_one(cli, wait)? else {
        bail!("no response on {} within {wait}s", cli.iface);
    };
    eprintln!(
        "card: type 0x{:02x}, firmware {}.{}, reports {}x{}",
        info.card_id, info.ver_major, info.ver_minor, info.cols, info.rows
    );
    eprintln!(
        "plan: spec {spec_path} ({w}x{h}), cabinet at {},{}",
        position.0, position.1
    );
    let want_version = match firmware {
        Some(fw) => {
            let (a, b) = version_in_name(fw)
                .with_context(|| format!("no version in the firmware file name {fw}"))?;
            eprintln!("plan: firmware {fw}, card to report {a}.{b} afterwards");
            Some((a, b))
        }
        None => None,
    };
    if !commit {
        println!("dry run: nothing written (add --commit)");
        return Ok(());
    }

    // 1. Snapshot: the only copy of what this card held.
    let snap = snapshot_dir.map_or_else(
        || format!("build/snapshot-{}", unix_seconds()),
        ToString::to_string,
    );
    eprintln!("[1/5] snapshot: {snap}");
    restore::snapshot(cli, &snap, 0, wait)?;
    let backup = format!("{snap}/primary-region.bin");

    // 2. Firmware.
    if let (Some(fw), Some(want)) = (firmware, want_version) {
        eprintln!("[2/5] firmware: {fw}");
        if install_firmware(cli, fw, &backup, wait)? {
            eprintln!(
                "firmware: power-cycle the card now; waiting for {}.{}",
                want.0, want.1
            );
            let info = wait_for_version(cli, want, Duration::from_mins(10))?;
            eprintln!(
                "firmware: card back on {}.{}",
                info.ver_major, info.ver_minor
            );
            // The card answers discovery before it has finished loading its
            // parameters; flash writes sent before then are unreliable.
            std::thread::sleep(Duration::from_secs(12));
        }
    } else {
        eprintln!("[2/5] firmware: skipped (no --firmware)");
    }

    // 3. Read the EEPROM records before block 7 wipes their mirror.
    eprintln!("[3/5] eeprom: reading records");
    let before = {
        let mut dev = open(cli)?;
        screen::read(&mut dev, 0, wait)?
    };
    let erased = screen::looks_erased(&before);
    if erased {
        eprintln!("eeprom: record reads as erased; only the control area will be written");
    }

    // 4. Configuration image.
    eprintln!("[4/5] config: {spec_path}");
    let out = format!("{snap}/config");
    config::gen_config(spec_path, &out)?;
    let img = format!("{out}/{}-block7.bin", spec.name);
    restore_flash(cli, &img, true, 0)?;

    // 5. EEPROM: every record back, control area set for this cabinet.
    eprintln!("[5/5] eeprom: writing records");
    let mut dev = open(cli)?;
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
    let after = screen::read(&mut dev, 0, wait)?;
    match eeprom::parse_control_area(&after[2..]) {
        Some((x0, y0, x1, y1))
            if (x0, y0, x1, y1) == (position.0, position.1, position.0 + w, position.1 + h) =>
        {
            eprintln!("eeprom: control area verified {x0},{y0}-{x1},{y1}");
        }
        other => bail!("eeprom: control area reads back as {other:?}"),
    }
    drop(dev);
    match discover_one(cli, wait)? {
        Some(i) if (i.cols, i.rows) == (w, h) => {
            eprintln!(
                "discovery: {}x{} on firmware {}.{}",
                i.cols, i.rows, i.ver_major, i.ver_minor
            );
        }
        Some(i) => eprintln!(
            "discovery: {}x{} (expected {w}x{h}); usually corrects after the power-cycle",
            i.cols, i.rows
        ),
        None => bail!("the card stopped answering discovery"),
    }

    eprintln!("power-cycle the card to apply");
    Ok(())
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
