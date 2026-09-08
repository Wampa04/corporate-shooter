#!/usr/bin/env bash
# Prueft, dass Rust und WebAssembly dieselbe Bewegung rechnen.
#
# Der ganze Sinn des WASM-Umwegs ist, dass es nur *eine* Bewegung gibt. Laufen
# die beiden Fassungen auseinander, ist die Grundannahme verletzt - und zwar
# still: der Client wuerde einfach an anderer Stelle stehen als der Server ihn
# sieht.
#
#     scripts/gleichlauf.sh
set -euo pipefail

cd "$(dirname "$0")/.."
ARBEIT="${TMPDIR:-/tmp}/corpshoot-gleichlauf"
WASM=target/wasm32-unknown-unknown/wasm/predict.wasm

command -v node >/dev/null || { echo "node wird gebraucht"; exit 1; }

echo "1/4  Karte und Konfiguration ausgeben"
mkdir -p "$ARBEIT"
cargo run --quiet -p server -- --dump-map "$ARBEIT" >/dev/null

echo "2/4  WASM uebersetzen"
rustup target list --installed | grep -q wasm32-unknown-unknown \
  || rustup target add wasm32-unknown-unknown
cargo build --quiet -p predict --target wasm32-unknown-unknown --profile wasm

echo "3/4  Bahn in Rust aufzeichnen"
PREDICT_FIXTURES="$ARBEIT" cargo test --quiet -p predict --test gleichlauf >/dev/null

echo "4/4  Dieselben Eingaben durch das Modul"
node crates/predict/tests/gleichlauf.mjs "$ARBEIT" "$WASM"
