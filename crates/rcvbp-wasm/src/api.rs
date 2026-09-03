//! The module's functions as plain Rust: `anyhow` errors in, serde structs
//! out. `lib.rs` wraps each one for JavaScript; the tests here compare the
//! results with the CLI's files.

use anyhow::{anyhow, Context, Result};
use panelspec::{embedded, ChipLibrary, Meta, PanelSpec, Status};
use rcvbp::record01::{View, LEN as RECORD01_LEN};
use rcvbp::{image, Format, Rcvbp};
use serde::{Serialize, Serializer};
use std::fmt::Write as _;
use wall::Canvas;

fn bytes<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_bytes(v)
}

/// What `generate` produced: the files `rxp config gen` would write, named
/// as it names them, minus the sources report (`sources` and `notes` are
/// its two lists).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Generated {
    pub name: String,
    pub files: Vec<GenFile>,
    pub sources: Vec<String>,
    pub notes: Vec<String>,
}

/// One output file. The bytes cross into JavaScript as a `Uint8Array`
/// (`serialize_bytes`), which is what the `ts` attribute says.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GenFile {
    pub name: String,
    #[serde(serialize_with = "bytes")]
    #[cfg_attr(feature = "ts", ts(type = "Uint8Array"))]
    pub bytes: Vec<u8>,
}

/// What `import` recovered from a vendor file.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Imported {
    /// The spec as TOML, the form the Builder edits.
    pub spec_toml: String,
    /// Fields the file did not determine, by name.
    pub unresolved: Vec<String>,
    /// The registry format the file was read as, or `spec` for a TOML spec.
    pub format: &'static str,
}

