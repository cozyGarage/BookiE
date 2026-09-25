# Changelog: TablePro Linux

## [Unreleased]

### Added

- Loaded results can be exported as Markdown, HTML, XML, SQL INSERT statements, or an Excel workbook, alongside CSV and JSON. Statement export is offered only for connections whose engine can express SQL literals, and an Excel export that would exceed a worksheet's row limit is refused before any file is written.
- Saved queries have a management dialog (Ctrl+Shift+D): search them, open one in a new editor tab, rename it, or delete it. Until now a saved query could only be reached through the quick switcher and could not be removed.
- A query that runs for 10 seconds or longer shows a desktop notification when it finishes while the window is not focused.
- Ctrl+O opens a .sql file in a new editor tab. A file larger than the editor's limit is refused with an explanation instead of being cut short.
- An editor tab opened from a .sql file is titled by the file, marked while it has unsaved changes, and saves back to it with Ctrl+S; Ctrl+Shift+S saves to a new file, and Ctrl+S on an unsaved query asks where to save it. If another program changed or removed the file since it was opened, the save stops and offers Overwrite or Save As instead of replacing the other program's changes. Closing a tab with unsaved file changes asks first, and after a restart the tab is linked to its file again; if the file is gone, the tab keeps its text and says it is no longer linked.
- Query history now also records the statements the app itself runs when you save a table structure change or save edited rows, and the history dialog can filter by where a statement came from.
- Logs can be written as structured JSON lines by setting the log format environment variable, for collection by a log agent. Any other value keeps the human-readable format.
- A CSV file can be loaded into an existing table from the table's context menu. The file's separator and header row are detected and can be changed, each table column is mapped to a field before anything runs, and the whole import is approved once. It commits in bounded batches, shows progress, can be cancelled, and reports how many rows were written when it stops early.
- A CSV file can also create the table it is loaded into, from the schema header in the sidebar. The column names and a type guessed from the first rows of the file are shown for every column and can be changed before the table is created. A guess falls back to text whenever any value in the sample does not fit a narrower type.
- The structure editor shows a Comment field for each column, and reads each column's comment on PostgreSQL, MySQL, SQL Server and ClickHouse, and writes it when creating a table, adding a column or editing one. SQLite refuses a comment with a clear message instead of dropping it silently.
- Open Quickly (Ctrl+P) lists the connection's tables and views across every schema. Typing a name or a schema-qualified name such as `sales.orders` finds them, and choosing one opens it.
- A browsed table's cell menu has Filter by This Value, which narrows the table to rows holding that value, or to rows where the column is NULL for an empty cell. It replaces an existing rule on the same column and keeps the others.
- PostgreSQL sessions opened by the app report the application name BookiE, and those opened for agents report BookiE agent, so they can be told apart in `pg_stat_activity`.
- PostgreSQL materialized views are listed with views in the sidebar and open read-only.
- The built-in SSH client can authenticate with keys held by ssh-agent, chosen as "SSH agent" in the SSH section, including in the Flatpak build; a missing agent or an agent with no keys is reported plainly.
- An SSH tunnel can use the system OpenSSH client instead of the built-in one, so `~/.ssh/config`, ProxyJump, ssh-agent and host certificates apply. Host keys are always checked: the app asks before trusting a new host even if `~/.ssh/config` would accept it, and the agent daemon refuses an unknown host. A saved password or passphrase is only given to the host or key it belongs to, never to a jump host. Not available in the Flatpak build, which cannot run the host's ssh.
- An editor tab can turn on Session to run on its own PostgreSQL, MySQL, SQL Server or SQLite file connection, so settings, temporary tables and a transaction started with BEGIN carry over between runs. The button shows when a transaction is open, and turning Session off or closing the tab with one open asks whether to commit or roll back. Every statement still goes through policy, approval and the audit journal, and COMMIT and ROLLBACK are recorded as transaction outcomes.
- A Catalog window in the main menu lists a PostgreSQL connection's functions and procedures, triggers, sequences, extensions, roles, user-defined types and each table's privileges by role, optionally within one schema. An engine without catalog support, a listing refused by policy, a failed listing and an empty one each say so differently.
- Agents can list a PostgreSQL connection's routines, triggers, sequences, extensions, roles, user-defined types and table privileges through the new `list_objects` MCP tool, optionally within one schema. It needs only a read token, is limited to the token's allowed connections, and is audited like other catalog reads; an engine without catalog support answers that the listing is unsupported rather than returning an empty list.
- A saved connection can have its own connect timeout and query timeout, set when it is created. The connect timeout also applies to agent connections; 0 keeps the defaults.
- A saved connection can be given a name; leaving the name blank keeps the address-derived label.
- A saved connection can be duplicated; the copy starts without saved credentials.
- A saved connection can carry a colour tag from a fixed palette, shown beside it in the connection list.
- The connection list is split into a section per group, with ungrouped connections in their own section.
- Saved connections can be exported to and imported from a connection bundle file. A bundle without a passphrase carries no credentials, a passphrase-protected bundle encrypts them, and an import shows every connection and what will happen to it before anything is written.

