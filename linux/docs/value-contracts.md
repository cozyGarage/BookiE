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

The [type-contract strategy](type-contract-strategy.md) defines boundary families,
proof requirements and remaining driver targets. This status was reconciled through
`linux` commit `a90b86041` on 2026-09-28; it is an
inventory, not a fresh run of every suite. A passing scalar suite does not establish
complete native-type support.

| Path | Assertions |
| --- | --- |
| PostgreSQL, MySQL, SQLite, SQL Server, ClickHouse, DuckDB | Bound parameters and generated SQL preserve signed integer limits, values around 2^53, small/large floats, text and NULL |
| Six SQL drivers, mixed named bindings | Repeated names, signed integer limits, empty text versus NULL, SQL-like Unicode payloads and placeholder-looking text retain type, position and exact content |
| Fixed-decimal SQL engines | Positive and negative decimals retain their value through binding and generated SQL; SQLite has no fixed-decimal storage contract |
| Redis | Integer command replies and quoted text arguments retain their values; a missing key differs from empty text |
| MongoDB | JSON commands preserve integer, float, text and NULL values through BSON and result decoding |
| Grid, filter, CSV and named parameter parsers | Integer overflow and decimal rounding are refused; representable boundaries survive parsing |
| Float input parsers | Numeric overflow to infinity and nonzero underflow to zero are refused |
| XLSX | Integers beyond 15 digits and all exact decimals are text cells; stored XML verifies each value and cell reference, including decimal scale |
| JSON and MCP | Non-finite values and negative zero remain distinct from SQL NULL and positive zero |

## CSV negative-zero export/import, 2026-09-28

A focused core contract exports IEEE-754 negative zero as CSV, reads the result
back through the delimited-file reader and typed `DOUBLE PRECISION` cell parser,
then compares the imported float's bit pattern with the submitted value. The
independent `to_bits()` oracle distinguishes negative zero from positive zero.
The test passed; no production mismatch was found.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-core --lib value_contract_csv_round_trip_preserves_negative_zero_bits
```

## CSV signed integer boundary export/import

The typed `BIGINT` CSV round trip covers `i64::MIN`, `i64::MAX`, and
`9007199254740993` (one above the exact-integer limit of binary64). It checks
both the emitted decimal digits and the exact imported `Value::Int` values.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-core --lib value_contract_csv_round_trip_preserves_signed_integer_boundaries
```

The focused core test passed.

## CSV decimal scale export/import

A typed `DECIMAL(10,4)` CSV round trip preserves `12.3000` in the emitted
token and in the imported `Decimal` scale. The assertions compare the text
representation because numeric equality alone would treat `12.30` and
`12.3000` as equivalent.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-core --lib value_contract_csv_round_trip_preserves_decimal_trailing_zeroes
```

The focused core test passed.

## JSON negative-zero consumer contract

The JSON exporter writes finite negative zero as the numeric token `-0.0`.
Parsing the exported document recovers the original IEEE-754 negative-zero
bits, while positive zero remains positive and SQL NULL remains JSON null.
This is a JSON writer and parser contract; it does not establish all JSON
number bit patterns.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-core --lib value_contract_json_keeps_negative_zero_distinct_from_positive_zero_and_null
```

The focused core test passed.

A second typed CSV contract carries `NaN`, positive infinity, negative infinity
and NULL through default CSV export and `DOUBLE PRECISION` import. It checks the
exact emitted tokens, the imported NaN/infinity classifications and NULL variant.
The regression passed; no production mismatch was found.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-core --lib value_contract_csv_round_trip_keeps_nonfinite_floats_distinct_from_null
```

The default CSV export and typed `DOUBLE PRECISION` import also preserve the
smallest positive subnormal (`f64::from_bits(1)`) exactly. The importer result is
compared by IEEE-754 bits; the focused regression passes with no production
mismatch. The largest finite `f64` also survives the default CSV export and
typed import with its exact bit pattern. Other finite float bit patterns and
spreadsheet-application import remain open.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-core --lib value_contract_csv_round_trip_preserves_smallest_subnormal_bits
```

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-core --lib value_contract_csv_round_trip_preserves_largest_finite_float_bits
```

A typed `TIMESTAMP WITH TIME ZONE` CSV contract starts with a timestamp at
`+05:30` and nine fractional digits. Default export normalizes it to the exact
UTC token, and typed import reproduces the identical UTC instant. This closes
one nanosecond/timezone consumer cell; broader cross-format temporal parity
remains open.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-core --lib value_contract_csv_round_trip_preserves_timestamptz_nanoseconds
```

ClickHouse long-value reads are checked with a server-generated value. Oversized
inline SQL is required to return its explicit query-size error; the fixture does
not raise that server limit or accept a truncated success.

## PostgreSQL script consumer agreement, 2026-09-28

A local policy integration contract uses CRLF line endings and issue-shaped
comments before, between, inside and after two statements. The comments and a
quoted literal contain semicolons and placeholder-shaped text. The planner must
return exactly the SELECT then UPDATE; parameter extraction must retain only
`shown`, then `name` and `id`; formatting must preserve comment text and leave
the same executable statements in order. Policy classification must still see
SELECT then UPDATE, including the UPDATE target and WHERE clause. This verifies
consumer agreement for this PostgreSQL boundary without a live database; it
does not close the other dialects or delimiter edge cases.

Focused result:

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-policy --test administrative_dialects postgres_script_consumers_agree_across_crlf_comments_and_formatting -- --exact
```

The test passed. No production defect was exposed.

The editor also converts GTK's character offset to a UTF-8 byte offset before
asking the planner for a statement. A focused app regression places `東京` in
the first statement and positions the cursor at the start of the second; it
asserts the mapped byte offset and that the editor selects `SELECT 2`. The
conversion was already present inline, so this closes an evidence gap rather
than correcting a behavior mismatch.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib gtk_character_offset_after_multibyte_text_selects_the_following_statement -- --test-threads=1
```

The test passed.

A second app regression passes `SELECT :safe; SELECT 'unfinished :tail` through
the planner, execution splitter, parameter extractor, formatter and policy. The
planner reports an unterminated quote; the app rejects the entire script rather
than returning its valid prefix. Parameter extraction converts only `:safe`
and leaves the malformed suffix intact, the formatter retains that suffix, and
policy classifies the whole input as unparseable and write-capable. This closes
the malformed-tail contract for PostgreSQL only.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib malformed_tail_is_not_accepted_as_a_valid_script_prefix -- --test-threads=1
```

The test passed. No production defect was exposed.

## SQL Server GO consumer agreement, 2026-09-28

An app regression sends three statements through a SQL Server script with two
`GO` batch separators. Placeholder-shaped text and semicolons in each `GO`
comment must not become parameters. The planner and app splitter retain three
statements and `ContinueNextBatch`; the formatter preserves both separators and
comments; named-parameter extraction returns only `read`, `name`, `id` and
`last`; policy classification keeps SELECT/UPDATE/SELECT order and the UPDATE
WHERE fact. The contract uses the shared lexer and formatter without a live
SQL Server. Non-default MySQL delimiter forms remain open.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib mssql_go_batches_keep_consumer_order_and_ignore_delimiter_comments -- --test-threads=1
```

The test passed. No production defect was exposed.

## SQL Server repeated GO count refusal, 2026-09-28

The app contract feeds `GO 2` through the SQL Server script consumers. The plan
retains the repeat count, but execution planning refuses to run it instead of
silently running the batch once. Formatting keeps the directive and its comment;
parameter extraction returns only the SQL parameter, and policy classifies the
full repeated script as unparseable and write-capable. This proves safe refusal,
not repeated-batch execution support.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib mssql_go_repetition_is_preserved_and_refused_by_script_execution
```

The test passed; no production defect was found.

## SQL Server `datetime2(7)` 100-nanosecond result precision

The existing temporal export fixture now checks its `datetime2(7)` result
directly: `2024-01-02 03:04:05.1234567` decodes to a `NaiveDateTime` with
`123456700` nanoseconds, matching the independent server text from
`CONVERT(varchar(27), precise, 126)`. The same fixture exports its temporal rows
as SQL literals and confirms source/export equality on the server. This verifies
the seventh fractional digit through decoding and SQL export; it does not claim
coverage for all SQL Server temporal edge cases.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mssql --test integration temporal_sql_exports_round_trip_legacy_and_high_precision_columns -- --include-ignored --exact --test-threads=1
```

The Docker-backed test passed; no production mismatch was found.

### SQL Server `smalldatetime` rounding threshold

