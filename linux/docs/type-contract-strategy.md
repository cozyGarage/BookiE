# B3 type and consumer work board

The shared technical standard is [ADR 0007](decisions/0007-type-and-value-preservation.md).
This page owns remaining B3 work; [the sprint](bookie-0.2-sprint.md) owns order
and acceptance. Updated 2026-10-05; each new case links to its source-fingerprinted
evidence packet, and this summary is not itself runtime evidence.

## Current evidence and next targets

Detailed native cases and old counts are in [type-contract history](type-contract-history.md)
and [the value evidence index](value-contracts.md). Those records keep their
source/SHA attribution; this summary does not certify the current tree.

PostgreSQL inferred enum-array binding now has direct evidence for the
domain-over-enum `ANY($1)` predicate, including native type and wire-byte
oracles. Follow-up contracts cover containment and overlap operators with the
parameter on either side, including a same-named shadow enum earlier in
`search_path`; the bound OID and bytes continue to match the qualified target
type. A separate case now covers inferred scalar parameters to
`array_remove(ARRAY[status::base_enum], $1)` for text and SQL NULL; native typed
queries provide the result oracle while `pg_typeof` checks the parameter and
result types ([evidence](evidence/postgres-domain-enum-array-remove-results-2026-10-04/manifest.json)).
`array_position` is covered for the same inputs, including PostgreSQL's
`IS NOT DISTINCT FROM` behavior where SQL NULL matches a NULL array element
([evidence](evidence/postgres-domain-enum-array-position-results-2026-10-04/manifest.json)).
`array_replace` now covers two inferred scalar parameter slots together,
including NULL search/replacement values and both parameter types
([evidence](evidence/postgres-domain-enum-array-replace-results-2026-10-04/manifest.json)).
The same two-slot context now also has a shadowed-`search_path` contract: both
parameters retain the qualified target enum type, native array bytes match,
and shadow-only labels fail in either slot ([evidence](evidence/postgres-domain-enum-array-replace-shadowed-results-2026-10-05/manifest.json)).
The first array argument is now covered too: `array_position($1, enum_value)`,
`array_remove($1, enum_value)`, `array_append($1, enum_value)` and
`array_prepend(enum_value, $1)` infer the target enum array under that path,
including lower bounds, empty/NULL arrays, NULL elements and shadow-only label
refusal ([evidence](evidence/postgres-domain-enum-array-input-functions-shadowed-results-2026-10-05/manifest.json)).
`array_cat` now covers inferred text and SQL NULL array parameters for a
domain-over-enum source under a same-named shadow enum in `search_path`
([evidence](evidence/postgres-domain-enum-array-cat-results-2026-10-04/manifest.json)).
`array_positions` now checks inferred scalar enum parameters, repeated matches and
NULL-element matching under the same shadowed path
([evidence](evidence/postgres-domain-enum-array-positions-results-2026-10-04/manifest.json)).
All six PostgreSQL array comparison operators now cover inferred domain-over-enum
array parameters, NULL arrays, NULL elements, empty arrays and lower-bound-sensitive
comparisons against native results and wire bytes while a same-named shadow enum
leads transaction `search_path` ([evidence](evidence/postgres-domain-enum-array-equality-results-2026-10-04/manifest.json)).
The `array_append` and `array_prepend` function contexts now also preserve the
target enum type under a same-named shadow enum, verified with typed native
results, `pg_typeof`, result wire bytes, text labels including literal `NULL`,
SQL NULL and native invalid-label SQLSTATE ([evidence](evidence/postgres-domain-enum-array-functions-shadowed-results-2026-10-04/manifest.json)).
The PostgreSQL 16 `trim_array` boundary is now explicit: a standalone
`trim_array($1, n)` cannot infer an unknown polymorphic array parameter
(`42804`), while a schema-qualified array cast preserves NULL, empty, NULL
element and lower-bound inputs against native result and wire oracles under a
shadowed `search_path` ([evidence](evidence/postgres-trim-array-enum-inference-boundary-results-2026-10-05/manifest.json)).
Other inferred array operator/function contexts remain candidates; these cases
do not close the PostgreSQL or B3 matrix. See the [ANY evidence](evidence/postgres-domain-enum-any-array-parameter-results-2026-10-04/manifest.json),
[operator evidence](evidence/postgres-domain-enum-array-operators-results-2026-10-04/manifest.json),
and [shadowed-path evidence](evidence/postgres-shadowed-enum-array-operator-results-2026-10-04/manifest.json).

An inferred enum-array parameter with domain elements is covered through 63
domain layers and explicitly refused at 64 for both text and SQL NULL. The
array type's own hierarchy is included in this boundary; see the [depth evidence](evidence/postgres-inferred-enum-array-depth-results-2026-10-04/manifest.json).
At 63 layers PostgreSQL itself rejects scalar equality with `ANY(domain_array)`
as `42883`; the test keeps that native limitation distinct from containment
binding coverage.

MariaDB 11's `EMPTY_STRING_IS_NULL` mode converts empty-string parameters to
SQL NULL. MySQL/MariaDB CSV plans now reconstruct empty ENUM labels, empty SET
values, and empty CHAR/VARCHAR/TEXT destinations with `SPACE(0)`, keeping them
distinct from marker-bound SQL NULL; native mode contracts check ordinals,
masks, text bytes, NULL state, and a sibling row
([standalone mode](evidence/mariadb-empty-enum-set-empty-string-is-null-results-2026-10-05/manifest.json)).
The same round trip also passes with `STRICT_TRANS_TABLES`, `ANSI_QUOTES`, and
`NO_BACKSLASH_ESCAPES` enabled alongside that mode, including native verification
that the pre-existing sibling is unchanged
([combined-mode evidence](evidence/mariadb-empty-enum-set-combined-mode-results-2026-10-05/manifest.json)).

