//! Reader and writer for Colorlight `.rcvbp` receiver-parameter files
//! (`docs/rcvbp-format.md`). The record stream must tile the blob exactly.
//!
//! ```text
//! file:   [32-byte header][zlib stream][u32 CRC trailer]
//! header: 0x00 16 bytes  signature
//!         0x10 u32       version (4)
//!         0x14 u32       compressed size
//!         0x18 u32       decompressed size
//!         0x1c u32       reserved (0)
//! record: [u16 size_le][u16 type][payload; size-4]   (size counts the header)
//! ```

pub mod image;
pub mod record01;
pub mod spec;

pub use panelspec;
pub use spec::ChipLookup;

use anyhow::{bail, Context, Result};
use panelspec::{ChipLibrary, PanelSpec};
use serde::Serialize;
use std::borrow::Cow;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Offset of the record within the decompressed blob.
    pub offset: usize,
    /// Record type: the two bytes stored after the size field.
    pub rtype: [u8; 2],
    pub payload: Vec<u8>,
}

impl Record {
    /// A record not read from a file, ready to be written into one.
    pub fn new(rtype: u16, payload: Vec<u8>) -> Self {
        Self {
            offset: 0,
            rtype: rtype.to_be_bytes(),
            payload,
        }
    }

    pub fn type_u16(&self) -> u16 {
        u16::from_be_bytes(self.rtype)
    }

    /// The id byte alone; the vendor parser ignores the marker byte before it.
    #[must_use]
    pub fn id(&self) -> u8 {
        self.rtype[1]
    }

    /// True when the record carries no actual settings (empty table).
    pub fn is_empty_table(&self) -> bool {
        self.payload.iter().all(|&b| b == 0)
    }

    /// What the record holds, for listings; empty for a type not yet decoded.
    #[must_use]
    pub fn describe(&self) -> &'static str {
        match (self.type_u16(), self.is_empty_table()) {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rcvbp {
    pub version: u32,
    pub records: Vec<Record>,
}

impl Rcvbp {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let d = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
        Self::from_bytes(&d).with_context(|| format!("parse {}", path.display()))
    }

    pub fn from_bytes(d: &[u8]) -> Result<Self> {
        if d.len() < 32 {
            bail!("too short to be a .rcvbp");
        }
        let version = le_u32(d, 0x10)?;

        // Signed files zlib-compress the record stream; legacy ones store it
        // inline after the version field, with the CRC trailer inside it (slack).
        let (blob, slack): (Cow<[u8]>, usize) = if d[0..4] == SIG_COMPRESSED {
            let raw_len = le_u32(d, 0x18)? as usize;
            let mut blob = Vec::with_capacity(raw_len.min(1 << 20));
            flate2::read::ZlibDecoder::new(&d[0x20..])
                .read_to_end(&mut blob)
                .context("inflate rcvbp payload")?;
            if blob.len() != raw_len {
                bail!("inflated {} bytes but header says {raw_len}", blob.len());
            }
            (Cow::Owned(blob), 0)
        } else {
            (Cow::Borrowed(&d[0x14..]), 4)
        };
        let records = parse_records(&blob, slack)?;
        Ok(Self { version, records })
    }

    pub fn find(&self, rtype: u16) -> Option<&Record> {
        self.records.iter().find(|r| r.type_u16() == rtype)
    }

    /// Cabinet geometry from the 0x0aca record: (width, scan).
    /// Panel height is not stored directly; it is scan * data groups.
    pub fn geometry(&self) -> Option<(u16, u16)> {
        let r = self.find(0x0aca)?;
        if r.payload.len() < 4 {
            return None;
        }
        Some((
            u16::from_le_bytes([r.payload[0], r.payload[1]]),
            u16::from_le_bytes([r.payload[2], r.payload[3]]),
        ))
    }

    /// Width/scan from the main 0x0a01 parameter block: (width, scan).
    pub fn main_geometry(&self) -> Option<(u8, u8)> {
        let r = self.record_01()?;
        if r.payload.len() < 2 {
            return None;
        }
        Some((r.payload[0], r.payload[1]))
    }

    /// The first record with this id byte, whatever container marker it
    /// carries (see [`Record::id`]).
    #[must_use]
    pub fn find_by_id(&self, id: u8) -> Option<&Record> {
        self.records.iter().find(|r| r.id() == id)
    }

    /// The main parameter record.
    #[must_use]
    pub fn record_01(&self) -> Option<&Record> {
        self.find_by_id(0x01)
    }

    /// Scan denominator (16, 32, 64) at record 0x01 +0x020. The byte at
    /// +0x001 is stored module height, not scan.
    pub fn scan(&self) -> Option<u8> {
        self.record_01()?.payload.get(0x20).copied()
    }

    pub fn find_mut(&mut self, rtype: u16) -> Option<&mut Record> {
        self.records.iter_mut().find(|r| r.type_u16() == rtype)
    }

    /// Replace a record's payload, or append the record if absent.
    ///
    /// New records are inserted before the trailing geometry record when there
    /// is one, matching where vendor files place them.
    pub fn upsert(&mut self, rtype: u16, payload: Vec<u8>) {
        if let Some(r) = self.find_mut(rtype) {
            r.payload = payload;
            return;
        }
        let rec = Record::new(rtype, payload);
        match self.records.iter().position(|r| r.type_u16() == 0x0aca) {
            Some(i) => self.records.insert(i, rec),
            None => self.records.push(rec),
        }
    }

    pub fn remove(&mut self, rtype: u16) -> bool {
        let before = self.records.len();
        self.records.retain(|r| r.type_u16() != rtype);
        self.records.len() != before
    }

    /// Serialise the records back into a record stream.
    ///
    /// # Errors
    /// Fails if a record is too large for the 16-bit length field.
    pub fn to_blob(&self) -> Result<Vec<u8>> {
        let len = self.records.iter().map(|r| r.payload.len() + 4).sum();
        let mut out = Vec::with_capacity(len);
        for r in &self.records {
            let size: u16 = r
                .payload
                .len()
                .checked_add(4)
                .and_then(|n| u16::try_from(n).ok())
                .with_context(|| {
                    format!(
                        "record 0x{:04x} is too large to encode ({} bytes)",
                        r.type_u16(),
                        r.payload.len()
                    )
                })?;
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&r.rtype);
            out.extend_from_slice(&r.payload);
        }
        Ok(out)
    }

