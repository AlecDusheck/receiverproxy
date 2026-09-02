//! Which card is on the link: `--card` or the daemon's setting names a
//! model, otherwise the discovery reply's id byte picks one from
//! `config/cards/`.

use crate::capture::discover_one;
use crate::{protocol, Ctx, Progress};
use anyhow::{bail, Context, Result};
use panelspec::{embedded, ChipLibrary, PanelSpec};
use receivers::{by_id, by_name, models, CardModel, Status};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The model for a discovered card.
///
/// # Errors
/// Fails when no model file carries the card's id byte.
pub fn for_card(info: &protocol::DiscoveryInfo) -> Result<&'static CardModel> {
    by_id(info.card_id).with_context(|| {
        format!(
            "card id 0x{:02x} matches no model in config/cards (pass --card NAME to override)",
            info.card_id
        )
    })
}

/// The model named `name`, for `--card` and the daemon's setting.
///
/// # Errors
/// Fails on an unknown name, listing the known ones.
pub fn named(name: &str) -> Result<&'static CardModel> {
    by_name(name).with_context(|| {
        let known: Vec<&str> = models().iter().map(|m| m.name.as_str()).collect();
        format!("unknown card model {name:?}; known: {}", known.join(", "))
    })
}

/// The context's model, or the one discovery returns within `wait` seconds.
///
/// # Errors
/// Fails when no card answers or its id has no model.
pub fn resolve(ctx: &Ctx, wait: u64) -> Result<&'static CardModel> {
    if let Some(m) = ctx.model {
        return Ok(m);
    }
    let Some(info) = discover_one(ctx, wait)? else {
        bail!("no response on {} within {wait}s", ctx.iface);
    };
    for_card(&info)
}

/// The colorlight allowlists for a model.
#[must_use]
pub fn flash_map(m: &CardModel) -> protocol::FlashMap {
    protocol::FlashMap {
        param_block: m.memory.parameter_block,
        firmware_blocks: m.memory.primary_blocks(),
        golden_block: m.memory.golden_block(),
        screen_record_addr: m.memory.eeprom_mirror,
    }
}

/// Bytes in the primary firmware bank.
#[must_use]
pub fn bank_bytes(m: &CardModel) -> usize {
    m.memory.bank_bytes as usize
}

/// `rxp card models`: one line per model, tested first.
pub fn list(p: &mut dyn Progress) {
    for m in models() {
        p.out(&format!(
            "{:<8} id=0x{:02x}  {:<11} {}",
            m.name, m.id, m.status, m.vendor
        ));
    }
}

/// The matrix cell for a status.
const fn symbol(s: Status) -> &'static str {
    match s {
        Status::Tested => "✅",
        Status::Generates => "⚠️",
        Status::Unsupported => "❌",
    }
}

/// Columns the model files do not carry: the other cards of the implemented
/// family share its protocol and firmware naming but none has been tried,
/// and no other vendor's protocol is implemented.
const OTHER_COLUMNS: [(&str, Status); 2] = [
    ("other Colorlight E-series", Status::Generates),
    ("Linsn · Novastar · Huidu", Status::Unsupported),
];

/// Rows the mined set does not carry: chips whose register record is not
/// decoded, and module wirings the pixel-map generator does not produce.
const UNSUPPORTED_ROWS: [&str; 2] = [
    "SM16369S · ICND2263 (register record not decoded)",
    "snake-wired outdoor modules (1/2, 1/4, 1/5, 1/10 scan)",
];

/// A chip library's name without its `(mined)` / `(factory values)` tag.
fn chip_name(lib: &ChipLibrary) -> String {
    lib.name.split(" (").next().unwrap_or(&lib.name).to_owned()
}

/// The driver-chip family of a chip name: its leading letters, `ICND`
/// folded into `ICN`.
fn family(chip: &str) -> String {
    let letters: String = chip.chars().take_while(char::is_ascii_alphabetic).collect();
    if letters == "ICND" { "ICN".to_owned() } else { letters }
}

fn embedded_chip(path: &str) -> Result<ChipLibrary> {
    let text = embedded::chip(path).with_context(|| format!("{path}: not embedded"))?;
    ChipLibrary::parse(text).with_context(|| format!("parse {path}"))
}

