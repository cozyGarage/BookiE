# GTK tests

Part of [testing](testing.md), which holds the test tiers, runner and fixtures.

## GTK tests

Run the installed safety suite with:

```bash
./scripts/test-gtk-safety.sh
```

The script builds `tablepro-app`, always starts a private runtime directory, isolated D-Bus session and Xvfb display, and drives the real application through PyAT-SPI. Each scenario gets temporary XDG directories and a fixture SQLite database. A scenario may mark its saved fixture connection as production to test policy prompts; it never opens a user's saved connection or production database. The runner uses a standalone AT-SPI D-Bus daemon and passes the Xvfb display to D-Bus activation, so it does not reuse desktop accessibility or portal processes. GTK tests exercise the X11/GTK portal path; they do not replace installed Wayland package testing.

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

The XML scenarios live in `apps/linux/app/tests/gtk_xml.py`. Run `xml_export_preserves_line_endings_and_null_distinctions` or `xml_export_refuses_illegal_text_without_publishing` with `TABLEPRO_GTK_SCENARIO`. Their Python oracle tests deliberately normalize carriage returns and collapse empty text into NULL, requiring both corruptions to fail. Core tests independently check atomic destination preservation for XML-illegal controls and U+FFFE/U+FFFF.

The parameter scenarios live in `apps/linux/app/tests/gtk_parameters.py`; select `repeated_parameters_preserve_wide_ids_and_sql_like_text` or `cancelled_parameters_never_execute_and_retry_uses_new_values` through `TABLEPRO_GTK_SCENARIO` to run one. Registration tests require both in the default suite. Each checks committed SQLite rows independently of the visible UI result.

The workbook scenarios live in `apps/linux/app/tests/gtk_workbook.py` and are registered in the default suite. Their XML oracle has separate negative tests that reject rounded values, wrong types, lost precision, formulas and incorrect page boundaries. SQLite supplies typed Date/Time/DateTime for these UI scenarios; typed TimestampTz UI coverage against a network server remains separate from the core and PostgreSQL contracts. The cancellation UI test stops at the chooser; a core regression also cancels after writing a row and checks atomic destination preservation.

To run one scenario using an already-built candidate (without another Cargo build):

```bash
TABLEPRO_GTK_BINARY=/absolute/path/to/bookie \
TABLEPRO_GTK_SCENARIO=current_page_workbook_preserves_typed_values \
TABLEPRO_GTK_ARTIFACT_DIR=target/gtk-evidence \
bash scripts/test-gtk-safety.sh
```

Use `cancelled_workbook_export_preserves_existing_file` for the cancellation case. Record the binary's commit/checksum; a prebuilt binary is not evidence for later Rust edits. Omit `TABLEPRO_GTK_BINARY` to build the current release profile.

Each scenario creates a temporary profile and fixture connection/database. Its `environment` attribute selects the policy environment to exercise, and `audit_available` controls whether the fixture audit journal is present. These tests do not use existing saved connections or a user's production database.

Memory profiles and second-launch checks retain the exact subprocess launched by the fixture. They verify its PID lifetime and executable file identity rather than scanning process names. Second launch reuses the supplied fixture environment; it never reads another process's environment. Profile rows record the owned PID and Linux process start ticks. An exited or replaced target fails the check instead of selecting another application.

The Open Quickly scenario waits for the filtered result set before invoking its single action; the already-visible favorite is not proof that the debounced row rebuild has completed. It still requires the window to close and usage to be persisted.

Buttons and rows are invoked through named AT-SPI actions; the harness re-queries the row while waiting up to 15 seconds for its action to appear in the accessibility tree. There is no generic Return-key fallback. Keyboard events exercise shortcuts and the export format combo's navigation. The combo has no AT-SPI click/focus action: its scenario tabs until the named control reports focus, opens its list, navigates to the requested format, and verifies its selected label before exporting. Each denial assertion requires the row count to hold for a settle window rather than matching once.

On Arch or Omarchy, install the local harness dependencies with:

```bash
sudo pacman -S --needed dbus xorg-server-xvfb at-spi2-core python-atspi
```

Service-level tests cover pure state and policy behavior, but they do not replace cross-layer GTK and native desktop checks.

SQLite integration regressions also distinguish NULL from zero, false, empty text, and empty blobs in declared columns, and preserve binary values despite declared-type affinity mismatches. Export unit tests overlap writers deterministically and check that pre-existing temporary symlinks cannot redirect writes. Workspace tests preserve active-tab identity when unknown records are removed.

For ordinary UI changes, test the affected flow manually and include before and after screenshots. Add deterministic automation when a regression can be reproduced without timing or desktop-session assumptions.

## GTK test design

Prefer deterministic assertions about visible state, accessibility actions and stored data over wall-clock timing. Keep each scenario isolated from other test profiles and fixture databases.
