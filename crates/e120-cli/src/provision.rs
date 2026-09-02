//! Bring a receiver card from whatever it holds to a working state, in one
//! command: firmware, configuration, cabinet identity, verification.
//!
//! Every step here was learned the hard way on the first card
//! (`docs/rendering-recipe.md`, `docs/black-floor.md`, `docs/receiver-identity.md`):
//!
//! * firmware 16.53 write-protects its header and trailer sectors (blocks
//!   0-2 and 8) from the host path, and its SDRAM self-program writes only
//!   those, so a complete install needs both paths and a whole-bank verify;
//! * writing block 7 wipes the EEPROM mirror, and the card then reports a
//!   healthy size while dropping every pixel, so the EEPROM records are read
//!   first and written back one by one afterwards, with the control area set
//!   for the card's place in the wall;
//! * the card answers discovery before it has finished loading its own
//!   parameters, so anything sent right after power-on is unreliable — the
//!   configuration lives in flash and the card arms itself at boot.

use crate::flash::{flash_firmware, read_blocks, restore_flash};
use crate::util::open;
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

fn discover(cli: &Cli, wait: u64) -> Result<Option<protocol::DiscoveryInfo>> {
    let mut dev = open(cli)?;
    dev.send(&protocol::discovery())?;
    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        for f in dev.recv()? {
            if let Some(info) = protocol::parse_discovery_response(&f) {
                return Ok(Some(info));
            }
        }
    }
    Ok(None)
}

