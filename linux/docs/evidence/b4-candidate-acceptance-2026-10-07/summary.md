# B4 candidate acceptance checkpoint — 2026-10-07

The broad candidate layer report records local work based on `fork/linux`
`4f819f9123fe005db7dbd5f530ad8f3c73e1f0d4` (PR #159). It passed the full,
security-policy, drivers, TLS, SSH, PostgreSQL release, widgets, keyring,
app-server, supply-chain and SQL Server Kerberos. The Arch packaging-contract
test passed; the Debian package-contract step first lacked `dpkg-deb` on this
host. The UI layer first exposed stale setup in `audit_failure_denies` and a flaky
bundle-import file chooser. The Debian package validator passed in a disposable Debian testing container.

PR #162 fixes the UI test setup and adds the Xvfb/AT-SPI runtime packages to
the PostgreSQL release job. PR #161 had already supplied its GTK/GLib build
dependencies. On PR #162's exact source SHA `301b05926418eddd7cf895781de56dea77c87c77`,
the full local UI layer passed all 47 scenarios in 268.396 seconds. The raw
report and log are `ui-pr162-report.json` and `ui-pr162.log`.

At the 2026-10-07 08:38 UTC hosted snapshot, workflow lint, harness, ref
resolution, security policy, supply-chain, SonarCloud and the development
Flatpak build passed. The preflight layer and default Flatpak build were still
running; GTK safety and PostgreSQL release jobs had not yet started. See the
manifest for exact report hashes and status details. Do not treat pending
checks as passes.

Installed Arch/Omarchy/Hyprland Wayland acceptance, upgrade/rollback,
Debian/GNOME Wayland acceptance, B3 completion and frozen-candidate acceptance
are not established here. This evidence does not close B4 or qualify a release.
