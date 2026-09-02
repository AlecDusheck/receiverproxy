use crate::parse_geometry;
use anyhow::Result;
use clap::Subcommand;
use ops::{probe, screen, Ctx, Progress};

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
    /// List the card models in config/cards and how far each is tested
    Models {
        /// Print the README's Tested matrix instead
        #[arg(long)]
        markdown: bool,
    },
    /// Read the card and check its model file's claims; exit 1 on a mismatch
    Probe {
        /// Directory for the bytes read (parameter block, EEPROM mirror)
        #[arg(long)]
        out: Option<String>,
        /// Receiver index on the chain
        #[arg(long, default_value_t = 0)]
        index: u16,
        /// Seconds to wait for each reply
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
}

impl Card {
    pub fn needs_card(&self) -> bool {
        matches!(self, Self::ScreenSize { .. })
    }
}

pub fn run(ctx: &Ctx, cmd: &Card, p: &mut dyn Progress) -> Result<()> {
    match cmd {
        Card::ScreenSize {
            set,
            commit,
            index,
            wait,
        } => screen::screen_size(ctx, *set, *commit, *index, *wait, p).map(drop),
        Card::Reload { index, full } => screen::reload(ctx, *index, *full),
        Card::TestMode { pattern, index } => screen::test_mode(ctx, *index, *pattern),
        Card::TestSweep { count, secs, index } => screen::test_sweep(ctx, *count, *secs, *index, p),
        Card::SetLayout {
            panel_width,
            panel_height,
            index,
        } => screen::set_layout(ctx, *index, *panel_width, *panel_height),
        Card::LayoutExample => {
            // Two cards side by side; each receiver's x,y is its --position.
            let canvas = wall::Canvas::cards(128, 64, 2, 1);
            p.out(&serde_json::to_string_pretty(&canvas)?);
            Ok(())
        }
        Card::Models { markdown: false } => {
            ops::model::list(p);
            Ok(())
        }
        Card::Models { markdown: true } => {
            p.out(&ops::model::matrix_markdown()?);
            Ok(())
        }
        Card::Probe { out, index, wait } => {
            let report = probe::probe(ctx, &probe::Args { out: out.as_deref(), index: *index, wait: *wait }, p)?;
            let n = report.mismatches();
            anyhow::ensure!(n == 0, "{n} mismatch{}", if n == 1 { "" } else { "es" });
            Ok(())
        }
    }
}
