# B3 type and consumer work board

The shared technical standard is [ADR 0007](decisions/0007-type-and-value-preservation.md).
This page owns remaining B3 work; [the sprint](bookie-0.2-sprint.md) owns order
and acceptance. Updated 2026-10-03; each new case links to its source-fingerprinted
evidence packet, and this summary is not itself runtime evidence.

## Current evidence and next targets

Detailed native cases and old counts are in [type-contract history](type-contract-history.md)
and [the value evidence index](value-contracts.md). Those records keep their
source/SHA attribution; this summary does not certify the current tree.

| Owner / engine | Retain established contracts | Remaining scope to select one case from |
| --- | --- | --- |
| PostgreSQL | Wide NUMERIC exact text; scalar/array/temporal/interval cases; extended DATE/TIMESTAMP/TIMESTAMPTZ exact-text support, including year 1,000,000 DATE and upper finite bounds through result, literal, parameter, CSV and keyed-edit paths; ordinary custom-enum labels and SQL NULL have direct scalar/array results plus schema-aware keyed-edit, draft-insert, all-operator structured-filter and CSV-import coverage; keyed edits select the correct native enum when the same type name exists in two schemas and the active search_path points at the shadow type; the literal label `NULL` survives keyed writes, direct scalar/array projections, filters and CSV import distinctly from SQL NULL; PostgreSQL 16 scalar/array projections, all shared structured filters, JSON and replayed SQL files confirm domain-over-enum values retain labels, SQL NULL and native type; catalog enum metadata also supports domain-column keyed updates and draft inserts; explicitly cast text/SQL NULL parameters, assignment-inferred text/SQL NULL updates, and base-enum comparison query parameters preserve domain-over-enum values and native type; a three-level domain-over-enum chain now has native projection, recursive catalog metadata, filter, keyed/draft write, inferred transaction parameter and invalid-label refusal coverage; raw PostgreSQL CSV export/import preserves domain-over-enum labels, empty text and SQL NULL with an explicit marker, while ambiguous default blanks are refused before writes; domain values now also have native MCP `execute_query`, JSON/CSV `export_data` and CSV import proof; shared JSON rendering, core file-writer JSON/CSV/XML/HTML/Markdown and XLSX (non-empty enum strings remain text; empty enum labels are refused safely), SQL-file export replay, and MCP enum exports preserve the tested values; raw text/SQL NULL query, update and transaction parameters adopt server-inferred enum types when context identifies them | A raw domain-to-unknown comparison is explicitly refused by PostgreSQL with SQLSTATE 42883; casting the column to its base enum allows inferred text/NULL query parameters. Other direct query-parameter operator contexts and session configurations, deeper nested-domain depths and other custom/native cases need proof |
| MySQL/MariaDB | Signed/unsigned bounds, wide DECIMAL text, zero/extended temporals, BIT/spatial refusal, tested SQL modes, and locally completed zero-row metadata/bounded query regressions ([native regressions](evidence/mysql-atomic-results-2026-10-03/manifest.json)) | Remaining configuration/consumer cases and installed typed edits; retain UTC pool versus dedicated-session distinctions |
| SQLite | Dynamic storage classes, exact bytes/NULL, typed affinity consumer cases, REAL float boundary refusal/preservation, and STRICT `ANY` edits that retain existing INTEGER/REAL/TEXT classes; clearing an existing TEXT cell stores empty TEXT, empty NULL/new cells stay NULL, nonempty NULL/new input stays TEXT, and BLOB runtime values are read-only with exact bytes preserved. Direct nonempty table-column query results now recover declared `ANY` metadata for query, bound-query and transaction consumers; query-result CSV round-trips storage classes with native `typeof()` proof ([evidence](evidence/sqlite-strict-any-query-csv-results-2026-10-03/manifest.json)) | Remaining storage-class/affinity mixtures, zero-row query metadata, computed expressions and attached-schema origins without resolvable declared metadata, other consumers and installed editing |
| SQL Server | Decimal/offset/calendar contracts and temporal/money/variant refusals; U1 server-owned metadata | Remaining native/consumer gaps, metadata precision, U1 other consumers/ledger and U2 identity copying; O2 large values/O3 result sets need reproduction |
| ClickHouse | Wide/nested exact representations, DateTime64 precision/bounds/zones and type-less write refusal | Remaining nested/type/consumer combinations and installed editing; no generic type-less JSON binding |
| Redis | RESP3 tagged nested values/binary kinds, caps and persistent-stream refusal | Remaining protocol/native consumer coverage; display conversion does not prove subscription support |
| MongoDB | Canonical Extended JSON, BSON width/subtypes and typed keyed edits; browse metadata and retained page rows come from one collection cursor; an off-page type change observed later in that cursor marks the column `mixed` and the visible rows read-only; a failpoint test asserts no second page `find`; explicit BSON null and missing sparse-document fields have distinct result markers | Full-scan cost, writes to documents already observed during the cursor, `run_find`'s separate schema/query reads, remaining consumer combinations and installed editing |
| DuckDB (optional) | Wide integers, intervals, DuckDB enum result/literal/bound labels and app keyed edits (blank remains SQL NULL; `''` enters the empty label; doubled quotes decode), and extended DATE/TIMESTAMP/TIMESTAMPTZ text fallbacks; tested nested refusals | Remaining nested combinations, native sub-microsecond bindings and installed GTK editing; text transport is not native binding |
| Shared consumers | Named parameters, export/import, grid/filter, JSON/MCP and workbook cases | One missing format/type/configuration boundary, native kind after writes and refused-operation postconditions |
| Evidence / mutation | Registered fixtures, independent oracles and existing scoped mutation results | Portable raw proof for missing reports; genuine survivors/timeouts and uncovered consumers remain open |

