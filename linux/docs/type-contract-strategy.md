# Type contracts for B3 and future drivers

Coverage reconciled against source at `35d457fa488768a5204a26d785c90114073d4acd`
and documentation tip `62dc257c7` on 2026-09-29. The source baseline includes
the Redis RESP3 contracts, PostgreSQL multirange metadata cases, ClickHouse
DateTime64(9) boundary behavior and SQL Server's safe `sql_variant` metadata
refusal, DuckDB mixed-interval exact-text round trips, and MySQL comment behavior
under both backslash modes. DuckDB primary-key metadata now includes composite
keys for keyed grid edits. The SQL Server type remains unsupported for exact
decoding. MongoDB's app parser now sends values beyond Rust Decimal range through
canonical `$numberDecimal` markers and preserves a 34-digit integer on a native
Decimal128 keyed edit. The strict value suite passed on this clean baseline with
16 DuckDB, 4 Redis, 8 MongoDB driver and 12 app contracts (133 selected tests across 11
suites, no missing suites);
other gates remain distinct from this source/test inventory. Evidence:
`target/quality/20260929T215946474345Z-values/report.json` (`dirty: false`).

A successful query is not evidence that its values survived. The acceptance unit
is a database type, its boundary cases, and each operation that consumes it.
Keep this work inside the current eight drivers. Oracle is a later driver project,
not a dependency of the 0.2.0 sprint.

## Required proof

| Layer | What to assert | Failure to prevent |
| --- | --- | --- |
| Wire decoder | Exact independent expected value, malformed lengths, invalid tags, overflow and resource limits | Debug text, truncation, panic, invalid data becoming NULL |
| Engine round trip | Compare the original native representation with parameter and SQL re-import | Rounding, timezone shifts, loss of decimal scale or interval fields |
| Shared consumers | Parser, grid editing, SQL INSERT, JSON/MCP, file export and import where applicable | Correct decode followed by a lossy consumer |
| Sessions | Repeat under relevant timezone, date/interval style, collation and SQL modes | Defaults masking configuration-dependent corruption |
| Failure paths | Unsupported values remain explicit; writes refuse; existing files survive | A green test produced by skipping, fallback conversion or partial output |
| Mutation and generated cases | Stable seeds, persisted failures, targeted arithmetic/sign/guard mutations | Assertions that execute code without detecting wrong results |

Use four distinct outcomes: exact typed support, exact text fallback, explicit
refusal, and untested. A refusal is a safety contract, not completed support.
Do not mark a type complete from one scalar query or from a crate's green badge.

## Boundary corpus

- Integers: every width and signedness boundary, adjacent values, zero and values
  around 2^53. Never compare exact integers through floating point.
- Decimal: precision and scale limits, trailing zeroes, negative values, exponents,
  tiny nonzero values, overflow, NaN/infinity where the engine supports them.
- Floating point: adjacent representable values, negative zero, subnormals,
  infinities and NaNs. Distinguish numeric equality from bit preservation.
- Temporal: both sides of every epoch, fractional units, precision truncation,
  leap dates, BC/large years, infinities, midnight versus 24:00, UTC offsets with
  seconds, DST folds/gaps, and the engine's finite range.
- Interval: preserve months, days and sub-day units independently; cross mixed
  signs with extremes and session styles. Duration equality alone is insufficient.
- Collections: NULL collection, empty collection, NULL elements, empty text,
  nesting, dimensions/lower bounds, element precision and size limits.

The existing shared scalar corpus remains in `testdata/value-contract.json`.
Add engine-specific fixtures when semantics differ; do not force all engines into
the narrowest shared type. PostgreSQL interval tests now add a deterministic
32-case generated corpus to explicit boundaries and run all 59 cases under four
styles. They construct expected wire bytes independently of the decoder and
verify the fixture before using it as an oracle.

## Current evidence and next targets

This is a prioritization ledger, not an exhaustive inventory of all crate tests.
Detailed checkpoint evidence is in [value contracts](value-contracts.md).

