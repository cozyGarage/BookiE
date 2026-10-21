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
