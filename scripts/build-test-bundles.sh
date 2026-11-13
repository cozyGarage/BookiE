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
  all|forgejo|github) groups=() ;;
  *) printf 'Unknown test bundle group: %s\n' "$requested_group" >&2; exit 2 ;;
esac
mkdir -p "$output"
output=$(realpath "$output")
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-8}
export CARGO_PROFILE_TEST_STRIP="${CARGO_PROFILE_TEST_STRIP:-symbols}"
nextest="$(bash scripts/ensure-nextest.sh)"
build_provider_bundles() {
  cargo metadata --manifest-path apps/gpui/Cargo.toml --locked --format-version 1 >/dev/null
  local config="$ROOT/scripts/ci-test-bundle-groups.json"
  build_messages="$output/default-build.jsonl"
  if [[ ! -f "$build_messages" ]]; then
    cargo test --locked --workspace --lib --bins --tests --no-run --message-format=json-render-diagnostics > "$build_messages"
  fi
  if [[ "$requested_group" == all ]]; then
    mapfile -t selected_groups < <(python3 -c 'import json,sys; print(*json.load(open(sys.argv[1]))["groups"], sep="\n")' "$config")
  else
    mapfile -t selected_groups < <(python3 -c 'import json,sys; print(*json.load(open(sys.argv[1]))["providers"][sys.argv[2]], sep="\n")' "$config" "$requested_group")
  fi
  for group in "${selected_groups[@]}"; do
    local args=(--messages "$build_messages" --group "$group")
    while IFS= read -r package_kind; do args+=(--package-kind "$package_kind"); done < <(
      python3 -c 'import json,sys; group=json.load(open(sys.argv[1]))["groups"][sys.argv[2]]; print(*(f"{package}:{kind}" for kind,packages in group.items() for package in packages), sep="\n")' "$config" "$group"
    )
    python3 scripts/ci-test-bundle.py create "$output/$group" "$revision" "${args[@]}"
  done
  "$nextest" nextest archive --locked --workspace --lib --bins --tests \
    --archive-file "$output/workspace-nextest.tar.zst"
  python3 scripts/ci-test-bundle.py create "$output/workspace" "$revision" --messages "$build_messages" \
    --group workspace --archive "$output/workspace-nextest.tar.zst" --archive-only
}
for group in "${groups[@]}"; do
  selection=(--workspace)
  manifest=()
  if [[ "$group" == gpui ]]; then
    selection=()
    manifest=(--manifest-path apps/gpui/Cargo.toml)
  fi
  cargo test --locked "${manifest[@]}" "${selection[@]}" --lib --bins --tests --no-run --message-format=json-render-diagnostics > "$output/$group-build.jsonl"
  "$nextest" nextest archive --locked "${manifest[@]}" "${selection[@]}" --lib --bins --tests --archive-file "$output/$group-nextest.tar.zst"
  args=()
  if [[ "$group" == gpui ]]; then args+=(--archive-only); fi
  python3 scripts/ci-test-bundle.py create "$output/$group" "$revision" --messages "$output/$group-build.jsonl" \
    --group "$group" --archive "$output/$group-nextest.tar.zst" "${args[@]}"
done
if [[ "$requested_group" == forgejo || "$requested_group" == github || "$requested_group" == all ]]; then
  build_provider_bundles
fi
