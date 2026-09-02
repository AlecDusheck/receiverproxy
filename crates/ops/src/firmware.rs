//! Firmware images by manifest name or path.
//!
//! `config/firmware.toml` (`receivers::firmware`) lists the archived images
//! with their sha256; a name from it is looked for under
//! `third-party/firmware` and then in the config directory's cache, and every
//! image that is in the manifest is checked against it before a write.

use crate::util::warn;
use crate::Progress;
use anyhow::{bail, Context, Result};
use panelspec::{ChipLibrary, PanelSpec};
use receivers::firmware::{image as entry, manifest, Image};
use receivers::{CardModel, Tested};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The archived images, relative to the repository root.
pub const ARCHIVE: &str = "third-party/firmware";

/// The `provision --firmware` value that ranks the manifest instead of
/// naming an image ([`pick`]).
pub const AUTO: &str = "auto";

/// Where `fetch` writes: `<config dir>/receiverproxy/firmware`.
///
/// # Errors
/// When the OS names no configuration directory.
pub fn cache_dir() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("no configuration directory for this user")?
        .join("receiverproxy")
        .join("firmware"))
}

/// An image argument resolved to a file, and its manifest entry when it has one.
#[derive(Debug)]
pub struct Resolved {
    pub path: PathBuf,
    pub image: Option<&'static Image>,
}

/// A manifest name to the archive or the cache; anything else is a path.
///
/// A path is matched to the manifest by file name. The archive is relative
/// to the working directory, the repository root when the commands run as
/// documented.
///
/// # Errors
/// A manifest name whose file is in neither place.
pub fn resolve(arg: &str) -> Result<Resolved> {
    resolve_in(Path::new(""), arg)
}

fn resolve_in(root: &Path, arg: &str) -> Result<Resolved> {
    if let Some(image) = entry(arg) {
        let archive = root.join(ARCHIVE).join(arg);
        if archive.is_file() {
            return Ok(Resolved { path: archive, image: Some(image) });
        }
        let cached = cache_dir()?.join(arg);
        if cached.is_file() {
            return Ok(Resolved { path: cached, image: Some(image) });
        }
        let hint = if manifest().base_url.is_empty() {
            ""
        } else {
            "; rxp firmware fetch downloads it"
        };
        bail!(
            "{arg}: not at {} or {}{hint}",
            archive.display(),
            cached.display()
        );
    }
    let path = PathBuf::from(arg);
    let image = path.file_name().and_then(|n| n.to_str()).and_then(entry);
    Ok(Resolved { path, image })
}

/// An image read from disk.
#[derive(Debug)]
pub struct Loaded {
    /// The path read, as printed.
    pub path: String,
    pub bytes: Vec<u8>,
    /// True when the manifest's sha256 was checked.
    pub verified: bool,
}

/// Read `arg` and check it against the manifest when it is listed there;
/// a file outside the manifest is used as is, with a warning.
///
/// # Errors
/// The file cannot be read, or its size or sha256 disagrees with the manifest.
pub fn load(arg: &str, p: &mut dyn Progress) -> Result<Loaded> {
    let r = resolve(arg)?;
    let path = r.path.display().to_string();
    let bytes = std::fs::read(&r.path).with_context(|| format!("read {path}"))?;
    let verified = match r.image {
        Some(image) => {
            image
                .verify(&bytes)
                .map_err(|e| anyhow::anyhow!("firmware: {e}; refusing to write it"))?;
            true
        }
        None => {
            warn(p, format!("{path} is not in config/firmware.toml; used as is, sha256 unchecked"));
            false
        }
    };
    Ok(Loaded { path, bytes, verified })
}

/// The word a plan line carries for a loaded image.
#[must_use]
pub const fn checked(l: &Loaded) -> &'static str {
    if l.verified {
        "sha256 verified"
    } else {
        "unverified"
    }
}

/// Where a manifest entry's file is, for `list`.
fn location(name: &str) -> String {
    let archive = Path::new(ARCHIVE).join(name);
    if archive.is_file() {
        return ARCHIVE.to_string();
    }
    match cache_dir() {
        Ok(d) if d.join(name).is_file() => d.display().to_string(),
        _ => "absent".to_string(),
    }
}

