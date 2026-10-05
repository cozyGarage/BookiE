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
| B3 type/value contracts | Focused fixes and native/consumer evidence recorded; PostgreSQL shadowed domain-over-enum query operators and selected 6–10-, 63-, 64-, 65-, 128-, 129-, 256-, 257-, 258- and 259-layer shadowed-search_path writes and filters, including same-backend path changes, deterministic raw-parameter refusal and rollback; custom enum-array CSV import now has schema-qualified array-cast, native type/value/wire-byte and sibling-row coverage; domain-over-enum array CSV import also verifies same-named shadow-domain shadowing through `search_path`, and its JSON/CSV/XML/HTML/Markdown/XLSX/SQL file-writer paths have native round-trip coverage; UUID-domain arrays now decode through their base OID and preserve native UUID values through bound and keyed writes, while text/numeric/timestamptz domain arrays round-trip under a non-UTC session; domain-over-bytea array keyed edits select the target domain under a stricter same-named search_path shadow; MySQL/MariaDB ENUM/SET SQL-file, typed CSV and JSON consumers preserve values across twelve SQL modes, including strict modes combined with ANSI_QUOTES and NO_BACKSLASH_ESCAPES; app keyed edits cover twelve SQL modes, including strict ANSI_QUOTES combinations and preserve literal `NULL` labels separately from SQL NULL, empty ENUM/SET and invalid-member refusal; DuckDB grid edits also preserve a literal `NULL` enum label separately from SQL NULL; PostgreSQL scalar enum CSV round trips preserve quotes, embedded line breaks, backslashes and formula-shaped labels across four delimiters and three record endings with format auto-detection; SQL-file replay of backslash labels now passes all four standard_conforming_strings/backslash_quote combinations; custom enum[], bytea[], timestamptz[], interval[], numeric[] and float8[] XLSX cells survive LibreOffice Calc ODS/XLSX re-save; XLSX safely refuses empty SET text without replacing the prior file, alongside fractional TIME/DATETIME/TIMESTAMP mode behavior at FSP 0-6 with exact-half boundaries and an app parser-to-keyed-edit temporal regression with CSV/JSON/XLSX export and CSV import round-trip checks, collision-free enum clipboard CSV serialization; SQLite computed-expression/compound-result `ANY` fallback and compound CSV/XLSX/typed-CSV consumer checks and an 86,012-value CSV/JSON float corpus have regressions; still open pending the broader engine/type/consumer/configuration matrix | [Type/consumer board](type-contract-strategy.md), [B3 findings](b3-review-2026-10-01.md), case evidence and mutation triage |
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

After the PostgreSQL float8-array Calc contract, the full strict values runner
`./scripts/test-value-contracts.sh --gtk --duckdb` passed 288 selected contracts
across all 11 suites, including 95 PostgreSQL contracts; GTK and DuckDB were
enabled and no suites were missing. The PostgreSQL suite includes
`array_contract::value_contract_float8_array_file_exports_preserve_bits`. The
portable run report and suite logs are retained in
[evidence](evidence/postgres-float8-array-calc-reimport-results-2026-10-04/strict-values/report.json).
The runner reported the pre-commit revision `7f807cd7` with a dirty worktree;
the tested source change is committed at `8bb51d3c0`.

After the local GTK+DuckDB value tier exposed a 300-second cumulative timeout
in the 28-test MySQL suite, the runner cap was raised to 420 seconds. The MySQL
suite passed in 312.976 seconds and the full run passed 292 contracts across
all 11 suites, compiling from 747 fresh cached artifacts with no rebuilt
packages. The runner's 9 unit tests also pass. The durable report and raw suite
logs, including the original timeout, are in the
[full value-tier evidence](evidence/full-value-tier-results-2026-10-04/manifest.json).

October 4 follow-up: after the SQL Server legacy `datetime` grid edit fix, the
GTK + DuckDB value tier passed 307 selected contracts across all 11 suites with
no missing suites. The report used source `72f937b0`; its `dirty: true` flag is
from the two pre-existing B4 scratch directories. The portable report, suite
logs and checksums are in the [run evidence](evidence/full-value-tier-mssql-grid-results-2026-10-04/manifest.json).

