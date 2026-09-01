//! Building the compiled parameter image — flash block 7, the thing the card
//! actually applies at boot.
//!
//! Format per `docs/compiled-image-format.md`: a fixed-offset scatter of pack
//! *bodies* with no framing, lengths, or checksums. This builder starts from a
//! known-good 64 KB base image and replaces only the regions whose derivation
//! is proven:
//!
//! * page 0x00 — the basic-parameter pack body (raster geometry, scan, gray)
//! * page 0x09 — the chip-register pack body (record 0x84 verbatim). The
//!   factory image never wrote this page, which is why the drivers do not arm
//!   at boot.
//! * pages 0x30–0x5F — the pixel-mapping table, from record 0x03 with each
//!   entry's u16 flipped LE→BE (rule proven byte-exact over all 4096 factory
//!   entries).
//! * 0x8000.. — the u32-LE length-prefixed `.rcvbp` source file.
//!
//! Everything else is carried from the base image, and `finish` reports which
//! pages changed so a flash write is reviewable before it happens.

use crate::Rcvbp;
use anyhow::{bail, Result};

pub const IMAGE_LEN: usize = 0x1_0000;
pub const BASIC_PACK_OFFSET: usize = 0x0000;
pub const CHIP_PAGE_OFFSET: usize = 0x0900;
pub const MAPPING_OFFSET: usize = 0x3000;
pub const MAPPING_LEN: usize = 0x3000;
pub const RCVBP_OFFSET: usize = 0x8000;
/// The vendor clamps the embedded .rcvbp at this many bytes.
pub const RCVBP_MAX: usize = 0x6FFC;

pub struct Block7Builder {
    img: Vec<u8>,
    base: Vec<u8>,
    notes: Vec<String>,
}

impl Block7Builder {
    /// Start from a 64 KB base image (normally the factory dump's block 7).
    ///
    /// # Errors
    /// Rejects a base that is not exactly one 64 KB block.
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

    /// Install the 256-byte basic-parameter pack body at page 0.
    ///
    /// # Errors
    /// Rejects a body that is not exactly one page.
    pub fn basic_pack(&mut self, body: &[u8]) -> Result<()> {
        if body.len() != 0x100 {
            bail!("basic pack body is {} bytes, need 256", body.len());
        }
        self.img[BASIC_PACK_OFFSET..BASIC_PACK_OFFSET + 0x100].copy_from_slice(body);
        self.notes.push("page 0x00: basic-parameter pack".into());
        Ok(())
    }

    /// Install the chip-register table (record 0x84 of `cfg`) at page 0x09.
    ///
    /// # Errors
    /// Fails if the config has no record 0x84 or it is not one page long.
    pub fn chip_registers_from(&mut self, cfg: &Rcvbp) -> Result<()> {
        let rec = cfg
            .records
            .iter()
            .find(|r| r.rtype[1] == 0x84)
            .ok_or_else(|| anyhow::anyhow!("config has no chip-register record (0x84)"))?;
        if rec.payload.len() != 0x100 {
            bail!("record 0x84 is {} bytes, need 256", rec.payload.len());
        }
        self.img[CHIP_PAGE_OFFSET..CHIP_PAGE_OFFSET + 0x100].copy_from_slice(&rec.payload);
        self.notes.push("page 0x09: chip registers".into());
        Ok(())
    }

