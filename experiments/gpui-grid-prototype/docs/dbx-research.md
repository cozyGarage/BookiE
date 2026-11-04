# DBX layout and result presentation: what to learn

Read from DBX 0.6.65 (MIT, <https://github.com/t8y2/dbx>, `apps/desktop/src`) on 2026-11-04, plus a screenshot of the installed application's start screen. These are observations of structure and numbers, not copied code. Where a figure comes from a constant in DBX's source the file is named. Anything not read from source is marked as inferred.

## Where things sit

| Region | DBX | This prototype |
|---|---|---|
| Top | `AppToolbar`, about 40 px: text actions on the left, icon actions (history, settings, theme) on the right | toolbar of the same height, text and icon actions |
| Left | `AppSidebar`, resizable, about 260 px: header with Import/Export, a search box with regex toggle, the connection tree | 260 px, draggable edge, search, tree |
| Centre | `ContentArea` with tab groups (`EditorGroup`, `EditorGroupTabBar`); tabs can sit at the top or on the side, groups can split, tabs can detach into their own window | one tab bar |
| Query tab | vertical split: editor on top, results below (`Splitpanes horizontal` in `components/layout/ContentArea.vue`) | no editor yet |
| Bottom | I did not find a persistent status bar in the files I read (not a full search); run time and counts appear to live in the result tabs and an "Execution summary" view | a 24 px status bar |

## The result pane

- **Size.** In a query tab the results pane defaults to 68 percent of the height (`DEFAULT_QUERY_RESULTS_PANE_SIZE = 68`), may shrink to 20 percent, and the editor keeps at least 15 percent. The shared workspace pane (`SqlEditorWorkspace.vue`) defaults to 32 percent, with 12 to 80 percent limits. The last size is remembered. The pane can be hidden with a chevron, and a tab can show only the editor or only the results.
- **Header row, about 40 px, muted background.** A pin ("keep results automatically"), a close button, then the result runs as tabs. Each execution adds a tab instead of replacing the last one. A tab can be renamed, pinned, coloured, closed to the left or right, and closed with a middle click. Appending a result keeps the user's horizontal scroll position (a comment in the source says so).
- **View switcher, small, in the same row.** Table Data, Graph (where it applies), a Redis command line (Redis only), Execution Summary, Chart. The grid is one view of the result, not the whole result.
- **Result-set navigator** for a script that returns several result sets (`ResultSetNavigator.vue`), and a messages view (`QueryMessagesView.vue`) for non-row output.
- **Grid toolbar**, below the header: two logical rows (filters and actions) that collapse into one at narrow widths, with 20 px buttons and a "compact" mode that drops labels and keeps icons.

## The grid itself

- **Canvas drawn**, not DOM, like a GPU toolkit would (`lib/dataGrid/canvasDataGridRenderer.ts`); the DOM version is the fallback. Row height 26 px.
- **Typography.** Numbers use a tabular-figures variant of the font so digits line up. NULL is italic and muted.
- **Colour roles** (`dataGridPaintTheme.ts`): striped row `#f0f0f0` (light) and `#28282b` (dark); active row `#dbeafe` and `#1b2e46`; a stronger crosshair tint for the active row and column; amber for duplicates, sky for NULL highlights, search-match and current-match colours; and tints on the row-number gutter for new (green), edited (amber) and deleted (red) rows. A zebra switch exists.
- **Frozen columns** exist (the renderer has a "frozen" test set). This is the feature we declined in ADR 0033; DBX has it because it owns the grid drawing, which GTK's `ColumnView` does not allow.
- **Active cell.** The whole cell is highlighted (the prototype wrongly wrapped only the text until this review), and editing replaces the cell with an input.

## What to take for GPUI, in order of value

1. **Results as tabs in a pane under the editor**, with each run kept, rather than a single grid. This changes the shape of the whole result area and matters more than any colour.
2. **A draggable split** between editor and results with remembered sizes and a collapse control (defaults near 60 to 68 percent for results).
3. **Tabular figures and right-aligned numbers.** Cheap and visible.
4. **Crosshair highlight** of the active row and column, and gutter tints that show staged changes (new, edited, deleted). BookiE already tracks staged edits in `change_tracker`, so the data exists.
5. **A compact toolbar mode** (icons only) when the pane is narrow, matching the strip's overflow menu the Tauri prototype has.
6. **A view switcher** (Table, Summary, Messages) as the place for output that is not rows, instead of toasts.

Not to copy: DBX's AI assistant, plugin workbench, and tab detaching. They are product decisions, not layout.

## Linking the real application features

The prototype is deliberately isolated (ADR 0028) and has its own synthetic `model`. The real counterparts already exist in headless crates, so linking is mostly replacing the data source.

| Prototype | Real counterpart | Notes |
|---|---|---|
| `model` generator, Find, filter, compare | `app-services::{result_find, result_filter, result_compare}`, `stream_result_spool` | work on a spooled streamed result with generations, caps and cancellation |
| `shell::tree_rows`, `CollapseState` | `app-services::sidebar_tree` (`TreeNode`, `CollapseState`) | same job; use the real one |
| `shell::Tabs` | `app-services::workspace`, `workspace_state::WorkspaceTabRecord`, `recent_tabs` | persistence of tabs already exists |
| `palette` | `app-services::quick_switcher` (objects) | there is no command registry yet; the palette would define one |
| `selection` copy | `bookie_core::export::render_tsv` | the Tauri prototype already uses it |
| `edit` | `bookie_core::import::parse_cell` | the Tauri prototype already uses it |
| `theme` | the GTK application's colour scheme in `apps/linux/app/src/services` | tokens only; no sharing is wired |
| column widths | `app-services::column_widths` | |

Suggested order, each step small and reversible:

1. **Amend ADR 0028** to allow the GPUI prototype to depend on `bookie-core` (the Tauri prototype already does), then replace `edit.rs` and `selection::copy_tsv` with `parse_cell` and `render_tsv`. This removes two duplicates and makes values real.
2. **Introduce a row-source trait** in the library (length, rows in a range, find, filter) implemented by the synthetic model first and by a spooled result second. The renderer would only call the trait, which is the "app renders, services decide" split you described.
3. **Depend on `bookie-app-services`** only if its dependency set passes `cargo deny` for the experiment; check `cargo tree` first, because the crate pulls in the database drivers.
4. **Read from a real SQLite file** through `bookie-driver-sqlite` for a first end-to-end result (still a throwaway experiment, still no writes).

Risks: `gpui` 0.2.2 on crates.io is not Zed's current git version, so upgrading later may need API changes; the window cannot be created under Xvfb, so automated visual checks stay limited to `test-support`; and every added crate widens the experiment's `cargo deny` surface.

## Outcome of the linking steps (2026-11-04)

1. **`bookie-core` dependency: done.** Edits go through `parse_cell` and copy through `render_tsv`. Comparing the old local copies with core before deleting them found two differences: core accepts `NaN` as a float, and core picks a NULL marker that cannot collide with cell text (`\NNN` when a cell contains `\N`). The prototype now follows core. ADR 0028 did not need amending for this; it restricts what an experiment connects to, not which BookiE crates it links.
2. **`ResultSource` trait: done** (see the README).
3. **`bookie-app-services` dependency: measured, not taken.** `bookie-core` brings 86 unique crates; `bookie-app-services` brings 426, because it links storage, policy, transport (SSH) and dump. The prototype alone is already about 680 crates with `gpui`. The pieces worth reusing (`sidebar_tree`, `result_find`, `result_filter`, `result_compare`, `workspace`) are pure logic, so the cleaner move is to split them into a light crate that both the GTK application and a GPUI renderer depend on, which matches the modularisation you described. Doing that touches the shipped workspace, so it is a proposal, not part of this experiment.
4. **Real data: not done, needs a decision.** ADR 0028 says connecting an experimental frontend to real data requires a separate architecture and product decision. ADR 0036 (Proposed) asks for a narrow one: read a disposable SQLite fixture created by the experiment itself.

## Libraries and prior art for GPUI (searched on GitHub and crates.io, 2026-11-04)

| Project | What it is | Licence | Fit |
|---|---|---|---|
| `longbridge/gpui-component` (0.9.0) | 60+ components: virtualised **Table**, a **code-editor Input** (tree-sitter highlighting, LSP hooks), **Dock**, **Resizable**, Tree, List | Apache-2.0 | the most complete option, but it targets `gpui-pre =0.3.11`, not our `gpui 0.2.2`, so adopting it means moving the whole prototype to that GPUI line |
| `zanmato/blanco` | a SQL client on `gpui-component` with per-backend `tree-sitter-sql-polyglot` grammars and a crate per driver | Apache-2.0 | the best reference for a SQL editor and a driver layout in GPUI |
| `samuelcolvin/spread` | spreadsheet viewer on `gpui = "0.2.2"` (our version): `view.rs` is about 165 KB, with xlsx, parquet and csv sources | MIT | the best reference for a grid engine on the same GPUI we use |
| `stayhydated/gpui-table`, `GuyGooL5/gpui-virtualized-table`, `dirkvdb/tabulite` | small table components | Apache-2.0, MIT, MIT | read for virtualised-table ideas; too small to adopt |
| `fluxdb-alt/fluxDB`, `007qr/only-tables`, `NonGT/sql-honcho` | other GPUI database clients | GPL-3.0, none, MIT | read for layout ideas; `only-tables` has no licence, so do not copy from it |
| `zed-industries/zed` crates (`editor`, `rope`, `sum_tree`, `multi_buffer`) | the real editor | mixed, mostly GPL-3.0 | a model of how a code editor is built; not a dependency |
| `huacnlee/gpui-workspace` | dock layout extracted from Zed | GPL-3.0 | reference for panes and docks |

Two licence notes: BookiE is AGPL-3.0-only (the same as TablePro), so Apache-2.0, MIT and GPL-3.0 code can be combined with it, but copying code still needs the attribution entry in `docs/adopted-code-attribution.md`. The component libraries (`Ely-GPUI-Components`, `bezel`, `guise`) have more stars but are general UI kits, not data-grid or editor engines.

## What our upstream TablePro shows

TablePro (`TableProApp/TablePro`, AGPL-3.0) is the project BookiE's Linux client came from. Its repository is already split the way you described:

- `core/crates` is a shared Rust core used by every platform: `core` (including `browse`: page, count and export request building), `session` (connect attempts), `drivers`, `storage`, `ssh`, `net`, `mcp`, `ai`, and `ffi`, which exposes it to the Swift app.
- Each platform is a thin renderer on top: `apple/` (Swift, macOS and iOS), `linux/crates/app` (GTK), `windows/`.
- Its Apple data grid and result views are large and specific: per-type cell editors (array, JSON, binary, date and time, enum, foreign-key picker and preview, geometry map, image), a column jump panel, column visibility and value-filter popovers, a result-comparison grid and a row inspector.
- Its SQL editor is a separate package (`TableProEditor`, about 280 files) built on tree-sitter.

The lessons: keep query and result logic in the shared core, keep per-type cell editors as their own components, and treat the SQL editor as its own subproject rather than a widget.