    /// Serialise to a complete `.rcvbp` file in the compressed variant.
    ///
    /// # Errors
    /// Fails if a record cannot be encoded or compression fails.
    pub fn to_file_bytes(&self) -> Result<Vec<u8>> {
        let blob = self.to_blob()?;
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(&blob).context("compress record stream")?;
        let compressed = enc.finish().context("finish compression")?;

        let mut out = Vec::with_capacity(0x20 + compressed.len());
        out.extend_from_slice(&SIGNATURE);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&compressed);
        let crc = trailer_crc(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        Ok(out)
    }

    /// Write a `.rcvbp` file.
    ///
    /// # Errors
    /// Fails if serialisation or the write fails.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let bytes = self.to_file_bytes()?;
        std::fs::write(path, &bytes).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }
}

/// A generated configuration file and where each of its bytes came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    /// The file bytes the card's tooling loads.
    pub file: Vec<u8>,
    /// One line per byte range placed, with its source.
    pub sources: Vec<String>,
}

/// A registry entry: what a format is called and what its codec can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Format {
    /// The name `--format` and the site use.
    pub name: &'static str,
    pub vendor: &'static str,
    /// File extension without the dot.
    pub extension: &'static str,
    /// The codec writes a file from a spec.
    pub generate: bool,
    /// The codec reads a file back into a spec.
    pub import: bool,
}