A focused server test checks values at the `smalldatetime` second-rounding
boundary. `03:04:29.998` decodes and renders as `03:04:00`; `03:04:29.999`
rounds to `03:05:00`. Each decoded `DateTime` is checked beside SQL Server's
independent style-126 text value. The Docker test passed with no production
mismatch; broader legacy temporal boundaries remain open.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mssql --test integration value_contract_smalldatetime_rounding_matches_server_text -- --include-ignored --exact --test-threads=1
```

### SQL Server legacy `datetime` tick boundaries

A SQL Server Docker contract checks legacy `datetime`
rounding around its 1/300-second tick boundaries (`.001`, `.002`, `.004`,
`.005`, `.008`). The decoded nanoseconds and SQL Server's independent
millisecond text agree on the rounded values. The `2/300` tick maps to
6,666,666 nanoseconds in chrono, one nanosecond below the rational instant;
this is the current `NaiveDateTime` representation ceiling and remains an
explicit exactness limitation. The test passed without a production change.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mssql --test integration value_contract_legacy_datetime_rounding_matches_server_milliseconds -- --include-ignored --exact --test-threads=1
```

## SQL Server money float-decoding refusal

A Docker regression first reproduced SQL Server `money` as
`Float(123456789012345.67)` even though an independent server-side decimal text
cast returned `123456789012345.6789`. `smallmoney` also travels through the
TDS client's floating-point representation. Both native money types now return
an `Undecodable("money")` marker for non-NULL values, while NULL remains NULL;
the result's separate decimal text oracles preserve the server's exact values.
This is a safe refusal, not exact `money` editing or binding support.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mssql --lib money_columns_refuse_float_decoding_but_preserve_null
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mssql --test integration money_types_refuse_values_decoded_through_binary_float -- --include-ignored --exact --test-threads=1
```

Both focused tests passed; the integration reproducer fails before the refusal fix.

### SQL Server `sql_variant` result blocker

A Docker probe selected a `sql_variant` containing `bigint` value
`9007199254740993`, alongside `SQL_VARIANT_PROPERTY(..., 'BaseType')` and an
exact `CONVERT(varchar(40), ...)` oracle. The request panicked before the driver
could decode or refuse the result: the pinned Tiberius TDS metadata parser has
an unimplemented `SSVariant` branch. The probe is not retained as a passing
regression because running this query crashes the client; `sql_variant` remains
an open driver/dependency defect. A future fix must make metadata decoding safe,
then assert the bigint identity and server text without passing through a float.

## MySQL DELIMITER consumer agreement, 2026-09-28

An app regression uses `DELIMITER $$` around a stored procedure, resets the
delimiter to `;`, then runs a parameterized SELECT. The planner and editor
splitter return the procedure followed by the SELECT and omit client directives
from executable SQL. Formatting retains both directives. Parameter extraction
ignores placeholder-shaped text in directive comments and in the procedure's
quoted literal, retaining only `after` from the SELECT. The policy parser marks
the procedure statement unparseable and write-capable, then classifies the
following query as SELECT; agent policy denies the unparseable procedure. This
verifies client-side delimiter handling, not that stored-procedure definitions
are executable through BookiE. Human handling of unparseable SQL remains
policy-config dependent.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib mysql_delimiter_directives_agree_across_script_consumers -- --test-threads=1
```

The test passed. No production defect was exposed.

Text cases include empty strings, numeric-looking strings, Unicode, apostrophes,
quotes, backslashes, line breaks and text beyond 256 KiB. Float assertions in the
SQL harness compare bit patterns. Existing driver tests still cover binary
exports, typed row identity, cancellation, metadata and other engine behavior.
The new suite supplements those tests.

This is a growing contract, not proof of every database type. PostgreSQL's other
unconstrained numeric edit boundaries and special numerics, JSON and other unsupported array element
types, array grid write-back beyond the verified built-in `integer[]` case,
finite calendars beyond the shared range, and interval consumer parity still need
focused cases. ClickHouse Int128/UInt128
now have local parser and real-server exact-text result, binding and SQL export/import contracts at signed and unsigned boundaries, plus Int128 and UInt128 grid-edit contracts.
Installed GTK/package SQLite grid acceptance, spreadsheet floating-point edges and
remaining transport/persistence adapters also need focused cases. Add a reproducer
before changing a decoder or parser. Never make
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
Spreadsheet import and spreadsheet-application re-import contracts remain open.

### XLSX negative-zero cell token, 2026-09-28

The XLSX writer stores finite floats as numeric cells. A focused workbook
regression checks the generated worksheet XML directly and confirms that
negative zero is serialized as the numeric token `-0`, not positive `0` or a
string. Another regression parses the worksheet's numeric token for the smallest
positive subnormal and the representable value immediately above `1.0`, then
compares the recovered `f64` bits with independent expected values. All three
focused float workbook tests pass. BookiE has no XLSX import path, so these
contracts establish exported numeric tokens and their IEEE-754 parse-back, not
later spreadsheet-application round-trip behavior.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-core --lib value_contract_workbook_float
```

No production mismatch was found.

### XLSX nested Extended JSON consumer check, 2026-09-27

The workbook regression writes a nested value containing Decimal128, binary
subtype and millisecond-date Extended JSON markers. It inspects the generated
XLSX shared-string cell and confirms the markers remain in one exact text cell.
The focused regression and all 436 core library tests passed. This verifies the
shared XLSX writer boundary; it does not yet run MongoDB values through the
application's complete query-to-workbook path.

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

## PostgreSQL wide NUMERIC grid edit

The keyed-update builder binds a text-backed PostgreSQL `NUMERIC` edit through
`text` to the fixed `pg_catalog.numeric` type. Its unit contract covers a
`NUMERIC(80,40)` value with 40 integer digits and 40 fractional digits, beyond the
shared Decimal representation. Type metadata is matched against a numeric-only
grammar and never copied into SQL; ordinary text columns keep their existing
placeholder. The real-server contract compares the saved value with PostgreSQL's
own `numeric::text` output and passed against the Docker PostgreSQL fixture.

The focused core tests passed:

```sh
cargo test --locked -p tablepro-core --lib postgres_numeric -- --test-threads=1
cargo test --locked -p tablepro-core --lib postgres_wide_numeric -- --test-threads=1
```

The mapped change-contract gate also passed its four keyed-update regressions at
`target/quality/20260928T121821319834Z-change-contracts/report.json`.

The server test is discoverable as
`wide_numeric_contract::value_contract_wide_numeric_grid_edit_preserves_exact_value`.
The first run exposed two fixture mismatches: the temporary table was invisible to
pooled connections, and browse primary-key metadata was absent. After fixing those,
the server equality assertion passed. The `NUMERIC(1000,1000)` boundary contract
also passed against PostgreSQL, comparing the saved value to the server's
`numeric::text` output after a keyed grid update:

```sh
cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration wide_numeric_contract::value_contract_max_precision_numeric_grid_edit_preserves_exact_value -- --include-ignored --exact --test-threads=1
```

An app-parser regression then reproduced that the same `NUMERIC(80,40)` and
`NUMERIC(1000,1000)` inputs failed as `Invalid decimal` before reaching the grid
update builder. The PostgreSQL-only parser now retains a grammar-checked numeric
literal as exact text when `rust_decimal` cannot represent it; representable
values still use `Value::Decimal`. Its `NUMERIC(80,40)` case passes the parsed
value through the keyed-update builder and verifies the static
`$1::text::pg_catalog.numeric` cast. It rejects malformed input, and MySQL's
wide-numeric parser behavior is unchanged. Local app tests pass. PostgreSQL's
Docker composition test now uses the real app parser output, keyed-update
builder and PostgreSQL driver against both `NUMERIC(80,40)` and unconstrained
`numeric`, then compares each saved value to the server's independent
`numeric::text` output. The unconstrained case uses a 40-digit integer and
40-digit fraction, outside `rust_decimal` precision. The same app test sends an
unconstrained numeric at PostgreSQL's maximum fractional scale of 16,383 digits
and at its maximum integer width of 131,072 digits through the parser and keyed-
update builder. The server confirms exact `numeric::text` values, the fractional
scale, and the integer text length. Both unconstrained numeric width boundaries
are covered; other text-backed PostgreSQL types remain open. See
[PostgreSQL numeric type limits](https://www.postgresql.org/docs/16/datatype-numeric.html).

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib postgres_numeric_parser_outputs_round_trip_through_server -- --include-ignored --test-threads=1
```

