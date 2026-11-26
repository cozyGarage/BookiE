# BookiE GPUI app

The GPUI frontend for BookiE, built with `gpui-pre` 0.3.12. GPUI is the primary frontend for new product work; GTK is the historical Linux frontend and remains the current primary Linux release until GPUI passes the installed release gate ([ADR 0058](../../docs/decisions/0058-gpui-primary-gtk-historical.md)). This tree lives at `apps/gpui` with its own lockfile. Tagged releases may include an unsupported Linux x86_64 preview; accessibility remains unverified, and the preview is not a platform-support claim.

By default it shows a synthetic result grid. `BOOKIE_PROTOTYPE_FILE` loads a SQLite or DuckDB table read-only. `BOOKIE_PROTOTYPE_CONNECTION` selects a saved connection, and `BOOKIE_PROTOTYPE_TABLE` selects a table to browse through app-services and a `PolicyGuard`-wrapped session. On a saved connection not marked read-only, supported table edits, row inserts and deletes, structure changes, and CSV/XLSX imports can be reviewed and saved through the guarded app-services write path; database permissions and policy still apply. SQL query tabs are read-only outside the explicit transaction flow. ADR 0043 and 0044 remain as history; ADR 0058 defines the current frontend direction.

It has its own `Cargo.toml` with an empty `[workspace]` table, so it is not a member of the BookiE workspace, and its own `Cargo.lock`. It depends on `bookie-core` for typed values, `bookie-results` for paging, bounded scanning and text search, `bookie-app-services` for sessions, writes and shared services, and `bookie-driver-registry` for the static drivers. UI components come from `gpui-component` and `gpui-kit`. Equality filters are typed and exact through `bookie-results` when the column's declared type is a known edit kind (core's filter classifier decides); other columns compare the displayed text exactly and case-sensitively. A text filter such as `label = Sample 1` therefore matches only that exact case.

## Scope

The synthetic model in `src/model.rs` has no GPU dependency and is unit tested without a display. Its generator is deterministic (`value` is 9007199254740993 + row, `label` is `Sample N`, `state` is NULL for every third row and alternates true/false otherwise). It defines the filter predicates (`contains`, `=`, `!=`; `!=` keeps NULL) and the result types for Find (10,000-match cap) and whole-row and keyed compare (exact counts, 200 shown differences). Find, filter and compare run in `src/source.rs`. The pager is `bookie_results::result_page`: 500, 1,000, 5,000 or 20,000 rows per page (5,000 by default).

Settings has four pages: Appearance (color theme and dark mode), Fonts, Keyboard (shortcuts) and MCP (local server and tokens). Connection access stays with each connection and export limits stay with the export operation. Global policy defaults and a configurable query row limit are not exposed by this frontend.

The window is drawn by `src/render.rs` (main view and overlays), `src/chrome.rs` (toolbar, sidebar, tab bar, settings and status bar) and one `*_ui.rs` file per feature. It has:

