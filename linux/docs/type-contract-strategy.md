# Type contracts for B3 and future drivers

Coverage reconciled against `linux` at `a90b86041`
on 2026-09-28. This is source/test inventory, not a fresh execution of every suite.

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
| PostgreSQL | Scalars, wide NUMERIC text, scalar and temporal arrays, temporal eras/offsets/infinities, interval fields across styles; array SQL literals, typed text bindings, JSON text and server wire equality; integer[] grid update through allowlisted text cast; `json[]` and `jsonb[]` have independent server type/value oracles and are explicitly refused by result, SQL-literal and parameter consumers; native `int4range` is explicitly refused against server range text and bound/inclusion-function oracles; native `money` is explicitly `Undecodable` against a `money::numeric::text` oracle and NULL remains distinct; `citext` preserves case-sensitive label text through results, SQL literal and typed binding while PostgreSQL comparisons remain case-insensitive; custom domains over `numeric` and `uuid` preserve their exact values through results, SQL literal and typed binding; enum labels preserve exact text through `enum::text`, SQL literal and typed text binding, with a server `pg_typeof` oracle; NUMERIC(80,40), NUMERIC(1000,1000), unconstrained numeric with 16,383 fractional digits and 131,072 integer digits, and other wide numeric grid edits preserve exact text on PostgreSQL; the app parser feeds wide input through the keyed-update builder to server `numeric::text` oracles for declared and unconstrained numeric; year-1,000,000 DATE and upper-bound year-294276 TIMESTAMP are visibly undecodable while their independent server text values remain exact | Full support for finite temporal values beyond the shared calendar, other non-numeric domains, direct enum result decoding, composites and other range types, other JSON array element types, automatic typing outside the safe built-in array list, other text-backed types, and remaining consumer parity |
| DuckDB | Scalars, bounded native temporals, date text fallbacks, enum labels; native DATE, microsecond TIME and TIMESTAMP parameters; explicit collection/interval refusal; nested UHUGEINT[] has exact native type/value oracle and is refused by decode, SQL-literal and parameter consumers; a mixed month/day/microsecond interval has an exact native VARCHAR oracle and the same consumer refusal | Exact interval and collection decoders beyond these explicit refusal cases, extended timestamps, native nanosecond and TIMESTAMPTZ parameters (the bundled client binds only microsecond TIMESTAMP, so these stay exact text), other nested unsigned wide integers, consumer parity |
| MySQL | Shared scalar and exact decimal paths, unsigned BIGINT, signed and extended TIME, zero and partial-zero dates, YEAR, UTC TIMESTAMP decoding with pool reset before each checkout and explicit refusal in non-UTC dedicated sessions, text and JSON SQL exports with and without NO_BACKSLASH_ESCAPES on MySQL and MariaDB, BIT(1..64) as integers (bytes above i64), ENUM/SET labels, spatial values as stored SRID-prefixed bytes; grid edits parse bounded BIT values and keep spatial/too-wide BIT bytes read-only; backslash-bearing column comments are refused explicitly | SQL modes beyond permissive dates and backslash escapes, session-aware DDL comments, end-to-end UI edit acceptance, consumer parity |
| SQL Server | Shared scalar and Unicode SQL export paths; datetime2(7) result decoding preserves the seventh fractional digit against independent server text and SQL literal re-import; smalldatetime's 29.998/29.999-second rounding threshold matches native text; legacy datetime boundary inputs match native millisecond rounding, while the 2/300-second tick is represented as 6,666,666 ns (1 ns below the rational instant) in chrono; datetimeoffset retains its original offset and 100 ns fraction through results, parameters and SQL export; money/smallmoney values are explicitly undecodable because Tiberius exposes them as floats | Exact legacy datetime tick representation below nanosecond precision, money/smallmoney binding and editing, sql_variant and other native temporal edge cases |
| ClickHouse | Shared scalar, decimal with declared scale, nonfinite and long-value contracts, `DateTime64` scales 0/3/6/9, named-zone instants decoded through IANA timezone rules, nanosecond temporal parameters and SQL exports, local refusal of out-of-range `DateTime64(9)` parameters and SQL literals; a Docker-backed New York DST fold proves ambiguous local text is `Undecodable` and refused by SQL-literal/parameter consumers, while a nonexistent spring-forward input is checked against ClickHouse 24.8's normalized wall time and epoch; Int128 min/max and UInt128 max remain exact text through real-server results, text binding, SQL and CSV export/import both with formula sanitization disabled and with default sanitization; keyed grid edits cover Int128 min/max and UInt128 zero/max adjacent values | Broader temporal bounds, nested values and other value-consumer boundaries |
| MongoDB | Decimal128 extremes remain exact text; BSON dates outside chrono's RFC3339 range, nested documents/arrays and uncommon top-level BSON kinds use canonical Extended JSON; generic binary remains bytes while other subtypes retain canonical metadata; large nested Int64, null, Unicode, nested document/array and top-level BSON Timestamp/regex/MinKey/MaxKey/JavaScriptCodeWithScope/Symbol/Undefined grid edits, canonical Extended JSON inserts, JSON/CSV/XLSX and MCP browse output preserve their contracts; page metadata combines the first-50 sample with returned rows; a Docker-backed mixed String/Decimal128 case reports `mixed` metadata, retains each stored BSON kind, and is read-only in the grid | Collection-wide heterogeneity outside sampled and returned-page documents, preserving scalar BSON kinds through generic JSON/CSV/XLSX exports, and editing remaining top-level special BSON kinds |
| Redis | Integer/text/NULL protocol contracts | Nested reply shapes and command-specific binary/number semantics; SQL date types are not applicable |
| SQLite | Shared scalar/binary contracts; NUMERIC affinity text, real, integer, blob and NULL transitions survive bound edits, SQL-literal re-import and policy-guarded CSV import; the app parser plus keyed-update contract verifies `42.50` becomes SQLite REAL `42.5` | Installed GTK/package acceptance across storage-class transitions; fixed-decimal storage is not applicable |

