# B4 frozen-candidate checkpoint — 2026-10-08

## Candidate

Frozen source: `fork/linux` commit `7eea6f09d7154f03400e82cec7c16215488b2115`.
The worktree was detached at this SHA and clean after local test artifacts were
removed. Later commits through `93bcb839` only changed documentation and the
changelog. The hosted workflows below checked out the frozen SHA explicitly.

The release binary used for local installed-flow checks was staged at
`target/installed/usr/bin/tablepro` and had SHA-256
`76bc17fc94ec6791e72502c580374b6a255585e5571d6bd9d0b9267e110bb0a6`. GTK
checks ran under Xvfb and private D-Bus/AT-SPI, not on a native Wayland desktop
or from an installed distribution package.

## Local candidate evidence

| Layer or focused case | Result | B4 coverage |
| --- | --- | --- |
| `security-policy` | Passed | Policy and audit regressions |
| `ssh` | 22 passed | Native SSH, multi-hop, changed-key refusal, and cancellation at the second-hop trust prompt |
| `tls` | Passed | MySQL, SQL Server, PostgreSQL and other driver TLS fixtures |
| `postgres-release` | Passed | Seven GTK scenarios; saved mTLS, audited host-key/setup refusal, tunnel-loss retirement/reconnect, and multi-hop trust/decline/changed-key handling |
| `postgres-release` with staged release binary | Passed | Repeated all seven scenarios against the staged release binary |
| `ui` | Passed, 28 scenarios | Staged release binary; sanitized bundle export/import audit and encrypted credential round-trip included |
| `mssql-kerberos` | 2 passed | Local Samba AD ticket authentication over VerifyFull TLS and unregistered-SPN refusal |
| MySQL `mysql_atomic` integration group | 9 passed | Failed-batch rollback across InnoDB plus MyISAM, MEMORY, CSV and ARCHIVE trigger effects; INSERT/UPDATE/DELETE and session-variable side effects |
| PostgreSQL `rollback_failure::a_batch_reports_rollback_failure_after_postgres_terminates_its_backend` | 1 passed | `TransactionRollbackFailed`; parent and trigger rows rolled back; identity and trigger sequence allocations survived |

The broad `drivers` layer is not claimed as passed: its prior local run timed
out. The focused MySQL and PostgreSQL cases above completed on this candidate.

## Hosted candidate evidence

- [Linux Security dispatch #37703162399](https://github.com/cozyGarage/BookiE/actions/runs/37703162399) passed both security-policy and supply-chain jobs. Run logs confirm the tested SHA was `7eea6f09d7154f03400e82cec7c16215488b2115`.
- [Build Linux dispatch #37703159104](https://github.com/cozyGarage/BookiE/actions/runs/37703159104) checked out the frozen candidate SHA and completed. Preflight, Driver TLS, PostgreSQL release, Fast GTK, scheduled Clippy, and optional DuckDB passed. Driver integration timed out after its 30-minute budget while compiling the workspace; its tests did not execute. Installed GTK safety failed: 10 scenarios passed, but `encrypted_bundle_round_trip_restores_credentials` reported that confirmed encrypted import did not restore the bundled database credential. The test did not verify that its coordinate click enabled “Replace saved passwords”; the assertion added in PR #283 passed five fresh local sessions. The Linux regression gate failed because the driver integration and GTK jobs failed. The PostgreSQL release job used its debug binary.

- [Build Linux dispatch #37708183936](https://github.com/cozyGarage/BookiE/actions/runs/37708183936) checked out PR #286 head `a056bf1715cd134224caf9c3215d617ae9b5ebb6`, not the frozen SHA above. It passed preflight, Fast GTK, Clippy, Driver TLS, PostgreSQL release, optional DuckDB, and B4 rollback. The hosted GTK safety job ran the staged release binary under Xvfb/private D-Bus and passed encrypted credential restoration with the explicit “Replace saved passwords” checked-state assertion. All seven PostgreSQL SSH GTK scenarios also passed against the staged release binary, including audited refusals, stale-session recovery, and multi-hop trust/decline/changed-key cases. MySQL rollback passed nine cases and the PostgreSQL rollback-failure case passed one case. The broad driver integration tier again timed out during dependency compilation before tests ran. PR #286 head includes non-B4 B3 parser/ClickHouse changes and test/CI updates, so this is hosted evidence for that exact PR head, not an exact-SHA rerun of `7eea6f09`.

- [Build Linux push run #37711722439](https://github.com/cozyGarage/BookiE/actions/runs/37711722439) tested current Linux commit `1e1a30dc75e1d775b616cca77fdcd75490ed0a06`. PostgreSQL release, Driver TLS, optional DuckDB, focused B4 rollback, and staged-release GTK safety all passed. GTK diagnostics confirm `encrypted_bundle_round_trip_restores_credentials` passed; the seven PostgreSQL SSH GTK scenarios passed, including multi-hop trust and stale-session recovery. The broad driver integration layer again timed out while compiling before tests: its internal budget was 30 minutes even though the job allowed 45. This PR raises the layer budget to 45 minutes and the job budget to 55 minutes; the full driver matrix remains unverified until rerun.

## Remaining acceptance

B4-11 and B4-12 scoped rollback contracts passed locally on the frozen
candidate and in hosted runs #37708183936 and #37711722439; broader storage-engine or PostgreSQL
side-effect claims remain outside those exact tests. B4-7, B4-16, and B4-17's
seven PostgreSQL SSH GTK scenarios passed hosted on the staged release binary
at PR #286 head `a056bf1`; B4-22's hosted GTK safety rerun passed encrypted
credential restoration after the checked-state assertion was added. These
hosted results do not make the PR #286 head identical to frozen SHA `7eea6f09`
and do not count as distribution-package installation on native Wayland. The
newer current-Linux run corroborates the GTK, SSH and rollback results at
`1e1a30dc`; it did not execute the broad driver tests because the internal
30-minute layer budget expired during compilation. B4-9 remains unverified
until that matrix completes and installed acceptance is exercised. B4-21's Samba AD fixture passed
locally, but Windows AD interoperability is not established. B4-7, B4-16,
B4-17 and B4-22 still need the applicable installed package/native Wayland
acceptance. Local Arch package acceptance could not start because the host
lacks Rust `>=1.98` and installing it requires unavailable sudo access.
