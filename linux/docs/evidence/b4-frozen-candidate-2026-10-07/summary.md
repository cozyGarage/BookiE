# Frozen B4 candidate evidence — 2026-10-08

## Candidate and result

All recorded runs used clean frozen source SHA `0d22bfe9ddd9d1c66478fa55b751e3a887a992ca` (`fork/linux` PR #248 merge). The tracked diff was empty. Per-layer reports and logs are retained beside the [manifest](manifest.json), which records hashes.

The `full`, `security-policy`, `tls`, `postgres-release`, `mssql-kerberos`, `ssh`, and `ui` layers passed. The broad `drivers` layer hit its 30-minute limit, so it remains timed out as a whole. Before timeout, the relevant MySQL atomicity suite passed (72 MySQL integration tests total) and the PostgreSQL suite passed all 216 tests, including the rollback-failure selector. The Arch package contract passed; Debian package validation did not run because `dpkg-deb` is unavailable on the host.

## Item findings

| Item | Frozen-candidate evidence | Still open |
| --- | --- | --- |
| B4-7 | `security-policy`, native SSH (22 tests), and PostgreSQL release GTK (7 scenarios) passed. The release scenarios include terminal audit outcomes for host-key refusal and setup failure. | Hosted checks and distribution-package/native Wayland acceptance |
| B4-9 | Candidate `tls`, `postgres-release`, and `mssql-kerberos` layers passed for the documented route/auth/TLS slices. | Hosted matrix validation and distribution-package/native Wayland acceptance |
| B4-11 | All nine MySQL atomicity cases passed. Tests cover InnoDB rollback, nontransactional MyISAM/MEMORY/CSV/ARCHIVE trigger effects, UPDATE/DELETE trigger effects, and a surviving session-variable trigger side effect. | Hosted candidate run |
| B4-12 | The PostgreSQL backend-termination selector passed within the 216-test suite. It confirms `TransactionRollbackFailed`, absent table writes, and identity and trigger sequence values advanced despite rollback. | Hosted candidate run |
| B4-16 | The candidate release run passed tunnel-loss/session retirement/reconnect. The same scenario passed with the staged installed binary. | Hosted checks and distribution-package/native Wayland acceptance |
| B4-17 | Native SSH tests passed two-hop success, changed-key refusal, and cancellation while the second-hop trust prompt was pending. Candidate release GTK tests plus staged installed-binary GTK rerun passed trust on both hops, second-hop decline without learning, and changed-key refusal. | Hosted checks and distribution-package/native Wayland acceptance |
| B4-21 | Local Samba AD fixture passed the Kerberos VerifyFull query and unregistered-SPN refusal (2 tests). | Real Windows AD interoperability and hosted/package acceptance |
| B4-22 | The candidate UI layer passed 50 named scenarios, including bundle export audit, import audit, and encrypted credential round-trip. | Hosted checks and distribution-package/native Wayland acceptance |
| UI-1b | Reviewed the existing [per-hop secrets proposal](../../proposals/ui-1b-ssh-jump-chain-editor.md). Its staged migration, stable hop IDs, Secret Service references, cache identity, bundle v2, and test requirements are coherent. | Keep edit refusal until the proposal is implemented across storage, transport, bundles, and GTK together |

The staged `target/installed/usr/bin/tablepro` GTK checks run under private D-Bus and Xvfb. They validate a staged installed binary on this candidate, but do not count as installing a distro package into the live native Wayland desktop.
