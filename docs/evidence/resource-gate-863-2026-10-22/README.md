# Resource gate 863, 2026-10-22

## Observed state

The exact tested head is recorded in the [manifest](manifest.json). At inspection, 23 jobs passed, four were marked failed and two remained running. The four failed jobs were PostgreSQL E2E, GTK shards 2 and 3, and the PostgreSQL release fixture.

The downloaded logs contain no terminal test failure, assertion, compiler error or runner exit reason. All four stop abruptly between 01:14:08 and 01:14:19 UTC. PostgreSQL E2E stops while compiling. The release fixture shows successful test groups, then stops during an additional compilation. Both GTK logs show successful scenarios and ordinary AT-SPI cache warnings before stopping.

The clustered interruption suggests a runner or execution-service interruption, but it does not establish one. The cause remains unresolved. No job has been retried and no infrastructure resource has been changed during this inspection. Do not treat the marked failures as proven product defects or as proven infrastructure-only failures.

## Acceptance

Run 863 cannot approve a merge. Linux subsequently accepted packaging and branding changes in #1331 and #1324. The refreshed aggregate pins those changes and preserves the five constituent heads. Application, crate, Cargo and isolated-test registration diffs against the old head are empty. The refreshed aggregate still requires local validation and its own Forgejo acceptance.

The manifest hashes identify downloaded logs retained locally in the original aggregate worktree. Full runner paths and infrastructure details are intentionally omitted from this public record.
