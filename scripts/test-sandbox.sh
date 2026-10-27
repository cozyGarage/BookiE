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
  tablepro-core
  tablepro-policy
  tablepro-storage
  tablepro-transport
  tablepro-app-services
  tablepro-ssh
  tablepro-dump
  tablepro-mcp
  tablepro-agentd
  tablepro-driver-sqlite
)

PKG_ARGS=()
for c in "${CRATES[@]}"; do
  PKG_ARGS+=(-p "$c")
done

echo "==> sandbox tier: all integration targets"
NEXTEST="$("$ROOT/scripts/ensure-nextest.sh")"
PROFILE="${TABLEPRO_NEXTEST_PROFILE:-default}"
ARCHIVE_ARGS=()
if [[ -n "${BOOKIE_TEST_BUNDLE:-}" ]]; then
  python3 "$ROOT/scripts/ci-test-bundle.py" verify "$BOOKIE_TEST_BUNDLE" "${GITHUB_SHA:?source revision required}"
  ARCHIVE_ARGS=(--archive-file "$BOOKIE_TEST_BUNDLE/nextest.tar.zst" --workspace-remap "$ROOT")
elif [[ "${BOOKIE_REQUIRE_PREBUILT:-0}" == 1 ]]; then
  echo 'a source-matched prebuilt test bundle is required' >&2
  exit 1
fi
"$NEXTEST" nextest run "${PKG_ARGS[@]}" --test '*' --profile "$PROFILE" --no-tests=fail "${ARCHIVE_ARGS[@]}"
"$NEXTEST" nextest run -p tablepro-driver-redis --test cancellation --profile "$PROFILE" --no-tests=fail "${ARCHIVE_ARGS[@]}"
