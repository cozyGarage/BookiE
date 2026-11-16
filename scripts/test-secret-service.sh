#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Never unlock or write a probe into the user's desktop keyring.
if [[ "${BOOKIE_SECRET_DBUS_ACTIVE:-0}" != "1" ]]; then
  secret_root="$(mktemp -d "${TMPDIR:-/tmp}/bookie.XXXXXX")"
  mkdir -p \
    "$secret_root/home" \
    "$secret_root/config" \
    "$secret_root/data" \
    "$secret_root/cache" \
    "$secret_root/state" \
    "$secret_root/runtime"
  chmod 0700 "$secret_root/runtime"
  trap 'rm -rf -- "$secret_root"' EXIT
  git_config="$secret_root/gitconfig"
  git -C "$ROOT" config --file "$git_config" --add safe.directory "$ROOT"
  cargo_home="${CARGO_HOME:-$HOME/.cargo}"
  rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
  BOOKIE_SECRET_DBUS_ACTIVE=1 \
    CARGO_HOME="$cargo_home" \
    RUSTUP_HOME="$rustup_home" \
    HOME="$secret_root/home" \
    GIT_CONFIG_GLOBAL="$git_config" \
    GIT_CONFIG_NOSYSTEM=1 \
    XDG_CONFIG_HOME="$secret_root/config" \
    XDG_DATA_HOME="$secret_root/data" \
    XDG_CACHE_HOME="$secret_root/cache" \
    XDG_STATE_HOME="$secret_root/state" \
    XDG_RUNTIME_DIR="$secret_root/runtime" \
    dbus-run-session -- "$ROOT/scripts/test-secret-service.sh" "$@"
  exit
fi

gnome-keyring-daemon --daemonize --components=secrets >/dev/null
python3 "$ROOT/scripts/run-isolated-tests.py" keyring-fresh
eval "$(printf 'bookie-test' | gnome-keyring-daemon --replace --daemonize --unlock --components=secrets)"
# --replace returns before the old daemon releases the bus name; retry until the unlocked one answers.
for attempt in $(seq 1 20); do
  printf 'probe' | secret-tool store --label='BookiE CI probe' application bookie-ci && break
  [[ "$attempt" == 20 ]] && { echo "unlocked keyring daemon did not take over" >&2; exit 1; }
  sleep 0.5
done
secret-tool clear application bookie-ci
python3 "$ROOT/scripts/run-isolated-tests.py" keyring
