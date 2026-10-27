# Deleted connection drafts, 2026-10-27

Issue #1730, lane UX. A temporary workspace store saves an editor draft, forgets its connection, receives a late save and flushes. The original code recreates the deleted workspace and fails the regression.

The store now records forgotten connection IDs in its shared writer queue. Subsequent saves do not update cache or queue for those IDs. A writer batch filters forgotten IDs after taking the file lock. Forgetting holds that same lock through workspace and draft deletion, so a batch either writes before deletion or observes the deletion marker afterward. Pending sequence acknowledgements still complete normally even when every update is skipped.

A second deterministic test takes the file lock, waits for the writer to take a batch, marks that connection forgotten and releases the lock. The batch must not recreate state or drafts. This isolates the in-flight writer boundary; the first test exercises the public forget operation. No real connection, credentials or user files are involved.

The deletion marker lasts for the shared store lifetime. Persisting a deleted UUID again within that lifetime is intentionally ignored. Deletion of an active saved connection is not blocked by this change; its future persistence attempts cannot resurrect its saved drafts. Full local validation is pending. Forgejo is deferred during infrastructure migration.

Release-Note: Fixed: Deleted connection drafts stay removed when a late workspace save arrives.

Removing only the writer-side filter makes the in-flight batch test fail; restoring it passes. An additional regression confirms that cloned-store saves cannot revive the deleted connection and still update another connection correctly. All 39 workspace-state tests pass. Full preflight passed before the final test addition (2,199 unit tests, 291 sandbox tests and the Redis cancellation check); the expanded test head is being checked again.

Final expanded-head preflight passed: 2,200 unit tests, 291 sandbox tests and one Redis cancellation test, including formatting, Clippy and boundary checks. Forgejo acceptance remains deferred.
