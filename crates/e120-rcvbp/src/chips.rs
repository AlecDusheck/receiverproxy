//! Driver-chip library (`config/chips/*.toml`).
//!
//! The vendor's default register table for a chip, the chip-control block
//! the card's config carries for it, and the rules the vendor applies when
//! it builds record 0x84.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChipLibrary {
    pub name: String,
    pub family_id: u16,
    pub sub_id: Option<u16>,
    /// Vendor default serial clock for the chip (record +0x021).
    pub serial_clock: u16,
    /// The 20-byte `SChipControl` block the vendor's `ResetChipControl`
    /// emits for this chip (record 0x01 +0x0C4).
    pub chip_control: [u8; 20],
    /// Register ids in record order. Absent for chips without an addressed
    /// register table (the non-SH S-PWM parts, e.g. SM16169S).
    #[serde(default)]
    pub order: Vec<u8>,
    /// Register id (spelled `0x..` in the file) → R, G, B values.
    #[serde(default, deserialize_with = "hex_keys")]
    pub registers: BTreeMap<u8, [u8; 3]>,
    /// The 16-byte `SChipCustom` block (record 0x01 +0x06A) when the chip's
    /// configuration lives there instead of in record 0x84. When absent the
    /// generator writes only the PWM-flag/serial-clock pair the SH chips use.
    pub chip_custom: Option<[u8; 16]>,
    /// The scan patch the vendor applies to `chip_custom` on load:
    /// `byte = base | ((scan - 1) & mask)` for each listed byte.
    pub chip_custom_scan_patch: Option<ScanPatch>,
    /// `SChipCustomEX` (record 0x01 +0x0E0..+0x0E3).
    pub chip_custom_ex: Option<[u8; 4]>,
    /// The vendor omits record 0x84 for non-addressed chips; a zeroed record
    /// is not the same file.
    #[serde(default = "default_true")]
    pub emit_record_84: bool,
    /// Grey depth as a literal, for chips whose depth is not derived from
    /// registers 0x07/0x03.
    pub gray_bits: Option<u8>,
    /// Record 0x01 bytes this chip id sets differently from the 0x14C
    /// baseline; applied before the spec's own overrides.
    #[serde(default, deserialize_with = "record01_offsets")]
    pub record01_overrides: BTreeMap<usize, u8>,
}

/// `0x..` map keys parsed at load. Two spellings of one key (`0x02F`/`0x2F`)
/// are refused rather than letting the map pick one.
fn hex_keys<'de, D, K, V>(d: D) -> std::result::Result<BTreeMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: HexKey,
    V: serde::Deserialize<'de>,
{
    let raw: BTreeMap<String, V> = serde::Deserialize::deserialize(d)?;
    let mut out = BTreeMap::new();
    for (key, value) in raw {
        let k = K::parse_hex(&key).map_err(serde::de::Error::custom)?;
        if out.insert(k, value).is_some() {
            return Err(serde::de::Error::custom(format!("key {key} given twice (spelled differently)")));
        }
    }
    Ok(out)
}

/// `hex_keys` for record 0x01 offsets, which must lie inside the record.
pub(crate) fn record01_offsets<'de, D>(d: D) -> std::result::Result<BTreeMap<usize, u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let map: BTreeMap<usize, u8> = hex_keys(d)?;
    if let Some(at) = map.keys().find(|&&at| at >= crate::record01::LEN) {
        return Err(serde::de::Error::custom(format!(
            "record01_overrides offset {at:#05x} is past the record"
        )));
    }
    Ok(map)
}

pub(crate) trait HexKey: Ord {
    fn parse_hex(s: &str) -> Result<Self>
    where
        Self: Sized;
}

impl HexKey for u8 {
    fn parse_hex(s: &str) -> Result<Self> {
        Self::from_str_radix(s.trim_start_matches("0x"), 16).with_context(|| format!("bad register id {s:?}"))
    }
}

impl HexKey for usize {
    fn parse_hex(s: &str) -> Result<Self> {
        Self::from_str_radix(s.trim_start_matches("0x"), 16).with_context(|| format!("bad offset {s:?}"))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanPatch {
    pub bytes: Vec<usize>,
    pub mask: u8,
    pub base: u8,
}

const fn default_true() -> bool {
    true
}

impl ChipLibrary {
    /// # Errors
    /// Fails on a missing or malformed file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parse {}", path.display()))
    }

    /// Read a library from TOML text.
    ///
    /// # Errors
    /// Fails on malformed TOML, an unknown field or a bad hex key.
    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }

    fn reg(&self, id: u8) -> Result<[u8; 3]> {
        self.registers
            .get(&id)
            .copied()
            .with_context(|| format!("{}: no values for register {id:#04x}", self.name))
    }

