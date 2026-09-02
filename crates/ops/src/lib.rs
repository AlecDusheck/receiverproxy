//! The `rxp` commands as functions. `cli` wraps them in clap and prints
//! through [`Stdio`]; `daemon` runs them as jobs and collects their lines.
//! Every command takes a [`Ctx`] (the former global flags) and, when it has
//! anything to say, a `&mut dyn Progress`.

pub mod capture;
pub mod config;
pub mod display;
pub mod firmware;
pub mod flash;
pub mod ingest;
pub mod model;
pub mod params;
pub mod probe;
pub mod provision;
pub mod restore;
pub mod screen;
pub mod upgrade;
pub mod util;

pub use colorlight as protocol;
pub use panelspec;
pub use panelspec::{read_library, Loader};
pub use rcvbp;
pub use receivers;

use receivers::CardModel;

use anyhow::{Context, Result};
use std::io::{IsTerminal, Write};

/// The former global flags.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// Network interface directly connected to the receiving card.
    pub iface: String,
    /// Panel size used when no layout file is given.
    pub width: u16,
    pub height: u16,
    /// Color order on the wire.
    pub order: protocol::ColorOrder,
    /// Brightness 0-255, sent in sync frames.
    pub brightness: u8,
    /// The card's model: `--card`, the daemon's `card` setting, or what
    /// discovery returned; `None` until one of those has named it.
    pub model: Option<&'static CardModel>,
}

impl Ctx {
    /// The card model a flash or configuration command works from.
    ///
    /// # Errors
    /// Fails when nothing has named the card yet.
    pub fn model(&self) -> Result<&'static CardModel> {
        self.model
            .context("no card model: no card discovered and none named (--card NAME)")
    }
}

/// Where a command's lines go. `out` is what the CLI prints to stdout, `err`
/// what it prints to stderr: progress, plans, warnings.
pub trait Progress {
    fn out(&mut self, line: &str);
    fn err(&mut self, line: &str);
    /// A line that replaces the previous transient one: the `N frames, F fps`
    /// counter of `show video`. A terminal redraws it; a job keeps every one.
    fn transient(&mut self, line: &str) {
        self.err(line);
    }
    /// The transient line is finished with.
    fn clear_transient(&mut self) {}
    /// True once the caller wants the command to stop; polled between steps.
    fn cancelled(&self) -> bool {
        false
    }
}

/// The CLI's sink: `println!` and `eprintln!`, the transient line redrawn
/// with `\r` when stderr is a terminal and dropped otherwise.
pub struct Stdio;

impl Progress for Stdio {
    fn out(&mut self, line: &str) {
        println!("{line}");
    }

    fn err(&mut self, line: &str) {
        eprintln!("{line}");
    }

    fn transient(&mut self, line: &str) {
        let mut stderr = std::io::stderr();
        if stderr.is_terminal() {
            let _ = write!(stderr, "\r{line}");
        }
    }

    fn clear_transient(&mut self) {
        let mut stderr = std::io::stderr();
        if stderr.is_terminal() {
            let _ = write!(stderr, "\r");
        }
    }
}

/// Fails with `cancelled` once the sink asks the command to stop.
///
/// # Errors
/// When `p.cancelled()` is true.
pub fn check(p: &dyn Progress) -> Result<()> {
    anyhow::ensure!(!p.cancelled(), "cancelled");
    Ok(())
}

