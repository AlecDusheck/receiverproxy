//! Reading and writing the card's flash: configuration, dumps, and firmware.

use crate::util::{hexdump, is_card_frame, open};
use crate::{protocol, rcvbp, Cli};
use anyhow::{Context, Result};
use e120_net::bpf;
use std::time::{Duration, Instant};

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
        dev.send(&protocol::read_flash(index, page))?;

        let deadline = Instant::now() + Duration::from_secs(wait);
        let mut got = false;
        while Instant::now() < deadline && !got {
            for f in dev.recv()? {
                if !is_card_frame(&f) {
                    continue;
                }
                if let Some(data) = protocol::flash_reply_data(&f) {
                    flash.extend_from_slice(data);
                    got = true;
                    break;
                }
            }
        }
        if !got {
            anyhow::bail!("no reply for page 0x{page:04x} after {wait}s");
        }

        // The blob opens with its own total length, so we know when to stop.
        if expected.is_none() && flash.len() >= 4 {
            let n = u32::from_le_bytes([flash[0], flash[1], flash[2], flash[3]]) as usize;
            println!("card reports {n} bytes of stored configuration");
            expected = Some(n);
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
    println!("wrote {} bytes to {out}", file.len());

    match rcvbp::Rcvbp::load(out) {
        Ok(f) => {
            println!("parsed: {} records", f.records.len());
            if let Some((w, _)) = f.geometry() {
                println!("configured for width {w}");
            }
            if let Some(scan) = f.scan() {
                println!("scan: 1/{scan}");
            }
            let has_chip_regs = f.find(0x0a84).is_some_and(|r| !r.is_empty_table());
            println!(
                "driver-chip register table: {}",
                if has_chip_regs {
                    "present"
                } else {
                    "ABSENT - panels with PWM driver ICs will stay dark"
                }
            );
        }
        Err(e) => println!("saved, but did not parse as .rcvbp: {e}"),
    }
    Ok(())
}

/// Request one 1024-byte chunk of flash and return it.
pub fn read_chunk(dev: &mut bpf::Bpf, index: u16, page: u16, wait: u64) -> Result<Vec<u8>> {
    dev.send(&protocol::read_flash(index, page))?;
    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        for f in dev.recv()? {
            if !is_card_frame(&f) {
                continue;
            }
            if let Some(data) = protocol::flash_reply_data(&f) {
                return Ok(data.to_vec());
            }
        }
    }
    anyhow::bail!("no reply for page 0x{page:04x} within {wait}s")
}

/// Ask the card what firmware image its bootloader expects. Read-only.
pub fn upgrade_info(cli: &Cli, wait: u64) -> Result<()> {
    let mut dev = open(cli)?;
    dev.send(&protocol::upgrade_info())?;

    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        for f in dev.recv()? {
            if !is_card_frame(&f) || f.len() < 40 {
                continue;
            }
            println!("reply: type {:02x}{:02x}, {} bytes", f[12], f[13], f.len());
            let Some(info) = protocol::parse_upgrade_info(&f[14..]) else {
                println!("  no recognisable image length in this reply");
                hexdump(&f[14..f.len().min(14 + 128)]);
                continue;
            };
            println!("  declared image length: 0x{:06x}", info.declared_len);
            println!(
                "    matches: {}",
                match info.declared_len {
                    0x000b_0000 => "the PWM / LS0allDA image format",
                    0x000b_0080 => "the Normal image format",
                    _ => "neither known format",
                }
            );
            println!("  capabilities: 0b{:04b}", info.capabilities);
            println!("    golden image present:     {}", info.has_golden());
            println!(
                "    golden upgrade accepted:  {}",
                info.supports_golden_upgrade()
            );
            println!(
                "    SDRAM staging supported:  {}",
                info.supports_sdram_staging()
            );
            return Ok(());
        }
    }
    println!("no reply within {wait}s");
    Ok(())
}