October 4 B3 follow-up: SQL Server `datetimeoffset` grid edits now preserve
local time, fractional scale and offset through text parsing and keyed updates;
native checks cover mixed offsets, calendar boundaries, stored bytes and an
untouched sibling. The parser refuses unsupported scale, offset and UTC-range
inputs. The GTK + DuckDB value tier passed 308 contracts across all 11 suites;
see the [datetimeoffset evidence](evidence/mssql-datetimeoffset-grid-edit-results-2026-10-04/manifest.json).

October 4 follow-up: with the PostgreSQL domain-over-enum `COALESCE` contract
added, all 293 selected contracts still pass across the same 11 suites. The
source revision is `b165a0c7`; the report, suite logs and hashes are retained in
[the follow-up evidence](evidence/full-value-tier-postgres-coalesce-results-2026-10-04/manifest.json).

October 4 follow-up: PostgreSQL now binds an inferred enum-array parameter for
the domain-over-enum `ANY($1)` query context. The failing-first Docker contract
covers text parsing, multidimensional bounds, native array type/wire bytes,
NULL/empty distinctions, invalid labels, and pre-dispatch refusal. Commit
`cf633a51d` passes all 99 PostgreSQL contracts and the GTK + DuckDB value tier
(11 suites); the aggregate report and suite logs are in the
[evidence packet](evidence/postgres-domain-enum-any-array-parameter-results-2026-10-04/manifest.json).
B3 remains open because other inferred array contexts and the broader engine,
consumer, and configuration matrix are not covered.

October 4 B3 follow-up: domain-over-enum inferred enum-array contracts now also
cover containment and overlap with the parameter in either operand position.
The PostgreSQL integration suite passes all 129 tests, and the ignored-test
inventory now lists 437 declarations. Evidence and run logs are in the
[array-operator packet](evidence/postgres-domain-enum-array-operators-results-2026-10-04/manifest.json).
B3 remains open for other array contexts and the broader matrix.

October 4 B3 configuration follow-up: containment and overlap enum-array
parameters retain the qualified target enum OID and bytes when a same-named
shadow enum precedes it in `search_path`. The complete PostgreSQL suite passes
129 tests ([evidence](evidence/postgres-shadowed-enum-array-operator-results-2026-10-04/manifest.json)).
This closes only the tested array-operator/search-path combination.

October 4 B3 depth follow-up: inferred enum-array parameters round-trip through
62 and 63 nested enum domains, then return explicit unsupported results at 64
for text and SQL NULL. The test compares native type and wire bytes and passes
in the 130-test PostgreSQL integration run
([evidence](evidence/postgres-inferred-enum-array-depth-results-2026-10-04/manifest.json)).
It also records PostgreSQL's native `42883` refusal for scalar equality against
`ANY(domain_array)` at depth 63; containment isolates the parameter binding path.

October 4 B3 function follow-up: inferred text and SQL NULL parameters to
`array_remove` pass for domain-over-enum inputs, with typed native query results
and `pg_typeof` oracles. Other PostgreSQL array functions and the broader B3
matrix remain open ([evidence](evidence/postgres-domain-enum-array-remove-results-2026-10-04/manifest.json)).

October 4 B3 follow-up: `array_position` now has the same inferred text/SQL NULL
coverage, including PostgreSQL's NULL-element match behavior and native position
results ([evidence](evidence/postgres-domain-enum-array-position-results-2026-10-04/manifest.json)).

October 4 B3 follow-up: `array_replace` verifies both inferred scalar parameter
slots for domain-over-enum arrays, including NULL search and replacement values
([evidence](evidence/postgres-domain-enum-array-replace-results-2026-10-04/manifest.json)).

October 4 B3 follow-up: `array_cat` now covers inferred enum-array parameters
with text and SQL NULL inputs, special labels, and a same-named shadow enum in
`search_path`; the new case passes within the 105-test PostgreSQL value-contract
selection. Other inferred array functions and the broader B3 matrix remain
open ([evidence](evidence/postgres-domain-enum-array-cat-results-2026-10-04/manifest.json)).

October 4 B3 follow-up: `array_positions` now verifies inferred scalar enum
parameters against native results for repeated values and NULL elements while a
same-named shadow enum leads `search_path`; text and SQL NULL cases pass. Other
inferred array contexts and the wider B3 matrix remain open
([evidence](evidence/postgres-domain-enum-array-positions-results-2026-10-04/manifest.json)).

October 4 B3 follow-up: all six PostgreSQL array comparison operators infer the
domain-over-enum array parameter type and preserve native results for duplicates,
non-default lower bounds, empty arrays, SQL NULL arrays and NULL elements while a
same-named shadow enum leads `search_path`. Native array wire bytes are
asserted; see the [evidence manifest](evidence/postgres-domain-enum-array-equality-results-2026-10-04/manifest.json). Other PostgreSQL array contexts remain open.

