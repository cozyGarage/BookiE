# B3 type and consumer work board

The shared technical standard is [ADR 0007](decisions/0007-type-and-value-preservation.md).
This page owns remaining B3 work; [the sprint](bookie-0.2-sprint.md) owns order
and acceptance. Consolidated 2026-10-03 at `0cf70382e`; no tests rerun here.

## Current evidence and next targets

Detailed native cases and old counts are in [type-contract history](type-contract-history.md)
and [the value evidence index](value-contracts.md). Those records keep their
source/SHA attribution; this summary does not certify the current tree.

| Owner / engine | Retain established contracts | Remaining scope to select one case from |
| --- | --- | --- |
| PostgreSQL | Wide NUMERIC exact text; scalar/array/temporal/interval cases; extended DATE/TIMESTAMP/TIMESTAMPTZ exact-text support, including year 1,000,000 DATE and upper finite bounds through result, literal, parameter, CSV and keyed-edit paths; ordinary custom-enum labels and SQL NULL have direct scalar/array results plus schema-aware keyed-edit, draft-insert, structured-filter and CSV-import coverage; the literal label `NULL` survives keyed writes, direct scalar/array projections, filters and CSV import distinctly from SQL NULL; a PostgreSQL 16 scalar projection confirms a domain over an enum preserves labels, SQL NULL and its reported domain type; shared JSON rendering, core file-writer JSON/CSV/XML/HTML/Markdown and XLSX (non-empty enum strings remain text; empty enum labels are refused safely), SQL-file export replay, and MCP `execute_query`/JSON/CSV `export_data` preserve the tested enum values; raw text/SQL NULL query, update and transaction parameters adopt server-inferred enum types when context identifies them | Additional enum format/session configurations, domain consumers beyond scalar projection, and other custom/native cases need proof |
| MySQL/MariaDB | Signed/unsigned bounds, wide DECIMAL text, zero/extended temporals, BIT/spatial refusal, tested SQL modes, and locally completed zero-row metadata/bounded query regressions ([native regressions](evidence/mysql-atomic-results-2026-10-03/manifest.json)) | Remaining configuration/consumer cases and installed typed edits; retain UTC pool versus dedicated-session distinctions |
| SQLite | Dynamic storage classes, exact bytes/NULL and typed affinity consumer cases | Remaining storage-class/affinity mixtures, consumer paths and installed editing |
| SQL Server | Decimal/offset/calendar contracts and temporal/money/variant refusals; U1 server-owned metadata | Remaining native/consumer gaps, metadata precision, U1 other consumers/ledger and U2 identity copying; O2 large values/O3 result sets need reproduction |
| ClickHouse | Wide/nested exact representations, DateTime64 precision/bounds/zones and type-less write refusal | Remaining nested/type/consumer combinations and installed editing; no generic type-less JSON binding |
| Redis | RESP3 tagged nested values/binary kinds, caps and persistent-stream refusal | Remaining protocol/native consumer coverage; display conversion does not prove subscription support |
| MongoDB | Canonical Extended JSON, BSON width/subtypes and typed keyed edits; browse metadata and retained page rows come from one collection cursor; an off-page type change observed later in that cursor marks the column `mixed` and the visible rows read-only; a failpoint test asserts no second page `find`; explicit BSON null and missing sparse-document fields have distinct result markers | Full-scan cost, writes to documents already observed during the cursor, `run_find`'s separate schema/query reads, remaining consumer combinations and installed editing |
| DuckDB (optional) | Wide integers, intervals, enums and extended DATE/TIMESTAMP/TIMESTAMPTZ text fallbacks through CSV, parameters and keyed edits; tested nested refusals | Remaining nested combinations, native sub-microsecond bindings and installed editing; text transport is not native binding |
| Shared consumers | Named parameters, export/import, grid/filter, JSON/MCP and workbook cases | One missing format/type/configuration boundary, native kind after writes and refused-operation postconditions |
| Evidence / mutation | Registered fixtures, independent oracles and existing scoped mutation results | Portable raw proof for missing reports; genuine survivors/timeouts and uncovered consumers remain open |

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