/// One embedded panel spec as the gallery lists it.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Entry {
    pub path: &'static str,
    pub name: String,
    pub meta: Meta,
    pub module: EntryModule,
    pub chip: EntryChip,
    /// Registry formats that can generate for the entry.
    pub formats: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EntryModule {
    pub width: u16,
    pub height: u16,
    pub scan: u8,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EntryChip {
    /// The `[chip].library` path.
    pub library: String,
    /// The library's `name`.
    pub name: String,
    pub family_id: u16,
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
    /// The library's own `status`; `derived` when the file states none.
    pub status: Status,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LibraryPanel {
    pub path: &'static str,
    pub name: String,
    pub toml: &'static str,
    /// The spec's `[meta] status`; `derived` when the file states none.
    pub status: Status,
}

/// The chip library text for a `[chip].library` path, from the embedded set.
fn embedded_chip(path: &str) -> Result<String> {
    embedded::chip(path)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("chip library {path}: not in the embedded library"))
}

/// What `rxp config gen --format FORMAT` writes, in memory: the file, the
/// basic pack, and the boot image when it builds.
///
/// The boot image is laid out for the first tested card model. A block-7
/// build failure leaves it out and puts the reason in `notes`; the file and
/// the pack are still returned.
///
/// # Errors
/// An unknown `format` names the known ones; a spec or chip library the
/// format cannot hold fails as `rcvbp::spec::generate` does.
pub fn generate(spec_toml: &str, format: &str) -> Result<Generated> {
    // One codec is registered; the lookup is what refuses an unknown name.
    let format = rcvbp::codec(format)?.format();
    let spec = PanelSpec::parse(spec_toml).context("parse spec")?;
    let chip = spec.chip_library(&embedded_chip)?;
    let g = rcvbp::spec::generate(&spec, &chip)?;
    let mut files = vec![
        GenFile {
            name: format!("{}.{}", spec.name, format.extension),
            bytes: g.rcvbp.to_file_bytes()?,
        },
        GenFile {
            name: format!("{}-basic-pack.bin", spec.name),
            bytes: g.basic_pack.to_vec(),
        },
    ];
    let card = &receivers::default_model().memory.boot_image;
    let notes = match image::compile(card, &spec, &g) {
        Ok(b) => {
            let mut notes = b.notes;
            notes.push(format!(
                "pages written: {}: {}",
                b.changed_pages.len(),
                hex(&b.changed_pages)
            ));
            files.push(GenFile {
                name: format!("{}-block7.bin", spec.name),
                bytes: b.image,
            });
            notes
        }
        Err(e) => vec![format!("{e:#}")],
    };
    Ok(Generated {
        name: spec.name,
        files,
        sources: g.sources,
        notes,
    })
}

/// `rxp config import`, in memory: the spec that regenerates `bytes`.
///
/// `format` names a registry entry; without it the codec is the one whose
/// signature the bytes start with. A TOML panel spec is passed through as
/// format `spec`, so the Builder's drop target takes either.
///
/// # Errors
/// An unknown `format` names the known ones; bytes no codec recognises and
/// that do not parse as a spec fail as `rcvbp::detect` does.
pub fn import(bytes: &[u8], format: Option<&str>) -> Result<Imported> {
    let codec = match format {
        Some(name) => rcvbp::codec(name)?,
        None => match rcvbp::detect(bytes) {
            Ok(c) => c,
            Err(e) => {
                if let Some(spec) = std::str::from_utf8(bytes).ok().filter(|t| PanelSpec::parse(t).is_ok()) {
                    return Ok(Imported { spec_toml: spec.to_owned(), unresolved: Vec::new(), format: "spec" });
                }
                return Err(e);
            }
        },
    };
    let chips = |id: u16| embedded::chip_by_family(id).map(|(p, t)| (p.to_owned(), t.to_owned()));
    let (spec, unresolved) = codec.import(bytes, &chips)?;
    Ok(Imported {
        spec_toml: spec.to_toml()?,
        unresolved,
        format: codec.format().name,
    })
}

/// The embedded panel specs as the gallery shows them.
///
/// # Errors
/// Fails on an embedded spec or chip library that does not parse, which the
/// `panelspec` tests keep from being embedded.
pub fn gallery() -> Result<Vec<Entry>> {
    let generators: Vec<&'static str> = rcvbp::formats()
        .filter(|f| f.generate)
        .map(|f| f.name)
        .collect();
    embedded::specs()?
        .into_iter()
        .map(|(path, spec)| {
            let chip = ChipLibrary::parse(&embedded_chip(&spec.chip.library)?)
                .with_context(|| format!("parse {}", spec.chip.library))?;
            Ok(Entry {
                path,
                name: spec.name,
                meta: spec.meta,
                module: EntryModule {
                    width: spec.module.width,
                    height: spec.module.height,
                    scan: spec.module.scan,
                },
                chip: EntryChip {
                    library: spec.chip.library,
                    name: chip.name,
                    family_id: chip.family_id,
                },
                formats: generators.clone(),
            })
        })
        .collect()
}

/// The codec registry, as `rxp config formats` lists it.
pub fn formats() -> Vec<Format> {
    rcvbp::formats().collect()
}

/// Lowercase hex bytes separated by spaces, as `rxp config gen` lists pages.
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

/// The record listing of `rxp config info`, with record 0x01 decoded.
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

/// `rxp config diff`, with every differing offset rather than the first 16.
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
            status: toml_status(toml),
        })
        .collect();
    let panels = embedded::PANELS
        .iter()
        .map(|&(path, toml)| LibraryPanel {
            path,
            name: toml_name(path, toml),
            toml,
            status: toml_status(toml),
        })
        .collect();
    Libraries { chips, panels }
}

