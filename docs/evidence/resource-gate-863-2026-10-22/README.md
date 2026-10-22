# Resource gate 863, 2026-10-22

## Observed state

The exact tested head is recorded in the [manifest](manifest.json). At inspection, 23 jobs passed, four were marked failed and two remained running. The four failed jobs were PostgreSQL E2E, GTK shards 2 and 3, and the PostgreSQL release fixture.

The downloaded logs contain no terminal test failure, assertion, compiler error or runner exit reason. All four stop abruptly between 01:14:08 and 01:14:19 UTC. PostgreSQL E2E stops while compiling. The release fixture shows successful test groups, then stops during an additional compilation. Both GTK logs show successful scenarios and ordinary AT-SPI cache warnings before stopping.

The clustered interruption suggests a runner or execution-service interruption, but it does not establish one. The cause remains unresolved. No job has been retried and no infrastructure resource has been changed during this inspection. Do not treat the marked failures as proven product defects or as proven infrastructure-only failures.

## Acceptance

Run 863 cannot approve a merge. Linux subsequently accepted packaging and branding changes in #1331 and #1324. The refreshed aggregate pins those changes and preserves the five constituent heads. Application, crate, Cargo and isolated-test registration diffs against the old head are empty. The refreshed aggregate still requires local validation and its own Forgejo acceptance.

The manifest hashes identify downloaded logs retained locally in the original aggregate worktree. Full runner paths and infrastructure details are intentionally omitted from this public record.

## Terminal-run recheck

Forgejo subsequently marked the run failed, with a stopped timestamp of 01:25:20 UTC. Its jobs endpoint still reported two jobs running. Their logs had also stopped around 01:14 UTC. The six interrupted logs span three distinct runner identities, grouped anonymously in the manifest. This makes a shared execution or reporting interruption more plausible than one isolated test failure, but does not distinguish a control-service interruption, network loss, host failure or resource exhaustion.

Read-only SSH journal checks were rejected by the local execution approval policy. No runner was restarted or cleaned. Host/service journals covering 01:13–01:27 UTC are the next evidence needed to establish the cause. The earlier AT-SPI warnings also occur between successful GTK scenarios and are not a demonstrated terminal failure.

## Runner journal supplied by the maintainer

The runner journal shows `ReportState` and `ReportLog` receiving TCP connection refusals from the Forgejo service at 01:15:19–01:15:20 UTC. Reporting then resumes with repeated `submitted logs are lost 23 < 24` errors for task 23008. At 01:17:36 UTC, the runner reports that all final log/status upload attempts failed. Task 23008 is the ClickHouse GTK-server job in this run.

This establishes a CI control-service connectivity failure followed by a log-sequence reconciliation failure. It explains missing logs and unreliable terminal job reporting. It does not establish why the Forgejo endpoint refused connections, nor prove that every interrupted test would otherwise pass. The service journal and kernel journal on the Forgejo host remain necessary to distinguish a restart, crash, resource exhaustion or another service interruption.

The same supplied snapshot shows the executor had remained up for 11 days and its root filesystem was 91% used, with 26 GiB available. That is a capacity concern, not evidence of ENOSPC or the cause of this service outage. No cache deletion or runner restart was performed.
