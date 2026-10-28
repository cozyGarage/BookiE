# Documentation lifecycle and decision lessons

Proposed; none of this is accepted. It records what an October 28, 2026 review of `docs/` found and asks for three small rules. Delete or reword any rule you do not want; each stands alone.

## What the review found

Measured at `linux` `12e76013a` by reading the tree, links and git history.

- **Active pages are mechanically sound.** Of the backticked repository paths in current pages, 4 no longer exist (`capability-review.md`, `release-process.md` placeholder, two in `testing.md`). Every active page is linked from another active page. Every archive page has an inbound link. No page is a safe deletion on link evidence alone.
- **Archive and evidence are large, not wrong.** 88 archive pages (about 294,000 words), 353 evidence packets and about 99 MB. [Evidence branch migration](evidence-branch.md) already covers moving raw artifacts; this proposal does not repeat it.
- **ADRs are used as status logs.** [ADR README](../decisions/README.md) says accepted decisions stay stable, yet ADR 0011 has 17 commits, 0006 has 15, 0018 has 12 and 0005 has 10. ADR 0011 carries an "Implementation status" section and a dated clarification beside the decision; ADRs 0025 and 0018 cite about eight PRs each. A reader cannot tell the decision from its progress.
- **One fact lives in several places.** The streamed-result limits are written in ADR 0011, ADR 0025, `known-limitations.md` (about 35 lines of profile numbers) and issues #1272 and #1032. When one changes the others drift.
- **Dated snapshots sit inside current pages.** Current pages contain about 625 date literals and 27 commit hashes. `value-contracts.md` alone has 184 dates. The "Proposed 0.2.1 release plan" in `bookie-0.2-sprint.md` states a commit and a commit count that were true on one day.
- **Closure without acceptance is the recurring mistake.** Twelve issues carry a reopened state. #1032 was reopened because its closure was "associated with the wrong PR"; #1140, #1142, #1145, #1146 and #790 were reopened by the 2026-10-26 audit because the merged PR did not cover the stated acceptance; #1175 was closed on #1259 while #1739 shows the defect remained. On 2026-10-27 I closed #555, #1540 and #1542 on a merged PR and had to reopen them: the docs still named #555 as the home of installed acceptance.

## Proposed rules

1. **An ADR records a decision, not its progress.** After acceptance only wording clarifications may change an ADR. Implementation status moves to the [decision checklist](../decisions/checklist.md) (one row per ADR) or the owning issue. Reversing a decision is a new ADR that names the one it supersedes. Moving the existing "Implementation status" sections out of ADRs 0011, 0006 and 0018 is a follow-up, not part of this proposal.
2. **One home per fact.** A limit, a number or a status is written once, in the page that owns it (ADR for the reason, `known-limitations.md` for user-visible behavior, the issue for acceptance). Other pages link; they do not copy. Dated measurements go to `docs/evidence/` with a source commit, and the current page cites the packet instead of quoting the numbers.
3. **Close on acceptance, and record the reversal.** Before closing an issue, search `docs/` for its number and read the issue's acceptance list; a merged PR is not acceptance. When a closure is reversed, add one line to a [lessons log](../decisions/lessons.md) with the issue, what was assumed, the evidence that showed otherwise and the rule that changed. The log is the place to look before repeating a decision.

## Deleting or archiving a page

A page may be deleted only if all of these hold, and archived (with a banner naming its replacement) otherwise:

- no active page links to it, or every link has been retargeted to a named replacement;
- no issue or pull request body cites its path (search them, as the evidence-branch proposal requires);
- no script, test manifest or evidence manifest reads it;
- its content is superseded by a page that exists today.

## Candidates this review found

None meets the deletion rule today. Worth a maintainer look:

- [Statement effects and joined verdict](policy-effects-and-verdict.md): the index row says its behavioral defects are closed and only #571 remains; likely archivable once #571 is decided.
- `docs/0.2-feature-comparison.md` ("Unselected feature candidates") overlaps the backlog's feature candidates and the [capability review](../capability-review.md).
- The "Proposed 0.2.1 release plan" snapshot in `bookie-0.2-sprint.md`, if the plan is accepted or dropped.

## Not proposed

No script or CI check. Rule 1 could be checked by flagging ADR edits after acceptance, and rule 2 by flagging commit hashes in current pages, but both would add preflight cost and false positives for a docs set this size; revisit if the problems return.
