//! Receiving-card models as data. Each `config/cards/<name>.toml` describes
//! one card: how discovery identifies it, what it can drive, where its flash
//! holds firmware and parameters, and how far it has been tested. The files
//! are embedded at build time; nothing here reads the filesystem.

pub mod firmware;

use serde::Deserialize;
use std::fmt;
use std::ops::Range;
use std::sync::OnceLock;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/cards.rs"));
}

/// One receiving card.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardModel {
    /// The name `--card` takes, matched without regard to case.
    pub name: String,
    pub vendor: String,
    /// The protocol family: `colorlight` is the only one implemented.
    pub family: String,
    /// The card-type byte in the discovery reply.
    pub id: u8,
    pub status: Status,
    #[serde(default)]
    pub notes: String,
    /// A photo for the web app, relative to `web/static` (`cards/e120.jpg`).
    #[serde(default)]
    pub image: Option<String>,
    /// Where the photo came from.
    #[serde(default)]
    pub image_source: Option<String>,
    /// Panels driven on a bench with this card, one entry per measurement.
    #[serde(default)]
    pub tested: Vec<Tested>,
    pub limits: Limits,
    pub memory: Memory,
    pub firmware: Firmware,
}

/// How far the model has been exercised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Driven on a bench.
    Tested,
    /// Configurations generate; never driven.
    Generates,
    Unsupported,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tested => "tested",
            Self::Generates => "generates",
            Self::Unsupported => "unsupported",
        })
    }
}

/// One panel driven on a bench with the card.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tested {
    /// The spec it was driven with, relative to the repository root.
    pub panel: String,
    /// The firmware the card ran: an image name from `config/firmware.toml`.
    pub firmware: String,
}

impl Tested {
    /// The manifest entry `firmware` names.
    #[must_use]
    pub fn image(&self) -> Option<&'static firmware::Image> {
        firmware::image(&self.firmware)
    }

    /// The version the card reported, from the manifest.
    #[must_use]
    pub fn version(&self) -> Option<Version> {
        self.image().map(|i| i.version)
    }
}

/// What the card can drive, as its specification states it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_width: u16,
    pub max_height: u16,
    pub hub_ports: u8,
    /// Cards on one chain; absent when the specification does not say.
    pub chain: Option<u16>,
}

/// The flash map: banks by address, the parameter block by index.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub block_bytes: u32,
    pub primary_bank: u32,
    pub bank_bytes: u32,
    pub golden_bank: u32,
    pub parameter_block: u8,
    pub eeprom_mirror: u32,
    #[serde(default)]
    pub guarded: Vec<Guard>,
    pub boot_image: BootImage,
}

impl Memory {
    /// Blocks of the primary bank.
    #[must_use]
    pub fn primary_blocks(&self) -> Range<u8> {
        let first = (self.primary_bank / self.block_bytes) as u8;
        first..first + self.bank_blocks()
    }

    /// Blocks in one bank.
    #[must_use]
    pub fn bank_blocks(&self) -> u8 {
        self.bank_bytes.div_ceil(self.block_bytes) as u8
    }

    /// First block of the golden bank.
    #[must_use]
    pub fn golden_block(&self) -> u8 {
        (self.golden_bank / self.block_bytes) as u8
    }

    /// Blocks `version` write-protects from the host page-write path; empty
    /// when no range lists it.
    #[must_use]
    pub fn guarded_blocks(&self, version: Version) -> &[u8] {
        self.guarded
            .iter()
            .find(|g| g.covers(version))
            .map_or(&[], |g| g.blocks.as_slice())
    }

    /// The 256-byte page index of the embedded `.rcvbp`: the parameter
    /// block's first page plus the region offset.
    #[must_use]
    pub fn config_page(&self) -> u16 {
        (u16::from(self.parameter_block) << 8) | (self.boot_image.rcvbp / 0x100) as u16
    }
}

/// Blocks a firmware version range guards from the host path.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    pub from: Version,
    /// Inclusive; open-ended when absent.
    pub to: Option<Version>,
    pub blocks: Vec<u8>,
}

impl Guard {
    fn covers(&self, v: Version) -> bool {
        self.from <= v && self.to.is_none_or(|to| v <= to)
    }
}

/// A firmware version as the card reports it, `major.minor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u8, pub u8);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}

impl std::str::FromStr for Version {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let (a, b) = s
            .split_once('.')
            .ok_or_else(|| format!("version {s:?} is not major.minor"))?;
        let n = |v: &str| v.parse::<u8>().map_err(|e| format!("version {s:?}: {e}"));
        Ok(Self(n(a)?, n(b)?))
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Region offsets inside the parameter block, and the two limits the
/// vendor applies there.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootImage {
    pub basic_pack: usize,
    pub data_swap: usize,
    pub module_positions: usize,
    pub chip_page: usize,
    pub void_line: usize,
    pub void_line_columns: usize,
    pub anti_void: usize,
    pub mapping: usize,
    pub scan_table: usize,
    pub rcvbp: usize,
    /// Pixel-map entries the mapping region holds.
    pub map_entries: usize,
    /// Largest embedded `.rcvbp` the card accepts.
    pub rcvbp_max: usize,
}

impl BootImage {
    /// Bytes of the mapping region: three per entry.
    #[must_use]
    pub const fn mapping_len(&self) -> usize {
        self.map_entries * 3
    }
}

