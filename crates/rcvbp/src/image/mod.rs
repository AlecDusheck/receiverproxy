//! The compiled parameter image: flash block 7, applied by the card at boot.
//!
//! A fixed-offset scatter of pack bodies with no framing or checksums
//! (`docs/compiled-image-format.md`); the builder starts from erased flash
//! and reports every page it wrote.

pub mod anti_void;
pub mod data_swap;
pub mod module_pos;
pub mod scan_table;

use crate::record01::View;
use crate::spec::Generated;
use crate::Rcvbp;
use anyhow::{bail, Context, Result};
use panelspec::PanelSpec;
pub use receivers::BootImage;

pub const IMAGE_LEN: usize = 0x1_0000;

// The E120's region offsets (`config/cards/e120.toml`), pinned by
// `tests/factory.rs`; the builder itself reads them from a `BootImage`.
pub const BASIC_PACK_OFFSET: usize = 0x0000;
pub const DATA_SWAP_OFFSET: usize = 0x0500;
pub const MODULE_POS_OFFSET: usize = 0x0600;
pub const CHIP_PAGE_OFFSET: usize = 0x0900;
/// The void-line packs (zeroed for this chip; `send_params` slices them).
pub const VOID_LINE_OFFSET: usize = 0x1000;
pub const VOID_LINE_COLUMNS_OFFSET: usize = 0x1400;
pub const ANTI_VOID_OFFSET: usize = 0x1800;
pub const MAPPING_OFFSET: usize = 0x3000;
pub const SCAN_TABLE_OFFSET: usize = 0x6000;
pub const RCVBP_OFFSET: usize = 0x8000;

/// Regions the vendor zeroes for this chip because a builder gate fails: void
/// table (mode 0), current segment (chip id outside the table), current
/// exchange, void-line packs (empty table), anti-void packs 4-7 (no large-load).
const ZERO_REGIONS: [(usize, usize); 6] = [
    (0x0100, 0x0500),
    (0x0A00, 0x0C00),
    (0x0C00, 0x0D00),
    (0x1000, 0x1800),
    (0x6800, 0x7000),
    (0x7000, 0x8000),
];

pub struct Block7Builder {
    map: BootImage,
    img: Vec<u8>,
    notes: Vec<String>,
}

impl Block7Builder {
    /// Erased flash (all 0xFF), so every other byte is one the builder placed;
    /// `map` says where each region goes.
    #[must_use]
    pub fn erased(map: &BootImage) -> Self {
        Self {
            map: map.clone(),
            img: vec![0xFF; IMAGE_LEN],
            notes: Vec::new(),
        }
    }

