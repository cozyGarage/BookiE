# 0022: Release notes come from merge commits, not a tracked changelog

- **Status**: Accepted
- **Date**: 2026-10-24
- **Implementation status**: see [implementation-status.md](implementation-status.md#adr-0022-release-notes-come-from-merge-commits)
- **Supersedes**: the shared `CHANGELOG.md` release-note workflow.
- **Clarified**: 2026-10-25 for implementation status. The pull-request body validation workflow has landed; the release workflow still reads `CHANGELOG.md`.

## Context

Every pull request edited the `[Unreleased]` section of `CHANGELOG.md`. Concurrent pull requests conflicted in that shared section.

Release notes are release output. A file that every change rewrites puts generated-looking content in the source tree and makes concurrent changes harder to reconcile.

## Decision

The release note of a change is a git trailer in its squash-merge commit message:

```
Release-Note: <Category>: <one line of user impact>
Release-Note: none
```

1. Categories are the Keep a Changelog 1.1.0 headings: Added, Changed, Deprecated, Removed, Fixed, Security. A change may carry several lines. `none` is explicit and cannot be combined with other lines. The wording rules of AGENTS.md are unchanged: user impact, no paths, type or function names, no `Fixed` for a defect introduced and fixed before release.
2. The pull request body carries the same lines, so a check can validate them before merge. The repository's squash-merge defaults are set to the PR title and the PR body (`squash_merge_commit_title=PR_TITLE`, `squash_merge_commit_message=PR_BODY`, changed on 2026-10-24 from `COMMIT_OR_PR_TITLE` and `COMMIT_MESSAGES`), so every merge carries the lines without anyone remembering to copy them. A merger who wants a shorter body can pass only the lines: `gh pr view N --json body -q .body | python3 scripts/release-notes.py trailers - > body.txt`, then `gh pr merge N --squash --body-file body.txt`.
3. `scripts/release-notes.py check` validates a body or message. `scripts/release-notes.py assemble --since <previous tag>` builds the release notes from the commit range, refuses a commit with no line unless told otherwise, and appends the pull request number from the squash subject. The output is a build artifact and the body of the GitHub Release.
4. `CHANGELOG.md` stops growing. Its `[Unreleased]` entries stay until the first release cut after this decision. That release's notes are those entries plus the `Release-Note:` lines of the pull requests merged after the guard landed in `669635a06`, combined by hand into the version's `CHANGELOG.md` section (`assemble --since 669635a06`). Until the Release workflow reads trailers it still reads that section, so `CHANGELOG.md` must exist when the tag is pushed. After that release is published, a follow-up change copies the whole file unchanged to `docs/archive/changelog-through-<version>-<date>.md`, removes `CHANGELOG.md` and repoints its references. The 0.1.x record is already archived.

## Rationale

The squash commit is the unit that reaches `linux` and the common merge record. It cannot conflict, adds no per-PR file, and keeps the note next to the change it describes. A required line, even `none`, replaces the unenforced habit of remembering to edit the changelog.

## Consequences

Accepted:

- The whole PR description becomes the squash commit body, so the description must be written to stand in history. The check on the pull request body is what catches a missing line.
- A merger who overrides the default message in the GitHub merge box can drop the lines; the assembler reports such a commit as missing.
- Notes cannot be corrected by a later pull request; a wrong one is fixed in the release draft before publishing.
- AGENTS.md's "single-line subject and no body" no longer describes squash commits, whose body is the PR description. Branch commits keep the single-line rule.

Gained:

- No more changelog merge conflicts and no release-note content in source review diffs.
- A release can be previewed from any range with one command.

## Alternatives considered

- **Keep `CHANGELOG.md` as it is.** Rejected: the conflict rate above.
- **Towncrier or other fragment files (`changelog.d/<pr>.md`).** Rejected: removes conflicts but still adds a file to the repository for every change, which is the content this decision takes out of the tree, and adds a pinned generator with fragment naming rules.
- **Derive notes from Conventional Commit subjects alone.** Rejected: subjects describe the change to developers. AGENTS.md requires user-impact wording, and many commits (CI, tests, refactors) have no user impact.
- **A PR-body block read through the GitHub API at release time.** Rejected as the source of record: it needs network access and tokens at release time, and the text can be edited after merge. The same lines are in the PR body for validation only.
- **Leave the squash-message setting alone and rely on the merger to copy the lines.** Rejected: the rule would live only in AGENTS.md.
- **GitHub's generated release notes.** Rejected: they list titles and authors, not user impact.
