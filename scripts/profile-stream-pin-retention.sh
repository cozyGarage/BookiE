#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

target="${CARGO_TARGET_DIR:-$ROOT/target}"
rows="${BOOKIE_PROFILE_STREAM_ROWS:-34000}"
cell_bytes="${BOOKIE_PROFILE_STREAM_CELL_BYTES:-2048}"
repetitions="${BOOKIE_PROFILE_REPETITIONS:-3}"
out="${BOOKIE_PROFILE_OUT:-/tmp/bookie-stream-pin-retention-$(date +%Y%m%d-%H%M%S).jsonl}"

cargo build --release --locked -p bookie-app --bin bookie-app
install -Dm755 "$target/release/bookie-app" "$target/installed/usr/bin/bookie"
binary_sha256="$(sha256sum "$target/installed/usr/bin/bookie" | awk '{print $1}')"
source_revision="$(git rev-parse HEAD)"
postgres_image="$(docker image inspect postgres:17-alpine --format '{{.Id}}')"
mkdir -p "$(dirname "$out")"
printf '{"profile_mode":"metadata","binary_sha256":"%s","source_revision":"%s","postgres_image":"%s"}\n' \
  "$binary_sha256" "$source_revision" "$postgres_image" > "$out"

for repetition in $(seq 1 "$repetitions"); do
  BOOKIE_PROFILE_STREAM_PINS=1 \
  BOOKIE_PROFILE_STREAM_ROWS="$rows" \
  BOOKIE_PROFILE_STREAM_CELL_BYTES="$cell_bytes" \
  BOOKIE_PROFILE_REPETITION="$repetition" \
  BOOKIE_PROFILE_OUT="$out" \
  BOOKIE_GTK_SCENARIO=profile_streamed_pin_retention \
  BOOKIE_GTK_BINARY="$target/installed/usr/bin/bookie" \
    bash scripts/test-gtk-postgres.sh
done

cat "$out"