The MariaDB grid-edit contract now also covers both modes with parser input for
SQL NULL and explicit empty ENUM/SET values. Before the keyed-update fix, the
empty values became SQL NULL; the builder now uses `SPACE(0)` for empty MySQL
ENUM/SET edits while ordinary text remains parameter-bound. Native MySQL and
MariaDB checks pass across the existing mode matrix
([evidence](evidence/mariadb-empty-enum-set-grid-empty-string-mode-results-2026-10-05/manifest.json)).
SQL-file replay had the same mode-sensitive loss for empty ENUM and SET
values. The shared MySQL literal writer now emits `SPACE(0)` for empty text; a
core builder test also pins that expression for ordinary text columns. Native
SQL-file and typed CSV restore contracts compare ordinals, masks, exact bytes,
NULL state, and sibling rows separately under the standalone and combined MariaDB modes
([evidence](evidence/mariadb-empty-enum-set-sql-file-empty-string-mode-results-2026-10-05/manifest.json)).
The typed CSV contract also verifies empty VARCHAR through import and SQL-file
replay under both MariaDB mode combinations, against native bytes and NULL state
([evidence](evidence/mariadb-empty-varchar-import-empty-string-mode-results-2026-10-05/manifest.json)).

| Owner / engine | Retain established contracts | Remaining scope to select one case from |
| --- | --- | --- |
| PostgreSQL | Wide NUMERIC exact text; scalar/array/temporal/interval cases; extended DATE/TIMESTAMP/TIMESTAMPTZ exact-text support, including year 1,000,000 DATE and upper finite bounds through result, literal, parameter, CSV and keyed-edit paths; ordinary custom-enum labels and SQL NULL have direct scalar/array results plus schema-aware keyed-edit, draft-insert, all-operator structured-filter and CSV-import coverage; keyed edits select the correct native enum when the same type name exists in two schemas and the active search_path points at the shadow type; the literal label `NULL` survives keyed writes, direct scalar/array projections, filters and CSV import distinctly from SQL NULL; PostgreSQL 16 scalar/array projections, all shared structured filters, JSON and replayed SQL files confirm domain-over-enum values retain labels, SQL NULL and native type; catalog enum metadata also supports domain-column keyed updates and draft inserts; explicitly cast text/SQL NULL parameters, assignment-inferred text/SQL NULL updates, and base-enum comparison query parameters preserve domain-over-enum values and native type; three- through ten-level plus 63-, 64-, 65-, 128-, 129-, 256-, 257-, 258- and 259-level chains cover enum-leaf metadata and selected inferred parameters, writes and filters; raw inferred text and SQL NULL are refused from 64 levels while schema-aware operations pass through 259 levels ([259-layer evidence](evidence/postgres-domain-259-level-results-2026-10-04/manifest.json), [258-layer evidence](evidence/postgres-domain-258-level-results-2026-10-04/manifest.json), [257-layer evidence](evidence/postgres-domain-257-level-results-2026-10-04/manifest.json), [deep-domain follow-up](evidence/postgres-deep-domain-followup-results-2026-10-04/manifest.json)); raw PostgreSQL CSV export/import preserves domain-over-enum labels, empty text and SQL NULL with an explicit marker, while ambiguous default blanks are refused before writes; domain values also have native MCP `execute_query`, JSON/CSV `export_data` and CSV import proof; shared JSON rendering, core file-writer JSON/CSV/XML/HTML/Markdown and XLSX (non-empty enum strings remain text; empty enum labels are refused safely), SQL-file export replay, and MCP enum exports preserve the tested values; selected raw text/SQL NULL scalar query, update and transaction parameters adopt server-inferred enum types in covered contexts; inferred enum-array parameters also work for `ANY($1)`, containment, overlap, append/prepend, `array_remove`, `array_position`, `array_positions`, all six array comparison operators, and two-parameter `array_replace` ([ANY evidence](evidence/postgres-domain-enum-any-array-parameter-results-2026-10-04/manifest.json), [array-function evidence](evidence/postgres-domain-array-functions-results-2026-10-04/manifest.json), [array-remove evidence](evidence/postgres-domain-enum-array-remove-results-2026-10-04/manifest.json), [array-position evidence](evidence/postgres-domain-enum-array-position-results-2026-10-04/manifest.json), [array-replace evidence](evidence/postgres-domain-enum-array-replace-results-2026-10-04/manifest.json), [array-cat evidence](evidence/postgres-domain-enum-array-cat-results-2026-10-04/manifest.json), [array_positions evidence](evidence/postgres-domain-enum-array-positions-results-2026-10-04/manifest.json), [array equality evidence](evidence/postgres-domain-enum-array-equality-results-2026-10-04/manifest.json)); a same-named domain-over-enum collision preserves target metadata across 11 direct query-operator forms under shadowed transaction search paths | A raw domain-to-unknown comparison is explicitly refused by PostgreSQL with SQLSTATE 42883; casting the column to its base enum allows inferred text/NULL query parameters. Other session/search_path combinations, domain depths beyond 259, additional inferred parameter value types at or beyond 64 levels, and other custom/native cases need proof |
| MySQL/MariaDB | Signed/unsigned bounds, wide DECIMAL text, zero/extended temporals, BIT/spatial refusal, strict zero-date refusals, TIME/DATETIME/TIMESTAMP fractional mode behavior at FSP 0 through 6, exact-half boundaries for all three types at FSP 3, enum/set SQL-file, typed CSV, JSON, XML, HTML and Markdown consumers across the established twelve-mode matrix; MariaDB SQL-file and typed CSV empty ENUM/SET values also survive `EMPTY_STRING_IS_NULL` alone and with strict, ANSI, and backslash modes, with native ordinal/mask, byte, and NULL-state oracles; MySQL and MariaDB app grid edits refuse undeclared ENUM/SET values and preserve valid apostrophe/backslash labels and SET masks across all twelve modes, including strict modes alone and combined with ANSI_QUOTES and NO_BACKSLASH_ESCAPES, with literal `NULL` enum labels distinct from SQL NULL; MySQL and MariaDB distinguish explicit `''` empty ENUM/SET values from blank SQL NULL/required-field behavior ([grid evidence](evidence/mysql-enum-set-grid-edit-results-2026-10-04/manifest.json)); XLSX safely refuses empty SET text, locally completed zero-row metadata/bounded query regressions, and an app parser-to-keyed-grid edit for all three temporal types plus MySQL CSV/JSON export and CSV import round-trip coverage with UTC TIMESTAMP instant and sibling-row checks ([strict-date evidence](evidence/mysql-strict-zero-date-results-2026-10-04/manifest.json), [enum/set SQL-mode evidence](evidence/mysql-enum-sql-mode-results-2026-10-04/manifest.json), [TIME mode evidence](evidence/mysql-fractional-time-mode-results-2026-10-04/manifest.json), [TIME exact-half evidence](evidence/mysql-fractional-time-tie-results-2026-10-04/manifest.json), [DATETIME/TIMESTAMP exact-half evidence](evidence/mysql-fractional-half-boundary-matrix-results-2026-10-04/manifest.json), [TIME precision matrix](evidence/mysql-time-precision-matrix-results-2026-10-04/manifest.json), [DATETIME/TIMESTAMP precision matrix](evidence/mysql-datetime-timestamp-precision-matrix-results-2026-10-04/manifest.json), [app temporal grid edit](evidence/mysql-temporal-grid-edit-results-2026-10-04/manifest.json), [fractional DATETIME evidence](evidence/mysql-fractional-datetime-mode-results-2026-10-04/manifest.json), [fractional TIMESTAMP evidence](evidence/mysql-fractional-timestamp-mode-results-2026-10-04/manifest.json), [native regressions](evidence/mysql-atomic-results-2026-10-03/manifest.json)) | Other SQL mode/session configurations beyond this enum/SET matrix, additional file formats and installed typed edits; retain UTC pool versus dedicated-session distinctions |
| SQLite | Dynamic storage classes, exact bytes/NULL, typed affinity consumer cases, REAL float boundary refusal/preservation, and STRICT `ANY` edits that retain existing INTEGER/REAL/TEXT classes; clearing an existing TEXT cell stores empty TEXT, empty NULL/new cells stay NULL, nonempty NULL/new input stays TEXT, and BLOB runtime values are read-only with exact bytes preserved. Direct table-column query results recover declared `ANY` metadata for query, bound-query and transaction consumers, including empty results; mixed `CASE`, `COALESCE`, compound `UNION`/CTE/derived, `NULLIF`, `MIN`, `MAX`, `group_concat()`, `SUM`, `TOTAL`, `AVG`, arithmetic, `json_extract()` and `CAST(... AS BLOB)` results keep fallback metadata and exact per-row storage classes ([CASE evidence](evidence/sqlite-computed-any-results-2026-10-04/manifest.json), [COALESCE evidence](evidence/sqlite-coalesce-any-results-2026-10-04/manifest.json), [compound-result evidence](evidence/sqlite-union-any-results-2026-10-04/manifest.json), [NULLIF evidence](evidence/sqlite-nullif-any-results-2026-10-04/manifest.json), [MIN evidence](evidence/sqlite-min-any-results-2026-10-04/manifest.json), [GROUP_CONCAT evidence](evidence/sqlite-group-concat-any-results-2026-10-04/manifest.json), [MAX evidence](evidence/sqlite-max-any-results-2026-10-04/manifest.json), [SUM evidence](evidence/sqlite-sum-any-results-2026-10-04/manifest.json), [TOTAL evidence](evidence/sqlite-total-any-results-2026-10-04/manifest.json), [AVG evidence](evidence/sqlite-avg-any-results-2026-10-04/manifest.json), [arithmetic evidence](evidence/sqlite-arithmetic-any-results-2026-10-04/manifest.json), [json_extract evidence](evidence/sqlite-json-extract-any-results-2026-10-04/manifest.json), [CAST-to-BLOB evidence](evidence/sqlite-cast-blob-any-csv-results-2026-10-04/manifest.json)); compound-result and `json_extract()` JSON/XLSX cell kinds, plus compound-result CSV text ([export evidence](evidence/sqlite-union-any-export-results-2026-10-04/manifest.json), [json_extract evidence](evidence/sqlite-json-extract-any-results-2026-10-04/manifest.json)) and typed CSV storage-class round trips for compound, MIN, MAX, GROUP_CONCAT, SUM, TOTAL, AVG, arithmetic, `json_extract()` and CAST-to-BLOB results ([typed CSV evidence](evidence/sqlite-union-any-csv-roundtrip-results-2026-10-04/manifest.json), [GROUP_CONCAT evidence](evidence/sqlite-group-concat-any-results-2026-10-04/manifest.json), [MIN evidence](evidence/sqlite-min-any-results-2026-10-04/manifest.json), [MAX evidence](evidence/sqlite-max-any-results-2026-10-04/manifest.json), [SUM evidence](evidence/sqlite-sum-any-results-2026-10-04/manifest.json), [TOTAL evidence](evidence/sqlite-total-any-results-2026-10-04/manifest.json), [AVG evidence](evidence/sqlite-avg-any-results-2026-10-04/manifest.json), [arithmetic evidence](evidence/sqlite-arithmetic-any-results-2026-10-04/manifest.json), [json_extract evidence](evidence/sqlite-json-extract-any-results-2026-10-04/manifest.json)); query-result CSV round-trips storage classes with native `typeof()` proof ([nonempty result evidence](evidence/sqlite-strict-any-query-csv-results-2026-10-03/manifest.json), [empty result evidence](evidence/sqlite-query-empty-metadata-results-2026-10-03/manifest.json), [attached-schema origins](evidence/sqlite-attached-any-metadata-results-2026-10-03/manifest.json)); compound-result and direct table-projection workbooks both keep formula-shaped text as text through LibreOffice Calc ODS/XLSX re-save ([formula-text evidence](evidence/sqlite-xlsx-calc-formula-text-results-2026-10-04/manifest.json)) | Other computed-expression shapes beyond CASE/COALESCE/compound/NULLIF/MIN/MAX/GROUP_CONCAT/SUM/TOTAL/AVG/arithmetic/json_extract/CAST-to-BLOB, storage-class/affinity mixtures, spreadsheet-app re-import beyond the two tested Calc workbook shapes and installed editing |
| SQL Server | Decimal/offset/calendar contracts; all 300 legacy `datetime` ticks use exact typed or style-126 text values checked against native text and parameter byte round trips; representative SQL-literal restores match bytes; app grid display/parser/keyed updates preserve representative exact and fallback legacy `datetime` ticks plus sibling rows ([tick evidence](evidence/mssql-legacy-datetime-ticks-results-2026-10-04/manifest.json), [grid evidence](evidence/mssql-legacy-datetime-grid-edit-results-2026-10-04/manifest.json)); `datetimeoffset` grid edits preserve local text, scale, offset and native bytes, with catalog scale and UTC-range refusal ([grid evidence](evidence/mssql-datetimeoffset-grid-edit-results-2026-10-04/manifest.json)); money/variant refusals; U1 server-owned metadata | Other native/consumer gaps, metadata precision, U1 other consumers/ledger and U2 identity copying; O2 large values/O3 result sets need reproduction |
| ClickHouse | Wide/nested exact representations, DateTime64 precision/bounds/zones and type-less write refusal | Remaining nested/type/consumer combinations and installed editing; no generic type-less JSON binding |
| Redis | RESP3 tagged nested values/binary kinds, caps and persistent-stream refusal | Remaining protocol/native consumer coverage; display conversion does not prove subscription support |
| MongoDB | Canonical Extended JSON, BSON width/subtypes and typed keyed edits; browse metadata and retained page rows come from one collection cursor; an off-page type change observed later in that cursor marks the column `mixed` and the visible rows read-only; a native `serverStatus` find-command counter asserts exactly one page `find`; selected-row `run_find` type changes after census merge to `mixed`, with CSV/JSON preserving materialized Extended JSON; explicit BSON null and missing sparse-document fields have distinct result markers; stale grid edits compare each edited field's original value; keyed deletes compare all materialized non-key values, with native tests for changed payloads, untouched sibling fields, removed fields and BSON NULL becoming a NULL-containing array ([edit evidence](evidence/mongodb-stale-grid-edit-results-2026-10-04/manifest.json), [delete evidence](evidence/mongodb-stale-grid-delete-results-2026-10-04/manifest.json)) | larger-scale full-scan performance budgets, ABA edits/deletes, new fields outside the materialized column set, off-page `run_find` type changes after census, remaining consumer combinations and installed editing |
| DuckDB (optional) | Wide integers, intervals, DuckDB enum result/literal/bound labels, keyed edits and raw CSV export/import (empty text, literal `NULL`, Unicode, quotes, formula-shaped text and SQL NULL retain native ENUM and a sibling row); spreadsheet-safe CSV prefixes formulas, and its typed import plan shows a collision when labels overlap ([CSV evidence](evidence/duckdb-enum-csv-roundtrip-results-2026-10-04/manifest.json)); extended DATE/TIMESTAMP/TIMESTAMPTZ text fallbacks; tested nested refusals | Remaining nested combinations, native sub-microsecond bindings and installed GTK editing; text transport is not native binding |