### Changed

- When a saved password can't be read because no keyring is running, the keyring is locked, or its unlock prompt was cancelled, the connection error now says which one and what to do, instead of showing the raw D-Bus error.
- The keyboard-shortcuts dialog and date editor now use the GNOME 50 APIs.
- Development builds isolate connections, history, audit, drafts, settings, the policy file, locks, and keyring items from installed builds.
- Column-width and filter persistence is owned by the application and shared explicitly with its windows and tabs.
- Query history is owned by the application and passed explicitly to editors, dialogs, preferences, and retention work.
- Workspace cache, coalesced writer, and close-time flush are owned by the application and shared explicitly across windows. A connection whose editor draft cannot be read no longer blocks restoring every other open connection's tabs, and deleting a saved connection now removes its drafts from disk instead of leaving them behind.
- Database connections, user preferences, and the MCP agent bridge are owned by the application and shared explicitly with its windows and dialogs.

### Fixed

- The structure editor shows a column default as the SQL that defines it, so an empty-string default no longer reads as no default, and a MySQL column edit no longer drops an empty-string default or fails on a text default.
- On X11 the app's windows now group under its launcher in docks and task switchers, whichever command started it.
- Changing a PostgreSQL column's type in the structure editor keeps the column's own collation instead of resetting it to the type default, and a MySQL column edit keeps its collation instead of taking the table default.
- A database driver that stops unexpectedly is reported as a failed operation, with writes marked as having an unknown result, instead of leaving the request waiting forever. The connection is then replaced rather than reused.
- Embedded application icons now use the same BookiE artwork as installed packages.
- Malformed MongoDB SCRAM nonces are rejected instead of crashing authentication.
- SSH key and TLS certificate reads remain bounded if a file changes while it is read.
- SSH socket forwarding falls back to a short private path when the runtime directory would exceed Unix limits.
- ClickHouse connection timeouts no longer misreport a certificate hostname mismatch.
- MongoDB server-selection failures (for example, an unreachable host) no longer misreport a certificate hostname mismatch.
- A SQL Server script now keeps running the next `GO` batch after one batch fails, instead of stopping the whole script.
- PostgreSQL indexes built on an expression now appear in the structure tab with the expression as their key, a partial index shows its WHERE condition, and INCLUDE columns are no longer listed as index keys.
- SQLite foreign keys that reference the parent table without naming its columns now show the parent's primary key columns instead of blanks.
- SQLite foreign keys, indexes and columns read right after a structure change are no longer missing when the read lands on another pooled connection.
- A SQL Server batch that raises an error after its first result set now reports the error instead of showing the first result as a success.
- A SQL Server result cut at the row limit now waits for the rest of the batch to finish, instead of leaving it running on the server with its locks held until the next query.
- MongoDB accepts `DROP TABLE people` as well as the quoted form, instead of reporting the statement as unsupported.
- Renaming a column and changing nothing else now saves. On SQLite the whole save was refused, and on MySQL the column definition was restated without its collation, character set or comment.
- MySQL and SQL Server values that the driver cannot decode now show as undecodable instead of as an empty cell. Such a cell stays read-only, and a row whose key could not be read is refused rather than updated or deleted silently.
- A PostgreSQL result with one undecodable value (for example, a NUMERIC too wide to represent) shows that cell as undecodable, logs the column so the cause can be traced, and keeps the rest of the row and result instead of failing the whole query. An undecodable cell stays read-only, Duplicate Row leaves it empty, a row whose key could not be read is refused with an explanation instead of being changed or deleted silently, and SQL exports write a fixed placeholder comment for such a cell.

### Security

