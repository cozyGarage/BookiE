# PostgreSQL stream profile evidence

The original six-cycle profile for [issue 1272](https://github.com/cozyGarage/BookiE/issues/1272) used Linux source commit `5587ba7b537293661a8a23fbe8f5b7617ad83fbf`. The release app queried a pinned PostgreSQL 16 container under Xvfb/Cairo, private D-Bus and AT-SPI. The executable and harness identities are in [build.json](build.json); each cycle is preserved in [the raw profile](attempt-2/profile-uninstrumented.json).

## Results

Each of six cycles received 2,000,000 rows across 400 pages and passed the probe's backward/forward navigation checks. The profile used the retained result pager backed by temporary storage for older pages.

| Measure | Result |
| --- | ---: |
| Query time, min / median / max | 13.065 / 16.955 / 20.517 s |
| Process CPU through completion, min / max | 17.42 / 29.74 s |
| RSS after editor close, cycles 1–6 | 533.2, 551.7, 596.8, 611.9, 598.6, 622.4 MiB |
| After-close RSS change, cycle 1 to 6 | +89.2 MiB |

Another Rust compilation was active during this profile. The timings are loaded-host observations and are not a controlled comparison with the earlier 2026-10-27 profile. The RSS series shows continued growth over these six cycles but does not distinguish live application allocations from allocator or toolkit retention, and does not prove a leak. Keep issue 1272 open for a quiescent-host rerun and current-source allocation attribution.

## Follow-up attempts

The original binary predates Linux `9cad64e64d9c64faaa4da3946c0e1ebdd35a8023`. A release build from that Linux head completed a six-cycle rerun; see [attempt 4](attempt-4/README.md). A Heaptrack-instrumented attempt on the same build exceeded the probe's 180-second query deadline without reaching a terminal UI state; [attempt 3](attempt-3/README.md) records the timeout. That profiler run is not latency evidence and did not complete a profile cycle.

## GTK harness correction

The first profile attempt stopped before launching BookiE because AT-SPI could not open the X display. The test harness now starts Xvfb before its private D-Bus session and supplies an isolated D-Bus service entry for the AT-SPI bus. The rerun completed all six cycles and its scenario result is `passed`. No desktop-wide systemd environment was changed.

The same Xvfb-before-D-Bus ordering is used by [the installed GTK acceptance script](../../../scripts/test-gtk-safety.sh). A contract test checks the wrapper order and the isolated AT-SPI activation entry.

## Reproduction

Build the release GTK test binary, then run the six-cycle profile with `HEAPTRACK=0` and `TABLEPRO_PROFILE_REPETITIONS=6` using [the profile script](../postgres-stream-heap-profile-2026-10-25/run.sh). The script creates and removes its own pinned PostgreSQL container. It needs Docker, Xvfb, AT-SPI, GTK 4.24 or newer, libadwaita 1.10 or newer, and GtkSourceView 5.22 or newer.
