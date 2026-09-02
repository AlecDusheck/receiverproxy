//! The firmware manifest, `config/firmware.toml`: the vendor images under
//! `third-party/firmware` by name, version, kind, size and sha256. Embedded
//! at build time like the card models.

use crate::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::sync::OnceLock;

const TEXT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/firmware.toml"));

/// The manifest as a whole.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The asset host `fetch` downloads `base_url/path` from; empty means local only.
    pub base_url: String,
    #[serde(default)]
    pub image: Vec<Image>,
}

/// One firmware image.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    /// The file name, and what commands take.
    pub name: String,
    /// Object path under `base_url`: `firmware/<vendor>/<family>/<file>`.
    pub path: String,
    /// What the card reports after the install.
    pub version: Version,
    /// The board revision in the file name; absent when the name carries none.
    pub pcb: Option<String>,
    /// The vendor's build variant: `PWM`, `Normal`, `LS0allDA`.
    pub kind: String,
    /// Driver chips the file name lists.
    pub chips: Vec<String>,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: String,
}

impl Image {
    /// Check `bytes` against the manifest's size and sha256.
    ///
    /// # Errors
    /// Names the field that disagrees, expected and found.
    pub fn verify(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() as u64 != self.size {
            return Err(format!(
                "{}: size {} bytes, manifest says {}",
                self.name,
                bytes.len(),
                self.size
            ));
        }
        let got = sha256_hex(bytes);
        if got != self.sha256 {
            return Err(format!(
                "{}: sha256 {got}, manifest says {}",
                self.name, self.sha256
            ));
        }
        Ok(())
    }
}

/// The sha256 of `bytes` as lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn parse(text: &str) -> Result<Manifest, String> {
    let m: Manifest = toml::from_str(text).map_err(|e| format!("config/firmware.toml: {e}"))?;
    for (i, img) in m.image.iter().enumerate() {
        if img.sha256.len() != 64 || !img.sha256.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(format!("config/firmware.toml: {}: sha256 is not 64 lowercase hex digits", img.name));
        }
        if m.image[..i].iter().any(|o| o.name == img.name) {
            return Err(format!("config/firmware.toml: {}: listed twice", img.name));
        }
    }
    Ok(m)
}

/// The embedded manifest.
///
/// # Panics
/// When `config/firmware.toml` does not parse; the tests catch that first.
pub fn manifest() -> &'static Manifest {
    static MANIFEST: OnceLock<Manifest> = OnceLock::new();
    MANIFEST.get_or_init(|| parse(TEXT).unwrap_or_else(|e| panic!("{e}")))
}

/// The image called `name`, exactly.
#[must_use]
pub fn image(name: &str) -> Option<&'static Image> {
    manifest().image.iter().find(|i| i.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo(rel: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
    }

    #[test]
    fn the_manifest_lists_every_archived_image_and_the_hashes_match() {
        let m = manifest();
        assert!(m.base_url.is_empty() || m.base_url.starts_with("https://"));
        let dir = repo("third-party/firmware");
        // The images are not in the repository; the check runs where a local archive exists.
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("skipped: no local firmware archive at {}", dir.display());
            return;
        };
        let mut on_disk: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| Path::new(n).extension().is_some_and(|x| x == "hex"))
            .collect();
        on_disk.sort();
        let mut listed: Vec<String> = m.image.iter().map(|i| i.name.clone()).collect();
        listed.sort();
        assert_eq!(listed, on_disk);
        for img in &m.image {
            let bytes = std::fs::read(dir.join(&img.name)).expect("read image");
            assert_eq!(img.verify(&bytes), Ok(()), "{}", img.name);
        }
    }

    #[test]
    fn names_resolve_exactly() {
        let img = image("E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex").expect("16.53 listed");
        assert_eq!(img.version, Version(16, 53));
        assert_eq!(img.kind, "PWM");
        assert_eq!(img.chips, ["SM16386S", "SM16269SH"]);
        assert_eq!(img.pcb, None);
        assert_eq!(
            image("E320_PCB6.1_LS0allDA_FPGA6.69_20220907.hex").map(|i| i.pcb.as_deref()),
            Some(Some("6.1"))
        );
        assert!(image("e320_pwm_fpga16.53_20231227_sm16386s_sm16269sh.hex").is_none());
        assert!(image("third-party/firmware/E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex").is_none());
    }

    #[test]
    fn a_wrong_hash_or_size_is_refused() {
        let img = Image {
            name: "synthetic.hex".into(),
            path: "firmware/x/x.hex".into(),
            version: Version(1, 0),
            pcb: None,
            kind: "PWM".into(),
            chips: Vec::new(),
            size: 3,
            sha256: sha256_hex(b"abc"),
        };
        assert_eq!(img.sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(img.verify(b"abc"), Ok(()));
        assert!(img.verify(b"abd").unwrap_err().contains("sha256"));
        assert!(img.verify(b"abcd").unwrap_err().contains("size"));
    }

    #[test]
    fn a_malformed_manifest_is_refused() {
        let bad = "base_url = \"\"\n[[image]]\nname = \"x.hex\"\npath = \"firmware/x/x.hex\"\nversion = \"1.0\"\nkind = \"PWM\"\nchips = []\nsize = 1\nsha256 = \"abc\"\n";
        assert!(parse(bad).unwrap_err().contains("64 lowercase hex"));
        let twice = TEXT.to_string() + "\n[[image]]\nname = \"E320_PCB6.0_PWM_FPGA9.53_20221031.hex\"\npath = \"firmware/x/y.hex\"\nversion = \"9.53\"\nkind = \"PWM\"\nchips = []\nsize = 1\nsha256 = \"cb7c264231d7123bbf3fba4a9ec964a410b20e284db5715e46f50da0eeaffa19\"\n";
        assert!(parse(&twice).unwrap_err().contains("listed twice"));
    }
}
