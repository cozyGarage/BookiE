# Stream result spool local evidence

- **Branch**: `ux/stream-result-spool`
- **Base**: `fa50f371c` (`fork/linux` fetched and equal before validation)
- **Implementation commit**: `14634cc`
- **Environment**: Arch Linux / Omarchy, PostgreSQL 17.11 fixture, installed local GTK test harness
- **Acceptance**: #554, same-execution navigation through all fetched PostgreSQL result pages

## Checks

- `bash scripts/preflight.sh`: passed. This included 509 script-harness tests, 2,124 unit tests, 291 sandbox tests and the Redis sandbox integration. Formatting, Clippy, size, panic, crate-boundary and ignored-test inventory guards passed.
- `cargo test -p tablepro-app-services stream_result_spool --lib`: 8 passed.
- `cargo test -p tablepro-app stream_pages --lib`: 15 passed, 1 ignored for isolated GTK.
- `cargo test -p tablepro-app --lib streamed_progress_pluralizes_rows_and_pages_independently`: passed.
- `TABLEPRO_CHANGE_BASE=fa50f371c python3 scripts/run-change-contract-tests.py`: passed after fixing the existing empty-stream-page identity regression (796 app tests passed, 204 ignored).
- `bash scripts/preflight.sh`: passed again after the page-identity fix. The Debian package check was skipped because this host is Arch Linux; no Debian tooling is needed for the Arch acceptance path.
- `cargo build --profile ci --locked -p tablepro-app --bin tablepro-app`: passed.
- `scripts/check-known-issues.py`, `scripts/check-doc-links.py`, `scripts/inventory-ignored-tests.py --check`, Python syntax compilation for `gtk_ux.py`, and `git diff --check`: passed.

The GTK scenario passed with this command:

```bash
PATH=/usr/bin:/bin:/home/trung/.cargo/bin \
TABLEPRO_GTK_BINARY=/home/trung/Projects/bookie-554-audit/target/ci/tablepro-app \
TABLEPRO_GTK_SCENARIO=postgres_large_stream_pages_round_trip_from_spool \
bash scripts/test-gtk-postgres.sh
```

It streamed 100,000 rows with 2 KiB text cells, navigated backward through every 5,000-row page, reloaded earlier pages from the spool, returned to the first page, and checked that the transaction timestamp marker stayed unchanged after visiting other pages. This verifies that page navigation reads the original execution rather than rerunning the query.

`python-atspi` was installed from Arch's `python-atspi` package because the selected `mise` Python did not include the binding. The GTK test uses `/usr/bin/python3`. The Debian package validation was skipped because `dpkg-deb` is not installed on this Arch host.

The installed GTK scenario and checks above are local evidence. GitHub's first PR run found an empty-stream-page identity regression in the full app suite; the fix is included in the current branch work and the full change-contract suite and preflight passed afterward. Refreshed GitHub checks and the Forgejo gate remain outstanding. This evidence does not approve ADR 0025 or close #554.
