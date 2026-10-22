# Resource acceptance train, October 22

The [pinned manifest](../../../ci/forgejo-train.json) combines the toolbar layout fix (#1303), deferred stream-grid construction (#1309), visible NUL preview fix (#1314), retained-byte budget (#1322) and bounded visible previews (#1323), in that order. All five two-parent merges applied without conflicts on Linux `c3b2a054e`. The manifest and first-parent merge order were verified with the aggregate verifier before publishing.

The byte budget and bounded previews are accepted as one candidate. Earlier profiling found lower RAM but higher CPU with the byte budget alone. The combined follow-up reduced both in the recorded wide-result workload; its [measurement limits](../bounded-preview-layout-2026-10-22/README.md) still apply. Full values, exact copy/export paths and policy checks are preserved. No allocator tuning or process-memory ceiling is introduced.

The constituent evidence records the failing regressions and individual checks. Combined preflight, GTK widget tests, GitHub checks, exact-head Forgejo acceptance and the post-merge tree comparison remain pending. The train must pass the aggregate verifier before gating and immediately before merging. No constituent is closed or accepted merely because it appears in this manifest.

Forgejo Linux sync 860 passed all 29 jobs at PostgreSQL fix commit `e7bd0e44cab881b71a25fc805f29ec3d6733e4c8`; its run identity, workflow and success were verified. This sync does not gate the resource train.

Remaining resource work includes longer pin/export/cancel lifetimes, keyless browse preview routing, parsed-JSON preview serialization and oversized editor lines. This train does not close the broader resource issues or establish long-duration leak freedom. Native visual checks remain separate.

## Inspector correction and rebuild

The first local candidate passed preflight and all 105 GTK widget tests, but app unit tests reported 717 passes and one failure: the row inspector’s stored sample was shortened through a shared formatter. It was not published or gated. The unchanged inspector regression now passes on the corrected preview constituent, along with all 716 app unit tests and app all-target Clippy. The original candidate remains on a local evidence branch. This train was rebuilt from the same Linux base with all five exact constituent heads and a corrected manifest. Combined checks must pass again before publication and gating.
