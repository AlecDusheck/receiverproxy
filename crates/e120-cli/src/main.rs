mod capture;
mod config;
mod display;
mod flash;
mod params;
mod provision;
mod restore;
mod screen;
mod upgrade;
mod util;

use capture::{discover, listen, pcap_summary, raw_send, replay};
use config::{config_build, config_diff, rcvbp_info};
use display::{play, probe, show_image, show_pattern, show_solid};
use flash::{
    dump_flash, dump_range, flash_firmware, read_config, restore_flash, scan_flash, write_config,
};
use params::send_params;
use util::{open, parse_color};

use anyhow::{Context, Result};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use e120_proto as protocol;
use e120_rcvbp as rcvbp;
use protocol::ColorOrder;
use std::process::ExitCode;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "e120",
    about = "Drive a Colorlight receiving card over raw Ethernet"
)]
struct Cli {
    // display_order puts the globals after each subcommand's own options.
    /// Network interface directly connected to the receiving card
    #[arg(short, long, global = true, display_order = 1000, default_value = "en24")]
    iface: String,

    /// Panel width in pixels
    #[arg(long, global = true, display_order = 1001, default_value_t = 128)]
    width: u16,

    /// Panel height in pixels
    #[arg(long, global = true, display_order = 1002, default_value_t = 64)]
    height: u16,

    /// Color order on the wire
    #[arg(long, global = true, display_order = 1003, default_value = "bgr")]
    order: ColorOrder,

    /// Brightness 0-255 (sent in sync frames)
    #[arg(short, long, global = true, display_order = 1004, default_value_t = 255)]
    brightness: u8,

    #[command(subcommand)]
    cmd: Cmd,
}

/// Parse two `u16`s split by one of `seps`; `what` names the expected form.
fn parse_pair(s: &str, seps: &[char], what: &str) -> Result<(u16, u16), String> {
    let (a, b) = s
        .split_once(seps)
        .ok_or_else(|| format!("expected {what}, got {s:?}"))?;
    let parse = |v: &str| {
        v.trim()
            .parse::<u16>()
            .map_err(|e| format!("{what} in {s:?}: {e}"))
    };
    Ok((parse(a)?, parse(b)?))
}

/// Parse a `WIDTHxHEIGHT` geometry argument.
fn parse_geometry(s: &str) -> Result<(u16, u16), String> {
    parse_pair(s, &['x', 'X'], "WIDTHxHEIGHT")
}

/// Parse an `x,y` position argument.
fn parse_position(s: &str) -> Result<(u16, u16), String> {
    parse_pair(s, &[','], "x,y")
}

#[derive(Subcommand)]
enum UpgradeWhat {
    /// Report the image the card expects and its upgrade capabilities
    Info {
        /// Seconds to wait for the reply
        #[arg(long, default_value_t = 4)]
        wait: u64,
    },
    /// Install a firmware image through the card's SDRAM staging
    Install {
        /// The .hex bitstream to install
        image: String,
        /// Send it; without this only the plan is printed
        #[arg(long)]
        commit: bool,
        /// Target the golden backup instead of the primary image
        #[arg(long)]
        golden: bool,
        /// Seconds to wait for the card to finish programming
        #[arg(long, default_value_t = 120)]
        timeout: u64,
        /// Microseconds between upload chunks; too fast and the card drops them
        #[arg(long, default_value_t = 3000)]
        chunk_delay_us: u64,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 4)]
        wait: u64,
    },
}

