//! The screen-size record: a 256-byte EEPROM-backed record at a linear flash
//! address that the page-addressed frames cannot reach.
//!
//! A block erase clears it and a firmware write never restores it, so it is
//! read and set by value. The other `card` commands, one frame each, are
//! here too.

use crate::model::flash_map;
use crate::util::{await_reply, open};
use crate::{protocol, Ctx, Progress};
use anyhow::{Context, Result};
use rawlink::Link;
use receivers::CardModel;
use std::time::Duration;

/// Byte offsets of the geometry fields within the record.
const WIDTH: usize = 6;
const HEIGHT: usize = 8;

/// Read the record off the card.
///
/// # Errors
/// Fails if the card does not answer.
pub fn read(m: &CardModel, dev: &mut Link, index: u16, wait: u64) -> Result<Vec<u8>> {
    // The card answers the unrestricted linear read at the EEPROM mirror.
    dev.send(&flash_map(m).read_screen_record(index))?;
    // Linear reads answer with a different type than the page-addressed
    // reads, so take any reply long enough to hold a record.
    await_reply(dev, Duration::from_secs(wait), |f| {
        f.get(15..15 + protocol::SCREEN_RECORD_LEN)
            .map(<[u8]>::to_vec)
    })?
    .with_context(|| format!("no screen-size record from the card within {wait}s"))
}

/// Offsets of the receiver's control area within the record: the rectangle
/// `(startX, startY) -> (endX, endY)` the card windows incoming pixels
/// against. `endX`/`endY` are the geometry fields above.
const START_X: usize = 2;
const START_Y: usize = 4;

/// True when the record has been erased rather than programmed.
///
/// The write path sends all 256 bytes, i.e. every EEPROM record
/// (docs/eeprom-map.md), so an erased read must never be written back: it
/// would persist `0xFF` across the control area and the card would drop
/// every pixel (docs/receiver-identity.md).
#[must_use]
pub fn looks_erased(record: &[u8]) -> bool {
    let empty_window =
        |o: usize| matches!((record.get(o), record.get(o + 1)), (Some(0xFF), Some(0xFF)));
    empty_window(START_X)
        || empty_window(START_Y)
        || record.iter().fold(0, |n, &b| n + usize::from(b == 0xFF)) > record.len() / 2
}

/// Geometry encoded in a record.
#[must_use]
pub fn geometry(record: &[u8]) -> Option<(u16, u16)> {
    let be16 = |o| {
        record
            .get(o..o + 2)
            .and_then(|s| s.try_into().ok())
            .map(u16::from_be_bytes)
    };
    Some((be16(WIDTH)?, be16(HEIGHT)?))
}

/// Show the record, and optionally set the geometry it carries. Returns the
/// geometry the card holds when the command finishes.
///
/// # Errors
/// Fails if the card does not answer or the write is refused.
pub fn screen_size(
    ctx: &Ctx,
    set: Option<(u16, u16)>,
    commit: bool,
    index: u16,
    wait: u64,
    p: &mut dyn Progress,
) -> Result<(u16, u16)> {
    let m = ctx.model()?;
    let mut dev = open(ctx)?;
    let record = read(m, &mut dev, index, wait)?;
    let (w, h) = geometry(&record).context("the record is too short to hold a geometry")?;

    let Some((nw, nh)) = set else {
        p.out(&format!("{w}x{h}"));
        return Ok((w, h));
    };
    if (nw, nh) == (w, h) {
        p.out(&format!("{w}x{h}"));
        return Ok((w, h));
    }
    if looks_erased(&record) {
        let sx = u16::from_be_bytes([record[START_X], record[START_X + 1]]);
        let sy = u16::from_be_bytes([record[START_Y], record[START_Y + 1]]);
        anyhow::bail!(
            "EEPROM record reads as erased (control area starts at {sx},{sy}); \
             writing it back would persist 0xFF across every record in it \
             (docs/eeprom-map.md); restore it first: \
             python3 scripts/eeprom-restore.py --commit"
        );
    }
    if !commit {
        p.out(&format!("{w}x{h} -> {nw}x{nh} (dry run; add --commit)"));
        return Ok((w, h));
    }

    let mut updated = record;
    updated[WIDTH..WIDTH + 2].copy_from_slice(&nw.to_be_bytes());
    updated[HEIGHT..HEIGHT + 2].copy_from_slice(&nh.to_be_bytes());
    let map = flash_map(m);
    dev.send(&map.write_screen_record(index, map.screen_record_addr, &updated)?)?;
    std::thread::sleep(Duration::from_millis(200));

    let after = read(m, &mut dev, index, wait)?;
    let got = match geometry(&after) {
        Some((aw, ah)) if (aw, ah) == (nw, nh) => {
            p.out(&format!("{aw}x{ah}"));
            (aw, ah)
        }
        Some((aw, ah)) => anyhow::bail!("wrote {nw}x{nh} but the card reads back {aw}x{ah}"),
        None => anyhow::bail!("the card returned an unreadable record"),
    };
    p.err("power-cycle the card to apply");
    Ok(got)
}

