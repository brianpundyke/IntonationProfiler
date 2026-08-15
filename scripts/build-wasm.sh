#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -p ip-wasm --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/ip_wasm.wasm \
  --out-dir web/pkg \
  --target web \
  --no-typescript
