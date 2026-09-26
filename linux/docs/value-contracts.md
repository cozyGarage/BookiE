# Value preservation tests

B3 uses a shared boundary corpus at `testdata/value-contract.json`. Tests must
compare the submitted value with the returned value, not just check that a query
succeeded. A NULL, an empty value, an unsupported value and a failed conversion
are different outcomes.

## Run

```bash
./scripts/test-value-contracts.sh --gtk --duckdb
./scripts/test-value-contracts.sh --gtk --duckdb --unit-only
```

The first command covers all eight drivers plus grid, filter, CSV import, named parameter, JSON
export and MCP conversion paths. Docker is required for the six server engines;
SQLite and DuckDB run locally. GTK development libraries are required for the
grid parser. Without `--gtk` or `--duckdb`, the report explicitly records those
exclusions. `--unit-only` keeps the same compilation configuration and runs only
the parser and consumer suites.

The runner compiles once, reads Cargo's test artifact records, and executes each
selected binary. A missing suite, zero matching tests, compilation error, test
failure or five-minute fixture timeout makes the command fail. Every selected suite gets a log and the combined
report is under `target/quality/*-values/report.json`. Failures in one engine do
not prevent the other compiled suites from running.

## Current corpus

| Path | Assertions |
| --- | --- |
| PostgreSQL, MySQL, SQLite, SQL Server, ClickHouse, DuckDB | Bound parameters and generated SQL preserve signed integer limits, values around 2^53, small/large floats, text and NULL |
| Fixed-decimal SQL engines | Positive and negative decimals retain their value through binding and generated SQL; SQLite has no fixed-decimal storage contract |
| Redis | Integer command replies and quoted text arguments retain their values; a missing key differs from empty text |
| MongoDB | JSON commands preserve integer, float, text and NULL values through BSON and result decoding |
| Grid, filter, CSV and named parameter parsers | Integer overflow and decimal rounding are refused; representable boundaries survive parsing |
| Float input parsers | Numeric overflow to infinity and nonzero underflow to zero are refused |
| XLSX | Integers beyond 15 digits and all exact decimals are text cells; stored XML verifies each value and cell reference, including decimal scale |
| JSON and MCP | Non-finite values remain distinct from SQL NULL |

ClickHouse long-value reads are checked with a server-generated value. Oversized
inline SQL is required to return its explicit query-size error; the fixture does
not raise that server limit or accept a truncated success.

Text cases include empty strings, numeric-looking strings, Unicode, apostrophes,
quotes, backslashes, line breaks and text beyond 256 KiB. Float assertions in the
SQL harness compare bit patterns. Existing driver tests still cover binary
exports, typed row identity, cancellation, metadata and other engine behavior.
The new suite supplements those tests.

This is a growing contract, not proof of every database type. PostgreSQL arbitrary-precision numeric editing, native arrays, temporal extremes and intervals, nested BSON values,
spreadsheet floating-point/temporal precision and every transport/persistence adapter still need
focused cases. Add a reproducer before changing a decoder or parser. Never make
a failing exact-value case pass by converting both sides to floats or by treating
an unsupported result as NULL.

## Build reuse

