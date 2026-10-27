# History export outcomes, 2026-10-27

Issue #1736, lane UX. Source base: `887678414`.

The save-dialog callback wrote and synchronised the history file on the GTK thread. Export preparation and file write failures only produced log entries. The visible-failure regression failed before the fix: the log-only failure handler was extracted unchanged and the mounted history component presented no error dialog.

File writing now uses Relm4's blocking one-shot command. Its result returns to the component, which presents a dismissible failure dialog. Cancelling or dismissing the chooser performs no write and shows no error. Unexpected chooser failures and unsupported non-local destinations are reported. The existing atomic export writer remains responsible for publishing the destination.

Focused validation passed: the GTK failure-dialog regression and four tests for exact SQL/CSV bytes, failed publication preserving existing destination contents and cleaning temporary files, chooser cancellation, and local destinations. The tests use synthetic data and temporary files. Moving file I/O off the main context is verified by the blocking-command call path; no filesystem latency benchmark is claimed.

Full preflight passed: 2,208 unit tests, 291 sandbox tests and one Redis check. All 180 GTK widget tests and app Clippy passed. The rebuilt installed application passed `selected_history_exports_to_sql_and_csv` under isolated Xvfb. It waits for durable history, selects the row, uses the native save dialogs for both formats, verifies selected SQL and parses CSV to assert exactly one matching query. Earlier scenario attempts failed on persistence timing and menu selection assumptions; the final scenario selects menus from Home and verifies the resulting format. Forgejo acceptance is deferred during infrastructure migration.

This branch overlaps #1753 in history command handling. Land #1753 first, then refresh this branch and retain both regression sets.

Release-Note: Fixed: Query history export writes files without blocking the interface and reports export failures.
