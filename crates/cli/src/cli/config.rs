use anyhow::Result;
use clap::Subcommand;
use ops::config::{config_build, config_diff, gen_config, import_config, list_formats, rcvbp_info};
use ops::flash::{read_config, save_config, write_config};
use ops::params::send_params;
use ops::{read_library, Ctx, Progress};

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
        /// Configuration format, one of `rxp config formats`
        #[arg(long, default_value = "rcvbp")]
        format: String,
    },
    /// Write the panel spec (TOML) that regenerates a configuration file
    Import {
        /// The file to read, e.g. a .rcvbp from `config read` or a vendor tool
        file: String,
        /// Where to write the spec
        #[arg(long, default_value = "spec.toml")]
        out: String,
        /// Configuration format, one of `rxp config formats` [default: detected from the file]
        #[arg(long)]
        format: Option<String>,
    },
    /// List the configuration formats and what each codec can do
    Formats,
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
        /// Starting 256-byte flash page [default: the card model's parameter page]
        #[arg(long)]
        page: Option<u16>,
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

impl Config {
    pub fn needs_card(&self) -> bool {
        matches!(self, Self::Read { .. } | Self::Write { .. } | Self::Send { .. })
    }
}

pub fn run(ctx: &Ctx, cmd: &Config, p: &mut dyn Progress) -> Result<()> {
    match cmd {
        Config::Gen {
            spec,
            out_dir,
            format,
        } => {
            // Offline: the boot image is laid out for --card, else the first tested model.
            let card = ctx.model.unwrap_or_else(ops::receivers::default_model);
            gen_config(card, spec, out_dir, format, &read_library, p).map(drop)
        }
        Config::Import { file, out, format } => import_config(file, out, format.as_deref(), p),
        Config::Formats => {
            list_formats(p);
            Ok(())
        }
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
