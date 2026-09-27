# B4 task board: transport and sessions

Surveyed on 2026-09-27 at `5fac54059` (source version 0.1.5). This board splits
the open B4 work in [the 0.2 sprint](bookie-0.2-sprint.md) into small tasks that
separate agents can take in parallel.

## Rules for every task

1. Audit the named behavior and cite the code.
2. Write the failing reproducer first, in the lowest tier that can show it.
   Record the failure message.
3. Fix the root cause with the smallest change. Keep the reproducer as a
   permanent regression test.
4. Run the affected crate tests, Clippy with `-D warnings`, `cargo fmt`, and the
   file-size and function-size guards. Regenerate `ignored-tests.md` when an
   `#[ignore]` test is added.
5. Add a changelog line only for user-visible behavior. Commit one fix per
   commit.
6. Stay inside the lane's files. Tasks in the same lane run in order.

A task that names a decision follows the recorded decision.

## Decisions

Decided on 2026-09-27.

| # | Question | Decision |
|---|---|---|
| 1 | The GUI built-in SSH client trusts an unknown host key without asking (`UnknownHostKey::Learn` in `app/src/services/database_service.rs:179` and `connection_monitor.rs:210`). agentd refuses. | The GUI asks before trusting a new host key, as the OpenSSH path does. |
| 2 | With "Use system OpenSSH" inside Flatpak, the sprint doc says the app falls back to the built-in client, `connections.md` says it fails, and the code does neither. | Refuse with a clear message. Both docs describe that behavior. |
| 3 | One unknown write outcome turns off governed writes for every connection until restart. | Turn off governed writes only for the affected connection, restore them when that connection restarts, and document the rule. |
| 4 | `SET autocommit=0`, `SET IMPLICIT_TRANSACTIONS ON` and `XA START` are not treated as transaction control. | Refuse them on shared connections. Session handling is unchanged for now. |
| 5 | A batch such as `BEGIN; UPDATE ...` without COMMIT is allowed on a shared connection. | Refuse unterminated batches on shared connections for now. |
| 6 | Tunnel setup and host-key refusal write no audit record. | Write audit records for both. |
| 7 | agentd reuses a cached connection when it cannot verify the SSH key material (`agentd/src/lib.rs:312`). | Refuse it as a weaker fallback. |

## Lane A: built-in SSH

Files: `crates/ssh/src/lib.rs`, `crates/ssh/tests/agent_auth.rs`.

| # | Task | Tier |
|---|---|---|
| A1 | The unknown-key error advises connecting once with `ssh`, which writes `~/.ssh/known_hosts`. The built-in client reads only its own `known_hosts`. Correct the message. | unit |
| A2 | Real-server tests for password, wrong password, rejected key and an encrypted key with passphrase. | ssh |
| A3 | A real two-hop chain succeeds, and a changed key on the second hop returns a host-key mismatch. | ssh |
| A4 | Keyboard-interactive login is not supported. Return a clear error and test it. | unit |
| A5 | Add keepalive and expose tunnel loss, so a dead bastion triggers reconnect instead of driver timeouts. | unit, then release |

## Lane B: system OpenSSH

Files: `crates/ssh/src/openssh/*`, `crates/ssh/tests/openssh_*.rs`, `crates/transport/src/route.rs`.

| # | Task | Tier |
|---|---|---|
| B1 | `route.rs:105` passes a new `CancellationToken`, so a caller's cancel never reaches the connect. Pass the caller's token. Test that cancelling during a prompt stops the master and removes its directory. | unit (fake ssh) |
| B2 | If the app is killed, the `ssh -M` master keeps running until the next launch. Set a parent-death signal when spawning it. Test by killing the parent. | unit |
| B3 | A `ProxyJump` line in the SSH config reaches the target. The current jump test only asserts failure. | ssh |
| B4 | Real tests for a changed host key, key auth, agent auth and passphrase auth. | ssh |
| B5 | `OpenSshAuth::KeyboardInteractive` cannot be selected (`route.rs:73-82`). Remove it or make it saveable. | unit |

## Lane C: TLS through the tunnel

Files: `crates/drivers/{clickhouse,mongodb,redis,mssql}`, `crates/driver-tls-tests`, `scripts/test-driver-tls.sh`.

| # | Task | Tier |
|---|---|---|
| C1 | ClickHouse checks the certificate against the tunnel's 127.0.0.1, so Verify CA and Verify Full fail through SSH. Use the service hostname. | unit |
| C2 | The same for MongoDB. | unit |
| C3 | The same for Redis. | unit |
| C4 | SQL Server has no TLS test. Add it to the TLS fixture with verify, wrong CA and wrong hostname cases. | driver-tls |
| C5 | Add no-plaintext-fallback tests for MySQL, ClickHouse and SQL Server. | driver-tls |
| C6 | Prove TLS through SSH for MySQL's socket forward and SQL Server's identity. Needs C4 and a bastion in the fixture. | driver-tls |

## Lane D: policy sessions

