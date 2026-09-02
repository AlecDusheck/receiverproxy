//! The module's functions as plain Rust: `anyhow` errors in, serde structs
//! out. `lib.rs` wraps each one for JavaScript; the tests here compare the
//! results with the CLI's files.

use anyhow::{anyhow, Context, Result};
use wall::Canvas;
use panelspec::{embedded, PanelSpec};
use rcvbp::record01::{View, LEN as RECORD01_LEN};
use rcvbp::{image, Rcvbp};
use serde::{Serialize, Serializer};
use std::fmt::Write as _;


fn bytes<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_bytes(v)
}

// serde's `serialize_with` hands over `&Option<_>`; the signature is its.
#[allow(clippy::ref_option)]
fn opt_bytes<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(b) => s.serialize_bytes(b),
        None => s.serialize_none(),
    }
}

/// The bytes fields cross into JavaScript as `Uint8Array` (`serialize_bytes`),
/// which is what the `ts` attributes say.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Generated {
    pub name: String,
    #[serde(serialize_with = "bytes")]
    #[cfg_attr(feature = "ts", ts(type = "Uint8Array"))]
    pub rcvbp: Vec<u8>,
    #[serde(serialize_with = "bytes")]
    #[cfg_attr(feature = "ts", ts(type = "Uint8Array"))]
    pub basic_pack: Vec<u8>,
    #[serde(serialize_with = "opt_bytes")]
    #[cfg_attr(feature = "ts", ts(type = "Uint8Array | null"))]
    pub block7: Option<Vec<u8>>,
    pub sources: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Inspection {
    pub version: u32,
    pub cabinet: Option<(u16, u16)>,
    pub records: Vec<RecordInfo>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RecordInfo {
    pub offset: u32,
    #[serde(rename = "type")]
    pub rtype: String,
    pub id: u8,
    pub length: u32,
    pub nonzero: u32,
    pub empty: bool,
    pub description: &'static str,
    pub fields: Option<Record01>,
}

/// Every `record01::View` accessor.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Record01 {
    pub module_width: u16,
    pub module_height_stored: u16,
    pub scan: u8,
    pub serial_clock: u16,
    pub gray: u8,
    pub luminance_level: u16,
    pub max_width: u16,
    pub max_height: u16,
    pub grid: (u16, u16),
    pub line_dir: u8,
    pub split_segment: u8,
    pub segments: u32,
    pub min_oe: f32,
    pub hr_style: u8,
    pub hr_scan_style: u8,
    pub chip_id: u16,
    #[serde(serialize_with = "bytes")]
    #[cfg_attr(feature = "ts", ts(type = "Uint8Array"))]
    pub swap_ramp: Vec<u8>,
    #[serde(serialize_with = "bytes")]
    #[cfg_attr(feature = "ts", ts(type = "Uint8Array"))]
    pub chip_custom: Vec<u8>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Diff {
    pub a_records: u32,
    pub b_records: u32,
    pub only_a: Vec<String>,
    pub only_b: Vec<String>,
    pub records: Vec<RecordDiff>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RecordDiff {
    #[serde(rename = "type")]
    pub rtype: String,
    pub len_a: u32,
    pub len_b: u32,
    pub offsets: Vec<u32>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Libraries {
    pub chips: Vec<LibraryChip>,
    pub panels: Vec<LibraryPanel>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LibraryChip {
    pub path: &'static str,
    pub name: String,
    pub toml: &'static str,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LibraryPanel {
    pub path: &'static str,
    pub name: String,
    pub toml: &'static str,
    pub mined: bool,
}

/// The chip library text for a `[chip].library` path, from the embedded set.
fn embedded_chip(path: &str) -> Result<String> {
    embedded::chip(path)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("chip library {path}: not in the embedded library"))
}

/// What `e120 config gen` writes, in memory.
///
/// The boot image is laid out for the first tested card model. A block-7
/// build failure leaves `block7` empty and puts the reason in `notes`; the
/// `.rcvbp` and the pack are still returned.
pub fn generate(spec_toml: &str) -> Result<Generated> {
    let spec = PanelSpec::parse(spec_toml).context("parse spec")?;
    let chip = spec.chip_library(&embedded_chip)?;
    let g = rcvbp::spec::generate(&spec, &chip)?;
    let rcvbp = g.rcvbp.to_file_bytes()?;
    let card = &receivers::default_model().memory.boot_image;
    let (block7, notes) = match image::compile(card, &spec, &g) {
        Ok(b) => {
            let mut notes = b.notes;
            notes.push(format!(
                "pages written: {}: {}",
                b.changed_pages.len(),
                hex(&b.changed_pages)
            ));
            (Some(b.image), notes)
        }
        Err(e) => (None, vec![format!("{e:#}")]),
    };
    Ok(Generated {
        name: spec.name,
        rcvbp,
        basic_pack: g.basic_pack.to_vec(),
        block7,
        sources: g.sources,
        notes,
    })
}

/// Lowercase hex bytes separated by spaces, as `e120 config gen` lists pages.
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The record listing of `e120 config info`, with record 0x01 decoded.
pub fn inspect(rcvbp: &[u8]) -> Result<Inspection> {
    let f = Rcvbp::from_bytes(rcvbp).context("parse rcvbp")?;
    let records = f
        .records
        .iter()
        .map(|r| RecordInfo {
            offset: r.offset as u32,
            rtype: format!("0x{:04x}", r.type_u16()),
            id: r.id(),
            length: r.payload.len() as u32,
            nonzero: r.payload.iter().filter(|&&b| b != 0).count() as u32,
            empty: r.is_empty_table(),
            description: r.describe(),
            fields: (r.id() == 0x01 && r.payload.len() >= RECORD01_LEN)
                .then(|| View::new(&r.payload).ok().map(record01))
                .flatten(),
        })
        .collect();
    Ok(Inspection {
        version: f.version,
        cabinet: f.geometry(),
        records,
    })
}

fn record01(v: View<'_>) -> Record01 {
    Record01 {
        module_width: v.module_width(),
        module_height_stored: v.module_height_stored(),
        scan: v.scan(),
        serial_clock: v.serial_clock(),
        gray: v.gray(),
        luminance_level: v.luminance_level(),
        max_width: v.max_width(),
        max_height: v.max_height(),
        grid: v.grid(),
        line_dir: v.line_dir(),
        split_segment: v.split_segment(),
        segments: v.segments(),
        min_oe: v.min_oe(),
        hr_style: v.hr_style(),
        hr_scan_style: v.hr_scan_style(),
        chip_id: v.chip_id(),
        swap_ramp: v.swap_ramp().to_vec(),
        chip_custom: v.chip_custom().to_vec(),
    }
}

/// `e120 config diff`, with every differing offset rather than the first 16.
pub fn diff(a: &[u8], b: &[u8]) -> Result<Diff> {
    let fa = Rcvbp::from_bytes(a).context("parse a")?;
    let fb = Rcvbp::from_bytes(b).context("parse b")?;
    let types_a: Vec<u16> = fa
        .records
        .iter()
        .map(rcvbp::Record::type_u16)
        .collect();
    let types_b: Vec<u16> = fb
        .records
        .iter()
        .map(rcvbp::Record::type_u16)
        .collect();
    let only = |xs: &[u16], ys: &[u16]| -> Vec<String> {
        xs.iter()
            .filter(|t| !ys.contains(t))
            .map(|t| format!("0x{t:04x}"))
            .collect()
    };
    let records = types_a
        .iter()
        .filter_map(|&t| {
            let (ra, rb) = (fa.find(t)?, fb.find(t)?);
            if ra.payload == rb.payload {
                return None;
            }
            let offsets = ra
                .payload
                .iter()
                .zip(&rb.payload)
                .enumerate()
                .filter(|(_, (x, y))| x != y)
                .map(|(i, _)| i as u32)
                .collect();
            Some(RecordDiff {
                rtype: format!("0x{t:04x}"),
                len_a: ra.payload.len() as u32,
                len_b: rb.payload.len() as u32,
                offsets,
            })
        })
        .collect();
    Ok(Diff {
        a_records: fa.records.len() as u32,
        b_records: fb.records.len() as u32,
        only_a: only(&types_a, &types_b),
        only_b: only(&types_b, &types_a),
        records,
    })
}

/// The embedded chip libraries and panel specs.
pub fn libraries() -> Libraries {
    let chips = embedded::CHIPS
        .iter()
        .map(|&(path, toml)| LibraryChip {
            path,
            name: toml_name(path, toml),
            toml,
        })
        .collect();
    let panels = embedded::PANELS
        .iter()
        .map(|&(path, toml)| LibraryPanel {
            path,
            name: toml_name(path, toml),
            toml,
            mined: embedded::is_mined(path),
        })
        .collect();
    Libraries { chips, panels }
}

/// The file's `name =`, else its stem.
fn toml_name(path: &str, text: &str) -> String {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("name")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| {
            let file = path.rsplit('/').next().unwrap_or(path);
            file.strip_suffix(".toml").unwrap_or(file).to_owned()
        })
}

/// `"ok"`, or the `LayoutError` text for a layout that cannot be driven.
pub fn validate_layout(json: &str) -> Result<String> {
    let canvas: Canvas = serde_json::from_str(json).context("parse layout")?;
    Ok(match canvas.validate() {
        Ok(()) => "ok".to_owned(),
        Err(e) => e.to_string(),
    })
}

/// `Canvas::cards(w, h, cols, rows)` as `e120 card layout-example` prints it.
pub fn layout_example(cols: u32, rows: u32, w: u32, h: u32) -> Result<String> {
    Ok(serde_json::to_string_pretty(&Canvas::cards(
        w, h, cols, rows,
    ))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const SPEC: &str = "config/panels/p25-128x64-sm16269s.toml";
    const REFERENCE: &str = "third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp";

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn read(rel: &str) -> Vec<u8> {
        std::fs::read(root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
    }

    /// The files `e120 config gen` writes for the bench spec, generated by the
    /// CLI itself so the two paths are compared and not just the library.
    fn cli_gen() -> PathBuf {
        let out = root().join("target/rcvbp-wasm-test");
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let status = Command::new(cargo)
            .current_dir(root())
            .args([
                "run",
                "-q",
                "-p",
                "cli",
                "--",
                "config",
                "gen",
                "--spec",
                SPEC,
                "--out-dir",
            ])
            .arg(&out)
            .stdout(std::process::Stdio::null())
            .status()
            .expect("run the CLI");
        assert!(status.success(), "e120 config gen failed");
        out
    }

    #[test]
    fn generate_matches_the_cli_byte_for_byte() {
        let out = cli_gen();
        let spec = std::fs::read_to_string(root().join(SPEC)).unwrap();
        let g = generate(&spec).unwrap();
        assert_eq!(g.name, "p25-128x64-sm16269s");
        let file = |suffix: &str| std::fs::read(out.join(format!("{}{suffix}", g.name))).unwrap();
        assert_eq!(g.rcvbp, file(".rcvbp"));
        assert_eq!(g.basic_pack, file("-basic-pack.bin"));
        assert_eq!(g.block7.as_deref(), Some(file("-block7.bin").as_slice()));

        // The sources file is the two lists under fixed headings.
        let mut text = format!("spec: {SPEC}\n\n# record and pack sources\n");
        for s in &g.sources {
            text.push_str(s);
            text.push('\n');
        }
        text.push_str("\n# compiled image\n");
        for n in &g.notes {
            text.push_str(n);
            text.push('\n');
        }
        assert_eq!(text, String::from_utf8(file("-sources.txt")).unwrap());
    }

    #[test]
    fn generate_only_knows_embedded_chip_libraries() {
        let spec = std::fs::read_to_string(root().join(SPEC))
            .unwrap()
            .replace("config/chips/sm16269s-factory.toml", "config/chips/x.toml");
        let err = generate(&spec).unwrap_err();
        assert_eq!(
            format!("{err:#}"),
            "chip library config/chips/x.toml: not in the embedded library"
        );
    }

    #[test]
    fn inspect_lists_the_records_the_cli_lists() {
        let spec = std::fs::read_to_string(root().join(SPEC)).unwrap();
        let g = generate(&spec).unwrap();
        let i = inspect(&g.rcvbp).unwrap();
        assert_eq!(i.version, 4);
        assert_eq!(
            i.cabinet,
            Some((128, 32)),
            "record 0xca: width, stored height"
        );
        assert_eq!(i.records.len(), 17);
        let r = &i.records[0];
        assert_eq!(
            (
                r.offset,
                r.rtype.as_str(),
                r.id,
                r.length,
                r.nonzero,
                r.empty
            ),
            (0, "0x0a01", 1, 764, 361, false)
        );
        assert_eq!(
            r.description,
            "main receiver parameters (geometry, scan, timing)"
        );
        let f = r.fields.as_ref().unwrap();
        assert_eq!(
            (
                f.module_width,
                f.module_height_stored,
                f.scan,
                f.serial_clock
            ),
            (128, 32, 16, 8)
        );
        assert_eq!(
            (f.gray, f.max_width, f.max_height, f.line_dir),
            (12, 128, 64, 0)
        );
        assert_eq!((f.swap_ramp.len(), f.chip_custom.len()), (64, 16));
        let mapping = i.records.iter().find(|r| r.rtype == "0x0a03").unwrap();
        assert_eq!(
            (mapping.offset, mapping.length, mapping.nonzero),
            (0x0001_2539, 12290, 7921)
        );
        assert_eq!(mapping.description, "pixel/row mapping table");
        assert!(mapping.fields.is_none());
        assert_eq!(i.records[1].description, "(empty table)");
    }

    #[test]
    fn inspect_reads_the_reference_file() {
        let i = inspect(&read(REFERENCE)).unwrap();
        assert_eq!(i.records.len(), 17);
        assert!(i.records[0].fields.is_some());
    }

    #[test]
    fn diff_reports_what_the_cli_reports() {
        let spec = std::fs::read_to_string(root().join(SPEC)).unwrap();
        let g = generate(&spec).unwrap();
        let d = diff(&g.rcvbp, &read(REFERENCE)).unwrap();
        assert_eq!((d.a_records, d.b_records), (17, 17));
        assert_eq!(d.only_a, ["0x0a07"]);
        assert_eq!(d.only_b, ["0x0907"]);
        assert_eq!(d.records.len(), 2);
        assert_eq!(d.records[0].rtype, "0x0a01");
        assert_eq!((d.records[0].len_a, d.records[0].len_b), (764, 764));
        assert_eq!(d.records[0].offsets, [0x23, 0x2f, 0xc0, 0xc1, 0xc2, 0xc3]);
        assert_eq!(d.records[1].rtype, "0x0a8a");
        assert_eq!(d.records[1].offsets, [0x10, 0x11, 0x12, 0x13]);

        let same = diff(&g.rcvbp, &g.rcvbp).unwrap();
        assert!(same.records.is_empty() && same.only_a.is_empty() && same.only_b.is_empty());
    }

    #[test]
    fn libraries_are_the_config_tree_in_order() {
        let l = libraries();
        let chips: Vec<&str> = l.chips.iter().map(|c| c.path).collect();
        let panels: Vec<&str> = l.panels.iter().map(|p| p.path).collect();
        assert!(chips.contains(&"config/chips/sm16269s-factory.toml"));
        assert!(chips.contains(&"config/chips/mined/icn2053.toml"));
        assert_eq!(panels[0], SPEC);
        assert!(!l.panels[0].mined);
        assert!(l.panels[1].mined && l.panels[1].path.starts_with("config/panels/mined/"));
        let is_sorted = |xs: &[&str]| {
            let (plain, mined): (Vec<&str>, Vec<&str>) =
                xs.iter().partition(|p| !p.contains("/mined/"));
            xs.iter().take(plain.len()).all(|p| !p.contains("/mined/"))
                && plain.windows(2).all(|w| w[0] < w[1])
                && mined.windows(2).all(|w| w[0] < w[1])
        };
        assert!(is_sorted(&chips) && is_sorted(&panels));
        let factory = l
            .chips
            .iter()
            .find(|c| c.path == "config/chips/sm16269s-factory.toml")
            .unwrap();
        assert_eq!(factory.name, "SM16269S (factory values)");
        assert!(factory.toml.starts_with("# Driver-chip library"));
        assert_eq!(l.panels[0].name, "p25-128x64-sm16269s");
    }

    #[test]
    fn layouts_round_trip_through_the_canvas_crate() {
        let example = layout_example(2, 1, 128, 64).unwrap();
        assert_eq!(
            example,
            serde_json::to_string_pretty(&Canvas::cards(128, 64, 2, 1)).unwrap()
        );
        assert_eq!(validate_layout(&example).unwrap(), "ok");
        let bad = example.replace("\"width\": 256", "\"width\": 128");
        let text = validate_layout(&bad).unwrap();
        assert!(
            text.starts_with("canvas is not valid:\n  receiver 1 at (128, 0)"),
            "{text}"
        );
        assert!(validate_layout("{")
            .unwrap_err()
            .to_string()
            .starts_with("parse layout"));
    }
}
