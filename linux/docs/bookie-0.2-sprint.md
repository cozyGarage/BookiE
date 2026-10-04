# BookiE 0.2 active sprint

Approved 2026-09-16. Consolidated 2026-10-03 from source/document checkpoint
`0cf70382e`; status reconciled against `aeac107a4`. Delivery branch: `linux`; source version 0.1.5; target 0.2.0.
Implementation is authorized; no 0.2 release is approved.
During consolidation, `573435766` merged F4/F9 and `8c0f17494` updated the B4 board;
they are preserved in this checkout. The history files retain the earlier `0cf70382e` snapshot.

## Current continuation plan: 2026-10-04

Order: **B3 → B4 → installed Arch/Omarchy/Hyprland Wayland → Debian/GNOME
Wayland → B7 qualification**. Review/preparation may overlap with reserved
files and fixtures; a lane pass does not close a milestone.

Read [the documentation entry point](README.md) and the relevant ADR, then the
owning packet. Detailed progress and old handoff prompts have moved to
[sprint history](bookie-0.2-history.md). That archive retains source SHAs,
commands and counts; its dated next-task statements are not current instructions.

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
| B3 type/value contracts | Focused fixes and native/consumer evidence recorded; PostgreSQL shadowed domain-over-enum query operators and selected 6–10-, 63-, 64-, 65-, 128-, 129-, 256- and 257-layer shadowed-search_path writes and filters, including same-backend path changes, deterministic raw-parameter refusal and rollback; custom enum-array CSV import now has schema-qualified array-cast, native type/value/wire-byte and sibling-row coverage; domain-over-enum array CSV import also verifies same-named shadow-domain shadowing through `search_path`, and its JSON/CSV/XML/HTML/Markdown/XLSX/SQL file-writer paths have native round-trip coverage; UUID-domain arrays now decode through their base OID and preserve native UUID values through bound and keyed writes, while text/numeric/timestamptz domain arrays round-trip under a non-UTC session; domain-over-bytea array keyed edits select the target domain under a stricter same-named search_path shadow; MySQL/MariaDB ENUM/SET SQL-file, typed CSV and JSON consumers preserve values across twelve SQL modes, including strict modes combined with ANSI_QUOTES and NO_BACKSLASH_ESCAPES; app keyed edits cover twelve SQL modes, including strict ANSI_QUOTES combinations and preserve literal `NULL` labels separately from SQL NULL, empty ENUM/SET and invalid-member refusal; DuckDB grid edits also preserve a literal `NULL` enum label separately from SQL NULL; PostgreSQL scalar enum CSV round trips preserve quotes, embedded line breaks, backslashes and formula-shaped labels across four delimiters and three record endings with format auto-detection; SQL-file replay of backslash labels is verified with standard_conforming_strings on and backslash_quote safe_encoding, and both off; custom enum[], bytea[], timestamptz[] and interval[] XLSX cells survive LibreOffice Calc ODS/XLSX re-save; XLSX safely refuses empty SET text without replacing the prior file, alongside fractional TIME/DATETIME/TIMESTAMP mode behavior at FSP 0-6 with exact-half boundaries and an app parser-to-keyed-edit temporal regression with CSV/JSON/XLSX export and CSV import round-trip checks, collision-free enum clipboard CSV serialization; SQLite computed-expression/compound-result `ANY` fallback and compound CSV/XLSX/typed-CSV consumer checks and an 86,012-value CSV/JSON float corpus have regressions; still open pending the broader engine/type/consumer/configuration matrix | [Type/consumer board](type-contract-strategy.md), [B3 findings](b3-review-2026-10-01.md), case evidence and mutation triage |
| B4 transport/sessions | Policy, daemon cache refusal, editor callback/retirement and awaited cleanup patches, built-in host consent and GUI uncertainty split merged through `6346a431c` | [Current B4 board](b4-task-board.md#b4-continuation-status-october-3): F4/F9, F6 and GUI F8 implementation are merged; MySQL DDL refusal, honest rollback reporting and the MyISAM-trigger rollback boundary have local regressions; headless generation/retirement parity, TLS/daemon/route/audit, PostgreSQL rollback-failure acceptance and installed acceptance remain open |
| B5 editor/files | Open/Save/Save As, changed-on-disk detection and file relinking implemented | Installed file-dialog/recovery/dirty-close flows |
| B6 PostgreSQL catalog | Guarded read-only catalog/types implemented | Restricted-role, stale-owner and installed catalog flows |
| B7 qualification | Open | Frozen SHA, affected automated gates, both installed desktop targets and retry-free soak; publication separate |

This table summarizes source/evidence ownership, not a fresh runtime pass.
Consult each exact case/SHA; do not promote a whole type or engine from one test.

The merged working tree passed local quick CI and the strict GTK+DuckDB values
layer (196 selected tests across all 11 suites, with no missing suites). These
reports capture the dirty merge worktree with `origin/linux` at `6346a431c`
integrated on top of local commit `cdcfcb0`, before the merge commit:
quick CI (local ignored report, not portable: `../target/quality/20261003T001826192859Z-quick/report.json`) and
values layer (local ignored report, not portable: `../target/quality/20261003T002033438424Z-values/report.json`).

On 2026-10-04, the full local values runner
`./scripts/test-value-contracts.sh --gtk --duckdb` passed 284 selected tests
across 11 crate suites, with no missing suites; GTK and DuckDB were enabled.
The report records a successful 28.983-second compile (709 fresh artifacts,
17 rebuilt packages) and zero-exit suites. It is local and ignored at
`target/quality/20261004T094534064491Z-values/report.json`; `dirty: true`
reflects the two pre-existing untracked B4 scratch directories, with tracked
source at `16a52969125c31e12ee161e56677cc295a046ce7`.

After adding the MySQL-backed unparseable-routine approval contract,
`./scripts/test-value-contracts.sh --gtk --duckdb` passed 285 selected tests
across all 11 suites, with no missing suites. The local report is ignored at
`target/quality/20261004T101608010995Z-values/report.json`; it records a
successful 8.014-second compile (745 fresh artifacts, 2 rebuilt packages) and
zero-exit suites. Its `dirty: true` state reflects the test/evidence edits made
during the run and the two existing B4 scratch directories; the final code is
at `b80347facbc6671265fac9a87ed852de261582ea`. The new MySQL approval case is
selected by the runner's `value_contract_` prefix.

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

October 4 B3-4 follow-up: DuckDB enum CSV coverage now proves raw mode restores
empty text, literal `NULL`, Unicode, quotes, formula-shaped labels and SQL NULL
with native ENUM type and an untouched destination row. Spreadsheet-safe mode
prefixes formula-like labels and is explicitly lossy when a valid label already
starts with an apostrophe; it is for presentation, not restore. Other DuckDB
consumer parity remains open; see the [evidence
manifest](evidence/duckdb-enum-csv-roundtrip-results-2026-10-04/manifest.json).

October 4 B3-4 follow-up: SQLite `CAST(value AS BLOB)` results over STRICT
`ANY` now have JSON, XLSX and typed CSV coverage with native storage-class and
byte checks, covering numeric/text casts, empty bytes, binary bytes and SQL
NULL. See the [evidence manifest](evidence/sqlite-cast-blob-any-csv-results-2026-10-04/manifest.json).

October 4 B3-4 follow-up: PostgreSQL scalar enum XLSX output now has a
LibreOffice Calc XLSX-to-ODS-to-XLSX re-import regression for literal `NULL`,
Unicode, markup and formula-shaped labels; SQL NULL stays blank and empty-label
refusal preserves the destination. Other workbook shapes and applications
remain open ([evidence](evidence/postgres-enum-scalar-calc-reimport-results-2026-10-04/manifest.json)).

October 4 B3-1 follow-up: PostgreSQL custom enum metadata, keyed updates and
structured equality filters cover schema/type/table names with spaces and
embedded double quotes, with native type and sibling-row assertions. Other
identifier and session/search_path combinations remain open ([spaces evidence](evidence/postgres-enum-quoted-identifiers-results-2026-10-04/manifest.json),
[quote-escaping evidence](evidence/postgres-enum-identifier-escaping-results-2026-10-04/manifest.json)).

October 4 B3-1 follow-up: a same-named PostgreSQL enum under the pool role's
default shadowed `search_path` cannot accept the target-only label; qualified
keyed edit and filter paths select the target type and preserve the shadow row
([evidence](evidence/postgres-enum-shadowed-quoted-search-path-results-2026-10-04/manifest.json)).

R1–R5/R7 have recorded fixes; R6 evidence portability and the remaining matrix
stay open. U1 metadata/two INSERT paths have [native proof](upstream-u1-sql-server-2026-10-03.md);
other consumers/ledger acceptance remain open. U2–U6 and older-release O1–O3
are mapped in [main review](upstream-main-review-2026-10-03.md) and
[older-release review](upstream-older-releases-review-2026-10-03.md). Reuse these
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

The [architecture review](architecture-consistency-review-2026-10-03.md#remaining-source-risks)
records panic privacy and headless retirement gaps alongside these owners.

## Arch / Omarchy / Wayland UI packets

Use [manual acceptance](manual-verification-0.2-features.md) and
[the package guide](omarchy.md). This is application work; no desktop/system
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
Cargo/Docker work; preserve warm build reuse per [toolchains](toolchains.md).
Do not infer new passes from test-source inspection. Documentation packets use
diff/link checks; runtime/package claims require their actual owning checks.

## History lookup

[Sprint history](bookie-0.2-history.md) preserves original scope, dated progress,
commit tables, build-cache measurements and rollback details. Read a relevant
section only. New current work updates this sprint/owning board; new case results
update the value ledger, with links here instead of copied logs/counts.
