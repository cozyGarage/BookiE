# Type and value evidence index

The shared standard is [ADR 0007](decisions/0007-type-and-value-preservation.md).
[The B3 board](type-contract-strategy.md) owns remaining tasks. This page is a
bounded entry into case evidence, not another conversion policy or aggregate
support claim. Updated 2026-10-04; this index links evidence but does not run tests.

## Run

From `linux/`, use the existing value runner:

```bash
./scripts/test-value-contracts.sh --gtk --duckdb
./scripts/test-value-contracts.sh --gtk --duckdb --unit-only
```

The first selects all eight drivers and shared consumers. Six server engines
need Docker; SQLite/DuckDB are local, and app/grid compilation needs GTK libs.
Exclusions are recorded when features are omitted. Driver contracts use the
`value_contract` prefix; a new selector must have execution ownership.
Missing suites, zero matches, compilation/test errors and fixture timeouts fail.
Each suite has a 420-second timeout; reports/logs go to `target/quality/*-values/`.
See [testing](testing.md) and
[validation](validation-playbook.md) for affected layers and evidence requirements.

## Case lookup

Detailed native oracles, exact selectors and dated results are preserved in
[value-contract history](value-contract-history.md). Use these starting points,
then search that ledger/test for the specific type and consumer; one starting
point does not represent all support for that engine.

A PostgreSQL `bytea[]` file-writer case preserves binary, empty and NULL array
elements across JSON, CSV, XLSX and SQL replay ([evidence](evidence/postgres-bytea-array-filewriter-results-2026-10-04/manifest.json)).

A PostgreSQL custom `enum[]` XLSX cell preserves its NULL-label, empty, Unicode,
quoted and markup text through LibreOffice Calc's ODS/XLSX re-save
([evidence](evidence/postgres-enum-array-calc-reimport-results-2026-10-04/manifest.json)).

A PostgreSQL `bytea[]` XLSX cell containing binary bytes, empty bytea and SQL
NULL also survives Calc's ODS/XLSX re-save with its string contents unchanged
([evidence](evidence/postgres-bytea-array-calc-reimport-results-2026-10-04/manifest.json)).

A PostgreSQL `timestamptz[]` XLSX cell preserves both sides of a repeated hour,
a BC instant, infinities and SQL NULL through Calc's ODS/XLSX re-save as exact
text ([evidence](evidence/postgres-timestamptz-array-calc-reimport-results-2026-10-04/manifest.json)).

A PostgreSQL `interval[]` XLSX cell preserves mixed-sign intervals,
microseconds, zero intervals and SQL NULL through Calc's ODS/XLSX re-save as
exact text ([evidence](evidence/postgres-interval-array-calc-reimport-results-2026-10-04/manifest.json)).

A PostgreSQL `numeric[]` XLSX cell preserves wide precision, scale, NaN,
infinities and SQL NULL as one exact text value through Calc's ODS/XLSX re-save
([evidence](evidence/postgres-numeric-array-calc-reimport-results-2026-10-04/manifest.json)).

A PostgreSQL `float8[]` XLSX cell preserves an adjacent double, negative zero,
the minimum subnormal, NaN, infinities and SQL NULL as exact text through the
same Calc round trip ([evidence](evidence/postgres-float8-array-calc-reimport-results-2026-10-04/manifest.json)).

A PostgreSQL array of a domain over `bytea` preserves binary, empty and NULL
elements and its declared type through decoding, qualified binding and keyed
edits; invalid values are refused without changing either row
([evidence](evidence/postgres-domain-bytea-array-results-2026-10-04/manifest.json)).
Its keyed edit also stays bound to the target domain when `search_path` starts
with a same-named domain whose stricter CHECK rejects the value
([shadowed-path evidence](evidence/postgres-shadowed-domain-bytea-array-results-2026-10-04/manifest.json)).

