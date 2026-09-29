# B3 external test-scenario survey

External sources below were first reviewed 2026-09-26 against BookiE `2eb9414c2`.
The local B3 status was reconciled against source
`35d457fa488768a5204a26d785c90114073d4acd` and documentation tip `62dc257c7`,
including September 26–29 changes. The source baseline contains Redis RESP3
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

| Priority | Candidate | Existing evidence / gap | Next test and oracle |
| --- | --- | --- | --- |
| B3-1 | PostgreSQL arrays | Scalar and temporal arrays preserve elements, NULL, dimensions and lower bounds as exact text; SQL INSERT, typed text input, JSON output and server wire equality have real-server contracts. Both `json[]` and `jsonb[]` are explicitly refused as `Undecodable`, with native type, exact JSON text and semantic server oracles; SQL literal and parameter consumers refuse them too. A custom enum[] fixture verifies exact server type, array text, and JSON semantics, but direct projection fails in SQLx enum metadata resolution before BookiE returns a value. Safe built-in casts make integer[] grid write-back pass with NULL preserved. The text[] contract checks NULL and escaped quote/backslash/comma through ordered unnest. The numeric[] grid contract checks wide precision, scale, NaN, ±Infinity and NULL with per-element `numeric::text` comparisons; both server grid contracts pass. | Other JSON array element types and automatic parameter typing remain unsupported/unverified; enum-array decoding is blocked by SQLx metadata handling. |
| B3-2 | Temporal boundaries | End-of-day time, timetz offsets, BC/extended-year SQL literals, infinities, mixed interval fields and temporal arrays have server-backed contracts. DuckDB's mixed month/day/microsecond interval has an exact VARCHAR oracle and is refused by non-lossless result consumers; sub-microsecond temporal parameters are verified as exact VARCHAR echoes, with explicit casts required for typed expressions. SQL Server `smalldatetime` values immediately below/at the 29.999-second rounding threshold and legacy `datetime` inputs around 1/300-second ticks match independent native text; chrono represents a 2/300-second tick as 6,666,666 ns, one nanosecond below the rational instant. ClickHouse's ambiguous New York fall-back `DateTime64` returns exact local text and a valid fold epoch, while BookiE and its SQL-literal/parameter consumers refuse to guess the missing offset; a nonexistent spring-forward input is checked against ClickHouse's normalized wall time and exact epoch. PostgreSQL DATE at year 1,000,000 and TIMESTAMP at year 294276's finite upper bound are explicitly refused as `Undecodable` while independent `::text` oracles remain exact; the app parser rejects both values. SQL literal and parameter paths also refuse their undecodable markers. `int4range`, `daterange`, `numrange`, `tsrange`, and `int8range` now have independent type, canonical text, endpoint, and consumer-refusal oracles; `tstzrange` also has a dedicated fractional-instant and UTC-bounds contract. | Full support for finite dates/timestamps outside the shared chrono range, exact legacy `datetime` tick representation below nanosecond precision, other native rounding boundaries and full non-SQL consumer/edit parity. |
| B3-3 | Nested JSON/BSON | MongoDB nested documents/arrays and uncommon top-level BSON kinds preserve special markers; Decimal128 extrema/date bounds and binary subtype tags have exact regressions, with server checks for UUID/user-defined binaries, large nested Int64, explicit null, Unicode, JSON/CSV/XLSX, Timestamp/regex/MinKey/MaxKey/JavaScriptCode/JavaScriptCodeWithScope/Symbol/Undefined/DbPointer grid edits, canonical Extended JSON re-import and MCP browse output. The CodeWithScope grid test verifies native code and Int64 scope fields; JavaScriptCode, Symbol, Undefined and DbPointer edits verify native BSON kinds. Unignored codec unit contracts round-trip Timestamp, regex, JavaScript, code-with-scope, Symbol, ObjectId, DbPointer, Undefined, MinKey and MaxKey through canonical Extended JSON and grid BSON conversion, verify mixed String/Decimal128 distinction through JSON/CSV, and verify a sample/page BSON type conflict is marked `mixed` before decoding. Page metadata includes returned rows beyond its first-50 sample in the Docker test. Homogeneous positive Decimal128 and formula-sanitized negative Decimal128 survive default CSV export, typed import parsing and keyed edit as native BSON Decimal128. Mixed BSON columns now decode to canonical Extended JSON values; a Docker-backed String/Decimal128 contract verifies kind-distinct JSON/CSV/XLSX output, retains native BSON kinds in storage, and the app gate refuses editing. | Collection-wide heterogeneity beyond sampled and returned-page documents and native-server grid acceptance for other special kinds beyond named integration cases remain open. Keep SQL NULL distinct from JSON null. The mixed-pair CSV/JSON/XLSX regression passed locally; exact command and evidence are recorded in [value contracts](value-contracts.md). |
| B3-4 | Export/import consumers | SQL binary, SQLite NUMERIC-affinity storage-class re-import and policy-guarded CSV import, XLSX integers/decimals/temporal fallbacks, nested BSON markers, finite/nonfinite floats, XML text and CSV quoting have regressions; JSON export and parse preserve negative-zero bits and distinguish positive zero and SQL NULL. CSV typed import/export preserves `i64::MIN`, `i64::MAX` and `9007199254740993` exactly, as well as negative-zero, smallest-subnormal and largest-finite `f64` bits. An unignored deterministic corpus checks 262 finite bit patterns through CSV export/import and JSON export/parse. NaN/±Infinity remain distinct from NULL. A nine-digit `TIMESTAMP WITH TIME ZONE` at `+05:30` exports as its exact UTC instant and imports identically; XLSX emits the same offset-origin instant as exact canonical UTC text. XLSX also verifies negative zero, the smallest subnormal and the value adjacent to 1.0 with bit oracles. PostgreSQL app-parser output reaches the keyed-update builder and live server for declared/unconstrained NUMERIC, NaN/±Infinity and the maximum 16,383-digit scale and 131,072-digit integer width; a custom numeric domain preserves a wide value through results, SQL literal and typed binding; custom enum labels preserve exact text through explicit `enum::text`, SQL literal and typed binding (direct enum result decoding remains open). A SQLite app parser/keyed-save contract verifies NUMERIC input to REAL storage. ClickHouse wide integers round-trip through SQL and CSV with both raw and default formula-safe export settings; the typed importer strips the apostrophe only for valid Int128/UInt128 values within range. Grid edits cover Int128 min/max and UInt128 zero/max adjacent values. XLSX explicitly refuses empty text; full format equivalence is unproven. | Installed grid acceptance, additional finite-float seeds and broader cross-format temporal parity. Parse generated files and re-import into typed columns where supported; document lossy format contracts. |
| B3-5 | Lexer/parser consumer agreement | A PostgreSQL CRLF script with issue-shaped leading, inter-statement, inline and trailing comments keeps its two executable statements in order across planner, named-parameter extraction, formatter and policy classification; SELECT/UPDATE classes, UPDATE target and WHERE survive formatting. The editor cursor contract maps GTK character offsets to byte offsets and selects the second statement after multibyte text. A malformed quoted tail reports a planner diagnostic, blocks whole-script execution, remains intact through parameter extraction and formatting, and classifies as unparseable/write. SQL Server GO batches retain three statements and batch policy through formatting, ignore placeholder-shaped delimiter comments, and classify SELECT/UPDATE/SELECT in order; `GO 2` keeps its count in the plan/formatter and is refused for execution, with only SQL placeholders extracted and full-script policy failing closed. MySQL `DELIMITER $$` and `DELIMITER //` directives survive planning and formatting, yield routine plus trailing query in order, and do not add parameters; the routine is unparseable and denied to agents by policy. | Broaden the executable-identity, malformed-tail and delimiter oracle across dialects; cover other MySQL delimiters and human approval behavior for unparseable routines. |
| B3/B4 | Result delivery and session state | The shared value path rejects incomplete rows instead of inventing NULL cells; PostgreSQL Docker and DuckDB local contracts preserve zero-row metadata, duplicate column names and row order; the row cap is checked at exactly `MAX_QUERY_ROWS` and one row over. SQL Server returns only the first result set but drains later sets, reports a later-set error, and remains usable afterward. PostgreSQL's Docker contract terminates a running backend, requires SQLSTATE `57P01` to surface as `Disconnected`, and verifies pool recovery; ordinary cancellation `57014` remains a query error. PostgreSQL and MySQL unit tests classify unexpected SQLx I/O loss as `Disconnected`, preserving `ConnectionRefused` and TLS errors; MySQL's Docker contract kills an active query connection and verifies pool recovery. MongoDB classifies transport I/O and server-selection loss during established operations as `Disconnected`, while initial refused endpoints remain `ConnectionRefused`; Docker tests cover bad credentials and stopping MongoDB after a successful operation. MongoDB pool recovery after server restart remains untested. Driver cancellation and session tests also exist. A uniform delivery matrix is not established. | Verify MongoDB behavior after server restart; exercise mid-stream failure/cancel and late results on other engines. Assert row order/count, completeness status and connection state. |
| B4 acceptance | Secure connection and authorization | TLS fixture crates and policy/MCP enforcement tests exist; this survey has not audited their full matrix. | Trusted/untrusted/expired certificates, endpoint identity through SSH, bad credentials, lost sessions, read-only operations, scopes/allowlists and audit outcomes. Explicitly map supported mechanisms per engine. |

