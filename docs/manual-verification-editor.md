# Manual verification: SQL editor and sessions

Part of the [manual verification checklist](manual-verification-0.2-features.md), which holds the setup and private recording rules for every item here.

## Editor and history

- [ ] At 847 px and 802 px window widths, the navigation sidebar collapses and the main menu, editor Split action and window close control remain visible and usable. Fold block and Unfold all sit in the toolbar's fold menu at every width, and the toolbar actions wrap to a second line when the pane is narrow; Ctrl+Shift+[ and Ctrl+Shift+] fold and unfold SQL blocks.

- [ ] Toggle **Split** on and off with the mouse and keyboard; edit the query from either pane, confirm both views update, and check that each pane keeps its scroll position at a narrow window width.
- [ ] Ctrl+O opens a `.sql` file into a new tab titled with the file name.
- [ ] Typing marks the title with `•`; Ctrl+S saves, clears the mark and shows "Saved".
- [ ] Ctrl+S on an editor tab never opened from a file asks where to save.
- [ ] Ctrl+Shift+S saves to a new file and the tab then follows that file.
- [ ] Change the file in another editor, then press Ctrl+S: the "changed on disk" dialog appears. Cancel leaves both copies alone; Overwrite writes yours; Save As… writes elsewhere.
- [ ] Closing a tab with unsaved file changes asks Save/Discard/Cancel, and Save closes it only after the file is written.
- [ ] Saving a file that is a symlink updates its target and keeps the link.
- [ ] Restart with a file tab open: the tab is titled by the file again, unsaved edits still show `•`, and Ctrl+S saves to the same file.
- [ ] Restart with a saved connection, an active editor draft, a kept table tab and a preview table tab: restore the connection, draft and kept tab, omit the preview tab, and do not execute the draft until Run.
- [ ] Delete that file, then restart: the tab keeps its text and a toast says it is no longer linked.
- [ ] Ctrl+S on a browse or structure tab still saves pending edits there.
- [ ] A `.sql` file above the size limit is refused with an explanation, not truncated.
- [ ] A `.sql` file containing an excessively long line exceeding the per-line length ceiling is refused with an explanation before loading.
- [ ] Ctrl+Shift+D lists saved queries; open, rename and delete all work.
- [ ] Saving a structure change or edited rows appears in history, and the history dialog can filter those out.
- [ ] `TABLEPRO_LOG_FORMAT=json ./target/debug/tablepro` writes lines that parse as JSON; unset keeps the human format.

## Editor sessions (PostgreSQL, MySQL, SQL Server and SQLite files)

- [ ] Turn on Session in an editor tab: `SET search_path` and a `CREATE TEMP TABLE` from one run are still in effect on the next run.
- [ ] `BEGIN`, then an `UPDATE` in a later run: the button reads "Session · transaction open"; another tab cannot see the change.
- [ ] In a MySQL Session transaction, locking reads, `SET @user_variable`, savepoints and supported `SHOW` forms run; raw `BEGIN`, `COMMIT` and `ROLLBACK`, DDL, account statements, `LOCK TABLES` and `SET autocommit` remain refused. Confirm `ROLLBACK` leaves user-variable values unchanged and does not release named locks acquired by functions in assignment expressions; release the lock explicitly.
- [ ] Turning Session off with a transaction open asks Commit / Roll Back / Cancel, and each does what it says; Cancel leaves the session on.
- [ ] Turn Session off while idle: after close completes, the transient closing status disappears and Run is enabled.
- [ ] Closing the tab with a transaction open asks first; Roll Back and Close leaves the table unchanged.
- [ ] Without Session, a lone `BEGIN` is refused and the message mentions Session.
- [ ] On ClickHouse or an in-memory SQLite database, turning Session on explains that the engine does not offer it yet and the button turns back off.
- [ ] The audit journal shows the session's COMMIT or ROLLBACK against the same batch as the statements inside it.

## Session failure and shutdown paths (B4 I4)

- [ ] Stop appears only where server cancellation is supported; stopping a session query records the terminal outcome and permits later supported queries.
- [ ] A session timeout reports its actual cancelled/unknown state in the UI and audit; it does not present an ambiguous write as safely undone.
- [ ] Failed COMMIT after an aborted PostgreSQL transaction shows rollback; a deferred-constraint failure clears the transaction-open state and retry does not fabricate a second commit.
- [ ] Window close and Disconnect with an open transaction ask first, honor Cancel, and finish the chosen rollback/commit before teardown; verify persisted rows.
- [ ] With an editor Session open, stop the database host (or block its port) and close the window. After the cleanup timeout, **Could not close database sessions** offers **Keep Open** (default) and **Close Anyway**; Close Anyway closes the window, Keep Open leaves it usable.
- [ ] A retired session turns its toggle off; late state messages cannot relabel a replacement session. Automatic reconnect retires the previous tab sessions.
- [ ] An unknown write outcome blocks only its owning connection; another connection stays usable. Restart/reconnect recovery follows the durable audit rules.
- [ ] `SET autocommit=0`, `SET IMPLICIT_TRANSACTIONS ON`, `XA START` and an unterminated transaction batch are refused on applicable shared connections.

