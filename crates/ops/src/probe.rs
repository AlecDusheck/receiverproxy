//! `rxp card probe`: read the card and check every claim in its model file
//! that a read can check. Nothing is written; guarded blocks stay
//! `not checked` because checking them means writing.

use crate::capture::discover_one;
use crate::flash::{read_blocks, read_chunk};
use crate::model::for_card;
use crate::screen::looks_erased;
use crate::util::{contains_lattice_header, hex, open};
use crate::{protocol, rcvbp, Ctx, Progress};
use anyhow::{bail, Context, Result};
use protocol::{eeprom, DiscoveryInfo, SCREEN_RECORD_LEN};
use rcvbp::record01::View;
use receivers::{CardModel, Version};
use std::fmt;

/// What one check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// As the model says; the text adds what was seen.
    Ok(String),
    Mismatch { expected: String, seen: String },
    /// Reads cannot decide it; the text says why.
    NotChecked(String),
}

/// One claim of the model file and its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub what: String,
    pub state: State,
}

impl Check {
    fn ok(what: impl Into<String>, seen: impl Into<String>) -> Self {
        Self { what: what.into(), state: State::Ok(seen.into()) }
    }

    fn mismatch(what: impl Into<String>, expected: impl Into<String>, seen: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            state: State::Mismatch { expected: expected.into(), seen: seen.into() },
        }
    }

    fn unchecked(what: impl Into<String>, why: impl Into<String>) -> Self {
        Self { what: what.into(), state: State::NotChecked(why.into()) }
    }

    fn of(what: impl Into<String>, ok: bool, expected: impl Into<String>, seen: impl Into<String>) -> Self {
        if ok {
            Self::ok(what, seen)
        } else {
            Self::mismatch(what, expected, seen)
        }
    }
}

impl fmt::Display for Check {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.state {
            State::Ok(seen) if seen.is_empty() => write!(f, "{:<12} {}", "ok", self.what),
            State::Ok(seen) => write!(f, "{:<12} {}: {seen}", "ok", self.what),
            State::Mismatch { expected, seen } => {
                write!(f, "{:<12} {}: expected {expected}, seen {seen}", "mismatch", self.what)
            }
            State::NotChecked(why) => write!(f, "{:<12} {}: {why}", "not checked", self.what),
        }
    }
}

/// The checklist.
#[derive(Debug, Default)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    #[must_use]
    pub fn mismatches(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| matches!(c.state, State::Mismatch { .. }))
            .count()
    }

    /// `N ok, N mismatch, N not checked`.
    #[must_use]
    pub fn summary(&self) -> String {
        let n = |f: fn(&State) -> bool| self.checks.iter().filter(|c| f(&c.state)).count();
        format!(
            "{} ok, {} mismatch, {} not checked",
            n(|s| matches!(s, State::Ok(_))),
            n(|s| matches!(s, State::Mismatch { .. })),
            n(|s| matches!(s, State::NotChecked(_)))
        )
    }
}

/// The claims a discovery reply can check: the id byte and the size limits.
#[must_use]
pub fn check_discovery(m: &CardModel, info: &DiscoveryInfo) -> Vec<Check> {
    let l = &m.limits;
    vec![
        Check::of(
            "discovery id",
            info.card_id == m.id,
            format!("0x{:02x}", m.id),
            format!("0x{:02x}", info.card_id),
        ),
        Check::of(
            "reported size within limits",
            info.cols <= l.max_width && info.rows <= l.max_height,
            format!("at most {}x{}", l.max_width, l.max_height),
            format!("{}x{}", info.cols, info.rows),
        ),
    ]
}

/// The claims the first chunk of each firmware bank can check: a bitstream
/// header at the bank's start.
#[must_use]
pub fn check_banks(m: &CardModel, primary_head: &[u8], golden_head: &[u8]) -> Vec<Check> {
    let bank = |what: &str, addr: u32, head: &[u8]| {
        Check::of(
            format!("{what} bank at 0x{addr:06x}"),
            contains_lattice_header(head),
            "a bitstream header",
            if contains_lattice_header(head) { "bitstream header".to_owned() } else { format!("none in the first {} bytes", head.len()) },
        )
    };
    vec![
        bank("primary", m.memory.primary_bank, primary_head),
        bank("golden", m.memory.golden_bank, golden_head),
    ]
}

/// Where the EEPROM mirror sits inside the parameter block, when it does.
#[must_use]
pub fn mirror_in_block(m: &CardModel) -> Option<usize> {
    let mem = &m.memory;
    (mem.eeprom_mirror / mem.block_bytes == u32::from(mem.parameter_block))
        .then(|| (mem.eeprom_mirror % mem.block_bytes) as usize)
}

fn erased(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b == 0xFF)
}

