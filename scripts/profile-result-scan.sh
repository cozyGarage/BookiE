#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

rows="${BOOKIE_PROFILE_ROWS:-500000}"
payload="${BOOKIE_PROFILE_PAYLOAD_BYTES:-200}"
repetitions="${BOOKIE_PROFILE_REPETITIONS:-3}"
out="${BOOKIE_PROFILE_OUT:-/tmp/bookie-result-scan-$(date +%Y%m%d-%H%M%S).jsonl}"

cargo build --release --locked -p bookie-app-services --example result_scan_profile
binary="${CARGO_TARGET_DIR:-$ROOT/target}/release/examples/result_scan_profile"
mkdir -p "$(dirname "$out")"
printf '{"profile":"result_scan","source_revision":"%s","binary_sha256":"%s","rows":%s,"payload_bytes":%s,"spool_dir":"%s"}\n' \
  "$(git rev-parse HEAD)" "$(sha256sum "$binary" | awk '{print $1}')" "$rows" "$payload" "${BOOKIE_SPOOL_DIR:-default}" > "$out"

for repetition in $(seq 1 "$repetitions"); do
  BOOKIE_PROFILE_ROWS="$rows" BOOKIE_PROFILE_PAYLOAD_BYTES="$payload" "$binary" \
    | sed "s/^PROFILE /{\"repetition\":$repetition,\"data\":/; s/\$/}/" >> "$out"
done
printf '%s\n' "$out"
