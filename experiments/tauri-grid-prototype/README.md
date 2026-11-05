# Tauri result-grid experiment

This isolated experiment compares a welcome layout and a virtualized result grid with the GTK application. It uses synthetic values only and cannot connect to a database. It is not a supported frontend, a package, or an alternative execution path. The shipped application continues to use GTK. The experiment does not establish accessibility, input-method, security, cross-platform or performance parity.

The Rust commands use bounded requests and the shared `bookie-core` value type for a small synthetic fixture. Their JSON envelope carries every `bookie_core::Value` variant as a kind and an exact text: integers and decimals as decimal strings, bytes as `\x` hex, dates and times as ISO text, raw JSON text with its original spacing, and an undecodable cell as `<undecodable TYPE>`. The text rules mirror the export text of `bookie-core`, so a value reads the same here as in an export. This is still not a complete value bridge: nothing is parsed back, edited or compared by type, and no policy or database operations cross it.

## Scope

The result screen mirrors the GTK streamed-result header so both layouts can be compared side by side:

- One wrapping strip: progress, pager (First, Previous page, range, Next page, Last), Find with Previous and Next match, Show matching rows, Compare…, Export all rows…, and Continuous scrolling. Below 600 px the last three move into a **More actions** menu. The names match the GTK accessible names.
- A filter row (Ctrl+F): WHERE expression (`label contains 7`, `state = false`, `value != 9007199254740993`), Saved filters, Clear all, Apply and close.
- Find over all synthetic rows with a 10,000-match cap, "Match N of M" status, wrap-around navigation and stale-reply dropping by generation.
- Show matching rows builds a synthetic filtered view held in Rust state, paged through `filtered_rows`.
- A compare panel over two synthetic generators: whole-row multiset (Added and Removed) and keyed (Before and After pairs), capped at 200 shown rows.
- A fourth **Typed** column that cycles through every value variant (float, bytes, date, time, datetime, timestamp with time zone, decimal, UUID, JSON, raw JSON text, undecodable, NULL) so the grid, Find and the filter row are exercised against exact typed text. Each kind has its own cell class; numbers align right, binary and JSON use a monospace face.
- Row selection: click selects one row, Shift-click extends a contiguous range from the anchor, Escape clears. Ctrl+C copies the selected rows (up to 5,000) as tab-separated text. The text is produced by `bookie_core::export::render_tsv`, the same renderer the product uses, so NULL is `\N`, values keep their exact digits and typed cells use the export text. A selection is a range of source row numbers, so while a filter is applied Shift-click is disabled and only single rows can be selected. Only contiguous ranges are supported; there is no Ctrl-click multi-select and no cell-level selection.
- A keyboard cell cursor (`frontend/cursor.mjs`): click a cell or focus the grid, then the arrow keys move one cell, Page Up and Page Down move a screenful, Home and End go to the row ends, Ctrl+Home and Ctrl+End to the grid corners. Moving past the last row of a page continues on the next page. Space selects the cursor row, Shift+Up and Shift+Down extend the selection, and Enter or F2 edits the cell. The active cell has an outline and `aria-current`.
- A command palette (`frontend/palette.mjs`), opened with Ctrl+Shift+P as in Zed and VS Code: type letters of a command, Up and Down choose, Enter runs, Escape closes. It lists page navigation, Find, Filter, Clear filter, Compare, the paged and continuous modes, Copy, Clear selection and Edit the active cell, and hides what does not apply right now (for example Clear filter with no filter). It is a `<dialog>` with a combobox and listbox, so focus is trapped while it is open and returns to the grid on close.
- Row counts of 150,000, 500,000 or 1,000,000 through `?rows=` or `BOOKIE_PROTOTYPE_ROWS`.

Double-click a label, state or typed cell to edit it in place. Enter applies, Escape cancels and Ctrl+Backspace sets NULL. The text is parsed by `bookie_core::import::parse_cell` for the cell's own kind, so an invalid date, UUID, number, boolean, JSON or hex value is refused with a reason that never repeats the text. Edits live in the web view only: nothing is written anywhere, paging or filtering keeps them, and Copy still uses the original values (the status line says so). The undecodable cell is read-only.

`BOOKIE_PROTOTYPE_SELFTEST=1` opens the result, selects rows 2 to 4, runs the copy path and prints a `PROTOTYPE_REPORT` line with the copy outcome, then drives the editor through the DOM (an invalid date is refused, a valid one is applied and marked) and prints a second line. The self-test has no user gesture, so WebKit refuses both clipboard routes (`navigator.clipboard.writeText` and the `execCommand("copy")` fallback) and the report says so; a real Ctrl+C with a gesture has not been verified in this environment.

The self-test also drives the cursor and the palette through the DOM: three keys put the cursor on row 3 of the Label column, Space selects the row, and Ctrl+Shift+P with `last page` moves to page 200 of 200 and closes the dialog.

Export is inert. Nothing is read from or written to disk.

## Run and test

From the repository root:

```bash
node --test experiments/tauri-grid-prototype/frontend/*.test.mjs
cargo test --manifest-path experiments/tauri-grid-prototype/src-tauri/Cargo.toml --locked
cargo deny --manifest-path experiments/tauri-grid-prototype/src-tauri/Cargo.toml --config deny.toml check advisories licenses
cargo run --manifest-path experiments/tauri-grid-prototype/src-tauri/Cargo.toml --locked
```

Select **Open sample result**. `BOOKIE_PROTOTYPE_ROWS`, `BOOKIE_PROTOTYPE_WIDTH` and `BOOKIE_PROTOTYPE_HEIGHT` choose the synthetic size and window size for a run. Only a small window of rows is requested at a time; the prototype caps requests, matches and the scroll surface.

## Scroll and memory measurement

```bash
cargo build --release --locked --manifest-path experiments/tauri-grid-prototype/src-tauri/Cargo.toml
node experiments/tauri-grid-prototype/measure.mjs --rows=150000 --steps=120 --runs=3
```

The script starts the release build under `xvfb-run` when no display is set, scrolls from the top to the bottom in the requested number of steps, samples resident and proportional memory of the application and the WebKit web process every 400 ms, and prints one JSON line per run. Redirect the output outside the repository. The figures come from a synthetic Rust generator and a web view. They are not comparable with BookiE's database-driven GTK grid, which decodes, owns and spools real typed rows. Compare layout and behavior, not these numbers.

## Accessibility checklist (manual)

No screen-reader result is claimed. To evaluate, run the app in a desktop session and record the outcome of each step in a private verification record:

1. With Orca running, move through the strip with Tab. Confirm each control announces its name (First page, Previous page, Next page, Last page, Find in all rows, Previous match, Next match, Show matching rows, Compare…, Export all rows…, Continuous scrolling, More actions).
2. Confirm Continuous scrolling announces its pressed state.
3. Run Find and confirm the "Match N of M" status is announced without moving focus.
4. In the grid, confirm the `grid` role, row count and row indexes are exposed (an AT-SPI tree dump or Accerciser).
5. Confirm Page Up, Page Down, Home and End move through rows and that focus is visible.
6. Resize the window to about 920 px and 600 px and confirm nothing overlaps or clips, and the overflow menu is keyboard reachable.

Before considering a second frontend, evaluate real application integration, exact values across every variant, authorization, audit, cancellation, clipboard, keyboard navigation, screen readers, input methods, themes, scaling, operating-system packaging and installed performance. Changes to the product's Linux-only UI decision require a separate accepted decision.
