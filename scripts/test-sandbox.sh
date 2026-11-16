#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -f "$ROOT/scripts/dev-env.sh" ]]; then
  # shellcheck source=/dev/null
  source "$ROOT/scripts/dev-env.sh"
fi

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
case "$CARGO_TARGET_DIR" in
  /tmp/*) export CARGO_TARGET_DIR="$ROOT/target" ;;
esac

CRATES=(
  bookie-core
  bookie-policy
  bookie-storage
  bookie-transport
  bookie-app-services
  bookie-results
  bookie-ssh
  bookie-dump
  bookie-mcp
  bookie-agentd
  bookie-driver-sqlite
)

PKG_ARGS=()
for c in "${CRATES[@]}"; do
  PKG_ARGS+=(-p "$c")
done

echo "==> sandbox tier: all integration targets"
NEXTEST="$("$ROOT/scripts/ensure-nextest.sh")"
PROFILE="${BOOKIE_NEXTEST_PROFILE:-default}"
test_bundle="${BOOKIE_SANDBOX_BUNDLE:-${BOOKIE_TEST_BUNDLE:-}}"
if [[ -n "$test_bundle" ]]; then
  mkdir -p "${CARGO_TARGET_DIR:?Cargo target directory required}"
  export TMPDIR="$CARGO_TARGET_DIR"
  expected_group=()
  if [[ -n "${BOOKIE_SANDBOX_BUNDLE:-}" ]]; then expected_group+=(--expected-group workspace); fi
  python3 "$ROOT/scripts/ci-test-bundle.py" verify "$test_bundle" \
    "${BOOKIE_SOURCE_COMMIT:-${GITHUB_SHA:?source revision required}}" "${expected_group[@]}"
  packages=""
  for crate in "${CRATES[@]}"; do
    packages+="${packages:+ | }package(=$crate)"
  done
  "$NEXTEST" nextest run --archive-file "$test_bundle/nextest.tar.zst" --workspace-remap "$ROOT" --profile "$PROFILE" --no-tests=fail -E "($packages) & kind(test)"
  "$NEXTEST" nextest run --archive-file "$test_bundle/nextest.tar.zst" --workspace-remap "$ROOT" --profile "$PROFILE" --no-tests=fail -E 'package(=bookie-driver-redis) & binary(=cancellation)'
  exit 0
fi
if [[ "${BOOKIE_REQUIRE_PREBUILT:-0}" == 1 ]]; then
  echo 'a source-matched prebuilt test bundle is required' >&2
  exit 1
fi
"$NEXTEST" nextest run "${PKG_ARGS[@]}" --test '*' --profile "$PROFILE" --no-tests=fail
"$NEXTEST" nextest run -p bookie-driver-redis --test cancellation --profile "$PROFILE" --no-tests=fail