fn wait_for_version(cli: &Cli, want: (u8, u8), timeout: Duration) -> Result<protocol::DiscoveryInfo> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(info) = discover(cli, 2)? {
            if (info.ver_major, info.ver_minor) == want {
                return Ok(info);
            }
        }
        if Instant::now() > deadline {
            bail!("the card did not come back reporting firmware {}.{}", want.0, want.1);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Install `image` into the primary bank through both write paths and
/// verify the whole bank. Returns true when a power-cycle is needed.
fn install_firmware(cli: &Cli, image: &str, backup: &str, wait: u64) -> Result<bool> {
    let img = std::fs::read(image).with_context(|| format!("read {image}"))?;
    let bank = 0xB0000usize;
    let want = &img[..bank.min(img.len())];

    let current = {
        let mut dev = open(cli)?;
        read_blocks(&mut dev, 0, protocol::FIRMWARE_BLOCKS.start, protocol::FIRMWARE_BLOCKS.len() as u16, wait)?
    };
    let differing = |bank_bytes: &[u8]| -> Vec<u8> {
        (0u8..11)
            .filter(|&b| b != protocol::PARAM_BLOCK)
            .filter(|&b| {
                let s = usize::from(b) * 0x10000;
                bank_bytes.get(s..s + 0x10000) != want.get(s..s + 0x10000)
            })
            .collect()
    };
    let before = differing(&current);
    if before.is_empty() {
        println!("firmware: the bank already holds {image}; nothing to do");
        return Ok(false);
    }
    println!("firmware: blocks {before:02x?} differ from {image}");

    // The card programs the guarded sectors itself from SDRAM.
    println!("firmware: SDRAM self-program (the card writes its guarded sectors; do not power off)");
    upgrade::install(cli, image, true, e120_proto::upgrade::Partition::Primary, 120, 3000, wait)?;

    // The rest goes in through the host path, block by block.
    let mut dev = open(cli)?;
    let after_sdram = read_blocks(&mut dev, 0, protocol::FIRMWARE_BLOCKS.start, protocol::FIRMWARE_BLOCKS.len() as u16, wait)?;
    drop(dev);
    let mut left = differing(&after_sdram);
    for b in left.clone() {
        if [0u8, 1, 2, 8].contains(&b) {
            continue; // guarded; only the self-program can write these
        }
        println!("firmware: host write of block 0x{b:02x}");
        flash_firmware(cli, image, backup, true, b..b + 1, 0, wait)?;
    }
    let mut dev = open(cli)?;
    let final_bank = read_blocks(&mut dev, 0, protocol::FIRMWARE_BLOCKS.start, protocol::FIRMWARE_BLOCKS.len() as u16, wait)?;
    left = differing(&final_bank);
    if !left.is_empty() {
        bail!("firmware: blocks {left:02x?} still differ from {image} after both write paths");
    }
    println!("firmware: bank verified against {image}");
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
    let Some(info) = discover(cli, wait)? else {
        bail!("no receiver card answered discovery on {}", cli.iface);
    };
    println!(
        "card: type 0x{:02x}, firmware {}.{}, reports {}x{}",
        info.card_id, info.ver_major, info.ver_minor, info.cols, info.rows
    );
    println!("plan: spec {spec_path} ({w}x{h}), cabinet at ({}, {})", position.0, position.1);
    if let Some(fw) = firmware {
        match version_in_name(fw) {
            Some((a, b)) => println!("      firmware {fw} (expects the card to report {a}.{b} afterwards)"),
            None => bail!("cannot read a version from the firmware file name {fw}"),
        }
    }
    if !commit {
        println!("\ndry run: nothing written. Re-run with --commit.");
        return Ok(());
    }

    // 1. Snapshot: the only copy of what this card held.
    let snap = snapshot_dir.map_or_else(
        || format!("build/snapshot-{}", chrono_like_stamp()),
        ToString::to_string,
    );
    println!("\n[1/5] snapshot -> {snap}");
    restore::snapshot(cli, &snap, 0, wait)?;
    let backup = format!("{snap}/primary-region.bin");

    // 2. Firmware.
    if let Some(fw) = firmware {
        println!("\n[2/5] firmware");
        if install_firmware(cli, fw, &backup, wait)? {
            let want = version_in_name(fw).unwrap();
            println!("firmware: power-cycle the card now; waiting for it to report {}.{} ...", want.0, want.1);
            let info = wait_for_version(cli, want, Duration::from_secs(600))?;
            println!("firmware: card is back on {}.{}", info.ver_major, info.ver_minor);
            // Give it its full boot before touching flash again.
            std::thread::sleep(Duration::from_secs(12));
        }
    } else {
        println!("\n[2/5] firmware: skipped (no --firmware)");
    }

    // 3. Read the EEPROM records before block 7 wipes their mirror.
    println!("\n[3/5] eeprom: reading the current records");
    let before = {
        let mut dev = open(cli)?;
        screen::read(&mut dev, 0, wait)?
    };
    let erased = screen::looks_erased(&before);
    if erased {
        println!("eeprom: the record reads as erased; only the control area will be written");
    }

    // 4. Configuration image.
    println!("\n[4/5] config: generating from {spec_path}");
    let out = format!("{snap}/config");
    config::gen_config(spec_path, &out)?;
    let img = format!("{out}/{}-block7.bin", spec.name);
    restore_flash(cli, &img, true, 0)?;

    // 5. EEPROM: every record back, control area set for this cabinet.
    println!("\n[5/5] eeprom: writing records");
    let mut dev = open(cli)?;
    let ca = eeprom::control_area(position.0, position.1, w, h);
    for r in eeprom::RECORDS {
        let (a, n) = (usize::from(r.addr), usize::from(r.len));
        let data: Vec<u8> = if r.addr == 0x002 {
            ca.to_vec()
        } else if erased || a + n > before.len() {
            continue;
        } else {
            before[a..a + n].to_vec()
        };
        dev.send(&eeprom::write(r.addr, &data))?;
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
        Some((x0, y0, x1, y1)) if (x0, y0, x1, y1) == (position.0, position.1, position.0 + w, position.1 + h) => {
            println!("eeprom: control area verified ({x0},{y0})-({x1},{y1})");
        }
        other => bail!("eeprom: control area reads back as {other:?}"),
    }
    drop(dev);
    match discover(cli, wait)? {
        Some(i) if (i.cols, i.rows) == (w, h) => println!("discovery: {}x{} on firmware {}.{}", i.cols, i.rows, i.ver_major, i.ver_minor),
        Some(i) => println!("discovery: reports {}x{} (expected {w}x{h}); it usually corrects after the power-cycle", i.cols, i.rows),
        None => bail!("the card stopped answering discovery"),
    }

    println!("\ndone. Power-cycle the card: it arms from flash and renders whatever you send.");
    println!("  e120 image picture.png --hold      e120 play video.mp4 --loop");
    Ok(())
}

fn chrono_like_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
