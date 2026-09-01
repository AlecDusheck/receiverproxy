//! Driver-chip register library: record 0x84 generated from a chip's
//! vendor default table (`config/chips/*.toml`) instead of copied from a donor.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChipLibrary {
    pub name: String,
    pub family_id: u16,
    pub sub_id: Option<u16>,
    /// Register ids in record order.
    pub order: Vec<u8>,
    /// Register id (as `0x..` string) → R, G, B values.
    pub registers: BTreeMap<String, [u8; 3]>,
}

impl ChipLibrary {
    /// # Errors
    /// Fails on a missing or malformed file.
    pub fn load(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        toml::from_str(&text).with_context(|| format!("parse {path}"))
    }

    /// Record 0x84: `(register, R, G, B)` quads in library order, zero-filled
    /// to 256 bytes — the layout `GetChipCustomPlus` copies verbatim.
    ///
    /// # Errors
    /// Fails if the order names a register the table lacks, or overflows.
    pub fn record_84(&self) -> Result<[u8; 256]> {
        if self.order.len() * 4 > 256 {
            bail!("{}: {} registers do not fit a 256-byte record", self.name, self.order.len());
        }
        let mut out = [0u8; 256];
        for (i, reg) in self.order.iter().enumerate() {
            let rgb = self
                .registers
                .get(&format!("{reg:#04x}"))
                .with_context(|| format!("{}: no values for register {reg:#04x}", self.name))?;
            out[i * 4] = *reg;
            out[i * 4 + 1..i * 4 + 4].copy_from_slice(rgb);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quads_land_in_order_with_zero_fill() {
        let lib: ChipLibrary = toml::from_str(
            r#"
            name = "t"
            family_id = 1
            order = [0x02, 0xf0]
            [registers]
            0x02 = [1, 2, 3]
            0xf0 = [4, 5, 6]
            "#,
        )
        .unwrap();
        let r = lib.record_84().unwrap();
        assert_eq!(&r[..8], &[0x02, 1, 2, 3, 0xf0, 4, 5, 6]);
        assert!(r[8..].iter().all(|&b| b == 0));
    }
}
