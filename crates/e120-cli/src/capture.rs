//! Talking to the card directly: discovery, listening, replay, raw frames.

use crate::util::{hexdump, is_card_frame, is_sender_frame, mac, open};
use crate::{protocol, Cli};
use anyhow::{Context, Result};
use e120_net::pcap;
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
    cli: &Cli,
    ty: &str,
    payload: &str,
    pad: usize,
    wait: u64,
    show: usize,
) -> Result<()> {
    let t = parse_hex(ty)?;
    anyhow::ensure!(t.len() == 2, "--type must be exactly two hex bytes");
    let mut p = parse_hex(payload)?;
    if p.len() < pad {
        p.resize(pad, 0);
    }

    let mut frame = Vec::with_capacity(14 + p.len());
    frame.extend_from_slice(&protocol::CARD_MAC);
    frame.extend_from_slice(&protocol::SENDER_MAC);
    frame.extend_from_slice(&t);
    frame.extend_from_slice(&p);

    let mut dev = open(cli)?;
    println!("sending type {}{:02x}, {} byte frame", ty, 0, frame.len());
    dev.send(&frame)?;

    let deadline = Instant::now() + Duration::from_secs(wait);
    let mut seen = 0;
    while Instant::now() < deadline {
        for f in dev.recv()? {
            if !is_card_frame(&f) {
                continue;
            }
            seen += 1;
            println!(
                "reply {seen}: type {:02x}{:02x}, {} bytes",
                f[12],
                f[13],
                f.len()
            );
            hexdump(&f[14..f.len().min(14 + show)]);
            if seen >= 2 {
                return Ok(());
            }
        }
    }
    if seen == 0 {
        println!("no reply within {wait}s");
    }
    Ok(())
}

pub fn pcap_summary(path: &str, dump: bool) -> Result<()> {
    let pkts = pcap::read_pcap(path)?;
    println!("{} packets", pkts.len());
    let mut counts: std::collections::BTreeMap<(bool, u8), (usize, usize)> =
        std::collections::BTreeMap::default();
    for p in &pkts {
        let d = &p.data;
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
            let t0 = f64::from(pkts[0].ts_sec) + f64::from(pkts[0].ts_usec) / 1e6;
            let t = f64::from(p.ts_sec) + f64::from(p.ts_usec) / 1e6 - t0;
            println!(
                "\n[{:9.4}s] {} type 0x{:02x} len {}",
                t,
                if dir_tx { "PC->card" } else { "card->PC" },
                ty,
                d.len()
            );
            hexdump(&d[..d.len().min(160)]);
        }
    }
    println!("\n{:<10} {:>6} {:>10}  type", "direction", "count", "bytes");
    for ((tx, ty), (n, bytes)) in counts {
        println!(
            "{:<10} {:>6} {:>10}  0x{ty:02x}",
            if tx { "PC->card" } else { "card->PC" },
            n,
            bytes
        );
    }
    Ok(())
}

pub fn replay(cli: &Cli, path: &str, types: Option<&str>, gap_us: u64, all: bool) -> Result<()> {
    let filter: Option<Vec<u8>> = match types {
        Some(t) => Some(
            t.split(',')
                .map(|s| u8::from_str_radix(s.trim(), 16))
                .collect::<Result<_, _>>()
                .context("bad --types list")?,
        ),
        None => None,
    };
    let pkts = pcap::read_pcap(path)?;
    let mut dev = open(cli)?;
    let mut sent = 0usize;
    for p in &pkts {
        let d = &p.data;
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
    println!("replayed {sent} frames from {path}");
    Ok(())
}

pub fn discover(cli: &Cli, wait: u64) -> Result<()> {
    let mut dev = open(cli)?;
    println!("sending discovery on {} ...", cli.iface);
    dev.send(&protocol::discovery())?;
    let deadline = Instant::now() + Duration::from_secs(wait);
    let mut found = 0;
    while Instant::now() < deadline {
        for f in dev.recv()? {
            // Ignore our own transmissions (BPF loops them back)
            if f.len() >= 12 && f[6..12] == protocol::SENDER_MAC {
                continue;
            }
            if let Some(info) = protocol::parse_discovery_response(&f) {
                found += 1;
                println!(
                    "receiver card #{}: id=0x{:02x} firmware={}.{:02} detected size {}x{}",
                    info.controller,
                    info.card_id,
                    info.ver_major,
                    info.ver_minor,
                    info.cols,
                    info.rows
                );
                println!("first 64 payload bytes:");
                hexdump(&info.raw[..info.raw.len().min(64)]);
            } else if f.len() >= 14 {
                println!(
                    "other frame: src {} type {:02x}{:02x} len {}",
                    mac(&f[6..12]),
                    f[12],
                    f[13],
                    f.len()
                );
            }
        }
    }
    if found == 0 {
        println!("no discovery response received in {wait}s");
        println!("(check link on {}, and that the card has power)", cli.iface);
    }
    Ok(())
}

pub fn listen(cli: &Cli, wait: u64, include_ours: bool) -> Result<()> {
    let mut dev = open(cli)?;
    println!("listening on {} for {wait}s ...", cli.iface);
    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        for f in dev.recv()? {
            // Our own transmissions are normally noise, but they are the only
            // way to confirm a display frame actually reached the wire.
            if f.len() < 14 || (!include_ours && f[6..12] == protocol::SENDER_MAC) {
                continue;
            }
            println!(
                "frame: dst {} src {} type {:02x}{:02x} len {}",
                mac(&f[0..6]),
                mac(&f[6..12]),
                f[12],
                f[13],
                f.len()
            );
            hexdump(&f[14..f.len().min(14 + 96)]);
        }
    }
    Ok(())
}
