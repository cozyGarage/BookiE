# Data-integrity suite

The data-integrity suite is the deterministic check that a value written to a database comes back through BookiE unchanged. Each case states a native type, the values to store and the comparison rule. The suite stores the values with the engine's own SQL, asks the database server to render them, reads the same rows through BookiE's governed path and compares the two.

It applies [ADR 0007](decisions/0007-type-and-value-preservation.md) and the [value contract rules](value-contract-rules.md). A pass here proves the named engine version, type, value and route. It does not prove other routes or other server versions.

## The referee is the server

The expected value always comes from the database server, never from BookiE code:

- The table is created and filled by the server's own command-line client inside the container (`psql`, `mysql`, `mariadb`, `clickhouse-client`, `redis-cli`). For the file engines it is the engine library opened directly: `sqlx-sqlite` for SQLite and the `duckdb` crate for DuckDB, used only to run SQL and read back text, never through BookiE's driver or decoder. BookiE code does not write the values.
- The same client asks the server for its canonical rendering. Every rendered field is hex-encoded by the server, so the client output cannot be altered by quoting, newlines or character sets.
- BookiE reads the rows through the registry driver wrapped in `PolicyGuard`, with an audit journal and a read-only session, the same way an app read session does.
- The harness compares BookiE's typed `Value` with the server rendering using the case's rule. The harness formats a `Value` itself; it does not call BookiE's display, copy or export code, so a formatter bug cannot hide a decoder bug.

Where a server rendering is lossy, the corpus names a different server expression, never a hand-written value. For example MySQL `CAST(float AS CHAR)` prints six digits, so the `FLOAT` case uses `CAST(v AS DOUBLE)`.

Server renderings per engine:

| Engine | Text | Bytes | Instant | Type |
| --- | --- | --- | --- | --- |
| PostgreSQL | `format('%s', v)` (the type output function, which keeps `char(n)` padding and host-only `inet`) | `encode(v, 'hex')` | `extract(epoch from v)` | `pg_typeof(v)` |
| MySQL, MariaDB | `CAST(v AS CHAR)` | `HEX(v)` | `UNIX_TIMESTAMP(v)` | `information_schema.COLUMNS.COLUMN_TYPE` |
| SQLite | `CAST(v AS TEXT)`; REAL as `printf('%!.17g', v)`; BLOB as `hex(v)` | `hex(v)` | `unixepoch(v, 'subsec')` | `typeof(v)` per row |
| DuckDB | `CAST(v AS VARCHAR)` | `hex(v)` | `epoch_ns(v)` | `typeof(v)` |
| ClickHouse | `toString(v)`; decimals as `toDecimalString(v, scale)` | `hex(v)` | `toUnixTimestamp64Nano(toDateTime64(v, 9))` | `toTypeName(v)` |
| Redis | The reply of the read command, flattened and hex-encoded by a server-side Lua script (`EVAL`) | Same | Not applicable | `TYPE` |

Nanosecond epochs are converted to exact decimal seconds before comparison.

## Routes

Each case runs through every read route:

| Route | BookiE call | Consumer it stands for |
| --- | --- | --- |
| `query` | `query_result_sets_controlled("SELECT id, v FROM t ORDER BY id")` | SQL editor batch |
| `browse` | `fetch_rows_controlled(schema, t, 0, 1000)` | Table page read |
| `param_where` | `query_params_controlled("SELECT v FROM t WHERE id = $1")` | Parameterised read of a stored column; on MySQL this is the binary protocol |
| `param_select` | `query_params_controlled("SELECT $1::type")` on PostgreSQL, `SELECT ?` on MySQL, binding the value BookiE read in `query` | A typed parameter echoed back, the class of the #4094 decoder-plan bug |

### Copy, export, import and write routes

`routes::run_routes` moves the same corpus through app-services and core the way the GPUI and GTK consumers do, without their widgets. Every route ends at the server: a re-read artefact is compared with the server rendering by the case rule, and a write is compared row by row with what the server stored for the original value.

