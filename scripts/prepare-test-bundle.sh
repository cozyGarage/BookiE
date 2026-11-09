#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
revision=${1:?source revision required}
group=${2:?build group required}
if [[ "$(git -C "$ROOT" rev-parse HEAD)" != "$revision" ]]; then
  echo 'requested bundle revision differs from the checkout' >&2
  exit 1
fi
cache=${BOOKIE_BUNDLE_CACHE:-/var/cache/ci/bundles}
mkdir -p "$cache"
exec 9>"$cache/.lock"
flock 9
destination="$cache/$revision-$group"
if ! python3 "$ROOT/scripts/ci-bundle-registry.py" fetch "$destination" "$revision" "$group"; then
  if [[ -z "${BOOKIE_GITHUB_ARTIFACT_TOKEN:-}" ]]; then
    echo 'No matching Forgejo bundle and GitHub fallback authentication is unavailable' >&2
    exit 1
  fi
  python3 "$ROOT/scripts/ci-bundle-registry.py" github-fetch "$destination" "$revision" "$group"
fi
printf 'BOOKIE_TEST_BUNDLE=%s\nBOOKIE_REQUIRE_PREBUILT=1\nBOOKIE_SOURCE_COMMIT=%s\nGSETTINGS_SCHEMA_DIR=%s/schemas\n' "$destination" "$revision" "$destination" >> "${GITHUB_ENV:?job environment file required}"
