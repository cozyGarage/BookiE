# B3 external test-scenario survey

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
`target/quality/20260929T215946474345Z-values/report.json` (`dirty: false`).
No external source code or fixtures were copied.

### B3-P6 mutation follow-up — September 29

The MySQL spatial GTK mutation report had five missed, stricter-read-only
mutations because that isolated test only asserted spatial refusal. An ordinary
VARCHAR positive-control contract now proves regular text columns remain
editable. Running the full app library suite against the editability guard caught
all 11 generated mutants, with no misses, timeouts or unviable changes. Evidence:
`target/quality/20260929-grid-editability-positive-control-mutants-final/mutants.out/outcomes.json`.

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
  type/consumer combinations and installed grid acceptance; see the current status
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

| Priority | Candidate | Existing evidence / gap | Next test and oracle |
| --- | --- | --- | --- |
| B3-1 | PostgreSQL arrays | Scalar and temporal arrays preserve elements, NULL, dimensions and lower bounds as exact text; SQL INSERT, typed text input, JSON output and server wire equality have real-server contracts. Both `json[]` and `jsonb[]` are explicitly refused as `Undecodable`, with native type, exact JSON text and semantic server oracles; SQL literal and parameter consumers refuse them too. Ordinary custom-enum scalar and array projections preserve labels, native type text and SQL NULL. Schema-aware keyed updates and draft inserts preserve ordinary labels and SQL NULL as the native enum, with sibling-row checks; a native shadow-schema case with search_path aimed at the shadow schema confirms same-named enum types resolve to the table declared schema. A literal label exactly `NULL` is preserved by keyed writes; direct enum scalar/array projections preserve the literal label `NULL` as text separately from SQL NULL; UPDATE RETURNING persists the label and fires its trigger exactly once. Structured enum filters cover equality/inequality, BETWEEN, IN and SQL NULL with schema-qualified casts and native type checks. CSV import applies the schema-qualified enum cast; with the default empty null marker it refuses blank enum cells as ambiguous, and a configured marker preserves empty label, literal `NULL` and SQL NULL as distinct values. MCP and core CSV file exports choose collision-free NULL markers; native PostgreSQL tests import raw-text output through the schema-aware importer and verify stored type/value. PostgreSQL shared JSON rendering preserves empty text, literal `NULL`, Unicode, and SQL NULL as distinct values. Safe built-in casts make integer[], boolean[], bytea[], uuid[] and timestamptz[] grid write-back pass with NULL preserved. The text[] contract checks NULL and escaped quote/backslash/comma through ordered unnest. The numeric[] grid contract checks wide precision, scale, NaN, ±Infinity and NULL with per-element `numeric::text` comparisons. The float8[] grid contract checks adjacent values, negative zero, the minimum subnormal, NaN, ±Infinity and NULL against `array_send` bytes. All eight built-in server grid contracts pass. The timestamptz[] case preserves offset-origin fractional instants and NULL against array_send while keeping the sibling wire value. UUID[] also caught a parser dispatch bug where the full array literal was treated as one UUID; PostgreSQL `[]` metadata now stays text before scalar parsing. | Name any remaining non-JSON array consumer/configuration gaps. Other file-writer formats and additional enum configurations remain separate open cases. |
| B3-2 | Temporal boundaries | End-of-day time, timetz offsets, BC/extended-year SQL literals, infinities, mixed interval fields and temporal arrays have server-backed contracts. DuckDB's mixed month/day/microsecond interval has an exact VARCHAR oracle and is refused by non-lossless result consumers; sub-microsecond temporal parameters are verified as exact VARCHAR echoes, with explicit casts required for typed expressions. SQL Server `smalldatetime` values immediately below/at the 29.999-second rounding threshold and legacy `datetime` inputs around 1/300-second ticks match independent native text; every third legacy tick is exactly representable in nanoseconds, while a 2/300-second tick is refused because its integer-nanosecond conversion is one nanosecond below the rational instant. ClickHouse's ambiguous New York fall-back `DateTime64` returns exact local text and a valid fold epoch, while BookiE and its SQL-literal/parameter consumers refuse to guess the missing offset; a nonexistent spring-forward input is checked against ClickHouse's normalized wall time and exact epoch. PostgreSQL year-1,000,000 DATE and upper-bound year-294276 TIMESTAMP/TIMESTAMPTZ now round-trip as exact text through results, SQL literals, bound casts, CSV import, and app keyed edits, with independent server wire-byte oracles. The maximum finite DATE `5874897-12-31` has result/literal/bind wire-byte checks. `int4range`, `daterange`, `numrange`, `tsrange`, and `int8range` now have independent type, canonical text, endpoint, and consumer-refusal oracles; `tstzrange` also has a dedicated fractional-instant and UTC-bounds contract. | Lossless representation of legacy `datetime` ticks that fall between nanoseconds, other native rounding boundaries and full non-SQL consumer/edit parity. |
| B3-3 | Nested JSON/BSON | MongoDB nested documents/arrays and BSON-only kinds preserve special markers; Decimal128 extrema/date bounds and binary subtype tags have exact regressions, with server checks for UUID/user-defined binaries, large nested Int64, explicit null, Unicode, JSON/CSV/XLSX, Timestamp/regex/MinKey/MaxKey/JavaScriptCode/JavaScriptCodeWithScope/Symbol/Undefined/DbPointer grid edits, canonical Extended JSON re-import and MCP browse output. Native keyed edits verify exact subtype and bytes for Generic, Function, BinaryOld, UUIDOld, UUID, MD5, Encrypted, Sensitive, Vector, Reserved `0a`, UserDefined `80`, and valid Column subtype `07`; BSONColumn input comes from MongoDB-compressed time-series buckets, while arbitrary malformed Column bytes are rejected by the server. Unignored codec unit contracts round-trip special variants through canonical Extended JSON and grid BSON conversion, verify mixed String/Decimal128 distinction through JSON/CSV, and verify collection-wide mixed metadata before decoding. The Docker contract checks a first-page String and a later Decimal128 page/query, retaining the `$numberDecimal` marker. A native sparse-document contract checks explicit BSON null and an absent field separately; absent fields use the distinct undecodable marker rather than `Value::Null`. Homogeneous positive Decimal128 and formula-sanitized negative Decimal128 survive default CSV export, typed import parsing and keyed edit as native BSON Decimal128. Mixed BSON columns now decode to canonical Extended JSON values; a Docker-backed String/Decimal128 contract verifies kind-distinct JSON/CSV/XLSX output, retains native BSON kinds in storage, and the app gate refuses editing. A separate failpoint contract records that scans are best-effort rather than snapshot-isolated when a document changes after it was read. `run_find` re-merges returned-page types after its separate census and filtered query; CSV/JSON export preserves markers in the materialized result without a refresh. A diagnostic profile measures 1,000- and 10,000-document census time for a 50-row page. | Snapshot isolation and optimistic conflict detection are not provided; off-page `run_find` changes after census are not observable. Scan timings are environment-specific, and broader consumer/edit combinations remain open. Keep SQL NULL distinct from JSON null. Exact commands and evidence are in [value contracts](value-contracts.md). |
| B3-4 | Export/import consumers | SQL binary, SQLite NUMERIC-affinity storage-class re-import and policy-guarded CSV import, XLSX integers/decimals/temporal fallbacks, nested BSON markers, finite/nonfinite floats, XML text and CSV quoting have regressions; JSON export and parse preserve negative-zero bits and distinguish positive zero and SQL NULL. CSV typed import/export preserves `i64::MIN`, `i64::MAX` and `9007199254740993` exactly, as well as negative-zero, smallest-subnormal and largest-finite `f64` bits. An unignored deterministic corpus checks 262 seeded plus 20,480 exponent/mantissa-boundary finite values through CSV export/import and JSON export/parse (20,742 total). JSON booleans remain distinct from text and SQL NULL. NaN/±Infinity remain distinct from NULL. A nine-digit offset-origin `TIMESTAMP WITH TIME ZONE` now has a direct CSV/JSON canonical UTC text parity assertion and CSV typed import verifies the same instant; XLSX emits that instant as exact canonical UTC text. XLSX keeps Excel-safe floats numeric and uses exact text for negative zero, subnormals, precision-risk floats and out-of-range magnitudes; workbook XML tests pin numeric boundaries. Spreadsheet-application re-import remains unverified. PostgreSQL app-parser output reaches the keyed-update builder and live server for declared/unconstrained NUMERIC, NaN/±Infinity and the maximum 16,383-digit scale and 131,072-digit integer width; a custom numeric domain preserves a wide value through results, SQL literal and typed binding. Custom enum labels preserve direct scalar/array results and schema-aware writes, filters and CSV import. The literal label `NULL` remains distinct from SQL NULL; default blank CSV imports refuse ambiguity, while an explicit null marker preserves empty enum labels and SQL NULL. A MySQL app parser/keyed-edit contract rejects out-of-range TINYINT UNSIGNED input before permissive-mode clamping and verifies exact BIGINT UNSIGNED u64::MAX persistence plus neighbor-row identity. SQLite app parser/keyed-save contracts verify NUMERIC input to REAL storage and STRICT `ANY` edits: existing INTEGER/REAL/TEXT kinds persist, clearing existing TEXT stores empty TEXT, empty NULL/new cells stay NULL, nonempty NULL/new input (including numeric-looking text) stays TEXT through keyed update/draft insert, and BLOB values remain read-only with exact bytes verified. DuckDB grid parsing keeps blank as SQL NULL, accepts `''` for an empty enum label and decodes doubled single quotes; keyed edits preserve native ENUM type and siblings, and invalid labels are refused without mutation. Installed GTK interaction remains open. ClickHouse wide integers round-trip through SQL and CSV with both raw and default formula-safe export settings; fifteen nested Array/Map/Tuple results match native type/JSON oracles, are refused by type-less SQL/parameter consumers, and real MergeTree keyed-grid edits verify refusal leaves stored id/value/type/JSON unchanged; the typed importer strips the apostrophe only for valid Int128/UInt128 values within range. Grid edits cover Int128 min/max and UInt128 zero/max adjacent values. XLSX explicitly refuses empty text; full format equivalence is unproven. | Uncovered finite mantissas, installed grid acceptance and broader temporal combinations. |
| B3-5 | Lexer/parser consumer agreement | A PostgreSQL CRLF script with issue-shaped leading, inter-statement, inline and trailing comments keeps its two executable statements in order across planner, named-parameter extraction, formatter and policy classification; SELECT/UPDATE classes, UPDATE target and WHERE survive formatting. The editor cursor contract maps GTK character offsets to byte offsets and selects the second statement after multibyte text. A malformed quoted tail reports a planner diagnostic, blocks whole-script execution, remains intact through parameter extraction and formatting, and classifies as unparseable/write. SQL Server GO batches retain three statements and batch policy through formatting, ignore placeholder-shaped delimiter comments, and classify SELECT/UPDATE/SELECT in order; `GO 2` keeps its count in the plan/formatter and is refused for execution, with only SQL placeholders extracted and full-script policy failing closed. MySQL `DELIMITER $$`, `DELIMITER //`, and repeated-semicolon `DELIMITER ;;` directives survive planning and formatting; routine bodies retain internal semicolons, trailing query identity/order is preserved, and body placeholders do not escape into parameter extraction. An unterminated quote in a MySQL routine produces a diagnostic, blocks whole-script planning, leaves malformed text intact through formatting, and is denied by fail-closed agent policy. | Broaden the executable-identity and malformed-tail oracle across dialects; cover other MySQL delimiter forms and human approval behavior for unparseable routines. |
| B3/B4 | Result delivery and session state | The shared value path rejects incomplete rows instead of inventing NULL cells; PostgreSQL Docker and DuckDB local contracts preserve zero-row metadata, duplicate column names and row order; the row cap is checked at exactly `MAX_QUERY_ROWS` and one row over. SQL Server returns only the first result set but drains later sets, reports a later-set error, and remains usable afterward. All six remote drivers classify established server loss as `Disconnected`; setup-time refusal remains distinct. PostgreSQL and MySQL verify pool recovery; SQL Server, MongoDB, Redis and ClickHouse reconnect and complete a fresh operation after restart. Mid-stream/page loss now has explicit whole-operation failure contracts for PostgreSQL, MySQL, SQL Server, MongoDB cursor `getMore`, Redis browse-page key reads, and ClickHouse row streams. PostgreSQL ordinary cancellation `57014` remains a query error, and ClickHouse graceful shutdown remains a server cancellation error. A consolidated matrix now records each driver’s cancellation, reconnect and session-state expectations and maps them to the drivers or sandbox runner. Broader late-result completeness and acceptance scenarios remain open. | Add any remaining late-result completeness scenarios; assert row order/count, completeness, connection/session state and user-visible outcome at each affected consumer. |
| B4 acceptance | Secure connection and authorization | TLS fixture crates and policy/MCP enforcement tests exist; this survey has not audited their full matrix. | Trusted/untrusted/expired certificates, endpoint identity through SSH, bad credentials, lost sessions, read-only operations, scopes/allowlists and audit outcomes. Explicitly map supported mechanisms per engine. |

