# SQL line-boundary profile, 2026-10-26

Related: [#1299](https://github.com/cozyGarage/BookiE/issues/1299).

## Setup

The source is Linux commit `0dc3a2ce882a191d1684eb05ae4726a00c39c812`. The CI-profile application was built with Rust 1.98 using `CARGO_BUILD_JOBS=8`; its SHA-256 is recorded in [binary.sha256](binary.sha256).

The existing `overlong_sql_files_are_refused_without_replacing_the_editor` GTK scenario ran three times in one application process. Each attempt selected an 8 MiB single-line SQL file, verified the refusal, and checked that the current query, tab count and source file were unchanged. The scenario used the repository's private D-Bus and Xvfb harness at 1280×1024 with Cairo rendering and AT-SPI.

## Results

| Attempt | Refusal time | RSS before | RSS after | Process peak RSS |
| --- | ---: | ---: | ---: | ---: |
| 1 | 1.02 s | 173.5 MiB | 176.7 MiB | 183.2 MiB |
| 2 | 0.90 s | 177.2 MiB | 189.1 MiB | 189.1 MiB |
| 3 | 0.85 s | 189.1 MiB | 200.3 MiB | 200.3 MiB |

The scenario passed all three attempts. RSS rose by 26.8 MiB from the initial sample to the final sample. This short run does not establish a leak or a stable plateau; file-chooser and accessibility allocations are included. Raw samples are in [refusal-cycles.jsonl](refusal-cycles.jsonl).

The measurement covers repeated refusal of one overlong file. It does not measure accepted multiline files, long editing sessions, idle CPU, or native Wayland behavior. No product change was made.
