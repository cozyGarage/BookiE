#!/usr/bin/env bash
set -u

workspace="${GITHUB_WORKSPACE:-$PWD}"
ci_cache_helper="${BOOKIE_CI_CACHE_HELPER:-/usr/local/libexec/bookie-ci-cache}"
status=0

if [[ -n "${SCCACHE_SERVER_UDS:-}" && -S "$SCCACHE_SERVER_UDS" ]]; then
  sccache --stop-server || status=$?
fi

if [[ -n "${BOOKIE_TEST_RUN_ID:-}" && -f "$workspace/src/scripts/cleanup-testcontainers.py" ]]; then
  python3 "$workspace/src/scripts/cleanup-testcontainers.py" "$BOOKIE_TEST_RUN_ID" || status=$?
fi

fixture_name=""
case "${GITHUB_JOB:-}" in
  driver-tls) fixture_name=driver-tls ;;
  pg-release) fixture_name=postgres-release ;;
esac
if [[ -n "$fixture_name" && "${BOOKIE_TEST_RUN_ID:-}" =~ ^[A-Za-z0-9_-]+$ ]]; then
  fixture="$workspace/src/tests/fixtures/$fixture_name"
  if [[ -f "$fixture/docker-compose.yml" ]]; then
    docker compose --project-name "$BOOKIE_TEST_RUN_ID" --project-directory "$fixture" \
      -f "$fixture/docker-compose.yml" down --volumes --remove-orphans || status=$?
  fi
fi

if [[ "${GITHUB_RUN_ID:-}" =~ ^[0-9]+$ ]]; then
  if [[ "${CARGO_TARGET_DIR:-}" == "/var/cache/ci/target/bookie-guards-$GITHUB_RUN_ID" ]]; then
    rm -rf -- "$CARGO_TARGET_DIR" || status=$?
  fi
  if [[ "${TMPDIR:-}" == "/var/cache/ci/tmp/bookie-guards-$GITHUB_RUN_ID" ]]; then
    rm -rf -- "$TMPDIR" || status=$?
  fi
  if [[ "${GITHUB_JOB:-}" == driver ]]; then
    pg_dump_tmp="$workspace/.tmp-pg-dump-$GITHUB_RUN_ID-${GITHUB_RUN_ATTEMPT:-1}"
    if [[ -d "$pg_dump_tmp" && ! -L "$pg_dump_tmp" ]]; then
      rm -rf -- "$pg_dump_tmp" || status=$?
    fi
  fi
fi

if [[ "${GITHUB_JOB:-}" == driver && "${TMPDIR:-}" =~ ^/var/cache/ci/tmp/b-[0-9]+-[0-9]+-(core|analytics|extended|postgres|mysql|mssql|clickhouse|redis|mongodb|oracle|duckdb|driver-core|driver-analytics|driver-extended|socket-ssh)$ ]]; then
  rm -rf -- "$TMPDIR" || status=$?
fi
if [[ "${GITHUB_JOB:-}" == driver-followup && "${GITHUB_RUN_ID:-}" =~ ^[0-9]+$ && "${GITHUB_RUN_ATTEMPT:-}" =~ ^[0-9]+$ && ( "${TMPDIR:-}" == "/var/cache/ci/tmp/bookie-driver-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}-driver-extended" || "${TMPDIR:-}" == "/var/cache/ci/tmp/bookie-driver-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}-socket-ssh" ) ]]; then
  rm -rf -- "$TMPDIR" || status=$?
fi
if [[ "${GITHUB_JOB:-}" == b4-rollback && "${GITHUB_RUN_ID:-}" =~ ^[0-9]+$ && "${GITHUB_RUN_ATTEMPT:-}" =~ ^[0-9]+$ && "${TMPDIR:-}" == "/var/cache/ci/tmp/bookie-b4-rollback-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}" ]]; then
  rm -rf -- "$TMPDIR" || status=$?
fi
if [[ "${GITHUB_JOB:-}" == pg-release && "${GITHUB_RUN_ID:-}" =~ ^[0-9]+$ && "${GITHUB_RUN_ATTEMPT:-}" =~ ^[0-9]+$ && "${BOOKIE_FIXTURE_TOXIPROXY_CONFIG:-}" == "/var/cache/ci/tmp/bookie-pg-release-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}.json" ]]; then
  rm -rf -- "$BOOKIE_FIXTURE_TOXIPROXY_CONFIG" || status=$?
fi

for entry in src gtk-artifact test-bundles floor-artifact floor-package floor-package-path; do
  rm -rf -- "$workspace/$entry" || status=$?
done
rm -rf -- "$workspace"/coverage*.json "$workspace"/mutants-* "$workspace"/soak-*.log || status=$?

if [[ "${CARGO_TARGET_DIR:-}" == /var/cache/ci/target/* ]]; then
  if [[ -x "$ci_cache_helper" ]]; then
    "$ci_cache_helper" cleanup --min-free-gib 30 || status=$?
  else
    printf 'CI cache helper is unavailable: %s\n' "$ci_cache_helper" >&2
    status=1
  fi
fi

exit "$status"
