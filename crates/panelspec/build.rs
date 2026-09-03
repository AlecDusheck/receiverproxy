//! Embeds `config/chips/**/*.toml` and `config/panels/**/*.toml` as
//! `(path, text)` pairs, path relative to the crate root. `status =
//! "verified"` files first, then the rest, each alphabetical (`embedded` in
//! lib.rs).
//!
//! `crates/panelspec/config/{chips,panels}` are symlinks to the repository's
//! `config/`: one copy of the files, and `cargo package` follows them, so a
//! published crate carries the libraries it embeds.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    // Not canonicalized: the paths below stay under the crate root, so `rel`
    // keeps producing `config/chips/...` keys.
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let mut src = String::new();
    for (name, dir) in [("CHIPS", "config/chips"), ("PANELS", "config/panels")] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
        let mut files = Vec::new();
        collect(&root.join(dir), &mut files);
        // Verified first: the ordering the gallery, the library pickers and
        // `chip_by_family` read. Matched on the text rather than parsed, so
        // the build script needs no TOML dependency.
        files.sort_by_key(|p| (!is_verified(p), rel(&root, p)));
        let _ = writeln!(src, "pub static {name}: &[(&str, &str)] = &[");
        for p in &files {
            println!("cargo:rerun-if-changed={}", p.display());
            let _ = writeln!(
                src,
                "    ({:?}, include_str!({:?})),",
                rel(&root, p),
                p.display().to_string()
            );
        }
        src.push_str("];\n");
    }
    std::fs::write(out.join("libraries.rs"), src).expect("write libraries.rs");
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "toml") {
            out.push(p);
        }
    }
}

fn is_verified(p: &Path) -> bool {
    std::fs::read_to_string(p)
        .is_ok_and(|t| t.lines().any(|l| l.trim() == r#"status = "verified""#))
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}
