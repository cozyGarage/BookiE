# Source-build manual run and dbx comparison, 2026-10-29

This record covers a driven run of a debug build. It is not an installed-package acceptance run and does not replace the [installed Wayland records](installed-wayland-0.2.0-2026-10-23/README.md). The per-item results are in the [checklists](../manual-verification-0.2-features.md#areas).

## Identity

- Source: `65befe8aa` on `linux`, equal to GitHub `linux` at the start of the run. Debug build with `cargo build -p tablepro-app -j4` (a 16-job build was killed by `systemd-oomd` in the 2026-10-28 run).
- Session: native Wayland on Hyprland 0.56.2, workspace 5, 1896 by 1150 window, display scale 1, dark appearance with a light pass for the Production, ENUM and SET checks. A float and resize to 800 by 1000 was used for the narrow checks.
- Toolkit: GTK 4.24.1, libadwaita 1.10.0, GtkSourceView 5.22.0.
- Profile: private `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME`. `GSETTINGS_BACKEND=keyfile` for preference persistence, because the `memory` backend drops every preference on exit.
- Fixtures: `tests/manual-connections/docker-compose.yml` (PostgreSQL 17.11, MySQL 8.4.11, MongoDB 8.0.30, Redis 7.4, ClickHouse 24.8, SQL Server 2022), a second PostgreSQL 18.6 container for the restore failure, and a SQLite 3.54 file. PostgreSQL held `shop` and `Mixed Case` schemas, a 300,000-row `big` table, a 1,100,000-row `huge` table, a `docs` table with JSON, `bytea`, a generated column and a 1,200,000-character cell, composite and deferred foreign keys. MongoDB held 300 documents.
- Driving: pointer through `ydotool` (started by the maintainer, absolute coordinates are halved), keys through `wtype` and `ydotool key`, screenshots through `grim`, state checked with `psql`, `mysql`, `sqlite3`, the audit journal and `hyprctl`. Screenshots are not committed.

## Coverage

Counted from the four checklists after this run. "Ticked" means every clause of the item has a recorded passing result.

| Checklist | Items | Ticked | Open |
|---|---|---|---|
| Catalog, sidebar and application shell | 46 | 15 | 31 |
| Connections and SSH | 48 | 24 | 24 |
| SQL editor and sessions | 63 | 34 | 29 |
| Grid, filters and data transfer | 83 | 48 | 35 |
| **All four** | 240 | 121 | 119 |

Other contributors added 20 items to the checklists while this run merged `linux`, so the open count is higher than the 101 recorded at the first push. Counts are from the files at the head of this run.

The run added a note or changed a tick on 52 items. One item, the native Wayland repeat of the restart restore, was unticked because its own text says that repeat is untested.

## Observations that need an owner

These were seen on `65befe8aa`. None has been filed from this run; the rows below say whether an issue already exists.

- MySQL `EXPLAIN` returns "No rows returned" in the editor and an empty Text tab in the Explain dialog, while the `mysql` client returns plan rows. Still present.
- MySQL Session: `create table` ran inside an open Session transaction with no refusal, and `lock tables`, `set autocommit=0` and `savepoint` asked for approval instead of being refused. The checklist expects refusals.
- PostgreSQL Session: a typed `commit` after an aborted transaction ends with "No rows returned" and no rollback wording, while the audit records `rolled_back`.
- Approval prompts now read "Targets: (none detected)" and `SHOW STATUS LIKE` inside a MySQL Session ran without approval, so [#1919](https://github.com/cozyGarage/BookiE/issues/1919) and [#1925](https://github.com/cozyGarage/BookiE/issues/1925) look fixed.
- The 1,200,000-character View Value dialog opens in 3 to 5 s and its Copy value copies all 1,200,000 bytes, so [#1979](https://github.com/cozyGarage/BookiE/issues/1979) and the clipboard discrepancy look fixed on this build.
- Ctrl+Shift+D still opens the GTK Inspector prompt and an inspector window ([#1948](https://github.com/cozyGarage/BookiE/issues/1948)). The MySQL SET picker was not clipped at 1896 px ([#1945](https://github.com/cozyGarage/BookiE/issues/1945) not reproduced, narrow width not tried). SQLite still omits views from the sidebar ([#1908](https://github.com/cozyGarage/BookiE/issues/1908)).
- Open Quickly (Ctrl+P) does not close on Escape. A browse tab for a view says "This table has no primary key".
- One saved connection with an unknown `environment` value (`production` instead of `prod`) hides every saved connection, with "No connections yet" and nothing in the log. The file is left untouched.
- Restore refusals after an unknown outcome say "a prior audit outcome could not be persisted" on the first write and repeat "an unresolved write outcome blocks this connection" twice after a reconnect.
- A hidden column makes Go to Referenced Row greyed out instead of showing a toast, and removes the edit entries from the cell menu.
- The Pin results button sits where the Session button moves when the transaction banner appears, so a second click lands on Pin results.
- With a pending edit open, a wrong path typed into the portal file chooser opens an error dialog that does not show in screenshots and blocks the chooser until it is closed with `hyprctl`.

## Findings after the first push

- Redis 7.4.11: "Filter by This Value" on a Key cell adds `Key equals a` and the tab then fails with "Redis bound reads support the Value column for one key". The filter persists with the tab across a restart until Clear all is used.
- SQL Server: the sidebar and Catalog list no views, triggers or sequences outside tables (see the application checklist).
- Connection bundles drop configured SQLite attachments (source read, see the connections checklist).
- The ER diagram zoom label stays at 100% while the canvas zooms.
- SQLite views are listed in the sidebar on `e226d3f5c`; they were missing on `65befe8aa`.

## Not run

Orca and any screen reader, a language input method, DuckDB, MariaDB and Kerberos fixtures, the `.deb`, Flatpak and AppImage packages, ProxyJump and system OpenSSH password cases, local-socket MySQL, a contrast-ratio measurement, a system high-contrast setting, scale 1.25 and 2, enlarged desktop text, and opening the `.xlsx` in LibreOffice (not installed). An Excel export above the row limit cannot be reached from the browse scope, which refuses all rows first.

## dbx comparison

dbx 0.6.59 (`/usr/bin/dbx`, a Tauri and web view client) was run with a private profile and connected to the same PostgreSQL. The notes record what was visible, not an endorsement. BookiE keeps its own constraints: GTK4 and libadwaita, no web view, no account.

| Area | dbx | BookiE today | Worth considering |
|---|---|---|---|
| New connection | Categorised grid of engines with brand icons, a search box, list and grid toggle, a JDBC option and a paste-URL field | A dropdown of seven drivers in one scrolling form | Driver tiles with icons and a URL paste field |
| Connection form | Tabs for Connection, TLS and SSL, Tunnel and proxy, Advanced; inline colour swatches; Test beside Save and Connect; optional notes | One page; Test and Connect; colour set from the list menu | Group the form by tab or section; show colour swatches in the form |
| Home view | Dashboard with connection counts, Quick Start per connection, shortcuts, recent history | A list of connections only | A recent connections and queries panel |
| Sidebar tree | Tints the whole subtree with the connection colour; databases with a Default badge; groups for Tables, Views, Materialized Views, Procedures, Functions, Sequences, Types, Extensions and Users and Privileges | Schemas with Tables and Views groups; the rest only in the Catalog window | Surface Functions, Sequences and Materialized Views as groups, or link the Catalog from the tree |
| Grid | Right-aligned numbers, row-number column, data-type colouring, sort arrows in headers, executed SQL and query time in the footer, selection sum, average, minimum and maximum | Left-aligned numbers, no row-number column, type text under the name, selection statistics in a popover | Right-align numeric columns; show the executed SQL and time under the grid |
| Pending edits | Explicit Commit and Rollback buttons beside Add Row and Delete Row | Pending cells tinted with underlines, a footer with Preview SQL, Discard and Save | BookiE's non-hue underline marks are an advantage to keep |
| Cell menu | Sort, Filter, Highlight, Hide identical columns, Bulk Edit Selection, Generate Value, Transpose Row, Screenshot selected cells, Open with | Edit Full Value, Copy as, Filter by This Value, Go to Referenced Row, Show Referencing Rows, Show Row as JSON, Jump to Column | Sort from the cell menu; Bulk Edit Selection |
| Editor | Toolbar of icon buttons, connection and schema pickers, commit mode selector, error gutter, Locate error and Copy, Fix with AI | Run, Explain, Split, Session, a fold menu; error gutter marks; Go to error | A visible commit-mode control (Session is less discoverable) |
| Results | Tabs for Table Data, Summary (per statement status, rows, time), Chart (bar, line, pie) and Messages | Results and Errors, result sets selector, pinned results | A per-statement summary; a basic chart |
| Settings | Fourteen themes including editor presets, UI scale, fonts, corner style, tab bar options, formatter, snippets, rebindable and searchable shortcuts with conflict detection | System light and dark, editor font and size, Vim keys | Searchable shortcuts page; SQL formatter |
| Navigation | Quick open (Ctrl+P) across all connections and objects with type chips; back and forward navigation; reopen closed tab | Open Quickly for favourites, tabs, connections and tables; reopen closed tab (Ctrl+Shift+T) | Show the connection path on Open Quickly rows |
| Themes | Fourteen presets and a custom option | Five themes (Default, Ledger, Catppuccin, Tokyo Night, Gruvbox) chosen in Preferences | Keep; add a high-contrast variant |
| Colour | Light "White" default with a green accent and black primary buttons; tint per connection | Follows the system; blue accent; Production red bar | Keep the system theme; consider tinting the sidebar by connection colour |

Safe-by-design differences are not gaps. BookiE refuses unsafe writes through `PolicyGuard`, asks for approval, audits every statement and runs sessions as a separate transaction owner; dbx shows an Auto-commit selector instead.

## Cleanup

The app and `ydotoold` stay running until the maintainer stops them; the compose project `bookie-manual-20261029`, the `bm-pg18` container and the private profiles under `/tmp/opencode` were started by this run and are listed here for removal.

## Regression found and fixed after the merge

After merging `linux` at `0a46b6bd0` (the commit `df1996177` that added enum metadata to query columns), every browse of a PostgreSQL table with an enum column failed with "Couldn't load rows: error occurred while decoding column 4: mismatched types; Rust type `String` (as SQL type `TEXT`) is not compatible with SQL type `"CHAR"`". The metadata query reads `pg_type.typtype`, which is the `"char"` type. The existing integration test `enum_labels_come_back_in_declared_order_for_the_named_type_only` failed on the unfixed code and passes with `typtype::text`. The fix is one line on branch `fix/postgres-enum-typtype-decode` (commit `98e136030`), pushed to GitHub without a pull request because `gh` is not authenticated on this machine. It has not been through the Forgejo gate.

## Contrast sample

Measured from full-resolution captures on the grid, not from class names. Light header text is 2.28:1 and dark header text is 3.76:1; both are below 4.5:1. Other sampled text is 7:1 or higher. The values are on the [application checklist](../manual-verification-app.md#visual-audit-matrix).