/// `rxp card models --markdown`: the README's Tested matrix. One column per
/// model file, one row per panel a model was driven with, then one row per
/// driver-chip family among the mined module classes.
///
/// # Errors
/// Fails when a model names a panel or chip library that is not embedded.
pub fn matrix_markdown() -> Result<String> {
    let mut s = String::from("✅ driven on the bench · ⚠️ configuration generates, never driven · ❌ not supported

");
    let _ = write!(s, "| panel (driver chip) |");
    for m in models() {
        let _ = write!(s, " {} {} |", m.vendor, m.name);
    }
    for (name, _) in OTHER_COLUMNS {
        let _ = write!(s, " {name} |");
    }
    s.push_str("
|---|");
    for _ in 0..models().len() + OTHER_COLUMNS.len() {
        s.push_str(":---:|");
    }
    s.push('\n');
    let row = |s: &mut String, label: &str, cell: &dyn Fn(&CardModel) -> Status, other: &dyn Fn(Status) -> Status| {
        let _ = write!(s, "| {label} |");
        for m in models() {
            let _ = write!(s, " {} |", symbol(cell(m)));
        }
        for (_, status) in OTHER_COLUMNS {
            let _ = write!(s, " {} |", symbol(other(status)));
        }
        s.push('\n');
    };

    let mut driven: Vec<&str> = Vec::new();
    for t in models().iter().flat_map(|m| &m.tested) {
        if !driven.contains(&t.panel.as_str()) {
            driven.push(&t.panel);
        }
    }
    for path in driven {
        let text = embedded::panel(path).with_context(|| format!("{path}: not embedded"))?;
        let spec = PanelSpec::parse(text).with_context(|| format!("parse {path}"))?;
        let chip = chip_name(&embedded_chip(&spec.chip.library)?);
        let label = format!(
            "{}x{} 1/{}, {chip} (`{path}`)",
            spec.module.width, spec.module.height, spec.module.scan
        );
        let cell = |m: &CardModel| {
            if m.tested.iter().any(|t| t.panel == path) {
                Status::Tested
            } else {
                m.status.max(Status::Generates)
            }
        };
        row(&mut s, &label, &cell, &|o| o);
    }

    // Mined module classes, grouped by the family of the chip each names.
    let mut families: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut classes = 0usize;
    for (path, text) in embedded::PANELS {
        if !embedded::is_mined(path) {
            continue;
        }
        classes += 1;
        let spec = PanelSpec::parse(text).with_context(|| format!("parse {path}"))?;
        let chip = chip_name(&embedded_chip(&spec.chip.library)?);
        let chips = families.entry(family(&chip)).or_default();
        if !chips.contains(&chip) {
            chips.push(chip);
        }
    }
    let generates = |m: &CardModel| m.status.max(Status::Generates);
    for chips in families.values_mut() {
        chips.sort();
        row(&mut s, &chips.join(" · "), &generates, &|o| o);
    }
    for label in UNSUPPORTED_ROWS {
        row(&mut s, label, &|_| Status::Unsupported, &|_| Status::Unsupported);
    }
    let _ = write!(
        s,
        "
The ⚠️ chip rows are the {classes} module classes in `config/panels/mined/`, grouped by driver-chip family."
    );
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The README's matrix is what `--markdown` prints.
    #[test]
    fn the_readme_matrix_is_generated() {
        let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md")).unwrap();
        let (_, rest) = readme.split_once("<!-- tested -->").expect("<!-- tested --> marker");
        let (table, _) = rest.split_once("<!-- /tested -->").expect("<!-- /tested --> marker");
        assert_eq!(table.trim(), matrix_markdown().unwrap().trim(), "regenerate with: rxp card models --markdown");
    }

    #[test]
    fn the_matrix_has_one_column_per_model_and_the_tested_panel_first() {
        let s = matrix_markdown().unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[2], "| panel (driver chip) | Colorlight E120 | other Colorlight E-series | Linsn · Novastar · Huidu |");
        assert_eq!(lines[3], "|---|:---:|:---:|:---:|");
        assert_eq!(lines[4], "| 128x64 1/16, SM16269S (`config/panels/p25-128x64-sm16269s.toml`) | ✅ | ⚠️ | ❌ |");
        assert!(lines[5].starts_with("| DP5525 | ⚠️ |"), "{}", lines[5]);
        assert!(s.contains("| ICN2038S · ICN2053 · ICN2055 · ICN2065 · ICND2163 | ⚠️ | ⚠️ | ❌ |"), "{s}");
        assert_eq!(family("ICND2163"), "ICN");
        assert_eq!(family("SM16380"), "SM");
    }

    #[test]
    fn the_e120_model_gives_the_pinned_allowlists() {
        assert_eq!(flash_map(named("e120").unwrap()), protocol::E120);
        assert_eq!(bank_bytes(named("E120").unwrap()), 11 * 0x10000);
    }

    #[test]
    fn an_unknown_name_lists_the_models() {
        let e = named("x").unwrap_err().to_string();
        assert!(e.contains("E120"), "{e}");
    }
}
