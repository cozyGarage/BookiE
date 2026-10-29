# Linux and open pull request audit status, October 29

This is an interim record, not a completed audit or merge approval. The reviewed Linux base was `58292488fd6e10336d2186fdaa5284241b34c303`. The open-PR inventory contained 49 entries. Later changes require another review. The [earlier non-UI audit](non-ui-code-audit-2026-10-29.md) covers a different source snapshot.

## Corrections published during review

The linked PR comments hold the commands, source identities and validation limits. Passing local checks do not replace the required acceptance gate.

| PR | Finding and correction | Verification |
| --- | --- | --- |
| [#2013](https://github.com/cozyGarage/BookiE/pull/2013) | Decreasing XLSX row references caused subtraction overflow. Reject them before calculating skipped rows. | New regression failed before the fix; all nine XLSX tests passed after refresh. |
| [#1891](https://github.com/cozyGarage/BookiE/pull/1891) | A temporary rename could collide with an existing differently cased column. Compare generated temporary names without ASCII case distinctions. | Regression failed before the fix; four rename tests and full preflight passed. |
| [#1955](https://github.com/cozyGarage/BookiE/pull/1955) | Stored marker-shaped text could be mistaken for a truncated preview. Carry per-column truncation flags from the query and remove them before exposing results. | Regression failed before the fix; focused browse, native SQLite/PostgreSQL/MySQL/SQL Server checks and full preflight passed. Full GTK acceptance was not repeated. |
| [#1893](https://github.com/cozyGarage/BookiE/pull/1893) | MongoDB shell input ending with an incomplete chained call was accepted. Reject the incomplete remainder. | Regression failed before the fix; 59 unit tests, Clippy and full preflight passed. |
| [#1890](https://github.com/cozyGarage/BookiE/pull/1890) | ClickHouse checked its line budget after growing the pending buffer. Check the combined length first. | Buffer length/capacity regression, 50 unit tests, real-server oversized-response case and full preflight passed. |
| [#1899](https://github.com/cozyGarage/BookiE/pull/1899) | SQL Server mask lookup used non-Unicode literals. A column with Japanese identifiers lost its mask during alteration. Use Unicode literals. | New real-server regression failed before the fix; all three mask cases and full preflight passed on `a278c49babaf958a9591db98994727f1ef2073ac`. |
| [#1988](https://github.com/cozyGarage/BookiE/pull/1988) | Catalog lock acquisition happened before deadline enforcement. Bound setup while preserving another active operation. | Held-lock regression failed before the fix; 47 unit tests and full preflight passed on `b054fe1da989c9b0a6b3a0a9b4b7e263e040ffb7`. |

## Integration repairs

- [#1944](https://github.com/cozyGarage/BookiE/pull/1944) was refreshed without replacing current SQLite attached-schema listing with main-only listing. Its SQLite test, two app-services deduplication tests, native MySQL/MariaDB view tests and full preflight passed on `21112acbbcab65dc9fa3218ab852d27eff5beba8`. GTK acceptance remains outstanding.
- [#2010](https://github.com/cozyGarage/BookiE/pull/2010) was refreshed and its existing floating-point tests moved to a support module to satisfy the file-size guard. Both moved tests, five attachment tests and the SQLite missing-file test passed. Full preflight and optional DuckDB checks were pending when this record was written. [#1173](https://github.com/cozyGarage/BookiE/issues/1173) stays open until acceptance is satisfied.

## Open findings and limits

| Priority | Finding | Evidence and next step |
| --- | --- | --- |
| P2 | SQL Server batch cleanup can discard rollback failures and retain a client after an unconfirmed completion. | Source-confirmed error-handling gap, tracked in [#2033](https://github.com/cozyGarage/BookiE/issues/2033). Resulting server state has not been reproduced. Test injected cleanup failures and prevent reuse when completion is uncertain. |
| P2 | Redis collection previews have no hard byte bound in [#1903](https://github.com/cozyGarage/BookiE/pull/1903). | Redis 7 returned 1,048,585 bytes from HSCAN with COUNT 20 for one 1 MiB hash value. A count hint does not bound member bytes. String GETRANGE is bounded; collection preview behavior needs a separate bound and regression. |
| P2 | DuckDB's missing-path check has a check/open race. | Source shows an existence check followed by an open that can create a database. Concurrent-removal behavior is unverified. Do not claim the already-missing-path test proves a no-create invariant under concurrent removal. |
| P3 | SSH authentication errors can be misreported as an attempt limit after one refused identity. | Source review of [#1937](https://github.com/cozyGarage/BookiE/pull/1937). Preserve unrelated I/O and signing failures; distinguish a diagnosis from confirmed server refusal. |

These priorities describe review attention, not confirmed data-loss incidents. The mask-loss case above was reproduced and corrected on its PR branch; it is not evidence that the fix has merged.

## Other reviewed changes

- [#1878](https://github.com/cozyGarage/BookiE/pull/1878): the MCP offset ceiling precedes driver acquisition; Redis checks addition and address-space conversion. No new defect found in the diff. Independent SCAN requests still do not establish stable pagination.
- [#1881](https://github.com/cozyGarage/BookiE/pull/1881): pending outcomes retain recovery intent; terminal outcomes remove it. No new defect found in the diff. A terminal-after-pending test would strengthen the crash-recovery coverage.
- [#1924](https://github.com/cozyGarage/BookiE/pull/1924) includes [#1923](https://github.com/cozyGarage/BookiE/pull/1923). Direct TCP probes do not establish recovery timing or SSH tunnel liveness.
- [#1961](https://github.com/cozyGarage/BookiE/pull/1961) covers original text keys and edited values. Its SQL Server comparison includes byte length, and PostgreSQL qualifies its deterministic collation. Reconcile the overlapping collation PRs before acceptance; test the combined empty-string and floating-point changes.
- [#2032](https://github.com/cozyGarage/BookiE/pull/2032) carries attachment configuration through bundles. Usable attachments are a separate guarantee: failed attachment reporting is tracked by [#1993](https://github.com/cozyGarage/BookiE/issues/1993).

## Remaining scope

The 49-PR inventory has not received complete independent acceptance testing. Full decoder coverage against ADR 0007, native Wayland behavior, complete driver matrices, the remaining open diffs and changed PR comments still require review. Source inspection, focused regressions and full preflight have different scopes; none alone proves the whole application correct. No Forgejo run or merge was started as part of these review corrections.
