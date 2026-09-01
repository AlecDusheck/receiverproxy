//! Generating a receiver-card configuration from a declarative panel spec.
//!
//! The spec names the things a panel actually differs by — module geometry,
//! scan, driver chip, clocks, gains, colour wiring, screen size — and the
//! generator places them into the two artifacts the card consumes:
//!
//! * record 0x01 of the `.rcvbp` (field offsets from `docs/record-0x01-fields.md`,
//!   decoded from the vendor's loader/serializer);
//! * the 256-byte basic-parameter pack body at page 0 of the compiled boot
//!   image (offsets from the vendor's `GetBasicParam` builder, each formula
//!   cross-checked against the factory pack).
//!
//! Bytes the spec does not derive are carried from a named template, and
//! every placed byte is reported in the provenance list so nothing lands in
//! flash unexplained. The pixel-mapping and chip-register records are taken
//! from templates too: their *containers* are fully understood, their
//! contents are known-good vendor data for this geometry and chip, and a
//! first-principles generator for them is future work.

use crate::{Rcvbp, Record};
use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// A panel, as the config generator needs to know it.
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
    /// Whole screen this card drives, in pixels (MaxWidth).
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    /// Config supplying every record the spec does not derive.
    pub rcvbp: String,
    /// 256-byte basic-pack body supplying the bytes GetBasicParam derives
    /// from state we have not decoded.
    pub basic_pack: String,
    /// 64 KB base image for the compiled regions we do not yet compute,
    /// as PATH or PATH:HEXOFFSET.
    pub base_block: String,
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

/// The generator's outputs.
pub struct Generated {
    pub rcvbp: Rcvbp,
    pub basic_pack: [u8; 256],
    /// One line per byte range placed, with its source.
    pub provenance: Vec<String>,
}

// Record 0x01 payload offsets (docs/record-0x01-fields.md).
const R_MODULE_W: usize = 0x000;
const R_MODULE_H_HALF: usize = 0x001;
const R_GAMMA: usize = 0x01C;
const R_SCAN: usize = 0x020;
const R_SERIAL_CLOCK: usize = 0x021;
const R_GRAY: usize = 0x023;
const R_COLOR_SWAP: usize = 0x02B;
const R_COLOR_SOURCE: usize = 0x02C;
const R_GCLOCK: usize = 0x031;
const R_GAINS: usize = 0x032;
const R_CHIP_LO: usize = 0x036;
const R_LINE_DIR: usize = 0x03C;
const R_SERIAL_CLOCK_HALF: usize = 0x049;
const R_SERIAL_CLOCK_DUP: usize = 0x04B;
const R_REFRESH: usize = 0x0AA;
const R_CURRENT_PCT: usize = 0x0B4;
const R_MAX_W: usize = 0x0C0;
const R_MAX_H: usize = 0x0C2;
const R_CHIP_HI: usize = 0x204;
const R_FIELD3: usize = 0x028;
const R_LUM: usize = 0x024;
const R_FIELD26: usize = 0x026;
const R_CHIP_CUSTOM: usize = 0x06A;

