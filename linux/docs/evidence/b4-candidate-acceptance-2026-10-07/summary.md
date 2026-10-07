# B4 candidate acceptance checkpoint — 2026-10-07

The broad candidate layer report records local work based on `fork/linux`
`4f819f9123fe005db7dbd5f530ad8f3c73e1f0d4` (PR #159). It passed the full,
security-policy, drivers, TLS, SSH, PostgreSQL release, widgets, keyring,
app-server, supply-chain and SQL Server Kerberos layers. The Arch packaging
contract passed. The Debian package step lacked `dpkg-deb` on the host; its
validator passed in a disposable Debian testing container.

PR #162 fixes the GTK safety setup and adds Xvfb/AT-SPI runtime packages to
the PostgreSQL release job. PR #161 had already added the GTK build dependencies.
On PR #162 SHA `301b05926418eddd7cf895781de56dea77c87c77`, the full local UI
layer passed all 47 scenarios in 268.396 seconds.

The first hosted PR #162 run exposed a GTK accessibility role mismatch: Debian
reported the visible SSH trust prompt as a frame, while the test required an
alert. The attempted connection then timed out. The trust prompt selectors now
accept either role. The focused scenario and the complete PostgreSQL release
layer, including all five PostgreSQL GTK scenarios, passed locally after this
fix; see the rerun report and log. The first hosted run's PostgreSQL failure
report/log are retained here. Its Fast checks also failed before building when
a Debian mirror served a package at a size inconsistent with the package index;
installed GTK smoke was skipped as a consequence. This is distinct from the
GTK role failure.

At the last first-run status check, preflight, driver TLS, DuckDB, security,
supply chain, SonarCloud, harness, ref resolution, workflow lint and both
Flatpak builds passed. General driver integration was still running. The exact
hosted status is recorded in the manifest; the final follow-up commit needs
fresh hosted validation.

Installed Arch/Omarchy/Hyprland Wayland acceptance, upgrade/rollback,
Debian/GNOME Wayland acceptance, B3 completion and frozen-candidate acceptance
are not established here. This evidence does not close B4 or qualify a release.