| Driver | Evidence established by the B3 contract suite | Remaining native-type targets |
| --- | --- | --- |
| PostgreSQL | Scalars, wide NUMERIC text, exact BIT/VARBIT text including leading zeroes, exact six-octet MACADDR and eight-octet MACADDR8 text, IPv6 `inet` host-prefix versus `cidr` network semantics and maximum `pg_lsn` across results, SQL literals and typed binding; scalar and temporal arrays, temporal eras/offsets/infinities, interval fields across styles; array SQL literals, typed text bindings, JSON text and server wire equality; boolean[], bytea[], uuid[], timestamptz[], integer[], text[], numeric[] and float8[] grid edits through allowlisted text casts, with float8[] proving adjacent values, negative zero, minimum subnormal, NaN/infinities and NULL against array_send, boolean[] verifying NULL/sibling identity, bytea[] verifying escaped bytea elements/empty bytes/NULL, uuid[] verifying NULL/sibling identity and timestamptz[] verifying offset-origin fractional instants/NULL plus sibling identity against array_send; PostgreSQL `[]` metadata is retained as text before scalar app parsing; `json[]` and `jsonb[]` have independent server type/value oracles and are explicitly refused by result, SQL-literal and parameter consumers; a custom enum[] has exact server type/text/JSON oracles, but direct projection fails SQLx enum metadata resolution before BookiE returns a value; all six built-in range types are explicitly refused against server type/text/bounds/inclusivity oracles; all six built-in multirange types have native type/text/hull/component oracles, with direct projection refused explicitly by SQLx metadata resolution; built-in point, line, lseg, box, path, polygon and circle each have exact native type/text oracles and are explicitly undecodable, with SQL-literal and parameter refusal and separate NULL checks; named composite results are explicitly refused against independent `row_to_json` and field-value oracles; native `money` is explicitly `Undecodable` against a `money::numeric::text` oracle and NULL remains distinct; `citext` preserves case-sensitive label text through results, SQL literal and typed binding while PostgreSQL comparisons remain case-insensitive; custom domains over `numeric`, `uuid`, and `jsonb` preserve their exact values through results and supported SQL consumers, with JSON null distinct from SQL NULL; enum labels preserve exact text through `enum::text`, SQL literal and typed text binding, with a server `pg_typeof` oracle; NUMERIC(80,40), NUMERIC(1000,1000), unconstrained numeric with 16,383 fractional digits and 131,072 integer digits, and other wide numeric grid edits preserve exact text on PostgreSQL; the app parser feeds wide input through the keyed-update builder to server `numeric::text` oracles for declared and unconstrained numeric; year-1,000,000 DATE and upper-bound year-294276 TIMESTAMP are visibly undecodable while their independent server text values remain exact | Full support for finite temporal values beyond the shared calendar, other non-numeric domains, direct enum/enum-array result decoding, other composites and other range types, other JSON array element types, automatic typing outside the safe built-in array list, additional custom types, and remaining consumer parity |
| DuckDB | Scalars, exact scalar HUGEINT signed min/max, UBIGINT signed-boundary/max and UHUGEINT max through results, SQL literals and text-bound casts; bounded native temporals and date text fallbacks; empty, literal-NULL and Unicode enum labels survive results, SQL literals and typed bindings while SQL NULL stays distinct; native DATE, microsecond TIME and TIMESTAMP parameters; sub-microsecond TIME/TIMESTAMP and every TIMESTAMPTZ parameter use exact VARCHAR fallback, verified by parameter type and echoed value; explicit casts to TIME_NS/TIMESTAMP_NS compare equal to exact nanosecond server literals, while DuckDB rejects `TIME_NS + INTERVAL`; a direct native binding test against the pinned DuckDB crate proves nanosecond TIME/TIMESTAMP parameters are reduced to microseconds; grid parsing refuses sub-microsecond edits for microsecond TIME, TIMESTAMP and TIMESTAMPTZ columns before writes, while finer TIME_NS/TIMESTAMP_NS types retain nanoseconds; independent cast oracles confirm the lower-precision native types drop those digits; simple/composite primary-key columns come from native constraint metadata; all eight sign combinations for mixed month/day/microsecond INTERVAL components decode as exact text, round-trip through SQL literals and explicitly cast parameters, and survive the app parser/keyed-update path as a native interval; interval month/day limits and the largest positive/negative microsecond-aligned carrier values have native `typeof`, `VARCHAR`, `date_part`, literal-cast and bound-cast oracles; wide-valued MAP, LIST, fixed ARRAY, STRUCT and UNION have native type/value oracles and are explicitly refused by decode and SQL-literal/parameter consumers; nested UHUGEINT[] is also explicitly refused | Installed GTK edit acceptance, additional nested collection combinations, extended timestamps, native sub-microsecond TIME/TIMESTAMP and TIMESTAMPTZ binding (exact text transport is tested; native typed binding remains unsupported), other nested unsigned wide integers, consumer parity |
| MySQL | Shared scalar and exact decimal paths; values wider than Rust `Decimal`, including 65 integer digits and scale 30, stay exact text through results and JSON/CSV export/import; typed CSV fallback is limited to values fitting explicitly declared precision/scale, accepts catalog `UNSIGNED`/`ZEROFILL` modifiers and refuses unknown/malformed suffixes and out-of-range values; native `HEX(CAST(... AS CHAR))` checks bound and SQL-literal re-import; unsigned BIGINT, signed and extended TIME, zero and partial-zero dates, YEAR, UTC TIMESTAMP decoding with pool reset before each checkout and explicit refusal in non-UTC dedicated sessions, text and JSON SQL export/re-import under default, `NO_BACKSLASH_ESCAPES`, `ANSI_QUOTES`, and combined modes on MySQL and MariaDB, BIT(1..64) as integers (bytes above i64), ENUM/SET labels, spatial values as stored SRID-prefixed bytes; unignored app parser contracts enforce signed/unsigned integer width boundaries, while the live keyed edit preserves `TINYINT UNSIGNED` 255 and `BIGINT UNSIGNED` `u64::MAX` with native text oracles; this edit refuses 256 before permissive-mode clamping can corrupt it; BIT(1), BIT(8) and BIT(63) also have parser-to-keyed-update contracts with native HEX() oracles; BIT(64) above i64 remains bytes and is refused by the parser; GTK tests keep spatial and too-wide BIT bytes read-only; backslash-bearing column comments are refused explicitly, with a live-server regression proving default and NO_BACKSLASH_ESCAPES store different comment bytes; unexpected SQLx I/O EOF after a server-side connection kill maps to `Disconnected` and the pool recovers, while `ConnectionRefused` remains distinct | SQL modes beyond permissive dates and backslash escapes, session-aware DDL comments, installed GTK-to-MySQL edit acceptance, consumer parity |
| SQL Server | Shared scalar and Unicode SQL export paths; datetime2(7) result decoding preserves the seventh fractional digit against independent server text and SQL literal re-import; smalldatetime's 29.998/29.999-second rounding threshold matches native text; legacy `datetime` ticks divisible by three decode exactly as nanoseconds while other ticks are explicitly undecodable, with SQL NULL distinct and independent server-text contracts; DATE year-0001/year-9999, TIME(7) minimum/maximum and DATETIME2(7) minimum/maximum supported calendar values are checked against independent native text through results, parameters and SQL export/re-import; datetimeoffset retains its original offset and 100 ns fraction through results, parameters, SQL export/re-import, exact JSON, and CSV export/import with native instant and offset oracles; money/smallmoney values are explicitly undecodable because Tiberius exposes them as floats; the pinned Tiberius `sql_variant` metadata panic is converted to an explicit unsupported error and the affected connection/session is retired, with a Docker regression checking exact bigint server text first | Lossless recovery of inexact legacy `datetime` ticks, money/smallmoney and `sql_variant`, plus temporal types or conversions outside the tested DATE/TIME/DATETIME2 calendar edges |
| ClickHouse | Shared scalar, decimal with declared scale, nonfinite and long-value contracts, `DateTime64` every scale 0 through 9 in-range plus extended-range scales 0 through 7, named-zone instants decoded through IANA timezone rules, nanosecond temporal parameters and SQL exports, local refusal of out-of-range `DateTime64(9)` parameters and SQL literals; a Docker-backed boundary contract checks exact lower/upper nanosecond epochs and pins ClickHouse 24.8 behavior just outside them (lower-year clamp, upper-range error); a New York DST fold proves ambiguous local text is `Undecodable` and refused by SQL-literal/parameter consumers, while a nonexistent spring-forward input is checked against ClickHouse 24.8's normalized wall time and epoch; Int128 min/max and UInt128 max remain exact text through real-server results, text binding, SQL and CSV export/import both with formula sanitization disabled and with default sanitization; keyed grid edits cover Int128 min/max and UInt128 zero/max adjacent values; ten nested Array/Map/Tuple combinations, including Array(Map), Map(Array), Tuple(Array), Map(Tuple), and Map(Array(Tuple(String, Nullable(Decimal(38, 9))))), preserve exact JSON against native type and toJSONString oracles; nested Decimal(38, 9)/DateTime64 and nullable/empty Decimal-array cases preserve exact JSON through JSON and CSV exports plus CSV import; SQL export and typed parameters refuse values without native type metadata, and real MergeTree keyed-edit attempts refuse all nested shapes while the native stored rows remain unchanged | Other temporal precision/timezone boundaries, additional nested combinations and cross-consumer parity |
| MongoDB | Decimal128 extremes remain exact text; BSON dates outside chrono's RFC3339 range, nested documents/arrays and all BSON enum kinds preserve canonical Extended JSON where needed; Generic, Function, BinaryOld, UUIDOld, UUID, MD5, Encrypted, Sensitive, Vector, Reserved `0a`, UserDefined `80`, and valid Column subtype `07` survive native-server keyed edits with exact subtype and bytes; valid BSONColumn payloads come from compressed time-series buckets, while malformed opaque bytes are refused by MongoDB as `NonConformantBSON` 378; aligned BSON DateTime edits preserve milliseconds and sub-millisecond `DateTime`/`TimestampTz` values are refused before rounding; native keyed-edit contracts cover Timestamp, regex, JavaScript, code-with-scope, Symbol, ObjectId, DbPointer, Undefined, MinKey and MaxKey, with canonical Extended JSON conversion and `value_to_bson` unit round trips; page metadata combines the first-50 sample with returned rows and now drives visible grid editability, filters and factory rebuilds when types change; homogeneous positive Decimal128 and formula-sanitized negative Decimal128 survive default CSV export, typed import parsing and keyed grid save as native BSON Decimal128; mixed BSON columns now decode each value as canonical Extended JSON, preserving String versus Decimal128 through JSON/CSV/XLSX; a real MongoDB MCP browse round trip compares the response with native BSON values for Decimal128, DateTime, user-defined binary, Int64, NULL and Unicode | Collection-wide heterogeneity outside sampled and returned-page documents |
| Redis | Integer/text/NULL protocol contracts; RESP3 hash maps retain key/value rows and invalid UTF-8 bulk values as bytes; XREAD stream replies preserve nested arrays and binary markers; a local RESP3 TCP fixture proves attribute wire frames survive the redis client and become tagged JSON; nested arrays, maps, sets and binary leaves retain JSON structure with explicit tags; BigNumber replies retain exact decimal digits as text, including inside nested arrays; map row caps set `truncated`; Pub/Sub subscribe/unsubscribe, MONITOR and CLIENT TRACKING ON are explicitly refused by the one-shot query interface and verified against Redis 7.4; tracking refusal covers BCAST, OPTIN, NOLOOP, REDIRECT and mixed-case command spellings | Consuming asynchronous push frames remains open; SQL date types are not applicable |
| SQLite | Shared scalar/binary contracts; NUMERIC affinity text, real, integer, blob and NULL transitions survive bound edits, SQL-literal re-import and policy-guarded CSV import; the app parser plus keyed-update contract verifies `42.50` becomes SQLite REAL `42.5` | Installed GTK/package acceptance across storage-class transitions; fixed-decimal storage is not applicable |