October 4 B3 function follow-up: `array_append` and `array_prepend` preserve
inferred target enum binding under a same-named shadow enum in `search_path`.
Native typed results, `pg_typeof`, result bytes, literal `NULL` versus SQL NULL,
and target-enum invalid-label refusal are covered; all 108 selected PostgreSQL
value contracts pass ([evidence](evidence/postgres-domain-enum-array-functions-shadowed-results-2026-10-04/manifest.json)).
Other PostgreSQL function contexts and the broader B3 matrix remain open.

October 5 B3 function follow-up: `array_replace` now checks both inferred
parameter slots when a same-named shadow enum leads `search_path`. Native result
and wire-byte oracles cover literal `NULL`, SQL NULL and invalid shadow-only
labels in either slot; all 109 selected PostgreSQL value contracts pass
([evidence](evidence/postgres-domain-enum-array-replace-shadowed-results-2026-10-05/manifest.json)).
Other PostgreSQL contexts and the broader B3 matrix remain open.

October 5 B3 function follow-up: `array_position($1, enum_value)`,
`array_remove($1, enum_value)`, `array_append($1, enum_value)` and
`array_prepend(enum_value, $1)` now verify inference for the array parameter
itself under a same-named shadow enum. Native result and wire oracles include
lower-bound, empty/NULL array, NULL element and literal `NULL` cases; all 110
selected PostgreSQL value contracts pass
([evidence](evidence/postgres-domain-enum-array-input-functions-shadowed-results-2026-10-05/manifest.json)).
Other PostgreSQL contexts and the broader B3 matrix remain open.

October 5 B3 consumer follow-up: PostgreSQL `text[]` XLSX output survives
LibreOffice Calc XLSX-to-ODS-to-XLSX re-save with all 84 characters unchanged,
as a text cell and without formulas. The artifact, pinned Debian/Calc run and
XML verifier are retained in the
[text-array Calc evidence packet](evidence/postgres-text-array-calc-reimport-results-2026-10-05/manifest.json).
Other PostgreSQL array families, spreadsheet applications and B3 matrices remain open.

October 5 B3 consumer follow-up: PostgreSQL `uuid[]` XLSX output survives the
same pinned LibreOffice Calc XLSX-to-ODS-to-XLSX re-save as 84-character text.
Native array rebinding preserves exact wire bytes despite PostgreSQL and
BookiE using different valid UUID-array literal quote spellings. The artifacts
and XML verifier are retained in the
[UUID-array Calc evidence packet](evidence/postgres-uuid-array-calc-reimport-results-2026-10-05/manifest.json).
Other PostgreSQL array shapes and consumer matrices remain open.

October 5 B3-2 consumer follow-up: PostgreSQL `date[]` with a BC date, year
10000, both infinities and SQL NULL survives the pinned Calc XLSX-to-ODS-to-XLSX
round trip as exact text, without formulas. Native `array_to_json` and
`array_send` match after rebinding. See the
[date-array Calc evidence](evidence/postgres-date-array-calc-reimport-results-2026-10-05/manifest.json).
Other temporal-array shapes and consumers remain open.

October 5 B3-2 consumer follow-up: PostgreSQL `timestamp[]` with a BC value,
year 10000, the maximum finite value, both infinities and SQL NULL survives the
pinned Calc XLSX/ODS/XLSX round trip as exact text. Native `array_to_json` and
`array_send` match after binding. See the
[timestamp-array Calc evidence](evidence/postgres-timestamp-array-calc-reimport-results-2026-10-05/manifest.json).

October 5 B3-2 consumer follow-up: PostgreSQL `time[]` with midnight,
fractional microseconds, 23:59:59.999999, 24:00 and SQL NULL survives the
pinned Calc round trip as text. Native `array_to_json` and `array_send` match
after binding. See the
[time-array Calc evidence](evidence/postgres-time-array-calc-reimport-results-2026-10-05/manifest.json).

October 5 B3-2 consumer follow-up: PostgreSQL `timetz[]` with both maximum
legal offsets, fractional wall times and SQL NULL survives the pinned Calc
round trip as text. Native `array_to_json` and `array_send` match after binding.
See the [timetz-array Calc evidence](evidence/postgres-timetz-array-calc-reimport-results-2026-10-05/manifest.json).