- The agent daemon no longer trusts an SSH host key it has not seen before. An unattended connection to an unknown host fails and names the key's fingerprint; connecting once from the app or with `ssh` records it. A changed key is still refused everywhere.
- A lone BEGIN, COMMIT or ROLLBACK on a shared connection is refused with an explanation, in the SQL editor and through MCP. Each statement ran on a shared connection, so a script such as `BEGIN; UPDATE …; ROLLBACK;` committed the update while reporting every step as successful. A whole transaction sent as one batch, such as a SQL Server `GO` batch, still runs.
- A saved connection's SSH jump chain is capped at eight hops, so an edited connection file cannot force a deep recursive parse.

## [0.1.4] - 2026-09-17

### Changed

- Isolated GTK and Secret Service tests through shared local/CI runners.
- Arch and Debian packages: system SQLite, Rust 1.98, and `bookie` launchers with `tablepro` aliases.

### Fixed

- SQL formatting dropping significant whitespace and literal prefixes.
- BOOLEAN cells with mismatched bytes/text entering the inline editor.
- PostgreSQL INTERVAL overflow and malformed INET/CIDR prefixes treated as NULL.
- DuckDB identifier quoting and Redis read-only SELECT in the manual connection lab.
- Debian fallback package installing `tablepro` while the desktop file launched `bookie`.
- ClickHouse VerifyFull reporting a hostname mismatch as a dropped connection.
- MongoDB VerifyFull reporting an IP-endpoint identity mismatch as connection refused.
- Redis VerifyFull reporting an IP-endpoint identity mismatch as connection refused.

### Security

- Agentd session reuse after Secret Service or SSH/CA file rotation.
- Unbounded PostgreSQL and MongoDB SCRAM iteration counts.

## Historical 0.1.x implementation ledger

### September 14 safety sprint

- Copy as IN clause uses the result's SQL dialect and reports omitted values
- SQL splitting preserves escaped quotes, nested PostgreSQL comments and escaped identifiers
- SQL editor warnings identify full-width characters, curly quotes and unusual spaces without rewriting queries
- Clipboard and file exports share exact value formatting, duplicate-column naming and `\x` binary encoding
- CSV export defaults to spreadsheet-safe text; raw text remains an explicit option
- File exports run off the GTK thread with progress, cancellation and atomic replacement
- DDL completion and table/view refresh retain their originating connection and reject stale results
- Redis browsing uses an isolated connection so interruption cannot change the normal command database
- MongoDB filters and inserts preserve BSON numbers when arbitrary-precision JSON is enabled
- MongoDB shell commands reject trailing text instead of executing only a valid prefix
- Replaced the yanked transitive chacha20 0.10.1 release with 0.10.2

### Bug and consistency follow-up

- SQLite preserves NULL in declared numeric, boolean, date/time, and blob columns, and preserves binary values on type mismatch
- JSON page export writes actual binary contents as `\x`-prefixed hexadecimal instead of a byte-count label
- Export writes use unique private temporary files, preventing overlapping exports and pre-existing symlinks from corrupting files
- Workspace restoration keeps the selected editor when unknown saved tab types are removed
- Daemon index and foreign-key metadata calls preserve the driver's controlled path
- GTK regressions cover JSON page contents and process restart; local driver checks include Redis, and fixture scripts resolve their workspace and isolate keyring sessions
- Optional DuckDB CI trusts only its checked-out workspace for Git ownership validation
- Retired the unshippable Oracle/ODPI driver path, which had no working build or runtime support

### September stabilization

- PostgreSQL pattern filters support non-text values without changing typed comparisons; invalid filters no longer produce unfiltered row counts
- Browse page/count planning is shared outside GTK; schema refresh avoids duplicate reads and retired-connection sidebar results are ignored
- The agent daemon preserves controlled view metadata through its tunnel-owning connection wrapper
- DuckDB bundles JSON and Parquet functionality, with flat-file regressions and an explicit optional-feature CI job
- PostgreSQL decodes rows during streaming and caches column type names, reducing large-result memory; measured development-build latency tradeoff is documented
- GTK testing isolates D-Bus/AT-SPI/portal state and waits for debounced search results before activating a favorite
- Documentation now tracks the stabilization evidence, ignored-test inventory and whole-app gaps through macOS 0.72

### Added

