# Data-integrity suite

The data-integrity suite is the deterministic check that a value written to a database comes back through BookiE unchanged. Each case states a native type, the values to store and the comparison rule. The suite stores the values with the engine's own SQL, asks the database server to render them, reads the same rows through BookiE's governed path and compares the two.

It applies [ADR 0007](decisions/0007-type-and-value-preservation.md) and the [value contract rules](value-contract-rules.md). A pass here proves the named engine version, type, value and route. It does not prove other routes or other server versions.

## Entry points covered

The suite reads through one entry point: the registry driver from `default_registry()`, wrapped in `PolicyGuard` with an audit journal and a read-only session. Every route below calls a `PolicyGuard` method.

It does not yet cover the application-service entry points that the frontends call:

- The app-services read session: `ReadSession` and `ReadSessionStore`, including the PostgreSQL cursor browse (`PostgresTableCursor`, `ReadSession::open_table_cursor_sorted`).
- The interactive editor session (`interactive_session`), which keeps one server session for a user's statements, including `SET` commands.

Those are planned routes (see [Routes](#routes)). A defect that exists only in those layers is not caught here.

## The referee is the server

The expected value always comes from the database server, never from BookiE code:

- The table is created and filled by the server's own command-line client inside the container (`psql`, `mysql`, `mariadb`, `clickhouse-client`, `redis-cli`). For the file engines it is the engine library opened directly: `sqlx-sqlite` for SQLite and the `duckdb` crate for DuckDB, used only to run SQL and read back text, never through BookiE's driver or decoder. BookiE code does not write the values.
- The same client asks the server for its canonical rendering. Every rendered field is hex-encoded by the server, so the client output cannot be altered by quoting, newlines or character sets. Where the text rendering of a cell is not UTF-8 (for example SQLite TEXT holding `X'FF'`), only the hex rendering is used.
- BookiE reads the rows through the registry driver wrapped in `PolicyGuard`, with an audit journal and a read-only session. This is the layer under the app read session, not the app read session itself.
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
| Redis | The reply of the read command from `redis-cli --quoted-json`, which keeps the reply's nesting, prints integers as exact decimal text and escapes every non-printable byte | Same | Not applicable | `TYPE` |

Nanosecond epochs are converted to exact decimal seconds before comparison. The ClickHouse client runs with `output_format_json_quote_64bit_integers = 0`, so `toJSONString` prints 64-bit integers as numbers and an integer that BookiE carries as a JSON string cannot pass. Redis setup commands run through a Lua script (`EVAL`) that receives hex-encoded arguments, so binary values reach the server unchanged.

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

Planned routes, not run yet:

| Route | BookiE call | Consumer it stands for |
| --- | --- | --- |
| `cursor_browse` | `ReadSession::open_table_cursor_sorted` (`PostgresTableCursor` on PostgreSQL) | Table browse in the app read session |
| `session_query` | `interactive_session` statements, including a user `SET` before the read | SQL editor session with user session settings |

### Session settings

A case with `bookie_session` reads through its own connection whose session settings differ from the defaults. The harness proves each setting reached the session before it reads; a setting that did not apply breaks the case.

- PostgreSQL: the harness creates a login role for the case, grants it `pg_read_all_data` and sets each setting with `ALTER ROLE ... SET`. BookiE connects as that role. The check reads `current_setting(name)` through BookiE.
- MySQL and MariaDB: the harness sets each setting with `SET GLOBAL` before BookiE connects and restores the default afterwards. The check reads `@@GLOBAL.name` with the server client. BookiE sets its own session `time_zone` to UTC on connect, so these cases prove a non-UTC server default does not move a stored instant.
- ClickHouse: the harness creates a user for the case whose profile carries the settings (`CREATE USER ... SETTINGS`), grants it `SELECT` and drops it afterwards. BookiE logs in as that user. The check reads `getSetting(name)` through BookiE. A second ClickHouse server runs with the server time zone `America/New_York`, so `DateTime` without a zone is also read on a non-UTC server.
- DuckDB and SQLite: there is no server or login, so BookiE runs `SET name = value` (DuckDB) or `PRAGMA` (SQLite) itself on a writable session before it reads, as a user would in the editor. The check reads `current_setting(name)` or the pragma through BookiE. A read-only DuckDB session refuses `SET TimeZone` ([#4229](https://github.com/cozyGarage/BookiE/issues/4229)).
- Redis has no session settings that change how a reply is decoded.

The PostgreSQL driver sends `DateStyle = 'ISO, MDY'` and `extra_float_digits` in its startup packet, so role and database defaults for those two settings never reach BookiE's session; the setting check confirmed this. A user `SET DateStyle` or `SET extra_float_digits` belongs to the planned `session_query` route.

Redis has no tables, so its routes differ:

| Route | BookiE call | Server reply it is compared with |
| --- | --- | --- |
| `command` | The read command (`GET`, `HGETALL`, `LRANGE`, `SMEMBERS`, `ZRANGE ... WITHSCORES`, `XRANGE`, `PEXPIRETIME` and others) through `query_result_sets_controlled` | The same command run inside the server, compared with its nesting: a stream entry must be a nested list, not three flat cells |
| `full_value` | `SELECT "Value" FROM "db0" WHERE "Key" = ?` | `GET`, or no row for a missing key; for a collection, the specific refusal "collection values cannot be fetched" and no other error |
| `browse` | `fetch_rows_controlled(None, "db0", ...)` | `TYPE`, `HLEN`/`LLEN`/`SCARD`/`ZCARD`, and `GET` for values under the 4 KiB preview; a larger value must not be shown as whole |

A bulk reply that is valid UTF-8 must be `Value::Text`; other bytes must be `Value::Bytes`. Integer replies are compared as exact decimal text, so values past 2^53 cannot round. Two documented normalisations apply: the server's map reply (RESP3 in `redis-cli`) equals BookiE's alternating key and value rows (RESP2), and a case marked `unordered = true` (`SMEMBERS`, and `HGETALL` of a hashtable-encoded hash) compares its elements as a multiset. A sorted-set score may differ only in spelling (`0.1` against `0.10000000000000001`), only at score positions the case names with `scores = "single"` (`ZSCORE`) or `scores = "with_scores"` (odd elements of `ZRANGE ... WITHSCORES`), and must parse to the same double. Redis commands are not SQL, so the policy asks for confirmation; the Redis run uses a writable session and confirms, as a user would, because a read-only session refuses every command ([#4230](https://github.com/cozyGarage/BookiE/issues/4230)).

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
bookie_session = ["TimeZone = 'Pacific/Chatham'"]  # optional: BookiE reads in its own session with these settings
known_failing = [{ route = "param_select", issue = 4206, expect = "BookiE \"0\"", ids = [2, 3], servers = ["pg16"], reason = "..." }]
note = "Why this case exists or why its rule is what it is."
```

A marker shared by several cases is written once at corpus level, before the cases, and lists them:

```toml
[[known_failing]]
cases = ["binary", "varbinary", "blob"]
route = "param_select"
servers = ["mysql-8.4"]
issue = 4228
expect = "expected carrier Bytes, BookiE has"
reason = "..."
```

A case's own marker takes precedence over a corpus-level marker for the same route.

The tables are created in a fresh container or temporary file, so a case does not need cleanup.

The Redis corpus (`tests/data/integrity/redis.toml`) lists, per case, the setup commands, the read command and optional `scores`, `unordered`, `skip` and `known_failing`. `{key}` expands to the case key, an argument written `hex:...` is sent as those bytes, `seq:N:p` expands to `p0` to `pN-1` and `pairs:N:f` to `N` field and value pairs.

### Expected refusals

A route where BookiE must refuse rather than return a value carries `expect_error`:

```toml
expect_error = [{ route = "param_select", expect = "nested values cannot be edited losslessly", reason = "..." }]
```

The route must fail, and every failure detail must contain `expect`; it then counts as refused. A pass, or a failure with another message, fails the run. Use it for refusals that ADR 0007 accepts, never for wrong values.

### Comparison rules

| Rule | Comparison | Normalisation |
| --- | --- | --- |
| `exact` | BookiE's text form equals the server text | None |
| `server_cast` | The server renders BookiE's text cast back to the column type, and that equals the server text | Accepts any spelling the server parses to the same value, for exact text fallbacks such as PostgreSQL arrays. The cast runs with the case's `bookie_session` settings, and the rendering runs with the defaults. The corpus check refuses `server_cast` on a type with a modifier, such as `numeric(10,2)`, `varchar(3)` or `timestamp(0)`, because the cast would round or truncate BookiE's text into a match, and requires a `note` saying why the case uses it |
| `number` | Decimal text equality | Leading and trailing zeros only |
| `float` | Bit equality of the double parsed from the server text | NaN equals NaN |
| `float32` | Bit equality after narrowing both to single precision | For single-precision columns BookiE widens through the shortest single decimal |
| `bytes` | Lowercase hex equality | Hex case |
| `bool` | `t`, `true`, `1` and `f`, `false`, `0` | Server spelling |
| `temporal` | Text equality | Trailing zeros of fractional seconds |
| `instant` | Epoch seconds equality; a `Date` is midnight UTC and a `DateTime` is a UTC wall clock | Trailing zeros |
| `json` | Both documents are re-parsed and serialised compactly, keys in the order they first appear | Whitespace, duplicate keys (the last value wins) and escape spelling (`\u00e9` and `é` compare equal). Use `exact` where the source text itself is the value, as for PostgreSQL `json` |
| `refusal` | BookiE returns `Value::Undecodable` for a non-NULL cell | None |

Every rule first checks NULL: a server NULL must be `Value::Null`, and a non-NULL server value must not be. Then it checks the carrier, so a right-looking text in the wrong `Value` variant fails.

## Known failures

A case that fails today carries `known_failing` with the issue number, the route and `expect`, a substring that every failure detail of that case and route must contain. `ids` optionally names the exact failing rows.

- A failure that matches the marker counts as known-failing.
- A failure that does not contain `expect`, or fails on other rows than `ids`, is an unexpected failure. A marker cannot hide a second, different defect on the same route.
- A pass on a marked route fails the run with "known issue #N looks fixed" until the marker is removed.
- A partial fix shows up when the marker lists `ids`; without `ids`, a fix of some rows passes silently until the last row is fixed.
- A setup error, a referee error, or a referee that returns a different number of rows than the case has literals breaks the case. A broken case is always a failure, never a known failure.

A marker must point at an open issue. `python3 scripts/check-integrity-markers.py` reads every corpus file and checks each marker's issue state with `gh issue view`; a closed issue fails it. It runs in the GitHub `supply-chain` job with the workflow token. Locally it needs an authenticated `gh`. The Rust targets do not query GitHub.

## Adding a case

A value bug is fixed by first adding a failing corpus row:

1. Add the row with the native type and literal to `tests/data/integrity/<engine>.toml`, with the carrier and rule the ADR 0007 outcome requires.
2. Run the engine target and confirm the case fails for the reported reason.
3. If the fix is not in the same change, add `known_failing` with the issue number, an `expect` substring of the failure and, where only some rows fail, their `ids`.
4. The fix removes the marker in the same commit.

Do not change a carrier or rule to make a failing value pass. Change it only when BookiE's result is a valid ADR 0007 outcome (exact typed value, exact text fallback or explicit refusal) and say why in `note`.

## Running locally

The PostgreSQL, MySQL, ClickHouse and Redis targets need Docker. The corpus check and the SQLite and DuckDB targets need none: they use a temporary file. Build with plain Cargo.

```bash
cargo test -p bookie-data-integrity-tests --test corpus
cargo test -p bookie-data-integrity-tests --test postgres -- --include-ignored --test-threads=1 --nocapture
cargo test -p bookie-data-integrity-tests --test mysql -- --include-ignored --test-threads=1 --nocapture
cargo test -p bookie-data-integrity-tests --test clickhouse -- --include-ignored --test-threads=1 --nocapture
cargo test -p bookie-data-integrity-tests --test redis -- --include-ignored --test-threads=1 --nocapture
cargo test -p bookie-data-integrity-tests --test sqlite --test duckdb -- --nocapture
```

Each engine target also has a `copy_export_import_and_edits` test for the routes above (PostgreSQL 16 and 18, MySQL 8.4, MariaDB 11, SQLite, DuckDB; ClickHouse without the edit routes). Select one with a name filter, for example `-- --include-ignored copy_export`.

Each target prints a summary per server on stdout: pass, known-failing, refused, skipped, unexpected and fixed counts of case and route results, then every known failure with its issue. `scripts/run-driver-group.sh` passes `--nocapture`, so the summary is in the CI log of a passing run. Each Docker test starts its own container from the images listed in `ci/test-images.txt` for that driver group (PostgreSQL 16 and 18, MySQL 8.4.13, MariaDB 11, ClickHouse 24.8, Redis 7.4.12) and removes it at the end. With `BOOKIE_TEST_RUN_ID` set, containers carry the run label that `scripts/cleanup-testcontainers.py` removes.

## CI tier

The corpus check and the SQLite and DuckDB targets run in the sandbox tier. The Docker targets belong to the driver tier: `scripts/run-driver-group.sh <driver>` runs `tests/data-integrity/tests/<driver>.rs` after that driver's own targets, so they run in the `driver-core` job (PostgreSQL and MySQL) and the `driver-analytics` job (ClickHouse and Redis) of the Forgejo and GitHub gates.

## Coverage

Route results in the last local run (pass, known-failing, refused): PostgreSQL 16 and 18 771, 43, 98 each; MySQL 8.4 547, 16, 69; MariaDB 11 583, 14, 71; SQLite 180, 15, 29; DuckDB 428, 37, 55; ClickHouse 24.8 307, 27, 44.

Current corpus: PostgreSQL 79 cases (pg16 and pg18); MySQL 63 cases, of which 55 run on MySQL 8.4 and 58 on MariaDB 11. SQLite 20, DuckDB 46, ClickHouse 54 (24.8, on a UTC and an America/New_York server), Redis 24 (7.4.12).

Read routes by type family. "Covered" means every read route passes on every listed server. "Refusal" means BookiE returns `Undecodable` instead of a value, which ADR 0007 accepts as a safe outcome but is not support.

| Family | PostgreSQL | MySQL 8.4 | MariaDB 11 |
| --- | --- | --- | --- |
| Integers, signed and unsigned, boundaries | covered (`oid` as exact text) | covered | covered |
| Exact decimals, max precision, beyond rust_decimal, unsigned | covered, including negative scale; bound zero loses scale on `param_select`: known-failing [#4206](https://github.com/cozyGarage/BookiE/issues/4206) | covered (wide values as exact text) | covered |
| Floats: -0, subnormal, infinity, NaN, `DOUBLE UNSIGNED` | covered | covered | covered |
| Boolean, bit, varbit, comparison expression | covered | covered; a bound Bool echoes as an integer because the server reports a plain integer column | as MySQL |
| Text: Unicode, padding, trailing spaces, NUL where allowed, 'NULL' vs NULL vs '', collations, `TINYTEXT` to `LONGTEXT` | covered | covered | covered |
| Binary, `LONGBLOB`, `CHARACTER SET binary` | covered | covered except `param_select`: known-failing [#4228](https://github.com/cozyGarage/BookiE/issues/4228) | covered |
| Date, time, timestamp, precision 0 and 3, time zones, DST, BC, infinity, zero dates and datetimes, TIME over 24 h, YEAR | covered | covered; a bound TIMESTAMP echoes as DATETIME, which the server reports | covered |
| Writer or server time zone other than UTC | covered (`Pacific/Chatham`, `America/St_Johns`) | covered (`+05:30` writer, `-03:30` global) | as MySQL |
| Interval, including `postgres_verbose` and `iso_8601` IntervalStyle | covered (exact text) | not applicable | not applicable |
| UUID | covered | not applicable | covered (`UUID` as text) |
| Network types | covered (exact text) | not applicable | covered (`INET4`, `INET6` as text) |
| JSON and JSONB: key order, duplicate keys, big numbers, `\u` escapes | covered | covered | covered (JSON is text) |
| Arrays: nested, NULL elements, empty, bounds; uuid, jsonb, timestamptz, bytea (`escape` output), bool, enum | covered (`server_cast`) | not applicable | not applicable |
| Enum, set, domain | covered | covered | covered |
| Ranges | `int4range`, `int8range` as exact text; `numrange`, `daterange`, `tsrange`, `tstzrange` and multiranges are explicit refusals (`Undecodable`) | not applicable | not applicable |
| Composite, money | explicit refusal (`Undecodable`) | not applicable | not applicable |
| Geometric, xml, tsvector, pg_lsn | covered (exact text) | geometry covered as stored bytes except `param_select`: [#4228](https://github.com/cozyGarage/BookiE/issues/4228) | covered |

| Family | SQLite | DuckDB | ClickHouse |
| --- | --- | --- | --- |
| Integers, signed and unsigned, boundaries, 128 and 256 bit | covered, including an INTEGER literal past i64 stored as REAL | covered (TINYINT to BIGINT, UTINYINT to UBIGINT; HUGEINT, UHUGEINT and UBIGINT past i64 as exact text) | covered (Int8 to Int256, UInt8 to UInt256; past i64 as exact text) |
| Exact decimals, max precision, beyond rust_decimal | not applicable (stored as REAL) | covered (DECIMAL(4,2) and (9,3) on 16- and 32-bit storage, (18,8), (38,10), wide as exact text) | covered (Decimal32, Decimal64, Decimal128, Decimal256 as exact text when wide) |
| Floats: -0, subnormal, infinity, NaN, single precision | covered | covered | covered |
| Boolean and bits | covered | BIT known-failing [#4214](https://github.com/cozyGarage/BookiE/issues/4214) | covered |
| Text: Unicode, NUL, padding, 'NULL' vs NULL vs '', collations, large | covered, including TEXT that is not UTF-8 (kept as bytes) | covered | covered (FixedString and LowCardinality(FixedString) keep NUL padding; binary String as bytes; Nullable(String) NULL, '' and 'NULL') |
| Binary | covered | covered | covered |
| Temporal | covered (declared temporal TEXT, INTEGER, REAL and BLOB storage) | covered (TIMESTAMP_S, _MS, _NS, infinity, a `TimeZone` session); TIMETZ loses its offset: known-failing [#4238](https://github.com/cozyGarage/BookiE/issues/4238) | covered (Date, Date32, DateTime64 with zones, DateTime on a New York server; DST fold is a refusal); bound Date32 outside 1970-2149 clamps: [#4215](https://github.com/cozyGarage/BookiE/issues/4215); DateTime ignores `session_timezone`: [#4236](https://github.com/cozyGarage/BookiE/issues/4236) |
| Interval | not applicable | covered (`server_cast`) | not yet |
| UUID | not applicable | covered | covered |
| JSON | not applicable | exact text, but read as `Text`: known-failing [#4231](https://github.com/cozyGarage/BookiE/issues/4231) | not yet |
| Arrays and nested values | not applicable | refusal (LIST, fixed-size ARRAY, STRUCT, MAP, UNION) | covered as JSON (Array, Tuple, named Tuple, Map), binding refused; 64-bit integers inside read as strings: known-failing [#4235](https://github.com/cozyGarage/BookiE/issues/4235) |
| Enum, LowCardinality, Nullable | not applicable | covered | covered (Enum8, Enum16) |
| Aggregate states | not applicable | not applicable | `AggregateFunction` fails the whole read: known-failing [#4237](https://github.com/cozyGarage/BookiE/issues/4237) |
| Network | not applicable | not applicable | IPv4 and IPv6 covered |
| SQLite affinity and mixed storage classes | covered (NUMERIC, untyped, STRICT ANY) | not applicable | not applicable |

Redis: strings (UTF-8, empty, the text `NULL`, NUL, binary, number-like, integer-encoded, past 2^53, 300000 bytes), hashes (small and 600 fields), lists (small and 5000 elements), sets (small and 1000 members, unordered), sorted sets with float scores, streams, an integer reply past 2^53 and a missing key are covered on the `command`, `full_value` and `browse` routes.

Engines and routes. "Refused" lists cases where BookiE refuses a value on that route instead of carrying it (for example NaN in SQL INSERT export, empty text in XLSX export, ClickHouse `max_query_size`); the run prints each refusal.

| Route | PostgreSQL 16 | MySQL 8.4, MariaDB 11 | SQLite | DuckDB | ClickHouse 24.8 | Redis |
| --- | --- | --- | --- | --- | --- | --- |
| Read through `PolicyGuard`: query, browse, param_where | covered | covered | covered | covered except #4214, #4231, #4238 | covered except #4235, #4236, #4237 | covered (command, full_value, browse) |
| Read through `PolicyGuard`: param_select | covered except [#4206](https://github.com/cozyGarage/BookiE/issues/4206) | covered except MySQL 8.4 binary echo [#4228](https://github.com/cozyGarage/BookiE/issues/4228) | covered | covered except #4214 | covered except #4215; nested values are refused as expected | not applicable |
| App read session: `cursor_browse` | planned | not applicable | not applicable | not applicable | not applicable | not applicable |
| Interactive editor session: `session_query` | planned | planned | planned | planned | planned | planned |
| Copy as TSV and paste back | covered | covered except #4221 | covered except #4221 | covered except #4214, #4238 | covered except #4235, #4236, #4237 | not yet |
| Export CSV, JSON, JSON Lines, re-read | covered except #4223 (JSON) | covered except #4223 (JSON) | covered | covered except #4214, #4223, #4238 | covered except #4235, #4236, #4237 | not yet |
| Export SQL INSERT, run into an empty table | covered | covered | covered | covered except #4214, #4238 | covered except #4237 | not applicable |
| Export to CSV or XLSX, import into an empty table | covered except #4206, #4222 | covered | covered except #4220 | covered except #4238 | covered except #4226, #4227, #4237 | not applicable |
| Grid edit, paste, set cells, insert row | covered except #4206 | covered | covered; NULL cells store text by rule | covered | not yet (MergeTree rows have no unique key) | not yet |
| No-op edit, table unchanged | covered | covered | covered | covered | not yet | not yet |
| GPUI and GTK display formatting | not covered by this suite | not covered | not covered | not covered | not covered | not covered |

Defects found while building the corpus that do not affect a route above:

- [#4229](https://github.com/cozyGarage/BookiE/issues/4229): the read-only policy parser refuses valid reads such as `CAST(? AS DateTime('UTC'))` on ClickHouse and `$1::text COLLATE "C"` on PostgreSQL. The PostgreSQL `text-collation-c` case selects the bound value as plain text because of it.
- [#4230](https://github.com/cozyGarage/BookiE/issues/4230): a read-only Redis connection refuses every command, including `GET`.
- [#4231](https://github.com/cozyGarage/BookiE/issues/4231): DuckDB `JSON` reads as `Text` while PostgreSQL `json` reads as `JsonText` (now a marked DuckDB case).
- ClickHouse `DateTime` without a zone reads as a wall clock without its zone. On the New York server two different instants at the 01:30 fold read as the same value, which a write back cannot tell apart; the `datetime-server-zone` case pins this reading against the server's own `toString`.