    /// Install the pixel-mapping table (record 0x03 of `cfg`) at 0x3000.
    ///
    /// The record payload opens with a u16 entry count; each 3-byte entry's
    /// trailing u16 is stored little-endian in the file and big-endian in the
    /// compiled image.
    ///
    /// # Errors
    /// Fails if the record is missing, malformed, or too large for its region.
    pub fn mapping_from(&mut self, cfg: &Rcvbp) -> Result<()> {
        let rec = cfg
            .records
            .iter()
            .find(|r| r.rtype[1] == 0x03)
            .ok_or_else(|| anyhow::anyhow!("config has no mapping record (0x03)"))?;
        let body = &rec.payload[2..];
        if body.len() % 3 != 0 {
            bail!("mapping record body is {} bytes, not a multiple of 3", body.len());
        }
        if body.len() > MAPPING_LEN {
            bail!("mapping record ({} bytes) exceeds its region", body.len());
        }
        for (i, e) in body.chunks_exact(3).enumerate() {
            let at = MAPPING_OFFSET + i * 3;
            self.img[at] = e[0];
            self.img[at + 1] = e[2];
            self.img[at + 2] = e[1];
        }
        if body.len() < MAPPING_LEN {
            // The factory image fills its whole region, so a shorter table's
            // padding rule is unverified; zeros match the vendor's bzero'd
            // scratch buffer.
            self.img[MAPPING_OFFSET + body.len()..MAPPING_OFFSET + MAPPING_LEN].fill(0);
            self.notes.push(format!(
                "pages 0x30-0x5f: mapping table ({} entries, zero-padded — padding UNVERIFIED)",
                body.len() / 3
            ));
        } else {
            self.notes
                .push(format!("pages 0x30-0x5f: mapping table ({} entries)", body.len() / 3));
        }
        Ok(())
    }

    /// Embed the `.rcvbp` source file, length-prefixed, at 0x8000. The space
    /// after it is erased flash (0xFF), matching the vendor's write pattern.
    ///
    /// # Errors
    /// Rejects a file larger than the vendor's clamp.
    pub fn rcvbp(&mut self, file: &[u8]) -> Result<()> {
        if file.len() > RCVBP_MAX {
            bail!("rcvbp is {} bytes; the vendor clamps at {RCVBP_MAX}", file.len());
        }
        self.img[RCVBP_OFFSET..RCVBP_OFFSET + 4]
            .copy_from_slice(&(file.len() as u32).to_le_bytes());
        self.img[RCVBP_OFFSET + 4..RCVBP_OFFSET + 4 + file.len()].copy_from_slice(file);
        // Erased flash after the file — but stop short of page 0xF0: that page
        // is EEPROM-backed (the screen-size record), never reachable by page
        // writes, and the base image's copy of it should ride along untouched.
        self.img[RCVBP_OFFSET + 4 + file.len()..0xF000].fill(0xFF);
        self.notes
            .push(format!("+0x8000: embedded .rcvbp ({} bytes)", file.len()));
        Ok(())
    }

    /// The finished image, what was installed, and which 256-byte pages now
    /// differ from the base.
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

pub const DATA_SWAP_OFFSET: usize = 0x0500;
pub const MODULE_POS_OFFSET: usize = 0x0600;
pub const ANTI_VOID_OFFSET: usize = 0x1800;
pub const SCAN_TABLE_OFFSET: usize = 0x6000;
pub const SCAN_TABLE_LEN: usize = 0x0400;

/// The regions the vendor writes as zeros for this chip and config, each
/// because a gate in its builder fails (see `docs/compiled-image-format.md`):
/// void table (mode 0), current segment (chip id out of table range),
/// current exchange, void-line packs (empty table), anti-void packs 4-7
/// (only 4 packs without large-load support).
const ZERO_REGIONS: [(usize, usize); 6] = [
    (0x0100, 0x0500),
    (0x0A00, 0x0C00),
    (0x0C00, 0x0D00),
    (0x1000, 0x1800),
    (0x6800, 0x7000),
    (0x7000, 0x8000),
];

// Record 0x01 fields the region builders read (docs/record-0x01-fields.md).
const R01_LINE_DIR: usize = 0x03C;
const R01_SPLIT: usize = 0x03E;
const R01_GRID_W_LO: usize = 0x057;
const R01_GRID_H_LO: usize = 0x058;
const R01_GRID_W_HI: usize = 0x24E;
const R01_GRID_H_HI: usize = 0x24F;
const R01_MAX_W: usize = 0x0C0;
const R01_MAX_H: usize = 0x0C2;
const R01_SWAP_RAMP: usize = 0x19A;

/// The data-swap pack body (`GetDataSwapEx2ParamPack`).
///
/// Shared by the boot image and the real-time pack: the 64-byte lane map from
/// record +0x19A, zeros, and the three deseam-correction pairs, which are 8.8
/// fixed point 1.0 (`01 00`) with deseam off.
///
/// # Errors
/// Fails if the record is too short.
pub fn data_swap_body(rec01: &[u8]) -> Result<[u8; 256]> {
    if rec01.len() < R01_SWAP_RAMP + 64 {
        bail!("record 0x01 too short for the swap ramp");
    }
    let mut body = [0u8; 256];
    body[..64].copy_from_slice(&rec01[R01_SWAP_RAMP..R01_SWAP_RAMP + 64]);
    for pair in [0xEA, 0xF0, 0xF6] {
        body[pair] = 0x01;
    }
    Ok(body)
}

/// Split-segment count from record +0x03E (vendor `GetSplitSegment`).
const fn split_segment(c: u8) -> u8 {
    if c & 4 != 0 {
        4
    } else if c & 1 == 0 {
        1
    } else if c < 8 {
        2
    } else {
        c >> 3
    }
}

impl Block7Builder {
    /// Start from erased flash — all 0xFF, as the block looks after the
    /// vendor's erase — so that every byte present is one we placed.
    #[must_use]
    pub fn erased() -> Self {
        let base = vec![0xFF; IMAGE_LEN];
        Self {
            img: base.clone(),
            base,
            notes: Vec::new(),
        }
    }

