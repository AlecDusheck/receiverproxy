//! The screen-size record.
//!
//! The card keeps its panel geometry in a 256-byte record that the ordinary
//! page-addressed flash frames cannot reach — it answers at a linear address
//! but is backed by a small EEPROM, so a block erase clears it and a firmware
//! write never restores it.
//!
//! It is read and set here directly, by value. Earlier code recovered it by
//! slicing the same offset out of a saved flash image, which silently produces
//! nonsense whenever that image is a firmware bitstream rather than a real
//! config block.

use crate::util::{hexdump, is_card_frame, open};
use crate::{protocol, Cli};
use anyhow::{Context, Result};
use e120_net::bpf;
use std::time::{Duration, Instant};

/// Byte offsets of the geometry fields within the record.
const WIDTH: usize = 6;
const HEIGHT: usize = 8;

/// Read the record off the card.
///
/// # Errors
/// Fails if the card does not answer.
pub fn read(dev: &mut bpf::Bpf, index: u16, wait: u64) -> Result<Vec<u8>> {
    // The card answers the unrestricted linear read here; the guarded
    // screen-record read frame goes unanswered because of its length field.
    dev.send(&protocol::read_flash_linear(
        index,
        protocol::SCREEN_RECORD_ADDR,
        protocol::SCREEN_RECORD_LEN as u32,
    ))?;
    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        for f in dev.recv()? {
            if !is_card_frame(&f) {
                continue;
            }
            // Linear reads answer with a different type than the page-addressed
            // reads, so take any reply long enough to hold a record.
            if f.len() >= 15 + protocol::SCREEN_RECORD_LEN {
                return Ok(f[15..15 + protocol::SCREEN_RECORD_LEN].to_vec());
            }
        }
    }
    anyhow::bail!("the card did not return its screen-size record within {wait}s")
}

/// Geometry encoded in a record.
#[must_use]
pub fn geometry(record: &[u8]) -> Option<(u16, u16)> {
    Some((
        u16::from_be_bytes([*record.get(WIDTH)?, *record.get(WIDTH + 1)?]),
        u16::from_be_bytes([*record.get(HEIGHT)?, *record.get(HEIGHT + 1)?]),
    ))
}

/// Show the record, and optionally set the geometry it carries.
///
/// # Errors
/// Fails if the card does not answer or the write is refused.
pub fn screen_size(
    cli: &Cli,
    set: Option<(u16, u16)>,
    commit: bool,
    index: u16,
    wait: u64,
) -> Result<()> {
    let mut dev = open(cli)?;
    let record = read(&mut dev, index, wait)?;
    let (w, h) = geometry(&record).context("the record is too short to hold a geometry")?;
    println!("screen-size record at 0x{:06x}:", protocol::SCREEN_RECORD_ADDR);
    println!("  geometry {w}x{h}");
    hexdump(&record[..32]);

    let Some((nw, nh)) = set else {
        return Ok(());
    };
    if (nw, nh) == (w, h) {
        println!("\nalready {nw}x{nh}; nothing to write");
        return Ok(());
    }
    println!("\nsetting geometry to {nw}x{nh}");
    if !commit {
        println!("dry run: nothing written. Re-run with --commit.");
        return Ok(());
    }

    let mut updated = record;
    updated[WIDTH..WIDTH + 2].copy_from_slice(&nw.to_be_bytes());
    updated[HEIGHT..HEIGHT + 2].copy_from_slice(&nh.to_be_bytes());
    dev.send(&protocol::write_screen_record(
        index,
        protocol::SCREEN_RECORD_ADDR,
        &updated,
    )?)?;
    std::thread::sleep(Duration::from_millis(200));

    let after = read(&mut dev, index, wait)?;
    match geometry(&after) {
        Some((aw, ah)) if (aw, ah) == (nw, nh) => println!("verified: the card reports {aw}x{ah}"),
        Some((aw, ah)) => anyhow::bail!("wrote {nw}x{nh} but the card reads back {aw}x{ah}"),
        None => anyhow::bail!("the card returned an unreadable record"),
    }
    println!("power-cycle the card for it to take effect");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_is_read_big_endian_from_the_documented_offsets() {
        let mut r = vec![0u8; protocol::SCREEN_RECORD_LEN];
        r[WIDTH] = 0x00;
        r[WIDTH + 1] = 0x80;
        r[HEIGHT] = 0x00;
        r[HEIGHT + 1] = 0x40;
        assert_eq!(geometry(&r), Some((128, 64)));
    }

    #[test]
    fn a_short_record_has_no_geometry() {
        assert_eq!(geometry(&[0u8; 4]), None);
    }
}
