//! Embeds `config/cards/*.toml` as `(file name, text)` pairs, alphabetical.
//! `crates/receivers/config/` symlinks the repository's `config/`, so the
//! files are inside the package `cargo package` builds.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo")).join("config/cards");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("config/cards exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    let mut src = String::from("pub static FILES: &[(&str, &str)] = &[\n");
    for p in &files {
        println!("cargo:rerun-if-changed={}", p.display());
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 file name");
        let _ = writeln!(
            src,
            "    ({name:?}, include_str!({:?})),",
            p.display().to_string()
        );
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo"));
    std::fs::write(out.join("cards.rs"), src).expect("write cards.rs");
}