/// One vendor's configuration format: a panel spec in, the card's file out,
/// and that file read back.
///
/// [`RcvbpCodec`] is the Colorlight implementation; a second vendor
/// implements this in its own crate (docs/cards.md) and adds it to
/// [`codecs`].
pub trait Codec: Sync {
    /// The registry entry.
    fn format(&self) -> Format;
    /// True when `file` starts the way this format's files start; what
    /// [`detect`] reads.
    fn matches(&self, file: &[u8]) -> bool;
    /// The file for `spec` and its chip library.
    ///
    /// # Errors
    /// Fails on a spec or library the format cannot hold.
    fn generate(&self, spec: &PanelSpec, chip: &ChipLibrary) -> Result<Encoded>;
    /// One line per record of `file`, as `rxp config info` lists them.
    ///
    /// # Errors
    /// Fails when `file` is not in the format.
    fn inspect(&self, file: &[u8]) -> Result<Vec<String>>;
    /// The spec that regenerates `file`, with `chips` mapping a chip id to
    /// a library, and the fields it could not recover by name. Implemented
    /// when [`Format::import`] says so.
    ///
    /// # Errors
    /// Fails when `file` is not in the format, or the codec cannot import.
    fn import(&self, file: &[u8], chips: ChipLookup) -> Result<(PanelSpec, Vec<String>)> {
        let _ = (file, chips);
        bail!("format {}: import is not implemented", self.format().name)
    }
}

/// The `.rcvbp` format behind [`Codec`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RcvbpCodec;

impl Codec for RcvbpCodec {
    fn format(&self) -> Format {
        Format {
            name: "rcvbp",
            vendor: "Colorlight",
            extension: "rcvbp",
            generate: true,
            import: true,
        }
    }

    fn matches(&self, file: &[u8]) -> bool {
        file.starts_with(&SIG_COMPRESSED)
    }

    fn generate(&self, spec: &PanelSpec, chip: &ChipLibrary) -> Result<Encoded> {
        let g = spec::generate(spec, chip)?;
        Ok(Encoded {
            file: g.rcvbp.to_file_bytes()?,
            sources: g.sources,
        })
    }

    fn inspect(&self, file: &[u8]) -> Result<Vec<String>> {
        Ok(Rcvbp::from_bytes(file)?
            .records
            .iter()
            .map(|r| format!("0x{:04x} {:5} bytes  {}", r.type_u16(), r.payload.len(), r.describe()))
            .collect())
    }

    fn import(&self, file: &[u8], chips: ChipLookup) -> Result<(PanelSpec, Vec<String>)> {
        spec::spec_from_rcvbp(file, chips)
    }
}

/// The registered codecs, one per format; `rxp config formats` and the
/// site's format list read this.
#[must_use]
pub fn codecs() -> &'static [&'static dyn Codec] {
    &[&RcvbpCodec]
}

/// The registry entries, in registration order.
pub fn formats() -> impl Iterator<Item = Format> {
    codecs().iter().map(|c| c.format())
}

/// The codec registered under `name`.
///
/// # Errors
/// Names the known formats when `name` is not one of them.
pub fn codec(name: &str) -> Result<&'static dyn Codec> {
    codecs()
        .iter()
        .copied()
        .find(|c| c.format().name == name)
        .with_context(|| {
            let known: Vec<&str> = formats().map(|f| f.name).collect();
            format!("format {name}: unknown; known formats: {}", known.join(", "))
        })
}

/// The codec whose signature `file` starts with.
///
/// # Errors
/// Names the known formats when none matches.
pub fn detect(file: &[u8]) -> Result<&'static dyn Codec> {
    codecs()
        .iter()
        .copied()
        .find(|c| c.matches(file))
        .with_context(|| {
            let known: Vec<&str> = formats().map(|f| f.name).collect();
            format!("format: not recognised from the file's first bytes; known formats: {}", known.join(", "))
        })
}

/// CRC-32, reflected polynomial 0xEDB88320. The file trailer and the basic
/// pack use it with different init/final xor.
mod crc32 {
    const TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 == 1 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    };

    pub fn update(mut crc: u32, data: &[u8]) -> u32 {
        for &b in data {
            crc = TABLE[((crc ^ u32::from(b)) & 0xff) as usize] ^ (crc >> 8);
        }
        crc
    }
}

/// The trailer CRC: CRC-32 over the file up to the trailer, init 0, no final
/// inversion (so it does not match a stock CRC-32). Pinned by `crc_tests`.
#[must_use]
pub fn trailer_crc(data: &[u8]) -> u32 {
    crc32::update(0, data)
}