| Shared consumers | Named parameters, export/import, grid/filter, JSON/MCP and workbook cases; grid context-menu CSV uses a collision-free SQL NULL marker ([serializer evidence](evidence/postgres-enum-clipboard-csv-results-2026-10-04/manifest.json)); scalar enum and seventeen array XLSX shapes, including `text[]`, `uuid[]`, `date[]`, `timestamp[]`, `time[]`, `timetz[]`, `boolean[]`, `int8[]`, `smallint[]` and `integer[]`, survive Calc ODS/XLSX re-save ([text[] evidence](evidence/postgres-text-array-calc-reimport-results-2026-10-05/manifest.json), [uuid[] evidence](evidence/postgres-uuid-array-calc-reimport-results-2026-10-05/manifest.json), [date[] evidence](evidence/postgres-date-array-calc-reimport-results-2026-10-05/manifest.json), [timestamp[] evidence](evidence/postgres-timestamp-array-calc-reimport-results-2026-10-05/manifest.json), [time[] evidence](evidence/postgres-time-array-calc-reimport-results-2026-10-05/manifest.json), [timetz[] evidence](evidence/postgres-timetz-array-calc-reimport-results-2026-10-05/manifest.json), [boolean[] evidence](evidence/postgres-boolean-array-calc-reimport-results-2026-10-05/manifest.json), [int8[] evidence](evidence/postgres-int8-array-calc-reimport-results-2026-10-05/manifest.json), [integer arrays evidence](evidence/postgres-integer-arrays-calc-reimport-results-2026-10-05/manifest.json)); 86,012 finite-f64 values round-trip through CSV and JSON with exact bits ([corpus evidence](evidence/finite-float-consumer-results-2026-10-04/manifest.json)) | Remaining format/type/configuration boundaries, native kind after writes, OS clipboard delivery/paste and restore, other spreadsheet applications/shapes, exhaustive finite-f64 enumeration and refused-operation postconditions |
| Evidence / mutation | Registered fixtures, independent oracles and existing scoped mutation results; the October 5 strict GTK + DuckDB value tier passed 316 selected tests across all 11 suites ([run evidence](evidence/local-gtk-duckdb-value-tier-results-2026-10-05/manifest.json)); the separate 315-test local integration result remains at `d783182f` ([run evidence](evidence/local-integration-tier-results-2026-10-04/manifest.json)) | Portable raw proof for missing mutation reports; genuine survivors/timeouts and uncovered consumers remain open |