October 5 B3-1 consumer follow-up: PostgreSQL `boolean[]` preserves true,
false and SQL NULL through the pinned Calc round trip as a text cell. Native
`array_to_json` and `array_send` match after rebinding. See the
[boolean-array Calc evidence](evidence/postgres-boolean-array-calc-reimport-results-2026-10-05/manifest.json).

October 5 B3-1 consumer follow-up: PostgreSQL `int8[]` with lower bound zero,
`i64::MIN`, `9007199254740993`, `i64::MAX` and SQL NULL survives the pinned
Calc round trip as exact text. Native `array_to_json` and `array_send` match
after rebinding. See the
[int8-array Calc evidence](evidence/postgres-int8-array-calc-reimport-results-2026-10-05/manifest.json).

October 5 B3-1 consumer follow-up: PostgreSQL `smallint[]` and `integer[]`
with signed boundaries, SQL NULL, and non-default lower bounds survive the
pinned Calc re-save as exact text. Native array JSON and wire bytes match after
rebinding. See the
[integer-array Calc evidence](evidence/postgres-integer-arrays-calc-reimport-results-2026-10-05/manifest.json).

October 5 local GTK + DuckDB value tier: all 316 selected tests passed across
all 11 suites, with no missing suites. PostgreSQL contributed 115 contracts;
the current MySQL/MariaDB suite contributed 28. Compile reused 746 cached
artifacts and rebuilt one package. The runner marked the tree dirty because the
preserved B4 scratch directories remain outside `linux/`; see the [run packet](evidence/local-gtk-duckdb-value-tier-results-2026-10-05/manifest.json).

October 4 local GTK + DuckDB value tier after the PostgreSQL
`array_positions` addition passed 304 selected contracts across all 11 suites,
including 106 PostgreSQL contracts; no suites were missing. See the current
[source-fingerprinted evidence packet](
evidence/postgres-domain-enum-array-positions-results-2026-10-04/manifest.json).
B3 remains open for the broader matrix.

October 4 B3 MongoDB follow-up: stale keyed edits now compare original values of
edited fields in the atomic update filter. A MongoDB 7 regression preserves a
concurrent value and distinguishes explicit BSON NULL from a concurrently
removed field and an array containing NULL. Stale deletes, ABA changes, and the broader B3 matrix remain open
([evidence](evidence/mongodb-stale-grid-edit-results-2026-10-04/manifest.json)).

October 4 B3 MongoDB delete follow-up: parameterized keyed deletes are now
supported and guard all materialized non-key values. Native coverage proves
unchanged rows delete and stale string, untouched sibling, missing-field, and
BSON NULL-to-array changes survive. ABA changes and the broader B3 matrix remain open
([evidence](evidence/mongodb-stale-grid-delete-results-2026-10-04/manifest.json)).

October 4 follow-up: the GTK + DuckDB local value tier now passes 302 selected
contracts across all 11 suites, including 104 PostgreSQL and 15 MongoDB
contracts. The exact-revision report and raw logs are in the [latest value-tier
packet](evidence/full-value-tier-mongodb-stale-delete-results-2026-10-04/manifest.json);
the earlier 297-contract enum-array packet is retained separately.

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

October 4 B3-5 follow-up: a SQL Server script with a valid `GO` batch before an
unterminated quoted tail now fails whole-script planning, keeps tail placeholders
out of extraction, remains malformed after formatting and is denied to agents
by fail-closed policy. Other dialects and installed SQL editor interaction
remain open; see the [evidence manifest](evidence/mssql-malformed-go-tail-results-2026-10-04/manifest.json).

October 4 B3-2 follow-up: SQL Server legacy `datetime` now decodes every
1/300-second tick exactly, using typed nanosecond values where representable
and canonical style-126 text for the others. Native SQL Server tests compare
all 300 ticks with server text and verify parameter and SQL-literal restore by
the stored 8-byte value; see the [evidence manifest](evidence/mssql-legacy-datetime-ticks-results-2026-10-04/manifest.json).

October 4 B3-4 follow-up: the SQL Server app parser/keyed-grid path now keeps
representative fallback and exact legacy `datetime` ticks byte-for-byte across
a no-op edit and preserves an untouched sibling. The grid display now includes
nonzero `DateTime` fractions, which was required to keep exact ticks editable.
See the [grid evidence](evidence/mssql-legacy-datetime-grid-edit-results-2026-10-04/manifest.json).

