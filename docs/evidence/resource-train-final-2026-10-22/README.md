# Refreshed resource train

This train replaces #1351 after constituent branches #1323 and #1335 merged
Linux to resolve integration conflicts. The pinned members and base are in
`ci/forgejo-train.json`. All five merges were clean in the aggregate.

The complete `apps`, `crates` and `scripts` trees match the previously locally
validated aggregate `8a5cb31c0d0b98a962b1061a61d7597241da1fa7`. Its local
preflight, 728 app unit tests, Clippy and 105 GTK widget results remain
source-matched. See [previous validation](../resource-train-next-2026-10-22/README.md).
The new manifest and documentation require fresh GitHub checks, followed by
a Forgejo gate of this exact aggregate. Acceptance is pending.
