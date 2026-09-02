//! Inspecting, composing and comparing `.rcvbp` configuration files.

use crate::rcvbp;
use crate::util::{hex, hexdump, warn};
use crate::{Loader, Progress};
use anyhow::{Context, Result};
use panelspec::PanelSpec;
use receivers::CardModel;
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
    p: &mut dyn Progress,
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
            warn(p, format!("{base} has no record 0x{t:04x} to remove"));
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
    p.out(out);
    Ok(())
}

pub fn config_diff(a: &str, b: &str, p: &mut dyn Progress) -> Result<()> {
    let fa = rcvbp::Rcvbp::load(a)?;
    let fb = rcvbp::Rcvbp::load(b)?;
    p.out(&format!("{a}: {} records", fa.records.len()));
    p.out(&format!("{b}: {} records", fb.records.len()));

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
        p.out(&format!("only in {a}: {}", only_a.join(", ")));
    }
    if !only_b.is_empty() {
        p.out(&format!("only in {b}: {}", only_b.join(", ")));
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
        p.out(&format!(
            "record 0x{t:04x}: {} vs {} bytes, {} differ",
            ra.payload.len(),
            rb.payload.len(),
            diffs.len()
        ));
        for i in diffs.iter().take(16) {
            p.out(&format!(
                "    +0x{i:03x}: {:3} (0x{:02x})  vs  {:3} (0x{:02x})",
                ra.payload[*i], ra.payload[*i], rb.payload[*i], rb.payload[*i]
            ));
        }
        if diffs.len() > 16 {
            p.out(&format!("    ... and {} more", diffs.len() - 16));
        }
    }
    Ok(())
}

pub fn rcvbp_info(path: &str, dump: bool, p: &mut dyn Progress) -> Result<()> {
    let f = rcvbp::Rcvbp::load(path)?;
    p.out(&format!(
        "{path}\n  version {}, {} bytes decompressed, {} records",
        f.version,
        f.to_blob()?.len(),
        f.records.len()
    ));
    if let Some((w, _)) = f.geometry() {
        p.out(&format!("  cabinet width: {w}"));
    }
    if let Some(scan) = f.scan() {
        p.out(&format!("  scan: 1/{scan}"));
    }
    if let Some((w, scan)) = f.main_geometry() {
        p.out(&format!("  main param block: width {w}, scan 1/{scan}"));
    }
    p.out(&format!(
        "\n{:>8} {:>7} {:>7} {:>8}  description",
        "offset", "type", "bytes", "nonzero"
    ));
    for r in &f.records {
        let nz = r.payload.iter().filter(|&&b| b != 0).count();
        p.out(&format!(
            "0x{:06x}  0x{:04x} {:7} {:8}  {}",
            r.offset,
            r.type_u16(),
            r.payload.len(),
            nz,
            r.describe()
        ));
    }
    if dump {
        for r in &f.records {
            if r.is_empty_table() {
                continue;
            }
            p.out(&format!(
                "\n=== record 0x{:04x} ({} bytes)",
                r.type_u16(),
                r.payload.len()
            ));
            hexdump(p, &r.payload[..r.payload.len().min(512)]);
        }
    }
    Ok(())
}

/// `e120 config formats`: the codec registry as a table.
pub fn list_formats(p: &mut dyn Progress) {
    p.out(&format!("{:<8} {:<12} {:<10} {:<9} import", "format", "vendor", "extension", "generate"));
    let yes_no = |b: bool| if b { "yes" } else { "no" };
    for f in rcvbp::formats() {
        p.out(&format!(
            "{:<8} {:<12} .{:<9} {:<9} {}",
            f.name,
            f.vendor,
            f.extension,
            yes_no(f.generate),
            yes_no(f.import)
        ));
    }
}

/// `e120 config import`: the spec that regenerates `path`, written to `out`.
///
/// The spec is named after the file. `format` names a codec; without it the
/// codec is the one whose signature the file starts with. Chip libraries are
/// chosen by chip id from the embedded set, as the site does. Every field
/// the file did not determine is warned by name.
pub fn import_config(path: &str, out: &str, format: Option<&str>, p: &mut dyn Progress) -> Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("read {path}"))?;
    let codec = match format {
        Some(name) => rcvbp::codec(name)?,
        None => rcvbp::detect(&bytes).with_context(|| path.to_owned())?,
    };
    let chips = |id: u16| panelspec::embedded::chip_by_family(id).map(|(p, t)| (p.to_owned(), t.to_owned()));
    let (mut spec, unresolved) = codec.import(&bytes, &chips).with_context(|| path.to_owned())?;
    spec.name = std::path::Path::new(path)
        .file_stem()
        .map_or_else(|| spec.name.clone(), |s| s.to_string_lossy().into_owned());
    std::fs::write(out, spec.to_toml()?).with_context(|| format!("write {out}"))?;
    for u in &unresolved {
        warn(p, format!("{path}: not recovered: {u}"));
    }
    p.out(out);
    Ok(())
}

