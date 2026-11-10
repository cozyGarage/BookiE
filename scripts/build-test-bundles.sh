#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
revision=${BOOKIE_SOURCE_COMMIT:-$(git rev-parse HEAD)}
if [[ "$revision" != "$(git rev-parse HEAD)" ]]; then
  echo 'requested bundle revision differs from the checkout' >&2
  exit 1
fi
output=${1:?Usage: build-test-bundles.sh OUTPUT_DIRECTORY}
requested_group=${2:-default}
case "$requested_group" in
  default) groups=(default) ;;
  gpui) groups=(gpui) ;;
  all) groups=(default gpui) ;;
  *) printf 'Unknown test bundle group: %s\n' "$requested_group" >&2; exit 2 ;;
esac
mkdir -p "$output"
output=$(realpath "$output")
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-8}
export CARGO_PROFILE_TEST_STRIP="${CARGO_PROFILE_TEST_STRIP:-symbols}"
for group in "${groups[@]}"; do
  selection=(--workspace)
  manifest=()
  if [[ "$group" == gpui ]]; then
    selection=()
    manifest=(--manifest-path apps/gpui/Cargo.toml)
  fi
  cargo test --locked "${manifest[@]}" "${selection[@]}" --lib --bins --tests --no-run --message-format=json > "$output/$group-build.jsonl"
  nextest="$(bash scripts/ensure-nextest.sh)"
  "$nextest" nextest archive --locked "${manifest[@]}" "${selection[@]}" --lib --bins --tests --archive-file "$output/$group-nextest.tar.zst"
  python3 scripts/ci-test-bundle.py create "$output/$group" "$revision" --messages "$output/$group-build.jsonl" --group "$group" --archive "$output/$group-nextest.tar.zst"
done
