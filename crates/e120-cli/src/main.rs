use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use e120_net::{bpf, pcap};
use e120_proto as protocol;
use e120_rcvbp as rcvbp;
use protocol::ColorOrder;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "e120",
    about = "Drive a Colorlight receiving card over raw Ethernet"
)]
struct Cli {
    /// Network interface directly connected to the receiving card
    #[arg(short, long, global = true, default_value = "en24")]
    iface: String,

    /// Panel width in pixels
    #[arg(long, global = true, default_value_t = 128)]
    width: u16,

    /// Panel height in pixels
    #[arg(long, global = true, default_value_t = 64)]
    height: u16,

    /// Color order on the wire
    #[arg(long, global = true, default_value = "bgr")]
    order: ColorOrder,

    /// Brightness 0-255 (sent in sync frames)
    #[arg(short, long, global = true, default_value_t = 255)]
    brightness: u8,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Send a discovery packet and print any card that answers
    Discover {
        /// Seconds to listen for responses
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Passively dump frames seen on the interface (debugging)
    Listen {
        #[arg(long, default_value_t = 10)]
        wait: u64,
    },
    /// Set panel brightness (0-255)
    Brightness { value: u8 },
    /// Fill the panel with a solid color, e.g. `fill ff0000` or `fill 255 0 0`
    Fill { color: Vec<String> },
    /// Show a test pattern
    Test {
        /// gradient | rows | border | rgb
        #[arg(default_value = "gradient")]
        pattern: String,
        /// Keep refreshing until Ctrl-C
        #[arg(long)]
        hold: bool,
    },
    /// Display an image file (scaled to panel size)
    Image {
        path: String,
        /// Keep refreshing until Ctrl-C
        #[arg(long)]
        hold: bool,
    },
    /// Blank the panel
    Blank,
    /// Read the card's stored configuration into a .rcvbp file (read-only)
    ReadConfig {
        /// Where to write the recovered .rcvbp
        #[arg(long, default_value = "card-config.rcvbp")]
        out: String,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Starting 256-byte flash page
        #[arg(long, default_value_t = protocol::FLASH_PAGE_BASIC_PARAM)]
        page: u16,
        /// Safety stop: most 1024-byte chunks to request
        #[arg(long, default_value_t = 64)]
        max_chunks: u16,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 2)]
        wait: u64,
    },
    /// Dump a whole 64KB flash block from the card (read-only)
    DumpFlash {
        #[arg(long, default_value = "block07.bin")]
        out: String,
        /// 64KB block selector; 0x07 holds the receiver parameters
        #[arg(long, default_value_t = 7)]
        block: u8,
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 2)]
        wait: u64,
    },
    /// Write a .rcvbp into the card's parameter flash (read-modify-write)
    WriteConfig {
        /// The .rcvbp to install
        config: String,
        /// Actually write. Without this it only reports what it would do.
        #[arg(long)]
        commit: bool,
        /// Where to save the pre-write backup of the whole block
        #[arg(long, default_value = "block07-backup.bin")]
        backup: String,
        /// Use this saved 64KB block image instead of reading the card
        #[arg(long)]
        base_image: Option<String>,
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 2)]
        wait: u64,
    },
    /// Tell the card its own size and the size of the whole screen
    SetLayout {
        #[arg(long, default_value_t = 128)]
        panel_width: u16,
        #[arg(long, default_value_t = 64)]
        panel_height: u16,
        #[arg(long, default_value_t = 0)]
        index: u16,
    },
    /// Write a single flash page taken from a block image (debugging)
    WritePage {
        /// Page index within the parameter block
        #[arg(long)]
        page: u8,
        /// 64KB block image to take the page contents from
        #[arg(long)]
        from_image: String,
        /// Flag byte to send; writes normally use 0
        #[arg(long, default_value_t = 0)]
        flag: u8,
        #[arg(long)]
        commit: bool,
        #[arg(long, default_value_t = 0)]
        index: u16,
    },
    /// Restore a previously dumped 64KB block back to the card
    RestoreFlash {
        image: String,
        #[arg(long)]
        commit: bool,
        #[arg(long, default_value_t = 0)]
        index: u16,
    },
    /// Build a .rcvbp by combining and editing existing ones
    ConfigBuild {
        /// Starting point: the config to modify
        #[arg(long)]
        base: String,
        /// Copy records out of this config into the base
        #[arg(long)]
        copy_from: Option<String>,
        /// Record types to copy, comma separated hex, e.g. 0a84,0a01
        #[arg(long, default_value = "")]
        copy: String,
        /// Record types to delete, comma separated hex
        #[arg(long, default_value = "")]
        remove: String,
        /// Where to write the result
        #[arg(long)]
        out: String,
    },
    /// Compare two .rcvbp files record by record
    ConfigDiff { a: String, b: String },
    /// Inspect a .rcvbp receiver-parameter file
    Rcvbp {
        path: String,
        /// Hexdump each record's payload (non-empty ones)
        #[arg(long)]
        dump: bool,
    },
    /// Summarize Colorlight packet types in a pcap capture
    PcapSummary {
        path: String,
        /// Show full hexdumps of non-pixel packets
        #[arg(long)]
        dump: bool,
    },
    /// Replay sender->card frames from a pcap capture
    Replay {
        path: String,
        /// Comma-separated packet-type bytes (hex) to replay, e.g. "10,11,1f,26". Default: all non-pixel config types
        #[arg(long)]
        types: Option<String>,
        /// Delay between frames in microseconds
        #[arg(long, default_value_t = 500)]
        gap_us: u64,
        /// Include 0x55 pixel and 0x01 sync frames too
        #[arg(long)]
        all: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(&cli)
}

