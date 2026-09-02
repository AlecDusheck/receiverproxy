//! Sets `cfg(web_dist)` when the embedded web build exists
//! (`cd web && pnpm build:embed` writes `web/build-static`), so `assets.rs`
//! can embed it. Rebuilds when that directory changes.

use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(web_dist)");
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/build-static");
    println!("cargo::rerun-if-changed={}", dist.display());
    if dist.join("index.html").is_file() {
        println!("cargo::rustc-cfg=web_dist");
    }
}