/// Read the firmware region back and count bytes that differ from `img`.
fn verify_firmware(dev: &mut bpf::Bpf, index: u16, img: &[u8], wait: u64) -> Result<usize> {
    let mut bad = 0usize;
    for block in protocol::FIRMWARE_BLOCKS {
        for lo in (0u16..0x100).step_by(protocol::FLASH_PAGES_PER_CHUNK as usize) {
            let page = (u16::from(block) << 8) | lo;
            let got = read_chunk(dev, index, page, wait)?;
            let off =
                (usize::from(block) * 256 + usize::from(lo as u8)) * protocol::FLASH_PAGE_BYTES;
            bad += got
                .iter()
                .enumerate()
                .filter(|(i, g)| **g != img.get(off + i).copied().unwrap_or(0xff))
                .count();
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
            println!("  {}", header[i..].split("  ").next().unwrap_or("").trim());
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
    const LATTICE: &[u8] = b"Lattice Semiconductor";
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
        img.windows(LATTICE.len()).take(256).any(|w| w == LATTICE),
        "{image} does not look like a Lattice bitstream"
    );
    let span = protocol::FIRMWARE_BLOCKS.len() * 64 * 1024;
    anyhow::ensure!(
        img.len() >= span,
        "{image} is only {} bytes; the primary bank is {span}",
        img.len()
    );
    // Images carry padding past their declared length. The meaningful content
    // ends with the end marker and CRC just inside the bank, so write exactly
    // one bank's worth and drop the tail.
    if img.len() > span {
        println!(
            "  note: dropping {} bytes of padding past the {span}-byte bank",
            img.len() - span
        );
    }
    let img = &img[..span];

    // Refuse to proceed without a local copy of what we are about to replace.
    let old = std::fs::read(backup).with_context(|| format!("read backup {backup}"))?;
    anyhow::ensure!(
        old.len() >= span && old.windows(LATTICE.len()).take(256).any(|w| w == LATTICE),
        "{backup} is not a usable dump of the current primary bank"
    );

    println!("installing {image} ({} bytes)", img.len());
    describe_image(img);
    println!(
        "  target: blocks 0x{:02x}..0x{:02x}; golden bank at 0x{:02x} untouched",
        blocks.start,
        blocks.end - 1,
        protocol::GOLDEN_BLOCK
    );
    println!("  recovery: {backup}");

    if !commit {
        println!("\ndry run: nothing was written. Re-run with --commit to install.");
        return Ok(());
    }

    let mut dev = open(cli)?;

    // The program region is write-protected; without this every erase and
    // write is silently ignored.
    println!("unlocking the program region");
    dev.send(&protocol::set_program_writable(index, true))?;
    std::thread::sleep(Duration::from_millis(200));

    for block in blocks.clone() {
        println!("erasing block 0x{block:02x}");
        dev.send(&protocol::erase_firmware_block(index, block)?)?;
        std::thread::sleep(Duration::from_secs(3));
    }

    let mut written = 0usize;
    for block in blocks {
        for page in 0..=0xffu8 {
            let off = (usize::from(block) * 256 + usize::from(page)) * protocol::FLASH_PAGE_BYTES;
            let mut buf = [0xffu8; protocol::FLASH_PAGE_BYTES];
            if off < img.len() {
                let n = (img.len() - off).min(protocol::FLASH_PAGE_BYTES);
                buf[..n].copy_from_slice(&img[off..off + n]);
            }
            dev.send(&protocol::write_firmware_page(index, block, page, &buf)?)?;
            std::thread::sleep(Duration::from_millis(6));
            written += protocol::FLASH_PAGE_BYTES;
        }
        println!(
            "  block 0x{block:02x} written ({} KB total)",
            written / 1024
        );
    }

    // Relock before verifying, so the region is protected even if we stop here.
    dev.send(&protocol::set_program_writable(index, false))?;
    println!("relocked the program region");

    println!("verifying...");
    let bad = verify_firmware(&mut dev, index, img, wait)?;
    if bad == 0 {
        println!("verified: the primary bank matches the image");
    } else {
        println!("WARNING: {bad} bytes differ after writing");
        println!(
            "  the golden bank at 0x{:02x} is untouched, and {backup} can be written back with:",
            protocol::GOLDEN_BLOCK
        );
        println!("  e120 flash-firmware {backup} --backup {backup} --commit");
    }
    println!("\npower-cycle the card to load the new bitstream");
    Ok(())
}

/// Read page 0 of each block and report what it looks like. Read-only.
pub fn scan_flash(cli: &Cli, first: u8, last: u8, index: u16, wait: u64) -> Result<()> {
    const LATTICE: &[u8] = b"Lattice Semiconductor";
    let mut dev = open(cli)?;
    let mut runs: Vec<(u8, String)> = Vec::new();
    for blk in first..=last {
        let page = u16::from(blk) << 8;
        let Ok(d) = read_chunk(&mut dev, index, page, wait) else {
            continue;
        };
        let kind = if d.windows(LATTICE.len()).any(|w| w == LATTICE) {
            "LATTICE BITSTREAM HEADER"
        } else if d.starts_with(&[0x20, 0x20, 0x19, 0xbe]) {
            "rcvbp config"
        } else if d.iter().all(|&b| b == 0xff) {
            continue; // erased
        } else if d.iter().all(|&b| b == 0) {
            continue; // blank
        } else {
            "data"
        };
        println!(
            "  block 0x{blk:02x} (0x{:06x}): {kind}  {}",
            u32::from(blk) << 16,
            d.iter()
                .take(12)
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        runs.push((blk, kind.to_string()));
    }
    let heads: Vec<String> = runs
        .iter()
        .filter(|(_, k)| k.starts_with("LATTICE"))
        .map(|(b, _)| format!("0x{b:02x}"))
        .collect();
    println!("\nblocks with data: {}", runs.len());
    println!("bitstream headers found at blocks: {}", heads.join(", "));
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
    let mut first_reply = true;
    while addr < start + len {
        dev.send(&protocol::read_flash_linear(index, addr, step))?;
        let deadline = Instant::now() + Duration::from_secs(wait);
        let mut got = false;
        while Instant::now() < deadline && !got {
            for f in dev.recv()? {
                if !is_card_frame(&f) {
                    continue;
                }
                // Linear reads may answer with a different type than the
                // page-addressed reads, so take any sufficiently long reply.
                if f.len() < 15 + step as usize {
                    continue;
                }
                if first_reply {
                    println!("  reply type {:02x}{:02x}, {} bytes", f[12], f[13], f.len());
                    first_reply = false;
                }
                image.extend_from_slice(&f[15..15 + step as usize]);
                got = true;
                break;
            }
        }
        if !got {
            misses += 1;
            image.extend(std::iter::repeat_n(0xffu8, step as usize));
            if misses > 8 {
                println!("giving up after {misses} unanswered reads at 0x{addr:08x}");
                break;
            }
        }
        if (addr - start).is_multiple_of(0x10000) {
            println!("  0x{addr:08x} ({} KB read)", image.len() / 1024);
        }
        addr += step;
    }
    std::fs::write(out, &image).with_context(|| format!("write {out}"))?;
    println!(
        "wrote {} bytes to {out} ({misses} unanswered reads)",
        image.len()
    );
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
    let mut image = Vec::with_capacity(64 * 1024 * blocks as usize);
    for b in 0..blocks {
        let blk = block.wrapping_add(b as u8);
        // Each request returns 1024 bytes, which is four 256-byte pages.
        for lo in (0u16..0x100).step_by(protocol::FLASH_PAGES_PER_CHUNK as usize) {
            let page = (u16::from(blk) << 8) | lo;
            image.extend_from_slice(&read_chunk(&mut dev, index, page, wait)?);
        }
        println!("  block 0x{blk:02x} done, {} KB total", image.len() / 1024);
    }
    std::fs::write(out, &image).with_context(|| format!("write {out}"))?;
    println!("wrote {} bytes to {out}", image.len());

    // Summarise which pages hold anything, so we can see the layout.
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, page) in image.chunks(256).enumerate() {
        let blank = page.iter().all(|&b| b == 0 || b == 0xff);
        match (blank, start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                runs.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push((s, image.len() / 256));
    }
    println!("non-blank page ranges (256-byte pages):");
    for (a, b) in runs {
        println!(
            "  pages 0x{a:02x}..0x{b:02x}  = offsets 0x{:05x}..0x{:05x}",
            a * 256,
            b * 256
        );
    }
    Ok(())
}

/// Offset of the parameter blob within the 64KB parameter block.
pub const PARAM_OFFSET: usize = 0x8000;

/// Largest parameter blob the card will accept.
pub const PARAM_MAX: usize = 0x6ffc;

/// Read the whole parameter block into memory.
pub fn read_block(dev: &mut bpf::Bpf, index: u16, wait: u64) -> Result<Vec<u8>> {
    read_blocks(dev, index, protocol::PARAM_BLOCK, 1, wait)
}

/// Read `count` consecutive 64KB blocks starting at `first`.
///
/// # Errors
/// Fails if the card stops answering partway through.
pub fn read_blocks(
    dev: &mut bpf::Bpf,
    index: u16,
    first: u8,
    count: u16,
    wait: u64,
) -> Result<Vec<u8>> {
    let mut image = Vec::with_capacity(64 * 1024 * count as usize);
    for b in 0..count {
        let block = first.wrapping_add(b as u8);
        for lo in (0u16..0x100).step_by(protocol::FLASH_PAGES_PER_CHUNK as usize) {
            let page = (u16::from(block) << 8) | lo;
            image.extend_from_slice(&read_chunk(dev, index, page, wait)?);
        }
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
    dev: &mut bpf::Bpf,
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
            let bad: Vec<usize> = (0..pages.len())
                .filter(|i| {
                    after[i * protocol::FLASH_PAGE_BYTES..(i + 1) * protocol::FLASH_PAGE_BYTES]
                        != *pages[*i]
                })
                .collect();
            if bad.is_empty() {
                println!("verified: flash matches what we wrote");
                return Ok(());
            }
            // Rewriting only works into still-erased pages.
            let dirty = bad.iter().any(|i| {
                after[i * protocol::FLASH_PAGE_BYTES..(i + 1) * protocol::FLASH_PAGE_BYTES]
                    .iter()
                    .any(|&b| b != 0xff)
            });
            println!(
                "attempt {attempt}: {} pages need rewriting{}",
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
                println!("  wrote {n} / {} pages", repair.len());
            }
        }
    }
    // Some pages sit outside the window the card lets us write. Those are not
    // part of the configuration blob, so report them rather than failing.
    let after = read_block(dev, index, wait)?;
    let bad: Vec<usize> = (0..pages.len())
        .filter(|i| {
            after[i * protocol::FLASH_PAGE_BYTES..(i + 1) * protocol::FLASH_PAGE_BYTES]
                != *pages[*i]
        })
        .collect();
    let in_config = bad.iter().any(|i| must_verify.contains(i));
    anyhow::ensure!(
        !in_config,
        "flash did not verify: {} pages differ, including configuration pages",
        bad.len()
    );
    println!(
        "note: {} page(s) outside the configuration area would not take writes: {}",
        bad.len(),
        bad.iter()
            .map(|i| format!("0x{i:02x}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("configuration pages verified");
    Ok(())
}

/// Erase the parameter block and wait for the chip to finish.
pub fn erase_and_settle(dev: &mut bpf::Bpf, index: u16) -> Result<()> {
    println!("erasing block 0x{:02x}...", protocol::PARAM_BLOCK);
    dev.send(&protocol::erase_block(index, protocol::PARAM_BLOCK)?)?;
    // A block erase takes far longer than a page write; writing during it is
    // silently dropped, which is exactly what went wrong the first time.
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
    println!(
        "{config}: {} records, {} bytes on disk",
        parsed.records.len(),
        file.len()
    );
    anyhow::ensure!(
        file.len() <= PARAM_MAX,
        "config is {} bytes, over the {PARAM_MAX}-byte limit the card accepts",
        file.len()
    );
    let has_chip = parsed.find(0x0a84).is_some_and(|r| !r.is_empty_table());
    println!(
        "  driver-chip register table: {}",
        if has_chip { "present" } else { "absent" }
    );

    let mut dev = open(cli)?;
    let original = match base_image {
        Some(path) => {
            let img = std::fs::read(path).with_context(|| format!("read {path}"))?;
            anyhow::ensure!(img.len() == 64 * 1024, "{path} must be exactly 65536 bytes");
            println!("using {path} as the block contents");
            img
        }
        None => {
            println!("reading current block 0x{:02x}...", protocol::PARAM_BLOCK);
            let img = read_block(&mut dev, index, wait)?;
            std::fs::write(backup, &img).with_context(|| format!("write {backup}"))?;
            println!("backed up {} bytes to {backup}", img.len());
            img
        }
    };

    // Splice the new parameter blob in, leaving the rest of the block alone.
    let mut image = original.clone();
    let old_len = u32::from_le_bytes([
        image[PARAM_OFFSET],
        image[PARAM_OFFSET + 1],
        image[PARAM_OFFSET + 2],
        image[PARAM_OFFSET + 3],
    ]) as usize;
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
    println!(
        "replacing parameter blob: {old_len} bytes -> {} bytes ({changed} bytes of the block change)",
        file.len()
    );

    if !commit {
        println!(
            "
dry run: nothing was written. Re-run with --commit to install."
        );
        return Ok(());
    }

    // Only the pages holding the configuration blob itself have to verify.
    let first = PARAM_OFFSET / protocol::FLASH_PAGE_BYTES;
    let last = (PARAM_OFFSET + 4 + file.len()).div_ceil(protocol::FLASH_PAGE_BYTES);
    rewrite_block(&mut dev, index, &image, wait, first..last)
        .with_context(|| format!("the original block is saved at {backup}; restore it with: e120 restore-flash {backup} --commit"))?;

    // The block erase also clears the screen-size record, which lives outside
    // the window these frames can rewrite. Put it back through the
    // linear-address path, or the card boots with a bogus screen size.
    // Only when the block was read off the card. A block taken from a firmware
    // image holds bitstream data at this address, not a record, and writing
    // that would set a nonsense screen size.
    let off = protocol::SCREEN_RECORD_ADDR as usize & 0xffff;
    let record = &original[off..off + protocol::SCREEN_RECORD_LEN];
    if base_image.is_some() {
        println!(
            "note: leaving the screen-size record alone; \
             {} is a firmware image, so it holds no record to restore",
            base_image.unwrap_or_default()
        );
    } else if record.iter().any(|&b| b != 0xff) {
        dev.send(&protocol::write_screen_record(
            index,
            protocol::SCREEN_RECORD_ADDR,
            record,
        )?)?;
        std::thread::sleep(Duration::from_millis(100));
        println!(
            "restored the screen-size record ({}x{})",
            u16::from_be_bytes([record[6], record[7]]),
            u16::from_be_bytes([record[8], record[9]])
        );
    }

    println!("power-cycle the card for the new configuration to take effect");
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
            "dry run: would restore {image_path} to block 0x{:02x}. Re-run with --commit.",
            protocol::PARAM_BLOCK
        );
        return Ok(());
    }
    let mut dev = open(cli)?;
    rewrite_block(&mut dev, index, &image, 2, 0..256)?;
    println!("restored {image_path}");
    Ok(())
}
