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

This is a growing contract, not proof of every database type. PostgreSQL wide
NUMERIC, native arrays, temporal extremes and intervals, nested BSON values,
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
