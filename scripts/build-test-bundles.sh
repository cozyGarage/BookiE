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
  all) groups=() ;;
  forgejo) groups=() ;;
  *) printf 'Unknown test bundle group: %s\n' "$requested_group" >&2; exit 2 ;;
esac
mkdir -p "$output"
output=$(realpath "$output")
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-8}
export CARGO_PROFILE_TEST_STRIP="${CARGO_PROFILE_TEST_STRIP:-symbols}"
nextest="$(bash scripts/ensure-nextest.sh)"
build_forgejo_bundles() {
  sandbox_packages=(
    bookie-core bookie-policy bookie-storage bookie-transport bookie-app-services bookie-results
    bookie-ssh bookie-dump bookie-mcp bookie-agentd bookie-driver-sqlite
  )
  package_args=()
  for package in "${sandbox_packages[@]}"; do package_args+=(-p "$package"); done
  sandbox_package_args=()
  for package in "${sandbox_packages[@]}"; do sandbox_package_args+=(--package "$package"); done
  build_messages="$output/default-build.jsonl"
  if [[ ! -f "$build_messages" ]]; then
    cargo test --locked --workspace --lib --bins --tests --no-run --message-format=json-render-diagnostics > "$build_messages"
  fi
  python3 scripts/ci-test-bundle.py create "$output/direct" "$revision" --messages "$build_messages" \
    --group direct --test-kind test \
    --package-kind bookie-app:lib --package-kind bookie-storage:lib --package-kind bookie-transport:lib
  "$nextest" nextest archive --locked --workspace --lib --bins --archive-file "$output/unit-nextest.tar.zst"
  python3 scripts/ci-test-bundle.py create "$output/unit" "$revision" --messages "$build_messages" \
    --group unit --archive "$output/unit-nextest.tar.zst" --archive-only --test-kind lib --test-kind bin
  "$nextest" nextest archive --locked "${package_args[@]}" --test '*' --archive-file "$output/sandbox-nextest.tar.zst"
  python3 scripts/ci-test-bundle.py create "$output/sandbox" "$revision" --messages "$build_messages" \
    --group sandbox --archive "$output/sandbox-nextest.tar.zst" --archive-only --test-kind test \
    "${sandbox_package_args[@]}"
  cargo test --locked --manifest-path apps/gpui/Cargo.toml --lib --bins --tests --no-run \
    --message-format=json-render-diagnostics > "$output/gpui-build.jsonl"
  "$nextest" nextest archive --locked --manifest-path apps/gpui/Cargo.toml --lib --bins --tests \
    --archive-file "$output/gpui-nextest.tar.zst"
  python3 scripts/ci-test-bundle.py create "$output/gpui" "$revision" --messages "$output/gpui-build.jsonl" \
    --group gpui --archive "$output/gpui-nextest.tar.zst" --archive-only
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
if [[ "$requested_group" == forgejo || "$requested_group" == all ]]; then build_forgejo_bundles; fi
