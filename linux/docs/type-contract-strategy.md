# Type contracts for B3 and future drivers

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
| PostgreSQL | Scalars, wide NUMERIC text, common scalar and temporal arrays, temporal eras/offsets, infinities, interval fields across styles | Finite ranges beyond the shared calendar, domains/enums/composites/ranges, JSON array elements, arbitrary-precision editing and consumer parity |
| DuckDB | Scalars, bounded native temporals, date text fallbacks, enum labels; native DATE, microsecond TIME and TIMESTAMP parameters; explicit collection/interval refusal | Exact interval and collection decoders, extended timestamps, native nanosecond and TIMESTAMPTZ parameters (the bundled client binds only microsecond TIMESTAMP, so these stay exact text), nested unsigned wide integers, consumer parity |
| MySQL | Shared scalar and exact decimal paths, unsigned BIGINT, signed and extended TIME, zero and partial-zero dates, YEAR, text and JSON SQL exports with and without NO_BACKSLASH_ESCAPES on MySQL and MariaDB, BIT(1..64) as integers (bytes above i64), ENUM/SET labels, spatial values as stored SRID-prefixed bytes; grid edits parse bounded BIT values and keep spatial/too-wide BIT bytes read-only; backslash-bearing column comments are refused explicitly | Session time zones and SQL modes beyond permissive dates and backslash escapes, session-aware DDL comments, end-to-end UI edit acceptance, consumer parity |
| SQL Server | Shared scalar and Unicode SQL export paths, datetime SQL exports, datetimeoffset as exact text with its original offset and 100 ns fraction through results, parameters and SQL export | Native temporal precision beyond datetimeoffset, money and variant families |
| ClickHouse | Shared scalar, decimal with declared scale, nonfinite and long-value contracts, `DateTime64` scales 0/3/6/9, named-zone instants decoded through IANA timezone rules, nanosecond temporal parameters and SQL exports, local refusal of out-of-range `DateTime64(9)` parameters and SQL literals | Ambiguous/nonexistent local times remain explicit undecodable values because the wire format omits their offset; broader temporal bounds, wider integers and nested values |
| MongoDB | Shared BSON command/result scalar contracts | Decimal128 extremes, nested BSON, native date range and binary subtypes |
| Redis | Integer/text/NULL protocol contracts | Nested reply shapes and command-specific binary/number semantics; SQL date types are not applicable |
| SQLite | Shared scalar and binary contracts | Dynamic storage-class/affinity transitions through edit/import/export; fixed-decimal storage is not applicable |

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
