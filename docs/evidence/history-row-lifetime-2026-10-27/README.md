# History row lifetime, 2026-10-27

Issue #1728, lane UX. A real temporary SQLite history store supplies one synthetic query to the History component. Clearing that store and refreshing removes the row from the list. Before the fix, its weak reference still upgrades. The Copy SQL action captures the row whose action group owns that callback.

A weak row capture makes the refresh regression pass. A second isolated check closes the populated component. Before shutdown cleanup, GTK warns that an ActionRow finalizes with a PopoverMenu still attached. Shutdown now unparents tracked popovers and removes a pending search debounce source. Query history is not deleted by closing the component.

Focused checks use isolated Xvfb and a simple input method. All 178 ordinary widget tests, full preflight (2,197 unit tests, 291 sandbox tests and one Redis cancellation test), and app Clippy passed. Two rebuilt installed-app scenarios passed: reopening history shows queries executed while it was closed, and opening or closing the docked panel preserves the workspace. The stronger shutdown assertion also passed after the full-tier run. Forgejo acceptance is deferred during infrastructure migration.

Release-Note: Fixed: Refreshing or closing query history releases its rows and popovers.

The shutdown regression retains a popover and checks that it is unparented after the component closes. Removing only the shutdown hook causes a GTK finalization warning and a GTK type assertion when inspecting the stale parent; restoring cleanup passes. [Validation record](validation.json) records the source hashes and limits.
