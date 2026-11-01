#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

target="${CARGO_TARGET_DIR:-$ROOT/target}"
rows="${TABLEPRO_PROFILE_ROWS:-100000}"
repetitions="${TABLEPRO_PROFILE_REPETITIONS:-3}"
out="${TABLEPRO_PROFILE_OUT:-$ROOT/target/quality/grid-scroll-matrix-$(date +%Y%m%d-%H%M%S).jsonl}"
full_matrix="${TABLEPRO_PROFILE_MATRIX_FULL:-0}"
include_scrub_reference="${TABLEPRO_PROFILE_INCLUDE_SCRUB_REFERENCE:-0}"
wheel_events="${TABLEPRO_PROFILE_WHEEL_EVENTS:-40}"
wheel_delay="${TABLEPRO_PROFILE_WHEEL_DELAY_SECONDS:-0.08}"
scroll_steps="${TABLEPRO_PROFILE_SCROLL_STEPS:-1000}"

if [[ ! "$rows" =~ ^[0-9]+$ || ! "$repetitions" =~ ^[1-9][0-9]*$ ]]; then
  echo "rows and repetitions must be positive integers" >&2
  exit 2
fi
if [[ "$rows" == "0" ]]; then
  echo "rows and repetitions must be positive integers" >&2
  exit 2
fi
if [[ ! "$full_matrix" =~ ^[01]$ || ! "$include_scrub_reference" =~ ^[01]$ ]]; then
  echo "profile matrix switches must be 0 or 1" >&2
  exit 2
fi

cargo build --release --locked -p tablepro-app --bin tablepro-app
binary="$target/installed/usr/bin/tablepro"
install -Dm755 "$target/release/tablepro-app" "$binary"
source_revision="$(git rev-parse HEAD)"
binary_sha256="$(sha256sum "$binary" | cut -d' ' -f1)"
mkdir -p "$(dirname "$out")"
: > "$out"

run_case() {
  local text_mode="$1"
  local layout="$2"
  local input_mode="$3"
  local repetition="$4"
  printf 'profile text=%s layout=%s input=%s repetition=%s rows=%s\n' \
    "$text_mode" "$layout" "$input_mode" "$repetition" "$rows"
  TABLEPRO_PROFILE_ROWS="$rows" \
  TABLEPRO_PROFILE_TEXT_MODE="$text_mode" \
  TABLEPRO_PROFILE_LAYOUT="$layout" \
  TABLEPRO_PROFILE_INPUT="$input_mode" \
  TABLEPRO_PROFILE_WHEEL_EVENTS="$wheel_events" \
  TABLEPRO_PROFILE_WHEEL_DELAY_SECONDS="$wheel_delay" \
  TABLEPRO_PROFILE_SCROLL_STEPS="$scroll_steps" \
  TABLEPRO_PROFILE_REPETITION="$repetition" \
  TABLEPRO_PROFILE_OUT="$out" \
  TABLEPRO_PROFILE_SOURCE_REVISION="$source_revision" \
  TABLEPRO_PROFILE_BINARY_SHA256="$binary_sha256" \
  TABLEPRO_GTK_SCENARIO=profile_large_result_in_the_grid \
  TABLEPRO_GTK_BINARY="$binary" \
    bash scripts/test-gtk-safety.sh
}

for text_mode in unique repeated; do
  for layout in narrow wide; do
    for repetition in $(seq 1 "$repetitions"); do
      run_case "$text_mode" "$layout" wheel "$repetition"
    done
  done
done

if [[ "$include_scrub_reference" == "1" ]]; then
  for repetition in $(seq 1 "$repetitions"); do
    run_case unique narrow scrub "$repetition"
  done
fi

if [[ "$full_matrix" == "1" ]]; then
  for text_mode in unique repeated; do
    for layout in narrow wide; do
      for repetition in $(seq 1 "$repetitions"); do
        run_case "$text_mode" "$layout" scrub "$repetition"
      done
    done
  done
fi

cat "$out"
