# Large PostgreSQL query stage profile, 2026-10-30

The release build used Linux `0883c6fc5c9867ec40929dbc30f61d05e2dc83fd` with the temporary instrumentation below. The instrumented executable SHA-256 was `20ccc86f3d8c7cc82ea7d8f358b404c0460da7970ca2a1cf89b71254e2b85f19`. The uninstrumented executable SHA-256 was `120741f75af80632eb549388822c49b84f7dc36fcfef045ce89dec0289cfca89`. The instrumentation was removed after profiling; it is not a product change.

The existing [profile fixture](../postgres-stream-heap-profile-2026-10-25/run.sh) created an isolated PostgreSQL 16 container and used Xvfb/Cairo and a private desktop session. It queried two million rows and ten columns three times per build. Each cycle received 400 data pages and passed the probe's backward and forward page navigation checks. An additional final empty fetch is included in each 401-page timing sum. This is one host and workload, not native Wayland or installed-package acceptance.

| Cycle | Uninstrumented total (s) | Instrumented total (s) | Fetch and decode (s) | Spool (s) | UI acceptance wait (s) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 10.471 | 10.439 | 5.305 | 1.854 | 2.033 |
| 2 | 10.292 | 9.780 | 4.924 | 1.875 | 2.026 |
| 3 | 9.812 | 9.746 | 4.871 | 1.891 | 2.046 |

The fetch interval includes the database, network, decoding and policy work in `fetch_page_with_audit`. The spool interval includes scheduling a blocking worker, serialization, encryption and file writes. UI acceptance is elapsed time from dispatch through main-context handling and acknowledgement, including scheduling; it is not exclusive GTK CPU time or paint latency. Durations are sequential wall-clock intervals; they do not attribute CPU, and exclude setup, terminal handling and per-page log overhead. Instrumentation and host load affect both elapsed time and attribution. The first page's three intervals were 19.623/4.563/2.482 ms, 18.002/4.557/2.122 ms and 16.100/4.672/2.159 ms. These do not include query setup or actual first-paint time.

Uninstrumented RSS before the first query was 178,500 KiB; after editor close in the three cycles it was 500,904, 554,284 and 554,620 KiB. Three cycles cannot prove a stable memory ceiling or a leak. No live-allocation attribution was captured. Follow-up work should measure first paint and per-stage CPU, and evaluate bounded overlap only with correctness, cancellation, ordering and spool quota tests. Related open work is [#1863](https://github.com/cozyGarage/BookiE/issues/1863) and [#1032](https://github.com/cozyGarage/BookiE/issues/1032).

## Reproduction

Build with `TABLEPRO_GTK_BUILD_PROFILE=release bash scripts/gtk-test-binary.sh`, apply the instrumentation to an isolated checkout and rebuild. Then run `HEAPTRACK=0 TABLEPRO_PROFILE_REPETITIONS=3 bash docs/evidence/postgres-stream-heap-profile-2026-10-25/run.sh <output-directory>`. Sum the microsecond fields on `BOOKIE_PROFILE_PAGE` lines in the scenario stderr artifact; sequence zero starts each cycle. Repeat without instrumentation for a separate baseline. The fixture creates and removes its own container.

```diff
diff --git i/apps/linux/app/src/ui/editor/outcomes.rs w/apps/linux/app/src/ui/editor/outcomes.rs
index bcf7d734d..d2d92d7c8 100644
--- i/apps/linux/app/src/ui/editor/outcomes.rs
+++ w/apps/linux/app/src/ui/editor/outcomes.rs
@@ -535,7 +535,9 @@ async fn fetch_stream_pages<F, Fut>(
 {
     let mut sequence = 0;
     loop {
+        let stage_started = std::time::Instant::now();
         let fetched = cursor.fetch_page_with_audit(control).await;
+        let fetch_us = stage_started.elapsed().as_micros();
         merge_audit_receipts(&mut audit_receipts, fetched.audit_receipts);
         let page = match fetched.result {
             Ok(page) => page,
@@ -556,7 +558,10 @@ async fn fetch_stream_pages<F, Fut>(
         let has_next_page = page.has_next_page;
         let page = std::sync::Arc::new(page.result);
         let truncated = page.truncated;
+        let stage_started = std::time::Instant::now();
         let spooled = spool.append_page(sequence, page.clone()).await.is_ok();
+        let spool_us = stage_started.elapsed().as_micros();
+        let stage_started = std::time::Instant::now();
         if !stream_update(StreamUpdate::Page {
             sequence,
             page,
@@ -567,6 +572,7 @@ async fn fetch_stream_pages<F, Fut>(
             finish_stream_cursor(cursor, true).await;
             return ScriptRunResult::Streamed(StreamTerminal::Cancelled);
         }
+        eprintln!("BOOKIE_PROFILE_PAGE {sequence} {fetch_us} {spool_us} {}", stage_started.elapsed().as_micros());
         sequence = sequence.saturating_add(1);
         if !spooled {
             finish_stream_cursor(cursor, true).await;

```
