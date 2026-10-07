# BookiE 0.2 active sprint

Approved 2026-09-16. Consolidated 2026-10-03 from source/document checkpoint
`0cf70382e`; status reconciled against `aeac107a4`. Delivery branch: `linux`; source version 0.1.5; target 0.2.0.
Implementation is authorized; no 0.2 release is approved.
During consolidation, `573435766` merged F4/F9 and `8c0f17494` updated the B4 board;
they are preserved in this checkout. The history files retain the earlier `0cf70382e` snapshot.

## Current continuation plan: 2026-10-07

Current integrated source baseline: `5f7e80145` (PR #123) on `linux`, pushed to
`origin/linux`. Merges #103/#105 added mixed-case enum
shadow-name and MongoDB stale-field delete coverage; #106/#107 refined Rust
formatting and full-suite determinism. Merges #109/#111 made panic log capture
safe under forced-color CI; #110 moved the transaction-local enum evidence to
the merged PR #108 comment; #112 covers a missing target schema in transaction
`search_path`; #113 records the B3 docs and evidence comments; #114 covers the
same missing-schema case under restricted session and login roles; #115 advances
the GUI phase 2 work; #117 adds the mixed-case quoted target under a restricted
role with its schema absent from `search_path`. Earlier October 6 merges #65–#67,
#81 and #82 retain the DuckDB, SQLite and ClickHouse cases described below.
Their implementations and source-pinned evidence are linked from the [B3 board](type-contract-strategy.md)
and [value evidence index](value-contracts.md). B3 remains open: the broader
engine/type/consumer/configuration matrix, mutation triage and installed grid
acceptance still need work. PR #118 merged PostgreSQL `xml[]` support across
results, binding, grid edits and CSV restore. These changes do not qualify the
release.

Order: **B3 → B4 → installed Arch/Omarchy/Hyprland Wayland → Debian/GNOME
Wayland → B7 qualification**. Review/preparation may overlap with reserved
files and fixtures; a lane pass does not close a milestone.

Case-level progress belongs on the [B3 work board](type-contract-strategy.md);
source-pinned results live in the [value evidence index](value-contracts.md)
and its linked manifests. This sprint keeps milestone order and acceptance
status, not a second dated log of completed scenarios.

## Technical decisions

| Topic | Canonical decision |
| --- | --- |
| Static drivers; Linux GTK stack; component ownership | ADRs [0001](decisions/0001-no-plugin-system.md), [0002](decisions/0002-rust-gtk4-libadwaita.md), [0003](decisions/0003-relm4-architecture.md) |
| Credentials; server cancellation; panic containment | ADRs [0004](decisions/0004-libsecret-secret-storage.md), [0005](decisions/0005-server-side-cancellation.md), [0006](decisions/0006-driver-panic-containment.md) |
| Shared type/value outcomes, metadata and proof | [ADR 0007](decisions/0007-type-and-value-preservation.md) |
| Live connection/session ownership, trust and uncertainty | [ADR 0008](decisions/0008-connection-and-session-ownership.md) |
| Durable identity, migration and rollback compatibility | [ADR 0009](decisions/0009-persistence-and-identity-compatibility.md) |

Plans and case evidence apply these decisions; they do not define another
conversion, transport or persistence policy. Internal contracts update owning
wrappers/consumers together. Engine expansion such as Oracle remains outside
this existing-eight-driver stabilization scope.

## Current milestone status

| Milestone | Established scope | Remaining acceptance / owner |
| --- | --- | --- |
| A1–A4 | Prior correctness, drafts/planning, Jump to Column and BookiE branding implemented | Historical 0.1.x proof does not qualify 0.2; A5 installed candidate work folds into B7 |
| B1 platform/build | Rust 1.98, GNOME 50, SQLx/system SQLite, resources and dev profiles integrated | Installed Arch then Debian/GNOME qualification; full Flatpak qualification separate |
| B2 runtime/storage | Owned tasks/stores, migrations, GSettings mirrors and coalesced writers implemented | Installed upgrade/rollback and shutdown acceptance in B7 |
| B3 type/value contracts | Focused native and consumer cases are recorded across the existing engines. Merges #81/#82 add SQLite STRICT `ANY` `length()` typed-CSV round trips and ClickHouse nested `Map → Array → Tuple → UInt128` oracle/refusal coverage; #112 adds a mixed-case PostgreSQL enum case with its target schema absent from transaction-local `search_path`; #122/#123 add PostgreSQL domain-over-array metadata and its regression coverage. Prior DuckDB malformed-tail and decimal fixes remain covered. B3 remains open pending the broader engine/type/consumer/configuration matrix, mutation triage and installed grid acceptance. | [Type/consumer board](type-contract-strategy.md), [B3 findings](archive/b3-review-2026-10-01.md), [value evidence index](value-contracts.md) |
| B4 transport/sessions | Policy, daemon cache refusal, editor callback/retirement and awaited cleanup patches, built-in host consent and GUI uncertainty split merged through `6346a431c` | [Current B4 board](b4-task-board.md#b4-continuation-status-october-3): F4/F9, F6 and GUI F8 implementation are merged; MySQL DDL refusal, honest rollback reporting and the MyISAM-trigger rollback boundary have local regressions; headless generation/retirement parity, TLS/daemon/route/audit, PostgreSQL rollback-failure acceptance and installed acceptance remain open |
| B5 editor/files | Open/Save/Save As, changed-on-disk detection and file relinking implemented | Installed file-dialog/recovery/dirty-close flows |
| B6 PostgreSQL catalog | Guarded read-only catalog/types implemented | Restricted-role, stale-owner and installed catalog flows |
| B7 qualification | Open | Frozen SHA, affected automated gates, both installed desktop targets and retry-free soak; publication separate |

This table summarizes source/evidence ownership, not a fresh runtime pass.
Consult each exact case/SHA; do not promote a whole type or engine from one test.

Detailed case outcomes and exact proofs are maintained by the [B3 board](type-contract-strategy.md), [value evidence index](value-contracts.md), and linked manifests. Historical test runs are consulted from the evidence/history records; they are not repeated in the active sprint.

## B3 work packets

Use [ADR 0007](decisions/0007-type-and-value-preservation.md) for the standard,
[the board](type-contract-strategy.md) for remaining targets and
[the evidence index](value-contracts.md) for proof lookup. Choose one bounded
engine/type/consumer/configuration case; preserve existing fallbacks/refusals.

| Packet | Deliverable |
| --- | --- |
| B3-P1 coverage | Reconcile one remaining row against source/tests; return the smallest uncovered case and existing SHA/selector |
| B3-P2 native boundary | One decoder/calendar/array/type boundary with an independent native oracle and affected consumer proof |
| B3-P3 consumer parity | One format's value/type, NULL/empty, numeric, temporal, binary or nested round trip; destination survives refusal |
| B3-P4 grid/bind | One typed edit/binding path; native persisted kind/value, full row key and untouched siblings |
| B3-P5 delivery/SQL | One malformed-tail, zero-row, cap, multi-result or partial-stream case; identity/order/completeness and handle state |
| B3-P6 mutation | One relevant survivor/timeout group; independent assertions and scoped rerun; unavailable output stays unproven |


R1–R5/R7 have recorded fixes; R6 evidence portability and the remaining matrix
stay open. U1 metadata/two INSERT paths have [native proof](archive/upstream-u1-sql-server-2026-10-03.md);
other consumers/ledger acceptance remain open. U2–U6 and older-release O1–O3
are mapped in [main review](archive/upstream-main-review-2026-10-03.md) and
[older-release review](archive/upstream-older-releases-review-2026-10-03.md). Reuse these
owners; do not create a second completion cache, exporter or type policy.

## B4 next order

Use the [board](b4-task-board.md) for exact task ownership, prerequisites and
integration evidence. Its October 3 checkpoint supersedes the old isolated
worktree descriptions. Recheck HEAD before starting; do not reapply merged work.

| Order | Work |
| --- | --- |
| 1 | Confirm integrated E1/E2, G3 and F1/F3/F5/F2 acceptance; finish C6 per-engine tunneled TLS |
| 2 | Retain merged F4/F9 reconnect/session invalidation; verify F6 native trust, finish G5 unattended daemon behavior and I2 route refusal |
| 3 | F8 headless generation parity, I5 transport audit, I3 evidence/docs, F7 isolated GTK lifecycle flow after prerequisites |
| 4 | Combined affected gates, then installed Arch/Wayland acceptance |
| After Arch | I1 Debian packaging and the required GNOME/Wayland pass |

The [architecture review](archive/architecture-consistency-review-2026-10-03.md#remaining-source-risks)
records panic privacy and headless retirement gaps alongside these owners.

## Arch / Omarchy / Wayland UI packets

Use [manual acceptance](manual-verification-0.2-features.md) and
[the platform guide](platforms.md#arch-and-omarchy-package). This is application work; no desktop/system
configuration changes are requested by the sprint.

| Packet | Acceptance |
| --- | --- |
| UI-A1 connections/SSH | Driver defaults and saved values, narrow layout, focus/Enter/Escape, trust decline without writes |
| UI-A2 editor/session | Stop, Session, reconnect, every close/disconnect route, files/recovery; native database/audit postconditions |
| UI-A3 grid/export/catalog | Full cell values and typed edits, import/export, shortcuts, empty/error/denied states and stale/restricted-role catalog |
| UI-A4 installed candidate | Arch install/upgrade/rollback, aliases/resources/askpass/GSettings, profile isolation and recoverable durable data |

Record binary/package SHA and checksum, actual Wayland backend, desktop/library
versions, scale/monitor setup and light/dark screenshots. Check keyboard,
clipboard, popovers, resizing and scaling. Xvfb regressions do not qualify Wayland.

## Required next phase: GNOME on Debian Wayland

After UI-A1–A4, complete Debian I1 and repeat the applicable installed workflows
on GNOME/Wayland with libraries meeting ADR 0002. Record the actual Debian/library
versions; do not assume stable Debian meets GNOME 50 requirements. UI-D1 proves
package contents; UI-D2 repeats A1–A3; UI-D3 proves upgrade/rollback; UI-D4 resolves
platform differences. Source changes require a new SHA and affected Arch reruns.
This phase is required, not a prerequisite for current B3/B4 development.

## Qualification and handoff

B7 requires a frozen clean SHA, affected automated gates, installed Arch and
Debian/GNOME checks, and **30 consecutive retry-free GTK attempts across at least
six runs** at that SHA. Failed/blocked/not-run items stay open. No recipe, version
bump, older package or green task grants release approval.

Use the [agent task template](validation-playbook.md#agent-task-template): one
invariant/case, allowed files, exact baseline, selected layers and reserved build/
fixture resources. Return initial behavior, patch, selectors/results, report
paths, source fingerprints, unresolved gaps and resulting SHA. Serialize shared
Cargo/Docker work; preserve warm build reuse per [toolchains](platforms.md#rust-toolchain).
Do not infer new passes from test-source inspection. Documentation packets use
diff/link checks; runtime/package claims require their actual owning checks.

## History lookup

[Sprint history](archive/bookie-0.2-history.md) preserves original scope, dated progress,
commit tables, build-cache measurements and rollback details. Read a relevant
section only. New current work updates this sprint/owning board; new case results
update the value ledger, with links here instead of copied logs/counts.