October 4 B3-5 follow-up: a Docker-backed GTK scenario now checks MySQL
unparseable-routine approval text, deny-before-dispatch, and Allow Once against
the native routine catalog. It exposed the pooled MySQL query path missing the
text-protocol fallback already used by explicit sessions; both now share it.
The [retained evidence](evidence/mysql-gtk-unparseable-approval-results-2026-10-04/manifest.json)
includes the pre-fix error and passing GTK, strict unit, Clippy, and format
results. Other dialect executable-identity/malformed-tail cases and additional
MySQL delimiter forms remain open.

October 4 B3-5 follow-up: the MySQL `\d //` shorthand now has a named consumer
contract across planning, formatting, parameter extraction and policy
classification. See the [evidence manifest](evidence/mysql-short-delimiter-results-2026-10-04/manifest.json).

October 4 B3-5 follow-up: ClickHouse's shared parameter scanner now recognizes
the same `$tag$...$tag$` heredoc rule as the script lexer, so placeholders
inside closed or unterminated bodies are not extracted. An app contract checks
planner diagnostics, whole-script refusal, safe-prefix parameters, formatter
retention and fail-closed policy; see the
[ClickHouse malformed-heredoc evidence](evidence/clickhouse-malformed-heredoc-results-2026-10-04/manifest.json).

October 4 B3-1 follow-up: PostgreSQL schema-aware enum-domain metadata, keyed
writes, filters and SQL NULL sibling preservation now pass through 259 nested
domain layers while a same-named shadow enum leads `search_path`. The adjacent
258/259 Docker contracts pass; raw inferred text/NULL remains explicitly
unsupported from 64 layers. See the
[259-layer evidence](evidence/postgres-domain-259-level-results-2026-10-04/manifest.json).

October 4 B3-1/B3-4 follow-up: a PostgreSQL domain-over-enum array XLSX cell
survives LibreOffice Calc XLSX-to-ODS-to-XLSX re-save as the same 50-code-point
string, including literal `NULL`, empty text, Unicode, separators, markup, a
formula-shaped label and SQL NULL. See the [evidence packet](
evidence/postgres-domain-enum-array-calc-reimport-results-2026-10-04/manifest.json).

October 4 B3-1 follow-up: a PostgreSQL 16 contract now covers domain-over-enum
`COALESCE` with text and SQL NULL in both argument positions. It verifies
PostgreSQL resolves the parameter/result to the base enum while the source
column remains the domain; all 16 domain contracts pass. See the
[COALESCE evidence](evidence/postgres-domain-coalesce-results-2026-10-04/manifest.json).
A companion array-function contract covers `array_append`/`array_prepend`,
text/NULL inference and native input/result array types; all 17 domain contracts
pass ([evidence](evidence/postgres-domain-array-functions-results-2026-10-04/manifest.json)).

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

