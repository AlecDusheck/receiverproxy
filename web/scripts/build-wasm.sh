#!/bin/sh
# Build crates/rcvbp-wasm and emit the JS glue into web/src/wasm.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
cargo build -p rcvbp-wasm --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir web/src/wasm \
  target/wasm32-unknown-unknown/release/rcvbp_wasm.wasm
