#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
revision=${BOOKIE_SOURCE_COMMIT:-$(git rev-parse HEAD)}
output=${1:?Usage: build-test-bundles.sh OUTPUT_DIRECTORY}
requested_group=${2:-both}
case "$requested_group" in
  both) groups=(default duckdb) ;;
  default|duckdb) groups=("$requested_group") ;;
  *) printf 'Unknown test bundle group: %s\n' "$requested_group" >&2; exit 2 ;;
esac
mkdir -p "$output"
output=$(realpath "$output")
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-8}
export CARGO_PROFILE_TEST_STRIP="${CARGO_PROFILE_TEST_STRIP:-debuginfo}"
for group in "${groups[@]}"; do
  selection=(--workspace --exclude tablepro-driver-duckdb)
  if [[ "$group" == duckdb ]]; then
    selection=(-p tablepro-driver-duckdb -p tablepro-app --features tablepro-app/duckdb)
  fi
  cargo test --locked "${selection[@]}" --lib --bins --tests --no-run --message-format=json > "$output/$group-build.jsonl"
  nextest="$(bash scripts/ensure-nextest.sh)"
  "$nextest" nextest archive --locked "${selection[@]}" --lib --bins --tests --archive-file "$output/$group-nextest.tar.zst"
  python3 scripts/ci-test-bundle.py create "$output/$group" "$revision" --messages "$output/$group-build.jsonl" --group "$group" --archive "$output/$group-nextest.tar.zst"
done
