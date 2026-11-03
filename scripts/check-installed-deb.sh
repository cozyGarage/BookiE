#!/usr/bin/env bash
set -euo pipefail
for binary in bookie bookie-agentd bookie bookie-agentd bookie-askpass; do
  test -x "/usr/bin/$binary" || { echo "missing /usr/bin/$binary"; exit 1; }
done
desktop-file-validate /usr/share/applications/com.bookie.linux.desktop
test -f /usr/share/glib-2.0/schemas/com.bookie.linux.gschema.xml || { echo "GSettings schema is not installed"; exit 1; }
test -f /usr/share/glib-2.0/schemas/gschemas.compiled
keys="$(gsettings list-keys com.bookie.linux)"
grep -q '^editor-font-family$' <<<"$keys"
libraries="$(ldd /usr/bin/bookie)"
if grep -q "not found" <<<"$libraries"; then echo "unresolved libraries"; exit 1; fi
echo "installed package checks passed"