One compound SQLite STRICT `ANY` workbook now preserves numeric `42`, text
`42` and blank SQL NULL through a LibreOffice Calc import, ODS save and XLSX
re-save ([evidence](evidence/sqlite-xlsx-calc-reimport-results-2026-10-04/manifest.json)).

The compound-result and direct table-projection XLSX workbooks both keep `=1+1`
and `'=1+1` as text through the Calc ODS/XLSX round trip, without creating
formulas; other spreadsheet applications and workbook shapes remain unverified
([formula-text evidence](evidence/sqlite-xlsx-calc-formula-text-results-2026-10-04/manifest.json)).

| Scope | Evidence entry points |
| --- | --- |
| PostgreSQL | [Scalar enum Calc XLSX re-import](evidence/postgres-enum-scalar-calc-reimport-results-2026-10-04/manifest.json), [Quoted enum identifiers](evidence/postgres-enum-quoted-identifiers-results-2026-10-04/manifest.json), [Embedded-quote identifier escaping](evidence/postgres-enum-identifier-escaping-results-2026-10-04/manifest.json), [Shadowed quoted enum search_path](evidence/postgres-enum-shadowed-quoted-search-path-results-2026-10-04/manifest.json), [Current enum result/write/filter/import/JSON/parameter/MCP/file-writer evidence](evidence/postgres-enum-results-2026-10-03/manifest.json), [deep-domain 129/256-layer follow-up](evidence/postgres-deep-domain-followup-results-2026-10-04/manifest.json), [enum NULLIF parameter inference](evidence/postgres-enum-nullif-parameter-results-2026-10-04/manifest.json), [domain-over-enum NULLIF boundary](evidence/postgres-domain-nullif-parameter-results-2026-10-04/manifest.json), [all-operator enum/domain filter matrix](evidence/postgres-enum-filter-matrix-results-2026-10-03/manifest.json), [domain query comparison ordering parameters](evidence/postgres-domain-enum-param-operator-results-2026-10-03/manifest.json), [domain query comparison list parameters](evidence/postgres-domain-enum-param-list-results-2026-10-03/manifest.json), [domain enum NULL-safe distinctness parameters](evidence/postgres-domain-enum-distinct-parameter-results-2026-10-03/manifest.json), [shadowed domain-over-enum metadata and parameters](evidence/postgres-shadowed-domain-enum-results-2026-10-03/manifest.json), [shadowed domain-over-enum direct operator matrix](evidence/postgres-shadowed-domain-enum-operator-results-2026-10-03/manifest.json), [shadowed enum direct query parameter](evidence/postgres-shadowed-enum-parameter-results-2026-10-03/manifest.json), [shadowed enum equality/list/range filters](evidence/postgres-shadowed-enum-filter-matrix-results-2026-10-03/manifest.json), [shadowed enum equality filter](evidence/postgres-shadowed-enum-filter-results-2026-10-03/manifest.json), [domain-enum filter operators](evidence/postgres-domain-enum-filter-results-2026-10-03/manifest.json), [domain-enum query operators](evidence/postgres-domain-enum-query-operators-results-2026-10-03/manifest.json), [three-level nested-domain evidence](evidence/postgres-three-level-domain-results-2026-10-03/manifest.json), [three-level query parameter operators](evidence/postgres-three-level-enum-query-operator-results-2026-10-03/manifest.json), [four-domain-layer consumer evidence](evidence/postgres-four-level-domain-results-2026-10-04/manifest.json), [five-domain-layer consumer evidence](evidence/postgres-five-level-domain-results-2026-10-04/manifest.json), [six-domain-layer shadowed-search-path draft/write evidence](evidence/postgres-six-level-domain-results-2026-10-04/manifest.json), [nested domain-chain evidence](evidence/postgres-nested-domain-results-2026-10-03/manifest.json), [MCP CSV null-marker import](value-contract-history.md#mcp-csv-null-markers-preserve-postgresql-enum-values-october-3), [enum parameter inference history](value-contract-history.md#postgresql-custom-enum-parameter-type-inference-october-3), [enum JSON rendering history](value-contract-history.md#postgresql-custom-enum-json-rendering-october-3), [enum CSV import history](value-contract-history.md#postgresql-custom-enum-csv-import-october-3), [historical enum result follow-up](value-contract-history.md#postgresql-direct-custom-enum-result-follow-up-october-3), [NUMERIC](value-contract-history.md#postgresql-numeric-decoding-checkpoint), [extended temporal consumer support](value-contract-history.md#postgresql-extended-temporal-consumer-support-october-3), [BC and signed-year CSV import](value-contract-history.md#postgresql-bc-and-extended-year-csv-import-october-3), [arrays](value-contract-history.md#postgresql-array-checkpoint), [interval/temporal arrays and infinities](value-contract-history.md#postgresql-interval-fields-temporal-arrays-and-infinities), [native type census](value-contract-history.md#postgresql-built-in-array-oid-census-2026-10-01) |
| MySQL/MariaDB | [Wide DECIMAL](value-contract-history.md#mysql-wide-decimal-csv-import-2026-10-01), [permissive TIME/zero dates](value-contract-history.md#mysql-native-time-zero-date-and-year-checkpoint), [strict zero-date modes](evidence/mysql-strict-zero-date-results-2026-10-04/manifest.json), [ENUM/SET SQL/CSV/JSON/XML/HTML/Markdown twelve-mode and XLSX-refusal matrix](evidence/mysql-enum-sql-mode-results-2026-10-04/manifest.json), [ENUM/SET keyed grid edits across twelve modes, empty values, literal `NULL` label and invalid-value refusal](evidence/mysql-enum-set-grid-edit-results-2026-10-04/manifest.json), [fractional TIME mode semantics](evidence/mysql-fractional-time-mode-results-2026-10-04/manifest.json), [TIME exact-half boundary](evidence/mysql-fractional-time-tie-results-2026-10-04/manifest.json), [TIME(0-6) precision matrix](evidence/mysql-time-precision-matrix-results-2026-10-04/manifest.json), [DATETIME/TIMESTAMP exact-half boundaries](evidence/mysql-fractional-half-boundary-matrix-results-2026-10-04/manifest.json), [all temporal precision matrix](evidence/mysql-datetime-timestamp-precision-matrix-results-2026-10-04/manifest.json), [app temporal grid edit and CSV/JSON/XLSX round trip](evidence/mysql-temporal-grid-edit-results-2026-10-04/manifest.json), [fractional DATETIME mode semantics](evidence/mysql-fractional-datetime-mode-results-2026-10-04/manifest.json), [fractional TIMESTAMP mode semantics](evidence/mysql-fractional-timestamp-mode-results-2026-10-04/manifest.json), [unsigned edits](value-contract-history.md#mysql-signed-and-unsigned-integer-grid-parser-2026-09-30), [spatial refusal](value-contract-history.md#mysql-spatial-grid-edit-refusal-2026-09-30) |
| SQLite | [STRICT ANY grid edits](evidence/sqlite-strict-any-results-2026-10-03/manifest.json), [STRICT ANY table CSV storage-class round trip](evidence/sqlite-strict-any-csv-results-2026-10-03/manifest.json), [STRICT ANY query-result CSV round trip](evidence/sqlite-strict-any-query-csv-results-2026-10-03/manifest.json), [empty query metadata](evidence/sqlite-query-empty-metadata-results-2026-10-03/manifest.json), [attached ANY origins](evidence/sqlite-attached-any-metadata-results-2026-10-03/manifest.json), [computed ANY-expression results](evidence/sqlite-computed-any-results-2026-10-04/manifest.json), [computed BLOB CAST JSON/XLSX/CSV](evidence/sqlite-cast-blob-any-csv-results-2026-10-04/manifest.json), [NULLIF expression CSV round trip](evidence/sqlite-nullif-any-results-2026-10-04/manifest.json), [COALESCE expression results](evidence/sqlite-coalesce-any-results-2026-10-04/manifest.json), [compound UNION result metadata](evidence/sqlite-union-any-results-2026-10-04/manifest.json), [compound result CSV/XLSX output](evidence/sqlite-union-any-export-results-2026-10-04/manifest.json), [compound result typed CSV round trip](evidence/sqlite-union-any-csv-roundtrip-results-2026-10-04/manifest.json), [GROUP_CONCAT result CSV round trip](evidence/sqlite-group-concat-any-results-2026-10-04/manifest.json), [MIN result CSV round trip](evidence/sqlite-min-any-results-2026-10-04/manifest.json), [MAX result CSV round trip](evidence/sqlite-max-any-results-2026-10-04/manifest.json), [SUM result CSV round trip and overflow](evidence/sqlite-sum-any-results-2026-10-04/manifest.json), [TOTAL result CSV round trip](evidence/sqlite-total-any-results-2026-10-04/manifest.json), [AVG result CSV round trip](evidence/sqlite-avg-any-results-2026-10-04/manifest.json), [arithmetic expression CSV round trip](evidence/sqlite-arithmetic-any-results-2026-10-04/manifest.json), [JSON extraction consumers](evidence/sqlite-json-extract-any-results-2026-10-04/manifest.json), [REAL float boundaries](evidence/sqlite-real-results-2026-10-03/manifest.json), [float boundary history](value-contract-history.md#sqlite-real-float-edge-checkpoint-2026-10-03), [storage classes](value-contract-history.md#sqlite-dynamic-storage-class-checkpoint), [CSV binary regression](value-contract-history.md#csv-regression-audit-2026-09-29) |
| SQL Server | [Offset CSV](value-contract-history.md#sql-server-datetimeoffset-csv-import-2026-10-01), [calendar edges](value-contract-history.md#sql-server-date-time-and-datetime2-calendar-edges-2026-09-30), [stream drain](value-contract-history.md#sql-server-multi-result-draining-2026-09-28), [U1 metadata](upstream-u1-sql-server-2026-10-03.md) |
| ClickHouse | [Wide integers](value-contract-history.md#clickhouse-wide-integer-parser-contract-2026-09-28), [nested values](value-contract-history.md#clickhouse-nested-value-consumer-boundary-2026-09-30), [nested tuple/numeric-map follow-up](evidence/clickhouse-nested-tuple-map-results-2026-10-04/manifest.json), [malformed heredoc consumers](evidence/clickhouse-malformed-heredoc-results-2026-10-04/manifest.json), [named timezones](value-contract-history.md#clickhouse-named-temporal-timezones-2026-09-27) |
| Redis | [RESP3 kinds/binary](value-contract-history.md#redis-resp3-nested-values-and-binary-replies), [disconnection](value-contract-history.md#redis-disconnect-classification-2026-09-30) |
| MongoDB | [Native BSON](value-contract-history.md#mongodb-nested-bson-and-native-boundary-checkpoint), [Int32 width](value-contract-history.md#mongodb-int32-grid-edits-preserve-bson-width-2026-10-01), [collection-wide mixed metadata](value-contract-history.md#mongodb-collection-wide-heterogeneity-blocks-grid-editing-october-3), [browse cursor consistency](value-contract-history.md#mongodb-browse-metadata-and-cursor-consistency-2026-10-03), [missing field versus BSON null](value-contract-history.md#mongodb-missing-fields-remain-distinct-from-bson-null-october-3), [case manifest](evidence/mongodb-missing-null-results-2026-10-03/manifest.json), [current census/run_find regression group](evidence/mongodb-current-census-results-2026-10-04/manifest.json) |
| DuckDB | [Native temporal/enum](value-contract-history.md#duckdb-native-temporal-and-enum-checkpoint), [enum keyed edit](evidence/duckdb-enum-keyed-results-2026-10-03/manifest.json), [literal `NULL` enum grid edit](evidence/duckdb-enum-null-label-results-2026-10-04/manifest.json), [enum CSV round trip](evidence/duckdb-enum-csv-roundtrip-results-2026-10-04/manifest.json), [extended calendar and TIMESTAMPTZ](value-contract-history.md#duckdb-extended-calendar-and-timestamptz-consumer-support-october-3), [interval import](value-contract-history.md#duckdb-interval-csv-import-2026-10-01), [sub-microsecond binding](value-contract-history.md#duckdb-sub-microsecond-parameter-expression-boundary-2026-10-01), [nested wide values](value-contract-history.md#duckdb-nested-uhugeint-refusal-boundaries-2026-09-30) |
| CSV/JSON and typed import | [CSV conditions](value-contract-history.md#csv-export-quoting-and-decimal-comma-contracts-2026-09-30), [JSON shape](value-contract-history.md#single-row-json-export-2026-09-30), [typed arrays](value-contract-history.md#postgresql-built-in-array-csv-insert-matrix-2026-10-01), [grid clipboard CSV NULL/enum formatting](evidence/postgres-enum-clipboard-csv-results-2026-10-04/manifest.json), [finite-float CSV/JSON consumer corpus](evidence/finite-float-consumer-results-2026-10-04/manifest.json) |
| XLSX/XML/other formats | [Workbook precision](value-contract-history.md#xlsx-float-precision-and-excel-safe-cell-types-2026-10-02), [temporal workbook](value-contract-history.md#xlsx-temporal-consumer-checkpoint-2026-09-27), [XML text](value-contract-history.md#xml-text-consumer-checkpoint-2026-09-27) |
| Mutation and infrastructure | [Scoped re-audit](value-contract-history.md#b3-mutation-survivor-re-audit-2026-10-01), [regression audit](regression-audit-2026-09-29.md), [B3 review](b3-review-2026-10-01.md) |
| Local integration gate | [Six-driver, MCP, session, socket and SSH run (315 passed)](evidence/local-integration-tier-results-2026-10-04/manifest.json) |
| Script planning and approval | [MySQL delimiter consumer agreement](value-contract-history.md#mysql-delimiter-consumer-agreement-2026-09-28), [word-character delimiter boundary](evidence/mysql-word-delimiter-results-2026-10-04/manifest.json), [quoted delimiter consumer agreement](evidence/mysql-quoted-delimiter-results-2026-10-04/manifest.json), [short `\d` delimiter consumer](evidence/mysql-short-delimiter-results-2026-10-04/manifest.json), [malformed-routine fail-closed contract](value-contract-history.md#mysql-malformed-routine-delimiter-fails-closed-2026-09-30), [human policy decision](evidence/mysql-malformed-human-approval-results-2026-10-04/manifest.json), [GTK approval route](evidence/gtk-unparseable-approval-results-2026-10-04/manifest.json), [MySQL-backed GTK approval and dispatch](evidence/mysql-gtk-unparseable-approval-results-2026-10-04/manifest.json) |
| Transport/partial delivery | [Disconnection contracts](disconnection-contracts.md); coordinate type/result delivery with B4 ownership |

PostgreSQL domain-over-enum arrays now also have a bound text-parameter contract
for literal `NULL`, empty text, Unicode, comma-containing labels and SQL NULL,
verified against native `array_send` bytes ([parameter evidence](evidence/postgres-domain-enum-array-parameter-results-2026-10-04/manifest.json)); a separate 2×2 parameter case preserves lower bounds 0 and 3 ([bounds evidence](evidence/postgres-domain-enum-array-bounds-results-2026-10-04/manifest.json)).
CSV import now preserves the domain element type and enforces its CHECK constraints ([import evidence](evidence/postgres-domain-enum-array-import-results-2026-10-04/manifest.json)).
It also resolves the target schema when `search_path` contains a same-named shadow domain ([shadowed-path import evidence](evidence/postgres-domain-enum-array-shadowed-import-results-2026-10-04/manifest.json)).
The app parser and keyed grid edit use the same qualified domain-array cast; invalid values roll back without changing the row ([grid evidence](evidence/postgres-domain-enum-array-grid-results-2026-10-04/manifest.json)).
Structured equality filters use the qualified domain-array type and match native array values ([filter evidence](evidence/postgres-domain-enum-array-filter-results-2026-10-04/manifest.json)).
JSON, CSV, XML, HTML, Markdown and XLSX exports preserve domain-over-enum array text, and SQL replay restores the native type, JSON values and array wire bytes ([file-writer evidence](evidence/postgres-domain-enum-array-filewriter-results-2026-10-04/manifest.json)).
A UUID-domain array now decodes using its base OID and preserves NULL versus empty arrays, bound round trips, keyed edits, domain CHECK refusal and sibling rows ([UUID-domain array evidence](evidence/postgres-domain-uuid-array-results-2026-10-04/manifest.json)).
Domain arrays over text, numeric and timestamptz also preserve NULL elements through qualified binding under an Asia/Kathmandu session, checked against native JSON and wire bytes ([base-type matrix evidence](evidence/postgres-domain-array-family-results-2026-10-04/manifest.json)).

The PostgreSQL shadowed-search-path enum/domain write case now covers six
through ten levels, plus 63, 64, 65, 128, 129, 256, 257 and 258 levels. Raw inferred text and SQL
NULL parameters work through 63 levels; at 64 or more levels they return
explicit unsupported results, including after schema-aware work warms the
same-backend transaction. Schema-aware writes and filters pass through 258
levels ([258-level evidence](evidence/postgres-domain-258-level-results-2026-10-04/manifest.json),
[257-level evidence](evidence/postgres-domain-257-level-results-2026-10-04/manifest.json)); see the [deep-domain
boundary evidence](evidence/postgres-deep-domain-results-2026-10-04/manifest.json),
[129/256-level follow-up](evidence/postgres-deep-domain-followup-results-2026-10-04/manifest.json),
[six/seven-level evidence](evidence/postgres-seven-domain-results-2026-10-04/manifest.json),
[eight-level evidence](evidence/postgres-eight-domain-results-2026-10-04/manifest.json)
and [nine-level evidence](evidence/postgres-nine-domain-results-2026-10-04/manifest.json).

A PostgreSQL 16 transaction also changes ordinary session `search_path` twice
on one backend, then verifies target-schema enum metadata and keyed-write safety
under the final shadowed path ([session evidence](evidence/postgres-enum-session-search-path-results-2026-10-04/manifest.json)).

Quoted schema/type identifiers containing spaces and embedded quotes now have
metadata, keyed-edit, draft-insert and filter coverage ([evidence](evidence/postgres-quoted-enum-identifiers-results-2026-10-04/manifest.json)).

PostgreSQL custom-enum scalar and array labels at 63 bytes are verified for
ASCII and three-byte UTF-8 text, and a 64-byte label is refused without leaving
a type behind ([accepted boundary](evidence/postgres-enum-label-byte-boundary-results-2026-10-04/manifest.json), [refusal evidence](evidence/postgres-enum-overlength-refusal-results-2026-10-04/manifest.json)).
Native enum ordering also follows declaration order rather than lexical text sorting ([ordering evidence](evidence/postgres-enum-order-results-2026-10-04/manifest.json)).

Raw PostgreSQL enum CSV export, parsing and schema-aware typed import also
preserve labels containing double quotes, embedded line breaks and backslashes,
along with formula-shaped text and SQL NULL across all four supported delimiters
and LF/CRLF/CR record endings. Import format auto-detection identifies every
combination; restored UTF-8 bytes and native enum type are checked independently
([evidence](evidence/postgres-enum-csv-quoted-lines-results-2026-10-04/manifest.json)).

Separate PostgreSQL expression contexts now infer text and SQL NULL enum
parameters in `COALESCE` and `array_append` ([evidence](evidence/postgres-enum-expression-parameter-results-2026-10-04/manifest.json)). A `NULLIF(enum_column, $1)` follow-up checks inferred type, NULL/match behavior and invalid-label refusal ([evidence](evidence/postgres-enum-nullif-parameter-results-2026-10-04/manifest.json)). A domain-over-enum case records the raw `42883` refusal and passing qualified base-enum cast control under a shadowed `search_path` ([evidence](evidence/postgres-domain-nullif-parameter-results-2026-10-04/manifest.json)).

PostgreSQL custom-enum SQL export now preserves a label containing literal `\n`
when replayed with (`standard_conforming_strings=on`, `backslash_quote=safe_encoding`)
and (`standard_conforming_strings=off`, `backslash_quote=off`); the writer uses
an explicit escape string and native type/text checks verify both results
([evidence](evidence/postgres-enum-sql-literal-session-modes-results-2026-10-04/manifest.json)).

One PostgreSQL `text[]` case now checks XML, HTML, Markdown and XLSX output plus
replayed SQL against native array text, JSON elements and wire bytes; see the
[array file-writer evidence](evidence/postgres-array-filewriter-results-2026-10-04/manifest.json).

A `numeric[]` file-writer contract checks high precision, scale, NaN, infinities
and SQL NULL through JSON, CSV, XLSX and replayed SQL. Native JSON and wire
oracles confirm that the driver's quoted element text remains bindable ([evidence](evidence/postgres-numeric-array-filewriter-results-2026-10-04/manifest.json)).

A custom enum-array file-writer contract preserves empty text, literal `NULL`,
Unicode, comma, quote, markup and SQL NULL through JSON, CSV, XLSX and replayed
SQL. Native `array_to_json` and `array_send` checks verify bound and restored
values ([evidence](evidence/postgres-enum-array-filewriter-results-2026-10-04/manifest.json)).
Custom enum-array CSV import now uses a schema-qualified array cast and verifies
native type, values and sibling-row bytes ([import evidence](evidence/postgres-enum-array-csv-import-results-2026-10-04/manifest.json)).
An explicit null marker also preserves NULL arrays, empty arrays, SQL NULL
elements, lower bounds and two-dimensional shape ([shape evidence](evidence/postgres-enum-array-csv-shapes-results-2026-10-04/manifest.json)).
The target enum remains correct when a same-named type shadows it in the active
`search_path` ([shadowed-path evidence](evidence/postgres-shadowed-enum-array-import-results-2026-10-04/manifest.json)).
The app parser and live keyed edit now preserve custom enum-array labels and
refuse invalid labels without mutation ([grid-edit evidence](evidence/postgres-enum-array-grid-edit-results-2026-10-04/manifest.json)).
Grid input also keeps blank SQL NULL distinct from the empty array literal
`{}` ([evidence](evidence/postgres-enum-array-grid-null-results-2026-10-04/manifest.json)).
Default CSV import also maps a blank whole-array field to SQL NULL while
retaining `{}` as the empty array ([evidence](evidence/postgres-enum-array-default-null-results-2026-10-04/manifest.json)).

A `timestamptz[]` file-writer contract preserves distinct repeated-hour
instants, a BC instant, infinities and SQL NULL through JSON, CSV, XLSX and
replayed SQL, checked against native JSON and wire bytes ([evidence](evidence/postgres-timestamptz-array-filewriter-results-2026-10-04/manifest.json)).

An `interval[]` file-writer case runs under `postgres_verbose` `IntervalStyle`
on one transaction backend and verifies text binding, all four writers and
replayed SQL against PostgreSQL JSON/wire oracles ([evidence](evidence/postgres-interval-array-filewriter-results-2026-10-04/manifest.json)).

The SQLite STRICT `ANY` computed `CASE` and `COALESCE` paths now have app CSV
export/import coverage with native storage-class assertions; see the [CASE
consumer evidence](evidence/sqlite-case-any-csv-roundtrip-results-2026-10-04/manifest.json)
and [COALESCE consumer evidence](evidence/sqlite-coalesce-any-csv-roundtrip-results-2026-10-04/manifest.json).
The CASE workbook path now refuses empty TEXT without replacing the existing
destination ([XLSX refusal evidence](evidence/sqlite-case-any-xlsx-refusal-results-2026-10-04/manifest.json)).
Computed COALESCE JSON output checks number/string distinctions, and XLSX output
checks numeric/shared-string cell kinds, including BLOB text encoding
([consumer evidence](evidence/sqlite-coalesce-any-xlsx-results-2026-10-04/manifest.json)).

SQLite grouped `MAX()` over STRICT `ANY` now round-trips INTEGER, REAL, TEXT,
BLOB and NULL groups through typed CSV, checked by native `typeof()` and exact
BLOB-byte comparisons ([evidence](evidence/sqlite-max-any-results-2026-10-04/manifest.json)).

SQLite grouped `MIN()` covers the same storage classes plus SQLite's native numeric-before-text ordering in a mixed group. Typed CSV re-import
preserves exact values, `typeof()` results, BLOB bytes and marker-shaped text
([evidence](evidence/sqlite-min-any-results-2026-10-04/manifest.json)).

SQLite ordered `group_concat()` over STRICT `ANY` verifies native numeric-to-text
conversion, NULL skipping, empty-text separators, all-NULL output and marker-shaped
text. Typed CSV re-import preserves the exact aggregate text and SQL NULL
([evidence](evidence/sqlite-group-concat-any-results-2026-10-04/manifest.json)).

SQLite grouped `SUM()` over STRICT `ANY` preserves SQLite's dynamic INTEGER,
REAL and NULL results through typed CSV re-import. Numeric text coercion,
nonnumeric text/BLOB coercion to REAL zero, and integer overflow refusal are
covered with native `typeof()` checks ([evidence](evidence/sqlite-sum-any-results-2026-10-04/manifest.json)).

SQLite `total()` over STRICT `ANY` always returns REAL, including all-NULL and
empty input, and accepts an integer sum above `i64::MAX` without SUM's integer
overflow. Typed CSV re-import preserves every f64 result and native REAL class
([evidence](evidence/sqlite-total-any-results-2026-10-04/manifest.json)).

SQLite grouped `AVG()` over STRICT `ANY` returns REAL for non-NULL groups,
including near-`i64::MAX` values, while all-NULL groups remain SQL NULL. Typed
CSV re-import preserves computed values and runtime classes ([evidence](evidence/sqlite-avg-any-results-2026-10-04/manifest.json)).

SQLite arithmetic expressions over STRICT `ANY` verify numeric coercion,
integer division, overflow promotion to REAL, divide-by-zero NULL and typed CSV
restoration ([evidence](evidence/sqlite-arithmetic-any-results-2026-10-04/manifest.json)).

SQLite `json_extract()` over STRICT `ANY` preserves dynamic numeric/text/NULL
storage classes through JSON and XLSX export and typed CSV re-import. JSON keeps
large integers numeric and distinguishes JSON null from a missing path through
the companion kind column; XLSX uses numeric cells for numeric results and
shared strings for extracted text without formulas. Native `typeof()` and
`json_type()` remain the independent SQLite oracles ([evidence](evidence/sqlite-json-extract-any-results-2026-10-04/manifest.json)).

## Evidence updates

Add or update one named case in the ledger with the ADR 0007 outcome, native
type/configuration, independent oracle, selector, initial failure, source SHA/
fingerprint and actual result. Use the [bounded record](type-contract-strategy.md#bounded-case-record).
Link it from its task; do not copy the same result/count into the sprint and
strategy. Checked-in test source is not execution evidence.

Some historical `target/quality` paths are unavailable in this checkout; the
[consistency review](architecture-consistency-review-2026-10-03.md#evidence-audit)
records the known gaps. Retained hashes/summaries cannot recover missing raw logs.
Retrieve exact-SHA artifacts or rerun the owning gate before using unavailable
proof for acceptance. Native fixtures, hosted CI, installed GTK, package rollback
and Wayland/soak remain separately attributed.
