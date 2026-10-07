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
  docker run --rm -v "$PWD:/src:ro" -v "${tag}-target:/target" -e CARGO_TARGET_DIR=/target "$tag" bash -ceu '
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends debhelper dpkg-dev fakeroot desktop-file-utils appstream >/dev/null
    rm -rf /work && mkdir /work && cp -a /src/. /work/
    cd /work/packaging
    DEB_BUILD_OPTIONS=nocheck dpkg-buildpackage -b -us -uc -d >/tmp/build.log 2>&1 || { tail -40 /tmp/build.log; exit 1; }
    deb=$(ls /work/tablepro_*.deb)
    dpkg-deb --info "$deb" | sed -n "1,12p"
    apt-get install -y -qq "$deb" >/dev/null
    bash /src/scripts/check-installed-deb.sh
    apt-get remove -y -qq tablepro >/dev/null
    test ! -e /usr/bin/bookie
  '
done
