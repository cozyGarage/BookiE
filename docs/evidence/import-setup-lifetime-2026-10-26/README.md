# Import setup lifetime, 2026-10-26

On Linux base `ac190e9b2`, import format controls capture the state that owns those controls and the source bytes. The Import button also captures its parent dialog. Both form reference cycles.

Two isolated GTK regressions fail before the fix: releasing format state leaves its weak reference live, and dismissing import setup retains the callback. Weak state and dialog captures make both tests pass with fatal GTK criticals enabled. The focused mechanism checks use the simple input method. The ordinary widget suite checks the normal environment separately.

The format callbacks remain active while setup is open because the Import button owns its state. Import submission still resolves the chosen format before closing setup and invoking its callback. Neither test accesses a database or a user file.

Full preflight, ordinary widget validation, app Clippy, installed import acceptance and Forgejo acceptance are pending.

Release-Note: Fixed: Closing import setup releases its source data and callbacks.
