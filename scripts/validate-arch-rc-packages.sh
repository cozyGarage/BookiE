#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ $# -lt 2 ]]; then
  echo "usage: $0 OUTPUT_DIR PACKAGE [PACKAGE ...]" >&2
  exit 2
fi
output_dir="$(cd "$1" && pwd)"
shift
packages=()
app_packages=()
for package in "$@"; do
  if [[ "$package" != /* ]]; then
    package="$PWD/$package"
  fi
  if [[ ! -f "$package" ]]; then
    echo "package not found: $package" >&2
    exit 2
  fi
  packages+=("$package")
  if [[ "$(basename "$package")" =~ ^bookie-([0-9]+\.[0-9]+\.[0-9]+([a-z]+[0-9]+)?)-([0-9]+)-x86_64\.pkg\.tar\.(zst|xz)$ ]]; then
    package_version="${BASH_REMATCH[1]}"
    package_release="${BASH_REMATCH[3]}"
    app_packages+=("$package")
  fi
done
if [[ ${#app_packages[@]} -ne 1 ]]; then
  echo "expected exactly one BookiE application package, found ${#app_packages[@]}" >&2
  exit 1
fi

validation_dir="$(mktemp -d "$output_dir/.validate.XXXXXX")"
trap 'rm -rf -- "$validation_dir"' EXIT
export TMPDIR="$validation_dir"
embedded_metadata="$validation_dir/BUILD_INPUTS.json"
bsdtar -xOf "${app_packages[0]}" usr/share/doc/bookie/BUILD_INPUTS.json > "$embedded_metadata"
metadata_fields="$validation_dir/metadata-fields"
python3 - "$embedded_metadata" "$metadata_fields" "$package_version" <<'PY'
from datetime import date
import json
from pathlib import Path
import re
import sys

metadata = json.loads(Path(sys.argv[1]).read_text())
if metadata.get("package_version") != sys.argv[3] or metadata.get("source_version") != sys.argv[3]:
    raise SystemExit("package and recorded source versions do not match")
if metadata.get("target") != "x86_64-unknown-linux-gnu":
    raise SystemExit("release input target does not match the Arch package architecture")
if not re.fullmatch(r"[0-9a-f]{40}", metadata.get("commit", "")):
    raise SystemExit("release input commit is not an immutable SHA")
for key in ("source_archive_sha256", "vendor_archive_sha256", "arch_recipe_sha256"):
    if not re.fullmatch(r"[0-9a-f]{64}", metadata.get(key, "")):
        raise SystemExit(f"invalid {key}")
date.fromisoformat(metadata.get("release_date", ""))
Path(sys.argv[2]).write_text("\n".join((
    metadata["commit"],
    metadata["source_archive_sha256"],
    metadata["vendor_archive_sha256"],
    metadata["arch_recipe_sha256"],
)) + "\n")
PY
mapfile -t fields < "$metadata_fields"
commit="${fields[0]}"
source_archive="$output_dir/bookie-$package_version-$commit.tar.gz"
vendor_archive="$output_dir/bookie-vendor-$package_version-$commit.tar.gz"
metadata="$output_dir/bookie-$package_version-$commit-inputs.json"
for input in "$source_archive" "$vendor_archive" "$metadata"; do
  if [[ ! -f "$input" ]]; then
    echo "candidate input is missing: $input" >&2
    exit 1
  fi
done
cmp "$embedded_metadata" "$metadata"
source_sha256="$(sha256sum "$source_archive" | awk '{print $1}')"
vendor_sha256="$(sha256sum "$vendor_archive" | awk '{print $1}')"
recipe="$root/apps/linux/packaging/arch/PKGBUILD"
recipe_sha256="$(sha256sum "$recipe" | awk '{print $1}')"
if [[ "$source_sha256" != "${fields[1]}" || "$vendor_sha256" != "${fields[2]}" ||
      "$recipe_sha256" != "${fields[3]}" ]]; then
  echo "candidate source, vendor archive or recipe checksum does not match package metadata" >&2
  exit 1
fi
archived_recipe="$validation_dir/source-PKGBUILD"
tar -xOf "$source_archive" "TablePro-$commit/apps/linux/packaging/arch/PKGBUILD" > "$archived_recipe"
cmp "$recipe" "$archived_recipe"

TABLEPRO_RC_VERSION="$package_version" TABLEPRO_RC_PKGREL="$package_release" \
TABLEPRO_RC_COMMIT="$commit" TABLEPRO_RC_SHA256="$source_sha256" \
TABLEPRO_RC_ARCHIVE="$source_archive" TABLEPRO_RC_VENDOR_SHA256="$vendor_sha256" \
TABLEPRO_RC_VENDOR_ARCHIVE="$vendor_archive" TABLEPRO_RC_METADATA="$metadata" \
TABLEPRO_RC_METADATA_SHA256="$(sha256sum "$metadata" | awk '{print $1}')" \
python3 "$root/scripts/resolve-arch-rc-pkgbuild.py" "$recipe" "$validation_dir/PKGBUILD"
namcap "$validation_dir/PKGBUILD" "${packages[@]}"
for package in "${app_packages[@]}"; do
  "$root/scripts/validate-arch-package.sh" "$package"
  sha256sum "$package"
done