- Jump to Column searches browse/result metadata, distinguishes duplicate names by ordinal, and supports Ctrl+Shift+J.
- BookiE display name and original book icon; new package commands retain legacy aliases.
- Arch candidates can be built from an explicit local commit SHA and version before publishing a tag.

- SQL drafts are stored as complete private files instead of being truncated at 256 KiB; the first migration preserves the legacy workspace as a backup

- PostgreSQL views appear in the sidebar as read-only objects, listed through the same policy and timeout path as tables
- Saved connections can name a certificate authority, so a server whose certificate is issued privately can be verified with Verify Ca or Verify Full
- Policy crate with AST SQL classification, PolicyGuard, blast-radius rewrite, column masking, and policy.toml. Statements that read host files, run a program, or send SQL to another server are treated as administrative, so an agent is refused and a read-only connection denies them
- Environment field on saved connections (Local / Dev / Staging / Prod)
- Hash-chained audit journal at `$XDG_DATA_HOME/tablepro/audit.jsonl`
- Interactive `Connection::begin` transactions (PostgreSQL, MySQL, SQLite)
- TLS modes including VerifyFull default for new TLS connections, with certificate hostname and authority failures reported as TLS errors
- MCP bridge (interactive-app loopback HTTP plus on-demand agentd stdio) with scoped tokens and rate limiting, speaking the 2026-07-28 protocol revision while still accepting the two before it, and refusing to listen anywhere but the loopback interface
- Agents can read a table's columns, keys and indexes, count its rows exactly, and page through it, each through the same policy, allowlist and audit checks as any other tool and none of them needing write access
- Headless on-demand `tablepro-agentd` with required `--policy`, issuing read-only tokens by default, with an optional expiry and a way to write the token to an owner-only file instead of the terminal
- GTK approval dialog for policy `RequireApproval` decisions
- Server activity SQL templates (sessions, locks, long-running, replication), with the views an engine cannot answer shown as disabled and explained rather than failing when clicked
- Example policy file and MSSQL in AppStream metainfo
- SSH jump-host chains via nested `ssh.jump` in saved connections and `SshTunnel::open_chain`
- Keyset pagination helper for browse Next past the OFFSET threshold when primary keys are known
- Current-page CSV export with explicit non-snapshot semantics, written so another program never reads a half-finished file
- EXPLAIN plan dialog from the SQL editor and main menu
- Server version cached on connect and shown in the window subtitle
- ClickHouse, Redis, DuckDB, and MongoDB drivers
- Preferences → MCP token pairing (libsecret + loopback endpoint)
- Multi-window via New Window, with each window connecting on its own
- Saved connections can be put in a group, tagged, and starred as favourites, searched by any of those or by driver, and created by pasting a connection URL, whose password goes to the keyring rather than to disk
- Flathub submission notes and screenshot capture guide
- Rust file-size guardrail in preflight: soft 1200 / hard 1800 lines, with ratchet ceilings in `file-size-baselines.txt`
- Driver maturity labels in the Connect dialog (Experimental subtitle for Redis, MongoDB, and DuckDB)
- Driver maturity matrix in `docs/driver-maturity.md`
- Drivers declare whether they can report indexes and foreign keys, so an engine that has none is no longer indistinguishable from a table that has none
- Windows integrated authentication for SQL Server through the current Kerberos ticket cache
- Principal-aware GUI approval routing with fail-closed behavior when no active window exists
- Explicit administrative SQL classification for PostgreSQL backend-control functions and MySQL KILL
- Weekly and manually dispatched full-workspace Clippy on current stable Rust, alongside the Rust 1.93 MSRV gate
- Arch/Omarchy Rust toolchain guidance and a traceable upstream Linux sync log
- Installed GTK safety checks for approval dismissal, approve-once scope, and unavailable audit storage
- Deterministic PostgreSQL release checks for TLS hostname and authority verification, tunnelled access, read-only denial, rollback, blocking locks, and reconnect
- Verify Full for PostgreSQL over SSH, using the original database hostname for certificate checks
- Named `:parameter` placeholders in the SQL editor, with a per-value type choice, sent to the database as bound parameters. Statement splitting and placeholder scanning follow the connected engine's own quoting, so a PostgreSQL function body is run whole and a MySQL `#` comment hides nothing but itself
- Schema-aware editor completion that offers tables after FROM and JOIN, and the columns of the tables in the statement, including through table aliases
- Saved query favorites in `favorites.json`, saved with Ctrl+D
- Open Quickly (Ctrl+P) over favorites, open tabs, and saved connections
- Direct PostgreSQL Unix-socket connections shared by GUI and agentd, with saved-path compatibility and a real socket fixture
- Required GTK connection-isolation smoke plus a daily five-attempt retry-free soak workflow
- Internal Arch RC packaging from an immutable commit archive and verified checksum
- Screen readers now announce the browse toolbar's insert-row button by name
- Installed GTK checks that a connection switch gates pending row edits and that a browse tab reads the connection it was reopened against
- Each window holds its own database connection, so several databases can be open at the same time in separate windows
- A binary column holding valid UTF-8 text now displays as that text instead of a byte count
- Browse marks a foreign-key column's header with 🔗
- Result grids can copy selected rows as TSV, CSV, JSON, Markdown, or an IN clause, show a row as JSON, and export displayed table or query results as CSV or JSON

