#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

target="${CARGO_TARGET_DIR:-$ROOT/target}"
rows="${BOOKIE_PROFILE_EXPORT_ROWS:-39999}"
repetitions="${BOOKIE_PROFILE_REPETITIONS:-3}"
cell_bytes="${BOOKIE_PROFILE_EXPORT_CELL_BYTES:-2048}"
text_mode="${BOOKIE_PROFILE_EXPORT_TEXT_MODE:-unique}"
out="${BOOKIE_PROFILE_OUT:-$ROOT/target/quality/grid-export-dialog-${rows}-rows.jsonl}"

if [[ ! "$rows" =~ ^[1-9][0-9]*$ || ! "$repetitions" =~ ^[1-9][0-9]*$ ]]; then
  echo "rows and repetitions must be positive integers" >&2
  exit 2
fi
if [[ ! "$cell_bytes" =~ ^[1-9][0-9]*$ || "$cell_bytes" -gt 8192 ]]; then
  echo "cell bytes must be between 1 and 8192" >&2
  exit 2
fi
if (( rows * cell_bytes > 268435456 )); then
  echo "synthetic text payload exceeds the 256 MiB bound" >&2
  exit 2
fi
if [[ "$text_mode" != unique && "$text_mode" != repeated ]]; then
  echo "text mode must be unique or repeated" >&2
  exit 2
fi

cargo build --release --locked -p bookie-app --bin bookie-app
binary="$target/installed/usr/bin/bookie"
install -Dm755 "$target/release/bookie-app" "$binary"
source_revision="$(git rev-parse HEAD)"
binary_sha256="$(sha256sum "$binary" | cut -d' ' -f1)"
mkdir -p "$(dirname "$out")"
: > "$out"

for repetition in $(seq 1 "$repetitions"); do
  printf 'profile export-dialog rows=%s cell-bytes=%s text=%s repetition=%s\n' \
    "$rows" "$cell_bytes" "$text_mode" "$repetition"
  BOOKIE_PROFILE_EXPORT_ROWS="$rows" \
  BOOKIE_PROFILE_EXPORT_CELL_BYTES="$cell_bytes" \
  BOOKIE_PROFILE_EXPORT_TEXT_MODE="$text_mode" \
  BOOKIE_PROFILE_REPETITION="$repetition" \
  BOOKIE_PROFILE_OUT="$out" \
  BOOKIE_PROFILE_SOURCE_REVISION="$source_revision" \
  BOOKIE_PROFILE_BINARY_SHA256="$binary_sha256" \
  BOOKIE_GTK_SCENARIO=profile_large_result_export_dialog \
  BOOKIE_GTK_BINARY="$binary" \
    bash scripts/test-gtk-safety.sh
done

cat "$out"
