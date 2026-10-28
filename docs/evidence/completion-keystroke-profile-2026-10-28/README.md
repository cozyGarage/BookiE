# Completion keystroke profile, 2026-10-28

Host samples for the SQL completion change in [#1830](https://github.com/cozyGarage/BookiE/pull/1830): comparison keys for the catalog are built once per catalog and grammar, tables are filtered before they are cloned and sorted, and the script is lexed once per lookup. The deterministic guards for the same behaviour are the work counters in the [testing guide](../../testing-quality-and-performance.md#performance-regression-contracts); this entry records only elapsed time, which is a host sample and not a gate.

## What was measured

One `completion_proposals::candidates` call with the PostgreSQL grammar set, as the app sets it. Synthetic catalog names `tenant_NNN.table_NNNNNN` (200 tenants) and a synthetic script of `INSERT`, comment and `SELECT` statements with quoted semicolons and dollar quoting, the cursor at its end. The harness is the repository example [`completion_benchmark`](../../../crates/app-services/examples/completion_benchmark.rs), built unchanged into both trees:

| Side | Commit |
|---|---|
| Before | `401672b86cc279f495623591378a5914a8d4d235` (parent of the #1830 squash commit) |
| After | `09fce5f1b`, the `linux` head containing the #1830 squash commit `a75a47ce5` |

The squash commit also moves inline tests into sibling files, which changes no production code, so the difference below is the completion change.

## Results

Each cell is the minimum over all iterations and processes, then the median of the per-process medians. Raw per-process values are in [host-samples.json](host-samples.json).

| Case | Before (min / median) | After (min / median) | Speed-up (min / median) |
|---|---|---|---|
| 1,000 tables, selective prefix | 0.348 / 0.408 ms | 0.019 / 0.021 ms | 17.8x / 19.8x |
| 1,000 tables, no-match prefix | 0.372 / 0.429 ms | 0.018 / 0.019 ms | 21.2x / 22.3x |
| 10,000 tables, selective prefix | 3.664 / 3.943 ms | 0.037 / 0.040 ms | 99.8x / 99.6x |
| 10,000 tables, no-match prefix | 3.684 / 3.947 ms | 0.047 / 0.049 ms | 79.0x / 80.4x |
| 100,000 tables, selective prefix | 38.378 / 41.013 ms | 0.222 / 0.306 ms | 172.7x / 134.2x |
| 100,000 tables, no-match prefix | 38.151 / 40.761 ms | 0.347 / 0.402 ms | 109.8x / 101.3x |
| 16 KB script, 50 tables | 0.300 / 0.314 ms | 0.090 / 0.098 ms | 3.3x / 3.2x |
| 346 KB script, 50 tables | 5.313 / 5.632 ms | 1.488 / 1.597 ms | 3.6x / 3.5x |
| 3,589 KB script, 50 tables | 75.406 / 77.946 ms | 22.215 / 23.361 ms | 3.4x / 3.3x |

Correctness check: for every case the before and after runs returned the same number of candidates in all 14 paired processes, and the equivalence test in the source tree compares the full candidate lists at every cursor position.

## Method

Seven separate processes per side, alternating, each pinned with `taskset` to one logical CPU. One warm-up lookup, then 40 timed lookups for the catalog cases and seven for the script cases.

Reproduce on the current tree with:

    cargo run --release -p tablepro-app-services --example completion_benchmark

To measure an older revision, copy the example into that checkout's `crates/app-services/examples/` first. It uses only `SchemaIndex`, `set_tables` and `candidates`, which exist in both.

## Limits

- **The host was saturated, not quiet.** Every logical CPU was 98 to 100 percent busy with other builds, and the load average moved from 11.2 to 12.5 (15-minute value 15.1) on 16 CPUs. Pinning to one CPU does not make that quiet. Minimum and median agree within the spread, which is why both are reported, but treat the absolute times as samples; the ratios are the useful part.
- One machine: 11th Gen Intel(R) Core(TM) i7-11700K @ 3.60GHz, kernel 7.2.8-9-omarchy, rustc 1.99.1 (58cf8f9c3 2026-10-13), release profile.
- Synthetic names and script text, not a production catalog. A real schema with long common prefixes matches more tables per keystroke; the work contracts bound that separately.
- Library call only. It does not include popup construction in the GTK completion provider, painting, or scrolling. Scroll and paint latency were not measured.
- Other paths profiled the same day (statement splitter, policy classifier, sidebar filter, CSV rendering) were left unchanged and are not recorded here.