SQLite computed `CAST(value AS BLOB)` results now have JSON and XLSX text-cell
assertions plus typed CSV restore, all checked against native storage classes
and exact bytes
([evidence](evidence/sqlite-cast-blob-any-csv-results-2026-10-04/manifest.json));
other computed-expression shapes remain open.

PostgreSQL scalar custom-enum XLSX values now survive LibreOffice Calc ODS/XLSX
re-save with formula-shaped labels retained as text and SQL NULL left blank
([evidence](evidence/postgres-enum-scalar-calc-reimport-results-2026-10-04/manifest.json));
other spreadsheet applications and cell shapes remain open.

The PostgreSQL enum CSV importer now also has a round trip for double quotes,
an embedded line break, backslashes and formula-shaped text across four
delimiters and three record endings; auto-detection identifies each format,
and native UTF-8 byte/type assertions verify import ([evidence](evidence/postgres-enum-csv-quoted-lines-results-2026-10-04/manifest.json)).

PostgreSQL enum SQL-file output also replays a backslash-bearing label with
all four `standard_conforming_strings` and `backslash_quote` combinations
(`on/safe_encoding`, `on/off`, `off/off`, `off/on`); backslashes use explicit
`E''` literals ([initial evidence](evidence/postgres-enum-sql-literal-session-modes-results-2026-10-04/manifest.json),
[full setting matrix](evidence/postgres-enum-sql-literal-session-modes-full-results-2026-10-05/manifest.json)).

