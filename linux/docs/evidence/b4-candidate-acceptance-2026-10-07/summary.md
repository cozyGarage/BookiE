B4 candidate acceptance checkpoint — 2026-10-07

Baseline: fork/linux 4f819f9123fe005db7dbd5f530ad8f3c73e1f0d4 (PR #159 merge).

Initial broad local layer run: full, security-policy, drivers, TLS, SSH,
PostgreSQL release, widgets, keyring, app-server, packaging contracts,
supply-chain and SQL Server Kerberos passed. UI initially exposed a stale GTK
setup in audit_failure_denies and a flaky file chooser interaction in the
bundle-import scenarios. Host Debian package-contract step was blocked because
dpkg-deb is absent on this host.

Local follow-up fixes: audit_failure_denies now exercises a saved connection
without opening the editor and verifies denial/audit behavior; bundle import
uses the AT-SPI location action. The complete UI layer then passed (47 GTK
scenarios; 238.348 seconds). The Debian package symlink validator passed in a
disposable debian:testing container (image digest recorded in the manifest).
Workflow actionlint and the 9-test workflow unit suite passed. Python bytecode
compilation passed.

Hosted PR #159 at merge: PostgreSQL release job failed before tests because
GTK/GLib development pkg-config dependencies were missing. Installed GTK
safety smoke failed because its setup opened the editor before testing the
expected audit fail-closed path. The PostgreSQL workflow now installs the GTK
build and AT-SPI runtime dependencies; the GTK test corrections are local.
These hosted fixes still require a follow-up PR and fresh hosted checks.

Not established here: installed Arch/Omarchy/Hyprland Wayland acceptance,
upgrade/rollback, Debian/GNOME Wayland acceptance, or B3 completion. This
checkpoint does not close B4 or qualify a release.
