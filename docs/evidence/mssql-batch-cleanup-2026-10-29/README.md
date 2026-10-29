# SQL Server batch cleanup regression checks

Source tested: `ab18cfa8a6e55b6a805a8df1780bb3f9e7f211b6` on [PR #2042](https://github.com/cozyGarage/BookiE/pull/2042). Work is tracked by [#2033](https://github.com/cozyGarage/BookiE/issues/2033). These are isolated local SQL Server fixture checks, not application or release acceptance.

## Reproduced failures

- A batch that executes COMMIT itself makes SQL Server report a transaction-count mismatch. The driver's subsequent rollback also fails. Before correction, the driver returned an ordinary Transaction error, which implies confirmed rollback. The regression now observes TransactionRollbackFailed and rejects a subsequent query as Disconnected.
- A second query was polled while a WAITFOR batch held the client lock. After failed cleanup retired the client, the queued query still executed. The corrected client acquisition rechecks retirement under the lock; the queued query now returns Disconnected.

## Checks

`cargo test -p tablepro-driver-mssql --test integration batch_cleanup -- --ignored` passed both regressions. Each regression was observed failing before its corresponding correction. The existing `ddl_batch_commits_and_rolls_back_as_a_unit` real-server regression passed separately. File-size, function-size and ignored-test registration checks passed without raising baselines.

Full preflight was still running when this record was written. Commit-failure injection, the complete driver suite and Forgejo acceptance remain unverified. A failed COMMIT conservatively retires the connection and returns an unknown outcome; the rollback regressions do not directly test that branch.
