#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "${BOOKIE_GTK_CLICKHOUSE_DBUS_ACTIVE:-0}" != "1" ]]; then
  cargo_home="${CARGO_HOME:-$HOME/.cargo}"
  rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
  runtime="$(mktemp -d "${TMPDIR:-/tmp}/bookie-gtk-clickhouse.XXXXXX")"
  mkdir -p "$runtime"/{home,config,data,cache,state,runtime}
  chmod 0700 "$runtime/runtime"
  cleanup_runtime() {
    if command -v fusermount3 >/dev/null 2>&1; then
      for mount in "$runtime/runtime/doc" "$runtime/runtime/gvfs"; do
        fusermount3 -uz -- "$mount" >/dev/null 2>&1 || true
      done
    fi
    rm -rf -- "$runtime"
  }
  trap cleanup_runtime EXIT
  BOOKIE_GTK_CLICKHOUSE_DBUS_ACTIVE=1 \
    ATSPI_DBUS_IMPLEMENTATION=dbus-daemon \
    XDG_CURRENT_DESKTOP=GNOME \
    HOME="$runtime/home" \
    XDG_CONFIG_HOME="$runtime/config" \
    XDG_DATA_HOME="$runtime/data" \
    XDG_CACHE_HOME="$runtime/cache" \
    XDG_STATE_HOME="$runtime/state" \
    XDG_RUNTIME_DIR="$runtime/runtime" \
    CARGO_HOME="$cargo_home" \
    RUSTUP_HOME="$rustup_home" \
    dbus-run-session -- "$ROOT/scripts/test-gtk-clickhouse.sh"
  exit $?
fi

for command_name in docker dbus-run-session gnome-keyring-daemon secret-tool grep; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "missing required command: $command_name" >&2
    exit 1
  fi
done

container="$(docker run --rm --detach --label "com.bookie.test-run=${BOOKIE_TEST_RUN_ID:-local-$$}" --publish 127.0.0.1::8123 \
  --env CLICKHOUSE_PASSWORD=bookie \
  --env CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1 \
  clickhouse/clickhouse-server:24.8)"
cleanup() {
  docker rm --force "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT

port="$(docker port "$container" 8123/tcp | sed -n 1p)"
port="${port##*:}"
attempt=0
while true; do
  if response="$(docker exec "$container" clickhouse-client --user=default --password=bookie \
    --query='SELECT 1' 2>/dev/null)" && [[ "$response" == "1" ]]; then
    break
  fi
  attempt=$((attempt + 1))
  if (( attempt >= 180 )); then
    docker logs "$container" >&2
    echo "ClickHouse fixture did not become ready" >&2
    exit 1
  fi
  sleep 1
done

docker exec "$container" clickhouse-client --user=default --password=bookie \
  --query="CREATE TABLE enum_grid (id UInt8, state8 Nullable(Enum8('low8' = -128, 'NULL' = 0, 'high8' = 127, '' = 1)), state16 Nullable(Enum16('low16' = -32768, 'NULL' = 0, 'high16' = 32767, '' = 1))) ENGINE = MergeTree ORDER BY id"
docker exec "$container" clickhouse-client --user=default --password=bookie \
  --query="INSERT INTO enum_grid VALUES (1, 'low8', 'low16'), (2, 'NULL', 'NULL'), (3, NULL, NULL), (4, '', ''), (5, 'high8', 'high16')"

eval "$(printf 'bookie' | gnome-keyring-daemon --daemonize --unlock --components=secrets)"
printf 'bookie' | secret-tool store --label='BookiE GTK ClickHouse fixture' \
  xdg:schema com.bookie.linux.Password \
  connection-id 31e33a85-4cbd-42ec-8f54-c66be5630f17 \
  kind db_password

BOOKIE_GTK_KEYRING_READY=1 \
  BOOKIE_GTK_DBUS_ACTIVE=1 \
  BOOKIE_GTK_CLICKHOUSE_CONTAINER="$container" \
  BOOKIE_GTK_CLICKHOUSE_HOST=127.0.0.1 \
  BOOKIE_GTK_CLICKHOUSE_PORT="$port" \
  BOOKIE_GTK_SCENARIO="${BOOKIE_GTK_SCENARIO:-clickhouse_enum_grid_edit_preserves_native_label_and_siblings,clickhouse_structure_distinguishes_unavailable_foreign_keys}" \
  bash "$ROOT/scripts/test-gtk-safety.sh"
