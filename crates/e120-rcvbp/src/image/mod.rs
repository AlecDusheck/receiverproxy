//! The compiled parameter image — flash block 7, what the card applies at
//! boot. Format: `docs/compiled-image-format.md`.
//!
//! The vendor writes it as a fixed-offset scatter of pack bodies with no
//! framing or checksums; the builder assembles it from erased flash, one
//! region per module below, and reports every page it wrote.

pub mod anti_void;
pub mod data_swap;
pub mod module_pos;
pub mod scan_table;

use crate::record01::View;
use crate::Rcvbp;
use anyhow::{bail, Result};

pub const IMAGE_LEN: usize = 0x1_0000;
pub const BASIC_PACK_OFFSET: usize = 0x0000;
pub const DATA_SWAP_OFFSET: usize = 0x0500;
pub const MODULE_POS_OFFSET: usize = 0x0600;
pub const CHIP_PAGE_OFFSET: usize = 0x0900;
pub const ANTI_VOID_OFFSET: usize = 0x1800;
pub const MAPPING_OFFSET: usize = 0x3000;
pub const MAPPING_LEN: usize = 0x3000;
pub const SCAN_TABLE_OFFSET: usize = 0x6000;
pub const RCVBP_OFFSET: usize = 0x8000;
/// The vendor clamps the embedded .rcvbp at this many bytes.
pub const RCVBP_MAX: usize = 0x6FFC;

/// Regions the vendor writes as zeros for this chip and config, each because
/// a gate in its builder fails: void table (mode 0), current segment (chip id
/// outside the table), current exchange, void-line packs (empty table),
/// anti-void packs 4-7 (no large-load support).
const ZERO_REGIONS: [(usize, usize); 6] = [
    (0x0100, 0x0500),
    (0x0A00, 0x0C00),
    (0x0C00, 0x0D00),
    (0x1000, 0x1800),
    (0x6800, 0x7000),
    (0x7000, 0x8000),
];

pub struct Block7Builder {
    img: Vec<u8>,
    base: Vec<u8>,
    notes: Vec<String>,
}

impl Block7Builder {
    /// Start from erased flash — all 0xFF, as the block looks after the
    /// vendor's erase — so every byte present is one we placed.
    #[must_use]
    pub fn erased() -> Self {
        let base = vec![0xFF; IMAGE_LEN];
        Self {
            img: base.clone(),
            base,
            notes: Vec::new(),
        }
    }

    /// Start from an existing 64 KB block (for patching a dump).
    ///
    /// # Errors
    /// Rejects a base that is not exactly one block.
    pub fn from_base(base: &[u8]) -> Result<Self> {
        if base.len() != IMAGE_LEN {
            bail!("base image is {} bytes, need 0x10000", base.len());
        }
        Ok(Self {
            img: base.to_vec(),
            base: base.to_vec(),
            notes: Vec::new(),
        })
    }

    fn place(&mut self, at: usize, bytes: &[u8], note: impl Into<String>) {
        self.img[at..at + bytes.len()].copy_from_slice(bytes);
        self.notes.push(note.into());
    }

    /// The gated-off regions, as the vendor's zeroed buffers leave them.
    pub fn zero_regions(&mut self) {
        for (lo, hi) in ZERO_REGIONS {
            self.img[lo..hi].fill(0);
        }
        self.notes
            .push("0x100/0xA00/0xC00/0x1000/0x6800/0x7000: zeros (builders gated off)".into());
    }

    /// Page 0: the basic-parameter pack body.
    ///
    /// # Errors
    /// Rejects a body that is not exactly one page.
    pub fn basic_pack(&mut self, body: &[u8]) -> Result<()> {
        if body.len() != 0x100 {
            bail!("basic pack body is {} bytes, need 256", body.len());
        }
        self.place(BASIC_PACK_OFFSET, body, "page 0x00: basic-parameter pack");
        Ok(())
    }

    /// Page 0x09: the chip-register table, record 0x84 verbatim. The
    /// factory image never wrote this page, which is why the drivers do not
    /// arm at boot.
    ///
    /// A config without record 0x84 (a non-addressed chip, configured
    /// through the basic pack's chip-custom block) leaves the page erased.
    ///
    /// # Errors
    /// Fails if record 0x84 is present but not one page.
    pub fn chip_registers_from(&mut self, cfg: &Rcvbp) -> Result<()> {
        let Ok(rec) = record(cfg, 0x84) else {
            return Ok(());
        };
        if rec.len() != 0x100 {
            bail!("record 0x84 is {} bytes, need 256", rec.len());
        }
        self.place(CHIP_PAGE_OFFSET, rec, "page 0x09: chip registers");
        Ok(())
    }