/// 16-byte signature of the compressed variant, copied from vendor files.
const SIGNATURE: [u8; 16] = [
    0x20, 0x20, 0x19, 0xbe, 0x74, 0x23, 0x43, 0x45, 0xb1, 0xc7, 0x93, 0x03, 0x9b, 0x83, 0xae, 0xab,
];

/// The first four signature bytes, enough to tell the variants apart.
const SIG_COMPRESSED: [u8; 4] = [0x20, 0x20, 0x19, 0xbe];

fn parse_records(blob: &[u8], slack: usize) -> Result<Vec<Record>> {
    let mut records = Vec::new();
    let mut off = 0usize;
    while off + 4 + slack <= blob.len() {
        let size = u16::from_le_bytes([blob[off], blob[off + 1]]) as usize;
        if size < 4 {
            bail!("record at 0x{off:05x} has bogus size {size}");
        }
        if off + size > blob.len() {
            bail!(
                "record at 0x{off:05x} size {size} overruns blob ({} bytes left)",
                blob.len() - off
            );
        }
        records.push(Record {
            offset: off,
            rtype: [blob[off + 2], blob[off + 3]],
            payload: blob[off + 4..off + size].to_vec(),
        });
        off += size;
    }
    if blob.len() - off > slack {
        bail!(
            "records do not tile the blob: ended at 0x{off:05x} of 0x{:05x}",
            blob.len()
        );
    }
    Ok(records)
}

