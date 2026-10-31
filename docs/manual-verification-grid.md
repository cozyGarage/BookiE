# Manual verification: grid, filters and data transfer

Part of the [manual verification checklist](manual-verification-0.2-features.md), which holds the setup and private recording rules for every item here.

## Export dialog

Open a result, then Export Results.

- [ ] The Format list offers CSV, JSON, Markdown, HTML, XML, SQL and Excel.
- [ ] SQL is absent for a MongoDB or Redis connection, present for the others.
- [ ] The CSV options group shows for CSV and hides for every other format.
- [ ] A SQL options group with an editable Table name shows only for SQL.
- [ ] For a browse tab the Table name is prefilled `schema.table`; for an editor tab it is the tab label, and editing it changes the emitted statements.
- [ ] The written `.xlsx` opens in LibreOffice and the `.html` in a browser.
- [ ] Cancelling a large export leaves any existing destination file untouched.
- [ ] An Excel export above the sheet row limit is refused, and the message names the limit.

## CSV import

Right-click a table in the sidebar, then Import CSV. Separately, use the second icon button in a schema header for Table from CSV.

- [ ] Separator and header-row detection are right for a comma file, a semicolon file, and a file with no header.
- [ ] The preview grid matches the file.
- [ ] Column mapping is prefilled sensibly and can be changed.
- [ ] For an existing table, remember a mapping, reorder the CSV headers, restore it, and confirm values still target the named columns; added destination columns remain on Skip.
- [ ] Duplicate CSV headers are distinguishable, and a saved mapping refuses to restore if its selected header or destination column is missing or ambiguous.
- [ ] A mapping saved for one connection, database, schema or table is not offered for another; Forget removes only the current table mapping.
- [ ] With the header-row option off, saving or restoring a mapping is refused. No CSV row values appear in the saved mapping file.
- [ ] For SQLite `id INTEGER PRIMARY KEY`, a matching CSV `id` header is preselected when explicit auto-increment values are supported. The preview says those CSV values will be inserted when mapped, and source IDs 51 and 52 are preserved.
- [ ] Creating a table shows a name and an inferred type per column, both editable before anything runs.
- [ ] Approval is requested **once** for the whole import, not per batch.
- [ ] Progress advances and Cancel stops it.
- [ ] A file with a bad row partway through stops the import and the message says how many rows were written.
- [ ] Import refuses to start on a read-only connection.

## Structure tab, column comments

- [ ] Open an existing Structure tab and make an unsaved change: its title keeps the `· Structure` suffix alongside the dirty marker.
- [ ] SQLite: create a new table with a foreign key in the Structure tab and save; the table is created with the key. On an existing SQLite table, Add Foreign Key… is greyed out and says SQLite cannot add one to an existing table.
- [ ] A Comment row appears per column on PostgreSQL, MySQL, SQL Server and ClickHouse.
- [ ] On ClickHouse the field is populated but not editable for an existing column, with a tooltip saying why.
- [ ] No Comment row appears on SQLite or DuckDB.
- [ ] Adding, editing and clearing a comment all save and survive a reload.
- [ ] Renaming a column and editing its comment in the same save works.

## Active table cell close and preview paths

- [ ] Type into a table cell without pressing Enter, then close it with the tab button and Ctrl+W. Cancel keeps the editor and draft, Save stores the typed value, and Discard leaves the database unchanged.
- [ ] Repeat the pending cell edit through Close Other Tabs, Close Tabs to the Right, window close, quit and Disconnect. Each prompt or teardown decision must wait until the edit is staged; cancel preserves the connection and draft. GTK automation covers the tab button, Ctrl+W, save/discard, inactive-tab close, preview replacement, quit and Disconnect. Verify both tab context-menu actions by hand.
- [ ] Type into a preview table cell and open another table. The edited preview becomes a kept tab and retains the pending value. Repeat with a kept tab and with the edited tab inactive.
- [ ] Turn off **Reuse one tab when opening tables** in Preferences, then click three tables in the sidebar. Each opens its own kept tab with no preview icon, and clicking an already-open table selects its tab. Turn the setting back on: the next click opens a preview tab again, and restarting keeps the choice.
- [ ] Attempt closure while a cell editor has an active IME preedit or value popover. Closure is refused without discarding or replacing the in-progress input.
- [ ] Close a tab while its save is in flight, then deliver a late save completion after opening a replacement tab. The completion must not close or change the replacement.