| Route | Path | Compared with |
| --- | --- | --- |
| `copy_tsv` | `result_copy::tsv`, read back with the paste parser `clipboard_grid::parse_clipboard_grid` | Server rendering, case rule |
| `export_csv` | `write_result_file` CSV with a NULL marker and no formula guard (the lossless settings) | Server rendering, case rule |
| `export_json`, `export_json_lines` | `write_result_file` JSON and JSON Lines | Server rendering, case rule |
| `export_sql` | `write_result_file` SQL INSERT into an empty copy of the table, run through the governed connection | Server rows of the copy equal the original rows |
| `import_csv`, `import_xlsx` | CSV or XLSX export, then `plan_csv_import` into an empty copy and the governed batch | Server rows of the copy equal the original rows (export to import checksum) |
| `edit` | Edit text of the read value, `parse_grid_cell_input_with_style`, `plan_cell_saves`, `execute_in_transaction_checked_controlled` into a placeholder row | Server row equals the original row |
| `paste` | `copy_text`, `parse_pasted_cell_with_style`, then as `edit` | Same |
| `set_cells` | `plan_set_cells` with the edit text, then `plan_cell_saves` | Same |
| `insert_row` | `plan_row_insert` with the edit text | Same |
| `noop_edit` | Re-enter the edit text of each original cell over itself | Every original row is unchanged on the server |

The harness reads artefacts with its own parsers per carrier (and decodes the documented `bookie:sqlite-any:v1` tags). A spelling it cannot parse, such as a BC date, is cast by the server and compared with the server text. The writable session confirms approval prompts as a user would.

An error is a safe outcome and is reported as "refused" without failing the run: an export or import that refuses a value, a plan that refuses input, or a statement the server rejects inside the batch, so nothing is written. A different value shown or stored is a failure. Rows whose value the grid cannot edit inline (NULL, binary, undecodable, text with NUL, and empty text except for `paste`) are not entered.

Redis has no tables, so its routes differ:

| Route | BookiE call | Server reply it is compared with |
| --- | --- | --- |
| `command` | The read command (`GET`, `HGETALL`, `LRANGE`, `SMEMBERS`, `ZRANGE ... WITHSCORES`, `XRANGE` and others) through `query_result_sets_controlled`, every cell in row order | The same command run inside the server |
| `full_value` | `SELECT "Value" FROM "db0" WHERE "Key" = ?` | `GET`, or no row for a missing key, or a refusal for a collection |
| `browse` | `fetch_rows_controlled(None, "db0", ...)` | `TYPE`, `HLEN`/`LLEN`/`SCARD`/`ZCARD`, and `GET` for values under the 4 KiB preview; a larger value must not be shown as whole |

A bulk reply that is valid UTF-8 must be `Value::Text`; other bytes must be `Value::Bytes`. Redis commands are not SQL, so the policy asks for confirmation; the Redis run confirms, as a user would.

## Corpus format

The corpus lives in `tests/data/integrity/<engine>.toml`. One file serves every server of an engine; `servers` lists their labels.

```toml
engine = "postgres"
servers = ["pg16", "pg18"]

[[case]]
id = "numeric-scaled"          # unique; also names the table di_numeric_scaled
type = "numeric(30,8)"         # column DDL; {table} expands to the case table
literals = ["NULL", "1.23"]    # SQL literals, one row each, in order
literals_from = "decimals"     # optional: append a list from tests/data/value-contract.json
carrier = "Decimal"            # the Value variant BookiE must return
rule = "exact"                 # how BookiE's value is compared with the server
servers = ["pg16"]             # optional: only these servers
setup = ["CREATE TYPE ..."]    # optional: statements run before CREATE TABLE
session = "SET SESSION ..."    # optional: prefix for every INSERT, same client session
create = "CREATE TABLE {table} (id INTEGER PRIMARY KEY, v {type}) STRICT"  # optional: replaces the engine's CREATE TABLE
by_type = { integer = { carrier = "Int", rule = "exact" } }  # optional: expectation by the server's per-row type, for SQLite storage classes
referee = "CAST({v} AS DOUBLE)"  # optional: server expression for the rule's rendering
param = "SELECT CAST(? AS JSON)" # optional: replaces the param_select statement
param_carrier = "Text"         # optional: carrier for param_select
param_rule = "exact"           # optional: rule for param_select
skip = [{ route = "param_select", reason = "..." }]
known_failing = [{ route = "param_select", issue = 4206, servers = ["pg16"], reason = "..." }]
note = "Why this case exists or why its rule is what it is."
```

The tables are created in a fresh container or temporary file, so a case does not need cleanup.

The Redis corpus (`tests/data/integrity/redis.toml`) lists, per case, the setup commands, the read command and optional `skip` and `known_failing`. `{key}` expands to the case key, and an argument written `hex:...` is sent as those bytes.

### Comparison rules

