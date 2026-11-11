# 0053: Release notes come from changelog fragments

- **Status**: Accepted (2026-11-11)
- **Date**: 2026-11-11
- **Supersedes**: 0022

## Context

[ADR 0022](0022-release-notes-from-merge-commits.md) takes release notes from `Release-Note:` trailers in squash commits. That needs a pull request body, a body check and a release workflow, and a squash commit cannot be amended afterwards: assembling 0.2.6 and 0.2.7 dropped or rewrote commits with a missing trailer, an unknown category or an unfilled template line. The maintainer wants releases and changelog entries that do not depend on pull requests or Actions.

## Decision

1. A user-facing change adds one file `changelog.d/<slug>.<category>.md` in the same commit. `<category>` is one of `added`, `changed`, `deprecated`, `removed`, `fixed`, `security`. The file holds one line of user impact, with the wording rules of `AGENTS.md`. A change with no user impact adds no file.
2. Each change owns its own file, so concurrent changes do not conflict.
3. `scripts/release-notes.py` validates fragments (`check`), and at release assembles them into the `CHANGELOG.md` section and deletes them (`cut`). The release is cut locally and committed; no workflow assembles notes from history.
4. A change to a product the release does not ship, such as GPUI before the conditions in [ADR 0052](0052-gpui-is-the-cross-platform-frontend.md) hold, adds no fragment.
5. The `Release-Note:` trailer and its pull request check retire when the script and the preflight check for fragments land. Until that change merges, ADR 0022 stays in force.

## Rationale

A fragment is reviewed with the code, validated by preflight on the author's machine, and fixable at any time before release. It removes the dependence on how a pull request is merged.

## Alternatives considered

- Keep trailers and tighten the check: still depends on the merge box and cannot be corrected after merge.
- Edit `CHANGELOG.md` per change: the shared section conflicts, the reason ADR 0022 existed.

## Consequences

- Implementation (script, preflight check, `AGENTS.md`, `docs/release-process.md`, pull request template, removal of the body check) follows in its own change; this ADR does not change the current process by itself.
- A wrong fragment is fixed by a normal commit before the release is cut.
