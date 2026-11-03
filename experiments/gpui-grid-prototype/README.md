# GPUI result-grid experiment

This isolated experiment renders the same synthetic result grid as `experiments/tauri-grid-prototype` with GPUI (Zed's GPU UI framework, `gpui` 0.2.2 from crates.io), so the two can be compared. It uses synthetic values only and cannot connect to a database. It is **not a supported frontend**, a package, or an alternative execution path. The shipped application continues to use GTK ([ADR 0002](../../docs/decisions/0002-rust-gtk4-libadwaita.md)), and the experiment is governed by [ADR 0028](../../docs/decisions/0028-isolated-frontend-experiments.md).

It has its own `Cargo.toml` with an empty `[workspace]` table, so it is not a member of the BookiE workspace, and its own `Cargo.lock`. It depends on no BookiE crate.

## Scope

The model in `src/model.rs` has no GPU dependency and is unit tested without a display. It reproduces the Tauri prototype's generator exactly (`value` is 9007199254740993 + row, `label` is `Sample N`, `state` is NULL for every third row and alternates true/false otherwise), Find with a 10,000-match cap, the filter predicates (`contains`, `=`, `!=`; `!=` keeps NULL), whole-row and keyed compare with exact counts and 200 shown differences, and the 5,000-row pager.

The window (`src/main.rs`) has:

- One wrapping strip: progress, pager (First, Previous page, range, Next page, Last), Find with Previous and Next match and a "Match N of M" status, Show matching rows, Compare…, Export all rows… (inert) and Continuous scrolling.
- A filter row (Ctrl+F): a WHERE expression such as `label contains 7`, Apply and Clear all.
- A compare bar with Whole row and Key: value.
- A virtualized grid over `uniform_list` with 36 px rows. Find, filter and compare run on the background executor and drop stale results by generation.

The text fields are deliberately minimal: they append typed characters, handle Backspace and Enter, and nothing else. There is no selection, caret movement, IME or clipboard. No SQL editor was attempted.

Row counts are 150,000, 500,000 or 1,000,000 through `BOOKIE_PROTOTYPE_ROWS`. `BOOKIE_PROTOTYPE_WIDTH` and `BOOKIE_PROTOTYPE_HEIGHT` set the window size.

## Run and test

From the repository root:

```bash
cargo test --manifest-path experiments/gpui-grid-prototype/Cargo.toml --locked
cargo deny --manifest-path experiments/gpui-grid-prototype/Cargo.toml --config deny.toml check advisories licenses
cargo run --release --manifest-path experiments/gpui-grid-prototype/Cargo.toml --locked
```

The window needs a Wayland or X11 session with a Vulkan driver that can present.

## Scroll and memory measurement

```bash
cargo build --release --locked --manifest-path experiments/gpui-grid-prototype/Cargo.toml
node experiments/gpui-grid-prototype/measure.mjs --rows=150000 --steps=120 --runs=3
```

The script starts the release binary on the current display, waits for the application to report its first frame, scrolls the first 5,000-row page from top to bottom in the requested number of steps, samples resident and proportional memory of the process every 400 ms, and prints one JSON line per run. It uses the same cadence as the Tauri script (a step, then a short wait) so total elapsed time is comparable.

`--pacing=frame` instead advances the scroll on every frame and requests the next one, so the recorded frame intervals show the real frame time. `--scope=continuous` scrolls the whole result instead of one page.
The application prints `PROTOTYPE_FIRST_FRAME`, `PROTOTYPE_SCROLL_DONE steps=… elapsed_ms=…` and, in frame mode, `frame_p50_ms`, `frame_p95_ms`, `frame_p99_ms` and `frame_max_ms`.

## Limits

- Synthetic data, no database, no typed value bridge beyond null, boolean, integer and text.
- No accessibility tree. Screen readers and the accessible-name scenarios the GTK application is tested with cannot see this window.
- No text editing beyond the minimal fields above, and no SQL editor.
- It measures one machine with one GPU driver; it does not establish performance on other hardware.
