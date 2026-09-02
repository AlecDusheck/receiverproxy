//! Installing firmware by the route the card supports: upload the whole image
//! into SDRAM, then ask the card to erase and program itself.
//!
//! Direct writes to the program area are silently ignored, so this is the
//! only path.

use crate::util::{await_any_frame, has_lattice_header, open};
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
    await_any_frame(
        &mut dev,
        Duration::from_secs(wait),
        upgrade::parse_descriptor,
    )?
    .with_context(|| format!("no firmware descriptor from the card within {wait}s"))
}

/// Print what the card reports.
///
/// # Errors
/// Fails if the card does not answer.
pub fn info(cli: &Cli, wait: u64) -> Result<()> {
    let d = describe(cli, wait)?;
    println!("image start     0x{:06x}", d.start);
    println!(
        "image length    0x{:06x} ({} bytes)",
        d.image_len, d.image_len
    );
    println!(
        "file length     0x{:06x} ({} bytes)",
        d.file_len, d.file_len
    );
    println!("chunks          {}", d.chunks());
    println!("flash op type   0x{:02x}", d.flash_op_type);
    println!("sdram staging   {}", d.supports_sdram());
    println!("golden bank     {}", d.has_golden());
    println!("partition sel   {}", d.supports_select_part());
    println!("golden upgrade  {}", d.supports_golden_upgrade());
    Ok(())
}

/// Install a firmware image through SDRAM staging and wait for the card to
/// report done.
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
        has_lattice_header(&img),
        "{image_path} does not look like a Lattice bitstream"
    );

    let d = describe(cli, wait)?;
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
    eprintln!(
        "upgrade: {image_path} -> {} image, {} chunks of {} bytes {chunk_delay_us}us apart, ~{:.1}s to program",
        match partition {
            Partition::Primary => "primary",
            Partition::Golden => "golden",
        },
        d.chunks(),
        upgrade::CHUNK,
        d.estimated_ms() as f64 / 1000.0
    );
    if !commit {
        println!("dry run: nothing sent (add --commit)");
        return Ok(());
    }

    let mut dev = open(cli)?;
    let sel = protocol::BROADCAST;

    for (n, chunk) in staged.chunks(upgrade::CHUNK).enumerate() {
        let offset = (n * upgrade::CHUNK) as u32;
        dev.send(&upgrade::sdram_chunk(sel, offset, chunk))?;
        // Chunks are not acknowledged; pacing is the only flow control. Sent
        // too fast, runs of chunks drop silently and stale SDRAM gets programmed.
        std::thread::sleep(Duration::from_micros(chunk_delay_us));
        if n.is_multiple_of(128) {
            eprintln!("upgrade: chunk {n}/{}", d.chunks());
        }
    }
    std::thread::sleep(Duration::from_millis(1));

    dev.send(&upgrade::sdram_erase(sel, partition, d.image_len))?;
    std::thread::sleep(Duration::from_millis(1));

    dev.send(&upgrade::sdram_program(sel, partition, d.image_len))?;
    std::thread::sleep(Duration::from_millis(1));

    eprintln!("upgrade: programming, do not power off");
    std::thread::sleep(Duration::from_millis(d.first_poll_ms()));

    let deadline = Instant::now() + Duration::from_secs(timeout_s);
    let mut polls = 0u32;
    while Instant::now() < deadline {
        dev.send(&protocol::upgrade_info())?;
        let done = await_any_frame(&mut dev, Duration::from_millis(600), |f| {
            upgrade::programming_finished(f).then_some(())
        })?;
        if done.is_some() {
            eprintln!("upgrade: programming complete; power-cycle the card to load it");
            return Ok(());
        }
        polls += 1;
        if polls.is_multiple_of(5) {
            eprintln!("upgrade: still programming ({polls}s)");
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    anyhow::bail!(
        "no completion report within {timeout_s}s; the card may still be programming, \
         do not power it off; check with: e120 firmware info"
    )
}
