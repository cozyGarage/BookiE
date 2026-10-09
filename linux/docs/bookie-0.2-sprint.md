# BookiE 0.2 active sprint

Approved 2026-09-16. Delivery branch: `linux`; source version 0.1.6-dev (0.1.5 is released); target
0.2.0. Implementation is authorized; no 0.2 release is approved.


## 0.2.0 readiness: 2026-10-09

No 0.2 release is approved. Before a frozen candidate SHA, these stay open
(owner in brackets; details in the [ledger](known-issues.md)):

- B3 [B3]: the broader type, consumer and configuration matrix, PERF-10 memory
  reduction (the preview is display-only; full values are still held), PERF-3
  MongoDB census, UI-19 (deferred), mutation triage, installed grid acceptance.
  TEST-29's fast and durable test-container paths are in PR #457. The focused
  same-session MySQL/MariaDB tests and local preflight pass. Local commits
  `559ba69` and `c0eb82f` restore the duplicate-key failure scenario and wait
  for authenticated MongoDB readiness; neither is pushed yet.
  Forgejo run 99 completed red on the older published head `ab4b273`; its
  PostgreSQL query-plan, MongoDB readiness, Redis TLS and SQL Server startup
  jobs failed. The final local head is queued behind UX's run 100.
- B4 [B4]: B4-12 and B4-17 frozen-candidate and installed acceptance, TEST-15
  remaining write paths, AUD-2 approval timeout, PERF-2 and PERF-8 cursor paging
  (ADR 0011), UI-13b guarded read snapshot for full export (ADR 0014).
- Desktop acceptance [maintainer]: PKG-1 native Wayland on Arch, PKG-2
  Debian/GNOME, PKG-4 Flathub submission and screenshots, PKG-8 the Flatpak
  file-access decision, TEST-6 the 107-item manual checklist.
- B7: a frozen SHA, the affected gates green on Forgejo at that SHA (the
  acceptance gate since 2026-10-08), installed checks on Arch and Debian, and the
  30-attempt soak (Forgejo nightly `gtk-soak`).

## Current continuation: 2026-10-09

