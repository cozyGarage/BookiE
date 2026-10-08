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
| F6 | Merged as `d1434438e` and follow-up `6e2c5687a` (#22/#23) | GUI defaults to refusal and installs the built-in prompter. Native two-hop success, second-hop key-change refusal, and in-flight cancellation while the second-hop trust prompt is pending passed locally on `f3a1765` via `bash scripts/test-ssh.sh`; the [combined release-binary GTK run](evidence/ssh-gtk-multihop-2026-10-07/manifest.json) passes all five default PostgreSQL release GTK scenarios, including two-hop success, second-hop decline, changed-key refusal, audit refusal, and saved mTLS. The current [relay image rerun](evidence/postgres-release-relay-no-expose-2026-10-07/manifest.json) also passed after removing unnecessary EXPOSE metadata. The two-hop success, decline and changed-key flows each record exactly one terminal audit outcome. Frozen-candidate native cancellation and package-installed/native Wayland trust acceptance remain; all seven hosted release-binary PostgreSQL SSH GTK scenarios passed in [run #37708183936](https://github.com/cozyGarage/BookiE/actions/runs/37708183936) at PR #286 head `a056bf1` |
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

The current `linux` branch tip fetched on 2026-10-07 is `c8c06edd` (PR #201).
Recent B4 merges include #159 (SSH audit, rollback and Kerberos), #161/#162
(PostgreSQL GTK fixture setup), #163/#170/#173/#175 (SSH trust-prompt timeout
and dismissal), #165 (GTK SSH fixture hardening), and #178 (file-chooser
accessibility wait). PR #181 records fresh merged-linux plaintext bundle-audit
GTK runs; #182 adds a fresh encrypted-bundle run and corrects the stale I5 GTK
coverage row. PR #183 adds B3 UI and MSSQL coverage; #184 reconciles the sprint
baseline. PR #185 fixes bundle chooser interaction and connection-list rebuild
after its GTK run exposed a fatal-critical regression. The export, plaintext
import and encrypted retry/credential round-trip GTK scenarios were rerun on
`3a3d742fe` via `TABLEPRO_GTK_SCENARIO=bundle_export_records_sanitized_audit_outcome,bundle_import_records_sanitized_audit_outcome,encrypted_bundle_round_trip_restores_credentials bash scripts/test-gtk-safety.sh`;
all three passed on the local host under isolated D-Bus/Xvfb. The harness sets
`G_DEBUG=fatal-criticals`. The [PR #185 discussion](https://github.com/cozyGarage/BookiE/pull/185)
records the original failure and fix. Required pull-request checks on #185
completed successfully; Docker-backed suites and installed GTK safety were
skipped on the pull-request event. The later push run on #194's merge commit
([run 37641533809](https://github.com/cozyGarage/BookiE/actions/runs/37641533809))
was superseded by the #195 push: its installed GTK and driver integration jobs
were cancelled, and the Linux regression gate reported those cancelled lanes
as incomplete. This does not establish hosted installed-flow acceptance. The
[October 7 candidate
checkpoint](evidence/b4-candidate-acceptance-2026-10-07/manifest.json) retains
the earlier candidate evidence. These runs do not establish frozen-candidate
or installed acceptance; those gates remain open.

## Frozen-candidate acceptance (2026-10-08)

The previous local checkpoint is candidate
`7eea6f09d7154f03400e82cec7c16215488b2115`; see the [candidate results and
hosted run state](https://github.com/cozyGarage/BookiE/pull/298#issuecomment-6050658653).
Policy, SSH, TLS, PostgreSQL release, GTK bundle audit, SQL Server Kerberos,
MySQL atomicity and PostgreSQL rollback-failure checks passed locally on this
exact source. The initial pinned Build Linux run passed PostgreSQL release,
Fast GTK, TLS, Clippy, and optional DuckDB, but its driver integration job
timed out during workspace compilation and its GTK suite exposed an encrypted
credential restoration failure. The corrected exact-SHA rerun below passed
GTK safety and release-binary SSH scenarios; driver integration again timed
out during compilation. See the
[candidate checkpoint](https://github.com/cozyGarage/BookiE/pull/298#issuecomment-6050658653).
The prior PostgreSQL release job used a debug application binary. PR #286
merged at `d5d63a95`; its pinned hosted dispatch on PR commit
`a056bf1715cd134224caf9c3215d617ae9b5ebb6` passed the focused B4 rollback layer
(9 MySQL cases and 1 PostgreSQL case), hosted release-binary SSH GTK scenarios,
and installed GTK safety smoke. The broad driver integration tier exhausted its
30-minute budget during dependency compilation before tests ran. See the [hosted results
comment on PR #287](https://github.com/cozyGarage/BookiE/pull/287#issuecomment-6049956346)
and [candidate workflow](https://github.com/cozyGarage/BookiE/actions/runs/37708183936).

The earlier `0d22bfe9` checkpoint remains available in the
[PR #258 evidence discussion](https://github.com/cozyGarage/BookiE/pull/258#issuecomment-6048355227).
Neither checkpoint establishes distribution-package installation on native
Wayland, upgrade/rollback acceptance or Windows AD interoperability. This does
not qualify 0.2.0.

### Next B4 acceptance run

The previous candidate and hosted evidence is historical; `linux` has advanced
since the `7eea6f09` checkpoint. The user-requested local B4 rerun passed on
clean SHA `7eff6f26`; the rollback layer was repeated on `8e5d1b18` after PR
#308 changed PostgreSQL integration tests. See the [current candidate
evidence comment index](https://github.com/cozyGarage/BookiE/pull/310#issuecomment-6051740646). This does not
change the approved sprint order: full B4 release acceptance remains scheduled
after B3 closes. If `8e5d1b18` is selected as the frozen release candidate,
rerun the remaining B4 layers on that exact SHA. Preserve the pinned results
and rerun for any later source tip.

The hosted Build Linux run for `8e5d1b18` was
[cancelled](https://github.com/cozyGarage/BookiE/actions/runs/37723088254) as
`linux` advanced through PRs #316–#331 to `b5112ca7`. At the 2026-10-08
check, Security had passed, Build Linux was pending and Flatpak was in progress
([Build Linux](https://github.com/cozyGarage/BookiE/actions/runs/37732479926),
[Flatpak](https://github.com/cozyGarage/BookiE/actions/runs/37732479760),
[Linux Security](https://github.com/cozyGarage/BookiE/actions/runs/37732479814)).
PR #312 skipped its B4 rollback job because it only changed MongoDB tests. The
focused `b4-rollback` layer passed locally on documentation-only commit
`73f8ce935`; the tested source files match `684ea40f`. See the [PR #314 evidence
comment](https://github.com/cozyGarage/BookiE/pull/314#issuecomment-6052065028).
Those results do not establish hosted acceptance on a selected frozen
candidate. GTK runs using a staged release binary under Xvfb do not establish
distribution-package installation or native Wayland acceptance.
A combined local B4 run on clean source `98134709` passed rollback, security
policy, SSH, PostgreSQL release, SQL Server Kerberos and GTK safety. Its TLS
layer failed four MongoDB TLS cases with `ConnectionRefused` while the
ClickHouse TLS cases passed. The full TLS script then passed twice on
`a47b1fb`, including all five MongoDB tests; the retained
[48-test evidence comment](https://github.com/cozyGarage/BookiE/pull/329#issuecomment-6052925352)
contains the manifest and complete second-run log. The original cause remains
undetermined.

| Work | Required evidence on the next candidate | Current boundary |
| --- | --- | --- |
| B4-7, B4-16 | Re-run affected layers on the selected frozen SHA, confirm hosted results for that SHA, then complete installed-package/native Wayland acceptance | On `b5112ca7`, Security passed, Build Linux was pending and Flatpak was in progress. All three acceptance gates remain open |
| B4-9 | Re-run the TLS matrix on the selected frozen SHA, confirm hosted results for that SHA, then complete installed-package/native Wayland acceptance | The full local matrix passed twice on `a47b1fb` (48/48); the earlier MongoDB refusal on `98134709` did not recur, and its cause remains undetermined. See [PR evidence comment](https://github.com/cozyGarage/BookiE/pull/329#issuecomment-6052925352) |
| B4-22 | Run GTK export/import audit and encrypted credential round-trip on the selected frozen SHA; confirm hosted result and installed-package/native Wayland acceptance | Prior local and hosted GTK results are SHA-scoped; later hosted run on `8e5d1b18` was cancelled |
| B4-12 | Repeat `a_batch_reports_rollback_failure_after_postgres_terminates_its_backend` on the selected frozen candidate and retain exact SHA/results | The exact frozen candidate `7eea6f09d7154f03400e82cec7c16215488b2115` passed the focused local rerun; see [candidate evidence](evidence/postgres-rollback-frozen-7eea6f09-2026-10-08/manifest.json). Hosted B4 rollback passes exist at other SHAs, but exact-candidate hosted coverage and acceptance on any next candidate remain open |
| B4-11 | Extend failed-batch tests to the additional MySQL storage engines and side-effect patterns requested | [PR #337 local rollback evidence](https://github.com/cozyGarage/BookiE/pull/337#issuecomment-6053382536) adds direct failed-batch INSERT behavior on InnoDB, MyISAM, MEMORY, CSV and ARCHIVE; it also reruns the existing trigger, update/delete, AUTO_INCREMENT, session-variable and rollback-failure cases (10 MySQL tests, 1 PostgreSQL selector). Optional/vendor-specific engines and broader side effects remain open |
| B4-17 | Reuse the existing `ssh-gtk-*` evidence for GTK trust prompts on both hops, routed query, second-hop decline without learning, changed-key refusal and terminal audit outcomes; run selected-candidate/hosted acceptance and installed-package/native Wayland trust flow | Existing manifests run staged release binaries under Xvfb/AT-SPI, not a distribution-package installation. Hosted flows exist for earlier SHA `a056bf1`; installed trust acceptance remains open |
| B4-21 | Preserve Samba Kerberos+VerifyFull TLS proof, then perform Windows AD interoperability and candidate acceptance | Samba AD does not establish Windows AD interoperability; both Windows AD and selected-candidate acceptance remain open |
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
| B4-22 | ADR [0010](decisions/0010-administrative-action-audit.md) defines privacy-bounded administrative audit events; code records durable intent/outcome around confirmed bundle export/import and gates side effects on intent durability. Failed passphrase attempts retain the parsed bundle and reopen the passphrase prompt without requiring another file selection. Focused interrupted-intent recovery, fail-closed and paired-event checks pass; the affected crate suite passed earlier (903 passed, 45 ignored). Earlier plaintext and encrypted runs are preserved in the [PR #181 discussion](https://github.com/cozyGarage/BookiE/pull/181#issuecomment-6038419580) and [PR #182 discussion](https://github.com/cozyGarage/BookiE/pull/182#issuecomment-6038507980). Those runs exposed a GTK wrong-parent removal. After PR #185, release-binary local Xvfb reruns on `3a3d742fe` passed all three bundle audit GTK scenarios with `G_DEBUG=fatal-criticals`; hosted installed GTK safety smoke also passed all three scenarios on `linux` commit `3a00ecc` (50 scenarios total; [report and scenario results](evidence/hosted-b4-22-installed-gtk-2026-10-07/manifest.json)). A fresh [current-Linux release-binary rerun](evidence/b4-bundle-audit-linux-68e6727-2026-10-07/manifest.json) again passed export audit, import audit, and encrypted credential round-trip. Frozen candidate `7eea6f09` passed staged-binary GTK acceptance. The pinned hosted installed GTK run initially failed encrypted credential restoration; after adding an explicit checked-state assertion for “Replace saved passwords,” the rerun in hosted run [#37708183936](https://github.com/cozyGarage/BookiE/actions/runs/37708183936) passed the full GTK safety suite including encrypted credential restoration. See the [candidate checkpoint](https://github.com/cozyGarage/BookiE/pull/298#issuecomment-6050658653). Distribution-package/native Wayland acceptance remains open | `widgets`, GTK safety scenarios, frozen-candidate` |

## Completed local slices

| ID | Local evidence | Scope remaining |
| --- | --- | --- |
| I3 | [Route, authentication and TLS matrix](#i3-route-authentication-and-tls-evidence-2026-10-07) records exact engine/backend, SHA/selector, unsupported and unrun combinations after C6/G5/I2 | Source evidence matrix and frozen candidate `7eea6f09` checks complete; hosted driver/PostgreSQL/GTK checks are running and distribution-package/native Wayland acceptance remains separate |
| G5 | PR #151 merged at `11672cbe6`; before merge, `bash scripts/test-postgres-release.sh` on the rebased branch verified that the actual agentd provider refused an unattended unknown system OpenSSH key without writing it, then reached a guarded PostgreSQL query with a pretrusted key. The PostgreSQL release integration suite passed; `audit_isolation_tests` passed separately (4 tests) | Frozen B3+B4 candidate SHA, completion of hosted checks, and installed acceptance remain separate |
| C6-MySQL / C6-SQLServer | C6 source commit `0ecb5bc4f`; rerun on the assembled B4 branch with `bash scripts/test-driver-tls.sh` on 2026-10-07: all 43 TLS tests passed, including 11 MySQL and 9 SQL Server tests. Both tunneled engines execute a query with valid CA/hostname verification, reject an untrusted CA and wrong/local service identities; MySQL proves no plaintext fallback and SQL Server checks `encrypt_option` plus tunnel cleanup | Built-in SSH forwarding only; system OpenSSH, frozen-candidate, hosted and installed acceptance remain separate |
| I2 | `tablepro-transport::tests::saved_system_openssh_refuses_deterministically_in_flatpak_without_fallback`: injectable sandbox context proves the selected OpenSSH route refuses before password-keyring resolution; explicit built-in route still constructs, and native OpenSSH route remains supported. The `quick` layer passed locally on `b4/i5-reconciled` (2026-10-07) and includes the sandbox tier | Flatpak package/runtime acceptance remains separate |
| F8-headless | `agentd::audit_isolation_tests`: connection A uncertainty leaves B writable; replacement recovers; cancellation of an old write after replacement does not poison the new generation; outcome journal failure blocks other sessions and replacement generations | Re-run on the frozen B3+B4 candidate SHA; hosted and installed restart acceptance remain separate |
| I5 | `security-policy`, `ssh`, and `postgres-release` passed on `b4/i5-reconciled` (2026-10-07). PostgreSQL release fixtures assert one journal event each for built-in unknown/mismatched-key refusal and system OpenSSH unknown/mismatched-key refusal, plus one connected event for pretrusted system OpenSSH. Transport units cover cancellation, audit sink failure (no connection returned; governed writes disabled), and legacy journal deserialization. Agent credentials are hashed before persistence. GTK saved/open, reconnect, connect-dialog submit, and test-connection call sites pass the audit context into the same transport boundary. The targeted GTK setup-failure scenario also proves a closed SSH port produces exactly one durable `connection_failed` outcome; `audit_journal_loss_after_connection_denies_mutation` verifies a live editor still denies writes after journal storage disappears. See [setup-failure evidence](evidence/ssh-gtk-setup-failure-2026-10-07/manifest.json) and the [candidate checkpoint](evidence/b4-candidate-acceptance-2026-10-07/manifest.json). | Frozen candidate `7eea6f09` passed local I5 layers. Hosted release-binary PostgreSQL SSH GTK scenarios also passed in run `37708183936` at PR #286 head `a056bf1`; distribution-package/native Wayland acceptance remains |
| F7 | The rollback action sends `ROLLBACK`, awaits its result, then closes the dedicated session; Session stays active and controls stay disabled until both steps finish. Unit coverage gates delayed rollback/close and close-after-rollback-error; isolated GTK dialog coverage checks Cancel, Roll Back and Commit semantics. The `widgets` layer passed 13 selectors, `postgres-release` passed, and the full app library suite passed (537 passed, 39 ignored). The focused PostgreSQL 17 GTK flow passed: Cancel kept the same transaction open, later writes remained uncommitted, and Roll Back closed the session with zero persisted rows. See [GTK session evidence](evidence/gtk-postgres-session-confirmation-2026-10-07/manifest.json) | Re-run on the frozen B3+B4 candidate; hosted CI and installed acceptance remain |
| B4-16 | The [current-Linux seven-scenario evidence](evidence/ssh-gtk-audit-linux-fb82ea1-2026-10-07/manifest.json) cuts the saved SSH route during an editor transaction; the transaction remains absent, a stale-session write is refused, and after recovery a new Session successfully runs `SELECT 42`. The same tunnel-loss retirement/reconnect scenario passed on frozen candidate `7eea6f09` against the staged release binary and in hosted run `37708183936` against the staged release binary at PR #286 head `a056bf1`; see the [candidate checkpoint](https://github.com/cozyGarage/BookiE/pull/298#issuecomment-6050658653) | **Unverified:** distribution-package/native Wayland acceptance remains |
| B4-17 | The existing `ssh-gtk-*` evidence covers trust prompts and durable known-host entries for both hops, query through the route, second-hop decline without learning its key, changed-key refusal and one terminal audit outcome per attempt. On frozen candidate `7eea6f09`, 22 native SSH tests and seven PostgreSQL GTK scenarios passed. All seven GTK scenarios passed in hosted run `37708183936` against the staged release binary at PR #286 head `a056bf1`; see the [candidate checkpoint](https://github.com/cozyGarage/BookiE/pull/298#issuecomment-6050658653) | Native second-hop prompt cancellation has local SSH-layer coverage; hosted cancellation-prompt GTK coverage and distribution-package/native Wayland trust acceptance remain |
| B4-21 | [SQL Server Kerberos evidence](evidence/mssql-kerberos-2026-10-07/manifest.json): the driver used a valid AD ticket over VerifyFull TLS, queried `SYSTEM_USER` as `DOMAIN1\bookiekerb`, and refused an unregistered SPN. The same two local Samba AD fixture tests passed on frozen candidate `7eea6f09`; the pinned hosted Driver TLS fixture passed. Generated keytabs are removed by default | Real Windows AD interoperability remains unverified; the privileged Samba AD fixture is local-only |
| B4-12 | [Native PostgreSQL evidence](evidence/postgres-rollback-failure-results-2026-10-07/manifest.json), [identity-sequence side-effect evidence](evidence/postgres-rollback-sequence-side-effect-2026-10-07/manifest.json), and the [frozen-candidate rerun](evidence/postgres-rollback-frozen-7eea6f09-2026-10-08/manifest.json): the exact backend-termination test passed locally on candidate `7eea6f09d7154f03400e82cec7c16215488b2115`, reporting `TransactionRollbackFailed` while table rows rolled back and identity/trigger sequences remained advanced. Hosted B4 rollback jobs passed at `a056bf1` and `1e1a30d`, not the frozen candidate SHA. Exact-candidate hosted and next-candidate acceptance remain open; wider PostgreSQL side-effect patterns remain outside this test |
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