    /// Record 0x84: `(register, R, G, B)` quads in library order, zero-filled
    /// to 256 bytes, with the vendor's post-load patch of register 0x02 to
    /// `scan - 1` (`ResetChipCustom`, chip 0x14C case).
    ///
    /// # Errors
    /// Fails if the order names a register the table lacks, or overflows.
    pub fn record_84(&self, scan: u8) -> Result<Option<[u8; 256]>> {
        if !self.emit_record_84 || self.order.is_empty() {
            return Ok(None);
        }
        if self.order.len() * 4 > 256 {
            bail!("{}: {} registers do not fit a 256-byte record", self.name, self.order.len());
        }
        let mut out = [0u8; 256];
        for (quad, &reg) in out.chunks_exact_mut(4).zip(&self.order) {
            let rgb = if reg == 0x02 { [scan.wrapping_sub(1) & 0x3F; 3] } else { self.reg(reg)? };
            quad[0] = reg;
            quad[1..].copy_from_slice(&rgb);
        }
        Ok(Some(out))
    }

    /// The `SChipCustom` block for a scan count, with the vendor's load-time
    /// patch applied; `None` for chips configured through record 0x84.
    #[must_use]
    pub fn chip_custom_block(&self, scan: u8) -> Option<[u8; 16]> {
        let mut block = self.chip_custom?;
        if let Some(p) = &self.chip_custom_scan_patch {
            for &i in &p.bytes {
                if let Some(b) = block.get_mut(i) {
                    *b = p.base | (scan.wrapping_sub(1) & p.mask);
                }
            }
        }
        Some(block)
    }

    /// Grayscale depth the vendor derives from the registers
    /// (`GetSupporttedGray`, chip 0x14C branch): line-gray from reg 0x07
    /// bits 4:3 times a multiplier from reg 0x03, bucketed into 12..16 bits.
    ///
    /// # Errors
    /// Fails if registers 0x03 or 0x07 are missing.
    pub fn gray_bits(&self) -> Result<u8> {
        if let Some(g) = self.gray_bits {
            return Ok(g);
        }
        let r07 = self.reg(0x07)?[0];
        let r03 = self.reg(0x03)?[0];
        let g = 128u32 << ((r07 >> 3) & 3);
        let m = if r03 < 0x40 { 64 } else { 32 };
        Ok(match m * g {
            x if x < 0x1000 => 12,
            x if x < 0x2000 => 13,
            x if x < 0x4000 => 14,
            x if x < 0x8000 => 15,
            _ => 16,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> ChipLibrary {
        toml::from_str(
            r#"
            name = "t"
            family_id = 1
            serial_clock = 15
            chip_control = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
            order = [0x02, 0x03, 0x07, 0xf0]
            [registers]
            0x02 = [0x3f, 0x3f, 0x3f]
            0x03 = [0x3f, 0x3f, 0x3f]
            0x07 = [0x04, 0x04, 0x04]
            0xf0 = [4, 5, 6]
            "#,
        )
        .unwrap()
    }

    #[test]
    fn quads_land_in_order_with_scan_patch_and_zero_fill() {
        let r = lib().record_84(16).unwrap().unwrap();
        assert_eq!(&r[..4], &[0x02, 15, 15, 15], "reg 0x02 = scan - 1");
        assert_eq!(&r[12..16], &[0xf0, 4, 5, 6]);
        assert!(r[16..].iter().all(|&b| b == 0));
    }

    #[test]
    fn hex_keys_are_typed_and_range_checked_at_load() {
        let lib: ChipLibrary = toml::from_str(
            r#"
            name = "t"
            family_id = 1
            serial_clock = 15
            chip_control = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
            [registers]
            0x2 = [1, 2, 3]
            [record01_overrides]
            "0x02F" = 1
            "0x2fb" = 2
            "#,
        )
        .unwrap();
        assert_eq!(lib.registers.get(&2), Some(&[1, 2, 3]));
        assert_eq!(lib.record01_overrides.iter().collect::<Vec<_>>(), vec![(&0x2F, &1), (&0x2FB, &2)]);

        let past: std::result::Result<ChipLibrary, _> = toml::from_str(
            "name = \"t\"\nfamily_id = 1\nserial_clock = 1\nchip_control = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]\n[record01_overrides]\n\"0x2FC\" = 1\n",
        );
        assert!(past.unwrap_err().to_string().contains("past the record"));
        let twice: std::result::Result<ChipLibrary, _> = toml::from_str(
            "name = \"t\"\nfamily_id = 1\nserial_clock = 1\nchip_control = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]\n[record01_overrides]\n\"0x02F\" = 1\n\"0x2F\" = 0\n",
        );
        assert!(twice.unwrap_err().to_string().contains("twice"));
    }

    #[test]
    fn gray_bits_follow_the_vendor_formula() {
        assert_eq!(lib().gray_bits().unwrap(), 14); // 128 x 64 = 0x2000 -> 14
    }
}
