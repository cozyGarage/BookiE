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

## Current merged-tip B4 checkpoint (2026-10-09)

`fork/linux` is `ca1080a55c9a47fb0d241d92395820864836491f`; the latest product
source remains `c2f3f78b954ef0c7cb7d8956f035aa08edc71c7f` after PR #458, followed
by documentation-only TEST-28 ledger updates. PR #458 computes internal
`Effects` alongside the legacy facts and leaves the S6–S8 verdict behavior
unchanged. The current-tip MySQL rollback rerun passed 18/18 selectors locally;
see the B4-11 row below. This updates source and local evidence only; it does
not establish frozen-candidate, hosted, package-installed or native Wayland
acceptance.

## AUD-9 query-plan masking regression (2026-10-10)

PR #456's fail-closed fallback treated plain `EXPLAIN SELECT` as an unknown
projection and redacted the plan returned by the agent `explain_query` tool.
The PostgreSQL release fixture reproduced this twice on the post-#456 source;
the pre-#456 parent passed the same full fixture. The fix allows output only
for one plain, non-`ANALYZE` `EXPLAIN` whose inner statement is a query.
`EXPLAIN ANALYZE`, plans for non-query statements and all other unknown shapes
remain redacted. The full policy suite passed 216 tests with 4 Docker-only tests
ignored, and the PostgreSQL release fixture passed, including the query-plan
MCP regression and seven SSH/mTLS GTK scenarios. See the
[AUD-9 evidence](evidence/aud9-explain-plan-mask-2026-10-10/manifest.json).
This local result does not establish the Forgejo gate, hosted acceptance or
frozen-candidate/installed acceptance.

## Previous merged-tip B4 checkpoint (2026-10-09; source `97e5f55`)

`fork/linux` is `97e5f55bddacdfef427c51f162917fedcace3a9f` after PR #451.
PR #451 makes the SSH cancellation fixture deterministic; its rebased focused
test passed 10/10 locally. On the merged tip, the per-user source build passed
the 54-scenario GTK safety suite, the MySQL approval-dialog regression, the
seven PostgreSQL SSH/mTLS GTK scenarios, and the 18-test `mysql_atomic`
selector. The PostgreSQL release fixture also passed one system OpenSSH test,
three mTLS tests, and 59 release integration tests. Logs, hashes, exact
selectors, and scope limits are in the [current-tip evidence](evidence/b4-current-linux-tip-2026-10-09/manifest.json).

This evidence is from a user-local package-layout install, not a package built
and installed by pacman. The post-merge hosted Build Linux run was still in
progress when the evidence was captured. Distribution-package and native
Wayland acceptance and Windows AD interoperability remain open. A separate
manual launch on the earlier source `3b903b8` produced a GTK/Wayland
SIGSEGV. An isolated eight-second Wayland smoke on `97e5f55` showed no
SIGSEGV but did not reach workspace readiness because its D-Bus session could
not activate AT-SPI; this is incomplete, not acceptance. The headless Wayland
runner could not start because `gnome-shell` is unavailable on the test host.
See the [run notes](evidence/b4-current-linux-tip-2026-10-09/README.md).

On exact `fork/linux` source `2ef1df8`,
`bash scripts/test-arch-package-container.sh` built and validated Arch package
`0.1.6-1` (SHA-256
`709d61f931efc7b833e3a9f45f02a5f17927213b3c122e098ffddae5328e1ee8`). The
container test upgraded `0.1.5-1` to `0.1.6-1`, verified the installed
executables, and removed the package while preserving user-data sentinels. It
does not cover package-installed B4 UI flows or native Wayland; those rows stay
open. The packaging output also contains a `namcap` PKGBUILD environment
diagnostic despite an overall exit code of zero. See the [Arch package
lifecycle manifest](evidence/b4-arch-package-lifecycle-2026-10-09/manifest.json).

## Previous pinned B4 acceptance checkpoint (2026-10-09; linux tip 4bcec064)

