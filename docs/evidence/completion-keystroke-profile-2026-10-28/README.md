# Completion keystroke profile, 2026-10-28

Host measurements for the SQL completion change in [#1830](https://github.com/cozyGarage/BookiE/pull/1830): the catalog's comparison keys are built once per catalog and grammar, tables are filtered before they are cloned and sorted, and the script is lexed once per lookup. The deterministic guards for the same behaviour are the work counters in the [testing guide](../../testing-quality-and-performance.md#performance-regression-contracts); this entry records only elapsed time, which is a host sample and not a gate.

## What was measured

`completion_proposals::candidates` with the PostgreSQL grammar set, as the app sets it, on a synthetic catalog of `tenant_NNN.table_NNNNNN` names (200 tenants) and on a synthetic script of `INSERT`, comment and `SELECT` statements with quoted semicolons and dollar quoting, the cursor at its end. The harness is [benchmark.rs.txt](benchmark.rs.txt) with [benchmark-Cargo.toml.txt](benchmark-Cargo.toml.txt), built twice against two checkouts.

| Side | Commit |
|---|---|
| Before | `401672b86cc279f495623591378a5914a8d4d235` (parent of the squash commit) |
| After | `a75a47ce52dde963a6eb0ddd179de47b837b3427` (#1830) |

The squash commit also moves inline tests into sibling files. That part changes no production code, so the difference measured here is the completion change.

## Results

Median of five per-process means, release build with debuginfo, alternating before and after. Raw per-process values and the spread are in [host-samples.json](host-samples.json).

| Case | Before | After | Speed-up |
|---|---|---|---|
| 1,000 tables, selective prefix | 0.385 ms | 0.026 ms | 14.6x |
| 1,000 tables, no-match prefix | 0.419 ms | 0.025 ms | 16.6x |
| 10,000 tables, selective prefix | 4.075 ms | 0.043 ms | 95.4x |
| 10,000 tables, no-match prefix | 4.422 ms | 0.056 ms | 78.8x |
| 100,000 tables, selective prefix | 45.041 ms | 0.644 ms | 70.0x |
| 100,000 tables, no-match prefix | 45.007 ms | 0.651 ms | 69.1x |
| 16 KB script, 50 tables | 0.362 ms | 0.111 ms | 3.3x |
| 346 KB script, 50 tables | 7.317 ms | 1.974 ms | 3.7x |
| 3,589 KB script, 50 tables | 120.394 ms | 32.497 ms | 3.7x |

Correctness check: every one of the 45 paired runs returned the same number of candidates before and after, and the equivalence test in the source tree compares the full candidate lists at every cursor position.

## Method

Five separate processes per side, alternating, each with two warm-up and 20 measured iterations for the catalog cases and one warm-up and five measured iterations for the script cases. The reported statistic is the median of the per-process means.

## Limits

- One host: 11th Gen Intel(R) Core(TM) i7-11700K @ 3.60GHz, 16 logical CPUs at about 81% scaling, kernel 7.2.8-9-omarchy, rustc 1.99.1 (58cf8f9c3 2026-10-13).
- The load average at the start was 11.49, 9.85, 7.87 because other builds were running. The spread columns in the JSON show the noise; treat the figures as samples, not quiet-machine numbers.
- Synthetic names and script text, not a production catalog. Real schemas with long common prefixes match more tables per keystroke, which the work contracts bound separately.
- This measures the library call only. It does not include GTK popup construction, the completion provider's own filtering, or paint. Scroll and paint latency were not measured.
- Other paths profiled the same day (SQL statement splitter, policy classifier, sidebar filter, CSV rendering) are not recorded here because they were left unchanged.
