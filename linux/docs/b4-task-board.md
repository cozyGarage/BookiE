# B4 task board: transport and sessions

Surveyed on 2026-09-27 at `5fac54059` (source version 0.1.5). This board splits
the open B4 work in [the 0.2 sprint](bookie-0.2-sprint.md) into small tasks that
separate agents can take in parallel.

## September 28 continuation

Reviewed at `85fecbe0b`. See the [commit archive](sprint-review-2026-09-28.md)
and [active order / Luna packets](bookie-0.2-sprint.md#b4-next-order).
A/B, C1–C5, D, G1/G2/G4 and H have recorded implementation/regression evidence;
B4 remains open for C6, E, F, G3/G5 and active I tasks. A5's monitor follow-up is F9.
Existing results apply to their recorded revisions; this review reran no tests.

Current UI target: Arch/Omarchy/Hyprland native Wayland. GNOME desktop and Debian
installed acceptance, including I1, are the required next phase after Arch. The GTK/libadwaita build stack
remains. Use one bounded task per Luna handoff and serialize shared editor files.

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

## Lane A: built-in SSH — implementation delivered; A5 monitor follow-up open

Files: `crates/ssh/src/lib.rs`, `crates/ssh/tests/agent_auth.rs`.

| # | Task | Tier | Status |
|---|---|---|---|
| A1 | The unknown-key error advises connecting once with `ssh`, which writes `~/.ssh/known_hosts`. The built-in client reads only its own `known_hosts`. Correct the message. | unit | done, `d8e3479fc` |
| A2 | Real-server tests for password, wrong password, rejected key and an encrypted key with passphrase. | ssh | done, `543bed77d` |
| A3 | A real two-hop chain succeeds, and a changed key on the second hop returns a host-key mismatch. | ssh | done, `543bed77d` |
| A4 | Keyboard-interactive login is not supported. Return a clear error and test it. | unit | done, `12bd158de` |
| A5 | Add keepalive and expose tunnel loss, so a dead bastion triggers reconnect instead of driver timeouts. | unit, then release | done, `6fca02c24`. Keepalive and `SshTunnel::is_closed()` land in `crates/ssh`; wiring a reconnect from the transport layer on `is_closed()` remains open as F9. This does not yet prove the complete reconnect behavior in A5. |

## Lane B: system OpenSSH — done (2026-09-27)

Files: `crates/ssh/src/openssh/*`, `crates/ssh/tests/openssh_*.rs`, `crates/transport/src/route.rs`.

| # | Task | Tier | Status |
|---|---|---|---|
| B1 | `route.rs:105` passes a new `CancellationToken`, so a caller's cancel never reaches the connect. Pass the caller's token. Test that cancelling during a prompt stops the master and removes its directory. | unit (fake ssh) | done, `9746d9a05` |
| B2 | If the app is killed, the `ssh -M` master keeps running until the next launch. Set a parent-death signal when spawning it. Test by killing the parent. | unit | done, `9746d9a05` |
| B3 | A `ProxyJump` line in the SSH config reaches the target. The current jump test only asserts failure. | ssh | done, `bfa4be264` |
| B4 | Real tests for a changed host key, key auth, agent auth and passphrase auth. | ssh | done, `528f5b3ad` |
| B5 | `OpenSshAuth::KeyboardInteractive` cannot be selected (`route.rs:73-82`). Remove it or make it saveable. | unit | done, `f248ec6d9`. Removed: nothing in transport/storage/app could ever construct it, and wiring it end to end was disproportionate to a mode nothing exposes. |

## Lane C: TLS through the tunnel — C1-C5 done (2026-09-27), C6 open

Files: `crates/drivers/{clickhouse,mongodb,redis,mssql}`, `crates/driver-tls-tests`, `scripts/test-driver-tls.sh`.

| # | Task | Tier | Status |
|---|---|---|---|
| C1 | ClickHouse checks the certificate against the tunnel's 127.0.0.1, so Verify CA and Verify Full fail through SSH. Use the service hostname. | unit | done, `608525079` |
| C2 | The same for MongoDB. | unit | done, `b83e62618` |
| C3 | The same for Redis. | unit | done, `8fd527f2f` |
| C4 | SQL Server has no TLS test. Add it to the TLS fixture with verify, wrong CA and wrong hostname cases. | driver-tls | done, `cb84d0ec3` |
| C5 | Add no-plaintext-fallback tests for MySQL, ClickHouse and SQL Server. | driver-tls | done, `cb84d0ec3` |
| C6 | Prove TLS through SSH for MySQL's socket forward and SQL Server's identity. Needs C4 and a bastion in the fixture. | driver-tls | open, C4 is now available |

## Lane D: policy sessions — D1-D8 done (2026-09-28)

Files: `crates/policy/src/guard/session.rs`, `crates/policy/src/guard_tests_session.rs`. One agent, in order.

| # | Task | Tier | Status |
|---|---|---|---|
| D1 | A PostgreSQL COMMIT on an aborted transaction is audited as committed, although the server rolled back. | driver | fixed, `6ae98c0ed`; the real-Postgres gap is closed by `01f892ac2`, which confirms it unmodified. |
| D2 | A failed COMMIT keeps a batch open after the engine has already ended the transaction, for example a deferred foreign key failure. | driver | fixed, `29b0a6b02`; the real-Postgres gap is closed by `01f892ac2`, which confirms it unmodified. |
| D3 | A statement on a retired session is sent and fails as an ambiguous error, which turns off governed writes. Refuse it before dispatch. | unit | done, `8428263d3` |
| D4 | A `Disconnected` error retires the session and marks its transaction uncertain. | unit | done, `8428263d3` |
| D5 | Audit or refuse COMMIT without BEGIN and a nested BEGIN. | unit | done, `8428263d3` |
| D6 | Cover a denied, cancelled or timed-out statement inside a transaction followed by COMMIT, and ROLLBACK AND CHAIN. | unit | done, `6ae98c0ed` |
| D7 | Cancelled, timed-out and unknown outcomes on the session path write the required audit state. | unit | done, `29b0a6b02` |
| D8 | A guarded session on real PostgreSQL: BEGIN, UPDATE and close write an audited ROLLBACK and leave the row unchanged. | driver | done; the mocked-driver unit test (`closing_a_session_with_an_open_transaction_rolls_it_back_and_audits_it`) is now backed by a real-Postgres reproduction in `01f892ac2`. |

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
| F9 | Consume the built-in `SshTunnel::is_closed()` state through transport/connection-monitor ownership, then retire old editor sessions and reconnect. A5 added the API but no consumer. Coordinate with F4; reproduce bastion loss against a real SSH fixture. | unit, ssh/release |

F2, F3 and F5 share editor files and go to one agent in order.

## Lane G: agentd and MCP — G1/G2/G4 done (2026-09-27), G3/G5 open (need decision 7 / wave 2)

Files: `crates/agentd`, `crates/mcp`, `crates/release-tests`.

| # | Task | Tier | Status |
|---|---|---|---|
| G1 | "agentd refuses unknown host keys" is backed only by a constant assert, and the release test uses `Learn`. Test with an empty `known_hosts`: refused, and no file written. | release | done, `96cd2f8f7` |
| G2 | Race test for key material rotated between the digest and the connection, pending since the 2026-09-17 baseline review. | unit | done, `256bf0572`. Fixed by recomputing the session-material digest after `establish()` succeeds and caching under that key. |
| G3 | Refuse reuse of a cached connection with unverifiable key material. Cover digest failure before cache lookup and after connect; the latter still retains the old digest. Decision 7. | unit | open |
| G4 | MCP shutdown during a write records a Cancelled or Unknown audit outcome. | mcp tests | done, `eb3682068`. Force-shutdown deadline now derives from `query_timeout_secs + 5s` instead of a hardcoded 5s. |
| G5 | agentd through OpenSSH: an unattended decline and a successful connect. | ssh | open |

## Lane H: driver cancellation and refusal — H1-H4 done (2026-09-27/28)

Files: driver `src/lib.rs` and tests for the engines named.

| # | Task | Tier | Status |
|---|---|---|---|
| H1 | ClickHouse, DuckDB, MongoDB and Redis refuse `open_session`, one test each. | unit | done, `1059edfb6`. Behavior was already correct; added regression tests only. |
| H2 | A MySQL session cancel returns Cancelled and leaves the session usable. | driver | done, `2c7321638`. Behavior was already correct; added a real-MySQL regression test. |
| H3 | DuckDB and MongoDB timeouts return an unknown outcome. | unit | done, `33ed55080`. Behavior was already correct; added regression tests only. |
| H4 | SQL Server retirement reports to the connection monitor instead of waiting for the next 30 second ping. | unit | done, `0aef65e82`. `PolicyGuard::caught_read`/`caught_write` (`crates/policy/src/guard/panic_boundary.rs`) now report a `DriverError::Disconnected` result to `ConnectionFaultSink::connection_became_unusable`, the same fast-path hook that already fired on a caught panic. This reaches the app's `Arc<Notify>`-based connection monitor fast path (`crates/app/src/services/connection_monitor.rs`) for any driver, including SQL Server after `retire()`, without a `crates/drivers/mssql` change. |

## Lane I: packaging and docs

| # | Task | Tier |
|---|---|---|
| I1 | `packaging/debian/rules` does not build/install `tablepro-askpass`, although `scripts/build-deb.sh` already does. Fix the rules recipe and validator check in the required GNOME/Debian phase after Arch. | packaging |
| I2 | Flatpak OpenSSH behavior, and reconcile `connections.md` with the sprint doc. Decision 2. | unit, docs |
| I3 | Update the dated no-askpass claim in `upstream-sync.md`; distinguish SSH transport/auth fixtures from per-driver TLS-through-SSH evidence in the tier docs. The drivers layer invokes the SSH runner; this does not prove every driver uses a tunnel. | docs |
| I4 | Done in the September 28 documentation review: manual checks now cover Stop/timeout, failed COMMIT retry, close/Disconnect, retired sessions and connection-specific write blocking. Runtime boxes remain unchecked. | docs |
| I5 | Audit record for tunnel setup and host-key refusal. Decision 6. | unit |

## Remaining waves (September 28)

1. E1/E2 and G3; F1 and F3/F5/F2 in one editor stream. Keep recorded decisions.
2. F4/F9, F6/F8, C6/G5, then I2/I3/I5 and F7 after the lifecycle work.
3. Installed manual checks on Arch/Omarchy native Wayland. Automated tiers do not
   tick these boxes. Then complete the required GNOME desktop on Debian phase,
   including recipe I1 and installed Debian checks.

Completed lane tasks are archived evidence, not a request to reimplement them.
Review current callers and record a failure before changing behavior. Preserve
one regression and the existing tier ownership for every confirmed fix.
