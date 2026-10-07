# Documentation entry point

Read this page before loading long plans or evidence ledgers. The active sprint
sets work order; accepted ADRs set technical rules. A dated result proves only
its recorded source, environment and consumer.

## Minimum agent context

1. Repository `CLAUDE.md` and applicable `AGENTS.md` instructions.
2. [Active sprint](bookie-0.2-sprint.md): choose one milestone/task.
3. The relevant [ADR](decisions/README.md); for every type/value change read
   [ADR 0007](decisions/0007-type-and-value-preservation.md).
4. The owning task/case and affected checks in the [validation playbook](validation-playbook.md).

Do not load every archive as mandatory context. Look up the specific heading,
test selector and SHA needed for the task. Recheck current source before relying
on historical statements such as “next”, “current” or “pending”.

## Where each fact belongs

| Fact | Owner | Update rule |
| --- | --- | --- |
| Type/value semantics and proof standard | [ADR 0007](decisions/0007-type-and-value-preservation.md) | Change a decision explicitly; link it from plans |
| Connection/session/trust semantics | [ADR 0008](decisions/0008-connection-and-session-ownership.md) | B4 task IDs track implementation, not competing rules |
| Durable identity and migration semantics | [ADR 0009](decisions/0009-persistence-and-identity-compatibility.md) | Operational procedures stay in storage/state guides |
| Every known open issue outside B3, with its status, evidence and owner | [Known issues ledger](known-issues.md) (owners in [Owners and handoff](known-issues.md#owners-and-handoff)) | Cross out a row only with the commit or test that closed it; `scripts/check-known-issues.py` enforces the format |
| Milestones, order and acceptance | [Active sprint](bookie-0.2-sprint.md) | Keep current instructions compact; archive dated progress |
| B3 remaining work | [Type/consumer board](type-contract-strategy.md) | One bounded case, outcome, evidence pointer and next action |
| B4 implementation packets | [B4 board](b4-task-board.md) | Update the owning task and integrated status |
| UI-1b SSH jump-chain editor | [Per-hop credential proposal](proposals/ui-1b-ssh-jump-chain-editor.md) | Proposal only; retain edit refusal until storage, transport, bundle, and GTK work land together |
| Type/consumer proof lookup | [Value evidence index](value-contracts.md) | Link to the case; detailed logs/counts belong in history/evidence |
| Supported distros, Rust toolchain, Arch package, Flathub, accessibility | [Platforms](platforms.md) | Keep one section per concern; the build floor is stated here |
| Commands, execution ownership, handoff format | [Validation playbook](validation-playbook.md) | Reuse existing layers; distinguish local/hosted/installed |
| SonarCloud findings | [SonarCloud triage](validation-playbook.md#sonarcloud-triage) | Confirm framework-specific findings against the native platform before changing source |
| Crate/source structure | [Architecture](../ARCHITECTURE.md) | Match actual manifests and owning wrappers |
| Active milestone / short status | [Sprint](bookie-0.2-sprint.md) / [ROADMAP](../ROADMAP.md) | Sprint owns acceptance; roadmap links to current boards |

## Archive

[docs/archive](archive/) holds the dated reviews, audits, inventories and long
histories (sprint, B3, B4, connections, validation, value contracts). They are
source-pinned evidence, not current instructions: open items from them live in
the [ledger](known-issues.md). Use the
[changelog engineering history index](../CHANGELOG.md#historical-engineering-records)
to locate a dated record, and the current boards and linked PR discussions or
CI artifacts for active decisions and case proof. Useful starting points:

- [Release audit, October 3](archive/release-audit-2026-10-03.md): a dated broad source audit pinned to its then-current baseline; use the live ledger and boards for current status.
- [Architecture review](archive/architecture-consistency-review-2026-10-03.md): source risks and unavailable raw proof.
- [App layer audit](archive/app-layer-external-audit-2026-10-06.md): GUI gaps and the TablePro and dbx UI comparison.
- [Grid memory and fetch action plan](archive/grid-memory-and-fetch-action-plan-2026-10-07.md): scroll profile, weak-cache implementation and measured results; keep query paging and ODBC scoped separately.
- [Upstream main](archive/upstream-main-review-2026-10-03.md) and [older releases](archive/upstream-older-releases-review-2026-10-03.md): candidate fixes.

## Keep context bounded

Record a rule once in an ADR and a task once on its board. Put run-specific
manifests and logs in the PR discussion or CI artifacts, then link them from the
owning board and changelog; do not add per-run evidence directories to source.
Do not copy old test counts into a current acceptance claim. Keep ignored
`target/quality` paths as plain text; when output is unavailable, say so instead
of leaving a dead link.