/// Commands that put an image on the panel.
fn run_display(cli: &Cli) -> Result<Option<()>> {
    match &cli.cmd {
        Cmd::Fill { color } => {
            let (r, g, b) = parse_color(color)?;
            let fb = solid(cli, r, g, b);
            show(cli, &fb, false).map(Some)
        }
        Cmd::Test { pattern, hold } => {
            let fb = test_pattern(cli, pattern)?;
            show(cli, &fb, *hold).map(Some)
        }
        Cmd::Image { path, hold } => {
            let img = image::open(path).with_context(|| format!("open image {path}"))?;
            let img = img
                .resize_exact(
                    u32::from(cli.width),
                    u32::from(cli.height),
                    image::imageops::FilterType::Lanczos3,
                )
                .to_rgb8();
            let mut fb = vec![[0u8; 3]; cli.width as usize * cli.height as usize];
            for (x, y, px) in img.enumerate_pixels() {
                fb[y as usize * cli.width as usize + x as usize] = [px[0], px[1], px[2]];
            }
            show(cli, &fb, *hold).map(Some)
        }
        Cmd::Blank => {
            let fb = solid(cli, 0, 0, 0);
            show(cli, &fb, false).map(Some)
        }
        Cmd::Brightness { value } => {
            let mut dev = open(cli)?;
            dev.send(&protocol::brightness(*value))?;
            dev.send(&protocol::sync(*value))?;
            println!("brightness set to {value}");
            Ok(Some(()))
        }
        Cmd::SetLayout {
            panel_width,
            panel_height,
            index,
        } => {
            let mut dev = open(cli)?;
            dev.send(&protocol::set_layout(
                *index,
                *panel_width,
                *panel_height,
                0,
                0,
                *panel_width,
                *panel_height,
            ))?;
            println!("sent layout: {panel_width}x{panel_height}");
            Ok(Some(()))
        }
        _ => Ok(None),
    }
}

