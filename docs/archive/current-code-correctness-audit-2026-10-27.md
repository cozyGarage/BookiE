# Current code correctness audit, October 27, 2026

## Source and method

Reviewed GitHub `linux` at `644c74dd34afb35f01c62254b458c12304890573`. The repository had 830 Rust files and 247,731 Rust lines under `crates/` and `apps/`. This is a source-pinned review of the paths below, supported by focused executions. It is not a line-by-line verification of every file or a release qualification.

The review compared production call paths, accepted decisions, active documentation, 161 open issues, 18 open PRs and 1,250 Project items captured at the start. It checked competing work before filing findings. Open-PR implementations and the paused SQL Server mask worktree were excluded from claims about merged Linux.

The [evidence packet](../evidence/current-code-audit-2026-10-27/README.md) retains diagnostic source, failed assertions, source hashes and tracking changes. Four diagnostic probes were temporarily added to actual workspace test targets, run, then removed from the active suite. They assert desired behavior and fail on the reviewed code. No product implementation was changed by this audit.

## Newly established defects

| Priority | Finding and actual result | Tracking |
| --- | --- | --- |
| P1 | A stale edit can overwrite a concurrent case-only change. The real SQLite `NOCASE` fixture received SQL from the production optimistic update builder. The old value was `Ada`, the concurrent value `ADA`; saving the stale draft affected one row and replaced it. A checked count of one cannot catch this. | [#1820](https://github.com/cozyGarage/BookiE/issues/1820), B3 |
| P2 | A failed workspace save can change the previously durable snapshot. With two persisted drafts, blocking the second destination makes flush fail and leaves metadata unchanged, but the first file already contains the new body. | [#1821](https://github.com/cozyGarage/BookiE/issues/1821), UX |
| P2 | One unreadable draft prevents restoring an unrelated requested connection. An invalid UTF-8 draft under A makes `prefetch_connections(&[B])` return `WriteFailed`, leaving healthy B uncached. | [#1822](https://github.com/cozyGarage/BookiE/issues/1822), UX |
| P2 | A late preload recaches a deleted connection. A deterministic interleaving loads a snapshot, calls the real `forget_connection`, then allows prefetch to apply that snapshot. The deleted workspace becomes readable from memory again. | Additional reproduction on [#1730](https://github.com/cozyGarage/BookiE/issues/1730), UX |

### Why the existing checks miss them

**Equality is not necessarily identity.** `crates/core/src/sql_dialect/updates.rs:179–220` uses database equality for original-value predicates. The comparison inherits the column's collation. SQLite reproduces the defect; other engines' collations, trailing-space behavior and type equivalence require separate tests. Do not infer that MySQL, SQL Server or PostgreSQL cases were executed. The original-value contract in ADR 0024 needs per-type faithful comparisons or explicit refusal where those comparisons are unavailable. Keep normal search/filter semantics separate from stale-write detection.

**An atomic file is not an atomic workspace.** `workspace_disk.rs:73–104` replaces draft bodies before publishing the reference metadata. A later draft or metadata failure can leave an acknowledged snapshot referencing changed bodies. Existing malformed-metadata and retry tests do not cover failure after an earlier draft has been replaced. Immutable staged draft versions, published by the metadata switch, are one candidate; the fix must preserve recovery and external-change behavior rather than merely suppress the error.

**Restore failure is too global.** `workspace_state.rs:326–333,365–383` loads the whole workspace before selecting requested IDs. `workspace_disk.rs:8–14` propagates the first connection's draft error. Preserve corrupt data, but isolate that failure from independently readable requested connections and surface which restore failed.

**Deletion has no preload tombstone.** `forget_connection` removes `memory.saved_at[id]` at lines 500–501. Prefetch checks only whether a save timestamp is newer than its starting clock at lines 585–602. Deletion therefore removes the evidence needed to reject a stale preload. The merged session save token addresses a different boundary; retain that fix and add deletion generation/tombstone coverage.

These are service and database reproductions. The audit did not reproduce them through an installed GTK session.

## Policy and evidence risks confirmed from source

### Configured policy can disappear into defaults

`apps/linux/app/src/services/database_service.rs:24–29` uses `Path::exists()` to decide whether a policy is absent. A dangling symlink returns false, so default policy is loaded successfully and `policy_available` stays true. The filesystem condition was executed; the installed GUI/MCP route was traced rather than run. [#1823](https://github.com/cozyGarage/BookiE/issues/1823) tracks this P1 configuration risk in UX with B4 consultation.

A separate, already documented branch handles readable malformed policy by marking it unavailable and disabling MCP/import, while local SQL still uses defaults (`with_audit`, lines 245–254). A present configured policy and an intentionally absent policy need different failure handling. The current default fallback must not be described as universal fail-closed configuration enforcement. Agentd has a separate stricter startup path.

### Passing evidence discovery omits recent records

The baseline default evidence scan succeeded with 165 schema-v1 manifests, four legacy provenance packets and 87 warnings requiring review. The command's success does not mean every warning is resolved or every evidence file was inspected.

Default discovery at `scripts/validate-evidence.py:234–244` searches only `manifest.json`. Twelve other schema-v1 records use `validation.json`; explicit strict validation identified 11 with no source/base commit. A broad filename glob also includes an unrelated array, which is not a supported evidence record. [#1824](https://github.com/cozyGarage/BookiE/issues/1824) owns schema-aware discovery and honest provenance recovery.

Do not update historical hashes to current bytes to turn a check green. Recover the actual tested commit where possible; otherwise retain the uncertainty. Missing logs, incomplete provenance, historical source drift and a reproduced product defect are distinct findings.

## Existing risks rechecked on merged source

These are already filed and are not new audit discoveries. The source paths still contain the relevant behavior; this review does not add a native reproduction for them.

| Concern | Current source and implication | Existing work |
| --- | --- | --- |
| Wrong-row mutations | ClickHouse metadata reports a non-unique MergeTree key as `primary_key`; the materializer's fallback uses that key, without checked single-row transactions. One displayed row can identify multiple physical rows. | [#1739](https://github.com/cozyGarage/BookiE/issues/1739) |
| Empty versus NULL | The production optimistic MySQL update builder binds empty text directly; the `SPACE(0)` special case exists in another builder. MariaDB `EMPTY_STRING_IS_NULL` needs the production path qualified. | [#1707](https://github.com/cozyGarage/BookiE/issues/1707) |
| False conflicts and filters | MySQL FLOAT and MSSQL real predicates lack the corresponding native-width treatment. PostgreSQL already has a real cast. Exact operators must be tested per type. | [#1708](https://github.com/cozyGarage/BookiE/issues/1708) |
| Uncertain writes | Bulk import errors can be audited with stronger failure/rollback claims than the driver outcome supports. Session blast-radius queries use the shared connection rather than the owned session. | [#1720](https://github.com/cozyGarage/BookiE/issues/1720), [#1726](https://github.com/cozyGarage/BookiE/issues/1726) |
| Agent confidentiality | Masking inspects top-level result names/known projection positions; nested JSON/BSON keys are not traversed. | [#1727](https://github.com/cozyGarage/BookiE/issues/1727), lineage separately [#600](https://github.com/cozyGarage/BookiE/issues/600) |
| Connection ownership | Any ping failure enters reconnect; replacement drops the prior tunnel owner. Pool exhaustion and transport loss are not distinguished by this monitor. | [#1735](https://github.com/cozyGarage/BookiE/issues/1735), [#1413](https://github.com/cozyGarage/BookiE/issues/1413) |
| Result completeness | MSSQL can resume admitting rows after budget refusal. Its repair is in open [#1766](https://github.com/cozyGarage/BookiE/pull/1766), outside the reviewed Linux tree. | [#1711](https://github.com/cozyGarage/BookiE/issues/1711) |

Other open PRs cover SQLite temporal storage, Redis escaping, ClickHouse invalid UTF-8, import type detection and ILIKE behavior. Their presence is evidence of work under review, not proof that Linux contains the fix. Consult the current PR and issue before starting another implementation.

## Architecture and verification assessment

The crate boundary guard passes. The source still follows the static-driver design, with GTK-free core, driver and app-services crates. Shared state transitions in app-services made the persistence failures reproducible without GTK. Keep that separation; the defects found here do not establish a need to replace message-driven UI architecture.

PolicyGuard remains the governed consumer seam in the reviewed composition paths. That seam does not prove the correctness of every predicate, outcome mapping or masking transformation inside it. Likewise, request generations cover many UI paths, but prefetch/deletion demonstrates that each state producer needs its own invalidation contract.

Recent Linux merges include row/popover/dialog lifecycle repairs. Old source-audit issue text must not be presented as evidence that all those original cycles remain. This audit did not repeat native lifetime acceptance. Merged PR #1819 still says its checks/gate are pending and calls itself not ready to merge; the mismatch is recorded on [#1543](https://github.com/cozyGarage/BookiE/issues/1543). It establishes stale review metadata, not that Forgejo was omitted. The actual gate and merged-tree proof need attaching before inferring completion from that prose.

The baseline non-GTK preflight result is recorded in the evidence packet. Full GTK, all driver integrations, TLS, packages, native Wayland and Forgejo were not run by this audit. Passing baseline tests can coexist with the four failing new probes because those edge cases were absent from the baseline suite.

## Documentation and Project corrections

- Replaced MongoDB's blanket last-write-wins description with its implemented field/row predicates and their limits.
- Added the reproduced stale-edit, workspace and policy-load limitations without claiming they are fixed.
- Corrected the user guide's apparent shared 64 MiB cache to a per-result cache; the 1 GiB encrypted spool quota is shared.
- Updated the architecture's spool description and clarified evidence discovery by filename.
- Replaced obsolete globally serialized gate guidance with the accepted worker-isolation rules, and corrected the backlog's claim that merged #1695 remains unmerged.
- Applied 34 missing Project Lane fields from existing issue labels and moved 11 fix-linked, non-draft PR issues to In review. Added existing #1413/#1419 and the five new findings to Project #3 with explicit lanes and Backlog status. No issue was closed from source inspection or an unverified hosted gate.

The worktree inventory found 36 checkouts. Outside this audit, the paused SQL Server mask branch had 93 modified files, and two other checkouts also had local changes. Several clean branches were ahead of their configured upstream. These are preservation/reconciliation candidates, not safe deletion candidates. No unrelated checkout or draft was changed.

## Recommended fix sequence

1. Protect data first: #1820 and #1739, then the MariaDB NULL boundary #1707. Exercise the real production materializer, native storage and checked save path, with a sibling row that must stay unchanged.
2. Resolve policy-load behavior #1823, uncertain bulk outcomes #1720 and agent masking #1727. Pin allowed and denied routes, nested values and terminal audit state.
3. Repair workspace publication #1821, restore isolation #1822 and the #1730 preload race. Keep failure, retry, deletion/reimport and external-edit cases in one shared persistence contract suite.
4. Fix evidence discovery/provenance #1824 and attach missing acceptance records. Improve the oracle and freshness checks rather than collecting another unqualified green badge.
5. Qualify per-type equality and result completeness, then revisit the remaining UX matrix. Use the existing issues and PRs; do not restart work already under review.

These are suggested correctness priorities, not an approved release scope. A follow-up fix closes its issue only after its observable acceptance and applicable gates are satisfied.
