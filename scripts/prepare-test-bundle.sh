#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
revision=${1:?source revision required}
group=${2:?build group required}
directory=${3:?downloaded test bundle directory required}
if [[ "$(git -C "$ROOT" rev-parse HEAD)" != "$revision" ]]; then
  echo 'requested bundle revision differs from the checkout' >&2
  exit 1
fi
python3 "$ROOT/scripts/ci-test-bundle.py" verify "$directory" "$revision" --expected-group "$group"
find "$directory" -type f \( -path "$directory/tests/*" -o -path "$directory/bin/*" \) -exec chmod u+x {} +
printf 'BOOKIE_TEST_BUNDLE=%s\nBOOKIE_REQUIRE_PREBUILT=1\nBOOKIE_SOURCE_COMMIT=%s\nGSETTINGS_SCHEMA_DIR=%s/schemas\n' \
  "$(realpath "$directory")" "$revision" "$(realpath "$directory")" >> "${GITHUB_ENV:?job environment file required}"