| Rule | Comparison | Normalisation |
| --- | --- | --- |
| `exact` | BookiE's text form equals the server text | None |
| `server_cast` | The server renders BookiE's text cast back to the column type, and that equals the server text | Accepts any spelling the server parses to the same value, for exact text fallbacks such as PostgreSQL arrays |
| `number` | Decimal text equality | Leading and trailing zeros only |
| `float` | Bit equality of the double parsed from the server text | NaN equals NaN |
| `float32` | Bit equality after narrowing both to single precision | For single-precision columns BookiE widens through the shortest single decimal |
| `bytes` | Lowercase hex equality | Hex case |
| `bool` | `t`, `true`, `1` and `f`, `false`, `0` | Server spelling |
| `temporal` | Text equality | Trailing zeros of fractional seconds |
| `instant` | Epoch seconds equality; a `Date` is midnight UTC and a `DateTime` is a UTC wall clock | Trailing zeros |
| `json` | Compact re-serialisation of both documents, key order kept | Whitespace |
| `refusal` | BookiE returns `Value::Undecodable` for a non-NULL cell | None |

Every rule first checks NULL: a server NULL must be `Value::Null`, and a non-NULL server value must not be. Then it checks the carrier, so a right-looking text in the wrong `Value` variant fails.

## Known failures

A case that fails today carries `known_failing` with the issue number and the route. The suite asserts that it still fails. When a fix makes it pass, the suite fails with "known issue #N looks fixed" until the marker is removed, so a fix is never silent. A setup or referee error is always a failure, never a known failure.

## Adding a case

A value bug is fixed by first adding a failing corpus row:

1. Add the row with the native type and literal to `tests/data/integrity/<engine>.toml`, with the carrier and rule the ADR 0007 outcome requires.
2. Run the engine target and confirm the case fails for the reported reason.
3. If the fix is not in the same change, add `known_failing` with the issue number.
4. The fix removes the marker in the same commit.

Do not change a carrier or rule to make a failing value pass. Change it only when BookiE's result is a valid ADR 0007 outcome (exact typed value, exact text fallback or explicit refusal) and say why in `note`.

## Running locally

All targets need Docker except the corpus check. Build with plain Cargo.

```bash
cargo test -p bookie-data-integrity-tests --test corpus
cargo test -p bookie-data-integrity-tests --test postgres -- --include-ignored --test-threads=1 --nocapture
cargo test -p bookie-data-integrity-tests --test mysql -- --include-ignored --test-threads=1 --nocapture
cargo test -p bookie-data-integrity-tests --test clickhouse -- --include-ignored --test-threads=1 --nocapture
cargo test -p bookie-data-integrity-tests --test redis -- --include-ignored --test-threads=1 --nocapture
cargo test -p bookie-data-integrity-tests --test sqlite --test duckdb -- --nocapture
```

Each engine target also has a `copy_export_import_and_edits` test for the routes above (PostgreSQL 16, MySQL 8.4, MariaDB 11, SQLite, DuckDB; ClickHouse without the edit routes). Select one with a name filter, for example `-- --include-ignored copy_export`.

`--nocapture` prints a summary per server and every known failure. Each Docker test starts its own container from the images listed in `ci/test-images.txt` for that driver group (PostgreSQL 16 and 18, MySQL 8.4.13, MariaDB 11, ClickHouse 24.8, Redis 7.4.12) and removes it at the end. SQLite and DuckDB use a temporary file and need no Docker. With `BOOKIE_TEST_RUN_ID` set, containers carry the run label that `scripts/cleanup-testcontainers.py` removes.

## CI tier

The corpus check and the SQLite and DuckDB targets run in the sandbox tier. The Docker targets belong to the driver tier: `scripts/run-driver-group.sh <driver>` runs `tests/data-integrity/tests/<driver>.rs` after that driver's own targets, so they run in the `driver-core` job (PostgreSQL and MySQL) and the `driver-analytics` job (ClickHouse and Redis) of the Forgejo and GitHub gates.

## Coverage

Route results in the last local run (pass, known-failing, refused): PostgreSQL 16 549, 28, 87; MySQL 8.4 422, 16, 46; MariaDB 11 423, 14, 47; SQLite 164, 12, 28; DuckDB 317, 24, 39; ClickHouse 228, 4, 27.

Current corpus: PostgreSQL 57 cases (pg16 and pg18), MySQL 43 cases shared by MySQL 8.4 and MariaDB 11, SQLite 18, DuckDB 34, ClickHouse 37 (24.8), Redis 18 (7.4.12).

Read routes by type family. "Covered" means every read route passes on every listed server. "Refusal" means BookiE returns `Undecodable` instead of a value, which ADR 0007 accepts as a safe outcome but is not support.