The merged `linux` baseline is `97e5f55bddacdfef427c51f162917fedcace3a9f`
(PR #451, including #452); PRs #441–#452 are merged. PR #443 adds SQL Server
`decimal(38,38)` preservation and refuses lossy `sql_variant` values. Its full
MSSQL run passed 77 tests; the full local value-contract tier passed 428 tests
with no missing suites, and unpiped preflight passed. The scoped SQL Server
codec mutation audit tested 38 variants: 32 caught, 6 build-unviable, none
missed or timed out. PR #448 adds deep PostgreSQL enum expression and array
result boundaries through 4,096 domain layers; its Docker contract, local
preflight and all 11 required hosted checks passed. PR #449 adds
`array_remove`, `array_prepend`, `array_position` and `array_positions`
scalar-parameter behavior at 63, 64 and deeper enum-domain levels; its Docker
contract, unpiped preflight and all 11 required hosted checks passed. A focused
17-variant MySQL `decode_by_type` audit on the current source caught all 16
buildable mutants across native selectors; one was build-unviable. PR #450 adds
MySQL and MariaDB invalid-UTF-8 ENUM/SET result regressions. Its GitHub checks
passed, but Docker driver integration was skipped and Forgejo run 85 was
disrupted by executor restarts. A follow-up branch adds native stored-value
oracles and shared MariaDB fixtures for the serial full suite. Both charset
selectors and all 88 serialized MySQL integration tests passed locally using
one MySQL and one MariaDB container (66.36 s); unpiped preflight passed. The branch was
published at `f29743c`; Forgejo run 88 failed on that pre-#452 tip. The
PostgreSQL failure was a disconnect regression checking for its tagged backend
before it was visible in `pg_stat_activity`. Commit `b98ce22`
reuses bounded polling in the affected backend-termination tests; the local
PostgreSQL disconnection module passed 10/10 tests, its full serialized
integration suite passed 234/234 in 120.91 s with one server, and unpiped
preflight passed on the current head. Ubuntu and Debian also failed GTK safety waits for
`open_editor` and `audit_failure_denies`; their cause remains unclassified and
needs separate UI follow-up. The updated local head still needs its own gate;
run 88 predates the current B3 fix and the #451/#452 merges.
Merged topic branches have been deleted.

For TEST-29, the local full PostgreSQL integration binary passed 234/234 in
133.16 seconds and the MySQL/MariaDB binary passed 90/90 in 55.50 seconds.
SQL Server's DDL batch, durable lost-ack and restart tests, and fast-fixture wide
numeric test passed. PR #457 is open on GitHub at published head `ab4b273`, with
the latest `origin/linux` merged. All GitHub checks for that head passed. The
same-session MySQL/MariaDB checks pass 4/4, and local unpiped preflight passes.
Forgejo run 99 completed red on `ab4b273`: `driver (mongodb)` got
`ConnectionRefused` before authentication, `pg-release` redacted the wildcard
query plan, `driver-tls` got `ConnectionRefused` from its Redis TLS fixture, and
one SQL Server testcontainer timed out during startup. MySQL passed 90/90; all
GTK shards, Postgres e2e and both distro floors passed. Local commit `559ba69`
keeps the duplicate-key failure cases in dedicated sessions, and `c0eb82f`
adds a bounded readiness check that passes locally. Current local head
`9549634` is queued behind UX run 100; it will push and gate only after that run
releases the shared lock. Run 97 was canceled as a duplicate.

B3 remains open after #450: continue the wider engine/type/consumer/configuration
matrix and TEST-2 mutation triage, then complete installed grid acceptance. B4
and B7 remain open as listed above; keep the sequence B3 → B4 → installed
Arch/Wayland → Debian/GNOME → B7. The merged baseline is not release-qualified.

Feature gaps against other clients are in
[the feature comparison](0.2-feature-comparison.md); none is a 0.2 blocker unless
the maintainer adds it.

## Historical continuation snapshot: 2026-10-08

Code baseline: `0bb8e34354d13e3ab9911b9db710eddb02115170` on `linux`, checked
2026-10-08.
Merged since the 0.1.5 release, by lane:

- **UX:** result headers show type and key markers (#289), selection sum and
  average (#346), a Copy error button (#347), a connection-coloured workspace
  tab strip (#348), elapsed time without the approval wait (#364), line and
  column of PostgreSQL statement errors (#366), pinned results and per-statement
  gutter marks (#372), a collapsible sidebar tree with counts and saved state
  (#373), and installed scenarios for restore after a deleted connection and a
  second-profile bundle import (#375), package upgrade checks from the released
  0.1.5 deb and Arch package (#377, #381) and a Flatpak window check through
  AT-SPI (#380). The single action table is deferred
  behind the drift tests (#349, docs only).
- **B3 values and drivers:** PostgreSQL range refusal and enum grid coverage
  (#305–#311), MongoDB nested null-filter parity (#312), SQLite STRICT `ANY`
  computed `iif()` (#331), declared BOOLEAN/temporal and BLOB-affinity decoders
  (#333), malformed temporal text fallback (#335), schema-aware domain-over-enum
  at 302 and 512 layers (#338, #340), direct enum-domain query metadata (#342),
  domain-array projections and a higher bounded type-resolution cap (#344),
  enum-leaf metadata (#324), timestamp-array JSON export under `SQL, DMY` (#325),
  SQLite affinity for enum-like declared types (#359).
- **B3/B4 lost-ack and rollback:** MySQL lost-ack no-replay (#315), Redis (#345),
  ClickHouse (#354), SQL Server (#355); MySQL failed-batch INSERT (#337) and
  direct UPDATE/DELETE (#353) across storage engines; frozen-candidate
  PostgreSQL rollback rerun (#350); Redis binary-value, binary-key and SCAN
  page-boundary coverage (#317–#320).
- **Mutation audits and test hygiene:** #322, #326–#328.
- **CI:** GTK soak setup (#357, #358), serialized GTK tunnel-loss assertions
  (#360), Forgejo parallel merge tier (#288).

The PR workflow skips Docker driver integration; the post-merge merge-tier run
`37752816805` passed. Post-merge Build Linux run `37743149992` on `32b170f` was
cancelled, so that SHA has no hosted result. None of these runs establish B4
acceptance on a selected frozen candidate. Per-PR commands and results are in
the PR evidence comments: [#314](https://github.com/cozyGarage/BookiE/pull/314#issuecomment-6052065028),
[#331](https://github.com/cozyGarage/BookiE/pull/331#issuecomment-6053023990),
[#333](https://github.com/cozyGarage/BookiE/pull/333#issuecomment-6053147574),
[#335](https://github.com/cozyGarage/BookiE/pull/335#issuecomment-6053236106),
[#337](https://github.com/cozyGarage/BookiE/pull/337#issuecomment-6053382536),
[#338](https://github.com/cozyGarage/BookiE/pull/338#issuecomment-6053281386),
[#340](https://github.com/cozyGarage/BookiE/pull/340#issuecomment-6053311277),
[#342](https://github.com/cozyGarage/BookiE/pull/342#issuecomment-6053339776),
[#344](https://github.com/cozyGarage/BookiE/pull/344#issuecomment-6053519593),
[#345](https://github.com/cozyGarage/BookiE/pull/345#issuecomment-6053620744),
[#355](https://github.com/cozyGarage/BookiE/pull/355#issuecomment-6055451200).

[B3 PR #365](https://github.com/cozyGarage/BookiE/pull/365) merged with
MySQL/MariaDB ENUM/SET malformed-metadata refusal and consumer regressions.
[PR #369](https://github.com/cozyGarage/BookiE/pull/369) also merged with the
PostgreSQL 513-layer enum-domain checkpoint; see the [B3 board](type-contract-strategy.md)
for contract details. B3 remains open for the broader
engine/type/consumer/configuration matrix,
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
| B3 type/value contracts | Focused native and consumer cases are recorded across the existing engines. SQLite STRICT `ANY` computed `iif()` now has native storage-class and typed CSV coverage (#331); declared BOOLEAN/temporal decoding, BLOB-affinity runtime classes, malformed temporal text fallback, and enum-like NUMERIC-affinity grid/CSV parsing have focused tests (#333, #335, #359). PostgreSQL domain-over-enum metadata/query coverage now reaches 302 and 512 layers, including arrays (#338, #340, #342, #344); PostgreSQL, MySQL, Redis, ClickHouse and SQL Server have committed-write lost-ack contracts (#315, #345, #354, #355); MongoDB now covers a dropped post-commit update acknowledgement with a native row oracle and `system.profile` replay oracle (`a_committed_update_with_a_lost_ack_is_not_replayed_after_reconnect`). U2 identity-copy behavior is implemented and covered for PostgreSQL, SQL Server and MySQL policy; PostgreSQL enum parameter inference has restricted-role coverage, and PR #249 adds a keyed grid-edit case under shadowed transaction-local `search_path`. The broader engine/type/consumer/configuration matrix, mutation triage, remaining TEST-15 paths and installed grid acceptance remain open. | [Type/consumer board](type-contract-strategy.md), [B3 findings](archive/b3-review-2026-10-01.md), [value evidence index](value-contracts.md) |
| B4 transport/sessions | SSH audit, rollback-failure, Kerberos, GTK trust-flow, prompt-timeout, cross-engine DML regressions and B4-11 failed-batch INSERT/UPDATE/DELETE coverage (#337, #353) are merged. | [Current B4 board](b4-task-board.md): frozen-candidate and hosted/installed acceptance remains open for the listed rows; B4-11 broader engine/effect coverage, B4-21 Windows AD interoperability, and UI-1b chain editing remain. B4-12's scoped frozen-source rollback acceptance is complete; full-candidate and package qualification remain separate |
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

This is the next acceptance slice on current `fork/linux` tip `e697824`.
Build Linux #37797466945 completed successfully on `c3122d9`. The later run
[#37806196821](https://github.com/cozyGarage/BookiE/actions/runs/37806196821)
completed successfully on `896f1b3`, including B4 rollback, PostgreSQL release,
Driver TLS, driver/SSH integration, installed GTK safety smoke and the
regression gate; scheduled Clippy was skipped. Build Linux
[#37849452394](https://github.com/cozyGarage/BookiE/actions/runs/37849452394)
is pending on `e697824`; Linux Security #37849452452 passed and Flatpak
[#37849452274](https://github.com/cozyGarage/BookiE/actions/runs/37849452274)
is in progress. No completed current-tip Linux CI contracts run was visible.
The B4 Docker rollback job was skipped on the PR events for #407/#408. Earlier
Build Linux #37847211229 on `7be233d` and #37844691724 on `ae3a1e0` are historical
and superseded. Hosted checks do not substitute for exact
frozen-candidate or native Wayland acceptance. The SSH GTK artifacts below
prove staged release-binary behavior under Xvfb/AT-SPI; they do not prove a
distribution-package installation or native Wayland session.

| Item | Current evidence and action |
| --- | --- |
| B4-7, B4-16 | Keep UNVERIFIED. Local SSH/GTK trust, audit and tunnel-loss scenarios passed on `c3122d9`; hosted PostgreSQL release and driver/SSH integration passed in #37797466945 and #37806196821. Frozen-candidate and installed-package/native Wayland acceptance remain open. |
| B4-9 | The full local driver TLS matrix passed twice on `a47b1fb` (48/48); see [the PR evidence comment](https://github.com/cozyGarage/BookiE/pull/329#issuecomment-6052925352). Hosted Driver TLS passed on `c3122d9` in #37797466945 and `896f1b3` in #37806196821. The earlier MongoDB `ConnectionRefused` failures on `98134709` did not recur; cause remains unknown. Frozen-candidate and installed-package/native Wayland acceptance remain open. |
| B4-22 | Keep UNVERIFIED. Five bundle/export GTK scenarios passed locally with a staged `a7f14fa` binary. Build Linux #37806196821 passed a generic installed GTK safety smoke on `896f1b3`, not the bundle flow. Obtain frozen-candidate bundle acceptance and installed-package/native Wayland evidence. |
| B4-12 | DONE for the scoped frozen-candidate rollback acceptance: the backend-termination selector passed on candidate source `a7f14fa` (1 test) and in hosted B4 rollback jobs #37797466945 and #37806196821. It verifies `TransactionRollbackFailed`, the failing statement index and disconnect errors, absent transactional row/trigger effects after reconnect, and expected sequence advancement. Full-candidate and package qualification remain separate; see [candidate evidence](evidence/b4-rollback-frozen-candidate-2026-10-08/manifest.json). |
| B4-11 | Existing direct failed-batch coverage includes INSERTs on InnoDB, MyISAM, MEMORY, CSV and ARCHIVE ([PR #337](https://github.com/cozyGarage/BookiE/pull/337#issuecomment-6053382536)); UPDATE and DELETE on InnoDB, MyISAM, MEMORY and CSV ([PR #353](https://github.com/cozyGarage/BookiE/pull/353)); and Aria direct DML and trigger effects. PR #408 adds a BLACKHOLE trigger sink; after rebasing onto `e697824`, the full `mysql_atomic` selector passed locally (13 passed, 66 filtered, 143.42 s) on test tree `e267ba0`. Its hosted checks remain in progress and the PR-event Docker rollback job was skipped. Other storage engines and side-effect patterns remain open; see [PR #408](https://github.com/cozyGarage/BookiE/pull/408). |
| B4-17 | Existing `ssh-gtk-*` artifacts show both-hop trust prompts, routed query, second-hop decline without learning, changed-key refusal and terminal audit outcomes using staged release binaries under Xvfb/AT-SPI. Build Linux #37806196821 passed the PostgreSQL release and driver/SSH integration jobs on `896f1b3`; the installed GTK safety smoke is not native SSH trust acceptance. A staged-candidate Wayland probe did not establish host-key refusal; see [probe evidence](evidence/b4-ssh-native-wayland-2026-10-08/manifest.json). Close only after selected-candidate and installed-package/native Wayland trust flow. |
| B4-21 | Keep Windows AD interoperability open. Samba AD Kerberos+TLS fixtures are useful local coverage but do not establish interoperability with Windows AD; candidate and distribution-package acceptance are also outstanding. |
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