The test passed against the PostgreSQL 16 Docker fixture.

A separate PostgreSQL 16 contract covers a custom domain over `numeric`. The
40-digit integer/20-digit fraction remains exact in the domain result and the
server's `numeric::text` oracle; SQL-literal re-import and an explicitly typed
text binding also return the same value. This establishes the numeric-domain
cell, not broad domain, enum, composite or range support.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration wide_numeric_contract::value_contract_numeric_domain_preserves_wide_value_across_consumers -- --include-ignored --exact --test-threads=1
```

The focused PostgreSQL Docker test passed.

A second domain contract uses PostgreSQL's `uuid` base type. The direct domain
result remains `Value::Uuid`; an independent `uuid::text` value and
`pg_typeof` name confirm the native value/type. SQL-literal re-import and a
typed UUID parameter also produce the exact text. This closes one non-numeric
domain consumer case only.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration value_contract_uuid_domain_preserves_uuid_across_consumers -- --include-ignored --exact --test-threads=1
```

The PostgreSQL 16 Docker contract passed.

### PostgreSQL `int4range` explicit refusal

The PostgreSQL 16 contract checks a native `[1,5)` range against the server's
exact `int4range::text`, lower/upper bounds, and endpoint-inclusion functions.
The driver returns `Undecodable`; SQL-literal rendering and parameter binding
refuse that value rather than flattening range semantics into text.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration value_contract_int4range_is_explicitly_unsupported -- --include-ignored --exact --test-threads=1
```

The focused PostgreSQL Docker contract passed. This is an explicit refusal
boundary, not support for PostgreSQL range values.

### PostgreSQL composite explicit refusal

A PostgreSQL 16 fixture defines `(id bigint, label text)` and returns a value
containing `9007199254740993` and `東京`. PostgreSQL's `row_to_json`, type name,
and individual field expressions provide exact independent oracles. The
composite result is `Undecodable`; SQL-literal rendering and parameter binding
refuse it instead of flattening its structure.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration value_contract_composite_result_is_explicitly_unsupported -- --include-ignored --exact --test-threads=1
```

The focused PostgreSQL Docker contract passed. This does not add composite
support.

### PostgreSQL temporal range explicit refusal

The PostgreSQL 16 contract returns a `tstzrange` spanning two fractional-second
instants supplied at `+02`, with UTC fixed for independent text oracles. The
driver marks the range `Undecodable`; server `tstzrange::text`, lower/upper
`timestamptz::text`, type name, and endpoint-inclusion functions confirm the
stored bounds and `[)` semantics. SQL-literal rendering and parameter binding
refuse the result. This adds one temporal range refusal boundary and does not
add PostgreSQL range support.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration value_contract_tstzrange_is_explicitly_unsupported -- --include-ignored --exact --test-threads=1
```

The focused PostgreSQL Docker contract passed.

### PostgreSQL scalar enum label text projection

A PostgreSQL 16 contract defines labels `NULL`, `東京`, and `o'brien`. For
each label, PostgreSQL's `pg_typeof` confirms the source expression remains the
custom enum while `enum::text` returns the exact label; SQL-literal re-import
and an explicitly typed text parameter also preserve it. SQL NULL is checked
separately from the literal label `NULL`. Directly returning the enum-typed
column currently fails during SQLx type metadata resolution (`enum_labels`:
unexpected NULL), so direct enum result decoding remains open; this contract
does not claim it is supported.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration value_contract_scalar_enum_labels_preserve_exact_text -- --include-ignored --exact --test-threads=1
```

The text-projection and consumer round-trip contract passed against PostgreSQL
16. The direct enum projection attempt failed before assertions in SQLx metadata
resolution and is recorded as an open boundary, not a passing result contract.

### PostgreSQL custom enum array metadata boundary

A PostgreSQL 16 fixture defines enum labels `NULL`, `東京`, and `o'brien`, then
builds an array containing all three labels plus SQL NULL. Server-side scalar
projections confirm the exact native array type, PostgreSQL array text, and
JSON semantics. Selecting the enum array itself fails during SQLx metadata
resolution (`enum_labels`: unexpected NULL); it does not produce a BookiE
`Undecodable` value. This records an open decoder/dependency boundary rather
than claiming unsupported-value handling.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration array_contract::value_contract_custom_enum_array_projection_is_rejected -- --include-ignored --exact --test-threads=1
```

The PostgreSQL 16 contract passed: independent server projections succeeded,
and direct enum-array projection returned the expected metadata error.

### PostgreSQL money safe refusal

A PostgreSQL 16 contract returns a native `money` value with a separate exact
`money::numeric::text` oracle and server equality check. The driver reports
non-NULL money as `Undecodable`, SQL-literal and parameter consumers refuse the
marker, and SQL NULL remains `Value::Null`. This is an explicit safety boundary,
not exact money support.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration value_contract_postgres_money_is_refused_with_exact_server_oracle -- --include-ignored --exact --test-threads=1
```

The PostgreSQL 16 Docker contract passed.

### PostgreSQL `citext` text consumer contract

The PostgreSQL 16 fixture creates the `citext` extension and checks that a
mixed-case address remains byte-for-byte text through result decoding, SQL
literal re-import and a typed parameter. A separate comparison against the
lowercase spelling confirms PostgreSQL still applies `citext`'s
case-insensitive comparison semantics. This establishes the label's text
fidelity without claiming that `citext` semantics are represented by `Value`.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration value_contract_citext_preserves_label_and_case_insensitive_comparison -- --include-ignored --exact --test-threads=1
```

The focused Docker contract passed.

### PostgreSQL keyed-update cast and parser mutation checkpoint, 2026-09-28

The shared keyed-update builder's final focused mutation report selected 16
mutations: 14 were caught, two were unviable compilation changes, and none
survived or timed out. A surviving `&& -> ||` mutation had shown that a typed
`Value::Decimal` could incorrectly receive the text cast. The new unit regression
requires typed numeric binds to keep `$1` and their native decimal parameter;
the same core suite also checks safe metadata handling at and beyond the 64-byte
allowlist input limit.

The app-parser check adds malformed-literal refusals to the wide numeric parser
contract. Its final `--iterate` report covers the 18 remaining mutations: seven
were caught, nine survived, and two cursor-increment mutations timed out. The
remaining survivors are in `is_postgres_numeric_literal` (cursor comparison,
fraction/exponent grammar, and cursor advancement); they are recorded as open
parser-test coverage, not as confirmed production defects. The timeout mutants
replace cursor increments with multiplication and can stall the scanner. No
source behavior was changed in response to these parser survivors.

```sh
rtk cargo mutants --dir . --package tablepro-core --file crates/core/src/sql_dialect.rs --re 'build_keyed_update|postgres_numeric_cast_type' --test-tool cargo --timeout 30 --build-timeout 120 --output target/quality/20260928-pg-keyed-update-cast-final -- --lib
rtk cargo mutants --dir . --package tablepro-app --file crates/app/src/ui/browse_tab/value_parse.rs --re 'parse_input_for_driver|is_postgres_numeric_type|is_postgres_numeric_literal' --test-tool cargo --timeout 30 --build-timeout 120 --iterate --output target/quality/20260928-pg-numeric-parser-mutants -- --lib postgres_
```

Reports: `target/quality/20260928-pg-keyed-update-cast-final/mutants.out/outcomes.json`
and `target/quality/20260928-pg-numeric-parser-mutants/mutants.out/outcomes.json`.
The app parser unit regression passed. The PostgreSQL Docker acceptance contracts
remain a separate server-side check.

### PostgreSQL NUMERIC special-value grid edit

The app's regular decimal parser rejects `NaN`, `Infinity`, and `-Infinity`.
Inline editing now keeps those three spellings as `Value::Text` only for the
PostgreSQL driver and decimal/numeric columns. The existing keyed-update builder
binds the text through `text` to the static `pg_catalog.numeric` target. The app
parser regression and PostgreSQL 16 integration case pass. The integration updates
one numeric column with each value and compares both decoded results and stored
values with the server's independent `amount::text` oracle. The app parser to
PostgreSQL composition test also parses each special spelling, applies the
keyed grid update, then compares the decoded value and `amount::text` result.

Focused checks:

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib postgres_numeric_specials_remain_exact_text_only_for_postgres -- --test-threads=1
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration wide_numeric_contract::value_contract_numeric_special_grid_edits_match_server_text -- --include-ignored --exact --test-threads=1
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib postgres_numeric_parser_outputs_round_trip_through_server -- --include-ignored --test-threads=1
```