/// Ask the card to reload its parameters from flash; `full` sends the
/// vendor's post-save frame instead of the bare reload.
///
/// # Errors
/// Fails if the link cannot be opened.
pub fn reload(ctx: &Ctx, index: u16, full: bool) -> Result<()> {
    let mut dev = open(ctx)?;
    if full {
        dev.send(&protocol::reload_params_full(index))?;
    } else {
        dev.send(&protocol::reload_params(index))?;
    }
    Ok(())
}

/// Select the card's built-in test pattern; 0 is off.
///
/// # Errors
/// Fails if the link cannot be opened.
pub fn test_mode(ctx: &Ctx, index: u16, pattern: u8) -> Result<()> {
    let mut dev = open(ctx)?;
    dev.send(&protocol::test_mode(index, pattern))?;
    Ok(())
}

/// Step through the card's test patterns, pausing on each, then switch off.
///
/// # Errors
/// Fails if the link cannot be opened.
pub fn test_sweep(ctx: &Ctx, count: u8, secs: u64, index: u16, p: &mut dyn Progress) -> Result<()> {
    let mut dev = open(ctx)?;
    for pattern in 0..count {
        p.out(&format!("pattern {pattern}"));
        dev.send(&protocol::test_mode(index, pattern))?;
        std::thread::sleep(Duration::from_secs(secs));
    }
    dev.send(&protocol::test_mode(index, 0))?;
    Ok(())
}

/// Tell the card its own size and the size of the whole screen.
///
/// # Errors
/// Fails if the link cannot be opened.
pub fn set_layout(ctx: &Ctx, index: u16, panel_width: u16, panel_height: u16) -> Result<()> {
    let mut dev = open(ctx)?;
    dev.send(&protocol::set_layout(
        index,
        panel_width,
        panel_height,
        0,
        0,
        panel_width,
        panel_height,
    ))?;
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

    #[test]
    fn an_erased_record_is_recognised_before_it_can_be_written_back() {
        // The exact shape the card was left in: geometry restored, control
        // area still erased. `discover` reports a healthy 128x64 in this
        // state, so the geometry fields alone cannot be the check.
        let mut r = vec![0u8; protocol::SCREEN_RECORD_LEN];
        r[START_X..START_X + 4].copy_from_slice(&[0xFF; 4]);
        r[WIDTH..WIDTH + 2].copy_from_slice(&128u16.to_be_bytes());
        r[HEIGHT..HEIGHT + 2].copy_from_slice(&64u16.to_be_bytes());
        assert_eq!(geometry(&r), Some((128, 64)), "geometry still reads fine");
        assert!(looks_erased(&r), "but the record must not be written back");
    }

    #[test]
    fn a_wholly_erased_record_is_recognised() {
        assert!(looks_erased(&[0xFFu8; protocol::SCREEN_RECORD_LEN]));
    }

    #[test]
    fn the_factory_record_is_accepted() {
        let mut r = vec![0u8; protocol::SCREEN_RECORD_LEN];
        r[WIDTH..WIDTH + 2].copy_from_slice(&128u16.to_be_bytes());
        r[HEIGHT..HEIGHT + 2].copy_from_slice(&64u16.to_be_bytes());
        assert!(!looks_erased(&r));
    }
}