/// `rxp firmware list`: the manifest, one line per image, and where each is.
pub fn list(p: &mut dyn Progress) {
    let m = manifest();
    p.out(&format!(
        "base_url: {}",
        if m.base_url.is_empty() { "(empty: local only)" } else { &m.base_url }
    ));
    p.out(&format!(
        "{:<56} {:<7} {:<4} {:<9} {:<20} {:>7}  location",
        "name", "version", "pcb", "kind", "chips", "bytes"
    ));
    for i in &m.image {
        p.out(&format!(
            "{:<56} {:<7} {:<4} {:<9} {:<20} {:>7}  {}",
            i.name,
            i.version.to_string(),
            i.pcb.as_deref().unwrap_or("-"),
            i.kind,
            if i.chips.is_empty() { "-".to_string() } else { i.chips.join(",") },
            i.size,
            location(&i.name)
        ));
    }
}

/// `rxp firmware fetch NAME`: download `base_url/path` with `curl` into the
/// cache after checking its sha256. With an empty `base_url` it reports
/// where the image is expected instead.
///
/// # Errors
/// An unknown name, a failed download, or a hash that disagrees with the
/// manifest; the cache is left without the file on a mismatch.
pub fn fetch(name: &str, p: &mut dyn Progress) -> Result<()> {
    fetch_in(Path::new(""), name, p)
}

fn fetch_in(root: &Path, name: &str, p: &mut dyn Progress) -> Result<()> {
    let image = entry(name).with_context(|| format!("{name}: not in config/firmware.toml (rxp firmware list)"))?;
    let cache = cache_dir()?;
    let dest = cache.join(name);
    let m = manifest();
    // A local copy that verifies is used before anything is downloaded.
    let archive = root.join(ARCHIVE).join(name);
    for path in [&archive, &dest] {
        if path.is_file() {
            let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
            if image.verify(&bytes).is_ok() {
                p.out(&path.display().to_string());
                return Ok(());
            }
        }
    }
    if m.base_url.is_empty() {
        bail!(
            "base_url is empty in config/firmware.toml; {name} is expected at {} or {}",
            archive.display(),
            dest.display()
        );
    }
    let url = format!("{}/{}", m.base_url.trim_end_matches('/'), image.path);
    p.err(&format!("fetch: {url}"));
    std::fs::create_dir_all(&cache).with_context(|| format!("create {}", cache.display()))?;
    let tmp = cache.join(format!("{name}.part"));
    let status = std::process::Command::new("curl")
        .args(["-fsSL", "--max-filesize", &(image.size + 1).to_string(), "-o"])
        .arg(&tmp)
        .arg(&url)
        .status()
        .context("run curl")?;
    anyhow::ensure!(status.success(), "fetch {url}: curl exited with {status}");
    let bytes = std::fs::read(&tmp).with_context(|| format!("read {}", tmp.display()))?;
    if let Err(e) = image.verify(&bytes) {
        let _ = std::fs::remove_file(&tmp);
        bail!("fetch: {e}; not cached");
    }
    std::fs::rename(&tmp, &dest).with_context(|| format!("rename to {}", dest.display()))?;
    p.out(&dest.display().to_string());
    Ok(())
}

// --- choosing an image for a panel ------------------------------------------

/// Rule 1: the card model was driven with this image and this spec.
const TESTED: u32 = 1000;
/// Rule 2: the image's chip list names the spec's chip.
const CHIP_EXACT: u32 = 500;
/// Rule 2 through the vendor's suffix forms (`SM16269S` in `SM16269SH`).
const CHIP_FAMILY: u32 = 400;
/// Rule 3: the build kind suits the chip class.
const KIND: u32 = 100;

/// One image the ranking considered, and why it scored what it did.
#[derive(Debug)]
pub struct Candidate {
    pub image: &'static Image,
    pub score: u32,
    pub reasons: Vec<String>,
}

impl Candidate {
    /// The reasons as one line, for a table.
    #[must_use]
    pub fn why(&self) -> String {
        self.reasons.join("; ")
    }

    /// Rule 1 or rule 2 decided this candidate; anything below is a guess.
    const fn decided(&self) -> bool {
        self.score >= CHIP_FAMILY
    }

    /// What orders equal scores: version, then the build date in the name.
    fn tie_break(&self) -> (receivers::Version, u32) {
        (self.image.version, built(&self.image.name).unwrap_or(0))
    }
}

/// What the chip library says the part is. The vendor builds a `PWM`
/// gateware for an S-PWM driver and a `Normal` one for a plain shift
/// register; the library's shape says which the chip is, not its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// Addressed registers (`order`/`registers`) or a `chip_custom` block.
    Spwm,
    /// Neither: a plain shift register.
    Plain,
}