The composed parser-to-server test passed against the PostgreSQL 16 Docker fixture.

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

Limits: enum/domain/composite/range and most JSON/BSON array element contracts
remain unsupported or untested; automatic array editing and the full grid/MCP/import acceptance matrix
remain open. Binding text in these tests uses an explicit
PostgreSQL array cast; this does not establish automatic array parameter typing.

Test locations: `crates/drivers/postgres/src/array.rs` and
`crates/drivers/postgres/tests/support/array_contract.rs`, invoked by the ignored
`value_contract_arrays_preserve_elements_dimensions_and_exports` integration test.
The shared value runner selects this test automatically.

The 2026-09-28 P1 reconciliation added a Docker-backed acceptance test in
`crates/drivers/postgres/tests/support/array_contract.rs`,
`array_contract::value_contract_array_grid_edit_preserves_array_elements`, for an
`integer[]` grid edit containing NULL. With Docker access, it reproduced SQLSTATE
42804: the generated UPDATE assigned a TEXT parameter to an `integer[]` column.
The shared keyed-update builder now casts text through `text` to a fixed,
allowlisted PostgreSQL built-in array type. Database metadata is never interpolated
into SQL. PostgreSQL verified the edited value with a typed array comparison;
NULL remained an array NULL element.

The focused core cast and hostile-metadata tests passed. The Docker-backed
PostgreSQL regression passed after the fix:

```sh
cargo test --locked -p tablepro-core --lib postgres_array_update
cargo test --locked -p tablepro-driver-postgres --test integration array_contract::value_contract_array_grid_edit_preserves_array_elements -- --include-ignored --exact --test-threads=1
```

The expanded server test adds a `text[]` edit through the unchanged app text
parser and shared `text` to `pg_catalog.text[]` cast. Its value includes NULL and an
element containing a quote, backslash and comma. An independent `unnest` query
with ordinality checks each element and a separate NULL flag. The app
parser-to-builder unit, focused core cast test, and expanded integration test
passed against PostgreSQL.

Exact grid editing is server-verified for `integer[]` and `text[]`. JSON and other
unsupported element OIDs, custom/user-defined arrays
and automatic array parameter typing remain outside the tested support surface.
B3 remains open.

A separate `numeric[]` grid contract now feeds an exact literal containing a
wide numeric with 20 fractional digits, a scale-preserving `1.2300`, NaN,
positive and negative Infinity, and a NULL element through the shared numeric[]
cast. Its independent server oracle unnests with ordinality, checks NULL
separately, and compares every element as `numeric::text`. The app
parser-to-builder unit, PostgreSQL scalar numeric decoder unit, escaped-text
array unit, and Docker-backed numeric[] grid assertion passed against PostgreSQL.

The `jsonb[]` refusal contract checks a native JSONB array containing an object
and JSON null. PostgreSQL independently reports the `jsonb[]` type, its exact
`array_to_json(... )::text`, and semantic JSONB equality. BookiE returns an
`Undecodable` marker and refuses both SQL literal rendering and parameter
binding. This records a precise unsupported boundary; it does not add JSON-array
support.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration array_contract::value_contract_json_array_elements_are_explicitly_unsupported -- --include-ignored --exact --test-threads=1
```

The PostgreSQL 16 Docker contract passed.

The neighboring `json[]` contract uses the same refusal boundary with a native
JSON array containing an object and JSON `null`. The server reports `json[]`,
returns exact `array_to_json` text, and independently confirms semantic
equality after conversion to `jsonb`. The driver returns `Undecodable`; SQL
literal rendering and parameter binding refuse it. JSON and JSONB array support
is not claimed.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration array_contract::value_contract_json_text_array_is_explicitly_unsupported -- --include-ignored --exact --test-threads=1
```

The PostgreSQL 16 Docker contract passed.

## ClickHouse wide integer parser contract, 2026-09-28

ClickHouse `Int128` and `UInt128` values cannot fit `Value::Int`. A local
regression feeds the unquoted JSON integer tokens for signed minimum, signed
maximum and unsigned maximum through the same `parse_line` and `response_row`
functions used by the driver. All three remain exact `Value::Text`. This verifies
the parser boundary with `serde_json`'s `arbitrary_precision` feature enabled.
The real-server query contract below separately checks native result decoding.

The focused unit command passed, and the mapped change-contract gate passed all
four ClickHouse response regressions. Its report is
`target/quality/20260928T113903658583Z-change-contracts/report.json`.

```sh
cargo test --locked -p tablepro-driver-clickhouse --lib clickhouse_json_row_preserves_wide_integer_tokens_exactly
```

