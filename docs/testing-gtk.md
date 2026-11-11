# GTK tests

Part of [testing](testing.md), which holds the test tiers, runner and fixtures.

## GTK tests

Run the installed safety suite with:

```bash
./scripts/test-gtk-safety.sh
```

The script builds `bookie-app`, always starts a private runtime directory, isolated D-Bus session and Xvfb display, and drives the real application through PyAT-SPI. Each scenario gets temporary XDG directories and a fixture SQLite database. A scenario may mark its saved fixture connection as production to test policy prompts; it never opens a user's saved connection or production database. The runner uses a standalone AT-SPI D-Bus daemon and passes the Xvfb display to D-Bus activation, so it does not reuse desktop accessibility or portal processes. GTK tests exercise the GTK chooser on X11; they do not replace installed Wayland package testing.

The Xvfb runner sets `GDK_DEBUG=no-portals`. Its isolated GNOME portal backend exposes settings but no file chooser, so the harness uses GTK's chooser in tests instead of depending on the host desktop's portal implementation.

The suite reports every failed scenario and exits non-zero. Read the first failure first because later scenarios may be affected by shared fixture state.

The grid-search keyboard scenario checks focused, visible rows across a 5,000-row result after accepting and dismissing both search dialogs. It complements the 300-column alignment scenario. On X11, these scenarios also capture successful scroll positions when an artifact directory and either `scrot` or ImageMagick `import` are available. Screenshots supplement the focus and bounds assertions; they do not establish native desktop or package acceptance.

Representative end-to-end scenarios in the suite are listed below; the current registry, including scenarios from the GTK workbook, parameter, XML and UX modules, is in [`gtk_safety.py`](../apps/linux/app/tests/gtk_safety.py).

1. Dismissing approval for a fixture connection marked production leaves its SQLite database unchanged.
2. Approving once performs one mutation and prompts again for the next operation.
3. Unavailable audit storage denies the mutation without showing an approval path around the failure.
4. A named `:parameter` in the editor prompts for a value and writes the bound value, not the placeholder text.
5. Ctrl+D saves the editor query as a favorite, and Ctrl+P finds it, opens it, and records the use.
6. A successful saved-connection switch tears down the old workspace and sends writes only to the new database.
7. A failed candidate connection leaves the original editor and database usable.
8. A running read is cancelled and fully settles before the candidate connection is activated; subsequent writes reach only the new database.
9. Current-page CSV export writes exactly the first 100 PK-ordered rows from a 150-row fixture through the real portal chooser.
10. Pending edits gate a connection switch; discarding does not write to either database.
11. A browse tab reopened after switching reads the new connection.
12. Two windows keep queries attached to their own databases.
13. Switching away and back after the persistence debounce preserves workspace tabs.
14. Switching one window preserves another window's pending edits.
15. Current-page JSON export preserves order, Unicode, escaped text, empty text, NULL, and binary values; binary values use `\x`-prefixed hexadecimal, including `\x` for an empty blob.
16. Graceful quit and a new process restore the last connection and selected editor text without executing it; a subsequent explicit Run writes only to that connection. The saved browse tab also reopens.
17. Keyboard column search jumps to the selected column.
18. Committed editor DDL refreshes the sidebar.
19. SQL character warnings leave the editor query unchanged.
20. Current-page XLSX export preserves typed SQLite values, nanosecond times/timestamps, BC/year-10000 dates, wide signed integers, infinity, NULL and formula-like Unicode text. Workbook XML proves native versus text cells and the 100-row page boundary; all source rows remain unchanged.
21. Cancelling the workbook save chooser preserves an existing destination, leaves no temporary export, and leaves all source rows unchanged.
22. Empty text in workbook results produces a visible error with the row/column and CSV/JSON alternatives. No lossy workbook or temporary export is published, and source rows remain unchanged.
23. Repeated named parameters insert exact minimum/maximum signed integers and SQL-like Unicode text into two rows, preserving binding order and treating the payload as data.
24. Cancelling the parameter dialog writes no rows; retrying the same statement uses the replacement values and never executes the cancelled values.
25. XML export round-trips carriage returns, CRLF, LF, tabs, literal entity-looking text and Unicode through an independent XML parser; NULL remains distinct from empty text.
26. XML export of NUL-containing text shows the code point and row/column error, publishes no file, leaves no temporary export and does not change source rows.

The XML scenarios live in `apps/linux/app/tests/gtk_xml.py`. Run `xml_export_preserves_line_endings_and_null_distinctions` or `xml_export_refuses_illegal_text_without_publishing` with `BOOKIE_GTK_SCENARIO`. Their Python oracle tests deliberately normalize carriage returns and collapse empty text into NULL, requiring both corruptions to fail. Core tests independently check atomic destination preservation for XML-illegal controls and U+FFFE/U+FFFF.