## Explain tree

- [ ] On PostgreSQL, Explain a join: the dialog opens on a Tree tab with nested steps, a cost bar per step and the filter or condition lines under their step; the Text tab shows the raw plan.
- [ ] Press Escape while the Explain plan dialog is open: the dialog closes.
- [ ] On SQLite, Explain a query that scans and searches: steps nest by their parent.
- [ ] On MySQL, ClickHouse or DuckDB the dialog shows the text plan with no Tree tab.
- [ ] Explain on an empty editor still shows the existing message.
- [ ] On a connection where a plan takes a few seconds (for example `EXPLAIN ANALYZE SELECT pg_sleep(20)` on PostgreSQL), open the Explain window and close it at once. In the database's activity view the statement is gone within about a second, not after the query timeout.

## SQL code folding

- [ ] Place the cursor inside a multiline CASE block and press Ctrl+Shift+[. The block body folds and its opening and closing lines remain visible.
- [ ] Click the fold marker in the gutter. The body returns without changing the query text.
- [ ] Fold two blocks, then use Unfold all and Ctrl+Shift+]. Both show the complete query.
- [ ] Edit the query while a block is folded. The editor reveals the full query and clears stale fold markers.

## Pinned results and gutter marks

- [ ] Run `SELECT 1 AS first;`, then run `SELECT pg_sleep(3), 2 AS second;`: while it runs, the first result stays on screen, dimmed, with Running… and the spinner in the toolbar; when it finishes the dimming is gone and the second result is shown. Repeat with a query that fails, with Cancel, and with a statement that times out: each replaces the dimmed result. A streamed PostgreSQL SELECT keeps the previous result until its first page arrives.

- [ ] On SQL Server, run `EXEC sys.sp_executesql N'SELECT 1 AS first_result; SELECT 2 AS second_result;'` as one query. Confirm the result-set selector shows both sets in source order and each displays its own column and value.
- [ ] Run `SELECT 1 AS one; SELECT 2 AS two;`, pin both results, then run `SELECT 3 AS three;` and `SELECT 4 AS four; SELECT * FROM missing_table;`: pinned results stay first and the failed second statement is labelled **Statement 2**, not Statement 4. Unpin removes the pinned results.
- [ ] After running a script, each statement's first line shows a tick or, for a failed statement, a cross in the gutter, in light and dark. Edit the script and run it again: the marks follow the new run.
- [ ] PostgreSQL: pin a result, then run a long streamed SELECT and press Stop (or let it time out): the pinned result stays above the Cancelled or Timed out page, and Pin is unavailable while the stream is still loading.

## Closing a window during a save

- [ ] Start a structure save that fails on the server (for example, add a NOT NULL column without a default to a table that has rows), then close the window while it runs. The "Saving in progress" dialog closes when the save ends, and only the unsaved-changes prompt remains.

## Exact SQL completion

- [ ] On PostgreSQL, create distinct `users` and `"Users"` relations, plus a relation with a space or escaped quote in its name. Use Ctrl+Space to open completion, select each relation with the arrow keys, and accept with Enter or Tab. Run the resulting SELECT and confirm the intended relation. Repeat in the split editor view.
- [ ] Complete an alias-qualified column in the middle of a query. The alias and following SQL stay intact. Undo restores the original identifier in one step.
- [ ] With a screen reader, confirm completion names can be read and selected. This remains a manual accessibility check; provider activation tests do not establish screen reader behavior.

## Editor bottom panel

- [ ] On the target desktop, run a query with a unique visible value, hide/show Results by button and Ctrl+J from SQL and the result grid, and confirm the same result and selection remain.
- [ ] Resize the divider, maximize/restore, and hide while maximized. Confirm the normal divider size and the previously focused SQL split are restored.
- [ ] Check 1,200 px with normal SQL text and 802 px with a 20-point SQL font in light/dark themes. Every panel control remains reachable.
- [ ] Run successful, failed and timed-out queries. Errors shows only the latest accepted failures; switching views does not hide or rerun the result set.
- [ ] Close a query with the panel hidden while another editor remains open. Its controls still work and no late result revives the closed editor.

Xvfb coverage does not replace native desktop or accessibility checks.

Repeat these checks on the target desktop and release package; Xvfb coverage does not replace native interaction or accessibility checks.

## Editor error navigation

- [ ] On native Wayland with PostgreSQL, run a script whose third statement fails after a Unicode value. Go to error in the result restores a maximized editor and places the caret at the failed statement's exact character position.
- [ ] Run Current with two identical statements through real Ctrl+Shift+Enter. Go to error moves the caret to the second occurrence. SQL inside a leading comment is not used as a navigation target or marked as executed.
- [ ] Use Go to error from the Errors view. Edit the query and retry the old action; the caret stays put and a fresh run is requested. A unique database row confirms navigation never reruns the script.
- [ ] Reach the action by mouse in a 1,200 px dark window and an 802 px light window with 20-point SQL text. Error controls remain visible above scrollable text.

Release package behavior, toolkit compatibility and screen-reader behavior require separate checks.
