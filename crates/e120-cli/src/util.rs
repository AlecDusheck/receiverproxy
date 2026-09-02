//! Small helpers shared across the command modules.

use crate::{protocol, Cli};
use anyhow::{Context, Result};
use e120_net::bpf;

/// True for frames sent by the sender/PC to the card.
pub fn is_sender_frame(d: &[u8]) -> bool {
    d.len() >= 14 && d[0..6] == protocol::CARD_MAC
}

/// True for frames sent by the card back to the PC.
pub fn is_card_frame(d: &[u8]) -> bool {
    d.len() >= 14 && d[6..12] == protocol::CARD_MAC
}

pub fn open(cli: &Cli) -> Result<bpf::Bpf> {
    bpf::Bpf::open(&cli.iface, true, 500)
}

/// `RRGGBB` (optionally `#`-prefixed) or three decimal components.
pub fn parse_color(parts: &[String]) -> Result<[u8; 3]> {
    match parts {
        [hex] => {
            let hex = hex.trim_start_matches('#');
            anyhow::ensure!(hex.len() == 6, "expected RRGGBB hex or three 0-255 values");
            let v = u32::from_str_radix(hex, 16).context("bad hex color")?;
            Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
        }
        [r, g, b] => Ok([r.parse()?, g.parse()?, b.parse()?]),
        _ => anyhow::bail!("expected RRGGBB hex or three 0-255 values"),
    }
}

pub fn hexdump(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        println!("  {:04x}: {}", i * 16, hex.join(" "));
    }
}

/// Format a MAC address for display.
pub fn mac(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}