October 3 B3-4 follow-up: table-based and direct nonempty query-result SQLite
STRICT `ANY` CSV paths now have exact tagged INTEGER/REAL/TEXT/BLOB restoration
with an explicit NULL marker and native storage-class/value assertions.
Ambiguous blanks and malformed reserved tags are refused. Zero-row query
metadata, computed expressions, attached-schema origins, other formats and
installed editing remain open; see the
[table CSV case](value-contract-history.md#sqlite-strict-any-csv-storage-class-round-trip-2026-10-03)
and [query-result case](value-contract-history.md#sqlite-strict-any-query-result-csv-round-trip-2026-10-03).

October 3 B3-1 follow-up: the PostgreSQL domain-over-enum structured filter
regression now also verifies `<>`, `<` and `BETWEEN` alongside equality, `IN`
and `IS NULL`, with native domain type checks. Other domain operator and
session contexts remain open; see the
[focused evidence](evidence/postgres-domain-enum-filter-results-2026-10-03/manifest.json).
The direct query parameter contract also checks base-enum-cast `=`, `<>` and
`BETWEEN`; raw domain equality remains explicitly refused. See the
[query-operator evidence](evidence/postgres-domain-enum-query-operators-results-2026-10-03/manifest.json).
The same-named enum schema case now applies a structured equality filter under
the shadowed `search_path` and confirms the filter remains bound to the target
schema's native type; see the
[focused evidence](evidence/postgres-shadowed-enum-filter-results-2026-10-03/manifest.json).

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
legacy `datetime`, `money`/`smallmoney` and `sql_variant` have visible refusals;
exact support remains open. Additional MySQL SQL modes and DDL/session
interactions, ClickHouse additional nested and temporal combinations, SQLite
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
under default mode and `NO_BACKSLASH_ESCAPES`. Other DDL/session interactions
and SQL modes outside the enumerated cases remain open in the driver matrix.

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
by the cursor. Full-scan cost, `run_find`'s separate schema/query reads, export
scope and installed editing remain open. The final app-server layer passed all
12 registered tests, including both MongoDB cursor cases. Reports:
[`20260930T215342621227Z-values/report.json`](../target/quality/20260930T215342621227Z-values/report.json)
and
[`20260930T215742792833Z-layers/report.json`](../target/quality/20260930T215742792833Z-layers/report.json).

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

The ClickHouse nested consumer contract now includes a numeric-key
`Map(UInt8, Nullable(UInt128))`, checking exact server oracles, JSON/CSV
preservation and safe refusal across type-less write consumers. The matrix has
fifteen nested shapes, including both numeric-key Map/Array nesting orders with
wide UInt128 values and NULL; other nested combinations remain open.

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