    /// 0x500: the data-swap pack body.
    ///
    /// # Errors
    /// Fails on a short record.
    pub fn data_swap_from(&mut self, rec01: &[u8]) -> Result<()> {
        let body = data_swap::body(&View::new(rec01)?);
        self.place(DATA_SWAP_OFFSET, &body, "0x500: data-swap (lane map + deseam 1.0 x3)");
        Ok(())
    }

    /// 0x600: the module-position table.
    ///
    /// # Errors
    /// Fails on a short record or an unimplemented split layout.
    pub fn module_positions_from(&mut self, rec01: &[u8]) -> Result<()> {
        let (region, note) = module_pos::region(&View::new(rec01)?)?;
        self.place(MODULE_POS_OFFSET, &region, note);
        Ok(())
    }

    /// 0x1800: the anti-void-line counters.
    pub fn anti_void_lines(&mut self) {
        let region = anti_void::region();
        self.place(ANTI_VOID_OFFSET, &region, "0x1800: anti-void-line counters");
    }

    /// 0x3000: the pixel mapping — record 0x03 with each entry's u16 flipped
    /// LE→BE (the vendor's 16 pixel-sequence packs are this table, sliced).
    ///
    /// # Errors
    /// Fails if the record is missing, malformed, or too large.
    pub fn mapping_from(&mut self, cfg: &Rcvbp) -> Result<()> {
        let rec = record(cfg, 0x03)?;
        let body = &rec[2..];
        if !body.len().is_multiple_of(3) {
            bail!("mapping record body is {} bytes, not a multiple of 3", body.len());
        }
        if body.len() > MAPPING_LEN {
            bail!("mapping record ({} bytes) exceeds its region", body.len());
        }
        let mut out = vec![0u8; MAPPING_LEN];
        for (i, e) in body.chunks_exact(3).enumerate() {
            out[i * 3..i * 3 + 3].copy_from_slice(&[e[0], e[2], e[1]]);
        }
        let note = if body.len() < MAPPING_LEN {
            format!(
                "pages 0x30-0x5f: mapping ({} entries, zero-padded — padding UNVERIFIED)",
                body.len() / 3
            )
        } else {
            format!("pages 0x30-0x5f: mapping ({} entries)", body.len() / 3)
        };
        self.place(MAPPING_OFFSET, &out, note);
        Ok(())
    }

    /// 0x6000: the scan table from the vendor's bit-time solver.
    ///
    /// # Errors
    /// Fails for solver inputs outside the transcribed cases.
    pub fn scan_table_from(&mut self, rec01: &[u8], card_scan_len: u16) -> Result<()> {
        let table = scan_table::body(&View::new(rec01)?, card_scan_len)?;
        self.place(SCAN_TABLE_OFFSET, &table, "0x6000: scan table (bit-time solver)");
        Ok(())
    }

    /// 0x8000: the length-prefixed `.rcvbp` source, erased flash after it
    /// up to the EEPROM-backed page 0xF0.
    ///
    /// # Errors
    /// Rejects a file over the vendor's clamp.
    pub fn rcvbp(&mut self, file: &[u8]) -> Result<()> {
        if file.len() > RCVBP_MAX {
            bail!("rcvbp is {} bytes; the vendor clamps at {RCVBP_MAX}", file.len());
        }
        let at = RCVBP_OFFSET;
        self.img[at..at + 4].copy_from_slice(&(file.len() as u32).to_le_bytes());
        self.img[at + 4..at + 4 + file.len()].copy_from_slice(file);
        self.img[at + 4 + file.len()..0xF000].fill(0xFF);
        self.notes.push(format!("+0x8000: embedded .rcvbp ({} bytes)", file.len()));
        Ok(())
    }

    /// The image, what was placed, and which pages differ from the base.
    #[must_use]
    pub fn finish(self) -> (Vec<u8>, Vec<String>, Vec<u8>) {
        let changed = (0..=255u8)
            .filter(|&p| {
                let at = usize::from(p) * 0x100;
                self.img[at..at + 0x100] != self.base[at..at + 0x100]
            })
            .collect();
        (self.img, self.notes, changed)
    }
}

fn record(cfg: &Rcvbp, id: u8) -> Result<&[u8]> {
    cfg.records
        .iter()
        .find(|r| r.rtype[1] == id)
        .map(|r| r.payload.as_slice())
        .ok_or_else(|| anyhow::anyhow!("config has no record 0x{id:02x}"))
}

/// The data-swap body for the real-time pack (same bytes as the image).
///
/// # Errors
/// Fails on a short record.
pub fn data_swap_body(rec01: &[u8]) -> Result<[u8; 256]> {
    Ok(data_swap::body(&View::new(rec01)?))
}
