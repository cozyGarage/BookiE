# B3 external test-scenario survey

### October 4 follow-up

PostgreSQL inferred enum-array binding is covered for domain-over-enum `ANY`,
containment and overlap operators, including either operand position for
containment and overlap. Native array type, operator results and wire bytes are
checked. `array_cat` covers text and SQL NULL array parameters against a domain-over-enum
source, including NULL/empty/special labels and a same-named shadow enum earlier
in `search_path`; `array_positions` covers inferred text/NULL scalar parameters,
repeated positions and NULL-element matching under that shadowed path
([array_cat evidence](evidence/postgres-domain-enum-array-cat-results-2026-10-04/manifest.json),
[array_positions evidence](evidence/postgres-domain-enum-array-positions-results-2026-10-04/manifest.json)); all six array comparison operators also check inferred domain-array typing, native parameter wire bytes, NULL arrays/elements, empty arrays and lower-bound-sensitive results under a same-named shadow enum `search_path` ([evidence](evidence/postgres-domain-enum-array-equality-results-2026-10-04/manifest.json)); other inferred array contexts remain open. See the [ANY packet](evidence/postgres-domain-enum-any-array-parameter-results-2026-10-04/manifest.json)
and [operator packet](evidence/postgres-domain-enum-array-operators-results-2026-10-04/manifest.json).
The operator matrix also runs with a same-named shadow enum first in
`search_path` ([configuration packet](evidence/postgres-shadowed-enum-array-operator-results-2026-10-04/manifest.json)).
The inferred array-element domain chain also covers 62/63 supported layers and
an explicit 64-layer refusal ([depth packet](evidence/postgres-inferred-enum-array-depth-results-2026-10-04/manifest.json)).
PostgreSQL 16 `trim_array($1, n)` is now recorded as an inference boundary:
without a typed array argument the server returns `42804`; a schema-qualified
array cast round-trips NULL, empty, NULL-element and lower-bound values with
native `pg_typeof` and wire-byte checks under a same-named shadow enum
([evidence](evidence/postgres-trim-array-enum-inference-boundary-results-2026-10-05/manifest.json)).
The scalar enum parameter matrix now also checks `CASE` result type inference
with `$1` in both result branches, distinguishing the literal `NULL` label,
the empty label and SQL NULL and asserting invalid-label refusal
([evidence](evidence/postgres-enum-case-parameter-inference-results-2026-10-05/manifest.json)).
`GREATEST` and `LEAST` now cover `$1` in both argument positions, with native
value and type comparisons for valid enum values and SQL NULL; invalid labels
retain SQLSTATE `22P02`
([evidence](evidence/postgres-enum-greatest-least-parameter-inference-results-2026-10-05/manifest.json)).
A domain-over-enum CASE follow-up checks both parameter branches with a
same-named shadow enum leading `search_path`; native type/value checks preserve
the target enum and outer domain and distinguish empty text, literal `NULL`,
and SQL NULL ([evidence](evidence/postgres-domain-case-shadowed-parameter-results-2026-10-05/manifest.json)).
Ordinary custom-enum `COALESCE`, `NULLIF`, `GREATEST` and `LEAST` parameter
contexts now also preserve the qualified target enum after warming type metadata
and switching `search_path` on the same backend, including empty/literal-`NULL`
labels, SQL NULL and invalid shadow-only labels
([evidence](evidence/postgres-shadowed-enum-parameter-context-results-2026-10-05/manifest.json)).
The same shadowed-path fixture compares both domain-over-enum `COALESCE`
parameter positions against explicit native target-enum casts, including
literal `NULL`, empty text, SQL NULL and invalid shadow-only labels
([evidence](evidence/postgres-domain-coalesce-shadowed-results-2026-10-05/manifest.json)).

MongoDB keyed grid edits now compare each edited field with its value from the
materialized result in the atomic update filter. A failing-first MongoDB 7 case
reproduces last-write-wins, then verifies conflict counts and preservation for
a concurrent string change, explicit BSON NULL becoming missing, and explicit
BSON NULL becoming an array containing NULL
([evidence](evidence/mongodb-stale-grid-edit-results-2026-10-04/manifest.json)).
MongoDB keyed row deletes now compare every materialized non-key field in the
same `delete_one` filter. The MongoDB 7 case confirms unchanged rows delete,
while a concurrent string change, an untouched sibling-field change, explicit
NULL becoming missing, and explicit NULL becoming an array containing NULL all
conflict and survive
([evidence](evidence/mongodb-stale-grid-delete-results-2026-10-04/manifest.json)).
Neither write nor delete guards detect ABA changes; the broader B3 matrix stays
open.

External sources below were first reviewed 2026-09-26 against BookiE `2eb9414c2`.
The September 29 B3-P1 reconciliation used source
`35d457fa488768a5204a26d785c90114073d4acd` and documentation tip `62dc257c7`.
Subsequent case-level updates through September 30 are recorded below and in
the linked value-contract evidence. That source baseline contains Redis RESP3
nested-value/binary and attribute-wire contracts, explicit
PostgreSQL multirange metadata refusal coverage, ClickHouse DateTime64(9)
server-boundary evidence and safe SQL Server `sql_variant` refusal. The upstream
review sampled eight test files in four projects and two issue reports. The
strict selected-contract report passed 133 tests across 11 suites at the source
tip, with no missing suites; this is not a fresh execution of every crate test
or installed-app workflow. Evidence:
`target/quality/20260929T215946474345Z-values/report.json` (`dirty: false`; raw report unavailable in this checkout).
No external source code or fixtures were copied.

### B3-P6 mutation follow-up — September 29

The MySQL spatial GTK mutation report had five missed, stricter-read-only
mutations because that isolated test only asserted spatial refusal. An ordinary
VARCHAR positive-control contract now proves regular text columns remain
editable. Running the full app library suite against the editability guard caught
all 11 generated mutants, with no misses, timeouts or unviable changes. Evidence:
`target/quality/20260929-grid-editability-positive-control-mutants-final/mutants.out/outcomes.json` (raw report unavailable in this checkout).

The MongoDB Decimal128 edit audit also found Rust `Decimal` rejected valid
34-digit inputs. The app now validates the fallback with BSON Decimal128 and
passes canonical type markers through the keyed edit path. A MongoDB 7 contract
confirms the row identity and exact native BSON value; invalid over-precision
input is refused. The clean strict report at `35d457fa` includes both app-level
parser and Docker edit contracts.

## Focus to carry forward

- Finish B3 lossless values and consumer contracts before B4–B6 acceptance.
- Use upstream tests, issues and fix commits as bug hypotheses. Reproduce locally
  before changing production behavior; keep the reproducer as a permanent regression.
- Current coverage includes eight-driver scalar contracts, PostgreSQL exact
  BIT/VARBIT and MACADDR/MACADDR8 text, IPv6 inet/cidr and maximum pg_lsn
  consumer parity, scalar and temporal arrays,
  DuckDB scalar HUGEINT/UHUGEINT boundaries and enum consumer parity, temporal eras/infinities/interval fields, MongoDB nested BSON
  consumers and SQLite NUMERIC-affinity transitions. B3 remains open for uncovered
  type/consumer combinations and installed grid acceptance; a deterministic 86,012-entry finite-float CSV/JSON corpus is now retained, while exhaustive finite-`f64` enumeration remains open. See the current status
  table and [value-contract evidence](value-contracts.md).
- Docker/local tests support B3. The GNOME VM supports later installed desktop
  acceptance and does not block this work.

## Inspected sources