Protocol references: [PostgreSQL arrays](https://www.postgresql.org/docs/16/arrays.html)
and [array_send](https://github.com/postgres/postgres/blob/REL_16_STABLE/src/backend/utils/adt/arrayfuncs.c).

### ClickHouse wide integer result, binding and SQL re-import, 2026-09-28

The signed Int128 minimum and maximum and unsigned UInt128 maximum pass through
text-valued bound parameters into native Int128/UInt128 columns and return as
exact `Value::Text`. The result row is written with the shared generated INSERT
literal, executed into matching native columns, and read back as the exact
original text. The same real-server result passes through CSV export, the shared
CSV reader and typed import cell parser both with formula sanitization disabled
and with default formula sanitization. The default-mode import is bound into a
second native Int128/UInt128 table and read back from ClickHouse to verify the
stored digits. Default export prefixes the negative
Int128 with an apostrophe; import removes that marker only when the column type
is exactly Int128 or UInt128 and the remaining signed digits fit that type.
Ordinary text, invalid values, unsigned negatives and overflow retain the
apostrophe. This preserves formula protection for other text columns while the
three wide integer fields re-import as their exact original text. The first
import attempt exposed two issues: `Int128` was classified as i64, then the
formula-safe apostrophe remained in text-backed wide integers. Both import paths
are now covered.

```sh
cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-clickhouse --test integration wide_integer_binding_and_sql_export_preserve_exact_server_values -- --include-ignored --exact --test-threads=1
```

The CSV round trip covers this wide-integer result path with formula
sanitization both disabled and enabled. The grid contract also covers the four
adjacent wide-integer boundaries:
Int128 minimum to minimum+1, Int128 maximum to maximum-1, UInt128 zero to one,
and UInt128 maximum to maximum-1. Other export formats remain open.

### ClickHouse Int128 and UInt128 grid edits, 2026-09-28

A real-server keyed update changes both rows through the same shared builder
used by the grid save path. It moves Int128 minimum to minimum+1 and UInt128
maximum to maximum-1, then Int128 maximum to maximum-1 and UInt128 zero to one.
The independent result query compares all four exact text values and verifies
the non-target row remains unchanged after each edit.

```sh
cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-clickhouse --test integration wide_integer_grid_edits_preserve_exact_values_and_row_identity -- --include-ignored --exact --test-threads=1
```

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

## PostgreSQL era and mutation checkpoint

The BC regression reproduced an export failure: `0001-01-01 BC` became the
invalid PostgreSQL literal `0000-01-01`. PostgreSQL SQL rendering now uses era
years and a BC suffix, and removes the ISO leading plus for years above 9999.
Other dialects and ordinary AD formatting retain their existing contracts.
Fifteen server cases run in three session time zones, covering BC dates,
large representable years, microseconds, the PostgreSQL epoch boundary and both
instants of a repeated DST hour. Direct/session values, type metadata, typed
bindings, SQL literals and generated INSERTs are compared with server wire bytes.
The integration test is `value_contract_dates_preserve_eras_large_years_and_instants`;
its lowest-tier regression is in core's SQL literal tests. Dates outside chrono's
range, infinities, mixed intervals, temporal arrays and non-SQL consumer/editing
acceptance remain open.

The [CI audit](ci-audit-2026-09-27.md) found nine survivors in the broader hosted
PostgreSQL mutation run. New regressions use an independent 16,777,216-byte limit,
test exact quoted-output and raw-element boundaries, check dimension rejection
before decoding, reject fractional groups beyond declared scale and preserve
one-digit fractions and noncanonical numeric text. Bounded element reading is
now a directly testable helper; the size policy is unchanged. No mutation was
blanket-excluded.

Local cargo-mutants 27.1.0 evidence, working tree based on `c57ac7715`:

| Scope | Result | Report beneath target/quality |
| --- | --- | --- |
| Changed SQL date renderer | 10 caught, 1 unviable, no survivors/timeouts | `20260926-b3-date-mutants/mutants.out/outcomes.json` |
| Array/numeric functions implicated by hosted survivors | 58 caught, 1 additional unit-only survivor, 3 unviable | `20260927-b3-pg-survivors/mutants.out/outcomes.json` |
| Numeric type survivor after independent-oracle correction | 1 caught, no survivors/timeouts | `20260927-b3-numeric-type-followup/mutants.out/outcomes.json` |

The additional survivor changed normal numerics from Decimal to Text. The unit
test reused decode_text as its own expected-value oracle; it now asserts the
independent value, variant and decimal text. The server contract already covered
this type distinction. Unviable mutations are compilation failures, not catches.
Diff-scoped mutation listing required workspace-relative paths and explicit
`--src-prefix=a/ --dst-prefix=b/` because the local Git mnemonic prefixes produced
an empty selection. The list was checked before executing the 11 date mutations.

Final local validation for this checkpoint: formatting, Clippy, unit and sandbox
checks passed at `target/quality/20260926T231119112831Z-full/report.json`.
All 11 selected value-contract suites passed at
`target/quality/20260926T231558864082Z-values/report.json`, including all eight
drivers, GTK, DuckDB, nine core contracts and five PostgreSQL server contracts.
Compilation reused 707 artifacts and rebuilt 17 affected packages in 39.688s.
All eight SSH fixture tests and 15 runner/workflow regression tests passed.
The complete PostgreSQL unit/integration suite passed all 55 tests with ignored
fixture tests explicitly enabled (`--include-ignored --test-threads=1`).
Hosted execution, the broader core mutation backlog and installed desktop
acceptance remain separate gates.

## XLSX temporal consumer checkpoint, 2026-09-27

Workbook XML regressions reproduced that temporal values were forced into
spreadsheet serial cells: fractional seconds were displayed with whole-second
formats, timezone-bearing timestamps lost their explicit UTC identity, and
dates outside the native spreadsheet range lacked a text fallback. Exports now
write fractional Time/DateTime values, every TimestampTz and dates/timestamps
outside 1900–9999 as exact text using the existing canonical export spelling.
Supported dates and whole-second values retain native cells. Text timestamps
include their UTC offset; the two distinct instants in a repeated DST hour stay
distinct. This intentionally trades spreadsheet date arithmetic for exact text
when the native cell cannot carry the contract.

A timestamp originating at `+05:30` with nine fractional digits is also checked
in the XLSX shared string. The emitted canonical UTC text is
`2026-09-27T07:04:56.123456789+00:00`, preserving the exact instant across the
offset conversion; the corresponding CSV typed-import contract independently
checks the same instant.

Three added unit contracts inspect ZIP worksheet/shared-string XML: temporal
fallbacks, native boundary dates/whole-second cells, and finite/nonfinite floats
versus NULL. The temporal test failed before the fix. Eight XLSX unit tests pass.
This does not establish every IEEE floating-point rounding behavior, arbitrary
precision editing, temporal arrays or all remaining consumer/driver types.

Cargo-mutants selected 15 mutations in the complete XLSX cell writer. All 15 were
caught, with no missed, timeout or unviable outcomes. Evidence:
`target/quality/20260927-b3-xlsx-temporal-mutants/mutants.out/outcomes.json`.
This includes both finite-float guard survivors from hosted run `36280114335`;
row/column error-path and other export survivors remain separate triage work.

The shared eight-driver runner passed all 11 suites, including 12 core value
contracts, at `target/quality/20260927T091952740079Z-values/report.json`.
All 19 installed release-build UI scenarios passed and wrote successful result
JSON files under `target/quality/20260927-b3-xlsx-ui/`. The layer report
`target/quality/20260927T091941354836Z-layers/report.json` intentionally remains
failed: its initial full gate caught two unnecessary clones in the new tests,
while the independent value/UI layers passed. The clones were corrected rather
than suppressing Clippy. Thirty Python harness tests passed, including both
successful and failed GTK artifact-production regressions.
The corrected full gate passed at
`target/quality/20260927T092738833389Z-layers/report.json` (formatting, Clippy,
workspace unit/binary tests and sandbox regressions). Hosted execution and
installed Wayland/package acceptance remain separate from these local checks.

## Workbook UI and failure-boundary checkpoint, 2026-09-27

Three default installed-GTK scenarios exercise keyboard format selection and the
real save chooser: exact typed SQLite export, save cancellation, and visible
empty-text refusal. The XML oracle checks wide signed integers, nanosecond
precision, dates outside 1900–9999, native numeric dates/times, NULL, infinity,
formula-like Unicode text and the 100-row page boundary. Every scenario compares
all source rows before and after. Four Python tests cover oracle failures,
formula rejection and default scenario registration.

The expanded UI corpus found a new defect: rust_xlsxwriter discards empty strings,
so successful workbook export silently collapsed empty text into missing cells.
The initial combined report `target/quality/20260927T094125010168Z-layers/report.json`
therefore remains failed, with full/value layers passed and the UI assertion
failing on the missing cell. A core regression also failed before correction.
Workbook export now refuses empty text with its one-based data row/column and
CSV/JSON alternatives. The dedicated UI scenario requires that visible error,
no published file and no temporary export. A public core regression separately
requires existing destination bytes to survive this refusal. This is an explicit
format limitation, not a claim that XLSX now round-trips empty text.

Coordinate regressions also reproduced accepting column 16,385 in the converter.
The writer now checks the actual worksheet limits before narrowing coordinates,
reserves the header row, and saturates diagnostic counts instead of overflowing.
Public file tests cover column-limit refusal and cancellation after a row has
been written; both leave existing destination bytes intact and no temporary file.

Final local evidence for this checkpoint:

- `target/quality/20260927T095051662483Z-layers/report.json`: full, values, UI and
  harness layers all passed; 34 Python harness tests passed.
- `target/quality/20260927T095333331615Z-values/report.json`: all 11 suites passed,
  including eight drivers, GTK, DuckDB and 16 core value contracts.
- `target/quality/20260927-b3-workbook-ui-corrected/`: all 22 installed release
  scenarios passed, with 22 result JSON files and 22 stderr artifacts.
- `target/quality/20260927-b3-xlsx-refusal-mutants/mutants.out/outcomes.json`:
  all 11 selected `write_row`/`cell_column` mutations caught; no missed, timeout or
  unviable outcomes. This final run includes the empty-text refusal and supersedes
  the earlier seven-coordinate-mutation run for this checkpoint.

These are local X11/AT-SPI and fixture results. Hosted CI, network-database temporal
UI scenarios, installed Wayland/package acceptance and the remaining B3 type and
consumer contracts remain separate work.

## Mixed bindings and failure propagation checkpoint, 2026-09-27

The shared SQL contract now passes named parameters through extraction, typed
input parsing, placeholder generation, driver binding and server decoding in one
query. Each of PostgreSQL, MySQL, SQL Server, ClickHouse, SQLite and DuckDB checks
three adversarial payloads across repeated names and mixed types. Expected rows
are constructed independently of the binding list. Quoted/commented colon names
must not become parameters. Redis and MongoDB continue their protocol-specific
contracts rather than receiving synthetic SQL scenarios.

Two installed GTK scenarios check committed rows: repeated parameters preserve
signed integer limits and SQL-like Unicode data; cancellation writes nothing and
a retry uses replacement values. Lexer regressions cover nested PostgreSQL
comments/dollar quotes, MySQL escaped quotes and SQL Server escaped identifiers.
Explicit text parsing retains whitespace and numeric/placeholder-looking text.

Transport regressions cover all explicit TLS modes against both legacy TLS flags,
CA paths, connection identity, authentication and timeout propagation. Local
sockets refuse every enabled TLS mode plus missing/regular-file paths before any
driver dial. These are deterministic option/refusal tests, not new live TLS or
SSH acceptance evidence.

Export writer tests simulate three-byte writes, interrupted writes and destination
I/O failures. All six text formats must finish short writes without byte loss;
all seven formats, including XLSX, must propagate write failures. These injected
writer failures do not simulate filesystem exhaustion or a broken network stream.

The first parser mutation run intentionally remains failed at
`target/quality/20260927-b3-parameter-mutants/mutants.out/outcomes.json`:
16 caught, one survivor, five timeouts and two unviable mutations. The survivor
removed the Boolean false arm; a new truth-table regression covers accepted
true/false spellings and invalid text. Extractor tests now call the parser on a
worker with a two-second receive deadline, turning nontermination into a test
failure rather than waiting for cargo-mutants to kill the suite. Production
parsing is unchanged. Compilation failures are unviable, not caught mutations.

The installed UI run reused the release binary from checkpoint `39198f1` because
this change only adds tests and documentation. Its SHA-256 is
`010fbec4d00d9cd391fa982e54ab038ac18a16d43755369c6afc28b31555cd30`.

The corrected run at
`target/quality/20260927-b3-parameter-mutants-corrected/mutants.out/outcomes.json`
caught all 22 compilable mutations, with two unviable, no survivors and no timeouts.
No mutation was excluded. The initial combined full/value/UI/harness run passed at
`target/quality/20260927T100452073699Z-layers/report.json`; its eight-driver report
is `target/quality/20260927T100713477391Z-values/report.json`. All 24 installed UI
scenarios passed with 24 JSON/stderr pairs in
`target/quality/20260927-b3-parameter-ui/`. Thirty-five Python harness tests passed.

After the Boolean and bounded-parser refinements, the final full/value/harness
rerun passed at `target/quality/20260927T101253832016Z-layers/report.json`.
All 11 value suites passed again at
`target/quality/20260927T101514358887Z-values/report.json`, including 21 core
contracts. Hosted CI and B4–B6 acceptance remain separate; B3 stays open for the
remaining native types and consumer parity.

## XML text consumer checkpoint, 2026-09-27

Two core regressions failed before the fix: literal carriage returns were not
protected from parser normalization, and XML-illegal text was silently replaced
with U+FFFD while export reported success. An installed-app reproducer against
the previous binary also failed after parsing its real exported file; evidence
is retained under `target/quality/20260927-b3-xml-before/`.

XML export now writes CR as `&#13;`, preserving standalone CR and CRLF alongside
LF, tabs, literal entity-looking text and Unicode. Unsupported XML 1.0 characters
are refused with the code point and one-based data row/column, with a JSON export
suggestion. Existing destination bytes survive the refusal, including when an
earlier row has already been written to the temporary file. Tests cover NUL,
controls, U+FFFE/U+FFFF, legal Unicode range boundaries, NULL and empty text.
The behavior follows [XML line-ending handling](https://www.w3.org/TR/xml/#sec-line-ends)
and [legal XML characters](https://www.w3.org/TR/xml/#charsets).

Two default installed-GTK scenarios exercise XML selection and the save chooser,
then parse the output or require the visible refusal. Four Python tests validate
the independent oracle and scenario registration, including deliberately
normalized line endings and an empty-text/NULL corruption. The existing workbook
scenarios share the named export-format keyboard helper. XML column-name
sanitization and type annotation are unchanged; this text checkpoint does not
establish reversible arbitrary column labels or complete typed XML import.

Scoped mutation evidence is
`target/quality/20260927-b3-xml-mutants/mutants.out/outcomes.json`: 15 caught,
one unviable, no survivors or timeouts across the row writer, text escaping and
legal-character predicate. The compilation failure is not counted as a catch.
The shared value report at `target/quality/20260927T102839954853Z-values/report.json`
passed all 11 suites, including 24 core contracts and all eight drivers.

Final local full/value/UI/harness report:
`target/quality/20260927T102554255832Z-layers/report.json`, all layers passed.
All 26 installed release-build UI scenarios passed with 26 JSON/stderr pairs at
`target/quality/20260927-b3-xml-ui/`; all 39 Python harness tests passed.
These are X11/AT-SPI and local fixture results. Hosted CI, installed Wayland and
package acceptance remain separate; B3 stays open for remaining native types
and consumer parity before B4–B6 acceptance.

## DuckDB native temporal and enum checkpoint

The native decoder previously exposed `date32:-1`, timestamp unit counters, interval descriptions and Arrow collection debug output as ordinary text. The first new live embedded-engine regression failed when SQL re-import tried to parse `date32:-1` as a date.

Dates and times now use shared typed values where representable. Timestamp seconds, milliseconds, microseconds and nanoseconds are split with Euclidean division so negative fractional epochs retain their exact fraction. Arrow timezone metadata selects the UTC instant variant. Enum dictionary labels preserve empty text, literal NULL text, Unicode and SQL NULL. Date infinities, BC dates and years above 9999 use DuckDB-compatible text; 24:00:00 remains distinct from midnight.

The integration corpus checks server-rendered source values against both parameter rebinding and generated SQL literals, plus independent JSON expectations for nanosecond timestamps and microsecond times. It includes nulls for every temporal family. Decoder unit tests cover all four units, negative remainders, end-of-day boundaries and arithmetic overflow.

Intervals, lists, fixed arrays, structs, maps and unions now return `Undecodable`, never debug text masquerading as the original value. The `UHUGEINT[]` case has a native `typeof` and exact `VARCHAR` oracle for `18446744073709551616`; the result is explicitly undecodable, and SQL literal and parameter consumers refuse it. A mixed `INTERVAL '1 month 2 days 3 microseconds'` case separately checks DuckDB's exact `VARCHAR` rendering while requiring `Undecodable` and SQL-literal/parameter refusal. Full interval/collection decoding remains open, including other nested unsigned values and interval carrier limits. Dates outside the shared calendar range and finite timestamps outside years 1–9999 are also explicitly undecodable. This is not full native-type, arbitrary-precision editing, GTK, MCP or release acceptance.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-duckdb --test integration value_contract_interval_components_refuse_lossy_consumers
```

The local embedded-engine contract passed.

Run locally:

```sh
cargo test --locked -p tablepro-driver-duckdb --lib --test integration
./scripts/test-value-contracts.sh --gtk --duckdb
```

The strict value runner automatically includes the new integration cases. Native decoder unit cases also run in the optional DuckDB crate test job. Protocol references: [DuckDB timestamps](https://duckdb.org/docs/current/sql/data_types/timestamp) and [interval basis units](https://duckdb.org/docs/current/sql/data_types/interval).

Validation: the DuckDB crate passed 16 tests (12 unit, four integration). Full local checks, all 11 selected value suites and the Python harness passed in `target/quality/20260927T104559674200Z-layers/report.json`; detailed value evidence is `target/quality/20260927T104731515648Z-values/report.json`. The combined value build reused 742 artifacts and rebuilt only the DuckDB package. Installed UI/Wayland and package acceptance were not rerun for this driver-only checkpoint.

Optional-driver Clippy (`cargo clippy --locked -p tablepro-driver-duckdb --all-targets -- -D warnings`) also passed. Its first run selected a separate native build fingerprint and rebuilt the bundled C++ library; this differs from the cached combined value runner above. Reuse a consistent command/feature graph for routine value checks.

Scoped mutation evidence: `target/quality/20260927-b3-duckdb-temporal-mutants/mutants.out/outcomes.json` records all 31 generated mutations in `temporal.rs`: 28 caught, three unviable, zero survivors and zero timeouts. Both the crate unit tests and integration suite ran for each viable mutation; compile failures are not counted as catches. Command:

```sh
mkdir -p target/mutation-tmp
TMPDIR=$PWD/target/mutation-tmp CARGO_TARGET_DIR=$PWD/target cargo mutants --package tablepro-driver-duckdb --file crates/drivers/duckdb/src/temporal.rs --test-tool cargo --timeout 30 --build-timeout 180 --output target/quality/duckdb-temporal-mutants -- --lib --test integration
```

## PostgreSQL interval fields, temporal arrays and infinities

The native interval regression first failed when `i64::MIN` microseconds became `-2562047788:00:54.775808`, which PostgreSQL could not parse back. Large hour fields now use an exact decimal-seconds representation. Any negative interval component triggers explicit signs on positive components, avoiding IntervalStyle-dependent reinterpretation. The corpus constructs expected months/days/microseconds wire bytes itself and checks the source fixture before comparing decoder results. It covers 59 boundary/generated cases across postgres, sql_standard, postgres_verbose and iso_8601 styles (236 combinations), pooled/session parity, SQL literals, typed casts of bound text and JSON text.

A second reproducer returned Undecodable for DATE infinity and date arrays. The driver now recognizes PostgreSQL infinity sentinels and decodes date, time, timetz, timestamp, timestamptz and interval array elements. Existing array dimension/lower-bound and size guards remain. Native array_send equality checks cover eras, year 10000, microseconds, end-of-day time, second-resolution offsets, DST-distinct instants, infinities, NULL elements and multidimensional bounds. SQL INSERT and bound parameter re-imports are included; a session in Asia/Kathmandu verifies that timestamp array imports preserve UTC instants.

New unit contracts check temporal element lengths, out-of-range payloads, empty/null arrays, infinity signs, era formatting and interval extremes. A real PostgreSQL DATE at year 1,000,000 remains within the server's finite range but exceeds chrono's shared calendar: the driver returns `Undecodable("DATE")`, while an independent `date::text` column remains exactly `1000000-01-01`. The app date parser rejects that input, and SQL literal rendering and parameter binding refuse the undecodable marker. A TIMESTAMP at PostgreSQL's upper finite bound, `294276-12-31 23:59:59.999999`, is also accepted by the server but returned as `Undecodable("TIMESTAMP")`; its independent `timestamp::text` value is exact, and the app parser, SQL literal renderer and parameter binder refuse it. Full finite temporal support beyond chrono's range, unsupported element types and broader consumer parity remain open. These changes do not prove complete native-type support for PostgreSQL or the other drivers.

Run `cargo test --locked -p tablepro-driver-postgres --lib value_contract`, then `cargo test --locked -p tablepro-driver-postgres --test integration value_contract_dates_preserve_eras_large_years_and_instants -- --include-ignored --exact --test-threads=1` with Docker available. The combined strict value runner discovers all seven PostgreSQL server contracts. Updated fixture declarations are recorded in [ignored tests](ignored-tests.md).

Reference: [PostgreSQL interval input and storage](https://www.postgresql.org/docs/current/datatype-datetime.html#DATATYPE-INTERVAL-INPUT). The strategy for extending these proofs across drivers is in [type contracts](type-contract-strategy.md).

Mutation evidence: `target/quality/20260927-b3-pg-native-mutants/mutants.out/outcomes.json` records 74 selected mutations: 73 caught, one unviable, zero survivors and zero timeouts. This measurement ran value-contract unit tests for interval decoding/formatting, temporal-array dispatch/text and scalar temporal decoding; it was not a full-workspace mutation run. The separate PostgreSQL 16 fixture run supplies real-server evidence. Reproduce from the Linux workspace:

```sh
mkdir -p target/mutation-tmp
TMPDIR=$PWD/target/mutation-tmp CARGO_TARGET_DIR=$PWD/target cargo mutants --package tablepro-driver-postgres --file crates/drivers/postgres/src/temporal.rs --file crates/drivers/postgres/src/array.rs --file crates/drivers/postgres/src/decode.rs --re 'decode_interval|format_interval|push_counted|decode_temporal|temporal_element|array_text' --test-tool cargo --timeout 30 --build-timeout 180 --output target/quality/pg-native-mutants -- --lib value_contract
```

The first full-gate attempts remain recorded in `20260927T110823517670Z-layers/report.json` (stale ignored-test inventory) and `20260927T110938605682Z-layers/report.json` (file-size guard). Both are under `target/quality/`. The inventory was regenerated and scalar temporal decoding moved into the existing temporal module without raising a guardrail baseline. The subsequent `20260927T111058478564Z-layers/report.json` passed full, values and harness; its value report is `20260927T111306775227Z-values/report.json` with all 11 suites and seven PostgreSQL contracts passing.

Final clean-source validation after mutation testing passed full, values and harness in `target/quality/20260927T111941420743Z-layers/report.json`. All 11 selected value suites passed in `target/quality/20260927T112138445468Z-values/report.json`; compilation reused 745 artifacts, rebuilt zero packages and took 0.592 seconds. Installed UI/Wayland and package acceptance were not rerun for this driver checkpoint.

## MySQL native TIME, zero-date and YEAR checkpoint

The reproducer ran against MySQL 8 with a permissive `sql_mode`. Before the fix, `-01:00:00` read back as `01:00:00`. Zero DATE, DATETIME and TIMESTAMP values read as NULL, because sqlx reports an all-zero binary value as NULL. TIME outside one day, dates with zero parts and every YEAR value were undecodable. Times of day remain `Time`. Negative or extended times, zero dates and dates with zero parts are now exact text, and YEAR is an integer. sqlx 0.9 also reports the TIME sign inverted, so the driver reads the sign field directly.

The test compares every value with an independent expected value. It then writes the rows back through bound parameters and SQL INSERT export, and the server confirms all six rows match with `<=>`. Scoped cargo-mutants on `crates/drivers/mysql/src/temporal.rs`: 16 mutants, 13 caught, 3 unviable, zero survivors and zero timeouts, using the recipe above with `-- --lib --test integration -- --include-ignored calendar_fields native_time_zero`.

MySQL `TIMESTAMP` values are exact only while the server session is UTC because MySQL returns a session-local wall time and sqlx decodes it as UTC. Driver-owned pool connections set `time_zone = '+00:00'` when created and reset it before each checkout. Dedicated sessions start in UTC; if a session later uses another time zone, non-NULL `TIMESTAMP` cells are returned as undecodable rather than as a wrong instant. A Docker regression first reproduced the mismatch at `+05:45`, then verified the independent session-local text and Unix-epoch microseconds while checking the refusal marker. The same regression changes a pooled connection to `+05:45` and confirms the next checkout still returns the exact UTC instant. The existing zero-date temporal round-trip continues to verify UTC pooled decoding. Run `cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mysql --test integration session_non_utc_time_zone_refuses_mysql_timestamp_instant -- --include-ignored --exact --test-threads=1` and `cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mysql --test integration native_time_zero_date_and_year_values_survive_reads_parameters_and_exports -- --include-ignored --exact --test-threads=1` with Docker available.

Subsequent B3 coverage verifies BIT(1..64), ENUM/SET labels and spatial bytes;
bounded BIT edits survive a driver update, while spatial and too-wide BIT values
remain read-only. Text exports also run with and without `NO_BACKSLASH_ESCAPES`;
backslash-bearing column comments are explicitly refused. Session time-zone and
stricter SQL-mode matrices, installed-app-to-MySQL grid acceptance and broader
consumer parity remain open.

## SQLite dynamic storage-class checkpoint

A new file-backed regression starts a `NUMERIC` column with TEXT, BLOB and NULL
values, then exercises bound edits that transition through REAL, BLOB, TEXT and
INTEGER storage classes. The test checks both SQLite's independent `typeof`
result and TablePro's decoded value. It then exports the edited rows as SQL
INSERT literals, re-imports into a second `NUMERIC` table and requires the same
storage classes and values. This closes the driver-level edit/SQL re-import
case. A policy-guarded CSV import test also covers legal text and numeric values
in INTEGER, REAL and NUMERIC affinity columns. A focused app contract now sends
`42.50` through the same column parser and keyed update builder used for grid
edits, then verifies `typeof(amount) = 'real'` and the returned `42.5` value.
Installed GTK/package acceptance across the storage-class matrix remains open.

The full SQLite integration suite passed all 21 tests. Run locally:

```sh
cargo test --locked -p tablepro-driver-sqlite --test integration
cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib sqlite_numeric_grid_edit_keeps_parser_and_affinity_behavior -- --test-threads=1
```

The mapped change-contract gate passed this exact app regression at
`target/quality/20260928T130321776497Z-change-contracts/report.json`.

## MongoDB nested BSON and native boundary checkpoint

A decoder unit regression first failed for nested `Decimal128`, binary and date
values. The driver converted all three to their debug-display strings, losing the
Decimal128 type/precision, binary subtype and exact BSON date milliseconds. Generic
binary values remain `Value::Bytes` for editing. Nested documents/arrays and top-level
non-generic binary values use canonical MongoDB Extended JSON so BSON special types
and subtype metadata remain explicit.

The unit regressions check Decimal128's smallest exponent, largest finite value,
negative zero and scale; every BSON binary subtype, including legacy UUID, encrypted,
column, sensitive, vector, reserved and user-defined values; and dates at both ends
of the signed 64-bit millisecond range. A Docker-backed test checks real UUID and
user-defined subtype values. Top-level Decimal128
values remain exact text because the shared decimal type cannot represent this
range. Dates that cannot be rendered as RFC3339 use canonical Extended JSON with
the exact signed millisecond count. A Docker-backed MongoDB 7 test inserts these
native BSON values directly, reads them through the driver query path, and checks
the resulting value/type representation. Uncommon top-level BSON types now use
canonical Extended JSON instead of display text. Real-server JSON, CSV and XLSX
consumer checks preserve their markers. A Docker-backed grid edit regression
updates a nested document and array through the keyed row update path, then
checks the stored Decimal128, date, binary subtype and integer fields through
the native BSON client. App parser tests route MongoDB object, array and ObjectId
columns through JSON parsing. MongoDB 7 integration tests verify canonical
Extended JSON re-import and MCP browse output preserve nested BSON types.
They also check nested Int64 above 2^53, explicit nested null and Unicode across
driver results, JSON/CSV/XLSX output, import and MCP browse response.
Top-level Timestamp, regex, MinKey, MaxKey, JavaScriptCodeWithScope and Symbol
grid edits also round-trip as native BSON. The CodeWithScope regression checks
both the stored code and its Int64 scope value; the Symbol regression checks a
stored native BSON Symbol through the native client. Other special types and
mixed-type-column edits remain open.

Focused local checks:

```sh
cargo test --locked -p tablepro-driver-mongodb --lib nested_bson_special_values_keep_their_extended_json_types
cargo test --locked -p tablepro-driver-mongodb --lib bson_decimal_and_date_extremes_remain_exact_outside_core_ranges
cargo test --locked -p tablepro-driver-mongodb --test integration -- nested_bson_special_values_keep_exact_extended_json_types --include-ignored --test-threads=1
cargo test --locked -p tablepro-driver-mongodb --test integration a_nested_and_max_key_grid_edit_writes_extended_json_back_as_native_bson -- --include-ignored --exact --test-threads=1
```

Against the old decoder, the regression failed with
`Decimal128("...")`, `Binary(...)`, and `DateTime("...")` as JSON strings. Full
MongoDB unit and real-server suites passed (24 unit tests; eight integration
tests). Scoped mutation evidence at
`target/quality/20260927-b3-mongodb-native-mutants/mutants.out/outcomes.json`
records four generated mutations: three caught, one unviable whole-function
replacement (`Value` has no `Default`), and no survivors or timeouts. The failing
pre-fix regressions independently demonstrate test sensitivity. The real-server
test verifies nested Extended JSON through JSON export and parses generated CSV
back into fields to check that quoting preserves the nested object. It also
exports the actual query result as XLSX and checks Decimal128, date and binary
subtype markers in workbook strings. An MCP unit contract confirms BSON
Extended JSON is passed through without flattening; Mongo-backed MCP browse and
native BSON re-import checks now run in the integration suite. Top-level
Timestamp, regex, MinKey, MaxKey, JavaScriptCodeWithScope and Symbol grid edits
are covered; Docker assertions check native CodeWithScope code/scope and Symbol
types after edits. Local regressions use a BSON
String and Decimal128 with identical text: the field is labeled `mixed`, and
the shared app grid editability gate refuses both values because the result
model maps each scalar to the same `Value::Text`. The grid still displays the
exact text. JSON, CSV and XLSX consume that same text value and do not retain
the BSON-kind distinction. The Docker-backed real-server test passed and
confirmed storage still contains BSON String and Decimal128 as distinct kinds.
Browse and `find` metadata starts from a 50-document sample, then incorporates
the bounded documents actually returned on the page. A Docker regression puts
Decimal128 at offset 50 after 50 String values and verifies the returned page's
column is `mixed` while both BSON values still display as the same exact text.
The shared grid gate keeps that mixed column read-only. Heterogeneity outside
both the sample and the returned page remains undetected, as do remaining
special BSON kinds and mixed-type export fidelity. The earlier MongoDB 7
query/export/import and MCP browse Docker tests passed with large Int64, null
and Unicode values added on 2026-09-28.

A separate homogeneous Decimal128 contract composes the default CSV exporter,
typed CSV parser, and keyed grid update. The decimal column metadata parses the
CSV cell as `Value::Decimal`; after the update, a native BSON read confirms the
field remains `Bson::Decimal128` with the exact original value. This covers a
typed same-schema round trip and does not preserve BSON identity in generic
exports or mixed String/Decimal128 columns.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mongodb --test integration decimal128_csv_round_trip_through_typed_grid_edit_keeps_native_bson -- --include-ignored --exact --test-threads=1
```

The focused MongoDB 7 Docker contract passed.

Focused local results:

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mongodb --lib mixed_scalar_bson_column_is_marked_and_values_remain_visible
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-app --lib mixed_bson_columns_are_read_only -- --test-threads=1
```

Both pass. The real-server contract is
`mixed_string_and_decimal128_columns_keep_values_and_refuse_lossy_edit_metadata`:

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mongodb --test integration mixed_string_and_decimal128_columns_keep_values_and_refuse_lossy_edit_metadata -- --include-ignored --exact --test-threads=1
```

Late page sampling is covered by
`browse_page_types_include_documents_after_the_metadata_sample`:

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mongodb --test integration browse_page_types_include_documents_after_the_metadata_sample -- --include-ignored --exact --test-threads=1
```

### MongoDB BSON Undefined grid edit

A Docker-backed contract inserts native BSON `Undefined`, verifies the grid
metadata and canonical `{"$undefined":true}` value, applies the shared keyed
row update, then reads the row through both BookiE and the native BSON client.
The persisted value remains `Bson::Undefined`; it does not become null. The
focused MongoDB 7 test passed.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mongodb --test integration value_contract_undefined_grid_edit_preserves_native_bson -- --include-ignored --exact --test-threads=1
```

## ClickHouse named temporal timezones, 2026-09-27

A live ClickHouse 24.8 regression showed that `DateTime64(6, 'Asia/Tokyo')`
returned a Tokyo wall clock that TablePro decoded as a timezone-free timestamp.
That changed the instant by nine hours. The type metadata contains the IANA zone,
so the decoder now applies timezone rules and returns the corresponding UTC
`TimestampTz`, retaining the fractional seconds.

The decoder refuses unknown zones and local wall-clock strings that fall in DST
gaps or folds. The ClickHouse row format returns a local wall clock without its
offset, so an ambiguous returned value does not identify a unique instant and
must not be guessed. Untagged `DateTime` and `DateTime64` values remain
timezone-free `DateTime` values.

Unit tests cover type wrappers, Tokyo conversion, unknown zones, and New York DST
gaps/folds. A Docker integration test first failed with `DateTime(12:34...)`
instead of the expected `TimestampTz(03:34...Z)`, then passed after the fix. It
also checks that the instant survives a bound parameter and a SQL literal round
trip. The full ClickHouse integration suite passed all 23 cases. The combined `values`
layer passed in 113.058 seconds, `full` passed in 100.8 seconds, and the harness
passed. Reports are in `target/quality/20260927T180959352277Z-layers/`,
`target/quality/20260927T181245187611Z-layers/`, and
`target/quality/20260927T181435128973Z-layers/`.

A second Docker regression covers the New York fall-back `DateTime64(3)` local
time `2024-11-03 01:30:00.000`. The server confirms the exact local text and
reports an epoch in one of the two valid fold instants. Because the result wire
text omits which offset was used, BookiE returns `Undecodable`; SQL literal and
parameter consumers refuse it. The focused test passed against ClickHouse
24.8.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-clickhouse --test integration value_contract_ambiguous_datetime64_local_time_is_refused -- --include-ignored --exact --test-threads=1
```

A third server check supplies the nonexistent spring-forward local time
`2024-03-10 02:30:00` to `DateTime64(3, 'America/New_York')`. ClickHouse 24.8
normalizes it to returned local time `01:30:00.000`; the server epoch oracle is
`1710052200000` (`2024-03-10T06:30:00Z`). BookiE decodes that normalized,
unambiguous server result to the exact UTC instant. This tests server behavior
for the supplied expression; it does not preserve the nonexistent civil input.

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-clickhouse --test integration value_contract_nonexistent_datetime64_input_matches_server_normalization -- --include-ignored --exact --test-threads=1
```

Run the focused checks locally with:

```sh
cargo test --locked -p tablepro-driver-clickhouse --lib
cargo test --locked -p tablepro-driver-clickhouse --test integration -- --include-ignored --test-threads=1
./scripts/run-test-layer.py values
./scripts/run-test-layer.py full
```

Scoped Rust mutation evidence for `temporal.rs` is in
`target/quality/20260927-ch-temporal-mutants-final/mutants.out/outcomes.json`:
13 caught, 2 unviable, no survivors or timeouts. The initial mutation pass made
two synthetic wrapper results loop; the progress guard now exits safely and the
repeat has no timeout. Reproduce with:

```sh
mkdir -p target/mutation-tmp
TMPDIR=$PWD/target/mutation-tmp CARGO_TARGET_DIR=$PWD/target cargo mutants --package tablepro-driver-clickhouse --file crates/drivers/clickhouse/src/temporal.rs --test-tool cargo --timeout 30 --build-timeout 180 --output target/quality/20260927-ch-temporal-mutants-final -- --lib
```
