# Non-UI code audit, October 29

This record covers a read-only audit of every crate below `apps/linux`, at `linux` `e207862cc`: `core`, `policy`, `mcp`, `agentd`, `storage`, `ssh`, `transport`, `dump`, `app-services` and the eight drivers. It asks whether the code still follows [AGENTS.md](../../AGENTS.md) and the accepted ADRs after the recent moves between crates. It proves only that source and date. Open items belong on [Project #3](https://github.com/users/cozyGarage/projects/3); nothing here has been filed.

## Method

- The repository guards all passed: crate boundaries, comment lines, function size, bounded operations, moved paths, doc symbols, panic sites and file size. `cargo clippy --workspace --all-targets -D warnings` and `cargo deny check` also passed.
- The guards allow baselined exceptions, so five reviewers read the code in parallel, one area each (core, policy with mcp and agentd, storage with ssh, transport and dump, app-services, drivers). No code was run by them.
- The findings ranked highest were rechecked against the source by the author of this record. Items marked "not verified" were reported by a reviewer and not rechecked.

## Result in one paragraph

No reviewer found a confirmed guard bypass, a write classified as a read in the engines that have classification rules, a production `unwrap`, a secret in argv or logs, or a crate-boundary breach. The security pipeline is intact: every `Connection` method is overridden by `PolicyGuard`, audit failure fails closed, and an unresolved write outcome survives reconnect through the durable journal. The defects found are gaps in rule coverage, a few unbounded reads, one file-mode inconsistency, some code that belongs in another crate, and leftover bloat from the moves.

## Verified findings

Ranked by impact. "Verified" means the author read the cited code.

### Medium

1. **Database dump is created world-readable.** `pg_dump` writes the `.partial` file under the process umask (`crates/dump/src/postgres.rs:48-52`). The dump written during the 2026-10-29 run had mode 0644, while the CSV, HTML and Excel exports were 0600. A backup holds complete table data. Create the file 0600 first, or run the tool in a 0700 directory.
2. **Read-scoped agents can reach server-side file and network functions on ClickHouse and SQL Server.** `is_host_or_file_access_function` and the administrative lists in `crates/policy/src/effects_classification.rs:222-301` cover only PostgreSQL, MySQL, SQLite and DuckDB. ClickHouse `file()`, `url()`, `s3()`, `remote()` and SQL Server `OPENROWSET`, `OPENQUERY` appear in no list and classify as a plain read. Add the engine entries, and record user-defined functions with side effects as a known limitation.
3. **A read-scoped MCP token is enforced only by the classifier.** The scope check runs once in `crates/mcp/src/bridge.rs:272` and the guard's agent principal carries no scope, so no server-side read-only session backs it. Together with finding 2, any classifier gap becomes a write path for a read token. Open a read-only session for read-scoped principals where the engine supports it.
4. **The `explain_query` tool description is wrong.** `crates/mcp/src/server.rs:317` says plan rows are withheld from read-scoped agents. `explain_query` in `bridge.rs:355-363` has no such check; it runs the statement through the normal query path, and masking applies only when the masking setting is on. Enforce the claim or change the text.
5. **SQL Server explicit transaction leaves a transaction open if `COMMIT` fails.** `execute_in_transaction_checked` (`crates/drivers/mssql/src/lib.rs:470-523`) rolls back on every statement error but ends with `exec_simple(COMMIT).await?`. A failed `COMMIT` returns while the shared client still holds the transaction, and the next call reuses that client. Roll back or retire the client on that error, and set `XACT_ABORT ON`.
6. **Redis builds its URL from the raw host.** `crates/drivers/redis/src/lib.rs:65` formats `{scheme}://{auth}{host}:{port}/...` with no validation or IPv6 brackets. MongoDB validates with `uri_host`. A host containing `@`, `/` or `#` changes the URL, and `#` is how Redis toggles `insecure` TLS. Validate the host or build the connection info structurally.
7. **Dead UI code in `core`.** `crates/core/src/shortcut_label.rs` and its re-export at `lib.rs:68-70` have no user anywhere in `crates`, `apps` or `tests`. It is GTK shortcut logic, which also breaks the rule that `core` holds domain types and driver traits only. Delete it.
8. **Application file handling lives in `core`.** `export/`, `import/`, `text_file.rs` and `sql_format/` bring `csv`, `calamine`, `rust_xlsxwriter`, `encoding_rs`, `tempfile`, `sqlformat` and `libc` into every driver and `bookie-agentd`. ADR 0018 places GTK-free application logic in `app-services`. Moving them needs care because the app, `drivers/clickhouse` tests and `tests/release` use them.
9. **No cancellation or server timeout for MongoDB and Redis.** Neither crate uses `OperationControl` or `maxTimeMS`; only connect timeouts exist. ADR 0005 expects cancellation to reach the operation. Record the exception or add the server-side limit. Not verified: whether `docs/known-limitations.md` already records it.

### Low

- The audit journal reads the whole legacy file into memory at startup (`crates/storage/src/audit_journal.rs:354`, verified) and has no rotation or size cap. `known_hosts` revocation checks and `app-services` state-file reads are also unbounded (reported, not rechecked).
- `core` keeps `pinned_fingerprint` (`crates/core/src/tls.rs:71`), a persisted TLS field that no driver honours; the dump crate rejects it (`crates/dump/src/postgres.rs:157`, verified).
- `crates/app-services` holds `thread_local!` registries (`change_tracker.rs:608`, verified, and `structure_tracker.rs`) and `Rc`-based single-flight, so they cannot be used from `bookie-agentd` or a tokio task. ADR 0018 intends a shared facade for both roots.
- 34 production `Result<_, String>` signatures in `app-services` (`browse_query.rs`, `csv_import_mappings.rs`, `state_file.rs`) conflict with the typed-error rule across crate boundaries.
- `er_diagram/read.rs:45-47` records any non-cancel error as a skipped table, including a policy denial or a lost connection, so the diagram looks partial with no reason.
- A built-in SSH revoked host key is reported as `KnownHostsIo` and ends up as a generic connection failure, while the OpenSSH path reports `HostKeyRevoked`.
- Private-key and TLS-key bytes are read into a plain `Vec<u8>` (`crates/transport/src/session_material.rs:110-143`) and not zeroized.
- The built-in tunnel's local TCP port has no peer check and no cap on concurrent forwards (`crates/ssh/src/lib.rs:469`).
- Leftover from the moves: 12 re-export shims in `apps/linux/app/src/services/mod.rs`, test-only methods and a different `FLUSH_TIMEOUT` under `cfg(test)` in `app-services`, and an unused `_is_params_exec` parameter on `PolicyGuard::authorize`.
- Duplicated quoting helpers: `quote_ident` and `qualified` are copied in the MySQL, MSSQL, SQLite and DuckDB drivers while `core::sql_dialect::quote_ident` already covers all dialects. No escaping bug was found in any copy.
- ClickHouse inlines value literals through a hand-written scanner instead of binding (`crates/drivers/clickhouse/src/lib.rs:733-850`). The escaping is correct today; the property tests that would keep it so are missing.
- `#[allow(clippy::too_many_arguments)]` in `crates/ssh/src/lib.rs:419` and `crates/core/src/sql_ddl/diff.rs:24`, which AGENTS forbids.
- `Value::MISSING_FIELD` encodes a MongoDB sentinel as a magic string in `Undecodable` (`crates/core/src/query.rs:120`). A typed marker would be safer under ADR 0007.

## Standing debt the guards hide

The guards pass because their baselines absorb the debt. The reviewers counted comments against the "no comments" rule in production code: about 750 lines in `core`, about 450 in the drivers, about 200 in `policy`, `mcp` and `agentd`, 189 in `storage`, and a similar number in `app-services`. The comment baseline holds 4,069 lines in 185 files. Several files sit within 100 lines of the 1,200-line cap (`crates/policy/src/guard/connection.rs` at 1192, `crates/drivers/sqlite/src/lib.rs` at 1175, `crates/drivers/postgres/src/lib.rs` at 1152). The function-size baseline lists 66 files; `PolicyGuard::resolve_authorization` is about 115 lines.

## Checked and clean

- Crate dependency direction, and no GTK dependency in `app-services` or any crate below it.
- No `unwrap`, `expect`, `panic!` or `todo!` in production paths in any audited crate.
- Identifier quoting in every driver, values bound everywhere except ClickHouse, `LIMIT` and `OFFSET` as `u64`.
- TLS verification, hostname pinning through tunnels, and empty-password handling in every driver.
- Result budgets and cancellation in `core`; pool session reset in PostgreSQL and MySQL.
- Dump subprocess secrets (passfile, cleared environment, no argv secrets), OpenSSH hardening, host-key refusal on unknown or changed keys, bundle encryption, and atomic state-file writes with 0600 permissions.
- MCP token handling, loopback-only binding, body limits, deny-by-default allowlists, and audit fail-closed behaviour.

## Not reviewed

Decoders against ADR 0007 line by line (`postgres/decode.rs`, `mysql/decode.rs`, `mssql/codec.rs`, `clickhouse/temporal.rs`, `mongodb/codec.rs`), the `#[ignore]` inventory against the isolated-test registrations, test assertion text, CSV import and export formatting (they live in `core`, not `app-services`), and anything under `apps/linux`.
