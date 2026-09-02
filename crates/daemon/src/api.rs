//! The request and response bodies of `docs/ui.md` section 2. With the `ts`
//! feature every type here derives `TS`; `tests/ts.rs` writes them to
//! `web/src/api/types.ts`, which the web app imports.

use crate::jobs::{GatedOutcome, Line};
use colorlight::DiscoveryInfo;
use serde::{Deserialize, Serialize};
use sources::{Fit, Pattern};
use std::fmt::Display;
use std::str::FromStr;
use wall::Canvas;

/// `settings.json`, and the body of `GET`/`PUT /settings`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Settings {
    pub iface: String,
    pub brightness: u8,
    /// A model from `config/cards/` overriding what discovery reports;
    /// `None` follows the last discovered card.
    #[serde(default)]
    pub card: Option<String>,
}

/// `colorlight::DiscoveryInfo` without `raw`; the field names are the API.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[allow(clippy::struct_field_names)]
pub struct Card {
    pub controller: u8,
    pub card_id: u8,
    /// The model name `config/cards/` gives the id byte; null when none does.
    pub model: Option<String>,
    pub ver_major: u8,
    pub ver_minor: u8,
    pub cols: u16,
    pub rows: u16,
}

impl From<&DiscoveryInfo> for Card {
    fn from(i: &DiscoveryInfo) -> Self {
        Self {
            controller: i.controller,
            card_id: i.card_id,
            model: receivers::by_id(i.card_id).map(|m| m.name.clone()),
            ver_major: i.ver_major,
            ver_minor: i.ver_minor,
            cols: i.cols,
            rows: i.rows,
        }
    }
}

/// `GET /health`: `version` alone without the token; the whole body with it.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct Health {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cards: Option<Vec<Card>>,
}

/// `POST /discover`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct DiscoverReq {
    /// Seconds to listen; 3 by default.
    pub wait: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Cards {
    pub cards: Vec<Card>,
}

/// `POST /brightness`, and its reply.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Brightness {
    pub value: u8,
}

/// The reply of every route that starts a job.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Started {
    pub id: String,
}

/// `Fit` and `Pattern` cross the API as their CLI spellings.
fn de_name<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    let s: String = Deserialize::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
}

fn de_name_opt<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    de_name(d).map(Some)
}

/// `POST /show/image` as JSON; the multipart form carries the same fields.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ShowImageReq {
    pub path: String,
    /// `stretch` by default, as `rxp show image`.
    #[serde(default, deserialize_with = "de_name_opt")]
    pub fit: Option<Fit>,
    pub hold: Option<bool>,
}

/// `POST /show/video`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ShowVideoReq {
    pub path: String,
    #[serde(rename = "loop")]
    pub looping: Option<bool>,
    /// 30 by default.
    pub fps: Option<u32>,
    /// `contain` by default.
    #[serde(default, deserialize_with = "de_name_opt")]
    pub fit: Option<Fit>,
    /// The daemon's wall by default.
    pub layout: Option<Canvas>,
}

/// `POST /show/pattern`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ShowPatternReq {
    #[serde(deserialize_with = "de_name")]
    pub name: Pattern,
    pub hold: Option<bool>,
}

/// `POST /show/fill`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ShowFillReq {
    /// `RRGGBB`, `#` optional.
    pub rgb: String,
    pub hold: Option<bool>,
}

/// `POST /config/gen`.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SpecReq {
    pub spec_toml: String,
}

/// The reply of `POST /config/gen`: the files `rxp config gen` writes.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GenFiles {
    pub name: String,
    pub files: GenFileSet,
    pub sources: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GenFileSet {
    /// Base64, `<name>.rcvbp`.
    pub rcvbp: String,
    /// Base64, 256 bytes.
    pub basic_pack: String,
    /// Base64, 65536 bytes; null when the image could not be built.
    pub block7: Option<String>,
    /// The `<name>-sources.txt` text.
    pub sources_txt: String,
}

