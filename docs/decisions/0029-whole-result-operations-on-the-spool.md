# 0029: Operate on a completed streamed result through the spool

- **Status**: Proposed
- **Date**: 2026-11-03

## Context

[ADR 0025](0025-transient-result-page-spool.md) keeps every page of a streamed PostgreSQL result in an encrypted, process-local spool so earlier pages stay revisitable without running SQL again. The grid's Find, filter and sort act only on the one page that is loaded, so a user can scroll a million rows but cannot search or narrow them. Re-running SQL with a `WHERE` clause can observe different data and goes through the database again.

## Decision

Allow local operations over a completed streamed result by reading its spool. A result is eligible only when its stream ended complete, it still owns its spool and every page was stored.

1. **Scan.** `bookie-app-services` provides one GTK-free scan that loads spool pages in order, one at a time, on a blocking worker. It checks a cancellation token and reports page progress between pages, never holds the spool lock between pages and keeps only the current page and a bounded accumulator in memory. A cancelled or failed scan leaves the source result unchanged.
2. **Find.** Find matches typed values by the text the grid shows for them (dates and times in the grid's format, readable bytes as text and also as hex, other bytes as hex), in one column or in all. It returns a capped list of matches as page and row positions and a flag when the cap was reached. Cells that cannot be searched (`Undecodable`) are counted and reported, never matched (a missing source field is a determinate absent value, like NULL: never matched and not counted as unreadable); a "not equal" filter drops them and counts them the same way instead of treating them as different. Cells cut by a size limit are not tracked separately. NULL is never matched by text. Jumping to a match loads the page that holds it; it never scans again.
3. **Filter.** Filter runs the scan with a typed predicate and writes matching rows into a new spool created by the same manager. The derived spool has its own random key and nonce space. It is a complete result with its own row counts, so paging, continuous scrolling, pinning and export treat it like any streamed result. Rows keep their source order. Empty output pages are not written, because spool page indices are dense. The filter does not run SQL and writes no database audit event.
4. **Quota.** A derived spool draws on the same process-wide quota as its source. A quota or filesystem failure stops the filter, drops the partial derived spool and leaves the source intact. The refusal names the quota so the user can free space by closing results.
5. **Predicates.** Equality uses the exact typed comparison of [ADR 0007](0007-type-and-value-preservation.md): NULL, empty text and empty bytes stay distinct, NaN payloads, signed zero and decimal scale are not merged, an integer never equals its text form, and a value that cannot be compared is not coerced. "Not equal" keeps NULL cells, and "equal" to a NULL value matches only NULL cells. The cell menu of a completed streamed result offers both as **Show All Rows Equal to This Value** and **Show All Rows Not Equal to This Value**, scoped to that column. A comparison value that is `Undecodable` is refused, and `Undecodable` cells never match a text search.
6. **Location.** The spool directory defaults to the operating system temporary directory, which may be memory-backed. `BOOKIE_SPOOL_DIR` selects another existing absolute directory; a missing or relative value is ignored with a logged warning. Files stay anonymous and encrypted wherever they live.
7. **Limits.** The 4,096-page cap and the quota of ADR 0025 stay. The Find match list holds at most 10,000 positions. The page size of a derived spool matches its source.

Sort over the whole result is not part of this decision. It needs run files and a merge, with a larger quota headroom, and needs its own record.

## Rationale

The spool is already the one place that holds the exact rows the user saw. Scanning it keeps the same-execution guarantee that ADR 0025 protects, adds no database load and no audit noise, and keeps masked values masked because the pages were masked before they were stored. A derived spool reuses every consumer of a streamed result instead of adding a second result model.

## Consequences

- A filter can double stored bytes for one result until it is closed. The shared quota bounds the total, and a refused filter says why.
- A long scan competes with page loads for the spool lock one page at a time. Measured delay is recorded in private verification records, not here.
- Memory-backed temporary directories count spool bytes against RAM. `BOOKIE_SPOOL_DIR` lets a user point the spool at disk.
- Tests must cover cancellation mid-scan, a failed or tampered page, quota refusal, source integrity after a failed filter, every `Value` variant in predicates, and release of the derived spool with its last owner.

## Alternatives considered

- **Re-run SQL with a filter.** Rejected. It can observe changed data and sends the work back to the database.
- **A `PageSource` trait over spool and derived results.** Deferred. A derived result is a `StreamedResult` with its own spool, which needs no new abstraction for the first consumers.
- **An in-memory filtered index only.** Rejected for filters. A filtered result must support paging, pinning and export without keeping rows in RAM.
- **A shared spool for source and derived pages.** Rejected. A separate key and file keep one owner per result and let the derived result outlive or predecease its source.
- **Sorting in the same change.** Deferred. See the decision above.