Cargo already caches build artifacts in `target`. Preserve that directory and
keep the toolchain, feature selection, profile and compiler flags consistent.
Changes to a shared crate correctly rebuild its consumers. Different package
selections can produce different dependency features and build-dependency hashes;
that can rerun DuckDB's C++ build even when its own feature list is unchanged.
See [Cargo build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)
and [feature resolution](https://doc.rust-lang.org/cargo/reference/features.html#feature-resolver-version-2).

Use the same flags for repeated value-suite runs. The report records compile
seconds, fresh artifacts and rebuilt packages, so cache reuse is measurable.
Changing the test filter with `--unit-only` does not change the compile graph.
The local integration runner also selects the six server driver packages in one
Cargo invocation, including MongoDB. The PostgreSQL socket fixture preserves
`RUSTUP_HOME` while isolating application state, avoiding a fresh toolchain
download into its temporary home.

A secondary compiler cache such as [sccache](https://github.com/mozilla/sccache)
can help when switching build configurations or rebuilding native code. It is
not installed or enabled by this change. It cannot avoid relinking changed code
or replace tests, and Rust incremental compilation affects what it can cache.
No existing artifacts are deleted and no global Cargo or desktop settings change.

## Measured checkpoint: 2026-09-26

The complete `--gtk --duckdb` suite passed twice after the fixes. Evidence:

- `target/quality/20260926T182536049588Z-values/report.json`: all 11 selected
  suites passed; 33.393 seconds compiling changed Rust code.
- `target/quality/20260926T182710143702Z-values/report.json`: unchanged rerun;
  all suites passed, 0.924 seconds in the build step, 745 fresh artifacts and
  zero rebuilt packages. Database startup and test execution still take time.

- `target/quality/20260926T184410518775Z-integration/report.json`: 93 server
  integration tests and two PostgreSQL socket tests passed.
- `target/quality/20260926T184128878545Z-full/report.json`: full local gate
  passed; Debian package validation skipped because `dpkg-deb` is unavailable.
- `target/quality/20260926T184028346457Z-values/report.json`: final parser
  refinement and runner timeout handling; all 11 suites passed.

After preserving `RUSTUP_HOME`, the socket fixture passed again with build
steps of 0.50 and 0.33 seconds and no toolchain download.

These runs used the dirty working tree based on `9b7a996ca`; these are local
implementation results, not qualification of a frozen release candidate.

## XLSX integer and decimal checkpoint

The regression first reproduced `i64::MIN` becoming `-9223372036854776000`
and a wide decimal becoming `100000000000000000000`. Exports now store
integers beyond 15 digits and every exact decimal as text. Smaller integers
remain numeric. Decimal scale survives; arithmetic on these text cells requires
explicit conversion, which can lose precision in the spreadsheet application.
The test opens the generated XLSX ZIP and checks cell references against their
exact shared strings, not just successful file creation. The shared value runner
picks up both workbook regressions automatically.

Excel documents its [15-digit precision limit](https://support.microsoft.com/en-us/excel/format-numbers-as-text).
Floating-point, temporal and nested-value spreadsheet contracts remain open.

Validation: 415 core tests, core/workspace Clippy, size guards and `cargo deny
check` passed. The full local gate passed at
`target/quality/20260926T195741317332Z-full/report.json` on the working tree
based on `49981d123`; Debian validation was skipped because `dpkg-deb` is
unavailable. The XLSX regression failed before the fix.

## PostgreSQL NUMERIC decoding checkpoint

The real-server regression first returned an undecodable marker for a 40-digit
NUMERIC. The decoder now reads the base-10000 wire groups directly and retains
the server scale. Exactly representable values remain Decimal; wider values,
longer fractions, NaN and infinities remain text. NULL stays separate.
Direct, bound and session queries are compared with PostgreSQL numeric-to-text
output, including positive/negative values, 1e1000 and 1e-1000. Unit cases
exercise maximum wire weight and scale, malformed lengths, invalid groups/signs,
and values whose nonzero digits would otherwise be truncated by scale.

This closes the scalar decode gap. Arbitrary-precision editing, numeric arrays
and every downstream consumer still need their own acceptance. The encoding
follows PostgreSQL [numeric_send](https://github.com/postgres/postgres/blob/REL_16_STABLE/src/backend/utils/adt/numeric.c).

Validation: all 45 PostgreSQL unit/integration tests passed. The full local gate
passed at `target/quality/20260926T202553866194Z-full/report.json` on the
working tree based on `28df4581c`; Debian package validation was skipped because
`dpkg-deb` is unavailable. No new dependencies were added.
The all-eight-driver suite also passed at
`target/quality/20260926T202754372916Z-values/report.json`: all 11 selected
suites, including both PostgreSQL cases and eight core cases.

## PostgreSQL array checkpoint

Scenario B3-1 follows the DBeaver array distinctions recorded in the
[external survey](b3-test-scenario-survey.md). The first real-server regression
failed on an empty int4 array, which returned Undecodable. No external fixture
or source code was copied.

The driver now decodes binary arrays of bool, int2/int4/int8, oid, text/name/
varchar/bpchar, float4/float8, numeric, UUID and bytea. Results use PostgreSQL array
text in Value::Text and retain the original array type in ColumnInfo. Whole-array
NULL remains Value::Null. Nested braces and explicit bounds preserve dimensions;
quoted elements distinguish NULL, literal NULL, empty strings and escaped text.
JSON export retains this text representation, not a flattened JSON array.

The regression compares PostgreSQL array_send bytes after SQL literal re-import,
explicitly typed parameter binding and execution of generated INSERT exports.
It also checks session/direct parity, exact column type and JSON string fidelity.
The corpus includes six dimensions, non-default lower bounds, Unicode, quotes,
backslashes, newline and SQL-shaped text, numeric scale/wide fractions, signed
zero, NaN/infinities, empty binary values and integer boundaries.

Unit tests reject truncated/trailing data, invalid dimensions or element lengths,
OID mismatches, invalid UTF-8 and NULL flags, unsupported element types and
size/product overflow. Deterministic malformed-input and header-mutation cases
exercise bounded decoding. Binary-to-text array decoding is capped at 16 MiB; exceeding the
limit returns the existing visible undecodable marker rather than truncated data.

Limits: enum/domain/composite/range, temporal and JSON/BSON array elements still
need explicit contracts; arbitrary array editing and the full grid/MCP/import
acceptance matrix remain open. Binding text in these tests uses an explicit
PostgreSQL array cast; this does not establish automatic array parameter typing.

Test locations: `crates/drivers/postgres/src/array.rs` and
`crates/drivers/postgres/tests/support/array_contract.rs`, invoked by the ignored
`value_contract_arrays_preserve_elements_dimensions_and_exports` integration test.
The shared value runner selects this test automatically.

Protocol references: [PostgreSQL arrays](https://www.postgresql.org/docs/16/arrays.html)
and [array_send](https://github.com/postgres/postgres/blob/REL_16_STABLE/src/backend/utils/adt/arrayfuncs.c).

Validation on the working tree based on `7cb2fe3ca`: all 50 PostgreSQL
unit/integration tests passed. The full local gate passed at
`target/quality/20260926T204949649377Z-full/report.json`; Debian package validation
was skipped because `dpkg-deb` is unavailable. The subsequent shared run caught
an overly literal metadata expectation: SQLx names PostgreSQL bpchar arrays
`CHAR[]`. After correcting that assertion, all 11 suites passed at
`target/quality/20260926T205412392759Z-values/report.json`, including all eight
drivers, GTK and DuckDB. That run reused 744 artifacts, rebuilt one package and
spent 3.034 seconds compiling. Function/file size guards and whitespace checks
also passed. Installed desktop acceptance remains a later gate.

## PostgreSQL time checkpoint

B3-2 adopts the temporal-boundary scenarios from the
[external survey](b3-test-scenario-survey.md). The new server regression first
failed because `24:00:00` was decoded as midnight: exported wire bytes changed
from `000000141dd76000` to `0000000000000000`.

Binary time decoding now retains ordinary times as typed values, preserves
`24:00:00` as exact text, and represents timetz as text with its stored signed
offset, including seconds. Column metadata retains TIME/TIMETZ. NULL remains
NULL. Invalid wire lengths, out-of-range times and offsets are refused.

Eleven cases run under UTC, Asia/Kathmandu and America/New_York, covering
microseconds, end-of-day and both offset extremes. Tests compare time_send or
timetz_send bytes after SQL literal re-import, explicitly typed bindings and
generated INSERT exports. Direct/session parity, type metadata and JSON text
fidelity are asserted. The permanent test is
`value_contract_times_preserve_midnight_fraction_and_offset`; malformed-wire and
boundary units live in `crates/drivers/postgres/src/temporal.rs`.

This checkpoint does not establish editing support for text-backed time values,
temporal arrays, BC/large-year dates, infinities, mixed intervals or every
grid/MCP/XLSX consumer. Those B3 cases remain open. Protocol references:
[date/time types](https://www.postgresql.org/docs/16/datatype-datetime.html) and
[time/timetz send and receive](https://github.com/postgres/postgres/blob/REL_16_STABLE/src/backend/utils/adt/date.c).

Fresh cargo-mutants 27.1.0 evidence, on the working tree based on `6de5351ee`:

| Scope | Result | Report beneath target/quality |
| --- | --- | --- |
| Array decode entry, scalar elements and float rendering, 26 generated mutations | 24 caught, 1 survivor, 1 unviable | `20260926-b3-array-mutants/mutants.out/outcomes.json` |
| Array survivor after including malformed-input unit tests | 1 caught, no survivors/timeouts | `20260926-b3-array-mutants-followup/mutants.out/outcomes.json` |
| Complete new time decoder, 22 generated mutations | 21 caught, 1 unviable, no survivors/timeouts | `20260926-b3-time-mutants/mutants.out/outcomes.json` |

The array survivor removed the NUL-byte guard. Its existing unit regression was
omitted by the mutation run's `value_contract` filter; array/numeric malformed
units now share that prefix. The rerun caught the mutation. Both unviable changes
attempted to construct a nonexistent default Value and failed compilation; they
are not test catches. These were scratch-source runs reusing the local target
directory, with no concurrent Cargo build. The time mutation run used unit tests;
array runs also used the real-server value contracts. This is scoped evidence,
not a complete array, numeric, workspace or hosted mutation pass.

Local validation: `target/quality/20260926T210713778702Z-full/report.json`
passed formatting, Clippy, unit and sandbox checks. Debian package validation
was skipped because `dpkg-deb` is unavailable. The stricter shared runner passed
all 11 selected suites at
`target/quality/20260926T210925595822Z-values/report.json`, including four
PostgreSQL server contracts and all eight drivers with GTK/DuckDB selected.
The complete PostgreSQL unit/integration run also passed all 53 tests.
Ten runner/workflow regression tests passed, including a later refinement that
rejects a mutation report containing only unviable changes. Workflow YAML parsed;
the mutation summary shell was executed against complete, missing, empty and
unviable-only fixture reports. Function/file size and whitespace guards passed.
Hosted mutation/coverage execution and installed desktop acceptance remain
separate from these local results.
