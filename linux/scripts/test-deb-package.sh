#!/usr/bin/env bash
# Build the .deb on a supported Debian-family stack, install it with apt, and check what landed.
# Usage: bash scripts/test-deb-package.sh [ubuntu:24.04|debian:13 ...]
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
images=("$@")
[ ${#images[@]} -gt 0 ] || images=(ubuntu:24.04 debian:13)

for base in "${images[@]}"; do
  tag="bookie-floor-${base//[:\/]/-}"
  echo "== $base"
  docker build -q -t "$tag" --build-arg "BASE=$base" scripts/distro-floor >/dev/null
  docker run --rm -v "$PWD:/src:ro" -v "${tag}-target:/target" -v "${tag}-cargo:/opt/cargo/registry" -v "${tag}-git:/opt/cargo/git" -e CARGO_TARGET_DIR=/target "$tag" bash -ceu '
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends desktop-file-utils >/dev/null
    rm -rf /work && mkdir /work && cp -a /src/. /work/
    cd /work
    DEB_OUT=/work/out bash scripts/build-deb.sh >/tmp/build.log 2>&1 || { tail -40 /tmp/build.log; exit 1; }
    tail -3 /tmp/build.log
    deb=$(ls /work/out/tablepro_*.deb)
    dpkg-deb --info "$deb" | sed -n "1,12p"
    mkdir -p ~/.config/tablepro ~/.local/share/tablepro
    echo keep > ~/.config/tablepro/connections.json
    echo keep > ~/.local/share/tablepro/history.db
    apt-get install -y -qq "$deb" >/dev/null
    bash /src/scripts/check-installed-deb.sh
    apt-get install -y -qq --reinstall "$deb" >/dev/null
    bash /src/scripts/check-installed-deb.sh
    apt-get purge -y -qq tablepro >/dev/null
    test ! -e /usr/bin/bookie
    test "$(cat ~/.config/tablepro/connections.json)" = keep
    test "$(cat ~/.local/share/tablepro/history.db)" = keep
    echo "install, reinstall and purge keep the user data under the tablepro paths"
  '
done
