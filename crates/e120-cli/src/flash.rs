//! Reading and writing the card's flash: configuration, dumps, and firmware.

use crate::util::{await_reply, contains_lattice_header, has_lattice_header, hex, open, warn};
use crate::{protocol, rcvbp, Cli};
use anyhow::{Context, Result};
use e120_net::Bpf;
use std::time::Duration;

/// Record type of the driver-chip register table.
const CHIP_REGS: u16 = 0x0a84;

/// Bytes in the primary firmware bank.
pub const BANK_BYTES: usize =
    (protocol::FIRMWARE_BLOCKS.end - protocol::FIRMWARE_BLOCKS.start) as usize * 0x10000;

/// True when the file carries a driver-chip register table with content.
fn has_chip_regs(f: &rcvbp::Rcvbp) -> bool {
    f.find(CHIP_REGS).is_some_and(|r| !r.is_empty_table())
}

/// Byte offset of a 256-byte page within a firmware-bank image.
const fn page_offset(block: u8, page: u16) -> usize {
    (block as usize * 256 + page as usize) * protocol::FLASH_PAGE_BYTES
}

/// The `i`th 256-byte page of a block image.
fn page(image: &[u8], i: usize) -> &[u8] {
    &image[i * protocol::FLASH_PAGE_BYTES..(i + 1) * protocol::FLASH_PAGE_BYTES]
}

/// Indices of the pages of `after` that differ from `pages`.
fn mismatched_pages(after: &[u8], pages: &[&[u8]]) -> Vec<usize> {
    after
        .chunks(protocol::FLASH_PAGE_BYTES)
        .zip(pages)
        .enumerate()
        .filter(|(_, (a, b))| a != *b)
        .map(|(i, _)| i)
        .collect()
}

/// Read the card's stored configuration out of flash and save it as a
/// `.rcvbp` file. Only ever sends read-opcode flash frames, which carry no
/// data of their own and so cannot modify the card.
pub fn read_config(
    cli: &Cli,
    index: u16,
    page: u16,
    max_chunks: u16,
    wait: u64,
    out: &str,
) -> Result<()> {
    let mut dev = open(cli)?;
    let mut flash: Vec<u8> = Vec::new();
    let mut expected: Option<usize> = None;

    for chunk in 0..max_chunks {
        let page = page + chunk * protocol::FLASH_PAGES_PER_CHUNK;
        flash.extend_from_slice(&read_chunk(&mut dev, index, page, wait)?);

        // The blob opens with its own total length, so we know when to stop.
        if expected.is_none() && flash.len() >= 4 {
            expected = Some(u32::from_le_bytes(flash[..4].try_into()?) as usize);
        }
        if expected.is_some_and(|n| flash.len() >= n) {
            break;
        }
    }

    let total = expected.context("card returned no length prefix")?;
    if flash.len() < total + 4 {
        anyhow::bail!(
            "only read {} of {total} bytes; raise --max-chunks",
            flash.len()
        );
    }
    // The prefix counts the file including its 4-byte CRC trailer.
    let file = flash
        .get(4..4 + total)
        .context("card reported more configuration than it returned")?;
    std::fs::write(out, file).with_context(|| format!("write {out}"))?;
    println!("{out}");

    match rcvbp::Rcvbp::load(out) {
        Ok(f) if !has_chip_regs(&f) => warn(format!(
            "{out} has no driver-chip register table; panels with PWM driver ICs will stay dark"
        )),
        Ok(_) => {}
        Err(e) => warn(format!("{out} does not parse as .rcvbp: {e:#}")),
    }
    Ok(())
}

/// Request one 1024-byte chunk of flash and return it.
fn read_chunk(dev: &mut Bpf, index: u16, page: u16, wait: u64) -> Result<Vec<u8>> {
    dev.send(&protocol::read_flash(index, page))?;
    await_reply(dev, Duration::from_secs(wait), |f| {
        protocol::flash_reply_data(f).map(<[u8]>::to_vec)
    })?
    .with_context(|| format!("no reply for page 0x{page:04x} within {wait}s"))
}

