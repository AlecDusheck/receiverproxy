#!/bin/sh
# Build crates/e120-wasm and emit the JS glue into web/src/wasm.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
cargo build -p e120-wasm --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir web/src/wasm \
  target/wasm32-unknown-unknown/release/e120_wasm.wasm