- A result toolbar: progress, a paging menu (500, 1,000, 5,000 or 20,000 rows per page, **Scroll continuously**, or **Append pages**, which keeps earlier pages in view), Previous page, the row range and Next page, an Export menu, Find with Previous and Next match and **Show matching rows only**, **Filter rows**, **Columns** and **More grid actions**. The Export menu copies the page or selection as CSV or opens the file export dialog; formats and row scopes are listed in [known limitations](../../docs/known-limitations.md#gpui-frontend-appsgpui-unsupported-preview). Where each surface opens is set in [docs/ui-design.md](../../docs/ui-design.md#where-content-goes-gpui).
- A WHERE row on table tabs: the clause is sent to the database through the shared browse query, so it filters the whole table, not only the loaded rows. Enter or Apply reloads the table, Clear removes the clause, and a chip shows the current sort.
- A filter row for loaded rows (Ctrl+F): a predicate such as `label contains 7`, Apply and Clear all.
- A compare bar with Whole row and Key: value.
- In the synthetic source, a fourth `typed` column that cycles through every value kind (float, bytes, date, time, datetime, timestamp with time zone, decimal, UUID, JSON, raw JSON text, undecodable, NULL), searchable with Find and the filter row.
- A grid built on the `DataTable` component from `gpui-kit` (`src/grid_table.rs`). Find, filter and compare run on the background executor; replaced work is cancelled and stale replies are discarded by generation.

Find, the filter row and the cell editor use `src/text_input.rs`, a single-line input ported from GPUI's own `examples/input.rs` (Apache-2.0, comments removed). It has a caret, mouse and Shift selection, Ctrl+A/C/X/V, Home/End, grapheme-aware deletion and the IME hooks (`EntityInputHandler`). These fields are single line. The Query tab uses a separate multiline SQL editor.

The grid has a keyboard cell cursor (the `grid_cursor` module of `bookie-results`). Click a cell or focus the grid, then the arrow keys move one cell, Page Up and Page Down a screenful, Home and End to the row ends and Ctrl+Home and Ctrl+End to the corners. Moving past the last row of a page continues on the next page. Space selects the cursor row, Shift+Up and Shift+Down extend the selection, and Enter or F2 edits the cell. The active row and column are tinted, the active cell has a blue outline, and the status line names it. Numeric values and row numbers align to the right, following the DBX grid's numeric layout.

Ctrl+Shift+P opens a command palette, as in Zed. Matching is in `src/palette.rs`, the command list and actions in `src/palette_actions.rs`, palette state in `src/palette_state.rs`, and the overlay in `src/render.rs`: type letters of a command, Up and Down choose, Enter runs, Escape closes. It lists the window's commands, including page navigation, Find, Filter, Compare, the paging modes, Copy and Edit the active cell, and hides commands that do not apply. It reuses the same `TextInput` as Find, so it also checks that component's keyboard and IME handling in a second place.

Double-click a label, state or typed cell to edit it. Enter applies scalar edits and Escape cancels them. Long text, multiline text and JSON open a full-value editor with Apply and Cancel; it preserves the complete staged value, including line endings and Unicode, and refuses values above 1 MiB instead of silently clipping them. Invalid JSON stays in the editor and does not replace an earlier staged edit. Values are validated by `src/core_bridge.rs` through `bookie_core::import::parse_cell`; `src/edit.rs` forwards to it. Synthetic-row edits remain view-only. Edits to an eligible live table are staged with an amber underline and are not sent until the user reviews and confirms Save. Copy includes the full pending values.

Connection and table changes are refused while staged grid edits are pending. Closing a result or query tab with pending edits is also refused, and an open cell editor cannot be replaced by switching grids or results. **Commands → Discard all view-only edits** discards pending edits without sending them; live table changes reach the database only through the separate review and Save confirmation flow.

Rows can be selected with a click, extended with Shift+click, and copied with Ctrl+C (Ctrl+Shift+C adds the header row) through the GPUI clipboard as tab-separated text (`bookie-app-services::result_copy`). The quoting follows the core export rules and is covered by unit tests; copy is limited to 5,000 rows. Writing to the real system clipboard was not verified, because a headless run cannot read it back.

Copy as INSERT is available for table rows. Catalog, tabs and generated SQL retain the selected table's schema and name as separate identifiers, including names containing dots or quotes. Query results do not infer an INSERT target from the previously browsed table.

The look follows DBX (Apache-2.0, [LICENSE](https://github.com/t8y2/dbx/blob/main/LICENSE)): its light and dark design tokens from `apps/desktop/src/styles/tokens.css` (white and near-black surfaces, `#e5e5e5` borders, 4 px radii, a muted header), its data-grid constants (26 px rows, 13 px text, striped rows `#f0f0f0`, blue active row, italic muted NULL) and outline buttons. Source code adapted from DBX is listed in [adopted-code attribution](../../docs/adopted-code-attribution.md); the upstream [NOTICE](https://github.com/t8y2/dbx/blob/main/NOTICE) records third-party contributions.

The window also follows DBX's layout (`src/chrome.rs`, state in `src/shell.rs`), with synthetic data by default: a top toolbar (Home, an expand-sidebar button while the sidebar is hidden, the connection's environment chip, New connection, New query and the connection Import / Export menu on the left; More, the command palette, the theme switch and window controls on the right), a resizable sidebar (340 px by default, `src/sidebar_state.rs`) with a search box and a connection → database → table tree (click to expand; typing in the search filters the tables and expands the tree), a horizontally scrolling tab bar with a permanent Home tab and closable table tabs, a Home dashboard (statistics, Quick Start, Shortcuts, Query History), a row-number gutter and a status bar. The selected tab is brought into view. Only `synthetic_rows` has data; the other tables open a tab that says so. Ctrl+B hides the sidebar and the command palette can open Home or the table. Light is the default. `BOOKIE_PROTOTYPE_THEME=dark` starts dark, and the toolbar button and the command palette switch between light and dark. The tokens live in `src/theme.rs`. Not copied from DBX: the Geist font (the window uses the system UI font) and its canvas-drawn cells.

Icons are Lucide, the set DBX uses (ISC licence, `assets/icons/LICENSE`): 45 SVGs embedded in the binary through GPUI's asset source (`src/assets.rs`) and drawn in the theme's muted colour. Our GTK application's symbolic icons are the other option; they are not wired here.

The UI font defaults to the first installed of Inter, SF Pro Text, Segoe UI, Adwaita Sans, Roboto, Noto Sans and Fira Sans (`UI_FONTS` in `src/main.rs`), and otherwise to the system font. `BOOKIE_PROTOTYPE_FONT="Some Family"` is tried first at start. The palette's **Choose font…** lists every installed family (type to filter) and applies it at once. The Fonts settings page sets the UI and editor font family and size. Font and theme choices are saved in `$XDG_CONFIG_HOME/bookie-gpui/settings.json`, and window geometry in `window.json` beside it; these files ignore `BOOKIE_PROFILE`.

The built-in themes are BookiE Light and BookiE Dark, both with the DBX tokens in [DESIGN.md](../../DESIGN.md) (`BOOKIE_PROTOTYPE_THEME=dark` starts dark). Bundled color themes in `themes/` (Catppuccin, Dracula, Gruvbox, Nord, One, Solarized, Tokyo Night and VS Code; sources and licences in [themes/README.md](themes/README.md)) can be chosen on the Appearance page, and the choice is saved with the font settings. The Adwaita style was removed: [ADR 0054](../../docs/decisions/0054-one-visual-language.md) gives both applications one look.

Visible cell text is limited to 512 Unicode characters before layout. Line breaks, tabs and NULs use visible markers. Byte cells show at most 16 bytes in hexadecimal plus their exact byte count. These previews apply to normal and comparison rows; editing and copying still use full values. Painting requests each visible row once and shares it across its four cells. The synthetic source still constructs an owned row for each request, so this does not bound source allocation or establish real-result performance.

## Schema diagrams

Connect to a database and choose **Show the schema diagram** from Commands. Pick a schema, find a table, or select its card to inspect columns and related tables. **Open data** and **Structure** open the selected table. Drag a card header to move it or drag the handle to change its width. **Layout → Save layout** keeps the arrangement for the current connection and scope. Mermaid and DBML export use the diagram scope.

On a writable connection that supports adding foreign keys, choose **Add relationship** on the source table. Pick the referenced table and add the column pairs in order. **Review SQL** opens the existing save review. BookiE checks the tables again before sending the change through the normal approval and audit path.

On saved writable connections with transactional DDL, use **Create table** or choose **Add columns** on an existing card. A new table can define nullable columns and single-column or composite primary keys. **Review SQL** shows the exact statements to apply. The diagram refreshes after the change.

## Renderer boundary

The formatter tests expand the [large-cell inputs](testdata/large-cells.json). They check bounded previews and exact typed-value round trips. They do not measure native rendering or process memory.

Application logic belongs in app-services and the application acts as a renderer. The library half (`src/lib.rs`) has no `gpui` import and is tested without a display:

| Module | Holds |
|---|---|
| `model` | synthetic rows, column metadata, filter predicates, Find and compare result types |
| `source` | the `ResultSource` trait and its synthetic implementation, Find, filter and compare over scanned rows |
| `live` | live sessions through `bookie-app-services`, table loads, queries, transactions and writes |
| `selection` | row selection and the copy limit |
| `edit`, `core_bridge` | typed cell validation through `bookie-core` |
| `palette` | command matching and ordering |
| `shell` | sidebar tree and tab model |
| `theme`, `type_colors` | colour and size tokens |
| `keymap` | shortcut bindings and user overrides |
| `font_settings`, `window_geometry` | saved fonts, theme and window geometry |
| `detail`, `preview` | value detail text and bounded cell previews |
| `messages`, `status_view` | server messages and status text |
| `chart_source`, `pubsub_view`, `net_check`, `host_key`, `save_check` | chart data, pub/sub samples, reachability checks, SSH host-key prompts and save checks |
| `csv_delimiter`, `export_limit` | palette command parsing for the CSV delimiter and export limit |

The binary half is the frontend. `main.rs` defines `GridView`, which owns the window's view state, staged edits and background work tickets, and starts the app. `render.rs` and `chrome.rs` render that state, `grid_table.rs` holds the grid, and the `*_ui.rs` files add one feature each. `GridView` still owns both state and layout decisions; moving more state behind library types would let the renderer read them and send intents (a key press, a click) back.

**Ctrl+T** opens a Query tab with the SQL editor above a draggable split and the result pane below. Each run has its own tab. **Ctrl+Enter** executes the selection or the statement at the active caret. **Ctrl+Shift+Enter** executes the whole buffer. A standalone query shows elapsed time and a **Stop** control. Live queries are read-only outside the explicit transaction flow.

The gutter marks the current run target. The editor highlights matching parentheses and occurrences of a selected SQL word. When a database supplies an error position, the editor underlines the token in the submitted text. **Tools** includes Find, Replace, SQL formatting and snippets. Press **Ctrl+Space** to request SQL and catalog suggestions. The editor also suggests names after SQL clauses such as `FROM` and after a qualifier's dot.

**Shift+Alt+Up** and **Shift+Alt+Down** add a caret on the line above or below. **Alt+click** adds one at the pointer. Typing or deleting applies at each caret in one undo step. **Escape** returns to a single caret.

The view reads rows, Find, filter and compare from a `ResultSource` trait (`src/source.rs`); implementations include the synthetic generator and a live table loaded through `bookie-app-services::ReadSession`. A test drives the whole view from a 3-row source that is not the generator. `BOOKIE_PROTOTYPE_START=query` opens the Query tab at start.

Synthetic row counts are 150,000, 500,000 or 1,000,000 (the default) through `BOOKIE_PROTOTYPE_ROWS`. Keyboard, IME and mouse behavior is covered by `src/ui_tests.rs` and `src/ui_tests/`, the `*_tests.rs` files beside each feature, and `tests/saved_write.rs`, which runs a saved SQLite write in an isolated profile and checks its audit record. The UI tests drive the views through GPUI's `test-support` context. A window cannot be opened under Xvfb here (no DRI3 for the Vulkan surface), so how it looks and feels has not been checked by eye.

`BOOKIE_PROTOTYPE_WIDTH` and `BOOKIE_PROTOTYPE_HEIGHT` set the window size.

## Run and test

From the repository root:

```bash
scripts/gpui-cargo.sh test --locked
scripts/gpui-cargo.sh deny --config deny.toml check advisories licenses
scripts/gpui-cargo.sh run --release --locked
```

The wrapper uses the primary checkout's `target` directory when run from a Git worktree, so GPUI builds reuse Cargo and DuckDB artifacts across BookiE worktrees. Outside a Git checkout it defaults to `${XDG_CACHE_HOME:-~/.cache}/bookie/gpui-target`. It also uses the installed `sccache` for Rust and C/C++ compilation when available, so unchanged DuckDB C++ sources can be reused if Cargo reruns their build. Set `CARGO_TARGET_DIR` explicitly to use another target directory.

Forgejo runs the model and simulated interaction tests without a display. Native rendering and system clipboard behavior require a desktop session.

The window needs a Wayland or X11 session with a Vulkan driver that can present.

The GitHub macOS check builds this standalone GPUI workspace on a macOS 15 arm64 runner with Rust 1.98 and Xcode command line tools. GPUI uses Metal on macOS. The check compiles the application and runs headless tests; it does not verify native rendering, accessibility, clipboard behavior, packaging or macOS support. The Arch package remains a separate GTK build of `bookie-app`.

## Scroll and memory measurement

To measure scrolling, build the release binary and run `measure.mjs` from a desktop session. Close other heavy applications first and repeat a run; one machine and one GPU driver is all this shows.

```bash
scripts/gpui-cargo.sh build --release --locked
target_dir="$(scripts/gpui-cargo.sh target-dir)"
node apps/gpui/measure.mjs --binary "$target_dir/release/bookie-gpui" --rows=150000 --steps=120 --runs=3
```

The script starts the release binary on the current display, waits for the application to report its first frame, scrolls the first 5,000-row page from top to bottom in the requested number of steps, samples resident and proportional memory of the process every 400 ms, and prints one JSON line per run. It steps, then waits briefly so total elapsed time is comparable.

`--pacing=frame` instead advances the scroll on every frame and requests the next one, so the recorded frame intervals show the real frame time. `--scope=continuous` scrolls the whole result instead of one page.
The application prints `PROTOTYPE_FIRST_FRAME`, `PROTOTYPE_SCROLL_DONE steps=… elapsed_ms=…` and, in frame mode, `frame_p50_ms`, `frame_p95_ms`, `frame_p99_ms` and `frame_max_ms`.

## Limits

- The default source is synthetic. A saved connection can also be opened from the Home page. An empty catalog opens a query tab on the live connection. Set `BOOKIE_PROTOTYPE_TABLE` and either `BOOKIE_PROTOTYPE_FILE` for SQLite or DuckDB or `BOOKIE_PROTOTYPE_CONNECTION` for a saved connection to load a live table through `ReadSession`. A table opened with `BOOKIE_PROTOTYPE_FILE` is read-only; writes need a saved writable connection. Live data is capped at 100,000 rows and subject to the shared decoded-byte and cell budgets. The status marks results that may be incomplete. PostgreSQL table loads use a read-only repeatable-read cursor for the duration of each load. Other drivers use separate page reads, and the status warns that concurrent changes may skip or repeat rows.
- Cells are `bookie_core::Value` throughout (`Cell` is a type alias), so driver values and synthetic values use the same type. Tests cover typed round trips and bounded JSON previews.
- Accessibility is unverified. GPUI includes AccessKit, and some controls set accessible labels, but keyboard and screen-reader behavior has not been checked. Passing those checks is part of the [ADR 0058](../../docs/decisions/0058-gpui-primary-gtk-historical.md) gate.
- The full-value editor accepts values up to 1 MiB; larger values remain viewable and copyable but are not editable.
- It measures one machine with one GPU driver; it does not establish performance on other hardware.
