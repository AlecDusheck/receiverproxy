//! Snapshots of the card's flash, and restoring the configuration from one.
//!
//! `flash snapshot` saves the primary bank and the golden bank; `flash restore`
//! puts the `.rcvbp` configuration (and with it the screen-size record) back.
//! Firmware is not restored here: the firmware guards blocks from the host
//! path (`config/cards/*.toml`, `memory.guarded`), so a firmware image goes
//! in through `firmware install` plus `firmware write` (`provision
//! --firmware` does both).

use crate::flash::{read_blocks, read_primary_bank, write_config};
use crate::util::{open, warn};
use crate::{Ctx, Progress};
use anyhow::{Context, Result};

/// A saved copy of everything we know how to put back.
#[derive(Debug)]
pub struct Snapshot {
    pub firmware: Option<String>,
    pub config: Option<String>,
}

/// Load whatever a snapshot directory holds. The bank image's size is
/// checked by `flash_firmware` when it is used as the recovery copy.
///
/// # Errors
/// Fails if a file is present but unreadable, or neither is there.
pub fn load_snapshot(dir: &str) -> Result<Snapshot> {
    let firmware_path = format!("{dir}/primary-region.bin");
    let firmware = match std::fs::metadata(&firmware_path) {
        Ok(_) => Some(firmware_path),
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
pub fn all(
    ctx: &Ctx,
    dir: &str,
    commit: bool,
    index: u16,
    wait: u64,
    p: &mut dyn Progress,
) -> Result<()> {
    let snap = load_snapshot(dir)?;
    if snap.firmware.is_some() {
        warn(p, format!(
            "{dir}/primary-region.bin is not restored by this command; host-writable blocks go back with: \
             rxp firmware write {dir}/primary-region.bin --backup <fresh dump> --from-block 3 --to-block 7 --commit"
        ));
    }
    let Some(config) = &snap.config else {
        anyhow::bail!("{dir} holds no config.rcvbp; nothing this command can restore");
    };
    if !commit {
        p.out(&format!(
            "dry run: {config} -> parameter block (add --commit)"
        ));
        return Ok(());
    }

    // write_config reads the block off the card and restores the screen record.
    let backup = format!("{dir}/block07-before-restore.bin");
    write_config(ctx, config, true, &backup, None, index, wait, p)
}

/// Capture everything we know how to restore into a directory.
///
/// # Errors
/// Fails if the card does not answer or the files cannot be written.
pub fn snapshot(ctx: &Ctx, dir: &str, index: u16, wait: u64, p: &mut dyn Progress) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("create {dir}"))?;
    let m = ctx.model()?;
    let mut dev = open(ctx)?;

    let blocks = u16::from(m.memory.bank_blocks());
    let primary = read_primary_bank(m, &mut dev, index, wait, p)?;
    let path = format!("{dir}/primary-region.bin");
    std::fs::write(&path, &primary).with_context(|| format!("write {path}"))?;
    p.out(&path);

    let golden = read_blocks(&mut dev, index, m.memory.golden_block(), blocks, wait, p)?;
    let path = format!("{dir}/golden-bank.bin");
    std::fs::write(&path, &golden).with_context(|| format!("write {path}"))?;
    p.out(&path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("rxp-restore-{name}"));
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
    fn a_bank_image_is_listed() {
        let d = tmpdir("full");
        std::fs::write(format!("{d}/primary-region.bin"), vec![0u8; 0x1000]).unwrap();
        let snap = load_snapshot(&d).unwrap();
        assert!(snap.firmware.is_some());
        assert!(snap.config.is_none());
        std::fs::remove_file(format!("{d}/primary-region.bin")).unwrap();
    }
}
