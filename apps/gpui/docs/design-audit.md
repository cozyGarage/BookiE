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
| 6 | Window controls are text glyphs (`–`, `□`, `×`) | `chrome.rs` `window_button` | Open: use icons; platform buttons on macOS and Windows |
| 7 | `GridView` is a god object: 70 fields, with parallel vectors (`runs`, `run_edits`, `run_sources`) and eight palette fields | `main.rs:80` | Partly fixed: one `RunSlot` per result, and `PaletteState`, `SidebarState` and `Measure` groups; the struct is down from 70 to 49 fields (`PanelState`, `FontState`, `ExportState` added). Find, filter and editor state are the next groups |
| 8 | No dropdown or context menu existed | `rg "PopupMenu\|ContextMenu"` found nothing | Fixed and checked on a real display: a toolbar overflow menu (ellipsis) and a cell context menu (right click selects the row, then copies, edits, clears), both built from the command palette's own labels (`menus.rs`), so a command has one name everywhere. The table swallows right clicks on cells, so the menu opens from its `RightClickedCell` event |
| 9 | Saved connections could be opened from Home, but none created | `chrome.rs` Home Quick Start | Fixed: `app-services::connection_save` (validate, build the record, save with the keyring password and rollback, SSH secret) is shared by GPUI's New and Edit connection form and the GTK dialog's password persistence. The form sets password or Kerberos sign-in, TLS mode with CA, client certificate and key, a local socket, and one SSH hop (key file or password); a multi-hop chain, an SSH agent or the OpenSSH client that GTK saved is kept untouched on edit. Verified on screen by saving a TLS, socket and SSH connection and reading the record |
| 10 | The toolbar carries window-title controls but no query actions (run only appears inside the query tab) | `chrome.rs` | Open |
| 11 | Zoom for result views is a font setting only, not Ctrl+scroll | `font_settings.rs` | Open |
| 12 | Find filters only after Enter | `find_filter.rs` | Open (known) |
| 16 | Grid rows were 32 px while the scrolling maths assumed 26 | `grid_table.rs` used the table's default size | Fixed: `.xsmall()` (26 px) |
| 17 | Hover and selection in the grid were pale blue from the component library's theme | screenshot | Fixed in `ui_theme::map_component_colors` |
| 14 | Query history was the session's message log; the persistent history database was not read or written | `home_query_history_card` read `panel.messages` | Fixed for saved connections: `app-services::statement_history` records and lists statements with the connection identity; GPUI records every run and Home lists the latest 8 from the shared database, failures in the warning colour. Connections opened from a file have no saved identity and are not recorded. Retention pruning and the GTK recorder still use their own paths |
| 15 | Explain was missing | no command | Fixed: toolbar button, overflow item and palette command run the core `explain_statement` for the driver through the read-only session |

## Feature gap against the GTK app

GPUI has: result grid with Find, filter, compare, export and row copy; SQL editor with several carets; tabs; sidebar tree with search and regex; command palette; settings (appearance, fonts); server status views (sessions, locks, replication, variables). GTK also has, and GPUI lacks: connection manager and forms, SSH, structure editor, import, backup and restore, ER diagram, query history and saved queries, audit log view, catalog dialogs, Explain, session mode and transactions, schema compare, data dictionary export, preferences beyond appearance, approvals UI.

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

- `ConnectionCommands`: `validate`, `save`, `load_draft` and password and SSH-secret persistence exist in `connection_save`. `test` and `delete` now exist there too (GTK's delete calls the same `delete`; GPUI's Test and two-step Delete were checked on screen: SQLite answered `Connected · SQLite 3.54.0`, and a deleted record left the file). Still to move out of the GTK dialog: multi-hop chains, the SSH agent and the OpenSSH client, and the audit record, with typed errors the frontends only render.
- `HistoryCommands`: `record` and `recent` now exist in `statement_history`; the GTK recorder now calls `record_in`; search with filters, pinning and the retention setting still have to move onto it.

The GTK connection dialog and `record_history` move onto them in the same pull request, then GPUI builds its form and history list on the same calls.

## Recommended order

1. Done in this change: tokens (4), one look (5), menus (8), first part of the `GridView` split (7).
2. A connection write command in app-services shared by GTK and GPUI, then the GPUI connection form (9).
3. Move the GTK history recorder and its retention onto `statement_history` (Explain and the Home list are done).
4. Finish splitting `GridView` (sidebar, find, export, editor groups) before adding more features.
5. Visual comparison on a real display, light and dark, after each step.
