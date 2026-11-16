#!/usr/bin/env bash
# Build and test the GTK crate on the supported distro stack.
# Usage: bash scripts/test-distro-floor.sh [debian:testing ...]
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
cleanup_distro_floor() {
  [[ -z "${build_log:-}" ]] || rm -f -- "$build_log"
  [[ -z "${package_archive:-}" ]] || rm -f -- "$package_archive"
  [[ -z "${package_stage:-}" ]] || rm -rf -- "$package_stage"
}
trap cleanup_distro_floor EXIT
images=("$@")
[ ${#images[@]} -gt 0 ] || images=(debian:testing)

for base in "${images[@]}"; do
  tag="bookie-floor-${base//[:\/]/-}"
  echo "== $base"
  package_input=/dev/null
  package_archive=""
  package_stage=""
  package_env=()
  package_interactive=()
  source_mounts=(-v "$PWD:/src:ro")
  build_mounts=()
  build_environment=()
  if [ -n "${DISTRO_FLOOR_PACKAGE:-}" ]; then
    test -s "$DISTRO_FLOOR_PACKAGE"
    package_archive="$(mktemp "$PWD/.distro-floor-input.XXXXXX.tar")"
    package_stage="$(mktemp -d "$PWD/.distro-floor-input.XXXXXX")"
    mkdir -p "$package_stage/src/scripts" "$package_stage/src/apps/linux/app/tests" "$package_stage/tmp"
    cp scripts/test-gtk-safety.sh scripts/gtk-test-binary.sh scripts/report-gtk-failure.sh "$package_stage/src/scripts/"
    cp -a apps/linux/app/tests/. "$package_stage/src/apps/linux/app/tests/"
    cp "$DISTRO_FLOOR_PACKAGE" "$package_stage/tmp/bookie.deb"
    tar -C "$package_stage" -cf "$package_archive" .
    package_input="$package_archive"
    package_env=(-e DISTRO_FLOOR_DEB_NAME=bookie.deb)
    package_interactive=(-i)
    source_mounts=()
    dockerfile=scripts/distro-floor/Dockerfile.runtime
  else
    dockerfile=scripts/distro-floor/Dockerfile
    build_mounts=(-v "${tag}-target:/target" -v "${tag}-cargo:/opt/cargo/registry" -v "${tag}-git:/opt/cargo/git")
    build_environment=(-e CARGO_TARGET_DIR=/target)
  fi
  build_log="$(mktemp "$PWD/.docker-build.XXXXXX")"
  for attempt in 1 2 3; do
    if docker build -t "$tag" --build-arg "BASE=$base" -f "$dockerfile" scripts/distro-floor 2>&1 | tee "$build_log"; then
      break
    else
      build_status=$?
    fi
    if [ "$attempt" -eq 3 ] || ! grep -Eqi '429[[:space:]]+Too Many Requests|toomanyrequests' "$build_log"; then
      exit "$build_status"
    fi
    echo "Docker Hub rate limit; retrying image build (attempt $((attempt + 1))/3)"
    sleep "$((attempt * 20 + RANDOM % 15))"
  done
  rm -f "$build_log"
  docker run --rm "${package_interactive[@]}" ${CARGO_BUILD_JOBS:+-e CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS"} "${package_env[@]}" -e BOOKIE_GTK_SHARD="${BOOKIE_GTK_SHARD:-}" -e DISTRO_FLOOR_INSTALLED="${DISTRO_FLOOR_INSTALLED:-0}" ${BOOKIE_GTK_Y_OFFSET:+-e BOOKIE_GTK_Y_OFFSET="$BOOKIE_GTK_Y_OFFSET"} ${BOOKIE_GTK_WAIT_SECONDS:+-e BOOKIE_GTK_WAIT_SECONDS="$BOOKIE_GTK_WAIT_SECONDS"} ${BOOKIE_GTK_SCENARIO_RETRIES:+-e BOOKIE_GTK_SCENARIO_RETRIES="$BOOKIE_GTK_SCENARIO_RETRIES"} "${source_mounts[@]}" "${build_mounts[@]}" "${build_environment[@]}" "$tag" bash -ceu -o pipefail '
    if [ -n "${DISTRO_FLOOR_DEB_NAME:-}" ]; then
      tar -xf - -C /
      apt-get update
      apt install -y /tmp/bookie.deb
      status=0
      BOOKIE_GTK_BINARY=/usr/bin/bookie bash /src/scripts/test-gtk-safety.sh >/tmp/gtk-safety.log 2>&1 || status=$?
      if [ "$status" -ne 0 ]; then
        bash /src/scripts/report-gtk-failure.sh /tmp/gtk-safety.log "$status"
      fi
      grep -aE "^passed:|^gtk: .*executed successfully" /tmp/gtk-safety.log
    else
      pkg-config --modversion gtk4 libadwaita-1 gtksourceview-5
      cargo test --manifest-path /src/Cargo.toml -p bookie-app --lib 2>&1 | grep -E -A1 "^test result|FAILED|failed|panicked" | sed -n 1,60p
      if bash /src/scripts/test-gtk-widgets.sh >/tmp/gtk-widgets.log 2>&1; then
        tail -4 /tmp/gtk-widgets.log
      else
        cat /tmp/gtk-widgets.log
        exit 1
      fi
      if [ "${DISTRO_FLOOR_INSTALLED:-0}" = 1 ]; then
        status=0
        bash /src/scripts/test-gtk-safety.sh >/tmp/gtk-safety.log 2>&1 || status=$?
        if [ "$status" -ne 0 ]; then
          bash /src/scripts/report-gtk-failure.sh /tmp/gtk-safety.log "$status"
        fi
        grep -aE "^passed:|^gtk: .*executed successfully" /tmp/gtk-safety.log
      fi
    fi
  ' < "$package_input"
  cleanup_distro_floor
  build_log=""
  package_archive=""
  package_stage=""
done
