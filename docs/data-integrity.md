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

- The table is created and filled by the server's own command-line client inside the container (`psql`, `mysql`, `mariadb`). BookiE code does not write the values.
- The same client asks the server for its canonical rendering. Every rendered field is hex-encoded by the server, so the client output cannot be altered by quoting, newlines or character sets.
- BookiE reads the rows through the registry driver wrapped in `PolicyGuard`, with an audit journal and a read-only session. This is the layer under the app read session, not the app read session itself.
- The harness compares BookiE's typed `Value` with the server rendering using the case's rule. The harness formats a `Value` itself; it does not call BookiE's display, copy or export code, so a formatter bug cannot hide a decoder bug.

Where a server rendering is lossy, the corpus names a different server expression, never a hand-written value. For example MySQL `CAST(float AS CHAR)` prints six digits, so the `FLOAT` case uses `CAST(v AS DOUBLE)`.

Server renderings per engine:

| Engine | Text | Bytes | Instant | Type |
| --- | --- | --- | --- | --- |
| PostgreSQL | `format('%s', v)` (the type output function, which keeps `char(n)` padding and host-only `inet`) | `encode(v, 'hex')` | `extract(epoch from v)` | `pg_typeof(v)` |
| MySQL, MariaDB | `CAST(v AS CHAR)` | `HEX(v)` | `UNIX_TIMESTAMP(v)` | `information_schema.COLUMNS.COLUMN_TYPE` |

## Routes

Each case runs through every read route:

| Route | BookiE call | Consumer it stands for |
| --- | --- | --- |
| `query` | `query_result_sets_controlled("SELECT id, v FROM t ORDER BY id")` | SQL editor batch |
| `browse` | `fetch_rows_controlled(schema, t, 0, 1000)` | Table page read |
| `param_where` | `query_params_controlled("SELECT v FROM t WHERE id = $1")` | Parameterised read of a stored column; on MySQL this is the binary protocol |
| `param_select` | `query_params_controlled("SELECT $1::type")` on PostgreSQL, `SELECT ?` on MySQL, binding the value BookiE read in `query` | A typed parameter echoed back, the class of the #4094 decoder-plan bug |

Planned routes, not run yet:

| Route | BookiE call | Consumer it stands for |
| --- | --- | --- |
| `cursor_browse` | `ReadSession::open_table_cursor_sorted` (`PostgresTableCursor` on PostgreSQL) | Table browse in the app read session |
| `session_query` | `interactive_session` statements, including a user `SET` before the read | SQL editor session with user session settings |

### Session settings

A case with `bookie_session` reads through its own connection whose session settings differ from the defaults. The harness proves each setting reached the session before it reads; a setting that did not apply breaks the case.

- PostgreSQL: the harness creates a login role for the case, grants it `pg_read_all_data` and sets each setting with `ALTER ROLE ... SET`. BookiE connects as that role. The check reads `current_setting(name)` through BookiE.
- MySQL and MariaDB: the harness sets each setting with `SET GLOBAL` before BookiE connects and restores the default afterwards. The check reads `@@GLOBAL.name` with the server client. BookiE sets its own session `time_zone` to UTC on connect, so these cases prove a non-UTC server default does not move a stored instant.

The PostgreSQL driver sends `DateStyle = 'ISO, MDY'` and `extra_float_digits` in its startup packet, so role and database defaults for those two settings never reach BookiE's session; the setting check confirmed this. A user `SET DateStyle` or `SET extra_float_digits` belongs to the planned `session_query` route.

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

The tables are created in a fresh container, so a case does not need cleanup.

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
| `json` | Both documents are re-parsed and serialised compactly, key order kept | Whitespace, duplicate keys (the last value wins) and escape spelling (`\u00e9` and `é` compare equal). Use `exact` where the source text itself is the value, as for PostgreSQL `json` |
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

All targets need Docker except the corpus check. Build with plain Cargo.

```bash
cargo test -p bookie-data-integrity-tests --test corpus
cargo test -p bookie-data-integrity-tests --test postgres -- --include-ignored --test-threads=1 --nocapture
cargo test -p bookie-data-integrity-tests --test mysql -- --include-ignored --test-threads=1 --nocapture
```

Each target prints a summary per server on stdout: pass, known-failing, skipped, unexpected and fixed counts of case and route results, then every known failure with its issue. `scripts/run-driver-group.sh` passes `--nocapture`, so the summary is in the CI log of a passing run. Each test starts its own container from the images listed in `ci/test-images.txt` for that driver group (PostgreSQL 16 and 18, MySQL 8.4.13, MariaDB 11) and removes it at the end. With `BOOKIE_TEST_RUN_ID` set, containers carry the run label that `scripts/cleanup-testcontainers.py` removes.

## CI tier

The corpus check runs in the sandbox tier. The Docker targets belong to the driver tier: `scripts/run-driver-group.sh <driver>` runs `tests/data-integrity/tests/<driver>.rs` after that driver's own targets, so they run in the `driver-core` job (PostgreSQL and MySQL) of the Forgejo and GitHub gates.

## Coverage

Current corpus: PostgreSQL 79 cases (pg16 and pg18); MySQL 63 cases, of which 55 run on MySQL 8.4 and 58 on MariaDB 11.

Read routes by type family. "Covered" means all four read routes pass on every listed server.

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

Engines and routes:

| Route | PostgreSQL | MySQL, MariaDB | SQLite | DuckDB | ClickHouse | Redis |
| --- | --- | --- | --- | --- | --- | --- |
| Read through `PolicyGuard`: query, browse, param_where | covered | covered | not yet | not yet | not yet | not yet |
| Read through `PolicyGuard`: param_select | covered except [#4206](https://github.com/cozyGarage/BookiE/issues/4206) | covered except MySQL 8.4 binary echo [#4228](https://github.com/cozyGarage/BookiE/issues/4228) | not yet | not yet | not yet | not yet |
| App read session: `cursor_browse` | planned | not applicable | not applicable | not applicable | not applicable | not applicable |
| Interactive editor session: `session_query` | planned | planned | planned | planned | planned | planned |
| Copy as TSV | not yet | not yet | not yet | not yet | not yet | not yet |
| Export CSV, JSON, JSON Lines, XLSX, SQL INSERT and re-read | not yet | not yet | not yet | not yet | not yet | not yet |
| Grid edit, insert row, set cells, paste, CSV and XLSX import | not yet | not yet | not yet | not yet | not yet | not yet |
| No-op save and export to import checksums | not yet | not yet | not yet | not yet | not yet | not yet |
| GPUI and GTK display formatting | not covered by this suite | not covered | not covered | not covered | not covered | not covered |

Defects found while building the corpus that do not affect a route above:

- [#4229](https://github.com/cozyGarage/BookiE/issues/4229): the read-only policy parser refuses valid reads such as `CAST(? AS DateTime('UTC'))` on ClickHouse and `$1::text COLLATE "C"` on PostgreSQL. The PostgreSQL `text-collation-c` case selects the bound value as plain text because of it.
- [#4230](https://github.com/cozyGarage/BookiE/issues/4230): a read-only Redis connection refuses every command, including `GET`.
- [#4231](https://github.com/cozyGarage/BookiE/issues/4231): DuckDB `JSON` reads as `Text` while PostgreSQL `json` reads as `JsonText`.