## ENUM and SET pickers (PostgreSQL, MySQL and MariaDB)

- [ ] Double-click an ENUM cell: a list of the declared members opens with the current one highlighted; choosing another shows a pending change and Save writes that member.
- [ ] Choose the (empty) member of an ENUM that declares one: the cell shows empty and the stored value is the empty string, not NULL.
- [ ] Double-click a SET cell: each member has a check; toggling and pressing Apply writes the members in declared order; with none checked the stored value is the empty set, not NULL.
- [ ] Set to NULL from the cell menu still works on a nullable ENUM or SET.
- [ ] On PostgreSQL, an enum column offers its labels in declared order; a column of an enum array type still opens the text editor.

## Paste into cells

- [ ] Copy a 2 by 2 block from a spreadsheet, click a cell in a browsed table with at least two rows and columns below and right of it, press Ctrl+V: four cells show as pending changes and the unsaved-change count reads 4.
- [ ] Paste a block that runs past the last loaded row or column: a toast says so and nothing changes.
- [ ] Paste a value that does not parse for its column type: a toast names the column and row and nothing changes.
- [ ] Paste with no cell focused: a toast asks you to click a cell first.

## Go to Referenced Row

- [ ] In a table with a foreign key, right-click a non-empty value in the key column: Go to Referenced Row opens the referenced table with one filter rule on the referenced column.
- [ ] On a cell of a column that is not a foreign key, or on a NULL key, the item is greyed out.
- [ ] With the referenced table already open, the same action switches to that tab and replaces its filter.
- [ ] A composite key filters every referenced column from the same row; with one of its columns hidden, a toast says so.

## Filter by This Value

- [ ] Right-click a cell in a browsed table: Filter by This Value narrows the rows and the filter strip shows the new rule.
- [ ] On a NULL cell it filters with IS NULL.
- [ ] Doing it again on another value of the same column replaces that rule.
- [ ] A JSON or binary cell shows "Values of this type can't be used as a filter".
- [ ] The item is absent from SQL editor result grids.

## Named browse filters

- [ ] On one table, build and apply two structured filters, save them under different names, and switch between them without editing the rules.
- [ ] Rename one preset and delete the other; restart BookiE and confirm the renamed preset remains scoped to the same connection, schema and table.
- [ ] Save a preset with raw SQL, an empty name, an overlong name and beyond the per-table limit; each is refused with a useful message.
- [ ] Open the preset on a table whose columns changed; the invalid saved rule is not applied silently.

## MongoDB sorted browse

Use a stable collection with enough documents to browse multiple pages. Pages outside the ascending `_id` keyset path use offset pagination. Keep performance measurements in private verification records.

- [ ] Sort by a supported field in ascending and descending order; each page follows the selected order.
- [ ] Apply a filter and sort, then move across several pages; each page keeps the requested order and contains the expected rows.
- [ ] Repeat with a sort other than ascending `_id` and a deep offset; correct ordering is required, but keyset performance is not implied.

## Grid previews for long values

