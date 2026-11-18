# 0056: Forgejo validates first, then becomes the main remote

- **Status**: Accepted for stage 1 (2026-11-19); stages 2 and 3 need the maintainer's go
- **Date**: 2026-11-19

## Context

GitHub-hosted runners are slow and shared, and they have been the wait on every pull request. The Forgejo lab has stronger self-hosted runners, a broader suite (driver, GTK server, release fixtures, nightly) and `scripts/forgejo-gate.sh`, which pushes a `ci/` branch, dispatches `ci.yml` and waits for the result. Since 2026-11-18 Forgejo `main` equals GitHub `main`, and the maintainer merges on Forgejo.

Moving everything at once would break things that only GitHub provides today, so the move is staged. Each stage is reversible and ends with a check that says whether to continue.

## Decision

1. **Stage 1: Forgejo is the validating authority.** For a change to code, workflows or packages, the required result is the Forgejo run for the exact head, started with `bash scripts/forgejo-gate.sh <branch>`. GitHub runs stay in place and report, but a green GitHub run no longer substitutes for a red or missing Forgejo run. Documentation-only changes keep the local documentation checks.
2. **Stage 2: pull requests open on Forgejo.** Pull requests are opened and merged on Forgejo against its `main`; GitHub `main` becomes a push mirror of Forgejo `main`. This stage starts only when all of these hold:
   - the Forgejo `Forgejo regression gate` job is a required status check on a protected `main`;
   - a Forgejo push mirror to `cozyGarage/BookiE` is configured and has carried one merge;
   - `scripts/check-pr-readiness.py` and `scripts/check-ci-revision.py` read the Forgejo API instead of GitHub;
   - the CI GTK image (today `ghcr.io/cozygarage/bookie/ci-debian-testing-gtk`) is built and pulled from a registry the Forgejo runners reach, or the Forgejo jobs already do not depend on it.
3. **Stage 3: issues and the project board move.** The ledger index, `scripts/check-github-issue-status.py` and the lane rules in `AGENTS.md` move to Forgejo issues. GitHub becomes a read-only mirror used for releases and public visibility.

Until stage 2, agents still branch from and merge the latest GitHub `main` (which Forgejo `main` equals) and open pull requests on GitHub; they add the Forgejo gate result to the pull request.

## What stays on GitHub for now

| Dependency | Where | Stage that replaces it |
| --- | --- | --- |
| Pull requests, branch protection, merge | GitHub | 2 |
| `check-pr-readiness.py`, `check-ci-revision.py` | read the GitHub API | 2 |
| GTK CI image on GHCR, pushed by `build-linux.yml` | GitHub Actions | 2 |
| SonarCloud scans (reference, not a gate) | GitHub | 2, or dropped |
| Issues, Project #3, lane labels, `check-github-issue-status.py` | GitHub | 3 |
| Releases and packages | GitHub | decided with stage 3 |

## Consequences

- Runners are the Forgejo lab's, so a red job there is evidence about the product only after the runner health notes in `docs/testing-ci.md` are checked (disk reserve, concurrent fixtures, memory).
- Two CI definitions exist until stage 2 ends. A workflow change must keep both contract tests green (`scripts/tests/test_ci_workflow.py`).
- The agent rules change in stage 1 only in what counts as the required check, so no agent needs a new remote.

## Alternatives considered

- **Switch all at once:** loses branch protection, the GHCR image and the issue ledger in one step, with nothing to fall back to.
- **Keep GitHub as the gate and Forgejo as extra:** keeps the wait that this decision removes.