The ordinary custom-enum shadow-schema case now also checks a structured `=`
filter on `enum_shadow_b.items` while `search_path` resolves the same enum name
in `enum_shadow_a`; native target type/value checks pass. See the
[focused evidence](evidence/postgres-shadowed-enum-filter-results-2026-10-03/manifest.json).

The PostgreSQL 16 ordinary-enum and domain-over-enum structured filters now
cover every shared `FilterOp`, including range comparisons, text-pattern
filters, both NULL operators and both list operators, with exact rows/labels and
native type assertions; see the
[complete filter matrix](evidence/postgres-enum-filter-matrix-results-2026-10-03/manifest.json).
For direct query parameters, base-enum-cast `=`, `<>`, `<`, `<=`, `>`, `>=`,
`IN`, `NOT IN` and `BETWEEN` preserve inferred parameter types; raw domain
`= $1` remains explicitly refused with SQLSTATE 42883. See the
[query-operator evidence](evidence/postgres-domain-enum-query-operators-results-2026-10-03/manifest.json)
and the [ordering](evidence/postgres-domain-enum-param-operator-results-2026-10-03/manifest.json)
and [list-operator evidence](evidence/postgres-domain-enum-param-list-results-2026-10-03/manifest.json).
Other direct query-parameter contexts, session configurations, deeper
nested-domain depths and other custom/native cases remain open.

