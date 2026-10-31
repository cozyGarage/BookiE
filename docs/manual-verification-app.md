# Manual verification: catalog, sidebar and application shell

Part of the [manual verification checklist](manual-verification-0.2-features.md), which holds the setup and private recording rules for every item here.

## Visual audit matrix

Use synthetic data and a separate test profile. Retain package and environment details with each private verification record.

Run the core grid/editor/connection screens in light, dark, high-contrast light and high-contrast dark. Start at scale 1 and the normal window size; repeat at the minimum supported width and with enlarged desktop text. Then inspect at 1.25 and 2 scale where supported, including a move between differently scaled monitors. Record logical window dimensions, monitor scale and text scale separately. Unavailable combinations remain blocked or not run.

- [ ] Show ordinary, selected, focused, edited, inserted, deleted and failed cells, including Boolean, NULL, empty text and long values. Pending state remains distinguishable without hue and is announced by Orca; recycling a row clears its previous state. Keep the existing production badge and window-title marker readable.
- [ ] Toggle light/dark and high contrast while both SQL panes are open. Check syntax text, selection, caret, gutter marks, statement band, completion and error highlights; compare with the native controls beside them.
- [ ] Measure custom text against its actual composited background, including selected rows and translucent state tints. Record foreground/background colors, ratio and screenshot. Use 4.5:1 as the target for normal text and 3:1 for meaningful non-text indicators; class names and uncomposited palette values alone do not prove a pass.
- [ ] Resize connection forms/popovers, editor toolbars, the browse pager, import mapping controls and result panes. Use long translated labels and large fonts. Run, Stop, transaction state, primary dialog actions and secondary menu actions remain reachable without overlap or clipped labels. Refresh row counts and change page size while narrow, then widen again.
- [ ] Navigate with keyboard only through text, Boolean and editable cells, menus and dialogs. Focus stays visible on selected rows and returns to its origin after dismissal. With animations disabled, a failed edit retains an understandable error indication.
- [ ] Scroll to the final grid row and rightmost column with wrapped/multiline values. Inspect text baselines, descenders, focus borders, bottom bars and popover edges at each tested scale. Copy and full-value editing preserve the underlying value; a display preview never becomes the saved value.

For a failure, record the action, expected and actual behavior, theme, scale, size, screenshot, binary identity and issue in the private verification record. Add an automated behavior regression where possible.

## Active grid edit ownership ([#1029](https://github.com/cozyGarage/BookiE/issues/1029))

These manual cases cover input-method composition and the full-value editor popover. The automated active-edit scenarios cover ordinary text entry and tab-close actions; they do not establish these input paths.

- [ ] Given a writable test table with two distinguishable rows, edit a text cell using the installed input method and compose a non-ASCII value. Commit the composition, then move focus to another cell and to a toolbar action. Save the draft and verify the database contains the exact composed Unicode value once, while the sibling row is unchanged.
- [ ] Given a dirty cell with its full-value editor popover visible, attempt to close the table tab. The close must be refused while the popover owns the editor; the popover and draft remain available, and the database is unchanged. Dismiss the popover, then verify the normal Save, Discard and Cancel outcomes for the draft.
- [ ] Record the input method and version, app build, database fixture, stored values before and after, and any refusal or prompt shown. Follow the [shared manual evidence rules](manual-verification-0.2-features.md).

## Catalog window

Main menu, then Catalog, on a PostgreSQL connection with a few objects of each kind.

- [ ] Each kind in the dropdown lists its objects with schema and detail.
- [ ] Typing a schema name and pressing Enter narrows the list; clearing it lists all.
- [ ] On SQLite or MySQL the status says the engine does not list that kind, not "Nothing of this kind was found".
- [ ] Switching kind quickly never shows the previous kind's rows under the new one.
- [ ] Closing the window mid-load leaves no error toast behind.

## Sidebar tree