## Consumer coverage reconciliation, 2026-09-28

| Type / consumer | Established behavior | Remaining status |
| --- | --- | --- |
| PostgreSQL supported arrays / decode, SQL INSERT, JSON | Exact PostgreSQL array text preserves NULL, literal `NULL`, dimensions, lower bounds and supported scalar/temporal elements; real-server wire equality is tested after SQL and typed-text re-import | Built-in `integer[]`, `text[]`, and `numeric[]` grid write-back are verified. The text case includes NULL and escaped text; numeric covers high precision, scale, special values and NULL. Both `json[]` and `jsonb[]` are explicitly undecodable and refused by SQL literal and parameter consumers against independent server type/value oracles; no JSON-array support is claimed. Other unsupported element OIDs remain to be checked individually. |
| PostgreSQL array text / grid edit | Array values are `Value::Text`; app parser preserves text[] and numeric[] literals and the shared update builder casts allowlisted built-in array text types safely. | The real-server contract first reproduced SQLSTATE 42804 (`integer[]` target, TEXT expression); integer[], text[], and numeric[] grid regressions passed. The server oracles check each unnest element and NULL flag. |
| PostgreSQL wide NUMERIC / grid edit | Builder unit tests prove `NUMERIC(80,40)` text edits emit a cast through `text` to static `pg_catalog.numeric`; ordinary text columns remain uncast and hostile metadata is rejected. Docker-backed contracts verify exact equality for `NUMERIC(80,40)`, `NUMERIC(1000,1000)`, unconstrained `numeric` with a 40-digit integer/40-digit fraction, maximum 16,383-digit fractional scale, maximum 131,072-digit integer width, a numeric domain through results and both SQL consumers, and keyed edits of `NaN`, `Infinity`, and `-Infinity` through PostgreSQL's `numeric::text` oracle. The app parser output reaches the keyed-update builder and real server for declared and unconstrained numeric, including all three special values. | Other text-backed PostgreSQL types remain open. |
| MongoDB nested BSON / grid, JSON, CSV, XLSX, MCP | Existing integration cases cover nested values and Timestamp/regex/MinKey/MaxKey/JavaScriptCodeWithScope/Symbol/Undefined top-level edits plus canonical Extended JSON preservation across these consumers. The CodeWithScope edit verifies native code and Int64 scope fields; the Symbol edit verifies the persisted BSON Symbol kind. Page metadata combines the first-50 sample with returned page rows. A Docker-backed BSON String/Decimal128 pair with identical text produces `mixed` metadata, retains native stored kinds, and is read-only in the app grid. | The two scalar values still display/export as text and lose their BSON distinction in generic export; collection-wide heterogeneity outside the sampled and returned page remains unknown. |
| SQLite NUMERIC affinity / parser, keyed grid save and import | The app parses `42.50` as Decimal, materializes the keyed edit, and SQLite stores the NUMERIC-affinity result as REAL `42.5`; driver contracts cover TEXT, BLOB, NULL, REAL and INTEGER transitions plus SQL-literal re-import | Installed GTK/package grid acceptance across the storage-class matrix remains open. |
| ClickHouse Int128/UInt128 / result, SQL, CSV and grid consumers | Local raw-token parser checks plus Docker-backed ClickHouse 24.8 tests prove Int128 signed minimum/maximum and UInt128 maximum remain exact `Value::Text` through results, text binding, SQL INSERT and CSV export/import both with formula sanitization disabled and default sanitization. The typed import removes the apostrophe marker only for valid Int128/UInt128 values in range; unit cases retain it for invalid, overflow, negative UInt128 and ordinary text. Keyed grid edits cover Int128 min/max and UInt128 zero/max adjacent values, with non-target row identity checked after each edit. | Other value-consumer boundaries remain untested. |
| JSON / floating point | Negative zero exports as numeric `-0.0`; parsing the output preserves its IEEE-754 sign bit and keeps positive zero and SQL NULL distinct. | Other finite JSON floating-point bit patterns remain open. |
| CSV / integers, floating point, time zone and NULL | `BIGINT` CSV export/import preserves `i64::MIN`, `i64::MAX`, and `9007199254740993` exactly; negative zero, the smallest positive subnormal and the largest finite `f64` preserve their IEEE-754 bits through CSV export and typed import; a nine-digit `TIMESTAMP WITH TIME ZONE` value at `+05:30` exports as the exact UTC instant and imports identically; `NaN`, positive/negative infinity and NULL retain distinct float/null variants through CSV export/import | Other finite float bit patterns, broader cross-format temporal parity and spreadsheet-application import remain open. |
| XLSX / wide numeric, temporal, nested BSON | Wide integers and exact decimals use text cells; temporal values use native cells only when exact, otherwise text; an offset-origin nine-digit `TIMESTAMP WITH TIME ZONE` is emitted as its exact canonical UTC instant; nested BSON markers are retained; negative zero, the smallest positive subnormal and the adjacent representable value above 1.0 are checked as numeric cells with IEEE-754 bit oracles | Spreadsheet-application re-import and remaining XLSX precision/format limitations remain per the checkpoint entries below. |

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
