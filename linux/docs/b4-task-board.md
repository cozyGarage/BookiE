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

`fork/linux` is `d9d476f7a9e024eb133373ab4e4f7d67dedcdfb4` (2026-10-10).
The latest application-code commit is the dependency cleanup in #485; the tip
also includes refreshed B4 evidence. PR #458 computes `Effects`
alongside legacy facts, and PR #459 adds engine read-only enforcement; both are
merged. PR #460 implements migration step 2 (S8) and is merged. PR #463
implements DuckDB read-only selected-file restrictions and is merged. PR #486
fixes a selected-file open race and remains open. PR
#461 keeps read-scoped explain plans redacted, and PR #475 adds the B4-11
`MRG_MyISAM` rollback case; both are merged. PR #480 implements migration step
3 (joined verdicts for S6/S7) and remains open. Migration step 4, removing the
legacy facts and duplicate walkers, remains. Exact local verification and
acceptance boundaries are recorded below; local runs do not establish Forgejo
gate or frozen-candidate/package acceptance.

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
| B4-7, B4-16 | Complete package-installed/native Wayland trust and reconnect acceptance | At exact Linux tip `d9d476f7`, all seven PostgreSQL SSH/mTLS GTK scenarios passed, including unknown-host decline audit, setup-failure audit, multi-hop trust, changed-key refusal and tunnel-loss retirement/reconnect. Frozen-candidate, package-installed and native Wayland acceptance remain open. See [current release evidence](evidence/b4-current-postgres-release-d9d476f-2026-10-10/manifest.json) |
| B4-9 | Complete package-installed/native Wayland route/auth/TLS acceptance | At exact Linux tip `d9d476f7`, the PostgreSQL release layer passed: 1 system OpenSSH test, 3 mTLS tests, 59 release integration tests and all seven SSH/mTLS GTK scenarios. Frozen-candidate, package-installed and native Wayland acceptance remain open; the earlier MongoDB refusal cause remains undetermined. See [current release evidence](evidence/b4-current-postgres-release-d9d476f-2026-10-10/manifest.json), [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) and [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json) |
| B4-22 | Complete package-installed/native Wayland bundle audit acceptance | At exact source `d916b3aa`, four targeted GTK safety scenarios passed for sanitized export/import audit outcomes, encrypted credential restoration and cross-profile import. This was a per-user source build under Xvfb/X11; distribution-package installed flow and native Wayland acceptance remain open. See [exact-tip evidence](evidence/b4-22-bundle-current-tip-d916b3aa-2026-10-10/manifest.json) |
| B4-12 | ~~PostgreSQL rollback-failure acceptance on the selected frozen candidate~~ | Hosted candidate and local current-product-source (`fc180933`) backend-termination selectors passed. The scenario confirms `TransactionRollbackFailed`, absent transactional rows and persistent identity/trigger-sequence advancement. See [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) and [local linux-tip evidence](evidence/b4-rollback-linux-tip-2026-10-09/manifest.json) |
| B4-11 | Extend failed-batch tests to optional/vendor MySQL-family engines and broader side-effect patterns | PR #475 adds `MRG_MyISAM` write-through cases for both MySQL and MariaDB. On exact Linux source tip `788ff9ddb`, the serialized `mysql_atomic` selector passed 20/20 against Docker MySQL and MariaDB fixtures. Existing coverage includes MyISAM, MEMORY, CSV, ARCHIVE, BLACKHOLE, MariaDB Aria, direct/update/delete effects, triggers, auto-increment allocation, pinned-session state and advisory locks. Optional/vendor engines and other side-effect classes remain open; frozen-candidate and installed acceptance remain separate. See [current-tip run manifest](evidence/mysql-atomic-linux-788ff9ddb-2026-10-10/manifest.json), [advisory-lock evidence](evidence/mysql-advisory-lock-rollback-2026-10-09/manifest.json) and [session-state evidence](evidence/mysql-session-state-rollback-2026-10-09/manifest.json) |
| B4-17 | Complete package-installed/native Wayland trust flow | At exact Linux tip `d9d476f7`, 1 system OpenSSH test, 3 mTLS tests, 59 PostgreSQL release integration tests and all seven GTK trust scenarios passed, including multi-hop trust, second-hop decline, changed-key refusal and tunnel-loss recovery. This was a per-user source build under Xvfb/X11; frozen-candidate, package-installed multi-hop trust and native Wayland acceptance remain open. See [current release evidence](evidence/b4-current-postgres-release-d9d476f-2026-10-10/manifest.json), [local Arch Wayland evidence](evidence/b4-arch-package-wayland-2026-10-09/manifest.json), [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json), [native SSH evidence](evidence/b4-ssh-native-current-2026-10-09/manifest.json), and [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json) |
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

