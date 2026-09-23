#!/usr/bin/env bash
# Prueft, dass Rust und WebAssembly dieselbe Bewegung rechnen.
#
# Der ganze Sinn des WASM-Umwegs ist, dass es nur *eine* Bewegung gibt. Laufen
# die beiden Fassungen auseinander, ist die Grundannahme verletzt - und zwar
# still: der Client wuerde einfach an anderer Stelle stehen als der Server ihn
# sieht.
#
# Geprueft werden zwei Module: das frisch uebersetzte und das eingecheckte
# `client/vendor/predict.wasm`. Das zweite liefert `cargo run` aus; blieb es
# nach einer Protokollaenderung liegen, lehnte es die Konfiguration ab und die
# Vorhersage schaltete sich still ab. Genau das ist einmal passiert.
#
#     scripts/gleichlauf.sh
set -euo pipefail

cd "$(dirname "$0")/.."
ARBEIT="$(mktemp -d "${TMPDIR:-/tmp}/corpshoot-gleichlauf.XXXXXX")"
trap 'rm -rf "$ARBEIT"' EXIT
WASM=target/wasm32-unknown-unknown/wasm/predict.wasm
EINGECHECKT=client/vendor/predict.wasm

command -v node >/dev/null || { echo "node wird gebraucht"; exit 1; }

echo "1/5  Karte und Konfiguration ausgeben"
cargo run --quiet --locked -p server -- --dump-map "$ARBEIT" >/dev/null

echo "2/5  WASM uebersetzen"
rustup target list --installed | grep -q wasm32-unknown-unknown \
  || rustup target add wasm32-unknown-unknown
cargo build --quiet --locked -p predict --target wasm32-unknown-unknown --profile wasm

echo "3/5  Bahn in Rust aufzeichnen"
PREDICT_FIXTURES="$ARBEIT" cargo test --quiet --locked -p predict --test gleichlauf >/dev/null

echo "4/5  Dieselben Eingaben durch das frisch uebersetzte Modul"
node crates/predict/tests/gleichlauf.mjs "$ARBEIT" "$WASM"

echo "5/5  Dieselben Eingaben durch das eingecheckte Modul"
if ! node crates/predict/tests/gleichlauf.mjs "$ARBEIT" "$EINGECHECKT"; then
  echo
  echo "Das eingecheckte $EINGECHECKT passt nicht mehr zum Quelltext."
  echo "Neu uebersetzen und einchecken:"
  echo
  echo "    cargo build -p predict --target wasm32-unknown-unknown --profile wasm"
  echo "    cp $WASM $EINGECHECKT"
  exit 1
fi
