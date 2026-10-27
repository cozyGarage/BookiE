# Grid edit popover lifetime, 2026-10-27

Issue #1733, lane UX. A realized cell opens and closes enum, set, calendar, exact text, numeric and multiline edit popovers in sequence. The regression checks each popover's weak reference after closing. Original code fails on the first enum picker. Replacing descendant callbacks' strong popover captures with weak references makes all six variants release in the focused test.

The test uses isolated Xvfb, fatal GTK criticals and a simple input method. It does not access a database. Full ordinary widget validation, preflight, app Clippy and installed edit acceptance are pending. Forgejo acceptance is deferred during infrastructure migration.

Release-Note: Fixed: Closing cell edit popovers releases their widgets and callbacks.
