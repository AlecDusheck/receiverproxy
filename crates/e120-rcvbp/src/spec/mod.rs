//! A panel described declaratively, and everything the card needs generated from it.
//!
//! Record 0x01, the pixel mapping and the basic pack. Field offsets come from
//! `docs/record-0x01-fields.md`; pack formulas from the vendor's `GetBasicParam`,
//! each pinned against the factory bytes under test.
//!
//! Bytes the spec does not yet derive come from the `[template]` section and
//! are listed in the provenance so nothing reaches flash unexplained.

mod basic_pack;
mod generate;
mod mapping;

pub use generate::{generate, Generated};

use crate::record01::{off, View};
use crate::Rcvbp;
use anyhow::{bail, Context, Result};
use serde::Deserialize;

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
    pub template: Template,
    #[serde(default)]
    pub boot: Boot,
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
    /// Grayscale depth in bits.
    #[serde(default = "default_gray")]
    pub gray_bits: u8,
    /// Serial (data) clock setting, the vendor's SetSerialClockFrequency unit.
    #[serde(default = "default_serial_clock")]
    pub serial_clock: u16,
    /// Data line direction: 0/1 vertical, 2/3 horizontal (vendor GetLineDir).
    #[serde(default)]
    pub line_dir: u8,
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
    /// Vendor chip id, e.g. 0x014C for the SM16269 family.
    pub id: u16,
    /// `.rcvbp` whose record 0x84 supplies the chip registers; default: the
    /// template config.
    pub registers_from: Option<String>,
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
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            gamma: 2.8,
            refresh_hz: 60.0,
            gclock: 0x14,
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
}

impl Default for Mapping {
    fn default() -> Self {
        Self {
            reversed_groups: true,
            reversed_lines: false,
        }
    }
}

/// Sources for the bytes the spec does not derive yet.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    /// Config supplying every record the spec does not derive.
    pub rcvbp: String,
    /// 256-byte basic-pack body supplying the bytes `GetBasicParam` derives
    /// from state not yet decoded.
    pub basic_pack: String,
    /// `.rcvbp` whose record 0x03 replaces the generated pixel mapping, for
    /// wirings the `[mapping]` knobs cannot express.
    pub mapping_from: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boot {
    /// Install the chip-register page so the card arms the drivers at
    /// power-on. Until the config boots dark this rails the supply.
    pub arm_at_boot: bool,
}

const fn default_gray() -> u8 {
    14
}
const fn default_serial_clock() -> u16 {
    8
}

impl PanelSpec {
    /// Read a spec from a TOML file.
    ///
    /// # Errors
    /// Fails on a missing or malformed file.
    pub fn load(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        toml::from_str(&text).with_context(|| format!("parse {path}"))
    }

    /// Generate the config and basic pack, loading the templates the spec
    /// names (paths relative to the working directory).
    ///
    /// # Errors
    /// Fails on an invalid spec or unusable template files.
    pub fn generate(&self) -> Result<Generated> {
        let template = Rcvbp::load(&self.template.rcvbp)?;
        let pack = std::fs::read(&self.template.basic_pack)
            .with_context(|| format!("read {}", self.template.basic_pack))?;
        let chip_regs = self
            .chip
            .registers_from
            .as_deref()
            .map(Rcvbp::load)
            .transpose()?;
        let mapping = self
            .template
            .mapping_from
            .as_deref()
            .map(Rcvbp::load)
            .transpose()?;
        generate(self, &template, &pack, chip_regs.as_ref(), mapping.as_ref())
    }

    /// Check the spec against what the generator can honour.
    ///
    /// # Errors
    /// Rejects geometry the record cannot express or the template cannot serve.
    pub fn validate(&self, template: &Rcvbp) -> Result<()> {
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
        // The carried basic-pack bytes include scan-sized row-order tables,
        // so the reference must have been computed for the same scan.
        let t = View::new(&template.record_01().context("template has no record 0x01")?.payload)?;
        if t.u8(off::SCAN) != self.module.scan {
            bail!(
                "template config is 1/{} scan; its reference pack cannot serve 1/{}",
                t.u8(off::SCAN),
                self.module.scan
            );
        }
        Ok(())
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