### Changed

- BookiE app icon: open ledger with a grid page on a teal squircle.
- Preferences no longer offers an Admin MCP token. That option did not grant extra tools or bypass policy, and a stored Admin token still authenticates as read and write.
- Saving several row edits at once writes them in primary-key order, so two windows saving overlapping rows cannot deadlock against each other and a failed save reports the same row every time
- Stop is offered for the engines that can actually abort a statement, because the policy layer no longer hides a driver's cancellation support
- A cell edit is discarded with an explanation if the row moved out from under the editor, instead of being written to whichever row took its place
- Copy row as INSERT leaves out generated columns, which the database always rejects, and escapes a value the way the connected engine reads it, so a row holding a backslash copies as data rather than as SQL
- Stop and the query timeout now abort the running statement on MySQL, ClickHouse and SQLite, so the statement stops in the database and the session can keep writing afterwards
- A cancelled or timed-out SQL Server statement now reports that its outcome is unknown and closes the connection, instead of leaving every later query on that connection waiting forever
- SQLite results show the value of a computed column instead of a blank cell, so counts, aggregates and literal expressions read correctly, and a browse tab shows its total row count
- Browsing, row counts, structure reads, DDL, saving row edits, server activity, EXPLAIN and connecting all honour the configured query timeout instead of running without limit
- MSSQL uses native TLS and keeps its Kerberos service identity when SSH forwards the socket through localhost
- Workspace crates declare AGPL-3.0-or-later; cargo-deny allows that license and documents the rsa Marvin advisory ignore
- Linux UI and DDL modules split by domain: browse tab, grid, editor, app workspace helpers, and `sql_ddl` are directories of focused files instead of multi-thousand-line units
- Read-only is enforced by policy classification (data-modifying CTEs blocked)
- DatabaseService exposes only policy-gated connection handles
- Roadmap and production audit rewritten to match the governed data-plane plan
- Arbitrary SQL query soft row cap raised to 1,000,000 (truncated flag still set); browse pagination remains uncapped by that constant
- Agentd approval strategies are deny (default) or interactive TTY; automatic approval is test-only
- DuckDB driver is an optional `duckdb` Cargo feature (bundled build is large)
- Linux CI runs a non-GTK preflight job before the full GTK checks; local `./scripts/preflight.sh` mirrors that gate
- Policy evaluation takes the resolved connection EnvPolicy once; connection overrides no longer re-run evaluate
- Connect dialog TLS control is a mode picker (Disabled / Prefer / Require / Verify CA / Verify Full), defaulting to Verify Full for network drivers
- Local and development human writes require audit by default; best-effort unaudited writes require an explicit policy setting
- PostgreSQL query timeout and cancellation now wait within bounded deadlines for server confirmation before returning to GTK or MCP callers
- Connection changes validate a candidate before exclusively replacing the old workspace; failed candidates and failed saves keep the old connection active
- PostgreSQL browse pages resolve primary-key ordering before the first row fetch and append PK tie-breakers to user sorts
- Public package metadata names only drivers that are actually shipped

### Fixed

- Updated Rustls to 0.23.45 for RUSTSEC-2026-0285, including its required crypto dependencies.

- PostgreSQL decode failures stay errors, not false NULLs; INTERVAL, INET, CIDR, and LSN still render as text
- Binary values in a TEXT column opening as editable text
- SQL formatting preserves dialect-specific executable tokens and leaves statements unchanged when the formatter cannot safely reflow them
- SSH password and key-passphrase storage failures are reported instead of silently appearing to save successfully
- Column widths and filters persist in order with coalesced writes, preserve unreadable settings files, and flush on application exit

