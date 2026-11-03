#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

if [[ -n "${BOOKIE_GTK_BINARY:-}" ]]; then
  printf '%s\n' "$BOOKIE_GTK_BINARY"
  exit 0
fi

profile="${BOOKIE_GTK_BUILD_PROFILE:-ci}"
case "$profile" in
  ci | release) ;;
  *)
    echo "BOOKIE_GTK_BUILD_PROFILE must be ci or release, got: $profile" >&2
    exit 2
    ;;
esac

(cd "$ROOT" && cargo build --profile "$profile" --locked -p bookie-app --bin bookie-app) >&2
test_binary="$TARGET_DIR/installed-$profile/usr/bin/bookie"
install -Dm755 "$TARGET_DIR/$profile/bookie-app" "$test_binary"
printf '%s\n' "$test_binary"