// Basic-pack body offsets (pack offset - 4), from GetBasicParam.
const P_FIELD3: usize = 0x01;
const P_MODULE_DIMS: usize = 0x04;
const P_MODULES_IN_LINE: usize = 0x06;
const P_SCAN: usize = 0x07;
const P_GRAY: usize = 0x08;
const P_SERIAL_CLOCK: usize = 0x09;
const P_ONE_SCAN_LEN: usize = 0x0B;
const P_CARD_SCAN_LEN: usize = 0x0D;
const P_COLOR: usize = 0x10;
const P_LUM: usize = 0x15;
const P_FIELD26: usize = 0x17;
const P_GAINS: usize = 0x30;
const P_CHIP_CUSTOM: usize = 0x70;
const P_MAX_W: usize = 0x88;
const P_MAX_H: usize = 0x8A;
const P_CHIP_ID: usize = 0xE7;

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

    /// Check the spec against what the generator can actually honour.
    ///
    /// # Errors
    /// Rejects geometry the templates cannot express.
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
        let t = template.record_01().context("template has no record 0x01")?;
        if t.payload[R_SCAN] != self.module.scan {
            bail!(
                "template config is 1/{} scan; its reference pack cannot serve 1/{}",
                t.payload[R_SCAN],
                self.module.scan
            );
        }
        Ok(())
    }

    /// Record 0x03, the pixel mapping: for every module pixel in raster order
    /// (over the stored height), the scan line it belongs to and its slot in
    /// that line's buffer. Derived from the vendor's count formula
    /// (`SaveBpToBuffer` @ 0x1cc404: count = width x stored height) and
    /// corpus-validated byte-exact against the consensus tables.
    #[must_use]
    pub fn mapping_record(&self) -> Vec<u8> {
        let w = self.module.width;
        let h = self.module.height / 2;
        let scan = u16::from(self.module.scan);
        let groups = h / scan;
        let n = w * h;
        let mut out = Vec::with_capacity(2 + usize::from(n) * 3);
        out.extend_from_slice(&n.to_le_bytes());
        for i in 0..n {
            let (row, col) = (i / w, i % w);
            let line = if self.mapping.reversed_lines {
                scan - 1 - row % scan
            } else {
                row % scan
            };
            let group = if self.mapping.reversed_groups {
                groups - 1 - row / scan
            } else {
                row / scan
            };
            let slot = group * w + col;
            out.push(line as u8);
            out.extend_from_slice(&slot.to_le_bytes());
        }
        out
    }

    /// Modules chained along the data-line direction (vendor
    /// GetModuleCountInLineDir): screen extent / module extent along that axis.
    #[must_use]
    pub fn modules_in_line_dir(&self) -> u16 {
        if self.module.line_dir >= 2 {
            self.screen.height.div_ceil(self.module.height)
        } else {
            self.screen.width.div_ceil(self.module.width)
        }
    }

    /// Clocks in one scan line (vendor GetOneScanLen): W x stored H / scan.
    #[must_use]
    pub fn one_scan_len(&self) -> u16 {
        let v = u32::from(self.module.width) * u32::from(self.module.height / 2)
            / u32::from(self.module.scan);
        v.max(1) as u16
    }

    /// Clocks in one card scan line (vendor GetCardScanLen): OneScanLen
    /// scaled by the modules along the line direction.
    #[must_use]
    pub fn card_scan_len(&self) -> u16 {
        self.one_scan_len() * self.modules_in_line_dir()
    }
}