| Family | PostgreSQL | MySQL 8.4, MariaDB 11 | SQLite | DuckDB | ClickHouse |
| --- | --- | --- | --- | --- | --- |
| Integers, signed and unsigned, boundaries, 128 and 256 bit | covered | covered except `param_select`: #4094 | covered | covered (HUGEINT, UHUGEINT and UBIGINT past i64 as exact text) | covered (Int128, UInt256 and UInt64 past i64 as exact text) |
| Exact decimals, max precision, beyond rust_decimal | covered; bound zero loses scale: #4206 | covered except `param_select`: #4094 | not applicable (stored as REAL) | covered | covered (Decimal256 as exact text when wide) |
| Floats: -0, subnormal, infinity, NaN, single precision | covered | covered except `param_select`: #4094 | covered | covered | covered |
| Boolean and bits | covered | covered except `param_select`: #4094 | covered | BIT known-failing #4214 | covered |
| Text: Unicode, NUL, padding, 'NULL' vs NULL vs '', collations, large | covered | covered | covered | covered | covered (FixedString keeps NUL padding; binary String as bytes) |
| Binary | covered | covered except `param_select`: #4094 | covered | covered | covered |
| Temporal: dates, times, timestamps, time zones, DST, BC, infinity, zero dates, nanoseconds | covered | covered except `param_select`: #4094 | covered (declared temporal TEXT, INTEGER, REAL and BLOB storage) | covered (TIMESTAMP_NS, infinity) | covered (Date32, DateTime64 with zones; DST fold is a refusal); bound Date32 outside 1970-2149 clamps: #4215 |
| Interval | covered (exact text) | not applicable | not applicable | covered (`server_cast`) | not yet |
| UUID | covered | not applicable | not applicable | covered | covered |
| JSON | covered (json exact, jsonb normalized) | covered | not applicable | covered (exact text) | not yet |
| Arrays and nested values | covered (`server_cast`) | not applicable | not applicable | refusal (LIST, STRUCT, MAP, UNION) | covered as JSON (Array, Tuple, Map); binding refused |
| Enum, set, domain, LowCardinality, Nullable | covered | covered | not applicable | covered | covered |
| Composite, `tstzrange`, multirange, money | refusal | not applicable | not applicable | not applicable | not applicable |
| Network, geometric, other | covered (exact text) | geometry as stored bytes, except `param_select`: #4094 | not applicable | not applicable | IPv4 and IPv6 covered |
| SQLite affinity and mixed storage classes | not applicable | not applicable | covered (NUMERIC, untyped, STRICT ANY) | not applicable | not applicable |

Redis: strings (UTF-8, empty, the text `NULL`, NUL, binary, number-like, integer-encoded, 300000 bytes), hashes, lists, sets, sorted sets with float scores, streams and a missing key are covered on the `command`, `full_value` and `browse` routes.

Engines and routes. "Refused" lists cases where BookiE refuses a value on that route instead of carrying it (for example NaN in SQL INSERT export, empty text in XLSX export, ClickHouse `max_query_size`); the run prints each refusal.

| Route | PostgreSQL 16 | MySQL 8.4, MariaDB 11 | SQLite | DuckDB | ClickHouse 24.8 | Redis |
| --- | --- | --- | --- | --- | --- | --- |
| Read: query, browse, param_where | covered (pg16, pg18) | covered | covered | covered except #4214 | covered | covered (command, full_value, browse) |
| Read: param_select | covered except #4206 | known-failing #4094 for non-text types | covered | covered except #4214 | covered except #4215 | not applicable |
| Copy as TSV and paste back | covered | covered except #4221 | covered except #4221 | covered except #4214 | covered | not yet |
| Export CSV, JSON, JSON Lines, re-read | covered except #4223 (JSON) | covered except #4223 (JSON) | covered | covered except #4214 | covered | not yet |
| Export SQL INSERT, run into an empty table | covered | covered | covered except #4220 | covered except #4214 | covered | not applicable |
| Export to CSV or XLSX, import into an empty table | covered except #4206, #4222 | covered except #4225 | covered except #4220 | covered except #4218, #4219 | covered except #4226, #4227 | not applicable |
| Grid edit, paste, set cells, insert row | covered except #4206, #4217, #4218, #4224 | covered except #4217, #4224, #4225 | covered except #4217; NULL cells store text by rule | covered except #4217, #4218 | not yet (MergeTree rows have no unique key) | not yet |
| No-op edit, table unchanged | covered except #4217, #4218 | covered except #4217, #4225 | covered except #4220 | covered except #4217, #4218 | not yet | not yet |
| GPUI and GTK display formatting | not covered by this suite | not covered | not covered | not covered | not covered | not covered |