#[derive(Subcommand)]
enum RestoreWhat {
    /// Restore the configuration and screen record from a snapshot
    All {
        /// Snapshot directory
        #[arg(long, default_value = "snapshot")]
        dir: String,
        /// Write it; without this only the plan is printed
        #[arg(long)]
        commit: bool,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Seconds to wait for each reply
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
    /// Dump every frame seen on the interface
    Listen {
        /// Seconds to listen
        #[arg(long, default_value_t = 10)]
        wait: u64,
        /// Also show frames we transmit, to confirm they reach the wire
        #[arg(long)]
        include_ours: bool,
    },
    /// Set panel brightness (0-255)
    Brightness {
        /// 0-255
        value: u8,
    },
    /// Fill the panel with a solid color, e.g. `fill ff0000` or `fill 255 0 0`
    Fill {
        /// RRGGBB hex, or three 0-255 values
        #[arg(required = true)]
        color: Vec<String>,
        /// Keep refreshing until Ctrl-C, so a meter can settle on the draw
        #[arg(long)]
        hold: bool,
    },
    /// Send parts of a refresh with explicit pacing
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
        /// gradient | rows | border | rgb | white
        #[arg(default_value = "gradient")]
        pattern: String,
        /// Keep refreshing until Ctrl-C
        #[arg(long)]
        hold: bool,
    },
    /// Display an image file (scaled to panel size)
    Image {
        /// Any image format the `image` crate reads
        path: String,
        /// Keep refreshing until Ctrl-C
        #[arg(long)]
        hold: bool,
    },
    /// Bring a card to a working state: snapshot, firmware, config, EEPROM, verify
    Provision {
        /// Panel spec, see config/panels/*.toml
        #[arg(long)]
        spec: String,
        /// Vendor firmware image to install (skipped when absent)
        #[arg(long)]
        firmware: Option<String>,
        /// Cabinet position in the whole screen, "x,y" in pixels
        #[arg(long, default_value = "0,0", value_parser = parse_position)]
        position: (u16, u16),
        /// Directory for the pre-provisioning snapshot [default: build/snapshot-<time>]
        #[arg(long)]
        snapshot_dir: Option<String>,
        /// Write it; without this only the plan is printed
        #[arg(long)]
        commit: bool,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Blank the panel
    Blank,
    /// Save the card's stored configuration as a .rcvbp file
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
        /// A prior dump of the primary bank, kept as the recovery path
        #[arg(long)]
        backup: String,
        /// Write it; without this only the plan is printed
        #[arg(long)]
        commit: bool,
        /// First 64KB block to write, so a partial image can be repaired
        #[arg(long, default_value_t = 0x00)]
        from_block: u8,
        /// One past the last block to write
        #[arg(long, default_value_t = 0x0b)]
        to_block: u8,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// List the 64KB flash blocks that hold data and what they look like
    ScanFlash {
        /// First block to scan
        #[arg(long, default_value_t = 0)]
        first: u8,
        /// Last block to scan
        #[arg(long, default_value_t = 255)]
        last: u8,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 1)]
        wait: u64,
    },
    /// Send a hand-built frame and print any reply
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
    /// Dump a flash address range, firmware included, to a file
    DumpRange {
        /// Output file
        #[arg(long, default_value = "flash.bin")]
        out: String,
        /// Start address, hex
        #[arg(long, default_value = "0")]
        start: String,
        /// Bytes to read, hex
        #[arg(long, default_value = "100000")]
        len: String,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 2)]
        wait: u64,
    },
    /// Dump one or more 64KB flash blocks to a file
    DumpFlash {
        /// Output file
        #[arg(long, default_value = "block07.bin")]
        out: String,
        /// First 64KB block; 0x07 holds parameters, 0x00 onward holds firmware
        #[arg(long, default_value_t = 7)]
        block: u8,
        /// How many consecutive blocks to read
        #[arg(long, default_value_t = 1)]
        blocks: u16,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 2)]
        wait: u64,
    },
    /// Write a .rcvbp into the card's parameter flash (read-modify-write)
    WriteConfig {
        /// The .rcvbp to install
        config: String,
        /// Write it; without this only the plan is printed
        #[arg(long)]
        commit: bool,
        /// Where to save the pre-write backup of the whole block
        #[arg(long, default_value = "block07-backup.bin")]
        backup: String,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 2)]
        wait: u64,
    },
    /// Play a video (or any ffmpeg-readable source) on the wall
    Play {
        /// File path or URL
        input: String,
        /// Frames per second
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// stretch | contain | cover
        #[arg(long, default_value = "contain")]
        fit: String,
        /// Loop forever
        #[arg(long = "loop", alias = "looping")]
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
        /// Keep refreshing until Ctrl-C
        #[arg(long)]
        hold: bool,
        /// Wall layout JSON; defaults to a single panel of --width x --height
        #[arg(long)]
        layout: Option<String>,
    },
    /// Print an example wall layout to adapt
    LayoutExample,
    /// Tell the card its own size and the size of the whole screen
    SetLayout {
        /// Panel width in pixels
        #[arg(long, default_value_t = 128)]
        panel_width: u16,
        /// Panel height in pixels
        #[arg(long, default_value_t = 64)]
        panel_height: u16,
        /// Receiver index on the chain
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
    /// Select the card's built-in test pattern (needs no pixel data)
    TestMode {
        /// Pattern selector; 0 is normal/off
        pattern: u8,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
    },
    /// Step through the card's test patterns, pausing on each
    TestSweep {
        /// Patterns to step through, starting at 0
        #[arg(long, default_value_t = 16)]
        count: u8,
        /// Seconds to pause on each
        #[arg(long, default_value_t = 2)]
        secs: u64,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
    },
    /// Ask the card to reload its parameters from flash
    ReloadParams {
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Send the vendor's post-save frame (opcode 0x77, all three classes) instead of the bare 0x79 reload
        #[arg(long)]
        full: bool,
    },
    /// Print, and optionally set, the card's screen-size record
    ScreenSize {
        /// New geometry as WIDTHxHEIGHT, e.g. 128x64
        #[arg(long, value_parser = parse_geometry)]
        set: Option<(u16, u16)>,
        /// Write it; without this only the plan is printed
        #[arg(long)]
        commit: bool,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Save the primary and golden firmware banks into a directory
    Snapshot {
        /// Output directory (created if missing)
        #[arg(long, default_value = "snapshot")]
        dir: String,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Install firmware through the card's own upgrade path
    Upgrade {
        #[command(subcommand)]
        what: UpgradeWhat,
    },
    /// Put the card back from a snapshot
    Restore {
        #[command(subcommand)]
        what: RestoreWhat,
    },
    /// Write a previously dumped 64KB block image back to the parameter block
    RestoreFlash {
        /// The 65536-byte block image
        image: String,
        /// Write it; without this only the plan is printed
        #[arg(long)]
        commit: bool,
        /// Receiver index on the chain
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
    ConfigDiff {
        /// First .rcvbp
        a: String,
        /// Second .rcvbp
        b: String,
    },
    /// List the records in a .rcvbp receiver-parameter file
    Rcvbp {
        /// The .rcvbp to list
        path: String,
        /// Hexdump each record's payload (non-empty ones)
        #[arg(long)]
        dump: bool,
    },
    /// Summarize Colorlight packet types in a pcap capture
    PcapSummary {
        /// Classic pcap capture (pcapng is rejected)
        path: String,
        /// Show full hexdumps of non-pixel packets
        #[arg(long)]
        dump: bool,
    },
    /// Replay sender->card frames from a pcap capture
    Replay {
        /// Classic pcap capture (pcapng is rejected)
        path: String,
        /// Comma-separated packet-type bytes (hex) to replay, e.g. "10,11,1f,26" [default: all non-pixel config types]
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

/// The subcommand path as typed, e.g. `upgrade install`, for error prefixes.
fn subcommand_path(m: &clap::ArgMatches) -> String {
    let mut path = Vec::new();
    let mut m = m;
    while let Some((name, sub)) = m.subcommand() {
        path.push(name);
        m = sub;
    }
    path.join(" ")
}

#[allow(unsafe_code)] // one libc call, before any thread exists
fn main() -> ExitCode {
    // Die quietly when a pipe closes (`e120 rcvbp f | head`), like other unix tools.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let matches = Cli::command().get_matches();
    let subject = subcommand_path(&matches);
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match run(&cli).with_context(|| subject) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("e120: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[allow(clippy::too_many_lines)]
fn run(cli: &Cli) -> Result<()> {
    match &cli.cmd {
        // Pixels.
        Cmd::Probe {
            rows,
            row_gap_us,
            sync,
            repeat,
            color,
        } => {
            let rgb = parse_color(std::slice::from_ref(color))?;
            probe(cli, *rows, *row_gap_us, *sync, *repeat, rgb)
        }
        Cmd::Fill { color, hold } => show_solid(cli, parse_color(color)?, *hold),
        Cmd::Test { pattern, hold } => show_pattern(cli, pattern, *hold, None),
        Cmd::Image { path, hold } => show_image(cli, path, *hold),
        Cmd::Blank => show_solid(cli, [0, 0, 0], false),
        Cmd::Brightness { value } => {
            let mut dev = open(cli)?;
            dev.send(&protocol::brightness(*value))?;
            dev.send(&protocol::sync(*value))?;
            Ok(())
        }
        Cmd::Play {
            input,
            fps,
            fit,
            looping,
            layout,
        } => play(cli, input, *fps, fit, *looping, layout.as_deref()),
        Cmd::Pattern { name, hold, layout } => show_pattern(cli, name, *hold, layout.as_deref()),
        Cmd::LayoutExample => {
            let canvas = e120_canvas::Canvas::grid(128, 64, 2, 1);
            println!("{}", serde_json::to_string_pretty(&canvas)?);
            Ok(())
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
            Ok(())
        }

        // Provisioning, firmware, snapshots.
        Cmd::Provision {
            spec,
            firmware,
            position,
            snapshot_dir,
            commit,
            wait,
        } => provision::provision(
            cli,
            spec,
            firmware.as_deref(),
            *position,
            snapshot_dir.as_deref(),
            *commit,
            *wait,
        ),
        Cmd::Upgrade { what } => match what {
            UpgradeWhat::Info { wait } => upgrade::info(cli, *wait),
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
            }
        },
        Cmd::Snapshot { dir, index, wait } => restore::snapshot(cli, dir, *index, *wait),
        Cmd::Restore { what } => match what {
            RestoreWhat::All {
                dir,
                commit,
                index,
                wait,
            } => restore::all(cli, dir, *commit, *index, *wait),
        },

        // Flash and EEPROM.
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
            blocks,
            index,
            wait,
        } => dump_flash(cli, *block, *blocks, *index, *wait, out),
        Cmd::DumpRange {
            out,
            start,
            len,
            index,
            wait,
        } => dump_range(cli, start, len, *index, *wait, out),
        Cmd::ScanFlash {
            first,
            last,
            index,
            wait,
        } => scan_flash(cli, *first, *last, *index, *wait),
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
        ),
        Cmd::WriteConfig {
            config,
            commit,
            backup,
            index,
            wait,
        } => write_config(cli, config, *commit, backup, None, *index, *wait),
        Cmd::ScreenSize {
            set,
            commit,
            index,
            wait,
        } => screen::screen_size(cli, *set, *commit, *index, *wait),
        Cmd::RestoreFlash {
            image,
            commit,
            index,
        } => restore_flash(cli, image, *commit, *index),

        // Parameters and the card's own test modes.
        Cmd::SendParams {
            spec,
            chip_only,
            gap_ms,
        } => send_params(cli, spec, *chip_only, *gap_ms),
        Cmd::TestMode { pattern, index } => {
            let mut dev = open(cli)?;
            dev.send(&protocol::test_mode(*index, *pattern))?;
            Ok(())
        }
        Cmd::TestSweep { count, secs, index } => {
            let mut dev = open(cli)?;
            for pattern in 0..*count {
                println!("pattern {pattern}");
                dev.send(&protocol::test_mode(*index, pattern))?;
                std::thread::sleep(Duration::from_secs(*secs));
            }
            dev.send(&protocol::test_mode(*index, 0))?;
            Ok(())
        }
        Cmd::ReloadParams { index, full } => {
            let mut dev = open(cli)?;
            if *full {
                dev.send(&protocol::reload_params_full(*index))?;
            } else {
                dev.send(&protocol::reload_params(*index))?;
            }
            Ok(())
        }

        // Wire diagnostics and config files.
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
    }
}