/// `POST /config/read`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ConfigReadReq {
    pub index: Option<u16>,
    /// `FLASH_PAGE_BASIC_PARAM` by default.
    pub page: Option<u16>,
    /// 64 by default.
    pub max_chunks: Option<u16>,
    /// Seconds; 2 by default.
    pub wait: Option<u64>,
}

/// The reply of `POST /config/read`: base64 of the file bytes.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ConfigRead {
    pub rcvbp: String,
    pub lines: Vec<Line>,
}

/// `POST /config/write`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ConfigWriteReq {
    /// Base64 of the `.rcvbp` file.
    pub rcvbp: String,
    pub commit: Option<bool>,
    pub index: Option<u16>,
    pub wait: Option<u64>,
}

/// `POST /config/send`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ConfigSendReq {
    pub spec_toml: String,
    pub chip_only: Option<bool>,
    /// 8 by default.
    pub gap_ms: Option<u64>,
}

/// `POST /provision`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ProvisionReq {
    pub spec_toml: String,
    /// A `config/firmware.toml` name, a path, or `auto` for the image
    /// `POST /firmware/pick` chooses.
    pub firmware_path: Option<String>,
    pub position: (u16, u16),
    /// The card's position in the Ethernet chain; absent, the EEPROM frames
    /// broadcast and a chain of more than one card is refused.
    pub index: Option<u16>,
    /// `<data dir>/snapshots/<unix seconds>` by default.
    pub snapshot_dir: Option<String>,
    pub commit: Option<bool>,
    pub wait: Option<u64>,
}

/// `POST /flash/snapshot`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct SnapshotReq {
    pub dir: Option<String>,
    pub index: Option<u16>,
    pub wait: Option<u64>,
}

/// `POST /flash/restore`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct RestoreReq {
    pub dir: String,
    pub commit: Option<bool>,
    pub index: Option<u16>,
    pub wait: Option<u64>,
}

/// `POST /firmware/install`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct FirmwareReq {
    pub path: String,
    pub commit: Option<bool>,
    pub golden: Option<bool>,
    /// Seconds; 120 by default.
    pub timeout: Option<u64>,
    /// 3000 by default.
    pub chunk_delay_us: Option<u64>,
    /// Seconds; 4 by default.
    pub wait: Option<u64>,
}

/// One image of the `POST /firmware/pick` ranking (`ops::firmware::select`).
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct FirmwareCandidate {
    pub name: String,
    pub version: String,
    /// The board revision in the name; absent when it carries none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcb: Option<String>,
    pub kind: String,
    pub chips: Vec<String>,
    pub size: u64,
    pub sha256: String,
    pub score: u32,
    pub reasons: Vec<String>,
}

/// The reply of `POST /firmware/pick`: the whole ranking, and either the
/// image it decided on or why it refused.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct FirmwarePick {
    /// The spec's chip, as the ranking read it from the chip library.
    pub chip: String,
    /// The card model the ranking used.
    pub card: String,
    /// The image `provision --firmware auto` would install; null when the
    /// ranking refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen: Option<String>,
    /// The refusal text; null when one was chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
    pub candidates: Vec<FirmwareCandidate>,
}

/// The query of `GET /card/screen-size`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ScreenSizeQuery {
    pub index: Option<u16>,
    pub wait: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

/// `PUT /card/screen-size`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ScreenSizeReq {
    pub width: u16,
    pub height: u16,
    pub commit: Option<bool>,
    pub index: Option<u16>,
    pub wait: Option<u64>,
}

/// The reply of `PUT /card/screen-size`: the plan or the write, and the
/// size read back.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SizeOutcome {
    #[serde(flatten)]
    pub outcome: GatedOutcome,
    pub width: u16,
    pub height: u16,
}

/// `POST /card/reload`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct ReloadReq {
    pub index: Option<u16>,
    pub full: Option<bool>,
}

/// `POST /card/test-mode`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct TestModeReq {
    /// 0-255; 0 is off.
    pub n: u8,
    pub index: Option<u16>,
}

/// `POST /card/set-layout`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(optional_fields))]
pub struct SetLayoutReq {
    pub panel_width: u16,
    pub panel_height: u16,
    pub index: Option<u16>,
}
