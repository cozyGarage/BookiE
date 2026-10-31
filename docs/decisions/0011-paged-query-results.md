# 0011: Page large editor results through a cursor

- **Status**: Accepted
- **Date**: 2026-10-07

## Context

The editor needs to read arbitrary query results incrementally without rewriting user SQL. Re-running a statement with `LIMIT` and `OFFSET` can repeat or miss rows as data changes, and SEC-6 requires that result limits never append a `LIMIT`.

## Decision

Add an optional driver capability that opens a forward-only cursor over a single read statement and returns pages of at most 5,000 rows.

1. Only a statement that `PolicyGuard` classifies as a read, run in a `Session`, may open a cursor. Other statements keep the capped, materialized path.
2. PostgreSQL implements it with a server cursor inside the session's transaction. Other engines report the capability as unsupported and keep the capped path until each has a tested cursor.
3. The statement is classified, ruled and audited once. Each page fetch writes a page event under the same operation identifier. Masking applies to every page. A denied, failed, cancelled or timed-out fetch writes the terminal state and closes the cursor.
4. The grid retains pages within the 64 MiB estimated-row budget and keeps the newest page even when that page alone exceeds it. A forward-only cursor cannot re-read evicted pages. An export can preserve every row only when it drains the cursor to exhaustion under the snapshot rules in ADR 0014.
5. Cancel and timeout reach the server through the existing server-side cancellation. Closing the tab, switching connection or running a new statement closes the cursor.

## Alternatives considered

- **Re-run the statement with `LIMIT` and `OFFSET`.** Rejected; see Context. It rewrites user SQL, can repeat or miss rows, and re-evaluates the statement per page.
- **Append every page forever.** Rejected. Memory is still unbounded.
- **A page-count cache limit.** Rejected for an estimated-byte limit (see the retention budget in this decision). A fixed page count can exceed any byte budget when rows are large.
- **Strong, permanent row objects.** Rejected. Memory grew with scrolling. Row slots hold weak references plus a source index, and draft rows stay strong. Identity must not become a bare row position, because inserts, removals and sorting move positions.
- **ODBC columnar fetch.** Rejected; see [ADR 0001](0001-no-plugin-system.md).

## Consequences

- The stream cache bounds estimated retained row storage, except for the newest-page allowance. This is not a process-memory ceiling: pins, overlapping results, widgets, incoming buffers, metadata and allocator retention remain outside the estimate.
- Every page is a guarded, audited call. The audit journal grows with the number of pages, so the page size is fixed and a long scroll writes bounded events.
- A forward-only cursor cannot show earlier pages without keeping them. The retained-page budget decides what the user can scroll back to.
- Engines without a cursor keep the current caps and the truncated banner.