/// How firmware images are named and installed.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Firmware {
    /// Vendor image names, `{version}` standing for `major.minor`.
    pub image_pattern: String,
    /// The card stages an image in SDRAM and programs itself.
    pub sdram_staging: bool,
}

impl Firmware {
    /// The version in an image file name, read after the pattern's last
    /// `_`-separated token before `{version}` (`FPGA` in `E320_PWM_FPGA16.53_...`).
    #[must_use]
    pub fn version_in_name(&self, path: &str) -> Option<Version> {
        let name = std::path::Path::new(path).file_name()?.to_str()?;
        let prefix = self.image_pattern.split("{version}").next()?;
        let marker = prefix.rsplit('_').next().filter(|m| !m.is_empty())?;
        let rest = &name[name.find(marker)? + marker.len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        rest[..end].parse().ok()
    }
}

fn parse_all() -> Vec<CardModel> {
    let mut models: Vec<CardModel> = embedded::FILES
        .iter()
        .map(|(file, text)| {
            toml::from_str(text).unwrap_or_else(|e| panic!("config/cards/{file}: {e}"))
        })
        .collect();
    models.sort_by(|a, b| a.status.cmp(&b.status).then_with(|| a.name.cmp(&b.name)));
    models
}

/// Every embedded model: tested first, then by name.
pub fn models() -> &'static [CardModel] {
    static MODELS: OnceLock<Vec<CardModel>> = OnceLock::new();
    MODELS.get_or_init(parse_all)
}

/// The model whose discovery id byte is `id`.
#[must_use]
pub fn by_id(id: u8) -> Option<&'static CardModel> {
    models().iter().find(|m| m.id == id)
}

/// The model called `name`, case-insensitively.
#[must_use]
pub fn by_name(name: &str) -> Option<&'static CardModel> {
    models().iter().find(|m| m.name.eq_ignore_ascii_case(name))
}

/// The first tested model: what offline generation targets when no card is
/// named.
#[must_use]
pub fn default_model() -> &'static CardModel {
    &models()[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_file_parses_and_ids_are_unique() {
        let all = models();
        assert!(!all.is_empty());
        for (i, m) in all.iter().enumerate() {
            assert!(
                all[..i].iter().all(|o| o.id != m.id),
                "{}: id shared",
                m.name
            );
            assert!(
                all[..i]
                    .iter()
                    .all(|o| !o.name.eq_ignore_ascii_case(&m.name)),
                "{}: name shared",
                m.name
            );
            assert!(
                m.memory
                    .primary_blocks()
                    .contains(&m.memory.parameter_block),
                "{}: parameter block outside the bank",
                m.name
            );
            assert!(
                !m.memory.primary_blocks().contains(&m.memory.golden_block()),
                "{}: golden bank inside the primary",
                m.name
            );
        }
        assert_eq!(all[0].status, Status::Tested);
        for m in all {
            assert_eq!(m.status == Status::Tested, !m.tested.is_empty(), "{}: status and tested disagree", m.name);
            for t in &m.tested {
                assert!(t.image().is_some(), "{}: tested firmware {} is not in config/firmware.toml", m.name, t.firmware);
            }
        }
    }

    #[test]
    fn the_e120_is_found_by_id_and_by_name() {
        let m = by_id(0x64).expect("E120 by id");
        assert_eq!(m.name, "E120");
        assert_eq!(
            m.tested,
            [Tested {
                panel: "config/panels/p25-128x64-sm16269s.toml".into(),
                firmware: "E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex".into()
            }]
        );
        assert_eq!(m.tested[0].version(), Some(Version(16, 53)));
        assert!(std::ptr::eq(by_name("e120").unwrap(), m));
        assert!(std::ptr::eq(default_model(), m));
        assert!(by_id(0x03).is_none());
        assert!(by_name("e121").is_none());
    }

    #[test]
    fn the_e120_map_reads_as_blocks() {
        let m = by_name("E120").unwrap().memory.clone();
        assert_eq!(m.primary_blocks(), 0x00..0x0b);
        assert_eq!(m.bank_blocks(), 11);
        assert_eq!(m.golden_block(), 0x20);
        assert_eq!(m.config_page(), 0x0780);
        assert_eq!(m.guarded_blocks(Version(16, 53)), &[0, 1, 2, 8]);
        assert_eq!(m.guarded_blocks(Version(17, 0)), &[0, 1, 2, 8]);
        assert!(m.guarded_blocks(Version(10, 81)).is_empty());
        assert_eq!(m.boot_image.mapping_len(), 0x3000);
    }

    #[test]
    fn versions_order_numerically_and_round_trip() {
        assert!(Version(16, 53) < Version(17, 0));
        assert!(Version(9, 53) < Version(16, 53));
        assert_eq!("16.53".parse::<Version>(), Ok(Version(16, 53)));
        assert_eq!(Version(16, 53).to_string(), "16.53");
        assert!("16".parse::<Version>().is_err());
    }

    #[test]
    fn the_version_is_read_from_the_image_name() {
        let fw = &by_name("E120").unwrap().firmware;
        assert_eq!(
            fw.version_in_name(
                "third-party/firmware/E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex"
            ),
            Some(Version(16, 53))
        );
        assert_eq!(
            fw.version_in_name("E320_PCB6.0_PWM_FPGA10.81_20230907.hex"),
            Some(Version(10, 81))
        );
        assert_eq!(fw.version_in_name("image.hex"), None);
    }
}