    /// Write the gated-off regions as zeros (what the vendor's bzero'd pack
    /// buffers leave behind).
    pub fn zero_regions(&mut self) {
        for (lo, hi) in ZERO_REGIONS {
            self.img[lo..hi].fill(0);
        }
        self.notes
            .push("0x100/0xA00/0xC00/0x1000/0x6800/0x7000: zeros (builders gated off)".into());
    }

    /// The data-swap pack body at 0x500: the 64-byte lane map from record
    /// +0x19A, and the three deseam-correction pairs, which are 8.8 fixed
    /// point 1.0 (`01 00`) when deseam is off.
    ///
    /// # Errors
    /// Fails if the record is too short.
    pub fn data_swap_from(&mut self, rec01: &[u8]) -> Result<()> {
        let body = data_swap_body(rec01)?;
        self.img[DATA_SWAP_OFFSET..DATA_SWAP_OFFSET + 0x100].copy_from_slice(&body);
        self.notes
            .push("0x500: data-swap (lane map from record +0x19A, deseam 1.0 x3)".into());
        Ok(())
    }

    /// The module-position table at 0x600 (vendor `GetDefaultModulePos`):
    /// the screen tiled by the record's grid unit, one 10-byte entry per
    /// tile, row-major. Left all-zero when the vendor's own gates fail —
    /// notably more than 64 tiles, which is why the seller's 256x384 wall
    /// config carries zeros here.
    ///
    /// # Errors
    /// Fails if the record is too short or uses a split layout the builder
    /// does not implement.
    pub fn module_positions_from(&mut self, rec01: &[u8]) -> Result<()> {
        if rec01.len() < 0x250 {
            bail!("record 0x01 too short for module positions");
        }
        let at = MODULE_POS_OFFSET;
        self.img[at..at + 0x300].fill(0);
        let mw = u16::from_le_bytes([rec01[R01_GRID_W_LO], rec01[R01_GRID_W_HI]]);
        let mh = u16::from_le_bytes([rec01[R01_GRID_H_LO], rec01[R01_GRID_H_HI]]);
        let w = u16::from_le_bytes([rec01[R01_MAX_W], rec01[R01_MAX_W + 1]]);
        let h = u16::from_le_bytes([rec01[R01_MAX_H], rec01[R01_MAX_H + 1]]);
        let dir = rec01[R01_LINE_DIR];
        let gated = mw == 0
            || mh == 0
            || !w.is_multiple_of(mw)
            || !h.is_multiple_of(mh)
            || (w / mw) * (h / mh) > 64
            || dir > 3;
        if gated {
            self.notes.push(format!(
                "0x600: module positions all-zero (vendor gate: {w}x{h} screen / {mw}x{mh} grid)"
            ));
            return Ok(());
        }
        let k = split_segment(rec01[R01_SPLIT]);
        if k != 1 {
            bail!("module positions for split-segment layout {k} are not implemented");
        }
        let (nx, ny) = (w / mw, h / mh);
        self.img[at + 5] = (nx * ny) as u8;
        for row in 0..ny {
            for col in 0..nx {
                let e = at + 0x16 + usize::from(row * nx + col) * 10;
                // Index bytes: outer loop, inner loop. Under line_dir 0 the
                // vendor walks rows outer / columns inner (right-to-left);
                // which of the two indices is which is medium confidence.
                self.img[e] = row as u8;
                self.img[e + 1] = col as u8;
                self.img[e + 2..e + 4].copy_from_slice(&(mw * col).to_be_bytes());
                self.img[e + 4..e + 6].copy_from_slice(&(mh * row).to_be_bytes());
                self.img[e + 6..e + 8].copy_from_slice(&mw.to_be_bytes());
                self.img[e + 8..e + 10].copy_from_slice(&mh.to_be_bytes());
            }
        }
        self.notes.push(format!(
            "0x600: module positions ({nx}x{ny} tiles of {mw}x{mh}, line_dir {dir})"
        ));
        Ok(())
    }

