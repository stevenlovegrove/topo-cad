#!/bin/sh
# Builds the topo-cad engine for the browser into web/pkg (the rest of web/
# is plain files). Needs: rustup target add wasm32-unknown-unknown, and
# wasm-bindgen-cli matching the wasm-bindgen crate version (see Cargo.lock);
# wasm-opt (binaryen) is used if present to shrink the module.
set -e
cd "$(dirname "$0")/.."
cargo build -p topo-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir web/pkg target/wasm32-unknown-unknown/release/topo_wasm.wasm
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -O3 --all-features web/pkg/topo_wasm_bg.wasm -o web/pkg/topo_wasm_bg.wasm
fi
ls -l web/pkg