At this checkpoint, `fork/linux` was `4bcec064d1dc4a8f4905f92f40461e4c215ed23b`
(PR #419). PR #419 updates test-container reuse/support; it does not change
application runtime code. Exact-source Build Linux run
[#37865445368](https://github.com/cozyGarage/BookiE/actions/runs/37865445368)
was explicitly dispatched against immutable product candidate
`5db1cfe5f15ac258472ac04c82f9083ca2ea81aa`; its B4 rollback, TLS, PostgreSQL
release and installed GTK safety jobs completed successfully. The artifact
reports and exact selectors are in the [candidate manifest](evidence/b4-hosted-acceptance-2026-10-09/manifest.json).
This advances B4-7/9/11/12/16/17/22 hosted evidence. On current tip
`4bcec064`, Debian 13 package lifecycle passed install, reinstall, upgrade,
downgrade and purge while preserving user data. The MySQL approval dialog GTK
scenario also passed using `/usr/bin/bookie` from the installed package. See
the [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json).
Other package-installed B4 flows, native Wayland acceptance, and Windows AD
interoperability remain open. PR #420 has since moved `fork/linux` to
`525dee3eb00968a1211161d333e244187c4bce31` with the B3 MongoDB bounded-browse
change. Its Build Linux run [#37872021441](https://github.com/cozyGarage/BookiE/actions/runs/37872021441)
was queued when this note was updated; no B4 acceptance is inferred for that
newer tip from the earlier exact-SHA results.

On exact post-PR-427 `fork/linux` source `b4ee463`, the user-local release executable passed the 53-scenario GTK UI workflow suite and all seven PostgreSQL SSH/mTLS scenarios under Xvfb/AT-SPI. The reported `mysql_unparseable_routine_dialog_denial_preserves_database` scenario also passed twice against MySQL 8.0, so the earlier empty-dialog regression was not reproduced at this tip; its historical cause remains unconfirmed. The approval runner needed the same `ATSPI_DBUS_IMPLEMENTATION=dbus-daemon` setup as the sibling MySQL runner; this change adds it and the script passed without an external override. See [GTK workflows](evidence/gtk-ui-workflows-installed-linux-b4ee-2026-10-09/manifest.json), [SSH flows](evidence/b4-ssh-installed-linux-tip-b4ee-2026-10-09/manifest.json), and [MySQL approval regression evidence](evidence/gtk-mysql-approval-linux-b4ee-2026-10-09/manifest.json). These local runs do not establish distro-package or native Wayland acceptance.

PR #417 adds the MySQL human-approval dialog regression to hosted installed
GTK acceptance. Exact-SHA dispatch [#37867814390](https://github.com/cozyGarage/BookiE/actions/runs/37867814390)
ran against `102ef48017f10c7a63c271608b98d3b05b449795`. Its B4 rollback
(14 MySQL tests and 1 PostgreSQL rollback-failure test), TLS (48 tests), and
PostgreSQL release GTK jobs passed. The PostgreSQL release job's MySQL
human-approval dialog step passed, and all seven SSH/mTLS scenarios passed.
At the initial capture, installed GTK safety and broad driver-integration jobs
were still running. Both subsequently passed; the broad integration layer
passed 462 tests across 13 groups, and the complete workflow concluded
successfully. This hosted pass does not resolve why the operator observed two
earlier empty-dialog failures; the root cause remains open. See the [follow-up
acceptance note](evidence/b4-hosted-acceptance-2026-10-09/focused.txt).

The native SSH suites were also rerun locally on the same exact source SHA with
`bash scripts/test-ssh.sh`: 10 `agent_auth` and 12 `openssh_session` tests
passed, including native multi-hop, second-hop cancellation, host-key refusal,
agent authentication and OpenSSH forwarding. Distribution-package and native
Wayland trust-flow acceptance remain open. See the [native SSH evidence
manifest](evidence/b4-ssh-native-current-2026-10-09/manifest.json).

The SQL Server Kerberos/TLS fixture also passed locally on `102ef480`:
`bash scripts/test-mssql-kerberos.sh` ran both verified-TLS ticket
authentication and unregistered-SPN refusal tests successfully against the
Samba AD fixture. This does not establish Windows AD interoperability or
package-installed acceptance. See the [current Kerberos evidence
manifest](evidence/b4-mssql-kerberos-current-2026-10-09/manifest.json).

## Exact-SHA B4 acceptance checkpoint (2026-10-08)

Earlier B4 layers passed on runtime SHA `a7f14faef07f05bd4a3d63a6ca31b60f2a4e0200`,
including rollback, TLS, SSH, PostgreSQL release GTK, Samba Kerberos,
security-policy and sandbox; its evidence remains in the
[candidate manifest](evidence/b4-acceptance-2026-10-08/manifest.json). The
tested `fork/linux` tip was `ca1dc6002cf269f1eb799fe9f04ce0b8c255a1b9` (PR #388).
On this exact tip, local SSH, PostgreSQL release and Samba Kerberos fixtures
passed for B4-7, B4-16, B4-17 and B4-21. The [current-tip SSH evidence](evidence/b4-ssh-current-tip-2026-10-08/manifest.json)
records 22 native SSH tests and seven GTK SSH trust/audit scenarios; the
[Kerberos evidence](evidence/b4-mssql-kerberos-2026-10-08/manifest.json)
records both Kerberos selectors passing. These were local fixture/Xvfb runs,
not distribution-package or native Wayland acceptance. Build Linux run
[#37794837722](https://github.com/cozyGarage/BookiE/actions/runs/37794837722)
was cancelled before jobs started; Security and Flatpak packaging passed on
that source SHA.

At the previous checkpoint, `fork/linux` was
`c3122d97459685060f1cf3474b55e1a11b961abe` (PR #396). The local PostgreSQL
release fixture passed after rebasing the evidence branch onto that tip; see
the [SSH evidence](evidence/b4-ssh-current-tip-2026-10-08/manifest.json).
Build Linux run
[#37797466945](https://github.com/cozyGarage/BookiE/actions/runs/37797466945)
completed successfully on `c3122d9`. The later run
[#37806196821](https://github.com/cozyGarage/BookiE/actions/runs/37806196821)
completed successfully on `896f1b3`, including rollback, PostgreSQL release,
Driver TLS, integration, installed GTK and regression-gate jobs. The latest
merged `fork/linux` tip is `e697824f92438ea641b0e3b98cdf7d11173ce25a` (PR #403).
Linux Security run [#37849452452](https://github.com/cozyGarage/BookiE/actions/runs/37849452452)
passed; Build Linux run [#37849452394](https://github.com/cozyGarage/BookiE/actions/runs/37849452394)
is pending and Flatpak run [#37849452274](https://github.com/cozyGarage/BookiE/actions/runs/37849452274)
is in progress. No completed current-tip Linux CI contracts run was visible at
this check. Build Linux #37849452394 has not completed its B4 rollback job, so
the skipped PR-event job on #407 or #408 is not hosted rollback acceptance.
These runs do not qualify the selected frozen B3+B4 candidate.
Older hosted and cancelled runs remain historical in the candidate and
rollback manifests. Distribution-package/native Wayland, Windows AD, optional
MySQL engines and broader rollback side effects remain open.

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

The `e697824` checkpoint below is historical. The current product candidate
and hosted job results are recorded above. Rerun affected layers if a later
source SHA changes application or driver code. Remaining work includes
distribution-package/native Wayland acceptance, optional MySQL engine and
side-effect coverage, Windows AD interoperability, and UI-1b per-hop
credential implementation.

The hosted Build Linux run for `8e5d1b18` was
[cancelled](https://github.com/cozyGarage/BookiE/actions/runs/37723088254) as
`linux` advanced through PRs #316–#331 to `b5112ca7`. At the 2026-10-08
check, Security had passed, Build Linux was pending and Flatpak was in progress
([Build Linux](https://github.com/cozyGarage/BookiE/actions/runs/37732479926),
[Flatpak](https://github.com/cozyGarage/BookiE/actions/runs/37732479760),
[Linux Security](https://github.com/cozyGarage/BookiE/actions/runs/37732479814)).
That status is superseded: PRs #351 and #353 have since merged, advancing
`linux` to `32b170f`. Build Linux run
[#37743149992](https://github.com/cozyGarage/BookiE/actions/runs/37743149992)
on that merge SHA is pending; the B4 rollback job has not started yet. PR #353's
PR checks skipped that job by workflow condition, so confirm the post-merge run
when it completes. Neither result is acceptance on a selected frozen candidate.
PR #312 skipped its B4 rollback job because it only changed MongoDB tests. The
focused `b4-rollback` layer passed locally on a documentation-only commit
that was squashed away; the tested source files match `684ea40f`. See the [PR #314 evidence
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
| B4-7, B4-16 | Complete package-installed/native Wayland trust and reconnect acceptance | On exact source `102ef480`, hosted PostgreSQL release GTK passed all seven trust/audit scenarios, including accessible Cancel refusal and tunnel-loss retirement/reconnect. A local Arch candidate built from source snapshot `8fdbed84` passed the unknown-host-key decline/audit scenario on native Wayland, but was only extracted to a user-local prefix. Product source advanced to `395bc1826` via PR #432; docs-only PR #437 and test-only PR #424 moved the branch tip to `5540db7e`; package-manager installation, current-candidate and reconnect acceptance remain open. See [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json), [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json), and [local Arch Wayland evidence](evidence/b4-arch-package-wayland-2026-10-09/manifest.json) |
| B4-9 | Complete package-installed/native Wayland route/auth/TLS acceptance | On exact source `102ef480`, hosted TLS passed all 48 tests and PostgreSQL release GTK passed saved mTLS over SSH. Debian package lifecycle and its installed MySQL approval scenario passed on `4bcec064`; package-installed route/auth/TLS flows and native Wayland remain open. The earlier MongoDB refusal cause remains undetermined. See [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) and [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json) |
| B4-22 | Complete package-installed/native Wayland bundle audit acceptance | On exact source `102ef480`, hosted installed GTK safety passed 53/53 scenarios, including sanitized export/import audits, encrypted credential round-trip and cross-profile import. Debian package lifecycle passed and the installed package's MySQL approval scenario passed on `4bcec064`; package-installed bundle flow and native Wayland remain open. See [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) and [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json) |
| B4-12 | ~~PostgreSQL rollback-failure acceptance on the selected frozen candidate~~ | Hosted candidate and local current-product-source (`fc180933`) backend-termination selectors passed. The scenario confirms `TransactionRollbackFailed`, absent transactional rows and persistent identity/trigger-sequence advancement. See [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) and [local linux-tip evidence](evidence/b4-rollback-linux-tip-2026-10-09/manifest.json) |
| B4-11 | Extend failed-batch tests to optional/vendor MySQL-family engines and broader side-effect patterns | The full `mysql_atomic::` selector passed 18/18 locally on current merged source `c2f3f78b9` (123.93 s); see [current merged-tip evidence](evidence/mysql-atomic-linux-c2f3f78-2026-10-09/manifest.json). Existing coverage includes MySQL/MariaDB advisory locks, `LAST_INSERT_ID()`, trigger session variables, BLACKHOLE, ARCHIVE, CSV, MyISAM, MEMORY, direct/update/delete effects and auto-increment allocation. Optional/vendor engines and other side-effect classes remain open; frozen-candidate, hosted and installed acceptance are not established by this local run. Earlier candidate evidence remains in the [hosted acceptance manifest](evidence/b4-hosted-acceptance-2026-10-09/manifest.json) |
| B4-17 | Complete package-installed/native Wayland trust flow | Product source `93d2e337` passed the system OpenSSH, mTLS, PostgreSQL release and all seven GTK SSH/mTLS scenarios; package snapshot `8fdbed84` passed the unknown-host-key decline/audit scenario on native Wayland and 11 GTK workflows under Xvfb (10 general workflows plus the MySQL approval regression). The candidate archive was extracted locally but not installed by pacman. Product source advanced to `395bc1826` via PR #432. Docs-only PR #437 and test-only PR #424 later moved the branch tip to `5540db7e`; this is not current-candidate acceptance; package-installed multi-hop trust and reconnect remain open. See [current-tip release evidence](evidence/b4-ssh-release-linux-tip-2026-10-09/manifest.json), [local Arch Wayland evidence](evidence/b4-arch-package-wayland-2026-10-09/manifest.json), [hosted candidate evidence](evidence/b4-hosted-acceptance-2026-10-09/manifest.json), [native SSH evidence](evidence/b4-ssh-native-current-2026-10-09/manifest.json), and [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json) |
| B4-21 | Preserve current Samba Kerberos+VerifyFull proof, then perform Windows AD interoperability | On exact source `102ef480`, both Samba AD Kerberos selectors passed with verified TLS (2 tests). Debian package lifecycle and its installed MySQL approval scenario passed on `4bcec064`; package-installed Kerberos flow and Windows AD interoperability remain open. See [current Kerberos evidence](evidence/b4-mssql-kerberos-current-2026-10-09/manifest.json) and [package evidence](evidence/b4-deb-package-lifecycle-2026-10-09/manifest.json) |
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
| B4-21 | Preserve candidate Samba Kerberos+VerifyFull proof, then perform Windows AD interoperability | Both Samba Kerberos fixture tests passed on `ca1dc6002`; see [current-tip evidence](evidence/b4-mssql-kerberos-2026-10-08/manifest.json). It does not establish Windows AD interoperability, which remains open |
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
