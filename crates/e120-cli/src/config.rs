//! Inspecting, composing and comparing `.rcvbp` configuration files.

use crate::rcvbp;
use crate::util::hexdump;
use anyhow::{Context, Result};

/// Parse a comma-separated list of hex record types.
pub fn parse_types(s: &str) -> Result<Vec<u16>> {
    s.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| {
            u16::from_str_radix(t.trim_start_matches("0x"), 16)
                .with_context(|| format!("bad record type {t:?}"))
        })
        .collect()
}

pub fn config_build(
    base: &str,
    copy_from: Option<&str>,
    copy: &str,
    remove: &str,
    out: &str,
) -> Result<()> {
    let mut cfg = rcvbp::Rcvbp::load(base)?;
    println!("base {base}: {} records", cfg.records.len());

    let to_copy = parse_types(copy)?;
    if !to_copy.is_empty() {
        let src_path = copy_from.context("--copy needs --copy-from")?;
        let src = rcvbp::Rcvbp::load(src_path)?;
        for t in to_copy {
            let rec = src
                .find(t)
                .with_context(|| format!("{src_path} has no record 0x{t:04x}"))?;
            let existed = cfg.find(t).is_some();
            cfg.upsert(t, rec.payload.clone());
            println!(
                "  {} record 0x{t:04x} ({} bytes) from {src_path}",
                if existed { "replaced" } else { "added" },
                rec.payload.len()
            );
        }
    }

    for t in parse_types(remove)? {
        println!(
            "  {} record 0x{t:04x}",
            if cfg.remove(t) { "removed" } else { "no such" }
        );
    }

    cfg.save(out)?;
    let written = std::fs::metadata(out)?.len();
    println!(
        "wrote {out}: {} records, {written} bytes on disk",
        cfg.records.len()
    );

    // Read it straight back so a broken file never reaches the card.
    let back = rcvbp::Rcvbp::load(out)?;
    anyhow::ensure!(
        back.records.len() == cfg.records.len(),
        "verification failed: wrote {} records but read back {}",
        cfg.records.len(),
        back.records.len()
    );
    println!("verified: reparses to {} records", back.records.len());
    Ok(())
}

pub fn config_diff(a: &str, b: &str) -> Result<()> {
    let fa = rcvbp::Rcvbp::load(a)?;
    let fb = rcvbp::Rcvbp::load(b)?;
    println!("{a}: {} records", fa.records.len());
    println!("{b}: {} records", fb.records.len());

    let types_a: Vec<u16> = fa.records.iter().map(rcvbp::Record::type_u16).collect();
    let types_b: Vec<u16> = fb.records.iter().map(rcvbp::Record::type_u16).collect();
    let only_a: Vec<String> = types_a
        .iter()
        .filter(|t| !types_b.contains(t))
        .map(|t| format!("0x{t:04x}"))
        .collect();
    let only_b: Vec<String> = types_b
        .iter()
        .filter(|t| !types_a.contains(t))
        .map(|t| format!("0x{t:04x}"))
        .collect();
    if !only_a.is_empty() {
        println!("only in {a}: {}", only_a.join(", "));
    }
    if !only_b.is_empty() {
        println!("only in {b}: {}", only_b.join(", "));
    }

    for t in &types_a {
        let (Some(ra), Some(rb)) = (fa.find(*t), fb.find(*t)) else {
            continue;
        };
        if ra.payload == rb.payload {
            continue;
        }
        let n = ra.payload.len().min(rb.payload.len());
        let diffs: Vec<usize> = (0..n)
            .filter(|i| ra.payload[*i] != rb.payload[*i])
            .collect();
        println!(
            "record 0x{t:04x}: {} vs {} bytes, {} differ",
            ra.payload.len(),
            rb.payload.len(),
            diffs.len()
        );
        for i in diffs.iter().take(16) {
            println!(
                "    +0x{i:03x}: {:3} (0x{:02x})  vs  {:3} (0x{:02x})",
                ra.payload[*i], ra.payload[*i], rb.payload[*i], rb.payload[*i]
            );
        }
        if diffs.len() > 16 {
            println!("    ... and {} more", diffs.len() - 16);
        }
    }
    Ok(())
}

pub fn rcvbp_info(path: &str, dump: bool) -> Result<()> {
    let f = rcvbp::Rcvbp::load(path)?;
    println!(
        "{path}\n  version {}, {} bytes decompressed, {} records",
        f.version,
        f.blob.len(),
        f.records.len()
    );
    if let Some((w, _)) = f.geometry() {
        println!("  cabinet width: {w}");
    }
    if let Some(scan) = f.scan() {
        println!("  scan: 1/{scan}");
    }
    if let Some((w, scan)) = f.main_geometry() {
        println!("  main param block: width {w}, scan 1/{scan}");
    }
    println!(
        "\n{:>8} {:>7} {:>7} {:>8}  description",
        "offset", "type", "bytes", "nonzero"
    );
    for r in &f.records {
        let nz = r.payload.iter().filter(|&&b| b != 0).count();
        println!(
            "0x{:06x}  0x{:04x} {:7} {:8}  {}",
            r.offset,
            r.type_u16(),
            r.payload.len(),
            nz,
            describe_record(r.type_u16(), r.is_empty_table())
        );
    }
    if dump {
        for r in &f.records {
            if r.is_empty_table() {
                continue;
            }
            println!(
                "\n=== record 0x{:04x} ({} bytes)",
                r.type_u16(),
                r.payload.len()
            );
            hexdump(&r.payload[..r.payload.len().min(512)]);
        }
    }
    Ok(())
}

pub fn describe_record(t: u16, empty: bool) -> &'static str {
    match (t, empty) {
        (_, true) => "(empty table)",
        (0x0a01, _) => "main receiver parameters (geometry, scan, timing)",
        (0x0a03, _) => "pixel/row mapping table",
        (0x0a84, _) => "driver-chip register table",
        (0x0a8a, _) => "secondary parameters",
        (0x0aca, _) => "cabinet geometry",
        (0x0a83 | 0x0a89, _) => "RGB coefficients",
        _ => "",
    }
}
