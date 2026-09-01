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

use anyhow::{bail, Context, Result};
use std::io::Read;

#[derive(Clone)]
pub struct Record {
    /// Offset of the record within the decompressed blob.
    pub offset: usize,
    /// Record type: the two bytes stored after the size field.
    pub rtype: [u8; 2],
    pub payload: Vec<u8>,
}

impl Record {
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
        if d.len() < 32 {
            bail!("{path}: too short to be a .rcvbp");
        }
        let version = u32::from_le_bytes(d[0x10..0x14].try_into().unwrap());

        // Two variants exist in the wild, distinguished by their 16-byte
        // signature: the newer one zlib-compresses the record stream, the
        // older one stores it inline right after the version field and ends
        // with a 4-byte trailer.
        let (blob, compressed) = if d[0..4] == SIG_COMPRESSED {
            let raw_len = u32::from_le_bytes(d[0x18..0x1c].try_into().unwrap()) as usize;
            let mut blob = Vec::with_capacity(raw_len);
            flate2::read::ZlibDecoder::new(&d[0x20..])
                .read_to_end(&mut blob)
                .context("inflate rcvbp payload")?;
            if blob.len() != raw_len {
                bail!(
                    "{path}: inflated {} bytes but header says {raw_len}",
                    blob.len()
                );
            }
            (blob, true)
        } else {
            (d[0x14..].to_vec(), false)
        };

        // The uncompressed variant carries a 4-byte trailer after the records.
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
        let r = self.find(0x0a01)?;
        if r.payload.len() < 2 {
            return None;
        }
        Some((r.payload[0], r.payload[1]))
    }
}

/// Signature of the newer, zlib-compressed variant.
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
