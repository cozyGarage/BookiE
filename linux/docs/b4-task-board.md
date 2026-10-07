# B4 task board: transport and sessions

Base commit: `aeac107a4`, branch `linux`, reviewed 2026-10-03. The local source
includes the MySQL batch/result fixes integrated in `de5831eb1`, indexed in the
[evidence manifest](evidence/mysql-atomic-results-2026-10-03/manifest.json).
The [sprint](bookie-0.2-sprint.md) owns release order; [ADR 0008](decisions/0008-connection-and-session-ownership.md)
owns the decisions. B4 remains open. Implementation, local execution, hosted
execution and installed acceptance are separate states.

## B4 continuation status (October 3)

| Task | Integrated source | Remaining acceptance |
| --- | --- | --- |
| A1–A5, B1–B5, C1–C5, D1–D8, G1/G2/G4, H1–H4 | Prior implementation and regressions retained | Recheck affected cases on the frozen candidate; A5's consumer is F9 |
| E1/E2, G3, F1/F2/F3 | Merged in #13–#18 | No reimplementation; native/shared policy, verified cache and close-route acceptance at candidate SHA |
| F5 | Merged as `4b7814f5e` (#19) | Installed retirement/toggle flow |
| F4/F9 | Merged as `573435766` (#20) | Monitor consumes tunnel closure and swaps live identity; native reconnect and installed stale-session acceptance |
| F6 | Merged as `d1434438e` and follow-up `6e2c5687a` (#22/#23) | GUI defaults to refusal and installs the built-in prompter; decline/accept fixtures exist. Native two-hop success, second-hop key-change refusal, and in-flight cancellation while the second-hop trust prompt is pending passed locally on `f3a1765` via `bash scripts/test-ssh.sh`; installed trust flow remains a candidate gate. See [SSH multi-hop evidence](evidence/ssh-multihop-2026-10-07/manifest.json) |
| F8 | GUI generation split merged as `6346a431c` (#24); daemon generations merged as `09c5351a3` | Policy, GUI and daemon scope uncertainty per connection generation while sharing journal failure. Agentd regressions cover isolation, replacement, late cancellation and shared journal failure; frozen-candidate and installed restart acceptance remain |

Source integration above was checked locally. No fresh hosted result is inferred
from a merge. Historical local/hosted reports and original packets are preserved
in [B4 history](archive/b4-history.md); missing worktree/cache paths remain unavailable
evidence. Fresh audit results are in [the release audit](archive/release-audit-2026-10-03.md).

## C6 tunneled TLS local fixture evidence (October 7)

PR #150 merged the TLS-only MySQL and SQL Server fixtures behind an SSH bastion
restricted to those two services. The ignored per-engine cases exercise
BookiE's built-in SSH forwarding and preserve the original service identity for
TLS verification. Local `bash scripts/test-driver-tls.sh` passed all 43 tests
on merged HEAD `11672cbe6`, including 11 MySQL and 9 SQL Server cases. Both
drivers execute a native query through the tunnel with a valid CA and hostname,
reject an unrelated CA, reject wrong and local-dial identities, and clean up the
local forward. MySQL refuses plaintext on its TLS-only endpoint; SQL Server's
native `encrypt_option` proves the server forces encrypted sessions.

The PR #150 hosted Build run did not execute the TLS fixture: preflight failed
because the ignored-test inventory was stale, and the regression gate correctly
rejected the dependent skipped jobs. PR #151's hosted harness then found two
follow-up defects: its new G5 case was misclassified by the inventory generator,
and `known-issues.md` retained duplicate B4-1 through B4-4 rows. PR #153 merged
those fixes and moved SSH host-key generation from the bastion image build to
container startup after SonarCloud reported image-build generation. The
follow-up establishes the hosted fixture and inventory checks; C6 still needs a
successful hosted execution of the fixture itself and frozen-candidate
acceptance.

The tests exercise BookiE's built-in SSH route; the OpenSSH route and installed
acceptance remain unproven. The fixture correction also replaces ineffective
SQL Server TLS environment variables with the documented `mssql-conf` TLS
settings.

## Remaining tasks

Each row is a bounded task; implement engines and route variants separately.
Use existing fixtures, wrappers and the layer catalog. The release audit gives
additional privacy, value and evidence tasks without duplicating this board.

| ID | Small task and required assertion | Local layers |
| --- | --- | --- |
| G5 | Exercise the actual daemon provider through system OpenSSH: unattended unknown key declines without learning; pretrusted host reaches a guarded query | `ssh`, `postgres-release` |
| F8-headless | Extend headless acceptance for late cancellation after replacement and journal-failure propagation across cached and replacement generations | `security-policy`, agentd units, `postgres-release` |
| I2 | Selecting system OpenSSH inside Flatpak refuses explicitly before subprocess/driver dispatch; no backend switch. Add a deterministic sandbox-context regression | `harness`, transport units |
| I5 | Record tunnel setup and host-key refusal through the approved audit contract; success/denial/cancel/error each has one safe terminal outcome | `security-policy`, `ssh`, `postgres-release` |
| F7 | Isolated GTK Session → BEGIN → transaction label → toggle-off confirmation; Cancel retains session, rollback settles before closing; register selector once | `widgets`, `postgres-release`; `ui` if the safety flow changes |
| I3 | Reconcile route/auth/TLS evidence after C6/G5/I2; exact engine/backend/SHA/selector, unsupported and unrun combinations explicit | Documentation links and evidence inventory |
| B4-atomic-batch / B4-rollback-error | MySQL source and native regressions complete locally: non-DML batches are refused before dispatch, failed rollback reports unknown outcome, and a trigger-to-MyISAM side effect is shown to survive InnoDB rollback with an accurate UI warning. PostgreSQL native rollback-failure acceptance and broader engine/side-effect coverage remain open | [Retained MySQL evidence](evidence/mysql-atomic-results-2026-10-03/manifest.json); frozen candidate acceptance remains |
| B4-MySQL-engine-atomicity | **Narrow contract tested locally:** failed InnoDB DML rolls back InnoDB rows, but an AFTER INSERT trigger's MyISAM write survives; the UI warns that non-transactional writes may remain. Other storage engines and side-effect patterns still need scoped proof | [Native trigger regression and evidence](evidence/mysql-atomic-results-2026-10-03/manifest.json); broader candidate acceptance remains |
| I1 | In the Debian phase, build/install executable `tablepro-askpass` in debhelper rules and make validator reject its absence; inspect the rules-built package | `packaging-contracts`, Debian package build |

## In-progress local slices

| ID | Current implementation and validation | Remaining |
| --- | --- | --- |
| B4-22 | ADR [0010](decisions/0010-administrative-action-audit.md) defines privacy-bounded administrative audit events; code records durable intent/outcome around confirmed bundle export/import and gates side effects on intent durability. The focused interrupted-intent recovery, fail-closed and paired-event checks pass; `cargo test -p tablepro-policy -p tablepro-storage -p tablepro-app --lib` passed (903 passed, 45 ignored) | GTK bundle-flow plus frozen-candidate and hosted acceptance remain |

## Completed local slices

| ID | Local evidence | Scope remaining |
| --- | --- | --- |
| G5 | `bash scripts/test-postgres-release.sh` on the BookiE `b4/g5-daemon-openssh` worktree, 2026-10-07: actual agentd provider refuses an unattended unknown system OpenSSH key without writing it, then reaches a guarded PostgreSQL query with a pretrusted key. The command also passed the existing PostgreSQL release integration suite | Re-run on the frozen B3+B4 candidate SHA; hosted and installed acceptance remain separate |
| F8-headless | `agentd::audit_isolation_tests`: connection A uncertainty leaves B writable; replacement recovers; cancellation of an old write after replacement does not poison the new generation; outcome journal failure blocks other sessions and replacement generations | Re-run on the frozen B3+B4 candidate SHA; hosted and installed restart acceptance remain separate |
| C6-MySQL / C6-SQLServer | Commit `0ecb5bc4f`; `bash scripts/test-driver-tls.sh` passed all 43 tests on 2026-10-07, including MySQL 11 and SQL Server 9. Both engine fixtures execute queries through built-in SSH forwarding with valid TLS identity and refuse invalid CA/hostname cases without plaintext fallback | Re-run on frozen candidate SHA; hosted results, system OpenSSH route and installed acceptance remain separate |

Headless panic retirement is covered by `a_session_whose_driver_panicked_is_not_reused_even_though_its_ping_is_healthy`;
include it in the combined G5/F8 candidate acceptance.

The agentd panic-retirement unit test uses a credential-free saved connection
and carries its tracing subscriber through the async operation. Secret Service
behavior stays in the dedicated keyring tier.

## Acceptance order

1. Continue B3's bounded cases and evidence reconciliation. B3 completion
   precedes closing B4 acceptance; independent review/preparation may proceed.
2. Finish C6, G5, headless F8, I2, I5 and F7; update I3 with actual results.
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