The parameter scenarios live in `apps/linux/app/tests/gtk_parameters.py`; select `repeated_parameters_preserve_wide_ids_and_sql_like_text` or `cancelled_parameters_never_execute_and_retry_uses_new_values` through `BOOKIE_GTK_SCENARIO` to run one. Registration tests require both in the default suite. Each checks committed SQLite rows independently of the visible UI result.

The workbook scenarios live in `apps/linux/app/tests/gtk_workbook.py` and are registered in the default suite. Their XML oracle has separate negative tests that reject rounded values, wrong types, lost precision, formulas and incorrect page boundaries. SQLite supplies typed Date/Time/DateTime for these UI scenarios; typed TimestampTz UI coverage against a network server remains separate from the core and PostgreSQL contracts. The cancellation UI test stops at the chooser; a core regression also cancels after writing a row and checks atomic destination preservation.

To run one scenario using an already-built candidate (without another Cargo build):

```bash
BOOKIE_GTK_BINARY=/absolute/path/to/bookie \
BOOKIE_GTK_SCENARIO=current_page_workbook_preserves_typed_values \
BOOKIE_GTK_ARTIFACT_DIR=target/gtk-evidence \
bash scripts/test-gtk-safety.sh
```

Use `cancelled_workbook_export_preserves_existing_file` for the cancellation case. Record the binary's commit/checksum; a prebuilt binary is not evidence for later Rust edits. Omit `BOOKIE_GTK_BINARY` to build the current release profile.

Each scenario creates a temporary profile and fixture connection/database. Its `environment` attribute selects the policy environment to exercise, and `audit_available` controls whether the fixture audit journal is present. These tests do not use existing saved connections or a user's production database.

The PostgreSQL scenario `postgres_large_stream_pages_round_trip_from_spool` checks page navigation and continuous-grid keyboard navigation back to the first row after the last page loads. It checks the query's run marker throughout. Run it with `BOOKIE_GTK_SCENARIO=postgres_large_stream_pages_round_trip_from_spool bash scripts/test-gtk-postgres.sh`; the script starts a disposable PostgreSQL container. `BOOKIE_GTK_STREAM_ROWS` defaults to 150,000 and accepts a multiple of 5,000 of at least 10,000.

The PostgreSQL scenario `postgres_whole_result_find_filter_and_compare` streams a 2 KiB-per-row result, finds a known label across every saved page and steps through the matches, filters them into a derived result that offers **Export all rows…**, then pins the result, runs a second query with computed added, removed and changed rows and compares the two by `id`. It asserts the exact summary counts and that the audit journal holds one `result_compare` intent and outcome pair with the differing-row count and no row values. Run it with `BOOKIE_GTK_SCENARIO=postgres_whole_result_find_filter_and_compare bash scripts/test-gtk-postgres.sh`; `BOOKIE_GTK_STREAM_ROWS` defaults to 150,000 and accepts a multiple of 5,000 of at least 10,000. Two results of 150,000 rows use about 650 MB of the temporary spool, so keep the spool location (`BOOKIE_SPOOL_DIR`) on a file system with that much room.

Memory profiles and second-launch checks retain the exact subprocess launched by the fixture. They verify its PID lifetime and executable file identity rather than scanning process names. Second launch reuses the supplied fixture environment; it never reads another process's environment. Profile rows record the owned PID and Linux process start ticks. An exited or replaced target fails the check instead of selecting another application.

To capture GTK allocation stacks for a wide editable table, use `bash scripts/profile-wide-table-heaptrack.sh`. It injects Heaptrack into only the BookiE child process, so executable identity checks remain active. `BOOKIE_PROFILE_ROWS` and `BOOKIE_PROFILE_COLUMNS` select the synthetic table shape; the defaults are 100 rows and 512 columns. `BOOKIE_PROFILE_HEAPTRACK_OUT` selects the compressed trace path. Heaptrack adds substantial overhead; use the ordinary RSS profile for memory comparisons and do not compare instrumented RSS or timing with it.

`bash scripts/profile-stream-pin-retention.sh` profiles five PostgreSQL streamed results pinned in separate editor tabs. It samples the owned app's RSS, PSS and private memory after each result, after unpinning it, and after closing its editor; it also samples three seconds after all five editors close. Each sample includes `peak_rss_kib`, the process-lifetime RSS high-water mark from `/proc/<pid>/status`, separately from checkpoint RSS. It writes JSONL to `/tmp` by default. The default workload uses 34,000 to 38,000 rows per result with a 2 KiB text cell; three repetitions exercise aggregate retention beyond one editor. `BOOKIE_PROFILE_STREAM_ROWS` sets the starting row count from 34,000 to 40,000; each subsequent result adds 1,000 rows. `BOOKIE_PROFILE_STREAM_CELL_BYTES` accepts 1,024 to 4,096 bytes. `BOOKIE_PROFILE_STREAM_VALUE` selects `text`, `binary` or `json`, and `BOOKIE_PROFILE_STREAM_WIDTH` selects 3 to 64 columns. This is diagnostic evidence, not a memory limit assertion: allocator retention and the fixture environment affect RSS, while spool files are outside process memory. The script starts and removes its own PostgreSQL container. Set `BOOKIE_PROFILE_REPETITIONS` or `BOOKIE_PROFILE_OUT` to adjust the run.

