# Known limitations on the `linux` branch

This page describes user-visible behavior and limits in the current source. Current work and acceptance criteria belong to the public issue tracker.

## Data and values

- **SQL editor input is bounded** ([#1299](https://github.com/cozyGarage/BookiE/issues/1299)). Opening or dropping a SQL file, pasting text and restoring saved drafts enforce a 256 KiB UTF-8 physical-line limit and an 8 MiB editor limit. Inputs are refused before changing the editor. An older saved draft over either limit remains saved and opens as a read-only notice until valid SQL replaces it. These limits do not cap total process memory.
- **Full-table export is unavailable** ([ADR 0014](decisions/0014-full-table-export-snapshot.md)). Export the current page or a loaded query result instead. Database snapshots for backup and restore are a separate feature with an accepted design that is not available in the current application ([ADR 0015](decisions/0015-database-snapshots.md)).
- **Large results have materialization limits** ([#1272](https://github.com/cozyGarage/BookiE/issues/1272)). Materialized results stop at 1 million rows, 10 million cells, or 64 MiB estimated size. Single-statement PostgreSQL `SELECT`s stream cursor pages through `PolicyGuard`, retaining them within an estimated 64 MiB row budget while keeping the newest page and the page being read. A process-local encrypted spool ([ADR 0025](decisions/0025-transient-result-page-spool.md)) has a shared 1 GiB quota and a 4,096-page cap per result. Other statement shapes and engines remain materialized. These budgets bound retained data, not total process memory.
- **SQL result filters apply only to loaded rows.** Filter and sort actions are local and do not re-run SQL. Clearing a filter restores loaded rows. Settings carry across reruns of the same statement and pin/unpin while the result remains current or pinned. Selection follows source-row identity when a materialized result is rerendered; a fresh query resets selection. Settings do not survive an app restart.
- **File exports use text markers for undecodable cells** ([#883](https://github.com/cozyGarage/BookiE/issues/883)). CSV, JSON, Markdown, HTML, XML, Excel, copied text and MCP CSV content use a visible `<undecodable TYPE>` string; these text representations do not retain a separate typed marker, so a real text value with the same spelling is ambiguous. SQL export refuses the value. MCP JSON retains an explicit tagged `undecodable` value.
- **Grid cells show bounded text.** Text and JSON cells display at most 512 Unicode characters. Text shows the remaining byte count; JSON shows an ellipsis when bounded serialization stops before the end. Values over 8 KiB use a typed preview. **View Value** displays at most 1 MiB of rendered text or the first 16 KiB of binary data as hex. It includes pending edits and drafts. Persisted rows are refetched by primary key; keyless rows use the loaded value. **Copy Value** copies complete text and JSON values; binary values copy a byte-count placeholder ([#1587](https://github.com/cozyGarage/BookiE/issues/1587)). Text and JSON can be edited in the full-value dialog up to 1 MiB. Invalid JSON cannot be staged, and SQL NULL, empty text, JSON `null` and text `null` remain distinct.
- **Hidden-column projection does not fetch hidden non-key values.** Full-row copy, row JSON and current-page export refuse when required values were not loaded ([#596](https://github.com/cozyGarage/BookiE/issues/596)).
- **A table without a primary key cannot be deleted from the grid.** There is no all-column fallback, by design.
- **ClickHouse nested values** can be exported as JSON. SQL literal export and parameter binding refuse nested values when required type metadata is unavailable.
- **ClickHouse ambiguous local times:** a `DateTime64` in a repeated or missing daylight-saving hour is shown as undecodable and is not converted to a literal.
- **MySQL generated columns:** Structure edits do not preserve every generation expression. Expressions rejected by the metadata safety filter can retain earlier failure or loss behavior ([#1248](https://github.com/cozyGarage/BookiE/issues/1248)).
- **SQLite real numbers:** negative zero and NaN cannot be bound as parameters.
- **SQLite views are not listed.** The sidebar lists tables only; a view can be queried in the SQL editor but does not appear for browsing.
- **SQLite JSON aggregates can vary by system library version** ([#1545](https://github.com/cozyGarage/BookiE/issues/1545)). On older SQLite libraries, a JSON object aggregate with a SQL NULL key may produce malformed JSON. The driver uses the system SQLite library and returns its result.
- **SQL row export does not preserve every server-assigned value.** When column metadata marks a value as generated, SQL export and Copy as INSERT omit it. Auto-increment columns are also omitted for PostgreSQL and SQL Server, so replay assigns fresh values. Row statements do not restore sequence state or remap foreign keys. Do not use them as a relational backup. For PostgreSQL, use the [dedicated backup tools](user-guide.md#back-up-and-restore-postgresql).
- **SQL Server server-owned columns are omitted from row writes.** CSV import drops mapped IDENTITY, computed and rowversion fields; Copy as SQL and SQL export omit generated values so the destination creates them. PostgreSQL, MySQL/MariaDB and SQLite can import explicit auto-increment values when mapped.
- **PostgreSQL sessions use `TimeZone=UTC`** ([#1749](https://github.com/cozyGarage/BookiE/issues/1749)). Date and time functions that depend on the session zone follow UTC rather than the server or role default.
- **Read-only ClickHouse accounts can connect but cannot browse** ([#1237](https://github.com/cozyGarage/BookiE/issues/1237)). Result reads require settings that such accounts may not change, so browsing is refused rather than returning potentially lossy values.
- **Unzoned PostgreSQL `timestamptz` CSV values** are interpreted as UTC ([#857](https://github.com/cozyGarage/BookiE/issues/857)). Include an explicit zone to avoid ambiguity.
- **MongoDB browse pages are separate observations, not one point-in-time view.** Grid updates compare original values of edited fields; deletes compare materialized non-key fields ([ADR 0024](decisions/0024-keyed-grid-writes-check-what-they-overwrite.md)). These checks do not cover fields absent from the loaded set. Column types come from a bounded `_id`-ordered sample of at most 128 documents combined with the displayed page; other fields or types may remain undiscovered. Mixed-type columns are read-only. Reverse, last-page, explicitly sorted and keyless deep-page navigation use offset pagination.
- **MySQL failed batches do not undo every side effect.** InnoDB changes are rolled back, but writes to nontransactional tables and some trigger or session effects can persist.

## Editing and connections

- **Grid saves compare only edited cell values, and relational deletes check the key only** ([ADR 0024](decisions/0024-keyed-grid-writes-check-what-they-overwrite.md)). Case-insensitive equality may miss a case-only text change ([#1820](https://github.com/cozyGarage/BookiE/issues/1820)). A value changed and changed back is also not detected. Engines without checked writes save by key and warn after commit when no row matched.
- **Redis supports a single host/port endpoint.** Sentinel and Cluster topologies are unsupported.
- **PostgreSQL behind a connection pooler:** PgBouncer in session mode works. Transaction mode requires support for prepared statements (PgBouncer 1.21 or newer with `max_prepared_statements` above 0); otherwise backend switching can cause prepared-statement conflicts. Session features such as `SET`, temporary tables and transactions spanning statements require session pooling.
- **SSH jump chains cannot be edited in the form.** Existing saved chains can still connect.
- **Database attachment paths can be skipped if missing, and errors applying existing paths are not surfaced** ([#1993](https://github.com/cozyGarage/BookiE/issues/1993)).
- **Local socket endpoint differences:** PostgreSQL selects a socket directory plus port; MySQL and MariaDB select the full socket-file path. Local sockets reject TLS and SSH and never fall back to TCP. Connection URLs use `?socket=/path/to/mysqld.sock` for MySQL/MariaDB or `?host=/socket/directory` for PostgreSQL, with an empty host or `localhost` and no other option.
- **SQL Server query cancellation is disabled.** An interrupted operation may have an unknown outcome, so its connection or session is retired ([#850](https://github.com/cozyGarage/BookiE/issues/850)).
- **DuckDB read-only flat files** require Linux and a writable directory on the same filesystem as the input file. The driver pins the selected file through a private link. Read-only in-memory connections are refused.

## Interface

- Multi-cursor editing is unavailable. Split panes and SQL code folding are available. Vim keys are an opt-in preference. Preview tabs are temporary and are not restored after restart; kept tabs are restored. Query history opens in a docked panel.
- The PostgreSQL catalog lists materialized views, routines, triggers, sequences, extensions and roles. The activity dialog is a plain result grid; it does not provide a typed activity console.
- Server messages such as NOTICE and PRINT are not displayed. Query timing shows elapsed time only.
- A foreign-key value picker and estimated row counts are unavailable. Go to Referenced Row, enum/set pickers and tab-separated row paste are available.
- PostgreSQL schemas can be hidden from the sidebar per connection ([#580](https://github.com/cozyGarage/BookiE/issues/580)). This display filter does not reduce catalog query work.
- Database snapshots and one-click rollback are unavailable ([ADR 0015](decisions/0015-database-snapshots.md)). PostgreSQL logical backup and restore use the installed `pg_dump` and `pg_restore` tools; see the [user guide](user-guide.md#back-up-and-restore-postgresql). They are not point-in-time snapshots.

## Platforms and packaging

- Native builds require GTK 4.24, libadwaita 1.10 and GtkSourceView 5.22 or newer ([ADR 0021](decisions/0021-toolkit-floor.md)). Older toolkit stacks are unsupported for native packages.
- Windows and macOS are unsupported; the application is built for Linux with GTK 4 and libadwaita.

## Operation outcomes

- **PostgreSQL transaction outcome can remain unknown after session loss.** A disconnected session cannot confirm whether its transaction committed. BookiE records an unknown outcome and blocks governed writes for that connection. A fresh session can run reads and end its read-only snapshot with ROLLBACK. Commits and other governed writes stay blocked until the outcome is reconciled. The app has no in-app reconciliation action ([#947](https://github.com/cozyGarage/BookiE/issues/947)).