/// The claims the parameter block and the EEPROM mirror can check: a boot
/// image at the model's region offsets, consistent with the `.rcvbp` it
/// embeds, and a programmed mirror.
#[must_use]
pub fn check_block(m: &CardModel, block: &[u8], mirror: &[u8]) -> Vec<Check> {
    let mem = &m.memory;
    let bi = &mem.boot_image;
    let mut out = Vec::new();
    if block.len() != mem.block_bytes as usize {
        out.push(Check::mismatch(
            format!("parameter block 0x{:02x}", mem.parameter_block),
            format!("{} bytes", mem.block_bytes),
            format!("{} bytes", block.len()),
        ));
        return out;
    }

    let pack = &block[bi.basic_pack..bi.basic_pack + 0x100];
    let pack_ok = rcvbp::spec::verify_basic_pack(pack);
    out.push(Check::of(
        format!("basic pack at +0x{:04x}", bi.basic_pack),
        pack_ok,
        "marker 0xa8 and its CRC at +0xfc",
        if pack_ok {
            "marker and CRC".to_owned()
        } else if pack[0] == 0xA8 {
            "marker, CRC differs".to_owned()
        } else {
            format!("marker 0x{:02x}", pack[0])
        },
    ));

    let at = bi.rcvbp;
    let len = u32::from_le_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]) as usize;
    let file = (len <= bi.rcvbp_max && at + 4 + len <= block.len()).then(|| &block[at + 4..at + 4 + len]);
    let cfg = file.and_then(|f| rcvbp::Rcvbp::from_bytes(f).ok());
    let what = format!("embedded .rcvbp at +0x{at:04x}");
    match (&file, &cfg) {
        (None, _) => out.push(Check::mismatch(
            what,
            format!("a length up to {}", bi.rcvbp_max),
            if len == 0xFFFF_FFFF { "erased".to_owned() } else { format!("length {len}") },
        )),
        (Some(_), None) => out.push(Check::mismatch(what, "a parsable file", format!("{len} bytes that do not parse"))),
        (Some(_), Some(c)) => out.push(Check::ok(what, format!("{len} bytes, {} records", c.records.len()))),
    }

    for (what, at, n) in [
        ("mapping", bi.mapping, bi.mapping_len()),
        ("scan table", bi.scan_table, 0x100),
    ] {
        out.push(Check::of(
            format!("{what} at +0x{at:04x}"),
            !erased(&block[at..at + n]),
            "written",
            if erased(&block[at..at + n]) { "erased" } else { "written" },
        ));
    }

    let page = &block[bi.chip_page..bi.chip_page + 0x100];
    let what = format!("chip page at +0x{:04x}", bi.chip_page);
    let reg84 = cfg.as_ref().and_then(|c| c.find_by_id(0x84)).map(|r| r.payload.as_slice());
    if erased(page) {
        out.push(Check::ok(what, "erased, drivers not armed at boot"));
    } else {
        match reg84 {
            Some(r) if r == page => out.push(Check::ok(what, "record 0x84")),
            Some(r) => out.push(Check::mismatch(
                what,
                "record 0x84",
                format!("{} bytes differ", r.iter().zip(page).filter(|(a, b)| a != b).count()),
            )),
            None => out.push(Check::unchecked(what, "no record 0x84 to compare")),
        }
    }

    let what = "basic pack against record 0x01";
    let rec = cfg.as_ref().and_then(|c| c.record_01()).and_then(|r| View::new(&r.payload).ok());
    match rec {
        Some(v) if pack_ok => {
            let seen = (pack[0x07], pack[0x08], u16::from_be_bytes([pack[0x88], pack[0x89]]), u16::from_be_bytes([pack[0x8A], pack[0x8B]]));
            let want = (v.scan(), v.gray(), v.max_width(), v.max_height());
            let show = |(s, g, w, h): (u8, u8, u16, u16)| format!("scan 1/{s}, {g} bits, screen {w}x{h}");
            out.push(Check::of(what, seen == want, show(want), show(seen)));
        }
        _ => out.push(Check::unchecked(what, "needs a verified pack and record 0x01")),
    }

    let what = format!("eeprom mirror at 0x{:06x}", mem.eeprom_mirror);
    if mirror.len() < SCREEN_RECORD_LEN {
        out.push(Check::mismatch(what, format!("{SCREEN_RECORD_LEN} bytes"), format!("{} bytes", mirror.len())));
    } else if looks_erased(mirror) {
        out.push(Check::mismatch(what, "a programmed record", "erased"));
    } else {
        let area = eeprom::parse_control_area(&mirror[2..])
            .map_or_else(String::new, |(x0, y0, x1, y1)| format!("control area {x0},{y0}-{x1},{y1}"));
        out.push(Check::ok(what, area));
    }
    out
}

