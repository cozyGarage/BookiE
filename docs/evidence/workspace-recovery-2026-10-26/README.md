# Workspace recovery, October 26, 2026

## Source and scope

The audit inventoried 16 original worktrees, 130 local branches, 50 GitHub branch tips, 138 Forgejo branch tips and six stashes. It read all 1,274 GitHub PR records available at the inventory, including the heads of all 1,215 merged PRs. A closed PR, a missing commit ID after squash, and accepted current source were evaluated separately.

The recovered application code was tested at `33529f6f9`. GitHub Linux `9d82a5ebef7455d8ff28bb3289ffcab144947f7c` was subsequently merged into recovery head `8fca5cc9cbc59bd9fd79fb296703ea15e2c9b13e`. The application, crate, script, workflow and Cargo trees were compared against the tested head and were unchanged by this final baseline sync; only documentation changed.

## Recovered work

- Import and clipboard processing from the original B3 and UX branches. The shared file reader rejects non-regular inputs and bounds reads even when a file grows. Two failing-first regressions corrected the recovered bounded-read helper and retained space for the XLSX header at the million-data-row boundary. Clipboard size checking limits subsequent paste parsing; GTK's text transfer itself is still obtained through its existing text API.
- MCP authentication throttling before token-store locking for unknown guesses, while preserving valid-token access after the failed-attempt budget is spent and recognizing tokens issued by another process.
- The abandoned CI scheduling edit, adapted to the current workflow. Integration jobs can overlap unit and sandbox tests; GTK artifact consumers still depend on the producer. All tiers remain mandatory. No faster runtime is claimed before complete fleet measurements.
- Three orphaned evidence worktrees. Distinct historical coverage reports retain their original revisions and explicitly lack independently reproduced machine-readable output. The restricted-role catalog SQL was reproduced in an isolated, network-disabled PostgreSQL container with the original image digest; all four expected results matched and the container was removed.
- PR [#1686](https://github.com/cozyGarage/BookiE/pull/1686) was pinned at `1486937e50b500f50cc9d64231f1c2a7ff72195c` for combined testing. It subsequently landed independently on Linux; the recovery baseline sync preserves the current upstream documentation.

## Validation

On the combined tested code:

- Script harness: 576 tests passed.
- Preflight: formatting, size and boundary guards, Clippy, 2,197 unit/bin tests, 291 sandbox tests and the Redis cancellation test passed. The existing configurations skipped 16 unit and 50 sandbox tests.
- All 174 isolated GTK widget tests passed.
- `cargo deny check`: advisories, bans, licenses and sources passed.
- Focused checks passed: 124 import tests, 67 MCP tests and 11 connection-form tests.
- The final documentation-only baseline sync passed documentation links, issue-ledger and source-tree comparison checks.

The evidence validator reported 70 historical/source-drift warnings. These include missing old logs and outdated provenance; this audit does not turn those packets into current acceptance evidence.

## Preservation and cleanup

The GitHub branch [archive/workspace-before-recovery-20261026](https://github.com/cozyGarage/BookiE/tree/archive/workspace-before-recovery-20261026) preserves 318 original local/remote branch tips through the parent graph of archive commit `aa57080772f05015b302e2f8c9b22cc995c688d9`. Its inventory distinguishes original refs from the archive's source tree. It is a history archive, not a feature-integration branch, and must not be merged into Linux.

Retired refs were first backed up locally and then verified as ancestors of the published archive. Remote deletions used exact expected-head leases. All 14 inactive original worktrees were removed only after checking for newer changes, non-build ignored files and mounts used by running containers. The active tab-close checkout was preserved; the subsequently created SQL-profile checkout is outside the original inventory and was also preserved.

Six private stashes, original uncommitted patches and a complete Git bundle remain in the local recovery backup. Stash contents were not published. Superseded experiments, canaries and stale train manifests remain recoverable in the archive rather than being restored as current product behavior.

## Acceptance boundary

The recovery branch is published to GitHub. Local validation and archival preservation do not establish full merge-tier acceptance. Existing Forgejo runs #985 and #986 were still running/queued after their associated GitHub PRs had landed. A serialized watcher waits for those runs to drain before invoking the repository gate for the recovery branch. No existing run was cancelled, and no recovery code was pushed directly to either Linux branch.

The remaining native tab-close evidence belongs to its active agent. The SQL-profile checkout and any later work are outside the frozen original inventory.
