#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURE="$ROOT/tests/fixtures/postgres-release"
STATE="$FIXTURE/state"
cd "$ROOT"

if ! command -v docker >/dev/null 2>&1; then
  echo "missing required command: docker" >&2
  exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
  echo "docker compose is required" >&2
  exit 1
fi

KEEP_UP="${BOOKIE_FIXTURE_KEEP_UP:-0}"
fixture_host="${BOOKIE_FIXTURE_HOST:-localhost}"
fixture_bind_address="${BOOKIE_FIXTURE_BIND_ADDRESS:-$fixture_host}"
fixture_ssh_host="${BOOKIE_FIXTURE_SSH_HOST:-$fixture_host}"

if [[ -z "${BOOKIE_FIXTURE_SSH_HOST:-}" && "$fixture_ssh_host" == localhost ]]; then
  fixture_ssh_host=127.0.0.1
fi

if [[ "$fixture_bind_address" == "$fixture_host" && "$fixture_host" != 127.0.0.1 ]]; then
  fixture_bind_address="$(getent ahostsv4 "$fixture_host" | awk 'NR == 1 { print $1; exit}')"
fi

if [[ -z "$fixture_bind_address" ]]; then
  echo "could not resolve the Docker fixture host address: $fixture_host" >&2
  exit 1
fi
export BOOKIE_FIXTURE_BIND_ADDRESS="$fixture_bind_address"
export BOOKIE_FIXTURE_SSH_HOST="$fixture_ssh_host"

compose() {
  docker compose --project-directory "$FIXTURE" -f "$FIXTURE/docker-compose.yml" "$@"
}

teardown() {
  if [[ "$KEEP_UP" == "1" ]]; then
    echo "leaving the fixture running (BOOKIE_FIXTURE_KEEP_UP=1)"
    return
  fi
  compose down --volumes --remove-orphans >/dev/null 2>&1 || true
  rm -rf -- "${secret_root:-}"
}

wait_for_port() {
  local host="$1" port="$2" label="$3"
  for _ in $(seq 1 120); do
    if (exec 3<>"/dev/tcp/$host/$port") 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  echo "$label did not accept connections on $host:$port" >&2
  return 1
}

bash "$FIXTURE/generate-materials.sh"
chmod 0755 "$FIXTURE/seed"
chmod 0644 "$FIXTURE/seed/01-seed.sql"

mkdir -p "$STATE/config"
rm -f "$STATE/config/bookie/known_hosts"

trap teardown EXIT
compose down --volumes --remove-orphans >/dev/null 2>&1 || true
if compose up -d --build --wait --wait-timeout 300; then
  :
else
  startup_status=$?
  compose ps --all || true
  compose logs --no-color --tail 50 || true
  exit "$startup_status"
fi

wait_for_port "$fixture_host" 8474 "toxiproxy api"
wait_for_port "$fixture_host" 5433 "postgres path"
wait_for_port "$fixture_ssh_host" 2223 "bastion path"

# A forwarded Unix socket's path (under XDG_RUNTIME_DIR) is capped at
# 100 bytes, so the isolated runtime dir has to stay short -- the
# fixture's own state directory, deep under the checked-out repo, is
# already too long for that once a socket name is appended.
secret_root="$(mktemp -d "${TMPDIR:-/tmp}/bookie.XXXXXX")"
mkdir -p "$secret_root/home" "$secret_root/data" "$secret_root/cache" "$secret_root/state" "$secret_root/runtime"
chmod 0700 "$secret_root/runtime"

