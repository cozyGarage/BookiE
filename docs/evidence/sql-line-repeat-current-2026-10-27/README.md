# Repeated overlong SQL refusal profile, 2026-10-27

Related: [#1299](https://github.com/cozyGarage/BookiE/issues/1299).

## Setup

The source is Linux commit `3ca4a17aef1e50b59b8d71407be47893a7a5caf4`. The optimized executable is `/home/trung/Projects/bookie/target/installed-release/usr/bin/tablepro`, SHA-256 `1eb5b45f377afc0b948fd19e8caa543310b4dd5fe205e71963a64f16f0156f25`. The host toolkits are GTK 4.24.1, libadwaita 1.10.0 and GtkSourceView 5.22.0; Rust is 1.98.1.

The existing `overlong_sql_files_are_refused_without_replacing_the_editor` scenario selected the same 8 MiB, single-line SQL file twenty times in one application process. Each refusal preserved the open editor and tab count, left the input file unchanged, and was followed by a sentinel query. The test's loop was temporarily increased for measurement and restored afterward. Runs used the isolated D-Bus/Xvfb harness with Cairo rendering and AT-SPI.

## Results

The uninstrumented run passed all twenty cycles in 119.2 seconds. RSS rose from 170.6 MiB before the first selection to 354.1 MiB after cycle 17, then remained at 354.1 MiB through cycle 20. Refusal interaction took 0.86–1.45 seconds per cycle. The per-cycle measurements are in [refusal-cycles-20.jsonl](refusal-cycles-20.jsonl).

A second twenty-cycle run with Heaptrack preloaded into the application also passed, in 123.2 seconds. Its RSS reached 415.8 MiB by cycle 17 and stayed there for the final three cycles. This instrumented run is not directly comparable to the uninstrumented run. Its samples are in [refusal-heaptrack-cycles.jsonl](refusal-heaptrack-cycles.jsonl).

Heaptrack reported a 69.27 MiB peak tracked heap and 53.02 MiB live at process exit. The largest group is 25.20 MiB across 19,515 allocations with an unresolved instruction address. The next group is 525 KiB in Mesa Gallium/EGL/GTK graphics-context creation. Heaptrack's live-at-exit category includes reachable library caches and does not prove a leak. The unresolved group cannot currently be attributed to BookiE. The raw trace is a local build artifact and is not committed.

## Scope

These results show a repeatable RSS rise followed by a plateau during repeated refusal; they do not establish which owner retains memory or whether the plateau is acceptable. The Heaptrack trace does not establish an application leak. The measurements do not cover idle CPU, accepted large files, a native Wayland session or package installation. No product code changed. Keep #1299 open for attribution and native-session acceptance.