    /// The anti-void-line packs at 0x1800 (vendor `GetAntiVoidLineParam`):
    /// with no void lines configured, two identical blocks of big-endian
    /// counters `0x2000 + n`, sliced into four packs.
    pub fn anti_void_lines(&mut self) {
        for block in 0..2 {
            for n in 0..0x400u16 {
                let at = ANTI_VOID_OFFSET + block * 0x800 + usize::from(n) * 2;
                self.img[at..at + 2].copy_from_slice(&(0x2000 + n).to_be_bytes());
            }
        }
        self.notes
            .push("0x1800: anti-void-line counters (no void lines configured)".into());
    }

    /// The scan table at 0x6000. Its renderer is decoded but the bit-time
    /// solver behind it is not, so the bytes are carried from a compiled
    /// image of the same chip and clock — and the solver's input depends on
    /// the load width, so a different screen width may need a different
    /// table.
    ///
    /// # Errors
    /// Rejects a table that is not exactly one pack body.
    pub fn scan_table(&mut self, table: &[u8]) -> Result<()> {
        if table.len() != SCAN_TABLE_LEN {
            bail!("scan table is {} bytes, need 0x400", table.len());
        }
        self.img[SCAN_TABLE_OFFSET..SCAN_TABLE_OFFSET + SCAN_TABLE_LEN].copy_from_slice(table);
        self.notes.push(
            "0x6000: scan table (carried verbatim — solver untranscribed, width-dependent)".into(),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factory_dump() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../firmware/card-dumps/primary-region.bin"
        ))
        .expect("factory dump present in repo")
    }

    #[test]
    fn factory_parts_rebuild_the_factory_image() {
        // The factory block embeds the .rcvbp it was compiled from; feeding
        // that .rcvbp's mapping record back through the builder must
        // reproduce the factory mapping region byte for byte, and re-embedding
        // the file must reproduce the tail. This pins the LE->BE entry rule.
        let dump = factory_dump();
        let base = &dump[0x7_0000..0x8_0000];
        let n = u32::from_le_bytes(base[RCVBP_OFFSET..RCVBP_OFFSET + 4].try_into().unwrap()) as usize;
        let file = &base[RCVBP_OFFSET + 4..RCVBP_OFFSET + 4 + n];
        let cfg = Rcvbp::from_bytes(file).unwrap();

        let mut b = Block7Builder::from_base(base).unwrap();
        b.mapping_from(&cfg).unwrap();
        b.rcvbp(file).unwrap();
        let (img, _, changed) = b.finish();
        assert_eq!(img, base, "round-trip must be byte-exact");
        assert!(changed.is_empty());
    }

