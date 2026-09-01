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

/// Compile a complete block-7 flash image from explicit parts, using the
/// factory dump (or another known-good block) as the base for every region we
/// cannot yet derive. Offline: writes a file for `restore-flash` to send.
#[allow(clippy::too_many_arguments)]
pub fn compile_config(
    rcvbp_path: &str,
    basic_pack: Option<&str>,
    chip_from: Option<&str>,
    mapping_from: Option<&str>,
    base: &str,
    out: &str,
) -> Result<()> {
    let (path, off) = match base.split_once(':') {
        Some((p, o)) => (
            p,
            usize::from_str_radix(o.trim_start_matches("0x"), 16)
                .with_context(|| format!("bad base offset {o:?}"))?,
        ),
        None => (base, 0),
    };
    let dump = std::fs::read(path).with_context(|| format!("read {path}"))?;
    anyhow::ensure!(
        dump.len() >= off + rcvbp::compiled::IMAGE_LEN,
        "{path} is too short for a 64KB block at 0x{off:x}"
    );
    let mut b = rcvbp::compiled::Block7Builder::from_base(
        &dump[off..off + rcvbp::compiled::IMAGE_LEN],
    )?;

    if let Some(p) = basic_pack {
        let body = std::fs::read(p).with_context(|| format!("read {p}"))?;
        b.basic_pack(&body)?;
    }
    if let Some(p) = chip_from {
        b.chip_registers_from(&rcvbp::Rcvbp::load(p)?)?;
    }
    if let Some(p) = mapping_from {
        b.mapping_from(&rcvbp::Rcvbp::load(p)?)?;
    }
    let file = std::fs::read(rcvbp_path).with_context(|| format!("read {rcvbp_path}"))?;
    b.rcvbp(&file)?;

    let (img, notes, changed) = b.finish();
    std::fs::write(out, &img).with_context(|| format!("write {out}"))?;
    println!("wrote {out}");
    for n in &notes {
        println!("  {n}");
    }
    let pages: Vec<String> = changed.iter().map(|p| format!("{p:02x}")).collect();
    println!("  pages differing from base: {}: {}", changed.len(), pages.join(" "));
    println!("flash it with: e120 restore-flash {out} --commit   (then screen-size + power-cycle)");
    Ok(())
}

/// Generate a panel's configuration from a TOML spec: the `.rcvbp`, the
/// basic-pack body, the compiled block-7 boot image, and a provenance list
/// naming the source of every placed byte.
pub fn gen_config(spec_path: &str, out_dir: &str) -> Result<()> {
    let text = std::fs::read_to_string(spec_path).with_context(|| format!("read {spec_path}"))?;
    let spec: rcvbp::spec::PanelSpec =
        toml::from_str(&text).with_context(|| format!("parse {spec_path}"))?;

    let template = rcvbp::Rcvbp::load(&spec.template.rcvbp)?;
    let pack = std::fs::read(&spec.template.basic_pack)
        .with_context(|| format!("read {}", spec.template.basic_pack))?;
    let chip_regs = spec
        .chip
        .registers_from
        .as_deref()
        .map(rcvbp::Rcvbp::load)
        .transpose()?;
    let mapping = spec
        .template
        .mapping_from
        .as_deref()
        .map(rcvbp::Rcvbp::load)
        .transpose()?;
    let g = rcvbp::spec::generate(&spec, &template, &pack, chip_regs.as_ref(), mapping.as_ref())?;

    std::fs::create_dir_all(out_dir).with_context(|| format!("create {out_dir}"))?;
    let stem = format!("{out_dir}/{}", spec.name);
    let rcvbp_path = format!("{stem}.rcvbp");
    g.rcvbp.save(&rcvbp_path)?;
    let pack_path = format!("{stem}-basic-pack.bin");
    std::fs::write(&pack_path, g.basic_pack).with_context(|| format!("write {pack_path}"))?;

    // The boot image, built from erased flash: every region generated from
    // the spec and the generated config, except the scan table, whose
    // solver is untranscribed and is carried from the reference block.
    let (base_path, base_off) = match spec.template.base_block.split_once(':') {
        Some((p, o)) => (
            p.to_string(),
            usize::from_str_radix(o.trim_start_matches("0x"), 16)
                .with_context(|| format!("bad base offset {o:?}"))?,
        ),
        None => (spec.template.base_block.clone(), 0),
    };
    let dump = std::fs::read(&base_path).with_context(|| format!("read {base_path}"))?;
    anyhow::ensure!(
        dump.len() >= base_off + rcvbp::compiled::IMAGE_LEN,
        "{base_path} is too short for a 64KB block at 0x{base_off:x}"
    );
    let scan_at = base_off + rcvbp::compiled::SCAN_TABLE_OFFSET;
    let rec01 = g.rcvbp.record_01().context("generated config lost record 0x01")?.payload.clone();

    let mut b = rcvbp::compiled::Block7Builder::erased();
    b.zero_regions();
    b.basic_pack(&g.basic_pack)?;
    b.data_swap_from(&rec01)?;
    b.module_positions_from(&rec01)?;
    b.anti_void_lines();
    b.mapping_from(&g.rcvbp)?;
    b.scan_table(&dump[scan_at..scan_at + rcvbp::compiled::SCAN_TABLE_LEN])?;
    if spec.boot.arm_at_boot {
        b.chip_registers_from(&g.rcvbp)?;
    }
    b.rcvbp(&g.rcvbp.to_file_bytes()?)?;
    let (img, notes, changed) = b.finish();
    let img_path = format!("{stem}-block7.bin");
    std::fs::write(&img_path, &img).with_context(|| format!("write {img_path}"))?;

    let mut report = String::new();
    report.push_str(&format!("spec: {spec_path}\n\n# record and pack provenance\n"));
    for line in &g.provenance {
        report.push_str(line);
        report.push('\n');
    }
    report.push_str("\n# compiled image\n");
    for n in &notes {
        report.push_str(n);
        report.push('\n');
    }
    let pages: Vec<String> = changed.iter().map(|p| format!("{p:02x}")).collect();
    report.push_str(&format!(
        "scan table <- {} (only region not generated)\npages written: {}: {}\n",
        spec.template.base_block,
        changed.len(),
        pages.join(" ")
    ));
    let report_path = format!("{stem}-provenance.txt");
    std::fs::write(&report_path, &report).with_context(|| format!("write {report_path}"))?;

    println!("generated {}:", spec.name);
    println!("  {rcvbp_path}");
    println!("  {pack_path}");
    println!("  {img_path}   ({} pages differ from base)", changed.len());
    println!("  {report_path}");
    println!("install: e120 restore-flash {img_path} --commit && e120 screen-size --set {}x{} --commit", spec.screen.width, spec.screen.height);
    Ok(())
}