- [dbeaver/dbeaver / PostgreValueParserTest.java](https://github.com/dbeaver/dbeaver/blob/50180e664c8b917e6a8865b43018a2f479806e5a/test/org.jkiss.dbeaver.ext.postgresql.test/src/org/jkiss/dbeaver/ext/postgresql/PostgreValueParserTest.java)
- [dbeaver/dbeaver / DataExporterCSVTest.java](https://github.com/dbeaver/dbeaver/blob/50180e664c8b917e6a8865b43018a2f479806e5a/test/org.jkiss.dbeaver.test.platform/src/org/jkiss/dbeaver/tools/transfer/DataExporterCSVTest.java)
- [beekeeper-studio/beekeeper-studio / all.js](https://github.com/beekeeper-studio/beekeeper-studio/blob/e90c46f4306475d58b0dadfdd441b865f2b5f1a1/apps/studio/tests/integration/lib/db/clients/all.js)
- [beekeeper-studio/beekeeper-studio / jsonb.spec.ts](https://github.com/beekeeper-studio/beekeeper-studio/blob/e90c46f4306475d58b0dadfdd441b865f2b5f1a1/apps/studio/tests/integration/lib/db/clients/postgres/jsonb.spec.ts)
- [dbcli/pgcli / test_pgexecute.py](https://github.com/dbcli/pgcli/blob/101e523eb2987ada87231c4533f0ab701c4c3124/tests/test_pgexecute.py)
- [dbcli/pgcli / test_sqlformatter.py](https://github.com/dbcli/pgcli/blob/101e523eb2987ada87231c4533f0ab701c4c3124/tests/formatter/test_sqlformatter.py)
- [sqlfluff/sqlfluff / lexer_test.py](https://github.com/sqlfluff/sqlfluff/blob/c7401613e851a6913ec3de8e003247e6fe8e5387/test/core/parser/lexer_test.py)
- [sqlfluff/sqlfluff / corpus_test.py](https://github.com/sqlfluff/sqlfluff/blob/c7401613e851a6913ec3de8e003247e6fe8e5387/test/core/parser/parity/corpus_test.py)

Commit-pinned links above identify the reviewed snapshots. Source ideas are
adapted to our contracts; verbatim code or fixture reuse needs recorded source
license and retained attribution before import.

## What the assertions teach us

| Project | Observed scenarios and flow | Application to BookiE |
| --- | --- | --- |
| DBeaver | PostgreSQL scalar/composite/array conversion; nested dimensions; NULL versus literal NULL; empty arrays and escaped braces, quotes and whitespace. CSV parameterizes separators, quoting and row content. | Add nested-value and consumer-format matrices. Verify server re-import as well as displayed text. |
| Beekeeper Studio | Shared driver suite with setup per scenario; metadata, stream counts/chunks/cancellation, mutation rollback and concurrent transactions. Separate read-only checks. | Extend the shared contract harness with explicit per-engine capability expectations. Session and read-only cases feed B4/policy acceptance. |
| pgcli | Real PostgreSQL execution then rendered output; binary, Unicode enum/JSON values, BC dates, timetz, intervals, large numeric strings, and batch stop/resume after errors. | Test the entire decode-to-consumer path. Use exact typed/server comparisons instead of relying on output substrings. |
| SQLFluff | Dialect fixture corpus parsed through multiple implementations; parity checks; lexer Unicode/source-span cases. Known divergence records must be removed when they unexpectedly pass. | Reuse one dialect corpus across splitting, parameters, formatting and policy classification, with consumer-specific expectations. |

Two inspected pgcli issue reports are linked directly by its regressions:

- [#1362](https://github.com/dbcli/pgcli/issues/1362): leading comments sent a special command through normal SQL dispatch.
- [#1403](https://github.com/dbcli/pgcli/issues/1403): a leading comment caused a multiline statement to execute incompletely.

These establish useful failure patterns, not confirmed BookiE bugs. BookiE does
not need pgcli backslash-command support to benefit from comment/dispatch tests.

The sampled Beekeeper common suite has capability returns and a runSoft helper
that catches some non-SQLite failures. Our gates must record exclusions and
unsupported outcomes explicitly; an assertion failure must fail its suite.
SQLFluff parser agreement is a useful differential signal, but agreement alone
does not establish server semantics or satisfy our authorization rules.

## Prioritized candidate matrix

Inventory status below comes from current local code/tests, not new executions.

The PostgreSQL enum CSV case now distinguishes raw-text restore from
spreadsheet-safe output. Formula protection can make two distinct enum labels
collide after import; the export dialog directs users to turn it off for
lossless re-import. Further enum consumer and session combinations remain open.
A PostgreSQL scalar-enum CSV import case now also runs with a same-named shadow
enum first in `search_path`; it restores a target-only label, an empty label,
literal `NULL`, and SQL NULL with the qualified target type while preserving a
sibling row ([evidence](evidence/postgres-enum-csv-shadow-search-path-results-2026-10-05/manifest.json)).
The import plan also survives a role `search_path` change before execution on a
fresh connection, with another same-named decoy enum first; native target type,
all four value/null states, the sibling, and the untouched shadow table are
asserted ([evidence](evidence/postgres-enum-csv-search-path-transition-results-2026-10-05/manifest.json)).
CSV import into schema, table, and enum type identifiers with spaces and
embedded quotes also passes while a same-named shadow schema leads the role's
`search_path`; the generated cast, target type, sibling rows, and shadow row
are checked ([evidence](evidence/postgres-enum-csv-quoted-identifiers-results-2026-10-05/manifest.json)).
MariaDB 11's `EMPTY_STRING_IS_NULL` mode also has a native enum/set CSV restore
contract: an empty ENUM label and zero-member SET stay distinct from SQL NULL
by rebuilding empty text with a server expression; native ordinal/byte checks
verify the stored values ([evidence](evidence/mariadb-empty-enum-set-empty-string-is-null-results-2026-10-05/manifest.json)).
The same mode exposed an ordinary-text CSV gap: empty VARCHAR became SQL NULL
while the SQL-file copy passed. The importer now emits `SPACE(0)` for empty
CHAR/VARCHAR/TEXT destinations, and native CSV/SQL round trips compare text
bytes and NULL state across the MySQL and MariaDB mode matrices
([evidence](evidence/mariadb-empty-varchar-import-empty-string-mode-results-2026-10-05/manifest.json)).
The same import now also passes with `STRICT_TRANS_TABLES`, `ANSI_QUOTES`, and
`NO_BACKSLASH_ESCAPES` combined with `EMPTY_STRING_IS_NULL`; native ENUM/SET
ordinals, bytes, SQL NULL state, and the pre-existing sibling match the
standalone-mode restore ([evidence](evidence/mariadb-empty-enum-set-combined-mode-results-2026-10-05/manifest.json)).
The MariaDB keyed-grid contract now exercises `EMPTY_STRING_IS_NULL` alone and
combined with strict, ANSI_QUOTES, and NO_BACKSLASH_ESCAPES modes; it caught and
guards a loss where explicit empty ENUM/SET edits became SQL NULL. The shared
MySQL keyed-update builder now renders `SPACE(0)` for those typed empty edits,
with native MySQL/MariaDB checks confirming other rows and values are preserved
([evidence](evidence/mariadb-empty-enum-set-grid-empty-string-mode-results-2026-10-05/manifest.json)).
The SQL-file writer had the same empty-value failure for ENUM and SET values.
It now writes `SPACE(0)` for empty MySQL text literals; a core builder test also
pins that expression for ordinary text columns. Native SQL-file and typed CSV
round trips independently check ENUM ordinals, SET masks, bytes, NULL state,
and sibling preservation under `EMPTY_STRING_IS_NULL` alone and with strict,
ANSI, and backslash modes ([evidence](evidence/mariadb-empty-enum-set-sql-file-empty-string-mode-results-2026-10-05/manifest.json)).
A PostgreSQL 16 boundary test now preserves both a 63-byte ASCII label and a
21-character three-byte UTF-8 label through scalar and enum-array projections,
against `pg_enum`, JSON-element and wire oracles; an overlength 64-byte label is
refused with SQLSTATE 42602 and leaves no type behind. See the [accepted-boundary evidence](evidence/postgres-enum-label-byte-boundary-results-2026-10-04/manifest.json)
and [overlength-refusal evidence](evidence/postgres-enum-overlength-refusal-results-2026-10-04/manifest.json).
A separate PostgreSQL enum ordering case proves `ORDER BY` uses declared
`enumsortorder` (`zulu`, `alpha`, `middle`) rather than lexical label order and
retains a SQL NULL row; see the [enum-order evidence](evidence/postgres-enum-order-results-2026-10-04/manifest.json).

| Priority | Candidate | Existing evidence / gap | Next test and oracle |
| --- | --- | --- | --- |
| B3-1 | PostgreSQL arrays | Scalar and temporal arrays preserve elements, NULL, dimensions and lower bounds as exact text; SQL INSERT, typed text input, JSON output and server wire equality have real-server contracts. Both `json[]` and `jsonb[]` are explicitly refused as `Undecodable`, with native type, exact JSON text and semantic server oracles; SQL literal and parameter consumers refuse them too. Ordinary custom-enum scalar and array projections preserve labels, native type text and SQL NULL. Schema-aware keyed updates and draft inserts preserve ordinary labels and SQL NULL as the native enum, with sibling-row checks; a native shadow-schema case with search_path aimed at the shadow schema confirms same-named enum types resolve to the table declared schema. A literal label exactly `NULL` is preserved by keyed writes; direct enum scalar/array projections preserve the literal label `NULL` as text separately from SQL NULL; UPDATE RETURNING persists the label and fires its trigger exactly once. Raw enum CSV import also preserves double quotes, embedded line breaks, backslashes and formula-shaped labels across all four delimiters and three record endings, with automatic format detection and native UTF-8 byte checks ([evidence](evidence/postgres-enum-csv-quoted-lines-results-2026-10-04/manifest.json)). Structured enum filters cover equality/inequality, BETWEEN, IN and SQL NULL with schema-qualified casts and native type checks. CSV import applies the schema-qualified enum cast; with the default empty null marker it refuses blank enum cells as ambiguous, and a configured marker preserves empty label, literal `NULL` and SQL NULL as distinct values. MCP and core CSV file exports choose collision-free NULL markers; native PostgreSQL tests import raw-text output through the schema-aware importer and verify stored type/value. PostgreSQL shared JSON rendering preserves empty text, literal `NULL`, Unicode, and SQL NULL as distinct values. Safe built-in casts make integer[], boolean[], bytea[], uuid[], date[], time[], timestamp[], timetz[] and timestamptz[] grid write-back pass with NULL preserved. The text[] contract checks NULL and escaped quote/backslash/comma through ordered unnest and XML/HTML/Markdown/XLSX/SQL writers; `boolean[]` preserves true, false and SQL NULL through Calc XLSX/ODS/XLSX re-save; `int8[]` preserves a lower bound of zero and integers above spreadsheet exact precision; `smallint[]` and `integer[]` cover signed boundaries and lower bounds zero and two. A numeric[] file-writer contract preserves high precision, scale, NaN, infinities and NULL through JSON/CSV/XLSX text output and replayed SQL; native array_to_json and array_send agree after binding ([evidence](evidence/postgres-numeric-array-filewriter-results-2026-10-04/manifest.json)). The numeric[] grid contract checks wide precision, scale, NaN, ±Infinity and NULL with per-element `numeric::text` comparisons. The float8[] grid contract checks adjacent values, negative zero, the minimum subnormal, NaN, ±Infinity and NULL against `array_send` bytes. All eight built-in server grid contracts pass. The timestamptz[] case preserves offset-origin fractional instants and NULL against array_send while keeping the sibling wire value. UUID[] also caught a parser dispatch bug where the full array literal was treated as one UUID; PostgreSQL `[]` metadata now stays text before scalar parsing. | Enum-array shape introspection (`array_dims`, `array_ndims`, `array_length`, `array_lower`, `array_upper`, `cardinality`) and `array_to_string` now have native value and wire checks, including shadowed type resolution and NULL boundaries. Enum-array `array_fill` now covers polymorphic inference, empty/NULL labels and elements, zero-sized and multidimensional arrays, non-default bounds, and shadowed enum resolution with native type, JSON and wire comparisons. Multi-array enum `unnest` now checks heterogeneous enum/integer inputs, storage order, multidimensional flattening, shorter/NULL/empty-array NULL padding, output types and shadowed type resolution. Enum-array `array_sample` and `array_shuffle` now use multiset and result-shape oracles for repeated 1D sampling/shuffling and 2D first-dimension slices; these random functions do not assert a fixed order. Other PostgreSQL array families and file-writer combinations, spreadsheet-application re-import beyond tested text[], uuid[], date[], timestamp[], time[], timetz[], boolean[], int8[], smallint[], integer[], enum[], domain-over-enum [], bytea[], timestamptz[], interval[], numeric[] and float8[] shapes and additional enum configurations remain open. |
| B3-2 | Temporal boundaries | End-of-day time, timetz offsets, BC/extended-year SQL literals, infinities, mixed interval fields and temporal arrays have server-backed contracts. DuckDB's mixed month/day/microsecond interval has an exact VARCHAR oracle and is refused by non-lossless result consumers; sub-microsecond temporal parameters are verified as exact VARCHAR echoes, with explicit casts required for typed expressions. SQL Server `smalldatetime` values immediately below/at the 29.999-second rounding threshold match native text. Legacy `datetime` uses typed values for every-third ticks and canonical style-126 text for the other ticks; a native sweep of all 300 ticks verifies each carrier against server output and byte-for-byte parameter restore, while representative SQL-literal re-import also preserves original bytes. The app grid display/parser/keyed-update path round-trips representative text-fallback and exact ticks with native 8-byte and sibling-row checks. SQL Server `datetimeoffset` also preserves local text, fractional scale and offset through keyed edits, with native byte/sibling checks and refusal at precision, offset and UTC calendar bounds ([evidence](evidence/mssql-datetimeoffset-grid-edit-results-2026-10-04/manifest.json)). ClickHouse's ambiguous New York fall-back `DateTime64` returns exact local text and a valid fold epoch, while BookiE and its SQL-literal/parameter consumers refuse to guess the missing offset; a nonexistent spring-forward input is checked against ClickHouse's normalized wall time and exact epoch. PostgreSQL year-1,000,000 DATE and upper-bound year-294276 TIMESTAMP/TIMESTAMPTZ now round-trip as exact text through results, SQL literals, bound casts, CSV import, and app keyed edits, with independent server wire-byte oracles. The maximum finite DATE `5874897-12-31` has result/literal/bind wire-byte checks. `int4range`, `daterange`, `numrange`, `tsrange`, and `int8range` now have independent type, canonical text, endpoint, and consumer-refusal oracles; `tstzrange` also has a dedicated fractional-instant and UTC-bounds contract. | Other native rounding boundaries, additional SQL Server non-SQL consumers, broader temporal grid parity, and temporal-array consumer/session combinations and other spreadsheet applications. |
| B3-3 | Nested JSON/BSON | MongoDB nested documents/arrays and BSON-only kinds preserve special markers; Decimal128 extrema/date bounds and binary subtype tags have exact regressions, with server checks for UUID/user-defined binaries, large nested Int64, explicit null, Unicode, JSON/CSV/XLSX, Timestamp/regex/MinKey/MaxKey/JavaScriptCode/JavaScriptCodeWithScope/Symbol/Undefined/DbPointer grid edits, canonical Extended JSON re-import and MCP browse output. Native keyed edits verify exact subtype and bytes for Generic, Function, BinaryOld, UUIDOld, UUID, MD5, Encrypted, Sensitive, Vector, Reserved `0a`, UserDefined `80`, and valid Column subtype `07`; BSONColumn input comes from MongoDB-compressed time-series buckets, while arbitrary malformed Column bytes are rejected by the server. Unignored codec unit contracts round-trip special variants through canonical Extended JSON and grid BSON conversion, verify mixed String/Decimal128 distinction through JSON/CSV, and verify collection-wide mixed metadata before decoding. The Docker contract checks a first-page String and a later Decimal128 page/query, retaining the `$numberDecimal` marker. A native sparse-document contract checks explicit BSON null and an absent field separately; absent fields use the distinct undecodable marker rather than `Value::Null`. Homogeneous positive Decimal128 and formula-sanitized negative Decimal128 survive default CSV export, typed import parsing and keyed edit as native BSON Decimal128. Mixed BSON columns now decode to canonical Extended JSON values; a Docker-backed String/Decimal128 contract verifies kind-distinct JSON/CSV/XLSX output, retains native BSON kinds in storage, and the app gate refuses editing. A separate failpoint contract records that scans are best-effort rather than snapshot-isolated when a document changes after it was read. `run_find` re-merges returned-page types after its separate census and filtered query; CSV/JSON export preserves markers in the materialized result without a refresh. A diagnostic profile measures 1,000- and 10,000-document census time for a 50-row page. | Snapshot isolation and optimistic conflict detection are not provided; off-page `run_find` changes after census are not observable. Scan timings are environment-specific, and broader consumer/edit combinations remain open. Keep SQL NULL distinct from JSON null. Exact commands and evidence are in [value contracts](value-contracts.md). |
| B3-4 | Export/import consumers | SQL binary, SQLite NUMERIC-affinity storage-class re-import and policy-guarded CSV import, XLSX integers/decimals/temporal fallbacks, nested BSON markers, finite/nonfinite floats, XML text and CSV quoting have regressions; JSON export and parse preserve negative-zero bits and distinguish positive zero and SQL NULL. CSV typed import/export preserves `i64::MIN`, `i64::MAX` and `9007199254740993` exactly, as well as negative-zero, smallest-subnormal and largest-finite `f64` bits. An unignored deterministic corpus checks six explicit edge values, 65,536 raw finite bit samples and 20,470 sign/exponent/mantissa boundaries (86,012 entries total) through CSV export/import and JSON export/parse; exponent-all-ones patterns are excluded. See [finite-float evidence](evidence/finite-float-consumer-results-2026-10-04/manifest.json). JSON booleans remain distinct from text and SQL NULL. NaN/±Infinity remain distinct from NULL. A nine-digit offset-origin `TIMESTAMP WITH TIME ZONE` now has a direct CSV/JSON canonical UTC text parity assertion and CSV typed import verifies the same instant; XLSX emits that instant as exact canonical UTC text. XLSX keeps Excel-safe floats numeric and uses exact text for negative zero, subnormals, precision-risk floats and out-of-range magnitudes; workbook XML tests pin numeric boundaries. A LibreOffice Calc import/ODS save/XLSX re-save preserves numeric 42, text "42" and blank SQL NULL in one SQLite STRICT ANY workbook ([evidence](evidence/sqlite-xlsx-calc-reimport-results-2026-10-04/manifest.json)); broader workbook/application combinations remain unverified. PostgreSQL app-parser output reaches the keyed-update builder and live server for declared/unconstrained NUMERIC, NaN/±Infinity and the maximum 16,383-digit scale and 131,072-digit integer width; a custom numeric domain preserves a wide value through results, SQL literal and typed binding. Custom enum labels preserve direct scalar/array results and schema-aware writes, filters and CSV import. The literal label `NULL` remains distinct from SQL NULL; default blank CSV imports refuse ambiguity, while an explicit null marker preserves empty enum labels and SQL NULL. A MySQL app parser/keyed-edit contract rejects out-of-range TINYINT UNSIGNED input before permissive-mode clamping and verifies exact BIGINT UNSIGNED u64::MAX persistence plus neighbor-row identity. SQLite app parser/keyed-save contracts verify NUMERIC input to REAL storage and STRICT `ANY` edits: existing INTEGER/REAL/TEXT kinds persist, clearing existing TEXT stores empty TEXT, empty NULL/new cells stay NULL, nonempty NULL/new input (including numeric-looking text) stays TEXT through keyed update/draft insert, and BLOB values remain read-only with exact bytes verified. DuckDB grid parsing keeps blank as SQL NULL, accepts `''` for an empty enum label and decodes doubled single quotes; keyed edits preserve native ENUM type and siblings, and invalid labels are refused without mutation. Installed GTK interaction remains open. ClickHouse wide integers round-trip through SQL and CSV with both raw and default formula-safe export settings; sixteen nested Array/Map/Tuple results match native type/JSON oracles, are refused by type-less SQL/parameter consumers, and real MergeTree keyed-grid edits verify refusal leaves stored id/value/type/JSON unchanged; the typed importer strips the apostrophe only for valid Int128/UInt128 values within range. Grid edits cover Int128 min/max and UInt128 zero/max adjacent values. XLSX explicitly refuses empty text; full format equivalence is unproven. | Exhaustive finite-f64 enumeration, installed grid acceptance and broader temporal combinations. |
| B3-5 | Lexer/parser consumer agreement | A PostgreSQL CRLF script with issue-shaped leading, inter-statement, inline and trailing comments keeps its two executable statements in order across planner, named-parameter extraction, formatter and policy classification; SELECT/UPDATE classes, UPDATE target and WHERE survive formatting. The editor cursor contract maps GTK character offsets to byte offsets and selects the second statement after multibyte text. A malformed quoted tail reports a planner diagnostic, blocks whole-script execution, remains intact through parameter extraction and formatting, and classifies as unparseable/write. SQL Server GO batches retain three statements and batch policy through formatting, ignore placeholder-shaped delimiter comments, and classify SELECT/UPDATE/SELECT in order; `GO 2` keeps its count in the plan/formatter and is refused for execution, with only SQL placeholders extracted and full-script policy failing closed. A SQL Server script with a valid first `GO` batch and an unterminated quoted tail now blocks whole-script execution, preserves the malformed tail through formatting, extracts only the safe earlier parameter, and is classified unparseable/write with agent denial ([evidence](evidence/mssql-malformed-go-tail-results-2026-10-04/manifest.json)). MySQL `DELIMITER $$`, `DELIMITER //`, repeated-semicolon `DELIMITER ;;`, word-character `DELIMITER xyz` and quoted multi-character `DELIMITER '_finish'` directives survive planning and formatting; routine bodies retain internal semicolons, trailing query identity/order is preserved, and body placeholders do not escape into parameter extraction ([word-delimiter evidence](evidence/mysql-word-delimiter-results-2026-10-04/manifest.json), [quoted-delimiter evidence](evidence/mysql-quoted-delimiter-results-2026-10-04/manifest.json)). An unterminated quote in a MySQL routine produces a diagnostic, blocks whole-script planning, leaves malformed text intact through formatting, and is denied by fail-closed agent policy; policy tests request human approval by default and honor the explicit override ([policy evidence](evidence/mysql-malformed-human-approval-results-2026-10-04/manifest.json)). The GTK harness confirms malformed lexical SQL stops before approval, while a complete unparseable procedure displays its class/SQL in the approval dialog and denial preserves data under the SQLite fixture ([SQLite GTK evidence](evidence/gtk-unparseable-approval-results-2026-10-04/manifest.json)). A native MySQL session test verifies human approval routing and one-time procedure creation ([MySQL session evidence](evidence/mysql-unparseable-approval-results-2026-10-04/manifest.json)); a new MySQL-backed GTK layer checks dialog text, denial without procedure creation, and Allow Once with native catalog verification. That test exposed the pooled MySQL query path's missing text-protocol fallback for approved unparameterized DDL; pooled and explicit-session queries now share the fallback ([GTK/MySQL evidence](evidence/mysql-gtk-unparseable-approval-results-2026-10-04/manifest.json)). | Complete executable-identity and malformed-tail oracles for remaining dialects, then extend the MySQL delimiter boundary matrix beyond the tested forms. |
| B3/B4 | Result delivery and session state | The shared value path rejects incomplete rows instead of inventing NULL cells; PostgreSQL Docker and DuckDB local contracts preserve zero-row metadata, duplicate column names and row order; the row cap is checked at exactly `MAX_QUERY_ROWS` and one row over. SQL Server returns only the first result set but drains later sets, reports a later-set error, and remains usable afterward. All six remote drivers classify established server loss as `Disconnected`; setup-time refusal remains distinct. PostgreSQL and MySQL verify pool recovery; SQL Server, MongoDB, Redis and ClickHouse reconnect and complete a fresh operation after restart. Mid-stream/page loss now has explicit whole-operation failure contracts for PostgreSQL, MySQL, SQL Server, MongoDB cursor `getMore`, Redis browse-page key reads, and ClickHouse row streams. PostgreSQL ordinary cancellation `57014` remains a query error, and ClickHouse graceful shutdown remains a server cancellation error. A consolidated matrix now records each driver’s cancellation, reconnect and session-state expectations and maps them to the drivers or sandbox runner. Broader late-result completeness and acceptance scenarios remain open. | Add any remaining late-result completeness scenarios; assert row order/count, completeness, connection/session state and user-visible outcome at each affected consumer. |
| B4 acceptance | Secure connection and authorization | TLS fixture crates and policy/MCP enforcement tests exist; this survey has not audited their full matrix. | Trusted/untrusted/expired certificates, endpoint identity through SSH, bad credentials, lost sessions, read-only operations, scopes/allowlists and audit outcomes. Explicitly map supported mechanisms per engine. |

October 4 B3-4 follow-up: a SQLite computed `CAST(value AS BLOB)` over STRICT
`ANY` now checks INTEGER, REAL, TEXT, empty text, BLOB and SQL NULL results
against `typeof()` and exact `hex()` bytes. JSON retains computed bytes as
escaped text, XLSX stores them as shared strings, and typed CSV re-import into
another STRICT `ANY` table preserves the storage classes and bytes. See the
[CAST evidence](evidence/sqlite-cast-blob-any-csv-results-2026-10-04/manifest.json).

October 4 B3-4 follow-up: a local DuckDB enum CSV round trip now preserves the
empty label, literal `NULL`, Unicode, a quoted label, formula-shaped labels
and SQL NULL using a collision-free null marker. Native `typeof`, exact label
text and `IS NULL` checks verify the imported enum values; a pre-existing
destination row remains unchanged. The spreadsheet-safe CSV path prefixes
formula-like text, and its typed import plan maps `=1+1` to the same text as the
valid label `'=1+1`; that encoding is for spreadsheet presentation, while raw
CSV is the lossless restore format. See the [DuckDB enum CSV evidence](evidence/duckdb-enum-csv-roundtrip-results-2026-10-04/manifest.json).

October 4 B3-1 follow-up: one PostgreSQL `text[]` result now passes through the
XML, HTML and Markdown file writers; the SQL writer is replayed into PostgreSQL
and checked against native values. PostgreSQL `pg_typeof`, array text,
`array_to_json` elements and `array_send` bytes provide independent type,
content and round-trip oracles; hostile markup and Unicode exercise the text
writers; XLSX asserts a text cell and the expected shared-string content. This
is one array family. Other array types, spreadsheet-application re-import
beyond `text[]`, `uuid[]`, `enum[]`, domain-over-enum `[]`, `bytea[]`, `timestamptz[]`,
`interval[]`, `numeric[]` and `float8[]` cases, and additional enum configurations remain open.
See the [array file-writer
evidence](evidence/postgres-array-filewriter-results-2026-10-04/manifest.json).

October 5 B3-1 follow-up: PostgreSQL enum-array parameter text now has direct
codec cases for quoted and escaped labels, empty text, SQL NULL, custom lower
bounds, nested arrays, ragged shapes, inconsistent bounds and malformed tails.
The native domain-over-enum `ANY($1)` contract also passed against the server's
type and `array_send` wire-byte oracles
([evidence](evidence/postgres-enum-array-codec-boundaries-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: the domain-over-enum inferred array parameter matrix
now also exercises `= ALL($1)` beside `= ANY($1)` for quoted/escaped labels,
SQL NULL arrays, empty arrays and explicit multidimensional bounds. Each result
is compared with a typed PostgreSQL query, inferred array type and `array_send`
wire bytes ([evidence](evidence/postgres-enum-array-all-quantifier-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: the same `ANY`/`ALL` matrix now runs in a transaction
with a same-named shadow enum first in `search_path`; target types and wire
bytes remain exact, and a shadow-only label fails with PostgreSQL `22P02`
([evidence](evidence/postgres-enum-array-quantifier-shadowed-path-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: the existing PostgreSQL `text[]` workbook now also
survives LibreOffice Calc XLSX-to-ODS-to-XLSX re-save. Its 84-character shared
string is identical in all three artifacts, remains a string cell, contains
hostile markup only as escaped text, and creates no formulas
([Calc evidence](evidence/postgres-text-array-calc-reimport-results-2026-10-05/manifest.json)).

October 5 B3-1 follow-up: a PostgreSQL `uuid[]` XLSX cell with two UUIDs and SQL
NULL survives LibreOffice Calc XLSX-to-ODS-to-XLSX re-save as the same text.
The native fixture also rebinds the driver's quoted array text and checks exact
`array_send` bytes ([Calc evidence](evidence/postgres-uuid-array-calc-reimport-results-2026-10-05/manifest.json)).

October 5 B3-2 consumer follow-up: a PostgreSQL `date[]` containing a BC date,
year 10000, positive/negative infinity and SQL NULL survives LibreOffice Calc
XLSX-to-ODS-to-XLSX re-save with exact text, string cell types and no formulas.
The native result and rebound parameter match `array_send`
([Calc evidence](evidence/postgres-date-array-calc-reimport-results-2026-10-05/manifest.json)).

October 5 B3-2 consumer follow-up: a PostgreSQL `timestamp[]` containing a BC
timestamp, year 10000, the maximum finite timestamp, both infinities and SQL
NULL survives the same Calc re-save with exact text, string cell types and no
formulas; native `array_to_json` and `array_send` match after rebinding
([Calc evidence](evidence/postgres-timestamp-array-calc-reimport-results-2026-10-05/manifest.json)).

October 5 B3-2 consumer follow-up: PostgreSQL `time[]` now covers midnight,
fractional microseconds, the last finite microsecond before 24:00, 24:00 and
SQL NULL through the same Calc re-save. Native type, `array_to_json` and
`array_send` are checked before and after rebinding
([Calc evidence](evidence/postgres-time-array-calc-reimport-results-2026-10-05/manifest.json)).

October 5 B3-2 consumer follow-up: PostgreSQL `timetz[]` now preserves fractional
wall times, both maximum legal offsets and SQL NULL through the same Calc
re-save. Native type, `array_to_json` and `array_send` match after rebinding
([Calc evidence](evidence/postgres-timetz-array-calc-reimport-results-2026-10-05/manifest.json)).

October 5 B3-2 session follow-up: `timestamptz[]` file export now runs under
`America/New_York`, where the native oracle renders distinct fall-back offsets;
BookiE's result remains canonical UTC. Typed binding and SQL-file replay after
switching to `Asia/Kathmandu` preserve native `array_send` bytes
([evidence](evidence/postgres-timestamptz-array-non-utc-session-results-2026-10-05/manifest.json)).
The XLSX from that same New York session also survives Calc and Gnumeric
ODS/XLSX re-saves with the canonical UTC text, string cell kinds and no formulas
([spreadsheet evidence](evidence/postgres-timestamptz-array-non-utc-calc-results-2026-10-05/manifest.json)).

October 5 B3-1 consumer follow-up: PostgreSQL `boolean[]` with true, false and
SQL NULL survives the Calc re-save as exact text; native type, `array_to_json`
and `array_send` match after rebinding
([Calc evidence](evidence/postgres-boolean-array-calc-reimport-results-2026-10-05/manifest.json)).

October 5 B3-1 consumer follow-up: PostgreSQL `int8[]` with lower bound zero,
`i64::MIN`, `9007199254740993`, `i64::MAX` and SQL NULL survives Calc's
XLSX/ODS/XLSX re-save with the bound and every digit preserved. Native type,
`array_to_json` and `array_send` are checked after rebind
([Calc evidence](evidence/postgres-int8-array-calc-reimport-results-2026-10-05/manifest.json)).

October 5 B3-1 consumer follow-up: PostgreSQL `smallint[]` and `integer[]`
workbooks preserve type-specific signed boundaries, SQL NULL, and lower bounds
zero and two through Calc's re-save; both cells remain strings without formulas.
Native `array_to_json` and `array_send` match after rebinding
([Calc evidence](evidence/postgres-integer-arrays-calc-reimport-results-2026-10-05/manifest.json)).

October 4 B3-1 follow-up: a PostgreSQL `bytea[]` containing non-UTF-8 bytes,
empty bytea and SQL NULL now passes JSON, CSV, XLSX and replayed SQL file
writers. Native element hex, `array_to_json` and `array_send` oracles verify
the binary values and type after replay. The XLSX text cell also survives a
LibreOffice Calc ODS/XLSX re-save unchanged. Other array families and session
combinations remain open; see the [bytea[] file-writer evidence](evidence/postgres-bytea-array-filewriter-results-2026-10-04/manifest.json)
and [Calc re-import evidence](evidence/postgres-bytea-array-calc-reimport-results-2026-10-04/manifest.json).

October 4 B3-1 follow-up: one custom PostgreSQL `enum[]` result now passes
through JSON, CSV, XLSX and SQL file writers with labels `NULL`, empty text,
Unicode, comma, quote and markup, alongside SQL NULL. Native type, `array_to_json`
and `array_send` oracles verify the bound representation; replayed SQL restores
the same native text, JSON elements and wire bytes. XLSX stores a string cell
and XML-escapes markup. SQL replay now also preserves a backslash-bearing label
across all four `standard_conforming_strings`/`backslash_quote` combinations;
native text, JSON, wire bytes and an existing sibling are checked
([mode evidence](evidence/postgres-enum-array-sql-mode-results-2026-10-05/manifest.json)).
Other array families/configurations remain open. Separate
Calc re-import cases cover `enum[]`, `bytea[]`, `timestamptz[]` and `interval[]`
([enum[]](evidence/postgres-enum-array-calc-reimport-results-2026-10-04/manifest.json),
[bytea[]](evidence/postgres-bytea-array-calc-reimport-results-2026-10-04/manifest.json),
[timestamptz[]](evidence/postgres-timestamptz-array-calc-reimport-results-2026-10-04/manifest.json),
[interval[]](evidence/postgres-interval-array-calc-reimport-results-2026-10-04/manifest.json),
[domain-over-enum[]](evidence/postgres-domain-enum-array-calc-reimport-results-2026-10-04/manifest.json)).
See the [enum-array file-writer evidence](evidence/postgres-enum-array-filewriter-results-2026-10-04/manifest.json).

October 4 B3-4 follow-up: a PostgreSQL scalar custom-enum XLSX result now
survives LibreOffice Calc XLSX-to-ODS-to-XLSX re-save. Literal `NULL`, Unicode,
markup and formula-shaped labels remain text, SQL NULL remains blank, and an
empty enum label is refused without replacing the destination workbook. Other
spreadsheet applications and enum cell shapes remain open; see the
[scalar-enum Calc evidence](evidence/postgres-enum-scalar-calc-reimport-results-2026-10-04/manifest.json).

October 5 B3-4 follow-up: the same scalar-enum XLSX also survives Gnumeric's
XLSX/ODS/XLSX re-save. Literal `NULL`, Unicode, markup and `=1+1` stay string
cells, SQL NULL stays blank, and no formula is created ([Gnumeric evidence](evidence/postgres-enum-scalar-gnumeric-reimport-results-2026-10-05/manifest.json)).

October 5 B3-1/B3-4 follow-up: a PostgreSQL domain-over-enum array XLSX also
survives Gnumeric's XLSX/ODS/XLSX re-save with the escaped array text and
formula-shaped label unchanged, as a string cell without formulas
([Gnumeric evidence](evidence/postgres-domain-enum-array-gnumeric-reimport-results-2026-10-05/manifest.json)).

October 5 B3-2 app-consumer follow-up: PostgreSQL `date[]`, `time[]`,
`timestamp[]` and `timetz[]` now round-trip through the app parser and keyed
grid update with BC/extended years, infinities, 24:00, fractional precision,
maximum offsets and SQL NULL. `pg_typeof`, `array_to_json` and `array_send`
match a native cast oracle; invalid `date[]` is refused with target and sibling
rows unchanged
([evidence](evidence/postgres-temporal-array-grid-edit-results-2026-10-05/manifest.json)).

October 5 B3-1 app-consumer follow-up: PostgreSQL `smallint[]`, `integer[]` and
`bigint[]` now round-trip through the app parser and keyed update with signed
endpoints, lower bounds `-1`, `2` and `0`, and SQL NULL. The `bigint[]` case
preserves `9007199254740993`; native type, array text, JSON and wire bytes match
PostgreSQL oracles, and overflowing `smallint[]` input is refused without
changing the target or sibling
([evidence](evidence/postgres-integer-array-grid-edit-results-2026-10-05/manifest.json)).

October 5 B3-1 app-consumer follow-up: PostgreSQL `text[]` now passes a
lower-bound-zero grid edit with SQL NULL, literal `NULL`, empty text, comma,
quote, backslash, Unicode, markup and formula-shaped text kept distinct. Native
type, array JSON and wire bytes match; malformed array syntax is refused with
the target and sibling unchanged
([evidence](evidence/postgres-text-array-grid-edit-results-2026-10-05/manifest.json)).

October 5 B3-2/B3-4 follow-up: PostgreSQL `date[]`, `timestamp[]`, `time[]` and
`timetz[]` XLSX cells also survive Gnumeric XLSX/ODS/XLSX re-save with exact
boundary text, string cell types and no formulas
([Gnumeric evidence](evidence/postgres-temporal-arrays-gnumeric-reimport-results-2026-10-05/manifest.json)).

October 5 B3-1/B3-4 follow-up: PostgreSQL custom `enum[]` file export now
includes `=1+1` among the labels and proves it stays part of an escaped text
cell through native SQL replay under four string-mode combinations and Gnumeric
XLSX/ODS/XLSX re-save ([Gnumeric evidence](evidence/postgres-enum-array-gnumeric-reimport-results-2026-10-05/manifest.json)).
The same source workbook also survives the pinned LibreOffice Calc XLSX/ODS/XLSX
re-save unchanged, with string cell types and no formulas
([Calc evidence](evidence/postgres-enum-array-formula-calc-reimport-results-2026-10-05/manifest.json)).

October 4 B3-1/B3-4 follow-up: PostgreSQL custom enum metadata now covers a
schema, type and table whose names contain spaces and embedded double quotes.
The generated keyed edit and structured equality filter preserve the exact
native enum type and leave the sibling row unchanged; see the [spaces case](evidence/postgres-enum-quoted-identifiers-results-2026-10-04/manifest.json)
and [identifier-escaping follow-up](evidence/postgres-enum-identifier-escaping-results-2026-10-04/manifest.json).

October 4 B3-1 follow-up: the quoted target enum now competes with a same-named
shadow enum in the pool role's default `search_path`. The shadow enum lacks the
target-only label, so schema-qualified keyed writes and structured equality
filters must resolve the target type; native type/OID and untouched-shadow-row
checks pass ([evidence](evidence/postgres-enum-shadowed-quoted-search-path-results-2026-10-04/manifest.json)).

October 4 B3-1 follow-up: one PostgreSQL `timestamptz[]` with both sides of a
New York fall-back hour, a BC instant, infinities and SQL NULL now passes
through JSON, CSV, XLSX and replayed SQL. Native `array_to_json` semantics and
`array_send` bytes verify both instants remain distinct after binding and
re-import; XLSX stores the array as text. Other array families/configurations
and re-import with other spreadsheet applications remain open. Calc re-import
preserves this XLSX cell's exact text ([Calc evidence](evidence/postgres-timestamptz-array-calc-reimport-results-2026-10-04/manifest.json)).
See the [timestamptz-array file-writer evidence](evidence/postgres-timestamptz-array-filewriter-results-2026-10-04/manifest.json).

October 4 B3-4 follow-up: the PostgreSQL `timestamptz[]` XLSX text cell now
survives a LibreOffice Calc XLSX-to-ODS-to-XLSX re-save with exact text. It
includes the distinct instants on both sides of a New York fall-back
hour, a BC instant, both infinities and SQL NULL; the checker confirms text cell
kinds and no formula cells. Other array families and spreadsheet applications
remain open ([Calc evidence](evidence/postgres-timestamptz-array-calc-reimport-results-2026-10-04/manifest.json)).

October 4 B3-1 follow-up: a PostgreSQL `interval[]` with mixed-sign months,
days and sub-second time now passes through JSON, CSV, XLSX and replayed SQL
under one transaction's `postgres_verbose` `IntervalStyle`. Native
`array_to_json` and `array_send` checks verify binding and replay; rollback
leaves the destination empty. Its XLSX text cell also survives LibreOffice Calc
XLSX-to-ODS-to-XLSX re-save exactly, without formulas. Other interval styles
and spreadsheet applications remain open. See the [interval-array file-writer
evidence](evidence/postgres-interval-array-filewriter-results-2026-10-04/manifest.json)
and [Calc re-import evidence](evidence/postgres-interval-array-calc-reimport-results-2026-10-04/manifest.json).

October 3 B3-4 follow-up: table-based and direct nonempty query-result SQLite
STRICT `ANY` CSV paths now have exact tagged INTEGER/REAL/TEXT/BLOB restoration
with an explicit NULL marker and native storage-class/value assertions.
Ambiguous blanks and malformed reserved tags are refused. Empty query results
retain their declared `ANY` metadata and CSV header; other expression shapes,
formats and installed editing remain open. See
the
[table CSV case](value-contract-history.md#sqlite-strict-any-csv-storage-class-round-trip-2026-10-03)
and [query-result case](value-contract-history.md#sqlite-strict-any-query-result-csv-round-trip-2026-10-03),
with [empty-result evidence](evidence/sqlite-query-empty-metadata-results-2026-10-03/manifest.json).

October 4 B3-4 follow-up: mixed `CASE` and `COALESCE` expressions over STRICT
`ANY` keep fallback metadata while native `typeof()` proves per-row storage
classes. A `CASE` result also round-trips INTEGER, TEXT, BLOB and NULL through
the app CSV writer/importer with native storage-class checks; an empty TEXT
CASE result round-trips separately from NULL in CSV and is refused by XLSX
without replacing the existing workbook. COALESCE also
round-trips INTEGER, REAL, fallback TEXT, BLOB and stored TEXT; JSON keeps
numeric versus string values, and XLSX uses numeric cells for INTEGER/REAL and
text cells for fallback/stored TEXT, BLOB encoding and tag-prefix text. Spreadsheet re-import and other
expression consumers remain open. See the [CASE metadata evidence](evidence/sqlite-computed-any-results-2026-10-04/manifest.json),
[CASE CSV evidence](evidence/sqlite-case-any-csv-roundtrip-results-2026-10-04/manifest.json),
[CASE XLSX refusal evidence](evidence/sqlite-case-any-xlsx-refusal-results-2026-10-04/manifest.json),
[COALESCE metadata evidence](evidence/sqlite-coalesce-any-results-2026-10-04/manifest.json)
and [COALESCE CSV evidence](evidence/sqlite-coalesce-any-csv-roundtrip-results-2026-10-04/manifest.json),
[XLSX evidence](evidence/sqlite-coalesce-any-xlsx-results-2026-10-04/manifest.json).
Unambiguous attached-schema query origins also recover declared `ANY` metadata;
colliding flattened names retain fallback metadata. See [attached-origin evidence](evidence/sqlite-attached-any-metadata-results-2026-10-03/manifest.json)

October 4 B3-4 follow-up: SQLite compound `UNION ALL` outputs over STRICT
`ANY` retain fallback metadata through direct, CTE and derived-table queries,
ordinary/bound/transaction consumers, while native `typeof()` checks preserve
the INTEGER/TEXT/NULL row kinds. Direct `ANY` projection still recovers declared
metadata when a UNION occurs only in its filter. App file-writer contracts
check CSV text, XLSX numeric/string/blank cell kinds, and a compound-result CSV
round trip restoring INTEGER, TEXT, BLOB and NULL through native `typeof()`
checks. Spreadsheet-app re-import remains open. See the
[compound-result evidence](evidence/sqlite-union-any-results-2026-10-04/manifest.json),
[export evidence](evidence/sqlite-union-any-export-results-2026-10-04/manifest.json)
and [typed CSV evidence](evidence/sqlite-union-any-csv-roundtrip-results-2026-10-04/manifest.json).

October 4 B3-4 follow-up: grouped SQLite `MAX()` results over STRICT `ANY`
preserve per-group INTEGER, REAL, TEXT, BLOB and NULL runtime classes despite
fallback result metadata. Typed CSV export/import restores each class, exact
BLOB bytes, and reserved-tag-shaped text; source and restored `typeof()` values
are the native oracle. Other computed expressions, consumer formats and
spreadsheet-app re-import remain open. See the [MAX evidence](evidence/sqlite-max-any-results-2026-10-04/manifest.json).

October 4 B3-4 follow-up: grouped SQLite `MIN()` over STRICT `ANY` now checks
INTEGER, REAL, TEXT, BLOB and all-NULL groups, plus SQLite's numeric-before-text
ordering in a mixed group. Typed CSV re-import restores exact values and native
`typeof()` classes, including marker-shaped text and BLOB bytes; see the
[MIN evidence](evidence/sqlite-min-any-results-2026-10-04/manifest.json).

October 4 B3-3/B3-4 follow-up: ordered SQLite `group_concat()` over STRICT
`ANY` checks native numeric-to-text conversion, NULL skipping, empty-text
separators, all-NULL output and marker-shaped text. Typed CSV re-import restores
the exact text and SQL NULL under native `typeof()` checks; see the
[GROUP_CONCAT evidence](evidence/sqlite-group-concat-any-results-2026-10-04/manifest.json).

October 4 B3-3/B3-4 follow-up: SQLite `json_extract()` over STRICT `ANY`
produces dynamic INTEGER, REAL, TEXT and SQL NULL results from one expression.
The contract distinguishes JSON integer, real, string, boolean, null, object,
array and missing-path kinds with native `json_type()` while `typeof()` verifies
the result carrier. JSON export keeps large integers numeric and preserves
text/null distinctions; XLSX uses numeric cells for numeric results and shared
strings for extracted text without formulas. Typed CSV re-import preserves
storage classes and keeps JSON null distinct from a missing path in the
companion kind column. Spreadsheet-app re-import remains open. See the
[JSON extraction evidence](evidence/sqlite-json-extract-any-results-2026-10-04/manifest.json).

October 4 B3-3/B3-4 follow-up: grouped SQLite `SUM()` over STRICT `ANY`
preserves computed INTEGER, REAL and SQL NULL result classes through typed CSV
re-import. Numeric text contributes to integer/real sums by SQLite's native
rules, nonnumeric text and BLOB inputs yield REAL zero, and an integer-overflow
case is refused with SQLite's native error. See the
[SUM evidence](evidence/sqlite-sum-any-results-2026-10-04/manifest.json).

October 4 B3-3/B3-4 follow-up: SQLite `total()` over STRICT `ANY` returns REAL
for every group, including all-NULL input, and returns 0.0 for an empty input
set. An integer pair above `i64::MAX` remains a REAL result rather than raising
SUM's integer-overflow error; typed CSV re-import preserves the f64 values and
native `typeof()` results. See the
[TOTAL evidence](evidence/sqlite-total-any-results-2026-10-04/manifest.json).

October 4 B3-3/B3-4 follow-up: grouped SQLite `AVG()` over STRICT `ANY` returns
REAL for every non-NULL group, including integer and numeric-text inputs, and
SQL NULL for an all-NULL group. Nonnumeric text and BLOB values yield REAL
zero; a near-`i64::MAX` pair averages as REAL without integer overflow. Typed
CSV re-import preserves each result, verified by native `typeof()`. See the
[AVG evidence](evidence/sqlite-avg-any-results-2026-10-04/manifest.json).

October 4 B3-3/B3-4 follow-up: SQLite arithmetic over STRICT `ANY` preserves
dynamic INTEGER/REAL/NULL classes through `value + 1`, `value / 2` and
`value / 0`. Numeric text is coerced; nonnumeric text and BLOB inputs act as
zero; integer overflow promotes to REAL and integer division truncates. Division
by zero returns SQL NULL. Typed CSV re-import preserves each computed value and
class under native `typeof()` checks. See the
[arithmetic evidence](evidence/sqlite-arithmetic-any-results-2026-10-04/manifest.json).

October 4 B3-2 follow-up: MySQL `TIME(3)`, `DATETIME(3)` and `TIMESTAMP(3)`
round fractional input when `TIME_TRUNCATE_FRACTIONAL` is absent and truncate
when enabled; MariaDB truncates when `TIME_ROUND_FRACTIONAL` is absent and
rounds when enabled. Bound and generated-literal writes are checked against
native stored values; the TIMESTAMP cases also pin UTC session/epoch semantics.
An exact `TIME(3)` half-millisecond case confirms the `.789500` tie rounds to
`.790` or truncates to `.789` according to each engine's mode. Matching exact-
half `DATETIME(3)` and `TIMESTAMP(3)` cases now pin the same engine-specific
round/truncate behavior. New `TIME(0)` through `TIME(6)` coverage checks each
fractional precision under both engine defaults and the alternate mode, through
bound and generated-literal inserts. `DATETIME(0)` through `DATETIME(6)` and
`TIMESTAMP(0)` through `TIMESTAMP(6)` now also cover both engine defaults and
alternate modes, across bound/literal writes; timestamp assertions pin UTC
epoch microseconds. Other SQL-mode/session configurations and consumer paths
remain open. See the [TIME evidence](evidence/mysql-fractional-time-mode-results-2026-10-04/manifest.json),
[TIME exact-half evidence](evidence/mysql-fractional-time-tie-results-2026-10-04/manifest.json),
[DATETIME/TIMESTAMP exact-half evidence](evidence/mysql-fractional-half-boundary-matrix-results-2026-10-04/manifest.json),
[TIME precision matrix](evidence/mysql-time-precision-matrix-results-2026-10-04/manifest.json),
[DATETIME/TIMESTAMP precision matrix](evidence/mysql-datetime-timestamp-precision-matrix-results-2026-10-04/manifest.json),
[DATETIME evidence](evidence/mysql-fractional-datetime-mode-results-2026-10-04/manifest.json)
and [TIMESTAMP evidence](evidence/mysql-fractional-timestamp-mode-results-2026-10-04/manifest.json).

October 4 B3-4 follow-up: the MySQL app parser accepts fractional TIME(6),
DATETIME(6) and TIMESTAMP(6) edits as typed values, feeds them through the
keyed-update builder, and verifies native stored values, microseconds and an
untouched sibling row. A real MySQL result also exports to CSV, JSON and XLSX
with exact fractional temporal text, then round-trips through CSV import and
native writes. MySQL TIMESTAMP CSV import restores the UTC instant instead of
treating the RFC3339 value as a naive datetime. Other format consumers and
installed grid interaction remain open; see the
[native app-edit evidence](evidence/mysql-temporal-grid-edit-results-2026-10-04/manifest.json).

October 3 B3-1 follow-up: ordinary PostgreSQL enums and domains over enums now
cover all 17 shared structured-filter operators, including text-pattern
operators, list complements and both NULL checks. Exact row labels and native
enum/domain types are asserted. Direct query-parameter operator contexts and
session configurations remain open; see the
[complete filter matrix](evidence/postgres-enum-filter-matrix-results-2026-10-03/manifest.json).
The direct query parameter contract checks base-enum-cast `=`, `<>`, `<`,
`<=`, `>`, `>=`, `IN`, `NOT IN` and `BETWEEN`; raw domain equality remains explicitly refused.
See the [query-operator evidence](evidence/postgres-domain-enum-query-operators-results-2026-10-03/manifest.json)
and [ordering](evidence/postgres-domain-enum-param-operator-results-2026-10-03/manifest.json)
and [list-operator evidence](evidence/postgres-domain-enum-param-list-results-2026-10-03/manifest.json).
October 4 B3-1 follow-up: text and SQL NULL parameters infer as the custom enum
in both `COALESCE` argument positions and in `array_append(ARRAY[enum_column], $1)`.
Native type/value oracles, `22P02` invalid-label refusals and unchanged source
rows are checked ([evidence](evidence/postgres-enum-expression-parameter-results-2026-10-04/manifest.json)).
A separately bound text-array parameter explicitly cast to a domain-over-enum
array preserves literal `NULL`, empty text, Unicode, comma-containing text and
SQL NULL against native `array_send` and JSON oracles; invalid elements retain
SQLSTATE `22P02` ([evidence](evidence/postgres-domain-enum-array-parameter-results-2026-10-04/manifest.json)).
A 2×2 bound array with lower bounds 0 and 3 also preserves exact dimensions,
enum values and SQL NULL ([bounds evidence](evidence/postgres-domain-enum-array-bounds-results-2026-10-04/manifest.json)).
A `NULLIF(enum_column, $1)` follow-up checks matching, non-matching and SQL NULL
parameters, native inferred/result enum types, invalid-label SQLSTATE `22P02`
and unchanged rows ([evidence](evidence/postgres-enum-nullif-parameter-results-2026-10-04/manifest.json)).
For a domain-over-enum column under a shadowed `search_path`, raw `NULLIF` is
explicitly refused with `42883`; a qualified base-enum cast is the passing
control for text/NULL inference and native `22P02` invalid-label handling
([evidence](evidence/postgres-domain-nullif-parameter-results-2026-10-04/manifest.json)).
The same fixture now covers `IS DISTINCT FROM` and `IS NOT DISTINCT FROM` with
text and SQL NULL parameters, checking server-inferred enum types and exact
NULL-safe results; see [distinctness evidence](evidence/postgres-domain-enum-distinct-parameter-results-2026-10-03/manifest.json).
The same-named enum schema case applies structured equality, `IN` and `BETWEEN`
filters under the shadowed `search_path` and confirms they remain bound to the
target schema's native type; see the
[initial equality evidence](evidence/postgres-shadowed-enum-filter-results-2026-10-03/manifest.json)
and [list/range follow-up](evidence/postgres-shadowed-enum-filter-matrix-results-2026-10-03/manifest.json).
A direct bound equality, `IN` and `BETWEEN` query under the same collision also
select the target enum row and leave the shadow row unchanged; see [query parameter evidence](evidence/postgres-shadowed-enum-parameter-results-2026-10-03/manifest.json).
The fixture also changes `search_path` with transaction-local `SET LOCAL` and
checks direct `status = $1` inference against the qualified target table.
For diagnostics using polymorphic `pg_typeof($1)`, native preparation reports
ambiguous parameter type `42P08`, which the driver preserves across its text
fallback; the explicit target-enum cast is covered too.
Domain-over-enum metadata and structured equality also resolve to the target
schema when both the domain and its leaf enum have same-named shadow definitions.
The tested direct comparison, ordering, list, range and NULL-safe query
operators select the exact target rows after transaction-local `SET LOCAL search_path`;
shadow-domain rows and their siblings remain unchanged. See the [domain-over-enum
shadow evidence](evidence/postgres-shadowed-domain-enum-results-2026-10-03/manifest.json)
and its [direct operator matrix](evidence/postgres-shadowed-domain-enum-operator-results-2026-10-03/manifest.json).
The grid context-menu CSV serializer selects a collision-free SQL NULL marker
so empty enum labels, literal `NULL`, SQL NULL and marker-shaped labels remain
distinct. Formula sanitization and spreadsheet/restore parity remain open; see
[clipboard serializer evidence](evidence/postgres-enum-clipboard-csv-results-2026-10-04/manifest.json).

The three-level domain chain now checks inferred direct text and SQL NULL
parameters for both NULL-safe distinctness operators, with leaf enum and outer
domain `pg_typeof` assertions. See [three-level query evidence](evidence/postgres-three-level-enum-query-operator-results-2026-10-03/manifest.json).

A separate four-domain-layer contract verifies recursive enum-leaf metadata,
keyed edit, server-inferred enum query/update parameters, typed filter and
preservation of a SQL NULL sibling. A combined `pg_typeof($1)`/enum-comparison
query is explicitly refused with SQLSTATE `42P08`; an explicit cast is the
passing control. A five-domain follow-up verifies metadata, keyed edit,
inferred parameters, filter, invalid-label refusal and native sibling values.
A follow-up exercises 6-, 7-, 8-, 9-, 10-, 63-, 64-, 65-, 128-, 129- and
256-layer domains
with a same-named shadow enum on the session `search_path`; a same-backend
transaction changes `search_path` twice and repeats typed writes/filter checks
using previously fetched metadata. Rollback and exact outer type/value with
NULL preserved are asserted. Raw inferred SQL NULL updates preserve the outer
domain type through 63 layers; invalid text labels reach PostgreSQL and are
refused. At 64 or more layers, valid text, invalid text and SQL NULL return an
explicit unsupported operation both before and after schema-aware work in the
same backend. Schema-aware writes and filters pass through 256 layers, with
separate shadowed-session contracts extending that boundary through 259
layers ([257-layer evidence](evidence/postgres-domain-257-level-results-2026-10-04/manifest.json),
[258-layer evidence](evidence/postgres-domain-258-level-results-2026-10-04/manifest.json),
[259-layer evidence](evidence/postgres-domain-259-level-results-2026-10-04/manifest.json)).
Domain depths beyond 259 and other enum/session combinations remain open; see the
[four-level evidence](evidence/postgres-four-level-domain-results-2026-10-04/manifest.json),
[five-level evidence](evidence/postgres-five-level-domain-results-2026-10-04/manifest.json)
and [six/seven-level evidence](evidence/postgres-seven-domain-results-2026-10-04/manifest.json)
plus the [eight-level follow-up](evidence/postgres-eight-domain-results-2026-10-04/manifest.json)
and [nine-level follow-up](evidence/postgres-nine-domain-results-2026-10-04/manifest.json).
A [deep-domain boundary contract](evidence/postgres-deep-domain-results-2026-10-04/manifest.json)
records support through 63 layers and safe refusal at the 64-layer parameter-resolution limit.
A [follow-up through 256 layers](evidence/postgres-deep-domain-followup-results-2026-10-04/manifest.json)
extends schema-aware metadata, writes and filters through 256 layers while confirming raw-parameter refusal from 64 layers.
A [257-layer follow-up](evidence/postgres-domain-257-level-results-2026-10-04/manifest.json)
confirms target enum metadata, schema-aware keyed writes and filters under a
shadowed `search_path`, plus explicit raw-parameter refusal.
A separate same-backend transaction changes ordinary session `search_path`
twice, then verifies target enum metadata and keyed writes under the final
shadowed path; see the [session evidence](evidence/postgres-enum-session-search-path-results-2026-10-04/manifest.json).

### B3-P1 current coverage verdict — September 30

Temporal follow-up (2026-10-02): ClickHouse now checks both a one-hour New York
fall-back and Lord Howe's 30-minute fall-back against server local-text and
epoch oracles. BookiE refuses ambiguous values. Other zones and transitions
remain untested and need a contract or an explicit 0.2.0 scope decision.

Read the driver matrix in `type-contract-strategy.md` as four separate states:
exact typed support, exact text fallback, explicit refusal, and untested. The
current rows contain examples of all four. PostgreSQL range/multirange,
geometric, composite, money and JSON-array cases have independent server oracles
for their visible refusal; direct enum scalar/array projections preserve a
literal `NULL` label after the SQLx text-array parser fix. DuckDB off-microsecond TIMESTAMPTZ
grid edits now refuse before write, while native sub-microsecond binding and
additional nested combinations remain unsupported or untested. SQL Server's
legacy `datetime` now has native type/text and full tick parameter/SQL-literal
round-trip contracts; money/`smallmoney` and `sql_variant` retain visible
refusals. MySQL and MariaDB enum/set SQL-file and typed CSV
round trips plus JSON preservation and safe XLSX refusal cover four modes
([evidence](evidence/mysql-enum-sql-mode-results-2026-10-04/manifest.json));
additional MySQL SQL modes and DDL/session interactions, ClickHouse additional
nested and temporal combinations, SQLite
installed-grid transitions, and broad
cross-format parity remain untested. MongoDB keeps canonical Extended JSON for
supported nested/mixed values and now has native-server ObjectId, millisecond
DateTime, Generic binary and UUID binary edit contracts; collection-wide
heterogeneity remains untested. Redis's one-shot API explicitly refuses
Pub/Sub, MONITOR and CLIENT TRACKING ON; the local RESP3 attribute-wire test
passes, while asynchronous push consumption remains unsupported.

The first smallest MongoDB edit gap found here—a top-level ObjectId regular
field—has since been closed by `value_contract_object_id_grid_edit_preserves_native_bson`.
It keeps `_id` stable, reloads the edited `$oid`, and checks the stored BSON type
through an independent client. The clean strict report shows this test among
MongoDB's eight selected contracts. Collection-wide heterogeneity remains
open. Returned-page metadata now reaches the
browse grid, so late fields and mixed types update editability and rebuild
factories even when the number of columns is unchanged; whole-collection
schema discovery remains outside this page-scoped contract. See the updated
[value contracts](value-contract-history.md#mongodb-nested-bson-and-native-boundary-checkpoint).

The same follow-up found that BSON `date` values display RFC3339 text but the
generic app parser treated their metadata as date-only and rejected the value.
A failing-first app parser contract now proves that error; the driver-aware
parser preserves UTC milliseconds and refuses sub-millisecond edits. A MongoDB
7 app integration contract verifies parser-to-keyed-update-to-native-BSON
round-trip and unchanged data after refusal. The clean strict report passed 133
selected contracts across 11 suites at
`35d457fa488768a5204a26d785c90114073d4acd`; details and mutation evidence are
in [value contracts](value-contract-history.md#mongodb-bson-datetime-grid-edit-precision).

The September 30 pass checked two previously raised gaps against the current
tests. MySQL spatial values now have parser refusals for all eight spatial base
types and a Docker contract for GEOMETRY, POINT and MULTIPOLYGON that compares
native geometry type, WKT, stored HEX and row identity. SQLite INTEGER/REAL CSV
import is also already covered: `csv_import_preserves_text_in_sqlite_integer_real_and_numeric_affinities`
checks ordinary text, `42.50` and `42` against SQLite `typeof()` and returned
values in INTEGER, REAL and NUMERIC columns. These items are not missing tests.
The full six-driver disconnect audit likewise confirms established server loss
maps to `Disconnected`, mid-stream/page loss rejects the whole result, and
recovery is exercised through the existing pool for PostgreSQL/MySQL and a
fresh connection for SQL Server, MongoDB, Redis and ClickHouse. That does not
close the separate uniform cancellation, late-result and session-state matrix.

The same delivery audit added a MongoDB held-open-socket regression for an
explicit in-flight cancellation. It requires
`OperationOutcomeUnknown(Cancelled)`, distinguishing cancellation from
`Disconnected`; the focused case and all 36 MongoDB library tests passed. The
broader cross-driver session-state matrix remains open because each driver has
different cancellation and post-cancellation connection semantics.

The MySQL `STRICT_TRANS_TABLES` unsigned-edit gap is now closed by extending
`value_contract_mysql_unsigned_integer_grid_edits_refuse_coercion_and_preserve_u64`.
The contract covers permissive clamping and strict native rejection in pinned
sessions, parser refusal in both modes, valid exact edits and neighbor identity.
The focused command is
`cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib value_contract_mysql_unsigned_integer_grid_edits_refuse_coercion_and_preserve_u64 -- --include-ignored --test-threads=1`.
The dedicated-session column-comment DDL case now has a live integration test
under default mode and `NO_BACKSLASH_ESCAPES`. An October 4 native test also
verifies that MySQL strict zero-date modes refuse zero and incomplete dates
without writing, while valid DATE values stay exact
([evidence](evidence/mysql-strict-zero-date-results-2026-10-04/manifest.json)).
Other DDL/session interactions and SQL modes outside the enumerated cases
remain open in the driver matrix.

The earlier MongoDB page-scoped heterogeneity contract has since been extended
to a whole-collection cursor scan. Browse metadata and retained page rows now
come from the same cursor. A failpoint-driven app-server regression changes an
off-page value from String to Decimal128 while the cursor is blocked before that
document is read; the returned column becomes `mixed`, visible rows keep their
original values, and the grid refuses edits. A second regression blocks any
second `find` and asserts browse completes with one find. Before the change,
the off-page inter-command race left the metadata `string`; that failure is
retained in the [MongoDB census evidence](evidence/mongodb-census-results-2026-10-03/manifest.json).
This does not provide snapshot semantics for writes to documents already read
by the cursor; a concurrent write can still be overwritten. `run_find` merges
type changes in selected rows between its census and query, while off-page
changes after census remain invisible. Larger-scale full-scan performance
budgets, other export combinations and installed editing remain open. The
current checkout passed all 12 `value_contract_mongodb_` app tests, including
all five census/browse consistency cases ([current evidence](evidence/mongodb-current-census-results-2026-10-04/manifest.json)).
Earlier full-layer reports:
`20260930T215342621227Z-values/report.json` (raw report unavailable in this checkout: `../target/quality/20260930T215342621227Z-values/report.json`)
and
`20260930T215742792833Z-layers/report.json` (raw report unavailable in this checkout: `../target/quality/20260930T215742792833Z-layers/report.json`).

One ClickHouse nested-value combination is now covered beyond the existing
UInt128 containers: an array of tuples containing `Decimal(38, 9)` and
`DateTime64(9, 'UTC')`. The server type and `toJSONString` outputs are compared
with the decoded value; SQL export, parameter binding and keyed grid editing
must refuse the type-less JSON value, and the live MergeTree row must remain
unchanged. JSON and CSV exports, plus CSV import parsing, preserve the same
value against the native JSON oracle. Other nested combinations remain open; see the matching contract in
the value ledger. Scoped mutation testing of the ClickHouse decoder caught 9
of 10 changes, with one compile-time unviable replacement, no survivors and no
timeouts. The same fixture now covers a nested nullable/empty Decimal array
inside an array of tuples, keeping a high-precision Decimal, NULL element and
empty nested array distinct under the server JSON oracle. The strict combined
runner passed all 163 selected tests across 11 suites, including both nested
contracts.

### B3-P1 reconciliation update — October 1

The ClickHouse nested contract now checks fifteen Array/Map/Tuple shapes. The
latest additions `Tuple(String, Map(String, Nullable(Decimal(38, 9))))` and
`Array(Map(String, Nullable(Decimal(38, 9))))` were run
against the native `toTypeName` and `toJSONString` oracles, JSON/CSV parsing,
SQL and bind refusal, and unchanged MergeTree rows after edit refusal; its
focused Docker test passed. The runnable command and exact assertions are in
the [value ledger](value-contract-history.md#clickhouse-tuple-containing-a-nullable-decimal-map-2026-10-01).

The MySQL wide-decimal CSV path now validates catalog precision/scale and known
`UNSIGNED`/`ZEROFILL` modifiers, rejects unknown or repeated suffixes, and has a
live `DECIMAL(65,0) UNSIGNED` import contract. The 36-mutant scope, 168-test
strict values layer and quick gate passed on the updated source; evidence is
linked in the [wide DECIMAL checkpoint](value-contract-history.md#mysql-wide-decimal-csv-import-2026-10-01).

Disconnect and cancellation expectations are now consolidated in
[`disconnection-contracts.md`](disconnection-contracts.md), including the
separate sandbox-owned Redis cancellation target. B3 remains open for the
remaining temporal/nested combinations, unverified consumers and explicit
unsupported boundaries listed in `type-contract-strategy.md`; these updates do
not close those open targets.

The PostgreSQL enum contracts cover direct scalar/array results and schema-aware
keyed updates/draft inserts, including the literal label `NULL` separately from
SQL NULL. The MCP `execute_query` and JSON/CSV `export_data` paths have a PostgreSQL 16 contract for the literal label `NULL`, empty text, Unicode, SQL NULL and server-reported enum type. MCP CSV responses include a collision-free null marker; the native test imports that exact content using the returned marker and verifies the destination enum values and types. Core SQL file export is replayed into an enum destination and verifies the literal `NULL`, empty, Unicode, SQL NULL, and injection-shaped apostrophe label retain their native type and value. The vendored SQLx text-array parser now tracks quoting; the native
`UPDATE RETURNING` test verifies the label is persisted and its trigger fires
once. Explicitly cast bound enum values and schema-aware equality/inequality,
ordering, BETWEEN and IN filters are covered with schema-qualified casts and
native type checks. Schema-aware CSV import also binds enum values to the
catalog type and refuses ambiguous blank fields without an explicit NULL
marker. The shared `render_json` result renderer preserves empty text, literal
`NULL`, Unicode, and SQL NULL as separate JSON values. Raw text and SQL NULL
parameters infer the custom enum for queries, updates, and transactions when
the server can resolve the parameter from SQL context. Core file-writer JSON,
CSV, SQL, XML, HTML, and Markdown output now have native PostgreSQL enum coverage.
SQL-file replay also preserves an enum label containing literal `\n` and an
apostrophe across all four `standard_conforming_strings` and `backslash_quote`
combinations (`on/safe_encoding`, `on/off`, `off/off`, and `off/on`); backslashes
use explicit `E''` literals ([initial evidence](evidence/postgres-enum-sql-literal-session-modes-results-2026-10-04/manifest.json),
[full setting matrix](evidence/postgres-enum-sql-literal-session-modes-full-results-2026-10-05/manifest.json)).
The XML case distinguishes literal `NULL`, empty text, and SQL NULL while
escaping a markup-shaped label. HTML also distinguishes those values and escapes
a hostile image/event-handler label so it remains text. Markdown output quotes
text values and escapes table/markup characters, distinguishing literal `NULL`
from SQL NULL. XLSX writes tested non-empty enum labels as text cells and
leaves SQL NULL blank; the PostgreSQL fixture also verifies empty-label refusal
without replacing an existing workbook. Broader enum format and session
configurations remain open. Core CSV file export shows a collision-free NULL
marker that matches the import dialog; a PostgreSQL 16 round-trip verifies the
exact file bytes, empty enum label, marker-shaped labels, formula-shaped text
in raw mode, SQL NULL and stored enum type. Spreadsheet-safe formula prefixing
is a separate CSV configuration. Without an explicit marker, the importer
refuses ambiguous blank rows before writes. See the
[retained enum evidence](evidence/postgres-enum-results-2026-10-03/manifest.json).

The bounded DuckDB `UBIGINT[]` case above `i64::MAX` now verifies exact native
array text and safe refusal by the result, SQL-literal and binding paths. The
clean strict layer passed it on `6c2e61f`; other nested unsigned collection
shapes remain open.

The next DuckDB nested unsigned case extends this refusal contract to a STRUCT
with `UBIGINT` values above the signed boundary and `u64::MAX`, plus a NULL
field. The clean strict layer passed 187 tests across all 11 suites on
`19c5b6a`; other nested unsigned shapes remain open. See the
[value evidence](value-contract-history.md#duckdb-nested-ubigint-struct-refusal-2026-10-02).

The ClickHouse nested consumer contract now covers 16 shapes, including
`Array(Tuple(String, Map(UInt8, Nullable(UInt128))))` with a value above
`u64::MAX` and a NULL map value. Native type/JSON oracles, JSON/CSV
preservation, type-less SQL/bind refusal and unchanged MergeTree storage are
asserted ([follow-up evidence](evidence/clickhouse-nested-tuple-map-results-2026-10-04/manifest.json)).
Both numeric-key Map/Array nesting orders are covered; other nested
combinations remain open. The shared core XLSX writer has a passing exact-text
test for nested Extended JSON; ClickHouse-specific workbook parity and
spreadsheet-app re-import remain open.

DuckDB now has separate native-oracle refusal contracts for `UBIGINT[]` and a
`STRUCT` containing wide `UBIGINT` fields and NULL. The new STRUCT contract is
included in the clean 187-test, 11-suite value run on `19c5b6a`; other nested
unsigned combinations remain open.

The next DuckDB nested slice covers a `MAP(VARCHAR, UBIGINT)` with both unsigned
boundaries and NULL. Exact `typeof`/`VARCHAR` plus result/literal/bind refusal
passed in the clean 188-test strict layer and the quick local CI gate on
`efcbec7`.

The fixed-size DuckDB array contract now includes `UHUGEINT[2]` with a value
above `u64::MAX` and NULL. Native type/text oracles and result/literal/bind
refusal passed in the strict 189-test, 11-suite run and local quick gate on
`98611db`.

A native DuckDB UNION now has the corresponding nested unsigned boundary:
`UNION("value" UHUGEINT)` retains its exact native type and decimal text oracle,
while result, SQL-literal and parameter consumers refuse it. The strict
190-test, 11-suite layer and local quick gate passed on `5fb63af`.

Local anchors: `crates/drivers/postgres/tests/integration.rs`,
`crates/core/src/sql_lex.rs`, `crates/core/src/export/csv.rs`,
`crates/driver-tls-tests/tests/`, `crates/mcp/tests/enforce_policy.rs`.

## Test adoption flow

1. Record source project, exact revision, file/test and issue/fix link when available.
2. State the invariant, affected engines/types/consumers and expected failure mode.
3. Check existing local coverage; extend it or add the smallest missing case.
4. Establish expected behavior using engine/protocol documentation and a real
   server oracle. A different client is a comparison target, not the authority.
5. Run the reproducer against current code. Record reproduced, already covered,
   unsupported-by-design, not applicable, or blocked with the missing prerequisite.
6. Fix confirmed defects, rerun the reproducer and the relevant shared corpus.
7. Assign one existing regression tier and a runner; no orphan test files.
8. Keep exact value/type/scale/order assertions and negative cases. Add bounded
   property/fuzz cases for decoders and lexers; retain minimized failing seeds.

Each adopted case should carry: scenario ID, provenance, capability conditions,
input, expected value/type/metadata or explicit error, oracle, test location,
runner, baseline failure evidence and final passing revision. Do not hide failures
with broad retries, implicit skips, float-normalized comparisons or NULL fallbacks.

## Remaining survey work

- Server-loss contracts reproduce `Disconnected` after established operations
  for all six remote drivers, with setup-time refusal kept distinct. PostgreSQL
  and MySQL verify pool recovery; SQL Server, MongoDB, Redis and ClickHouse
  reconnect and complete a fresh operation after restart. Whole-query/page
  failure on mid-stream loss is covered for all six. A uniform cancellation,
  late-result and session-state matrix remains open.
- Inspect transport/authentication suites and fix commits in depth; only their
  inventory was sampled here. Include driver/protocol projects for wire behavior.
- Read the delegated Beekeeper helper assertions and CI entry points before
  adopting its stream/transaction test setup. The shared orchestration alone
  does not prove coverage across every engine.
- Review DBeaver statement-parser fixtures and the remaining SQLFluff dialect
  corpus selectively for our six SQL engines.
- October 5 B3-1 enum-array follow-up: `array_to_string` covers empty labels,
  literal `NULL`, omitted/replaced SQL NULL elements, empty and SQL NULL arrays,
  non-default bounds, multidimensional values and a same-named shadow enum.
  Untyped calls return `42804`; qualified parameters match native strings,
  types and wire bytes ([evidence](evidence/postgres-enum-array-to-string-results-2026-10-05/manifest.json)).
- October 5 B3-1 enum-array follow-up: `array_fill` checks typed and untyped
  polymorphic calls, target enum resolution under a shadow path, NULL and empty
  labels, SQL NULL fills, multidimensional bounds and zero-sized arrays against
  native type, JSON and wire oracles ([evidence](evidence/postgres-enum-array-fill-results-2026-10-05/manifest.json)).
- October 5 B3-1 enum-array follow-up: multi-array `unnest` accepts a
  qualified enum array beside an integer array under a shadowed `search_path`;
  untyped arguments return `42725`. Native rows verify storage order, row-major
  multidimensional flattening, NULL padding for shorter and SQL NULL arrays,
  empty arrays, ordinality, and each output type ([evidence](evidence/postgres-enum-multi-array-unnest-results-2026-10-05/manifest.json)).
- October 5 B3-1 enum-array follow-up: `array_sample` and `array_shuffle`
  validate randomized outputs as multisets, not fixed sequences. Repeated 1D
  sample sizes check subset counts; 1D shuffles preserve duplicates and NULLs;
  2D cases sample/shuffle whole slices. Empty/NULL arrays, SQL NULL results,
  oversized samples and shadow-only labels are covered ([evidence](evidence/postgres-enum-random-arrays-results-2026-10-05/manifest.json)).
- October 5 B3-1 enum-array follow-up: PostgreSQL cannot infer the enum-array
  argument to `unnest($1)` (`42725`, ambiguous function); under a same-named
  shadowed `search_path`, a qualified enum-array cast preserves flattened
  values, NULL elements, multidimensional/lower-bound wire bytes, empty arrays,
  SQL NULL, and target-label refusal ([evidence](evidence/postgres-enum-array-unnest-inference-results-2026-10-05/manifest.json)).
- October 5 B3-1 enum-identifier follow-up: schema and enum names containing
  spaces and embedded quotes remain bound to the target enum when a same-named
  quoted type leads transaction `search_path`; native type names, keyed-write
  values, filters, target/sibling rows, and shadow-only label refusal are checked
  ([evidence](evidence/postgres-enum-quoted-shadow-path-results-2026-10-05/manifest.json)).
- B3-1 implementation follow-up: scalar and temporal arrays now preserve
  elements, dimensions and lower bounds through server wire, SQL and JSON checks;
  built-in `boolean[]`, `bytea[]`, `uuid[]`, `timestamptz[]`, `integer[]`, `text[]`, `numeric[]`, and
  `float8[]` grid write-back pass with NULL preserved. The boolean[] case now
  traverses app parsing, the keyed builder and PostgreSQL wire verification with
  sibling-row identity; bytea[] verifies escaped byte elements, empty bytes,
  NULL and sibling wire identity; timestamptz[] checks offset-origin fractional instants/NULL and sibling wire identity with array_send; uuid[] verifies array_send equality and
  sibling identity. UUID[] also exposed scalar-parser dispatch on the whole array
  literal, now fixed by retaining PostgreSQL `[]` metadata as text; the parser
  matrix covers uuid/date/time/numeric/boolean/timestamptz arrays. The float8[] case also traverses CSV export/import
  and a keyed re-edit, with PostgreSQL wire-byte and sibling-row oracles. CSV
  import now keeps array-shaped cells as exact text. Broader array types and
  automatic parameter typing remain open. See the
  [array checkpoint](value-contract-history.md#postgresql-array-checkpoint).
- PostgreSQL IPv6 `inet[]` now has a bounded safe-refusal contract: independent
  server type/text/JSON and per-element host/prefix/family oracles pass, while
  result, SQL-literal and parameter consumers refuse the unsupported array.
- B3-2 implementation follow-up: end-of-day time and timetz offsets have a
  [server round-trip contract](value-contract-history.md#postgresql-time-checkpoint);
  temporal eras, infinities, mixed intervals and temporal arrays are also covered.
- Keep mutation testing alongside each decoder change and ensure its selected
  test filter includes malformed-input units as well as server regressions.
- DuckDB collection follow-up: a nested `UHUGEINT[]` result has a native type and
  exact-value oracle; unsupported decode, SQL literal and parameter consumers
  refuse it instead of narrowing the unsigned value. Other nested collection
  types and interval carrier limits remain open.
- B3-2 SQL export follow-up: BC dates and years above 9999 have a
  [server wire round-trip contract](value-contract-history.md#postgresql-era-and-mutation-checkpoint),
  including repeated-hour instants. Values outside chrono's range, infinities,
  mixed intervals and temporal arrays still need their own acceptance.
- ClickHouse Int128/UInt128 now have Docker-backed exact-text contracts for
  query results, text binding, SQL export/re-import and keyed grid edits; other
  wide-integer grid boundaries remain open.
- PostgreSQL 16 server oracles now cover `int4multirange` text, hull, component
  count and NULL, but SQLx fails direct projection while resolving `typtype`
  code `m`; this is an upstream metadata blocker, not a BookiE value refusal.
- B3-1 enum follow-up: custom enum-array CSV import resolves the direct array
  element's catalog type and emits a schema-qualified `enum[]` cast. The tests
  preserve literal `NULL`, empty text, Unicode, quoted/comma/markup labels,
  SQL NULL arrays and elements, empty arrays, a lower-bound-zero array,
  two-dimensional shape and an existing sibling row against native type, JSON,
  dimensions and wire-byte oracles. A same-named enum in a shadowed
  `search_path` cannot redirect the import away from the table's schema
  ([label evidence](evidence/postgres-enum-array-csv-import-results-2026-10-04/manifest.json), [shape evidence](evidence/postgres-enum-array-csv-shapes-results-2026-10-04/manifest.json), [shadowed-path evidence](evidence/postgres-shadowed-enum-array-import-results-2026-10-04/manifest.json)).
  Default blank CSV fields now restore SQL NULL arrays, while `{}` remains an
  empty array and scalar-enum blanks remain safely refused as ambiguous
  ([evidence](evidence/postgres-enum-array-default-null-results-2026-10-04/manifest.json)).
  Custom enum-array grid edits also now pass through the app parser and keyed
  update builder, with native type/value checks, invalid-label no-mutation and
  sibling wire-byte assertions; blank input remains SQL NULL while `{}` stores
  an empty array ([edit evidence](evidence/postgres-enum-array-grid-edit-results-2026-10-04/manifest.json), [NULL/empty evidence](evidence/postgres-enum-array-grid-null-results-2026-10-04/manifest.json)).
- B3-1 domain-array follow-up: CSV import now retains a domain-over-enum array's
  qualified element domain, round-trips native values and bytes, and refuses a
  value rejected by the domain CHECK without inserting its row. The app parser
  and keyed grid edit also retain the domain array cast and roll back invalid
  values without changing the stored row; structured equality filters match
  native domain arrays ([import evidence](evidence/postgres-domain-enum-array-import-results-2026-10-04/manifest.json), [grid evidence](evidence/postgres-domain-enum-array-grid-results-2026-10-04/manifest.json), [filter evidence](evidence/postgres-domain-enum-array-filter-results-2026-10-04/manifest.json)).
  A same-named domain earlier in `search_path` does not redirect CSV import
  from the target table's schema ([shadowed-path evidence](evidence/postgres-domain-enum-array-shadowed-import-results-2026-10-04/manifest.json)).
  JSON, CSV, XML, HTML, Markdown and XLSX file writers now retain the array
  representation; SQL replay restores the native domain-array type and bytes
  ([file-writer evidence](evidence/postgres-domain-enum-array-filewriter-results-2026-10-04/manifest.json)).
  A live UUID-domain array exposed `Undecodable` despite supported UUID[]
  elements; the decoder now uses the domain base OID and the regression checks
  bound/grid writes, a rejected CHECK value and unchanged siblings
  ([evidence](evidence/postgres-domain-uuid-array-results-2026-10-04/manifest.json)).
  Text, numeric and timestamptz domain arrays also round-trip through typed
  binding under a non-UTC session with native JSON and wire-byte oracles
  ([evidence](evidence/postgres-domain-array-family-results-2026-10-04/manifest.json)).

October 4 B3-5 follow-up: ClickHouse malformed heredocs now block whole-script
execution, suppress placeholders inside the unterminated body, survive
formatting as a malformed tail, and classify as unparseable/write with
fail-closed agent denial. The app contract exposed that shared parameter
extraction handled PostgreSQL dollar quotes but not ClickHouse's
`$tag$...$tag$` heredocs; the shared scanner now follows the existing ClickHouse
lexer rule ([evidence](evidence/clickhouse-malformed-heredoc-results-2026-10-04/manifest.json)).


October 4 B3-5 follow-up: the GTK harness now opens a real MySQL 8.0 fixture, shows the unparseable routine SQL/class in the approval dialog, checks Deny leaves no routine, and checks Allow Once creates one. The regression exposed pooled MySQL execution returning `This command is not supported in the prepared statement protocol yet` for approved routine DDL; pooled and session queries now share the existing text-protocol fallback. The pre-fix accessibility snapshot and post-fix local release-layer run are retained in the [GTK/MySQL evidence](evidence/mysql-gtk-unparseable-approval-results-2026-10-04/manifest.json). The separate [native MySQL approval test](evidence/mysql-unparseable-approval-results-2026-10-04/manifest.json) still records the guarded-session router contract.

October 4 B3-5 follow-up: MySQL's `\d` delimiter shorthand with a `//` token
now has editor-consumer coverage. Planner and formatter retain both shorthand
directives; the routine body stays one statement, only the following SELECT
placeholder is extracted, and post-format classification keeps the routine
unparseable/write. The app selector passes in the strict unit values layer; see
the [short-delimiter evidence](evidence/mysql-short-delimiter-results-2026-10-04/manifest.json).

October 4 B3-1 follow-up: PostgreSQL domain-over-enum `COALESCE` now checks
text and SQL NULL parameters in both argument positions. `pg_typeof` pins the
server's base-enum expression/parameter result while the source column remains
the outer domain ([evidence](evidence/postgres-domain-coalesce-results-2026-10-04/manifest.json)).
A companion contract covers text and SQL NULL in `array_append` and
`array_prepend`, including domain-array input and base-enum-array output
([evidence](evidence/postgres-domain-array-functions-results-2026-10-04/manifest.json)).

October 5 B3-4 follow-up: an isolated GTK test now publishes the generated enum
CSV through GDK, reads the clipboard text back exactly, and checks CSV parser
recovery for literal `NULL`, empty text, SQL NULL, marker-shaped labels and
formula-shaped text. The formula-safe export prefix is preserved by import, so
lossless formula-text restore is still open; paste through external spreadsheet
applications is also unverified ([GTK clipboard evidence](evidence/gtk-clipboard-csv-delivery-results-2026-10-05/manifest.json)).

October 5 B3-2/B3-4 follow-up: DuckDB `TIME_NS` and `TIMESTAMP_NS` app edits now
run through the grid parser and keyed-update builder into a real embedded
database. Exact type/text and nanosecond values survive, and the adjacent row
is unchanged ([evidence](evidence/duckdb-nanosecond-keyed-grid-edit-results-2026-10-05/manifest.json)).
Direct native nanosecond parameter binding remains open; the current driver
uses its exact-text cast path.