/// The claims reads cannot decide.
#[must_use]
pub fn not_checked(m: &CardModel, running: Version) -> Vec<Check> {
    let guarded = m.memory.guarded_blocks(running);
    let mut out = vec![Check::unchecked(
        if guarded.is_empty() {
            format!("no guarded blocks on {running}")
        } else {
            format!("guarded blocks {} on {running}", hex(guarded, ","))
        },
        "checking means writing",
    )];
    out.push(Check::unchecked(format!("{} hub ports", m.limits.hub_ports), "not readable"));
    if let Some(chain) = m.limits.chain {
        out.push(Check::unchecked(format!("chain of {chain}"), "not readable"));
    }
    out.push(Check::unchecked(
        format!("firmware {}", if m.firmware.sdram_staging { "via SDRAM staging" } else { "via host page writes" }),
        "checking means installing firmware",
    ));
    out
}

/// What `rxp card probe` takes.
#[derive(Clone, Debug)]
pub struct Args<'a> {
    /// Directory for the bytes read, when wanted; nothing is written otherwise.
    pub out: Option<&'a str>,
    pub index: u16,
    /// Seconds to wait for each reply.
    pub wait: u64,
}

/// Discover the card, read its banks' heads, its parameter block and the
/// EEPROM mirror, and check the model. Read-only: every frame sent is a
/// discovery or a flash read.
///
/// # Errors
/// Fails when no card answers, its id has no model and none was named, or
/// a read goes unanswered.
pub fn probe(ctx: &Ctx, a: &Args, p: &mut dyn Progress) -> Result<Report> {
    let Some(info) = discover_one(ctx, a.wait)? else {
        bail!("no response on {} within {}s", ctx.iface, a.wait);
    };
    let m = match ctx.model {
        Some(m) => m,
        None => for_card(&info)?,
    };
    let running = Version(info.ver_major, info.ver_minor);
    p.err(&format!(
        "card: {} (id 0x{:02x}), firmware {running}, reports {}x{}",
        m.name, info.card_id, info.cols, info.rows
    ));
    let mut report = Report::default();
    report.checks.extend(check_discovery(m, &info));

    let mut dev = open(ctx)?;
    let head = |dev: &mut rawlink::Link, block: u8| read_chunk(dev, a.index, u16::from(block) << 8, a.wait);
    let primary = head(&mut dev, m.memory.primary_blocks().start)?;
    let golden = head(&mut dev, m.memory.golden_block())?;
    report.checks.extend(check_banks(m, &primary, &golden));

    let block = read_blocks(&mut dev, a.index, m.memory.parameter_block, 1, a.wait, p)?;
    let mirror = match mirror_in_block(m) {
        Some(off) => block[off..off + SCREEN_RECORD_LEN].to_vec(),
        None => {
            let page = (m.memory.eeprom_mirror / protocol::FLASH_PAGE_BYTES as u32) as u16;
            read_chunk(&mut dev, a.index, page, a.wait)?[..SCREEN_RECORD_LEN].to_vec()
        }
    };
    report.checks.extend(check_block(m, &block, &mirror));
    report.checks.extend(not_checked(m, running));

    for c in &report.checks {
        p.out(&c.to_string());
    }
    p.err(&report.summary());

    if let Some(dir) = a.out {
        std::fs::create_dir_all(dir).with_context(|| format!("create {dir}"))?;
        for (name, bytes) in [("parameter-block.bin", &block), ("eeprom-mirror.bin", &mirror)] {
            let path = format!("{dir}/{name}");
            std::fs::write(&path, bytes).with_context(|| format!("write {path}"))?;
            p.out(&path);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use panelspec::{embedded, PanelSpec};
    use receivers::by_name;

    fn e120() -> &'static CardModel {
        by_name("E120").unwrap()
    }

    /// The bench spec compiled for the E120, with a programmed mirror page.
    fn synthetic_block() -> Vec<u8> {
        let spec = PanelSpec::parse(embedded::PANELS[0].1).unwrap();
        let chip = spec
            .chip_library(&|p| embedded::chip(p).map(str::to_owned).ok_or_else(|| anyhow::anyhow!("{p}")))
            .unwrap();
        let g = rcvbp::spec::generate(&spec, &chip).unwrap();
        let mut block = rcvbp::image::compile(&e120().memory.boot_image, &spec, &g).unwrap().image;
        let off = mirror_in_block(e120()).unwrap();
        block[off..off + SCREEN_RECORD_LEN].fill(0);
        block[off + 2..off + 44].copy_from_slice(&eeprom::control_area(0, 0, 128, 64));
        block
    }

    fn mirror(block: &[u8]) -> Vec<u8> {
        let off = mirror_in_block(e120()).unwrap();
        block[off..off + SCREEN_RECORD_LEN].to_vec()
    }

    fn states(checks: &[Check]) -> Vec<(&str, &State)> {
        checks.iter().map(|c| (c.what.as_str(), &c.state)).collect()
    }

    #[test]
    fn the_generated_image_passes_every_block_check() {
        let block = synthetic_block();
        let checks = check_block(e120(), &block, &mirror(&block));
        let mismatches: Vec<_> = checks.iter().filter(|c| !matches!(c.state, State::Ok(_))).collect();
        assert!(mismatches.is_empty(), "{mismatches:?}");
        let lines: Vec<String> = checks.iter().map(ToString::to_string).collect();
        assert_eq!(lines[0], "ok           basic pack at +0x0000: marker and CRC");
        assert!(lines[1].starts_with("ok           embedded .rcvbp at +0x8000: "), "{}", lines[1]);
        assert!(lines[1].ends_with(" bytes, 17 records"), "{}", lines[1]);
        assert_eq!(lines[4], "ok           chip page at +0x0900: record 0x84");
        assert_eq!(lines[5], "ok           basic pack against record 0x01: scan 1/16, 12 bits, screen 128x64");
        assert_eq!(lines[6], "ok           eeprom mirror at 0x07f000: control area 0,0-128,64");
    }

    #[test]
    fn each_damaged_region_is_reported_against_the_model() {
        let m = e120();
        let good = synthetic_block();

        let mut block = good.clone();
        block[0x07] ^= 1;
        let c = check_block(m, &block, &mirror(&block));
        assert_eq!(
            c[0].state,
            State::Mismatch { expected: "marker 0xa8 and its CRC at +0xfc".into(), seen: "marker, CRC differs".into() }
        );
        assert!(matches!(c[5].state, State::NotChecked(_)), "{:?}", c[5]);

        let mut block = good.clone();
        block[0x8000..].fill(0xFF);
        let c = check_block(m, &block, &mirror(&block));
        assert_eq!(c[1].state, State::Mismatch { expected: "a length up to 28668".into(), seen: "erased".into() });
        assert_eq!(c[4].state, State::NotChecked("no record 0x84 to compare".into()));

        let mut block = good.clone();
        block[0x0900..0x0A00].fill(0xFF);
        let c = check_block(m, &block, &mirror(&block));
        assert_eq!(c[4].state, State::Ok("erased, drivers not armed at boot".into()));
        block[0x0900] = 0x55;
        let c = check_block(m, &block, &mirror(&block));
        assert!(matches!(&c[4].state, State::Mismatch { seen, .. } if seen.ends_with("bytes differ")), "{:?}", c[4]);

        let mut block = good.clone();
        block[0x3000..0x6000].fill(0xFF);
        let c = check_block(m, &block, &mirror(&block));
        assert_eq!(c[2].state, State::Mismatch { expected: "written".into(), seen: "erased".into() });

        let c = check_block(m, &good, &[0xFF; SCREEN_RECORD_LEN]);
        assert_eq!(c[6].state, State::Mismatch { expected: "a programmed record".into(), seen: "erased".into() });

        let c = check_block(m, &good[..0x8000], &mirror(&good));
        assert_eq!(states(&c), [("parameter block 0x07", &State::Mismatch { expected: "65536 bytes".into(), seen: "32768 bytes".into() })]);
    }

    #[test]
    fn discovery_and_banks_are_checked_against_the_model() {
        let m = e120();
        let info = |id, cols, rows| DiscoveryInfo { card_id: id, ver_major: 16, ver_minor: 53, cols, rows, controller: 0, raw: Vec::new() };
        let c = check_discovery(m, &info(0x64, 128, 64));
        assert!(c.iter().all(|c| matches!(c.state, State::Ok(_))), "{c:?}");
        let c = check_discovery(m, &info(0x65, 2048, 64));
        assert_eq!(c[0].state, State::Mismatch { expected: "0x64".into(), seen: "0x65".into() });
        assert_eq!(c[1].state, State::Mismatch { expected: "at most 1024x192".into(), seen: "2048x64".into() });

        let mut head = vec![0u8; 1024];
        head[100..121].copy_from_slice(b"Lattice Semiconductor");
        let c = check_banks(m, &head, &[0xFF; 1024]);
        assert_eq!(c[0].to_string(), "ok           primary bank at 0x000000: bitstream header");
        assert_eq!(c[1].to_string(), "mismatch     golden bank at 0x200000: expected a bitstream header, seen none in the first 1024 bytes");

        let n = not_checked(m, Version(16, 53));
        assert_eq!(n[0].to_string(), "not checked  guarded blocks 00,01,02,08 on 16.53: checking means writing");
        assert_eq!(n[1].to_string(), "not checked  12 hub ports: not readable");
        let r = Report { checks: [c, n].concat() };
        assert_eq!(r.mismatches(), 1);
        assert_eq!(r.summary(), "1 ok, 1 mismatch, 3 not checked");
    }
}
