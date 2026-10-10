# B4 task board: transport and sessions

Base scope: `aeac107a4`, branch `linux`, reviewed 2026-10-03. The active B4 integration branch advances as work lands. Use each evidence
manifest for the exact source revision and Git for the current branch tip.
The local source includes MySQL batch/result fixes indexed in the [evidence manifest]
(evidence/mysql-atomic-results-2026-10-03/manifest.json).
The [sprint](bookie-0.2-sprint.md) owns release order; [ADR 0008](decisions/0008-connection-and-session-ownership.md)
owns the decisions. Use the current Git tip for its revision because this board is updated
independently of source commits. B4 remains open. Implementation, local execution, hosted
execution and installed acceptance are separate states.

## Current Linux-tip B4 checkpoint (2026-10-10)

Tip includes #458/`Effects`, #459 engine read-only, #460 migration step 2,
#461 explain-plan masking, #463 DuckDB selected-file restrictions, #475
`MRG_MyISAM` rollback and #480 joined verdicts (AUD-11 step 3). PR #486
(DuckDB replaced-path / nonblocking open) remains open. AUD-11 step 4
(remove legacy facts and duplicate walkers) remains. Exact local verification
and acceptance boundaries are recorded below; local runs do not establish
Forgejo gate or frozen-candidate/package acceptance.

## AUD-9 query-plan masking regression (2026-10-10)

PR #456's fail-closed fallback treated plain `EXPLAIN SELECT` as an unknown
projection and redacted the plan returned by the agent `explain_query` tool.
The PostgreSQL release fixture reproduced this twice on the post-#456 source;
the pre-#456 parent passed the same fixture. Merged PR #461 keeps plan output
masked for read-scoped agents and aligns the tool description and release test
with that behavior, because a plan may contain server-generated predicates.
The policy suite passed 216 tests with 4 Docker-only tests ignored. Code tip
`d4d26c8` was rechecked on 2026-10-10: system OpenSSH 1/1, mTLS
3/3, PostgreSQL release integration 59/59 (including plan denial and both
`EXPLAIN ANALYZE` denial checks), and seven SSH/mTLS GTK scenarios passed. See
the [AUD-9 evidence](evidence/aud9-explain-plan-mask-2026-10-10/manifest.json).
This local result does not establish the Forgejo gate, hosted acceptance or
frozen-candidate/installed acceptance.

## Retest indicators (from archived checkpoints)

