# Grid edit popover lifetime, 2026-10-27

Issue #1733, lane UX. A realized cell opens and closes enum, set, calendar, exact text, numeric and multiline edit popovers in sequence. The regression checks each popover's weak reference after closing. Original code fails on the first enum picker. Replacing descendant callbacks' strong popover captures with weak references makes all six variants release in the focused test.

The test uses isolated Xvfb, fatal GTK criticals and a simple input method. It does not access a database. App Clippy passed with warnings denied. All 177 ordinary isolated widget tests passed, including all six popover lifetimes. Full preflight passed (2,197 unit tests, 291 sandbox tests and one Redis cancellation test). Installed general edit acceptance passed as recorded below. Forgejo acceptance is deferred during infrastructure migration.

Release-Note: Fixed: Closing cell edit popovers releases their widgets and callbacks.

Three rebuilt installed SQLite scenarios passed: saving a grid edit persists to the database, reopening a browse tab preserves editability, and cancelling tab close preserves an active edit. These cover general edit workflows, not engine-specific enum/set popover behavior. [Validation record](validation.json) pins the binary hash.
