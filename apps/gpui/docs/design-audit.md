# GPUI design audit

Date: 2026-11-18. Against [DESIGN.md](../../../DESIGN.md), [docs/ui-review-checklist.md](../../../docs/ui-review-checklist.md) and [ADR 0054](../../../docs/decisions/0054-one-visual-language.md), at `main` plus this change.

## Method

Read `theme.rs`, `chrome.rs`, `render.rs`, `main.rs`, `text_input.rs` and `palette_actions.rs`, counted patterns with grep, and ran `cargo test` for the app (244 tests, all pass). Visual checks were done on a real display (light, dark, the overflow menu, the cell context menu, tooltips). A side-by-side comparison with the GTK app is still to do.

## What matches

- Colour tokens follow dbx (white and near-black surfaces, `#e5e5e5` border, 4 px control radius) in `Theme::dbx_light` and `dbx_dark`.
- No hex colours in widget code except two in `text_input.rs`.
- Functions are short: 539 functions, one over 60 lines (`sidebar_tree_row`, 61). There are no comment lines.
- Logic is outside the renderer: `model`, `shell`, `palette`, `selection` and `edit` are in the library and tested without a display (129 tests); 115 more drive the views through GPUI's test platform.
- Every command is reachable by keyboard through the command palette (Ctrl+Shift+P).

## Findings

