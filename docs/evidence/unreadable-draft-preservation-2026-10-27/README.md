# Unreadable draft preservation, 2026-10-27

Issue #1737, lane UX. Source base: `887678414`.

A draft read failure previously restored empty SQL and discarded the draft reference. Deleting an unrelated connection rewrote that empty state. A cold-cache database save also treated a failed draft read as an absent workspace and replaced its metadata.

Three regressions failed on the original code:

- `an_unreadable_draft_fails_both_load_paths_without_changing_files`: the full load incorrectly succeeded.
- `deleting_another_connection_does_not_read_or_replace_unreadable_drafts`: restoring the original file did not recover the referenced SQL.
- `saving_a_database_after_a_failed_draft_read_preserves_the_previous_workspace`: a database save replaced the existing metadata after a failed draft read.

Tests use invalid UTF-8 to produce a deterministic read error without depending on filesystem permissions or running as root. They use temporary files only. They verify metadata, original draft bytes and recovered SQL after repairing the file.

Draft loads now propagate errors other than missing files. The existing missing-file fallback remains covered. Connection deletion edits metadata without reading or rewriting any remaining drafts. A cold-cache database save logs the failed read and preserves the durable workspace. It can be retried after repairing the draft; no new persistence format is introduced.

Local workspace checks passed: 46 unit tests and four selected integration tests. Full preflight passed: 2,211 unit tests, 291 sandbox tests and one Redis check. A rebuilt installed app passed `restart_restores_active_editor_and_connection` and `workspace_connection_removal_confirms_once_and_preserves_the_database` under isolated Xvfb. These installed scenarios cover healthy persistence and deletion, not read-error UI presentation. Forgejo acceptance is deferred during infrastructure migration. No GTK source changed, and installed UI behavior on a failed draft read is unverified. This change does not add a recovery dialog or claim that the rejected new database snapshot was saved.

Integration: merged #1754’s branch (`c20839a63`) without rebasing. The combined deletion path holds one file lock, removes metadata without rewriting other drafts, and rejects late or already-dequeued saves for forgotten connections. Both regression sets pass together: 49 workspace unit tests and four selected integration tests. Combined full preflight passed: 2,214 unit tests, 291 sandbox tests and one Redis check. The previously recorded installed scenarios were not repeated for this integration; the combined service regressions and full preflight cover the merge resolution. #1754 may still land first to keep review scope small.

Release-Note: Fixed: Unreadable SQL drafts remain recoverable when connections are deleted or another database workspace is saved.