/// Write the spec into record 0x01, returning the provenance of each edit.
fn apply_to_record01(spec: &PanelSpec, p: &mut [u8], prov: &mut Vec<String>) -> Result<()> {
    if p.len() < 0x2FC {
        bail!("record 0x01 payload is {} bytes, need 764", p.len());
    }
    let mut put = |off: usize, bytes: &[u8], what: &str| {
        p[off..off + bytes.len()].copy_from_slice(bytes);
        prov.push(format!("record01 +{off:#05x} <- {what}"));
    };
    put(R_MODULE_W, &[spec.module.width as u8], "module.width");
    put(R_MODULE_H_HALF, &[(spec.module.height / 2) as u8], "module.height / 2");
    put(R_GAMMA, &spec.timing.gamma.to_le_bytes(), "timing.gamma (f32)");
    put(R_SCAN, &[spec.module.scan], "module.scan");
    put(R_SERIAL_CLOCK, &spec.module.serial_clock.to_le_bytes(), "module.serial_clock");
    put(
        R_SERIAL_CLOCK_HALF,
        &(spec.module.serial_clock / 2).to_le_bytes(),
        "module.serial_clock / 2 (vendor-derived duplicate)",
    );
    put(
        R_SERIAL_CLOCK_DUP,
        &spec.module.serial_clock.to_le_bytes(),
        "module.serial_clock (vendor-derived duplicate)",
    );
    put(R_GRAY, &[spec.module.gray_bits], "module.gray_bits");
    put(R_COLOR_SWAP, &[spec.color.swap], "color.swap");
    put(R_COLOR_SOURCE, &spec.color.source, "color.source");
    put(R_GCLOCK, &[spec.timing.gclock], "timing.gclock");
    put(R_GAINS, &spec.current.gains, "current.gains");
    put(R_CHIP_LO, &[(spec.chip.id & 0xFF) as u8], "chip.id low byte");
    put(R_CHIP_HI, &[(spec.chip.id >> 8) as u8], "chip.id high byte");
    put(R_LINE_DIR, &[spec.module.line_dir], "module.line_dir");
    put(R_REFRESH, &spec.timing.refresh_hz.to_le_bytes(), "timing.refresh_hz (f32)");
    for (i, pct) in spec.current.percent.iter().enumerate() {
        put(R_CURRENT_PCT + 4 * i, &pct.to_le_bytes(), "current.percent (f32)");
    }
    put(R_MAX_W, &spec.screen.width.to_le_bytes(), "screen.width (MaxWidth)");
    put(R_MAX_H, &spec.screen.height.to_le_bytes(), "screen.height (MaxHeight)");
    Ok(())
}

/// Build the basic-pack body from the spec and the finished record 0x01,
/// carrying undecoded bytes from `template`.
fn basic_pack_body(
    spec: &PanelSpec,
    rec01: &[u8],
    template: &[u8],
    prov: &mut Vec<String>,
) -> Result<[u8; 256]> {
    if template.len() != 256 {
        bail!("template basic pack is {} bytes, need 256", template.len());
    }
    let mut b = [0u8; 256];
    b.copy_from_slice(template);
    prov.push("basicpack: all bytes not listed below <- template".into());
    let mut put = |off: usize, bytes: &[u8], what: &str| {
        b[off..off + bytes.len()].copy_from_slice(bytes);
        prov.push(format!("basicpack +{off:#04x} <- {what}"));
    };
    put(P_FIELD3, &rec01[R_FIELD3..R_FIELD3 + 3], "record01 +0x028 (3 bytes, verbatim)");
    let (w, h2) = (spec.module.width as u8, (spec.module.height / 2) as u8);
    if spec.module.line_dir >= 2 {
        put(P_MODULE_DIMS, &[w, h2], "module width, height/2 (line_dir horizontal)");
    } else {
        put(P_MODULE_DIMS, &[h2, w], "module height/2, width (line_dir vertical)");
    }
    put(
        P_MODULES_IN_LINE,
        &[spec.modules_in_line_dir() as u8],
        "modules in line dir = screen extent / module extent",
    );
    put(P_SCAN, &[spec.module.scan], "module.scan");
    put(P_GRAY, &[spec.module.gray_bits], "module.gray_bits");
    put(P_SERIAL_CLOCK, &spec.module.serial_clock.to_be_bytes(), "module.serial_clock (BE)");
    put(
        P_ONE_SCAN_LEN,
        &spec.one_scan_len().to_be_bytes(),
        "OneScanLen = W x H/2 / scan (BE)",
    );
    put(
        P_CARD_SCAN_LEN,
        &spec.card_scan_len().to_be_bytes(),
        "CardScanLen = OneScanLen x modules in line dir (BE)",
    );
    let [s0, s1, s2] = spec.color.source;
    put(
        P_COLOR,
        &[(spec.color.swap << 6) | (s2 << 4) | (s1 << 2) | s0],
        "color.swap<<6 | source[2]<<4 | source[1]<<2 | source[0]",
    );
    put(P_LUM, &[rec01[R_LUM]], "record01 +0x024 low byte");
    put(P_FIELD26, &[rec01[R_FIELD26]], "record01 +0x026 low byte");
    put(P_GAINS, &spec.current.gains, "current.gains");
    put(
        P_CHIP_CUSTOM,
        &rec01[R_CHIP_CUSTOM..R_CHIP_CUSTOM + 16],
        "record01 +0x06A chip-custom block (verbatim)",
    );
    put(P_MAX_W, &spec.screen.width.to_be_bytes(), "screen.width (BE)");
    put(P_MAX_H, &spec.screen.height.to_be_bytes(), "screen.height (BE)");
    put(P_CHIP_ID, &spec.chip.id.to_be_bytes(), "chip.id (BE)");
    Ok(b)
}