| # | Finding | Evidence | Status |
| --- | --- | --- | --- |
| 1 | Icon-only buttons had no tooltip or name, and the run button revealed its label only on hover | no `tooltip` anywhere in `src/`; `ghost_icon` used `group_hover` to show text | Fixed: every toolbar, sidebar, window and run button has a tooltip with its name and shortcut |
| 2 | Sizes were literal pixels, against gpui-component's guide and DESIGN.md | 80 `px(..)` literals (chrome 50, text_input 17, render 6, main 6) | Fixed: `theme::size` holds the scale and control tiers (24, 28, 32); the ratchet `scripts/check-ui-patterns.py` now counts GPUI literals (baseline 0 for these files) |
| 3 | Icon size 15 px, DESIGN.md says 14 | `assets.rs` | Fixed: `size::ICON` |
| 4 | Theme lacked tokens the design needs: destructive, success, focus ring, popover, sidebar | `Theme` had 14 fields; none of these | Fixed: all five added with the DESIGN.md values, sidebar and bars use their own token |
| 5 | A second visual style (`Style::Adwaita`, 6 px corners, GNOME blue) contradicted ADR 0054's single look | `theme.rs`, palette commands | Fixed: removed; two themes, light and dark |
| 13 | The pager strip was bordered text buttons (Compare, Export…, Matches only, Continuous) and the UI font fell back to monospace | `render.rs`, `default_font` | Fixed: icon buttons with tooltips; the UI font prefers Inter, Segoe UI, Adwaita Sans, Roboto or Noto Sans |
| 6 | Window controls are text glyphs (`–`, `□`, `×`) | `chrome.rs` `window_button` | Fixed: minus, square and x icons from the Lucide set at the standard icon size. Platform buttons on macOS and Windows are still open |
| 7 | `GridView` is a god object: 70 fields, with parallel vectors (`runs`, `run_edits`, `run_sources`) and eight palette fields | `main.rs:80` | Fixed: one `RunSlot` per result, and `PaletteState`, `SidebarState` and `Measure` groups; the struct is down from 70 to 42 fields (`PanelState`, `FontState`, `ExportState`, `SearchState` for find, filter and the match cursor, `CellEditState` for the cell editor and pending edits, and `RunTabs` for the run slots, the active run, its scroll handle and the next run number). The pager is already a single field, so the split of loose state is done |
| 8 | No dropdown or context menu existed | `rg "PopupMenu\|ContextMenu"` found nothing | Fixed and checked on a real display: a toolbar overflow menu (ellipsis) and a cell context menu (right click selects the row, then copies, edits, clears), both built from the command palette's own labels (`menus.rs`), so a command has one name everywhere. The table swallows right clicks on cells, so the menu opens from its `RightClickedCell` event |
| 9 | Saved connections could be opened from Home, but none created | `chrome.rs` Home Quick Start | Fixed: `app-services::connection_save` (validate, build the record, save with the keyring password and rollback, SSH secret) is shared by GPUI's New and Edit connection form and the GTK dialog's password persistence. The form sets password or Kerberos sign-in, TLS mode with CA, client certificate and key, a local socket, and an SSH tunnel with any number of jump hosts, each signing in by password, key file or agent, and the OpenSSH client choice. Verified on screen by saving an agent head with one agent jump host and reading the nested `jump` record |
| 10 | The toolbar carries window-title controls but no query actions (run only appears inside the query tab) | `chrome.rs` | Open |
| 11 | Zoom for result views is a font setting only, not Ctrl+scroll | `font_settings.rs` | Partly fixed: in the query tab Ctrl+plus, Ctrl+minus, Ctrl+0 and Ctrl+scroll over the editor change and save the editor font size (6 to 100 points, 14 by default). The editor ignored the Editor font size setting until now, because gpui-component reads its own mono font size; it is set from the setting at startup and on every change. The result grid does not zoom: its 26 px row tier belongs to the component, so a zoomed grid needs its own row height |
| 12 | Find filters only after Enter | `find_filter.rs` | Open (known) |
| 16 | Grid rows were 32 px while the scrolling maths assumed 26 | `grid_table.rs` used the table's default size | Fixed: `.xsmall()` (26 px) |
| 17 | Hover and selection in the grid were pale blue from the component library's theme | screenshot | Fixed in `ui_theme::map_component_colors` |
| 14 | Query history was the session's message log; the persistent history database was not read or written | `home_query_history_card` read `panel.messages` | Fixed for saved connections: `app-services::statement_history` records and lists statements with the connection identity; GPUI records every run and Home lists the latest 8 from the shared database, failures in the warning colour. A database opened from a file gets a stable identity from its path (a version 5 UUID of its file URL, named by the path), so its statements are recorded and can be filtered. The GTK recorder and the retention prune (GPUI prunes once per session) now call the same service |
| 15 | Explain was missing | no command | Fixed: toolbar button, overflow item and palette command run the core `explain_statement` for the driver through the read-only session |
| 18 | GPUI had no history page: only the latest eight statements on Home, no search, no pinning, no way to reuse one | `history_view.rs` read `recent(8)` | Fixed: a History tab (overflow menu or palette) with text search, All, Succeeded, Failed and Cancelled filters, a time window (any time, 24 hours, 7 days, 30 days), pinned statements first, pin and delete buttons and a click that puts the statement in the query editor without running it. It uses `statement_history::{search, set_pinned, delete}`, which also hold the full-text query quoting that the GTK dialog had; GTK now calls the same `fts5_query`. Checked on a private display: search, filter, pin and reuse against a live SQLite connection |
| 19 | GPUI had no audit log view, though its sessions write to the same audit journal as GTK | `audit_view` (app-services) was used only by the GTK dialog | Fixed: an Audit log tab (overflow menu or palette) lists the latest 1,000 operations with one row each (outcome mark, redacted statement, time, kind, rows, duration, who, approval, connection, and the error when there is one) and a filter over operation id, connection, principal, kind, statement and outcome. A denied, failed, cancelled or row-skipping operation uses the warning colour and a cross, the rest a circle. `audit_view::recent_operations` reads the journal for both frontends. Checked on a private display against real journal entries, including a denied one |
| 20 | GPUI had no data dictionary, though the GTK app lists every table, column and index and exports it | `data_dictionary` (app-services) was used only by the GTK diagram dialog | Fixed: Show the data dictionary (overflow menu or palette) reads the connection's tables, columns and indexes into a result tab, new `ReadDataDictionary` command and `DataDictionaryRead` event, `ReadSession::read_data_dictionary`. The result takes the existing export, copy and find. Checked on a private display against a live SQLite file; a Postgres schema read is covered by the service's own integration test |

