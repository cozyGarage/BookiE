# Historical test adoption

Archived from `aeac107a4` on 2026-10-03. Counts and uncovered behavior below
belong to September 16; several named gaps have since gained regressions.
Use [testing](../testing.md), [the B3 board](../type-contract-strategy.md) and
[the scenario survey](b3-test-scenario-survey.md) for current work.

## Upstream test-suite parity

The macOS `main` branch's Swift test suite (roughly 1,300 files, mostly under
`Packages/TableProCore/Tests` and `TableProTests`) is a source of test *cases*
Linux can adopt even though the two apps share no code. Most of it does not
transfer: it exercises Apple-only surfaces (Keychain, `NSView` focus, CloudKit
sync, license tiers, the Vim engine, TeamLibrary) or plugins for engines
Linux does not ship (Snowflake, BigQuery, DynamoDB, Elasticsearch, Etcd,
CockroachDB, Cassandra, SurrealDB, Teradata, Trino, Beancount). Foreign-app
connection import (DBeaver, DataGrip, Navicat, TablePlus, KeePass) is a
missing *feature*, not a test gap, and is a product decision, not a QA task.

Reviewed 2026-09-16. Adoption targets, ranked by value, each ported as its
own change with a regression test that fails against the current code first:

1. **SSH.** Done. Added known_hosts-directory IO-error handling,
   `PrivateKey` passphrase Debug redaction, `map_connect_error` precedence
   between a recorded host-key mismatch and a raw connect error, and the
   forwarded Unix-socket path-length guard (accept and reject). 7 → 13
   tests in `crates/ssh/src/lib.rs`.
2. **Cell/value formatting.** Done, scoped down: `PhpSerializeParserTests`,
   `HexEditorTests`, and `CellValueContentDetectorTests` have no Linux
   analogue (no hex viewer, no PHP-serialize/content-type detection
   feature) — missing features, not test gaps, so left alone. Added
   embedded-NUL bytes, empty bytes, edit-text-for-bytes, and truncation
   boundary tests to [grid rendering](../../crates/app/src/ui/grid/display.rs)
   (11 → 17 tests). Surfaced but did not fix a real risk found along the
   way: `is_cell_editable` gates on the column's declared type name only,
   not the runtime `Value`, so a SQLite type-affinity mismatch (a `TEXT`
   column holding actual blob bytes for one row) could show `<N bytes>` as
   editable text and let a careless commit overwrite the real binary data.
   Needs its own slice inside the cell-factory bind path; not attempted
   here since it touches the same GTK `ColumnView` area already flagged as
   needing visual verification before changes.
3. **Row copy/paste.** Done, scoped down: multi-cell/multi-row *paste* is
   an intentionally unsupported feature (`PasteNotSupported` toast), not a
   test gap. `render_csv`/`render_tsv`/`render_json`/`render_markdown` in
   `core/export.rs` were already thoroughly tested. Added tests for
   `escape_tsv_cell` (TSV clipboard copy structural-character guard) and
   `normalize_single_line_input` (multi-line paste collapse into a
   single-line cell edit) in `crates/app/src/ui/browse_tab/` — both were
   live, called code with zero coverage.
4. **Storage migration/corruption.** Smaller than expected once read:
   `crates/storage/src/connections.rs` already has 29 tests covering
   truncated files, oversized files/entries, legacy-version rejection,
   legacy-field defaults, and forward-compatible unknown-field carrying
   across a downgrade — ahead of main's equivalent suite in places. The
   one real gap found: `outcome_summary` in `query_history.rs` (formats a
   history entry's outcome for SQL/CSV export) had zero tests. Added 6
   covering all branches.
5. **Redis correctness.** Done. `redis_value_to_result` and `redis_scalar`
   (reply-to-`QueryResult`/`Value` mapping) had zero tests despite handling
   every reply shape, including lossy UTF-8 decoding of binary values and
   the `MAX_QUERY_ROWS` truncation path. `urlencoding_lite` and
   `map_redis_error` (connection-refusal vs. auth-failure vs. generic query
   error classification) were also untested. 7 → 19 tests in
   `crates/drivers/redis/src/lib.rs`.

All five items landed in the `test/ssh-hardening` branch: 34 new tests, 5
commits, full workspace suite 879 → 911 passing. Continue the upstream
review from here as new upstream releases land, rather than treating this
pass as exhaustive.

