use crate::util::open;
use crate::{parse_geometry, protocol, screen, Cli};
use anyhow::Result;
use clap::Subcommand;
use std::time::Duration;

#[derive(Subcommand)]
pub enum Card {
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
    /// Ask the card to reload its parameters from flash
    Reload {
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Send the vendor's post-save frame (opcode 0x77, all three classes) instead of the bare 0x79 reload
        #[arg(long)]
        full: bool,
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
    /// Print an example wall layout to adapt
    LayoutExample,
}

pub fn run(cli: &Cli, cmd: &Card) -> Result<()> {
    match cmd {
        Card::ScreenSize {
            set,
            commit,
            index,
            wait,
        } => screen::screen_size(cli, *set, *commit, *index, *wait),
        Card::Reload { index, full } => {
            let mut dev = open(cli)?;
            if *full {
                dev.send(&protocol::reload_params_full(*index))?;
            } else {
                dev.send(&protocol::reload_params(*index))?;
            }
            Ok(())
        }
        Card::TestMode { pattern, index } => {
            let mut dev = open(cli)?;
            dev.send(&protocol::test_mode(*index, *pattern))?;
            Ok(())
        }
        Card::TestSweep { count, secs, index } => {
            let mut dev = open(cli)?;
            for pattern in 0..*count {
                println!("pattern {pattern}");
                dev.send(&protocol::test_mode(*index, pattern))?;
                std::thread::sleep(Duration::from_secs(*secs));
            }
            dev.send(&protocol::test_mode(*index, 0))?;
            Ok(())
        }
        Card::SetLayout {
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
        Card::LayoutExample => {
            // Two cards side by side; each receiver's x,y is its --position.
            let canvas = e120_canvas::Canvas::cards(128, 64, 2, 1);
            println!("{}", serde_json::to_string_pretty(&canvas)?);
            Ok(())
        }
    }
}
