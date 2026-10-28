#!/usr/bin/env bash
# Widget tests do not use AT-SPI. Installed accessibility flows have their own tier.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
widget_root="$(mktemp -d "${TMPDIR:-/tmp}/tablepro.XXXXXX")"
cleanup_widget_root() {
  if command -v fusermount3 >/dev/null 2>&1; then
    for widget_mount in "$widget_root/runtime/doc" "$widget_root/runtime/gvfs"; do
      fusermount3 -uz -- "$widget_mount" >/dev/null 2>&1 || true
    done
  fi
  rm -rf -- "$widget_root" || true
}
trap cleanup_widget_root EXIT
export XDG_RUNTIME_DIR="$widget_root/runtime"
export XDG_CONFIG_HOME="$widget_root/config"
export XDG_DATA_HOME="$widget_root/data"
export XDG_CACHE_HOME="$widget_root/cache"
xvfb_error_log="$widget_root/xvfb.log"
mkdir -m 700 "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"
export G_DEBUG=fatal-criticals GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=test GIO_USE_VFS=local
xvfb_server_num=$((1000 + RANDOM))
if dbus-run-session -- xvfb-run --auto-servernum --server-num="$xvfb_server_num" -e "$xvfb_error_log" -s "-screen 0 1600x1200x24" bash -c '
  python3 "$1" gtk
  test_status=$?
  if command -v xdpyinfo >/dev/null 2>&1 && xdpyinfo -display "${DISPLAY:-}" >/dev/null 2>&1; then
    display_alive=alive
  else
    display_alive=unavailable
  fi
  if [ "$test_status" -ne 0 ]; then
    printf "GTK widget process exit=%s DISPLAY=%s display_alive=%s\n" "$test_status" "${DISPLAY:-unset}" "$display_alive" >&2
  fi
  exit "$test_status"
' _ "$ROOT/scripts/run-isolated-tests.py"; then
  exit 0
else
  test_status=$?
  printf 'GTK widget run=%s attempt=%s job=%s requested_display=:%s exit=%s\n' \
    "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-1}" "${GITHUB_JOB:-unknown}" "$xvfb_server_num" "$test_status" >&2
  printf 'Xvfb diagnostics:\n' >&2
  cat "$xvfb_error_log" >&2
  exit "$test_status"
fi