| ID | Local evidence | Scope remaining |
| --- | --- | --- |
| I3 | [Route, authentication and TLS matrix](#i3-route-authentication-and-tls-evidence-2026-10-07) records exact engine/backend, SHA/selector, unsupported and unrun combinations after C6/G5/I2 | Source evidence matrix and frozen candidate `7eea6f09` checks complete; hosted driver/PostgreSQL/GTK checks are running and distribution-package/native Wayland acceptance remains separate |
| G5 | PR #151 merged at `11672cbe6`; before merge, `bash scripts/test-postgres-release.sh` on the rebased branch verified that the actual agentd provider refused an unattended unknown system OpenSSH key without writing it, then reached a guarded PostgreSQL query with a pretrusted key. The PostgreSQL release integration suite passed; `audit_isolation_tests` passed separately (4 tests) | Frozen B3+B4 candidate SHA, completion of hosted checks, and installed acceptance remain separate |
| C6-MySQL / C6-SQLServer | C6 source commit `0ecb5bc4f`; rerun on the assembled B4 branch with `bash scripts/test-driver-tls.sh` on 2026-10-07: all 43 TLS tests passed, including 11 MySQL and 9 SQL Server tests. Both tunneled engines execute a query with valid CA/hostname verification, reject an untrusted CA and wrong/local service identities; MySQL proves no plaintext fallback and SQL Server checks `encrypt_option` plus tunnel cleanup | Built-in SSH forwarding only; system OpenSSH, frozen-candidate, hosted and installed acceptance remain separate |
| I2 | `tablepro-transport::tests::saved_system_openssh_refuses_deterministically_in_flatpak_without_fallback`: injectable sandbox context proves the selected OpenSSH route refuses before password-keyring resolution; explicit built-in route still constructs, and native OpenSSH route remains supported. The `quick` layer passed locally on `b4/i5-reconciled` (2026-10-07) and includes the sandbox tier | Flatpak package/runtime acceptance remains separate |
| F8-headless | `agentd::audit_isolation_tests`: connection A uncertainty leaves B writable; replacement recovers; cancellation of an old write after replacement does not poison the new generation; outcome journal failure blocks other sessions and replacement generations | Re-run on the frozen B3+B4 candidate SHA; hosted and installed restart acceptance remain separate |
| I5 | `security-policy`, `ssh`, and `postgres-release` passed on `b4/i5-reconciled` (2026-10-07). PostgreSQL release fixtures assert one journal event each for built-in unknown/mismatched-key refusal and system OpenSSH unknown/mismatched-key refusal, plus one connected event for pretrusted system OpenSSH. Transport units cover cancellation, audit sink failure (no connection returned; governed writes disabled), and legacy journal deserialization. Agent credentials are hashed before persistence. GTK saved/open, reconnect, connect-dialog submit, and test-connection call sites pass the audit context into the same transport boundary. The targeted GTK setup-failure scenario also proves a closed SSH port produces exactly one durable `connection_failed` outcome; `audit_journal_loss_after_connection_denies_mutation` verifies a live editor still denies writes after journal storage disappears. On current source `ca1dc6002`, the PostgreSQL release and SSH layers passed again; see [current-tip SSH evidence](evidence/b4-ssh-current-tip-2026-10-08/manifest.json), [setup-failure evidence](evidence/ssh-gtk-setup-failure-2026-10-07/manifest.json), and the [candidate checkpoint](evidence/b4-candidate-acceptance-2026-10-07/manifest.json). | Frozen-candidate, hosted and distribution-package/native Wayland acceptance remain |
| F7 | The rollback action sends `ROLLBACK`, awaits its result, then closes the dedicated session; Session stays active and controls stay disabled until both steps finish. Unit coverage gates delayed rollback/close and close-after-rollback-error; isolated GTK dialog coverage checks Cancel, Roll Back and Commit semantics. The `widgets` layer passed 13 selectors, `postgres-release` passed, and the full app library suite passed (537 passed, 39 ignored). The focused PostgreSQL 17 GTK flow passed: Cancel kept the same transaction open, later writes remained uncommitted, and Roll Back closed the session with zero persisted rows. See [GTK session evidence](evidence/gtk-postgres-session-confirmation-2026-10-07/manifest.json) | Re-run on the frozen B3+B4 candidate; hosted CI and installed acceptance remain |
| B4-16 | The [current-Linux seven-scenario evidence](evidence/ssh-gtk-audit-linux-fb82ea1-2026-10-07/manifest.json) cuts the saved SSH route during an editor transaction; the transaction remains absent, a stale-session write is refused, and after recovery a new Session successfully runs `SELECT 42`. The local installed release-binary run on `b4ee463` also passed tunnel-loss retirement/reconnect; see [current-tip SSH evidence](evidence/b4-ssh-installed-linux-tip-b4ee-2026-10-09/manifest.json) | **Unverified:** frozen-candidate, distribution-package/native Wayland acceptance remains |
| B4-17 | Verify native multi-hop, cancellation, and installed trust flow | Product source `93d2e337` passed the local PostgreSQL release acceptance: system OpenSSH, 3 mTLS tests, 59 release integration tests, and all seven GTK SSH/mTLS scenarios covering multi-hop trust/query, second-hop decline, changed-key refusal, and tunnel-loss recovery. Current `fork/linux` tip `5540db7e` contains docs-only PR #437 and test-only PR #424 atop product source `395bc182`; neither source snapshot is qualified by the `93d2e337` run; see [current-tip release evidence](evidence/b4-ssh-release-linux-tip-2026-10-09/manifest.json). The earlier installed release-binary run on `b4ee463` remains useful, but Debian package-installed SSH trust and native Wayland acceptance remain open |
| B4-21 | Preserve candidate Samba Kerberos+VerifyFull proof, then perform Windows AD interoperability | Exact current Linux tip `e208eb1b` passed both Samba AD Kerberos selectors with verified TLS; see [current-tip evidence](evidence/b4-mssql-kerberos-linux-tip-2026-10-10/manifest.json). It does not establish Windows AD interoperability, which remains open |
| B4-12 | ~~Repeat the PostgreSQL rollback-failure selector on the selected frozen candidate and hosted CI~~ | The backend-termination selector passed on candidate source `a7f14fa`, the earlier hosted candidate runs, and pinned candidate `5db1cfe` in Build Linux #37865445368. See [current hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) |
| B4-18 | Candidate `SERVER VERSION`, `LIST TABLES`, and `LIST VIEWS` reads for saved connect and connect-dialog submit now use `CandidateGuardFactory`; Test Connection table listing uses the same guard. `candidate_catalog_reads_use_the_durable_policy_audit_journal` verifies intent/outcome entries; `candidate_metadata_read_refuses_when_its_audit_intent_cannot_be_written` verifies fail-closed behavior. Both passed in `cargo test -p tablepro-app --lib candidate_ -- --nocapture` | Local app tests only; frozen-candidate, hosted, and installed acceptance remain |
| B4-19 | The vendored PostgreSQL SCRAM client rejects server nonces that fail to extend the client nonce or contain non-ASCII/non-printable bytes, before deriving a proof. A local wire fixture sends an echoed nonce, a non-ASCII suffix, and 100,001 iterations; all three cases reject before a client proof is sent. `cargo test -p tablepro-driver-postgres --lib` passed (54 tests) | Local driver tests only; frozen-candidate and hosted acceptance remain |

### I5 sanitized event shape and coverage

```json
{
  "phase": "outcome",
  "principal": { "kind": "agent", "token": "sha256:<short digest>" },
  "connection_id": "<attempt connection UUID>",
  "driver_id": "postgres",
  "operation_class": "administrative",
  "redacted_sql": "[NOT_APPLICABLE]",
  "decision_rule": "transport_attempt",
  "terminal_status": "denied",
  "error_category": "authentication",
  "error": "ssh_host_key_or_authentication_refused",
  "transport_attempt": {
    "client": "system_open_ssh",
    "outcome": "host_key_refused"
  }
}
```

| Caller / SSH client | Local evidence |
| --- | --- |
| GUI saved connect / reconnect / connect-dialog submit and test | Explicit audit context and cancellation reach the shared transport boundary. GTK release-binary scenarios assert exactly one durable outcome for unknown-key refusal, accepted trust/query, and closed-port setup failure; see [refusal](evidence/b4-gtk-ssh-audit-2026-10-07/manifest.json), [accepted trust](evidence/ssh-gtk-trust-accept-2026-10-07/manifest.json), and [setup failure](evidence/ssh-gtk-setup-failure-2026-10-07/manifest.json) evidence |
| Daemon / system OpenSSH | PostgreSQL release fixture asserts unknown-key refusal, changed-key refusal, and trusted connection outcomes from the durable journal; exactly one transport event per attempt |
| Shared transport / built-in SSH | PostgreSQL release fixture asserts unknown-key and changed-key outcomes with exactly one terminal event per attempt |
| Shared transport / failure paths | Unit coverage asserts one cancelled terminal event, sanitized agent principal, and audit-write failure returns no connection and disables governed writes |

### I3 route, authentication and TLS evidence (2026-10-07)

| Engine / caller | Source SHA and selector | Route and authentication evidence | TLS evidence | Scope not established |
| --- | --- | --- | --- | --- |
| MySQL | `0ecb5bc4f`; `verify_full_through_ssh_reaches_the_unpublished_server_and_runs_a_query`, `verify_full_through_ssh_rejects_an_untrusted_authority`, `verify_full_through_ssh_rejects_a_wrong_or_local_identity` | Built-in SSH local socket forward to an unpublished fixture; SSH uses a fixture key and the database uses the fixture username/password | `driver-tls` suite: valid VerifyFull query; untrusted CA and wrong/local service identity refused; TLS-only endpoint rejects plaintext and driver does not retry without TLS | System OpenSSH route; other MySQL authentication plugins; frozen-candidate/hosted/installed runs |
| SQL Server | `0ecb5bc4f`; same three `verify_full_through_ssh_*` selectors as MySQL | Built-in SSH TCP forward to an unpublished fixture; SSH uses a fixture key and the database uses the fixture username/password | `driver-tls` suite: valid VerifyFull query; untrusted CA and wrong/local service identity refused; server `encrypt_option` confirms encryption and dropping the tunnel closes the forward | System OpenSSH route; Kerberos; frozen-candidate/hosted/installed runs |
| PostgreSQL / built-in SSH | `e66ff814e`; `the_shared_transport_verifies_the_database_hostname_through_the_bastion` | Shared transport uses built-in SSH and fixture key to reach PostgreSQL; PostgreSQL uses fixture username/password | `postgres-release` asserts VerifyFull against the database hostname through the bastion and rejects use of the local TCP-forward address as TLS identity | System OpenSSH TLS verification is not separately asserted by the G5 selector; frozen-candidate/hosted/installed runs |
| PostgreSQL / agentd system OpenSSH | `37f984c56` / `c739f5afa`; `agentd_refuses_without_learning_an_unknown_system_openssh_key_then_queries_after_trust` | Actual daemon refuses the unknown SSH host key without learning it, then a pretrusted key reaches a guarded PostgreSQL query; PostgreSQL uses fixture username/password | The G5 saved connection requests VerifyFull, but the acceptance assertion is host-key refusal/trust and guarded query, not a separate TLS-negative matrix | Built-in/system OpenSSH TLS equivalence; frozen-candidate/hosted/installed runs |
| Saved system OpenSSH in Flatpak | `c2399c730`; `saved_system_openssh_refuses_deterministically_in_flatpak_without_fallback` | Deterministic route-resolution test refuses before credential lookup or driver dispatch, with no automatic fallback; built-in SSH remains selectable | Not a TLS integration test | Installed Flatpak runtime acceptance |

The assembled source `0b544ac18` passed `bash scripts/test-driver-tls.sh` (43
tests) after integrating C6. The selector and driver configuration stay in the engine
fixtures and tests; this matrix records only claims established by those tests.
Other TLS-tested engines (ClickHouse, MongoDB and Redis) have direct TLS fixture
coverage, but no SSH-forwarding coverage is claimed here. Do not treat this
matrix as frozen-candidate, hosted, or installed acceptance.

Headless panic retirement is covered by `a_session_whose_driver_panicked_is_not_reused_even_though_its_ping_is_healthy`;
include it in the combined G5/F8 candidate acceptance.

The agentd panic-retirement unit test uses a credential-free saved connection
and carries its tracing subscriber through the async operation. Secret Service
behavior stays in the dedicated keyring tier.

## Acceptance order

1. Continue B3's bounded cases and evidence reconciliation. B3 completion
   precedes closing B4 acceptance; independent review/preparation may proceed.
2. C6 and G5 are merged; retain their local evidence and resolve hosted check results. Finish headless F8 candidate acceptance, I2/I5/F7, and keep the completed I3 source matrix aligned with integration.
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
