# Documentation map

Read this page before loading long plans or evidence. Rules for agents and contributors are in the repository [AGENTS.md](../AGENTS.md). The completion page records milestone status and the backlog sets subsequent work order; accepted ADRs set technical rules. A dated result proves only its recorded source, environment and consumer.

## Minimum context for a task

1. [AGENTS.md](../AGENTS.md).
2. [Completion and continuation](bookie-0.2-sprint.md), then the selected [post-0.2.0 backlog](backlog.md) issue.
3. Your lane's issues on [Bookie Project #3](https://github.com/users/cozyGarage/projects/3) (filter by **Lane**). The [ledger index](known-issues.md) maps ledger IDs to issues.
4. The governing [ADR](decisions/README.md); for every type or value change, [ADR 0007](decisions/0007-type-and-value-preservation.md).
5. The affected checks in the [validation playbook](validation-playbook.md).

Do not load archives as mandatory context. Look up the heading, test selector or SHA you need, and recheck current source before trusting words such as "next", "current" or "pending" in a dated document.

## Documents by audience

| Audience | Documents |
|---|---|
| People using BookiE | [User guide](user-guide.md), [feature list](features.md), [platforms](platforms.md), [root README](../README.md) |
| Maintainers reviewing capability scope | [Capability review](capability-review.md) |
| Contributors writing code | [AGENTS.md](../AGENTS.md), [architecture](../ARCHITECTURE.md) (including [where new code goes](../ARCHITECTURE.md#where-new-code-goes) and the boundary checklist), [code conventions](code-conventions.md), [adding drivers](adding-drivers.md), [static driver capabilities](adding-drivers.md#static-driver-capabilities), [state management](state-management.md), [value contract rules](value-contract-rules.md), [connections](connections.md), [disconnection contracts](disconnection-contracts.md), [storage](storage.md), [error handling](error-handling.md) |
| Releases | [Release process](release-process.md): tags, packages, release notes |
| Contributors testing | [Validation playbook](validation-playbook.md) (including [CI tiers](validation-playbook.md#ci-tiers): GitHub checks and Forgejo acceptance), [testing](testing.md) ([GTK](testing-gtk.md), [CI](testing-ci.md), [quality and performance](testing-quality-and-performance.md)), [test design rules](test-design-rules.md), [performance and acceptance scenarios](performance-acceptance-scenarios.md), [test-oracle review](oracle-review.md), ignored-test inventory (`scripts/inventory-ignored-tests.py`, generated, not committed), [manual verification](manual-verification-0.2-features.md) |
| Planning and status | [0.2.0 completion and continuation](bookie-0.2-sprint.md), [backlog](backlog.md), [known limitations](known-limitations.md), [Project #3](https://github.com/users/cozyGarage/projects/3), [ledger index](known-issues.md), [B3 board pointer](lane-boards.md#b3-types-and-values), [B4 board pointer](lane-boards.md#b4-transport-and-sessions), [value evidence index](value-contracts.md), [changelog](../CHANGELOG.md) |
| Product context and specs | [PRODUCT.md](../PRODUCT.md), [user contexts](personas.md), [PRINCIPLES.md](../PRINCIPLES.md), [SECURITY.md](../SECURITY.md), [capability specs](../specs/README.md) (descriptive drafts until architect review; AGENTS and ADRs rank above them) |
| Decisions | [ADRs](decisions/README.md), [decision checklist](decisions/checklist.md), [proposal index](proposals/README.md) |
| Proof and history | [evidence](evidence/), [archive](archive/), [adopted-code attribution](upstream-sync.md) |

## Where each fact belongs

| Fact | Owner | Update rule |
| --- | --- | --- |
| A rule for every change | [AGENTS.md](../AGENTS.md) | One copy; tool-specific files point to it |
| Product constraints, users and outcomes | [PRODUCT.md](../PRODUCT.md), [personas](personas.md), [PRINCIPLES.md](../PRINCIPLES.md) | Keep aligned with shipped behaviour; product constraints are not implementation status |
| Current capability behaviour extracted from code | [specs/](../specs/README.md) | Descriptive only; when a spec and the code disagree, fix the spec |
| An architecture or behaviour decision | An [ADR](decisions/README.md) | Proposed, then Accepted by the maintainer; change a decision explicitly in a new or amended ADR; keep the [checklist](decisions/checklist.md) row in sync |
| Which forks already have an answer | The [decision checklist](decisions/checklist.md) | Link, N/A, or open item; do not invent a decision to fill a cell |
| A design not yet decided | [proposal index](proposals/README.md) | Becomes an accepted decision or is removed from active proposals |
| An open issue, its status and lane | [Project #3](https://github.com/users/cozyGarage/projects/3); ID map in the [ledger index](known-issues.md) | Status and Lane on the project; keep the index ID→issue→Lane table in sync; `scripts/check-known-issues.py` checks format offline, while `scripts/check-github-issue-status.py` reports live issue state |
| Milestone completion and subsequent order | [Completion and scope](bookie-0.2-sprint.md), [backlog](backlog.md) | Record maintainer completion separately from publication; keep issue acceptance on Project #3 and dated proof in the archive |
| B3 remaining work | Project #3 Lane B3; [B3 board pointer](lane-boards.md#b3-types-and-values) | Issues on the project; pointer keeps accepted refusals and the case record |
| B4 implementation and acceptance | Project #3 Lane B4; [B4 board pointer](lane-boards.md#b4-transport-and-sessions) | Issues on the project; acceptance checklists stay in the archived board |
| Type and consumer proof lookup | The [value evidence index](value-contracts.md) | Link to the case; detailed logs belong in evidence or history |
| Distros, toolchain, packages, Flathub, accessibility | [Platforms](platforms.md) | One toolkit floor ([ADR 0021](decisions/0021-toolkit-floor.md)) |
| Commands, layers, gates, handoff format | The [validation playbook](validation-playbook.md) | Reuse existing layers; say whether a result was local, Forgejo or installed |
| Crates, request pipeline, where new code goes, DAG, rename boundary, CI gate split | [Architecture](../ARCHITECTURE.md) (Mermaid map, trait boundaries, ownership table) | Match the manifests and [AGENTS.md](../AGENTS.md); cheap vs Forgejo detail stays in the [validation playbook](validation-playbook.md#ci-tiers) |
| A user-visible change | A `Release-Note:` line in the PR body and squash commit ([ADR 0022](decisions/0022-release-notes-from-merge-commits.md)) | One line of user impact, or `none`; no test or evidence notes. `CHANGELOG.md` is frozen until the next release cut |
| What a user can do and how | The [user guide](user-guide.md) | Update with the feature that changes it |
| What BookiE does, by task, with screenshots | The [feature list](features.md) | Update with the feature that changes it; screenshots follow the [brand guide](brand/README.md#screenshots) |
| Name, logo, colours and image files | The [brand guide](brand/README.md) | Change the guide and the files together |
| Run-specific proof (manifests, focused logs) | `evidence/<topic>[-<sha>]-<YYYY-MM-DD>/` | A `manifest.json` or concise `README.md`, with only the artifacts needed to check the claim; link it from the owning row or board |
| A finished review, audit or long history | [archive/](archive/) | Dated file name; corrections to links or neutral wording must preserve the recorded evidence, dates and decisions |

## Adding a document

- Look for the owner in the table above first; most new facts are a row, a section or an ADR, not a new file.
- A new reference document goes in `docs/` with a lowercase, hyphenated name and a line in the audience table above. Keep it under about 3,000 words and split by concern before it grows past that. A single-audience reference that is read top to bottom, such as the [user guide](user-guide.md) and the [validation playbook](validation-playbook.md), may run to about 4,000 words, because splitting it would scatter one path across files.
- Number ADRs sequentially (`decisions/NNNN-title.md`) and add them to the ADR index with their status.
- New evidence directories are named `<topic>[-<sha>]-<YYYY-MM-DD>` and hold only what a reviewer needs to check a claim. Use a manifest for structured evidence or a concise README for a focused local report. Keep large or raw logs in the PR or CI run. Keep current acceptance evidence at stable paths. Obsolete historical packets with no current test, code or acceptance consumer may move to `archive/evidence/` after all narrative links are updated; preserve their bytes and source identity.
- A packet whose source identity cannot be proven from retained runner records or reachable Git history carries `"source_status": "unpinned-historical"` and no `source_commit` or `base_commit`. It is historical context, not acceptance proof; do not infer a commit from its date, branch label or the current checkout. Replace the label with a source commit only when the exact source is independently verified. `scripts/validate-evidence.py` rejects the label on a packet that records a source identity. A packet whose recorded source hashes match no available snapshot carries `"source_status": "unverified-source"`: the recorded hashes stay exactly as written and are not overwritten with the current file's hash, and the packet does not prove the source files it names.
- When a document stops being current, move it to `archive/` with its date in the name, update links (`scripts/check-doc-links.py` finds the broken ones) and leave its open items on Project #3.
- Do not move or rename a current document casually: scripts, CI and other lanes link to these paths.

## Archive starting points

[archive/](archive/) holds dated reviews, audits, inventories and long histories. They are source-pinned evidence, not instructions; open items from them live on [Project #3](https://github.com/users/cozyGarage/projects/3). Use the archive starting points below to locate a record, then verify current behavior and status in active docs and issues.

- [Historical 0.2 development record](archive/bookie-0.2-history.md) (brief archive link; see the active sprint for current status)
- [B4 history](archive/b4-history.md) (includes superseded tip checkpoints, [completed local slices](archive/b4-history.md#archived-completed-local-slices-2026-10-10-consolidation) and [remaining-task prose](archive/b4-history.md#archived-remaining-task-prose-2026-10-10-pass-4))
- [Archived evidence packets](archive/evidence/) (dated artifacts retained after reference triage)
- [PostgreSQL session outcome regressions, October 20](evidence/postgres-session-outcomes-375253c-2026-10-20/README.md): audit, typed commit and GTK evidence for #1186 and #1187.
- [AppImage SQL highlighting, October 26](evidence/appimage-sql-highlighting-2026-10-26/README.md): a clean Debian runtime comparison, AppImage and GtkSourceView library hashes, and the missing language schema root cause.
- [PostgreSQL restricted-role catalog visibility, October 24](evidence/postgres-catalog-visibility-2026-10-24/README.md): the original isolated SQL probe and its metadata-visibility limits.
- [PostgreSQL driver coverage, October 24](evidence/postgres-driver-coverage-2026-10-24/README.md): the recovered integration-coverage report at source `d2166fd`.
- [Workspace unit coverage at `d2166fd`, October 24](evidence/workspace-unit-coverage-d2166fd-2026-10-24/README.md) and [at `583c244`](evidence/workspace-unit-coverage-583c244-2026-10-24/README.md): recovered historical reports for two distinct source revisions.
- [Workspace unit coverage, October 24](evidence/workspace-unit-coverage-9d61c7a-2026-10-24/README.md): 61.42% line coverage on Linux `9d61c7a` against the 59% floor, with the command and its scope.
- [Multi-editor stream ownership profile, October 26](evidence/stream-retention-multi-owner-2026-10-26/README.md): synthetic two-editor and cancelled export-page sample, with the RSS/allocator limitation stated.
- [SQL line-boundary profile, October 26](evidence/sql-line-current-linux-2026-10-26/README.md): three repeated 8 MiB file refusals at the recorded source, with memory samples and acceptance limits.
- [B3 board narrative](archive/type-contract-history.md#archived-b3-board-narrative-2026-10-09-consolidation-pass-2) and [value-contracts narrative](archive/value-contract-history.md#archived-value-contracts-narrative-2026-10-09-consolidation-pass-2) (2026-10-09 pass 2)
- [Third-party notice inventory, October 19](archive/third-party-notice-inventory-2026-10-19.md) (dated package inventory and generator requirements)

## Keep context bounded

Record a rule once in an ADR and a task once on Project #3. Do not copy old test counts into a current claim. Keep ignored `target/quality` paths as plain text, and when output is unavailable, say so instead of leaving a dead link.
