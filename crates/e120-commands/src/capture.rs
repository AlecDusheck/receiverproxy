//! Talking to the card directly: discovery, listening, replay, raw frames.

use crate::util::{
    await_any_frame, await_reply, hexdump, is_card_frame, is_our_frame, is_sender_frame, mac, open,
};
use crate::{protocol, Ctx, Progress};
use anyhow::{Context, Result};
use e120_net::{read_pcap, PcapPacket};
use std::time::{Duration, Instant};

pub fn parse_hex(s: &str) -> Result<Vec<u8>> {
    let clean: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    anyhow::ensure!(
        clean.len().is_multiple_of(2),
        "hex string must have an even length"
    );
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).context("bad hex"))
        .collect()
}

/// Send a hand-built frame and report replies. For probing unknown commands.
pub fn raw_send(
    ctx: &Ctx,
    ty: &str,
    payload: &str,
    pad: usize,
    wait: u64,
    show: usize,
    p: &mut dyn Progress,
) -> Result<()> {
    let t = parse_hex(ty)?;
    anyhow::ensure!(t.len() == 2, "--type must be exactly two hex bytes");
    let mut pl = parse_hex(payload)?;
    if pl.len() < pad {
        pl.resize(pad, 0);
    }

    let mut frame = Vec::with_capacity(14 + pl.len());
    frame.extend_from_slice(&protocol::CARD_MAC);
    frame.extend_from_slice(&protocol::SENDER_MAC);
    frame.extend_from_slice(&t);
    frame.extend_from_slice(&pl);

    let mut dev = open(ctx)?;
    dev.send(&frame)?;

    let mut seen = 0;
    await_reply(&mut dev, Duration::from_secs(wait), |f| {
        seen += 1;
        p.out(&format!(
            "reply {seen}: type {:02x}{:02x}, {} bytes",
            f[12],
            f[13],
            f.len()
        ));
        hexdump(p, &f[14..f.len().min(14 + show)]);
        (seen >= 2).then_some(())
    })?;
    if seen == 0 {
        p.out(&format!("no reply within {wait}s"));
    }
    Ok(())
}

/// A packet's capture time in seconds.
fn ts(p: &PcapPacket) -> f64 {
    f64::from(p.ts_sec) + f64::from(p.ts_usec) / 1e6
}

pub fn pcap_summary(path: &str, dump: bool, p: &mut dyn Progress) -> Result<()> {
    let pcap = read_pcap(path)?;
    let pkts: Vec<_> = pcap.packets().collect();
    p.out(&format!("{} packets", pkts.len()));
    let t0 = pkts.first().map_or(0.0, ts);
    let mut counts: std::collections::BTreeMap<(bool, u8), (usize, usize)> =
        std::collections::BTreeMap::default();
    for pk in &pkts {
        let d = pk.data;
        if d.len() < 14 {
            continue;
        }
        let (dir_tx, ty) = if is_sender_frame(d) {
            (true, d[12])
        } else if is_card_frame(d) {
            (false, d[12])
        } else {
            continue;
        };
        let e = counts.entry((dir_tx, ty)).or_default();
        e.0 += 1;
        e.1 += d.len();
        if dump && ty != 0x55 && ty != 0x01 && ty != 0x0a {
            p.out(&format!(
                "\n[{:9.4}s] {} type 0x{:02x} len {}",
                ts(pk) - t0,
                if dir_tx { "PC->card" } else { "card->PC" },
                ty,
                d.len()
            ));
            hexdump(p, &d[..d.len().min(160)]);
        }
    }
    p.out(&format!(
        "\n{:<10} {:>6} {:>10}  type",
        "direction", "count", "bytes"
    ));
    for ((tx, ty), (n, bytes)) in counts {
        p.out(&format!(
            "{:<10} {:>6} {:>10}  0x{ty:02x}",
            if tx { "PC->card" } else { "card->PC" },
            n,
            bytes
        ));
    }
    Ok(())
}

pub fn replay(
    ctx: &Ctx,
    path: &str,
    types: Option<&str>,
    gap_us: u64,
    all: bool,
    p: &mut dyn Progress,
) -> Result<()> {
    let filter: Option<Vec<u8>> = match types {
        Some(t) => Some(
            t.split(',')
                .map(|s| u8::from_str_radix(s.trim(), 16))
                .collect::<Result<_, _>>()
                .context("bad --types list")?,
        ),
        None => None,
    };
    let pcap = read_pcap(path)?;
    let mut dev = open(ctx)?;
    let mut sent = 0usize;
    for pk in pcap.packets() {
        let d = pk.data;
        if !is_sender_frame(d) {
            continue;
        }
        let ty = d[12];
        let selected = match &filter {
            Some(f) => f.contains(&ty),
            None => all || !matches!(ty, 0x55 | 0x01 | 0x0a | 0x07),
        };
        if !selected {
            continue;
        }
        dev.send(d)?;
        sent += 1;
        std::thread::sleep(Duration::from_micros(gap_us));
    }
    p.out(&format!("replayed {sent} frames from {path}"));
    Ok(())
}

/// Send one discovery frame and return the first card that answers.
pub fn discover_one(ctx: &Ctx, wait: u64) -> Result<Option<protocol::DiscoveryInfo>> {
    let mut dev = open(ctx)?;
    dev.send(&protocol::discovery())?;
    await_any_frame(
        &mut dev,
        Duration::from_secs(wait),
        protocol::parse_discovery_response,
    )
}

/// One line per card, as `e120 discover` prints it.
#[must_use]
pub fn describe(info: &protocol::DiscoveryInfo) -> String {
    format!(
        "receiver card #{}: id=0x{:02x} firmware={}.{:02} detected size {}x{}",
        info.controller, info.card_id, info.ver_major, info.ver_minor, info.cols, info.rows
    )
}

/// Send one discovery frame, report every reply as it arrives until `wait`
/// runs out, and return them. Empty is not an error here; the CLI makes it one.
pub fn discover(
    ctx: &Ctx,
    wait: u64,
    p: &mut dyn Progress,
) -> Result<Vec<protocol::DiscoveryInfo>> {
    let mut dev = open(ctx)?;
    dev.send(&protocol::discovery())?;
    let deadline = Instant::now() + Duration::from_secs(wait);
    let mut found = Vec::new();
    while Instant::now() < deadline {
        for f in dev.recv()? {
            // The kernel loops our own transmissions back.
            if is_our_frame(f) {
                continue;
            }
            if let Some(info) = protocol::parse_discovery_response(f) {
                p.out(&describe(&info));
                found.push(info);
            }
        }
    }
    Ok(found)
}

pub fn listen(ctx: &Ctx, wait: u64, include_ours: bool, p: &mut dyn Progress) -> Result<()> {
    let mut dev = open(ctx)?;
    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        for f in dev.recv()? {
            // Our own transmissions are normally noise, but they are the only
            // way to confirm a display frame actually reached the wire.
            if f.len() < 14 || (!include_ours && is_our_frame(f)) {
                continue;
            }
            p.out(&format!(
                "frame: dst {} src {} type {:02x}{:02x} len {}",
                mac(&f[0..6]),
                mac(&f[6..12]),
                f[12],
                f[13],
                f.len()
            ));
            hexdump(p, &f[14..f.len().min(14 + 96)]);
        }
    }
    Ok(())
}
