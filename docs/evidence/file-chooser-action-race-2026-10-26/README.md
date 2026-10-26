# File chooser accessibility action race, 2026-10-26

Source base: `fd0926455`. This is a test-harness correction, with no application behavior change.

Two installed-app profiling attempts using the simple input method completed one oversized-file refusal, then failed with `No action with index 0` while invoking the next chooser's location action. The helper caught errors when obtaining the action interface, but not when enumerating or invoking its transient actions.

The regression supplies an action that disappears during invocation. On the original helper, both retry and keyboard-fallback cases fail with the injected exception. Moving enumeration and invocation inside the existing exception boundary lets the helper continue its existing bounded wait and keyboard fallback. All three focused cases pass, including the immediately available action.

The installed-app malformed-SQL refusal and oversized-restored-draft preservation scenarios pass on the editor-buffer fix binary. These exercise policy refusal and saved-text preservation; they do not establish complete security coverage.

Release-Note: none
