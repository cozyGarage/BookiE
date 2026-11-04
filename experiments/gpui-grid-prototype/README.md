# GPUI result-grid experiment

This isolated experiment renders the same synthetic result grid as `experiments/tauri-grid-prototype` with GPUI (Zed's GPU UI framework, `gpui` 0.2.2 from crates.io), so the two can be compared. It uses synthetic values only and cannot connect to a database. It is **not a supported frontend**, a package, or an alternative execution path. The shipped application continues to use GTK ([ADR 0002](../../docs/decisions/0002-rust-gtk4-libadwaita.md)), and the experiment is governed by [ADR 0028](../../docs/decisions/0028-isolated-frontend-experiments.md).

It has its own `Cargo.toml` with an empty `[workspace]` table, so it is not a member of the BookiE workspace, and its own `Cargo.lock`. It depends on no BookiE crate.

## Scope

The model in `src/model.rs` has no GPU dependency and is unit tested without a display. It reproduces the Tauri prototype's generator exactly (`value` is 9007199254740993 + row, `label` is `Sample N`, `state` is NULL for every third row and alternates true/false otherwise), Find with a 10,000-match cap, the filter predicates (`contains`, `=`, `!=`; `!=` keeps NULL), whole-row and keyed compare with exact counts and 200 shown differences, and the 5,000-row pager.

The window (`src/main.rs`) has:

- One wrapping strip: progress, pager (First, Previous page, range, Next page, Last), Find with Previous and Next match and a "Match N of M" status, Show matching rows, Compare…, Export all rows… (inert) and Continuous scrolling.
- A filter row (Ctrl+F): a WHERE expression such as `label contains 7`, Apply and Clear all.
- A compare bar with Whole row and Key: value.
- A fourth `typed` column that cycles through every value kind (float, bytes, date, time, datetime, timestamp with time zone, decimal, UUID, JSON, raw JSON text, undecodable, NULL), searchable with Find and the filter row.
- A virtualized grid over `uniform_list` with 36 px rows. Find, filter and compare run on the background executor and drop stale results by generation.

Find, the filter row and the cell editor use `src/text_input.rs`, a single-line input ported from GPUI's own `examples/input.rs` (Apache-2.0, comments removed). It has a caret, mouse and Shift selection, Ctrl+A/C/X/V, Home/End, grapheme-aware deletion and the IME hooks (`EntityInputHandler`). It is single line only: no SQL editor, no multi-line text, no undo and no completion.

The grid has a keyboard cell cursor (`src/cursor.rs`). Click a cell or focus the grid, then the arrow keys move one cell, Page Up and Page Down a screenful, Home and End to the row ends and Ctrl+Home and Ctrl+End to the corners. Moving past the last row of a page continues on the next page. Space selects the cursor row, Shift+Up and Shift+Down extend the selection, and Enter or F2 edits the cell. The active cell has a blue outline and the status line names it.

Ctrl+Shift+P opens a command palette (`src/palette.rs` and an overlay in `src/main.rs`), as in Zed: type letters of a command, Up and Down choose, Enter runs, Escape closes. It lists page navigation, Find, Filter, Clear filter, Compare, the paged and continuous modes, Copy, Clear selection and Edit the active cell, and hides commands that do not apply. It reuses the same `TextInput` as Find, so it also checks that component's keyboard and IME handling in a second place.

Double-click a label, state or typed cell to edit it. Enter applies, Escape cancels. The text is validated by `src/edit.rs`, which follows the same rules and error words as the Tauri prototype (and so as `bookie_core::import::parse_cell`), without depending on any BookiE crate. Edits live in the view only and mark the cell with an amber underline; nothing is written anywhere and Copy still uses the original values.

Rows can be selected with a click, extended with Shift+click, and copied with Ctrl+C (Ctrl+Shift+C adds the header row) through the GPUI clipboard as tab-separated text. The quoting follows the core export rules and is covered by unit tests; copy is limited to 5,000 rows. Writing to the real system clipboard was not verified, because a headless run cannot read it back.

The look follows DBX (MIT, <https://github.com/t8y2/dbx>): its light and dark design tokens from `apps/desktop/src/styles/tokens.css` (white and near-black surfaces, `#e5e5e5` borders, 4 px radii, a muted header), its data-grid constants (26 px rows, 13 px text, striped rows `#f0f0f0`, blue active row, italic muted NULL) and outline buttons. Only colour values and sizes were taken; no DBX code.

The window also follows DBX's layout (`src/chrome.rs`, state in `src/shell.rs`), all with synthetic data: a top toolbar (New Connection, Data Transfer, Driver Manager, Plugin Center and More are inert; Commands, Sidebar and a theme switch work), a 260 px sidebar with a search box and a connection → database → table tree (click to expand; typing in the search filters the tables and expands the tree), a tab bar with a permanent Home tab and closable table tabs, a Home dashboard (statistics, Quick Start, Shortcuts, Query History), a row-number gutter and a status bar. Only `synthetic_rows` has data; the other tables open a tab that says so. Ctrl+B hides the sidebar and the command palette can open Home or the table. Light is the default. `BOOKIE_PROTOTYPE_THEME=dark` starts dark, and the command palette has **Switch to dark theme** and **Switch to light theme**. The tokens live in `src/theme.rs`. Not copied from DBX: the Geist font (the window uses the system UI font) and its canvas-drawn cells.

Row counts are 150,000, 500,000 or 1,000,000 through `BOOKIE_PROTOTYPE_ROWS`. Real keyboard, IME and mouse behavior is covered by `src/ui_tests.rs`, which drives the views through GPUI's `test-support` context (`cargo test`). A window cannot be opened under Xvfb here (no DRI3 for the Vulkan surface), so how it looks and feels has not been checked by eye.

`BOOKIE_PROTOTYPE_WIDTH` and `BOOKIE_PROTOTYPE_HEIGHT` set the window size.

## Run and test

From the repository root:

```bash
cargo test --manifest-path experiments/gpui-grid-prototype/Cargo.toml --locked
cargo deny --manifest-path experiments/gpui-grid-prototype/Cargo.toml --config deny.toml check advisories licenses
cargo run --release --manifest-path experiments/gpui-grid-prototype/Cargo.toml --locked
```

The window needs a Wayland or X11 session with a Vulkan driver that can present.

## Scroll and memory measurement

To measure both prototypes the same way, run `experiments/profile.sh` from a desktop session (`ROWS=500000 RUNS=5 experiments/profile.sh` changes the size). It builds both in release mode and prints one JSON line per run: scroll time, and resident and proportional memory (for Tauri, the app plus the WebKit processes). Compare `elapsed_ms` and the `pss_kib` peaks, and the GPUI frame-paced `frame_p95_ms`. Close other heavy applications first and repeat a run; one machine and one GPU driver is all this shows.


```bash
cargo build --release --locked --manifest-path experiments/gpui-grid-prototype/Cargo.toml
node experiments/gpui-grid-prototype/measure.mjs --rows=150000 --steps=120 --runs=3
```

The script starts the release binary on the current display, waits for the application to report its first frame, scrolls the first 5,000-row page from top to bottom in the requested number of steps, samples resident and proportional memory of the process every 400 ms, and prints one JSON line per run. It uses the same cadence as the Tauri script (a step, then a short wait) so total elapsed time is comparable.

`--pacing=frame` instead advances the scroll on every frame and requests the next one, so the recorded frame intervals show the real frame time. `--scope=continuous` scrolls the whole result instead of one page.
The application prints `PROTOTYPE_FIRST_FRAME`, `PROTOTYPE_SCROLL_DONE steps=… elapsed_ms=…` and, in frame mode, `frame_p50_ms`, `frame_p95_ms`, `frame_p99_ms` and `frame_max_ms`.

## Limits

- Synthetic data, no database. The typed column carries the same kinds and exact text as the Tauri prototype, but as canonical text held by a local cell type, not by `bookie-core` values, because this experiment depends on no BookiE crate. It shows how a typed cell renders, not that the shared value type round-trips.
- No accessibility tree. Screen readers and the accessible-name scenarios the GTK application is tested with cannot see this window.
- No text editing beyond the minimal fields above, and no SQL editor. A multi-line editor is a much larger piece (Zed's editor is its own crate) and is left until the single-line input has had real keyboard and IME feedback.
- It measures one machine with one GPU driver; it does not establish performance on other hardware.