impl Class {
    fn of(lib: &ChipLibrary) -> Self {
        if lib.order.is_empty() && lib.registers.is_empty() && lib.chip_custom.is_none() {
            Self::Plain
        } else {
            Self::Spwm
        }
    }

    const fn kind(self) -> &'static str {
        match self {
            Self::Spwm => "PWM",
            Self::Plain => "Normal",
        }
    }

    const fn what(self) -> &'static str {
        match self {
            Self::Spwm => "an S-PWM chip",
            Self::Plain => "a plain shift register",
        }
    }
}

/// The part name in a chip library's `name`: `SM16269S (factory values)` and
/// `MBI5124 (mined)` are both the part before the parenthetical.
fn part(name: &str) -> &str {
    name.split('(')
        .next()
        .unwrap_or(name)
        .split_whitespace()
        .next()
        .unwrap_or(name)
}

/// The spec's chip library: the embedded set, then the filesystem.
fn library(spec: &PanelSpec) -> Option<ChipLibrary> {
    let text = panelspec::embedded::chip(&spec.chip.library)
        .map(str::to_owned)
        .or_else(|| panelspec::read_library(&spec.chip.library).ok())?;
    ChipLibrary::parse(&text).ok()
}

/// The part name a spec's chip library gives, or the library path when it
/// cannot be read.
#[must_use]
pub fn chip_name(spec: &PanelSpec) -> String {
    library(spec).map_or_else(
        || spec.chip.library.clone(),
        |lib| part(&lib.name).to_owned(),
    )
}

/// `SM16269SH` as `("SM", "16269", "SH")`; `None` without a digit run.
fn split_part(s: &str) -> Option<(&str, &str, &str)> {
    let first = s.find(|c: char| c.is_ascii_digit())?;
    let end = s[first..]
        .find(|c: char| !c.is_ascii_digit())
        .map_or(s.len(), |n| first + n);
    Some((&s[..first], &s[first..end], &s[end..]))
}

/// The same part: equal names, or equal prefix and digits with one trailing
/// letter group a prefix of the other (`SM16269S` in `SM16269SH`,
/// `ICN2263` in `ICN2263ALL`). `ICND2263` is a different part.
fn same_part(a: &str, b: &str) -> bool {
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    let (Some((pa, da, sa)), Some((pb, db, sb))) = (split_part(a), split_part(b)) else {
        return false;
    };
    let alpha = |s: &str| s.chars().all(|c| c.is_ascii_alphabetic());
    pa.eq_ignore_ascii_case(pb)
        && da == db
        && alpha(sa)
        && alpha(sb)
        && (starts(sa, sb) || starts(sb, sa))
}

fn starts(long: &str, short: &str) -> bool {
    long.len() >= short.len() && long[..short.len()].eq_ignore_ascii_case(short)
}

/// The `yyyymmdd` in a vendor image name, when it carries one.
fn built(name: &str) -> Option<u32> {
    let b = name.as_bytes();
    let digit = |j: usize| b.get(j).is_some_and(u8::is_ascii_digit);
    for i in 0..b.len().saturating_sub(7) {
        let run = &b[i..i + 8];
        // A run of exactly eight digits, so a truncated date is not read as one.
        if run.starts_with(b"20")
            && run.iter().all(u8::is_ascii_digit)
            && !(i > 0 && digit(i - 1))
            && !digit(i + 8)
        {
            return name[i..i + 8].parse().ok();
        }
    }
    None
}

/// True when a `[[tested]]` entry names this spec: the embedded spec at its
/// path carries the same name, or the file stem does.
fn tested_with(spec: &PanelSpec, t: &Tested) -> bool {
    panelspec::embedded::panel(&t.panel)
        .and_then(|text| PanelSpec::parse(text).ok())
        .map_or_else(
            || Path::new(&t.panel).file_stem().is_some_and(|s| s == spec.name.as_str()),
            |s| s.name == spec.name,
        )
}

