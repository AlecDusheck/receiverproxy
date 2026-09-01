//! Parser for Colorlight `.rcvbp` receiver-parameter files.
//!
//! Layout (verified against P2.5-32S-128X64-SM16269S-256X384I.rcvbp):
//!
//! ```text
//! file:   [32-byte header][zlib stream]
//! header: 0x00 16 bytes  signature/uuid
//!         0x10 u32       version (4)
//!         0x14 u32       compressed size
//!         0x18 u32       decompressed size
//!         0x1c u32       reserved (0)
//! blob:   a sequence of records that tiles the whole buffer exactly:
//!         [u16 size_le][u16 type][payload; size-4]
//!         `size` counts the 4-byte header.
//! ```
//!
//! Record types seen in the wild (type = the two header bytes as stored):
//!   0x0a01  main receiver parameter block (geometry, scan, timing)
//!   0x0a03  large pixel/row mapping table
//!   0x0a84  driver-chip register table (e.g. SM16269S)
//!   0x0a8a  secondary parameter block
//!   0x0aca  cabinet geometry (width, height, scan)
//!   0x0a83 / 0x0a89  small RGB coefficient records
//!   0x0a8d / 0x0a91 / 0x0ad8 / 0x0a95 / 0x0ada / 0x0a8e / 0x0acd / 0x008f / 0x0907
//!           gamma / calibration tables (all zero in an uncalibrated profile)

pub mod compiled;

use anyhow::{bail, Context, Result};
use std::io::{Read, Write};

#[derive(Clone)]
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
    /// True when the record carries no actual settings (empty table).
    pub fn is_empty_table(&self) -> bool {
        self.payload.iter().all(|&b| b == 0)
    }
}

pub struct Rcvbp {
    pub version: u32,
    pub blob: Vec<u8>,
    pub records: Vec<Record>,
}

impl Rcvbp {
    pub fn load(path: &str) -> Result<Self> {
        let d = std::fs::read(path).with_context(|| format!("read {path}"))?;
        Self::from_bytes(&d).with_context(|| format!("parse {path}"))
    }

    pub fn from_bytes(d: &[u8]) -> Result<Self> {
        if d.len() < 32 {
            bail!("too short to be a .rcvbp");
        }
        let version = le_u32(&d, 0x10)?;

        // Two variants exist in the wild, distinguished by their 16-byte
        // signature: the newer one zlib-compresses the record stream, the
        // older one stores it inline right after the version field and ends
        // with a 4-byte trailer.
        let (blob, compressed) = if d[0..4] == SIG_COMPRESSED {
            let raw_len = le_u32(&d, 0x18)? as usize;
            let mut blob = Vec::with_capacity(raw_len);
            flate2::read::ZlibDecoder::new(&d[0x20..])
                .read_to_end(&mut blob)
                .context("inflate rcvbp payload")?;
            if blob.len() != raw_len {
                bail!("inflated {} bytes but header says {raw_len}", blob.len());
            }
            (blob, true)
        } else {
            (d[0x14..].to_vec(), false)
        };

        // Both variants end with a 4-byte CRC trailer.
        let slack = if compressed { 0 } else { 4 };
        let records = parse_records(&blob, slack)?;
        Ok(Self {
            version,
            blob,
            records,
        })
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

    /// The main parameter record, whatever container marker it carries.
    ///
    /// The marker byte is not part of the record identity — the vendor parser
    /// takes only the id byte and ignores it — so match on the id alone.
    pub fn record_01(&self) -> Option<&Record> {
        self.records.iter().find(|r| r.rtype[1] == 0x01)
    }

    /// The driver-chip identifier, split across two bytes of record 0x01.
    ///
    /// The low byte sits at +0x036 and the high byte a long way off at +0x204;
    /// reading only the low byte silently mistakes an SM16269S (0x014c) for
    /// whatever dumb chip shares its low byte.
    pub fn chip_type(&self) -> Option<u16> {
        let p = &self.record_01()?.payload;
        Some(u16::from(*p.get(0x204)?) << 8 | u16::from(*p.get(0x036)?))
    }

    /// Scan denominator, held literally (16, 32 or 64) at record 0x01 +0x020.
    ///
    /// This is the authoritative scan field; the value near the start of the
    /// record is module geometry and is easily mistaken for it.
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
        let mut out = Vec::new();
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
    pub fn save(&self, path: &str) -> Result<()> {
        let bytes = self.to_file_bytes()?;
        std::fs::write(path, &bytes).with_context(|| format!("write {path}"))?;
        Ok(())
    }
}

/// The 4-byte trailer every `.rcvbp` ends with: a CRC-32 over the whole file
/// up to the trailer itself.
///
/// It uses the ordinary reflected polynomial but an initial value of 0 and no
/// final inversion, which is why it does not match a stock CRC-32.
#[must_use]
pub fn trailer_crc(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &byte in data {
        let mut c = (crc ^ u32::from(byte)) & 0xff;
        for _ in 0..8 {
            c = if c & 1 == 1 {
                (c >> 1) ^ 0xedb8_8320
            } else {
                c >> 1
            };
        }
        crc = (crc >> 8) ^ c;
    }
    crc
}

/// Full 16-byte signature of the compressed variant, as written by the vendor
/// tools and validated by the card. Files we generate reuse it verbatim.
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

/// Read a little-endian u32 without risking a panic on a short buffer.
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

    fn sample() -> Rcvbp {
        Rcvbp {
            version: 4,
            blob: Vec::new(),
            records: vec![
                Record::new(0x0a01, vec![0x80, 0x20, 1, 0]),
                Record::new(0x0a03, vec![7; 32]),
                Record::new(0x0aca, vec![0x80, 0, 0x20, 0]),
            ],
        }
    }

    #[test]
    fn records_round_trip_through_a_blob() {
        let f = sample();
        let blob = f.to_blob().unwrap();
        let parsed = parse_records(&blob, 0).unwrap();
        assert_eq!(parsed.len(), f.records.len());
        for (a, b) in parsed.iter().zip(&f.records) {
            assert_eq!(a.type_u16(), b.type_u16());
            assert_eq!(a.payload, b.payload);
        }
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
        // Inserted ahead of the trailing geometry record.
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

        let dir = std::env::temp_dir().join("e120-rcvbp-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("round-trip.rcvbp");
        let path = path.to_str().unwrap();
        std::fs::write(path, &bytes).unwrap();

        let back = Rcvbp::load(path).unwrap();
        assert_eq!(back.version, 4);
        assert_eq!(back.records.len(), f.records.len());
        for (a, b) in back.records.iter().zip(&f.records) {
            assert_eq!(a.type_u16(), b.type_u16());
            assert_eq!(a.payload, b.payload);
        }
        std::fs::remove_file(path).ok();
    }
}

#[cfg(test)]
mod crc_tests {
    use super::*;

    /// Trailer values recovered from real vendor files and from the card.
    #[test]
    fn trailer_matches_known_files() {
        for (path, expected) in [(
            "/Users/amd/Downloads/P2.5-32S-128X64-SM16269S-256X384I.rcvbp",
            0x128b_ebeeu32,
        )] {
            let Ok(d) = std::fs::read(path) else {
                continue; // not present in this checkout
            };
            let body = &d[..d.len() - 4];
            assert_eq!(trailer_crc(body), expected, "trailer mismatch for {path}");
            assert_eq!(&d[d.len() - 4..], &expected.to_le_bytes());
        }
    }

    #[test]
    fn a_written_file_carries_a_valid_trailer() {
        let f = Rcvbp {
            version: 4,
            blob: Vec::new(),
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
