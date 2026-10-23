# Stream page realization investigation

Status: local experiment for [issue 1523](https://github.com/cozyGarage/BookiE/issues/1523). [Build identities](build.json) record the optimized executables and toolkit versions. No competing compilation ran during the timing measurements. The baseline and candidate source bases differ by documentation and error wording changes; the streaming presentation files are unchanged between those bases. This is Xvfb/Cairo evidence, not native Wayland visual acceptance.

## Mechanism

The original live stream creates a lazy page holder for each received result page and builds a complete grid on its next frame when mapped. Fast streams repeatedly evict the selected page and map its successor. Building grids for these short-lived pages dominates client CPU. The raw-driver comparison in the [earlier investigation](../postgres-optimized-stream-2026-10-23/README.md) completed the generated two-million-row query in 3.367 seconds without GTK.

The candidate waits 50 ms before realizing a mapped stream page. The callback checks that the holder remains mapped. It keeps weak references to both the holder and its builder so an evicted page can release its result immediately. Pager ranges and received/retained counts still update per page. A page the user selects remains available and builds after the short delay. Sorting, filtering, selection and copy/export code are unchanged.

## Measurements

The [baseline](stream-2m-baseline.json) timed out at 60.65 seconds after receiving 1880000 rows and used 61.82 process CPU seconds. It reported retained rows 1860001–1880000. The diagnostic accepts explicit timeouts, so its passing invocation does not mean the completion acceptance passed.

The [first candidate](stream-2m-realization-experiment.json) completed all 2000000 rows in 4.316 seconds and used 3.19 CPU seconds. It reported retained rows 1980001–2000000. Settled RSS was 226188 KiB with results and 231744 KiB after close. Repeated measurements and pager verification are in [the interaction record](stream-2m-realization-pager.json).

An initial pager probe sent three accessibility activations without awaiting their effects; the app advanced one page. The corrected probe waits for each displayed range and first row value before the next activation. The failed probe is not treated as acceptance.

## Reproduction

Use the existing [disposable PostgreSQL fixture](../postgres-optimized-stream-2026-10-23/run-fixture.sh) with the absolute paths of [run-probe.sh](run-probe.sh) and [probe-stream.py](probe-stream.py) as TABLEPRO_PROFILE_RUNNER and TABLEPRO_PROFILE_SCRIPT. Set TABLEPRO_PROFILE_REPO to the checkout, TABLEPRO_PROFILE_FIXTURE to the fixture, TABLEPRO_GTK_BINARY to an immutable release executable in an installed layout, TABLEPRO_PROFILE_OUT to an owned JSON file, TABLEPRO_PROFILE_ROWS=2000000, TABLEPRO_PROFILE_REPETITIONS=3, TABLEPRO_PROFILE_VERIFY_PAGER=1, TABLEPRO_GTK_SCENARIO=profile_postgres_large_stream and TABLEPRO_GTK_SCENARIO_RETRIES=0, then invoke the fixture with Bash.

The interaction check visits all four retained pages in both directions and checks the displayed ranges and first row IDs. Query timing is captured before navigation. CPU and memory measurements come only from the owned application process.

The three pager-verified runs completed in 4.076, 4.084, 4.079 seconds and used 3.02, 3.01, 3.00 CPU seconds. All received two million rows and retained rows 1980001–2000000. Post-close RSS was 296532, 307444, 311980 KiB after visiting all retained pages. These values include more constructed grids than the first single-page run, so the memory figures are not the same presentation workload.

The [completion assertion](check-completion.py) rejects the baseline timeout record and accepts all three pager-verified candidate cycles. [Assertion results](completion-acceptance.json) preserve both exits. This checks the recorded outcomes from the real fixture; it is not a hardware-independent CI timing threshold. Existing GTK regression tests cover lazy realization, repeated selection and release of retired page data; the retired-page check also drains the pending delay before checking release again.

## Validation and limits

[Local validation](validation.json) passed: preflight with 2073 non-GTK unit and 2086 sandbox tests plus the Redis cancellation case; application all-target Clippy; 791 app unit tests; all 153 isolated GTK widget scenarios; and six PostgreSQL interaction scenarios covering browsing, projection/edit preservation, committed edit/delete, enum values, database switching and transaction outcomes. Documentation links, issue index and ignored-test inventory passed.

The optimization allows up to 50 ms before a newly selected page constructs its grid. This trades a short presentation delay for avoiding work on transient pages. Exact stored values, page limits and operation deadlines remain unchanged. Faster streams can finish before an intermediate page stays visible long enough to build; the final retained pages and pager were verified.

The work has local validation only. It has no Forgejo or merge acceptance. Native Wayland appearance, long-term retention diagnosis, other engines and the memory cost of an open large SQL file remain separate work.

## Ten-cycle retention and idle check

The [longer run](stream-2m-ten-cycles.json) completed ten sequential two-million-row queries in one app process, closing the editor after each query. This decoded twenty million rows in total. No competing build ran. The [summary](ten-cycle-summary.json) records the timing range, CPU and memory sequence.

Post-close RSS rose during warm-up. Cycles six through nine stayed near 251 MiB, but cycle ten reached 256.6 MiB, about 5.6 MiB higher. Growth slowed, but a flat plateau and absence of leaks are not established. The run did not visit every retained page; the separate three-cycle pager check covers that interaction. The connected app used 0.02 CPU seconds over its initial 15-second idle observation.

## Cancellation diagnostic scope

The first cancellation probe incorrectly expected only `Query cancelled`. It completed three such cycles, then observed an explicit unknown outcome and an `unsupported` cleanup warning. [Partial results](cancellation-initial-partial.json) preserve the completed cycles, but their original server check matched only SQL containing `generate_series`; it could miss an active cursor FETCH and must not be treated as proof of server inactivity.

The revised [probe](probe-cancel.py) records confirmed and unknown outcomes separately, as required by ADR 0005. After each interruption it checks that the isolated database has neither active client statements nor idle-in-transaction clients, excluding its observer connection. It then verifies a fresh query value and checks that the old cancellation result does not replace the new one. Its measured duration includes UI activation, accessibility calls and waiting for Run to become available; it is not isolated server abort latency.


The paired records are [baseline cancellation](cancellation-baseline.json) and [candidate cancellation](cancellation-candidate.json). All five cycles on each executable left zero active or idle-in-transaction client connections and successfully returned the next query value. Baseline UI recovery took 24.28–25.29 seconds and reported three unknown outcomes and two confirmed cancellations. Candidate recovery took 0.377–0.389 seconds, with five confirmed cancellations. Unknown outcomes therefore also occur on the unchanged baseline; this experiment does not establish their cleanup cause or remove the requirement to report uncertainty.

## Wide streamed values

The [wide-value run](wide-50k-cycles.json) queried 50000 rows with an 8192-byte text prefix in each row, approximately 410 MB of generated text per query before protocol overhead. Three cycles completed in 1.099, 1.065 and 1.078 seconds. Every cycle reported all 50000 received rows and retained only rows 45001–50000, with an explicit notice that earlier rows were unavailable. This exercises byte-driven eviction rather than the four-page row limit.

Post-close RSS was 339512, 383756 and 383996 KiB. Similar second and third measurements are insufficient to establish a long-term plateau. The byte budget bounds retained result data, not whole-process RSS: transient buffers, constructed widgets and allocator retention also consume memory. This probe did not verify exact clipboard or export contents for these oversized cells.


## Twenty-cycle follow-up

The [twenty-cycle record](stream-2m-twenty-cycles.json) and [summary](twenty-cycle-summary.json) cover forty million decoded rows in one process, closing each editor before the next query. All twenty queries completed in 4.287–4.610 seconds, averaging 4.350 seconds and 3.259 process CPU seconds. No pager navigation or competing compilation occurred in this run.

Post-close RSS increased from 225332 to 260172 KiB, approximately 34 MiB. Private dirty memory increased from 92436 to 127812 KiB. Growth occurred in steps after apparently stable intervals, including another increase after cycle thirteen. The final four observations were similar, but this is not sufficient evidence of a permanent plateau or absence of leaks. The remaining investigation must distinguish live retained allocations from allocator caches, using repeated-cycle allocation profiles and widget lifetime checks. No allocator tuning or forced heap trimming is included in this fix.


## Allocator diagnostic

A separate [ten-cycle diagnostic](stream-2m-allocator-diagnostic.json) ran the same executable and fixture with `MALLOC_ARENA_MAX=1 MALLOC_TRIM_THRESHOLD_=0` in the temporary fixture process environment. No shipped configuration changed. All ten two-million-row queries completed, averaging 4.716 seconds and 3.672 process CPU seconds. Post-close RSS increased from 207852 to 221540 KiB; private dirty memory increased from 73320 to 87008 KiB. This is lower absolute RSS than the ordinary allocator runs, but growth of about 13.4 MiB remains. Both variables changed together and the inherited settings also affect fixture helpers, so this is diagnostic evidence rather than a controlled attribution to one allocator mechanism. No production allocator tuning is proposed. The next investigation needs allocation-lifetime evidence across repeated closed editors.


## Repeated-cycle allocation profile

The [instrumented five-cycle record](stream-250k-heap-five.json) queried 250000 rows per cycle and closed each editor. [Trace identity](heap-five-identity.json) records the compressed trace hash and instrumentation. All five queries completed, but each took about 26 seconds under instrumentation. These are not ordinary performance timings: slowing page delivery can cause more pages to remain mapped past the 50 ms realization delay and construct grids.

The [allocation report](heap-five-allocations.txt) records 171239582 allocation calls, about 83.9 MB peak tracked heap, and about 331.6 MB peak RSS including profiler overhead. Large call counts pass through GTK column construction, shortcut controllers, cell labels and accessibility contexts. The [outstanding-allocation report](heap-five-outstanding.txt) uses the profiler's own leak terminology, but the fixture sends SIGTERM; outstanding allocations are not proof of leaks. A large portion also originates in graphics initialization and unresolved native frames.

The [live-heap summary](heap-five-summary.json), generated by [summarize-heap.py](summarize-heap.py), records low-water observations in five-second windows starting at 35, 70, 105, 140 and 175 seconds. Their minima rise from 47608001 to 54525683 bytes, about 6.6 MiB. Whole-process RSS after each close rises from 302688 to 323788 KiB. Thus live allocations grow in this instrumented workload too; allocator caching alone does not explain it. The selected windows describe the recorded timeline, not synchronization markers proving exact editor-close instants.

One concrete lead is 245 outstanding allocations on a GTK accessibility-context path originating from stream-page labels in `update_live_stream`. This is attribution, not proof of an application reference cycle. The ordinary isolated widget suite sets `GTK_A11Y=test`; this profiling fixture uses real AT-SPI. Follow-up lifetime checks must exercise the real accessibility backend before either blaming application ownership or changing page cleanup. Do not disable accessibility as a fix.

To regenerate the timeline, interpret the raw trace with `/usr/lib/heaptrack/libexec/heaptrack_interpret`, compress with zstd, and run `heaptrack_print -f TRACE -M MASSIF --massif-detailed-freq 0`. Then run `python3 docs/evidence/stream-page-realization-2026-10-23/summarize-heap.py MASSIF`. The compressed interpreted trace remains in the owned target directory; the verified redundant raw trace is removed to recover disk space.