/// The manifest ranked for a panel on a card, best first.
///
/// The rules, by weight: an image the card model records as tested with this
/// spec; the image's chip list naming the spec's chip; a build kind that
/// suits the chip class; then version and build date. `config/cards/*.toml`
/// records no board revision, so nothing here matches `pcb`.
///
/// A build for one chip family (`LS0allDA`, `LS9937`, `DP3263`, `DS`) is a
/// candidate only when its chip list matched.
#[must_use]
pub fn select(spec: &PanelSpec, card: &CardModel) -> Vec<Candidate> {
    let lib = library(spec);
    let chip = lib.as_ref().map(|l| part(&l.name));
    let class = lib.as_ref().map(Class::of);
    let mut out: Vec<Candidate> = manifest()
        .image
        .iter()
        .filter_map(|image| candidate(image, spec, card, chip, class))
        .collect();
    out.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.tie_break().cmp(&a.tie_break()))
            .then_with(|| a.image.name.cmp(&b.image.name))
    });
    out
}

fn candidate(
    image: &'static Image,
    spec: &PanelSpec,
    card: &CardModel,
    chip: Option<&str>,
    class: Option<Class>,
) -> Option<Candidate> {
    let mut score = 0;
    let mut reasons = Vec::new();

    if card
        .tested
        .iter()
        .any(|t| t.firmware == image.name && tested_with(spec, t))
    {
        score += TESTED;
        reasons.push(format!("driven on the {} with this spec", card.name));
    }

    let hit = chip.and_then(|c| image.chips.iter().find(|listed| same_part(c, listed)));
    match (chip, hit) {
        (Some(c), Some(listed)) if listed.eq_ignore_ascii_case(c) => {
            score += CHIP_EXACT;
            reasons.push(format!("names {listed}"));
        }
        (Some(c), Some(listed)) => {
            score += CHIP_FAMILY;
            reasons.push(format!("names {listed}, the {c} family"));
        }
        _ => {}
    }

    // A kind built for one chip family says nothing about any other chip.
    let general =
        image.kind.eq_ignore_ascii_case("PWM") || image.kind.eq_ignore_ascii_case("Normal");
    if !general && hit.is_none() {
        return None;
    }
    if let Some(class) = class {
        if image.kind.eq_ignore_ascii_case(class.kind()) {
            score += KIND;
            reasons.push(format!("{} suits {}", class.kind(), class.what()));
        } else if general {
            reasons.push(format!("{} does not suit {}", image.kind, class.what()));
        }
    }
    if reasons.is_empty() {
        reasons.push("nothing matched".to_owned());
    }
    Some(Candidate { image, score, reasons })
}

/// The one image a ranking chose: the top candidate, when rule 1 or rule 2
/// decided it and nothing else ranks with it.
///
/// # Errors
/// Names `chip` and the top five candidates with their reasons.
pub fn chosen<'a>(ranked: &'a [Candidate], chip: &str) -> Result<&'a Candidate> {
    let top = ranked.first().filter(|c| c.decided());
    let tie = |a: &Candidate, b: &Candidate| a.score == b.score && a.tie_break() == b.tie_break();
    match (top, ranked.get(1)) {
        (Some(c), Some(next)) if tie(c, next) => Err(refusal(ranked, chip)),
        (Some(c), _) => Ok(c),
        (None, _) => Err(refusal(ranked, chip)),
    }
}

fn refusal(ranked: &[Candidate], chip: &str) -> anyhow::Error {
    let mut text = format!("no firmware chosen for {chip}: {} candidates", ranked.len());
    for c in ranked.iter().take(5) {
        let _ = write!(
            text,
            "\n  {} {} {}: {}",
            c.image.name,
            c.image.version,
            c.image.kind,
            c.why()
        );
    }
    anyhow::anyhow!(text)
}

/// The image to install for a panel on a card.
///
/// # Errors
/// When no candidate was decided by the chip or a tested entry, or two rank
/// alike; the message is the ranking.
pub fn pick(spec: &PanelSpec, card: &CardModel) -> Result<&'static Image> {
    Ok(chosen(&select(spec, card), &chip_name(spec))?.image)
}

