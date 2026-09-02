use crate::flash::{dump_flash, dump_range, restore_flash, scan_flash};
use crate::{restore, Cli};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum Flash {
    /// Dump one or more 64KB flash blocks to a file
    Dump {
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
    /// List the 64KB flash blocks that hold data and what they look like
    Scan {
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
    /// Write a previously dumped 64KB block image back to the parameter block
    RestoreBlock {
        /// The 65536-byte block image
        image: String,
        /// Write it; without this only the plan is printed
        #[arg(long)]
        commit: bool,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
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
    /// Restore the configuration and screen record from a snapshot
    Restore {
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

pub fn run(cli: &Cli, cmd: &Flash) -> Result<()> {
    match cmd {
        Flash::Dump {
            out,
            block,
            blocks,
            index,
            wait,
        } => dump_flash(cli, *block, *blocks, *index, *wait, out),
        Flash::DumpRange {
            out,
            start,
            len,
            index,
            wait,
        } => dump_range(cli, start, len, *index, *wait, out),
        Flash::Scan {
            first,
            last,
            index,
            wait,
        } => scan_flash(cli, *first, *last, *index, *wait),
        Flash::RestoreBlock {
            image,
            commit,
            index,
        } => restore_flash(cli, image, *commit, *index),
        Flash::Snapshot { dir, index, wait } => restore::snapshot(cli, dir, *index, *wait),
        Flash::Restore {
            dir,
            commit,
            index,
            wait,
        } => restore::all(cli, dir, *commit, *index, *wait),
    }
}
