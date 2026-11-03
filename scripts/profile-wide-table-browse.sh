#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

target="${CARGO_TARGET_DIR:-$ROOT/target}"
rows="${BOOKIE_PROFILE_ROWS:-1000}"
columns="${BOOKIE_PROFILE_COLUMNS:-256}"
repetitions="${BOOKIE_PROFILE_REPETITIONS:-3}"
out="${BOOKIE_PROFILE_OUT:-$target/quality/wide-table-browse.jsonl}"
screenshot_prefix="${BOOKIE_PROFILE_SCREENSHOT_PREFIX:-}"

cargo build --release --locked -p bookie-app --bin bookie-app
install -Dm755 "$target/release/bookie-app" "$target/installed/usr/bin/bookie"
mkdir -p "$(dirname "$out")"
: > "$out"

for repetition in $(seq 1 "$repetitions"); do
  BOOKIE_PROFILE_ROWS="$rows" \
  BOOKIE_PROFILE_COLUMNS="$columns" \
  BOOKIE_GTK_SCENARIO=profile_wide_table_browse \
  BOOKIE_PROFILE_OUT="$out" \
  BOOKIE_GTK_BINARY="$target/installed/usr/bin/bookie" \
  BOOKIE_PROFILE_REPETITION="$repetition" \
  BOOKIE_PROFILE_SCREENSHOT="${screenshot_prefix:+${screenshot_prefix}-${repetition}.png}" \
    bash scripts/test-gtk-safety.sh
done

cat "$out"
