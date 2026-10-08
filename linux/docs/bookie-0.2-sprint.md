# BookiE 0.2 active sprint

Approved 2026-09-16. Delivery branch: `linux`; source version 0.1.5; target
0.2.0. Implementation is authorized; no 0.2 release is approved.

## Current continuation plan: 2026-10-08

Code baseline: `1b16d1101a33af30acb5c695bec7f97846a788eb` on `linux` (PR #347;
fetched 2026-10-08). PR #346 adds result-footer statistics, PR #347 adds the
failed-statement copy action, PR #349 adds the action registry, and PR #354 adds
ClickHouse lost-ack coverage. PR #344 adds
PostgreSQL domain-array projections and PR #350 records the frozen-candidate
PostgreSQL rollback rerun. Since PR #315,
PRs #316, #319, #321–#323, #328–#330 refreshed architecture/B3/B4 documentation;
PRs #317–#320 added Redis binary-value, binary-key and SCAN page-boundary
coverage; PRs #322 and #328 recorded mutation audits; PR #324 added enum-leaf
metadata coverage; PR #325 added PostgreSQL timestamp-array JSON export under
`SQL, DMY`; PR #326 bounded JSON field-name mutation loops; and PR #327 recorded
shared exporter mutation results. PR #331 adds SQLite STRICT `ANY` computed
`iif()` native storage-class and typed CSV round-trip coverage; PR #333 adds
declared SQLite BOOLEAN/temporal and BLOB-affinity runtime decoder contracts,
PR #334 refreshes the known-issues baseline; PR #335 adds malformed temporal
text fallback coverage; PR #338 extends the schema-aware PostgreSQL
domain-over-enum contract to 302 layers; PR #340 adds a 512-layer stress case;
and PR #342 checks direct enum-domain query values and result metadata at both
depths. PR #344 adds domain-array projections at 302 and 512 layers and raises
the bounded SQLx type-resolution cap to support those cases. PR #345 adds a
Redis committed-write lost-ack contract across reconnect. PR #354 adds a
ClickHouse committed INSERT lost-ack contract with independent native and
materialized-view oracles. Earlier B3 work added
PostgreSQL range refusal contracts and enum grid coverage (#305–#311), MongoDB
nested null-filter parity (#312), and a MySQL lost-ack no-replay contract
(#315). These tests preserve native type/value oracles and verify refused
writes or unsaved edits do not alter database state. Details and dated
validation are in the changelog and owning boards.

The local `scripts/ci-local.sh quick` gate passed on the PR #312 working tree;
the focused MongoDB selector passed against MongoDB 7. The MySQL lost-ack
selector passed against its Docker fixture on PR #315. PRs #333 and #335 passed
preflight, GTK fast checks, both Flatpak builds, security, SonarCloud and the
Linux regression gate; Docker and installed acceptance jobs were skipped by
workflow conditions. PRs #338 and #340 passed preflight, GTK fast checks, both
Flatpak builds, security, SonarCloud and the regression gate; Docker and
installed acceptance were skipped because those PRs did not select the owning
test layers. PR #342 passed preflight, both Flatpak builds, security, SonarCloud
and the regression gate; GTK fast checks were still running when checked.
PR #343 post-merge passed preflight, both Flatpak builds, security, SonarCloud,
GTK fast checks and the regression gate; Docker and installed jobs were skipped
by workflow conditions. PR #344 passed its focused Docker contract locally and
all required hosted checks. Its first hosted run hit a transient Docker Hub
`mongo:7` image-stream error in an unrelated isolated test; the exact test passed
locally and the failed CI jobs passed on rerun. PR #345 passed the local Docker
lost-ack test, strict Clippy, formatting, docs checks and ordinary Redis
integration tier; all required hosted checks passed after syncing the branch to
PR #344. These runs do not establish B4 acceptance on a selected frozen
candidate. PR #331 local validation is recorded in its [evidence
comment](https://github.com/cozyGarage/BookiE/pull/331#issuecomment-6053023990),
PR #333 mutation/test evidence in its [discussion](https://github.com/cozyGarage/BookiE/pull/333#issuecomment-6053147574),
PR #335 evidence in its [discussion](https://github.com/cozyGarage/BookiE/pull/335#issuecomment-6053236106),
PR #338 evidence in its [discussion](https://github.com/cozyGarage/BookiE/pull/338#issuecomment-6053281386),
PR #340 evidence in its [discussion](https://github.com/cozyGarage/BookiE/pull/340#issuecomment-6053311277),
PR #342 evidence in its [discussion](https://github.com/cozyGarage/BookiE/pull/342#issuecomment-6053339776),
PR #344 local run details are in its [evidence comment](https://github.com/cozyGarage/BookiE/pull/344#issuecomment-6053519593),
and PR #345 Redis lost-ack details are in its [evidence comment](https://github.com/cozyGarage/BookiE/pull/345#issuecomment-6053620744).
These runs do not establish B4 acceptance on a selected frozen candidate.
The local focused B4 rollback layer passed on clean documentation-only commit
`73f8ce935`; its tested source files match `684ea40f`. Commands, results and
source hashes are in the [PR #314 evidence comment](https://github.com/cozyGarage/BookiE/pull/314#issuecomment-6052065028).
PR #337 added failed-batch INSERT coverage across InnoDB, MyISAM, MEMORY, CSV
and ARCHIVE. PR #353 then added direct UPDATE and DELETE coverage across InnoDB,
MyISAM, MEMORY and CSV. Its local Docker rollback layer passed; hosted PR checks
passed, while the B4 Docker job was skipped by the PR workflow condition. The
post-merge Build Linux run on `32b170f` is still pending. See the [PR #337
evidence comment](https://github.com/cozyGarage/BookiE/pull/337#issuecomment-6053382536)
and [PR #353](https://github.com/cozyGarage/BookiE/pull/353).
PR #312 skipped its B4-specific job because it only changed MongoDB tests.
These focused cases do not close B3: the broader
engine/type/consumer/configuration matrix, mutation triage, performance rows and
installed grid acceptance remain open. See the [B4 board](b4-task-board.md)
for its remaining package/Wayland and Windows AD acceptance.

B3 remains open for the broader engine/type/consumer/configuration matrix,
mutation triage and installed grid acceptance. B4 still needs the remaining
privacy, daemon retirement, headless ownership and transport/session acceptance.
B7 still needs a frozen candidate, installed Arch/Wayland and Debian/GNOME
acceptance, upgrade/rollback and soak. Case-level evidence belongs on the
[B3 board](type-contract-strategy.md), [B4 board](b4-task-board.md) and [value
evidence index](value-contracts.md); do not infer a lane pass or release
qualification from these merged scenarios.

Order: **B3 → B4 → installed Arch/Omarchy/Hyprland Wayland → Debian/GNOME
Wayland → B7 qualification**. Review/preparation may overlap with reserved
files and fixtures; a lane pass does not close a milestone.

This sprint keeps milestone order and acceptance status; source-pinned case
results live in the B3 board and value evidence index.

## Technical decisions

| Topic | Canonical decision |
| --- | --- |
| Static drivers; Linux GTK stack; component ownership | ADRs [0001](decisions/0001-no-plugin-system.md), [0002](decisions/0002-rust-gtk4-libadwaita.md), [0003](decisions/0003-relm4-architecture.md) |
| Credentials; server cancellation; panic containment | ADRs [0004](decisions/0004-libsecret-secret-storage.md), [0005](decisions/0005-server-side-cancellation.md), [0006](decisions/0006-driver-panic-containment.md) |
| Shared type/value outcomes, metadata and proof | [ADR 0007](decisions/0007-type-and-value-preservation.md) |
| Live connection/session ownership, trust and uncertainty | [ADR 0008](decisions/0008-connection-and-session-ownership.md) |
| Durable identity, migration and rollback compatibility | [ADR 0009](decisions/0009-persistence-and-identity-compatibility.md) |
| Redis topology in 0.2.0 | One host/port endpoint; Sentinel and Cluster deferred ([known-issues ledger](known-issues.md#owners-and-handoff)) |

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
| B3 type/value contracts | Focused native and consumer cases are recorded across the existing engines. SQLite STRICT `ANY` computed `iif()` now has native storage-class and typed CSV coverage (#331); declared BOOLEAN/temporal decoding, BLOB-affinity runtime classes, and malformed temporal text fallback have focused tests (#333, #335). PostgreSQL domain-over-enum metadata/query coverage now reaches 302 and 512 layers, including arrays (#338, #340, #342, #344); Redis and ClickHouse have committed-write lost-ack contracts (#345 and current B3 follow-up). U2 identity-copy behavior is implemented and covered for PostgreSQL, SQL Server and MySQL policy; PostgreSQL enum parameter inference has restricted-role coverage, and PR #249 adds a keyed grid-edit case under shadowed transaction-local `search_path`. The broader engine/type/consumer/configuration matrix, mutation triage, remaining TEST-15 paths and installed grid acceptance remain open. | [Type/consumer board](type-contract-strategy.md), [B3 findings](archive/b3-review-2026-10-01.md), [value evidence index](value-contracts.md) |
| B4 transport/sessions | SSH audit, rollback-failure, Kerberos, GTK trust-flow, prompt-timeout, cross-engine DML regressions and B4-11 failed-batch INSERT/UPDATE/DELETE coverage (#337, #353) are merged. | [Current B4 board](b4-task-board.md): frozen-candidate and hosted/installed acceptance remains open for the listed rows; B4-11 other engines/effects, B4-12 next-candidate rollback proof, B4-21 Windows AD interoperability, and UI-1b chain editing remain |
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
stay open. U1 server-owned columns have native consumer coverage for metadata,
grid editability, CSV import, Copy as SQL and SQL export replay
(`value_contract_mssql_server_owned_columns_use_native_defaults_across_consumers`);
the grid assertion is helper-level, while installed GTK interaction remains
part of release acceptance. U2–U6 and older-release O1–O3
are mapped in [main review](archive/upstream-main-review-2026-10-03.md) and
[older-release review](archive/upstream-older-releases-review-2026-10-03.md). Reuse these
owners; do not create a second completion cache, exporter or type policy.

## B4 acceptance worklist (2026-10-08)

This is the next acceptance slice on current `linux` tip `32b170f`. Prior runs
remain useful evidence for their exact SHAs, but do not substitute for current
frozen-candidate, hosted or installed acceptance where required. The SSH GTK
artifacts below prove staged release-binary behavior under Xvfb/AT-SPI; they do
not prove a distribution-package installation or native Wayland session.

| Item | Current evidence and action |
| --- | --- |
| B4-7, B4-16 | Keep UNVERIFIED. Run affected layers on the selected frozen candidate, confirm hosted results for that SHA, then complete installed-package/native Wayland acceptance. Post-merge Build Linux run `37743149992` on `32b170f` is pending; the B4 rollback job has not started. |
| B4-9 | The full local driver TLS matrix passed twice on `a47b1fb` (48/48); see [the PR evidence comment](https://github.com/cozyGarage/BookiE/pull/329#issuecomment-6052925352). The earlier MongoDB `ConnectionRefused` failures on `98134709` did not recur; cause remains unknown. Repeat on the selected frozen SHA, confirm hosted results, and complete installed-package/native Wayland acceptance. |
| B4-22 | Keep UNVERIFIED. The GTK bundle export/import and encrypted credential round-trip have local and earlier hosted evidence; obtain acceptance on the selected frozen candidate, hosted SHA, and installed package/native Wayland. |
| B4-12 | The focused PostgreSQL backend-termination case passed again on frozen candidate `7eea6f09d7154f03400e82cec7c16215488b2115`; see the [PR discussion](https://github.com/cozyGarage/BookiE/pull/350#issuecomment-6054276678). Exact-candidate hosted coverage and a rerun on any next selected candidate remain open. |
| B4-11 | [PR #337](https://github.com/cozyGarage/BookiE/pull/337#issuecomment-6053382536) covers direct failed-batch INSERTs on InnoDB, MyISAM, MEMORY, CSV and ARCHIVE. [PR #353](https://github.com/cozyGarage/BookiE/pull/353) adds direct UPDATE and DELETE coverage on InnoDB, MyISAM, MEMORY and CSV. The local rollback layer passed; other storage engines and additional side-effect patterns remain open. |
| B4-17 | Reconcile existing `ssh-gtk-*` artifacts before rerunning: they already show both-hop trust prompts, routed query, second-hop decline without learning, changed-key refusal, and terminal audit outcomes. They use staged release binaries under Xvfb/AT-SPI. Close only after selected-candidate/hosted evidence and the required installed-package/native Wayland trust flow. |
| B4-21 | Keep Windows AD interoperability open. Samba AD Kerberos+TLS fixtures are useful local coverage but do not establish interoperability with Windows AD; candidate acceptance is also outstanding. |
| UI-1b | Keep refusing edits to saved SSH jump chains. Implement the per-hop secret editor design across persistence, transport identity, bundle compatibility and GTK before enabling chain editing. |

Recheck the remote tip before each candidate run; do not reuse a stale SHA or
reapply work already merged.

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
