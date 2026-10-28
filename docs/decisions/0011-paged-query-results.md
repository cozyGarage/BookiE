# 0011: Page large editor results through a cursor

- **Status**: Accepted
- **Date**: 2026-10-07
- **Implementation status**: see [implementation-status.md](implementation-status.md#adr-0011-page-large-editor-results-through-a-cursor)
- **Clarified**: 2026-10-16, 2026-10-22 and 2026-10-24 for implementation status, retained-byte limits and shipped cache behavior.

## Context

A SQL editor result is materialized in one pass up to three caps: 1,000,000 rows, 10,000,000 cells and 64 MiB of estimated value bytes. The byte cap binds first. A historical grid profile measured about 2.5 times the driver result; it predates #518’s shared-row path, and no comparable post-change profile establishes a current ratio (PERF-2, PERF-8). At the time of this decision, full-table export paged 5,000 rows through the guard and the grid read shared rows lazily. [ADR 0014](0014-full-table-export-snapshot.md) later required snapshot-safe paging; the current export path refuses full-table export until a driver proves that contract. The remaining editor cost at that point was the single large fetch of an arbitrary query.

Re-running the user's statement with `LIMIT` and `OFFSET` is not an option. It rewrites SQL the user wrote, can repeat or miss rows when data changes, and re-evaluates the whole statement per page. SEC-6 also requires that the cap never appends a `LIMIT`.

## Decision

Add an optional driver capability that opens a forward-only cursor over a single read statement and returns pages of at most 5,000 rows.

1. Only a statement that `PolicyGuard` classifies as a read, run in a `Session`, may open a cursor. Other statements keep the capped, materialized path.
2. PostgreSQL implements it with a server cursor inside the session's transaction. Other engines report the capability as unsupported and keep the capped path until each has a tested cursor.
3. The statement is classified, ruled and audited once. Each page fetch writes a page event under the same operation identifier. Masking applies to every page. A denied, failed, cancelled or timed-out fetch writes the terminal state and closes the cursor.
4. The grid retains pages within the 64 MiB estimated-row budget and keeps the newest page even when that page alone exceeds it. A forward-only cursor cannot re-read evicted pages. An export can preserve every row only when it drains the cursor to exhaustion under the snapshot rules in ADR 0014.
5. Cancel and timeout reach the server through the existing server-side cancellation. Closing the tab, switching connection or running a new statement closes the cursor.

## Alternatives considered

- **Re-run the statement with `LIMIT` and `OFFSET`.** Rejected; see Context. It rewrites user SQL, can repeat or miss rows, and re-evaluates the statement per page.
- **Append every page forever.** Rejected. Memory is still unbounded ([grid memory plan](../archive/grid-memory-and-fetch-action-plan-2026-10-07.md)).
- **A page-count cache limit.** Rejected for an estimated-byte limit (see the October 22 and 24 clarification). The earlier four-page cache admitted by count and could exceed any byte budget ([Linux peer review](../archive/linux-peer-review-2026-10-16.md), finding 4).
- **Strong, permanent row objects.** Rejected. Memory grew with scrolling. Row slots hold weak references plus a source index, and draft rows stay strong. Identity must not become a bare row position, because inserts, removals and sorting move positions.
- **ODBC columnar fetch.** Rejected; see [ADR 0001](0001-no-plugin-system.md).

## Consequences

- The stream cache bounds estimated retained row storage, except for the newest-page allowance. This is not a process-memory ceiling: pins, overlapping results, widgets, incoming buffers, metadata and allocator retention remain outside the estimate.
- Every page is a guarded, audited call. The audit journal grows with the number of pages, so the page size is fixed and a long scroll writes bounded events.
- A forward-only cursor cannot show earlier pages without keeping them. The retained-page budget decides what the user can scroll back to.
- Engines without a cursor keep the current caps and the truncated banner.

## Open questions

- Whether MySQL and SQL Server can use their native streaming without holding a transaction open.

## Clarification, October 22 and 24, 2026

The maintainer selected an estimated-byte retention limit. The UI stream cache retains pages while their estimated row storage stays within the existing 64 MiB query-result budget. It has no separate page-count limit. The newest page is always kept, including a single oversized page. Older pages leave the forward-only RAM window without changing their values or received-row counts. ADR 0025 adds a separate encrypted spool for revisiting fetched PostgreSQL pages while the result owner remains open; its quota and page cap can stop the stream as incomplete.

The estimate reuses core row/value accounting and excludes column metadata, widgets, incoming driver buffers, overlapping result snapshots, other tabs, pins and allocator-retained pages. It is not a process-memory ceiling. The PostgreSQL spool preserves access to pages successfully written within its limits, but a quota, page-cap or storage refusal stops the stream as incomplete. Other engines and unsupported query shapes remain materialized. Broader ownership and profiling acceptance remain under [#1032](https://github.com/cozyGarage/BookiE/issues/1032); installed GTK and release-candidate acceptance remain separate.
