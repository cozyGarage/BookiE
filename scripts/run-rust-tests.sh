#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -n "${BOOKIE_TEST_BUNDLE:-}" ]]; then
  exec python3 "$ROOT/scripts/run-prebuilt-tests.py" "$@"
fi
if [[ "${BOOKIE_REQUIRE_PREBUILT:-0}" == 1 ]]; then
  echo 'a source-matched prebuilt test bundle is required' >&2
  exit 1
fi
exec cargo test "$@"
