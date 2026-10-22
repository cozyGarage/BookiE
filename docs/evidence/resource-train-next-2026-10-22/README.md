# Resource acceptance train after the tab-close fix, 2026-10-22

Base: `09a600d5fa078dc9c0984a4aa6d6e59f29034469`. The toolbar and Discard-close fixes are already on this base. Exact constituent heads and order are in [the train manifest](../../../ci/forgejo-train.json).

1. #1309 defers streamed grid construction until the first frame.
2. #1314 preserves embedded NUL markers in visible previews.
3. #1322 bounds retained streamed pages by estimated bytes.
4. #1323 bounds large-cell layout previews while preserving inspector and exact copy/export values.
5. #1335 bounds JSON preview serialization memory.

The first preparation attempt stopped on documentation and isolated-registration conflicts. Those conflicts were resolved in the owning #1309 and #1314 branches, with current evidence and all required test registrations preserved. The train then integrated the five refreshed exact heads without conflicts. No production conflict resolution was performed inside the train.

Full aggregate preflight, all 728 app unit tests, app all-target Clippy and all 105 registered GTK widget tests passed. Documentation links, the issue ledger and ignored-test inventory pass. Required GitHub checks and the exact-head Forgejo gate remain pending. The earlier #863 reporting outage is preserved in [its evidence](../resource-gate-863-2026-10-22/README.md); that failed run does not accept this train. #1342 is the obsolete draft predecessor.

Editor callback lifetime, query publication batching and workspace-save copies are prepared separately because they need their own integration checks. Connection rows and single-confirmation removal form a separate UI batch. Issues and constituent PRs remain open until the accepted aggregate is merged and its resulting tree verified.
