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
            .copy_from_slice(&u32::try_from(file.len()).expect("clamped above").to_le_bytes());
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