`bash scripts/profile-stream-export.sh` profiles the streamed query export path with a 39,999-row, 10-column PostgreSQL result and a 2 KiB text cell in each row. It measures query and export duration, app CPU, RSS/PSS/private memory before and after export, and output size, then validates every exported CSV row. Export reads the completed query result from its encrypted spool without re-executing SQL. The editor tests cover page order, cancellation, incomplete-result refusal and preserving an existing destination on export failure. The script uses Xvfb/AT-SPI, writes JSONL to `/tmp` by default, and starts and removes its own PostgreSQL container. Set `BOOKIE_PROFILE_EXPORT_ROWS`, `BOOKIE_PROFILE_EXPORT_CELL_BYTES`, `BOOKIE_PROFILE_REPETITIONS` or `BOOKIE_PROFILE_OUT` to adjust the run. Measurements are host-specific and are not thresholds or native Wayland evidence.

The Open Quickly scenario waits for the filtered result set before invoking its single action; the already-visible favorite is not proof that the debounced row rebuild has completed. It still requires the window to close and usage to be persisted.

Buttons and rows are invoked through named AT-SPI actions; the harness re-queries the row while waiting up to 15 seconds for its action to appear in the accessibility tree. There is no generic Return-key fallback. Keyboard events exercise shortcuts and the export format combo's navigation. The combo has no AT-SPI click/focus action: its scenario tabs until the named control reports focus, opens its list, navigates to the requested format, and verifies its selected label before exporting. Each denial assertion requires the row count to hold for a settle window rather than matching once.

On Arch or Omarchy, install the local harness dependencies with:

```bash
sudo pacman -S --needed dbus xorg-server-xvfb at-spi2-core python-atspi
```

Service-level tests cover pure state and policy behavior, but they do not replace cross-layer GTK and native desktop checks.

SQLite integration regressions also distinguish NULL from zero, false, empty text, and empty blobs in declared columns, and preserve binary values despite declared-type affinity mismatches. Export unit tests overlap writers deterministically and check that pre-existing temporary symlinks cannot redirect writes. Workspace tests preserve active-tab identity when unknown records are removed.

For ordinary UI changes, test the affected flow manually and include before and after screenshots. Add deterministic automation when a regression can be reproduced without timing or desktop-session assumptions.

## Large-result grid scroll profiles

Use `bash scripts/profile-grid-scroll-matrix.sh` to profile the GTK grid with 100,000 synthetic SQLite rows. The default matrix runs ordinary XTest wheel input with unique or repeated text and narrow or wide results. Each case runs three times. The wheel profile sends 40 wheel events per direction at 80 ms intervals, records visible row ranges and timing, then uses the accessible scrollbar only to verify the loaded result's end and return-to-top range outside the timed interval. A final query confirms that the Run control and result grid still respond after scrolling.

Set `BOOKIE_PROFILE_ROWS`, `BOOKIE_PROFILE_REPETITIONS`, `BOOKIE_PROFILE_WHEEL_EVENTS`, `BOOKIE_PROFILE_WHEEL_DELAY_SECONDS`, `BOOKIE_PROFILE_SCROLL_STEPS` or `BOOKIE_PROFILE_OUT` to adjust the bounded workload. The scenario also accepts `BOOKIE_PROFILE_TEXT_MODE=unique|repeated`, `BOOKIE_PROFILE_LAYOUT=narrow|wide` and `BOOKIE_PROFILE_INPUT=wheel|scrub`. `BOOKIE_PROFILE_INCLUDE_SCRUB_REFERENCE=1` adds repeated unique-text narrow scrollbar-scrub runs. `BOOKIE_PROFILE_MATRIX_FULL=1` adds scrollbar-scrub runs for every text and layout combination. Results are JSON Lines and record the input mode, layout, text mode, requested rows, row-range checks, app CPU and wall time, and process RSS. They are synthetic X11/Xvfb measurements; native Wayland interaction remains a separate acceptance step.

Use `bash scripts/profile-export-dialog.sh` to measure how long the loaded-result CSV export dialog takes to open for a synthetic ten-column SQLite query. It records query time separately from export-dialog wall time and application CPU, plus resident and peak memory. The default uses 39,999 rows with one 2 KiB text cell per row. Set `BOOKIE_PROFILE_EXPORT_ROWS`, `BOOKIE_PROFILE_EXPORT_CELL_BYTES`, `BOOKIE_PROFILE_EXPORT_TEXT_MODE=unique|repeated`, `BOOKIE_PROFILE_REPETITIONS` or `BOOKIE_PROFILE_OUT` to adjust the bounded sample. The script caps the generated text payload at 256 MiB. These figures measure opening the dialog, not writing an export file.

## GTK test design

Prefer deterministic assertions about visible state, accessibility actions and stored data over wall-clock timing. Keep each scenario isolated from other test profiles and fixture databases.