### B3-P1 consumer audit result — September 29

The remaining-column entries above are classified as untested combinations or
unfinished exact support unless they explicitly name a refusal or an external
blocker. Explicit refusal protects existing data but does not satisfy exact
support. Applied across drivers, the live status is: PostgreSQL has exact
scalar/array/temporal coverage plus explicit refusal or SQLx metadata blockage
for several advanced families; DuckDB has exact text fallbacks, grid-edit and
filter refusals for sub-microsecond values targeting lower-precision temporal
columns, with native sub-microsecond bindings still unsupported; MySQL has exact temporal/BIT and session-mode
contracts but broader SQL-mode and DDL-session cases open; SQL Server safely
refuses inexact legacy datetime ticks, money and sql_variant pending exact representations;
ClickHouse has explicit finite-range/DST behavior and now selects a lower exact
DateTime64 precision for values beyond the scale-9 Int64 ceiling, while more
nested/temporal combinations remain open; MongoDB has native-server edits for
named BSON kinds, including Generic and UUID-subtype binary; millisecond-aligned
DateTime writes are exact and sub-millisecond writes refuse before rounding.
Collection-wide type heterogeneity outside sampled/current-page documents
remains unverified, and other top-level edits outside named grid contracts
remain untested;
Redis refuses identified asynchronous command streams and tests RESP3 attribute
wire decoding, while asynchronous push consumption is unsupported; SQLite has
storage-class/parser/import contracts but installed GTK acceptance is untested.

