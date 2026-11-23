#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

if command -v sccache >/dev/null 2>&1; then
  sccache_path="$(command -v sccache)"
  export RUSTC_WRAPPER="${RUSTC_WRAPPER:-$sccache_path}"
  export CMAKE_C_COMPILER_LAUNCHER="${CMAKE_C_COMPILER_LAUNCHER:-$sccache_path}"
  export CMAKE_CXX_COMPILER_LAUNCHER="${CMAKE_CXX_COMPILER_LAUNCHER:-$sccache_path}"
fi

cargo_args=()
separator_seen=false
for arg in "$@"; do
  if [[ "$arg" == "--" && "$separator_seen" == false ]]; then
    cargo_args+=(--manifest-path "$ROOT/apps/gpui/Cargo.toml" --)
    separator_seen=true
  else
    cargo_args+=("$arg")
  fi
done

if [[ "$separator_seen" == false ]]; then
  cargo_args+=(--manifest-path "$ROOT/apps/gpui/Cargo.toml")
fi

exec cargo "${cargo_args[@]}"
