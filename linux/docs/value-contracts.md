# Type and value evidence index

The shared standard is [ADR 0007](decisions/0007-type-and-value-preservation.md).
[The B3 board](type-contract-strategy.md) owns remaining tasks. This page is a
bounded entry into case evidence, not another conversion policy or aggregate
support claim. Updated 2026-10-03; this index links evidence but does not run tests.

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
Reports/logs go to `target/quality/*-values/`. See [testing](testing.md) and
[validation](validation-playbook.md) for affected layers and evidence requirements.

## Case lookup

Detailed native oracles, exact selectors and dated results are preserved in
[value-contract history](value-contract-history.md). Use these starting points,
then search that ledger/test for the specific type and consumer; one starting
point does not represent all support for that engine.

| Scope | Evidence entry points |
| --- | --- |
| PostgreSQL | [Current enum result/write/filter/import/JSON/parameter/MCP/file-writer evidence](evidence/postgres-enum-results-2026-10-03/manifest.json), [three-level nested-domain evidence](evidence/postgres-three-level-domain-results-2026-10-03/manifest.json), [nested domain-chain evidence](evidence/postgres-nested-domain-results-2026-10-03/manifest.json), [MCP CSV null-marker import](value-contract-history.md#mcp-csv-null-markers-preserve-postgresql-enum-values-october-3), [enum parameter inference history](value-contract-history.md#postgresql-custom-enum-parameter-type-inference-october-3), [enum JSON rendering history](value-contract-history.md#postgresql-custom-enum-json-rendering-october-3), [enum CSV import history](value-contract-history.md#postgresql-custom-enum-csv-import-october-3), [historical enum result follow-up](value-contract-history.md#postgresql-direct-custom-enum-result-follow-up-october-3), [NUMERIC](value-contract-history.md#postgresql-numeric-decoding-checkpoint), [extended temporal consumer support](value-contract-history.md#postgresql-extended-temporal-consumer-support-october-3), [BC and signed-year CSV import](value-contract-history.md#postgresql-bc-and-extended-year-csv-import-october-3), [arrays](value-contract-history.md#postgresql-array-checkpoint), [interval/temporal arrays](value-contract-history.md#postgresql-interval-fields-temporal-arrays-and-infinities), [native type census](value-contract-history.md#postgresql-built-in-array-oid-census-2026-10-01) |
| MySQL/MariaDB | [Wide DECIMAL](value-contract-history.md#mysql-wide-decimal-csv-import-2026-10-01), [TIME/zero dates](value-contract-history.md#mysql-native-time-zero-date-and-year-checkpoint), [unsigned edits](value-contract-history.md#mysql-signed-and-unsigned-integer-grid-parser-2026-09-30), [spatial refusal](value-contract-history.md#mysql-spatial-grid-edit-refusal-2026-09-30) |
| SQLite | [STRICT ANY grid edits](evidence/sqlite-strict-any-results-2026-10-03/manifest.json), [REAL float boundaries](evidence/sqlite-real-results-2026-10-03/manifest.json), [float boundary history](value-contract-history.md#sqlite-real-float-edge-checkpoint-2026-10-03), [storage classes](value-contract-history.md#sqlite-dynamic-storage-class-checkpoint), [CSV binary regression](value-contract-history.md#csv-regression-audit-2026-09-29) |
| SQL Server | [Offset CSV](value-contract-history.md#sql-server-datetimeoffset-csv-import-2026-10-01), [calendar edges](value-contract-history.md#sql-server-date-time-and-datetime2-calendar-edges-2026-09-30), [stream drain](value-contract-history.md#sql-server-multi-result-draining-2026-09-28), [U1 metadata](upstream-u1-sql-server-2026-10-03.md) |
| ClickHouse | [Wide integers](value-contract-history.md#clickhouse-wide-integer-parser-contract-2026-09-28), [nested values](value-contract-history.md#clickhouse-nested-value-consumer-boundary-2026-09-30), [named timezones](value-contract-history.md#clickhouse-named-temporal-timezones-2026-09-27) |
| Redis | [RESP3 kinds/binary](value-contract-history.md#redis-resp3-nested-values-and-binary-replies), [disconnection](value-contract-history.md#redis-disconnect-classification-2026-09-30) |
| MongoDB | [Native BSON](value-contract-history.md#mongodb-nested-bson-and-native-boundary-checkpoint), [Int32 width](value-contract-history.md#mongodb-int32-grid-edits-preserve-bson-width-2026-10-01), [collection-wide mixed metadata](value-contract-history.md#mongodb-collection-wide-heterogeneity-blocks-grid-editing-october-3), [browse cursor consistency](value-contract-history.md#mongodb-browse-metadata-and-cursor-consistency-2026-10-03), [missing field versus BSON null](value-contract-history.md#mongodb-missing-fields-remain-distinct-from-bson-null-october-3), [case manifest](evidence/mongodb-missing-null-results-2026-10-03/manifest.json) |
| DuckDB | [Native temporal/enum](value-contract-history.md#duckdb-native-temporal-and-enum-checkpoint), [extended calendar and TIMESTAMPTZ](value-contract-history.md#duckdb-extended-calendar-and-timestamptz-consumer-support-october-3), [interval import](value-contract-history.md#duckdb-interval-csv-import-2026-10-01), [sub-microsecond binding](value-contract-history.md#duckdb-sub-microsecond-parameter-expression-boundary-2026-10-01), [nested wide values](value-contract-history.md#duckdb-nested-uhugeint-refusal-boundaries-2026-09-30) |
| CSV/JSON and typed import | [CSV conditions](value-contract-history.md#csv-export-quoting-and-decimal-comma-contracts-2026-09-30), [JSON shape](value-contract-history.md#single-row-json-export-2026-09-30), [typed arrays](value-contract-history.md#postgresql-built-in-array-csv-insert-matrix-2026-10-01) |
| XLSX/XML/other formats | [Workbook precision](value-contract-history.md#xlsx-float-precision-and-excel-safe-cell-types-2026-10-02), [temporal workbook](value-contract-history.md#xlsx-temporal-consumer-checkpoint-2026-09-27), [XML text](value-contract-history.md#xml-text-consumer-checkpoint-2026-09-27) |
| Mutation and infrastructure | [Scoped re-audit](value-contract-history.md#b3-mutation-survivor-re-audit-2026-10-01), [regression audit](regression-audit-2026-09-29.md), [B3 review](b3-review-2026-10-01.md) |
| Transport/partial delivery | [Disconnection contracts](disconnection-contracts.md); coordinate type/result delivery with B4 ownership |

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