Keep these as “test again” signals until cleared on a current candidate SHA.
Full dated narratives are in
[B4 history](archive/b4-history.md#archived-b4-dated-checkpoints-2026-10-09-consolidation).

| Signal | Status at archive | Action |
| --- | --- | --- |
| MongoDB TLS `ConnectionRefused` on `98134709` | Intermittent; later 48/48 TLS passes on `a47b1fb` and hosted `102ef480` | Retest if TLS layer flakes again; do not treat as closed root-cause |
| Empty MySQL approval dialog (operator observation) | Not reproduced on `b4ee463` / hosted `102ef480`; cause unconfirmed | Retest on installed package / native Wayland |
| Build Linux cancelled / pending on superseded tips (`8e5d1b18`, `32b170f`, `e697824`) | Superseded by later green and rate-limited runs | Ignore SHA; retest only current tip |
| Headless Wayland smoke incomplete (`97e5f55`) | No SIGSEGV; AT-SPI not activated | Retest native Wayland PKG-1 path |
| Docker Hub rate limits on tip near `c2f3f78b9` | Infrastructure red, not product signal | Retest Build Linux when pulls succeed |

## Next B4 acceptance run

Rerun affected layers when application or driver code changes. Remaining work
includes distribution-package/native Wayland acceptance, optional MySQL engine
and side-effect coverage, Windows AD interoperability, and UI-1b. Superseded
hosted-run narratives and the MongoDB TLS intermittent failure are in
[B4 history](archive/b4-history.md#archived-b4-dated-checkpoints-2026-10-09-consolidation)
and the retest table above. Xvfb staged-binary runs do not establish
distribution-package or native Wayland acceptance.

| Work | Required evidence on the next candidate | Current boundary |
| --- | --- | --- |
| B4-7, B4-16 | Complete package-installed/native Wayland trust and reconnect acceptance | At exact Linux tip `50a4dc140`, all seven PostgreSQL SSH/mTLS GTK scenarios passed, including unknown-host decline audit, setup-failure audit, multi-hop trust, changed-key refusal and tunnel-loss retirement/reconnect. Frozen-candidate, package-installed and native Wayland acceptance remain open. See [current release evidence](evidence/b4-current-postgres-release-50a4dc140-2026-10-10/manifest.json) |
| B4-9 | Complete package-installed/native Wayland route/auth/TLS acceptance | Exact PR #480 source `fbae986` (Linux base `8517a68`) passed the PostgreSQL release layer: 1 system OpenSSH test, 3 mTLS tests, 59 release integration tests and all seven SSH/mTLS GTK scenarios. Frozen-candidate, package-installed and native Wayland acceptance remain open; the earlier MongoDB refusal cause remains undetermined. See [AUD-11 release compatibility evidence](evidence/aud11-postgres-release-fbae986-2026-10-10/manifest.json), [current Linux release evidence](evidence/b4-current-postgres-release-50a4dc140-2026-10-10/manifest.json), [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) and [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json) |
| B4-22 | Complete package-installed/native Wayland bundle audit acceptance | At exact Linux tip `d0debc6`, four GTK safety scenarios passed for sanitized export/import audit outcomes, encrypted credential restoration and cross-profile import. This was a source build under Xvfb/X11; distribution-package installed flow, frozen-candidate and native Wayland acceptance remain open. See [current-tip evidence](evidence/b4-22-bundle-current-tip-d0debc6-2026-10-10/manifest.json); the earlier `d916b3aa` run remains in [its manifest](evidence/b4-22-bundle-current-tip-d916b3aa-2026-10-10/manifest.json) |
| B4-12 | ~~PostgreSQL rollback-failure acceptance on the selected frozen candidate~~ | Hosted candidate and local current-product-source (`fc180933`) backend-termination selectors passed. The scenario confirms `TransactionRollbackFailed`, absent transactional rows and persistent identity/trigger-sequence advancement. See [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) and [local linux-tip evidence](evidence/b4-rollback-linux-tip-2026-10-09/manifest.json) |
| B4-11 | Extend failed-batch tests to optional/vendor MySQL-family engines and broader side-effect patterns | PR #475 adds `MRG_MyISAM` write-through cases for both MySQL and MariaDB. On exact Linux source tip `788ff9ddb`, the serialized `mysql_atomic` selector passed 20/20 against Docker MySQL and MariaDB fixtures. Existing coverage includes MyISAM, MEMORY, CSV, ARCHIVE, BLACKHOLE, MariaDB Aria, direct/update/delete effects, triggers, auto-increment allocation, pinned-session state and advisory locks. The Aria selector passed again on current Linux source `2b79ae3` (one focused selector); see [current Aria run manifest](evidence/mysql-aria-batch-effects-linux-2b79ae3-2026-10-10/manifest.json). Optional/vendor engines and other side-effect classes remain open; frozen-candidate and installed acceptance remain separate. See [current-tip MySQL/MariaDB run manifest](evidence/mysql-atomic-linux-788ff9ddb-2026-10-10/manifest.json), [advisory-lock evidence](evidence/mysql-advisory-lock-rollback-2026-10-09/manifest.json) and [session-state evidence](evidence/mysql-session-state-rollback-2026-10-09/manifest.json) |
| B4-17 | Complete package-installed/native Wayland trust flow | Exact PR #480 source `fbae986` (Linux base `8517a68`) passed 1 system OpenSSH test, 3 mTLS tests, 59 PostgreSQL release integration tests and all seven GTK trust scenarios, including multi-hop trust, second-hop decline, changed-key refusal and tunnel-loss recovery. This was a per-user source build under Xvfb/X11; frozen-candidate, package-installed multi-hop trust and native Wayland acceptance remain open. See [AUD-11 release compatibility evidence](evidence/aud11-postgres-release-fbae986-2026-10-10/manifest.json), [current Linux release evidence](evidence/b4-current-postgres-release-50a4dc140-2026-10-10/manifest.json), [local Arch Wayland evidence](evidence/b4-arch-package-wayland-2026-10-09/manifest.json), [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json), [native SSH evidence](evidence/b4-ssh-native-current-2026-10-09/manifest.json), and [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json) |
| B4-21 | Preserve Samba Kerberos+VerifyFull proof, then perform Windows AD interoperability | At exact Linux source tip `3b9f2c84`, both Samba AD selectors passed with verified TLS. PR #467 bounds the Kerberos connect future inside its five-second blocking-worker deadline and includes a regression for dropping a pending connect. Windows AD interoperability and distribution-package Kerberos remain open. See [current-tip Kerberos evidence](evidence/b4-21-mssql-kerberos-current-3b9f2c84-2026-10-10/manifest.json) and [PR #467](https://github.com/cozyGarage/BookiE/pull/467) |
| UI-1b | Preserve edit refusal while implementing the [per-hop secrets design](proposals/ui-1b-ssh-jump-chain-editor.md) across storage, transport, bundle compatibility and GTK | Proposal only; implementation and acceptance are open |

## Remaining tasks

Each row is a bounded task; implement engines and route variants separately.
Use existing fixtures, wrappers and the layer catalog. The release audit gives
additional privacy, value and evidence tasks without duplicating this board.

| ID | Small task and required assertion | Local layers |
| --- | --- | --- |
| I2 | **Implemented and locally tested.** Explicit saved system OpenSSH selection refuses at route resolution inside Flatpak before SSH credential resolution or driver dispatch; there is no automatic backend switch. The deterministic regression also verifies built-in SSH remains selectable inside the sandbox and system OpenSSH remains selectable outside it. Flatpak package/runtime acceptance remains separate | `quick` (includes sandbox tests) |
| I5 | **Local implementation and backend fixture coverage complete.** The shared transport boundary emits one terminal event per SSH attempt. Saved GUI opens/reconnects, connect-dialog submit/test, and daemon opens attach principal/connection identity and cancellation. Built-in and system OpenSSH real fixtures cover unknown and changed host keys; system OpenSSH also covers trusted success. Unit tests cover cancellation, audit-write failure/fail-closed behavior, and legacy journal compatibility. GTK refusal, accepted-query, and closed-port setup-failure scenarios each assert exactly one durable transport outcome. The [current-Linux seven-scenario release-binary run](evidence/ssh-gtk-audit-linux-fb82ea1-2026-10-07/manifest.json) also passed. All seven PostgreSQL SSH GTK scenarios passed in hosted run [#37708183936](https://github.com/cozyGarage/BookiE/actions/runs/37708183936) at PR #286 head `a056bf1`. Distribution-package/native Wayland acceptance remains open | `security-policy`, `ssh`, `postgres-release` |
| F7 | **Local implementation, unit/widget, and PostgreSQL GTK flow verified.** Toggle-off confirmation keeps Session active; Cancel retains the same open transaction; explicit `ROLLBACK` finishes before session close and leaves no rows persisted | `widgets`, `postgres-release`, and `TABLEPRO_GTK_SCENARIO=postgres_session_transaction_confirmation_cancels_or_rolls_back bash scripts/test-gtk-postgres.sh` passed locally; candidate/hosted/installed acceptance remains |
| B4-atomic-batch / B4-rollback-error | MySQL source and native regressions cover non-DML refusal, rollback failure reporting, InnoDB transactional rows, and surviving MyISAM, MEMORY, CSV and ARCHIVE trigger effects for failed INSERT/UPDATE/DELETE. InnoDB AUTO_INCREMENT and trigger session-variable effects are also covered. The nine-test ignored MySQL atomicity group passed on frozen candidate `7eea6f09` and in hosted run `37708183936`. The PostgreSQL backend-termination rollback test passed on that candidate and in the same hosted run; see the [hosted-results comment on PR #287](https://github.com/cozyGarage/BookiE/pull/287#issuecomment-6049956346). Broad driver integration previously timed out during compilation before tests; wider PostgreSQL side-effect patterns remain open | [Retained MySQL evidence](evidence/mysql-atomic-results-2026-10-03/manifest.json); [expanded MySQL engine evidence](evidence/mysql-rollback-engine-effects-2026-10-07/manifest.json); [candidate checkpoint](https://github.com/cozyGarage/BookiE/pull/298#issuecomment-6050658653); [PostgreSQL rollback-failure evidence](evidence/postgres-rollback-failure-results-2026-10-07/manifest.json) |
| B4-MySQL-engine-atomicity | **Scoped contract verified locally, on frozen candidate, and hosted:** failed InnoDB DML rolls back parent and InnoDB trigger rows, while MyISAM, MEMORY, CSV, and ARCHIVE trigger writes survive failed INSERT, UPDATE, and DELETE batches. An InnoDB AUTO_INCREMENT allocation is not restored, and a trigger session-variable effect survives rollback. The nine-test atomicity group passed on frozen candidate `7eea6f09` and in hosted run `37708183936`; storage engines and side effects beyond these fixtures remain unverified | [Original MyISAM evidence](evidence/mysql-atomic-results-2026-10-03/manifest.json); [MyISAM and MEMORY regression](evidence/mysql-nontransactional-trigger-engines-2026-10-07/manifest.json); [CSV regression](evidence/mysql-csv-trigger-rollback-2026-10-07/manifest.json); [expanded MySQL engine and counter evidence](evidence/mysql-rollback-engine-effects-2026-10-07/manifest.json); [ARCHIVE regression](evidence/mysql-archive-rollback-effects-2026-10-07/manifest.json); [multi-engine UPDATE/DELETE regression](evidence/mysql-update-delete-multiengine-rollback-2026-10-07/manifest.json); [hosted results comment](https://github.com/cozyGarage/BookiE/pull/287#issuecomment-6049956346) |
| I1 | In the Debian phase, build/install executable `tablepro-askpass` in debhelper rules and make validator reject its absence; inspect the rules-built package | `packaging-contracts`, Debian package build |

## In-progress local slices

| ID | Current implementation and validation | Remaining |
| --- | --- | --- |
| B4-22 | Complete distribution-package/native Wayland bundle audit acceptance | Hosted installed GTK safety passed 53/53 scenarios on pinned candidate `5db1cfe`, including bundle export/import audit, encrypted credential round-trip and cross-profile import. Package/native Wayland acceptance remains open; see [current candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) |

## Completed local slices

Dated local-slice narratives, the I5 event shape and the I3 route/auth/TLS
matrix live in
[B4 history](archive/b4-history.md#archived-completed-local-slices-2026-10-10-consolidation).
Live acceptance bounds stay in [Next B4 acceptance run](#next-b4-acceptance-run)
and the [ledger](known-issues.md).

| ID | Tip status | Remaining |
| --- | --- | --- |
| I3 / C6 / G5 | Source matrix and tunneled TLS local proof recorded | Package / native Wayland; system OpenSSH TLS equivalence |
| I2 / I5 / F7 / F8-headless | Local implementation and fixture coverage recorded | Frozen candidate, hosted and installed acceptance |
| B4-12 | Scoped rollback DONE on hosted candidate | Full-candidate / package qualification separate |
| B4-16 / B4-17 / B4-21 | Local SSH / Kerberos proof recorded | Package-installed / native Wayland; Windows AD |
| B4-18 / B4-19 | Local unit proof recorded | Frozen-candidate and hosted acceptance |

## Acceptance order

1. Continue B3's bounded cases and evidence reconciliation. B3 completion
   precedes closing B4 acceptance; independent review/preparation may proceed.
2. C6 and G5 are merged; retain local evidence in
   [B4 history](archive/b4-history.md#archived-completed-local-slices-2026-10-10-consolidation).
   Finish headless F8 candidate acceptance and I2/I5/F7 hosted/installed bounds.
3. Freeze an accepted B3+B4 source SHA and run affected `full security-policy
   drivers tls postgres-release widgets ui packaging-contracts`; include keyring
   and optional-feature checks where affected. Avoid repeated overlapping suites.
4. Installed Arch/Omarchy/Hyprland native Wayland workflows and upgrade/rollback.
5. I1, then installed Debian/GNOME native Wayland workflows and upgrade/rollback.
6. B7 retry-free candidate soak and release decision. Publication is separate.

Use the shared checkout and `linux/target` cache in the current single-agent
session. Serialize Cargo and fixture runs. Do not clean the cache to diagnose a
failure. Record exact source hashes, commands, counts and raw sanitized logs.

## B4 completion checklist

- [ ] Shared policy/cache refusal and all session close/retirement routes pass on the candidate.
- [ ] Built-in trust and unattended daemon routes pass against real SSH fixtures.
- [ ] Connection uncertainty, journal failure and headless retirement follow ADR 0008.
- [ ] MySQL and SQL Server TLS through SSH have positive and negative native proof.
- [ ] Transport audit, Flatpak refusal and registered GTK session flow pass.
- [ ] Installed Arch native Wayland and upgrade/rollback evidence recorded.
- [ ] Debian rules/helper and installed GNOME native Wayland evidence recorded.

## Historical packets

The [October 2 dispatch plan](archive/b4-history.md#october-2-review-and-dispatch-plan),
[October 3 checkpoint](archive/b4-history.md#october-3-integration-checkpoint), original
lane tables, seven decision IDs, worktree reservations and detailed reproducers
are dated history. Their old “open”, “next” and “not merged” wording does not
override the current status above. Keep original task IDs when adding evidence.