### B3-P1 current coverage verdict — September 29

Read the driver matrix in `type-contract-strategy.md` as four separate states:
exact typed support, exact text fallback, explicit refusal, and untested. The
current rows contain examples of all four. PostgreSQL range/multirange,
geometric, composite, money and JSON-array cases have independent server oracles
for their visible refusal; direct enum-array projection stops in SQLx metadata
resolution before BookiE can return a value. DuckDB off-microsecond TIMESTAMPTZ
grid edits now refuse before write, while native sub-microsecond binding and
additional nested combinations remain unsupported or untested. SQL Server's
legacy `datetime`, `money`/`smallmoney` and `sql_variant` have visible refusals;
exact support remains open. MySQL session modes, ClickHouse additional nested
and temporal combinations, SQLite installed-grid transitions, and broad
cross-format parity remain untested. MongoDB keeps canonical Extended JSON for
supported nested/mixed values and now has native-server ObjectId and
millisecond DateTime edit contracts; collection-wide heterogeneity and other
unnamed top-level edits remain untested. Redis's one-shot API explicitly refuses
Pub/Sub, MONITOR and CLIENT TRACKING ON; the local RESP3 attribute-wire test
passes, while asynchronous push consumption remains unsupported.

The first smallest MongoDB edit gap found here—a top-level ObjectId regular
field—has since been closed by `value_contract_object_id_grid_edit_preserves_native_bson`.
It keeps `_id` stable, reloads the edited `$oid`, and checks the stored BSON type
through an independent client. The clean strict report shows this test among
MongoDB's eight selected contracts. Collection-wide heterogeneity and other
unnamed top-level edits remain open; see the updated
[value contracts](value-contracts.md#mongodb-nested-bson-and-native-boundary-checkpoint).

The same follow-up found that BSON `date` values display RFC3339 text but the
generic app parser treated their metadata as date-only and rejected the value.
A failing-first app parser contract now proves that error; the driver-aware
parser preserves UTC milliseconds and refuses sub-millisecond edits. A MongoDB
7 app integration contract verifies parser-to-keyed-update-to-native-BSON
round-trip and unchanged data after refusal. The clean strict report passed 133
selected contracts across 11 suites at
`35d457fa488768a5204a26d785c90114073d4acd`; details and mutation evidence are
in [value contracts](value-contracts.md#mongodb-bson-datetime-grid-edit-precision).

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

- Inspect transport/authentication suites and fix commits in depth; only their
  inventory was sampled here. Include driver/protocol projects for wire behavior.
- Read the delegated Beekeeper helper assertions and CI entry points before
  adopting its stream/transaction test setup. The shared orchestration alone
  does not prove coverage across every engine.
- Review DBeaver statement-parser fixtures and the remaining SQLFluff dialect
  corpus selectively for our six SQL engines.
- B3-1 implementation follow-up: scalar and temporal arrays now preserve
  elements, dimensions and lower bounds through server wire, SQL and JSON checks;
  built-in `integer[]` grid write-back passes with NULL preserved. Broader array
  types and automatic parameter typing remain open. See the
  [array checkpoint](value-contracts.md#postgresql-array-checkpoint).
- B3-2 implementation follow-up: end-of-day time and timetz offsets have a
  [server round-trip contract](value-contracts.md#postgresql-time-checkpoint);
  temporal eras, infinities, mixed intervals and temporal arrays are also covered.
- Keep mutation testing alongside each decoder change and ensure its selected
  test filter includes malformed-input units as well as server regressions.
- DuckDB collection follow-up: a nested `UHUGEINT[]` result has a native type and
  exact-value oracle; unsupported decode, SQL literal and parameter consumers
  refuse it instead of narrowing the unsigned value. Other nested collection
  types and interval carrier limits remain open.
- B3-2 SQL export follow-up: BC dates and years above 9999 have a
  [server wire round-trip contract](value-contracts.md#postgresql-era-and-mutation-checkpoint),
  including repeated-hour instants. Values outside chrono's range, infinities,
  mixed intervals and temporal arrays still need their own acceptance.
- ClickHouse Int128/UInt128 now have Docker-backed exact-text contracts for
  query results, text binding, SQL export/re-import and keyed grid edits; other
  wide-integer grid boundaries remain open.
- PostgreSQL 16 server oracles now cover `int4multirange` text, hull, component
  count and NULL, but SQLx fails direct projection while resolving `typtype`
  code `m`; this is an upstream metadata blocker, not a BookiE value refusal.
