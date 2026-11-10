#!/usr/bin/env bash
# Local CI mirror for the repository.
#
# Modes:
#   ./scripts/ci-local.sh           # default: full fast checks (GTK included)
#   ./scripts/ci-local.sh quick     # alias for ./scripts/preflight.sh (no GTK app)
#   ./scripts/ci-local.sh full      # fmt + clippy + build + unit tests (GTK)
#   ./scripts/ci-local.sh integration  # driver docker integration tests
#   ./scripts/ci-local.sh release      # automated gates; manual package/soak approval remains separate
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="${1:-full}"
if [[ "${BOOKIE_CI_REPORT_ACTIVE:-0}" != 1 ]]; then
  export BOOKIE_CI_REPORT_ACTIVE=1
  exec python3 "$ROOT/scripts/ci-report.py" "$MODE" bash "$0" "$MODE"
fi

if [[ -f "$ROOT/scripts/dev-env.sh" ]]; then
  # shellcheck source=/dev/null
  source "$ROOT/scripts/dev-env.sh"
fi

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
case "$CARGO_TARGET_DIR" in
  /tmp/*) export CARGO_TARGET_DIR="$ROOT/target" ;;
esac

run_full() {
  bash scripts/test-harness.sh
  echo "==> file size guardrail"
  "$ROOT/scripts/check-file-size.sh"

  echo "==> bounded database operations in the GUI"
  "$ROOT/scripts/check-bounded-operations.sh"

  echo "==> panic sites in production code"
  "$ROOT/scripts/check-panic-sites.sh"

  echo "==> function sizes"
  python3 scripts/check-function-size.py

  echo "==> cargo fmt --check"
  cargo fmt --all -- --check

  echo "==> cargo clippy"
  cargo clippy --workspace --all-targets -- -D warnings

  echo "==> nextest --workspace --lib --bins"
  # Compiling tests already builds the crates; a separate `cargo build`
  # beforehand doubles wall time for little signal.
  NEXTEST="$("$ROOT/scripts/ensure-nextest.sh")"
  "$NEXTEST" nextest run --workspace --lib --bins --profile "${BOOKIE_NEXTEST_PROFILE:-default}" --no-tests=fail

  echo "==> sandbox integration tier"
  "$ROOT/scripts/test-sandbox.sh"

  echo "Full fast checks passed."
  echo "Driver integration: ./scripts/ci-local.sh integration"
}

run_gtk_app() {
  echo "==> cargo clippy (GTK app)"
  cargo clippy --locked -p bookie-app --all-targets -- -D warnings

  echo "==> nextest (GTK app library and binaries)"
  NEXTEST="$("$ROOT/scripts/ensure-nextest.sh")"
  if [[ -n "${BOOKIE_TEST_BUNDLE:-}" ]]; then
    python3 "$ROOT/scripts/ci-test-bundle.py" verify "$BOOKIE_TEST_BUNDLE" "${BOOKIE_SOURCE_COMMIT:-${GITHUB_SHA:?source revision required}}"
    "$NEXTEST" nextest run --archive-file "$BOOKIE_TEST_BUNDLE/nextest.tar.zst" \
      --workspace-remap "$ROOT" --profile "${BOOKIE_NEXTEST_PROFILE:-default}" --no-tests=fail \
      -E 'package(=bookie-app) & (kind(lib) | kind(bin))'
  elif [[ "${BOOKIE_REQUIRE_PREBUILT:-0}" == 1 ]]; then
    echo 'a source-matched prebuilt test bundle is required' >&2
    exit 1
  else
    "$NEXTEST" nextest run --locked -p bookie-app --lib --bins --profile "${BOOKIE_NEXTEST_PROFILE:-default}" --no-tests=fail
  fi
}

run_integration() {
  export BOOKIE_TEST_RUN_ID="ci-${$}-${RANDOM}"
  cleanup_testcontainers() {
    python3 "$ROOT/scripts/cleanup-testcontainers.py" "$BOOKIE_TEST_RUN_ID"
  }
  trap cleanup_testcontainers EXIT
  trap 'cleanup_testcontainers; exit 143' TERM
  trap 'cleanup_testcontainers; exit 130' INT
  echo "==> SQL and document/key-value driver integration"
  bash scripts/run-rust-tests.sh --locked --test integration \
    -p bookie-driver-postgres -p bookie-driver-mysql -p bookie-driver-mssql \
    -p bookie-driver-clickhouse -p bookie-driver-redis -p bookie-driver-mongodb -p bookie-driver-oracle \
    -- --include-ignored --test-threads=1
  bash scripts/run-rust-tests.sh --locked -p bookie-driver-duckdb \
    --test integration --test read_only --test missing_file -- --include-ignored --test-threads=1
  bash scripts/run-rust-tests.sh --locked -p bookie-mcp --test mongodb_extended_json -- --include-ignored --test-threads=1
  bash scripts/run-rust-tests.sh --locked -p bookie-mcp --test postgres_custom_enum -- --include-ignored --test-threads=1
  bash scripts/run-rust-tests.sh --locked -p bookie-policy --test session_postgres -- --include-ignored --test-threads=1
  bash scripts/run-rust-tests.sh --locked -p bookie-policy --test session_mssql -- --include-ignored --test-threads=1
  echo "==> PostgreSQL Unix-socket integration"
  ./scripts/test-postgres-socket.sh
  bash ./scripts/test-ssh.sh
  echo "Integration checks passed."
}

run_release() {
  run_full
  bash "$ROOT/scripts/test-gtk-widgets.sh"
  run_integration
  python3 scripts/run-isolated-tests.py app-server
  "$ROOT/scripts/test-driver-tls.sh"
  "$ROOT/scripts/test-secret-service.sh"
  echo "==> PostgreSQL release fixture"
  "$ROOT/scripts/test-postgres-release.sh"
  echo "==> Installed GTK safety flows"
  "$ROOT/scripts/test-gtk-safety.sh"
  echo "==> ClickHouse Enum16 GTK edit"
  python3 scripts/run-test-layer.py gtk-clickhouse
  echo "==> MySQL-backed GTK approval dialog"
  python3 scripts/run-test-layer.py gtk-mysql-approval
  echo "==> MongoDB-backed GTK cursor and edit contract"
  python3 scripts/run-test-layer.py gtk-mongodb
  cargo test --locked -p bookie-driver-duckdb
  cargo build --locked -p bookie-app
  cargo deny check
  cargo audit
  echo "Automated release gates passed. Package install/upgrade/rollback, Wayland and candidate soak remain separate."
}

case "$MODE" in
  quick | preflight)
    exec "$ROOT/scripts/preflight.sh"
    ;;
  full | "")
    run_full
    ;;
  gtk-app)
    run_gtk_app
    ;;
  integration)
    run_integration
    ;;
  widgets)
    bash "$ROOT/scripts/test-gtk-widgets.sh"
    ;;
  release)
    run_release
    ;;
  *)
    echo "usage: $0 [quick|full|gtk-app|widgets|integration|release]" >&2
    exit 2
    ;;
esac
