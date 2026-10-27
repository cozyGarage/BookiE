# Local acceptance and refused-file memory measurements

Recorded on 2026-10-27 for [aggregate #1810](https://github.com/cozyGarage/BookiE/pull/1810) and [issue #1299](https://github.com/cozyGarage/BookiE/issues/1299).

## Acceptance

The CI-profile application passed all 144 scenarios selected by `bash scripts/test-gtk-safety.sh` in private Xvfb and D-Bus sessions. No scenario failed or retried. Synthetic SQLite data and temporary profiles were used. This run does not certify Wayland, package installation, database scenarios requiring optional external fixtures, or the Forgejo merge tier.

`results.json` records source and executable identity. The tested source and aggregate head have identical application, dependency and test files; they differ only in the train manifest and the two gate-security files merged in #1787. `acceptance-results.txt` lists executed scenarios and durations.

## Refused file

The existing `overlong_sql_files_are_refused_without_replacing_the_editor` scenario opened the same 8 MiB single-line comment repeatedly. Every completed cycle verified explicit refusal, unchanged editor text, unchanged tab count and exact file preservation. The profiling-only loop was temporarily changed from three to twenty cycles and then restored. These diagnostics have no memory acceptance threshold.

With the default allocator environment, all twenty cycles passed. RSS rose from 172.42 MiB before the first open to about 304.46 MiB, plateauing near the end. With `MALLOC_ARENA_MAX=2`, thirteen cycles were measured: RSS peaked at 186.21 MiB and later fell to 158.83 MiB. That comparison stopped with a Python accessibility traversal error, `__len__() should return >= 0`, and is not a completed acceptance run.

The standalone Rust control reproduces bounded reading, UTF-8 conversion and line validation on a fresh thread per cycle without GTK. It plateaued at 10.54 MiB RSS from cycle two through twenty. Its source and output are included. This control does not reproduce the entire application.

The allocator comparison supports retention as a hypothesis but does not attribute live allocations or prove absence of a leak. Allocation stacks and a repeatable ownership test remain required before choosing a product fix. No allocator setting was added to the application. Timing is diagnostic and includes file chooser interaction.

## Allocation trace

A separate three-cycle refusal run passed with Heaptrack preloaded only into the application. The instrumentation was removed afterward. The installed CI-profile executable was unchanged. `heaptrack-summary.txt` records 64.56 MB peak tracked heap and 275.79 MB peak RSS including profiler overhead. The largest reported allocation group includes graphics-library frames; other frames are unsymbolized. This trace does not isolate a BookiE ownership defect.

Heaptrack reports 48.38 MB as memory not freed at shutdown. That count includes library caches and must not be interpreted as demonstrated application leaks. A baseline trace and better symbolization are needed for attribution. Raw traces remain local build outputs; the concise analysis is retained here.

## Refreshed-source acceptance follow-up

A later retry-free full run on source `4b0e56e956432e671de6bdb35959261ab5fd1fc1` ended with 15 failed scenarios. Its MCP listener failed to bind the occupied test port; save-before-quit saved the value but timed out waiting for window closure; an AT-SPI traversal error preceded twelve immediate application-startup failures. The latter cascade remains unexplained. This full run is failed, not accepted.

All fifteen failed scenarios subsequently passed in one fresh private Xvfb/D-Bus session with an isolated MCP port and no automatic retries. `isolated-followup-results.txt` records each scenario and duration. The executable was unchanged; this is focused follow-up evidence, not a clean rerun of the whole suite. The listener collision is tracked by #1815, with a request-ownership guard in #1817; the intermittent save/quit symptom is recorded on #1333. Neither the AT-SPI cascade nor the save/quit root cause is established by passing follow-up tests.
