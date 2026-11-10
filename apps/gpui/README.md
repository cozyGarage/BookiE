# BookiE GPUI app

The GPUI frontend for BookiE, built with `gpui-pre` 0.3.11. By default it shows a synthetic result grid, the same one as `experiments/tauri-grid-prototype`, so the two can be compared. With `BOOKIE_PROTOTYPE_FILE` (a SQLite or DuckDB file) or `BOOKIE_PROTOTYPE_CONNECTION` (a saved connection name or id) plus `BOOKIE_PROTOTYPE_TABLE` it reads a real table, read-only, through `bookie-app-services` ([ADR 0043](../../docs/decisions/0043-gpui-frontend-may-read-real-data-through-app-services.md)). It is **not shipped**: no package or release includes it, and the GTK application stays the product ([ADR 0002](../../docs/decisions/0002-rust-gtk4-libadwaita.md)). Its location is [ADR 0044](../../docs/decisions/0044-gpui-app-lives-under-apps.md).

It has its own `Cargo.toml` with an empty `[workspace]` table, so it is not a member of the BookiE workspace, and its own `Cargo.lock`. It depends on `bookie-core` for typed values and `bookie-results` for bounded scanning and text search. Equality filters keep the prototype's case-insensitive displayed-text behavior in the experiment adapter; they do not weaken typed equality in shared result consumers.

## Scope

The model in `src/model.rs` has no GPU dependency and is unit tested without a display. It reproduces the Tauri prototype's generator exactly (`value` is 9007199254740993 + row, `label` is `Sample N`, `state` is NULL for every third row and alternates true/false otherwise), Find with a 10,000-match cap, the filter predicates (`contains`, `=`, `!=`; `!=` keeps NULL), whole-row and keyed compare with exact counts and 200 shown differences, and the 5,000-row pager.

The window (`src/main.rs`) has:

- One wrapping strip: progress, pager (First, Previous page, range, Next page, Last), Find with Previous and Next match and a "Match N of M" status, Show matching rows, Compare…, Export all rows… (inert) and Continuous scrolling.
- A filter row (Ctrl+F): a WHERE expression such as `label contains 7`, Apply and Clear all.
- A compare bar with Whole row and Key: value.
- A fourth `typed` column that cycles through every value kind (float, bytes, date, time, datetime, timestamp with time zone, decimal, UUID, JSON, raw JSON text, undecodable, NULL), searchable with Find and the filter row.
- A virtualized grid over `uniform_list` with 36 px rows. Find, filter and compare run on the background executor; replaced work is cancelled and stale replies are discarded by generation.

Find, the filter row and the cell editor use `src/text_input.rs`, a single-line input ported from GPUI's own `examples/input.rs` (Apache-2.0, comments removed). It has a caret, mouse and Shift selection, Ctrl+A/C/X/V, Home/End, grapheme-aware deletion and the IME hooks (`EntityInputHandler`). These fields are single line. The Query tab uses a separate multiline SQL editor.

The grid has a keyboard cell cursor (`src/cursor.rs`). Click a cell or focus the grid, then the arrow keys move one cell, Page Up and Page Down a screenful, Home and End to the row ends and Ctrl+Home and Ctrl+End to the corners. Moving past the last row of a page continues on the next page. Space selects the cursor row, Shift+Up and Shift+Down extend the selection, and Enter or F2 edits the cell. The active row and column are tinted, the active cell has a blue outline, and the status line names it. Numeric values and row numbers align to the right, following the DBX grid's numeric layout.

Ctrl+Shift+P opens a command palette (`src/palette.rs` and an overlay in `src/main.rs`), as in Zed: type letters of a command, Up and Down choose, Enter runs, Escape closes. It lists page navigation, Find, Filter, Clear filter, Compare, the paged and continuous modes, Copy, Clear selection and Edit the active cell, and hides commands that do not apply. It reuses the same `TextInput` as Find, so it also checks that component's keyboard and IME handling in a second place.

Double-click a label, state or typed cell to edit it. Enter applies, Escape cancels. The text is validated by `src/edit.rs`, which follows the same rules and error words as the Tauri prototype (and so as `bookie_core::import::parse_cell`), without depending on any BookiE crate. Opening a value containing a newline is refused with a single-line editor notice, preserving its full value. Edits live in the view only and mark the cell with an amber underline; nothing is written anywhere. Copy includes the full pending values.

Connection and table changes are refused while view-only edits are pending. Closing a result or query tab with pending edits is also refused, and an open cell editor cannot be replaced by switching grids or results. **Commands → Discard all view-only edits** discards all such edits and allows the change.