/// Generate the `.rcvbp` and basic pack for a spec.
///
/// # Errors
/// Fails on an invalid spec or unusable templates.
pub fn generate(
    spec: &PanelSpec,
    template: &Rcvbp,
    template_pack: &[u8],
    chip_regs: Option<&Rcvbp>,
    mapping: Option<&Rcvbp>,
) -> Result<Generated> {
    spec.validate(template)?;
    let mut prov = Vec::new();
    let mut out = Rcvbp {
        version: template.version,
        blob: Vec::new(),
        records: template.records.clone(),
    };
    prov.push(format!(
        "rcvbp: {} records <- template (record 0x01 then edited)",
        out.records.len()
    ));

    let r01 = out
        .records
        .iter_mut()
        .find(|r| r.rtype[1] == 0x01)
        .context("template has no record 0x01")?;
    apply_to_record01(spec, &mut r01.payload, &mut prov)?;
    let rec01 = r01.payload.clone();

    if let Some(src) = chip_regs {
        replace_record(&mut out, src, 0x84, &mut prov, "chip.registers_from")?;
    }
    if let Some(src) = mapping {
        replace_record(&mut out, src, 0x03, &mut prov, "template.mapping_from")?;
    } else {
        let generated = spec.mapping_record();
        let slot = out
            .records
            .iter_mut()
            .find(|r| r.rtype[1] == 0x03)
            .context("template has no record 0x03 to replace")?;
        slot.payload = generated;
        prov.push(format!(
            "rcvbp record 0x03 <- generated ({}x{} stored, 1/{}, groups {}, lines {})",
            spec.module.width,
            spec.module.height / 2,
            spec.module.scan,
            if spec.mapping.reversed_groups { "reversed" } else { "forward" },
            if spec.mapping.reversed_lines { "reversed" } else { "top-down" },
        ));
    }

    let basic_pack = basic_pack_body(spec, &rec01, template_pack, &mut prov)?;
    Ok(Generated {
        rcvbp: out,
        basic_pack,
        provenance: prov,
    })
}

