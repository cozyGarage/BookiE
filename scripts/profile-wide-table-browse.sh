#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

target="${CARGO_TARGET_DIR:-$ROOT/target}"
rows="${TABLEPRO_PROFILE_ROWS:-1000}"
columns="${TABLEPRO_PROFILE_COLUMNS:-256}"
repetitions="${TABLEPRO_PROFILE_REPETITIONS:-3}"
out="${TABLEPRO_PROFILE_OUT:-$target/quality/wide-table-browse.jsonl}"
screenshot_prefix="${TABLEPRO_PROFILE_SCREENSHOT_PREFIX:-}"

cargo build --release --locked -p tablepro-app --bin tablepro-app
install -Dm755 "$target/release/tablepro-app" "$target/installed/usr/bin/tablepro"
mkdir -p "$(dirname "$out")"
: > "$out"

for repetition in $(seq 1 "$repetitions"); do
  TABLEPRO_PROFILE_ROWS="$rows" \
  TABLEPRO_PROFILE_COLUMNS="$columns" \
  TABLEPRO_GTK_SCENARIO=profile_wide_table_browse \
  TABLEPRO_PROFILE_OUT="$out" \
  TABLEPRO_GTK_BINARY="$target/installed/usr/bin/tablepro" \
  TABLEPRO_PROFILE_REPETITION="$repetition" \
  TABLEPRO_PROFILE_SCREENSHOT="${screenshot_prefix:+${screenshot_prefix}-${repetition}.png}" \
    bash scripts/test-gtk-safety.sh
done

cat "$out"