## Feature gap against the GTK app

GPUI has: result grid with Find, filter, compare, export and row copy; SQL editor with several carets; tabs; sidebar tree with search and regex; command palette; settings (appearance, fonts); server status views (sessions, locks, replication, variables). GTK also has, and GPUI lacks: connection manager and forms, SSH, structure editor, import, backup and restore, ER diagram, saved queries, catalog dialogs, Explain, session mode and transactions, schema compare, preferences beyond appearance, approvals UI.

## Side by side with the GTK app

Same SQLite table, light, 2026-11-18, GTK on a private display and GPUI on a real one.

| Area | GTK | GPUI | Action |
| --- | --- | --- | --- |
| Toolbar language | Icons for Pin, Explain, Split; text kept for Compare…, Session, Run | Icons in the pager strip; Run is the one filled, labelled button | GTK: give Compare… and Session icons. GPUI: matches |
| Grid row height | compact rows | was 32 px (the table's default tier) against a 26 px constant used for scrolling maths | Fixed: the table uses its 26 px tier (finding 16) |
| Selection colour | neutral grey fill | was blue from gpui-component's own theme | Fixed (finding 17): the component theme's table, list, selection, ring and primary colours are set from our tokens when a theme is installed |
| Sidebar | grouped Tables and Catalog, fields expand in place, 265 px | connection, schema and tables tree, 340 px; a click shows the fields (new `ReadColumns` command), a double click opens | Fixed: fields on a click, and a Catalog group (new `ReadCatalog` command) with the driver's kinds, checked against Postgres 17 |
| Result tabs | Results and Errors as icons under the editor | result header with run tabs | Open: decide whether both use the same strip |
| UI font | Adwaita Sans | Inter or Adwaita Sans, proportional | Matches |
| Menus | popover menus with grouped items | grouped overflow and cell menus built from palette labels | Matches |

## Shared services the GPUI form and history need

Both findings 9 and 14 need behaviour that today lives in GTK code or is written straight to storage by the GTK app. A frontend must not repeat it (ADR 0041), so the next step is two app-services commands used by both apps:

- `ConnectionCommands`: `validate`, `save`, `load_draft` and password and SSH-secret persistence exist in `connection_save`. `test` and `delete` now exist there too (GTK's delete calls the same `delete`; GPUI's Test and two-step Delete were checked on screen: SQLite answered `Connected · SQLite 3.54.0`, and a deleted record left the file). Multi-hop chains, the SSH agent and the OpenSSH client are in the service (GTK's secret persistence calls `persist_connection`), and `test` takes the transport audit factory. The GTK dialog records no separate audit entry for a connection change (only the SSH transport audit, which `test` now takes), so nothing is left to move. Test with a two-hop agent chain was checked on screen and reached the SSH connect step.
- `HistoryCommands`: `record` and `recent` now exist in `statement_history`; the GTK recorder now calls `record_in`; `prune` applies the retention setting; `search`, `set_pinned` and `delete` serve GPUI's History page. GTK's history dialog builds its filter through `statement_history::search_filter` (status, time window, connection, source and the quoted text), so both frontends mean the same thing by a filter.

The GTK connection dialog and `record_history` move onto them in the same pull request, then GPUI builds its form and history list on the same calls.

## Recommended order

1. Done in this change: tokens (4), one look (5), menus (8), first part of the `GridView` split (7).
2. A connection write command in app-services shared by GTK and GPUI, then the GPUI connection form (9).
3. Done: history filters, search, pinning, Explain, the Home list, the recorder and retention are all on `statement_history`.
4. Done: `GridView` loose state is grouped; add features on the named groups.
5. Visual comparison on a real display, light and dark, after each step.
