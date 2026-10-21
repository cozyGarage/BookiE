# Stream page construction, October 22

Owner: [#1306](https://github.com/cozyGarage/BookiE/issues/1306). Source base: `bf036c8dc`.

## Reproduction

A stream page builds a grid synchronously when its holder maps. Mapping and removing that holder before a rendered frame still constructs its grid. An isolated GTK regression verifies that sequence and fails on the existing implementation. Live streaming removes old notebook pages; this can briefly map a retained page that is soon removed.

The proposed repair defers construction to a widget-owned frame callback, with one pending callback per holder. The callback takes the build closure once. It does not capture the holder or use a global idle queue. The regression passes, including a weak-reference assertion that dropping the retired holder releases its result. All 104 registered GTK tests passed after selected-page tests were changed to await the observable grid on its frame while preserving grid identity assertions. Full preflight and application all-target Clippy also passed.

## CPU evidence

The normal-width 0.2.0 PostgreSQL capture in the [release evidence](../release-0.2.0-2026-10-21/README.md) resolves using the matching Arch debug package, build ID `3bcdc9f0005231cddcb27d6529c570cc27b509ee`. Inclusive samples place about 19.98% in stream-page map callbacks and 23.54% in live-stream updates. These paths overlap and the percentages must not be added. They do not quantify the proposed fix's benefit.

## Combined preceding fixes

The optimized aggregate #1305 build (`07b85c9a5`, binary SHA-256 `a451835f303d76310a0394057ee9e0cd6092e9f24ea2bfb35a37a6cb9d545675`) completed 10,000-, 100,000- and 1,000,000-row PostgreSQL queries with a successful terminal status and no schema error. The million-row view retained rows 980,001–1,000,000 and reported completion in 16,265 ms. After replacement and close, RSS was about 218.5 MiB.

The wide-value case subsequently lost its accessibility root. The harness exited 1 after 60.214 seconds, with no sampler safety abort. That case and the remainder of the six-case series are not qualified. A concurrent test build also prevents a controlled CPU comparison. This partial capture proves the observed completion fixes for the first three cases, not a resource improvement or crash diagnosis.

Raw captures remain under the dated local release-artifact directory, including `profiles/postgres-load-fixed-train`. Native visual acceptance, optimized before/after comparison and Forgejo acceptance of the frame repair remain pending.

## Optimized comparison

The baseline binary above was compared with `bfe4c159d` (frame repair plus the same preceding fixes), SHA-256 `876dcf853025d7f5cea6f5ba98196d78b4cbceab7be8f6a457c7c9289cd254ed`. Both used the workspace release profile, PostgreSQL 17.11, a private D-Bus session, 1280×1024 Xvfb, Cairo and perf sampling. No concurrent build or local test was running. Each process performed three cycles of the following query, followed by replacement with `SELECT 1` and closing the editor:

```sql
SELECT g AS resource_probe, repeat('x',128) AS payload
FROM generate_series(1,1000000) g;
```

All six queries reported successful completion and retained rows 980,001–1,000,000. [Measurements](comparison.json) include process samples summarized at each phase.

| Three-cycle series | Baseline | Frame repair |
| --- | ---: | ---: |
| Process CPU seconds | 54.08 | 48.97 |
| Total elapsed seconds | 77.229 | 71.915 |
| Peak RSS, MiB | 226.59 | 222.95 |
| Per-query observed CPU seconds | 16.67 / 17.28 / 17.59 | 15.10 / 15.47 / 15.92 |

The observed CPU reduction is approximately 9.4%. Total duration includes connection setup, controlled pauses, accessibility traversal, replacement and close. This is one baseline-first comparison, not a statistical benchmark or a native Wayland result. Perf returned -2 after stopping the baseline capture and 0 for the repaired capture; process CPU totals come from `/proc` sampling rather than profiler exit status. The fixture was stopped afterward.

An earlier comparison attempt connected before PostgreSQL finished initialization and failed before running the workload. It is excluded; the accepted pair ran after an explicit readiness check. After integrating the preceding fixes, the combined 105-test GTK suite and full preflight passed. Longer runs, reversed run order, wide-value behavior and exact-head Forgejo acceptance remain open.