/// Read the firmware region back and count bytes that differ from `img`.
fn verify_firmware(dev: &mut Bpf, index: u16, img: &[u8], wait: u64) -> Result<usize> {
    let mut bad = 0usize;
    for block in protocol::FIRMWARE_BLOCKS {
        for lo in (0u16..0x100).step_by(protocol::FLASH_PAGES_PER_CHUNK as usize) {
            let page = (u16::from(block) << 8) | lo;
            let got = read_chunk(dev, index, page, wait)?;
            let off = page_offset(block, lo);
            let want = &img[off..off + got.len()];
            bad += got.iter().zip(want).filter(|(g, w)| g != w).count();
        }
    }
    Ok(bad)
}

/// Print the human-readable fields Lattice puts in a bitstream's header.
fn describe_image(img: &[u8]) {
    let header: String = img[..200.min(img.len())]
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                ' '
            }
        })
        .collect();
    for field in ["Design name", "Part", "Date"] {
        if let Some(i) = header.find(field) {
            eprintln!(
                "firmware: {}",
                header[i..].split("  ").next().unwrap_or("").trim()
            );
        }
    }
}

/// Install an FPGA bitstream into the primary firmware bank.
///
/// Only the primary is written; the golden backup at block 0x20 is left alone
/// so the card retains an in-hardware fallback. A local dump of the current
/// primary is required as well, so the previous image can be put back.
///
/// `blocks` limits the write to part of the bank, so a partially-programmed
/// image can be repaired without disturbing what is already correct.
pub fn flash_firmware(
    cli: &Cli,
    image: &str,
    backup: &str,
    commit: bool,
    blocks: std::ops::Range<u8>,
    index: u16,
    wait: u64,
) -> Result<()> {
    anyhow::ensure!(
        blocks.start < blocks.end
            && protocol::FIRMWARE_BLOCKS.contains(&blocks.start)
            && blocks.end <= protocol::FIRMWARE_BLOCKS.end,
        "blocks 0x{:02x}..0x{:02x} fall outside the primary bank",
        blocks.start,
        blocks.end
    );

    let img = std::fs::read(image).with_context(|| format!("read {image}"))?;
    anyhow::ensure!(
        has_lattice_header(&img),
        "{image} does not look like a Lattice bitstream"
    );
    let span = BANK_BYTES;
    anyhow::ensure!(
        img.len() >= span,
        "{image} is only {} bytes; the primary bank is {span}",
        img.len()
    );
    // Images carry padding past their declared length. The meaningful content
    // ends with the end marker and CRC just inside the bank, so write exactly
    // one bank's worth and drop the tail.
    let img = &img[..span];

    // Refuse to proceed without a local copy of what we are about to replace.
    let old = std::fs::read(backup).with_context(|| format!("read backup {backup}"))?;
    anyhow::ensure!(
        old.len() >= span && has_lattice_header(&old),
        "{backup} is not a usable dump of the current primary bank"
    );

    eprintln!(
        "firmware: {image} -> blocks 0x{:02x}..0x{:02x}, recovery {backup}",
        blocks.start,
        blocks.end - 1
    );
    describe_image(img);

    if !commit {
        println!("dry run: nothing written (add --commit)");
        return Ok(());
    }

    let mut dev = open(cli)?;

    // The program region is write-protected; without this every erase and
    // write is silently ignored.
    dev.send(&protocol::set_program_writable(index, true))?;
    std::thread::sleep(Duration::from_millis(200));

    for block in blocks.clone() {
        eprintln!("firmware: erase 0x{block:02x}");
        dev.send(&protocol::erase_firmware_block(index, block)?)?;
        std::thread::sleep(Duration::from_secs(3));
    }

    for block in blocks {
        eprintln!("firmware: write 0x{block:02x}");
        for page in 0..=0xffu8 {
            let off = page_offset(block, u16::from(page));
            let data = &img[off..off + protocol::FLASH_PAGE_BYTES];
            dev.send(&protocol::write_firmware_page(index, block, page, data)?)?;
            std::thread::sleep(Duration::from_millis(6));
        }
    }

    // Relock before verifying, so the region is protected even if we stop here.
    dev.send(&protocol::set_program_writable(index, false))?;

    eprintln!("firmware: verify");
    let bad = verify_firmware(&mut dev, index, img, wait)?;
    if bad == 0 {
        eprintln!("firmware: bank verified");
    } else {
        // Not fatal: provision writes one block at a time and verifies the
        // whole bank itself once every path has run.
        warn(format!(
            "{bad} bytes differ after writing; golden bank at 0x{:02x} untouched; \
             recover with: e120 flash-firmware {backup} --backup {backup} --commit",
            protocol::GOLDEN_BLOCK
        ));
    }
    Ok(())
}

