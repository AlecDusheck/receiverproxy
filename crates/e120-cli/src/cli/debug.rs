use crate::capture::{listen, pcap_summary, raw_send, replay};
use crate::display::probe;
use crate::util::parse_color;
use crate::Cli;
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum Debug {
    /// Dump every frame seen on the interface
    Listen {
        /// Seconds to listen
        #[arg(long, default_value_t = 10)]
        wait: u64,
        /// Also show frames we transmit, to confirm they reach the wire
        #[arg(long)]
        include_ours: bool,
    },
    /// Send a hand-built frame and print any reply
    Send {
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
    /// Summarize Colorlight packet types in a pcap capture
    Pcap {
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

pub fn run(cli: &Cli, cmd: &Debug) -> Result<()> {
    match cmd {
        Debug::Listen { wait, include_ours } => listen(cli, *wait, *include_ours),
        Debug::Send {
            r#type,
            payload,
            pad,
            wait,
            show,
        } => raw_send(cli, r#type, payload, *pad, *wait, *show),
        Debug::Probe {
            rows,
            row_gap_us,
            sync,
            repeat,
            color,
        } => {
            let rgb = parse_color(std::slice::from_ref(color))?;
            probe(cli, *rows, *row_gap_us, *sync, *repeat, rgb)
        }
        Debug::Pcap { path, dump } => pcap_summary(path, *dump),
        Debug::Replay {
            path,
            types,
            gap_us,
            all,
        } => replay(cli, path, types.as_deref(), *gap_us, *all),
    }
}