SQLite STRICT `ANY` table and direct nonempty query-result CSV now tag INTEGER,
REAL, TEXT and BLOB cells so a native import can retain their runtime storage
classes; SQL NULL uses the export's explicit collision-free marker. Untagged
text stays text, while a blank without a marker and malformed reserved tags are
refused. Both app round trips have a native `typeof()`/value oracle. Zero-row
queries, computed expressions and attached-schema origins without resolvable
declared metadata, other formats and installed editing remain open; see the
[table CSV case](value-contract-history.md#sqlite-strict-any-csv-storage-class-round-trip-2026-10-03)
and [query-result case](value-contract-history.md#sqlite-strict-any-query-result-csv-round-trip-2026-10-03).

Rows identify work areas, not completed engine support. State the four ADR 0007
outcomes per concrete case. The older detailed matrix remains useful for lookup;
do not re-import its long case descriptions into this board.

## 0.2.0 accepted explicit refusals

The maintainer accepted these named, tested refusals out of scope for 0.2.0.
They remain safety requirements, not exact-support claims. Every other in-scope
row, including untested combinations, requires exact support before B3 closes.

| ID | Accepted gap | Evidence |
| --- | --- | --- |
| B3-OOS-PG-1 | Built-in ranges/multiranges, geometric values, named composites and native `money` | [Ranges](value-contract-history.md#postgresql-built-in-range-family-refusal), [geometry](value-contract-history.md#postgresql-geometric-types-exact-server-oracle-and-visible-refusal), [composites](value-contract-history.md#postgresql-composite-explicit-refusal), [money](value-contract-history.md#postgresql-money-safe-refusal) |
| B3-OOS-PG-2 | `json[]`/`jsonb[]` result, SQL-literal and parameter consumers | [JSON array contracts](value-contract-history.md#postgresql-array-checkpoint) |
| B3-OOS-DUCK-1 | Named nested collection shapes containing unsigned values outside decoder support | [Nested unsigned values](value-contract-history.md#duckdb-nested-uhugeint-refusal-boundaries-2026-09-30) |
| B3-OOS-DUCK-2 | Sub-microsecond edits to lower-precision temporal columns | [Edit precision refusal](value-contract-history.md#duckdb-timestamptz-grid-edit-precision-boundary) |
| B3-OOS-MYSQL-1 | Spatial and too-wide `BIT(64)` values as editable grid cells | [Spatial](value-contract-history.md#mysql-spatial-bytes-in-the-gtk-grid), [BIT](value-contract-history.md#mysql-signed-and-unsigned-integer-grid-parser-2026-09-30) |
| B3-OOS-MYSQL-2 | MySQL `TIMESTAMP` instant decoding in a non-UTC dedicated session | [Session contract](value-contract-history.md#mysql-native-time-zero-date-and-year-checkpoint) |
| B3-OOS-MSSQL-1 | Inexact legacy `datetime` ticks, `money`/`smallmoney` and `sql_variant` | [Legacy datetime](value-contract-history.md#sql-server-legacy-datetime-tick-boundaries), [variant](value-contract-history.md#sql-server-sql_variant-metadata-refusal), [money](value-contract-history.md#sql-server-money-float-decoding-refusal) |
| B3-OOS-CH-1 | Tested ambiguous/out-of-range `DateTime64` values and nested shapes refused by type-less consumers | [Nested consumers](value-contract-history.md#clickhouse-nested-value-consumer-boundary-2026-09-30), [DateTime64 bounds](value-contract-history.md#clickhouse-datetime649-server-boundary-behavior-2026-09-29) |
| B3-OOS-MONGO-1 | BSON DateTime grid edits finer than one millisecond | [DateTime precision](value-contract-history.md#mongodb-bson-datetime-grid-edit-precision) |
| B3-OOS-REDIS-1 | Pub/Sub, `MONITOR` and `CLIENT TRACKING` through the one-shot query interface | [Redis refusals](value-contract-history.md#redis-pubsub-and-monitor-stream-refusal-2026-09-29) |

## Bounded case record

Use one record per selected case in the case evidence/owning task:

```text
ID / owning sprint packet:
Engine + version; native type + declared metadata:
Consumer + session/configuration; boundary input:
Outcome: exact typed | exact text fallback | explicit refusal | untested
Existing evidence: SHA/fingerprint + selector + report/artifact (or absent)
Independent native value/type oracle and mutation/refusal postconditions:
Smallest remaining gap / next action; affected files and layer ownership:
Status: open | reproduced | fixed with named proof | blocked/unrun acceptance
```

Use [the scenario survey](b3-test-scenario-survey.md) for candidate boundaries
and [the validation playbook](validation-playbook.md#turn-every-finding-into-a-regression)
for execution. A defect needs initial failure, narrow correction and valid/
invalid neighbors. Native tests assert stored kind/value and unchanged siblings.
Register selectors/fixture ownership and preserve sanitized evidence. Keep
unavailable reports and mutation uncertainty explicit.

Oracle/new engines are later driver projects. They must follow ADR 0007 with
their own native semantics; another engine's fixtures are not their proof.

## Historical reconciliation

[Type-contract history](type-contract-history.md) retains the September 28–
October 2 native/consumer matrix and follow-ups. For an exact case, search its
engine/type heading and inspect only that section and the referenced test.