- A denied statement or dismissed approval is refused if that denial cannot be written to the audit journal
- The last open connection is reopened when the app starts. If it cannot connect, its tabs stay with that connection instead of attaching to another database
- Saved connection rows show the environment as a colour
- Measuring how many rows an UPDATE or DELETE would touch now uses the same timeout and cancellation as the write itself, instead of running an unbounded count first
- Reading a table's indexes and foreign keys now stops at the query timeout, and a failed read no longer pretends the table has none
- Structure tabs reopen after a reconnect, and the saved workspace no longer points at the wrong tab when a draft was skipped
- Closing the server activity dialog, or starting another activity query, stops the one that was still running
- Agent table lists and column descriptions now stop at the query timeout, reject invalid names, and are refused when audit storage is unavailable
- A policy file that cannot be read is no longer treated as an empty policy. Agent access stays off until the file can be loaded, and reopening Preferences keeps the previous policy instead of replacing it
- An empty mask list no longer turns off result masking. Use the environment setting that disables agent masking if that is what you want
- Query history export is written so another program never reads a half-finished file
- Setting a cell to NULL or deleting a row from the grid is discarded if the row moved, instead of changing whichever row took its place
- Switching connections no longer clears the previous connection's saved tabs
- A browse page or SQL run that finishes late no longer replaces a newer page or result
- Unsaved edits in one window no longer block or discard edits in another window
- Workspace changes are coalesced off the GTK thread, flushed before the last window closes, and reported instead of silently lost when persistence fails
- Saved connections, favorites, organization data, and MCP tokens preserve valid concurrent updates across processes and refuse malformed, oversized, symlinked, or permissively stored input where applicable
- MCP deadlines cover connection lookup, acquisition, metadata, preview execution, and rollback cleanup, with timed-out or uncertain guarded operations receiving terminal audit outcomes
- The headless agent refreshes cached sessions when saved transport settings change, bounds health checks and connection attempts, and keeps SSH tunnels alive only while issued connections still use them
- Secret Service failures are reported instead of being treated as empty database passwords, while SQLite and DuckDB connections do not require Secret Service
- The headless agent daemon reuses one database session per connection instead of opening a new one on every tool call
- The agent rate limiter forgets idle callers instead of growing for the lifetime of the process
- Saved connections are written so only the owner can read them, and a save that fails part way leaves the previous file intact
- Query plans use the connected engine's own EXPLAIN form, and engines without a query plan statement say so instead of failing on malformed SQL
- Reading a query plan through an agent no longer requires write access, while EXPLAIN ANALYZE stays governed as the write it performs
- Agent CSV export quotes values containing separators, quotation marks, or line breaks instead of producing corrupt rows
- Renaming a column in the structure editor applies the rename instead of failing, and the column's other edits apply to the new name
- PostgreSQL foreign key ON DELETE and ON UPDATE actions are reported as they are defined instead of always reading as NO ACTION
- Integer cells wider than 2^53 keep their exact value when edited instead of being rounded
- CSV export is limited to the loaded filtered page instead of silently paging an unordered changing table
- ClickHouse integration tests connect over plain HTTP instead of default VerifyFull HTTPS
- Flatpak CI bootstraps Rust 1.93 via rustup so the GNOME 47 SDK's older rust-stable extension is not required
- MCP write preview and other `begin()` paths now run statements through PolicyGuard instead of a raw driver transaction
- Agents are denied (and humans must approve) when a blast-radius estimate cannot be computed for UPDATE/DELETE
- Read-only connections deny unparseable SQL before any human approval fallback
- Activity termination accepts only validated positive numeric session identifiers
- Partial environment and connection policies inherit secure environment defaults and default masking rules
- Transactional batches request approval once instead of once per statement
- Mixed SQL batches retain DDL and unscoped UPDATE or DELETE restrictions regardless of statement order
- The agent daemon requires at least one existing saved connection when issuing a token
- Verified legacy audit journals rotate intact when upgrading to durable intent and outcome records
- PostgreSQL cancellation now stops the server query, records a terminal audit outcome, supports parameterized operations and transaction rollback, and discards sessions with unknown outcomes
- Approval dialogs attach to a visible application window when the desktop does not report an active window
- Saved connections expose a keyboard and screen-reader accessible open action
- Drop Table on a MongoDB collection now drops it instead of failing with an unsupported-statement error

