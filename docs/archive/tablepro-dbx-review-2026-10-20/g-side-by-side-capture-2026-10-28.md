# Side-by-side UI capture, October 28

Dated capture. It records what four database clients showed on the same PostgreSQL 18.6 database on 2026-10-28. It selects no work and decides no design; [#1862](https://github.com/cozyGarage/BookiE/issues/1862), [#1863](https://github.com/cozyGarage/BookiE/issues/1863) and [#1864](https://github.com/cozyGarage/BookiE/issues/1864) own the follow-up. Unlike the rest of this review, these clients were built or installed and run.

## What was run

| Client | Version and source | How it was run |
|---|---|---|
| BookiE | `linux` `3728ae9bc`, debug build | Isolated `XDG_*` dirs, `GSETTINGS_BACKEND=memory` |
| TablePro Linux | upstream `main` `86651d743` (`linux/`), debug build | Isolated `XDG_*` dirs; its GSettings schema compiled from `data/app.tablepro.TablePro.gschema.xml.in` |
| Beekeeper Studio | 6.1.6 (installed package) | `--user-data-dir` in a scratch directory, Wayland |
| dbx | 0.6.58 AppImage, SHA-256 matched the digest GitHub lists for the release asset | Isolated `HOME`/`XDG_*`, `--appimage-extract-and-run` |

Not captured: **DBeaver 26.2.3** (Community tarball, SHA-256 matched). The first launch ignored the isolated `HOME` and opened the maintainer's real workspace, then failed because no X server answered on `DISPLAY=:0`; it changed only that workspace's metadata and log files. A second, isolated launch showed no window within about 55 seconds and was stopped. Also not run: the macOS TablePro app (macOS only); see [c-tablepro-macos-ux.md](c-tablepro-macos-ux.md) for its source-based review.

Data was a PostgreSQL 18 container with `shop.customers` (50 rows), `shop.orders`, a table with 34 PostgreSQL types including edge values, and `public.big` (1,000,000 rows, five columns). All windows were about 920 px wide, so wide-window behaviour is not covered. Screenshots were taken with `grim` and stay in the maintainer's scratch directory; none are committed.

## Findings by screen

### Connecting

| Client | Observed |
|---|---|
| BookiE | `bookie <url>` opens a confirm dialog; Import and Connect saves and connects |
| TablePro Linux | A form with Name, Driver, Host, Port, Database, Username, Password. Encryption defaults to Verify full, so a plain local server failed until it was set to Disable. Pressing Enter in a field submitted the whole form, here with the default port 5432 |
| Beekeeper | Import from URL, or a form with Test and Connect, and Save as a separate step |
| dbx | An optional URL field above the form, Test and Save & Connect in one footer |

### Sidebar

| Client | Observed |
|---|---|
| BookiE | Schemas with Tables and Views groups and counts. Routines, sequences and types are only in the Catalog dialog |
| TablePro Linux | Schemas with one flat list; views and materialized views are told apart by icon. No counts |
| Beekeeper | One tree headed "ENTITIES 228" with schemas and tables |
| dbx | Per schema: Tables, Views, Materialized Views, Procedures, Functions, Sequences and Types; per database: Extensions, Event Triggers, Queries, Users & Privileges |

### Table data and filtering

| Client | Filter | Footer |
|---|---|---|
| BookiE | A panel of four stacked rows: Clear all / Apply / close; saved filters with Apply saved and Delete; a name field with Save as and Rename; the WHERE field; a collapsed rule editor with Add rule | Add row, four pager buttons, "Rows 1 – 50 of 50", a `Rows: 1,000` menu, a text `Filter` button, an export icon, an inspector toggle |
| TablePro Linux | The same panel without saved filters: Clear All / Apply / close, WHERE field, collapsed rule editor | Identical to BookiE's footer |
| Beekeeper | One row, always visible: raw-SQL toggle, column menu, operator menu, value, `+` to add a rule, search button | `Structure`, a row-count slot ("Unknown"), "just now", a page box, refresh, `+`, a gear |
| dbx | One toolbar row, always visible: `All rows` menu, a WHERE input, an ORDER BY input, refresh, timer, add, delete, save, revert; a breadcrumb and a Columns button above it | Total rows with a count-on-demand link, query time, the SQL, a page-size menu, a pager, Export |

BookiE's footer and filter panel are inherited from TablePro Linux, which is why those two match. dbx and Beekeeper keep the filter as a single always-visible row with the column, operator and value (or WHERE and ORDER BY) in place; BookiE and TablePro Linux hide the structured rule editor behind a disclosure and put saved-filter management above the input.

### SQL editor

| Client | Toolbar | Result strip |
|---|---|---|
| BookiE | Two rows. Row 1: caret position, run status, Session, Run, a menu arrow. Row 2: Fold block, Unfold all, Pin results, Explain, Split, all text buttons | A `Results` toggle, an unlabeled list icon ("Show result sets"), an `Errors (0)` button, a spacer, a maximize icon ([#1862](https://github.com/cozyGarage/BookiE/issues/1862)) |
| Beekeeper | None above the editor | Below the editor: `Auto Commit` / `Manual` toggle, Ask AI, Save, Run All with a menu. Status bar: connection, engine, `50000 / 1000000`, `0 affected`, time, Edit Data, Download. An empty result shows a shortcut cheat sheet |
| dbx | One row of icon buttons (run and related actions, save, a more menu), a transaction mode label, and connection / database / schema selectors | A result header with pin and close, the result title, Columns, Go to column, DDL; a tab strip (Table Data and three icons); an error state with Copy, Locate error (line, column) and Fix with AI |

### A million-row result

| Client | Observed |
|---|---|
| BookiE | Streams all rows: about 130,000 at 5 s, 435,000 and 565,000 in later captures, 905,000 when the default 60 s timeout stopped it, and 1,000,000 in 70.7 s with a 240 s timeout. The grid was sometimes shown during the stream and sometimes empty with only the summary and pager. After a timeout the pager shows `Previous page`, `Status` and a disabled `Next page`, and the grid is replaced by "Query timed out" |
| Beekeeper | Shows the first 50,000 rows of 1,000,000 after 1.7 s, with the count in the status bar |
| dbx | Returns one page of 100 rows in 124 ms; the total is counted on demand |
| TablePro Linux | Not run for this query |

## What this suggests for the open issues

This is input for the issues, not a decision.

- [#1862](https://github.com/cozyGarage/BookiE/issues/1862): dbx puts the result title, Columns, Go to column and DDL in one header, and Beekeeper puts the transaction mode and Run in one strip below the editor. Neither has a single bare icon between two labelled buttons.
- [#1863](https://github.com/cozyGarage/BookiE/issues/1863): dbx and Beekeeper load a first page (100 or 50,000 rows) and show the total separately, while BookiE loads every row, which is why its run took 70 s with a counter changing throughout.
- [#1864](https://github.com/cozyGarage/BookiE/issues/1864): a single filter row with column, operator and value, and primary actions grouped in one toolbar row, is the pattern in both clients. The saved-filter controls could move into a menu.
- dbx's per-schema routines, functions, sequences and types in the tree are a candidate for BookiE's sidebar, which today lists only tables and views.

## Not covered

DBeaver; wide windows; light theme; keyboard and screen-reader behaviour of the other clients; editing, import and export flows; MySQL, SQLite and the other engines in the other clients.
