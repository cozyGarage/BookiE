#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE="${TABLEPRO_PG_DUMP_IMAGE:-postgres:18-alpine}"
DOCKER="$(command -v docker)"
SHARED_TMP="${TMPDIR:-/tmp}"

work_root="$(mktemp -d)"
trap 'rm -rf -- "$work_root"' EXIT

docker pull --quiet "$IMAGE" >/dev/null

cat >"$work_root/pg_dump" <<EOF
#!/bin/sh
exec "$DOCKER" run --rm --network host --user "$(id -u):$(id -g)" \\
  --volume "$SHARED_TMP:$SHARED_TMP" \\
  -e LC_ALL -e PGHOST -e PGHOSTADDR -e PGPORT -e PGDATABASE -e PGUSER -e PGAPPNAME \\
  -e PGSSLMODE -e PGSSLROOTCERT -e PGSSLCERT -e PGSSLKEY -e PGCONNECT_TIMEOUT -e PGPASSFILE \\
  "$IMAGE" pg_dump "\$@"
EOF
chmod 0755 "$work_root/pg_dump"

cd "$ROOT"
TABLEPRO_TEST_PG_DUMP="$work_root/pg_dump" \
  cargo test --locked -p tablepro-dump --test pg_dump_roundtrip -- --include-ignored --test-threads=1