PostgreSQL custom enum-array CSV import now has native round trips for NULL
arrays versus empty arrays and NULL elements, lower-bound-zero and
two-dimensional shapes, literal `NULL` and empty labels, Unicode, and sibling
row preservation ([shape evidence](evidence/postgres-enum-array-csv-shapes-results-2026-10-04/manifest.json)).
The CSV import also selects the table's enum when an identically named type in
another schema shadows it through `search_path` ([evidence](evidence/postgres-shadowed-enum-array-import-results-2026-10-04/manifest.json)).
Scalar-enum CSV restore has the same configuration check with a target-only
label, empty text, literal `NULL`, SQL NULL and an unchanged sibling row
([evidence](evidence/postgres-enum-csv-shadow-search-path-results-2026-10-05/manifest.json)).
The import plan also remains bound to its destination enum when a fresh
execution connection changes the role's `search_path` to put a different
same-named decoy enum first between planning and execution; native target type,
values and untouched shadow/sibling rows are checked
([evidence](evidence/postgres-enum-csv-search-path-transition-results-2026-10-05/manifest.json)).
Custom enum-array grid edits now have app-parser and native keyed-write
coverage, including invalid-label refusal and sibling preservation ([evidence](evidence/postgres-enum-array-grid-edit-results-2026-10-04/manifest.json)).
The grid parser and live write path distinguish blank SQL NULL from the empty
array literal `{}` ([evidence](evidence/postgres-enum-array-grid-null-results-2026-10-04/manifest.json)).
Default CSV now likewise imports blank as a SQL NULL array and `{}` as an empty
array; scalar enum blank refusal remains in place ([evidence](evidence/postgres-enum-array-default-null-results-2026-10-04/manifest.json)).
Arrays of domains over enums now import through their qualified domain-array
type, preserving domain checks and native array bytes ([evidence](evidence/postgres-domain-enum-array-import-results-2026-10-04/manifest.json)).
That import also remains bound to the target schema when `search_path` starts
with an identically named shadow domain ([shadowed-path evidence](evidence/postgres-domain-enum-array-shadowed-import-results-2026-10-04/manifest.json)).
The same type also has app-parser and live keyed-grid coverage with domain-check
refusal and rollback ([evidence](evidence/postgres-domain-enum-array-grid-results-2026-10-04/manifest.json)).
Structured equality filtering is checked against native array JSON and wire
values ([evidence](evidence/postgres-domain-enum-array-filter-results-2026-10-04/manifest.json)).
The same array now has JSON, CSV, XML, HTML, Markdown and XLSX file-writer
coverage plus typed SQL replay checked against native type, JSON and wire
oracles ([file-writer evidence](evidence/postgres-domain-enum-array-filewriter-results-2026-10-04/manifest.json)).
A UUID-domain array now decodes by its base OID, with a typed keyed edit, CHECK
refusal and sibling preservation ([evidence](evidence/postgres-domain-uuid-array-results-2026-10-04/manifest.json)).
Text, numeric and timestamptz domain arrays also pass qualified binding and
native JSON/wire comparisons in a non-UTC session
([matrix evidence](evidence/postgres-domain-array-family-results-2026-10-04/manifest.json)).
Domain arrays over `bytea` preserve non-UTF-8 bytes, empty elements and NULL
through qualified binding and keyed edits; domain CHECK refusal preserves the
row and its sibling, with native JSON/wire oracles
([evidence](evidence/postgres-domain-bytea-array-results-2026-10-04/manifest.json)).
The keyed edit also succeeds with a stricter same-named domain first in
`search_path`, proving the generated cast resolves to the target schema
([shadowed-path evidence](evidence/postgres-shadowed-domain-bytea-array-results-2026-10-04/manifest.json)).
The custom `enum[]` XLSX text cell also survives a LibreOffice Calc re-save as
ODS and then XLSX ([evidence](evidence/postgres-enum-array-calc-reimport-results-2026-10-04/manifest.json)).
The `bytea[]` XLSX text cell now survives the same Calc re-import with its
binary, empty and SQL NULL elements intact
([evidence](evidence/postgres-bytea-array-calc-reimport-results-2026-10-04/manifest.json)).
A `timestamptz[]` XLSX cell also preserves repeated-hour instants, a BC instant,
infinities and SQL NULL through Calc's ODS/XLSX re-save
([evidence](evidence/postgres-timestamptz-array-calc-reimport-results-2026-10-04/manifest.json)).
A PostgreSQL `interval[]` XLSX cell preserves mixed signs, microseconds, zero
intervals and SQL NULL through the same Calc re-save
([evidence](evidence/postgres-interval-array-calc-reimport-results-2026-10-04/manifest.json)).

