# 0011: Page large editor results through a cursor

- **Status**: Accepted
- **Date**: 2026-10-07
- **Implementation**: PostgreSQL editor streaming through a server cursor and spool is shipped; other engines keep the capped path until each has a tested cursor. A read-only “snapshot session” wording in older notes means this cursor session, not ADR 0014 or 0015.

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

- Each stream cache targets a 64 MiB estimated retained-row window. The GTK application also shares a 256 MiB estimated page budget across streamed result owners. This is not a process-memory ceiling: widgets, incoming buffers, metadata and allocator retention remain outside the estimate.
- Every page is a guarded, audited call. The audit journal grows with the number of pages, so the page size is fixed and a long scroll writes bounded events.
- A forward-only cursor cannot show earlier pages without keeping them. The retained-page budget decides what the user can scroll back to.
- Engines without a cursor keep the current caps and the truncated banner.

## Amendment (2026-11-15): Shared GTK page budget

The GTK application shares a 256 MiB estimated page budget across streamed-result pages, including pages retained by snapshots, streamed-result pins, derived results, visible page loads and exports. Page decode and reload reserve up to 64 MiB before work and resize the lease to the decoded page, so in-flight reservations count against the cap. Per-cache eviction still targets 64 MiB. The shared estimate does not cover materialized non-streaming results or process RSS; it counts estimated row payloads and cells, not GTK allocations, driver buffers or allocator retention.
