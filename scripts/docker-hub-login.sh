#!/usr/bin/env bash
set -euo pipefail
if [ -z "${DOCKERHUB_USERNAME:-}" ] || [ -z "${DOCKERHUB_TOKEN:-}" ]; then
  echo "Docker Hub credentials not set; pulling anonymously"
  exit 0
fi
for attempt in 1 2 3; do
  if printf '%s' "$DOCKERHUB_TOKEN" | timeout 20s docker login --username "$DOCKERHUB_USERNAME" --password-stdin >/dev/null; then
    echo "Docker Hub login ok"
    exit 0
  fi
  echo "Docker Hub login failed (attempt $attempt of 3)" >&2
  sleep "$attempt"
done
exit 1