### Connection-loss contract update — September 30

Docker-backed tests stop each remote server after a successful request and
require the next operation on the established connection to return
`DriverError::Disconnected` for PostgreSQL, MySQL, SQL Server, MongoDB, Redis
and ClickHouse. PostgreSQL and MySQL verify pool recovery both after a query
terminates a pooled connection and after the server container stops and restarts
with its host port held stable. SQL Server, MongoDB, Redis and ClickHouse restart
the same container, open a fresh driver connection and require a protocol-level
operation to succeed. These tests verify pool or explicit reconnect after
restart, not transparent recovery of a previous non-pooled connection handle.
SQLite and DuckDB are local engines, so they are not part of this remote-server-
loss set. All six remote drivers also have native-driver checks against an unused
local TCP port, proving connect-time refusal is distinct from established loss.
PostgreSQL and MySQL exercise this through SQLx pool setup, not only the pure
error mapper. After these cases were added, the complete `drivers` layer passed
207 driver, MCP, socket and SSH tests in 864.5 seconds with no failures. Evidence:
[`20260930T021134930354Z-layers/report.json`](../target/quality/20260930T021134930354Z-layers/report.json).

The complete layer was rerun at source SHA `8ed0f66ed54b1411feaac5d7a8e49abc6999fb18` on September 30. It ran 219 tests with zero failures or ignored tests in the executed suites, taking 939.7 seconds. All six remote-driver disconnect and stream/page-loss cases passed, including PostgreSQL/MySQL pool recovery and SQL Server/MongoDB/Redis/ClickHouse restart paths. Evidence: [`20260930T110146955489Z-layers/report.json`](../target/quality/20260930T110146955489Z-layers/report.json).

