//! Installing firmware, by the route the card actually supports.
//!
//! The host does not write the firmware region. It uploads a whole image into
//! the card's SDRAM, then asks the card to erase and program itself. Direct
//! writes to the program area are silently ignored no matter what we send, so
//! this is the only path that works.

use crate::util::open;
use crate::{protocol, Cli};
use anyhow::{Context, Result};
use protocol::upgrade::{self, Descriptor, Partition};
use std::time::{Duration, Instant};

/// Ask the card what image it expects and how it can be upgraded.
///
/// # Errors
/// Fails if the card does not answer or the reply cannot be decoded.
pub fn describe(cli: &Cli, wait: u64) -> Result<Descriptor> {
    let mut dev = open(cli)?;
    dev.send(&protocol::upgrade_info())?;
    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        for f in dev.recv()? {
            if let Some(d) = upgrade::parse_descriptor(&f) {
                return Ok(d);
            }
        }
    }
    anyhow::bail!("the card did not describe its firmware within {wait}s")
}

/// Print what the card reports.
///
/// # Errors
/// Fails if the card does not answer.
pub fn info(cli: &Cli, wait: u64) -> Result<()> {
    let d = describe(cli, wait)?;
    println!("the card expects:");
    println!("  image start   0x{:06x}", d.start);
    println!(
        "  image length  0x{:06x} ({} bytes)",
        d.image_len, d.image_len
    );
    println!(
        "  file length   0x{:06x} ({} bytes)",
        d.file_len, d.file_len
    );
    println!("  chunks        {}", d.chunks());
    println!("  flash op type 0x{:02x}", d.flash_op_type);
    println!("capabilities:");
    println!("  stages via SDRAM        {}", d.supports_sdram());
    println!("  has a golden bank       {}", d.has_golden());
    println!("  accepts partition sel   {}", d.supports_select_part());
    println!("  golden upgrade allowed  {}", d.supports_golden_upgrade());
    println!(
        "\nupgrade path: {}",
        if d.supports_sdram() {
            "SDRAM staging — the card programs itself"
        } else {
            "direct flash writes from the host"
        }
    );
    Ok(())
}

/// Install a firmware image.
///
/// Uploads the image into the card's SDRAM, then asks it to erase and program
/// itself, then waits for it to report done.
///
/// # Errors
/// Fails if the image does not match what the card expects, if the card does
/// not support SDRAM staging, or if programming does not complete in time.
pub fn install(
    cli: &Cli,
    image_path: &str,
    commit: bool,
    partition: Partition,
    timeout_s: u64,
    chunk_delay_us: u64,
    wait: u64,
) -> Result<()> {
    let img = std::fs::read(image_path).with_context(|| format!("read {image_path}"))?;
    anyhow::ensure!(
        img.windows(21)
            .take(256)
            .any(|w| w == b"Lattice Semiconductor"),
        "{image_path} does not look like a Lattice bitstream"
    );

    let d = describe(cli, wait)?;
    println!("card expects {} bytes, file is {}", d.file_len, img.len());
    anyhow::ensure!(
        img.len() as u32 == d.file_len,
        "{image_path} is {} bytes but the card expects exactly {}",
        img.len(),
        d.file_len
    );
    anyhow::ensure!(
        d.supports_sdram(),
        "this card does not stage via SDRAM, and the direct-write path is not implemented"
    );
    if partition == Partition::Golden {
        anyhow::ensure!(
            d.supports_golden_upgrade(),
            "this card does not accept upgrades aimed at the golden bank"
        );
    }

    let staged = &img[..d.image_len as usize];
    println!(
        "installing {image_path} into the {} image",
        match partition {
            Partition::Primary => "primary",
            Partition::Golden => "golden",
        }
    );
    println!(
        "  {} chunks of {} bytes, {chunk_delay_us}us apart",
        d.chunks(),
        upgrade::CHUNK
    );
    println!(
        "  the card estimates {:.1}s to program",
        d.estimated_ms() as f64 / 1000.0
    );
    if !commit {
        println!("\ndry run: nothing sent. Re-run with --commit to install.");
        return Ok(());
    }

    let mut dev = open(cli)?;
    let sel = protocol::BROADCAST;

    println!("uploading into SDRAM...");
    for (n, chunk) in staged.chunks(upgrade::CHUNK).enumerate() {
        let offset = (n * upgrade::CHUNK) as u32;
        dev.send(&upgrade::sdram_chunk(sel, offset, chunk))?;
        // Nothing is acknowledged, so pacing is the only flow control there
        // is. Sending too fast overruns the card and whole runs of chunks are
        // dropped silently, leaving stale SDRAM that then gets programmed.
        std::thread::sleep(Duration::from_micros(chunk_delay_us));
        if n.is_multiple_of(128) {
            println!("  {n} / {} chunks", d.chunks());
        }
    }
    std::thread::sleep(Duration::from_millis(1));

    println!("asking the card to erase...");
    dev.send(&upgrade::sdram_erase(sel, partition, d.image_len))?;
    std::thread::sleep(Duration::from_millis(1));

    println!("asking the card to program from SDRAM...");
    dev.send(&upgrade::sdram_program(sel, partition, d.image_len))?;
    std::thread::sleep(Duration::from_millis(1));

    // The card is now writing its own flash. Do not interrupt it.
    println!("waiting for the card to finish (do not power off)...");
    std::thread::sleep(Duration::from_millis(d.first_poll_ms()));

    let deadline = Instant::now() + Duration::from_secs(timeout_s);
    let mut polls = 0u32;
    while Instant::now() < deadline {
        dev.send(&protocol::upgrade_info())?;
        let until = Instant::now() + Duration::from_millis(600);
        while Instant::now() < until {
            for f in dev.recv()? {
                if upgrade::programming_finished(&f) {
                    println!("the card reports programming complete");
                    println!("power-cycle it to load the new firmware");
                    return Ok(());
                }
            }
        }
        polls += 1;
        if polls.is_multiple_of(5) {
            println!("  still programming ({polls}s)");
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    anyhow::bail!(
        "the card did not report completion within {timeout_s}s. \
         It may still be programming — do NOT power it off. Re-run \
         `e120 upgrade info` to check whether it is responsive."
    )
}
