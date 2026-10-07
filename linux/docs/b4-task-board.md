# B4 task board: transport and sessions

Base scope: `aeac107a4`, branch `linux`, reviewed 2026-10-03. The active B4 integration branch advances as work lands. Use each evidence
manifest for the exact source revision and Git for the current branch tip.
The local source includes MySQL batch/result fixes indexed in the [evidence manifest]
(evidence/mysql-atomic-results-2026-10-03/manifest.json).
The [sprint](bookie-0.2-sprint.md) owns release order; [ADR 0008](decisions/0008-connection-and-session-ownership.md)
owns the decisions. Use the current Git tip for its revision because this board is updated
independently of source commits. B4 remains open. Implementation, local execution, hosted
execution and installed acceptance are separate states.

## B4 continuation checkpoint (October 3; historical integration snapshot)

| Task | Integrated source | Remaining acceptance |
| --- | --- | --- |
| A1–A5, B1–B5, C1–C5, D1–D8, G1/G2/G4, H1–H4 | Prior implementation and regressions retained | Recheck affected cases on the frozen candidate; A5's consumer is F9 |
| E1/E2, G3, F1/F2/F3 | Merged in #13–#18 | No reimplementation; native/shared policy, verified cache and close-route acceptance at candidate SHA |
| F5 | Merged as `4b7814f5e` (#19) | Installed retirement/toggle flow |
| F4/F9 | Merged as `573435766` (#20) | Monitor consumes tunnel closure and swaps live identity; native reconnect and [release-binary GTK stale-session evidence](evidence/ssh-reconnect-session-gtk-2026-10-07/manifest.json) passed; distribution-package/native Wayland acceptance remains |
| F6 | Merged as `d1434438e` and follow-up `6e2c5687a` (#22/#23) | GUI defaults to refusal and installs the built-in prompter. Native two-hop success, second-hop key-change refusal, and in-flight cancellation while the second-hop trust prompt is pending passed locally on `f3a1765` via `bash scripts/test-ssh.sh`; the [combined release-binary GTK run](evidence/ssh-gtk-multihop-2026-10-07/manifest.json) passes all five default PostgreSQL release GTK scenarios, including two-hop success, second-hop decline, changed-key refusal, audit refusal, and saved mTLS. The current [relay image rerun](evidence/postgres-release-relay-no-expose-2026-10-07/manifest.json) also passed after removing unnecessary EXPOSE metadata. The two-hop success, decline and changed-key flows each record exactly one terminal audit outcome. Package-installed/native Wayland, frozen-candidate and hosted acceptance remain |
| F8 | GUI generation split merged as `6346a431c` (#24); daemon generations merged as `09c5351a3` | Policy, GUI and daemon scope uncertainty per connection generation while sharing journal failure. Agentd regressions cover isolation, replacement, late cancellation and shared journal failure; frozen-candidate and installed restart acceptance remain |

This table is the October 3 integration snapshot with later evidence links added
where useful. The current merged state is summarized below; merges do not imply
fresh hosted results. Historical reports and original packets are preserved in
[B4 history](archive/b4-history.md); missing worktree/cache paths remain
unavailable evidence. The broad release audit is dated
[October 3](archive/release-audit-2026-10-03.md).

## Latest merged slices (checked 2026-10-07)

| Task | Merge | Local evidence | Hosted state at last check |
| --- | --- | --- | --- |
| C6 | PR #150, merge commit `8f2df09ee` | MySQL and SQL Server tunneled TLS focused suite passed locally (43 tests) | Preflight, regression gate and SonarCloud failed; integration tests were skipped; Flatpak and supply-chain checks were queued or in progress |
| G5 | PR #151, merge commit `11672cbe6` | Actual system OpenSSH trust flow and PostgreSQL release suite passed locally; `audit_isolation_tests` passed separately (4 tests) | SonarCloud and resolve-ref passed; other checks were queued |

The current `linux` baseline fetched on 2026-10-07 is `e47dbbbc2` (PR #183).
Recent B4 merges include #159 (SSH audit, rollback and Kerberos), #161/#162
(PostgreSQL GTK fixture setup), #163/#170/#173/#175 (SSH trust-prompt timeout
and dismissal), #165 (GTK SSH fixture hardening), and #178 (file-chooser
accessibility wait). PR #181 records fresh merged-linux plaintext bundle-audit
GTK runs; #182 adds a fresh encrypted-bundle run and corrects the stale I5 GTK
coverage row. PR #183 adds B3 UI and MSSQL coverage. The [October 7 candidate
checkpoint](evidence/b4-candidate-acceptance-2026-10-07/manifest.json) retains
the earlier candidate evidence. These merges do not establish fresh hosted,
frozen-candidate, or installed acceptance; those gates remain open.

## Remaining tasks

Each row is a bounded task; implement engines and route variants separately.
Use existing fixtures, wrappers and the layer catalog. The release audit gives
additional privacy, value and evidence tasks without duplicating this board.

| ID | Small task and required assertion | Local layers |
| --- | --- | --- |
| I2 | **Implemented and locally tested.** Explicit saved system OpenSSH selection refuses at route resolution inside Flatpak before SSH credential resolution or driver dispatch; there is no automatic backend switch. The deterministic regression also verifies built-in SSH remains selectable inside the sandbox and system OpenSSH remains selectable outside it. Flatpak package/runtime acceptance remains separate | `quick` (includes sandbox tests) |
| I5 | **Local implementation and backend fixture coverage complete.** The shared transport boundary emits one terminal event per SSH attempt. Saved GUI opens/reconnects, connect-dialog submit/test, and daemon opens attach principal/connection identity and cancellation. Built-in and system OpenSSH real fixtures cover unknown and changed host keys; system OpenSSH also covers trusted success. Unit tests cover cancellation, audit-write failure/fail-closed behavior, and legacy journal compatibility. GTK refusal, accepted-query, and closed-port setup-failure scenarios each assert exactly one durable transport outcome. See [GTK refusal evidence](evidence/b4-gtk-ssh-audit-2026-10-07/manifest.json), [GTK success evidence](evidence/ssh-gtk-trust-accept-2026-10-07/manifest.json), and [GTK setup-failure evidence](evidence/ssh-gtk-setup-failure-2026-10-07/manifest.json). Frozen-candidate, hosted, and installed acceptance remain open | `security-policy`, `ssh`, `postgres-release` |
| F7 | **Local implementation, unit/widget, and PostgreSQL GTK flow verified.** Toggle-off confirmation keeps Session active; Cancel retains the same open transaction; explicit `ROLLBACK` finishes before session close and leaves no rows persisted | `widgets`, `postgres-release`, and `TABLEPRO_GTK_SCENARIO=postgres_session_transaction_confirmation_cancels_or_rolls_back bash scripts/test-gtk-postgres.sh` passed locally; candidate/hosted/installed acceptance remains |
| B4-atomic-batch / B4-rollback-error | MySQL source and native regressions complete locally: non-DML batches are refused before dispatch, failed rollback reports unknown outcome, and transactional InnoDB trigger rows roll back while MyISAM, MEMORY, CSV and ARCHIVE trigger writes survive. Failed batches also leave InnoDB AUTO_INCREMENT allocation advanced; UPDATE and DELETE batch failures restore InnoDB parent/trigger rows while MyISAM trigger effects survive. The seven-test ignored MySQL atomicity group passed locally on 2026-10-07; the expanded UPDATE/DELETE regression passed again on the current worktree. The UI warns that non-transactional writes may remain. PostgreSQL native tests terminate the active backend and prove `TransactionRollbackFailed`, rolled-back table data, and surviving identity/trigger sequence increments. Broader side-effect coverage and frozen-candidate acceptance remain open | [Retained MySQL evidence](evidence/mysql-atomic-results-2026-10-03/manifest.json); [three-engine MySQL trigger evidence](evidence/mysql-csv-trigger-rollback-2026-10-07/manifest.json); [expanded MySQL engine and counter evidence](evidence/mysql-rollback-engine-effects-2026-10-07/manifest.json); [ARCHIVE MySQL regression](evidence/mysql-archive-rollback-effects-2026-10-07/manifest.json); [UPDATE/DELETE and seven-test group evidence](evidence/mysql-update-delete-rollback-effects-2026-10-07/manifest.json); [PostgreSQL rollback-failure evidence](evidence/postgres-rollback-failure-results-2026-10-07/manifest.json); [PostgreSQL sequence side-effect evidence](evidence/postgres-rollback-sequence-side-effect-2026-10-07/manifest.json); [current B4 worktree rerun](evidence/postgres-rollback-current-branch-2026-10-07/manifest.json) |
| B4-MySQL-engine-atomicity | **Scoped contract tested locally:** failed InnoDB DML rolls back parent and InnoDB trigger rows, while MyISAM, MEMORY, CSV and ARCHIVE INSERT-trigger writes survive; MyISAM UPDATE and DELETE trigger writes also survive failed batches. An InnoDB AUTO_INCREMENT allocation is not restored. The UI warns that non-transactional writes may remain. Other engines and side-effect patterns still need scoped proof | [Original MyISAM evidence](evidence/mysql-atomic-results-2026-10-03/manifest.json); [MyISAM and MEMORY regression](evidence/mysql-nontransactional-trigger-engines-2026-10-07/manifest.json); [CSV regression](evidence/mysql-csv-trigger-rollback-2026-10-07/manifest.json); [expanded MySQL engine and counter evidence](evidence/mysql-rollback-engine-effects-2026-10-07/manifest.json); [ARCHIVE regression](evidence/mysql-archive-rollback-effects-2026-10-07/manifest.json); [UPDATE/DELETE regression including transactional trigger rollback](evidence/mysql-update-delete-rollback-effects-2026-10-07/manifest.json); broader candidate acceptance remains |
| I1 | In the Debian phase, build/install executable `tablepro-askpass` in debhelper rules and make validator reject its absence; inspect the rules-built package | `packaging-contracts`, Debian package build |

## In-progress local slices

| ID | Current implementation and validation | Remaining |
| --- | --- | --- |
| B4-22 | ADR [0010](decisions/0010-administrative-action-audit.md) defines privacy-bounded administrative audit events; code records durable intent/outcome around confirmed bundle export/import and gates side effects on intent durability. Failed passphrase attempts retain the parsed bundle and reopen the passphrase prompt without requiring another file selection. Focused interrupted-intent recovery, fail-closed and paired-event checks pass; affected crate suite passed (903 passed, 45 ignored). [GTK export evidence](evidence/b4-bundle-export-audit-2026-10-07/manifest.json) and [GTK import evidence](evidence/b4-bundle-import-audit-2026-10-07/manifest.json) verify plaintext flows; [installed-path GTK evidence](evidence/b4-bundle-audit-installed-2026-10-07/manifest.json) covers both plaintext scenarios; [encrypted GTK round-trip evidence](evidence/b4-encrypted-bundle-gtk-2026-10-07/manifest.json) records earlier retry, unlock, credential replacement and audit redaction. Fresh merged-linux [plaintext](evidence/b4-bundle-audit-linux-2026-10-07/manifest.json) and [encrypted](evidence/b4-encrypted-bundle-linux-2026-10-07/manifest.json) release-binary Xvfb reruns passed | Frozen-candidate and hosted acceptance, plus distribution-package/native Wayland acceptance, remain |

## Completed local slices

| ID | Local evidence | Scope remaining |
| --- | --- | --- |
| I3 | [Route, authentication and TLS matrix](#i3-route-authentication-and-tls-evidence-2026-10-07) records exact engine/backend, SHA/selector, unsupported and unrun combinations after C6/G5/I2 | Source evidence matrix complete; frozen candidate, hosted checks and installed acceptance remain separate |
| G5 | PR #151 merged at `11672cbe6`; before merge, `bash scripts/test-postgres-release.sh` on the rebased branch verified that the actual agentd provider refused an unattended unknown system OpenSSH key without writing it, then reached a guarded PostgreSQL query with a pretrusted key. The PostgreSQL release integration suite passed; `audit_isolation_tests` passed separately (4 tests) | Frozen B3+B4 candidate SHA, completion of hosted checks, and installed acceptance remain separate |
| C6-MySQL / C6-SQLServer | C6 source commit `0ecb5bc4f`; rerun on the assembled B4 branch with `bash scripts/test-driver-tls.sh` on 2026-10-07: all 43 TLS tests passed, including 11 MySQL and 9 SQL Server tests. Both tunneled engines execute a query with valid CA/hostname verification, reject an untrusted CA and wrong/local service identities; MySQL proves no plaintext fallback and SQL Server checks `encrypt_option` plus tunnel cleanup | Built-in SSH forwarding only; system OpenSSH, frozen-candidate, hosted and installed acceptance remain separate |
| I2 | `tablepro-transport::tests::saved_system_openssh_refuses_deterministically_in_flatpak_without_fallback`: injectable sandbox context proves the selected OpenSSH route refuses before password-keyring resolution; explicit built-in route still constructs, and native OpenSSH route remains supported. The `quick` layer passed locally on `b4/i5-reconciled` (2026-10-07) and includes the sandbox tier | Flatpak package/runtime acceptance remains separate |
| F8-headless | `agentd::audit_isolation_tests`: connection A uncertainty leaves B writable; replacement recovers; cancellation of an old write after replacement does not poison the new generation; outcome journal failure blocks other sessions and replacement generations | Re-run on the frozen B3+B4 candidate SHA; hosted and installed restart acceptance remain separate |
| I5 | `security-policy`, `ssh`, and `postgres-release` passed on `b4/i5-reconciled` (2026-10-07). PostgreSQL release fixtures assert one journal event each for built-in unknown/mismatched-key refusal and system OpenSSH unknown/mismatched-key refusal, plus one connected event for pretrusted system OpenSSH. Transport units cover cancellation, audit sink failure (no connection returned; governed writes disabled), and legacy journal deserialization. Agent credentials are hashed before persistence. GTK saved/open, reconnect, connect-dialog submit, and test-connection call sites pass the audit context into the same transport boundary. The targeted GTK setup-failure scenario also proves a closed SSH port produces exactly one durable `connection_failed` outcome; `audit_journal_loss_after_connection_denies_mutation` verifies a live editor still denies writes after journal storage disappears. See [setup-failure evidence](evidence/ssh-gtk-setup-failure-2026-10-07/manifest.json) and the [candidate checkpoint](evidence/b4-candidate-acceptance-2026-10-07/manifest.json). | Frozen candidate SHA, hosted CI, and installed acceptance |
| F7 | The rollback action sends `ROLLBACK`, awaits its result, then closes the dedicated session; Session stays active and controls stay disabled until both steps finish. Unit coverage gates delayed rollback/close and close-after-rollback-error; isolated GTK dialog coverage checks Cancel, Roll Back and Commit semantics. The `widgets` layer passed 13 selectors, `postgres-release` passed, and the full app library suite passed (537 passed, 39 ignored). The focused PostgreSQL 17 GTK flow passed: Cancel kept the same transaction open, later writes remained uncommitted, and Roll Back closed the session with zero persisted rows. See [GTK session evidence](evidence/gtk-postgres-session-confirmation-2026-10-07/manifest.json) | Re-run on the frozen B3+B4 candidate; hosted CI and installed acceptance remain |
| B4-16 | [Release-binary GTK evidence](evidence/ssh-reconnect-session-gtk-2026-10-07/manifest.json): Toxiproxy cuts the saved SSH route during an editor transaction; the transaction remains absent, a stale-session write is refused, and after recovery a new Session successfully runs `SELECT 42` | Distribution-package/native Wayland, frozen-candidate and hosted acceptance remain |
| B4-21 | [SQL Server Kerberos evidence](evidence/mssql-kerberos-2026-10-07/manifest.json): the driver used a valid AD ticket over VerifyFull TLS, queried `SYSTEM_USER` as `DOMAIN1\bookiekerb`, and refused an unregistered SPN. The [hardened-container rerun](evidence/mssql-kerberos-hardening-2026-10-07/manifest.json) passes with a non-root client and pinned digest-only image references. Generated keytabs are removed by default | Real Windows AD interoperability and frozen-candidate/hosted acceptance remain separate |
| B4-12 | [Native PostgreSQL evidence](evidence/postgres-rollback-failure-results-2026-10-07/manifest.json) plus [identity-sequence side-effect evidence](evidence/postgres-rollback-sequence-side-effect-2026-10-07/manifest.json): `a_batch_reports_rollback_failure_after_postgres_terminates_its_backend` runs on PostgreSQL 16 in Docker. A monitor terminates the backend during statement 1; the driver reports `TransactionRollbackFailed`, the parent and trigger rows are absent, and identity/trigger sequence allocations remain advanced. [Current `linux` rerun](evidence/postgres-rollback-linux-2026-10-07/manifest.json) passed on source `b33a0f903` with the module-qualified exact selector; [current B4 worktree rerun](evidence/postgres-rollback-current-branch-2026-10-07/manifest.json) and [trigger-effects rerun](evidence/postgres-rollback-trigger-effects-2026-10-07/manifest.json) also passed | Frozen-candidate SHA, broader side-effect patterns and hosted acceptance remain |
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
