# BookiE 0.2 completion and continuation

## Delivery status

The maintainer marked 0.2.0 delivery complete, and the release was published on 2026-10-23. The [platform guide](platforms.md#published-020-release) records its assets and package names. Open work and issue acceptance live in [Project #3](https://github.com/users/cozyGarage/projects/3) and the linked issues.

## Delivery boundary

0.2.0 is the Linux release for the Arch/Hyprland target, with hotfixes to follow from use:

- PostgreSQL, MySQL/MariaDB and SQLite are the principal database paths; DuckDB is optional. SQL Server, ClickHouse, MongoDB and Redis retain the maturity and restrictions in [known limitations](known-limitations.md).
- GUI and agent operations share the governed policy/audit path. Lost write acknowledgement does not authorize replay, and agent output remains subject to masking.
- Arch/Hyprland was the desktop target for 0.2.0. Debian/Ubuntu GNOME acceptance and Flathub distribution were outside that release. The release page also carries a GNOME 50 Flatpak bundle; it does not establish Flathub publication or acceptance against the later GNOME 51 runtime.

The app and agentd manifests identify source version `0.2.0`. The current AppStream entry describes the published 0.2.0 release. The `bookie` Debian package name landed after the release tag and is not included in its published artifacts. These later changes do not reopen the completed milestone.

## Current continuation

[Project #3](https://github.com/users/cozyGarage/projects/3) owns issue status and Lane; the [ledger index](known-issues.md) maps IDs to issues. Existing open correctness, qualification and infrastructure issues keep their evidence and follow-up scope. They are not silently closed by a milestone-status change.

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

The remaining accepted decisions are in the [ADR index](decisions/README.md). Code reuse does not change these contracts implicitly.

## Working on the next packet

Choose one lane-owned issue and refresh its source pins and competing PRs. State existing coverage, the missing user case, allowed files and observable acceptance. Record a handoff before cross-lane edits. Preserve exact native values, session identity, terminal audit outcomes and safe refusals.

Use the existing [validation tiers](validation-playbook.md), with one failing-before regression for a changed behavior. Driver contracts need real-engine outcomes; visual or package claims need their installed checks. Require exact-revision checks and record skipped, blocked and unrun checks explicitly.

Open work is selected from Project #3 and remains subject to issue-owned acceptance. See [known limitations](known-limitations.md) for user-visible restrictions and the [value evidence index](value-contracts.md) for case-specific proof.
