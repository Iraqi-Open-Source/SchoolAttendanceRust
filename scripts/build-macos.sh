#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v rustup >/dev/null; then echo 'Install Rust via https://rustup.rs first'; exit 1; fi
rustup toolchain install 1.77.2 --profile minimal
rustup toolchain install stable --profile minimal
if ! cargo tauri --version 2>/dev/null | grep -Eq '1\.[0-9]+'; then
  cargo +stable install tauri-cli --version 1.6.3 --locked
fi
cargo +1.77.2 tauri build
printf '\nGenerated app/bundles: src-tauri/target/release/bundle/\n'
