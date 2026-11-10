#!/usr/bin/env bash
set -u

workspace="${GITHUB_WORKSPACE:-$PWD}"
status=0

if [[ -n "${BOOKIE_TEST_RUN_ID:-}" && -f "$workspace/src/scripts/cleanup-testcontainers.py" ]]; then
  python3 "$workspace/src/scripts/cleanup-testcontainers.py" "$BOOKIE_TEST_RUN_ID" || status=$?
fi

if [[ "${GITHUB_JOB:-}" == "driver-tls" && "${BOOKIE_TEST_RUN_ID:-}" =~ ^[A-Za-z0-9_-]+$ ]]; then
  fixture="$workspace/src/tests/fixtures/driver-tls"
  if [[ -f "$fixture/docker-compose.yml" ]]; then
    docker compose --project-name "$BOOKIE_TEST_RUN_ID" --project-directory "$fixture" \
      -f "$fixture/docker-compose.yml" down --volumes --remove-orphans || status=$?
  fi
fi

if [[ -f "$workspace/src/scripts/ci-cache.py" && "${CARGO_TARGET_DIR:-}" == /var/cache/ci/target/* ]]; then
  python3 "$workspace/src/scripts/ci-cache.py" cleanup --min-free-gib 30 || status=$?
fi

if [[ "${GITHUB_RUN_ID:-}" =~ ^[0-9]+$ ]]; then
  if [[ "${CARGO_TARGET_DIR:-}" == "/var/cache/ci/target/bookie-guards-$GITHUB_RUN_ID" ]]; then
    rm -rf -- "$CARGO_TARGET_DIR" || status=$?
  fi
  if [[ "${TMPDIR:-}" == "/var/cache/ci/tmp/bookie-guards-$GITHUB_RUN_ID" ]]; then
    rm -rf -- "$TMPDIR" || status=$?
  fi
  if [[ "${TMPDIR:-}" == "$workspace/.tmp-pg-dump-$GITHUB_RUN_ID-${GITHUB_RUN_ATTEMPT:-1}" ]]; then
    rm -rf -- "$TMPDIR" || status=$?
  fi
fi

if [[ "${GITHUB_JOB:-}" == driver && "${TMPDIR:-}" =~ ^/var/cache/ci/tmp/bookie-driver-[0-9]+-[0-9]+-(postgres|mysql|mssql|clickhouse|redis|mongodb|oracle|duckdb)$ ]]; then
  rm -rf -- "$TMPDIR" || status=$?
fi

for entry in src gtk-artifact test-bundles floor-artifact floor-package floor-package-path; do
  rm -rf -- "$workspace/$entry" || status=$?
done
rm -rf -- "$workspace"/coverage*.json "$workspace"/mutants-* "$workspace"/soak-*.log || status=$?

exit "$status"