/// The file's `status`, a chip library's own or a spec's `[meta]` one;
/// `derived` when the file states none.
fn toml_status(text: &str) -> Status {
    let table = text.parse::<toml::Table>().unwrap_or_default();
    let raw = table
        .get("status")
        .or_else(|| table.get("meta")?.get("status"))
        .and_then(toml::Value::as_str);
    match raw {
        Some("verified") => Status::Verified,
        Some("stub") => Status::Stub,
        _ => Status::Derived,
    }
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

/// `Canvas::cards(w, h, cols, rows)` as `rxp card layout-example` prints it.
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

    /// The files `rxp config gen` writes for the bench spec, generated by the
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
                "receiverproxy",
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
        assert!(status.success(), "rxp config gen failed");
        out
    }

    /// The bytes of the output file `name` in `g`.
    fn produced<'a>(g: &'a Generated, name: &str) -> &'a [u8] {
        &g.files.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no {name}")).bytes
    }

    #[test]
    fn generate_matches_the_cli_byte_for_byte() {
        let out = cli_gen();
        let spec = std::fs::read_to_string(root().join(SPEC)).unwrap();
        let g = generate(&spec, "rcvbp").unwrap();
        assert_eq!(g.name, "p25-128x64-sm16269s");
        let names: Vec<&str> = g.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "p25-128x64-sm16269s.rcvbp",
                "p25-128x64-sm16269s-basic-pack.bin",
                "p25-128x64-sm16269s-block7.bin"
            ]
        );
        for name in names {
            assert_eq!(produced(&g, name), std::fs::read(out.join(name)).unwrap(), "{name}");
        }
        let file = |suffix: &str| std::fs::read(out.join(format!("{}{suffix}", g.name))).unwrap();

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
            .replace("config/chips/sm16269s.toml", "config/chips/x.toml");
        let err = generate(&spec, "rcvbp").unwrap_err();
        assert_eq!(
            format!("{err:#}"),
            "chip library config/chips/x.toml: not in the embedded library"
        );
    }

    #[test]
    fn generate_refuses_a_format_the_registry_lacks() {
        let spec = std::fs::read_to_string(root().join(SPEC)).unwrap();
        let err = generate(&spec, "novastar").unwrap_err();
        assert_eq!(format!("{err:#}"), "format novastar: unknown; known formats: rcvbp");
    }

    #[test]
    fn formats_and_gallery_read_the_embedded_sets() {
        let f = formats();
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].name, f[0].vendor, f[0].extension), ("rcvbp", "Colorlight", "rcvbp"));
        assert!(f[0].generate && f[0].import);

        let g = gallery().unwrap();
        assert_eq!(g.len(), embedded::PANELS.len());
        let bench = &g[0];
        assert_eq!(bench.path, SPEC);
        assert_eq!(bench.name, "p25-128x64-sm16269s");
        assert_eq!(bench.meta.status, panelspec::Status::Verified);
        assert_eq!(bench.meta.pitch_mm, Some(2.5));
        assert_eq!((bench.module.width, bench.module.height, bench.module.scan), (128, 64, 16));
        assert_eq!(bench.chip.library, "config/chips/sm16269s.toml");
        assert_eq!(bench.chip.name, "SM16269S");
        assert_eq!(bench.chip.family_id, 0x14C);
        assert_eq!(bench.formats, ["rcvbp"]);
        let derived = g.iter().find(|e| e.path == "config/panels/64x64-16s-icn2053.toml").unwrap();
        assert_eq!(derived.meta.status, panelspec::Status::Derived);
        assert_eq!(derived.meta.sources, 25);
        assert_eq!(derived.meta.examples.len(), 3);
        assert_eq!(derived.chip.name, "ICN2053");
    }

    #[test]
    fn inspect_lists_the_records_the_cli_lists() {
        let spec = std::fs::read_to_string(root().join(SPEC)).unwrap();
        let g = generate(&spec, "rcvbp").unwrap();
        let i = inspect(produced(&g, "p25-128x64-sm16269s.rcvbp")).unwrap();
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
        let g = generate(&spec, "rcvbp").unwrap();
        let rcvbp = produced(&g, "p25-128x64-sm16269s.rcvbp");
        let d = diff(rcvbp, &read(REFERENCE)).unwrap();
        assert_eq!((d.a_records, d.b_records), (17, 17));
        assert_eq!(d.only_a, ["0x0a07"]);
        assert_eq!(d.only_b, ["0x0907"]);
        assert_eq!(d.records.len(), 2);
        assert_eq!(d.records[0].rtype, "0x0a01");
        assert_eq!((d.records[0].len_a, d.records[0].len_b), (764, 764));
        assert_eq!(d.records[0].offsets, [0x23, 0x2f, 0xc0, 0xc1, 0xc2, 0xc3]);
        assert_eq!(d.records[1].rtype, "0x0a8a");
        assert_eq!(d.records[1].offsets, [0x10, 0x11, 0x12, 0x13]);

        let same = diff(rcvbp, rcvbp).unwrap();
        assert!(same.records.is_empty() && same.only_a.is_empty() && same.only_b.is_empty());
    }

    #[test]
    fn import_reads_a_file_back_into_the_spec_that_generates_it() {
        let spec = std::fs::read_to_string(root().join(SPEC)).unwrap();
        let g = generate(&spec, "rcvbp").unwrap();
        let rcvbp = produced(&g, "p25-128x64-sm16269s.rcvbp");
        let i = import(rcvbp, None).unwrap();
        assert_eq!(i.format, "rcvbp");
        assert_eq!(i.unresolved, ["meta", "mapping.gate_phantom_positions", "boot.arm_at_boot"]);
        assert!(i.spec_toml.starts_with("name = \"128x64-16s-sm16269s\"\n"), "{}", i.spec_toml);
        let again = generate(&i.spec_toml, "rcvbp").unwrap();
        assert_eq!(produced(&again, "128x64-16s-sm16269s.rcvbp"), rcvbp);
        assert_eq!(import(rcvbp, Some("rcvbp")).unwrap().spec_toml, i.spec_toml);

        let reference = import(&read(REFERENCE), None).unwrap();
        assert!(reference.spec_toml.contains("\nwidth = 256\nheight = 384\n"), "{}", reference.spec_toml);
        assert!(!reference.spec_toml.contains("record01_overrides"));

        let passed = import(spec.as_bytes(), None).unwrap();
        assert_eq!((passed.format, passed.spec_toml.as_str()), ("spec", spec.as_str()));
        assert!(passed.unresolved.is_empty());
        let err = import(b"neither", None).unwrap_err();
        assert_eq!(
            format!("{err:#}"),
            "format: not recognised from the file's first bytes; known formats: rcvbp"
        );
        let err = import(rcvbp, Some("novastar")).unwrap_err();
        assert_eq!(format!("{err:#}"), "format novastar: unknown; known formats: rcvbp");
    }

    #[test]
    fn libraries_are_the_config_tree_in_order() {
        let l = libraries();
        let chips: Vec<(Status, &str)> = l.chips.iter().map(|c| (c.status, c.path)).collect();
        let panels: Vec<(Status, &str)> = l.panels.iter().map(|p| (p.status, p.path)).collect();
        assert!(chips.iter().any(|&(_, p)| p == "config/chips/sm16269s.toml"));
        assert!(chips.iter().any(|&(_, p)| p == "config/chips/icn2053.toml"));
        assert_eq!(panels[0], (Status::Verified, SPEC));
        assert_eq!(l.panels[1].status, Status::Derived);
        assert_eq!(chips[0], (Status::Verified, "config/chips/sm16269s.toml"));
        // Verified first, then the rest, each alphabetical by path.
        let is_sorted = |xs: &[(Status, &str)]| {
            let split = xs.iter().take_while(|(s, _)| *s == Status::Verified).count();
            let (verified, rest) = xs.split_at(split);
            rest.iter().all(|(s, _)| *s != Status::Verified)
                && verified.windows(2).all(|w| w[0].1 < w[1].1)
                && rest.windows(2).all(|w| w[0].1 < w[1].1)
        };
        assert!(is_sorted(&chips) && is_sorted(&panels));
        let bench = l
            .chips
            .iter()
            .find(|c| c.path == "config/chips/sm16269s.toml")
            .unwrap();
        assert_eq!(bench.name, "SM16269S");
        assert_eq!(bench.status, Status::Verified);
        assert!(bench.toml.starts_with("# SM16269S (chip id 0x014C"));
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
        // Two cards addressed at the same screen window.
        let bad = example.replace("\"x\": 128,\n      \"y\": 0,\n      \"width\": 128,\n      \"height\": 64\n    }\n  ],", "\"x\": 0,\n      \"y\": 0,\n      \"width\": 128,\n      \"height\": 64\n    }\n  ],");
        let text = validate_layout(&bad).unwrap();
        assert!(text.contains("overlap in screen space"), "{text}");
        assert!(validate_layout("{")
            .unwrap_err()
            .to_string()
            .starts_with("parse layout"));
    }
}