At `f88159dcb23dd74a960c638a6af42b608a1b4345`, the full `drivers` layer
passed again: 220 tests, zero failures, in 984 seconds. The executed log
contains established disconnect, whole-query failure after mid-stream loss,
pool or reconnect recovery, and six unused-port `ConnectionRefused` checks.
[`20260930T232244336622Z-layers/report.json`](../target/quality/20260930T232244336622Z-layers/report.json)
is the clean-tree report.

After splitting the nested MongoDB test into its own support module, the strict combined value-contract layer passed with GTK and DuckDB enabled at base SHA `736a73f8434a57fd33dbe9d6212956e29351b168`: 144 selected tests across 11 suites, no missing suites, and zero failures in 201.7 seconds. Its report records 746 fresh build artifacts and one rebuilt package. The quick layer then passed in 92.0 seconds, including the file-size and function-size guards. Evidence: [`20260930T114622059959Z-layers/report.json`](../target/quality/20260930T114622059959Z-layers/report.json) and [`20260930T114425279874Z-layers/report.json`](../target/quality/20260930T114425279874Z-layers/report.json).

Redis and ClickHouse mapper mutations are retained at
`target/quality/20260930-redis-disconnect-mutants-final/` and
`target/quality/20260930-clickhouse-disconnect-mutants-final/`. Redis caught
8/9 mutants, with one unviable and none missed or timed out; a temporary-filesystem
quota blocked the first copy-based baseline before mutation execution. The
in-place retry exposed two equivalent Redis predicates, which were simplified
before the final run. ClickHouse caught 4/5 mutants, with one unviable and none
missed or timed out; the first run's missed connect-error distinction gained a
regression before the final run. MySQL and MongoDB disconnect mapper mutation
evidence is recorded in [value contracts](value-contracts.md).

PostgreSQL and MySQL have Docker regressions that terminate a slow query after
row production has begun; each whole query must fail as `Disconnected`, never
return a partial result, and its pool must recover. ClickHouse tests both a
hard server stop (`Disconnected`) and graceful shutdown, which must return the
server's cancellation error rather than partial rows. SQL Server now tests
server loss while draining a multi-result stream and requires `Disconnected`,
no partial result, and a successful fresh connection after restart. These
focused cases passed. MongoDB now has a cursor `getMore` loss test after its
first batch, and Redis has a browse-page loss test after `SCAN` while reading a
key. Both require the whole operation to fail as `Disconnected`; Redis also
verifies that a later browse through the same driver object uses a fresh
operation-local connection and returns the full row. MongoDB now pins its
container to a stable host port and verifies the existing client recovers after
restart; a focused Docker regression passed. MongoDB also tests an in-flight
cancellation separately: the held-open socket yields
`OperationOutcomeUnknown(Cancelled)`, not `Disconnected`. PostgreSQL, MySQL,
ClickHouse, Redis and SQL Server retain their distinct cancellation and
post-cancellation connection policies. The tested expectations and runner
ownership are consolidated in [disconnect contracts](disconnection-contracts.md).
Native-type and consumer targets in this matrix remain open. Passing the
server-loss and explicit reconnect cases does not close those other gaps.

