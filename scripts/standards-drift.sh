#!/usr/bin/env bash
# Print repository-standards drift. Always exits 0. Not a Forgejo gate.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
if [[ ! -f scripts/self-verify.mjs ]]; then
  echo "standards-drift: scripts/self-verify.mjs is missing" >&2
  exit 0
fi
node scripts/self-verify.mjs --warn --profile core
