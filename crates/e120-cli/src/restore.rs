//! Snapshots of the card's flash, and restoring the configuration from one.
//!
//! `snapshot` saves the primary bank and the golden bank; `all` puts the
//! `.rcvbp` configuration (and with it the screen-size record) back. Firmware
//! is not restored here: 16.53 guards blocks 0-2 and 8 from the host path, so
//! a firmware image goes in through `upgrade install` plus `flash-firmware`
//! (`provision --firmware` does both).

use crate::flash::{read_blocks, write_config};
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

/// Put the configuration and screen record back from a snapshot.
///
/// # Errors
/// Fails if the snapshot holds no `config.rcvbp` or the write does not verify.
pub fn all(cli: &Cli, dir: &str, commit: bool, index: u16, wait: u64) -> Result<()> {
    let snap = load_snapshot(dir)?;
    println!("restoring from {dir}");
    if snap.firmware.is_some() {
        println!(
            "  firmware: primary-region.bin present but NOT restored by this command; \
             host-writable blocks go back with\n    e120 flash-firmware {dir}/primary-region.bin \
             --backup <fresh dump> --from-block 3 --to-block 7 --commit"
        );
    }
    let Some(config) = &snap.config else {
        anyhow::bail!("{dir} holds no config.rcvbp; nothing this command can restore");
    };
    println!("  config:   {config}");
    if !commit {
        println!("\ndry run: nothing written. Re-run with --commit.");
        return Ok(());
    }

    // write_config reads the block off the card and restores the screen record.
    let backup = format!("{dir}/block07-before-restore.bin");
    write_config(cli, config, true, &backup, None, index, wait)?;
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
