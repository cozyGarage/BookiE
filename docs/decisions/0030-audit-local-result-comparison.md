# 0030: Audit local result comparison

- **Status**: Proposed
- **Date**: 2026-11-03

## Context

Compare Results diffs two result sets that the editor already holds, in memory or in a spool. It runs no SQL, so no database operation is audited for it, yet it reads and summarises values a reviewer may care about. [ADR 0010](0010-administrative-action-audit.md) records confirmed settings operations as durable intent and outcome pairs without paths, names or secrets.

## Decision

Record a confirmed comparison as an intent and outcome pair in the existing audit journal, under the same rules as ADR 0010.

1. The pair uses a new stable action code `result_compare` and the operation class `Read`. A comparison never makes governed writes wait on it, and an intent left without an outcome after a crash is not an unresolved write obligation.
2. The event carries the source connection identifier, name and environment, because the compared data came from that connection. It carries one aggregate count, the number of differing rows, in the metadata that ADR 0010 defines. It carries no SQL, column names, values, keys, row totals or file paths.
3. The intent is written after the user confirms the comparison and before any page is read. If the intent cannot be made durable, the comparison does not start. The outcome is written when the comparison ends as succeeded, cancelled or failed.
4. Both inputs must come from one connection. A comparison across connections is refused.
5. Inputs that are truncated, still loading, cancelled, timed out or failed are refused. A comparison never reports a difference it cannot prove.
6. Equality uses the exact typed comparison of [ADR 0007](0007-type-and-value-preservation.md). Cells that cannot be compared (`Undecodable`) make the row `unknown`, not equal and not changed. When either result was masked by policy, the comparison says so, because different values can look the same after masking.
7. A comparison reads at most 1,000,000 rows per side and keeps at most 100,000 differing rows. A side with more than 1,000,000 rows is refused with a message rather than compared in part; when more than 100,000 differing rows exist, the difference list is cut at the bound, the counts stay exact, and the result says it was capped. It runs off the GTK thread and stops on cancel.

## Rationale

The journal exists to show what data work happened. A local comparison is read-only, but it is also a review of data from a connection, so a bounded aggregate record keeps that visible without copying data into the journal.

## Consequences

- The journal gains one action code. Older readers ignore it because the metadata is optional.
- A comparison can be refused when the audit journal is unavailable, matching bundle administration.
- The audit view shows the action with its counts and no values.

## Alternatives considered

- **No audit for local operations.** Rejected. The user requires a record, and the data came from a governed connection.
- **Audit with column names or key values.** Rejected. It copies data into the journal.
- **The `Administrative` class with the nil connection.** Rejected. The comparison belongs to a connection and is not a settings change.
