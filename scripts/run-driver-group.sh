#!/usr/bin/env bash
set -euo pipefail

group=${1:?usage: run-driver-group.sh <core|analytics|extended|DRIVER>}
case "$group" in
  core) drivers=(mssql mysql postgres) ;;
  analytics) drivers=(clickhouse mongodb redis) ;;
  extended) drivers=(cassandra duckdb oracle) ;;
  *)
    case " mssql mysql postgres clickhouse mongodb redis cassandra duckdb oracle " in
      *" $group "*) drivers=("$group") ;;
      *) echo "unknown driver group or driver: $group" >&2; exit 2 ;;
    esac
    ;;
esac

base_run_id=${BOOKIE_TEST_RUN_ID:-ci-$$}
current_run_id=
mkdir -p "${TMPDIR:-/tmp}"
cleanup_current() {
  if [[ -n "$current_run_id" ]]; then
    python3 scripts/cleanup-testcontainers.py "$current_run_id"
    current_run_id=
  fi
}
finish() {
  status=$?
  trap - EXIT
  cleanup_current || status=$?
  exit "$status"
}
trap finish EXIT
trap 'exit 143' TERM
trap 'exit 130' INT

for driver in "${drivers[@]}"; do
  current_run_id="$base_run_id-$driver"
  export BOOKIE_TEST_RUN_ID="$current_run_id"
  if [[ "$driver" != duckdb ]]; then
    bash scripts/prepull-test-images.sh "$driver"
  fi
  if [[ "$driver" == duckdb ]]; then
    bash scripts/run-rust-tests.sh --locked -p bookie-driver-duckdb \
      --test integration --test read_only --test missing_file -- --include-ignored --test-threads=1
  else
    test_targets=(--test integration)
    if [[ -f "crates/drivers/$driver/tests/conformance.rs" ]]; then
      test_targets+=(--test conformance)
    fi
    bash scripts/run-rust-tests.sh --locked -p "bookie-driver-$driver" \
      "${test_targets[@]}" -- --include-ignored --test-threads=1
  fi
  if [[ "$driver" == oracle ]]; then
    bash scripts/run-rust-tests.sh --locked -p bookie-policy \
      --test oracle_stored_function -- --include-ignored --test-threads=1
  fi
  cleanup_current
done
