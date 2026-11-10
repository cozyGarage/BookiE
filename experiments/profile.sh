#!/usr/bin/env bash
# Builds both prototypes in release mode and measures scroll time and memory with the same row count and steps.
# Run it from a desktop session: the GPUI window needs a real display. ROWS, STEPS and RUNS override the defaults.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
rows="${ROWS:-150000}" steps="${STEPS:-120}" runs="${RUNS:-3}"
tauri_manifest=experiments/tauri-grid-prototype/src-tauri/Cargo.toml
gpui_manifest=apps/gpui/Cargo.toml
target_of() { cargo metadata --no-deps --format-version 1 --manifest-path "$1" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])'; }
cargo build --release --locked --manifest-path "$tauri_manifest"
cargo build --release --locked --manifest-path "$gpui_manifest"
echo "== Tauri (WebKitGTK): rows=$rows steps=$steps runs=$runs"
node experiments/tauri-grid-prototype/measure.mjs --rows="$rows" --steps="$steps" --runs="$runs" \
  --binary="$(target_of "$tauri_manifest")/release/bookie-tauri-grid-prototype"
gpui_binary="$(target_of "$gpui_manifest")/release/bookie-gpui"
echo "== GPUI, timer pacing (same cadence as Tauri)"
node apps/gpui/measure.mjs --rows="$rows" --steps="$steps" --runs="$runs" --binary="$gpui_binary"
echo "== GPUI, frame pacing (real frame times)"
node apps/gpui/measure.mjs --rows="$rows" --steps="$steps" --runs="$runs" --pacing=frame --binary="$gpui_binary"