Source follow-up [UI-43 / #1349](https://github.com/cozyGarage/BookiE/issues/1349) adds a persistent toggle, native docked divider and lazy field previews. Run this checklist on a build that includes the issue changes before marking the item complete.

- [ ] Toggle navigation at wide and narrow widths; the first Run click works after connecting. Drag the docked divider and use its keyboard handle; the editor remains at least 600 pixels wide. Cross the collapse breakpoint both ways while a table is expanded.
- [ ] Expand a table/view without opening a browse tab. Names and exact types are readable, keys have a visual cue, and Structure opens the correct object. Collapse while loading, refresh DDL metadata, switch connections, retry a failed request and exercise field limits; old replies never populate a replacement row.

- [ ] After connecting, confirm the schema/table sidebar is visible; hide/show it from a discoverable control and resize it with pointer and keyboard.
- [ ] On a PostgreSQL connection with two schemas, the sidebar shows each schema with Tables and Views groups and a count. Click a group: its objects hide and the chevron points right; click again to restore.
- [ ] Type in the sidebar search: groups holding a match open even if collapsed; clearing the search restores your collapsed groups. Quit and reopen the connection: the groups you collapsed are still collapsed.
- [ ] New Table and Table from CSV on a Tables group still open their dialogs for that schema. Light and dark look right.
- [ ] With a table tab open on a connection with many tables, collapse its group, type something that does not match it in the sidebar search, and scroll the list away. Press Ctrl+Shift+L: the search clears, the group opens, and the table's row is selected, in view and focused. From a SQL editor tab it says to open a table first. For a table in a hidden schema it says the schema is hidden.

## Preferences, About and application icons

- [ ] Open Preferences and confirm the page navigation separates General, Appearance, Editor and MCP.
- [ ] Change Appearance between System, Light and Dark; confirm the active application style changes and the choice survives a restart.
- [ ] Confirm the Data browser, Query history and SQL editor controls retain their current values and live-save behavior.
- [ ] Open About and confirm project credits and third-party attribution are presented separately, and the copyright attribution remains visible.
- [ ] Check the BookiE launcher icon at small and large sizes in the application menu and task switcher; confirm the same artwork is used by the installed package.

## Cell editor

- [ ] Check in-place cell editing in light and dark.

## Known gaps that are not GTK

- Creating a table from a CSV file falls back to PostgreSQL-leaning type names on ClickHouse, MongoDB, Redis and DuckDB. Only PostgreSQL, MySQL, SQLite and SQL Server have real type-name tables, and none of the four fallback engines was exercised.
- Check a bundle round trip between two real profiles in addition to automated coverage.
- Bundle export and import write administrative audit events. Run a bundle round trip and confirm both events appear in `audit.jsonl` (B4-22).
- SQL-format export and copied INSERT statements now preserve binary values on PostgreSQL, MySQL, SQLite, SQL Server and ClickHouse, with real-engine tests for NULL, empty blobs and all 256 byte values. DuckDB binary SQL literals also have a NULL/empty/all-byte round-trip regression in the B3 ledger; this checklist has no installed acceptance result for DuckDB. Other lossless-contract requirements are listed under B3.

## PostgreSQL backup and restore

- [ ] With `pg_dump` and `pg_restore` installed, connect to a PostgreSQL database, choose Back Up Database… from the main menu and save the file. A progress window with Cancel appears and closes, a toast says the backup was saved, and the `.dump` file exists.
- [ ] Change some rows, choose Restore from Backup…, pick the file and approve the prompt. The rows return to their backed-up state and the open browse tab refreshes. Deny the prompt on a second try: nothing changes.
- [ ] Repeat both through an SSH tunnel (built-in and system OpenSSH) and with TLS Verify Full.
- [ ] On a read-only connection, Back Up Database… works and Restore from Backup… is refused before a file is chosen. On a MySQL connection both explain that backups are PostgreSQL only.
- [ ] Press Cancel during a large backup: the window closes, nothing is written next to the chosen file, and `audit.jsonl` shows a Cancelled administrative outcome after its intent.
- [ ] On a disposable database, start a large restore and press Cancel after `pg_restore` starts. `audit.jsonl` shows an Unknown outcome, and the affected connection refuses governed writes before and after reconnect while a separate connection remains available. There is no in-app action to reconcile the outcome; inspect the database before deciding how to proceed. A restore failure after `pg_restore` starts has the same unknown outcome.
- [ ] Remove `pg_dump` from `/usr/bin` (or test on a host without it): the error names the missing tool and how to install it.

## Native workflow follow-up

- [ ] With an open owned-session transaction, check the separate active badge and commit/rollback-and-close buttons at narrow width and enlarged text. While a query runs they cannot dispatch; Commit on an aborted PostgreSQL transaction reports rollback. Session closure and connection replacement do not accept an old queued action.
- [ ] Run All and Run Current through the menu match their shortcuts in both split panes. File output names the statement/result set; exporting loaded rows never reruns SQL. A stream rerun is clearly labeled.
- [ ] Table toolbar and grid-menu export both show Scope before Format. All matching rows explains its snapshot refusal and cannot proceed; returning to Current page restores the action. A joined query exports its result columns and cannot be mistaken for a full source-table export.
- [ ] Header names and types retain their case; Column details exposes complete null/key metadata and a bounded column comment without making every header taller. Test keyboard menus and light/dark/high-contrast themes.

## Schema ER diagram

- [ ] Right-click a schema or table in the sidebar and choose "Show ER Diagram": a dialog opens showing cards for the schema's tables, column names with types, PK and FK indicators, and connector lines for foreign keys.
- [ ] Pan the canvas by mouse drag or arrow keys; zoom in and out with `+`, `-`, `0` and Ctrl+scroll.
- [ ] Selecting a table name from the left-side list centres and highlights that table card and highlights connected foreign-key paths.
- [ ] On a database with more than 150 tables in the selected schema, the ER diagram caps loading at 150 and displays a notice explaining the ceiling.
- [ ] Opening the diagram on an engine without foreign-key metadata (or without defined foreign keys) renders the table cards cleanly without connector lines or layout errors.

## SQL Server non-dbo catalog objects

- [ ] On a SQL Server database containing objects in non-dbo schemas (such as `sales`, `analytics`, or custom user schemas), the sidebar lists tables, views, triggers, routines, sequences, custom types, and roles partitioned under their respective schemas.
- [ ] Disabled triggers display a `(disabled)` status indicator in their sidebar listing.
- [ ] Multi-statement scripts creating and using session `#temp` tables execute sequentially and return results without syntax or scoping errors.

