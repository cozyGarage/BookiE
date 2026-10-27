# CI hardening ideas from upstream TablePro

Proposed; none of this is accepted or scheduled. It records what a review of TablePro's CI at `main` commit `5533f2e7` (pushed 2026-10-24) found that BookiE does not do, after the four low-risk items below were made. Forgejo remains the merge gate and the GitHub tier stays cheap ([ADR 0020](../decisions/0020-aggregate-forgejo-trains.md), [validation playbook](../validation-playbook.md#ci-tiers)). Ideas for the `.forgejo` workflows go through the app-side session.

## Already made

- `persist-credentials: false` on every GitHub checkout, and a `workflow-security` job running `zizmor` offline (pinned, medium severity and confidence). The review found 18 checkouts that kept credentials and one template injection in the soak summary.
- A `PR title` check, because the title is now the squash subject, and a guard that only a `chore(release)` pull request edits `CHANGELOG.md`.
- The Release workflow refuses a tagged commit that is not on `linux`.

## Open questions

1. **`Swatinem/rust-cache` in the publishing workflow.** Resolved: the `.deb` and AppImage jobs in `release.yml` now use the cache only on the pull request dry run, so a tag or dispatch build that publishes starts clean. The repository held no `release-*` cache entries when this was checked (the cache was at 9.7 of 10 GiB), so little was lost. `zizmor` cannot see the `if:` guard, so both steps carry an inline ignore with that reason.
2. **Unused dependencies and notices.** Done: CI already fails on stale third-party notices (`scripts/test-harness.sh` runs `update-third-party-notices.py --check`, which the `harness` job runs). `cargo shear` found 12 real issues, now fixed, and runs as the `unused-dependencies` job. Two dependencies it flagged are intentional and are recorded as exceptions: `sqlx-core` in the PostgreSQL driver, which exists only to enable `rustls-native-certs` (removing it would drop the operating system trust store from TLS verification and still compile), and `serde_json` in the SQLite driver, which a test file included by path from `core` uses.
3. **Build without default features.** Partly done: `python3 scripts/run-test-layer.py no-default-features` runs `cargo check --workspace --all-targets --locked --no-default-features` (about 1.5 minutes cold, passing today). The default feature is `kerberos` (SQL Server integrated authentication); DuckDB is opt-in and already built with `--features duckdb` in CI. Packaging builds the three binaries without Kerberos only when the Meson option turns it off, so nothing covers the other workspace crates and test targets without it. What remains is a Forgejo merge-tier job that calls the layer, which goes through the app-side session. The layer's hosted gate says "None yet" until then.
4. **Where a change is compared from.** Upstream's planner compares a push to `main` with the last commit whose CI run passed, so a cancelled or replaced run's changes are tested again. BookiE's path filters compare against the previous push. This matters for the Forgejo gate more than for GitHub.
5. **Release environment.** Upstream releases through a protected environment with a required reviewer that is limited to release tags. Consider the same for the GitHub release job.

## Not worth copying

A single `ci.yml` entry with one aggregate check (BookiE has `regression-gate` and the Forgejo gate), the path-selection script (BookiE has `change-test-map.json`), and the Debian/Fedora PPA and COPR release (BookiE ships Arch, Flatpak, AppImage and Debian).
