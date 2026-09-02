//! Small helpers shared across the command modules.

use crate::{protocol, Cli};
use anyhow::{Context, Result};
use e120_net::Link;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// How long one `recv` poll blocks before returning empty-handed.
const RECV_TIMEOUT: Duration = Duration::from_millis(500);

/// True for frames sent by the sender/PC to the card.
pub fn is_sender_frame(d: &[u8]) -> bool {
    d.len() >= 14 && d[0..6] == protocol::CARD_MAC
}

/// True for frames sent by the card back to the PC.
pub fn is_card_frame(d: &[u8]) -> bool {
    d.len() >= 14 && d[6..12] == protocol::CARD_MAC
}

/// True for our own transmissions, which the kernel loops back to us.
pub fn is_our_frame(d: &[u8]) -> bool {
    d.len() >= 12 && d[6..12] == protocol::SENDER_MAC
}

pub fn open(cli: &Cli) -> Result<Link> {
    Link::open(&cli.iface, RECV_TIMEOUT)
}

/// Poll `dev` until `wait` runs out or `pick` accepts a frame from the card.
pub fn await_reply<T>(
    dev: &mut Link,
    wait: Duration,
    mut pick: impl FnMut(&[u8]) -> Option<T>,
) -> Result<Option<T>> {
    await_any_frame(dev, wait, |f| if is_card_frame(f) { pick(f) } else { None })
}

/// Like [`await_reply`] but offers every frame, not only the card's.
pub fn await_any_frame<T>(
    dev: &mut Link,
    wait: Duration,
    mut pick: impl FnMut(&[u8]) -> Option<T>,
) -> Result<Option<T>> {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        for f in dev.recv()? {
            if let Some(v) = pick(f) {
                return Ok(Some(v));
            }
        }
    }
    Ok(None)
}

const LATTICE: &[u8] = b"Lattice Semiconductor";

/// True when a bitstream's text header sits within its first 256 bytes.
pub fn has_lattice_header(img: &[u8]) -> bool {
    img.windows(LATTICE.len()).take(256).any(|w| w == LATTICE)
}

/// True when a bitstream header appears anywhere in `d`.
pub fn contains_lattice_header(d: &[u8]) -> bool {
    d.windows(LATTICE.len()).any(|w| w == LATTICE)
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

/// Two lowercase hex digits per byte, `sep` between them.
pub fn hex(bytes: &[u8], sep: &str) -> String {
    let mut s = String::with_capacity(bytes.len() * (2 + sep.len()));
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push_str(sep);
        }
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// A non-fatal problem, on stderr in the same shape as an error.
pub fn warn(msg: impl std::fmt::Display) {
    eprintln!("e120: warning: {msg}");
}

pub fn hexdump(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        println!("  {:04x}: {}", i * 16, hex(chunk, " "));
    }
}

/// Format a MAC address for display.
pub fn mac(b: &[u8]) -> String {
    hex(b, ":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_matches_the_per_byte_format_and_join() {
        let bytes: Vec<u8> = (0..=255).collect();
        let joined = |sep: &str| {
            bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(sep)
        };
        assert_eq!(hex(&bytes, " "), joined(" "));
        assert_eq!(hex(&bytes, ":"), joined(":"));
        assert_eq!(hex(&[], " "), "");
        assert_eq!(
            mac(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]),
            "11:22:33:44:55:66"
        );
    }

    #[test]
    fn a_lattice_header_is_only_found_near_the_start() {
        let mut img = vec![0u8; 600];
        img[100..121].copy_from_slice(LATTICE);
        assert!(has_lattice_header(&img));
        assert!(contains_lattice_header(&img));
        let mut late = vec![0u8; 600];
        late[300..321].copy_from_slice(LATTICE);
        assert!(!has_lattice_header(&late));
        assert!(contains_lattice_header(&late));
    }
}