fn replace_record(
    out: &mut Rcvbp,
    src: &Rcvbp,
    id: u8,
    prov: &mut Vec<String>,
    what: &str,
) -> Result<()> {
    let donor = src
        .records
        .iter()
        .find(|r| r.rtype[1] == id)
        .with_context(|| format!("{what} has no record 0x{id:02x}"))?;
    let slot = out
        .records
        .iter_mut()
        .find(|r| r.rtype[1] == id)
        .with_context(|| format!("template has no record 0x{id:02x} to replace"))?;
    *slot = Record {
        offset: slot.offset,
        rtype: slot.rtype,
        payload: donor.payload.clone(),
    };
    prov.push(format!("rcvbp record 0x{id:02x} <- {what}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(path: &str) -> String {
        format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
    }

    fn our_panel() -> PanelSpec {
        toml::from_str(&std::fs::read_to_string(repo("panels/p25-128x64-sm16269s.toml")).unwrap())
            .unwrap()
    }

    #[test]
    fn our_panel_reproduces_the_hand_built_pack_from_formulas() {
        // basic-pack-single-module-v2.bin was built by hand from the factory
        // pack with four justified byte edits; the generator must arrive at
        // the identical 256 bytes from the spec alone.
        let spec = our_panel();
        let template = Rcvbp::load(&repo(&spec.template.rcvbp)).unwrap();
        let pack = std::fs::read(repo(&spec.template.basic_pack)).unwrap();
        let g = generate(&spec, &template, &pack, None, None).unwrap();
        let expected = std::fs::read(repo("firmware/derived/basic-pack-single-module-v2.bin")).unwrap();
        let diffs: Vec<usize> = (0..256).filter(|&i| g.basic_pack[i] != expected[i]).collect();
        assert!(diffs.is_empty(), "generated pack differs from v2 at {diffs:x?}");
    }

    #[test]
    fn our_panel_changes_only_the_screen_size_in_the_template_record() {
        // The template record already describes this module; the spec's
        // screen size (one module, not the seller's 256x384 wall) is the
        // only thing that should move.
        let spec = our_panel();
        let template = Rcvbp::load(&repo(&spec.template.rcvbp)).unwrap();
        let pack = std::fs::read(repo(&spec.template.basic_pack)).unwrap();
        let g = generate(&spec, &template, &pack, None, None).unwrap();
        let before = &template.record_01().unwrap().payload;
        let after = &g.rcvbp.record_01().unwrap().payload;
        let diffs: Vec<usize> = (0..before.len()).filter(|&i| before[i] != after[i]).collect();
        assert_eq!(diffs, vec![0x0C0, 0x0C1, 0x0C2, 0x0C3], "unexpected record edits");
        assert_eq!(&after[0x0C0..0x0C4], &[0x80, 0x00, 0x40, 0x00]);
        // And the file round-trips through the serializer.
        let bytes = g.rcvbp.to_file_bytes().unwrap();
        let back = Rcvbp::from_bytes(&bytes).unwrap();
        assert_eq!(back.records.len(), g.rcvbp.records.len());
    }

    #[test]
    fn a_two_module_screen_reproduces_the_factory_pack() {
        // The seller's config was compiled for a 2-wide line of modules;
        // asking the generator for that screen must land on the factory
        // bytes for every derived field.
        let mut spec = our_panel();
        spec.screen.width = 256;
        spec.screen.height = 384;
        let template = Rcvbp::load(&repo(&spec.template.rcvbp)).unwrap();
        let pack = std::fs::read(repo(&spec.template.basic_pack)).unwrap();
        let g = generate(&spec, &template, &pack, None, None).unwrap();
        assert_eq!(g.basic_pack[..], pack[..], "factory pack not reproduced");
    }

    #[test]
    fn a_scan_the_reference_pack_was_not_computed_for_is_refused() {
        let mut spec = our_panel();
        spec.module.scan = 32;
        let template = Rcvbp::load(&repo(&spec.template.rcvbp)).unwrap();
        assert!(spec.validate(&template).is_err());
    }

    #[test]
    fn the_generated_mapping_is_the_vendor_consensus_table() {
        // 34 known-good vendor configs for a 128x64 module at 1/16 share one
        // byte-identical record 0x03; the geometry formula must land on it.
        let spec = our_panel();
        let donor =
            Rcvbp::load(&repo("firmware/derived/donor-P2.5-320x160-2153-consensus.rcvbp")).unwrap();
        let consensus = &donor.records.iter().find(|r| r.rtype[1] == 0x03).unwrap().payload;
        assert_eq!(spec.mapping_record(), *consensus);
    }

    #[test]
    fn the_sellers_outlier_is_not_what_the_knobs_produce() {
        // The seller's table interleaves the two column halves across data
        // groups; neither knob describes that wiring, and the generator must
        // not silently reproduce it.
        let spec = our_panel();
        let seller = Rcvbp::load(&repo("firmware/P2.5-32S-128X64-SM16269S-256X384I.rcvbp")).unwrap();
        let outlier = &seller.records.iter().find(|r| r.rtype[1] == 0x03).unwrap().payload;
        assert_ne!(spec.mapping_record(), *outlier);
    }
}
