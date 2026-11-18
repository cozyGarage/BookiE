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
| 9 | Saved connections could be opened from Home, but none created | `chrome.rs` Home Quick Start | Partly fixed: `app-services::connection_save` (validate, build the record, save with the keyring password and rollback) is shared by GPUI's New connection form and the GTK dialog's password persistence. The form covers password connections; SSH, certificates, Kerberos, socket paths and editing an existing connection are still GTK-only |
| 10 | The toolbar carries window-title controls but no query actions (run only appears inside the query tab) | `chrome.rs` | Open |
| 11 | Zoom for result views is a font setting only, not Ctrl+scroll | `font_settings.rs` | Open |
| 12 | Find filters only after Enter | `find_filter.rs` | Open (known) |
| 14 | Query history is the current session's message log; the persistent history database the GTK app writes is not read or written | `home_query_history_card` reads `panel.messages`; `LiveSession` has no connection id, which `HistoryStore::record` requires | Open, blocked with finding 9: needs a service that records and searches with the connection identity and the retention setting |
| 15 | Explain was missing | no command | Fixed: toolbar button, overflow item and palette command run the core `explain_statement` for the driver through the read-only session |

## Feature gap against the GTK app

GPUI has: result grid with Find, filter, compare, export and row copy; SQL editor with several carets; tabs; sidebar tree with search and regex; command palette; settings (appearance, fonts); server status views (sessions, locks, replication, variables). GTK also has, and GPUI lacks: connection manager and forms, SSH, structure editor, import, backup and restore, ER diagram, query history and saved queries, audit log view, catalog dialogs, Explain, session mode and transactions, schema compare, data dictionary export, preferences beyond appearance, approvals UI.

## Shared services the GPUI form and history need

Both findings 9 and 14 need behaviour that today lives in GTK code or is written straight to storage by the GTK app. A frontend must not repeat it (ADR 0041), so the next step is two app-services commands used by both apps:

- `ConnectionCommands`: `validate`, `save` and password persistence now exist in `connection_save`; `test(draft)`, `delete(id)`, editing and the SSH, certificate and Kerberos fields still have to move out of the GTK dialog, with the policy checks and the audit record, and typed errors the frontends only render.
- `HistoryCommands`: `record(session, statement, outcome)` and `search(filter)`, owning the connection identity, redaction and the retention setting.

The GTK connection dialog and `record_history` move onto them in the same pull request, then GPUI builds its form and history list on the same calls.

## Recommended order

1. Done in this change: tokens (4), one look (5), menus (8), first part of the `GridView` split (7).
2. A connection write command in app-services shared by GTK and GPUI, then the GPUI connection form (9).
3. History on `HistoryCommands` (Explain is done).
4. Finish splitting `GridView` (sidebar, find, export, editor groups) before adding more features.
5. Visual comparison on a real display, light and dark, after each step.
