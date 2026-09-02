//! Embeds `config/chips/**/*.toml` and `config/panels/**/*.toml` as
//! `(path, text)` pairs, path relative to the repository root. Non-mined
//! files first, then `mined/`, each alphabetical (`embedded` in lib.rs).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap_or(root);
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let mut src = String::new();
    for (name, dir) in [("CHIPS", "config/chips"), ("PANELS", "config/panels")] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
        let mut files = Vec::new();
        collect(&root.join(dir), &mut files);
        files.sort_by_key(|p| (rel(&root, p).contains("/mined/"), rel(&root, p)));
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

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}