In browse and SQL result grids, text and JSON cells display at most 512 Unicode characters. Text shows the remaining byte count. JSON shows a byte count when it fits within the bounded serialization budget, or an ellipsis when serialization stops at that budget. This bounds GTK/Pango work for long values in wide grids. Values over 8 KiB also use a typed preview with the original byte count; binary cells over 8 KiB show a typed preview. SQL result grids are read-only. In an editable browse table, confirm that a long text cell opens for inline editing with the full value, not the displayed preview. **View Value** shows a bounded rendering. **Copy Value** copies complete text and JSON values; binary values currently copy only a byte-count placeholder ([#1587](https://github.com/cozyGarage/BookiE/issues/1587)). Text and JSON cells over 8 KiB and up to 1 MiB can be edited in the bounded full-value dialog. Use values just below and just above the 512-character and 8 KiB thresholds, plus a value over 1 MiB for the read-only view.

- [ ] Values up to 512 Unicode characters display in full; longer text shows a byte-count suffix, while JSON beyond the bounded serialization budget shows an ellipsis. Copy Value and View Value retain the complete source value.
- [ ] Values over the threshold show a prefix and `… (N bytes total)`; SQL result cells and binary/unsupported browse values do not offer inline editing. Editable browse text opens with the full value.
- [ ] View Value shows up to 1 MiB of rendered text or JSON and indicates when longer content is truncated.
- [ ] Short values wrap without display-only hyphens. Long values use horizontal scrolling so GTK does not spend excessive time laying out one unbroken line.
- [ ] Copy Value copies the complete text or JSON rather than the displayed preview.
- [ ] Repeat with a binary value over the threshold; the preview reports the original byte count and View Value shows at most the first 16 KiB of source bytes as hex with the total byte count.

Binary Copy Value currently copies only the byte-count placeholder; full binary copying is tracked in [#1587](https://github.com/cozyGarage/BookiE/issues/1587).

## Grid cell editing: long text and JSON

Use a keyed table with a nullable text column and values above 8 KiB. Include CRLF, Unicode, trailing spaces and a value near 1 MiB.

- [ ] The cell shows a shortened preview and byte count. Right-click it and choose **Edit Full Value…**; the dialog contains the complete value.
- [ ] Cancel stages no change. Reopen the dialog, change the final character and choose **Apply**; save and reload to confirm the exact text.
- [ ] Apply empty text and confirm it remains empty. Use **Set to NULL** separately and confirm the stored value is SQL NULL.
- [ ] Edit a draft text row, save it, and discard a separate pending edit.
- [ ] A value over 1 MiB is refused. Non-text, generated, key and read-only cells do not offer the action.
- [ ] Edit JSON and JSONB values with reordered keys, duplicate source-text keys, large integer lexemes, JSON `null`, and SQL NULL. Apply valid input and reload to confirm the correct stored carrier; invalid input leaves the dialog open and stages no change.
- [ ] **Format JSON** visibly formats semantic JSON values. Source-text JSON has no lossy formatter; its exact text stays intact until edited. Confirm empty text, JSON `null`, text `null`, and SQL NULL remain distinct.

## Server activity view: binary values

- [ ] Open a connection's activity/session view while a query on a `bytea` or `varbinary` column is visible. A binary value shows as `<N bytes>`, not as a value that reads like a hexadecimal escape (for example not `\x10 bytes`).

## Result headers, selection statistics, error copy and connection strip

- [ ] Browse a table: each column header shows its name, a key marker for primary and foreign keys, and the exact engine type spelling beneath it, in light and dark.
- [ ] Select several rows of a numeric column: the selection badge popover lists the column's count, sum, average, min and max, and Copy summary copies them. A text column is not listed.
- [ ] Run a failing statement in the editor: Copy error places the exact error text on the clipboard.

## Clipboard export formats (Copy as...)

- [ ] Select multiple rows in a result grid, right-click, and choose **Copy as > Text Table**: the clipboard receives complete selected rows in an ASCII-rendered table with header names, border delimiters `+---+`, column dividers `|`, and right-aligned numbers.
- [ ] Select multiple rows, right-click, and choose **Copy as > JSON Lines**: the clipboard receives newline-delimited JSON objects with one object per row and column names as keys.
- [ ] Select multiple rows in a SQL-supported connection, right-click, and choose **Copy as > Values List**: the clipboard receives a comma-separated SQL tuple list `(val1, val2), (val3, val4)` with engine-specific escaping (`TRUE`/`FALSE` or `1`/`0`, `N'...'` on SQL Server).
- [ ] On MongoDB and Redis connections, the Values List action is omitted or disabled.

## Foreign key navigation (Referenced and Referencing rows)

- [ ] In a table browse tab, right-click a foreign key cell and choose **Go to Referenced Row**: opens a tab for the referenced parent table filtered to `parent_pk = <fk_value>`.
- [ ] On a table referenced by inbound foreign keys (e.g. `customers` referenced by `orders.customer_id`), right-click any persisted row and choose **Show Referencing Rows**: a sub-menu lists referencing child tables. Selecting a target opens that table in a new tab filtered to matching child rows (`customer_id = <id>`).
- [ ] On an unpersisted newly inserted row or a table with no inbound foreign keys, **Show Referencing Rows** is absent or greyed out.
