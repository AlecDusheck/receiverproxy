mod capture;
mod config;
mod display;
mod flash;
mod params;
mod restore;
mod screen;
mod upgrade;
mod util;

use capture::{discover, listen, pcap_summary, raw_send, replay};
use config::{config_build, config_diff, rcvbp_info};
use display::{play, probe, show, show_pattern, solid, test_pattern};
use flash::{
    dump_flash, dump_range, flash_firmware, read_config, restore_flash, scan_flash,
    upgrade_info, write_config,
};
use params::send_params;
use util::{open, parse_color};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use e120_proto as protocol;
use e120_rcvbp as rcvbp;
use protocol::ColorOrder;
use std::time::Duration;

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

/// Parse a `WIDTHxHEIGHT` geometry argument.
fn parse_geometry(s: &str) -> Result<(u16, u16), String> {
    let (w, h) = s
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("expected WIDTHxHEIGHT, got {s:?}"))?;
    let parse = |v: &str, what: &str| {
        v.trim()
            .parse::<u16>()
            .map_err(|e| format!("{what} in {s:?}: {e}"))
    };
    Ok((parse(w, "width")?, parse(h, "height")?))
}

#[derive(Subcommand)]
enum UpgradeWhat {
    /// Ask the card what image it expects and how it can be upgraded
    Info {
        #[arg(long, default_value_t = 4)]
        wait: u64,
    },
    /// Install a firmware image via the card's own SDRAM staging
    Install {
        image: String,
        /// Actually send it. Without this it only reports what it would do.
        #[arg(long)]
        commit: bool,
        /// Target the golden backup instead of the primary image
        #[arg(long)]
        golden: bool,
        /// Seconds to wait for the card to finish programming
        #[arg(long, default_value_t = 120)]
        timeout: u64,
        /// Microseconds between upload chunks. Too fast and the card drops them.
        #[arg(long, default_value_t = 3000)]
        chunk_delay_us: u64,
        #[arg(long, default_value_t = 4)]
        wait: u64,
    },
}

