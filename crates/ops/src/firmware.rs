//! Firmware images by manifest name or path.
//!
//! `config/firmware.toml` (`receivers::firmware`) lists the archived images
//! with their sha256; a name from it is looked for under
//! `third-party/firmware` and then in the config directory's cache, and every
//! image that is in the manifest is checked against it before a write.

use crate::util::warn;
use crate::Progress;
use anyhow::{bail, Context, Result};
use receivers::firmware::{image as entry, manifest, Image};
use std::path::{Path, PathBuf};

/// The archived images, relative to the repository root.
pub const ARCHIVE: &str = "third-party/firmware";

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

/// `rxp firmware fetch NAME`: download `base_url/NAME` with `curl` into the
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
    let url = format!("{}/{name}", m.base_url.trim_end_matches('/'));
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

    #[test]
    fn fetch_prefers_a_verified_local_copy_and_names_unknown_images() {
        let mut lines = Lines::default();
        fetch_in(&root(), NAME, &mut lines).unwrap();
        assert_eq!(lines.out, [root().join(ARCHIVE).join(NAME).display().to_string()]);
        let e = fetch("missing.hex", &mut lines).unwrap_err().to_string();
        assert!(e.contains("not in config/firmware.toml"), "{e}");
    }
}
