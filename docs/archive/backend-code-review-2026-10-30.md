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

Tests added without product changes: the agent denial matrix and script splitting ([#2068](https://github.com/cozyGarage/BookiE/pull/2068)); result spool, enum input, reconnect and editor limit edge cases ([#2069](https://github.com/cozyGarage/BookiE/pull/2069)).

## Findings revised after checking

- `mcp/src/bridge.rs:333`: the row cap is applied after the driver returns, but drivers already stop at `MAX_QUERY_ROWS` (1,000,000) and the result byte budget. Memory is bounded, only wasteful; lower priority.
- `core/src/pagination.rs:81`: keyset paging runs on primary-key columns only, which cannot be NULL outside SQLite, so the other drivers' NULL ordering is not reachable today.
- `policy/src/classify.rs:247,405` and the MCP preview lookup are fixed above.

## Open, by priority

Bugs and risks:

1. `policy/src/guard.rs:290`: an approval with no control object has no timeout.
2. `app-services/src/database_backup.rs:60`: the keyring secret and SSH route are read before the policy guard; failures there write no audit record.
3. `app-services/src/filter_settings.rs:53`: stored `extra_sql` has no size or count cap, unlike named presets.
4. `core/src/sql_lex.rs:21`: the legacy splitter ignores SQLite backtick and bracket quoting.
5. `transport/src/lib.rs:295`: client certificate and key files are opened without `O_NONBLOCK`; a FIFO blocks the task.
6. `ssh/src/lib.rs:728`: the forwarder loop exits on a transient `accept` error while the tunnel still reports healthy.
7. `ssh/src/lib.rs:749`: `channel_open_direct_tcpip` has no timeout and no cap on accepted connections.
8. `ssh/src/handshake.rs:15`: the handshake clock stops during a prompt with no maximum.
9. `storage/src/audit_journal.rs:354`: the whole journal is read at start and never rotates.
10. `storage/src/query_history.rs:319,475`: no row cap, unbounded error text, one bound variable per id.
11. `mcp/src/tools.rs:58`: cheap malformed calls run before the rate limit and take a blocking lock.
12. `app-services/src/named_filter_presets.rs:85`: the preset cap and duplicate check run outside the update.
13. `app-services/src/column_widths.rs:132`: `forget_connection` stops at the first failed update; the files have no caps.

Missing tests: see the reviewers' lists in the pull request descriptions as each fix lands; the highest value ones are locking reads on every driver, keyset NULL boundaries per driver, sqlite splitter quoting, spool page above 64 KiB, bundle bounds and depth limits, and torn audit journal tails.

Refactors:

1. `mcp/src/tools.rs` `dispatch_structured` (218 lines), `policy/src/guard.rs` `resolve_authorization` (123), `policy/src/classify.rs` `classify_statement_details` (123), `core/src/filter.rs` `build_rule_sql` (119), `core/src/pagination.rs` `keyset_where_comparison` (99), `core/src/sql_ddl/diff.rs` (137 and 109).
2. `policy/src/guard/connection.rs` is 1,192 of 1,200 lines with four near-identical `execute_in_transaction*` and `Transaction::execute*` groups.
3. Two lexers in `core` (`sql_lex.rs` and `sql_syntax/script`) with different dialect rules.
4. `storage/src/connections.rs` (1,117 lines, mostly inline tests) and `secrets.rs` (about 20 near-identical functions).
5. `core/src/sql_ddl/tests.rs` is exactly 1,200 lines.
