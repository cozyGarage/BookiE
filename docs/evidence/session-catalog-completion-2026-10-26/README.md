# Session catalog completion, October 26

Issue: [#1698](https://github.com/cozyGarage/BookiE/issues/1698)

## Finding

The installed GTK scenarios reproduced two failures on the pre-fix application. After `Commit & close session`, an independent SQLite `sqlite_master` query confirmed the new table was committed, but the sidebar did not show it. After `Roll back & close session`, the rolled-back table stayed absent as expected, but a later autocommit table was present in SQLite and missing from the sidebar. The raw AT-SPI results are in [before](before/).

The fix moves transaction-aware catalog tracking to GTK-free `tablepro-app-services`. The Relm4 callback applies only an acknowledged session outcome, then publishes through the existing connection-identity check. An acknowledged rollback clears pending DDL without publication; errors, cancellation and timeout keep it for retry. The raw post-fix results are in [after](after/).

## Exact source and installed evidence

- Fix commit: `9d00c2594` (`fix(app): publish session catalog changes on completion`), based on Linux `f61a52b6d`.
- The installed pre-fix binary SHA-256 is `504ae8c525948a33cefb6e69a67365cdc6be6d1f553dba76fbf604764dbadbba`. It was built for #1695 and includes that PR's unrelated Structure metadata change. The catalog and session-mode source at the reproduction base matched Linux `f2db1eed1`; the two failures isolate the session catalog behavior.
- After the fix, both installed scenarios passed on a fresh optimized fixture-installed binary under isolated X11/Xvfb with AT-SPI and a private SQLite database: `session_commit_refreshes_committed_catalog` (9.874 s) and `session_rollback_does_not_hide_later_autocommit_ddl` (10.616 s). Each verifies the independent database oracle before asserting the sidebar state.
- `CatalogChanges` focused tests cover acknowledged commit and rollback, duplicate completion, failure/retry, cancellation, timeout, no pending DDL, SQL `COMMIT AND CHAIN`, and non-transactional DDL behavior.
- Existing application catalog tests passed: 2/2.
- `bash scripts/preflight.sh` passed: 2,204 unit tests, 291 sandbox tests and the Redis cancellation test; the workspace contract checks passed.
- `bash scripts/test-gtk-widgets.sh` passed: 175 isolated widget tests. This is a separate tier from installed AT-SPI acceptance.
- The two installed GTK safety scenarios were invoked directly against the fixture-installed binary. The packet records those two scenarios, not the full installed GTK safety suite.
- Strict application Clippy passed: `cargo clippy -p tablepro-app --all-targets -- -D warnings`.
- Documentation checks passed: `check-known-issues.py`, `check-doc-links.py`, and `inventory-ignored-tests.py --check`.

This is local installed evidence on SQLite, not native Wayland, a system package, a multi-engine transaction guarantee, GitHub PR acceptance, or the maintainer-owned Forgejo gate. The regression code and tests landed together; no existing tests were removed or weakened.
