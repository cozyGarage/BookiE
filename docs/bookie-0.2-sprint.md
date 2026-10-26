# BookiE 0.2 completion and continuation

## Delivery status

The maintainer marked 0.2.0 delivery complete, and the release was published on 2026-10-23. The [platform guide](platforms.md#published-020-release) records its assets and package names. Remaining work and candidate order for future releases are in the [backlog](backlog.md).

The previous scope, blocker table and sprint checklists are kept in the [history](#history). They are historical checklists, not fresh test, installed-package or soak evidence, and their open rows do not reopen 0.2.0.

## Delivery boundary

0.2.0 is the Linux release for the Arch/Hyprland target, with hotfixes to follow from use:

- PostgreSQL, MySQL/MariaDB and SQLite are the principal database paths; DuckDB is optional. SQL Server, ClickHouse, MongoDB and Redis retain the maturity and restrictions in [known limitations](known-limitations.md).
- GUI and agent operations share the governed policy/audit path. Lost write acknowledgement does not authorize replay, and agent output remains subject to masking.
- Arch/Hyprland is the selected desktop target. Debian/Ubuntu GNOME desktop acceptance and Flathub distribution stay in the post-0.2.0 backlog. The release page also carries a GNOME 50 Flatpak bundle; it does not establish Flathub publication or acceptance against the later GNOME 51 runtime.

The app and agentd manifests identify source version `0.2.0`. The current AppStream entry describes the published 0.2.0 release. Post-release source changes remain at version `0.2.0` until a later release is explicitly scoped; no `0.2.1` scope is assigned. The `bookie` Debian package name landed after the release tag and will be used for a later build. New upstream adoption, broader engine matrices and development-tooling changes are post-0.2.0 work. They do not reopen this milestone. Preserve exact source and environment evidence when selecting or closing each follow-up.

## Current continuation

[Project #3](https://github.com/users/cozyGarage/projects/3) owns issue status and Lane; the [ledger index](known-issues.md) maps IDs to issues. Existing open correctness, qualification and infrastructure issues keep their evidence and follow-up scope. They are not silently closed by a milestone-status change.

## Proposed 0.2.1 release plan

Status: draft for maintainer review, snapshot 2026-10-26. This does not authorize a release tag or publication. `0.2.1` is a patch version.

At this snapshot, `linux` is `15ad48925` and contains 391 first-parent commits after `linux-v0.2.0`. The accumulated change is larger than a narrow maintenance patch: it includes the entries under `[Unreleased]` in `CHANGELOG.md`, including new workflows and substantial performance, import, editor, policy and driver work. A `0.2.1` release from `linux` must qualify that full accumulated delta. Do not describe it as only the fixes in the last PRs.

### Candidate scope

- Freeze new feature work while qualifying the current `linux` tree. Fix only release-blocking defects found in qualification; each fix gets a regression and the normal GitHub and Forgejo gates.
- Include all changes already merged after `linux-v0.2.0`; do not selectively omit merged features from release notes.
- Preserve the existing database, saved-connection, keyring, workspace, audit and MCP compatibility contracts. No data-format or protocol change is planned.
- Keep database snapshots, Flathub publication, Debian/Ubuntu desktop support, new driver maturity claims, and other unscheduled backlog candidates out of this release.

### Release blockers

1. Resolve AppStream version classification and dates for the published 0.2.0 entry and the unreleased current source under [#1585](https://github.com/cozyGarage/BookiE/issues/1585); validate the metadata and version fields.
2. Complete and record native Arch/Omarchy/Hyprland install, upgrade from 0.2.0, and rollback checks under [#574](https://github.com/cozyGarage/BookiE/issues/574). Verify saved connections, credentials, workspace state and compatibility aliases after upgrade.
3. Triage the manual checklist under [#575](https://github.com/cozyGarage/BookiE/issues/575). Cover every release-critical changed flow; file a follow-up for each remaining unchecked behavior rather than implying all 107 checks passed.
4. Reconcile the intermittent installed GTK evidence under [#556](https://github.com/cozyGarage/BookiE/issues/556). Meet its current run-count criterion or record a maintainer-approved disposition based on recent exact-source runs.
5. Review release-critical test oracles under [#594](https://github.com/cozyGarage/BookiE/issues/594), especially policy, type/value preservation, import and transaction outcomes. Open defects before the release candidate is frozen.

### Qualification and cut

1. **Release hygiene:** recheck Project #3 and open pull requests, resolve #1585, confirm the changelog entries match merged user-visible behavior, and freeze new feature work.
2. **Desktop acceptance:** complete #574, triage release-critical items in #575, and gather the exact-source Forgejo GTK runs required to resolve #556. Fix any release-blocking issue in its owning lane, with a regression and fresh gates.
3. **Candidate qualification:** review release-critical test oracles under #594; record the candidate commit; run preflight, relevant GTK, driver/TLS/release fixture layers, packaging builds, GitHub checks and one serialized Forgejo gate. Complete install, 0.2.0-to-candidate upgrade and rollback on Arch/Hyprland. Skipped GitHub merge-tier checks are not passes.
4. **Release cut:** update the workspace version, `meson.build`, AppStream metadata and release notes together to `0.2.1`; run `scripts/check-release-version.py`, `appstreamcli validate`, link, issue-ledger, ignored-test inventory and release-note checks. Archive the previous changelog according to the [release process](release-process.md).
5. **Publish:** build every artifact from the exact candidate tag workflow, verify `SHA256SUMS` and provenance, install the tag-built Arch package, and inspect the draft. Publish only after blockers are closed or explicitly dispositioned and the maintainer approves. If the accumulated tree cannot meet patch-release qualification, keep the tag unpublished and revise the version/scope before cutting it.

Release-note boundary: summarize user-visible behavior since 0.2.0, grouped by added, changed, fixed and security impact. Do not copy test evidence or internal refactors into user-facing notes. The release workflow and [ADR 0022](decisions/0022-release-notes-from-merge-commits.md) govern the final notes.

## Standing decisions

| Topic | Canonical decision |
|---|---|
| Static drivers; Linux GTK stack; Relm4 ownership | ADRs [0001](decisions/0001-no-plugin-system.md), [0002](decisions/0002-rust-gtk4-libadwaita.md), [0003](decisions/0003-relm4-architecture.md) |
| Credentials; server cancellation; panic containment | ADRs [0004](decisions/0004-libsecret-secret-storage.md), [0005](decisions/0005-server-side-cancellation.md), [0006](decisions/0006-driver-panic-containment.md) |
| Native type/value outcomes and proof | [ADR 0007](decisions/0007-type-and-value-preservation.md) |
| Session ownership, trust and uncertain outcomes | [ADR 0008](decisions/0008-connection-and-session-ownership.md) |
| Durable identity and recoverable state | [ADR 0009](decisions/0009-persistence-and-identity-compatibility.md) |
| Cursor results and full exports | ADRs [0011](decisions/0011-paged-query-results.md), [0014](decisions/0014-full-table-export-snapshot.md) |
| Shared governed consumer path and headless services | ADRs [0016](decisions/0016-governed-consumer-seam.md), [0018](decisions/0018-headless-app-services-crate.md) |

The remaining accepted decisions are in the [ADR index](decisions/README.md). Upstream source reuse does not change these contracts implicitly.

## Working on the next packet

Choose one lane-owned issue and refresh its source pins and competing PRs. State existing coverage, the missing user case, allowed files and observable acceptance. Record a handoff before cross-lane edits. Preserve exact native values, session identity, terminal audit outcomes and safe refusals.

Use the existing [validation tiers](validation-playbook.md), with one failing-before regression for a changed behavior. Driver contracts need real-engine outcomes; visual or package claims need their installed checks. Keep GitHub cheap checks and the serialized Forgejo merge gate. Record skipped, blocked and unrun checks explicitly.

Broader type/consumer matrices, mutation triage, Debian/GNOME desktop work, Flatpak and new features continue under their existing backlog owners. See [known limitations](known-limitations.md) for user-visible restrictions and the [value evidence index](value-contracts.md) for case-specific proof.

## History

The [completion handoff](archive/bookie-0.2-completion-handoff-2026-10-18.md) preserves the immediately preceding scope, qualification recipes and packet lists. The [test and validation summary](archive/bookie-0.2-test-records-2026-10-23.md) collects historical coverage records. The older [sprint history](archive/bookie-0.2-history.md) holds earlier implementation records. Read the case needed; dated plans do not override current decisions or scheduling.
