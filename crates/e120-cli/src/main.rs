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
    match &cli.cmd {
        Cmd::Discover { wait } => discover(&cli, *wait),
        Cmd::Listen { wait } => listen(&cli, *wait),
        Cmd::Brightness { value } => {
            let mut dev = open(&cli)?;
            dev.send(&protocol::brightness(*value))?;
            dev.send(&protocol::sync(*value))?;
            println!("brightness set to {value}");
            Ok(())
        }
        Cmd::Fill { color } => {
            let (r, g, b) = parse_color(color)?;
            let fb = solid(&cli, r, g, b);
            show(&cli, &fb, false)
        }
        Cmd::Test { pattern, hold } => {
            let fb = test_pattern(&cli, pattern)?;
            show(&cli, &fb, *hold)
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
            show(&cli, &fb, *hold)
        }
        Cmd::Blank => {
            let fb = solid(&cli, 0, 0, 0);
            show(&cli, &fb, false)
        }
        Cmd::ReadConfig {
            out,
            index,
            page,
            max_chunks,
            wait,
        } => read_config(&cli, *index, *page, *max_chunks, *wait, out),
        Cmd::Rcvbp { path, dump } => rcvbp_info(path, *dump),
        Cmd::PcapSummary { path, dump } => pcap_summary(path, *dump),
        Cmd::Replay {
            path,
            types,
            gap_us,
            all,
        } => replay(&cli, path, types.as_deref(), *gap_us, *all),
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
    if flash.len() < total {
        anyhow::bail!(
            "only read {} of {total} bytes; raise --max-chunks",
            flash.len()
        );
    }
    // Drop the length prefix; what follows is a .rcvbp file.
    let file = &flash[4..total];
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
