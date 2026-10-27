# Workspace draft batch publication

Issue [#1821](https://github.com/cozyGarage/BookiE/issues/1821), based on [#1827](https://github.com/cozyGarage/BookiE/pull/1827) head `107c120e5`.

The public-API regression failed before the fix: a blocked second draft caused flush to fail, but the first previously acknowledged body changed from `old first` to `new first` while metadata remained unchanged.

Changed bodies are now written into new private draft files. Unchanged bodies retain their existing file identity. New files and their directory are synchronized before metadata publication. Staged files are removed if preparation fails. Before publishing metadata, cleanup ownership is relinquished so a metadata error after rename cannot delete a body that published metadata may already reference. Unreferenced staged versions from a failed metadata attempt are reclaimed on a later save using the previously committed references. Referenced draft files are never rewritten in place.

All 14 public persistence tests and 11 workspace disk tests passed. Coverage includes failure on the later draft, unchanged prior metadata/body, retry of the pending save, failed metadata publication for both full and incremental paths, cleanup of staged orphan versions on retry, unchanged-body identity, external changes, nested database drafts and symlink refusal. The body comparison uses a fixed 8 KiB buffer and tests exact contents across buffer boundaries, empty bodies and differing lengths.

The final source at `2169db0c4` passed five rebuilt installed scenarios without retries: restart restoration, save-and-close of a file tab, reimported connection persistence, preservation of an unreadable draft while deleting another connection, and restoration of a healthy connection alongside an unreadable draft. The prerequisite merge at `8234d5b82` has an identical tree, verified with `git diff --exit-code 2169db0c4 8234d5b82`. The installed scenario durations were 27.438, 8.522, 21.829, 8.587 and 7.094 seconds respectively. These are acceptance durations under concurrent local builds, not performance benchmarks.

Full preflight passed: Clippy, 2,231 unit tests, 296 sandbox tests and the isolated Redis cancellation test. Size, whitespace and documentation guards passed. Forgejo acceptance and native Wayland checks remain unverified.

Metadata I/O errors can leave unreferenced versions for recovery until a subsequent save. A failed metadata operation is not proof that its rename did not occur. These tests establish filesystem-error behavior, not power-cut fault injection or a universal rollback guarantee for storage hardware failures.