The MongoDB cancellation cell now has a Docker regression: `currentOp` confirms
`listCollections` is active before cancellation, the outcome is
`OperationOutcomeUnknown(Cancelled)`, and the same client successfully performs
a later collection listing. This completes the consolidated disconnect and
cancellation index; its per-driver test ownership and remaining post-cancellation
limits are in [disconnect contracts](disconnection-contracts.md). Native-type
and consumer targets in this type matrix remain open.

The strict combined values layer also passed against source SHA `636584f` on
September 30 in 219.9 seconds, with all selected suites exiting 0. Its report is
`target/quality/20260929T233014301456Z-layers/report.json`; local Docker
server-loss regressions are recorded separately because they are not all part of
the shared value-contract filter.

MongoDB Decimal128's bounded grid-edit gap is now covered for 34-digit integer
and fractional text beyond `rust_decimal`, with an independent BSON oracle; input
outside Decimal128 precision is explicitly refused.

PostgreSQL and MySQL now have failing-first server-side connection-loss tests.
PostgreSQL maps server-termination SQLSTATEs and unexpected SQLx socket loss to
`Disconnected`, preserving query cancellation, TLS and connection-refusal
errors. MySQL maps unexpected SQLx I/O loss to `Disconnected` while preserving
TLS and connection refusal; both Docker cases verify pooled query recovery.
The MySQL unsigned grid-edit contract now uses pinned sessions to compare the
permissive clamp with `STRICT_TRANS_TABLES` rejection, then verifies parser
refusal and a valid exact edit in strict mode. Other SQL-mode and DDL-session
combinations remain open.

MongoDB late-page BSON conflicts now have an app-level native-server contract:
after an initial 50-row string sample, a fetched page containing Decimal128
updates the column to `mixed`, rebuilds the grid layout and blocks inline edit.
An independent client checks the persisted BSON kind and row identity. This
establishes the current-page safety contract; a full-collection type census is
not part of the paged browse contract and is not claimed.

The Docker-backed app case passed with the strict GTK/DuckDB value selector:
26 app tests and 163 tests across all 11 configured suites passed at working
tree source `7fd33187c1f9bfa024a09be85e203bb82e8f24c4`. The quick layer also
passed on that tree. Reports:
[`20260930T215342621227Z-values/report.json`](../target/quality/20260930T215342621227Z-values/report.json)
and
[`20260930T215742792833Z-layers/report.json`](../target/quality/20260930T215742792833Z-layers/report.json).
Scoped mutation testing of `columns_for_browse_page` caught three of six
generated changes; the other three were unviable, with no misses or timeouts.
The in-place report is
[`outcomes.json`](../target/quality/20261001-mongodb-page-schema-mutants-inplace/mutants.out/outcomes.json).

ClickHouse nested refusal coverage now also includes an array of tuples carrying
`Decimal(38, 9)` and `DateTime64(9, 'UTC')`, checked against native type and JSON
oracles with unchanged-row assertions after consumer refusals. It now also
includes an `Array(Tuple(String, Array(Nullable(Decimal(38, 9)))))` case with
an exact high-precision decimal, NULL element and empty nested array. It uses the
same native JSON oracle and requires export, binding and keyed-edit refusal with
the stored row unchanged. More nested temporal, decimal and collection
combinations remain untested. Its scoped decoder mutation run caught 9 of 10
generated changes; one was compile-time unviable, with no survivors or
timeouts. The strict 163-test value run and quick layer passed on the working
tree; details are in [value contracts](value-contracts.md).