/// `rxp firmware pick`: the ranking, best first, and what it chose.
pub fn print_pick(spec: &PanelSpec, card: &CardModel, top: usize, p: &mut dyn Progress) {
    let chip = chip_name(spec);
    let ranked = select(spec, card);
    p.err(&format!(
        "spec {}, chip {chip}, card {}: {} candidates",
        spec.name,
        card.name,
        ranked.len()
    ));
    p.out(&format!(
        "{:<62} {:<7} {:<7} {:<20} why",
        "name", "version", "kind", "chips"
    ));
    for c in ranked.iter().take(top) {
        p.out(&format!(
            "{:<62} {:<7} {:<7} {:<20} {}",
            c.image.name,
            c.image.version.to_string(),
            c.image.kind,
            if c.image.chips.is_empty() { "-".to_owned() } else { c.image.chips.join(",") },
            c.why()
        ));
    }
    match chosen(&ranked, &chip) {
        Ok(c) => p.out(&format!("pick: {}", c.image.name)),
        Err(e) => p.err(&format!("pick: {e:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex";

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[derive(Default)]
    struct Lines {
        out: Vec<String>,
        err: Vec<String>,
    }

    impl Progress for Lines {
        fn out(&mut self, line: &str) {
            self.out.push(line.to_string());
        }
        fn err(&mut self, line: &str) {
            self.err.push(line.to_string());
        }
    }

    #[test]
    fn a_name_resolves_to_the_archive_and_a_path_to_its_entry() {
        let r = resolve_in(&root(), NAME).unwrap();
        assert_eq!(r.path, root().join(ARCHIVE).join(NAME));
        assert_eq!(r.image.map(|i| i.version), Some(receivers::Version(16, 53)));

        let by_path = root().join(ARCHIVE).join(NAME);
        let r = resolve(by_path.to_str().unwrap()).unwrap();
        assert!(r.image.is_some());
        let mut lines = Lines::default();
        assert!(load(by_path.to_str().unwrap(), &mut lines).unwrap().verified);
        assert!(lines.err.is_empty());

        let r = resolve("build/other.hex").unwrap();
        assert!(r.image.is_none());
        assert_eq!(r.path, Path::new("build/other.hex"));
        assert_eq!(resolve("").unwrap().path, Path::new(""));
    }

    #[test]
    fn a_manifest_entry_with_the_wrong_bytes_is_refused() {
        let dir = std::env::temp_dir().join(format!("rxp-firmware-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join(NAME);
        std::fs::write(&bad, b"not the image").unwrap();
        let mut lines = Lines::default();
        let e = load(bad.to_str().unwrap(), &mut lines).unwrap_err().to_string();
        assert!(e.contains("size"), "{e}");
        assert!(e.contains("refusing"), "{e}");

        let other = dir.join("unlisted.hex");
        std::fs::write(&other, b"anything").unwrap();
        let l = load(other.to_str().unwrap(), &mut lines).unwrap();
        assert!(!l.verified);
        assert_eq!(l.bytes, b"anything");
        assert!(lines.err.iter().any(|l| l.contains("not in config/firmware.toml")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn bench_spec() -> PanelSpec {
        PanelSpec::load(root().join("config/panels/p25-128x64-sm16269s.toml")).unwrap()
    }

    /// A spec whose only interesting field is the chip library it names.
    fn spec_for(library: &str) -> PanelSpec {
        let mut s = bench_spec();
        s.name = "test".into();
        s.chip.library = library.into();
        s
    }

    fn e120() -> &'static receivers::CardModel {
        receivers::by_name("E120").unwrap()
    }

    #[test]
    fn the_bench_spec_picks_the_image_it_was_driven_with() {
        let spec = bench_spec();
        let card = e120();
        assert_eq!(chip_name(&spec), "SM16269S");
        let ranked = select(&spec, card);
        assert_eq!(ranked[0].image.name, NAME);
        assert_eq!(ranked[0].score, TESTED + CHIP_FAMILY + KIND);
        assert!(ranked[0].reasons.iter().any(|r| r.contains("driven on the E120")));
        assert_eq!(pick(&spec, card).unwrap().name, NAME);

        // Rule 2 alone reaches the same image: it is the only one whose chip
        // list names the SM16269S family.
        let mut untested = card.clone();
        untested.tested.clear();
        let ranked = select(&spec, &untested);
        assert_eq!(ranked[0].image.name, NAME);
        assert_eq!(ranked[0].score, CHIP_FAMILY + KIND);
        assert!(ranked[0].reasons.iter().any(|r| r.contains("SM16269SH")));
        assert_eq!(pick(&spec, &untested).unwrap().name, NAME);
    }

    #[test]
    fn an_spwm_chip_no_image_names_is_refused_with_the_ranking() {
        // No image in config/firmware.toml lists ICN2053.
        let spec = spec_for("config/chips/mined/icn2053.toml");
        let ranked = select(&spec, e120());
        assert_eq!(chip_name(&spec), "ICN2053");
        assert!(ranked.iter().all(|c| c.score <= KIND));
        // Rule 3 still sorts the S-PWM builds first.
        assert!(ranked[0].image.kind.eq_ignore_ascii_case("PWM"));
        // A build for one chip family is not a candidate for another chip.
        assert!(ranked.iter().all(|c| {
            c.image.kind.eq_ignore_ascii_case("PWM") || c.image.kind.eq_ignore_ascii_case("Normal")
        }));
        let e = pick(&spec, e120()).unwrap_err().to_string();
        assert!(e.starts_with("no firmware chosen for ICN2053: "), "{e}");
        assert_eq!(e.lines().count(), 6, "{e}");
        assert!(e.contains(&ranked[0].image.name), "{e}");
    }

    #[test]
    fn a_plain_shift_register_prefers_a_normal_build() {
        // No shipped chip library classes as plain: every one carries either a
        // register table or a chip_custom block, the mined MBI5124 included.
        // The rule is pinned here against a library of the shape a plain part
        // would have.
        let plain = ChipLibrary::parse(
            "name = \"MBI5124\"\nfamily_id = 6\nserial_clock = 8\nchip_control = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]\n",
        )
        .unwrap();
        assert_eq!(Class::of(&plain), Class::Plain);
        let mined = ChipLibrary::parse(
            &std::fs::read_to_string(root().join("config/chips/mined/mbi5124.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(part(&mined.name), "MBI5124");
        assert_eq!(Class::of(&mined), Class::Spwm);

        // The manifest lists no MBI5124, so rule 3 alone orders the ranking.
        let spec = spec_for("config/chips/mined/mbi5124.toml");
        let card = e120();
        let scored = |class| -> Vec<&'static Image> {
            manifest()
                .image
                .iter()
                .filter_map(|i| candidate(i, &spec, card, Some("MBI5124"), Some(class)))
                .filter(|c| c.score > 0)
                .map(|c| c.image)
                .collect()
        };
        let plain = scored(Class::Plain);
        assert!(!plain.is_empty());
        assert!(plain.iter().all(|i| i.kind.eq_ignore_ascii_case("Normal")));
        assert!(scored(Class::Spwm).iter().all(|i| i.kind.eq_ignore_ascii_case("PWM")));
        // The mined library's chip_custom block classes it S-PWM, so the
        // shipped file ranks the PWM builds first.
        assert!(select(&spec, card)[0].image.kind.eq_ignore_ascii_case("PWM"));
    }

    #[test]
    fn part_names_match_the_vendors_suffix_forms() {
        assert!(same_part("SM16269S", "SM16269SH"));
        assert!(same_part("ICN2053", "icn2053"));
        assert!(same_part("ICN2263", "ICN2263ALL"));
        assert!(!same_part("ICN2263", "ICND2263"));
        assert!(!same_part("SM16269S", "SM16289N"));
        assert!(!same_part("MBI5124", "MBI5153"));
        assert_eq!(part("SM16269S (0x0214) — NO VENDOR DATA; stub only"), "SM16269S");
        assert_eq!(part("SM16269 (LEDSetting 2.2.6)"), "SM16269");
        assert_eq!(built(NAME), Some(20_231_227));
        assert_eq!(built("E320_PCB6.0_PWM_FPGA12.52_2024527_FDFP3.0.hex"), None);
        assert_eq!(built("image.hex"), None);
    }

    #[test]
    fn every_manifest_entry_is_ranked_for_a_chip_it_names() {
        // The kinds beyond PWM/Normal are only reached through rule 2.
        let spec = bench_spec();
        let special = manifest()
            .image
            .iter()
            .filter(|i| !i.kind.eq_ignore_ascii_case("PWM") && !i.kind.eq_ignore_ascii_case("Normal"))
            .count();
        assert!(special > 0);
        assert_eq!(select(&spec, e120()).len() + special, manifest().image.len());
    }

    #[test]
    fn fetch_prefers_a_verified_local_copy_and_names_unknown_images() {
        let mut lines = Lines::default();
        fetch_in(&root(), NAME, &mut lines).unwrap();
        assert_eq!(lines.out, [root().join(ARCHIVE).join(NAME).display().to_string()]);
        let e = fetch("missing.hex", &mut lines).unwrap_err().to_string();
        assert!(e.contains("not in config/firmware.toml"), "{e}");
    }
}
