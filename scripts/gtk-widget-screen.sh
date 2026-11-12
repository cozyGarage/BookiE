#!/usr/bin/env bash
set -euo pipefail
scale="${GDK_SCALE:-1}"
if [[ ! "$scale" =~ ^[1-4]$ ]]; then
  printf 'GDK_SCALE must be an integer from 1 to 4; got %s\n' "$scale" >&2
  exit 2
fi
printf '%sx%dx24' "$((1600 * scale))" "$((1200 * scale))"