fn run(cli: &Cli) -> Result<()> {
    if run_display(cli)?.is_some() {
        return Ok(());
    }
    match &cli.cmd {
        Cmd::Discover { wait } => discover(cli, *wait),
        Cmd::Listen { wait } => listen(cli, *wait),
        Cmd::ReadConfig {
            out,
            index,
            page,
            max_chunks,
            wait,
        } => read_config(cli, *index, *page, *max_chunks, *wait, out),
        Cmd::DumpFlash {
            out,
            block,
            index,
            wait,
        } => dump_flash(cli, *block, *index, *wait, out),
        Cmd::WriteConfig {
            config,
            commit,
            backup,
            base_image,
            index,
            wait,
        } => write_config(
            cli,
            config,
            *commit,
            backup,
            base_image.as_deref(),
            *index,
            *wait,
        ),
        Cmd::WritePage {
            page,
            from_image,
            flag,
            commit,
            index,
        } => write_single_page(cli, *page, from_image, *flag, *commit, *index),
        Cmd::RestoreFlash {
            image,
            commit,
            index,
        } => restore_flash(cli, image, *commit, *index),
        Cmd::ConfigBuild {
            base,
            copy_from,
            copy,
            remove,
            out,
        } => config_build(base, copy_from.as_deref(), copy, remove, out),
        Cmd::ConfigDiff { a, b } => config_diff(a, b),
        Cmd::Rcvbp { path, dump } => rcvbp_info(path, *dump),
        Cmd::PcapSummary { path, dump } => pcap_summary(path, *dump),
        Cmd::Replay {
            path,
            types,
            gap_us,
            all,
        } => replay(cli, path, types.as_deref(), *gap_us, *all),
        _ => unreachable!("handled by run_display"),
    }
}

/// True for frames sent by the sender/PC to the card.
fn is_sender_frame(d: &[u8]) -> bool {
    d.len() >= 14 && d[0..6] == protocol::CARD_MAC
}

/// True for frames sent by the card back to the PC.
fn is_card_frame(d: &[u8]) -> bool {
    d.len() >= 14 && d[6..12] == protocol::CARD_MAC
}

