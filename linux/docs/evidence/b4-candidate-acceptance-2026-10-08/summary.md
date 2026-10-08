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
- [Build Linux dispatch #37703159104](https://github.com/cozyGarage/BookiE/actions/runs/37703159104) was dispatched with the same pinned SHA. Preflight, Driver TLS, PostgreSQL release, Fast GTK and scheduled Clippy passed. Driver integration and optional DuckDB remained in progress at the latest check. Hosted Installed GTK safety smoke failed: 10 scenarios passed, but `encrypted_bundle_round_trip_restores_credentials` reported that confirmed encrypted import did not restore the bundled database credential. The local staged-binary run passed that scenario, so the discrepancy remains unresolved; the hosted diagnostics artifact is retained outside the repository in the candidate cache. Do not treat installed GTK acceptance as passed.

## Remaining acceptance

B4-7, B4-9, B4-11, B4-12, B4-16, B4-17 and B4-22 remain open pending final
hosted candidate results and the applicable installed acceptance. The MySQL
and PostgreSQL focused rollback cases passed locally; hosted driver integration
remains in progress. The hosted installed GTK failure also leaves B4-22 open,
despite the successful local staged-binary rerun. B4-21's Samba AD fixture
passes locally, but Windows AD
interoperability is not tested by that fixture or by hosted CI. Native Wayland
and distribution-package installation/upgrade acceptance are separate from the
Xvfb staged-binary run. Local Arch package acceptance could not start because
the host lacks Rust `>=1.98` and installing it requires unavailable sudo access.