SQLite computed `CASE` and `COALESCE` results now have native app CSV round
trips with storage-class assertions; see the [CASE evidence](evidence/sqlite-case-any-csv-roundtrip-results-2026-10-04/manifest.json)
and [COALESCE evidence](evidence/sqlite-coalesce-any-csv-roundtrip-results-2026-10-04/manifest.json).
The CASE path also verifies empty TEXT round-trips separately from NULL in CSV
and that XLSX refuses the ambiguous cell without replacing the destination;
see the [refusal evidence](evidence/sqlite-case-any-xlsx-refusal-results-2026-10-04/manifest.json).
The COALESCE JSON output checks numeric versus string values, and the XLSX output
checks numeric versus shared-string cells and BLOB text encoding against its
computed runtime kinds; see the [consumer evidence](evidence/sqlite-coalesce-any-xlsx-results-2026-10-04/manifest.json).

LibreOffice Calc now imports and re-saves one compound STRICT `ANY` workbook
while preserving numeric `42`, text `42` and blank SQL NULL as distinct cell
kinds ([re-import evidence](evidence/sqlite-xlsx-calc-reimport-results-2026-10-04/manifest.json)).
Other workbook cases, spreadsheet applications and installed editing remain open.

One PostgreSQL `text[]` result now has XML, HTML, Markdown and XLSX file-writer
coverage plus a replayed SQL export, checked against native array JSON and wire
oracles; see the [array export evidence](evidence/postgres-array-filewriter-results-2026-10-04/manifest.json).
Other array families remain open; spreadsheet-application re-import is covered
for `text[]`, `uuid[]`, `date[]`, `timestamp[]`, `time[]`, `timetz[]`, `boolean[]`, `int8[]`, `smallint[]`, `integer[]`, `enum[]`, domain-over-enum `[]`, `bytea[]`,
`timestamptz[]`, `interval[]`, `numeric[]` and `float8[]` XLSX cells ([text[] Calc evidence](evidence/postgres-text-array-calc-reimport-results-2026-10-05/manifest.json), [uuid[] Calc evidence](evidence/postgres-uuid-array-calc-reimport-results-2026-10-05/manifest.json), [numeric[] Calc evidence](evidence/postgres-numeric-array-calc-reimport-results-2026-10-04/manifest.json), [float8[] Calc evidence](evidence/postgres-float8-array-calc-reimport-results-2026-10-04/manifest.json)), with other shapes unverified (see the [enum[] evidence](evidence/postgres-enum-array-calc-reimport-results-2026-10-04/manifest.json),
[bytea[] evidence](evidence/postgres-bytea-array-calc-reimport-results-2026-10-04/manifest.json)
and [timestamptz[] evidence](evidence/postgres-timestamptz-array-calc-reimport-results-2026-10-04/manifest.json),
[interval[] evidence](evidence/postgres-interval-array-calc-reimport-results-2026-10-04/manifest.json)).

A PostgreSQL `bytea[]` with non-UTF-8 bytes, an empty element and SQL NULL now
has JSON, CSV, XLSX and replayed SQL file-writer coverage against native element,
JSON and wire oracles ([evidence](evidence/postgres-bytea-array-filewriter-results-2026-10-04/manifest.json)).

The ordinary custom-enum shadow-schema case checks structured `=`, `IN` and
`BETWEEN` filters on `enum_shadow_b.items` while `search_path` resolves the same
enum name in `enum_shadow_a`; native target type/value checks pass. See the
[initial equality evidence](evidence/postgres-shadowed-enum-filter-results-2026-10-03/manifest.json)
and [list/range follow-up](evidence/postgres-shadowed-enum-filter-matrix-results-2026-10-03/manifest.json).
A direct equality, `IN` and `BETWEEN` query with bound text values also resolve
against the qualified target enum and leave the shadow row untouched; see the
[query-parameter evidence](evidence/postgres-shadowed-enum-parameter-results-2026-10-03/manifest.json).
The keyed-edit regression now changes ordinary session `search_path` twice
inside one transaction on the same backend, then verifies target-schema enum
metadata and write safety under the final shadowed path; see the
[session evidence](evidence/postgres-enum-session-search-path-results-2026-10-04/manifest.json).