/// Read the card's stored configuration out of flash and save it as a
/// `.rcvbp` file. Only ever sends read-opcode flash frames, which carry no
/// data of their own and so cannot modify the card.
fn read_config(
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
            if let Some((w, scan)) = f.geometry() {
                println!("configured for width {w}, 1/{scan} scan");
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
fn read_chunk(dev: &mut bpf::Bpf, index: u16, page: u16, wait: u64) -> Result<Vec<u8>> {
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

/// Dump an entire 64KB flash block. Read-only.
fn dump_flash(cli: &Cli, block: u8, index: u16, wait: u64, out: &str) -> Result<()> {
    let mut dev = open(cli)?;
    let mut image = Vec::with_capacity(64 * 1024);
    // Each request returns 1024 bytes, which is four 256-byte pages.
    for lo in (0u16..0x100).step_by(protocol::FLASH_PAGES_PER_CHUNK as usize) {
        let page = (u16::from(block) << 8) | lo;
        image.extend_from_slice(&read_chunk(&mut dev, index, page, wait)?);
        if lo % 0x40 == 0 {
            println!("  read {:5} / 65536 bytes", image.len());
        }
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
const PARAM_OFFSET: usize = 0x8000;
/// Largest parameter blob the card will accept.
const PARAM_MAX: usize = 0x6ffc;

/// Read the whole parameter block into memory.
fn read_block(dev: &mut bpf::Bpf, index: u16, wait: u64) -> Result<Vec<u8>> {
    let mut image = Vec::with_capacity(64 * 1024);
    for lo in (0u16..0x100).step_by(protocol::FLASH_PAGES_PER_CHUNK as usize) {
        let page = (u16::from(protocol::PARAM_BLOCK) << 8) | lo;
        image.extend_from_slice(&read_chunk(dev, index, page, wait)?);
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
fn rewrite_block(
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
            if repair.len() > 32 && n % 64 == 0 {
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
fn erase_and_settle(dev: &mut bpf::Bpf, index: u16) -> Result<()> {
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
fn write_config(
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
    println!("power-cycle the card for the new configuration to take effect");
    Ok(())
}

/// Write one page, for probing which regions the card will accept.
fn write_single_page(
    cli: &Cli,
    page: u8,
    from_image: &str,
    flag: u8,
    commit: bool,
    index: u16,
) -> Result<()> {
    let img = std::fs::read(from_image).with_context(|| format!("read {from_image}"))?;
    anyhow::ensure!(
        img.len() == 64 * 1024,
        "{from_image} must be exactly 65536 bytes"
    );
    let off = usize::from(page) * protocol::FLASH_PAGE_BYTES;
    let data = &img[off..off + protocol::FLASH_PAGE_BYTES];
    println!("page 0x{page:02x} from {from_image}, flag {flag}");
    if !commit {
        println!("dry run; re-run with --commit");
        return Ok(());
    }
    let mut dev = open(cli)?;
    dev.send(&protocol::write_page_flag(
        index,
        protocol::PARAM_BLOCK,
        page,
        data,
        flag,
    )?)?;
    std::thread::sleep(Duration::from_millis(50));

    let got = read_chunk(
        &mut dev,
        index,
        (u16::from(protocol::PARAM_BLOCK) << 8) | u16::from(page),
        2,
    )?;
    let ok = got[..protocol::FLASH_PAGE_BYTES] == *data;
    println!("readback: {}", if ok { "MATCHES" } else { "still differs" });
    Ok(())
}

/// Write a previously dumped block image back to the card, for recovery.
fn restore_flash(cli: &Cli, image_path: &str, commit: bool, index: u16) -> Result<()> {
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

/// Parse a comma-separated list of hex record types.
fn parse_types(s: &str) -> Result<Vec<u16>> {
    s.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| {
            u16::from_str_radix(t.trim_start_matches("0x"), 16)
                .with_context(|| format!("bad record type {t:?}"))
        })
        .collect()
}

fn config_build(
    base: &str,
    copy_from: Option<&str>,
    copy: &str,
    remove: &str,
    out: &str,
) -> Result<()> {
    let mut cfg = rcvbp::Rcvbp::load(base)?;
    println!("base {base}: {} records", cfg.records.len());

    let to_copy = parse_types(copy)?;
    if !to_copy.is_empty() {
        let src_path = copy_from.context("--copy needs --copy-from")?;
        let src = rcvbp::Rcvbp::load(src_path)?;
        for t in to_copy {
            let rec = src
                .find(t)
                .with_context(|| format!("{src_path} has no record 0x{t:04x}"))?;
            let existed = cfg.find(t).is_some();
            cfg.upsert(t, rec.payload.clone());
            println!(
                "  {} record 0x{t:04x} ({} bytes) from {src_path}",
                if existed { "replaced" } else { "added" },
                rec.payload.len()
            );
        }
    }

    for t in parse_types(remove)? {
        println!(
            "  {} record 0x{t:04x}",
            if cfg.remove(t) { "removed" } else { "no such" }
        );
    }

    cfg.save(out)?;
    let written = std::fs::metadata(out)?.len();
    println!(
        "wrote {out}: {} records, {written} bytes on disk",
        cfg.records.len()
    );

    // Read it straight back so a broken file never reaches the card.
    let back = rcvbp::Rcvbp::load(out)?;
    anyhow::ensure!(
        back.records.len() == cfg.records.len(),
        "verification failed: wrote {} records but read back {}",
        cfg.records.len(),
        back.records.len()
    );
    println!("verified: reparses to {} records", back.records.len());
    Ok(())
}

fn config_diff(a: &str, b: &str) -> Result<()> {
    let fa = rcvbp::Rcvbp::load(a)?;
    let fb = rcvbp::Rcvbp::load(b)?;
    println!("{a}: {} records", fa.records.len());
    println!("{b}: {} records", fb.records.len());

    let types_a: Vec<u16> = fa.records.iter().map(rcvbp::Record::type_u16).collect();
    let types_b: Vec<u16> = fb.records.iter().map(rcvbp::Record::type_u16).collect();
    let only_a: Vec<String> = types_a
        .iter()
        .filter(|t| !types_b.contains(t))
        .map(|t| format!("0x{t:04x}"))
        .collect();
    let only_b: Vec<String> = types_b
        .iter()
        .filter(|t| !types_a.contains(t))
        .map(|t| format!("0x{t:04x}"))
        .collect();
    if !only_a.is_empty() {
        println!("only in {a}: {}", only_a.join(", "));
    }
    if !only_b.is_empty() {
        println!("only in {b}: {}", only_b.join(", "));
    }

    for t in &types_a {
        let (Some(ra), Some(rb)) = (fa.find(*t), fb.find(*t)) else {
            continue;
        };
        if ra.payload == rb.payload {
            continue;
        }
        let n = ra.payload.len().min(rb.payload.len());
        let diffs: Vec<usize> = (0..n)
            .filter(|i| ra.payload[*i] != rb.payload[*i])
            .collect();
        println!(
            "record 0x{t:04x}: {} vs {} bytes, {} differ",
            ra.payload.len(),
            rb.payload.len(),
            diffs.len()
        );
        for i in diffs.iter().take(16) {
            println!(
                "    +0x{i:03x}: {:3} (0x{:02x})  vs  {:3} (0x{:02x})",
                ra.payload[*i], ra.payload[*i], rb.payload[*i], rb.payload[*i]
            );
        }
        if diffs.len() > 16 {
            println!("    ... and {} more", diffs.len() - 16);
        }
    }
    Ok(())
}

fn rcvbp_info(path: &str, dump: bool) -> Result<()> {
    let f = rcvbp::Rcvbp::load(path)?;
    println!(
        "{path}\n  version {}, {} bytes decompressed, {} records",
        f.version,
        f.blob.len(),
        f.records.len()
    );
    if let Some((w, scan)) = f.geometry() {
        println!("  cabinet: width {w}, 1/{scan} scan");
    }
    if let Some((w, scan)) = f.main_geometry() {
        println!("  main param block: width {w}, scan 1/{scan}");
    }
    println!(
        "\n{:>8} {:>7} {:>7} {:>8}  description",
        "offset", "type", "bytes", "nonzero"
    );
    for r in &f.records {
        let nz = r.payload.iter().filter(|&&b| b != 0).count();
        println!(
            "0x{:06x}  0x{:04x} {:7} {:8}  {}",
            r.offset,
            r.type_u16(),
            r.payload.len(),
            nz,
            describe_record(r.type_u16(), r.is_empty_table())
        );
    }
    if dump {
        for r in &f.records {
            if r.is_empty_table() {
                continue;
            }
            println!(
                "\n=== record 0x{:04x} ({} bytes)",
                r.type_u16(),
                r.payload.len()
            );
            hexdump(&r.payload[..r.payload.len().min(512)]);
        }
    }
    Ok(())
}

fn describe_record(t: u16, empty: bool) -> &'static str {
    match (t, empty) {
        (_, true) => "(empty table)",
        (0x0a01, _) => "main receiver parameters (geometry, scan, timing)",
        (0x0a03, _) => "pixel/row mapping table",
        (0x0a84, _) => "driver-chip register table",
        (0x0a8a, _) => "secondary parameters",
        (0x0aca, _) => "cabinet geometry",
        (0x0a83 | 0x0a89, _) => "RGB coefficients",
        _ => "",
    }
}

fn pcap_summary(path: &str, dump: bool) -> Result<()> {
    let pkts = pcap::read_pcap(path)?;
    println!("{} packets", pkts.len());
    let mut counts: std::collections::BTreeMap<(bool, u8), (usize, usize)> =
        std::collections::BTreeMap::default();
    for p in &pkts {
        let d = &p.data;
        if d.len() < 14 {
            continue;
        }
        let (dir_tx, ty) = if is_sender_frame(d) {
            (true, d[12])
        } else if is_card_frame(d) {
            (false, d[12])
        } else {
            continue;
        };
        let e = counts.entry((dir_tx, ty)).or_default();
        e.0 += 1;
        e.1 += d.len();
        if dump && ty != 0x55 && ty != 0x01 && ty != 0x0a {
            let t0 = f64::from(pkts[0].ts_sec) + f64::from(pkts[0].ts_usec) / 1e6;
            let t = f64::from(p.ts_sec) + f64::from(p.ts_usec) / 1e6 - t0;
            println!(
                "\n[{:9.4}s] {} type 0x{:02x} len {}",
                t,
                if dir_tx { "PC->card" } else { "card->PC" },
                ty,
                d.len()
            );
            hexdump(&d[..d.len().min(160)]);
        }
    }
    println!("\n{:<10} {:>6} {:>10}  type", "direction", "count", "bytes");
    for ((tx, ty), (n, bytes)) in counts {
        println!(
            "{:<10} {:>6} {:>10}  0x{ty:02x}",
            if tx { "PC->card" } else { "card->PC" },
            n,
            bytes
        );
    }
    Ok(())
}

fn replay(cli: &Cli, path: &str, types: Option<&str>, gap_us: u64, all: bool) -> Result<()> {
    let filter: Option<Vec<u8>> = match types {
        Some(t) => Some(
            t.split(',')
                .map(|s| u8::from_str_radix(s.trim(), 16))
                .collect::<Result<_, _>>()
                .context("bad --types list")?,
        ),
        None => None,
    };
    let pkts = pcap::read_pcap(path)?;
    let mut dev = open(cli)?;
    let mut sent = 0usize;
    for p in &pkts {
        let d = &p.data;
        if !is_sender_frame(d) {
            continue;
        }
        let ty = d[12];
        let selected = match &filter {
            Some(f) => f.contains(&ty),
            None => all || !matches!(ty, 0x55 | 0x01 | 0x0a | 0x07),
        };
        if !selected {
            continue;
        }
        dev.send(d)?;
        sent += 1;
        std::thread::sleep(Duration::from_micros(gap_us));
    }
    println!("replayed {sent} frames from {path}");
    Ok(())
}

fn open(cli: &Cli) -> Result<bpf::Bpf> {
    bpf::Bpf::open(&cli.iface, true, 500)
}

fn discover(cli: &Cli, wait: u64) -> Result<()> {
    let mut dev = open(cli)?;
    println!("sending discovery on {} ...", cli.iface);
    dev.send(&protocol::discovery())?;
    let deadline = Instant::now() + Duration::from_secs(wait);
    let mut found = 0;
    while Instant::now() < deadline {
        for f in dev.recv()? {
            // Ignore our own transmissions (BPF loops them back)
            if f.len() >= 12 && f[6..12] == protocol::SENDER_MAC {
                continue;
            }
            if let Some(info) = protocol::parse_discovery_response(&f) {
                found += 1;
                println!(
                    "receiver card #{}: id=0x{:02x} firmware={}.{:02} detected size {}x{}",
                    info.controller,
                    info.card_id,
                    info.ver_major,
                    info.ver_minor,
                    info.cols,
                    info.rows
                );
                println!("first 64 payload bytes:");
                hexdump(&info.raw[..info.raw.len().min(64)]);
            } else if f.len() >= 14 {
                println!(
                    "other frame: src {} type {:02x}{:02x} len {}",
                    mac(&f[6..12]),
                    f[12],
                    f[13],
                    f.len()
                );
            }
        }
    }
    if found == 0 {
        println!("no discovery response received in {wait}s");
        println!("(check link on {}, and that the card has power)", cli.iface);
    }
    Ok(())
}

fn listen(cli: &Cli, wait: u64) -> Result<()> {
    let mut dev = open(cli)?;
    println!("listening on {} for {wait}s ...", cli.iface);
    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        for f in dev.recv()? {
            if f.len() < 14 || f[6..12] == protocol::SENDER_MAC {
                continue;
            }
            println!(
                "frame: dst {} src {} type {:02x}{:02x} len {}",
                mac(&f[0..6]),
                mac(&f[6..12]),
                f[12],
                f[13],
                f.len()
            );
            hexdump(&f[14..f.len().min(14 + 96)]);
        }
    }
    Ok(())
}

/// Send one full frame of pixels: row packets, then the sync/display frame.
fn send_frame(dev: &mut bpf::Bpf, cli: &Cli, fb: &[[u8; 3]]) -> Result<()> {
    let w = cli.width as usize;
    for row in 0..cli.height {
        let line = &fb[row as usize * w..(row as usize + 1) * w];
        let mut offset = 0usize;
        for chunk in line.chunks(protocol::MAX_PIXELS_PER_PACKET) {
            dev.send(&protocol::pixel_row(row, offset as u16, chunk, cli.order))?;
            offset += chunk.len();
        }
    }
    dev.send(&protocol::sync(cli.brightness))?;
    Ok(())
}

fn show(cli: &Cli, fb: &[[u8; 3]], hold: bool) -> Result<()> {
    let mut dev = open(cli)?;
    dev.send(&protocol::brightness(cli.brightness))?;
    if hold {
        println!("refreshing at ~30fps, Ctrl-C to stop");
        loop {
            send_frame(&mut dev, cli, fb)?;
            std::thread::sleep(Duration::from_millis(33));
        }
    } else {
        // Send a few frames so at least one lands after the card settles
        for _ in 0..3 {
            send_frame(&mut dev, cli, fb)?;
            std::thread::sleep(Duration::from_millis(33));
        }
        println!(
            "frame sent ({}x{}, order {:?})",
            cli.width, cli.height, cli.order
        );
        Ok(())
    }
}

fn solid(cli: &Cli, r: u8, g: u8, b: u8) -> Vec<[u8; 3]> {
    vec![[r, g, b]; cli.width as usize * cli.height as usize]
}

fn test_pattern(cli: &Cli, pattern: &str) -> Result<Vec<[u8; 3]>> {
    let (w, h) = (cli.width as usize, cli.height as usize);
    let mut fb = vec![[0u8; 3]; w * h];
    match pattern {
        "gradient" => {
            for y in 0..h {
                for x in 0..w {
                    fb[y * w + x] = [(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 128];
                }
            }
        }
        "rows" => {
            // Each row: red if row%3==0, green if 1, blue if 2 — for mapping checks
            for y in 0..h {
                let c = match y % 3 {
                    0 => [255, 0, 0],
                    1 => [0, 255, 0],
                    _ => [0, 0, 255],
                };
                for x in 0..w {
                    fb[y * w + x] = c;
                }
            }
        }
        "border" => {
            for y in 0..h {
                for x in 0..w {
                    if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                        fb[y * w + x] = [255, 255, 255];
                    }
                }
            }
            // Corner markers: top-left red, top-right green, bottom-left blue
            fb[0] = [255, 0, 0];
            fb[w - 1] = [0, 255, 0];
            fb[(h - 1) * w] = [0, 0, 255];
        }
        "rgb" => {
            // Thirds: red / green / blue vertical bands — color order check
            for y in 0..h {
                for x in 0..w {
                    fb[y * w + x] = if x < w / 3 {
                        [255, 0, 0]
                    } else if x < 2 * w / 3 {
                        [0, 255, 0]
                    } else {
                        [0, 0, 255]
                    };
                }
            }
        }
        other => anyhow::bail!("unknown pattern {other:?} (gradient|rows|border|rgb)"),
    }
    Ok(fb)
}

fn parse_color(parts: &[String]) -> Result<(u8, u8, u8)> {
    match parts {
        [hex] => {
            let hex = hex.trim_start_matches('#');
            anyhow::ensure!(hex.len() == 6, "expected RRGGBB hex or three 0-255 values");
            let v = u32::from_str_radix(hex, 16).context("bad hex color")?;
            Ok(((v >> 16) as u8, (v >> 8) as u8, v as u8))
        }
        [r, g, b] => Ok((r.parse()?, g.parse()?, b.parse()?)),
        _ => anyhow::bail!("expected RRGGBB hex or three 0-255 values"),
    }
}

fn mac(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn hexdump(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        println!("  {:04x}: {}", i * 16, hex.join(" "));
    }
}