fn le_u32(d: &[u8], off: usize) -> Result<u32> {
    let b: [u8; 4] = d
        .get(off..off + 4)
        .context("truncated rcvbp header")?
        .try_into()?;
    Ok(u32::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identities(records: &[Record]) -> Vec<(u16, &[u8])> {
        records.iter().map(|r| (r.type_u16(), r.payload.as_slice())).collect()
    }

    fn sample() -> Rcvbp {
        Rcvbp {
            version: 4,
            records: vec![
                Record::new(0x0a01, vec![0x80, 0x20, 1, 0]),
                Record::new(0x0a03, vec![7; 32]),
                Record::new(0x0aca, vec![0x80, 0, 0x20, 0]),
            ],
        }
    }

    #[test]
    fn the_codec_generates_and_reads_back_the_bench_spec() {
        let spec = PanelSpec::parse(panelspec::embedded::PANELS[0].1).unwrap();
        let chip = spec.chip_library(&|p| {
            panelspec::embedded::chip(p).map(str::to_owned).ok_or_else(|| anyhow::anyhow!("{p}"))
        }).unwrap();
        let e = RcvbpCodec.generate(&spec, &chip).unwrap();
        assert_eq!(e.file, spec::generate(&spec, &chip).unwrap().rcvbp.to_file_bytes().unwrap());
        assert!(!e.sources.is_empty());
        let lines = RcvbpCodec.inspect(&e.file).unwrap();
        assert_eq!(lines.len(), 17);
        assert_eq!(lines[0], "0x0a01   764 bytes  main receiver parameters (geometry, scan, timing)");
        assert!(RcvbpCodec.inspect(&[0; 8]).is_err());
    }

    #[test]
    fn the_registry_names_its_formats_and_refuses_others() {
        let names: Vec<&str> = formats().map(|f| f.name).collect();
        assert_eq!(names, ["rcvbp"]);
        let f = codec("rcvbp").unwrap().format();
        assert_eq!((f.vendor, f.extension, f.generate, f.import), ("Colorlight", "rcvbp", true, true));
        let err = codec("novastar").err().map(|e| format!("{e:#}"));
        assert_eq!(err.as_deref(), Some("format novastar: unknown; known formats: rcvbp"));
    }

    #[test]
    fn the_signature_bytes_pick_the_codec() {
        let file = sample().to_file_bytes().unwrap();
        assert_eq!(detect(&file).unwrap().format().name, "rcvbp");
        let err = detect(b"name = \"t\"\n").err().map(|e| format!("{e:#}"));
        assert_eq!(
            err.as_deref(),
            Some("format: not recognised from the file's first bytes; known formats: rcvbp")
        );
    }

    #[test]
    fn records_round_trip_through_a_blob() {
        let f = sample();
        let blob = f.to_blob().unwrap();
        let parsed = parse_records(&blob, 0).unwrap();
        // `offset` differs between built and parsed records, so compare the rest.
        assert_eq!(identities(&parsed), identities(&f.records));
    }

    #[test]
    fn blob_tiles_exactly() {
        let f = sample();
        let blob = f.to_blob().unwrap();
        let expected: usize = f.records.iter().map(|r| r.payload.len() + 4).sum();
        assert_eq!(blob.len(), expected);
    }

    #[test]
    fn upsert_replaces_existing_and_appends_new_before_geometry() {
        let mut f = sample();
        f.upsert(0x0a01, vec![1, 2, 3]);
        assert_eq!(f.find(0x0a01).unwrap().payload, vec![1, 2, 3]);
        assert_eq!(f.records.len(), 3);

        f.upsert(0x0a84, vec![9; 8]);
        assert_eq!(f.records.len(), 4);
        assert_eq!(f.records.last().unwrap().type_u16(), 0x0aca);
    }

    #[test]
    fn remove_reports_whether_it_removed_anything() {
        let mut f = sample();
        assert!(f.remove(0x0a03));
        assert!(!f.remove(0x0a03));
        assert!(f.find(0x0a03).is_none());
    }

    #[test]
    fn a_written_file_parses_back_identically() {
        let f = sample();
        let bytes = f.to_file_bytes().unwrap();
        assert_eq!(&bytes[..4], &SIG_COMPRESSED);

        let dir = std::env::temp_dir().join("rcvbp-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("round-trip.rcvbp");
        std::fs::write(&path, &bytes).unwrap();

        let back = Rcvbp::load(&path).unwrap();
        assert_eq!(back.version, 4);
        assert_eq!(identities(&back.records), identities(&f.records));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_legacy_file_is_parsed_from_its_inline_record_stream() {
        let f = sample();
        let mut bytes = vec![0u8; 0x14];
        bytes[0x10..0x14].copy_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&f.to_blob().unwrap());
        bytes.extend_from_slice(&[0; 4]);
        let back = Rcvbp::from_bytes(&bytes).unwrap();
        assert_eq!(identities(&back.records), identities(&f.records));
    }
}

#[cfg(test)]
mod crc_tests {
    use super::*;

    #[test]
    fn trailer_matches_the_reference_file() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp"
        );
        let d = std::fs::read(path).expect("reference config");
        let expected = 0x128b_ebeeu32;
        let (body, tail) = d.split_at(d.len() - 4);
        assert_eq!(trailer_crc(body), expected);
        assert_eq!(tail, &expected.to_le_bytes());
    }

    /// The reference loop the table replaced.
    fn bit_serial_crc(data: &[u8]) -> u32 {
        let mut crc: u32 = 0;
        for &byte in data {
            let mut c = (crc ^ u32::from(byte)) & 0xff;
            for _ in 0..8 {
                c = if c & 1 == 1 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
            }
            crc = (crc >> 8) ^ c;
        }
        crc
    }

    #[test]
    fn the_table_matches_the_bit_serial_loop() {
        let data: Vec<u8> = (0..4096u32).map(|i| (i * 7 + i / 3) as u8).collect();
        assert_eq!(trailer_crc(&data), bit_serial_crc(&data));
        assert_eq!(trailer_crc(&[]), 0);
    }

    #[test]
    fn a_written_file_carries_a_valid_trailer() {
        let f = Rcvbp {
            version: 4,
            records: vec![Record::new(0x0a01, vec![0x80, 0x20, 1, 0])],
        };
        let bytes = f.to_file_bytes().unwrap();
        let (body, tail) = bytes.split_at(bytes.len() - 4);
        assert_eq!(
            trailer_crc(body).to_le_bytes(),
            tail,
            "written trailer must be the CRC of the body"
        );
    }
}
