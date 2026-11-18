#!/usr/bin/env bash
# Driver TLS tier: proves each network driver's TLS mode mapping against a
# real server holding a privately issued certificate.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURE="$ROOT/tests/fixtures/driver-tls"
cd "$ROOT"

if ! command -v docker >/dev/null 2>&1; then
  echo "missing required command: docker" >&2
  exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
  echo "docker compose is required" >&2
  exit 1
fi
BOOKIE_DRIVER_TLS_REDIS_PORT="${BOOKIE_DRIVER_TLS_REDIS_PORT:-0}"
export BOOKIE_DRIVER_TLS_REDIS_PORT

KEEP_UP="${BOOKIE_FIXTURE_KEEP_UP:-0}"
if [[ "${BOOKIE_DRIVER_TLS_HOST:-}" == "host.docker.internal" && -z "${BOOKIE_DRIVER_TLS_WRONG_IDENTITY_HOST:-}" ]]; then
  BOOKIE_DRIVER_TLS_WRONG_IDENTITY_HOST="$(getent ahostsv4 "$BOOKIE_DRIVER_TLS_HOST" | awk 'NR == 1 { print $1; exit}' || true)"
  if [[ -z "$BOOKIE_DRIVER_TLS_WRONG_IDENTITY_HOST" ]]; then
    echo "could not resolve the Docker fixture host address: $BOOKIE_DRIVER_TLS_HOST" >&2
    exit 1
  fi
  export BOOKIE_DRIVER_TLS_WRONG_IDENTITY_HOST
fi
XDG_CONFIG_HOME="$(mktemp -d)"
export XDG_CONFIG_HOME
trap teardown EXIT

compose() {
  docker compose --project-name "${BOOKIE_TEST_RUN_ID:-driver-tls}" \
    --project-directory "$FIXTURE" -f "$FIXTURE/docker-compose.yml" "$@"
}

wait_for_fixture_health() {
  local deadline=$((SECONDS + 300))
  local containers container status ready
  containers="$(compose ps --all --quiet)"
  [[ -n "$containers" ]] || return 1
  while :; do
    ready=1
    while IFS= read -r container; do
      [[ -n "$container" ]] || continue
      status="$(docker inspect --format '{{.State.Health.Status}}' "$container" 2>/dev/null || true)"
      [[ "$status" == healthy ]] || ready=0
    done <<< "$containers"
    [[ "$ready" == 1 ]] && return 0
    (( SECONDS < deadline )) || return 1
    sleep 2
  done
}

teardown() {
  if [[ "$KEEP_UP" == "1" ]]; then
    echo "leaving the driver-tls fixture running (BOOKIE_FIXTURE_KEEP_UP=1)"
  else
    compose down --volumes --remove-orphans >/dev/null 2>&1 || true
  fi
  rm -rf "$XDG_CONFIG_HOME"
}

bash "$FIXTURE/generate-materials.sh" --force
chmod 0755 "$FIXTURE/seed"
chmod 0644 "$FIXTURE/seed/01-seed.js"
compose down --volumes --remove-orphans >/dev/null 2>&1 || true
if compose up -d --build --wait --wait-timeout 300; then
  wait_for_fixture_health || startup_status=$?
else
  startup_status=$?
fi
if [[ "${startup_status:-0}" != 0 ]]; then
  compose ps --all || true
  for container in $(compose ps --all --quiet); do
    docker inspect --format '{{json .State.Health}}' "$container" || true
  done
  compose logs --no-color --tail 50 || true
  exit "$startup_status"
fi
if [[ "$BOOKIE_DRIVER_TLS_REDIS_PORT" == 0 ]]; then
  redis_binding="$(compose port redis 6379)"
  BOOKIE_DRIVER_TLS_REDIS_PORT="${redis_binding##*:}"
fi
if [[ ! "$BOOKIE_DRIVER_TLS_REDIS_PORT" =~ ^[1-9][0-9]{0,4}$ ]]; then
  echo "Docker did not publish the driver TLS Redis port: $BOOKIE_DRIVER_TLS_REDIS_PORT" >&2
  exit 1
fi
export BOOKIE_DRIVER_TLS_REDIS_PORT

export BOOKIE_FIXTURE_DRIVER_TLS=1
export BOOKIE_DRIVER_TLS_MATERIALS="$FIXTURE/materials"
export BOOKIE_DRIVER_TLS_SSH_KEY="$FIXTURE/materials/ssh_client"

bash scripts/run-rust-tests.sh --locked -p bookie-driver-tls-tests --tests -- --include-ignored --test-threads=1 --skip system_store_
SSL_CERT_FILE="$FIXTURE/materials/ca.crt" SSL_CERT_DIR="$FIXTURE/materials/empty-system-store" \
  bash scripts/run-rust-tests.sh --locked -p bookie-driver-tls-tests --test system_trust -- --include-ignored --test-threads=1