A quoted-identifier enum case verifies catalog metadata, keyed edit, draft
insert and typed filtering when both schema and type names contain spaces and
embedded quotes. PostgreSQL catalog values confirm the exact stored enum type
and values; see [quoted enum identifier evidence](evidence/postgres-quoted-enum-identifiers-results-2026-10-04/manifest.json).
The same contract changes `search_path` with `SET LOCAL` and verifies that an
uncast `status = $1` query still infers the target table's enum type.
When `pg_typeof($1)` leaves an enum comparison parameter ambiguous, native
PostgreSQL preparation returns `42P08`; an explicit qualified enum cast makes
the parameter type unambiguous. The driver preserves native `42P08` if its
text-typed fallback conflicts with the enum operator. The regression pins both
errors and the explicit-cast success in that evidence packet.

The PostgreSQL 16 ordinary-enum and domain-over-enum structured filters now
cover every shared `FilterOp`, including range comparisons, text-pattern
filters, both NULL operators and both list operators, with exact rows/labels and
native type assertions; see the
[complete filter matrix](evidence/postgres-enum-filter-matrix-results-2026-10-03/manifest.json).
For direct query parameters, base-enum-cast `=`, `<>`, `<`, `<=`, `>`, `>=`,
`IN`, `NOT IN`, `BETWEEN`, `IS DISTINCT FROM` and `IS NOT DISTINCT FROM`
preserve inferred parameter types; raw domain
`= $1` remains explicitly refused with SQLSTATE 42883. See the
[query-operator evidence](evidence/postgres-domain-enum-query-operators-results-2026-10-03/manifest.json)
and the [ordering](evidence/postgres-domain-enum-param-operator-results-2026-10-03/manifest.json)
and [list-operator evidence](evidence/postgres-domain-enum-param-list-results-2026-10-03/manifest.json),
plus [NULL-safe distinctness evidence](evidence/postgres-domain-enum-distinct-parameter-results-2026-10-03/manifest.json).
Other direct query-parameter contexts, session configurations, domain chains
deeper than seven layers and other custom/native cases remain open.

PostgreSQL 16 also infers text and SQL NULL as the custom enum in both argument
positions of `COALESCE` and in `array_append(ARRAY[enum_column], $1)`. Native
`pg_typeof`, exact values, NULL behavior, native `22P02` invalid-label refusals
and unchanged source rows are asserted
([evidence](evidence/postgres-enum-expression-parameter-results-2026-10-04/manifest.json)).
A CASE result context now covers `$1` in both `THEN`/`ELSE` positions. The
literal `NULL` label, empty enum label and SQL NULL are compared with explicitly
typed native expressions; `pg_typeof` verifies parameter and result types, and
invalid labels retain native `22P02` behavior
([evidence](evidence/postgres-enum-case-parameter-inference-results-2026-10-05/manifest.json)).
`GREATEST` and `LEAST` now cover the parameter in both argument positions,
including their NULL handling, against explicitly typed enum expressions and
`pg_typeof` results ([evidence](evidence/postgres-enum-greatest-least-parameter-inference-results-2026-10-05/manifest.json)).
These `COALESCE`, `NULLIF`, `GREATEST` and `LEAST` contexts now also have a native
same-named-shadow `search_path` contract for ordinary custom enums, with
empty/literal-`NULL` labels, SQL NULL, type checks and shadow-only label refusal
([evidence](evidence/postgres-shadowed-enum-parameter-context-results-2026-10-05/manifest.json)).
A follow-up also verifies `NULLIF(enum_column, $1)` infers both text and SQL
NULL parameters as the native enum, preserves exact results, and returns native
`22P02` for an invalid label
([evidence](evidence/postgres-enum-nullif-parameter-results-2026-10-04/manifest.json)).
A shadow-schema domain-over-enum follow-up confirms raw `NULLIF(status, $1)`
retains PostgreSQL's `42883` refusal; casting the column to its qualified
base enum enables text/NULL inference without resolving to the shadow type.
It also checks literal `NULL`, invalid-label `22P02`, result types and unchanged
domain rows ([evidence](evidence/postgres-domain-nullif-parameter-results-2026-10-04/manifest.json)).

A domain-over-enum `COALESCE` contract now covers text and SQL NULL in both
argument positions. PostgreSQL infers the parameter and expression as the base
enum while the source column remains the outer domain, verified with `pg_typeof`
and exact rows ([evidence](evidence/postgres-domain-coalesce-results-2026-10-04/manifest.json)).
The same domain-over-enum path now covers both CASE result branches while a
same-named shadow enum leads `search_path`; the target base enum parameter and
CASE result types, outer domain type, empty/literal-NULL/SQL-NULL values and
invalid-label refusal are all checked against native expressions
([evidence](evidence/postgres-domain-case-shadowed-parameter-results-2026-10-05/manifest.json)).
Both domain-over-enum `COALESCE` parameter positions now have the same shadowed
path checks against explicit native casts, including empty text, literal
`NULL`, SQL NULL and invalid shadow-only labels
([evidence](evidence/postgres-domain-coalesce-shadowed-results-2026-10-05/manifest.json)).
The same text/NULL inference now covers `array_append` and `array_prepend`: the
input constructor retains `state_domain[]`, while the polymorphic function result
uses `state[]` ([array-function evidence](evidence/postgres-domain-array-functions-results-2026-10-04/manifest.json)).

