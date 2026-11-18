# CI checks

This page summarizes the contributor-facing CI contract. Test tiers and local commands are listed in the [validation playbook](validation-playbook.md#ci-tiers).

Required pull-request checks apply to the exact proposed commit. Passing a fast check does not certify test tiers it did not run. A skipped, pending or failed required check is not a pass. Maintainers confirm acceptance and handle merging.

CI results apply only to the source revision they tested. Keep fixtures and test inputs isolated, and include any needed test or workflow changes in the pull request.

## Nightly workflow

`.forgejo/workflows/nightly.yml` runs every night at 23:00 UTC (`schedule`) and can also be started with `workflow_dispatch`.

A first `plan` job runs `scripts/ci-nightly-plan.py` and every other job reads its result. The planner looks up the last green nightly through the Forgejo API (it needs `FORGEJO_TOKEN`; without it, or when the lookup fails, the nightly runs everything planned for the weekday) and skips the run when nothing changed since then or only Markdown, `docs/` or `changelog.d/` changed. Unit-tier coverage runs every night, Docker driver coverage and mutation shards rotate by weekday, and Sunday runs everything. Mutation for a crate with a baseline in `baselines/mutants/` is limited to its changed files.

- Edit the rotation in the tables at the top of `scripts/ci-nightly-plan.py`; `scripts/tests/test_ci_nightly_plan.py` fails when a driver or mutation shard is missing or repeated.
- Every matrix runs at most `MAX_PARALLEL` jobs (3) at a time. The workflow repeats the number as a literal, and `scripts/tests/test_ci_workflow.py` fails if the two differ. Jobs run in stages (coverage, Docker, mutation, then the Sunday extras), so the nightly holds at most three runners.
- Time budget, as the sum of each stage's job timeouts, which the planner checks: weeknights 7 hours, Sunday 16 hours. A new nightly cancels the one still running for the same ref.
- Runner use: the nightly may hold at most 7 of the 15 runners (50%) at once. Its jobs use `runs-on: ${{ vars.NIGHTLY_LABEL || 'any-host' }}`, so they run on any `any-host` runner until the variable is set. The widest stage is 5 jobs, and a contract test fails if the stages sum to more than 7.
- To confine the nightly to a reserved set: label 7 runners `nightly` in the Forgejo runner settings, then set the repository variable `NIGHTLY_LABEL` to `nightly`. Set the variable only after the runners carry the label, because a job that asks for a label no runner has waits and is cancelled.
- Dispatch with `full` to run everything, or with `plan_only` to print the plan in the `plan` job log and start nothing else.

## Forgejo is the validating remote

[ADR 0056](decisions/0056-forgejo-validates-first.md) makes the Forgejo run the required check for code, workflow and package changes. Start it with `bash scripts/forgejo-gate.sh <branch>`: it pushes a `ci/` branch to the `forgejo` remote, dispatches `ci.yml` and waits for the result. Match a run to a pull request by the branch name and commit it reports, not by the run number alone.

Before blaming product code for a red Forgejo job, check the runner: the free-space reserve, other fixtures sharing the Docker host (the PostgreSQL release fixture names its compose project after the run), and memory pressure when many database containers start together.
