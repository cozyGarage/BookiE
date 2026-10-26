# Export setup lifetime, 2026-10-26

Related: #1032, lane UX. This isolates export setup ownership; it does not establish an aggregate process-memory ceiling.

On Linux base `ac190e9b2`, the export setup dialog owns its Export button, while the button's clicked callback holds a strong dialog reference and the result. The table-page scope selector also holds the Export button through a selected-notify callback, while the button's callback holds that selector.

The new isolated GTK regression presents export setup with an 8 MiB synthetic byte value, dismisses it and pumps the main context. Before the fix it fails because the closed dialog remains alive. The fix uses weak references for the dialog capture and the scope selector's button capture. The test now requires both the dialog and Export button to finalize, releasing the callback that owns the result. It passed locally with fatal GTK criticals enabled. The focused mechanism check selects the simple input method to exclude unrelated input-context retention; full GTK-tier validation uses the ordinary suite environment.

The test is registered in `scripts/isolated-tests.json`. Full preflight, widget-suite validation and Forgejo acceptance are pending. No database or user file is read or written by this synthetic test.

Release-Note: Fixed: Dismissing export setup releases retained result data.
