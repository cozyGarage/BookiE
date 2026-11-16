fixture_host="${BOOKIE_GTK_FIXTURE_HOST:-127.0.0.1}"
fixture_bind_address="$(getent ahostsv4 "$fixture_host" 2>/dev/null | awk 'NR == 1 { print $1; exit}' || true)"

if [[ -z "$fixture_bind_address" ]]; then
  echo "could not resolve the GTK fixture host address: $fixture_host" >&2
  exit 1
fi