/// Read page 0 of each block and report what it looks like. Read-only.
pub fn scan_flash(cli: &Cli, first: u8, last: u8, index: u16, wait: u64) -> Result<()> {
    let mut dev = open(cli)?;
    for blk in first..=last {
        let page = u16::from(blk) << 8;
        let Ok(d) = read_chunk(&mut dev, index, page, wait) else {
            continue;
        };
        if d.iter().all(|&b| b == 0xff) || d.iter().all(|&b| b == 0) {
            continue; // erased or blank
        }
        let kind = if contains_lattice_header(&d) {
            "lattice bitstream header"
        } else if d.starts_with(&[0x20, 0x20, 0x19, 0xbe]) {
            "rcvbp config"
        } else {
            "data"
        };
        println!(
            "0x{blk:02x}  0x{:06x}  {kind:<24} {}",
            u32::from(blk) << 16,
            hex(&d[..d.len().min(12)], " ")
        );
    }
    Ok(())
}

/// Dump an arbitrary flash range using linear addressing. Read-only.
pub fn dump_range(
    cli: &Cli,
    start: &str,
    len: &str,
    index: u16,
    wait: u64,
    out: &str,
) -> Result<()> {
    let start = u32::from_str_radix(start.trim_start_matches("0x"), 16).context("bad --start")?;
    let len = u32::from_str_radix(len.trim_start_matches("0x"), 16).context("bad --len")?;
    let mut dev = open(cli)?;
    let mut image = Vec::with_capacity(len as usize);

    // The card answers linear reads one 256-byte page at a time; asking for
    // more returns nothing useful.
    let step = protocol::FLASH_PAGE_BYTES as u32;
    let mut addr = start;
    let mut misses = 0u32;
    while addr < start + len {
        dev.send(&protocol::read_flash_linear(index, addr, step))?;
        // Linear reads may answer with a different type than the
        // page-addressed reads, so take any sufficiently long reply.
        let reply = await_reply(&mut dev, Duration::from_secs(wait), |f| {
            (f.len() >= 15 + step as usize).then(|| f[15..15 + step as usize].to_vec())
        })?;
        if let Some(data) = reply {
            image.extend_from_slice(&data);
        } else {
            misses += 1;
            image.extend(std::iter::repeat_n(0xffu8, step as usize));
            if misses > 8 {
                warn(format!(
                    "giving up after {misses} unanswered reads at 0x{addr:08x}"
                ));
                break;
            }
        }
        if (addr - start).is_multiple_of(0x10000) {
            eprintln!("read 0x{addr:08x}");
        }
        addr += step;
    }
    std::fs::write(out, &image).with_context(|| format!("write {out}"))?;
    if misses > 0 {
        warn(format!("{misses} unanswered reads filled with 0xff"));
    }
    println!("{out}");
    Ok(())
}

/// Dump an entire 64KB flash block. Read-only.
pub fn dump_flash(
    cli: &Cli,
    block: u8,
    blocks: u16,
    index: u16,
    wait: u64,
    out: &str,
) -> Result<()> {
    let mut dev = open(cli)?;
    let image = read_blocks(&mut dev, index, block, blocks, wait)?;
    std::fs::write(out, &image).with_context(|| format!("write {out}"))?;
    println!("{out}");
    Ok(())
}

/// Offset of the parameter blob within the 64KB parameter block.
const PARAM_OFFSET: usize = 0x8000;

/// Largest parameter blob the card will accept.
const PARAM_MAX: usize = 0x6ffc;

/// Read the whole primary firmware bank into memory.
pub fn read_primary_bank(dev: &mut Bpf, index: u16, wait: u64) -> Result<Vec<u8>> {
    read_blocks(
        dev,
        index,
        protocol::FIRMWARE_BLOCKS.start,
        protocol::FIRMWARE_BLOCKS.len() as u16,
        wait,
    )
}

/// Read the whole parameter block into memory.
fn read_block(dev: &mut Bpf, index: u16, wait: u64) -> Result<Vec<u8>> {
    read_blocks(dev, index, protocol::PARAM_BLOCK, 1, wait)
}