/// Everything `e120 config gen` produces for a spec.
pub struct GenOutputs {
    /// `spec.name`, the stem of the output files.
    pub name: String,
    /// The format the file is in.
    pub format: rcvbp::Format,
    /// The configuration file bytes (`.rcvbp` for the Colorlight codec).
    pub rcvbp: Vec<u8>,
    /// The 256-byte basic-pack body.
    pub basic_pack: Vec<u8>,
    /// The 64 KB block-7 boot image; `None` when it could not be built, with
    /// the reason as the last note.
    pub block7: Option<Vec<u8>>,
    /// One line per byte range placed in the `.rcvbp` and the pack.
    pub sources: Vec<String>,
    /// The image builder's notes and the pages it wrote.
    pub notes: Vec<String>,
    /// The `<name>-sources.txt` text.
    pub report: String,
    /// Files written, in the order `e120 config gen` prints them; empty
    /// when nothing was written.
    pub paths: Vec<String>,
}

/// Generate a spec's configuration in memory.
///
/// The `.rcvbp`, the basic pack, the boot image laid out for `card` and the
/// sources report. `label` names the spec in the report; `format` names a
/// codec in `rcvbp::formats()`; `load` resolves `[chip].library`.
///
/// # Errors
/// Fails on an unknown format, an invalid spec or chip library. A boot-image
/// build failure is not an error here: `block7` is `None` and the reason is
/// the last note.
pub fn generate(
    card: &CardModel,
    spec: &PanelSpec,
    label: &str,
    format: &str,
    load: Loader,
) -> Result<GenOutputs> {
    // One codec is registered; the lookup is what refuses an unknown name.
    // The pack and the boot image are the E320 line's, built beside the file.
    let format = rcvbp::codec(format)?.format();
    let g = rcvbp::spec::generate(spec, &spec.chip_library(load)?)?;
    let rcvbp = g.rcvbp.to_file_bytes()?;

    let mut notes = Vec::new();
    let block7 = match rcvbp::image::compile(&card.memory.boot_image, spec, &g) {
        Ok(b) => {
            notes.extend(b.notes);
            notes.push(format!(
                "pages written: {}: {}",
                b.changed_pages.len(),
                hex(&b.changed_pages, " ")
            ));
            Some(b.image)
        }
        Err(e) => {
            notes.push(format!("{e:#}"));
            None
        }
    };

    let mut report = String::new();
    let _ = writeln!(report, "spec: {label}\n\n# record and pack sources");
    for line in &g.sources {
        report.push_str(line);
        report.push('\n');
    }
    report.push_str("\n# compiled image\n");
    for n in &notes {
        report.push_str(n);
        report.push('\n');
    }

    Ok(GenOutputs {
        name: spec.name.clone(),
        format,
        rcvbp,
        basic_pack: g.basic_pack.to_vec(),
        block7,
        sources: g.sources,
        notes,
        report,
        paths: Vec::new(),
    })
}

/// Generate a panel's configuration from a TOML spec into `out_dir`: the
/// `.rcvbp`, the basic-pack body, the compiled block-7 boot image, and a file
/// listing where every placed byte came from.
pub fn gen_config(
    card: &CardModel,
    spec_path: &str,
    out_dir: &str,
    format: &str,
    load: Loader,
    p: &mut dyn Progress,
) -> Result<GenOutputs> {
    let spec = PanelSpec::load(spec_path)?;
    let mut g = generate(card, &spec, spec_path, format, load)?;

    std::fs::create_dir_all(out_dir).with_context(|| format!("create {out_dir}"))?;
    let stem = format!("{out_dir}/{}", g.name);
    let rcvbp_path = format!("{stem}.{}", g.format.extension);
    std::fs::write(&rcvbp_path, &g.rcvbp).with_context(|| format!("write {rcvbp_path}"))?;
    let pack_path = format!("{stem}-basic-pack.bin");
    std::fs::write(&pack_path, &g.basic_pack).with_context(|| format!("write {pack_path}"))?;

    let Some(img) = &g.block7 else {
        anyhow::bail!("{}", g.notes.last().map_or("", String::as_str));
    };
    let img_path = format!("{stem}-block7.bin");
    std::fs::write(&img_path, img).with_context(|| format!("write {img_path}"))?;
    let report_path = format!("{stem}-sources.txt");
    std::fs::write(&report_path, &g.report).with_context(|| format!("write {report_path}"))?;

    p.out(&format!(
        "{rcvbp_path}\n{pack_path}\n{img_path}\n{report_path}"
    ));
    g.paths = vec![rcvbp_path, pack_path, img_path, report_path];
    Ok(g)
}