    /// The raster-state regions. `void_line_columns` must follow `zero_regions`,
    /// which clears 0x1000..0x1800 (docs/rendering.md). The chip page and the
    /// embedded `.rcvbp` are left to the caller: RAM pushes must not send them.
    ///
    /// # Errors
    /// Fails if the generated config lacks a record or a region builder
    /// refuses the spec.
    pub fn from_generated(map: &BootImage, spec: &PanelSpec, g: &Generated) -> Result<Self> {
        let rec01 = &g.rcvbp.record_01().context("generated config has no record 0x01")?.payload;
        let mut b = Self::erased(map);
        b.zero_regions();
        b.basic_pack(&g.basic_pack)?;
        b.data_swap_from(rec01)?;
        b.module_positions_from(rec01)?;
        b.anti_void_lines();
        if spec.mapping.gate_phantom_positions {
            b.void_line_columns(spec.module.width, spec.module.width * 2);
        }
        b.mapping_from(&g.rcvbp)?;
        b.scan_table_from(rec01, spec.card_scan_len())?;
        Ok(b)
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

    /// 0x1400: the void-line column table, one byte per line position
    /// (`physical = a + table[a]`, `GetVoidLineInfoPacks` @ 0x1e58c0). 0xFF
    /// pushes `from..to` past the end of the chain; for this wiring
    /// `width..2*width` carried a fixed pattern instead (docs/rendering.md).
    pub fn void_line_columns(&mut self, from: u16, to: u16) {
        let at = self.map.void_line_columns;
        self.img[at + usize::from(from)..at + usize::from(to)].fill(0xFF);
        self.notes.push(format!(
            "0x1400: void-line column table, positions {from}..{to} displaced off the chain"
        ));
    }

    /// Page 0: the basic-parameter pack body.
    ///
    /// # Errors
    /// Rejects a body that is not exactly one page.
    pub fn basic_pack(&mut self, body: &[u8]) -> Result<()> {
        if body.len() != 0x100 {
            bail!("basic pack body is {} bytes, need 256", body.len());
        }
        self.place(self.map.basic_pack, body, "page 0x00: basic-parameter pack");
        Ok(())
    }

    /// Page 0x09: record 0x84 verbatim; the card arms the drivers at boot only
    /// when this page is written. No record 0x84 leaves the page erased.
    ///
    /// # Errors
    /// Fails if record 0x84 is present but not one page.
    pub fn chip_registers_from(&mut self, cfg: &Rcvbp) -> Result<()> {
        let Some(rec) = cfg.find_by_id(0x84) else {
            return Ok(());
        };
        if rec.payload.len() != 0x100 {
            bail!("record 0x84 is {} bytes, need 256", rec.payload.len());
        }
        self.place(self.map.chip_page, &rec.payload, "page 0x09: chip registers");
        Ok(())
    }

    /// 0x500: the data-swap pack body.
    ///
    /// # Errors
    /// Fails on a short record.
    pub fn data_swap_from(&mut self, rec01: &[u8]) -> Result<()> {
        let body = data_swap::body(View::new(rec01)?);
        self.place(self.map.data_swap, &body, "0x500: data-swap (lane map + deseam 1.0 x3)");
        Ok(())
    }

    /// 0x600: the module-position table.
    ///
    /// # Errors
    /// Fails on a short record or an unimplemented split layout.
    pub fn module_positions_from(&mut self, rec01: &[u8]) -> Result<()> {
        let (region, note) = module_pos::region(View::new(rec01)?)?;
        self.place(self.map.module_positions, &region, note);
        Ok(())
    }

    /// 0x1800: the anti-void-line counters.
    pub fn anti_void_lines(&mut self) {
        let region = anti_void::region();
        self.place(self.map.anti_void, &region, "0x1800: anti-void-line counters");
    }

    /// 0x3000: record 0x03 with each entry's u16 flipped LE to BE; the
    /// vendor's 16 pixel-sequence packs are this table, sliced.
    ///
    /// # Errors
    /// Fails if the record is missing, malformed, or too large.
    pub fn mapping_from(&mut self, cfg: &Rcvbp) -> Result<()> {
        let rec = record(cfg, 0x03)?;
        let body = &rec[2..];
        if !body.len().is_multiple_of(3) {
            bail!("mapping record body is {} bytes, not a multiple of 3", body.len());
        }
        let len = self.map.mapping_len();
        if body.len() > len {
            bail!("mapping record ({} entries) exceeds the card's {} entries", body.len() / 3, self.map.map_entries);
        }
        let dst = &mut self.img[self.map.mapping..self.map.mapping + len];
        dst.fill(0);
        let (dst3, _) = dst.as_chunks_mut::<3>();
        let (src3, _) = body.as_chunks::<3>();
        for (d, e) in dst3.iter_mut().zip(src3) {
            d.copy_from_slice(&[e[0], e[2], e[1]]);
        }
        let note = if body.len() < len {
            format!(
                "pages 0x30-0x5f: mapping ({} entries, zero-padded — padding UNVERIFIED)",
                body.len() / 3
            )
        } else {
            format!("pages 0x30-0x5f: mapping ({} entries)", body.len() / 3)
        };
        self.notes.push(note);
        Ok(())
    }

    /// 0x6000: the scan table from the vendor's bit-time solver.
    ///
    /// # Errors
    /// Fails for solver inputs outside the transcribed cases.
    pub fn scan_table_from(&mut self, rec01: &[u8], card_scan_len: u16) -> Result<()> {
        let table = scan_table::body(View::new(rec01)?, card_scan_len)?;
        self.place(self.map.scan_table, &table, "0x6000: scan table (bit-time solver)");
        Ok(())
    }

    /// 0x8000: the length-prefixed `.rcvbp`, erased flash after it.
    ///
    /// # Errors
    /// Rejects a file over the vendor's clamp.
    pub fn rcvbp(&mut self, file: &[u8]) -> Result<()> {
        let max = self.map.rcvbp_max;
        if file.len() > max {
            bail!("rcvbp is {} bytes; the vendor clamps at {max}", file.len());
        }
        let at = self.map.rcvbp;
        self.img[at..at + 4].copy_from_slice(&(file.len() as u32).to_le_bytes());
        self.img[at + 4..at + 4 + file.len()].copy_from_slice(file);
        self.img[at + 4 + file.len()..].fill(0xFF);
        self.notes.push(format!("+0x8000: embedded .rcvbp ({} bytes)", file.len()));
        Ok(())
    }

    /// The image, what was placed, and which pages are no longer erased.
    #[must_use]
    pub fn finish(self) -> Block7 {
        let changed_pages = self
            .img
            .as_chunks::<0x100>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, page)| page.iter().any(|&b| b != 0xFF))
            .map(|(i, _)| i as u8)
            .collect();
        Block7 {
            image: self.img,
            notes: self.notes,
            changed_pages,
        }
    }
}

/// The whole image for a generated config: the raster regions, the chip page
/// when the spec arms at boot, and the embedded `.rcvbp`. What `rxp config
/// gen` writes as `<name>-block7.bin`.
///
/// # Errors
/// Fails where the region builders do, or if the `.rcvbp` cannot be encoded.
pub fn compile(map: &BootImage, spec: &PanelSpec, g: &Generated) -> Result<Block7> {
    let mut b = Block7Builder::from_generated(map, spec, g)?;
    if spec.boot.arm_at_boot {
        b.chip_registers_from(&g.rcvbp)?;
    }
    b.rcvbp(&g.rcvbp.to_file_bytes()?)?;
    Ok(b.finish())
}

/// A finished block-7 image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block7 {
    pub image: Vec<u8>,
    /// One line per region placed.
    pub notes: Vec<String>,
    /// Pages (256 B) that are no longer erased flash.
    pub changed_pages: Vec<u8>,
}

fn record(cfg: &Rcvbp, id: u8) -> Result<&[u8]> {
    cfg.find_by_id(id)
        .map(|r| r.payload.as_slice())
        .with_context(|| format!("config has no record 0x{id:02x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_e120_model_carries_the_pinned_offsets() {
        let m = &receivers::by_name("E120").unwrap().memory.boot_image;
        assert_eq!(
            [m.basic_pack, m.data_swap, m.module_positions, m.chip_page, m.void_line, m.void_line_columns, m.anti_void, m.mapping, m.scan_table, m.rcvbp],
            [BASIC_PACK_OFFSET, DATA_SWAP_OFFSET, MODULE_POS_OFFSET, CHIP_PAGE_OFFSET, VOID_LINE_OFFSET, VOID_LINE_COLUMNS_OFFSET, ANTI_VOID_OFFSET, MAPPING_OFFSET, SCAN_TABLE_OFFSET, RCVBP_OFFSET]
        );
        assert_eq!((m.map_entries, m.rcvbp_max), (4096, 0x6FFC));
    }
}