#[derive(Subcommand)]
enum RestoreWhat {
    /// Rewrite the primary firmware bank from a saved image
    Firmware {
        image: String,
        #[arg(long)]
        commit: bool,
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Restore firmware, configuration and screen record from a snapshot
    All {
        #[arg(long, default_value = "snapshot")]
        dir: String,
        #[arg(long)]
        commit: bool,
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
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
        /// Also show frames we transmit, to confirm they reach the wire
        #[arg(long)]
        include_ours: bool,
    },
    /// Set panel brightness (0-255)
    Brightness { value: u8 },
    /// Fill the panel with a solid color, e.g. `fill ff0000` or `fill 255 0 0`
    Fill {
        color: Vec<String>,
        /// Keep refreshing until Ctrl-C, so a meter can settle on the draw
        #[arg(long)]
        hold: bool,
    },
    /// Send pieces of a refresh with explicit pacing (diagnosis)
    Probe {
        /// Rows to send, starting at 0
        #[arg(long, default_value_t = 64)]
        rows: u16,
        /// Microseconds between row frames; 0 = back to back
        #[arg(long, default_value_t = 0)]
        row_gap_us: u64,
        /// Send a sync/vsync frame after the rows
        #[arg(long)]
        sync: bool,
        /// Repeat the whole pass this many times, 33ms apart
        #[arg(long, default_value_t = 1)]
        repeat: u32,
        /// Solid colour as RRGGBB
        #[arg(long, default_value = "ffffff")]
        color: String,
    },
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
    /// Write an FPGA bitstream into the card's primary firmware bank
    FlashFirmware {
        /// The .hex bitstream to install
        image: String,
        /// A prior dump of the primary bank, required as a recovery path
        #[arg(long)]
        backup: String,
        /// Actually write. Without this it only reports what it would do.
        #[arg(long)]
        commit: bool,
        /// First 64KB block to write. Lets a partial image be repaired.
        #[arg(long, default_value_t = 0x00)]
        from_block: u8,
        /// One past the last block to write.
        #[arg(long, default_value_t = 0x0b)]
        to_block: u8,
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Ask the card what firmware image it expects (read-only)
    UpgradeInfo {
        #[arg(long, default_value_t = 4)]
        wait: u64,
    },
    /// Scan every 64KB flash block for known signatures (read-only)
    ScanFlash {
        #[arg(long, default_value_t = 0)]
        first: u8,
        #[arg(long, default_value_t = 255)]
        last: u8,
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 1)]
        wait: u64,
    },
    /// Send a hand-built frame and show any reply (experimentation)
    RawSend {
        /// Two type bytes, hex, e.g. 1900
        #[arg(long)]
        r#type: String,
        /// Payload after the type bytes, hex; padded with zeros to --pad
        #[arg(long, default_value = "")]
        payload: String,
        /// Zero-pad the payload to this many bytes
        #[arg(long, default_value_t = 126)]
        pad: usize,
        /// Seconds to listen for a reply
        #[arg(long, default_value_t = 2)]
        wait: u64,
        /// Bytes of reply to hexdump
        #[arg(long, default_value_t = 64)]
        show: usize,
    },
    /// Dump an arbitrary flash range, including firmware (read-only)
    DumpRange {
        #[arg(long, default_value = "flash.bin")]
        out: String,
        /// Start address, hex
        #[arg(long, default_value = "0")]
        start: String,
        /// Bytes to read, hex
        #[arg(long, default_value = "100000")]
        len: String,
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 2)]
        wait: u64,
    },
    /// Dump one or more 64KB flash blocks from the card (read-only)
    DumpFlash {
        #[arg(long, default_value = "block07.bin")]
        out: String,
        /// First 64KB block; 0x07 holds parameters, 0x00 onward holds firmware
        #[arg(long, default_value_t = 7)]
        block: u8,
        /// How many consecutive blocks to read
        #[arg(long, default_value_t = 1)]
        blocks: u16,
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
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 2)]
        wait: u64,
    },
    /// Play a video (or any ffmpeg-readable source) on the wall
    Play {
        /// File path or URL
        input: String,
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// stretch | contain | cover
        #[arg(long, default_value = "contain")]
        fit: String,
        /// Loop forever
        #[arg(long, name = "loop")]
        looping: bool,
        /// Wall layout JSON; defaults to a single panel of --width x --height
        #[arg(long)]
        layout: Option<String>,
    },
    /// Show a built-in pattern through the wall pipeline
    Pattern {
        /// rgb | border | rows | gradient | white
        #[arg(default_value = "rgb")]
        name: String,
        #[arg(long)]
        hold: bool,
        #[arg(long)]
        layout: Option<String>,
    },
    /// Print an example wall layout to adapt
    LayoutExample,
    /// Tell the card its own size and the size of the whole screen
    SetLayout {
        #[arg(long, default_value_t = 128)]
        panel_width: u16,
        #[arg(long, default_value_t = 64)]
        panel_height: u16,
        #[arg(long, default_value_t = 0)]
        index: u16,
    },
    /// Push a panel spec's parameters into the card's RAM (no flash, no reboot)
    SendParams {
        /// Panel spec, see config/panels/*.toml
        #[arg(long)]
        spec: String,
        /// Send only the chip-register pack (arms the drivers)
        #[arg(long)]
        chip_only: bool,
        /// Milliseconds between packs
        #[arg(long, default_value_t = 8)]
        gap_ms: u64,
    },
    /// Run the card's built-in test pattern (needs no pixel data from us)
    TestMode {
        /// Pattern selector; 0 is normal/off
        pattern: u8,
        #[arg(long, default_value_t = 0)]
        index: u16,
    },
    /// Sweep every test pattern, pausing on each
    TestSweep {
        #[arg(long, default_value_t = 16)]
        count: u8,
        #[arg(long, default_value_t = 2)]
        secs: u64,
        #[arg(long, default_value_t = 0)]
        index: u16,
    },
    /// Ask the card to reload parameters from flash
    ReloadParams {
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Use the vendor's post-save frame (opcode 0x77, all three classes)
        /// instead of the bare 0x79 reload
        #[arg(long)]
        full: bool,
    },
    /// Show, and optionally set, the card's screen-size record
    ScreenSize {
        /// New geometry as WIDTHxHEIGHT, e.g. 128x64
        #[arg(long, value_parser = parse_geometry)]
        set: Option<(u16, u16)>,
        #[arg(long)]
        commit: bool,
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Capture everything we know how to restore into a directory
    Snapshot {
        #[arg(long, default_value = "snapshot")]
        dir: String,
        #[arg(long, default_value_t = 0)]
        index: u16,
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Install firmware the way the card supports
    Upgrade {
        #[command(subcommand)]
        what: UpgradeWhat,
    },
    /// Put the card back the way it was
    Restore {
        #[command(subcommand)]
        what: RestoreWhat,
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
    /// Generate a config (.rcvbp + boot image) from a panel spec (TOML)
    GenConfig {
        /// Panel spec, see config/panels/*.toml
        #[arg(long)]
        spec: String,
        /// Directory for the outputs (created if missing)
        #[arg(long, default_value = "build")]
        out_dir: String,
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
        Cmd::Probe {
            rows,
            row_gap_us,
            sync,
            repeat,
            color,
        } => {
            let c = u32::from_str_radix(color, 16)?;
            let rgb = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
            probe(cli, *rows, *row_gap_us, *sync, *repeat, rgb).map(Some)
        }
        Cmd::Fill { color, hold } => {
            let (r, g, b) = parse_color(color)?;
            let fb = solid(cli, r, g, b);
            show(cli, &fb, *hold).map(Some)
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
        Cmd::Play {
            input,
            fps,
            fit,
            looping,
            layout,
        } => play(cli, input, *fps, fit, *looping, layout.as_deref()).map(Some),
        Cmd::Pattern { name, hold, layout } => {
            show_pattern(cli, name, *hold, layout.as_deref()).map(Some)
        }
        Cmd::LayoutExample => {
            let canvas = e120_canvas::Canvas::grid(128, 64, 2, 1);
            println!("{}", serde_json::to_string_pretty(&canvas)?);
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

/// Commands that read or write the card's flash and EEPROM.
/// Commands that install or restore firmware.
fn run_firmware(cli: &Cli) -> Result<Option<()>> {
    match &cli.cmd {
        Cmd::Upgrade { what } => match what {
            UpgradeWhat::Info { wait } => upgrade::info(cli, *wait).map(Some),
            UpgradeWhat::Install {
                image,
                commit,
                golden,
                timeout,
                chunk_delay_us,
                wait,
            } => {
                let partition = if *golden {
                    protocol::upgrade::Partition::Golden
                } else {
                    protocol::upgrade::Partition::Primary
                };
                upgrade::install(
                    cli,
                    image,
                    *commit,
                    partition,
                    *timeout,
                    *chunk_delay_us,
                    *wait,
                )
                .map(Some)
            }
        },
        Cmd::Snapshot { dir, index, wait } => restore::snapshot(cli, dir, *index, *wait).map(Some),
        Cmd::Restore { what } => match what {
            RestoreWhat::Firmware {
                image,
                commit,
                index,
                wait,
            } => restore::firmware(cli, image, *commit, *index, *wait).map(Some),
            RestoreWhat::All {
                dir,
                commit,
                index,
                wait,
            } => restore::all(cli, dir, *commit, *index, *wait).map(Some),
        },
        _ => Ok(None),
    }
}

fn run_flash(cli: &Cli) -> Result<Option<()>> {
    match &cli.cmd {
        Cmd::ReadConfig {
            out,
            index,
            page,
            max_chunks,
            wait,
        } => read_config(cli, *index, *page, *max_chunks, *wait, out).map(Some),
        Cmd::DumpFlash {
            out,
            block,
            blocks,
            index,
            wait,
        } => dump_flash(cli, *block, *blocks, *index, *wait, out).map(Some),
        Cmd::DumpRange {
            out,
            start,
            len,
            index,
            wait,
        } => dump_range(cli, start, len, *index, *wait, out).map(Some),
        Cmd::ScanFlash {
            first,
            last,
            index,
            wait,
        } => scan_flash(cli, *first, *last, *index, *wait).map(Some),
        Cmd::UpgradeInfo { wait } => upgrade_info(cli, *wait).map(Some),
        Cmd::FlashFirmware {
            image,
            backup,
            commit,
            from_block,
            to_block,
            index,
            wait,
        } => flash_firmware(
            cli,
            image,
            backup,
            *commit,
            *from_block..*to_block,
            *index,
            *wait,
        )
        .map(Some),
        Cmd::WriteConfig {
            config,
            commit,
            backup,
            index,
            wait,
        } => write_config(cli, config, *commit, backup, None, *index, *wait).map(Some),
        Cmd::ScreenSize {
            set,
            commit,
            index,
            wait,
        } => screen::screen_size(cli, *set, *commit, *index, *wait).map(Some),
        Cmd::RestoreFlash {
            image,
            commit,
            index,
        } => restore_flash(cli, image, *commit, *index).map(Some),
        _ => Ok(None),
    }
}

/// Commands that push parameters or run the card's own test modes.
fn run_params(cli: &Cli) -> Result<Option<()>> {
    match &cli.cmd {
        Cmd::SendParams {
            spec,
            chip_only,
            gap_ms,
        } => send_params(cli, spec, *chip_only, *gap_ms).map(Some),
        Cmd::TestMode { pattern, index } => {
            let mut dev = open(cli)?;
            dev.send(&protocol::test_mode(*index, *pattern))?;
            println!("test pattern {pattern} selected");
            Ok(Some(()))
        }
        Cmd::TestSweep { count, secs, index } => {
            let mut dev = open(cli)?;
            for pattern in 0..*count {
                println!("pattern {pattern} (0x{pattern:02x})");
                dev.send(&protocol::test_mode(*index, pattern))?;
                std::thread::sleep(Duration::from_secs(*secs));
            }
            dev.send(&protocol::test_mode(*index, 0))?;
            println!("back to normal");
            Ok(Some(()))
        }
        Cmd::ReloadParams { index, full } => {
            let mut dev = open(cli)?;
            if *full {
                dev.send(&protocol::reload_params_full(*index))?;
                println!("sent the vendor's full reload (opcode 0x77, all classes)");
            } else {
                dev.send(&protocol::reload_params(*index))?;
                println!("asked the card to reload parameters from flash");
            }
            Ok(Some(()))
        }
        _ => Ok(None),
    }
}

fn run(cli: &Cli) -> Result<()> {
    for dispatch in [run_display, run_flash, run_firmware, run_params] {
        if dispatch(cli)?.is_some() {
            return Ok(());
        }
    }
    match &cli.cmd {
        Cmd::Discover { wait } => discover(cli, *wait),
        Cmd::Listen { wait, include_ours } => listen(cli, *wait, *include_ours),
        Cmd::RawSend {
            r#type,
            payload,
            pad,
            wait,
            show,
        } => raw_send(cli, r#type, payload, *pad, *wait, *show),
        Cmd::ConfigBuild {
            base,
            copy_from,
            copy,
            remove,
            out,
        } => config_build(base, copy_from.as_deref(), copy, remove, out),
        Cmd::GenConfig { spec, out_dir } => config::gen_config(spec, out_dir),
        Cmd::ConfigDiff { a, b } => config_diff(a, b),
        Cmd::Rcvbp { path, dump } => rcvbp_info(path, *dump),
        Cmd::PcapSummary { path, dump } => pcap_summary(path, *dump),
        Cmd::Replay {
            path,
            types,
            gap_us,
            all,
        } => replay(cli, path, types.as_deref(), *gap_us, *all),
        _ => unreachable!("handled by a dispatcher above"),
    }
}
