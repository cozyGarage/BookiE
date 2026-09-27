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
proof requirements and remaining driver targets. A passing scalar suite does not
establish complete native-type support.

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
| JSON and MCP | Non-finite values remain distinct from SQL NULL |

ClickHouse long-value reads are checked with a server-generated value. Oversized
inline SQL is required to return its explicit query-size error; the fixture does
not raise that server limit or accept a truncated success.

Text cases include empty strings, numeric-looking strings, Unicode, apostrophes,
quotes, backslashes, line breaks and text beyond 256 KiB. Float assertions in the
SQL harness compare bit patterns. Existing driver tests still cover binary
exports, typed row identity, cancellation, metadata and other engine behavior.
The new suite supplements those tests.

This is a growing contract, not proof of every database type. PostgreSQL arbitrary-precision numeric editing, remaining array element types, extreme finite calendars and interval consumer parity, nested BSON values,
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

Intervals, lists, fixed arrays, structs, maps and unions now return `Undecodable`, never debug text masquerading as the original value. Tests require SQL literal and parameter refusal. Full interval/collection decoding remains open, including nested unsigned wide integers and interval carrier limits. Dates outside the shared calendar range and finite timestamps outside years 1–9999 are also explicitly undecodable. This is not full native-type, arbitrary-precision editing, GTK, MCP or release acceptance.

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

New unit contracts check temporal element lengths, out-of-range payloads, empty/null arrays, infinity signs, era formatting and interval extremes. Finite dates outside the shared calendar, unsupported element types and broader consumer parity remain open. These changes do not prove complete native-type support for PostgreSQL or the other drivers.

Run `cargo test --locked -p tablepro-driver-postgres --lib value_contract`, then `cargo test --locked -p tablepro-driver-postgres --test integration value_contract -- --include-ignored --test-threads=1` with Docker available. The combined strict value runner discovers all seven PostgreSQL server contracts. Updated fixture declarations are recorded in [ignored tests](ignored-tests.md).

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
in INTEGER, REAL and NUMERIC affinity columns. Installed grid acceptance remains
open.

The full SQLite integration suite passed all 21 tests. Run locally:

```sh
cargo test --locked -p tablepro-driver-sqlite --test integration
```

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
Top-level Timestamp, regex and MinKey grid edits also round-trip as native BSON;
remaining special types and mixed-type-column edits remain open.

Focused local checks:

```sh
cargo test --locked -p tablepro-driver-mongodb --lib nested_bson_special_values_keep_their_extended_json_types
cargo test --locked -p tablepro-driver-mongodb --lib bson_decimal_and_date_extremes_remain_exact_outside_core_ranges
cargo test --locked -p tablepro-driver-mongodb --test integration -- nested_bson_special_values_keep_exact_extended_json_types --include-ignored --test-threads=1
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
Timestamp, regex and MinKey grid edits are covered; remaining special BSON kinds
and other consumers remain open.

## ClickHouse named temporal timezones, 2026-09-27

A live ClickHouse 24.8 regression showed that `DateTime64(6, 'Asia/Tokyo')`
returned a Tokyo wall clock that TablePro decoded as a timezone-free timestamp.
That changed the instant by nine hours. The type metadata contains the IANA zone,
so the decoder now applies timezone rules and returns the corresponding UTC
`TimestampTz`, retaining the fractional seconds.

The decoder refuses unknown zones and local times in DST gaps or folds. The
ClickHouse row format returns a local wall clock without its offset, so those
values do not identify a unique instant and must not be guessed. Untagged
`DateTime` and `DateTime64` values remain timezone-free `DateTime` values.

Unit tests cover type wrappers, Tokyo conversion, unknown zones, and New York DST
gaps/folds. A Docker integration test first failed with `DateTime(12:34...)`
instead of the expected `TimestampTz(03:34...Z)`, then passed after the fix. It
also checks that the instant survives a bound parameter and a SQL literal round
trip. The full ClickHouse integration suite passed all 23 cases. The combined `values`
layer passed in 113.058 seconds, `full` passed in 100.8 seconds, and the harness
passed. Reports are in `target/quality/20260927T180959352277Z-layers/`,
`target/quality/20260927T181245187611Z-layers/`, and
`target/quality/20260927T181435128973Z-layers/`.

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
