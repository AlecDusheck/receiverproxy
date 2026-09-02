use anyhow::Result;
use clap::Subcommand;
use ops::flash::flash_firmware;
use ops::{protocol, upgrade, Ctx, Progress};

#[derive(Subcommand)]
pub enum Firmware {
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
    /// Write an FPGA bitstream into the primary bank with host page writes
    Write {
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
}

impl Firmware {
    pub fn needs_card(&self) -> bool {
        matches!(self, Self::Write { .. })
    }
}

pub fn run(ctx: &Ctx, cmd: &Firmware, p: &mut dyn Progress) -> Result<()> {
    match cmd {
        Firmware::Info { wait } => upgrade::info(ctx, *wait, p),
        Firmware::Install {
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
                ctx,
                image,
                *commit,
                partition,
                *timeout,
                *chunk_delay_us,
                *wait,
                p,
            )
        }
        Firmware::Write {
            image,
            backup,
            commit,
            from_block,
            to_block,
            index,
            wait,
        } => flash_firmware(
            ctx,
            image,
            backup,
            *commit,
            *from_block..*to_block,
            *index,
            *wait,
            p,
        ),
    }
}
