# BookiE 0.2 active sprint

Approved 2026-09-16. Consolidated 2026-10-03 from source/document checkpoint
`0cf70382e`; status reconciled against `aeac107a4`. Delivery branch: `linux`; source version 0.1.5; target 0.2.0.
Implementation is authorized; no 0.2 release is approved.
During consolidation, `573435766` merged F4/F9 and `8c0f17494` updated the B4 board;
they are preserved in this checkout. The history files retain the earlier `0cf70382e` snapshot.

## Current continuation plan: 2026-10-07

Current integrated source baseline: `e47dbbbc2` on `linux` (PR #183).
Merges #146–#149 add PostgreSQL `name[]` decoding, typed rebinding, grid edits
and escaped/boundary CSV round trips; #150 adds tunneled TLS fixtures for MySQL
and SQL Server; #151 adds the daemon system-OpenSSH trust flow and headless
audit-generation regressions. Earlier merges #138–#145 cover result budgets,
PostgreSQL enum/array cases and the GTK AT-SPI row-action wait. PR #153
reconciles hosted preflight/harness findings and SSH fixture setup; #154 and
#156 add PostgreSQL enum-array slice and range-array coverage. PR #157 adds B4
SSH audit, rollback-failure and Kerberos cases; #159 reconciles its hosted CI
findings. PRs #158 and #160 add text-search and JSON array refusal contracts;
#164 covers all six built-in range arrays; #166 adds seven geometric array
families. PRs #161 and #162 fix PostgreSQL release GTK dependencies and the
safety fixture setup. PR #165 hardens GTK SSH trust role detection and the
two-hop tunnel-loss setup. PR #163 fixes the SSH trust-prompt timeout and
merged; PR #170 then preserved the remaining handshake deadline after the
trust decision. PR #169 implements the U2 default-copy policy: PostgreSQL and
SQL Server generate fresh identity values, while MySQL preserves explicit
auto-increment values. PR #171 removes an unused fixture import exposed by the
first hosted preflight. PR #172 reconciles sprint evidence after #171. PR #173
ends built-in and system-OpenSSH handshakes when the host-key decision is
declined or the trust dialog is closed; PR #175 adds Escape dismissal. The
merged #173 Linux run exposed two follow-up regressions: GTK signal ordering
could send a close-as-decline before the explicit trust response, and the
installed Debian file chooser lacks the AT-SPI `show_location` action. PR #176
defers close-as-decline to the GLib idle loop, uses Ctrl+L when that chooser
action is absent, and adds native MySQL SQL-export and Copy-as-INSERT identity
contracts. The focused PostgreSQL two-hop trust scenario, local quick CI and the
complete Debian testing floor run (app library, GTK widgets and installed GTK
safety suite) pass on the integrated source. The Debian run uses Xvfb; it does
not replace GNOME/Wayland acceptance. These fixes close the #173 regressions but
do not qualify B3 or the release. See PR #176 for command-level evidence.
Since #176, #177 adds font preference, keyboard history navigation and a
two-profile bundle test; #178 hardens the GTK file-chooser accessibility wait;
#179 reconciles sprint status; #180 splits PR and merge CI tiers; #181 and #182
refresh B4 bundle-audit evidence and correct the B4 SSH-audit coverage record;
#183 adds loaded-row search and MSSQL maximum-value/restart coverage. The exact
local bundle flows are linked from the [B4 board](b4-task-board.md); hosted,
installed and frozen-candidate evidence remains distinct from those local runs.
Case details and evidence belong in the [B3
board](type-contract-strategy.md), [B4 board](b4-task-board.md), [value evidence
index](value-contracts.md), test sources and relevant PR comments. B3 remains
open: the broader engine/type/consumer/configuration matrix, mutation triage and
installed grid acceptance still need work. The scoped XLSX writer mutation run
caught 41/41 mutants; broad core/package coverage remains open. These changes
do not qualify the release.

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
| B3 type/value contracts | Focused native and consumer cases are recorded across the existing engines. U2 identity-copy behavior is implemented and covered for PostgreSQL, SQL Server and MySQL policy; enum text/NULL inference and native-type mismatch refusals have PostgreSQL coverage. The broader engine/type/consumer/configuration matrix, mutation triage and installed grid acceptance remain open. | [Type/consumer board](type-contract-strategy.md), [B3 findings](archive/b3-review-2026-10-01.md), [value evidence index](value-contracts.md) |
| B4 transport/sessions | C6/G5, SSH audit, rollback-failure, Kerberos, GTK trust-flow and prompt-timeout fixes are merged through #170. | [Current B4 board](b4-task-board.md): frozen B3+B4 candidate, hosted/installed acceptance, privacy and headless ownership acceptance remain open |
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