Rows can be selected with a click, extended with Shift+click, and copied with Ctrl+C (Ctrl+Shift+C adds the header row) through the GPUI clipboard as tab-separated text. The quoting follows the core export rules and is covered by unit tests; copy is limited to 5,000 rows. Writing to the real system clipboard was not verified, because a headless run cannot read it back.

Copy as INSERT is available for table rows. Query results do not infer an INSERT target from the previously browsed table.

The look follows DBX (MIT, <https://github.com/t8y2/dbx>): its light and dark design tokens from `apps/desktop/src/styles/tokens.css` (white and near-black surfaces, `#e5e5e5` borders, 4 px radii, a muted header), its data-grid constants (26 px rows, 13 px text, striped rows `#f0f0f0`, blue active row, italic muted NULL) and outline buttons. Only colour values and sizes were taken; no DBX code.

The window also follows DBX's layout (`src/chrome.rs`, state in `src/shell.rs`), with synthetic data by default: a top toolbar (New Connection, Data Transfer, Driver Manager, Plugin Center and More are inert; Commands, Sidebar and a theme switch work), a 260 px sidebar with a search box and a connection → database → table tree (click to expand; typing in the search filters the tables and expands the tree), a horizontally scrolling tab bar with a permanent Home tab and closable table tabs, a Home dashboard (statistics, Quick Start, Shortcuts, Query History), a row-number gutter and a status bar. The selected tab is brought into view. Only `synthetic_rows` has data; the other tables open a tab that says so. Ctrl+B hides the sidebar and the command palette can open Home or the table. Light is the default. `BOOKIE_PROTOTYPE_THEME=dark` starts dark, and the command palette has **Switch to dark theme** and **Switch to light theme**. The tokens live in `src/theme.rs`. Not copied from DBX: the Geist font (the window uses the system UI font) and its canvas-drawn cells.

Icons are Lucide, the set DBX uses (ISC licence, `assets/icons/LICENSE`): 20 SVGs embedded in the binary through GPUI's asset source (`src/assets.rs`) and drawn in the theme's muted colour. Our GTK application's symbolic icons are the other option; they are not wired here.

The UI font defaults to the first installed of `Fira Sans` and `Fira Code`, and otherwise to the system font. Only Fira Code (monospace) is installed on the machine this was written on, so the window shows it; `sudo pacman -S ttf-fira-sans` gives the proportional one. `BOOKIE_PROTOTYPE_FONT="Some Family"` picks another at start, and the palette's **Choose font…** lists every installed family (type to filter) and applies it at once. The choice is not saved, because this experiment writes nothing to disk.

There are four themes: DBX light and dark, and **Adwaita** light and dark (libadwaita's neutral surfaces, the GNOME blue `#3584e4` accent and 6 px corners, to look like our GTK application). `BOOKIE_PROTOTYPE_THEME` takes `light`, `dark`, `adwaita-light` or `adwaita-dark`. The palette has **Theme: DBX style**, **Theme: Adwaita style** and the light/dark switch, which keeps the chosen style.

Visible cell text is limited to 512 Unicode characters before layout. Line breaks, tabs and NULs use visible markers. Byte cells show at most 16 bytes in hexadecimal plus their exact byte count. These previews apply to normal and comparison rows; editing and copying still use full values. Painting requests each visible row once and shares it across its four cells. The synthetic source still constructs an owned row for each request, so this does not bound source allocation or establish real-result performance.

## Renderer boundary

The formatter tests expand the [shared large-cell inputs](../../experiments/fixtures/README.md), also used by the Tauri frontend tests. They check bounded previews and exact typed-value round trips. They do not measure native rendering or process memory.

The planned modularisation puts application logic in app-services and leaves the application as a renderer. This prototype already splits that way, so it shows what a renderer would need. The library half (`src/lib.rs`) has no `gpui` import and is tested without a display:

| Module | Holds | Would become |
|---|---|---|
| `model` | rows, Find, filter, compare, pager | a result service |
| `selection` | row selection and tab-separated copy | result clipboard |
| `edit` | typed cell validation | value editing |
| `cursor` | active-cell movement rules | grid view-model |
| `palette` | command matching and ordering | command registry |
| `shell` | sidebar tree and tab model | workspace view-model |
| `theme` | colour and size tokens | design tokens (CSS variables for a web renderer too) |

The binary half is the frontend: `main.rs` owns `GridView` state and actions, `render.rs` renders that state, and `chrome.rs`, `grid_edit.rs`, `text_input.rs`, `ui_theme.rs` and `assets.rs` provide the surrounding UI. `GridView` still owns both state and layout decisions; the next step is to move its state fields behind the library types so the renderer reads them and sends intents (a key press, a click) back. The Tauri prototype follows the same split with `frontend/*.mjs` logic and `main.mjs` rendering.

The result area follows DBX's query-tab structure (`src/chrome.rs`, model in `src/shell.rs`): **Ctrl+T** opens a Query tab with a multiline SQL editor on top, a draggable divider (the result keeps 20 to 80 percent of the height, 68 percent by default) and a result pane below. The pane header has closable run tabs (**Ctrl+Enter** adds a run with its own row count: 150,000, then 1,000,000, 5,000, 40, and so on), a **Table Data / Summary** switcher and a chevron that hides the pane. Selecting a run swaps its result in and clears the cursor and selection. Applied edits remain with their run until it is closed, separately from synthetic table edits; an open editor is dismissed when switching. The command palette offers **Close current result**, **Previous result** and **Next result** as keyboard paths; the run strip can scroll horizontally. Enter inserts a line in the SQL editor; Ctrl+Enter runs the statement. Live sessions execute through app-services and remain read-only.

The view reads rows, Find, filter and compare from a `ResultSource` trait (`src/source.rs`); implementations include the synthetic generator and an experimental live table loaded through `bookie-app-services::ReadSession`. A test drives the whole view from a 3-row source that is not the generator. `BOOKIE_PROTOTYPE_START=query` opens the Query tab at start.

Row counts are 150,000, 500,000 or 1,000,000 through `BOOKIE_PROTOTYPE_ROWS`. Real keyboard, IME and mouse behavior is covered by `src/ui_tests.rs`, which drives the views through GPUI's `test-support` context (`cargo test`). A window cannot be opened under Xvfb here (no DRI3 for the Vulkan surface), so how it looks and feels has not been checked by eye.

`BOOKIE_PROTOTYPE_WIDTH` and `BOOKIE_PROTOTYPE_HEIGHT` set the window size.

## Run and test

From the repository root:

```bash
cargo test --manifest-path apps/gpui/Cargo.toml --locked
cargo deny --manifest-path apps/gpui/Cargo.toml --config deny.toml check advisories licenses
cargo run --release --manifest-path apps/gpui/Cargo.toml --locked
```

Forgejo runs the model and simulated interaction tests without a display. Native rendering and system clipboard behavior require a desktop session.

The window needs a Wayland or X11 session with a Vulkan driver that can present.

## Scroll and memory measurement

To measure both prototypes the same way, run `experiments/profile.sh` from a desktop session (`ROWS=500000 RUNS=5 experiments/profile.sh` changes the size). It builds both in release mode and prints one JSON line per run: scroll time, and resident and proportional memory (for Tauri, the app plus the WebKit processes). Compare `elapsed_ms` and the `pss_kib` peaks, and the GPUI frame-paced `frame_p95_ms`. Close other heavy applications first and repeat a run; one machine and one GPU driver is all this shows.


```bash
cargo build --release --locked --manifest-path apps/gpui/Cargo.toml
node apps/gpui/measure.mjs --rows=150000 --steps=120 --runs=3
```

The script starts the release binary on the current display, waits for the application to report its first frame, scrolls the first 5,000-row page from top to bottom in the requested number of steps, samples resident and proportional memory of the process every 400 ms, and prints one JSON line per run. It uses the same cadence as the Tauri script (a step, then a short wait) so total elapsed time is comparable.

`--pacing=frame` instead advances the scroll on every frame and requests the next one, so the recorded frame intervals show the real frame time. `--scope=continuous` scrolls the whole result instead of one page.
The application prints `PROTOTYPE_FIRST_FRAME`, `PROTOTYPE_SCROLL_DONE steps=… elapsed_ms=…` and, in frame mode, `frame_p50_ms`, `frame_p95_ms`, `frame_p99_ms` and `frame_max_ms`.

## Limits

- The default source is synthetic. A saved connection can also be opened from the Home page. An empty catalog opens a query tab on the live connection. Set `BOOKIE_PROTOTYPE_TABLE` and either `BOOKIE_PROTOTYPE_FILE` for SQLite or DuckDB or `BOOKIE_PROTOTYPE_CONNECTION` for a saved connection to load a live table through `ReadSession`. Live data is read-only, capped at 100,000 rows and subject to the shared decoded-byte and cell budgets. The status marks results that may be incomplete. Pages are separate reads, not a consistent snapshot; concurrent changes can affect the rows shown.
- Driver-shaped temporal, decimal, UUID and JSON values retain their original `bookie_core::Value` through the cell bridge. Synthetic edits still use the prototype's local cell representation. Tests cover typed round trips and bounded JSON previews.
- No accessibility tree. Screen readers and the accessible-name scenarios the GTK application is tested with cannot see this window.
- Cell editing uses a single-line field; values containing newlines cannot be edited there.
- It measures one machine with one GPU driver; it does not establish performance on other hardware.
