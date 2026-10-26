# File chooser accessibility action race, 2026-10-26

Source base: `fd0926455`. This is a test-harness correction, with no application behavior change.

Two installed-app profiling attempts using the simple input method completed one oversized-file refusal, then failed with `No action with index 0` while invoking the next chooser's location action. The helper caught errors when obtaining the action interface, but not when enumerating or invoking its transient actions.

The regression supplies an action that disappears during invocation. On the original helper, both retry and keyboard-fallback cases fail with the injected exception. Moving enumeration and invocation inside the existing exception boundary lets the helper continue its existing bounded wait and keyboard fallback. All three focused cases pass, including the immediately available action.

The installed-app malformed-SQL refusal and oversized-restored-draft preservation scenarios pass on the editor-buffer fix binary. These exercise policy refusal and saved-text preservation; they do not establish complete security coverage.

Validation passed: 582 harness tests, preflight with 2,197 unit/bin tests, 291 sandbox tests and the Redis cancellation check. The same installed binary completed all 18 oversized-file refusals with the corrected helper, preserved the query, tabs and file, then exited normally. [The profiling packet](https://github.com/cozyGarage/BookiE/blob/fix/editor-buffer-release/docs/evidence/editor-buffer-release-2026-10-26/README.md) is published separately on the editor-buffer fix branch; this branch does not contain that unmerged application change.

Release-Note: none
