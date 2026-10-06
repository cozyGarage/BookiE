#!/usr/bin/env bash
# Build and test the GTK crate on the oldest supported distro stacks.
# Usage: bash scripts/test-distro-floor.sh [ubuntu:24.04|debian:13 ...]
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
images=("$@")
[ ${#images[@]} -gt 0 ] || images=(ubuntu:24.04 debian:13)

for base in "${images[@]}"; do
  tag="bookie-floor-${base//[:\/]/-}"
  echo "== $base"
  docker build -q -t "$tag" --build-arg "BASE=$base" scripts/distro-floor >/dev/null
  docker run --rm -v "$PWD:/src:ro" -v "${tag}-target:/target" -e CARGO_TARGET_DIR=/target "$tag" bash -ceu '
    pkg-config --modversion gtk4 libadwaita-1 gtksourceview-5
    cargo test --manifest-path /src/Cargo.toml -p tablepro-app --lib 2>&1 | tail -3
    xvfb-run -a dbus-run-session -- cargo test --manifest-path /src/Cargo.toml -p tablepro-app --lib -- --ignored --test-threads=1 2>&1 | tail -5
  '
done