October 5 B3-1 follow-up: scalar-enum CSV restore now has a same-named shadow
enum first in `search_path`; a target-only label, empty label, literal `NULL`,
and SQL NULL restore to the qualified destination type without changing its
sibling or writing to the shadow table ([evidence](evidence/postgres-enum-csv-shadow-search-path-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: MariaDB 11's `EMPTY_STRING_IS_NULL` turns a bound
empty CSV value into SQL NULL. The importer now uses a `SPACE(0)` expression
for empty MySQL/MariaDB ENUM labels and zero-member SET values; a native round
trip distinguishes both from SQL NULL and preserves the sibling ([evidence](evidence/mariadb-empty-enum-set-empty-string-is-null-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: the MariaDB empty ENUM/SET CSV restore also passes
with `EMPTY_STRING_IS_NULL`, `STRICT_TRANS_TABLES`, `ANSI_QUOTES` and
`NO_BACKSLASH_ESCAPES` active together; the native ordinal/byte/NULL checks and
untouched sibling match the standalone-mode case ([evidence](evidence/mariadb-empty-enum-set-combined-mode-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: the MariaDB app grid's explicit empty ENUM and SET
edits became SQL NULL under `EMPTY_STRING_IS_NULL`, including when combined
with strict and quoting modes. The keyed-update builder now emits `SPACE(0)`
for empty MySQL ENUM/SET values; native MySQL and MariaDB grid contracts pass,
and ordinary text stays bound ([evidence](evidence/mariadb-empty-enum-set-grid-empty-string-mode-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: the same MariaDB mode also turned empty ENUM/SET
values into SQL NULL during SQL-file replay. The shared MySQL SQL literal
writer now emits `SPACE(0)` for empty text, including ordinary text columns.
Native ENUM ordinals, SET masks, exact bytes, SQL NULL state, and CSV restore
are checked independently under both `EMPTY_STRING_IS_NULL` configurations;
the core builder test pins the ordinary-text literal ([evidence](evidence/mariadb-empty-enum-set-sql-file-empty-string-mode-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: an empty VARCHAR also became SQL NULL through typed
CSV import, while SQL-file replay preserved it. The shared import planner now
reconstructs empty CHAR/VARCHAR/TEXT values with `SPACE(0)`; MySQL and MariaDB
native mode contracts compare text bytes and NULL state independently for CSV
and SQL replay ([evidence](evidence/mariadb-empty-varchar-import-empty-string-mode-results-2026-10-05/manifest.json)).

October 5 local GTK + DuckDB value tier: 321 selected tests passed across all
11 suites with no missing suites at source `7664ae7c7b14002c81eb5673a46fa54d0aac2742`,
with the empty-text SQL and typed-CSV changes present in the working tree.
The run report and suite logs are recorded in the
[value-tier packet](evidence/local-gtk-duckdb-value-tier-empty-varchar-import-results-2026-10-05/manifest.json) and the [MySQL/MariaDB import packet](evidence/mariadb-empty-varchar-import-empty-string-mode-results-2026-10-05/manifest.json).

October 5 B3-1 follow-up: PostgreSQL scalar-enum CSV import now also carries a
schema-qualified destination cast across a role `search_path` change between
planning and execution on a fresh connection. A distinct same-named enum leads
the new path; native target type/value checks confirm restoration and the
shadow table remains empty ([evidence](evidence/postgres-enum-csv-search-path-transition-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: PostgreSQL 16's `trim_array($1, n)` cannot infer an
unknown polymorphic array input and returns `42804`. A new boundary contract
pins that server refusal and verifies that a schema-qualified array cast
preserves NULL, empty, NULL-element and lower-bound values under a shadowed
`search_path` ([evidence](evidence/postgres-trim-array-enum-inference-boundary-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: the scalar enum parameter matrix now verifies
PostgreSQL's CASE-result inference in both branch positions. It preserves the
literal `NULL` and empty labels separately from SQL NULL, checks native
parameter/result types, and confirms invalid labels fail with `22P02`
([evidence](evidence/postgres-enum-case-parameter-inference-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: PostgreSQL scalar enum parameters are now checked in
both `GREATEST` and `LEAST` argument positions against native typed results.
The contracts retain PostgreSQL's NULL handling and `22P02` invalid-label
behavior ([evidence](evidence/postgres-enum-greatest-least-parameter-inference-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: domain-over-enum CASE parameters now have native
coverage in both result branches with a same-named shadow enum first in
`search_path`. Literal `NULL`, empty text, SQL NULL, `pg_typeof`, invalid-label
refusal and unchanged outer-domain rows are checked
([evidence](evidence/postgres-domain-case-shadowed-parameter-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: ordinary custom-enum `COALESCE`, `GREATEST` and
`LEAST` parameter contexts now match native target-enum casts with a same-named
shadow first in `search_path`; empty/literal-`NULL`, SQL NULL, type results and
shadow-only label refusal are checked ([evidence](evidence/postgres-shadowed-enum-parameter-context-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: domain-over-enum `COALESCE` parameters now also match
explicit native target-enum casts in both argument positions under a shadowed
`search_path`, including empty text, literal `NULL`, SQL NULL and invalid
shadow-only labels ([evidence](evidence/postgres-domain-coalesce-shadowed-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: PostgreSQL custom-enum CSV import also handles schema,
table and type identifiers with spaces and embedded quotes while a same-named
shadow schema leads `search_path`. The generated cast selects the target enum;
native type/value checks confirm the row and preserve target siblings and the
shadow row ([evidence](evidence/postgres-enum-csv-quoted-identifiers-results-2026-10-05/manifest.json)).

October 4 local integration gate: all 315 tests passed at
`d783182f972036e8af1521b0e3d44acd54cf77a7` across six database drivers, MCP,
policy/session, PostgreSQL socket and SSH suites. B3/B4 and release acceptance
remain open ([run evidence](evidence/local-integration-tier-results-2026-10-04/manifest.json)).

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
