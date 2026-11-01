#!/usr/bin/env bash
# Profile RowStore retention while GtkColumnView is scrolled through results.
# Run from anywhere with the local GTK/AT-SPI prerequisites installed:
#   bash scripts/profile-row-retention.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

target="${CARGO_TARGET_DIR:-$ROOT/target}"
rows_list="${BOOKIE_PROFILE_ROWS_LIST:-10000 100000 300000}"
repetitions="${BOOKIE_PROFILE_REPETITIONS:-3}"
scroll_steps="${BOOKIE_PROFILE_SCROLL_STEPS:-1000}"
cell_bytes="${BOOKIE_PROFILE_CELL_BYTES:-0}"
out="${BOOKIE_PROFILE_OUT:-/tmp/bookie-row-retention-$(date +%Y%m%d-%H%M%S).jsonl}"

cargo build --release --locked -p bookie-app --bin bookie-app
install -Dm755 "$target/release/bookie-app" "$target/installed/usr/bin/bookie"
: > "$out"

for rows in $rows_list; do
  for repetition in $(seq 1 "$repetitions"); do
    BOOKIE_PROFILE_ROWS="$rows" \
    BOOKIE_PROFILE_REPETITION="$repetition" \
    BOOKIE_PROFILE_SCROLL_STEPS="$scroll_steps" \
    BOOKIE_PROFILE_CELL_BYTES="$cell_bytes" \
    BOOKIE_PROFILE_OUT="$out" \
    BOOKIE_GTK_SCENARIO=profile_large_result_in_the_grid \
    BOOKIE_GTK_BINARY="$target/installed/usr/bin/bookie" \
      bash scripts/test-gtk-safety.sh
  done
done

cat "$out"
