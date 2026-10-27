# Workspace draft batch publication

Issue [#1821](https://github.com/cozyGarage/BookiE/issues/1821), based on [#1827](https://github.com/cozyGarage/BookiE/pull/1827) head `107c120e5`.

The public-API regression failed before the fix: a blocked second draft caused flush to fail, but the first previously acknowledged body changed from `old first` to `new first` while metadata remained unchanged.

Changed bodies are now written into new private draft files. Unchanged bodies retain their existing file identity. New files and their directory are synchronized before metadata publication. Staged files are removed if preparation fails. Before publishing metadata, cleanup ownership is relinquished so a metadata error after rename cannot delete a body that published metadata may already reference. Unreferenced staged versions from a failed metadata attempt are reclaimed on a later save using the previously committed references. Referenced draft files are never rewritten in place.

All 14 public persistence tests and 10 workspace disk tests passed. Coverage includes failure on the later draft, unchanged prior metadata/body, retry of the pending save, failed metadata publication for both full and incremental paths, cleanup of staged orphan versions on retry, unchanged-body identity, external changes, nested database drafts and symlink refusal. Size and whitespace guards passed. Clippy, full preflight and installed acceptance remain pending.

Metadata I/O errors can leave unreferenced versions for recovery until a subsequent save. A failed metadata operation is not proof that its rename did not occur. These tests establish filesystem-error behavior, not power-cut fault injection or a universal rollback guarantee for storage hardware failures.
