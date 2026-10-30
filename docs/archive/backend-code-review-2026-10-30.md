# Backend code review, 2026-10-30

Source: `linux` at `5a9cb8e92`. Read-only review of `app-services`, `policy`, `mcp`, `agentd`, `storage`, `transport`, `ssh` and `core` by four reviewers, then checked by hand before any change. Claims not listed under "Fixed" were read from the code and not reproduced. Each needs its own failing test before a fix.

## Fixed (pull requests)

| Finding | Pull request |
| --- | --- |
| Editing then deleting the same row made the save fail on the guarded UPDATE | [#2060](https://github.com/cozyGarage/BookiE/pull/2060) |
| Column type and default accepted a second clause (`int, DROP COLUMN x`) | [#2061](https://github.com/cozyGarage/BookiE/pull/2061) |
| A locking read (`FOR UPDATE`) carried only a read effect, so read-only and agent write rules skipped it | [#2062](https://github.com/cozyGarage/BookiE/pull/2062) |
| SQL Server default-constraint drop without `QUOTENAME`; import `OVERRIDING` text replace; single-cell update index panic; filter comment swallowed `)` | [#2063](https://github.com/cozyGarage/BookiE/pull/2063) |
| ClickHouse table functions and PostgreSQL slot copies missing from the administrative lists | [#2064](https://github.com/cozyGarage/BookiE/pull/2064) |
| A bundle import could drop or swap a pinned certificate authority | [#2065](https://github.com/cozyGarage/BookiE/pull/2065) |
| UPDATE and DELETE listed only their first table, so approvals, audit targets and the MyISAM preview check missed joined tables | [#2066](https://github.com/cozyGarage/BookiE/pull/2066) |
| A write preview ran when the connection's engine could not be identified | [#2067](https://github.com/cozyGarage/BookiE/pull/2067) |
| A named pipe at a TLS client certificate or key path blocked the connect | [#2070](https://github.com/cozyGarage/BookiE/pull/2070) |
| The SSH forwarding loop stopped on a transient `accept` error while the tunnel reported healthy | [#2071](https://github.com/cozyGarage/BookiE/pull/2071) |
| Query history had no row cap and unbounded error text | [#2072](https://github.com/cozyGarage/BookiE/pull/2072) |
| An unanswered SSH host-key or password prompt never timed out | [#2073](https://github.com/cozyGarage/BookiE/pull/2073) |
| Malformed and unknown MCP tool calls did not count against the rate limit | [#2074](https://github.com/cozyGarage/BookiE/pull/2074) |

The audit journal never rotates; [ADR 0026](../decisions/0026-audit-journal-segments.md) proposes size-rotated, chained segments and is waiting for acceptance.

Tests added without product changes: the agent denial matrix and script splitting ([#2068](https://github.com/cozyGarage/BookiE/pull/2068)); result spool, enum input, reconnect and editor limit edge cases ([#2069](https://github.com/cozyGarage/BookiE/pull/2069)).

## Findings revised after checking

- `app-services/src/database_backup.rs:60`: the secret is read before the guard, but a test with a refused restore could not tell the order apart, because a missing keyring entry becomes an empty password and the SSH key file is read later. No change made.
- `app-services/src/filter_settings.rs:53`: the stored raw filter is text the user typed into a field that is meant to take raw SQL, so a size cap is a product choice rather than a defect.
- `storage/src/query_history.rs:475`: `delete_many` and `fetch_by_ids` handled 40,000 ids in a test; the bundled SQLite allows more bound variables than the reviewer assumed.
- `policy/src/guard.rs:290`: the unbounded approval wait applies only to callers that pass no control object, which are interactive GUI approvals; a default timeout there is a product choice.

- `mcp/src/bridge.rs:333`: the row cap is applied after the driver returns, but drivers already stop at `MAX_QUERY_ROWS` (1,000,000) and the result byte budget. Memory is bounded, only wasteful; lower priority.
- `core/src/pagination.rs:81`: keyset paging runs on primary-key columns only, which cannot be NULL outside SQLite, so the other drivers' NULL ordering is not reachable today.
- `policy/src/classify.rs:247,405` and the MCP preview lookup are fixed above.

## Open, by priority

Bugs and risks:

1. `core/src/sql_lex.rs:21`: the legacy splitter ignores SQLite backtick and bracket quoting.
2. `ssh/src/lib.rs:749`: `channel_open_direct_tcpip` has no timeout and no cap on accepted connections.
3. `app-services/src/named_filter_presets.rs:85`: the preset cap and duplicate check run outside the update.
4. `app-services/src/column_widths.rs:132`: `forget_connection` stops at the first failed update; the files have no caps.

Missing tests: see the reviewers' lists in the pull request descriptions as each fix lands; the highest value ones are locking reads on every driver, keyset NULL boundaries per driver, sqlite splitter quoting, spool page above 64 KiB, bundle bounds and depth limits, and torn audit journal tails.

Refactors:

1. `mcp/src/tools.rs` `dispatch_structured` (218 lines), `policy/src/guard.rs` `resolve_authorization` (123), `policy/src/classify.rs` `classify_statement_details` (123), `core/src/filter.rs` `build_rule_sql` (119), `core/src/pagination.rs` `keyset_where_comparison` (99), `core/src/sql_ddl/diff.rs` (137 and 109).
2. `policy/src/guard/connection.rs` is 1,192 of 1,200 lines with four near-identical `execute_in_transaction*` and `Transaction::execute*` groups.
3. Two lexers in `core` (`sql_lex.rs` and `sql_syntax/script`) with different dialect rules.
4. `storage/src/connections.rs` (1,117 lines, mostly inline tests) and `secrets.rs` (about 20 near-identical functions).
5. `core/src/sql_ddl/tests.rs` is exactly 1,200 lines.
