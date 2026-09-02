mod capture;
mod cli;
mod config;
mod display;
mod flash;
mod ingest;
mod params;
mod provision;
mod restore;
mod screen;
mod upgrade;
mod util;

use anyhow::{Context, Result};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use e120_proto as protocol;
use e120_rcvbp as rcvbp;
use protocol::ColorOrder;
use std::process::ExitCode;
use util::open;

#[derive(Parser)]
#[command(
    name = "e120",
    about = "Drive a Colorlight receiving card over raw Ethernet"
)]
struct Cli {
    // display_order puts the globals after each subcommand's own options.
    /// Network interface directly connected to the receiving card
    #[arg(
        short,
        long,
        global = true,
        display_order = 1000,
        default_value = "en24"
    )]
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
    #[arg(
        short,
        long,
        global = true,
        display_order = 1004,
        default_value_t = 255
    )]
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
enum Cmd {
    /// Send a discovery packet and print any card that answers
    Discover {
        /// Seconds to listen for responses
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Set panel brightness (0-255)
    Brightness {
        /// 0-255
        value: u8,
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
    /// Put pixels on the panel: images, video, streams, patterns
    #[command(subcommand)]
    Show(cli::show::Show),
    /// Generate, inspect and transfer .rcvbp configurations
    #[command(subcommand)]
    Config(cli::config::Config),
    /// Read and write the card's flash, and snapshot or restore it
    #[command(subcommand)]
    Flash(cli::flash::Flash),
    /// Install FPGA firmware
    #[command(subcommand)]
    Firmware(cli::firmware::Firmware),
    /// Card state held in RAM or EEPROM: layout, screen size, test modes
    #[command(subcommand)]
    Card(cli::card::Card),
    /// Wire diagnostics: listen, hand-built frames, pcap tools
    #[command(subcommand)]
    Debug(cli::debug::Debug),
}

/// The subcommand path as typed, e.g. `firmware install`, for error prefixes.
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
    // Die quietly when a pipe closes (`e120 config info f | head`), like other unix tools.
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

fn run(cli: &Cli) -> Result<()> {
    match &cli.cmd {
        Cmd::Discover { wait } => capture::discover(cli, *wait),
        Cmd::Brightness { value } => {
            let mut dev = open(cli)?;
            dev.send(&protocol::brightness(*value))?;
            dev.send(&protocol::sync(*value))?;
            Ok(())
        }
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
        Cmd::Show(c) => cli::show::run(cli, c),
        Cmd::Config(c) => cli::config::run(cli, c),
        Cmd::Flash(c) => cli::flash::run(cli, c),
        Cmd::Firmware(c) => cli::firmware::run(cli, c),
        Cmd::Card(c) => cli::card::run(cli, c),
        Cmd::Debug(c) => cli::debug::run(cli, c),
    }
}
