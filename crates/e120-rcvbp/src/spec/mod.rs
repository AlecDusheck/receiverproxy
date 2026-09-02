//! A panel spec (`config/panels/*.toml`) and the config generated from it.
//!
//! Record 0x01 (`docs/record-0x01-fields.md`), the pixel mapping and the basic
//! pack (vendor `GetBasicParam`). Nothing is copied from a donor file: every
//! byte is a vendor default, a spec field, a chip-library value or a listed literal.

mod basic_pack;
mod generate;
mod mapping;
mod record01;
mod records;

pub use generate::{generate, Generated};

use crate::chips::ChipLibrary;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelSpec {
    /// Name used for output files.
    pub name: String,
    pub module: Module,
    pub screen: Screen,
    pub chip: Chip,
    #[serde(default)]
    pub color: Color,
    #[serde(default)]
    pub current: Current,
    #[serde(default)]
    pub timing: Timing,
    #[serde(default)]
    pub mapping: Mapping,
    #[serde(default)]
    pub boot: Boot,
    /// Raw record 0x01 byte overrides (`"0x043" = 0x20`), applied last. The
    /// bench spec's `+0x02F = 1` lives here; nothing displays without it.
    #[serde(default, deserialize_with = "crate::chips::record01_offsets")]
    pub record01_overrides: BTreeMap<usize, u8>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    /// Pixels across one module.
    pub width: u16,
    /// Pixels down one module. The record stores half of this.
    pub height: u16,
    /// Scan denominator, e.g. 16 for 1/16.
    pub scan: u8,
    /// Serial (data) clock setting, the vendor's SetSerialClockFrequency unit;
    /// the chip library's default when absent.
    pub serial_clock: Option<u16>,
    /// Grayscale depth override; derived from the chip registers when absent.
    pub gray_bits: Option<u8>,
    /// Data line direction: 0/1 vertical, 2/3 horizontal (vendor GetLineDir).
    #[serde(default)]
    pub line_dir: u8,
    /// Data-group / output code (record +0x044 low nibble).
    #[serde(default = "default_data_groups")]
    pub data_groups: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Screen {
    /// Whole screen this card drives, in pixels (MaxWidth/MaxHeight).
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chip {
    /// Chip library (`config/chips/*.toml`): ids, register defaults, chip control.
    pub library: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Color {
    /// Colour-swap index (record +0x02B).
    pub swap: u8,
    /// R/G/B source indices (record +0x02C..0x02E); (2,1,0) = no exchange.
    pub source: [u8; 3],
}

impl Default for Color {
    fn default() -> Self {
        Self {
            swap: 3,
            source: [2, 1, 0],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Current {
    /// Red, green, blue, virtual-red current gain, 0-63.
    pub gains: [u8; 4],
    /// Per-channel current percent (record +0x0B4/B8/BC, f32).
    pub percent: [f32; 3],
}

impl Default for Current {
    fn default() -> Self {
        Self {
            gains: [43; 4],
            percent: [0.1; 3],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    pub gamma: f32,
    pub refresh_hz: f32,
    /// GCLK setting (record +0x031); vendor default 0x14.
    pub gclock: u8,
    /// Minimum OE time (record +0x0AE); the PWM bit-time solver's floor.
    pub min_oe: f32,
    /// Luminance level (record +0x026), split across the colour percents.
    pub luminance_level: u16,
    /// 8 ns OE enable (record +0x050 bit 0).
    pub oe_8ns: bool,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            gamma: 2.8,
            refresh_hz: 60.0,
            gclock: 0x14,
            min_oe: 1e-4,
            luminance_level: 188,
            oe_8ns: true,
        }
    }
}

/// How the module's pixels are wired into the card's scan-line buffer
/// (record 0x03). The vendor corpus shows two knobs beyond geometry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    /// Data groups (`stored height / scan` of them) in reverse order in the
    /// buffer — the vendor default (234 of 241 two-group configs).
    pub reversed_groups: bool,
    /// Scan lines addressed bottom-up (`scan-1-row`) instead of top-down.
    pub reversed_lines: bool,
    /// Columns per run of the shift chain before it switches data group:
    /// `[lower 0..b][upper 0..b][lower b..2b]...`. Default (module width) gives
    /// each group one contiguous half; the bench panel's own file uses 64.
    #[serde(default)]
    pub block: Option<u16>,
    /// Displace line positions `width..2*width` off the chain via the void-line
    /// column table; otherwise the card drives them with a fixed pattern that
    /// shows as a floor at black (docs/rendering.md). Off reproduces the factory image.
    #[serde(default = "default_true")]
    pub gate_phantom_positions: bool,
}

const fn default_true() -> bool {
    true
}

impl Default for Mapping {
    fn default() -> Self {
        Self {
            reversed_groups: true,
            reversed_lines: false,
            block: None,
            gate_phantom_positions: true,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boot {
    /// Install the chip-register page so the card arms the drivers at
    /// power-on. Until the config boots dark this rails the supply.
    pub arm_at_boot: bool,
}

const fn default_data_groups() -> u8 {
    1
}

impl PanelSpec {
    /// Read a spec from a TOML file.
    ///
    /// # Errors
    /// Fails on a missing or malformed file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    /// Generate the config and basic pack; the chip library path is relative
    /// to the working directory.
    ///
    /// # Errors
    /// Fails on an invalid spec or unusable chip library.
    pub fn generate(&self) -> Result<Generated> {
        let chip = ChipLibrary::load(&self.chip.library)?;
        generate(self, &chip)
    }

    /// Check the spec against what the record can express.
    ///
    /// # Errors
    /// Rejects geometry the record cannot hold.
    pub fn validate(&self) -> Result<()> {
        if !self.module.height.is_multiple_of(2) {
            bail!("module height must be even (the record stores height/2)");
        }
        if self.module.width > 255 || self.module.height / 2 > 255 {
            bail!("module dimensions exceed the record's byte fields");
        }
        if !self.screen.width.is_multiple_of(self.module.width)
            || !self.screen.height.is_multiple_of(self.module.height)
        {
            bail!("screen size must be a whole number of modules");
        }
        if self.module.scan == 0 || u16::from(self.module.scan) > self.module.height {
            bail!("scan denominator must be 1..=module height");
        }
        if !(self.module.height / 2).is_multiple_of(u16::from(self.module.scan)) {
            bail!("stored module height (height/2) must be a whole number of scan groups");
        }
        Ok(())
    }

    /// The serial clock (record +0x021): the spec's, else the chip's default.
    #[must_use]
    pub fn serial_clock(&self, chip: &ChipLibrary) -> u16 {
        self.module.serial_clock.unwrap_or(chip.serial_clock)
    }

    /// Grayscale depth: the spec's override, else the vendor's derivation
    /// from the chip registers.
    ///
    /// # Errors
    /// Fails if the library lacks the registers the derivation reads.
    pub fn gray_bits(&self, chip: &ChipLibrary) -> Result<u8> {
        match self.module.gray_bits {
            Some(g) => Ok(g),
            None => chip.gray_bits(),
        }
    }

    /// The screen extent along the data-line direction (vendor
    /// `GetMaxInLineDir`, before void adjustments).
    #[must_use]
    pub fn screen_extent_in_line_dir(&self) -> u16 {
        if self.module.line_dir >= 2 {
            self.screen.height
        } else {
            self.screen.width
        }
    }

    /// Vendor `GetModuleInputCount`: the 16-pixel grid unit over the module
    /// dimension along the line direction, at least 1.
    #[must_use]
    pub fn module_input_count(&self) -> u8 {
        let unit = 16u16;
        let dim = if self.module.line_dir >= 2 { self.module.width } else { self.module.height / 2 };
        (unit / dim.max(1)).max(1) as u8
    }

    /// Modules chained along the data-line direction (vendor
    /// `GetModuleCountInLineDir`): screen extent / module extent on that axis.
    #[must_use]
    pub fn modules_in_line_dir(&self) -> u16 {
        if self.module.line_dir >= 2 {
            self.screen.height.div_ceil(self.module.height)
        } else {
            self.screen.width.div_ceil(self.module.width)
        }
    }

    /// Clocks in one scan line (vendor `GetOneScanLen`): W x stored H / scan.
    #[must_use]
    pub fn one_scan_len(&self) -> u16 {
        let v = u32::from(self.module.width) * u32::from(self.module.height / 2)
            / u32::from(self.module.scan);
        v.max(1) as u16
    }

    /// Clocks in one card scan line (vendor `GetCardScanLen`): OneScanLen
    /// scaled by the modules along the line direction.
    #[must_use]
    pub fn card_scan_len(&self) -> u16 {
        self.one_scan_len() * self.modules_in_line_dir()
    }

    /// Record 0x03, the pixel mapping, from geometry and the `[mapping]` knobs.
    #[must_use]
    pub fn mapping_record(&self) -> Vec<u8> {
        mapping::record(self)
    }
}
