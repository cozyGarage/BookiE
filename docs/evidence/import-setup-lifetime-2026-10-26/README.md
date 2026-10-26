# Import setup lifetime, 2026-10-26

On Linux base `ac190e9b2`, import format controls capture the state that owns those controls and the source bytes. The Import button also captures its parent dialog. Both form reference cycles.

Two isolated GTK regressions fail before the fix: releasing format state leaves its weak reference live, and dismissing import setup retains the callback. Weak state and dialog captures make both tests pass with fatal GTK criticals enabled. The focused mechanism checks use the simple input method. The ordinary widget suite checks the normal environment separately.

The format callbacks remain active while setup is open because the Import button owns its state. Import submission still resolves the chosen format before closing setup and invoking its callback. Neither test accesses a database or a user file.

App Clippy passed with warnings denied. Three rebuilt optimized installed-app scenarios passed: importing refreshes the target table, failed import preserves unrelated edits, and importing refreshes every open view of the target table. Full preflight passed (2,197 unit tests, 291 sandbox tests and one Redis cancellation test), and all 177 isolated GTK widget tests passed in the ordinary suite environment. Five installed security scenarios also passed on the same binary: oversized SQL paste refusal, unavailable audit denial, audit loss after connection denial, and sanitized bundle export and import audit outcomes. Forgejo acceptance is pending. [Focused regression evidence](validation.json) records source hashes and before/after outcomes.

Release-Note: Fixed: Closing import setup releases its source data and callbacks.