cargo_home="${CARGO_HOME:-$HOME/.cargo}"
target_dir="${CARGO_TARGET_DIR:-$ROOT/target}"
CARGO_HOME="$cargo_home" \
RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
  HOME="$secret_root/home" \
  XDG_CONFIG_HOME="$STATE/config" \
  XDG_DATA_HOME="$secret_root/data" \
  XDG_CACHE_HOME="$secret_root/cache" \
  XDG_STATE_HOME="$secret_root/state" \
  XDG_RUNTIME_DIR="$secret_root/runtime" \
  BOOKIE_FIXTURE_POSTGRES_RELEASE=1 \
  BOOKIE_FIXTURE_PROXY_HOST="$fixture_host" \
  BOOKIE_FIXTURE_SSH_HOST="$fixture_ssh_host" \
  BOOKIE_FIXTURE_TOXIPROXY="$fixture_host:8474" \
  BOOKIE_FIXTURE_MATERIALS="$FIXTURE/materials" \
  BOOKIE_TARGET_DIR="$target_dir" \
  BOOKIE_GTK_POSTGRES_CONTAINER="$(compose ps -q db)" \
  dbus-run-session -- bash -c '
    set -euo pipefail
    eval "$(printf "bookie-test" | gnome-keyring-daemon --daemonize --unlock --components=secrets)"
    if [[ -n "${BOOKIE_TEST_BUNDLE:-}" ]]; then
      export PATH="$BOOKIE_TEST_BUNDLE/bin:$PATH"
      export BOOKIE_GTK_BINARY="$BOOKIE_TEST_BUNDLE/bin/bookie-app"
    elif [[ "${BOOKIE_REQUIRE_PREBUILT:-0}" == 1 ]]; then
      echo "a source-matched prebuilt test bundle is required" >&2
      exit 1
    elif [[ -n "${BOOKIE_GTK_BINARY:-}" ]]; then
      if [[ ! -x "$BOOKIE_GTK_BINARY" ]]; then
        echo "the supplied GTK test binary is not executable: $BOOKIE_GTK_BINARY" >&2
        exit 1
      fi
      cargo build --locked -p bookie-ssh --bin bookie-askpass
    else
      cargo build --locked -p bookie-ssh --bin bookie-askpass
      cargo build --locked -p bookie-app --bin bookie-app
    fi
    PATH="$BOOKIE_TARGET_DIR/debug:$PATH" bash scripts/run-rust-tests.sh --locked -p bookie-agentd --test g5_system_openssh -- --include-ignored --test-threads=1
    bash scripts/run-rust-tests.sh --locked -p bookie-agentd --test mtls -- --include-ignored --test-threads=1
    bash scripts/run-rust-tests.sh --locked -p bookie-release-tests --tests -- --include-ignored --test-threads=1
    BOOKIE_GTK_DBUS_ACTIVE=0 \
      BOOKIE_GTK_POSTGRES_DB=bookie \
      BOOKIE_GTK_POSTGRES_USER=bookie \
      BOOKIE_GTK_POSTGRES_MTLS_PASSWORD=bookie \
      BOOKIE_GTK_BINARY="${BOOKIE_GTK_BINARY:-$BOOKIE_TARGET_DIR/debug/bookie-app}" \
      BOOKIE_GTK_SCENARIO="${BOOKIE_GTK_SCENARIO:-postgres_saved_mtls_connection_authenticates_and_queries,postgres_structure_tabs_keep_backend_count_bounded,postgres_ssh_unknown_host_key_decline_is_durably_audited,postgres_ssh_multihop_trusts_both_hops_and_queries,postgres_ssh_second_hop_decline_does_not_learn_key,postgres_ssh_changed_second_hop_key_is_refused,postgres_ssh_setup_failure_is_durably_audited,postgres_ssh_tunnel_loss_retires_session_and_reconnects}" \
      BOOKIE_GTK_POSTGRES_MTLS_PORT=5433 \
      BOOKIE_GTK_POSTGRES_MTLS_HOST="$BOOKIE_FIXTURE_PROXY_HOST" \
      BOOKIE_GTK_POSTGRES_MTLS_DB=bookie \
      BOOKIE_GTK_POSTGRES_MTLS_USER=bookie_mtls \
      BOOKIE_GTK_POSTGRES_MTLS_CA="$BOOKIE_FIXTURE_MATERIALS/ca.crt" \
      BOOKIE_GTK_POSTGRES_MTLS_CERT="$BOOKIE_FIXTURE_MATERIALS/client.crt" \
      BOOKIE_GTK_POSTGRES_MTLS_KEY="$BOOKIE_FIXTURE_MATERIALS/client.key" \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_PORT=2223 \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_HOST="$BOOKIE_FIXTURE_SSH_HOST" \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_JUMP_HOST=relay \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_JUMP_PORT=22 \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_JUMP_USER=tunnel \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_READ_ONLY=false \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_DB_HOST=db.bookie.test \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_DB_PORT=5432 \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_KEY="$BOOKIE_FIXTURE_MATERIALS/client_ed25519_key" \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_CA="$BOOKIE_FIXTURE_MATERIALS/ca.crt" \
      BOOKIE_GTK_POSTGRES_SSH_AUDIT_PASSWORD=bookie \
      PATH="$BOOKIE_TARGET_DIR/debug:$PATH" \
      bash scripts/test-gtk-safety.sh
  '