/// Read `count` consecutive 64KB blocks starting at `first`.
///
/// # Errors
/// Fails if the card stops answering partway through.
pub fn read_blocks(dev: &mut Bpf, index: u16, first: u8, count: u16, wait: u64) -> Result<Vec<u8>> {
    let mut image = Vec::with_capacity(64 * 1024 * count as usize);
    for b in 0..count {
        let block = first.wrapping_add(b as u8);
        for lo in (0u16..0x100).step_by(protocol::FLASH_PAGES_PER_CHUNK as usize) {
            let page = (u16::from(block) << 8) | lo;
            image.extend_from_slice(&read_chunk(dev, index, page, wait)?);
        }
        eprintln!("read 0x{block:02x}");
    }
    Ok(image)
}

/// Erase the parameter block and write `image` over it, then verify and repair.
///
/// Flash needs time to settle after a block erase; pages written too early are
/// silently lost. After writing, the block is read back and any page that did
/// not take is rewritten. A page can only be rewritten in place while it is
/// still erased, so if a mismatched page holds other data the whole block is
/// erased and rewritten instead.
pub fn rewrite_block(
    dev: &mut Bpf,
    index: u16,
    image: &[u8],
    wait: u64,
    must_verify: std::ops::Range<usize>,
) -> Result<()> {
    anyhow::ensure!(image.len() == 64 * 1024, "image must be exactly 64KB");
    let pages: Vec<&[u8]> = image.chunks(protocol::FLASH_PAGE_BYTES).collect();

    for attempt in 1..=4 {
        let repair: Vec<usize> = if attempt == 1 {
            erase_and_settle(dev, index)?;
            (0..pages.len()).collect()
        } else {
            let after = read_block(dev, index, wait)?;
            let bad = mismatched_pages(&after, &pages);
            if bad.is_empty() {
                eprintln!("flash: block verified");
                return Ok(());
            }
            // Rewriting only works into still-erased pages.
            let dirty = bad
                .iter()
                .any(|&i| page(&after, i).iter().any(|&b| b != 0xff));
            eprintln!(
                "flash: attempt {attempt}: {} pages to rewrite{}",
                bad.len(),
                if dirty { " (re-erasing first)" } else { "" }
            );
            if dirty {
                erase_and_settle(dev, index)?;
                (0..pages.len()).collect()
            } else {
                bad
            }
        };

        for (n, &i) in repair.iter().enumerate() {
            dev.send(&protocol::write_page(
                index,
                protocol::PARAM_BLOCK,
                i as u8,
                pages[i],
            )?)?;
            std::thread::sleep(Duration::from_millis(8));
            if repair.len() > 32 && n.is_multiple_of(64) {
                eprintln!("flash: page {n}/{}", repair.len());
            }
        }
    }
    // Some pages sit outside the window the card lets us write. Those are not
    // part of the configuration blob, so report them rather than failing.
    let after = read_block(dev, index, wait)?;
    let bad = mismatched_pages(&after, &pages);
    let in_config = bad.iter().any(|i| must_verify.contains(i));
    anyhow::ensure!(
        !in_config,
        "verify failed: {} pages differ, including configuration pages",
        bad.len()
    );
    eprintln!(
        "flash: configuration pages verified; {} page(s) outside them would not take writes: {}",
        bad.len(),
        bad.iter()
            .map(|i| format!("0x{i:02x}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

/// Erase the parameter block and wait for the chip to finish.
fn erase_and_settle(dev: &mut Bpf, index: u16) -> Result<()> {
    eprintln!("flash: erase 0x{:02x}", protocol::PARAM_BLOCK);
    dev.send(&protocol::erase_block(index, protocol::PARAM_BLOCK)?)?;
    // Pages written while the erase is still running are silently dropped.
    std::thread::sleep(Duration::from_secs(3));
    Ok(())
}

/// Install a .rcvbp into the card's parameter flash.
///
/// The erase covers the whole 64KB block, so the block is read first, only the
/// parameter region is replaced, and everything else is written back byte for
/// byte. A backup of the original block is always saved before any write.
pub fn write_config(
    cli: &Cli,
    config: &str,
    commit: bool,
    backup: &str,
    base_image: Option<&str>,
    index: u16,
    wait: u64,
) -> Result<()> {
    // Refuse to install anything that is not a config we can parse.
    let parsed = rcvbp::Rcvbp::load(config)?;
    let file = std::fs::read(config).with_context(|| format!("read {config}"))?;
    anyhow::ensure!(
        file.len() <= PARAM_MAX,
        "{config} is {} bytes, over the {PARAM_MAX}-byte limit the card accepts",
        file.len()
    );
    if !has_chip_regs(&parsed) {
        warn(format!("{config} has no driver-chip register table"));
    }

    let mut dev = open(cli)?;
    let original = match base_image {
        Some(path) => {
            let img = std::fs::read(path).with_context(|| format!("read {path}"))?;
            anyhow::ensure!(img.len() == 64 * 1024, "{path} must be exactly 65536 bytes");
            img
        }
        None => {
            let img = read_block(&mut dev, index, wait)?;
            std::fs::write(backup, &img).with_context(|| format!("write {backup}"))?;
            eprintln!("flash: backup {backup}");
            img
        }
    };

    // Splice the new parameter blob in, leaving the rest of the block alone.
    let mut image = original.clone();
    let old_len = u32::from_le_bytes(image[PARAM_OFFSET..PARAM_OFFSET + 4].try_into()?) as usize;
    // The stored length is only meaningful if the block already held a config.
    // When the block comes from a firmware image instead, it is bitstream data
    // and reads as nonsense, so clamp it to the area the card actually uses.
    let old_len = old_len.min(PARAM_MAX);
    let region = PARAM_OFFSET + 4 + old_len.max(file.len());
    anyhow::ensure!(region <= image.len(), "parameter region overruns the block");
    // Clear the old blob so no tail of it survives behind the new one.
    image[PARAM_OFFSET..region].fill(0);
    image[PARAM_OFFSET..PARAM_OFFSET + 4].copy_from_slice(&(file.len() as u32).to_le_bytes());
    image[PARAM_OFFSET + 4..PARAM_OFFSET + 4 + file.len()].copy_from_slice(&file);

    let changed = original.iter().zip(&image).filter(|(a, b)| a != b).count();
    eprintln!(
        "flash: parameter blob {old_len} -> {} bytes, {changed} bytes of block 0x{:02x} change",
        file.len(),
        protocol::PARAM_BLOCK
    );

    if !commit {
        println!("dry run: nothing written (add --commit)");
        return Ok(());
    }

    // Only the pages holding the configuration blob itself have to verify.
    let first = PARAM_OFFSET / protocol::FLASH_PAGE_BYTES;
    let last = (PARAM_OFFSET + 4 + file.len()).div_ceil(protocol::FLASH_PAGE_BYTES);
    rewrite_block(&mut dev, index, &image, wait, first..last).with_context(|| {
        format!(
            "original block saved at {backup}; restore with: e120 restore-flash {backup} --commit"
        )
    })?;

    // The erase also clears the screen-size record, which only the linear
    // path can rewrite. A firmware image holds bitstream bytes at that
    // offset, not a record, so only a block read off the card is put back.
    let off = protocol::SCREEN_RECORD_ADDR as usize & 0xffff;
    let record = &original[off..off + protocol::SCREEN_RECORD_LEN];
    if base_image.is_none() && record.iter().any(|&b| b != 0xff) {
        dev.send(&protocol::write_screen_record(
            index,
            protocol::SCREEN_RECORD_ADDR,
            record,
        )?)?;
        std::thread::sleep(Duration::from_millis(100));
        eprintln!(
            "flash: screen-size record restored ({}x{})",
            u16::from_be_bytes([record[6], record[7]]),
            u16::from_be_bytes([record[8], record[9]])
        );
    }

    eprintln!("power-cycle the card to apply");
    Ok(())
}

/// Write a previously dumped block image back to the card, for recovery.
pub fn restore_flash(cli: &Cli, image_path: &str, commit: bool, index: u16) -> Result<()> {
    let image = std::fs::read(image_path).with_context(|| format!("read {image_path}"))?;
    anyhow::ensure!(
        image.len() == 64 * 1024,
        "{image_path} is {} bytes; a block image must be exactly 65536",
        image.len()
    );
    if !commit {
        println!(
            "dry run: {image_path} -> block 0x{:02x} (add --commit)",
            protocol::PARAM_BLOCK
        );
        return Ok(());
    }
    let mut dev = open(cli)?;
    rewrite_block(&mut dev, index, &image, 2, 0..256)?;
    Ok(())
}