Two small MongoDB edit gaps identified in this audit now have native-server
contracts: a non-key ObjectId edit preserves `_id` and BSON kind, and an RFC3339
BSON DateTime edit preserves millisecond UTC instants while refusing sub-ms
input before writes. The latter combines app parsing, keyed update construction,
and an independent MongoDB client oracle. The clean strict run passed 133
selected contracts across 11 suites at
`35d457fa488768a5204a26d785c90114073d4acd`. Collection-wide heterogeneity and
named top-level edits without native-server assertions remain open; evidence is
in [value contracts](value-contracts.md#mongodb-nested-bson-and-native-boundary-checkpoint).

## Consumer coverage reconciliation, 2026-09-28

| Type / consumer | Established behavior | Remaining status |
| --- | --- | --- |
| PostgreSQL supported arrays / decode, SQL INSERT, JSON | Exact PostgreSQL array text preserves NULL, literal `NULL`, dimensions, lower bounds and supported scalar/temporal elements; real-server wire equality is tested after SQL and typed-text re-import | Built-in `integer[]`, `text[]`, `numeric[]`, `float8[]`, `boolean[]`, `bytea[]`, `uuid[]`, and `timestamptz[]` grid write-back are verified. The text case includes NULL and escaped text; numeric covers high precision, scale, special values and NULL; float8 covers adjacent/special values; bool, bytea and uuid cover NULL and sibling identity; timestamptz covers offset-origin fractional instants/NULL and sibling identity with native array_send oracles. Builder-level generated SQL assertions cover every allowlisted built-in cast alias and reject malformed modifiers. PostgreSQL `[]` metadata remains text in the app parser so scalar element names do not parse the whole array literal. Both `json[]` and `jsonb[]` are explicitly undecodable and refused by SQL literal and parameter consumers against independent server type/value oracles; no JSON-array support is claimed. Other unsupported element OIDs remain to be checked individually. |
| PostgreSQL array text / grid edit | Array values are `Value::Text`; app parser preserves array literals and the shared update builder casts allowlisted built-in array types safely. CSV import now also classifies array cells as text, preserving driver array syntax instead of parsing the element type as a scalar. | The real-server contracts reproduce and prevent SQLSTATE 42804 (`integer[]` target, TEXT expression); integer[], text[], numeric[], float8[], boolean[], bytea[], uuid[] and timestamptz[] grid regressions compare native values/wire bytes, with float8[] also tested through CSV export/import and keyed re-edit; timestamptz[] checks offset-origin fractions and NULL. Server oracles check array contents and sibling identity. UUID[] added a regression for accidental scalar parsing of the whole literal and verifies its native array_send representation. |
| PostgreSQL wide NUMERIC / grid edit | Builder unit tests prove `NUMERIC(80,40)` text edits emit a cast through `text` to static `pg_catalog.numeric`; ordinary text columns remain uncast and hostile metadata is rejected. Docker-backed contracts verify exact equality for `NUMERIC(80,40)`, `NUMERIC(1000,1000)`, unconstrained `numeric` with a 40-digit integer/40-digit fraction, maximum 16,383-digit fractional scale, maximum 131,072-digit integer width, a numeric domain through results and both SQL consumers, and keyed edits of `NaN`, `Infinity`, and `-Infinity` through PostgreSQL's `numeric::text` oracle. The app parser output reaches the keyed-update builder and real server for declared and unconstrained numeric, including all three special values. | Other text-backed PostgreSQL types remain open. |
| MongoDB nested BSON / grid, JSON, CSV, XLSX, MCP | Existing integration cases cover nested values and Timestamp/regex/MinKey/MaxKey/JavaScriptCode/JavaScriptCodeWithScope/Symbol/Undefined/DbPointer top-level edits plus canonical Extended JSON preservation across these consumers. The CodeWithScope edit verifies native code and Int64 scope fields; standalone JavaScriptCode, Symbol, Undefined and DbPointer edits verify native BSON kinds. A new Docker-backed app contract parses and saves a whole nested document through the keyed-update path, then checks exact Int64, Decimal128 and ObjectId kinds, the stable row key and untouched sibling. Page metadata combines the first-50 sample with returned page rows. Homogeneous positive and negative Decimal128 values survive default CSV export, typed parsing (including removal of the formula-safety marker for a valid decimal), and keyed edit as native BSON Decimal128. Mixed BSON columns decode to canonical Extended JSON values; a Docker-backed mixed String/Decimal128 contract verifies distinct JSON values plus CSV and XLSX output, retains native stored kinds, and is read-only in the app grid. | Collection-wide heterogeneity outside the sampled and returned page remains unknown. Other top-level BSON kinds outside the named edit cases remain untested. |
| SQLite NUMERIC affinity / parser, keyed grid save and import | The app parses `42.50` as Decimal, materializes the keyed edit, and SQLite stores the NUMERIC-affinity result as REAL `42.5`; driver contracts cover TEXT, BLOB, NULL, REAL and INTEGER transitions plus SQL-literal re-import | Installed GTK/package grid acceptance across the storage-class matrix remains open. |
| ClickHouse Int128/UInt128 / result, SQL, CSV and grid consumers | Local raw-token parser checks plus Docker-backed ClickHouse 24.8 tests prove Int128 signed minimum/maximum and UInt128 maximum remain exact `Value::Text` through results, text binding, SQL INSERT and CSV export/import both with formula sanitization disabled and default sanitization. The typed import removes the apostrophe marker only for valid Int128/UInt128 values in range; unit cases retain it for invalid, overflow, negative UInt128 and ordinary text. Keyed grid edits cover Int128 min/max and UInt128 zero/max adjacent values, with non-target row identity checked after each edit. | Other value-consumer boundaries remain untested. |
| JSON / floating point | Negative zero exports as numeric `-0.0`; parsing preserves its IEEE-754 sign bit and keeps positive zero and SQL NULL distinct. A deterministic matrix covers both signs across every finite exponent field and representative mantissa boundaries (20,480 values). Booleans remain JSON booleans and distinct from text `"true"`/`"false"` and SQL NULL. | Uncovered mantissas within the exponent bands remain open; this matrix is broad deterministic coverage, not exhaustive enumeration of all finite `f64` encodings. |
| CSV / numeric, floating point, time zone and NULL | `BIGINT` CSV export/import preserves `i64::MIN`, `i64::MAX`, and `9007199254740993` exactly; typed `DECIMAL(10,4)` export/import preserves the `12.3000` trailing-zero scale; negative zero, the smallest positive subnormal, largest finite `f64`, 262 seeded values and 20,480 deterministic finite exponent/mantissa-boundary values preserve IEEE-754 bits through CSV export and typed import; that same 20,742-value corpus preserves bits through JSON export/parse; a nine-digit offset-origin `TIMESTAMP WITH TIME ZONE` produces identical canonical UTC text through CSV and JSON, then CSV typed import recovers the same instant; `NaN`, positive/negative infinity and NULL retain distinct float/null variants through CSV export/import | Uncovered finite mantissas, broader temporal combinations and spreadsheet-application import remain open. |
| XLSX / wide numeric, temporal, nested BSON | Wide integers and exact decimals use text cells; temporal values use native cells only when exact, otherwise text; an offset-origin nine-digit `TIMESTAMP WITH TIME ZONE` is emitted as its exact canonical UTC instant; nested BSON markers are retained; negative zero, the smallest positive subnormal and the adjacent representable value above 1.0 are checked as numeric cells with IEEE-754 bit oracles | Spreadsheet-application re-import and remaining XLSX precision/format limitations remain per the checkpoint entries below. |

### DuckDB nested UHUGEINT refusal coverage — September 30

The native type ledger now includes both `STRUCT(amount UHUGEINT)` and a list
of that struct. Each case checks DuckDB's independent `typeof` and exact text
before requiring `Undecodable` output and refusal by SQL-literal and parameter
consumers. Other nested collection combinations remain open; this does not
claim general nested unsigned support.

### MySQL spatial grid refusal — September 30

All eight MySQL spatial column types now have driver-aware parser refusal
coverage. A native MySQL contract verifies type, WKT and stored HEX values for
GEOMETRY, POINT and MULTIPOLYGON and confirms the refused edit leaves both rows
unchanged. GTK-level read-only behavior remains covered separately; installed
GTK-to-MySQL acceptance and other SQL-mode cases remain open.

## Test workflow

1. Add a failing native-engine or decoder reproducer. Record the failure before
   changing production code. Include an independent expected value or native wire
   representation so a shared formatter bug cannot make both sides agree.
2. Add nearby boundaries and malformed input. Seed generated cases deterministically;
   promote every discovered failure into a named minimal regression.
3. Fix the narrow conversion boundary, then test its consumers and session paths.
   Keep exact text and unsupported markers distinct from ordinary typed values.
4. Run the affected crate tests, then `python3 scripts/run-test-layer.py full values
   harness`. The values layer includes GTK and DuckDB and checks that every listed
   contract executes exactly once; Docker failures are failures, not skips.
5. Run scoped cargo-mutants on the changed logic. Investigate survivors and timeouts;
   report unviable mutations separately. Broader scheduled mutation coverage
   complements this step. Add byte-level fuzz targets for decoder/parser input
   as a separate follow-up; this checkpoint does not add a fuzzing pipeline.
6. Update the ignored-test inventory when adding fixture tests, preserve reports,
   and document remaining gaps. Installed UI, Wayland and release acceptance remain
   separate gates.

When Oracle is introduced later, require this same applicable conformance suite
plus Oracle-native cases before claiming support. Define its NUMBER, DATE,
timestamp/timezone, interval, LOB and empty-string semantics explicitly. Do not
assume that passing another SQL driver's fixtures proves those contracts.
