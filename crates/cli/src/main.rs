mod cli;

use anyhow::{Context, Result};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use ops::{capture, display, protocol, provision, Ctx, Progress, Stdio};
use protocol::ColorOrder;
use std::process::ExitCode;

#[derive(Parser)]
// bin_name keeps the usage line `rxp` when the binary is run as `receiverproxy`.
#[command(
    name = "rxp",
    bin_name = "rxp",
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

    /// Card model to work from instead of the one discovery reports (`rxp card models`)
    #[arg(long, global = true, display_order = 1005, value_name = "NAME")]
    card: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

impl Cli {
    fn ctx(&self) -> Result<Ctx> {
        Ok(Ctx {
            iface: self.iface.clone(),
            width: self.width,
            height: self.height,
            order: self.order,
            brightness: self.brightness,
            model: self.card.as_deref().map(ops::model::named).transpose()?,
        })
    }
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
        /// Firmware image to install, a name from config/firmware.toml or a path (skipped when absent)
        #[arg(long)]
        firmware: Option<String>,
        /// Cabinet position in the whole screen, "x,y" in pixels
        #[arg(long, default_value = "0,0", value_parser = parse_position)]
        position: (u16, u16),
        /// The card's position in the Ethernet chain; the EEPROM writes address it alone. Absent: broadcast, one card on the link
        #[arg(long)]
        index: Option<u16>,
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
    /// List, fetch and install FPGA firmware
    #[command(subcommand)]
    Firmware(cli::firmware::Firmware),
    /// Card state held in RAM or EEPROM: layout, screen size, test modes
    #[command(subcommand)]
    Card(cli::card::Card),
    /// Wire diagnostics: listen, hand-built frames, pcap tools
    #[command(subcommand)]
    Debug(cli::debug::Debug),
    /// Serve the web UI and its JSON API; the printed URL carries the token
    Ui {
        /// TCP port
        #[arg(long, default_value_t = 7120)]
        port: u16,
        /// Address to bind; anything but 127.0.0.1 exposes the daemon to that network
        #[arg(long, default_value_t = std::net::Ipv4Addr::LOCALHOST)]
        listen: std::net::Ipv4Addr,
        /// Do not open the browser
        #[arg(long)]
        no_open: bool,
        /// The token every API request must carry (X-Token); random when absent
        #[arg(long)]
        token: Option<String>,
        /// Where settings, the wall layout, backups and snapshots are kept [default: the OS config dir, receiverproxy/]
        #[arg(long)]
        data_dir: Option<String>,
    },
}

impl Cmd {
    /// Commands that read the card's memory map: without `--card` the model
    /// comes from a discovery first.
    fn needs_card(&self) -> bool {
        match self {
            Self::Config(c) => c.needs_card(),
            Self::Flash(c) => c.needs_card(),
            Self::Firmware(c) => c.needs_card(),
            Self::Card(c) => c.needs_card(),
            _ => false,
        }
    }
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
    // Die quietly when a pipe closes (`rxp config info f | head`), like other unix tools.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let matches = Cli::command().get_matches();
    let subject = subcommand_path(&matches);
    // `--iface` typed on the command line beats the daemon's saved setting.
    let iface_given = matches.value_source("iface") == Some(clap::parser::ValueSource::CommandLine);
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match run(&cli, iface_given).with_context(|| subject) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rxp: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli, iface_given: bool) -> Result<()> {
    let mut ctx = cli.ctx()?;
    if cli.cmd.needs_card() && ctx.model.is_none() {
        ctx.model = Some(ops::model::resolve(&ctx, 3)?);
    }
    let p: &mut dyn Progress = &mut Stdio;
    match &cli.cmd {
        Cmd::Discover { wait } => {
            let found = capture::discover(&ctx, *wait, p)?;
            anyhow::ensure!(
                !found.is_empty(),
                "no response on {} within {wait}s",
                ctx.iface
            );
            Ok(())
        }
        Cmd::Brightness { value } => display::brightness(&ctx, *value),
        Cmd::Provision {
            spec,
            firmware,
            position,
            index,
            snapshot_dir,
            commit,
            wait,
        } => provision::provision(
            &ctx,
            &provision::Args {
                spec_path: spec,
                firmware: firmware.as_deref(),
                position: *position,
                index: *index,
                snapshot_dir: snapshot_dir.as_deref(),
                commit: *commit,
                wait: *wait,
            },
            &ops::read_library,
            p,
        ),
        Cmd::Show(c) => cli::show::run(&ctx, c, p),
        Cmd::Config(c) => cli::config::run(&ctx, c, p),
        Cmd::Flash(c) => cli::flash::run(&ctx, c, p),
        Cmd::Firmware(c) => cli::firmware::run(&ctx, c, p),
        Cmd::Card(c) => cli::card::run(&ctx, c, p),
        Cmd::Debug(c) => cli::debug::run(&ctx, c, p),
        Cmd::Ui {
            port,
            listen,
            no_open,
            token,
            data_dir,
        } => daemon::run(daemon::Options {
            port: *port,
            listen: *listen,
            open: !*no_open,
            token: token.clone(),
            iface: iface_given.then(|| ctx.iface.clone()),
            data_dir: data_dir.as_deref().map(Into::into),
        }),
    }
}
