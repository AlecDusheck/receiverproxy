use anyhow::Result;
use clap::Subcommand;
use e120_commands::config::{config_build, config_diff, gen_config, rcvbp_info};
use e120_commands::flash::{read_config, save_config, write_config};
use e120_commands::params::send_params;
use e120_commands::{protocol, read_library, Ctx, Progress};

#[derive(Subcommand)]
pub enum Config {
    /// Generate a .rcvbp and boot image from a panel spec (TOML)
    Gen {
        /// Panel spec, see config/panels/*.toml
        #[arg(long)]
        spec: String,
        /// Directory for the outputs (created if missing)
        #[arg(long, default_value = "build")]
        out_dir: String,
    },
    /// Build a .rcvbp by combining and editing existing ones
    Build {
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
    Diff {
        /// First .rcvbp
        a: String,
        /// Second .rcvbp
        b: String,
    },
    /// List the records in a .rcvbp file
    Info {
        /// The .rcvbp to list
        path: String,
        /// Hexdump each record's payload (non-empty ones)
        #[arg(long)]
        dump: bool,
    },
    /// Save the card's stored configuration as a .rcvbp file
    Read {
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
    /// Write a .rcvbp into the card's parameter flash (read-modify-write)
    Write {
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
    /// Push a panel spec's parameters into the card's RAM (no flash, no reboot)
    Send {
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
}

pub fn run(ctx: &Ctx, cmd: &Config, p: &mut dyn Progress) -> Result<()> {
    match cmd {
        Config::Gen { spec, out_dir } => gen_config(spec, out_dir, &read_library, p).map(drop),
        Config::Build {
            base,
            copy_from,
            copy,
            remove,
            out,
        } => config_build(base, copy_from.as_deref(), copy, remove, out, p),
        Config::Diff { a, b } => config_diff(a, b, p),
        Config::Info { path, dump } => rcvbp_info(path, *dump, p),
        Config::Read {
            out,
            index,
            page,
            max_chunks,
            wait,
        } => {
            let file = read_config(ctx, *index, *page, *max_chunks, *wait)?;
            save_config(&file, out, p)
        }
        Config::Write {
            config,
            commit,
            backup,
            index,
            wait,
        } => write_config(ctx, config, *commit, backup, None, *index, *wait, p),
        Config::Send {
            spec,
            chip_only,
            gap_ms,
        } => send_params(ctx, spec, *chip_only, *gap_ms, &read_library),
    }
}
