#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

target="${CARGO_TARGET_DIR:-$ROOT/target}"
columns="${BOOKIE_PROFILE_COLUMNS:-512}"
rows="${BOOKIE_PROFILE_ROWS:-100}"
binary="${BOOKIE_GTK_BINARY:-$target/release/bookie-app}"
output="${BOOKIE_PROFILE_HEAPTRACK_OUT:-$target/quality/wide-table-${rows}x${columns}.heaptrack.zst}"
raw_output="${TMPDIR:-/tmp}/bookie-wide-table-heaptrack-$$.raw"
preload="/usr/lib/heaptrack/libheaptrack_preload.so"
interpreter="/usr/lib/heaptrack/libexec/heaptrack_interpret"

for executable in "$preload" "$interpreter"; do
  if [[ ! -x "$executable" ]]; then
    echo "missing required Heaptrack component: $executable" >&2
    exit 1
  fi
done

if [[ ! -x "$binary" ]]; then
  if [[ -n "${BOOKIE_GTK_BINARY:-}" ]]; then
    echo "profile binary is not executable: $binary" >&2
    exit 1
  fi
  cargo build --release --locked -p bookie-app --bin bookie-app
fi
install -Dm755 "$binary" "$target/installed-heaptrack/usr/bin/bookie"
binary="$target/installed-heaptrack/usr/bin/bookie"

runtime="$(mktemp -d "${TMPDIR:-/tmp}/bookie-heaptrack.XXXXXX")"
fifo="$runtime/heaptrack.fifo"
mkdir -p "$target/quality"
mkdir -p "$(dirname "$output")"
mkfifo "$fifo"

cleanup() {
  local status=$?
  if [[ -n "${interpreter_pid:-}" ]]; then
    kill "$interpreter_pid" 2>/dev/null || true
    wait "$interpreter_pid" 2>/dev/null || true
  fi
  rm -f -- "$raw_output"
  rm -rf -- "$runtime"
  exit "$status"
}
trap cleanup EXIT

"$interpreter" < "$fifo" > "$raw_output" &
interpreter_pid=$!

BOOKIE_GTK_SCENARIO=profile_wide_table_browse \
BOOKIE_PROFILE_ROWS="$rows" \
BOOKIE_PROFILE_COLUMNS="$columns" \
BOOKIE_PROFILE_OUT="$target/quality/wide-table-${rows}x${columns}-heaptrack.jsonl" \
BOOKIE_GTK_BINARY="$binary" \
BOOKIE_GTK_APP_LD_PRELOAD="$preload" \
BOOKIE_GTK_APP_HEAPTRACK_OUTPUT="$fifo" \
  bash scripts/test-gtk-safety.sh

wait "$interpreter_pid"
interpreter_pid=""
zstd -T0 -f "$raw_output" -o "$output"
heaptrack_print "$output" > "${output%.zst}-report.txt"
cat "$target/quality/wide-table-${rows}x${columns}-heaptrack.jsonl"
