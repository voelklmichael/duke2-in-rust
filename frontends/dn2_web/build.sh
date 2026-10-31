#!/bin/sh
# Builds the page into www/: the wasm and its JavaScript bindings in www/pkg.
# Serve www/ over HTTP, e.g. `python3 -m http.server -d www`; game files in
# www/data/ are used without asking, otherwise the page asks for them (a zip
# of the game folder will do). GitHub Pages gets www/ from
# .github/workflows/pages.yml.
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown -p dn2_web
wasm-bindgen --target web --no-typescript --out-dir www/pkg \
    ../../target/wasm32-unknown-unknown/release/dn2_web.wasm