    #[test]
    fn the_factory_image_rebuilds_from_erased_flash_and_its_own_parts() {
        // No base image: every region is generated (or, for the scan table,
        // carried explicitly) and the result must equal the factory block
        // byte for byte, except page 0xF0, which is EEPROM-backed and not
        // part of the image the vendor writes.
        let dump = factory_dump();
        let base = &dump[0x7_0000..0x8_0000];
        let n = u32::from_le_bytes(base[RCVBP_OFFSET..RCVBP_OFFSET + 4].try_into().unwrap()) as usize;
        let file = &base[RCVBP_OFFSET + 4..RCVBP_OFFSET + 4 + n];
        let cfg = Rcvbp::from_bytes(file).unwrap();
        let rec01 = &cfg.record_01().unwrap().payload;

        let mut b = Block7Builder::erased();
        b.zero_regions();
        b.basic_pack(&base[..0x100]).unwrap();
        b.data_swap_from(rec01).unwrap();
        b.module_positions_from(rec01).unwrap();
        b.anti_void_lines();
        b.mapping_from(&cfg).unwrap();
        b.scan_table(&base[SCAN_TABLE_OFFSET..SCAN_TABLE_OFFSET + SCAN_TABLE_LEN]).unwrap();
        b.rcvbp(file).unwrap();
        let (img, _, _) = b.finish();

        let bad: Vec<u8> = (0..=255u8)
            .filter(|&p| p != 0xF0)
            .filter(|&p| {
                let at = usize::from(p) * 0x100;
                img[at..at + 0x100] != base[at..at + 0x100]
            })
            .collect();
        assert!(bad.is_empty(), "pages differing from factory: {bad:02x?}");
    }

    #[test]
    fn a_single_module_screen_gets_module_positions() {
        let dump = factory_dump();
        let base = &dump[0x7_0000..0x8_0000];
        let n = u32::from_le_bytes(base[RCVBP_OFFSET..RCVBP_OFFSET + 4].try_into().unwrap()) as usize;
        let cfg = Rcvbp::from_bytes(&base[RCVBP_OFFSET + 4..RCVBP_OFFSET + 4 + n]).unwrap();
        let mut rec01 = cfg.record_01().unwrap().payload.clone();
        rec01[R01_MAX_W..R01_MAX_W + 2].copy_from_slice(&128u16.to_le_bytes());
        rec01[R01_MAX_H..R01_MAX_H + 2].copy_from_slice(&64u16.to_le_bytes());

        let mut b = Block7Builder::erased();
        b.module_positions_from(&rec01).unwrap();
        let (img, _, _) = b.finish();
        assert_eq!(img[MODULE_POS_OFFSET + 5], 32, "8x4 tiles of 16x16");
        // Last tile: row 3, col 7 -> x 112, y 48, 16x16.
        let e = MODULE_POS_OFFSET + 0x16 + 31 * 10;
        assert_eq!(&img[e..e + 10], &[3, 7, 0, 112, 0, 48, 0, 16, 0, 16]);
        assert!(img[MODULE_POS_OFFSET + 0x16 + 32 * 10..MODULE_POS_OFFSET + 0x300]
            .iter()
            .all(|&x| x == 0));
    }

    #[test]
    fn chip_page_lands_where_the_boot_loader_reads_it() {
        let dump = factory_dump();
        let base = &dump[0x7_0000..0x8_0000];
        let n = u32::from_le_bytes(base[RCVBP_OFFSET..RCVBP_OFFSET + 4].try_into().unwrap()) as usize;
        let cfg = Rcvbp::from_bytes(&base[RCVBP_OFFSET + 4..RCVBP_OFFSET + 4 + n]).unwrap();

        let mut b = Block7Builder::from_base(base).unwrap();
        b.chip_registers_from(&cfg).unwrap();
        let (img, _, changed) = b.finish();
        assert_eq!(changed, vec![0x09], "only the chip page may change");
        let rec = cfg.records.iter().find(|r| r.rtype[1] == 0x84).unwrap();
        assert_eq!(&img[CHIP_PAGE_OFFSET..CHIP_PAGE_OFFSET + 0x100], &rec.payload[..]);
    }
}
