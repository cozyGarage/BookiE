#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
allowed_signers="${2:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/apps/linux/packaging/arch/release-allowed-signers}"

if [[ ! "$tag" =~ ^linux-v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf 'invalid release tag: %s\n' "$tag" >&2
  exit 2
fi
if [[ ! -s "$allowed_signers" ]]; then
  printf 'release signing trust file is missing or empty: %s\n' "$allowed_signers" >&2
  exit 2
fi
if ! git rev-parse --verify --quiet "refs/tags/$tag^{tag}" >/dev/null; then
  printf 'release tag is missing or unsigned: %s\n' "$tag" >&2
  exit 1
fi

git -c gpg.format=ssh -c gpg.ssh.allowedSignersFile="$(realpath "$allowed_signers")" verify-tag "$tag"
