# GPUI design audit

Date: 2026-11-18. Against [DESIGN.md](../../../DESIGN.md), [docs/ui-review-checklist.md](../../../docs/ui-review-checklist.md) and [ADR 0054](../../../docs/decisions/0054-one-visual-language.md), at `main` plus this change.

## Method

Read `theme.rs`, `chrome.rs`, `render.rs`, `main.rs`, `text_input.rs` and `palette_actions.rs`, counted patterns with grep, and ran `cargo test` for the app (244 tests, all pass). Not done: a visual comparison. GPUI needs a GPU and the private test display has none, so no screenshots were taken. Do that next on a real display.

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
| 4 | Theme lacks tokens the design needs: destructive, success, focus ring, popover, sidebar | `Theme` has 14 fields; none of these | Open |
| 5 | A second visual style (`Style::Adwaita`, 6 px corners, GNOME blue) contradicts ADR 0054's single dbx look | `theme.rs`, `BOOKIE_PROTOTYPE_THEME` | Open: decide whether to remove it or keep it as a labelled option |
| 6 | Window controls are text glyphs (`–`, `□`, `×`) | `chrome.rs` `window_button` | Open: use icons; platform buttons on macOS and Windows |
| 7 | `GridView` is a god object: 70 fields, with parallel vectors (`runs`, `run_edits`, `run_sources`) and about ten palette fields | `main.rs:80` | Open: one `Run` struct per result, palette state in its own struct |
| 8 | No dropdown or context menu exists: the GTK menu, toolbar overflow and row menus have no GPUI counterpart | `rg "PopupMenu\|ContextMenu"` finds nothing | Open |
| 9 | Connections can only be chosen through environment variables | README, `BOOKIE_PROTOTYPE_CONNECTION` | Open: needs a connection list and form |
| 10 | The toolbar carries window-title controls but no query actions (run only appears inside the query tab) | `chrome.rs` | Open |
| 11 | Zoom for result views is a font setting only, not Ctrl+scroll | `font_settings.rs` | Open |
| 12 | Find filters only after Enter | `find_filter.rs` | Open (known) |

## Feature gap against the GTK app

GPUI has: result grid with Find, filter, compare, export and row copy; SQL editor with several carets; tabs; sidebar tree with search and regex; command palette; settings (appearance, fonts); server status views (sessions, locks, replication, variables). GTK also has, and GPUI lacks: connection manager and forms, SSH, structure editor, import, backup and restore, ER diagram, query history and saved queries, audit log view, catalog dialogs, Explain, session mode and transactions, schema compare, data dictionary export, preferences beyond appearance, approvals UI.

## Recommended order

1. Finish the token set (finding 4) and decide finding 5, so the theme matches DESIGN.md.
2. Menus: a shared popup menu and context menu (finding 8), then move the editor toolbar into icons plus an overflow, as in GTK.
3. Connection list and form (finding 9), then Explain and history, which are the most used GTK features missing here.
4. Split `GridView` (finding 7) before adding the features above, so they do not grow it.
5. Visual comparison on a real display, light and dark, after each step.