Files: `crates/policy/src/guard/session.rs`, `crates/policy/src/guard_tests_session.rs`. One agent, in order.

| # | Task | Tier |
|---|---|---|
| D1 | A PostgreSQL COMMIT on an aborted transaction is audited as committed, although the server rolled back. | driver |
| D2 | A failed COMMIT keeps a batch open after the engine has already ended the transaction, for example a deferred foreign key failure. | driver |
| D3 | A statement on a retired session is sent and fails as an ambiguous error, which turns off governed writes. Refuse it before dispatch. | unit |
| D4 | A `Disconnected` error retires the session and marks its transaction uncertain. | unit |
| D5 | Audit or refuse COMMIT without BEGIN and a nested BEGIN. | unit |
| D6 | Cover a denied, cancelled or timed-out statement inside a transaction followed by COMMIT, and ROLLBACK AND CHAIN. | unit |
| D7 | Cancelled, timed-out and unknown outcomes on the session path write the required audit state. | unit |
| D8 | A guarded session on real PostgreSQL: BEGIN, UPDATE and close write an audited ROLLBACK and leave the row unchanged. | driver |

## Lane E: policy rules

Files: `crates/policy/src/rules.rs`, `crates/policy/src/transaction_control.rs`.

| # | Task | Tier |
|---|---|---|
| E1 | Implicit transaction starters. Decision 4. | unit |
| E2 | Unterminated batches on shared connections, with MySQL and SQL Server reproducers. Decision 5. | driver |

## Lane F: GUI

Files: `crates/app/src/ui/editor/*`, `crates/app/src/ui/app/*`, `crates/app/src/services/*`.

| # | Task | Tier |
|---|---|---|
| F1 | Stop is shown for engines that cannot stop a query (SQL Server, DuckDB, MongoDB, Redis). Gate it on `supports_server_cancellation`. | unit |
| F2 | Window close and Disconnect ignore an open session transaction, and the ROLLBACK may not finish before exit. | unit |
| F3 | The session state message has no session identity, so a late message can relabel a newer session. | unit |
| F4 | After an automatic reconnect, end or mark the editor sessions on that connection. | unit |
| F5 | A retired session turns its toggle off. Needs D3. | unit |
| F6 | Host-key prompt for built-in SSH. Decision 1. | unit, gtk-widgets |
| F7 | An isolated GTK test: Session on, BEGIN, "transaction open" label, toggle off shows the dialog. | gtk-widgets |
| F8 | Governed writes turn off only for the connection with the unknown outcome and return when that connection restarts. Decision 3. Document it in the manual checklist. | unit |

F2, F3 and F5 share editor files and go to one agent in order.

## Lane G: agentd and MCP

Files: `crates/agentd`, `crates/mcp`, `crates/release-tests`.

| # | Task | Tier |
|---|---|---|
| G1 | "agentd refuses unknown host keys" is backed only by a constant assert, and the release test uses `Learn`. Test with an empty `known_hosts`: refused, and no file written. | release |
| G2 | Race test for key material rotated between the digest and the connection, pending since the 2026-09-17 baseline review. | unit |
| G3 | Reuse of a cached connection with unverifiable key material. Decision 7. | unit |
| G4 | MCP shutdown during a write records a Cancelled or Unknown audit outcome. | mcp tests |
| G5 | agentd through OpenSSH: an unattended decline and a successful connect. | ssh |

## Lane H: driver cancellation and refusal

Files: driver `src/lib.rs` and tests for the engines named.

| # | Task | Tier |
|---|---|---|
| H1 | ClickHouse, DuckDB, MongoDB and Redis refuse `open_session`, one test each. | unit |
| H2 | A MySQL session cancel returns Cancelled and leaves the session usable. | driver |
| H3 | DuckDB and MongoDB timeouts return an unknown outcome. | unit |
| H4 | SQL Server retirement reports to the connection monitor instead of waiting for the next 30 second ping. | unit |

## Lane I: packaging and docs

| # | Task | Tier |
|---|---|---|
| I1 | The Debian package does not build or install `tablepro-askpass`. Add it and a validator check. | packaging |
| I2 | Flatpak OpenSSH behavior, and reconcile `connections.md` with the sprint doc. Decision 2. | unit, docs |
| I3 | `upstream-sync.md` still says there is no askpass bridge. `test-layers.json` says the drivers layer has SSH fixtures, but no driver integration test uses SSH. | docs |
| I4 | Add manual checks for Stop in a session, a timeout in a session, failed COMMIT retry, close or Disconnect with an open transaction, a retired session, and governed writes turned off. | docs |
| I5 | Audit record for tunnel setup and host-key refusal. Decision 6. | unit |

## Waves

1. Without decisions: A, B, C1 to C5, D, F1 and F4, G1, G2 and G4, H, and I1, I3 and I4.
2. After wave 1: C6, F2 with F3 and F5, F7, F8, G5, E, F6, G3, I2 and I5. Their decisions are recorded above.
3. Last: the manual checklist in the VM. Automated tiers do not tick those boxes.
