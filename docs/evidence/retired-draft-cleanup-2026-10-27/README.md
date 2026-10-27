# Retired SQL draft cleanup, 2026-10-27

Issue #1731, lane UX. Linux source base: `887678414`. This branch integrates #1757 at `f3e5f20e3`, including #1754. Land #1757 first to keep review scope focused.

Two regressions failed before the fix because closing a tab left its generated SQL draft on disk. The failed metadata publication preservation test pinned existing behavior. Final focused validation passes 53 workspace unit tests and four selected integration tests.

After a successful metadata publication, both save paths remove unreferenced regular files named `<UUID>.sql` from each affected connection directory. Per-connection saves clean only the updated connections. Retained references include other databases. Empty tab snapshots also clean retired drafts. Previously orphaned generated drafts in affected directories are removed, rather than accumulating indefinitely.

Cleanup does not follow symlinked connection directories or delete symlinked draft entries. Unrelated names and files belonging to another connection are preserved. A failed metadata publication never starts cleanup. Cleanup errors propagate to the workspace writer and its flush result; metadata can already be durable when cleanup fails, and a later save can retry cleanup. No draft format or UUID contract changes.

The tests use temporary directories and synthetic SQL. They check retained bytes, closed and orphaned drafts, other databases/connections, unrelated files, symlink boundaries and failure before metadata publication. No real user draft directories are scanned during validation. Disk unlinking removes the saved reference and file, and does not promise secure erasure or removal from external backups.

Full preflight and the installed close/reopen scenario are pending. Forgejo acceptance is deferred during infrastructure migration.

Release-Note: Fixed: Closing SQL editors removes their saved draft files after the workspace is saved.
