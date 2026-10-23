# Native build and runner baseline verification

Recorded on 2026-10-23. This evidence describes the baseline branch before merge.

## Local verification

- The distro-floor workflow regression failed twice before replacing Ubuntu 24.04 package inputs, then passed all 34 existing cases.
- The expanded workflow contract passes 36 cases, including native job placement and Cargo feature requirements.
- Full preflight passed: script harness, guards, formatting, non-GTK Clippy, unit and sandbox tiers.
- `cargo metadata --locked --offline --format-version 1 --no-deps` passed. No dependency version or lockfile changes were needed.
- `python3 scripts/vendor-patches.py check` passed for all four vendored crates. Upstream checksums and recorded patches reproduce the committed sources.
- App Clippy with warnings denied passed. The full isolated GTK tier passed all 143 tests after the required calendar API and native shortcuts-dialog migration.
- Package installation and full Forgejo acceptance are pending.

## Lab verification

The lab provisioning repository owns addresses, service configuration and cutover steps. Its `docs/FORGEJO-UBUNTU-RUNNER.md` records the replacement guest.

- Five reachable runners now provide checksum-pinned nextest 0.9.148.
- Four existing runners were idle before service maintenance. Capacity is now one and `CARGO_BUILD_JOBS=8` on each; services restarted successfully.
- Only recognized generated Cargo debug/release profiles were removed under the cache maintenance lock. Registry downloads and container images were retained.
- Free space after maintenance: exec-a 62 GiB, exec-b 68 GiB, exec-c 61 GiB and the old Ubuntu runner 59 GiB.
- The replacement runner provides GTK 4.22.4, GLib 2.88.0, GtkSourceView 5.18.0 and libadwaita 1.9.1. Docker, isolated D-Bus/Xvfb and public DNS checks passed.
- Canary 908 identified an incorrect global Cargo target override. It was removed.
- Canary 909 identified missing Docker Compose. Compose and Buildx were installed and added to provisioning.
- Canary 910 passed all runner tool, container-plugin and display checks. Post-reboot canary 911 also passed through `gnome50-host`; DNS, the cache lock and runner startup persisted.
- Replacement acceptance is pending; the old Ubuntu VM has not been retired.

A newer native stack does not establish that the intermittent completion popup defect is fixed. Issue #1445 was closed by the separate completion-test correction in PR #1456; that change will be integrated before gating.

The GitHub scheduled PostgreSQL release backup also builds the GTK application directly. Its host label now uses Ubuntu 26.04. GitHub documents that label as generally available in its [runner image inventory](https://github.com/actions/runner-images/blob/main/README.md). Container-only and headless GitHub orchestration jobs keep their existing host labels.

## Integration with current Linux

The baseline branch integrated PRs #1456, #1457, #1458 and #1459 from Linux commit `c753b6f7e`. The dist release profile and PostgreSQL toolchain-home correction were retained.

Integrated preflight exposed a new comment in the completion test exceeding its zero-comment baseline. Only that comment was removed; the two-proposal test and its documented limitation remain.

The integrated GTK suite also reproduced a narrow-toolbar overflow after restoring the transaction label: 616 px requested with 600 px available in the larger-text case. The existing width assertions were kept. The Session button now uses GTK's built-in shrinking, preserving its full label and accessible name.

The cached GitHub GTK image now checks all four library floors during its build. Changing the Dockerfile also changes the image cache tag, preventing reuse of an older native stack.

Final integrated local validation passed: full preflight, app Clippy with warnings denied, and all 143 isolated GTK tests. The unchanged narrow-toolbar regression passes with the built-in button shrinking. Full Forgejo acceptance and package installation remain pending.

Local logs are retained in the primary checkout under `target/quality/gnome50-baseline-2026-10-23/`; this document records the portable findings.