The three-level domain chain also covers directly inferred text and SQL NULL
parameters for both NULL-safe distinctness operators, with `pg_typeof` checks
for the leaf enum and outer domain. See the
[three-level query-parameter evidence](evidence/postgres-three-level-enum-query-operator-results-2026-10-03/manifest.json).

A separate PostgreSQL 16 contract follows four nested domains to the enum leaf,
then verifies fetched enum metadata, a keyed update, a typed equality filter,
the stored outer-domain type and an untouched SQL NULL row. See the
[four-domain-layer evidence](evidence/postgres-four-level-domain-results-2026-10-04/manifest.json).

A five-level contract also checks enum-leaf metadata, a schema-aware keyed
edit, inferred query/update parameters, a typed filter and invalid-label
refusal while preserving the outer domain and a SQL NULL sibling. See the
[five-domain-layer evidence](evidence/postgres-five-level-domain-results-2026-10-04/manifest.json).

A contract at depths 6, 7, 8, 9, 10, 63, 64, 65 and 128 checks recursive enum-leaf
metadata under a session `search_path` shadowed by a same-named enum, schema-aware
keyed and draft writes (including SQL NULL), typed equality filters and exact
outer-domain type/value while preserving a SQL NULL sibling. A same-backend
transaction changes `search_path` twice and repeats typed writes/filtering with
previously fetched metadata; rollback leaves the original rows intact. Raw
inferred SQL NULL updates preserve the outer domain type and invalid labels
reach PostgreSQL through 63 layers. At depths 64, 65 and 128, raw inferred text
(valid and invalid) and SQL NULL return an explicit unsupported result even
after schema-aware work in the same-backend transaction; schema-aware
operations pass through 256 layers. Domain depths beyond 256 and other
enum/session configurations remain open; see the
[deep-domain boundary evidence](evidence/postgres-deep-domain-results-2026-10-04/manifest.json),
[129/256-layer follow-up](evidence/postgres-deep-domain-followup-results-2026-10-04/manifest.json),
[domain-over-enum array parameter evidence](evidence/postgres-domain-enum-array-parameter-results-2026-10-04/manifest.json),
[domain-over-enum array bounds evidence](evidence/postgres-domain-enum-array-bounds-results-2026-10-04/manifest.json),
[six/seven-domain evidence](evidence/postgres-seven-domain-results-2026-10-04/manifest.json),
[eight-domain follow-up](evidence/postgres-eight-domain-results-2026-10-04/manifest.json)
and the [nine-domain follow-up](evidence/postgres-nine-domain-results-2026-10-04/manifest.json),
plus the [six-domain checkpoint](evidence/postgres-six-level-domain-results-2026-10-04/manifest.json).

PostgreSQL custom-enum results now cover accepted 63-byte ASCII and multibyte
UTF-8 scalar and array labels plus refusal of a 64-byte label without partial type creation; see
the [accepted boundary](evidence/postgres-enum-label-byte-boundary-results-2026-10-04/manifest.json)
and [refusal evidence](evidence/postgres-enum-overlength-refusal-results-2026-10-04/manifest.json).
An ordering contract checks non-lexical enum sorting against `pg_enum.enumsortorder`
and native enum types ([ordering evidence](evidence/postgres-enum-order-results-2026-10-04/manifest.json)).
Schema/type/table identifiers with spaces and embedded quotes pass metadata
discovery, a keyed edit and structured equality filtering with exact native
type and sibling-row checks; CSV import now also emits the escaped,
schema-qualified enum cast under a same-named shadow path and preserves the
target type and shadow row ([spaces evidence](evidence/postgres-enum-quoted-identifiers-results-2026-10-04/manifest.json),
[CSV import evidence](evidence/postgres-enum-csv-quoted-identifiers-results-2026-10-05/manifest.json),
[quote-escaping evidence](evidence/postgres-enum-identifier-escaping-results-2026-10-04/manifest.json));
the same-name target/shadow type collision also passes keyed edit and filtering
with the role's default `search_path` aimed at the shadow schema ([shadow-path evidence](evidence/postgres-enum-shadowed-quoted-search-path-results-2026-10-04/manifest.json));
other identifier forms and transaction/session `search_path` permutations remain open.

SQLite STRICT `ANY` table and direct query-result CSV now tag INTEGER,
REAL, TEXT and BLOB cells so a native import can retain their runtime storage
classes; SQL NULL uses the export's explicit collision-free marker. Untagged
text stays text, while a blank without a marker and malformed reserved tags are
refused. Both app round trips have a native `typeof()`/value oracle. Empty query
results retain their column metadata and CSV header. Other computed-expression
shapes, attached-origin patterns and formats, and installed editing remain open;
see the
[table CSV case](value-contract-history.md#sqlite-strict-any-csv-storage-class-round-trip-2026-10-03)
and [query-result case](value-contract-history.md#sqlite-strict-any-query-result-csv-round-trip-2026-10-03).
See the [computed-expression case](evidence/sqlite-computed-any-results-2026-10-04/manifest.json)
and [attached-origin case](evidence/sqlite-attached-any-metadata-results-2026-10-03/manifest.json) for the tested fallback behavior.

Direct attached-schema origins now recover declared metadata when SQLite's
database list and schema-qualified table metadata identify one source. If a
main-schema table name with a dot collides with that flattened origin, metadata
stays at the original fallback. See the
[attached-origin evidence](evidence/sqlite-attached-any-metadata-results-2026-10-03/manifest.json).

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
| B3-OOS-MSSQL-1 | `money`/`smallmoney` and `sql_variant` | [Variant](value-contract-history.md#sql-server-sql_variant-metadata-refusal), [money](value-contract-history.md#sql-server-money-float-decoding-refusal) |
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
