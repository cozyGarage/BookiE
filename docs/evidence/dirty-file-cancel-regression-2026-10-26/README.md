# Dirty-file close cancellation regression, 2026-10-26

Related: #1333. This test-only change pins existing behavior. It does not fix or close the broader native freeze report.

The regular installed-app e2e suite now includes `cancelling_a_dirty_file_close_preserves_text_and_file_association`. It restores a disposable file-associated editor, changes its SQL and chooses Cancel when closing. The scenario requires the original file, tab and unsaved text to remain. It then closes again and chooses Save, requiring the same file to receive the changed text and the empty-state editor action to appear. The ordinary scenario registry assigns it to a GTK shard.

The scenario passed locally on the optimized editor-lifetime fix binary from `ab238b4acdacbcec941d11c82d7460c518685f45`. Four shard-selection tests and five UI-oracle harness tests passed. Preflight is recorded separately when complete. Native Wayland remains unverified.

Release-Note: none
