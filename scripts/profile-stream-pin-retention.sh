#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

target="${CARGO_TARGET_DIR:-$ROOT/target}"
rows="${TABLEPRO_PROFILE_STREAM_ROWS:-34000}"
cell_bytes="${TABLEPRO_PROFILE_STREAM_CELL_BYTES:-2048}"
repetitions="${TABLEPRO_PROFILE_REPETITIONS:-3}"
out="${TABLEPRO_PROFILE_OUT:-/tmp/bookie-stream-pin-retention-$(date +%Y%m%d-%H%M%S).jsonl}"

cargo build --release --locked -p tablepro-app --bin tablepro-app
install -Dm755 "$target/release/tablepro-app" "$target/installed/usr/bin/tablepro"
binary_sha256="$(sha256sum "$target/installed/usr/bin/tablepro" | awk '{print $1}')"
source_revision="$(git rev-parse HEAD)"
postgres_image="$(docker image inspect postgres:17-alpine --format '{{.Id}}')"
mkdir -p "$(dirname "$out")"
printf '{"profile_mode":"metadata","binary_sha256":"%s","source_revision":"%s","postgres_image":"%s"}\n' \
  "$binary_sha256" "$source_revision" "$postgres_image" > "$out"

for repetition in $(seq 1 "$repetitions"); do
  TABLEPRO_PROFILE_STREAM_PINS=1 \
  TABLEPRO_PROFILE_STREAM_ROWS="$rows" \
  TABLEPRO_PROFILE_STREAM_CELL_BYTES="$cell_bytes" \
  TABLEPRO_PROFILE_REPETITION="$repetition" \
  TABLEPRO_PROFILE_OUT="$out" \
  TABLEPRO_GTK_SCENARIO=profile_streamed_pin_retention \
  TABLEPRO_GTK_BINARY="$target/installed/usr/bin/tablepro" \
    bash scripts/test-gtk-postgres.sh
done

cat "$out"
