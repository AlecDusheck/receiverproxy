//! Inspecting, composing and comparing `.rcvbp` configuration files.

use crate::rcvbp;
use crate::util::{hex, hexdump, warn};
use anyhow::{Context, Result};
use std::fmt::Write as _;

/// Parse a comma-separated list of hex record types.
fn parse_types(s: &str) -> Result<Vec<u16>> {
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

    let to_copy = parse_types(copy)?;
    if !to_copy.is_empty() {
        let src_path = copy_from.context("--copy needs --copy-from")?;
        let src = rcvbp::Rcvbp::load(src_path)?;
        for t in to_copy {
            let rec = src
                .find(t)
                .with_context(|| format!("{src_path} has no record 0x{t:04x}"))?;
            cfg.upsert(t, rec.payload.clone());
        }
    }

    for t in parse_types(remove)? {
        if !cfg.remove(t) {
            warn(format!("{base} has no record 0x{t:04x} to remove"));
        }
    }

    cfg.save(out)?;

    // Read it straight back so a broken file never reaches the card.
    let back = rcvbp::Rcvbp::load(out)?;
    anyhow::ensure!(
        back.records.len() == cfg.records.len(),
        "{out}: wrote {} records but read back {}",
        cfg.records.len(),
        back.records.len()
    );
    println!("{out}");
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
        let diffs: Vec<usize> = ra
            .payload
            .iter()
            .zip(&rb.payload)
            .enumerate()
            .filter(|(_, (x, y))| x != y)
            .map(|(i, _)| i)
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
        f.to_blob()?.len(),
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

fn describe_record(t: u16, empty: bool) -> &'static str {
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

/// Generate a panel's configuration from a TOML spec: the `.rcvbp`, the
/// basic-pack body, the compiled block-7 boot image, and a file listing
/// where every placed byte came from.
pub fn gen_config(spec_path: &str, out_dir: &str) -> Result<()> {
    let spec = rcvbp::spec::PanelSpec::load(spec_path)?;
    let g = spec.generate()?;

    std::fs::create_dir_all(out_dir).with_context(|| format!("create {out_dir}"))?;
    let stem = format!("{out_dir}/{}", spec.name);
    let rcvbp_path = format!("{stem}.rcvbp");
    g.rcvbp.save(&rcvbp_path)?;
    let pack_path = format!("{stem}-basic-pack.bin");
    std::fs::write(&pack_path, g.basic_pack).with_context(|| format!("write {pack_path}"))?;

    let mut b = rcvbp::image::Block7Builder::from_generated(&spec, &g)?;
    if spec.boot.arm_at_boot {
        b.chip_registers_from(&g.rcvbp)?;
    }
    b.rcvbp(&g.rcvbp.to_file_bytes()?)?;
    let rcvbp::image::Block7 {
        image: img,
        notes,
        changed_pages: changed,
    } = b.finish();
    let img_path = format!("{stem}-block7.bin");
    std::fs::write(&img_path, &img).with_context(|| format!("write {img_path}"))?;

    let mut report = String::new();
    let _ = writeln!(report, "spec: {spec_path}\n\n# record and pack sources");
    for line in &g.sources {
        report.push_str(line);
        report.push('\n');
    }
    report.push_str("\n# compiled image\n");
    for n in &notes {
        report.push_str(n);
        report.push('\n');
    }
    let _ = writeln!(
        report,
        "pages written: {}: {}",
        changed.len(),
        hex(&changed, " ")
    );
    let report_path = format!("{stem}-sources.txt");
    std::fs::write(&report_path, &report).with_context(|| format!("write {report_path}"))?;

    println!("{rcvbp_path}\n{pack_path}\n{img_path}\n{report_path}");
    Ok(())
}
