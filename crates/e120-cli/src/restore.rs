//! Putting the card back the way it was.
//!
//! Every write in this project is paired with a way to undo it, and this is
//! that half. The card keeps three separate pieces of state and each needs a
//! different route back:
//!
//! * the **firmware image** in flash blocks 0x00-0x0A, of which only 0x03
//!   onwards can be written at all;
//! * the **`.rcvbp` configuration** at 0x070000, which lives inside the
//!   firmware image's address range and is erased whenever firmware is written;
//! * the **screen-size record** in a small EEPROM, which the block erase also
//!   clears and which the ordinary page frames cannot reach.
//!
//! Restoring firmware therefore destroys the configuration, so `all` sequences
//! them in the order that leaves the card whole.

use crate::flash::{read_blocks, rewrite_block, write_config};
use crate::util::open;
use crate::{protocol, Cli};
use anyhow::{Context, Result};

/// A saved copy of everything we know how to put back.
#[derive(Debug)]
pub struct Snapshot {
    pub firmware: Option<Vec<u8>>,
    pub config: Option<String>,
}

/// Load whatever a snapshot directory holds.
///
/// # Errors
/// Fails if a file is present but unreadable or the wrong size.
pub fn load_snapshot(dir: &str) -> Result<Snapshot> {
    let firmware_path = format!("{dir}/primary-region.bin");
    let firmware = match std::fs::read(&firmware_path) {
        Ok(d) => {
            let span = protocol::FIRMWARE_BLOCKS.len() * 64 * 1024;
            anyhow::ensure!(
                d.len() >= span,
                "{firmware_path} is {} bytes; a primary-bank image needs at least {span}",
                d.len()
            );
            Some(d)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).context(firmware_path),
    };

    let config_path = format!("{dir}/config.rcvbp");
    let config = std::fs::metadata(&config_path)
        .is_ok()
        .then_some(config_path);

    anyhow::ensure!(
        firmware.is_some() || config.is_some(),
        "{dir} holds neither primary-region.bin nor config.rcvbp"
    );
    Ok(Snapshot { firmware, config })
}

/// Write a saved firmware image back into the primary bank.
///
/// This also wipes the configuration, because the configuration is stored
/// inside the region being written. Restore the configuration afterwards.
///
/// # Errors
/// Fails if the image is the wrong size or the card stops responding.
pub fn firmware(cli: &Cli, image_path: &str, commit: bool, index: u16, wait: u64) -> Result<()> {
    let img = std::fs::read(image_path).with_context(|| format!("read {image_path}"))?;
    let span = protocol::FIRMWARE_BLOCKS.len() * 64 * 1024;
    anyhow::ensure!(
        img.len() >= span,
        "{image_path} is {} bytes; the primary bank is {span}",
        img.len()
    );
    let img = &img[..span];

    println!("restoring the primary bank from {image_path}");
    println!(
        "  blocks 0x{:02x}..0x{:02x}; the golden bank at 0x{:02x} is not touched",
        protocol::FIRMWARE_BLOCKS.start,
        protocol::FIRMWARE_BLOCKS.end - 1,
        protocol::GOLDEN_BLOCK
    );
    println!("  note: this erases the configuration at 0x070000; restore it afterwards");
    if !commit {
        println!("\ndry run: nothing written. Re-run with --commit.");
        return Ok(());
    }

    let mut dev = open(cli)?;
    rewrite_block(&mut dev, index, img, wait, 0..0)?;
    println!("done; blocks 0x00-0x02 are write-protected and will not have changed");
    Ok(())
}

/// Put firmware, configuration and screen record back, in an order that leaves
/// the card whole.
///
/// # Errors
/// Fails if any stage fails; earlier stages are not rolled back, so read the
/// output to see how far it got.
pub fn all(cli: &Cli, dir: &str, commit: bool, index: u16, wait: u64) -> Result<()> {
    let snap = load_snapshot(dir)?;
    println!("restoring from {dir}");
    println!(
        "  firmware: {}",
        if snap.firmware.is_some() {
            "present"
        } else {
            "absent, skipping"
        }
    );
    println!(
        "  config:   {}",
        snap.config.as_deref().unwrap_or("absent, skipping")
    );
    if !commit {
        println!("\ndry run: nothing written. Re-run with --commit.");
        return Ok(());
    }

    // Firmware first: writing it erases the configuration region.
    if snap.firmware.is_some() {
        firmware(cli, &format!("{dir}/primary-region.bin"), true, index, wait)?;
    }
    // Then the configuration, which also restores the screen record.
    if let Some(config) = &snap.config {
        write_config(
            cli,
            config,
            true,
            &format!("{dir}/primary-region.bin"),
            snap.firmware
                .is_some()
                .then_some(&format!("{dir}/primary-region.bin")[..]),
            index,
            wait,
        )?;
    }
    println!("\nrestore complete; power-cycle the card");
    Ok(())
}

/// Capture everything we know how to restore into a directory.
///
/// # Errors
/// Fails if the card does not answer or the files cannot be written.
pub fn snapshot(cli: &Cli, dir: &str, index: u16, wait: u64) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("create {dir}"))?;
    let mut dev = open(cli)?;

    let blocks = protocol::FIRMWARE_BLOCKS.len() as u16;
    println!("reading the primary bank ({blocks} blocks)...");
    let primary = read_blocks(
        &mut dev,
        index,
        protocol::FIRMWARE_BLOCKS.start,
        blocks,
        wait,
    )?;
    let path = format!("{dir}/primary-region.bin");
    std::fs::write(&path, &primary).with_context(|| format!("write {path}"))?;
    println!("  {} bytes -> {path}", primary.len());

    println!("reading the golden bank...");
    let golden = read_blocks(&mut dev, index, protocol::GOLDEN_BLOCK, blocks, wait)?;
    let path = format!("{dir}/golden-bank.bin");
    std::fs::write(&path, &golden).with_context(|| format!("write {path}"))?;
    println!("  {} bytes -> {path}", golden.len());

    println!("snapshot written to {dir}");
    println!("capture the configuration too:  e120 read-config --out {dir}/config.rcvbp");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("e120-restore-{name}"));
        std::fs::create_dir_all(&d).unwrap();
        d.to_str().unwrap().to_owned()
    }

    #[test]
    fn an_empty_directory_is_rejected() {
        let d = tmpdir("empty");
        let _ = std::fs::remove_file(format!("{d}/primary-region.bin"));
        let _ = std::fs::remove_file(format!("{d}/config.rcvbp"));
        assert!(load_snapshot(&d).is_err());
    }

    #[test]
    fn a_short_firmware_image_is_rejected() {
        let d = tmpdir("short");
        std::fs::write(format!("{d}/primary-region.bin"), vec![0u8; 1024]).unwrap();
        let err = load_snapshot(&d).unwrap_err().to_string();
        assert!(err.contains("needs at least"), "unexpected error: {err}");
        std::fs::remove_file(format!("{d}/primary-region.bin")).unwrap();
    }

    #[test]
    fn a_full_size_image_is_accepted() {
        let d = tmpdir("full");
        let span = protocol::FIRMWARE_BLOCKS.len() * 64 * 1024;
        std::fs::write(format!("{d}/primary-region.bin"), vec![0u8; span]).unwrap();
        let snap = load_snapshot(&d).unwrap();
        assert!(snap.firmware.is_some());
        assert!(snap.config.is_none());
        std::fs::remove_file(format!("{d}/primary-region.bin")).unwrap();
    }
}
