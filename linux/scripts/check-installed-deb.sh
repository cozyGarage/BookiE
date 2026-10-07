#!/usr/bin/env bash
set -euo pipefail
for binary in bookie bookie-agentd tablepro tablepro-agentd tablepro-askpass; do
  test -x "/usr/bin/$binary" || { echo "missing /usr/bin/$binary"; exit 1; }
done
desktop-file-validate /usr/share/applications/com.tablepro.linux.desktop
test -f /usr/share/glib-2.0/schemas/com.tablepro.linux.gschema.xml || { echo "GSettings schema is not installed"; exit 1; }
test -f /usr/share/glib-2.0/schemas/gschemas.compiled
gsettings list-keys com.tablepro.linux | grep -q '^editor-font-family$'
ldd /usr/bin/bookie | grep -q "not found" && { echo "unresolved libraries"; exit 1; }
echo "installed package checks passed"