### Removed

- Unused `Approve for session` approval outcome (no session-grant store existed)
- Broken stdio-under-systemd agentd unit and generated Python bytecode tracked in the repository
- MCP `search_query_history` until history can be connection-isolated, redacted, rate-limited, and audited

### Security

- Cached agent session reuse after Secret Service or key/certificate file rotation
- Unbounded PostgreSQL SCRAM iteration counts in sqlx-postgres
- Unbounded MongoDB SCRAM iteration counts
- SSH tunnel crashes or over-allocates against a hostile host (CVE-2026-73429, CVE-2026-48107, CVE-2026-46702)
- Unused rkyv 0.7 via rust_decimal (RUSTSEC-2026-0235)
- The headless agent daemon opens a saved connection through its configured SSH chain and verifies the certificate against the real database hostname, instead of dialling the database directly
- MongoDB connections honour the selected TLS mode and certificate authority instead of always connecting without encryption
- MySQL Verify Ca connections no longer crash the application
- ClickHouse connections distinguish encrypt-only from verifying TLS and can name a certificate authority, instead of always verifying against the bundled roots
- Redis connections can use TLS, including a named certificate authority, instead of failing whenever encryption is selected
- MongoDB and Redis connection attempts stop after five seconds instead of waiting on library defaults
- SSH tunnels stop waiting on a host that accepts a connection and never answers, reporting which host and port timed out
- MySQL server-control functions and SQL Server extended and system procedures are recognised as administrative, so agents are denied them the same way they are on PostgreSQL
- Agent write-scope checks classify SQL with the connected engine's dialect instead of always assuming PostgreSQL
- MCP HTTP rejects untrusted browser origins before JSON-RPC dispatch
- SSH host-key changes across key algorithms are treated as mismatches instead of first use
- Linux GitHub Actions are pinned to immutable commits
- TLS certificate verification modes (VerifyCa / VerifyFull) replace encrypt-only Require
- Agent results masked for sensitive column name patterns by default
- MCP tokens with an empty connection allowlist can no longer touch any connection
- SSH stack upgraded to russh 0.60.3 (fixes unbounded allocation advisories and drops vulnerable libcrux 0.0.4)
- Translation locale initialization runs before worker threads and uses the corrected gettext safety contract
- Production GUI and daemon paths no longer construct automatic approval sinks
- Mutations, DDL, administration, and transaction completion persist durable audit intent before database execution and fail closed when required records cannot be written
- Unknown or interrupted write outcomes block later governed writes across restarts and concurrent app and daemon processes
- The audit journal enforces private file permissions, verifies its hash chain, recovers interrupted appends, and serializes writers across processes
- Agentd and in-app MCP refuse service when required audit storage is unavailable
- PostgreSQL administrative and side-effecting function calls are denied to agents and read-only connections while literals and comments remain read-only
- Agent result masking also matches a sensitive column reached through an alias, a wrapping expression, or a subquery, instead of only the result set's reported column name
- A policy pattern the operator adds is applied on top of the built-in sensitive-column patterns instead of replacing them, and an unparseable mask pattern refuses to load instead of silently matching nothing
- An SSH jump hop configured for password authentication is refused instead of silently authenticating with the first hop's password
- ClickHouse identifier quoting escapes a backslash, so a table or column name reported by the server cannot break out of its quoted identifier
- Query history is stored at 0600, including its WAL and SHM files, instead of the filesystem default, so another local account cannot read past statement text
- The headless agent daemon's terminal approval prompt strips control characters from the displayed SQL and rule/reason text instead of writing them to the terminal as-is, and denies an unanswered prompt after two minutes instead of blocking every later tool call indefinitely
- Blast-radius limits now cover INSERT the same way they already covered UPDATE and DELETE, an UPDATE/DELETE's JOIN, USING, or FROM clause is included in the affected-row estimate instead of being silently dropped, and a batch of statements with a known row count each (for example several literal-values INSERTs) is estimated by summing them instead of always requiring approval it can't satisfy
- The administrative-function list denies more PostgreSQL host-access and dblink calls (pg_file_write, pg_file_unlink, dblink_open, dblink_fetch, dblink_connect_u, pg_stat_statements_reset), and SQLite's fileio and extension-loading functions (writefile, readfile, load_extension) are recognised as administrative for the first time
- The MCP execute_query tool refuses a statement that writes instead of committing it directly, so a write can no longer skip execute_write's preview-by-default workflow
- MySQL connections tunnelled through SSH verify Verify Ca and Verify Full against the real database hostname instead of the local tunnel endpoint, matching how PostgreSQL already worked
- Saving a Browse tab edit made before a Structure tab dropped or reordered a column now shows a clear "reload and reapply" error instead of crashing the whole application
- Two windows can no longer open the same saved connection at once; the second is refused with a toast instead of silently taking over the first window's live connection
- Closing a secondary window now closes its database connection, SSH tunnel, and reconnect monitor, and cancels its health-poll and history-prune timers, instead of leaving them running for the rest of the process
- GRANT, REVOKE, COPY, MERGE, and CREATE FUNCTION now name the table or object they target in the audit trail and approval dialog instead of showing a blank object list
- A misspelled policy.toml environment or field name, or a connection override keyed by a UUID typed in a different case, now refuses to load instead of the override silently never applying
- The MCP tools/list method requires the same token tools/call already does, instead of disclosing the full tool catalogue to any caller that can reach the listener
- The MCP stdio server recovers from a non-UTF-8 line the same way it already does from invalid JSON, instead of ending the whole session
- Repeated failed MCP authentication attempts are rate limited even when every attempt uses a different token string
- The MCP HTTP transport now enforces the same request-size limit the stdio transport already did, instead of a larger framework default
- A malformed Redis database index (a typo, stray text) refuses the connection instead of silently landing on database 0, usually the production database
- The query-timeout preference is cached instead of re-read from disk on the GTK thread before every query dispatch
- Preferences → Default page size now offers the same fixed choices (100 / 500 / 1,000 / 5,000 / 10,000) the per-tab paginator does, so a chosen default can no longer silently revert to 1,000 after restart
- Copy row as INSERT now copies the row shown at the clicked position instead of a different row's values when a draft row is pending
- Closing an editor or structure tab now disconnects its dark-mode-change handler instead of leaking the tab's widget tree for the rest of the process
- The editor font-size preference now scopes to the SQL editor instead of resizing every text view on screen, including the EXPLAIN dialog and JSON popover
- Browsing a different Redis database from the sidebar no longer leaves later queries on that connection running against it; the connection's own database is restored afterward
- A failed row-count query now clears the paginator's total and disables Last Page instead of leaving the previous total on screen as if it were still accurate
- An approval dialog now appears on the window that owns the connection the statement runs on, instead of whichever window last had keyboard focus
- SQL Server Verify Ca and Verify Full connections now use a saved certificate authority to verify the server, instead of only ever checking the system trust store
- A KILL QUERY dispatch that outlasts its own two-second cancellation window now finishes and closes the connection it used instead of leaving MySQL's single-slot cancellation pool with a connection whose response was never fully read
- MongoDB connections always request a direct connection, so a replica set member's advertised hostname can no longer redirect the client away from the saved host or its SSH tunnel
- MongoDB errors are now classified from the driver's structured error kind instead of substring-matching the error message, so an unrelated failure whose text happens to contain "connection" or "auth" is no longer misreported as connection-refused or an authentication failure
- Connecting to PostgreSQL or MySQL now makes one authentication attempt instead of two, so a server that locks an account out after repeated failures no longer counts a single Connect click twice
- A cancellation request that outlasts its own two-second dispatch window now finishes and closes the connection it used on PostgreSQL as well as MySQL, instead of risking a protocol-desynced connection in the single-slot cancellation pool
- Restoring a saved workspace with a "new table" draft or an unnamed table tab ahead of the previously active tab now reactivates the correct tab instead of one shifted by the tabs that were never persisted
- The audit journal's hash chain is now verified against each record's exact original bytes instead of a re-serialization of its parsed contents, so a future change to how an audit event is written can no longer retroactively break verification of records already on disk
- SQLite values that don't match their column's declared type (SQLite's type affinity can store a row's value in a different storage class than the column declares) now keep their text or byte representation instead of silently appearing as an empty cell
- MySQL BIGINT UNSIGNED values past i64's range now show their full digits as text instead of appearing as an empty cell
- A SQL Server NUMERIC value past rust_decimal's range no longer crashes the query; it now shows its full precision as text instead
- MySQL DECIMAL values past rust_decimal's range now show their exact digits as text instead of appearing as an empty cell
